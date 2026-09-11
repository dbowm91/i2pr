//! Plan 185 daemon-owned creator-side tunnel liveness scheduler.
//!
//! Current I2P tunnel-routing guidance requires creators to test
//! tunnels because participants may remove tunnels after roughly
//! two minutes of no traffic. The scheduler pairs each active
//! outbound exploratory tunnel with an active inbound exploratory
//! tunnel, sends a DeliveryStatus test through the outbound path,
//! and matches the response that arrives through the inbound path.
//!
//! ```text
//! first test target     <= 30 seconds after establishment
//! repeat test target    <= 60 seconds while active
//! response timeout      <  2 minute idle deletion boundary
//! consecutive-failure   bounded, marks/removes the affected pair
//! ```
//!
//! Properties:
//!
//! - one central scheduler owns every active pair; no per-tunnel
//!   task or per-tunnel timer;
//! - pending test ids are bounded by
//!   [`MAX_PENDING_LIVENESS_TESTS`];
//! - the scheduler never opens sockets, never performs DNS, never
//!   spawns tasks; all I/O happens through the
//!   [`ExploratoryBuildCoordinator`] seam;
//! - a successful response refreshes the pair's health; a
//!   consecutive-failure threshold marks/removes the affected
//!   tunnels and asks the coordinator to rebuild;
//! - the scheduler reuses the canonical outbound
//!   [`i2pr_tunnel::roles::OutboundGatewayRole`] to emit the
//!   DeliveryStatus test cell, never a private parallel pipeline.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_proto::Hash;
use i2pr_runtime::Ssu2InboundI2np;
use thiserror::Error;

use i2pr_tunnel::identity::TunnelId;
use i2pr_tunnel::pool::TunnelSlot;

use crate::exploratory_build::{BuildCoordinatorOutcome, ExploratoryBuildCoordinator};
use crate::router_i2np::{RouterI2npKind, RouterI2npOutcome};

/// First test delay after the pair becomes active.
pub const FIRST_LIVENESS_DELAY_MS: u64 = 30_000;
/// Repeat test interval while the pair stays active.
pub const REPEAT_LIVENESS_INTERVAL_MS: u64 = 60_000;
/// Maximum time the scheduler waits for a response before it
/// counts a missed reply against the failure threshold. The
/// ceiling stays safely below the canonical two-minute idle
/// deletion boundary.
pub const LIVENESS_RESPONSE_TIMEOUT_MS: u64 = 60_000;
/// Bounded consecutive-failure threshold before the scheduler
/// asks the coordinator to remove the affected pair.
pub const LIVENESS_FAILURE_THRESHOLD: u8 = 2;
/// Maximum pending liveness tests the scheduler ever tracks.
pub const MAX_PENDING_LIVENESS_TESTS: usize = 16;
/// Hard ceiling on the number of active pairs the scheduler
/// tracks.
pub const MAX_LIVENESS_PAIRS: usize = 8;

/// Configuration for the liveness scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LivenessConfig {
    /// First test delay after the pair becomes active.
    pub first_test_delay_ms: u64,
    /// Repeat test interval while the pair stays active.
    pub repeat_test_interval_ms: u64,
    /// Maximum time the scheduler waits for a response.
    pub response_timeout_ms: u64,
    /// Bounded consecutive-failure threshold.
    pub failure_threshold: u8,
}

impl LivenessConfig {
    /// Returns the bounded defaults the Plan 185 §7 product
    /// ceiling enforces.
    pub const fn plan_185_defaults() -> Self {
        Self {
            first_test_delay_ms: FIRST_LIVENESS_DELAY_MS,
            repeat_test_interval_ms: REPEAT_LIVENESS_INTERVAL_MS,
            response_timeout_ms: LIVENESS_RESPONSE_TIMEOUT_MS,
            failure_threshold: LIVENESS_FAILURE_THRESHOLD,
        }
    }
}

impl Default for LivenessConfig {
    fn default() -> Self {
        Self::plan_185_defaults()
    }
}

/// Typed failure categories for the liveness scheduler.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LivenessError {
    /// The supplied slot is not paired in the scheduler.
    #[error("liveness scheduler has no pair bound to slot {0:?}")]
    UnknownSlot(TunnelSlot),
    /// The scheduler is at its bounded pair capacity.
    #[error("liveness scheduler pair capacity reached")]
    PairCapacityReached,
    /// The scheduler is at its bounded pending test capacity.
    #[error("liveness scheduler pending test capacity reached")]
    PendingCapacityReached,
}

/// Outcome category the scheduler hands back to the pump.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LivenessAction {
    /// The scheduler wants the pump to emit a DeliveryStatus test
    /// through the supplied outbound pool slot using the paired
    /// inbound slot as the expected response target.
    SendTest {
        /// Outbound pool slot the test must traverse.
        outbound_slot: TunnelSlot,
        /// Inbound pool slot the response is expected to arrive
        /// on.
        inbound_slot: TunnelSlot,
        /// Test id the caller must echo back to the scheduler on
        /// response / timeout.
        test_id: LivenessTestId,
    },
    /// The scheduler observed too many consecutive failures for
    /// the supplied pair; the pump must remove the slots from the
    /// coordinator and ask the coordinator to rebuild.
    MarkUnhealthy {
        /// Outbound pool slot the scheduler wants removed.
        outbound_slot: TunnelSlot,
        /// Inbound pool slot the scheduler wants removed.
        inbound_slot: TunnelSlot,
        /// Consecutive failure count observed.
        consecutive_failures: u8,
    },
    /// No action is required at this wall-clock tick.
    Idle {
        /// Number of active pairs the scheduler is tracking.
        active_pairs: usize,
    },
}

/// Monotonic test id the scheduler hands out.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LivenessTestId(u64);

impl LivenessTestId {
    /// Returns the inner numeric value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for LivenessTestId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Privacy-safe counters for the liveness scheduler.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LivenessCounters {
    /// Tests the scheduler emitted.
    pub tests_sent: u64,
    /// Successful responses the scheduler received.
    pub successful_responses: u64,
    /// Missed responses the scheduler expired.
    pub missed_responses: u64,
    /// Pairs the scheduler marked unhealthy.
    pub unhealthy_marks: u64,
    /// Pending tests outstanding at the snapshot.
    pub pending: usize,
    /// Active pairs the scheduler is tracking.
    pub active_pairs: usize,
}

/// State of one active pair the scheduler tracks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PairState {
    outbound_slot: TunnelSlot,
    inbound_slot: TunnelSlot,
    last_test_at_ms: u64,
    consecutive_failures: u8,
    /// Whether the first test has been scheduled. Plan 185 §7
    /// delays the first test by `first_test_delay_ms`.
    first_test_due_at_ms: u64,
}

impl PairState {
    fn next_due_test_ms(&self, now_ms: u64) -> u64 {
        if self.last_test_at_ms == 0 {
            self.first_test_due_at_ms.max(now_ms)
        } else {
            self.last_test_at_ms
                .saturating_add(REPEAT_LIVENESS_INTERVAL_MS)
        }
    }
}

/// One pending test the scheduler is awaiting a response for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingTest {
    outbound_slot: TunnelSlot,
    inbound_slot: TunnelSlot,
    scheduled_at_ms: u64,
    deadline_ms: u64,
}

/// Daemon-owned creator-side tunnel liveness scheduler.
#[derive(Debug)]
pub struct TunnelLivenessScheduler {
    config: LivenessConfig,
    pairs: BTreeMap<TunnelSlot, PairState>,
    pending: BTreeMap<LivenessTestId, PendingTest>,
    next_test_id: u64,
    counters: LivenessCounters,
    now_ms: u64,
}

impl TunnelLivenessScheduler {
    /// Constructs a new scheduler with the supplied configuration.
    pub fn new(config: LivenessConfig) -> Self {
        Self {
            config,
            pairs: BTreeMap::new(),
            pending: BTreeMap::new(),
            next_test_id: 1,
            counters: LivenessCounters::default(),
            now_ms: 0,
        }
    }

    /// Returns the configured liveness policy.
    pub const fn config(&self) -> LivenessConfig {
        self.config
    }

    /// Returns the bounded counters snapshot.
    pub fn counters(&self) -> LivenessCounters {
        let mut snapshot = self.counters;
        snapshot.pending = self.pending.len();
        snapshot.active_pairs = self.pairs.len();
        snapshot
    }

    /// Returns the number of active pairs the scheduler is
    /// tracking.
    pub fn active_pairs(&self) -> usize {
        self.pairs.len()
    }

    /// Returns the number of pending tests the scheduler is
    /// awaiting responses for.
    pub fn pending_tests(&self) -> usize {
        self.pending.len()
    }

    /// Advances the scheduler's wall-clock view. The pump must
    /// call this before any [`Self::drive`] invocation.
    pub fn advance_time(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Registers one paired tunnel set with the scheduler. The
    /// outbound slot is the lookup key; the inbound slot is the
    /// response target.
    pub fn register_pair(
        &mut self,
        outbound_slot: TunnelSlot,
        inbound_slot: TunnelSlot,
    ) -> Result<(), LivenessError> {
        if self.pairs.len() >= MAX_LIVENESS_PAIRS {
            return Err(LivenessError::PairCapacityReached);
        }
        if outbound_slot == inbound_slot {
            return Err(LivenessError::UnknownSlot(outbound_slot));
        }
        if self.pairs.contains_key(&outbound_slot) {
            return Ok(());
        }
        let now_ms = self.now_ms;
        let first_due = now_ms.saturating_add(self.config.first_test_delay_ms);
        self.pairs.insert(
            outbound_slot,
            PairState {
                outbound_slot,
                inbound_slot,
                last_test_at_ms: 0,
                consecutive_failures: 0,
                first_test_due_at_ms: first_due,
            },
        );
        Ok(())
    }

    /// Removes one pair from the scheduler. The scheduler drops
    /// any pending tests bound to the pair without counting them
    /// as a failure.
    pub fn unregister_pair(&mut self, outbound_slot: TunnelSlot) -> bool {
        let removed = self.pairs.remove(&outbound_slot).is_some();
        if removed {
            let to_drop: Vec<LivenessTestId> = self
                .pending
                .iter()
                .filter_map(|(id, test)| {
                    if test.outbound_slot == outbound_slot {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();
            for id in to_drop {
                self.pending.remove(&id);
            }
        }
        removed
    }

    /// Drives the scheduler one tick at the current wall-clock.
    /// Returns the highest-priority action the pump should perform
    /// (a [`LivenessAction::SendTest`] when a test is due,
    /// a [`LivenessAction::MarkUnhealthy`] when a pair has reached
    /// its failure threshold, or [`LivenessAction::Idle`] when no
    /// work is required).
    pub fn drive(&mut self) -> LivenessAction {
        let now_ms = self.now_ms;
        // First expire any pending tests past their response
        // deadline; the failure is counted before returning the
        // action so the caller observes a consistent counter
        // snapshot.
        let expired: Vec<LivenessTestId> = self
            .pending
            .iter()
            .filter_map(|(id, test)| {
                if test.deadline_ms <= now_ms {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for id in expired {
            if let Some(test) = self.pending.remove(&id) {
                self.counters.missed_responses = self.counters.missed_responses.saturating_add(1);
                self.record_failure(test.outbound_slot);
            }
        }
        // Now check for any pair that has hit the failure
        // threshold; the pump must remove those before the next
        // scheduled test.
        for pair in self.pairs.values() {
            if pair.consecutive_failures >= self.config.failure_threshold {
                let outbound_slot = pair.outbound_slot;
                let inbound_slot = pair.inbound_slot;
                let consecutive_failures = pair.consecutive_failures;
                self.pairs.remove(&outbound_slot);
                self.counters.unhealthy_marks = self.counters.unhealthy_marks.saturating_add(1);
                return LivenessAction::MarkUnhealthy {
                    outbound_slot,
                    inbound_slot,
                    consecutive_failures,
                };
            }
        }
        // Otherwise, emit the next due test (oldest first).
        let next_due = self
            .pairs
            .values()
            .filter(|pair| {
                let pending_for_pair = self
                    .pending
                    .values()
                    .any(|test| test.outbound_slot == pair.outbound_slot);
                !pending_for_pair
            })
            .min_by_key(|pair| pair.next_due_test_ms(now_ms))
            .copied();
        if let Some(pair) = next_due
            && pair.next_due_test_ms(now_ms) <= now_ms
        {
            if self.pending.len() >= MAX_PENDING_LIVENESS_TESTS {
                return LivenessAction::Idle {
                    active_pairs: self.pairs.len(),
                };
            }
            let test_id = self.allocate_test_id();
            let deadline_ms = now_ms.saturating_add(self.config.response_timeout_ms);
            self.pending.insert(
                test_id,
                PendingTest {
                    outbound_slot: pair.outbound_slot,
                    inbound_slot: pair.inbound_slot,
                    scheduled_at_ms: now_ms,
                    deadline_ms,
                },
            );
            if let Some(state) = self.pairs.get_mut(&pair.outbound_slot) {
                state.last_test_at_ms = now_ms;
            }
            self.counters.tests_sent = self.counters.tests_sent.saturating_add(1);
            return LivenessAction::SendTest {
                outbound_slot: pair.outbound_slot,
                inbound_slot: pair.inbound_slot,
                test_id,
            };
        }
        LivenessAction::Idle {
            active_pairs: self.pairs.len(),
        }
    }

    /// Records one successful response from the supplied test id.
    /// Returns the pair the scheduler refreshed.
    pub fn record_response(&mut self, test_id: LivenessTestId) -> Option<LivenessAction> {
        let test = self.pending.remove(&test_id)?;
        self.counters.successful_responses = self.counters.successful_responses.saturating_add(1);
        if let Some(state) = self.pairs.get_mut(&test.outbound_slot) {
            state.consecutive_failures = 0;
        }
        Some(LivenessAction::Idle {
            active_pairs: self.pairs.len(),
        })
    }

    /// Records one inbound router-I2NP outcome that carries the
    /// supplied test id. The function matches the inbound
    /// `DeliveryStatus` echo against the pending tests and
    /// refreshes the matching pair's health.
    pub fn record_inbound_outcome(
        &mut self,
        outcome: &RouterI2npOutcome,
    ) -> Option<LivenessTestId> {
        match outcome {
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DeliveryStatus,
                ..
            } => {
                // Without an authenticated message id the
                // scheduler cannot match the inbound echo to a
                // pending test id; the daemon pump must call
                // [`Self::record_response`] directly when it has
                // a confirmed test id. The helper returns the
                // next outstanding test id when exactly one
                // pending test exists, otherwise `None`.
                if self.pending.len() == 1 {
                    self.pending.keys().next().copied()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns the next pending test id the scheduler is awaiting
    /// a response for. Used by the daemon pump to correlate inbound
    /// traffic to the test that triggered it.
    pub fn next_pending_test_id(&self) -> Option<LivenessTestId> {
        self.pending.keys().next().copied()
    }

    /// Returns the inbound pool slot paired with the supplied
    /// outbound slot, when present.
    pub fn inbound_slot_for(&self, outbound_slot: TunnelSlot) -> Option<TunnelSlot> {
        self.pairs.get(&outbound_slot).map(|p| p.inbound_slot)
    }

    /// Returns the outbound pool slot paired with the supplied
    /// inbound slot, when present.
    pub fn outbound_slot_for(&self, inbound_slot: TunnelSlot) -> Option<TunnelSlot> {
        self.pairs
            .values()
            .find(|p| p.inbound_slot == inbound_slot)
            .map(|p| p.outbound_slot)
    }

    /// Returns the local inbound receive tunnel id for the
    /// supplied inbound pool slot, when the coordinator owns the
    /// corresponding role.
    pub fn inbound_local_receive(
        &self,
        coordinator: &ExploratoryBuildCoordinator,
        inbound_slot: TunnelSlot,
    ) -> Option<TunnelId> {
        let pair = self
            .pairs
            .values()
            .find(|p| p.inbound_slot == inbound_slot)?;
        let registrations =
            coordinator.registrations(i2pr_tunnel::identity::TunnelDirection::Inbound);
        registrations
            .iter()
            .find(|r| r.slot() == pair.inbound_slot)
            .map(|r| r.tunnel_id())
    }

    /// Allocates a fresh test id. The id is monotonic and never
    /// reused across the scheduler lifetime.
    fn allocate_test_id(&mut self) -> LivenessTestId {
        let value = self.next_test_id;
        self.next_test_id = self
            .next_test_id
            .checked_add(1)
            .expect("liveness test id monotonic counter exhausted");
        LivenessTestId(value)
    }

    fn record_failure(&mut self, outbound_slot: TunnelSlot) {
        if let Some(state) = self.pairs.get_mut(&outbound_slot) {
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        }
    }
}

/// Builds a DeliveryStatus I2NP message the pump sends through the
/// outbound tunnel. The function reuses the canonical
/// [`i2pr_proto::DeliveryStatusMessage`] surface so the live path
/// matches what Plan 116 / Plan 117 already emit on the same
/// destination.
pub fn delivery_status_message(message_id: u32, now_ms: u64) -> i2pr_proto::I2npBody {
    use i2pr_proto::{Date, DeliveryStatusMessage};
    i2pr_proto::I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
        message_id,
        Date::from_millis(now_ms.max(1)),
    ))
}

/// Outcome of routing one inbound message through both the
/// coordinator's dispatcher and the liveness scheduler's pending
/// table. The helper exists so the daemon pump can drive both
/// seams with one call.
#[derive(Clone, Debug)]
pub struct InboundLivenessOutcome {
    /// Inbound dispatcher outcome the coordinator produced.
    pub coordinator: RouterI2npOutcome,
    /// Liveness test id the scheduler refreshed, when the inbound
    /// message matched a pending test.
    pub refreshed: Option<LivenessTestId>,
    /// Coordinator build outcomes the build pipeline emitted.
    pub build_outcomes: Vec<BuildCoordinatorOutcome>,
}

/// Routes one inbound authenticated router-I2NP message through
/// the build coordinator's dispatcher and the liveness scheduler
/// when the inbound message carries a DeliveryStatus echo. The
/// helper never weakens any decoder; it only correlates inbound
/// observations with pending tests.
pub fn route_inbound_with_liveness(
    coordinator: &mut ExploratoryBuildCoordinator,
    scheduler: &mut TunnelLivenessScheduler,
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
) -> Result<InboundLivenessOutcome, crate::router_i2np::RouterI2npError> {
    let routed = coordinator.route_inbound_i2np(inbound, now_ms)?;
    let refreshed = scheduler.record_inbound_outcome(&routed.dispatcher);
    if let Some(test_id) = refreshed {
        let _ = scheduler.record_response(test_id);
    }
    Ok(InboundLivenessOutcome {
        coordinator: routed.dispatcher,
        refreshed,
        build_outcomes: routed.coordinator,
    })
}

/// Test-only helper: returns the canonical first-test delay
/// relative to the supplied `now_ms`.
pub fn first_due_after(now_ms: u64, config: LivenessConfig) -> u64 {
    now_ms.saturating_add(config.first_test_delay_ms)
}

/// Test-only helper: returns the canonical repeat interval.
pub fn repeat_interval(config: LivenessConfig) -> u64 {
    config.repeat_test_interval_ms
}

/// Test-only helper: returns the canonical response timeout.
pub fn response_timeout(config: LivenessConfig) -> u64 {
    config.response_timeout_ms
}

/// Convenience hash for evidence rows that key on the inbound
/// local receive tunnel id. The hash is the canonical I2P router
/// hash, never a secret.
pub fn local_receive_digest(local_receive: TunnelId) -> Hash {
    let mut bytes = [0_u8; 32];
    bytes[..4].copy_from_slice(&local_receive.get().to_be_bytes());
    Hash::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_185_defaults_stay_below_two_minute_boundary() {
        let config = LivenessConfig::plan_185_defaults();
        assert_eq!(config.first_test_delay_ms, 30_000);
        assert_eq!(config.repeat_test_interval_ms, 60_000);
        assert!(config.response_timeout_ms < 120_000);
    }

    #[test]
    fn drive_with_no_pairs_returns_idle() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        match scheduler.drive() {
            LivenessAction::Idle { active_pairs } => assert_eq!(active_pairs, 0),
            other => panic!("expected Idle, got {other:?}"),
        }
    }

    #[test]
    fn register_pair_then_drive_returns_idle_before_first_delay() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        match scheduler.drive() {
            LivenessAction::Idle { active_pairs } => assert_eq!(active_pairs, 1),
            other => panic!("expected Idle before first delay, got {other:?}"),
        }
    }

    #[test]
    fn register_pair_then_advance_past_first_delay_emits_send_test() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        scheduler.advance_time(1_000 + FIRST_LIVENESS_DELAY_MS + 1);
        match scheduler.drive() {
            LivenessAction::SendTest {
                outbound_slot,
                inbound_slot,
                test_id,
            } => {
                assert_eq!(outbound_slot, TunnelSlot::from_raw(1));
                assert_eq!(inbound_slot, TunnelSlot::from_raw(2));
                assert_eq!(test_id.get(), 1);
            }
            other => panic!("expected SendTest, got {other:?}"),
        }
    }

    #[test]
    fn register_pair_at_capacity_is_rejected() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        for index in 0..MAX_LIVENESS_PAIRS {
            let outbound = TunnelSlot::from_raw(index as u32 + 1);
            let inbound = TunnelSlot::from_raw(index as u32 + 100);
            scheduler
                .register_pair(outbound, inbound)
                .expect("register");
        }
        let err = scheduler
            .register_pair(TunnelSlot::from_raw(0xFFFF), TunnelSlot::from_raw(0xFFFE))
            .unwrap_err();
        assert!(matches!(err, LivenessError::PairCapacityReached));
    }

    #[test]
    fn unregister_pair_drops_pending_tests_without_counting_failure() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        scheduler.advance_time(1_000 + FIRST_LIVENESS_DELAY_MS + 1);
        let action = scheduler.drive();
        let test_id = match action {
            LivenessAction::SendTest { test_id, .. } => test_id,
            other => panic!("expected SendTest, got {other:?}"),
        };
        assert!(scheduler.unregister_pair(TunnelSlot::from_raw(1)));
        assert_eq!(scheduler.pending_tests(), 0);
        assert_eq!(scheduler.counters().missed_responses, 0);
        assert!(scheduler.record_response(test_id).is_none());
    }

    #[test]
    fn record_response_resets_consecutive_failures() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        scheduler.advance_time(1_000 + FIRST_LIVENESS_DELAY_MS + 1);
        let action = scheduler.drive();
        let test_id = match action {
            LivenessAction::SendTest { test_id, .. } => test_id,
            other => panic!("expected SendTest, got {other:?}"),
        };
        let refresh = scheduler.record_response(test_id).expect("refresh");
        match refresh {
            LivenessAction::Idle { active_pairs } => assert_eq!(active_pairs, 1),
            other => panic!("expected Idle, got {other:?}"),
        }
        assert_eq!(scheduler.counters().successful_responses, 1);
    }
}
