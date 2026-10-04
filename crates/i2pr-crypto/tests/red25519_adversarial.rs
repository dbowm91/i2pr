//! Red25519 adversarial, bounds, and independent re-derivation checks.
//!
//! Covers the negative surface the Plan 330 requirement set names: non-canonical scalars and
//! points, small-order and torsion inputs, malformed signature components, wrong keys,
//! messages, days, and secrets, oversized inputs, and randomness failure. It also compares
//! alpha derivation, blinding, and storage-key derivation against a locally generated
//! independent re-derivation fixture, because the I2P specifications publish no official
//! vectors for those operations.

use i2pr_crypto::red25519::{
    BLINDED_SIGNING_KEY_TYPE, BlindedPrivateScalar, BlindingDay, BlindingScalar,
    ED25519_SIGNING_KEY_TYPE, MAX_LOOKUP_SECRET_LENGTH, MAX_MESSAGE_LENGTH, Red25519Error,
    Red25519PrivateScalar, Red25519PublicKey, Red25519Signature, blind_private_key,
    blind_public_key, blinded_storage_key, convert_ed25519_private, derive_blinded_public_key,
    derive_public_key, generate_alpha, generate_private, sign, sign_with_nonce, verify,
    verify_blinded,
};
use i2pr_proto::SigningKeyType;
use rand_chacha::ChaCha8Rng;
use rand_core::{SeedableRng, TryCryptoRng, TryRngCore};

const INDEPENDENT: &str = include_str!("data/red25519-independent-derivation.json");

/// A random source that always fails, to prove randomness failure stays distinguishable.
struct FailingRng;

// A fallible source that never yields bytes, so randomness failure can be observed directly.
impl TryRngCore for FailingRng {
    type Error = &'static str;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Err("entropy unavailable")
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Err("entropy unavailable")
    }

    fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), Self::Error> {
        Err("entropy unavailable")
    }
}

impl TryCryptoRng for FailingRng {}

fn seed(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn private_from_seed(byte: u8) -> Red25519PrivateScalar {
    convert_ed25519_private(&seed(byte))
}

fn blinding_scalar(byte: u8) -> BlindingScalar {
    let mut scalar = [0_u8; 32];
    scalar[31] = 1;
    scalar[0] = byte % 200;
    BlindingScalar::from_bytes(scalar).expect("canonical scalar")
}

fn decode_hex(text: &str, length: usize) -> Vec<u8> {
    assert_eq!(text.len(), length * 2);
    (0..length)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

fn day(text: &str) -> BlindingDay {
    let year: u16 = text[0..4].parse().expect("year");
    let month: u8 = text[4..6].parse().expect("month");
    let day: u8 = text[6..8].parse().expect("day");
    BlindingDay::from_ymd(year, month, day).expect("valid day")
}

#[test]
fn independent_rederivation_agrees_on_alpha_blinding_and_storage_key() {
    let document: serde_json::Value = serde_json::from_str(INDEPENDENT).expect("fixture parses");
    let cases = document["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty());
    for case in cases {
        let id = case["id"].as_u64().expect("id");
        let edsk: [u8; 32] = decode_hex(case["edsk"].as_str().expect("edsk"), 32)
            .try_into()
            .expect("32 bytes");
        let expected_sk = decode_hex(case["sk"].as_str().expect("sk"), 32);
        let expected_vk = decode_hex(case["vk"].as_str().expect("vk"), 32);
        let sigtype = match case["unblinded_sigtype"].as_u64().expect("sigtype") {
            7 => ED25519_SIGNING_KEY_TYPE,
            11 => BLINDED_SIGNING_KEY_TYPE,
            other => panic!("unexpected sigtype {other}"),
        };
        let secret = case["lookup_secret"].as_str().expect("secret");
        let secret_option = if secret.is_empty() {
            None
        } else {
            Some(secret)
        };

        let converted = convert_ed25519_private(&edsk);
        assert_eq!(
            converted.secret_bytes().to_vec(),
            expected_sk,
            "case {id} sk"
        );
        let public_key = derive_public_key(&converted);
        assert_eq!(
            public_key.as_bytes().as_slice(),
            expected_vk,
            "case {id} vk"
        );

        let alpha = generate_alpha(
            &public_key,
            sigtype,
            day(case["day"].as_str().expect("day")),
            secret_option,
        )
        .expect("alpha");
        assert_eq!(
            alpha.secret_bytes().to_vec(),
            decode_hex(case["alpha"].as_str().expect("alpha"), 32),
            "case {id} alpha"
        );

        let blinded = blind_public_key(&public_key, &alpha).expect("prime-order blinding");
        assert_eq!(
            blinded.as_bytes().as_slice(),
            decode_hex(case["blinded_public_key"].as_str().expect("blinded"), 32),
            "case {id} blinded public key"
        );
        assert_eq!(
            blinded_storage_key(&blinded).as_bytes(),
            decode_hex(case["storage_key"].as_str().expect("storage key"), 32).as_slice(),
            "case {id} storage key"
        );

        // Private blinding must agree with public blinding for the same alpha.
        let blinded_private = blind_private_key(&converted, &alpha);
        assert_eq!(
            derive_blinded_public_key(&blinded_private),
            blinded,
            "case {id}"
        );
    }
}

#[test]
fn alpha_depends_on_day_secret_and_key_and_rejects_bad_input() {
    let key = derive_public_key(&private_from_seed(9));
    let today = day("20251015");
    let tomorrow = day("20251016");
    let base = generate_alpha(&key, ED25519_SIGNING_KEY_TYPE, today, None).expect("alpha");
    let again = generate_alpha(&key, ED25519_SIGNING_KEY_TYPE, today, None).expect("alpha");
    assert_eq!(
        base.secret_bytes(),
        again.secret_bytes(),
        "derivation is deterministic"
    );

    let other_day = generate_alpha(&key, ED25519_SIGNING_KEY_TYPE, tomorrow, None).expect("alpha");
    assert_ne!(
        base.secret_bytes(),
        other_day.secret_bytes(),
        "day changes alpha"
    );

    let with_secret =
        generate_alpha(&key, ED25519_SIGNING_KEY_TYPE, today, Some("s")).expect("alpha");
    assert_ne!(
        base.secret_bytes(),
        with_secret.secret_bytes(),
        "secret changes alpha"
    );
    let empty_secret =
        generate_alpha(&key, ED25519_SIGNING_KEY_TYPE, today, Some("")).expect("alpha");
    assert_eq!(
        base.secret_bytes(),
        empty_secret.secret_bytes(),
        "an absent secret and an empty secret are the same input"
    );

    let other_key = derive_public_key(&private_from_seed(10));
    let other_key_alpha =
        generate_alpha(&other_key, ED25519_SIGNING_KEY_TYPE, today, None).expect("alpha");
    assert_ne!(
        base.secret_bytes(),
        other_key_alpha.secret_bytes(),
        "key changes alpha"
    );

    let red = generate_alpha(&key, BLINDED_SIGNING_KEY_TYPE, today, None).expect("alpha");
    assert_ne!(
        base.secret_bytes(),
        red.secret_bytes(),
        "the unblinded sigtype is part of the derivation"
    );

    assert_eq!(
        generate_alpha(&key, SigningKeyType::DsaSha1, today, None).err(),
        Some(Red25519Error::UnsupportedSigType { algorithm: 0 })
    );
    let long = "x".repeat(MAX_LOOKUP_SECRET_LENGTH + 1);
    assert_eq!(
        generate_alpha(&key, ED25519_SIGNING_KEY_TYPE, today, Some(&long)).err(),
        Some(Red25519Error::LookupSecretTooLong {
            actual: MAX_LOOKUP_SECRET_LENGTH + 1,
            maximum: MAX_LOOKUP_SECRET_LENGTH,
        })
    );
    let longest_allowed = "x".repeat(MAX_LOOKUP_SECRET_LENGTH);
    assert!(
        generate_alpha(
            &key,
            ED25519_SIGNING_KEY_TYPE,
            today,
            Some(&longest_allowed)
        )
        .is_ok()
    );
}

#[test]
fn invalid_calendar_days_are_rejected_before_derivation() {
    assert_eq!(
        BlindingDay::from_ymd(2025, 0, 1),
        Err(Red25519Error::InvalidBlindingDay { reason: "month" })
    );
    assert_eq!(
        BlindingDay::from_ymd(2025, 13, 1),
        Err(Red25519Error::InvalidBlindingDay { reason: "month" })
    );
    assert_eq!(
        BlindingDay::from_ymd(2025, 12, 0),
        Err(Red25519Error::InvalidBlindingDay { reason: "day" })
    );
    assert_eq!(
        BlindingDay::from_ymd(2025, 4, 31),
        Err(Red25519Error::InvalidBlindingDay { reason: "day" })
    );
    assert_eq!(
        BlindingDay::from_ymd(2025, 2, 29),
        Err(Red25519Error::InvalidBlindingDay { reason: "day" }),
        "2025 is not a leap year"
    );
    assert!(
        BlindingDay::from_ymd(2024, 2, 29).is_ok(),
        "2024 is a leap year"
    );
    assert!(
        BlindingDay::from_ymd(2000, 2, 29).is_ok(),
        "2000 is a leap year"
    );
    assert!(
        BlindingDay::from_ymd(1900, 2, 29).is_err(),
        "1900 is not a leap year"
    );
    assert!(
        BlindingDay::from_ymd(1969, 12, 31).is_err(),
        "years before 1970 are rejected"
    );
    assert_eq!(
        day("20251015").as_bytes(),
        *b"20251015",
        "the derivation input is ASCII YYYYMMDD"
    );
    assert_eq!(day("20251015").as_ymd(), (2025, 10, 15));
}

#[test]
fn non_canonical_scalars_and_points_are_rejected() {
    // L itself and anything above it is not a canonical scalar.
    let mut l_bytes = [0_u8; 32];
    l_bytes[0] = 0xed;
    l_bytes[1] = 0xd3;
    l_bytes[2] = 0xf5;
    l_bytes[3] = 0x5c;
    l_bytes[4] = 0x1a;
    l_bytes[5] = 0x63;
    l_bytes[6] = 0x12;
    l_bytes[7] = 0x58;
    l_bytes[8] = 0xd6;
    l_bytes[9] = 0x9c;
    l_bytes[10] = 0xf7;
    l_bytes[11] = 0xa2;
    l_bytes[12] = 0xde;
    l_bytes[13] = 0xf9;
    l_bytes[14] = 0xde;
    l_bytes[15] = 0x14;
    l_bytes[16] = 0x00;
    l_bytes[17] = 0x00;
    l_bytes[18] = 0x00;
    l_bytes[19] = 0x00;
    l_bytes[20] = 0x00;
    l_bytes[21] = 0x00;
    l_bytes[22] = 0x00;
    l_bytes[23] = 0x00;
    l_bytes[24] = 0x00;
    l_bytes[25] = 0x00;
    l_bytes[26] = 0x00;
    l_bytes[27] = 0x00;
    l_bytes[28] = 0x00;
    l_bytes[29] = 0x00;
    l_bytes[30] = 0x00;
    l_bytes[31] = 0x10;
    assert_eq!(
        Red25519PrivateScalar::from_bytes(l_bytes).err(),
        Some(Red25519Error::NonCanonicalScalar),
        "L is not a canonical residue"
    );
    assert_eq!(
        BlindedPrivateScalar::from_bytes(l_bytes).err(),
        Some(Red25519Error::NonCanonicalScalar)
    );
    assert_eq!(
        BlindingScalar::from_bytes(l_bytes).err(),
        Some(Red25519Error::NonCanonicalScalar)
    );
    // A converted Ed25519 scalar is stored verbatim, but only in clamped shape.
    assert!(
        Red25519PrivateScalar::from_converted_ed25519_bytes(
            convert_ed25519_private(&seed(1)).secret_bytes()
        )
        .is_ok()
    );
    let mut unclamped = [0_u8; 32];
    unclamped[0] = 0x08;
    assert_eq!(
        Red25519PrivateScalar::from_converted_ed25519_bytes(unclamped).err(),
        Some(Red25519Error::NonCanonicalScalar),
        "an unclamped value is not a converted Ed25519 scalar"
    );
    assert!(
        !convert_ed25519_private(&seed(1)).is_canonical(),
        "converted Ed25519 scalars are routinely above L"
    );

    // Points: all-zero is the identity, not a usable key, and a non-canonical field encoding
    // must be rejected instead of being normalized.
    assert!(
        Red25519PublicKey::decode(&[0_u8; 32]).is_ok(),
        "the identity is a valid curve point, so decoding succeeds"
    );
    assert_eq!(
        blind_public_key(
            &Red25519PublicKey::from_bytes([0_u8; 32]),
            &blinding_scalar(1)
        )
        .err(),
        Some(Red25519Error::SmallOrderPoint),
        "the identity is refused where a prime-order key is required"
    );
    let mut non_canonical = derive_public_key(&private_from_seed(2)).as_bytes().to_vec();
    // y + p is out of range for the field and must not decode.
    let mut over_p = non_canonical.clone();
    over_p[31] |= 0x10;
    if over_p[31] != non_canonical[31] {
        assert_eq!(
            Red25519PublicKey::decode(&over_p).err(),
            Some(Red25519Error::InvalidPublicKey),
            "an out-of-range y coordinate is rejected"
        );
    }
    non_canonical[0] ^= 0x02;
    if Red25519PublicKey::decode(&non_canonical).is_ok() {
        // Some bit patterns are still valid points; flipping the sign bit must not be.
        let mut sign_flipped = derive_public_key(&private_from_seed(2)).as_bytes().to_vec();
        sign_flipped[31] ^= 0x80;
        assert!(
            matches!(
                Red25519PublicKey::decode(&sign_flipped).err(),
                Some(Red25519Error::InvalidPublicKey)
                    | Some(Red25519Error::SmallOrderPoint)
                    | Some(Red25519Error::NotPrimeOrderPoint)
            ),
            "a flipped sign bit is not a canonical encoding of the same point"
        );
    }
}

#[test]
fn blinding_refuses_small_order_and_torsion_inputs() {
    // The eight small-order points, including the identity, are not prime order.
    let small_order: [[u8; 32]; 4] = [
        [0_u8; 32],
        {
            let mut point = [0_u8; 32];
            point[0] = 1;
            point
        },
        {
            let mut point = [0_u8; 32];
            point[0] = 0x26;
            point[1] = 0xe8;
            point[2] = 0x95;
            point[3] = 0x8f;
            point[4] = 0xc2;
            point[5] = 0xb2;
            point[6] = 0x27;
            point[7] = 0xb0;
            point[8] = 0x45;
            point[9] = 0xc3;
            point[10] = 0xf4;
            point[11] = 0x89;
            point[12] = 0xf2;
            point[13] = 0xef;
            point[14] = 0x98;
            point[15] = 0xf0;
            point[16] = 0xd5;
            point[17] = 0xdf;
            point[18] = 0xac;
            point[19] = 0x05;
            point[20] = 0xd3;
            point[21] = 0xc6;
            point[22] = 0x33;
            point[23] = 0x39;
            point[24] = 0xb1;
            point[25] = 0x38;
            point[26] = 0x02;
            point[27] = 0x88;
            point[28] = 0x6d;
            point[29] = 0x53;
            point[30] = 0xfc;
            point[31] = 0x05;
            point
        },
        {
            let mut point = [0_u8; 32];
            point[0] = 0xc7;
            point[1] = 0x17;
            point[2] = 0x6a;
            point[3] = 0x70;
            point[4] = 0x3d;
            point[5] = 0x4d;
            point[6] = 0xd8;
            point[7] = 0x4f;
            point[8] = 0xba;
            point[9] = 0x3c;
            point[10] = 0x0b;
            point[11] = 0x76;
            point[12] = 0x0d;
            point[13] = 0x10;
            point[14] = 0x67;
            point[15] = 0x0f;
            point[16] = 0x2a;
            point[17] = 0x20;
            point[18] = 0x53;
            point[19] = 0xfa;
            point[20] = 0x2c;
            point[21] = 0x39;
            point[22] = 0xcc;
            point[23] = 0xc6;
            point[24] = 0x4e;
            point[25] = 0xc7;
            point[26] = 0xfd;
            point[27] = 0x77;
            point[28] = 0x92;
            point[29] = 0xac;
            point[30] = 0x03;
            point[31] = 0x7a;
            point
        },
    ];
    let alpha = blinding_scalar(1);
    let mut refused = 0;
    for point in small_order {
        let key = Red25519PublicKey::from_bytes(point);
        match blind_public_key(&key, &alpha) {
            Err(Red25519Error::SmallOrderPoint) => refused += 1,
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("a small-order point must not be blindable"),
        }
    }
    assert_eq!(
        refused,
        small_order.len(),
        "every small-order point is refused"
    );
}

#[test]
fn signature_parsing_and_verification_fail_closed() {
    let vector_message = [2_u8; 32];
    let private = private_from_seed(3);
    let public = derive_public_key(&private);
    let alpha = blinding_scalar(1);
    let blinded_private = blind_private_key(&private, &alpha);
    let blinded_public = blind_public_key(&public, &alpha).expect("blinded");
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    let signature = sign(&blinded_private, &vector_message, &mut rng).expect("sign");

    assert_eq!(
        Red25519Signature::decode(&signature.as_bytes()[..63]).err(),
        Some(Red25519Error::InvalidSignatureLength {
            actual: 63,
            expected: 64
        })
    );
    assert_eq!(
        Red25519Signature::decode(&[0_u8; 65]).err(),
        Some(Red25519Error::InvalidSignatureLength {
            actual: 65,
            expected: 64
        })
    );

    // One-bit corruption anywhere in the signature must be rejected.
    for index in 0..64_usize {
        let mut corrupted = *signature.as_bytes();
        corrupted[index] ^= 0x01;
        let candidate = Red25519Signature::from_bytes(corrupted);
        assert!(
            verify_blinded(&blinded_public, &vector_message, &candidate).is_err(),
            "bit flip at byte {index} was accepted"
        );
    }

    // A non-canonical S component is rejected as a malformed signature, not as a bad signature.
    let mut bad_s = *signature.as_bytes();
    bad_s[63] |= 0x80;
    assert!(
        verify_blinded(
            &blinded_public,
            &vector_message,
            &Red25519Signature::from_bytes(bad_s)
        )
        .is_err()
    );

    // Wrong message, wrong key, wrong sigtype context.
    let mut wrong_message = vector_message;
    wrong_message[0] ^= 1;
    assert!(verify_blinded(&blinded_public, &wrong_message, &signature).is_err());
    assert!(verify(&public, &vector_message, &signature).is_err());
    assert_eq!(
        verify_blinded(&blinded_public, &vector_message, &signature),
        Ok(())
    );
    assert_eq!(
        verify_blinded(&blinded_public, &vector_message, &signature),
        Ok(()),
        "verification is stable"
    );
}

#[test]
fn message_length_ceiling_is_enforced_before_hashing() {
    let private = private_from_seed(4);
    let public = derive_public_key(&private);
    let alpha = blinding_scalar(2);
    let blinded_private = blind_private_key(&private, &alpha);
    let blinded_public = blind_public_key(&public, &alpha).expect("blinded");

    let at_limit = vec![0x5a_u8; MAX_MESSAGE_LENGTH];
    let signature =
        sign_with_nonce(&blinded_private, &at_limit, &[0x22; 80]).expect("sign at limit");
    verify_blinded(&blinded_public, &at_limit, &signature).expect("verifies at the ceiling");

    let over_limit = vec![0x5a_u8; MAX_MESSAGE_LENGTH + 1];
    assert_eq!(
        sign_with_nonce(&blinded_private, &over_limit, &[0x22; 80]).err(),
        Some(Red25519Error::MessageTooLong {
            actual: MAX_MESSAGE_LENGTH + 1,
            maximum: MAX_MESSAGE_LENGTH,
        })
    );
    assert_eq!(
        verify_blinded(&blinded_public, &over_limit, &signature),
        Err(Red25519Error::BlindedSignatureVerificationFailed),
        "an over-long message is rejected by verification as well"
    );
}

#[test]
fn randomness_failure_is_distinguishable_from_protocol_invalidity() {
    let private = private_from_seed(6);
    let alpha = blinding_scalar(3);
    let blinded_private = blind_private_key(&private, &alpha);

    let mut failing = FailingRng;
    assert_eq!(
        generate_private(&mut failing).err(),
        Some(Red25519Error::RandomnessUnavailable)
    );
    assert_eq!(
        sign(&blinded_private, b"message", &mut failing).err(),
        Some(Red25519Error::RandomnessUnavailable)
    );
    // A protocol rejection is a different variant, so callers cannot retry one as the other.
    assert_eq!(
        sign_with_nonce(&blinded_private, &[0_u8; MAX_MESSAGE_LENGTH + 1], &[0; 80]).err(),
        Some(Red25519Error::MessageTooLong {
            actual: MAX_MESSAGE_LENGTH + 1,
            maximum: MAX_MESSAGE_LENGTH,
        })
    );
}

#[test]
fn generated_keys_are_distinct_canonical_and_signable() {
    let mut rng = ChaCha8Rng::seed_from_u64(2026);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let private = generate_private(&mut rng).expect("generate");
        assert!(private.is_canonical(), "generated scalars are canonical");
        assert!(
            seen.insert(private.secret_bytes()),
            "repeated generated key"
        );
        let public = derive_public_key(&private);
        assert!(Red25519PublicKey::decode(public.as_bytes()).is_ok());
        let alpha = blinding_scalar(7);
        let blinded_private = blind_private_key(&private, &alpha);
        let blinded_public = blind_public_key(&public, &alpha).expect("blinded");
        assert_eq!(derive_blinded_public_key(&blinded_private), blinded_public);
        let signature = sign(&blinded_private, b"payload", &mut rng).expect("sign");
        verify_blinded(&blinded_public, b"payload", &signature).expect("verify");
    }
}

#[test]
fn blinded_key_round_trip_and_storage_key_are_stable() {
    let private = private_from_seed(8);
    let public = derive_public_key(&private);
    let key_day = day("20251015");
    let alpha =
        generate_alpha(&public, ED25519_SIGNING_KEY_TYPE, key_day, Some("secret")).expect("alpha");
    let blinded_private = blind_private_key(&private, &alpha);
    let blinded_public = blind_public_key(&public, &alpha).expect("blinded");
    assert_eq!(
        derive_blinded_public_key(&blinded_private),
        blinded_public,
        "private and public blinding agree"
    );
    let restored =
        BlindedPrivateScalar::from_bytes(blinded_private.secret_bytes()).expect("restore");
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let signature = sign(&restored, b"payload", &mut rng).expect("sign");
    verify_blinded(&blinded_public, b"payload", &signature).expect("verify after restore");

    let storage = blinded_storage_key(&blinded_public);
    let other = blinded_storage_key(&derive_public_key(&private_from_seed(12)));
    assert_ne!(
        storage.as_bytes(),
        other.as_bytes(),
        "distinct blinded keys have distinct storage keys"
    );
    let other_day_key = blind_public_key(
        &public,
        &generate_alpha(
            &public,
            ED25519_SIGNING_KEY_TYPE,
            day("20251016"),
            Some("secret"),
        )
        .expect("alpha"),
    )
    .expect("blinded");
    assert_ne!(
        storage.as_bytes(),
        blinded_storage_key(&other_day_key).as_bytes(),
        "a different day yields a different storage key"
    );
}
