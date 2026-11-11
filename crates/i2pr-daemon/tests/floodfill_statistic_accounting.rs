//! Plan 538's lane -- the NetDB's floodfill advertiser count must be
//! **accounted**, not derived by a scan of every record.
//!
//! The router's validated `RouterInfo` store already maintains its **encoded
//! byte total** incrementally, on exactly the paths that insert and remove a
//! record. The floodfill advertiser count -- the second number the same struct
//! reports about the same set -- was **derived** instead: `stats()` walked
//! every record on every call. The daemon's bootstrap reads that statistic
//! while holding the router's NetDB lock, and read it **twice** per state
//! computation, because `compute_state` refreshed the snapshot (which reads
//! the statistics) and then read them again.
//!
//! The lane proves the accounting against an **independent** count: the count
//! the store's own contents imply, computed by walking the store's iterator
//! rather than by reading the reported value. Every mutation path is covered
//! -- the new-key insert, the idempotent insert, the conflict, the stale
//! insert, the capacity refusal, the replacement in both directions, the
//! removal and the prune -- and the count must equal the independent count
//! after each one.
//!
//! The lane also proves that the store's **selection** surface is unchanged
//! (`floodfill_advertisers()` still yields exactly the advertisers, `iter()`
//! still yields every record), and that the daemon's bootstrap reports the
//! same count and reaches the same `BootstrapState`.
//!
//! The lane cannot see the *shape* of the fix -- that `stats()` walks no
//! record map, that the bootstrap computes the statistics once, that the
//! count is maintained on the store's own paths. That is the guard's claim,
//! and the guard does not make this lane's claim.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::bootstrap::{Bootstrap, BootstrapPolicy, BootstrapState};
use i2pr_daemon::config::ReseedConfig;
use i2pr_netdb::{
    InsertOutcome, LocalRouterInfoBuilder, RouterInfoStore, RouterInfoStoreConfig,
    ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{Date, Mapping};
use i2pr_storage::IdentityStore;
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

/// The count the store **reports**.
fn reported(store: &RouterInfoStore) -> usize {
    store.stats().floodfill_advertiser_count
}

/// The count the store's **contents** imply.
///
/// This is the independent truth: it walks the store's own iterator and
/// counts the records that advertise the `f` capability, so it never reads
/// the value the store reports.
fn independent(store: &RouterInfoStore) -> usize {
    store
        .iter()
        .filter(|(_, record)| record.advertises_floodfill())
        .count()
}

/// Asserts that the reported count is the independent count.
fn assert_accounted(store: &RouterInfoStore, expected: usize) {
    assert_eq!(
        reported(store),
        independent(store),
        "the reported count disagrees with the store's contents"
    );
    assert_eq!(reported(store), expected, "the reported count is not exact");
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

#[test]
fn a_new_key_insert_accounts_the_advertiser() {
    let signer = make_bundle(0x0005_3801);
    let mut store = RouterInfoStore::default();
    let record = sign(&signer, 1, Some("f"));
    assert_eq!(store.insert(record), InsertOutcome::Inserted);
    assert_accounted(&store, 1);
}

#[test]
fn a_record_without_the_capability_accounts_nothing() {
    let signer = make_bundle(0x0005_3802);
    let mut store = RouterInfoStore::default();
    let record = sign(&signer, 1, None);
    assert_eq!(store.insert(record), InsertOutcome::Inserted);
    assert_accounted(&store, 0);
}

#[test]
fn an_idempotent_insert_leaves_the_count_unchanged() {
    let signer = make_bundle(0x0005_3803);
    let mut store = RouterInfoStore::default();
    let record = sign(&signer, 1, Some("f"));
    assert_eq!(store.insert(record.clone()), InsertOutcome::Inserted);
    assert_eq!(store.insert(record), InsertOutcome::Idempotent);
    assert_accounted(&store, 1);
}

#[test]
fn a_conflicting_insert_leaves_the_count_unchanged() {
    let signer = make_bundle(0x0005_3804);
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&signer, 1, Some("f"))),
        InsertOutcome::Inserted
    );
    // The same publication with different signed bytes is a conflict. The
    // existing record is retained, so the count must not move.
    assert_eq!(
        store.insert(sign(&signer, 1, None)),
        InsertOutcome::Conflict
    );
    assert_accounted(&store, 1);
}

#[test]
fn a_stale_insert_leaves_the_count_unchanged() {
    let signer = make_bundle(0x0005_3805);
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&signer, 5, Some("f"))),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.insert(sign(&signer, 4, None)),
        InsertOutcome::StaleReplacement
    );
    assert_accounted(&store, 1);
}

#[test]
fn a_capacity_refusal_leaves_the_count_unchanged() {
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(1, 1 << 20));
    assert_eq!(
        store.insert(sign(&make_bundle(0x0005_3806), 1, Some("f"))),
        InsertOutcome::Inserted
    );
    // The store is at its record ceiling, so the second identity is refused
    // without mutating anything.
    assert_eq!(
        store.insert(sign(&make_bundle(0x0005_3807), 1, Some("f"))),
        InsertOutcome::CapacityExceeded
    );
    assert_accounted(&store, 1);
}

#[test]
fn a_replacement_that_gains_the_capability_accounts_the_advertiser() {
    let signer = make_bundle(0x0005_3808);
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&signer, 1, None)),
        InsertOutcome::Inserted
    );
    assert_accounted(&store, 0);
    // The same identity, published later and advertising the `f`
    // capability, replaces the earlier record.
    assert_eq!(
        store.insert(sign(&signer, 2, Some("f"))),
        InsertOutcome::Replaced
    );
    assert_accounted(&store, 1);
}

#[test]
fn a_replacement_that_loses_the_capability_releases_the_advertiser() {
    let signer = make_bundle(0x0005_3809);
    let mut store = RouterInfoStore::default();
    assert_eq!(
        store.insert(sign(&signer, 1, Some("f"))),
        InsertOutcome::Inserted
    );
    assert_accounted(&store, 1);
    assert_eq!(
        store.insert(sign(&signer, 2, None)),
        InsertOutcome::Replaced
    );
    assert_accounted(&store, 0);
}

#[test]
fn a_removal_releases_the_advertiser() {
    let signer = make_bundle(0x0005_3810);
    let mut store = RouterInfoStore::default();
    let record = sign(&signer, 1, Some("f"));
    let key = record.key();
    assert_eq!(store.insert(record), InsertOutcome::Inserted);
    assert_accounted(&store, 1);
    assert!(store.remove(&key));
    assert_accounted(&store, 0);
    // Removing an unknown key changes nothing.
    assert!(!store.remove(&key));
    assert_accounted(&store, 0);
}

#[test]
fn a_prune_that_removes_several_records_releases_every_advertiser() {
    let mut store = RouterInfoStore::default();
    for seed in 0x0005_3811..0x0005_3815 {
        assert_eq!(
            store.insert(sign(&make_bundle(seed), 1, Some("f"))),
            InsertOutcome::Inserted
        );
    }
    assert_eq!(
        store.insert(sign(&make_bundle(0x0005_3815), 900, Some("f"))),
        InsertOutcome::Inserted
    );
    assert_accounted(&store, 5);
    // Four records are older than the window; the fifth is inside it.
    let removed = store.prune(Date::from_millis(1000), 500);
    assert_eq!(removed, 4);
    assert_accounted(&store, 1);
}

#[test]
fn the_count_is_exact_for_a_large_population() {
    const POPULATION: u64 = 200;
    let mut store = RouterInfoStore::default();
    for seed in 0x0053_8100..0x0053_8100 + POPULATION {
        let caps = if seed % 2 == 0 { Some("f") } else { None };
        assert_eq!(
            store.insert(sign(&make_bundle(seed), 1, caps)),
            InsertOutcome::Inserted
        );
    }
    // The count is exact, not a sample and not a bound.
    assert_accounted(&store, (POPULATION / 2) as usize);
    assert_eq!(store.stats().record_count, POPULATION as usize);
}

#[test]
fn the_selection_surface_is_unchanged() {
    let mut store = RouterInfoStore::default();
    for seed in 0x0053_8200..0x0053_8205 {
        let caps = if seed % 2 == 0 { Some("f") } else { None };
        assert_eq!(
            store.insert(sign(&make_bundle(seed), 1, caps)),
            InsertOutcome::Inserted
        );
    }
    // The selection surface yields exactly the advertisers ...
    let advertisers: Vec<_> = store
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect();
    assert_eq!(advertisers.len(), independent(&store));
    for key in &advertisers {
        let record = store.get(key).expect("advertiser present");
        assert!(record.advertises_floodfill());
    }
    // ... and the iterator still yields every record.
    assert_eq!(store.iter().count(), store.stats().record_count);
}

#[test]
fn the_bootstrap_reports_the_same_count() {
    let mut bootstrap = Bootstrap::new(RouterInfoStoreConfig::default(), reseed());
    for seed in 0x0053_8300..0x0053_8305 {
        let caps = if seed % 2 == 0 { Some("f") } else { None };
        assert_eq!(
            bootstrap
                .store_mut()
                .insert(sign(&make_bundle(seed), 1, caps)),
            InsertOutcome::Inserted
        );
    }
    // The prune is the production path that refreshes the bootstrap's
    // snapshot, so the snapshot's counts are the store's counts.
    bootstrap.prune(1_000, 60_000);
    let snapshot = bootstrap.snapshot();
    assert_eq!(snapshot.record_count, 5);
    assert_eq!(snapshot.floodfill_advertisers, 3);
    let diagnostics = bootstrap.diagnostics();
    assert_eq!(diagnostics.floodfill_advertisers, 3);
}

#[test]
fn the_bootstrap_reaches_the_same_state() {
    let directory = tempfile::tempdir().expect("temp dir");
    let data_dir = directory.path().join("state");
    IdentityStore::prepare_directory(&data_dir).expect("prepare data dir");
    let identity = make_bundle(0x0053_8999);
    IdentityStore::in_data_dir(&data_dir)
        .save_new(&identity)
        .expect("save identity");
    let builder = LocalRouterInfoBuilder::new(&identity);

    let mut bootstrap = Bootstrap::new(RouterInfoStoreConfig::default(), reseed());
    for seed in 0x0053_8400..0x0053_8402 {
        assert_eq!(
            bootstrap
                .store_mut()
                .insert(sign(&make_bundle(seed), 1, Some("f"))),
            InsertOutcome::Inserted
        );
    }
    let policy = BootstrapPolicy {
        min_router_infos: 1,
        min_floodfill_advertisers: 2,
        reseed_enabled: false,
    };
    let report = bootstrap
        .run(&data_dir, &builder, policy, 1)
        .expect("bootstrap");
    // The state decision reads the floodfill advertiser count, and the count
    // is exact, so the decision is the one the tree already made.
    assert_eq!(
        report.final_state,
        BootstrapState::ReadyForNetworkIntegration
    );
    assert_eq!(report.snapshot.floodfill_advertisers, 2);
    assert_eq!(report.snapshot.record_count, 2);
}
