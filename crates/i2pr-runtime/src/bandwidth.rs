//! Shared, bounded byte-rate enforcement for runtime-owned transports.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use crate::CancellationToken;

/// Direction of bytes charged to the global governor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BandwidthDirection {
    /// Bytes received and admitted for protocol processing.
    Inbound,
    /// Bytes submitted to the socket, including framing and encryption.
    Outbound,
}

/// Opaque in-memory fairness key. Its contents are never formatted.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct BandwidthPeerKey([u8; 32]);

impl BandwidthPeerKey {
    /// Creates a key from a canonical router identity hash.
    pub const fn from_hash(hash: [u8; 32]) -> Self {
        Self(hash)
    }

    /// Creates a key from a caller-computed digest of an unauthenticated peer.
    pub const fn from_local_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }
}

impl fmt::Debug for BandwidthPeerKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BandwidthPeerKey(<redacted>)")
    }
}

/// Immutable limits for an enabled global shaper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BandwidthGovernorConfig {
    /// Inbound accepted-processing rate, bytes per second.
    pub inbound_bytes_per_second: u64,
    /// Outbound socket write rate, bytes per second.
    pub outbound_bytes_per_second: u64,
    /// Maximum bytes available as an instantaneous burst.
    pub burst_bytes: usize,
    /// Maximum queued reservation requests across the process.
    pub max_pending_requests: usize,
    /// Maximum queued reservation requests for one peer.
    pub max_pending_per_peer: usize,
}

/// Maximum supported burst size.
pub const MAX_BANDWIDTH_BURST_BYTES: usize = 4 * 1024 * 1024;
/// Maximum process-wide queued governor requests.
pub const MAX_BANDWIDTH_PENDING_REQUESTS: usize = 16_384;
/// Maximum admitted byte rate in either direction (10 GiB/s).
pub const MAX_BANDWIDTH_BYTES_PER_SECOND: u64 = 10 * 1024 * 1024 * 1024;

/// Invalid explicit governor limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BandwidthGovernorConfigError {
    /// Enabled governors need nonzero limits in both directions.
    ZeroRate,
    /// Configured rate exceeds the supported range.
    RateTooLarge,
    /// Burst is zero or exceeds the hard ceiling.
    InvalidBurst,
    /// Waiter queue bounds are inconsistent or exceed the hard ceiling.
    InvalidQueueLimit,
}

impl BandwidthGovernorConfig {
    /// Validates the limits before any service starts.
    pub fn validate(self) -> Result<Self, BandwidthGovernorConfigError> {
        if self.inbound_bytes_per_second == 0 || self.outbound_bytes_per_second == 0 {
            return Err(BandwidthGovernorConfigError::ZeroRate);
        }
        if self.inbound_bytes_per_second > MAX_BANDWIDTH_BYTES_PER_SECOND
            || self.outbound_bytes_per_second > MAX_BANDWIDTH_BYTES_PER_SECOND
        {
            return Err(BandwidthGovernorConfigError::RateTooLarge);
        }
        if self.burst_bytes == 0 || self.burst_bytes > MAX_BANDWIDTH_BURST_BYTES {
            return Err(BandwidthGovernorConfigError::InvalidBurst);
        }
        if self.max_pending_requests == 0
            || self.max_pending_requests > MAX_BANDWIDTH_PENDING_REQUESTS
            || self.max_pending_per_peer == 0
            || self.max_pending_per_peer > self.max_pending_requests
        {
            return Err(BandwidthGovernorConfigError::InvalidQueueLimit);
        }
        Ok(self)
    }
}

/// Why a reservation was not admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BandwidthAcquireError {
    /// Owner cancellation while waiting.
    Cancelled,
    /// Request is larger than the maximum burst.
    RequestTooLarge,
    /// Global waiter queue is full.
    QueueFull,
    /// This peer has reached its waiter limit.
    PeerQueueFull,
    /// Governor state was poisoned.
    StateUnavailable,
}

/// Aggregate privacy-safe governor counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BandwidthGovernorSnapshot {
    /// Current queued reservations.
    pub pending_requests: usize,
    /// Cumulative admitted inbound bytes.
    pub inbound_bytes: u64,
    /// Cumulative admitted outbound bytes.
    pub outbound_bytes: u64,
    /// Rejected requests due to a full waiter queue.
    pub queue_rejections: u64,
}

#[derive(Clone, Copy)]
struct Waiter {
    ticket: u64,
    direction: BandwidthDirection,
}

struct Bucket {
    rate: u64,
    capacity: u64,
    tokens: u64,
    fractional_token_nanos: u128,
    last_refill: Instant,
}

impl Bucket {
    fn new(rate: u64, capacity: u64, now: Instant) -> Self {
        Self {
            rate,
            capacity,
            tokens: capacity,
            fractional_token_nanos: 0,
            last_refill: now,
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.last_refill);
        let numerator = elapsed
            .as_nanos()
            .saturating_mul(u128::from(self.rate))
            .saturating_add(self.fractional_token_nanos);
        let added = numerator / 1_000_000_000;
        self.fractional_token_nanos = numerator % 1_000_000_000;
        let added = added.min(u128::from(u64::MAX)) as u64;
        self.tokens = self.tokens.saturating_add(added).min(self.capacity);
        if self.tokens == self.capacity {
            self.fractional_token_nanos = 0;
        }
        self.last_refill = now;
    }

    fn wait_for(&self, bytes: usize) -> Duration {
        let deficit = (bytes as u64).saturating_sub(self.tokens);
        if deficit == 0 {
            return Duration::ZERO;
        }
        let required = u128::from(deficit)
            .saturating_mul(1_000_000_000)
            .saturating_sub(self.fractional_token_nanos);
        let nanos = required
            .div_ceil(u128::from(self.rate))
            .min(u128::from(u64::MAX)) as u64;
        Duration::from_nanos(nanos)
    }
}

#[derive(Default)]
struct State {
    waiters: VecDeque<Waiter>,
    peer_pending: HashMap<BandwidthPeerKey, usize>,
    inbound_bytes: u64,
    outbound_bytes: u64,
    queue_rejections: u64,
}

struct Shared {
    config: BandwidthGovernorConfig,
    inbound: Mutex<Bucket>,
    outbound: Mutex<Bucket>,
    state: Mutex<State>,
    next_ticket: AtomicU64,
    notify: Notify,
}

/// Process-wide FIFO shaper shared by runtime-owned transport owners.
#[derive(Clone)]
pub struct BandwidthGovernor {
    shared: Arc<Shared>,
}

impl fmt::Debug for BandwidthGovernor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BandwidthGovernor")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

impl BandwidthGovernor {
    /// Creates one validated governor that can be cloned into transport owners.
    pub fn new(config: BandwidthGovernorConfig) -> Result<Self, BandwidthGovernorConfigError> {
        let config = config.validate()?;
        let now = Instant::now();
        Ok(Self {
            shared: Arc::new(Shared {
                config,
                inbound: Mutex::new(Bucket::new(
                    config.inbound_bytes_per_second,
                    config.burst_bytes as u64,
                    now,
                )),
                outbound: Mutex::new(Bucket::new(
                    config.outbound_bytes_per_second,
                    config.burst_bytes as u64,
                    now,
                )),
                state: Mutex::new(State::default()),
                next_ticket: AtomicU64::new(1),
                notify: Notify::new(),
            }),
        })
    }

    /// Returns the configured byte limits.
    pub fn config(&self) -> BandwidthGovernorConfig {
        self.shared.config
    }

    /// Waits for a FIFO reservation and charges exactly `bytes`.
    ///
    /// Call immediately before admitting inbound socket bytes to processing or
    /// writing outbound framed/encrypted bytes. The caller's cancellation token
    /// owns the wait lifecycle.
    pub async fn acquire(
        &self,
        direction: BandwidthDirection,
        peer: BandwidthPeerKey,
        bytes: usize,
        cancellation: &CancellationToken,
    ) -> Result<(), BandwidthAcquireError> {
        if bytes == 0 {
            return Ok(());
        }
        if bytes > self.shared.config.burst_bytes {
            return Err(BandwidthAcquireError::RequestTooLarge);
        }
        let ticket = self.shared.next_ticket.fetch_add(1, Ordering::Relaxed);
        {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| BandwidthAcquireError::StateUnavailable)?;
            if state.waiters.len() >= self.shared.config.max_pending_requests {
                state.queue_rejections = state.queue_rejections.saturating_add(1);
                return Err(BandwidthAcquireError::QueueFull);
            }
            let peer_count = state.peer_pending.get(&peer).copied().unwrap_or(0);
            if peer_count >= self.shared.config.max_pending_per_peer {
                state.queue_rejections = state.queue_rejections.saturating_add(1);
                return Err(BandwidthAcquireError::PeerQueueFull);
            }
            state.peer_pending.insert(peer, peer_count + 1);
            state.waiters.push_back(Waiter { ticket, direction });
        }
        let mut waiter = WaiterGuard {
            shared: Arc::clone(&self.shared),
            ticket,
            peer,
            armed: true,
        };

        loop {
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let wait = {
                let mut state = self
                    .shared
                    .state
                    .lock()
                    .map_err(|_| BandwidthAcquireError::StateUnavailable)?;
                // Preserve FIFO order independently per direction so a
                // depleted inbound bucket cannot stall available outbound
                // capacity (or the reverse).
                let Some(head) = state
                    .waiters
                    .iter()
                    .find(|waiter| waiter.direction == direction)
                    .copied()
                else {
                    return Err(BandwidthAcquireError::StateUnavailable);
                };
                if head.ticket != ticket {
                    None
                } else {
                    let bucket_mutex = match direction {
                        BandwidthDirection::Inbound => &self.shared.inbound,
                        BandwidthDirection::Outbound => &self.shared.outbound,
                    };
                    let mut bucket = bucket_mutex
                        .lock()
                        .map_err(|_| BandwidthAcquireError::StateUnavailable)?;
                    bucket.refill(Instant::now());
                    if bucket.tokens >= bytes as u64 {
                        bucket.tokens -= bytes as u64;
                        let Some(index) = state.waiters.iter().position(|row| row.ticket == ticket)
                        else {
                            return Err(BandwidthAcquireError::StateUnavailable);
                        };
                        state.waiters.remove(index);
                        state.remove_peer_pending(peer);
                        match direction {
                            BandwidthDirection::Inbound => {
                                state.inbound_bytes =
                                    state.inbound_bytes.saturating_add(bytes as u64)
                            }
                            BandwidthDirection::Outbound => {
                                state.outbound_bytes =
                                    state.outbound_bytes.saturating_add(bytes as u64)
                            }
                        }
                        waiter.armed = false;
                        drop(bucket);
                        self.shared.notify.notify_waiters();
                        return Ok(());
                    }
                    Some(bucket.wait_for(bytes))
                }
            };
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(BandwidthAcquireError::Cancelled),
                _ = async {
                    if let Some(duration) = wait { tokio::time::sleep(duration).await; }
                    else { notified.await; }
                } => {}
            }
        }
    }

    /// Returns aggregate counters without peer identifiers or endpoints.
    pub fn snapshot(&self) -> BandwidthGovernorSnapshot {
        self.shared
            .state
            .lock()
            .map(|state| BandwidthGovernorSnapshot {
                pending_requests: state.waiters.len(),
                inbound_bytes: state.inbound_bytes,
                outbound_bytes: state.outbound_bytes,
                queue_rejections: state.queue_rejections,
            })
            .unwrap_or_default()
    }
}

impl State {
    fn remove_peer_pending(&mut self, peer: BandwidthPeerKey) {
        if let Some(count) = self.peer_pending.get_mut(&peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.peer_pending.remove(&peer);
            }
        }
    }
}

struct WaiterGuard {
    shared: Arc<Shared>,
    ticket: u64,
    peer: BandwidthPeerKey,
    armed: bool,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut state) = self.shared.state.lock()
            && let Some(index) = state
                .waiters
                .iter()
                .position(|row| row.ticket == self.ticket)
        {
            state.waiters.remove(index);
            state.remove_peer_pending(self.peer);
        }
        self.shared.notify.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(value: u8) -> BandwidthPeerKey {
        BandwidthPeerKey::from_hash([value; 32])
    }

    fn governor() -> BandwidthGovernor {
        BandwidthGovernor::new(BandwidthGovernorConfig {
            inbound_bytes_per_second: 2,
            outbound_bytes_per_second: 2,
            burst_bytes: 2,
            max_pending_requests: 2,
            max_pending_per_peer: 1,
        })
        .expect("valid governor")
    }

    #[tokio::test(start_paused = true)]
    async fn bucket_preserves_fractional_refill_across_observations() {
        let start = Instant::now();
        let mut bucket = Bucket::new(2, 2, start);
        bucket.tokens = 0;
        bucket.refill(start + Duration::from_millis(250));
        assert_eq!(bucket.tokens, 0);
        assert_eq!(bucket.wait_for(1), Duration::from_millis(250));
        bucket.refill(start + Duration::from_millis(500));
        assert_eq!(bucket.tokens, 1);
        assert_eq!(bucket.fractional_token_nanos, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn global_rate_refills_fifo_and_bounds_each_peer() {
        let governor = governor();
        let cancellation = CancellationToken::new();
        governor
            .acquire(BandwidthDirection::Outbound, key(1), 2, &cancellation)
            .await
            .expect("initial burst");

        let waiter_governor = governor.clone();
        let waiter_cancel = cancellation.clone();
        let waiter = tokio::spawn(async move {
            waiter_governor
                .acquire(BandwidthDirection::Outbound, key(2), 2, &waiter_cancel)
                .await
        });
        tokio::task::yield_now().await;
        assert_eq!(governor.snapshot().pending_requests, 1);
        assert_eq!(
            governor
                .acquire(BandwidthDirection::Outbound, key(2), 1, &cancellation)
                .await,
            Err(BandwidthAcquireError::PeerQueueFull)
        );

        tokio::time::advance(Duration::from_millis(999)).await;
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        tokio::time::advance(Duration::from_millis(1)).await;
        assert_eq!(waiter.await.expect("waiter task"), Ok(()));
        assert_eq!(
            governor.snapshot(),
            BandwidthGovernorSnapshot {
                pending_requests: 0,
                inbound_bytes: 0,
                outbound_bytes: 4,
                queue_rejections: 1,
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_waiter_releases_its_queue_lease() {
        let governor = governor();
        let cancellation = CancellationToken::new();
        governor
            .acquire(BandwidthDirection::Inbound, key(1), 2, &cancellation)
            .await
            .expect("initial burst");
        let waiter_governor = governor.clone();
        let waiter_cancel = cancellation.clone();
        let waiter = tokio::spawn(async move {
            waiter_governor
                .acquire(BandwidthDirection::Inbound, key(2), 2, &waiter_cancel)
                .await
        });
        tokio::task::yield_now().await;
        assert_eq!(governor.snapshot().pending_requests, 1);
        cancellation.cancel(crate::CancellationReason::OperatorRequest);
        assert_eq!(
            waiter.await.expect("waiter task"),
            Err(BandwidthAcquireError::Cancelled)
        );
        assert_eq!(governor.snapshot().pending_requests, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn depleted_inbound_does_not_block_outbound_capacity() {
        let governor = governor();
        let cancellation = CancellationToken::new();
        governor
            .acquire(BandwidthDirection::Inbound, key(1), 2, &cancellation)
            .await
            .expect("consume inbound burst");
        let waiter_governor = governor.clone();
        let waiter_cancel = cancellation.clone();
        let waiter = tokio::spawn(async move {
            waiter_governor
                .acquire(BandwidthDirection::Inbound, key(2), 2, &waiter_cancel)
                .await
        });
        tokio::task::yield_now().await;
        governor
            .acquire(BandwidthDirection::Outbound, key(3), 2, &cancellation)
            .await
            .expect("independent outbound burst");
        assert_eq!(governor.snapshot().pending_requests, 1);
        cancellation.cancel(crate::CancellationReason::OperatorRequest);
        assert_eq!(
            waiter.await.expect("waiter task"),
            Err(BandwidthAcquireError::Cancelled)
        );
    }

    #[tokio::test]
    async fn rejects_requests_that_cannot_fit_the_burst() {
        let governor = governor();
        assert_eq!(
            governor
                .acquire(
                    BandwidthDirection::Inbound,
                    key(1),
                    3,
                    &CancellationToken::new(),
                )
                .await,
            Err(BandwidthAcquireError::RequestTooLarge)
        );
        assert_eq!(governor.snapshot().pending_requests, 0);
    }
}
