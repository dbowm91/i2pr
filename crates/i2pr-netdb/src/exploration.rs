//! Plan 489 §6: the exploration job.
//!
//! An *exploratory* `DatabaseLookup` asks a peer for the peers closest
//! to a random search key's position in the day's keyspace. The answer
//! is a `DatabaseSearchReply` naming peers — never a stored record —
//! and those peers are what feeds the routing table.
//!
//! The job is a bounded accounting unit, not a scheduler:
//!
//! - a **peer budget** caps how many peers one exploration may query;
//! - a **deadline** caps how long one exploration may live;
//! - an **outstanding table** records the requests in flight, so a
//!   late reply can be matched to its request and a dropped request
//!   releases its slot;
//! - **cancelling** releases every outstanding slot and consumes the
//!   budget — a cancelled exploration never refunds a query.
//!
//! The job is runtime-neutral: it reads no clock. The caller supplies
//! the time on every call.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::router_info::RouterHash;

/// The default number of peers one exploration may query.
pub const DEFAULT_EXPLORATION_MAX_PEERS: usize = 8;

/// The default lifetime of one exploration, in milliseconds.
pub const DEFAULT_EXPLORATION_DEADLINE_MS: u64 = 30_000;

/// The largest legal per-exploration peer budget.
pub const MAX_EXPLORATION_PEERS_LIMIT: usize = 64;

/// The largest legal per-exploration deadline, in milliseconds.
pub const MAX_EXPLORATION_DEADLINE_MS_LIMIT: u64 = 3_600_000;

/// The smallest legal per-exploration deadline, in milliseconds.
pub const MIN_EXPLORATION_DEADLINE_MS: u64 = 1_000;

/// The largest number of suggestions one reply may contribute.
pub const MAX_EXPLORATION_SUGGESTIONS: usize = 64;

/// The largest number of requests one exploration may have in flight.
pub const MAX_EXPLORATION_OUTSTANDING: usize = 16;

/// The bounded budget of one exploration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplorationBudget {
    /// The maximum number of peers one exploration may query.
    pub max_peers: usize,
    /// The maximum lifetime of one exploration, in milliseconds.
    pub deadline_ms: u64,
}

impl Default for ExplorationBudget {
    fn default() -> Self {
        Self {
            max_peers: DEFAULT_EXPLORATION_MAX_PEERS,
            deadline_ms: DEFAULT_EXPLORATION_DEADLINE_MS,
        }
    }
}

/// The typed refusal of an invalid exploration budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplorationBudgetError {
    /// The offending field name.
    pub field: &'static str,
    /// The supplied value.
    pub value: u64,
    /// The inclusive lower bound, when there is one.
    pub minimum: Option<u64>,
    /// The inclusive upper bound, when there is one.
    pub maximum: Option<u64>,
}

impl fmt::Display for ExplorationBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "exploration {} = {} is out of range",
            self.field, self.value
        )?;
        if let Some(minimum) = self.minimum {
            write!(formatter, " (minimum {minimum})")?;
        }
        if let Some(maximum) = self.maximum {
            write!(formatter, " (maximum {maximum})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ExplorationBudgetError {}

impl ExplorationBudget {
    /// Validates the budget.
    pub fn validate(self) -> Result<Self, ExplorationBudgetError> {
        if self.max_peers == 0 {
            return Err(ExplorationBudgetError {
                field: "max_peers",
                value: 0,
                minimum: Some(1),
                maximum: Some(MAX_EXPLORATION_PEERS_LIMIT as u64),
            });
        }
        if self.max_peers > MAX_EXPLORATION_PEERS_LIMIT {
            return Err(ExplorationBudgetError {
                field: "max_peers",
                value: self.max_peers as u64,
                minimum: Some(1),
                maximum: Some(MAX_EXPLORATION_PEERS_LIMIT as u64),
            });
        }
        if self.deadline_ms < MIN_EXPLORATION_DEADLINE_MS {
            return Err(ExplorationBudgetError {
                field: "deadline_ms",
                value: self.deadline_ms,
                minimum: Some(MIN_EXPLORATION_DEADLINE_MS),
                maximum: Some(MAX_EXPLORATION_DEADLINE_MS_LIMIT),
            });
        }
        if self.deadline_ms > MAX_EXPLORATION_DEADLINE_MS_LIMIT {
            return Err(ExplorationBudgetError {
                field: "deadline_ms",
                value: self.deadline_ms,
                minimum: Some(MIN_EXPLORATION_DEADLINE_MS),
                maximum: Some(MAX_EXPLORATION_DEADLINE_MS_LIMIT),
            });
        }
        Ok(self)
    }
}

/// The bounded policy of one exploration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplorationPolicy {
    /// The bounded budget.
    pub budget: ExplorationBudget,
    /// The maximum number of suggestions one reply may contribute.
    pub max_suggestions: usize,
}

impl Default for ExplorationPolicy {
    fn default() -> Self {
        Self {
            budget: ExplorationBudget::default(),
            max_suggestions: MAX_EXPLORATION_SUGGESTIONS,
        }
    }
}

/// Why an exploration stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorationStop {
    /// The peer budget is exhausted.
    BudgetExhausted,
    /// The deadline elapsed.
    DeadlineElapsed,
    /// The caller cancelled the exploration.
    Cancelled,
    /// The exploration completed.
    Completed,
}

impl ExplorationStop {
    /// Returns the stable label used in receipts.
    pub const fn label(self) -> &'static str {
        match self {
            Self::BudgetExhausted => "budget_exhausted",
            Self::DeadlineElapsed => "deadline_elapsed",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
        }
    }
}

/// The aggregate state of one exploration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplorationReport {
    /// The number of peers the exploration queried.
    pub peers_queried: usize,
    /// The number of suggestions the exploration accepted.
    pub suggestions_received: usize,
    /// The number of suggestions the exploration discarded.
    pub suggestions_dropped: usize,
    /// The number of requests still in flight.
    pub outstanding: usize,
    /// The number of peers the exploration budget allows.
    pub max_peers: usize,
    /// The exploration's lifetime budget, in milliseconds.
    pub deadline_ms: u64,
    /// The number of suggestions the policy accepts per reply.
    pub max_suggestions: usize,
    /// `true` when the exploration was cancelled.
    pub cancelled: bool,
}

/// The outcome of ingesting one exploration reply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorationReplyOutcome {
    /// The suggestions the exploration accepted.
    pub accepted: Vec<RouterHash>,
    /// The number of suggestions the exploration discarded.
    pub dropped: usize,
    /// `true` when the reply named a peer the exploration never asked.
    pub unknown_peer: bool,
}

/// One bounded exploration.
#[derive(Clone, Debug)]
pub struct ExplorationJob {
    /// The random search key the request carries.
    search_key: RouterHash,
    /// The daily routing key of the search key.
    routing_key: RouterHash,
    /// The bounded policy.
    policy: ExplorationPolicy,
    /// The moment the exploration began, in milliseconds.
    started_ms: u64,
    /// The moment the exploration expires, in milliseconds.
    expires_ms: u64,
    /// The peers the exploration has already queried.
    queried: BTreeSet<RouterHash>,
    /// The requests in flight: peer -> the moment it was issued.
    outstanding: BTreeMap<RouterHash, u64>,
    /// The number of suggestions the exploration accepted.
    suggestions_received: usize,
    /// The number of suggestions the exploration discarded.
    suggestions_dropped: usize,
    /// `true` when the exploration was cancelled.
    cancelled: bool,
    /// `true` when the exploration completed.
    finished: bool,
}

impl ExplorationJob {
    /// Begins an exploration for the supplied random search key.
    ///
    /// The routing key is the daily routing key of the search key; the
    /// caller derives it with the same
    /// [`daily_routing_key`](crate::routing::daily_routing_key) the
    /// lookup path uses.
    pub fn begin(
        search_key: RouterHash,
        routing_key: RouterHash,
        policy: ExplorationPolicy,
        now_ms: u64,
    ) -> Self {
        let expires_ms = now_ms.saturating_add(policy.budget.deadline_ms);
        Self {
            search_key,
            routing_key,
            policy,
            started_ms: now_ms,
            expires_ms,
            queried: BTreeSet::new(),
            outstanding: BTreeMap::new(),
            suggestions_received: 0,
            suggestions_dropped: 0,
            cancelled: false,
            finished: false,
        }
    }

    /// Returns the random search key the request carries.
    pub fn search_key(&self) -> RouterHash {
        self.search_key
    }

    /// Returns the daily routing key of the search key.
    pub fn routing_key(&self) -> RouterHash {
        self.routing_key
    }

    /// Returns the moment the exploration began, in milliseconds.
    pub fn started_ms(&self) -> u64 {
        self.started_ms
    }

    /// Returns the moment the exploration expires, in milliseconds.
    pub fn expires_ms(&self) -> u64 {
        self.expires_ms
    }

    /// Returns `true` when the deadline elapsed at `now_ms`.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_ms
    }

    /// Returns the number of peers the exploration has queried.
    pub fn peers_queried(&self) -> usize {
        self.queried.len()
    }

    /// Returns the number of requests in flight.
    pub fn outstanding_len(&self) -> usize {
        self.outstanding.len()
    }

    /// Returns `true` when the peer budget is exhausted.
    pub fn budget_exhausted(&self) -> bool {
        self.queried.len() >= self.policy.budget.max_peers
    }

    /// Records that a request was issued to a peer.
    ///
    /// The peer counts against the budget and occupies one outstanding
    /// slot. Recording the same peer twice is refused, so a request is
    /// never issued twice.
    pub fn record_issued(&mut self, peer: &RouterHash) -> bool {
        if self.finished || self.cancelled {
            return false;
        }
        if self.outstanding.len() >= MAX_EXPLORATION_OUTSTANDING {
            return false;
        }
        if !self.queried.insert(*peer) {
            return false;
        }
        self.outstanding.insert(*peer, 0);
        true
    }

    /// Returns the next peer to query.
    ///
    /// The candidate list is walked in the order the caller supplied —
    /// the routing table's closest-first order — and the first peer
    /// that was neither queried nor issued is chosen. A peer already in
    /// flight is not issued twice.
    ///
    /// Returns `None` when the budget is exhausted, when the
    /// exploration is finished or cancelled, or when the candidate list
    /// holds no eligible peer.
    pub fn next_candidate(&mut self, candidates: &[RouterHash]) -> Option<RouterHash> {
        if self.finished || self.cancelled || self.budget_exhausted() {
            return None;
        }
        for &candidate in candidates {
            if self.queried.contains(&candidate) || self.outstanding.contains_key(&candidate) {
                continue;
            }
            if self.outstanding.len() >= MAX_EXPLORATION_OUTSTANDING {
                return None;
            }
            self.queried.insert(candidate);
            self.outstanding.insert(candidate, 0);
            return Some(candidate);
        }
        None
    }

    /// Ingests one exploration reply.
    ///
    /// The reply's suggestions are accepted up to the policy's
    /// per-reply ceiling; the rest are counted as dropped. A reply
    /// that names a peer the exploration never asked is still counted,
    /// but flagged as `unknown_peer` so the caller can refuse it.
    pub fn record_reply(
        &mut self,
        peer: &RouterHash,
        suggested: &[RouterHash],
    ) -> ExplorationReplyOutcome {
        let unknown_peer = self.outstanding.remove(peer).is_none();
        let ceiling = self.policy.max_suggestions.min(MAX_EXPLORATION_SUGGESTIONS);
        let mut accepted = Vec::new();
        let mut dropped = 0usize;
        for &candidate in suggested {
            if accepted.len() < ceiling {
                accepted.push(candidate);
            } else {
                dropped += 1;
            }
        }
        self.suggestions_received += accepted.len();
        self.suggestions_dropped += dropped;
        ExplorationReplyOutcome {
            accepted,
            dropped,
            unknown_peer,
        }
    }

    /// Records that a request went unanswered.
    ///
    /// The outstanding slot is released. The query still counts against
    /// the budget: a peer that did not answer is a fact about the
    /// exploration, not a free retry.
    pub fn record_timeout(&mut self, peer: &RouterHash) -> bool {
        self.outstanding.remove(peer).is_some()
    }

    /// Cancels the exploration.
    ///
    /// Every outstanding slot is released and the budget stays spent.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.finished = true;
        self.outstanding.clear();
    }

    /// Marks the exploration complete.
    pub fn complete(&mut self) {
        self.finished = true;
        self.outstanding.clear();
    }

    /// Returns why the exploration stopped, if it has.
    ///
    /// The reason takes precedence over the plain "completed" state: an
    /// exploration that ran out of budget stopped *because* of the
    /// budget, and the receipt must say so.
    pub fn stop_reason(&self, now_ms: u64) -> Option<ExplorationStop> {
        if self.cancelled {
            return Some(ExplorationStop::Cancelled);
        }
        if self.budget_exhausted() {
            return Some(ExplorationStop::BudgetExhausted);
        }
        if self.is_expired(now_ms) {
            return Some(ExplorationStop::DeadlineElapsed);
        }
        if self.finished {
            return Some(ExplorationStop::Completed);
        }
        None
    }

    /// Returns the aggregate state of the exploration.
    pub fn report(&self) -> ExplorationReport {
        ExplorationReport {
            peers_queried: self.queried.len(),
            suggestions_received: self.suggestions_received,
            suggestions_dropped: self.suggestions_dropped,
            outstanding: self.outstanding.len(),
            max_peers: self.policy.budget.max_peers,
            deadline_ms: self.policy.budget.deadline_ms,
            max_suggestions: self.policy.max_suggestions,
            cancelled: self.cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(byte: u8) -> RouterHash {
        RouterHash::from_bytes([byte; 32])
    }

    fn job(max_peers: usize) -> ExplorationJob {
        ExplorationJob::begin(
            peer(1),
            peer(2),
            ExplorationPolicy {
                budget: ExplorationBudget {
                    max_peers,
                    deadline_ms: 10_000,
                },
                max_suggestions: 4,
            },
            1_000,
        )
    }

    #[test]
    fn an_exploration_issues_a_candidate_once() {
        let mut job = job(4);
        let candidates = [peer(10), peer(11), peer(12)];
        assert_eq!(job.next_candidate(&candidates), Some(peer(10)));
        assert_eq!(job.next_candidate(&candidates), Some(peer(11)));
        assert_eq!(job.next_candidate(&candidates), Some(peer(12)));
        assert_eq!(job.next_candidate(&candidates), None);
        assert_eq!(job.peers_queried(), 3);
    }

    #[test]
    fn the_peer_budget_caps_the_exploration() {
        let mut job = job(2);
        let candidates = [peer(10), peer(11), peer(12)];
        assert_eq!(job.next_candidate(&candidates), Some(peer(10)));
        assert_eq!(job.next_candidate(&candidates), Some(peer(11)));
        assert_eq!(job.next_candidate(&candidates), None);
        assert!(job.budget_exhausted());
        assert_eq!(
            job.stop_reason(1_000),
            Some(ExplorationStop::BudgetExhausted)
        );
    }

    #[test]
    fn the_outstanding_table_is_bounded() {
        let mut job = job(64);
        let candidates: Vec<RouterHash> = (0..MAX_EXPLORATION_OUTSTANDING as u8 + 4)
            .map(|index| peer(20 + index))
            .collect();
        for _ in 0..MAX_EXPLORATION_OUTSTANDING {
            assert!(job.next_candidate(&candidates).is_some());
        }
        assert_eq!(job.outstanding_len(), MAX_EXPLORATION_OUTSTANDING);
        assert_eq!(job.next_candidate(&candidates), None);
    }

    #[test]
    fn a_reply_releases_the_outstanding_slot() {
        let mut job = job(4);
        let candidates = [peer(10)];
        assert_eq!(job.next_candidate(&candidates), Some(peer(10)));
        assert_eq!(job.outstanding_len(), 1);
        let outcome = job.record_reply(&peer(10), &[peer(30), peer(31)]);
        assert_eq!(outcome.accepted, vec![peer(30), peer(31)]);
        assert!(!outcome.unknown_peer);
        assert_eq!(job.outstanding_len(), 0);
    }

    #[test]
    fn a_reply_from_an_unknown_peer_is_flagged() {
        let mut job = job(4);
        let outcome = job.record_reply(&peer(99), &[]);
        assert!(outcome.unknown_peer);
    }

    #[test]
    fn suggestions_are_capped_and_the_rest_are_counted() {
        let mut job = job(4);
        let suggested: Vec<RouterHash> = (0..10).map(|index| peer(40 + index as u8)).collect();
        let outcome = job.record_reply(&peer(99), &suggested);
        assert_eq!(outcome.accepted.len(), 4);
        assert_eq!(outcome.dropped, 6);
        assert_eq!(job.report().suggestions_received, 4);
        assert_eq!(job.report().suggestions_dropped, 6);
    }

    #[test]
    fn a_timeout_releases_the_slot_but_keeps_the_budget_spent() {
        let mut job = job(4);
        let candidates = [peer(10), peer(11)];
        assert_eq!(job.next_candidate(&candidates), Some(peer(10)));
        assert!(job.record_timeout(&peer(10)));
        assert_eq!(job.outstanding_len(), 0);
        assert_eq!(job.peers_queried(), 1);
        assert_eq!(job.next_candidate(&candidates), Some(peer(11)));
    }

    #[test]
    fn cancelling_releases_every_slot_and_stops_the_job() {
        let mut job = job(8);
        let candidates = [peer(10), peer(11), peer(12)];
        job.next_candidate(&candidates);
        job.next_candidate(&candidates);
        assert_eq!(job.outstanding_len(), 2);
        job.cancel();
        assert_eq!(job.outstanding_len(), 0);
        assert_eq!(job.next_candidate(&candidates), None);
        assert_eq!(job.stop_reason(1_000), Some(ExplorationStop::Cancelled));
        assert!(job.report().cancelled);
    }

    #[test]
    fn the_deadline_ends_the_exploration() {
        let job = job(8);
        assert!(!job.is_expired(1_000));
        assert!(job.is_expired(11_000));
        assert_eq!(
            job.stop_reason(11_000),
            Some(ExplorationStop::DeadlineElapsed)
        );
    }

    #[test]
    fn an_invalid_budget_is_refused() {
        assert!(
            ExplorationBudget {
                max_peers: 0,
                deadline_ms: 10_000,
            }
            .validate()
            .is_err()
        );
        assert!(
            ExplorationBudget {
                max_peers: 1,
                deadline_ms: 10,
            }
            .validate()
            .is_err()
        );
        assert!(
            ExplorationBudget {
                max_peers: MAX_EXPLORATION_PEERS_LIMIT + 1,
                deadline_ms: 10_000,
            }
            .validate()
            .is_err()
        );
        assert!(
            ExplorationBudget {
                max_peers: 1,
                deadline_ms: 10_000,
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn the_exploration_releases_every_drop_path() {
        // Path one: a reply releases the outstanding slot.
        let mut job = job(8);
        let candidates = [peer(50), peer(51), peer(52)];
        job.next_candidate(&candidates);
        assert_eq!(job.outstanding_len(), 1);
        job.record_reply(&peer(50), &[peer(60)]);
        assert_eq!(job.outstanding_len(), 0);

        // Path two: a timeout releases the outstanding slot.
        job.next_candidate(&candidates);
        assert_eq!(job.outstanding_len(), 1);
        assert!(job.record_timeout(&peer(51)));
        assert_eq!(job.outstanding_len(), 0);

        // Path three: cancelling releases every outstanding slot.
        job.next_candidate(&candidates);
        assert_eq!(job.outstanding_len(), 1);
        job.cancel();
        assert_eq!(job.outstanding_len(), 0);
        assert_eq!(job.stop_reason(1_000), Some(ExplorationStop::Cancelled));
    }

    #[test]
    fn a_peer_is_never_issued_twice() {
        let mut job = job(8);
        let candidates = [peer(70)];
        assert_eq!(job.next_candidate(&candidates), Some(peer(70)));
        assert_eq!(job.next_candidate(&candidates), None);
        assert!(
            !job.record_issued(&peer(70)),
            "an already-issued peer must not be issued again"
        );
    }

    #[test]
    fn the_outstanding_ceiling_is_enforced() {
        const _: () = assert!(MAX_EXPLORATION_OUTSTANDING <= MAX_EXPLORATION_PEERS_LIMIT);
        const _: () = assert!(MAX_EXPLORATION_SUGGESTIONS <= MAX_EXPLORATION_PEERS_LIMIT);
    }
}
