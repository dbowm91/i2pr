//! Runtime-neutral private router <-> trusted application-manager protocol.
//!
//! This crate is the **router-facing half** of the managed native application
//! runtime. It owns the wire contract only: handshake, bounded frames, strict
//! directional control vocabulary, validated opaque handles, and pure bounded
//! accounting. It has no transport, no runtime, no sockets, no process
//! ownership, no filesystem, no DNS, and no sandbox backend.
//!
//! # What this protocol is
//!
//! An internal trusted-component protocol used by a future separately supervised
//! application manager (`i2pr-appd`, owned by Plan 369) to project an
//! **already authenticated** application principal into the router's existing
//! principal/capability gateway (`AppGatewaySession`, Plan 355).
//!
//! # What this protocol is not
//!
//! It is not the application v1 protocol (`i2pr-app-proto`), not SAM, not I2CP,
//! not Proposal 170 / I2PControl, and not an AppManager administrator API. Its
//! authority ceiling is deliberately smaller than Proposal 170: it can create
//! and close one app gateway session, open SAM/I2CP service connections that
//! the session's immutable effective capabilities already allow, forward opaque
//! protocol octets, observe backend termination, and answer health/shutdown.
//!
//! It cannot install packages, mutate grants or policy, launch or stop
//! processes, read or write router configuration, dispatch general I2PControl,
//! or name daemon methods. `control_scoped` is **unrepresentable** in the
//! service vocabulary and is rejected with a typed error rather than being
//! silently downgraded.
//!
//! Transport selection is deliberately out of scope here. Plan 368 implements
//! the daemon bridge over an injected reliable duplex byte stream; Plan 369 owns
//! the concrete anonymous inherited transport. A discoverable localhost
//! endpoint is not a permitted transport for either.
//!
//! See `specs/references/managed-app-manager-protocol-v1.md` for the
//! language-neutral normative contract. No Rust representation here is a wire
//! ABI; the JSON control vocabulary and the frame layout are.

#![forbid(unsafe_code)]

pub mod apphost;
pub mod datagram;

use std::collections::BTreeSet;

use i2pr_app_proto::{AppId, AppInstanceId, AppPrincipal, Capability, PublisherId, RequestId};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

/// Protocol version, independent of the application protocol version.
pub const MANAGER_PROTOCOL_MAJOR: u8 = 1;
pub const MANAGER_PROTOCOL_MINOR: u8 = 1;

/// Manager-protocol handshake magic. Distinct from `I2PA` so a manager-protocol
/// byte can never be mistaken for an application-protocol byte.
pub const HANDSHAKE_MAGIC: [u8; 4] = *b"I2PM";
pub const HANDSHAKE_BYTES: usize = 9;

/// Frame header: version, kind, two reserved zero bytes, stream id, length.
pub const FRAME_HEADER_BYTES: usize = 12;
/// Frame wire version. Bumped independently of the protocol version.
pub const FRAME_VERSION: u8 = 1;

/// Control frames are unstreamed; `stream_id` must be zero.
pub const CONTROL_STREAM_ID: u32 = 0;

pub const MAX_CONTROL_BYTES: usize = 16_384;
pub const MAX_DATA_FRAME_BYTES: usize = 65_536;
/// Protocol ceiling on concurrent app gateway sessions per manager transport.
pub const MAX_MANAGER_SESSIONS: usize = 32;
/// Per-session live service streams. Matches the app protocol stream ceiling.
pub const MAX_SERVICE_STREAMS_PER_SESSION: usize = 128;
pub const MAX_LOCAL_SERVICES_PER_SESSION: usize = i2pr_app_proto::MAX_LOCAL_SERVICES;
pub const MAX_LOCAL_SERVICE_CONNECTIONS_PER_SESSION: usize =
    i2pr_app_proto::MAX_LOCAL_SERVICE_CONNECTIONS;
pub const MAX_LOCAL_SERVICE_QUEUE_FRAMES: usize = i2pr_app_proto::MAX_LOCAL_SERVICE_QUEUE_FRAMES;
pub const MAX_INFLIGHT_REQUESTS: usize = 64;
pub const MAX_DIAGNOSTIC_BYTES: usize = 1_024;
/// Bounded decimal digits for a 128-bit opaque id (39 digits is the maximum).
///
/// Mirrors `i2pr_app_proto::MAX_DECIMAL_DIGITS`. Both protocols now share one
/// canonical id grammar, defined once by `AppInstanceId::parse`.
pub const MAX_DECIMAL_DIGITS: usize = 39;
/// Absolute ceiling a manager session may request for gateway connections.
pub const MAX_GATEWAY_CONNECTIONS: u32 = 128;

/// Typed protocol failures. No variant carries payload bytes or secrets.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ManagerProtocolError {
    #[error("unsupported protocol version")]
    UnsupportedVersion,
    #[error("manager handshake role mismatch")]
    RoleMismatch,
    #[error("manager handshake magic mismatch")]
    BadMagic,
    #[error("invalid manager handle")]
    InvalidHandle,
    #[error("invalid manager identifier")]
    InvalidIdentifier,
    #[error("limit exceeded: {0}")]
    LimitExceeded(&'static str),
    #[error("malformed manager frame")]
    MalformedFrame,
    #[error("truncated manager frame")]
    TruncatedFrame,
    #[error("unsupported manager frame")]
    UnsupportedFrame,
    #[error("invalid manager control payload")]
    InvalidControl,
    #[error("service is unavailable")]
    UnsupportedService,
    #[error("response does not match an active request")]
    UnmatchedRequest,
    #[error("transport ended")]
    TransportClosed,
}

// ---------------------------------------------------------------------------
// Opaque daemon-assigned handles
// ---------------------------------------------------------------------------

/// Opaque, daemon-assigned manager session handle.
///
/// This is deliberately *not* an `AppInstanceId`, a `SessionLimits` value, or
/// any daemon-internal identifier: no Rust address, pointer, connection id, or
/// router state crosses the wire. Handles are only meaningful to the daemon
/// that issued them and are scoped to one manager transport.
macro_rules! manager_handle {
    ($name:ident, $limit:expr) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(try_from = "u64", into = "u64")]
        pub struct $name(u64);

        impl $name {
            /// Zero is reserved so a handle is never ambiguous with "absent".
            pub fn new(value: u64) -> Result<Self, ManagerProtocolError> {
                if value == 0 {
                    return Err(ManagerProtocolError::InvalidHandle);
                }
                Ok(Self(value))
            }
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl TryFrom<u64> for $name {
            type Error = ManagerProtocolError;
            fn try_from(value: u64) -> Result<Self, Self::Error> {
                if value == 0 || value > $limit {
                    return Err(ManagerProtocolError::InvalidHandle);
                }
                Ok(Self(value))
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

/// Maximum issued handle value. Handles are dense and never wrap in practice;
/// the bound exists so a hostile peer cannot fabricate an unbounded handle.
const MAX_HANDLE: u64 = u64::MAX;

manager_handle!(ManagerSessionId, MAX_HANDLE);
manager_handle!(ManagerServiceStreamId, MAX_HANDLE);
manager_handle!(ManagerLocalServiceId, MAX_HANDLE);

/// Opaque 128-bit application launch-instance id, carried as decimal digits.
///
/// The manager protocol keeps its own principal type so this crate's wire shape
/// does not move when the application contract moves. Both now use the *same*
/// canonical grammar: `AppInstanceId` itself adopted bounded decimal digits in
/// managed-runtime Plan 370, which corrected the app v1 `hello` field from a
/// JSON number. That Plan 368 fix was correct but had been applied only here,
/// because the root cause — serde's internally tagged enum buffer, which has no
/// `visit_u128` — was not recorded precisely; see ADR 0035 defect D1.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ManagerInstanceId(String);

impl ManagerInstanceId {
    pub fn new(value: u128) -> Self {
        Self(value.to_string())
    }

    /// Parses this id as an [`AppInstanceId`].
    ///
    /// There is deliberately no second grammar here: `AppInstanceId::parse` is
    /// the single definition of canonical form, so the two protocols cannot
    /// drift into accepting different spellings of one id.
    pub fn to_app_instance_id(&self) -> Result<AppInstanceId, ManagerProtocolError> {
        AppInstanceId::parse(&self.0).map_err(|_| ManagerProtocolError::InvalidIdentifier)
    }
}

impl TryFrom<String> for ManagerInstanceId {
    type Error = ManagerProtocolError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        // Validate eagerly so an invalid id cannot sit in a decoded message.
        let candidate = Self(value);
        candidate.to_app_instance_id()?;
        Ok(candidate)
    }
}

impl From<ManagerInstanceId> for String {
    fn from(value: ManagerInstanceId) -> Self {
        value.0
    }
}

impl From<AppInstanceId> for ManagerInstanceId {
    fn from(value: AppInstanceId) -> Self {
        Self::new(value.get())
    }
}

impl From<&AppInstanceId> for ManagerInstanceId {
    fn from(value: &AppInstanceId) -> Self {
        Self(value.as_str().to_owned())
    }
}

/// The one application principal a manager session may bind.
///
/// It deliberately is not `AppPrincipal`: this type is the only principal shape
/// the bridge accepts from the wire. Both carry the same canonical decimal-digit
/// instance id, so converting between them is total and lossless.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerPrincipal {
    pub app_id: AppId,
    pub instance_id: ManagerInstanceId,
    pub publisher_id: Option<PublisherId>,
}

impl ManagerPrincipal {
    /// Converts to the application contract's principal for gateway binding.
    pub fn to_app_principal(&self) -> Result<AppPrincipal, ManagerProtocolError> {
        Ok(AppPrincipal {
            app_id: self.app_id.clone(),
            instance_id: self.instance_id.to_app_instance_id()?,
            publisher_id: self.publisher_id.clone(),
        })
    }
}

impl From<&AppPrincipal> for ManagerPrincipal {
    fn from(value: &AppPrincipal) -> Self {
        Self {
            app_id: value.app_id.clone(),
            instance_id: ManagerInstanceId::from(&value.instance_id),
            publisher_id: value.publisher_id.clone(),
        }
    }
}

/// Daemon-assigned effective capability grant.
///
/// This is deliberately a distinct type from `RequestedCapability`. It carries
/// a name that reflects that the *manager* — not the application — asserts the
/// grant, and the daemon still re-derives it through the existing
/// `GrantedCapability` administrator path before any backend is allocated. An
/// application's `hello` or requested-capability message can never construct
/// one: there is no decoder from either into this type.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EffectiveGrant {
    pub capability: Capability,
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagerRole {
    /// The trusted application manager process.
    Manager,
}

pub const MANAGER_ROLE_BYTE: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Handshake {
    pub role: ManagerRole,
    pub major: u8,
    pub minor: u8,
}

impl Handshake {
    pub fn encode(&self) -> [u8; HANDSHAKE_BYTES] {
        [
            HANDSHAKE_MAGIC[0],
            HANDSHAKE_MAGIC[1],
            HANDSHAKE_MAGIC[2],
            HANDSHAKE_MAGIC[3],
            self.major,
            self.minor,
            match self.role {
                ManagerRole::Manager => MANAGER_ROLE_BYTE,
            },
            0,
            0,
        ]
    }

    /// Exact-consumption decode. Any wrong magic, length, reserved byte, role,
    /// or major version is rejected; the minor version is informational so a
    /// compatible later manager can still be admitted.
    pub fn decode(bytes: &[u8]) -> Result<Self, ManagerProtocolError> {
        if bytes.len() != HANDSHAKE_BYTES {
            return Err(ManagerProtocolError::TruncatedFrame);
        }
        if bytes[..4] != HANDSHAKE_MAGIC {
            return Err(ManagerProtocolError::BadMagic);
        }
        if bytes[7..9] != [0, 0] {
            return Err(ManagerProtocolError::MalformedFrame);
        }
        if bytes[6] != MANAGER_ROLE_BYTE {
            return Err(ManagerProtocolError::RoleMismatch);
        }
        if bytes[4] != MANAGER_PROTOCOL_MAJOR {
            return Err(ManagerProtocolError::UnsupportedVersion);
        }
        Ok(Self {
            role: ManagerRole::Manager,
            major: bytes[4],
            minor: bytes[5],
        })
    }

    /// Directional decode guard. Only the manager role exists on this protocol,
    /// so any other role is a typed mismatch rather than a silent accept.
    pub fn decode_manager_to_daemon(
        &self,
        bytes: &[u8],
    ) -> Result<ManagerToDaemonMessage, ManagerProtocolError> {
        if self.role != ManagerRole::Manager {
            return Err(ManagerProtocolError::RoleMismatch);
        }
        decode_manager_to_daemon_control(bytes)
    }

    /// Directional decode guard for the daemon's replies and notifications.
    pub fn decode_daemon_to_manager(
        &self,
        bytes: &[u8],
    ) -> Result<DaemonToManagerMessage, ManagerProtocolError> {
        if self.role != ManagerRole::Manager {
            return Err(ManagerProtocolError::RoleMismatch);
        }
        decode_daemon_to_manager_control(bytes)
    }
}

// ---------------------------------------------------------------------------
// Service vocabulary
// ---------------------------------------------------------------------------

/// Service a manager session may open through this bridge.
///
/// `control_scoped` is intentionally **absent**: it is unrepresentable here, so
/// no decode path can name it. [`ManagerService::parse`] additionally recognises the
/// literal string so a peer that asks for it receives a typed
/// [`ManagerProtocolError::UnsupportedService`] rather than a generic malformed
/// payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerService {
    Sam,
    I2cp,
    /// Typed private datagram operations scoped to a SAM PRIMARY/child.
    SamDatagram,
}

impl ManagerService {
    /// Parses the wire spelling. `control_scoped` is rejected by type, not by
    /// being silently ignored.
    pub fn parse(value: &str) -> Result<Self, ManagerProtocolError> {
        match value {
            "sam" => Ok(Self::Sam),
            "i2cp" => Ok(Self::I2cp),
            "sam_datagram" => Ok(Self::SamDatagram),
            "control_scoped" => Err(ManagerProtocolError::UnsupportedService),
            _ => Err(ManagerProtocolError::InvalidIdentifier),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sam => "sam",
            Self::I2cp => "i2cp",
            Self::SamDatagram => "sam_datagram",
        }
    }

    /// Capability this service requires, for the daemon's pre-allocation check.
    pub const fn required_capability(self) -> Capability {
        match self {
            Self::Sam | Self::SamDatagram => Capability::Sam,
            Self::I2cp => Capability::I2cp,
        }
    }
}

/// Bounded gateway limits a manager may request for one session.
///
/// The daemon clamps this further against the private SAM/I2CP owner ceilings;
/// a request above the protocol ceiling is rejected, not truncated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerGatewayLimits {
    pub max_connections: u32,
}

impl ManagerGatewayLimits {
    pub fn new(max_connections: u32) -> Result<Self, ManagerProtocolError> {
        if max_connections == 0 || max_connections > MAX_GATEWAY_CONNECTIONS {
            return Err(ManagerProtocolError::LimitExceeded("gateway connections"));
        }
        Ok(Self { max_connections })
    }
}

// ---------------------------------------------------------------------------
// Control vocabulary
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ManagerToDaemonMessage {
    #[serde(rename = "create_session")]
    CreateSession {
        request_id: RequestId,
        principal: ManagerPrincipal,
        effective_capabilities: Vec<EffectiveGrant>,
        limits: ManagerGatewayLimits,
    },
    #[serde(rename = "close_session")]
    CloseSession {
        request_id: RequestId,
        session: ManagerSessionId,
    },
    #[serde(rename = "open_service")]
    OpenService {
        request_id: RequestId,
        session: ManagerSessionId,
        service: ManagerService,
    },
    #[serde(rename = "close_service")]
    CloseService {
        request_id: RequestId,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
    },
    #[serde(rename = "publish_local_service")]
    PublishLocalService {
        request_id: RequestId,
        session: ManagerSessionId,
        service_name: String,
        preferred_port: Option<u16>,
    },
    #[serde(rename = "unpublish_local_service")]
    UnpublishLocalService {
        request_id: RequestId,
        session: ManagerSessionId,
        service: ManagerLocalServiceId,
    },
    #[serde(rename = "reset_service")]
    ResetService {
        request_id: RequestId,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
        reason: String,
    },
    #[serde(rename = "health")]
    Health { request_id: RequestId },
    #[serde(rename = "shutdown")]
    Shutdown {
        request_id: RequestId,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerErrorCode {
    UnsupportedOperation,
    PermissionDenied,
    InvalidRequest,
    ResourceLimit,
    Conflict,
    NotFound,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerError {
    pub code: ManagerErrorCode,
    pub diagnostic: Option<String>,
}

/// Why a service stream ended, as observed by the manager.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceEndReason {
    /// EOF, cancellation, or protocol completion closed the backend.
    BackendClosed,
    /// The existing protocol owner rejected admission after gateway admission.
    BackendRejected,
    /// The manager closed or reset the stream.
    ManagerClosed,
    /// The owning session closed, or the manager transport died.
    SessionEnded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum DaemonToManagerMessage {
    #[serde(rename = "session_opened")]
    SessionOpened {
        request_id: RequestId,
        session: ManagerSessionId,
    },
    #[serde(rename = "session_closed")]
    SessionClosed {
        request_id: RequestId,
        session: ManagerSessionId,
    },
    #[serde(rename = "service_opened")]
    ServiceOpened {
        request_id: RequestId,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
    },
    #[serde(rename = "service_closed")]
    ServiceClosed {
        request_id: RequestId,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
    },
    #[serde(rename = "service_reset")]
    ServiceReset {
        request_id: RequestId,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
    },
    #[serde(rename = "local_service_published")]
    LocalServicePublished {
        request_id: RequestId,
        session: ManagerSessionId,
        service: ManagerLocalServiceId,
        port: u16,
    },
    #[serde(rename = "local_service_unpublished")]
    LocalServiceUnpublished {
        request_id: RequestId,
        session: ManagerSessionId,
        service: ManagerLocalServiceId,
    },
    #[serde(rename = "health_status")]
    HealthStatus {
        request_id: RequestId,
        state: String,
        detail: Option<String>,
    },
    #[serde(rename = "shutdown_ack")]
    ShutdownAck { request_id: RequestId },
    /// Terminal reply for a request the daemon refused. `request_id` always
    /// correlates with an in-flight request; the daemon never emits an
    /// uncorrelated reply.
    #[serde(rename = "rejected")]
    Rejected {
        request_id: RequestId,
        error: ManagerError,
    },
    /// Unsolicited: the daemon observed the backend for one service stream end.
    #[serde(rename = "service_ended")]
    ServiceEnded {
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
        reason: ServiceEndReason,
    },
    #[serde(rename = "local_service_incoming")]
    LocalServiceIncoming {
        session: ManagerSessionId,
        service: ManagerLocalServiceId,
        stream: ManagerServiceStreamId,
    },
    /// Unsolicited: the session was torn down and every descendant stream died.
    #[serde(rename = "session_ended")]
    SessionEnded {
        session: ManagerSessionId,
        reason: ServiceEndReason,
    },
}

impl DaemonToManagerMessage {
    /// The request this message answers, when it answers one.
    ///
    /// Notifications carry no correlation and return `None`; a reply that
    /// answers a request always returns the manager's own request id, which is
    /// how the daemon's per-session request ledger validates replies.
    pub fn correlation(&self) -> Option<RequestId> {
        match self {
            Self::SessionOpened { request_id, .. }
            | Self::SessionClosed { request_id, .. }
            | Self::ServiceOpened { request_id, .. }
            | Self::ServiceClosed { request_id, .. }
            | Self::ServiceReset { request_id, .. }
            | Self::LocalServicePublished { request_id, .. }
            | Self::LocalServiceUnpublished { request_id, .. }
            | Self::HealthStatus { request_id, .. }
            | Self::ShutdownAck { request_id }
            | Self::Rejected { request_id, .. } => Some(*request_id),
            Self::ServiceEnded { .. }
            | Self::SessionEnded { .. }
            | Self::LocalServiceIncoming { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    Control,
    Data,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub kind: FrameKind,
    /// Data-frame stream id, carried as the daemon-assigned
    /// [`ManagerServiceStreamId`]. Always zero for control frames.
    pub stream_id: u32,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn control(payload: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Control,
            stream_id: CONTROL_STREAM_ID,
            payload,
        }
    }

    pub fn data(stream: ManagerServiceStreamId, payload: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Data,
            stream_id: u32::try_from(stream.get()).unwrap_or(u32::MAX),
            payload,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ManagerProtocolError> {
        let limit = match self.kind {
            FrameKind::Control => MAX_CONTROL_BYTES,
            FrameKind::Data => MAX_DATA_FRAME_BYTES,
        };
        if self.payload.len() > limit {
            return Err(ManagerProtocolError::LimitExceeded("frame payload"));
        }
        if (self.kind == FrameKind::Control) != (self.stream_id == CONTROL_STREAM_ID) {
            return Err(ManagerProtocolError::MalformedFrame);
        }
        if self.kind == FrameKind::Data
            && ManagerServiceStreamId::try_from(u64::from(self.stream_id)).is_err()
        {
            return Err(ManagerProtocolError::InvalidHandle);
        }
        let len = u32::try_from(self.payload.len())
            .map_err(|_| ManagerProtocolError::LimitExceeded("frame payload"))?;
        let mut out = Vec::with_capacity(FRAME_HEADER_BYTES + self.payload.len());
        out.push(FRAME_VERSION);
        out.push(match self.kind {
            FrameKind::Control => 1,
            FrameKind::Data => 2,
        });
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&self.stream_id.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Exact-consumption decode returning `(frame, bytes_consumed)`.
    pub fn decode(input: &[u8]) -> Result<(Self, usize), ManagerProtocolError> {
        if input.len() < FRAME_HEADER_BYTES {
            return Err(ManagerProtocolError::TruncatedFrame);
        }
        if input[0] != FRAME_VERSION {
            return Err(ManagerProtocolError::UnsupportedFrame);
        }
        let kind = match input[1] {
            1 => FrameKind::Control,
            2 => FrameKind::Data,
            _ => return Err(ManagerProtocolError::UnsupportedFrame),
        };
        if input[2] != 0 || input[3] != 0 {
            return Err(ManagerProtocolError::MalformedFrame);
        }
        let stream_id = u32::from_be_bytes(
            input[4..8]
                .try_into()
                .map_err(|_| ManagerProtocolError::MalformedFrame)?,
        );
        let payload_len = u32::from_be_bytes(
            input[8..12]
                .try_into()
                .map_err(|_| ManagerProtocolError::MalformedFrame)?,
        ) as usize;
        let limit = match kind {
            FrameKind::Control => MAX_CONTROL_BYTES,
            FrameKind::Data => MAX_DATA_FRAME_BYTES,
        };
        if payload_len > limit {
            return Err(ManagerProtocolError::LimitExceeded("frame payload"));
        }
        if (kind == FrameKind::Control) != (stream_id == CONTROL_STREAM_ID) {
            return Err(ManagerProtocolError::MalformedFrame);
        }
        let total = FRAME_HEADER_BYTES
            .checked_add(payload_len)
            .ok_or(ManagerProtocolError::MalformedFrame)?;
        if input.len() < total {
            return Err(ManagerProtocolError::TruncatedFrame);
        }
        Ok((
            Self {
                kind,
                stream_id,
                payload: input[FRAME_HEADER_BYTES..total].to_vec(),
            },
            total,
        ))
    }
}

// ---------------------------------------------------------------------------
// Directional control codecs
// ---------------------------------------------------------------------------

fn decode_directional_control<T: DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, ManagerProtocolError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ManagerProtocolError::LimitExceeded("control"));
    }
    // `deny_unknown_fields` does NOT reject a repeated key: serde keeps the last
    // occurrence and decodes successfully. That makes `{"request_id":1,
    // "request_id":2}` and `{"request_id":2}` indistinguishable on the wire, so
    // an exact-consumption protocol must refuse it explicitly. The strict scan
    // runs first and fails closed on any malformed JSON.
    reject_duplicate_object_keys(bytes)?;
    // `serde_json::from_slice` rejects trailing bytes, so a control frame is
    // consumed exactly or not at all.
    serde_json::from_slice(bytes).map_err(|_| ManagerProtocolError::InvalidControl)
}

/// Rejects any JSON object that repeats a member name, at any nesting depth.
///
/// This is a bounded, allocation-free scanner over a payload already capped at
/// [`MAX_CONTROL_BYTES`]. It is intentionally conservative: any shape it cannot
/// classify is reported as malformed rather than skipped.
fn reject_duplicate_object_keys(bytes: &[u8]) -> Result<(), ManagerProtocolError> {
    let mut parser = StrictKeyScanner {
        bytes,
        offset: 0,
        // Object nesting is bounded by the payload size; 64 is far above any
        // real nesting in this vocabulary and keeps the stack bounded.
        depth: 0,
    };
    parser.scan_value()?;
    if parser.offset != bytes.len() {
        // Trailing bytes after a complete JSON document.
        return Err(ManagerProtocolError::InvalidControl);
    }
    Ok(())
}

const MAX_JSON_DEPTH: usize = 64;

struct StrictKeyScanner<'a> {
    bytes: &'a [u8],
    offset: usize,
    depth: usize,
}

impl StrictKeyScanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.offset += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ManagerProtocolError> {
        self.skip_whitespace();
        if self.peek() == Some(byte) {
            self.offset += 1;
            Ok(())
        } else {
            Err(ManagerProtocolError::InvalidControl)
        }
    }

    fn scan_value(&mut self) -> Result<(), ManagerProtocolError> {
        if self.depth >= MAX_JSON_DEPTH {
            return Err(ManagerProtocolError::InvalidControl);
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.scan_object(),
            Some(b'[') => self.scan_array(),
            Some(b'"') => {
                self.scan_string()?;
                Ok(())
            }
            Some(_) => self.scan_scalar(),
            None => Err(ManagerProtocolError::InvalidControl),
        }
    }

    fn scan_object(&mut self) -> Result<(), ManagerProtocolError> {
        self.expect(b'{')?;
        self.depth += 1;
        let mut keys: BTreeSet<Vec<u8>> = BTreeSet::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.offset += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.skip_whitespace();
            let key = self.scan_string()?;
            if !keys.insert(key) {
                return Err(ManagerProtocolError::InvalidControl);
            }
            self.expect(b':')?;
            self.scan_value()?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.offset += 1,
                Some(b'}') => {
                    self.offset += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(ManagerProtocolError::InvalidControl),
            }
        }
    }

    fn scan_array(&mut self) -> Result<(), ManagerProtocolError> {
        self.expect(b'[')?;
        self.depth += 1;
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.offset += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.scan_value()?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.offset += 1,
                Some(b']') => {
                    self.offset += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(ManagerProtocolError::InvalidControl),
            }
        }
    }

    /// Scans a JSON string into its raw (still-escaped) bytes so two spellings
    /// of the same member name are compared consistently.
    fn scan_string(&mut self) -> Result<Vec<u8>, ManagerProtocolError> {
        self.expect(b'"')?;
        let start = self.offset;
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    let key = self.bytes[start..self.offset].to_vec();
                    self.offset += 1;
                    return Ok(key);
                }
                b'\\' => {
                    self.offset += 1;
                    if self.peek().is_none() {
                        return Err(ManagerProtocolError::InvalidControl);
                    }
                    self.offset += 1;
                }
                0x00..=0x1F => return Err(ManagerProtocolError::InvalidControl),
                _ => self.offset += 1,
            }
        }
        Err(ManagerProtocolError::InvalidControl)
    }

    /// Scans a JSON literal or number. Only `true`, `false`, `null`, and a
    /// number-shaped token are accepted; any other bare word is malformed.
    fn scan_scalar(&mut self) -> Result<(), ManagerProtocolError> {
        let start = self.offset;
        while let Some(byte) = self.peek() {
            if matches!(
                byte,
                b' ' | b'\t' | b'\n' | b'\r' | b',' | b'}' | b']' | b':'
            ) {
                break;
            }
            self.offset += 1;
        }
        if self.offset == start {
            return Err(ManagerProtocolError::InvalidControl);
        }
        let literal = &self.bytes[start..self.offset];
        let is_keyword = matches!(literal, b"true" | b"false" | b"null");
        // A number may use `-`, a decimal point, and `e`/`E` exponents. serde
        // performs the real validation; this only rejects bare words.
        let is_number = literal
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E'));
        if !is_keyword && !is_number {
            return Err(ManagerProtocolError::InvalidControl);
        }
        Ok(())
    }
}

pub fn encode_manager_to_daemon_control(
    message: &ManagerToDaemonMessage,
) -> Result<Vec<u8>, ManagerProtocolError> {
    validate_manager_to_daemon_message(message)?;
    let bytes = serde_json::to_vec(message).map_err(|_| ManagerProtocolError::InvalidControl)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ManagerProtocolError::LimitExceeded("control"));
    }
    Ok(bytes)
}

pub fn decode_manager_to_daemon_control(
    bytes: &[u8],
) -> Result<ManagerToDaemonMessage, ManagerProtocolError> {
    let message = decode_directional_control(bytes)?;
    validate_manager_to_daemon_message(&message)?;
    Ok(message)
}

pub fn encode_daemon_to_manager_control(
    message: &DaemonToManagerMessage,
) -> Result<Vec<u8>, ManagerProtocolError> {
    validate_daemon_to_manager_message(message)?;
    let bytes = serde_json::to_vec(message).map_err(|_| ManagerProtocolError::InvalidControl)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ManagerProtocolError::LimitExceeded("control"));
    }
    Ok(bytes)
}

pub fn decode_daemon_to_manager_control(
    bytes: &[u8],
) -> Result<DaemonToManagerMessage, ManagerProtocolError> {
    let message = decode_directional_control(bytes)?;
    validate_daemon_to_manager_message(&message)?;
    Ok(message)
}

/// Validates a manager request. There is deliberately no administrator variant
/// here: the control vocabulary contains no grant, revoke, policy, package,
/// configuration, or process operation to validate.
pub fn validate_manager_to_daemon_message(
    message: &ManagerToDaemonMessage,
) -> Result<(), ManagerProtocolError> {
    match message {
        ManagerToDaemonMessage::CreateSession {
            effective_capabilities,
            limits,
            ..
        } => {
            validate_effective_grants(effective_capabilities)?;
            let _ = ManagerGatewayLimits::new(limits.max_connections)?;
            Ok(())
        }
        ManagerToDaemonMessage::CloseSession { .. }
        | ManagerToDaemonMessage::OpenService { .. }
        | ManagerToDaemonMessage::CloseService { .. }
        | ManagerToDaemonMessage::UnpublishLocalService { .. }
        | ManagerToDaemonMessage::Health { .. } => Ok(()),
        ManagerToDaemonMessage::PublishLocalService {
            service_name,
            preferred_port,
            ..
        } => {
            if service_name.is_empty()
                || service_name.len() > i2pr_app_proto::MAX_LOCAL_SERVICE_NAME_BYTES
                || !service_name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                || preferred_port.is_some_and(|port| port < 1024)
            {
                return Err(ManagerProtocolError::InvalidControl);
            }
            Ok(())
        }
        ManagerToDaemonMessage::ResetService { reason, .. }
        | ManagerToDaemonMessage::Shutdown { reason, .. } => bounded_string(reason),
    }
}

pub fn validate_daemon_to_manager_message(
    message: &DaemonToManagerMessage,
) -> Result<(), ManagerProtocolError> {
    match message {
        DaemonToManagerMessage::LocalServicePublished { port, .. } if *port < 1024 => {
            Err(ManagerProtocolError::InvalidControl)
        }
        DaemonToManagerMessage::HealthStatus { state, detail, .. } => {
            bounded_string(state)?;
            if let Some(detail) = detail {
                bounded_string(detail)?;
            }
            Ok(())
        }
        DaemonToManagerMessage::Rejected { error, .. } => {
            if let Some(diagnostic) = &error.diagnostic {
                bounded_string(diagnostic)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_effective_grants(grants: &[EffectiveGrant]) -> Result<(), ManagerProtocolError> {
    if grants.len() > i2pr_app_proto::MAX_CAPABILITIES {
        return Err(ManagerProtocolError::LimitExceeded("capabilities"));
    }
    let unique = grants
        .iter()
        .map(|grant| grant.capability)
        .collect::<BTreeSet<_>>();
    if unique.len() != grants.len() {
        return Err(ManagerProtocolError::InvalidControl);
    }
    Ok(())
}

fn bounded_string(value: &str) -> Result<(), ManagerProtocolError> {
    if value.len() > MAX_DIAGNOSTIC_BYTES || !value.is_ascii() {
        return Err(ManagerProtocolError::LimitExceeded("diagnostic"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pure bounded accounting
// ---------------------------------------------------------------------------

/// Pure per-manager-session accounting. A transport owner is responsible for
/// serializing access and applying lifecycle cleanup; this type owns no I/O and
/// no router state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ManagerScopeLimits {
    sessions: BTreeSet<u64>,
    requests: BTreeSet<u32>,
}

impl ManagerScopeLimits {
    pub fn open_session(&mut self, session: ManagerSessionId) -> Result<(), ManagerProtocolError> {
        if self.sessions.contains(&session.get()) {
            return Err(ManagerProtocolError::InvalidHandle);
        }
        if self.sessions.len() >= MAX_MANAGER_SESSIONS {
            return Err(ManagerProtocolError::LimitExceeded("manager sessions"));
        }
        self.sessions.insert(session.get());
        Ok(())
    }

    pub fn close_session(&mut self, session: ManagerSessionId) -> Result<(), ManagerProtocolError> {
        if self.sessions.remove(&session.get()) {
            Ok(())
        } else {
            Err(ManagerProtocolError::InvalidHandle)
        }
    }

    pub fn register_request(&mut self, request_id: RequestId) -> Result<(), ManagerProtocolError> {
        if self.requests.contains(&request_id.get()) {
            return Err(ManagerProtocolError::UnmatchedRequest);
        }
        if self.requests.len() >= MAX_INFLIGHT_REQUESTS {
            return Err(ManagerProtocolError::LimitExceeded("in-flight requests"));
        }
        self.requests.insert(request_id.get());
        Ok(())
    }

    pub fn complete_request(&mut self, request_id: RequestId) -> Result<(), ManagerProtocolError> {
        if self.requests.remove(&request_id.get()) {
            Ok(())
        } else {
            Err(ManagerProtocolError::UnmatchedRequest)
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn inflight_count(&self) -> usize {
        self.requests.len()
    }

    pub fn has_session(&self, session: ManagerSessionId) -> bool {
        self.sessions.contains(&session.get())
    }
}

/// Pure per-session service-stream accounting, bounded by
/// [`MAX_SERVICE_STREAMS_PER_SESSION`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServiceStreamLedger {
    streams: BTreeSet<u64>,
}

impl ServiceStreamLedger {
    pub fn open(&mut self, stream: ManagerServiceStreamId) -> Result<(), ManagerProtocolError> {
        if self.streams.contains(&stream.get()) {
            return Err(ManagerProtocolError::InvalidHandle);
        }
        if self.streams.len() >= MAX_SERVICE_STREAMS_PER_SESSION {
            return Err(ManagerProtocolError::LimitExceeded("service streams"));
        }
        self.streams.insert(stream.get());
        Ok(())
    }

    /// Closes a stream handle. A handle that is not live fails deterministically,
    /// which is how a stale or cross-session handle is refused.
    pub fn close(&mut self, stream: ManagerServiceStreamId) -> Result<(), ManagerProtocolError> {
        if self.streams.remove(&stream.get()) {
            Ok(())
        } else {
            Err(ManagerProtocolError::InvalidHandle)
        }
    }

    pub fn is_open(&self, stream: ManagerServiceStreamId) -> bool {
        self.streams.contains(&stream.get())
    }

    pub fn len(&self) -> usize {
        self.streams.len()
    }

    pub fn is_empty(&self) -> bool {
        self.streams.is_empty()
    }
}
