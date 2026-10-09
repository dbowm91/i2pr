//! Plan 369 §7 — AppManager ↔ apphost bootstrap contract.
//!
//! This is a **one-way authority handoff**: the trusted manager sends exactly one
//! bounded launch request, apphost replies with a typed ready/failure result, and
//! after success the channel becomes byte-transparent. Apphost never interprets
//! application protocol bytes again.
//!
//! It lives in the runtime-neutral contract crate because it is a wire contract,
//! not an implementation. Neither side's process, filesystem, or exec authority
//! is expressed here.
//!
//! # What this contract deliberately cannot express
//!
//! - No shell string, command interpreter, script hook, or PATH lookup. The
//!   entrypoint is a validated package-relative path under a prevalidated root.
//! - No environment expansion. Environment entries are bounded, sanitised
//!   key/value pairs.
//! - Secured launches carry a manager-owned data root and a ready attestation.
//! - No decoder from application protocol messages or manifest bytes. This is the
//!   whole point: launch authority is manager-created, never app-created.

use std::collections::BTreeMap;

use i2pr_app_proto::{LaunchProfile, MAX_IDENTIFIER_BYTES, SandboxAttestation};

use serde::{Deserialize, Serialize};
/// Re-exported so a peer can decode a reply without adding its own dependency.
pub use serde_json;
use thiserror::Error;

/// Bootstrap magic. Distinct from both `I2PA` (app v1) and `I2PM` (manager).
pub const APPHOST_BOOTSTRAP_MAGIC: [u8; 4] = *b"I2PA";
/// The app-protocol handshake that must follow a successful bootstrap.
pub const APPHOST_HANDSHAKE_MAGIC: [u8; 4] = i2pr_app_proto::HANDSHAKE_MAGIC;

pub const APPHOST_BOOTSTRAP_BYTES: usize = 9;

/// Bytes reserved for the big-endian `u32` length prefix that frames the
/// bootstrap payload behind the handshake.
///
/// The framing lives here rather than in either implementation because it *is*
/// protocol: two independently-chosen constants in the manager and the host
/// would drift into a handshake that works until it does not.
pub const BOOTSTRAP_LENGTH_PREFIX_BYTES: usize = 4;

/// Ceiling on one bootstrap payload, before any parsing.
///
/// Every field inside `LaunchRequest` is individually bounded, so this is a
/// generous outer envelope whose job is to bound the *allocation*, not to
/// describe the schema.
pub const MAX_BOOTSTRAP_PAYLOAD_BYTES: usize = 64 * 1024;
pub const APPHOST_BOOTSTRAP_MAJOR: u8 = 1;
pub const APPHOST_BOOTSTRAP_MINOR: u8 = 0;
pub const APPHOST_ROLE_BYTE: u8 = 1;

pub const MAX_ENTRYPOINT_BYTES: usize = 512;
pub const MAX_ARGV_ITEMS: usize = 32;
pub const MAX_ARGV_ITEM_BYTES: usize = 512;
pub const MAX_ENV_ENTRIES: usize = 32;
pub const MAX_ENV_KEY_BYTES: usize = 64;
pub const MAX_ENV_VALUE_BYTES: usize = 512;
pub const MAX_ROOT_BYTES: usize = 1_024;

/// Refusal reasons. Every variant is typed so apphost can fail closed without
/// guessing, and so a test can assert *which* gate rejected a launch.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ApphostBootstrapError {
    #[error("bootstrap magic mismatch")]
    BadMagic,
    #[error("unsupported bootstrap version")]
    UnsupportedVersion,
    #[error("unsupported apphost role")]
    RoleMismatch,
    #[error("malformed bootstrap")]
    Malformed,
    #[error("truncated bootstrap")]
    Truncated,
    /// The current host or executable cannot meet the secured profile.
    #[error("secured launch is unavailable on this host or executable")]
    SecuredUnavailable,
    #[error("launch root is invalid")]
    InvalidRoot,
    #[error("application data root is invalid")]
    InvalidDataRoot,
    #[error("secured sandbox setup failed")]
    SandboxSetupFailed,
    #[error("entrypoint escapes the launch root")]
    EntrypointEscapesRoot,
    #[error("entrypoint is invalid")]
    InvalidEntrypoint,
    #[error("argv is invalid or oversize")]
    InvalidArgv,
    #[error("environment is invalid or oversize")]
    InvalidEnvironment,
    #[error("limit exceeded: {0}")]
    LimitExceeded(&'static str),
}

/// A validated package-relative entrypoint path.
///
/// It is deliberately **not** a filesystem path type: this contract describes
/// the string a launch will use, and the `..`/absolute/escape checks are pure
/// so they can be tested without touching a filesystem.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Entrypoint(String);

impl Entrypoint {
    pub fn new(value: impl Into<String>) -> Result<Self, ApphostBootstrapError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_ENTRYPOINT_BYTES
            && value.is_ascii()
            && !value.starts_with('/')
            && !value.contains('\\')
            && value
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
        if !valid {
            return Err(ApphostBootstrapError::InvalidEntrypoint);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Entrypoint {
    type Error = ApphostBootstrapError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Entrypoint> for String {
    fn from(value: Entrypoint) -> Self {
        value.0
    }
}

/// A prevalidated application root.
///
/// The daemon/manager chooses it from a trust decision. There is no decoder from
/// an application's own bytes, so a hostile app cannot select its own root.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LaunchRoot(String);

impl LaunchRoot {
    pub fn new(value: impl Into<String>) -> Result<Self, ApphostBootstrapError> {
        let value = value.into();
        // The root must be absolute: a relative root would make containment
        // depend on the process working directory, which is not a trust input.
        if value.is_empty()
            || value.len() > MAX_ROOT_BYTES
            || !value.is_ascii()
            || !value.starts_with('/')
            || value.contains('\0')
            || value.contains('\\')
        {
            return Err(ApphostBootstrapError::InvalidRoot);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when `entrypoint` would resolve inside this root.
    ///
    /// The entrypoint is package-relative, so containment is structural: it has
    /// already rejected `..`, `.`, absolute, and backslash forms, and this makes
    /// the relationship explicit and testable rather than implied.
    pub fn contains_entrypoint(&self, entrypoint: &Entrypoint) -> bool {
        !entrypoint.as_str().starts_with('/') && !entrypoint.as_str().contains("..")
    }
}

/// Manager-owned stable data directory, never supplied by package input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AppDataRoot(String);

impl AppDataRoot {
    pub fn new(value: impl Into<String>) -> Result<Self, ApphostBootstrapError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_ROOT_BYTES
            || !value.starts_with('/')
            || !value.is_ascii()
            || value.contains('\\')
            || value.contains('\0')
        {
            return Err(ApphostBootstrapError::InvalidDataRoot);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for AppDataRoot {
    type Error = ApphostBootstrapError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<AppDataRoot> for String {
    fn from(value: AppDataRoot) -> Self {
        value.0
    }
}

impl TryFrom<String> for LaunchRoot {
    type Error = ApphostBootstrapError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LaunchRoot> for String {
    fn from(value: LaunchRoot) -> Self {
        value.0
    }
}

/// Bounded, sanitised environment entries. No expansion, no shell metacharacters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SanitizedEnvironment(BTreeMap<String, String>);

impl SanitizedEnvironment {
    pub fn new(entries: BTreeMap<String, String>) -> Result<Self, ApphostBootstrapError> {
        if entries.len() > MAX_ENV_ENTRIES {
            return Err(ApphostBootstrapError::LimitExceeded("environment entries"));
        }
        for (key, value) in &entries {
            let key_ok = !key.is_empty()
                && key.len() <= MAX_ENV_KEY_BYTES
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
            let value_ok =
                value.len() <= MAX_ENV_VALUE_BYTES && value.is_ascii() && !value.contains('\0');
            if !key_ok || !value_ok {
                return Err(ApphostBootstrapError::InvalidEnvironment);
            }
        }
        Ok(Self(entries))
    }

    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

/// Descriptive resource request. Plan 369 makes no enforcement claim: these
/// values are recorded and echoed back, and a later sandbox plan is what makes
/// them real. The type says so in its name so nobody mistakes them for a limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptiveResourceRequest {
    pub requested_memory_bytes: u64,
    pub requested_open_files: u32,
}

/// The one bounded launch request the manager sends.
///
/// There is no decoder from application protocol messages or manifest bytes into
/// this type. In Plan 369 it has no production external constructor at all,
/// because there is no package/grant owner yet.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRequest {
    pub principal: crate::ManagerPrincipal,
    pub launch_profile: LaunchProfile,
    pub root: LaunchRoot,
    pub data_root: AppDataRoot,
    pub entrypoint: Entrypoint,
    pub argv: Vec<String>,
    pub environment: SanitizedEnvironment,
    pub resources: DescriptiveResourceRequest,
}

impl LaunchRequest {
    /// Validates the manager-created launch request.
    ///
    /// This is the single gate Plan 369 requires before any exec. It is pure, so
    /// it is unit-testable without a process, filesystem, or sandbox.
    pub fn validate(&self) -> Result<(), ApphostBootstrapError> {
        if !self.root.contains_entrypoint(&self.entrypoint) {
            return Err(ApphostBootstrapError::EntrypointEscapesRoot);
        }
        if self.argv.len() > MAX_ARGV_ITEMS {
            return Err(ApphostBootstrapError::LimitExceeded("argv items"));
        }
        for argument in &self.argv {
            if argument.len() > MAX_ARGV_ITEM_BYTES || !argument.is_ascii() {
                return Err(ApphostBootstrapError::InvalidArgv);
            }
            if argument.contains('\0') {
                return Err(ApphostBootstrapError::InvalidArgv);
            }
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ApphostBootstrapError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| ApphostBootstrapError::Malformed)
    }

    /// Exact-consumption decode.
    ///
    /// `serde_json::from_slice` tolerates trailing whitespace, so a length
    /// re-serialisation check is what makes "consumed exactly or not at all"
    /// true here. A second encoding is produced only for an already-bounded,
    /// already-validated payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, ApphostBootstrapError> {
        let request: Self =
            serde_json::from_slice(bytes).map_err(|_| ApphostBootstrapError::Malformed)?;
        request.validate()?;
        // Reject a payload that does not round-trip byte-for-byte, which is
        // exactly the trailing-data case `from_slice` waves through.
        if request.encode()? != bytes {
            return Err(ApphostBootstrapError::Malformed);
        }
        Ok(request)
    }
}

/// Typed apphost reply.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ApphostReply {
    #[serde(rename = "ready")]
    Ready {
        instance_id: String,
        attestation: Option<SandboxAttestation>,
    },
    #[serde(rename = "failed")]
    Failed {
        reason: ApphostFailureReason,
        diagnostic: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApphostFailureReason {
    /// The Plan 369 gate fired: `Secured` has no qualified backend.
    SecuredUnavailable,
    SandboxSetupFailed,
    InvalidRoot,
    EntrypointEscapesRoot,
    EntrypointNotFound,
    ExecFailed,
    /// The bootstrap itself was malformed or oversize.
    MalformedBootstrap,
}

/// Apphost → manager bootstrap handshake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApphostHandshake;

impl ApphostHandshake {
    pub fn encode() -> [u8; APPHOST_BOOTSTRAP_BYTES] {
        [
            APPHOST_BOOTSTRAP_MAGIC[0],
            APPHOST_BOOTSTRAP_MAGIC[1],
            APPHOST_BOOTSTRAP_MAGIC[2],
            APPHOST_BOOTSTRAP_MAGIC[3],
            APPHOST_BOOTSTRAP_MAJOR,
            APPHOST_BOOTSTRAP_MINOR,
            APPHOST_ROLE_BYTE,
            0,
            0,
        ]
    }

    /// Exact-consumption decode.
    pub fn decode(bytes: &[u8]) -> Result<Self, ApphostBootstrapError> {
        if bytes.len() != APPHOST_BOOTSTRAP_BYTES {
            return Err(ApphostBootstrapError::Truncated);
        }
        if bytes[..4] != APPHOST_BOOTSTRAP_MAGIC {
            return Err(ApphostBootstrapError::BadMagic);
        }
        if bytes[6] != APPHOST_ROLE_BYTE {
            return Err(ApphostBootstrapError::RoleMismatch);
        }
        if bytes[4] != APPHOST_BOOTSTRAP_MAJOR {
            return Err(ApphostBootstrapError::UnsupportedVersion);
        }
        if bytes[7..9] != [0, 0] {
            return Err(ApphostBootstrapError::Malformed);
        }
        Ok(Self)
    }
}

/// Re-exported so callers do not need a second import for the shared ceiling.
pub use i2pr_app_proto::MAX_IDENTIFIER_BYTES as MAX_APPHOST_IDENTIFIER_BYTES;

const _: () = assert!(MAX_IDENTIFIER_BYTES > 0);
