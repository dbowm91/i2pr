//! Runtime-neutral tunnel roles for the local data plane.
//!
//! Plan 116 §17-§24 own the in-process role composition that drives
//! a deterministic outbound tunnel chain and a deterministic
//! outbound-to-inbound exploratory pair. The roles emit semantic
//! router-delivery actions; a later runtime/composition layer
//! adapts those actions into the existing transport boundary.
//!
//! The data plane handles the cell-level layer transform
//! independent of the I2NP `TunnelDataMessage` framing. The
//! `TunnelDataMessage` carries `(tunnel_id, data[1024])` where
//! `data[0..16]` is the IV and `data[16..1024]` is the encrypted
//! 1008-byte payload. The role layer extracts the IV and
//! ciphertext, applies the participant transform, and re-emits
//! the next `TunnelDataMessage` addressed to the next hop.
//!
//! The module keeps each role state minimal. Forwarding roles own
//! CSPRNG injection through a generic `R: CryptoRng + RngCore`
//! parameter; no deterministic-zero RNG ever appears in the
//! production code path.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    unused_imports,
    clippy::manual_range_contains,
    clippy::type_complexity,
    clippy::needless_borrow,
    missing_docs
)]

use std::fmt;

use i2pr_proto::{Hash, I2npMessage, TunnelDataMessage, TunnelGatewayMessage};
use rand_core::{CryptoRng, RngCore, TryRngCore};
use thiserror::Error;
use zeroize::Zeroize;

use crate::build_crypto::LayerKeys;
use crate::data::{
    DeliveryInstruction, FragmentDelivery, TunnelMessageBuilder, TunnelMessageError,
    TunnelMessageParser, TunnelPayloadHeader,
};
use crate::established::{
    EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    zero_id, zero_peer,
};
use crate::fragment::{BoundedReassembler, ReassemblyError, ReassemblyKey, TunnelFragment};
use crate::identity::{TunnelDirection, TunnelId, TunnelPeer};
use crate::layer::{
    DuplicateToken, DuplicateWindow, DuplicateWindowError, TUNNEL_IV_LEN, TUNNEL_PAYLOAD_LEN,
    TunnelLayerTransform,
};

/// Semantic router-delivery kind the data plane emits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterDeliveryKind {
    /// Send the reconstructed standard I2NP message directly to the
    /// target router.
    Router,
    /// Wrap the reconstructed standard I2NP message in a
    /// `TunnelGateway` and deliver to the target inbound tunnel
    /// gateway.
    TunnelGateway,
    /// Deliver the reconstructed standard I2NP message locally to
    /// the local router-facing boundary.
    Local,
}

/// Semantic router-delivery action the data plane emits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterDeliveryAction {
    /// Target router hash.
    pub target_router: Hash,
    /// Delivery kind.
    pub kind: RouterDeliveryKind,
    /// Inbound gateway receive tunnel id when `kind ==
    /// TunnelGateway`; ignored otherwise.
    pub tunnel_id: Option<TunnelId>,
    /// Reconstructed standard I2NP message bytes.
    pub message: Vec<u8>,
    /// Original message identifier when the data plane retained it
    /// from the first-fragment header; `0` when the message was
    /// unfragmented or reassembled without a fragment message id.
    pub message_id: u32,
    /// Original expiration timestamp in milliseconds since the
    /// Unix epoch when the first-fragment header carried one; `0`
    /// otherwise.
    pub expiration_ms: u64,
}

impl fmt::Display for RouterDeliveryAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            RouterDeliveryKind::Router => {
                write!(formatter, "router/{:08x}", self.message_id)
            }
            RouterDeliveryKind::TunnelGateway => {
                if let Some(tunnel_id) = self.tunnel_id {
                    write!(formatter, "tunnel-gw/{}/{:08x}", tunnel_id, self.message_id)
                } else {
                    write!(formatter, "tunnel-gw/<none>/{:08x}", self.message_id)
                }
            }
            RouterDeliveryKind::Local => write!(formatter, "local/{:08x}", self.message_id),
        }
    }
}

/// Failure categories for the runtime-neutral role state.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TunnelRoleError {
    /// The role was given an expired or removed established
    /// tunnel.
    #[error("tunnel is no longer usable")]
    TunnelUnavailable,
    /// The role received a byte-exact replay of a cell its
    /// duplicate window already admitted (Plan 130 §9: the window
    /// is an enforcement point, not a passive counter).
    #[error("duplicate tunnel data cell rejected by the replay window")]
    DuplicateCell,
    /// The role received a tunnel-data cell addressed to a
    /// receive id that does not match the role's configured
    /// tunnel.
    #[error("tunnel data cell receive id {actual} does not match expected {expected}")]
    ReceiveTunnelMismatch {
        /// Actual receive id from the cell.
        actual: TunnelId,
        /// Expected receive id for this role.
        expected: TunnelId,
    },
    /// The role received a tunnel-data cell from a previous peer
    /// that does not match the locked previous peer.
    #[error("previous peer identity does not match the locked peer")]
    PreviousPeerMismatch,
    /// The role's duplicate window rejected the supplied cell.
    #[error("duplicate window rejected the cell: {0}")]
    DuplicateWindow(#[from] DuplicateWindowError),
    /// The role's reassembler rejected the supplied fragment.
    #[error("reassembler rejected the fragment: {0}")]
    Reassembly(#[from] ReassemblyError),
    /// The role's builder/parser rejected a payload.
    #[error("tunnel message rejected: {0}")]
    TunnelMessage(#[from] TunnelMessageError),
    /// The role received an inbound message that was not
    /// `TunnelGateway`.
    #[error("inbound message kind is not TunnelGateway")]
    NotTunnelGateway,
    /// The role received a tunnel data message with a zero tunnel
    /// id.
    #[error("tunnel data message has zero tunnel id")]
    ZeroTunnelId,
    /// The role's TunnelGateway target tunnel id does not match
    /// the role's configured inbound tunnel.
    #[error("TunnelGateway target tunnel id {actual} does not match expected {expected}")]
    GatewayTunnelMismatch {
        /// Actual target tunnel id.
        actual: TunnelId,
        /// Expected receive tunnel id.
        expected: TunnelId,
    },
    /// An unsupported delivery instruction was used (e.g.
    /// `Tunnel` from an outbound gateway).
    #[error("unsupported delivery instruction")]
    UnsupportedDeliveryInstruction,
    /// A local inbound endpoint received a non-LOCAL delivery
    /// instruction.
    #[error("local inbound endpoint received non-LOCAL delivery instruction")]
    LocalInboundNonLocalDelivery,
    /// An established tunnel hop that should have carried a
    /// `next` field did not.
    #[error("established hop is missing required next-hop state")]
    MissingNextHop,
    /// A reassembled fragmented message reached completion
    /// without a retained first-fragment delivery instruction.
    /// The data plane refuses to fabricate an unspecified
    /// delivery action; the failure is reported to the caller.
    #[error("reassembled message id {message_id} had no retained delivery instruction")]
    UnspecifiedDeliveryInstruction {
        /// The completed message identifier.
        message_id: u32,
    },
}

/// Extracts the IV and the encrypted 1008-byte payload from the
/// `TunnelDataMessage.data` field.
fn split_cell(cell: &TunnelDataMessage) -> ([u8; TUNNEL_IV_LEN], [u8; TUNNEL_PAYLOAD_LEN]) {
    let mut iv = [0_u8; TUNNEL_IV_LEN];
    let mut payload = [0_u8; TUNNEL_PAYLOAD_LEN];
    iv.copy_from_slice(&cell.data[..TUNNEL_IV_LEN]);
    payload.copy_from_slice(&cell.data[TUNNEL_IV_LEN..]);
    (iv, payload)
}

/// Reassembles the `(iv, payload)` pair into the
/// `TunnelDataMessage.data` field.
fn join_cell(
    tunnel_id: u32,
    iv: [u8; TUNNEL_IV_LEN],
    payload: [u8; TUNNEL_PAYLOAD_LEN],
) -> TunnelDataMessage {
    let mut data = [0_u8; 1024];
    data[..TUNNEL_IV_LEN].copy_from_slice(&iv);
    data[TUNNEL_IV_LEN..].copy_from_slice(&payload);
    TunnelDataMessage { tunnel_id, data }
}

/// Picks a fresh 16-byte IV using the supplied CSPRNG.
fn fresh_iv<R: CryptoRng + RngCore>(
    rng: &mut R,
) -> Result<[u8; TUNNEL_IV_LEN], TunnelMessageError> {
    let mut iv = [0_u8; TUNNEL_IV_LEN];
    rng.try_fill_bytes(&mut iv)
        .map_err(|_| TunnelMessageError::RandomnessUnavailable)?;
    Ok(iv)
}

/// Outbound gateway role. The owner is the local creator that
/// dispatches messages through the established outbound tunnel.
#[derive(Debug)]
pub struct OutboundGatewayRole {
    established: EstablishedTunnel,
    expires_at_ms: u64,
}

impl OutboundGatewayRole {
    /// Constructs a new outbound gateway role from an established
    /// outbound tunnel.
    pub const fn new(established: EstablishedTunnel, expires_at_ms: u64) -> Self {
        Self {
            established,
            expires_at_ms,
        }
    }

    /// Returns the established outbound tunnel.
    pub const fn established(&self) -> &EstablishedTunnel {
        &self.established
    }

    /// Returns whether the role is usable at the supplied time.
    pub fn is_usable(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms && self.established.direction() == TunnelDirection::Outbound
    }

    /// Fragments a complete standard I2NP message into the ordered
    /// `(iv, payload)` TunnelData cells.
    pub fn fragment<R: CryptoRng + RngCore>(
        &self,
        header: &TunnelPayloadHeader,
        complete_message: &[u8],
        rng: &mut R,
    ) -> Result<Vec<([u8; TUNNEL_IV_LEN], [u8; TUNNEL_PAYLOAD_LEN])>, TunnelRoleError> {
        if !self.is_usable(0) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if matches!(header.delivery, DeliveryInstruction::Local) {
            return Err(TunnelRoleError::UnsupportedDeliveryInstruction);
        }
        let fragments = TunnelMessageBuilder::fragment_complete_message(
            &header.delivery,
            header.message_id,
            complete_message,
        )?;
        TunnelMessageBuilder::new()
            .build_cells(&fragments, rng)
            .map_err(TunnelRoleError::TunnelMessage)
    }

    /// Forwards one standard I2NP message through the outbound
    /// tunnel. The function returns the preprocessed
    /// `TunnelDataMessage` addressed to the first hop. The
    /// payload must fit a single TunnelData cell.
    pub fn forward<R: CryptoRng + RngCore>(
        &self,
        header: &TunnelPayloadHeader,
        complete_message: &[u8],
        rng: &mut R,
        now_ms: u64,
    ) -> Result<OBGWRouterDelivery, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if matches!(header.delivery, DeliveryInstruction::Local) {
            return Err(TunnelRoleError::UnsupportedDeliveryInstruction);
        }
        let iv = fresh_iv(rng)?;
        let plaintext = TunnelMessageBuilder::new()
            .build_single(header, complete_message, iv, rng)
            .map_err(TunnelRoleError::TunnelMessage)?;
        let hops_reverse: Vec<LayerKeys> = self
            .established
            .hops()
            .iter()
            .map(|hop| hop.layer_keys().clone())
            .rev()
            .collect();
        let hops_ref: Vec<&LayerKeys> = hops_reverse.iter().collect();
        let (cell_iv, cell_data) =
            TunnelLayerTransform::outbound_preprocess(&hops_ref, iv, plaintext);
        let first_hop = self.established.first_hop_router();
        let receive_tunnel = self.established.first_hop_receive_tunnel();
        let cell = join_cell(receive_tunnel.get(), cell_iv, cell_data);
        Ok(OBGWRouterDelivery {
            target_router: first_hop,
            receive_tunnel,
            cell,
        })
    }

    /// Forwards a complete standard I2NP message through the
    /// outbound tunnel, fragmenting across multiple TunnelData
    /// cells when necessary. Returns the preprocessed
    /// `TunnelDataMessage` cells addressed to the first hop, in
    /// canonical fragment order.
    ///
    /// The Plan 116 fragmented cross-tunnel trajectory exercises
    /// this entry point so the data plane can carry a message
    /// that does not fit in one cell. Each returned
    /// `OBGWRouterDelivery` carries the same target router and
    /// receive tunnel id; the local endpoint reassembles the
    /// fragments into the original bytes.
    pub fn forward_cells<R: CryptoRng + RngCore>(
        &self,
        header: &TunnelPayloadHeader,
        complete_message: &[u8],
        rng: &mut R,
        now_ms: u64,
    ) -> Result<Vec<OBGWRouterDelivery>, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if matches!(header.delivery, DeliveryInstruction::Local) {
            return Err(TunnelRoleError::UnsupportedDeliveryInstruction);
        }
        let hops_reverse: Vec<LayerKeys> = self
            .established
            .hops()
            .iter()
            .map(|hop| hop.layer_keys().clone())
            .rev()
            .collect();
        let hops_ref: Vec<&LayerKeys> = hops_reverse.iter().collect();
        let fragments = TunnelMessageBuilder::fragment_complete_message(
            &header.delivery,
            header.message_id,
            complete_message,
        )?;
        let first_hop = self.established.first_hop_router();
        let receive_tunnel = self.established.first_hop_receive_tunnel();
        let mut cells = Vec::with_capacity(fragments.len());
        for fragment in fragments {
            let mut iv = [0_u8; TUNNEL_IV_LEN];
            rng.try_fill_bytes(&mut iv).map_err(|_| {
                TunnelRoleError::TunnelMessage(TunnelMessageError::RandomnessUnavailable)
            })?;
            let plaintext = TunnelMessageBuilder::new()
                .pack_payload(&fragment, &iv, rng)
                .map_err(TunnelRoleError::TunnelMessage)?;
            let (cell_iv, cell_data) =
                TunnelLayerTransform::outbound_preprocess(&hops_ref, iv, plaintext);
            let cell = join_cell(receive_tunnel.get(), cell_iv, cell_data);
            cells.push(OBGWRouterDelivery {
                target_router: first_hop,
                receive_tunnel,
                cell,
            });
        }
        Ok(cells)
    }

    /// Convenience helper for tests and unit data that supplies a
    /// concrete IV and uses the caller-time `now_ms`.
    pub fn forward_with_iv<R: CryptoRng + RngCore>(
        &self,
        header: &TunnelPayloadHeader,
        complete_message: &[u8],
        iv: [u8; TUNNEL_IV_LEN],
        rng: &mut R,
        now_ms: u64,
    ) -> Result<OBGWRouterDelivery, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if matches!(header.delivery, DeliveryInstruction::Local) {
            return Err(TunnelRoleError::UnsupportedDeliveryInstruction);
        }
        let plaintext = TunnelMessageBuilder::new()
            .build_single(header, complete_message, iv, rng)
            .map_err(TunnelRoleError::TunnelMessage)?;
        let hops_reverse: Vec<LayerKeys> = self
            .established
            .hops()
            .iter()
            .map(|hop| hop.layer_keys().clone())
            .rev()
            .collect();
        let hops_ref: Vec<&LayerKeys> = hops_reverse.iter().collect();
        let (cell_iv, cell_data) =
            TunnelLayerTransform::outbound_preprocess(&hops_ref, iv, plaintext);
        let first_hop = self.established.first_hop_router();
        let receive_tunnel = self.established.first_hop_receive_tunnel();
        let cell = join_cell(receive_tunnel.get(), cell_iv, cell_data);
        Ok(OBGWRouterDelivery {
            target_router: first_hop,
            receive_tunnel,
            cell,
        })
    }
}

/// Outbound gateway delivery record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OBGWRouterDelivery {
    /// First-hop router hash the caller must address.
    pub target_router: TunnelPeer,
    /// First-hop receive tunnel id the `TunnelData` cell carries.
    pub receive_tunnel: TunnelId,
    /// Preprocessed `TunnelData` cell addressed to the first hop.
    pub cell: TunnelDataMessage,
}

/// Participant-role shared state.
#[derive(Debug)]
struct ParticipantState {
    direction: TunnelDirection,
    receive_tunnel: TunnelId,
    next: EstablishedNextHop,
    layer_keys: LayerKeys,
    locked_previous_peer: Option<Hash>,
    duplicates: DuplicateWindow,
    expires_at_ms: u64,
}

impl ParticipantState {
    fn new(
        direction: TunnelDirection,
        hop: &EstablishedHop,
        duplicates: DuplicateWindow,
        expires_at_ms: u64,
    ) -> Result<Self, TunnelRoleError> {
        let next = hop.next().cloned().ok_or(TunnelRoleError::MissingNextHop)?;
        Ok(Self {
            direction,
            receive_tunnel: hop.receive_tunnel(),
            next,
            layer_keys: hop.layer_keys().clone(),
            locked_previous_peer: None,
            duplicates,
            expires_at_ms,
        })
    }

    fn process(
        &mut self,
        previous_peer: &Hash,
        cell: &TunnelDataMessage,
        now_ms: u64,
    ) -> Result<TunnelDataMessage, TunnelRoleError> {
        if now_ms >= self.expires_at_ms {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if cell.tunnel_id == 0 {
            return Err(TunnelRoleError::ZeroTunnelId);
        }
        let cell_tunnel_id =
            TunnelId::new(cell.tunnel_id).map_err(|_| TunnelRoleError::ZeroTunnelId)?;
        if cell_tunnel_id != self.receive_tunnel {
            return Err(TunnelRoleError::ReceiveTunnelMismatch {
                actual: cell_tunnel_id,
                expected: self.receive_tunnel,
            });
        }
        match self.locked_previous_peer {
            None => self.locked_previous_peer = Some(*previous_peer),
            Some(locked) => {
                if locked != *previous_peer {
                    return Err(TunnelRoleError::PreviousPeerMismatch);
                }
            }
        }
        let (iv, ciphertext) = split_cell(cell);
        let token = DuplicateToken::compute(&iv, &ciphertext);
        // Plan 130 §9: an exact cell replay is rejected by the live
        // duplicate window instead of being forwarded again.
        if !self.duplicates.observe(token)? {
            return Err(TunnelRoleError::DuplicateCell);
        }
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&self.layer_keys, &iv, &ciphertext);
        Ok(join_cell(self.next.tunnel.get(), next_iv, next_data))
    }

    fn next_router(&self) -> TunnelPeer {
        self.next.router
    }
}

/// Outbound participant role.
#[derive(Debug)]
pub struct OutboundParticipantRole {
    inner: ParticipantState,
}

impl OutboundParticipantRole {
    /// Constructs an outbound participant role from the supplied
    /// hop record.
    pub fn new(
        hop: &EstablishedHop,
        duplicates: DuplicateWindow,
        expires_at_ms: u64,
    ) -> Result<Self, TunnelRoleError> {
        Ok(Self {
            inner: ParticipantState::new(
                TunnelDirection::Outbound,
                hop,
                duplicates,
                expires_at_ms,
            )?,
        })
    }

    /// Returns whether the role is usable at the supplied time.
    pub fn is_usable(&self, now_ms: u64) -> bool {
        self.inner.expires_at_ms > now_ms
    }

    /// Processes one inbound TunnelData cell. Returns the
    /// next-hop `TunnelData` cell.
    pub fn process(
        &mut self,
        previous_peer: &Hash,
        cell: &TunnelDataMessage,
        now_ms: u64,
    ) -> Result<TunnelDataMessage, TunnelRoleError> {
        self.inner.process(previous_peer, cell, now_ms)
    }

    /// Returns the next-hop router hash.
    pub fn next_router(&self) -> TunnelPeer {
        self.inner.next_router()
    }
}

/// Inbound participant role.
#[derive(Debug)]
pub struct InboundParticipantRole {
    inner: ParticipantState,
}

impl InboundParticipantRole {
    /// Constructs an inbound participant role from the supplied
    /// hop record.
    pub fn new(
        hop: &EstablishedHop,
        duplicates: DuplicateWindow,
        expires_at_ms: u64,
    ) -> Result<Self, TunnelRoleError> {
        Ok(Self {
            inner: ParticipantState::new(TunnelDirection::Inbound, hop, duplicates, expires_at_ms)?,
        })
    }

    /// Returns whether the role is usable at the supplied time.
    pub fn is_usable(&self, now_ms: u64) -> bool {
        self.inner.expires_at_ms > now_ms
    }

    /// Processes one inbound TunnelData cell.
    pub fn process(
        &mut self,
        previous_peer: &Hash,
        cell: &TunnelDataMessage,
        now_ms: u64,
    ) -> Result<TunnelDataMessage, TunnelRoleError> {
        self.inner.process(previous_peer, cell, now_ms)
    }

    /// Returns the next-hop router hash.
    pub fn next_router(&self) -> TunnelPeer {
        self.inner.next_router()
    }
}

/// Outbound endpoint role. The OBEP applies the **final forward
/// participant layer** to expose the plaintext Tunnel Message,
/// then parses and reassembles all fragment records.
#[derive(Debug)]
pub struct OutboundEndpointRole {
    receive_tunnel: TunnelId,
    layer_keys: LayerKeys,
    duplicates: DuplicateWindow,
    expires_at_ms: u64,
    reassembler: BoundedReassembler,
    last_action: Option<RouterDeliveryAction>,
}

impl OutboundEndpointRole {
    /// Constructs an outbound endpoint role from a per-hop
    /// `EstablishedHop`.
    pub fn new(
        hop: &EstablishedHop,
        duplicates: DuplicateWindow,
        reassembler_capacity: usize,
        reassembler_aggregate_bytes: usize,
        reassembler_expiry_ms: u64,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            receive_tunnel: hop.receive_tunnel(),
            layer_keys: hop.layer_keys().clone(),
            duplicates,
            expires_at_ms,
            reassembler: BoundedReassembler::new(
                reassembler_capacity,
                reassembler_aggregate_bytes,
                reassembler_expiry_ms,
                now_ms,
            ),
            last_action: None,
        }
    }

    /// Returns the configured receive tunnel id.
    pub const fn receive_tunnel(&self) -> TunnelId {
        self.receive_tunnel
    }

    /// Returns whether the role is usable at the supplied time.
    pub fn is_usable(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }

    /// Processes one inbound TunnelData cell and returns the
    /// semantic router-delivery action when the fragment stream
    /// completes.
    pub fn process(
        &mut self,
        previous_peer: &Hash,
        cell: &TunnelDataMessage,
        now_ms: u64,
    ) -> Result<Option<RouterDeliveryAction>, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if cell.tunnel_id == 0 {
            return Err(TunnelRoleError::ZeroTunnelId);
        }
        let cell_tunnel_id =
            TunnelId::new(cell.tunnel_id).map_err(|_| TunnelRoleError::ZeroTunnelId)?;
        if cell_tunnel_id != self.receive_tunnel {
            return Err(TunnelRoleError::ReceiveTunnelMismatch {
                actual: cell_tunnel_id,
                expected: self.receive_tunnel,
            });
        }
        let _ = previous_peer;
        let (iv, ciphertext) = split_cell(cell);
        let token = DuplicateToken::compute(&iv, &ciphertext);
        // Plan 130 §9: an exact cell replay is rejected by the live
        // duplicate window instead of being reassembled again.
        if !self.duplicates.observe(token)? {
            return Err(TunnelRoleError::DuplicateCell);
        }
        // The OBEP applies the **final forward participant layer**
        // and recovers the original (next_iv, plaintext) the
        // creator preprocessed. The plaintext checksum must be
        // verified using the *recovered* `next_iv` because the
        // checksum was computed against the original plaintext IV
        // the gateway chose.
        let (next_iv, plaintext) =
            TunnelLayerTransform::participant_forward(&self.layer_keys, &iv, &ciphertext);
        let records = TunnelMessageParser::new()
            .parse(&next_iv, &plaintext)
            .map_err(TunnelRoleError::TunnelMessage)?;
        self.assemble_actions(records, now_ms)
    }

    fn assemble_actions(
        &mut self,
        records: Vec<FragmentDelivery>,
        now_ms: u64,
    ) -> Result<Option<RouterDeliveryAction>, TunnelRoleError> {
        self.reassembler.set_now(now_ms);
        let mut completed: Option<RouterDeliveryAction> = None;
        let mut last_action: Option<RouterDeliveryAction> = None;
        for record in records {
            let message_id = record.fragment.message_id().unwrap_or(0);
            match record.fragment {
                TunnelFragment::Unfragmented { body } => {
                    // The delivery instruction sits on the
                    // FragmentDelivery.
                    let delivery = record
                        .delivery
                        .clone()
                        .ok_or(TunnelRoleError::UnsupportedDeliveryInstruction)?;
                    let action = action_from_delivery(&delivery, body.clone(), message_id, 0)?;
                    last_action = Some(action.clone());
                    completed = Some(action);
                }
                TunnelFragment::First { body, .. } => {
                    let delivery = record
                        .delivery
                        .clone()
                        .ok_or(TunnelRoleError::UnsupportedDeliveryInstruction)?;
                    let key = ReassemblyKey {
                        context_id: self.receive_tunnel.get(),
                        message_id,
                    };
                    match self.reassembler.insert_with_delivery(
                        key,
                        TunnelFragment::First { message_id, body },
                        Some(delivery.clone()),
                    ) {
                        Ok(Some(reassembled)) => {
                            let delivery_instruction = reassembled.delivery.unwrap_or(delivery);
                            let action = action_from_delivery(
                                &delivery_instruction,
                                reassembled.message,
                                message_id,
                                0,
                            )?;
                            last_action = Some(action.clone());
                            completed = Some(action);
                        }
                        Ok(None) => {}
                        Err(error) => return Err(TunnelRoleError::Reassembly(error)),
                    }
                }
                TunnelFragment::FollowOn {
                    message_id: fmid,
                    sequence,
                    is_last,
                    body,
                } => {
                    let key = ReassemblyKey {
                        context_id: self.receive_tunnel.get(),
                        message_id: fmid,
                    };
                    match self.reassembler.insert_with_delivery(
                        key,
                        TunnelFragment::FollowOn {
                            message_id: fmid,
                            sequence,
                            is_last,
                            body,
                        },
                        record.delivery.clone(),
                    ) {
                        Ok(Some(reassembled)) => {
                            // The reassembler retained the
                            // first-fragment delivery instruction.
                            // Without it the OBEP must reject the
                            // completed message rather than
                            // synthesise a LOCAL fallback.
                            let delivery_instruction = reassembled.delivery.ok_or(
                                TunnelRoleError::UnspecifiedDeliveryInstruction {
                                    message_id: fmid,
                                },
                            )?;
                            let action = action_from_delivery(
                                &delivery_instruction,
                                reassembled.message,
                                fmid,
                                0,
                            )?;
                            last_action = Some(action.clone());
                            completed = Some(action);
                        }
                        Ok(None) => {}
                        Err(error) => return Err(TunnelRoleError::Reassembly(error)),
                    }
                }
            }
        }
        self.last_action = last_action;
        Ok(completed)
    }
}

fn action_from_delivery(
    delivery: &DeliveryInstruction,
    message: Vec<u8>,
    message_id: u32,
    expiration_ms: u64,
) -> Result<RouterDeliveryAction, TunnelRoleError> {
    match delivery {
        DeliveryInstruction::Local => Ok(RouterDeliveryAction {
            target_router: zero_hash(),
            kind: RouterDeliveryKind::Local,
            tunnel_id: None,
            message,
            message_id,
            expiration_ms,
        }),
        DeliveryInstruction::Router { router } => Ok(RouterDeliveryAction {
            target_router: *router,
            kind: RouterDeliveryKind::Router,
            tunnel_id: None,
            message,
            message_id,
            expiration_ms,
        }),
        DeliveryInstruction::Tunnel { tunnel_id, gateway } => {
            let id = TunnelId::new(*tunnel_id).map_err(|_| TunnelRoleError::ZeroTunnelId)?;
            Ok(RouterDeliveryAction {
                target_router: *gateway,
                kind: RouterDeliveryKind::TunnelGateway,
                tunnel_id: Some(id),
                message,
                message_id,
                expiration_ms,
            })
        }
    }
}

fn zero_hash() -> Hash {
    Hash::from_bytes([0; 32])
}

/// Inbound gateway role. The IBGW accepts a `TunnelGateway` and
/// emits one outbound `TunnelData` cell.
#[derive(Debug)]
pub struct InboundGatewayRole {
    receive_tunnel: TunnelId,
    next: EstablishedNextHop,
    layer_keys: LayerKeys,
    duplicates: DuplicateWindow,
    expires_at_ms: u64,
}

impl InboundGatewayRole {
    /// Constructs an inbound gateway role from the inbound
    /// gateway `EstablishedHop`.
    pub fn new(
        hop: &EstablishedHop,
        duplicates: DuplicateWindow,
        expires_at_ms: u64,
    ) -> Result<Self, TunnelRoleError> {
        let next = hop.next().cloned().ok_or(TunnelRoleError::MissingNextHop)?;
        Ok(Self {
            receive_tunnel: hop.receive_tunnel(),
            next,
            layer_keys: hop.layer_keys().clone(),
            duplicates,
            expires_at_ms,
        })
    }

    /// Returns the configured receive tunnel id.
    pub const fn receive_tunnel(&self) -> TunnelId {
        self.receive_tunnel
    }

    /// Returns the next-hop router hash.
    pub fn next_router(&self) -> TunnelPeer {
        self.next.router
    }

    /// Returns whether the role is usable at the supplied time.
    pub fn is_usable(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }

    /// Wraps the supplied standard I2NP message in a
    /// `TunnelGatewayMessage` and emits one outbound `TunnelData`
    /// cell addressed to the next hop.
    pub fn process<R: CryptoRng + RngCore>(
        &self,
        gateway: &TunnelGatewayMessage,
        rng: &mut R,
        now_ms: u64,
    ) -> Result<OutboundCell, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        let actual = TunnelId::new(gateway.tunnel_id).map_err(|_| TunnelRoleError::ZeroTunnelId)?;
        if actual != self.receive_tunnel {
            return Err(TunnelRoleError::GatewayTunnelMismatch {
                actual,
                expected: self.receive_tunnel,
            });
        }
        let iv = fresh_iv(rng)?;
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 1,
            expiration_ms: 0,
        };
        // The standard I2NP message already carries its own
        // canonical message metadata. We do **not** layer an
        // additional `message_id` or expiration timestamp over it;
        // the tunnel data plane simply carries its bytes.
        let inner_bytes = gateway
            .message
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .map_err(|_| TunnelRoleError::TunnelMessage(TunnelMessageError::EmptyMessage))?;
        let plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &inner_bytes, iv, rng)
            .map_err(TunnelRoleError::TunnelMessage)?;
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&self.layer_keys, &iv, &plaintext);
        Ok(OutboundCell {
            target_router: self.next.router,
            tunnel_id: self.next.tunnel,
            iv: next_iv,
            data: next_data,
            cell: join_cell(self.next.tunnel.get(), next_iv, next_data),
        })
    }

    /// Wraps the supplied standard I2NP message in a
    /// `TunnelGatewayMessage` and emits the ordered
    /// `OutboundCell` set addressed to the next hop, one cell per
    /// fragment. The Plan 116 fragmented cross-tunnel trajectory
    /// uses this entry point when the inner I2NP message does
    /// not fit in one TunnelData cell.
    pub fn process_cells<R: CryptoRng + RngCore>(
        &self,
        gateway: &TunnelGatewayMessage,
        rng: &mut R,
        now_ms: u64,
    ) -> Result<Vec<OutboundCell>, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        let actual = TunnelId::new(gateway.tunnel_id).map_err(|_| TunnelRoleError::ZeroTunnelId)?;
        if actual != self.receive_tunnel {
            return Err(TunnelRoleError::GatewayTunnelMismatch {
                actual,
                expected: self.receive_tunnel,
            });
        }
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 1,
            expiration_ms: 0,
        };
        let inner_bytes = gateway
            .message
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .map_err(|_| TunnelRoleError::TunnelMessage(TunnelMessageError::EmptyMessage))?;
        let fragments = TunnelMessageBuilder::fragment_complete_message(
            &header.delivery,
            header.message_id,
            &inner_bytes,
        )?;
        let cells = TunnelMessageBuilder::new()
            .build_cells(&fragments, rng)
            .map_err(TunnelRoleError::TunnelMessage)?;
        let mut outbound = Vec::with_capacity(cells.len());
        for (iv, plaintext) in cells {
            let (next_iv, next_data) =
                TunnelLayerTransform::participant_forward(&self.layer_keys, &iv, &plaintext);
            outbound.push(OutboundCell {
                target_router: self.next.router,
                tunnel_id: self.next.tunnel,
                iv: next_iv,
                data: next_data,
                cell: join_cell(self.next.tunnel.get(), next_iv, next_data),
            });
        }
        Ok(outbound)
    }
}

/// Local inbound endpoint role. The owner is the creator; the
/// role strips every inbound hop layer and exposes the
/// reconstructed standard I2NP message.
#[derive(Debug)]
pub struct LocalInboundEndpointRole {
    established: EstablishedTunnel,
    reassembler: BoundedReassembler,
    expires_at_ms: u64,
}

impl LocalInboundEndpointRole {
    /// Constructs a local inbound endpoint role from the inbound
    /// established tunnel.
    pub fn new(
        established: EstablishedTunnel,
        reassembler_capacity: usize,
        reassembler_aggregate_bytes: usize,
        reassembly_expiry_ms: u64,
        now_ms: u64,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            established,
            reassembler: BoundedReassembler::new(
                reassembler_capacity,
                reassembler_aggregate_bytes,
                reassembly_expiry_ms,
                now_ms,
            ),
            expires_at_ms,
        }
    }

    /// Returns the local inbound receive tunnel id.
    pub fn local_receive_tunnel(&self) -> TunnelId {
        self.established.local_inbound_receive()
    }

    /// Returns whether the role is usable at the supplied time.
    pub fn is_usable(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms && self.established.direction() == TunnelDirection::Inbound
    }

    /// Processes one inbound TunnelData cell. Returns the
    /// reconstructed standard I2NP message when the reassembler
    /// completes an in-flight message.
    pub fn process(
        &mut self,
        _previous_peer: &Hash,
        cell: &TunnelDataMessage,
        now_ms: u64,
    ) -> Result<Option<Vec<u8>>, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if cell.tunnel_id == 0 {
            return Err(TunnelRoleError::ZeroTunnelId);
        }
        let cell_tunnel_id =
            TunnelId::new(cell.tunnel_id).map_err(|_| TunnelRoleError::ZeroTunnelId)?;
        if cell_tunnel_id != self.local_receive_tunnel() {
            return Err(TunnelRoleError::ReceiveTunnelMismatch {
                actual: cell_tunnel_id,
                expected: self.local_receive_tunnel(),
            });
        }
        let hops_reverse: Vec<LayerKeys> = self
            .established
            .hops()
            .iter()
            .map(|hop| hop.layer_keys().clone())
            .rev()
            .collect();
        let hops_ref: Vec<&LayerKeys> = hops_reverse.iter().collect();
        let (iv, ciphertext) = split_cell(cell);
        // The local endpoint decrypts every inbound layer over
        // the remote hops in reverse path order. The output
        // `(recovered_iv, plaintext)` pair matches the original
        // creator-side preprocessing: `recovered_iv` is the IV
        // the original builder used to seed `build_single`, and
        // the checksum was computed against it.
        let (recovered_iv, plaintext) =
            TunnelLayerTransform::inbound_endpoint_decrypt(&hops_ref, iv, ciphertext);
        let records = TunnelMessageParser::new()
            .parse(&recovered_iv, &plaintext)
            .map_err(TunnelRoleError::TunnelMessage)?;
        if records.is_empty() {
            return Ok(None);
        }
        // The local endpoint MUST receive LOCAL delivery. Walk
        // every record, advancing the reassembler with the
        // record's first-fragment delivery instruction retained
        // until completion, and surface the completed bytes when
        // a fragment stream completes. The retained delivery
        // instruction must be `Local`; a non-local delivery
        // instruction is rejected as a hard violation of the
        // local-endpoint contract.
        self.reassembler.set_now(now_ms);
        let mut completed: Option<Vec<u8>> = None;
        for record in records {
            let delivery = record.delivery.clone();
            match &delivery {
                Some(DeliveryInstruction::Router { .. })
                | Some(DeliveryInstruction::Tunnel { .. }) => {
                    return Err(TunnelRoleError::LocalInboundNonLocalDelivery);
                }
                _ => {}
            }
            let message_id = record.fragment.message_id().unwrap_or(0);
            let key = ReassemblyKey {
                context_id: self.local_receive_tunnel().get(),
                message_id,
            };
            match self.reassembler.insert_with_delivery(
                key,
                record.fragment.clone(),
                delivery.clone(),
            ) {
                Ok(Some(reassembled)) => {
                    // Plan 116 F4: the reassembled message must
                    // surface its retained delivery instruction.
                    // The local endpoint accepts only `Local`
                    // delivery; any other retained delivery
                    // instruction is a hard violation.
                    match reassembled.delivery {
                        Some(DeliveryInstruction::Local) => {}
                        Some(_) => return Err(TunnelRoleError::LocalInboundNonLocalDelivery),
                        None => {
                            return Err(TunnelRoleError::UnspecifiedDeliveryInstruction {
                                message_id,
                            });
                        }
                    }
                    completed = Some(reassembled.message);
                }
                Ok(None) => {}
                Err(error) => return Err(TunnelRoleError::Reassembly(error)),
            }
        }
        Ok(completed)
    }
}

/// Outbound cell produced by the inbound gateway role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundCell {
    /// Next-hop router.
    pub target_router: TunnelPeer,
    /// Next-hop receive tunnel id.
    pub tunnel_id: TunnelId,
    /// IV the IBGW emits after the participant forward.
    pub iv: [u8; TUNNEL_IV_LEN],
    /// Encrypted 1008-byte payload.
    pub data: [u8; TUNNEL_PAYLOAD_LEN],
    /// Convenience `TunnelDataMessage` the caller hands to the
    /// next hop.
    pub cell: TunnelDataMessage,
}

impl Drop for ParticipantState {
    fn drop(&mut self) {
        self.layer_keys.zeroize();
    }
}

impl Drop for OutboundEndpointRole {
    fn drop(&mut self) {
        self.layer_keys.zeroize();
    }
}

impl Drop for InboundGatewayRole {
    fn drop(&mut self) {
        self.layer_keys.zeroize();
    }
}

impl Drop for LocalInboundEndpointRole {
    fn drop(&mut self) {
        // The EstablishedTunnel zeros its own hops on drop.
    }
}

/// Build an `EstablishedMaterial` placeholder. Used by tests that
/// exercise the participant role without going through the full
/// build state machine.
#[allow(dead_code)]
pub fn build_test_established_outbound(
    creator_tunnel_id: TunnelId,
    hops: Vec<(TunnelPeer, LayerKeys, TunnelPeer, TunnelId)>,
    created_at_seconds: u64,
) -> Result<EstablishedMaterial, crate::established::EstablishedTunnelError> {
    use crate::established::{
        EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole,
    };
    let new_hops: Vec<EstablishedHop> = {
        let mut v = Vec::with_capacity(hops.len());
        for (peer_hash, hop_keys, next_router, next_tunnel) in hops.into_iter() {
            let role = EstablishedRole::Participant;
            let next = EstablishedNextHop::new(next_router, next_tunnel);
            v.push(EstablishedHop::with_next(
                peer_hash,
                role,
                TunnelId::new(1).expect("id"),
                hop_keys,
                next,
            ));
        }
        // Re-order: ensure the OBEP is last.
        if let Some(last) = v.last_mut() {
            let peer = last.peer();
            let receive = last.receive_tunnel();
            let keys = last.layer_keys().clone();
            *last =
                EstablishedHop::terminal(peer, EstablishedRole::OutboundEndpoint, receive, keys);
        }
        v
    };
    let _ = new_hops;
    Ok(EstablishedMaterial {
        direction: TunnelDirection::Outbound,
        creator_tunnel_id,
        hops: Vec::new(),
        created_at_seconds,
        inbound_gateway: (zero_peer(), zero_id()),
        local_inbound_receive: zero_id(),
        extracted: true,
    })
}

/// Re-export the build module's tunnel-build attempt id type used
/// by the registrar public API.
pub use crate::short::BuildAttemptId as TestBuildAttemptId;

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn rng_seed(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    fn keys(seed: u8) -> LayerKeys {
        LayerKeys::new(
            [seed; 32],
            [seed.wrapping_add(1); 32],
            [seed.wrapping_add(2); 32],
        )
    }

    fn peer(value: u8) -> TunnelPeer {
        TunnelPeer::from_hash(Hash::from_bytes([value; 32]))
    }

    fn build_one_hop_outbound_established(creator: TunnelId) -> EstablishedTunnel {
        // Outbound OBEP is the only hop (and has no `next`).
        let hops = vec![EstablishedHop::terminal(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
        )];
        EstablishedTunnel::new(TunnelDirection::Outbound, creator, hops, 0, None, None)
            .expect("tunnel")
    }

    fn build_two_hop_outbound_established(creator: TunnelId) -> EstablishedTunnel {
        // Outbound [Participant, OBEP].
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::Participant,
                TunnelId::new(0x100).expect("id"),
                keys(0x10),
                EstablishedNextHop::new(peer(2), TunnelId::new(0x200).expect("id")),
            ),
            EstablishedHop::terminal(
                peer(2),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x200).expect("id"),
                keys(0x11),
            ),
        ];
        EstablishedTunnel::new(TunnelDirection::Outbound, creator, hops, 0, None, None)
            .expect("tunnel")
    }

    fn build_three_hop_outbound_established(creator: TunnelId) -> EstablishedTunnel {
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::Participant,
                TunnelId::new(0x100).expect("id"),
                keys(0x10),
                EstablishedNextHop::new(peer(2), TunnelId::new(0x200).expect("id")),
            ),
            EstablishedHop::with_next(
                peer(2),
                EstablishedRole::Participant,
                TunnelId::new(0x200).expect("id"),
                keys(0x11),
                EstablishedNextHop::new(peer(3), TunnelId::new(0x300).expect("id")),
            ),
            EstablishedHop::terminal(
                peer(3),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x300).expect("id"),
                keys(0x12),
            ),
        ];
        EstablishedTunnel::new(TunnelDirection::Outbound, creator, hops, 0, None, None)
            .expect("tunnel")
    }

    fn build_three_hop_inbound_established(creator: TunnelId) -> (TunnelId, EstablishedTunnel) {
        let local_receive = TunnelId::new(0x901).expect("id");
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::InboundGateway,
                TunnelId::new(0x100).expect("id"),
                keys(0x10),
                EstablishedNextHop::new(peer(2), TunnelId::new(0x200).expect("id")),
            ),
            EstablishedHop::with_next(
                peer(2),
                EstablishedRole::Participant,
                TunnelId::new(0x200).expect("id"),
                keys(0x11),
                EstablishedNextHop::new(peer(3), TunnelId::new(local_receive.get()).expect("id")),
            ),
        ];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            creator,
            hops,
            0,
            Some((peer(1), TunnelId::new(0x100).expect("id"))),
            Some(local_receive),
        )
        .expect("tunnel");
        (local_receive, tunnel)
    }

    #[test]
    fn outbound_gateway_emits_first_hop_cell() {
        let tunnel = build_two_hop_outbound_established(TunnelId::new(1).expect("id"));
        let gateway = OutboundGatewayRole::new(tunnel, 60_000);
        let mut rng = rng_seed(1);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x99; 32]),
            },
            message_id: 1,
            expiration_ms: 60_000,
        };
        let delivery = gateway
            .forward(&header, &[0xAA_u8; 32], &mut rng, 0)
            .expect("forward");
        assert_eq!(delivery.receive_tunnel.get(), 0x100);
        assert_eq!(delivery.target_router, peer(1));
        assert_eq!(delivery.cell.tunnel_id, 0x100);
    }

    #[test]
    fn outbound_gateway_rejects_local_delivery() {
        let tunnel = build_one_hop_outbound_established(TunnelId::new(1).expect("id"));
        let gateway = OutboundGatewayRole::new(tunnel, 60_000);
        let mut rng = rng_seed(1);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 1,
            expiration_ms: 60_000,
        };
        let outcome = gateway.forward(&header, &[0xAA_u8; 32], &mut rng, 0);
        assert!(matches!(
            outcome,
            Err(TunnelRoleError::UnsupportedDeliveryInstruction)
        ));
    }

    #[test]
    fn outbound_gateway_rejects_expired_tunnel() {
        let tunnel = build_one_hop_outbound_established(TunnelId::new(1).expect("id"));
        let gateway = OutboundGatewayRole::new(tunnel, 1_000);
        let mut rng = rng_seed(1);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x99; 32]),
            },
            message_id: 1,
            expiration_ms: 60_000,
        };
        let outcome = gateway.forward(&header, &[0xAA_u8; 32], &mut rng, 2_000);
        assert!(matches!(outcome, Err(TunnelRoleError::TunnelUnavailable)));
    }

    #[test]
    fn outbound_two_hop_router_round_trip() {
        let tunnel = build_two_hop_outbound_established(TunnelId::new(1).expect("id"));
        let gateway = OutboundGatewayRole::new(tunnel, 60_000);
        let mut rng = rng_seed(2);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x99; 32]),
            },
            message_id: 7,
            expiration_ms: 60_000,
        };
        let message = vec![0xCC_u8; 64];
        let delivery = gateway
            .forward(&header, &message, &mut rng, 0)
            .expect("forward");
        // Construct participant roles and the OBEP from the
        // established hops to round-trip the cell.
        let tunnel = gateway.established();
        let hops = tunnel.hops();
        let mut participant =
            OutboundParticipantRole::new(&hops[0], DuplicateWindow::new(16), 60_000)
                .expect("participant");
        let mut obep = OutboundEndpointRole::new(
            &hops[1],
            DuplicateWindow::new(16),
            16,
            1 << 20,
            60_000,
            60_000,
            0,
        );
        let next_cell = participant
            .process(&peer(0).hash(), &delivery.cell, 0)
            .expect("participant forward");
        let action = obep
            .process(&peer(1).hash(), &next_cell, 0)
            .expect("obep")
            .expect("delivery");
        assert_eq!(action.kind, RouterDeliveryKind::Router);
        assert_eq!(action.target_router, Hash::from_bytes([0x99; 32]));
        assert_eq!(action.message, message);
    }

    #[test]
    fn outbound_three_hop_trajectory_reconstructs_exact_bytes() {
        let tunnel = build_three_hop_outbound_established(TunnelId::new(1).expect("id"));
        let gateway = OutboundGatewayRole::new(tunnel, 60_000);
        let mut rng = rng_seed(3);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0xA5; 32]),
            },
            message_id: 9,
            expiration_ms: 60_000,
        };
        let message = vec![0x5A_u8; 200];
        let delivery = gateway
            .forward(&header, &message, &mut rng, 0)
            .expect("forward");
        let tunnel = gateway.established();
        let hops = tunnel.hops();
        let mut p1 =
            OutboundParticipantRole::new(&hops[0], DuplicateWindow::new(16), 60_000).expect("p1");
        let mut p2 =
            OutboundParticipantRole::new(&hops[1], DuplicateWindow::new(16), 60_000).expect("p2");
        let mut obep = OutboundEndpointRole::new(
            &hops[2],
            DuplicateWindow::new(16),
            16,
            1 << 20,
            60_000,
            60_000,
            0,
        );
        let after_p1 = p1.process(&peer(0).hash(), &delivery.cell, 0).expect("a");
        let after_p2 = p2.process(&peer(1).hash(), &after_p1, 0).expect("b");
        let action = obep
            .process(&peer(2).hash(), &after_p2, 0)
            .expect("o")
            .expect("delivery");
        assert_eq!(action.message, message);
        assert_eq!(action.target_router, Hash::from_bytes([0xA5; 32]));
    }

    #[test]
    fn participant_role_rejects_wrong_receive_tunnel() {
        let tunnel = build_two_hop_outbound_established(TunnelId::new(1).expect("id"));
        let hop = &tunnel.hops()[0];
        let mut role = OutboundParticipantRole::new(hop, DuplicateWindow::new(16), 60_000)
            .expect("participant");
        let mut cell_data = [0_u8; 1024];
        cell_data[0..16].copy_from_slice(&[0x11_u8; 16]);
        cell_data[16..].copy_from_slice(&[0x22_u8; 1008]);
        let cell = TunnelDataMessage {
            tunnel_id: 0x999,
            data: cell_data,
        };
        let outcome = role.process(&peer(0).hash(), &cell, 0);
        assert!(matches!(
            outcome,
            Err(TunnelRoleError::ReceiveTunnelMismatch { .. })
        ));
    }

    #[test]
    fn participant_role_rejects_zero_tunnel_id() {
        let tunnel = build_two_hop_outbound_established(TunnelId::new(1).expect("id"));
        let hop = &tunnel.hops()[0];
        let mut role = OutboundParticipantRole::new(hop, DuplicateWindow::new(16), 60_000)
            .expect("participant");
        let cell = TunnelDataMessage {
            tunnel_id: 0,
            data: [0_u8; 1024],
        };
        let outcome = role.process(&peer(0).hash(), &cell, 0);
        assert!(matches!(outcome, Err(TunnelRoleError::ZeroTunnelId)));
    }

    #[test]
    fn participant_role_locks_previous_peer() {
        let tunnel = build_two_hop_outbound_established(TunnelId::new(1).expect("id"));
        let hop = &tunnel.hops()[0];
        let mut role = OutboundParticipantRole::new(hop, DuplicateWindow::new(16), 60_000)
            .expect("participant");
        let mut cell_data = [0_u8; 1024];
        cell_data[0..16].copy_from_slice(&[0x11_u8; 16]);
        cell_data[16..].copy_from_slice(&[0x22_u8; 1008]);
        let cell = TunnelDataMessage {
            tunnel_id: 0x100,
            data: cell_data,
        };
        let _ = role.process(&peer(7).hash(), &cell, 0).expect("first peer");
        let outcome = role.process(&peer(8).hash(), &cell, 0);
        assert!(matches!(
            outcome,
            Err(TunnelRoleError::PreviousPeerMismatch)
        ));
    }

    #[test]
    fn outbound_to_inbound_tunnel_trajectory_exact_bytes() {
        // Plan 116 §10 + §11: terminal closure trajectory. The
        // test executes OBGW -> outbound participant(s) -> OBEP
        // -> TunnelGateway construction -> IBGW -> inbound
        // participant(s) -> local inbound endpoint and asserts
        // the recovered standard I2NP bytes equal the original
        // body bytes exactly. The trajectory also exercises one
        // inbound participant hop (IBGW -> Participant).
        use i2pr_proto::{Date, I2npBody};
        let (local_receive, inbound_tunnel) =
            build_three_hop_inbound_established(TunnelId::new(2).expect("id"));
        // Outbound gateway: two hops into the OBEP.
        let obgw_id = TunnelId::new(0x111).expect("id");
        let outbound_hops = vec![
            EstablishedHop::with_next(
                peer(10),
                EstablishedRole::Participant,
                TunnelId::new(0x301).expect("id"),
                keys(0x20),
                EstablishedNextHop::new(peer(11), TunnelId::new(0x401).expect("id")),
            ),
            EstablishedHop::terminal(
                peer(11),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x401).expect("id"),
                keys(0x21),
            ),
        ];
        let outbound_tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            obgw_id,
            outbound_hops,
            0,
            None,
            None,
        )
        .expect("outbound");
        let gateway = OutboundGatewayRole::new(outbound_tunnel, 60_000);
        let mut rng = rng_seed(11);
        // The original standard I2NP message we will recover on
        // the local inbound endpoint. The body is a
        // DeliveryStatusMessage; the canonical encoding of the
        // standard I2NP header + payload round-trips through the
        // tunnel data plane.
        let original_message_id = 0x0102_0304_u32;
        let original_timestamp_ms = 60_000_u64;
        let original_inner = I2npMessage::new_standard(
            original_message_id,
            Date::from_millis(original_timestamp_ms),
            I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
                original_message_id,
                Date::from_millis(original_timestamp_ms),
            )),
        )
        .expect("inner i2np");
        let original_inner_bytes = original_inner
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode inner");
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Tunnel {
                tunnel_id: inbound_tunnel.hops()[0].receive_tunnel().get(),
                gateway: inbound_tunnel.hops()[0].peer().hash(),
            },
            message_id: original_message_id,
            expiration_ms: original_timestamp_ms,
        };
        let delivery = gateway
            .forward(&header, &original_inner_bytes, &mut rng, 0)
            .expect("forward");
        // The outbound is now travelling through:
        //   local OBGW -> outbound Participant (p1) -> outbound OBEP
        let outbound_hops_iter = gateway.established().hops();
        let mut out_p =
            OutboundParticipantRole::new(&outbound_hops_iter[0], DuplicateWindow::new(16), 60_000)
                .expect("op");
        let mut out_obep = OutboundEndpointRole::new(
            &outbound_hops_iter[1],
            DuplicateWindow::new(16),
            16,
            1 << 20,
            60_000,
            60_000,
            0,
        );
        let cell_after_op = out_p
            .process(&peer(0).hash(), &delivery.cell, 0)
            .expect("op forward");
        let obep_action = out_obep
            .process(&peer(11).hash(), &cell_after_op, 0)
            .expect("obep")
            .expect("delivery");
        // OBEP must report TunnelGateway to the IBGW with the
        // exact target router + receive tunnel id configured by
        // the inbound tunnel.
        let RouterDeliveryKind::TunnelGateway = obep_action.kind else {
            panic!("expected TunnelGateway");
        };
        let tunnel_id = obep_action.tunnel_id.expect("tunnel id");
        let gateway_router = obep_action.target_router;
        assert_eq!(tunnel_id, inbound_tunnel.hops()[0].receive_tunnel());
        assert_eq!(gateway_router, inbound_tunnel.hops()[0].peer().hash());
        // Construct the TunnelGatewayMessage the IBGW consumes
        // from the OBEP action. The inner `message` carries the
        // original standard I2NP message bytes.
        let nested_i2np =
            I2npMessage::decode_standard(&obep_action.message, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode obep inner i2np");
        let gateway_msg = TunnelGatewayMessage {
            tunnel_id: tunnel_id.get(),
            message: Box::new(nested_i2np),
        };
        // IBGW -> inbound participant -> local inbound endpoint.
        let ibgw_hop = &inbound_tunnel.hops()[0];
        let ibgw =
            InboundGatewayRole::new(ibgw_hop, DuplicateWindow::new(16), 60_000).expect("ibgw role");
        let ibgw_out = ibgw.process(&gateway_msg, &mut rng, 0).expect("ibgw");
        // The IBGW emitted cell is addressed to the next
        // (participant) hop with the configured next-tunnel id.
        assert_eq!(ibgw_out.target_router, inbound_tunnel.hops()[1].peer());
        assert_eq!(
            ibgw_out.tunnel_id.get(),
            inbound_tunnel.hops()[1].receive_tunnel().get()
        );
        let inbound_hops: Vec<crate::established::EstablishedHop> = inbound_tunnel.hops().to_vec();
        let mut in_p =
            InboundParticipantRole::new(&inbound_hops[1], DuplicateWindow::new(16), 60_000)
                .expect("inbound participant");
        let in_p_cell = in_p
            .process(&ibgw_hop.peer().hash(), &ibgw_out.cell, 0)
            .expect("participant forward");
        // The inbound participant's emitted cell targets the
        // local inbound endpoint with the local receive tunnel id.
        assert_eq!(in_p_cell.tunnel_id, local_receive.get());
        let mut endpoint =
            LocalInboundEndpointRole::new(inbound_tunnel, 16, 1 << 20, 60_000, 0, 60_000);
        let recovered_bytes = endpoint
            .process(&inbound_hops[1].peer().hash(), &in_p_cell, 0)
            .expect("endpoint process")
            .expect("reassembled message");
        // The recovered standard I2NP bytes must equal the
        // original inner bytes exactly. The local endpoint has
        // no TunnelGateway wrapping (delivery is LOCAL); the
        // bytes round-trip the inbound layer transforms and the
        // reassembler without loss.
        assert_eq!(recovered_bytes, original_inner_bytes);
        // Decode the recovered standard I2NP message and verify
        // every boundary-level field.
        let recovered_message =
            I2npMessage::decode_standard(&recovered_bytes, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode recovered");
        assert_eq!(
            recovered_message.header().message_id(),
            Some(original_message_id)
        );
        match recovered_message.body() {
            I2npBody::DeliveryStatus(status) => {
                assert_eq!(status.message_id, original_message_id);
                assert_eq!(status.timestamp.as_millis(), original_timestamp_ms);
            }
            other => panic!("expected DeliveryStatus body, got {other:?}"),
        }
    }

    #[test]
    fn outbound_to_inbound_fragmented_trajectory_exact_bytes() {
        // Plan 116 §10.6: large message that requires multiple
        // outbound and/or inbound TunnelData cells. The test
        // constructs a body big enough to fragment, runs the full
        // cross-tunnel trajectory through every role, and asserts
        // the recovered bytes equal the original.
        use i2pr_proto::{Date, I2npBody};
        let (_local_receive, inbound_tunnel) =
            build_three_hop_inbound_established(TunnelId::new(2).expect("id"));
        // Outbound gateway: two hops into the OBEP.
        let obgw_id = TunnelId::new(0x211).expect("id");
        let outbound_hops = vec![
            EstablishedHop::with_next(
                peer(20),
                EstablishedRole::Participant,
                TunnelId::new(0x501).expect("id"),
                keys(0x30),
                EstablishedNextHop::new(peer(21), TunnelId::new(0x601).expect("id")),
            ),
            EstablishedHop::terminal(
                peer(21),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x601).expect("id"),
                keys(0x31),
            ),
        ];
        let outbound_tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            obgw_id,
            outbound_hops,
            0,
            None,
            None,
        )
        .expect("outbound");
        let gateway = OutboundGatewayRole::new(outbound_tunnel, 60_000);
        let mut rng = rng_seed(31);
        // Construct a standard I2NP message whose encoded
        // standard-header + payload size is large enough to
        // fragment. The DeliveryStatus body itself is small; we
        // construct a `Data` body that carries an opaque payload
        // we control byte-for-byte.
        let payload: Vec<u8> = (0..4096_u32).map(|value| (value & 0xFF) as u8).collect();
        let expected_payload = payload.clone();
        let original_inner = I2npMessage::new_standard(
            0x0203_0405_u32,
            Date::from_millis(30_000),
            I2npBody::Data(i2pr_proto::OpaqueMessageBody {
                payload: i2pr_proto::DeferredPayload::new(
                    payload,
                    i2pr_proto::MAX_I2NP_PAYLOAD_SIZE,
                )
                .expect("payload size"),
            }),
        )
        .expect("inner i2np");
        let original_inner_bytes = original_inner
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode inner");
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Tunnel {
                tunnel_id: inbound_tunnel.hops()[0].receive_tunnel().get(),
                gateway: inbound_tunnel.hops()[0].peer().hash(),
            },
            message_id: 0x0203_0405,
            expiration_ms: 30_000,
        };
        let deliveries = gateway
            .forward_cells(&header, &original_inner_bytes, &mut rng, 0)
            .expect("forward_cells");
        assert!(
            deliveries.len() > 1,
            "fragmented trajectory must produce more than one cell"
        );
        // Walk every outbound cell through the outbound chain.
        // Each cell goes through the outbound participant then the
        // OBEP. The OBEP reassembles the original complete I2NP
        // message across fragments; when the last cell is
        // processed the OBEP exposes a `RouterDeliveryAction`
        // with the original delivery instruction and inner
        // bytes.
        let outbound_hops_iter = gateway.established().hops();
        let mut out_p =
            OutboundParticipantRole::new(&outbound_hops_iter[0], DuplicateWindow::new(16), 60_000)
                .expect("op");
        let mut out_obep = OutboundEndpointRole::new(
            &outbound_hops_iter[1],
            DuplicateWindow::new(16),
            16,
            1 << 20,
            60_000,
            60_000,
            0,
        );
        let mut obep_action: Option<RouterDeliveryAction> = None;
        for (index, delivery) in deliveries.iter().enumerate() {
            let cell_after_op = out_p
                .process(&peer(0).hash(), &delivery.cell, 0)
                .expect("op forward");
            let outcome = out_obep
                .process(&peer(21).hash(), &cell_after_op, 0)
                .expect("obep");
            // The OBEP emits the delivery action exactly once,
            // on the cell that completes reassembly.
            if index + 1 == deliveries.len() {
                obep_action = outcome;
            } else {
                assert!(
                    outcome.is_none(),
                    "OBEP must not emit a delivery action before the last fragment"
                );
            }
        }
        let obep_action = obep_action.expect("obep action after last fragment");
        let RouterDeliveryKind::TunnelGateway = obep_action.kind else {
            panic!("expected TunnelGateway");
        };
        let tunnel_id = obep_action.tunnel_id.expect("tunnel id");
        // The large original I2NP message forces the OBEP
        // reassembler to retain a partial first fragment until
        // the inbound trajectory delivers the follow-on cells.
        let nested_i2np =
            I2npMessage::decode_standard(&obep_action.message, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode obep inner i2np");
        let gateway_msg = TunnelGatewayMessage {
            tunnel_id: tunnel_id.get(),
            message: Box::new(nested_i2np),
        };
        let ibgw_hop = &inbound_tunnel.hops()[0];
        let ibgw =
            InboundGatewayRole::new(ibgw_hop, DuplicateWindow::new(16), 60_000).expect("ibgw role");
        let ibgw_cells = ibgw
            .process_cells(&gateway_msg, &mut rng, 0)
            .expect("ibgw cells");
        assert!(
            ibgw_cells.len() > 1,
            "fragmented inbound trajectory must produce more than one cell"
        );
        let inbound_hops: Vec<crate::established::EstablishedHop> = inbound_tunnel.hops().to_vec();
        let mut in_p =
            InboundParticipantRole::new(&inbound_hops[1], DuplicateWindow::new(16), 60_000)
                .expect("inbound participant");
        let ibgw_hop_peer_hash = inbound_hops[0].peer().hash();
        let inbound_participant_peer_hash = inbound_hops[1].peer().hash();
        let mut endpoint =
            LocalInboundEndpointRole::new(inbound_tunnel, 16, 1 << 20, 60_000, 0, 60_000);
        let mut recovered_bytes: Option<Vec<u8>> = None;
        for (index, ibgw_cell) in ibgw_cells.iter().enumerate() {
            let in_p_cell = in_p
                .process(&ibgw_hop_peer_hash, &ibgw_cell.cell, 0)
                .expect("participant forward");
            let outcome = endpoint
                .process(&inbound_participant_peer_hash, &in_p_cell, 0)
                .expect("endpoint process");
            if index + 1 == ibgw_cells.len() {
                recovered_bytes = outcome;
            } else {
                assert!(
                    outcome.is_none(),
                    "endpoint must not emit a message before the last fragment"
                );
            }
        }
        let recovered_bytes = recovered_bytes.expect("endpoint action after last fragment");
        assert_eq!(recovered_bytes, original_inner_bytes);
        let recovered_message =
            I2npMessage::decode_standard(&recovered_bytes, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode recovered");
        match recovered_message.body() {
            I2npBody::Data(data) => {
                assert_eq!(data.payload.as_bytes(), expected_payload.as_slice());
            }
            other => panic!("expected Data body, got {other:?}"),
        }
    }

    #[test]
    fn outbound_to_inbound_fragmented_out_of_order_trajectory_exact_bytes() {
        // Plan 116 T3: same role-level trajectory as
        // `outbound_to_inbound_fragmented_trajectory_exact_bytes`,
        // but the local endpoint receives the inbound TunnelData
        // cells in an order where at least one follow-on arrives
        // before the first fragment. The endpoint must not emit
        // any message until all unique fragments are present, and
        // must emit the recovered message exactly once with
        // byte-exact equality to the original standard I2NP bytes.
        use i2pr_proto::{Date, I2npBody};
        let (_local_receive, inbound_tunnel) =
            build_three_hop_inbound_established(TunnelId::new(2).expect("id"));
        // Outbound gateway: two hops into the OBEP.
        let obgw_id = TunnelId::new(0x311).expect("id");
        let outbound_hops = vec![
            EstablishedHop::with_next(
                peer(30),
                EstablishedRole::Participant,
                TunnelId::new(0x701).expect("id"),
                keys(0x40),
                EstablishedNextHop::new(peer(31), TunnelId::new(0x801).expect("id")),
            ),
            EstablishedHop::terminal(
                peer(31),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x801).expect("id"),
                keys(0x41),
            ),
        ];
        let outbound_tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            obgw_id,
            outbound_hops,
            0,
            None,
            None,
        )
        .expect("outbound");
        let gateway = OutboundGatewayRole::new(outbound_tunnel, 60_000);
        let mut rng = rng_seed(71);
        // Same payload size as the canonical fragmented trajectory
        // test to guarantee enough fragments for an out-of-order
        // acceptance criterion.
        let payload: Vec<u8> = (0..4096_u32).map(|value| (value & 0xFF) as u8).collect();
        let expected_payload = payload.clone();
        let original_inner = I2npMessage::new_standard(
            0x0303_0405_u32,
            Date::from_millis(40_000),
            I2npBody::Data(i2pr_proto::OpaqueMessageBody {
                payload: i2pr_proto::DeferredPayload::new(
                    payload,
                    i2pr_proto::MAX_I2NP_PAYLOAD_SIZE,
                )
                .expect("payload size"),
            }),
        )
        .expect("inner i2np");
        let original_inner_bytes = original_inner
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode inner");
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Tunnel {
                tunnel_id: inbound_tunnel.hops()[0].receive_tunnel().get(),
                gateway: inbound_tunnel.hops()[0].peer().hash(),
            },
            message_id: 0x0303_0405,
            expiration_ms: 40_000,
        };
        let deliveries = gateway
            .forward_cells(&header, &original_inner_bytes, &mut rng, 0)
            .expect("forward_cells");
        assert!(
            deliveries.len() > 1,
            "fragmented trajectory must produce more than one cell"
        );
        // Outbound leg: every cell goes through the outbound
        // participant and OBEP. The OBEP must emit exactly one
        // TUNNEL action after the last fragment; the action must
        // carry the exact IBGW router and inbound receive tunnel
        // id.
        let outbound_hops_iter = gateway.established().hops();
        let mut out_p =
            OutboundParticipantRole::new(&outbound_hops_iter[0], DuplicateWindow::new(16), 60_000)
                .expect("op");
        let mut out_obep = OutboundEndpointRole::new(
            &outbound_hops_iter[1],
            DuplicateWindow::new(16),
            16,
            1 << 20,
            60_000,
            60_000,
            0,
        );
        let mut obep_action: Option<RouterDeliveryAction> = None;
        for (index, delivery) in deliveries.iter().enumerate() {
            let cell_after_op = out_p
                .process(&peer(0).hash(), &delivery.cell, 0)
                .expect("op forward");
            let outcome = out_obep
                .process(&peer(31).hash(), &cell_after_op, 0)
                .expect("obep");
            if index + 1 == deliveries.len() {
                obep_action = outcome;
            } else {
                assert!(
                    outcome.is_none(),
                    "OBEP must not emit a delivery action before the last fragment"
                );
            }
        }
        let obep_action = obep_action.expect("obep action after last fragment");
        let RouterDeliveryKind::TunnelGateway = obep_action.kind else {
            panic!("expected TunnelGateway");
        };
        let tunnel_id = obep_action.tunnel_id.expect("tunnel id");
        assert_eq!(
            tunnel_id.get(),
            inbound_tunnel.hops()[0].receive_tunnel().get()
        );
        let nested_i2np =
            I2npMessage::decode_standard(&obep_action.message, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode obep inner i2np");
        let gateway_msg = TunnelGatewayMessage {
            tunnel_id: tunnel_id.get(),
            message: Box::new(nested_i2np),
        };
        // Inbound leg: run every IBGW-produced cell through the
        // inbound participant and collect the resulting endpoint
        // TunnelData cells. The reassembler must accept out-of-
        // order delivery, so the endpoint receives all cells and
        // only emits a complete message once the first fragment
        // has finally arrived.
        let ibgw_hop = &inbound_tunnel.hops()[0];
        let ibgw =
            InboundGatewayRole::new(ibgw_hop, DuplicateWindow::new(16), 60_000).expect("ibgw role");
        let ibgw_cells = ibgw
            .process_cells(&gateway_msg, &mut rng, 0)
            .expect("ibgw cells");
        assert!(
            ibgw_cells.len() > 1,
            "fragmented inbound trajectory must produce more than one cell"
        );
        let inbound_hops: Vec<crate::established::EstablishedHop> = inbound_tunnel.hops().to_vec();
        let mut in_p =
            InboundParticipantRole::new(&inbound_hops[1], DuplicateWindow::new(16), 60_000)
                .expect("inbound participant");
        let ibgw_hop_peer_hash = inbound_hops[0].peer().hash();
        let inbound_participant_peer_hash = inbound_hops[1].peer().hash();
        // Build the canonical endpoint-delivery order first so we
        // can detect the first-fragment cell unambiguously and
        // move it to the end of the delivery order.
        let mut endpoint_cells: Vec<TunnelDataMessage> = Vec::with_capacity(ibgw_cells.len());
        for ibgw_cell in &ibgw_cells {
            let in_p_cell = in_p
                .process(&ibgw_hop_peer_hash, &ibgw_cell.cell, 0)
                .expect("participant forward");
            endpoint_cells.push(in_p_cell);
        }
        // The first IBGW-produced cell is the first-fragment cell
        // and carries the LOCAL delivery instruction that drives
        // the local endpoint's reassembler. Move that cell to the
        // end of the delivery order so at least one follow-on
        // (including the `is_last = true` cell) arrives first.
        assert!(!endpoint_cells.is_empty());
        let first_fragment_cell = endpoint_cells[0].clone();
        let mut reordered = Vec::with_capacity(endpoint_cells.len());
        for cell in endpoint_cells.iter().skip(1).cloned() {
            reordered.push(cell);
        }
        reordered.push(first_fragment_cell);
        assert!(
            reordered.len() > 1,
            "reordered delivery vector must contain multiple cells"
        );
        // Now feed the cells to the local endpoint in the
        // reordered order. The endpoint must not emit anything
        // until every unique fragment has arrived, and must emit
        // exactly once.
        let mut endpoint =
            LocalInboundEndpointRole::new(inbound_tunnel, 16, 1 << 20, 60_000, 0, 60_000);
        let mut completion_count = 0usize;
        let mut recovered_bytes: Option<Vec<u8>> = None;
        for (index, cell) in reordered.iter().enumerate() {
            let outcome = endpoint
                .process(&inbound_participant_peer_hash, cell, 0)
                .expect("endpoint process");
            let is_last_cell = index + 1 == reordered.len();
            if let Some(bytes) = outcome {
                completion_count += 1;
                if is_last_cell {
                    recovered_bytes = Some(bytes);
                } else {
                    panic!(
                        "endpoint must not emit a message before all unique fragments are present"
                    );
                }
            } else {
                assert!(
                    !is_last_cell,
                    "endpoint must emit exactly once after all unique fragments are present"
                );
            }
        }
        assert_eq!(completion_count, 1, "endpoint must emit exactly once");
        let recovered_bytes = recovered_bytes.expect("recovered bytes");
        assert_eq!(recovered_bytes, original_inner_bytes);
        let recovered_message =
            I2npMessage::decode_standard(&recovered_bytes, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode recovered");
        match recovered_message.body() {
            I2npBody::Data(data) => {
                assert_eq!(data.payload.as_bytes(), expected_payload.as_slice());
            }
            other => panic!("expected Data body, got {other:?}"),
        }
    }
}
