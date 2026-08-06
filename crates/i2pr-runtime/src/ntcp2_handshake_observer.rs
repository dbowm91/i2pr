//! Plan 092 privacy-safe handshake progress observer.
//!
//! The observer trait records operation metadata only: the bounded
//! stage name, expected/completed octet counts, the typed I/O result,
//! and the elapsed milliseconds for each operation. The observer never
//! receives raw payload bytes, private keys, Noise state, transcript
//! bytes, ciphertext, plaintext, RouterInfo bytes, packet captures,
//! or arbitrary remote error text. The observer callback is
//! synchronous, infallible, non-blocking, and allocation-bounded so
//! it cannot influence the handshake result or the cancellation
//! ordering.

use std::fmt;

/// A typed I/O outcome category for a single handshake operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandshakeIoResult {
    /// The operation completed exactly as expected.
    Completed,
    /// The peer closed the stream or returned EOF.
    Eof,
    /// The operation exceeded its deadline.
    Timeout,
    /// The caller cancelled the operation.
    Cancelled,
    /// The operating system rejected the operation.
    Failed,
    /// The operation is not applicable for this stage.
    NotApplicable,
}

impl HandshakeIoResult {
    /// Returns the bounded lowercase string used by the canonical
    /// Plan 092 observation schema.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Eof => "eof",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::NotApplicable => "not-applicable",
        }
    }
}

impl fmt::Display for HandshakeIoResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A metadata-only observation emitted by the runtime handshake
/// driver. The struct is constructed by the driver, passed by
/// reference to the observer, and discarded after the callback
/// returns. The struct never holds raw bytes, keys, transcripts, or
/// RouterInfo contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandshakeStageObservation {
    /// Bounded stage name. The closed allowlist is defined in the
    /// runtime handshake driver and matches the canonical Plan 092
    /// `i2pr-ntcp2-handshake-stage-v1` schema.
    pub stage: &'static str,
    /// Expected octet count for the operation, when applicable. The
    /// driver passes `None` for stages that do not have an octet
    /// count (for example, the encode-completed marker).
    pub expected_octets: Option<u32>,
    /// Completed octet count observed by the driver. The value is
    /// always `<= expected_octets` when `expected_octets` is set.
    pub completed_octets: Option<u32>,
    /// Typed I/O result. `NotApplicable` is used for stages that do
    /// not perform an I/O operation (for example, encode-completed).
    pub io_result: HandshakeIoResult,
    /// Elapsed wall-clock milliseconds since the operation began. The
    /// driver uses the tokio `Instant` for monotonic time so the value
    /// is non-negative and bounded by the handshake deadline.
    pub elapsed_millis: u32,
}

/// A metadata-only handshake progress observer.
///
/// The default no-op implementation discards every observation. The
/// `tools/i2pr-interop` and the focused tests supply recording
/// implementations that emit Plan 092 observation records. The trait
/// is intentionally infallible and synchronous: the driver never
/// awaits, locks, or allocates inside the callback, and a panicking
/// observer must never influence the handshake result.
pub trait HandshakeProgressObserver {
    /// Called once per handshake stage observation. The driver
    /// invokes the callback exactly once for every stage it
    /// recognises and is structurally unable to invoke the callback
    /// from inside an awaited future or with raw payload bytes.
    fn observe(&self, observation: HandshakeStageObservation);
}

/// The canonical no-op observer. Every call discards the
/// observation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopHandshakeObserver;

impl HandshakeProgressObserver for NoopHandshakeObserver {
    fn observe(&self, _observation: HandshakeStageObservation) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default observer is identity-equivalent: every observation
    /// is accepted and discarded.
    #[test]
    fn noop_observer_accepts_every_observation() {
        let observer = NoopHandshakeObserver;
        observer.observe(HandshakeStageObservation {
            stage: "session_request_write_completed",
            expected_octets: Some(287),
            completed_octets: Some(287),
            io_result: HandshakeIoResult::Completed,
            elapsed_millis: 4,
        });
    }

    /// The typed I/O result strings are closed and bounded.
    #[test]
    fn io_result_strings_are_bounded() {
        for value in [
            HandshakeIoResult::Completed,
            HandshakeIoResult::Eof,
            HandshakeIoResult::Timeout,
            HandshakeIoResult::Cancelled,
            HandshakeIoResult::Failed,
            HandshakeIoResult::NotApplicable,
        ] {
            let label = value.as_str();
            assert!(label.is_ascii());
            assert!(!label.contains('/'));
            assert!(!label.contains('\n'));
        }
    }
}
