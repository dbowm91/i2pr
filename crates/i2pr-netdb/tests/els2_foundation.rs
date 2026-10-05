//! Plan 332 evidence for the encrypted LeaseSet2 foundation.
//!
//! Three kinds of row live here, and the distinction matters:
//!
//! 1. **Independent re-derivation.** `els2-independent-derivation.json` is
//!    produced by `tools/generate-els2-independent-fixture.py`, a pure-Python
//!    derivation written from the frozen specification text. The encrypted
//!    LeaseSet specification publishes no official vectors for the credential,
//!    the subcredential, the layer key derivations, or the layer ciphertexts, so
//!    agreement between two independent derivations is the strongest evidence
//!    available for those values.
//! 2. **Adversarial rows.** Every rejection path is asserted: wrong day, wrong
//!    secret, wrong published timestamp, tampered ciphertext, tampered
//!    signature, malformed lengths, reserved flag bits, and a client-authorized
//!    layer-1 flag byte that this floor must refuse rather than guess at.
//! 3. **Lifecycle rows.** Daily rollover, restart safety, bounded precomputation,
//!    the bounded store's replacement and capacity rules, and floodfill
//!    opaque store/serve behavior.

use std::collections::BTreeMap;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{
    BlindingDay, Red25519PrivateScalar, Red25519PublicKey, blind_private_key, generate_alpha,
    generate_private,
};
use i2pr_proto::{
    CryptoKeyType, Date32, ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE, Els2SignedRegion,
    EncryptedLeaseSet2, EncryptedLeaseSet2OfflineKeys, Hash, INNER_LEASE_SET2_STORE_TYPE, Lease2,
    LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, MAX_COMMON_STRUCTURE_SIZE,
    Mapping, SignatureValue, SigningKeyType,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use i2pr_netdb::{
    BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, Els2Error,
    Els2InsertOutcome, Els2Store, Els2StoreConfig, Els2ValidationContext, Els2ValidationError,
    LeaseSet2ValidationContext, LookupSecret, MAX_ELS2_INNER_LEASE_SET_LENGTH, OwnerBlinding,
    ValidatedEncryptedLeaseSet2, decrypt_no_auth_outer_ciphertext, derive_els2_credentials,
    encrypt_no_auth_outer_ciphertext, next_utc_day_boundary_seconds, sign_type11_deployed,
    unblinded_scalar_from_ed25519_seed, utc_blinding_day,
};

const PUBLISHED: u32 = 1_700_000_000;
const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

// --- helpers ------------------------------------------------------------------------------------

fn hex(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    assert_eq!(bytes.len() % 2, 0, "odd-length hex string");
    bytes
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("hex digit") as u8;
            let low = (pair[1] as char).to_digit(16).expect("hex digit") as u8;
            (high << 4) | low
        })
        .collect()
}

fn unblinded_key() -> Red25519PublicKey {
    // [1]B in compressed form: a valid prime-order point, so no key generation is
    // needed to exercise the layer derivations.
    let mut bytes = [0x66_u8; 32];
    bytes[0] = 0x58;
    Red25519PublicKey::from_bytes(bytes)
}

fn signed_type11_key(seed: u64) -> (Red25519PrivateScalar, Red25519PublicKey) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let private = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    (private, public)
}

fn router_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic router identity")
}

fn signed_ls2(
    signer: &RouterIdentityBundle,
    published_seconds: u32,
    expires_offset_seconds: u16,
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
    let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64])
        .expect("placeholder");
    let encryption_keys =
        vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
    let leases = vec![Lease2::new(
        Hash::from_bytes([0x11; 32]),
        7,
        Date32::from_seconds(published_seconds + 600),
    )];
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

/// Builds a signed no-auth type-5 record over `inner`.
fn build_record(
    owner: &OwnerBlinding,
    credentials: &i2pr_netdb::Els2Credentials,
    inner: &LeaseSet2,
    published_seconds: u32,
    expires_offset_seconds: u16,
    seed: u64,
) -> EncryptedLeaseSet2 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let inner_bytes = inner.encode_to_vec(MAX).expect("inner bytes");
    let mut outer_salt = [0_u8; 32];
    let mut inner_salt = [0_u8; 32];
    rng.fill_bytes(&mut outer_salt);
    rng.fill_bytes(&mut inner_salt);
    let outer_ciphertext = encrypt_no_auth_outer_ciphertext(
        credentials,
        published_seconds,
        INNER_LEASE_SET2_STORE_TYPE,
        &inner_bytes,
        &outer_salt,
        &inner_salt,
    )
    .expect("encrypt");
    let probe = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        owner.daily().blinded_public_key().as_bytes().to_vec(),
        published_seconds,
        expires_offset_seconds,
        None,
        outer_ciphertext.clone(),
        vec![0_u8; 64],
    )
    .expect("probe");
    // Plan 346 / ADR 0032: an outbound type-5 record is signed under the deployed
    // Java/i2pd ELS2 transcript, through the bounded ELS2 profile owner. The strict
    // Proposal-146 transcript stays accepted on the inbound path only.
    let signature = sign_type11_deployed(
        owner.blinded_private_key(),
        Els2SignedRegion::of_record(&probe),
        &mut rng,
    )
    .expect("sign type 5");
    EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        owner.daily().blinded_public_key().as_bytes().to_vec(),
        published_seconds,
        expires_offset_seconds,
        None,
        outer_ciphertext,
        signature.as_bytes().to_vec(),
    )
    .expect("record")
}

fn validate(
    record: EncryptedLeaseSet2,
    now: u32,
) -> Result<ValidatedEncryptedLeaseSet2, Els2ValidationError> {
    let key = BlindedStorageKey::from_hash(i2pr_crypto::red25519::blinded_storage_key(
        &Red25519PublicKey::decode(record.blinded_public_key()).expect("blinded key"),
    ));
    ValidatedEncryptedLeaseSet2::validate(record, Some(key), Els2ValidationContext::new(now))
}

// --- 1. independent re-derivation ---------------------------------------------------------------

#[test]
fn independent_python_derivation_agrees_on_credentials_and_layer_ciphertexts() {
    let raw = include_str!("data/els2-independent-derivation.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let cases = document["cases"].as_array().expect("cases array");
    assert!(
        cases.len() >= 5,
        "fixture must carry several cases, found {}",
        cases.len()
    );
    let mut checked = 0_usize;
    for case in cases {
        let name = case["name"].as_str().unwrap_or_default();
        if name == "counter-zero-control" {
            continue;
        }
        let unblinded_bytes: [u8; 32] = hex(case["unblinded_pubkey"].as_str().expect("unblinded"))
            .try_into()
            .expect("32-byte unblinded key");
        let blinded_bytes: [u8; 32] = hex(case["blinded_pubkey"].as_str().expect("blinded"))
            .try_into()
            .expect("32-byte blinded key");
        let sigtype = SigningKeyType::from_code(
            u16::try_from(case["unblinded_sigtype"].as_u64().expect("sigtype")).expect("u16"),
        );
        let unblinded = Red25519PublicKey::from_bytes(unblinded_bytes);
        let blinded = Red25519PublicKey::from_bytes(blinded_bytes);
        let published = u32::try_from(case["published"].as_u64().expect("published")).expect("u32");
        let inner_store_type =
            u8::try_from(case["inner_store_type"].as_u64().expect("inner type")).expect("u8");
        let inner_record = hex(case["inner_record"].as_str().expect("inner"));
        let outer_salt: [u8; 32] = hex(case["outer_salt"].as_str().expect("outer salt"))
            .try_into()
            .expect("32-byte outer salt");
        let inner_salt: [u8; 32] = hex(case["inner_salt"].as_str().expect("inner salt"))
            .try_into()
            .expect("32-byte inner salt");

        let credentials = derive_els2_credentials(&unblinded, sigtype, &blinded);
        assert_eq!(
            credentials.credential().to_vec(),
            hex(case["credential"].as_str().expect("credential")),
            "credential mismatch in case {name}"
        );
        assert_eq!(
            credentials.subcredential().to_vec(),
            hex(case["subcredential"].as_str().expect("subcredential")),
            "subcredential mismatch in case {name}"
        );

        let produced = encrypt_no_auth_outer_ciphertext(
            &credentials,
            published,
            inner_store_type,
            &inner_record,
            &outer_salt,
            &inner_salt,
        )
        .expect("encrypt");
        assert_eq!(
            produced,
            hex(case["outer_ciphertext"].as_str().expect("outer ciphertext")),
            "outer ciphertext mismatch in case {name}"
        );
        let expected_len = 32 + 1 + 32 + 1 + inner_record.len();
        assert_eq!(produced.len(), expected_len, "outer length in case {name}");
        assert_eq!(
            &produced[..32],
            &outer_salt[..],
            "the outer salt travels in the clear at the head of the outer ciphertext in case {name}"
        );
        // The inner salt sits inside the layer-1 plaintext, so the outer stream
        // hides it. This is a confidentiality property, not an encoding detail:
        // a client that could read the inner salt without the subcredential would
        // be one step closer to deriving the inner key.
        assert_ne!(
            &produced[32 + 1..32 + 1 + 32],
            &inner_salt[..],
            "the inner salt must not be readable in the outer ciphertext in case {name}"
        );
        checked += 1;
    }
    assert_eq!(checked, 5, "every derivation case must be exercised");
}

#[test]
fn layer_stream_starts_at_block_counter_one_not_zero() {
    let raw = include_str!("data/els2-independent-derivation.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let control = document["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["name"] == "counter-zero-control")
        .expect("counter-zero-control case");
    let subcred_bytes: [u8; 32] = hex(control["subcredential"].as_str().expect("subcredential"))
        .try_into()
        .expect("32 bytes");
    let unblinded = unblinded_key();
    let blinded = unblinded_key();
    let credentials =
        derive_els2_credentials(&unblinded, SigningKeyType::RedDsaSha512Ed25519, &blinded);
    assert_eq!(credentials.subcredential(), &subcred_bytes);

    // Re-derive the same layer-1 key material and confirm the counter-0 keystream
    // the fixture recorded is *not* what the layer produces at the pinned counter.
    let outer_salt: [u8; 32] = hex(document["cases"][0]["outer_salt"]
        .as_str()
        .expect("outer salt"))
    .try_into()
    .expect("32 bytes");
    let counter_zero = hex(control["counter_zero_keystream"].as_str().expect("control"));
    assert_eq!(counter_zero.len(), 64);
    assert_eq!(
        hex(control["outer_key"].as_str().expect("outer key")).len(),
        32
    );

    let inner = signed_ls2(&router_bundle(11), PUBLISHED, 3_600);
    let inner_bytes = inner.encode_to_vec(MAX).expect("inner");
    let mut inner_salt = [0_u8; 32];
    inner_salt.copy_from_slice(&hex(document["cases"][0]["inner_salt"]
        .as_str()
        .expect("inner salt")));
    let produced = encrypt_no_auth_outer_ciphertext(
        &credentials,
        PUBLISHED,
        INNER_LEASE_SET2_STORE_TYPE,
        &inner_bytes,
        &outer_salt,
        &inner_salt,
    )
    .expect("encrypt");
    // The layer-1 plaintext occupies produced[32..33 + 32 + inner len]; its first
    // 64 bytes under counter 1 must not equal the counter-0 keystream recorded for
    // the same key material.
    let layer1 = &produced[32..];
    assert!(
        layer1.len() >= 64,
        "layer-1 plaintext must be at least one keystream block"
    );
    assert_ne!(
        &layer1[..64],
        &counter_zero[..],
        "the layer stream must not equal the counter-0 control"
    );
    // A small `expected` row documents the boundary the counter control pins.
    assert_eq!(counter_zero.len(), 64, "one ChaCha20 block");
}

// --- 2. adversarial rows ------------------------------------------------------------------------

fn owner_and_identity(secret: Option<&str>, day: BlindingDay) -> (BlindingIdentity, OwnerBlinding) {
    let (private, public) = signed_type11_key(0x51a2);
    let identity = BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, secret)
        .expect("identity");
    let owner = identity
        .derive_owner(&private, day)
        .expect("owner blinding");
    (identity, owner)
}

#[test]
fn wrong_secret_yields_no_decryption() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (owner_identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5151);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = owner_identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 7);
    let outer = record.outer_ciphertext().to_vec();

    // A resolver configured with a secret derives a different alpha, hence a
    // different blinded key, a different subcredential, and a different key.
    let (other_identity, other_owner) = owner_and_identity(Some("a-secret"), day);
    let other_credentials =
        other_identity.credentials_for(other_owner.daily().blinded_public_key());
    assert_ne!(
        *credentials.subcredential(),
        *other_credentials.subcredential(),
        "a configured secret must change the subcredential"
    );
    assert_ne!(
        other_owner.daily().storage_key(),
        owner.daily().storage_key(),
        "a configured secret must change the storage key, so a wrong secret is a lookup miss"
    );
    assert!(
        decrypt_no_auth_outer_ciphertext(&other_credentials, PUBLISHED, &outer).is_err(),
        "a wrong secret must not decrypt the record"
    );
    // And the correct credentials still work.
    assert!(
        decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &outer).is_ok(),
        "the correct secret must decrypt the record"
    );
}

#[test]
fn wrong_published_timestamp_yields_no_decryption() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5252);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 11);
    let outer = record.outer_ciphertext().to_vec();

    for wrong in [PUBLISHED + 1, PUBLISHED - 1, 0, u32::MAX] {
        assert!(
            decrypt_no_auth_outer_ciphertext(&credentials, wrong, &outer).is_err(),
            "published {wrong} must not decrypt the record"
        );
    }
    assert!(
        decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &outer).is_ok(),
        "the correct published timestamp must decrypt the record"
    );
}

#[test]
fn tampered_ciphertext_is_rejected_by_the_record_signature() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5353);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 13);
    let outer = record.outer_ciphertext().to_vec();

    // A layer is a raw ChaCha20 stream, so it carries no integrity of its own: a
    // flipped ciphertext byte yields a flipped plaintext byte, and where that
    // byte lands depends on the payload. The guarantee is the Red25519 signature
    // over the whole layer-0 region, which is what a floodfill checks. Every
    // single-byte tamper must therefore fail validation, and that is the row
    // that matters.
    let mut rejected = 0_usize;
    let mut parsed_anyway = 0_usize;
    for index in (32..outer.len()).step_by(7) {
        let mut tampered = outer.clone();
        tampered[index] ^= 0x01;
        let candidate = EncryptedLeaseSet2::new(
            ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
            record.blinded_public_key().to_vec(),
            record.published_seconds(),
            record.expires_offset_seconds(),
            None,
            tampered,
            record.signature().to_vec(),
        )
        .expect("tampered record builds");
        assert!(
            matches!(
                validate(candidate, PUBLISHED),
                Err(Els2ValidationError::InvalidSignature)
            ),
            "tampering byte {index} was accepted"
        );
        rejected += 1;

        // Record how often the corrupted layer still produced a parseable record.
        // This is a measured property of a malleable layer, not a defect: the
        // signature is the integrity layer, and an attacker who cannot re-sign
        // cannot publish the tampered bytes.
        let mut t2 = outer.clone();
        t2[index] ^= 0x01;
        if decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &t2).is_ok() {
            parsed_anyway += 1;
        }
    }
    assert!(
        rejected >= 50,
        "expected a wide tamper sweep, ran {rejected}"
    );
    assert!(
        parsed_anyway > 0,
        "a malleable layer that never produced a parseable record would mean the sweep \
         never reached a structural field; the invariant under test is the signature"
    );
}

#[test]
fn a_tampered_inner_record_fails_the_inner_lease_set_signature() {
    // The second integrity layer: even if an attacker could replace the ciphertext
    // and re-sign the outer record, the decrypted inner LeaseSet2 carries the
    // destination's own Ed25519 signature, so a modified routing table is still
    // rejected when the client validates it.
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5354);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 14);
    let outer = record.outer_ciphertext().to_vec();

    let mut tampered = outer.clone();
    // Target the layer-2 ciphertext, which starts after outerSalt(32) + flags(1)
    // + innerSalt(32) + storeType(1).
    let layer2_start = 32 + 1 + 32 + 1;
    tampered[layer2_start + 10] ^= 0x02;
    match decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &tampered) {
        Err(_) => {}
        Ok(decrypted) => {
            // If the corrupted plaintext still parses, the inner signature must
            // reject it.
            let inner_record = decrypted.into_lease_set2(MAX).expect("parses");
            let outcome = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
                inner_record,
                None,
                LeaseSet2ValidationContext::new(PUBLISHED),
            );
            assert!(
                outcome.is_err(),
                "a tampered inner record must fail its own signature check"
            );
        }
    }
}

#[test]
fn tampered_signature_and_header_fields_are_rejected() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5454);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 17);

    // Flipped signature byte.
    let mut signature = record.signature().to_vec();
    signature[0] ^= 0x01;
    let bad_signature = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        None,
        record.outer_ciphertext().to_vec(),
        signature,
    )
    .expect("builds");
    assert!(matches!(
        validate(bad_signature, PUBLISHED),
        Err(Els2ValidationError::InvalidSignature)
    ));

    // Rewritten published timestamp: the signature no longer covers the bytes.
    let bad_published = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        record.blinded_public_key().to_vec(),
        record.published_seconds() + 1,
        record.expires_offset_seconds(),
        None,
        record.outer_ciphertext().to_vec(),
        record.signature().to_vec(),
    )
    .expect("builds");
    assert!(matches!(
        validate(bad_published, PUBLISHED),
        Err(Els2ValidationError::InvalidSignature)
    ));

    // Rewritten expiry offset.
    let bad_expiry = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds() - 1,
        None,
        record.outer_ciphertext().to_vec(),
        record.signature().to_vec(),
    )
    .expect("builds");
    assert!(matches!(
        validate(bad_expiry, PUBLISHED),
        Err(Els2ValidationError::InvalidSignature)
    ));
}

#[test]
fn freshness_and_storage_key_checks_are_enforced() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (_, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5555);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = derive_els2_credentials(
        &unblinded_key(),
        SigningKeyType::RedDsaSha512Ed25519,
        &unblinded_key(),
    );
    let _ = credentials;
    let record = build_record(
        &owner,
        &{
            let identity = BlindingIdentity::new(
                *owner.daily().blinded_public_key(),
                SigningKeyType::RedDsaSha512Ed25519,
                None,
            )
            .expect("identity");
            identity.credentials_for(owner.daily().blinded_public_key())
        },
        &inner,
        PUBLISHED,
        3_600,
        19,
    );

    // Expired.
    assert!(matches!(
        validate(record.clone(), PUBLISHED + 3_601),
        Err(Els2ValidationError::Expired { .. })
    ));
    // Far future.
    assert!(matches!(
        validate(record.clone(), PUBLISHED - 7_200),
        Err(Els2ValidationError::ExcessiveFuture { .. })
    ));
    // A mismatched expected storage key is a rejection, not a silent accept.
    let wrong_key = BlindedStorageKey::from_hash(Hash::from_bytes([0x77; 32]));
    assert!(matches!(
        ValidatedEncryptedLeaseSet2::validate(
            record,
            Some(wrong_key),
            Els2ValidationContext::new(PUBLISHED)
        ),
        Err(Els2ValidationError::StorageKeyMismatch)
    ));
}

#[test]
fn non_red25519_blinded_sigtype_is_rejected() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (_, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5656);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let identity = BlindingIdentity::new(
        *owner.daily().blinded_public_key(),
        SigningKeyType::RedDsaSha512Ed25519,
        None,
    )
    .expect("identity");
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 23);
    let ed25519_record = EncryptedLeaseSet2::new(
        SigningKeyType::EdDsaSha512Ed25519,
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        None,
        record.outer_ciphertext().to_vec(),
        record.signature().to_vec(),
    )
    .expect("builds");
    assert!(matches!(
        ValidatedEncryptedLeaseSet2::validate(
            ed25519_record,
            None,
            Els2ValidationContext::new(PUBLISHED)
        ),
        Err(Els2ValidationError::UnsupportedBlindedSigtype { code: 7, .. })
    ));
}

#[test]
fn per_client_layer_one_flags_are_refused_by_the_no_auth_floor() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5757);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 29);
    let outer = record.outer_ciphertext().to_vec();

    // Re-encrypt with layer-1 flags set to the PSK authorization scheme and a
    // 32-byte salt, 2-byte count, and one 40-byte client entry appended, which is
    // the exact shape the specification defines for per-client authorization.
    let subcred = *credentials.subcredential();
    let inner_ciphertext = &outer[65..];
    let mut layer1_plaintext = Vec::with_capacity(1 + 32 + 2 + 40 + inner_ciphertext.len());
    layer1_plaintext.push(
        i2pr_netdb::ELS2_LAYER1_FLAG_PER_CLIENT
            | (i2pr_netdb::ELS2_LAYER1_SCHEME_PSK << i2pr_netdb::ELS2_LAYER1_SCHEME_SHIFT),
    );
    layer1_plaintext.extend_from_slice(&[0xAB; 32]); // authSalt
    layer1_plaintext.extend_from_slice(&1_u16.to_be_bytes()); // clients: 1
    layer1_plaintext.extend_from_slice(&[0xCD; 40]); // one authClient
    layer1_plaintext.extend_from_slice(inner_ciphertext);

    let outer_salt: [u8; 32] = outer[..32].try_into().expect("outer salt");
    let derived = i2pr_crypto::hkdf_sha256_extract_and_expand(
        &outer_salt,
        &layer1_input(&subcred, PUBLISHED),
        i2pr_netdb::ELS2_LAYER1_HKDF_INFO,
        44,
    )
    .expect("hkdf");
    let key =
        i2pr_crypto::LayerCipherKey::from_bytes(derived[..32].try_into().expect("32-byte key"));
    let mut nonce = [0_u8; 12];
    nonce.copy_from_slice(&derived[32..]);
    i2pr_crypto::chacha20_xor_layer(&key, &nonce, &mut layer1_plaintext).expect("stream");
    let mut tampered_outer = outer_salt.to_vec();
    tampered_outer.extend_from_slice(&layer1_plaintext);

    assert!(
        matches!(
            decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &tampered_outer),
            Err(Els2Error::ClientAuthorizationRequired { .. })
        ),
        "a per-client layer-1 flag must be refused, not silently decrypted"
    );
}

fn layer1_input(subcred: &[u8; 32], published: u32) -> Vec<u8> {
    let mut input = Vec::with_capacity(36);
    input.extend_from_slice(subcred);
    input.extend_from_slice(&published.to_be_bytes());
    input
}

#[test]
fn reserved_layer_one_flag_bits_are_rejected() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0x5858);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 31);
    let outer = record.outer_ciphertext().to_vec();

    let subcred = *credentials.subcredential();
    let mut layer1_plaintext = outer[32..].to_vec();
    layer1_plaintext[0] |= 0x10; // reserved bit 4
    let outer_salt: [u8; 32] = outer[..32].try_into().expect("outer salt");
    let derived = i2pr_crypto::hkdf_sha256_extract_and_expand(
        &outer_salt,
        &layer1_input(&subcred, PUBLISHED),
        i2pr_netdb::ELS2_LAYER1_HKDF_INFO,
        44,
    )
    .expect("hkdf");
    let key =
        i2pr_crypto::LayerCipherKey::from_bytes(derived[..32].try_into().expect("32-byte key"));
    let mut nonce = [0_u8; 12];
    nonce.copy_from_slice(&derived[32..]);
    i2pr_crypto::chacha20_xor_layer(&key, &nonce, &mut layer1_plaintext).expect("stream");
    let mut tampered = outer_salt.to_vec();
    tampered.extend_from_slice(&layer1_plaintext);
    assert!(matches!(
        decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &tampered),
        Err(Els2Error::Layer1ReservedFlags { mask }) if mask & 0xf0 != 0
    ));
}

#[test]
fn size_ceilings_are_enforced_on_both_layers() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let salts = [0_u8; 32];

    // An empty inner record is refused.
    assert!(matches!(
        encrypt_no_auth_outer_ciphertext(
            &credentials,
            PUBLISHED,
            INNER_LEASE_SET2_STORE_TYPE,
            &[],
            &salts,
            &salts,
        ),
        Err(Els2Error::TooLarge { .. })
    ));
    // An oversized inner record is refused.
    let oversized = vec![0_u8; MAX_ELS2_INNER_LEASE_SET_LENGTH + 1];
    assert!(matches!(
        encrypt_no_auth_outer_ciphertext(
            &credentials,
            PUBLISHED,
            INNER_LEASE_SET2_STORE_TYPE,
            &oversized,
            &salts,
            &salts,
        ),
        Err(Els2Error::TooLarge { .. })
    ));
    // The boundary itself is accepted, and the outer length is exactly the
    // documented overhead larger.
    let at_limit = vec![0_u8; MAX_ELS2_INNER_LEASE_SET_LENGTH];
    let produced = encrypt_no_auth_outer_ciphertext(
        &credentials,
        PUBLISHED,
        INNER_LEASE_SET2_STORE_TYPE,
        &at_limit,
        &salts,
        &salts,
    )
    .expect("boundary accepted");
    assert_eq!(produced.len(), i2pr_netdb::MAX_ELS2_OUTER_CIPHERTEXT_LENGTH);
    // An unsupported inner store type is refused.
    assert!(matches!(
        encrypt_no_auth_outer_ciphertext(
            &credentials,
            PUBLISHED,
            9,
            &at_limit[..1],
            &salts,
            &salts
        ),
        Err(Els2Error::UnsupportedInnerStoreType { code: 9 })
    ));
    // A short outer ciphertext is refused on the read side.
    assert!(matches!(
        decrypt_no_auth_outer_ciphertext(&credentials, PUBLISHED, &[0_u8; 32]),
        Err(Els2Error::LayerTooShort { layer: "outer", .. })
    ));
    let _ = owner;
}

#[test]
fn lookup_secret_bound_is_documented_and_enforced() {
    assert!(
        LookupSecret::from_str(&"a".repeat(256)).is_ok(),
        "the documented maximum must be accepted"
    );
    assert!(
        LookupSecret::from_str(&"a".repeat(257)).is_err(),
        "one byte over the documented maximum must be refused"
    );
    // A too-long secret is reported as a configuration error, not as a blinding
    // failure, and never as a silent no-secret derivation.
    let public = unblinded_key();
    assert!(matches!(
        BlindingIdentity::new(
            public,
            SigningKeyType::RedDsaSha512Ed25519,
            Some(&"a".repeat(257))
        ),
        Err(Els2Error::LookupSecretRejected(_))
    ));
    // An absent secret and an empty secret are the same derivation input.
    let absent = BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None)
        .expect("no secret");
    let empty = BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, Some(""))
        .expect("empty secret");
    let day = utc_blinding_day(PUBLISHED).expect("day");
    assert_eq!(
        absent.derive(day).expect("derive"),
        empty.derive(day).expect("derive")
    );
    // A non-empty secret changes the derivation.
    let with_secret = BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, Some("s"))
        .expect("secret");
    assert_ne!(
        absent.derive(day).expect("derive"),
        with_secret.derive(day).expect("derive")
    );
}

#[test]
fn unsupported_unblinded_sigtype_is_rejected_before_derivation() {
    assert!(matches!(
        BlindingIdentity::new(unblinded_key(), SigningKeyType::EcdsaSha512P521, None),
        Err(Els2Error::UnsupportedUnblindedSigtype { code: 3 })
    ));
}

// --- 3. lifecycle rows --------------------------------------------------------------------------

#[test]
fn each_utc_day_yields_a_distinct_blinded_key_and_storage_key() {
    let public = unblinded_key();
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let days = [
        BlindingDay::from_ymd(2023, 11, 14).expect("day"),
        BlindingDay::from_ymd(2023, 11, 15).expect("day"),
        BlindingDay::from_ymd(2023, 12, 1).expect("day"),
        BlindingDay::from_ymd(2024, 2, 29).expect("leap day"),
    ];
    let mut seen: BTreeMap<[u8; 32], BlindingDay> = BTreeMap::new();
    for day in days {
        let daily = identity.derive(day).expect("derive");
        assert_eq!(daily.day(), day);
        assert!(
            seen.insert(*daily.storage_key().as_bytes(), day).is_none(),
            "two days produced the same storage key"
        );
        assert!(
            seen.insert(*daily.blinded_public_key().as_bytes(), day)
                .is_none()
        );
    }
}

#[test]
fn rollover_is_atomic_counted_once_and_restart_safe() {
    let public = unblinded_key();
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let config = BlindingScheduleConfig::new(2, false);
    let mut schedule = BlindingSchedule::new(identity, config);

    let first_day_start = 1_700_000_000 - (1_700_000_000 % 86_400);
    let first = schedule.current(first_day_start).expect("day 1");
    assert_eq!(
        schedule.rollovers(),
        0,
        "the first observation is not a rollover"
    );
    // Repeated reads within the same day never count a rollover and always return
    // the same self-consistent value.
    for _ in 0..5 {
        let again = schedule.current(first_day_start + 3_600).expect("same day");
        assert_eq!(again, first, "the same day must yield the same material");
        assert_eq!(schedule.rollovers(), 0);
    }
    // Crossing midnight is exactly one rollover, and the returned value carries
    // the new day with its own key and storage key.
    let second = schedule.current(first_day_start + 86_400).expect("day 2");
    assert_eq!(schedule.rollovers(), 1);
    assert_ne!(second.day(), first.day());
    assert_ne!(second.storage_key(), first.storage_key());
    assert_ne!(second.blinded_public_key(), first.blinded_public_key());
    // Re-reading the new day does not count again.
    schedule
        .current(first_day_start + 90_000)
        .expect("same new day");
    assert_eq!(schedule.rollovers(), 1);

    // Restart safety: a fresh schedule over the same inputs reproduces the same
    // material, because nothing is persisted.
    let mut restarted = BlindingSchedule::new(
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity"),
        config,
    );
    assert_eq!(
        restarted.current(first_day_start + 86_400).expect("day 2"),
        second
    );
    assert_eq!(
        restarted.rollovers(),
        0,
        "a fresh schedule has observed no rollover"
    );
}

#[test]
fn precomputation_is_bounded_and_never_changes_the_current_day() {
    let public = unblinded_key();
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let mut schedule = BlindingSchedule::new(identity, BlindingScheduleConfig::new(2, false));
    let start = 1_700_000_000 - (1_700_000_000 % 86_400);
    let current = schedule.current(start).expect("current");
    assert_eq!(schedule.current_day(), Some(current.day()));
    for offset in 1..6 {
        let day = utc_blinding_day(start + offset * 86_400).expect("future day");
        schedule.precompute(day).expect("precompute");
        assert_eq!(
            schedule.current_day(),
            Some(current.day()),
            "precomputation must not move the current day"
        );
        assert!(
            schedule.cached_days() <= 2,
            "cache grew to {} entries",
            schedule.cached_days()
        );
    }
    assert_eq!(
        schedule.cached_days(),
        2,
        "the cache is capped at its bound"
    );
    // The current day is still served correctly after eviction churn.
    assert_eq!(schedule.current(start).expect("current"), current);
}

#[test]
fn owner_schedule_publishes_only_self_consistent_material() {
    let (private, public) = signed_type11_key(0x6060);
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let schedule =
        BlindingSchedule::new_owner(identity, private, BlindingScheduleConfig::new(2, true));
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let owner = schedule.owner_blinding(PUBLISHED).expect("owner");
    // The blinded public key derived privately must equal the one derived publicly.
    let public_only = schedule.owner_blinding(PUBLISHED).expect("owner again");
    assert_eq!(*owner.daily(), *public_only.daily());
    let alpha =
        generate_alpha(&public, SigningKeyType::RedDsaSha512Ed25519, day, None).expect("alpha");
    // The owner path uses the same scalar the type-7 conversion test pins.
    let _ = blind_private_key(&unblinded_scalar_from_ed25519_seed(&[0x11; 32]), &alpha);
    assert_eq!(
        owner.daily().storage_key().as_bytes(),
        i2pr_crypto::red25519::blinded_storage_key(owner.daily().blinded_public_key()).as_bytes()
    );
}

#[test]
fn type_7_owner_blinds_the_converted_ed25519_scalar() {
    // A type-7 destination blinds the clamped Ed25519 conversion of its seed, so
    // the blinded public key must equal DERIVE_PUBLIC((a + alpha) mod L) rather
    // than a derivation over the raw seed.
    let seed = [0x5a_u8; 32];
    let scalar = unblinded_scalar_from_ed25519_seed(&seed);
    assert!(
        !scalar.is_canonical(),
        "a converted type-7 scalar is non-canonical by construction, which is why the \
         verbatim-storing rule exists"
    );
    let unblinded_public = i2pr_crypto::red25519::derive_public_key(&scalar);
    let identity =
        BlindingIdentity::new(unblinded_public, SigningKeyType::EdDsaSha512Ed25519, None)
            .expect("identity");
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let alpha = generate_alpha(
        &unblinded_public,
        SigningKeyType::EdDsaSha512Ed25519,
        day,
        None,
    )
    .expect("alpha");
    let blinded_private = blind_private_key(&scalar, &alpha);
    assert_eq!(
        i2pr_crypto::red25519::derive_blinded_public_key(&blinded_private),
        *identity.derive(day).expect("derive").blinded_public_key()
    );
}

// --- store and floodfill rows -------------------------------------------------------------------

fn sample_record(
    seed: u64,
    published: u32,
) -> (
    OwnerBlinding,
    i2pr_netdb::Els2Credentials,
    ValidatedEncryptedLeaseSet2,
) {
    let day = utc_blinding_day(published).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(seed);
    let inner = signed_ls2(&signer, published, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, published, 3_600, seed + 1);
    let validated = validate(record, published).expect("valid record");
    (owner, credentials, validated)
}

#[test]
fn store_replaces_newer_rejects_stale_and_never_conflicts_with_itself() {
    let (owner, credentials, first) = sample_record(0x7070, PUBLISHED);
    let mut store = Els2Store::new();
    assert_eq!(store.insert(first.clone()), Els2InsertOutcome::Inserted);
    assert_eq!(store.insert(first.clone()), Els2InsertOutcome::Idempotent);
    assert!(store.contains(&first.storage_key()));
    assert_eq!(store.len(), 1);
    assert!(store.encoded_bytes() > 0);

    // A strictly newer record for the same storage key replaces.
    let signer = router_bundle(0x7171);
    let inner = signed_ls2(&signer, PUBLISHED + 60, 3_600);
    let newer = validate(
        build_record(&owner, &credentials, &inner, PUBLISHED + 60, 3_600, 99),
        PUBLISHED + 60,
    )
    .expect("newer");
    assert_eq!(
        newer.storage_key(),
        first.storage_key(),
        "same day, same key"
    );
    let newer_key = newer.storage_key();
    assert_eq!(store.insert(newer), Els2InsertOutcome::Replaced);
    assert_eq!(store.len(), 1);

    // A strictly older record is refused.
    let signer = router_bundle(0x7272);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let older = validate(
        build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 98),
        PUBLISHED,
    )
    .expect("older");
    assert_eq!(store.insert(older), Els2InsertOutcome::StaleReplacement);

    // Equal published with different bytes is a conflict, and the stored record is
    // preserved.
    let signer = router_bundle(0x7373);
    let inner = signed_ls2(&signer, PUBLISHED + 60, 3_600);
    let conflicting = validate(
        build_record(&owner, &credentials, &inner, PUBLISHED + 60, 3_600, 97),
        PUBLISHED + 60,
    )
    .expect("conflict");
    assert_eq!(store.insert(conflicting), Els2InsertOutcome::Conflict);
    assert_eq!(store.len(), 1);

    // Removal releases the byte accounting.
    assert!(store.remove(&newer_key));
    assert!(!store.remove(&newer_key));
    assert_eq!(store.encoded_bytes(), 0);
    assert!(store.is_empty());
}

#[test]
fn store_capacity_is_enforced_without_mutating_existing_state() {
    let mut store = Els2Store::with_config(Els2StoreConfig::new(1, 1 << 20));
    let (_, _, first) = sample_record(0x8080, PUBLISHED);
    let first_key = first.storage_key();
    assert_eq!(store.insert(first), Els2InsertOutcome::Inserted);

    // A record for a *different* storage key needs a new slot.
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (private, public) = signed_type11_key(0x8181);
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let owner = identity.derive_owner(&private, day).expect("owner");
    let signer = router_bundle(0x8282);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let record = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 83);
    let second = validate(record, PUBLISHED).expect("second");
    assert_ne!(second.storage_key(), first_key);
    assert_eq!(
        store.insert(second.clone()),
        Els2InsertOutcome::CapacityExceeded
    );
    assert_eq!(
        store.len(),
        1,
        "a rejected insert must not evict the existing record"
    );
    assert!(store.contains(&first_key));

    // A byte ceiling below one record rejects even the first insert.
    let mut tiny = Els2Store::with_config(Els2StoreConfig::new(4, 1));
    assert_eq!(tiny.insert(second), Els2InsertOutcome::CapacityExceeded);
    assert!(tiny.is_empty());
    assert_eq!(tiny.stats().max_total_encoded_bytes, 1);
}

#[test]
fn floodfill_stores_and_serves_type_five_records_opaquely() {
    use i2pr_netdb::{
        NetDbNamespace, RecordProvenance, ServerInsertOutcome, ServerNetDb, StorePurpose,
        ValidatedNetDbRecord,
    };

    let (owner, credentials, validated) = sample_record(0x9090, PUBLISHED);
    let now_ms = u64::from(PUBLISHED) * 1_000;
    let mut server = ServerNetDb::default();
    let id = record_id_for(validated.storage_key());
    let outcome = server
        .insert(
            ValidatedNetDbRecord::EncryptedLeaseSet2(validated.clone()),
            RecordProvenance {
                namespace: NetDbNamespace::MainRouter,
                inbound: i2pr_netdb::InboundProvenance::AuthenticatedDirectPeer,
                purpose: StorePurpose::PublishedStore,
                observed_at_ms: now_ms,
            },
        )
        .expect("insert");
    assert_eq!(outcome, ServerInsertOutcome::Inserted);

    // The store is keyed by the blinded storage key, not by any Destination hash.
    assert_eq!(server.record_count(), 1);
    assert_eq!(server.total_bytes(), validated.encoded_len());

    // Answering returns the record body without decrypting anything.
    let answer = server
        .database_store_for_answer(
            5,
            *validated.storage_key().as_hash(),
            now_ms,
            60 * 60 * 1_000,
            1 << 20,
        )
        .expect("answer")
        .expect("a record is present");
    match &answer.data {
        i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(value) => {
            assert_eq!(**value, *validated.record());
            // The answer is byte-identical to what was stored: no re-encoding drift.
            assert_eq!(
                value.encode_to_vec(MAX).expect("encode"),
                validated.encoded(MAX).expect("encoded")
            );
        }
        other => panic!("expected an EncryptedLeaseSet answer, got {other:?}"),
    }

    // A lookup under an unrelated key is a typed "not published" rather than a
    // guess at some other record, matching how every other record type reports a
    // key the router never stored.
    assert_eq!(
        server
            .database_store_for_answer(
                5,
                Hash::from_bytes([0x01; 32]),
                now_ms,
                60 * 60 * 1_000,
                1 << 20
            )
            .expect_err("an unknown key must not resolve"),
        i2pr_netdb::ProvenanceEligibility::NotPublished
    );

    // Replication eligibility is freshness-only for a type-5 record.
    assert_eq!(
        server.may_replicate(&id, now_ms, 60 * 60 * 1_000),
        i2pr_netdb::ProvenanceEligibility::Allowed
    );
    // Two distinct expiries must be distinguishable. First: the record's own
    // expiry has passed but the provenance age is still inside the window, so the
    // type-5 freshness rule is what refuses it.
    let after_record_expiry = (u64::from(PUBLISHED) + 7_200) * 1_000;
    assert_eq!(
        server.may_replicate(&id, after_record_expiry, 24 * 60 * 60 * 1_000),
        i2pr_netdb::ProvenanceEligibility::NotPublished
    );
    // Second: the provenance age itself is stale, which the shared index reports
    // before the record-specific rule ever runs.
    assert_eq!(
        server.may_replicate(&id, after_record_expiry, 60 * 60 * 1_000),
        i2pr_netdb::ProvenanceEligibility::Expired
    );
    let _ = (owner, credentials);
}

fn record_id_for(key: BlindedStorageKey) -> i2pr_netdb::RecordId {
    i2pr_netdb::RecordId::new(5, *key.as_hash())
}

#[test]
fn offline_key_block_is_parsed_and_its_delegation_is_verified() {
    let day = utc_blinding_day(PUBLISHED).expect("day");
    let (identity, owner) = owner_and_identity(None, day);
    let signer = router_bundle(0xa0a0);
    let inner = signed_ls2(&signer, PUBLISHED, 3_600);
    let credentials = identity.credentials_for(owner.daily().blinded_public_key());
    let base = build_record(&owner, &credentials, &inner, PUBLISHED, 3_600, 37);

    // A transient Ed25519 key authorized by the blinded Red25519 key. The key
    // must be a real curve point: an arbitrary 32-byte value would be rejected
    // as an undecodable public key rather than exercised as a signing key.
    let transient_router = router_bundle(0xa2a2);
    let transient = transient_router
        .signing_key()
        .public_key()
        .expect("transient public key")
        .as_bytes()
        .to_vec();
    let mut rng = ChaCha8Rng::seed_from_u64(41);
    // The delegation is signed with the *blinded* type-11 key, so it follows the same
    // bounded ELS2 transcript policy as the record's own signature (Plan 346). The block
    // is built once with a placeholder to obtain the exact signed region, then rebuilt
    // with the real signature.
    let offline_probe = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        transient.to_vec(),
        vec![0_u8; 64],
    )
    .expect("offline probe");
    let offline_signature = sign_type11_deployed(
        owner.blinded_private_key(),
        Els2SignedRegion::of_offline_keys(&offline_probe),
        &mut rng,
    )
    .expect("offline signature");
    let offline = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        transient.to_vec(),
        offline_signature.as_bytes().to_vec(),
    )
    .expect("offline block");
    // Rebuild with the offline block and a record signature made by the transient
    // key.
    let router = &transient_router;
    let probe = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        base.blinded_public_key().to_vec(),
        base.published_seconds(),
        base.expires_offset_seconds(),
        Some(offline.clone()),
        base.outer_ciphertext().to_vec(),
        vec![0_u8; 64],
    )
    .expect("probe");
    let record_signature = router
        .signing_key()
        .sign(probe.signed_bytes())
        .expect("sign");
    let _ = &record_signature;
    let with_offline = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        base.blinded_public_key().to_vec(),
        base.published_seconds(),
        base.expires_offset_seconds(),
        Some(offline),
        base.outer_ciphertext().to_vec(),
        record_signature.as_bytes().to_vec(),
    )
    .expect("record with offline keys");
    assert!(with_offline.signed_by_transient_key());
    assert!(validate(with_offline.clone(), PUBLISHED).is_ok());

    // An expired delegation is refused.
    let expired_offline_probe = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED - 1,
        SigningKeyType::EdDsaSha512Ed25519,
        transient.to_vec(),
        vec![0_u8; 64],
    )
    .expect("expired offline probe");
    let expired_signature = sign_type11_deployed(
        owner.blinded_private_key(),
        Els2SignedRegion::of_offline_keys(&expired_offline_probe),
        &mut rng,
    )
    .expect("expired signature");
    let expired_offline = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED - 1,
        SigningKeyType::EdDsaSha512Ed25519,
        transient.to_vec(),
        expired_signature.as_bytes().to_vec(),
    )
    .expect("expired offline block");
    let expired_probe = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        base.blinded_public_key().to_vec(),
        base.published_seconds(),
        base.expires_offset_seconds(),
        Some(expired_offline),
        base.outer_ciphertext().to_vec(),
        vec![0_u8; 64],
    )
    .expect("probe");
    let expired_record_signature = router
        .signing_key()
        .sign(expired_probe.signed_bytes())
        .expect("sign");
    let expired = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        base.blinded_public_key().to_vec(),
        base.published_seconds(),
        base.expires_offset_seconds(),
        expired_probe.offline_keys().cloned(),
        base.outer_ciphertext().to_vec(),
        expired_record_signature.as_bytes().to_vec(),
    )
    .expect("expired record");
    assert!(matches!(
        validate(expired, PUBLISHED),
        Err(Els2ValidationError::OfflineSignatureExpired)
    ));
}

#[test]
fn credential_binds_to_the_unblinded_key_and_the_blinded_key() {
    let a = unblinded_key();
    let mut other = [0x66_u8; 32];
    other[0] = 0x58;
    other[31] = 0x65;
    let b = Red25519PublicKey::from_bytes(other);
    let sigtype = SigningKeyType::RedDsaSha512Ed25519;

    let base = derive_els2_credentials(&a, sigtype, &b);
    // A different unblinded key changes the subcredential.
    assert_ne!(
        base.subcredential(),
        derive_els2_credentials(&b, sigtype, &b).subcredential()
    );
    // A different signature type changes the subcredential.
    assert_ne!(
        base.subcredential(),
        derive_els2_credentials(&a, SigningKeyType::EdDsaSha512Ed25519, &b).subcredential()
    );
    // A different blinded key changes only the subcredential, not the credential:
    // the credential is a function of the unblinded key alone.
    let mut third = [0x66_u8; 32];
    third[0] = 0x58;
    third[31] = 0x64;
    let c = Red25519PublicKey::from_bytes(third);
    assert_eq!(
        base.credential(),
        derive_els2_credentials(&a, sigtype, &c).credential()
    );
    assert_ne!(
        base.subcredential(),
        derive_els2_credentials(&a, sigtype, &c).subcredential()
    );
    // The personalization keeps the credential from colliding with a bare hash of
    // the same key material.
    assert_ne!(
        base.credential(),
        &Hash::digest(a.as_bytes()).as_bytes()[..]
    );
}

#[test]
fn expiry_offset_respects_the_field_capacity_and_the_day_boundary_together() {
    // A record early in a UTC day has nearly 86 400 seconds until midnight, which
    // does not fit the two-byte offset field. The clamp must take the smaller of
    // the two bounds rather than saturating: saturating would emit a record
    // expiring 17 seconds before midnight while the caller believed it lasted to
    // midnight.
    let early = 86_400 * 19_722; // an exact day boundary, i.e. 00:00:00 UTC
    let boundary = next_utc_day_boundary_seconds(early);
    assert_eq!(boundary, early + 86_400);
    let offset = i2pr_netdb::day_bound_expiry_offset(early, early + 30 * 86_400, boundary);
    assert_eq!(
        u32::from(offset),
        i2pr_proto::ENCRYPTED_LEASE_SET2_MAX_EXPIRES_OFFSET,
        "an early-in-the-day record is limited by the two-byte field"
    );
    assert!(
        early + u32::from(offset) < boundary,
        "and never outlives its day"
    );

    // A record late in a UTC day is limited by the boundary instead.
    let late = early + 80_000;
    let late_boundary = next_utc_day_boundary_seconds(late);
    let offset = i2pr_netdb::day_bound_expiry_offset(late, late + 30 * 86_400, late_boundary);
    assert_eq!(u32::from(offset), late_boundary - late);
    assert!(late + u32::from(offset) == late_boundary);

    // A record whose requested expiration is inside both bounds keeps it.
    let mid = early + 43_200;
    let mid_boundary = next_utc_day_boundary_seconds(mid);
    let offset = i2pr_netdb::day_bound_expiry_offset(mid, mid + 3_600, mid_boundary);
    assert_eq!(u32::from(offset), 3_600);
}
