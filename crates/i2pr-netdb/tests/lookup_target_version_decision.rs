//! Plan 509 — the lookup target is **not** version-gated: the executable
//! proof.
//!
//! Plan 503 recorded as a Medium finding that *the router still asks a
//! below-floor floodfill for a lookup*, and left it open because extending
//! the floor to the lookup path would change which floodfills the router can
//! ask at all — a product decision, and therefore a plan-of-record's work.
//!
//! Plan 509 makes that decision: **the lookup target is not version-gated.**
//! The decision rests on three claims about the *real* lookup path, and this
//! file proves all three:
//!
//! 1. **A below-floor floodfill's correct answer completes the lookup.** The
//!    responder's published API version is irrelevant, because what the router
//!    trusts is the answer's *validity*, not the answerer's version.
//! 2. **An incorrect answer is refused by the validation, not by the
//!    version.** The peer was **asked** — a version gate would have stopped
//!    the request before it was sent — and the answer was then refused on its
//!    own merits.
//! 3. **A silent below-floor floodfill is skipped, not re-asked.** The
//!    rotation moves on to the next candidate, and the lookup completes.
//!
//! Every row drives the engine through its **public surface** only. No
//! private field is touched, no socket is opened, and no peer identity leaks
//! into a diagnostic.
//!
//! The rows use the **lease-set2** lookup, because that is the lookup a
//! destination uses: the same engine, the same nearest-floodfill selection
//! and the same rotation that the router's destination path uses.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    DestinationHash, LeaseSet2Store, LeaseSet2ValidationContext, LookupAction, LookupId,
    LookupKind, LookupPolicy, LookupResult, MIN_FLOODFILL_VERSION_FOR_DSM, ReplyPath,
    ResponseOutcome, RouterHash, RouterInfoLookup, RouterInfoStore, StartOutcome,
    ValidatedRouterInfo, ValidationContext, dsm_target_eligibility,
    handle_database_store_lease_set2, published_version_of, select_floodfill_candidates,
    select_version_gated_floodfills,
};
use i2pr_proto::{
    CryptoKeyType, DatabaseStoreData, DatabaseStoreMessage, Date, Date32, Hash, Lease2, LeaseSet2,
    LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, Mapping, SignatureValue,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The publication moment every peer record in this file carries.
const PUBLISHED_MS: u64 = 1;

/// The moment the lease records in this file are published at, in seconds.
const PUBLISHED_SECONDS: u32 = 1_000;

/// The moment the lease records in this file are validated at, in seconds.
const NOW_SECONDS: u32 = 1_000;

/// A published API version **below** the tree's floodfill floor.
///
/// The specification's table attaches no duty to a lookup, so this version
/// is not "too old to ask" — it is simply a version the tree knows how to
/// compare and finds lower than the floor it applies to a **DSM**.
const BELOW_FLOOR_VERSION: &str = "0.9.58";

/// A published API version **at** the tree's floodfill floor.
const AT_FLOOR_VERSION: &str = "0.9.62";

/// The deterministic seed the rows draw their peers from.
const SEED_BASE: u64 = 0x509_0000;

/// The lookup request id the rows use.
const REQUEST_ID: u64 = 0x5091;

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs a floodfill advertiser that publishes `version`.
///
/// The record advertises the floodfill capability, so it is a candidate for
/// the nearest-floodfill selection, and it publishes an API version, so its
/// eligibility under the tree's DSM floor is decidable.
fn floodfill_at(signer: &RouterIdentityBundle, version: &str) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    options
        .insert("router.version".to_owned(), version.to_owned())
        .unwrap();
    let published = Date::from_millis(PUBLISHED_MS);
    let info = signer
        .sign_router_info(
            published,
            Vec::new(),
            Vec::new(),
            options.build().expect("mapping"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

/// Signs a floodfill advertiser that publishes **no** API version.
///
/// The record advertises the floodfill capability, so it is a candidate for
/// the nearest-floodfill selection, and it publishes nothing, so its
/// eligibility is decidable as *unknown*.
fn floodfill_without_version(signer: &RouterIdentityBundle) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    let published = Date::from_millis(PUBLISHED_MS);
    let info = signer
        .sign_router_info(
            published,
            Vec::new(),
            Vec::new(),
            options.build().expect("mapping"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

/// The canonical destination hash of a signer's destination.
fn destination_hash_for(signer: &RouterIdentityBundle) -> DestinationHash {
    let destination = i2pr_proto::Destination::new(signer.identity().key_and_cert().clone())
        .expect("destination");
    DestinationHash::from_hash(destination.hash().expect("hash"))
}

/// The lookup key a lease-set2 lookup files a record under.
///
/// It is the destination hash's own bytes, which is what the state machine
/// compares a `DatabaseStore`'s key against.
fn lookup_target_for(signer: &RouterIdentityBundle) -> RouterHash {
    RouterHash::from_hash(*destination_hash_for(signer).as_hash())
}

/// Builds a signed `LeaseSet2` for a signer's destination.
fn build_signed_ls2(
    signer: &RouterIdentityBundle,
    published_seconds: u32,
    expires_offset_seconds: u16,
    leases: Vec<Lease2>,
) -> LeaseSet2 {
    let destination = i2pr_proto::Destination::new(signer.identity().key_and_cert().clone())
        .expect("destination");
    let header = LeaseSet2Header::new(
        destination,
        published_seconds,
        expires_offset_seconds,
        LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0u8; 64])
        .expect("placeholder");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("encryption key"),
    ];
    let unsigned = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        leases,
        placeholder,
    )
    .expect("unsigned ls2");
    let signature = signer
        .signing_key()
        .sign(&unsigned.signature_preimage())
        .expect("sign");
    LeaseSet2::new(
        unsigned.header().clone(),
        unsigned.options().clone(),
        unsigned.encryption_keys().to_vec(),
        unsigned.leases().to_vec(),
        signature,
    )
    .expect("ls2")
}

/// A lease record that validates at [`NOW_SECONDS`].
fn valid_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    build_signed_ls2(
        signer,
        PUBLISHED_SECONDS,
        3_600,
        vec![Lease2::new(
            Hash::from_bytes([0x11; 32]),
            7,
            Date32::from_seconds(PUBLISHED_SECONDS + 600),
        )],
    )
}

/// A lease record that has already expired at [`NOW_SECONDS`].
///
/// The signature is genuine; only the record's freshness fails, so the
/// refusal that follows is the freshness check's and nothing else's.
fn expired_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    build_signed_ls2(
        signer,
        0,
        600,
        vec![Lease2::new(
            Hash::from_bytes([0x22; 32]),
            7,
            Date32::from_seconds(60),
        )],
    )
}

/// The `DatabaseStore` a floodfill answers a lease lookup with.
fn store_message(target: &RouterHash, ls2: LeaseSet2) -> DatabaseStoreMessage {
    DatabaseStoreMessage {
        key: Hash::from_bytes(*target.as_bytes()),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::LeaseSet2(Box::new(ls2)),
    }
}

/// The reply path every row hands to the lookup.
fn reply_path() -> ReplyPath {
    ReplyPath::new(RouterHash::from_bytes([0x77; 32]), 0xCAFE).expect("reply path")
}

/// Returns the peer an outcome asked.
fn asked_peer(outcome: StartOutcome) -> RouterHash {
    let StartOutcome::PendingAttempt(action) = outcome else {
        panic!("expected an attempt, got {outcome:?}");
    };
    let LookupAction::SendDatabaselookup { peer, .. } = action else {
        panic!("expected SendDatabaselookup");
    };
    peer
}

/// Asserts that a peer's published version is below the tree's floodfill
/// floor, and returns the peer.
fn assert_below_the_floor(record: &ValidatedRouterInfo) {
    let published = published_version_of(record);
    assert_eq!(
        dsm_target_eligibility(published),
        i2pr_proto::VersionEligibility::BelowFloor {
            floor: MIN_FLOODFILL_VERSION_FOR_DSM,
        },
        "the peer must be below the floor the tree applies to a DSM"
    );
}

/// Asserts that a peer's published version is at the tree's floodfill floor.
fn assert_at_the_floor(record: &ValidatedRouterInfo) {
    assert!(
        dsm_target_eligibility(published_version_of(record)).is_eligible(),
        "the peer must be eligible under the floor the tree applies to a DSM"
    );
}

#[test]
fn a_below_floor_floodfills_correct_answer_completes_the_lookup() {
    // The claim: the responder's version does not decide the lookup.
    //
    // The store holds exactly one floodfill, and it publishes a version
    // below the floor the tree applies to a DSM. The lookup asks it — the
    // version did not stop the request — and the peer answers with a valid
    // lease record. The lookup completes.
    let destination = bundle(SEED_BASE + 1);
    let peer = bundle(SEED_BASE + 2);
    let peer_record = floodfill_at(&peer, BELOW_FLOOR_VERSION);
    assert_below_the_floor(&peer_record);

    let mut store = RouterInfoStore::default();
    store.insert(peer_record.clone());
    let peer_key = peer_record.key();

    let target = lookup_target_for(&destination);
    let routing_key = RouterHash::from_bytes([0x21; 32]);
    let lookup_id = LookupId::new(REQUEST_ID, LookupKind::LeaseSet2, target);
    let mut lookup = RouterInfoLookup::new(LookupPolicy::default());

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    assert_eq!(
        asked_peer(lookup.handle_pending_after_path(&store, &routing_key)),
        peer_key,
        "a peer below the DSM floor is still asked for a lookup — the lookup \
         path is not version-gated"
    );

    let mut lease_store = LeaseSet2Store::default();
    let message = store_message(&target, valid_ls2(&destination));
    let outcome = handle_database_store_lease_set2(
        &mut lookup,
        &mut lease_store,
        lookup_id,
        &message,
        LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("handle");
    let ResponseOutcome::Completed(result) = outcome else {
        panic!("the answer must reach the caller, got {outcome:?}");
    };
    let LookupResult::LeaseSet2Success {
        lookup_id: completed,
        ..
    } = *result
    else {
        panic!("the lookup must succeed, got {result:?}");
    };
    assert_eq!(completed, lookup_id);
    assert!(
        lookup.active_lookup().is_none(),
        "a completed lookup holds no active state"
    );
}

#[test]
fn an_incorrect_answer_is_refused_by_the_validation_not_by_the_version() {
    // The claim: the refusal is the router's own answer check.
    //
    // The peer is **asked** — which a version gate would have prevented —
    // and its answer is then refused because the record has expired. The
    // lookup stays active, so the refusal is not a terminal verdict about
    // the peer either: the lookup may still ask somebody else.
    let destination = bundle(SEED_BASE + 3);
    let peer = bundle(SEED_BASE + 4);
    let peer_record = floodfill_at(&peer, BELOW_FLOOR_VERSION);
    assert_below_the_floor(&peer_record);

    let mut store = RouterInfoStore::default();
    store.insert(peer_record.clone());
    let peer_key = peer_record.key();

    let target = lookup_target_for(&destination);
    let routing_key = RouterHash::from_bytes([0x22; 32]);
    let lookup_id = LookupId::new(REQUEST_ID, LookupKind::LeaseSet2, target);
    let mut lookup = RouterInfoLookup::new(LookupPolicy::default());

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    assert_eq!(
        asked_peer(lookup.handle_pending_after_path(&store, &routing_key)),
        peer_key,
        "the request was sent, so no version gate stood in front of it"
    );

    let mut lease_store = LeaseSet2Store::default();
    let message = store_message(&target, expired_ls2(&destination));
    let outcome = handle_database_store_lease_set2(
        &mut lookup,
        &mut lease_store,
        lookup_id,
        &message,
        LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("handle");
    assert_eq!(
        outcome,
        ResponseOutcome::Continue,
        "an expired record is refused, and the refusal is the validation's"
    );
    assert!(
        lookup.active_lookup().is_some(),
        "a refused answer leaves the lookup active — the refusal is about the \
         record, not about the peer"
    );
    assert_eq!(
        lease_store.stats().record_count,
        0,
        "a refused record is never installed"
    );
}

#[test]
fn a_silent_below_floor_floodfill_is_skipped_and_the_lookup_completes() {
    // The claim: an unhelpful candidate costs one round trip, not the
    // lookup.
    //
    // Two floodfills: one below the floor, one at it. The routing key is
    // the below-floor peer's own key, so it is the nearest candidate and is
    // asked first. It does not answer, and the rotation moves on to the
    // second peer — the first is never asked again. The second peer answers
    // correctly and the lookup completes.
    let destination = bundle(SEED_BASE + 5);
    let silent = bundle(SEED_BASE + 6);
    let answering = bundle(SEED_BASE + 7);
    let silent_record = floodfill_at(&silent, BELOW_FLOOR_VERSION);
    let answering_record = floodfill_at(&answering, AT_FLOOR_VERSION);
    assert_below_the_floor(&silent_record);
    assert_at_the_floor(&answering_record);

    let mut store = RouterInfoStore::default();
    store.insert(silent_record.clone());
    store.insert(answering_record.clone());
    let silent_key = silent_record.key();
    let answering_key = answering_record.key();

    // The below-floor peer is the nearest candidate: the routing key is its
    // own key, so its XOR distance is zero.
    let routing_key = silent_key;

    let target = lookup_target_for(&destination);
    let lookup_id = LookupId::new(REQUEST_ID, LookupKind::LeaseSet2, target);
    let mut lookup = RouterInfoLookup::new(LookupPolicy::default());

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    assert_eq!(
        asked_peer(lookup.handle_pending_after_path(&store, &routing_key)),
        silent_key,
        "the nearest candidate is asked first, whatever its version"
    );

    // The peer does not answer. The rotation moves on.
    let outcome = lookup.advance_after_attempt_timeout(&store, &routing_key);
    assert_eq!(
        asked_peer(outcome),
        answering_key,
        "the silent candidate is skipped, not re-asked"
    );

    // The second peer answers correctly and the lookup completes.
    let mut lease_store = LeaseSet2Store::default();
    let message = store_message(&target, valid_ls2(&destination));
    let outcome = handle_database_store_lease_set2(
        &mut lookup,
        &mut lease_store,
        lookup_id,
        &message,
        LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("handle");
    let ResponseOutcome::Completed(result) = outcome else {
        panic!("the answer must reach the caller, got {outcome:?}");
    };
    assert!(
        matches!(*result, LookupResult::LeaseSet2Success { .. }),
        "the lookup completes once a candidate answers"
    );

    // The silent peer is not asked again: the rotation excludes what it has
    // already queried, so a below-floor peer costs one round trip.
    let diagnostics = lookup.diagnostics();
    assert!(
        diagnostics.is_none(),
        "a completed lookup holds no diagnostics"
    );
}

#[test]
fn a_peer_that_publishes_no_version_is_asked_for_a_lookup_too() {
    // The second unreachable class: a peer that publishes **no** version at
    // all.
    //
    // The tree refuses such a peer for a DSM and for a tunnel build, and it
    // still asks it for a lookup, because the lookup path is not gated and
    // the answer is validated on its own merits. A peer that publishes no
    // version is not thereby unable to answer.
    let destination = bundle(SEED_BASE + 8);
    let peer = bundle(SEED_BASE + 9);
    let peer_record = floodfill_without_version(&peer);
    assert_eq!(
        dsm_target_eligibility(published_version_of(&peer_record)),
        i2pr_proto::VersionEligibility::UnknownVersion,
        "the peer must publish no version at all"
    );

    let mut store = RouterInfoStore::default();
    store.insert(peer_record.clone());
    let peer_key = peer_record.key();

    let target = lookup_target_for(&destination);
    let routing_key = RouterHash::from_bytes([0x24; 32]);
    let lookup_id = LookupId::new(REQUEST_ID, LookupKind::LeaseSet2, target);
    let mut lookup = RouterInfoLookup::new(LookupPolicy::default());

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    assert_eq!(
        asked_peer(lookup.handle_pending_after_path(&store, &routing_key)),
        peer_key,
        "the candidate set is the nearest-floodfill set, unchanged by version"
    );
}

#[test]
fn a_gated_selection_would_have_refused_the_peer_the_lookup_asked() {
    // The inverted probe.
    //
    // The claim "the responder's version does not decide the lookup" would
    // be **vacuous** if a version-gated selection would have asked the same
    // peer anyway. It would not: the two selections genuinely differ, so
    // the decision is doing something and the rows above are not reading a
    // gate that is not there.
    let peer = bundle(SEED_BASE + 10);
    let peer_record = floodfill_at(&peer, BELOW_FLOOR_VERSION);
    assert_below_the_floor(&peer_record);

    let mut store = RouterInfoStore::default();
    store.insert(peer_record.clone());
    let peer_key = peer_record.key();

    let policy = LookupPolicy::default();
    let routing_key = RouterHash::from_bytes([0x25; 32]);
    let target = RouterHash::from_bytes([0x26; 32]);

    // The selection the lookup path uses offers the below-floor peer.
    let ungated = select_floodfill_candidates(&store, &target, &routing_key, &[], &policy);
    assert_eq!(
        ungated.hashes(),
        &[peer_key][..],
        "the lookup path's selection offers the below-floor peer"
    );

    // The selection the DSM paths use refuses it, and counts the refusal by
    // reason.
    let gated = select_version_gated_floodfills(&store, &routing_key, &[], &policy);
    assert!(
        gated.targets().is_empty(),
        "a gated selection would have refused the peer the lookup asked"
    );
    assert_eq!(gated.examined(), 1, "the gate examined the one candidate");
    assert_eq!(gated.accepted(), 0, "the gate accepted nothing");
    assert_eq!(
        gated.refusals().below_floor,
        1,
        "the refusal is counted by reason"
    );
}
