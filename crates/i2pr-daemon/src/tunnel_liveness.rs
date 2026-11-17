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
//!
//! Plan 504 additions:
//!
//! - the probe carries a scheduler-allocated identifier in the
//!   `DeliveryStatus` body, and the dispatcher carries the
//!   acknowledged identifier back out, so a response is attributed to
//!   one specific pending test instead of to "the only pending test";
//! - a pair becomes *proven* only when it has produced such an
//!   authenticated response, and only a proven pair may ever be
//!   marked unhealthy: a pair that has never answered must not be
//!   torn down on the strength of a signal the router cannot
//!   distinguish from its own negation.

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
/// Plan 504 — the first identifier the scheduler hands to a probe.
///
/// The identifier is the value the remote echoes in the
/// `DeliveryStatus` body, so it shares its space with every other
/// `DeliveryStatus` correlation token on the router. The NetDB
/// publication reply tokens are a monotonic counter that starts at
/// one, so the liveness probes take the *high* half of the `u32`
/// range and the two spaces cannot overlap.
pub const LIVENESS_PROBE_MESSAGE_ID_BASE: u32 = 0x8000_0000;
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
        /// Identifier the probe carries in the `DeliveryStatus`
        /// body. The remote echoes it back, so the scheduler can
        /// attribute the response to this exact test instead of
        /// guessing. Plan 504.
        probe_message_id: u32,
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
    /// Pairs that have never produced an authenticated response.
    ///
    /// Plan 504 invariant 9: such a pair is never torn down, because
    /// a missing response is indistinguishable from "this peer does
    /// not answer".
    pub unproven_pairs: usize,
    /// Pairs that reached the failure threshold while still
    /// unproven, so the teardown was withheld.
    ///
    /// Plan 504: the count is the operator-visible signal that the
    /// liveness probe is not being answered at all.
    pub unproven_failures: u64,
    /// Responses that carried no identifier matching a pending
    /// probe.
    ///
    /// Plan 504: such a response is not evidence and never refreshes
    /// a pair; the count makes the unauthenticated correlation
    /// visible instead of silent.
    pub unauthenticated_responses: u64,
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
    /// Whether the pair has ever produced an *authenticated*
    /// response.
    ///
    /// Plan 504 invariant 9: only an authenticated response proves
    /// the pair can answer, so only a proven pair may be marked
    /// unhealthy. An unproven pair is never torn down.
    proven: bool,
    /// Whether the withheld teardown for this unproven pair has
    /// already been counted, so the count is reported once per pair
    /// rather than once per tick.
    unproven_reported: bool,
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
    /// Identifier the probe carried in its `DeliveryStatus` body.
    ///
    /// Plan 504: the remote echoes this value back, so it is the
    /// only authenticated way to attribute a response to a test.
    probe_message_id: u32,
}

/// Daemon-owned creator-side tunnel liveness scheduler.
#[derive(Debug)]
pub struct TunnelLivenessScheduler {
    config: LivenessConfig,
    pairs: BTreeMap<TunnelSlot, PairState>,
    pending: BTreeMap<LivenessTestId, PendingTest>,
    next_test_id: u64,
    next_probe_message_id: u32,
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
            next_probe_message_id: LIVENESS_PROBE_MESSAGE_ID_BASE,
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
        snapshot.unproven_pairs = self.pairs.values().filter(|pair| !pair.proven).count();
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

    /// Returns the outbound slots of the pairs the scheduler is
    /// tracking.
    ///
    /// Plan 504: the pump uses this to unregister the pairs the
    /// coordinator no longer installs.
    pub fn tracked_outbound_slots(&self) -> Vec<TunnelSlot> {
        self.pairs.keys().copied().collect()
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
                proven: false,
                unproven_reported: false,
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

    /// Drops one pending test without counting a failure.
    ///
    /// Plan 504: the pump uses this when the probe could not be
    /// emitted at all — a local composition fault, or a pair that went
    /// away between the scheduler's decision and the emission. The
    /// peer never saw the probe, so the peer is not at fault and the
    /// test must not count against the pair's failure threshold.
    pub fn abandon_test(&mut self, test_id: LivenessTestId) -> bool {
        self.pending.remove(&test_id).is_some()
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
        //
        // Plan 504 invariant 9: only a *proven* pair may be marked
        // unhealthy. A pair that has never produced an
        // authenticated response must never be torn down, because a
        // missing response is indistinguishable from "this peer does
        // not answer" — acting on it would remove a healthy pair the
        // router cannot prove is broken. The withheld teardown is
        // counted instead, so an operator can see the probe is not
        // being answered.
        for pair in self.pairs.values() {
            if pair.proven && pair.consecutive_failures >= self.config.failure_threshold {
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
        // Count the withheld teardown once per unproven pair.
        let mut withheld = 0_u64;
        for pair in self.pairs.values_mut() {
            if !pair.proven
                && !pair.unproven_reported
                && pair.consecutive_failures >= self.config.failure_threshold
            {
                pair.unproven_reported = true;
                withheld = withheld.saturating_add(1);
            }
        }
        self.counters.unproven_failures = self.counters.unproven_failures.saturating_add(withheld);
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
            let Some(probe_message_id) = self.allocate_probe_message_id() else {
                return LivenessAction::Idle {
                    active_pairs: self.pairs.len(),
                };
            };
            let test_id = self.allocate_test_id();
            let deadline_ms = now_ms.saturating_add(self.config.response_timeout_ms);
            self.pending.insert(
                test_id,
                PendingTest {
                    outbound_slot: pair.outbound_slot,
                    inbound_slot: pair.inbound_slot,
                    scheduled_at_ms: now_ms,
                    deadline_ms,
                    probe_message_id,
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
                probe_message_id,
            };
        }
        LivenessAction::Idle {
            active_pairs: self.pairs.len(),
        }
    }

    /// Records one successful response from the supplied test id.
    /// Returns the pair the scheduler refreshed.
    ///
    /// The function does **not** mark the pair proven: only an
    /// authenticated response ([`Self::record_authenticated_response`])
    /// proves that the pair can answer, and a proven pair is the only
    /// pair that may be marked unhealthy.
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

    /// Records one inbound response whose body names the message it
    /// acknowledges.
    ///
    /// Plan 504: the acknowledged identifier is the only
    /// authenticated way to attribute a response to the probe that
    /// caused it. The function returns the test whose probe carried
    /// that identifier and nothing else.
    ///
    /// A response whose identifier matches no pending probe is **not**
    /// evidence: it refreshes no pair and marks no pair proven. It
    /// is counted in [`LivenessCounters::unauthenticated_responses`]
    /// so the unauthenticated correlation is visible instead of
    /// silent.
    pub fn record_authenticated_response(
        &mut self,
        acknowledged_message_id: u32,
    ) -> Option<LivenessTestId> {
        let matched = self
            .pending
            .iter()
            .find(|(_, test)| test.probe_message_id == acknowledged_message_id)
            .map(|(id, _)| *id);
        let Some(test_id) = matched else {
            self.counters.unauthenticated_responses =
                self.counters.unauthenticated_responses.saturating_add(1);
            return None;
        };
        let test = self.pending.remove(&test_id)?;
        self.counters.successful_responses = self.counters.successful_responses.saturating_add(1);
        if let Some(state) = self.pairs.get_mut(&test.outbound_slot) {
            state.consecutive_failures = 0;
            // Plan 504 invariant 9: an authenticated response is the
            // only proof that the pair answers, so only now may the
            // pair be torn down if it later stops answering.
            state.proven = true;
        }
        Some(test_id)
    }

    /// Records one inbound router-I2NP outcome that carries the
    /// supplied test id. The function matches the inbound
    /// `DeliveryStatus` echo against the pending tests and
    /// refreshes the matching pair's health.
    ///
    /// Plan 504: this helper is the *fallback* correlation and it is
    /// unauthenticated — it matches any `DeliveryStatus` when exactly
    /// one test is pending, because the outcome could not name the
    /// message it acknowledged. It stays as it is for the lanes that
    /// use it; the live product path uses
    /// [`Self::record_authenticated_response`], which matches the
    /// acknowledged identifier the dispatcher now carries.
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

    /// Allocates the identifier the next probe carries in its
    /// `DeliveryStatus` body.
    ///
    /// Plan 504: the identifier must be nonzero (zero is not a
    /// usable I2NP message id) and must not collide with a probe
    /// that is still pending, otherwise a response could not be
    /// attributed to one specific test. The scan is bounded by the
    /// pending-table ceiling, so the loop always terminates.
    fn allocate_probe_message_id(&mut self) -> Option<u32> {
        let mut candidate = self.next_probe_message_id;
        for _ in 0..=MAX_PENDING_LIVENESS_TESTS {
            if candidate >= LIVENESS_PROBE_MESSAGE_ID_BASE
                && !self
                    .pending
                    .values()
                    .any(|test| test.probe_message_id == candidate)
            {
                self.next_probe_message_id = if candidate == u32::MAX {
                    LIVENESS_PROBE_MESSAGE_ID_BASE
                } else {
                    candidate + 1
                };
                return Some(candidate);
            }
            candidate = if candidate == u32::MAX {
                LIVENESS_PROBE_MESSAGE_ID_BASE
            } else {
                candidate + 1
            };
        }
        None
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
///
/// The field names mirror
/// [`crate::exploratory_build::InboundRouteOutcome`] exactly, so a
/// caller that already consumed the coordinator's routed outcome
/// consumes this one the same way.
#[derive(Clone, Debug)]
pub struct InboundLivenessOutcome {
    /// Inbound dispatcher outcome the coordinator produced.
    pub dispatcher: RouterI2npOutcome,
    /// Liveness test id the scheduler refreshed, when the inbound
    /// message matched a pending test.
    pub refreshed: Option<LivenessTestId>,
    /// Coordinator build outcomes the build pipeline emitted.
    pub coordinator: Vec<BuildCoordinatorOutcome>,
}

/// Routes one inbound authenticated router-I2NP message through
/// the build coordinator's dispatcher and the liveness scheduler
/// when the inbound message carries a DeliveryStatus echo. The
/// helper never weakens any decoder; it only correlates inbound
/// observations with pending tests.
///
/// Plan 504: the correlation is *authenticated*. The inbound
/// `DeliveryStatus` body names the message it acknowledges, and only
/// a response whose acknowledged identifier matches a pending probe
/// refreshes a pair. A response carrying no pending identifier is
/// not evidence and is counted as
/// [`LivenessCounters::unauthenticated_responses`].
pub fn route_inbound_with_liveness(
    coordinator: &mut ExploratoryBuildCoordinator,
    scheduler: &mut TunnelLivenessScheduler,
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
) -> Result<InboundLivenessOutcome, crate::router_i2np::RouterI2npError> {
    let routed = coordinator.route_inbound_i2np(inbound, now_ms)?;
    let refreshed = record_authenticated_outcome(scheduler, &routed.dispatcher);
    Ok(InboundLivenessOutcome {
        dispatcher: routed.dispatcher,
        refreshed,
        coordinator: routed.coordinator,
    })
}

/// Routes one inbound authenticated router-I2NP message whose
/// dispatcher outcome the caller already decoded.
///
/// Plan 504: the live pump decodes the inbound message once and
/// classifies it once; this variant correlates that *same* outcome
/// with the liveness scheduler without running a second decoder. It
/// is the same seam as [`route_inbound_with_liveness`], never a
/// parallel inbound path.
pub fn route_inbound_with_liveness_classified(
    coordinator: &mut ExploratoryBuildCoordinator,
    scheduler: &mut TunnelLivenessScheduler,
    inbound: &Ssu2InboundI2np,
    outcome: RouterI2npOutcome,
) -> InboundLivenessOutcome {
    let routed = coordinator.route_inbound_i2np_classified(inbound, outcome);
    let refreshed = record_authenticated_outcome(scheduler, &routed.dispatcher);
    InboundLivenessOutcome {
        dispatcher: routed.dispatcher,
        refreshed,
        coordinator: routed.coordinator,
    }
}

/// Correlates one dispatcher outcome with the scheduler's pending
/// tests using the acknowledged-message identifier the outcome
/// carries.
fn record_authenticated_outcome(
    scheduler: &mut TunnelLivenessScheduler,
    outcome: &RouterI2npOutcome,
) -> Option<LivenessTestId> {
    let acknowledged = outcome.acknowledged_message_id()?;
    scheduler.record_authenticated_response(acknowledged)
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
                probe_message_id,
            } => {
                assert_eq!(outbound_slot, TunnelSlot::from_raw(1));
                assert_eq!(inbound_slot, TunnelSlot::from_raw(2));
                assert_eq!(test_id.get(), 1);
                // Plan 504: the probe identifier lives in the liveness
                // half of the identifier space, so it can never
                // collide with a NetDB publication reply token.
                assert_eq!(probe_message_id, LIVENESS_PROBE_MESSAGE_ID_BASE);
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

    /// Plan 504 — an authenticated response is attributed to the
    /// probe that carried the acknowledged identifier, and it proves
    /// the pair.
    #[test]
    fn authenticated_response_matches_the_probe_that_carried_the_identifier() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        scheduler.advance_time(1_000 + FIRST_LIVENESS_DELAY_MS + 1);
        let (test_id, probe_message_id) = match scheduler.drive() {
            LivenessAction::SendTest {
                test_id,
                probe_message_id,
                ..
            } => (test_id, probe_message_id),
            other => panic!("expected SendTest, got {other:?}"),
        };
        assert_ne!(probe_message_id, 0, "the probe identifier is nonzero");
        assert!(
            probe_message_id >= LIVENESS_PROBE_MESSAGE_ID_BASE,
            "the probe identifier lives in the liveness half of the identifier space"
        );
        // A response that carries no pending identifier is not
        // evidence and refreshes nothing.
        assert_eq!(
            scheduler.record_authenticated_response(probe_message_id ^ 0xFFFF),
            None
        );
        assert_eq!(scheduler.counters().successful_responses, 0);
        assert_eq!(scheduler.counters().unauthenticated_responses, 1);
        // The matching identifier is credited to its own test.
        assert_eq!(
            scheduler.record_authenticated_response(probe_message_id),
            Some(test_id)
        );
        assert_eq!(scheduler.counters().successful_responses, 1);
        assert_eq!(scheduler.counters().unproven_pairs, 0);
    }

    /// Plan 504 invariant 9 — a pair that has never produced an
    /// authenticated response is never torn down, and the withheld
    /// teardown is counted instead.
    #[test]
    fn unproven_pair_is_never_marked_unhealthy() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        // Two consecutive timeouts on a pair that never answered.
        let base = 1_000 + FIRST_LIVENESS_DELAY_MS + 1;
        scheduler.advance_time(base);
        assert!(
            matches!(scheduler.drive(), LivenessAction::SendTest { .. }),
            "the scheduler probes the pair"
        );
        // The first probe times out: the miss is counted, but the
        // pair is still unproven, so no teardown is emitted and the
        // scheduler keeps probing it.
        scheduler.advance_time(base + LIVENESS_RESPONSE_TIMEOUT_MS + 1);
        match scheduler.drive() {
            LivenessAction::SendTest { .. } => {}
            other => panic!("an unproven pair is never marked unhealthy: {other:?}"),
        }
        // The second probe times out: the threshold is reached, but
        // the pair is still unproven, so the teardown is withheld and
        // counted instead.
        scheduler.advance_time(base + 2 * (LIVENESS_RESPONSE_TIMEOUT_MS + 1));
        match scheduler.drive() {
            LivenessAction::SendTest { .. } => {}
            other => panic!("an unproven pair is never marked unhealthy: {other:?}"),
        }
        let counters = scheduler.counters();
        assert_eq!(
            counters.unhealthy_marks, 0,
            "an unproven pair is never torn down"
        );
        assert_eq!(
            counters.unproven_pairs, 1,
            "the unproven pair is visible to an operator"
        );
        assert_eq!(
            counters.unproven_failures, 1,
            "the withheld teardown is counted once per pair"
        );
        assert!(counters.missed_responses >= 1, "the miss is counted");
    }

    /// Plan 504 — once a pair has produced an authenticated response
    /// it is proven, and only then may two consecutive failures tear
    /// it down.
    #[test]
    fn proven_pair_is_marked_unhealthy_after_two_consecutive_failures() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        scheduler.advance_time(1_000 + FIRST_LIVENESS_DELAY_MS + 1);
        let (test_id, probe_message_id) = match scheduler.drive() {
            LivenessAction::SendTest {
                test_id,
                probe_message_id,
                ..
            } => (test_id, probe_message_id),
            other => panic!("expected SendTest, got {other:?}"),
        };
        assert_eq!(
            scheduler.record_authenticated_response(probe_message_id),
            Some(test_id)
        );
        // Two consecutive misses on the proven pair tear it down.
        let base = 1_000 + FIRST_LIVENESS_DELAY_MS + 1;
        for round in 1..=3_u64 {
            scheduler.advance_time(base + round * (LIVENESS_RESPONSE_TIMEOUT_MS + 1));
            match scheduler.drive() {
                LivenessAction::SendTest { .. } => {}
                LivenessAction::MarkUnhealthy {
                    outbound_slot,
                    consecutive_failures,
                    ..
                } => {
                    assert_eq!(outbound_slot, TunnelSlot::from_raw(1));
                    assert_eq!(consecutive_failures, LIVENESS_FAILURE_THRESHOLD);
                    assert_eq!(scheduler.counters().unhealthy_marks, 1);
                    return;
                }
                LivenessAction::Idle { .. } => {}
            }
        }
        panic!("a proven pair with two consecutive failures is marked unhealthy");
    }

    /// Plan 504 — abandoning a probe that was never emitted must not
    /// count against the pair's failure threshold.
    #[test]
    fn abandoned_test_does_not_count_a_failure() {
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(1_000);
        scheduler
            .register_pair(TunnelSlot::from_raw(1), TunnelSlot::from_raw(2))
            .expect("register");
        scheduler.advance_time(1_000 + FIRST_LIVENESS_DELAY_MS + 1);
        let test_id = match scheduler.drive() {
            LivenessAction::SendTest { test_id, .. } => test_id,
            other => panic!("expected SendTest, got {other:?}"),
        };
        assert!(scheduler.abandon_test(test_id));
        assert_eq!(scheduler.pending_tests(), 0);
        assert_eq!(scheduler.counters().missed_responses, 0);
        assert!(!scheduler.abandon_test(test_id), "the test is gone");
    }
}
