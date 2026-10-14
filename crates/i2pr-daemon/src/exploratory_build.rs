//! Plan 185 daemon-owned short-build coordinator, extended by Plan 314 to
//! share one attempt owner across exploratory and qualified Destination paths.
//!
//! The coordinator owns the bounded, single-scheduler surface that
//! translates the existing [`crate::router_i2np::dispatch_router_i2np`]
//! authenticated router-I2NP handoff into the canonical
//! [`i2pr_tunnel::short::ShortBuildStateMachine`] lifecycle. It
//! does not spawn a task per build attempt; one central pump drives
//! every attempt through its phases. All request/reply ids,
//! deadlines, pending builds, and installed material are bounded.
//!
//! Required ownership:
//!
//! ```text
//! request inbound/outbound exploratory build
//!  -> choose exact reference RouterInfo from authoritative bounded
//!     store/config
//!  -> ShortBuildStateMachine
//!  -> ShortBuildI2npBridge (canonical production I2NP bridge)
//!  -> authenticated SSU2 RouterDelivery (Plan 184 narrow seam)
//!  -> authenticated build reply via central I2NP dispatcher
//!  -> state machine verification
//!  -> install EstablishedMaterial into existing pool/registry
//! ```
//!
//! Properties:
//!
//! - the coordinator never accepts a caller-supplied peer identity
//!   for the live path; the only live peer references are the
//!   authenticated `Ssu2InboundI2np::peer` values the Plan 184
//!   dispatcher hands the pump;
//! - one bounded scheduler owns every attempt; no per-build task;
//! - tunnel ids are allocated from a bounded monotonic pool with no
//!   reuse across the lifetime of the coordinator;
//! - the coordinator holds no secrets past the
//!   [`i2pr_tunnel::short::ShortBuildStateMachine`] boundary;
//!   established material is consumed once into the existing
//!   [`i2pr_tunnel::pool::ExploratoryPool`] and
//!   [`i2pr_tunnel::data_plane_registry::DataPlaneRegistry`]
//!   seams without synthetic insertion;
//! - the canonical production
//!   [`i2pr_tunnel::bridge::ShortBuildI2npBridge`] is the only
//!   path that emits a complete I2NP type-25 message;
//! - the
//!   [`crate::router_i2np::RouterDeliveryService`] is the only path
//!   that emits outbound authenticated I2NP;
//! - the strict decoder behaviour from Plan 184 (bounded strict
//!   rejection, no weakened AEAD/record-count checks) is preserved;
//! - tunnel-build replay protection uses the supplied attempt id
//!   and a bounded pending-build table; duplicate replies for the
//!   same attempt are dropped, not re-driven.
//!
//! The coordinator is runtime-neutral: it never opens sockets, never
//! spawns tasks, never performs DNS, never touches the filesystem.
//! All I/O happens through the daemon-owned seams
//! ([`crate::router_i2np::RouterDeliveryService`] for outbound,
//! the Plan 184 central dispatcher for inbound). The daemon service
//! owns exactly one coordinator instance; no hidden second
//! coordinator coexists with it in counted tests.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::time::Duration;

use i2pr_proto::{Date, Hash, I2npBody, I2npMessage};
use i2pr_runtime::Ssu2InboundI2np;
use i2pr_transport::{MAX_I2NP_MESSAGE_BYTES, PeerId};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;
use zeroize::Zeroizing;

use i2pr_client::DestinationId;
use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::build_crypto::GARLIC_REPLY_TAG_LEN;
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::data::{DeliveryInstruction, TunnelPayloadHeader};
use i2pr_tunnel::data_plane_registry::{DataPlaneCapacity, DataPlaneRegistry};
use i2pr_tunnel::garlic_reply::decrypt_build_reply_garlic;
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelLifetime, TunnelState};
use i2pr_tunnel::multirecord::decode_outbound_tunnel_build_reply;
use i2pr_tunnel::pool::{
    ExploratoryPool, PoolFullError, RegisterError, RegisterOutcome, TunnelRegistration, TunnelSlot,
};
use i2pr_tunnel::short::{
    BuildAttemptId, BuildEvent, HopSpec, ShortBuildAction, ShortBuildConstructionError,
    ShortBuildOutcome, ShortBuildPath, ShortBuildStateMachine,
};
use i2pr_tunnel::short_record::HopRole;

use crate::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, RouterDeliveryService, RouterDeliveryTicket,
    RouterDeliveryTransport, RouterI2npError, RouterI2npKind, RouterI2npOutcome,
    dispatch_router_i2np,
};

/// Maximum number of simultaneous pending exploratory build
/// attempts the coordinator tracks. The ceiling is conservative
/// for loopback-only operation and matches the canonical I2P
/// per-router build concurrency ceiling.
pub const MAX_PENDING_BUILDS: usize = 16;
/// Maximum message/tunnel identifiers retained across pending builds and the
/// longest local tunnel plus I2NP replay window.
pub const MAX_RETAINED_BUILD_IDS: usize = 65_536;
/// I2NP accepts message expirations up to fifteen minutes ahead. Retain build
/// message identifiers for that replay window after the active tunnel window.
pub const BUILD_MESSAGE_REPLAY_RETENTION_MS: u64 = 15 * 60 * 1000;
/// Upper bound for one active tunnel lifetime, from `TunnelLifetime`.
const MAX_BUILD_TUNNEL_LIFETIME_MS: u64 = 30 * 60 * 1000;
const BUILD_TUNNEL_ID_RETENTION_MS: u64 =
    MAX_BUILD_TUNNEL_LIFETIME_MS + BUILD_MESSAGE_REPLAY_RETENTION_MS;
/// Maximum established Destination materials awaiting owner registration.
pub const MAX_DESTINATION_HANDOFFS: usize = MAX_PENDING_BUILDS;
/// Maximum lifetime a tunnel is allowed to claim. Mirrors
/// [`i2pr_tunnel::identity::TunnelLifetime::MAX_LIFETIME_SECONDS`].
pub const MAX_EXPLORATORY_LIFETIME_SECONDS: u32 = TunnelLifetime::MAX_LIFETIME_SECONDS;
/// Default lifetime used when the daemon does not pick one.
pub const DEFAULT_EXPLORATORY_LIFETIME_SECONDS: u32 = TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS;
/// Maximum number of consecutive build failures tolerated before
/// the coordinator pauses further submissions.
pub const DEFAULT_FAILURE_THRESHOLD: u16 = 4;
/// Bounded reassembler capacity for an inbound exploratory tunnel.
pub const REASSEMBLER_CAPACITY: usize = 16;
/// Bounded reassembler aggregate byte budget.
pub const REASSEMBLER_AGGREGATE_BYTES: usize = 64 * 1024;
/// Bounded reassembler expiry in milliseconds.
pub const REASSEMBLER_EXPIRY_MS: u64 = 10_000;
/// Maximum outbound delivery timeout accepted by the build
/// coordinator. The narrow seam reuses
/// [`crate::router_i2np::MAX_ROUTER_DELIVERY_TIMEOUT`].
pub const BUILD_DELIVERY_TIMEOUT: Duration = Duration::from_secs(60);
/// Build deadline in milliseconds (relative). The coordinator
/// derives absolute deadlines from this plus `now_ms`.
pub const BUILD_DEADLINE_MS: u64 = 60_000;
/// Starting creator tunnel id; subsequent ids are allocated as
/// `next_creator_tunnel_id += 1`. The starting value lives above
/// the canonical zero sentinel and below the upper bound the
/// codec accepts.
pub const INITIAL_CREATOR_TUNNEL_ID: u32 = 0x1000;
/// Starting attempt id; subsequent ids are allocated as
/// `next_attempt_id += 1`. The starting value is the first
/// monotonic id the coordinator hands out.
pub const INITIAL_ATTEMPT_ID: u64 = 1;

fn fresh_transport_message_id<R: RngCore>(rng: &mut R) -> Result<u32, BuildCoordinatorError> {
    for _ in 0..32 {
        let candidate = rng.next_u32();
        if candidate != 0 {
            return Ok(candidate);
        }
    }
    Err(BuildCoordinatorError::Coordinator(
        "bounded tunnel-data message-id allocation exhausted",
    ))
}

fn wrap_inner_wire(
    bridge: &ShortBuildI2npBridge,
    action: &ShortBuildAction,
    header: BridgeHeader,
) -> Result<Vec<u8>, BuildCoordinatorError> {
    let (i2np_message, _) = bridge
        .wrap_deliver_action(action, header)
        .map_err(|error| {
            BuildCoordinatorError::Coordinator(match error {
                i2pr_tunnel::bridge::BridgeError::ZeroRecordCount => "STBM record count is zero",
                i2pr_tunnel::bridge::BridgeError::RecordCountMismatch { .. } => {
                    "STBM record count prefix mismatch"
                }
                i2pr_tunnel::bridge::BridgeError::RecordCountOutOfRange { .. } => {
                    "STBM record count above maximum"
                }
                i2pr_tunnel::bridge::BridgeError::PayloadLengthMismatch { .. } => {
                    "STBM payload length mismatch"
                }
                i2pr_tunnel::bridge::BridgeError::DeferredBuildRecords(_) => {
                    "deferred build records rejected"
                }
                i2pr_tunnel::bridge::BridgeError::MessageFraming(_) => {
                    "I2NP framing rejected the build message"
                }
                i2pr_tunnel::bridge::BridgeError::RoundTripBodyMismatch { .. } => {
                    "I2NP round-trip body mismatch"
                }
            })
        })?;
    match header {
        BridgeHeader::Standard { .. } => i2np_message
            .encode_standard_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .map_err(|_| BuildCoordinatorError::Coordinator("I2NP standard encoding failed")),
        BridgeHeader::ShortTransport { .. } => i2np_message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .map_err(|_| {
                BuildCoordinatorError::Coordinator("I2NP short-transport encoding failed")
            }),
    }
}

fn compose_outbound_build_deliveries<R: CryptoRng + RngCore>(
    role: &i2pr_tunnel::roles::OutboundGatewayRole,
    target_router: Hash,
    message_id: u32,
    expiration_ms: u64,
    wire: &[u8],
    now_ms: u64,
    rng: &mut R,
) -> Result<Vec<(PeerId, Vec<u8>, u32)>, BuildCoordinatorError> {
    let header = TunnelPayloadHeader {
        delivery: DeliveryInstruction::Router {
            router: target_router,
        },
        message_id,
        expiration_ms,
    };
    let cells = role
        .forward_cells(&header, wire, rng, now_ms)
        .map_err(|_| {
            BuildCoordinatorError::Coordinator("outbound tunnel rejected build payload")
        })?;
    if cells.is_empty() || cells.len() > 8 {
        return Err(BuildCoordinatorError::Coordinator(
            "outbound build fragmentation exceeded bounded cell count",
        ));
    }
    let transport_expiration = u32::try_from(expiration_ms / 1000)
        .map_err(|_| BuildCoordinatorError::Coordinator("tunnel-data expiration out of range"))?;
    cells
        .into_iter()
        .map(|routed| {
            // The fresh transport message ID is returned alongside
            // the wire so submit receipts can attribute stock-side
            // per-message outcomes (expiry/duplicate counters name
            // these IDs) without naming peers.
            let transport_message_id = fresh_transport_message_id(rng)?;
            let transport_message = I2npMessage::new_short_transport(
                transport_message_id,
                transport_expiration,
                I2npBody::TunnelData(Box::new(routed.cell)),
            )
            .map_err(|_| BuildCoordinatorError::Coordinator("tunnel-data I2NP framing failed"))?;
            let transport_wire = transport_message
                .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
                .map_err(|_| BuildCoordinatorError::Coordinator("tunnel-data encoding failed"))?;
            Ok((
                PeerId::from_hash(routed.target_router.hash()),
                transport_wire,
                transport_message_id,
            ))
        })
        .collect()
}

/// Direction of one pending build attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildDirection {
    /// Outbound exploratory tunnel: the local creator is the
    /// gateway; the remote peer is the endpoint.
    Outbound,
    /// Inbound exploratory tunnel: the local creator is the
    /// endpoint; the remote peer is the gateway.
    Inbound,
}

impl BuildDirection {
    /// Returns the matching tunnel direction for the pool/registry.
    pub const fn tunnel_direction(self) -> TunnelDirection {
        match self {
            Self::Outbound => TunnelDirection::Outbound,
            Self::Inbound => TunnelDirection::Inbound,
        }
    }
}

impl std::fmt::Display for BuildDirection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Outbound => "outbound",
            Self::Inbound => "inbound",
        };
        formatter.write_str(label)
    }
}

/// Per-peer static configuration used to derive the short-build
/// path. The coordinator never derives this from netDb lookups;
/// the caller supplies it through the bounded authoritative store.
#[derive(Clone)]
pub struct PeerBuildMaterial {
    /// Router hash (the receiving peer's identity).
    pub router_hash: Hash,
    /// Hop's static X25519 encryption key.
    pub static_encryption_key: [u8; i2pr_tunnel::build_crypto::EPHEMERAL_KEY_LEN],
    /// Tunnel id the peer is asked to use as its receive tunnel id.
    pub receive_tunnel: TunnelId,
    /// Tunnel id the peer is asked to use as its next-tunnel id
    /// (== the local creator's inbound endpoint receive tunnel for
    /// inbound builds; == the local creator's outbound gateway
    /// receive tunnel for outbound builds).
    pub next_tunnel: TunnelId,
    /// Hop role the peer plays.
    pub role: HopRole,
}

impl fmt::Debug for PeerBuildMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PeerBuildMaterial")
            .field("router_hash", &"<redacted>")
            .field("static_encryption_key", &"<redacted>")
            .field("receive_tunnel", &self.receive_tunnel)
            .field("next_tunnel", &self.next_tunnel)
            .field("role", &self.role)
            .finish()
    }
}

impl PeerBuildMaterial {
    /// Returns the canonical [`HopSpec`] the coordinator hands the
    /// state machine.
    pub fn hop_spec(&self) -> HopSpec {
        HopSpec::new(
            self.router_hash,
            self.static_encryption_key,
            self.role,
            self.receive_tunnel,
            self.next_tunnel,
        )
    }
}

/// One-peer exploratory submission record. Qualified Destination builds use
/// [`DestinationBuildRequest`] and share the coordinator's same attempt core.
#[derive(Clone)]
pub struct BuildRequest {
    /// Direction this build is for.
    pub direction: BuildDirection,
    /// Per-peer build material the state machine consumes.
    pub peer: PeerBuildMaterial,
    /// Creator tunnel id the pool registrar reserves for the
    /// established material. The coordinator owns the monotonic
    /// pool of creator tunnel ids.
    pub creator_tunnel_id: TunnelId,
    /// Terminal build reply-correlation ID. The outer I2NP header
    /// ID is supplied independently through `BridgeHeader`.
    pub message_id: u32,
    /// Outbound reply-router identity hash the OBEP records as
    /// its terminal `next_router_hash`. Required for outbound
    /// builds; the local creator's identity is the only valid
    /// source.
    pub outbound_reply_router: Option<Hash>,
    /// Inbound originator identity hash the terminal inbound hop
    /// records as its terminal `next_router_hash`. Required for
    /// inbound builds; the local creator's identity is the only
    /// valid source.
    pub originator_hash: Option<Hash>,
}

impl fmt::Debug for BuildRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BuildRequest")
            .field("direction", &self.direction)
            .field("peer", &self.peer)
            .field("creator_tunnel_id", &self.creator_tunnel_id)
            .field("message_id", &self.message_id)
            .field("outbound_reply_router", &"<redacted>")
            .field("originator_hash", &"<redacted>")
            .finish()
    }
}

/// Qualified Destination build request. Its ordered peers are kept separate
/// from the one-peer exploratory request and must contain exactly three hops.
#[derive(Clone)]
pub struct DestinationBuildRequest {
    /// Group Destination whose canonical pool will receive established
    /// material. This is a local owner key and is never sent on the wire.
    pub destination_id: DestinationId,
    /// Direction this build is for.
    pub direction: BuildDirection,
    /// Ordered remote hop material selected by the Destination peer selector.
    pub peers: Vec<PeerBuildMaterial>,
    /// Creator tunnel id reserved for established material.
    pub creator_tunnel_id: TunnelId,
    /// Terminal build reply-correlation ID. The outer I2NP header
    /// ID is supplied independently through `BridgeHeader`.
    pub message_id: u32,
    /// Local router hash used by the outbound terminal hop.
    pub outbound_reply_router: Option<Hash>,
    /// Local router hash used by the inbound terminal hop.
    pub originator_hash: Option<Hash>,
}

impl fmt::Debug for DestinationBuildRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DestinationBuildRequest")
            .field("direction", &self.direction)
            .field("peer_count", &self.peers.len())
            .field("peers", &"<redacted>")
            .field("creator_tunnel_id", &self.creator_tunnel_id)
            .field("message_id", &self.message_id)
            .field("outbound_reply_router", &"<redacted>")
            .field("originator_hash", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
struct BuildPathRequest {
    direction: BuildDirection,
    peers: Vec<PeerBuildMaterial>,
    creator_tunnel_id: TunnelId,
    message_id: u32,
    outbound_reply_router: Option<Hash>,
    originator_hash: Option<Hash>,
    destination_id: Option<DestinationId>,
}

fn reserved_path_tunnel_ids(
    request: &BuildPathRequest,
) -> Result<HashSet<u32>, BuildCoordinatorError> {
    if request.peers.is_empty() {
        return Err(BuildCoordinatorError::Coordinator(
            "build path has no peers",
        ));
    }
    let mut ids = HashSet::new();
    ids.insert(request.creator_tunnel_id.get());
    for peer in &request.peers {
        if !ids.insert(peer.receive_tunnel.get()) {
            return Err(BuildCoordinatorError::Coordinator(
                "receive tunnel id collides within build path",
            ));
        }
    }
    if request
        .peers
        .windows(2)
        .any(|pair| pair[0].next_tunnel != pair[1].receive_tunnel)
    {
        return Err(BuildCoordinatorError::Coordinator(
            "adjacent build-hop tunnel chain is inconsistent",
        ));
    }
    let terminal_next = request
        .peers
        .last()
        .expect("nonempty path")
        .next_tunnel
        .get();
    if !ids.insert(terminal_next) {
        return Err(BuildCoordinatorError::Coordinator(
            "terminal tunnel id collides within build path",
        ));
    }
    Ok(ids)
}

impl From<BuildRequest> for BuildPathRequest {
    fn from(request: BuildRequest) -> Self {
        Self {
            direction: request.direction,
            peers: vec![request.peer],
            creator_tunnel_id: request.creator_tunnel_id,
            message_id: request.message_id,
            outbound_reply_router: request.outbound_reply_router,
            originator_hash: request.originator_hash,
            destination_id: None,
        }
    }
}

impl From<DestinationBuildRequest> for BuildPathRequest {
    fn from(request: DestinationBuildRequest) -> Self {
        Self {
            direction: request.direction,
            peers: request.peers,
            creator_tunnel_id: request.creator_tunnel_id,
            message_id: request.message_id,
            outbound_reply_router: request.outbound_reply_router,
            originator_hash: request.originator_hash,
            destination_id: Some(request.destination_id),
        }
    }
}

/// Outcome of one resolved attempt that the coordinator hands back
/// to its caller. Variants mirror the state-machine outcomes plus
/// the coordinator-level disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildCoordinatorOutcome {
    /// The build succeeded and the material was installed in the
    /// pool + registry without synthetic insertion.
    Installed {
        /// Slot the pool reserved.
        slot: TunnelSlot,
        /// Pool registration the coordinator published.
        registration: TunnelRegistration,
        /// Hop count the established tunnel carries.
        hop_count: usize,
        /// Direction that succeeded.
        direction: BuildDirection,
    },
    /// A qualified Destination build reached Established and its one-shot
    /// material is waiting for registration by the owning group runtime.
    DestinationEstablished {
        /// Attempt id used to take the bounded material handoff.
        attempt_id: BuildAttemptId,
        /// Owning group Destination id.
        destination_id: DestinationId,
        /// Hop count proven by the completed build.
        hop_count: usize,
        /// Direction that succeeded.
        direction: BuildDirection,
    },
    /// The owning group did not take the bounded established-material handoff.
    DestinationHandoffRejected {
        /// Direction that failed.
        direction: BuildDirection,
    },
    /// A group-owned Destination build reached a terminal failure. Keeping
    /// the owner id on this summary lets the owning pool account failures
    /// without exposing peer or payload details.
    DestinationBuildFailed {
        /// Owning group Destination.
        destination_id: DestinationId,
        /// Direction that failed.
        direction: BuildDirection,
    },
    /// A group-owned Destination build was cancelled by generation removal.
    DestinationBuildCancelled {
        /// Owning group Destination.
        destination_id: DestinationId,
        /// Direction cancelled.
        direction: BuildDirection,
    },
    /// The build reached `Established` but the pool rejected the
    /// installed material without losing the established
    /// [`i2pr_tunnel::established::EstablishedTunnel`]. The
    /// coordinator drops the material and reports the typed
    /// rejection.
    PoolRejected {
        /// Direction that failed.
        direction: BuildDirection,
        /// Typed rejection category.
        kind: PoolFullError,
    },
    /// The build reached `Established` but the supplied
    /// [`i2pr_tunnel::established::EstablishedMaterial`] was
    /// invalid. The coordinator drops the material and reports the
    /// typed registration error.
    InvalidRegistration {
        /// Direction that failed.
        direction: BuildDirection,
        /// Typed rejection category.
        reason: &'static str,
    },
    /// The remote hop rejected the build with the supplied short
    /// response code.
    HopRejected {
        /// Direction that failed.
        direction: BuildDirection,
        /// Index of the rejecting hop (zero for one-hop).
        hop_index: u8,
        /// Response code byte the hop produced.
        response_code: u8,
    },
    /// The build reply never arrived within the coordinator's
    /// bounded deadline.
    TimedOut {
        /// Direction that failed.
        direction: BuildDirection,
    },
    /// The build was cancelled before it reached a terminal state.
    Cancelled {
        /// Direction that failed.
        direction: BuildDirection,
    },
    /// The build reply was malformed and the state machine could
    /// not reach Established. The coordinator never weakens the
    /// decoder.
    InvalidReply {
        /// Attempt whose authenticated reply failed validation.
        attempt_id: BuildAttemptId,
        /// Direction that failed.
        direction: BuildDirection,
        /// Short reason from the state machine.
        reason: &'static str,
    },
    /// The delivery of the build request failed through the narrow
    /// outbound seam.
    DeliveryFailed {
        /// Direction that failed.
        direction: BuildDirection,
        /// Typed delivery outcome.
        delivery: RouterDeliveryOutcome,
    },
    /// The path was invalid; no build attempt was started.
    InvalidPath {
        /// Direction that failed.
        direction: BuildDirection,
        /// Short reason from the path validator.
        reason: &'static str,
    },
    /// The coordinator rejected the submission for a bounded
    /// reason (too many pending builds, exhausted attempt ids,
    /// no router identity, etc.).
    CoordinatorRejected {
        /// Direction that failed.
        direction: BuildDirection,
        /// Short reason from the coordinator.
        reason: &'static str,
    },
}

impl BuildCoordinatorOutcome {
    /// Returns the direction of the attempt regardless of the
    /// variant.
    pub const fn direction(&self) -> BuildDirection {
        match self {
            Self::Installed { direction, .. }
            | Self::PoolRejected { direction, .. }
            | Self::InvalidRegistration { direction, .. }
            | Self::HopRejected { direction, .. }
            | Self::TimedOut { direction, .. }
            | Self::Cancelled { direction, .. }
            | Self::InvalidReply { direction, .. }
            | Self::DeliveryFailed { direction, .. }
            | Self::DestinationEstablished { direction, .. }
            | Self::DestinationHandoffRejected { direction, .. }
            | Self::DestinationBuildFailed { direction, .. }
            | Self::DestinationBuildCancelled { direction, .. }
            | Self::InvalidPath { direction, .. }
            | Self::CoordinatorRejected { direction, .. } => *direction,
        }
    }
}

/// Typed submission / dispatch failure the coordinator surfaces.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BuildCoordinatorError {
    /// The supplied request violated a coordinator-level invariant.
    #[error("short-build coordinator rejected the request: {0}")]
    Coordinator(&'static str),
    /// The path validator rejected the supplied path.
    #[error("short-build path is invalid: {reason}")]
    InvalidPath {
        /// Short reason from the validator.
        reason: &'static str,
    },
}

/// Bounded privacy-safe counters for the coordinator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildCoordinatorCounters {
    /// Successful installed material.
    pub installed: u64,
    /// Hop-rejected builds observed.
    pub hop_rejections: u64,
    /// Timeouts the coordinator expired.
    pub timeouts: u64,
    /// Cancelled builds the coordinator drained.
    pub cancellations: u64,
    /// Invalid replies the strict decoder rejected.
    pub invalid_replies: u64,
    /// Delivery failures the narrow seam reported.
    pub delivery_failures: u64,
    /// Pending builds outstanding at the snapshot.
    pub pending: usize,
    /// Consecutive failures observed since the last successful
    /// install.
    pub consecutive_failures: u16,
    /// Inbound dispatched tunnel-build outcomes the coordinator
    /// routed to a pending attempt.
    pub inbound_routed: u64,
    /// Inbound dispatched tunnel-build outcomes the coordinator
    /// dropped because no pending attempt matched.
    pub inbound_orphans: u64,
    /// Unmatched forwarded inbound build replies whose authenticated
    /// peer matched a pending inbound attempt but whose message id did not.
    pub inbound_orphans_peer_match: u64,
    /// Unmatched forwarded inbound build replies whose message id matched
    /// a pending inbound attempt but whose authenticated peer did not.
    pub inbound_orphans_message_match: u64,
    /// Same-terminal-peer orphan forwarded builds whose ID is expected ID + 1 modulo u32.
    pub inbound_orphans_peer_id_plus_one: u64,
    /// Same-terminal-peer orphan forwarded builds whose ID is expected ID - 1 modulo u32.
    pub inbound_orphans_peer_id_minus_one: u64,
    /// Same-terminal-peer orphan forwarded builds with another nonmatching ID.
    pub inbound_orphans_peer_id_other: u64,
    /// Inbound-build TunnelGateway replies observed on the authenticated path.
    pub inbound_gateway_messages: u64,
    /// Unmatched inbound-build gateways from the selected terminal peer.
    pub inbound_gateway_peer_match: u64,
    /// Unmatched inbound-build gateways matching both terminal peer and local reply tunnel.
    pub inbound_gateway_route_match: u64,
    /// Duplicate inbound replies the coordinator rejected.
    pub duplicate_replies: u64,
    /// Outbound tunnel-build requests the coordinator emitted.
    pub outbound_builds: u64,
}

/// Attempt-scoped, count-only relationship between an authenticated inbound
/// short-build message and one pending inbound build. This stays in memory
/// with the pending attempt and is emitted only if that attempt expires.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InboundBuildObservation {
    pub authenticated_candidates: u64,
    pub peer_matches: u64,
    pub message_id_matches: u64,
    pub peer_match_id_plus_one: u64,
    pub peer_match_id_minus_one: u64,
    pub peer_match_id_other: u64,
}

impl InboundBuildObservation {
    fn record(&mut self, peer_match: bool, message_id: u32, expected_message_id: u32) {
        self.authenticated_candidates = self.authenticated_candidates.saturating_add(1);
        if peer_match {
            if message_id == expected_message_id {
                self.message_id_matches = self.message_id_matches.saturating_add(1);
            } else {
                match message_id.wrapping_sub(expected_message_id) {
                    1 => {
                        self.peer_match_id_plus_one = self.peer_match_id_plus_one.saturating_add(1);
                    }
                    u32::MAX => {
                        self.peer_match_id_minus_one =
                            self.peer_match_id_minus_one.saturating_add(1);
                    }
                    _ => {
                        self.peer_match_id_other = self.peer_match_id_other.saturating_add(1);
                    }
                }
            }
            self.peer_matches = self.peer_matches.saturating_add(1);
        } else if message_id == expected_message_id {
            self.message_id_matches = self.message_id_matches.saturating_add(1);
        }
    }
}

/// One pending build attempt tracked by the coordinator. The
/// coordinator never clones the secrets inside the state machine;
/// the state machine is owned by `state` until the attempt reaches
/// a terminal outcome.
#[allow(dead_code)]
struct PendingBuild {
    attempt_id: BuildAttemptId,
    direction: BuildDirection,
    destination_id: Option<DestinationId>,
    /// Outbound peer hash (the receiving router). The coordinator
    /// matches inbound replies whose authenticated peer equals
    /// this value.
    target_peer: PeerId,
    /// Deadline (wall clock ms) at which the coordinator expires
    /// the attempt.
    deadline_ms: u64,
    /// Per-attempt message id the I2NP header carried.
    message_id: u32,
    /// Independently allocated initial outer-header message id.
    outer_message_id: u32,
    /// All locally allocated tunnel ids in this build, retained through
    /// tunnel lifetime and the subsequent replay window.
    reserved_tunnel_ids: Vec<u32>,
    /// Creator-supplied `next_tunnel` the reference echoes in its
    /// `TunnelGateway` reply for outbound endpoint builds.
    /// Plan 188 correlation seam for garlic-wrapped replies.
    next_tunnel: TunnelId,
    /// OBEP `RGarlicKeyAndTag` key for garlic-wrapped endpoint
    /// replies (`None` for inbound builds, which never garlic-wrap).
    garlic_key: Option<Zeroizing<[u8; 32]>>,
    /// OBEP garlic reply tag (8 bytes) paired with `garlic_key`.
    garlic_tag: Option<[u8; GARLIC_REPLY_TAG_LEN]>,
    /// Bounded by this pending attempt's lifetime and the existing pending
    /// attempt ceiling. Raw peer and message identifiers are not retained.
    inbound_observation: InboundBuildObservation,
    /// Source-owned writer receipts for the one direct request or each
    /// TunnelData transport cell. SSU2 queue admission remains explicitly
    /// unobserved until its writer-ticket producer is available.
    delivery_set: BuildDeliverySet,
    /// Live state machine. The coordinator never spawns a task
    /// per attempt; the daemon-owned pump drives every attempt.
    state: ShortBuildStateMachine,
}

#[derive(Default)]
struct BuildDeliverySet {
    required: usize,
    tickets: Vec<Option<RouterDeliveryTicket>>,
    observed: usize,
    emitted: usize,
    failed: bool,
    unobserved: bool,
}

/// Local disposition for one pending build's originator-side transport
/// writes. This does not imply remote receipt or stock-router acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildEmissionDisposition {
    /// The selected transport owner has not completed the write yet.
    Pending,
    /// Every required originator delivery completed at its socket writer.
    AllOriginatorDeliveriesEmitted,
    /// At least one required delivery failed after some write activity.
    PartialEmission,
    /// No write completed and delivery was rejected or its owner closed.
    NotEmitted,
    /// One or more selected transports have no source-owned write ticket.
    UnobservedTransport,
}

impl BuildDeliverySet {
    fn disposition(&self) -> BuildEmissionDisposition {
        if self.failed {
            if self.observed < self.tickets.len() {
                return BuildEmissionDisposition::Pending;
            }
            return if self.emitted == 0 {
                BuildEmissionDisposition::NotEmitted
            } else {
                BuildEmissionDisposition::PartialEmission
            };
        }
        if self.unobserved || self.tickets.len() != self.required {
            return BuildEmissionDisposition::UnobservedTransport;
        }
        if self.observed == self.required && self.emitted == self.required {
            BuildEmissionDisposition::AllOriginatorDeliveriesEmitted
        } else {
            BuildEmissionDisposition::Pending
        }
    }

    fn poll(&mut self) {
        for slot in &mut self.tickets {
            let Some(ticket) = slot.as_mut() else {
                continue;
            };
            if let Some(result) = ticket.try_result() {
                if result {
                    self.emitted = self.emitted.saturating_add(1);
                } else {
                    self.failed = true;
                }
                self.observed = self.observed.saturating_add(1);
                *slot = None;
            }
        }
    }
}

struct RetainedBuildDeliverySet {
    set: BuildDeliverySet,
    expires_at_ms: u64,
}

/// Daemon-owned exploratory build coordinator.
pub struct ExploratoryBuildCoordinator {
    pool: ExploratoryPool,
    registry: DataPlaneRegistry,
    pending: BTreeMap<BuildAttemptId, PendingBuild>,
    settling_delivery_sets: BTreeMap<BuildAttemptId, RetainedBuildDeliverySet>,
    pending_tunnel_ids: HashSet<u32>,
    retired_tunnel_ids: BTreeMap<u32, u64>,
    retired_message_ids: BTreeMap<u32, u64>,
    build_id_scope_exhausted: bool,
    established_handoffs: BTreeMap<BuildAttemptId, i2pr_tunnel::EstablishedMaterial>,
    next_attempt_id: u64,
    next_creator_tunnel_id: u32,
    next_destination_role_slot: u32,
    counters: BuildCoordinatorCounters,
    failure_threshold: u16,
    lifetime_seconds: u32,
    paused: bool,
    /// Wall-clock source used to drive deadlines and snapshots.
    now_ms: u64,
}

impl ExploratoryBuildCoordinator {
    /// Constructs a new coordinator with bounded defaults and the
    /// supplied pool configuration.
    pub fn new(pool_config: ExploratoryPoolConfig) -> Self {
        let inbound = pool_config.max_inbound();
        let outbound = pool_config.max_outbound();
        let registry_capacity =
            DataPlaneCapacity::new(outbound.max(inbound), inbound.max(outbound));
        Self {
            pool: ExploratoryPool::new(pool_config),
            registry: DataPlaneRegistry::new(registry_capacity),
            pending: BTreeMap::new(),
            settling_delivery_sets: BTreeMap::new(),
            pending_tunnel_ids: HashSet::new(),
            retired_tunnel_ids: BTreeMap::new(),
            retired_message_ids: BTreeMap::new(),
            build_id_scope_exhausted: false,
            established_handoffs: BTreeMap::new(),
            next_attempt_id: INITIAL_ATTEMPT_ID,
            next_creator_tunnel_id: INITIAL_CREATOR_TUNNEL_ID,
            next_destination_role_slot: 0x8000_0000,
            counters: BuildCoordinatorCounters::default(),
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            lifetime_seconds: DEFAULT_EXPLORATORY_LIFETIME_SECONDS,
            paused: false,
            now_ms: 0,
        }
    }

    /// Constructs the product coordinator with a bounded shared role registry
    /// for the exploratory pool and service Destination ownership. Inbound
    /// service roles remain in the registry for routing until their owning
    /// Destination pool evicts them. Outbound service roles are moved to the
    /// bridge synchronously, so only one activation slot is reserved beyond
    /// the exploratory pool ceiling.
    pub fn new_with_service_destination_capacity(pool_config: ExploratoryPoolConfig) -> Self {
        let max_inbound = pool_config.max_inbound();
        let max_outbound = pool_config.max_outbound();
        let service_inbound = u16::try_from(i2pr_service_tunnels::MAX_SERVICE_TUNNELS)
            .expect("service tunnel maximum fits u16")
            .checked_mul(u16::from(
                i2pr_service_tunnels::MAX_EFFECTIVE_DIRECTION_TUNNELS,
            ))
            .expect("service inbound role maximum fits u16");
        let inbound_capacity = max_inbound
            .checked_add(service_inbound)
            .expect("combined inbound role maximum fits u16");
        let outbound_capacity = max_outbound
            .checked_add(1)
            .expect("outbound transient role headroom fits u16");
        let mut coordinator = Self::new(pool_config);
        coordinator.registry =
            DataPlaneRegistry::new(DataPlaneCapacity::new(outbound_capacity, inbound_capacity));
        coordinator
    }

    /// Creates the production coordinator with an OS-randomized tunnel-id
    /// generation. Deterministic callers should use `new` in test-owned code.
    pub(crate) fn try_new_with_service_destination_capacity(
        pool_config: ExploratoryPoolConfig,
    ) -> Result<Self, BuildCoordinatorError> {
        let mut rng = ChaCha8Rng::try_from_os_rng().map_err(|_| {
            BuildCoordinatorError::Coordinator("operating-system randomness unavailable")
        })?;
        let mut coordinator = Self::new_with_service_destination_capacity(pool_config);
        coordinator.next_creator_tunnel_id = rng.next_u32().max(1);
        Ok(coordinator)
    }

    /// Returns the configured failure threshold.
    pub const fn failure_threshold(&self) -> u16 {
        self.failure_threshold
    }

    /// Sets the failure threshold (must be > 0).
    pub fn set_failure_threshold(&mut self, threshold: u16) {
        self.failure_threshold = threshold.max(1);
    }

    /// Returns the configured lifetime in seconds.
    pub const fn lifetime_seconds(&self) -> u32 {
        self.lifetime_seconds
    }

    /// Sets the lifetime in seconds. The value must lie in
    /// `[1, MAX_EXPLORATORY_LIFETIME_SECONDS]`.
    pub fn set_lifetime_seconds(&mut self, seconds: u32) -> Result<(), BuildCoordinatorError> {
        if seconds == 0 || seconds > MAX_EXPLORATORY_LIFETIME_SECONDS {
            return Err(BuildCoordinatorError::Coordinator(
                "lifetime must be in [1, MAX_EXPLORATORY_LIFETIME_SECONDS]",
            ));
        }
        self.lifetime_seconds = seconds;
        Ok(())
    }

    /// Returns the bounded counters snapshot.
    pub fn counters(&self) -> BuildCoordinatorCounters {
        let mut snapshot = self.counters;
        snapshot.pending = self.pending.len();
        snapshot.consecutive_failures = self.pool.consecutive_failures();
        snapshot
    }

    /// Returns the message identifiers held by live pending attempts.
    /// Callers that allocate another terminal reply identifier must exclude
    /// this bounded set so two live builds cannot claim the same reply key.
    #[cfg(test)]
    pub(crate) fn pending_message_ids(&self) -> Vec<u32> {
        self.pending
            .values()
            .map(|attempt| attempt.message_id)
            .collect()
    }

    /// Returns every live or replay-retained message identifier that a new
    /// build or outer I2NP header must exclude.
    pub(crate) fn reserved_message_ids(&self) -> Vec<u32> {
        let mut ids = self
            .pending
            .values()
            .flat_map(|attempt| [attempt.message_id, attempt.outer_message_id])
            .collect::<Vec<_>>();
        ids.extend(self.retired_message_ids.keys().copied());
        ids
    }

    /// Returns tunnel IDs held by pending builds or still inside their
    /// active-tunnel/replay retention window.
    pub(crate) fn reserved_tunnel_ids(&self) -> Vec<u32> {
        let mut ids = self.pending_tunnel_ids.iter().copied().collect::<Vec<_>>();
        ids.extend(self.retired_tunnel_ids.keys().copied());
        ids
    }

    #[cfg(test)]
    fn has_pending_message_id(&self, message_id: u32) -> bool {
        self.pending
            .values()
            .any(|attempt| attempt.message_id == message_id)
    }

    /// Returns the count-only inbound correlation observations for one live
    /// attempt. The caller must already own the attempt identifier.
    pub fn inbound_build_observation(
        &self,
        attempt_id: BuildAttemptId,
    ) -> Option<InboundBuildObservation> {
        self.pending
            .get(&attempt_id)
            .map(|attempt| attempt.inbound_observation)
    }

    /// Returns whether the coordinator has paused new submissions
    /// because the consecutive failure threshold was reached.
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Returns the number of inbound tunnel slots currently held
    /// by the pool.
    pub fn inbound_pool_len(&self) -> usize {
        self.pool.inbound_len()
    }

    /// Returns the number of outbound tunnel slots currently held
    /// by the pool.
    pub fn outbound_pool_len(&self) -> usize {
        self.pool.outbound_len()
    }

    /// Takes the one-shot established material waiting for registration by
    /// its owning Destination runtime.
    pub fn take_destination_material(
        &mut self,
        attempt_id: BuildAttemptId,
    ) -> Option<i2pr_tunnel::EstablishedMaterial> {
        self.established_handoffs.remove(&attempt_id)
    }

    /// Activates a tunnel whose registration and secret material are owned by
    /// one Destination runtime. The returned role slot is a bounded router
    /// data-plane handle and is intentionally distinct from the pool slot.
    pub fn activate_destination_tunnel(
        &mut self,
        direction: BuildDirection,
        tunnel: i2pr_tunnel::EstablishedTunnel,
        now_seconds: u64,
    ) -> Result<TunnelSlot, BuildCoordinatorError> {
        let raw_slot = self.next_destination_role_slot;
        self.next_destination_role_slot =
            raw_slot
                .checked_add(1)
                .ok_or(BuildCoordinatorError::Coordinator(
                    "Destination role slot space exhausted",
                ))?;
        let slot = TunnelSlot::from_raw(raw_slot);
        let expires_at_ms = now_seconds
            .saturating_add(u64::from(self.lifetime_seconds))
            .saturating_mul(1000);
        match direction {
            BuildDirection::Outbound => self
                .registry
                .activate_outbound(slot, tunnel, expires_at_ms)
                .map(|_| ())
                .map_err(|_| {
                    BuildCoordinatorError::Coordinator(
                        "Destination outbound role registry rejected activation",
                    )
                })?,
            BuildDirection::Inbound => self
                .registry
                .activate_inbound(
                    slot,
                    tunnel,
                    REASSEMBLER_CAPACITY,
                    REASSEMBLER_AGGREGATE_BYTES,
                    REASSEMBLER_EXPIRY_MS,
                    now_seconds.saturating_mul(1000),
                    expires_at_ms,
                )
                .map(|_| ())
                .map_err(|_| {
                    BuildCoordinatorError::Coordinator(
                        "Destination inbound role registry rejected activation",
                    )
                })?,
        }
        self.counters.installed = self.counters.installed.saturating_add(1);
        Ok(slot)
    }

    /// Returns the bounded number of pending builds.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Returns the bounded number of pending builds for one group
    /// Destination.
    pub fn pending_destination_len(&self, destination_id: DestinationId) -> usize {
        self.pending
            .values()
            .filter(|pending| pending.destination_id == Some(destination_id))
            .count()
    }

    /// Returns the bounded number of pending builds for one group
    /// Destination and direction. Replenishment uses this alongside live
    /// registrations so overlapping ticks cannot overshoot either target.
    pub fn pending_destination_direction_len(
        &self,
        destination_id: DestinationId,
        direction: BuildDirection,
    ) -> usize {
        self.pending
            .values()
            .filter(|pending| {
                pending.destination_id == Some(destination_id) && pending.direction == direction
            })
            .count()
    }

    /// Cancels all pending attempts owned by one group Destination. The
    /// global table is capped, so this scan is bounded by
    /// `MAX_PENDING_BUILDS`.
    pub fn cancel_destination_builds(
        &mut self,
        destination_id: DestinationId,
    ) -> Vec<BuildCoordinatorOutcome> {
        let attempts: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(attempt_id, pending)| {
                (pending.destination_id == Some(destination_id)).then_some(*attempt_id)
            })
            .collect();
        attempts
            .into_iter()
            .filter_map(|attempt_id| self.cancel(attempt_id))
            .collect()
    }

    /// Returns a snapshot of the pool registrations for one
    /// direction (the pool owns the public metadata).
    pub fn registrations(&self, direction: TunnelDirection) -> Vec<TunnelRegistration> {
        match direction {
            TunnelDirection::Inbound => self.pool.inbound_registrations(),
            TunnelDirection::Outbound => self.pool.outbound_registrations(),
        }
    }

    /// Returns a snapshot of the registry's inbound-first-hop
    /// router hash for the supplied local receive tunnel id, if
    /// present.
    pub fn registry_inbound_first_hop(&self, receive: TunnelId) -> Option<Hash> {
        self.registry.inbound_first_hop(receive)
    }

    /// Returns the pool slot bound to the supplied local receive
    /// tunnel id, when present.
    pub fn registry_inbound_slot(&self, receive: TunnelId) -> Option<TunnelSlot> {
        self.registry.inbound_slot(receive)
    }

    /// Returns the registry's outbound-first-hop router hash and
    /// receive tunnel id bound to the supplied pool slot.
    pub fn registry_outbound_first_hop(&self, slot: TunnelSlot) -> Option<(Hash, TunnelId)> {
        self.registry.outbound_first_hop(slot)
    }

    /// Returns the bounded set of currently usable outbound role slots.
    pub fn usable_outbound_role_slots(&self) -> Vec<TunnelSlot> {
        self.registry.usable_outbound_slots(self.now_ms)
    }

    /// Returns the data-plane registry holding the installed tunnel
    /// roles. Plan 187 §6/§7 drives destination lookup/publication
    /// composition and inbound Garlic recovery through these real
    /// roles; the borrow is scoped to one composition or dispatch
    /// call so pool ownership never moves.
    pub fn registry(&self) -> &DataPlaneRegistry {
        &self.registry
    }

    /// Returns a mutable borrow of the data-plane registry for
    /// inbound TunnelData dispatch. Scoped like [`Self::registry`].
    pub fn registry_mut(&mut self) -> &mut DataPlaneRegistry {
        &mut self.registry
    }

    /// Advances the wall-clock view of the coordinator. Callers
    /// must call this before any operation that depends on time.
    pub fn advance_time(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        for pending in self.pending.values_mut() {
            pending.delivery_set.poll();
        }
        for retained in self.settling_delivery_sets.values_mut() {
            retained.set.poll();
        }
        self.settling_delivery_sets
            .retain(|_, retained| retained.expires_at_ms > now_ms);
        self.retired_tunnel_ids
            .retain(|_, expires_at| *expires_at > now_ms);
        self.retired_message_ids
            .retain(|_, expires_at| *expires_at > now_ms);
    }

    /// Returns the local originator-writer disposition for a live attempt.
    /// This reports only source-owned socket writes; reply correlation and
    /// remote protocol acceptance remain separate outcomes.
    pub fn emission_disposition(
        &self,
        attempt_id: BuildAttemptId,
    ) -> Option<BuildEmissionDisposition> {
        self.pending
            .get(&attempt_id)
            .map(|pending| pending.delivery_set.disposition())
            .or_else(|| {
                self.settling_delivery_sets
                    .get(&attempt_id)
                    .map(|retained| retained.set.disposition())
            })
    }

    /// Allocates a fresh attempt id. The id is monotonic and never
    /// reused across the coordinator lifetime.
    pub fn next_attempt_id(&mut self) -> BuildAttemptId {
        let value = self.next_attempt_id;
        self.next_attempt_id = self
            .next_attempt_id
            .checked_add(1)
            .expect("attempt id monotonic counter exhausted");
        BuildAttemptId::new(value)
    }

    /// Allocates a fresh creator tunnel id. The id is monotonic
    /// and never reused across the coordinator lifetime.
    fn next_creator_tunnel_id_value(&mut self) -> Result<u32, BuildCoordinatorError> {
        for _ in 0..=MAX_RETAINED_BUILD_IDS {
            let value = self.next_creator_tunnel_id;
            self.next_creator_tunnel_id = self.next_creator_tunnel_id.checked_add(1).ok_or(
                BuildCoordinatorError::Coordinator("creator tunnel id monotonic counter exhausted"),
            )?;
            if value != 0
                && !self.pending_tunnel_ids.contains(&value)
                && !self.retired_tunnel_ids.contains_key(&value)
            {
                return Ok(value);
            }
        }
        Err(BuildCoordinatorError::Coordinator(
            "bounded creator tunnel id collision search exhausted",
        ))
    }

    fn retire_pending_ids(&mut self, pending: &PendingBuild) {
        let message_expiry = self
            .now_ms
            .saturating_add(BUILD_MESSAGE_REPLAY_RETENTION_MS);
        let tunnel_expiry = self.now_ms.saturating_add(BUILD_TUNNEL_ID_RETENTION_MS);
        self.pending_tunnel_ids
            .retain(|id| !pending.reserved_tunnel_ids.contains(id));
        let new_tunnel_ids = pending
            .reserved_tunnel_ids
            .iter()
            .filter(|id| !self.retired_tunnel_ids.contains_key(id))
            .count();
        let new_message_ids = [pending.message_id, pending.outer_message_id]
            .into_iter()
            .filter(|id| !self.retired_message_ids.contains_key(id))
            .count();
        if self.retired_tunnel_ids.len().saturating_add(new_tunnel_ids) > MAX_RETAINED_BUILD_IDS
            || self
                .retired_message_ids
                .len()
                .saturating_add(new_message_ids)
                > MAX_RETAINED_BUILD_IDS
        {
            self.build_id_scope_exhausted = true;
            return;
        }
        for id in &pending.reserved_tunnel_ids {
            self.retired_tunnel_ids.insert(*id, tunnel_expiry);
        }
        self.retired_message_ids
            .insert(pending.message_id, message_expiry);
        self.retired_message_ids
            .insert(pending.outer_message_id, message_expiry);
    }

    /// Submits one build request through the canonical production
    /// I2NP bridge and the narrow authenticated outbound seam. The
    /// function does **not** spawn a task; it returns either a
    /// typed outcome (synchronous failure path) or the registered
    /// attempt id for the daemon pump to track.
    ///
    /// The caller is responsible for routing inbound dispatched
    /// outcomes back through [`Self::route_inbound_i2np`].
    pub fn submit<R>(
        &mut self,
        request: BuildRequest,
        delivery: &RouterDeliveryService,
        bridge: &ShortBuildI2npBridge,
        bridge_header: BridgeHeader,
        rng: &mut R,
    ) -> Result<SubmitResult, BuildCoordinatorError>
    where
        R: CryptoRng + RngCore,
    {
        self.submit_path(request.into(), delivery, bridge, bridge_header, None, rng)
    }

    /// Submits an exact-three-hop Destination request through the same shared
    /// short-build engine as exploratory requests.
    pub fn submit_destination<R>(
        &mut self,
        request: DestinationBuildRequest,
        delivery: &RouterDeliveryService,
        bridge: &ShortBuildI2npBridge,
        bridge_header: BridgeHeader,
        rng: &mut R,
    ) -> Result<SubmitResult, BuildCoordinatorError>
    where
        R: CryptoRng + RngCore,
    {
        if request.peers.len() != crate::destination_peers::QUALIFIED_DESTINATION_HOPS {
            return Err(BuildCoordinatorError::InvalidPath {
                reason: "qualified Destination path must contain exactly three hops",
            });
        }
        self.submit_path(request.into(), delivery, bridge, bridge_header, None, rng)
    }

    /// Submits a service Destination build through an already activated
    /// outbound tunnel, targeting the selected first build hop at the tunnel
    /// endpoint. Bootstrap callers continue to use `submit_destination`.
    pub fn submit_destination_via_outbound_tunnel<R>(
        &mut self,
        request: DestinationBuildRequest,
        delivery: &RouterDeliveryService,
        bridge: &ShortBuildI2npBridge,
        bridge_header: BridgeHeader,
        outbound_slot: TunnelSlot,
        rng: &mut R,
    ) -> Result<SubmitResult, BuildCoordinatorError>
    where
        R: CryptoRng + RngCore,
    {
        if request.peers.len() != crate::destination_peers::QUALIFIED_DESTINATION_HOPS {
            return Err(BuildCoordinatorError::Coordinator(
                "outbound-tunnel build delivery requires a qualified service Destination path",
            ));
        }
        self.submit_path(
            request.into(),
            delivery,
            bridge,
            bridge_header,
            Some(outbound_slot),
            rng,
        )
    }

    fn submit_path<R>(
        &mut self,
        request: BuildPathRequest,
        delivery: &RouterDeliveryService,
        bridge: &ShortBuildI2npBridge,
        bridge_header: BridgeHeader,
        outbound_slot: Option<TunnelSlot>,
        rng: &mut R,
    ) -> Result<SubmitResult, BuildCoordinatorError>
    where
        R: CryptoRng + RngCore,
    {
        if request.destination_id.is_some()
            && self.established_handoffs.len() >= MAX_DESTINATION_HANDOFFS
        {
            return Err(BuildCoordinatorError::Coordinator(
                "Destination material handoff table is full",
            ));
        }
        if self.paused {
            return Err(BuildCoordinatorError::Coordinator(
                "coordinator is paused after consecutive failures",
            ));
        }
        if self
            .pending
            .len()
            .saturating_add(self.settling_delivery_sets.len())
            >= MAX_PENDING_BUILDS
        {
            return Err(BuildCoordinatorError::Coordinator(
                "too many pending builds",
            ));
        }
        if self.build_id_scope_exhausted {
            return Err(BuildCoordinatorError::Coordinator(
                "retained build identifier registry is exhausted",
            ));
        }
        if request.peers.is_empty()
            || request.peers.iter().any(|peer| {
                peer.router_hash.as_bytes().iter().all(|byte| *byte == 0)
                    || peer.static_encryption_key.iter().all(|byte| *byte == 0)
            })
        {
            return Err(BuildCoordinatorError::Coordinator(
                "build path contains invalid peer material",
            ));
        }
        if request.message_id == 0 {
            return Err(BuildCoordinatorError::Coordinator("message id is zero"));
        }
        let outer_message_id = match bridge_header {
            BridgeHeader::Standard { message_id, .. }
            | BridgeHeader::ShortTransport { message_id, .. } => message_id,
        };
        if outer_message_id == 0 || outer_message_id == request.message_id {
            return Err(BuildCoordinatorError::Coordinator(
                "outer and terminal message ids must be distinct and nonzero",
            ));
        }
        if self.reserved_message_ids().contains(&request.message_id)
            || self.reserved_message_ids().contains(&outer_message_id)
        {
            return Err(BuildCoordinatorError::Coordinator(
                "message id is pending or retained for replay protection",
            ));
        }
        let reserved_tunnel_ids = reserved_path_tunnel_ids(&request)?;
        if reserved_tunnel_ids.iter().any(|id| {
            self.pending_tunnel_ids.contains(id) || self.retired_tunnel_ids.contains_key(id)
        }) {
            return Err(BuildCoordinatorError::Coordinator(
                "tunnel id is pending, active, retiring, or replay-retained",
            ));
        }
        if self
            .pending_tunnel_ids
            .len()
            .saturating_add(self.retired_tunnel_ids.len())
            .saturating_add(reserved_tunnel_ids.len())
            > MAX_RETAINED_BUILD_IDS
            || self
                .pending
                .len()
                .saturating_mul(2)
                .saturating_add(self.retired_message_ids.len())
                .saturating_add(2)
                > MAX_RETAINED_BUILD_IDS
        {
            return Err(BuildCoordinatorError::Coordinator(
                "bounded build identifier registry is full",
            ));
        }
        if self.now_ms == 0 {
            return Err(BuildCoordinatorError::Coordinator(
                "advance_time not called",
            ));
        }
        let direction = request.direction;
        let now_ms = self.now_ms;
        let attempt_id = self.next_attempt_id();
        tracing::info!(
            direction = ?direction,
            transition = "selected",
            "destination build attempt"
        );
        let path = short_build_path(&request, BuildAttemptId::new(attempt_id.get()), now_ms);
        path.validate().map_err(map_construction_error)?;
        let deadline_ms = now_ms.saturating_add(BUILD_DEADLINE_MS);
        let mut state = ShortBuildStateMachine::with_cryptography(
            path,
            deadline_ms,
            i2pr_tunnel::build_crypto::EciesX25519BuildCryptography::new(),
        );
        let message = state.prepare(rng).map_err(map_construction_error)?;
        let action: ShortBuildAction = state
            .deliver_action(message)
            .map_err(map_construction_error)?;
        // The inner build wire is framed per delivery branch
        // below: the direct bootstrap exception keeps the short
        // framing the pinned reference dispatches from complete
        // SSU2 blocks, while a tunneled inner must be standard
        // framed. The stock tunnel endpoint checks the inner
        // `IsExpired` at the standard millisecond offset before
        // routing; a short inner reads its seconds expiry plus
        // record bytes as far-future milliseconds and is dropped
        // as `TunnelMessage: Message expired` at the OBEP after
        // traversing the whole outbound path.
        let first_peer_hash = request.peers.first().map(|peer| peer.router_hash).ok_or(
            BuildCoordinatorError::Coordinator("build path has no first hop"),
        )?;
        let peer = PeerId::from_hash(first_peer_hash);
        let mut delivery_set = BuildDeliverySet::default();
        // Redacted family of the selected transports, so a tunneled send
        // that never reaches stock can be attributed to the
        // SSU2-first/NTCP2-fallback seam without naming peers.
        // Ephemeral selected-link generations (never peer identities),
        // so a socket-emitted send that never reaches stock can be
        // checked against session rotation without naming peers.
        let mut delivery_links: Vec<u64> = Vec::new();
        let mut delivery_family: Option<&'static str> = None;
        // Redacted per-cell accounting: wire lengths and fresh
        // transport message IDs (ephemeral randoms, same class
        // as link generations) so a tunneled send that never
        // reaches stock can be checked cell-by-cell against
        // stock-side fragment/expiry counters.
        let mut cell_wire_lens: Vec<usize> = Vec::new();
        let mut cell_message_ids: Vec<u32> = Vec::new();
        let delivery_outcome = if let Some(slot) = outbound_slot {
            let expiration_ms = match bridge_header {
                BridgeHeader::Standard { expiration_ms, .. } => expiration_ms,
                BridgeHeader::ShortTransport {
                    expiration_seconds, ..
                } => u64::from(expiration_seconds).saturating_mul(1000),
            };
            let outbound_role =
                self.registry
                    .outbound(slot)
                    .ok_or(BuildCoordinatorError::Coordinator(
                        "selected outbound tunnel is unavailable",
                    ))?;
            // Same identifiers and instant as the direct framing,
            // standard envelope so the stock OBEP expiry check
            // passes on the tunneled inner.
            let wire = wrap_inner_wire(
                bridge,
                &action,
                BridgeHeader::Standard {
                    message_id: outer_message_id,
                    expiration_ms,
                },
            )?;
            let cells = compose_outbound_build_deliveries(
                outbound_role,
                first_peer_hash,
                outer_message_id,
                expiration_ms,
                &wire,
                now_ms,
                rng,
            )?;
            delivery_set.required = cells.len();
            let mut result = RouterDeliveryOutcome::Accepted;
            for (target_peer, transport_wire, cell_message_id) in cells {
                cell_wire_lens.push(transport_wire.len());
                cell_message_ids.push(cell_message_id);
                let delivery_request =
                    RouterDeliveryRequest::new(target_peer, transport_wire, BUILD_DELIVERY_TIMEOUT)
                        .map_err(|_| {
                            BuildCoordinatorError::Coordinator("delivery request validation failed")
                        })?;
                let (cell_result, selected, ticket) = delivery
                    .deliver_with_ticket(delivery_request, &i2pr_runtime::CancellationToken::new());
                if let Some(transport) = selected {
                    let family = transport.as_str();
                    delivery_family = Some(match delivery_family {
                        None => family,
                        Some(previous) if previous == family => family,
                        Some(_) => "mixed",
                    });
                    let generation = match transport {
                        RouterDeliveryTransport::Ssu2(link)
                        | RouterDeliveryTransport::Ntcp2(link) => link.value(),
                    };
                    if delivery_links.len() < 8 {
                        delivery_links.push(generation);
                    }
                }
                delivery_set.unobserved |= ticket.is_none();
                delivery_set.tickets.push(ticket);
                result = cell_result;
                if !matches!(result, RouterDeliveryOutcome::Accepted) {
                    delivery_set.failed = true;
                    break;
                }
            }
            result
        } else {
            // Direct bootstrap exception keeps the caller framing.
            let wire = wrap_inner_wire(bridge, &action, bridge_header)?;
            let wire_len = wire.len();
            let delivery_request = RouterDeliveryRequest::new(peer, wire, BUILD_DELIVERY_TIMEOUT)
                .map_err(|_| {
                BuildCoordinatorError::Coordinator("delivery request validation failed")
            })?;
            delivery_set.required = 1;
            let (result, selected, ticket) = delivery
                .deliver_with_ticket(delivery_request, &i2pr_runtime::CancellationToken::new());
            // Direct bootstrap exception: one cell carrying the outer
            // message ID, recorded with the same redacted accounting.
            cell_wire_lens.push(wire_len);
            cell_message_ids.push(outer_message_id);
            if let Some(transport) = selected {
                delivery_family = Some(transport.as_str());
                let generation = match transport {
                    RouterDeliveryTransport::Ssu2(link) | RouterDeliveryTransport::Ntcp2(link) => {
                        link.value()
                    }
                };
                delivery_links.push(generation);
            }
            delivery_set.unobserved = ticket.is_none();
            delivery_set.tickets.push(ticket);
            if !matches!(result, RouterDeliveryOutcome::Accepted) {
                delivery_set.failed = true;
            }
            result
        };
        let accepted = matches!(delivery_outcome, RouterDeliveryOutcome::Accepted);
        tracing::info!(
            direction = ?direction,
            delivery_family = delivery_family.unwrap_or("unobserved"),
            delivery_links = ?delivery_links,
            cell_count = cell_wire_lens.len(),
            cell_wire_lens = ?cell_wire_lens,
            cell_message_ids = ?cell_message_ids,
            transition = if accepted { "delivery_admitted" } else { "delivery_rejected" },
            "destination build attempt"
        );
        self.counters.outbound_builds = self.counters.outbound_builds.saturating_add(1);
        if !accepted {
            if !delivery_set.tickets.is_empty()
                && self.settling_delivery_sets.len() < MAX_PENDING_BUILDS
            {
                self.settling_delivery_sets.insert(
                    attempt_id,
                    RetainedBuildDeliverySet {
                        set: delivery_set,
                        expires_at_ms: now_ms.saturating_add(BUILD_DEADLINE_MS),
                    },
                );
            }
            self.counters.delivery_failures = self.counters.delivery_failures.saturating_add(1);
            if request.destination_id.is_none() {
                self.record_failure();
            }
            return Ok(SubmitResult::Rejected {
                direction,
                attempt_id,
                delivery: delivery_outcome,
            });
        }
        state.mark_dispatched().map_err(map_construction_error)?;
        // Plan 381: the inbound reply arrives DIRECTLY from the
        // terminal hop, which forwards the sealed records to the
        // creator (`originator_hash` = us) — not from the first hop
        // we sent to. Correlating on the first hop orphans every
        // inbound reply (observed live: all three references created
        // their hops, the terminal forwarded to us, and the 30s wait
        // saw nothing). Both directions correlate on the terminal hop.
        let reply_peer_hash = match direction {
            BuildDirection::Inbound => request.peers.last().map(|peer| peer.router_hash).ok_or(
                BuildCoordinatorError::Coordinator("build path has no terminal hop"),
            )?,
            BuildDirection::Outbound => request.peers.last().map(|peer| peer.router_hash).ok_or(
                BuildCoordinatorError::Coordinator("build path has no terminal hop"),
            )?,
        };
        let target_peer = PeerId::from_hash(reply_peer_hash);
        // Plan 188: retain the OBEP Garlic material for garlic-wrapped
        // endpoint replies. The state machine owns the authoritative
        // copy; this is the coordinator's correlation copy for
        // TunnelGateway unwrapping. Inbound builds never garlic-wrap.
        let (garlic_key, garlic_tag) = match state.obep_garlic_material() {
            Some((key, tag)) => (Some(Zeroizing::new(key)), Some(tag)),
            None => (None, None),
        };
        let pending = PendingBuild {
            attempt_id,
            direction,
            destination_id: request.destination_id,
            target_peer,
            deadline_ms,
            message_id: request.message_id,
            outer_message_id,
            reserved_tunnel_ids: reserved_tunnel_ids.into_iter().collect(),
            next_tunnel: request.peers.last().map(|peer| peer.next_tunnel).ok_or(
                BuildCoordinatorError::Coordinator("build path has no terminal hop"),
            )?,
            garlic_key,
            garlic_tag,
            inbound_observation: InboundBuildObservation::default(),
            delivery_set,
            state,
        };
        self.pending_tunnel_ids
            .extend(pending.reserved_tunnel_ids.iter().copied());
        let _ = self.pending.insert(attempt_id, pending);
        Ok(SubmitResult::Submitted {
            direction,
            attempt_id,
        })
    }

    /// Routes one inbound authenticated router-I2NP message
    /// through the central Plan 184 dispatcher and the build
    /// coordinator's pending table. The function preserves the
    /// typed dispatcher outcomes for non-build replies so the
    /// caller can drive NetDB / control surfaces in later plans.
    ///
    /// Plan 188 extends the build pipeline beyond direct
    /// `OutboundTunnelBuildReply`:
    /// - `ShortTunnelBuild` with a matching `(peer, message_id)` for
    ///   a pending inbound attempt is the reference-forwarded
    ///   inbound build (i2pd gateway forwards the same
    ///   `1 + count*218` records after sealing its own reply);
    /// - `TunnelGateway` (dispatcher `Unsupported` type 19) addressed
    ///   to a pending outbound `next_tunnel` carries the
    ///   garlic-wrapped endpoint reply; the OBEP
    ///   `RGarlicKeyAndTag` unwraps it to the inner OTBRM.
    pub fn route_inbound_i2np(
        &mut self,
        inbound: &Ssu2InboundI2np,
        now_ms: u64,
    ) -> Result<InboundRouteOutcome, RouterI2npError> {
        let outcome = dispatch_router_i2np(inbound, now_ms)?;
        Ok(self.route_inbound_i2np_classified(inbound, outcome))
    }

    /// Routes the coordinator portion of an already decoded authenticated
    /// message without running another I2NP decoder.
    pub(crate) fn route_inbound_i2np_classified(
        &mut self,
        inbound: &Ssu2InboundI2np,
        outcome: RouterI2npOutcome,
    ) -> InboundRouteOutcome {
        let coordinator_outcomes = match outcome {
            RouterI2npOutcome::TunnelBuildReserved {
                kind,
                message_id,
                peer,
                ..
            } => self.route_build_outcome(kind, message_id, peer, inbound.bytes.as_slice()),
            RouterI2npOutcome::Unsupported {
                type_byte: 19,
                peer,
                ..
            } => self.route_tunnel_gateway_reply(peer, inbound.bytes.as_slice()),
            _ => Vec::new(),
        };
        InboundRouteOutcome {
            dispatcher: outcome,
            coordinator: coordinator_outcomes,
        }
    }

    fn route_build_outcome(
        &mut self,
        kind: RouterI2npKind,
        message_id: u32,
        peer: PeerId,
        raw_bytes: &[u8],
    ) -> Vec<BuildCoordinatorOutcome> {
        match kind {
            RouterI2npKind::OutboundTunnelBuildReply => {
                self.route_direct_reply(peer, message_id, raw_bytes)
            }
            RouterI2npKind::ShortTunnelBuild => {
                self.route_forwarded_inbound_build(peer, message_id, raw_bytes)
            }
            _ => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                Vec::new()
            }
        }
    }

    /// Direct `OutboundTunnelBuildReply` path (synthetic responder
    /// and any future direct reference reply). Preserves the Plan
    /// 185 strict decoder and `(peer, message_id)` correlation.
    fn route_direct_reply(
        &mut self,
        peer: PeerId,
        message_id: u32,
        raw_bytes: &[u8],
    ) -> Vec<BuildCoordinatorOutcome> {
        let attempt_id = self.pending.iter().find_map(|(id, attempt)| {
            if attempt.target_peer == peer && attempt.message_id == message_id {
                Some(*id)
            } else {
                None
            }
        });
        let Some(attempt_id) = attempt_id else {
            self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
            return Vec::new();
        };
        tracing::info!(
            build_attempt_id = attempt_id.get(),
            direction = "outbound",
            transition = "authenticated_reply_matched",
            "destination build attempt"
        );
        let reply_payload = match extract_reply_payload(raw_bytes) {
            Ok(payload) => payload,
            Err(reason) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                let direction = self.pending.get(&attempt_id).map(|p| p.direction);
                if let Some(direction) = direction {
                    return vec![BuildCoordinatorOutcome::InvalidReply {
                        attempt_id,
                        direction,
                        reason,
                    }];
                }
                return Vec::new();
            }
        };
        self.drive_attempt_to_terminal(attempt_id, reply_payload)
    }

    /// Forwarded inbound build path: i2pd IBGW seals its own reply
    /// in place and forwards the same `ShortTunnelBuild` records to
    /// the creator (`next_router` = us). The payload shape is
    /// identical to OTBRM (`1 + count*218`), so the existing state
    /// machine postprocessor authenticates it unchanged.
    ///
    /// Only pending inbound attempts with matching `(peer,
    /// message_id)` are eligible. Unmatched `ShortTunnelBuild`
    /// arrivals are the reference's own builds through us (its only
    /// peer); they are counted as orphans, never as invalid
    /// replies, and never install.
    fn route_forwarded_inbound_build(
        &mut self,
        peer: PeerId,
        message_id: u32,
        raw_bytes: &[u8],
    ) -> Vec<BuildCoordinatorOutcome> {
        let attempt_id = self.pending.iter().find_map(|(id, attempt)| {
            if matches!(attempt.direction, BuildDirection::Inbound)
                && attempt.target_peer == peer
                && attempt.message_id == message_id
            {
                Some(*id)
            } else {
                None
            }
        });
        let Some(attempt_id) = attempt_id else {
            self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
            let inbound_attempts = self
                .pending
                .values_mut()
                .filter(|attempt| matches!(attempt.direction, BuildDirection::Inbound));
            let mut peer_match = false;
            let mut message_match = false;
            for attempt in inbound_attempts {
                let attempt_peer_match = attempt.target_peer == peer;
                attempt.inbound_observation.record(
                    attempt_peer_match,
                    message_id,
                    attempt.message_id,
                );
                peer_match |= attempt_peer_match;
                message_match |= attempt.message_id == message_id;
                if attempt.target_peer == peer && attempt.message_id != message_id {
                    match message_id.wrapping_sub(attempt.message_id) {
                        1 => {
                            self.counters.inbound_orphans_peer_id_plus_one = self
                                .counters
                                .inbound_orphans_peer_id_plus_one
                                .saturating_add(1);
                        }
                        u32::MAX => {
                            self.counters.inbound_orphans_peer_id_minus_one = self
                                .counters
                                .inbound_orphans_peer_id_minus_one
                                .saturating_add(1);
                        }
                        _ => {
                            self.counters.inbound_orphans_peer_id_other = self
                                .counters
                                .inbound_orphans_peer_id_other
                                .saturating_add(1);
                        }
                    }
                }
            }
            if peer_match {
                self.counters.inbound_orphans_peer_match =
                    self.counters.inbound_orphans_peer_match.saturating_add(1);
            }
            if message_match {
                self.counters.inbound_orphans_message_match = self
                    .counters
                    .inbound_orphans_message_match
                    .saturating_add(1);
            }
            return Vec::new();
        };
        tracing::info!(
            build_attempt_id = attempt_id.get(),
            direction = "inbound",
            transition = "authenticated_reply_matched",
            "destination build attempt"
        );
        let reply_payload = match extract_forwarded_build_payload(raw_bytes) {
            Ok(payload) => payload,
            Err(reason) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                let direction = self.pending.get(&attempt_id).map(|p| p.direction);
                if let Some(direction) = direction {
                    return vec![BuildCoordinatorOutcome::InvalidReply {
                        attempt_id,
                        direction,
                        reason,
                    }];
                }
                return Vec::new();
            }
        };
        self.drive_attempt_to_terminal(attempt_id, reply_payload)
    }

    /// Garlic-wrapped outbound endpoint reply path: i2pd OBEP sends
    /// `TunnelGateway(next_tunnel, Garlic(RGarlicKeyAndTag(
    /// ShortTunnelBuildReply)))` directly to the creator. The outer
    /// `TunnelGateway` header message id is unrelated to the build;
    /// correlation is by `(peer, tunnel_id == next_tunnel)` plus the
    /// inner reply `message_id` after unwrap.
    fn route_tunnel_gateway_reply(
        &mut self,
        peer: PeerId,
        raw_bytes: &[u8],
    ) -> Vec<BuildCoordinatorOutcome> {
        self.counters.inbound_gateway_messages =
            self.counters.inbound_gateway_messages.saturating_add(1);
        let (tunnel_id, garlic_opaque) = match extract_tunnel_gateway_garlic(raw_bytes) {
            Some(value) => value,
            None => {
                self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
                return Vec::new();
            }
        };
        let inbound_peer_match = self.pending.values().any(|attempt| {
            matches!(attempt.direction, BuildDirection::Inbound) && attempt.target_peer == peer
        });
        let inbound_route_match = self.pending.values().any(|attempt| {
            matches!(attempt.direction, BuildDirection::Inbound)
                && attempt.target_peer == peer
                && attempt.next_tunnel.get() == tunnel_id
        });
        if inbound_peer_match {
            self.counters.inbound_gateway_peer_match =
                self.counters.inbound_gateway_peer_match.saturating_add(1);
        }
        if inbound_route_match {
            self.counters.inbound_gateway_route_match =
                self.counters.inbound_gateway_route_match.saturating_add(1);
        }
        // Find the pending outbound attempt bound to this tunnel.
        // At most `MAX_PENDING_BUILDS` entries; linear scan is
        // bounded and avoids a second index.
        let attempt_id = self.pending.iter().find_map(|(id, attempt)| {
            if matches!(attempt.direction, BuildDirection::Outbound)
                && attempt.target_peer == peer
                && attempt.next_tunnel.get() == tunnel_id
                && attempt.garlic_key.is_some()
                && attempt.garlic_tag.is_some()
            {
                Some(*id)
            } else {
                None
            }
        });
        let Some(attempt_id) = attempt_id else {
            self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
            return Vec::new();
        };
        tracing::info!(
            build_attempt_id = attempt_id.get(),
            direction = "outbound",
            transition = "authenticated_reply_matched",
            "destination build attempt"
        );
        let (key, tag) = match self
            .pending
            .get(&attempt_id)
            .and_then(|p| p.garlic_key.as_ref().map(|k| (**k, p.garlic_tag)))
        {
            Some((key, Some(tag))) => (key, tag),
            _ => {
                self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
                return Vec::new();
            }
        };
        let decrypted = match decrypt_build_reply_garlic(&key, &tag, &garlic_opaque) {
            Ok(value) => value,
            Err(_) => {
                // Tag mismatch means this Gateway is not for the
                // matched attempt (or is unrelated Garlic); keep the
                // attempt for its deadline rather than failing it.
                // Authentication failure on a tunnel-matched Gateway
                // is a real reply failure: report InvalidReply but
                // retain the pending entry for timeout accounting,
                // mirroring the direct-reply extract-failure path.
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                let direction = self.pending.get(&attempt_id).map(|p| p.direction);
                if let Some(direction) = direction {
                    return vec![BuildCoordinatorOutcome::InvalidReply {
                        attempt_id,
                        direction,
                        reason: "garlic build-reply unwrap failed",
                    }];
                }
                return Vec::new();
            }
        };
        // Inner message-id must equal the attempt's SEND_MSG_ID;
        // otherwise this Gateway is not the reply to this build.
        let expected = self
            .pending
            .get(&attempt_id)
            .map(|p| p.message_id)
            .unwrap_or(0);
        if decrypted.inner_message_id != expected {
            self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
            return Vec::new();
        }
        self.drive_attempt_to_terminal(attempt_id, decrypted.reply_payload)
    }

    /// Removes the pending attempt and drives its state machine with
    /// the recovered reply payload to a terminal outcome.
    fn drive_attempt_to_terminal(
        &mut self,
        attempt_id: BuildAttemptId,
        reply_payload: Vec<u8>,
    ) -> Vec<BuildCoordinatorOutcome> {
        self.counters.inbound_routed = self.counters.inbound_routed.saturating_add(1);
        let pending = match self.pending.remove(&attempt_id) {
            Some(value) => value,
            None => return Vec::new(),
        };
        self.retire_pending_ids(&pending);
        let mut pending = pending;
        let destination_id = pending.destination_id;
        let direction = pending.direction;
        let event_result = pending.state.handle_event(BuildEvent::BuildReply {
            reply: Zeroizing::new(reply_payload),
        });
        let outcome = match event_result {
            Ok(Some(outcome)) => outcome,
            Ok(None) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                return scope_destination_terminal(
                    destination_id,
                    direction,
                    vec![BuildCoordinatorOutcome::InvalidReply {
                        attempt_id,
                        direction,
                        reason: "state machine did not reach terminal",
                    }],
                );
            }
            Err(_) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                return scope_destination_terminal(
                    destination_id,
                    direction,
                    vec![BuildCoordinatorOutcome::InvalidReply {
                        attempt_id,
                        direction,
                        reason: "state machine rejected the reply",
                    }],
                );
            }
        };
        scope_destination_terminal(
            destination_id,
            direction,
            self.finalize_attempt(pending, outcome),
        )
    }

    fn finalize_attempt(
        &mut self,
        mut pending: PendingBuild,
        outcome: ShortBuildOutcome,
    ) -> Vec<BuildCoordinatorOutcome> {
        match outcome {
            ShortBuildOutcome::Established { .. } => {
                let established_at_seconds = self.now_ms / 1000;
                let material = match pending
                    .state
                    .take_established_material(established_at_seconds)
                {
                    Ok(material) => material,
                    Err(_) => {
                        self.counters.invalid_replies =
                            self.counters.invalid_replies.saturating_add(1);
                        return vec![BuildCoordinatorOutcome::InvalidReply {
                            attempt_id: pending.attempt_id,
                            direction: pending.direction,
                            reason: "established material could not be extracted",
                        }];
                    }
                };
                self.install_established(pending, material)
            }
            ShortBuildOutcome::HopRejected {
                hop_index,
                reply_code,
            } => {
                self.counters.hop_rejections = self.counters.hop_rejections.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                vec![BuildCoordinatorOutcome::HopRejected {
                    direction: pending.direction,
                    hop_index,
                    response_code: reply_code.byte(),
                }]
            }
            ShortBuildOutcome::TimedOut => {
                self.counters.timeouts = self.counters.timeouts.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                vec![BuildCoordinatorOutcome::TimedOut {
                    direction: pending.direction,
                }]
            }
            ShortBuildOutcome::Cancelled => {
                self.counters.cancellations = self.counters.cancellations.saturating_add(1);
                vec![BuildCoordinatorOutcome::Cancelled {
                    direction: pending.direction,
                }]
            }
            ShortBuildOutcome::InvalidReply => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                vec![BuildCoordinatorOutcome::InvalidReply {
                    attempt_id: pending.attempt_id,
                    direction: pending.direction,
                    reason: "multi-record reply rejected",
                }]
            }
            ShortBuildOutcome::CryptoFailed => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                vec![BuildCoordinatorOutcome::InvalidReply {
                    attempt_id: pending.attempt_id,
                    direction: pending.direction,
                    reason: "cryptography primitive failed",
                }]
            }
            ShortBuildOutcome::DeliveryFailed => {
                self.counters.delivery_failures = self.counters.delivery_failures.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                vec![BuildCoordinatorOutcome::DeliveryFailed {
                    direction: pending.direction,
                    delivery: RouterDeliveryOutcome::Cancelled,
                }]
            }
        }
    }

    fn install_established(
        &mut self,
        pending: PendingBuild,
        material: i2pr_tunnel::established::EstablishedMaterial,
    ) -> Vec<BuildCoordinatorOutcome> {
        let now_seconds = self.now_ms / 1000;
        let direction = pending.direction;
        let hop_count = material.hops().len();
        if let Some(destination_id) = pending.destination_id {
            if self.established_handoffs.len() >= MAX_DESTINATION_HANDOFFS {
                self.record_attempt_failure(pending.destination_id);
                return vec![BuildCoordinatorOutcome::DestinationHandoffRejected { direction }];
            }
            let attempt_id = pending.attempt_id;
            self.established_handoffs.insert(attempt_id, material);
            return vec![BuildCoordinatorOutcome::DestinationEstablished {
                attempt_id,
                destination_id,
                hop_count,
                direction,
            }];
        }
        let outcome = match direction {
            BuildDirection::Inbound => self
                .pool
                .register_inbound_with_material(material, now_seconds),
            BuildDirection::Outbound => self
                .pool
                .register_outbound_with_material(material, now_seconds),
        };
        let slot = match outcome {
            Ok(RegisterOutcome::Inserted { slot, .. }) => slot,
            Ok(RegisterOutcome::Duplicate { slot }) => slot,
            Err(RegisterError::Full { kind, .. }) => {
                self.counters.delivery_failures = self.counters.delivery_failures.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                return vec![BuildCoordinatorOutcome::PoolRejected { direction, kind }];
            }
            Err(RegisterError::Invalid(error)) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                return vec![BuildCoordinatorOutcome::InvalidRegistration {
                    direction,
                    reason: registration_error_reason(error),
                }];
            }
        };
        let registration = match self.pool.registration(slot) {
            Some(reg) => reg.clone(),
            None => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                return vec![BuildCoordinatorOutcome::InvalidRegistration {
                    direction,
                    reason: "pool returned a slot without a registration",
                }];
            }
        };
        self.counters.installed = self.counters.installed.saturating_add(1);
        self.pool
            .mark_established(slot)
            .expect("slot was just inserted");
        self.paused = false;
        // The pool now owns the EstablishedMaterial; we activate
        // the role in the registry by extracting the tunnel through
        // the canonical activation seam. The pool entry keeps the
        // public metadata so reply-path selection continues to
        // work after activation.
        let _ = direction;
        let _ = pending;
        let tunnel = match self.pool.activate(slot) {
            Ok(tunnel) => tunnel,
            Err(error) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                return vec![BuildCoordinatorOutcome::InvalidRegistration {
                    direction,
                    reason: registry_activation_reason(error),
                }];
            }
        };
        self.activate_registry(direction, slot, tunnel, now_seconds);
        vec![BuildCoordinatorOutcome::Installed {
            slot,
            registration,
            hop_count,
            direction,
        }]
    }

    fn activate_registry(
        &mut self,
        direction: BuildDirection,
        slot: TunnelSlot,
        tunnel: i2pr_tunnel::established::EstablishedTunnel,
        now_seconds: u64,
    ) {
        let lifetime_seconds = self.lifetime_seconds;
        let expires_at_ms = now_seconds
            .saturating_add(u64::from(lifetime_seconds))
            .saturating_mul(1000);
        match direction {
            BuildDirection::Outbound => {
                let _ = self.registry.activate_outbound(slot, tunnel, expires_at_ms);
            }
            BuildDirection::Inbound => {
                let _ = self.registry.activate_inbound(
                    slot,
                    tunnel,
                    REASSEMBLER_CAPACITY,
                    REASSEMBLER_AGGREGATE_BYTES,
                    REASSEMBLER_EXPIRY_MS,
                    now_seconds * 1000,
                    expires_at_ms,
                );
            }
        }
    }

    /// Drives the coordinator's wall-clock sweep: any pending
    /// attempt past its deadline is expired and reported.
    pub fn expire_pending(&mut self) -> Vec<BuildCoordinatorOutcome> {
        let now_ms = self.now_ms;
        let expired: Vec<BuildAttemptId> = self
            .pending
            .iter()
            .filter_map(|(id, attempt)| {
                if attempt.deadline_ms <= now_ms {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        let mut outcomes = Vec::with_capacity(expired.len());
        for id in expired {
            if let Some(pending) = self.pending.remove(&id) {
                self.retire_pending_ids(&pending);
                self.counters.timeouts = self.counters.timeouts.saturating_add(1);
                self.record_attempt_failure(pending.destination_id);
                outcomes.extend(scope_destination_terminal(
                    pending.destination_id,
                    pending.direction,
                    vec![BuildCoordinatorOutcome::TimedOut {
                        direction: pending.direction,
                    }],
                ));
            }
        }
        outcomes
    }

    /// Cancels one pending attempt by id and reports its terminal
    /// disposition.
    pub fn cancel(&mut self, attempt_id: BuildAttemptId) -> Option<BuildCoordinatorOutcome> {
        let pending = self.pending.remove(&attempt_id)?;
        self.retire_pending_ids(&pending);
        let outcome = pending.state.cancel();
        self.counters.cancellations = self.counters.cancellations.saturating_add(1);
        let terminal = match outcome {
            ShortBuildOutcome::Cancelled => BuildCoordinatorOutcome::Cancelled {
                direction: pending.direction,
            },
            other => map_terminal(other, pending.direction, pending.attempt_id),
        };
        scope_destination_terminal(pending.destination_id, pending.direction, vec![terminal]).pop()
    }

    /// Removes one slot from both the pool and the registry,
    /// reporting the typed removal category.
    pub fn remove_slot(
        &mut self,
        slot: TunnelSlot,
    ) -> Result<i2pr_tunnel::data_plane_registry::RegistryRemoval, BuildCoordinatorError> {
        let removed = self.pool.mark_failed(slot);
        if removed.is_none() {
            return Err(BuildCoordinatorError::Coordinator("unknown slot"));
        }
        Ok(self.registry.remove_slot(slot))
    }

    fn record_failure(&mut self) {
        self.paused = self.pool.note_build_failure() >= self.failure_threshold;
    }

    fn record_attempt_failure(&mut self, destination_id: Option<DestinationId>) {
        if destination_id.is_none() {
            self.record_failure();
        }
    }
}

/// Result of one [`ExploratoryBuildCoordinator::submit`] call. The
/// caller receives either a successful submission (and must drive
/// the coordinator through the inbound dispatcher to reach a
/// terminal outcome) or a typed rejection (the attempt did not
/// reach the pending table).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmitResult {
    /// The build request entered the pending table.
    Submitted {
        /// Direction the attempt is for.
        direction: BuildDirection,
        /// Attempt id the caller must use to correlate inbound
        /// dispatch outcomes.
        attempt_id: BuildAttemptId,
    },
    /// The build request was rejected by the narrow outbound
    /// seam. The coordinator removed the attempt bookkeeping.
    Rejected {
        /// Direction the attempt was for.
        direction: BuildDirection,
        /// Attempt id the coordinator reserved before the
        /// rejection. The id is never reused.
        attempt_id: BuildAttemptId,
        /// Delivery outcome the narrow seam reported.
        delivery: RouterDeliveryOutcome,
    },
}

/// Typed outcome of routing one inbound authenticated router-I2NP
/// message through the Plan 184 central dispatcher and the
/// build coordinator. The dispatcher outcome is preserved so
/// non-build replies can drive NetDB / control surfaces in later
/// plans.
#[derive(Clone, Debug)]
pub struct InboundRouteOutcome {
    /// Dispatcher outcome the central router-I2NP classifier
    /// returned.
    pub dispatcher: RouterI2npOutcome,
    /// Coordinator outcomes the build pipeline emitted. Empty
    /// when the inbound message was not a tunnel-build reply.
    pub coordinator: Vec<BuildCoordinatorOutcome>,
}

/// Extracts the count-prefixed OTBRM payload from the raw inbound
/// I2NP bytes. The decoder is strict: it must satisfy the
/// `1 + count * 218` contract exactly; otherwise the inbound
/// reply is rejected with a stable reason. The decoder never
/// weakens the canonical tunnel-build codec.
fn extract_reply_payload(raw_bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let message = decode_inbound_build_message(raw_bytes)?;
    let body = match message.body() {
        I2npBody::OutboundTunnelBuildReply(records) => records,
        _ => return Err("inbound build reply is not OutboundTunnelBuildReply"),
    };
    let expected = usize::from(body.count()).saturating_mul(usize::from(body.record_size()));
    if body.records().len() != expected {
        return Err("OTBRM record length mismatch");
    }
    // Reconstruct the canonical count-prefixed payload so the
    // state machine receives the exact `1 + count * RECORD_BYTES`
    // bytes it expects.
    let mut out = Vec::with_capacity(1 + expected);
    out.push(body.count());
    out.extend_from_slice(body.records());
    // Strict decoder pass: the canonical tunnel-build codec must
    // accept the reconstructed bytes without dropping or
    // transforming records.
    let (_count, _records) =
        decode_outbound_tunnel_build_reply(&out).map_err(|_| "OTBRM count/records invalid")?;
    Ok(out)
}

/// Decodes one inbound build message, trying short-transport first
/// (the controlled SSU2 path) then standard. Mirrors the central
/// dispatcher order in reverse for the narrow build seam: the
/// reference may emit either framing on the direct link.
fn decode_inbound_build_message(raw_bytes: &[u8]) -> Result<I2npMessage, &'static str> {
    if let Ok(message) = I2npMessage::decode_short_transport(raw_bytes, MAX_I2NP_MESSAGE_BYTES) {
        return Ok(message);
    }
    I2npMessage::decode_standard(raw_bytes, MAX_I2NP_MESSAGE_BYTES)
        .map_err(|_| "short-transport decode failed")
}

/// Extracts the forwarded inbound build payload from a
/// `ShortTunnelBuild` the reference gateway forwards to the
/// creator. The records already carry the gateway's sealed reply;
/// the shape is identical to OTBRM, so the state machine
/// postprocessor authenticates it unchanged.
fn extract_forwarded_build_payload(raw_bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let message = decode_inbound_build_message(raw_bytes)?;
    let body = match message.body() {
        I2npBody::ShortTunnelBuild(records) => records,
        _ => return Err("forwarded inbound build is not ShortTunnelBuild"),
    };
    let expected = usize::from(body.count()).saturating_mul(usize::from(body.record_size()));
    if body.records().len() != expected {
        return Err("forwarded STBM record length mismatch");
    }
    let mut out = Vec::with_capacity(1 + expected);
    out.push(body.count());
    out.extend_from_slice(body.records());
    let (_count, _records) =
        decode_outbound_tunnel_build_reply(&out).map_err(|_| "forwarded STBM count invalid")?;
    Ok(out)
}

/// Extracts `(tunnel_id, garlic_opaque)` from a `TunnelGateway`
/// frame. Tries short-transport then standard framing, mirroring
/// the central dispatcher. Returns `None` when the bytes are not a
/// decodable `TunnelGateway` carrying a Garlic inner message.
fn extract_tunnel_gateway_garlic(raw_bytes: &[u8]) -> Option<(u32, Vec<u8>)> {
    // The reference sends the Gateway directly over the
    // authenticated link; either header variant is accepted, and
    // only a Garlic inner is eligible for build-reply unwrap.
    for message in [
        I2npMessage::decode_short_transport(raw_bytes, MAX_I2NP_MESSAGE_BYTES).ok(),
        I2npMessage::decode_standard(raw_bytes, MAX_I2NP_MESSAGE_BYTES).ok(),
    ]
    .into_iter()
    .flatten()
    {
        if let I2npBody::TunnelGateway(gateway) = message.body() {
            let tunnel_id = gateway.tunnel_id;
            if tunnel_id == 0 {
                continue;
            }
            if let I2npBody::Garlic(opaque) = gateway.message.body() {
                let bytes = opaque.payload.as_bytes().to_vec();
                if bytes.is_empty() {
                    continue;
                }
                return Some((tunnel_id, bytes));
            }
        }
    }
    None
}

fn map_construction_error(error: ShortBuildConstructionError) -> BuildCoordinatorError {
    match error {
        ShortBuildConstructionError::InvalidPath { reason } => {
            BuildCoordinatorError::InvalidPath { reason }
        }
        other => BuildCoordinatorError::Coordinator(match other {
            ShortBuildConstructionError::CryptographyUnavailable => {
                "build cryptography unavailable"
            }
            ShortBuildConstructionError::AlreadyTerminal => "state machine already terminal",
            ShortBuildConstructionError::InvalidEvent { .. } => "invalid state machine event",
            ShortBuildConstructionError::ReplyRecordSize { .. } => "reply record size mismatch",
            ShortBuildConstructionError::ReplyRecordCount { .. } => "reply record count mismatch",
            ShortBuildConstructionError::InvalidReply { .. } => "reply failed validation",
            ShortBuildConstructionError::InvalidDeliveryPayload { .. } => {
                "delivery payload rejected"
            }
            ShortBuildConstructionError::Record(_) => "short build record rejected",
            ShortBuildConstructionError::MissingInboundOriginatorIdentity => {
                "inbound originator identity missing"
            }
            ShortBuildConstructionError::MissingOutboundReplyRouter => {
                "outbound reply router missing"
            }
            ShortBuildConstructionError::NotEstablished => "established material not ready",
            ShortBuildConstructionError::EstablishedMaterialAlreadyTaken => {
                "established material already extracted"
            }
            ShortBuildConstructionError::EstablishedPathStateInvalid { .. } => {
                "established path state invalid"
            }
            _ => "short build construction failed",
        }),
    }
}

fn map_terminal(
    outcome: ShortBuildOutcome,
    direction: BuildDirection,
    attempt_id: BuildAttemptId,
) -> BuildCoordinatorOutcome {
    match outcome {
        ShortBuildOutcome::Established { .. } => BuildCoordinatorOutcome::InvalidRegistration {
            direction,
            reason: "state machine reached Established after cancellation",
        },
        ShortBuildOutcome::HopRejected {
            hop_index,
            reply_code,
        } => BuildCoordinatorOutcome::HopRejected {
            direction,
            hop_index,
            response_code: reply_code.byte(),
        },
        ShortBuildOutcome::TimedOut => BuildCoordinatorOutcome::TimedOut { direction },
        ShortBuildOutcome::Cancelled => BuildCoordinatorOutcome::Cancelled { direction },
        ShortBuildOutcome::InvalidReply => BuildCoordinatorOutcome::InvalidReply {
            attempt_id,
            direction,
            reason: "multi-record reply rejected",
        },
        ShortBuildOutcome::CryptoFailed => BuildCoordinatorOutcome::InvalidReply {
            attempt_id,
            direction,
            reason: "cryptography primitive failed",
        },
        ShortBuildOutcome::DeliveryFailed => BuildCoordinatorOutcome::DeliveryFailed {
            direction,
            delivery: RouterDeliveryOutcome::Cancelled,
        },
    }
}

fn scope_destination_terminal(
    destination_id: Option<DestinationId>,
    direction: BuildDirection,
    outcomes: Vec<BuildCoordinatorOutcome>,
) -> Vec<BuildCoordinatorOutcome> {
    let Some(destination_id) = destination_id else {
        return outcomes;
    };
    if outcomes.iter().any(|outcome| {
        matches!(
            outcome,
            BuildCoordinatorOutcome::DestinationEstablished { .. }
        )
    }) {
        return outcomes;
    }
    if outcomes.iter().any(|outcome| {
        matches!(
            outcome,
            BuildCoordinatorOutcome::Cancelled { .. }
                | BuildCoordinatorOutcome::DestinationBuildCancelled { .. }
        )
    }) {
        return vec![BuildCoordinatorOutcome::DestinationBuildCancelled {
            destination_id,
            direction,
        }];
    }
    vec![BuildCoordinatorOutcome::DestinationBuildFailed {
        destination_id,
        direction,
    }]
}

fn registration_error_reason(error: i2pr_tunnel::pool::RegistrationError) -> &'static str {
    match error {
        i2pr_tunnel::pool::RegistrationError::EmptyHopList => "empty hop list",
        i2pr_tunnel::pool::RegistrationError::TooManyHops { .. } => "too many hops",
    }
}

fn registry_activation_reason(error: i2pr_tunnel::pool::ActivationError) -> &'static str {
    match error {
        i2pr_tunnel::pool::ActivationError::UnknownSlot(_) => "unknown slot",
        i2pr_tunnel::pool::ActivationError::AlreadyActivated => "slot already activated",
        i2pr_tunnel::pool::ActivationError::PlaceholderMaterial => "placeholder material",
    }
}

fn short_build_path(
    request: &BuildPathRequest,
    attempt_id: BuildAttemptId,
    now_ms: u64,
) -> ShortBuildPath {
    ShortBuildPath {
        attempt_id,
        direction: request.direction.tunnel_direction(),
        originator_hash: request.originator_hash,
        outbound_reply_router: request.outbound_reply_router,
        creator_tunnel_id: request.creator_tunnel_id,
        hops: request
            .peers
            .iter()
            .map(PeerBuildMaterial::hop_spec)
            .collect(),
        request_time: Date::from_millis(now_ms),
        next_message_id: request.message_id,
        options: Default::default(),
    }
}

/// Test-only seam: returns the typed [`TunnelState`] for the
/// supplied slot when present. Used by the unit suites to assert
/// pool-side bookkeeping.
pub fn tunnel_state_at(
    coordinator: &ExploratoryBuildCoordinator,
    slot: TunnelSlot,
) -> Option<TunnelState> {
    coordinator.pool.registration(slot).map(|reg| reg.state())
}

/// Test-only seam: returns the live creator tunnel id range the
/// coordinator will hand out next. Used by the unit suites to
/// assert the monotonic pool.
pub fn next_creator_tunnel_id_value(coordinator: &ExploratoryBuildCoordinator) -> u32 {
    coordinator.next_creator_tunnel_id
}

/// Test-only seam: advances the monotonic creator tunnel id pool
/// by one without allocating a state machine. Used by the unit
/// suites to keep the allocation sequence deterministic.
pub fn preallocate_creator_tunnel_id(
    coordinator: &mut ExploratoryBuildCoordinator,
) -> Result<TunnelId, BuildCoordinatorError> {
    let value = coordinator.next_creator_tunnel_id_value()?;
    TunnelId::new(value).map_err(|_| BuildCoordinatorError::Coordinator("creator tunnel id"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    #[test]
    fn build_delivery_set_waits_for_every_source_owned_writer_ticket() {
        let (first_tx, first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, second_rx) = tokio::sync::oneshot::channel();
        let mut set = BuildDeliverySet {
            required: 2,
            tickets: vec![
                Some(RouterDeliveryTicket::from_receiver(
                    PeerId::from_hash(Hash::from_bytes([0x11; 32])),
                    crate::router_i2np::RouterDeliveryTransport::Ntcp2(
                        i2pr_transport::LinkId::new(1).expect("link generation"),
                    ),
                    first_rx,
                )),
                Some(RouterDeliveryTicket::from_receiver(
                    PeerId::from_hash(Hash::from_bytes([0x12; 32])),
                    crate::router_i2np::RouterDeliveryTransport::Ntcp2(
                        i2pr_transport::LinkId::new(2).expect("link generation"),
                    ),
                    second_rx,
                )),
            ],
            observed: 0,
            emitted: 0,
            failed: false,
            unobserved: false,
        };
        assert_eq!(set.disposition(), BuildEmissionDisposition::Pending);
        first_tx.send(true).expect("first write receipt");
        set.poll();
        assert_eq!(set.disposition(), BuildEmissionDisposition::Pending);
        second_tx.send(true).expect("second write receipt");
        set.poll();
        assert_eq!(
            set.disposition(),
            BuildEmissionDisposition::AllOriginatorDeliveriesEmitted
        );

        let (first_tx, first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, second_rx) = tokio::sync::oneshot::channel();
        let mut partial = BuildDeliverySet {
            required: 2,
            tickets: vec![
                Some(RouterDeliveryTicket::from_receiver(
                    PeerId::from_hash(Hash::from_bytes([0x13; 32])),
                    crate::router_i2np::RouterDeliveryTransport::Ntcp2(
                        i2pr_transport::LinkId::new(3).expect("link generation"),
                    ),
                    first_rx,
                )),
                Some(RouterDeliveryTicket::from_receiver(
                    PeerId::from_hash(Hash::from_bytes([0x14; 32])),
                    crate::router_i2np::RouterDeliveryTransport::Ntcp2(
                        i2pr_transport::LinkId::new(4).expect("link generation"),
                    ),
                    second_rx,
                )),
            ],
            observed: 0,
            emitted: 0,
            failed: false,
            unobserved: false,
        };
        first_tx.send(true).expect("first write receipt");
        second_tx.send(false).expect("failed second write");
        partial.poll();
        assert_eq!(
            partial.disposition(),
            BuildEmissionDisposition::PartialEmission
        );
    }

    #[test]
    fn path_id_reservations_allow_only_adjacent_tunnel_chain_aliases() {
        let make_peer = |index: u8, receive: u32, next: u32| PeerBuildMaterial {
            router_hash: Hash::from_bytes([index; 32]),
            static_encryption_key: [index.wrapping_add(1); 32],
            receive_tunnel: TunnelId::new(receive).expect("receive tunnel"),
            next_tunnel: TunnelId::new(next).expect("next tunnel"),
            role: if index == 3 {
                HopRole::OutboundEndpoint
            } else {
                HopRole::Participant
            },
        };
        let request = BuildPathRequest {
            direction: BuildDirection::Outbound,
            peers: vec![
                make_peer(1, 11, 12),
                make_peer(2, 12, 13),
                make_peer(3, 13, 14),
            ],
            creator_tunnel_id: TunnelId::new(10).expect("creator tunnel"),
            message_id: 20,
            outbound_reply_router: None,
            originator_hash: None,
            destination_id: None,
        };
        let reserved = reserved_path_tunnel_ids(&request).expect("valid adjacent chain");
        assert_eq!(reserved, HashSet::from([10, 11, 12, 13, 14]));

        let mut nonadjacent_collision = request.clone();
        nonadjacent_collision.peers[2].next_tunnel = TunnelId::new(12).expect("reused id");
        assert!(reserved_path_tunnel_ids(&nonadjacent_collision).is_err());

        let mut broken_chain = request;
        broken_chain.peers[1].receive_tunnel = TunnelId::new(99).expect("unlinked receive");
        assert!(reserved_path_tunnel_ids(&broken_chain).is_err());
    }

    #[test]
    fn service_build_tunnel_delivery_targets_build_gateway_through_outbound_first_hop() {
        use i2pr_tunnel::build_crypto::LayerKeys;
        use i2pr_tunnel::established::{EstablishedHop, EstablishedRole, EstablishedTunnel};
        use i2pr_tunnel::roles::OutboundGatewayRole;
        use i2pr_tunnel::{TunnelDirection, TunnelPeer};

        let outbound_peer_hash = Hash::from_bytes([0x31; 32]);
        let build_gateway_hash = Hash::from_bytes([0x42; 32]);
        let receive = TunnelId::new(0x1234).expect("outbound receive tunnel");
        let hop = EstablishedHop::terminal(
            TunnelPeer::from_hash(outbound_peer_hash),
            EstablishedRole::OutboundEndpoint,
            receive,
            LayerKeys::new([0x51; 32], [0x52; 32], [0x53; 32]),
        );
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(0x2345).expect("creator id"),
            vec![hop],
            0,
            None,
            None,
        )
        .expect("established outbound tunnel");
        let role = OutboundGatewayRole::new(tunnel, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(0x478);
        let deliveries = compose_outbound_build_deliveries(
            &role,
            build_gateway_hash,
            0x3456,
            30_000,
            &[0x61; 32],
            1_000,
            &mut rng,
        )
        .expect("tunnel route composition");
        assert_eq!(deliveries.len(), 1);
        let (target, bytes, cell_message_id) = &deliveries[0];
        assert_eq!(*target, PeerId::from_hash(outbound_peer_hash));
        assert_ne!(*target, PeerId::from_hash(build_gateway_hash));
        assert_ne!(
            *cell_message_id, 0,
            "tunnel-data transport ID must be nonzero"
        );
        let message = I2npMessage::decode_short_transport(bytes, MAX_I2NP_MESSAGE_BYTES)
            .expect("TunnelData transport envelope");
        let I2npBody::TunnelData(cell) = message.body() else {
            panic!("outbound route must deliver TunnelData");
        };
        assert_eq!(cell.tunnel_id, receive.get());
    }

    #[test]
    fn wrapped_outbound_invalid_reply_keeps_its_exact_attempt_id() {
        let destination_id = DestinationId::from_hash(Hash::from_bytes([0x31; 32]));
        let attempt_id = BuildAttemptId::new(17);
        let peers = (0..3)
            .map(|index| PeerBuildMaterial {
                router_hash: Hash::from_bytes([0x40 + index; 32]),
                static_encryption_key: [0x50 + index; 32],
                receive_tunnel: TunnelId::new(0x7400 + u32::from(index)).expect("receive"),
                next_tunnel: TunnelId::new(0x7500 + u32::from(index)).expect("next"),
                role: if index == 2 {
                    HopRole::OutboundEndpoint
                } else {
                    HopRole::Participant
                },
            })
            .collect::<Vec<_>>();
        let request = BuildPathRequest {
            direction: BuildDirection::Outbound,
            peers,
            creator_tunnel_id: TunnelId::new(0x7601).expect("creator tunnel"),
            message_id: 0x7701,
            outbound_reply_router: Some(Hash::from_bytes([0x78; 32])),
            originator_hash: None,
            destination_id: Some(destination_id),
        };
        let path = short_build_path(&request, attempt_id, 1_700_000_000_000);
        let terminal = path.hops.last().expect("terminal hop");
        let target_peer = PeerId::from_hash(terminal.router_hash());
        let next_tunnel = terminal.next_tunnel();
        let pending = PendingBuild {
            attempt_id,
            direction: BuildDirection::Outbound,
            destination_id: Some(destination_id),
            target_peer,
            deadline_ms: 1_700_000_060_000,
            message_id: path.next_message_id,
            outer_message_id: 0x7702,
            reserved_tunnel_ids: vec![0x7601, 0x7400, 0x7401, 0x7402, 0x7500, 0x7501, 0x7502],
            next_tunnel,
            garlic_key: Some(Zeroizing::new([0xA1; 32])),
            garlic_tag: Some([0xB1; GARLIC_REPLY_TAG_LEN]),
            inbound_observation: InboundBuildObservation::default(),
            delivery_set: BuildDeliverySet::default(),
            state: ShortBuildStateMachine::new(path, 1_700_000_060_000),
        };
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator
            .pending_tunnel_ids
            .extend(pending.reserved_tunnel_ids.iter().copied());
        coordinator.pending.insert(attempt_id, pending);
        assert_eq!(coordinator.pending_message_ids(), vec![0x7701]);
        assert!(coordinator.has_pending_message_id(0x7701));
        assert!(!coordinator.has_pending_message_id(0x7702));
        let garlic = I2npMessage::new_standard(
            0x7701,
            Date::from_millis(1_700_000_000_000),
            I2npBody::Garlic(i2pr_proto::OpaqueMessageBody {
                payload: i2pr_proto::DeferredPayload::new(vec![0; 27], MAX_I2NP_MESSAGE_BYTES)
                    .expect("bounded opaque garlic"),
            }),
        )
        .expect("nested garlic message");
        let gateway = I2npMessage::new_short_transport(
            0x7801,
            1_700_000_060,
            I2npBody::TunnelGateway(Box::new(i2pr_proto::TunnelGatewayMessage {
                tunnel_id: next_tunnel.get(),
                message: Box::new(garlic),
            })),
        )
        .expect("gateway message")
        .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
        .expect("gateway bytes");
        let inbound = Ssu2InboundI2np {
            link_id: i2pr_transport::LinkId::new(1).expect("link"),
            peer: target_peer,
            bytes: gateway,
        };

        let routed = coordinator
            .route_inbound_i2np(&inbound, 1_700_000_000_000)
            .expect("dispatcher accepts the bounded gateway");
        assert!(matches!(
            routed.dispatcher,
            RouterI2npOutcome::Unsupported {
                type_byte: 19,
                peer,
                ..
            } if peer == target_peer
        ));
        assert!(matches!(
            routed.coordinator.as_slice(),
            [BuildCoordinatorOutcome::InvalidReply {
                attempt_id: correlated_attempt,
                direction: BuildDirection::Outbound,
                ..
            }] if *correlated_attempt == attempt_id
        ));
        assert_eq!(
            coordinator.pending_len(),
            1,
            "bad garlic keeps the attempt pending"
        );
        assert!(coordinator.cancel(attempt_id).is_some());
        assert!(coordinator.reserved_message_ids().contains(&0x7701));
        assert!(coordinator.reserved_message_ids().contains(&0x7702));
        assert!(coordinator.reserved_tunnel_ids().contains(&0x7601));
        coordinator.advance_time(1_700_000_000_000 + BUILD_TUNNEL_ID_RETENTION_MS + 1);
        assert!(!coordinator.reserved_tunnel_ids().contains(&0x7601));
        assert!(!coordinator.reserved_message_ids().contains(&0x7701));
    }

    #[test]
    fn inbound_orphan_observations_are_attempt_scoped_and_identifier_free() {
        let attempt_id = BuildAttemptId::new(27);
        let peers = (0..3)
            .map(|index| PeerBuildMaterial {
                router_hash: Hash::from_bytes([0x60 + index; 32]),
                static_encryption_key: [0x70 + index; 32],
                receive_tunnel: TunnelId::new(0x7400 + u32::from(index)).expect("receive"),
                next_tunnel: TunnelId::new(0x7500 + u32::from(index)).expect("next"),
                role: if index == 0 {
                    HopRole::InboundGateway
                } else {
                    HopRole::Participant
                },
            })
            .collect::<Vec<_>>();
        let request = BuildPathRequest {
            direction: BuildDirection::Inbound,
            peers,
            creator_tunnel_id: TunnelId::new(0x7601).expect("creator tunnel"),
            message_id: 0x7701,
            outbound_reply_router: None,
            originator_hash: Some(Hash::from_bytes([0x78; 32])),
            destination_id: None,
        };
        let path = short_build_path(&request, attempt_id, 1_700_000_000_000);
        let terminal = path.hops.last().expect("terminal hop");
        let target_peer = PeerId::from_hash(terminal.router_hash());
        let expected_message_id = path.next_message_id;
        let pending = PendingBuild {
            attempt_id,
            direction: BuildDirection::Inbound,
            destination_id: None,
            target_peer,
            deadline_ms: 1_700_000_060_000,
            message_id: expected_message_id,
            outer_message_id: 0x7702,
            reserved_tunnel_ids: vec![0x7601, 0x7400, 0x7401, 0x7402, 0x7500, 0x7501, 0x7502],
            next_tunnel: terminal.next_tunnel(),
            garlic_key: None,
            garlic_tag: None,
            inbound_observation: InboundBuildObservation::default(),
            delivery_set: BuildDeliverySet::default(),
            state: ShortBuildStateMachine::new(path, 1_700_000_060_000),
        };
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.pending.insert(attempt_id, pending);

        coordinator.route_forwarded_inbound_build(
            PeerId::from_hash(Hash::from_bytes([0x79; 32])),
            expected_message_id,
            &[],
        );
        coordinator.route_forwarded_inbound_build(
            target_peer,
            expected_message_id.wrapping_add(1),
            &[],
        );
        coordinator.route_forwarded_inbound_build(
            target_peer,
            expected_message_id.wrapping_sub(1),
            &[],
        );
        coordinator.route_forwarded_inbound_build(
            PeerId::from_hash(Hash::from_bytes([0x7A; 32])),
            expected_message_id.wrapping_add(2),
            &[],
        );

        assert_eq!(
            coordinator.inbound_build_observation(attempt_id),
            Some(InboundBuildObservation {
                authenticated_candidates: 4,
                peer_matches: 2,
                message_id_matches: 1,
                peer_match_id_plus_one: 1,
                peer_match_id_minus_one: 1,
                peer_match_id_other: 0,
            })
        );
    }

    #[test]
    fn coordinator_starts_with_no_pending_attempts() {
        let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        assert_eq!(coord.pending_len(), 0);
        assert_eq!(coord.counters().installed, 0);
        assert_eq!(coord.counters().pending, 0);
        assert!(!coord.is_paused());
    }

    #[test]
    fn service_coordinator_capacity_covers_bounded_destination_roles() {
        let pool = ExploratoryPoolConfig::balanced();
        let coordinator = ExploratoryBuildCoordinator::new_with_service_destination_capacity(pool);
        assert_eq!(
            coordinator.registry().capacity(),
            DataPlaneCapacity::new(
                pool.max_outbound() + 1,
                pool.max_inbound()
                    + u16::try_from(i2pr_service_tunnels::MAX_SERVICE_TUNNELS).unwrap()
                        * u16::from(i2pr_service_tunnels::MAX_EFFECTIVE_DIRECTION_TUNNELS),
            )
        );
        let standalone = ExploratoryBuildCoordinator::new(pool);
        assert_eq!(
            standalone.registry().capacity(),
            DataPlaneCapacity::new(pool.max_outbound(), pool.max_inbound())
        );
    }

    #[test]
    fn destination_build_failures_are_scoped_and_do_not_pause_exploratory_pool() {
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.failure_threshold = 1;
        let destination_id = DestinationId::from_hash(Hash::from_bytes([0x31; 32]));
        coordinator.record_attempt_failure(Some(destination_id));
        assert!(!coordinator.is_paused());
        coordinator.record_attempt_failure(None);
        assert!(coordinator.is_paused());

        let failed = scope_destination_terminal(
            Some(destination_id),
            BuildDirection::Inbound,
            vec![BuildCoordinatorOutcome::HopRejected {
                direction: BuildDirection::Inbound,
                hop_index: 1,
                response_code: 1,
            }],
        );
        assert!(matches!(
            failed.as_slice(),
            [BuildCoordinatorOutcome::DestinationBuildFailed {
                destination_id: failed_id,
                direction: BuildDirection::Inbound,
            }] if *failed_id == destination_id
        ));
        let cancelled = scope_destination_terminal(
            Some(destination_id),
            BuildDirection::Outbound,
            vec![BuildCoordinatorOutcome::Cancelled {
                direction: BuildDirection::Outbound,
            }],
        );
        assert!(matches!(
            cancelled.as_slice(),
            [BuildCoordinatorOutcome::DestinationBuildCancelled {
                destination_id: cancelled_id,
                direction: BuildDirection::Outbound,
            }] if *cancelled_id == destination_id
        ));
    }

    #[test]
    fn destination_pending_builds_are_counted_by_direction_and_cancelled_by_owner() {
        fn pending(
            destination_id: DestinationId,
            direction: BuildDirection,
            attempt_raw: u64,
        ) -> PendingBuild {
            let peers = (0..3)
                .map(|index| PeerBuildMaterial {
                    router_hash: Hash::from_bytes([0x40 + index; 32]),
                    static_encryption_key: [0x50 + index; 32],
                    receive_tunnel: TunnelId::new(0x7400 + index as u32).expect("receive"),
                    next_tunnel: TunnelId::new(0x7500 + index as u32).expect("next"),
                    role: match (direction, index) {
                        (BuildDirection::Outbound, 2) => HopRole::OutboundEndpoint,
                        (BuildDirection::Inbound, 0) => HopRole::InboundGateway,
                        _ => HopRole::Participant,
                    },
                })
                .collect::<Vec<_>>();
            let request = DestinationBuildRequest {
                destination_id,
                direction,
                peers,
                creator_tunnel_id: TunnelId::new(0x7600 + attempt_raw as u32).expect("creator"),
                message_id: 0x7700 + attempt_raw as u32,
                outbound_reply_router: (direction == BuildDirection::Outbound)
                    .then_some(Hash::from_bytes([0x78; 32])),
                originator_hash: (direction == BuildDirection::Inbound)
                    .then_some(Hash::from_bytes([0x79; 32])),
            };
            let path_request = BuildPathRequest::from(request);
            let mut reserved_tunnel_ids = vec![path_request.creator_tunnel_id.get()];
            for peer in &path_request.peers {
                reserved_tunnel_ids.push(peer.receive_tunnel.get());
                reserved_tunnel_ids.push(peer.next_tunnel.get());
            }
            let attempt_id = BuildAttemptId::new(attempt_raw);
            let path = short_build_path(&path_request, attempt_id, 1_000);
            let target_peer = PeerId::from_hash(path.hops[0].router_hash());
            let message_id = path.next_message_id;
            PendingBuild {
                attempt_id,
                direction,
                destination_id: Some(destination_id),
                target_peer,
                deadline_ms: 60_000,
                message_id,
                outer_message_id: 0x7800 + attempt_raw as u32,
                reserved_tunnel_ids,
                next_tunnel: TunnelId::new(0x7A00 + attempt_raw as u32).expect("next tunnel"),
                garlic_key: None,
                garlic_tag: None,
                inbound_observation: InboundBuildObservation::default(),
                delivery_set: BuildDeliverySet::default(),
                state: ShortBuildStateMachine::new(path, 60_000),
            }
        }

        let group_a = DestinationId::from_hash(Hash::from_bytes([0x31; 32]));
        let group_b = DestinationId::from_hash(Hash::from_bytes([0x32; 32]));
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.pending.insert(
            BuildAttemptId::new(1),
            pending(group_a, BuildDirection::Inbound, 1),
        );
        coordinator.pending.insert(
            BuildAttemptId::new(2),
            pending(group_a, BuildDirection::Outbound, 2),
        );
        coordinator.pending.insert(
            BuildAttemptId::new(3),
            pending(group_b, BuildDirection::Inbound, 3),
        );

        assert_eq!(coordinator.pending_destination_len(group_a), 2);
        assert_eq!(
            coordinator.pending_destination_direction_len(group_a, BuildDirection::Inbound),
            1
        );
        assert_eq!(
            coordinator.pending_destination_direction_len(group_a, BuildDirection::Outbound),
            1
        );
        let cancelled = coordinator.cancel_destination_builds(group_a);
        assert_eq!(cancelled.len(), 2);
        assert_eq!(coordinator.pending_len(), 1);
        assert_eq!(coordinator.pending_destination_len(group_b), 1);
        assert!(cancelled.iter().all(|outcome| matches!(
            outcome,
            BuildCoordinatorOutcome::DestinationBuildCancelled { destination_id, .. }
                if *destination_id == group_a
        )));
        // A subsequent generation reconcile must not surface or retain the
        // attempts cancelled by the preceding generation transition.
        assert!(coordinator.cancel_destination_builds(group_a).is_empty());
        assert_eq!(coordinator.pending_destination_len(group_a), 0);
        assert_eq!(coordinator.pending_destination_len(group_b), 1);
    }

    #[test]
    fn selected_destination_order_is_preserved_into_shared_short_build_path() {
        let candidates = vec![
            crate::destination_peers::test_candidate(701, "family-a", "10.1.1.1", 1201),
            crate::destination_peers::test_candidate(702, "family-b", "10.2.1.1", 1202),
            crate::destination_peers::test_candidate(703, "family-c", "10.3.1.1", 1203),
            crate::destination_peers::test_candidate(704, "family-d", "10.4.1.1", 1204),
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(705);
        let selected = crate::destination_peers::select_destination_path(&candidates, &mut rng)
            .expect("exact-three selection");
        let expected = selected
            .iter()
            .map(crate::destination_peers::DestinationPeerCandidate::router_hash)
            .collect::<Vec<_>>();
        let peers = selected
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| PeerBuildMaterial {
                router_hash: candidate.router_hash(),
                static_encryption_key: *candidate.static_encryption_key(),
                receive_tunnel: TunnelId::new(0x7000 + index as u32).expect("receive"),
                next_tunnel: TunnelId::new(0x7001 + index as u32).expect("next"),
                role: if index == 2 {
                    HopRole::OutboundEndpoint
                } else {
                    HopRole::Participant
                },
            })
            .collect::<Vec<_>>();
        let request = DestinationBuildRequest {
            destination_id: DestinationId::from_hash(Hash::from_bytes([0x70; 32])),
            direction: BuildDirection::Outbound,
            peers,
            creator_tunnel_id: TunnelId::new(0x7010).expect("creator"),
            message_id: 0x7011,
            outbound_reply_router: Some(Hash::from_bytes([0x71; 32])),
            originator_hash: None,
        };
        let path_request = BuildPathRequest::from(request);
        let path = short_build_path(&path_request, BuildAttemptId::new(1), 1_700_000_000_000);
        assert_eq!(
            path.hops
                .iter()
                .map(HopSpec::router_hash)
                .collect::<Vec<_>>(),
            expected,
            "the same shared path conversion used by submit_path preserves selector order"
        );
        path.validate().expect("valid submitted path");
    }

    #[test]
    fn destination_build_request_debug_redacts_router_identity_and_build_keys() {
        let peer = PeerBuildMaterial {
            router_hash: Hash::from_bytes([0xA5; 32]),
            static_encryption_key: [0x5A; 32],
            receive_tunnel: TunnelId::new(0x7110).expect("receive"),
            next_tunnel: TunnelId::new(0x7111).expect("next"),
            role: HopRole::Participant,
        };
        let debug = format!("{peer:?}");
        assert!(!debug.contains("a5a5"));
        assert!(!debug.contains("90, 90, 90"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn lifetime_rejects_zero_and_over_maximum() {
        let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        assert!(matches!(
            coord.set_lifetime_seconds(0),
            Err(BuildCoordinatorError::Coordinator(_))
        ));
        assert!(matches!(
            coord.set_lifetime_seconds(MAX_EXPLORATORY_LIFETIME_SECONDS + 1),
            Err(BuildCoordinatorError::Coordinator(_))
        ));
        assert!(coord.set_lifetime_seconds(600).is_ok());
        assert_eq!(coord.lifetime_seconds(), 600);
    }

    #[test]
    fn next_attempt_id_is_monotonic() {
        let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        let first = coord.next_attempt_id();
        let second = coord.next_attempt_id();
        let third = coord.next_attempt_id();
        assert_eq!(first.get(), INITIAL_ATTEMPT_ID);
        assert_eq!(second.get(), INITIAL_ATTEMPT_ID + 1);
        assert_eq!(third.get(), INITIAL_ATTEMPT_ID + 2);
    }

    #[test]
    fn pool_registrations_return_empty_initially() {
        let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        assert_eq!(coord.registrations(TunnelDirection::Inbound).len(), 0);
        assert_eq!(coord.registrations(TunnelDirection::Outbound).len(), 0);
        assert_eq!(coord.inbound_pool_len(), 0);
        assert_eq!(coord.outbound_pool_len(), 0);
    }

    #[test]
    fn extract_reply_payload_rejects_empty_bytes() {
        assert!(matches!(
            extract_reply_payload(&[]),
            Err("short-transport decode failed")
        ));
    }

    #[test]
    fn extract_reply_payload_rejects_unknown_body() {
        // A DeliveryStatus message is not a build reply.
        use i2pr_proto::{Date, DeliveryStatusMessage};
        let body = i2pr_proto::I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            1,
            Date::from_millis(1),
        ));
        let message = I2npMessage::new_short_transport(1, 60, body).expect("message");
        let wire = message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .expect("encode");
        assert!(matches!(
            extract_reply_payload(&wire),
            Err("inbound build reply is not OutboundTunnelBuildReply")
        ));
    }

    #[test]
    fn tunneled_inbound_build_composes_single_two_fragment_cell() {
        use i2pr_tunnel::build_crypto::LayerKeys;
        use i2pr_tunnel::established::{
            EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
        };
        use i2pr_tunnel::roles::OutboundGatewayRole;
        use i2pr_tunnel::{TunnelDirection, TunnelPeer};

        let peers = (0..3)
            .map(|index| PeerBuildMaterial {
                router_hash: Hash::from_bytes([0x60 + index; 32]),
                static_encryption_key: [0x70 + index; 32],
                receive_tunnel: TunnelId::new(0x8100 + u32::from(index)).expect("receive"),
                next_tunnel: TunnelId::new(0x8101 + u32::from(index)).expect("next"),
                role: match index {
                    0 => HopRole::InboundGateway,
                    _ => HopRole::Participant,
                },
            })
            .collect::<Vec<_>>();
        let request = BuildPathRequest {
            direction: BuildDirection::Inbound,
            peers,
            creator_tunnel_id: TunnelId::new(0x8001).expect("creator tunnel"),
            message_id: 0x8002,
            outbound_reply_router: None,
            originator_hash: Some(Hash::from_bytes([0x79; 32])),
            destination_id: None,
        };
        let now_ms = 1_700_000_000_000_u64;
        let path = short_build_path(&request, BuildAttemptId::new(99), now_ms);
        path.validate().expect("valid path");
        let mut state = ShortBuildStateMachine::with_cryptography(
            path,
            now_ms.saturating_add(60_000),
            i2pr_tunnel::build_crypto::EciesX25519BuildCryptography::new(),
        );
        let mut rng = ChaCha8Rng::seed_from_u64(0x4780);
        let message = state.prepare(&mut rng).expect("prepare");
        let action: ShortBuildAction = state.deliver_action(message).expect("deliver");
        let bridge = ShortBuildI2npBridge::new();
        // The tunneled inner mirrors the live submit path: standard
        // framing with millisecond expiry, so the stock OBEP
        // `IsExpired` check at the standard offset passes. A short
        // inner reads as far-future and is dropped as
        // `TunnelMessage: Message expired` after traversing the
        // whole outbound path.
        let header = BridgeHeader::Standard {
            message_id: 0x8003,
            expiration_ms: 1_700_000_060_000,
        };
        let wire = wrap_inner_wire(&bridge, &action, header).expect("wrap inner");
        let decoded = I2npMessage::decode_standard(&wire, MAX_I2NP_MESSAGE_BYTES)
            .expect("standard inner decodes");
        assert!(
            matches!(decoded.body(), I2npBody::ShortTunnelBuild(_)),
            "tunneled inner carries the build records"
        );
        // A three-hop short build request (4 records) is small
        // enough for one TunnelData cell; the fixed 1028-byte cell
        // body always needs two SSU2 fragments at the default
        // 1024-byte budget. Both facts are live-lane evidence
        // inputs, so they are pinned here.
        assert_eq!(wire.len(), 889, "standard inner build wire size");
        let outbound_peer_hash = Hash::from_bytes([0x31; 32]);
        let outbound_hops = vec![
            EstablishedHop::with_next(
                TunnelPeer::from_hash(outbound_peer_hash),
                EstablishedRole::Participant,
                TunnelId::new(0x301).expect("first receive"),
                LayerKeys::new([0x51; 32], [0x52; 32], [0x53; 32]),
                EstablishedNextHop::new(
                    TunnelPeer::from_hash(Hash::from_bytes([0x32; 32])),
                    TunnelId::new(0x401).expect("endpoint receive"),
                ),
            ),
            EstablishedHop::terminal(
                TunnelPeer::from_hash(Hash::from_bytes([0x32; 32])),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x401).expect("endpoint receive"),
                LayerKeys::new([0x54; 32], [0x55; 32], [0x56; 32]),
            ),
        ];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(0x2345).expect("creator id"),
            outbound_hops,
            0,
            None,
            None,
        )
        .expect("established outbound tunnel");
        let role = OutboundGatewayRole::new(tunnel, now_ms.saturating_add(60_000));
        let deliveries = compose_outbound_build_deliveries(
            &role,
            Hash::from_bytes([0x60; 32]),
            0x8003,
            60_000,
            &wire,
            now_ms,
            &mut rng,
        )
        .expect("tunnel route composition");
        assert_eq!(deliveries.len(), 1, "one cell per inbound build");
        let (_, bytes, cell_message_id) = &deliveries[0];
        assert_ne!(*cell_message_id, 0, "cell transport ID must be nonzero");
        // 4-byte tunnel ID + 16-byte IV + 1008-byte payload, framed
        // with a 9-byte short-transport header.
        assert_eq!(bytes.len(), 1037, "cell transport wire size");
        let body_len = bytes.len() - 9;
        assert_eq!(body_len, 1028, "cell transport body size");
        assert_eq!(
            body_len.div_ceil(1024),
            2,
            "cell needs two SSU2 fragments at the default budget"
        );
    }
}
