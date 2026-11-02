//! Plan 515 — the lane for the router's own RouterInfo publication.
//!
//! The lane proves the **schedule** and the **plan**:
//!
//! - a publication is due at the schedule's start, and then exactly one
//!   period after the last attempt;
//! - every outcome is counted by its own reason, and a refusal is never
//!   counted as a publication;
//! - the plan names the **closest** eligible floodfills, computed here
//!   from the store's own records rather than read back from the answer;
//! - a publication with no local record, or with no eligible floodfill,
//!   is refused rather than half-performed;
//! - a floodfill that publishes no usable version is never a target,
//!   which is Plan 503's API-version floor holding for the publication.
//!
//! The lane uses the public API only, fixed seeds, an explicit clock, no
//! socket, no environment variable, no `#[ignore]`, and no sleeping.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::router_info_publication::{
    PUBLICATION_FLOODFILLS, PUBLICATION_INTERVAL_MS, RouterInfoPublicationOutcome,
    RouterInfoPublicationOwner, RouterInfoPublicationPlan, RouterInfoPublicationSchedule,
};
use i2pr_netdb::{
    RouterHash, RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext,
    router_hash,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the lane's peers.
const LOCAL_SEED: u64 = 0x5150_0001;
const ALPHA_SEED: u64 = 0x5150_0002;
const BETA_SEED: u64 = 0x5150_0003;
const GAMMA_SEED: u64 = 0x5150_0004;
const DELTA_SEED: u64 = 0x5150_0005;
const THETA_SEED: u64 = 0x5150_0006;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_700_000_000_000;

/// The version a peer publishes when it is eligible for a DSM.
const ELIGIBLE_VERSION: &str = "0.9.62";

/// The lane's own identity.
fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// A validated RouterInfo signed by `signer`.
///
/// `version` is the published `router.version`. When it is `None` the
/// record publishes no version at all, which the version gate refuses.
fn validated(signer: &RouterIdentityBundle, version: Option<&str>) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    if let Some(version) = version {
        options
            .insert("router.version".to_owned(), version.to_owned())
            .unwrap();
    }
    let published = Date::from_millis(T0);
    let info = signer
        .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

/// The XOR distance between two hashes.
fn xor_distance(left: &RouterHash, right: &RouterHash) -> [u8; 32] {
    let mut distance = [0u8; 32];
    for (index, byte) in distance.iter_mut().enumerate() {
        *byte = left.as_bytes()[index] ^ right.as_bytes()[index];
    }
    distance
}

/// The lane's store: four eligible floodfills, nearest first is the
/// lane's own business, not the store's.
fn store_with_eligible_floodfills() -> (RouterInfoStore, RouterHash) {
    let local = bundle(LOCAL_SEED);
    let local_hash = router_hash(local.identity()).expect("hash");
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    for seed in [ALPHA_SEED, BETA_SEED, GAMMA_SEED, DELTA_SEED] {
        let peer = bundle(seed);
        let record = validated(&peer, Some(ELIGIBLE_VERSION));
        let _ = store.insert(record);
    }
    (store, local_hash)
}

/// The closest eligible floodfills, computed here from the store.
fn expected_closest(store: &RouterInfoStore, local_hash: &RouterHash) -> Vec<RouterHash> {
    let mut candidates: Vec<(RouterHash, [u8; 32])> = store
        .floodfill_advertisers()
        .map(|record| {
            let key = record.key();
            let distance = xor_distance(&key, local_hash);
            (key, distance)
        })
        .collect();
    candidates.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
    candidates
        .into_iter()
        .map(|(key, _)| key)
        .take(PUBLICATION_FLOODFILLS)
        .collect()
}

#[test]
fn the_first_publication_is_due_at_the_schedule_start() {
    let schedule = RouterInfoPublicationSchedule::new(PUBLICATION_INTERVAL_MS);
    assert!(
        schedule.is_due(0),
        "the first publication must be due at the schedule's start"
    );
    assert!(
        schedule.is_due(T0),
        "a schedule that has not run is due whatever the clock says"
    );
}

#[test]
fn a_publication_is_due_exactly_one_period_after_the_last_attempt() {
    let mut schedule = RouterInfoPublicationSchedule::new(PUBLICATION_INTERVAL_MS);
    schedule.record(
        T0,
        RouterInfoPublicationOutcome::Published {
            targets: 3,
            accepted: 3,
        },
    );
    assert!(
        !schedule.is_due(T0),
        "the schedule must not be due again immediately"
    );
    assert!(
        !schedule.is_due(T0 + PUBLICATION_INTERVAL_MS - 1),
        "the schedule must not be due before the period elapses"
    );
    assert!(
        schedule.is_due(T0 + PUBLICATION_INTERVAL_MS),
        "the schedule must be due exactly one period after the last attempt"
    );
}

#[test]
fn every_outcome_is_counted_by_its_own_reason() {
    let mut schedule = RouterInfoPublicationSchedule::new(PUBLICATION_INTERVAL_MS);
    let outcomes = [
        RouterInfoPublicationOutcome::Published {
            targets: 3,
            accepted: 2,
        },
        RouterInfoPublicationOutcome::RefusedNoLocalRecord,
        RouterInfoPublicationOutcome::RefusedNoTarget,
        RouterInfoPublicationOutcome::DeliveryRefused { targets: 3 },
        RouterInfoPublicationOutcome::Cancelled,
    ];
    for (index, outcome) in outcomes.iter().enumerate() {
        schedule.record(T0 + index as u64, *outcome);
    }
    let counters = schedule.counters();
    assert_eq!(counters.attempts, 5, "every attempt is counted");
    assert_eq!(counters.published, 1, "one publication succeeded");
    assert_eq!(counters.refused_no_local_record, 1);
    assert_eq!(counters.refused_no_target, 1);
    assert_eq!(counters.deliveries_refused, 1);
    assert_eq!(counters.cancelled, 1);
}

#[test]
fn a_refused_publication_is_never_counted_as_a_publication() {
    let mut schedule = RouterInfoPublicationSchedule::new(PUBLICATION_INTERVAL_MS);
    schedule.record(T0, RouterInfoPublicationOutcome::RefusedNoTarget);
    let counters = schedule.counters();
    assert_eq!(counters.attempts, 1);
    assert_eq!(
        counters.published, 0,
        "a refusal must never be counted as a publication"
    );
    assert_eq!(counters.refused_no_target, 1);
    assert!(
        !RouterInfoPublicationOutcome::RefusedNoTarget.is_published(),
        "a refusal is not a publication"
    );
    assert!(
        RouterInfoPublicationOutcome::Published {
            targets: 1,
            accepted: 1
        }
        .is_published(),
        "a publication that reached a peer is a publication"
    );
}

#[test]
fn the_plan_names_the_closest_eligible_floodfills() {
    let (store, local_hash) = store_with_eligible_floodfills();
    let owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    let plan = owner.plan(&store, Some(b"router info bytes"));
    let RouterInfoPublicationPlan::Publish { targets, store: _ } = &plan else {
        panic!("the plan must publish when a record and a floodfill exist");
    };
    let expected = expected_closest(&store, &local_hash);
    assert_eq!(
        *targets, expected,
        "the publication must name the closest eligible floodfills"
    );
    assert!(
        targets.len() <= PUBLICATION_FLOODFILLS,
        "the fan-out must stay bounded"
    );
}

#[test]
fn a_publication_without_a_local_record_is_refused() {
    let (store, local_hash) = store_with_eligible_floodfills();
    let owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    let plan = owner.plan(&store, None);
    assert_eq!(
        plan.refusal(),
        Some(RouterInfoPublicationOutcome::RefusedNoLocalRecord),
        "a publication with no local record must be refused"
    );
    assert!(plan.targets().is_empty(), "a refused plan names no target");
}

#[test]
fn a_publication_with_no_eligible_floodfill_is_refused() {
    let local = bundle(LOCAL_SEED);
    let local_hash = router_hash(local.identity()).expect("hash");
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    // A peer that publishes no version is refused by the version gate,
    // so the store holds no eligible floodfill at all.
    let unversioned = bundle(ALPHA_SEED);
    let _ = store.insert(validated(&unversioned, None));
    let owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    let plan = owner.plan(&store, Some(b"router info bytes"));
    assert_eq!(
        plan.refusal(),
        Some(RouterInfoPublicationOutcome::RefusedNoTarget),
        "a publication with no eligible floodfill must be refused"
    );
}

#[test]
fn a_floodfill_that_publishes_no_version_is_never_a_target() {
    let (mut store, local_hash) = store_with_eligible_floodfills();
    // A peer that is **not** already in the store: the store keys on the
    // identity, so a second record for the same identity would be
    // refused and the unversioned one would never land.
    let unversioned = bundle(THETA_SEED);
    let _ = store.insert(validated(&unversioned, None));
    let owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    let plan = owner.plan(&store, Some(b"router info bytes"));
    let RouterInfoPublicationPlan::Publish { targets, store: _ } = &plan else {
        panic!("the eligible floodfills must still be publishable");
    };
    let unversioned_hash = router_hash(unversioned.identity()).expect("hash");
    assert!(
        !targets.contains(&unversioned_hash),
        "a floodfill that publishes no version must never be a target"
    );
}
