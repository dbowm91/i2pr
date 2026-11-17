//! Plan 516 — the lane for the publication's **acknowledgement**.
//!
//! The lane proves that the router's own RouterInfo publication asks for
//! the network's acknowledgement, and that the acknowledgement is
//! correlated, counted, and refused when it matches nothing:
//!
//! - the store message the publication puts on the wire carries the
//!   correlator's **minted** reply token, and the token is never zero;
//! - the reply form is the reference's **direct** reply — a zero reply
//!   tunnel id and our own hash as the gateway
//!   (`libi2pd/I2NPProtocol.cpp:285`);
//! - the store keys **our own** hash, and its payload is the record we
//!   registered, so the key, the gateway and the payload are one record;
//! - a `DeliveryStatus` that carries the token back **confirms** the
//!   attempt;
//! - an acknowledgement that matches no live attempt is **refused** and
//!   counted under its own reason, and confirms nothing;
//! - a **delivery** admission is not an **acknowledgement**: the two are
//!   counted separately, and a delivery alone never yields a confirmation;
//! - the attempt book stays **bounded**.
//!
//! The lane uses the public API only, fixed seeds, an explicit clock, no
//! socket, no environment variable, no `#[ignore]`, and no sleeping.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::router_info_publication::{
    PUBLICATION_INTERVAL_MS, PublicationConfirmation, RouterInfoPublicationOwner,
    RouterInfoPublicationPlan, begin_attempts, confirm_publication,
};
use i2pr_netdb::{
    LookupPolicy, PublicationAttemptState, PublicationCoordinator, RouterHash, RouterInfoStore,
    RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext, router_hash,
};
use i2pr_proto::{Date, Hash, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the lane's peers.
const LOCAL_SEED: u64 = 0x5160_0001;
const ALPHA_SEED: u64 = 0x5160_0002;
const BETA_SEED: u64 = 0x5160_0003;

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

/// The lane's store: two eligible floodfills, and the local hash.
fn store_with_eligible_floodfills() -> (RouterInfoStore, RouterHash) {
    let local = bundle(LOCAL_SEED);
    let local_hash = router_hash(local.identity()).expect("hash");
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    for seed in [ALPHA_SEED, BETA_SEED] {
        let peer = bundle(seed);
        let record = validated(&peer, Some(ELIGIBLE_VERSION));
        let _ = store.insert(record);
    }
    (store, local_hash)
}

/// The record the publication publishes: the encoded RouterInfo.
///
/// Plan 516: the record is registered **explicitly**, so the store
/// message's key, its reply gateway and its payload are one record.
fn local_record() -> (RouterHash, Vec<u8>) {
    let local = bundle(LOCAL_SEED);
    let local_hash = router_hash(local.identity()).expect("hash");
    let record = validated(&local, Some(ELIGIBLE_VERSION));
    let encoded = record
        .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    (local_hash, encoded)
}

/// The lane's publication: one attempt against the closest eligible
/// floodfill, begun through the shared path.
fn begun_attempt() -> (
    RouterInfoStore,
    RouterHash,
    PublicationCoordinator,
    i2pr_netdb::PublicationAttemptRecord,
) {
    let (store, local_hash) = store_with_eligible_floodfills();
    let (_, encoded) = local_record();
    let target = *store
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect::<Vec<_>>()
        .first()
        .expect("the store holds floodfills");
    let mut coordinator = PublicationCoordinator::new(LookupPolicy::default());
    let plan = RouterInfoPublicationPlan::publish(vec![target], encoded);
    let attempts = begin_attempts(&mut coordinator, local_hash, &plan, &store);
    assert_eq!(
        attempts.len(),
        1,
        "the lane's publication begins one attempt"
    );
    let attempt = attempts.into_iter().next().expect("attempt");
    (store, local_hash, coordinator, attempt)
}

#[test]
fn the_publication_carries_the_correlators_minted_token() {
    let (_store, _local_hash, _coordinator, attempt) = begun_attempt();
    let store_message = &attempt.store_message;
    assert_ne!(
        store_message.reply_token, 0,
        "the correlation token must reach the wire: a zero token asks for no acknowledgement"
    );
    assert_eq!(
        store_message.reply_token,
        attempt.attempt.reply_token(),
        "the wire token must be the correlator's own minted token"
    );
}

#[test]
fn the_store_uses_the_reference_s_direct_reply_form() {
    let (_store, local_hash, _coordinator, attempt) = begun_attempt();
    let store_message = &attempt.store_message;
    assert_eq!(
        store_message.reply_tunnel_id,
        Some(0),
        "a zero reply tunnel id means a direct reply"
    );
    assert_eq!(
        store_message.reply_gateway,
        Some(Hash::from_bytes(*local_hash.as_bytes())),
        "the reply gateway is our own hash"
    );
}

#[test]
fn the_store_keys_our_own_hash_and_carries_our_own_record() {
    let (store, local_hash, _coordinator, attempt) = begun_attempt();
    let store_message = &attempt.store_message;
    assert_eq!(
        store_message.key,
        Hash::from_bytes(*local_hash.as_bytes()),
        "the store must key our own hash"
    );
    let (_, encoded) = local_record();
    let i2pr_proto::DatabaseStoreData::RouterInfoCompressed(payload) = &store_message.data else {
        panic!("the store must carry the RouterInfo bytes");
    };
    let mut decoder = flate2::read::GzDecoder::new(payload.as_bytes());
    let mut recovered = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut recovered).expect("decompress");
    assert_eq!(
        recovered, encoded,
        "the store's payload must be the record the publication registered"
    );
    assert!(
        store.contains(&attempt.attempt.peer()),
        "the target must be a peer the store holds"
    );
}

#[test]
fn a_matching_acknowledgement_confirms_the_attempt() {
    let (_store, _local_hash, mut coordinator, attempt) = begun_attempt();
    let token = attempt.attempt.reply_token();
    let request_id = attempt.attempt.request_id();
    assert_eq!(
        confirm_publication(&mut coordinator, token),
        PublicationConfirmation::Confirmed { request_id },
        "the acknowledgement must confirm the attempt it names"
    );
    assert_eq!(
        coordinator.attempt(request_id).expect("attempt").state(),
        PublicationAttemptState::Acknowledged,
        "a confirmed attempt is acknowledged"
    );
    assert_eq!(
        coordinator.snapshot().acknowledged,
        1,
        "the coordinator counts the acknowledgement"
    );
}

#[test]
fn an_unknown_acknowledgement_is_refused_and_counted() {
    let (_store, local_hash, mut coordinator, attempt) = begun_attempt();
    let _ = attempt;
    assert_eq!(
        confirm_publication(&mut coordinator, 0xDEAD_BEEF),
        PublicationConfirmation::Unmatched,
        "an acknowledgement that matches no attempt must be refused"
    );
    assert_eq!(
        coordinator.snapshot().acknowledged,
        0,
        "a refused acknowledgement confirms nothing"
    );
    let mut owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    assert_eq!(
        owner.confirm(0xDEAD_BEEF),
        PublicationConfirmation::Unmatched,
        "the owner refuses an unmatched acknowledgement"
    );
    let counters = owner.schedule().counters();
    assert_eq!(counters.unmatched, 1, "the refusal is counted");
    assert_eq!(counters.confirmed, 0, "the refusal is not a confirmation");
}

#[test]
fn a_delivery_admission_is_not_an_acknowledgement() {
    let (_store, _local_hash, mut coordinator, attempt) = begun_attempt();
    let request_id = attempt.attempt.request_id();
    coordinator.mark_delivered(request_id).expect("delivered");
    assert_eq!(
        coordinator.attempt(request_id).expect("attempt").state(),
        PublicationAttemptState::Delivered,
        "a delivery admission leaves the attempt delivered"
    );
    assert_eq!(
        coordinator.snapshot().acknowledged,
        0,
        "a delivery admission is not an acknowledgement"
    );
}

#[test]
fn the_acknowledgement_is_counted_separately_from_the_publication() {
    // The owner owns the correlator, exactly as the SSU2 service's loop
    // does in production: the attempt and the acknowledgement live in one
    // attempt book.
    let (store, local_hash) = store_with_eligible_floodfills();
    let (_, encoded) = local_record();
    let target = *store
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect::<Vec<_>>()
        .first()
        .expect("the store holds floodfills");
    let mut owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    let plan = RouterInfoPublicationPlan::publish(vec![target], encoded);
    let attempts = begin_attempts(owner.coordinator_mut(), local_hash, &plan, &store);
    let attempt = attempts.into_iter().next().expect("attempt");
    let request_id = attempt.attempt.request_id();
    owner
        .coordinator_mut()
        .mark_delivered(request_id)
        .expect("delivered");
    owner.schedule_mut().record(
        T0,
        i2pr_daemon::router_info_publication::RouterInfoPublicationOutcome::Published {
            targets: 1,
            accepted: 1,
        },
    );
    assert_eq!(
        owner.confirm(attempt.attempt.reply_token()),
        PublicationConfirmation::Confirmed { request_id },
        "the acknowledgement confirms the attempt"
    );
    let counters = owner.schedule().counters();
    assert_eq!(
        counters.published, 1,
        "the publication is counted as published"
    );
    assert_eq!(
        counters.confirmed, 1,
        "the acknowledgement is counted as confirmed"
    );
}

#[test]
fn the_attempt_book_stays_bounded() {
    let (mut store, local_hash) = store_with_eligible_floodfills();
    // Fill the store with far more peers than the attempt book can hold.
    for index in 0..64u64 {
        let peer = bundle(0x5160_1000 + index);
        let record = validated(&peer, Some(ELIGIBLE_VERSION));
        let _ = store.insert(record);
    }
    let (_, encoded) = local_record();
    let targets: Vec<RouterHash> = store
        .floodfill_advertisers()
        .map(|record| record.key())
        .collect();
    assert!(
        targets.len() > 32,
        "the store holds more peers than the bound"
    );
    let mut coordinator = PublicationCoordinator::new(LookupPolicy::default());
    let target_count = targets.len();
    let plan = RouterInfoPublicationPlan::publish(targets, encoded);
    let attempts = begin_attempts(&mut coordinator, local_hash, &plan, &store);
    assert!(
        attempts.len() < target_count,
        "the attempt book must refuse to hold every peer"
    );
}
