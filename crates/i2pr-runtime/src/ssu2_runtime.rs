//! Bounded runtime-owned SSU2 UDP socket and session lifecycle (Plan 158).
//!
//! This module is the production adapter between the runtime-neutral
//! `i2pr-transport-ssu2` state machines and real UDP sockets. It owns
//! Tokio `UdpSocket` objects, admission counters, deadlines, bounded
//! queues, and supervised receive/scheduler children; protocol codecs,
//! handshake sequencing, token lifecycle, and data-phase reliability
//! stay in `i2pr-transport-ssu2`, and link ownership stays in the
//! generic `i2pr-transport::TransportManager`.
//!
//! ```text
//! i2pr-transport-ssu2   pure bounded protocol/state machines
//!        actions/events ↕
//! i2pr-runtime          UdpSocket + time + OS RNG + ChildScope + bounded queues
//!                       ↕
//! i2pr-transport        generic link/resource/delivery manager
//! ```
//!
//! Normative traceability: Plan 158
//! (`plans/implementation/ssu2/158-m8-ssu2-udp-runtime-and-local-session-product.md`).
//! Classical SSU2 v2 only; no public RouterInfo advertisement, no
//! introducer/relay roles, no endpoint migration (Plans 159-160), no
//! router-to-router interoperability claim (Plan 161).
//!
//! Design notes for reviewers:
//!
//! - One loop task per bound socket family performs both receive and
//!   central-scheduler duties over shared state: a single Tokio sleep
//!   recomputed to the earliest handshake/ACK/RTO/idle deadline. There
//!   is no task per dial, no task per packet, and no timer per packet.
//! - The receive path applies cheap checks first (datagram length,
//!   active-session trial match without side effects, intro-key
//!   prevalidation, admission caps) before any DH, allocation of
//!   persistent handshake state, or manager promotion. Unknown random
//!   traffic never creates one persistent object per source:
//!   TokenRequest answers and tokenless-SessionRequest Retries are
//!   stateless (a transient scratch responder), and only a
//!   token-bearing SessionRequest that passes admission creates a
//!   bounded pending entry.
//! - Outbound dials are single-flight per destination address while a
//!   handshake is pending; a second concurrent dial to the same
//!   address is denied with [`Ssu2DialOutcome::ResourceDenied`].
//! - Pending inbound handshakes hold a peer-agnostic
//!   [`i2pr_transport::ResourceClass::PendingHandshakes`] lease from
//!   the shared [`i2pr_transport::TransportResources`] (the peer hash
//!   is unknown until SessionConfirmed validates), then atomically
//!   promote through [`i2pr_transport::TransportManager`] at the
//!   authenticated gate with no await between admission and
//!   registration. Pending outbound dials hold a peer-bound
//!   [`i2pr_transport::PendingHandshake`] for the same gate.
//! - Outbound I2NP delivery is admitted through the generic
//!   [`i2pr_transport::TransportManager`] delivery contract
//!   (`delivery_capability` + `enqueue_on_link`) as an admission gate;
//!   the admitted bytes are then queued into the bounded
//!   [`Ssu2Session`](i2pr_transport_ssu2::Ssu2Session) outbound queue,
//!   which is the transport buffer. Per-session byte depth is observed
//!   through the read-only [`Ssu2Session::outbound_pending`](i2pr_transport_ssu2::Ssu2Session::outbound_pending)
//!   accessor so both bounds hold without double buffering.
//! - Authenticated inbound I2NP messages are handed to the caller
//!   through a bounded channel ([`Ssu2InboundI2np`]); this narrow local
//!   sink matches the intended ownership direction (SSU2 runtime ->
//!   transport-neutral authenticated handoff -> caller/router dispatch)
//!   until full production router dispatch exists.
//! - Dial targets must be loopback literals while non-loopback SSU2
//!   operation is unsupported (Plan 159 owns reachability). The daemon
//!   `[ssu2]` surface stays `enabled = false`; see
//!   `i2pr-daemon/src/config.rs`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::X25519PrivateKey;
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, RouterInfo, SigningPublicKey};
use i2pr_transport::{
    AddressFamily, CandidateDecision, ConfirmedPeerTest, DeliveryRequest, Direction,
    EncodedI2npMessage, FamilyNetworkCondition, LinkCandidate, LinkId, PeerId, PeerTestOutcomeKind,
    PendingHandshake, ReachabilitySignal, ReachabilitySnapshot, ReachabilityState,
    ReachabilityTracker, ResourceClass, TerminationCategory, TransportKind, TransportLimits,
    TransportManager, effective_reachability,
};
use i2pr_transport_ssu2::{
    AddressBlock, AuthenticatedSsu2Session, ClockSkewPolicy, ConfirmedParams, DeadlineKind,
    DropCategory, HandshakeAction, HandshakeReplayCache, Initiator, InitiatorConfig,
    InitiatorSecrets, IntroKey, PATH_CHALLENGE_LENGTH, PathError, PathEvent, PathValidator,
    PeerTestBlock, PeerTestOutcome, PeerTestRole, PeerTestState, Responder, ResponderConfig,
    ResponderParams, RetryAnswer, SessionAction, SessionConfig, SessionEvent, Ssu2AddressClass,
    Ssu2Endpoint, Ssu2PublicKey, Ssu2RouterAddress, TerminateReason, TokenStore,
    build_out_of_session_peer_test, constants, parse_hole_punch, parse_out_of_session_peer_test,
    parse_session_request, parse_token_request, peer_test_conn_ids, retry_response_budget,
};
use rand_core::{OsRng, TryRngCore};
use tokio::net::UdpSocket;
use tokio::sync::{Notify, mpsc, oneshot};
use zeroize::Zeroize;

use crate::ntcp2_runtime::{DialAdmission, DialBackoffConfig, DialKey};
use crate::ntcp2_runtime::{DialBackoffDecision, IpPrefixPolicy};
use crate::ssu2_peer_relay::{Ssu2PeerRelayConfig, Ssu2PeerRelayService};
use crate::{
    BandwidthDirection, BandwidthGovernor, BandwidthPeerKey, BandwidthTrafficClass,
    CancellationToken, ChildScope, ChildTaskFailure,
};
use i2pr_core::CancellationReason;

/// Hard maximum for any Plan 158 runtime duration.
pub const MAX_SSU2_RUNTIME_DURATION: Duration = Duration::from_secs(3_600);

/// The keepalive's unanswered deadline, in intervals.
///
/// Plan 522: this is the transport's own policy, read rather than
/// restated, so the runtime and the transport cannot disagree about
/// when an unanswered keepalive ends a session.
pub const KEEPALIVE_TIMEOUT_INTERVALS: u32 =
    i2pr_transport_ssu2::constants::DATA_KEEPALIVE_TIMEOUT_INTERVALS as u32;

/// The keepalive interval, in milliseconds.
///
/// Plan 522: this is the transport's own cadence, re-exported so the
/// daemon reads the one value without taking a second dependency on
/// the transport crate.
pub const KEEPALIVE_INTERVAL_MS: u64 = i2pr_transport_ssu2::constants::DATA_KEEPALIVE_INTERVAL_MS;
/// Absolute ceiling for pending handshakes (both directions).
pub const MAX_SSU2_PENDING_CEILING: usize = 1024;
/// Absolute ceiling for active sessions.
pub const MAX_SSU2_ACTIVE_CEILING: usize = 1024;
/// Absolute ceiling for staged outbound datagrams.
pub const MAX_SSU2_STAGED_DATAGRAMS_CEILING: usize = 4096;
/// Absolute ceiling for staged outbound bytes.
pub const MAX_SSU2_STAGED_BYTES_CEILING: usize = 64 * 1024 * 1024;
/// Absolute ceiling for the inbound authenticated-I2NP handoff queue.
pub const MAX_SSU2_INBOUND_QUEUE_CEILING: usize = 1024;
/// Default local MTU constraint recorded into sessions.
pub const SSU2_DEFAULT_MTU: u16 = 1280;
/// SessionConfirmed fragment budget used for local RouterInfo emission.
pub const SSU2_CONFIRMED_MTU_PAYLOAD: usize = 1000;
/// Maximum age of a RouterInfo accepted by `install_local_router_info`.
///
/// Mirrors the Plan 103 `DEFAULT_MAX_AGE` window enforced by
/// `i2pr-netdb`; the constant is duplicated here because the
/// dependency direction forbids `i2pr-runtime` from depending on
/// `i2pr-netdb`.
pub const LOCAL_ROUTER_INFO_MAX_AGE_SECS: u64 = 24 * 60 * 60;
/// Maximum future publication skew accepted by `install_local_router_info`.
/// Mirrors the Plan 103 `DEFAULT_MAX_FUTURE_SKEW` window (see above).
pub const LOCAL_ROUTER_INFO_MAX_FUTURE_SKEW_SECS: u64 = 60 * 60;
/// Maximum encoded length accepted by `install_local_router_info`.
/// Mirrors the Plan 103 `DEFAULT_MAX_ENCODED_LEN` bound (see above); the
/// establishment decode cap below is stricter and fires first.
pub const LOCAL_ROUTER_INFO_MAX_ENCODED_LEN: usize = 16 * 1024;
/// Grace period for a cached-token SessionRequest before the dial
/// falls back to the tokenless Retry path within the same attempt.
pub const SSU2_CACHED_TOKEN_GRACE: Duration = Duration::from_millis(1_500);
/// Maximum cached future-handshake tokens retained per peer.
pub const SSU2_TOKEN_CACHE_PER_PEER: usize = 2;
/// Maximum peer token-cache entries before deterministic eviction.
pub const SSU2_TOKEN_CACHE_PEERS: usize = 128;
/// Maximum TokenRequest sources tracked for per-source rate limiting.
pub const SSU2_TOKEN_REQUEST_SOURCES: usize = 1024;
/// Per-source TokenRequest answers admitted per one-second window.
pub const SSU2_TOKEN_REQUESTS_PER_SECOND: u32 = 8;
/// Maximum links retained for one peer by the owned manager.
pub const SSU2_LINKS_PER_PEER: u64 = 8;
/// Per-link queue message ceiling inside the owned manager.
pub const SSU2_MANAGER_MESSAGES_PER_LINK: u64 = 64;
/// Per-link queue byte ceiling inside the owned manager.
pub const SSU2_MANAGER_BYTES_PER_LINK: u64 = 256 * 1024;
/// Bounded dial-backoff entries retained by the service.
pub const SSU2_BACKOFF_ENTRIES: usize = 256;
/// Maximum outbound datagrams drained per scheduler wake per session.
pub const SSU2_MAX_DRAIN_PER_SESSION: usize = 4;
/// Maximum outstanding path-validation candidates service-wide.
///
/// Per-session tables are bounded by the protocol crate; this ceiling
/// additionally bounds the service-wide challenge surface under
/// spoofed-source floods.
pub const SSU2_MAX_PATH_CANDIDATES_GLOBAL: usize = 256;

/// How often the path in use is asked to prove itself again.
///
/// Plan 549: a validated path keeps reachability class 2 only while it is
/// being re-proved, and the transport's observation TTL is 30 minutes. A
/// ten-minute cadence leaves two further attempts inside every window, so one
/// lost datagram cannot cost the class, and it is far below the TTL, so a
/// healthy session never lapses. The probe is a liveness check on the path
/// already carrying traffic; the reference answers every PathChallenge
/// unconditionally, so no peer state changes.
pub const PATH_REVALIDATION_INTERVAL_MS: u64 = 10 * 60 * 1_000;

/// A bounded category for SSU2 runtime limit validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2LimitKind {
    /// All pending handshakes (both directions).
    PendingHandshakes,
    /// Pending handshakes for one exact IP.
    PendingPerIp,
    /// Pending handshakes for one subnet prefix.
    PendingPerSubnet,
    /// Authenticated sessions owned by the runtime.
    ActiveSessions,
    /// Authenticated sessions with one exact-IP peer source.
    ActivePerIp,
    /// Authenticated sessions within one subnet prefix.
    ActivePerSubnet,
    /// Staged outbound datagrams awaiting socket write.
    OutboundDatagrams,
    /// Staged outbound datagram bytes awaiting socket write.
    OutboundBytes,
    /// Inbound authenticated I2NP messages awaiting dispatch.
    InboundI2np,
    /// Inbound authenticated relay blocks awaiting the role owner.
    InboundRelay,
}

/// Runtime resource limits for SSU2 services.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2RuntimeLimits {
    /// Maximum pending (unauthenticated) handshakes, both directions.
    pub max_pending_handshakes: usize,
    /// Maximum pending handshakes for one exact IP.
    pub max_pending_per_ip: usize,
    /// Maximum pending handshakes for one subnet prefix.
    pub max_pending_per_subnet: usize,
    /// Maximum active (authenticated) sessions.
    pub max_active_sessions: usize,
    /// Maximum active sessions with one exact-IP peer source.
    pub max_active_per_ip: usize,
    /// Maximum active sessions within one subnet prefix.
    pub max_active_per_subnet: usize,
    /// Maximum staged outbound datagrams awaiting socket write.
    pub max_outbound_datagrams: usize,
    /// Maximum staged outbound datagram bytes awaiting socket write.
    pub max_outbound_bytes: usize,
    /// Maximum inbound authenticated I2NP messages awaiting dispatch.
    pub max_inbound_i2np_queue: usize,
    /// Maximum inbound authenticated relay blocks awaiting the role
    /// owner (Plan 490).
    pub max_inbound_relay_queue: usize,
}

impl Default for Ssu2RuntimeLimits {
    fn default() -> Self {
        Self {
            max_pending_handshakes: 64,
            max_pending_per_ip: 4,
            max_pending_per_subnet: 16,
            max_active_sessions: 64,
            max_active_per_ip: 8,
            max_active_per_subnet: 32,
            max_outbound_datagrams: 256,
            max_outbound_bytes: 1024 * 1024,
            max_inbound_i2np_queue: 64,
            max_inbound_relay_queue: 16,
        }
    }
}

impl Ssu2RuntimeLimits {
    /// Validates all nonzero, ordered limits against hard ceilings.
    pub fn validate(self) -> Result<Self, Ssu2RuntimeConfigError> {
        let values = [
            (
                Ssu2LimitKind::PendingHandshakes,
                self.max_pending_handshakes,
            ),
            (Ssu2LimitKind::PendingPerIp, self.max_pending_per_ip),
            (Ssu2LimitKind::PendingPerSubnet, self.max_pending_per_subnet),
            (Ssu2LimitKind::ActiveSessions, self.max_active_sessions),
            (Ssu2LimitKind::ActivePerIp, self.max_active_per_ip),
            (Ssu2LimitKind::ActivePerSubnet, self.max_active_per_subnet),
            (
                Ssu2LimitKind::OutboundDatagrams,
                self.max_outbound_datagrams,
            ),
            (Ssu2LimitKind::OutboundBytes, self.max_outbound_bytes),
            (Ssu2LimitKind::InboundI2np, self.max_inbound_i2np_queue),
            (Ssu2LimitKind::InboundRelay, self.max_inbound_relay_queue),
        ];
        for (kind, value) in values {
            if value == 0 {
                return Err(Ssu2RuntimeConfigError::ZeroLimit { kind });
            }
        }
        let ceilings = [
            (
                Ssu2LimitKind::PendingHandshakes,
                self.max_pending_handshakes,
                MAX_SSU2_PENDING_CEILING,
            ),
            (
                Ssu2LimitKind::ActiveSessions,
                self.max_active_sessions,
                MAX_SSU2_ACTIVE_CEILING,
            ),
            (
                Ssu2LimitKind::OutboundDatagrams,
                self.max_outbound_datagrams,
                MAX_SSU2_STAGED_DATAGRAMS_CEILING,
            ),
            (
                Ssu2LimitKind::OutboundBytes,
                self.max_outbound_bytes,
                MAX_SSU2_STAGED_BYTES_CEILING,
            ),
            (
                Ssu2LimitKind::InboundI2np,
                self.max_inbound_i2np_queue,
                MAX_SSU2_INBOUND_QUEUE_CEILING,
            ),
            (
                Ssu2LimitKind::InboundRelay,
                self.max_inbound_relay_queue,
                MAX_SSU2_INBOUND_QUEUE_CEILING,
            ),
        ];
        for (kind, value, maximum) in ceilings {
            if value > maximum {
                return Err(Ssu2RuntimeConfigError::LimitTooLarge { kind, maximum });
            }
        }
        if self.max_pending_per_ip > self.max_pending_handshakes
            || self.max_pending_per_subnet > self.max_pending_handshakes
        {
            return Err(Ssu2RuntimeConfigError::InconsistentLimits);
        }
        if self.max_active_per_ip > self.max_active_sessions
            || self.max_active_per_subnet > self.max_active_sessions
        {
            return Err(Ssu2RuntimeConfigError::InconsistentLimits);
        }
        Ok(self)
    }
}

/// Bounded timing policy for handshake, dial, idle, queue, drain, and
/// central-scheduler work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2RuntimeDeadlines {
    /// Total handshake timeout (backstop behind machine schedules).
    pub handshake: Duration,
    /// Total outbound dial timeout, including cached-token fallback.
    pub dial: Duration,
    /// Data-phase idle timeout handed to sessions.
    pub idle: Duration,
    /// The outbound session keepalive interval.
    ///
    /// Plan 522: `None` disables the keepalive entirely, so the
    /// default router's wire behaviour is unchanged. When it is
    /// set, the interval must be strictly below the idle timeout:
    /// the keepalive's own unanswered deadline has to fall before
    /// the idle timeout, or the two would disagree about when a
    /// session dies.
    pub keepalive: Option<Duration>,
    /// Queue admission timeout for `send_i2np`.
    pub queue_wait: Duration,
    /// Graceful close drain timeout.
    pub drain: Duration,
    /// Upper bound for one central-scheduler sleep.
    pub scheduler_poll_max: Duration,
}

impl Default for Ssu2RuntimeDeadlines {
    fn default() -> Self {
        Self {
            handshake: Duration::from_secs(20),
            dial: Duration::from_secs(30),
            idle: Duration::from_secs(300),
            keepalive: None,
            queue_wait: Duration::from_secs(5),
            drain: Duration::from_secs(5),
            scheduler_poll_max: Duration::from_millis(200),
        }
    }
}

impl Ssu2RuntimeDeadlines {
    /// Validates all configured durations.
    pub fn validate(self) -> Result<Self, Ssu2RuntimeConfigError> {
        let values = [
            ("handshake", self.handshake),
            ("dial", self.dial),
            ("idle", self.idle),
            ("queue_wait", self.queue_wait),
            ("drain", self.drain),
            ("scheduler_poll_max", self.scheduler_poll_max),
        ];
        for (field, value) in values {
            if value.is_zero() {
                return Err(Ssu2RuntimeConfigError::ZeroDeadline { field });
            }
            if value > MAX_SSU2_RUNTIME_DURATION {
                return Err(Ssu2RuntimeConfigError::DeadlineTooLong { field });
            }
        }
        if self.dial < self.handshake {
            return Err(Ssu2RuntimeConfigError::DialShorterThanHandshake);
        }
        // Plan 522: the keepalive's own unanswered deadline must
        // fall strictly before the idle timeout.
        if let Some(keepalive) = self.keepalive {
            if keepalive.is_zero() {
                return Err(Ssu2RuntimeConfigError::ZeroDeadline { field: "keepalive" });
            }
            if keepalive > MAX_SSU2_RUNTIME_DURATION {
                return Err(Ssu2RuntimeConfigError::DeadlineTooLong { field: "keepalive" });
            }
            if keepalive.saturating_mul(KEEPALIVE_TIMEOUT_INTERVALS) >= self.idle {
                return Err(Ssu2RuntimeConfigError::KeepaliveTooLong);
            }
        }
        Ok(self)
    }
}

/// The router's own reachability policy.
///
/// Plan 535: the transport's model holds every reachability signal as a
/// **timed observation**, and the observation lifetime is the transport's own
/// (`DEFAULT_OBSERVATION_TTL`). The runtime uses that value unless it is
/// configured with another one, and the runtime then reads the lifetime back
/// from the tracker's **own policy**, so the expiry rule keeps exactly one
/// owner and the runtime cannot drift from it.
///
/// The parameter exists so the behaviour is provable: a router configured
/// with a short lifetime must keep its own class counted **past** that
/// lifetime, and the lane proves exactly that. The production default is the
/// transport's own value, and the daemon's configuration surface is
/// unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2RuntimeReachability {
    /// How long a reachability observation stays counted.
    pub observation_ttl: Duration,
}

impl Default for Ssu2RuntimeReachability {
    fn default() -> Self {
        Self {
            observation_ttl: i2pr_transport::DEFAULT_OBSERVATION_TTL,
        }
    }
}

impl Ssu2RuntimeReachability {
    /// Validates the configured observation lifetime.
    ///
    /// A zero lifetime would expire every observation immediately and a
    /// lifetime beyond the runtime's own horizon would keep evidence for
    /// ever, so both are refused.
    pub fn validate(self) -> Result<(), Ssu2RuntimeConfigError> {
        const FIELD: &str = "reachability_observation_ttl";
        if self.observation_ttl.is_zero() {
            return Err(Ssu2RuntimeConfigError::ZeroDeadline { field: FIELD });
        }
        if self.observation_ttl > MAX_SSU2_RUNTIME_DURATION {
            return Err(Ssu2RuntimeConfigError::DeadlineTooLong { field: FIELD });
        }
        Ok(())
    }
}

/// Plan 535: whether the router's own bound-address fact must be
/// re-asserted now.
///
/// The rule is **pure**, so it can be proved rather than described: the
/// re-assertion is due once the record has aged past **half** the
/// observation lifetime. Half is chosen so the re-assertion always happens
/// while the previous record is still live, and so that a missed
/// maintenance pass cannot lapse the class.
///
/// The rule **fails closed**: a fact that was never asserted is due, and a
/// zero lifetime is due on every pass, because a class that is re-asserted
/// too often is harmless and a class that lapses is not.
pub fn explicit_bind_refresh_due(
    recorded_at: Option<Duration>,
    now: Duration,
    observation_ttl: Duration,
) -> bool {
    let Some(recorded_at) = recorded_at else {
        return true;
    };
    let half = observation_ttl / 2;
    now.saturating_sub(recorded_at) >= half
}

/// Complete validated SSU2 runtime policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ssu2RuntimeConfig {
    /// Resource limits.
    pub limits: Ssu2RuntimeLimits,
    /// Timing limits.
    pub deadlines: Ssu2RuntimeDeadlines,
    /// Subnet accounting policy.
    pub prefixes: IpPrefixPolicy,
    /// The router's own reachability policy (Plan 535).
    ///
    /// The observation lifetime is the transport's own by default; the
    /// runtime reads it back from the tracker's policy rather than
    /// restating it.
    pub reachability: Ssu2RuntimeReachability,
    /// How often the path in use is asked to prove itself again (Plan 549).
    ///
    /// The default is [`PATH_REVALIDATION_INTERVAL_MS`], which is a third of
    /// the transport's observation lifetime so one lost datagram cannot cost
    /// the validated-path class. The field exists because the cadence is the
    /// only way a lane can observe the probe inside a test's lifetime; the
    /// daemon passes the default, so **no operator key is added**. A zero
    /// value is treated as the default rather than as "as fast as possible".
    pub path_revalidation_interval_ms: u64,
    /// Permits explicit-bind corroboration in the reachability tracker.
    ///
    /// When false (default) the tracker ignores `LocalConfiguredBind`
    /// observations, preserving the conservative posture that an
    /// explicit bind never attests reachability by itself. When true,
    /// an explicitly configured bind recorded through
    /// [`Ssu2RuntimeService::note_explicit_bind_for_controlled_qualification`]
    /// counts as one corroboration class alongside real handshake
    /// observations. The daemon sets this because its SSU2 service is
    /// always an explicitly configured loopback bind; recording still
    /// requires the narrow controlled-qualification call, so normal
    /// operation is unchanged.
    pub explicit_bind_corroboration: bool,
    /// Permits publication of the router's own **bound** SSU2 address
    /// (Plan 546).
    ///
    /// When false (the default) `publication_material` and
    /// [`Ssu2RuntimeService::note_explicit_bind_for_controlled_qualification`]
    /// refuse every non-loopback bound socket, so the only address the router
    /// can publish is `127.0.0.1` and publication is confined to a loopback
    /// lab. When true, the bound socket's own address may be published.
    ///
    /// **This is an operator precondition, not a switch.** The reachability
    /// the record then carries comes from the `LocalConfiguredBind`
    /// corroboration, which means "the socket is bound to this address" -- not
    /// "peers can reach this address". Those are the same thing when the host
    /// is directly reachable at its bind (a lab LAN, or a public host with no
    /// NAT in front of it), and they are **not** the same thing behind NAT,
    /// where peers see the NAT's address rather than the bind's. A NAT'd host
    /// must leave this false: a router that publishes a reachability it does
    /// not have gets blocked by the peers it wastes, and a widely blocked
    /// address makes live-network testing harder rather than easier.
    ///
    /// The public-address arm of `publication_material`'s reachability test is
    /// **not** relaxed by this field, and neither is the expiry test.
    pub publish_bound_address: bool,
    /// Permits the controlled peer-test evidence path (Plan 283).
    ///
    /// When false (default) the service ignores every inbound
    /// peer-test datagram, refuses every controlled peer-test call,
    /// and owns no live controlled test: session behavior is exactly
    /// the pre-283 shape. When true, the narrow
    /// `*_controlled_peer_test` / `queue_controlled_address` APIs and
    /// the single-flight controlled exchange may run. The daemon sets
    /// this only alongside `explicit_bind_corroboration` on the
    /// controlled M12 qualification path; normal operation is
    /// unchanged.
    pub controlled_peer_test: bool,
    /// Enables bounded, in-memory source receipts for an explicitly selected
    /// private qualification profile. Disabled for ordinary router operation.
    pub qualification_observer: bool,
    /// Enables the introducer (Bob) role inside the peer/relay
    /// coordinator (Plan 490).
    ///
    /// When false (default) the runtime owns a disabled introducer: every
    /// inbound Relay Request is refused with `ServiceDisabled` and no
    /// relay-tag or request state exists. When true, the role's state
    /// machine is live; the daemon's `ssu2.introducer_service` key
    /// (default off) is the only production gate.
    pub introducer_service: bool,
    /// Enables the requester (Alice) role inside the peer/relay
    /// coordinator (Plan 531).
    ///
    /// When false (default) the runtime owns no requester path at all:
    /// the inbound hole-punch route does not exist, no relay request can
    /// be started, and the relay-firewalled indication has no producer.
    /// When true, the requester's seam is live; the daemon's
    /// `ssu2.relay_requester` key (default off) is the only production
    /// gate.
    pub relay_requester: bool,

    /// Enables the target (Charlie) role inside the peer/relay
    /// coordinator (Plan 532).
    ///
    /// When false (default) the runtime owns no target path at all: an
    /// inbound Relay Introduction is routed to the introducer's handoff
    /// exactly as before, and the introducer owner refuses it as not its
    /// role. When true, the introduction reaches the target's own bounded
    /// handoff and the target's seam is live; the daemon's
    /// `ssu2.relay_target` key (default off) is the only production gate.
    pub relay_target: bool,
}

impl Ssu2RuntimeConfig {
    /// Validates and returns this configuration.
    pub fn validate(self) -> Result<Self, Ssu2RuntimeConfigError> {
        self.limits.validate()?;
        self.deadlines.validate()?;
        self.reachability.validate()?;
        IpPrefixPolicy::new(self.prefixes.ipv4_prefix, self.prefixes.ipv6_prefix)
            .map_err(|_| Ssu2RuntimeConfigError::InvalidPrefix)?;
        Ok(self)
    }
}

/// Returns `true` when the runtime may publish the given bound endpoint.
///
/// Plan 546. This is the **single** authority for "may this socket's address
/// appear in a RouterInfo", and both publication entry points call it:
/// [`Ssu2RuntimeService::publication_material`] and
/// [`Ssu2RuntimeService::note_explicit_bind_for_controlled_qualification`].
///
/// An unbound (`port == 0`) or unspecified (`0.0.0.0`) endpoint is never
/// publishable. Loopback is always publishable, because a loopback address is
/// the one address a record can name without making a claim about the outside
/// world. Any other bound address is publishable **only** when the operator set
/// [`Ssu2RuntimeConfig::publish_bound_address`], which is their statement that
/// the host is directly reachable at that address.
///
/// The predicate does **not** check reachability or its expiry; callers do that
/// separately, and this plan does not relax either.
pub fn may_publish_bound_address(bound: std::net::SocketAddr, publish_bound_address: bool) -> bool {
    if bound.port() == 0 || bound.ip().is_unspecified() {
        return false;
    }
    bound.ip().is_loopback() || publish_bound_address
}

/// Configuration validation failure for the runtime SSU2 adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2RuntimeConfigError {
    /// A limit was zero.
    ZeroLimit { kind: Ssu2LimitKind },
    /// A limit exceeded an infrastructure ceiling.
    LimitTooLarge {
        /// Limit category.
        kind: Ssu2LimitKind,
        /// Maximum permitted value.
        maximum: usize,
    },
    /// Per-scope values cannot be satisfied by their global ceiling.
    InconsistentLimits,
    /// A deadline was zero.
    ZeroDeadline { field: &'static str },
    /// A deadline exceeded the runtime horizon.
    DeadlineTooLong { field: &'static str },
    /// A prefix was outside its address-family width.
    InvalidPrefix,
    /// The dial timeout cannot cover a full handshake attempt.
    DialShorterThanHandshake,
    /// The keepalive interval cannot fall before the idle timeout.
    ///
    /// Plan 522: the keepalive's unanswered deadline has to be
    /// strictly earlier than the session's own idle timeout, or
    /// the two would disagree about when a session dies.
    KeepaliveTooLong,
    /// Local identity material was rejected.
    InvalidIdentity,
    /// A dial target failed validation.
    InvalidDialTarget,
}

impl fmt::Display for Ssu2RuntimeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit { kind } => write!(formatter, "zero SSU2 runtime limit: {kind:?}"),
            Self::LimitTooLarge { kind, maximum } => {
                write!(formatter, "SSU2 runtime limit {kind:?} exceeds {maximum}")
            }
            Self::InconsistentLimits => formatter.write_str("inconsistent SSU2 runtime limits"),
            Self::ZeroDeadline { field } => write!(formatter, "zero SSU2 deadline: {field}"),
            Self::DeadlineTooLong { field } => {
                write!(formatter, "SSU2 deadline exceeds its bound: {field}")
            }
            Self::InvalidPrefix => formatter.write_str("invalid SSU2 IP prefix width"),
            Self::DialShorterThanHandshake => {
                formatter.write_str("SSU2 dial timeout is shorter than the handshake timeout")
            }
            Self::KeepaliveTooLong => {
                formatter.write_str("SSU2 keepalive interval does not fall before the idle timeout")
            }
            Self::InvalidIdentity => formatter.write_str("invalid SSU2 local identity material"),
            Self::InvalidDialTarget => formatter.write_str("invalid SSU2 dial target"),
        }
    }
}

impl std::error::Error for Ssu2RuntimeConfigError {}

/// Failure of a controlled peer-test evidence call (Plan 283).
///
/// Every variant fails closed without recording reachability evidence:
/// refused calls never touch the tracker, and a failed exchange leaves
/// whatever evidence already recorded (bind observations, completed
/// outcomes) untouched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlledPeerTestError {
    /// The service was not configured with `controlled_peer_test`.
    NotPermitted,
    /// The service is shut down or its state is unavailable.
    Unavailable,
    /// Another controlled test is already live (single-flight).
    AlreadyRunning,
    /// No controlled test is live for this call.
    NoControlledTest,
    /// The nonce does not match the live controlled test.
    NonceMismatch,
    /// A role/hash/endpoint binding check failed.
    BindingMismatch,
    /// The block message number does not fit this egress path
    /// (Msgs 1–4 in-session only, Msgs 5–7 out-of-session only).
    WrongTransport,
    /// No single live session exists for the peer.
    NoSession,
    /// The reported endpoint is not the live observed peer address.
    NotObserved,
    /// A target, endpoint, or key failed validation.
    InvalidTarget,
    /// Staging, quota, or randomness was unavailable.
    ResourceDenied,
    /// The driver configuration (timeouts) failed validation.
    InvalidDriverConfig,
    /// A driver signing, key-generation, or block-build step failed.
    ExchangeCrypto,
    /// A helper handshake failed or was denied.
    ExchangeHandshake,
    /// A driver step deadline elapsed.
    ExchangeTimeout,
    /// The driver was cancelled.
    ExchangeCancelled,
    /// The exchange completed but its verdict was not a confirmation.
    ///
    /// Plan 528: the driver no longer raises this for a verdict it
    /// received -- every verdict is returned as a typed fact. The
    /// **caller** raises it when a returned verdict is not a
    /// confirmation and it therefore cannot honour the claim it asked
    /// the exchange to prove.
    ExchangeNotConfirmed,
    /// The driver observed a protocol deviation (unexpected forward,
    /// state, or outcome shape).
    ExchangeProtocol,
}

impl fmt::Display for ControlledPeerTestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotPermitted => "controlled peer test is not permitted on this service",
            Self::Unavailable => "controlled peer test service state is unavailable",
            Self::AlreadyRunning => "a controlled peer test is already running",
            Self::NoControlledTest => "no controlled peer test is running",
            Self::NonceMismatch => "nonce does not match the controlled peer test",
            Self::BindingMismatch => "role, hash, or endpoint does not bind the controlled test",
            Self::WrongTransport => "peer-test block uses the wrong egress path",
            Self::NoSession => "no live session exists for the controlled peer",
            Self::NotObserved => "reported endpoint is not the observed peer address",
            Self::InvalidTarget => "controlled peer test target is invalid",
            Self::ResourceDenied => "controlled peer test resource was denied",
            Self::InvalidDriverConfig => "controlled peer test driver configuration is invalid",
            Self::ExchangeCrypto => "controlled peer test crypto step failed",
            Self::ExchangeHandshake => "controlled peer test handshake failed",
            Self::ExchangeTimeout => "controlled peer test step timed out",
            Self::ExchangeCancelled => "controlled peer test was cancelled",
            Self::ExchangeNotConfirmed => "controlled peer test did not confirm",
            Self::ExchangeProtocol => "controlled peer test observed a protocol deviation",
        })
    }
}

impl std::error::Error for ControlledPeerTestError {}

/// Single-flight controlled peer-test context (Plan 283).
///
/// Lives only while the controlled driver runs one exchange: the
/// role/hash/endpoint bindings fixed at start plus the terminal table
/// outcome once the exchange completes. At most one context exists per
/// service; the driver closes it explicitly on every exit path.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ControlledPeerTestContext {
    nonce: u32,
    role: PeerTestRole,
    alice_hash: [u8; 32],
    bob_hash: [u8; 32],
    charlie_hash: [u8; 32],
    alice_endpoint: Ssu2Endpoint,
    outcome: Option<PeerTestOutcome>,
    /// Latest transport-only forward (Msg 1 or 2) with its real
    /// sender endpoint, drained by the driver to advance causally.
    last_forward: Option<(PeerTestBlock, Ssu2Endpoint)>,
}

/// Local SSU2 identity material for one runtime service instance.
///
/// The caller (later: daemon composition with NetDB identity plumbing)
/// supplies the transport static secret, the intro key, the canonical
/// router hash, and the complete signed RouterInfo bytes that carry a
/// matching SSU2 address. Secrets never appear in diagnostics.
pub struct Ssu2IdentityMaterial {
    /// Canonical RouterIdentity hash of this router.
    pub router_hash: Hash,
    /// Raw transport static secret (X25519, 32 bytes).
    pub static_secret_bytes: [u8; 32],
    /// Local intro key for inbound header protection.
    pub intro_key: IntroKey,
    /// Complete signed RouterInfo bytes carrying the matching SSU2
    /// address (emitted in SessionConfirmed).
    pub router_info: Vec<u8>,
}

impl fmt::Debug for Ssu2IdentityMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ssu2IdentityMaterial(<redacted>)")
    }
}

/// A validated outbound SSU2 dial target.
///
/// Construction validates the address before any socket activity: the
/// port must be nonzero, the IP must not be unspecified, the address
/// must be loopback for the ordinary constructor, or RFC1918 IPv4 for the
/// explicitly scoped private-LAN constructor, and the peer reference must
/// match the expected router hash.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2DialTarget {
    peer: PeerId,
    expected_router_hash: Hash,
    address: SocketAddr,
    responder_static: Ssu2PublicKey,
    responder_intro: IntroKey,
}

/// Dial-target validation failure (no socket activity performed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2DialTargetError {
    /// UDP port zero cannot be dialed.
    ZeroPort,
    /// The unspecified address cannot be dialed.
    UnspecifiedIp,
    /// Non-loopback dials are unsupported in this milestone.
    NotLoopback,
    /// The private-LAN qualification constructor accepts RFC1918 IPv4 only.
    NotPrivateLanIpv4,
    /// The peer reference does not match the expected router hash.
    PeerHashMismatch,
}

impl fmt::Display for Ssu2DialTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroPort => "SSU2 dial port must be nonzero",
            Self::UnspecifiedIp => "SSU2 dial address must not be unspecified",
            Self::NotLoopback => "SSU2 dial address must be loopback in this milestone",
            Self::NotPrivateLanIpv4 => "SSU2 private-LAN dial address must be RFC1918 IPv4",
            Self::PeerHashMismatch => "SSU2 dial peer does not match its expected router hash",
        })
    }
}

impl std::error::Error for Ssu2DialTargetError {}

impl Ssu2DialTarget {
    /// Validates a dial target without touching a socket.
    pub fn new(
        peer: PeerId,
        expected_router_hash: Hash,
        address: SocketAddr,
        responder_static: Ssu2PublicKey,
        responder_intro: IntroKey,
    ) -> Result<Self, Ssu2DialTargetError> {
        if address.port() == 0 {
            return Err(Ssu2DialTargetError::ZeroPort);
        }
        if address.ip().is_unspecified() {
            return Err(Ssu2DialTargetError::UnspecifiedIp);
        }
        if !address.ip().is_loopback() {
            return Err(Ssu2DialTargetError::NotLoopback);
        }
        if peer.hash() != expected_router_hash {
            return Err(Ssu2DialTargetError::PeerHashMismatch);
        }
        Ok(Self {
            peer,
            expected_router_hash,
            address,
            responder_static,
            responder_intro,
        })
    }

    /// Validates a direct dial target for an explicitly configured private-LAN
    /// qualification run. This constructor accepts only RFC1918 IPv4 literals;
    /// the ordinary [`Self::new`] path remains loopback-only. It does not grant
    /// address publication or make public/IPv6 endpoints dialable.
    pub fn new_private_lan(
        peer: PeerId,
        expected_router_hash: Hash,
        address: SocketAddr,
        responder_static: Ssu2PublicKey,
        responder_intro: IntroKey,
    ) -> Result<Self, Ssu2DialTargetError> {
        if address.port() == 0 {
            return Err(Ssu2DialTargetError::ZeroPort);
        }
        if address.ip().is_unspecified() {
            return Err(Ssu2DialTargetError::UnspecifiedIp);
        }
        if !matches!(address.ip(), std::net::IpAddr::V4(ip) if ip.is_private()) {
            return Err(Ssu2DialTargetError::NotPrivateLanIpv4);
        }
        if peer.hash() != expected_router_hash {
            return Err(Ssu2DialTargetError::PeerHashMismatch);
        }
        Ok(Self {
            peer,
            expected_router_hash,
            address,
            responder_static,
            responder_intro,
        })
    }

    /// Returns the target peer reference.
    pub const fn peer(self) -> PeerId {
        self.peer
    }

    /// Returns the expected responder router hash.
    pub const fn expected_router_hash(self) -> Hash {
        self.expected_router_hash
    }

    /// Returns the literal dial address.
    pub const fn address(self) -> SocketAddr {
        self.address
    }

    /// Returns the responder static public key.
    pub const fn responder_static(self) -> Ssu2PublicKey {
        self.responder_static
    }

    /// Returns the responder intro key.
    pub const fn responder_intro(self) -> IntroKey {
        self.responder_intro
    }
}

/// Typed result of an outbound SSU2 dial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2DialOutcome {
    /// The dial found no bound socket, failed authentication, or hit
    /// an unrecoverable transport error.
    Failed,
    /// Admission (pending caps, per-address single-flight, backoff, or
    /// manager budget) rejected the attempt.
    ResourceDenied,
    /// The caller cancelled the dial.
    Cancelled,
    /// The dial deadline elapsed.
    Timeout,
}

impl fmt::Display for Ssu2DialOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Failed => "SSU2 dial failed",
            Self::ResourceDenied => "SSU2 dial denied by admission",
            Self::Cancelled => "SSU2 dial cancelled",
            Self::Timeout => "SSU2 dial timed out",
        })
    }
}

impl std::error::Error for Ssu2DialOutcome {}

/// Typed result of queueing one I2NP message onto an SSU2 session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2SendOutcome {
    /// The message entered the bounded session outbound queue.
    Accepted,
    /// The session or manager queue is full.
    QueueFull,
    /// A shared resource budget denied admission.
    ResourceDenied,
    /// No active session exists for the peer.
    Closed,
    /// The encoded message exceeds the transport boundary.
    TooLarge,
    /// The caller deadline had already elapsed.
    DeadlineElapsed,
}

impl fmt::Display for Ssu2SendOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Accepted => "SSU2 message accepted",
            Self::QueueFull => "SSU2 session queue is full",
            Self::ResourceDenied => "SSU2 send denied by admission",
            Self::Closed => "SSU2 session is closed",
            Self::TooLarge => "SSU2 message exceeds its bound",
            Self::DeadlineElapsed => "SSU2 send deadline elapsed",
        })
    }
}

impl std::error::Error for Ssu2SendOutcome {}

/// One authenticated inbound I2NP message at the transport-neutral
/// handoff: the runtime never delivers into NetDB/tunnel/client code.
#[derive(Debug)]
pub struct Ssu2InboundI2np {
    /// Exact link the message arrived on.
    pub link_id: LinkId,
    /// Authenticated peer reference.
    pub peer: PeerId,
    /// Complete encoded I2NP bytes, owned by the caller after handoff.
    pub bytes: Vec<u8>,
}

/// One authenticated inbound SSU2 relay block at the transport-neutral
/// handoff (Plan 490).
///
/// The runtime decodes and authenticates the block; it never interprets
/// the relay role. The daemon's role owner owns the disposition.
#[derive(Debug)]
pub struct Ssu2InboundRelay {
    /// Exact link the block arrived on.
    pub link_id: LinkId,
    /// Authenticated peer reference.
    pub peer: PeerId,
    /// The decoded relay block and the role it belongs to.
    pub event: Ssu2RelayEvent,
}

/// The bounded set of relay blocks the runtime can hand off.
#[derive(Debug)]
pub enum Ssu2RelayEvent {
    /// A Relay Request (Alice to Bob).
    Request(i2pr_transport_ssu2::RelayRequestBlock),
    /// A Relay Intro (Bob to Charlie).
    Intro(i2pr_transport_ssu2::RelayIntroBlock),
    /// A Relay Response (Charlie to Bob).
    Response(i2pr_transport_ssu2::RelayResponseBlock),
    /// A relay tag observed on an authenticated session.
    ///
    /// The peer that asked for a tag is the peer that has to learn it, so
    /// the observed tag travels back over the same bounded relay handoff
    /// as the other relay blocks. The runtime creates no state for it.
    Tag(u32),
}

/// One authenticated inbound event on the **requester** side (Plan 531).
///
/// The requester's inbound events are the ones this router's **own**
/// live relay requests attract: a Relay Response from the introducer it
/// named, and an out-of-session HolePunch from the target it named.
/// They have their own bounded handoff because a hole punch names
/// neither a session nor a peer hash, and because a Relay Response that
/// correlates with one of the router's own requests belongs to the
/// requester role rather than to the introducer role.
#[derive(Debug)]
pub enum Ssu2InboundRequesterEvent {
    /// A Relay Response for one of this router's live relay requests.
    Response(Ssu2InboundRelay),
    /// An out-of-session HolePunch for one of this router's live relay
    /// requests.
    HolePunch(Ssu2InboundHolePunch),
}

/// One authenticated inbound out-of-session HolePunch (Plan 531).
///
/// A hole punch travels **out of session**: it is sealed under the
/// receiver's own introduction key and it names no session and no peer
/// hash. It therefore cannot ride the session-shaped relay handoff --
/// [`Ssu2InboundRelay`] carries a link and an authenticated peer, and a
/// hole punch has neither -- so it has its own bounded handoff.
///
/// The runtime decodes and authenticates the datagram; it never
/// interprets the relay role. The daemon's requester owner owns the
/// disposition.
#[derive(Debug)]
pub struct Ssu2InboundHolePunch {
    /// The source address the datagram arrived from.
    ///
    /// The address is the datagram's transport source, not a claim about
    /// a peer: the datagram carries no router hash.
    pub source: SocketAddr,
    /// The decoded hole punch.
    pub message: i2pr_transport_ssu2::HolePunchMessage,
}

/// One authenticated inbound Relay Introduction (Plan 532).
///
/// A Relay Introduction is addressed to **this** router: the introducer
/// forwards it to the peer its tag named. It therefore belongs to the
/// target role and not to the introducer's, and it has its own bounded
/// handoff so the introducer's drain stays byte-identical.
///
/// The runtime decodes and authenticates the datagram; it never
/// interprets the relay role. The daemon's target owner owns the
/// disposition.
#[derive(Debug)]
pub struct Ssu2InboundTarget {
    /// Exact link the block arrived on.
    pub link_id: LinkId,
    /// The authenticated introducer the block arrived from.
    pub peer: PeerId,
    /// The decoded Relay Introduction.
    pub block: i2pr_transport_ssu2::RelayIntroBlock,
}

/// The requester's state after one correlated Relay Response (Plan 531).
///
/// The transport owns the state machine; the runtime names the two
/// outcomes the requester owner acts on, so the daemon never has to
/// name a transport type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2RelayRequestState {
    /// The response is correlated and the request still awaits an answer.
    AwaitingResponse,
    /// The peer accepted; the exchange now awaits the hole punch.
    AwaitingHolePunch,
    /// The peer refused; the request is finished.
    Refused,
}

/// The reason an outbound relay hole punch could not be staged (Plan 532).
///
/// The vocabulary is count-only: it names no peer, no address, no key and
/// no nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2RelayTargetError {
    /// The destination is outside the router's own dial profile.
    DestinationRefused,
    /// The router can name no bound endpoint for the response's address.
    NoLocalEndpoint,
    /// The transport refused to build the hole punch.
    TransportRefused,
    /// The router's own randomness produced no usable packet number.
    NoRandomness,
    /// The runtime's bounded staging queue refused the datagram.
    StagingRefused,
    /// The responder's token store refused the establishment token.
    ///
    /// Plan 533. The token is issued by the runtime's **own** responder
    /// table, and that table is the only authority on a presented token.
    /// A refusal here is a **local** condition about this router's table,
    /// never a fact about a peer.
    EstablishmentRefused,
}

impl Ssu2RelayTargetError {
    /// A stable, count-only reason key for this refusal.
    ///
    /// The key is a label only: it carries no peer identity, no address and
    /// no payload. It exists so a refusal can be counted by reason.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::DestinationRefused => "destination-refused",
            Self::NoLocalEndpoint => "no-local-endpoint",
            Self::TransportRefused => "transport-refused",
            Self::NoRandomness => "no-randomness",
            Self::StagingRefused => "staging-refused",
            Self::EstablishmentRefused => "establishment-refused",
        }
    }
}

/// The reason an outbound relay block could not be carried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2RelayQueueError {
    /// No live session exists for the peer.
    NoSession,
    /// More than one live session exists for the peer; the runtime refuses to guess.
    AmbiguousSession,
    /// The session is terminated.
    SessionTerminated,
    /// The session's control queue is at its ceiling.
    ControlQueueFull,
}

impl Ssu2RelayQueueError {
    /// A stable, count-only reason key for this refusal.
    ///
    /// The key is a label only: it carries no peer identity, no tag and no
    /// payload. It exists so a refusal can be counted by reason.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoSession => "no_session",
            Self::AmbiguousSession => "ambiguous_session",
            Self::SessionTerminated => "session_terminated",
            Self::ControlQueueFull => "control_queue_full",
        }
    }
}

/// Why one relay-tag request was refused, or one tag was not recorded.
///
/// Plan 524. The carriage reasons stay in [`Ssu2RelayQueueError`]: this
/// type carries the two refusals that belong to the *request* rather than
/// to the block's carriage -- the outstanding set's ceiling, and a tag that
/// could not be recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2TagRequestError {
    /// The request could not be queued on the peer's session.
    NotQueued(Ssu2RelayQueueError),
    /// The outstanding-request set is at its ceiling.
    Capacity,
    /// The tag could not be recorded as an authenticated introducer.
    NotRecorded(i2pr_transport_ssu2::IntroducerError),
    /// The router has no bound endpoint to record as its own contact
    /// material, and a record with none would be a lie.
    NoLocalEndpoint,
    /// The router has no usable introduction key to record as its own
    /// contact material.
    NoLocalIntroKey,
}

impl Ssu2TagRequestError {
    /// A stable, count-only reason key for this refusal.
    ///
    /// The key is a label only: it carries no peer identity, no tag and no
    /// payload. It exists so a refusal can be counted by reason.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NotQueued(error) => error.reason(),
            Self::Capacity => "request_capacity",
            Self::NotRecorded(_) => "not_recorded",
            Self::NoLocalEndpoint => "no_local_endpoint",
            Self::NoLocalIntroKey => "no_local_intro_key",
        }
    }
}

/// Plan 524: the ceiling on the peers this router has asked for a relay tag
/// and has not yet seen the answer for.
///
/// A request beyond the bound is **refused**, never queued, and never
/// evicts an older request.
pub const MAX_OUTSTANDING_TAG_REQUESTS: usize = 16;

/// Plan 524: how long a learned relay tag stays usable, in seconds.
///
/// The tag itself carries no expiry. This is the router's own lifetime for
/// a tag it learned over an authenticated session.
pub const RELAY_TAG_INTRODUCER_TTL_SECS: u64 = 3600;

/// One batch of runtime handoff events produced by a datagram or a
/// scheduler tick.
#[derive(Debug, Default)]
pub struct Ssu2InboundBatch {
    /// Authenticated inbound I2NP messages.
    pub i2np: Vec<Ssu2InboundI2np>,
    /// Authenticated inbound relay blocks.
    pub relay: Vec<Ssu2InboundRelay>,
    /// Authenticated inbound requester events (Plan 531).
    pub requester: Vec<Ssu2InboundRequesterEvent>,
    /// Authenticated inbound target events (Plan 532).
    pub target: Vec<Ssu2InboundTarget>,
}

/// Privacy-safe SSU2 service counters (counts only, no payloads, keys,
/// tokens, or endpoint histories).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ssu2Snapshot {
    /// Datagrams dropped by cheap pre-crypto checks or admission.
    pub cheap_drops: u64,
    /// Datagrams rejected by header protection or AEAD.
    pub auth_failures: u64,
    /// Authenticated datagrams dropped by protocol policy.
    pub protocol_drops: u64,
    /// Token validations rejected (unknown/expired/reused/source).
    pub token_rejections: u64,
    /// Handshake replays rejected.
    pub replay_rejections: u64,
    /// Handshakes terminated by timeout or retry exhaustion.
    pub handshake_timeouts: u64,
    /// Sessions promoted through the manager (both directions).
    pub sessions_established: u64,
    /// Sessions closed and released (both directions).
    pub sessions_closed: u64,
    /// Duplicate-link promotions resolved without disturbing live links.
    pub duplicate_resolutions: u64,
    /// Complete I2NP messages delivered to the inbound handoff.
    pub i2np_received: u64,
    /// I2NP messages accepted into session outbound queues.
    pub i2np_sent: u64,
    /// UDP datagrams transmitted.
    pub datagrams_sent: u64,
    /// UDP datagrams received.
    pub datagrams_received: u64,
    /// Transmissions suppressed by the test-only fault policy.
    pub fault_drops: u64,
    /// Inbound messages dropped because the handoff queue was full.
    pub inbound_queue_drops: u64,
    /// Staged outbound datagrams dropped because the staging bound was full.
    pub staging_drops: u64,
    /// Sends that found no active link for the peer.
    pub send_without_link: u64,
    /// PathChallenge datagrams issued to unvalidated candidates.
    pub path_challenges: u64,
    /// PathResponse datagrams sent answering peer challenges.
    pub path_responses: u64,
    /// Sessions migrated after matching PathResponse proof.
    pub path_migrations: u64,
    /// Re-validations of the path in use that completed.
    pub path_revalidations: u64,
    /// Wrong/stale path responses rejected without migration.
    pub path_rejections: u64,
    /// Path candidates expired before proof arrived.
    pub path_expirations: u64,
    /// Candidate admissions denied by quota/budget ceilings.
    pub path_denied: u64,
    /// Relay-tag requests observed on an authenticated session.
    pub relay_tag_requests: u64,
    /// Relay tags minted, bound to the requesting peer and answered.
    pub relay_tags_issued: u64,
    /// Relay tags observed on an authenticated session.
    pub relay_tags_observed: u64,
    /// Plan 524: relay-tag requests this router queued toward a peer.
    pub relay_tag_requests_queued: u64,
    /// Plan 524: relay-tag requests refused because the outstanding set was
    /// at its ceiling.
    pub relay_tag_requests_refused_capacity: u64,
    /// Plan 524: relay tags recorded as authenticated introducers.
    pub relay_tags_recorded: u64,
    /// Plan 524: relay tags refused because the router never asked the peer.
    pub relay_tags_refused_unrequested: u64,
    /// Plan 524: relay tags that answered an outstanding request but could
    /// not be recorded.
    pub relay_tags_refused_record: u64,
    /// Relay blocks queued onto a live session.
    pub relay_blocks_queued: u64,
    /// Relay blocks refused for any reason.
    ///
    /// The per-reason breakdown lives in [`Ssu2RelayCounters`].
    pub relay_blocks_refused: u64,
    /// Conservative router-level reachability state.
    pub reachability: ReachabilityState,
    /// Plan 535: the router's own bound-address fact re-asserted, so the
    /// class does not lapse while the router runs. Counts only.
    pub explicit_bind_refreshes: u64,
    /// Current pending outbound handshakes.
    pub pending_outbound: usize,
    /// Current pending inbound handshakes.
    pub pending_inbound: usize,
    /// Current active sessions.
    pub active_sessions: usize,
    /// Current token-table entries.
    pub token_table_entries: usize,
    /// Current cached future-handshake tokens.
    pub cached_tokens: usize,
    /// Local RouterInfo installation generation (diagnostics only).
    pub local_router_info_generation: u64,
}

/// Counts-only SSU2 relay counters (Plan 499).
///
/// Every relay-tag request the runtime observes and every outbound relay
/// block it refuses is counted here. No field carries a peer hash, a key, a
/// tag value, an endpoint or a payload: counts and reason labels only.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ssu2RelayCounters {
    /// Relay-tag requests observed on an authenticated session.
    pub tag_requests_observed: u64,
    /// Relay tags minted, bound to the requesting peer and answered.
    pub tags_issued: u64,
    /// Tag requests refused because the introducer service is disabled.
    pub tag_refused_disabled: u64,
    /// Tag requests refused because no usable nonzero tag could be minted.
    pub tag_refused_random: u64,
    /// Tag requests refused by the transport's introducer quota.
    pub tag_refused_quota: u64,
    /// Tag answers refused because the session is terminated.
    pub tag_refused_terminated: u64,
    /// Tag answers refused because the control queue is at its ceiling.
    pub tag_refused_control_queue_full: u64,
    /// Relay tags observed on an authenticated session.
    pub tags_observed_inbound: u64,
    /// Relay blocks queued onto a live session.
    pub blocks_queued: u64,
    /// Relay blocks refused because no live session exists for the peer.
    pub blocks_refused_no_session: u64,
    /// Relay blocks refused because more than one live session exists.
    pub blocks_refused_ambiguous_session: u64,
    /// Relay blocks refused because the session is terminated.
    pub blocks_refused_session_terminated: u64,
    /// Relay blocks refused because the control queue is at its ceiling.
    pub blocks_refused_control_queue_full: u64,
}

/// The form an SSU2 publication material represents.
///
/// Plan 525: the router publishes **one** SSU2 address, and its shape
/// depends on the router's own posture. The form is a **value** the
/// caller matches on, never a pair of answers the caller must reconcile
/// and never a refusal the caller must interpret.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2PublicationForm {
    /// The **direct** form: the address carries the router's host and
    /// port, and a peer can dial it.
    Direct,
    /// The **introducer-only** form: the address carries **no** host and
    /// **no** port, and publishes the validated introducer set instead.
    IntroducerOnly,
}

/// Factual public publication material obtained from the live SSU2 owner.
/// Private key material never crosses this runtime boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ssu2PublicationMaterial {
    /// Strictly validated SSU2 address for the actual bound endpoint.
    pub address: RouterAddress,
    /// The form this material represents.
    ///
    /// Plan 525: the caller matches on this and hands the address to the
    /// builder that qualifies **that** form.
    pub form: Ssu2PublicationForm,
    /// Whether the address advertises the router's **introducer role**.
    ///
    /// Plan 526: the specification's Relay capability is the `"C"` letter
    /// in the address's `caps` option. The letter is a **claim** -- a peer
    /// that reads it may ask this router to introduce a third party -- so it
    /// is carried **only** while the role is live. The caller matches on
    /// this field and hands the address to the netdb's builder that
    /// qualifies **that** form.
    pub advertises_introducer_role: bool,
    /// Monotonic expiry of the reachability evidence used to build it.
    pub evidence_expires_at: Duration,
    /// Conservative current reachability classification.
    pub reachability: ReachabilityState,
    /// Bound endpoint family represented by the address.
    pub bound_family: i2pr_transport::AddressFamily,
}

/// Why the router's own confirmed peer test was not recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2PeerTestEvidenceError {
    /// The runtime has shut down.
    Closed,
    /// State was poisoned during shutdown or failure.
    StateUnavailable,
    /// The session the exchange travelled on is gone, so the address family
    /// the evidence belongs to cannot be established.
    NoSession,
}

/// Why the runtime cannot currently expose SSU2 publication material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2PublicationUnavailable {
    /// The runtime has shut down.
    Closed,
    /// No live socket is bound.
    NoBoundSocket,
    /// State was poisoned during shutdown or failure.
    StateUnavailable,
    /// Reachability evidence is unknown, unreachable, or expired.
    ReachabilityUnqualified,
    /// The bound endpoint or generated protocol address was invalid.
    InvalidPublication,
    /// No validated introducer is live, so the introducer-only address
    /// form cannot be built (Plan 490).
    NoIntroducers,
}

/// Test-only deterministic pre-send datagram fault policy.
///
/// Never constructed by production composition: the daemon has no
/// startup path that sets it, and the default service runs with no
/// policy. Tests arm it after establishment to prove loss/reorder/
/// duplicate recovery across real UDP application boundaries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ssu2TestFaults {
    /// Zero-based transmit indices to silently drop.
    pub drop_transmit: HashSet<u64>,
    /// Zero-based transmit indices to transmit twice.
    pub duplicate_transmit: HashSet<u64>,
    /// Swap the next two transmissions once (reorder), then clear.
    pub swap_next_two: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum SubnetKey {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

fn subnet_key(prefixes: IpPrefixPolicy, ip: IpAddr) -> SubnetKey {
    match ip {
        IpAddr::V4(value) => {
            let bits = u32::from(value);
            let mask = if prefixes.ipv4_prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefixes.ipv4_prefix)
            };
            SubnetKey::V4(Ipv4Addr::from(bits & mask))
        }
        IpAddr::V6(value) => {
            let bits = u128::from(value);
            let mask = if prefixes.ipv6_prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefixes.ipv6_prefix)
            };
            SubnetKey::V6(Ipv6Addr::from(bits & mask))
        }
    }
}

/// Which dial phase a pending outbound handshake is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialPhase {
    /// Token-bearing SessionRequest from the peer token cache.
    CachedToken {
        /// Monotonic time after which the dial restarts tokenless.
        grace_at_ms: u64,
    },
    /// Tokenless TokenRequest/Retry trajectory.
    Tokenless,
}

/// Completion reported to one waiting dial future.
enum DialCompletion {
    /// The handshake promoted; the caller owns this link handle.
    Established(Ssu2LinkHandle, bool),
    /// The handshake failed before promotion.
    Failed,
}

/// One pending outbound handshake: the initiator machine plus the
/// peer-bound manager lease and candidate it will promote.
struct PendingOutbound {
    machine: Option<Initiator>,
    target: Ssu2DialTarget,
    lease: Option<PendingHandshake>,
    candidate: LinkCandidate,
    dial_key: DialKey,
    phase: DialPhase,
    token_used: Option<u64>,
    backstop_ms: u64,
    next_resend_ms: u64,
    completion: Option<oneshot::Sender<DialCompletion>>,
}

/// One pending inbound handshake: the responder machine plus a
/// peer-agnostic pending lease (the peer hash is unknown until
/// SessionConfirmed validates).
struct PendingInbound {
    machine: Option<Responder>,
    source: SocketAddr,
    lease: Option<i2pr_transport::TransportLease>,
    backstop_ms: u64,
    next_resend_ms: Option<u64>,
}
/// SessionConfirmed fragments retained after promotion until the peer
/// proves data-phase liveness (or the resend budget ends).
struct ConfirmedResend {
    datagrams: Vec<Vec<u8>>,
    next_at_ms: u64,
    attempts: u8,
}

/// One active authenticated session with its exact manager link.
struct ActiveSession {
    link_id: LinkId,
    peer: PeerId,
    direction: Direction,
    peer_addr: SocketAddr,
    subnet: SubnetKey,
    session: i2pr_transport_ssu2::Ssu2Session,
    /// Peer RouterInfo bytes observed in the SessionConfirmed
    /// handshake that established this session. RouterInfos are public
    /// data; later installs never mutate established sessions.
    peer_router_info: Vec<u8>,
    /// Authenticated path-validation state (Plan 159): the validated
    /// endpoint starts as the promotion address and only migrates on
    /// matching PathResponse proof.
    path: PathValidator,
    /// When the path in use may next be asked to prove itself again.
    ///
    /// Plan 549: a validated path keeps its class only while it is being
    /// re-proved, and the re-proof rides the runtime's existing maintenance
    /// pass. The instant is a local timestamp and names no peer.
    next_revalidation_ms: u64,
    confirmed_resend: Option<ConfirmedResend>,
    data_rx_observed: bool,
}

/// One staged outbound datagram awaiting socket write.
struct StagedDatagram {
    bytes: Vec<u8>,
    addr: SocketAddr,
    class: BandwidthTrafficClass,
    trace: TracedI2np,
}

#[derive(Default)]
struct TracedI2np {
    message_ids: Vec<u32>,
    fragments: Vec<i2pr_transport_ssu2::session::I2npFragmentReceipt>,
    source: Option<(PeerId, LinkId)>,
}

struct PendingSsu2WriteReceipt {
    expected_fragments: u8,
    written_fragments: HashSet<u8>,
    expires_ms: u64,
    completion: tokio::sync::oneshot::Sender<bool>,
}

struct TraceContext<'a> {
    source_link: LinkId,
    observer: Option<&'a crate::ssu2_qualification_observer::Ssu2QualificationObserver>,
}

struct TracedStage<'a> {
    message_ids: Vec<u32>,
    fragments: Vec<i2pr_transport_ssu2::session::I2npFragmentReceipt>,
    context: TraceContext<'a>,
}

fn bandwidth_peer_key(addr: SocketAddr) -> BandwidthPeerKey {
    use std::hash::{Hash, Hasher};

    // Retained only as a private scheduler key; never format or export it.
    let mut digest = [0_u8; 32];
    for lane in 0..4 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        lane.hash(&mut hasher);
        addr.hash(&mut hasher);
        digest[lane * 8..(lane + 1) * 8].copy_from_slice(&hasher.finish().to_be_bytes());
    }
    BandwidthPeerKey::from_local_digest(digest)
}

/// One cached future-handshake token learned from a NewToken block.
struct CachedToken {
    value: u64,
    expires_secs: u64,
}

/// Mutable service state behind one mutex. No await ever happens while
/// this lock is held: handlers compute outbound staging and completions
/// under the lock, then release it before any socket or channel await.
struct ServiceState {
    pending_outbound: HashMap<u64, PendingOutbound>,
    pending_outbound_addrs: HashSet<SocketAddr>,
    pending_inbound: HashMap<u64, PendingInbound>,
    active: HashMap<LinkId, ActiveSession>,
    peer_links: HashMap<PeerId, HashSet<LinkId>>,
    pending_ip: HashMap<IpAddr, usize>,
    pending_subnet: HashMap<SubnetKey, usize>,
    active_ip: HashMap<IpAddr, usize>,
    active_subnet: HashMap<SubnetKey, usize>,
    token_store: TokenStore,
    replay: HandshakeReplayCache,
    token_cache: BTreeMap<PeerId, Vec<CachedToken>>,
    token_request_rates: HashMap<SocketAddr, (u64, u32)>,
    pending_write_receipts: HashMap<(LinkId, u32), PendingSsu2WriteReceipt>,
    staged_v4: VecDeque<StagedDatagram>,
    staged_v6: VecDeque<StagedDatagram>,
    staged_bytes: usize,
    fault_transmits: u64,
    fault_held: Option<StagedDatagram>,
    /// Router-level reachability policy owner (Plan 159): accumulates
    /// privacy-safe signals (validated paths, observed addresses) into
    /// one conservative state for publication snapshots. Never sees
    /// packet or session objects.
    reachability: ReachabilityTracker,
    /// The reachability state and class count this router last reported.
    ///
    /// Plan 549: a transition is reported only when it happens, so the log
    /// carries the *reason* a floodfill withdrew rather than a periodic poll.
    /// The pair is a state label and a count: no peer, no address, no key.
    last_logged_reachability: Option<(ReachabilityState, usize)>,
    /// Single-flight controlled peer-test exchange (Plan 283): `None`
    /// unless the controlled driver started one through
    /// `start_controlled_peer_test`.
    controlled_test: Option<ControlledPeerTestContext>,
    /// Plan 535: the families this router's own bound sockets can be
    /// named for, and the time the standing fact was last asserted.
    ///
    /// The explicit bind is a **standing** fact -- the socket is bound and
    /// stays bound -- but the reachability model holds it as a timed
    /// observation, so the runtime re-asserts it. These two fields are what
    /// let the re-assertion name the families and measure the record's age
    /// without a second lock and a second clock: the families are captured
    /// at bind time, because the runtime binds once and does not rebind.
    explicit_bind_families: BTreeSet<AddressFamily>,
    /// Plan 535: when the standing explicit-bind fact was last asserted.
    ///
    /// `None` means it was never asserted, so the re-assertion is due.
    explicit_bind_recorded_at: Option<Duration>,
    /// Plan 524: the peers this router asked for a relay tag, and has not
    /// yet seen the answer for.
    ///
    /// Bounded by [`MAX_OUTSTANDING_TAG_REQUESTS`]. A tag that arrives for
    /// a peer outside this set is **refused**: a peer may send the block
    /// unsolicited, and recording an introducer nobody asked for would put
    /// an unauthenticated peer into the router's introducer set.
    outstanding_tag_requests: BTreeSet<PeerId>,
}

impl ServiceState {
    fn pending_total(&self) -> usize {
        self.pending_outbound.len() + self.pending_inbound.len()
    }
}

fn count_up(map: &mut HashMap<IpAddr, usize>, ip: IpAddr) {
    *map.entry(ip).or_default() = map.get(&ip).copied().unwrap_or(0).saturating_add(1);
}

fn count_down(map: &mut HashMap<IpAddr, usize>, ip: &IpAddr) {
    if let Some(value) = map.get_mut(ip) {
        *value = value.saturating_sub(1);
        if *value == 0 {
            map.remove(ip);
        }
    }
}

fn subnet_up(map: &mut HashMap<SubnetKey, usize>, key: SubnetKey) {
    *map.entry(key).or_default() = map.get(&key).copied().unwrap_or(0).saturating_add(1);
}

fn subnet_down(map: &mut HashMap<SubnetKey, usize>, key: &SubnetKey) {
    if let Some(value) = map.get_mut(key) {
        *value = value.saturating_sub(1);
        if *value == 0 {
            map.remove(key);
        }
    }
}

/// Shared runtime state behind one `Clone` service owner.
struct Shared {
    config: Ssu2RuntimeConfig,
    bandwidth: Option<BandwidthGovernor>,
    local_peer: PeerId,
    local_static: [u8; 32],
    local_intro: IntroKey,
    local_router_info: Mutex<Vec<u8>>,
    local_router_info_generation: AtomicU64,
    local_mtu: u16,
    manager: TransportManager,
    backoff: DialAdmission,
    state: Mutex<ServiceState>,
    peer_relay: Ssu2PeerRelayService,
    notify: Notify,
    shutdown: CancellationToken,
    started_at: tokio::time::Instant,
    faults: Mutex<Option<Ssu2TestFaults>>,
    sockets: Mutex<ServiceSockets>,
    counters: ServiceCounters,
    qualification_observer: Option<crate::ssu2_qualification_observer::Ssu2QualificationObserver>,
}

impl fmt::Debug for Shared {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Shared(..)")
    }
}

#[derive(Default)]
struct ServiceSockets {
    v4: Option<SocketAddr>,
    v6: Option<SocketAddr>,
    /// Whether `start` was *asked* to bind this family, independent of
    /// whether the bind succeeded. Plan 339 needs the configured-vs-bound
    /// distinction so a family that was never requested is not reported as a
    /// fault, while a family that was requested and never bound is.
    requested_v4: bool,
    requested_v6: bool,
}

#[derive(Default)]
struct ServiceCounters {
    cheap_drops: AtomicU64,
    auth_failures: AtomicU64,
    protocol_drops: AtomicU64,
    token_rejections: AtomicU64,
    replay_rejections: AtomicU64,
    handshake_timeouts: AtomicU64,
    sessions_established: AtomicU64,
    sessions_closed: AtomicU64,
    duplicate_resolutions: AtomicU64,
    i2np_received: AtomicU64,
    i2np_sent: AtomicU64,
    datagrams_sent: AtomicU64,
    datagrams_received: AtomicU64,
    fault_drops: AtomicU64,
    inbound_queue_drops: AtomicU64,
    staging_drops: AtomicU64,
    send_without_link: AtomicU64,
    path_challenges: AtomicU64,
    path_responses: AtomicU64,
    path_migrations: AtomicU64,
    path_revalidations: AtomicU64,
    path_rejections: AtomicU64,
    path_expirations: AtomicU64,
    path_denied: AtomicU64,
    controlled_started: AtomicU64,
    controlled_confirmed: AtomicU64,
    controlled_received: AtomicU64,
    controlled_drops: AtomicU64,
    /// The router's **own** confirmed peer tests, recorded from a matched
    /// echo (Plan 521). Distinct from `controlled_confirmed`, which mirrors
    /// the controlled table's outcome.
    confirmed_peer_tests: AtomicU64,
    /// The measured local round-trip time of the most recent confirmed peer
    /// test, in milliseconds; zero when none has been confirmed.
    last_confirmed_peer_test_ms: AtomicU64,
    /// Plan 535: the router's own bound-address fact re-asserted, so the
    /// class does not lapse while the router runs. Counts only.
    explicit_bind_refreshes: AtomicU64,
    relay_tag_requests: AtomicU64,
    relay_tags_issued: AtomicU64,
    relay_tag_refused_disabled: AtomicU64,
    relay_tag_refused_random: AtomicU64,
    relay_tag_refused_quota: AtomicU64,
    relay_tag_refused_terminated: AtomicU64,
    relay_tag_refused_control_queue_full: AtomicU64,
    relay_tags_observed: AtomicU64,
    /// Plan 524: relay-tag requests this router queued toward a peer.
    relay_tag_requests_queued: AtomicU64,
    /// Plan 524: relay-tag requests refused because the outstanding set was
    /// at its ceiling.
    relay_tag_requests_refused_capacity: AtomicU64,
    /// Plan 524: relay tags recorded as authenticated introducers.
    relay_tags_recorded: AtomicU64,
    /// Plan 524: relay tags refused because the router never asked the peer.
    relay_tags_refused_unrequested: AtomicU64,
    /// Plan 524: relay tags that answered an outstanding request but could
    /// not be recorded.
    relay_tags_refused_record: AtomicU64,
    relay_blocks_queued: AtomicU64,
    relay_blocks_refused_no_session: AtomicU64,
    relay_blocks_refused_ambiguous_session: AtomicU64,
    relay_blocks_refused_session_terminated: AtomicU64,
    relay_blocks_refused_control_queue_full: AtomicU64,
}

fn monotonic_ms(started: &tokio::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn fill_random(dest: &mut [u8]) -> Result<(), ()> {
    OsRng.try_fill_bytes(dest).map_err(|_| ())
}

fn random_conn_ids() -> Result<(u64, u64), ()> {
    for _ in 0..8 {
        let mut local = [0_u8; 8];
        let mut remote = [0_u8; 8];
        fill_random(&mut local)?;
        fill_random(&mut remote)?;
        let (local, remote) = (u64::from_be_bytes(local), u64::from_be_bytes(remote));
        if local != 0 && remote != 0 && local != remote {
            return Ok((local, remote));
        }
    }
    Err(())
}

fn random_u32_nonzero() -> Result<u32, ()> {
    for _ in 0..8 {
        let mut bytes = [0_u8; 4];
        fill_random(&mut bytes)?;
        let value = u32::from_be_bytes(bytes);
        if value != 0 {
            return Ok(value);
        }
    }
    Err(())
}

fn ephemeral_secret() -> Result<X25519PrivateKey, ()> {
    X25519PrivateKey::generate(&mut OsRng).map_err(|_| ())
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.local_static.zeroize();
    }
}

/// The production SSU2 UDP runtime owner.
///
/// Construction validates configuration and identity without opening
/// a socket; [`Ssu2RuntimeService::start`] binds loopback UDP sockets
/// under a caller-owned [`ChildScope`]. The service is `Clone` (shared
/// `Arc` state) so dial/send/close call sites hold it alongside the
/// [`Ssu2ServiceHandle`] that owns the inbound handoff receiver.
#[derive(Clone)]
pub struct Ssu2RuntimeService {
    shared: Arc<Shared>,
}

impl fmt::Debug for Ssu2RuntimeService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ssu2RuntimeService(..)")
    }
}

/// Socket bind selection for [`Ssu2RuntimeService::start`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2SocketConfig {
    /// IPv4 bind literal (loopback tests use `127.0.0.1:0`).
    pub ipv4: Option<SocketAddr>,
    /// IPv6 bind literal, or `None` to leave IPv6 disabled.
    pub ipv6: Option<SocketAddr>,
}

/// Privacy-safe per-family network condition for the Plan 339 Proposal 170
/// selectors.
///
/// Carries no address, port, endpoint, key, peer identity, or payload — only
/// booleans and a reachability enum per family, so it is safe to project onto
/// the authenticated control surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ssu2NetworkCondition {
    /// IPv4 condition.
    pub v4: FamilyNetworkCondition,
    /// IPv6 condition.
    pub v6: FamilyNetworkCondition,
}

impl Ssu2NetworkCondition {
    /// Returns the condition for one family.
    pub const fn family(&self, family: AddressFamily) -> FamilyNetworkCondition {
        match family {
            AddressFamily::Ipv4 => self.v4,
            AddressFamily::Ipv6 => self.v6,
            AddressFamily::Unknown => FamilyNetworkCondition::inert(),
        }
    }
}

/// A started SSU2 service: bound socket addresses plus the bounded
/// authenticated-I2NP handoff receiver.
pub struct Ssu2ServiceHandle {
    service: Ssu2RuntimeService,
    inbound: mpsc::Receiver<Ssu2InboundI2np>,
    relay: mpsc::Receiver<Ssu2InboundRelay>,
    /// Plan 531: the bounded requester handoff. It is a separate
    /// channel because a hole punch names no session and no peer hash,
    /// and because a Relay Response that correlates with one of the
    /// router's own requests belongs to the requester role. It carries
    /// the same ceiling as the relay handoff.
    requester: mpsc::Receiver<Ssu2InboundRequesterEvent>,
    /// Plan 532: the bounded target handoff. A Relay Introduction is
    /// addressed to this router, so it belongs to the target role and not
    /// to the introducer's. It carries the same ceiling as the relay
    /// handoff.
    target: mpsc::Receiver<Ssu2InboundTarget>,
    local_v4: Option<SocketAddr>,
    local_v6: Option<SocketAddr>,
}

impl fmt::Debug for Ssu2ServiceHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ssu2ServiceHandle")
            .field("local_v4", &self.local_v4.is_some())
            .field("local_v6", &self.local_v6.is_some())
            .finish()
    }
}

impl Ssu2ServiceHandle {
    /// Returns the owning service for dial/send/close/snapshot calls.
    pub const fn service(&self) -> &Ssu2RuntimeService {
        &self.service
    }
    /// Receives the next authenticated inbound I2NP message, or `None`
    /// after service shutdown drains the handoff queue.
    pub async fn next_inbound(&mut self) -> Option<Ssu2InboundI2np> {
        self.inbound.recv().await
    }

    /// Receives the next authenticated inbound relay block, or `None`
    /// after service shutdown drains the handoff queue (Plan 490).
    pub async fn next_relay_event(&mut self) -> Option<Ssu2InboundRelay> {
        self.relay.recv().await
    }

    /// Receives the next inbound relay block without waiting.
    ///
    /// Plan 498: the daemon's live inbound loop drains the relay handoff on
    /// a bounded tick. `next_relay_event` awaits, and `tokio::select!` over
    /// the two handle receivers is impossible because both take `&mut self`
    /// on the same handle — so the loop needs a non-blocking pull.
    ///
    /// Returns `None` when the channel is empty **or** disconnected. The
    /// channel is the existing bounded relay channel
    /// (`max_inbound_relay_queue`, default 16); this adds no channel and no
    /// capacity, and changes nothing about the runtime's authentication path.
    pub fn try_next_relay_event(&mut self) -> Option<Ssu2InboundRelay> {
        self.relay.try_recv().ok()
    }

    /// Receives the next inbound requester event without waiting (Plan 531).
    ///
    /// The requester's events have their own bounded handoff with the
    /// same ceiling as the relay handoff (`max_inbound_relay_queue`,
    /// default 16). Returns `None` when the handoff is empty **or**
    /// disconnected.
    pub fn try_next_requester_event(&mut self) -> Option<Ssu2InboundRequesterEvent> {
        self.requester.try_recv().ok()
    }

    /// Receives the next inbound target event without waiting (Plan 532).
    ///
    /// The target's events have their own bounded handoff with the same
    /// ceiling as the relay handoff (`max_inbound_relay_queue`, default
    /// 16). Returns `None` when the handoff is empty **or** disconnected.
    pub fn try_next_target_event(&mut self) -> Option<Ssu2InboundTarget> {
        self.target.try_recv().ok()
    }

    /// Returns the bound IPv4 socket address, if any.
    pub const fn local_v4(&self) -> Option<SocketAddr> {
        self.local_v4
    }

    /// Returns the bound IPv6 socket address, if any.
    pub const fn local_v6(&self) -> Option<SocketAddr> {
        self.local_v6
    }
}

/// The messages one peer-test exchange delivered to this router.
///
/// Plan 528: the specification's peer-test result table is a table over
/// exactly these three messages, so the verdict is the specification's own
/// and never a guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2PeerTestMessages {
    /// The answer relayed by the introducer arrived.
    pub msg4: bool,
    /// The helper's direct answer arrived.
    pub msg5: bool,
    /// The helper's direct verdict arrived.
    pub msg7: bool,
}

/// What one peer-test exchange proved about this router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2PeerTestVerdict {
    /// The helper could reach this router directly.
    Reachable,
    /// The helper could not reach this router directly: the router is
    /// firewalled.
    Firewalled,
    /// The exchange proved nothing: the received messages do not decide.
    Undetermined,
}

/// An established SSU2 dial: the exact link plus whether the tokenless
/// Retry round trip was skipped via a cached token.
#[derive(Debug)]
pub struct Ssu2EstablishedLink {
    /// Exact-link handle for close and snapshots.
    pub link: Ssu2LinkHandle,
    /// Whether a cached token established this session directly.
    pub used_cached_token: bool,
}

/// An exact-link handle. Sends resolve the current manager link for
/// the peer (robust to duplicate replacement); close removes exactly
/// this link without disturbing a replacement.
#[derive(Clone, Debug)]
pub struct Ssu2LinkHandle {
    shared: Arc<Shared>,
    link_id: LinkId,
    peer: PeerId,
}

impl Ssu2LinkHandle {
    /// Returns the exact manager link identifier.
    pub fn link_id(self) -> LinkId {
        self.link_id
    }

    /// Returns the authenticated peer reference.
    pub fn peer(self) -> PeerId {
        self.peer
    }
}

impl Ssu2RuntimeService {
    /// Creates a bounded runtime service without opening a socket.
    pub fn new(
        config: Ssu2RuntimeConfig,
        identity: Ssu2IdentityMaterial,
    ) -> Result<Self, Ssu2RuntimeConfigError> {
        Self::new_with_bandwidth(config, identity, None)
    }

    /// Creates a runtime service with an optional process-shared byte governor.
    pub fn new_with_bandwidth(
        config: Ssu2RuntimeConfig,
        identity: Ssu2IdentityMaterial,
        bandwidth: Option<BandwidthGovernor>,
    ) -> Result<Self, Ssu2RuntimeConfigError> {
        config.validate()?;
        if identity.router_info.is_empty()
            || identity.router_info.len() > constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES
        {
            return Err(Ssu2RuntimeConfigError::InvalidIdentity);
        }
        let initial_router_info = RouterInfo::decode(
            &identity.router_info,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .map_err(|_| Ssu2RuntimeConfigError::InvalidIdentity)?;
        i2pr_crypto::verify_router_info(&initial_router_info)
            .map_err(|_| Ssu2RuntimeConfigError::InvalidIdentity)?;
        if initial_router_info
            .router_identity()
            .hash()
            .map_err(|_| Ssu2RuntimeConfigError::InvalidIdentity)?
            != identity.router_hash
        {
            return Err(Ssu2RuntimeConfigError::InvalidIdentity);
        }
        let static_secret = X25519PrivateKey::from_bytes(identity.static_secret_bytes);
        let static_public = Ssu2PublicKey::new(static_secret.public_bytes())
            .map_err(|_| Ssu2RuntimeConfigError::InvalidIdentity)?;
        let address_bound = initial_router_info.addresses().iter().any(|address| {
            if address.transport_style() != "SSU2" {
                return false;
            }
            let Ok(parsed) = Ssu2RouterAddress::parse(address) else {
                return false;
            };
            parsed.static_public_key().as_bytes() == static_public.as_bytes()
                && parsed
                    .intro_key()
                    .is_some_and(|intro| intro.as_bytes() == identity.intro_key.as_bytes())
        });
        if !address_bound {
            return Err(Ssu2RuntimeConfigError::InvalidIdentity);
        }
        let per_peer = SSU2_LINKS_PER_PEER.min(config.limits.max_active_sessions as u64);
        let per_link_msgs =
            SSU2_MANAGER_MESSAGES_PER_LINK.min(config.limits.max_outbound_datagrams as u64);
        let per_link_bytes =
            SSU2_MANAGER_BYTES_PER_LINK.min(config.limits.max_outbound_bytes as u64);
        let manager = TransportManager::new(
            TransportLimits::new(
                config.limits.max_pending_handshakes as u64,
                config.limits.max_active_sessions as u64,
                config.limits.max_outbound_bytes as u64,
                config.limits.max_outbound_datagrams as u64,
                per_peer.max(1),
                per_link_msgs.max(1),
                per_link_bytes.max(1),
            )
            .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?,
        )
        .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?;
        let replay = HandshakeReplayCache::new(
            constants::MAX_HANDSHAKE_REPLAY_ENTRIES,
            constants::HANDSHAKE_REPLAY_RETENTION_SECONDS,
        )
        .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?;
        // Plan 535: the lifetime is the runtime's own policy parameter, and
        // the tracker publishes it back to the runtime, so the expiry rule
        // keeps exactly one owner. The corroboration floor, the
        // contradiction floor and the explicit-configuration gate are the
        // transport's own and are unchanged.
        let reachability_policy = i2pr_transport::ReachabilityPolicy::new(
            2,
            i2pr_transport::MIN_CONTRADICTION_FLOOR,
            config.reachability.observation_ttl,
            config.explicit_bind_corroboration,
        );
        let reachability = ReachabilityTracker::new(reachability_policy)
            .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?;
        let peer_relay = Ssu2PeerRelayService::new(Ssu2PeerRelayConfig {
            introducer_enabled: config.introducer_service,
        })
        .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?;
        let qualification_observer = if config.qualification_observer {
            Some(
                crate::ssu2_qualification_observer::Ssu2QualificationObserver::new()
                    .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?,
            )
        } else {
            None
        };
        if let Some(observer) = &qualification_observer {
            observer.record_process(
                PeerId::from_hash(identity.router_hash),
                crate::ssu2_qualification_observer::Ssu2QualificationEventKind::LocalIdentityAttested,
            );
        }
        Ok(Self {
            shared: Arc::new(Shared {
                config,
                bandwidth,
                local_peer: PeerId::from_hash(identity.router_hash),
                local_static: identity.static_secret_bytes,
                local_intro: identity.intro_key,
                local_router_info: Mutex::new(identity.router_info),
                local_router_info_generation: AtomicU64::new(0),
                local_mtu: SSU2_DEFAULT_MTU,
                manager,
                backoff: DialAdmission::new(DialBackoffConfig::default(), SSU2_BACKOFF_ENTRIES)
                    .map_err(|_| Ssu2RuntimeConfigError::InconsistentLimits)?,
                state: Mutex::new(ServiceState {
                    pending_outbound: HashMap::new(),
                    pending_outbound_addrs: HashSet::new(),
                    pending_inbound: HashMap::new(),
                    active: HashMap::new(),
                    peer_links: HashMap::new(),
                    pending_ip: HashMap::new(),
                    pending_subnet: HashMap::new(),
                    active_ip: HashMap::new(),
                    active_subnet: HashMap::new(),
                    token_store: TokenStore::establishment(),
                    replay,
                    token_cache: BTreeMap::new(),
                    token_request_rates: HashMap::new(),
                    pending_write_receipts: HashMap::new(),
                    staged_v4: VecDeque::new(),
                    staged_v6: VecDeque::new(),
                    staged_bytes: 0,
                    fault_transmits: 0,
                    fault_held: None,
                    reachability,
                    last_logged_reachability: None,
                    explicit_bind_families: BTreeSet::new(),
                    explicit_bind_recorded_at: None,
                    controlled_test: None,
                    outstanding_tag_requests: BTreeSet::new(),
                }),
                peer_relay,
                notify: Notify::new(),
                shutdown: CancellationToken::new(),
                started_at: tokio::time::Instant::now(),
                faults: Mutex::new(None),
                sockets: Mutex::new(ServiceSockets::default()),
                counters: ServiceCounters::default(),
                qualification_observer,
            }),
        })
    }

    fn now_ms(&self) -> u64 {
        monotonic_ms(&self.shared.started_at)
    }

    fn service_now(&self) -> Duration {
        self.shared.started_at.elapsed()
    }

    /// Returns the router's reachability evidence.
    ///
    /// Plan 527: the router's reachability has **one** owner -- this
    /// runtime -- and the product reads it. The evidence carries the
    /// state, the number of **distinct** corroborating classes behind it,
    /// the family's expiry and the family itself, so a reader can see how
    /// much the claim rests on and not only what the claim is.
    ///
    /// A contradicting observation supports nothing, so it never raises the
    /// corroboration count: one observation is one fact.
    pub fn reachability_evidence(&self) -> ReachabilitySnapshot {
        let now = self.service_now();
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return ReachabilitySnapshot {
                    state: ReachabilityState::Unknown,
                    corroboration: 0,
                    expires_at: now,
                    family: AddressFamily::Unknown,
                };
            }
        };
        state.reachability.poll_expiry(now);
        state.reachability.snapshot(now)
    }

    /// Returns the transport manager for teardown assertions in tests.
    pub fn manager(&self) -> &TransportManager {
        &self.shared.manager
    }

    /// Returns the bounded, in-memory observer only when explicitly enabled
    /// by the caller's qualification profile.
    pub fn qualification_observer(
        &self,
    ) -> Option<crate::ssu2_qualification_observer::Ssu2QualificationObserver> {
        self.shared.qualification_observer.clone()
    }

    /// Returns the peer RouterInfo bytes observed in the
    /// SessionConfirmed handshake that established one link, if the
    /// link is still live.
    ///
    /// Read-only public-data observability: RouterInfos are public
    /// protocol data the handshake already retained. Established
    /// sessions keep the bytes they were established with; later
    /// `install_local_router_info` calls never mutate them, so a
    /// session established before an install still reports the older
    /// bytes while sessions established after report the newer ones.
    /// The bytes are empty on the initiator side by SSU2 design: the
    /// responder's RouterInfo is never sent on the wire (the dialer
    /// brings it out-of-band), while the responder always observes
    /// the initiator's RouterInfo in SessionRequest/Confirmed.
    pub fn session_peer_router_info(&self, link_id: LinkId) -> Option<Vec<u8>> {
        self.shared
            .state
            .lock()
            .ok()?
            .active
            .get(&link_id)
            .map(|record| record.peer_router_info.clone())
    }

    /// Returns the live link identifiers for one peer, if any.
    ///
    /// Read-only diagnostics so tests resolve per-link observations
    /// (such as [`Self::session_peer_router_info`]) without guessing
    /// at link identifiers.
    pub fn session_peer_links(&self, peer: PeerId) -> Vec<LinkId> {
        self.shared
            .state
            .lock()
            .map(|state| {
                state
                    .peer_links
                    .get(&peer)
                    .map(|links| links.iter().copied().collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    /// Returns the router hashes of peers holding at least one live
    /// session link, bounded at [`MAX_SSU2_ACTIVE_CEILING`] entries.
    ///
    /// Read-only public-data observability for the Plan 295 control
    /// plane (`transport.ssu2.active_sessions`): peer hashes are
    /// public protocol identity already retained for link routing,
    /// never secrets. The set is point-in-time; links established or
    /// released concurrently may appear or vanish between calls.
    /// Lock poisoning degrades to empty, never to fabrication.
    pub fn active_peer_hashes(&self) -> Vec<Hash> {
        self.shared
            .state
            .lock()
            .map(|state| {
                state
                    .peer_links
                    .keys()
                    .take(MAX_SSU2_ACTIVE_CEILING)
                    .map(|peer| peer.hash())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Binds the configured sockets and starts one supervised loop
    /// task per bound family under the caller-owned scope.
    pub async fn start(
        &self,
        scope: &ChildScope,
        sockets: Ssu2SocketConfig,
    ) -> Result<Ssu2ServiceHandle, Ssu2BindError> {
        if sockets.ipv4.is_none() && sockets.ipv6.is_none() {
            return Err(Ssu2BindError::NoSocket);
        }
        // Record the requested families before binding, so a partial or failed
        // bind still reports which families the composition asked for.
        if let Ok(mut guard) = self.shared.sockets.lock() {
            guard.requested_v4 = sockets.ipv4.is_some();
            guard.requested_v6 = sockets.ipv6.is_some();
        }
        let mut bound_v4 = None;
        let mut bound_v6 = None;
        if let Some(addr) = sockets.ipv4 {
            let socket = UdpSocket::bind(addr)
                .await
                .map_err(|_| Ssu2BindError::Bind)?;
            bound_v4 = Some((Arc::new(socket), addr));
        }
        if let Some(addr) = sockets.ipv6 {
            let socket = UdpSocket::bind(addr)
                .await
                .map_err(|_| Ssu2BindError::Bind)?;
            bound_v6 = Some((Arc::new(socket), addr));
        }
        let (inbound_tx, inbound_rx) =
            mpsc::channel(self.shared.config.limits.max_inbound_i2np_queue);
        let (relay_tx, relay_rx) = mpsc::channel(self.shared.config.limits.max_inbound_relay_queue);
        // Plan 531: the requester's inbound events travel out of session
        // or belong to one of the router's own live requests, so they
        // have their own bounded handoff. The ceiling is the relay
        // handoff's ceiling: one inbound requester queue per router,
        // never one per peer.
        let (requester_tx, requester_rx) =
            mpsc::channel(self.shared.config.limits.max_inbound_relay_queue);
        // Plan 532: a Relay Introduction is addressed to this router, so it
        // belongs to the target role and not to the introducer's. It has
        // its own bounded handoff, with the relay handoff's ceiling.
        let (target_tx, target_rx) =
            mpsc::channel(self.shared.config.limits.max_inbound_relay_queue);
        {
            let mut guard = self
                .shared
                .sockets
                .lock()
                .map_err(|_| Ssu2BindError::State)?;
            if bound_v4.is_some() {
                guard.v4 = bound_v4
                    .as_ref()
                    .and_then(|(socket, _)| socket.local_addr().ok());
            }
            if bound_v6.is_some() {
                guard.v6 = bound_v6
                    .as_ref()
                    .and_then(|(socket, _)| socket.local_addr().ok());
            }
        }
        let local_v4 = self
            .shared
            .sockets
            .lock()
            .map_err(|_| Ssu2BindError::State)?
            .v4;
        let local_v6 = self
            .shared
            .sockets
            .lock()
            .map_err(|_| Ssu2BindError::State)?
            .v6;
        if let Some((socket, _)) = bound_v4 {
            let service = self.clone();
            let inbound_tx = inbound_tx.clone();
            let relay_tx = relay_tx.clone();
            let requester_tx = requester_tx.clone();
            let target_tx = target_tx.clone();
            scope
                .spawn(move |child| async move {
                    service
                        .run_loop(
                            child,
                            socket,
                            inbound_tx,
                            relay_tx,
                            requester_tx,
                            target_tx,
                            true,
                        )
                        .await
                })
                .map_err(|_| Ssu2BindError::Scope)?;
        }
        if let Some((socket, _)) = bound_v6 {
            let service = self.clone();
            let requester_tx = requester_tx.clone();
            let target_tx = target_tx.clone();
            scope
                .spawn(move |child| async move {
                    service
                        .run_loop(
                            child,
                            socket,
                            inbound_tx.clone(),
                            relay_tx.clone(),
                            requester_tx.clone(),
                            target_tx.clone(),
                            false,
                        )
                        .await
                })
                .map_err(|_| Ssu2BindError::Scope)?;
        }
        // The original senders are held by the loop tasks; dropping the
        // local clone here would close the channel only after loops exit.
        if let Some(observer) = &self.shared.qualification_observer {
            observer.record_process(
                self.shared.local_peer,
                crate::ssu2_qualification_observer::Ssu2QualificationEventKind::SocketsBound,
            );
        }
        // Plan 530: the router's own bound sockets are the explicit-bind
        // corroboration class, and the fact is true from the moment the bind
        // succeeds. The recorder lives in the runtime because the runtime owns
        // both the tracker and the sockets.
        self.note_bound_endpoints_as_explicit_bind();
        Ok(Ssu2ServiceHandle {
            service: self.clone(),
            inbound: inbound_rx,
            relay: relay_rx,
            requester: requester_rx,
            target: target_rx,
            local_v4,
            local_v6,
        })
    }

    /// Requests service shutdown. Loop tasks drain, release all
    /// handshakes/sessions/manager links, then exit for the scope join.
    pub fn shutdown(&self) {
        if let Some(observer) = &self.shared.qualification_observer {
            observer.record_process(
                self.shared.local_peer,
                crate::ssu2_qualification_observer::Ssu2QualificationEventKind::RouterShutdown,
            );
        }
        let _ = self
            .shared
            .shutdown
            .cancel(CancellationReason::OperatorRequest);
        self.shared.notify.notify_waiters();
    }

    /// Installs the test-only deterministic fault policy (`None`
    /// disables). Arming resets the transmit index so policies apply
    /// relative to the next transmission. Production composition never
    /// calls this.
    pub fn set_test_faults(&self, faults: Option<Ssu2TestFaults>) {
        if let Ok(mut guard) = self.shared.faults.lock() {
            *guard = faults;
        }
        if let Ok(mut state) = self.shared.state.lock() {
            state.fault_transmits = 0;
            state.fault_held = None;
        }
        self.shared.notify.notify_waiters();
    }

    /// Rotates the address-validation token table, invalidating
    /// outstanding tokens (restart semantics; also exercised by the
    /// cached-token acceptance test to prove stale-token recovery).
    pub fn rotate_address_tokens(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.token_store.rotate();
        }
    }

    /// Returns the privacy-safe per-family network condition used by the
    /// Plan 339 Proposal 170 `net.status.v6` / `net.error` / `net.testing`
    /// selectors.
    ///
    /// Each family's effective reachability is taken from this service's own
    /// reachability tracker and is honoured only when the qualified snapshot
    /// belongs to that family and has not expired, so an IPv4-qualified router
    /// never reports an IPv6 claim. `None` means the state could not be read:
    /// a poisoned lock degrades to the caller's gap path and never to a
    /// fabricated condition.
    pub fn network_condition(&self) -> Option<Ssu2NetworkCondition> {
        let now = self.service_now();
        let (requested_v4, requested_v6, bound_v4, bound_v6) = {
            let sockets = self.shared.sockets.lock().ok()?;
            (
                sockets.requested_v4,
                sockets.requested_v6,
                sockets.v4.is_some(),
                sockets.v6.is_some(),
            )
        };
        let snapshot = {
            let mut state = self.shared.state.lock().ok()?;
            // Same expiry maintenance the publication path performs, so the
            // reported condition cannot outlive its evidence.
            state.reachability.poll_expiry(now);
            state.reachability.snapshot(now)
        };
        Some(Ssu2NetworkCondition {
            v4: FamilyNetworkCondition {
                configured: requested_v4,
                bound: bound_v4,
                reachability: effective_reachability(Some(&snapshot), AddressFamily::Ipv4, now),
            },
            v6: FamilyNetworkCondition {
                configured: requested_v6,
                bound: bound_v6,
                reachability: effective_reachability(Some(&snapshot), AddressFamily::Ipv6, now),
            },
        })
    }

    /// Returns privacy-safe aggregate counters plus table gauges.
    pub fn snapshot(&self) -> Ssu2Snapshot {
        let counters = &self.shared.counters;
        let (pending_outbound, pending_inbound, active_sessions, token_table_entries, cached) =
            self.shared
                .state
                .lock()
                .map(|state| {
                    (
                        state.pending_outbound.len(),
                        state.pending_inbound.len(),
                        state.active.len(),
                        state.token_store.len(),
                        state.token_cache.values().map(Vec::len).sum(),
                    )
                })
                .unwrap_or((0, 0, 0, 0, 0));
        let reachability = self
            .shared
            .state
            .lock()
            .map(|state| state.reachability.state())
            .unwrap_or(ReachabilityState::Unknown);
        Ssu2Snapshot {
            cheap_drops: counters.cheap_drops.load(Ordering::Relaxed),
            auth_failures: counters.auth_failures.load(Ordering::Relaxed),
            protocol_drops: counters.protocol_drops.load(Ordering::Relaxed),
            token_rejections: counters.token_rejections.load(Ordering::Relaxed),
            explicit_bind_refreshes: counters.explicit_bind_refreshes.load(Ordering::Relaxed),
            replay_rejections: counters.replay_rejections.load(Ordering::Relaxed),
            handshake_timeouts: counters.handshake_timeouts.load(Ordering::Relaxed),
            sessions_established: counters.sessions_established.load(Ordering::Relaxed),
            sessions_closed: counters.sessions_closed.load(Ordering::Relaxed),
            duplicate_resolutions: counters.duplicate_resolutions.load(Ordering::Relaxed),
            i2np_received: counters.i2np_received.load(Ordering::Relaxed),
            i2np_sent: counters.i2np_sent.load(Ordering::Relaxed),
            datagrams_sent: counters.datagrams_sent.load(Ordering::Relaxed),
            datagrams_received: counters.datagrams_received.load(Ordering::Relaxed),
            fault_drops: counters.fault_drops.load(Ordering::Relaxed),
            inbound_queue_drops: counters.inbound_queue_drops.load(Ordering::Relaxed),
            staging_drops: counters.staging_drops.load(Ordering::Relaxed),
            send_without_link: counters.send_without_link.load(Ordering::Relaxed),
            path_challenges: counters.path_challenges.load(Ordering::Relaxed),
            path_responses: counters.path_responses.load(Ordering::Relaxed),
            path_migrations: counters.path_migrations.load(Ordering::Relaxed),
            path_revalidations: counters.path_revalidations.load(Ordering::Relaxed),
            path_rejections: counters.path_rejections.load(Ordering::Relaxed),
            path_expirations: counters.path_expirations.load(Ordering::Relaxed),
            path_denied: counters.path_denied.load(Ordering::Relaxed),
            relay_tag_requests: counters.relay_tag_requests.load(Ordering::Relaxed),
            relay_tags_issued: counters.relay_tags_issued.load(Ordering::Relaxed),
            relay_tags_observed: counters.relay_tags_observed.load(Ordering::Relaxed),
            relay_tag_requests_queued: counters.relay_tag_requests_queued.load(Ordering::Relaxed),
            relay_tag_requests_refused_capacity: counters
                .relay_tag_requests_refused_capacity
                .load(Ordering::Relaxed),
            relay_tags_recorded: counters.relay_tags_recorded.load(Ordering::Relaxed),
            relay_tags_refused_unrequested: counters
                .relay_tags_refused_unrequested
                .load(Ordering::Relaxed),
            relay_tags_refused_record: counters.relay_tags_refused_record.load(Ordering::Relaxed),
            relay_blocks_queued: counters.relay_blocks_queued.load(Ordering::Relaxed),
            relay_blocks_refused: counters
                .relay_blocks_refused_no_session
                .load(Ordering::Relaxed)
                .saturating_add(
                    counters
                        .relay_blocks_refused_ambiguous_session
                        .load(Ordering::Relaxed),
                )
                .saturating_add(
                    counters
                        .relay_blocks_refused_session_terminated
                        .load(Ordering::Relaxed),
                )
                .saturating_add(
                    counters
                        .relay_blocks_refused_control_queue_full
                        .load(Ordering::Relaxed),
                ),
            reachability,
            pending_outbound,
            pending_inbound,
            active_sessions,
            token_table_entries,
            cached_tokens: cached,
            local_router_info_generation: self
                .shared
                .local_router_info_generation
                .load(Ordering::Relaxed),
        }
    }

    /// Returns the local RouterInfo installation generation (diagnostics only).
    pub fn local_router_info_generation(&self) -> u64 {
        self.shared
            .local_router_info_generation
            .load(Ordering::Relaxed)
    }

    /// Returns the counts-only relay counters (Plan 499).
    ///
    /// The per-reason breakdown lives here rather than in
    /// [`Ssu2Snapshot`], whose flat aggregate shape has no reason
    /// dimension. No counter carries a peer hash, a key, a tag value, an
    /// endpoint or a payload.
    pub fn relay_counters(&self) -> Ssu2RelayCounters {
        let counters = &self.shared.counters;
        Ssu2RelayCounters {
            tag_requests_observed: counters.relay_tag_requests.load(Ordering::Relaxed),
            tags_issued: counters.relay_tags_issued.load(Ordering::Relaxed),
            tag_refused_disabled: counters.relay_tag_refused_disabled.load(Ordering::Relaxed),
            tag_refused_random: counters.relay_tag_refused_random.load(Ordering::Relaxed),
            tag_refused_quota: counters.relay_tag_refused_quota.load(Ordering::Relaxed),
            tag_refused_terminated: counters
                .relay_tag_refused_terminated
                .load(Ordering::Relaxed),
            tag_refused_control_queue_full: counters
                .relay_tag_refused_control_queue_full
                .load(Ordering::Relaxed),
            tags_observed_inbound: counters.relay_tags_observed.load(Ordering::Relaxed),
            blocks_queued: counters.relay_blocks_queued.load(Ordering::Relaxed),
            blocks_refused_no_session: counters
                .relay_blocks_refused_no_session
                .load(Ordering::Relaxed),
            blocks_refused_ambiguous_session: counters
                .relay_blocks_refused_ambiguous_session
                .load(Ordering::Relaxed),
            blocks_refused_session_terminated: counters
                .relay_blocks_refused_session_terminated
                .load(Ordering::Relaxed),
            blocks_refused_control_queue_full: counters
                .relay_blocks_refused_control_queue_full
                .load(Ordering::Relaxed),
        }
    }

    // -------------------------------------------------------------------
    // Plan 499 — the outbound relay carriage API.
    //
    // The runtime owns the session registry and the transport's bounded
    // control queue; the caller owns the block's content. The peer is
    // the destination: a relay block carries no destination of its own,
    // the resolved session is the destination.
    // -------------------------------------------------------------------

    /// Queues one Relay Request block toward `peer`.
    ///
    /// The block rides the session's normal transmit path, so it leaves in
    /// the next Data packet and the peer decodes it as an authenticated
    /// relay block. Exactly one live session must exist for the peer: a
    /// peer with more than one live link is refused with
    /// [`Ssu2RelayQueueError::AmbiguousSession`] rather than guessed at.
    pub fn queue_relay_request(
        &self,
        peer: PeerId,
        block: i2pr_transport_ssu2::RelayRequestBlock,
    ) -> Result<(), Ssu2RelayQueueError> {
        self.queue_relay_block(peer, |session| session.queue_relay_request(block))
    }

    /// Queues one in-session RelayTagRequest toward `peer`.
    ///
    /// The request body is empty: the peer is the destination, so the
    /// session is the whole destination. The request carries no policy —
    /// whether the *receiving* peer answers is that peer's own introducer
    /// gate (`ssu2.introducer_service`), not the requester's.
    ///
    /// Plan 524. The peer joins the **outstanding** set, so that a tag
    /// arriving for a peer this router did not ask is refused rather than
    /// recorded. The set is bounded and a request beyond the bound is
    /// refused: it never queues and never evicts an older request.
    pub fn queue_relay_tag_request(&self, peer: PeerId) -> Result<(), Ssu2TagRequestError> {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => {
                let error = Ssu2RelayQueueError::SessionTerminated;
                self.note_relay_refusal(error);
                return Err(Ssu2TagRequestError::NotQueued(error));
            }
        };
        if !state.outstanding_tag_requests.contains(&peer)
            && state.outstanding_tag_requests.len() >= MAX_OUTSTANDING_TAG_REQUESTS
        {
            self.shared
                .counters
                .relay_tag_requests_refused_capacity
                .fetch_add(1, Ordering::Relaxed);
            return Err(Ssu2TagRequestError::Capacity);
        }
        match self.queue_relay_block_on_state(&mut state, peer, |session| {
            session.queue_relay_tag_request()
        }) {
            Ok(()) => {
                state.outstanding_tag_requests.insert(peer);
                self.shared
                    .counters
                    .relay_tag_requests_queued
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => {
                self.note_relay_refusal(error);
                Err(Ssu2TagRequestError::NotQueued(error))
            }
        }
    }

    /// Queues one Relay Intro block toward `peer`.
    ///
    /// See [`Self::queue_relay_request`] for the resolution contract.
    pub fn queue_relay_intro(
        &self,
        peer: PeerId,
        block: i2pr_transport_ssu2::RelayIntroBlock,
    ) -> Result<(), Ssu2RelayQueueError> {
        self.queue_relay_block(peer, |session| session.queue_relay_intro(block))
    }

    /// Queues one Relay Response block toward `peer`.
    ///
    /// See [`Self::queue_relay_request`] for the resolution contract.
    pub fn queue_relay_response(
        &self,
        peer: PeerId,
        block: i2pr_transport_ssu2::RelayResponseBlock,
    ) -> Result<(), Ssu2RelayQueueError> {
        self.queue_relay_block(peer, |session| session.queue_relay_response(block))
    }

    /// Resolves `peer` to exactly one live link and queues one control on it.
    ///
    /// This is the single place the runtime resolves a peer to a session,
    /// so every carriage refusal has one reason vocabulary. The state
    /// mutex is held for the lookup and the queueing only; nothing here
    /// awaits, and the scheduler is woken after the queueing so the block
    /// leaves on the next tick.
    ///
    /// A poisoned state mutex means no live session can be established, so
    /// it fails closed as a terminated session — the same shape as
    /// [`Self::queue_to_session`]'s closed outcome — and is counted like
    /// every other refusal.
    fn queue_relay_block<F>(&self, peer: PeerId, queue: F) -> Result<(), Ssu2RelayQueueError>
    where
        F: FnOnce(
            &mut i2pr_transport_ssu2::Ssu2Session,
        ) -> Result<(), i2pr_transport_ssu2::SessionError>,
    {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => {
                let error = Ssu2RelayQueueError::SessionTerminated;
                self.note_relay_refusal(error);
                return Err(error);
            }
        };
        match self.queue_relay_block_on_state(&mut state, peer, queue) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.note_relay_refusal(error);
                Err(error)
            }
        }
    }

    /// Resolves `peer` to exactly one live link and queues one control on
    /// it.
    ///
    /// Plan 524. The caller holds the state lock and owns the refusal
    /// count, so the tag-request path can make the queueing and the
    /// outstanding-set update one step under one lock.
    fn queue_relay_block_on_state<F>(
        &self,
        state: &mut ServiceState,
        peer: PeerId,
        queue: F,
    ) -> Result<(), Ssu2RelayQueueError>
    where
        F: FnOnce(
            &mut i2pr_transport_ssu2::Ssu2Session,
        ) -> Result<(), i2pr_transport_ssu2::SessionError>,
    {
        let link = resolve_single_peer_link(state.peer_links.get(&peer))?;
        let Some(record) = state.active.get_mut(&link) else {
            return Err(Ssu2RelayQueueError::NoSession);
        };
        match queue(&mut record.session) {
            Ok(()) => {
                self.shared
                    .counters
                    .relay_blocks_queued
                    .fetch_add(1, Ordering::Relaxed);
                self.shared.notify.notify_one();
                Ok(())
            }
            Err(error) => Err(relay_queue_reason(error)),
        }
    }

    /// Records one relay tag as an authenticated introducer.
    ///
    /// Plan 524. The record's `peer_hash` and `relay_tag` are the facts
    /// the SSU2 introducer tuple publishes. The record's `endpoint` and
    /// `intro_key` are **this router's own**: the SSU 1 shape published
    /// the *target's* contact material so that the introducer could reach
    /// the target, and the SSU2 published form carries neither (Plan 523).
    ///
    /// The tag arrived over an authenticated session, so the session *is*
    /// the authentication and the provenance is
    /// [`i2pr_transport_ssu2::IntroducerProvenance::AuthenticatedDirect`].
    fn record_authenticated_introducer(
        &self,
        peer: &PeerId,
        tag: u32,
        local_endpoint: Option<Ssu2Endpoint>,
        now_secs: u64,
    ) -> Result<(), Ssu2TagRequestError> {
        let Some(endpoint) = local_endpoint else {
            // Without the router's own bound endpoint there is no contact
            // material to record, and a record with none would be a lie.
            return Err(Ssu2TagRequestError::NoLocalEndpoint);
        };
        // The record's introduction key is the **published** form of the
        // router's own key, so it is the address module's type; the
        // runtime holds the session's copy of the same 32 bytes.
        let intro_key = match i2pr_transport_ssu2::address::IntroKey::new(
            *self.shared.local_intro.as_bytes(),
        ) {
            Ok(key) => key,
            Err(_) => return Err(Ssu2TagRequestError::NoLocalIntroKey),
        };
        let expires_secs = now_secs.saturating_add(RELAY_TAG_INTRODUCER_TTL_SECS);
        self.shared
            .peer_relay
            .note_authenticated_introducer(
                now_secs,
                *peer.hash().as_bytes(),
                endpoint,
                intro_key,
                tag,
                expires_secs,
                i2pr_transport_ssu2::IntroducerProvenance::AuthenticatedDirect,
            )
            .map_err(Ssu2TagRequestError::NotRecorded)
    }

    /// Counts one refused relay-block carriage by reason.
    fn note_relay_refusal(&self, error: Ssu2RelayQueueError) {
        let counters = &self.shared.counters;
        let counter = match error {
            Ssu2RelayQueueError::NoSession => &counters.relay_blocks_refused_no_session,
            Ssu2RelayQueueError::AmbiguousSession => {
                &counters.relay_blocks_refused_ambiguous_session
            }
            Ssu2RelayQueueError::SessionTerminated => {
                &counters.relay_blocks_refused_session_terminated
            }
            Ssu2RelayQueueError::ControlQueueFull => {
                &counters.relay_blocks_refused_control_queue_full
            }
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    // -------------------------------------------------------------------
    // Plan 490 — the introducer (Bob) role, driven by the daemon's role
    // owner. The runtime owns the bounded tables and the socket; the
    // daemon owns the policy and the receipts.
    // -------------------------------------------------------------------

    /// Returns whether the introducer (Bob) role is enabled.
    pub fn introducer_service_enabled(&self) -> bool {
        self.shared.peer_relay.introducer_enabled()
    }

    /// Returns the introducers this router has learned from authenticated
    /// sessions.
    ///
    /// Plan 524. Read-only public protocol material: the SSU2 introducer
    /// tuple is published in the RouterInfo address, so it is public data
    /// and never a secret. The set is point-in-time, and a record that
    /// expired is not returned.
    pub fn learned_introducers(&self, now_secs: u64) -> Vec<i2pr_transport_ssu2::Ssu2Introducer> {
        self.shared.peer_relay.select_introducers(now_secs)
    }

    /// Issues one relay tag that identifies `target_hash`.
    ///
    /// The specification binds a relay tag to the introduction target:
    /// the target asks the introducer for a tag and publishes it, and a
    /// requester presents that tag to name the target. The tag value
    /// comes from the caller (production: the OS CSPRNG).
    pub fn issue_relay_tag(
        &self,
        tag: u32,
        target_hash: &[u8; 32],
        now_secs: u64,
    ) -> Result<(), i2pr_transport_ssu2::RelayError> {
        self.shared
            .peer_relay
            .issue_introducer_tag(tag, target_hash, now_secs)
    }

    /// Resolves the peer a live relay tag identifies.
    ///
    /// `None` means the tag is unknown or expired; no state is created.
    pub fn relay_tag_owner(&self, tag: u32, now_secs: u64) -> Option<[u8; 32]> {
        self.shared.peer_relay.introducer_tag_owner(tag, now_secs)
    }

    /// Handles one authenticated inbound Relay Request (Bob).
    ///
    /// `target_hash` is the peer the request's relay tag identifies; the
    /// requester's signature is verified over the specification's
    /// preimage, which binds that target. `requester_signer` is the
    /// requester's own signing key, resolved by the daemon's role owner
    /// from a validated RouterInfo.
    // The relay vocabulary is inherently a tuple (block, the three peer
    // hashes, the signer's key, the size budget, and the two clocks); the
    // peer-relay service it forwards to carries the same shape.
    #[allow(clippy::too_many_arguments)]
    pub fn on_relay_request(
        &self,
        block: &i2pr_transport_ssu2::RelayRequestBlock,
        requester_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        target_hash: &[u8; 32],
        requester_signer: &i2pr_proto::SigningPublicKey,
        request_bytes: usize,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, i2pr_transport_ssu2::RelayError> {
        self.shared.peer_relay.on_relay_request_for_target(
            block,
            requester_hash,
            bob_hash,
            target_hash,
            requester_signer,
            request_bytes,
            now_secs,
            now_ms,
        )
    }

    /// Handles one authenticated inbound Relay Response (Bob).
    pub fn on_introducer_relay_response(
        &self,
        block: &i2pr_transport_ssu2::RelayResponseBlock,
        sender_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        target_signer: &i2pr_proto::SigningPublicKey,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<i2pr_transport_ssu2::IntroducerResponseOutcome, i2pr_transport_ssu2::RelayError>
    {
        self.shared.peer_relay.on_introducer_relay_response(
            block,
            sender_hash,
            bob_hash,
            target_signer,
            now_secs,
            now_ms,
        )
    }

    /// Handles one authenticated inbound Relay Intro for the target
    /// (Charlie) role (Plan 531).
    ///
    /// The transport's target table owns the correlation, the replay
    /// suppression and the forwarded-signature verification. The
    /// requester's signing key is resolved by the caller from the
    /// **validated** store, because the validated store is the daemon's
    /// authority and not the runtime's.
    pub fn on_target_relay_intro(
        &self,
        block: &i2pr_transport_ssu2::RelayIntroBlock,
        expected_bob_hash: &[u8; 32],
        expected_charlie_hash: &[u8; 32],
        alice_signer: &i2pr_proto::SigningPublicKey,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, i2pr_transport_ssu2::RelayError> {
        self.shared.peer_relay.on_relay_intro_with_signer(
            block,
            expected_bob_hash,
            expected_charlie_hash,
            alice_signer,
            now_secs,
            now_ms,
        )
    }

    /// Consumes one pending relay introduction (Plan 532).
    ///
    /// The transport's target table owns the pending introduction, and the
    /// transport's own replay suppression is the authority on it. This
    /// releases it after the hole punch has been emitted, so a replayed
    /// introduction cannot produce a second emission.
    pub fn consume_target_intro(&self, nonce: u32) {
        self.shared.peer_relay.consume_target_intro(nonce);
    }

    /// Stages one outbound HolePunch for a relay introduction (Plan 532).
    ///
    /// The datagram is built with the transport's own codec -- so the
    /// transport's state machine is used and not re-implemented -- and it
    /// is sealed under the **requester's** introduction key. It travels
    /// **out of session** and is staged through the runtime's existing
    /// bounded queue, so it is bounded by the runtime's own limits and
    /// never becomes an unbounded send.
    ///
    /// `charlie_endpoint` is this router's own bound endpoint, which the
    /// Address block carries; `requester_endpoint` is where the datagram
    /// goes, and it is checked against the router's own dial profile, so
    /// the target role cannot widen the router's reach.
    pub fn stage_relay_hole_punch(
        &self,
        requester_intro: &IntroKey,
        nonce: u32,
        charlie_endpoint: Ssu2Endpoint,
        requester_endpoint: Ssu2Endpoint,
        response: i2pr_transport_ssu2::RelayResponseBlock,
        now_secs: u64,
    ) -> Result<(), Ssu2RelayTargetError> {
        let destination = requester_endpoint.socket_addr();
        if !is_dial_profile_address(&destination) {
            return Err(Ssu2RelayTargetError::DestinationRefused);
        }
        let (destination_id, source_id) = i2pr_transport_ssu2::hole_punch_conn_ids(nonce);
        let packet_number = random_u32_nonzero().map_err(|_| Ssu2RelayTargetError::NoRandomness)?;
        let timestamp = u32::try_from(now_secs).map_err(|_| Ssu2RelayTargetError::NoRandomness)?;
        let datagram = i2pr_transport_ssu2::build_hole_punch(
            requester_intro,
            source_id,
            destination_id,
            packet_number,
            timestamp,
            charlie_endpoint,
            response,
        )
        .map_err(|_| Ssu2RelayTargetError::TransportRefused)?;
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| Ssu2RelayTargetError::StagingRefused)?;
        if !stage_class_locked(
            &mut state,
            &self.shared.config.limits,
            &self.shared.counters,
            datagram,
            destination,
            BandwidthTrafficClass::Control,
            TracedI2np::default(),
        ) {
            return Err(Ssu2RelayTargetError::StagingRefused);
        }
        self.shared.notify.notify_one();
        Ok(())
    }

    /// Issues an establishment token for the relay establishment
    /// (Plan 533).
    ///
    /// The token is issued by the runtime's **own** responder table and
    /// is bound to the requester's exact source address. That table is
    /// the only authority on a presented token: the responder consumes a
    /// token it issued, from the address it issued it to, exactly once.
    /// A token the daemon drew for itself would be in no table and the
    /// responder would refuse it as unknown -- which is exactly the
    /// defect this seam closes.
    ///
    /// The randomness is the runtime's, and so is the table: the caller
    /// receives the **value** it must put in the accept and never sees
    /// the store.
    ///
    /// `responder_conn_id` is `None`, exactly as for a data-phase
    /// `NewToken`: the responder mints a fresh connection ID at
    /// SessionCreated rather than advertising one the initiator never
    /// saw.
    ///
    /// A refusal here is a **local** condition about this router's own
    /// table. It is never a fact about the requester.
    pub fn issue_relay_establishment_token(
        &self,
        requester: SocketAddr,
        now_secs: u64,
    ) -> Result<u64, Ssu2RelayTargetError> {
        let mut bytes = [0_u8; 8];
        if fill_random(&mut bytes).is_err() {
            return Err(Ssu2RelayTargetError::NoRandomness);
        }
        if u64::from_be_bytes(bytes) == 0 {
            return Err(Ssu2RelayTargetError::NoRandomness);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| Ssu2RelayTargetError::EstablishmentRefused)?;
        match state.token_store.issue(requester, now_secs, bytes, None) {
            Ok(token) => Ok(token.value()),
            Err(_) => Err(Ssu2RelayTargetError::EstablishmentRefused),
        }
    }

    /// Starts one relay request (Alice, the requester side; Plan 531).
    ///
    /// The transport's requester table owns the correlation, the
    /// per-peer quota and the global ceiling. The daemon's requester
    /// owner owns the policy -- which target, which introducer, when --
    /// and calls this seam. The table itself is the transport's own and
    /// this plan does not edit it.
    pub fn start_relay_request(
        &self,
        nonce: u32,
        tag: u32,
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), i2pr_transport_ssu2::RelayError> {
        self.shared.peer_relay.start_relay_request(
            nonce,
            tag,
            bob_hash,
            charlie_hash,
            alice_endpoint,
            now_ms,
        )
    }

    /// Handles one inbound Relay Response for the requester's own request
    /// (Plan 531).
    ///
    /// `bob_hash` is the introducer the request named and `sender_hash`
    /// is the peer the response arrived from; the transport refuses a
    /// response whose sender is not that introducer. `charlie_signer`
    /// is the **target's** signing key, resolved by the daemon from the
    /// target's validated RouterInfo: the accept is Charlie's signature
    /// forwarded verbatim by the introducer.
    pub fn on_requester_relay_response(
        &self,
        block: &i2pr_transport_ssu2::RelayResponseBlock,
        sender_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        charlie_signer: &i2pr_proto::SigningPublicKey,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<Ssu2RelayRequestState, i2pr_transport_ssu2::RelayError> {
        let state = self.shared.peer_relay.on_relay_response_with_signer(
            block,
            sender_hash,
            bob_hash,
            charlie_signer,
            now_secs,
            now_ms,
        )?;
        Ok(match state {
            i2pr_transport_ssu2::RequesterState::AwaitingResponse => {
                Ssu2RelayRequestState::AwaitingResponse
            }
            i2pr_transport_ssu2::RequesterState::AwaitingHolePunch => {
                Ssu2RelayRequestState::AwaitingHolePunch
            }
            i2pr_transport_ssu2::RequesterState::Completed => Ssu2RelayRequestState::Refused,
        })
    }

    /// Handles one inbound HolePunch for the requester's own request
    /// (Plan 531).
    ///
    /// `true` means the punch is ready: the exchange completed, the peer
    /// is reachable **through the relay**, and the router's own inbound
    /// reachability is contradicted. The runtime owns the router's
    /// reachability, so the runtime records the indication.
    pub fn on_requester_hole_punch(
        &self,
        message: &i2pr_transport_ssu2::HolePunchMessage,
        bob_hash: &[u8; 32],
        charlie_signer: &i2pr_proto::SigningPublicKey,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, i2pr_transport_ssu2::RelayError> {
        self.shared.peer_relay.on_hole_punch_with_signer(
            message,
            bob_hash,
            charlie_signer,
            now_secs,
            now_ms,
        )
    }

    /// Cancels one relay request and releases its quota (Plan 531).
    pub fn cancel_relay_request(&self, nonce: u32) -> Result<(), i2pr_transport_ssu2::RelayError> {
        self.shared.peer_relay.cancel_relay_request(nonce)
    }

    /// Returns the router's own bound endpoint (Plan 531).
    ///
    /// The requester's Relay Request carries the router's own endpoint,
    /// and the router must name the endpoint it actually holds: a
    /// wildcard or port-zero bind names none, and a router with no bound
    /// socket yields `None`. IPv4 is preferred; IPv6 is the fallback.
    pub fn local_bound_endpoint(&self) -> Option<Ssu2Endpoint> {
        self.bound_endpoint(AddressFamily::Ipv4)
            .or_else(|| self.bound_endpoint(AddressFamily::Ipv6))
    }

    /// Returns this router's own 32-byte hash (Plan 532).
    ///
    /// The target role's verification is bound to the target, and the
    /// target is this router, so the runtime is the one that knows the
    /// hash. It is the router's identity hash and never an address.
    pub fn local_router_hash(&self) -> [u8; 32] {
        *self.shared.local_peer.hash().as_bytes()
    }

    /// Builds the specification's introducer-only publication material
    /// from the validated introducer set (Plan 490).
    ///
    /// The form omits host/port, publishes the introducer groups and the
    /// family capability the specification demands alongside them, and
    /// lists at most three introducers in the table's deterministic
    /// order.
    ///
    /// Plan 525: the snapshot's **evidence** is the introducer set
    /// itself, so the evidence expiry is the **earliest** expiry among
    /// the introducers the form publishes. Passing the build time as the
    /// evidence expiry instead -- as this builder used to -- made the
    /// transport's own evidence gate withdraw **every** snapshot, so the
    /// function could never succeed. The gate was never wrong; the value
    /// handed to it was.
    ///
    /// The family capability is derived from the router's **bound**
    /// socket rather than assumed, so the router never claims a family it
    /// has no socket for.
    pub fn introducer_publication_material(
        &self,
        now_secs: u64,
    ) -> Result<Ssu2PublicationMaterial, Ssu2PublicationUnavailable> {
        let introducers = self.shared.peer_relay.select_introducers(now_secs);
        if introducers.is_empty() {
            return Err(Ssu2PublicationUnavailable::NoIntroducers);
        }
        let family = self.bound_family()?;
        // Plan 526: the introducer-only form carries the **family**
        // letter and never the role letter. The form publishes no
        // endpoint, so it is not an introducer to a peer that reads the
        // advertisement -- the reference's own peer-side check demands an
        // endpoint for an introducer (`libi2pd/RouterInfo.cpp:1059`) -- and
        // the netdb's introducer rule refuses any role letter on this form
        // (`crates/i2pr-netdb/src/floodfill_role.rs`).
        let capabilities = match family {
            i2pr_transport::AddressFamily::Ipv4 => "4",
            i2pr_transport::AddressFamily::Ipv6 => "6",
            i2pr_transport::AddressFamily::Unknown => {
                return Err(Ssu2PublicationUnavailable::NoBoundSocket);
            }
        };
        let capabilities = i2pr_transport_ssu2::Ssu2Capabilities::parse(capabilities)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        // The form's evidence is the introducer set, and the set's members
        // expire individually. The **earliest** expiry is the fail-closed
        // choice: the form is withdrawn the moment its first listed
        // introducer dies, so a refresh re-selects and the published set
        // never carries a dead introducer for longer than one publication
        // cycle. Every selected introducer is live, so the earliest expiry
        // is strictly greater than `now_secs`.
        let evidence_expires_secs = introducers
            .iter()
            .map(i2pr_transport_ssu2::Ssu2Introducer::expires_secs)
            .min()
            .ok_or(Ssu2PublicationUnavailable::NoIntroducers)?;
        let snapshot = i2pr_transport_ssu2::publication::build_publication_snapshot(
            i2pr_transport_ssu2::publication::PublicationRequest {
                policy: i2pr_transport_ssu2::publication::PublicationPolicy::new(false, true),
                static_public: X25519PrivateKey::from_bytes(self.shared.local_static)
                    .public_bytes(),
                intro_public: *self.shared.local_intro.as_bytes(),
                endpoint: None,
                reachability: i2pr_transport::ReachabilitySnapshot {
                    state: i2pr_transport::ReachabilityState::Firewalled,
                    corroboration: 0,
                    expires_at: std::time::Duration::from_secs(0),
                    family,
                },
                mtu: self.shared.local_mtu,
                caps: &capabilities,
                introducers: &introducers,
                now_secs,
                evidence_expires_secs,
            },
        )
        .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        let i2pr_transport_ssu2::publication::PublicationOutcome::IntroducerOnly(snapshot) =
            snapshot
        else {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        };
        let entries = snapshot.option_entries().to_vec();
        let options = Mapping::from_entries(entries)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        let address = RouterAddress::new(
            10,
            Date::from_millis(now_secs.saturating_mul(1000)),
            "SSU2".to_owned(),
            options,
        )
        .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        i2pr_transport_ssu2::publication::parse_snapshot(&snapshot)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        Ok(Ssu2PublicationMaterial {
            address,
            form: Ssu2PublicationForm::IntroducerOnly,
            advertises_introducer_role: false,
            evidence_expires_at: std::time::Duration::from_secs(
                evidence_expires_secs.saturating_sub(now_secs),
            ),
            reachability: i2pr_transport::ReachabilityState::Firewalled,
            bound_family: family,
        })
    }

    /// Returns the address family this router's **bound** socket
    /// represents.
    ///
    /// Plan 525: the introducer-only form publishes a family capability,
    /// and the family must be one the router actually has a socket for.
    /// IPv4 is preferred when both are bound, because the tree's SSU2
    /// posture is IPv4-first and the form publishes exactly one family
    /// letter.
    fn bound_family(&self) -> Result<i2pr_transport::AddressFamily, Ssu2PublicationUnavailable> {
        let sockets = self
            .shared
            .sockets
            .lock()
            .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
        let bound = sockets
            .v4
            .filter(|bound| bound.port() != 0 && !bound.ip().is_unspecified())
            .or_else(|| {
                sockets
                    .v6
                    .filter(|bound| bound.port() != 0 && !bound.ip().is_unspecified())
            });
        match bound {
            Some(bound) if bound.is_ipv4() => Ok(i2pr_transport::AddressFamily::Ipv4),
            Some(_) => Ok(i2pr_transport::AddressFamily::Ipv6),
            None => Err(Ssu2PublicationUnavailable::NoBoundSocket),
        }
    }

    /// Records one peer-test exchange's verdict into the router's
    /// reachability.
    ///
    /// Plan 528: the verdict is the **specification's own** table,
    /// transcribed as data in the transport
    /// (`i2pr_transport_ssu2::peer_test_result`). A FIREWALLED row is this
    /// router's own firewalled fact and is recorded as the firewalled
    /// verdict, which puts the router in `Firewalled`. An OK row
    /// corroborates **nothing** by itself: the corroboration is the
    /// address agreement, and that arrives through the comparison. An
    /// unknown or ambiguous row records nothing at all.
    ///
    /// The router's reachability has one owner -- this runtime -- so the
    /// verdict is recorded here and nowhere else.
    pub fn note_peer_test_verdict(
        &self,
        family: AddressFamily,
        messages: Ssu2PeerTestMessages,
    ) -> Ssu2PeerTestVerdict {
        use i2pr_transport_ssu2::PeerTestResult;
        let verdict = match i2pr_transport_ssu2::peer_test_result(
            messages.msg4,
            messages.msg5,
            messages.msg7,
        ) {
            PeerTestResult::Firewalled => Ssu2PeerTestVerdict::Firewalled,
            PeerTestResult::Ok => Ssu2PeerTestVerdict::Reachable,
            PeerTestResult::Unknown | PeerTestResult::Ambiguous | PeerTestResult::NotApplicable => {
                Ssu2PeerTestVerdict::Undetermined
            }
        };
        if verdict == Ssu2PeerTestVerdict::Firewalled && family != AddressFamily::Unknown {
            self.note_peer_test_outcome(i2pr_transport_ssu2::PeerTestOutcome::FirewalledLikely {
                family,
            });
        }
        verdict
    }

    /// Records one typed peer-test outcome into the router's reachability.
    ///
    /// Plan 528: the outcome is recorded **whole**, under its own kind.
    /// The runtime owns the router's reachability, so this is the only
    /// place a peer-test outcome becomes a reachability fact.
    pub fn note_peer_test_outcome(
        &self,
        outcome: i2pr_transport_ssu2::PeerTestOutcome,
    ) -> ReachabilityState {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => return ReachabilityState::Unknown,
        };
        self.record_peer_test_outcome_locked(&mut state, outcome);
        state.reachability.state()
    }

    /// Records one observed external address against the router's own
    /// bound address.
    ///
    /// Plan 527: the observation is an authenticated peer's report of the
    /// address at which it sees this router. An agreement corroborates the
    /// router's reachability; a disagreement **contradicts** it, because
    /// the router is behind a NAT and must not publish a reachability
    /// claim it cannot honour.
    ///
    /// The comparison is made against the **bound** socket for the
    /// observation's family, and a family with no bound socket is
    /// refused: the router never claims a reachability it cannot compare,
    /// and it never invents a contradiction it cannot support.
    ///
    /// A contradiction never also corroborates -- one observation is one
    /// fact, and it is filed under one family.
    ///
    /// Returns the state the observation left behind.
    pub fn note_observed_external_address(&self, observed: Ssu2Endpoint) -> ReachabilityState {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => return ReachabilityState::Unknown,
        };
        self.note_observed_external_address_locked(&mut state, observed)
    }

    /// The locked half of [`Self::note_observed_external_address`].
    fn note_observed_external_address_locked(
        &self,
        state: &mut ServiceState,
        observed: Ssu2Endpoint,
    ) -> ReachabilityState {
        let family = match observed.ip() {
            IpAddr::V4(_) => AddressFamily::Ipv4,
            IpAddr::V6(_) => AddressFamily::Ipv6,
        };
        let now = self.service_now();
        match self.bound_endpoint(family) {
            Some(bound) if bound == observed => {
                state.reachability.record(
                    ReachabilitySignal::AuthenticatedPeerObservedExternalAddress { family },
                    now,
                );
            }
            Some(_) => {
                // The peer sees this router at a different address. The
                // observation disagrees with the bound address, so it
                // contradicts -- and a contradiction never also
                // corroborates.
                //
                // Plan 528: the disagreement is this router's own
                // **firewalled fact**: the peer sees the router at an
                // address the router does not own, so the router is
                // behind a NAT. It is therefore recorded as the
                // **firewalled verdict**, not as a bare disagreement, so
                // the router's state says `Firewalled` rather than
                // `Unreachable`. This is the same fact the
                // specification's peer-test result table reports as
                // FIREWALLED, and both routes reach the same state.
                state.reachability.record(
                    ReachabilitySignal::PeerTestResult {
                        family,
                        outcome: PeerTestOutcomeKind::FirewalledLikely,
                    },
                    now,
                );
            }
            None => {
                // No bound socket for this family: the comparison is
                // refused, so nothing is recorded.
            }
        }
        state.reachability.state()
    }

    /// Returns the router's own bound address for a family.
    ///
    /// Plan 527: the comparison between an observed address and the
    /// router's own address is made against the **bound** socket, not
    /// against a configured value the socket may not honour. A family
    /// with no bound socket yields `None`, and the caller refuses the
    /// comparison rather than inventing an answer.
    fn bound_endpoint(&self, family: AddressFamily) -> Option<Ssu2Endpoint> {
        let sockets = self.shared.sockets.lock().ok()?;
        let bound = match family {
            AddressFamily::Ipv4 => sockets.v4,
            AddressFamily::Ipv6 => sockets.v6,
            AddressFamily::Unknown => None,
        }?;
        if bound.port() == 0 || bound.ip().is_unspecified() {
            return None;
        }
        Ssu2Endpoint::from_socket_addr(bound).ok()
    }

    /// Returns the capability string the router publishes for itself.
    ///
    /// Plan 526: this is the **same** value the published address carries,
    /// so a reader cannot see a different capability string than the one
    /// the router publishes. The family letter comes from the router's
    /// **bound** socket; the published address's own family comes from the
    /// reachability evidence, and the two are the same family whenever the
    /// direct form is admitted -- the direct builder looks up the bound
    /// socket of the reachability family and refuses the form when it has
    /// none.
    pub fn advertised_capabilities(
        &self,
    ) -> Result<i2pr_transport_ssu2::Ssu2Capabilities, Ssu2PublicationUnavailable> {
        let family = self.bound_family()?;
        let (capabilities, _advertises_introducer_role) = self.capabilities_for(family)?;
        Ok(capabilities)
    }

    /// Returns the capability string for a family, with the role letter
    /// when the role is live.
    ///
    /// Plan 526: the family letter comes from the router's **bound**
    /// socket, and the **introducer role** letter is added **only** while
    /// the role is live. The role letter is a claim the router must be able
    /// to honour, so a router whose role owner is not running never
    /// publishes it.
    ///
    /// The family letter comes **first** and the role letter second, which
    /// is the order the transport's own family helper uses
    /// (`crates/i2pr-transport-ssu2/src/publication.rs`). The result is
    /// parsed by the transport's own strict parser, so an unusable
    /// combination is refused rather than published.
    fn capabilities_for(
        &self,
        family: i2pr_transport::AddressFamily,
    ) -> Result<(i2pr_transport_ssu2::Ssu2Capabilities, bool), Ssu2PublicationUnavailable> {
        let family_letter = match family {
            i2pr_transport::AddressFamily::Ipv4 => "4",
            i2pr_transport::AddressFamily::Ipv6 => "6",
            i2pr_transport::AddressFamily::Unknown => {
                return Err(Ssu2PublicationUnavailable::InvalidPublication);
            }
        };
        let advertises_introducer_role = self.introducer_service_enabled();
        let value = if advertises_introducer_role {
            format!("{family_letter}C")
        } else {
            family_letter.to_owned()
        };
        let capabilities = i2pr_transport_ssu2::Ssu2Capabilities::parse(&value)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        Ok((capabilities, advertises_introducer_role))
    }

    /// Answers which form the router publishes for **itself** now.
    ///
    /// Plan 525: the router publishes **one** SSU2 address, and its shape
    /// depends on the router's own posture. The **direct** form -- with
    /// host and port -- is published while the router's reachability
    /// evidence admits it. When that evidence does not admit it, the
    /// router is firewalled, and the specification's **introducer-only**
    /// form is published instead: it carries the validated introducer set
    /// and **no** endpoint.
    ///
    /// The composition lives here because both facts are the runtime's:
    /// the router's reachability evidence and the validated introducer
    /// set. A caller never infers a form from a refusal, and the daemon
    /// holds no copy of the rule.
    ///
    /// A refusal that is **not** about reachability -- the runtime is
    /// closed, no socket is bound, the state is poisoned, the endpoint is
    /// invalid -- is propagated unchanged: such a router is **broken**,
    /// not firewalled, and publishing an introducer-only address would
    /// hide the fault behind a plausible record.
    pub fn self_publication_material(
        &self,
        wall_now_ms: u64,
    ) -> Result<Ssu2PublicationMaterial, Ssu2PublicationUnavailable> {
        match self.publication_material(wall_now_ms) {
            Ok(material) => Ok(material),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified) => {
                let now_secs = wall_now_ms / 1_000;
                self.introducer_publication_material(now_secs)
            }
            Err(reason) => Err(reason),
        }
    }

    /// Builds direct SSU2 publication material only from the actual live bound endpoint and
    /// current non-expired reachability evidence. No private transport key leaves this owner.
    ///
    /// The reachability floor is `Reachable` (above-floor corroboration with fresh evidence);
    /// unknown/unreachable/expired evidence is refused. `CandidateReachable` (exactly the
    /// corroboration floor) is refused: the Plan 159 publication builder only renders a
    /// direct address for `Reachable`, and this bridge does not weaken that contract.
    /// Static loopback-homogeneous traffic cannot reach `Reachable` (at most the
    /// explicit-bind plus peer-observation classes); heterogeneous reference traffic or a
    /// third evidence class is required. The loopback clamp below additionally confines
    /// this API to the isolated qualification profile: no non-loopback endpoint can ever
    /// become advertised material through it.
    pub fn publication_material(
        &self,
        wall_now_ms: u64,
    ) -> Result<Ssu2PublicationMaterial, Ssu2PublicationUnavailable> {
        if self.shared.shutdown.is_cancelled() {
            return Err(Ssu2PublicationUnavailable::Closed);
        }
        let (bound_v4, bound_v6) = {
            let sockets = self
                .shared
                .sockets
                .lock()
                .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
            (sockets.v4, sockets.v6)
        };
        if bound_v4.is_none() && bound_v6.is_none() {
            return Err(Ssu2PublicationUnavailable::NoBoundSocket);
        }
        if [bound_v4, bound_v6]
            .into_iter()
            .flatten()
            .any(|bound| bound.port() == 0 || bound.ip().is_unspecified())
        {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let now = self.service_now();
        let (reachability, corroboration, expiry, family) = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
            state.reachability.poll_expiry(now);
            let snapshot = state.reachability.snapshot(now);
            (
                snapshot.state,
                snapshot.corroboration,
                snapshot.expires_at,
                snapshot.family,
            )
        };
        if reachability != ReachabilityState::Reachable || expiry <= now {
            // Plan 549: this refusal is sampled by the floodfill eligibility
            // evaluation, and a class that expires *inside* this call takes the
            // state below the corroboration threshold for an instant -- long
            // enough to withdraw the role, and invisible to the transition
            // reporter, which only runs on the maintenance pass. Reporting the
            // sample itself is what makes the withdrawal explainable.
            tracing::info!(
                transition = "publication_material_refused",
                reason = "reachability_unqualified",
                reachability = ?reachability,
                supporting_classes = corroboration,
                evidence_expires_in_millis = u64::try_from(
                    expiry
                        .saturating_sub(now)
                        .as_millis()
                        .min(u128::from(u64::MAX))
                )
                .unwrap_or(u64::MAX),
                "the publication gate sampled reachability below its floor"
            );
            return Err(Ssu2PublicationUnavailable::ReachabilityUnqualified);
        }
        let bound = match family {
            i2pr_transport::AddressFamily::Ipv4 => bound_v4,
            i2pr_transport::AddressFamily::Ipv6 => bound_v6,
            i2pr_transport::AddressFamily::Unknown => None,
        }
        .ok_or(Ssu2PublicationUnavailable::NoBoundSocket)?;
        if !may_publish_bound_address(bound, self.shared.config.publish_bound_address) {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let endpoint = Ssu2Endpoint::new(bound.ip(), bound.port())
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        // Plan 526: the direct form is the only form that may carry the
        // introducer role letter, because the reference publishes the role
        // letters on the published form only
        // (`libi2pd/RouterInfo.cpp:1350`-`:1373`) and a peer that reads a
        // role letter without an endpoint cannot act on it
        // (`libi2pd/RouterInfo.cpp:1059`).
        let (capabilities, advertises_introducer_role) = self.capabilities_for(family)?;
        let expires_secs = expiry.as_secs();
        let snapshot = i2pr_transport_ssu2::publication::build_publication_snapshot(
            i2pr_transport_ssu2::publication::PublicationRequest {
                policy: i2pr_transport_ssu2::publication::PublicationPolicy::new(true, false),
                static_public: X25519PrivateKey::from_bytes(self.shared.local_static)
                    .public_bytes(),
                intro_public: *self.shared.local_intro.as_bytes(),
                endpoint: Some(endpoint),
                reachability: i2pr_transport::ReachabilitySnapshot {
                    state: reachability,
                    corroboration,
                    expires_at: expiry,
                    family,
                },
                mtu: self.shared.local_mtu,
                caps: &capabilities,
                introducers: &[],
                now_secs: now.as_secs(),
                evidence_expires_secs: expires_secs,
            },
        )
        .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        let i2pr_transport_ssu2::publication::PublicationOutcome::Direct(snapshot) = snapshot
        else {
            return Err(Ssu2PublicationUnavailable::ReachabilityUnqualified);
        };
        let entries = snapshot.option_entries().to_vec();
        let options = Mapping::from_entries(entries)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        let address = RouterAddress::new(
            10,
            Date::from_millis(wall_now_ms),
            "SSU2".to_owned(),
            options,
        )
        .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        i2pr_transport_ssu2::publication::parse_snapshot(&snapshot)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        Ok(Ssu2PublicationMaterial {
            address,
            form: Ssu2PublicationForm::Direct,
            advertises_introducer_role,
            evidence_expires_at: expiry,
            reachability,
            bound_family: family,
        })
    }

    /// Records the router's **own** bound sockets as the explicit-bind
    /// corroboration class.
    ///
    /// Plan 530: the transport's reachability model needs **three** distinct
    /// supporting classes before it will say `Reachable`
    /// (`crates/i2pr-transport/src/reachability.rs:66`), and the explicit
    /// bind -- class 0 -- had exactly one recorder, which only the controlled
    /// floodfill activation called. The daemon already opts in
    /// (`explicit_bind_corroboration`), so the transport **counted** the class
    /// but nothing **earned** it in ordinary operation, and the router could
    /// never publish a direct address.
    ///
    /// The record is made at **bind time**, because that is when the fact
    /// becomes true: a locally configured bind address exists for the family
    /// from the moment the socket is bound. It is made for every family the
    /// runtime can **name** its own bound endpoint for, using the same
    /// predicate [`Self::bound_endpoint`] already applies -- a wildcard or a
    /// port-zero bind names no address and therefore records nothing.
    ///
    /// The record is gated on the same opt-in that lets the transport count
    /// the class, so a runtime that has not asked for explicit-bind
    /// corroboration records nothing and the transport never sees a
    /// configured origin it did not earn. The signal carries only the
    /// **family**, never a literal endpoint, so the record stays redacted by
    /// construction.
    fn note_bound_endpoints_as_explicit_bind(&self) {
        if !self.shared.config.explicit_bind_corroboration {
            return;
        }
        let mut families = Vec::new();
        for family in [AddressFamily::Ipv4, AddressFamily::Ipv6] {
            if self.bound_endpoint(family).is_some() {
                families.push(family);
            }
        }
        if families.is_empty() {
            return;
        }
        if let Ok(mut state) = self.shared.state.lock() {
            let now = self.service_now();
            for family in &families {
                state.reachability.record(
                    ReachabilitySignal::LocalConfiguredBind { family: *family },
                    now,
                );
            }
            // Plan 535: the standing fact's families and its age are the
            // re-assertion's inputs, and they are captured here, at the
            // bind, so the re-assertion needs neither the sockets lock nor
            // a second clock.
            state.explicit_bind_families = families.iter().copied().collect();
            state.explicit_bind_recorded_at = Some(now);
        }
    }

    /// Records the actual bound sockets as explicit-bind reachability evidence.
    ///
    /// This is the controlled-qualification half of the publication corroboration pair: the bind
    /// addresses are explicitly configured loopback sockets owned by this runtime, and the other
    /// half must come from real authenticated-peer observations. Every bound endpoint must be
    /// loopback with a nonzero port or the call fails closed without recording anything. Only
    /// the controlled M12 activation composition calls this; normal operation never does, and
    /// the observation is ignored unless the runtime was configured with
    /// `explicit_bind_corroboration`.
    pub fn note_explicit_bind_for_controlled_qualification(
        &self,
    ) -> Result<(), Ssu2PublicationUnavailable> {
        if self.shared.shutdown.is_cancelled() {
            return Err(Ssu2PublicationUnavailable::Closed);
        }
        let (bound_v4, bound_v6) = {
            let sockets = self
                .shared
                .sockets
                .lock()
                .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
            (sockets.v4, sockets.v6)
        };
        if bound_v4.is_none() && bound_v6.is_none() {
            return Err(Ssu2PublicationUnavailable::NoBoundSocket);
        }
        let mut families = Vec::new();
        for bound in [bound_v4, bound_v6].into_iter().flatten() {
            if !may_publish_bound_address(bound, self.shared.config.publish_bound_address) {
                return Err(Ssu2PublicationUnavailable::InvalidPublication);
            }
            families.push(match bound.ip() {
                std::net::IpAddr::V4(_) => i2pr_transport::AddressFamily::Ipv4,
                std::net::IpAddr::V6(_) => i2pr_transport::AddressFamily::Ipv6,
            });
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
        let now = self.service_now();
        for family in families {
            state.reachability.record(
                i2pr_transport::ReachabilitySignal::LocalConfiguredBind { family },
                now,
            );
        }
        Ok(())
    }

    /// Records the router's **own** confirmed peer test (Plan 521).
    ///
    /// This is the production counterpart of the controlled table's outcome
    /// mirror. The controlled mirror takes its outcome from the controlled
    /// exchange; this one takes the **sealed** proof the transport mints when
    /// an echoed identifier matched an outstanding probe. A caller therefore
    /// cannot assert a confirmed round trip with a boolean or a claim.
    ///
    /// The signal belongs to **this** router: it is the router's own
    /// reachability, and it is never written into a peer's profile. Only a
    /// `Confirmed` outcome records anything, so an inconclusive test records
    /// nothing and a failed test can never poison later evidence.
    ///
    /// The caller names the **link** the exchange travelled on. The runtime
    /// derives the address family from that session's own address, so a
    /// caller cannot assert a family and the evidence cannot be filed under
    /// a family the exchange never used. A session that is already gone is
    /// refused rather than recorded under a guessed family.
    ///
    /// Refused after shutdown.
    pub fn note_confirmed_peer_test(
        &self,
        link_id: LinkId,
        proof: ConfirmedPeerTest,
    ) -> Result<(), Ssu2PeerTestEvidenceError> {
        if self.shared.shutdown.is_cancelled() {
            return Err(Ssu2PeerTestEvidenceError::Closed);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| Ssu2PeerTestEvidenceError::StateUnavailable)?;
        // The family is the family of the session the exchange travelled on,
        // and the runtime is the only component that knows it. The caller
        // names the link and the runtime derives the family from the
        // session's own address, so the evidence is recorded on the family's
        // terms or not at all.
        let family = match state.active.get(&link_id) {
            Some(session) => match session.peer_addr.ip() {
                IpAddr::V4(_) => AddressFamily::Ipv4,
                IpAddr::V6(_) => AddressFamily::Ipv6,
            },
            None => return Err(Ssu2PeerTestEvidenceError::NoSession),
        };
        let now = self.service_now();
        state.reachability.record(
            ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::Confirmed,
            },
            now,
        );
        self.shared
            .counters
            .confirmed_peer_tests
            .fetch_add(1, Ordering::Relaxed);
        self.shared
            .counters
            .last_confirmed_peer_test_ms
            .store(proof.elapsed_ms(), Ordering::Relaxed);
        Ok(())
    }

    /// Records the router's **own** firewalled indication.
    ///
    /// Plan 527: the relay-success indication is a contradicting signal,
    /// and the router's reachability has exactly one owner -- this
    /// runtime. The peer-relay service used to keep a second tracker for
    /// it, and that tracker was read by nothing, so the indication never
    /// reached the product.
    ///
    /// The **requester** side of the SSU2 relay protocol produces the
    /// indication; the router implements the introducer side, so the
    /// producer is a successor's subject. The recorder lives here so the
    /// producer has exactly one place to write to.
    ///
    /// An unknown family records nothing: a signal is filed under one
    /// family, and the runtime never guesses one.
    pub fn note_relay_firewalled(&self, family: AddressFamily) {
        if family == AddressFamily::Unknown {
            return;
        }
        let now = self.service_now();
        if let Ok(mut state) = self.shared.state.lock() {
            state
                .reachability
                .record(ReachabilitySignal::RelayFirewalledSignal { family }, now);
        }
    }

    /// Returns the measured local round-trip time of the most recent
    /// confirmed peer test, in milliseconds; zero when none was confirmed.
    pub fn last_confirmed_peer_test_ms(&self) -> u64 {
        self.shared
            .counters
            .last_confirmed_peer_test_ms
            .load(Ordering::Relaxed)
    }

    /// Returns the number of the router's own confirmed peer tests recorded.
    pub fn confirmed_peer_test_count(&self) -> u64 {
        self.shared
            .counters
            .confirmed_peer_tests
            .load(Ordering::Relaxed)
    }

    /// Starts one single-flight controlled peer-test exchange (Plan 283).
    ///
    /// Binds this service's role, the three router hashes, and the
    /// endpoint under test, then starts the role in the service-owned
    /// peer-test table. The bindings are enforced, not advisory:
    /// - the service plays exactly the role whose hash is its own;
    /// - Alice role additionally requires `alice_endpoint` to be one
    ///   of this service's bound sockets (the test can only attest the
    ///   real endpoint);
    /// - `alice_endpoint` must be loopback with a nonzero port.
    ///
    /// Refused unless the service was configured with
    /// `controlled_peer_test` and no test is live. The caller registers
    /// the helper signing keys separately; this function stores no
    /// secrets.
    #[allow(clippy::too_many_arguments)]
    pub fn start_controlled_peer_test(
        &self,
        nonce: u32,
        role: PeerTestRole,
        alice_hash: [u8; 32],
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
    ) -> Result<(), ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        if self.shared.shutdown.is_cancelled() {
            return Err(ControlledPeerTestError::Unavailable);
        }
        if !alice_endpoint.socket_addr().ip().is_loopback()
            || alice_endpoint.socket_addr().port() == 0
        {
            return Err(ControlledPeerTestError::BindingMismatch);
        }
        let local_hash = *self.shared.local_peer.hash().as_bytes();
        let role_binds = match role {
            PeerTestRole::Alice => alice_hash == local_hash,
            PeerTestRole::Bob => bob_hash == local_hash,
            PeerTestRole::Charlie => charlie_hash == local_hash,
        };
        if !role_binds {
            return Err(ControlledPeerTestError::BindingMismatch);
        }
        if matches!(role, PeerTestRole::Alice) {
            let bound = self
                .shared
                .sockets
                .lock()
                .map_err(|_| ControlledPeerTestError::Unavailable)?;
            let endpoint = alice_endpoint.socket_addr();
            if Some(endpoint) != bound.v4 && Some(endpoint) != bound.v6 {
                return Err(ControlledPeerTestError::BindingMismatch);
            }
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        if state.controlled_test.is_some() {
            return Err(ControlledPeerTestError::AlreadyRunning);
        }
        let now_ms = monotonic_ms(&self.shared.started_at);
        self.shared
            .peer_relay
            .start_peer_test(
                nonce,
                role,
                alice_hash,
                bob_hash,
                charlie_hash,
                alice_endpoint,
                now_ms,
            )
            .map_err(|_| ControlledPeerTestError::ResourceDenied)?;
        state.controlled_test = Some(ControlledPeerTestContext {
            nonce,
            role,
            alice_hash,
            bob_hash,
            charlie_hash,
            alice_endpoint,
            outcome: None,
            last_forward: None,
        });
        self.shared
            .counters
            .controlled_started
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Registers one peer signing key for the live controlled test.
    ///
    /// Public keys only; refused unless the service was configured
    /// with `controlled_peer_test`. Unknown signers fail closed at
    /// ingest without allocating outcome state.
    pub fn register_controlled_peer_signer(
        &self,
        peer_hash: [u8; 32],
        key: &SigningPublicKey,
    ) -> Result<(), ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        self.shared
            .peer_relay
            .register_signer(peer_hash, key.clone())
            .map_err(|_| ControlledPeerTestError::ResourceDenied)?;
        Ok(())
    }

    /// Queues one in-session controlled peer-test block toward a peer
    /// (Msgs 1–4 only; Msgs 5–7 travel out-of-session).
    ///
    /// Requires the live test, exactly one session for the peer, and a
    /// non-terminal session. The queued block rides the normal
    /// session transmit path with a scheduler wakeup, so delivery is a
    /// real authenticated session event at the peer.
    pub fn queue_controlled_peer_test(
        &self,
        peer: PeerId,
        block: PeerTestBlock,
    ) -> Result<(), ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        if self.shared.shutdown.is_cancelled() {
            return Err(ControlledPeerTestError::Unavailable);
        }
        if !(1..=4).contains(&block.message()) {
            return Err(ControlledPeerTestError::WrongTransport);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        if state.controlled_test.is_none() {
            return Err(ControlledPeerTestError::NoControlledTest);
        }
        let link = match state.peer_links.get(&peer) {
            Some(links) if links.len() == 1 => links.iter().next().copied(),
            Some(_) => return Err(ControlledPeerTestError::ResourceDenied),
            None => return Err(ControlledPeerTestError::NoSession),
        };
        let Some(link) = link else {
            return Err(ControlledPeerTestError::NoSession);
        };
        let Some(record) = state.active.get_mut(&link) else {
            return Err(ControlledPeerTestError::NoSession);
        };
        record
            .session
            .queue_peer_test(block)
            .map_err(|_| ControlledPeerTestError::NoSession)?;
        self.shared.notify.notify_one();
        Ok(())
    }

    /// Queues one in-session Address report toward a peer (Plan 283).
    ///
    /// The reported endpoint is not caller-chosen: it is read from the
    /// live session's observed peer address, so the report always names
    /// the genuine observation. Returns the reported endpoint so the
    /// driver can correlate it. Requires the live test and exactly one
    /// session for the peer.
    pub fn queue_controlled_address(
        &self,
        peer: PeerId,
    ) -> Result<Ssu2Endpoint, ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        if self.shared.shutdown.is_cancelled() {
            return Err(ControlledPeerTestError::Unavailable);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        if state.controlled_test.is_none() {
            return Err(ControlledPeerTestError::NoControlledTest);
        }
        let link = match state.peer_links.get(&peer) {
            Some(links) if links.len() == 1 => links.iter().next().copied(),
            Some(_) => return Err(ControlledPeerTestError::ResourceDenied),
            None => return Err(ControlledPeerTestError::NoSession),
        };
        let Some(link) = link else {
            return Err(ControlledPeerTestError::NoSession);
        };
        let Some(record) = state.active.get_mut(&link) else {
            return Err(ControlledPeerTestError::NoSession);
        };
        let endpoint = Ssu2Endpoint::from_socket_addr(record.peer_addr)
            .map_err(|_| ControlledPeerTestError::InvalidTarget)?;
        record
            .session
            .queue_address(AddressBlock::new(endpoint))
            .map_err(|_| ControlledPeerTestError::NoSession)?;
        self.shared.notify.notify_one();
        Ok(endpoint)
    }

    /// Sends one out-of-session controlled peer-test block (Msgs 5–7
    /// only) sealed under the receiver's intro key.
    ///
    /// Alice role sends Msg 6 toward Charlie; Charlie role sends Msgs
    /// 5/7 toward Alice; Bob role has no out-of-session direction and
    /// is refused. The datagram stages through the normal bounded
    /// outbound queue with a scheduler wakeup.
    pub fn send_controlled_peer_test(
        &self,
        target: SocketAddr,
        receiver_intro: &IntroKey,
        block: PeerTestBlock,
    ) -> Result<(), ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        if self.shared.shutdown.is_cancelled() {
            return Err(ControlledPeerTestError::Unavailable);
        }
        if !(5..=7).contains(&block.message()) {
            return Err(ControlledPeerTestError::WrongTransport);
        }
        if target.port() == 0 || !target.ip().is_loopback() {
            return Err(ControlledPeerTestError::InvalidTarget);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        let context = state
            .controlled_test
            .as_ref()
            .ok_or(ControlledPeerTestError::NoControlledTest)?;
        let alice_to_charlie = match context.role {
            PeerTestRole::Alice => true,
            PeerTestRole::Charlie => false,
            PeerTestRole::Bob => return Err(ControlledPeerTestError::WrongTransport),
        };
        let (dest, src) = peer_test_conn_ids(context.nonce, alice_to_charlie);
        let packet_number =
            random_u32_nonzero().map_err(|_| ControlledPeerTestError::ResourceDenied)?;
        let datagram =
            build_out_of_session_peer_test(receiver_intro, src, dest, packet_number, block)
                .map_err(|_| ControlledPeerTestError::InvalidTarget)?;
        if !stage_one_locked(
            &mut state,
            &self.shared.config.limits,
            &self.shared.counters,
            datagram,
            target,
        ) {
            return Err(ControlledPeerTestError::ResourceDenied);
        }
        self.shared.notify.notify_one();
        Ok(())
    }

    /// Takes the stashed transport-only forward (Msg 1 or 2) for the
    /// live test, with the real sender endpoint attached.
    ///
    /// Msgs 1–2 carry no table state: they are wire-faithful
    /// commitments whose receipt causally triggers the driver's next
    /// step. Returns `Ok(None)` while nothing has arrived yet.
    pub fn take_controlled_forward(
        &self,
        nonce: u32,
    ) -> Result<Option<(PeerTestBlock, Ssu2Endpoint)>, ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        let Some(context) = state.controlled_test.as_mut() else {
            return Err(ControlledPeerTestError::NoControlledTest);
        };
        if context.nonce != nonce {
            return Err(ControlledPeerTestError::NonceMismatch);
        }
        Ok(context.last_forward.take())
    }

    /// Returns the terminal table outcome for the live test, if any.
    ///
    /// A `Confirmed` outcome here is mirrored onto the service
    /// reachability tracker at ingest time; every other outcome leaves
    /// the tracker untouched and the driver aborts the exchange.
    pub fn controlled_test_outcome(
        &self,
        nonce: u32,
    ) -> Result<Option<PeerTestOutcome>, ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        let Some(context) = state.controlled_test.as_ref() else {
            return Err(ControlledPeerTestError::NoControlledTest);
        };
        if context.nonce != nonce {
            return Err(ControlledPeerTestError::NonceMismatch);
        }
        Ok(context.outcome)
    }

    /// Returns the live table state for the controlled test, if tracked.
    ///
    /// Read-only diagnostics so the driver advances causally on table
    /// transitions (for example, waiting for Alice to reach
    /// `AliceAwaitingMsg5` before emitting Msg 5). Returns `Ok(None)`
    /// when the table no longer tracks the nonce.
    pub fn controlled_test_state(
        &self,
        nonce: u32,
    ) -> Result<Option<PeerTestState>, ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        let Some(context) = state.controlled_test.as_ref() else {
            return Err(ControlledPeerTestError::NoControlledTest);
        };
        if context.nonce != nonce {
            return Err(ControlledPeerTestError::NonceMismatch);
        }
        Ok(self.shared.peer_relay.peer_test_state(nonce))
    }

    /// Closes the live controlled test and releases its table entry.
    ///
    /// Best-effort and idempotent across driver exit paths: the table
    /// entry is cancelled when still present, and the context is
    /// cleared either way so no stale test survives the exchange.
    pub fn finish_controlled_peer_test(&self, nonce: u32) -> Result<(), ControlledPeerTestError> {
        if !self.shared.config.controlled_peer_test {
            return Err(ControlledPeerTestError::NotPermitted);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ControlledPeerTestError::Unavailable)?;
        let Some(context) = state.controlled_test.as_ref() else {
            return Err(ControlledPeerTestError::NoControlledTest);
        };
        if context.nonce != nonce {
            return Err(ControlledPeerTestError::NonceMismatch);
        }
        let _ = self.shared.peer_relay.cancel_peer_test(nonce);
        state.controlled_test = None;
        Ok(())
    }

    /// Routes one in-session controlled peer-test block into the
    /// service-owned table (or stashes a transport-only forward).
    ///
    /// The sender is always the session peer by construction: Msg 4
    /// arrives on the Alice–Bob session from Bob, Msg 3 on the
    /// Bob–Charlie session from Charlie. Msgs 1–2 carry no table
    /// state and are stashed with the real sender endpoint so the
    /// driver advances causally on wire receipt. Blocks arriving with
    /// no live test, or while the service is not controlled, are
    /// counted and dropped without touching any table.
    fn ingest_controlled_session_block(
        &self,
        state: &mut ServiceState,
        link_id: &LinkId,
        source: SocketAddr,
        block: &PeerTestBlock,
        now_secs: u64,
        now_ms: u64,
    ) {
        if !self.shared.config.controlled_peer_test {
            self.shared
                .counters
                .controlled_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let (sender_hash, bob_hash, alice_hash) = match state.controlled_test.as_ref() {
            Some(context) => (
                state
                    .active
                    .get(link_id)
                    .map(|record| *record.peer.hash().as_bytes()),
                context.bob_hash,
                context.alice_hash,
            ),
            None => {
                self.shared
                    .counters
                    .controlled_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let sender_endpoint = match Ssu2Endpoint::from_socket_addr(source) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                self.shared
                    .counters
                    .controlled_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        if matches!(block.message(), 1 | 2) {
            if let Some(context) = state.controlled_test.as_mut() {
                context.last_forward = Some((block.clone(), sender_endpoint));
                self.shared
                    .counters
                    .controlled_received
                    .fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
        let Some(sender_hash) = sender_hash else {
            self.shared
                .counters
                .controlled_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        // Signature preimage convention (mirrors the transport tests):
        // Alice-signed blocks (Msg 6) omit ahash like Msgs 1–2;
        // Charlie-signed blocks (Msgs 3–5, 7) include it like Msgs
        // 3–4. The driver signs to the same convention.
        let alice_hash_for_block = if block.message() == 6 {
            None
        } else {
            Some(&alice_hash)
        };
        match self.shared.peer_relay.on_peer_test(
            block,
            &sender_hash,
            sender_endpoint,
            &bob_hash,
            alice_hash_for_block,
            now_secs,
            now_ms,
        ) {
            Ok(outcome) => {
                self.shared
                    .counters
                    .controlled_received
                    .fetch_add(1, Ordering::Relaxed);
                self.map_controlled_outcome_locked(state, outcome);
            }
            Err(_) => {
                self.shared
                    .counters
                    .controlled_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Answers one authenticated relay-tag request on the session it
    /// arrived on (Plan 499, WP2).
    ///
    /// The order is fixed and fail-closed at every step:
    ///
    /// 1. the introducer gate — with the operator's default-off
    ///    `ssu2.introducer_service` opt-in off, no tag is issued and no
    ///    answer is queued;
    /// 2. the tag itself comes from the OS CSPRNG and is nonzero, so the
    ///    transport's block constructor cannot reject it;
    /// 3. the tag is bound to the **session peer's** router hash — the
    ///    handshake-authenticated peer, never a caller-supplied identity —
    ///    through the transport's introducer, whose quota is the bound;
    /// 4. the answer is queued on the same session and the scheduler is
    ///    woken so it leaves on the next tick.
    ///
    /// Every refusal is counted by reason; nothing is silently ignored.
    fn answer_relay_tag_request(
        &self,
        state: &mut ServiceState,
        link_id: &LinkId,
        peer: &PeerId,
        now_secs: u64,
    ) {
        self.shared
            .counters
            .relay_tag_requests
            .fetch_add(1, Ordering::Relaxed);
        if !self.shared.peer_relay.introducer_enabled() {
            self.shared
                .counters
                .relay_tag_refused_disabled
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let tag = match random_u32_nonzero() {
            Ok(tag) => tag,
            Err(()) => {
                self.shared
                    .counters
                    .relay_tag_refused_random
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let peer_hash = *peer.hash().as_bytes();
        if self
            .shared
            .peer_relay
            .issue_introducer_tag(tag, &peer_hash, now_secs)
            .is_err()
        {
            self.shared
                .counters
                .relay_tag_refused_quota
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let Ok(block) = i2pr_transport_ssu2::RelayTagBlock::new(tag) else {
            // Unreachable: `random_u32_nonzero` never returns zero and
            // the block constructor rejects only zero. Fail closed.
            self.shared
                .counters
                .relay_tag_refused_random
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let queued = match state.active.get_mut(link_id) {
            Some(record) => record.session.queue_relay_tag(block),
            None => Err(i2pr_transport_ssu2::SessionError::Terminated),
        };
        match queued {
            Ok(()) => {
                self.shared
                    .counters
                    .relay_tags_issued
                    .fetch_add(1, Ordering::Relaxed);
                self.shared.notify.notify_one();
            }
            Err(error) => match error {
                i2pr_transport_ssu2::SessionError::LocalPolicyDenied => {
                    self.shared
                        .counters
                        .relay_tag_refused_control_queue_full
                        .fetch_add(1, Ordering::Relaxed);
                }
                _ => {
                    self.shared
                        .counters
                        .relay_tag_refused_terminated
                        .fetch_add(1, Ordering::Relaxed);
                }
            },
        }
    }

    /// Routes one out-of-session controlled peer-test block into the
    /// service-owned table.
    ///
    /// Only Alice (Msgs 5/7 from Charlie) and Charlie (Msg 6 from
    /// Alice) receive out-of-session in the controlled flow; Bob role
    /// datagrams are dropped. Sender attribution follows the role, but
    /// authentication still rests on the registered signing keys: a
    /// wrong sender fails the table's sender gate, a wrong key fails
    /// the signature gate.
    fn ingest_controlled_out_of_session(
        &self,
        state: &mut ServiceState,
        source: SocketAddr,
        block: &PeerTestBlock,
        now_secs: u64,
        now_ms: u64,
    ) {
        let Some(context) = state.controlled_test.as_ref() else {
            self.shared
                .counters
                .controlled_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let (sender_hash, bob_hash, alice_hash) = match context.role {
            PeerTestRole::Alice => (context.charlie_hash, context.bob_hash, context.alice_hash),
            PeerTestRole::Charlie => (context.alice_hash, context.bob_hash, context.alice_hash),
            PeerTestRole::Bob => {
                self.shared
                    .counters
                    .controlled_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let sender_endpoint = match Ssu2Endpoint::from_socket_addr(source) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                self.shared
                    .counters
                    .controlled_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        // Msg 6 is Charlie's genuine observation carrier: stash it with
        // the real sender endpoint (Alice's observed source) so the
        // driver builds the verdict Msg 7 from observed fact, then
        // route it to the table normally for the helper transition.
        if block.message() == 6
            && let Some(context) = state.controlled_test.as_mut()
        {
            context.last_forward = Some((block.clone(), sender_endpoint));
            self.shared
                .counters
                .controlled_received
                .fetch_add(1, Ordering::Relaxed);
        }
        let alice_hash_for_block = if block.message() == 6 {
            None
        } else {
            Some(&alice_hash)
        };
        match self.shared.peer_relay.on_peer_test(
            block,
            &sender_hash,
            sender_endpoint,
            &bob_hash,
            alice_hash_for_block,
            now_secs,
            now_ms,
        ) {
            Ok(outcome) => {
                self.shared
                    .counters
                    .controlled_received
                    .fetch_add(1, Ordering::Relaxed);
                self.map_controlled_outcome_locked(state, outcome);
            }
            Err(_) => {
                self.shared
                    .counters
                    .controlled_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Mirrors one controlled table outcome onto the service tracker.
    ///
    /// Only `DirectReachabilityConfirmed` records evidence
    /// (`PeerTestResult{Confirmed}`): every other terminal outcome
    /// leaves the tracker untouched so a failed exchange can never
    /// poison later evidence, and the driver aborts fail-closed. The
    /// outcome is stored on the live context either way so the driver
    /// observes exactly what the table decided.
    fn map_controlled_outcome_locked(
        &self,
        state: &mut ServiceState,
        outcome: Option<PeerTestOutcome>,
    ) {
        let Some(outcome) = outcome else {
            return;
        };
        if let PeerTestOutcome::DirectReachabilityConfirmed { .. } = outcome {
            self.shared
                .counters
                .controlled_confirmed
                .fetch_add(1, Ordering::Relaxed);
        }
        // Plan 527: the outcome is recorded **whole**. Recording only the
        // confirming kind is what kept the router's reachability from
        // ever being contradicted.
        self.record_peer_test_outcome_locked(state, outcome);
        if let Some(context) = state.controlled_test.as_mut() {
            context.outcome = Some(outcome);
        }
    }

    /// Records one typed peer-test outcome into the router's reachability.
    ///
    /// Plan 527: the outcome is recorded **whole**. A contradicting
    /// outcome is evidence like any other, and dropping it is what made
    /// the router publish a reachability claim it could not honour: the
    /// only tracker that received contradicting signals was one the
    /// product never read.
    ///
    /// The router's reachability has one owner -- this runtime -- and the
    /// peer-relay service returns the outcome it computes rather than
    /// keeping a second tracker of its own.
    fn record_peer_test_outcome_locked(&self, state: &mut ServiceState, outcome: PeerTestOutcome) {
        let signal = match outcome {
            PeerTestOutcome::DirectReachabilityConfirmed { family, .. } => {
                ReachabilitySignal::PeerTestResult {
                    family,
                    outcome: PeerTestOutcomeKind::Confirmed,
                }
            }
            PeerTestOutcome::AddressMismatch { family, .. } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::AddressMismatch,
            },
            PeerTestOutcome::FirewalledLikely { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::FirewalledLikely,
            },
            PeerTestOutcome::Inconclusive { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::Inconclusive,
            },
            PeerTestOutcome::Rejected { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::Rejected,
            },
        };
        let now = self.service_now();
        state.reachability.record(signal, now);
    }

    /// Returns the currently installed local RouterInfo bytes, if readable.
    ///
    /// Diagnostics and controlled-activation rollback use only: the bytes
    /// are public material (the same bytes future handshakes emit), never
    /// key material. Returns `None` only when the installation lock is
    /// unavailable.
    pub fn installed_local_router_info(&self) -> Option<Vec<u8>> {
        self.shared
            .local_router_info
            .lock()
            .map(|installed| installed.clone())
            .ok()
    }

    /// Atomically installs a locally signed RouterInfo for future SessionConfirmed handshakes.
    /// The encoded size, signature, publication freshness, router hash, network id, and SSU2
    /// endpoint/key binding are checked before replacement; existing authenticated sessions
    /// retain the RouterInfo they already exchanged. Each successful installation bumps the
    /// diagnostics-only generation counter; no session is re-authenticated or mutated.
    ///
    /// Plan 520: the record may carry **no** SSU2 address. The transport's publication
    /// policy renders the endpoint-less form when the router's own reachability evidence
    /// does not admit a direct endpoint, and the router must not keep publishing an
    /// endpoint it can no longer justify. Every SSU2 address the record *does* carry is
    /// still bound to the router's own endpoint and the router's own keys, and two
    /// conflicting SSU2 addresses are still refused.
    pub fn install_local_router_info(
        &self,
        encoded: Vec<u8>,
        wall_now_ms: u64,
    ) -> Result<(), Ssu2PublicationUnavailable> {
        self.install_local_router_info_inner(encoded, wall_now_ms, None)
    }

    /// Installs the controlled dual-transport RouterInfo form after verifying
    /// its NTCP2 address names the separately bound loopback endpoint.
    pub fn install_controlled_dual_router_info(
        &self,
        encoded: Vec<u8>,
        wall_now_ms: u64,
        ntcp2_endpoint: SocketAddr,
    ) -> Result<(), Ssu2PublicationUnavailable> {
        self.install_scoped_dual_router_info(
            encoded,
            wall_now_ms,
            ntcp2_endpoint,
            crate::ntcp2_runtime::Ntcp2DialScope::Loopback,
        )
    }

    /// Installs the dual-transport RouterInfo form after verifying its NTCP2
    /// address names the separately bound endpoint **and** falls inside
    /// `scope`.
    ///
    /// Plan 496: the controlled profile keeps the historical loopback-only
    /// admission. The guarded operator profile admits an RFC1918 IPv4
    /// endpoint instead. The scope is supplied by the caller and is never
    /// widened here: an endpoint outside the scope is refused exactly as the
    /// controlled profile refuses a non-loopback one.
    pub fn install_scoped_dual_router_info(
        &self,
        encoded: Vec<u8>,
        wall_now_ms: u64,
        ntcp2_endpoint: SocketAddr,
        scope: crate::ntcp2_runtime::Ntcp2DialScope,
    ) -> Result<(), Ssu2PublicationUnavailable> {
        if !scope.admits(ntcp2_endpoint.ip()) || ntcp2_endpoint.port() < 1024 {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        self.install_local_router_info_inner(encoded, wall_now_ms, Some(ntcp2_endpoint))
    }

    fn install_local_router_info_inner(
        &self,
        encoded: Vec<u8>,
        wall_now_ms: u64,
        controlled_ntcp2_endpoint: Option<SocketAddr>,
    ) -> Result<(), Ssu2PublicationUnavailable> {
        if self.shared.shutdown.is_cancelled() {
            return Err(Ssu2PublicationUnavailable::Closed);
        }
        if encoded.len() > LOCAL_ROUTER_INFO_MAX_ENCODED_LEN {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let info = RouterInfo::decode(&encoded, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        i2pr_crypto::verify_router_info(&info)
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        if !local_router_info_fresh(info.published().as_millis(), wall_now_ms) {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let hash = info
            .router_identity()
            .hash()
            .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        if hash != self.shared.local_peer.hash() {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let current_bytes = self
            .shared
            .local_router_info
            .lock()
            .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?
            .clone();
        let current = RouterInfo::decode(
            &current_bytes,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
        let ssu2_count = info
            .addresses()
            .iter()
            .filter(|address| address.transport_style() == "SSU2")
            .count();
        let ntcp2_addresses: Vec<_> = info
            .addresses()
            .iter()
            .filter(|address| address.transport_style() == "NTCP2")
            .collect();
        let ntcp2_matches = match controlled_ntcp2_endpoint {
            Some(endpoint) => {
                ntcp2_addresses.len() == 1
                    && ntcp2_addresses.first().is_some_and(|address| {
                        address
                            .options()
                            .get("host")
                            .and_then(|host| host.parse::<IpAddr>().ok())
                            == Some(endpoint.ip())
                            && address
                                .options()
                                .get("port")
                                .and_then(|port| port.parse::<u16>().ok())
                                == Some(endpoint.port())
                            && address.options().get("v") == Some("2")
                            && address
                                .options()
                                .get("s")
                                .is_some_and(|value| !value.is_empty())
                            && address
                                .options()
                                .get("i")
                                .is_some_and(|value| !value.is_empty())
                    })
            }
            None => ntcp2_addresses.is_empty(),
        };
        if info.options().get("netId") != current.options().get("netId")
            || ssu2_count > 1
            || !ntcp2_matches
        {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let sockets = self
            .shared
            .sockets
            .lock()
            .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
        let expected_static = X25519PrivateKey::from_bytes(self.shared.local_static).public_bytes();
        // Plan 520: every SSU2 address the record carries must name the
        // router's own bound endpoint with the router's own static and
        // intro keys. The record may carry **none**: the transport's
        // publication policy renders the endpoint-less form when the
        // router's own evidence does not admit a direct endpoint.
        let address_matches = info
            .addresses()
            .iter()
            .filter(|address| address.transport_style() == "SSU2")
            .all(|address| {
                let Ok(parsed) = Ssu2RouterAddress::parse(address) else {
                    return false;
                };
                // The address must be the router's **own**: its static
                // key and its intro key are the router's.
                let own_keys = parsed.static_public_key().as_bytes() == &expected_static
                    && parsed
                        .intro_key()
                        .is_some_and(|key| key.as_bytes() == self.shared.local_intro.as_bytes());
                // Plan 526: the introducer role letter is a **claim** the
                // router must be able to honour. A record that carries it
                // while the role is not live is refused outright, so a
                // stale letter cannot reach the wire -- and the record
                // cannot stand installed with a claim the router has
                // stopped making.
                if parsed.capabilities().supports_relay() && !self.introducer_service_enabled() {
                    return false;
                }
                // Plan 525: the transport's parser refuses an address
                // carrying both a direct endpoint and introducers, so
                // the two forms are exclusive and the classification
                // is total.
                match parsed.address_class() {
                    Ssu2AddressClass::Direct => {
                        let Some(endpoint) = parsed.endpoint() else {
                            return false;
                        };
                        let is_bound =
                            [sockets.v4, sockets.v6].into_iter().flatten().any(|bound| {
                                bound.ip() == endpoint.ip() && bound.port() == endpoint.port()
                            });
                        is_bound && own_keys
                    }
                    // The **introducer-only** form carries **no**
                    // endpoint: the router is reached through the
                    // introducers it publishes, and those introducers
                    // are the router's own validated set.
                    Ssu2AddressClass::IntroducerOnly => {
                        !parsed.introducers().is_empty() && own_keys
                    }
                    // A static-key-only address names no contact at
                    // all, so it is not the router's published address.
                    Ssu2AddressClass::UnpublishedStatic => false,
                }
            });
        drop(sockets);
        if !address_matches {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        }
        let mut current = self
            .shared
            .local_router_info
            .lock()
            .map_err(|_| Ssu2PublicationUnavailable::StateUnavailable)?;
        *current = encoded;
        self.shared
            .local_router_info_generation
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

/// Returns whether a candidate local RouterInfo publication date is fresh
/// against the Plan 103 age/skew windows mirrored in
/// `LOCAL_ROUTER_INFO_MAX_AGE_SECS` /
/// `LOCAL_ROUTER_INFO_MAX_FUTURE_SKEW_SECS`. Pure and unit-testable; the
/// caller supplies both timestamps so no clock is read inside validation.
fn local_router_info_fresh(published_ms: u64, now_ms: u64) -> bool {
    if published_ms <= now_ms {
        now_ms.saturating_sub(published_ms) <= LOCAL_ROUTER_INFO_MAX_AGE_SECS.saturating_mul(1000)
    } else {
        published_ms.saturating_sub(now_ms)
            <= LOCAL_ROUTER_INFO_MAX_FUTURE_SKEW_SECS.saturating_mul(1000)
    }
}

/// Failure to bind or supervise SSU2 sockets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2BindError {
    /// No socket family was selected.
    NoSocket,
    /// The OS rejected the bind.
    Bind,
    /// Shared state was unavailable.
    State,
    /// The child scope could not retain the loop task.
    Scope,
}

impl fmt::Display for Ssu2BindError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoSocket => "no SSU2 socket family selected",
            Self::Bind => "SSU2 socket bind failed",
            Self::State => "SSU2 service state unavailable",
            Self::Scope => "SSU2 loop task rejected by scope",
        })
    }
}

impl std::error::Error for Ssu2BindError {}

impl Ssu2RuntimeService {
    /// Dials one validated target and drives the handshake to an
    /// authenticated session over real UDP datagrams.
    ///
    /// The dial checks shared/per-transport backoff, acquires pending
    /// admission, follows the TokenRequest/Retry or cached-token path,
    /// authenticates the peer, and registers the active link only at
    /// the authenticated gate. A stale cached token falls back to the
    /// tokenless path inside the same dial timeout. No task is spawned
    /// per dial: retransmits are driven by the central scheduler while
    /// this future waits on a one-shot completion.
    pub async fn dial_ssu2(
        &self,
        target: Ssu2DialTarget,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<Ssu2EstablishedLink, Ssu2DialOutcome> {
        let outcome = self
            .dial_ssu2_inner(target, None, timeout, cancellation)
            .await;
        // Plan 547: the dial's own typed outcome is **reported**. The peer
        // pool reports a coarser reason, and a dial that failed for want of
        // admission reads the same as a dial the peer never answered.
        match &outcome {
            Ok(_) => {
                tracing::info!(
                    transition = "ssu2_outbound_dial_established",
                    "SSU2 outbound dial established a session"
                );
            }
            Err(error) => {
                tracing::info!(
                    transition = "ssu2_outbound_dial_failed",
                    dial_outcome = ?error,
                    "SSU2 outbound dial did not establish a session"
                );
            }
        }
        outcome
    }

    /// Dials one validated target, presenting an **explicit**
    /// establishment token.
    ///
    /// Plan 533. The relay establishment reaches the peer that was
    /// reached through a relay with a token the **responder** issued:
    /// the target's accept carries it, and the target's responder holds
    /// it in its own table, bound to this router's exact source address.
    /// This seam presents that token in the initiator's Session Request,
    /// so the responder's own table consumes it.
    ///
    /// An explicit token **never** falls back to the cached-token path.
    /// The cache is a FIFO over the tokens peers announced, so an older
    /// cached entry would be presented instead and the responder would
    /// refuse it as unknown -- a failure that has nothing to do with the
    /// peer's health. The cached entry is left untouched for the dial it
    /// was issued for.
    ///
    /// The token is one-use and source-bound, and both properties belong
    /// to the transport's own table: a zero token is refused here, and
    /// a token the responder did not issue, or one presented from
    /// another address, or one presented twice, is refused by the
    /// responder.
    pub async fn dial_ssu2_with_token(
        &self,
        target: Ssu2DialTarget,
        token: u64,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<Ssu2EstablishedLink, Ssu2DialOutcome> {
        if token == 0 {
            return Err(Ssu2DialOutcome::Failed);
        }
        self.dial_ssu2_inner(target, Some(token), timeout, cancellation)
            .await
    }

    /// The one dial, with an optional explicit establishment token.
    ///
    /// Plan 533. `explicit` is the accept's token when the relay
    /// establishment dials the peer it reached through a relay, and
    /// `None` for every ordinary dial.
    async fn dial_ssu2_inner(
        &self,
        target: Ssu2DialTarget,
        explicit: Option<u64>,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<Ssu2EstablishedLink, Ssu2DialOutcome> {
        if timeout.is_zero() || timeout > MAX_SSU2_RUNTIME_DURATION {
            return Err(Ssu2DialOutcome::Failed);
        }
        if cancellation.is_cancelled() || self.shared.shutdown.is_cancelled() {
            return Err(Ssu2DialOutcome::Cancelled);
        }
        if self.sockets_bound().is_none() {
            return Err(Ssu2DialOutcome::Failed);
        }
        let dial_key = DialKey::new(*target.expected_router_hash().as_bytes());
        if !matches!(
            self.shared.backoff.check(dial_key),
            DialBackoffDecision::Allowed
        ) {
            return Err(Ssu2DialOutcome::ResourceDenied);
        }
        let (completion_rx, local_conn_id) = {
            let now_ms = self.now_ms();
            let Ok(mut state) = self.shared.state.lock() else {
                return Err(Ssu2DialOutcome::Failed);
            };
            let limits = self.shared.config.limits;
            if state.pending_total() >= limits.max_pending_handshakes
                || state.pending_outbound_addrs.contains(&target.address())
            {
                return Err(Ssu2DialOutcome::ResourceDenied);
            }
            if !self.active_capacity_locked(&state, &target.address().ip()) {
                return Err(Ssu2DialOutcome::ResourceDenied);
            }
            let lease = match self.shared.manager.begin_handshake(target.peer()) {
                Ok(lease) => lease,
                Err(_) => return Err(Ssu2DialOutcome::ResourceDenied),
            };
            let link_id = match LinkId::generate() {
                Ok(id) => id,
                Err(_) => return Err(Ssu2DialOutcome::ResourceDenied),
            };
            let mut candidate = LinkCandidate::with_id(
                link_id,
                target.peer(),
                TransportKind::Ssu2,
                Direction::Outbound,
            );
            if candidate.begin_handshake().is_err() {
                return Err(Ssu2DialOutcome::Failed);
            }
            let static_secret = X25519PrivateKey::from_bytes(self.shared.local_static);
            let ephemeral = match ephemeral_secret() {
                Ok(secret) => secret,
                Err(_) => return Err(Ssu2DialOutcome::Failed),
            };
            let (local_conn_id, remote_conn_id) = match random_conn_ids() {
                Ok(ids) => ids,
                Err(_) => return Err(Ssu2DialOutcome::Failed),
            };
            let packet_number = match random_u32_nonzero() {
                Ok(value) => value,
                Err(_) => return Err(Ssu2DialOutcome::Failed),
            };
            let now_secs = wall_secs();
            let mut padding = [0_u8; 16];
            if fill_random(&mut padding).is_err() {
                return Err(Ssu2DialOutcome::Failed);
            }
            // An explicit establishment token is presented **as it is**:
            // the cached-token path is skipped, so an older cached entry
            // can never shadow it and the cache is left for its own dial.
            let cached = match explicit {
                Some(value) => Some(value),
                None => take_cached_token_locked(&mut state, &target.peer(), now_secs),
            };
            let (phase, token) = match cached {
                Some(value) => (
                    DialPhase::CachedToken {
                        grace_at_ms: now_ms.saturating_add(
                            SSU2_CACHED_TOKEN_GRACE
                                .as_millis()
                                .min(u128::from(u64::MAX)) as u64,
                        ),
                    },
                    Some(value),
                ),
                None => (DialPhase::Tokenless, None),
            };
            let config = InitiatorConfig {
                responder_static: target.responder_static(),
                responder_intro: target.responder_intro(),
                expected_router_hash: target.expected_router_hash(),
                clock: ClockSkewPolicy::handshake(),
                local_mtu: self.shared.local_mtu,
                // Plan 529: the initiator reports the address at which
                // it sees the responder, so the responder can compare
                // that observation with its own bound socket. The
                // value is the dial target's address -- the address
                // this router actually uses to reach the responder --
                // and never a configured or published value.
                remote_address: Ssu2Endpoint::from_socket_addr(target.address()).ok(),
            };
            let secrets = InitiatorSecrets {
                static_secret,
                ephemeral_secret: ephemeral,
                local_conn_id,
                remote_conn_id,
                packet_number,
                timestamp: now_secs.min(u64::from(u32::MAX)) as u32,
            };
            let (machine, actions) =
                match Initiator::begin(config, secrets, token, padding.to_vec(), now_ms) {
                    Ok(value) => value,
                    Err(_) => {
                        self.shared
                            .counters
                            .auth_failures
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(Ssu2DialOutcome::Failed);
                    }
                };
            let mut next_resend = u64::MAX;
            for action in &actions {
                if let HandshakeAction::ArmDeadline { kind, at_ms } = action
                    && *kind != DeadlineKind::Handshake
                {
                    next_resend = next_resend.min(*at_ms);
                }
            }
            if !stage_actions_locked(
                &mut state,
                &self.shared.config.limits,
                &self.shared.counters,
                &actions,
                &target.address(),
            ) {
                return Err(Ssu2DialOutcome::ResourceDenied);
            }
            let (completion_tx, completion_rx) = oneshot::channel();
            let ip = target.address().ip();
            let subnet = subnet_key(self.shared.config.prefixes, ip);
            count_up(&mut state.pending_ip, ip);
            subnet_up(&mut state.pending_subnet, subnet);
            state.pending_outbound_addrs.insert(target.address());
            state.pending_outbound.insert(
                local_conn_id,
                PendingOutbound {
                    machine: Some(machine),
                    target,
                    lease: Some(lease),
                    candidate,
                    dial_key,
                    phase,
                    token_used: token,
                    backstop_ms: now_ms.saturating_add(
                        self.shared
                            .config
                            .deadlines
                            .handshake
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64,
                    ),
                    next_resend_ms: next_resend,
                    completion: Some(completion_tx),
                },
            );
            (completion_rx, local_conn_id)
        };
        self.shared.notify.notify_one();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.abort_dial(local_conn_id);
                Err(Ssu2DialOutcome::Cancelled)
            }
            _ = self.shared.shutdown.cancelled() => {
                self.abort_dial(local_conn_id);
                Err(Ssu2DialOutcome::Cancelled)
            }
            result = tokio::time::timeout(timeout, completion_rx) => match result {
                Ok(Ok(DialCompletion::Established(link, used_cached))) => {
                    Ok(Ssu2EstablishedLink { link, used_cached_token: used_cached })
                }
                Ok(Ok(DialCompletion::Failed)) | Ok(Err(_)) => {
                    let _ = self.shared.backoff.record_failure(dial_key);
                    Err(Ssu2DialOutcome::Failed)
                }
                Err(_) => {
                    self.abort_dial(local_conn_id);
                    let _ = self.shared.backoff.record_failure(dial_key);
                    Err(Ssu2DialOutcome::Timeout)
                }
            },
        }
    }

    fn sockets_bound(&self) -> Option<(Option<SocketAddr>, Option<SocketAddr>)> {
        self.shared
            .sockets
            .lock()
            .map(|guard| {
                if guard.v4.is_none() && guard.v6.is_none() {
                    None
                } else {
                    Some((guard.v4, guard.v6))
                }
            })
            .unwrap_or(None)
    }

    fn active_capacity_locked(&self, state: &ServiceState, ip: &IpAddr) -> bool {
        let limits = self.shared.config.limits;
        if state.active.len() >= limits.max_active_sessions {
            return false;
        }
        if state.active_ip.get(ip).copied().unwrap_or(0) >= limits.max_active_per_ip {
            return false;
        }
        if state
            .active_subnet
            .get(&subnet_key(self.shared.config.prefixes, *ip))
            .copied()
            .unwrap_or(0)
            >= limits.max_active_per_subnet
        {
            return false;
        }
        true
    }

    fn abort_dial(&self, local_conn_id: u64) {
        if let Ok(mut state) = self.shared.state.lock()
            && let Some(entry) = state.pending_outbound.remove(&local_conn_id)
        {
            state.pending_outbound_addrs.remove(&entry.target.address());
            let ip = entry.target.address().ip();
            count_down(&mut state.pending_ip, &ip);
            subnet_down(
                &mut state.pending_subnet,
                &subnet_key(self.shared.config.prefixes, ip),
            );
        }
        self.shared.notify.notify_one();
    }

    /// Queues one encoded I2NP message onto the current session for a
    /// peer through the generic delivery contract.
    ///
    /// Admission flows through `delivery_capability` +
    /// `enqueue_on_link` (typed outcomes stay transport-neutral), then
    /// the admitted bytes enter the bounded session outbound queue and
    /// the central scheduler transmits them.
    pub fn send_i2np(
        &self,
        peer: PeerId,
        message: EncodedI2npMessage,
        timeout: Duration,
    ) -> Ssu2SendOutcome {
        self.send_i2np_with_link(peer, message, timeout).0
    }

    /// Queues one message and returns the exact authenticated session
    /// generation selected before admission, without installing a write
    /// receipt ticket.
    pub fn send_i2np_with_link(
        &self,
        peer: PeerId,
        message: EncodedI2npMessage,
        timeout: Duration,
    ) -> (Ssu2SendOutcome, Option<LinkId>) {
        let mut selected_link = None;
        let outcome = self.send_i2np_inner(peer, message, timeout, None, &mut selected_link);
        (outcome, selected_link)
    }

    /// Queues one message and returns a receiver whose result is produced by
    /// the authenticated session and UDP writer after every original SSU2
    /// fragment has been accepted by the socket.
    pub fn send_i2np_with_receipt(
        &self,
        peer: PeerId,
        message: EncodedI2npMessage,
        timeout: Duration,
    ) -> (
        Ssu2SendOutcome,
        Option<LinkId>,
        Option<tokio::sync::oneshot::Receiver<bool>>,
    ) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut selected_link = None;
        let outcome =
            self.send_i2np_inner(peer, message, timeout, Some(sender), &mut selected_link);
        if outcome == Ssu2SendOutcome::Accepted {
            (outcome, selected_link, Some(receiver))
        } else {
            (outcome, selected_link, None)
        }
    }

    fn send_i2np_inner(
        &self,
        peer: PeerId,
        message: EncodedI2npMessage,
        timeout: Duration,
        completion: Option<tokio::sync::oneshot::Sender<bool>>,
        selected_link: &mut Option<LinkId>,
    ) -> Ssu2SendOutcome {
        let now = self.service_now();
        let absolute = now.saturating_add(timeout);
        let deadline = match i2pr_transport::Deadline::new(absolute) {
            Ok(deadline) => deadline,
            Err(_) => return Ssu2SendOutcome::DeadlineElapsed,
        };
        let capability = match self.shared.manager.delivery_capability(peer) {
            Ok(capability) => capability,
            Err(_) => {
                self.shared
                    .counters
                    .send_without_link
                    .fetch_add(1, Ordering::Relaxed);
                return Ssu2SendOutcome::Closed;
            }
        };
        *selected_link = Some(capability_link_id(&capability));
        let request = match DeliveryRequest::new(peer, message, deadline) {
            Ok(request) => request,
            Err(_) => return Ssu2SendOutcome::ResourceDenied,
        };
        if request.deadline().is_elapsed(now) {
            return Ssu2SendOutcome::DeadlineElapsed;
        }
        let bytes = request.message_bytes().to_vec();
        let queued = match self
            .shared
            .manager
            .enqueue_on_link(capability, request, now)
        {
            Ok(queued) => queued,
            Err(outcome) => {
                return match outcome {
                    i2pr_transport::DeliveryOutcome::QueueFull => Ssu2SendOutcome::QueueFull,
                    i2pr_transport::DeliveryOutcome::ResourceDenied => {
                        Ssu2SendOutcome::ResourceDenied
                    }
                    i2pr_transport::DeliveryOutcome::DeadlineElapsed => {
                        Ssu2SendOutcome::DeadlineElapsed
                    }
                    _ => Ssu2SendOutcome::Closed,
                };
            }
        };
        let outcome = self.queue_to_session(
            capability_link_id(&capability),
            &bytes,
            u64::try_from(absolute.as_millis()).unwrap_or(u64::MAX),
            completion,
        );
        drop(queued);
        outcome
    }

    fn queue_to_session(
        &self,
        link_id: LinkId,
        bytes: &[u8],
        expires_ms: u64,
        completion: Option<tokio::sync::oneshot::Sender<bool>>,
    ) -> Ssu2SendOutcome {
        let Ok(mut state) = self.shared.state.lock() else {
            return Ssu2SendOutcome::Closed;
        };
        let (queued, peer, expected_fragments) = {
            let Some(record) = state.active.get_mut(&link_id) else {
                return Ssu2SendOutcome::Closed;
            };
            if record.session.is_terminated() {
                return Ssu2SendOutcome::Closed;
            }
            let (messages, queued_bytes) = record.session.outbound_pending();
            if messages >= SSU2_SEND_QUEUE_MESSAGES
                || queued_bytes.saturating_add(bytes.len()).saturating_add(64)
                    > SSU2_SEND_QUEUE_BYTES
            {
                return Ssu2SendOutcome::QueueFull;
            }
            let queued = record.session.queue_i2np_message(bytes.to_vec());
            let expected_fragments = queued
                .as_ref()
                .ok()
                .and_then(|message_id| record.session.outbound_fragment_count(*message_id));
            (queued, record.peer, expected_fragments)
        };
        match queued {
            Ok(message_id) => {
                if let Some(completion) = completion {
                    let key = (link_id, message_id);
                    if let Some(expected_fragments) = expected_fragments
                        && state.pending_write_receipts.len() < SSU2_MAX_PENDING_WRITE_RECEIPTS
                        && !state.pending_write_receipts.contains_key(&key)
                    {
                        state.pending_write_receipts.insert(
                            key,
                            PendingSsu2WriteReceipt {
                                expected_fragments,
                                written_fragments: HashSet::new(),
                                expires_ms,
                                completion,
                            },
                        );
                    } else {
                        let _ = completion.send(false);
                    }
                }
                if let Some(observer) = &self.shared.qualification_observer {
                    observer.record(
                        peer,
                        link_id,
                        crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npQueueAccepted,
                        &[message_id],
                    );
                }
                self.shared
                    .counters
                    .i2np_sent
                    .fetch_add(1, Ordering::Relaxed);
                self.shared.notify.notify_one();
                Ssu2SendOutcome::Accepted
            }
            Err(i2pr_transport_ssu2::SessionError::OutboundQueueFull) => Ssu2SendOutcome::QueueFull,
            Err(i2pr_transport_ssu2::SessionError::MessageTooLarge) => Ssu2SendOutcome::TooLarge,
            Err(_) => Ssu2SendOutcome::Closed,
        }
    }

    /// Closes exactly one link: emits a termination block when the
    /// session is live, then removes the exact manager link without
    /// disturbing a replacement link (stale closes are no-ops).
    pub fn close_ssu2(
        &self,
        link_id: LinkId,
        reason: TerminationCategory,
    ) -> i2pr_transport::CloseOutcome {
        let mut finals = Vec::new();
        if let Ok(mut state) = self.shared.state.lock()
            && let Some(mut record) = state.active.remove(&link_id)
        {
            let addr = record.peer_addr;
            let code = termination_code(reason);
            if record.session.initiate_termination(code).is_ok() {
                let now_ms = monotonic_ms(&self.shared.started_at);
                for _ in 0..2 {
                    if let Some(datagram) = record.session.poll_transmit(now_ms) {
                        finals.push(datagram);
                    } else {
                        break;
                    }
                }
            }
            release_active_locked(&mut state, &record);
            remove_peer_link_locked(&mut state, &record.peer, &link_id);
            self.shared
                .counters
                .sessions_closed
                .fetch_add(1, Ordering::Relaxed);
            stage_finals_locked(
                &mut state,
                &self.shared.config.limits,
                &self.shared.counters,
                finals,
                &addr,
            );
        }
        self.shared.notify.notify_one();
        match self.shared.manager.close_link(link_id, reason) {
            Ok(outcome) => outcome,
            Err(_) => i2pr_transport::CloseOutcome::Stale { link_id },
        }
    }
}

/// Maximum outbound I2NP messages staged per session by `send_i2np`.
pub const SSU2_SEND_QUEUE_MESSAGES: usize = 64;
/// Maximum estimated outbound I2NP bytes staged per session.
pub const SSU2_SEND_QUEUE_BYTES: usize = 256 * 1024;
/// Maximum number of process-local delivery receipts retained while socket
/// writers settle fragments. This is global to one runtime and fail-closed.
pub const SSU2_MAX_PENDING_WRITE_RECEIPTS: usize = 512;

/// Maps a close reason to a wire termination code.
fn termination_code(reason: TerminationCategory) -> u8 {
    match reason {
        TerminationCategory::Timeout => 2,
        TerminationCategory::LocalShutdown
        | TerminationCategory::RemoteTermination
        | TerminationCategory::AuthenticationFailure
        | TerminationCategory::ReplayOrSkewRejection
        | TerminationCategory::MalformedFraming
        | TerminationCategory::QueueExhaustion
        | TerminationCategory::ResourceExhaustion
        | TerminationCategory::DuplicateReplacement
        | TerminationCategory::IoClosure => 0,
    }
}

fn capability_link_id(capability: &i2pr_transport::LinkDeliveryCapability) -> LinkId {
    capability.link_id()
}

fn release_active_locked(state: &mut ServiceState, record: &ActiveSession) {
    count_down(&mut state.active_ip, &record.peer_addr.ip());
    subnet_down(&mut state.active_subnet, &record.subnet);
}

/// Resolves one peer to exactly one live link (Plan 499).
///
/// The session registry keys live links by peer, and the generic
/// transport manager keeps at most one active link per peer, so the
/// ambiguous case is a guard rather than a routine. It is refused, never
/// guessed. A peer with no link — or an empty link set — has no session.
fn resolve_single_peer_link(
    links: Option<&HashSet<LinkId>>,
) -> Result<LinkId, Ssu2RelayQueueError> {
    match links {
        Some(links) if links.is_empty() => Err(Ssu2RelayQueueError::NoSession),
        Some(links) if links.len() == 1 => links
            .iter()
            .next()
            .copied()
            .ok_or(Ssu2RelayQueueError::NoSession),
        Some(_) => Err(Ssu2RelayQueueError::AmbiguousSession),
        None => Err(Ssu2RelayQueueError::NoSession),
    }
}

/// Maps one transport session error onto the runtime's carriage refusal.
///
/// The transport refuses a terminated session and a full control queue.
/// Every other failure is treated as a terminated session so the runtime
/// fails closed rather than retrying a block it cannot carry.
fn relay_queue_reason(error: i2pr_transport_ssu2::SessionError) -> Ssu2RelayQueueError {
    match error {
        i2pr_transport_ssu2::SessionError::Terminated => Ssu2RelayQueueError::SessionTerminated,
        i2pr_transport_ssu2::SessionError::LocalPolicyDenied => {
            Ssu2RelayQueueError::ControlQueueFull
        }
        _ => Ssu2RelayQueueError::SessionTerminated,
    }
}

fn remove_peer_link_locked(state: &mut ServiceState, peer: &PeerId, link_id: &LinkId) {
    if let Some(links) = state.peer_links.get_mut(peer) {
        links.remove(link_id);
        if links.is_empty() {
            state.peer_links.remove(peer);
        }
    }
    let stale_receipts = state
        .pending_write_receipts
        .keys()
        .filter(|(receipt_link, _)| receipt_link == link_id)
        .copied()
        .collect::<Vec<_>>();
    for key in stale_receipts {
        if let Some(receipt) = state.pending_write_receipts.remove(&key) {
            let _ = receipt.completion.send(false);
        }
    }
}

fn settle_ssu2_write_receipts(
    state: &mut ServiceState,
    link_id: LinkId,
    fragments: &[i2pr_transport_ssu2::session::I2npFragmentReceipt],
    written: bool,
) {
    for fragment in fragments {
        let key = (link_id, fragment.message_id);
        let result = state
            .pending_write_receipts
            .get_mut(&key)
            .and_then(|receipt| settle_ssu2_write_fragment(receipt, *fragment, written));
        if let Some(result) = result
            && let Some(receipt) = state.pending_write_receipts.remove(&key)
        {
            let _ = receipt.completion.send(result);
        }
    }
}

fn settle_ssu2_write_fragment(
    receipt: &mut PendingSsu2WriteReceipt,
    fragment: i2pr_transport_ssu2::session::I2npFragmentReceipt,
    written: bool,
) -> Option<bool> {
    if !written
        || fragment.total_fragments != receipt.expected_fragments
        || fragment.fragment_number >= receipt.expected_fragments
    {
        return Some(false);
    }
    receipt.written_fragments.insert(fragment.fragment_number);
    (receipt.written_fragments.len() == usize::from(receipt.expected_fragments)).then_some(true)
}

/// Stages handshake datagrams from machine actions into the
/// family-split outbound queues. Returns false when the staging bound
/// is full (the caller denies the operation; the handshake schedule
/// retries later).
fn stage_actions_locked(
    state: &mut ServiceState,
    limits: &Ssu2RuntimeLimits,
    counters: &ServiceCounters,
    actions: &[HandshakeAction],
    addr: &SocketAddr,
) -> bool {
    let mut staged = 0_usize;
    for action in actions {
        if let HandshakeAction::WriteDatagram(bytes) = action {
            if !stage_one_locked(state, limits, counters, bytes.as_bytes().to_vec(), *addr) {
                return false;
            }
            staged += 1;
            let _ = staged;
        }
    }
    true
}

/// Returns whether a destination is inside the router's own dial profile.
///
/// The profile is the **union** of the two dial constructors the runtime
/// offers: a loopback address of either family, or an RFC1918 IPv4 literal
/// for the private-LAN variant. A hole punch may go nowhere else, so the
/// target role can never widen the router's reach beyond what the router
/// already dials.
fn is_dial_profile_address(address: &SocketAddr) -> bool {
    if address.port() == 0 || address.ip().is_unspecified() {
        return false;
    }
    match address.ip() {
        std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        std::net::IpAddr::V6(ip) => ip.is_loopback(),
    }
}

fn stage_one_locked(
    state: &mut ServiceState,
    limits: &Ssu2RuntimeLimits,
    counters: &ServiceCounters,
    bytes: Vec<u8>,
    addr: SocketAddr,
) -> bool {
    stage_class_locked(
        state,
        limits,
        counters,
        bytes,
        addr,
        BandwidthTrafficClass::Client,
        TracedI2np::default(),
    )
}

fn stage_traced_locked(
    state: &mut ServiceState,
    limits: &Ssu2RuntimeLimits,
    counters: &ServiceCounters,
    bytes: Vec<u8>,
    addr: SocketAddr,
    trace: TracedStage<'_>,
) -> bool {
    let TracedStage {
        message_ids,
        fragments,
        context,
    } = trace;
    let receipt_ids = message_ids.clone();
    let fragments = fragments
        .into_iter()
        .filter(|fragment| {
            state
                .pending_write_receipts
                .contains_key(&(context.source_link, fragment.message_id))
        })
        .collect::<Vec<_>>();
    let source = state
        .active
        .get(&context.source_link)
        .map(|record| (record.peer, record.link_id));
    let accepted = stage_class_locked(
        state,
        limits,
        counters,
        bytes,
        addr,
        BandwidthTrafficClass::Client,
        TracedI2np {
            message_ids,
            fragments: fragments.clone(),
            source,
        },
    );
    if !accepted && !receipt_ids.is_empty() {
        if let (Some(observer), Some((peer, link_id))) = (context.observer, source) {
            observer.record(
                peer,
                link_id,
                crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramStageRejected,
                &receipt_ids,
            );
        }
        tracing::warn!(
            i2np_message_count = receipt_ids.len(),
            transition = "datagram_staging_rejected",
            "SSU2 message send receipt"
        );
        for fragment in fragments {
            if let Some(receipt) = state
                .pending_write_receipts
                .remove(&(context.source_link, fragment.message_id))
            {
                let _ = receipt.completion.send(false);
            }
        }
    }
    accepted
}

fn stage_class_locked(
    state: &mut ServiceState,
    limits: &Ssu2RuntimeLimits,
    counters: &ServiceCounters,
    bytes: Vec<u8>,
    addr: SocketAddr,
    class: BandwidthTrafficClass,
    trace: TracedI2np,
) -> bool {
    if bytes.is_empty() || bytes.len() > constants::MAX_DATAGRAM_IPV4_LENGTH {
        counters.cheap_drops.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    let queue_len = state.staged_v4.len() + state.staged_v6.len();
    if queue_len >= limits.max_outbound_datagrams
        || state.staged_bytes.saturating_add(bytes.len()) > limits.max_outbound_bytes
    {
        counters.staging_drops.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    state.staged_bytes = state.staged_bytes.saturating_add(bytes.len());
    let staged = StagedDatagram {
        bytes,
        addr,
        class,
        trace,
    };
    match addr.ip() {
        IpAddr::V4(_) => state.staged_v4.push_back(staged),
        IpAddr::V6(_) => state.staged_v6.push_back(staged),
    }
    true
}

fn stage_finals_locked(
    state: &mut ServiceState,
    limits: &Ssu2RuntimeLimits,
    counters: &ServiceCounters,
    finals: Vec<Vec<u8>>,
    addr: &SocketAddr,
) {
    for bytes in finals {
        let _ = stage_class_locked(
            state,
            limits,
            counters,
            bytes,
            *addr,
            BandwidthTrafficClass::Control,
            TracedI2np::default(),
        );
    }
}

fn take_cached_token_locked(state: &mut ServiceState, peer: &PeerId, now_secs: u64) -> Option<u64> {
    let entry = state.token_cache.get_mut(peer)?;
    entry.retain(|cached| cached.expires_secs > now_secs);
    let position = entry.iter().position(|_| true)?;
    let cached = entry.remove(position);
    if entry.is_empty() {
        state.token_cache.remove(peer);
    }
    if cached.expires_secs > now_secs && cached.value != 0 {
        Some(cached.value)
    } else {
        None
    }
}

fn note_cached_token_locked(
    state: &mut ServiceState,
    peer: &PeerId,
    value: u64,
    expires_secs: u64,
    now_secs: u64,
) {
    if value == 0 || expires_secs <= now_secs {
        return;
    }
    let entry = state.token_cache.entry(*peer).or_default();
    entry.retain(|cached| cached.expires_secs > now_secs);
    if entry.len() >= SSU2_TOKEN_CACHE_PER_PEER {
        entry.remove(0);
    }
    entry.push(CachedToken {
        value,
        expires_secs,
    });
    while state.token_cache.len() > SSU2_TOKEN_CACHE_PEERS {
        let oldest = state.token_cache.keys().next().copied();
        match oldest {
            Some(peer) => {
                state.token_cache.remove(&peer);
            }
            None => break,
        }
    }
}

/// Extracts the peer intro key from validated RouterInfo bytes by
/// binding the handshake static key to a strict v2 address.
fn peer_intro_from_router_info(router_info: &[u8], expected_static: &[u8; 32]) -> Option<IntroKey> {
    let info =
        RouterInfo::decode(router_info, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES).ok()?;
    for address in info.addresses() {
        if address.transport_style() != "SSU2" {
            continue;
        }
        let parsed = Ssu2RouterAddress::parse(address).ok()?;
        if parsed.static_public_key().as_bytes() != expected_static {
            continue;
        }
        let intro = parsed.intro_key()?;
        return Some(IntroKey::new(*intro.as_bytes()));
    }
    None
}

impl Ssu2RuntimeService {
    /// Builds the data-phase session for a freshly authenticated peer
    /// and issues one NewToken announcement for its next handshake.
    /// Returns the session plus its connection-ID pair.
    fn construct_session(
        &self,
        state: &mut ServiceState,
        auth: AuthenticatedSsu2Session,
        remote_intro: IntroKey,
        peer_addr: SocketAddr,
        direction: Direction,
    ) -> Option<i2pr_transport_ssu2::Ssu2Session> {
        let local_conn_id = auth.local_conn_id();
        let remote_conn_id = auth.remote_conn_id();
        let idle_ms = self
            .shared
            .config
            .deadlines
            .idle
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        // Plan 522: the keepalive is an **outbound** behaviour. An
        // inbound session's lifetime is exactly what it was before
        // this plan, so the inbound path is untouched.
        let keepalive_interval_ms = match direction {
            Direction::Outbound => self
                .shared
                .config
                .deadlines
                .keepalive
                .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64),
            Direction::Inbound => None,
        };
        let config = SessionConfig {
            local_conn_id,
            remote_conn_id,
            local_intro: self.shared.local_intro,
            remote_intro,
            initial_send_packet_number: 0,
            max_payload_bytes: SessionConfig::max_payload_for_mtu(self.shared.local_mtu, false),
            idle_timeout_ms: idle_ms,
            keepalive_interval_ms,
        };
        // Plan 529: the handshake's Address block is the peer's
        // authenticated observation of the address at which it sees
        // this router. It is the corroboration class the runtime could
        // not earn before: the data-phase seam existed, but no
        // handshake ever reached it, so the observation was never
        // produced in the field.
        //
        // The value is captured before the authenticated session is
        // consumed, and recorded only once a session was really
        // established: a corroboration class is never counted for a
        // session that does not exist.
        let observed_address = auth.observed_address();
        let mut session = i2pr_transport_ssu2::Ssu2Session::new(config, auth.into_keys()).ok()?;
        // The comparison is against this router's **own bound socket**,
        // and a disagreement is recorded as the firewalled verdict. A
        // peer that sent no Address block produces no observation at
        // all -- evidence absent, never a handshake failure.
        if let Some(observed) = observed_address {
            self.note_observed_external_address_locked(state, observed);
        }
        // One spare token for the peer's next handshake, announced
        // in-band. Failures (RNG/table) only forfeit the cached-token
        // fast path; the Retry path always remains. Data-phase tokens
        // carry no responder connection ID: the next handshake mints a
        // fresh ID at SessionCreated.
        let now_secs = wall_secs();
        if !session.is_terminated() {
            let mut token_bytes = [0_u8; 8];
            if fill_random(&mut token_bytes).is_ok()
                && let Ok(token) = state
                    .token_store
                    .issue(peer_addr, now_secs, token_bytes, None)
            {
                let expires = now_secs
                    .saturating_add(constants::TOKEN_LIFETIME_SECONDS)
                    .min(u64::from(u32::MAX)) as u32;
                let _ = session.queue_new_token(expires, token.value());
            }
        }
        Some(session)
    }

    /// Builds the initial validated-path state for a promotion address.
    ///
    /// The validated endpoint starts as the authenticated promotion
    /// address with the configured local MTU; migration afterwards
    /// requires matching PathResponse proof (Plan 159 §2).
    fn initial_path(&self, addr: SocketAddr) -> Option<PathValidator> {
        let endpoint = Ssu2Endpoint::from_socket_addr(addr).ok()?;
        PathValidator::new(endpoint, self.shared.local_mtu).ok()
    }

    fn remove_active_locked(&self, state: &mut ServiceState, link_id: &LinkId) {
        if let Some(record) = state.active.remove(link_id) {
            if let Some(observer) = &self.shared.qualification_observer {
                observer.record(
                    record.peer,
                    record.link_id,
                    crate::ssu2_qualification_observer::Ssu2QualificationEventKind::SessionClosed,
                    &[],
                );
            }
            release_active_locked(state, &record);
            remove_peer_link_locked(state, &record.peer, link_id);
            self.shared
                .counters
                .sessions_closed
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn insert_active_locked(&self, state: &mut ServiceState, record: ActiveSession) {
        if let Some(observer) = &self.shared.qualification_observer {
            observer.record(
                record.peer,
                record.link_id,
                match record.direction {
                    Direction::Inbound => crate::ssu2_qualification_observer::Ssu2QualificationEventKind::InboundSessionAuthenticated,
                    Direction::Outbound => crate::ssu2_qualification_observer::Ssu2QualificationEventKind::OutboundSessionAuthenticated,
                },
                &[],
            );
        }
        let ip = record.peer_addr.ip();
        let subnet = record.subnet;
        count_up(&mut state.active_ip, ip);
        subnet_up(&mut state.active_subnet, subnet);
        state
            .peer_links
            .entry(record.peer)
            .or_default()
            .insert(record.link_id);
        state.active.insert(record.link_id, record);
        self.shared
            .counters
            .sessions_established
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Promotes a completed outbound handshake: builds the session,
    /// registers the exact manager link at the authenticated gate, and
    /// completes the waiting dial.
    fn promote_outbound_locked(
        &self,
        state: &mut ServiceState,
        local_key: u64,
        auth: AuthenticatedSsu2Session,
        confirmed: Vec<Vec<u8>>,
    ) {
        let Some(mut entry) = state.pending_outbound.remove(&local_key) else {
            self.shared
                .counters
                .protocol_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        state.pending_outbound_addrs.remove(&entry.target.address());
        let ip = entry.target.address().ip();
        count_down(&mut state.pending_ip, &ip);
        subnet_down(
            &mut state.pending_subnet,
            &subnet_key(self.shared.config.prefixes, ip),
        );
        let completion = entry.completion.take();
        let fail = |completion: Option<oneshot::Sender<DialCompletion>>| {
            if let Some(tx) = completion {
                let _ = tx.send(DialCompletion::Failed);
            }
        };
        let target = entry.target;
        // A peer-issued establishment token (SessionCreated NewToken)
        // skips the next Retry round trip. Expired/zero announcements
        // are dropped by the cache; the Retry path always remains.
        if let Some(announced) = auth.peer_new_token() {
            note_cached_token_locked(
                state,
                &target.peer(),
                announced.token(),
                u64::from(announced.expires()),
                wall_secs(),
            );
        }
        let peer_router_info = auth.peer().router_info.clone();
        let session = match self.construct_session(
            state,
            auth,
            target.responder_intro(),
            target.address(),
            Direction::Outbound,
        ) {
            Some(session) => session,
            None => {
                self.shared
                    .counters
                    .auth_failures
                    .fetch_add(1, Ordering::Relaxed);
                fail(completion);
                return;
            }
        };
        if !self.active_capacity_locked(state, &target.address().ip()) {
            self.shared
                .counters
                .protocol_drops
                .fetch_add(1, Ordering::Relaxed);
            fail(completion);
            return;
        }
        let mut candidate = entry.candidate;
        if candidate.authenticate().is_err() {
            fail(completion);
            return;
        }
        let duplicate = match self
            .shared
            .manager
            .duplicate_resolution(self.shared.local_peer, &candidate)
        {
            Ok(duplicate) => duplicate,
            Err(_) => {
                fail(completion);
                return;
            }
        };
        let lease = match entry.lease.take() {
            Some(lease) => lease,
            None => {
                fail(completion);
                return;
            }
        };
        let link_id = candidate.link_id();
        match lease.register(
            &self.shared.manager,
            candidate,
            self.service_now(),
            duplicate,
        ) {
            Ok(CandidateDecision::AcceptFirst { .. })
            | Ok(CandidateDecision::AcceptAdditional { .. }) => {}
            Ok(CandidateDecision::ReplaceExisting {
                existing,
                candidate,
            }) => {
                self.remove_active_locked(state, &existing);
                let _ = candidate;
            }
            Ok(_) => {
                self.shared
                    .counters
                    .duplicate_resolutions
                    .fetch_add(1, Ordering::Relaxed);
                fail(completion);
                return;
            }
            Err(_) => {
                fail(completion);
                return;
            }
        }
        let now_ms = monotonic_ms(&self.shared.started_at);
        let confirmed_resend = if confirmed.is_empty() {
            None
        } else {
            Some(ConfirmedResend {
                datagrams: confirmed,
                next_at_ms: now_ms.saturating_add(constants::SESSION_CONFIRMED_RESEND_DELAYS_MS[0]),
                attempts: 1,
            })
        };
        let link = Ssu2LinkHandle {
            shared: Arc::clone(&self.shared),
            link_id,
            peer: target.peer(),
        };
        let used_cached = entry.token_used.is_some();
        let path = match self.initial_path(target.address()) {
            Some(path) => path,
            None => {
                self.shared
                    .counters
                    .auth_failures
                    .fetch_add(1, Ordering::Relaxed);
                let _ = self
                    .shared
                    .manager
                    .close_link(link_id, TerminationCategory::AuthenticationFailure);
                fail(completion);
                return;
            }
        };
        self.insert_active_locked(
            state,
            ActiveSession {
                link_id,
                peer: target.peer(),
                direction: Direction::Outbound,
                peer_addr: target.address(),
                subnet: subnet_key(self.shared.config.prefixes, target.address().ip()),
                session,
                peer_router_info,
                path,
                next_revalidation_ms: now_ms.saturating_add(self.revalidation_interval_ms()),
                confirmed_resend,
                data_rx_observed: false,
            },
        );
        self.shared.backoff.clear(entry.dial_key);
        if let Some(tx) = completion {
            let _ = tx.send(DialCompletion::Established(link, used_cached));
        }
    }

    /// Promotes a completed inbound handshake through a fresh
    /// peer-bound manager lease with no await between admission and
    /// registration.
    fn promote_inbound_locked(
        &self,
        state: &mut ServiceState,
        inbound_key: u64,
        auth: AuthenticatedSsu2Session,
        source: SocketAddr,
    ) {
        let Some(entry) = state.pending_inbound.remove(&inbound_key) else {
            self.shared
                .counters
                .protocol_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        count_down(&mut state.pending_ip, &source.ip());
        subnet_down(
            &mut state.pending_subnet,
            &subnet_key(self.shared.config.prefixes, source.ip()),
        );
        let peer = PeerId::from_hash(auth.peer().router_hash);
        let remote_intro = match peer_intro_from_router_info(
            &auth.peer().router_info,
            auth.peer().transport_static_key.as_bytes(),
        ) {
            Some(intro) => intro,
            None => {
                self.shared
                    .counters
                    .auth_failures
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let peer_router_info = auth.peer().router_info.clone();
        let mut session =
            match self.construct_session(state, auth, remote_intro, source, Direction::Inbound) {
                Some(session) => session,
                None => {
                    self.shared
                        .counters
                        .auth_failures
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
            };
        // Release the peer initiator immediately: it establishes only
        // on its first received ACK block, which may otherwise wait
        // for ack-eliciting data that never comes (proven against
        // exact-pinned i2pd 2.61.0 in Plan 161 direction B). The
        // honest zero ACK piggybacks on the NewToken packet below.
        session.request_bootstrap_ack();
        if !self.active_capacity_locked(state, &source.ip()) {
            self.shared
                .counters
                .protocol_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let link_id = match LinkId::generate() {
            Ok(id) => id,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let mut candidate =
            LinkCandidate::with_id(link_id, peer, TransportKind::Ssu2, Direction::Inbound);
        if candidate.begin_handshake().is_err() || candidate.authenticate().is_err() {
            self.shared
                .counters
                .protocol_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let duplicate = match self
            .shared
            .manager
            .duplicate_resolution(self.shared.local_peer, &candidate)
        {
            Ok(duplicate) => duplicate,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let lease = match self.shared.manager.begin_handshake(peer) {
            Ok(lease) => lease,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        match lease.register(
            &self.shared.manager,
            candidate,
            self.service_now(),
            duplicate,
        ) {
            Ok(CandidateDecision::AcceptFirst { .. })
            | Ok(CandidateDecision::AcceptAdditional { .. }) => {}
            Ok(CandidateDecision::ReplaceExisting {
                existing,
                candidate,
            }) => {
                self.remove_active_locked(state, &existing);
                let _ = candidate;
            }
            Ok(_) => {
                self.shared
                    .counters
                    .duplicate_resolutions
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // The peer-agnostic pending lease releases only after the
        // peer-bound active registration succeeds: no admission gap.
        drop(entry.lease);
        let path = match self.initial_path(source) {
            Some(path) => path,
            None => {
                self.shared
                    .counters
                    .auth_failures
                    .fetch_add(1, Ordering::Relaxed);
                let _ = self
                    .shared
                    .manager
                    .close_link(link_id, TerminationCategory::AuthenticationFailure);
                return;
            }
        };
        self.insert_active_locked(
            state,
            ActiveSession {
                link_id,
                peer,
                direction: Direction::Inbound,
                peer_addr: source,
                subnet: subnet_key(self.shared.config.prefixes, source.ip()),
                session,
                peer_router_info,
                path,
                next_revalidation_ms: monotonic_ms(&self.shared.started_at)
                    .saturating_add(self.revalidation_interval_ms()),
                confirmed_resend: None,
                data_rx_observed: false,
            },
        );
        // Flush the bootstrap ACK (+ NewToken) immediately so the peer
        // initiator establishes without waiting for our first I2NP.
        let now_ms = self.now_ms();
        if let Some(record) = state.active.get_mut(&link_id)
            && let Some(datagram) = record.session.poll_transmit(now_ms)
        {
            stage_one_locked(
                state,
                &self.shared.config.limits,
                &self.shared.counters,
                datagram,
                source,
            );
        }
    }
}

impl Ssu2RuntimeService {
    fn responder_config(&self) -> ResponderConfig {
        // Plan 529: the responder no longer publishes its own bound
        // address. The specification's Address block carries the
        // address at which the **recipient** sees the **sender**, so
        // the responder's block is built from the request's source --
        // the address at which this router sees the initiator -- and
        // never from the responder's own bound socket. The responder's
        // own address is what the **initiator** reports back.
        ResponderConfig {
            static_secret: X25519PrivateKey::from_bytes(self.shared.local_static),
            intro_key: self.shared.local_intro,
            expected_peer_hash: None,
            clock: ClockSkewPolicy::handshake(),
            local_mtu: self.shared.local_mtu,
        }
    }

    fn count_drop_category(&self, category: DropCategory) {
        let counters = &self.shared.counters;
        // Plan 547: the drop is **reported**. A silent drop is exactly the
        // kind of event that makes a router that is being refused look like
        // a router nobody answers, and the two have opposite causes. The
        // report carries the category only: no peer, no address, no key and
        // no payload.
        tracing::info!(
            transition = "ssu2_inbound_datagram_dropped",
            drop_category = ?category,
            "SSU2 inbound datagram dropped"
        );
        match category {
            DropCategory::Malformed | DropCategory::VersionNetworkType => {
                counters.cheap_drops.fetch_add(1, Ordering::Relaxed);
            }
            DropCategory::BadToken => {
                counters.token_rejections.fetch_add(1, Ordering::Relaxed);
            }
            DropCategory::Replay => {
                counters.replay_rejections.fetch_add(1, Ordering::Relaxed);
            }
            DropCategory::ClockSkew
            | DropCategory::PeerTerminated
            | DropCategory::Unexpected
            | DropCategory::ConnectionIdMismatch => {
                counters.protocol_drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn count_terminate_reason(&self, reason: TerminateReason) {
        let counters = &self.shared.counters;
        // Plan 547: the termination is **reported**. Without it, a refused
        // establishment reads exactly like an establishment that was never
        // attempted, and a router that is being refused is diagnosed as a
        // router nobody answers. The report carries the reason only.
        tracing::info!(
            transition = "ssu2_handshake_terminated",
            terminate_reason = ?reason,
            "SSU2 handshake terminated"
        );
        match reason {
            TerminateReason::HandshakeTimeout | TerminateReason::RetriesExhausted => {
                counters.handshake_timeouts.fetch_add(1, Ordering::Relaxed);
            }
            TerminateReason::AuthenticationFailed | TerminateReason::RouterInfoRejected => {
                counters.auth_failures.fetch_add(1, Ordering::Relaxed);
            }
            TerminateReason::TokenRejected => {
                counters.token_rejections.fetch_add(1, Ordering::Relaxed);
            }
            TerminateReason::ReplayDetected => {
                counters.replay_rejections.fetch_add(1, Ordering::Relaxed);
            }
            TerminateReason::Cancelled
            | TerminateReason::PeerTerminated
            | TerminateReason::ProtocolViolation => {
                counters.protocol_drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Applies initiator actions: stages datagrams, arms resends,
    /// counts silent drops, and reports establishment/termination.
    /// Returns the confirmed-fragment bytes when the SessionCreated
    /// path produced them (for post-promotion resends).
    ///
    /// A batch carrying resend arms supersedes the previous schedule:
    /// the next deadline is replaced by this batch's minimum, never
    /// min-ed with a stale past value (which would refire instantly
    /// and burn the retry budget).
    fn apply_initiator_actions(
        &self,
        state: &mut ServiceState,
        entry: &mut PendingOutbound,
        actions: Vec<HandshakeAction>,
        collect_confirmed: bool,
    ) -> (
        Option<AuthenticatedSsu2Session>,
        Option<TerminateReason>,
        Vec<Vec<u8>>,
    ) {
        let mut established = None;
        let mut terminated = None;
        let mut confirmed = Vec::new();
        let mut batch_resend = None;
        for action in actions {
            match action {
                HandshakeAction::WriteDatagram(bytes) => {
                    let raw = bytes.as_bytes().to_vec();
                    if collect_confirmed {
                        confirmed.push(raw.clone());
                    }
                    stage_one_locked(
                        state,
                        &self.shared.config.limits,
                        &self.shared.counters,
                        raw,
                        entry.target.address(),
                    );
                }
                HandshakeAction::ArmDeadline { kind, at_ms } => {
                    if kind != DeadlineKind::Handshake {
                        batch_resend = Some(batch_resend.map_or(at_ms, |min: u64| min.min(at_ms)));
                    }
                }
                HandshakeAction::Established(auth) => {
                    established = Some(auth);
                }
                HandshakeAction::Terminate(reason) => {
                    terminated = Some(reason);
                }
                HandshakeAction::DropSilently(category) => {
                    self.count_drop_category(category);
                }
            }
        }
        if let Some(next) = batch_resend {
            entry.next_resend_ms = next;
        }
        (established, terminated, confirmed)
    }
}

impl Ssu2RuntimeService {
    fn fail_outbound_locked(
        &self,
        state: &mut ServiceState,
        key: u64,
        reason: Option<TerminateReason>,
    ) {
        if let Some(entry) = state.pending_outbound.remove(&key) {
            state.pending_outbound_addrs.remove(&entry.target.address());
            let ip = entry.target.address().ip();
            count_down(&mut state.pending_ip, &ip);
            subnet_down(
                &mut state.pending_subnet,
                &subnet_key(self.shared.config.prefixes, ip),
            );
            if let Some(reason) = reason {
                self.count_terminate_reason(reason);
            }
            if let Some(tx) = entry.completion {
                let _ = tx.send(DialCompletion::Failed);
            }
        }
    }

    /// Drives one pending outbound handshake with an inbound datagram
    /// from its dialed address. Returns true when consumed.
    fn drive_pending_outbound(
        &self,
        state: &mut ServiceState,
        key: u64,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
    ) -> bool {
        let mut entry = match state.pending_outbound.remove(&key) {
            Some(entry) => entry,
            None => return false,
        };
        let Some(machine) = entry.machine.take() else {
            state.pending_outbound.insert(key, entry);
            return true;
        };
        if now_ms >= entry.backstop_ms {
            state.pending_outbound.insert(key, entry);
            self.shared
                .counters
                .handshake_timeouts
                .fetch_add(1, Ordering::Relaxed);
            self.fail_outbound_locked(state, key, None);
            return true;
        }
        let now32 = now_secs.min(u64::from(u32::MAX)) as u32;
        let answer = (|| {
            let mut padding = [0_u8; 8];
            fill_random(&mut padding).ok()?;
            Some(RetryAnswer {
                ephemeral_secret: ephemeral_secret().ok()?,
                packet_number: random_u32_nonzero().ok()?,
                timestamp: now32,
                padding: padding.to_vec(),
            })
        })();
        let Some(answer) = answer else {
            entry.machine = Some(machine);
            state.pending_outbound.insert(key, entry);
            self.shared
                .counters
                .auth_failures
                .fetch_add(1, Ordering::Relaxed);
            return true;
        };
        let (after_retry, actions) =
            match machine.on_retry(datagram.clone(), answer, now_ms, now_secs) {
                Ok(value) => value,
                Err(_) => {
                    self.shared
                        .counters
                        .auth_failures
                        .fetch_add(1, Ordering::Relaxed);
                    state.pending_outbound.insert(key, entry);
                    self.fail_outbound_locked(state, key, None);
                    return true;
                }
            };
        let retry_silent = actions
            .iter()
            .all(|action| matches!(action, HandshakeAction::DropSilently(_)));
        if !retry_silent {
            let (established, terminated, _) =
                self.apply_initiator_actions(state, &mut entry, actions, false);
            if let Some(auth) = established {
                state.pending_outbound.insert(key, entry);
                self.promote_outbound_locked(state, key, auth, Vec::new());
                return true;
            }
            if let Some(reason) = terminated {
                state.pending_outbound.insert(key, entry);
                self.fail_outbound_locked(state, key, Some(reason));
                return true;
            }
            entry.machine = Some(after_retry);
            state.pending_outbound.insert(key, entry);
            return true;
        }
        // Not a Retry for this handshake: offer it as SessionCreated.
        let mut padding = [0_u8; 8];
        let padding = if fill_random(&mut padding).is_ok() {
            padding.to_vec()
        } else {
            Vec::new()
        };
        let router_info = match self.shared.local_router_info.lock() {
            Ok(bytes) => bytes.clone(),
            Err(_) => return false,
        };
        let confirmed_params = ConfirmedParams {
            router_info,
            padding,
            mtu_payload: SSU2_CONFIRMED_MTU_PAYLOAD,
            peer_endpoint: source,
        };
        let (after_created, actions) =
            match after_retry.on_session_created(datagram, confirmed_params, now_ms) {
                Ok(value) => value,
                Err(_) => {
                    // The machine is consumed by the failed call: the
                    // handshake cannot continue without its transcript.
                    self.shared
                        .counters
                        .auth_failures
                        .fetch_add(1, Ordering::Relaxed);
                    state.pending_outbound.insert(key, entry);
                    self.fail_outbound_locked(state, key, None);
                    return true;
                }
            };
        // A SessionCreated that this machine rejects is unrelated
        // traffic: restore and let the inbound path classify it.
        let created_silent = actions
            .iter()
            .all(|action| matches!(action, HandshakeAction::DropSilently(_)));
        if created_silent {
            for action in &actions {
                if let HandshakeAction::DropSilently(category) = action {
                    self.count_drop_category(*category);
                }
            }
            entry.machine = Some(after_created);
            state.pending_outbound.insert(key, entry);
            return false;
        }
        let (established, terminated, confirmed) =
            self.apply_initiator_actions(state, &mut entry, actions, true);
        if let Some(auth) = established {
            state.pending_outbound.insert(key, entry);
            self.promote_outbound_locked(state, key, auth, confirmed);
            return true;
        }
        if let Some(reason) = terminated {
            state.pending_outbound.insert(key, entry);
            self.fail_outbound_locked(state, key, Some(reason));
            return true;
        }
        entry.machine = Some(after_created);
        state.pending_outbound.insert(key, entry);
        true
    }
}

impl Ssu2RuntimeService {
    fn check_token_request_rate(
        &self,
        state: &mut ServiceState,
        source: &SocketAddr,
        now_ms: u64,
    ) -> bool {
        if state.token_request_rates.len() >= SSU2_TOKEN_REQUEST_SOURCES
            && !state.token_request_rates.contains_key(source)
        {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let entry = state
            .token_request_rates
            .entry(*source)
            .or_insert((now_ms, 0));
        if now_ms.saturating_sub(entry.0) >= 1_000 {
            *entry = (now_ms, 1);
            return true;
        }
        if entry.1 >= SSU2_TOKEN_REQUESTS_PER_SECOND {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return false;
        }
        entry.1 = entry.1.saturating_add(1);
        true
    }

    /// Answers one TokenRequest statelessly: no persistent object is
    /// created and no DH is performed.
    fn handle_token_request(
        &self,
        state: &mut ServiceState,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
        local_addr: Option<SocketAddr>,
    ) {
        let request_len = datagram.len();
        if source.port() == 0 {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        if !self.check_token_request_rate(state, &source, now_ms) {
            return;
        }
        // The runtime refuses to answer without a bound socket
        // address: it has no local endpoint to compare an observed
        // address against. The refusal is unchanged by Plan 529 -- only
        // the value the responder used to publish is gone.
        if local_addr.is_none() {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let config = self.responder_config();
        let now32 = now_secs.min(u64::from(u32::MAX)) as u32;
        let Ok(local_conn_id) = random_u32_nonzero().map(u64::from) else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Ok(packet_number) = random_u32_nonzero() else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let mut padding = [0_u8; 8];
        let mut token_bytes = [0_u8; 8];
        if fill_random(&mut padding).is_err() || fill_random(&mut token_bytes).is_err() {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let responder = Responder::new(config, now_ms);
        let (_, actions) = match responder.on_token_request(
            datagram,
            source,
            local_conn_id,
            packet_number,
            now32,
            padding.to_vec(),
            token_bytes,
            &mut state.token_store,
            now_secs,
        ) {
            Ok(value) => value,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        for action in actions {
            match action {
                HandshakeAction::WriteDatagram(bytes) => {
                    let raw = bytes.as_bytes().to_vec();
                    if raw.len() > retry_response_budget(request_len) {
                        self.shared
                            .counters
                            .cheap_drops
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    stage_one_locked(
                        state,
                        &self.shared.config.limits,
                        &self.shared.counters,
                        raw,
                        source,
                    );
                }
                HandshakeAction::DropSilently(category) => self.count_drop_category(category),
                HandshakeAction::Terminate(reason) => self.count_terminate_reason(reason),
                HandshakeAction::ArmDeadline { .. } | HandshakeAction::Established(_) => {}
            }
        }
    }

    /// Handles one SessionRequest for an existing pending entry
    /// (duplicate resend) or statelessly (tokenless Retry) or by
    /// admitting a new bounded pending entry (token-bearing).
    fn handle_session_request(
        &self,
        state: &mut ServiceState,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
        local_addr: Option<SocketAddr>,
    ) {
        if source.port() == 0 {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let mut working = datagram.clone();
        let parts = match parse_session_request(&mut working, &self.shared.local_intro) {
            Ok(parts) => parts,
            Err(_) => {
                self.shared
                    .counters
                    .cheap_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        // Duplicate for a live entry: resend SessionCreated without
        // new admission or DH (handled inside the machine).
        if state
            .pending_inbound
            .contains_key(&parts.header.dst_conn_id())
        {
            let key = parts.header.dst_conn_id();
            self.drive_pending_request(state, key, datagram, source, now_ms, now_secs);
            return;
        }
        if parts.header.token() == 0 {
            self.answer_stateless_retry(state, datagram, source, now_ms, now_secs, local_addr);
            return;
        }
        self.admit_pending_inbound(state, datagram, source, now_ms, now_secs, local_addr);
    }

    /// Feeds a token-bearing SessionRequest to its pending entry.
    fn drive_pending_request(
        &self,
        state: &mut ServiceState,
        key: u64,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
    ) {
        let mut entry = match state.pending_inbound.remove(&key) {
            Some(entry) => entry,
            None => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let Some(machine) = entry.machine.take() else {
            state.pending_inbound.insert(key, entry);
            return;
        };
        if now_ms >= entry.backstop_ms {
            self.shared
                .counters
                .handshake_timeouts
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        // Fresh helper material for the machine call; the duplicate
        // path inside the machine ignores it and resends Created.
        let mut padding = [0_u8; 8];
        let params = (|| {
            if fill_random(&mut padding).is_err() {
                return None;
            }
            Some(ResponderParams {
                local_conn_id: key,
                ephemeral_secret: ephemeral_secret().ok()?,
                packet_number: random_u32_nonzero().ok()?,
                timestamp: now_secs.min(u64::from(u32::MAX)) as u32,
                padding: padding.to_vec(),
            })
        })();
        let mut retry_bytes = [0_u8; 8];
        let Some(params) = params.filter(|_| fill_random(&mut retry_bytes).is_ok()) else {
            entry.machine = Some(machine);
            state.pending_inbound.insert(key, entry);
            self.shared
                .counters
                .auth_failures
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let (store, replay) = (&mut state.token_store, &mut state.replay);
        let (machine, actions) = match machine.on_session_request(
            datagram,
            source,
            params,
            retry_bytes,
            store,
            replay,
            now_ms,
            now_secs,
        ) {
            Ok(value) => value,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let mut wrote = false;
        let mut terminated = None;
        for action in actions {
            match action {
                HandshakeAction::WriteDatagram(bytes) => {
                    wrote = true;
                    stage_one_locked(
                        state,
                        &self.shared.config.limits,
                        &self.shared.counters,
                        bytes.as_bytes().to_vec(),
                        source,
                    );
                }
                HandshakeAction::Terminate(reason) => terminated = Some(reason),
                HandshakeAction::DropSilently(category) => self.count_drop_category(category),
                HandshakeAction::ArmDeadline { kind, at_ms } => {
                    if kind == DeadlineKind::SessionCreated {
                        entry.next_resend_ms = Some(at_ms);
                    }
                }
                HandshakeAction::Established(_) => {}
            }
        }
        if let Some(reason) = terminated {
            self.count_terminate_reason(reason);
            return;
        }
        if wrote {
            // Re-key by the advertised ID when the machine adopted a
            // token-bound one (normally identical to `key`); a live
            // collision fails closed and releases the entry, mirroring
            // the conflicting-handshake path below.
            let advertised = machine.local_conn_id();
            let rekey = if advertised != 0 { advertised } else { key };
            if rekey != key && state.pending_inbound.contains_key(&rekey) {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            entry.machine = Some(machine);
            state.pending_inbound.insert(rekey, entry);
        }
        // A silent drop for a duplicate-shaped datagram releases the
        // entry: conflicting re-handshakes must re-present a token.
    }

    /// Answers a tokenless SessionRequest with a Retry without
    /// creating persistent state or performing DH.
    fn answer_stateless_retry(
        &self,
        state: &mut ServiceState,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
        local_addr: Option<SocketAddr>,
    ) {
        let request_len = datagram.len();
        if !self.check_token_request_rate(state, &source, now_ms) {
            return;
        }
        // The runtime refuses to answer without a bound socket
        // address: it has no local endpoint to compare an observed
        // address against. The refusal is unchanged by Plan 529 -- only
        // the value the responder used to publish is gone.
        if local_addr.is_none() {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let config = self.responder_config();
        let now32 = now_secs.min(u64::from(u32::MAX)) as u32;
        let params = (|| {
            let mut padding = [0_u8; 8];
            let mut retry_bytes = [0_u8; 8];
            if fill_random(&mut padding).is_err() || fill_random(&mut retry_bytes).is_err() {
                return None;
            }
            Some((
                ResponderParams {
                    local_conn_id: random_u32_nonzero().ok().map(u64::from)?,
                    ephemeral_secret: ephemeral_secret().ok()?,
                    packet_number: random_u32_nonzero().ok()?,
                    timestamp: now32,
                    padding: padding.to_vec(),
                },
                retry_bytes,
            ))
        })();
        let Some((params, retry_bytes)) = params else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let responder = Responder::new(config, now_ms);
        let (_, actions) = match responder.on_session_request(
            datagram,
            source,
            params,
            retry_bytes,
            &mut state.token_store,
            &mut state.replay,
            now_ms,
            now_secs,
        ) {
            Ok(value) => value,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        for action in actions {
            match action {
                HandshakeAction::WriteDatagram(bytes) => {
                    let raw = bytes.as_bytes().to_vec();
                    if raw.len() > retry_response_budget(request_len) {
                        self.shared
                            .counters
                            .cheap_drops
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    stage_one_locked(
                        state,
                        &self.shared.config.limits,
                        &self.shared.counters,
                        raw,
                        source,
                    );
                }
                HandshakeAction::DropSilently(category) => self.count_drop_category(category),
                HandshakeAction::Terminate(reason) => self.count_terminate_reason(reason),
                HandshakeAction::ArmDeadline { .. } | HandshakeAction::Established(_) => {}
            }
        }
    }

    /// Admits one token-bearing SessionRequest into a bounded pending
    /// entry after global/per-source admission and manager resource
    /// checks. Only a SessionCreated emission retains the entry.
    fn admit_pending_inbound(
        &self,
        state: &mut ServiceState,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
        local_addr: Option<SocketAddr>,
    ) {
        let limits = self.shared.config.limits;
        let ip = source.ip();
        let subnet = subnet_key(self.shared.config.prefixes, ip);
        if state.pending_total() >= limits.max_pending_handshakes
            || state.pending_ip.get(&ip).copied().unwrap_or(0) >= limits.max_pending_per_ip
            || state.pending_subnet.get(&subnet).copied().unwrap_or(0)
                >= limits.max_pending_per_subnet
        {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let lease = match self
            .shared
            .manager
            .resources()
            .admit(ResourceClass::PendingHandshakes, 1)
        {
            Ok(lease) => lease,
            Err(_) => {
                self.shared
                    .counters
                    .cheap_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        // The runtime refuses to answer without a bound socket
        // address: it has no local endpoint to compare an observed
        // address against. The refusal is unchanged by Plan 529 -- only
        // the value the responder used to publish is gone.
        if local_addr.is_none() {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let config = self.responder_config();
        let now32 = now_secs.min(u64::from(u32::MAX)) as u32;
        let mut local_conn_id = 0_u64;
        for _ in 0..8 {
            let Ok(candidate) = random_u32_nonzero().map(u64::from) else {
                break;
            };
            if !state.pending_inbound.contains_key(&candidate)
                && !state.pending_outbound.contains_key(&candidate)
            {
                local_conn_id = candidate;
                break;
            }
        }
        if local_conn_id == 0 {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let params = (|| {
            let mut padding = [0_u8; 8];
            let mut retry_bytes = [0_u8; 8];
            if fill_random(&mut padding).is_err() || fill_random(&mut retry_bytes).is_err() {
                return None;
            }
            Some((
                ResponderParams {
                    local_conn_id,
                    ephemeral_secret: ephemeral_secret().ok()?,
                    packet_number: random_u32_nonzero().ok()?,
                    timestamp: now32,
                    padding: padding.to_vec(),
                },
                retry_bytes,
            ))
        })();
        let Some((params, retry_bytes)) = params else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let responder = Responder::new(config, now_ms);
        let (store, replay) = (&mut state.token_store, &mut state.replay);
        let (machine, actions) = match responder.on_session_request(
            datagram,
            source,
            params,
            retry_bytes,
            store,
            replay,
            now_ms,
            now_secs,
        ) {
            Ok(value) => value,
            Err(_) => {
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let mut wrote = false;
        let mut next_resend = None;
        let mut terminated = None;
        for action in &actions {
            match action {
                HandshakeAction::WriteDatagram(_) => wrote = true,
                HandshakeAction::ArmDeadline { kind, at_ms } => {
                    if *kind == DeadlineKind::SessionCreated {
                        next_resend = Some(*at_ms);
                    }
                }
                HandshakeAction::Terminate(reason) => terminated = Some(*reason),
                HandshakeAction::DropSilently(category) => self.count_drop_category(*category),
                HandshakeAction::Established(_) => {}
            }
        }
        if let Some(reason) = terminated {
            self.count_terminate_reason(reason);
            return;
        }
        if !wrote {
            // Token/replay/skew rejection: no state retained.
            return;
        }
        // Key the entry by the connection ID the handshake actually
        // advertises (the token-bound Retry ID when the machine adopted
        // one), so the initiator's replies route back to this entry. A
        // live collision fails closed: cross-wired handshakes must
        // re-present a token; no accounting was charged yet.
        let entry_key = {
            let advertised = machine.local_conn_id();
            if advertised != 0 {
                advertised
            } else {
                local_conn_id
            }
        };
        if entry_key != local_conn_id && state.pending_inbound.contains_key(&entry_key) {
            self.shared
                .counters
                .protocol_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        for action in actions {
            if let HandshakeAction::WriteDatagram(bytes) = action {
                stage_one_locked(
                    state,
                    &self.shared.config.limits,
                    &self.shared.counters,
                    bytes.as_bytes().to_vec(),
                    source,
                );
            }
        }
        count_up(&mut state.pending_ip, ip);
        subnet_up(&mut state.pending_subnet, subnet);
        state.pending_inbound.insert(
            entry_key,
            PendingInbound {
                machine: Some(machine),
                source,
                lease: Some(lease),
                backstop_ms: now_ms.saturating_add(
                    self.shared
                        .config
                        .deadlines
                        .handshake
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                ),
                next_resend_ms: next_resend,
            },
        );
    }
}

impl Ssu2RuntimeService {
    /// Offers one datagram to every live inbound handshake awaiting
    /// SessionConfirmed. An empty action vector means a fragment was
    /// accepted (matched); only all-silent machines are skipped.
    fn drive_pending_confirmed(
        &self,
        state: &mut ServiceState,
        datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
    ) -> bool {
        let keys: Vec<u64> = state
            .pending_inbound
            .iter()
            .filter(|(_, entry)| entry.source == source && now_ms < entry.backstop_ms)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            let mut entry = match state.pending_inbound.remove(&key) {
                Some(entry) => entry,
                None => continue,
            };
            let Some(machine) = entry.machine.take() else {
                state.pending_inbound.insert(key, entry);
                continue;
            };
            let (machine, actions) =
                match machine.on_session_confirmed(datagram.clone(), source, now_ms, now_secs) {
                    Ok(value) => value,
                    Err(_) => {
                        self.shared
                            .counters
                            .protocol_drops
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                };
            if actions.is_empty() {
                // Fragment accepted; more fragments outstanding.
                entry.machine = Some(machine);
                state.pending_inbound.insert(key, entry);
                return true;
            }
            if actions
                .iter()
                .all(|action| matches!(action, HandshakeAction::DropSilently(_)))
            {
                for action in &actions {
                    if let HandshakeAction::DropSilently(category) = action {
                        self.count_drop_category(*category);
                    }
                }
                entry.machine = Some(machine);
                state.pending_inbound.insert(key, entry);
                continue;
            }
            let mut established = None;
            let mut terminated = None;
            for action in actions {
                match action {
                    HandshakeAction::Established(auth) => established = Some(auth),
                    HandshakeAction::Terminate(reason) => terminated = Some(reason),
                    HandshakeAction::DropSilently(category) => {
                        self.count_drop_category(category);
                    }
                    HandshakeAction::WriteDatagram(bytes) => {
                        stage_one_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            bytes.as_bytes().to_vec(),
                            source,
                        );
                    }
                    HandshakeAction::ArmDeadline { .. } => {}
                }
            }
            if let Some(auth) = established {
                state.pending_inbound.insert(key, entry);
                self.promote_inbound_locked(state, key, auth, source);
                return true;
            }
            if let Some(reason) = terminated {
                self.count_terminate_reason(reason);
                return true;
            }
            entry.machine = Some(machine);
            state.pending_inbound.insert(key, entry);
            return true;
        }
        false
    }

    /// Handles one datagram for a matched active session: ordered
    /// receive pipeline, event handoff, ACK/retransmit staging, and
    /// poll-driven termination.
    /// Opens one bounded path-validation candidate for authenticated
    /// traffic from a new endpoint and emits exactly one minimum-MTU
    /// PathChallenge datagram toward it.
    ///
    /// Quota, budget, RNG, and staging failures all fail closed without
    /// migration and without unbounded work; the validated path keeps
    /// carrying normal traffic.
    fn open_path_candidate_locked(
        &self,
        state: &mut ServiceState,
        link_id: &LinkId,
        source: SocketAddr,
        endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) {
        let outstanding: usize = state
            .active
            .values()
            .map(|record| record.path.candidate_count())
            .sum();
        if outstanding >= SSU2_MAX_PATH_CANDIDATES_GLOBAL {
            self.shared
                .counters
                .path_denied
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let mut challenge = [0_u8; PATH_CHALLENGE_LENGTH];
        if fill_random(&mut challenge).is_err() || challenge == [0_u8; PATH_CHALLENGE_LENGTH] {
            self.shared
                .counters
                .path_denied
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let event = match state.active.get_mut(link_id) {
            Some(record) => record
                .path
                .note_authenticated_packet(endpoint, challenge, now_ms),
            None => return,
        };
        let challenge = match event {
            Ok(Some(PathEvent::ChallengeToSend { challenge, .. })) => challenge,
            Ok(_) => return,
            Err(_) => {
                self.shared
                    .counters
                    .path_denied
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let queued = match state.active.get_mut(link_id) {
            Some(record) => record
                .session
                .queue_path_challenge(challenge.to_vec())
                .is_ok(),
            None => false,
        };
        if !queued {
            self.shared
                .counters
                .path_denied
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let datagram = match state.active.get_mut(link_id) {
            Some(record) => record.session.poll_transmit(now_ms),
            None => None,
        };
        match datagram {
            Some(bytes) if bytes.len() <= usize::from(constants::SSU2_MIN_MTU) => {
                stage_one_locked(
                    state,
                    &self.shared.config.limits,
                    &self.shared.counters,
                    bytes,
                    source,
                );
                self.shared
                    .counters
                    .path_challenges
                    .fetch_add(1, Ordering::Relaxed);
            }
            Some(_) => {
                self.shared
                    .counters
                    .staging_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
    }

    /// Answers one authenticated PathChallenge with a single
    /// minimum-MTU PathResponse addressed to the challenger.
    ///
    /// The challenge arrived inside an authenticated session packet, so
    /// the response cannot amplify to an unauthenticated victim; it is
    /// still a single small datagram, never a retransmitted series.
    fn answer_path_challenge_locked(
        &self,
        state: &mut ServiceState,
        link_id: &LinkId,
        source: SocketAddr,
        data: Vec<u8>,
        now_ms: u64,
    ) {
        let queued = match state.active.get_mut(link_id) {
            Some(record) => record.session.queue_path_response(data).is_ok(),
            None => return,
        };
        if !queued {
            return;
        }
        let datagram = match state.active.get_mut(link_id) {
            Some(record) => record.session.poll_transmit(now_ms),
            None => None,
        };
        match datagram {
            Some(bytes) if bytes.len() <= usize::from(constants::SSU2_MIN_MTU) => {
                stage_one_locked(
                    state,
                    &self.shared.config.limits,
                    &self.shared.counters,
                    bytes,
                    source,
                );
                self.shared
                    .counters
                    .path_responses
                    .fetch_add(1, Ordering::Relaxed);
            }
            Some(_) => {
                self.shared
                    .counters
                    .staging_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
    }

    /// Handles one authenticated PathResponse from `source`.
    ///
    /// Only an exact challenge match from the tracked candidate
    /// endpoint migrates. Migration moves the endpoint plus per-IP and
    /// per-subnet accounting, resets stale in-flight congestion state,
    /// and records validated-path evidence; the manager link itself is
    /// unchanged.
    fn accept_path_response_locked(
        &self,
        state: &mut ServiceState,
        link_id: &LinkId,
        source: SocketAddr,
        endpoint: Ssu2Endpoint,
        data: Vec<u8>,
        now_ms: u64,
    ) {
        let decision = match state.active.get_mut(link_id) {
            Some(record) => record.path.on_path_response(endpoint, &data, now_ms),
            None => return,
        };
        match decision {
            Ok(PathEvent::Revalidated { endpoint }) => {
                // Plan 549: the path **in use** proved itself again. This is
                // not a migration -- the endpoint is unchanged and nothing
                // moves -- so only the validated-path evidence is recorded.
                // Plan 549: the class is refreshed by a genuine authenticated
                // round trip on the path the router publishes.
                self.record_revalidated_path_locked(state, endpoint);
                return;
            }
            Ok(PathEvent::Validated { .. }) => {}
            Ok(PathEvent::ChallengeToSend { .. }) => return,
            Err(PathError::NotACandidate) => return,
            Err(_) => {
                self.shared
                    .counters
                    .path_rejections
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        let moved_addrs = match state.active.get_mut(link_id) {
            Some(record) => {
                let old_ip = record.peer_addr.ip();
                let old_subnet = record.subnet;
                record.peer_addr = source;
                record.subnet = subnet_key(self.shared.config.prefixes, source.ip());
                record.session.note_path_migrated();
                Some((old_ip, old_subnet, record.subnet))
            }
            None => None,
        };
        if let Some((old_ip, old_subnet, new_subnet)) = moved_addrs {
            count_down(&mut state.active_ip, &old_ip);
            subnet_down(&mut state.active_subnet, &old_subnet);
            count_up(&mut state.active_ip, source.ip());
            subnet_up(&mut state.active_subnet, new_subnet);
        }
        let family = match source.ip() {
            IpAddr::V4(_) => i2pr_transport::AddressFamily::Ipv4,
            IpAddr::V6(_) => i2pr_transport::AddressFamily::Ipv6,
        };
        let now = self.service_now();
        state
            .reachability
            .record(ReachabilitySignal::ValidatedPath { family }, now);
        let observation = state
            .reachability
            .as_transport_observation(TransportKind::Ssu2, now);
        let _ = self.shared.manager.record_reachability(observation);
        self.shared
            .counters
            .path_migrations
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Records one re-validation of the path in use as validated-path
    /// evidence.
    ///
    /// Plan 549: the evidence is filed under the family of the endpoint that
    /// proved itself and **nothing else** — one observation is one fact, so a
    /// re-validation never also records a peer test or an address
    /// observation. The endpoint is unchanged: this is not a migration, so no
    /// per-IP or per-subnet accounting moves and the session's congestion
    /// state is untouched.
    fn record_revalidated_path_locked(&self, state: &mut ServiceState, endpoint: Ssu2Endpoint) {
        let family = match endpoint.ip() {
            IpAddr::V4(_) => i2pr_transport::AddressFamily::Ipv4,
            IpAddr::V6(_) => i2pr_transport::AddressFamily::Ipv6,
        };
        let now = self.service_now();
        state
            .reachability
            .record(ReachabilitySignal::ValidatedPath { family }, now);
        let observation = state
            .reachability
            .as_transport_observation(TransportKind::Ssu2, now);
        let _ = self.shared.manager.record_reachability(observation);
        let total = self
            .shared
            .counters
            .path_revalidations
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        // Plan 549: without this line the probe is counted and never reported,
        // so "the probe ran and was answered" is indistinguishable from "the
        // probe never ran" -- the trap Plan 545 recorded one layer down.
        tracing::info!(
            transition = "path_revalidation_completed",
            path_revalidations = total,
            "the path in use proved itself again"
        );
        self.log_reachability_transition(state);
    }

    /// Reports a reachability transition, counts and labels only.
    ///
    /// A withdrawal used to be observable only as the floodfill role changing,
    /// which cannot distinguish "the path class lapsed" from "the peer test
    /// lapsed" from "a peer contradicted us" -- and those have opposite causes.
    /// The tracker's own state, its supporting-class count and the time left
    /// before its evidence expires are a label and two numbers, so reporting
    /// them adds no identity and no payload.
    fn log_reachability_transition(&self, state: &mut ServiceState) {
        let now = self.service_now();
        let snapshot = state.reachability.snapshot(now);
        let current = (snapshot.state, snapshot.corroboration);
        if state.last_logged_reachability == Some(current) {
            return;
        }
        let previous = state.last_logged_reachability;
        let expires_in_millis = u64::try_from(
            snapshot
                .expires_at
                .saturating_sub(now)
                .as_millis()
                .min(u128::from(u64::MAX)),
        )
        .unwrap_or(u64::MAX);
        state.last_logged_reachability = Some(current);
        tracing::info!(
            transition = "reachability_state_changed",
            from = ?previous.map(|(state, _)| state),
            to = ?snapshot.state,
            supporting_classes = snapshot.corroboration,
            evidence_expires_in_millis = expires_in_millis,
            "the router's own reachability changed, so why it changed is now in the log"
        );
    }

    /// The effective re-validation cadence.
    ///
    /// A zero value is the default, never "as fast as possible".
    fn revalidation_interval_ms(&self) -> u64 {
        match self.shared.config.path_revalidation_interval_ms {
            0 => PATH_REVALIDATION_INTERVAL_MS,
            configured => configured,
        }
    }

    /// Asks at most one session's path in use to prove itself again.
    ///
    /// Plan 549: the probe rides this maintenance pass, so the runtime gains
    /// no task, no timer and no socket. Exactly **one** session is probed per
    /// pass, and only when its cadence is due and it holds no probe already
    /// outstanding, so the work per pass is bounded however many sessions
    /// exist. A staging or queueing failure costs nothing but the cadence: the
    /// path keeps carrying traffic and the next attempt is one interval away.
    fn drive_path_revalidations_locked(&self, state: &mut ServiceState) {
        let now_ms = monotonic_ms(&self.shared.started_at);
        let mut target: Option<(LinkId, SocketAddr, [u8; PATH_CHALLENGE_LENGTH])> = None;
        for (link_id, record) in state.active.iter_mut() {
            if now_ms < record.next_revalidation_ms {
                continue;
            }
            if record.path.revalidation_in_flight(now_ms) {
                continue;
            }
            record.next_revalidation_ms = now_ms.saturating_add(self.revalidation_interval_ms());
            let mut challenge = [0_u8; PATH_CHALLENGE_LENGTH];
            if fill_random(&mut challenge).is_err() || challenge == [0_u8; PATH_CHALLENGE_LENGTH] {
                continue;
            }
            if record.path.request_revalidation(challenge, now_ms).is_err() {
                continue;
            }
            target = Some((*link_id, record.peer_addr, challenge));
            break;
        }
        let Some((link_id, addr, challenge)) = target else {
            return;
        };
        let queued = match state.active.get_mut(&link_id) {
            // Only the driver reaches this block: the candidate path has its
            // own queueing site, and the refusal below is the driver's.
            Some(record) => record
                .session
                .queue_path_challenge(challenge.to_vec())
                .is_ok(),
            None => false,
        };
        if !queued {
            self.shared
                .counters
                .path_denied
                .fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                transition = "path_revalidation_refused",
                "the re-validation could not be queued, so no probe reached the wire"
            );
            return;
        }
        let datagram = match state.active.get_mut(&link_id) {
            Some(record) => record.session.poll_transmit(now_ms),
            None => None,
        };
        match datagram {
            Some(bytes) if bytes.len() <= usize::from(constants::SSU2_MIN_MTU) => {
                stage_one_locked(
                    state,
                    &self.shared.config.limits,
                    &self.shared.counters,
                    bytes,
                    addr,
                );
                self.shared
                    .counters
                    .path_challenges
                    .fetch_add(1, Ordering::Relaxed);
            }
            Some(_) => {
                self.shared
                    .counters
                    .staging_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
    }

    /// Routes one authenticated datagram to its session with path
    /// validation (Plan 159 §2: the source is classified only after
    /// the session authenticated the bytes).
    #[allow(clippy::too_many_arguments)]
    fn handle_active_datagram(
        &self,
        state: &mut ServiceState,
        link_id: LinkId,
        source: SocketAddr,
        bytes: &[u8],
        now_ms: u64,
        now_secs: u64,
        local_endpoint: Option<Ssu2Endpoint>,
        inbound: &mut Vec<Ssu2InboundI2np>,
        relay: &mut Vec<Ssu2InboundRelay>,
        requester: &mut Vec<Ssu2InboundRequesterEvent>,
        target: &mut Vec<Ssu2InboundTarget>,
    ) {
        let mut remove = None;
        let outcome = match state.active.get_mut(&link_id) {
            Some(record) => record.session.receive_datagram(now_ms, now_secs, bytes),
            None => return,
        };
        if let Some(dropped) = outcome.dropped {
            match dropped {
                i2pr_transport_ssu2::DropReason::Malformed
                | i2pr_transport_ssu2::DropReason::AuthenticationFailed
                | i2pr_transport_ssu2::DropReason::BlockParseFailed
                | i2pr_transport_ssu2::DropReason::InvalidAck => {
                    self.shared
                        .counters
                        .auth_failures
                        .fetch_add(1, Ordering::Relaxed);
                }
                i2pr_transport_ssu2::DropReason::Replay
                | i2pr_transport_ssu2::DropReason::TooOld
                | i2pr_transport_ssu2::DropReason::FutureJump => {
                    self.shared
                        .counters
                        .replay_rejections
                        .fetch_add(1, Ordering::Relaxed);
                }
                i2pr_transport_ssu2::DropReason::ConnectionIdMismatch
                | i2pr_transport_ssu2::DropReason::Terminated => {
                    self.shared
                        .counters
                        .protocol_drops
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            return;
        }
        let peer = match state.active.get(&link_id) {
            Some(record) => record.peer,
            None => return,
        };
        // Resolve the source endpoint before any path effect. A zero
        // port can never be a real UDP peer; fail closed without
        // touching session or path state.
        let source_endpoint = match Ssu2Endpoint::from_socket_addr(source) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                self.shared
                    .counters
                    .cheap_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        // Authenticated traffic from a new endpoint opens at most one
        // bounded candidate. The session already authenticated and
        // replay-filtered this datagram; nothing migrates here.
        let needs_candidate = match state.active.get(&link_id) {
            Some(record) => record.peer_addr != source && !record.path.is_known(source_endpoint),
            None => return,
        };
        if needs_candidate {
            self.open_path_candidate_locked(state, &link_id, source, source_endpoint, now_ms);
        }
        if let Some(record) = state.active.get_mut(&link_id) {
            record.data_rx_observed = true;
            record.confirmed_resend = None;
        }
        for event in outcome.events {
            match event {
                SessionEvent::I2npMessage(message) => {
                    self.shared
                        .counters
                        .i2np_received
                        .fetch_add(1, Ordering::Relaxed);
                    if let Some(observer) = &self.shared.qualification_observer
                        && let Some(message_id) = message
                            .get(1..5)
                            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                            .map(u32::from_be_bytes)
                    {
                        observer.record_authenticated_ingress(peer, link_id, message_id);
                    }
                    inbound.push(Ssu2InboundI2np {
                        link_id,
                        peer,
                        bytes: message,
                    });
                }
                SessionEvent::NewToken { expires, token } => {
                    note_cached_token_locked(state, &peer, token, u64::from(expires), now_secs);
                }
                SessionEvent::Termination { .. } => {
                    remove = Some(TerminationCategory::RemoteTermination);
                }
                SessionEvent::PathChallenge(data) => {
                    self.answer_path_challenge_locked(state, &link_id, source, data, now_ms);
                }
                SessionEvent::PathResponse(data) => {
                    self.accept_path_response_locked(
                        state,
                        &link_id,
                        source,
                        source_endpoint,
                        data,
                        now_ms,
                    );
                }
                SessionEvent::AddressObserved { observed } => {
                    self.note_observed_external_address_locked(state, observed);
                }
                SessionEvent::PeerTest(block) => {
                    self.ingest_controlled_session_block(
                        state, &link_id, source, &block, now_secs, now_ms,
                    );
                }
                // Plan 490: the relay blocks are authenticated by the
                // session before they surface here. The runtime never
                // interprets the role; the daemon's role owner owns the
                // disposition.
                SessionEvent::RelayRequest(block) => {
                    relay.push(Ssu2InboundRelay {
                        link_id,
                        peer,
                        event: Ssu2RelayEvent::Request(block),
                    });
                }
                SessionEvent::RelayIntro(block) => {
                    // Plan 532: a Relay Introduction is addressed to THIS
                    // router -- the introducer forwarded it to the peer
                    // its tag named -- so it belongs to the target role
                    // and not to the introducer's. The route exists only
                    // while the operator's target opt-in is on; with the
                    // opt-in off the introduction keeps travelling the
                    // introducer's handoff exactly as before, and the
                    // introducer owner refuses it as not its role.
                    if self.shared.config.relay_target {
                        target.push(Ssu2InboundTarget {
                            link_id,
                            peer,
                            block,
                        });
                    } else {
                        relay.push(Ssu2InboundRelay {
                            link_id,
                            peer,
                            event: Ssu2RelayEvent::Intro(block),
                        });
                    }
                }
                SessionEvent::RelayResponse(block) => {
                    // Plan 531: a Relay Response that correlates with one
                    // of this router's own live relay requests belongs to
                    // the requester role, not to the introducer role. The
                    // correlation is by nonce, exactly as the transport's
                    // own table correlates, and the signature is verified
                    // later, when the requester owner hands the response
                    // to the transport. A response that correlates with no
                    // live request keeps travelling the introducer's
                    // handoff exactly as before.
                    if self
                        .shared
                        .peer_relay
                        .is_requester_nonce(block.nonce(), now_ms)
                    {
                        requester.push(Ssu2InboundRequesterEvent::Response(Ssu2InboundRelay {
                            link_id,
                            peer,
                            event: Ssu2RelayEvent::Response(block),
                        }));
                    } else {
                        relay.push(Ssu2InboundRelay {
                            link_id,
                            peer,
                            event: Ssu2RelayEvent::Response(block),
                        });
                    }
                }
                // Plan 499 (WP2): a peer asked this router for a relay
                // tag. The tag is minted, bound to the session peer and
                // answered on the same session; nothing about the
                // requester is taken from the request itself.
                SessionEvent::RelayTagRequest => {
                    self.answer_relay_tag_request(state, &link_id, &peer, now_secs);
                }
                // Plan 499 (WP2): this router learned a tag from a peer.
                //
                // Plan 524. A `Tag` is the answer to *this router's own*
                // request, so the event is **recorded** here as an
                // authenticated introducer. A tag for a peer this router
                // did **not** ask is refused -- a peer may send the block
                // unsolicited, and recording an introducer nobody asked
                // for would put an unauthenticated peer into the router's
                // introducer set.
                //
                // The event still reaches the relay handoff: the handoff
                // is the runtime's public authenticated-event channel, and
                // Plan 499's live-mesh evidence reads the minted tag
                // there. The **daemon's drain** is what no longer hands it
                // to the introducer role owner, so an answer to the
                // router's own request is no longer counted as one of the
                // role owner's refusals.
                SessionEvent::RelayTag(tag) => {
                    self.shared
                        .counters
                        .relay_tags_observed
                        .fetch_add(1, Ordering::Relaxed);
                    if state.outstanding_tag_requests.remove(&peer) {
                        match self.record_authenticated_introducer(
                            &peer,
                            tag,
                            local_endpoint,
                            now_secs,
                        ) {
                            Ok(()) => {
                                self.shared
                                    .counters
                                    .relay_tags_recorded
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            Err(reason) => {
                                self.shared
                                    .counters
                                    .relay_tags_refused_record
                                    .fetch_add(1, Ordering::Relaxed);
                                tracing::debug!(
                                    transition = "relay_tag_record_refused",
                                    reason = reason.reason(),
                                    "a relay tag could not be recorded as an introducer"
                                );
                            }
                        }
                    } else {
                        self.shared
                            .counters
                            .relay_tags_refused_unrequested
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    relay.push(Ssu2InboundRelay {
                        link_id,
                        peer,
                        event: Ssu2RelayEvent::Tag(tag),
                    });
                }
                _ => {}
            }
        }
        if remove.is_none() {
            for _ in 0..SSU2_MAX_DRAIN_PER_SESSION {
                let datagram = match state.active.get_mut(&link_id) {
                    Some(record) => record.session.poll_transmit_fragments_traced(now_ms),
                    None => None,
                };
                match datagram {
                    Some((datagram, message_ids, fragments)) => {
                        let addr = match state.active.get(&link_id) {
                            Some(record) => record.peer_addr,
                            None => break,
                        };
                        stage_traced_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            datagram,
                            addr,
                            TracedStage {
                                message_ids,
                                fragments,
                                context: TraceContext {
                                    source_link: link_id,
                                    observer: self.shared.qualification_observer.as_ref(),
                                },
                            },
                        );
                    }
                    None => break,
                }
            }
            loop {
                let action = match state.active.get_mut(&link_id) {
                    Some(record) => record.session.poll(now_ms, now_secs).into_iter().next(),
                    None => None,
                };
                match action {
                    Some(SessionAction::Transmit(datagram)) => {
                        let addr = match state.active.get(&link_id) {
                            Some(record) => record.peer_addr,
                            None => break,
                        };
                        stage_one_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            datagram,
                            addr,
                        );
                    }
                    Some(SessionAction::TransmitTracked {
                        datagram,
                        message_ids,
                        fragments,
                    }) => {
                        let addr = match state.active.get(&link_id) {
                            Some(record) => record.peer_addr,
                            None => break,
                        };
                        stage_traced_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            datagram,
                            addr,
                            TracedStage {
                                message_ids,
                                fragments,
                                context: TraceContext {
                                    source_link: link_id,
                                    observer: self.shared.qualification_observer.as_ref(),
                                },
                            },
                        );
                    }
                    Some(SessionAction::Terminate { .. }) => {
                        remove = Some(TerminationCategory::Timeout);
                        break;
                    }
                    None => break,
                }
            }
        }
        if let Some(reason) = remove {
            self.remove_active_locked(state, &link_id);
            let _ = self.shared.manager.close_link(link_id, reason);
        }
    }

    /// Routes one inbound datagram: active session, pending outbound,
    /// pending confirmed, then new-handshake classification. Collects
    /// handoff messages for delivery after the lock releases.
    fn handle_datagram(
        &self,
        bytes: &[u8],
        source: SocketAddr,
        is_v4: bool,
        local_addr: Option<SocketAddr>,
    ) -> Ssu2InboundBatch {
        let mut batch = Ssu2InboundBatch::default();
        let max_len = if is_v4 {
            constants::MAX_DATAGRAM_IPV4_LENGTH
        } else {
            constants::MAX_DATAGRAM_IPV6_LENGTH
        };
        if bytes.len() < constants::MIN_DATAGRAM_LENGTH || bytes.len() > max_len {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return batch;
        }
        let now_ms = self.now_ms();
        let now_secs = wall_secs();
        let Ok(mut state) = self.shared.state.lock() else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return batch;
        };
        // Active sessions first: side-effect-free trial match.
        let matched: Option<LinkId> = state
            .active
            .iter()
            .find(|(_, record)| record.session.matches_inbound(bytes))
            .map(|(id, _)| *id);
        // Plan 524: the router's own bound endpoint, for the introducer
        // record's contact material. It is read from the datagram's own
        // local address, so no lock is taken while the state lock is held.
        let local_endpoint = local_addr.and_then(|addr| Ssu2Endpoint::from_socket_addr(addr).ok());
        if let Some(link_id) = matched {
            self.handle_active_datagram(
                &mut state,
                link_id,
                source,
                bytes,
                now_ms,
                now_secs,
                local_endpoint,
                &mut batch.i2np,
                &mut batch.relay,
                &mut batch.requester,
                &mut batch.target,
            );
            return batch;
        }
        // Endpoint-scoped data fallback: the independent peer never
        // updates its initiator destination ID from SessionCreated, so
        // genuine Alice-to-Bob data arrives with a stale destination ID
        // that strict matching rejects. When exactly one active session
        // serves the source endpoint and the short header decodes under
        // its keys, deliver there; the session AEAD open over the wire
        // header still fails closed on misrouting. Ambiguous (zero or
        // several) candidates fall through to the handshake path.
        let fallback: Option<LinkId> = {
            let mut candidate = None;
            let mut count = 0_usize;
            for (id, record) in state.active.iter() {
                if record.peer_addr == source && record.session.matches_data_header(bytes) {
                    candidate = Some(*id);
                    count += 1;
                }
            }
            if count == 1 { candidate } else { None }
        };
        if let Some(link_id) = fallback {
            self.handle_active_datagram(
                &mut state,
                link_id,
                source,
                bytes,
                now_ms,
                now_secs,
                local_endpoint,
                &mut batch.i2np,
                &mut batch.relay,
                &mut batch.requester,
                &mut batch.target,
            );
            return batch;
        }
        // Pending outbound dials toward this source (single-flight).
        let outbound_key = state
            .pending_outbound
            .iter()
            .find(|(_, entry)| entry.target.address() == source)
            .map(|(key, _)| *key);
        if let Some(key) = outbound_key
            && self.drive_pending_outbound(
                &mut state,
                key,
                bytes.to_vec(),
                source,
                now_ms,
                now_secs,
            )
        {
            return batch;
        }
        // Pending inbound handshakes awaiting SessionConfirmed.
        if self.drive_pending_confirmed(&mut state, bytes.to_vec(), source, now_ms, now_secs) {
            return batch;
        }
        // New-handshake classification with intro-key prevalidation.
        let mut token_probe = bytes.to_vec();
        if parse_token_request(
            &mut token_probe,
            &self.shared.local_intro,
            ClockSkewPolicy::handshake(),
            now_secs,
        )
        .is_ok()
        {
            self.handle_token_request(
                &mut state,
                bytes.to_vec(),
                source,
                now_ms,
                now_secs,
                local_addr,
            );
            return batch;
        }
        let mut request_probe = bytes.to_vec();
        if parse_session_request(&mut request_probe, &self.shared.local_intro).is_ok() {
            self.handle_session_request(
                &mut state,
                bytes.to_vec(),
                source,
                now_ms,
                now_secs,
                local_addr,
            );
            return batch;
        }
        // Plan 531: the requester side of the SSU2 relay protocol. A
        // hole punch travels **out of session**: it is sealed under the
        // receiver's own introduction key and it names neither a session
        // nor a peer hash, so it cannot ride the session-shaped relay
        // handoff and it has its own bounded one. The route exists only
        // while the operator's requester opt-in is on; a datagram that
        // does not parse falls through to the cheap-drop below.
        if self.shared.config.relay_requester {
            if !self.shared.peer_relay.check_admission(source.ip(), now_ms) {
                self.shared
                    .counters
                    .cheap_drops
                    .fetch_add(1, Ordering::Relaxed);
                return batch;
            }
            let mut punch_probe = bytes.to_vec();
            if let Ok(message) = parse_hole_punch(&mut punch_probe, &self.shared.local_intro) {
                batch
                    .requester
                    .push(Ssu2InboundRequesterEvent::HolePunch(Ssu2InboundHolePunch {
                        source,
                        message,
                    }));
                return batch;
            }
        }
        // Controlled peer-test out-of-session ingress (Plan 283): only
        // when permitted. Parse-only with the local intro key; wrong
        // keys and garbage fall through to the cheap-drop below, and a
        // parsed block is routed without ever emitting a response.
        if self.shared.config.controlled_peer_test {
            if !self.shared.peer_relay.check_admission(source.ip(), now_ms) {
                self.shared
                    .counters
                    .cheap_drops
                    .fetch_add(1, Ordering::Relaxed);
                return batch;
            }
            let mut peer_probe = bytes.to_vec();
            if let Ok((_, block)) =
                parse_out_of_session_peer_test(&mut peer_probe, &self.shared.local_intro)
            {
                self.ingest_controlled_out_of_session(&mut state, source, &block, now_secs, now_ms);
                return batch;
            }
        }
        self.shared
            .counters
            .cheap_drops
            .fetch_add(1, Ordering::Relaxed);
        batch
    }
}

impl Ssu2RuntimeService {
    /// Rebuilds a cached-token dial as tokenless after the grace
    /// period without Created progress.
    fn fallback_tokenless_locked(&self, state: &mut ServiceState, key: u64, now_ms: u64) {
        let mut entry = match state.pending_outbound.remove(&key) {
            Some(entry) => entry,
            None => return,
        };
        if !matches!(entry.phase, DialPhase::CachedToken { .. }) {
            state.pending_outbound.insert(key, entry);
            return;
        }
        let now_secs = wall_secs();
        let static_secret = X25519PrivateKey::from_bytes(self.shared.local_static);
        let rebuilt = (|| {
            let ephemeral = ephemeral_secret().ok()?;
            let (_, remote_conn_id) = random_conn_ids().ok()?;
            let packet_number = random_u32_nonzero().ok()?;
            let mut padding = [0_u8; 16];
            fill_random(&mut padding).ok()?;
            let config = InitiatorConfig {
                responder_static: entry.target.responder_static(),
                responder_intro: entry.target.responder_intro(),
                expected_router_hash: entry.target.expected_router_hash(),
                clock: ClockSkewPolicy::handshake(),
                local_mtu: self.shared.local_mtu,
                // Plan 529: the same observation as the direct dial
                // path, built from the same dial target.
                remote_address: Ssu2Endpoint::from_socket_addr(entry.target.address()).ok(),
            };
            // The map key (local connection ID) is stable across the
            // fallback so in-flight responses still route.
            let local_conn_id = key;
            let secrets = InitiatorSecrets {
                static_secret,
                ephemeral_secret: ephemeral,
                local_conn_id,
                remote_conn_id,
                packet_number,
                timestamp: now_secs.min(u64::from(u32::MAX)) as u32,
            };
            Initiator::begin(config, secrets, None, padding.to_vec(), now_ms).ok()
        })();
        let Some((machine, actions)) = rebuilt else {
            state.pending_outbound.insert(key, entry);
            self.shared
                .counters
                .auth_failures
                .fetch_add(1, Ordering::Relaxed);
            self.fail_outbound_locked(state, key, None);
            return;
        };
        entry.machine = Some(machine);
        entry.phase = DialPhase::Tokenless;
        entry.token_used = None;
        entry.next_resend_ms = u64::MAX;
        for action in &actions {
            if let HandshakeAction::ArmDeadline { kind, at_ms } = action
                && *kind != DeadlineKind::Handshake
            {
                entry.next_resend_ms = entry.next_resend_ms.min(*at_ms);
            }
        }
        // Stage before re-inserting so a full staging bound denies the
        // fallback without losing the pending dial.
        let staged: Vec<Vec<u8>> = actions
            .iter()
            .filter_map(|action| match action {
                HandshakeAction::WriteDatagram(bytes) => Some(bytes.as_bytes().to_vec()),
                _ => None,
            })
            .collect();
        let addr = entry.target.address();
        let mut ok = true;
        for raw in staged {
            if !stage_one_locked(
                state,
                &self.shared.config.limits,
                &self.shared.counters,
                raw,
                addr,
            ) {
                ok = false;
                break;
            }
        }
        if !ok {
            entry.machine = None;
            state.pending_outbound.insert(key, entry);
            self.fail_outbound_locked(state, key, None);
            return;
        }
        state.pending_outbound.insert(key, entry);
    }

    /// Drives one outbound retransmit deadline.
    fn drive_outbound_timeout(&self, state: &mut ServiceState, key: u64, now_ms: u64) {
        let mut entry = match state.pending_outbound.remove(&key) {
            Some(entry) => entry,
            None => return,
        };
        let Some(machine) = entry.machine.take() else {
            state.pending_outbound.insert(key, entry);
            return;
        };
        if now_ms >= entry.backstop_ms {
            state.pending_outbound.insert(key, entry);
            self.shared
                .counters
                .handshake_timeouts
                .fetch_add(1, Ordering::Relaxed);
            self.fail_outbound_locked(state, key, None);
            return;
        }
        let (machine, actions) = match machine.on_timeout(now_ms) {
            Ok(value) => value,
            Err(_) => {
                state.pending_outbound.insert(key, entry);
                self.shared
                    .counters
                    .protocol_drops
                    .fetch_add(1, Ordering::Relaxed);
                self.fail_outbound_locked(state, key, None);
                return;
            }
        };
        let (established, terminated, _) =
            self.apply_initiator_actions(state, &mut entry, actions, false);
        if let Some(auth) = established {
            state.pending_outbound.insert(key, entry);
            self.promote_outbound_locked(state, key, auth, Vec::new());
            return;
        }
        if let Some(reason) = terminated {
            state.pending_outbound.insert(key, entry);
            self.fail_outbound_locked(state, key, Some(reason));
            return;
        }
        entry.machine = Some(machine);
        state.pending_outbound.insert(key, entry);
    }

    /// Plan 535: re-asserts the router's own bound-address fact.
    ///
    /// The explicit bind is a **standing** fact -- the socket is bound and
    /// stays bound -- but the reachability model holds it as a timed
    /// observation, so the class lapses unless the router re-asserts it. The
    /// two peer-sourced classes are refreshed by their peers; this one is
    /// the router's own, so the router refreshes it.
    ///
    /// The lifetime comes from the **tracker's own policy**, so the expiry
    /// rule keeps exactly one owner, and the re-assertion rides the
    /// runtime's **existing** maintenance pass, so no new timer exists.
    /// The families come from the shared state, captured at bind time, so
    /// the sockets lock is never taken while the state lock is held.
    fn refresh_explicit_bind_locked(&self, state: &mut ServiceState) {
        if !self.shared.config.explicit_bind_corroboration {
            return;
        }
        if state.explicit_bind_families.is_empty() {
            return;
        }
        let now = self.service_now();
        let ttl = state.reachability.policy().observation_ttl();
        if !explicit_bind_refresh_due(state.explicit_bind_recorded_at, now, ttl) {
            return;
        }
        let families: Vec<AddressFamily> = state.explicit_bind_families.iter().copied().collect();
        for family in families {
            state
                .reachability
                .record(ReachabilitySignal::LocalConfiguredBind { family }, now);
            self.shared
                .counters
                .explicit_bind_refreshes
                .fetch_add(1, Ordering::Relaxed);
        }
        state.explicit_bind_recorded_at = Some(now);
    }

    /// Drives all time-based work under one lock: handshake resends
    /// and backstops, cached-token fallback, session polls, Confirmed
    /// resends, and token-table expiry. Returns handoff messages for
    /// delivery after the lock releases.
    fn drive_timeouts_locked(&self, state: &mut ServiceState) -> Ssu2InboundBatch {
        // Plan 535: the standing explicit-bind fact is re-asserted on the
        // runtime's existing maintenance pass.
        self.refresh_explicit_bind_locked(state);
        // Plan 549: the path in use is re-proved on the same pass, so a
        // healthy session keeps its validated-path class instead of decaying
        // to the corroboration floor when the observation TTL lapses.
        self.drive_path_revalidations_locked(state);
        // Plan 549: whatever moved the state, the transition is reported here
        // as well, so a lapse no probe caused is still explained.
        self.log_reachability_transition(state);
        let batch = Ssu2InboundBatch::default();
        let now_ms = monotonic_ms(&self.shared.started_at);
        let now_secs = wall_secs();
        let expired_receipts = state
            .pending_write_receipts
            .iter()
            .filter(|(_, receipt)| now_ms >= receipt.expires_ms)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        for key in expired_receipts {
            if let Some(receipt) = state.pending_write_receipts.remove(&key) {
                let _ = receipt.completion.send(false);
            }
        }
        // Pending outbound: backstop, cached-token grace, resends.
        let outbound_keys: Vec<u64> = state.pending_outbound.keys().copied().collect();
        for key in outbound_keys {
            let (backstop, grace_due, resend_due) = match state.pending_outbound.get(&key) {
                Some(entry) => (
                    entry.backstop_ms,
                    match entry.phase {
                        DialPhase::CachedToken { grace_at_ms } => now_ms >= grace_at_ms,
                        DialPhase::Tokenless => false,
                    },
                    now_ms >= entry.next_resend_ms,
                ),
                None => continue,
            };
            if now_ms >= backstop {
                self.shared
                    .counters
                    .handshake_timeouts
                    .fetch_add(1, Ordering::Relaxed);
                self.fail_outbound_locked(state, key, None);
            } else if grace_due {
                self.fallback_tokenless_locked(state, key, now_ms);
            } else if resend_due {
                self.drive_outbound_timeout(state, key, now_ms);
            }
        }
        // Pending inbound: backstop and Created resends.
        let inbound_keys: Vec<u64> = state.pending_inbound.keys().copied().collect();
        for key in inbound_keys {
            let (backstop, resend_due) = match state.pending_inbound.get(&key) {
                Some(entry) => (
                    entry.backstop_ms,
                    entry.next_resend_ms.is_some_and(|at| now_ms >= at),
                ),
                None => continue,
            };
            if now_ms >= backstop {
                state.pending_inbound.remove(&key);
                self.shared
                    .counters
                    .handshake_timeouts
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if !resend_due {
                continue;
            }
            let mut entry = match state.pending_inbound.remove(&key) {
                Some(entry) => entry,
                None => continue,
            };
            let Some(machine) = entry.machine.take() else {
                state.pending_inbound.insert(key, entry);
                continue;
            };
            let (machine, actions) = match machine.on_timeout(now_ms) {
                Ok(value) => value,
                Err(_) => {
                    self.shared
                        .counters
                        .protocol_drops
                        .fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            };
            let mut terminated = None;
            for action in actions {
                match action {
                    HandshakeAction::WriteDatagram(bytes) => {
                        stage_one_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            bytes.as_bytes().to_vec(),
                            entry.source,
                        );
                    }
                    HandshakeAction::ArmDeadline { kind, at_ms } => {
                        if kind == DeadlineKind::SessionCreated {
                            entry.next_resend_ms = Some(at_ms);
                        }
                    }
                    HandshakeAction::Terminate(reason) => terminated = Some(reason),
                    HandshakeAction::DropSilently(category) => {
                        self.count_drop_category(category);
                    }
                    HandshakeAction::Established(_) => {}
                }
            }
            if let Some(reason) = terminated {
                self.count_terminate_reason(reason);
                continue;
            }
            entry.machine = Some(machine);
            state.pending_inbound.insert(key, entry);
        }
        // Active sessions: poll deadlines, Confirmed resends.
        let active_ids: Vec<LinkId> = state.active.keys().copied().collect();
        for link_id in active_ids {
            let mut remove = None;
            // Post-promotion Confirmed resends until data-phase
            // liveness or budget end.
            let resend_due = match state.active.get(&link_id) {
                Some(record) => {
                    !record.data_rx_observed
                        && record
                            .confirmed_resend
                            .as_ref()
                            .is_some_and(|resend| now_ms >= resend.next_at_ms)
                }
                None => false,
            };
            if resend_due {
                let datagrams = match state.active.get_mut(&link_id) {
                    Some(record) => match record.confirmed_resend.as_mut() {
                        Some(resend) if resend.attempts >= constants::MAX_HANDSHAKE_ATTEMPTS => {
                            record.confirmed_resend = None;
                            Vec::new()
                        }
                        Some(resend) => {
                            let delay = constants::SESSION_CONFIRMED_RESEND_DELAYS_MS[(resend
                                .attempts
                                as usize)
                                .min(constants::SESSION_CONFIRMED_RESEND_DELAYS_MS.len() - 1)];
                            resend.attempts = resend.attempts.saturating_add(1);
                            resend.next_at_ms = now_ms.saturating_add(delay);
                            resend.datagrams.clone()
                        }
                        None => Vec::new(),
                    },
                    None => Vec::new(),
                };
                let addr = match state.active.get(&link_id) {
                    Some(record) => record.peer_addr,
                    None => continue,
                };
                for datagram in datagrams {
                    stage_one_locked(
                        state,
                        &self.shared.config.limits,
                        &self.shared.counters,
                        datagram,
                        addr,
                    );
                }
            }
            loop {
                let action = match state.active.get_mut(&link_id) {
                    Some(record) => record.session.poll(now_ms, now_secs).into_iter().next(),
                    None => None,
                };
                match action {
                    Some(SessionAction::Transmit(datagram)) => {
                        let addr = match state.active.get(&link_id) {
                            Some(record) => record.peer_addr,
                            None => break,
                        };
                        stage_one_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            datagram,
                            addr,
                        );
                    }
                    Some(SessionAction::TransmitTracked {
                        datagram,
                        message_ids,
                        fragments,
                    }) => {
                        let addr = match state.active.get(&link_id) {
                            Some(record) => record.peer_addr,
                            None => break,
                        };
                        stage_traced_locked(
                            state,
                            &self.shared.config.limits,
                            &self.shared.counters,
                            datagram,
                            addr,
                            TracedStage {
                                message_ids,
                                fragments,
                                context: TraceContext {
                                    source_link: link_id,
                                    observer: self.shared.qualification_observer.as_ref(),
                                },
                            },
                        );
                    }
                    Some(SessionAction::Terminate { .. }) => {
                        remove = Some(TerminationCategory::Timeout);
                        break;
                    }
                    None => break,
                }
            }
            // Opportunistic transmit drain for sessions with queued
            // user data (notified by send paths).
            if remove.is_none() {
                for _ in 0..SSU2_MAX_DRAIN_PER_SESSION {
                    let datagram = match state.active.get_mut(&link_id) {
                        Some(record) => record.session.poll_transmit_fragments_traced(now_ms),
                        None => None,
                    };
                    match datagram {
                        Some((datagram, message_ids, fragments)) => {
                            let addr = match state.active.get(&link_id) {
                                Some(record) => record.peer_addr,
                                None => break,
                            };
                            stage_traced_locked(
                                state,
                                &self.shared.config.limits,
                                &self.shared.counters,
                                datagram,
                                addr,
                                TracedStage {
                                    message_ids,
                                    fragments,
                                    context: TraceContext {
                                        source_link: link_id,
                                        observer: self.shared.qualification_observer.as_ref(),
                                    },
                                },
                            );
                        }
                        None => break,
                    }
                }
            }
            // Expire path-validation candidates; expiry retains the old
            // validated path, never migrates.
            let expired = match state.active.get_mut(&link_id) {
                Some(record) => record.path.poll_expired(now_ms),
                None => Vec::new(),
            };
            if !expired.is_empty() {
                self.shared
                    .counters
                    .path_expirations
                    .fetch_add(expired.len() as u64, Ordering::Relaxed);
            }
            if let Some(reason) = remove {
                self.remove_active_locked(state, &link_id);
                let _ = self.shared.manager.close_link(link_id, reason);
            }
        }
        state.token_store.expire(now_secs);
        let _ = now_ms;
        batch
    }

    /// Computes how long the loop may sleep before the earliest armed
    /// handshake/ACK/RTO/idle deadline, capped by the poll bound.
    fn wake_in(&self) -> Duration {
        let now_ms = self.now_ms();
        let mut earliest = now_ms.saturating_add(
            self.shared
                .config
                .deadlines
                .scheduler_poll_max
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
        if let Ok(state) = self.shared.state.lock() {
            for entry in state.pending_outbound.values() {
                earliest = earliest.min(entry.next_resend_ms).min(entry.backstop_ms);
                if let DialPhase::CachedToken { grace_at_ms } = entry.phase {
                    earliest = earliest.min(grace_at_ms);
                }
            }
            for entry in state.pending_inbound.values() {
                earliest = earliest.min(entry.backstop_ms);
                if let Some(at) = entry.next_resend_ms {
                    earliest = earliest.min(at);
                }
            }
            for record in state.active.values() {
                if let Some(at) = record.session.next_deadline_ms(now_ms) {
                    earliest = earliest.min(at);
                }
                if let Some(at) = record.path.next_deadline_ms() {
                    earliest = earliest.min(at);
                }
                // Plan 549: the next re-validation of the path in use is a
                // scheduler deadline like any other. Without it an idle
                // session would sleep past its own cadence, and the probe
                // would only ever fire when other traffic happened to wake
                // the loop -- which is exactly the state this plan exists to
                // stop depending on.
                let revalidation = if record.path.revalidation_in_flight(now_ms) {
                    record.path.next_deadline_ms()
                } else {
                    Some(record.next_revalidation_ms)
                };
                if let Some(at) = revalidation {
                    earliest = earliest.min(at);
                }
                if !record.data_rx_observed
                    && let Some(resend) = record.confirmed_resend.as_ref()
                {
                    earliest = earliest.min(resend.next_at_ms);
                }
            }
        }
        Duration::from_millis(earliest.saturating_sub(now_ms))
    }
}

impl Ssu2RuntimeService {
    /// Pops staged datagrams for one address family, applies the
    /// test-only fault policy, and transmits them. Fault counters and
    /// send counters are the only observables; payloads are never logged.
    ///
    /// The one-shot reorder holds the next datagram across the flush
    /// boundary: it waits in `fault_held` until a successor transmits
    /// (or the next flush finds an empty queue, which releases it with
    /// only a scheduling delay).
    async fn flush_staged(
        &self,
        socket: &UdpSocket,
        is_v4: bool,
        cancellation: &CancellationToken,
    ) {
        struct Send {
            bytes: Vec<u8>,
            addr: SocketAddr,
            class: BandwidthTrafficClass,
            duplicates: bool,
            message_ids: Vec<u32>,
            fragments: Vec<i2pr_transport_ssu2::session::I2npFragmentReceipt>,
            source: Option<(PeerId, LinkId)>,
        }
        let policy = self
            .shared
            .faults
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let (sends, previous_held): (Vec<Send>, Option<StagedDatagram>) = {
            let Ok(mut state) = self.shared.state.lock() else {
                return;
            };
            let mut popped = Vec::new();
            if is_v4 {
                while let Some(datagram) = state.staged_v4.pop_front() {
                    popped.push(datagram);
                }
            } else {
                while let Some(datagram) = state.staged_v6.pop_front() {
                    popped.push(datagram);
                }
            }
            let drained: usize = popped.iter().map(|item| item.bytes.len()).sum();
            state.staged_bytes = state.staged_bytes.saturating_sub(drained);
            let mut sends = Vec::with_capacity(popped.len());
            for datagram in popped {
                let index = state.fault_transmits;
                state.fault_transmits = state.fault_transmits.saturating_add(1);
                if policy
                    .as_ref()
                    .is_some_and(|policy| policy.drop_transmit.contains(&index))
                {
                    self.shared
                        .counters
                        .fault_drops
                        .fetch_add(1, Ordering::Relaxed);
                    if !datagram.trace.message_ids.is_empty()
                        && let (Some(observer), Some((peer, link_id))) =
                            (&self.shared.qualification_observer, datagram.trace.source)
                    {
                        observer.record(
                            peer,
                            link_id,
                            crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWriteFailed,
                            &datagram.trace.message_ids,
                        );
                    }
                    if let Some((_, link_id)) = datagram.trace.source {
                        settle_ssu2_write_receipts(
                            &mut state,
                            link_id,
                            &datagram.trace.fragments,
                            false,
                        );
                    }
                    continue;
                }
                let duplicates = policy
                    .as_ref()
                    .is_some_and(|policy| policy.duplicate_transmit.contains(&index));
                sends.push(Send {
                    bytes: datagram.bytes,
                    addr: datagram.addr,
                    class: datagram.class,
                    duplicates,
                    message_ids: datagram.trace.message_ids,
                    fragments: datagram.trace.fragments,
                    source: datagram.trace.source,
                });
            }
            let previous_held = state.fault_held.take();
            let swap_armed = policy.as_ref().is_some_and(|policy| policy.swap_next_two);
            if swap_armed && previous_held.is_none() && !sends.is_empty() {
                let first = sends.remove(0);
                if sends.is_empty() {
                    // Lone datagram: hold for the next flush so the
                    // reorder still delays exactly one position.
                    state.fault_held = Some(StagedDatagram {
                        bytes: first.bytes,
                        addr: first.addr,
                        class: first.class,
                        trace: TracedI2np {
                            message_ids: first.message_ids,
                            fragments: first.fragments,
                            source: first.source,
                        },
                    });
                } else {
                    // Batched datagrams: the first joins at the tail.
                    sends.push(Send {
                        bytes: first.bytes,
                        addr: first.addr,
                        class: first.class,
                        duplicates: false,
                        message_ids: first.message_ids,
                        fragments: first.fragments,
                        source: first.source,
                    });
                    if let Ok(mut guard) = self.shared.faults.lock()
                        && let Some(policy) = guard.as_mut()
                    {
                        policy.swap_next_two = false;
                    }
                }
            }
            (sends, previous_held)
        };
        for send in sends {
            let times = if send.duplicates { 2 } else { 1 };
            for _ in 0..times {
                if let Some(governor) = &self.shared.bandwidth
                    && governor
                        .acquire_class(
                            BandwidthDirection::Outbound,
                            bandwidth_peer_key(send.addr),
                            send.bytes.len(),
                            send.class,
                            cancellation,
                        )
                        .await
                        .is_err()
                {
                    if !send.message_ids.is_empty() {
                        if let (Some(observer), Some((peer, link_id))) =
                            (&self.shared.qualification_observer, send.source)
                        {
                            observer.record(
                                peer,
                                link_id,
                                crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWriteFailed,
                                &send.message_ids,
                            );
                        }
                        tracing::warn!(
                            i2np_message_count = send.message_ids.len(),
                            transition = "udp_admission_rejected",
                            "SSU2 message send receipt"
                        );
                    }
                    if let Some((_, link_id)) = send.source
                        && let Ok(mut state) = self.shared.state.lock()
                    {
                        settle_ssu2_write_receipts(&mut state, link_id, &send.fragments, false);
                    }
                    continue;
                }
                if socket.send_to(&send.bytes, send.addr).await.is_ok() {
                    if let Some((_, link_id)) = send.source
                        && let Ok(mut state) = self.shared.state.lock()
                    {
                        settle_ssu2_write_receipts(&mut state, link_id, &send.fragments, true);
                    }
                    if !send.message_ids.is_empty()
                        && let (Some(observer), Some((peer, link_id))) =
                            (&self.shared.qualification_observer, send.source)
                    {
                        observer.record(
                            peer,
                            link_id,
                            crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWritten,
                            &send.message_ids,
                        );
                    }
                    if !send.message_ids.is_empty() {
                        // Ephemeral per-datagram I2NP IDs (never peer
                        // identities) so lane harnesses can correlate
                        // socket emission with stock-side per-message
                        // counters. One datagram carries at most a
                        // handful of messages; the recorded prefix is
                        // bounded regardless. Fragment ordinals
                        // (`id:number/total`) distinguish packed
                        // multi-message datagrams from retransmits.
                        let mut emitted_ids: Vec<u32> = Vec::new();
                        for id in send.message_ids.iter().take(8) {
                            emitted_ids.push(*id);
                        }
                        let mut emitted_fragments: Vec<u64> = Vec::new();
                        for fragment in send.fragments.iter().take(8) {
                            // Packed `message_id:frag/total` ledger:
                            // high 32 bits carry the message id, then
                            // fragment ordinal, then total count.
                            emitted_fragments.push(
                                (u64::from(fragment.message_id) << 32)
                                    | (u64::from(fragment.fragment_number) << 8)
                                    | u64::from(fragment.total_fragments),
                            );
                        }
                        tracing::info!(
                            i2np_message_count = send.message_ids.len(),
                            i2np_message_ids = ?emitted_ids,
                            i2np_fragments = ?emitted_fragments,
                            transition = "udp_emitted",
                            "SSU2 message send receipt"
                        );
                    }
                    self.shared
                        .counters
                        .datagrams_sent
                        .fetch_add(1, Ordering::Relaxed);
                } else {
                    if let Some((_, link_id)) = send.source
                        && let Ok(mut state) = self.shared.state.lock()
                    {
                        settle_ssu2_write_receipts(&mut state, link_id, &send.fragments, false);
                    }
                    if !send.message_ids.is_empty()
                        && let (Some(observer), Some((peer, link_id))) =
                            (&self.shared.qualification_observer, send.source)
                    {
                        observer.record(
                            peer,
                            link_id,
                            crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWriteFailed,
                            &send.message_ids,
                        );
                    }
                    if !send.message_ids.is_empty() {
                        tracing::warn!(
                            i2np_message_count = send.message_ids.len(),
                            transition = "udp_send_failed",
                            "SSU2 message send receipt"
                        );
                    }
                }
            }
        }
        if let Some(previous) = previous_held {
            // Released one position behind its successor (or merely
            // delayed when the queue was empty).
            let admitted = if let Some(governor) = &self.shared.bandwidth {
                governor
                    .acquire_class(
                        BandwidthDirection::Outbound,
                        bandwidth_peer_key(previous.addr),
                        previous.bytes.len(),
                        previous.class,
                        cancellation,
                    )
                    .await
                    .is_ok()
            } else {
                true
            };
            if !admitted && !previous.trace.message_ids.is_empty() {
                if let (Some(observer), Some((peer, link_id))) =
                    (&self.shared.qualification_observer, previous.trace.source)
                {
                    observer.record(
                        peer,
                        link_id,
                        crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWriteFailed,
                        &previous.trace.message_ids,
                    );
                }
                tracing::warn!(
                    i2np_message_count = previous.trace.message_ids.len(),
                    transition = "udp_admission_rejected",
                    "SSU2 message send receipt"
                );
            }
            if !admitted
                && let Some((_, link_id)) = previous.trace.source
                && let Ok(mut state) = self.shared.state.lock()
            {
                settle_ssu2_write_receipts(&mut state, link_id, &previous.trace.fragments, false);
            }
            if admitted && socket.send_to(&previous.bytes, previous.addr).await.is_ok() {
                if let Some((_, link_id)) = previous.trace.source
                    && let Ok(mut state) = self.shared.state.lock()
                {
                    settle_ssu2_write_receipts(
                        &mut state,
                        link_id,
                        &previous.trace.fragments,
                        true,
                    );
                }
                if !previous.trace.message_ids.is_empty()
                    && let (Some(observer), Some((peer, link_id))) =
                        (&self.shared.qualification_observer, previous.trace.source)
                {
                    observer.record(
                        peer,
                        link_id,
                        crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWritten,
                        &previous.trace.message_ids,
                    );
                }
                if !previous.trace.message_ids.is_empty() {
                    tracing::info!(
                        i2np_message_count = previous.trace.message_ids.len(),
                        transition = "udp_emitted",
                        "SSU2 message send receipt"
                    );
                }
                self.shared
                    .counters
                    .datagrams_sent
                    .fetch_add(1, Ordering::Relaxed);
            } else if admitted && !previous.trace.message_ids.is_empty() {
                if let Some((_, link_id)) = previous.trace.source
                    && let Ok(mut state) = self.shared.state.lock()
                {
                    settle_ssu2_write_receipts(
                        &mut state,
                        link_id,
                        &previous.trace.fragments,
                        false,
                    );
                }
                if let (Some(observer), Some((peer, link_id))) =
                    (&self.shared.qualification_observer, previous.trace.source)
                {
                    observer.record(
                        peer,
                        link_id,
                        crate::ssu2_qualification_observer::Ssu2QualificationEventKind::I2npDatagramWriteFailed,
                        &previous.trace.message_ids,
                    );
                }
                tracing::warn!(
                    i2np_message_count = previous.trace.message_ids.len(),
                    transition = "udp_send_failed",
                    "SSU2 message send receipt"
                );
            }
            if let Ok(mut guard) = self.shared.faults.lock()
                && let Some(policy) = guard.as_mut()
            {
                policy.swap_next_two = false;
            }
        }
    }

    /// Releases every handshake, session, and manager link exactly
    /// once for shutdown teardown and baseline return.
    fn cleanup_locked(&self, state: &mut ServiceState) {
        for (_, entry) in state.pending_outbound.drain() {
            if let Some(tx) = entry.completion {
                let _ = tx.send(DialCompletion::Failed);
            }
        }
        state.pending_outbound_addrs.clear();
        state.pending_inbound.clear();
        state.pending_ip.clear();
        state.pending_subnet.clear();
        let link_ids: Vec<LinkId> = state.active.keys().copied().collect();
        for link_id in link_ids {
            self.remove_active_locked(state, &link_id);
            let _ = self
                .shared
                .manager
                .close_link(link_id, TerminationCategory::IoClosure);
        }
        state.peer_links.clear();
        state.staged_v4.clear();
        state.staged_v6.clear();
        state.staged_bytes = 0;
        state.fault_held = None;
        state.token_request_rates.clear();
        state.token_cache.clear();
        state.token_store.rotate();
    }

    /// One supervised loop task per bound socket family: receive
    /// classification plus central-scheduler duties over shared state.
    #[allow(clippy::too_many_arguments)]
    async fn run_loop(
        &self,
        child: CancellationToken,
        socket: Arc<UdpSocket>,
        inbound_tx: mpsc::Sender<Ssu2InboundI2np>,
        relay_tx: mpsc::Sender<Ssu2InboundRelay>,
        requester_tx: mpsc::Sender<Ssu2InboundRequesterEvent>,
        target_tx: mpsc::Sender<Ssu2InboundTarget>,
        is_v4: bool,
    ) -> Result<(), ChildTaskFailure> {
        let mut buffer = vec![0_u8; 2048];
        let local_addr = socket.local_addr().ok();
        loop {
            let sleep_for = self.wake_in();
            tokio::select! {
                biased;
                _ = child.cancelled() => {
                    if let Ok(mut state) = self.shared.state.lock() {
                        self.cleanup_locked(&mut state);
                    }
                    return Ok(());
                }
                _ = self.shared.shutdown.cancelled() => {
                    if let Ok(mut state) = self.shared.state.lock() {
                        self.cleanup_locked(&mut state);
                    }
                    return Ok(());
                }
                _ = self.shared.notify.notified() => {
                    let inbound = self.shared.state.lock().map(|mut state| {
                        self.drive_timeouts_locked(&mut state)
                    }).unwrap_or_default();
                    self.deliver_inbound(
                        inbound,
                        &inbound_tx,
                        &relay_tx,
                        &requester_tx,
                        &target_tx,
                    );
                    self.flush_staged(&socket, is_v4, &child).await;
                }
                result = socket.recv_from(&mut buffer) => {
                    match result {
                        Ok((len, source)) => {
                            self.shared.counters.datagrams_received.fetch_add(1, Ordering::Relaxed);
                            let admitted = if let Some(governor) = &self.shared.bandwidth {
                                governor
                                    .acquire(
                                        BandwidthDirection::Inbound,
                                        bandwidth_peer_key(source),
                                        len,
                                        &child,
                                    )
                                    .await
                                    .is_ok()
                            } else {
                                true
                            };
                            if !admitted {
                                continue;
                            }
                            let inbound = self.handle_datagram(&buffer[..len], source, is_v4, local_addr);
                            self.deliver_inbound(
                                inbound,
                                &inbound_tx,
                                &relay_tx,
                                &requester_tx,
                                &target_tx,
                            );
                            self.flush_staged(&socket, is_v4, &child).await;
                        }
                        Err(_) => {
                            // Local socket failure, not peer traffic:
                            // back off instead of busy-spinning. Loss
                            // recovery owns retransmission.
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }
                }
                _ = tokio::time::sleep(sleep_for) => {
                    let inbound = self.shared.state.lock().map(|mut state| {
                        self.drive_timeouts_locked(&mut state)
                    }).unwrap_or_default();
                    self.deliver_inbound(
                        inbound,
                        &inbound_tx,
                        &relay_tx,
                        &requester_tx,
                        &target_tx,
                    );
                    self.flush_staged(&socket, is_v4, &child).await;
                }
            }
        }
    }

    fn deliver_inbound(
        &self,
        inbound: Ssu2InboundBatch,
        inbound_tx: &mpsc::Sender<Ssu2InboundI2np>,
        relay_tx: &mpsc::Sender<Ssu2InboundRelay>,
        requester_tx: &mpsc::Sender<Ssu2InboundRequesterEvent>,
        target_tx: &mpsc::Sender<Ssu2InboundTarget>,
    ) {
        for message in inbound.i2np {
            if inbound_tx.try_send(message).is_err() {
                self.shared
                    .counters
                    .inbound_queue_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        for event in inbound.relay {
            if relay_tx.try_send(event).is_err() {
                self.shared
                    .counters
                    .inbound_queue_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        for event in inbound.requester {
            if requester_tx.try_send(event).is_err() {
                self.shared
                    .counters
                    .inbound_queue_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        for event in inbound.target {
            if target_tx.try_send(event).is_err() {
                self.shared
                    .counters
                    .inbound_queue_drops
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl Ssu2LinkHandle {
    /// Queues one encoded I2NP message onto this handle's peer session.
    pub fn send(&self, message: EncodedI2npMessage, timeout: Duration) -> Ssu2SendOutcome {
        Ssu2RuntimeService {
            shared: Arc::clone(&self.shared),
        }
        .send_i2np(self.peer, message, timeout)
    }

    /// Closes exactly this link without disturbing a replacement link.
    pub fn close(&self, reason: TerminationCategory) -> i2pr_transport::CloseOutcome {
        Ssu2RuntimeService {
            shared: Arc::clone(&self.shared),
        }
        .close_ssu2(self.link_id, reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan 499: the peer-to-link resolution refuses an ambiguous peer
    /// rather than guessing which live session to carry a block on.
    ///
    /// The live two-link case cannot be built with the local harness: the
    /// generic transport manager keeps at most one active link per peer,
    /// so the runtime's link index never holds two links for one peer in
    /// production. The branch is therefore covered here, against the
    /// resolution helper the carriage API uses.
    #[test]
    fn relay_peer_resolution_refuses_ambiguous_and_empty_link_sets() {
        let first = LinkId::new(1).expect("link");
        let second = LinkId::new(2).expect("link");
        assert_eq!(
            resolve_single_peer_link(None),
            Err(Ssu2RelayQueueError::NoSession)
        );
        assert_eq!(
            resolve_single_peer_link(Some(&HashSet::new())),
            Err(Ssu2RelayQueueError::NoSession)
        );
        assert_eq!(
            resolve_single_peer_link(Some(&HashSet::from([first]))),
            Ok(first)
        );
        assert_eq!(
            resolve_single_peer_link(Some(&HashSet::from([first, second]))),
            Err(Ssu2RelayQueueError::AmbiguousSession)
        );
    }

    /// Plan 499: the transport's session errors map onto the runtime's
    /// carriage refusals, and anything unexpected fails closed.
    #[test]
    fn relay_session_errors_map_onto_carriage_refusals() {
        assert_eq!(
            relay_queue_reason(i2pr_transport_ssu2::SessionError::Terminated),
            Ssu2RelayQueueError::SessionTerminated
        );
        assert_eq!(
            relay_queue_reason(i2pr_transport_ssu2::SessionError::LocalPolicyDenied),
            Ssu2RelayQueueError::ControlQueueFull
        );
        assert_eq!(
            relay_queue_reason(i2pr_transport_ssu2::SessionError::OutboundQueueFull),
            Ssu2RelayQueueError::SessionTerminated
        );
    }

    #[test]
    fn source_write_receipt_requires_distinct_fragments_and_fails_partial_writes() {
        let (completion, mut result) = tokio::sync::oneshot::channel();
        let mut receipt = PendingSsu2WriteReceipt {
            expected_fragments: 3,
            written_fragments: HashSet::new(),
            expires_ms: 10,
            completion,
        };
        let fragment = |fragment_number| i2pr_transport_ssu2::session::I2npFragmentReceipt {
            message_id: 77,
            fragment_number,
            total_fragments: 3,
        };
        assert_eq!(
            settle_ssu2_write_fragment(&mut receipt, fragment(0), true),
            None
        );
        assert_eq!(
            settle_ssu2_write_fragment(&mut receipt, fragment(0), true),
            None
        );
        assert_eq!(
            settle_ssu2_write_fragment(&mut receipt, fragment(2), true),
            None
        );
        assert_eq!(receipt.written_fragments.len(), 2);
        assert_eq!(
            settle_ssu2_write_fragment(&mut receipt, fragment(1), true),
            Some(true)
        );
        let _ = receipt.completion.send(true);
        assert_eq!(result.try_recv(), Ok(true));

        let (completion, mut result) = tokio::sync::oneshot::channel();
        let mut partial = PendingSsu2WriteReceipt {
            expected_fragments: 2,
            written_fragments: HashSet::from([0]),
            expires_ms: 10,
            completion,
        };
        let failed = i2pr_transport_ssu2::session::I2npFragmentReceipt {
            message_id: 78,
            fragment_number: 1,
            total_fragments: 2,
        };
        assert_eq!(
            settle_ssu2_write_fragment(&mut partial, failed, false),
            Some(false)
        );
        let _ = partial.completion.send(false);
        assert_eq!(result.try_recv(), Ok(false));
    }

    fn identity_with_router_info(router_info: Vec<u8>) -> Ssu2IdentityMaterial {
        Ssu2IdentityMaterial {
            router_hash: Hash::from_bytes([0x11_u8; 32]),
            static_secret_bytes: [0x22_u8; 32],
            intro_key: IntroKey::new([0x33_u8; 32]),
            router_info,
        }
    }

    #[test]
    fn limits_validate_rejects_zero_ceiling_and_scope_violations() {
        assert!(Ssu2RuntimeConfig::default().validate().is_ok());
        let zero = Ssu2RuntimeLimits {
            max_pending_handshakes: 0,
            ..Default::default()
        };
        assert!(matches!(
            zero.validate(),
            Err(Ssu2RuntimeConfigError::ZeroLimit { .. })
        ));
        let huge = Ssu2RuntimeLimits {
            max_pending_handshakes: MAX_SSU2_PENDING_CEILING + 1,
            ..Default::default()
        };
        assert!(matches!(
            huge.validate(),
            Err(Ssu2RuntimeConfigError::LimitTooLarge { .. })
        ));
        let scoped = Ssu2RuntimeLimits {
            max_pending_per_ip: 65,
            max_pending_handshakes: 64,
            ..Default::default()
        };
        assert!(matches!(
            scoped.validate(),
            Err(Ssu2RuntimeConfigError::InconsistentLimits)
        ));
        let active_scoped = Ssu2RuntimeLimits {
            max_active_per_subnet: 65,
            max_active_sessions: 64,
            ..Default::default()
        };
        assert!(matches!(
            active_scoped.validate(),
            Err(Ssu2RuntimeConfigError::InconsistentLimits)
        ));
    }

    #[test]
    fn active_peer_hashes_is_empty_before_any_session() {
        let keys = make_path_keys();
        let service = Ssu2RuntimeService::new(
            Ssu2RuntimeConfig::default(),
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("service");
        assert!(service.active_peer_hashes().is_empty());
        assert_eq!(service.snapshot().active_sessions, 0);
    }

    #[test]
    fn deadlines_validate_ordering_and_bounds() {
        assert!(Ssu2RuntimeDeadlines::default().validate().is_ok());
        let zero = Ssu2RuntimeDeadlines {
            handshake: Duration::ZERO,
            ..Default::default()
        };
        assert!(matches!(
            zero.validate(),
            Err(Ssu2RuntimeConfigError::ZeroDeadline { .. })
        ));
        let inverted = Ssu2RuntimeDeadlines {
            handshake: Duration::from_secs(20),
            dial: Duration::from_secs(10),
            ..Default::default()
        };
        assert!(matches!(
            inverted.validate(),
            Err(Ssu2RuntimeConfigError::DialShorterThanHandshake)
        ));
    }

    #[test]
    fn the_keepalive_deadline_is_refused_when_it_cannot_fall_before_the_idle_timeout() {
        // Plan 522: the keepalive's unanswered deadline is three intervals,
        // and it must fall **strictly** before the session's idle timeout.
        // The two would otherwise disagree about when a session dies: the
        // keepalive refreshes the session's send timestamp, and the idle
        // timeout reads the maximum of the send and receive timestamps.
        //
        // The daemon's parse-time refusal is the first line. This is the
        // second, and it is the one a different caller of the runtime crate
        // would meet.
        let idle = Duration::from_secs(300);
        let accepted = Ssu2RuntimeDeadlines {
            keepalive: Some(Duration::from_secs(60)),
            idle,
            ..Default::default()
        };
        assert!(accepted.validate().is_ok());

        let zero = Ssu2RuntimeDeadlines {
            keepalive: Some(Duration::ZERO),
            idle,
            ..Default::default()
        };
        assert!(matches!(
            zero.validate(),
            Err(Ssu2RuntimeConfigError::ZeroDeadline { field: "keepalive" })
        ));

        let too_long = Ssu2RuntimeDeadlines {
            keepalive: Some(idle),
            idle,
            ..Default::default()
        };
        assert!(matches!(
            too_long.validate(),
            Err(Ssu2RuntimeConfigError::KeepaliveTooLong)
        ));

        let beyond_the_ceiling = Ssu2RuntimeDeadlines {
            keepalive: Some(MAX_SSU2_RUNTIME_DURATION + Duration::from_secs(1)),
            idle,
            ..Default::default()
        };
        assert!(matches!(
            beyond_the_ceiling.validate(),
            Err(Ssu2RuntimeConfigError::DeadlineTooLong { field: "keepalive" })
        ));

        // A router without the pool carries no keepalive at all, and that is
        // the default posture.
        assert!(Ssu2RuntimeDeadlines::default().validate().is_ok());
    }

    #[test]
    fn dial_targets_validate_before_socket_activity() {
        let hash = Hash::from_bytes([0x44_u8; 32]);
        let peer = PeerId::from_hash(hash);
        let public = Ssu2PublicKey::new([0x55_u8; 32]).expect("public");
        let intro = IntroKey::new([0x66_u8; 32]);
        let good: SocketAddr = "127.0.0.1:43000".parse().expect("addr");
        assert!(Ssu2DialTarget::new(peer, hash, good, public, intro).is_ok());
        let zero_port: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        assert_eq!(
            Ssu2DialTarget::new(peer, hash, zero_port, public, intro),
            Err(Ssu2DialTargetError::ZeroPort)
        );
        let unspecified: SocketAddr = "0.0.0.0:43000".parse().expect("addr");
        assert_eq!(
            Ssu2DialTarget::new(peer, hash, unspecified, public, intro),
            Err(Ssu2DialTargetError::UnspecifiedIp)
        );
        let public_ip: SocketAddr = "192.0.2.1:43000".parse().expect("addr");
        assert_eq!(
            Ssu2DialTarget::new(peer, hash, public_ip, public, intro),
            Err(Ssu2DialTargetError::NotLoopback)
        );
        let other = Hash::from_bytes([0x77_u8; 32]);
        assert_eq!(
            Ssu2DialTarget::new(peer, other, good, public, intro),
            Err(Ssu2DialTargetError::PeerHashMismatch)
        );
    }

    #[test]
    fn private_lan_dial_targets_are_rfc1918_ipv4_only() {
        let hash = Hash::from_bytes([0x44_u8; 32]);
        let peer = PeerId::from_hash(hash);
        let public = Ssu2PublicKey::new([0x55_u8; 32]).expect("public");
        let intro = IntroKey::new([0x66_u8; 32]);
        let private: SocketAddr = "192.168.44.11:19002".parse().expect("addr");
        assert!(Ssu2DialTarget::new_private_lan(peer, hash, private, public, intro).is_ok());

        for (address, expected) in [
            ("127.0.0.1:19002", Ssu2DialTargetError::NotPrivateLanIpv4),
            ("192.0.2.1:19002", Ssu2DialTargetError::NotPrivateLanIpv4),
            ("8.8.8.8:19002", Ssu2DialTargetError::NotPrivateLanIpv4),
            ("[fd00::1]:19002", Ssu2DialTargetError::NotPrivateLanIpv4),
            ("192.168.44.11:0", Ssu2DialTargetError::ZeroPort),
            ("0.0.0.0:19002", Ssu2DialTargetError::UnspecifiedIp),
        ] {
            let address = address.parse().expect("addr");
            assert_eq!(
                Ssu2DialTarget::new_private_lan(peer, hash, address, public, intro),
                Err(expected),
                "{address}"
            );
        }
        assert_eq!(
            Ssu2DialTarget::new_private_lan(
                peer,
                Hash::from_bytes([0x77; 32]),
                private,
                public,
                intro,
            ),
            Err(Ssu2DialTargetError::PeerHashMismatch)
        );
    }

    #[test]
    fn identity_rejects_malformed_router_info() {
        let config = Ssu2RuntimeConfig::default();
        assert!(matches!(
            Ssu2RuntimeService::new(config, identity_with_router_info(Vec::new())),
            Err(Ssu2RuntimeConfigError::InvalidIdentity)
        ));
        assert!(matches!(
            Ssu2RuntimeService::new(config, identity_with_router_info(vec![0xFF_u8; 64])),
            Err(Ssu2RuntimeConfigError::InvalidIdentity)
        ));
    }

    // -- Plan 498 non-blocking relay accessor ---------------------------
    //
    // The daemon's live inbound loop pulls the runtime's relay handoff on
    // a bounded tick, because the two handoff receivers live behind one
    // `&mut self` handle and cannot both be awaited by one `select!`.
    // These rows pin the pull's semantics without a socket: an empty
    // channel yields `None` and consumes nothing, a queued event is
    // returned, the channel is empty again afterwards, and a disconnected
    // channel is not mistaken for a live event.

    fn relay_service_for_test() -> Ssu2RuntimeService {
        let keys = make_path_keys();
        Ssu2RuntimeService::new(
            Ssu2RuntimeConfig::default(),
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info,
            },
        )
        .expect("service")
    }

    fn relay_handle_for_test() -> (Ssu2ServiceHandle, mpsc::Sender<Ssu2InboundRelay>) {
        let service = relay_service_for_test();
        let (_inbound_tx, inbound_rx) = mpsc::channel::<Ssu2InboundI2np>(1);
        let (relay_tx, relay_rx) = mpsc::channel::<Ssu2InboundRelay>(
            Ssu2RuntimeConfig::default().limits.max_inbound_relay_queue,
        );
        let (_requester_tx, requester_rx) = mpsc::channel::<Ssu2InboundRequesterEvent>(
            Ssu2RuntimeConfig::default().limits.max_inbound_relay_queue,
        );
        let (_target_tx, target_rx) = mpsc::channel::<Ssu2InboundTarget>(
            Ssu2RuntimeConfig::default().limits.max_inbound_relay_queue,
        );
        (
            Ssu2ServiceHandle {
                service,
                inbound: inbound_rx,
                relay: relay_rx,
                requester: requester_rx,
                target: target_rx,
                local_v4: None,
                local_v6: None,
            },
            relay_tx,
        )
    }

    fn relay_event_for_test() -> Ssu2InboundRelay {
        let endpoint = Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40000).expect("endpoint");
        let block =
            i2pr_transport_ssu2::RelayRequestBlock::new(21, 7, 1000, 2, endpoint, vec![0x5A; 64])
                .expect("block");
        Ssu2InboundRelay {
            link_id: LinkId::new(1).expect("link"),
            peer: PeerId::from_hash(Hash::from_bytes([0x77_u8; 32])),
            event: Ssu2RelayEvent::Request(block),
        }
    }

    #[test]
    fn try_next_relay_event_on_an_empty_channel_yields_none_and_consumes_nothing() {
        let (mut handle, relay_tx) = relay_handle_for_test();
        assert!(handle.try_next_relay_event().is_none());
        relay_tx.try_send(relay_event_for_test()).expect("queue");
        assert!(handle.try_next_relay_event().is_some());
    }

    #[test]
    fn try_next_relay_event_returns_a_queued_event_and_then_the_channel_is_empty() {
        let (mut handle, relay_tx) = relay_handle_for_test();
        relay_tx.try_send(relay_event_for_test()).expect("queue");
        let event = handle.try_next_relay_event().expect("event");
        match event.event {
            Ssu2RelayEvent::Request(block) => assert_eq!(block.nonce(), 21),
            other => panic!("unexpected relay event: {other:?}"),
        }
        assert!(handle.try_next_relay_event().is_none());
    }

    #[test]
    fn try_next_relay_event_yields_none_after_the_channel_disconnects() {
        let (mut handle, relay_tx) = relay_handle_for_test();
        drop(relay_tx);
        assert!(handle.try_next_relay_event().is_none());
    }

    // -- Plan 159 real-UDP path-validation acceptance -------------------
    //
    // Two live services drive authenticated sessions over real
    // loopback datagrams while a raw third socket plays the
    // changed-path peer. Sealed bytes are produced by the live
    // sessions themselves (never forged), so every migration decision
    // below authenticates exactly like production traffic.

    use crate::ChildFailurePolicy;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping, RouterAddress};

    const PATH_TEST_TIMEOUT: Duration = Duration::from_secs(10);
    const PATH_TEST_POLL: Duration = Duration::from_millis(25);

    struct PathKeys {
        bundle: RouterIdentityBundle,
        peer: PeerId,
        hash: Hash,
        static_bytes: [u8; 32],
        static_public: Ssu2PublicKey,
        intro: IntroKey,
        router_info: Vec<u8>,
    }

    struct PathFixture {
        service: Ssu2RuntimeService,
        scope: ChildScope,
        handle: Ssu2ServiceHandle,
        keys: PathKeys,
    }

    impl PathFixture {
        fn addr(&self) -> SocketAddr {
            self.handle.local_v4().expect("bound v4")
        }
    }

    fn i2p_b64_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let mut n: u32 = 0;
            for byte in chunk {
                n = (n << 8) | u32::from(*byte);
            }
            n <<= 8 * (3 - chunk.len());
            let digits = match chunk.len() {
                1 => 2,
                2 => 3,
                _ => 4,
            };
            for index in 0..digits {
                output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
            }
            for _ in digits..4 {
                output.push('=');
            }
        }
        output
    }

    fn path_wall_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(1_700_000_000)
    }

    fn make_path_keys() -> PathKeys {
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = bundle.identity().hash().expect("hash");
        let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
        let static_bytes = *static_key.secret_bytes();
        let static_public = Ssu2PublicKey::new(static_key.public_bytes()).expect("static public");
        let mut intro_bytes = [0_u8; 32];
        loop {
            OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
            if intro_bytes.iter().any(|byte| *byte != 0) {
                break;
            }
        }
        let intro = IntroKey::new(intro_bytes);
        let options = Mapping::from_entries(vec![
            ("host".to_string(), "127.0.0.1".to_string()),
            ("port".to_string(), "43000".to_string()),
            ("v".to_string(), "2".to_string()),
            ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
            ("i".to_string(), i2p_b64_encode(&intro_bytes)),
        ])
        .expect("options");
        let address = RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("address");
        let info = bundle
            .sign_router_info(
                Date::from_millis(path_wall_secs().saturating_mul(1000)),
                vec![address],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        let router_info = info
            .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode");
        PathKeys {
            bundle,
            peer: PeerId::from_hash(hash),
            hash,
            static_bytes,
            static_public,
            intro,
            router_info,
        }
    }

    async fn start_path_fixture(keys: PathKeys) -> PathFixture {
        start_path_fixture_with_config(Ssu2RuntimeConfig::default(), keys).await
    }

    async fn start_path_fixture_with_config(
        config: Ssu2RuntimeConfig,
        keys: PathKeys,
    ) -> PathFixture {
        let service = Ssu2RuntimeService::new(
            config,
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("service");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let handle = service
            .start(
                &scope,
                Ssu2SocketConfig {
                    ipv4: Some("127.0.0.1:0".parse().expect("loopback")),
                    ipv6: None,
                },
            )
            .await
            .expect("bind");
        PathFixture {
            service,
            scope,
            handle,
            keys,
        }
    }

    async fn wait_for_snapshot(
        service: &Ssu2RuntimeService,
        mut condition: impl FnMut(&Ssu2Snapshot) -> bool,
        what: &str,
    ) {
        let deadline = tokio::time::Instant::now() + PATH_TEST_TIMEOUT;
        loop {
            if condition(&service.snapshot()) {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            tokio::time::sleep(PATH_TEST_POLL).await;
        }
    }

    async fn path_dial(from: &PathFixture, peer: &PathKeys, addr: SocketAddr) {
        let target =
            Ssu2DialTarget::new(peer.peer, peer.hash, addr, peer.static_public, peer.intro)
                .expect("dial target");
        from.service
            .dial_ssu2(target, PATH_TEST_TIMEOUT, &CancellationToken::new())
            .await
            .expect("dial");
    }

    fn path_i2np(id: u32, body_len: usize, seed: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(9 + body_len);
        bytes.push(6_u8);
        bytes.extend_from_slice(&id.to_be_bytes());
        let expiration = path_wall_secs()
            .saturating_add(3600)
            .min(u64::from(u32::MAX)) as u32;
        bytes.extend_from_slice(&expiration.to_be_bytes());
        for index in 0..body_len {
            bytes.push(seed.wrapping_add((index % 251) as u8));
        }
        bytes
    }

    fn path_send(fixture: &PathFixture, peer: PeerId, id: u32) -> Vec<u8> {
        let bytes = path_i2np(id, 128, (id & 0xFF) as u8);
        let message = EncodedI2npMessage::new(bytes.clone()).expect("message");
        assert_eq!(
            fixture
                .service
                .send_i2np(peer, message, Duration::from_secs(5)),
            Ssu2SendOutcome::Accepted
        );
        bytes
    }

    async fn path_recv(handle: &mut Ssu2ServiceHandle, count: usize) -> Vec<Vec<u8>> {
        let mut messages = Vec::new();
        let deadline = tokio::time::Instant::now() + PATH_TEST_TIMEOUT;
        while messages.len() < count {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for I2NP");
            let next = tokio::time::timeout(remaining, handle.next_inbound())
                .await
                .expect("timeout")
                .expect("channel open");
            messages.push(next.bytes);
        }
        messages
    }

    /// Seals one fresh control packet through a live session without
    /// using the staging queue: the scheduler may concurrently consume
    /// queued controls, so retry until this call seals the bytes
    /// itself (every seal carries a fresh packet number).
    async fn seal_control(
        service: &Ssu2RuntimeService,
        control: &[u8],
        is_challenge: bool,
    ) -> Vec<u8> {
        for _ in 0..20 {
            let sealed = service.shared.state.lock().ok().and_then(|mut state| {
                let record = state.active.values_mut().next()?;
                let now_ms = monotonic_ms(&service.shared.started_at);
                let queued = if is_challenge {
                    record
                        .session
                        .queue_path_challenge(control.to_vec())
                        .is_ok()
                } else {
                    record.session.queue_path_response(control.to_vec()).is_ok()
                };
                if !queued {
                    return None;
                }
                record.session.poll_transmit(now_ms)
            });
            if let Some(bytes) = sealed {
                return bytes;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("live session did not seal the control packet");
    }

    fn session_peer_addr(service: &Ssu2RuntimeService) -> SocketAddr {
        service
            .shared
            .state
            .lock()
            .ok()
            .and_then(|state| state.active.values().next().map(|record| record.peer_addr))
            .expect("active session")
    }

    fn session_candidate_count(service: &Ssu2RuntimeService) -> usize {
        service
            .shared
            .state
            .lock()
            .ok()
            .map(|state| {
                state
                    .active
                    .values()
                    .map(|record| record.path.candidate_count())
                    .sum()
            })
            .unwrap_or(usize::MAX)
    }

    fn candidate_challenge(service: &Ssu2RuntimeService, addr: SocketAddr) -> [u8; 32] {
        let endpoint = Ssu2Endpoint::from_socket_addr(addr).expect("endpoint");
        service
            .shared
            .state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .active
                    .values()
                    .next()
                    .and_then(|record| record.path.challenge_for(endpoint))
            })
            .expect("candidate challenge")
    }

    async fn shutdown_path_fixture(fixture: PathFixture) {
        fixture.service.shutdown();
        let report = fixture.scope.shutdown().await;
        assert!(report.joined() >= 1, "loop task joined");
    }

    #[tokio::test]
    async fn legitimate_path_migration_over_real_udp() {
        let mut a = start_path_fixture(make_path_keys()).await;
        let mut b = start_path_fixture(make_path_keys()).await;
        path_dial(&a, &b.keys, b.addr()).await;
        wait_for_snapshot(
            &a.service,
            |snapshot| snapshot.active_sessions == 1,
            "a active",
        )
        .await;
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.active_sessions == 1,
            "b active",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Baseline health in both directions first.
        let first = path_send(&a, b.keys.peer, 0xE001);
        assert_eq!(path_recv(&mut b.handle, 1).await, vec![first]);
        let back = path_send(&b, a.keys.peer, 0xE101);
        assert_eq!(path_recv(&mut a.handle, 1).await, vec![back]);

        // A third loopback socket plays the peer's new path.
        let raw = UdpSocket::bind("127.0.0.1:0").await.expect("raw socket");
        let c_addr = raw.local_addr().expect("raw addr");
        let b_addr = b.addr();
        // Suppress A's staged originals while sealing triggers: every
        // byte below is sealed by the live session and crosses a real
        // socket exactly once, from the asserted source.
        a.service.set_test_faults(Some(Ssu2TestFaults {
            drop_transmit: (0..32).collect(),
            ..Default::default()
        }));
        let trigger = seal_control(&a.service, &[0xCC_u8; 32], true).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        a.service.set_test_faults(None);
        // Scheduler-stolen control copies (if any) were dropped by the
        // armed fault armor; the returned bytes below are fresh.
        // The genuine bytes arrive at B from the new source over UDP.
        raw.send_to(&trigger, b_addr).await.expect("trigger send");
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.path_challenges >= 1,
            "candidate challenge",
        )
        .await;
        assert_eq!(session_peer_addr(&b.service), a.addr());
        let count = session_candidate_count(&b.service);
        assert_eq!(count, 1);
        // B's challenge reaches the new path over real UDP (opaque
        // sealed bytes: the value is read from B's validator, exactly
        // what the legitimate peer learns by receiving the packet).
        let mut buffer = vec![0_u8; 2048];
        let (len, from) = tokio::time::timeout(PATH_TEST_TIMEOUT, raw.recv_from(&mut buffer))
            .await
            .expect("challenge recv")
            .expect("datagram");
        assert_eq!(from, b_addr);
        assert!((40..=1280).contains(&len));
        // Seal A's answer through A's live session, then deliver it
        // from the new source over real UDP.
        let challenge = candidate_challenge(&b.service, c_addr);
        a.service.set_test_faults(Some(Ssu2TestFaults {
            drop_transmit: (0..32).collect(),
            ..Default::default()
        }));
        let answer = seal_control(&a.service, &challenge, false).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        a.service.set_test_faults(None);
        raw.send_to(&answer, b_addr).await.expect("answer send");
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.path_migrations >= 1,
            "path migration",
        )
        .await;
        assert_eq!(session_peer_addr(&b.service), c_addr);
        assert_eq!(session_candidate_count(&b.service), 0);
        // Behavioral proof: B's next datagram to the peer leaves for
        // the migrated path and arrives at the raw socket.
        let e002 = path_send(&b, a.keys.peer, 0xE002);
        let (len, from) = tokio::time::timeout(PATH_TEST_TIMEOUT, raw.recv_from(&mut buffer))
            .await
            .expect("migrated recv")
            .expect("datagram");
        assert_eq!(from, b_addr);
        assert!((40..=1280).contains(&len));
        // Old-path replay: the same genuine bytes from A's original
        // address authenticate as a replay and migrate nothing.
        b.service.handle_datagram(&trigger, a.addr(), true, None);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(b.service.snapshot().path_migrations, 1);
        assert_eq!(session_candidate_count(&b.service), 0);
        // A fresh packet from the old address opens a bounded
        // candidate under the current rules. The live peer genuinely
        // answers B's challenge (automatic spec-defined
        // challenge/response in both runtimes), so the validated path
        // legitimately migrates back with zero test intervention.
        let old_path = path_send(&a, b.keys.peer, 0xE003);
        assert_eq!(path_recv(&mut b.handle, 1).await, vec![old_path]);
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.path_migrations >= 2,
            "return migration",
        )
        .await;
        assert_eq!(session_peer_addr(&b.service), a.addr());
        assert_eq!(session_candidate_count(&b.service), 0);
        // Both directions stay usable after the double migration.
        // The unacked pre-return message legitimately retransmits
        // fresh to the restored path, so A receives it alongside the
        // new traffic (order-insensitive set comparison).
        let there = path_send(&a, b.keys.peer, 0xE004);
        assert_eq!(path_recv(&mut b.handle, 1).await, vec![there]);
        let back_again = path_send(&b, a.keys.peer, 0xE102);
        let mut got = path_recv(&mut a.handle, 2).await;
        got.sort();
        let mut want = vec![e002, back_again];
        want.sort();
        assert_eq!(got, want);

        shutdown_path_fixture(a).await;
        shutdown_path_fixture(b).await;
    }

    #[tokio::test]
    async fn spoof_burst_from_new_sources_never_migrates() {
        let mut a = start_path_fixture(make_path_keys()).await;
        let mut b = start_path_fixture(make_path_keys()).await;
        path_dial(&a, &b.keys, b.addr()).await;
        wait_for_snapshot(
            &a.service,
            |snapshot| snapshot.active_sessions == 1,
            "a active",
        )
        .await;
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.active_sessions == 1,
            "b active",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let baseline = b.service.snapshot();

        // Four spoof sources blast unauthenticated traffic at B: short
        // datagrams, oversized datagrams, valid-length randomness, and
        // handshake-shaped prefixes. None can authenticate, so none
        // may open a candidate, let alone migrate.
        let b_addr = b.addr();
        let mut sources = Vec::new();
        for _ in 0..4 {
            sources.push(UdpSocket::bind("127.0.0.1:0").await.expect("spoof socket"));
        }
        let mut rng_state: u32 = 0x5A5A_5A5A;
        for _ in 0..50 {
            for source in &sources {
                // Too short.
                source.send_to(&[0x99_u8; 16], b_addr).await.expect("send");
                // Valid-length randomness.
                let mut random = vec![0_u8; 96];
                for byte in random.iter_mut() {
                    rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *byte = (rng_state >> 24) as u8;
                }
                source.send_to(&random, b_addr).await.expect("send");
                // Oversized.
                source
                    .send_to(&vec![0x77_u8; 2000], b_addr)
                    .await
                    .expect("send");
                // Handshake-shaped prefix with a bad version.
                let mut shaped = vec![0xAA_u8; 64];
                shaped[12] = 0x7F;
                source.send_to(&shaped, b_addr).await.expect("send");
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let after = b.service.snapshot();
        assert_eq!(after.active_sessions, 1);
        assert_eq!(after.path_challenges, 0);
        assert_eq!(after.path_migrations, 0);
        assert_eq!(after.path_rejections, 0);
        assert_eq!(session_candidate_count(&b.service), 0);
        assert_eq!(session_peer_addr(&b.service), a.addr());
        assert!(
            after.cheap_drops + after.auth_failures > baseline.cheap_drops + baseline.auth_failures,
            "spoof traffic was cheaply classified"
        );
        // The session survives the burst fully usable in both
        // directions.
        let first = path_send(&a, b.keys.peer, 0xF001);
        assert_eq!(path_recv(&mut b.handle, 1).await, vec![first]);
        let back = path_send(&b, a.keys.peer, 0xF101);
        assert_eq!(path_recv(&mut a.handle, 1).await, vec![back]);

        shutdown_path_fixture(a).await;
        shutdown_path_fixture(b).await;
    }

    #[tokio::test]
    async fn challenge_response_control_round_trip_over_real_udp() {
        let a = start_path_fixture(make_path_keys()).await;
        let b = start_path_fixture(make_path_keys()).await;
        path_dial(&a, &b.keys, b.addr()).await;
        wait_for_snapshot(
            &a.service,
            |snapshot| snapshot.active_sessions == 1,
            "a active",
        )
        .await;
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.active_sessions == 1,
            "b active",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // A challenges B over the validated path through the live
        // session; B answers. Neither side migrates: the source never
        // changed. Returned seals are staged explicitly; scheduler-
        // stolen copies already flew on their own.
        let sealed = seal_control(&a.service, &[0xD0_u8; 32], true).await;
        {
            let mut state = a.service.shared.state.lock().expect("state");
            let limits = a.service.shared.config.limits;
            stage_one_locked(
                &mut state,
                &limits,
                &a.service.shared.counters,
                sealed,
                b.addr(),
            );
        }
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.path_responses >= 1,
            "peer answer",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(a.service.snapshot().path_migrations, 0);
        assert_eq!(b.service.snapshot().path_migrations, 0);
        assert_eq!(a.service.snapshot().path_challenges, 0);
        assert_eq!(session_candidate_count(&a.service), 0);
        assert_eq!(session_candidate_count(&b.service), 0);

        shutdown_path_fixture(a).await;
        shutdown_path_fixture(b).await;
    }

    #[tokio::test]
    async fn publication_material_uses_bound_socket_and_requires_fresh_reachability() {
        let keys = make_path_keys();
        let unbound = Ssu2RuntimeService::new(
            Ssu2RuntimeConfig::default(),
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("valid unbound runtime");
        assert_eq!(
            unbound.publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::NoBoundSocket)
        );
        let fixture = start_path_fixture(keys).await;
        assert_eq!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)
        );
        {
            let mut state = fixture.service.shared.state.lock().expect("state");
            let family = i2pr_transport::AddressFamily::Ipv4;
            let now = fixture.service.service_now();
            state
                .reachability
                .record(ReachabilitySignal::ValidatedPath { family }, now);
            state.reachability.record(
                ReachabilitySignal::AuthenticatedPeerObservedExternalAddress { family },
                now,
            );
            state.reachability.record(
                ReachabilitySignal::PeerTestResult {
                    family,
                    outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed,
                },
                now,
            );
        }
        let material = fixture
            .service
            .publication_material(path_wall_secs() * 1000)
            .expect("qualified publication material");
        let parsed = Ssu2RouterAddress::parse(&material.address).expect("strict address");
        let endpoint = parsed.endpoint().expect("direct endpoint");
        assert_eq!(endpoint.ip(), fixture.addr().ip());
        assert_eq!(endpoint.port(), fixture.addr().port());
        assert_ne!(endpoint.port(), 0);
        assert_eq!(
            parsed.static_public_key().as_bytes(),
            fixture.keys.static_public.as_bytes()
        );
        assert_eq!(
            parsed.intro_key().expect("intro key").as_bytes(),
            fixture.keys.intro.as_bytes()
        );
        assert_eq!(
            material.reachability,
            i2pr_transport::ReachabilityState::Reachable
        );
        let replacement = fixture
            .keys
            .bundle
            .sign_router_info(
                Date::from_millis(path_wall_secs() * 1000),
                vec![material.address.clone()],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign current bound publication");
        let replacement = replacement
            .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode signed replacement");
        fixture
            .service
            .install_local_router_info(replacement.clone(), path_wall_secs() * 1000)
            .expect("install matching local identity and SSU2 binding");
        assert_eq!(
            *fixture
                .service
                .shared
                .local_router_info
                .lock()
                .expect("local RouterInfo"),
            replacement
        );
        shutdown_path_fixture(fixture).await;
    }

    fn sign_bound_router_info(
        bundle: &RouterIdentityBundle,
        static_public: &Ssu2PublicKey,
        intro: &IntroKey,
        addr: SocketAddr,
        published_ms: u64,
    ) -> Vec<u8> {
        let options = Mapping::from_entries(vec![
            ("host".to_string(), addr.ip().to_string()),
            ("port".to_string(), addr.port().to_string()),
            ("v".to_string(), "2".to_string()),
            ("s".to_string(), i2p_b64_encode(static_public.as_bytes())),
            ("i".to_string(), i2p_b64_encode(intro.as_bytes())),
        ])
        .expect("options");
        let address = RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("address");
        bundle
            .sign_router_info(
                Date::from_millis(published_ms),
                vec![address],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign")
            .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode")
    }

    fn qualify_reachability(service: &Ssu2RuntimeService) {
        let mut state = service.shared.state.lock().expect("state");
        let family = i2pr_transport::AddressFamily::Ipv4;
        let now = service.service_now();
        state
            .reachability
            .record(ReachabilitySignal::ValidatedPath { family }, now);
        state.reachability.record(
            ReachabilitySignal::AuthenticatedPeerObservedExternalAddress { family },
            now,
        );
        state.reachability.record(
            ReachabilitySignal::PeerTestResult {
                family,
                outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed,
            },
            now,
        );
    }

    #[tokio::test]
    async fn publication_material_rejects_expired_evidence_and_closed_runtime() {
        let fixture = start_path_fixture(make_path_keys()).await;
        qualify_reachability(&fixture.service);
        assert!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000)
                .is_ok(),
            "qualified evidence yields material"
        );
        {
            let mut state = fixture.service.shared.state.lock().expect("state");
            let expired_at = fixture.service.service_now()
                + i2pr_transport::DEFAULT_OBSERVATION_TTL
                + Duration::from_secs(2);
            assert!(
                state.reachability.poll_expiry(expired_at),
                "forced expiry withdraws support"
            );
        }
        assert_eq!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)
        );
        fixture.service.shutdown();
        assert_eq!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::Closed)
        );
        assert_eq!(
            fixture.service.install_local_router_info(
                fixture.keys.router_info.clone(),
                path_wall_secs() * 1000
            ),
            Err(Ssu2PublicationUnavailable::Closed)
        );
        let report = fixture.scope.shutdown().await;
        assert!(report.joined() >= 1, "loop task joined");
    }

    #[tokio::test]
    async fn publication_material_rejects_partial_evidence_and_forged_endpoint_state() {
        let fixture = start_path_fixture(make_path_keys()).await;
        {
            let mut state = fixture.service.shared.state.lock().expect("state");
            state.reachability.record(
                ReachabilitySignal::ValidatedPath {
                    family: i2pr_transport::AddressFamily::Ipv4,
                },
                fixture.service.service_now(),
            );
        }
        assert_eq!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)
        );
        qualify_reachability(&fixture.service);
        assert!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000)
                .is_ok(),
            "full corroboration qualifies"
        );
        {
            let mut sockets = fixture.service.shared.sockets.lock().expect("sockets");
            sockets.v4 = Some("10.0.0.1:1234".parse().expect("forged endpoint"));
        }
        assert_eq!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::InvalidPublication)
        );
        {
            let mut sockets = fixture.service.shared.sockets.lock().expect("sockets");
            sockets.v4 = Some("127.0.0.1:0".parse().expect("zero port"));
        }
        assert_eq!(
            fixture
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::InvalidPublication)
        );
        shutdown_path_fixture(fixture).await;
    }

    #[tokio::test]
    async fn explicit_bind_corroboration_pairs_with_peer_observation() {
        let observed = |service: &Ssu2RuntimeService| {
            let mut state = service.shared.state.lock().expect("state");
            state.reachability.record(
                ReachabilitySignal::AuthenticatedPeerObservedExternalAddress {
                    family: i2pr_transport::AddressFamily::Ipv4,
                },
                service.service_now(),
            );
        };
        // Default policy ignores the explicit bind: one observed class is not enough.
        let plain = start_path_fixture(make_path_keys()).await;
        observed(&plain.service);
        plain
            .service
            .note_explicit_bind_for_controlled_qualification()
            .expect("record loopback bind");
        assert_eq!(
            plain.service.publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)
        );
        shutdown_path_fixture(plain).await;
        // Opted-in policy counts the explicit loopback bind plus the real
        // peer observation as the two corroboration classes.
        let config = Ssu2RuntimeConfig {
            explicit_bind_corroboration: true,
            ..Ssu2RuntimeConfig::default()
        };
        let qualified = start_path_fixture_with_config(config, make_path_keys()).await;
        observed(&qualified.service);
        assert_eq!(
            qualified
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)
        );
        qualified
            .service
            .note_explicit_bind_for_controlled_qualification()
            .expect("record loopback bind");
        // The pair yields exactly the corroboration floor: the Plan 159 builder still
        // requires above-floor `Reachable` for a direct address, so loopback-homogeneous
        // evidence stops here by design (Plan 282 stop condition 5).
        {
            let state = qualified.service.shared.state.lock().expect("state");
            assert_eq!(
                state.reachability.state(),
                i2pr_transport::ReachabilityState::CandidateReachable
            );
        }
        assert_eq!(
            qualified
                .service
                .publication_material(path_wall_secs() * 1000),
            Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)
        );
        shutdown_path_fixture(qualified).await;
    }

    #[test]
    fn local_router_info_freshness_policy_boundaries() {
        let now_ms = 1_700_000_000_000_u64;
        let age_ms = LOCAL_ROUTER_INFO_MAX_AGE_SECS.saturating_mul(1000);
        let skew_ms = LOCAL_ROUTER_INFO_MAX_FUTURE_SKEW_SECS.saturating_mul(1000);
        assert!(local_router_info_fresh(now_ms, now_ms));
        assert!(local_router_info_fresh(
            now_ms.saturating_sub(age_ms),
            now_ms
        ));
        assert!(!local_router_info_fresh(
            now_ms.saturating_sub(age_ms.saturating_add(1000)),
            now_ms
        ));
        assert!(local_router_info_fresh(
            now_ms.saturating_add(skew_ms),
            now_ms
        ));
        assert!(!local_router_info_fresh(
            now_ms.saturating_add(skew_ms.saturating_add(1000)),
            now_ms
        ));
    }

    fn controlled_test_service(permitted: bool) -> (Ssu2RuntimeService, PathKeys) {
        let keys = make_path_keys();
        let service = Ssu2RuntimeService::new(
            Ssu2RuntimeConfig {
                controlled_peer_test: permitted,
                ..Ssu2RuntimeConfig::default()
            },
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("service");
        (service, keys)
    }

    fn controlled_test_block(message: u8) -> PeerTestBlock {
        let endpoint = Ssu2Endpoint::from_socket_addr("127.0.0.1:43001".parse().expect("loopback"))
            .expect("endpoint");
        PeerTestBlock::new(
            message,
            0,
            None,
            2,
            7,
            1_700_000_000,
            endpoint,
            vec![0xCD; 64],
        )
        .expect("block")
    }

    #[test]
    fn controlled_peer_test_apis_refused_without_permit() {
        let (service, keys) = controlled_test_service(false);
        let endpoint = Ssu2Endpoint::from_socket_addr("127.0.0.1:43001".parse().expect("loopback"))
            .expect("endpoint");
        let peer = PeerId::from_hash(keys.hash);
        assert_eq!(
            service.start_controlled_peer_test(
                7,
                PeerTestRole::Alice,
                *keys.hash.as_bytes(),
                [0x02; 32],
                [0x03; 32],
                endpoint,
            ),
            Err(ControlledPeerTestError::NotPermitted)
        );
        let signer = keys
            .bundle
            .signing_key()
            .public_key()
            .expect("signing public");
        assert_eq!(
            service.register_controlled_peer_signer([0x01; 32], &signer),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.queue_controlled_peer_test(peer, controlled_test_block(1)),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.queue_controlled_address(peer),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.send_controlled_peer_test(
                "127.0.0.1:43002".parse().expect("loopback"),
                &keys.intro,
                controlled_test_block(5),
            ),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.take_controlled_forward(7),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.controlled_test_outcome(7),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.controlled_test_state(7),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(
            service.finish_controlled_peer_test(7),
            Err(ControlledPeerTestError::NotPermitted)
        );
        assert_eq!(service.snapshot().reachability, ReachabilityState::Unknown);
    }

    #[test]
    fn controlled_peer_test_start_enforces_bindings() {
        let (service, keys) = controlled_test_service(true);
        let own_hash = *keys.hash.as_bytes();
        let endpoint = Ssu2Endpoint::from_socket_addr("127.0.0.1:43001".parse().expect("loopback"))
            .expect("endpoint");
        // The service plays exactly the role whose hash is its own.
        assert_eq!(
            service.start_controlled_peer_test(
                7,
                PeerTestRole::Bob,
                own_hash,
                [0x02; 32],
                [0x03; 32],
                endpoint,
            ),
            Err(ControlledPeerTestError::BindingMismatch)
        );
        // Non-loopback endpoints never bind a test.
        let public = Ssu2Endpoint::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 1)),
            43001,
        )
        .expect("endpoint");
        assert_eq!(
            service.start_controlled_peer_test(
                7,
                PeerTestRole::Alice,
                own_hash,
                [0x02; 32],
                [0x03; 32],
                public,
            ),
            Err(ControlledPeerTestError::BindingMismatch)
        );
        // Alice role requires the endpoint to be a bound socket; this
        // service bound nothing.
        assert_eq!(
            service.start_controlled_peer_test(
                7,
                PeerTestRole::Alice,
                own_hash,
                [0x02; 32],
                [0x03; 32],
                endpoint,
            ),
            Err(ControlledPeerTestError::BindingMismatch)
        );
        assert_eq!(service.snapshot().reachability, ReachabilityState::Unknown);
    }

    #[tokio::test]
    async fn install_local_router_info_enforces_policy_and_bumps_generation() {
        let fixture = start_path_fixture(make_path_keys()).await;
        let now_ms = path_wall_secs() * 1000;
        let age_ms = LOCAL_ROUTER_INFO_MAX_AGE_SECS.saturating_mul(1000);
        let skew_ms = LOCAL_ROUTER_INFO_MAX_FUTURE_SKEW_SECS.saturating_mul(1000);
        assert_eq!(fixture.service.local_router_info_generation(), 0);
        let stale = sign_bound_router_info(
            &fixture.keys.bundle,
            &fixture.keys.static_public,
            &fixture.keys.intro,
            fixture.addr(),
            now_ms.saturating_sub(age_ms.saturating_add(60_000)),
        );
        assert_eq!(
            fixture.service.install_local_router_info(stale, now_ms),
            Err(Ssu2PublicationUnavailable::InvalidPublication)
        );
        let future = sign_bound_router_info(
            &fixture.keys.bundle,
            &fixture.keys.static_public,
            &fixture.keys.intro,
            fixture.addr(),
            now_ms.saturating_add(skew_ms.saturating_add(60_000)),
        );
        assert_eq!(
            fixture.service.install_local_router_info(future, now_ms),
            Err(Ssu2PublicationUnavailable::InvalidPublication)
        );
        let other = make_path_keys();
        let wrong_identity = sign_bound_router_info(
            &other.bundle,
            &other.static_public,
            &other.intro,
            fixture.addr(),
            now_ms,
        );
        assert_eq!(
            fixture
                .service
                .install_local_router_info(wrong_identity, now_ms),
            Err(Ssu2PublicationUnavailable::InvalidPublication)
        );
        assert_eq!(
            fixture.service.install_local_router_info(
                vec![0_u8; LOCAL_ROUTER_INFO_MAX_ENCODED_LEN + 1],
                now_ms
            ),
            Err(Ssu2PublicationUnavailable::InvalidPublication)
        );
        assert_eq!(fixture.service.local_router_info_generation(), 0);
        let first = sign_bound_router_info(
            &fixture.keys.bundle,
            &fixture.keys.static_public,
            &fixture.keys.intro,
            fixture.addr(),
            now_ms,
        );
        fixture
            .service
            .install_local_router_info(first, now_ms)
            .expect("first install");
        assert_eq!(fixture.service.local_router_info_generation(), 1);
        assert_eq!(fixture.service.snapshot().local_router_info_generation, 1);
        let second = sign_bound_router_info(
            &fixture.keys.bundle,
            &fixture.keys.static_public,
            &fixture.keys.intro,
            fixture.addr(),
            now_ms,
        );
        fixture
            .service
            .install_local_router_info(second.clone(), now_ms)
            .expect("second install");
        assert_eq!(fixture.service.local_router_info_generation(), 2);
        assert_eq!(
            *fixture
                .service
                .shared
                .local_router_info
                .lock()
                .expect("local RouterInfo"),
            second
        );
        shutdown_path_fixture(fixture).await;
    }

    #[tokio::test]
    async fn install_local_router_info_preserves_live_session() {
        let a = start_path_fixture(make_path_keys()).await;
        let b = start_path_fixture(make_path_keys()).await;
        path_dial(&a, &b.keys, b.addr()).await;
        wait_for_snapshot(
            &a.service,
            |snapshot| snapshot.active_sessions == 1,
            "a active",
        )
        .await;
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.active_sessions == 1,
            "b active",
        )
        .await;
        let now_ms = path_wall_secs() * 1000;
        let fresh = sign_bound_router_info(
            &b.keys.bundle,
            &b.keys.static_public,
            &b.keys.intro,
            b.addr(),
            now_ms,
        );
        b.service
            .install_local_router_info(fresh, now_ms)
            .expect("install preserves sessions");
        assert_eq!(b.service.local_router_info_generation(), 1);
        assert_eq!(a.service.snapshot().active_sessions, 1);
        assert_eq!(b.service.snapshot().active_sessions, 1);
        assert_eq!(a.service.snapshot().sessions_established, 1);
        assert_eq!(b.service.snapshot().sessions_established, 1);
        let c = start_path_fixture(make_path_keys()).await;
        path_dial(&c, &b.keys, b.addr()).await;
        wait_for_snapshot(
            &c.service,
            |snapshot| snapshot.active_sessions == 1,
            "c active after rotation",
        )
        .await;
        wait_for_snapshot(
            &b.service,
            |snapshot| snapshot.active_sessions == 2,
            "b second inbound after rotation",
        )
        .await;
        shutdown_path_fixture(a).await;
        shutdown_path_fixture(b).await;
        shutdown_path_fixture(c).await;
    }
    /// Plan 339: the accessor reports the honest baseline for a router that
    /// has bound IPv4 and made no reachability observation at all, and never
    /// leaks the IPv4 family's state onto IPv6.
    #[tokio::test]
    async fn network_condition_reports_configured_and_bound_per_family() {
        let keys = make_path_keys();
        let service = Ssu2RuntimeService::new(
            Ssu2RuntimeConfig::default(),
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("valid runtime");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let handle = service
            .start(
                &scope,
                Ssu2SocketConfig {
                    ipv4: Some("127.0.0.1:0".parse().expect("loopback")),
                    ipv6: None,
                },
            )
            .await
            .expect("bind");
        let condition = service.network_condition().expect("state is readable");

        assert!(condition.v4.configured, "IPv4 was requested");
        assert!(condition.v4.bound, "IPv4 is bound");
        assert_eq!(
            condition.v4.reachability,
            ReachabilityState::Unknown,
            "no observation means no claim"
        );

        assert!(
            !condition.v6.configured && !condition.v6.bound,
            "IPv6 was never requested, so it is not a fault"
        );
        assert_eq!(condition.v6.reachability, ReachabilityState::Unknown);

        assert_eq!(
            i2pr_transport::network_status_code(&condition.v4),
            i2pr_transport::NetworkStatusCode::Unknown,
            "an unobserved family must not become OK"
        );
        assert_eq!(
            i2pr_transport::network_error_code(&condition.v4, true),
            i2pr_transport::NetworkErrorCode::None,
            "a running family is not offline"
        );
        assert_eq!(
            i2pr_transport::network_error_code(&condition.v6, true),
            i2pr_transport::NetworkErrorCode::None,
            "a never-configured family is not offline"
        );
        assert_eq!(i2pr_transport::network_testing_flag(&condition.v4), 0);
        assert_eq!(i2pr_transport::network_testing_flag(&condition.v6), 0);

        assert_eq!(condition.family(AddressFamily::Ipv4), condition.v4);
        assert_eq!(condition.family(AddressFamily::Ipv6), condition.v6);
        assert_eq!(
            condition.family(AddressFamily::Unknown),
            FamilyNetworkCondition::inert(),
            "an unclassified family yields the inert condition"
        );

        drop(handle);
        service.shutdown();
        let report = scope.shutdown().await;
        assert!(report.joined() >= 1, "loop task joined");
    }

    /// Plan 339: an unstarted service reports neither family as configured,
    /// so a composition that never bound a socket cannot claim a status.
    #[test]
    fn network_condition_on_an_unstarted_service_makes_no_claim() {
        let keys = make_path_keys();
        let service = Ssu2RuntimeService::new(
            Ssu2RuntimeConfig::default(),
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("valid unbound runtime");
        let condition = service.network_condition().expect("state readable");
        assert_eq!(condition.v4, FamilyNetworkCondition::inert());
        assert_eq!(condition.v6, FamilyNetworkCondition::inert());
        assert_eq!(
            i2pr_transport::network_status_code(&condition.v4),
            i2pr_transport::NetworkStatusCode::Unknown
        );
    }
}
