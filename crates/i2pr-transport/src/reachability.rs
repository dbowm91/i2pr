//! Conservative router-level reachability and publication policy (Plan 159).
//!
//! This module owns the transport-neutral reachability state machine
//! that sits outside every protocol codec. Packet code (SSU2 or
//! otherwise) emits privacy-safe typed [`ReachabilitySignal`] values;
//! this policy accumulates them into one [`ReachabilityState`] that
//! publication snapshots consume. The machine never sees mutable
//! packet/session objects — publication consumes [`ReachabilitySnapshot`]
//! copies only.
//!
//! States (plan §6):
//!
//! ```text
//! Unknown -> ObservedUnconfirmed -> CandidateReachable -> Reachable
//!                                          ↘ Firewalled / Unreachable
//! ```
//!
//! Conservative rules enforced before Plan 160 peer-test evidence exists:
//!
//! - one peer's external-address observation can never reach
//!   `Reachable` (structural: [`ReachabilityPolicy`] requires at least
//!   two corroborating signal kinds, and `PeerObservedExternalAddress`
//!   contributes at most one);
//! - a locally configured public endpoint follows explicit
//!   configuration policy ([`ReachabilityPolicy::configured_direct_allowed`]),
//!   never inference;
//! - contradictory observations reduce confidence and keep the state
//!   unconfirmed;
//! - observations expire; the snapshot carries the expiry so direct
//!   addresses withdraw when evidence lapses.
//!
//! Plan 160 feeds peer-test/introducer evidence into these same
//! `PeerTestResult` / `RelayFirewalledSignal` variants.
//!
//! Normative traceability: `plans/159-m8-ssu2-path-validation-`
//! `publication-and-transport-selection.md` §§5–6. No sockets, no Tokio,
//! no async; every contract is a concrete struct/enum.

use std::collections::VecDeque;
use std::time::Duration;

use crate::snapshot::ReachabilityObservation;
use crate::types::{
    AddressFamily, AddressOrigin, Confidence, MAX_DEADLINE, MAX_REACHABILITY_OBSERVATIONS,
    Reachability, TransportKind, ValidationState,
};

/// Maximum corroborating observations retained by one tracker.
///
/// Reuses the manager observation bound so router-wide accounting stays
/// comparable; over-bound signals evict the oldest, exactly like
/// [`crate::TransportManager::record_reachability`].
pub const MAX_TRACKED_OBSERVATIONS: usize = MAX_REACHABILITY_OBSERVATIONS;

/// Default observation lifetime before evidence expires.
pub const DEFAULT_OBSERVATION_TTL: Duration = Duration::from_secs(30 * 60);

/// Minimum corroborating signal kinds required by policy validation.
///
/// Values below two are rejected: a single observation class must never
/// be sufficient for `Reachable`.
pub const MIN_CORROBORATION_FLOOR: usize = 2;

/// Conservative router-level reachability state.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReachabilityState {
    /// No observation has been recorded.
    #[default]
    Unknown,
    /// Evidence exists but corroboration, configuration, or freshness
    /// policy does not support a reachability claim.
    ObservedUnconfirmed,
    /// Corroborated evidence supports reachability, but full
    /// confirmation (peer-test or configured binding) is pending.
    CandidateReachable,
    /// Corroborated evidence plus confirmation supports publication.
    Reachable,
    /// Corroborated evidence indicates a firewall/NAT without direct
    /// reachability.
    Firewalled,
    /// Corroborated evidence indicates unreachability.
    Unreachable,
}

/// Privacy-safe typed reachability signal (plan §5).
///
/// Variants carry only the address family — never a literal endpoint —
/// so snapshots stay redacted by construction. The peer-test and
/// relay variants are structurally present for Plan 160; this plan
/// consumes them conservatively (success corroborates, failure
/// contradicts) without implementing the Plan 160 protocols.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReachabilitySignal {
    /// A locally configured bind address exists for the family.
    LocalConfiguredBind {
        /// The configured address family.
        family: AddressFamily,
    },
    /// An authenticated peer reported seeing us at an external address.
    AuthenticatedPeerObservedExternalAddress {
        /// The observed address family.
        family: AddressFamily,
    },
    /// A path completed authenticated validation for the family.
    ValidatedPath {
        /// The validated path family.
        family: AddressFamily,
    },
    /// A peer-test round completed (Plan 160 owns the protocol).
    PeerTestResult {
        /// The tested address family.
        family: AddressFamily,
        /// Whether the test succeeded.
        success: bool,
    },
    /// A relay/firewalled indication arrived (Plan 160 owns the protocol).
    RelayFirewalledSignal {
        /// The affected address family.
        family: AddressFamily,
    },
}

impl ReachabilitySignal {
    /// Returns the signal family.
    pub const fn family(self) -> AddressFamily {
        match self {
            Self::LocalConfiguredBind { family }
            | Self::AuthenticatedPeerObservedExternalAddress { family }
            | Self::ValidatedPath { family }
            | Self::PeerTestResult { family, .. }
            | Self::RelayFirewalledSignal { family } => family,
        }
    }

    /// Returns whether the signal supports direct reachability.
    const fn supports_reachability(self) -> bool {
        match self {
            Self::LocalConfiguredBind { .. }
            | Self::AuthenticatedPeerObservedExternalAddress { .. }
            | Self::ValidatedPath { .. } => true,
            Self::PeerTestResult { success, .. } => success,
            Self::RelayFirewalledSignal { .. } => false,
        }
    }

    /// Returns whether the signal contradicts direct reachability.
    const fn contradicts_reachability(self) -> bool {
        match self {
            Self::PeerTestResult { success, .. } => !success,
            Self::RelayFirewalledSignal { .. } => true,
            _ => false,
        }
    }

    /// Returns the corroboration class: signals in the same class from
    /// the same family do not corroborate each other, so one peer's
    /// repeated external-address observation can never reach
    /// `Reachable` alone.
    const fn corroboration_class(self) -> u8 {
        match self {
            Self::LocalConfiguredBind { .. } => 0,
            Self::AuthenticatedPeerObservedExternalAddress { .. } => 1,
            Self::ValidatedPath { .. } => 2,
            Self::PeerTestResult { .. } => 3,
            Self::RelayFirewalledSignal { .. } => 4,
        }
    }
}

/// Tunable corroboration/expiry policy with a structural safety floor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReachabilityPolicy {
    min_corroboration: usize,
    observation_ttl: Duration,
    configured_direct_allowed: bool,
}

impl ReachabilityPolicy {
    /// Builds an explicit policy.
    ///
    /// `min_corroboration` counts distinct supporting signal classes
    /// and must be at least two; `observation_ttl` bounds evidence
    /// freshness; `configured_direct_allowed` lets an explicit local
    /// configuration (never inference) corroborate direct reachability.
    pub const fn new(
        min_corroboration: usize,
        observation_ttl: Duration,
        configured_direct_allowed: bool,
    ) -> Self {
        Self {
            min_corroboration,
            observation_ttl,
            configured_direct_allowed,
        }
    }

    /// Returns the corroboration threshold.
    pub const fn min_corroboration(self) -> usize {
        self.min_corroboration
    }

    /// Returns the observation lifetime.
    pub const fn observation_ttl(self) -> Duration {
        self.observation_ttl
    }

    /// Returns whether explicit local configuration corroborates.
    pub const fn configured_direct_allowed(self) -> bool {
        self.configured_direct_allowed
    }

    /// Validates ceilings and the corroboration safety floor.
    pub fn validate(self) -> Result<Self, ReachabilityPolicyError> {
        if self.min_corroboration < MIN_CORROBORATION_FLOOR {
            return Err(ReachabilityPolicyError::CorroborationTooLow);
        }
        if self.min_corroboration > MAX_TRACKED_OBSERVATIONS {
            return Err(ReachabilityPolicyError::CorroborationTooHigh);
        }
        if self.observation_ttl.is_zero() || self.observation_ttl > MAX_DEADLINE {
            return Err(ReachabilityPolicyError::InvalidTtl);
        }
        Ok(self)
    }
}

impl Default for ReachabilityPolicy {
    fn default() -> Self {
        Self {
            min_corroboration: 2,
            observation_ttl: DEFAULT_OBSERVATION_TTL,
            configured_direct_allowed: false,
        }
    }
}

/// Policy construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReachabilityPolicyError {
    /// Fewer than two corroborating classes were required.
    CorroborationTooLow,
    /// The threshold exceeds the bounded observation budget.
    CorroborationTooHigh,
    /// The TTL is zero or beyond the monotonic horizon.
    InvalidTtl,
}

impl std::fmt::Display for ReachabilityPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CorroborationTooLow => {
                formatter.write_str("reachability corroboration must require at least two classes")
            }
            Self::CorroborationTooHigh => {
                formatter.write_str("reachability corroboration exceeds its observation bound")
            }
            Self::InvalidTtl => formatter.write_str("reachability observation TTL is out of range"),
        }
    }
}

impl std::error::Error for ReachabilityPolicyError {}

/// One retained signal with its monotonic observation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrackedSignal {
    signal: ReachabilitySignal,
    observed_at: Duration,
}

/// Privacy-safe publication input: the reachability snapshot.
///
/// Publication consumes this copy, never mutable packet/session state.
/// `corroboration` counts distinct supporting classes; `expires_at` is
/// the earliest supporting-evidence expiry, after which direct
/// addresses must withdraw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReachabilitySnapshot {
    /// The current conservative state.
    pub state: ReachabilityState,
    /// Distinct supporting signal classes behind the state.
    pub corroboration: usize,
    /// Monotonic time the supporting evidence expires.
    pub expires_at: Duration,
    /// The most recently observed family, if any.
    pub family: AddressFamily,
}

/// The conservative router-level reachability tracker.
#[derive(Clone, Debug)]
pub struct ReachabilityTracker {
    policy: ReachabilityPolicy,
    signals: VecDeque<TrackedSignal>,
    state: ReachabilityState,
    state_since: Duration,
}

impl ReachabilityTracker {
    /// Creates a tracker with an explicitly validated policy.
    pub fn new(policy: ReachabilityPolicy) -> Result<Self, ReachabilityPolicyError> {
        Ok(Self {
            policy: policy.validate()?,
            signals: VecDeque::new(),
            state: ReachabilityState::Unknown,
            state_since: Duration::ZERO,
        })
    }

    /// Returns the current state.
    pub const fn state(&self) -> ReachabilityState {
        self.state
    }

    /// Returns the tracker policy.
    pub const fn policy(&self) -> ReachabilityPolicy {
        self.policy
    }

    /// Returns the number of retained signals.
    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    /// Records one typed signal and recomputes the state.
    ///
    /// Over-bound signals evict the oldest first. Unknown-family
    /// signals are ignored without mutating state.
    pub fn record(&mut self, signal: ReachabilitySignal, now: Duration) -> ReachabilityState {
        if signal.family() == AddressFamily::Unknown {
            return self.state;
        }
        self.expire_locked(now);
        if self.signals.len() >= MAX_TRACKED_OBSERVATIONS {
            self.signals.pop_front();
        }
        self.signals.push_back(TrackedSignal {
            signal,
            observed_at: now,
        });
        self.recompute(now);
        self.state
    }

    /// Drops expired signals and downgrades the state when evidence
    /// lapses. Returns whether the state changed.
    pub fn poll_expiry(&mut self, now: Duration) -> bool {
        let before = self.state;
        self.expire_locked(now);
        self.recompute(now);
        self.state != before
    }

    /// Returns the publication snapshot copy for this tracker.
    pub fn snapshot(&self, now: Duration) -> ReachabilitySnapshot {
        let expires_at = self
            .signals
            .iter()
            .filter(|entry| entry.signal.supports_reachability())
            .map(|entry| {
                entry
                    .observed_at
                    .saturating_add(self.policy.observation_ttl)
            })
            .min()
            .unwrap_or(now);
        let family = self
            .signals
            .back()
            .map(|entry| entry.signal.family())
            .unwrap_or(AddressFamily::Unknown);
        ReachabilitySnapshot {
            state: self.state,
            corroboration: self.supporting_classes(),
            expires_at,
            family,
        }
    }

    /// Maps the current state to the transport-neutral manager
    /// observation vocabulary so runtimes can retain it with
    /// [`crate::TransportManager::record_reachability`].
    pub fn as_transport_observation(
        &self,
        transport: TransportKind,
        observed_at: Duration,
    ) -> ReachabilityObservation {
        let family = self
            .signals
            .back()
            .map(|entry| entry.signal.family())
            .unwrap_or(AddressFamily::Unknown);
        let (reachability, validation) = match self.state {
            ReachabilityState::Unknown => (Reachability::Unknown, ValidationState::Unvalidated),
            ReachabilityState::ObservedUnconfirmed => {
                (Reachability::Unconfirmed, ValidationState::Unvalidated)
            }
            ReachabilityState::CandidateReachable => {
                (Reachability::Unconfirmed, ValidationState::Validated)
            }
            ReachabilityState::Reachable => (Reachability::Reachable, ValidationState::Validated),
            ReachabilityState::Firewalled | ReachabilityState::Unreachable => {
                (Reachability::Unreachable, ValidationState::Validated)
            }
        };
        let origin = if self
            .signals
            .iter()
            .any(|entry| matches!(entry.signal, ReachabilitySignal::LocalConfiguredBind { .. }))
        {
            AddressOrigin::Configured
        } else {
            AddressOrigin::Observed
        };
        ReachabilityObservation {
            transport,
            origin,
            family,
            reachability,
            observed_at,
            validation,
            confidence: None,
        }
    }

    fn expire_locked(&mut self, now: Duration) {
        while let Some(front) = self.signals.front() {
            if front
                .observed_at
                .saturating_add(self.policy.observation_ttl)
                > now
            {
                break;
            }
            self.signals.pop_front();
        }
    }

    /// Counts distinct supporting signal classes for the latest family.
    fn supporting_classes(&self) -> usize {
        let Some(latest) = self.signals.back().map(|entry| entry.signal.family()) else {
            return 0;
        };
        let mut classes = [false; 5];
        for entry in &self.signals {
            if entry.signal.family() != latest || !entry.signal.supports_reachability() {
                continue;
            }
            if matches!(entry.signal, ReachabilitySignal::LocalConfiguredBind { .. })
                && !self.policy.configured_direct_allowed
            {
                continue;
            }
            classes[entry.signal.corroboration_class() as usize] = true;
        }
        classes.iter().filter(|present| **present).count()
    }

    fn contradicting_classes(&self) -> usize {
        let Some(latest) = self.signals.back().map(|entry| entry.signal.family()) else {
            return 0;
        };
        let mut classes = [false; 5];
        for entry in &self.signals {
            if entry.signal.family() != latest || !entry.signal.contradicts_reachability() {
                continue;
            }
            classes[entry.signal.corroboration_class() as usize] = true;
        }
        classes.iter().filter(|present| **present).count()
    }

    fn firewalled_classes(&self) -> usize {
        let Some(latest) = self.signals.back().map(|entry| entry.signal.family()) else {
            return 0;
        };
        let mut classes = [false; 5];
        for entry in &self.signals {
            if entry.signal.family() != latest {
                continue;
            }
            if !matches!(
                entry.signal,
                ReachabilitySignal::RelayFirewalledSignal { .. }
            ) {
                continue;
            }
            classes[entry.signal.corroboration_class() as usize] = true;
        }
        classes.iter().filter(|present| **present).count()
    }

    fn recompute(&mut self, now: Duration) {
        if self.signals.is_empty() {
            self.transition(ReachabilityState::Unknown, now);
            return;
        }
        // Contradictory evidence keeps the router unconfirmed: it takes
        // precedence over corroboration until the contradiction expires.
        if self.contradicting_classes() >= self.policy.min_corroboration {
            let firewalled = self.firewalled_classes() >= self.policy.min_corroboration
                || self.signals.iter().any(|entry| {
                    matches!(
                        entry.signal,
                        ReachabilitySignal::RelayFirewalledSignal { .. }
                    )
                });
            self.transition(
                if firewalled {
                    ReachabilityState::Firewalled
                } else {
                    ReachabilityState::Unreachable
                },
                now,
            );
            return;
        }
        if self.contradicting_classes() > 0 {
            self.transition(ReachabilityState::ObservedUnconfirmed, now);
            return;
        }
        let supporting = self.supporting_classes();
        if supporting > self.policy.min_corroboration {
            self.transition(ReachabilityState::Reachable, now);
        } else if supporting >= self.policy.min_corroboration {
            self.transition(ReachabilityState::CandidateReachable, now);
        } else {
            self.transition(ReachabilityState::ObservedUnconfirmed, now);
        }
    }

    fn transition(&mut self, next: ReachabilityState, now: Duration) {
        if self.state != next {
            self.state = next;
            self.state_since = now;
        }
    }
}

/// A bounded confidence annotation helper for snapshots.
///
/// Confidence stays optional in [`ReachabilityObservation`]; this
/// helper maps corroboration depth to a bounded score without
/// inventing per-peer precision.
pub const fn corroboration_confidence(corroboration: usize) -> Option<Confidence> {
    match corroboration {
        0 | 1 => None,
        2 => match Confidence::new(50) {
            Ok(value) => Some(value),
            Err(_) => None,
        },
        _ => match Confidence::new(75) {
            Ok(value) => Some(value),
            Err(_) => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> ReachabilityTracker {
        ReachabilityTracker::new(ReachabilityPolicy::default()).expect("policy")
    }

    fn configured_tracker() -> ReachabilityTracker {
        ReachabilityTracker::new(ReachabilityPolicy::new(2, DEFAULT_OBSERVATION_TTL, true))
            .expect("policy")
    }

    #[test]
    fn policy_rejects_single_observation_corroboration() {
        assert_eq!(
            ReachabilityPolicy::new(1, DEFAULT_OBSERVATION_TTL, false).validate(),
            Err(ReachabilityPolicyError::CorroborationTooLow)
        );
        assert_eq!(
            ReachabilityPolicy::new(0, DEFAULT_OBSERVATION_TTL, false).validate(),
            Err(ReachabilityPolicyError::CorroborationTooLow)
        );
    }

    #[test]
    fn single_peer_observation_never_reaches_reachable() {
        let mut tracker = tracker();
        let now = Duration::from_secs(10);
        // One peer observation, then repeats of the same class: the
        // class contributes once, so the state stalls unconfirmed.
        for step in 0..6 {
            let state = tracker.record(
                ReachabilitySignal::AuthenticatedPeerObservedExternalAddress {
                    family: AddressFamily::Ipv4,
                },
                now + Duration::from_secs(step),
            );
            assert_eq!(state, ReachabilityState::ObservedUnconfirmed);
        }
        assert_eq!(tracker.supporting_classes(), 1);
    }

    #[test]
    fn corroborated_path_and_peer_observation_become_candidate() {
        let mut tracker = tracker();
        let now = Duration::from_secs(100);
        assert_eq!(
            tracker.record(
                ReachabilitySignal::AuthenticatedPeerObservedExternalAddress {
                    family: AddressFamily::Ipv4,
                },
                now,
            ),
            ReachabilityState::ObservedUnconfirmed
        );
        assert_eq!(
            tracker.record(
                ReachabilitySignal::ValidatedPath {
                    family: AddressFamily::Ipv4,
                },
                now + Duration::from_secs(1),
            ),
            ReachabilityState::CandidateReachable
        );
    }

    #[test]
    fn configured_binding_corroborates_only_under_explicit_policy() {
        let mut tracker = tracker();
        let now = Duration::from_secs(200);
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv4,
            },
            now,
        );
        // Inference is disabled: configuration alone is unconfirmed.
        assert_eq!(
            tracker.record(
                ReachabilitySignal::ValidatedPath {
                    family: AddressFamily::Ipv4,
                },
                now + Duration::from_secs(1),
            ),
            ReachabilityState::ObservedUnconfirmed
        );

        let mut allowed = configured_tracker();
        allowed.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv4,
            },
            now,
        );
        assert_eq!(
            allowed.record(
                ReachabilitySignal::ValidatedPath {
                    family: AddressFamily::Ipv4,
                },
                now + Duration::from_secs(1),
            ),
            ReachabilityState::CandidateReachable
        );
    }

    #[test]
    fn third_class_promotes_candidate_to_reachable() {
        let mut tracker = configured_tracker();
        let now = Duration::from_secs(300);
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv4,
            },
            now,
        );
        tracker.record(
            ReachabilitySignal::ValidatedPath {
                family: AddressFamily::Ipv4,
            },
            now + Duration::from_secs(1),
        );
        assert_eq!(
            tracker.record(
                ReachabilitySignal::PeerTestResult {
                    family: AddressFamily::Ipv4,
                    success: true,
                },
                now + Duration::from_secs(2),
            ),
            ReachabilityState::Reachable
        );
    }

    #[test]
    fn contradictory_observation_keeps_state_unconfirmed() {
        let mut tracker = configured_tracker();
        let now = Duration::from_secs(400);
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv4,
            },
            now,
        );
        tracker.record(
            ReachabilitySignal::ValidatedPath {
                family: AddressFamily::Ipv4,
            },
            now + Duration::from_secs(1),
        );
        assert_eq!(tracker.state(), ReachabilityState::CandidateReachable);
        assert_eq!(
            tracker.record(
                ReachabilitySignal::PeerTestResult {
                    family: AddressFamily::Ipv4,
                    success: false,
                },
                now + Duration::from_secs(2),
            ),
            ReachabilityState::ObservedUnconfirmed
        );
    }

    #[test]
    fn corroborated_failures_mark_unreachable() {
        let mut tracker = tracker();
        let now = Duration::from_secs(500);
        tracker.record(
            ReachabilitySignal::PeerTestResult {
                family: AddressFamily::Ipv4,
                success: false,
            },
            now,
        );
        // Same class twice still contradicts once; add the relay class.
        tracker.record(
            ReachabilitySignal::RelayFirewalledSignal {
                family: AddressFamily::Ipv4,
            },
            now + Duration::from_secs(1),
        );
        assert_eq!(tracker.state(), ReachabilityState::Firewalled);
    }

    #[test]
    fn expiry_withdraws_support_and_downgrades() {
        let mut tracker = configured_tracker();
        let start = Duration::from_secs(600);
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv4,
            },
            start,
        );
        tracker.record(
            ReachabilitySignal::ValidatedPath {
                family: AddressFamily::Ipv4,
            },
            start + Duration::from_secs(1),
        );
        assert_eq!(tracker.state(), ReachabilityState::CandidateReachable);
        let snapshot = tracker.snapshot(start + Duration::from_secs(1));
        assert_eq!(snapshot.state, ReachabilityState::CandidateReachable);
        assert_eq!(snapshot.corroboration, 2);
        // Past the TTL the evidence lapses back to unknown.
        let changed = tracker.poll_expiry(start + DEFAULT_OBSERVATION_TTL + Duration::from_secs(2));
        assert!(changed);
        assert_eq!(tracker.state(), ReachabilityState::Unknown);
    }

    #[test]
    fn snapshot_expiry_uses_earliest_supporting_evidence() {
        let mut tracker = configured_tracker();
        let start = Duration::from_secs(700);
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv4,
            },
            start,
        );
        tracker.record(
            ReachabilitySignal::ValidatedPath {
                family: AddressFamily::Ipv4,
            },
            start + Duration::from_secs(60),
        );
        let snapshot = tracker.snapshot(start + Duration::from_secs(60));
        assert_eq!(snapshot.expires_at, start + DEFAULT_OBSERVATION_TTL);
    }

    #[test]
    fn unknown_family_signals_are_ignored() {
        let mut tracker = tracker();
        let state = tracker.record(
            ReachabilitySignal::ValidatedPath {
                family: AddressFamily::Unknown,
            },
            Duration::from_secs(800),
        );
        assert_eq!(state, ReachabilityState::Unknown);
        assert_eq!(tracker.signal_count(), 0);
    }

    #[test]
    fn transport_observation_mapping_stays_redacted() {
        let mut tracker = configured_tracker();
        let now = Duration::from_secs(900);
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind {
                family: AddressFamily::Ipv6,
            },
            now,
        );
        tracker.record(
            ReachabilitySignal::ValidatedPath {
                family: AddressFamily::Ipv6,
            },
            now + Duration::from_secs(1),
        );
        let observation =
            tracker.as_transport_observation(TransportKind::Ssu2, now + Duration::from_secs(1));
        assert_eq!(observation.reachability, Reachability::Unconfirmed);
        assert_eq!(observation.validation, ValidationState::Validated);
        assert_eq!(observation.family, AddressFamily::Ipv6);
        assert_eq!(observation.origin, AddressOrigin::Configured);
        let debug = format!("{observation:?}");
        assert!(!debug.contains("127.0.0.1"));
    }
}
