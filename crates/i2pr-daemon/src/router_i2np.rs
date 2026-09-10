//! Plan 184 authenticated router-I2NP dispatch spine.
//!
//! The daemon-owned SSU2 runtime delivers authenticated inbound I2NP
//! through [`i2pr_runtime::Ssu2InboundI2np`]. This module owns the
//! single central dispatcher above the existing
//! [`crate::inbound_dispatch`] tunnel-payload helper plus the narrow
//! outbound router-delivery capability used by future tunnel/NetDB
//! plans.
//!
//! ```text
//! Ssu2InboundI2np { authenticated peer/link metadata, encoded message }
//!  -> bounded standard/short I2NP decode
//!  -> expiration/size/type validation
//!  -> typed dispatch outcome
//! ```
//!
//! Properties:
//!
//! - the authenticated peer/link identity travels with every outcome;
//!   no caller-supplied peer identity enters the live path;
//! - standard (16-byte) and short-transport (9-byte) headers are both
//!   accepted and explicitly classified; the obsolete 5-byte short-SSU
//!   header is rejected boundedly;
//! - expiration is validated against the supplied wall clock with a
//!   bounded future horizon; size is bounded by the transport
//!   boundary; unknown/unsupported bodies receive an explicit bounded
//!   disposition and never panic or retain unbounded state;
//! - outbound delivery resolves an already-established authenticated
//!   link through the existing `Ssu2RuntimeService::send_i2np` path
//!   with no task per delivery and no global router context leaking
//!   into tunnel/NetDB/client crates.
//!
//! Initial destinations:
//!
//! - `ShortTunnelBuild` / `OutboundTunnelBuildReply`: typed hook
//!   reserved for Plan 185 (no tunnel-build claim in Plan 184);
//! - `TunnelData`: typed hook into the existing data-plane
//!   registry/`inbound_dispatch` seam (Plan 185 owns the live cells);
//! - `DatabaseStore` / `DatabaseLookup` / `DatabaseSearchReply` /
//!   `DeliveryStatus`: typed router-control/NetDB hooks (Plan 186
//!   owns live NetDB exchange);
//! - every other known body or unknown type byte: bounded explicit
//!   `Unsupported` disposition.
//!
//! `inbound_dispatch.rs` stays focused on recovered tunnel payloads;
//! this dispatcher sits above it and never duplicates its NetDB
//! normalization logic.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::time::Duration;

use i2pr_crypto::{OsRng, RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{
    Date, Hash, I2npBody, I2npHeader, I2npMessage, Mapping, MessageType, RouterAddress,
};
use i2pr_runtime::{
    CancellationToken, ChildScope, IntroKey, Ssu2DialTarget, Ssu2EstablishedLink,
    Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RouterAddress, Ssu2RuntimeConfig,
    Ssu2RuntimeDeadlines, Ssu2RuntimeLimits, Ssu2RuntimeService, Ssu2SendOutcome,
    Ssu2ServiceHandle, Ssu2SocketConfig, constants,
};
use i2pr_transport::{EncodedI2npMessage, LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId};
use rand_core::TryRngCore;
use thiserror::Error;

use crate::config::Ssu2Config;

/// Largest encoded router-I2NP message accepted by the dispatcher.
///
/// Matches the transport boundary so no second buffering policy can
/// diverge from `send_i2np` admission.
pub const MAX_ROUTER_I2NP_BYTES: usize = MAX_I2NP_MESSAGE_BYTES;
/// Bounded future horizon for router-I2NP expiration.
///
/// Inbound expirations beyond `now + horizon` are rejected as
/// `FarFuture` rather than queued. The horizon comfortably covers the
/// controlled-loopback control exchange (driver uses `now + 60 s`)
/// while rejecting tunnel-style multi-hour horizons that never belong
/// on the direct router link.
pub const MAX_ROUTER_I2NP_FUTURE_MS: u64 = 15 * 60 * 1000;
/// Maximum outbound router-delivery timeout accepted by the narrow
/// capability. Keeps one delivery bounded without a per-delivery task.
pub const MAX_ROUTER_DELIVERY_TIMEOUT: Duration = Duration::from_secs(60);
/// Minimum outbound router-delivery timeout (nonzero).
pub const MIN_ROUTER_DELIVERY_TIMEOUT: Duration = Duration::from_millis(1);

/// Which I2NP header variant carried the dispatched message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterI2npHeaderKind {
    /// 16-byte standard header with millisecond expiration.
    Standard,
    /// 9-byte NTCP2/SSU2 short-transport header with seconds expiration.
    ShortTransport,
}

/// Typed router-I2NP body classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterI2npKind {
    /// Short tunnel-build request (Plan 185 reserved).
    ShortTunnelBuild,
    /// Short tunnel-build reply (Plan 185 reserved).
    OutboundTunnelBuildReply,
    /// Tunnel data cell (Plan 185 live path).
    TunnelData,
    /// RouterInfo/LeaseSet store (Plan 186 live path).
    DatabaseStore,
    /// RouterInfo lookup (Plan 186 live path).
    DatabaseLookup,
    /// NetDB search reply (Plan 186 live path).
    DatabaseSearchReply,
    /// Delivery confirmation (preflight observable control).
    DeliveryStatus,
}

/// Typed outcome of one authenticated router-I2NP dispatch.
///
/// Every variant preserves the authenticated peer/link identity from
/// the live runtime handoff. Lengths are reported, never payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterI2npOutcome {
    /// Short-build request/reply reserved for the Plan 185 tunnel
    /// coordinator. No build is attempted in Plan 184.
    TunnelBuildReserved {
        /// Which reserved build body arrived.
        kind: RouterI2npKind,
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// TunnelData cell for the existing data-plane registry seam.
    TunnelData {
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Tunnel identifier carried by the cell body.
        tunnel_id: u32,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// Direct router-control/NetDB message for future hooks.
    RouterControl {
        /// Which control body arrived.
        kind: RouterI2npKind,
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// Bounded explicit disposition for unsupported/unknown bodies.
    /// Never a panic, never unbounded retention.
    Unsupported {
        /// I2NP type byte of the rejected body.
        type_byte: u8,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the rejected message.
        encoded_len: usize,
    },
}

impl RouterI2npOutcome {
    /// Returns the authenticated peer preserved by this outcome.
    pub const fn peer(self) -> PeerId {
        match self {
            Self::TunnelBuildReserved { peer, .. }
            | Self::TunnelData { peer, .. }
            | Self::RouterControl { peer, .. }
            | Self::Unsupported { peer, .. } => peer,
        }
    }

    /// Returns the exact link preserved by this outcome.
    pub const fn link_id(self) -> LinkId {
        match self {
            Self::TunnelBuildReserved { link_id, .. }
            | Self::TunnelData { link_id, .. }
            | Self::RouterControl { link_id, .. }
            | Self::Unsupported { link_id, .. } => link_id,
        }
    }

    /// Returns the encoded length carried by this outcome.
    pub const fn encoded_len(self) -> usize {
        match self {
            Self::TunnelBuildReserved { encoded_len, .. }
            | Self::TunnelData { encoded_len, .. }
            | Self::RouterControl { encoded_len, .. }
            | Self::Unsupported { encoded_len, .. } => encoded_len,
        }
    }
}

/// Bounded dispatcher failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RouterI2npError {
    /// No bytes were supplied.
    #[error("router I2NP message must not be empty")]
    Empty,
    /// The encoded message exceeds the transport boundary.
    #[error("router I2NP message exceeds its bound")]
    TooLarge {
        /// Supplied encoded length.
        actual: usize,
        /// Transport boundary maximum.
        maximum: usize,
    },
    /// Neither the standard nor the short-transport decoder accepted
    /// the bytes.
    #[error("router I2NP decoding failed: {0}")]
    Codec(String),
    /// The message expiration is in the past or undefined.
    #[error("router I2NP message is expired")]
    Expired,
    /// The message expiration exceeds the bounded future horizon.
    #[error("router I2NP expiration exceeds its horizon")]
    FarFuture,
}

/// Dispatches one authenticated inbound I2NP message.
///
/// The caller passes the live [`i2pr_runtime::Ssu2InboundI2np`]
/// handoff by reference so the authenticated peer/link identity
/// cannot be substituted. `now_ms` is wall-clock milliseconds since
/// the Unix epoch. The function decodes standard first, then
/// short-transport, validates expiration/size/type, and returns the
/// typed outcome. Unknown type bytes and known-but-unsupported bodies
/// return `Ok(Unsupported)`; only malformed/expired/oversized inputs
/// return `Err`.
pub fn dispatch_router_i2np(
    inbound: &i2pr_runtime::Ssu2InboundI2np,
    now_ms: u64,
) -> Result<RouterI2npOutcome, RouterI2npError> {
    let bytes = inbound.bytes.as_slice();
    if bytes.is_empty() {
        return Err(RouterI2npError::Empty);
    }
    if bytes.len() > MAX_ROUTER_I2NP_BYTES {
        return Err(RouterI2npError::TooLarge {
            actual: bytes.len(),
            maximum: MAX_ROUTER_I2NP_BYTES,
        });
    }
    // Unknown first-byte types receive the bounded unsupported
    // disposition without entering either decoder.
    let first = bytes[0];
    if matches!(MessageType::from_code(first), MessageType::Unknown(_)) {
        return Ok(RouterI2npOutcome::Unsupported {
            type_byte: first,
            peer: inbound.peer,
            link_id: inbound.link_id,
            encoded_len: bytes.len(),
        });
    }
    let standard = I2npMessage::decode_standard(bytes, MAX_ROUTER_I2NP_BYTES);
    let (message, header_kind) = match standard {
        Ok(message) => (message, RouterI2npHeaderKind::Standard),
        Err(first_err) => match I2npMessage::decode_short_transport(bytes, MAX_ROUTER_I2NP_BYTES) {
            Ok(message) => (message, RouterI2npHeaderKind::ShortTransport),
            Err(_) => {
                return Err(RouterI2npError::Codec(format!("{first_err:?}")));
            }
        },
    };
    let (message_id, expiration_ms) = match message.header() {
        I2npHeader::Standard {
            message_id,
            expiration,
            ..
        } => (message_id, expiration.as_millis()),
        I2npHeader::ShortTransport {
            message_id,
            expiration_seconds,
            ..
        } => (
            message_id,
            u64::from(expiration_seconds).saturating_mul(1000),
        ),
        I2npHeader::ShortSsu { .. } => {
            return Err(RouterI2npError::Codec(
                "obsolete short-SSU header is not accepted on the router link".to_owned(),
            ));
        }
    };
    if expiration_ms == 0 || expiration_ms <= now_ms {
        return Err(RouterI2npError::Expired);
    }
    if expiration_ms.saturating_sub(now_ms) > MAX_ROUTER_I2NP_FUTURE_MS {
        return Err(RouterI2npError::FarFuture);
    }
    classify_message(
        &message,
        header_kind,
        message_id,
        expiration_ms,
        inbound.peer,
        inbound.link_id,
        bytes.len(),
    )
}

fn classify_message(
    message: &I2npMessage,
    header: RouterI2npHeaderKind,
    message_id: u32,
    expiration_ms: u64,
    peer: PeerId,
    link_id: LinkId,
    encoded_len: usize,
) -> Result<RouterI2npOutcome, RouterI2npError> {
    match message.body() {
        I2npBody::ShortTunnelBuild(_) => Ok(RouterI2npOutcome::TunnelBuildReserved {
            kind: RouterI2npKind::ShortTunnelBuild,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::OutboundTunnelBuildReply(_) => Ok(RouterI2npOutcome::TunnelBuildReserved {
            kind: RouterI2npKind::OutboundTunnelBuildReply,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::TunnelData(cell) => Ok(RouterI2npOutcome::TunnelData {
            header,
            message_id,
            expiration_ms,
            tunnel_id: cell.tunnel_id,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DatabaseStore(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DatabaseStore,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DatabaseLookup(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DatabaseLookup,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DatabaseSearchReply(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DatabaseSearchReply,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DeliveryStatus(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DeliveryStatus,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        other => Ok(RouterI2npOutcome::Unsupported {
            type_byte: other.message_type().code(),
            peer,
            link_id,
            encoded_len,
        }),
    }
}

/// Bounded outbound router-delivery request.
#[derive(Debug)]
pub struct RouterDeliveryRequest {
    peer: PeerId,
    message: EncodedI2npMessage,
    timeout: Duration,
}

impl RouterDeliveryRequest {
    /// Validates one outbound delivery without touching a socket.
    pub fn new(
        peer: PeerId,
        bytes: Vec<u8>,
        timeout: Duration,
    ) -> Result<Self, RouterDeliveryError> {
        if timeout < MIN_ROUTER_DELIVERY_TIMEOUT {
            return Err(RouterDeliveryError::ZeroTimeout);
        }
        if timeout > MAX_ROUTER_DELIVERY_TIMEOUT {
            return Err(RouterDeliveryError::TimeoutTooLong);
        }
        let message =
            EncodedI2npMessage::new(bytes).map_err(|_| RouterDeliveryError::MessageTooLarge)?;
        Ok(Self {
            peer,
            message,
            timeout,
        })
    }

    /// Returns the target peer reference.
    pub fn peer(&self) -> PeerId {
        self.peer
    }

    /// Returns the delivery timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Validation failures for an outbound router-delivery request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum RouterDeliveryError {
    /// The encoded message was empty or exceeds the transport boundary.
    #[error("router delivery message exceeds its bound")]
    MessageTooLarge,
    /// The timeout was zero.
    #[error("router delivery timeout must be nonzero")]
    ZeroTimeout,
    /// The timeout exceeds the bounded delivery horizon.
    #[error("router delivery timeout exceeds its horizon")]
    TimeoutTooLong,
}

/// Typed outcome of one outbound router delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterDeliveryOutcome {
    /// The message entered the bounded session outbound queue.
    Accepted,
    /// The session or manager queue is full.
    QueueFull,
    /// A shared resource budget denied admission.
    ResourceDenied,
    /// No active authenticated session exists for the peer.
    NoActiveSession,
    /// The encoded message exceeds the transport boundary.
    TooLarge,
    /// The caller deadline had already elapsed.
    DeadlineElapsed,
    /// The caller cancelled before admission.
    Cancelled,
}

/// Narrow transport-neutral outbound capability.
///
/// The owner holds one daemon-owned [`Ssu2RuntimeService`] clone and
/// resolves already-established peer links through the existing
/// `send_i2np` seam. No task is spawned per delivery; bounded queue
/// and manager ceilings from the runtime remain authoritative. The
/// type lives in the daemon so tunnel/NetDB/client crates never
/// receive a global router context.
#[derive(Clone, Debug)]
pub struct RouterDeliveryService {
    service: Ssu2RuntimeService,
}

impl RouterDeliveryService {
    /// Wraps the daemon-owned runtime service.
    pub const fn new(service: Ssu2RuntimeService) -> Self {
        Self { service }
    }

    /// Delivers one validated request to its established session.
    ///
    /// Cancellation is checked before admission; no delivery task is
    /// spawned. Unknown/not-established peers report
    /// `NoActiveSession` explicitly.
    pub fn deliver(
        &self,
        request: RouterDeliveryRequest,
        cancellation: &CancellationToken,
    ) -> RouterDeliveryOutcome {
        if cancellation.is_cancelled() {
            return RouterDeliveryOutcome::Cancelled;
        }
        let RouterDeliveryRequest {
            peer,
            message,
            timeout,
        } = request;
        match self.service.send_i2np(peer, message, timeout) {
            Ssu2SendOutcome::Accepted => RouterDeliveryOutcome::Accepted,
            Ssu2SendOutcome::QueueFull => RouterDeliveryOutcome::QueueFull,
            Ssu2SendOutcome::ResourceDenied => RouterDeliveryOutcome::ResourceDenied,
            Ssu2SendOutcome::Closed => RouterDeliveryOutcome::NoActiveSession,
            Ssu2SendOutcome::TooLarge => RouterDeliveryOutcome::TooLarge,
            Ssu2SendOutcome::DeadlineElapsed => RouterDeliveryOutcome::DeadlineElapsed,
        }
    }
}

/// Failures constructing or starting the daemon-owned SSU2 service.
#[derive(Debug, Error)]
pub enum Ssu2ServiceError {
    /// The `[ssu2]` surface violates the strict controlled profile.
    #[error("SSU2 controlled profile rejected: {0}")]
    ControlledProfile(String),
    /// Local identity material was rejected.
    #[error("SSU2 identity material is invalid")]
    InvalidIdentity,
    /// The runtime configuration mapping failed validation.
    #[error("SSU2 runtime configuration is invalid: {0}")]
    RuntimeConfig(String),
    /// No loopback socket family was available.
    #[error("SSU2 service has no loopback socket to bind")]
    NoSocket,
    /// The OS rejected the UDP bind.
    #[error("SSU2 socket bind failed")]
    Bind,
    /// Shared service state was unavailable.
    #[error("SSU2 service state unavailable")]
    State,
    /// The child scope could not retain the loop task.
    #[error("SSU2 loop task rejected by scope")]
    Scope,
    /// The persistent router identity could not be loaded.
    #[error("SSU2 router identity unavailable: {0}")]
    Identity(String),
}

impl From<i2pr_runtime::Ssu2BindError> for Ssu2ServiceError {
    fn from(error: i2pr_runtime::Ssu2BindError) -> Self {
        match error {
            i2pr_runtime::Ssu2BindError::NoSocket => Self::NoSocket,
            i2pr_runtime::Ssu2BindError::Bind => Self::Bind,
            i2pr_runtime::Ssu2BindError::State => Self::State,
            i2pr_runtime::Ssu2BindError::Scope => Self::Scope,
        }
    }
}

/// Maps the daemon `[ssu2]` surface to the runtime policy.
///
/// Daemon ceilings stay authoritative; runtime-only fields use the
/// bounded runtime defaults clamped to the daemon ceilings so both
/// bounds hold without double buffering.
pub fn ssu2_runtime_config_from(
    config: &Ssu2Config,
) -> Result<Ssu2RuntimeConfig, Ssu2ServiceError> {
    let default_limits = Ssu2RuntimeLimits::default();
    let default_deadlines = Ssu2RuntimeDeadlines::default();
    let max_active_per_ip = default_limits
        .max_active_per_ip
        .min(config.max_active_sessions);
    let max_active_per_subnet = default_limits
        .max_active_per_subnet
        .min(config.max_active_sessions);
    let dial = default_deadlines.dial.max(config.handshake_timeout);
    let runtime = Ssu2RuntimeConfig {
        limits: Ssu2RuntimeLimits {
            max_pending_handshakes: config.max_pending_handshakes,
            max_pending_per_ip: config.max_pending_per_ip,
            max_pending_per_subnet: config.max_pending_per_subnet,
            max_active_sessions: config.max_active_sessions,
            max_active_per_ip: max_active_per_ip.max(1),
            max_active_per_subnet: max_active_per_subnet.max(1),
            max_outbound_datagrams: config.max_datagram_queue_items,
            max_outbound_bytes: config.max_datagram_queue_bytes,
            max_inbound_i2np_queue: config.max_inbound_i2np_queue,
        },
        deadlines: Ssu2RuntimeDeadlines {
            handshake: config.handshake_timeout,
            dial,
            idle: config.idle_timeout,
            queue_wait: default_deadlines.queue_wait,
            drain: default_deadlines.drain,
            scheduler_poll_max: config.scheduler_poll_max,
        },
        prefixes: i2pr_runtime::IpPrefixPolicy::default(),
    };
    runtime
        .validate()
        .map_err(|error| Ssu2ServiceError::RuntimeConfig(format!("{error}")))
}

/// Builds the loopback socket selection for the daemon-owned service.
///
/// Both families are re-validated fail-closed here so a future config
/// bypass cannot reach the socket layer. At least one loopback family
/// must be present; `port` comes from the daemon surface (`0` selects
/// an ephemeral port for tests).
pub fn ssu2_socket_config_from(config: &Ssu2Config) -> Result<Ssu2SocketConfig, Ssu2ServiceError> {
    if !config.enabled {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 service requires enabled = true".to_owned(),
        ));
    }
    if config.advertise || config.introducer_service {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 controlled profile forbids advertise/introducer".to_owned(),
        ));
    }
    let ipv4 = match config.bind_ipv4 {
        Some(ip) => {
            if !ip.is_loopback() {
                return Err(Ssu2ServiceError::ControlledProfile(
                    "SSU2 IPv4 bind must be loopback".to_owned(),
                ));
            }
            Some(SocketAddr::new(ip, config.port))
        }
        None => None,
    };
    let ipv6 = match config.bind_ipv6 {
        Some(ip) => {
            if !ip.is_loopback() {
                return Err(Ssu2ServiceError::ControlledProfile(
                    "SSU2 IPv6 bind must be loopback".to_owned(),
                ));
            }
            Some(SocketAddr::new(ip, config.port))
        }
        None => None,
    };
    if ipv4.is_none() && ipv6.is_none() {
        return Err(Ssu2ServiceError::NoSocket);
    }
    Ok(Ssu2SocketConfig { ipv4, ipv6 })
}

/// I2P Base64 alphabet (`A-Za-z0-9-~`, `=` padding) for controlled
/// RouterInfo construction. Matches the Plan 142 SAM correction.
fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
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

fn wall_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

/// Builds controlled loopback SSU2 identity material for one daemon
/// service instance.
///
/// The caller supplies the persistent router bundle (loaded from the
/// daemon data directory). Static/intro secrets come from the OS
/// CSPRNG; the signed RouterInfo carries exactly one loopback SSU2
/// address for the supplied host/port with the controlled
/// `router.version`/`netId` options the pinned reference requires
/// for ingest. No secret material appears in diagnostics. The address
/// is loopback-only and `advertise = false` stays mandatory, so the
/// record is never a public advertisement.
pub fn generate_controlled_identity(
    bundle: &RouterIdentityBundle,
    host: &str,
    port: u16,
) -> Result<Ssu2IdentityMaterial, Ssu2ServiceError> {
    if host != "127.0.0.1" && host != "::1" {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 controlled identity requires a loopback host".to_owned(),
        ));
    }
    if port == 0 {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 controlled identity requires a nonzero port".to_owned(),
        ));
    }
    let hash = bundle
        .identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let static_key =
        X25519PrivateKey::generate(&mut OsRng).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let static_bytes = *static_key.secret_bytes();
    let mut intro_bytes = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut intro_bytes)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    if intro_bytes.iter().all(|byte| *byte == 0) {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    let intro = IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), host.to_string()),
        ("port".to_string(), port.to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let ri_options = Mapping::from_entries(vec![
        ("router.version".to_string(), "0.9.58".to_string()),
        ("netId".to_string(), "2".to_string()),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            ri_options,
        )
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let router_info = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    // Re-validate through the standard decoder so a signing defect
    // fails closed before any socket opens.
    i2pr_proto::RouterInfo::decode(&router_info, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    Ok(Ssu2IdentityMaterial {
        router_hash: hash,
        static_secret_bytes: static_bytes,
        intro_key: intro,
        router_info,
    })
}

/// Verifies one out-of-band reference RouterInfo without importing
/// secrets.
///
/// Only public signed bytes cross this boundary; the caller retains
/// the file path for provenance. The decoded record must carry at
/// least one SSU2 address or the preflight fails closed.
pub fn verify_reference_router_info(
    bytes: &[u8],
) -> Result<(Hash, Ssu2RouterAddress), Ssu2ServiceError> {
    let info =
        i2pr_proto::RouterInfo::decode(bytes, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let hash = info
        .router_identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    for address in info.addresses() {
        if address.transport_style() == "SSU2" {
            let parsed =
                Ssu2RouterAddress::parse(address).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
            return Ok((hash, parsed));
        }
    }
    Err(Ssu2ServiceError::InvalidIdentity)
}

/// One daemon-owned SSU2 service instance.
///
/// Construction validates the strict profile and maps daemon ceilings
/// to the runtime policy; [`Ssu2DaemonService::start`] binds loopback
/// UDP sockets under the caller-owned [`ChildScope`]. The handle is
/// the only runtime owner in counted tests: no hidden standalone
/// runtime coexists with it.
#[derive(Clone, Debug)]
pub struct Ssu2DaemonService {
    service: Ssu2RuntimeService,
}

impl Ssu2DaemonService {
    /// Creates the daemon-owned runtime without opening a socket.
    pub fn new(
        config: &Ssu2Config,
        identity: Ssu2IdentityMaterial,
    ) -> Result<Self, Ssu2ServiceError> {
        if !config.enabled {
            return Err(Ssu2ServiceError::ControlledProfile(
                "SSU2 service requires enabled = true".to_owned(),
            ));
        }
        if config.advertise || config.introducer_service {
            return Err(Ssu2ServiceError::ControlledProfile(
                "SSU2 controlled profile forbids advertise/introducer".to_owned(),
            ));
        }
        if config.bind_ipv4.is_none() && config.bind_ipv6.is_none() {
            return Err(Ssu2ServiceError::NoSocket);
        }
        for bind in [config.bind_ipv4, config.bind_ipv6].into_iter().flatten() {
            if !bind.is_loopback() {
                return Err(Ssu2ServiceError::ControlledProfile(
                    "SSU2 bind must be loopback".to_owned(),
                ));
            }
        }
        let runtime = ssu2_runtime_config_from(config)?;
        let service = Ssu2RuntimeService::new(runtime, identity)
            .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
        Ok(Self { service })
    }

    /// Returns the owned runtime for dial/send/snapshot calls.
    pub const fn service(&self) -> &Ssu2RuntimeService {
        &self.service
    }

    /// Returns the narrow outbound delivery capability.
    pub fn delivery(&self) -> RouterDeliveryService {
        RouterDeliveryService::new(self.service.clone())
    }

    /// Binds loopback sockets and starts supervised loop tasks.
    pub async fn start(
        &self,
        scope: &ChildScope,
        config: &Ssu2Config,
    ) -> Result<Ssu2DaemonHandle, Ssu2ServiceError> {
        let sockets = ssu2_socket_config_from(config)?;
        let handle = self.service.start(scope, sockets).await?;
        Ok(Ssu2DaemonHandle {
            service: self.service.clone(),
            handle,
            delivery: self.delivery(),
        })
    }
}

/// A started daemon-owned SSU2 service: bound addresses plus the
/// bounded authenticated-I2NP handoff and delivery seams.
pub struct Ssu2DaemonHandle {
    service: Ssu2RuntimeService,
    handle: Ssu2ServiceHandle,
    delivery: RouterDeliveryService,
}

impl std::fmt::Debug for Ssu2DaemonHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Ssu2DaemonHandle(..)")
    }
}

impl Ssu2DaemonHandle {
    /// Returns the owning runtime service.
    pub const fn service(&self) -> &Ssu2RuntimeService {
        &self.service
    }

    /// Returns the narrow outbound delivery capability.
    pub const fn delivery(&self) -> &RouterDeliveryService {
        &self.delivery
    }

    /// Returns the bound IPv4 socket address, if any.
    pub const fn local_v4(&self) -> Option<SocketAddr> {
        self.handle.local_v4()
    }

    /// Returns the bound IPv6 socket address, if any.
    pub const fn local_v6(&self) -> Option<SocketAddr> {
        self.handle.local_v6()
    }

    /// Receives the next authenticated inbound I2NP message, or `None`
    /// after service shutdown drains the handoff queue.
    pub async fn next_inbound(&mut self) -> Option<i2pr_runtime::Ssu2InboundI2np> {
        self.handle.next_inbound().await
    }

    /// Receives the next inbound message and dispatches it through
    /// the central router-I2NP dispatcher, preserving authenticated
    /// peer identity. Returns `None` after shutdown drains the queue.
    pub async fn next_dispatched(
        &mut self,
        now_ms: u64,
    ) -> Option<Result<RouterI2npOutcome, RouterI2npError>> {
        let inbound = self.handle.next_inbound().await?;
        Some(dispatch_router_i2np(&inbound, now_ms))
    }

    /// Dials one validated SSU2 target to an authenticated session.
    pub async fn dial(
        &self,
        target: Ssu2DialTarget,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<Ssu2EstablishedLink, i2pr_runtime::Ssu2DialOutcome> {
        self.service.dial_ssu2(target, timeout, cancellation).await
    }

    /// Requests service shutdown. Loop tasks drain, release all
    /// handshakes/sessions/manager links, then exit for scope join.
    pub fn shutdown(&self) {
        self.service.shutdown();
    }

    /// Returns the privacy-safe runtime snapshot.
    pub fn snapshot(&self) -> i2pr_runtime::Ssu2Snapshot {
        self.service.snapshot()
    }
}

/// Builds the authenticated dial target for one verified reference
/// peer at its out-of-band loopback endpoint.
///
/// The peer hash must match the RouterInfo-derived hash; the address
/// must be loopback with a nonzero port. Returns the typed target or
/// a controlled-profile rejection without touching a socket.
pub fn daemon_dial_target(
    peer_hash: Hash,
    address: SocketAddr,
    responder_static: Ssu2PublicKey,
    responder_intro: IntroKey,
) -> Result<Ssu2DialTarget, Ssu2ServiceError> {
    let peer = PeerId::from_hash(peer_hash);
    Ssu2DialTarget::new(peer, peer_hash, address, responder_static, responder_intro)
        .map_err(|_| Ssu2ServiceError::ControlledProfile("invalid dial target".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{
        DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload, DeliveryStatusMessage,
        I2npBody, I2npMessage, OpaqueMessageBody,
    };
    use i2pr_transport::PeerId;

    const NOW_MS: u64 = 1_700_000_000_000;

    fn test_peer() -> (PeerId, LinkId) {
        (
            PeerId::from_hash(Hash::from_bytes([0x11; 32])),
            LinkId::new(7).expect("link"),
        )
    }

    fn inbound_with(bytes: Vec<u8>) -> i2pr_runtime::Ssu2InboundI2np {
        let (peer, link_id) = test_peer();
        i2pr_runtime::Ssu2InboundI2np {
            link_id,
            peer,
            bytes,
        }
    }

    fn delivery_status_short(message_id: u32, expiration_secs: u32) -> Vec<u8> {
        let body = I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            0x51A4_2001,
            Date::from_millis(NOW_MS),
        ));
        I2npMessage::new_short_transport(message_id, expiration_secs, body)
            .expect("message")
            .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode")
    }

    fn delivery_status_standard(message_id: u32, expiration_ms: u64) -> Vec<u8> {
        let body = I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            0x51A4_2001,
            Date::from_millis(NOW_MS),
        ));
        I2npMessage::new_standard(message_id, Date::from_millis(expiration_ms), body)
            .expect("message")
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode")
    }

    fn now_secs() -> u64 {
        NOW_MS / 1000
    }

    #[test]
    fn dispatcher_classifies_standard_vs_short_delivery_status() {
        let short_exp = now_secs() as u32 + 60;
        let short_bytes = delivery_status_short(0xA1, short_exp);
        let outcome =
            dispatch_router_i2np(&inbound_with(short_bytes), NOW_MS).expect("short dispatches");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DeliveryStatus,
                header: RouterI2npHeaderKind::ShortTransport,
                ..
            }
        ));

        let standard_bytes = delivery_status_standard(0xA2, NOW_MS + 60_000);
        let outcome = dispatch_router_i2np(&inbound_with(standard_bytes), NOW_MS)
            .expect("standard dispatches");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DatabaseStore | RouterI2npKind::DeliveryStatus,
                header: RouterI2npHeaderKind::Standard,
                ..
            }
        ));
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DeliveryStatus,
                ..
            }
        ));
    }

    #[test]
    fn dispatcher_preserves_authenticated_peer_identity() {
        let short_exp = now_secs() as u32 + 60;
        let bytes = delivery_status_short(0xB1, short_exp);
        let (peer, link) = test_peer();
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("dispatch");
        assert_eq!(outcome.peer(), peer);
        assert_eq!(outcome.link_id(), link);
    }

    #[test]
    fn dispatcher_rejects_expired_and_far_future() {
        // Expired: standard expiration at now.
        let expired = delivery_status_standard(0xC1, NOW_MS);
        assert_eq!(
            dispatch_router_i2np(&inbound_with(expired), NOW_MS),
            Err(RouterI2npError::Expired)
        );
        // Far future: standard expiration beyond the horizon.
        let far = delivery_status_standard(0xC2, NOW_MS + MAX_ROUTER_I2NP_FUTURE_MS + 1000);
        assert_eq!(
            dispatch_router_i2np(&inbound_with(far), NOW_MS),
            Err(RouterI2npError::FarFuture)
        );
        // Expired short-transport (seconds in the past).
        let past_secs = now_secs() as u32 - 10;
        let past = delivery_status_short(0xC3, past_secs);
        assert_eq!(
            dispatch_router_i2np(&inbound_with(past), NOW_MS),
            Err(RouterI2npError::Expired)
        );
    }

    #[test]
    fn dispatcher_rejects_oversized_and_empty() {
        assert_eq!(
            dispatch_router_i2np(&inbound_with(Vec::new()), NOW_MS),
            Err(RouterI2npError::Empty)
        );
        let oversized = vec![0x0A; MAX_ROUTER_I2NP_BYTES + 1];
        assert!(matches!(
            dispatch_router_i2np(&inbound_with(oversized), NOW_MS),
            Err(RouterI2npError::TooLarge { .. })
        ));
    }

    #[test]
    fn dispatcher_reports_unknown_and_unsupported_boundedly() {
        // Unknown type byte 99 never enters a decoder.
        let unknown = vec![99, 0, 1, 2, 3];
        let outcome =
            dispatch_router_i2np(&inbound_with(unknown.clone()), NOW_MS).expect("unsupported");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::Unsupported { type_byte: 99, .. }
        ));
        assert_eq!(outcome.encoded_len(), unknown.len());

        // Known-but-unsupported Garlic body returns Unsupported, never an error.
        let garlic = I2npBody::Garlic(OpaqueMessageBody {
            payload: i2pr_proto::DeferredPayload::new(vec![1, 2, 3], 1024).expect("payload"),
        });
        let message = I2npMessage::new_standard(0xD1, Date::from_millis(NOW_MS + 60_000), garlic)
            .expect("garlic message");
        let bytes = message
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode garlic");
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("garlic");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::Unsupported { type_byte: 11, .. }
        ));
    }

    #[test]
    fn dispatcher_routes_database_store_and_tunnel_data() {
        // DatabaseStore control hook (short-transport, small RouterInfo-shaped payload is
        // unnecessary here: use a Deferred empty RouterInfo payload via the codec).
        let gzip: Vec<u8> = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0, 0xFF, 3, 0];
        let payload = DeferredPayload::new(gzip, usize::from(u16::MAX)).expect("deferred payload");
        let store = DatabaseStoreMessage {
            key: Hash::from_bytes([0x22; 32]),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(payload),
        };
        let body = I2npBody::DatabaseStore(Box::new(store));
        let short_exp = now_secs() as u32 + 60;
        let message = I2npMessage::new_short_transport(0xE1, short_exp, body).expect("store");
        let bytes = message
            .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode store");
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("store");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DatabaseStore,
                ..
            }
        ));

        // TunnelData hook preserves the tunnel id.
        let cell = i2pr_proto::TunnelDataMessage {
            tunnel_id: 0x1234_5678,
            data: [0xAB; 1024],
        };
        let body = I2npBody::TunnelData(Box::new(cell));
        let message = I2npMessage::new_standard(0xE2, Date::from_millis(NOW_MS + 60_000), body)
            .expect("tunnel data");
        let bytes = message
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode cell");
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("cell");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::TunnelData {
                tunnel_id: 0x1234_5678,
                ..
            }
        ));
    }

    #[test]
    fn delivery_request_validates_bounds() {
        let peer = PeerId::from_hash(Hash::from_bytes([0x33; 32]));
        assert!(matches!(
            RouterDeliveryRequest::new(peer, vec![1, 2, 3], Duration::from_secs(0)),
            Err(RouterDeliveryError::ZeroTimeout)
        ));
        assert!(matches!(
            RouterDeliveryRequest::new(
                peer,
                vec![1, 2, 3],
                MAX_ROUTER_DELIVERY_TIMEOUT + Duration::from_secs(1)
            ),
            Err(RouterDeliveryError::TimeoutTooLong)
        ));
        assert!(matches!(
            RouterDeliveryRequest::new(peer, Vec::new(), Duration::from_secs(1)),
            Err(RouterDeliveryError::MessageTooLarge)
        ));
        let oversized = vec![0_u8; MAX_I2NP_MESSAGE_BYTES + 1];
        assert!(matches!(
            RouterDeliveryRequest::new(peer, oversized, Duration::from_secs(1)),
            Err(RouterDeliveryError::MessageTooLarge)
        ));
    }

    #[test]
    fn runtime_config_mapping_preserves_daemon_ceilings() {
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
        let config = crate::config::Config::parse(text).expect("strict profile");
        let runtime = ssu2_runtime_config_from(&config.ssu2).expect("mapping");
        assert_eq!(
            runtime.limits.max_pending_handshakes,
            config.ssu2.max_pending_handshakes
        );
        assert_eq!(
            runtime.limits.max_active_sessions,
            config.ssu2.max_active_sessions
        );
        assert_eq!(
            runtime.limits.max_outbound_datagrams,
            config.ssu2.max_datagram_queue_items
        );
        assert_eq!(runtime.deadlines.handshake, config.ssu2.handshake_timeout);
        assert!(runtime.deadlines.dial >= runtime.deadlines.handshake);
        runtime.validate().expect("runtime validates");
    }

    #[test]
    fn socket_config_rejects_disabled_and_broad_profiles() {
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n";
        let config = crate::config::Config::parse(text).expect("defaults");
        assert!(matches!(
            ssu2_socket_config_from(&config.ssu2),
            Err(Ssu2ServiceError::ControlledProfile(_))
        ));
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"\"\nbind_ipv6 = \"::1\"\n";
        let config = crate::config::Config::parse(text).expect("ipv6 profile");
        let sockets = ssu2_socket_config_from(&config.ssu2).expect("ipv6 sockets");
        assert!(sockets.ipv4.is_none());
        assert!(sockets.ipv6.is_some());
    }

    #[test]
    fn controlled_identity_rejects_non_loopback_and_zero_port() {
        use i2pr_crypto::RouterIdentityBundle;
        use rand_core::OsRng;
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        assert!(generate_controlled_identity(&bundle, "192.0.2.1", 44001).is_err());
        assert!(generate_controlled_identity(&bundle, "127.0.0.1", 0).is_err());
        let material =
            generate_controlled_identity(&bundle, "127.0.0.1", 44001).expect("controlled");
        assert!(!material.router_info.is_empty());
    }

    #[test]
    fn verify_reference_rejects_bytes_without_ssu2() {
        assert!(matches!(
            verify_reference_router_info(b"not-a-routerinfo"),
            Err(Ssu2ServiceError::InvalidIdentity)
        ));
    }
}
