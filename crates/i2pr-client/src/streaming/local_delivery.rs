//! Plan 143 runtime-neutral local destination delivery pump.
//!
//! This module is the single, reusable,
//! **authenticated-router-link-bypassed-local-seam** the SAM 3.1
//! STREAM product bridge (Plan 143) and the `i2pr-daemon` SAM
//! service use to drive a `TransportSendRequest` through the full
//! local destination stack to the receiver's [`StreamingManager`]
//! without leaving the process.
//!
//! ```text
//! TransportSendRequest
//!  -> StreamingDestinationAdapter::send
//!       -> canonical ECIES Garlic envelope (Plan 122/127)
//!  -> sender outbound tunnel data plane (Plan 116 OBEP)
//!  -> Plan 129 authenticated-router-link-bypassed-local-seam
//!  -> receiver inbound chain (IBGW -> participant -> endpoint)
//!  -> DestinationDispatcher
//!  -> StreamingDestinationAdapter::receive
//!  -> receiver StreamingManager
//! ```
//!
//! The pump is **not** a substitute for the production router
//! delivery layer. It exists so Plan 143 and the SAM STREAM
//! production path can exercise the same destination/garlic/
//! LS2/Streaming stack the broader router uses, without needing
//! live NTCP2/SSU2 transport. Independent-router interoperability
//! remains external acceptance debt; the seam does not advance
//! that claim.
//!
//! ## Concurrency
//!
//! The pump is stateless. Every call takes a [`LocalDeliverySender`]
//! plus a [`LocalDeliveryReceiver`] and produces a single inbound
//! streaming observation. The receiver-side reassembler and
//! producer-side outbound roles are rebuilt per call so no state
//! leaks between successive deliveries.

#![forbid(unsafe_code)]

use i2pr_netdb::{DestinationHash, LeaseSet2Store};
use i2pr_proto::{CodecError, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelGatewayMessage};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedTunnel, InboundGatewayRole, InboundParticipantRole,
    LocalInboundEndpointRole, OutboundEndpointRole, OutboundParticipantRole, RouterDeliveryAction,
    TunnelId, TunnelRoleError,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};

use crate::dispatch::{DestinationDispatcher, InboundDispatchOutcome};
use crate::identity::DestinationIdentity;
use crate::routing::{
    DestinationOutboundRole, DestinationRouting, OutboundRequest, SendError,
    compose_outbound_delivery,
};
use crate::session::EciesSessionManager;
use crate::streaming::manager::StreamingManager;
use crate::streaming::transport::TransportSendRequest;
use crate::streaming_adapter::{
    InboundStreamingOutcome, MAX_STREAMING_ADAPTER_PAYLOAD_BYTES, StreamingAdapterError,
    StreamingDestinationAdapter,
};

/// Plan 116 sets the canonical OBEP reassembly/duplicate-window sizes.
const REASSEMBLER_CAPACITY: usize = 16;
const REASSEMBLER_AGGREGATE_BYTES: usize = 1 << 20;
const REASSEMBLER_EXPIRY_MS: u64 = 60_000;
/// Plan 129 uses `start_ms + 120_000` for role expiry.
const ROLE_EXPIRY_OFFSET_MS: u64 = 120_000;
/// In-process fragment permutation is collision-free for the
/// single-delivery local seam.
const IBGW_CELL_RNG_SEED: u64 = 0x05EA_11B5;

/// Typed errors surfaced by the local delivery pump.
#[derive(Debug)]
pub enum LocalDeliveryError {
    /// The Plan 122 outbound composer rejected the request.
    Send(SendError),
    /// The sender-side outbound tunnel data plane reported a
    /// typed failure.
    Tunnel(TunnelRoleError),
    /// The Plan 129 OBEP path produced no post-OBEP action
    /// (typically because every outbound cell was a duplicate).
    NoObepAction,
    /// The receiver-side dispatcher has no queued application
    /// payload for the supplied destination.
    NoPayload,
    /// Inbound reconstruction failed before dispatcher intake.
    Reconstruct(ReconstructError),
    /// The streaming adapter rejected the inbound packet.
    Adapter(StreamingAdapterError),
    /// The supplied inbound tunnel has no first hop (IBGW)
    /// configured. The local seam needs the IBGW's receive tunnel
    /// id to gate the post-OBEP action; without a first hop the
    /// inbound material is invalid.
    InvalidInboundTunnel,
}

impl std::fmt::Display for LocalDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Send(error) => write!(formatter, "send composer: {error}"),
            Self::Tunnel(error) => write!(formatter, "tunnel data plane: {error}"),
            Self::NoObepAction => formatter.write_str("synthetic OBEP path produced no action"),
            Self::NoPayload => formatter.write_str("no queued application payload"),
            Self::Reconstruct(error) => write!(formatter, "reconstruct: {error}"),
            Self::Adapter(error) => write!(formatter, "streaming adapter: {error}"),
            Self::InvalidInboundTunnel => formatter.write_str("inbound tunnel has no IBGW hop"),
        }
    }
}

impl std::error::Error for LocalDeliveryError {}

impl From<SendError> for LocalDeliveryError {
    fn from(error: SendError) -> Self {
        Self::Send(error)
    }
}

impl From<TunnelRoleError> for LocalDeliveryError {
    fn from(error: TunnelRoleError) -> Self {
        Self::Tunnel(error)
    }
}

impl From<ReconstructError> for LocalDeliveryError {
    fn from(error: ReconstructError) -> Self {
        Self::Reconstruct(error)
    }
}

impl From<StreamingAdapterError> for LocalDeliveryError {
    fn from(error: StreamingAdapterError) -> Self {
        Self::Adapter(error)
    }
}

/// Inbound reconstruction failures surfaced by the local seam.
#[derive(Debug)]
pub enum ReconstructError {
    /// The sender OBEP action's tunnel id did not match the
    /// receiver's local receive tunnel id.
    TunnelIdMismatch,
    /// The receiver endpoint reassembler did not surface a
    /// carrier; either every inbound cell was a duplicate or
    /// the fragmenter produced an empty cell stream.
    NoReassembledMessage,
    /// The recovered envelope was not a Garlic carrier.
    NotGarlic,
    /// The standard I2NP decoder rejected the carrier bytes.
    Codec(CodecError),
}

impl std::fmt::Display for ReconstructError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TunnelIdMismatch => formatter.write_str("tunnel id mismatch on local seam"),
            Self::NoReassembledMessage => formatter.write_str("no reassembled carrier"),
            Self::NotGarlic => formatter.write_str("recovered envelope is not a Garlic body"),
            Self::Codec(error) => write!(formatter, "i2np codec: {error}"),
        }
    }
}

impl std::error::Error for ReconstructError {}

/// Outcome of one [`deliver`] call. Plan 143 threads the
/// per-stream driver through this surface so every application
/// byte crosses the same Plan 129 destination stack in both
/// directions.
#[derive(Debug)]
pub enum LocalDeliveryOutcome {
    /// The packet was delivered to the receiver's StreamingManager
    /// via the standard adapter path.
    Delivered {
        /// Inbound adapter observation (Plan 129 §3).
        observation: InboundStreamingOutcome,
    },
    /// The dispatcher rejected the carrier envelope.
    DispatchRejected(InboundDispatchOutcome),
}

/// Sender-side bridge state with owned outbound tunnel role and
/// immutable local-destination inputs the seam needs for every
/// delivery.
pub struct LocalDeliverySender<'a> {
    /// The sender's local destination identity.
    pub identity: &'a DestinationIdentity,
    /// The sender's destination routing pipeline.
    pub routing: &'a mut DestinationRouting,
    /// The sender's ECIES session manager.
    pub session: &'a mut EciesSessionManager,
    /// The sender's outbound tunnel role.
    pub outbound: &'a DestinationOutboundRole,
    /// The sender's local signed LeaseSet2.
    pub local_lease_set2: &'a i2pr_proto::LeaseSet2,
    /// Current wall-clock seconds for ECIES classification.
    pub now_seconds: u32,
    /// Current monotonic milliseconds for streaming/role timing.
    pub now_ms: u64,
}

/// Receiver-side bridge state with owned dispatcher, ECIES
/// session, routing, StreamingManager, and lease-set store.
pub struct LocalDeliveryReceiver<'a> {
    /// The receiver's local destination identity.
    pub identity: &'a DestinationIdentity,
    /// The receiver's authenticated dispatcher.
    pub dispatcher: &'a mut DestinationDispatcher,
    /// The receiver's ECIES session manager.
    pub session: &'a mut EciesSessionManager,
    /// The receiver's routing pipeline (mutable so install of
    /// the validated remote LeaseSet2 is recorded).
    pub routing: &'a mut DestinationRouting,
    /// The receiver's StreamingManager. The Plan 129 mirror
    /// manager that handles inbound SYN observations and data
    /// traffic for established receiver-side streams.
    pub streaming: &'a mut StreamingManager,
    /// Optional receiver-side canonical outbound StreamingManager
    /// that owns the outbound SYN trackers (Plan 129 §3, Plan 144
    /// §3: the SYN response must reach the *same* StreamingManager
    /// that issued the SYN, since the outbound connection state
    /// — including `outbound_by_stream` — lives there). When `Some`
    /// the delivery path peeks the streaming packet header; a SYN
    /// response is dispatched here, all other streaming traffic
    /// dispatches to `streaming`.
    pub canonical_streaming: Option<&'a mut StreamingManager>,
    /// Receiver-side lease set cache (mutable so validated
    /// senders can be inserted).
    pub lease_set2_store: &'a mut LeaseSet2Store,
    /// The receiver's "now" seconds for ECIES classification.
    pub now_seconds: u32,
}

/// One full local delivery for a single
/// `TransportSendRequest`. Every call recreates the synthetic
/// outbound roles (OBEP -> participant) and the receiver-side
/// reassembler, so no state leaks between calls.
#[allow(clippy::too_many_arguments)]
pub fn deliver<R: CryptoRng + RngCore>(
    request: &TransportSendRequest,
    sender: &mut LocalDeliverySender<'_>,
    receiver: &mut LocalDeliveryReceiver<'_>,
    outbound_hop0_hash: i2pr_proto::Hash,
    outbound_hop1_hash: i2pr_proto::Hash,
    inbound_tunnel: EstablishedTunnel,
    inbound_hop1_hash: i2pr_proto::Hash,
    inbound_hop2_hash: i2pr_proto::Hash,
    _outbound_tunnel_id: TunnelId,
    rng: &mut R,
) -> Result<LocalDeliveryOutcome, LocalDeliveryError> {
    let local_destination_hash_bytes: [u8; 32] = *sender.identity.id().as_hash().as_bytes();
    // The action's tunnel_id is the inbound gateway's receive
    // tunnel id at the gateway router — the Lease2 `tunnel_id` the
    // outbound delivery plan selects. The local seam uses the IBGW
    // hop's tunnel id (the inbound tunnel's first hop) as the
    // gating value, not the local_inbound_receive endpoint id the
    // tunnel reassembler expects at the very end.
    let inbound_ibgw_tunnel_id = inbound_tunnel
        .hops()
        .first()
        .map(|hop| hop.receive_tunnel())
        .ok_or(LocalDeliveryError::InvalidInboundTunnel)?;
    let local_static_secret: [u8; i2pr_crypto::X25519_KEY_LENGTH] =
        *sender.identity.static_secret_bytes();
    let remote_hash =
        DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(request.destination_hash));

    // 1. Compose the outbound delivery plan via the canonical
    //    Plan 129 adapter. The fresh bound NS / NSR / ES form is
    //    selected by the routing pipeline.
    let outbound_request = OutboundRequest::new(
        i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
        &request.application_payload,
        sender.now_ms,
        Some(sender.local_lease_set2.clone()),
    )?;
    let plan = compose_outbound_delivery(
        sender.routing,
        sender.session,
        sender.outbound,
        sender.identity.id(),
        &local_static_secret,
        remote_hash,
        &outbound_request,
        sender.now_seconds,
        sender.now_ms,
        rng,
    )?;

    // 2. Drive the synthetic OBEP hop to recover the post-OBEP
    //    action (authenticated-router-link-bypassed local seam).
    let action = synthesise_obep_action(
        &plan,
        sender.outbound,
        outbound_hop0_hash,
        outbound_hop1_hash,
        sender.now_ms,
    )?;

    // 3. Feed the action through the receiver-side inbound chain
    //    (IBGW -> participant -> endpoint) and recover the inner
    //    I2NP message bytes.
    let recovered_message = feed_inbound_chain(
        &action,
        inbound_tunnel,
        inbound_ibgw_tunnel_id,
        inbound_hop1_hash,
        inbound_hop2_hash,
        sender.now_ms,
    )?;

    // 4. Dispatch the Garlic envelope through the receiver's
    //    authenticated dispatcher. The Plan 127 binding order is
    //    the single integration point — no plaintext Streaming
    //    bytes cross any seam.
    let outcome = receiver.dispatcher.dispatch_garlic_envelope(
        receiver.session,
        receiver.identity.id(),
        receiver.identity.static_secret_bytes(),
        &receiver.identity.static_public_bytes(),
        receiver.now_seconds,
        &recovered_message,
        receiver.lease_set2_store,
    );
    match &outcome {
        InboundDispatchOutcome::Rejected(_) => {
            return Ok(LocalDeliveryOutcome::DispatchRejected(outcome));
        }
        InboundDispatchOutcome::NewSessionProcessed {
            validated_remote_lease_set2,
            ..
        } => {
            let _ = receiver
                .routing
                .install_remote_lease_set2(*validated_remote_lease_set2.clone());
        }
        InboundDispatchOutcome::ExistingSessionProcessed { .. }
        | InboundDispatchOutcome::NewSessionReplyProcessed { .. } => {}
    }

    // 5. Drain the queued application payload and feed it into the
    //    receiver's StreamingManager via the standard adapter
    //    entry point.
    let payload = receiver
        .dispatcher
        .pop_payload(receiver.identity.id())
        .ok_or(LocalDeliveryError::NoPayload)?;
    let payload_bytes = payload.bytes().to_vec();
    // Peek the streaming packet header to route the packet to the
    // correct StreamingManager. Plan 144 §3: the streaming manager
    // that issued the outbound SYN owns the outbound connection
    // state (including `outbound_by_stream`). A SYN response must
    // therefore land on the *canonical* outbound manager, not on
    // the receiver-side mirror. Other traffic (inbound SYN, data
    // on the receiver, etc.) stays on the mirror.
    //
    // The dispatcher payload is an I2NP envelope carrying a
    // gzip-encoded protocol-6 client payload; unwrap both layers
    // before peeking the *streaming* header.
    let peek_for_routing = (|| -> Option<i2pr_proto::streaming::StreamingHeaderPeek> {
        let msg =
            i2pr_proto::I2npMessage::decode_standard(&payload_bytes, MAX_I2NP_PAYLOAD_SIZE).ok()?;
        let body = match msg.body() {
            i2pr_proto::I2npBody::Data(body) => body.payload.as_bytes(),
            _ => return None,
        };
        let envelope =
            i2pr_proto::streaming::decode_client_payload(body, MAX_STREAMING_ADAPTER_PAYLOAD_BYTES)
                .ok()?;
        i2pr_proto::streaming::peek_streaming_header(&envelope.payload).ok()
    })();
    let target_streaming: &mut StreamingManager = match (
        receiver.canonical_streaming.as_deref_mut(),
        &peek_for_routing,
    ) {
        (Some(canonical), Some(peek)) => {
            let flags_bits = peek.flags_bits & !i2pr_proto::streaming::FLAG_RESERVED_MASK;
            let is_syn_response = flags_bits & i2pr_proto::streaming::FLAG_SYNCHRONIZE != 0
                && peek.send_stream_id != 0
                && peek.receive_stream_id != 0;
            if is_syn_response {
                canonical
            } else {
                receiver.streaming
            }
        }
        _ => receiver.streaming,
    };
    let observation = StreamingDestinationAdapter::receive(
        &payload_bytes,
        receiver.identity,
        target_streaming,
        &local_destination_hash_bytes,
        sender.now_ms,
    )?;
    let _ = outcome;
    Ok(LocalDeliveryOutcome::Delivered { observation })
}

/// Recovers the post-OBEP router-delivery action from a composed
/// Plan 129 outbound delivery plan. The seam uses the established
/// outbound tunnel role's hops and re-creates the synthetic
/// participant + OBEP roles for the supplied action.
fn synthesise_obep_action(
    plan: &crate::routing::OutboundDeliveryPlan,
    outbound_role: &DestinationOutboundRole,
    hop0_hash: i2pr_proto::Hash,
    hop1_hash: i2pr_proto::Hash,
    now_ms: u64,
) -> Result<RouterDeliveryAction, LocalDeliveryError> {
    let _ = hop1_hash;
    let outbound_role_established = outbound_role.role().established();
    let outbound_hops: Vec<_> = outbound_role_established.hops().to_vec();
    let expires_at = now_ms.saturating_add(ROLE_EXPIRY_OFFSET_MS);
    let mut participant =
        OutboundParticipantRole::new(&outbound_hops[0], DuplicateWindow::new(16), expires_at)?;
    let mut obep = OutboundEndpointRole::new(
        &outbound_hops[1],
        DuplicateWindow::new(16),
        REASSEMBLER_CAPACITY,
        REASSEMBLER_AGGREGATE_BYTES,
        REASSEMBLER_EXPIRY_MS,
        expires_at,
        0,
    );
    let mut action: Option<RouterDeliveryAction> = None;
    for cell in &plan.cells {
        let forwarded = participant.process(&hop0_hash, &cell.cell, now_ms)?;
        let delivered = obep.process(&hop0_hash, &forwarded, now_ms)?;
        if let Some(action_value) = delivered {
            assert!(action.is_none(), "exactly one post-OBEP action per plan");
            action = Some(action_value);
        }
    }
    action.ok_or(LocalDeliveryError::NoObepAction)
}

/// Drives the post-OBEP action through the receiver-side inbound
/// chain (IBGW -> participant -> local endpoint) and returns the
/// reconstructed standard I2NP message.
fn feed_inbound_chain(
    action: &RouterDeliveryAction,
    inbound_tunnel: EstablishedTunnel,
    inbound_tunnel_id: TunnelId,
    hop1_hash: i2pr_proto::Hash,
    hop2_hash: i2pr_proto::Hash,
    now_ms: u64,
) -> Result<I2npMessage, LocalDeliveryError> {
    let inner_i2np = I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE)
        .map_err(ReconstructError::Codec)?;
    let tunnel_id = action.tunnel_id.ok_or(ReconstructError::TunnelIdMismatch)?;
    if tunnel_id.get() != inbound_tunnel_id.get() {
        return Err(ReconstructError::TunnelIdMismatch.into());
    }
    let gateway = TunnelGatewayMessage {
        tunnel_id: tunnel_id.get(),
        message: Box::new(inner_i2np),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(IBGW_CELL_RNG_SEED);
    let ibgw = InboundGatewayRole::new(
        &inbound_tunnel.hops()[0],
        DuplicateWindow::new(16),
        now_ms.saturating_add(ROLE_EXPIRY_OFFSET_MS),
    )?;
    let cells = ibgw.process_cells(&gateway, &mut rng, now_ms)?;
    let mut participant = InboundParticipantRole::new(
        &inbound_tunnel.hops()[1],
        DuplicateWindow::new(16),
        now_ms.saturating_add(ROLE_EXPIRY_OFFSET_MS),
    )?;
    let mut endpoint = LocalInboundEndpointRole::new(
        inbound_tunnel,
        REASSEMBLER_CAPACITY,
        REASSEMBLER_AGGREGATE_BYTES,
        REASSEMBLER_EXPIRY_MS,
        now_ms,
        now_ms.saturating_add(ROLE_EXPIRY_OFFSET_MS),
    );
    let mut recovered: Option<Vec<u8>> = None;
    for cell in &cells {
        let tunnel_data = cell.cell.clone();
        let forwarded = participant.process(&hop1_hash, &tunnel_data, now_ms)?;
        if let Some(message) = endpoint.process(&hop2_hash, &forwarded, now_ms)? {
            recovered = Some(message);
        }
    }
    let bytes = recovered.ok_or(ReconstructError::NoReassembledMessage)?;
    let message = I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE)
        .map_err(ReconstructError::Codec)?;
    if !matches!(message.body(), I2npBody::Garlic(_)) {
        return Err(ReconstructError::NotGarlic.into());
    }
    Ok(message)
}
