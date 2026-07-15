//! Deterministic, bounded simulation primitives for local state-machine tests.
//!
//! `i2pr-testkit` is intentionally outside the production dependency graph.
//! It opens no sockets, performs no DNS lookups, and never contacts the I2P
//! network. Simulation is a controllable model of queues and failures, not a
//! transport-interoperability claim.

#![forbid(unsafe_code)]

mod clock;
mod faults;
mod network;
mod peers;
mod rng;

pub use clock::{
    ClockError, Deadline, MAX_PENDING_TIMERS, ManualClock, ManualInstant, MonotonicClock,
    MonotonicInstant, TokioClock,
};
pub use faults::{
    FaultAction, FaultError, FaultMatcher, FaultRule, FaultScript, FaultUnitKind, LinkDirection,
    LinkId, MAX_DUPLICATE_UNITS, MAX_FAULT_RULES,
};
pub use network::{
    AdvanceReport, DatagramConfig, DatagramEndpoint, DatagramError, DatagramLink, DatagramPacket,
    MAX_DATAGRAM_SIZE, MAX_LINK_ID, NetworkScheduler, ReplayEvent, SchedulerConfig, SchedulerError,
    SchedulerSnapshot, StreamConfig, StreamEndpoint, StreamError, StreamLink, SyntheticAddress,
};
pub use peers::{
    MAX_TEST_PEERS, PeerFactory, PeerFactoryError, PeerId, PeerSummary, SyntheticServiceId,
    TestPeer, Topology, TopologyError, TopologyKind,
};
pub use rng::{
    DeterministicRng, MAX_DOMAIN_LABEL_BYTES, ReproducibilitySeed, SeedDerivationError,
    SeedParseError,
};

/// A bounded deterministic simulation harness.
///
/// The harness is deliberately a synchronous/manual pump. Callers may run
/// supervised Tokio services against its endpoints, but the harness itself
/// never spawns or detaches a task. `shutdown` cancels the caller-visible
/// scope and closes all pending simulated work.
#[derive(Debug)]
pub struct SimulationHarness {
    seed: ReproducibilitySeed,
    scenario: String,
    clock: ManualClock,
    scheduler: NetworkScheduler,
    cancellation: i2pr_runtime::CancellationToken,
    steps: usize,
}

/// Maximum scenario identifier length retained in replay records.
pub const MAX_SCENARIO_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HarnessError {
    /// The scenario identifier was empty or too long.
    InvalidScenario,
    /// The scheduler could not advance.
    Scheduler(SchedulerError),
    /// The scheduler reached the explicit step limit.
    StepLimit { maximum: usize },
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScenario => formatter.write_str("invalid simulation scenario identifier"),
            Self::Scheduler(error) => error.fmt(formatter),
            Self::StepLimit { maximum } => {
                write!(formatter, "simulation step limit {maximum} reached")
            }
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<SchedulerError> for HarnessError {
    fn from(error: SchedulerError) -> Self {
        Self::Scheduler(error)
    }
}

impl SimulationHarness {
    /// Creates a bounded manual-pump harness with a stable scenario name.
    pub fn new(
        seed: ReproducibilitySeed,
        scenario: impl Into<String>,
        scheduler: NetworkScheduler,
    ) -> Result<Self, HarnessError> {
        let scenario = scenario.into();
        if scenario.is_empty() || scenario.len() > MAX_SCENARIO_BYTES {
            return Err(HarnessError::InvalidScenario);
        }
        Ok(Self {
            seed,
            scenario,
            clock: scheduler.clock().clone(),
            scheduler,
            cancellation: i2pr_runtime::CancellationToken::new(),
            steps: 0,
        })
    }

    /// Returns the root seed used for this scenario.
    pub const fn seed(&self) -> ReproducibilitySeed {
        self.seed
    }

    /// Returns the stable scenario identifier.
    pub fn scenario(&self) -> &str {
        &self.scenario
    }

    /// Returns the manual clock owned by the scheduler.
    pub const fn clock(&self) -> &ManualClock {
        &self.clock
    }

    /// Returns the scheduler used by this harness.
    pub const fn scheduler(&self) -> &NetworkScheduler {
        &self.scheduler
    }

    /// Returns the caller-visible cancellation scope.
    pub const fn cancellation(&self) -> &i2pr_runtime::CancellationToken {
        &self.cancellation
    }

    /// Validates that the manual harness is open and ready to pump.
    pub fn start(&self) -> Result<(), HarnessError> {
        if self.scheduler.snapshot().closed {
            Err(HarnessError::Scheduler(SchedulerError::Closed))
        } else {
            Ok(())
        }
    }

    /// Injects deterministic test-harness cancellation.
    pub fn inject_cancellation(&self) {
        let _ = self
            .cancellation
            .cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
    }

    /// Pumps until a caller-supplied bounded predicate becomes true.
    pub fn run_until<F>(&mut self, maximum: usize, mut predicate: F) -> Result<bool, HarnessError>
    where
        F: FnMut(&SchedulerSnapshot) -> bool,
    {
        if maximum == 0 {
            return Err(HarnessError::StepLimit { maximum });
        }
        for _ in 0..maximum {
            if predicate(&self.scheduler.snapshot()) {
                return Ok(true);
            }
            if !self.scheduler.has_pending() {
                return Ok(predicate(&self.scheduler.snapshot()));
            }
            self.advance_to_next_event()?;
        }
        Err(HarnessError::StepLimit { maximum })
    }

    /// Advances to and delivers the next scheduled event.
    pub fn advance_to_next_event(&mut self) -> Result<bool, HarnessError> {
        self.steps = self.steps.saturating_add(1);
        self.scheduler
            .advance_to_next_event()
            .map(|report| report.progressed)
            .map_err(HarnessError::from)
    }

    /// Advances manual time and delivers all units due at the new instant.
    pub fn advance(
        &mut self,
        duration: std::time::Duration,
    ) -> Result<AdvanceReport, HarnessError> {
        self.steps = self.steps.saturating_add(1);
        self.scheduler.advance(duration).map_err(HarnessError::from)
    }

    /// Runs the scheduler until no delivery remains or `maximum` pumps occur.
    pub fn run_until_idle(&mut self, maximum: usize) -> Result<usize, HarnessError> {
        if maximum == 0 {
            return Err(HarnessError::StepLimit { maximum });
        }
        let mut delivered = 0;
        for _ in 0..maximum {
            if !self.scheduler.has_pending() {
                return Ok(delivered);
            }
            let report = self
                .scheduler
                .advance_to_next_event()
                .map_err(HarnessError::from)?;
            self.steps = self.steps.saturating_add(1);
            delivered = delivered.saturating_add(report.delivered);
            if !report.progressed && self.scheduler.has_pending() {
                return Err(HarnessError::StepLimit { maximum });
            }
        }
        Err(HarnessError::StepLimit { maximum })
    }

    /// Returns a privacy-safe deterministic replay record.
    pub fn replay(&self) -> ReplayRecord {
        self.scheduler.replay(self.seed, &self.scenario, self.steps)
    }

    /// Cancels the caller-visible scope and drops all queued simulated work.
    pub fn shutdown(&self) -> SchedulerSnapshot {
        let _ = self
            .cancellation
            .cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        self.scheduler.close();
        self.scheduler.snapshot()
    }
}

/// A bounded replay record with no payload or secret material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayRecord {
    /// Root seed, safe to print and parse for reproduction.
    pub seed: ReproducibilitySeed,
    /// Stable scenario identifier.
    pub scenario: String,
    /// Applied fault and delivery outcomes.
    pub events: Vec<ReplayEvent>,
    /// Final simulated time.
    pub final_time: ManualInstant,
    /// Final task/queue/timer/resource snapshot.
    pub snapshot: SchedulerSnapshot,
    /// Number of manual pump steps used.
    pub steps: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn seed_domains_are_order_independent() {
        let root = ReproducibilitySeed::from_u128(7);
        let left = root.derive("link/a").expect("domain");
        let right = root.derive("link/b").expect("domain");
        assert_ne!(left, right);
        assert_eq!(left, root.derive("link/a").expect("domain"));
        assert_eq!(
            root.to_string().parse::<ReproducibilitySeed>().unwrap(),
            root
        );
    }

    #[tokio::test(start_paused = true)]
    async fn manual_clock_wakes_equal_deadline_sleepers() {
        let clock = ManualClock::new();
        let deadline = clock.deadline_after(Duration::from_secs(5)).unwrap();
        let first_clock = clock.clone();
        let second_clock = clock.clone();
        let first = tokio::spawn(async move { first_clock.sleep_until(deadline).await });
        let second = tokio::spawn(async move { second_clock.sleep_until(deadline).await });
        tokio::task::yield_now().await;
        assert_eq!(clock.pending_timers(), 2);
        clock.advance(Duration::from_secs(5)).unwrap();
        assert_eq!(first.await.unwrap(), Ok(()));
        assert_eq!(second.await.unwrap(), Ok(()));
        assert_eq!(clock.pending_timers(), 0);
    }

    #[test]
    fn stream_delivery_is_ordered_and_partial() {
        let clock = ManualClock::new();
        let scheduler = NetworkScheduler::new(clock, SchedulerConfig::default()).unwrap();
        let faults = FaultScript::empty(ReproducibilitySeed::from_u128(1));
        let link = scheduler
            .stream_link(
                LinkId::new(1).unwrap(),
                StreamConfig::new(16, 2).unwrap(),
                faults,
            )
            .unwrap();
        let left = link.left();
        let right = link.right();
        assert_eq!(left.try_write(b"abcd").unwrap(), 2);
        scheduler.advance(Duration::ZERO).unwrap();
        let mut output = [0; 2];
        assert_eq!(right.try_read(&mut output).unwrap(), Some(2));
        assert_eq!(&output, b"ab");
        assert_eq!(left.try_write(b"cd").unwrap(), 2);
        scheduler.advance(Duration::ZERO).unwrap();
        assert_eq!(right.try_read(&mut output).unwrap(), Some(2));
        assert_eq!(&output, b"cd");
    }

    #[test]
    fn datagram_boundaries_and_sources_are_preserved() {
        let clock = ManualClock::new();
        let scheduler = NetworkScheduler::new(clock, SchedulerConfig::default()).unwrap();
        let faults = FaultScript::empty(ReproducibilitySeed::from_u128(2));
        let link = scheduler
            .datagram_link(
                LinkId::new(2).unwrap(),
                DatagramConfig::new(8, 2).unwrap(),
                faults,
            )
            .unwrap();
        let left = link.left();
        let right = link.right();
        assert_eq!(left.try_send(b"one").unwrap(), 3);
        assert_eq!(left.try_send(b"two").unwrap(), 3);
        scheduler.advance(Duration::ZERO).unwrap();
        let first = right.try_recv().unwrap().unwrap();
        let second = right.try_recv().unwrap().unwrap();
        assert_eq!(first.payload, b"one");
        assert_eq!(second.payload, b"two");
        assert_eq!(first.source, left.address());
    }

    #[test]
    fn faults_are_executable_and_replay_safe() {
        let seed = ReproducibilitySeed::from_u128(3);
        let rule = FaultRule::new(7, FaultMatcher::any(), FaultAction::duplicate(1).unwrap());
        let script = FaultScript::new(seed, vec![rule]).unwrap();
        let clock = ManualClock::new();
        let scheduler = NetworkScheduler::new(clock, SchedulerConfig::default()).unwrap();
        let link = scheduler
            .stream_link(
                LinkId::new(3).unwrap(),
                StreamConfig::new(16, 16).unwrap(),
                script,
            )
            .unwrap();
        link.left().try_write(b"x").unwrap();
        scheduler.advance(Duration::ZERO).unwrap();
        let right = link.right();
        let mut value = [0; 1];
        assert_eq!(right.try_read(&mut value).unwrap(), Some(1));
        assert_eq!(right.try_read(&mut value).unwrap(), Some(1));
        assert!(
            scheduler
                .replay(seed, "fault", 1)
                .events
                .iter()
                .any(|event| event.rules == vec![7])
        );
    }

    #[test]
    fn peer_factory_is_reproducible_without_files() {
        let seed = ReproducibilitySeed::from_u128(4);
        let factory = PeerFactory::new(seed, 2).unwrap();
        let one = factory.peer(0).unwrap();
        let two = PeerFactory::new(seed, 2).unwrap().peer(0).unwrap();
        assert_eq!(one.summary(), two.summary());
        assert!(format!("{one:?}").contains("TestPeer"));
        assert!(!format!("{one:?}").contains("private"));
        assert!(one.router_info(1).is_ok());
        assert_eq!(factory.service_id(0).unwrap().get(), 1);
    }

    #[test]
    fn fixed_seed_matrix_replays_identically() {
        for seed_value in [0_u128, u128::MAX, 1, 2, 3, 4, 5] {
            let seed = ReproducibilitySeed::from_u128(seed_value);
            let make_replay = || {
                let scheduler =
                    NetworkScheduler::new(ManualClock::new(), SchedulerConfig::default()).unwrap();
                let link = scheduler
                    .datagram_link(
                        LinkId::new(10).unwrap(),
                        DatagramConfig::default(),
                        FaultScript::empty(seed),
                    )
                    .unwrap();
                link.left().try_send(b"seeded").unwrap();
                scheduler.advance(Duration::ZERO).unwrap();
                scheduler.replay(seed, "seed-matrix", 1)
            };
            assert_eq!(make_replay(), make_replay());
        }
    }

    #[test]
    fn harness_reaches_idle_and_tears_down() {
        let seed = ReproducibilitySeed::from_u128(5);
        let scheduler =
            NetworkScheduler::new(ManualClock::new(), SchedulerConfig::default()).unwrap();
        let link = scheduler
            .stream_link(
                LinkId::new(4).unwrap(),
                StreamConfig::default(),
                FaultScript::empty(seed),
            )
            .unwrap();
        link.left().try_write(b"payload").unwrap();
        let mut harness = SimulationHarness::new(seed, "idle", scheduler).unwrap();
        assert_eq!(harness.run_until_idle(8).unwrap(), 1);
        let snapshot = harness.shutdown();
        assert_eq!(snapshot.pending_deliveries, 0);
        assert_eq!(snapshot.buffered_bytes, 0);
        assert_eq!(snapshot.pending_timers, 0);
    }
}
