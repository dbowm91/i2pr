//! Plan 541's lane -- the router's floodfill selection must see the router's
//! floodfill advertisers.
//!
//! The router's **authoritative** floodfill selection store -- the
//! `DestinationTunnelCoordinator`'s store, which `candidate_hashes` and
//! `version_gated_candidate_hashes` read -- was fed
//! `Bootstrap::validated_router_info_snapshot()`, and that snapshot is the
//! **first 256 records in ascending `RouterHash` order**. That bound belongs
//! to the **group provider**'s RouterInfo supply. The floodfill advertisers
//! are distributed across the hash space, so the selection store held an
//! arbitrary subset of the NetDB -- typically no advertiser at all -- and the
//! router refused its own LeaseSet2 publication and its destination build
//! hops while its NetDB held thousands of floodfills.
//!
//! The lane proves what the selection **does**: it yields exactly the
//! advertisers the store's own contents imply, the index never drifts, an
//! advertiser **outside** the group provider's prefix is reachable through the
//! coordinator, and a peer that does not advertise `f` is never selected.
//!
//! The lane computes the advertiser set **independently**, by walking the
//! store's own iterator, so the lane's expectation and the store's own
//! bookkeeping are two different things that must agree.
//!
//! The lane cannot see the *shape* of the fix -- that the index is maintained
//! on every path that inserts or removes a record, and that the selection
//! walks the index rather than the record map. That is the guard's claim, and
//! the guard does not make this lane's claim.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value the lane draws comes from a seeded
//!   ChaCha8 stream; the peer identities are reproducible.
//! - **An explicit clock.** The lane never reads the wall clock: the
//!   publication timestamps and the prune's `now` are explicit values.
//! - **No environment variable, no `#[ignore]`, no stock peer.**
//! - **No unbounded waiting.** The lane never sleeps to make an assertion
//!   pass.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::bootstrap::MAX_GROUP_PROVIDER_ROUTER_INFOS;
use i2pr_daemon::destination_tunnels::DestinationTunnelCoordinator;
use i2pr_netdb::{
    DestinationHash, InsertOutcome, LookupPolicy, RouterHash, RouterInfoStore,
    RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext, router_hash_from_destination,
};
use i2pr_proto::{Date, Hash, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane's records are published around.
const NOW_MS: u64 = 1_700_000_000_000;

/// One deterministic test identity.
fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs one RouterInfo for `signer` at `published_ms`.
///
/// `caps` is the capability string the record advertises. `None` advertises no
/// capabilities at all, so the record does **not** advertise the `f`
/// capability.
fn sign(
    signer: &RouterIdentityBundle,
    published_ms: u64,
    caps: Option<&str>,
) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    if let Some(caps) = caps {
        options
            .insert("caps".to_owned(), caps.to_owned())
            .expect("insert caps");
    }
    let info = signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(published_ms)),
    )
    .expect("validate")
}

/// The advertiser set the store's **own contents** imply.
///
/// This is the independent truth: it walks the store's own iterator and
/// applies the capability predicate, so it never reads the store's index.
fn independent_advertisers(store: &RouterInfoStore) -> Vec<RouterHash> {
    store
        .iter()
        .filter(|(_, record)| record.advertises_floodfill())
        .map(|(key, _)| *key)
        .collect()
}

/// The keys the store still retains, in the store's own order.
fn retained(store: &RouterInfoStore) -> Vec<RouterHash> {
    store.iter().map(|(key, _)| *key).collect()
}

/// The group provider's bounded prefix: the first records in hash order.
fn group_prefix(store: &RouterInfoStore) -> Vec<RouterHash> {
    store
        .iter()
        .take(MAX_GROUP_PROVIDER_ROUTER_INFOS)
        .map(|(key, _)| *key)
        .collect()
}

/// The keys the store's selection yields.
fn selected(store: &RouterInfoStore) -> Vec<RouterHash> {
    store
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect()
}

/// A store with `count` records, every third one an advertiser.
fn store_with_advertisers(seed_base: u64, count: u64) -> RouterInfoStore {
    let mut store = RouterInfoStore::default();
    for index in 0..count {
        let caps = if index % 3 == 0 { Some("f") } else { None };
        assert_eq!(
            store.insert(sign(&make_bundle(seed_base + index), NOW_MS, caps)),
            InsertOutcome::Inserted
        );
    }
    store
}

#[test]
fn the_selection_yields_exactly_the_advertisers() {
    let store = store_with_advertisers(0x0054_0100, 40);
    let expected = independent_advertisers(&store);
    assert_eq!(
        expected.len(),
        14,
        "the lane's own advertiser set is not what it means"
    );
    assert_eq!(
        selected(&store),
        expected,
        "the selection is not the advertiser set the store's contents imply"
    );
}

#[test]
fn the_index_does_not_drift_on_an_insert() {
    let mut store = RouterInfoStore::default();
    let record = sign(&make_bundle(0x0054_0300), NOW_MS, Some("f"));
    let key = record.key();
    assert_eq!(store.insert(record), InsertOutcome::Inserted);
    assert_eq!(
        selected(&store),
        vec![key],
        "an inserted advertiser is not selected"
    );
}

#[test]
fn a_replacement_that_gains_the_capability_is_selected() {
    let bundle = make_bundle(0x0054_0400);
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&bundle, NOW_MS, None)),
        InsertOutcome::Inserted
    );
    assert_eq!(selected(&store).len(), 0);
    assert_eq!(
        store.insert(sign(&bundle, NOW_MS + 1, Some("f"))),
        InsertOutcome::Replaced
    );
    assert_eq!(
        selected(&store).len(),
        1,
        "a replacement that gained the capability is not selected"
    );
    assert_eq!(store.stats().floodfill_advertiser_count, 1);
}

#[test]
fn a_replacement_that_loses_the_capability_is_not_selected() {
    let bundle = make_bundle(0x0054_0500);
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&bundle, NOW_MS, Some("f"))),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.insert(sign(&bundle, NOW_MS + 1, None)),
        InsertOutcome::Replaced
    );
    assert_eq!(
        selected(&store).len(),
        0,
        "a replacement that lost the capability is still selected"
    );
    assert_eq!(store.stats().floodfill_advertiser_count, 0);
}

#[test]
fn a_removal_drops_the_advertiser_from_the_selection() {
    let mut store = RouterInfoStore::default();
    let record = sign(&make_bundle(0x0054_0600), NOW_MS, Some("f"));
    let key = record.key();
    assert_eq!(store.insert(record), InsertOutcome::Inserted);
    assert_eq!(selected(&store).len(), 1);
    assert!(store.remove(&key));
    assert_eq!(
        selected(&store).len(),
        0,
        "a removed advertiser is still selected"
    );
    assert_eq!(store.stats().floodfill_advertiser_count, 0);
}

#[test]
fn the_snapshot_carries_the_advertisers_and_not_the_prefix() {
    let store = store_with_advertisers(0x0054_0700, 40);
    let snapshot: Vec<RouterHash> = store
        .floodfill_advertiser_snapshot()
        .iter()
        .map(|record| record.key())
        .collect();
    assert_eq!(
        snapshot,
        independent_advertisers(&store),
        "the snapshot is not the advertiser set the store's contents imply"
    );
    assert_ne!(
        snapshot,
        group_prefix(&store),
        "the snapshot is the group provider's prefix"
    );
}

#[test]
fn an_advertiser_outside_the_prefix_is_still_reachable() {
    // More records than the group provider's bound, and the advertiser is
    // the **highest** hash among them, so the prefix provably excludes it.
    const RECORDS: u64 = MAX_GROUP_PROVIDER_ROUTER_INFOS as u64 + 44;
    let mut store = RouterInfoStore::default();
    let mut bundles = Vec::new();
    for index in 0..RECORDS {
        let bundle = make_bundle(0x0054_0800 + index);
        let record = sign(&bundle, NOW_MS, None);
        let key = record.key();
        assert_eq!(store.insert(record), InsertOutcome::Inserted);
        bundles.push((bundle, key));
    }
    let highest = retained(&store)
        .into_iter()
        .max()
        .expect("the store has records");
    let bundle = bundles
        .into_iter()
        .find(|(_, key)| *key == highest)
        .map(|(bundle, _)| bundle)
        .expect("the highest hash is one of the inserted records");

    // The highest hash now advertises `f`.
    assert_eq!(
        store.insert(sign(&bundle, NOW_MS + 1, Some("f"))),
        InsertOutcome::Replaced
    );
    assert_eq!(
        selected(&store),
        vec![highest],
        "the advertiser outside the prefix is not selected"
    );
    assert!(
        !group_prefix(&store).contains(&highest),
        "the advertiser is inside the group provider's prefix"
    );
    let snapshot: Vec<RouterHash> = store
        .floodfill_advertiser_snapshot()
        .iter()
        .map(|record| record.key())
        .collect();
    assert_eq!(
        snapshot,
        vec![highest],
        "the advertiser outside the prefix is not in the snapshot"
    );
}

#[test]
fn the_coordinator_selects_the_advertiser_it_was_fed() {
    let mut store = RouterInfoStore::default();
    let record = sign(&make_bundle(0x0054_0900), NOW_MS, Some("f"));
    let key = record.key();
    assert_eq!(store.insert(record), InsertOutcome::Inserted);

    // The coordinator is fed the **advertisers**, not the group provider's
    // prefix.
    let mut coordinator = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    coordinator.advance_time(NOW_MS);
    for advertiser in store.floodfill_advertiser_snapshot() {
        coordinator.insert_validated_router_info(advertiser);
    }

    let target = DestinationHash::from_hash(Hash::from_bytes([0x11u8; 32]));
    let routing_key = router_hash_from_destination(target);
    let chosen = coordinator.candidate_hashes(&target, &routing_key);
    assert!(
        chosen.contains(&key),
        "the coordinator does not select the advertiser it was fed"
    );
}

#[test]
fn the_selection_order_is_unchanged() {
    let store = store_with_advertisers(0x0054_0a00, 20);
    let order = selected(&store);
    let mut ascending = order.clone();
    ascending.sort();
    assert_eq!(
        order, ascending,
        "the selection's order is not ascending by router hash"
    );
}

#[test]
fn a_non_advertiser_is_never_selected() {
    let mut store = RouterInfoStore::default();
    let plain = sign(&make_bundle(0x0054_0b00), NOW_MS, None);
    let key = plain.key();
    assert_eq!(store.insert(plain), InsertOutcome::Inserted);
    assert_eq!(
        selected(&store).len(),
        0,
        "a peer that does not advertise `f` was selected"
    );
    assert!(
        !store
            .floodfill_advertiser_snapshot()
            .iter()
            .any(|record| record.key() == key),
        "a peer that does not advertise `f` reached the snapshot"
    );
}
