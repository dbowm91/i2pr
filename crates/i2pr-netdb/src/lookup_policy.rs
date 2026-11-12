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
use crate::version::{VersionRefusalCounts, dsm_target_eligibility, published_version_of};

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
    let mut excluded: Vec<RouterHash> = Vec::with_capacity(excluded_hashes.len() + 1);
    excluded.extend_from_slice(excluded_hashes);
    excluded.push(*target);
    FloodfillSelection {
        inner: nearest_floodfill_candidates(store, routing_key, &excluded, policy),
    }
}

/// Computes the nearest floodfill set for a routing key.
///
/// The helper is the shared ordering core: it excludes the supplied
/// hashes, sorts the remaining floodfill advertisers by XOR distance
/// with `routing_key` (RouterHash ascending is the deterministic
/// tie-breaker), and truncates to `policy.max_candidates_considered`.
///
/// The exclusion set is the caller's: a caller that must also exclude a
/// target adds it to `excluded`.
fn nearest_floodfill_candidates(
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
    policy: &LookupPolicy,
) -> Vec<NearestSelection> {
    let excluded: std::collections::BTreeSet<RouterHash> = excluded.iter().copied().collect();
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
    candidates
}

/// Computes the nearest **peers** for a routing key, whatever they advertise.
///
/// Plan 545. [`nearest_floodfill_candidates`] is floodfill-only because a
/// lookup must ask a floodfill. A dial does not: any version-eligible peer can
/// carry a session, and a private-LAN lab has no floodfill at all, because
/// floodfill eligibility requires a publicly routable address
/// (`crates/i2pr-daemon/src/floodfill.rs`, `is_publicly_routable_host`).
///
/// The ordering, the exclusion set and the policy ceiling are the same as the
/// floodfill form, so a caller that prefers floodfills and falls back to this
/// gets one order, not two.
fn nearest_peer_candidates(
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
    policy: &LookupPolicy,
) -> Vec<NearestSelection> {
    let excluded: std::collections::BTreeSet<RouterHash> = excluded.iter().copied().collect();
    let mut candidates: Vec<NearestSelection> = Vec::new();
    for (key, _) in store.iter() {
        if excluded.contains(key) {
            continue;
        }
        candidates.push(NearestSelection {
            distance: xor_distance(key, routing_key),
            key: *key,
        });
    }
    candidates.sort();
    candidates.truncate(policy.max_candidates_considered());
    candidates
}

/// The result of a version-gated floodfill selection.
///
/// The eligible targets, the counted refusals, and the number of
/// candidates the gate examined. No peer identity is retained beyond
/// the target set itself: the refusal counts name no peer, and no
/// address, key, or payload is kept.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VersionGatedFloodfillSelection {
    targets: Vec<RouterHash>,
    refusals: VersionRefusalCounts,
    examined: u32,
}

impl VersionGatedFloodfillSelection {
    /// The eligible floodfill targets, in nearest order.
    pub fn targets(&self) -> &[RouterHash] {
        &self.targets
    }

    /// The counted API-version refusals, by reason.
    pub const fn refusals(&self) -> &VersionRefusalCounts {
        &self.refusals
    }

    /// The number of candidates the gate examined.
    ///
    /// The gate examined exactly the candidates whose stored record it
    /// could read; see [`select_version_gated_floodfills`] for the
    /// identity this count satisfies.
    pub const fn examined(&self) -> u32 {
        self.examined
    }

    /// The number of accepted targets.
    pub fn accepted(&self) -> u32 {
        u32::try_from(self.targets.len()).unwrap_or(u32::MAX)
    }

    /// Whether no eligible target remains.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// The number of eligible targets.
    pub fn len(&self) -> usize {
        self.targets.len()
    }
}

/// Selects the nearest floodfills whose published API version satisfies the
/// DSM floor, counting every refusal by reason.
///
/// Fail closed: a floodfill that publishes no API version, an uncomparable
/// API version, or an API version below the floor is never a target.
///
/// The candidates considered are the nearest floodfills within
/// `policy.max_candidates_considered()`, in the same nearest order
/// [`select_floodfill_candidates`] produces, so the eligible subset of the
/// gated selection is the version-eligible subset of the ungated selection in
/// the same relative order.
///
/// # The examined count
///
/// `examined` counts only the candidates whose stored record could be read.
/// A candidate whose record cannot be read is **not** counted as examined and
/// yields neither a target nor a refusal, so the identity
/// `examined == accepted + refused` holds exactly: every examined candidate
/// is either an accepted target or a refusal counted by reason, and nothing
/// else contributes to either side. The unreadable branch is unreachable
/// today — the candidate list is derived from the same store the records are
/// read from, and the store is borrowed immutably for the whole selection —
/// but the branch is written so the identity cannot drift if that ever
/// changes.
///
/// The refusal counters saturate, and the candidate set is bounded by the
/// policy ceiling, so the counts stay bounded.
pub fn select_version_gated_floodfills(
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
    policy: &LookupPolicy,
) -> VersionGatedFloodfillSelection {
    let candidates = nearest_floodfill_candidates(store, routing_key, excluded, policy);
    gate_candidates(store, candidates)
}

/// Selects the nearest **peers** whose published API version satisfies the
/// DSM floor, whether or not they advertise floodfill.
///
/// Plan 545. This is the fallback a caller uses when the floodfill set is
/// empty, so a router keeps sessions with the peers it has actually learned.
/// The eligibility rule, the refusal counting and the
/// `examined == accepted + refused` identity are the floodfill selector's own,
/// reached through the same `gate_candidates`.
pub fn select_version_gated_peers(
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
    policy: &LookupPolicy,
) -> VersionGatedFloodfillSelection {
    let candidates = nearest_peer_candidates(store, routing_key, excluded, policy);
    gate_candidates(store, candidates)
}

/// Applies the DSM version floor to an ordered candidate set.
fn gate_candidates(
    store: &RouterInfoStore,
    candidates: Vec<NearestSelection>,
) -> VersionGatedFloodfillSelection {
    let mut targets: Vec<RouterHash> = Vec::new();
    let mut refusals = VersionRefusalCounts::default();
    let mut examined: u32 = 0;
    for candidate in candidates {
        let Some(record) = store.get(&candidate.key) else {
            // The record could not be read, so no version decision was
            // taken: the candidate is neither examined, nor accepted, nor
            // refused. The identity `examined == accepted + refused` holds.
            continue;
        };
        examined = examined.saturating_add(1);
        let eligibility = dsm_target_eligibility(published_version_of(record));
        if eligibility.is_eligible() {
            targets.push(candidate.key);
        } else {
            refusals.record(eligibility);
        }
    }
    VersionGatedFloodfillSelection {
        targets,
        refusals,
        examined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::{ValidatedRouterInfo, ValidationContext, router_hash};
    use crate::version::MIN_FLOODFILL_VERSION_FOR_DSM;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{ApiVersion, Date};
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

    /// A floodfill record that publishes an explicit API version.
    ///
    /// The `router.version` option is absent when `version` is `None`: the
    /// signed RouterInfo validation rejects an empty value, so "no version"
    /// and "malformed version" are the only two non-comparable cases. The
    /// record is built exactly like [`floodfill`], with the version option
    /// added.
    fn floodfill_with_optional_version(
        b: &RouterIdentityBundle,
        version: Option<&str>,
    ) -> ValidatedRouterInfo {
        let mut options = i2pr_proto::Mapping::builder();
        options.insert("caps".to_owned(), "f".to_owned()).unwrap();
        if let Some(version) = version {
            options
                .insert("router.version".to_owned(), version.to_owned())
                .unwrap();
        }
        let info = b
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate")
    }

    /// The DSM floor as the version string a peer publishes.
    fn floor_version() -> String {
        MIN_FLOODFILL_VERSION_FOR_DSM.to_string()
    }

    /// A version strictly below the DSM floor.
    fn below_floor_version() -> String {
        let floor = MIN_FLOODFILL_VERSION_FOR_DSM;
        assert_eq!(
            floor.components().len(),
            3,
            "the DSM floor must carry three components"
        );
        assert!(
            floor.patch() > 0,
            "the DSM floor must have a patch component"
        );
        ApiVersion::new(floor.major(), floor.minor(), floor.patch() - 1).to_string()
    }

    /// A version strictly above the DSM floor.
    fn above_floor_version() -> String {
        let floor = MIN_FLOODFILL_VERSION_FOR_DSM;
        ApiVersion::new(floor.major(), floor.minor(), floor.patch() + 1).to_string()
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

    /// A floodfill that publishes no API version is refused: an unknown
    /// version is never treated as "newest".
    #[test]
    fn the_shared_gate_refuses_a_floodfill_that_publishes_no_version() {
        let unknown = bundle(0x540);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(&unknown, None));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let policy = LookupPolicy::default();
        let selection = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
        assert!(selection.is_empty());
        assert_eq!(selection.len(), 0);
        assert_eq!(selection.accepted(), 0);
        assert_eq!(selection.examined(), 1);
        assert_eq!(selection.refusals().unknown_version, 1);
        assert_eq!(selection.refusals().uncomparable_version, 0);
        assert_eq!(selection.refusals().below_floor, 0);
        assert_eq!(selection.refusals().total(), 1);
    }

    /// A floodfill that publishes a malformed version is refused, and the
    /// refusal is counted under its own reason.
    #[test]
    fn the_shared_gate_refuses_a_floodfill_with_an_uncomparable_version() {
        let uncomparable = bundle(0x541);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(
            &uncomparable,
            Some("0.9.62."),
        ));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let policy = LookupPolicy::default();
        let selection = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
        assert!(selection.is_empty());
        assert_eq!(selection.examined(), 1);
        assert_eq!(selection.refusals().unknown_version, 0);
        assert_eq!(selection.refusals().uncomparable_version, 1);
        assert_eq!(selection.refusals().below_floor, 0);
        assert_eq!(selection.refusals().total(), 1);
    }

    /// A floodfill that publishes a version below the DSM floor is refused,
    /// and the refusal is counted under its own reason.
    #[test]
    fn the_shared_gate_refuses_a_floodfill_below_the_dsm_floor() {
        let below_floor = bundle(0x542);
        let below = below_floor_version();
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(&below_floor, Some(&below)));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let policy = LookupPolicy::default();
        let selection = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
        assert!(selection.is_empty());
        assert_eq!(selection.examined(), 1);
        assert_eq!(selection.refusals().unknown_version, 0);
        assert_eq!(selection.refusals().uncomparable_version, 0);
        assert_eq!(selection.refusals().below_floor, 1);
        assert_eq!(selection.refusals().total(), 1);
    }

    /// A floodfill that publishes exactly the DSM floor is eligible.
    #[test]
    fn a_floodfill_at_the_dsm_floor_is_eligible() {
        let at_floor = bundle(0x543);
        let floor = floor_version();
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(&at_floor, Some(&floor)));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let policy = LookupPolicy::default();
        let selection = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
        assert_eq!(
            selection.targets(),
            &[router_hash(at_floor.identity()).unwrap()]
        );
        assert_eq!(selection.len(), 1);
        assert_eq!(selection.accepted(), 1);
        assert_eq!(selection.examined(), 1);
        assert_eq!(selection.refusals().total(), 0);
    }

    /// The DSM gate has exactly two outcomes — eligible or refused by
    /// reason — so `examined == accepted + refused` holds exactly.
    #[test]
    fn the_examined_count_equals_accepted_plus_refused() {
        let at_floor = bundle(0x550);
        let unknown = bundle(0x551);
        let uncomparable = bundle(0x552);
        let below_floor = bundle(0x553);
        let floor = floor_version();
        let below = below_floor_version();
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(&at_floor, Some(&floor)));
        store.insert(floodfill_with_optional_version(&unknown, None));
        store.insert(floodfill_with_optional_version(
            &uncomparable,
            Some("0.9.62."),
        ));
        store.insert(floodfill_with_optional_version(&below_floor, Some(&below)));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let policy = LookupPolicy::default();
        let selection = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
        assert_eq!(selection.examined(), 4);
        assert_eq!(selection.accepted(), 1);
        assert_eq!(selection.refusals().total(), 3);
        assert_eq!(
            selection.examined(),
            selection.accepted() + selection.refusals().total()
        );
    }

    /// The gated selection keeps the ungated nearest order: its targets are
    /// the version-eligible subset of [`select_floodfill_candidates`]'s
    /// candidates, in the same relative order.
    #[test]
    fn the_shared_gate_keeps_the_ungated_nearest_order() {
        let at_floor = bundle(0x560);
        let above_floor = bundle(0x561);
        let below_floor = bundle(0x562);
        let unknown = bundle(0x563);
        let uncomparable = bundle(0x564);
        let floor = floor_version();
        let above = above_floor_version();
        let below = below_floor_version();
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(&at_floor, Some(&floor)));
        store.insert(floodfill_with_optional_version(&above_floor, Some(&above)));
        store.insert(floodfill_with_optional_version(&below_floor, Some(&below)));
        store.insert(floodfill_with_optional_version(&unknown, None));
        store.insert(floodfill_with_optional_version(
            &uncomparable,
            Some("0.9.62."),
        ));
        let target = RouterHash::from_bytes([0u8; 32]);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let policy = LookupPolicy::new(8, 1, 1, 1, 1000, 500).expect("policy");

        let ungated = select_floodfill_candidates(&store, &target, &routing_key, &[], &policy);
        let gated = select_version_gated_floodfills(&store, &routing_key, &[target], &policy);

        let eligible = [
            router_hash(at_floor.identity()).unwrap(),
            router_hash(above_floor.identity()).unwrap(),
        ];
        let expected: Vec<RouterHash> = ungated
            .hashes()
            .into_iter()
            .filter(|hash| eligible.contains(hash))
            .collect();
        assert_eq!(ungated.len(), 5);
        assert_eq!(gated.targets(), expected.as_slice());
        assert_eq!(gated.examined(), 5);
        assert_eq!(gated.accepted(), 2);
        assert_eq!(gated.refusals().total(), 3);
    }

    /// The gated selection honours the policy's candidate ceiling: it never
    /// examines more candidates than the policy allows.
    #[test]
    fn the_shared_gate_respects_the_candidate_ceiling() {
        let policy = LookupPolicy::new(1, 1, 1, 1, 1000, 500).expect("policy");
        let floor = floor_version();
        let signer_a = bundle(0x570);
        let signer_b = bundle(0x571);
        let signer_c = bundle(0x572);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(&signer_a, Some(&floor)));
        store.insert(floodfill_with_optional_version(&signer_b, Some(&floor)));
        store.insert(floodfill_with_optional_version(&signer_c, Some(&floor)));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let selection = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection.examined(), 1);
        assert_eq!(selection.refusals().total(), 0);
    }

    /// The gated selection honours the caller's exclusion set.
    #[test]
    fn the_shared_gate_excludes_the_supplied_hashes() {
        let floor = floor_version();
        let excluded_signer = bundle(0x580);
        let kept_signer = bundle(0x581);
        let mut store = RouterInfoStore::default();
        store.insert(floodfill_with_optional_version(
            &excluded_signer,
            Some(&floor),
        ));
        store.insert(floodfill_with_optional_version(&kept_signer, Some(&floor)));
        let routing_key = RouterHash::from_bytes([0u8; 32]);
        let policy = LookupPolicy::default();
        let excluded = router_hash(excluded_signer.identity()).unwrap();
        let selection = select_version_gated_floodfills(&store, &routing_key, &[excluded], &policy);
        assert_eq!(
            selection.targets(),
            &[router_hash(kept_signer.identity()).unwrap()]
        );
        assert_eq!(selection.examined(), 1);
    }
}
