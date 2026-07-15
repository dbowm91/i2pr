//! Deterministic foundations for state-machine and private-testnet tests.
//!
//! The testkit is not a production runtime dependency.  It provides a manual
//! monotonic clock, reproducible seeded randomness, and bounded fault-model
//! vocabulary that future in-memory links can consume.

#![forbid(unsafe_code)]

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// A stable 128-bit seed used to reproduce deterministic tests.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReproducibilitySeed([u8; 16]);

impl ReproducibilitySeed {
    /// Creates a seed from its raw bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Creates a seed from a numeric value using big-endian formatting.
    pub const fn from_u128(value: u128) -> Self {
        Self(value.to_be_bytes())
    }

    /// Returns the raw seed bytes.
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Display for ReproducibilitySeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ReproducibilitySeed {
    type Err = SeedParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.strip_prefix("0x").unwrap_or(value);
        if value.len() != 32 {
            return Err(SeedParseError::WrongLength {
                actual: value.len(),
            });
        }

        let mut bytes = [0_u8; 16];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_value(pair[0]).ok_or(SeedParseError::InvalidHex { index: index * 2 })?;
            let low = hex_value(pair[1]).ok_or(SeedParseError::InvalidHex {
                index: index * 2 + 1,
            })?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// Error returned when a reproducibility seed is not exactly 128 bits of hex.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeedParseError {
    /// The input did not contain 32 hexadecimal characters.
    WrongLength { actual: usize },
    /// A non-hexadecimal byte occurred at this zero-based character offset.
    InvalidHex { index: usize },
}

impl fmt::Display for SeedParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength { actual } => {
                write!(
                    formatter,
                    "seed must contain 32 hex characters, got {actual}"
                )
            }
            Self::InvalidHex { index } => write!(formatter, "seed contains invalid hex at {index}"),
        }
    }
}

impl std::error::Error for SeedParseError {}

/// A deterministic ChaCha8 generator for tests and simulations only.
#[derive(Debug)]
pub struct DeterministicRng {
    seed: ReproducibilitySeed,
    rng: ChaCha8Rng,
}

impl DeterministicRng {
    /// Creates a generator from a reproducibility seed.
    pub fn new(seed: ReproducibilitySeed) -> Self {
        let mut expanded_seed = [0_u8; 32];
        expanded_seed[..16].copy_from_slice(&seed.as_bytes());
        expanded_seed[16..].copy_from_slice(&seed.as_bytes());
        Self {
            seed,
            rng: ChaCha8Rng::from_seed(expanded_seed),
        }
    }

    /// Returns the seed used to initialize this generator.
    pub const fn seed(&self) -> ReproducibilitySeed {
        self.seed
    }

    /// Generates the next deterministic 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    /// Fills a caller-provided buffer deterministically.
    pub fn fill_bytes(&mut self, bytes: &mut [u8]) {
        self.rng.fill_bytes(bytes);
    }
}

/// A monotonic instant measured in nanoseconds from a manual clock's origin.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManualInstant(u64);

impl ManualInstant {
    /// Returns the elapsed duration represented by this instant.
    pub const fn elapsed(self) -> Duration {
        Duration::from_nanos(self.0)
    }
}

/// Error returned when a manual clock operation would overflow its range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockError {
    /// The requested duration cannot be represented in nanoseconds.
    Overflow,
}

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("manual clock duration overflow")
    }
}

impl std::error::Error for ClockError {}

/// A clonable monotonic clock advanced explicitly by a test.
#[derive(Clone, Debug, Default)]
pub struct ManualClock {
    now: Arc<AtomicU64>,
}

impl ManualClock {
    /// Creates a clock at its zero origin.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current manual instant without sleeping.
    pub fn now(&self) -> ManualInstant {
        ManualInstant(self.now.load(Ordering::Acquire))
    }

    /// Advances the clock and returns its new instant.
    pub fn advance(&self, duration: Duration) -> Result<ManualInstant, ClockError> {
        let nanos: u64 = duration
            .as_nanos()
            .try_into()
            .map_err(|_| ClockError::Overflow)?;
        loop {
            let current = self.now.load(Ordering::Acquire);
            let next = current.checked_add(nanos).ok_or(ClockError::Overflow)?;
            if self
                .now
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(ManualInstant(next));
            }
        }
    }

    /// Computes a deadline from the current instant without advancing time.
    pub fn deadline_after(&self, duration: Duration) -> Result<Deadline, ClockError> {
        let nanos: u64 = duration
            .as_nanos()
            .try_into()
            .map_err(|_| ClockError::Overflow)?;
        let at = self
            .now()
            .0
            .checked_add(nanos)
            .ok_or(ClockError::Overflow)?;
        Ok(Deadline(ManualInstant(at)))
    }
}

/// A clock-relative deadline used by deterministic state-machine tests.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Deadline(ManualInstant);

impl Deadline {
    /// Returns whether the deadline has passed at the supplied instant.
    pub const fn is_expired(self, now: ManualInstant) -> bool {
        now.0 >= self.0.0
    }

    /// Returns the instant at which this deadline expires.
    pub const fn instant(self) -> ManualInstant {
        self.0
    }
}

/// Bounded future link behavior reserved for the in-memory network testkit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultAction {
    /// Discard the affected unit.
    Drop,
    /// Hold the affected unit for a deterministic duration.
    Delay(Duration),
    /// Deliver the original plus the requested number of copies.
    Duplicate { copies: u8 },
    /// Allow a bounded reorder window.
    Reorder { window: u16 },
    /// Deliver no more than the requested number of bytes.
    Truncate { max_bytes: u32 },
    /// Close the simulated link.
    Disconnect,
}

impl FaultAction {
    /// Creates a duplicate action with at least one extra copy.
    pub const fn duplicate(copies: u8) -> Result<Self, FaultError> {
        if copies == 0 {
            Err(FaultError::ZeroValue)
        } else {
            Ok(Self::Duplicate { copies })
        }
    }

    /// Creates a reorder action with a nonzero bounded window.
    pub const fn reorder(window: u16) -> Result<Self, FaultError> {
        if window == 0 {
            Err(FaultError::ZeroValue)
        } else {
            Ok(Self::Reorder { window })
        }
    }

    /// Creates a truncation action with a nonzero byte bound.
    pub const fn truncate(max_bytes: u32) -> Result<Self, FaultError> {
        if max_bytes == 0 {
            Err(FaultError::ZeroValue)
        } else {
            Ok(Self::Truncate { max_bytes })
        }
    }
}

/// Error returned when a fault action would have no observable effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultError {
    /// A count, window, or byte bound was zero.
    ZeroValue,
}

impl fmt::Display for FaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("fault action value must be nonzero")
    }
}

impl std::error::Error for FaultError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_format_round_trips() {
        let seed = ReproducibilitySeed::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let encoded = seed.to_string();
        assert_eq!(encoded, "0123456789abcdef0123456789abcdef");
        assert_eq!(
            encoded.parse::<ReproducibilitySeed>().expect("valid seed"),
            seed
        );
        assert_eq!(
            format!("0x{encoded}")
                .parse::<ReproducibilitySeed>()
                .expect("prefixed seed"),
            seed
        );
    }

    #[test]
    fn same_seed_produces_same_sequence() {
        let seed = ReproducibilitySeed::from_u128(7);
        let mut left = DeterministicRng::new(seed);
        let mut right = DeterministicRng::new(seed);
        let left_values: Vec<_> = (0..8).map(|_| left.next_u64()).collect();
        let right_values: Vec<_> = (0..8).map(|_| right.next_u64()).collect();
        assert_eq!(left_values, right_values);
    }

    #[test]
    fn manual_clock_controls_deadlines_without_sleep() {
        let clock = ManualClock::new();
        let deadline = clock
            .deadline_after(Duration::from_secs(5))
            .expect("deadline");
        assert!(!deadline.is_expired(clock.now()));
        clock.advance(Duration::from_secs(5)).expect("advance");
        assert!(deadline.is_expired(clock.now()));
    }

    #[test]
    fn fault_values_are_bounded_and_nonzero() {
        assert!(FaultAction::duplicate(0).is_err());
        assert_eq!(
            FaultAction::truncate(4).expect("valid truncation"),
            FaultAction::Truncate { max_bytes: 4 }
        );
    }
}
