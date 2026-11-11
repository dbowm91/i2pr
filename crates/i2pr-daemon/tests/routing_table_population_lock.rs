//! Plan 539's lane -- the routing table's population must not hold the
//! router's NetDB lock.
//!
//! The composition root populated the router's **routing table** by walking
//! every floodfill advertiser the NetDB holds -- an O(population) walk -- and
//! it did that walk **under the router's NetDB lock**. Worse, the walk's body
//! called the routing table's own `index`, which takes the routing table's
//! own mutex, so the router's NetDB lock **wrapped** the routing table's
//! mutex, for the whole population, on the router's **live** maintenance
//! cadence.
//!
//! The population is now read through a **source**, and the source releases
//! the router's NetDB lock **before the read returns**. The lane proves that
//! behaviourally, and it proves it without a timing assertion:
//!
//! - after the read returns, the bootstrap's lock is **free** -- the lane
//!   takes it with `try_lock`, which fails immediately rather than blocking
//!   if the guard were still held;
//! - the returned advertisers index into the routing table **while the
//!   bootstrap's lock is held**, so the indexing cannot need that lock;
//! - a **poisoned** lock is the same typed refusal, which is what proves the
//!   read goes through the lock at all;
//! - the population's **absence** is a typed refusal that is **counted**,
//!   never a silent skip;
//! - the population is the **same set** the store's own selection yields, and
//!   the table still refuses a peer that does not advertise `f`.
//!
//! The lane cannot see the composition root's private population, so the
//! **shape** -- that the composition root holds no NetDB lock across its
//! indexing -- is the guard's claim, and the guard does not make this lane's
//! claim.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;
use i2pr_daemon::netdb_participation::{
    RoutingPopulationRefusal, RoutingTableConfig, RoutingTableConsumer, RoutingTableDriver,
    RoutingTablePopulationSource, ValidatedStorePopulationSource,
};
use i2pr_netdb::{
    InsertOutcome, RouterHash, RouterInfoStore, RouterInfoStoreConfig, TableInsertOutcome,
    ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// One deterministic test identity.
fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs one RouterInfo for `signer` at `published_ms`.
///
/// `caps` is the capability string the record advertises. `None` advertises
/// no capabilities at all, so the record does **not** advertise the `f`
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

/// A reseed configuration that never runs.
fn reseed() -> ReseedConfig {
    ReseedConfig {
        enabled: false,
        max_sources: 0,
        max_su3_bytes: 0,
        sources: Vec::new(),
    }
}

/// A bootstrap holding `advertisers` floodfills and `others` plain records.
///
/// The plain records do **not** advertise the `f` capability, so they must
/// never appear in the population.
fn bootstrap_with(advertisers: u64, others: u64) -> Arc<Mutex<Bootstrap>> {
    let mut bootstrap = Bootstrap::new(RouterInfoStoreConfig::default(), reseed());
    for index in 0..advertisers {
        assert_eq!(
            bootstrap
                .store_mut()
                .insert(sign(&make_bundle(0x0053_9400 + index), 1, Some("f"))),
            InsertOutcome::Inserted
        );
    }
    for index in 0..others {
        assert_eq!(
            bootstrap
                .store_mut()
                .insert(sign(&make_bundle(0x0053_9500 + index), 1, None)),
            InsertOutcome::Inserted
        );
    }
    Arc::new(Mutex::new(bootstrap))
}

/// An enabled routing table.
fn a_table() -> RoutingTableConsumer {
    RoutingTableConsumer::new(
        RoutingTableDriver::new(
            RouterHash::from_bytes([0x52; 32]),
            Date::from_millis(1),
            RoutingTableConfig::default(),
        ),
        true,
    )
}

#[test]
fn the_source_reads_the_advertisers() {
    let bootstrap = bootstrap_with(3, 2);
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    let advertisers = source.advertisers().expect("advertisers");
    assert_eq!(advertisers.len(), 3);
    for record in &advertisers {
        assert!(
            record.advertises_floodfill(),
            "the population holds only advertisers"
        );
    }
}

#[test]
fn the_read_releases_the_lock_before_it_returns() {
    let bootstrap = bootstrap_with(2, 0);
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    let _ = source.advertisers().expect("advertisers");
    // `try_lock` fails **immediately** when the lock is held, so this is a
    // deterministic proof that the guard was dropped before the read
    // returned -- and not a timing assertion.
    let guard = bootstrap
        .try_lock()
        .expect("the read released the lock before it returned");
    drop(guard);
}

#[test]
fn the_indexing_needs_no_netdb_lock() {
    let bootstrap = bootstrap_with(3, 1);
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    let advertisers = source.advertisers().expect("advertisers");
    assert_eq!(advertisers.len(), 3);

    // The NetDB lock is held for the whole indexing. The material the read
    // returned is owned, so the indexing cannot need that lock -- and if it
    // did, this would deadlock rather than pass.
    let guard = bootstrap.lock().expect("the lock");
    let table = a_table();
    for record in &advertisers {
        let outcome = table.index(record).expect("the table is enabled");
        assert!(outcome.is_indexed(), "the advertiser must be indexed");
    }
    assert_eq!(table.len(), 3);
    drop(guard);
}

#[test]
fn an_absent_store_is_a_typed_refusal() {
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(None, &refusals);
    assert_eq!(
        source.advertisers(),
        Err(RoutingPopulationRefusal::StoreUnavailable)
    );
    assert_eq!(
        refusals.load(Ordering::Relaxed),
        1,
        "the refusal is counted under its own reason"
    );
}

#[test]
fn a_poisoned_lock_is_the_same_typed_refusal() {
    let bootstrap = bootstrap_with(1, 0);
    let poisoned = Arc::clone(&bootstrap);
    let _ = std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("lock");
        panic!("poison the bootstrap's lock");
    })
    .join();
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    assert_eq!(
        source.advertisers(),
        Err(RoutingPopulationRefusal::StoreUnavailable)
    );
    assert_eq!(refusals.load(Ordering::Relaxed), 1);
}

#[test]
fn the_population_is_the_same_set() {
    let bootstrap = bootstrap_with(3, 2);
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    let advertisers = source.advertisers().expect("advertisers");

    // The store's own selection is the same set, in the same order.
    let expected: Vec<RouterHash> = bootstrap
        .lock()
        .expect("lock")
        .store()
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect();
    let actual: Vec<RouterHash> = advertisers.iter().map(|record| record.key()).collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 3);
}

#[test]
fn the_table_refuses_a_peer_that_does_not_advertise_f() {
    let table = a_table();
    let record = sign(&make_bundle(0x0053_9601), 1, None);
    let outcome = table.index(&record).expect("the table is enabled");
    assert_eq!(outcome, TableInsertOutcome::RejectedNotFloodfill);
    assert!(table.is_empty(), "the refused peer is not in the table");
}

#[test]
fn the_population_mutates_nothing() {
    let bootstrap = bootstrap_with(3, 2);
    let refusals = AtomicU32::new(0);
    let source = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    let before = bootstrap.lock().expect("lock").store().stats();
    let _ = source.advertisers().expect("advertisers");
    let after = bootstrap.lock().expect("lock").store().stats();
    assert_eq!(before, after, "the read mutates nothing in the store");
}

#[test]
fn the_store_is_its_own_source() {
    let mut store = RouterInfoStore::default();
    for index in 0..4u64 {
        let caps = if index % 2 == 0 { Some("f") } else { None };
        assert_eq!(
            store.insert(sign(&make_bundle(0x0053_9700 + index), 1, caps)),
            InsertOutcome::Inserted
        );
    }
    // The trait is implemented **for the store itself**, so a lane that
    // passes a `&store` still compiles and still passes.
    let source: &dyn RoutingTablePopulationSource = &store;
    let advertisers = source.advertisers().expect("the store can always answer");
    assert_eq!(advertisers.len(), 2);
}

#[test]
fn every_refusal_is_counted_under_its_own_reason() {
    let refusals = AtomicU32::new(0);

    let absent = ValidatedStorePopulationSource::new(None, &refusals);
    assert!(absent.advertisers().is_err());
    assert_eq!(refusals.load(Ordering::Relaxed), 1);

    let bootstrap = bootstrap_with(1, 0);
    let present = ValidatedStorePopulationSource::new(Some(&bootstrap), &refusals);
    assert!(present.advertisers().is_ok());
    assert_eq!(
        refusals.load(Ordering::Relaxed),
        1,
        "a successful read counts nothing"
    );

    let absent_again = ValidatedStorePopulationSource::new(None, &refusals);
    assert!(absent_again.advertisers().is_err());
    assert_eq!(refusals.load(Ordering::Relaxed), 2);
}
