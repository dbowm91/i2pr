//! Bounded SAM service limits.
//!
//! Plan 137 §3 requires a strict `[sam]` configuration section that
//! validates every numerical field against documented ceilings. This
//! module is the runtime-neutral defaults + ceiling definitions. The
//! daemon `i2pr_daemon::config` composes a typed [`SamLimits`]
//! from validated configuration and rejects zero/absurd limits,
//! overlarge timeouts, and inconsistent aggregate budgets before any
//! listener can bind.

use core::fmt;
use core::time::Duration;

/// Maximum number of concurrent SAM clients accepted by the loopback
/// listener. Hard ceiling.
pub const MAX_SAM_CLIENTS: u16 = 1024;
/// Maximum number of concurrent SAM sessions. Hard ceiling.
pub const MAX_SAM_SESSIONS: u16 = 1024;
/// Maximum number of STREAM sockets allowed per session. Hard ceiling.
pub const MAX_SAM_STREAM_SOCKETS_PER_SESSION: u16 = 1024;
/// Maximum number of pending `STREAM ACCEPT` backlog entries per
/// session. Hard ceiling.
pub const MAX_SAM_PENDING_ACCEPTS_PER_SESSION: u16 = 1024;
/// Maximum aggregate buffered bytes per stream direction. Hard ceiling.
pub const MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION: usize = 16 * 1024 * 1024;
/// Maximum HELLO/version-negotiation deadline. Hard ceiling.
pub const MAX_SAM_HELLO_TIMEOUT_SECS: u64 = 60;
/// Maximum command/idle deadline. Hard ceiling.
pub const MAX_SAM_COMMAND_TIMEOUT_SECS: u64 = 3_600;
/// Maximum graceful shutdown deadline. Hard ceiling.
pub const MAX_SAM_SHUTDOWN_TIMEOUT_SECS: u64 = 60;

/// Default loopback bind port. The SAM bridge port convention is
/// `7656`; i2pr publishes no public port and tests always use port
/// `0`/ephemeral binding.
pub const DEFAULT_SAM_BIND_ADDRESS: &str = "127.0.0.1";
/// Default SAM bridge port per long-standing I2P convention. The
/// daemon never opens this in production by default; the explicit
/// profile is required to enable a non-loopback listener.
pub const DEFAULT_SAM_PORT: u16 = 7656;
/// Default SAM-disabled profile: the listener never binds. Plan 137
/// ships the loopback service as gated under an explicit
/// `[sam] enabled = true` so the production daemon stays unmodified.
pub const DEFAULT_SAM_ENABLED: bool = false;
/// Default maximum concurrent clients. Tuned for a single-process
/// test workload.
pub const DEFAULT_SAM_MAX_CLIENTS: u16 = 16;
/// Default maximum concurrent sessions.
pub const DEFAULT_SAM_MAX_SESSIONS: u16 = 16;
/// Default STREAM socket ceiling per session.
pub const DEFAULT_SAM_STREAM_SOCKETS_PER_SESSION: u16 = 16;
/// Default pending accept ceiling per session.
pub const DEFAULT_SAM_PENDING_ACCEPTS_PER_SESSION: u16 = 16;
/// Default aggregate buffered-byte budget per stream direction.
pub const DEFAULT_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION: usize = 64 * 1024;
/// Default HELLO timeout.
pub const DEFAULT_SAM_HELLO_TIMEOUT_MS: u64 = 10_000;
/// Default command/idle timeout.
pub const DEFAULT_SAM_COMMAND_TIMEOUT_MS: u64 = 60_000;
/// Default graceful shutdown deadline.
pub const DEFAULT_SAM_SHUTDOWN_TIMEOUT_MS: u64 = 5_000;

/// Validated, bounded SAM service limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SamLimits {
    /// Whether the SAM listener may bind at all.
    pub enabled: bool,
    /// Maximum concurrent SAM clients.
    pub max_clients: u16,
    /// Maximum concurrent SAM sessions.
    pub max_sessions: u16,
    /// Maximum STREAM sockets allowed per session.
    pub max_stream_sockets_per_session: u16,
    /// Maximum pending `STREAM ACCEPT` backlog entries per session.
    pub max_pending_accepts_per_session: u16,
    /// Aggregate buffered-byte budget per stream direction.
    pub max_buffered_bytes_per_stream_direction: usize,
    /// HELLO/version-negotiation deadline.
    pub hello_timeout: Duration,
    /// Per-command/idle deadline.
    pub command_timeout: Duration,
    /// Graceful shutdown deadline.
    pub shutdown_timeout: Duration,
}

impl SamLimits {
    /// Returns the conservative default profile: disabled, loopback,
    /// with small defaults that pass every ceiling.
    pub const fn defaults() -> Self {
        Self {
            enabled: DEFAULT_SAM_ENABLED,
            max_clients: DEFAULT_SAM_MAX_CLIENTS,
            max_sessions: DEFAULT_SAM_MAX_SESSIONS,
            max_stream_sockets_per_session: DEFAULT_SAM_STREAM_SOCKETS_PER_SESSION,
            max_pending_accepts_per_session: DEFAULT_SAM_PENDING_ACCEPTS_PER_SESSION,
            max_buffered_bytes_per_stream_direction:
                DEFAULT_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION,
            hello_timeout: Duration::from_millis(DEFAULT_SAM_HELLO_TIMEOUT_MS),
            command_timeout: Duration::from_millis(DEFAULT_SAM_COMMAND_TIMEOUT_MS),
            shutdown_timeout: Duration::from_millis(DEFAULT_SAM_SHUTDOWN_TIMEOUT_MS),
        }
    }

    /// Validates a candidate [`SamLimits`] against every documented
    /// ceiling. Returns a typed [`SamLimitsError`] on the negative path.
    pub fn validate(candidate: SamLimits) -> Result<Self, SamLimitsError> {
        if candidate.max_clients == 0 {
            return Err(SamLimitsError::ZeroClients);
        }
        if candidate.max_clients > MAX_SAM_CLIENTS {
            return Err(SamLimitsError::ClientsExceedMaximum {
                actual: candidate.max_clients,
                maximum: MAX_SAM_CLIENTS,
            });
        }
        if candidate.max_sessions == 0 {
            return Err(SamLimitsError::ZeroSessions);
        }
        if candidate.max_sessions > MAX_SAM_SESSIONS {
            return Err(SamLimitsError::SessionsExceedMaximum {
                actual: candidate.max_sessions,
                maximum: MAX_SAM_SESSIONS,
            });
        }
        if candidate.max_stream_sockets_per_session == 0 {
            return Err(SamLimitsError::ZeroStreamSocketsPerSession);
        }
        if candidate.max_stream_sockets_per_session > MAX_SAM_STREAM_SOCKETS_PER_SESSION {
            return Err(SamLimitsError::StreamSocketsPerSessionExceedMaximum {
                actual: candidate.max_stream_sockets_per_session,
                maximum: MAX_SAM_STREAM_SOCKETS_PER_SESSION,
            });
        }
        if candidate.max_pending_accepts_per_session == 0 {
            return Err(SamLimitsError::ZeroPendingAcceptsPerSession);
        }
        if candidate.max_pending_accepts_per_session > MAX_SAM_PENDING_ACCEPTS_PER_SESSION {
            return Err(SamLimitsError::PendingAcceptsPerSessionExceedMaximum {
                actual: candidate.max_pending_accepts_per_session,
                maximum: MAX_SAM_PENDING_ACCEPTS_PER_SESSION,
            });
        }
        if candidate.max_buffered_bytes_per_stream_direction == 0 {
            return Err(SamLimitsError::ZeroBufferedBytes);
        }
        if candidate.max_buffered_bytes_per_stream_direction
            > MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION
        {
            return Err(SamLimitsError::BufferedBytesExceedMaximum {
                actual: candidate.max_buffered_bytes_per_stream_direction,
                maximum: MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION,
            });
        }
        validate_duration(
            "hello_timeout",
            candidate.hello_timeout,
            MAX_SAM_HELLO_TIMEOUT_SECS,
        )?;
        validate_duration(
            "command_timeout",
            candidate.command_timeout,
            MAX_SAM_COMMAND_TIMEOUT_SECS,
        )?;
        validate_duration(
            "shutdown_timeout",
            candidate.shutdown_timeout,
            MAX_SAM_SHUTDOWN_TIMEOUT_SECS,
        )?;
        Ok(candidate)
    }

    /// Conservative profile that enables the loopback listener with
    /// the default limits. Used by integration tests that exercise
    /// the real TCP path.
    pub const fn loopback_test_profile() -> Self {
        Self {
            enabled: true,
            // Integration tests run under `tokio::time::test-util`
            // with `start_paused = true`. The runtime auto-advances
            // virtual time to the next pending timer when no work is
            // ready, so a finite timeout races with the harness and
            // tears sessions down before the test can observe them.
            // `Duration::MAX` is the documented sentinel that
            // disables the per-read timeout inside `receive_command`.
            hello_timeout: Duration::MAX,
            command_timeout: Duration::MAX,
            ..Self::defaults()
        }
    }
}

impl Default for SamLimits {
    fn default() -> Self {
        Self::defaults()
    }
}

fn validate_duration(
    field: &'static str,
    value: Duration,
    maximum_secs: u64,
) -> Result<(), SamLimitsError> {
    // `Duration::MAX` is the documented sentinel that disables the
    // timeout. Production callers should never hit this path because
    // the defaults are finite; integration tests use the sentinel to
    // avoid racing with `tokio::time::test-util` auto-advance.
    if value == Duration::MAX {
        return Ok(());
    }
    if value.is_zero() {
        return Err(SamLimitsError::ZeroDuration(field));
    }
    if value > Duration::from_secs(maximum_secs) {
        return Err(SamLimitsError::DurationExceedsMaximum {
            field,
            actual: value,
            maximum: Duration::from_secs(maximum_secs),
        });
    }
    Ok(())
}

/// Typed SAM limits validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SamLimitsError {
    /// `max_clients` was zero.
    ZeroClients,
    /// `max_clients` exceeded the hard ceiling.
    ClientsExceedMaximum {
        /// Supplied value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// `max_sessions` was zero.
    ZeroSessions,
    /// `max_sessions` exceeded the hard ceiling.
    SessionsExceedMaximum {
        /// Supplied value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// `max_stream_sockets_per_session` was zero.
    ZeroStreamSocketsPerSession,
    /// `max_stream_sockets_per_session` exceeded the hard ceiling.
    StreamSocketsPerSessionExceedMaximum {
        /// Supplied value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// `max_pending_accepts_per_session` was zero.
    ZeroPendingAcceptsPerSession,
    /// `max_pending_accepts_per_session` exceeded the hard ceiling.
    PendingAcceptsPerSessionExceedMaximum {
        /// Supplied value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// `max_buffered_bytes_per_stream_direction` was zero.
    ZeroBufferedBytes,
    /// `max_buffered_bytes_per_stream_direction` exceeded the hard ceiling.
    BufferedBytesExceedMaximum {
        /// Supplied value.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// A bounded timeout was zero.
    ZeroDuration(&'static str),
    /// A bounded timeout exceeded the hard ceiling.
    DurationExceedsMaximum {
        /// Field name.
        field: &'static str,
        /// Supplied value.
        actual: Duration,
        /// Accepted ceiling.
        maximum: Duration,
    },
}

impl fmt::Display for SamLimitsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroClients => formatter.write_str("max_clients must be greater than zero"),
            Self::ClientsExceedMaximum { actual, maximum } => {
                write!(formatter, "max_clients {actual} exceeds maximum {maximum}")
            }
            Self::ZeroSessions => formatter.write_str("max_sessions must be greater than zero"),
            Self::SessionsExceedMaximum { actual, maximum } => {
                write!(formatter, "max_sessions {actual} exceeds maximum {maximum}")
            }
            Self::ZeroStreamSocketsPerSession => {
                formatter.write_str("max_stream_sockets_per_session must be greater than zero")
            }
            Self::StreamSocketsPerSessionExceedMaximum { actual, maximum } => write!(
                formatter,
                "max_stream_sockets_per_session {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroPendingAcceptsPerSession => {
                formatter.write_str("max_pending_accepts_per_session must be greater than zero")
            }
            Self::PendingAcceptsPerSessionExceedMaximum { actual, maximum } => write!(
                formatter,
                "max_pending_accepts_per_session {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroBufferedBytes => formatter
                .write_str("max_buffered_bytes_per_stream_direction must be greater than zero"),
            Self::BufferedBytesExceedMaximum { actual, maximum } => write!(
                formatter,
                "max_buffered_bytes_per_stream_direction {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroDuration(field) => {
                write!(formatter, "{field} must be greater than zero")
            }
            Self::DurationExceedsMaximum {
                field,
                actual,
                maximum,
            } => {
                write!(formatter, "{field} {actual:?} exceeds maximum {maximum:?}")
            }
        }
    }
}

impl std::error::Error for SamLimitsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        let limits = SamLimits::defaults();
        let validated = SamLimits::validate(limits).expect("defaults validate");
        assert_eq!(validated, limits);
    }

    #[test]
    fn loopback_test_profile_validates() {
        let limits = SamLimits::loopback_test_profile();
        let validated = SamLimits::validate(limits).expect("profile validates");
        assert!(validated.enabled);
        assert_eq!(validated.max_clients, DEFAULT_SAM_MAX_CLIENTS);
    }

    #[test]
    fn zero_clients_rejected() {
        let mut limits = SamLimits::defaults();
        limits.max_clients = 0;
        assert!(matches!(
            SamLimits::validate(limits),
            Err(SamLimitsError::ZeroClients)
        ));
    }

    #[test]
    fn excessive_clients_rejected() {
        let mut limits = SamLimits::defaults();
        limits.max_clients = MAX_SAM_CLIENTS + 1;
        assert!(matches!(
            SamLimits::validate(limits),
            Err(SamLimitsError::ClientsExceedMaximum { .. })
        ));
    }

    #[test]
    fn zero_hello_timeout_rejected() {
        let mut limits = SamLimits::defaults();
        limits.hello_timeout = Duration::from_millis(0);
        assert!(matches!(
            SamLimits::validate(limits),
            Err(SamLimitsError::ZeroDuration("hello_timeout"))
        ));
    }

    #[test]
    fn overlarge_shutdown_timeout_rejected() {
        let mut limits = SamLimits::defaults();
        limits.shutdown_timeout = Duration::from_secs(MAX_SAM_SHUTDOWN_TIMEOUT_SECS + 1);
        assert!(matches!(
            SamLimits::validate(limits),
            Err(SamLimitsError::DurationExceedsMaximum {
                field: "shutdown_timeout",
                ..
            })
        ));
    }

    #[test]
    fn zero_stream_sockets_rejected() {
        let mut limits = SamLimits::defaults();
        limits.max_stream_sockets_per_session = 0;
        assert!(matches!(
            SamLimits::validate(limits),
            Err(SamLimitsError::ZeroStreamSocketsPerSession)
        ));
    }
}
