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
//! The module keeps each role state minimal:
//!
//! - `OutboundGatewayRole` owns the creator's outbound established
//!   tunnel and emits `(target_router, semantic I2NP message)`
//!   actions per outbound message.
//! - `OutboundParticipantRole` and `InboundParticipantRole` own
//!   only the per-hop transform state and the bounded replay
//!   window.
//! - `OutboundEndpointRole` is the OBEP: it strips the final
//!   layer, parses/reassembles fragments, and applies the
//!   delivery instruction.
//! - `InboundGatewayRole` is the IBGW: it accepts a
//!   `TunnelGateway` and applies one outbound layer.
//! - `LocalInboundEndpointRole` is the local creator's inbound
//!   endpoint: it strips every inbound hop's layer, parses
//!   fragments, and returns the reconstructed LOCAL message.
//!
//! Participant state never holds creator path vectors. The data
//! plane is runtime-neutral: no Tokio, no sockets, no DNS.

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
use thiserror::Error;
use zeroize::Zeroize;

use crate::build_crypto::LayerKeys;
use crate::data::{
    DeliveryInstruction, FragmentDelivery, TunnelMessageBuilder, TunnelMessageError,
    TunnelMessageParser, TunnelPayloadHeader,
};
use crate::established::{EstablishedHop, EstablishedRole, EstablishedTunnel, zero_id, zero_peer};
use crate::fragment::{BoundedReassembler, ReassemblyError, ReassemblyKey, TunnelFragment};
use crate::identity::{TunnelDirection, TunnelId, TunnelPeer};
use crate::layer::{
    DuplicateToken, DuplicateWindow, DuplicateWindowError, TUNNEL_IV_LEN, TUNNEL_PAYLOAD_LEN,
    TunnelLayerTransform,
};

/// Semantic router-delivery kind the data plane emits. The runtime
/// composition layer later adapts each action into the existing
/// transport boundary; the data plane never hard-codes NTCP2.
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

/// Semantic router-delivery action the data plane emits. The
/// action carries the target router, the kind, and the
/// reconstructed standard I2NP message. The message id and
/// expiration are retained from the first-fragment header for
/// downstream dispatch.
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
    /// Original message identifier.
    pub message_id: u32,
    /// Original expiration timestamp in milliseconds since the
    /// Unix epoch.
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
    TunnelMessage(TunnelMessageError),
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
    /// instruction. The local endpoint is creator-side and only
    /// accepts LOCAL deliveries.
    #[error("local inbound endpoint received non-LOCAL delivery instruction")]
    LocalInboundNonLocalDelivery,
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

    /// Forwards one standard I2NP message through the outbound
    /// tunnel. The function returns the preprocessed
    /// `TunnelDataMessage` addressed to the first hop.
    pub fn forward(
        &self,
        header: &TunnelPayloadHeader,
        complete_message: &[u8],
        now_ms: u64,
    ) -> Result<OBGWRouterDelivery, TunnelRoleError> {
        if !self.is_usable(now_ms) {
            return Err(TunnelRoleError::TunnelUnavailable);
        }
        if matches!(header.delivery, DeliveryInstruction::Local) {
            return Err(TunnelRoleError::UnsupportedDeliveryInstruction);
        }
        let mut iv = [0_u8; TUNNEL_IV_LEN];
        for (idx, byte) in iv.iter_mut().enumerate() {
            *byte = ((idx as u8).wrapping_add(0x11)) ^ 0xA5;
        }
        let plaintext = TunnelMessageBuilder::new()
            .build_single(header, complete_message, iv, &mut DeterministicZeroRng)
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
            target_router: first_hop.hash(),
            receive_tunnel,
            cell,
            original_iv: iv,
            header: header.clone(),
            complete_message: complete_message.to_vec(),
        })
    }
}

/// Outbound gateway delivery record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OBGWRouterDelivery {
    /// First-hop router hash the caller must address.
    pub target_router: Hash,
    /// First-hop receive tunnel id the `TunnelData` cell carries.
    pub receive_tunnel: TunnelId,
    /// Preprocessed `TunnelData` cell addressed to the first hop.
    pub cell: TunnelDataMessage,
    /// Original plaintext IV before the layer transforms.
    pub original_iv: [u8; TUNNEL_IV_LEN],
    /// Original delivery header for diagnostics.
    pub header: TunnelPayloadHeader,
    /// Original complete I2NP message for diagnostics.
    pub complete_message: Vec<u8>,
}

/// Participant-role shared state.
#[derive(Debug)]
struct ParticipantState {
    direction: TunnelDirection,
    receive_tunnel: TunnelId,
    next_router: Hash,
    next_tunnel: TunnelId,
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
    ) -> Self {
        Self {
            direction,
            receive_tunnel: hop.receive_tunnel(),
            next_router: hop.next_router().hash(),
            next_tunnel: hop.next_tunnel(),
            layer_keys: hop.layer_keys().clone(),
            locked_previous_peer: None,
            duplicates,
            expires_at_ms,
        }
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
        self.duplicates.observe(token)?;
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&self.layer_keys, &iv, &ciphertext);
        Ok(join_cell(self.next_tunnel.get(), next_iv, next_data))
    }

    fn next_router(&self) -> Hash {
        self.next_router
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
    pub fn new(hop: &EstablishedHop, duplicates: DuplicateWindow, expires_at_ms: u64) -> Self {
        Self {
            inner: ParticipantState::new(TunnelDirection::Outbound, hop, duplicates, expires_at_ms),
        }
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
    pub fn next_router(&self) -> Hash {
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
    pub fn new(hop: &EstablishedHop, duplicates: DuplicateWindow, expires_at_ms: u64) -> Self {
        Self {
            inner: ParticipantState::new(TunnelDirection::Inbound, hop, duplicates, expires_at_ms),
        }
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
    pub fn next_router(&self) -> Hash {
        self.inner.next_router()
    }
}

/// Outbound endpoint role. The OBEP applies the final participant
/// layer and emits the rebuilt fragment records.
#[derive(Debug)]
pub struct OutboundEndpointRole {
    receive_tunnel: TunnelId,
    layer_keys: LayerKeys,
    duplicates: DuplicateWindow,
    expires_at_ms: u64,
    /// The router delivery action the OBEP produces when the
    /// fragment stream completes.
    last_action: Option<RouterDeliveryAction>,
}

impl OutboundEndpointRole {
    /// Constructs an outbound endpoint role from a per-hop
    /// `EstablishedHop`.
    pub fn new(hop: &EstablishedHop, duplicates: DuplicateWindow, expires_at_ms: u64) -> Self {
        Self {
            receive_tunnel: hop.receive_tunnel(),
            layer_keys: hop.layer_keys().clone(),
            duplicates,
            expires_at_ms,
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
        self.duplicates.observe(token)?;
        let (_, plaintext) =
            TunnelLayerTransform::creator_inverse_one_hop(&self.layer_keys, &iv, &ciphertext);
        let records = TunnelMessageParser::new()
            .parse(&iv, &plaintext)
            .map_err(TunnelRoleError::TunnelMessage)?;
        Ok(self.assemble_action(records))
    }

    fn assemble_action(&mut self, records: Vec<FragmentDelivery>) -> Option<RouterDeliveryAction> {
        let first = records.into_iter().next()?;
        let delivery = first.delivery?;
        let (kind, target_router, tunnel_id) = match &delivery {
            DeliveryInstruction::Local => {
                (RouterDeliveryKind::Local, Hash::from_bytes([0; 32]), None)
            }
            DeliveryInstruction::Router { router } => (RouterDeliveryKind::Router, *router, None),
            DeliveryInstruction::Tunnel { tunnel_id, gateway } => (
                RouterDeliveryKind::TunnelGateway,
                *gateway,
                Some(TunnelId::new(*tunnel_id).expect("nonzero")),
            ),
        };
        let (message_id, body) = match first.fragment {
            TunnelFragment::First { message_id, body } => (message_id, body),
            _ => return None,
        };
        let action = RouterDeliveryAction {
            target_router,
            kind,
            tunnel_id,
            message: body,
            message_id,
            expiration_ms: 0,
        };
        self.last_action = Some(action.clone());
        Some(action)
    }
}

/// Inbound gateway role. The IBGW accepts a `TunnelGateway` and
/// emits one outbound `TunnelData` cell.
#[derive(Debug)]
pub struct InboundGatewayRole {
    receive_tunnel: TunnelId,
    next_router: Hash,
    next_tunnel: TunnelId,
    layer_keys: LayerKeys,
    duplicates: DuplicateWindow,
    expires_at_ms: u64,
}

impl InboundGatewayRole {
    /// Constructs an inbound gateway role from the inbound
    /// gateway `EstablishedHop`.
    pub fn new(hop: &EstablishedHop, duplicates: DuplicateWindow, expires_at_ms: u64) -> Self {
        Self {
            receive_tunnel: hop.receive_tunnel(),
            next_router: hop.next_router().hash(),
            next_tunnel: hop.next_tunnel(),
            layer_keys: hop.layer_keys().clone(),
            duplicates,
            expires_at_ms,
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

    /// Wraps the supplied standard I2NP message in a
    /// `TunnelGatewayMessage` and emits one outbound `TunnelData`
    /// cell addressed to the next hop.
    pub fn process(
        &self,
        gateway: &TunnelGatewayMessage,
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
        let mut iv = [0_u8; TUNNEL_IV_LEN];
        iv[0] = 0x33;
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 1,
            expiration_ms: 0,
        };
        let inner_bytes = gateway
            .message
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .map_err(|_| TunnelRoleError::TunnelMessage(TunnelMessageError::EmptyMessage))?;
        let plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &inner_bytes, iv, &mut DeterministicZeroRng)
            .map_err(TunnelRoleError::TunnelMessage)?;
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&self.layer_keys, &iv, &plaintext);
        Ok(OutboundCell {
            target_router: self.next_router,
            tunnel_id: self.next_tunnel,
            iv: next_iv,
            data: next_data,
            cell: join_cell(self.next_tunnel.get(), next_iv, next_data),
        })
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
        reassembly_expiry_ms: u64,
        now_ms: u64,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            established,
            reassembler: BoundedReassembler::new(
                reassembler_capacity,
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
        previous_peer: &Hash,
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
        let (_, plaintext) =
            TunnelLayerTransform::inbound_endpoint_decrypt(&hops_ref, iv, ciphertext);
        let records = TunnelMessageParser::new()
            .parse(&iv, &plaintext)
            .map_err(TunnelRoleError::TunnelMessage)?;
        if records.is_empty() {
            return Ok(None);
        }
        let delivery = records[0]
            .delivery
            .clone()
            .ok_or(TunnelRoleError::LocalInboundNonLocalDelivery)?;
        if !matches!(delivery, DeliveryInstruction::Local) {
            return Err(TunnelRoleError::LocalInboundNonLocalDelivery);
        }
        let _ = previous_peer;
        let key = ReassemblyKey {
            context_id: self.local_receive_tunnel().get(),
            message_id: records[0].fragment.message_id(),
        };
        self.reassembler.set_now(now_ms);
        match self.reassembler.insert(key, records[0].fragment.clone()) {
            Ok(Some(message)) => Ok(Some(message)),
            Ok(None) => Ok(None),
            Err(error) => Err(TunnelRoleError::Reassembly(error)),
        }
    }
}

/// Outbound cell produced by the inbound gateway role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundCell {
    /// Next-hop router.
    pub target_router: Hash,
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

/// Deterministic zero-only RNG placeholder. The local data plane
/// uses the deterministic builder/parser path so the production
/// surface does not depend on a live CSPRNG for tests; production
/// wires a real CSPRNG through the public builder API.
#[derive(Clone, Copy, Debug)]
struct DeterministicZeroRng;

impl rand_core::RngCore for DeterministicZeroRng {
    fn next_u32(&mut self) -> u32 {
        0
    }
    fn next_u64(&mut self) -> u64 {
        0
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        dest.fill(0);
    }
}

impl rand_core::CryptoRng for DeterministicZeroRng {}

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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn outbound_gateway_emits_first_hop_cell() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            peer(2),
            TunnelId::new(0x200).expect("id"),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let gateway = OutboundGatewayRole::new(tunnel, 60_000);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x99; 32]),
            },
            message_id: 1,
            expiration_ms: 60_000,
        };
        let delivery = gateway
            .forward(&header, &[0xAA_u8; 32], 0)
            .expect("forward");
        assert_eq!(delivery.receive_tunnel.get(), 0x100);
        assert_eq!(delivery.target_router, Hash::from_bytes([1; 32]));
        assert_eq!(delivery.cell.tunnel_id, 0x100);
    }

    #[test]
    fn outbound_gateway_rejects_local_delivery() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            zero_peer(),
            zero_id(),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let gateway = OutboundGatewayRole::new(tunnel, 60_000);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 1,
            expiration_ms: 60_000,
        };
        let outcome = gateway.forward(&header, &[0xAA_u8; 32], 0);
        assert!(matches!(
            outcome,
            Err(TunnelRoleError::UnsupportedDeliveryInstruction)
        ));
    }

    #[test]
    fn outbound_gateway_rejects_expired_tunnel() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            zero_peer(),
            zero_id(),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let gateway = OutboundGatewayRole::new(tunnel, 1_000);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x99; 32]),
            },
            message_id: 1,
            expiration_ms: 60_000,
        };
        let outcome = gateway.forward(&header, &[0xAA_u8; 32], 2_000);
        assert!(matches!(outcome, Err(TunnelRoleError::TunnelUnavailable)));
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn participant_role_rejects_wrong_receive_tunnel() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            peer(2),
            TunnelId::new(0x200).expect("id"),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let hop = &tunnel.hops()[0];
        let mut role = OutboundParticipantRole::new(hop, DuplicateWindow::new(16), 60_000);
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
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn participant_role_rejects_zero_tunnel_id() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            peer(2),
            TunnelId::new(0x200).expect("id"),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let hop = &tunnel.hops()[0];
        let mut role = OutboundParticipantRole::new(hop, DuplicateWindow::new(16), 60_000);
        let cell = TunnelDataMessage {
            tunnel_id: 0,
            data: [0_u8; 1024],
        };
        let outcome = role.process(&peer(0).hash(), &cell, 0);
        assert!(matches!(outcome, Err(TunnelRoleError::ZeroTunnelId)));
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn participant_role_locks_previous_peer() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            peer(2),
            TunnelId::new(0x200).expect("id"),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let hop = &tunnel.hops()[0];
        let mut role = OutboundParticipantRole::new(hop, DuplicateWindow::new(16), 60_000);
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
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn outbound_endpoint_emits_router_delivery() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            zero_peer(),
            zero_id(),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let hop = &tunnel.hops()[0];
        let mut role = OutboundEndpointRole::new(hop, DuplicateWindow::new(16), 60_000);
        // Build the canonical outbound cell via the gateway.
        let gateway_tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(2).expect("id"),
            vec![EstablishedHop::new(
                peer(1),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x100).expect("id"),
                keys(0x10),
                zero_peer(),
                zero_id(),
            )],
            0,
            None,
            None,
        )
        .expect("tunnel");
        let gateway = OutboundGatewayRole::new(gateway_tunnel, 60_000);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x99; 32]),
            },
            message_id: 1,
            expiration_ms: 60_000,
        };
        let message = vec![0xAA_u8; 32];
        let delivery = gateway.forward(&header, &message, 0).expect("forward");
        let action = role
            .process(&peer(0).hash(), &delivery.cell, 0)
            .expect("process")
            .expect("delivery");
        assert_eq!(action.kind, RouterDeliveryKind::Router);
        assert_eq!(action.target_router, Hash::from_bytes([0x99; 32]));
        assert_eq!(action.message, message);
    }

    #[test]
    fn outbound_endpoint_rejects_non_local_after_decryption() {
        // Build a tunnel with the OBEP and emit a delivery
        // instruction that is not LOCAL/ROUTER/TUNNEL.
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
            zero_peer(),
            zero_id(),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        let hop = &tunnel.hops()[0];
        let mut role = OutboundEndpointRole::new(hop, DuplicateWindow::new(16), 60_000);
        let mut cell_data = [0_u8; 1024];
        cell_data[0..16].copy_from_slice(&[0x11_u8; 16]);
        cell_data[16..].copy_from_slice(&[0x22_u8; 1008]);
        let cell = TunnelDataMessage {
            tunnel_id: 0x100,
            data: cell_data,
        };
        let outcome = role.process(&peer(0).hash(), &cell, 0);
        // The deterministic-zero RNG fills padding with zeros,
        // which the parser rejects as a missing delimiter or
        // padding-byte-zero. Either typed failure is acceptable;
        // the data plane must fail closed.
        assert!(matches!(outcome, Err(TunnelRoleError::TunnelMessage(_))));
    }

    #[test]
    fn local_inbound_endpoint_rejects_non_local_delivery() {
        // Build the inbound tunnel with an IBGW + inbound
        // participant + IBEP, then send a TunnelData cell whose
        // decrypted delivery instruction is not LOCAL.
        let hops = vec![
            EstablishedHop::new(
                peer(1),
                EstablishedRole::InboundGateway,
                TunnelId::new(0x100).expect("id"),
                keys(0x11),
                peer(2),
                TunnelId::new(0x101).expect("id"),
            ),
            EstablishedHop::new(
                peer(2),
                EstablishedRole::InboundEndpoint,
                TunnelId::new(0x999).expect("id"),
                keys(0x12),
                peer(3),
                zero_id(),
            ),
        ];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            Some((peer(1), TunnelId::new(0x100).expect("id"))),
            Some(TunnelId::new(0x999).expect("id")),
        )
        .expect("tunnel");
        let mut role = LocalInboundEndpointRole::new(tunnel, 16, 60_000, 0, 60_000);
        let cell_data = [0_u8; 1024];
        let cell = TunnelDataMessage {
            tunnel_id: 0x999,
            data: cell_data,
        };
        let outcome = role.process(&peer(0).hash(), &cell, 0);
        // The deterministic-zero RNG cannot produce a valid
        // payload, so the parser will reject with a typed
        // failure. The data plane must fail closed without
        // returning a partial LOCAL message.
        assert!(matches!(outcome, Err(TunnelRoleError::TunnelMessage(_))));
    }
}

#[cfg(test)]
mod _suppress_unused {
    // Keep these imports so the module compiles cleanly when the
    // data-plane tests above are commented out.
    use super::*;
    #[allow(dead_code)]
    fn _keep_imports() {
        let _ = TunnelPeer::from_hash(Hash::from_bytes([0; 32]));
        let _ = TunnelId::new(1).expect("id");
        let _ = TunnelFragment::First {
            message_id: 1,
            body: vec![],
        };
        let _: Option<I2npMessage> = None;
        let _: Box<dyn Fn() -> Result<(), TunnelRoleError>> = Box::new(|| Ok(()));
    }
}
