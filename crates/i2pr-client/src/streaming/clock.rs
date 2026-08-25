#![allow(dead_code)]

//! Deterministic clock abstraction for the streaming layer.
//!
//! The streaming state machine is fully synchronous and clock-driven.
//! Production callers always inject timestamps explicitly through
//! `now_ms` arguments; tests use [`ManualClock`] to drive deterministic
//! retransmit / timeout behavior. [`SystemClock`] is retained as a
//! monotonic reference clock so production adapters that want the
//! system clock can take a single `Instant` origin rather than
//! fabricating an `Instant::now().elapsed()` value that is always
//! effectively zero.

use std::time::Instant;

/// Abstraction over time sources. The streaming layer never calls
/// `Instant::now()` directly; it always goes through this trait or
/// receives `now_ms` from the caller.
pub trait Clock {
    /// Returns the current time in milliseconds since an arbitrary
    /// but fixed epoch. The value must be monotonically non-decreasing
    /// within a single clock instance.
    fn now_ms(&self) -> u64;
}

/// Production monotonic clock. The clock captures the moment it is
/// constructed and reports elapsed time from that origin in
/// milliseconds, so callers that share a single [`SystemClock`]
/// observe a strictly non-decreasing sequence. The previous
/// implementation called `Instant::now().elapsed()` directly inside
/// `now_ms`, which always returned effectively zero; that defect is
/// corrected here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// Constructs a new monotonic clock anchored at the supplied
    /// origin.
    pub const fn new(origin: Instant) -> Self {
        Self { origin }
    }

    /// Returns the clock origin `Instant`.
    pub const fn origin(&self) -> Instant {
        self.origin
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        let elapsed = self.origin.elapsed();
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    }
}

/// Deterministic controllable clock for tests.
///
/// The clock starts at zero and advances only when [`ManualClock::advance`]
/// is called.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct ManualClock {
    now_ms: u64,
}

impl ManualClock {
    /// Creates a new manual clock starting at the given time.
    pub const fn new(initial_ms: u64) -> Self {
        Self { now_ms: initial_ms }
    }

    /// Advances the clock by the given number of milliseconds.
    pub fn advance(&mut self, delta_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(delta_ms);
    }

    /// Sets the clock to an absolute time (useful for timeout tests).
    pub fn set(&mut self, absolute_ms: u64) {
        self.now_ms = absolute_ms;
    }

    /// Returns the current time without advancing.
    pub const fn current_ms(&self) -> u64 {
        self.now_ms
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn system_clock_is_monotonic_and_can_advance() {
        let origin = Instant::now();
        let clock = SystemClock::new(origin);
        let first = clock.now_ms();
        sleep(Duration::from_millis(20));
        let second = clock.now_ms();
        assert!(
            second > first,
            "SystemClock must advance: first={first} second={second}"
        );
        assert!(second >= 20);
    }

    #[test]
    fn manual_clock_drives_deterministic_timeouts() {
        let mut clock = ManualClock::new(0);
        assert_eq!(clock.now_ms(), 0);
        clock.advance(50);
        assert_eq!(clock.now_ms(), 50);
        clock.set(1000);
        assert_eq!(clock.now_ms(), 1000);
    }
}
