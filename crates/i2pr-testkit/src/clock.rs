use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

/// Maximum sleepers held by one manual clock unless a smaller limit is chosen.
pub const MAX_PENDING_TIMERS: usize = 4096;

/// A monotonic instant measured in nanoseconds from a clock origin.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManualInstant(u64);

impl ManualInstant {
    /// Returns the elapsed duration represented by this instant.
    pub const fn elapsed(self) -> Duration {
        Duration::from_nanos(self.0)
    }

    /// Returns the raw monotonic nanosecond value.
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Creates an instant from a monotonic nanosecond value.
    pub const fn from_nanos(value: u64) -> Self {
        Self(value)
    }
}

/// An alias naming the runtime-neutral monotonic representation.
pub type MonotonicInstant = ManualInstant;

/// Error returned by deterministic clock operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockError {
    /// A duration or deadline would overflow nanoseconds.
    Overflow,
    /// The clock has been torn down because all clock handles were dropped.
    Closed,
    /// The explicit pending-timer limit was reached.
    TimerLimit { maximum: usize },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow => formatter.write_str("monotonic clock duration overflow"),
            Self::Closed => formatter.write_str("monotonic clock is closed"),
            Self::TimerLimit { maximum } => {
                write!(formatter, "pending timer limit {maximum} reached")
            }
        }
    }
}

impl std::error::Error for ClockError {}

/// A clock-relative deadline.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Deadline(ManualInstant);

impl Deadline {
    /// Returns whether the deadline has passed at the supplied instant.
    pub const fn is_expired(self, now: ManualInstant) -> bool {
        now.0 >= self.0.0
    }

    /// Returns the deadline instant.
    pub const fn instant(self) -> ManualInstant {
        self.0
    }
}

/// The narrow clock contract used by simulation callers.
pub trait MonotonicClock {
    /// Future returned by the clock's bounded sleep operation.
    type Sleep: Future<Output = Result<(), ClockError>>;

    /// Returns current monotonic time.
    fn now(&self) -> ManualInstant;
    /// Calculates a checked deadline.
    fn deadline_after(&self, duration: Duration) -> Result<Deadline, ClockError>;
    /// Waits until a deadline without wall-clock fallback.
    fn sleep_until(&self, deadline: Deadline) -> Self::Sleep;
}

#[derive(Debug)]
struct Waiter {
    done: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl Waiter {
    fn new() -> Self {
        Self {
            done: AtomicBool::new(false),
            waker: Mutex::new(None),
        }
    }

    fn wake(&self) {
        self.done.store(true, Ordering::Release);
        if let Ok(mut waker) = self.waker.lock() {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
        }
    }
}

#[derive(Debug)]
struct ClockState {
    now: u64,
    next_sequence: u64,
    pending: BTreeMap<(u64, u64), Arc<Waiter>>,
    closed: bool,
    maximum: usize,
}

#[derive(Debug)]
struct ClockInner {
    state: Mutex<ClockState>,
    handles: AtomicU64,
}

/// A clonable manually advanced monotonic clock.
#[derive(Debug)]
pub struct ManualClock {
    inner: Arc<ClockInner>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::with_max_timers(MAX_PENDING_TIMERS).expect("default timer bound is valid")
    }
}

impl Clone for ManualClock {
    fn clone(&self) -> Self {
        self.inner.handles.fetch_add(1, Ordering::AcqRel);
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Drop for ManualClock {
    fn drop(&mut self) {
        if self.inner.handles.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let waiters = if let Ok(mut state) = self.inner.state.lock() {
            state.closed = true;
            std::mem::take(&mut state.pending)
                .into_values()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for waiter in waiters {
            waiter.wake();
        }
    }
}

impl ManualClock {
    /// Creates a clock at zero with the default timer bound.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a clock with an explicit nonzero pending-timer bound.
    pub fn with_max_timers(maximum: usize) -> Result<Self, ClockError> {
        if maximum == 0 || maximum > MAX_PENDING_TIMERS {
            return Err(ClockError::TimerLimit {
                maximum: MAX_PENDING_TIMERS,
            });
        }
        Ok(Self {
            inner: Arc::new(ClockInner {
                state: Mutex::new(ClockState {
                    now: 0,
                    next_sequence: 0,
                    pending: BTreeMap::new(),
                    closed: false,
                    maximum,
                }),
                handles: AtomicU64::new(1),
            }),
        })
    }

    /// Returns current manual time.
    pub fn now(&self) -> ManualInstant {
        self.inner
            .state
            .lock()
            .map(|state| ManualInstant(state.now))
            .unwrap_or_else(|_| ManualInstant(0))
    }

    /// Advances the clock and wakes all due sleepers in deadline/sequence order.
    pub fn advance(&self, duration: Duration) -> Result<ManualInstant, ClockError> {
        let nanos = duration
            .as_nanos()
            .try_into()
            .map_err(|_| ClockError::Overflow)?;
        let waiters = {
            let mut state = self.inner.state.lock().map_err(|_| ClockError::Closed)?;
            if state.closed {
                return Err(ClockError::Closed);
            }
            let next = state.now.checked_add(nanos).ok_or(ClockError::Overflow)?;
            state.now = next;
            let due = state
                .pending
                .keys()
                .copied()
                .take_while(|(at, _)| *at <= next)
                .collect::<Vec<_>>();
            due.into_iter()
                .filter_map(|key| state.pending.remove(&key))
                .collect::<Vec<_>>()
        };
        for waiter in waiters {
            waiter.wake();
        }
        Ok(self.now())
    }

    /// Computes a deadline without advancing time.
    pub fn deadline_after(&self, duration: Duration) -> Result<Deadline, ClockError> {
        let nanos = duration
            .as_nanos()
            .try_into()
            .map_err(|_| ClockError::Overflow)?;
        let state = self.inner.state.lock().map_err(|_| ClockError::Closed)?;
        if state.closed {
            return Err(ClockError::Closed);
        }
        Ok(Deadline(ManualInstant(
            state.now.checked_add(nanos).ok_or(ClockError::Overflow)?,
        )))
    }

    /// Returns the number of registered sleepers.
    pub fn pending_timers(&self) -> usize {
        self.inner
            .state
            .lock()
            .map(|state| state.pending.len())
            .unwrap_or(0)
    }

    /// Returns a future that completes at the supplied deadline.
    pub fn sleep_until(&self, deadline: Deadline) -> ManualSleep {
        ManualSleep {
            clock: Arc::clone(&self.inner),
            deadline,
            waiter: Arc::new(Waiter::new()),
            key: None,
        }
    }

    /// Returns a future that completes after the supplied duration.
    pub fn sleep_for(&self, duration: Duration) -> Result<ManualSleep, ClockError> {
        Ok(self.sleep_until(self.deadline_after(duration)?))
    }
}

impl MonotonicClock for ManualClock {
    type Sleep = ManualSleep;

    fn now(&self) -> ManualInstant {
        self.now()
    }

    fn deadline_after(&self, duration: Duration) -> Result<Deadline, ClockError> {
        self.deadline_after(duration)
    }

    fn sleep_until(&self, deadline: Deadline) -> Self::Sleep {
        self.sleep_until(deadline)
    }
}

/// A manually woken clock sleep. Dropping it unregisters its waiter.
#[derive(Debug)]
pub struct ManualSleep {
    clock: Arc<ClockInner>,
    deadline: Deadline,
    waiter: Arc<Waiter>,
    key: Option<(u64, u64)>,
}

impl Future for ManualSleep {
    type Output = Result<(), ClockError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let clock = Arc::clone(&self.clock);
        let mut state = match clock.state.lock() {
            Ok(state) => state,
            Err(_) => return Poll::Ready(Err(ClockError::Closed)),
        };
        if state.closed {
            return Poll::Ready(Err(ClockError::Closed));
        }
        if self.waiter.done.load(Ordering::Acquire)
            || state.now >= self.deadline.instant().as_nanos()
        {
            if let Some(key) = self.key.take() {
                state.pending.remove(&key);
            }
            self.waiter.done.store(true, Ordering::Release);
            return Poll::Ready(Ok(()));
        }
        if self.key.is_none() {
            if state.pending.len() >= state.maximum {
                return Poll::Ready(Err(ClockError::TimerLimit {
                    maximum: state.maximum,
                }));
            }
            let key = (self.deadline.instant().as_nanos(), state.next_sequence);
            let Some(next_sequence) = state.next_sequence.checked_add(1) else {
                return Poll::Ready(Err(ClockError::Overflow));
            };
            state.next_sequence = next_sequence;
            state.pending.insert(key, Arc::clone(&self.waiter));
            self.key = Some(key);
        }
        if let Ok(mut waker) = self.waiter.waker.lock() {
            *waker = Some(context.waker().clone());
        }
        Poll::Pending
    }
}

impl Drop for ManualSleep {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            if let Ok(mut state) = self.clock.state.lock() {
                state.pending.remove(&key);
            }
        }
    }
}

/// A production-style Tokio monotonic clock used by testkit adapters.
#[derive(Clone, Debug)]
pub struct TokioClock {
    origin: tokio::time::Instant,
}

impl TokioClock {
    /// Starts a clock at the current Tokio instant.
    pub fn new() -> Self {
        Self {
            origin: tokio::time::Instant::now(),
        }
    }
}

impl Default for TokioClock {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct TokioSleep {
    sleep: Pin<Box<tokio::time::Sleep>>,
}

impl Future for TokioSleep {
    type Output = Result<(), ClockError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.sleep.as_mut().poll(context).map(|_| Ok(()))
    }
}

impl MonotonicClock for TokioClock {
    type Sleep = TokioSleep;

    fn now(&self) -> ManualInstant {
        ManualInstant(
            tokio::time::Instant::now()
                .saturating_duration_since(self.origin)
                .as_nanos()
                .try_into()
                .unwrap_or(u64::MAX),
        )
    }

    fn deadline_after(&self, duration: Duration) -> Result<Deadline, ClockError> {
        let nanos = duration
            .as_nanos()
            .try_into()
            .map_err(|_| ClockError::Overflow)?;
        Ok(Deadline(ManualInstant(
            self.now()
                .as_nanos()
                .checked_add(nanos)
                .ok_or(ClockError::Overflow)?,
        )))
    }

    fn sleep_until(&self, deadline: Deadline) -> Self::Sleep {
        let instant = self.origin + deadline.instant().elapsed();
        TokioSleep {
            sleep: Box::pin(tokio::time::sleep_until(instant)),
        }
    }
}
