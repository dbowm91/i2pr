//! Bounded in-memory `RouterInfo` store.
//!
//! The store accepts only [`ValidatedRouterInfo`] values; callers cannot
//! insert an unchecked record. Plan 103 §3 mandates deterministic
//! replacement/conflict/expiry semantics with checked arithmetic and
//! no saturating hidden bugs.

use std::collections::BTreeMap;

use i2pr_proto::Date;

use crate::router_info::{RouterHash, ValidatedRouterInfo};

/// Outcome of a [`RouterInfoStore::insert`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertOutcome {
    /// The record was inserted as a new entry.
    Inserted,
    /// A byte-identical record was already present; the insert is a
    /// no-op.
    Idempotent,
    /// An older record for the same RouterHash was replaced by a
    /// strictly newer published record.
    Replaced,
    /// A record with the same RouterHash and publication timestamp but
    /// different bytes was rejected; the existing record is preserved.
    Conflict,
    /// A strictly older record for the same RouterHash was rejected.
    StaleReplacement,
    /// The store is at capacity; the candidate was rejected without
    /// mutating existing state.
    CapacityExceeded,
}

/// Configuration for [`RouterInfoStore::with_config`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterInfoStoreConfig {
    /// Maximum number of records retained by the store.
    pub max_records: usize,
    /// Maximum total encoded bytes retained by the store.
    pub max_total_encoded_bytes: usize,
}

impl Default for RouterInfoStoreConfig {
    fn default() -> Self {
        Self {
            max_records: 4_096,
            max_total_encoded_bytes: 4 * 1024 * 1024,
        }
    }
}

impl RouterInfoStoreConfig {
    /// Constructs a custom configuration.
    pub const fn new(max_records: usize, max_total_encoded_bytes: usize) -> Self {
        Self {
            max_records,
            max_total_encoded_bytes,
        }
    }
}

/// Privacy-safe aggregate statistics returned by the store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct RouterInfoStoreStats {
    /// Number of records currently retained.
    pub record_count: usize,
    /// Total encoded bytes currently retained.
    pub total_encoded_bytes: usize,
    /// Configured record ceiling.
    pub max_records: usize,
    /// Configured byte ceiling.
    pub max_total_encoded_bytes: usize,
    /// Number of records that advertise the `f` capability.
    pub floodfill_advertiser_count: usize,
}

/// A bounded in-memory store of validated RouterInfos.
///
/// Plan 103 §3 forbids an `insert_unchecked` bypass; the only public
/// insertion path is `insert`, which consumes a [`ValidatedRouterInfo`].
#[derive(Debug)]
pub struct RouterInfoStore {
    records: BTreeMap<RouterHash, ValidatedRouterInfo>,
    total_encoded_bytes: usize,
    config: RouterInfoStoreConfig,
}

impl Default for RouterInfoStore {
    fn default() -> Self {
        Self::with_config(RouterInfoStoreConfig::default())
    }
}

impl RouterInfoStore {
    /// Constructs a store with a custom configuration.
    ///
    /// A zero `max_records` or zero `max_total_encoded_bytes` is
    /// treated as an explicit "reject every insert" configuration.
    pub fn with_config(config: RouterInfoStoreConfig) -> Self {
        Self {
            records: BTreeMap::new(),
            total_encoded_bytes: 0,
            config,
        }
    }

    /// Inserts a validated record and returns the outcome.
    ///
    /// The replacement/conflict semantics follow Plan 103 §3.3:
    ///
    /// ```text
    /// incoming published > existing published  -> replace if valid and budget permits
    /// incoming published < existing published  -> reject/ignore as stale replacement
    /// same published + byte-identical record    -> idempotent no-op
    /// same published + different signed record  -> typed conflict; retain existing
    /// ```
    pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome {
        let key = validated.key();
        let encoded_len = validated.encoded_len();

        if let Some(existing) = self.records.get(&key) {
            let existing_published = existing.published();
            let incoming_published = validated.published();
            if incoming_published < existing_published {
                return InsertOutcome::StaleReplacement;
            }
            if incoming_published == existing_published {
                if same_record_bytes(existing, &validated) {
                    return InsertOutcome::Idempotent;
                }
                return InsertOutcome::Conflict;
            }
            // incoming > existing; replacement path.
            if encoded_len > existing.encoded_len() {
                let extra = encoded_len - existing.encoded_len();
                if self
                    .total_encoded_bytes
                    .checked_add(extra)
                    .is_none_or(|total| total > self.config.max_total_encoded_bytes)
                {
                    return InsertOutcome::CapacityExceeded;
                }
            }
            self.total_encoded_bytes = self
                .total_encoded_bytes
                .checked_sub(existing.encoded_len())
                .expect("accounting underflow on replace");
            self.total_encoded_bytes = self
                .total_encoded_bytes
                .checked_add(encoded_len)
                .expect("accounting overflow on replace");
            self.records.insert(key, validated);
            return InsertOutcome::Replaced;
        }

        // New key path: enforce both quotas up-front.
        if self.records.len() >= self.config.max_records {
            return InsertOutcome::CapacityExceeded;
        }
        if self
            .total_encoded_bytes
            .checked_add(encoded_len)
            .is_none_or(|total| total > self.config.max_total_encoded_bytes)
        {
            return InsertOutcome::CapacityExceeded;
        }
        self.total_encoded_bytes = self
            .total_encoded_bytes
            .checked_add(encoded_len)
            .expect("accounting overflow on insert");
        self.records.insert(key, validated);
        InsertOutcome::Inserted
    }

    /// Returns the record for the supplied key, if any.
    pub fn get(&self, key: &RouterHash) -> Option<&ValidatedRouterInfo> {
        self.records.get(key)
    }

    /// Returns whether the store currently retains a record for the
    /// supplied key.
    pub fn contains(&self, key: &RouterHash) -> bool {
        self.records.contains_key(key)
    }

    /// Removes the record for the supplied key.
    ///
    /// Returns `true` when a record was removed, `false` when the key
    /// was unknown. The byte accounting is updated atomically.
    pub fn remove(&mut self, key: &RouterHash) -> bool {
        match self.records.remove(key) {
            Some(removed) => {
                self.total_encoded_bytes = self
                    .total_encoded_bytes
                    .checked_sub(removed.encoded_len())
                    .expect("accounting underflow on remove");
                true
            }
            None => false,
        }
    }

    /// Returns the number of records currently retained.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Returns the aggregate encoded byte count.
    pub fn encoded_bytes(&self) -> usize {
        self.total_encoded_bytes
    }

    /// Returns the store configuration.
    pub fn config(&self) -> RouterInfoStoreConfig {
        self.config
    }

    /// Returns a privacy-safe aggregate snapshot.
    pub fn stats(&self) -> RouterInfoStoreStats {
        let floodfill_advertiser_count = self
            .records
            .values()
            .filter(|record| record.advertises_floodfill())
            .count();
        RouterInfoStoreStats {
            record_count: self.records.len(),
            total_encoded_bytes: self.total_encoded_bytes,
            max_records: self.config.max_records,
            max_total_encoded_bytes: self.config.max_total_encoded_bytes,
            floodfill_advertiser_count,
        }
    }

    /// Iterates records in canonical RouterHash order.
    pub fn iter(&self) -> impl Iterator<Item = (&RouterHash, &ValidatedRouterInfo)> {
        self.records.iter()
    }

    /// Prunes records whose publication timestamp falls outside the
    /// supplied freshness policy. Returns the count of removed records.
    ///
    /// Plan 103 §3.4 prefers explicit prune over silent eviction. The
    /// caller supplies the current `now` so the operation is
    /// deterministic.
    pub fn prune(&mut self, now: Date, max_age_ms: u64) -> usize {
        let mut removed = 0usize;
        let stale_keys: Vec<RouterHash> = self
            .records
            .iter()
            .filter_map(|(key, record)| {
                let age = now
                    .as_millis()
                    .saturating_sub(record.published().as_millis());
                if age > max_age_ms { Some(*key) } else { None }
            })
            .collect();
        for key in stale_keys {
            if self.remove(&key) {
                removed += 1;
            }
        }
        removed
    }

    /// Returns an iterator over records that advertise the
    /// `f` capability.
    pub fn floodfill_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo> {
        self.records
            .values()
            .filter(|record| record.advertises_floodfill())
    }
}

fn same_record_bytes(left: &ValidatedRouterInfo, right: &ValidatedRouterInfo) -> bool {
    left.router_info().signed_bytes() == right.router_info().signed_bytes()
        && left.router_info().signature().as_bytes() == right.router_info().signature().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::{ValidatedRouterInfo, ValidationContext};
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(bundle: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = bundle
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

    #[test]
    fn first_insert_returns_inserted_and_updates_accounting() {
        let signer = bundle(0x200);
        let mut store = RouterInfoStore::default();
        let validated = validate(&signer, 1);
        let key = validated.key();
        let bytes = validated.encoded_len();
        assert_eq!(store.insert(validated), InsertOutcome::Inserted);
        assert_eq!(store.len(), 1);
        assert_eq!(store.encoded_bytes(), bytes);
        assert!(store.contains(&key));
        assert_eq!(store.stats().record_count, 1);
        assert_eq!(store.stats().total_encoded_bytes, bytes);
    }

    #[test]
    fn byte_identical_reinsert_is_idempotent() {
        let signer = bundle(0x201);
        let mut store = RouterInfoStore::default();
        let validated = validate(&signer, 1);
        store.insert(validated.clone());
        let outcome = store.insert(validated);
        assert_eq!(outcome, InsertOutcome::Idempotent);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn newer_published_replaces_existing() {
        let signer = bundle(0x202);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        let bytes_v1 = store.encoded_bytes();
        store.insert(validate(&signer, 2));
        assert_eq!(store.len(), 1);
        // Same encoded length in this synthetic test, so accounting is stable.
        assert_eq!(store.encoded_bytes(), bytes_v1);
        let record = store.get(&validate(&signer, 2).key()).expect("present");
        assert_eq!(record.published().as_millis(), 2);
    }

    #[test]
    fn older_published_replacement_is_rejected_as_stale() {
        let signer = bundle(0x203);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 5));
        let outcome = store.insert(validate(&signer, 4));
        assert_eq!(outcome, InsertOutcome::StaleReplacement);
        assert_eq!(store.len(), 1);
        assert_eq!(
            store
                .get(&validate(&signer, 5).key())
                .unwrap()
                .published()
                .as_millis(),
            5
        );
    }

    #[test]
    fn equal_published_different_bytes_is_a_conflict() {
        // Two distinct RouterInfos with the same published timestamp but
        // different signed bytes are a structural conflict. The fixture
        // builder used in this crate emits byte-identical signed regions
        // for the same published value when the inputs match, so we
        // craft two routers whose `options` mapping differs; their
        // signed regions therefore differ.
        let signer = bundle(0x204);
        let now_ms = 10;
        let info_a = signer
            .sign_router_info(
                Date::from_millis(now_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign a");
        let mut options = Mapping::builder();
        options
            .insert("router.version".to_owned(), "0.9.68".to_owned())
            .unwrap();
        let info_b = signer
            .sign_router_info(
                Date::from_millis(now_ms),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign b");
        let validated_a = ValidatedRouterInfo::from_router_info(
            info_a,
            None,
            ValidationContext::new(Date::from_millis(now_ms)),
        )
        .expect("validate a");
        let validated_b = ValidatedRouterInfo::from_router_info(
            info_b,
            None,
            ValidationContext::new(Date::from_millis(now_ms)),
        )
        .expect("validate b");
        let mut store = RouterInfoStore::default();
        store.insert(validated_a.clone());
        let outcome = store.insert(validated_b);
        assert_eq!(outcome, InsertOutcome::Conflict);
        let kept = store.get(&validated_a.key()).expect("present");
        assert_eq!(kept.encoded_len(), validated_a.encoded_len());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn record_count_quota_rejects_extra_inserts() {
        let signer = bundle(0x205);
        let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(1, usize::MAX));
        store.insert(validate(&signer, 1));
        let other = bundle(0x206);
        let outcome = store.insert(validate(&other, 1));
        assert_eq!(outcome, InsertOutcome::CapacityExceeded);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn byte_quota_rejects_oversized_inserts() {
        let signer = bundle(0x207);
        let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(usize::MAX, 0));
        let outcome = store.insert(validate(&signer, 1));
        assert_eq!(outcome, InsertOutcome::CapacityExceeded);
        assert!(store.is_empty());
    }

    #[test]
    fn replacement_growing_encoded_length_respects_byte_quota() {
        let signer = bundle(0x208);
        let small_info = signer
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        let small_len = small_info.signed_bytes().len() + small_info.signature().as_bytes().len();
        // Build a configuration that admits the small record yet rejects a
        // larger replacement.
        let mut store =
            RouterInfoStore::with_config(RouterInfoStoreConfig::new(usize::MAX, small_len));
        let small = ValidatedRouterInfo::from_router_info(
            small_info,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate small");
        store.insert(small);
        // Build a larger replacement: same key, newer published, with an
        // extra options mapping entry that grows the signed region.
        let mut larger_options = Mapping::builder();
        larger_options
            .insert("router.version".to_owned(), "0.9.68".to_owned())
            .unwrap();
        let larger_info = signer
            .sign_router_info(
                Date::from_millis(2),
                Vec::new(),
                Vec::new(),
                larger_options.build().unwrap(),
            )
            .expect("sign larger");
        let larger = ValidatedRouterInfo::from_router_info(
            larger_info,
            None,
            ValidationContext::new(Date::from_millis(2)),
        )
        .expect("validate larger");
        assert!(larger.encoded_len() > small_len);
        let outcome = store.insert(larger);
        assert_eq!(outcome, InsertOutcome::CapacityExceeded);
        // Original record remains in place with intact accounting.
        assert_eq!(store.encoded_bytes(), small_len);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn prune_removes_stale_records() {
        let signer = bundle(0x209);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        store.insert(validate(&bundle(0x20a), 2));
        // Now = 3, max age 1ms; published 1 is stale, published 2 is fresh.
        let removed = store.prune(Date::from_millis(3), 1);
        assert_eq!(removed, 1);
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.encoded_bytes(),
            validate(&bundle(0x20a), 2).encoded_len()
        );
    }

    #[test]
    fn remove_releases_accounting() {
        let signer = bundle(0x20b);
        let mut store = RouterInfoStore::default();
        let validated = validate(&signer, 1);
        let key = validated.key();
        let bytes = validated.encoded_len();
        store.insert(validated);
        assert_eq!(store.encoded_bytes(), bytes);
        assert!(store.remove(&key));
        assert_eq!(store.encoded_bytes(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn remove_unknown_key_is_false_and_keeps_state() {
        let signer = bundle(0x20c);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        let other = bundle(0x20d);
        let phantom = router_hash_dummy(&other);
        assert!(!store.remove(&phantom));
        assert_eq!(store.len(), 1);
    }

    fn router_hash_dummy(b: &RouterIdentityBundle) -> RouterHash {
        crate::router_info::router_hash(b.identity()).expect("hash")
    }

    #[test]
    fn floodfill_advertiser_iterator_returns_only_self_advertisers() {
        let signer_floodfill = bundle(0x20e);
        let signer_plain = bundle(0x20f);
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), "f".to_owned()).unwrap();
        let info_floodfill = signer_floodfill
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign floodfill");
        let info_plain = signer_plain
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign plain");
        let validated_floodfill = ValidatedRouterInfo::from_router_info(
            info_floodfill,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate floodfill");
        let validated_plain = ValidatedRouterInfo::from_router_info(
            info_plain,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate plain");
        let mut store = RouterInfoStore::default();
        store.insert(validated_floodfill);
        store.insert(validated_plain);
        let advertisers: Vec<_> = store.floodfill_advertisers().collect();
        assert_eq!(advertisers.len(), 1);
        assert!(advertisers[0].advertises_floodfill());
        let stats = store.stats();
        assert_eq!(stats.floodfill_advertiser_count, 1);
    }
}
