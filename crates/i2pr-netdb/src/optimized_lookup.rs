//! Plan 489 §7: the optimized lookup plan.
//!
//! The optimized lookup draws its candidates from the routing table
//! instead of from the flat store, and it splits them into two
//! classes before a single byte goes on the wire:
//!
//! - **queryable** — the router holds the peer's `RouterInfo`, so the
//!   peer can be addressed and asked directly;
//! - **pending** — the table knows the peer but the router has not
//!   resolved its `RouterInfo` yet, so the peer cannot be asked
//!   until a `RouterInfo` lookup has produced one.
//!
//! A peer whose resolved `RouterInfo` is known **not** to advertise the
//! `f` capability is neither queryable nor pending: it is refused, and
//! the count of refusals is reported so the caller can evict it from
//! the table.
//!
//! The plan is a pure function. It allocates two bounded vectors, sorts
//! by XOR distance to the requested routing key and never mutates the
//! table or the store.

use crate::bucket_set::BucketSet;
use crate::router_info::RouterHash;
use crate::store::RouterInfoStore;

/// The default number of queryable candidates one plan carries.
pub const MAX_OPTIMIZED_QUERYABLE: usize = 32;

/// The default number of pending candidates one plan carries.
pub const MAX_OPTIMIZED_PENDING: usize = 32;

/// The largest legal queryable ceiling.
pub const MAX_OPTIMIZED_QUERYABLE_LIMIT: usize = 64;

/// The largest legal pending ceiling.
pub const MAX_OPTIMIZED_PENDING_LIMIT: usize = 64;

/// One candidate offered to the optimized lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptimizedCandidate {
    /// The peer's canonical `RouterHash`.
    pub key: RouterHash,
    /// The XOR distance from the peer's position to the requested
    /// routing key.
    pub distance: [u8; 32],
    /// The provenance of the peer's floodfill status.
    pub evidence: crate::bucket_set::FloodfillEvidence,
}

/// The optimized lookup plan.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OptimizedPlan {
    /// The candidates the router may ask directly, closest first.
    pub queryable: Vec<OptimizedCandidate>,
    /// The candidates whose `RouterInfo` must be resolved first,
    /// closest first.
    pub pending: Vec<OptimizedCandidate>,
    /// The number of peers the table offered that were refused
    /// because their resolved `RouterInfo` does not advertise `f`.
    pub refused: usize,
}

impl OptimizedPlan {
    /// Returns the queryable candidates' hashes, closest first.
    pub fn queryable_hashes(&self) -> Vec<RouterHash> {
        self.queryable
            .iter()
            .map(|candidate| candidate.key)
            .collect()
    }

    /// Returns the pending candidates' hashes, closest first.
    pub fn pending_hashes(&self) -> Vec<RouterHash> {
        self.pending.iter().map(|candidate| candidate.key).collect()
    }

    /// Returns `true` when the plan offers no peer at all.
    pub fn is_empty(&self) -> bool {
        self.queryable.is_empty() && self.pending.is_empty()
    }

    /// Returns the total number of candidates the plan carries.
    pub fn total(&self) -> usize {
        self.queryable.len() + self.pending.len()
    }
}

/// Builds the optimized lookup plan for a target routing key.
///
/// The candidates come from the routing table and are ordered by their
/// XOR distance to `routing_key`, closest first, with the lower
/// `RouterHash` breaking a distance tie. `excluded` peers are never
/// offered. Both lists are bounded by their ceiling.
pub fn plan_optimized_lookup(
    table: &BucketSet,
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
    max_queryable: usize,
    max_pending: usize,
) -> OptimizedPlan {
    let max_queryable = max_queryable.min(MAX_OPTIMIZED_QUERYABLE_LIMIT);
    let max_pending = max_pending.min(MAX_OPTIMIZED_PENDING_LIMIT);
    let mut queryable: Vec<OptimizedCandidate> = Vec::new();
    let mut pending: Vec<OptimizedCandidate> = Vec::new();
    let mut refused = 0usize;
    for candidate in table.select(routing_key, table.len()) {
        if excluded.contains(&candidate.key) {
            continue;
        }
        let entry = OptimizedCandidate {
            key: candidate.key,
            distance: candidate.distance,
            evidence: candidate.evidence,
        };
        match store.get(&candidate.key) {
            Some(record) => {
                if record.advertises_floodfill() {
                    queryable.push(entry);
                } else {
                    refused += 1;
                }
            }
            None => pending.push(entry),
        }
    }
    queryable.truncate(max_queryable);
    pending.truncate(max_pending);
    OptimizedPlan {
        queryable,
        pending,
        refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bucket_set::{BucketSet, BucketSetConfig};
    use crate::router_info::{ValidatedRouterInfo, ValidationContext};
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn day_at(days: u64) -> Date {
        Date::from_millis(days * 86_400_000 + 3_600_000)
    }

    fn signer(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validated(bundle: &RouterIdentityBundle, caps: &str) -> ValidatedRouterInfo {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        let published = day_at(20_000);
        let info = bundle
            .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
            .expect("sign");
        ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
            .expect("validate")
    }

    fn table_with(peers: &[ValidatedRouterInfo]) -> BucketSet {
        let mut table = BucketSet::new(peers[0].key(), day_at(20_000), BucketSetConfig::default());
        for peer in peers {
            table.insert_signed(peer, day_at(20_000));
        }
        table
    }

    fn empty_store() -> RouterInfoStore {
        RouterInfoStore::with_config(crate::store::RouterInfoStoreConfig::new(64, 1 << 20))
    }

    #[test]
    fn a_known_floodfill_is_queryable() {
        let peers: Vec<ValidatedRouterInfo> = (0..3)
            .map(|index| validated(&signer(0x4891_0000 + index), "Nf"))
            .collect();
        let table = table_with(&peers);
        let mut store = empty_store();
        store.insert(peers[1].clone());
        let plan = plan_optimized_lookup(
            &table,
            &store,
            &peers[0].key(),
            &[],
            MAX_OPTIMIZED_QUERYABLE,
            MAX_OPTIMIZED_PENDING,
        );
        assert_eq!(plan.queryable.len(), 1);
        assert_eq!(plan.queryable[0].key, peers[1].key());
        assert_eq!(plan.pending.len(), 2);
        assert_eq!(plan.refused, 0);
    }

    #[test]
    fn a_known_non_floodfill_is_refused() {
        let peers: Vec<ValidatedRouterInfo> = (0..2)
            .map(|index| validated(&signer(0x4891_0010 + index), "Nf"))
            .collect();
        let mut table = table_with(&peers);
        // A peer whose RouterInfo says it is not a floodfill.
        let plain = validated(&signer(0x4891_0020), "N");
        let mut store = empty_store();
        store.insert(plain.clone());
        // The table cannot hold the plain peer on its own evidence, so
        // it is forced in as a suggestion from a verified floodfill.
        table.insert_suggested(plain.key(), &peers[0].key(), day_at(20_000));
        let plan = plan_optimized_lookup(
            &table,
            &store,
            &peers[1].key(),
            &[],
            MAX_OPTIMIZED_QUERYABLE,
            MAX_OPTIMIZED_PENDING,
        );
        assert_eq!(plan.refused, 1);
        assert!(
            !plan
                .queryable
                .iter()
                .any(|candidate| candidate.key == plain.key()),
            "a non-floodfill must never be queryable"
        );
        assert!(
            !plan
                .pending
                .iter()
                .any(|candidate| candidate.key == plain.key()),
            "a non-floodfill must never be pending"
        );
    }

    #[test]
    fn the_plan_is_closest_first() {
        let peers: Vec<ValidatedRouterInfo> = (0..6)
            .map(|index| validated(&signer(0x4891_0030 + index), "Nf"))
            .collect();
        let table = table_with(&peers);
        let target = peers[0].key();
        let plan = plan_optimized_lookup(
            &table,
            &empty_store(),
            &target,
            &[],
            MAX_OPTIMIZED_QUERYABLE,
            MAX_OPTIMIZED_PENDING,
        );
        assert_eq!(plan.pending.len(), 6);
        for window in plan.pending.windows(2) {
            assert!(
                window[0].distance <= window[1].distance,
                "the plan was not closest-first"
            );
        }
    }

    #[test]
    fn excluded_peers_are_never_offered() {
        let peers: Vec<ValidatedRouterInfo> = (0..4)
            .map(|index| validated(&signer(0x4891_0040 + index), "Nf"))
            .collect();
        let table = table_with(&peers);
        let excluded = [peers[0].key(), peers[1].key()];
        let plan = plan_optimized_lookup(
            &table,
            &empty_store(),
            &peers[2].key(),
            &excluded,
            MAX_OPTIMIZED_QUERYABLE,
            MAX_OPTIMIZED_PENDING,
        );
        for key in excluded {
            assert!(!plan.pending_hashes().contains(&key));
        }
    }

    #[test]
    fn the_plan_is_bounded() {
        let peers: Vec<ValidatedRouterInfo> = (0..20)
            .map(|index| validated(&signer(0x4891_0050 + index), "Nf"))
            .collect();
        let table = table_with(&peers);
        let plan = plan_optimized_lookup(&table, &empty_store(), &peers[0].key(), &[], 2, 3);
        assert_eq!(plan.queryable.len(), 0);
        assert_eq!(plan.pending.len(), 3);
        assert_eq!(plan.total(), 3);
    }

    #[test]
    fn an_empty_table_yields_an_empty_plan() {
        let peers: Vec<ValidatedRouterInfo> = vec![validated(&signer(0x4891_0060), "Nf")];
        let table = BucketSet::new(peers[0].key(), day_at(20_000), BucketSetConfig::default());
        assert!(table.is_empty());
        let plan = plan_optimized_lookup(
            &table,
            &empty_store(),
            &peers[0].key(),
            &[],
            MAX_OPTIMIZED_QUERYABLE,
            MAX_OPTIMIZED_PENDING,
        );
        assert!(plan.is_empty());
        assert_eq!(plan.total(), 0);
    }
}
