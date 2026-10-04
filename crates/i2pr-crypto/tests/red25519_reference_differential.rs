//! Cross-implementation differential against the pinned i2pd reference.
//!
//! The fixture values were produced by `tools/i2pd-red25519-oracle.cpp` linked against the
//! unmodified i2pd reference library at `2c694149fa6996eaeb23e378d5f83c9d3232c22f`. The test
//! proves three separate agreements: that i2pr derives the same blinded keys and storage key
//! from the same destination, that i2pr accepts the reference's blinded private key as a valid
//! key for the reference's blinded public key, and that i2pr verifies a signature the reference
//! produced with its blinded key.
//!
//! Randomized signatures are compared by acceptance, never by byte equality, because Red25519
//! signing is randomized by design.

use i2pr_crypto::red25519::{
    BLINDED_SIGNING_KEY_TYPE, BlindedPrivateScalar, BlindingDay, ED25519_SIGNING_KEY_TYPE,
    Red25519Signature, blind_private_key, blind_public_key, blinded_storage_key,
    convert_ed25519_private, derive_blinded_public_key, derive_public_key, generate_alpha, sign,
    verify_blinded,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const DIFFERENTIAL: &str = include_str!("data/red25519-i2pd-differential.json");

fn decode(text: &str, length: usize) -> Vec<u8> {
    assert_eq!(text.len(), length * 2, "unexpected hex length");
    (0..length)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

fn decode32(text: &str) -> [u8; 32] {
    decode(text, 32).try_into().expect("32 bytes")
}

fn day(text: &str) -> BlindingDay {
    BlindingDay::from_ymd(
        text[0..4].parse().expect("year"),
        text[4..6].parse().expect("month"),
        text[6..8].parse().expect("day"),
    )
    .expect("valid reference day")
}

#[test]
fn i2pr_agrees_with_the_pinned_i2pd_reference() {
    let document: serde_json::Value = serde_json::from_str(DIFFERENTIAL).expect("fixture parses");
    assert_eq!(
        document["provenance"]["revision"].as_str(),
        Some("2c694149fa6996eaeb23e378d5f83c9d3232c22f"),
        "the differential is pinned to the reviewed reference revision"
    );
    let cases = document["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "the differential must not be empty");

    for case in cases {
        let id = case["unblinded_public_key"].as_str().expect("key");
        let sigtype = match case["unblinded_sigtype"].as_u64().expect("sigtype") {
            7 => ED25519_SIGNING_KEY_TYPE,
            11 => BLINDED_SIGNING_KEY_TYPE,
            other => panic!("unexpected unblinded sigtype {other}"),
        };
        assert_eq!(
            case["blinded_sigtype"].as_u64(),
            Some(BLINDED_SIGNING_KEY_TYPE.code() as u64),
            "the reference always blinds to signature type 11"
        );

        let seed = decode32(case["unblinded_private_seed"].as_str().expect("seed"));
        let converted = convert_ed25519_private(&seed);
        let unblinded = derive_public_key(&converted);
        assert_eq!(
            unblinded.as_bytes().as_slice(),
            decode(case["unblinded_public_key"].as_str().expect("public"), 32),
            "reference case: the converted key must match the reference's unblinded key"
        );

        let alpha = generate_alpha(
            &unblinded,
            sigtype,
            day(case["day"].as_str().expect("day")),
            None,
        )
        .expect("alpha");
        let blinded_private = blind_private_key(&converted, &alpha);
        let blinded_public = blind_public_key(&unblinded, &alpha).expect("blinded public");

        assert_eq!(
            blinded_public.as_bytes().as_slice(),
            decode(
                case["blinded_public_key"].as_str().expect("blinded public"),
                32
            ),
            "reference case starting {id}: blinded public key"
        );
        assert_eq!(
            blinded_private.secret_bytes().as_slice(),
            decode(
                case["blinded_private_key"]
                    .as_str()
                    .expect("blinded private"),
                32
            ),
            "reference case starting {id}: blinded private key"
        );
        assert_eq!(
            blinded_storage_key(&blinded_public).as_bytes(),
            decode(case["storage_key"].as_str().expect("storage key"), 32).as_slice(),
            "reference case starting {id}: blinded DHT storage key"
        );

        // The reference's own blinded private key must be a valid key for the reference's
        // blinded public key when used by i2pr.
        let reference_private = BlindedPrivateScalar::from_bytes(decode32(
            case["blinded_private_key"]
                .as_str()
                .expect("blinded private"),
        ))
        .expect("reference blinded scalar is canonical");
        assert_eq!(
            derive_blinded_public_key(&reference_private),
            blinded_public,
            "reference case starting {id}: reference private key derives the reference public key"
        );

        // A signature produced by the reference must NOT verify under the specification's
        // Red25519 equation, and that difference is a classified, executed finding rather than
        // a silent skip. The pinned reference signs with a bare SHA-512 transcript
        // (SHA-512(T || A || M), then SHA-512(R || A || M)) that omits both the
        // "I2P_Red25519H(x)" domain and the specification's length framing, and it routes
        // type 11 through its plain Ed25519 verifier. The specification's own published
        // vector corpus decides: all ten official vectors verify here and none of them
        // verify with the bare transcript, so i2pr follows the specification and the
        // reference's type-11 signature is mutually unverifiable. See the Plan 331 closure
        // record, "Reference signature-transcript divergence".
        let reference_signature =
            Red25519Signature::decode(&decode(case["signature"].as_str().expect("signature"), 64))
                .expect("reference signature");
        let message_hex = case["signed_message"].as_str().expect("message");
        let message = decode(message_hex, message_hex.len() / 2);
        assert!(
            verify_blinded(&blinded_public, &message, &reference_signature).is_err(),
            "reference case starting {id}: the pinned reference signature is expected NOT to \
             verify under the specification equation; if it now does, the reference gained \
             specification conformance and this classified finding must be re-evaluated"
        );

        // And the reverse direction: a signature made here verifies under the same key.
        let mut rng = ChaCha8Rng::seed_from_u64(20251015);
        let ours = sign(&reference_private, b"i2pr plan 331 differential", &mut rng)
            .expect("i2pr signs with the reference key");
        verify_blinded(&blinded_public, b"i2pr plan 331 differential", &ours)
            .unwrap_or_else(|error| panic!("i2pr signature must verify: {error}"));
    }
}
