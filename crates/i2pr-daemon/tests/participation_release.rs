//! Plan 543's lane -- the participation ledger must not exhaust the
//! exploratory pool.
//!
//! `ParticipationLedger` is the router's record of how often each peer has been
//! selected, and the selection **skips** any peer whose counter has reached the
//! cap. The counter **saturates** at the cap, and the ledger's only other
//! release is eviction of the **oldest** entry when it is **full**
//! (`crates/i2pr-netdb/src/selection.rs:209`).
//!
//! The ledger tracks `MAX_PARTICIPATION_TRACKED_PEERS = 4_096` peers, while the
//! profile store's population is the bounded 256-record prefix the reconcile
//! feeds it. The population is a few hundred and does not churn, so **eviction
//! never fires** and no counter is ever released. After at most
//! `256 x 8 = 2 048` selections the exploratory build driver's selection is
//! **permanently empty**.
//!
//! The lane proves the **release** through the real selection: a peer at the
//! cap whose last selection is inside the window is still skipped, the same
//! peer outside the window is selectable again, and a pool in which **every**
//! peer is at the cap becomes non-empty again once the clock passes the window.
//!
//! The lane fixes its clock explicitly and never reads the wall clock.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::PeerParticipationDriver;
use i2pr_netdb::{
    DEFAULT_MAX_PARTICIPATION, MAX_PARTICIPATION_TRACKED_PEERS, PARTICIPATION_WINDOW_MS,
    ProfileStoreConfig, RouterHash, SelectionConstraints, ValidatedRouterInfo, ValidationContext,
    select_candidates_with_report,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane's records are published around.
const NOW_MS: u64 = 1_700_000_000_000;

/// One deterministic test identity.
fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs and validates one RouterInfo that publishes a current API version.
fn sign(signer: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options
        .insert("router.version".to_owned(), "0.9.62".to_owned())
        .expect("router.version");
    options
        .insert("caps".to_owned(), "f".to_owned())
        .expect("caps");
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

/// A driver whose store holds `count` floodfill advertisers.
fn driver_with(seed_base: u64, count: u64) -> PeerParticipationDriver {
    let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
    for index in 0..count {
        driver.learn(sign(&make_bundle(seed_base + index), NOW_MS));
    }
    driver
}

/// The keys the driver's version-gated tunnel-build selection yields.
fn selected(driver: &mut PeerParticipationDriver, now_ms: u64) -> Vec<RouterHash> {
    driver.advance_time(now_ms);
    driver
        .select_tunnel_build_peers(4, 0x5430)
        .candidates
        .into_iter()
        .map(|candidate| candidate.key)
        .collect()
}

#[test]
fn a_peer_inside_the_window_is_still_skipped() {
    let mut driver = driver_with(0x0054_4300, 4);
    // Select four times at the same instant: every peer reaches the cap.
    for _ in 0..DEFAULT_MAX_PARTICIPATION {
        assert!(!selected(&mut driver, NOW_MS).is_empty());
    }
    assert!(
        driver
            .select_tunnel_build_peers(1, 0x5430)
            .candidates
            .is_empty(),
        "a pool at the cap must be skipped while its entries are inside the window"
    );
}

#[test]
fn a_peer_outside_the_window_is_selectable_again() {
    let mut driver = driver_with(0x0054_4400, 4);
    for _ in 0..DEFAULT_MAX_PARTICIPATION {
        let _ = selected(&mut driver, NOW_MS);
    }
    // Inside the window: nothing.
    assert!(
        driver
            .select_tunnel_build_peers(1, 0x5431)
            .candidates
            .is_empty()
    );
    // Outside the window: the pool is released.
    let later = NOW_MS + PARTICIPATION_WINDOW_MS + 1;
    assert!(
        !selected(&mut driver, later).is_empty(),
        "a pool whose entries are outside the window is still exhausted"
    );
}

#[test]
fn the_pool_is_not_permanently_exhausted() {
    // The defect's arithmetic: a fixed population, a saturating cap, and a
    // ledger large enough that eviction never fires. The release is what makes
    // the pool usable again.
    let mut driver = driver_with(0x0054_4500, 8);
    let mut rounds = 0u64;
    // Drive the pool to the cap, as the driver does over its lifetime.
    loop {
        let before = selected(&mut driver, NOW_MS).len();
        rounds += 1;
        if before == 0 {
            break;
        }
        assert!(rounds < 1_000, "the pool never reached the cap");
    }
    assert!(rounds > 1, "the pool reached the cap in one round");

    // Before the window passes, the pool is exhausted.
    assert!(selected(&mut driver, NOW_MS).is_empty());

    // After it passes, the pool is available again -- and stays available.
    let later = NOW_MS + PARTICIPATION_WINDOW_MS + 1;
    assert!(!selected(&mut driver, later).is_empty());
    let later_again = later + PARTICIPATION_WINDOW_MS + 1;
    assert!(!selected(&mut driver, later_again).is_empty());
}

#[test]
fn a_released_peer_is_recorded_with_the_new_instant() {
    let mut driver = driver_with(0x0054_4600, 2);
    for _ in 0..DEFAULT_MAX_PARTICIPATION {
        let _ = selected(&mut driver, NOW_MS);
    }
    // The release makes the pool available at `later`.
    let later = NOW_MS + PARTICIPATION_WINDOW_MS + 1;
    assert!(
        !selected(&mut driver, later).is_empty(),
        "the released pool yielded nothing"
    );
    // Driving it to the cap again at `later` re-arms the cap ...
    for _ in 0..DEFAULT_MAX_PARTICIPATION {
        let _ = selected(&mut driver, later);
    }
    assert!(
        selected(&mut driver, later).is_empty(),
        "the cap did not re-arm after the release"
    );
    // ... and the window restarts from `later`, not from `NOW_MS`: one window
    // past `later` releases it again.
    let much_later = later + PARTICIPATION_WINDOW_MS + 1;
    assert!(
        !selected(&mut driver, much_later).is_empty(),
        "the window did not restart from the new instant"
    );
}

#[test]
fn the_counter_still_saturates_at_the_cap() {
    let mut ledger = i2pr_netdb::ParticipationLedger::new(DEFAULT_MAX_PARTICIPATION, 16);
    let key = sign(&make_bundle(0x0054_4700), NOW_MS).key();
    for _ in 0..(DEFAULT_MAX_PARTICIPATION as usize * 4) {
        ledger.record(&key, NOW_MS);
    }
    assert_eq!(
        ledger.count(&key),
        DEFAULT_MAX_PARTICIPATION,
        "the counter must still saturate at the cap"
    );
    assert!(ledger.is_capped(&key));
}

#[test]
fn the_ledger_still_respects_its_entry_bound() {
    let bound = 4usize;
    let mut ledger = i2pr_netdb::ParticipationLedger::new(DEFAULT_MAX_PARTICIPATION, bound);
    for index in 0..(bound as u64 * 3) {
        let key = sign(&make_bundle(0x0054_4800 + index), NOW_MS).key();
        ledger.record(&key, NOW_MS);
    }
    assert!(
        ledger.len() <= bound,
        "the ledger tracked {} peers against a bound of {bound}",
        ledger.len()
    );
    assert!(MAX_PARTICIPATION_TRACKED_PEERS >= bound);
}

#[test]
fn the_reclaim_is_exact_and_counts_what_it_released() {
    let mut ledger = i2pr_netdb::ParticipationLedger::new(DEFAULT_MAX_PARTICIPATION, 16);
    let fresh = sign(&make_bundle(0x0054_4900), NOW_MS).key();
    let stale = sign(&make_bundle(0x0054_4a00), NOW_MS).key();
    ledger.record(&fresh, NOW_MS);
    ledger.record(&stale, NOW_MS - PARTICIPATION_WINDOW_MS - 1);

    let released = ledger.reclaim(NOW_MS);
    assert_eq!(
        released, 1,
        "the reclaim released the wrong number of peers"
    );
    assert_eq!(ledger.count(&fresh), 1, "the fresh entry was released");
    assert_eq!(ledger.count(&stale), 0, "the stale entry was not released");
    assert_eq!(ledger.last_recorded_at(&stale), None);
}

#[test]
fn the_selection_without_a_ledger_is_unchanged() {
    // The plain selection has no participation table, so the plan cannot have
    // narrowed it.
    let driver = driver_with(0x0054_4b00, 4);
    let report =
        select_candidates_with_report(driver.store(), &SelectionConstraints::new(4, 0x5432));
    assert!(
        !report.candidates.is_empty(),
        "the ungated selection yielded nothing"
    );
}
