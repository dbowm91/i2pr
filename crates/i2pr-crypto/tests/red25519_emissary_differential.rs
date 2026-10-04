//! Black-box differential against the Emissary reference implementation.
//!
//! The fixture values were produced by an external driver (kept outside this repository) that
//! links `eggstack/emissary@6885a945d25a5ae61bc68191d27c5816bc3df4c9` as an unmodified path
//! dependency and calls only the public `emissary_core::crypto::red25519` API. No Emissary
//! source is vendored, copied, or transliterated here, and the driver ran after the Plan 330
//! implementation commit was frozen, so the comparison could not have influenced i2pr's code.
//!
//! This is the strongest interoperability evidence the branch has: for the same key, day,
//! lookup secret, and signing transcript, i2pr and Emissary produce **byte-identical** alpha
//! values, blinded keys, DHT storage keys, and signatures. Note the contrast with the i2pd
//! differential in `red25519_reference_differential.rs`, where the key derivations agree but
//! the signature transcript does not.

use i2pr_crypto::red25519::{
    BLINDED_SIGNING_KEY_TYPE, BlindedPrivateScalar, BlindingDay, ED25519_SIGNING_KEY_TYPE,
    Red25519Signature, blind_private_key, blind_public_key, blinded_storage_key,
    convert_ed25519_private, derive_blinded_public_key, derive_public_key, generate_alpha, sign,
    sign_with_nonce, verify_blinded,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const EMISSARY: &str = include_str!("data/red25519-emissary-differential.json");

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
fn i2pr_is_byte_compatible_with_the_emissary_reference() {
    let document: serde_json::Value = serde_json::from_str(EMISSARY).expect("fixture parses");
    assert_eq!(
        document["provenance"]["revision"].as_str(),
        Some("6885a945d25a5ae61bc68191d27c5816bc3df4c9"),
        "the differential is pinned to the reviewed reference revision"
    );
    let cases = document["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "the differential must not be empty");

    for case in cases {
        let label = case["unblinded_public_key"]
            .as_str()
            .expect("key")
            .chars()
            .take(8)
            .collect::<String>();
        let sigtype = match case["unblinded_sigtype"].as_u64().expect("sigtype") {
            7 => ED25519_SIGNING_KEY_TYPE,
            11 => BLINDED_SIGNING_KEY_TYPE,
            other => panic!("unexpected unblinded sigtype {other}"),
        };
        assert_eq!(
            case["unblinded_public_key"].as_str().expect("key"),
            hex(derive_public_key(&convert_ed25519_private(&decode32(
                case["unblinded_private_seed"].as_str().expect("seed"),
            )))
            .as_bytes()),
            "case {label}: the converted key must match the reference's unblinded key"
        );

        let secret = case["lookup_secret"].as_str().expect("secret");
        let secret_option = if secret.is_empty() {
            None
        } else {
            Some(secret)
        };
        let unblinded = derive_public_key(&convert_ed25519_private(&decode32(
            case["unblinded_private_seed"].as_str().expect("seed"),
        )));

        // Alpha, blinded keys, and the DHT storage key must agree byte-for-byte, including the
        // optional lookup-secret case.
        let alpha = generate_alpha(
            &unblinded,
            sigtype,
            day(case["day"].as_str().expect("day")),
            secret_option,
        )
        .expect("alpha");
        assert_eq!(
            alpha.secret_bytes().as_slice(),
            decode(case["alpha"].as_str().expect("alpha"), 32),
            "case {label}: daily alpha"
        );

        let blinded_private = blind_private_key(
            &convert_ed25519_private(&decode32(
                case["unblinded_private_seed"].as_str().expect("seed"),
            )),
            &alpha,
        );
        let blinded_public = blind_public_key(&unblinded, &alpha).expect("blinded public");
        assert_eq!(
            blinded_public.as_bytes().as_slice(),
            decode(
                case["blinded_public_key"].as_str().expect("blinded public"),
                32
            ),
            "case {label}: blinded public key"
        );
        assert_eq!(
            blinded_private.secret_bytes().as_slice(),
            decode(
                case["blinded_private_key"]
                    .as_str()
                    .expect("blinded private"),
                32
            ),
            "case {label}: blinded private key"
        );
        assert_eq!(
            hex(derive_blinded_public_key(&blinded_private).as_bytes()),
            case["blinded_public_from_private"]
                .as_str()
                .expect("derived"),
            "case {label}: the reference agrees that private and public blinding match"
        );
        assert_eq!(
            blinded_storage_key(&blinded_public).as_bytes(),
            decode(case["storage_key"].as_str().expect("storage key"), 32).as_slice(),
            "case {label}: blinded DHT storage key"
        );

        // The decisive check: with the reference's own 80-byte signing transcript, i2pr must
        // reproduce the reference's signature byte-for-byte.
        let transcript_bytes = decode(
            case["signature_transcript"].as_str().expect("transcript"),
            80,
        );
        let transcript: [u8; 80] = transcript_bytes.try_into().expect("80 bytes");
        let message_hex = case["signed_message"].as_str().expect("message");
        let message = decode(message_hex, message_hex.len() / 2);
        let ours = sign_with_nonce(&blinded_private, &message, &transcript).expect("sign");
        assert_eq!(
            hex(ours.as_bytes()),
            case["signature"].as_str().expect("signature"),
            "case {label}: Red25519 signatures must be byte-identical for the same transcript"
        );

        // And the reference's signature must verify here, including the rejection behaviors the
        // reference itself confirmed.
        let reference_signature =
            Red25519Signature::decode(&decode(case["signature"].as_str().expect("signature"), 64))
                .expect("reference signature");
        verify_blinded(&blinded_public, &message, &reference_signature).unwrap_or_else(|error| {
            panic!("case {label}: reference signature must verify: {error}")
        });
        let mut wrong = message.clone();
        wrong[0] ^= 0x01;
        assert!(
            verify_blinded(&blinded_public, &wrong, &reference_signature).is_err(),
            "case {label}: a modified message must be rejected"
        );
        let mut corrupted = *reference_signature.as_bytes();
        corrupted[0] ^= 0x01;
        assert!(
            verify_blinded(
                &blinded_public,
                &message,
                &Red25519Signature::from_bytes(corrupted)
            )
            .is_err(),
            "case {label}: a corrupted signature must be rejected"
        );

        // Randomized signing with a CSPRNG still verifies against the reference-derived key, and
        // the reference's own blinded private key is a valid i2pr key.
        let restored = BlindedPrivateScalar::from_bytes(decode32(
            case["blinded_private_key"]
                .as_str()
                .expect("blinded private"),
        ))
        .expect("reference blinded scalar is canonical");
        assert_eq!(derive_blinded_public_key(&restored), blinded_public);
        let mut rng = ChaCha8Rng::seed_from_u64(31337);
        let random_signature = sign(&restored, &message, &mut rng).expect("sign");
        verify_blinded(&blinded_public, &message, &random_signature)
            .expect("randomized signature verifies");
        assert_ne!(hex(random_signature.as_bytes()), hex(ours.as_bytes()));
    }
}

#[test]
fn emissary_key_material_is_compatible_with_i2pd_key_material() {
    // The two references disagree about the signature transcript, so only the addressing half is
    // comparable. This test pins that half as cross-reference agreement, so a future change that
    // breaks blinding compatibility fails loudly instead of silently.
    let emissary: serde_json::Value =
        serde_json::from_str(include_str!("data/red25519-emissary-differential.json"))
            .expect("emissary fixture");
    let i2pd: serde_json::Value =
        serde_json::from_str(include_str!("data/red25519-i2pd-differential.json"))
            .expect("i2pd fixture");
    let emissary_cases = emissary["cases"].as_array().expect("emissary cases");
    let i2pd_cases = i2pd["cases"].as_array().expect("i2pd cases");

    // Pair by destination and day: i2pd has no lookup-secret case, so only the four no-secret
    // cases are comparable.
    let mut compared = 0;
    for left in emissary_cases {
        if !left["lookup_secret"].as_str().expect("secret").is_empty() {
            continue;
        }
        let key = left["unblinded_public_key"].as_str().expect("key");
        let right = i2pd_cases
            .iter()
            .find(|case| case["unblinded_public_key"].as_str() == Some(key))
            .unwrap_or_else(|| panic!("no i2pd case for {key}"));
        assert_eq!(left["day"].as_str(), right["day"].as_str(), "same day");
        compared += 1;
        assert_eq!(
            left["blinded_public_key"].as_str(),
            right["blinded_public_key"].as_str(),
            "blinded public key must agree across references"
        );
        assert_eq!(
            left["blinded_private_key"].as_str(),
            right["blinded_private_key"].as_str(),
            "blinded private key must agree across references"
        );
        assert_eq!(
            left["storage_key"].as_str(),
            right["storage_key"].as_str(),
            "DHT storage key must agree across references"
        );
        assert_ne!(
            left["signature"].as_str(),
            right["signature"].as_str(),
            "the references sign with different transcripts, so signatures cannot match"
        );
    }
    assert_eq!(compared, 4, "all four no-secret cases must be compared");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
