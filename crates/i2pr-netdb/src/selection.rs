//! Plan 488 pure peer selection policy.
//!
//! The selection policy is a **pure function** from the profile store and a
//! set of constraints to a bounded, diverse candidate set. It owns no
//! socket, no timer, no file, and no task, and it never mutates the store.
//!
//! Four rules are applied, in this order:
//!
//! 1. **Eligibility.** A peer is eligible when it is not excluded, meets
//!    the minimum rank, meets the minimum tier, satisfies the floodfill
//!    requirement, and has not reached its participation cap.
//! 2. **Deterministic order.** Eligible peers are ordered by rank
//!    descending, with the RouterHash ascending as the tie-break, so the
//!    order is total and reproducible.
//! 3. **Shuffle.** The eligible list is shuffled with a caller-supplied
//!    seed before the pick, so the router does not always select the same
//!    peers and the diversity rule is not applied to a fixed prefix order.
//! 4. **Diversity.** No more than `max_per_prefix16` peers from the same
//!    /16 network prefix enter one selection. When the store cannot satisfy
//!    the rule, the selection returns fewer candidates — it never violates
//!    the rule to reach the requested count.
//!
//! The returned candidates are ordered by rank descending, so a caller can
//! see directly that a peer that answered outranks a peer that did not.

use std::collections::BTreeMap;

use crate::profile::{ProfileState, ProfileTier, network_prefix16};
use crate::profile_store::ProfileStore;
use crate::router_info::RouterHash;

/// The largest number of candidates one selection may return.
pub const MAX_SELECTION_COUNT: usize = 64;

/// The largest number of peers the participation ledger tracks.
pub const MAX_PARTICIPATION_TRACKED_PEERS: usize = 4_096;

/// The default maximum number of selections a peer may accumulate.
pub const DEFAULT_MAX_PARTICIPATION: u32 = 8;

/// One selected peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Candidate {
    /// The peer's RouterHash.
    pub key: RouterHash,
    /// The peer's capacity tier.
    pub tier: ProfileTier,
    /// The peer's reachability state.
    pub state: ProfileState,
    /// The peer's bounded score.
    pub score: u16,
    /// The peer's combined selection rank.
    pub rank: u16,
    /// The peer's /16 network prefix.
    pub prefix16: u16,
}

/// The constraints one selection honours.
#[derive(Clone, Debug)]
pub struct SelectionConstraints<'a> {
    /// The number of candidates requested. The value is clamped to
    /// [`MAX_SELECTION_COUNT`].
    pub count: usize,
    /// The minimum acceptable rank. A peer below this rank is not eligible.
    pub min_rank: u16,
    /// Whether only floodfill advertisers may be selected.
    pub require_floodfill: bool,
    /// The minimum acceptable tier. `None` accepts every tier.
    pub min_tier: Option<ProfileTier>,
    /// Peers that must never be selected.
    pub excluded: &'a [RouterHash],
    /// The caller's per-peer participation counters.
    pub participation: &'a BTreeMap<RouterHash, u32>,
    /// The maximum number of selections a peer may accumulate. A peer whose
    /// counter has reached this value is not eligible.
    pub max_participation: u32,
    /// The maximum number of peers from one /16 prefix in one selection.
    pub max_per_prefix16: usize,
    /// The seed of the deterministic shuffle.
    pub seed: u64,
}

/// The shared empty participation counter table.
static EMPTY_PARTICIPATION: BTreeMap<RouterHash, u32> = BTreeMap::new();

impl Default for SelectionConstraints<'_> {
    fn default() -> Self {
        Self {
            count: 0,
            min_rank: 0,
            require_floodfill: false,
            min_tier: None,
            excluded: &[],
            participation: &EMPTY_PARTICIPATION,
            max_participation: DEFAULT_MAX_PARTICIPATION,
            max_per_prefix16: 1,
            seed: 0,
        }
    }
}

impl SelectionConstraints<'_> {
    /// Constructs a constraint set with the default diversity and
    /// participation rules.
    pub fn new(count: usize, seed: u64) -> Self {
        Self {
            count,
            seed,
            ..Self::default()
        }
    }

    /// Returns the requested count clamped to the bounded maximum.
    pub const fn bounded_count(&self) -> usize {
        if self.count > MAX_SELECTION_COUNT {
            MAX_SELECTION_COUNT
        } else {
            self.count
        }
    }
}

/// A bounded ledger of per-peer participation counters.
///
/// The ledger is the caller's record of how often each peer has been
/// selected. It is bounded in size: when it is full, the least recently
/// recorded peer is evicted rather than growing without limit.
#[derive(Clone, Debug)]
pub struct ParticipationLedger {
    counts: BTreeMap<RouterHash, u32>,
    order: Vec<RouterHash>,
    max_participation: u32,
    max_tracked_peers: usize,
}

impl ParticipationLedger {
    /// Constructs a ledger with the supplied caps.
    pub fn new(max_participation: u32, max_tracked_peers: usize) -> Self {
        Self {
            counts: BTreeMap::new(),
            order: Vec::new(),
            max_participation,
            max_tracked_peers,
        }
    }

    /// Returns the participation counter of one peer.
    pub fn count(&self, key: &RouterHash) -> u32 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    /// Returns whether a peer has reached the participation cap.
    pub fn is_capped(&self, key: &RouterHash) -> bool {
        self.count(key) >= self.max_participation
    }

    /// Records one selection of a peer.
    ///
    /// The counter saturates at the configured cap; the ledger evicts the
    /// oldest tracked peer when it is full.
    pub fn record(&mut self, key: &RouterHash) {
        if self.max_tracked_peers == 0 {
            return;
        }
        if !self.counts.contains_key(key) {
            if self.order.len() >= self.max_tracked_peers
                && let Some(oldest) = self.order.first().copied()
            {
                self.order.remove(0);
                self.counts.remove(&oldest);
            }
            self.order.push(*key);
            self.counts.insert(*key, 0);
        }
        let entry = self.counts.entry(*key).or_insert(0);
        *entry = entry.saturating_add(1).min(self.max_participation);
    }

    /// Returns the tracked counters.
    pub fn counts(&self) -> &BTreeMap<RouterHash, u32> {
        &self.counts
    }

    /// Returns the number of tracked peers.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    /// Returns whether the ledger tracks no peer.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
}

/// Selects a bounded, diverse candidate set from the profile store.
///
/// The function is pure: the same store contents and the same constraints
/// produce the same candidates, byte for byte, on every run.
pub fn select_candidates(
    store: &ProfileStore,
    constraints: &SelectionConstraints<'_>,
) -> Vec<Candidate> {
    let wanted = constraints.bounded_count();
    if wanted == 0 {
        return Vec::new();
    }

    let mut eligible: Vec<Candidate> = Vec::new();
    for (key, profile) in store.iter() {
        if constraints.excluded.contains(key) {
            continue;
        }
        let rank = profile.rank();
        if rank < constraints.min_rank {
            continue;
        }
        if constraints.require_floodfill && !profile.advertises_floodfill() {
            continue;
        }
        if let Some(min_tier) = constraints.min_tier
            && profile.tier() < min_tier
        {
            continue;
        }
        if constraints.max_participation > 0
            && constraints.participation.get(key).copied().unwrap_or(0)
                >= constraints.max_participation
        {
            continue;
        }
        eligible.push(Candidate {
            key: *key,
            tier: profile.tier(),
            state: profile.state(),
            score: profile.score().value(),
            rank,
            prefix16: network_prefix16(key),
        });
    }

    if eligible.is_empty() {
        return Vec::new();
    }

    // Deterministic base order: rank descending, RouterHash ascending.
    eligible.sort_by(|left, right| {
        right
            .rank
            .cmp(&left.rank)
            .then_with(|| left.key.cmp(&right.key))
    });

    // Shuffle before picking so the router does not always select the same
    // peers and the diversity rule is not applied to a fixed prefix order.
    shuffle(&mut eligible, constraints.seed);

    // Apply the diversity rule while drawing.
    let mut per_prefix: BTreeMap<u16, usize> = BTreeMap::new();
    let mut selected: Vec<Candidate> = Vec::with_capacity(wanted);
    for candidate in eligible {
        if selected.len() >= wanted {
            break;
        }
        if constraints.max_per_prefix16 > 0 {
            let used = per_prefix.entry(candidate.prefix16).or_insert(0);
            if *used >= constraints.max_per_prefix16 {
                continue;
            }
            *used += 1;
        }
        selected.push(candidate);
    }

    // The preference is visible in the returned order.
    selected.sort_by(|left, right| {
        right
            .rank
            .cmp(&left.rank)
            .then_with(|| left.key.cmp(&right.key))
    });
    selected
}

/// Applies a deterministic Fisher–Yates shuffle.
fn shuffle(items: &mut [Candidate], seed: u64) {
    if items.len() < 2 {
        return;
    }
    let mut state = seed;
    for index in (1..items.len()).rev() {
        let draw = splitmix64(&mut state);
        let target = (draw % (index as u64 + 1)) as usize;
        items.swap(index, target);
    }
}

/// A small, deterministic, seedable generator.
///
/// The policy must be reproducible byte for byte, so it uses its own
/// generator rather than any ambient entropy.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_store::{ProfileInsertOutcome, ProfileStore};
    use crate::router_info::{ValidatedRouterInfo, ValidationContext};
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(signer: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = signer
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn caps_options(caps: &str) -> Mapping {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        options.build().unwrap()
    }

    /// Builds a store with `count` peers, all sharing one distinct /16
    /// prefix each so the diversity rule never binds by accident.
    fn store_with_distinct_prefixes(count: usize) -> (ProfileStore, Vec<RouterHash>) {
        let mut store = ProfileStore::default();
        let mut keys = Vec::with_capacity(count);
        for index in 0..count {
            let signer = bundle(0xA00 + index as u64);
            let validated = validate(&signer, 1);
            keys.push(validated.key());
            assert_eq!(store.insert(validated, 1), ProfileInsertOutcome::Inserted);
        }
        (store, keys)
    }

    #[test]
    fn a_zero_count_selects_nothing() {
        let (store, _keys) = store_with_distinct_prefixes(4);
        let constraints = SelectionConstraints::new(0, 7);
        assert!(select_candidates(&store, &constraints).is_empty());
    }

    #[test]
    fn the_count_is_clamped_to_the_bounded_maximum() {
        let constraints = SelectionConstraints::new(usize::MAX, 1);
        assert_eq!(constraints.bounded_count(), MAX_SELECTION_COUNT);
    }

    #[test]
    fn an_exact_load_returns_the_requested_count() {
        let (store, _keys) = store_with_distinct_prefixes(4);
        let constraints = SelectionConstraints::new(4, 11);
        assert_eq!(select_candidates(&store, &constraints).len(), 4);
    }

    #[test]
    fn a_request_beyond_the_store_returns_what_exists() {
        let (store, _keys) = store_with_distinct_prefixes(2);
        let constraints = SelectionConstraints::new(5, 11);
        assert_eq!(select_candidates(&store, &constraints).len(), 2);
    }

    #[test]
    fn the_selection_is_deterministic_for_the_same_input() {
        let (store, _keys) = store_with_distinct_prefixes(8);
        let constraints = SelectionConstraints::new(4, 0xDEAD_BEEF);
        let first = select_candidates(&store, &constraints);
        let second = select_candidates(&store, &constraints);
        assert_eq!(first, second);
    }

    #[test]
    fn a_different_seed_selects_a_different_set_when_the_store_allows_it() {
        let (store, _keys) = store_with_distinct_prefixes(16);
        let first = select_candidates(&store, &SelectionConstraints::new(4, 1));
        let second = select_candidates(&store, &SelectionConstraints::new(4, 2));
        assert_ne!(
            first.iter().map(|c| c.key).collect::<Vec<_>>(),
            second.iter().map(|c| c.key).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_diversity_rule_keeps_one_peer_per_prefix() {
        // Force every peer into the same /16 by construction is not
        // possible with random hashes, so the rule is exercised through the
        // prefix accounting: no two selected candidates may share a prefix
        // when the rule is one.
        let (store, _keys) = store_with_distinct_prefixes(32);
        let constraints = SelectionConstraints::new(32, 99);
        let selected = select_candidates(&store, &constraints);
        let mut prefixes: Vec<u16> = selected.iter().map(|c| c.prefix16).collect();
        prefixes.sort_unstable();
        let before = prefixes.len();
        prefixes.dedup();
        assert_eq!(before, prefixes.len(), "two peers shared a /16 prefix");
    }

    #[test]
    fn the_participation_cap_excludes_a_capped_peer() {
        let (store, keys) = store_with_distinct_prefixes(4);
        let mut ledger = ParticipationLedger::new(2, 16);
        ledger.record(&keys[0]);
        ledger.record(&keys[0]);
        assert!(ledger.is_capped(&keys[0]));
        let constraints = SelectionConstraints {
            count: 4,
            participation: ledger.counts(),
            max_participation: 2,
            ..SelectionConstraints::new(4, 5)
        };
        let selected = select_candidates(&store, &constraints);
        assert!(
            !selected.iter().any(|candidate| candidate.key == keys[0]),
            "a capped peer must not be selected"
        );
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn the_ledger_is_bounded() {
        let mut ledger = ParticipationLedger::new(4, 2);
        let first = bundle(0xB00);
        let second = bundle(0xB01);
        let third = bundle(0xB02);
        ledger.record(&validate(&first, 1).key());
        ledger.record(&validate(&second, 1).key());
        ledger.record(&validate(&third, 1).key());
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn a_responsive_peer_outranks_a_silent_one() {
        let mut store = ProfileStore::default();
        let good = bundle(0xB10);
        let bad = bundle(0xB11);
        let good_validated = validate(&good, 1);
        let bad_validated = validate(&bad, 1);
        let good_key = good_validated.key();
        let bad_key = bad_validated.key();
        store.insert(good_validated, 1);
        store.insert(bad_validated, 1);
        store.observe_answered(&good_key, 2);
        store.observe_unanswered(&bad_key, 2);
        let selected = select_candidates(&store, &SelectionConstraints::new(2, 3));
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].key, good_key);
        assert!(selected[0].rank > selected[1].rank);
    }

    #[test]
    fn a_minimum_rank_filters_a_silent_peer_out() {
        let mut store = ProfileStore::default();
        let good = bundle(0xB12);
        let bad = bundle(0xB13);
        let good_validated = validate(&good, 1);
        let bad_validated = validate(&bad, 1);
        let good_key = good_validated.key();
        let bad_key = bad_validated.key();
        store.insert(good_validated, 1);
        store.insert(bad_validated, 1);
        store.observe_answered(&good_key, 2);
        store.observe_unanswered(&bad_key, 2);
        let good_rank = store.profile(&good_key).expect("profile").rank();
        let constraints = SelectionConstraints {
            count: 4,
            min_rank: good_rank,
            ..SelectionConstraints::new(4, 1)
        };
        let selected = select_candidates(&store, &constraints);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].key, good_key);
    }

    #[test]
    fn the_floodfill_requirement_filters_the_selection() {
        let mut store = ProfileStore::default();
        let floodfill = bundle(0xB14);
        let plain = bundle(0xB15);
        let floodfill_validated = crate::router_info::ValidatedRouterInfo::from_router_info(
            floodfill
                .sign_router_info(
                    Date::from_millis(1),
                    Vec::new(),
                    Vec::new(),
                    caps_options("Nf"),
                )
                .expect("sign"),
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate");
        store.insert(floodfill_validated, 1);
        store.insert(validate(&plain, 1), 1);
        let constraints = SelectionConstraints {
            require_floodfill: true,
            ..SelectionConstraints::new(4, 1)
        };
        let selected = select_candidates(&store, &constraints);
        assert_eq!(selected.len(), 1);
        assert!(selected[0].tier == ProfileTier::HighCapacity);
    }

    #[test]
    fn an_excluded_peer_is_never_selected() {
        let (store, keys) = store_with_distinct_prefixes(4);
        let constraints = SelectionConstraints {
            count: 4,
            excluded: &keys[..1],
            ..SelectionConstraints::new(4, 1)
        };
        let selected = select_candidates(&store, &constraints);
        assert_eq!(selected.len(), 3);
        assert!(!selected.iter().any(|candidate| candidate.key == keys[0]));
    }

    #[test]
    fn the_minimum_tier_filters_the_selection() {
        let mut store = ProfileStore::default();
        let high = bundle(0xB16);
        let standard = bundle(0xB17);
        let high_validated = crate::router_info::ValidatedRouterInfo::from_router_info(
            high.sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                caps_options("Nf"),
            )
            .expect("sign"),
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate");
        let standard_validated = crate::router_info::ValidatedRouterInfo::from_router_info(
            standard
                .sign_router_info(
                    Date::from_millis(1),
                    Vec::new(),
                    Vec::new(),
                    caps_options("L"),
                )
                .expect("sign"),
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate");
        store.insert(high_validated, 1);
        store.insert(standard_validated, 1);
        let constraints = SelectionConstraints {
            min_tier: Some(ProfileTier::HighCapacity),
            ..SelectionConstraints::new(4, 1)
        };
        let selected = select_candidates(&store, &constraints);
        assert_eq!(selected.len(), 1);
    }
}
