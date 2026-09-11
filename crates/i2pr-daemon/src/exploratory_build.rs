//! Plan 185 daemon-owned exploratory build coordinator.
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

use std::collections::BTreeMap;
use std::time::Duration;

use i2pr_proto::{Date, Hash, I2npBody, I2npMessage};
use i2pr_runtime::Ssu2InboundI2np;
use i2pr_transport::{MAX_I2NP_MESSAGE_BYTES, PeerId};
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;
use zeroize::Zeroizing;

use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::build_crypto::GARLIC_REPLY_TAG_LEN;
use i2pr_tunnel::config::ExploratoryPoolConfig;
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
    RouterDeliveryOutcome, RouterDeliveryRequest, RouterDeliveryService, RouterI2npError,
    RouterI2npKind, RouterI2npOutcome, dispatch_router_i2np,
};

/// Maximum number of simultaneous pending exploratory build
/// attempts the coordinator tracks. The ceiling is conservative
/// for loopback-only operation and matches the canonical I2P
/// per-router build concurrency ceiling.
pub const MAX_PENDING_BUILDS: usize = 16;
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
#[derive(Clone, Debug)]
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

/// Submission record for one build attempt.
#[derive(Clone, Debug)]
pub struct BuildRequest {
    /// Direction this build is for.
    pub direction: BuildDirection,
    /// Per-peer build material the state machine consumes.
    pub peer: PeerBuildMaterial,
    /// Creator tunnel id the pool registrar reserves for the
    /// established material. The coordinator owns the monotonic
    /// pool of creator tunnel ids.
    pub creator_tunnel_id: TunnelId,
    /// Per-attempt message id the I2NP header carries. The same
    /// value also seeds the build request `next_message_id` field.
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
            | Self::InvalidPath { direction, .. }
            | Self::CoordinatorRejected { direction, .. } => *direction,
        }
    }
}

/// Typed submission / dispatch failure the coordinator surfaces.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BuildCoordinatorError {
    /// The supplied request violated a coordinator-level invariant.
    #[error("exploratory build coordinator rejected the request: {0}")]
    Coordinator(&'static str),
    /// The path validator rejected the supplied path.
    #[error("exploratory build path is invalid: {reason}")]
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
    /// Duplicate inbound replies the coordinator rejected.
    pub duplicate_replies: u64,
    /// Outbound tunnel-build requests the coordinator emitted.
    pub outbound_builds: u64,
}

/// One pending build attempt tracked by the coordinator. The
/// coordinator never clones the secrets inside the state machine;
/// the state machine is owned by `state` until the attempt reaches
/// a terminal outcome.
#[allow(dead_code)]
struct PendingBuild {
    direction: BuildDirection,
    peer: PeerBuildMaterial,
    /// Outbound peer hash (the receiving router). The coordinator
    /// matches inbound replies whose authenticated peer equals
    /// this value.
    target_peer: PeerId,
    /// Deadline (wall clock ms) at which the coordinator expires
    /// the attempt.
    deadline_ms: u64,
    /// Per-attempt message id the I2NP header carried.
    message_id: u32,
    /// Creator-supplied `next_tunnel` the reference echoes in its
    /// `TunnelGateway` reply for outbound endpoint builds.
    /// Plan 188 correlation seam for garlic-wrapped replies.
    next_tunnel: TunnelId,
    /// OBEP `RGarlicKeyAndTag` key for garlic-wrapped endpoint
    /// replies (`None` for inbound builds, which never garlic-wrap).
    garlic_key: Option<Zeroizing<[u8; 32]>>,
    /// OBEP garlic reply tag (8 bytes) paired with `garlic_key`.
    garlic_tag: Option<[u8; GARLIC_REPLY_TAG_LEN]>,
    /// Live state machine. The coordinator never spawns a task
    /// per attempt; the daemon-owned pump drives every attempt.
    state: ShortBuildStateMachine,
}

/// Daemon-owned exploratory build coordinator.
pub struct ExploratoryBuildCoordinator {
    pool: ExploratoryPool,
    registry: DataPlaneRegistry,
    pending: BTreeMap<BuildAttemptId, PendingBuild>,
    next_attempt_id: u64,
    next_creator_tunnel_id: u32,
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
        let registry_capacity = DataPlaneCapacity::new(
            u8::try_from(outbound.max(inbound)).unwrap_or(u8::MAX),
            u8::try_from(inbound.max(outbound)).unwrap_or(u8::MAX),
        );
        Self {
            pool: ExploratoryPool::new(pool_config),
            registry: DataPlaneRegistry::new(registry_capacity),
            pending: BTreeMap::new(),
            next_attempt_id: INITIAL_ATTEMPT_ID,
            next_creator_tunnel_id: INITIAL_CREATOR_TUNNEL_ID,
            counters: BuildCoordinatorCounters::default(),
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            lifetime_seconds: DEFAULT_EXPLORATORY_LIFETIME_SECONDS,
            paused: false,
            now_ms: 0,
        }
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

    /// Returns the bounded number of pending builds.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
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
        let value = self.next_creator_tunnel_id;
        self.next_creator_tunnel_id = self.next_creator_tunnel_id.checked_add(1).ok_or(
            BuildCoordinatorError::Coordinator("creator tunnel id monotonic counter exhausted"),
        )?;
        Ok(value)
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
        if self.paused {
            return Err(BuildCoordinatorError::Coordinator(
                "coordinator is paused after consecutive failures",
            ));
        }
        if self.pending.len() >= MAX_PENDING_BUILDS {
            return Err(BuildCoordinatorError::Coordinator(
                "too many pending builds",
            ));
        }
        if request.peer.router_hash.as_bytes().iter().all(|b| *b == 0) {
            return Err(BuildCoordinatorError::Coordinator(
                "peer router hash is zero",
            ));
        }
        if request.peer.static_encryption_key.iter().all(|b| *b == 0) {
            return Err(BuildCoordinatorError::Coordinator(
                "peer static encryption key is zero",
            ));
        }
        if request.message_id == 0 {
            return Err(BuildCoordinatorError::Coordinator("message id is zero"));
        }
        if self.now_ms == 0 {
            return Err(BuildCoordinatorError::Coordinator(
                "advance_time not called",
            ));
        }
        let direction = request.direction;
        let now_ms = self.now_ms;
        let attempt_id = self.next_attempt_id();
        let creator_tunnel_id = request.creator_tunnel_id;
        let path_attempt = BuildAttemptId::new(attempt_id.get());
        let path = ShortBuildPath {
            attempt_id: path_attempt,
            direction: direction.tunnel_direction(),
            originator_hash: request.originator_hash,
            outbound_reply_router: request.outbound_reply_router,
            creator_tunnel_id,
            hops: vec![request.peer.hop_spec()],
            request_time: Date::from_millis(now_ms),
            next_message_id: request.message_id,
            options: Default::default(),
        };
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
        let (i2np_message, _bridge_record) = bridge
            .wrap_deliver_action(&action, bridge_header)
            .map_err(|error| {
                BuildCoordinatorError::Coordinator(match error {
                    i2pr_tunnel::bridge::BridgeError::ZeroRecordCount => {
                        "STBM record count is zero"
                    }
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
        let wire = i2np_message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .map_err(|_| {
                BuildCoordinatorError::Coordinator("I2NP short-transport encoding failed")
            })?;
        let peer = PeerId::from_hash(request.peer.router_hash);
        let delivery_request = RouterDeliveryRequest::new(peer, wire, BUILD_DELIVERY_TIMEOUT)
            .map_err(|_| {
                BuildCoordinatorError::Coordinator("delivery request validation failed")
            })?;
        let delivery_outcome =
            delivery.deliver(delivery_request, &i2pr_runtime::CancellationToken::new());
        let accepted = matches!(delivery_outcome, RouterDeliveryOutcome::Accepted);
        self.counters.outbound_builds = self.counters.outbound_builds.saturating_add(1);
        if !accepted {
            self.counters.delivery_failures = self.counters.delivery_failures.saturating_add(1);
            self.record_failure();
            return Ok(SubmitResult::Rejected {
                direction,
                attempt_id,
                delivery: delivery_outcome,
            });
        }
        state.mark_dispatched().map_err(map_construction_error)?;
        let target_peer = PeerId::from_hash(request.peer.router_hash);
        // Plan 188: retain the OBEP Garlic material for garlic-wrapped
        // endpoint replies. The state machine owns the authoritative
        // copy; this is the coordinator's correlation copy for
        // TunnelGateway unwrapping. Inbound builds never garlic-wrap.
        let (garlic_key, garlic_tag) = match state.obep_garlic_material() {
            Some((key, tag)) => (Some(Zeroizing::new(key)), Some(tag)),
            None => (None, None),
        };
        let pending = PendingBuild {
            direction,
            peer: request.peer.clone(),
            target_peer,
            deadline_ms,
            message_id: request.message_id,
            next_tunnel: request.peer.next_tunnel,
            garlic_key,
            garlic_tag,
            state,
        };
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
        Ok(InboundRouteOutcome {
            dispatcher: outcome,
            coordinator: coordinator_outcomes,
        })
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
        let reply_payload = match extract_reply_payload(raw_bytes) {
            Ok(payload) => payload,
            Err(reason) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                let direction = self.pending.get(&attempt_id).map(|p| p.direction);
                if let Some(direction) = direction {
                    return vec![BuildCoordinatorOutcome::InvalidReply { direction, reason }];
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
            return Vec::new();
        };
        let reply_payload = match extract_forwarded_build_payload(raw_bytes) {
            Ok(payload) => payload,
            Err(reason) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                let direction = self.pending.get(&attempt_id).map(|p| p.direction);
                if let Some(direction) = direction {
                    return vec![BuildCoordinatorOutcome::InvalidReply { direction, reason }];
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
        let (tunnel_id, garlic_opaque) = match extract_tunnel_gateway_garlic(raw_bytes) {
            Some(value) => value,
            None => {
                self.counters.inbound_orphans = self.counters.inbound_orphans.saturating_add(1);
                return Vec::new();
            }
        };
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
        let mut pending = pending;
        let event_result = pending.state.handle_event(BuildEvent::BuildReply {
            reply: Zeroizing::new(reply_payload),
        });
        let outcome = match event_result {
            Ok(Some(outcome)) => outcome,
            Ok(None) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                return vec![BuildCoordinatorOutcome::InvalidReply {
                    direction: pending.direction,
                    reason: "state machine did not reach terminal",
                }];
            }
            Err(_) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                return vec![BuildCoordinatorOutcome::InvalidReply {
                    direction: pending.direction,
                    reason: "state machine rejected the reply",
                }];
            }
        };
        self.finalize_attempt(pending, outcome)
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
                self.record_failure();
                vec![BuildCoordinatorOutcome::HopRejected {
                    direction: pending.direction,
                    hop_index,
                    response_code: reply_code.byte(),
                }]
            }
            ShortBuildOutcome::TimedOut => {
                self.counters.timeouts = self.counters.timeouts.saturating_add(1);
                self.record_failure();
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
                self.record_failure();
                vec![BuildCoordinatorOutcome::InvalidReply {
                    direction: pending.direction,
                    reason: "multi-record reply rejected",
                }]
            }
            ShortBuildOutcome::CryptoFailed => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                self.record_failure();
                vec![BuildCoordinatorOutcome::InvalidReply {
                    direction: pending.direction,
                    reason: "cryptography primitive failed",
                }]
            }
            ShortBuildOutcome::DeliveryFailed => {
                self.counters.delivery_failures = self.counters.delivery_failures.saturating_add(1);
                self.record_failure();
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
                self.record_failure();
                return vec![BuildCoordinatorOutcome::PoolRejected { direction, kind }];
            }
            Err(RegisterError::Invalid(error)) => {
                self.counters.invalid_replies = self.counters.invalid_replies.saturating_add(1);
                self.record_failure();
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
                self.counters.timeouts = self.counters.timeouts.saturating_add(1);
                self.record_failure();
                outcomes.push(BuildCoordinatorOutcome::TimedOut {
                    direction: pending.direction,
                });
            }
        }
        outcomes
    }

    /// Cancels one pending attempt by id and reports its terminal
    /// disposition.
    pub fn cancel(&mut self, attempt_id: BuildAttemptId) -> Option<BuildCoordinatorOutcome> {
        let pending = self.pending.remove(&attempt_id)?;
        let outcome = pending.state.cancel();
        self.counters.cancellations = self.counters.cancellations.saturating_add(1);
        let terminal = match outcome {
            ShortBuildOutcome::Cancelled => BuildCoordinatorOutcome::Cancelled {
                direction: pending.direction,
            },
            other => map_terminal(other, pending.direction),
        };
        Some(terminal)
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
        self.paused = self.pool.consecutive_failures() >= self.failure_threshold;
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

fn map_terminal(outcome: ShortBuildOutcome, direction: BuildDirection) -> BuildCoordinatorOutcome {
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
            direction,
            reason: "multi-record reply rejected",
        },
        ShortBuildOutcome::CryptoFailed => BuildCoordinatorOutcome::InvalidReply {
            direction,
            reason: "cryptography primitive failed",
        },
        ShortBuildOutcome::DeliveryFailed => BuildCoordinatorOutcome::DeliveryFailed {
            direction,
            delivery: RouterDeliveryOutcome::Cancelled,
        },
    }
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

    #[test]
    fn coordinator_starts_with_no_pending_attempts() {
        let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        assert_eq!(coord.pending_len(), 0);
        assert_eq!(coord.counters().installed, 0);
        assert_eq!(coord.counters().pending, 0);
        assert!(!coord.is_paused());
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
}
