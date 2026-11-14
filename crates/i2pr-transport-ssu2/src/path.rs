//! Authenticated SSU2 path validation and per-path MTU state (Plan 159).
//!
//! An authenticated-looking packet arriving from a new endpoint is never
//! sufficient by itself to migrate a session. This module owns the
//! runtime-neutral validation machine that the UDP runtime drives:
//!
//! ```text
//! current validated path
//! optional bounded candidate paths (per-family quotas)
//! path-challenge value/deadline per candidate
//! candidate MTU/congestion restriction (minimum MTU, conservative)
//! validation result
//! ```
//!
//! On an authenticated packet from a new endpoint the runtime must:
//!
//! - authenticate it against the existing session first (the caller
//!   only invokes this machine after [`crate::session::Ssu2Session`]
//!   accepted the datagram);
//! - create at most a bounded number of candidates;
//! - issue the spec-defined PathChallenge carrying caller-supplied
//!   challenge bytes (production: OS CSPRNG from the runtime);
//! - keep normal traffic on the current validated path;
//! - promote only after the matching authenticated PathResponse;
//! - expire candidates on deadline;
//! - never migrate on source change alone.
//!
//! MTU discipline: the validated path carries an explicit MTU
//! (`1280..=9000`); candidates are pinned to the SSU2 minimum until
//! validation; the MTU never increases from an unauthenticated packet
//! claim (there is no packet-driven MTU setter at all — only the
//! explicit [`ValidatedPath::with_mtu`] used for configured/validated
//! sources). Fragmentation consumes
//! [`crate::session::SessionConfig::max_payload_for_mtu`] of the
//! current validated MTU.
//!
//! Normative traceability: `plans/implementation/ssu2/159-m8-ssu2-path-validation-publication-and-transport-selection.md` §§2–4 and §10. No sockets,
//! no Tokio, no timers, no RNG: challenge bytes and both clocks arrive
//! as caller inputs.

use i2pr_transport::AddressFamily;
use thiserror::Error;

use crate::address::Ssu2Endpoint;
use crate::constants;

/// Maximum candidate paths retained by one session.
pub const MAX_PATH_CANDIDATES: usize = 4;
/// Maximum candidate paths retained for one address family.
///
/// IPv4 and IPv6 candidates are budgeted independently so one family
/// cannot exhaust the other's slots.
pub const MAX_CANDIDATES_PER_FAMILY: usize = 2;
/// Exact path-challenge length in bytes (well under the block cap and
/// the minimum MTU, so challenges never amplify).
pub const PATH_CHALLENGE_LENGTH: usize = 32;
/// Candidate lifetime in milliseconds before validation must complete.
pub const PATH_VALIDATION_TIMEOUT_MS: u64 = 10_000;
/// Maximum challenges issued by one session before operator action.
///
/// Bounds per-session crypto/work exposure under spoofed-source floods;
/// the runtime additionally bounds challenges service-wide.
pub const MAX_PATH_CHALLENGES_PER_SESSION: u32 = 8;
/// Candidate-path MTU: the SSU2 minimum until validation completes.
pub const PATH_CANDIDATE_MTU: u16 = constants::SSU2_MIN_MTU;

/// How many re-validations of the path **in use** one session may issue.
///
/// The budget is generous because a re-validation is a liveness probe on the
/// path that already carries traffic, not a migration attempt: at the cadence
/// the runtime drives, a session reaches this ceiling only after many hours.
/// It is a ceiling, not a target, so a pathological clock cannot turn the
/// probe into unbounded traffic. `MAX_PATH_CHALLENGES_PER_SESSION` continues
/// to govern migration candidates, which are a different question.
pub const MAX_PATH_REVALIDATIONS_PER_SESSION: u32 = 64;

/// Typed path-validation failures.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PathError {
    /// The session already retains its candidate ceiling.
    #[error("SSU2 path candidate table is full")]
    TooManyCandidates,
    /// The address family already retains its candidate ceiling.
    #[error("SSU2 path candidate family quota is full")]
    FamilyQuotaExceeded,
    /// The per-session challenge budget is exhausted.
    #[error("SSU2 path challenge budget is exhausted")]
    ChallengeBudgetExhausted,
    /// The supplied challenge value is weak (all zero).
    #[error("SSU2 path challenge value is invalid")]
    InvalidChallenge,
    /// The endpoint is not a tracked candidate.
    #[error("SSU2 path endpoint is not a tracked candidate")]
    NotACandidate,
    /// A re-validation of the path in use is already outstanding.
    #[error("SSU2 path re-validation is already in flight")]
    RevalidationInFlight,
    /// The candidate expired before the response arrived.
    #[error("SSU2 path candidate expired")]
    ExpiredCandidate,
    /// The response does not match the issued challenge.
    #[error("SSU2 path response does not match its challenge")]
    ChallengeMismatch,
    /// The MTU is outside the SSU2 `1280..=9000` range.
    #[error("SSU2 path MTU is out of range")]
    InvalidMtu,
}

/// The currently validated path: endpoint plus explicit MTU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPath {
    endpoint: Ssu2Endpoint,
    mtu: u16,
}

impl ValidatedPath {
    /// Creates a validated path after range-checking the MTU.
    pub fn new(endpoint: Ssu2Endpoint, mtu: u16) -> Result<Self, PathError> {
        if !(constants::SSU2_MIN_MTU..=constants::SSU2_MAX_MTU).contains(&mtu) {
            return Err(PathError::InvalidMtu);
        }
        Ok(Self { endpoint, mtu })
    }

    /// Returns the validated endpoint.
    pub const fn endpoint(self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the validated-path MTU.
    pub const fn mtu(self) -> u16 {
        self.mtu
    }

    /// Returns the address family of the validated path.
    pub const fn family(self) -> AddressFamily {
        self.endpoint.family()
    }

    /// Replaces the MTU from a configured/validated source only.
    ///
    /// There is deliberately no packet-driven MTU setter: an
    /// unauthenticated packet claim can never raise the MTU.
    pub fn with_mtu(self, mtu: u16) -> Result<Self, PathError> {
        Self::new(self.endpoint, mtu)
    }

    /// Returns the validated payload budget for an address family.
    pub fn max_payload_bytes(self, ipv6: bool) -> usize {
        crate::session::SessionConfig::max_payload_for_mtu(self.mtu, ipv6)
    }
}

/// One bounded unvalidated candidate path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PathCandidate {
    endpoint: Ssu2Endpoint,
    challenge: [u8; PATH_CHALLENGE_LENGTH],
    deadline_ms: u64,
}

impl PathCandidate {
    const fn expired(self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms
    }
}

/// One bounded re-validation of the path **in use**.
///
/// It is deliberately not a [`PathCandidate`]: a candidate is an endpoint the
/// router does not yet trust and may migrate to, while a re-validation names
/// the endpoint the router already publishes and must never move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Revalidation {
    challenge: [u8; PATH_CHALLENGE_LENGTH],
    deadline_ms: u64,
}

impl Revalidation {
    const fn expired(self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms
    }
}

/// One validation-machine effect for the runtime to fulfill.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathEvent {
    /// Send this exact PathChallenge to the candidate endpoint.
    ///
    /// The datagram must fit the minimum MTU; the 32-byte challenge
    /// always does.
    ChallengeToSend {
        /// The candidate endpoint awaiting proof.
        endpoint: Ssu2Endpoint,
        /// The exact challenge bytes to transmit.
        challenge: [u8; PATH_CHALLENGE_LENGTH],
    },
    /// The path already in use proved itself again.
    ///
    /// This is **not** a migration: the endpoint is unchanged, nothing moves,
    /// and no per-IP or per-subnet accounting is touched. The runtime records
    /// it as validated-path evidence and nothing else.
    Revalidated {
        /// The endpoint that proved itself, unchanged.
        endpoint: Ssu2Endpoint,
    },
    /// The candidate proved itself and was promoted.
    Validated {
        /// The previous validated endpoint.
        previous: Ssu2Endpoint,
        /// The newly validated endpoint.
        current: Ssu2Endpoint,
    },
}

/// Privacy-safe validation counters (counts only, no endpoints).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PathCounters {
    /// Challenges issued for candidates.
    pub challenges_issued: u64,
    /// Candidates promoted after matching proof.
    pub migrations: u64,
    /// Wrong/stale responses rejected without migration.
    pub rejected_responses: u64,
    /// Candidates refused by quota/budget ceilings.
    pub denied_candidates: u64,
    /// Candidates expired before proof arrived.
    pub expired_candidates: u64,
    /// Re-validations of the path in use that completed.
    pub revalidations: u64,
    /// Re-validations of the path in use that expired unanswered.
    pub expired_revalidations: u64,
}

/// The runtime-neutral path-validation state machine.
#[derive(Clone, Debug)]
pub struct PathValidator {
    validated: ValidatedPath,
    candidates: Vec<PathCandidate>,
    challenges_issued: u32,
    /// The outstanding re-validation of the path in use, if any.
    revalidation: Option<Revalidation>,
    revalidations_issued: u32,
    counters: PathCounters,
}

impl PathValidator {
    /// Creates a validator with one validated path and no candidates.
    pub fn new(endpoint: Ssu2Endpoint, mtu: u16) -> Result<Self, PathError> {
        Ok(Self {
            validated: ValidatedPath::new(endpoint, mtu)?,
            candidates: Vec::new(),
            challenges_issued: 0,
            revalidation: None,
            revalidations_issued: 0,
            counters: PathCounters::default(),
        })
    }

    /// Returns the current validated path.
    pub const fn validated(&self) -> ValidatedPath {
        self.validated
    }

    /// Returns the effective validated-path MTU.
    pub const fn effective_mtu(&self) -> u16 {
        self.validated.mtu()
    }

    /// Returns the candidate-path MTU (always the SSU2 minimum).
    pub const fn candidate_mtu(&self) -> u16 {
        PATH_CANDIDATE_MTU
    }

    /// Returns the validated payload budget for an address family.
    pub fn validated_payload_bytes(&self, ipv6: bool) -> usize {
        self.validated.max_payload_bytes(ipv6)
    }

    /// Returns the candidate payload budget (minimum MTU).
    pub fn candidate_payload_bytes(ipv6: bool) -> usize {
        crate::session::SessionConfig::max_payload_for_mtu(PATH_CANDIDATE_MTU, ipv6)
    }

    /// Returns privacy-safe counters.
    pub const fn counters(&self) -> PathCounters {
        self.counters
    }

    /// Returns the number of tracked candidates.
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Returns whether an endpoint is the validated path or a tracked
    /// candidate.
    pub fn is_known(&self, endpoint: Ssu2Endpoint) -> bool {
        endpoint == self.validated.endpoint()
            || self
                .candidates
                .iter()
                .any(|entry| entry.endpoint == endpoint)
    }

    /// Returns the outstanding challenge for a tracked candidate.
    ///
    /// Scheduler/test introspection: the runtime never transmits a
    /// challenge except through [`PathEvent::ChallengeToSend`].
    pub fn challenge_for(&self, endpoint: Ssu2Endpoint) -> Option<[u8; PATH_CHALLENGE_LENGTH]> {
        self.candidates
            .iter()
            .find(|entry| entry.endpoint == endpoint)
            .map(|entry| entry.challenge)
    }

    /// Notes one authenticated packet source after the session accepted
    /// the datagram (replay filtering included).
    ///
    /// - validated source: no effect, normal traffic continues;
    /// - known candidate: no effect (one challenge per candidate; the
    ///   deadline in [`PathValidator::poll_expired`] bounds the wait);
    /// - new endpoint: bounded admission, then at most one
    ///   [`PathEvent::ChallengeToSend`].
    ///
    /// `challenge` must be fresh caller randomness (production: OS
    /// CSPRNG); all-zero values are rejected. The machine never
    /// migrates here — only [`PathValidator::on_path_response`] with a
    /// matching value promotes.
    pub fn note_authenticated_packet(
        &mut self,
        endpoint: Ssu2Endpoint,
        challenge: [u8; PATH_CHALLENGE_LENGTH],
        now_ms: u64,
    ) -> Result<Option<PathEvent>, PathError> {
        self.expire_locked(now_ms);
        if endpoint == self.validated.endpoint() {
            return Ok(None);
        }
        if self
            .candidates
            .iter()
            .any(|entry| entry.endpoint == endpoint)
        {
            return Ok(None);
        }
        if challenge == [0_u8; PATH_CHALLENGE_LENGTH] {
            return Err(PathError::InvalidChallenge);
        }
        if self.candidates.len() >= MAX_PATH_CANDIDATES {
            self.counters.denied_candidates = self.counters.denied_candidates.saturating_add(1);
            return Err(PathError::TooManyCandidates);
        }
        let family_count = self
            .candidates
            .iter()
            .filter(|entry| entry.endpoint.family() == endpoint.family())
            .count();
        if family_count >= MAX_CANDIDATES_PER_FAMILY {
            self.counters.denied_candidates = self.counters.denied_candidates.saturating_add(1);
            return Err(PathError::FamilyQuotaExceeded);
        }
        if self.challenges_issued >= MAX_PATH_CHALLENGES_PER_SESSION {
            self.counters.denied_candidates = self.counters.denied_candidates.saturating_add(1);
            return Err(PathError::ChallengeBudgetExhausted);
        }
        self.challenges_issued = self.challenges_issued.saturating_add(1);
        self.counters.challenges_issued = self.counters.challenges_issued.saturating_add(1);
        let deadline_ms = now_ms.saturating_add(PATH_VALIDATION_TIMEOUT_MS);
        self.candidates.push(PathCandidate {
            endpoint,
            challenge,
            deadline_ms,
        });
        Ok(Some(PathEvent::ChallengeToSend {
            endpoint,
            challenge,
        }))
    }

    /// Asks the path **in use** to prove itself again.
    ///
    /// The challenge names the validated endpoint, and only a response from
    /// that endpoint is accepted, so a re-validation can never move the path.
    /// A session with one already outstanding refuses a second, and the
    /// per-session ceiling bounds the whole probe sequence: the count is a
    /// ceiling, not a target.
    ///
    /// Returns the endpoint the challenge must be sent to.
    pub fn request_revalidation(
        &mut self,
        challenge: [u8; PATH_CHALLENGE_LENGTH],
        now_ms: u64,
    ) -> Result<Ssu2Endpoint, PathError> {
        if challenge == [0_u8; PATH_CHALLENGE_LENGTH] {
            return Err(PathError::InvalidChallenge);
        }
        if self
            .revalidation
            .is_some_and(|entry| !entry.expired(now_ms))
        {
            return Err(PathError::RevalidationInFlight);
        }
        if self.revalidations_issued >= MAX_PATH_REVALIDATIONS_PER_SESSION {
            return Err(PathError::ChallengeBudgetExhausted);
        }
        self.revalidations_issued = self.revalidations_issued.saturating_add(1);
        self.revalidation = Some(Revalidation {
            challenge,
            deadline_ms: now_ms.saturating_add(PATH_VALIDATION_TIMEOUT_MS),
        });
        Ok(self.validated.endpoint())
    }

    /// Whether a re-validation of the path in use is outstanding at `now_ms`.
    pub fn revalidation_in_flight(&self, now_ms: u64) -> bool {
        self.revalidation
            .is_some_and(|entry| !entry.expired(now_ms))
    }

    /// Handles one authenticated PathResponse from `endpoint`.
    ///
    /// Only an exact challenge match from the tracked candidate
    /// endpoint promotes. Wrong, stale, replayed, expired, or
    /// cross-family responses are rejected without migration; the
    /// candidate survives a wrong value until its deadline so the
    /// legitimate peer can retry.
    pub fn on_path_response(
        &mut self,
        endpoint: Ssu2Endpoint,
        data: &[u8],
        now_ms: u64,
    ) -> Result<PathEvent, PathError> {
        self.expire_locked(now_ms);
        // A re-validation answer comes from the endpoint already in use, which
        // is never a candidate. It is resolved first, and only an exact match
        // from that endpoint counts; anything else falls through to the
        // migration machine so a genuine candidate response is never eaten.
        if let Some(revalidation) = self.revalidation
            && endpoint == self.validated.endpoint()
            && data == revalidation.challenge
        {
            if revalidation.expired(now_ms) {
                self.revalidation = None;
                self.counters.expired_revalidations =
                    self.counters.expired_revalidations.saturating_add(1);
                return Err(PathError::ExpiredCandidate);
            }
            self.revalidation = None;
            self.counters.revalidations = self.counters.revalidations.saturating_add(1);
            return Ok(PathEvent::Revalidated { endpoint });
        }
        let position = self
            .candidates
            .iter()
            .position(|entry| entry.endpoint == endpoint)
            .ok_or(PathError::NotACandidate)?;
        let candidate = self.candidates[position];
        if candidate.expired(now_ms) {
            self.candidates.remove(position);
            self.counters.expired_candidates = self.counters.expired_candidates.saturating_add(1);
            return Err(PathError::ExpiredCandidate);
        }
        if data != candidate.challenge {
            self.counters.rejected_responses = self.counters.rejected_responses.saturating_add(1);
            return Err(PathError::ChallengeMismatch);
        }
        self.candidates.remove(position);
        let previous = self.validated.endpoint();
        // The validated MTU is path policy, not candidate fate: keep it.
        // The caller resets stale in-flight accounting (see
        // `Ssu2Session::note_path_migrated`) and keeps the validated
        // MTU for fragmentation on the new path.
        self.validated.endpoint = endpoint;
        self.counters.migrations = self.counters.migrations.saturating_add(1);
        Ok(PathEvent::Validated {
            previous,
            current: endpoint,
        })
    }

    /// Removes expired candidates, returning their endpoints for
    /// diagnostics. The validated path is unaffected: expiry retains
    /// (returns to) the old path.
    pub fn poll_expired(&mut self, now_ms: u64) -> Vec<Ssu2Endpoint> {
        if self.revalidation.is_some_and(|entry| entry.expired(now_ms)) {
            self.revalidation = None;
            self.counters.expired_revalidations =
                self.counters.expired_revalidations.saturating_add(1);
        }
        let expired: Vec<Ssu2Endpoint> = self
            .candidates
            .iter()
            .filter(|entry| entry.expired(now_ms))
            .map(|entry| entry.endpoint)
            .collect();
        self.expire_locked(now_ms);
        expired
    }

    /// Returns the earliest candidate deadline, if any, for the
    /// runtime central scheduler.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        let candidate = self.candidates.iter().map(|entry| entry.deadline_ms).min();
        let revalidation = self.revalidation.map(|entry| entry.deadline_ms);
        match (candidate, revalidation) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        }
    }

    fn expire_locked(&mut self, now_ms: u64) {
        let before = self.candidates.len();
        self.candidates.retain(|entry| !entry.expired(now_ms));
        let expired = before.saturating_sub(self.candidates.len());
        self.counters.expired_candidates = self
            .counters
            .expired_candidates
            .saturating_add(expired as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `core::net` path per the static boundary script: pure endpoint
    // literals for the validator, never sockets. The types are
    // identical; the `core` path marks the runtime-neutral crate
    // correctly.
    use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    fn v4(octet: u8, port: u16) -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, octet)), port).expect("endpoint")
    }

    fn v6(id: u16, port: u16) -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, id)), port)
            .expect("endpoint")
    }

    fn challenge(byte: u8) -> [u8; PATH_CHALLENGE_LENGTH] {
        [byte; PATH_CHALLENGE_LENGTH]
    }

    fn socket(endpoint: Ssu2Endpoint) -> SocketAddr {
        endpoint.socket_addr()
    }

    // Plan 549: the path in use can prove itself again, and doing so is
    // **not** a migration.

    #[test]
    fn revalidation_proves_the_path_in_use_without_migrating() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        let target = validator
            .request_revalidation(challenge(9), 1_000)
            .expect("request");
        assert_eq!(target, current);
        assert!(validator.revalidation_in_flight(1_000));
        let event = validator
            .on_path_response(current, &challenge(9), 1_100)
            .expect("response");
        assert_eq!(event, PathEvent::Revalidated { endpoint: current });
        // The path did not move, no candidate was consumed, and the event is
        // not the migration variant.
        assert_eq!(validator.validated().endpoint(), current);
        assert_eq!(validator.candidate_count(), 0);
        assert_eq!(validator.counters().migrations, 0);
        assert_eq!(validator.counters().revalidations, 1);
    }

    #[test]
    fn a_revalidation_second_request_is_refused_while_one_is_in_flight() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        validator
            .request_revalidation(challenge(3), 1_000)
            .expect("first");
        assert_eq!(
            validator.request_revalidation(challenge(4), 1_000),
            Err(PathError::RevalidationInFlight)
        );
    }

    #[test]
    fn a_revalidation_answer_from_another_endpoint_is_not_the_proof() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        validator
            .request_revalidation(challenge(5), 1_000)
            .expect("request");
        // A different endpoint that echoes the same bytes is not the path in
        // use, so it can never complete the re-validation.
        assert_eq!(
            validator.on_path_response(v4(2, 2000), &challenge(5), 1_100),
            Err(PathError::NotACandidate)
        );
        assert!(validator.revalidation_in_flight(1_100));
        assert_eq!(validator.counters().revalidations, 0);
    }

    #[test]
    fn a_revalidation_wrong_value_keeps_the_probe_open() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        validator
            .request_revalidation(challenge(6), 1_000)
            .expect("request");
        assert_eq!(
            validator.on_path_response(current, &challenge(7), 1_100),
            Err(PathError::NotACandidate)
        );
        assert!(validator.revalidation_in_flight(1_100));
        // The legitimate answer still completes it inside the window.
        assert_eq!(
            validator.on_path_response(current, &challenge(6), 1_200),
            Ok(PathEvent::Revalidated { endpoint: current })
        );
    }

    #[test]
    fn a_revalidation_late_answer_is_refused_and_counted() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        validator
            .request_revalidation(challenge(8), 1_000)
            .expect("request");
        let late = 1_000 + PATH_VALIDATION_TIMEOUT_MS + 1;
        assert_eq!(
            validator.on_path_response(current, &challenge(8), late),
            Err(PathError::ExpiredCandidate)
        );
        assert_eq!(validator.counters().revalidations, 0);
        assert_eq!(validator.counters().expired_revalidations, 1);
        assert!(!validator.revalidation_in_flight(late));
    }

    #[test]
    fn a_zero_revalidation_challenge_is_refused() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        assert_eq!(
            validator.request_revalidation(challenge(0), 1_000),
            Err(PathError::InvalidChallenge)
        );
    }

    #[test]
    fn the_revalidation_budget_is_a_ceiling() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        let mut now = 1_000;
        for _ in 0..MAX_PATH_REVALIDATIONS_PER_SESSION {
            validator
                .request_revalidation(challenge(1), now)
                .expect("request");
            validator
                .on_path_response(current, &challenge(1), now + 1)
                .expect("response");
            now += 100;
        }
        assert_eq!(
            validator.request_revalidation(challenge(1), now),
            Err(PathError::ChallengeBudgetExhausted)
        );
        assert_eq!(
            validator.counters().revalidations,
            u64::from(MAX_PATH_REVALIDATIONS_PER_SESSION)
        );
    }

    #[test]
    fn a_candidate_response_is_not_eaten_by_an_outstanding_revalidation() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        let candidate = v4(2, 2000);
        let candidate_challenge = validator
            .note_authenticated_packet(candidate, challenge(2), 0)
            .expect("note")
            .and_then(|event| match event {
                PathEvent::ChallengeToSend { challenge, .. } => Some(challenge),
                _ => None,
            })
            .expect("candidate challenge");
        validator
            .request_revalidation(challenge(9), 10)
            .expect("request");
        // A genuine candidate response still migrates while the re-validation
        // is outstanding.
        assert_eq!(
            validator.on_path_response(candidate, &candidate_challenge, 20),
            Ok(PathEvent::Validated {
                previous: current,
                current: candidate
            })
        );
        assert_eq!(validator.validated().endpoint(), candidate);
    }

    #[test]
    fn validated_source_packet_needs_no_challenge() {
        let current = v4(1, 1000);
        let mut validator = PathValidator::new(current, 1280).expect("validator");
        assert_eq!(
            validator
                .note_authenticated_packet(current, challenge(1), 0)
                .expect("note"),
            None
        );
        assert_eq!(validator.candidate_count(), 0);
        assert_eq!(validator.counters().challenges_issued, 0);
    }

    #[test]
    fn new_endpoint_creates_exactly_one_bounded_candidate() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        let candidate = v4(2, 2000);
        let event = validator
            .note_authenticated_packet(candidate, challenge(7), 0)
            .expect("note")
            .expect("challenge");
        assert_eq!(
            event,
            PathEvent::ChallengeToSend {
                endpoint: candidate,
                challenge: challenge(7),
            }
        );
        assert_eq!(validator.candidate_count(), 1);
        // A second authenticated packet from the same candidate does
        // not issue a second challenge.
        assert_eq!(
            validator
                .note_authenticated_packet(candidate, challenge(8), 1)
                .expect("note"),
            None
        );
        assert_eq!(validator.candidate_count(), 1);
        assert_eq!(validator.counters().challenges_issued, 1);
        let _ = socket(candidate);
    }

    #[test]
    fn wrong_response_does_not_migrate() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        let candidate = v4(2, 2000);
        validator
            .note_authenticated_packet(candidate, challenge(7), 0)
            .expect("note");
        assert_eq!(
            validator.on_path_response(candidate, &challenge(9), 1),
            Err(PathError::ChallengeMismatch)
        );
        assert_eq!(validator.validated().endpoint(), v4(1, 1000));
        assert_eq!(validator.counters().rejected_responses, 1);
        // The candidate survives a wrong value so the peer can retry.
        assert_eq!(validator.candidate_count(), 1);
    }

    #[test]
    fn correct_response_migrates_exactly_once() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        let candidate = v4(2, 2000);
        validator
            .note_authenticated_packet(candidate, challenge(7), 0)
            .expect("note");
        assert_eq!(
            validator
                .on_path_response(candidate, &challenge(7), 1)
                .expect("migrate"),
            PathEvent::Validated {
                previous: v4(1, 1000),
                current: candidate,
            }
        );
        assert_eq!(validator.validated().endpoint(), candidate);
        assert_eq!(validator.candidate_count(), 0);
        assert_eq!(validator.counters().migrations, 1);
        // The proof is consumed: replaying it is not a candidate.
        assert_eq!(
            validator.on_path_response(candidate, &challenge(7), 2),
            Err(PathError::NotACandidate)
        );
        assert_eq!(validator.counters().migrations, 1);
    }

    #[test]
    fn candidate_timeout_retains_old_path() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        let candidate = v4(2, 2000);
        validator
            .note_authenticated_packet(candidate, challenge(7), 0)
            .expect("note");
        let late = PATH_VALIDATION_TIMEOUT_MS + 1;
        assert_eq!(
            validator.on_path_response(candidate, &challenge(7), late),
            Err(PathError::NotACandidate)
        );
        assert_eq!(validator.validated().endpoint(), v4(1, 1000));
        assert_eq!(validator.candidate_count(), 0);
        assert_eq!(validator.counters().expired_candidates, 1);
    }

    #[test]
    fn spoofed_sources_hit_quotas_without_unbounded_state() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        // Fill the IPv4 family quota first.
        for octet in 2..=(2 + MAX_CANDIDATES_PER_FAMILY as u8) {
            let result = validator.note_authenticated_packet(v4(octet, 2000), challenge(octet), 0);
            if octet < 2 + MAX_CANDIDATES_PER_FAMILY as u8 {
                assert!(result.expect("note").is_some());
            } else {
                assert_eq!(result, Err(PathError::FamilyQuotaExceeded));
            }
        }
        // IPv6 still admits its own independent quota.
        assert!(
            validator
                .note_authenticated_packet(v6(1, 3000), challenge(0xA0), 0)
                .expect("v6")
                .is_some()
        );
        // The IPv4 family stays closed while IPv6 has room.
        assert_eq!(
            validator.note_authenticated_packet(v4(9, 2000), challenge(0xB0), 0),
            Err(PathError::FamilyQuotaExceeded)
        );
        assert!(
            validator
                .note_authenticated_packet(v6(2, 3000), challenge(0xA1), 0)
                .expect("v6")
                .is_some()
        );
        assert_eq!(validator.candidate_count(), MAX_PATH_CANDIDATES);
        // Both families are full now, so either hits the global ceiling.
        assert_eq!(
            validator.note_authenticated_packet(v6(3, 3000), challenge(0xA2), 0),
            Err(PathError::TooManyCandidates)
        );
        assert_eq!(
            validator.note_authenticated_packet(v4(10, 2000), challenge(0xB1), 0),
            Err(PathError::TooManyCandidates)
        );
        assert!(validator.counters().denied_candidates >= 2);
    }

    #[test]
    fn v4_packet_cannot_validate_v6_candidate_and_vice_versa() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        let candidate_v6 = v6(1, 3000);
        validator
            .note_authenticated_packet(candidate_v6, challenge(7), 0)
            .expect("note");
        // Same challenge bytes from a different endpoint prove nothing.
        assert_eq!(
            validator.on_path_response(v4(2, 2000), &challenge(7), 1),
            Err(PathError::NotACandidate)
        );
        assert_eq!(
            validator
                .on_path_response(candidate_v6, &challenge(7), 1)
                .expect("v6"),
            PathEvent::Validated {
                previous: v4(1, 1000),
                current: candidate_v6,
            }
        );
    }

    #[test]
    fn v6_structures_round_trip_and_family_mismatch_fails() {
        let endpoint = v6(0xAB, 4000);
        assert_eq!(endpoint.family(), AddressFamily::Ipv6);
        assert_eq!(
            Ssu2Endpoint::from_socket_addr(endpoint.socket_addr()).expect("round-trip"),
            endpoint
        );
        let mut validator = PathValidator::new(endpoint, 1280).expect("validator");
        assert_eq!(validator.validated().family(), AddressFamily::Ipv6);
        // A v4 response for the v6 candidate is a different endpoint.
        validator
            .note_authenticated_packet(v6(0xAC, 4000), challenge(3), 0)
            .expect("note");
        assert_eq!(
            validator.on_path_response(v4(1, 1000), &challenge(3), 1),
            Err(PathError::NotACandidate)
        );
    }

    #[test]
    fn candidate_mtu_stays_conservative_until_validation() {
        let validator = PathValidator::new(v4(1, 1000), 9000).expect("validator");
        assert_eq!(validator.effective_mtu(), 9000);
        assert_eq!(validator.candidate_mtu(), constants::SSU2_MIN_MTU);
        assert_eq!(
            PathValidator::candidate_payload_bytes(false),
            crate::session::SessionConfig::max_payload_for_mtu(1280, false)
        );
        assert!(
            PathValidator::candidate_payload_bytes(false)
                < validator.validated_payload_bytes(false)
        );
    }

    #[test]
    fn mtu_never_increases_from_packet_claims() {
        // The only MTU writer is the explicit configured/validated path;
        // packets have no setter. Out-of-range values fail closed.
        let validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        assert_eq!(
            ValidatedPath::new(v4(1, 1000), 1279),
            Err(PathError::InvalidMtu)
        );
        assert_eq!(
            ValidatedPath::new(v4(1, 1000), 9001),
            Err(PathError::InvalidMtu)
        );
        let raised = validator.validated().with_mtu(1500).expect("raise");
        assert_eq!(raised.mtu(), 1500);
        // Migration keeps validated MTU policy, not candidate fate.
        let mut moved_validator = validator;
        let candidate = v4(2, 2000);
        moved_validator
            .note_authenticated_packet(candidate, challenge(7), 0)
            .expect("note");
        moved_validator
            .on_path_response(candidate, &challenge(7), 1)
            .expect("migrate");
        assert_eq!(moved_validator.effective_mtu(), 1280);
    }

    #[test]
    fn all_zero_challenge_is_rejected() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        assert_eq!(
            validator.note_authenticated_packet(v4(2, 2000), [0_u8; PATH_CHALLENGE_LENGTH], 0),
            Err(PathError::InvalidChallenge)
        );
        assert_eq!(validator.candidate_count(), 0);
    }

    #[test]
    fn challenge_budget_bounds_spoof_floods() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        // Exhaust the budget by cycling candidates through expiry.
        let mut now = 0_u64;
        for round in 0..MAX_PATH_CHALLENGES_PER_SESSION {
            now += PATH_VALIDATION_TIMEOUT_MS + 1;
            let endpoint = v4(2, 2000u16 + u16::from(round as u8));
            let mut value = [0_u8; PATH_CHALLENGE_LENGTH];
            value[0] = round as u8 + 1;
            assert!(
                validator
                    .note_authenticated_packet(endpoint, value, now)
                    .expect("budget")
                    .is_some()
            );
        }
        now += PATH_VALIDATION_TIMEOUT_MS + 1;
        assert_eq!(
            validator.note_authenticated_packet(v4(9, 9999), challenge(0xFF), now),
            Err(PathError::ChallengeBudgetExhausted)
        );
    }

    #[test]
    fn scheduler_deadline_tracks_earliest_candidate() {
        let mut validator = PathValidator::new(v4(1, 1000), 1280).expect("validator");
        assert_eq!(validator.next_deadline_ms(), None);
        validator
            .note_authenticated_packet(v4(2, 2000), challenge(1), 100)
            .expect("note");
        validator
            .note_authenticated_packet(v4(3, 2000), challenge(2), 50)
            .expect("note");
        assert_eq!(
            validator.next_deadline_ms(),
            Some(50 + PATH_VALIDATION_TIMEOUT_MS)
        );
    }
}
