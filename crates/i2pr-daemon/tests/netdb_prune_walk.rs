//! Plan 540's lane -- the NetDB's prune must not walk the population.
//!
//! The validated store's prune finds the stale records by walking **every**
//! retained record, and the daemon's peer reconcile calls it **under the
//! router's NetDB lock** -- the bootstrap's mutex, which *is* the router's
//! NetDB lock. The prune is on the router's live maintenance cadence, so the
//! lock was held across an O(population) walk and every other NetDB reader
//! queued behind it.
//!
//! The store now maintains an **age index** -- the retained records'
//! publication times, ordered -- so the prune walks only the **stale**
//! records. The lane proves what the prune *does*: it removes exactly the
//! stale set, it removes every stale record in one call, it removes nothing
//! when nothing is stale, the index never drifts, and the accounting and the
//! store's other surfaces survive the prune.
//!
//! The lane computes the stale set **independently**, by walking the store's
//! own iterator, so the lane's expectation and the store's own bookkeeping are
//! two different things that must agree.
//!
//! The lane cannot see the *shape* of the fix -- that the prune walks the
//! index and not the record map, and that the index is maintained on every
//! path that inserts or removes a record. That is the guard's claim, and the
//! guard does not make this lane's claim.
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
use i2pr_netdb::{
    InsertOutcome, RouterHash, RouterInfoStore, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane's records are published around.
const NOW_MS: u64 = 1_700_000_000_000;

/// The retention window the lane prunes with.
const WINDOW_MS: u64 = 30 * 60 * 1_000;

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

/// The stale set the store's **own contents** imply.
///
/// This is the independent truth: it walks the store's own iterator and
/// applies the prune's own predicate, so it never reads the store's index.
fn independent_stale(store: &RouterInfoStore, now_ms: u64, window_ms: u64) -> Vec<RouterHash> {
    store
        .iter()
        .filter(|(_, record)| now_ms.saturating_sub(record.published().as_millis()) > window_ms)
        .map(|(key, _)| *key)
        .collect()
}

/// The keys the store still retains.
fn retained(store: &RouterInfoStore) -> Vec<RouterHash> {
    store.iter().map(|(key, _)| *key).collect()
}

/// The count the store's **contents** imply.
fn independent_advertisers(store: &RouterInfoStore) -> usize {
    store
        .iter()
        .filter(|(_, record)| record.advertises_floodfill())
        .count()
}

/// A store with `fresh` fresh records and `stale` stale ones.
///
/// The fresh records are published inside the window and the stale ones
/// outside it, so the two sets are disjoint and both are known.
fn store_with(fresh: u64, stale: u64) -> RouterInfoStore {
    let mut store = RouterInfoStore::default();
    for index in 0..fresh {
        assert_eq!(
            store.insert(sign(
                &make_bundle(0x0054_0100 + index),
                NOW_MS - WINDOW_MS / 2,
                Some("f")
            )),
            InsertOutcome::Inserted
        );
    }
    for index in 0..stale {
        assert_eq!(
            store.insert(sign(
                &make_bundle(0x0054_0200 + index),
                NOW_MS - WINDOW_MS * 2,
                None
            )),
            InsertOutcome::Inserted
        );
    }
    store
}

#[test]
fn the_prune_removes_exactly_the_stale_set() {
    let mut store = store_with(40, 7);
    let expected = independent_stale(&store, NOW_MS, WINDOW_MS);
    assert_eq!(
        expected.len(),
        7,
        "the lane's own stale set is not what it means"
    );

    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, expected.len(), "the prune removed the wrong count");

    // Every stale record is gone ...
    for key in &expected {
        assert!(!store.contains(key), "a stale record survived the prune");
    }
    // ... and every fresh one is still here.
    let survivors = retained(&store);
    assert_eq!(survivors.len(), 40, "the prune removed a fresh record");
}

#[test]
fn the_prune_removes_every_stale_record_in_one_call() {
    // A population far larger than the number of stale records: the prune
    // removes every stale record in **one** call, not a slice at a time.
    let mut store = store_with(200, 5);
    assert_eq!(store.len(), 205);

    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, 5, "the prune did not remove every stale record");
    assert_eq!(store.len(), 200, "the store kept a stale record");
    assert_eq!(independent_stale(&store, NOW_MS, WINDOW_MS).len(), 0);
}

#[test]
fn the_prune_removes_nothing_when_nothing_is_stale() {
    let mut store = store_with(20, 0);
    let before = retained(&store);
    let bytes_before = store.encoded_bytes();
    let count_before = store.stats().floodfill_advertiser_count;

    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, 0, "the prune removed something it should not have");
    assert_eq!(retained(&store), before, "the prune changed the contents");
    assert_eq!(store.encoded_bytes(), bytes_before);
    assert_eq!(store.stats().floodfill_advertiser_count, count_before);
}

#[test]
fn the_index_does_not_drift_on_an_insert() {
    // A record that is inserted and then becomes stale **is** removed: the
    // index gained the record when it was inserted.
    let mut store = RouterInfoStore::default();
    let record = sign(&make_bundle(0x0054_0300), NOW_MS, Some("f"));
    let key = record.key();
    assert_eq!(store.insert(record), InsertOutcome::Inserted);

    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, 0, "a fresh record was pruned");

    // The same record, now past the window.
    let later = NOW_MS + WINDOW_MS * 2;
    let removed = store.prune(Date::from_millis(later), WINDOW_MS);
    assert_eq!(removed, 1, "the index missed the inserted record");
    assert!(!store.contains(&key));
}

#[test]
fn a_replacement_is_removed_at_its_new_time() {
    // A replacement may carry a different publication time, so the index must
    // follow the record: the old entry goes and the new one comes.
    let bundle = make_bundle(0x0054_0400);
    let old_ms = NOW_MS - WINDOW_MS * 2;
    let new_ms = NOW_MS - WINDOW_MS / 2;
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&bundle, old_ms, Some("f"))),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.insert(sign(&bundle, new_ms, Some("f"))),
        InsertOutcome::Replaced
    );

    // At the old time the record would have been stale; at its **new** time
    // it is not.
    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(
        removed, 0,
        "the index kept the replaced record's old publication time"
    );
    assert_eq!(store.len(), 1);

    // And it is removed once the **new** time is past the window.
    let removed = store.prune(Date::from_millis(NOW_MS + WINDOW_MS * 2), WINDOW_MS);
    assert_eq!(removed, 1, "the index missed the replaced record");
}

#[test]
fn a_record_published_in_the_future_is_never_stale() {
    // The age is a **saturating** subtraction, so a record published in the
    // future has an age of zero and is not stale.
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&make_bundle(0x0054_0500), NOW_MS + 10_000, Some("f"))),
        InsertOutcome::Inserted
    );

    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, 0, "a record published in the future was pruned");
    assert_eq!(store.len(), 1);
}

#[test]
fn a_now_below_the_window_removes_nothing() {
    // A `now` below the window saturates to zero, and no record is older
    // than that.
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&make_bundle(0x0054_0600), 0, Some("f"))),
        InsertOutcome::Inserted
    );

    let removed = store.prune(Date::from_millis(100), 1_000);
    assert_eq!(removed, 0, "a prune below the window removed a record");
    assert_eq!(store.len(), 1);
}

#[test]
fn the_prune_releases_the_bytes_and_the_advertiser() {
    // The prune still releases the record's bytes and its advertiser with the
    // record.
    let mut store = RouterInfoStore::default();
    let advertiser = sign(&make_bundle(0x0054_0700), NOW_MS - WINDOW_MS * 2, Some("f"));
    let bytes = advertiser.encoded_len();
    let plain = sign(&make_bundle(0x0054_0701), NOW_MS - WINDOW_MS * 2, None);
    let plain_bytes = plain.encoded_len();
    assert_eq!(store.insert(advertiser), InsertOutcome::Inserted);
    assert_eq!(store.insert(plain), InsertOutcome::Inserted);

    let bytes_before = store.encoded_bytes();
    let count_before = store.stats().floodfill_advertiser_count;
    assert_eq!(count_before, 1);

    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, 2);
    assert_eq!(
        store.encoded_bytes(),
        bytes_before - bytes - plain_bytes,
        "the prune did not release the records' bytes"
    );
    assert_eq!(
        store.stats().floodfill_advertiser_count,
        count_before - 1,
        "the prune did not release the advertiser"
    );
}

#[test]
fn the_statistics_and_the_selection_survive_the_prune() {
    let mut store = store_with(30, 4);
    let removed = store.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed, 4);

    // The statistics still agree with the store's own contents ...
    assert_eq!(
        store.stats().floodfill_advertiser_count,
        independent_advertisers(&store),
        "the reported count disagrees with the store's contents"
    );
    assert_eq!(store.stats().record_count, store.len());
    // ... and the selection still yields exactly the advertisers.
    let advertisers: Vec<_> = store
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect();
    assert_eq!(advertisers.len(), independent_advertisers(&store));
    assert_eq!(advertisers.len(), 30);
}

#[test]
fn the_prune_is_deterministic() {
    // Two identical stores prune to the same result.
    let mut first = store_with(15, 3);
    let mut second = store_with(15, 3);

    let removed_first = first.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    let removed_second = second.prune(Date::from_millis(NOW_MS), WINDOW_MS);
    assert_eq!(removed_first, removed_second);
    assert_eq!(retained(&first), retained(&second));
}
