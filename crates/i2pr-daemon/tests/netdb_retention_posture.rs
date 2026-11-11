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
//! The lane drives the **production** retention step -- `prune_netdb` and its
//! cadence -- and proves what it **does**: a store at its quota refuses a peer,
//! the step removes exactly the stale records, the peer is accepted afterwards,
//! a record at the boundary survives, the step is idempotent, a lock that
//! cannot be taken is counted rather than reported as a prune, and the router's
//! own `RouterInfo` is never a victim.
//!
//! Every row drives the production step rather than `Bootstrap::prune`
//! directly. The first draft of this lane called `Bootstrap::prune` in two
//! rows, and Plan 542's own negative controls caught it: a mutation that
//! stopped the step from pruning at all left those rows **passing**, because
//! they were testing a function the plan does not change. The rows now drive
//! the step, and the amendment log records the correction.
//!
//! The lane computes the stale set **independently**, by walking the store's own
//! iterator, so the lane's expectation and the store's own bookkeeping are two
//! different things that must agree.
//!
//! The lane cannot see the step's **reachability in the daemon's graph** -- it
//! constructs the owner directly. That is the guard's claim, and the guard does
//! not make this lane's claim. Plan 507's lane had exactly this shape and
//! constructed `PeerReconcile` by hand, which is why the reachability defect
//! survived it.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value comes from a seeded ChaCha8 stream.
//! - **An explicit clock.** The lane never reads the wall clock.
//! - **No environment variable, no `#[ignore]`, no stock peer.**
//! - **No unbounded waiting.** The async row uses `start_paused` and an
//!   explicit bounded deadline.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::Mutex;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;
use i2pr_daemon::netdb_participation::{
    NetDbRetentionCounters, NetDbRetentionExit, prune_netdb, run_netdb_retention,
};
use i2pr_daemon::peer_reconcile::PruneOutcome;
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
fn bootstrap_with(max_records: usize) -> Arc<Mutex<Bootstrap>> {
    let config = RouterInfoStoreConfig {
        max_records,
        ..RouterInfoStoreConfig::default()
    };
    Arc::new(Mutex::new(Bootstrap::new(
        config,
        ReseedConfig {
            enabled: false,
            max_sources: 0,
            max_su3_bytes: 0,
            sources: Vec::new(),
        },
    )))
}

/// Inserts one record into the store under the handle's lock.
fn insert(handle: &Arc<Mutex<Bootstrap>>, record: ValidatedRouterInfo) -> InsertOutcome {
    let mut guard = handle.lock().expect("the store's lock");
    guard.store_mut().insert(record)
}

/// Reads the store's contents under the handle's lock.
fn with_store<T>(handle: &Arc<Mutex<Bootstrap>>, read: impl FnOnce(&RouterInfoStore) -> T) -> T {
    let guard = handle.lock().expect("the store's lock");
    read(guard.store())
}

/// Runs the **production** retention step and returns the number it removed.
///
/// Every row that asserts the step's effect goes through here, so a mutation
/// that stops the step from pruning cannot leave a row passing.
fn run_step(handle: &Arc<Mutex<Bootstrap>>, now_ms: u64) -> usize {
    match prune_netdb(Some(handle), now_ms) {
        PruneOutcome::Removed(removed) => removed,
        other => panic!("the production step did not prune: {other:?}"),
    }
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

/// Inserts one record the lane wants to be **stale** at `NOW_MS`.
///
/// The record is validated at its own publication time, so it was accepted
/// while it was fresh and only became stale afterwards.
fn insert_stale(handle: &Arc<Mutex<Bootstrap>>, seed: u64, how_old: u64) {
    let published = NOW_MS.saturating_sub(how_old);
    let record = sign(&make_bundle(seed), published, published);
    assert_eq!(insert(handle, record), InsertOutcome::Inserted);
}

/// Inserts one record that is **fresh** at `NOW_MS`.
fn insert_fresh(handle: &Arc<Mutex<Bootstrap>>, seed: u64) {
    let record = sign(&make_bundle(seed), NOW_MS, NOW_MS);
    assert_eq!(insert(handle, record), InsertOutcome::Inserted);
}

#[test]
fn the_store_at_its_quota_refuses_the_next_peer() {
    let handle = bootstrap_with(3);
    for index in 0..3u64 {
        insert_fresh(&handle, 0x0054_3100 + index);
    }
    let refused = sign(&make_bundle(0x0054_3200), NOW_MS, NOW_MS);
    assert_eq!(
        insert(&handle, refused),
        InsertOutcome::CapacityExceeded,
        "a full store must refuse the next peer; the lane's premise is wrong"
    );
}

#[test]
fn the_step_removes_exactly_the_stale_records() {
    let handle = bootstrap_with(16);
    for index in 0..4u64 {
        insert_fresh(&handle, 0x0054_3300 + index);
    }
    for index in 0..4u64 {
        insert_stale(&handle, 0x0054_3400 + index, WINDOW_MS * 2);
    }

    let expected = with_store(&handle, |store| independent_stale(store, NOW_MS, WINDOW_MS));
    assert_eq!(
        expected.len(),
        4,
        "the lane's stale set is not what it means"
    );

    let removed = run_step(&handle, NOW_MS);
    assert_eq!(
        removed,
        expected.len(),
        "the step did not remove exactly the stale set"
    );
    let survivors = with_store(&handle, retained);
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
    let handle = bootstrap_with(2);
    for index in 0..2u64 {
        insert_stale(&handle, 0x0054_3500 + index, WINDOW_MS * 2);
    }
    let fresh = sign(&make_bundle(0x0054_3600), NOW_MS, NOW_MS);
    let key = fresh.key();
    assert_eq!(
        insert(&handle, fresh.clone()),
        InsertOutcome::CapacityExceeded,
        "the store must refuse before the step, or the row proves nothing"
    );

    assert_eq!(run_step(&handle, NOW_MS), 2, "the step did not make room");
    assert_eq!(
        insert(&handle, fresh),
        InsertOutcome::Inserted,
        "the peer the store refused is still refused after the step"
    );
    assert!(with_store(&handle, retained).contains(&key));
}

#[test]
fn a_record_at_the_boundary_survives() {
    let handle = bootstrap_with(8);
    // Exactly at the boundary: `now - published == window` is **inside** it.
    let at_boundary = sign(
        &make_bundle(0x0054_3700),
        NOW_MS - WINDOW_MS,
        NOW_MS - WINDOW_MS,
    );
    let key = at_boundary.key();
    assert_eq!(insert(&handle, at_boundary), InsertOutcome::Inserted);
    assert_eq!(
        run_step(&handle, NOW_MS),
        0,
        "a record at the boundary was treated as stale"
    );
    assert!(
        with_store(&handle, retained).contains(&key),
        "a record inside the window did not survive"
    );
}

#[test]
fn the_step_is_idempotent() {
    let handle = bootstrap_with(8);
    for index in 0..3u64 {
        insert_stale(&handle, 0x0054_3800 + index, WINDOW_MS * 2);
    }
    assert_eq!(run_step(&handle, NOW_MS), 3);
    let after_first = with_store(&handle, retained);
    assert_eq!(
        run_step(&handle, NOW_MS),
        0,
        "a second pass over a store with nothing stale removed something"
    );
    assert_eq!(
        with_store(&handle, retained),
        after_first,
        "a second pass changed the store's contents"
    );
}

#[test]
fn the_step_names_an_absent_store_rather_than_reporting_a_prune() {
    assert_eq!(
        prune_netdb(None, NOW_MS),
        PruneOutcome::NotApplicable,
        "an absent store must be NotApplicable, never a silent success"
    );
}

#[test]
fn a_poisoned_lock_is_counted_and_the_store_keeps_its_contents() {
    let handle = bootstrap_with(8);
    insert_stale(&handle, 0x0054_3900, WINDOW_MS * 2);
    let before = with_store(&handle, retained);

    // Poison the lock by panicking while it is held.
    let poisoned = Arc::clone(&handle);
    let _ = std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("the lock is not poisoned yet");
        panic!("poison the lock");
    })
    .join();

    assert_eq!(
        prune_netdb(Some(&handle), NOW_MS),
        PruneOutcome::Unavailable,
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
    let handle = bootstrap_with(8);
    for index in 0..4u64 {
        insert_stale(&handle, 0x0054_3a00 + index, WINDOW_MS * 2);
    }
    // The router's own record lives **beside** the store, never inside it.
    let local = {
        let guard = handle.lock().expect("the store's lock");
        guard.local().map(|local| local.router_hash())
    };
    assert_eq!(run_step(&handle, NOW_MS), 4);
    assert_eq!(with_store(&handle, RouterInfoStore::len), 0);
    let local_after = {
        let guard = handle.lock().expect("the store's lock");
        guard.local().map(|local| local.router_hash())
    };
    assert_eq!(
        local, local_after,
        "the step changed the router's own identity"
    );
}

#[test]
fn the_cadence_runs_the_step_and_counts_it() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let handle = bootstrap_with(4);
        for index in 0..2u64 {
            insert_stale(&handle, 0x0054_3b00 + index, WINDOW_MS * 4);
        }

        let counters = Arc::new(NetDbRetentionCounters::default());
        let cancellation = i2pr_runtime::CancellationToken::new();
        let period = std::time::Duration::from_millis(1_000);

        // One pass, then cancel. The deadline is explicit and bounded.
        let task = tokio::spawn(run_netdb_retention(
            Some(Arc::clone(&handle)),
            Arc::clone(&counters),
            cancellation.clone(),
            period,
        ));
        tokio::time::advance(period * 2).await;
        tokio::task::yield_now().await;
        tokio::time::advance(period).await;
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
    });
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
