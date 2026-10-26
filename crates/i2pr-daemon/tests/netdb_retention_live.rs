//! Plan 507 — the out-of-crate live lane for the NetDB retention policy.
//!
//! The lane drives the **public** daemon API only: the bootstrap store, the
//! destination coordinator, the reconcile and the inspection seam. It reaches
//! into no private module and adds no accessor of its own.
//!
//! The WP1 feasibility artefact (`netdb_retention_feasibility.rs`) proves the
//! **mechanism** — that the tree's own `RouterInfoStore::prune` makes a store
//! at its quota turn over. This lane proves the **product**: that the
//! router's own reconcile carries the prune, on the cadence it already had, in
//! the order the plan sets out, and that every outcome is counted.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value the lane's own code draws comes
//!   from a seeded ChaCha8 stream; the peer identities are reproducible.
//! - **An explicit `now_ms`.** Every decision the lane asserts is taken on
//!   an explicit clock: the reconcile's clock, the unsolicited store's
//!   validation context and the publication timestamps. The lane's outcomes
//!   therefore do not depend on the moment it runs.
//! - **No environment variable, no `#[ignore]`, no stock peer.** The only
//!   peers are ones the lane signs itself.
//! - **No unbounded waiting.** The lane never sleeps to make an assertion
//!   pass.
//! - **The production ingest path.** The peers arrive through
//!   [`Bootstrap::ingest_authenticated_router_store`], the same call the
//!   production ingest sites make, so the lane measures the real path and not
//!   a shortcut around it.
//!
//! The peer-profile store is deliberately **not** wired: the profile store
//! has its own eviction policy and self-corrects through the observation
//! tiers, so the retention policy does not apply to it.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    InsertOutcome, LocalRouterInfoBuilder, RouterInfoStoreConfig, ValidatedRouterInfo,
    ValidationContext,
};
use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, Date, Hash, Mapping, RouterAddress};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;
use i2pr_daemon::destination_tunnels::DestinationTunnelCoordinator;
use i2pr_daemon::i2pcontrol_inspection::{
    InspectionHandles, PeerReconcileCounterCell, ServiceEndpoint,
};
use i2pr_daemon::peer_reconcile::{
    MIN_RECONCILE_PERIOD_MS, PeerReconcile, PeerReconcileAction, PeerReconcileCounters,
    RETENTION_WINDOW_MS,
};
use tokio::sync::Mutex;

/// The clock the lane starts at.
const NOW_MS: u64 = 1_700_000_000_000;

/// The publication clock of the record that must **survive** the prune.
///
/// It is half a window before the prune clock, so the record is inside the
/// window by a wide margin. A window that collapsed would take this record
/// with it, which is what makes the lane sensitive to the window rather
/// than to the clock.
const FRESH_PUBLISHED_MS: u64 = NOW_MS + RETENTION_WINDOW_MS / 2;

/// The clock the prune runs at: one hour past the acceptance window.
///
/// The prune compares a record's own **publication** age against the
/// window, so a record published at [`NOW_MS`] is stale at this clock.
const PRUNE_AT_MS: u64 = NOW_MS + RETENTION_WINDOW_MS + 3_600_000;

/// One validated peer the lane signs itself.
struct Peer {
    record: ValidatedRouterInfo,
    hash: Hash,
}

/// Builds a validated peer published at `published_ms` and validated at
/// `validated_ms`.
///
/// The two timestamps are separate on purpose: a peer is validated when it is
/// fresh and pruned later, when it is not.
fn peer(seed: u64, published_ms: u64, validated_ms: u64) -> Peer {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let mut options = Mapping::builder();
    options
        .insert("family".to_owned(), "ipv4".to_owned())
        .expect("family");
    options
        .insert("router.version".to_owned(), "0.9.62".to_owned())
        .expect("router.version");
    let address_options = Mapping::from_entries(vec![
        ("host".to_owned(), "127.0.0.1".to_owned()),
        ("port".to_owned(), "12345".to_owned()),
    ])
    .expect("address options");
    let addresses = vec![
        RouterAddress::new(
            1,
            Date::from_millis(published_ms),
            "SSU2".to_owned(),
            address_options,
        )
        .expect("address"),
    ];
    let info = bundle
        .sign_router_info(
            Date::from_millis(published_ms),
            addresses,
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("signed info");
    let record = ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(validated_ms)),
    )
    .expect("validated info");
    let hash = Hash::from_bytes(*record.key().as_bytes());
    Peer { record, hash }
}

/// Builds the unsolicited `DatabaseStore` message that carries a peer.
///
/// This is the message the production ingest sites receive.
fn store_message(peer: &Peer) -> DatabaseStoreMessage {
    let encoded = peer
        .record
        .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    {
        use std::io::Write as _;
        encoder.write_all(&encoded).expect("compress");
    }
    let compressed = encoder.finish().expect("finish");
    let compressed_len = compressed.len();
    DatabaseStoreMessage {
        key: peer.hash,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(
            i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
        ),
    }
}

/// A real bootstrap store.
fn bootstrap() -> Arc<std::sync::Mutex<Bootstrap>> {
    Arc::new(std::sync::Mutex::new(Bootstrap::new(
        RouterInfoStoreConfig::default(),
        ReseedConfig {
            enabled: false,
            max_sources: 0,
            max_su3_bytes: 0,
            sources: Vec::new(),
        },
    )))
}

/// A real destination coordinator.
fn destinations() -> Arc<Mutex<DestinationTunnelCoordinator>> {
    Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
        i2pr_netdb::LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    )))
}

/// A real destination coordinator whose store holds exactly one record.
fn destinations_with_one_record() -> Arc<Mutex<DestinationTunnelCoordinator>> {
    Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
        i2pr_netdb::LookupPolicy::default(),
        RouterInfoStoreConfig::new(1, 64 * 1024),
    )))
}

/// The reconcile under test.
fn reconcile(
    bootstrap: &Arc<std::sync::Mutex<Bootstrap>>,
    destinations: &Arc<Mutex<DestinationTunnelCoordinator>>,
) -> PeerReconcile {
    PeerReconcile::new(
        Some(Arc::clone(bootstrap)),
        Arc::clone(destinations),
        None,
        MIN_RECONCILE_PERIOD_MS,
    )
    .expect("reconcile")
}

/// Ingests a peer through the production unsolicited-store path.
fn ingest(bootstrap: &Arc<std::sync::Mutex<Bootstrap>>, peer: &Peer, now_ms: u64) {
    let mut guard = bootstrap.lock().expect("bootstrap lock");
    let outcome = guard
        .ingest_authenticated_router_store(&store_message(peer), now_ms)
        .expect("ingest");
    assert!(
        matches!(outcome, i2pr_netdb::UnsolicitedStoreOutcome::Inserted(_)),
        "the live store must accept the peer"
    );
}

/// The canonical key of a peer.
fn key(peer: &Peer) -> i2pr_netdb::RouterHash {
    i2pr_netdb::RouterHash::from_hash(peer.hash)
}

/// Plan 507 requirement 1: the product prunes. A record the router would
/// refuse to accept today is gone from both live stores after one reconcile,
/// and both removals are counted.
#[tokio::test]
async fn the_product_prunes_the_bootstrap_and_the_destination_store() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let stale = peer(0x5070, NOW_MS, NOW_MS);
    let fresh = peer(0x5071, FRESH_PUBLISHED_MS, FRESH_PUBLISHED_MS);

    ingest(&bootstrap, &stale, NOW_MS);
    ingest(&bootstrap, &fresh, FRESH_PUBLISHED_MS);
    // The destination store holds the stale record too, so the prune has
    // something to remove on both sides.
    let outcome = destinations
        .lock()
        .await
        .insert_validated_router_info(stale.record.clone());
    assert!(
        matches!(outcome, InsertOutcome::Inserted),
        "the destination store takes the peer"
    );

    let mut reconcile = reconcile(&bootstrap, &destinations);
    assert_eq!(
        reconcile.run(PRUNE_AT_MS).await,
        PeerReconcileAction::Reconciled { records: 1 },
        "only the fresh record survives the prune"
    );

    let counters = reconcile.counters();
    assert_eq!(
        counters.pruned_bootstrap, 1,
        "the bootstrap lost the stale record"
    );
    assert_eq!(
        counters.pruned_destination, 1,
        "the destination store lost it too"
    );
    assert_eq!(counters.prune_unavailable, 0, "no prune failed");

    assert!(
        bootstrap
            .lock()
            .expect("bootstrap lock")
            .store()
            .get(&key(&stale))
            .is_none(),
        "the stale record must be gone from the bootstrap"
    );
    assert!(
        destinations
            .lock()
            .await
            .store()
            .get(&key(&stale))
            .is_none(),
        "the stale record must be gone from the destination store"
    );
    assert!(
        destinations
            .lock()
            .await
            .store()
            .get(&key(&fresh))
            .is_some(),
        "the fresh record must be in the destination store"
    );
}

/// Plan 507 requirement 2: a store at its quota refuses a new peer **before**
/// the prune and accepts it **after**. This is the product's answer to the
/// freeze the WP1 artefact measured on the store.
#[tokio::test]
async fn a_store_at_its_quota_refuses_a_new_peer_before_the_prune_and_accepts_it_after() {
    let bootstrap = bootstrap();
    let destinations = destinations_with_one_record();
    let stale = peer(0x5072, NOW_MS, NOW_MS);
    let fresh = peer(0x5073, FRESH_PUBLISHED_MS, FRESH_PUBLISHED_MS);
    ingest(&bootstrap, &stale, NOW_MS);
    ingest(&bootstrap, &fresh, FRESH_PUBLISHED_MS);
    {
        let mut guard = destinations.lock().await;
        assert!(
            matches!(
                guard.insert_validated_router_info(stale.record.clone()),
                InsertOutcome::Inserted
            ),
            "the store takes the first peer"
        );
    }

    let mut reconcile = reconcile(&bootstrap, &destinations);

    // Before the prune: nothing is stale yet, so the store is still at its
    // quota and the new peer is refused.
    reconcile.run(NOW_MS).await;
    let counters = reconcile.counters();
    assert_eq!(
        counters.capacity_exceeded, 1,
        "the quota refuses the new peer"
    );
    assert_eq!(counters.pruned_bootstrap, 0, "nothing was stale yet");
    assert_eq!(counters.pruned_destination, 0, "nothing was stale yet");
    assert_eq!(
        destinations.lock().await.store().len(),
        1,
        "the store still holds exactly one record"
    );

    // After the prune: the stale record is gone, so the store has room.
    reconcile.run(PRUNE_AT_MS).await;
    let counters = reconcile.counters();
    assert_eq!(
        counters.pruned_bootstrap, 1,
        "the bootstrap lost the stale record"
    );
    assert_eq!(
        counters.pruned_destination, 1,
        "the destination store lost it too"
    );
    assert_eq!(
        counters.capacity_exceeded, 1,
        "the earlier refusal is counted once and is not erased"
    );
    assert_eq!(
        counters.inserted, 1,
        "the fresh peer fitted after the prune"
    );
    assert!(
        destinations
            .lock()
            .await
            .store()
            .get(&key(&fresh))
            .is_some(),
        "the new peer must be accepted after the prune"
    );
    assert!(
        destinations
            .lock()
            .await
            .store()
            .get(&key(&stale))
            .is_none(),
        "the stale record must be gone after the prune"
    );
}

/// Plan 507 requirement 3: a record inside the window survives the prune.
/// The window is the router's own acceptance window, so the router retains
/// exactly what it accepts.
#[tokio::test]
async fn a_record_inside_the_window_survives_the_prune() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let fresh = peer(0x5074, FRESH_PUBLISHED_MS, FRESH_PUBLISHED_MS);
    ingest(&bootstrap, &fresh, FRESH_PUBLISHED_MS);

    let mut reconcile = reconcile(&bootstrap, &destinations);
    reconcile.run(PRUNE_AT_MS).await;

    let counters = reconcile.counters();
    assert_eq!(counters.pruned_bootstrap, 0, "a fresh record is not pruned");
    assert_eq!(
        counters.pruned_destination, 0,
        "a fresh record is not pruned"
    );
    assert!(
        bootstrap
            .lock()
            .expect("bootstrap lock")
            .store()
            .get(&key(&fresh))
            .is_some(),
        "a fresh record must survive the prune"
    );
    assert!(
        destinations
            .lock()
            .await
            .store()
            .get(&key(&fresh))
            .is_some(),
        "a fresh record must survive the prune in the destination store too"
    );
}

/// Plan 507 requirement 4: the router's own RouterInfo is never a prune
/// victim. The local record lives beside the store, not inside it, so the
/// prune cannot reach it.
#[tokio::test]
async fn the_router_own_router_info_is_never_a_prune_victim() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let mut rng = ChaCha8Rng::seed_from_u64(0x5075);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let local = LocalRouterInfoBuilder::new(&bundle)
        .build_default(Date::from_millis(NOW_MS))
        .expect("local router info");
    let local_hash = local.router_hash();
    {
        let mut guard = bootstrap.lock().expect("bootstrap lock");
        guard.replace_local(local);
    }
    ingest(&bootstrap, &peer(0x5076, NOW_MS, NOW_MS), NOW_MS);

    let mut reconcile = reconcile(&bootstrap, &destinations);
    reconcile.run(PRUNE_AT_MS).await;

    let guard = bootstrap.lock().expect("bootstrap lock");
    let kept = guard
        .local()
        .expect("the local record must survive the prune");
    assert_eq!(
        kept.router_hash(),
        local_hash,
        "the router's own record must be unchanged"
    );
    assert!(
        guard.store().get(&local_hash).is_none(),
        "the local record is not a store record, so the prune cannot reach it"
    );
    assert_eq!(
        reconcile.counters().pruned_bootstrap,
        1,
        "the stale peer was pruned and the local record was not a victim"
    );
}

/// Plan 507 requirement 5: the prune runs **before** the snapshot, so a
/// record the prune removed is never carried into the destination store.
#[tokio::test]
async fn the_prune_runs_before_the_snapshot() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let stale = peer(0x5077, NOW_MS, NOW_MS);
    ingest(&bootstrap, &stale, NOW_MS);

    let mut reconcile = reconcile(&bootstrap, &destinations);
    assert_eq!(
        reconcile.run(PRUNE_AT_MS).await,
        PeerReconcileAction::Reconciled { records: 0 },
        "the snapshot carries the fresh set, not the pruned one"
    );

    let counters = reconcile.counters();
    assert_eq!(
        counters.pruned_bootstrap, 1,
        "the bootstrap lost the stale record"
    );
    assert_eq!(
        counters.pruned_destination, 0,
        "the destination store never saw the pruned record"
    );
    assert!(
        destinations
            .lock()
            .await
            .store()
            .get(&key(&stale))
            .is_none(),
        "a pruned record must never reach the destination store"
    );
}

/// Plan 507 requirement 6: a prune that cannot run is counted, never
/// silently skipped. A poisoned bootstrap lock is the reachable case.
#[tokio::test]
async fn a_poisoned_bootstrap_lock_is_counted_and_never_silent() {
    let bootstrap = bootstrap();
    let destinations = destinations();

    // Poison the bootstrap's lock: a thread panics while holding it.
    let poisoned = Arc::clone(&bootstrap);
    let handle = std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("lock");
        panic!("the lane poisons the bootstrap lock on purpose");
    });
    let _ = handle.join();

    let mut reconcile = reconcile(&bootstrap, &destinations);
    assert_eq!(
        reconcile.run(NOW_MS).await,
        PeerReconcileAction::SourceUnavailable,
        "a poisoned source is a gap, never a fabricated zero"
    );

    let counters = reconcile.counters();
    assert_eq!(
        counters.prune_unavailable, 1,
        "the prune that could not run must be counted"
    );
    assert_eq!(
        counters.pruned_bootstrap, 0,
        "the prune did not run, so it must not report a removal"
    );
    assert_eq!(
        counters.source_unavailable, 1,
        "the snapshot reports the same gap"
    );
    assert_eq!(
        counters.counts_by_reason().get("prune-unavailable"),
        Some(&1),
        "the condition must carry a stable reason label"
    );
}

/// Plan 507 requirement 7: the prune counts reach the public inspection
/// seam, under stable reason labels.
#[test]
fn the_prune_counters_reach_the_public_inspection_seam() {
    let handles = InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    );
    let cell: Arc<PeerReconcileCounterCell> =
        Arc::new(std::sync::Mutex::new(PeerReconcileCounters::default()));
    handles.publish_peer_reconcile_counters(Arc::clone(&cell));

    let counters = PeerReconcileCounters {
        pruned_bootstrap: 3,
        pruned_destination: 4,
        prune_unavailable: 1,
        ..Default::default()
    };
    if let Ok(mut slot) = cell.lock() {
        *slot = counters;
    }

    let published = handles
        .peer_reconcile_counters_snapshot()
        .expect("published counters");
    assert_eq!(published.pruned_bootstrap, 3);
    assert_eq!(published.pruned_destination, 4);
    assert_eq!(published.prune_unavailable, 1);
    let by_reason = published.counts_by_reason();
    assert_eq!(by_reason.get("pruned-bootstrap"), Some(&3));
    assert_eq!(by_reason.get("pruned-destination"), Some(&4));
    assert_eq!(by_reason.get("prune-unavailable"), Some(&1));
}

/// Plan 507 requirement 8: the prune is deterministic for a fixed `now`.
/// Two identical runs over identical inputs remove the same records and
/// report the same counts.
#[tokio::test]
async fn the_prune_is_deterministic_for_a_fixed_now() {
    let mut observed: Vec<(String, u64, u64, u64, u64, usize, usize)> = Vec::new();
    for _ in 0..2 {
        let bootstrap = bootstrap();
        let destinations = destinations();
        ingest(&bootstrap, &peer(0x5078, NOW_MS, NOW_MS), NOW_MS);
        ingest(
            &bootstrap,
            &peer(0x5079, FRESH_PUBLISHED_MS, FRESH_PUBLISHED_MS),
            FRESH_PUBLISHED_MS,
        );
        let mut reconcile = reconcile(&bootstrap, &destinations);
        let action = reconcile.run(PRUNE_AT_MS).await;
        let counters = reconcile.counters();
        // The bootstrap's lock is released before the destination
        // coordinator's lock is taken, so no guard is held across an await.
        let bootstrap_len = bootstrap.lock().expect("bootstrap lock").store().len();
        let destination_len = destinations.lock().await.store().len();
        observed.push((
            format!("{action:?}"),
            counters.pruned_bootstrap,
            counters.pruned_destination,
            counters.prune_unavailable,
            counters.inserted,
            bootstrap_len,
            destination_len,
        ));
    }
    assert_eq!(
        observed[0], observed[1],
        "the same inputs and the same clock must produce the same outcome"
    );
}
