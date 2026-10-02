//! API version 1 authentication vocabulary (no token storage, no clocks).
//!
//! The daemon owns token generation/storage/expiry, the source-IP throttle,
//! and constant-time comparison. This module freezes the exact wire
//! vocabulary: the `Authenticate` method shape, the protected-token param,
//! the six standard error codes, and the bounded-behavior ceilings the daemon
//! must enforce (32 random bytes per token, one-day lifetime, 1024 live
//! tokens, 256-byte presented-token cap).

use crate::errors::ContractError;
use crate::limits::check_str;

/// Exact base method name that yields tokens for protected methods.
pub const AUTHENTICATE_METHOD: &str = "Authenticate";
/// Supported I2PControl API version for this workstream.
pub const SUPPORTED_API_VERSION: u32 = 1;
/// Random bytes per opaque token (donor bounded behavior, Plan 287).
pub const TOKEN_BYTES: usize = 32;
/// Finite monotonic token lifetime in seconds (one day).
pub const TOKEN_LIFETIME_SECS: u64 = 86_400;
/// Maximum live tokens with deterministic bounded eviction.
pub const MAX_LIVE_TOKENS: usize = 1_024;
/// Maximum presented token length in bytes.
pub const MAX_PRESENTED_TOKEN_LEN: usize = 256;
/// Param carrying the API version on `Authenticate`.
pub const PARAM_API: &str = "API";
/// Param carrying the password on `Authenticate`.
pub const PARAM_PASSWORD: &str = "Password";
/// Param carrying the opaque token on protected requests.
pub const PARAM_TOKEN: &str = "Token";
/// Compatibility header carrying the token when the Plan 286 contract
/// records it (mirrored here as vocabulary; daemon enforces agreement).
pub const TOKEN_HEADER: &str = "X-I2PControl-Token";

/// Standard I2PControl authentication errors (API version 1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthErrorCode {
    /// `-32001` password invalid.
    InvalidPassword,
    /// `-32002` protected request without a token.
    MissingToken,
    /// `-32003` invalid or unknown token.
    InvalidToken,
    /// `-32004` first use after expiry (token is then removed).
    ExpiredToken,
    /// `-32005` `Authenticate` without an API version.
    MissingApiVersion,
    /// `-32006` unsupported API version.
    UnsupportedApiVersion,
}

impl AuthErrorCode {
    /// Numeric wire code.
    pub const fn code(self) -> i64 {
        match self {
            Self::InvalidPassword => -32_001,
            Self::MissingToken => -32_002,
            Self::InvalidToken => -32_003,
            Self::ExpiredToken => -32_004,
            Self::MissingApiVersion => -32_005,
            Self::UnsupportedApiVersion => -32_006,
        }
    }

    /// Canonical message.
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidPassword => "Invalid password",
            Self::MissingToken => "Missing token",
            Self::InvalidToken => "Invalid token",
            Self::ExpiredToken => "Expired token",
            Self::MissingApiVersion => "Missing API version",
            Self::UnsupportedApiVersion => "Unsupported API version",
        }
    }

    /// All six codes in numeric order.
    pub const ALL: [Self; 6] = [
        Self::InvalidPassword,
        Self::MissingToken,
        Self::InvalidToken,
        Self::ExpiredToken,
        Self::MissingApiVersion,
        Self::UnsupportedApiVersion,
    ];

    /// Parses an exact numeric code.
    pub const fn from_code(code: i64) -> Option<Self> {
        match code {
            -32_001 => Some(Self::InvalidPassword),
            -32_002 => Some(Self::MissingToken),
            -32_003 => Some(Self::InvalidToken),
            -32_004 => Some(Self::ExpiredToken),
            -32_005 => Some(Self::MissingApiVersion),
            -32_006 => Some(Self::UnsupportedApiVersion),
            _ => None,
        }
    }
}

/// Validates a presented token shape (length only; never logs the value).
pub fn validate_presented_token(token: &str) -> Result<(), ContractError> {
    if token.is_empty() {
        return Err(ContractError::Malformed);
    }
    check_str(token, MAX_PRESENTED_TOKEN_LEN)
}
