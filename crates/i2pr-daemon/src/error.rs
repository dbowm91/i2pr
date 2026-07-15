//! Typed errors and stable process exit-code mapping for the CLI boundary.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::config::ConfigError;
use i2pr_crypto::CryptoError;
use i2pr_storage::StorageError;

/// Stable initial exit-code categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExitCode {
    /// The requested command completed successfully.
    Success = 0,
    /// The configuration file could not be read.
    ConfigUnavailable = 10,
    /// The configuration file was not valid TOML/schema data.
    ConfigParse = 11,
    /// The configuration was syntactically valid but semantically invalid.
    ConfigSemantic = 12,
    /// The requested live router capability is not in this milestone.
    RuntimeNotImplemented = 20,
    /// Identity persistence failed validation or an operating-system operation.
    IdentityStorage = 30,
    /// Identity cryptographic execution failed.
    IdentityCrypto = 31,
    /// An unexpected internal failure occurred.
    Internal = 70,
}

impl ExitCode {
    /// Returns the process representation of this code.
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Errors returned while loading or executing the non-networked daemon shell.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// A configuration path could not be read.
    #[error("configuration file unavailable: {path}: {source}")]
    ConfigUnavailable {
        /// Path requested by the operator.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// A configuration was found but failed parsing or validation.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// A live run was requested before router runtime work exists.
    #[error(
        "router runtime is not implemented in this milestone; use --dry-run to validate configuration"
    )]
    RuntimeNotImplemented,
    /// Identity persistence failed.
    #[error(transparent)]
    IdentityStorage(#[from] StorageError),
    /// Identity cryptographic execution failed.
    #[error(transparent)]
    IdentityCrypto(#[from] CryptoError),
}

impl DaemonError {
    /// Maps an error to the stable initial process exit-code category.
    pub const fn exit_code(&self) -> ExitCode {
        match self {
            Self::ConfigUnavailable { .. } => ExitCode::ConfigUnavailable,
            Self::Config(error) => error.exit_code(),
            Self::RuntimeNotImplemented => ExitCode::RuntimeNotImplemented,
            Self::IdentityStorage(_) => ExitCode::IdentityStorage,
            Self::IdentityCrypto(_) => ExitCode::IdentityCrypto,
        }
    }
}
