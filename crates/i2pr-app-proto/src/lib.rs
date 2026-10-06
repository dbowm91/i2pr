//! Runtime-neutral, bounded managed-application protocol and policy contract.
//!
//! This crate owns validated values only. It has no runtime, socket, process,
//! filesystem, DNS, sandbox, router, or application owner. See
//! `specs/references/managed-native-app-runtime-v1.md` for the language-neutral
//! wire contract. No Rust representation is a wire ABI.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

pub const PROTOCOL_MAJOR: u8 = 1;
pub const PROTOCOL_MINOR: u8 = 0;
pub const FRAME_HEADER_BYTES: usize = 12;
pub const MAX_FRAME_PAYLOAD_BYTES: usize = 65_536;
pub const MAX_CONTROL_BYTES: usize = 16_384;
pub const MAX_CONTROL_FIELDS: usize = 32;
pub const MAX_MANIFEST_BYTES: usize = 65_536;
pub const MAX_MANIFEST_FIELDS: usize = 16;
pub const MAX_IDENTIFIER_BYTES: usize = 64;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_NETWORK_RULES: usize = 64;
pub const MAX_ENTRYPOINTS: usize = 8;
pub const MAX_RESOURCE_ENTRIES: usize = 16;
pub const MAX_STREAMS: usize = 128;
pub const MAX_INFLIGHT_REQUESTS: usize = 64;
pub const MAX_UI_MESSAGE_BYTES: usize = 16_384;
pub const MAX_DIAGNOSTIC_BYTES: usize = 1_024;
pub const MAX_RESOURCE_REQUEST: u64 = 1_099_511_627_776;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("invalid identifier")]
    InvalidIdentifier,
    #[error("invalid opaque identifier")]
    InvalidOpaqueId,
    #[error("invalid path")]
    InvalidPath,
    #[error("limit exceeded: {0}")]
    LimitExceeded(&'static str),
    #[error("malformed frame")]
    MalformedFrame,
    #[error("truncated frame")]
    TruncatedFrame,
    #[error("unsupported frame version or kind")]
    UnsupportedFrame,
    #[error("invalid control payload")]
    InvalidControl,
    #[error("invalid manifest")]
    InvalidManifest,
    #[error("unsupported version")]
    UnsupportedVersion,
    #[error("role mismatch")]
    RoleMismatch,
    #[error("invalid policy rule")]
    InvalidPolicy,
    #[error("reserved capability cannot be granted")]
    ReservedCapability,
    #[error("response does not match an active request")]
    UnmatchedRequest,
    #[error("secured sandbox attestation is incomplete")]
    IncompleteAttestation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct RequestId(u32);
impl RequestId {
    pub fn new(value: u32) -> Result<Self, ContractError> {
        if value == 0 {
            Err(ContractError::InvalidControl)
        } else {
            Ok(Self(value))
        }
    }
    pub const fn get(self) -> u32 {
        self.0
    }
}
impl TryFrom<u32> for RequestId {
    type Error = ContractError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<RequestId> for u32 {
    fn from(value: RequestId) -> Self {
        value.0
    }
}

macro_rules! text_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, ContractError> {
                let value = value.into();
                let valid = !value.is_empty()
                    && value.len() <= MAX_IDENTIFIER_BYTES
                    && value.is_ascii()
                    && value.bytes().enumerate().all(|(i, b)| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || (i > 0 && i + 1 < value.len() && matches!(b, b'.' | b'_' | b'-'))
                    })
                    && value.as_bytes()[0].is_ascii_alphanumeric()
                    && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric();
                if !valid {
                    return Err(ContractError::InvalidIdentifier);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = ContractError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

text_id!(AppId);
text_id!(PublisherId);
text_id!(AppVersion);

/// Maximum number of ASCII decimal digits in a canonical 128-bit identifier.
/// `u128::MAX` is 39 digits, so this bound is exact and never truncates.
pub const MAX_DECIMAL_DIGITS: usize = 39;

/// Opaque, nonzero 128-bit application launch-instance id, carried on the wire
/// as **canonical decimal digits**.
///
/// Managed-app v1 messages are internally tagged (`#[serde(tag = "type")]`), and
/// serde deserializes such an enum by buffering the whole payload into
/// `serde::__private::de::Content` before replaying it. That buffer has no
/// `visit_u128`, so a `u128` field is **undecodable** in that position: every
/// `hello` encoded cleanly and then failed to decode with `InvalidControl`, for
/// every value including `1`. The magnitude was never the problem — the buffer
/// was. Canonical decimal digits survive the buffer, are language-neutral,
/// retain the full 128-bit range, and are the representation ADR 0035 already
/// ratified for the private manager protocol, so the two contracts agree.
///
/// Canonical form is a **total function**: exactly one accepted spelling per id.
/// Sign, whitespace, leading zeros, exponent and fractional forms, non-ASCII
/// digits, `0`, and over-length input are all rejected. Two spellings of one id
/// would be a wire defect, not a tolerance.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AppInstanceId(String);

impl AppInstanceId {
    pub fn new(value: u128) -> Result<Self, ContractError> {
        if value == 0 {
            Err(ContractError::InvalidOpaqueId)
        } else {
            Ok(Self(value.to_string()))
        }
    }

    /// Parses the one canonical spelling of an instance id.
    ///
    /// Rejects the empty string, a length above [`MAX_DECIMAL_DIGITS`], any
    /// non-ASCII-digit byte, and any multi-digit value with a leading zero.
    /// `0` is rejected by [`AppInstanceId::new`] as an invalid id.
    pub fn parse(value: &str) -> Result<Self, ContractError> {
        if value.is_empty() || value.len() > MAX_DECIMAL_DIGITS {
            return Err(ContractError::InvalidOpaqueId);
        }
        if !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ContractError::InvalidOpaqueId);
        }
        if value.len() > 1 && value.starts_with('0') {
            return Err(ContractError::InvalidOpaqueId);
        }
        let parsed = value
            .parse::<u128>()
            .map_err(|_| ContractError::InvalidOpaqueId)?;
        Self::new(parsed)
    }

    /// The canonical decimal-digit spelling, which is also the on-wire form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn get(&self) -> u128 {
        // Both constructors admit only a canonical decimal spelling of a nonzero
        // `u128`, so this parse cannot fail. `expect` names the invariant rather
        // than substituting a value that would silently truncate an id.
        self.0
            .parse::<u128>()
            .expect("AppInstanceId always holds canonical decimal digits")
    }
}
impl TryFrom<u128> for AppInstanceId {
    type Error = ContractError;
    fn try_from(value: u128) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl TryFrom<&str> for AppInstanceId {
    type Error = ContractError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl TryFrom<String> for AppInstanceId {
    type Error = ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<AppInstanceId> for String {
    fn from(value: AppInstanceId) -> Self {
        value.0
    }
}
impl From<AppInstanceId> for u128 {
    fn from(value: AppInstanceId) -> Self {
        value.get()
    }
}
impl std::fmt::Display for AppInstanceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct OwnedResourceId(u128);
impl OwnedResourceId {
    pub fn new(value: u128) -> Result<Self, ContractError> {
        if value == 0 {
            Err(ContractError::InvalidOpaqueId)
        } else {
            Ok(Self(value))
        }
    }
    pub const fn get(self) -> u128 {
        self.0
    }
}
impl TryFrom<u128> for OwnedResourceId {
    type Error = ContractError;
    fn try_from(value: u128) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<OwnedResourceId> for u128 {
    fn from(value: OwnedResourceId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppPrincipal {
    pub app_id: AppId,
    pub instance_id: AppInstanceId,
    pub publisher_id: Option<PublisherId>,
}

/// A runtime-owned resource reference carries its owner alongside the opaque
/// handle so the handle cannot be interpreted outside an application domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalOwnedResource {
    principal: AppPrincipal,
    resource_id: OwnedResourceId,
}
impl PrincipalOwnedResource {
    pub fn new(principal: AppPrincipal, resource_id: OwnedResourceId) -> Self {
        Self {
            principal,
            resource_id,
        }
    }
    pub fn principal(&self) -> &AppPrincipal {
        &self.principal
    }
    pub const fn resource_id(&self) -> OwnedResourceId {
        self.resource_id
    }
}

/// Attribution for a caller that a future AppManager transport has already
/// authenticated as an administrator. This type does not authenticate its
/// caller; the value must be created only at that trusted boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdministratorPrincipal {
    session_id: u128,
}
impl AdministratorPrincipal {
    pub fn from_authenticated_session(session_id: u128) -> Result<Self, ContractError> {
        if session_id == 0 {
            return Err(ContractError::InvalidOpaqueId);
        }
        Ok(Self { session_id })
    }
    pub const fn session_id(&self) -> u128 {
        self.session_id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Sam,
    I2cp,
    ControlScoped,
    BrokeredTcp,
    UiBridge,
    Health,
    Lifecycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestedCapability {
    pub capability: Capability,
}

/// Administrator-origin grant. There is deliberately no public deserializer
/// or conversion from `RequestedCapability`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantedCapability {
    capability: Capability,
}
impl GrantedCapability {
    /// Call only after the separate administrator principal has passed
    /// authorization. This value itself is not an authentication token.
    pub fn from_administrator_policy(
        _administrator: &AdministratorPrincipal,
        capability: Capability,
    ) -> Result<Self, ContractError> {
        if capability == Capability::BrokeredTcp {
            return Err(ContractError::ReservedCapability);
        }
        Ok(Self { capability })
    }
    pub const fn capability(&self) -> Capability {
        self.capability
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveCapabilities(Vec<Capability>);
impl EffectiveCapabilities {
    pub fn from_grants(grants: &[GrantedCapability]) -> Result<Self, ContractError> {
        if grants.len() > MAX_CAPABILITIES {
            return Err(ContractError::LimitExceeded("capabilities"));
        }
        let mut values = grants.iter().map(|g| g.capability).collect::<Vec<_>>();
        values.sort_by_key(|v| *v as u8);
        values.dedup();
        Ok(Self(values))
    }
    pub fn as_slice(&self) -> &[Capability] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Application,
    Administrator,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum AppToHostMessage {
    #[serde(rename = "hello")]
    Hello {
        request_id: RequestId,
        app_id: AppId,
        instance_id: AppInstanceId,
        protocol_major: u8,
        protocol_minor: u8,
    },
    #[serde(rename = "open")]
    Open {
        request_id: RequestId,
        stream_id: u32,
        service: AppService,
    },
    #[serde(rename = "permission_request")]
    PermissionRequest {
        request_id: RequestId,
        capabilities: Vec<RequestedCapability>,
    },
    #[serde(rename = "close")]
    Close { stream_id: u32 },
    #[serde(rename = "reset")]
    Reset { stream_id: u32, reason: String },
    #[serde(rename = "ui_message")]
    UiMessage { message_id: u32, payload: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum HostToAppMessage {
    #[serde(rename = "reply")]
    Reply {
        request_id: RequestId,
        outcome: AppRequestOutcome,
    },
    #[serde(rename = "permission_reply")]
    PermissionReply {
        request_id: RequestId,
        status: PermissionStatus,
    },
    #[serde(rename = "stream_closed")]
    StreamClosed { stream_id: u32 },
    #[serde(rename = "stream_reset")]
    StreamReset { stream_id: u32, reason: String },
    #[serde(rename = "capabilities")]
    Capabilities { capabilities: Vec<Capability> },
    #[serde(rename = "health")]
    Health {
        state: String,
        detail: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    content = "error",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AppRequestOutcome {
    Succeeded,
    Failed(RequestError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestErrorCode {
    UnsupportedOperation,
    PermissionDenied,
    InvalidRequest,
    ResourceLimit,
    Conflict,
    NotFound,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestError {
    pub code: RequestErrorCode,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum AdminToHostMessage {
    #[serde(rename = "reserved_request")]
    ReservedRequest {
        request_id: RequestId,
        operation: ReservedAdminOperation,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservedAdminOperation {
    Install,
    Update,
    Uninstall,
    Launch,
    Stop,
    Grant,
    Revoke,
    NetworkPolicy,
    LaunchProfile,
    ResourcePolicy,
    Inspect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum HostToAdminMessage {
    #[serde(rename = "reply")]
    Reply {
        request_id: RequestId,
        error: RequestError,
    },
}
impl HostToAdminMessage {
    pub fn unsupported_admin_operation(request_id: RequestId) -> Self {
        Self::Reply {
            request_id,
            error: RequestError {
                code: RequestErrorCode::UnsupportedOperation,
                diagnostic: None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppService {
    Sam,
    I2cp,
    ControlScoped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Pending,
    Denied,
    Recorded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handshake {
    pub role: Role,
    pub major: u8,
    pub minor: u8,
}
pub const HANDSHAKE_MAGIC: [u8; 4] = *b"I2PA";
impl Handshake {
    pub fn encode(&self) -> [u8; 9] {
        [
            HANDSHAKE_MAGIC[0],
            HANDSHAKE_MAGIC[1],
            HANDSHAKE_MAGIC[2],
            HANDSHAKE_MAGIC[3],
            self.major,
            self.minor,
            match self.role {
                Role::Application => 1,
                Role::Administrator => 2,
            },
            0,
            0,
        ]
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != 9 || bytes[..4] != HANDSHAKE_MAGIC || bytes[7..9] != [0, 0] {
            return Err(ContractError::UnsupportedVersion);
        }
        let role = match bytes[6] {
            1 => Role::Application,
            2 => Role::Administrator,
            _ => return Err(ContractError::RoleMismatch),
        };
        if bytes[4] != PROTOCOL_MAJOR {
            return Err(ContractError::UnsupportedVersion);
        }
        Ok(Self {
            role,
            major: bytes[4],
            minor: bytes[5],
        })
    }
    pub fn decode_app_to_host_message(
        &self,
        bytes: &[u8],
    ) -> Result<AppToHostMessage, ContractError> {
        if self.role != Role::Application {
            return Err(ContractError::RoleMismatch);
        }
        decode_app_to_host_control(bytes)
    }
    pub fn decode_host_to_app_message(
        &self,
        bytes: &[u8],
    ) -> Result<HostToAppMessage, ContractError> {
        if self.role != Role::Application {
            return Err(ContractError::RoleMismatch);
        }
        decode_host_to_app_control(bytes)
    }
    pub fn decode_admin_to_host_message(
        &self,
        bytes: &[u8],
    ) -> Result<AdminToHostMessage, ContractError> {
        if self.role != Role::Administrator {
            return Err(ContractError::RoleMismatch);
        }
        decode_admin_to_host_control(bytes)
    }
    pub fn decode_host_to_admin_message(
        &self,
        bytes: &[u8],
    ) -> Result<HostToAdminMessage, ContractError> {
        if self.role != Role::Administrator {
            return Err(ContractError::RoleMismatch);
        }
        decode_host_to_admin_control(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Control,
    Data,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub stream_id: u32,
    pub payload: Vec<u8>,
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        if self.payload.len() > MAX_FRAME_PAYLOAD_BYTES {
            return Err(ContractError::LimitExceeded("frame payload"));
        }
        if (self.kind == FrameKind::Control) != (self.stream_id == 0) {
            return Err(ContractError::MalformedFrame);
        }
        let len = u32::try_from(self.payload.len())
            .map_err(|_| ContractError::LimitExceeded("frame payload"))?;
        let mut out = Vec::with_capacity(FRAME_HEADER_BYTES + self.payload.len());
        out.extend_from_slice(&[
            1,
            if self.kind == FrameKind::Control {
                1
            } else {
                2
            },
            0,
            0,
        ]);
        out.extend_from_slice(&self.stream_id.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }
    pub fn decode(input: &[u8]) -> Result<(Self, usize), ContractError> {
        if input.len() < FRAME_HEADER_BYTES {
            return Err(ContractError::TruncatedFrame);
        }
        if input[0] != 1 {
            return Err(ContractError::UnsupportedFrame);
        }
        let kind = match input[1] {
            1 => FrameKind::Control,
            2 => FrameKind::Data,
            _ => return Err(ContractError::UnsupportedFrame),
        };
        if input[2] != 0 || input[3] != 0 {
            return Err(ContractError::MalformedFrame);
        }
        let stream_id = u32::from_be_bytes(
            input[4..8]
                .try_into()
                .map_err(|_| ContractError::MalformedFrame)?,
        );
        let payload_len = u32::from_be_bytes(
            input[8..12]
                .try_into()
                .map_err(|_| ContractError::MalformedFrame)?,
        ) as usize;
        if payload_len > MAX_FRAME_PAYLOAD_BYTES {
            return Err(ContractError::LimitExceeded("frame payload"));
        }
        if (kind == FrameKind::Control) != (stream_id == 0) {
            return Err(ContractError::MalformedFrame);
        }
        let total = FRAME_HEADER_BYTES
            .checked_add(payload_len)
            .ok_or(ContractError::MalformedFrame)?;
        if input.len() < total {
            return Err(ContractError::TruncatedFrame);
        }
        Ok((
            Self {
                kind,
                stream_id,
                payload: input[12..total].to_vec(),
            },
            total,
        ))
    }
}

fn encode_directional_control<T: Serialize>(message: &T) -> Result<Vec<u8>, ContractError> {
    let bytes = serde_json::to_vec(message).map_err(|_| ContractError::InvalidControl)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ContractError::LimitExceeded("control"));
    }
    Ok(bytes)
}
fn decode_directional_control<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ContractError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ContractError::LimitExceeded("control"));
    }
    serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidControl)
}
pub fn encode_app_to_host_control(message: &AppToHostMessage) -> Result<Vec<u8>, ContractError> {
    validate_app_to_host_message(message)?;
    encode_directional_control(message)
}
pub fn decode_app_to_host_control(bytes: &[u8]) -> Result<AppToHostMessage, ContractError> {
    let message = decode_directional_control(bytes)?;
    validate_app_to_host_message(&message)?;
    Ok(message)
}
pub fn encode_host_to_app_control(message: &HostToAppMessage) -> Result<Vec<u8>, ContractError> {
    validate_host_to_app_message(message)?;
    encode_directional_control(message)
}
pub fn decode_host_to_app_control(bytes: &[u8]) -> Result<HostToAppMessage, ContractError> {
    let message = decode_directional_control(bytes)?;
    validate_host_to_app_message(&message)?;
    Ok(message)
}
pub fn encode_admin_to_host_control(
    message: &AdminToHostMessage,
) -> Result<Vec<u8>, ContractError> {
    validate_admin_to_host_message(message)?;
    encode_directional_control(message)
}
pub fn decode_admin_to_host_control(bytes: &[u8]) -> Result<AdminToHostMessage, ContractError> {
    let message = decode_directional_control(bytes)?;
    validate_admin_to_host_message(&message)?;
    Ok(message)
}
pub fn encode_host_to_admin_control(
    message: &HostToAdminMessage,
) -> Result<Vec<u8>, ContractError> {
    validate_host_to_admin_message(message)?;
    encode_directional_control(message)
}
pub fn decode_host_to_admin_control(bytes: &[u8]) -> Result<HostToAdminMessage, ContractError> {
    let message = decode_directional_control(bytes)?;
    validate_host_to_admin_message(&message)?;
    Ok(message)
}
pub fn validate_app_to_host_message(message: &AppToHostMessage) -> Result<(), ContractError> {
    match message {
        AppToHostMessage::Hello { protocol_major, .. } => {
            if *protocol_major == PROTOCOL_MAJOR {
                Ok(())
            } else {
                Err(ContractError::UnsupportedVersion)
            }
        }
        AppToHostMessage::PermissionRequest { capabilities, .. } => {
            validate_requested_capabilities(capabilities)
        }
        AppToHostMessage::Open { stream_id, .. } => {
            if *stream_id == 0 {
                return Err(ContractError::InvalidControl);
            }
            Ok(())
        }
        AppToHostMessage::Close { stream_id } | AppToHostMessage::Reset { stream_id, .. } => {
            if *stream_id == 0 {
                return Err(ContractError::InvalidControl);
            }
            if let AppToHostMessage::Reset { reason, .. } = message {
                bounded_string(reason, MAX_DIAGNOSTIC_BYTES)
            } else {
                Ok(())
            }
        }
        AppToHostMessage::UiMessage {
            message_id,
            payload,
        } => {
            if *message_id == 0 {
                return Err(ContractError::InvalidControl);
            }
            bounded_string(payload, MAX_UI_MESSAGE_BYTES)?;
            serde_json::from_str::<serde_json::Value>(payload)
                .map(|_| ())
                .map_err(|_| ContractError::InvalidControl)
        }
    }
}

pub fn validate_host_to_app_message(message: &HostToAppMessage) -> Result<(), ContractError> {
    match message {
        HostToAppMessage::Reply { outcome, .. } => validate_request_outcome(outcome),
        HostToAppMessage::StreamClosed { stream_id } => {
            if *stream_id == 0 {
                Err(ContractError::InvalidControl)
            } else {
                Ok(())
            }
        }
        HostToAppMessage::StreamReset { stream_id, reason } => {
            if *stream_id == 0 {
                return Err(ContractError::InvalidControl);
            }
            bounded_string(reason, MAX_DIAGNOSTIC_BYTES)
        }
        HostToAppMessage::Capabilities { capabilities } => {
            validate_capabilities(capabilities)?;
            if capabilities.contains(&Capability::BrokeredTcp) {
                return Err(ContractError::ReservedCapability);
            }
            Ok(())
        }
        HostToAppMessage::Health { state, detail } => {
            bounded_string(state, 64)?;
            if let Some(v) = detail {
                bounded_string(v, MAX_DIAGNOSTIC_BYTES)?;
            }
            Ok(())
        }
        HostToAppMessage::PermissionReply { .. } => Ok(()),
    }
}

fn validate_request_error(error: &RequestError) -> Result<(), ContractError> {
    if let Some(diagnostic) = &error.diagnostic {
        bounded_string(diagnostic, MAX_DIAGNOSTIC_BYTES)?;
    }
    Ok(())
}
fn validate_request_outcome(outcome: &AppRequestOutcome) -> Result<(), ContractError> {
    if let AppRequestOutcome::Failed(error) = outcome {
        validate_request_error(error)?;
    }
    Ok(())
}
pub fn validate_admin_to_host_message(_message: &AdminToHostMessage) -> Result<(), ContractError> {
    Ok(())
}
pub fn validate_host_to_admin_message(message: &HostToAdminMessage) -> Result<(), ContractError> {
    match message {
        HostToAdminMessage::Reply { error, .. } => {
            if error.code != RequestErrorCode::UnsupportedOperation {
                return Err(ContractError::InvalidControl);
            }
            validate_request_error(error)
        }
    }
}

/// Pure per-session accounting helper. A transport owner is responsible for
/// serializing access and applying lifecycle cleanup; this type owns no I/O.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLimits {
    streams: std::collections::BTreeSet<u32>,
    requests: std::collections::BTreeSet<u32>,
}
impl SessionLimits {
    pub fn open_stream(&mut self, stream_id: u32) -> Result<(), ContractError> {
        if stream_id == 0 || self.streams.contains(&stream_id) {
            return Err(ContractError::InvalidControl);
        }
        if self.streams.len() >= MAX_STREAMS {
            return Err(ContractError::LimitExceeded("streams"));
        }
        self.streams.insert(stream_id);
        Ok(())
    }
    pub fn close_stream(&mut self, stream_id: u32) {
        self.streams.remove(&stream_id);
    }
    pub fn register_request(&mut self, request_id: u32) -> Result<(), ContractError> {
        if request_id == 0 || self.requests.contains(&request_id) {
            return Err(ContractError::InvalidControl);
        }
        if self.requests.len() >= MAX_INFLIGHT_REQUESTS {
            return Err(ContractError::LimitExceeded("in-flight requests"));
        }
        self.requests.insert(request_id);
        Ok(())
    }
    pub fn complete_request(&mut self, request_id: RequestId) -> Result<(), ContractError> {
        if self.requests.remove(&request_id.get()) {
            Ok(())
        } else {
            Err(ContractError::UnmatchedRequest)
        }
    }
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }
    pub fn inflight_count(&self) -> usize {
        self.requests.len()
    }
}
fn validate_capabilities(values: &[Capability]) -> Result<(), ContractError> {
    if values.len() > MAX_CAPABILITIES {
        return Err(ContractError::LimitExceeded("capabilities"));
    }
    let unique = values
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(ContractError::InvalidControl);
    }
    Ok(())
}
fn validate_requested_capabilities(values: &[RequestedCapability]) -> Result<(), ContractError> {
    if values.len() > MAX_CAPABILITIES {
        return Err(ContractError::LimitExceeded("capabilities"));
    }
    let unique = values
        .iter()
        .map(|value| value.capability)
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(ContractError::InvalidControl);
    }
    Ok(())
}
fn bounded_string(value: &str, max: usize) -> Result<(), ContractError> {
    if value.len() > max {
        Err(ContractError::LimitExceeded("string"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionPair {
    pub major: u16,
    pub minor: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackagePath(String);
impl PackagePath {
    pub fn parse(value: impl Into<String>) -> Result<Self, ContractError> {
        let value = value.into();
        let invalid = value.is_empty()
            || value.len() > 512
            || value.starts_with('/')
            || value.contains('\\')
            || value.contains(':')
            || value.contains('%')
            || value.contains('?')
            || value.contains('#')
            || !value.is_ascii()
            || value.bytes().any(|b| b == 0 || b.is_ascii_control())
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
            || value
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..");
        if invalid {
            return Err(ContractError::InvalidPath);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for PackagePath {
    type Error = ContractError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::parse(v)
    }
}
impl From<PackagePath> for String {
    fn from(v: PackagePath) -> Self {
        v.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entrypoint {
    pub target: String,
    pub path: PackagePath,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequest {
    pub name: String,
    pub requested: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiDescriptor {
    pub entrypoint: PackagePath,
    pub max_message_bytes: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u16,
    pub app_id: AppId,
    pub publisher_id: PublisherId,
    pub version: AppVersion,
    pub name: String,
    pub description: String,
    pub host_protocol_min: VersionPair,
    pub host_protocol_max: VersionPair,
    pub entrypoints: Vec<Entrypoint>,
    pub requested_capabilities: Vec<RequestedCapability>,
    pub resources: Vec<ResourceRequest>,
    pub ui: Option<UiDescriptor>,
    pub autostart_requested: bool,
    pub restart_requested: bool,
}
impl Manifest {
    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(ContractError::LimitExceeded("manifest"));
        }
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidManifest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != 1 || self.host_protocol_min > self.host_protocol_max {
            return Err(ContractError::InvalidManifest);
        }
        if self.name.is_empty() || self.name.chars().count() > 128 {
            return Err(ContractError::InvalidManifest);
        }
        if self.description.chars().count() > 1_024 {
            return Err(ContractError::InvalidManifest);
        }
        if self.entrypoints.is_empty() || self.entrypoints.len() > MAX_ENTRYPOINTS {
            return Err(ContractError::LimitExceeded("entrypoints"));
        }
        if self.resources.len() > MAX_RESOURCE_ENTRIES {
            return Err(ContractError::LimitExceeded("resources"));
        }
        validate_requested_capabilities(&self.requested_capabilities).map_err(
            |error| match error {
                ContractError::LimitExceeded(limit) => ContractError::LimitExceeded(limit),
                _ => ContractError::InvalidManifest,
            },
        )?;
        if let Some(ui) = &self.ui
            && (ui.max_message_bytes == 0 || ui.max_message_bytes as usize > MAX_UI_MESSAGE_BYTES)
        {
            return Err(ContractError::InvalidManifest);
        }
        let mut targets = std::collections::BTreeSet::new();
        for entry in &self.entrypoints {
            validate_identifier(&entry.target)?;
            if !targets.insert(&entry.target) {
                return Err(ContractError::InvalidManifest);
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for resource in &self.resources {
            validate_identifier(&resource.name)?;
            if resource.requested > MAX_RESOURCE_REQUEST {
                return Err(ContractError::LimitExceeded("resource request"));
            }
            if !names.insert(&resource.name) {
                return Err(ContractError::InvalidManifest);
            }
        }
        Ok(())
    }
}
fn validate_identifier(s: &str) -> Result<(), ContractError> {
    AppId::parse(s).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProtocol {
    Tcp,
    Udp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DestinationSelector {
    Hostname(String),
    Ip(IpAddr),
    Cidr(IpCidr),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpCidr {
    pub network: IpAddr,
    pub prefix: u8,
}
impl IpCidr {
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(a), IpAddr::V4(b)) => {
                self.prefix <= 32
                    && (self.prefix == 0
                        || (u32::from(a) ^ u32::from(b)) >> (32 - self.prefix) == 0)
            }
            (IpAddr::V6(a), IpAddr::V6(b)) => {
                self.prefix <= 128
                    && (self.prefix == 0
                        || (u128::from(a) ^ u128::from(b)) >> (128 - self.prefix) == 0)
            }
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PortSelector {
    Single(u16),
    Range { first: u16, last: u16 },
}
impl PortSelector {
    fn contains(self, port: u16) -> bool {
        match self {
            Self::Single(p) => p == port,
            Self::Range { first, last } => first <= port && port <= last,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Allow,
    Deny,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkRule {
    pub protocol: NetworkProtocol,
    pub destination: DestinationSelector,
    pub ports: PortSelector,
    pub action: RuleAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressScope {
    Global,
    Loopback,
    Private,
    LinkLocal,
    Multicast,
    Broadcast,
    Documentation,
    Benchmark,
    SpecialPurpose,
    Reserved,
    Unspecified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Deny,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPolicy {
    pub rules: Vec<NetworkRule>,
}
impl NetworkPolicy {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.rules.len() > MAX_NETWORK_RULES {
            return Err(ContractError::LimitExceeded("network rules"));
        }
        for rule in &self.rules {
            if rule.protocol != NetworkProtocol::Tcp {
                return Err(ContractError::InvalidPolicy);
            }
            if let PortSelector::Range { first, last } = rule.ports
                && (first == 0 || first > last)
            {
                return Err(ContractError::InvalidPolicy);
            }
            match &rule.destination {
                DestinationSelector::Hostname(host) => {
                    bounded_string(host, 253)?;
                    if !valid_hostname(host) {
                        return Err(ContractError::InvalidPolicy);
                    }
                }
                DestinationSelector::Cidr(cidr) => {
                    if matches!(cidr.network, IpAddr::V4(_)) && cidr.prefix > 32
                        || matches!(cidr.network, IpAddr::V6(_)) && cidr.prefix > 128
                        || is_ipv4_mapped_ipv6(cidr.network)
                    {
                        return Err(ContractError::InvalidPolicy);
                    }
                }
                DestinationSelector::Ip(ip) if is_ipv4_mapped_ipv6(*ip) => {
                    return Err(ContractError::InvalidPolicy);
                }
                DestinationSelector::Ip(_) => {}
            }
            if matches!(rule.ports, PortSelector::Single(0)) {
                return Err(ContractError::InvalidPolicy);
            }
        }
        Ok(())
    }
    pub fn evaluate_hostname(
        &self,
        protocol: NetworkProtocol,
        hostname: &str,
        port: u16,
    ) -> PolicyDecision {
        if self.validate().is_err()
            || protocol != NetworkProtocol::Tcp
            || port == 0
            || !valid_hostname(hostname)
        {
            return PolicyDecision::Deny;
        }
        self.decide(self.rules.iter().filter(|r| r.protocol == protocol && r.ports.contains(port) && matches!(&r.destination, DestinationSelector::Hostname(h) if valid_hostname(h) && h == hostname)))
    }
    pub fn evaluate_requested_ip(
        &self,
        protocol: NetworkProtocol,
        ip: IpAddr,
        port: u16,
    ) -> PolicyDecision {
        if self.validate().is_err() || protocol != NetworkProtocol::Tcp || port == 0 {
            return PolicyDecision::Deny;
        }
        let ip = canonical_policy_ip(ip);
        let scope = address_scope(ip);
        let matching = self.matching_ip_rules(protocol, ip, port);
        if scope != AddressScope::Global && !matching.iter().any(|r| r.action == RuleAction::Allow)
        {
            return PolicyDecision::Deny;
        }
        self.decide(matching.into_iter())
    }
    pub fn evaluate_resolved_address(
        &self,
        protocol: NetworkProtocol,
        hostname: &str,
        ip: IpAddr,
        port: u16,
    ) -> PolicyDecision {
        if self.evaluate_hostname(protocol, hostname, port) == PolicyDecision::Deny {
            return PolicyDecision::Deny;
        }
        if self.validate().is_err()
            || protocol != NetworkProtocol::Tcp
            || port == 0
            || !valid_hostname(hostname)
        {
            return PolicyDecision::Deny;
        }
        let ip = canonical_policy_ip(ip);
        let matching = self.matching_ip_rules(protocol, ip, port);
        if matching.iter().any(|rule| rule.action == RuleAction::Deny) {
            return PolicyDecision::Deny;
        }
        if address_scope(ip) == AddressScope::Global
            || matching.iter().any(|rule| rule.action == RuleAction::Allow)
        {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Deny
        }
    }
    fn matching_ip_rules(
        &self,
        protocol: NetworkProtocol,
        ip: IpAddr,
        port: u16,
    ) -> Vec<&NetworkRule> {
        self.rules
            .iter()
            .filter(|rule| {
                rule.protocol == protocol
                    && rule.ports.contains(port)
                    && match &rule.destination {
                        DestinationSelector::Ip(value) => *value == ip,
                        DestinationSelector::Cidr(cidr) => cidr.contains(ip),
                        DestinationSelector::Hostname(_) => false,
                    }
            })
            .collect()
    }
    fn decide<'a>(&self, rules: impl Iterator<Item = &'a NetworkRule>) -> PolicyDecision {
        let mut allow = false;
        for r in rules {
            if r.action == RuleAction::Deny {
                return PolicyDecision::Deny;
            }
            allow = true;
        }
        if allow {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Deny
        }
    }
}
fn valid_hostname(host: &str) -> bool {
    if host.is_empty() || !host.is_ascii() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && (label.as_bytes()[0].is_ascii_lowercase() || label.as_bytes()[0].is_ascii_digit())
            && (label.as_bytes()[label.len() - 1].is_ascii_lowercase()
                || label.as_bytes()[label.len() - 1].is_ascii_digit())
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

fn canonical_policy_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(value) => value
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(value)),
        IpAddr::V4(_) => ip,
    }
}

fn is_ipv4_mapped_ipv6(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(value) if value.to_ipv4_mapped().is_some())
}

pub fn address_scope(ip: IpAddr) -> AddressScope {
    match ip {
        IpAddr::V4(v) => {
            let octets = v.octets();
            if v.is_unspecified() {
                AddressScope::Unspecified
            } else if v.is_loopback() {
                AddressScope::Loopback
            } else if v == Ipv4Addr::BROADCAST {
                AddressScope::Broadcast
            } else if v.is_multicast() {
                AddressScope::Multicast
            } else if v.is_link_local() {
                AddressScope::LinkLocal
            } else if v.is_private() || (octets[0] == 100 && (64..=127).contains(&octets[1])) {
                AddressScope::Private
            } else if ipv4_in_prefix(v, [192, 0, 2, 0], 24)
                || ipv4_in_prefix(v, [198, 51, 100, 0], 24)
                || ipv4_in_prefix(v, [203, 0, 113, 0], 24)
            {
                AddressScope::Documentation
            } else if ipv4_in_prefix(v, [198, 18, 0, 0], 15) {
                AddressScope::Benchmark
            } else if octets[0] == 0
                || ipv4_in_prefix(v, [192, 0, 0, 0], 24)
                || ipv4_in_prefix(v, [192, 31, 196, 0], 24)
                || ipv4_in_prefix(v, [192, 52, 193, 0], 24)
                || ipv4_in_prefix(v, [192, 88, 99, 0], 24)
                || ipv4_in_prefix(v, [192, 175, 48, 0], 24)
            {
                AddressScope::SpecialPurpose
            } else if octets[0] >= 240 {
                AddressScope::Reserved
            } else {
                AddressScope::Global
            }
        }
        IpAddr::V6(v) => {
            if let Some(mapped) = v.to_ipv4_mapped() {
                return address_scope(IpAddr::V4(mapped));
            }
            if v.is_unspecified() {
                AddressScope::Unspecified
            } else if v.is_loopback() {
                AddressScope::Loopback
            } else if v.is_multicast() {
                AddressScope::Multicast
            } else if v.is_unicast_link_local() {
                AddressScope::LinkLocal
            } else if (v.segments()[0] & 0xfe00) == 0xfc00 {
                AddressScope::Private
            } else if ipv6_in_prefix(v, [0x2001, 0x0db8, 0, 0, 0, 0, 0, 0], 32)
                || ipv6_in_prefix(v, [0x3fff, 0, 0, 0, 0, 0, 0, 0], 20)
            {
                AddressScope::Documentation
            } else if ipv6_in_prefix(v, [0x2001, 0x0002, 0, 0, 0, 0, 0, 0], 48) {
                AddressScope::Benchmark
            } else if ipv6_in_prefix(v, [0x2001, 0, 0, 0, 0, 0, 0, 0], 23)
                || ipv6_in_prefix(v, [0x0064, 0xff9b, 0, 0, 0, 0, 0, 0], 96)
                || ipv6_in_prefix(v, [0x0064, 0xff9b, 0x0001, 0, 0, 0, 0, 0], 48)
                || ipv6_in_prefix(v, [0x0100, 0, 0, 0, 0, 0, 0, 0], 64)
                || ipv6_in_prefix(v, [0x0100, 0, 0, 1, 0, 0, 0, 0], 64)
                || ipv6_in_prefix(v, [0x2002, 0, 0, 0, 0, 0, 0, 0], 16)
            {
                AddressScope::SpecialPurpose
            } else if v.segments()[0] & 0xe000 != 0x2000 {
                AddressScope::Reserved
            } else {
                AddressScope::Global
            }
        }
    }
}

fn ipv4_in_prefix(address: Ipv4Addr, network: [u8; 4], prefix: u8) -> bool {
    let address = u32::from(address);
    let network = u32::from(Ipv4Addr::from(network));
    (address ^ network) >> (32 - prefix) == 0
}
fn ipv6_in_prefix(address: std::net::Ipv6Addr, network: [u16; 8], prefix: u8) -> bool {
    let address = u128::from(address);
    let network = u128::from(std::net::Ipv6Addr::from(network));
    (address ^ network) >> (128 - prefix) == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchProfile {
    Secured,
    UnsafeDirect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProperty {
    DirectNetworkDenied,
    LoopbackDenied,
    PrivateFilesystem,
    ProcessInspectionContained,
    ChildTreeContained,
    ResourceLimitsInstalled,
    EnvironmentSanitized,
    BrokerChannelInstalled,
}
pub const REQUIRED_SECURED_PROPERTIES: [SandboxProperty; 8] = [
    SandboxProperty::DirectNetworkDenied,
    SandboxProperty::LoopbackDenied,
    SandboxProperty::PrivateFilesystem,
    SandboxProperty::ProcessInspectionContained,
    SandboxProperty::ChildTreeContained,
    SandboxProperty::ResourceLimitsInstalled,
    SandboxProperty::EnvironmentSanitized,
    SandboxProperty::BrokerChannelInstalled,
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxAttestation {
    pub backend_kind: String,
    pub backend_version: String,
    pub evidence_generation: u64,
    pub properties: Vec<SandboxProperty>,
}
impl SandboxAttestation {
    pub fn validate_secured(&self) -> Result<(), ContractError> {
        for s in [&self.backend_kind, &self.backend_version] {
            bounded_string(s, 128)?;
            if s.is_empty() {
                return Err(ContractError::IncompleteAttestation);
            }
        }
        if self.properties.len() > 16 {
            return Err(ContractError::LimitExceeded("attestation fields"));
        }
        let present = self
            .properties
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if present.len() != self.properties.len() {
            return Err(ContractError::IncompleteAttestation);
        }
        if REQUIRED_SECURED_PROPERTIES
            .iter()
            .any(|p| !present.contains(p))
        {
            return Err(ContractError::IncompleteAttestation);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn minimal_manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            app_id: AppId::parse("sample-app").unwrap(),
            publisher_id: PublisherId::parse("sample-publisher").unwrap(),
            version: AppVersion::parse("1.0.0").unwrap(),
            name: "Sample".into(),
            description: String::new(),
            host_protocol_min: VersionPair { major: 1, minor: 0 },
            host_protocol_max: VersionPair { major: 1, minor: 0 },
            entrypoints: vec![Entrypoint {
                target: "linux-x86_64".into(),
                path: PackagePath::parse("bin/sample").unwrap(),
            }],
            requested_capabilities: vec![RequestedCapability {
                capability: Capability::Sam,
            }],
            resources: vec![ResourceRequest {
                name: "memory_bytes".into(),
                requested: 1024,
            }],
            ui: Some(UiDescriptor {
                entrypoint: PackagePath::parse("ui/index.html").unwrap(),
                max_message_bytes: 1024,
            }),
            autostart_requested: false,
            restart_requested: false,
        }
    }

    #[test]
    fn identifiers_and_package_paths_are_bounded_and_strict() {
        assert!(AppId::parse("a").is_ok());
        assert!(AppId::parse(format!("a{}", "x".repeat(63))).is_ok());
        assert!(AppId::parse("A").is_err());
        assert!(AppId::parse(format!("a{}", "x".repeat(64))).is_err());
        assert!(AppId::parse("a..b").is_ok());
        assert!(AppId::parse("a/b").is_err());
        assert!(AppId::parse("aé").is_err());
        assert!(PackagePath::parse("bin/app").is_ok());
        for bad in [
            "../app", "/bin/app", "a\\b", "http://x", "a//b", "a%2f..", "a?x", "aé",
        ] {
            assert!(PackagePath::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn manifest_schema_and_every_collection_ceiling_are_strict() {
        let manifest = minimal_manifest();
        assert!(manifest.validate().is_ok());
        let encoded = serde_json::to_vec(&manifest).unwrap();
        assert_eq!(Manifest::decode(&encoded).unwrap(), manifest);
        assert!(Manifest::decode(br#"{"schema_version":1,"schema_version":1}"#).is_err());

        let mut bad = manifest.clone();
        bad.entrypoints = (0..9)
            .map(|index| Entrypoint {
                target: format!("target-{index}"),
                path: PackagePath::parse("bin/app").unwrap(),
            })
            .collect();
        assert_eq!(
            bad.validate(),
            Err(ContractError::LimitExceeded("entrypoints"))
        );
        let mut bad = manifest.clone();
        bad.resources = (0..17)
            .map(|index| ResourceRequest {
                name: format!("resource-{index}"),
                requested: 1,
            })
            .collect();
        assert_eq!(
            bad.validate(),
            Err(ContractError::LimitExceeded("resources"))
        );
        let mut bad = manifest.clone();
        bad.resources[0].requested = MAX_RESOURCE_REQUEST + 1;
        assert_eq!(
            bad.validate(),
            Err(ContractError::LimitExceeded("resource request"))
        );
        assert!(Manifest::decode(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
        let mut bad = manifest;
        bad.name = "n".repeat(129);
        assert!(bad.validate().is_err());
        let mut bad = minimal_manifest();
        bad.requested_capabilities = vec![
            RequestedCapability {
                capability: Capability::Sam,
            };
            MAX_CAPABILITIES + 1
        ];
        assert_eq!(
            bad.validate(),
            Err(ContractError::LimitExceeded("capabilities"))
        );
    }

    #[test]
    fn role_and_capability_separation_is_fail_closed() {
        assert!(
            decode_app_to_host_control(
                br#"{"type":"grant_request","app_id":"a","capabilities":[]}"#
            )
            .is_err()
        );
        assert!(
            decode_app_to_host_control(
                br#"{"type":"permission_request","request_id":1,"capabilities":["sam"],"extra":1}"#
            )
            .is_err()
        );
        assert!(
            decode_app_to_host_control(
                br#"{"type":"open","request_id":0,"stream_id":1,"service":"sam"}"#
            )
            .is_err()
        );
        assert!(
            decode_app_to_host_control(
                br#"{"type":"open","request_id":1,"stream_id":1,"service":"brokered_tcp"}"#
            )
            .is_err()
        );
        let app = Handshake {
            role: Role::Application,
            major: 1,
            minor: 0,
        };
        assert_eq!(
            app.decode_admin_to_host_message(
                br#"{"type":"reserved_request","request_id":1,"operation":"inspect"}"#
            ),
            Err(ContractError::RoleMismatch)
        );
        assert!(
            app.decode_host_to_app_message(br#"{"type":"health","state":"ready"}"#)
                .is_ok()
        );
        assert!(
            app.decode_app_to_host_message(br#"{"type":"health","state":"ready"}"#)
                .is_err()
        );
        assert_eq!(
            app.decode_admin_to_host_message(
                br#"{"type":"reserved_request","request_id":1,"operation":"inspect"}"#
            ),
            Err(ContractError::RoleMismatch)
        );
        assert_eq!(
            app.decode_host_to_admin_message(
                br#"{"type":"reply","request_id":1,"error":{"code":"unsupported_operation","diagnostic":null}}"#
            ),
            Err(ContractError::RoleMismatch)
        );

        let admin_handshake = Handshake {
            role: Role::Administrator,
            major: 1,
            minor: 0,
        };
        assert!(
            admin_handshake
                .decode_admin_to_host_message(
                    br#"{"type":"reserved_request","request_id":1,"operation":"grant"}"#
                )
                .is_ok()
        );
        assert!(
            admin_handshake
                .decode_admin_to_host_message(
                    br#"{"type":"grant_request","app_id":"a","capabilities":["sam"]}"#
                )
                .is_err()
        );
        assert_eq!(
            admin_handshake.decode_app_to_host_message(
                br#"{"type":"permission_request","request_id":1,"capabilities":["sam"]}"#
            ),
            Err(ContractError::RoleMismatch)
        );
        assert_eq!(
            admin_handshake.decode_host_to_app_message(br#"{"type":"health","state":"ready"}"#),
            Err(ContractError::RoleMismatch)
        );
        assert!(admin_handshake
            .decode_host_to_admin_message(
                br#"{"type":"reply","request_id":1,"error":{"code":"unsupported_operation","diagnostic":null}}"#
            )
            .is_ok());
        assert!(
            admin_handshake
                .decode_host_to_admin_message(br#"{"type":"health","state":"ready"}"#)
                .is_err()
        );

        let administrator = AdministratorPrincipal::from_authenticated_session(1).unwrap();
        let grant =
            GrantedCapability::from_administrator_policy(&administrator, Capability::Sam).unwrap();
        assert_eq!(
            EffectiveCapabilities::from_grants(&[grant])
                .unwrap()
                .as_slice(),
            &[Capability::Sam]
        );
        assert_eq!(
            GrantedCapability::from_administrator_policy(&administrator, Capability::BrokeredTcp),
            Err(ContractError::ReservedCapability)
        );
        assert_eq!(
            encode_host_to_app_control(&HostToAppMessage::Capabilities {
                capabilities: vec![Capability::BrokeredTcp],
            }),
            Err(ContractError::ReservedCapability)
        );
    }

    #[test]
    fn directional_replies_are_correlated_typed_and_bounded() {
        let id = RequestId::new(17).unwrap();
        let hello = AppToHostMessage::Hello {
            request_id: id,
            app_id: AppId::parse("sample-app").unwrap(),
            instance_id: AppInstanceId::new(7).unwrap(),
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
        };
        assert!(encode_app_to_host_control(&hello).is_ok());
        let bad_version = AppToHostMessage::Hello {
            request_id: id,
            app_id: AppId::parse("sample-app").unwrap(),
            instance_id: AppInstanceId::new(7).unwrap(),
            protocol_major: PROTOCOL_MAJOR + 1,
            protocol_minor: PROTOCOL_MINOR,
        };
        assert_eq!(
            encode_app_to_host_control(&bad_version),
            Err(ContractError::UnsupportedVersion)
        );
        let success = HostToAppMessage::Reply {
            request_id: id,
            outcome: AppRequestOutcome::Succeeded,
        };
        let wire = encode_host_to_app_control(&success).unwrap();
        assert_eq!(
            wire,
            br#"{"type":"reply","request_id":17,"outcome":{"outcome":"succeeded"}}"#
        );
        assert_eq!(decode_host_to_app_control(&wire).unwrap(), success);

        let failure = HostToAppMessage::Reply {
            request_id: id,
            outcome: AppRequestOutcome::Failed(RequestError {
                code: RequestErrorCode::PermissionDenied,
                diagnostic: Some("denied".into()),
            }),
        };
        let wire = encode_host_to_app_control(&failure).unwrap();
        assert_eq!(decode_host_to_app_control(&wire).unwrap(), failure);
        let too_long = HostToAppMessage::Reply {
            request_id: id,
            outcome: AppRequestOutcome::Failed(RequestError {
                code: RequestErrorCode::Internal,
                diagnostic: Some("x".repeat(MAX_DIAGNOSTIC_BYTES + 1)),
            }),
        };
        assert_eq!(
            encode_host_to_app_control(&too_long),
            Err(ContractError::LimitExceeded("string"))
        );
        let max_text = HostToAdminMessage::Reply {
            request_id: id,
            error: RequestError {
                code: RequestErrorCode::UnsupportedOperation,
                diagnostic: Some("x".repeat(MAX_DIAGNOSTIC_BYTES)),
            },
        };
        assert!(encode_host_to_admin_control(&max_text).is_ok());
        assert!(
            decode_host_to_app_control(
                br#"{"type":"reply","request_id":17,"outcome":{"outcome":"succeeded"},"extra":1}"#
            )
            .is_err()
        );
        assert!(
            decode_host_to_app_control(
                br#"{"type":"reply","request_id":17,"outcome":{"outcome":"succeeded"},"request_id":18}"#
            )
            .is_err()
        );
        assert!(
            decode_host_to_app_control(br#"{"type":"health","state":"ready","request_id":17}"#)
                .is_err()
        );
        assert!(
            decode_host_to_app_control(
                br#"{"type":"reply","request_id":0,"outcome":{"outcome":"succeeded"}}"#
            )
            .is_err()
        );
        assert!(decode_host_to_app_control(
            br#"{"type":"reply","request_id":17,"outcome":{"outcome":"failed","error":{"code":"future_code","diagnostic":null}}}"#
        )
        .is_err());
        assert!(
            encode_host_to_app_control(&HostToAppMessage::StreamClosed { stream_id: 1 }).is_ok()
        );
        assert!(
            encode_host_to_app_control(&HostToAppMessage::StreamReset {
                stream_id: 1,
                reason: "closed by host".into(),
            })
            .is_ok()
        );

        let reserved = AdminToHostMessage::ReservedRequest {
            request_id: id,
            operation: ReservedAdminOperation::NetworkPolicy,
        };
        assert!(encode_admin_to_host_control(&reserved).is_ok());
        assert!(
            decode_admin_to_host_control(br#"{"type":"install_request","app_id":"sample-app"}"#)
                .is_err()
        );
        assert!(decode_host_to_admin_control(
            br#"{"type":"reply","request_id":17,"error":{"code":"internal","diagnostic":null}}"#
        )
        .is_err());
    }

    #[test]
    fn every_control_decoder_accepts_only_its_direction() {
        let id = RequestId::new(1).unwrap();
        let app = encode_app_to_host_control(&AppToHostMessage::Close { stream_id: 1 }).unwrap();
        let host_app = encode_host_to_app_control(&HostToAppMessage::Health {
            state: "ready".into(),
            detail: None,
        })
        .unwrap();
        let admin = encode_admin_to_host_control(&AdminToHostMessage::ReservedRequest {
            request_id: id,
            operation: ReservedAdminOperation::Inspect,
        })
        .unwrap();
        let host_admin =
            encode_host_to_admin_control(&HostToAdminMessage::unsupported_admin_operation(id))
                .unwrap();

        assert!(decode_app_to_host_control(&app).is_ok());
        assert!(decode_app_to_host_control(&host_app).is_err());
        assert!(decode_app_to_host_control(&admin).is_err());
        assert!(decode_app_to_host_control(&host_admin).is_err());
        assert!(decode_host_to_app_control(&app).is_err());
        assert!(decode_host_to_app_control(&host_app).is_ok());
        assert!(decode_host_to_app_control(&admin).is_err());
        assert!(decode_host_to_app_control(&host_admin).is_err());
        assert!(decode_admin_to_host_control(&app).is_err());
        assert!(decode_admin_to_host_control(&host_app).is_err());
        assert!(decode_admin_to_host_control(&admin).is_ok());
        assert!(decode_admin_to_host_control(&host_admin).is_err());
        assert!(decode_host_to_admin_control(&app).is_err());
        assert!(decode_host_to_admin_control(&host_app).is_err());
        assert!(decode_host_to_admin_control(&admin).is_err());
        assert!(decode_host_to_admin_control(&host_admin).is_ok());
    }

    #[test]
    fn owned_resource_reference_always_carries_its_principal() {
        let principal = AppPrincipal {
            app_id: AppId::parse("sample-app").unwrap(),
            instance_id: AppInstanceId::new(7).unwrap(),
            publisher_id: Some(PublisherId::parse("sample-publisher").unwrap()),
        };
        let resource =
            PrincipalOwnedResource::new(principal.clone(), OwnedResourceId::new(9).unwrap());
        assert_eq!(resource.principal(), &principal);
        assert_eq!(resource.resource_id().get(), 9);
    }

    #[test]
    fn handshake_and_frame_golden_fragmentation_and_bounds() {
        let h = Handshake {
            role: Role::Application,
            major: 1,
            minor: 0,
        };
        assert_eq!(h.encode(), *b"I2PA\x01\x00\x01\x00\x00");
        assert_eq!(Handshake::decode(&h.encode()).unwrap(), h);
        let f = Frame {
            kind: FrameKind::Data,
            stream_id: 7,
            payload: vec![0xaa, 0xbb],
        };
        let bytes = f.encode().unwrap();
        assert_eq!(&bytes[..12], &[1, 2, 0, 0, 0, 0, 0, 7, 0, 0, 0, 2]);
        assert_eq!(
            Frame::decode(&bytes[..13]),
            Err(ContractError::TruncatedFrame)
        );
        assert_eq!(Frame::decode(&bytes).unwrap(), (f, 14));
        assert_eq!(Frame::decode(&[0; 11]), Err(ContractError::TruncatedFrame));
        let mut too_big = vec![1, 2, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1];
        assert_eq!(
            Frame::decode(&too_big).unwrap_err(),
            ContractError::LimitExceeded("frame payload")
        );
        too_big[8..12].copy_from_slice(&65_537u32.to_be_bytes());
        assert_eq!(
            Frame::decode(&too_big),
            Err(ContractError::LimitExceeded("frame payload"))
        );
    }

    #[test]
    fn control_golden_unknown_duplicate_and_max_plus_one() {
        let msg = AppToHostMessage::Close { stream_id: 3 };
        let wire = encode_app_to_host_control(&msg).unwrap();
        assert_eq!(wire, br#"{"type":"close","stream_id":3}"#);
        assert_eq!(decode_app_to_host_control(&wire).unwrap(), msg);
        assert!(decode_app_to_host_control(br#"{"type":"close","stream_id":3,"x":1}"#).is_err());
        assert!(
            decode_app_to_host_control(br#"{"type":"close","stream_id":3,"stream_id":4}"#).is_err()
        );
        assert!(decode_app_to_host_control(&vec![b' '; MAX_CONTROL_BYTES + 1]).is_err());
        assert!(decode_host_to_app_control(&wire).is_err());
    }

    #[test]
    fn session_limits_reject_duplicates_and_max_plus_one() {
        let mut session = SessionLimits::default();
        for id in 1..=MAX_STREAMS as u32 {
            session.open_stream(id).unwrap();
        }
        assert_eq!(
            session.open_stream(MAX_STREAMS as u32 + 1),
            Err(ContractError::LimitExceeded("streams"))
        );
        assert_eq!(session.open_stream(1), Err(ContractError::InvalidControl));
        session.close_stream(1);
        session.open_stream(MAX_STREAMS as u32 + 1).unwrap();

        for id in 1..=MAX_INFLIGHT_REQUESTS as u32 {
            session.register_request(id).unwrap();
        }
        assert_eq!(
            session.register_request(MAX_INFLIGHT_REQUESTS as u32 + 1),
            Err(ContractError::LimitExceeded("in-flight requests"))
        );
        assert_eq!(
            session.register_request(1),
            Err(ContractError::InvalidControl)
        );
        assert_eq!(session.complete_request(RequestId::new(1).unwrap()), Ok(()));
        assert_eq!(
            session.complete_request(RequestId::new(1).unwrap()),
            Err(ContractError::UnmatchedRequest)
        );
        session
            .register_request(MAX_INFLIGHT_REQUESTS as u32 + 1)
            .unwrap();
    }

    #[test]
    fn default_deny_scopes_and_post_resolution_check() {
        let policy = NetworkPolicy::default();
        for raw in [
            "127.0.0.1",
            "10.1.2.3",
            "169.254.1.1",
            "224.0.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "::",
        ] {
            let ip: IpAddr = raw.parse().unwrap();
            assert_eq!(
                policy.evaluate_requested_ip(NetworkProtocol::Tcp, ip, 443),
                PolicyDecision::Deny
            );
        }
        assert_eq!(
            address_scope(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))),
            AddressScope::Private
        );
        let global = "8.8.8.8".parse().unwrap();
        let allow = NetworkPolicy {
            rules: vec![NetworkRule {
                protocol: NetworkProtocol::Tcp,
                destination: DestinationSelector::Hostname("allowed.example".into()),
                ports: PortSelector::Single(443),
                action: RuleAction::Allow,
            }],
        };
        assert_eq!(
            allow.evaluate_resolved_address(NetworkProtocol::Tcp, "allowed.example", global, 443),
            PolicyDecision::Allow
        );
        for raw in [
            "127.0.0.1",
            "10.1.2.3",
            "169.254.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "192.0.0.9",
            "224.0.0.1",
            "0.0.0.0",
            "240.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2001:2::1",
            "3fff::1",
            "ff02::1",
            "::",
        ] {
            let ip: IpAddr = raw.parse().unwrap();
            assert_eq!(
                allow.evaluate_resolved_address(NetworkProtocol::Tcp, "allowed.example", ip, 443,),
                PolicyDecision::Deny,
                "{raw} must not pass hostname-only authorization"
            );
        }

        let explicit_non_global = NetworkPolicy {
            rules: vec![
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Hostname("allowed.example".into()),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow,
                },
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Cidr(IpCidr {
                        network: "192.0.2.0".parse().unwrap(),
                        prefix: 24,
                    }),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow,
                },
            ],
        };
        assert_eq!(
            explicit_non_global.evaluate_resolved_address(
                NetworkProtocol::Tcp,
                "allowed.example",
                "192.0.2.1".parse().unwrap(),
                443,
            ),
            PolicyDecision::Allow
        );
        assert_eq!(
            explicit_non_global.evaluate_requested_ip(
                NetworkProtocol::Tcp,
                "192.0.3.1".parse().unwrap(),
                443,
            ),
            PolicyDecision::Deny
        );
        let explicit_non_global_deny = NetworkPolicy {
            rules: [
                explicit_non_global.rules.clone(),
                vec![NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Ip("192.0.2.1".parse().unwrap()),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Deny,
                }],
            ]
            .concat(),
        };
        assert_eq!(
            explicit_non_global_deny.evaluate_resolved_address(
                NetworkProtocol::Tcp,
                "allowed.example",
                "192.0.2.1".parse().unwrap(),
                443
            ),
            PolicyDecision::Deny
        );
        let scoped_loopback_allow = NetworkPolicy {
            rules: vec![NetworkRule {
                protocol: NetworkProtocol::Tcp,
                destination: DestinationSelector::Ip("127.0.0.1".parse().unwrap()),
                ports: PortSelector::Range {
                    first: 80,
                    last: 90,
                },
                action: RuleAction::Allow,
            }],
        };
        assert_eq!(
            scoped_loopback_allow.evaluate_requested_ip(
                NetworkProtocol::Tcp,
                "127.0.0.1".parse().unwrap(),
                80
            ),
            PolicyDecision::Allow
        );
        let deny_override = NetworkPolicy {
            rules: vec![
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Hostname("allowed.example".into()),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow,
                },
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Ip(global),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Deny,
                },
            ],
        };
        assert_eq!(
            deny_override.evaluate_resolved_address(
                NetworkProtocol::Tcp,
                "allowed.example",
                global,
                443,
            ),
            PolicyDecision::Deny
        );
        let too_many_rules = NetworkPolicy {
            rules: vec![
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Ip(global),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow,
                };
                MAX_NETWORK_RULES + 1
            ],
        };
        assert_eq!(
            too_many_rules.validate(),
            Err(ContractError::LimitExceeded("network rules"))
        );
        assert!(
            NetworkPolicy {
                rules: vec![NetworkRule {
                    protocol: NetworkProtocol::Udp,
                    destination: DestinationSelector::Ip(global),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow
                }]
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn mapped_ipv6_targets_share_ipv4_policy_identity() {
        fn rule(destination: DestinationSelector, action: RuleAction) -> NetworkRule {
            NetworkRule {
                protocol: NetworkProtocol::Tcp,
                destination,
                ports: PortSelector::Single(443),
                action,
            }
        }

        let ipv4: IpAddr = "8.8.8.8".parse().unwrap();
        let mapped: IpAddr = "::ffff:8.8.8.8".parse().unwrap();
        // The pre-fix raw IpAddr comparison treated these as distinct and
        // missed the IPv4 rule when the resolver supplied the mapped form.
        assert_ne!(ipv4, mapped);
        let hostname = rule(
            DestinationSelector::Hostname("allowed.example".into()),
            RuleAction::Allow,
        );

        for (action, expected) in [
            (RuleAction::Allow, PolicyDecision::Allow),
            (RuleAction::Deny, PolicyDecision::Deny),
        ] {
            let exact = NetworkPolicy {
                rules: vec![rule(DestinationSelector::Ip(ipv4), action)],
            };
            assert_eq!(exact.validate(), Ok(()));
            for target in [ipv4, mapped] {
                assert_eq!(
                    exact.evaluate_requested_ip(NetworkProtocol::Tcp, target, 443),
                    expected
                );
            }

            let cidr = NetworkPolicy {
                rules: vec![rule(
                    DestinationSelector::Cidr(IpCidr {
                        network: "8.8.8.0".parse().unwrap(),
                        prefix: 24,
                    }),
                    action,
                )],
            };
            assert_eq!(cidr.validate(), Ok(()));
            for target in [ipv4, mapped] {
                assert_eq!(
                    cidr.evaluate_requested_ip(NetworkProtocol::Tcp, target, 443),
                    expected
                );
            }
        }

        let exact_deny = NetworkPolicy {
            rules: vec![
                hostname.clone(),
                rule(DestinationSelector::Ip(ipv4), RuleAction::Deny),
            ],
        };
        let cidr_deny = NetworkPolicy {
            rules: vec![
                hostname,
                rule(
                    DestinationSelector::Cidr(IpCidr {
                        network: "8.8.8.0".parse().unwrap(),
                        prefix: 24,
                    }),
                    RuleAction::Deny,
                ),
            ],
        };
        for policy in [&exact_deny, &cidr_deny] {
            for target in [ipv4, mapped] {
                assert_eq!(
                    policy.evaluate_resolved_address(
                        NetworkProtocol::Tcp,
                        "allowed.example",
                        target,
                        443,
                    ),
                    PolicyDecision::Deny
                );
            }
        }

        for raw in ["10.1.2.3", "192.0.2.1"] {
            let ipv4: IpAddr = raw.parse().unwrap();
            let mapped = match ipv4 {
                IpAddr::V4(v4) => IpAddr::V6(v4.to_ipv6_mapped()),
                IpAddr::V6(_) => unreachable!(),
            };
            assert_eq!(address_scope(ipv4), address_scope(mapped));
            let deny_by_default = NetworkPolicy {
                rules: vec![rule(
                    DestinationSelector::Hostname("allowed.example".into()),
                    RuleAction::Allow,
                )],
            };
            for target in [ipv4, mapped] {
                assert_eq!(
                    deny_by_default.evaluate_resolved_address(
                        NetworkProtocol::Tcp,
                        "allowed.example",
                        target,
                        443,
                    ),
                    PolicyDecision::Deny
                );
            }
            let explicit_ipv4_allow = NetworkPolicy {
                rules: vec![rule(
                    DestinationSelector::Cidr(IpCidr {
                        network: ipv4,
                        prefix: 32,
                    }),
                    RuleAction::Allow,
                )],
            };
            for target in [ipv4, mapped] {
                assert_eq!(
                    explicit_ipv4_allow.evaluate_requested_ip(NetworkProtocol::Tcp, target, 443,),
                    PolicyDecision::Allow
                );
            }
        }

        let mapped_exact = NetworkPolicy {
            rules: vec![rule(DestinationSelector::Ip(mapped), RuleAction::Allow)],
        };
        let mapped_cidr = NetworkPolicy {
            rules: vec![rule(
                DestinationSelector::Cidr(IpCidr {
                    network: "::ffff:8.8.8.0".parse().unwrap(),
                    prefix: 120,
                }),
                RuleAction::Allow,
            )],
        };
        assert_eq!(mapped_exact.validate(), Err(ContractError::InvalidPolicy));
        assert_eq!(mapped_cidr.validate(), Err(ContractError::InvalidPolicy));
        assert_eq!(
            mapped_exact.evaluate_requested_ip(NetworkProtocol::Tcp, ipv4, 443),
            PolicyDecision::Deny
        );
        assert_eq!(
            mapped_cidr.evaluate_requested_ip(NetworkProtocol::Tcp, ipv4, 443),
            PolicyDecision::Deny
        );

        let native_ipv6: IpAddr = "2001:4860::1".parse().unwrap();
        let native_ipv6_policy = NetworkPolicy {
            rules: vec![rule(
                DestinationSelector::Ip(native_ipv6),
                RuleAction::Allow,
            )],
        };
        assert_eq!(native_ipv6_policy.validate(), Ok(()));
        assert_eq!(
            native_ipv6_policy.evaluate_requested_ip(NetworkProtocol::Tcp, native_ipv6, 443),
            PolicyDecision::Allow
        );

        // Deterministically exercise all addresses in a representative /24
        // against exact and CIDR allow/deny selectors and hostname post-resolution.
        for last_octet in 0..=u8::MAX {
            let target_v4 = IpAddr::V4(Ipv4Addr::new(8, 8, 8, last_octet));
            let target_mapped = match target_v4 {
                IpAddr::V4(v4) => IpAddr::V6(v4.to_ipv6_mapped()),
                IpAddr::V6(_) => unreachable!(),
            };
            let action = if last_octet % 2 == 0 {
                RuleAction::Deny
            } else {
                RuleAction::Allow
            };
            let exact = NetworkPolicy {
                rules: vec![
                    rule(
                        DestinationSelector::Hostname("allowed.example".into()),
                        RuleAction::Allow,
                    ),
                    rule(DestinationSelector::Ip(target_v4), action),
                ],
            };
            let cidr = NetworkPolicy {
                rules: vec![
                    rule(
                        DestinationSelector::Hostname("allowed.example".into()),
                        RuleAction::Allow,
                    ),
                    rule(
                        DestinationSelector::Cidr(IpCidr {
                            network: "8.8.8.0".parse().unwrap(),
                            prefix: 24,
                        }),
                        action,
                    ),
                ],
            };
            for policy in [&exact, &cidr] {
                assert_eq!(
                    policy.evaluate_requested_ip(NetworkProtocol::Tcp, target_v4, 443),
                    policy.evaluate_requested_ip(NetworkProtocol::Tcp, target_mapped, 443)
                );
                assert_eq!(
                    policy.evaluate_resolved_address(
                        NetworkProtocol::Tcp,
                        "allowed.example",
                        target_v4,
                        443,
                    ),
                    policy.evaluate_resolved_address(
                        NetworkProtocol::Tcp,
                        "allowed.example",
                        target_mapped,
                        443,
                    )
                );
            }
        }
    }

    #[test]
    fn address_classification_covers_frozen_special_purpose_boundaries() {
        let cases = [
            ("0.0.0.0", AddressScope::Unspecified),
            ("0.255.255.255", AddressScope::SpecialPurpose),
            ("1.0.0.0", AddressScope::Global),
            ("9.255.255.255", AddressScope::Global),
            ("10.0.0.0", AddressScope::Private),
            ("10.255.255.255", AddressScope::Private),
            ("11.0.0.0", AddressScope::Global),
            ("100.63.255.255", AddressScope::Global),
            ("100.64.0.0", AddressScope::Private),
            ("100.127.255.255", AddressScope::Private),
            ("100.128.0.0", AddressScope::Global),
            ("127.0.0.0", AddressScope::Loopback),
            ("127.255.255.255", AddressScope::Loopback),
            ("128.0.0.0", AddressScope::Global),
            ("169.253.255.255", AddressScope::Global),
            ("169.254.0.0", AddressScope::LinkLocal),
            ("169.254.255.255", AddressScope::LinkLocal),
            ("169.255.0.0", AddressScope::Global),
            ("172.15.255.255", AddressScope::Global),
            ("172.16.0.0", AddressScope::Private),
            ("172.31.255.255", AddressScope::Private),
            ("172.32.0.0", AddressScope::Global),
            ("191.255.255.255", AddressScope::Global),
            ("192.0.0.0", AddressScope::SpecialPurpose),
            ("192.0.0.255", AddressScope::SpecialPurpose),
            ("192.0.1.0", AddressScope::Global),
            ("192.0.1.255", AddressScope::Global),
            ("192.0.2.0", AddressScope::Documentation),
            ("192.0.2.255", AddressScope::Documentation),
            ("192.0.3.0", AddressScope::Global),
            ("192.31.195.255", AddressScope::Global),
            ("192.31.196.0", AddressScope::SpecialPurpose),
            ("192.31.196.255", AddressScope::SpecialPurpose),
            ("192.31.197.0", AddressScope::Global),
            ("192.52.192.255", AddressScope::Global),
            ("192.52.193.0", AddressScope::SpecialPurpose),
            ("192.52.193.255", AddressScope::SpecialPurpose),
            ("192.52.194.0", AddressScope::Global),
            ("192.88.98.255", AddressScope::Global),
            ("192.88.99.0", AddressScope::SpecialPurpose),
            ("192.88.99.255", AddressScope::SpecialPurpose),
            ("192.88.100.0", AddressScope::Global),
            ("192.168.0.0", AddressScope::Private),
            ("192.168.255.255", AddressScope::Private),
            ("192.175.48.0", AddressScope::SpecialPurpose),
            ("192.175.48.255", AddressScope::SpecialPurpose),
            ("192.175.49.0", AddressScope::Global),
            ("198.17.255.255", AddressScope::Global),
            ("198.18.0.0", AddressScope::Benchmark),
            ("198.19.255.255", AddressScope::Benchmark),
            ("198.20.0.0", AddressScope::Global),
            ("198.51.99.255", AddressScope::Global),
            ("198.51.100.0", AddressScope::Documentation),
            ("198.51.100.255", AddressScope::Documentation),
            ("198.51.101.0", AddressScope::Global),
            ("203.0.112.255", AddressScope::Global),
            ("203.0.113.0", AddressScope::Documentation),
            ("203.0.113.255", AddressScope::Documentation),
            ("203.0.114.0", AddressScope::Global),
            ("224.0.0.0", AddressScope::Multicast),
            ("239.255.255.255", AddressScope::Multicast),
            ("240.0.0.0", AddressScope::Reserved),
            ("255.255.255.255", AddressScope::Broadcast),
            ("::", AddressScope::Unspecified),
            ("::1", AddressScope::Loopback),
            ("::ffff:10.0.0.1", AddressScope::Private),
            ("::ffff:8.8.8.8", AddressScope::Global),
            ("64:ff9b::", AddressScope::SpecialPurpose),
            ("64:ff9b::ffff:ffff", AddressScope::SpecialPurpose),
            ("64:ff9b:1::", AddressScope::SpecialPurpose),
            (
                "64:ff9b:1:ffff:ffff:ffff:ffff:ffff",
                AddressScope::SpecialPurpose,
            ),
            ("100::", AddressScope::SpecialPurpose),
            ("100::ffff:ffff:ffff:ffff", AddressScope::SpecialPurpose),
            ("100:0:0:1::", AddressScope::SpecialPurpose),
            (
                "100:0:0:1:ffff:ffff:ffff:ffff",
                AddressScope::SpecialPurpose,
            ),
            ("2001::", AddressScope::SpecialPurpose),
            ("2001:1::", AddressScope::SpecialPurpose),
            ("2001:2::", AddressScope::Benchmark),
            ("2001:2:0:ffff:ffff:ffff:ffff:ffff", AddressScope::Benchmark),
            (
                "2001:1ff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::SpecialPurpose,
            ),
            ("2001:200::", AddressScope::Global),
            ("2001:db8::", AddressScope::Documentation),
            (
                "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::Documentation,
            ),
            ("2001:db9::", AddressScope::Global),
            ("2002::", AddressScope::SpecialPurpose),
            (
                "2002:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::SpecialPurpose,
            ),
            (
                "3ffe:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::Global,
            ),
            ("3fff::", AddressScope::Documentation),
            (
                "3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::Documentation,
            ),
            ("4000::", AddressScope::Reserved),
            ("5f00::", AddressScope::Reserved),
            (
                "5f00:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::Reserved,
            ),
            ("fc00::", AddressScope::Private),
            (
                "fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::Private,
            ),
            ("fe80::", AddressScope::LinkLocal),
            (
                "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::LinkLocal,
            ),
            ("ff00::", AddressScope::Multicast),
            (
                "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
                AddressScope::Multicast,
            ),
            ("2000::", AddressScope::Global),
            ("2003::", AddressScope::Global),
        ];
        for (raw, expected) in cases {
            let ip = raw.parse().unwrap();
            assert_eq!(address_scope(ip), expected, "classification of {raw}");
        }
        assert_eq!(
            address_scope("3fff:1000::".parse().unwrap()),
            AddressScope::Global
        );
        assert_eq!(
            address_scope("3fff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()),
            AddressScope::Global
        );
    }

    #[test]
    fn hostname_allow_only_passes_classified_global_addresses() {
        let policy = NetworkPolicy {
            rules: vec![NetworkRule {
                protocol: NetworkProtocol::Tcp,
                destination: DestinationSelector::Hostname("allowed.example".into()),
                ports: PortSelector::Single(443),
                action: RuleAction::Allow,
            }],
        };
        let mut seed = 0x349u64;
        for _ in 0..10_000 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let v4 = IpAddr::V4(Ipv4Addr::from((seed >> 32) as u32));
            let expected = if address_scope(v4) == AddressScope::Global {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny
            };
            assert_eq!(
                policy.evaluate_resolved_address(NetworkProtocol::Tcp, "allowed.example", v4, 443,),
                expected,
                "hostname-only IPv4 decision for {v4}"
            );
            let v6 = IpAddr::V6(std::net::Ipv6Addr::from(
                u128::from(seed).wrapping_mul(seed as u128 + 1),
            ));
            let expected = if address_scope(v6) == AddressScope::Global {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny
            };
            assert_eq!(
                policy.evaluate_resolved_address(NetworkProtocol::Tcp, "allowed.example", v6, 443,),
                expected,
                "hostname-only IPv6 decision for {v6}"
            );
        }
    }

    #[test]
    fn secured_attestation_requires_every_property_and_manifest_rejects_unknown() {
        let mut attestation = SandboxAttestation {
            backend_kind: "test".into(),
            backend_version: "1".into(),
            evidence_generation: 1,
            properties: REQUIRED_SECURED_PROPERTIES.to_vec(),
        };
        assert!(attestation.validate_secured().is_ok());
        attestation.backend_kind = "x".repeat(129);
        assert_eq!(
            attestation.validate_secured(),
            Err(ContractError::LimitExceeded("string"))
        );
        attestation.backend_kind = "test".into();
        attestation
            .properties
            .extend([SandboxProperty::DirectNetworkDenied; 9]);
        assert_eq!(
            attestation.validate_secured(),
            Err(ContractError::LimitExceeded("attestation fields"))
        );
        attestation
            .properties
            .truncate(REQUIRED_SECURED_PROPERTIES.len());
        for index in 0..attestation.properties.len() {
            let removed = attestation.properties.remove(index);
            assert!(attestation.validate_secured().is_err());
            attestation.properties.insert(index, removed);
        }
        assert!(Manifest::decode(br#"{"schema_version":1,"unexpected":true}"#).is_err());
    }

    #[test]
    fn hostile_decoder_inputs_do_not_panic_and_frame_payload_ceiling_is_exact() {
        for len in 0..=FRAME_HEADER_BYTES {
            let input = vec![0xA5; len];
            let _ = Frame::decode(&input);
        }
        let mut seed = 0x345_u64;
        for len in 0..=MAX_CONTROL_BYTES + 1 {
            let mut input = Vec::with_capacity(len);
            for _ in 0..len {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                input.push((seed >> 32) as u8);
            }
            let _ = decode_app_to_host_control(&input);
            let _ = decode_host_to_app_control(&input);
            let _ = decode_admin_to_host_control(&input);
            let _ = decode_host_to_admin_control(&input);
            let _ = Manifest::decode(&input);
        }
        let max = Frame {
            kind: FrameKind::Data,
            stream_id: 1,
            payload: vec![0x5a; MAX_FRAME_PAYLOAD_BYTES],
        };
        assert_eq!(Frame::decode(&max.encode().unwrap()).unwrap().0, max);
    }
}
