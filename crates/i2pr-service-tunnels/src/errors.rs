//! Typed service-tunnel errors.
//!
//! All errors are structural and carry no secrets, no socket handles,
//! and no raw private application payloads.

#![forbid(unsafe_code)]

use thiserror::Error;

/// Typed failure for service-tunnel configuration and destination
/// reference validation.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ServiceTunnelError {
    /// Service tunnel identifier is malformed.
    #[error("invalid service tunnel id '{value}': {reason}")]
    InvalidId {
        /// Rejected value (truncated to a bounded prefix by callers).
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Service tunnel kind is unknown.
    #[error("invalid service tunnel kind '{value}': {reason}")]
    InvalidKind {
        /// Rejected value.
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Shared client group identifier is malformed.
    #[error("invalid shared client group id '{value}': {reason}")]
    InvalidGroup {
        /// Rejected value.
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Destination reference is malformed.
    #[error("invalid destination reference '{value}': {reason}")]
    InvalidDestinationRef {
        /// Rejected value (truncated by callers where needed).
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Static alias is malformed or conflicts.
    #[error("invalid static alias '{value}': {reason}")]
    InvalidAlias {
        /// Rejected value.
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Local listener specification is malformed or non-loopback.
    #[error("invalid local listener '{value}': {reason}")]
    InvalidListener {
        /// Rejected value.
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Server target is malformed or non-loopback.
    #[error("invalid server target '{value}': {reason}")]
    InvalidTarget {
        /// Rejected value.
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// Duplicate service tunnel identifier.
    #[error("duplicate service tunnel id '{value}'")]
    DuplicateId {
        /// Duplicated identifier.
        value: String,
    },
    /// Duplicate local listener bind endpoint.
    #[error("duplicate local listener bind '{value}'")]
    DuplicateListener {
        /// Duplicated bind.
        value: String,
    },
    /// Duplicate static alias.
    #[error("duplicate static alias '{value}'")]
    DuplicateAlias {
        /// Duplicated alias.
        value: String,
    },
    /// Internally contradictory service options.
    #[error("contradictory service options for '{id}': {reason}")]
    ContradictoryOptions {
        /// Owning service identifier.
        id: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// A count, length, or deadline exceeds its hard ceiling.
    #[error("service tunnel limit exceeded for '{field}': {reason}")]
    ExceedsCeiling {
        /// Field that exceeded its ceiling.
        field: &'static str,
        /// Machine-readable reason.
        reason: &'static str,
    },
}
