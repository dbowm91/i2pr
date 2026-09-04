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
//! (`plans/158-m8-ssu2-udp-runtime-and-local-session-product.md`).
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

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::X25519PrivateKey;
use i2pr_proto::{Hash, RouterInfo};
use i2pr_transport::{
    CandidateDecision, DeliveryRequest, Direction, EncodedI2npMessage, LinkCandidate, LinkId,
    PeerId, PendingHandshake, ResourceClass, TerminationCategory, TransportKind, TransportLimits,
    TransportManager,
};
use i2pr_transport_ssu2::{
    AddressBlock, AuthenticatedSsu2Session, ClockSkewPolicy, ConfirmedParams, DeadlineKind,
    DropCategory, HandshakeAction, HandshakeReplayCache, Initiator, InitiatorConfig,
    InitiatorSecrets, IntroKey, Responder, ResponderConfig, ResponderParams, RetryAnswer,
    SessionAction, SessionConfig, SessionEvent, Ssu2Endpoint, Ssu2PublicKey, Ssu2RouterAddress,
    TerminateReason, TokenStore, constants, parse_session_request, parse_token_request,
    retry_response_budget,
};
use rand_core::{OsRng, TryRngCore};
use tokio::net::UdpSocket;
use tokio::sync::{Notify, mpsc, oneshot};
use zeroize::Zeroize;

use crate::ntcp2_runtime::{DialAdmission, DialBackoffConfig, DialKey};
use crate::ntcp2_runtime::{DialBackoffDecision, IpPrefixPolicy};
use crate::{CancellationToken, ChildScope, ChildTaskFailure};
use i2pr_core::CancellationReason;

/// Hard maximum for any Plan 158 runtime duration.
pub const MAX_SSU2_RUNTIME_DURATION: Duration = Duration::from_secs(3_600);
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
        Ok(self)
    }
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
}

impl Ssu2RuntimeConfig {
    /// Validates and returns this configuration.
    pub fn validate(self) -> Result<Self, Ssu2RuntimeConfigError> {
        self.limits.validate()?;
        self.deadlines.validate()?;
        IpPrefixPolicy::new(self.prefixes.ipv4_prefix, self.prefixes.ipv6_prefix)
            .map_err(|_| Ssu2RuntimeConfigError::InvalidPrefix)?;
        Ok(self)
    }
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
            Self::InvalidIdentity => formatter.write_str("invalid SSU2 local identity material"),
            Self::InvalidDialTarget => formatter.write_str("invalid SSU2 dial target"),
        }
    }
}

impl std::error::Error for Ssu2RuntimeConfigError {}

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
/// must be loopback while non-loopback operation is unsupported, and
/// the peer reference must match the expected router hash.
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
    /// The peer reference does not match the expected router hash.
    PeerHashMismatch,
}

impl fmt::Display for Ssu2DialTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroPort => "SSU2 dial port must be nonzero",
            Self::UnspecifiedIp => "SSU2 dial address must not be unspecified",
            Self::NotLoopback => "SSU2 dial address must be loopback in this milestone",
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
    peer_addr: SocketAddr,
    subnet: SubnetKey,
    session: i2pr_transport_ssu2::Ssu2Session,
    confirmed_resend: Option<ConfirmedResend>,
    data_rx_observed: bool,
}

/// One staged outbound datagram awaiting socket write.
struct StagedDatagram {
    bytes: Vec<u8>,
    addr: SocketAddr,
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
    staged_v4: VecDeque<StagedDatagram>,
    staged_v6: VecDeque<StagedDatagram>,
    staged_bytes: usize,
    fault_transmits: u64,
    fault_held: Option<StagedDatagram>,
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
    local_peer: PeerId,
    local_static: [u8; 32],
    local_intro: IntroKey,
    local_router_info: Vec<u8>,
    local_mtu: u16,
    manager: TransportManager,
    backoff: DialAdmission,
    state: Mutex<ServiceState>,
    notify: Notify,
    shutdown: CancellationToken,
    started_at: tokio::time::Instant,
    faults: Mutex<Option<Ssu2TestFaults>>,
    sockets: Mutex<ServiceSockets>,
    counters: ServiceCounters,
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

/// A started SSU2 service: bound socket addresses plus the bounded
/// authenticated-I2NP handoff receiver.
pub struct Ssu2ServiceHandle {
    service: Ssu2RuntimeService,
    inbound: mpsc::Receiver<Ssu2InboundI2np>,
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

    /// Returns the bound IPv4 socket address, if any.
    pub const fn local_v4(&self) -> Option<SocketAddr> {
        self.local_v4
    }

    /// Returns the bound IPv6 socket address, if any.
    pub const fn local_v6(&self) -> Option<SocketAddr> {
        self.local_v6
    }
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
        config.validate()?;
        if identity.router_info.is_empty()
            || identity.router_info.len() > constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES
        {
            return Err(Ssu2RuntimeConfigError::InvalidIdentity);
        }
        RouterInfo::decode(
            &identity.router_info,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .map_err(|_| Ssu2RuntimeConfigError::InvalidIdentity)?;
        let static_secret = X25519PrivateKey::from_bytes(identity.static_secret_bytes);
        Ssu2PublicKey::new(static_secret.public_bytes())
            .map_err(|_| Ssu2RuntimeConfigError::InvalidIdentity)?;
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
        Ok(Self {
            shared: Arc::new(Shared {
                config,
                local_peer: PeerId::from_hash(identity.router_hash),
                local_static: identity.static_secret_bytes,
                local_intro: identity.intro_key,
                local_router_info: identity.router_info,
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
                    staged_v4: VecDeque::new(),
                    staged_v6: VecDeque::new(),
                    staged_bytes: 0,
                    fault_transmits: 0,
                    fault_held: None,
                }),
                notify: Notify::new(),
                shutdown: CancellationToken::new(),
                started_at: tokio::time::Instant::now(),
                faults: Mutex::new(None),
                sockets: Mutex::new(ServiceSockets::default()),
                counters: ServiceCounters::default(),
            }),
        })
    }

    fn now_ms(&self) -> u64 {
        monotonic_ms(&self.shared.started_at)
    }

    fn service_now(&self) -> Duration {
        self.shared.started_at.elapsed()
    }

    /// Returns the transport manager for teardown assertions in tests.
    pub fn manager(&self) -> &TransportManager {
        &self.shared.manager
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
            scope
                .spawn(move |child| async move {
                    service.run_loop(child, socket, inbound_tx, true).await
                })
                .map_err(|_| Ssu2BindError::Scope)?;
        }
        if let Some((socket, _)) = bound_v6 {
            let service = self.clone();
            scope
                .spawn(move |child| async move {
                    service
                        .run_loop(child, socket, inbound_tx.clone(), false)
                        .await
                })
                .map_err(|_| Ssu2BindError::Scope)?;
        }
        // The original senders are held by the loop tasks; dropping the
        // local clone here would close the channel only after loops exit.
        Ok(Ssu2ServiceHandle {
            service: self.clone(),
            inbound: inbound_rx,
            local_v4,
            local_v6,
        })
    }

    /// Requests service shutdown. Loop tasks drain, release all
    /// handshakes/sessions/manager links, then exit for the scope join.
    pub fn shutdown(&self) {
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
        Ssu2Snapshot {
            cheap_drops: counters.cheap_drops.load(Ordering::Relaxed),
            auth_failures: counters.auth_failures.load(Ordering::Relaxed),
            protocol_drops: counters.protocol_drops.load(Ordering::Relaxed),
            token_rejections: counters.token_rejections.load(Ordering::Relaxed),
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
            pending_outbound,
            pending_inbound,
            active_sessions,
            token_table_entries,
            cached_tokens: cached,
        }
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
            let cached = take_cached_token_locked(&mut state, &target.peer(), now_secs);
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
        let outcome = self.queue_to_session(capability_link_id(&capability), &bytes);
        drop(queued);
        outcome
    }

    fn queue_to_session(&self, link_id: LinkId, bytes: &[u8]) -> Ssu2SendOutcome {
        let Ok(mut state) = self.shared.state.lock() else {
            return Ssu2SendOutcome::Closed;
        };
        let Some(record) = state.active.get_mut(&link_id) else {
            return Ssu2SendOutcome::Closed;
        };
        if record.session.is_terminated() {
            return Ssu2SendOutcome::Closed;
        }
        let (messages, queued_bytes) = record.session.outbound_pending();
        if messages >= SSU2_SEND_QUEUE_MESSAGES
            || queued_bytes.saturating_add(bytes.len()).saturating_add(64) > SSU2_SEND_QUEUE_BYTES
        {
            return Ssu2SendOutcome::QueueFull;
        }
        match record.session.queue_i2np_message(bytes.to_vec()) {
            Ok(_) => {
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

fn remove_peer_link_locked(state: &mut ServiceState, peer: &PeerId, link_id: &LinkId) {
    if let Some(links) = state.peer_links.get_mut(peer) {
        links.remove(link_id);
        if links.is_empty() {
            state.peer_links.remove(peer);
        }
    }
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

fn stage_one_locked(
    state: &mut ServiceState,
    limits: &Ssu2RuntimeLimits,
    counters: &ServiceCounters,
    bytes: Vec<u8>,
    addr: SocketAddr,
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
    let staged = StagedDatagram { bytes, addr };
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
        let _ = stage_one_locked(state, limits, counters, bytes, *addr);
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
        let config = SessionConfig {
            local_conn_id,
            remote_conn_id,
            local_intro: self.shared.local_intro,
            remote_intro,
            initial_send_packet_number: 0,
            max_payload_bytes: SessionConfig::max_payload_for_mtu(self.shared.local_mtu, false),
            idle_timeout_ms: idle_ms,
        };
        let mut session = i2pr_transport_ssu2::Ssu2Session::new(config, auth.into_keys()).ok()?;
        // One spare token for the peer's next handshake, announced
        // in-band. Failures (RNG/table) only forfeit the cached-token
        // fast path; the Retry path always remains.
        let now_secs = wall_secs();
        if !session.is_terminated() {
            let mut token_bytes = [0_u8; 8];
            if fill_random(&mut token_bytes).is_ok()
                && let Ok(token) = state.token_store.issue(peer_addr, now_secs, token_bytes)
            {
                let expires = now_secs
                    .saturating_add(constants::TOKEN_LIFETIME_SECONDS)
                    .min(u64::from(u32::MAX)) as u32;
                let _ = session.queue_new_token(expires, token.value());
            }
        }
        Some(session)
    }

    fn remove_active_locked(&self, state: &mut ServiceState, link_id: &LinkId) {
        if let Some(record) = state.active.remove(link_id) {
            release_active_locked(state, &record);
            remove_peer_link_locked(state, &record.peer, link_id);
            self.shared
                .counters
                .sessions_closed
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn insert_active_locked(&self, state: &mut ServiceState, record: ActiveSession) {
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
        let session =
            match self.construct_session(state, auth, target.responder_intro(), target.address()) {
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
        self.insert_active_locked(
            state,
            ActiveSession {
                link_id,
                peer: target.peer(),
                peer_addr: target.address(),
                subnet: subnet_key(self.shared.config.prefixes, target.address().ip()),
                session,
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
        let session = match self.construct_session(state, auth, remote_intro, source) {
            Some(session) => session,
            None => {
                self.shared
                    .counters
                    .auth_failures
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
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
        self.insert_active_locked(
            state,
            ActiveSession {
                link_id,
                peer,
                peer_addr: source,
                subnet: subnet_key(self.shared.config.prefixes, source.ip()),
                session,
                confirmed_resend: None,
                data_rx_observed: false,
            },
        );
    }
}

impl Ssu2RuntimeService {
    fn responder_config(&self, local_addr: SocketAddr) -> Option<ResponderConfig> {
        let endpoint = Ssu2Endpoint::from_socket_addr(local_addr).ok()?;
        Some(ResponderConfig {
            static_secret: X25519PrivateKey::from_bytes(self.shared.local_static),
            intro_key: self.shared.local_intro,
            expected_peer_hash: None,
            clock: ClockSkewPolicy::handshake(),
            local_mtu: self.shared.local_mtu,
            local_address: AddressBlock::new(endpoint),
        })
    }

    fn count_drop_category(&self, category: DropCategory) {
        let counters = &self.shared.counters;
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
        let confirmed_params = ConfirmedParams {
            router_info: self.shared.local_router_info.clone(),
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
        let Some(local) = local_addr else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Some(config) = self.responder_config(local) else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
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
            entry.machine = Some(machine);
            state.pending_inbound.insert(key, entry);
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
        let Some(local) = local_addr else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Some(config) = self.responder_config(local) else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
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
        let Some(local) = local_addr else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Some(config) = self.responder_config(local) else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
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
            local_conn_id,
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
    fn handle_active_datagram(
        &self,
        state: &mut ServiceState,
        link_id: LinkId,
        bytes: &[u8],
        now_ms: u64,
        now_secs: u64,
        inbound: &mut Vec<Ssu2InboundI2np>,
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
                _ => {}
            }
        }
        if remove.is_none() {
            for _ in 0..SSU2_MAX_DRAIN_PER_SESSION {
                let datagram = match state.active.get_mut(&link_id) {
                    Some(record) => record.session.poll_transmit(now_ms),
                    None => None,
                };
                match datagram {
                    Some(datagram) => {
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
    ) -> Vec<Ssu2InboundI2np> {
        let mut inbound = Vec::new();
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
            return inbound;
        }
        let now_ms = self.now_ms();
        let now_secs = wall_secs();
        let Ok(mut state) = self.shared.state.lock() else {
            self.shared
                .counters
                .cheap_drops
                .fetch_add(1, Ordering::Relaxed);
            return inbound;
        };
        // Active sessions first: side-effect-free trial match.
        let matched: Option<LinkId> = state
            .active
            .iter()
            .find(|(_, record)| record.session.matches_inbound(bytes))
            .map(|(id, _)| *id);
        if let Some(link_id) = matched {
            self.handle_active_datagram(&mut state, link_id, bytes, now_ms, now_secs, &mut inbound);
            return inbound;
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
            return inbound;
        }
        // Pending inbound handshakes awaiting SessionConfirmed.
        if self.drive_pending_confirmed(&mut state, bytes.to_vec(), source, now_ms, now_secs) {
            return inbound;
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
            return inbound;
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
            return inbound;
        }
        self.shared
            .counters
            .cheap_drops
            .fetch_add(1, Ordering::Relaxed);
        inbound
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

    /// Drives all time-based work under one lock: handshake resends
    /// and backstops, cached-token fallback, session polls, Confirmed
    /// resends, and token-table expiry. Returns handoff messages for
    /// delivery after the lock releases.
    fn drive_timeouts_locked(&self, state: &mut ServiceState) -> Vec<Ssu2InboundI2np> {
        let inbound = Vec::new();
        let now_ms = monotonic_ms(&self.shared.started_at);
        let now_secs = wall_secs();
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
                        Some(record) => record.session.poll_transmit(now_ms),
                        None => None,
                    };
                    match datagram {
                        Some(datagram) => {
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
                        None => break,
                    }
                }
            }
            if let Some(reason) = remove {
                self.remove_active_locked(state, &link_id);
                let _ = self.shared.manager.close_link(link_id, reason);
            }
        }
        state.token_store.expire(now_secs);
        let _ = now_ms;
        inbound
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
    async fn flush_staged(&self, socket: &UdpSocket, is_v4: bool) {
        struct Send {
            bytes: Vec<u8>,
            addr: SocketAddr,
            duplicates: bool,
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
                    continue;
                }
                let duplicates = policy
                    .as_ref()
                    .is_some_and(|policy| policy.duplicate_transmit.contains(&index));
                sends.push(Send {
                    bytes: datagram.bytes,
                    addr: datagram.addr,
                    duplicates,
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
                    });
                } else {
                    // Batched datagrams: the first joins at the tail.
                    sends.push(Send {
                        bytes: first.bytes,
                        addr: first.addr,
                        duplicates: false,
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
                if socket.send_to(&send.bytes, send.addr).await.is_ok() {
                    self.shared
                        .counters
                        .datagrams_sent
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        if let Some(previous) = previous_held {
            // Released one position behind its successor (or merely
            // delayed when the queue was empty).
            if socket.send_to(&previous.bytes, previous.addr).await.is_ok() {
                self.shared
                    .counters
                    .datagrams_sent
                    .fetch_add(1, Ordering::Relaxed);
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
    async fn run_loop(
        &self,
        child: CancellationToken,
        socket: Arc<UdpSocket>,
        inbound_tx: mpsc::Sender<Ssu2InboundI2np>,
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
                    self.deliver_inbound(inbound, &inbound_tx);
                    self.flush_staged(&socket, is_v4).await;
                }
                result = socket.recv_from(&mut buffer) => {
                    match result {
                        Ok((len, source)) => {
                            self.shared.counters.datagrams_received.fetch_add(1, Ordering::Relaxed);
                            let inbound = self.handle_datagram(&buffer[..len], source, is_v4, local_addr);
                            self.deliver_inbound(inbound, &inbound_tx);
                            self.flush_staged(&socket, is_v4).await;
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
                    self.deliver_inbound(inbound, &inbound_tx);
                    self.flush_staged(&socket, is_v4).await;
                }
            }
        }
    }

    fn deliver_inbound(
        &self,
        inbound: Vec<Ssu2InboundI2np>,
        inbound_tx: &mpsc::Sender<Ssu2InboundI2np>,
    ) {
        for message in inbound {
            if inbound_tx.try_send(message).is_err() {
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
}
