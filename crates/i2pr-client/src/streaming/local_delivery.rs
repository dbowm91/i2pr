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

use crate::bundle::ReplyBundling;
use crate::dispatch::{DestinationDispatcher, InboundDispatchOutcome};
use crate::identity::DestinationIdentity;
use crate::routing::{
    DestinationOutboundRole, DestinationRouting, OutboundDeliveryPlan, OutboundRequest, SendError,
    compose_bundled_reply_delivery, compose_outbound_delivery,
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
    /// The datagram manager rejected the inbound datagram
    /// (malformed, unverifiable, oversize, or queue-full).
    Datagram(crate::datagram::DatagramError),
    /// The supplied inbound tunnel has no first hop (IBGW)
    /// configured. The local seam needs the IBGW's receive tunnel
    /// id to gate the post-OBEP action; without a first hop the
    /// inbound material is invalid.
    InvalidInboundTunnel,
    /// The in-process New Session Reply could not complete the
    /// sender/receiver ECIES pairing used by the loopback product.
    Session(crate::session::EciesSessionError),
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
            Self::Datagram(error) => write!(formatter, "datagram manager: {error}"),
            Self::InvalidInboundTunnel => formatter.write_str("inbound tunnel has no IBGW hop"),
            Self::Session(error) => write!(formatter, "ECIES session pairing: {error}"),
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

impl From<crate::session::EciesSessionError> for LocalDeliveryError {
    fn from(error: crate::session::EciesSessionError) -> Self {
        Self::Session(error)
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
    /// A repliable (17) or raw (18) datagram was authenticated
    /// (where applicable) and queued on the receiver's
    /// [`crate::datagram::DatagramManager`] (Plan 291). Streaming
    /// never sees it.
    DatagramDelivered,
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
    /// The receiver's connectionless datagram manager. Repliable
    /// (17) and raw (18) client payloads authenticate and queue
    /// here; no initiator/mirror split exists because datagrams
    /// carry no connection state (Plan 291).
    pub datagrams: &'a mut crate::datagram::DatagramManager,
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
    let outbound_request = outbound_request_for(request, sender)?;
    let remote_hash =
        DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(request.destination_hash));
    // 1. Compose the outbound delivery plan via the canonical
    //    Plan 129 adapter. The fresh bound NS / NSR / ES form is
    //    selected by the routing pipeline.
    let plan = compose_outbound_delivery(
        sender.routing,
        sender.session,
        sender.outbound,
        sender.identity.id(),
        sender.identity.static_secret_bytes(),
        remote_hash,
        &outbound_request,
        sender.now_seconds,
        sender.now_ms,
        rng,
    )?;
    let outcome = drive_to_dispatch(
        &plan,
        sender,
        receiver,
        outbound_hop0_hash,
        outbound_hop1_hash,
        inbound_tunnel,
        inbound_hop1_hash,
        inbound_hop2_hash,
        _outbound_tunnel_id,
        rng,
    )?;
    drain_single_to_streaming(outcome, sender, receiver)
}

/// Decodes one `TransportSendRequest` into its canonical outbound
/// request: the Plan 192 streaming-envelope unwrap plus the
/// i2pd-compatible I2CP body wrapping exactly one gzip member.
/// Shared by the single and batched delivery paths so both build
/// byte-identical requests.
fn outbound_request_for(
    request: &TransportSendRequest,
    sender: &LocalDeliverySender<'_>,
) -> Result<OutboundRequest, LocalDeliveryError> {
    // Plan 192: the streaming manager already produced an
    // I2P-style gzip-wrapped client payload (with the negotiated
    // local/remote Streaming ports and the protocol byte embedded
    // in the gzip `MTIME` / `XFL` / `OS` fields). i2pd's
    // `StreamingDestination::CreateDataMessage` writes that gzip
    // wrapper verbatim around the raw streaming packet bytes and
    // then patches the same fields with the same values, so the
    // adapter unwraps the streaming-manager gzip once, extracts the
    // negotiated ports + protocol, and feeds the raw packet bytes
    // to `OutboundRequest::new` so the i2pd-compatible I2CP body
    // wraps exactly one gzip member.
    let streaming_envelope = i2pr_proto::streaming::decode_client_payload(
        &request.application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .map_err(|error| LocalDeliveryError::Adapter(StreamingAdapterError::ClientPayload(error)))?;
    OutboundRequest::new(
        streaming_envelope.protocol,
        streaming_envelope.source_port,
        streaming_envelope.destination_port,
        &streaming_envelope.payload,
        sender.now_ms,
        Some(sender.local_lease_set2.clone()),
    )
    .map_err(LocalDeliveryError::from)
}

/// Drives one composed delivery plan through the synthetic OBEP
/// hop, the receiver-side inbound chain, the authenticated
/// dispatcher, and the loopback session pairing (steps 2-4 of
/// [`deliver`]). Shared by the single and batched delivery paths
/// so both traverse byte-identical tunnel and authentication
/// seams; only payload assembly (step 1) and streaming drain
/// (step 5) differ.
#[allow(clippy::too_many_arguments)]
fn drive_to_dispatch<R: CryptoRng + RngCore>(
    plan: &OutboundDeliveryPlan,
    sender: &mut LocalDeliverySender<'_>,
    receiver: &mut LocalDeliveryReceiver<'_>,
    outbound_hop0_hash: i2pr_proto::Hash,
    outbound_hop1_hash: i2pr_proto::Hash,
    inbound_tunnel: EstablishedTunnel,
    inbound_hop1_hash: i2pr_proto::Hash,
    inbound_hop2_hash: i2pr_proto::Hash,
    _outbound_tunnel_id: TunnelId,
    rng: &mut R,
) -> Result<InboundDispatchOutcome, LocalDeliveryError> {
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
            return Ok(outcome);
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

    // A real router sends this New Session Reply back through its
    // transport. The self-composed localhost product has no second
    // transport hop, so complete that authenticated pairing at the
    // local delivery boundary before the next application packet is
    // admitted. The reply is empty because the original packet already
    // carried the application clove; only the session transition is
    // needed here.
    if matches!(outcome, InboundDispatchOutcome::NewSessionProcessed { .. }) {
        let sender_static_public = sender.identity.static_public_bytes();
        let reply = receiver.session.seal_new_session_reply_for(
            receiver.identity.id(),
            receiver.identity.static_secret_bytes(),
            &sender_static_public,
            &[],
            receiver.now_seconds,
            rng,
        )?;
        sender.session.accept_new_session_reply(
            sender.identity.id(),
            sender.identity.static_secret_bytes(),
            &reply.message,
            sender.now_seconds,
        )?;
    }
    Ok(outcome)
}

/// Drains one queued application payload into the receiver's
/// StreamingManager (step 5 of [`deliver`], single path). Dispatch
/// rejections surface as [`LocalDeliveryOutcome::DispatchRejected`]
/// without touching the queue.
fn drain_single_to_streaming(
    outcome: InboundDispatchOutcome,
    sender: &LocalDeliverySender<'_>,
    receiver: &mut LocalDeliveryReceiver<'_>,
) -> Result<LocalDeliveryOutcome, LocalDeliveryError> {
    if matches!(outcome, InboundDispatchOutcome::Rejected(_)) {
        return Ok(LocalDeliveryOutcome::DispatchRejected(outcome));
    }
    let local_destination_hash_bytes: [u8; 32] = *sender.identity.id().as_hash().as_bytes();
    // A single-composed delivery routes at most one payload: pop
    // exactly once so any foreign multi-data message's extra cloves
    // linger for subsequent drains, exactly as before Plan 296.
    let payload = receiver
        .dispatcher
        .pop_payload(receiver.identity.id())
        .ok_or(LocalDeliveryError::NoPayload)?;
    match feed_one_payload(
        payload.bytes().to_vec(),
        sender,
        receiver,
        local_destination_hash_bytes,
    )? {
        FedPayload::Streaming(observation) => Ok(LocalDeliveryOutcome::Delivered { observation }),
        FedPayload::Datagram => Ok(LocalDeliveryOutcome::DatagramDelivered),
    }
}

/// One drained application payload and how the receiver consumed it.
#[derive(Debug)]
enum FedPayload {
    /// Fed into a StreamingManager; the caller checks the
    /// observation (only `StreamingDispatched` counts as delivered).
    Streaming(InboundStreamingOutcome),
    /// Authenticated as a repliable/raw datagram and queued on the
    /// connectionless manager (Plan 291).
    Datagram,
}

/// Drains every queued application payload into the receiver (step 5
/// of [`deliver_batched`]). One entry per popped payload in pop
/// (wire) order; dispatch rejections drain nothing. Feed errors map
/// per payload so one bad payload cannot misattribute its siblings.
fn drain_all_to_streaming(
    outcome: InboundDispatchOutcome,
    sender: &LocalDeliverySender<'_>,
    receiver: &mut LocalDeliveryReceiver<'_>,
) -> Vec<Result<FedPayload, LocalDeliveryError>> {
    if matches!(outcome, InboundDispatchOutcome::Rejected(_)) {
        return Vec::new();
    }
    let local_destination_hash_bytes: [u8; 32] = *sender.identity.id().as_hash().as_bytes();
    let mut fed = Vec::new();
    while let Some(payload) = receiver
        .dispatcher
        .pop_payload(receiver.identity.id())
    {
        fed.push(feed_one_payload(
            payload.bytes().to_vec(),
            sender,
            receiver,
            local_destination_hash_bytes,
        ));
    }
    fed
}

/// Feeds one queued application payload into the receiver's
/// StreamingManager via the standard adapter entry point. Shared by
/// both drains so single and bundled deliveries observe identical
/// streaming/datagram routing.
fn feed_one_payload(
    payload_bytes: Vec<u8>,
    sender: &LocalDeliverySender<'_>,
    receiver: &mut LocalDeliveryReceiver<'_>,
    local_destination_hash_bytes: [u8; 32],
) -> Result<FedPayload, LocalDeliveryError> {
    // Peek the streaming packet header to route the packet to the
    // correct StreamingManager. Plan 144 §3: the streaming manager
    // that issued the outbound SYN owns the outbound connection
    // state (including `outbound_by_stream`). A SYN response must
    // therefore land on the *canonical* outbound manager, not on
    // the receiver-side mirror. Other traffic (inbound SYN, data
    // on the receiver, etc.) stays on the mirror.
    //
    // The dispatcher payload is an i2pd-compatible I2NP envelope
    // (Plan 192 9-byte short-transport form) carrying the i2cp
    // I2CP-style Data body around a gzip-encoded protocol-6 client
    // payload; unwrap both layers before peeking the *streaming*
    // header.
    let peek_for_routing = (|| -> Option<i2pr_proto::streaming::StreamingHeaderPeek> {
        let msg =
            i2pr_proto::I2npMessage::decode_short_transport(&payload_bytes, MAX_I2NP_PAYLOAD_SIZE)
                .ok()?;
        let body = match msg.body() {
            i2pr_proto::I2npBody::Data(body) => body.payload.as_bytes(),
            _ => return None,
        };
        let envelope = i2pr_proto::decode_i2cp_data_body(body).ok()?;
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
            } else if flags_bits & i2pr_proto::streaming::FLAG_SYNCHRONIZE != 0 {
                // Initial SYN (send_stream_id == 0) always lands on
                // the receiver mirror where listeners are bound.
                receiver.streaming
            } else if canonical.lookup_outbound(peek.receive_stream_id).is_some()
                || canonical.lookup_outbound(peek.send_stream_id).is_some()
            {
                // Established data from the peer that originated the
                // connection: the receiver mirror does not own this
                // stream id, the canonical outbound manager does.
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
    // Plan 291: repliable/raw datagrams authenticate and queue on
    // the connectionless manager; everything else keeps the
    // streaming observation path.
    if let InboundStreamingOutcome::DatagramReceived {
        protocol,
        source_port,
        destination_port,
        payload,
    } = observation
    {
        receiver
            .datagrams
            .process_inbound(
                protocol,
                source_port,
                destination_port,
                &payload,
                local_destination_hash_bytes,
                sender.now_ms,
            )
            .map_err(LocalDeliveryError::Datagram)?;
        return Ok(FedPayload::Datagram);
    }
    Ok(FedPayload::Streaming(observation))
}

/// Per-index outcome of one [`deliver_batched`] call: positions into
/// the caller's request slice.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BatchedDeliveryReport {
    /// Input positions delivered (streaming-dispatched or
    /// datagram-fed; dispatch rejections count as delivered, matching
    /// the single-path sweep counters).
    pub delivered: Vec<usize>,
    /// Input positions that failed (decode, compose, drive, feed, or
    /// a non-dispatched streaming observation). The caller
    /// terminates each failed request's stream.
    pub failed: Vec<usize>,
}

/// Outcome of one [`deliver_batched`] call.
#[derive(Debug)]
pub enum BatchedAttempt {
    /// The batch traveled as one bundled New Session Reply; the
    /// report carries per-index delivery positions. A fully failed
    /// report still means "bundled and sent" (the session advanced),
    /// so the caller must NOT retry these requests singly.
    Bundled(BatchedDeliveryReport),
    /// No bundling attempted: the policy is disabled, fewer than two
    /// requests decoded, the remotes are mixed, or the bundled
    /// composer refused (nothing sealed, session untouched). The
    /// caller runs the single path per request; indices that failed
    /// request decode are reported for stream termination.
    Singles {
        /// Input positions whose request failed to decode.
        decode_failed: Vec<usize>,
    },
}

/// Delivers a same-remote batch of `TransportSendRequest`s as one
/// bundled New Session Reply when the destination's reply-bundling
/// policy enables it (Plan 296).
///
/// The caller groups consecutive same-remote requests (the daemon
/// outbound sweep groups its drained runs); mixed-remote input falls
/// back to [`BatchedAttempt::Singles`] so no bundle ever mixes
/// remotes. Bundled compose refusal also falls back (nothing sealed).
/// A sealed-but-undeliverable bundle reports every index failed
/// without retry: re-sending would duplicate application bytes.
#[allow(clippy::too_many_arguments)]
pub fn deliver_batched<R: CryptoRng + RngCore>(
    requests: &[TransportSendRequest],
    bundling: ReplyBundling,
    sender: &mut LocalDeliverySender<'_>,
    receiver: &mut LocalDeliveryReceiver<'_>,
    outbound_hop0_hash: i2pr_proto::Hash,
    outbound_hop1_hash: i2pr_proto::Hash,
    inbound_tunnel: EstablishedTunnel,
    inbound_hop1_hash: i2pr_proto::Hash,
    inbound_hop2_hash: i2pr_proto::Hash,
    _outbound_tunnel_id: TunnelId,
    rng: &mut R,
) -> BatchedAttempt {
    // Decode partition: decode failures terminate without touching
    // the session, so they are reported for per-request cleanup.
    let mut indices = Vec::new();
    let mut outbound_requests = Vec::new();
    let mut decode_failed = Vec::new();
    let mut remote: Option<DestinationHash> = None;
    let mut uniform_remote = true;
    for (index, request) in requests.iter().enumerate() {
        match outbound_request_for(request, sender) {
            Ok(outbound) => {
                let hash = DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(
                    request.destination_hash,
                ));
                match remote {
                    None => remote = Some(hash),
                    Some(first) if first == hash => {}
                    _ => uniform_remote = false,
                }
                indices.push(index);
                outbound_requests.push(outbound);
            }
            Err(_) => decode_failed.push(index),
        }
    }
    if uniform_remote && outbound_requests.len() >= 2 && bundling.is_enabled() {
        if let Some(remote_hash) = remote
            && let Ok(plan) = compose_bundled_reply_delivery(
                sender.routing,
                sender.session,
                sender.outbound,
                sender.identity.id(),
                sender.identity.static_secret_bytes(),
                remote_hash,
                &outbound_requests,
                bundling,
                sender.now_seconds,
                sender.now_ms,
                rng,
            )
        {
            let mut report = BatchedDeliveryReport::default();
            match drive_to_dispatch(
                &plan,
                sender,
                receiver,
                outbound_hop0_hash,
                outbound_hop1_hash,
                inbound_tunnel,
                inbound_hop1_hash,
                inbound_hop2_hash,
                _outbound_tunnel_id,
                rng,
            ) {
                Ok(outcome) => {
                    let drained = drain_all_to_streaming(outcome, sender, receiver);
                    if drained.is_empty() {
                        // Dispatch rejected the carrier: parity with
                        // the single-path sweep counters, which count
                        // rejections as delivered.
                        report.delivered.extend(indices.iter().copied());
                    } else {
                        for (position, index) in indices.iter().enumerate() {
                            match drained.get(position) {
                                Some(Ok(FedPayload::Streaming(
                                    InboundStreamingOutcome::StreamingDispatched { .. },
                                )))
                                | Some(Ok(FedPayload::Datagram)) => {
                                    report.delivered.push(*index);
                                }
                                _ => report.failed.push(*index),
                            }
                        }
                    }
                }
                Err(_) => {
                    report.failed.extend(indices.iter().copied());
                }
            }
            return BatchedAttempt::Bundled(report);
        }
    }
    BatchedAttempt::Singles { decode_failed }
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
