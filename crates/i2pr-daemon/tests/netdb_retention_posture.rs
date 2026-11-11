//! Plan 542's lane -- the NetDB's retention must not depend on the
//! service-tunnel posture.
//!
//! `RouterInfoStore::insert` **refuses** at the quota and **never evicts**
//! (`crates/i2pr-netdb/src/store.rs:211`), and the tree's own documentation of
//! the prune states the consequence in as many words: *"a NetDB that filled up
//! would refuse every new peer, permanently, and would keep a record the
//! router's own validation policy would refuse to accept today"*
//! (`crates/i2pr-daemon/src/bootstrap.rs:283`).
//!
//! The operation that prevents that -- `Bootstrap::prune` -- had exactly **one**
//! production caller, and that caller is composed only when a service-tunnel
//! group is enabled. In the posture where the router runs SSU2 and nothing
//! else, the NetDB was **write-only**: it filled to its quota and then refused
//! every subsequent peer for as long as the router ran. The prune was not
//! narrowed; it was **unreachable**.
//!
//! The lane proves what the retention step **does**: a store at its quota
//! refuses a peer, the step removes exactly the stale records, the peer is
//! accepted afterwards, a record inside the window survives, the step is
//! idempotent, a lock that cannot be taken is counted rather than reported as a
//! prune, and the router's own `RouterInfo` is never a victim.
//!
//! The lane computes the stale set **independently**, by walking the store's own
//! iterator, so the lane's expectation and the store's own bookkeeping are two
//! different things that must agree.
//!
//! The lane cannot see the step's **reachability in the daemon's graph** -- it
//! constructs the owner directly. That is the guard's claim, and the guard does
//! not make this lane's claim. Plan 507's lane had exactly this shape and
//! constructed `PeerReconcile` by hand, which is why the reachability defect
//! survived it; Plan 542's guard, not its lane, is the authority for
//! reachability.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value comes from a seeded ChaCha8 stream.
//! - **An explicit clock.** The lane never reads the wall clock.
//! - **No environment variable, no `#[ignore]`, no stock peer.**
//! - **No unbounded waiting.** The lane never sleeps to make an assertion pass;
//!   the one async row uses `start_paused` and an explicit bounded deadline.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::Mutex;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;
use i2pr_daemon::netdb_participation::{
    NetDbRetentionCounters, NetDbRetentionExit, prune_netdb, run_netdb_retention,
};
use i2pr_netdb::{
    InsertOutcome, RouterHash, RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo,
    ValidationContext,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane's records are published around.
const NOW_MS: u64 = 1_700_000_000_000;

/// The retention window the step uses, read from the plan's own surface so the
/// lane cannot drift from the policy it is testing.
const WINDOW_MS: u64 = i2pr_daemon::peer_reconcile::RETENTION_WINDOW_MS;

/// One deterministic test identity.
fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs and validates one RouterInfo for `signer` at `published_ms`.
///
/// `context_ms` is the instant the record is **accepted**. A record the lane
/// wants to be stale later is validated at its own publication time, because
/// the router's acceptance window and its retention window are the **same**
/// value: a record older than the window is refused at insert, so a stale
/// record can only enter the store when it was accepted while it was fresh.
fn sign(signer: &RouterIdentityBundle, published_ms: u64, context_ms: u64) -> ValidatedRouterInfo {
    let info = signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            Mapping::builder().build().expect("options"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(context_ms)),
    )
    .expect("validate")
}

/// A bootstrap over a store with the supplied record ceiling.
fn bootstrap_with(max_records: usize) -> Bootstrap {
    let config = RouterInfoStoreConfig {
        max_records,
        ..RouterInfoStoreConfig::default()
    };
    Bootstrap::new(
        config,
        ReseedConfig {
            enabled: false,
            max_sources: 0,
            max_su3_bytes: 0,
            sources: Vec::new(),
        },
    )
}

/// The stale set the store's **own contents** imply, in the store's own order.
///
/// This is the independent truth: it walks the store's own iterator and applies
/// the window, so it never reads the age index the prune walks.
fn independent_stale(store: &RouterInfoStore, now_ms: u64, window_ms: u64) -> Vec<RouterHash> {
    store
        .iter()
        .filter(|(_, record)| now_ms.saturating_sub(record.published().as_millis()) > window_ms)
        .map(|(key, _)| *key)
        .collect()
}

/// The keys the store retains, in the store's own order.
fn retained(store: &RouterInfoStore) -> Vec<RouterHash> {
    store.iter().map(|(key, _)| *key).collect()
}

#[test]
fn the_store_at_its_quota_refuses_the_next_peer() {
    let mut bootstrap = bootstrap_with(3);
    for index in 0..3u64 {
        let record = sign(&make_bundle(0x0054_2100 + index), NOW_MS, NOW_MS);
        assert_eq!(
            bootstrap.store_mut().insert(record),
            InsertOutcome::Inserted
        );
    }
    let refused = sign(&make_bundle(0x0054_2200), NOW_MS, NOW_MS);
    assert_eq!(
        bootstrap.store_mut().insert(refused),
        InsertOutcome::CapacityExceeded,
        "a full store must refuse the next peer; the lane's premise is wrong"
    );
}

#[test]
fn the_step_removes_exactly_the_stale_records() {
    let mut bootstrap = bootstrap_with(16);
    // Four records well inside the window, four well past it.
    for index in 0..4u64 {
        let fresh = sign(&make_bundle(0x0054_2300 + index), NOW_MS, NOW_MS);
        assert_eq!(bootstrap.store_mut().insert(fresh), InsertOutcome::Inserted);
    }
    let stale_published = NOW_MS.saturating_sub(WINDOW_MS * 2);
    for index in 0..4u64 {
        let stale = sign(
            &make_bundle(0x0054_2400 + index),
            stale_published,
            stale_published,
        );
        assert_eq!(bootstrap.store_mut().insert(stale), InsertOutcome::Inserted);
    }

    let expected = independent_stale(bootstrap.store(), NOW_MS, WINDOW_MS);
    assert_eq!(
        expected.len(),
        4,
        "the lane's stale set is not what it means"
    );

    let removed = bootstrap.prune(NOW_MS, WINDOW_MS);
    assert_eq!(
        removed,
        expected.len(),
        "the step did not remove exactly the stale set"
    );
    let survivors = retained(bootstrap.store());
    assert_eq!(
        survivors.len(),
        4,
        "the step removed a record inside the window"
    );
    for key in &expected {
        assert!(!survivors.contains(key), "a stale record survived the step");
    }
}

#[test]
fn the_peer_the_store_refused_is_accepted_after_the_step() {
    let mut bootstrap = bootstrap_with(2);
    let stale_published = NOW_MS.saturating_sub(WINDOW_MS * 2);
    for index in 0..2u64 {
        let stale = sign(
            &make_bundle(0x0054_2500 + index),
            stale_published,
            stale_published,
        );
        assert_eq!(bootstrap.store_mut().insert(stale), InsertOutcome::Inserted);
    }
    let fresh = sign(&make_bundle(0x0054_2600), NOW_MS, NOW_MS);
    let key = fresh.key();
    assert_eq!(
        bootstrap.store_mut().insert(fresh.clone()),
        InsertOutcome::CapacityExceeded,
        "the store must refuse before the step, or the row proves nothing"
    );

    let removed = bootstrap.prune(NOW_MS, WINDOW_MS);
    assert_eq!(removed, 2, "the step did not make room");
    assert_eq!(
        bootstrap.store_mut().insert(fresh),
        InsertOutcome::Inserted,
        "the peer the store refused is still refused after the step"
    );
    assert!(retained(bootstrap.store()).contains(&key));
}

#[test]
fn a_record_inside_the_window_survives() {
    let mut bootstrap = bootstrap_with(8);
    // Exactly at the boundary: `now - published == window` is **inside** it.
    let at_boundary = sign(&make_bundle(0x0054_2700), NOW_MS - WINDOW_MS, NOW_MS);
    let key = at_boundary.key();
    assert_eq!(
        bootstrap.store_mut().insert(at_boundary),
        InsertOutcome::Inserted
    );
    assert_eq!(
        bootstrap.prune(NOW_MS, WINDOW_MS),
        0,
        "a record at the boundary was treated as stale"
    );
    assert!(
        retained(bootstrap.store()).contains(&key),
        "a record inside the window did not survive"
    );
}

#[test]
fn the_step_is_idempotent() {
    let mut bootstrap = bootstrap_with(8);
    let stale_published = NOW_MS.saturating_sub(WINDOW_MS * 2);
    for index in 0..3u64 {
        let stale = sign(
            &make_bundle(0x0054_2800 + index),
            stale_published,
            stale_published,
        );
        assert_eq!(bootstrap.store_mut().insert(stale), InsertOutcome::Inserted);
    }
    assert_eq!(bootstrap.prune(NOW_MS, WINDOW_MS), 3);
    let after_first = retained(bootstrap.store());
    assert_eq!(
        bootstrap.prune(NOW_MS, WINDOW_MS),
        0,
        "a second pass over a store with nothing stale removed something"
    );
    assert_eq!(
        retained(bootstrap.store()),
        after_first,
        "a second pass changed the store's contents"
    );
}

#[test]
fn the_step_counts_an_absent_store_rather_than_reporting_a_prune() {
    let outcome = prune_netdb(None, NOW_MS);
    assert_eq!(
        outcome,
        i2pr_daemon::peer_reconcile::PruneOutcome::NotApplicable,
        "an absent store must be NotApplicable, never a silent success"
    );
}

#[test]
fn a_poisoned_lock_is_counted_and_the_store_keeps_its_contents() {
    let mut bootstrap = bootstrap_with(8);
    let stale_published = NOW_MS.saturating_sub(WINDOW_MS * 2);
    let stale = sign(&make_bundle(0x0054_2900), stale_published, stale_published);
    assert_eq!(bootstrap.store_mut().insert(stale), InsertOutcome::Inserted);
    let before = retained(bootstrap.store());

    let handle = Arc::new(Mutex::new(bootstrap));
    // Poison the lock by panicking while it is held.
    let poisoned = Arc::clone(&handle);
    let _ = std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("the lock is not poisoned yet");
        panic!("poison the lock");
    })
    .join();

    let outcome = prune_netdb(Some(&handle), NOW_MS);
    assert_eq!(
        outcome,
        i2pr_daemon::peer_reconcile::PruneOutcome::Unavailable,
        "a poisoned lock must be counted unavailable"
    );
    let guard = handle.lock().unwrap_or_else(|error| error.into_inner());
    assert_eq!(
        retained(guard.store()),
        before,
        "the store did not keep its contents across an unavailable step"
    );
}

#[test]
fn the_routers_own_record_is_never_a_victim() {
    let mut bootstrap = bootstrap_with(8);
    // The router's own record is not in the store; the lane proves the step
    // never names the local hash at all, whatever the population.
    let stale_published = NOW_MS.saturating_sub(WINDOW_MS * 2);
    for index in 0..4u64 {
        let stale = sign(
            &make_bundle(0x0054_2a00 + index),
            stale_published,
            stale_published,
        );
        assert_eq!(bootstrap.store_mut().insert(stale), InsertOutcome::Inserted);
    }
    let local = bootstrap.local().map(|local| local.router_hash());
    let removed = bootstrap.prune(NOW_MS, WINDOW_MS);
    assert_eq!(removed, 4);
    if let Some(local) = local {
        assert!(
            bootstrap.store().get(&local).is_none(),
            "the step materialised the router's own record into the store"
        );
    }
    assert_eq!(
        bootstrap.store().len(),
        0,
        "the router's own record was not the store's to remove"
    );
}

#[tokio::test(start_paused = true)]
async fn the_cadence_runs_the_step_and_counts_it() {
    let mut bootstrap = bootstrap_with(4);
    let stale_published = NOW_MS.saturating_sub(WINDOW_MS * 4);
    for index in 0..2u64 {
        let stale = sign(
            &make_bundle(0x0054_2b00 + index),
            stale_published,
            stale_published,
        );
        assert_eq!(bootstrap.store_mut().insert(stale), InsertOutcome::Inserted);
    }

    let counters = Arc::new(NetDbRetentionCounters::default());
    let cancellation = i2pr_runtime::CancellationToken::new();
    let handle = Arc::new(Mutex::new(bootstrap));
    let period = std::time::Duration::from_millis(1_000);

    // One pass, then cancel. The deadline is explicit and bounded.
    let task = tokio::spawn(run_netdb_retention(
        Some(Arc::clone(&handle)),
        Arc::clone(&counters),
        cancellation.clone(),
        period,
    ));
    tokio::time::advance(period * 2).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    cancellation.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let exit = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("the cadence stopped within the deadline")
        .expect("the cadence task did not panic");

    assert_eq!(exit, NetDbRetentionExit::Cancelled);
    let snapshot = counters.snapshot();
    assert!(
        snapshot.runs >= 1,
        "the cadence ran no pass at all: {snapshot:?}"
    );
    assert_eq!(
        snapshot.removed, 2,
        "the cadence did not remove the stale records: {snapshot:?}"
    );
    assert_eq!(snapshot.unavailable, 0);
}

#[test]
fn a_period_outside_the_bound_is_refused() {
    // The bound is the participation maintenance bound, so the retention
    // cadence cannot be configured outside it.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    let exit = runtime.block_on(run_netdb_retention(
        None,
        Arc::new(NetDbRetentionCounters::default()),
        i2pr_runtime::CancellationToken::new(),
        std::time::Duration::ZERO,
    ));
    assert_eq!(exit, NetDbRetentionExit::InvalidPeriod);
    let exit = runtime.block_on(run_netdb_retention(
        None,
        Arc::new(NetDbRetentionCounters::default()),
        i2pr_runtime::CancellationToken::new(),
        std::time::Duration::from_secs(3_601),
    ));
    assert_eq!(exit, NetDbRetentionExit::InvalidPeriod);
}
