//! Lookup policy and bounded candidate selection.
//!
//! Plan 105 §2 defines a `LookupPolicy` object that bounds the active
//! query work and selects the eligible floodfill candidates from the
//! local store. The selector is deterministic for deterministic inputs;
//! the optional `Rng` lets an injector produce a deterministic shuffle
//! when a query needs to break symmetry between equal candidates.

use std::fmt;

use crate::router_info::RouterHash;
use crate::routing::{NearestSelection, xor_distance};
use crate::store::RouterInfoStore;

/// Loudest policy ceiling the lookup state machine accepts as a
/// safety net. Anything larger is treated as a programming error and
/// rejected by [`LookupPolicy::new`].
pub const MAX_SUGGESTED_HASH_LIMIT: usize = 256;
/// Default policy ceiling for the suggested-hash buffer. Matches the
/// current I2P Java FloodfillNetworkDatabaseSetttings `searchTimeout`
/// peer budget.
pub const DEFAULT_SUGGESTED_HASH_LIMIT: usize = 64;
/// Default per-query peer ceiling.
pub const DEFAULT_MAX_PEERS_PER_LOOKUP: usize = 8;
/// Default per-query total deadline in milliseconds.
pub const DEFAULT_TOTAL_DEADLINE_MS: u64 = 5_000;
/// Default per-attempt deadline in milliseconds.
pub const DEFAULT_PER_ATTEMPT_DEADLINE_MS: u64 = 1_500;
/// Default ceiling on total `DatabaseSearchReply` suggestions retained
/// across all responses inside one query.
pub const DEFAULT_MAX_SUGGESTED_HASHES: usize = 128;
/// Default bound on the candidate set the selector produces. The
/// value matches the maximum number of candidate floodfills the lookup
/// state machine will ever attempt to query.
pub const DEFAULT_MAX_CANDIDATES_CONSIDERED: usize = 32;

const _: () = {
    assert!(DEFAULT_MAX_PEERS_PER_LOOKUP <= DEFAULT_MAX_CANDIDATES_CONSIDERED);
    assert!(DEFAULT_SUGGESTED_HASH_LIMIT <= MAX_SUGGESTED_HASH_LIMIT);
};

/// Bounded policy object for iterative lookups and publication
/// selections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LookupPolicy {
    max_candidates_considered: usize,
    max_peers_per_lookup: usize,
    max_suggested_hashes: usize,
    suggested_hash_limit: usize,
    total_deadline_ms: u64,
    per_attempt_deadline_ms: u64,
}

impl Default for LookupPolicy {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_CANDIDATES_CONSIDERED,
            DEFAULT_MAX_PEERS_PER_LOOKUP,
            DEFAULT_MAX_SUGGESTED_HASHES,
            DEFAULT_SUGGESTED_HASH_LIMIT,
            DEFAULT_TOTAL_DEADLINE_MS,
            DEFAULT_PER_ATTEMPT_DEADLINE_MS,
        )
        .expect("default policy fits every bound")
    }
}

impl LookupPolicy {
    /// Constructs a policy after validating every bound.
    ///
    /// `total_deadline_ms` must be greater than or equal to
    /// `per_attempt_deadline_ms`. The candidate ceiling must be at
    /// least as large as the per-lookup peer ceiling. The suggested
    /// hash ceiling must be at least as large as the suggested hash
    /// per-response limit.
    pub const fn new(
        max_candidates_considered: usize,
        max_peers_per_lookup: usize,
        max_suggested_hashes: usize,
        suggested_hash_limit: usize,
        total_deadline_ms: u64,
        per_attempt_deadline_ms: u64,
    ) -> Result<Self, LookupPolicyError> {
        if max_candidates_considered == 0 {
            return Err(LookupPolicyError::ZeroCandidates);
        }
        if max_peers_per_lookup == 0 {
            return Err(LookupPolicyError::ZeroPeers);
        }
        if max_peers_per_lookup > max_candidates_considered {
            return Err(LookupPolicyError::PeerBudgetExceedsCandidates);
        }
        if max_suggested_hashes == 0 {
            return Err(LookupPolicyError::ZeroSuggestedHashes);
        }
        if suggested_hash_limit == 0 {
            return Err(LookupPolicyError::ZeroSuggestedHashLimit);
        }
        if suggested_hash_limit > MAX_SUGGESTED_HASH_LIMIT {
            return Err(LookupPolicyError::SuggestedHashLimitExceedsCeiling);
        }
        if suggested_hash_limit > max_suggested_hashes {
            return Err(LookupPolicyError::SuggestedHashLimitExceedsTotal);
        }
        if per_attempt_deadline_ms == 0 {
            return Err(LookupPolicyError::ZeroPerAttemptDeadline);
        }
        if total_deadline_ms < per_attempt_deadline_ms {
            return Err(LookupPolicyError::TotalDeadlineBelowPerAttempt);
        }
        Ok(Self {
            max_candidates_considered,
            max_peers_per_lookup,
            max_suggested_hashes,
            suggested_hash_limit,
            total_deadline_ms,
            per_attempt_deadline_ms,
        })
    }

    /// Maximum number of candidate floodfills the selector will
    /// produce.
    pub const fn max_candidates_considered(&self) -> usize {
        self.max_candidates_considered
    }

    /// Maximum number of peers the lookup will issue `DatabaseLookup`
    /// messages to.
    pub const fn max_peers_per_lookup(&self) -> usize {
        self.max_peers_per_lookup
    }

    /// Maximum number of search-reply suggestions retained across all
    /// responses for one query.
    pub const fn max_suggested_hashes(&self) -> usize {
        self.max_suggested_hashes
    }

    /// Maximum number of suggested hashes accepted per single response.
    pub const fn suggested_hash_limit(&self) -> usize {
        self.suggested_hash_limit
    }

    /// Lookup total deadline in milliseconds.
    pub const fn total_deadline_ms(&self) -> u64 {
        self.total_deadline_ms
    }

    /// Per-attempt deadline in milliseconds.
    pub const fn per_attempt_deadline_ms(&self) -> u64 {
        self.per_attempt_deadline_ms
    }
}

/// Validation failures for [`LookupPolicy`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupPolicyError {
    /// `max_candidates_considered` must be positive.
    ZeroCandidates,
    /// `max_peers_per_lookup` must be positive.
    ZeroPeers,
    /// `max_peers_per_lookup` cannot exceed `max_candidates_considered`.
    PeerBudgetExceedsCandidates,
    /// `max_suggested_hashes` must be positive.
    ZeroSuggestedHashes,
    /// `suggested_hash_limit` must be positive.
    ZeroSuggestedHashLimit,
    /// `suggested_hash_limit` cannot exceed the global
    /// [`MAX_SUGGESTED_HASH_LIMIT`].
    SuggestedHashLimitExceedsCeiling,
    /// `suggested_hash_limit` cannot exceed `max_suggested_hashes`.
    SuggestedHashLimitExceedsTotal,
    /// `per_attempt_deadline_ms` must be positive.
    ZeroPerAttemptDeadline,
    /// `total_deadline_ms` must be greater than or equal to
    /// `per_attempt_deadline_ms`.
    TotalDeadlineBelowPerAttempt,
}

impl fmt::Display for LookupPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCandidates => formatter.write_str("max_candidates_considered must be > 0"),
            Self::ZeroPeers => formatter.write_str("max_peers_per_lookup must be > 0"),
            Self::PeerBudgetExceedsCandidates => formatter
                .write_str("max_peers_per_lookup must not exceed max_candidates_considered"),
            Self::ZeroSuggestedHashes => formatter.write_str("max_suggested_hashes must be > 0"),
            Self::ZeroSuggestedHashLimit => formatter.write_str("suggested_hash_limit must be > 0"),
            Self::SuggestedHashLimitExceedsCeiling => {
                formatter.write_str("suggested_hash_limit exceeds the global per-response ceiling")
            }
            Self::SuggestedHashLimitExceedsTotal => {
                formatter.write_str("suggested_hash_limit must not exceed max_suggested_hashes")
            }
            Self::ZeroPerAttemptDeadline => {
                formatter.write_str("per_attempt_deadline_ms must be > 0")
            }
            Self::TotalDeadlineBelowPerAttempt => {
                formatter.write_str("total_deadline_ms must be >= per_attempt_deadline_ms")
            }
        }
    }
}

impl std::error::Error for LookupPolicyError {}

/// A bounded selection of candidate floodfills for a lookup.
pub struct FloodfillSelection {
    inner: Vec<NearestSelection>,
}

impl FloodfillSelection {
    /// Returns the selected candidates in deterministic nearest order.
    pub fn entries(&self) -> &[NearestSelection] {
        &self.inner
    }

    /// Returns the first candidate RouterHash, if any.
    pub fn first(&self) -> Option<RouterHash> {
        self.inner.first().map(|entry| entry.key)
    }

    /// Returns the number of candidates.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns whether the selection is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the RouterHashes of the selection in order.
    pub fn hashes(&self) -> Vec<RouterHash> {
        self.inner.iter().map(|entry| entry.key).collect()
    }
}

impl fmt::Debug for FloodfillSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FloodfillSelection")
            .field("count", &self.inner.len())
            .finish()
    }
}

/// Selects the eligible floodfill candidates for a lookup.
///
/// The selector is purely deterministic for the supplied `policy` and
/// `target`. It filters out records whose `RouterInfo` is not
/// currently valid, records that do not advertise the floodfill
/// capability, and records whose RouterHash is in `excluded_hashes`.
/// The remaining records are sorted by XOR distance with the supplied
/// `routing_key` (typically the daily routing key) and the
/// `policy.max_candidates_considered` closest entries are returned.
pub fn select_floodfill_candidates(
    store: &RouterInfoStore,
    target: &RouterHash,
    routing_key: &RouterHash,
    excluded_hashes: &[RouterHash],
    policy: &LookupPolicy,
) -> FloodfillSelection {
    let mut excluded: std::collections::BTreeSet<RouterHash> =
        excluded_hashes.iter().copied().collect();
    excluded.insert(*target);
    let mut candidates: Vec<NearestSelection> = Vec::new();
    for record in store.floodfill_advertisers() {
        let key = record.key();
        if excluded.contains(&key) {
            continue;
        }
        candidates.push(NearestSelection {
            distance: xor_distance(&key, routing_key),
            key,
        });
    }
    candidates.sort();
    candidates.truncate(policy.max_candidates_considered());
    FloodfillSelection { inner: candidates }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::{ValidatedRouterInfo, ValidationContext, router_hash};
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::Date;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(b: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = b
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                i2pr_proto::Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn floodfill(b: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let mut options = i2pr_proto::Mapping::builder();
        options.insert("caps".to_owned(), "f".to_owned()).unwrap();
        let info = b
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    #[test]
    fn default_policy_is_valid() {
        let policy = LookupPolicy::default();
        assert!(policy.max_candidates_considered() > 0);
        assert!(policy.max_peers_per_lookup() > 0);
        assert!(policy.total_deadline_ms() >= policy.per_attempt_deadline_ms());
    }

    #[test]
    fn policy_rejects_zero_candidates() {
        let error = LookupPolicy::new(0, 1, 1, 1, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::ZeroCandidates);
    }

    #[test]
    fn policy_rejects_zero_peers_per_lookup() {
        let error = LookupPolicy::new(2, 0, 1, 1, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::ZeroPeers);
    }

    #[test]
    fn policy_rejects_peer_budget_above_candidate_ceiling() {
        let error = LookupPolicy::new(2, 3, 1, 1, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::PeerBudgetExceedsCandidates);
    }

    #[test]
    fn policy_rejects_zero_suggested_total() {
        let error = LookupPolicy::new(2, 1, 0, 1, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::ZeroSuggestedHashes);
    }

    #[test]
    fn policy_rejects_zero_suggested_limit() {
        let error = LookupPolicy::new(2, 1, 1, 0, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::ZeroSuggestedHashLimit);
    }

    #[test]
    fn policy_rejects_suggested_limit_above_global_ceiling() {
        let total = MAX_SUGGESTED_HASH_LIMIT + 1;
        let error = LookupPolicy::new(2, 1, total, total, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::SuggestedHashLimitExceedsCeiling);
    }

    #[test]
    fn policy_rejects_suggested_limit_above_total() {
        let error = LookupPolicy::new(2, 1, 4, 5, 1000, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::SuggestedHashLimitExceedsTotal);
    }

    #[test]
    fn policy_rejects_zero_per_attempt_deadline() {
        let error = LookupPolicy::new(2, 1, 1, 1, 1000, 0).unwrap_err();
        assert_eq!(error, LookupPolicyError::ZeroPerAttemptDeadline);
    }

    #[test]
    fn policy_rejects_total_below_per_attempt() {
        let error = LookupPolicy::new(2, 1, 1, 1, 100, 1000).unwrap_err();
        assert_eq!(error, LookupPolicyError::TotalDeadlineBelowPerAttempt);
    }

    #[test]
    fn selector_filters_non_floodfill_records() {
        let floodfill_signer = bundle(0x501);
        let plain = bundle(0x502);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill(&floodfill_signer, 1));
        store.insert(validate(&plain, 1));
        let target = RouterHash::from_bytes([0x55u8; 32]);
        let routing_key = RouterHash::from_bytes([0x55u8; 32]);
        let policy = LookupPolicy::default();
        let selection = select_floodfill_candidates(&store, &target, &routing_key, &[], &policy);
        assert_eq!(selection.len(), 1);
        assert_eq!(
            selection.first().unwrap(),
            router_hash(floodfill_signer.identity()).unwrap()
        );
    }

    #[test]
    fn selector_respects_candidate_ceiling() {
        let policy = LookupPolicy::new(1, 1, 1, 1, 1000, 500).expect("policy");
        let signer_a = bundle(0x510);
        let signer_b = bundle(0x511);
        let signer_c = bundle(0x512);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill(&signer_a, 1));
        store.insert(floodfill(&signer_b, 1));
        store.insert(floodfill(&signer_c, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let selection = select_floodfill_candidates(&store, &target, &routing_key, &[], &policy);
        assert_eq!(selection.len(), 1);
    }

    #[test]
    fn selector_excludes_target_and_supplied_hashes() {
        let signer_a = bundle(0x520);
        let signer_b = bundle(0x521);
        let signer_c = bundle(0x522);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill(&signer_a, 1));
        store.insert(floodfill(&signer_b, 1));
        store.insert(floodfill(&signer_c, 1));
        let target = router_hash(signer_a.identity()).unwrap();
        let excluded = [router_hash(signer_b.identity()).unwrap()];
        let routing_key = router_hash(signer_c.identity()).unwrap();
        let policy = LookupPolicy::new(4, 1, 1, 1, 1000, 500).expect("policy");
        let selection =
            select_floodfill_candidates(&store, &target, &routing_key, &excluded, &policy);
        let hashes = selection.hashes();
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0], router_hash(signer_c.identity()).unwrap());
    }

    #[test]
    fn selector_orders_by_routing_key_distance_not_target() {
        // The selector uses the routing key, not the raw target, to
        // anchor the selection. We verify that the closest-to-key
        // candidates appear first.
        let signer_far = bundle(0x530);
        let signer_near = bundle(0x531);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill(&signer_far, 1));
        store.insert(floodfill(&signer_near, 1));
        let target = RouterHash::from_bytes([0x00u8; 32]);
        let near_key = router_hash(signer_near.identity()).unwrap();
        let routing_key = near_key;
        let policy = LookupPolicy::new(4, 1, 1, 1, 1000, 500).expect("policy");
        let selection = select_floodfill_candidates(&store, &target, &routing_key, &[], &policy);
        let hashes = selection.hashes();
        assert_eq!(hashes[0], near_key);
    }
}
