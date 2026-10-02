//! Typed contract errors (no `anyhow` in this library crate).

use thiserror::Error;

/// Stable typed classification for every contract rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum ContractError {
    /// Input exceeds a caller-visible ceiling.
    #[error("value exceeds bound")]
    OverBound,
    /// Unknown or unsupported literal (method/action/type/selector/option).
    #[error("unknown literal")]
    UnknownLiteral,
    /// Known literal with wrong casing or spelling.
    #[error("literal case or spelling mismatch")]
    CaseMismatch,
    /// Malformed envelope, params shape, or value encoding.
    #[error("malformed value")]
    Malformed,
    /// Duplicate key or field where uniqueness is required.
    #[error("duplicate field")]
    Duplicate,
    /// Truncated input.
    #[error("truncated input")]
    Truncated,
    /// A secret-classified value was presented where it must not appear.
    #[error("secret handling violation")]
    SecretViolation,
}
