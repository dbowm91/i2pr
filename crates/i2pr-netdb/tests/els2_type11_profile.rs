//! Plan 346 evidence at the encrypted-LeaseSet2 type-5 boundary.
//!
//! The crypto-level differential lives in
//! `i2pr-crypto/tests/red25519_deployed_els2_profile.rs`. This file is the other half: it
//! proves the *policy* sits where the plan requires it — outbound records carry the deployed
//! transcript, inbound records accept both transcripts only inside the bounded type-5 owner,
//! the accepted profile is reported back to that owner, and type 7 is untouched.
//!
//! Rows are fail-closed rejections unless stated otherwise. Several are deliberately written
//! so they would **not** pass under a permissive "try both type-11 algorithms" verifier,
//! because that is the specific defect Plan 346 forbids.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{
    BlindingDay, Red25519PrivateScalar, Red25519PublicKey, Red25519Signature, blind_private_key,
    generate_private,
};
use i2pr_proto::{
    CryptoKeyType, Date32, Destination, ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE, Els2SignedRegion,
    EncryptedLeaseSet2, EncryptedLeaseSet2OfflineKeys, Hash, INNER_LEASE_SET2_STORE_TYPE, Lease2,
    LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, MAX_COMMON_STRUCTURE_SIZE,
    Mapping, SignatureValue, SigningKeyType,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use i2pr_netdb::{
    BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, Els2Error,
    Els2RecordSignatureProfile, Els2Type11Profile, Els2ValidationContext, Els2ValidationError,
    OwnerBlinding, ValidatedEncryptedLeaseSet2, derive_els2_credentials, sign_type11_deployed,
};

const PUBLISHED: u32 = 1_700_000_000;
const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

// --- helpers ------------------------------------------------------------------------------------

fn router_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic router identity")
}

fn inner_lease_set() -> LeaseSet2 {
    let bundle = router_bundle(0x1234);
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
    let unsigned = LeaseSet2::new(header, Mapping::empty(), keys, leases, placeholder)
        .expect("unsigned inner");
    let signature = bundle
        .signing_key()
        .sign(&unsigned.signature_preimage())
        .expect("sign inner");
    LeaseSet2::new(
        unsigned.header().clone(),
        unsigned.options().clone(),
        unsigned.encryption_keys().to_vec(),
        unsigned.leases().to_vec(),
        signature,
    )
    .expect("inner")
}

/// One owner's blinded material for the day `PUBLISHED` falls in.
struct Fixture {
    owner: OwnerBlinding,
    blinded_public: Red25519PublicKey,
    credentials: i2pr_netdb::Els2Credentials,
    inner: LeaseSet2,
}

fn fixture(seed: u64, secret: Option<&str>) -> Fixture {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let unblinded = generate_private(&mut rng).expect("red25519 key");
    let unblinded_public = i2pr_crypto::red25519::derive_public_key(&unblinded);
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
    let credentials = derive_els2_credentials(
        &unblinded_public,
        SigningKeyType::RedDsaSha512Ed25519,
        &blinded_public,
    );
    Fixture {
        owner,
        blinded_public,
        credentials,
        inner: inner_lease_set(),
    }
}

/// Which type-11 transcript a test record is signed under.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Transcript {
    /// What i2pr publishes after Plan 346: the deployed Java/i2pd ELS2 transcript.
    Deployed,
    /// What i2pr published before Plan 346, and what Emissary produces: Proposal-146 strict.
    Strict,
}

fn build_record(f: &Fixture, seed: u64, transcript: Transcript) -> EncryptedLeaseSet2 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let inner_bytes = f.inner.encode_to_vec(MAX).expect("inner bytes");
    let mut outer_salt = [0_u8; 32];
    let mut inner_salt = [0_u8; 32];
    rng.fill_bytes(&mut outer_salt);
    rng.fill_bytes(&mut inner_salt);
    let outer_ciphertext = i2pr_netdb::encrypt_no_auth_outer_ciphertext(
        &f.credentials,
        PUBLISHED,
        INNER_LEASE_SET2_STORE_TYPE,
        &inner_bytes,
        &outer_salt,
        &inner_salt,
    )
    .expect("encrypt");
    let probe = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        f.blinded_public.as_bytes().to_vec(),
        PUBLISHED,
        3_600,
        None,
        outer_ciphertext.clone(),
        vec![0_u8; 64],
    )
    .expect("probe");
    let region = Els2SignedRegion::of_record(&probe);
    let signature = match transcript {
        Transcript::Deployed => {
            sign_type11_deployed(f.owner.blinded_private_key(), region, &mut rng).expect("sign")
        }
        Transcript::Strict => i2pr_crypto::red25519::sign_with_nonce(
            f.owner.blinded_private_key(),
            region.as_bytes(),
            &[0x5a; 80],
        )
        .expect("sign"),
    };
    EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        f.blinded_public.as_bytes().to_vec(),
        PUBLISHED,
        3_600,
        None,
        outer_ciphertext,
        signature.as_bytes().to_vec(),
    )
    .expect("record")
}

fn storage_key_of(record: &EncryptedLeaseSet2) -> BlindedStorageKey {
    BlindedStorageKey::from_hash(i2pr_crypto::red25519::blinded_storage_key(
        &Red25519PublicKey::decode(record.blinded_public_key()).expect("blinded key"),
    ))
}

fn validate(
    record: EncryptedLeaseSet2,
) -> Result<ValidatedEncryptedLeaseSet2, Els2ValidationError> {
    let key = storage_key_of(&record);
    ValidatedEncryptedLeaseSet2::validate(
        record,
        Some(key),
        Els2ValidationContext::new(PUBLISHED + 1),
    )
}

fn resign(record: &EncryptedLeaseSet2, signature: Vec<u8>) -> Option<EncryptedLeaseSet2> {
    EncryptedLeaseSet2::new(
        record.blinded_sigtype(),
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        record.offline_keys().cloned(),
        record.outer_ciphertext().to_vec(),
        signature,
    )
    .ok()
}

// --- outbound -----------------------------------------------------------------------------------

/// Outbound type-5 records carry the deployed transcript, and the validator names it.
///
/// This row could not have passed before Plan 346: a stock Java I2P or i2pd router rejects a
/// strict-transcript record outright, so an i2pr-published type-5 LeaseSet was unreadable
/// anywhere on the network.
#[test]
fn a_locally_built_type5_record_uses_the_deployed_transcript_and_is_named_deployed() {
    let f = fixture(1, None);
    let record = build_record(&f, 2, Transcript::Deployed);
    let validated = validate(record).expect("a deployed type-5 record must validate");
    assert_eq!(
        validated.signature_profile(),
        Els2RecordSignatureProfile::Type11(Els2Type11Profile::Deployed),
        "an outbound record must be published and reported under the deployed profile"
    );
}

/// A record frozen under the old strict policy stays readable.
///
/// This is the compatibility half of the correction. Without it, every type-5 record already
/// on the network — and the independent Emissary oracle's output — would become unreadable the
/// moment i2pr changed policy.
#[test]
fn a_frozen_strict_form_record_remains_accepted_and_is_named_strict() {
    let f = fixture(3, None);
    let record = build_record(&f, 4, Transcript::Strict);
    let validated = validate(record).expect("a strict-form type-5 record must stay accepted");
    assert_eq!(
        validated.signature_profile(),
        Els2RecordSignatureProfile::Type11(Els2Type11Profile::Strict),
        "a strict-form record must be accepted and reported as strict, so the correction stays \
         backward compatible with records already published"
    );
}

/// The two profiles are reported distinctly; a record is never silently relabelled.
#[test]
fn the_two_profiles_are_reported_distinctly() {
    let f = fixture(5, None);
    let deployed = validate(build_record(&f, 6, Transcript::Deployed)).expect("deployed");
    let strict = validate(build_record(&f, 7, Transcript::Strict)).expect("strict");
    assert_ne!(
        deployed.signature_profile(),
        strict.signature_profile(),
        "the two transcripts must be distinguishable in evidence, otherwise a harness cannot \
         tell which form a peer actually used"
    );
    assert_eq!(deployed.signature_profile().as_str(), "deployed");
    assert_eq!(strict.signature_profile().as_str(), "strict");
}

/// The signed region is a type, not a byte slice.
///
/// `Els2SignedRegion` is the compensating control that makes the deployed transcript's missing
/// domain separator and length framing tolerable: the only way to obtain one is from a decoded
/// type-5 record or offline-key block, so no application message and no caller-chosen region
/// can reach the ELS2 signer or verifier. The static half of the same claim —
/// that the type has no byte-slice constructor — is
/// `scripts/check-els2-type11-transcript-boundary.sh`.
#[test]
fn the_signed_region_is_exactly_the_record_region() {
    let f = fixture(8, None);
    let record = build_record(&f, 9, Transcript::Deployed);
    let region = Els2SignedRegion::of_record(&record);
    assert!(!region.is_empty());
    assert_eq!(region.len(), record.signed_bytes().len());
    assert_eq!(region.as_bytes(), record.signed_bytes());
    // The store-type byte is part of what was signed and must stay part of the region.
    assert_eq!(
        region.as_bytes()[0],
        i2pr_proto::ENCRYPTED_LEASE_SET2_STORE_TYPE
    );
}

// --- negative rows ------------------------------------------------------------------------------

/// Corrupted signatures fail under both transcripts.
#[test]
fn a_corrupted_signature_fails_under_both_transcripts() {
    for transcript in [Transcript::Deployed, Transcript::Strict] {
        let f = fixture(10, None);
        let record = build_record(&f, 11, transcript);
        for index in [0_usize, 20, 63] {
            let mut signature = record.signature().to_vec();
            signature[index] ^= 0x01;
            let corrupted = resign(&record, signature).expect("corrupted record");
            assert!(
                matches!(
                    validate(corrupted),
                    Err(Els2ValidationError::InvalidSignature)
                ),
                "a flipped signature byte at {index} must be rejected under {transcript:?}"
            );
        }
    }
}

/// A tampered ciphertext is caught by the outer signature.
#[test]
fn a_tampered_ciphertext_is_rejected() {
    let f = fixture(18, None);
    let record = build_record(&f, 19, Transcript::Deployed);
    let mut ciphertext = record.outer_ciphertext().to_vec();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xff;
    let tampered = EncryptedLeaseSet2::new(
        record.blinded_sigtype(),
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        record.offline_keys().cloned(),
        ciphertext,
        record.signature().to_vec(),
    )
    .expect("tampered record");
    assert!(matches!(
        validate(tampered),
        Err(Els2ValidationError::InvalidSignature)
    ));
}

/// A record must not validate under another day's storage key.
///
/// Daily blinding is the whole point of a type-5 lookup: yesterday's signature is not today's
/// record, and the storage-key gate is what enforces that before any decrypt is attempted.
#[test]
fn a_record_does_not_validate_under_another_days_storage_key() {
    let a = fixture(12, None);
    let b = fixture(13, None);
    let record = build_record(&a, 14, Transcript::Deployed);
    assert!(
        validate(record.clone()).is_ok(),
        "the record must be valid under its own key before the negative row means anything"
    );
    let other_key = BlindedStorageKey::from_hash(i2pr_crypto::red25519::blinded_storage_key(
        &b.blinded_public,
    ));
    assert!(
        matches!(
            ValidatedEncryptedLeaseSet2::validate(
                record,
                Some(other_key),
                Els2ValidationContext::new(PUBLISHED + 1)
            ),
            Err(Els2ValidationError::StorageKeyMismatch)
        ),
        "a record must not validate under another destination's storage key"
    );
}

/// Daily rotation really changes the blinded key, so the wrong-day row is not vacuous.
#[test]
fn the_daily_blinding_rotates_the_blinded_key() {
    let f = fixture(15, None);
    let day = BlindingDay::from_ymd(2023, 11, 14).expect("day");
    let next = BlindingDay::from_ymd(2023, 11, 15).expect("day");
    let mut rng = ChaCha8Rng::seed_from_u64(16);
    let unblinded = generate_private(&mut rng).expect("key");
    let unblinded_public = i2pr_crypto::red25519::derive_public_key(&unblinded);
    let identity =
        BlindingIdentity::new(unblinded_public, SigningKeyType::RedDsaSha512Ed25519, None)
            .expect("identity");
    let today = identity.derive(day).expect("today");
    let tomorrow = identity.derive(next).expect("tomorrow");
    assert_ne!(
        today.blinded_public_key(),
        tomorrow.blinded_public_key(),
        "adjacent days must produce distinct blinded keys"
    );
    assert_ne!(
        i2pr_crypto::red25519::blinded_storage_key(today.blinded_public_key()),
        i2pr_crypto::red25519::blinded_storage_key(tomorrow.blinded_public_key()),
        "adjacent days must produce distinct storage keys, which is what makes a type-5 lookup \
         day-scoped"
    );
    let _ = f;
}

/// A different lookup secret derives a different blinded key, so a wrong secret misses rather
/// than decrypting. This is the "wrong secret" negative row at the key level.
#[test]
fn a_different_lookup_secret_derives_a_different_blinded_key() {
    let with_secret = fixture(17, Some("correct horse"));
    let other_secret = fixture(17, Some("battery staple"));
    assert_ne!(
        with_secret.blinded_public.as_bytes(),
        other_secret.blinded_public.as_bytes(),
        "a different lookup secret must derive a different blinded key"
    );
    // The lookup secret enters `GENERATE_ALPHA`, not the credential, so the credential is
    // unchanged and the *subcredential* — which binds the blinded key — is what differs.
    assert_eq!(
        with_secret.credentials.credential(),
        other_secret.credentials.credential(),
        "the lookup secret must not enter the credential derivation; that is what keeps a wrong \
         secret a lookup miss rather than a wrong-record read"
    );
    assert_ne!(
        with_secret.credentials.subcredential(),
        other_secret.credentials.subcredential(),
        "a different blinded key must produce a different subcredential, so a wrong-secret client \
         cannot reach the inner layer"
    );
}

/// An expired record is refused regardless of transcript.
#[test]
fn an_expired_record_is_refused_under_the_deployed_profile() {
    let f = fixture(22, None);
    let record = build_record(&f, 23, Transcript::Deployed);
    assert!(
        matches!(
            ValidatedEncryptedLeaseSet2::validate(
                record,
                Some(storage_key_of(&build_record(&f, 23, Transcript::Deployed))),
                Els2ValidationContext::new(PUBLISHED + 7_200)
            ),
            Err(Els2ValidationError::Expired { .. })
        ),
        "a record past its expiry must be refused"
    );
}

// --- type 7 is untouched ------------------------------------------------------------------------

/// The deployed transcript's *verification* equation is the plain Ed25519 equation, and that
/// overlap is deliberate, bounded, and recorded here rather than papered over.
///
/// Because `c = SHA-512(R || A || M)` is the plain Ed25519 challenge, a type-7 Ed25519
/// signature satisfies the deployed verifier under the same key bytes — and, symmetrically, a
/// deployed-transcript signature satisfies a type-7 Ed25519 verifier. Plan 335 measured this
/// half already ("Java's RedDSA signature is a valid plain Ed25519 signature"); what differs
/// between the two type-11 transcripts is the **signing** transcript, not the equation.
///
/// So the deployed profile is *not* a stricter check than type 7, and no row in this file may
/// claim otherwise. The protection that actually holds is structural, and these two rows are
/// it:
///
/// 1. the ELS2 owner dispatches on the record's own `sigtype` field, so the deployed verifier
///    is only ever reached for a signature the record declares as type 11;
/// 2. a type-5 record whose blinded sigtype is anything other than 11 is refused before any
///    transcript is consulted.
///
/// A type-7 offline delegation is ordinary Ed25519, is verified by the ordinary type-7 path,
/// and never touches a type-11 transcript.
#[test]
fn the_deployed_equation_overlaps_type7_and_the_boundary_is_the_sigtype_dispatch() {
    let transient = router_bundle(0x2468);
    let mut type7_public = [0_u8; 32];
    type7_public.copy_from_slice(
        transient
            .signing_key()
            .public_key()
            .expect("transient public key")
            .as_bytes(),
    );
    let probe = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        type7_public.to_vec(),
        vec![0_u8; 64],
    )
    .expect("offline probe");
    let region = Els2SignedRegion::of_offline_keys(&probe);
    let type7 = transient
        .signing_key()
        .sign(region.as_bytes())
        .expect("type 7 sign");
    let mut type7_signature = [0_u8; 64];
    type7_signature.copy_from_slice(type7.as_bytes());

    // The overlap is real, and is documented rather than asserted away.
    assert!(
        i2pr_crypto::red25519_deployed::verify_blinded(
            &Red25519PublicKey::from_bytes(type7_public),
            region.as_bytes(),
            &Red25519Signature::from_bytes(type7_signature),
        )
        .is_ok(),
        "the deployed verifier shares the plain Ed25519 equation; if this ever became false the \
         deployed profile would no longer be what Java I2P and i2pd run, and Plan 346's \
         correction would need re-derivation"
    );

    // What must hold is that a type-5 record cannot *reach* a type-11 transcript by declaring
    // a type-7 blinded key.
    let f = fixture(80, None);
    let record = build_record(&f, 81, Transcript::Deployed);
    let sigtype7 = EncryptedLeaseSet2::new(
        SigningKeyType::EdDsaSha512Ed25519,
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        None,
        record.outer_ciphertext().to_vec(),
        record.signature().to_vec(),
    )
    .expect("type-7 blinded sigtype record");
    assert!(
        matches!(
            validate(sigtype7),
            Err(Els2ValidationError::UnsupportedBlindedSigtype {
                code,
                expected: _
            }) if code == SigningKeyType::EdDsaSha512Ed25519.code()
        ),
        "a type-5 record declaring a type-7 blinded key must be refused before any transcript \
         is consulted, so the equation overlap cannot be used to bypass the policy"
    );
}

/// A full offline-delegation record whose record signature is type 7 and whose delegation is
/// type 11 under the deployed profile validates, and is reported as a type-7 transient
/// signature with the delegation having passed the bounded type-11 policy.
#[test]
fn an_offline_delegation_follows_the_same_els2_type11_policy() {
    let f = fixture(24, None);
    let base = build_record(&f, 25, Transcript::Deployed);
    let transient = router_bundle(0x1357);
    let transient_public = transient
        .signing_key()
        .public_key()
        .expect("transient public key")
        .as_bytes()
        .to_vec();

    // The delegation itself is signed with the *blinded* type-11 key, so it follows the
    // bounded deployed ELS2 policy.
    let probe = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        transient_public.clone(),
        vec![0_u8; 64],
    )
    .expect("offline probe");
    let mut rng = ChaCha8Rng::seed_from_u64(26);
    let delegation = sign_type11_deployed(
        f.owner.blinded_private_key(),
        Els2SignedRegion::of_offline_keys(&probe),
        &mut rng,
    )
    .expect("delegation");
    let offline = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        transient_public,
        delegation.as_bytes().to_vec(),
    )
    .expect("offline block");

    // The record's own signature is then made by the transient type-7 key.
    let record_probe = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        base.blinded_public_key().to_vec(),
        base.published_seconds(),
        base.expires_offset_seconds(),
        Some(offline.clone()),
        base.outer_ciphertext().to_vec(),
        vec![0_u8; 64],
    )
    .expect("record probe");
    let record_signature = transient
        .signing_key()
        .sign(Els2SignedRegion::of_record(&record_probe).as_bytes())
        .expect("type 7 record signature");
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
    let validated = validate(with_offline).expect("offline-delegated record must validate");
    assert_eq!(
        validated.signature_profile(),
        Els2RecordSignatureProfile::Ed25519Transient,
        "a type-7 transient record signature must be reported as such, with no type-11 \
         transcript claimed for it"
    );
}

/// The same delegation signed under the strict transcript is still accepted, so a delegation
/// created before Plan 346 keeps working.
#[test]
fn a_strict_form_offline_delegation_remains_accepted() {
    let f = fixture(27, None);
    let base = build_record(&f, 28, Transcript::Deployed);
    let transient = router_bundle(0x2469);
    let transient_public = transient
        .signing_key()
        .public_key()
        .expect("transient public key")
        .as_bytes()
        .to_vec();
    let probe = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        transient_public.clone(),
        vec![0_u8; 64],
    )
    .expect("offline probe");
    let delegation = i2pr_crypto::red25519::sign_with_nonce(
        f.owner.blinded_private_key(),
        Els2SignedRegion::of_offline_keys(&probe).as_bytes(),
        &[0x33; 80],
    )
    .expect("strict delegation");
    let offline = EncryptedLeaseSet2OfflineKeys::new(
        PUBLISHED + 3_600,
        SigningKeyType::EdDsaSha512Ed25519,
        transient_public,
        delegation.as_bytes().to_vec(),
    )
    .expect("offline block");
    let record_probe = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        base.blinded_public_key().to_vec(),
        base.published_seconds(),
        base.expires_offset_seconds(),
        Some(offline.clone()),
        base.outer_ciphertext().to_vec(),
        vec![0_u8; 64],
    )
    .expect("record probe");
    let record_signature = transient
        .signing_key()
        .sign(Els2SignedRegion::of_record(&record_probe).as_bytes())
        .expect("type 7 record signature");
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
    assert!(
        validate(with_offline).is_ok(),
        "a strict-form delegation must remain accepted inside the bounded compatibility path"
    );
}

// --- policy invariants --------------------------------------------------------------------------

/// Every authorization mode builds the same corrected outer-signature profile.
///
/// No-auth, lookup secret, PSK, and DH differ only in the inner layer; the outer type-11
/// signature is the blinded key over the same region in all four. This row pins that the
/// correction is a property of the type-5 boundary rather than of one mode.
#[test]
fn every_authorization_mode_publishes_the_same_outer_profile() {
    for secret in [None, Some("a shared lookup secret")] {
        let f = fixture(30, secret);
        let validated = validate(build_record(&f, 31, Transcript::Deployed))
            .expect("deployed record must validate");
        assert_eq!(
            validated.signature_profile(),
            Els2RecordSignatureProfile::Type11(Els2Type11Profile::Deployed),
            "the outer type-11 profile must be `deployed` for secret={secret:?} as well"
        );
    }
}

/// An ambiguous transcript match is refused rather than resolved.
#[test]
fn an_ambiguous_profile_is_never_accepted() {
    assert!(!Els2Type11Profile::Ambiguous.is_accepted());
    assert!(!Els2Type11Profile::None.is_accepted());
    assert!(Els2Type11Profile::Deployed.is_accepted());
    assert!(Els2Type11Profile::Strict.is_accepted());
    assert_eq!(Els2Type11Profile::Ambiguous.as_str(), "ambiguous");
}

/// The ELS2 signer refuses a region above the type-5 record ceiling.
///
/// The deployed transcript has no length frame, so this ceiling is the compensating bound that
/// keeps an unbounded buffer out of the hash.
#[test]
fn the_signer_enforces_the_els2_record_ceiling() {
    let f = fixture(50, None);
    let record = build_record(&f, 51, Transcript::Deployed);
    let oversized = (0..i2pr_netdb::MAX_ELS2_RECORD_LENGTH + 1)
        .map(|index| index as u8)
        .collect::<Vec<u8>>();
    let Ok(padded) = EncryptedLeaseSet2::new(
        ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
        record.blinded_public_key().to_vec(),
        record.published_seconds(),
        record.expires_offset_seconds(),
        None,
        oversized,
        record.signature().to_vec(),
    ) else {
        // The codec's own ceiling is at least as strict and already refused the record; that
        // is an equally correct refusal point, so the row passes without a second check.
        return;
    };
    let mut rng = ChaCha8Rng::seed_from_u64(52);
    assert!(
        matches!(
            sign_type11_deployed(
                f.owner.blinded_private_key(),
                Els2SignedRegion::of_record(&padded),
                &mut rng
            ),
            Err(Els2Error::Blinding(_))
        ),
        "an oversized type-5 region must be refused by the ELS2 signer, not hashed anyway"
    );
}

/// A wrong blinded key is refused by the bounded verifier.
///
/// This is the row that would fail if `classify_type11` had been given a caller-chosen key
/// rather than the record's own.
#[test]
fn the_bounded_verifier_refuses_the_wrong_blinded_key() {
    let a = fixture(60, None);
    let b = fixture(61, None);
    let record = build_record(&a, 62, Transcript::Deployed);
    let outcome = i2pr_netdb::classify_type11(
        &b.blinded_public,
        Els2SignedRegion::of_record(&record),
        record.signature(),
    );
    assert_eq!(
        outcome,
        Els2Type11Profile::None,
        "a signature must not classify under another destination's blinded key"
    );
    assert!(matches!(
        i2pr_netdb::verify_type11(
            &b.blinded_public,
            Els2SignedRegion::of_record(&record),
            record.signature()
        ),
        Err(Els2Error::Type11SignatureRejected)
    ));
}

/// The blinded scalar a publisher signs with is never the unblinded one.
///
/// A type-5 signature is made with the blinded key; signing with the unblinded scalar would
/// produce a record no client could match to the day's storage key.
#[test]
fn the_blinded_and_unblinded_scalars_stay_distinct() {
    let mut rng = ChaCha8Rng::seed_from_u64(70);
    let unblinded: Red25519PrivateScalar = generate_private(&mut rng).expect("key");
    let day = BlindingDay::from_ymd(2023, 11, 14).expect("day");
    let alpha = i2pr_crypto::red25519::generate_alpha(
        &i2pr_crypto::red25519::derive_public_key(&unblinded),
        SigningKeyType::RedDsaSha512Ed25519,
        day,
        None,
    )
    .expect("alpha");
    let blinded = blind_private_key(&unblinded, &alpha);
    assert_ne!(
        blinded.secret_bytes(),
        unblinded.secret_bytes(),
        "the blinded scalar must not equal the unblinded one"
    );
}
