//! Plan 519 — the lane for the publication's **own task**.
//!
//! The router's own RouterInfo publication used to run inside the SSU2
//! service's `select!` loop, where its delivery awaited a session
//! establishment and stalled the loop's relay drain and its inbound
//! acknowledgement owner. Plan 519 gives the publication its **own** task,
//! with its own ownership, its own cancellation and its own bounded
//! delivery timeout; the transport's loop never awaits it.
//!
//! The lane proves the publication's **public** behaviour, one assertion
//! per test:
//!
//! 1. the selection **prefers** a peer the router is already connected to;
//! 2. the preference keeps the selection's own order;
//! 3. the preference never **adds** a target the selection did not name;
//! 4. a batch with no established session yields an empty preference and
//!    counts every target it skipped;
//! 5. the skipped targets are counted under **their own reason**;
//! 6. the delivery timeout is **bounded** and is the reference's
//!    `PUBLISH_TIMEOUT`;
//! 7. a delivery that timed out is refused under **its own reason** and is
//!    never counted as a publication;
//! 8. the acknowledgement handoff is **bounded**;
//! 9. a full handoff counts the acknowledgement as **unmatched** -- never
//!    dropped silently;
//! 10. the handoff **never blocks** the loop;
//! 11. a cancelled publication is counted under **its own reason** and does
//!     not move the publication's period;
//! 12. the ladder's timing is unchanged -- the reference's 1600
//!     milliseconds, armed from the moment the batch went out;
//! 13. the correlator's matching is unchanged;
//! 14. Plan 518's staleness rule is unchanged;
//! 15. the publication's period and fan-out are unchanged.
//!
//! The lane is local and deterministic:
//!
//! - fixed seeds and an explicit clock reading;
//! - no socket, no environment variable, no `#[ignore]`, no stock peer, and
//!   no sleeping.
//!
//! One behaviour is **not** drivable through the public API and is reported
//! rather than papered over: the wiring that gives the publication its task
//! -- the composition root's spawn, and the transport's loop losing its
//! publication arm -- needs the daemon's own service composition. That
//! wiring is covered by the structural guard
//! (`router_info_publication_off_loop_boundary.rs`), which reads the
//! production files.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::router_info_publication::{
    PUBLICATION_CONFIRMATION_TIMEOUT_MS, PUBLICATION_CYCLE_TARGETS,
    PUBLICATION_DELIVERY_TIMEOUT_MS, PUBLICATION_FLOODFILLS, PUBLICATION_INTERVAL_MS,
    PUBLICATION_MAX_RECORD_AGE_MS, PublicationConfirmation, RouterInfoPublicationOutcome,
    RouterInfoPublicationOwner, RouterInfoPublicationPlan, begin_attempts,
    prefer_established_sessions, record_is_publishable,
};
use i2pr_daemon::router_info_publication_task::{
    PUBLICATION_ACKNOWLEDGEMENT_CAPACITY, PUBLICATION_LADDER_TICK_MS,
    publication_acknowledgement_channel,
};
use i2pr_netdb::{
    LocalRouterInfoBuilder, RouterHash, RouterInfoStore, RouterInfoStoreConfig,
    ValidatedRouterInfo, ValidationContext, router_hash,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seed of the lane's own router.
const LOCAL_SEED: u64 = 0x5190_0001;
/// The first seed of the lane's peers.
const PEER_SEED_BASE: u64 = 0x5190_1000;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_700_000_000_000;

/// The version a peer publishes when it is eligible for a DSM.
const ELIGIBLE_VERSION: &str = "0.9.62";

/// The largest encoded RouterInfo the lane will decode.
const MAX_BYTES: usize = i2pr_proto::MAX_COMMON_STRUCTURE_SIZE;

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

/// The lane's owner.
fn owner(local_hash: RouterHash) -> RouterInfoPublicationOwner {
    RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS)
}

#[test]
fn the_selection_prefers_an_established_session() {
    // Plan 519: the publication prefers a peer the router is already
    // connected to, because that is the peer whose send needs no handshake.
    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let targets = vec![
        RouterHash::from_bytes([1_u8; 32]),
        RouterHash::from_bytes([2_u8; 32]),
        RouterHash::from_bytes([3_u8; 32]),
    ];
    let established = vec![*targets[1].as_hash()];
    let preference = prefer_established_sessions(&targets, &established);
    assert_eq!(
        preference.targets,
        vec![targets[1]],
        "the batch must prefer the peer that already holds a session"
    );
    assert_eq!(
        preference.skipped, 2,
        "the targets skipped for want of an established session must be counted"
    );
    let _ = local_hash;
}

#[test]
fn the_preference_keeps_the_selection_order() {
    let targets = vec![
        RouterHash::from_bytes([4_u8; 32]),
        RouterHash::from_bytes([5_u8; 32]),
        RouterHash::from_bytes([6_u8; 32]),
    ];
    let established = vec![*targets[2].as_hash(), *targets[0].as_hash()];
    let preference = prefer_established_sessions(&targets, &established);
    assert_eq!(
        preference.targets,
        vec![targets[0], targets[2]],
        "the preference must keep the selection's own order among the established peers"
    );
}

#[test]
fn the_preference_never_adds_a_target() {
    let targets = vec![
        RouterHash::from_bytes([7_u8; 32]),
        RouterHash::from_bytes([8_u8; 32]),
    ];
    let established = vec![
        *targets[0].as_hash(),
        *RouterHash::from_bytes([9_u8; 32]).as_hash(),
    ];
    let preference = prefer_established_sessions(&targets, &established);
    for target in &preference.targets {
        assert!(
            targets.contains(target),
            "the preference must never name a target the selection did not"
        );
    }
    assert_eq!(
        preference.skipped, 1,
        "a target the selection named and the preference dropped is counted"
    );
}

#[test]
fn a_batch_with_no_established_session_yields_an_empty_preference() {
    let targets = vec![
        RouterHash::from_bytes([10_u8; 32]),
        RouterHash::from_bytes([11_u8; 32]),
    ];
    let preference = prefer_established_sessions(&targets, &[]);
    assert!(
        preference.targets.is_empty(),
        "no target holds a session, so the preference names none"
    );
    assert_eq!(
        preference.skipped, 2,
        "every skipped target must be counted"
    );
}

#[test]
fn the_skipped_targets_are_counted_under_their_own_reason() {
    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner(local_hash);
    owner.schedule_mut().record_skipped_no_session(2);
    assert_eq!(
        owner.schedule().counters().skipped_no_session,
        2,
        "the skipped targets must be counted under their own reason"
    );
    assert_eq!(
        owner.schedule().counters().attempts,
        0,
        "a skipped target is not an attempt"
    );
    assert_eq!(
        owner.schedule().counters().published,
        0,
        "a skipped target is never a publication"
    );
    assert_eq!(
        owner.schedule().counters().deliveries_refused,
        0,
        "the skip reason must not be merged with the delivery refusal"
    );
}

#[test]
fn the_delivery_timeout_is_bounded_and_is_the_references_own() {
    // Plan 519: the timeout belongs to the publication. The reference
    // bounds one store attempt at 90 seconds
    // (`FloodfillNetworkDatabaseFacade.java:174`).
    let timeout = PUBLICATION_DELIVERY_TIMEOUT_MS;
    assert_eq!(
        timeout, 90_000,
        "the publication's delivery timeout must be the reference's 90 seconds"
    );
    assert!(
        timeout > 0,
        "the delivery timeout must be bounded and never zero"
    );
    assert!(timeout <= 120_000, "the delivery timeout must stay bounded");
}

#[test]
fn a_delivery_that_timed_out_is_refused_under_its_own_reason() {
    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner(local_hash);
    owner.schedule_mut().record(
        T0,
        RouterInfoPublicationOutcome::DeliveryTimedOut {
            targets: 3,
            timed_out: 1,
        },
    );
    assert_eq!(
        owner.schedule().counters().deliveries_timed_out,
        1,
        "the timeout must be counted under its own reason"
    );
    assert_eq!(
        owner.schedule().counters().published,
        0,
        "a timed-out delivery is never a publication"
    );
    assert_eq!(
        owner.schedule().counters().deliveries_refused,
        0,
        "the timeout must not be merged with the delivery refusal"
    );
    assert_eq!(
        owner.schedule().counters().attempts,
        1,
        "the attempt is still counted"
    );
}

#[test]
fn the_acknowledgement_handoff_is_bounded() {
    let capacity = PUBLICATION_ACKNOWLEDGEMENT_CAPACITY;
    assert!(
        capacity > 0,
        "the handoff must carry at least one acknowledgement"
    );
    assert!(capacity <= 64, "the handoff must stay bounded");
    assert_eq!(
        PUBLICATION_LADDER_TICK_MS, 100,
        "the ladder's tick is the publication's own 100 millisecond cadence"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_full_handoff_counts_the_acknowledgement_as_unmatched() {
    let (handoff, inbox) =
        publication_acknowledgement_channel(PUBLICATION_ACKNOWLEDGEMENT_CAPACITY);
    for index in 0..PUBLICATION_ACKNOWLEDGEMENT_CAPACITY {
        assert!(
            handoff.acknowledge(index as u32 + 1),
            "the handoff must carry an acknowledgement while it has room"
        );
    }
    // The channel is full: the next acknowledgement is refused, and it is
    // counted as **unmatched** rather than dropped silently.
    assert!(
        !handoff.acknowledge(0xDEAD_BEEF),
        "a full handoff must refuse the acknowledgement"
    );

    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner(local_hash);
    let folded = inbox.fold_overflow(&mut owner);
    assert_eq!(folded, 1, "the refused acknowledgement must be counted");
    assert_eq!(
        owner.schedule().counters().unmatched,
        1,
        "a refused acknowledgement is refused under the unmatched reason"
    );
    assert_eq!(
        owner.schedule().counters().confirmed,
        0,
        "a refused acknowledgement confirms nothing"
    );
    // The fold is a read-and-clear: a second fold counts nothing twice.
    assert_eq!(
        inbox.fold_overflow(&mut owner),
        0,
        "the fold must not count the same refusal twice"
    );
    assert_eq!(
        owner.schedule().counters().unmatched,
        1,
        "the refusal must be counted exactly once"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_handoff_never_blocks_the_loop() {
    let (handoff, _inbox) =
        publication_acknowledgement_channel(PUBLICATION_ACKNOWLEDGEMENT_CAPACITY);
    for index in 0..PUBLICATION_ACKNOWLEDGEMENT_CAPACITY {
        assert!(handoff.acknowledge(index as u32 + 1));
    }
    // The channel is full and the publication's task is not draining it.
    // The loop's call must still return at once: it may not wait for room.
    let refused = tokio::time::timeout(Duration::from_millis(50), async {
        handoff.acknowledge(0xDEAD_BEEF)
    })
    .await
    .expect("the handoff must never block the transport's loop");
    assert!(
        !refused,
        "the acknowledgement must be refused rather than waited for"
    );
}

#[test]
fn a_cancelled_publication_is_counted_under_its_own_reason() {
    let (_store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner(local_hash);
    owner.schedule_mut().record_cancelled();
    assert_eq!(
        owner.schedule().counters().cancelled,
        1,
        "the cancellation must be counted under its own reason"
    );
    assert_eq!(
        owner.schedule().counters().published,
        0,
        "a cancelled publication is never a publication"
    );
    assert_eq!(
        owner.schedule().last_attempt_ms(),
        None,
        "a cancellation must not move the publication's period"
    );
}

#[test]
fn the_ladders_timing_is_unchanged() {
    // Plan 517's timing, unchanged by Plan 519: the reference's
    // `ROUTER_INFO_CONFIRMATION_TIMEOUT`, armed from the moment the batch
    // went out.
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner(local_hash);
    assert!(owner.begin_cycle(), "the lane's cycle must begin");
    let targets = match owner.cycle_step(&store) {
        i2pr_daemon::router_info_publication::RouterInfoPublicationStep::Ask(targets) => targets,
        other => panic!("the first batch must be askable: {other:?}"),
    };
    owner.record_batch(T0, &targets);
    assert_eq!(
        owner.cycle().deadline_ms(),
        Some(T0 + PUBLICATION_CONFIRMATION_TIMEOUT_MS),
        "the deadline must still be the reference's timeout from the moment the batch went out"
    );
    assert_eq!(
        PUBLICATION_CONFIRMATION_TIMEOUT_MS, 1_600,
        "the reference's confirmation timeout must not change"
    );
    assert_eq!(
        owner.cycle().asked().len(),
        targets.len(),
        "the cycle's exclusion set must still hold the asked targets"
    );
    assert!(
        owner.cycle().asked().len() <= PUBLICATION_CYCLE_TARGETS,
        "the exclusion set stays bounded by the reference's bound"
    );
}

#[test]
fn the_correlators_matching_is_unchanged() {
    let (store, local_hash) = store_with_eligible_floodfills(6);
    let mut owner = owner(local_hash);
    assert!(owner.begin_cycle(), "the lane's cycle must begin");
    let targets = match owner.cycle_step(&store) {
        i2pr_daemon::router_info_publication::RouterInfoPublicationStep::Ask(targets) => targets,
        other => panic!("the first batch must be askable: {other:?}"),
    };
    let plan = RouterInfoPublicationPlan::publish(targets.clone(), b"router info bytes".to_vec());
    let attempts = begin_attempts(owner.coordinator_mut(), local_hash, &plan, &store);
    assert_eq!(attempts.len(), targets.len());
    let token = attempts[0].attempt.reply_token();
    owner.record_batch(T0, &targets);
    assert!(
        matches!(
            owner.confirm(token),
            PublicationConfirmation::Confirmed { .. }
        ),
        "the acknowledgement must confirm the attempt it names"
    );
    assert!(
        !owner.cycle_in_progress(),
        "a confirmation must end the cycle"
    );
    assert_eq!(owner.schedule().counters().confirmed, 1);
    assert_eq!(
        owner.confirm(0xDEAD_BEEF),
        PublicationConfirmation::Unmatched,
        "an acknowledgement that names no live attempt must still be refused"
    );
    assert_eq!(owner.schedule().counters().unmatched, 1);
}

#[test]
fn the_staleness_rule_is_unchanged() {
    // Plan 518's rule, unchanged by Plan 519: a record the network would
    // refuse is refused here too.
    let local = bundle(LOCAL_SEED);
    let record = LocalRouterInfoBuilder::new(&local)
        .build_default(Date::from_millis(T0))
        .expect("local record");
    let encoded = record.encoded(MAX_BYTES).expect("encoded");
    assert!(
        record_is_publishable(&encoded, T0),
        "a fresh record must still be publishable"
    );
    assert!(
        !record_is_publishable(&encoded, T0 + PUBLICATION_MAX_RECORD_AGE_MS + 1),
        "a stale record must still be refused"
    );
}

#[test]
fn the_period_and_the_fan_out_are_unchanged() {
    assert_eq!(
        PUBLICATION_INTERVAL_MS,
        39 * 60 * 1_000,
        "the publication's period must stay the reference's 39 minutes"
    );
    assert_eq!(
        PUBLICATION_FLOODFILLS, 3,
        "the publication's fan-out must stay bounded at the tree's three"
    );
}
