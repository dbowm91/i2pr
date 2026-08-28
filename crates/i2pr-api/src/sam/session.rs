//! SAM session identifier and per-session resource limits view.
//!
//! Plan 137 §7 requires an explicit global SAM session registry
//! separate from `DestinationRegistry`. This module owns the
//! runtime-neutral session identifier, the per-session resource
//! counters, and the typed lookup surface that downstream plans
//! (138 `STREAM CONNECT/ACCEPT`; 139 `STREAM FORWARD`/naming) will
//! extend without changing the registry boundary.

use core::fmt;

use crate::sam::limits::{MAX_SAM_PENDING_ACCEPTS_PER_SESSION, SamLimits};

use super::MAX_SAM_SESSION_ID_BYTES;

/// A SAM session identifier (`ID=` value). Non-secret; carries no
/// destination key material.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SamSessionId(String);

impl SamSessionId {
    /// Wraps a session identifier. Returns `None` if the supplied
    /// value is empty, exceeds `MAX_SAM_SESSION_ID_BYTES`, or contains
    /// a SAM-illegal byte (`=`, ` `, `\t`, `\n`, `\r`, `\\`, `"`,
    /// or any control byte).
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_SAM_SESSION_ID_BYTES {
            return None;
        }
        if !is_valid_session_id(&value) {
            return None;
        }
        Some(Self(value))
    }

    /// Returns the session identifier as a borrowed string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SamSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for SamSessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Returns whether the supplied byte is legal inside a SAM session
/// identifier. SAM does not formally restrict the character set
/// beyond forbidding whitespace; i2pr narrows the alphabet to a
/// conservative subset that is safe to embed in log lines and
/// configuration diffs without quoting.
fn is_valid_session_id(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\' && byte != b'=')
}

/// Per-session resource counters view. Updated atomically by the
/// registry owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SamSessionCounters {
    /// Live STREAM sockets attached to the session.
    pub stream_attachment_count: u16,
    /// Pending `STREAM ACCEPT` backlog entries.
    pub pending_accept_count: u16,
}

impl SamSessionCounters {
    /// Returns the zero counters.
    pub const fn zero() -> Self {
        Self {
            stream_attachment_count: 0,
            pending_accept_count: 0,
        }
    }

    /// Validates that the supplied counters respect the documented
    /// per-session ceilings.
    pub fn validate_against(
        counters: Self,
        limits: SamLimits,
    ) -> Result<(), SamSessionCountersError> {
        if counters.stream_attachment_count > limits.max_stream_sockets_per_session {
            return Err(SamSessionCountersError::StreamAttachmentsExceedMaximum {
                actual: counters.stream_attachment_count,
                maximum: limits.max_stream_sockets_per_session,
            });
        }
        if counters.pending_accept_count > MAX_SAM_PENDING_ACCEPTS_PER_SESSION {
            return Err(SamSessionCountersError::PendingAcceptsExceedMaximum {
                actual: counters.pending_accept_count,
                maximum: MAX_SAM_PENDING_ACCEPTS_PER_SESSION,
            });
        }
        Ok(())
    }
}

/// Typed session-counter validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamSessionCountersError {
    /// Live STREAM socket count exceeded the per-session ceiling.
    StreamAttachmentsExceedMaximum {
        /// Observed count.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// Pending accept count exceeded the hard ceiling.
    PendingAcceptsExceedMaximum {
        /// Observed count.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
}

impl fmt::Display for SamSessionCountersError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StreamAttachmentsExceedMaximum { actual, maximum } => write!(
                formatter,
                "stream attachment count {actual} exceeds per-session maximum {maximum}"
            ),
            Self::PendingAcceptsExceedMaximum { actual, maximum } => write!(
                formatter,
                "pending accept count {actual} exceeds per-session maximum {maximum}"
            ),
        }
    }
}

impl std::error::Error for SamSessionCountersError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_accepts_typical_values() {
        assert!(SamSessionId::new("alpha").is_some());
        assert!(SamSessionId::new("session-1").is_some());
        assert!(SamSessionId::new("session_1").is_some());
        assert!(SamSessionId::new("UPPERCASE").is_some());
    }

    #[test]
    fn session_id_rejects_illegal_bytes() {
        assert!(SamSessionId::new("").is_none());
        let overlong = "x".repeat(MAX_SAM_SESSION_ID_BYTES + 1);
        assert!(SamSessionId::new(overlong).is_none());
        assert!(SamSessionId::new("with space").is_none());
        assert!(SamSessionId::new("with\ttab").is_none());
        assert!(SamSessionId::new("with=equals").is_none());
        assert!(SamSessionId::new("with\"quote").is_none());
        assert!(SamSessionId::new("with\\backslash").is_none());
        assert!(SamSessionId::new("with\nnewline").is_none());
        assert!(SamSessionId::new("with\rcr").is_none());
    }

    #[test]
    fn counters_validate_against_limits() {
        let limits = SamLimits::defaults();
        let counters = SamSessionCounters::zero();
        assert!(SamSessionCounters::validate_against(counters, limits).is_ok());
    }

    #[test]
    fn counters_reject_overflow() {
        let limits = SamLimits::defaults();
        let counters = SamSessionCounters {
            stream_attachment_count: limits.max_stream_sockets_per_session + 1,
            pending_accept_count: 0,
        };
        assert!(matches!(
            SamSessionCounters::validate_against(counters, limits),
            Err(SamSessionCountersError::StreamAttachmentsExceedMaximum { .. })
        ));
    }
}
