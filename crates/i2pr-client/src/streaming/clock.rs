#![allow(dead_code)]

//! Deterministic clock abstraction for the streaming layer.
//!
//! The streaming state machine is fully synchronous and clock-driven.
//! Production code uses [`SystemClock`]; tests use [`ManualClock`]
//! so no `sleep()` is needed.

/// Abstraction over time sources. The streaming layer never calls
/// `Instant::now()` directly; it always goes through this trait.
pub trait Clock {
    /// Returns the current time in milliseconds since an arbitrary
    /// but fixed epoch. The value must be monotonically non-decreasing
    /// within a single clock instance.
    fn now_ms(&self) -> u64;
}

/// Production clock that reads the system monotonic clock.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::Instant::now().elapsed().as_millis() as u64
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
