//! Pure routing primitives used by NetDB selection logic.
//!
//! Plan 103 §5 restricts this module to XOR distance, deterministic
//! tie-breaking, floodfill filtering, and bounded nearest-N selection.
//! Daily routing-key derivation belongs to Plan 105 and is **not**
//! implemented here.

use crate::router_info::RouterHash;
use crate::store::RouterInfoStore;

/// Computes the XOR distance between two 32-byte hashes.
///
/// The result is itself a 32-byte value; the caller compares bytes
/// (lexicographically for the Kademlia convention). The function does
/// no allocation and is safe to call from tight loops.
pub fn xor_distance(left: &RouterHash, right: &RouterHash) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = left.as_bytes()[index] ^ right.as_bytes()[index];
    }
    out
}

/// Ordered pair of `(distance, candidate_key)` for nearest-N selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NearestSelection {
    /// The XOR distance to the target.
    pub distance: [u8; 32],
    /// The candidate's RouterHash.
    pub key: RouterHash,
}

impl Ord for NearestSelection {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Sort by distance ascending; RouterHash ascending is the
        // deterministic tie-breaker.
        self.distance
            .cmp(&other.distance)
            .then(self.key.cmp(&other.key))
    }
}

impl PartialOrd for NearestSelection {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Returns the `n` nearest records to `target`, restricted to records
/// that advertise floodfill capability.
///
/// `n` is bounded by the number of records in `store`; the function
/// never returns more than `n` entries. When `n` is zero the result is
/// always empty.
#[allow(dead_code)]
pub fn nearest_floodfill(
    store: &RouterInfoStore,
    target: &RouterHash,
    n: usize,
) -> Vec<NearestSelection> {
    if n == 0 {
        return Vec::new();
    }
    let mut candidates: Vec<NearestSelection> = store
        .floodfill_advertisers()
        .map(|record| NearestSelection {
            distance: xor_distance(&record.key(), target),
            key: record.key(),
        })
        .collect();
    candidates.sort();
    candidates.truncate(n);
    candidates
}

/// Returns the `n` nearest records to `target` without filtering by
/// capability. The same distance + RouterHash tie-break applies.
#[allow(dead_code)]
pub fn nearest(store: &RouterInfoStore, target: &RouterHash, n: usize) -> Vec<NearestSelection> {
    if n == 0 {
        return Vec::new();
    }
    let mut candidates: Vec<NearestSelection> = store
        .iter()
        .map(|(key, _record)| NearestSelection {
            distance: xor_distance(key, target),
            key: *key,
        })
        .collect();
    candidates.sort();
    candidates.truncate(n);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::{ValidatedRouterInfo, ValidationContext, router_hash};
    use crate::store::RouterInfoStore;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(b: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = b
            .sign_router_info(
                i2pr_proto::Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(i2pr_proto::Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn caps(b: &RouterIdentityBundle, published_ms: u64, caps: &str) -> ValidatedRouterInfo {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        let info = b
            .sign_router_info(
                i2pr_proto::Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(i2pr_proto::Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    #[test]
    fn xor_distance_is_symmetric_and_zero_for_identical_keys() {
        let key_a = RouterHash::from_bytes([0x11u8; 32]);
        let key_b = RouterHash::from_bytes([0x22u8; 32]);
        let ab = xor_distance(&key_a, &key_b);
        let ba = xor_distance(&key_b, &key_a);
        assert_eq!(ab, ba);
        let zero = xor_distance(&key_a, &key_a);
        assert_eq!(zero, [0u8; 32]);
    }

    #[test]
    fn nearest_returns_lexicographically_ordered_distances() {
        let signer_a = bundle(0x300);
        let signer_b = bundle(0x301);
        let signer_c = bundle(0x302);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer_a, 1));
        store.insert(validate(&signer_b, 1));
        store.insert(validate(&signer_c, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        let nearest_keys = nearest(&store, &target, 3);
        assert_eq!(nearest_keys.len(), 3);
        for window in nearest_keys.windows(2) {
            assert!(window[0].distance <= window[1].distance);
        }
    }

    #[test]
    fn nearest_floodfill_filters_to_capability_advertisers() {
        let floodfill = bundle(0x303);
        let plain = bundle(0x304);
        let mut store = RouterInfoStore::default();
        store.insert(caps(&floodfill, 1, "f"));
        store.insert(caps(&plain, 1, "L"));
        let target = RouterHash::from_bytes([0u8; 32]);
        let advertisers = nearest_floodfill(&store, &target, 10);
        assert_eq!(advertisers.len(), 1);
        let expected = router_hash(floodfill.identity()).expect("floodfill hash");
        assert_eq!(advertisers[0].key, expected);
    }

    #[test]
    fn nearest_with_zero_count_is_empty() {
        let signer = bundle(0x305);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        assert!(nearest(&store, &target, 0).is_empty());
        assert!(nearest_floodfill(&store, &target, 0).is_empty());
    }

    #[test]
    fn nearest_respects_n_smaller_than_store() {
        let signer_a = bundle(0x306);
        let signer_b = bundle(0x307);
        let signer_c = bundle(0x308);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer_a, 1));
        store.insert(validate(&signer_b, 1));
        store.insert(validate(&signer_c, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        assert_eq!(nearest(&store, &target, 2).len(), 2);
    }
}
