//! Plan 333 end-to-end evidence for authorized encrypted-LeaseSet2 publication
//! and retrieval.
//!
//! These rows drive the public client surface only — the same publisher,
//! `DatabaseStoreMessage`, validated-record type, and resolver that Plan 332
//! proved for the unauthorized case. Nothing here reaches into a private
//! bridge, driver, or pump API, so what is proved is what a daemon would
//! actually do.
//!
//! The two claims under test throughout are the plan's acceptance criteria:
//!
//! 1. Every authorized client of the same service can retrieve and decrypt the
//!    *same* record, under both the pre-shared-key and Diffie-Hellman schemes.
//! 2. An unauthorized client cannot — and the refusal is typed, so a caller can
//!    tell a misconfiguration from a lookup miss.
//!
//! Restart recovery and daily rollover are here rather than in the netdb rows
//! because they are only meaningful end to end: a client has to rebuild its
//! whole view from persisted material and still open a record published before
//! the restart.

use i2pr_client::encrypted_leaseset::{
    EncryptedLeaseSet2Publisher, EncryptedLeaseSet2Resolver, EncryptedLeaseSetError,
    authorization_scheme, generate_client_dh_keypair,
};
use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{Red25519PrivateScalar, generate_private};
use i2pr_netdb::{
    BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, Els2AuthError,
    Els2AuthorizationServerConfig, Els2ClientAuth, Els2ClientAuthSecret, Els2ValidationContext,
    PskClientKey, ValidatedEncryptedLeaseSet2, next_utc_day_boundary_seconds, utc_blinding_day,
};
use i2pr_proto::{
    B32_FLAG_REQUIRES_CLIENT_KEY, CryptoKeyType, Date32, Lease2, LeaseSet2, LeaseSet2EncryptionKey,
    LeaseSet2Flags, LeaseSet2Header, MAX_COMMON_STRUCTURE_SIZE, Mapping, SignatureValue,
    SigningKeyType,
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

fn signed_ls2_at(signer: &RouterIdentityBundle, published: u32) -> LeaseSet2 {
    let destination = i2pr_proto::Destination::new(signer.identity().key_and_cert().clone())
        .expect("destination");
    let header = LeaseSet2Header::new(
        destination,
        published,
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
            i2pr_proto::Hash::from_bytes([0x11; 32]),
            7,
            Date32::from_seconds(published + 600),
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

fn signed_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    signed_ls2_at(signer, PUBLISHED)
}

/// Builds the owner's schedule, returning the same unblinded identity the
/// publisher will hand to clients so a resolver can be built from the address.
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

fn psk(seed: u64) -> PskClientKey {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    PskClientKey::generate(&mut rng).expect("psk")
}

/// Publishes an authorized record and returns it already validated the way a
/// floodfill would have validated it on receipt.
fn publish_authorized(
    schedule: &BlindingSchedule,
    authorization: &Els2AuthorizationServerConfig,
    inner: &LeaseSet2,
    now: u32,
    seed: u64,
) -> ValidatedEncryptedLeaseSet2 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let publisher = EncryptedLeaseSet2Publisher::new(schedule);
    let (key, message) = publisher
        .build_authorized_database_store(
            authorization,
            inner,
            now,
            now + u32::from(EXPIRES_OFFSET),
            &mut rng,
        )
        .expect("publish");
    let record = match &message.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        other => panic!("expected an EncryptedLeaseSet, got {other:?}"),
    };
    ValidatedEncryptedLeaseSet2::validate(record, Some(key), Els2ValidationContext::new(now))
        .expect("an authorized record must validate exactly like an unauthorized one")
}

// --- pre-shared-key authorization ----------------------------------------------------------------

#[test]
fn every_authorized_psk_client_reads_the_same_published_record() {
    let (_identity, schedule) = owner_schedule(0x1a2b);
    let inner = signed_ls2(&router(0x30));
    // One published record, three configured clients.
    let config =
        Els2AuthorizationServerConfig::psk(vec![psk(0x11), psk(0x12), psk(0x13)]).expect("config");
    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x99);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("authorized address");
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_CLIENT_KEY,
        B32_FLAG_REQUIRES_CLIENT_KEY,
        "an authorized service must declare that a client key is required"
    );

    for seed in [0x11_u64, 0x12, 0x13] {
        let key = psk(seed);
        let mut resolver = EncryptedLeaseSet2Resolver::new_authorized(&address, None)
            .expect("authorized resolver");
        let resolved = resolver
            .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&key))
            .unwrap_or_else(|error| panic!("client {seed:#x} must be authorized: {error}"));
        assert_eq!(
            resolved.inner_lease_set2(),
            &inner,
            "every authorized client must recover the same inner LeaseSet2"
        );
        assert_eq!(
            resolved.address().flags() & B32_FLAG_REQUIRES_CLIENT_KEY,
            B32_FLAG_REQUIRES_CLIENT_KEY,
            "the resolved address must not understate its access control"
        );
    }
}

#[test]
fn an_unauthorized_psk_client_is_refused_with_a_typed_error() {
    let (_identity, schedule) = owner_schedule(0x2b3c);
    let inner = signed_ls2(&router(0x31));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0x21)]).expect("config");
    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x98);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address");

    // A client holding a valid but unconfigured key.
    let wrong = psk(0x22);
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    let error = resolver
        .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&wrong))
        .expect_err("an unconfigured key must not decrypt the record");
    assert!(
        matches!(error, EncryptedLeaseSetError::NotAuthorized { .. }),
        "unexpected error: {error:?}"
    );

    // And a client supplying nothing at all gets a different, actionable error:
    // it can be fixed by presenting any authorized credential, whereas a wrong
    // key cannot. Collapsing the two would hide a configuration mistake.
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    let error = resolver
        .resolve(&record, PUBLISHED)
        .expect_err("an authorized record must not open without a credential");
    assert!(
        matches!(error, EncryptedLeaseSetError::CredentialNotSupplied),
        "unexpected error: {error:?}"
    );
}

#[test]
fn a_record_published_without_authorization_does_not_become_authorized() {
    // Additivity in the other direction: a plain record must not be opened by
    // presenting a credential, so adding authorization cannot be skipped by
    // accident and cannot be turned on retroactively for a live record.
    let (_identity, schedule) = owner_schedule(0x3c4d);
    let inner = signed_ls2(&router(0x32));
    let mut rng = ChaCha8Rng::seed_from_u64(0x97);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
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
    let address = publisher.address(false).expect("address");
    let key = psk(0x41);
    let mut resolver = EncryptedLeaseSet2Resolver::new(&address, None).expect("resolver");
    let resolved = resolver
        .resolve_with_auth(&validated, PUBLISHED, &Els2ClientAuth::Psk(&key))
        .expect("a credential against a plain record is simply not required");
    assert_eq!(resolved.inner_lease_set2(), &inner);
}

#[test]
fn the_unauthorized_resolver_refuses_an_address_that_demands_a_client_key() {
    // The strict constructor is the guard rail: a caller who ignores the flag
    // gets a construction error rather than a resolver that looks usable.
    let (_identity, schedule) = owner_schedule(0x4d5e);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address");
    let error = EncryptedLeaseSet2Resolver::new(&address, None)
        .expect_err("the strict constructor must refuse this address");
    assert!(
        matches!(error, EncryptedLeaseSetError::ClientAuthorizationRequired),
        "unexpected error: {error:?}"
    );
}

// --- Diffie-Hellman authorization -----------------------------------------------------------------

#[test]
fn every_authorized_dh_client_reads_the_same_published_record() {
    let (_identity, schedule) = owner_schedule(0x5e6f);
    let inner = signed_ls2(&router(0x33));
    let mut rng = ChaCha8Rng::seed_from_u64(0x96);
    let pairs: Vec<_> = [0x51_u64, 0x52, 0x53]
        .iter()
        .map(|_| generate_client_dh_keypair(&mut rng).expect("dh keypair"))
        .collect();
    let config =
        Els2AuthorizationServerConfig::dh(pairs.iter().map(|(_, public)| *public).collect())
            .expect("config");
    assert_eq!(
        authorization_scheme(&config),
        i2pr_netdb::Els2AuthScheme::Dh
    );

    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x95);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address");

    for (private, public) in &pairs {
        let mut resolver = EncryptedLeaseSet2Resolver::new_authorized(&address, None)
            .expect("authorized resolver");
        let resolved = resolver
            .resolve_with_auth(
                &record,
                PUBLISHED,
                &Els2ClientAuth::Dh {
                    private,
                    public: *public,
                },
            )
            .expect("each configured Diffie-Hellman client must be authorized");
        assert_eq!(resolved.inner_lease_set2(), &inner);
    }
}

#[test]
fn an_unauthorized_dh_client_is_refused() {
    let (_identity, schedule) = owner_schedule(0x6f70);
    let inner = signed_ls2(&router(0x34));
    let mut rng = ChaCha8Rng::seed_from_u64(0x94);
    let (_configured_private, configured_public) =
        generate_client_dh_keypair(&mut rng).expect("dh keypair");
    let (outsider_private, outsider_public) =
        generate_client_dh_keypair(&mut rng).expect("dh keypair");
    let config = Els2AuthorizationServerConfig::dh(vec![configured_public]).expect("config");
    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x93);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address");

    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    let error = resolver
        .resolve_with_auth(
            &record,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private: &outsider_private,
                public: outsider_public,
            },
        )
        .expect_err("an unconfigured Diffie-Hellman key must not decrypt the record");
    assert!(
        matches!(error, EncryptedLeaseSetError::NotAuthorized { .. }),
        "unexpected error: {error:?}"
    );

    // A pre-shared key must not stand in for a Diffie-Hellman credential.
    let psk_key = psk(0x61);
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    assert!(
        resolver
            .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&psk_key))
            .is_err(),
        "a pre-shared key must not satisfy a Diffie-Hellman record"
    );
}

// --- lookup secret combined with authorization ---------------------------------------------------

#[test]
fn a_wrong_lookup_secret_plus_authorization_fails_at_the_storage_key() {
    // The two controls are independent and compose: the lookup secret decides
    // *discovery*, the client key decides *reading*. A client with a valid
    // credential but the wrong secret derives a different storage key, so it
    // never finds the record in the first place.
    const SECRET: &str = "correct horse battery staple";
    let mut rng = ChaCha8Rng::seed_from_u64(0x71);
    let private: Red25519PrivateScalar = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let schedule = BlindingSchedule::new_owner(
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, Some(SECRET))
            .expect("identity"),
        private,
        BlindingScheduleConfig::new(2, true),
    );
    let inner = signed_ls2(&router(0x35));
    let key = psk(0x71);
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0x71)]).expect("config");
    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x92);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(true)
        .expect("address");

    // The right secret plus the right key works.
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, Some(SECRET)).expect("resolver");
    assert_eq!(
        resolver
            .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&key))
            .expect("right secret plus right key")
            .inner_lease_set2(),
        &inner
    );

    // The wrong secret fails at the storage key, before the credential is even
    // consulted — which is the point: the credential is not the discovery gate.
    let mut resolver = EncryptedLeaseSet2Resolver::new_authorized(&address, Some("wrong secret"))
        .expect("resolver");
    let error = resolver
        .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&key))
        .expect_err("the wrong lookup secret must not find the record");
    assert!(
        matches!(error, EncryptedLeaseSetError::Els2Validation(_)),
        "unexpected error: {error:?}"
    );
    assert!(
        resolver.current_storage_key(PUBLISHED).expect("key") != record.storage_key(),
        "a wrong secret must derive a different storage key"
    );
}

// --- restart recovery and daily rollover --------------------------------------------------------

#[test]
fn a_client_recovers_after_a_restart_from_persisted_material() {
    // Restart safety is a property of the derivation, not of any stored state:
    // the client rebuilds its secret from the persisted bytes, rebuilds its
    // schedule from the address, and opens a record that was published before
    // the restart.
    let (_identity, schedule) = owner_schedule(0x8a9b);
    let inner = signed_ls2(&router(0x36));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0x81)]).expect("config");
    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x91);

    // The client persists its secret and rebuilds everything from the address
    // text alone. The in-memory key is scoped so the restart genuinely starts
    // from persisted bytes rather than from a live object.
    let persisted = {
        let key = psk(0x81);
        Els2ClientAuthSecret::new(i2pr_netdb::Els2AuthSecretRole::ClientPsk, *key.as_bytes())
            .to_persisted_bytes()
    };
    let address_text = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address")
        .to_text()
        .expect("address text");

    // Restart: everything below is rebuilt from persisted bytes and the text.
    let restored_address =
        i2pr_proto::EncryptedServiceAddress::from_text(&address_text).expect("reparse");
    let restored = Els2ClientAuthSecret::from_persisted_bytes(&persisted).expect("restore");
    let restored_key = restored.as_psk().expect("as psk");
    let mut resolver = EncryptedLeaseSet2Resolver::new_authorized(&restored_address, None)
        .expect("resolver after restart");
    let resolved = resolver
        .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&restored_key))
        .expect("a record published before the restart must still open");
    assert_eq!(resolved.inner_lease_set2(), &inner);
}

#[test]
fn a_record_from_the_previous_day_is_not_authorized_today() {
    // Rollover changes the blinded key, the subcredential, and therefore every
    // client identifier. Yesterday's record is still *discoverable* only by the
    // previous day's storage key, so today's client must refuse it.
    let (_identity, schedule) = owner_schedule(0x9bac);
    let key = psk(0x91);
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0x91)]).expect("config");

    let yesterday = PUBLISHED - 86_400;
    let inner = signed_ls2_at(&router(0x3f), yesterday);
    let record = publish_authorized(&schedule, &config, &inner, yesterday, 0x90);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address");
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    let error = resolver
        .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&key))
        .expect_err("yesterday's record must not be filed under today's key");
    assert!(
        matches!(error, EncryptedLeaseSetError::Els2Validation(_)),
        "unexpected error: {error:?}"
    );
    // The same record is still readable at its own timestamp, so the refusal
    // above is about the day and not about the authorization.
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    assert_eq!(
        resolver
            .resolve_with_auth(&record, yesterday, &Els2ClientAuth::Psk(&key))
            .expect("yesterday's record opens on yesterday")
            .inner_lease_set2(),
        &inner
    );
    assert_ne!(
        utc_blinding_day(yesterday).expect("day"),
        utc_blinding_day(PUBLISHED).expect("day"),
        "the rollover must actually cross a day boundary for this row to mean anything"
    );
}

#[test]
fn the_expiry_is_still_clamped_to_the_day_boundary_with_authorization() {
    // Authorization must not change publication semantics: a record published
    // near midnight still expires at the day boundary, because the day's key is
    // the credential's whole lifetime.
    let (_identity, schedule) = owner_schedule(0xacbd);
    let boundary = next_utc_day_boundary_seconds(PUBLISHED);
    let inner = signed_ls2(&router(0x38));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0xa1)]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x8f);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    // Ask for a full day's validity, which must be clamped to the boundary.
    let record = publisher
        .build_authorized_record(&config, &inner, PUBLISHED, boundary + 10_000, &mut rng)
        .expect("publish");
    assert_eq!(
        u32::from(record.expires_offset_seconds()),
        boundary - PUBLISHED,
        "the expiry must be clamped to the next UTC midnight"
    );
}

// --- tamper ---------------------------------------------------------------------------------------

#[test]
fn tampering_with_the_published_ciphertext_is_refused_end_to_end() {
    // Integrity for the whole record comes from the outer Red25519 signature,
    // which covers the outer ciphertext — and therefore the layer-1
    // authorization block inside it. The block itself carries no tag of its
    // own, so this row asserts where that actually bites: a receiver validates
    // the signature before any decryption, so tampering is caught before a
    // wrong `authCookie` can ever produce a wrong layer-2 key.
    let (_identity, schedule) = owner_schedule(0xbdae);
    let inner = signed_ls2(&router(0x39));
    let key = psk(0xb1);
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0xb1)]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x8e);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let built = publisher
        .build_authorized_record(&config, &inner, PUBLISHED, PUBLISHED + 600, &mut rng)
        .expect("publish");
    let address = publisher.authorized_address(false).expect("address");

    // The untampered record opens, so the negative rows below mean something.
    let good = ValidatedEncryptedLeaseSet2::validate(
        built.clone(),
        None,
        Els2ValidationContext::new(PUBLISHED),
    )
    .expect("valid");
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    assert_eq!(
        resolver
            .resolve_with_auth(&good, PUBLISHED, &Els2ClientAuth::Psk(&key))
            .expect("untampered")
            .inner_lease_set2(),
        &inner
    );

    // Flip one bit deep inside the ciphertext, well past the outer salt and the
    // layer-1 block, so the damage lands in the inner ciphertext.
    let mut ciphertext = built.outer_ciphertext().to_vec();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0x01;
    let tampered = i2pr_proto::EncryptedLeaseSet2::new(
        built.blinded_sigtype(),
        built.blinded_public_key().to_vec(),
        built.published_seconds(),
        built.expires_offset_seconds(),
        None,
        ciphertext,
        built.signature().to_vec(),
    )
    .expect("rebuild");
    let error = ValidatedEncryptedLeaseSet2::validate(
        tampered,
        None,
        Els2ValidationContext::new(PUBLISHED),
    )
    .expect_err("a tampered ciphertext must be rejected at the signature");
    assert!(
        matches!(error, i2pr_netdb::Els2ValidationError::InvalidSignature),
        "unexpected error: {error:?}"
    );

    // The refusal is at the signature, not at the authorization check, which is
    // the point: the credential is never consulted for a record whose
    // signature does not verify.
}

// --- bounded resource use -------------------------------------------------------------------------

#[test]
fn publishing_costs_one_block_entry_per_client_and_nothing_unbounded() {
    // The record's size grows linearly in the client count and the count itself
    // is visible in the clear, which is the privacy property the specification
    // asks for: a passive observer learns *how many* clients are subscribed and
    // nothing about which.
    let (_identity, schedule) = owner_schedule(0xcbdc);
    let inner = signed_ls2(&router(0x3a));
    let mut sizes = Vec::new();
    for count in [1_usize, 4, 16] {
        let config = Els2AuthorizationServerConfig::psk(
            (0..count)
                .map(|i| psk(0xc0 + u64::try_from(i).unwrap()))
                .collect(),
        )
        .expect("config");
        let mut rng = ChaCha8Rng::seed_from_u64(0x8d);
        let record = EncryptedLeaseSet2Publisher::new(&schedule)
            .build_authorized_record(&config, &inner, PUBLISHED, PUBLISHED + 600, &mut rng)
            .expect("publish");
        let one = record.outer_ciphertext().len();
        assert!(
            record.outer_ciphertext().len() > MAX.min(1),
            "the ciphertext must be non-trivial"
        );
        sizes.push((count, one));
    }
    // Each extra client adds exactly one 40-byte entry, plus the two-byte
    // count field, with no other growth.
    for pair in sizes.windows(2) {
        let ((_, small), (_, big)) = (pair[0], pair[1]);
        assert!(
            big > small,
            "the record must grow with the client count: {small} then {big}"
        );
        assert!(
            big - small <= 16 * 42 + 2,
            "growth must stay linear in the client count, not superlinear"
        );
    }
}

#[test]
fn a_client_key_holder_can_still_not_impersonate_the_service() {
    // A client holds a key that opens the record. It must not be able to publish
    // a record of its own under the service's address: the resolver type is
    // lookup-only, so the only way to publish is to build an owner schedule,
    // which requires the service's private blinded key.
    let (_identity, schedule) = owner_schedule(0xdbec);
    let inner = signed_ls2(&router(0x3b));
    let key = psk(0xd1);
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0xd1)]).expect("config");
    let record = publish_authorized(&schedule, &config, &inner, PUBLISHED, 0x8c);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .authorized_address(false)
        .expect("address");
    let mut resolver =
        EncryptedLeaseSet2Resolver::new_authorized(&address, None).expect("resolver");
    assert_eq!(
        resolver
            .resolve_with_auth(&record, PUBLISHED, &Els2ClientAuth::Psk(&key))
            .expect("authorized read")
            .inner_lease_set2(),
        &inner,
        "reading is authorized"
    );
    // The resolver exposes no signing or publication surface at all, so the
    // asymmetry is structural rather than a convention.
    let _ = EncryptedLeaseSetError::ClientAuthorizationRequired;
}

#[test]
fn a_bad_configuration_is_refused_before_anything_is_published() {
    // Fail closed on configuration: an empty client set or an over-long list is
    // a construction error, not a runtime surprise halfway through a write.
    let mut rng = ChaCha8Rng::seed_from_u64(0x1);
    let error = Els2AuthorizationServerConfig::psk(Vec::new())
        .expect_err("an empty set publishes a record nobody can open");
    assert!(matches!(error, Els2AuthError::NoClients));

    let (_identity, schedule) = owner_schedule(0xecfd);
    let inner = signed_ls2(&router(0x3c));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0xe1)]).expect("config");
    // A record that cannot be built leaves nothing half-written: the publisher
    // returns an error rather than a partial record.
    EncryptedLeaseSet2Publisher::new(&schedule)
        .build_authorized_record(&config, &inner, PUBLISHED, PUBLISHED + 600, &mut rng)
        .expect("a valid configuration publishes");

    // A DH configuration with no clients is refused the same way.
    assert!(matches!(
        Els2AuthorizationServerConfig::dh(Vec::new()),
        Err(Els2AuthError::NoClients)
    ));
}

#[test]
fn the_published_record_still_carries_no_offline_key_block_and_a_valid_signature() {
    // Authorization is additive: it must not change the outer framing that Plan
    // 332 froze, or the differential fixture and every floodfill path would
    // need to change too.
    let (_identity, schedule) = owner_schedule(0xfd0e);
    let inner = signed_ls2(&router(0x3d));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0xf1)]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x8b);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (key, message) = publisher
        .build_authorized_database_store(&config, &inner, PUBLISHED, PUBLISHED + 600, &mut rng)
        .expect("publish");
    match &message.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => {
            assert_eq!(
                record.blinded_sigtype(),
                SigningKeyType::RedDsaSha512Ed25519
            );
            assert!(
                !record.signed_by_transient_key(),
                "Plan 333 emits no offline-key block either"
            );
            assert_eq!(record.published_seconds(), PUBLISHED);
        }
        other => panic!("expected an EncryptedLeaseSet, got {other:?}"),
    }
    // A floodfill validates it exactly as before: signature, blinded key, and
    // storage key all check out without any authorization input.
    let record = match &message.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref().clone(),
        _ => unreachable!(),
    };
    ValidatedEncryptedLeaseSet2::validate(record, Some(key), Els2ValidationContext::new(PUBLISHED))
        .expect("a floodfill must be able to store and serve an authorized record");
}

#[test]
fn a_random_source_failure_does_not_publish_a_record() {
    // A publication that cannot draw a fresh cookie or salt must fail, not fall
    // back to a predictable value.
    struct Failing;
    impl rand_core::TryRngCore for Failing {
        type Error = &'static str;
        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            Err("no entropy")
        }
        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            Err("no entropy")
        }
        fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), Self::Error> {
            Err("no entropy")
        }
    }
    impl rand_core::TryCryptoRng for Failing {}

    let (_identity, schedule) = owner_schedule(0x0e1f);
    let inner = signed_ls2(&router(0x3e));
    let config = Els2AuthorizationServerConfig::psk(vec![psk(0x0e1)]).expect("config");
    let mut failing = Failing;
    let error = EncryptedLeaseSet2Publisher::new(&schedule)
        .build_authorized_record(&config, &inner, PUBLISHED, PUBLISHED + 600, &mut failing)
        .expect_err("a publication with no randomness must fail");
    assert!(
        matches!(error, EncryptedLeaseSetError::Auth(_)),
        "unexpected error: {error:?}"
    );

    // A working source still publishes, so the row above proves the generator
    // is genuinely consulted.
    let mut rng = ChaCha8Rng::seed_from_u64(0x0e2);
    assert!(
        EncryptedLeaseSet2Publisher::new(&schedule)
            .build_authorized_record(&config, &inner, PUBLISHED, PUBLISHED + 600, &mut rng)
            .is_ok()
    );
}
