//! Plan 350: the controlled floodfill must store and then **serve** an encrypted LeaseSet2
//! (record type 5) so a stock reference client can resolve it.
//!
//! # The bug this pins
//!
//! Two independent omissions in `FloodfillStoreService` made the `i2pr → reference` direction
//! of Plan 347 impossible, and neither was observable from a test about another record type:
//!
//! 1. `handle` refused `record_type == 5` with `FloodfillStoreEffect::Unsupported` before the
//!    type-5 arm in `validate` was ever reached, so a reference-published type-5 record was a
//!    silent no-op. The hold-back predated ADR 0032; its rationale was the type-11 transcript
//!    disagreement, which Plan 346 closed.
//! 2. `lookup_body` probed `[0,1,3,7]` / `[1,3,7]` and never asked for type 5, so even once
//!    stored the record could never be handed back. `ServerNetDb::database_store_for_answer`
//!    already had a complete `5 =>` arm; nothing connected it to the lookup lists.
//!
//! A type-5 record is filed under its **blinded** storage key, not the destination hash, so the
//! lookup a reference client issues is a `lookup_type == 1` LeaseSet lookup *at that blinded
//! key*. That is the row that must work.
//!
//! These rows drive the public production surface (`FloodfillStoreService::handle` /
//! `handle_lookup` over a real `ServerNetDb`), not private helpers.
//!
//! The static half is `scripts/check-floodfill-type5-serve.sh`, which fails if a servable type
//! is ever declared but not probed, or if the type-5 store refusal returns.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{derive_public_key, generate_private};
use i2pr_proto::{
    CryptoKeyType, DatabaseLookupMessage, DatabaseStoreData, DatabaseStoreMessage, Date32,
    Destination, ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE, Els2SignedRegion, EncryptedLeaseSet2, Hash,
    Lease2, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
    MAX_COMMON_STRUCTURE_SIZE, ReplyEncryption, SignatureValue, SigningKeyType,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use i2pr_netdb::{
    BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig,
    FloodfillIngress, FloodfillLookupEffect, FloodfillRole, FloodfillStoreEffect,
    FloodfillStorePolicy, FloodfillStoreService, FloodfillTime, SERVABLE_LEASE_LOOKUP,
    SERVABLE_NORMAL_LOOKUP, SERVABLE_RECORD_TYPES, ServerInsertOutcome, ServerNetDb,
    ServerNetDbConfig, ValidatedEncryptedLeaseSet2,
};

const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;
const WALL_MS: u64 = 1_700_000_000_000;
const PUBLISHED: u32 = 1_700_000_000;

// --- fixtures -------------------------------------------------------------------------------

struct Encrypted {
    record: EncryptedLeaseSet2,
    storage_key: BlindedStorageKey,
    credentials: i2pr_netdb::Els2Credentials,
    inner: LeaseSet2,
}

fn inner_lease_set(seed: u64) -> LeaseSet2 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("router identity");
    let destination =
        Destination::new(bundle.identity().key_and_cert().clone()).expect("destination");
    let header = LeaseSet2Header::new(destination, PUBLISHED, 3_600, LeaseSet2Flags::from_raw(0))
        .expect("header");
    let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64])
        .expect("placeholder");
    let keys =
        vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
    let leases = vec![Lease2::new(
        Hash::from_bytes([0x11; 32]),
        7,
        Date32::from_seconds(PUBLISHED + 600),
    )];
    let unsigned = LeaseSet2::new(
        header,
        i2pr_proto::Mapping::empty(),
        keys,
        leases,
        placeholder,
    )
    .expect("unsigned");
    let signature = bundle
        .signing_key()
        .sign(&unsigned.signature_preimage())
        .expect("inner signature");
    LeaseSet2::new(
        unsigned.header().clone(),
        unsigned.options().clone(),
        unsigned.encryption_keys().to_vec(),
        unsigned.leases().to_vec(),
        signature,
    )
    .expect("inner")
}

/// Builds a type-5 record signed with the **deployed** Java/i2pd ELS2 transcript (ADR 0032),
/// which is what a stock reference router produces and what the floodfill must now accept.
fn deployed_record(seed: u64, secret: Option<&str>, offline: bool) -> Encrypted {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let unblinded = generate_private(&mut rng).expect("red25519 key");
    let unblinded_public = derive_public_key(&unblinded);
    let identity = BlindingIdentity::new(
        unblinded_public,
        SigningKeyType::RedDsaSha512Ed25519,
        secret,
    )
    .expect("identity");
    let schedule =
        BlindingSchedule::new_owner(identity, unblinded, BlindingScheduleConfig::default());
    let owner = schedule.owner_blinding(PUBLISHED).expect("owner");
    let blinded_public = *owner.daily().blinded_public_key();
    let credentials = i2pr_netdb::derive_els2_credentials(
        &unblinded_public,
        SigningKeyType::RedDsaSha512Ed25519,
        &blinded_public,
    );
    let storage_key =
        BlindedStorageKey::from_hash(i2pr_crypto::red25519::blinded_storage_key(&blinded_public));

    let inner = inner_lease_set(seed ^ 0x5a5a);
    let inner_bytes = inner.encode_to_vec(MAX).expect("inner bytes");
    let mut outer_salt = [0_u8; 32];
    let mut inner_salt = [0_u8; 32];
    rng.fill_bytes(&mut outer_salt);
    rng.fill_bytes(&mut inner_salt);
    let outer_ciphertext = i2pr_netdb::encrypt_no_auth_outer_ciphertext(
        &credentials,
        PUBLISHED,
        i2pr_proto::INNER_LEASE_SET2_STORE_TYPE,
        &inner_bytes,
        &outer_salt,
        &inner_salt,
    )
    .expect("outer encrypt");

    // With the offline-key flag set, the *delegation block* is signed with the blinded type-11
    // key and the *record* is signed with the transient key the block delegates to. Signing the
    // record with the blinded key instead is correctly refused by the validator, which is what
    // `a_type5_record_with_offline_keys_is_stored_and_served` proves by getting it right here.
    let transient = offline.then(|| RouterIdentityBundle::generate(&mut rng).expect("transient"));
    let offline_block = transient.as_ref().map(|transient| {
        let transient_public = transient
            .signing_key()
            .public_key()
            .expect("transient public key")
            .as_bytes()
            .to_vec();
        let probe = i2pr_proto::EncryptedLeaseSet2OfflineKeys::new(
            PUBLISHED + 3_600,
            SigningKeyType::EdDsaSha512Ed25519,
            transient_public.clone(),
            vec![0_u8; 64],
        )
        .expect("offline probe");
        let signature = i2pr_netdb::sign_type11_deployed(
            owner.blinded_private_key(),
            Els2SignedRegion::of_offline_keys(&probe),
            &mut rng,
        )
        .expect("delegation");
        i2pr_proto::EncryptedLeaseSet2OfflineKeys::new(
            PUBLISHED + 3_600,
            SigningKeyType::EdDsaSha512Ed25519,
            transient_public,
            signature.as_bytes().to_vec(),
        )
        .expect("offline block")
    });

    let build = |signature: Vec<u8>| {
        EncryptedLeaseSet2::new(
            ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
            blinded_public.as_bytes().to_vec(),
            PUBLISHED,
            3_600,
            offline_block.clone(),
            outer_ciphertext.clone(),
            signature,
        )
        .expect("type-5 record")
    };

    let probe = build(vec![0_u8; 64]);
    let signature_bytes = match transient.as_ref() {
        // The record is signed by the transient key the offline block delegates to.
        Some(transient) => transient
            .signing_key()
            .sign(Els2SignedRegion::of_record(&probe).as_bytes())
            .expect("transient record signature")
            .as_bytes()
            .to_vec(),
        // Otherwise the blinded type-11 key signs it, under the deployed ELS2 transcript.
        None => i2pr_netdb::sign_type11_deployed(
            owner.blinded_private_key(),
            Els2SignedRegion::of_record(&probe),
            &mut rng,
        )
        .expect("type-5 signature")
        .as_bytes()
        .to_vec(),
    };

    Encrypted {
        record: build(signature_bytes),
        storage_key,
        credentials,
        inner,
    }
}

fn store_message(record: &EncryptedLeaseSet2, key: Hash) -> DatabaseStoreMessage {
    DatabaseStoreMessage {
        key,
        reply_token: 1,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::EncryptedLeaseSet(Box::new(record.clone())),
    }
}

fn service() -> FloodfillStoreService {
    FloodfillStoreService::new(FloodfillStorePolicy::default())
}

fn db() -> ServerNetDb {
    ServerNetDb::with_config(ServerNetDbConfig::default())
}

fn time() -> FloodfillTime {
    FloodfillTime {
        wall_ms: WALL_MS,
        monotonic_ms: 1,
    }
}

/// A `lookup_type == 1` LeaseSet lookup at `key` — the shape a reference client issues when
/// resolving an encrypted service, because the record lives at its blinded storage key.
fn lease_lookup(key: Hash) -> DatabaseLookupMessage {
    DatabaseLookupMessage {
        key,
        from: Hash::from_bytes([0x5b; 32]),
        delivery_flag: false,
        reply_tunnel_id: None,
        lookup_type: 1,
        excluded_peers: Vec::new(),
        reply_encryption: ReplyEncryption::None,
    }
}

fn normal_lookup(key: Hash) -> DatabaseLookupMessage {
    DatabaseLookupMessage {
        lookup_type: 0,
        ..lease_lookup(key)
    }
}

fn exploration_lookup(key: Hash) -> DatabaseLookupMessage {
    DatabaseLookupMessage {
        lookup_type: 3,
        ..lease_lookup(key)
    }
}

fn store(
    db: &mut ServerNetDb,
    service: &mut FloodfillStoreService,
    message: &DatabaseStoreMessage,
) -> FloodfillStoreEffect {
    service.handle(
        db,
        message,
        FloodfillRole::Serving,
        FloodfillIngress::RouterTunnel,
        1,
        time(),
    )
}

/// Asserts a store was admitted and returns the `RecordId` it was filed under.
///
/// The record type and key are both checked here, so a row cannot pass by being stored under
/// the wrong index — the specific mistake that made type 5 unservable in the first place.
#[track_caller]
fn assert_stored(effect: &FloodfillStoreEffect, expected_key: Hash) -> u8 {
    let FloodfillStoreEffect::Stored {
        record, outcome, ..
    } = effect
    else {
        panic!("the floodfill must admit a deployed-transcript type-5 record, got {effect:?}");
    };
    assert_eq!(
        record.record_type(),
        5,
        "the record must be filed as record type 5, got type {}",
        record.record_type()
    );
    assert_eq!(
        *record.key(),
        expected_key,
        "the record must be filed at its own blinded storage key"
    );
    assert!(
        matches!(
            outcome,
            ServerInsertOutcome::Inserted | ServerInsertOutcome::Replaced
        ),
        "the record must actually land in the store, got {outcome:?}"
    );
    record.record_type()
}

/// Whether a lookup was answered with a `DatabaseStore` body. Every negative row asserts this
/// is false, so the check is written once rather than repeated as a long `matches!`.
fn served_a_record(effect: &FloodfillLookupEffect) -> bool {
    matches!(
        effect,
        FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
            body: i2pr_proto::I2npBody::DatabaseStore(_),
            ..
        })
    )
}

// --- positive rows -------------------------------------------------------------------------

/// The headline row: a deployed-transcript type-5 record is stored by the floodfill and then
/// returned byte-identically to a blinded-key LeaseSet lookup.
///
/// This is the exact store→lookup chain Plan 347's `i2pr → reference` directions need, and it
/// failed on both halves before Plan 350.
#[test]
fn a_type5_record_is_stored_and_served_to_a_blinded_key_lease_lookup() {
    let encrypted = deployed_record(1, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();

    assert_eq!(
        assert_stored(
            &store(
                &mut db,
                &mut service,
                &store_message(&encrypted.record, key)
            ),
            key
        ),
        5,
        "the floodfill must admit a deployed-transcript type-5 record; ADR 0032 made this \
         verifiable and Plan 350 removed the hold-back that refused it"
    );

    let effect = service.handle_lookup(
        &db,
        &lease_lookup(key),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        time(),
    );
    let FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
        body: i2pr_proto::I2npBody::DatabaseStore(message),
        ..
    }) = effect
    else {
        panic!(
            "a blinded-key LeaseSet lookup must be answered with the stored type-5 record, got {effect:?}"
        );
    };

    assert_eq!(
        message.key, key,
        "the answer must be keyed at the requested blinded key"
    );
    let DatabaseStoreData::EncryptedLeaseSet(served) = message.data else {
        panic!(
            "the answer must carry the type-5 record, got {:?}",
            message.data
        );
    };
    assert_eq!(
        served.encode_to_vec(MAX).expect("re-encode"),
        encrypted.record.encode_to_vec(MAX).expect("original"),
        "the served record must be byte-identical to the stored one: a re-encode that changed \
         the signature preimage would break the consumer's outer type-11 verification"
    );
}

/// The same for a record carrying the offline-key delegation block, since a real authorized
/// service sets that flag and it changes the record length and signature.
#[test]
fn a_type5_record_with_offline_keys_is_stored_and_served() {
    let encrypted = deployed_record(2, None, true);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();

    assert_eq!(
        assert_stored(
            &store(
                &mut db,
                &mut service,
                &store_message(&encrypted.record, key)
            ),
            key
        ),
        5,
    );
    let FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
        body: i2pr_proto::I2npBody::DatabaseStore(message),
        ..
    }) = service.handle_lookup(
        &db,
        &lease_lookup(key),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        time(),
    )
    else {
        panic!("an offline-key type-5 record must be served");
    };
    assert!(
        matches!(message.data, DatabaseStoreData::EncryptedLeaseSet(_)),
        "an offline-key type-5 record must still be served as record type 5"
    );
}

/// A normal (`lookup_type == 0`) lookup at the blinded key also finds it, because a client may
/// address the blinded key directly rather than issuing a typed LeaseSet lookup.
#[test]
fn a_normal_lookup_at_the_blinded_key_also_serves_the_type5_record() {
    let encrypted = deployed_record(3, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();
    assert_eq!(
        assert_stored(
            &store(
                &mut db,
                &mut service,
                &store_message(&encrypted.record, key)
            ),
            key
        ),
        5,
    );
    let effect = service.handle_lookup(
        &db,
        &normal_lookup(key),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        time(),
    );
    assert!(
        matches!(
            effect,
            FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
                body: i2pr_proto::I2npBody::DatabaseStore(_),
                ..
            })
        ),
        "a normal lookup at a blinded key must serve the type-5 record, got {effect:?}"
    );
}

/// A lookup-secret-protected record round-trips the same way; the secret affects the blinded
/// key, not the transcript or the serve path.
#[test]
fn a_lookup_secret_type5_record_is_stored_and_served() {
    let encrypted = deployed_record(4, Some("a shared lookup secret"), false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();
    assert_eq!(
        assert_stored(
            &store(
                &mut db,
                &mut service,
                &store_message(&encrypted.record, key)
            ),
            key
        ),
        5,
    );
    assert!(matches!(
        service.handle_lookup(
            &db,
            &lease_lookup(key),
            FloodfillRole::Serving,
            Hash::from_bytes([0x99; 32]),
            time(),
        ),
        FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
            body: i2pr_proto::I2npBody::DatabaseStore(_),
            ..
        })
    ));
}

/// The served record still re-validates through the bounded type-5 validator and still reports
/// the transcript profile it was stored under, so ADR 0032's classification survives the
/// store→lookup round trip.
#[test]
fn the_served_record_revalidates_and_keeps_its_transcript_profile() {
    let encrypted = deployed_record(5, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();
    assert_eq!(
        assert_stored(
            &store(
                &mut db,
                &mut service,
                &store_message(&encrypted.record, key)
            ),
            key
        ),
        5,
    );
    let FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
        body: i2pr_proto::I2npBody::DatabaseStore(message),
        ..
    }) = service.handle_lookup(
        &db,
        &lease_lookup(key),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        time(),
    )
    else {
        panic!("record must be served");
    };
    let DatabaseStoreData::EncryptedLeaseSet(served) = message.data else {
        panic!("must be type 5");
    };
    let revalidated = ValidatedEncryptedLeaseSet2::validate(
        *served,
        Some(encrypted.storage_key),
        i2pr_netdb::Els2ValidationContext::new(PUBLISHED + 1),
    )
    .expect("the served record must still validate");
    assert_eq!(
        revalidated.signature_profile().as_str(),
        "deployed",
        "a reference-published record must keep the deployed profile through the round trip"
    );
    assert_eq!(
        revalidated.storage_key(),
        encrypted.storage_key,
        "the storage key gate must still hold after the round trip"
    );
    // The record a consumer receives must decrypt back to the inner LeaseSet2 that was
    // published, using the publisher's own blinded material. This is the end of the chain
    // Plan 347's `i2pr -> reference` row depends on, exercised here with the local
    // reimplementation as the consumer.
    let decrypted = i2pr_netdb::decrypt_no_auth_outer_ciphertext(
        &encrypted.credentials,
        PUBLISHED,
        revalidated.record().outer_ciphertext(),
    )
    .expect("the served record must decrypt with the publisher's credentials");
    assert_eq!(
        decrypted.store_type(),
        i2pr_proto::INNER_LEASE_SET2_STORE_TYPE,
        "the decrypted inner record must be a LeaseSet2"
    );
    let inner = decrypted
        .into_lease_set2(MAX)
        .expect("the decrypted payload must parse as an inner LeaseSet2");
    assert_eq!(
        inner.encode_to_vec(MAX).expect("inner encode"),
        encrypted.inner.encode_to_vec(MAX).expect("inner encode"),
        "the served record must decrypt back to exactly the inner LeaseSet2 that was published"
    );
}

// --- negative rows -------------------------------------------------------------------------

/// A tampered type-5 record is refused at the store, not served later.
#[test]
fn a_tampered_type5_record_is_refused_at_the_store() {
    let encrypted = deployed_record(6, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut tampered = encrypted.record.clone();
    let mut ciphertext = tampered.outer_ciphertext().to_vec();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xff;
    tampered = EncryptedLeaseSet2::new(
        tampered.blinded_sigtype(),
        tampered.blinded_public_key().to_vec(),
        tampered.published_seconds(),
        tampered.expires_offset_seconds(),
        tampered.offline_keys().cloned(),
        ciphertext,
        tampered.signature().to_vec(),
    )
    .expect("tampered record");

    let mut db = db();
    let mut service = service();
    assert_eq!(
        store(&mut db, &mut service, &store_message(&tampered, key)),
        FloodfillStoreEffect::Invalid,
        "a tampered outer ciphertext must be refused by the outer type-11 signature"
    );
    assert!(
        !served_a_record(&service.handle_lookup(
            &db,
            &lease_lookup(key),
            FloodfillRole::Serving,
            Hash::from_bytes([0x99; 32]),
            time(),
        )),
        "nothing may be served for a record the store refused"
    );
}

/// An expired type-5 record is refused, and never served.
#[test]
fn an_expired_type5_record_is_refused_and_never_served() {
    let encrypted = deployed_record(7, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();
    assert_eq!(
        assert_stored(
            &store(
                &mut db,
                &mut service,
                &store_message(&encrypted.record, key)
            ),
            key
        ),
        5
    );
    // Past the record's expiry window.
    let later = FloodfillTime {
        wall_ms: WALL_MS + 10_000_000,
        monotonic_ms: 2,
    };
    let effect = service.handle_lookup(
        &db,
        &lease_lookup(key),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        later,
    );
    assert!(
        !served_a_record(&effect),
        "an expired type-5 record must not be served, got {effect:?}"
    );
}

/// A record stored under a key that is not its own blinded key is not served under the
/// requested key. This is the storage-key gate, and it is what stops a publisher from parking a
/// record under a key it does not own.
#[test]
fn a_record_is_not_served_under_a_key_it_does_not_own() {
    let encrypted = deployed_record(8, None, false);
    let mut db = db();
    let mut service = service();
    let wrong_key = Hash::from_bytes([0x5c; 32]);
    assert_eq!(
        store(
            &mut db,
            &mut service,
            &store_message(&encrypted.record, wrong_key)
        ),
        FloodfillStoreEffect::Invalid,
        "a type-5 record must be refused when the store key is not its own blinded storage key"
    );
    let effect = service.handle_lookup(
        &db,
        &lease_lookup(wrong_key),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        time(),
    );
    assert!(
        !matches!(
            effect,
            FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
                body: i2pr_proto::I2npBody::DatabaseStore(_),
                ..
            })
        ),
        "nothing may be served at a key the store refused"
    );
}

/// A LeaseSet lookup at a blinded key with no stored record is a clean miss, not an error a
/// consumer could mistake for a store failure.
#[test]
fn a_lookup_at_an_unstored_blinded_key_is_a_clean_miss() {
    let db = db();
    let mut service = service();
    let effect = service.handle_lookup(
        &db,
        &lease_lookup(Hash::from_bytes([0x7e; 32])),
        FloodfillRole::Serving,
        Hash::from_bytes([0x99; 32]),
        time(),
    );
    assert!(
        !matches!(
            effect,
            FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
                body: i2pr_proto::I2npBody::DatabaseStore(_),
                ..
            })
        ),
        "an absent record must not be answered with a store body, got {effect:?}"
    );
}

/// A client-tunnel ingress is still refused for type 5, as for every other record type: a
/// client cannot push records straight into the floodfill.
#[test]
fn a_client_tunnel_type5_store_is_still_refused() {
    let encrypted = deployed_record(9, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();
    let effect = service.handle(
        &mut db,
        &store_message(&encrypted.record, key),
        FloodfillRole::Serving,
        FloodfillIngress::ClientTunnel,
        1,
        time(),
    );
    assert_eq!(
        effect,
        FloodfillStoreEffect::Invalid,
        "admitting type 5 must not weaken the client-tunnel ingress rule"
    );
}

/// A non-serving floodfill role is still refused for type 5, as for every record type.
#[test]
fn a_non_serving_role_cannot_store_a_type5_record() {
    let encrypted = deployed_record(10, None, false);
    let key = *encrypted.storage_key.as_hash();
    let mut db = db();
    let mut service = service();
    assert_eq!(
        service.handle(
            &mut db,
            &store_message(&encrypted.record, key),
            FloodfillRole::Disabled,
            FloodfillIngress::RouterTunnel,
            1,
            time(),
        ),
        FloodfillStoreEffect::Disabled
    );
}

// --- unchanged-answer rows -----------------------------------------------------------------

/// RouterInfo and exploration answers must be unaffected by the type-5 additions.
#[test]
fn routerinfo_and_exploration_answers_are_unchanged() {
    let db = db();
    let mut service = service();
    let local = Hash::from_bytes([0x11; 32]);

    // lookup_type 3 is exploration: it must never return a store body, least of all a type-5 one.
    let exploration = service.handle_lookup(
        &db,
        &exploration_lookup(Hash::from_bytes([0x22; 32])),
        FloodfillRole::Serving,
        local,
        time(),
    );
    assert!(
        matches!(
            &exploration,
            FloodfillLookupEffect::Reply(i2pr_netdb::FloodfillReplyIntent::Direct {
                body: i2pr_proto::I2npBody::DatabaseSearchReply(_),
                ..
            })
        ),
        "exploration must still answer with a search reply, got {exploration:?}"
    );

    // lookup_type 2 is the RouterInfo form: it still probes type 0 only.
    let router_info_lookup = service.handle_lookup(
        &db,
        &DatabaseLookupMessage {
            lookup_type: 2,
            ..lease_lookup(Hash::from_bytes([0x33; 32]))
        },
        FloodfillRole::Serving,
        local,
        time(),
    );
    assert!(
        !served_a_record(&router_info_lookup),
        "a RouterInfo lookup must not be answered with a store body, got {router_info_lookup:?}"
    );
}

// --- the declared coverage -----------------------------------------------------------------

/// The exported servable-type tables must stay mutually consistent, so the static guard's
/// premise is also true at runtime and cannot drift from the lists the service uses.
#[test]
fn the_servable_type_tables_agree_with_each_other() {
    assert_eq!(
        SERVABLE_RECORD_TYPES,
        &[0, 1, 3, 5, 7],
        "SERVABLE_RECORD_TYPES is the authoritative set of storeable record types"
    );
    assert_eq!(SERVABLE_NORMAL_LOOKUP, &[0, 1, 3, 5, 7]);
    assert_eq!(SERVABLE_LEASE_LOOKUP, &[1, 3, 5, 7]);
    for required in SERVABLE_RECORD_TYPES {
        assert!(
            SERVABLE_NORMAL_LOOKUP.contains(required)
                || SERVABLE_LEASE_LOOKUP.contains(required)
                || *required == 0,
            "record type {required} is storeable but no lookup list probes it, so it would be \
             stored and never served"
        );
    }
    assert!(
        SERVABLE_LEASE_LOOKUP.contains(&5),
        "type 5 must be probed by the LeaseSet lookup, which is what a reference client issues \
         for a blinded storage key"
    );
    assert!(
        !SERVABLE_LEASE_LOOKUP.contains(&0),
        "the LeaseSet lookup must not start answering RouterInfo lookups"
    );
}
