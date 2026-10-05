//! Runtime-neutral, bounded managed-application protocol and policy contract.
//!
//! This crate owns validated values only. It has no runtime, socket, process,
//! filesystem, DNS, sandbox, router, or application owner. See
//! `specs/references/managed-native-app-runtime-v1.md` for the language-neutral
//! wire contract. No Rust representation is a wire ABI.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
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
    #[error("secured sandbox attestation is incomplete")]
    IncompleteAttestation,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct AppInstanceId(u128);
impl AppInstanceId {
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
impl TryFrom<u128> for AppInstanceId {
    type Error = ContractError;
    fn try_from(value: u128) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<AppInstanceId> for u128 {
    fn from(value: AppInstanceId) -> Self {
        value.0
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
    ) -> Self {
        Self { capability }
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
pub enum AppMessage {
    #[serde(rename = "hello")]
    Hello {
        app_id: AppId,
        instance_id: AppInstanceId,
        protocol_major: u8,
        protocol_minor: u8,
    },
    #[serde(rename = "capabilities")]
    Capabilities { capabilities: Vec<Capability> },
    #[serde(rename = "open")]
    Open {
        request_id: u32,
        stream_id: u32,
        service: AppService,
    },
    #[serde(rename = "accept")]
    Accept { request_id: u32, stream_id: u32 },
    #[serde(rename = "close")]
    Close { stream_id: u32 },
    #[serde(rename = "reset")]
    Reset { stream_id: u32, reason: String },
    #[serde(rename = "permission_request")]
    PermissionRequest {
        request_id: u32,
        capabilities: Vec<RequestedCapability>,
    },
    #[serde(rename = "permission_status")]
    PermissionStatus {
        request_id: u32,
        status: PermissionStatus,
    },
    #[serde(rename = "ui_message")]
    UiMessage { message_id: u32, payload: String },
    #[serde(rename = "health")]
    Health {
        state: String,
        detail: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum AdminMessage {
    #[serde(rename = "admin_hello")]
    Hello {
        protocol_major: u8,
        protocol_minor: u8,
    },
    #[serde(rename = "install_request")]
    InstallRequest { app_id: AppId },
    #[serde(rename = "update_request")]
    UpdateRequest { app_id: AppId },
    #[serde(rename = "uninstall_request")]
    UninstallRequest { app_id: AppId },
    #[serde(rename = "launch_request")]
    LaunchRequest { app_id: AppId },
    #[serde(rename = "stop_request")]
    StopRequest { app_id: AppId },
    #[serde(rename = "grant_request")]
    GrantRequest {
        app_id: AppId,
        capabilities: Vec<Capability>,
    },
    #[serde(rename = "revoke_request")]
    RevokeRequest {
        app_id: AppId,
        capabilities: Vec<Capability>,
    },
    #[serde(rename = "network_policy_request")]
    NetworkPolicyRequest {
        app_id: AppId,
        rules: Vec<NetworkRule>,
    },
    #[serde(rename = "launch_profile_request")]
    LaunchProfileRequest {
        app_id: AppId,
        profile: LaunchProfile,
    },
    #[serde(rename = "resource_policy_request")]
    ResourcePolicyRequest {
        app_id: AppId,
        resources: Vec<ResourceRequest>,
    },
    #[serde(rename = "inspect_request")]
    InspectRequest { app_id: AppId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppService {
    Sam,
    I2cp,
    ControlScoped,
    BrokeredTcp,
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
    pub fn decode_app_message(&self, bytes: &[u8]) -> Result<AppMessage, ContractError> {
        if self.role != Role::Application {
            return Err(ContractError::RoleMismatch);
        }
        decode_control(bytes)
    }
    pub fn decode_admin_message(&self, bytes: &[u8]) -> Result<AdminMessage, ContractError> {
        if self.role != Role::Administrator {
            return Err(ContractError::RoleMismatch);
        }
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(ContractError::LimitExceeded("control"));
        }
        let message: AdminMessage =
            serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidControl)?;
        validate_admin_message(&message)?;
        Ok(message)
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

pub fn encode_control(message: &AppMessage) -> Result<Vec<u8>, ContractError> {
    validate_app_message(message)?;
    let bytes = serde_json::to_vec(message).map_err(|_| ContractError::InvalidControl)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ContractError::LimitExceeded("control"));
    }
    Ok(bytes)
}
pub fn encode_admin_control(message: &AdminMessage) -> Result<Vec<u8>, ContractError> {
    validate_admin_message(message)?;
    let bytes = serde_json::to_vec(message).map_err(|_| ContractError::InvalidControl)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ContractError::LimitExceeded("control"));
    }
    Ok(bytes)
}
pub fn decode_control(bytes: &[u8]) -> Result<AppMessage, ContractError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ContractError::LimitExceeded("control"));
    }
    let message: AppMessage =
        serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidControl)?;
    validate_app_message(&message)?;
    Ok(message)
}
pub fn validate_app_message(message: &AppMessage) -> Result<(), ContractError> {
    match message {
        AppMessage::Capabilities { capabilities } => validate_capabilities(capabilities),
        AppMessage::PermissionRequest { capabilities, .. } => {
            validate_requested_capabilities(capabilities)
        }
        AppMessage::Hello { protocol_major, .. } if *protocol_major != PROTOCOL_MAJOR => {
            Err(ContractError::UnsupportedVersion)
        }
        AppMessage::Open {
            request_id,
            stream_id,
            ..
        }
        | AppMessage::Accept {
            request_id,
            stream_id,
        } => {
            if *request_id == 0 || *stream_id == 0 {
                return Err(ContractError::InvalidControl);
            }
            Ok(())
        }
        AppMessage::Close { stream_id } | AppMessage::Reset { stream_id, .. } => {
            if *stream_id == 0 {
                return Err(ContractError::InvalidControl);
            }
            if let AppMessage::Reset { reason, .. } = message {
                bounded_string(reason, MAX_DIAGNOSTIC_BYTES)
            } else {
                Ok(())
            }
        }
        AppMessage::PermissionStatus { request_id, .. } => {
            if *request_id == 0 {
                Err(ContractError::InvalidControl)
            } else {
                Ok(())
            }
        }
        AppMessage::Health { state, detail } => {
            bounded_string(state, 64)?;
            if let Some(v) = detail {
                bounded_string(v, MAX_DIAGNOSTIC_BYTES)?;
            }
            Ok(())
        }
        AppMessage::UiMessage {
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
        _ => Ok(()),
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
    pub fn finish_request(&mut self, request_id: u32) {
        self.requests.remove(&request_id);
    }
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }
    pub fn inflight_count(&self) -> usize {
        self.requests.len()
    }
}
fn validate_admin_message(message: &AdminMessage) -> Result<(), ContractError> {
    match message {
        AdminMessage::Hello { protocol_major, .. } if *protocol_major != PROTOCOL_MAJOR => {
            Err(ContractError::UnsupportedVersion)
        }
        AdminMessage::GrantRequest { capabilities, .. }
        | AdminMessage::RevokeRequest { capabilities, .. } => validate_capabilities(capabilities),
        AdminMessage::NetworkPolicyRequest { rules, .. } => {
            if rules.len() > MAX_NETWORK_RULES {
                return Err(ContractError::LimitExceeded("network rules"));
            }
            NetworkPolicy {
                rules: rules.clone(),
            }
            .validate()
        }
        AdminMessage::ResourcePolicyRequest { resources, .. } => {
            if resources.len() > 16 {
                return Err(ContractError::LimitExceeded("resources"));
            }
            for resource in resources {
                validate_identifier(&resource.name)?;
                if resource.requested > MAX_RESOURCE_REQUEST {
                    return Err(ContractError::LimitExceeded("resource request"));
                }
            }
            Ok(())
        }
        _ => Ok(()),
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
    Public,
    Loopback,
    Private,
    LinkLocal,
    Multicast,
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
                    {
                        return Err(ContractError::InvalidPolicy);
                    }
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
        let scope = address_scope(ip);
        let matching = self
            .rules
            .iter()
            .filter(|r| {
                r.protocol == protocol
                    && r.ports.contains(port)
                    && match &r.destination {
                        DestinationSelector::Ip(v) => *v == ip,
                        DestinationSelector::Cidr(c) => c.contains(ip),
                        _ => false,
                    }
            })
            .collect::<Vec<_>>();
        if scope != AddressScope::Public && !matching.iter().any(|r| r.action == RuleAction::Allow)
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
        if self.evaluate_requested_ip(protocol, ip, port) == PolicyDecision::Deny {
            return PolicyDecision::Deny;
        }
        PolicyDecision::Allow
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
pub fn address_scope(ip: IpAddr) -> AddressScope {
    match ip {
        IpAddr::V4(v) => {
            if v.is_unspecified() {
                AddressScope::Unspecified
            } else if v.is_loopback() {
                AddressScope::Loopback
            } else if v.is_multicast() || v == Ipv4Addr::BROADCAST {
                AddressScope::Multicast
            } else if v.is_link_local() {
                AddressScope::LinkLocal
            } else if v.is_private() || v.octets()[0] == 100 && (64..=127).contains(&v.octets()[1])
            {
                AddressScope::Private
            } else {
                AddressScope::Public
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
            } else {
                AddressScope::Public
            }
        }
    }
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
            decode_control(br#"{"type":"grant_request","app_id":"a","capabilities":[]}"#).is_err()
        );
        assert!(
            decode_control(
                br#"{"type":"permission_request","request_id":1,"capabilities":["sam"],"extra":1}"#
            )
            .is_err()
        );
        let app = Handshake {
            role: Role::Application,
            major: 1,
            minor: 0,
        };
        assert_eq!(
            app.decode_admin_message(
                br#"{"type":"admin_hello","protocol_major":1,"protocol_minor":0}"#
            ),
            Err(ContractError::RoleMismatch)
        );
        let administrator = AdministratorPrincipal::from_authenticated_session(1).unwrap();
        let grant = GrantedCapability::from_administrator_policy(&administrator, Capability::Sam);
        assert_eq!(
            EffectiveCapabilities::from_grants(&[grant])
                .unwrap()
                .as_slice(),
            &[Capability::Sam]
        );
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
        let msg = AppMessage::Close { stream_id: 3 };
        let wire = encode_control(&msg).unwrap();
        assert_eq!(wire, br#"{"type":"close","stream_id":3}"#);
        assert_eq!(decode_control(&wire).unwrap(), msg);
        assert!(decode_control(br#"{"type":"close","stream_id":3,"x":1}"#).is_err());
        assert!(decode_control(br#"{"type":"close","stream_id":3,"stream_id":4}"#).is_err());
        assert!(decode_control(&vec![b' '; MAX_CONTROL_BYTES + 1]).is_err());
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
        session.finish_request(1);
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
        let public = "8.8.8.8".parse().unwrap();
        let allow = NetworkPolicy {
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
                        network: "8.8.8.0".parse().unwrap(),
                        prefix: 24,
                    }),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow,
                },
            ],
        };
        assert_eq!(
            allow.evaluate_resolved_address(NetworkProtocol::Tcp, "allowed.example", public, 443),
            PolicyDecision::Allow
        );
        assert_eq!(
            allow.evaluate_resolved_address(
                NetworkProtocol::Tcp,
                "allowed.example",
                "127.0.0.1".parse().unwrap(),
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
                    destination: DestinationSelector::Cidr(IpCidr {
                        network: "8.8.8.0".parse().unwrap(),
                        prefix: 24,
                    }),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow,
                },
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Ip(public),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Deny,
                },
            ],
        };
        assert_eq!(
            deny_override.evaluate_requested_ip(NetworkProtocol::Tcp, public, 443),
            PolicyDecision::Deny
        );
        let too_many_rules = NetworkPolicy {
            rules: vec![
                NetworkRule {
                    protocol: NetworkProtocol::Tcp,
                    destination: DestinationSelector::Ip(public),
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
                    destination: DestinationSelector::Ip(public),
                    ports: PortSelector::Single(443),
                    action: RuleAction::Allow
                }]
            }
            .validate()
            .is_err()
        );
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
            let _ = decode_control(&input);
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
