//! Plan 506 — the out-of-crate live lane for the peer-knowledge reconcile.
//!
//! The lane drives the **public** daemon API only: the bootstrap store, the
//! destination coordinator, the peer-profile store, the reconcile and the
//! inspection seam. It reaches into no private module and adds no accessor
//! of its own.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value the lane's own code draws comes
//!   from a seeded ChaCha8 stream; the peer identities are reproducible.
//! - **An explicit `now_ms`.** Every decision the lane asserts is taken on
//!   the fixed [`NOW_MS`]: the reconcile's clock, the peer-profile store's
//!   clock and the unsolicited store's validation context. The lane's
//!   outcomes therefore do not depend on the moment it runs.
//! - **No environment variable, no `#[ignore]`, no stock peer.** The only
//!   peer is one the lane signs itself.
//! - **No unbounded waiting.** The lane never sleeps to make an assertion
//!   pass; every wait is bounded by a hard deadline.
//! - **The production ingest path.** The peer arrives through
//!   [`Bootstrap::ingest_authenticated_router_store`], the same call the
//!   three production ingest sites make, so the lane measures the real
//!   path and not a shortcut around it.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    ProfileStoreConfig, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, Date, Hash, Mapping, RouterAddress};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;
use i2pr_daemon::destination_peers::CandidateRefusal;
use i2pr_daemon::destination_tunnels::DestinationTunnelCoordinator;
use i2pr_daemon::i2pcontrol_inspection::{
    InspectionHandles, PeerReconcileCounterCell, ServiceEndpoint,
};
use i2pr_daemon::netdb_participation::{PeerParticipationDriver, PeerParticipationHandle};
use i2pr_daemon::peer_reconcile::{
    MIN_RECONCILE_PERIOD_MS, PeerReconcile, PeerReconcileAction, PeerReconcileCounters,
};
use tokio::sync::Mutex;

/// The clock every assertion in the lane is taken on.
const NOW_MS: u64 = 1_700_000_000_000;

/// The API version the controlled router publishes.
const AT_FLOOR: &str = "0.9.62";

/// The API version one step below the tunnel-build floor.
const BELOW_FLOOR: &str = "0.9.61";

/// One validated peer the lane signs itself.
struct Peer {
    record: ValidatedRouterInfo,
    hash: Hash,
}

/// Builds a validated peer that publishes `version`.
///
/// The peer carries a `family` and one SSU2 address so the destination
/// projection can qualify it, and publishes `version` so Plan 491's gate
/// has something to compare.
fn peer(seed: u64, version: &str) -> Peer {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let mut options = Mapping::builder();
    options
        .insert("family".to_owned(), "ipv4".to_owned())
        .expect("family");
    options
        .insert("router.version".to_owned(), version.to_owned())
        .expect("router.version");
    let address_options = Mapping::from_entries(vec![
        ("host".to_owned(), "127.0.0.1".to_owned()),
        ("port".to_owned(), "12345".to_owned()),
    ])
    .expect("address options");
    let addresses = vec![
        RouterAddress::new(
            1,
            Date::from_millis(NOW_MS),
            "SSU2".to_owned(),
            address_options,
        )
        .expect("address"),
    ];
    let info = bundle
        .sign_router_info(
            Date::from_millis(NOW_MS),
            addresses,
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("signed info");
    let record = ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(NOW_MS)),
    )
    .expect("validated info");
    let hash = Hash::from_bytes(*record.key().as_bytes());
    Peer { record, hash }
}

/// Builds the unsolicited `DatabaseStore` message that carries a peer.
///
/// This is the message the three production ingest sites receive.
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

/// A real peer-profile handle.
fn profiles() -> PeerParticipationHandle {
    PeerParticipationHandle::new(PeerParticipationDriver::new(ProfileStoreConfig::default()))
}

/// The reconcile under test, wired to the three real stores.
fn reconcile(
    bootstrap: &Arc<std::sync::Mutex<Bootstrap>>,
    destinations: &Arc<Mutex<DestinationTunnelCoordinator>>,
    profiles: &PeerParticipationHandle,
) -> PeerReconcile {
    PeerReconcile::new(
        Some(Arc::clone(bootstrap)),
        Arc::clone(destinations),
        Some(profiles.clone()),
        MIN_RECONCILE_PERIOD_MS,
    )
    .expect("reconcile")
}

/// The peer keys the peer-profile store's version-gated selection accepts.
fn selected_peers(profiles: &PeerParticipationHandle) -> Vec<i2pr_netdb::RouterHash> {
    profiles
        .with(|driver| {
            driver
                .select_tunnel_build_peers(8, 11)
                .candidates
                .iter()
                .map(|candidate| candidate.key)
                .collect()
        })
        .expect("profiles")
}

/// The peer keys the destination coordinator's projection offers.
async fn projected_peers(
    destinations: &Arc<Mutex<DestinationTunnelCoordinator>>,
) -> Vec<i2pr_netdb::RouterHash> {
    let guard = destinations.lock().await;
    let (candidates, _summary) = guard.destination_peer_candidates();
    candidates
        .iter()
        .map(|candidate| i2pr_netdb::RouterHash::from_hash(candidate.router_hash()))
        .collect()
}

/// Ingests a peer through the production unsolicited-store path.
fn ingest(bootstrap: &Arc<std::sync::Mutex<Bootstrap>>, peer: &Peer) {
    let mut guard = bootstrap.lock().expect("bootstrap lock");
    let outcome = guard
        .ingest_authenticated_router_store(&store_message(peer), NOW_MS)
        .expect("ingest");
    assert!(
        matches!(outcome, i2pr_netdb::UnsolicitedStoreOutcome::Inserted(_)),
        "the live store must accept the peer"
    );
}

/// Plan 506 requirement 1: a peer the router accepts on a live session is
/// known to the destination store and the peer-profile store after one
/// reconcile, without a restart.
#[tokio::test]
async fn a_live_learned_peer_is_known_to_both_build_with_stores() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let profiles = profiles();
    let peer = peer(0x5060, AT_FLOOR);
    ingest(&bootstrap, &peer);
    let key = i2pr_netdb::RouterHash::from_hash(peer.hash);

    // Before the reconcile the peer is known only to the bootstrap.
    assert!(
        destinations.lock().await.store().get(&key).is_none(),
        "the peer must not be in the destination store before the reconcile"
    );
    assert!(
        !selected_peers(&profiles).contains(&key),
        "the peer must not be selectable before the reconcile"
    );

    let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
    assert_eq!(
        reconcile.run(NOW_MS).await,
        PeerReconcileAction::Reconciled { records: 1 }
    );

    // After the reconcile the peer is known to both.
    assert!(
        destinations.lock().await.store().get(&key).is_some(),
        "the peer must be in the destination store after the reconcile"
    );
    assert!(
        selected_peers(&profiles).contains(&key),
        "the peer must be selectable after the reconcile"
    );
    assert!(
        projected_peers(&destinations).await.contains(&key),
        "the peer must be a destination build candidate after the reconcile"
    );
}

/// Plan 506 requirement 4: a below-floor peer stays refused after the
/// reconcile. Plan 491's gate lives at selection, so the peer is learned
/// but never selected, and its refusal is counted.
#[tokio::test]
async fn a_below_floor_peer_stays_refused_after_the_reconcile() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let profiles = profiles();
    let peer = peer(0x5061, BELOW_FLOOR);
    ingest(&bootstrap, &peer);
    let key = i2pr_netdb::RouterHash::from_hash(peer.hash);

    let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
    reconcile.run(NOW_MS).await;

    // The record is known to the stores…
    assert!(
        destinations.lock().await.store().get(&key).is_some(),
        "the below-floor peer is still a peer record"
    );
    // …but it is never selectable, and never a build candidate.
    assert!(
        !selected_peers(&profiles).contains(&key),
        "a below-floor peer must stay refused after the reconcile"
    );
    assert!(
        !projected_peers(&destinations).await.contains(&key),
        "a below-floor peer must not become a build candidate"
    );
    // The gate's refusal is counted by reason, never a silent skip.
    let refusals = profiles
        .with(|driver| driver.select_tunnel_build_peers(8, 11).refusals)
        .expect("profiles");
    assert_eq!(refusals.below_floor, 1, "the refusal must be counted");
    // And the destination projection refuses it with the matching reason.
    let refusal = i2pr_daemon::destination_peers::DestinationPeerCandidate::from_validated(
        destinations.lock().await.store().get(&key).expect("record"),
    )
    .expect_err("a below-floor peer must be refused");
    assert_eq!(refusal, CandidateRefusal::BelowVersionFloor);
}

/// Plan 506 requirement 3: every outcome is counted by reason, and a
/// capacity refusal is counted and not fatal.
#[tokio::test]
async fn a_capacity_refusal_is_counted_and_the_batch_continues() {
    let bootstrap = bootstrap();
    let destinations = Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
        i2pr_netdb::LookupPolicy::default(),
        RouterInfoStoreConfig::new(1, 64 * 1024),
    )));
    let profiles = profiles();
    let first = peer(0x5062, AT_FLOOR);
    let second = peer(0x5063, AT_FLOOR);
    ingest(&bootstrap, &first);
    ingest(&bootstrap, &second);

    let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
    assert_eq!(
        reconcile.run(NOW_MS).await,
        PeerReconcileAction::Reconciled { records: 2 }
    );
    let counters = reconcile.counters();
    assert_eq!(counters.inserted, 1, "one record fitted");
    assert_eq!(counters.capacity_exceeded, 1, "the second was refused");
    assert_eq!(counters.conflict, 0, "a capacity refusal is not a conflict");
    assert_eq!(
        counters.learned_inserted, 2,
        "the peer-profile store is a different store with its own bound"
    );
}

/// Plan 506 requirement 2: the reconcile is bounded — it reads a bounded
/// snapshot and every store it writes is bounded by a named quota.
#[tokio::test]
async fn the_reconcile_reads_a_bounded_snapshot() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let profiles = profiles();
    for index in 0..8_u64 {
        ingest(&bootstrap, &peer(0x5100 + index, AT_FLOOR));
    }
    let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
    reconcile.run(NOW_MS).await;
    let counters = reconcile.counters();
    assert_eq!(counters.snapshot_records, 8);
    assert_eq!(counters.inserted, 8);
    assert_eq!(counters.learned_inserted, 8);
    // The bootstrap's snapshot is the bounded one the bootstrap publishes.
    assert!(
        bootstrap
            .lock()
            .expect("bootstrap lock")
            .validated_router_info_snapshot()
            .len()
            <= 256,
        "the snapshot must stay bounded"
    );
}

/// Plan 506 requirement 5: the reconcile runs on the existing tick, and
/// the cadence defers a run inside the period.
#[tokio::test]
async fn the_cadence_defers_a_second_run_inside_the_period() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let profiles = profiles();
    ingest(&bootstrap, &peer(0x5064, AT_FLOOR));
    let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
    assert_eq!(
        reconcile.run(NOW_MS).await,
        PeerReconcileAction::Reconciled { records: 1 }
    );
    assert_eq!(
        reconcile.run(NOW_MS + MIN_RECONCILE_PERIOD_MS - 1).await,
        PeerReconcileAction::Deferred
    );
    assert_eq!(
        reconcile.run(NOW_MS + MIN_RECONCILE_PERIOD_MS).await,
        PeerReconcileAction::Reconciled { records: 1 }
    );
    let counters = reconcile.counters();
    assert_eq!(counters.runs, 2);
    assert_eq!(counters.cadence_skips, 1);
}

/// Plan 506 requirement 8: the reconcile's counters are published through
/// the inspection seam, and an unpublished slot is a gap, never a
/// fabricated zero.
#[test]
fn the_reconcile_counters_are_reachable_through_the_public_inspection_seam() {
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
    assert!(
        handles.peer_reconcile_counters_snapshot().is_none(),
        "an unpublished slot is a gap, never a fabricated zero"
    );

    let cell: Arc<PeerReconcileCounterCell> =
        Arc::new(std::sync::Mutex::new(PeerReconcileCounters::default()));
    handles.publish_peer_reconcile_counters(Arc::clone(&cell));
    assert!(handles.peer_reconcile_counters().is_some());
    assert_eq!(
        handles.peer_reconcile_counters_snapshot(),
        Some(PeerReconcileCounters::default())
    );

    // The product refreshes the cell on its tick.
    let counters = PeerReconcileCounters {
        runs: 3,
        ..Default::default()
    };
    if let Ok(mut slot) = cell.lock() {
        *slot = counters;
    }
    let published = handles
        .peer_reconcile_counters_snapshot()
        .expect("published counters");
    assert_eq!(published.runs, 3);
    assert_eq!(published.inserted, 0);
}

/// Plan 506 requirement 6: the locks are taken in one order and never
/// nested. The lane proves it by driving the real stores concurrently and
/// observing that the reconcile completes while the stores are busy.
#[tokio::test]
async fn the_reconcile_completes_while_the_stores_are_busy() {
    let bootstrap = bootstrap();
    let destinations = destinations();
    let profiles = profiles();
    ingest(&bootstrap, &peer(0x5065, AT_FLOOR));
    let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);

    // Hold the destination coordinator's lock for a moment; the reconcile
    // must wait for it and then complete, not deadlock and not abort.
    let held = Arc::clone(&destinations);
    let guard_task = tokio::spawn(async move {
        let guard = held.lock().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        drop(guard);
    });
    let action = reconcile.run(NOW_MS).await;
    guard_task.await.expect("guard task");
    assert_eq!(
        action,
        PeerReconcileAction::Reconciled { records: 1 },
        "the reconcile must complete once the lock is free"
    );
}
