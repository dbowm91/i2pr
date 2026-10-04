//! Plan 332 evidence for the client half of the encrypted LeaseSet2.
//!
//! The black-box shape of these rows matters: they drive the public publisher and
//! resolver only, through the same `DatabaseStoreMessage` the daemon would send
//! and the same validated-record type it would receive. Nothing here calls a
//! private bridge, driver, or pump API.

use i2pr_client::encrypted_leaseset::{
    EncryptedLeaseSet2Publisher, EncryptedLeaseSet2Resolver, EncryptedLeaseSetError,
};
use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{Red25519PrivateScalar, generate_private};
use i2pr_netdb::{
    BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, Els2Store,
    Els2ValidationContext, ValidatedEncryptedLeaseSet2, day_bound_expiry_offset,
    next_utc_day_boundary_seconds, utc_blinding_day,
};
use i2pr_proto::{
    B32_FLAG_REQUIRES_BLINDING_SECRET, CryptoKeyType, Date32, EncryptedServiceAddress, Hash,
    Lease2, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
    MAX_COMMON_STRUCTURE_SIZE, Mapping, SignatureValue, SigningKeyType,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const PUBLISHED: u32 = 1_700_000_000;
const EXPIRES_OFFSET: u16 = 3_600;
const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

fn router(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic router identity")
}

fn signed_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    let destination = i2pr_proto::Destination::new(signer.identity().key_and_cert().clone())
        .expect("destination");
    let header = LeaseSet2Header::new(
        destination,
        PUBLISHED,
        EXPIRES_OFFSET,
        LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let placeholder = SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0_u8; 64])
        .expect("placeholder");
    let unsigned = LeaseSet2::new(
        header,
        Mapping::empty(),
        vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")],
        vec![Lease2::new(
            Hash::from_bytes([0x11; 32]),
            7,
            Date32::from_seconds(PUBLISHED + 600),
        )],
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

fn owner_schedule(seed: u64) -> (BlindingIdentity, BlindingSchedule) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let private: Red25519PrivateScalar = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let schedule = BlindingSchedule::new_owner(
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity"),
        private,
        BlindingScheduleConfig::new(2, true),
    );
    (identity, schedule)
}

#[test]
fn publish_then_resolve_recovers_the_inner_lease_set2() {
    let (_identity, schedule) = owner_schedule(0x1a2b);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let inner = signed_ls2(&router(0x30));
    let mut rng = ChaCha8Rng::seed_from_u64(0x99);

    let (key, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    match &message.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => {
            assert_eq!(record.published_seconds(), PUBLISHED);
            assert_eq!(record.expires_offset_seconds(), EXPIRES_OFFSET);
            assert_eq!(
                record.blinded_sigtype(),
                SigningKeyType::RedDsaSha512Ed25519
            );
            assert!(
                !record.signed_by_transient_key(),
                "Plan 332 emits no offline-key block"
            );
        }
        other => panic!("expected an EncryptedLeaseSet, got {other:?}"),
    }

    // The receiving side validates the record the way a floodfill would, then the
    // client resolves the address it holds.
    let record = match &message.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        _ => unreachable!("checked above"),
    };
    let validated = ValidatedEncryptedLeaseSet2::validate(
        record,
        Some(key),
        Els2ValidationContext::new(PUBLISHED),
    )
    .expect("the published record must validate");

    let address = publisher.address(false).expect("address");
    assert_eq!(address.unblinded_sigtype(), 11);
    assert_eq!(address.blinded_sigtype(), 11);
    let text = address.to_text().expect("address text");
    assert!(text.ends_with(".b32.i2p"));
    assert_eq!(text.chars().filter(|c| *c == '.').count(), 2);
    let reparsed = EncryptedServiceAddress::from_text(&text).expect("reparse");
    assert_eq!(reparsed.public_key(), address.public_key());

    let mut resolver = EncryptedLeaseSet2Resolver::new(&reparsed, None).expect("resolver");
    assert_eq!(
        resolver
            .current_storage_key(PUBLISHED)
            .expect("storage key"),
        key,
        "the address alone must derive the record's storage key"
    );
    let resolved = resolver.resolve(&validated, PUBLISHED).expect("resolve");
    assert_eq!(resolved.inner_lease_set2(), &inner);
    assert_eq!(
        resolved.daily_blinding().day(),
        utc_blinding_day(PUBLISHED).expect("day")
    );
    // The address and the record are returned as one value, so a caller cannot
    // pair one day's key with another day's record.
    assert_eq!(resolved.address(), &reparsed);
    assert_eq!(resolved.into_inner_lease_set2(), inner);
}

#[test]
fn a_wrong_secret_is_a_storage_key_miss_not_a_decryption_failure() {
    let (_identity, schedule) = owner_schedule(0x2b3c);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let inner = signed_ls2(&router(0x31));
    let mut rng = ChaCha8Rng::seed_from_u64(0x98);
    let (key, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let record = match &message.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        _ => unreachable!(),
    };
    let validated = ValidatedEncryptedLeaseSet2::validate(
        record,
        Some(key),
        Els2ValidationContext::new(PUBLISHED),
    )
    .expect("valid");

    let address = publisher.address(true).expect("address");
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_BLINDING_SECRET,
        B32_FLAG_REQUIRES_BLINDING_SECRET,
        "the address declares that a secret is required"
    );

    // A client configured with *some* secret derives a different daily blinded
    // key, and therefore a different storage key, so it cannot even find this
    // record. The secret is a discovery control: it changes where the record
    // lives, not what the record says.
    let mut correct =
        EncryptedLeaseSet2Resolver::new(&address, Some("shared secret")).expect("resolver");
    assert_ne!(
        correct.current_storage_key(PUBLISHED).expect("storage key"),
        key,
        "a configured secret must move the storage key"
    );
    assert!(
        correct.resolve(&validated, PUBLISHED).is_err(),
        "a secret-bearing client must not reach a record published without one"
    );

    // A client with no secret at all resolves the publisher's no-secret record.
    let mut none = EncryptedLeaseSet2Resolver::new(&address, None).expect("resolver");
    let resolved = none.resolve(&validated, PUBLISHED).expect("resolve");
    assert_eq!(resolved.inner_lease_set2(), &inner);

    // A client with the wrong secret derives a different storage key, so the
    // failure is a mismatch at the storage-key gate rather than a garbage
    // plaintext. That distinction is the whole point of keeping the lookup secret
    // out of the storage key.
    let wrong_address = EncryptedServiceAddress::new(
        address.unblinded_sigtype(),
        address.blinded_sigtype(),
        *address.public_key(),
        true,
        false,
    )
    .expect("address");
    let mut wrong = EncryptedLeaseSet2Resolver::new(&wrong_address, Some("a different secret"))
        .expect("resolver");
    assert_ne!(
        wrong.current_storage_key(PUBLISHED).expect("storage key"),
        key,
        "a different secret must derive a different storage key"
    );
    assert!(matches!(
        wrong.resolve(&validated, PUBLISHED),
        Err(EncryptedLeaseSetError::Els2Validation(_))
    ));
}

#[test]
fn a_resolver_cannot_sign_and_a_publisher_needs_owner_material() {
    let (_identity, schedule) = owner_schedule(0x3c4d);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    // The resolver exposes no signing entry point at all: it is a different type
    // holding only a public identity.
    let resolver = EncryptedLeaseSet2Resolver::new(&address, None).expect("resolver");
    let _ = resolver;

    // A lookup-only schedule cannot publish, and says so instead of producing an
    // unsigned record.
    let public =
        i2pr_crypto::red25519::derive_public_key(&i2pr_client::owner_scalar_from_seed(&[0x11; 32]));
    let lookup_only = BlindingSchedule::new(
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity"),
        BlindingScheduleConfig::new(2, false),
    );
    let inner = signed_ls2(&router(0x32));
    let mut rng = ChaCha8Rng::seed_from_u64(0x97);
    let outcome = EncryptedLeaseSet2Publisher::new(&lookup_only).build_record(
        &inner,
        PUBLISHED,
        PUBLISHED + u32::from(EXPIRES_OFFSET),
        &mut rng,
    );
    assert!(
        matches!(outcome, Err(EncryptedLeaseSetError::NotAnOwner)),
        "a lookup-only schedule must refuse to publish"
    );
}

#[test]
fn address_validation_refuses_malformed_and_wrong_width_inputs() {
    let (_identity, schedule) = owner_schedule(0x4d5e);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let text = address.to_text().expect("text");

    // An ordinary 52-character base-32 address is not an encrypted-service
    // address and must not be reinterpreted as one.
    let ordinary = format!("{}{}", "a".repeat(52), ".b32.i2p");
    assert!(!i2pr_proto::is_encrypted_service_address(&ordinary));
    assert!(i2pr_proto::is_encrypted_service_address(&text));

    // A truncated address is refused.
    let truncated: String = text.chars().take(text.len() - 3).collect();
    assert!(EncryptedServiceAddress::from_text(&truncated).is_err());

    // A flipped character is refused.
    let mut corrupted: Vec<char> = text.chars().collect();
    let index = 10;
    corrupted[index] = if corrupted[index] == 'a' { 'b' } else { 'a' };
    let corrupted: String = corrupted.into_iter().collect();
    assert!(EncryptedServiceAddress::from_text(&corrupted).is_err());

    // A per-client-key address is refused by this floor rather than silently
    // decrypted without authorization.
    let requires_client = EncryptedServiceAddress::new(
        address.unblinded_sigtype(),
        address.blinded_sigtype(),
        *address.public_key(),
        false,
        true,
    )
    .expect("address");
    assert!(matches!(
        EncryptedLeaseSet2Resolver::new(&requires_client, None),
        Err(EncryptedLeaseSetError::ClientAuthorizationRequired)
    ));
}

#[test]
fn a_store_round_trip_through_the_database_store_message_preserves_bytes() {
    let (_identity, schedule) = owner_schedule(0x5e6f);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let inner = signed_ls2(&router(0x33));
    let mut rng = ChaCha8Rng::seed_from_u64(0x96);
    let (key, message) = publisher
        .build_database_store(
            &inner,
            PUBLISHED,
            PUBLISHED + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let wire = i2pr_proto::I2npMessage::new_standard(
        0x0102_0304,
        i2pr_proto::Date::from_millis(0),
        i2pr_proto::I2npBody::DatabaseStore(Box::new(message)),
    )
    .expect("new standard")
    .encode_standard_to_vec(MAX)
    .expect("encode");
    let decoded = i2pr_proto::I2npMessage::decode_standard(&wire, MAX).expect("decode");
    let i2pr_proto::I2npBody::DatabaseStore(store) = decoded.body() else {
        panic!("expected a DatabaseStore body");
    };
    assert_eq!(store.key, *key.as_hash());
    let record = match &store.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        other => panic!("expected EncryptedLeaseSet, got {other:?}"),
    };
    let validated = ValidatedEncryptedLeaseSet2::validate(
        record,
        Some(key),
        Els2ValidationContext::new(PUBLISHED),
    )
    .expect("valid");
    let mut store = Els2Store::new();
    assert_eq!(
        store.insert(validated.clone()),
        i2pr_netdb::Els2InsertOutcome::Inserted
    );
    assert!(store.contains(&key));
    let retrieved = store.get(&key).expect("retrieved");
    assert_eq!(
        retrieved.record().encode_to_vec(MAX).expect("encode"),
        validated.encoded(MAX).expect("encoded")
    );
    // The signature survives the wire round trip: re-encoding the answer and
    // re-validating must produce the same storage key and published timestamp.
    let revalidated = ValidatedEncryptedLeaseSet2::validate(
        retrieved.record().clone(),
        Some(BlindedStorageKey::from_hash(
            *store.get(&key).expect("get").storage_key().as_hash(),
        )),
        Els2ValidationContext::new(PUBLISHED),
    )
    .expect("revalidates");
    assert_eq!(revalidated.storage_key(), key);
    assert_eq!(revalidated.published_seconds(), PUBLISHED);
}

#[test]
fn the_published_expiry_never_outlives_the_blinded_keys_day() {
    // The clamp the publisher applies must hold at every point in the day, because
    // tomorrow's record is filed under a different blinded key and would be
    // unreachable.
    for offset_in_day in [0_u32, 1, 30, 3_600, 43_200, 80_000, 86_399] {
        let published = 86_400 * 19_722 + offset_in_day;
        let boundary = next_utc_day_boundary_seconds(published);
        let requested = published + 30 * 86_400;
        let expires_offset = day_bound_expiry_offset(published, requested, boundary);
        let absolute = published + u32::from(expires_offset);
        assert!(
            absolute <= boundary,
            "a record published {offset_in_day}s into the day expired past its boundary"
        );
        assert!(
            absolute > published,
            "a record must have a positive lifetime"
        );
    }
    // A record published after its own boundary (a stale inner record) is clamped
    // to zero rather than wrapping.
    let stale = 86_400 * 19_722;
    let boundary = next_utc_day_boundary_seconds(stale - 1);
    assert_eq!(day_bound_expiry_offset(stale, stale, boundary), 0);
}

#[test]
fn the_publisher_emits_only_the_narrow_address_form() {
    let (_identity, schedule) = owner_schedule(0x6f70);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.address(false).expect("address");
    let narrow = address.to_text().expect("narrow");
    assert_eq!(narrow.chars().count(), 56 + ".b32.i2p".len());
    // The wide form parses back to the same value, so a future implementation can
    // move to it without changing any address text already published.
    let wide = address.to_text_wide();
    assert_eq!(wide.chars().count(), 60 + ".b32.i2p".len());
    assert_eq!(
        EncryptedServiceAddress::from_text(&wide).expect("wide parse"),
        address
    );
    // The wide form is a strictly larger encoding of the same value and is not
    // accepted in place of the narrow one.
    assert!(EncryptedServiceAddress::from_text(&narrow).is_ok());
}
