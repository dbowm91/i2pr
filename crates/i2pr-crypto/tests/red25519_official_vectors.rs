//! Official I2P Red25519 vector conformance.
//!
//! The fixture is the specification's own test-vector section, transcribed mechanically from
//! the page pinned in `specs/references/red25519-clean-room-freeze.md`. Per that freeze record
//! §3, the deterministic fields are compared byte-for-byte and the two signature rows are used
//! as verification vectors only, because `SIGN` mixes 80 random bytes the specification does
//! not publish.

use std::collections::BTreeMap;

use i2pr_crypto::red25519::BlindedPrivateScalar;
use i2pr_crypto::red25519::{
    BLINDED_SIGNING_KEY_TYPE, ED25519_SIGNING_KEY_TYPE, Red25519PrivateScalar, Red25519PublicKey,
    Red25519Signature, blind_private_key, blind_public_key, convert_ed25519_private,
    derive_blinded_public_key, derive_public_key, sign, sign_with_nonce, verify, verify_blinded,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

const FIXTURE: &str = include_str!("data/red25519-official-vectors.json");

struct Vector {
    id: u8,
    fields: BTreeMap<String, String>,
}

fn vectors() -> Vec<Vector> {
    let document: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture parses");
    let mut out = Vec::new();
    for entry in document["vectors"]
        .as_array()
        .expect("vectors array")
        .iter()
    {
        let id = entry["id"].as_u64().expect("vector id") as u8;
        let fields = entry
            .as_object()
            .expect("vector object")
            .iter()
            .filter(|(key, _)| key.as_str() != "id")
            .map(|(key, value)| (key.clone(), value.as_str().expect("hex string").to_string()))
            .collect();
        out.push(Vector { id, fields });
    }
    assert_eq!(out.len(), 10, "the specification publishes ten vectors");
    out
}

fn bytes32(vector: &Vector, field: &str) -> [u8; 32] {
    decode(&vector.fields[field], 32)
        .try_into()
        .expect("32 bytes")
}

fn decode(hex: &str, length: usize) -> Vec<u8> {
    assert_eq!(hex.len(), length * 2, "unexpected length for {hex}");
    (0..length)
        .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

fn blinded_from(vector: &Vector, field: &str) -> BlindedPrivateScalar {
    BlindedPrivateScalar::from_bytes(bytes32(vector, field))
        .expect("canonical vector blinded scalar")
}

fn private_from(vector: &Vector, field: &str) -> Red25519PrivateScalar {
    Red25519PrivateScalar::from_bytes(bytes32(vector, field)).expect("canonical vector scalar")
}

fn public_from(vector: &Vector, field: &str) -> Red25519PublicKey {
    Red25519PublicKey::decode(&bytes32(vector, field)).expect("vector public key decodes")
}

fn signature_from(vector: &Vector, field: &str) -> Red25519Signature {
    Red25519Signature::decode(&decode(&vector.fields[field], 64)).expect("64-byte vector signature")
}

#[test]
fn converted_private_scalars_and_public_keys_match_the_specification() {
    for vector in vectors() {
        let seed = bytes32(&vector, "edsk");
        let converted = convert_ed25519_private(&seed);
        assert_eq!(
            converted.secret_bytes().to_vec(),
            bytes32(&vector, "sk").to_vec(),
            "vector {} CONVERT_ED25519_PRIVATE",
            vector.id
        );
        assert_eq!(
            derive_public_key(&converted).as_bytes(),
            &bytes32(&vector, "vk"),
            "vector {} DERIVE_PUBLIC(sk) must equal vk",
            vector.id
        );
        assert_eq!(
            &bytes32(&vector, "vk")[..],
            &bytes32(&vector, "edpk")[..],
            "vector {} type conversion leaves the public key unchanged",
            vector.id
        );
        assert_eq!(
            ED25519_SIGNING_KEY_TYPE.code(),
            7,
            "the vectors describe Ed25519 destinations"
        );
        assert_eq!(BLINDED_SIGNING_KEY_TYPE.code(), 11);
    }
}

#[test]
fn re_randomization_matches_the_specification() {
    for vector in vectors() {
        let seed = bytes32(&vector, "edsk");
        let sk = convert_ed25519_private(&seed);
        let vk = derive_public_key(&sk);
        let alpha = private_from(&vector, "alpha");

        let blinded_private = blind_private_key(&sk, &alpha_secret(&alpha));
        assert_eq!(
            blinded_private.secret_bytes().to_vec(),
            bytes32(&vector, "rsk").to_vec(),
            "vector {} RANDOMIZE_PRIVATE",
            vector.id
        );
        assert_eq!(
            derive_blinded_public_key(&blinded_private).as_bytes(),
            &bytes32(&vector, "rvk"),
            "vector {} DERIVE_PUBLIC(rsk) must equal rvk",
            vector.id
        );

        let blinded_public = blind_public_key(&vk, &alpha_secret(&alpha)).expect("prime-order key");
        assert_eq!(
            blinded_public.as_bytes(),
            &bytes32(&vector, "rvk"),
            "vector {} RANDOMIZE_PUBLIC",
            vector.id
        );
    }
}

/// Wraps a vector alpha value in the blinding-scalar owner.
fn alpha_secret(alpha: &Red25519PrivateScalar) -> i2pr_crypto::red25519::BlindingScalar {
    i2pr_crypto::red25519::BlindingScalar::from_bytes(alpha.secret_bytes())
        .expect("vector alpha is canonical")
}

#[test]
fn converted_public_keys_match_the_crates_ordinary_ed25519_owner() {
    // Regression floor: the Red25519 composition must agree with the crate's existing type-7
    // Ed25519 owner for the same seed, so adding Red25519 cannot change ordinary identity or
    // signing behavior.
    for byte in 1..=8_u8 {
        let seed = [byte; 32];
        let signing = i2pr_crypto::SigningPrivateKey::from_bytes(seed);
        let ordinary = signing.public_key().expect("ed25519 public key");
        assert_eq!(ordinary.key_type().code(), 7);
        let converted = convert_ed25519_private(&seed);
        assert_eq!(
            derive_public_key(&converted).as_bytes().as_slice(),
            ordinary.as_bytes(),
            "CONVERT_ED25519_PUBLIC must be the identity function"
        );
    }
}

#[test]
fn published_signatures_verify_under_the_specified_equation() {
    for vector in vectors() {
        let seed = bytes32(&vector, "edsk");
        let vk = derive_public_key(&convert_ed25519_private(&seed));
        let message = bytes32(&vector, "msg");
        verify(&vk, &message, &signature_from(&vector, "sig"))
            .unwrap_or_else(|error| panic!("vector {} sig must verify: {error}", vector.id));

        let blinded_public = public_from(&vector, "rvk");
        verify_blinded(&blinded_public, &message, &signature_from(&vector, "rsig"))
            .unwrap_or_else(|error| panic!("vector {} rsig must verify: {error}", vector.id));
    }
}

#[test]
fn exact_equation_is_validated_with_injected_transcripts() {
    // `SIGN` cannot be compared against the published rows because the 80-byte nonce is not
    // published. Instead: a fixed transcript must be bit-reproducible, verification must accept
    // it, and a different transcript must produce a different signature over the same message.
    for vector in vectors() {
        let blinded = blinded_from(&vector, "rsk");
        let blinded_public = public_from(&vector, "rvk");
        let message = bytes32(&vector, "msg");
        let mut nonce = [0_u8; 80];
        nonce.copy_from_slice(&[0x11; 80]);

        let first = sign_with_nonce(&blinded, &message, &nonce).expect("sign");
        let second = sign_with_nonce(&blinded, &message, &nonce).expect("sign");
        assert_eq!(first, second, "an injected transcript is deterministic");
        verify_blinded(&blinded_public, &message, &first).expect("injected-transcript signature");

        let mut other = nonce;
        other[0] ^= 0x01;
        let randomised = sign_with_nonce(&blinded, &message, &other).expect("sign");
        assert_ne!(
            first, randomised,
            "a different transcript must produce a different signature"
        );
        verify_blinded(&blinded_public, &message, &randomised).expect("randomised signature");
    }
}

#[test]
fn injected_nonce_signature_equals_production_signing_for_the_same_nonce() {
    // The production path draws the nonce from the injected CSPRNG; with a seeded generator
    // the transcript it produces must match the explicit-nonce path byte for byte.
    let vector = &vectors()[0];
    let blinded = blinded_from(vector, "rsk");
    let blinded_public = public_from(vector, "rvk");
    let message = bytes32(vector, "msg");
    let mut expected_nonce = [0_u8; 80];
    expected_nonce.copy_from_slice(&[0x42; 80]);

    // Two generators with the same seed stand in for "the same drawn transcript".
    let mut producing_rng = ChaCha8Rng::seed_from_u64(7);
    let mut replay_rng = ChaCha8Rng::seed_from_u64(7);
    let mut replay_nonce = [0_u8; 80];
    replay_rng.fill_bytes(&mut replay_nonce);

    let produced = sign(&blinded, &message, &mut producing_rng).expect("production sign");
    let replayed = sign_with_nonce(&blinded, &message, &replay_nonce).expect("explicit sign");
    assert_eq!(
        produced, replayed,
        "production signing uses the drawn transcript"
    );
    assert_ne!(
        produced,
        sign_with_nonce(&blinded, &message, &expected_nonce).expect("sign")
    );
    verify_blinded(&blinded_public, &message, &produced).expect("produced signature verifies");
    verify_blinded(&blinded_public, &message, &replayed).expect("replayed signature verifies");
}

#[test]
fn randomized_production_signing_never_repeats_and_always_verifies() {
    let vector = &vectors()[0];
    let blinded = blinded_from(vector, "rsk");
    let blinded_public = public_from(vector, "rvk");
    let message = bytes32(vector, "msg");
    let mut rng = ChaCha8Rng::seed_from_u64(99);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..32 {
        let signature = sign(&blinded, &message, &mut rng).expect("sign");
        verify_blinded(&blinded_public, &message, &signature).expect("verifies");
        assert!(
            seen.insert(signature.as_bytes().to_vec()),
            "signature repeated"
        );
    }
}

#[test]
fn verification_rejects_a_vector_signature_under_the_wrong_key_or_message() {
    let vector = &vectors()[0];
    let other = &vectors()[1];
    let message = bytes32(vector, "msg");
    let vk = derive_public_key(&convert_ed25519_private(&bytes32(vector, "edsk")));
    let signature = signature_from(vector, "sig");

    assert!(verify(&vk, &message, &signature).is_ok());
    assert!(
        verify(
            &derive_public_key(&convert_ed25519_private(&bytes32(other, "edsk"))),
            &message,
            &signature
        )
        .is_err(),
        "another vector's key must not verify this signature"
    );
    assert!(
        verify(&vk, &bytes32(other, "msg"), &signature).is_err(),
        "another vector's message must not verify this signature"
    );
    assert!(
        verify(&public_from(vector, "rvk"), &message, &signature).is_err(),
        "the blinded key must not accept the unblinded signature"
    );
}
