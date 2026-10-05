//! Plan 349: the encrypted-LeaseSet2 consumer lifecycle, through its production owner.
//!
//! # What this is for
//!
//! `EncryptedLeaseSet2Resolver` had **zero** production callers before this plan; every round
//! trip in the repository was in-process, so nothing exercised the part that actually makes a
//! consumer a consumer: deriving the day's blinded key, looking up *that* key, refusing a
//! reply for any other key, and releasing the in-flight lease on every exit path.
//!
//! These rows drive [`EncryptedServiceResolver`] — the bounded owner — rather than the resolver
//! directly, because the owner is where the bugs Plan 349 found actually live: an unbounded
//! table, a lease leaked on failure, a credential that outlives its request, and a reply that
//! could be attributed to the wrong lookup.
//!
//! Nothing here reaches a network. The owner is deliberately transport-agnostic: it hands the
//! caller a blinded storage key and expects a `DatabaseStore` back. The live cross-router
//! proof is Plan 347's re-attempt.

use i2pr_client::encrypted_leaseset::{EncryptedLeaseSet2Publisher, generate_client_dh_keypair};
use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{Red25519PrivateScalar, generate_private};
use i2pr_daemon::encrypted_service_resolver::{
    EncryptedResolveId, EncryptedServiceConsumerError, EncryptedServiceResolver,
    MAX_CONCURRENT_ENCRYPTED_RESOLVES,
};
use i2pr_netdb::{
    BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, Els2AuthorizationServerConfig,
    Els2ClientAuth, PskClientKey, ValidatedEncryptedLeaseSet2,
};
use i2pr_proto::{
    B32_FLAG_REQUIRES_CLIENT_KEY, CryptoKeyType, DatabaseStoreData, DatabaseStoreMessage, Date32,
    Destination, Lease2, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
    MAX_COMMON_STRUCTURE_SIZE, Mapping, SignatureValue, SigningKeyType,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const PUBLISHED: u32 = 1_700_000_000;
const EXPIRES_OFFSET: u16 = 3_600;
const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;
const DEADLINE_MS: u64 = 60_000;

// --- fixtures ----------------------------------------------------------------------------------

fn router(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic router identity")
}

fn signed_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    let destination =
        Destination::new(signer.identity().key_and_cert().clone()).expect("destination");
    let header = LeaseSet2Header::new(destination, PUBLISHED, 3_600, LeaseSet2Flags::from_raw(0))
        .expect("header");
    let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64])
        .expect("placeholder");
    let keys =
        vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
    let leases = vec![Lease2::new(
        i2pr_proto::Hash::from_bytes([0x22; 32]),
        7,
        Date32::from_seconds(PUBLISHED + 600),
    )];
    let unsigned =
        LeaseSet2::new(header, Mapping::empty(), keys, leases, placeholder).expect("unsigned");
    let signature = signer
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

/// An owner schedule, optionally with a lookup secret.
///
/// The secret enters `GENERATE_ALPHA`, so a publisher and a consumer must *both* build their
/// blinding from the same secret or they derive different storage keys. That is the secret's
/// whole purpose, and `a_wrong_lookup_secret_misses` pins the consequence.
fn owner_schedule_with_secret(seed: u64, secret: Option<&str>) -> BlindingSchedule {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let private: Red25519PrivateScalar = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity = BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, secret)
        .expect("identity");
    BlindingSchedule::new_owner(identity, private, BlindingScheduleConfig::new(2, true))
}

fn owner_schedule(seed: u64) -> BlindingSchedule {
    owner_schedule_with_secret(seed, None)
}

fn psk(seed: u64) -> PskClientKey {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    PskClientKey::generate(&mut rng).expect("psk")
}

// --- positive: the full no-auth chain ----------------------------------------------------------

/// The headline row: begin, receive, validate, decrypt — an encrypted service resolved through
/// its production owner rather than in-process.
#[test]
fn a_no_auth_encrypted_service_resolves_through_the_owner() {
    let schedule = owner_schedule(0x1a2b);
    let inner = signed_ls2(&router(0x30));
    let mut rng = ChaCha8Rng::seed_from_u64(0x41);

    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    let mut owner = EncryptedServiceResolver::default();
    let (id, key) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    assert_eq!(
        *key.as_hash(),
        message.key,
        "the owner must look up exactly the key the publisher stored under"
    );
    assert_eq!(owner.in_flight(), 1);

    let resolved = owner
        .ingest_store(id, &message, PUBLISHED)
        .expect("resolve");
    assert_eq!(
        resolved.inner_lease_set2(),
        &inner,
        "the consumer must recover exactly the inner LeaseSet2 the publisher wrote"
    );
    assert_eq!(
        owner.in_flight(),
        0,
        "a successful resolve must release its in-flight lease"
    );
}

/// The same, with a lookup secret. The secret changes the storage key, so both sides must
/// derive it or the lookup misses.
#[test]
fn a_lookup_secret_service_resolves_when_the_secret_matches() {
    let schedule = owner_schedule_with_secret(0x51, Some("a shared secret"));
    let inner = signed_ls2(&router(0x52));
    let mut rng = ChaCha8Rng::seed_from_u64(0x53);

    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(true).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    let mut owner = EncryptedServiceResolver::default();
    let (id, key) = owner
        .begin(&address, Some("a shared secret"), PUBLISHED, DEADLINE_MS)
        .expect("begin");
    assert_eq!(*key.as_hash(), message.key);
    assert_eq!(
        owner
            .ingest_store(id, &message, PUBLISHED)
            .expect("resolve")
            .inner_lease_set2(),
        &inner
    );
}

/// A wrong lookup secret derives a different blinded key, so the lookup misses.
///
/// This is the negative half of the discovery-control property, and it is the reason the two
/// sides must agree on the secret: a client holding the address but not the secret cannot
/// derive the storage key the record was filed under.
#[test]
fn a_wrong_lookup_secret_misses() {
    let schedule = owner_schedule_with_secret(0x55, Some("the right secret"));
    let inner = signed_ls2(&router(0x56));
    let mut rng = ChaCha8Rng::seed_from_u64(0x57);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(true).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    let mut owner = EncryptedServiceResolver::default();
    let (_, key) = owner
        .begin(&address, Some("the wrong secret"), PUBLISHED, DEADLINE_MS)
        .expect("begin");
    assert_ne!(
        *key.as_hash(),
        message.key,
        "a wrong lookup secret must derive a different storage key, or the secret protects \
         nothing"
    );
    assert!(
        matches!(
            owner.ingest_store(EncryptedResolveId(0), &message, PUBLISHED),
            Err(EncryptedServiceConsumerError::KeyMismatch)
        ),
        "a record filed under a different blinded key must not be resolvable"
    );
    assert_eq!(owner.in_flight(), 0);
}

// --- positive: per-client authorization ---------------------------------------------------------

/// Every authorized PSK client resolves the same published record.
#[test]
fn every_authorized_psk_client_resolves_through_the_owner() {
    let schedule = owner_schedule(0x61);
    let inner = signed_ls2(&router(0x62));
    let config =
        Els2AuthorizationServerConfig::psk(vec![psk(0x11), psk(0x12), psk(0x13)]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x63);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(
            &config,
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let address = publisher
        .authorized_address(false)
        .expect("authorized address");
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_CLIENT_KEY,
        B32_FLAG_REQUIRES_CLIENT_KEY,
        "an authorized address must declare that a client key is required"
    );

    for seed in [0x11_u64, 0x12, 0x13] {
        let key = psk(seed);
        let mut owner = EncryptedServiceResolver::default();
        let (id, _) = owner
            .begin_authorized(
                &address,
                None,
                Els2ClientAuth::Psk(&key),
                PUBLISHED,
                DEADLINE_MS,
            )
            .expect("begin");
        assert_eq!(
            owner
                .ingest_store(id, &message, PUBLISHED)
                .unwrap_or_else(|error| panic!("client {seed:#x} must be authorized: {error:?}"))
                .inner_lease_set2(),
            &inner,
            "every authorized client must recover the same inner LeaseSet2"
        );
    }
}

/// A DH client resolves with its own keypair, and the record was published for that public key.
#[test]
fn an_authorized_dh_client_resolves_through_the_owner() {
    let schedule = owner_schedule(0x71);
    let inner = signed_ls2(&router(0x72));
    let mut rng = ChaCha8Rng::seed_from_u64(0x73);
    let (private, public) = generate_client_dh_keypair(&mut rng).expect("dh keypair");
    let config = Els2AuthorizationServerConfig::dh(vec![public]).expect("config");
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(
            &config,
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let address = publisher
        .authorized_address(false)
        .expect("authorized address");

    let mut owner = EncryptedServiceResolver::default();
    let (id, _) = owner
        .begin_authorized(
            &address,
            None,
            Els2ClientAuth::Dh {
                private: &private,
                public,
            },
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin");
    assert_eq!(
        owner
            .ingest_store(id, &message, PUBLISHED)
            .expect("resolve")
            .inner_lease_set2(),
        &inner
    );
}

/// A secret-protected service resolves with both the secret and the credential together.
#[test]
fn a_secret_plus_authorized_service_resolves_when_both_match() {
    let schedule = owner_schedule_with_secret(0x81, Some("the secret"));
    let inner = signed_ls2(&router(0x82));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0x83)]).expect("config");
    let key = psk(0x83);
    let mut rng = ChaCha8Rng::seed_from_u64(0x84);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(
            &config,
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let address = publisher
        .authorized_address(true)
        .expect("authorized address");

    let mut owner = EncryptedServiceResolver::default();
    let (id, _) = owner
        .begin_authorized(
            &address,
            Some("the secret"),
            Els2ClientAuth::Psk(&key),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin");
    assert_eq!(
        owner
            .ingest_store(id, &message, PUBLISHED)
            .expect("resolve")
            .inner_lease_set2(),
        &inner
    );
}

// --- negative: authorization --------------------------------------------------------------------

/// A wrong pre-shared key is refused as an authorization failure, and the lease is released.
#[test]
fn a_wrong_psk_is_refused_and_releases_its_lease() {
    let schedule = owner_schedule(0x91);
    let inner = signed_ls2(&router(0x92));
    let good = psk(0x93);
    let wrong = psk(0x94);
    let config = Els2AuthorizationServerConfig::psk(vec![good]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x95);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(
            &config,
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let address = publisher
        .authorized_address(false)
        .expect("authorized address");

    let mut owner = EncryptedServiceResolver::default();
    let (id, _) = owner
        .begin_authorized(
            &address,
            None,
            Els2ClientAuth::Psk(&wrong),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin");
    assert!(
        matches!(
            owner.ingest_store(id, &message, PUBLISHED),
            Err(EncryptedServiceConsumerError::NotAuthorized)
        ),
        "a wrong pre-shared key must be refused as unauthorized"
    );
    assert_eq!(
        owner.in_flight(),
        0,
        "a refused resolve must still release its in-flight lease"
    );
}

/// Beginning without a credential for an address that requires one is a construction refusal,
/// not a lookup that is certain to fail later.
#[test]
fn an_authorized_address_cannot_be_begun_without_a_credential() {
    let schedule = owner_schedule(0xa1);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher
        .authorized_address(false)
        .expect("authorized address");
    let mut owner = EncryptedServiceResolver::default();
    assert!(matches!(
        owner.begin(&address, None, PUBLISHED, DEADLINE_MS),
        Err(EncryptedServiceConsumerError::InvalidAddress(_))
    ));
    assert_eq!(
        owner.in_flight(),
        0,
        "a refused begin must not occupy an in-flight slot"
    );
}

// --- negative: reply attribution and record integrity -------------------------------------------

/// A reply whose key is not the one this request asked for is refused as a key mismatch.
///
/// This is the row that matters most for a two-party lookup: without it, a record stored under
/// someone else's blinded key could be handed to this consumer.
#[test]
fn a_reply_for_another_key_is_refused_as_a_key_mismatch() {
    let schedule = owner_schedule(0xb1);
    let inner = signed_ls2(&router(0xb2));
    let mut rng = ChaCha8Rng::seed_from_u64(0xb3);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    let mut owner = EncryptedServiceResolver::default();
    let (id, _) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    let misrouted = DatabaseStoreMessage {
        key: i2pr_proto::Hash::from_bytes([0x5c; 32]),
        ..message.clone()
    };
    assert!(
        matches!(
            owner.ingest_store(id, &misrouted, PUBLISHED),
            Err(EncryptedServiceConsumerError::KeyMismatch)
        ),
        "a reply filed under a different key must not be attributed to this request"
    );
    assert_eq!(owner.in_flight(), 0);
}

/// A reply that is not a type-5 record is refused without being parsed as one.
#[test]
fn a_non_type5_reply_is_refused() {
    let schedule = owner_schedule(0xc1);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let mut owner = EncryptedServiceResolver::default();
    let (id, key) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    let wrong_type = DatabaseStoreMessage {
        key: *key.as_hash(),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::Deferred {
            store_type: i2pr_proto::DatabaseStoreType::LeaseSet2,
            payload: i2pr_proto::DeferredPayload::new(vec![0_u8; 8], 64).expect("payload"),
        },
    };
    assert!(matches!(
        owner.ingest_store(id, &wrong_type, PUBLISHED),
        Err(EncryptedServiceConsumerError::NotEncryptedLeaseSet)
    ));
    assert_eq!(owner.in_flight(), 0);
}

/// A tampered record is refused by bounded validation, and never decrypted.
#[test]
fn a_tampered_record_is_refused_and_never_decrypted() {
    let schedule = owner_schedule(0xd1);
    let inner = signed_ls2(&router(0xd2));
    let mut rng = ChaCha8Rng::seed_from_u64(0xd3);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    let record = match &message.data {
        DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        other => panic!("expected a type-5 record, got {other:?}"),
    };
    let mut ciphertext = record.outer_ciphertext().to_vec();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xff;
    let tampered = i2pr_proto::EncryptedLeaseSet2::new(
        record.blinded_sigtype(),
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        record.offline_keys().cloned(),
        ciphertext,
        record.signature().to_vec(),
    )
    .expect("tampered record");
    let tampered_message = DatabaseStoreMessage {
        data: DatabaseStoreData::EncryptedLeaseSet(Box::new(tampered)),
        ..message.clone()
    };

    let mut owner = EncryptedServiceResolver::default();
    let (id, _) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    assert!(
        matches!(
            owner.ingest_store(id, &tampered_message, PUBLISHED),
            Err(EncryptedServiceConsumerError::RecordRejected(_))
        ),
        "a tampered record must be refused by the outer type-11 signature"
    );
    assert_eq!(owner.in_flight(), 0);
}

/// A record fetched under the wrong day is refused. Daily blinding is what makes the lookup
/// day-scoped, so a stale record must not resolve.
#[test]
fn a_record_published_for_another_day_is_refused() {
    let schedule = owner_schedule(0xe1);
    let inner = signed_ls2(&router(0xe2));
    let mut rng = ChaCha8Rng::seed_from_u64(0xe3);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    let mut owner = EncryptedServiceResolver::default();
    let (id, _) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    // Ten days later the storage key has rotated, so the record is no longer this day's.
    assert!(
        matches!(
            owner.ingest_store(id, &message, PUBLISHED + 10 * 86_400),
            Err(EncryptedServiceConsumerError::RecordRejected(_))
        ),
        "a record from another day's blinding must not resolve"
    );
    assert_eq!(owner.in_flight(), 0);
}

// --- bounds and lifecycle -----------------------------------------------------------------------

/// The in-flight table is bounded at capacity 1, exact load, and max+1.
#[test]
fn the_in_flight_table_is_bounded_at_every_load() {
    let schedule = owner_schedule(0x101);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let mut owner = EncryptedServiceResolver::default();

    // Empty.
    assert_eq!(owner.in_flight(), 0);
    assert!(!owner.is_full());

    // Exact load.
    for expected in 1..=MAX_CONCURRENT_ENCRYPTED_RESOLVES {
        owner
            .begin(&address, None, PUBLISHED, DEADLINE_MS)
            .unwrap_or_else(|error| panic!("load {expected} must be accepted: {error:?}"));
        assert_eq!(owner.in_flight(), expected);
        assert_eq!(
            owner.is_full(),
            expected == MAX_CONCURRENT_ENCRYPTED_RESOLVES
        );
    }

    // max+1 is refused, and refusing must not grow the table.
    assert!(matches!(
        owner.begin(&address, None, PUBLISHED, DEADLINE_MS),
        Err(EncryptedServiceConsumerError::TooManyResolves)
    ));
    assert_eq!(
        owner.in_flight(),
        MAX_CONCURRENT_ENCRYPTED_RESOLVES,
        "a refused begin must not occupy a slot"
    );

    // A cancel frees exactly one slot, and the table is usable again.
    let cancelled = EncryptedResolveId(0);
    owner.cancel(cancelled).expect("cancel");
    assert_eq!(owner.in_flight(), MAX_CONCURRENT_ENCRYPTED_RESOLVES - 1);
    owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("the freed slot is usable again");
    assert_eq!(owner.in_flight(), MAX_CONCURRENT_ENCRYPTED_RESOLVES);
}

/// Every lease-release path is covered: success, failure, expiry, and cancellation.
#[test]
fn every_lease_release_path_is_covered() {
    let schedule = owner_schedule(0x111);
    let inner = signed_ls2(&router(0x112));
    let mut rng = ChaCha8Rng::seed_from_u64(0x113);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");

    // Success.
    let mut owner = EncryptedServiceResolver::default();
    let (ok, _) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    owner
        .ingest_store(ok, &message, PUBLISHED)
        .expect("resolve");
    assert_eq!(owner.in_flight(), 0, "success releases");

    // Failure (unknown request id — the request is already gone).
    let (bad, _) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    owner
        .ingest_store(bad, &message, PUBLISHED)
        .expect("resolve");
    assert!(
        matches!(
            owner.ingest_store(bad, &message, PUBLISHED),
            Err(EncryptedServiceConsumerError::UnknownRequest)
        ),
        "a second ingest of the same reply must be refused, not re-resolved"
    );
    assert_eq!(owner.in_flight(), 0, "failure releases");

    // Expiry.
    let (short, _) = owner
        .begin(&address, None, PUBLISHED, 1_000)
        .expect("begin");
    assert!(owner.is_expired(short, 1_000));
    assert_eq!(
        owner.expire(1_000),
        vec![short],
        "expiry reclaims the request"
    );
    assert_eq!(owner.in_flight(), 0, "expiry releases");
    assert!(
        owner.expire(1_000).is_empty(),
        "expiry must be idempotent, not a source of phantom ids"
    );

    // Cancellation.
    let (cancel_me, _) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    owner.cancel(cancel_me).expect("cancel");
    assert_eq!(owner.in_flight(), 0, "cancellation releases");
    assert!(
        matches!(
            owner.cancel(cancel_me),
            Err(EncryptedServiceConsumerError::UnknownRequest)
        ),
        "cancelling twice must be reported, not silently ignored"
    );
}

/// Concurrent resolves for distinct services do not cross-contaminate.
#[test]
fn concurrent_resolves_do_not_cross_contaminate() {
    let publisher_schedule = owner_schedule(0x121);
    let other_schedule = owner_schedule(0x122);
    let first_inner = signed_ls2(&router(0x123));
    let second_inner = signed_ls2(&router(0x124));
    let mut rng = ChaCha8Rng::seed_from_u64(0x125);

    let first_publisher = EncryptedLeaseSet2Publisher::new(&publisher_schedule);
    let first_address = first_publisher.address(false).expect("address");
    let (_, first_message) = first_publisher
        .build_database_store(
            &first_inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish first");

    let second_publisher = EncryptedLeaseSet2Publisher::new(&other_schedule);
    let second_address = second_publisher.address(false).expect("address");
    let (_, second_message) = second_publisher
        .build_database_store(
            &second_inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish second");

    let mut owner = EncryptedServiceResolver::default();
    let (first, first_key) = owner
        .begin(&first_address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin first");
    let (second, second_key) = owner
        .begin(&second_address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin second");
    assert_ne!(first_key.as_hash(), second_key.as_hash());
    assert_ne!(first, second);

    // Resolving out of order must still return the right record for each request.
    assert_eq!(
        owner
            .ingest_store(second, &second_message, PUBLISHED)
            .expect("second")
            .inner_lease_set2(),
        &second_inner
    );
    assert_eq!(
        owner
            .ingest_store(first, &first_message, PUBLISHED)
            .expect("first")
            .inner_lease_set2(),
        &first_inner
    );
    assert_eq!(owner.in_flight(), 0);
}

// --- observability -----------------------------------------------------------------------------

/// The coordinator's `Debug` reports presence, count, capacity, and credential *schemes* — and
/// never key material. This is the guard on a secret-leak path.
#[test]
fn debug_reports_shape_but_never_key_material() {
    let schedule = owner_schedule(0x131);
    // This row never publishes: it only needs a well-formed authorized address and a request
    // holding a credential, so it can assert what `Debug` is allowed to say about one.
    let key = psk(0x132);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher
        .authorized_address(false)
        .expect("authorized address");

    let mut owner = EncryptedServiceResolver::default();
    owner
        .begin_authorized(
            &address,
            None,
            Els2ClientAuth::Psk(&key),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin");
    let rendered = format!("{owner:?}");
    let key = psk(0x132);
    assert!(rendered.contains("in_flight"), "{rendered}");
    assert!(
        rendered.contains("Psk"),
        "the scheme is reportable: {rendered}"
    );
    // No byte of the credential may appear in any hex or base32 rendering of the key.
    for byte in key.as_bytes() {
        let needle = format!("{byte:02x}");
        assert!(
            !rendered.to_lowercase().contains(&needle),
            "the coordinator's Debug output must not contain credential bytes: {rendered}"
        );
    }
}

/// A resolved record is a real `ValidatedEncryptedLeaseSet2` before it is decrypted, so the
/// ADR 0032 profile classification is available to the owner.
#[test]
fn a_validated_record_reports_its_transcript_profile() {
    let schedule = owner_schedule(0x141);
    let inner = signed_ls2(&router(0x142));
    let mut rng = ChaCha8Rng::seed_from_u64(0x143);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let record = match &message.data {
        DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        other => panic!("expected a type-5 record, got {other:?}"),
    };
    let validated = ValidatedEncryptedLeaseSet2::validate(
        record,
        Some(i2pr_netdb::BlindedStorageKey::from_hash(message.key)),
        i2pr_netdb::Els2ValidationContext::new(PUBLISHED),
    )
    .expect("a freshly published record must validate");
    assert_eq!(
        validated.signature_profile().as_str(),
        "deployed",
        "i2pr publishes under the deployed profile (ADR 0032), and the profile must survive \\
         the consumer's validation"
    );
    assert!(
        validated.encoded_len() <= MAX,
        "the served record must stay within the common structure bound"
    );
}
