//! Plan 517 — the lane for the publication's **retry**.
//!
//! A publication is a **cycle**: a bounded ladder of **batches**, each
//! batch being the closest eligible targets the cycle has not already
//! asked. The lane proves the ladder:
//!
//! - a cycle asks the **closest** eligible targets, computed here from the
//!   store's own records rather than read back from the answer;
//! - the **second** batch excludes the targets the first one already asked,
//!   so a target is never asked twice in one cycle;
//! - a cycle is **not** retried before the reference's
//!   `ROUTER_INFO_CONFIRMATION_TIMEOUT`, and **is** retried once it passes;
//! - a **delivery refusal** advances at once rather than waiting out the
//!   timeout;
//! - a **confirmation** ends the cycle, and the reference's exclusion set
//!   is cleared with it;
//! - the cycle is **bounded** by the reference's fifteen asked targets, and
//!   the fan-out stays bounded at the tree's three;
//! - the cycle's state depends only on the **reading it is given**: it
//!   never reads a clock.
//!
//! The lane uses the public API only, fixed seeds, an explicit clock, no
//! socket, no environment variable, no `#[ignore]`, and no sleeping.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::router_info_publication::{
    PUBLICATION_CONFIRMATION_TIMEOUT_MS, PUBLICATION_CYCLE_TARGETS, PUBLICATION_FLOODFILLS,
    PUBLICATION_INTERVAL_MS, PublicationConfirmation, RouterInfoPublicationOwner,
    RouterInfoPublicationPlan, RouterInfoPublicationStep, begin_attempts,
};
use i2pr_netdb::{
    RouterHash, RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext,
    router_hash,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seed of the lane's own router.
const LOCAL_SEED: u64 = 0x5170_0001;
/// The first seed of the lane's peers.
const PEER_SEED_BASE: u64 = 0x5170_1000;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_700_000_000_000;

/// The version a peer publishes when it is eligible for a DSM.
const ELIGIBLE_VERSION: &str = "0.9.62";

/// The lane's own identity.
fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// A validated RouterInfo signed by `signer`, advertising the floodfill
/// capability and an eligible version.
fn validated(signer: &RouterIdentityBundle) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    options
        .insert("router.version".to_owned(), ELIGIBLE_VERSION.to_owned())
        .unwrap();
    let published = Date::from_millis(T0);
    let info = signer
        .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

/// The lane's store: `count` eligible floodfills and the lane's own hash.
fn store_with_eligible_floodfills(count: usize) -> (RouterInfoStore, RouterHash) {
    let local = bundle(LOCAL_SEED);
    let local_hash = router_hash(local.identity()).expect("hash");
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    for index in 0..count {
        let peer = bundle(PEER_SEED_BASE + index as u64);
        let _ = store.insert(validated(&peer));
    }
    (store, local_hash)
}

/// The closest eligible floodfills, computed here from the store.
fn expected_closest(store: &RouterInfoStore, local_hash: &RouterHash) -> Vec<RouterHash> {
    let mut candidates: Vec<(RouterHash, [u8; 32])> = store
        .floodfill_advertisers()
        .map(|record| {
            let key = record.key();
            let mut distance = [0u8; 32];
            for (index, byte) in distance.iter_mut().enumerate() {
                *byte = key.as_bytes()[index] ^ local_hash.as_bytes()[index];
            }
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

/// The lane's owner, with a cycle begun.
fn owner_with_cycle(local_hash: RouterHash) -> RouterInfoPublicationOwner {
    let mut owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    assert!(owner.begin_cycle(), "the lane's cycle must begin");
    owner
}

/// The batch the cycle asks first.
fn first_batch(owner: &RouterInfoPublicationOwner, store: &RouterInfoStore) -> Vec<RouterHash> {
    match owner.cycle_step(store) {
        RouterInfoPublicationStep::Ask(targets) => targets,
        RouterInfoPublicationStep::Exhausted => panic!("the first batch must not be exhausted"),
        RouterInfoPublicationStep::NoTarget => panic!("the first batch must not be empty"),
    }
}

#[test]
fn a_cycle_asks_the_closest_eligible_targets() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let owner = owner_with_cycle(local_hash);
    let targets = first_batch(&owner, &store);
    assert_eq!(
        targets,
        expected_closest(&store, &local_hash),
        "the cycle must ask the closest eligible targets"
    );
    assert!(
        targets.len() <= PUBLICATION_FLOODFILLS,
        "the fan-out must stay bounded at the tree's three"
    );
}

#[test]
fn the_second_batch_excludes_the_targets_already_asked() {
    let (store, local_hash) = store_with_eligible_floodfills(8);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    let second = first_batch(&owner, &store);
    for target in &first {
        assert!(
            !second.contains(target),
            "a target the cycle already asked must never be asked twice"
        );
    }
    assert!(
        !second.is_empty(),
        "the cycle must still have a target to ask"
    );
}

#[test]
fn a_cycle_is_not_retried_before_the_reference_deadline() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    assert!(
        !owner.cycle_expired(T0),
        "a cycle must not be retried before the batch was sent"
    );
    assert!(
        !owner.cycle_expired(T0 + PUBLICATION_CONFIRMATION_TIMEOUT_MS - 1),
        "a cycle must not be retried before the reference's timeout"
    );
    assert_eq!(
        owner.cycle().deadline_ms(),
        Some(T0 + PUBLICATION_CONFIRMATION_TIMEOUT_MS),
        "the deadline must be the reference's timeout from the moment the batch went out"
    );
}

#[test]
fn a_cycle_is_retried_when_the_deadline_passes_with_no_confirmation() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    assert!(
        owner.cycle_expired(T0 + PUBLICATION_CONFIRMATION_TIMEOUT_MS),
        "a cycle must be retried once the reference's timeout has passed"
    );
    // The rung that follows the awaited batch.
    owner.abandon_batch(T0 + PUBLICATION_CONFIRMATION_TIMEOUT_MS);
    let second = first_batch(&owner, &store);
    assert!(
        !first.iter().any(|target| second.contains(target)),
        "the retry must ask a target the cycle has not asked"
    );
}

#[test]
fn a_delivery_refusal_advances_at_once() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    assert!(
        !owner.cycle_expired(T0),
        "an awaited batch is not due at once"
    );
    assert!(
        owner.abandon_batch(T0),
        "an awaited batch must be abandonable"
    );
    assert!(
        owner.cycle_expired(T0),
        "a refused batch must advance at once rather than waiting out the timeout"
    );
}

#[test]
fn a_batch_that_was_never_asked_cannot_be_abandoned() {
    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    assert!(
        !owner.abandon_batch(T0),
        "nothing is awaited, so nothing can be abandoned"
    );
    assert!(!owner.cycle_expired(T0), "a cycle with no batch is not due");
}

#[test]
fn a_confirmation_ends_the_cycle() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    // Begin the attempts through the correlator, so the acknowledgement
    // has a live attempt to confirm.
    let plan = RouterInfoPublicationPlan::publish(first.clone(), b"router info bytes".to_vec());
    let attempts = begin_attempts(owner.coordinator_mut(), local_hash, &plan, &store);
    assert_eq!(
        attempts.len(),
        first.len(),
        "every target begins an attempt"
    );
    let token = attempts[0].attempt.reply_token();
    owner.record_batch(T0, &first);
    assert!(owner.cycle_in_progress(), "the cycle is running");
    let confirmation = owner.confirm(token);
    assert!(
        matches!(confirmation, PublicationConfirmation::Confirmed { .. }),
        "the acknowledgement must confirm the attempt it names: {confirmation:?}"
    );
    assert!(
        !owner.cycle_in_progress(),
        "a confirmation must end the cycle"
    );
    assert!(
        owner.cycle().asked().is_empty(),
        "the reference clears its exclusion set on a confirmation"
    );
    assert_eq!(
        owner.schedule().counters().confirmed,
        1,
        "the confirmation is counted"
    );
}

#[test]
fn an_unmatched_acknowledgement_does_not_end_the_cycle() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    let confirmation = owner.confirm(0xDEAD_BEEF);
    assert_eq!(
        confirmation,
        PublicationConfirmation::Unmatched,
        "an acknowledgement that matches no attempt must be refused"
    );
    assert!(
        owner.cycle_in_progress(),
        "a refused acknowledgement must not end the cycle"
    );
    assert_eq!(
        owner.schedule().counters().unmatched,
        1,
        "the refusal is counted under its own reason"
    );
}

#[test]
fn the_cycle_is_bounded_by_the_reference_fifteen() {
    let (store, local_hash) = store_with_eligible_floodfills(24);
    let mut owner = owner_with_cycle(local_hash);
    let mut asked = 0usize;
    let mut batches = 0usize;
    // The loop is **bounded**: a mutation that stopped the exclusion set
    // from growing must fail an assertion here, not spin forever.
    for _ in 0..(PUBLICATION_CYCLE_TARGETS + 2) {
        let targets = match owner.cycle_step(&store) {
            RouterInfoPublicationStep::Ask(targets) => targets,
            RouterInfoPublicationStep::Exhausted => break,
            RouterInfoPublicationStep::NoTarget => panic!("the store holds eligible floodfills"),
        };
        asked += targets.len();
        batches += 1;
        owner.record_batch(T0 + batches as u64, &targets);
        if owner.cycle_reached_bound() {
            break;
        }
    }
    assert_eq!(
        asked, PUBLICATION_CYCLE_TARGETS,
        "the cycle must ask at most the reference's bound"
    );
    assert!(
        owner.cycle_reached_bound(),
        "the cycle must report that it reached the bound"
    );
    assert_eq!(
        owner.cycle_step(&store),
        RouterInfoPublicationStep::Exhausted,
        "the bound must end the cycle"
    );
    assert!(
        owner.cycle().asked().len() <= PUBLICATION_CYCLE_TARGETS,
        "the exclusion set stays bounded by the reference's bound"
    );
}

#[test]
fn a_cycle_with_no_eligible_target_left_ends_without_exhausting() {
    let (store, local_hash) = store_with_eligible_floodfills(4);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    // The store now holds only the floodfills the cycle already asked.
    let mut remaining = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    for target in &first {
        if let Some(record) = store.get(target) {
            let _ = remaining.insert(record.clone());
        }
    }
    assert_eq!(
        owner.cycle_step(&remaining),
        RouterInfoPublicationStep::NoTarget,
        "a cycle with no eligible target left must say so"
    );
    assert!(
        !owner.cycle_reached_bound(),
        "a cycle that ran out of targets has not reached the bound"
    );
}

#[test]
fn a_batch_beyond_the_first_is_counted_as_a_retry() {
    let (store, local_hash) = store_with_eligible_floodfills(8);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    assert_eq!(
        owner.schedule().counters().retries,
        0,
        "the cycle's first batch is not a retry"
    );
    let second = first_batch(&owner, &store);
    owner.record_batch(T0 + PUBLICATION_CONFIRMATION_TIMEOUT_MS, &second);
    assert_eq!(
        owner.schedule().counters().retries,
        1,
        "a batch beyond the first is counted as a retry"
    );
    assert_eq!(owner.cycle().batches(), 2, "the cycle counts its batches");
}

#[test]
fn a_retry_does_not_move_the_publication_period() {
    let (store, local_hash) = store_with_eligible_floodfills(8);
    let mut owner = owner_with_cycle(local_hash);
    let first = first_batch(&owner, &store);
    owner.record_batch(T0, &first);
    // The retry is an attempt, and it is counted under its own outcome.
    owner.schedule_mut().record_retry(
        i2pr_daemon::router_info_publication::RouterInfoPublicationOutcome::Published {
            targets: 3,
            accepted: 3,
        },
    );
    assert_eq!(
        owner.schedule().counters().attempts,
        1,
        "the retry is counted as an attempt"
    );
    assert_eq!(
        owner.schedule().last_attempt_ms(),
        None,
        "a retry must not move the publication's period"
    );
}

#[test]
fn the_cycle_depends_only_on_the_reading_it_is_given() {
    // Twelve floodfills: three batches of three, so every step has a
    // target to ask.
    let (store, local_hash) = store_with_eligible_floodfills(12);
    let mut first = owner_with_cycle(local_hash);
    let mut second = owner_with_cycle(local_hash);
    // The same readings, the same steps: the two cycles must agree, so
    // nothing in the cycle depends on a clock the lane never supplied.
    for step in 0u64..3 {
        let now_ms = T0 + step * PUBLICATION_CONFIRMATION_TIMEOUT_MS;
        let one = first_batch(&first, &store);
        let two = first_batch(&second, &store);
        assert_eq!(one, two, "the two cycles must ask the same targets");
        first.record_batch(now_ms, &one);
        second.record_batch(now_ms, &two);
        assert_eq!(
            first.cycle_expired(now_ms),
            second.cycle_expired(now_ms),
            "the two cycles must expire together"
        );
    }
    assert_eq!(
        first.cycle().deadline_ms(),
        second.cycle().deadline_ms(),
        "the two cycles must hold the same deadline"
    );
}

#[test]
fn a_cycle_does_not_start_over_a_running_one() {
    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner_with_cycle(local_hash);
    assert!(
        !owner.begin_cycle(),
        "a running cycle must not be started over"
    );
    owner.halt_cycle();
    assert!(
        owner.begin_cycle(),
        "a halted cycle must leave room for a fresh one"
    );
    assert!(
        owner.cycle().asked().is_empty(),
        "a fresh cycle starts with an empty exclusion set"
    );
}
