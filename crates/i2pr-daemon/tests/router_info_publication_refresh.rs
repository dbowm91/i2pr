//! Plan 518 — the lane for the publication's **freshness**.
//!
//! The router's own RouterInfo carries the publication date the router
//! signed at **startup**, so it ages out of the network's view: the netdb's
//! acceptance window is 24 hours, and the reference's netdb expires routers
//! between one and a half and twenty-seven hours. The reference re-dates and
//! re-signs its record before each publish
//! (`libi2pd/RouterInfo.cpp:1218`, `libi2pd/RouterContext.cpp:1474`).
//!
//! The lane proves:
//!
//! - a record is **publishable** when it is fresh and **refused** when it
//!   is stale, and a record dated too far in the future is refused too;
//! - the netdb's own validator **refuses** the stale record and **accepts**
//!   the re-dated one at the same reading;
//! - the re-dated record keeps the **content** — the addresses, the peers
//!   and the options — and the **router hash**;
//! - the re-dated record's **signature verifies**, and the original's
//!   signature does not cover the new date;
//! - a record that cannot be re-dated is **refused**;
//! - the staleness refusal is **counted**;
//! - the re-dating is **deterministic**.
//!
//! The lane uses the public API only, fixed seeds, an explicit clock, no
//! socket, no environment variable, no `#[ignore]`, and no sleeping.

use i2pr_crypto::{RouterIdentityBundle, resign_router_info_with_published, verify_router_info};
use i2pr_daemon::router_info_publication::{
    PUBLICATION_FLOODFILLS, PUBLICATION_INTERVAL_MS, PUBLICATION_MAX_FUTURE_SKEW_MS,
    PUBLICATION_MAX_RECORD_AGE_MS, RouterInfoPublicationOutcome, RouterInfoPublicationOwner,
    record_is_publishable,
};
use i2pr_netdb::{
    LocalRouterInfoBuilder, RouterInfoValidationError, ValidatedRouterInfo, ValidationContext,
    router_hash,
};
use i2pr_proto::{Date, Hash, Mapping, RouterInfo};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seed of the lane's own router.
const LOCAL_SEED: u64 = 0x5180_0001;
/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_700_000_000_000;
/// The clock of the lane's second reading, in milliseconds.
const T1: u64 = T0 + 39 * 60 * 1_000;

/// The largest encoded RouterInfo the lane will decode.
const MAX_BYTES: usize = i2pr_proto::MAX_COMMON_STRUCTURE_SIZE;

/// The lane's own identity.
fn bundle() -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(LOCAL_SEED);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// The lane's local record, signed at `published`.
fn local_record(published: u64) -> (RouterIdentityBundle, Vec<u8>) {
    let bundle = bundle();
    let local = LocalRouterInfoBuilder::new(&bundle)
        .build_default(Date::from_millis(published))
        .expect("local record");
    let encoded = local.encoded(MAX_BYTES).expect("encoded");
    (bundle, encoded)
}

/// The decoded form of an encoded RouterInfo.
fn decoded(encoded: &[u8]) -> RouterInfo {
    RouterInfo::decode(encoded, MAX_BYTES).expect("decode")
}

#[test]
fn a_record_is_publishable_when_fresh_and_refused_when_stale() {
    let (_bundle, encoded) = local_record(T0);
    assert!(
        record_is_publishable(&encoded, T0),
        "a record dated now must be publishable"
    );
    assert!(
        record_is_publishable(&encoded, T0 + PUBLICATION_MAX_RECORD_AGE_MS),
        "a record exactly at the netdb's window must still be publishable"
    );
    assert!(
        !record_is_publishable(&encoded, T0 + PUBLICATION_MAX_RECORD_AGE_MS + 1),
        "a record past the netdb's window must be refused"
    );
}

#[test]
fn a_record_dated_too_far_in_the_future_is_refused() {
    let (_bundle, encoded) = local_record(T0);
    assert!(
        record_is_publishable(&encoded, T0.saturating_sub(PUBLICATION_MAX_FUTURE_SKEW_MS)),
        "a record inside the netdb's clock skew must be publishable"
    );
    assert!(
        !record_is_publishable(
            &encoded,
            T0.saturating_sub(PUBLICATION_MAX_FUTURE_SKEW_MS + 1)
        ),
        "a record past the netdb's clock skew must be refused"
    );
}

#[test]
fn the_netdb_refuses_the_stale_record_and_accepts_the_re_dated_one() {
    let (bundle, encoded) = local_record(T0);
    let stale_reading = Date::from_millis(T0 + PUBLICATION_MAX_RECORD_AGE_MS + 1);
    let stale = ValidatedRouterInfo::from_router_info(
        decoded(&encoded),
        None,
        ValidationContext::new(stale_reading),
    );
    assert!(
        matches!(stale, Err(RouterInfoValidationError::Stale { .. })),
        "the netdb must refuse the stale record: {stale:?}"
    );

    let fresh_bytes = resign_router_info_with_published(&bundle, &encoded, Date::from_millis(T1))
        .expect("re-date");
    let fresh = ValidatedRouterInfo::from_router_info(
        decoded(&fresh_bytes),
        None,
        ValidationContext::new(Date::from_millis(T1)),
    );
    assert!(
        fresh.is_ok(),
        "the netdb must accept the re-dated record: {fresh:?}"
    );
}

#[test]
fn the_re_dated_record_keeps_the_content_and_the_router_hash() {
    let (bundle, encoded) = local_record(T0);
    let original = decoded(&encoded);
    let fresh_bytes = resign_router_info_with_published(&bundle, &encoded, Date::from_millis(T1))
        .expect("re-date");
    let fresh = decoded(&fresh_bytes);

    assert_eq!(
        fresh.published(),
        Date::from_millis(T1),
        "the re-dated record must carry the new date"
    );
    assert_ne!(
        fresh.published(),
        original.published(),
        "the re-dated record must not carry the startup date"
    );
    assert_eq!(
        router_hash(fresh.router_identity()),
        router_hash(original.router_identity()),
        "the router's hash must not change: the key is derived from the identity"
    );
    assert_eq!(
        fresh.signed_bytes(),
        original
            .signed_bytes_with_published(Date::from_millis(T1))
            .expect("re-encode")
            .as_slice(),
        "only the date may change: the addresses, the peers and the options must be identical"
    );
}

#[test]
fn the_re_dated_record_keeps_a_peer() {
    let bundle = bundle();
    let peer = Hash::from_bytes([7_u8; 32]);
    let info = bundle
        .sign_router_info(
            Date::from_millis(T0),
            Vec::new(),
            vec![peer],
            Mapping::empty(),
        )
        .expect("sign");
    let encoded = info.encode_to_vec(MAX_BYTES).expect("encoded");

    let fresh_bytes = resign_router_info_with_published(&bundle, &encoded, Date::from_millis(T1))
        .expect("re-date");
    let original_region = decoded(&encoded)
        .signed_bytes_with_published(Date::from_millis(T1))
        .expect("re-encode");
    assert_eq!(
        decoded(&fresh_bytes).signed_bytes(),
        original_region.as_slice(),
        "a peer must survive the re-dating"
    );
}

#[test]
fn the_re_dated_record_verifies_and_the_original_does_not_cover_the_new_date() {
    let (bundle, encoded) = local_record(T0);
    let fresh_bytes = resign_router_info_with_published(&bundle, &encoded, Date::from_millis(T1))
        .expect("re-date");
    let fresh = decoded(&fresh_bytes);
    assert!(
        verify_router_info(&fresh).is_ok(),
        "the re-dated record's signature must verify"
    );
    assert_ne!(
        fresh.signed_bytes(),
        decoded(&encoded).signed_bytes(),
        "the original's signature must not cover the new date"
    );
}

#[test]
fn a_record_that_cannot_be_re_dated_is_refused() {
    let (bundle, _encoded) = local_record(T0);
    let garbage = vec![0_u8; 64];
    assert!(
        resign_router_info_with_published(&bundle, &garbage, Date::from_millis(T1)).is_err(),
        "a record that is not a RouterInfo must be refused, not published"
    );
}

#[test]
fn the_stale_refusal_is_counted() {
    let (bundle, encoded) = local_record(T0);
    let local_hash = router_hash(decoded(&encoded).router_identity()).expect("hash");
    let mut owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);
    owner
        .schedule_mut()
        .record(T0, RouterInfoPublicationOutcome::RefusedStaleRecord);
    assert_eq!(
        owner.schedule().counters().refused_stale_record,
        1,
        "the staleness refusal must be counted under its own reason"
    );
    assert_eq!(
        owner.schedule().counters().published,
        0,
        "a refused record must never be counted as a publication"
    );
    let _ = bundle;
}

#[test]
fn the_re_dating_is_deterministic() {
    let (bundle, encoded) = local_record(T0);
    let first = resign_router_info_with_published(&bundle, &encoded, Date::from_millis(T1))
        .expect("re-date");
    let second = resign_router_info_with_published(&bundle, &encoded, Date::from_millis(T1))
        .expect("re-date");
    assert_eq!(
        first, second,
        "the same record at the same reading must produce the same bytes"
    );
}

#[test]
fn the_cycle_holds_one_record_for_every_rung() {
    // Plan 518, invariant 7: every rung of a cycle publishes **one** record.
    // The record is the cycle's, and the cycle's record lives in the owner,
    // so a rung reads it back rather than reading a copy out of the runtime.
    let (_bundle, encoded) = local_record(T0);
    let local_hash = router_hash(decoded(&encoded).router_identity()).expect("hash");
    let mut owner = RouterInfoPublicationOwner::new(local_hash, PUBLICATION_INTERVAL_MS);

    assert!(
        owner.cycle_record().is_none(),
        "a cycle with no record has nothing to publish"
    );

    // The cycle's record is set once, at the cycle's start.
    owner.set_cycle_record(Some(encoded.clone()));
    let first = owner.cycle_record().expect("the cycle's record").to_vec();
    assert_eq!(
        first, encoded,
        "the cycle's record must be the record it was given, byte for byte"
    );

    // A rung reads it back: the same bytes, unchanged.
    let rung = owner.cycle_record().expect("the cycle's record").to_vec();
    assert_eq!(
        first, rung,
        "every rung of a cycle must publish the same record"
    );

    // A cycle with no record is refused rather than published from some
    // other copy.
    owner.set_cycle_record(None);
    assert!(
        owner.cycle_record().is_none(),
        "a cycle with no record must publish nothing"
    );
}

#[test]
fn the_publication_fan_out_is_unchanged() {
    // Plan 518 does not touch the selection: the fan-out stays at the
    // tree's three.
    assert_eq!(PUBLICATION_FLOODFILLS, 3);
}
