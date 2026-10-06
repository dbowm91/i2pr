//! Plan 346: the deployed type-11 ELS2 transcript profile, measured against the pinned
//! Java I2P and i2pd outputs.
//!
//! # What changed and why these rows exist
//!
//! Plan 335 measured that i2pr's strict Proposal-146 transcript and the transcript stock
//! Java I2P and i2pd use reject each other, and closed with the honest consequence: no
//! type-5 record was interoperable. Plan 346 corrects the policy without touching the
//! strict primitive. This file is the measurement that the correction works, plus the
//! inversion rows that prove the two profiles did not merge.
//!
//! Both references are pinned to executed output, not to a source reading:
//!
//! - `red25519-java-differential.json` — `i2p/i2p.i2p@93eef5db…`, the unmodified
//!   `net.i2p.crypto.eddsa` subtree compiled with `javac` and driven by an independent
//!   harness;
//! - `red25519-i2pd-differential.json` — `PurpleI2P/i2pd@2c694149…`, the unmodified
//!   `libi2pd` linked into an oracle binary.
//!
//! The two fixtures deliberately share one blinded scalar, one blinded public key, and
//! one signed message, so a passing row isolates the transcript and nothing else.
//!
//! # What "the deployed profile" means here
//!
//! `i2pr_crypto::red25519_deployed` implements the randomized RedDSA form the Encrypted
//! LeaseSet2 specification describes. Its challenge hash is
//! `SHA-512(R || A || M) mod L` — which is exactly the plain Ed25519 challenge — so
//! `ed25519_dalek`'s verifier stands in for "the verifier a stock Java I2P or i2pd router
//! runs", and a reference signature satisfying it is a reference signature i2pr can read.
//! The two profiles are nonetheless disjoint on the *signing* side, which is what the
//! strict rows below pin.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use i2pr_crypto::red25519::{
    BlindedPrivateScalar, Red25519PublicKey, Red25519Signature, verify_blinded,
};
use i2pr_crypto::red25519_deployed as deployed;
use rand_core::SeedableRng;

const JAVA: &str = include_str!("data/red25519-java-differential.json");
const I2PD: &str = include_str!("data/red25519-i2pd-differential.json");

fn decode(text: &str, length: usize) -> Vec<u8> {
    assert_eq!(text.len(), length * 2, "unexpected hex length");
    (0..length)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

fn decode32(text: &str) -> [u8; 32] {
    decode(text, 32).try_into().expect("32 bytes")
}

fn decode64(text: &str) -> [u8; 64] {
    decode(text, 64).try_into().expect("64 bytes")
}

fn field(case: &serde_json::Value, name: &str) -> String {
    case[name]
        .as_str()
        .unwrap_or_else(|| panic!("{name}"))
        .to_string()
}

fn message_of(case: &serde_json::Value) -> Vec<u8> {
    let hex = field(case, "signed_message");
    decode(&hex, hex.len() / 2)
}

fn checked_cases(document: &str, schema: &str, revision: &str) -> Vec<serde_json::Value> {
    let parsed: serde_json::Value = serde_json::from_str(document).expect("fixture parses");
    assert_eq!(
        parsed["schema"].as_str(),
        Some(schema),
        "the fixture schema changed; re-measure the reference before trusting the rows below"
    );
    assert_eq!(
        parsed["provenance"]["revision"].as_str(),
        Some(revision),
        "the differential is pinned to the reviewed reference revision"
    );
    parsed["cases"]
        .as_array()
        .expect("cases array")
        .clone()
        .into_iter()
        .filter(|case| case["reference_signature_verified"] == true)
        .collect()
}

fn java_cases() -> Vec<serde_json::Value> {
    checked_cases(
        JAVA,
        "i2pr-red25519-java-differential/1",
        "93eef5db87fae48025de00c0eb9b669e97b92149",
    )
}

fn i2pd_cases() -> Vec<serde_json::Value> {
    checked_cases(
        I2PD,
        "i2pr-red25519-i2pd-differential/1",
        "2c694149fa6996eaeb23e378d5f83c9d3232c22f",
    )
}

/// The blinded scalar shared by fixture case 0 of both references, as recorded by the i2pd
/// oracle. It is the one committed private value in the corpus; the Java fixture
/// deliberately omits it because Java signs with a random nonce and could not reproduce the
/// signature anyway.
const SHARED_BLINDED_SCALAR: &str =
    "d76a9e48851444537a014ebe40d99b518e8bb1f109192708f63a2d2c68e1a20b";

fn shared_blinded_key() -> BlindedPrivateScalar {
    BlindedPrivateScalar::from_bytes(decode32(SHARED_BLINDED_SCALAR)).expect("canonical scalar")
}

fn nonce(byte: u8) -> [u8; 80] {
    [byte; 80]
}

/// The Plan-346 headline row: the deployed profile reads a stock Java I2P type-11 signature.
#[test]
fn the_deployed_profile_verifies_the_pinned_java_signature() {
    for case in java_cases() {
        let key = Red25519PublicKey::from_bytes(decode32(&field(&case, "blinded_public_key")));
        let message = message_of(&case);
        let signature = Red25519Signature::from_bytes(decode64(&field(&case, "signature")));

        deployed::verify_blinded(&key, &message, &signature).expect(
            "the ELS2 deployed profile must accept a stock Java I2P type-11 signature; if it \
             does not, Plan 346's correction is wrong and the Plan 335 boundary still stands",
        );
    }
}

/// The same row against i2pd, across every case the oracle recorded.
#[test]
fn the_deployed_profile_verifies_every_pinned_i2pd_signature() {
    for case in i2pd_cases() {
        let key = Red25519PublicKey::from_bytes(decode32(&field(&case, "blinded_public_key")));
        let message = message_of(&case);
        let signature = Red25519Signature::from_bytes(decode64(&field(&case, "signature")));

        deployed::verify_blinded(&key, &message, &signature)
            .expect("the ELS2 deployed profile must accept a stock i2pd type-11 signature");
    }
}

/// i2pr → reference, the other direction.
///
/// An i2pr deployed signature satisfies the plain Ed25519 equation, which is the equation
/// `RedDSAEngine` inherits and the one i2pd runs. This is the row that makes the correction
/// useful rather than merely permissive: a stock reference router can read a record i2pr
/// produced.
#[test]
fn an_i2pr_deployed_signature_verifies_under_a_plain_ed25519_verifier() {
    for case in i2pd_cases() {
        let blinded_public = decode32(&field(&case, "blinded_public_key"));
        let message = message_of(&case);
        // Each i2pd case records its own blinded private scalar, so this row signs with the
        // key the row is about rather than with a shared stand-in.
        let blinded_key =
            BlindedPrivateScalar::from_bytes(decode32(&field(&case, "blinded_private_key")))
                .expect("canonical blinded scalar");
        assert_eq!(
            i2pr_crypto::red25519::derive_blinded_public_key(&blinded_key)
                .as_bytes()
                .to_vec(),
            blinded_public.to_vec(),
            "i2pr must re-derive the reference's blinded public key from its own blinded \
             scalar; if this fails the differential is not isolating the transcript"
        );

        let ours = deployed::sign_with_nonce(&blinded_key, &message, &nonce(0x77)).expect("sign");

        VerifyingKey::from_bytes(&blinded_public)
            .expect("plain key")
            .verify(&message, &Signature::from_bytes(ours.as_bytes()))
            .expect(
                "an i2pr deployed-transcript signature must satisfy the plain Ed25519 \
                 equation a stock Java I2P / i2pd router verifies; if it does not, publishing a \
                 type-5 record still would not interoperate",
            );
    }
}

/// The reference control row both directions of Plan 335 already established, kept here so
/// the correction cannot quietly be read as "the references were wrong".
#[test]
fn the_references_still_agree_with_each_other() {
    for case in java_cases() {
        let blinded_public = decode32(&field(&case, "blinded_public_key"));
        let message = message_of(&case);
        let reference = decode64(&field(&case, "signature"));

        VerifyingKey::from_bytes(&blinded_public)
            .expect("plain key")
            .verify(&message, &Signature::from_bytes(&reference))
            .expect("Java and i2pd verify each other's type-11 signatures");
    }
}

/// Inversion row 1: the strict primitive still rejects every deployed signature.
///
/// This is the load-bearing negative. A deployed signature passing `verify_blinded` would
/// mean the two transcripts had merged and the strict primitive had been silently
/// redefined, which Plan 346 forbids and which would invalidate every official Red25519
/// vector as evidence.
#[test]
fn the_strict_primitive_rejects_every_deployed_signature() {
    for case in java_cases().into_iter().chain(i2pd_cases()) {
        let key = Red25519PublicKey::from_bytes(decode32(&field(&case, "blinded_public_key")));
        let message = message_of(&case);
        let reference = Red25519Signature::from_bytes(decode64(&field(&case, "signature")));

        assert!(
            verify_blinded(&key, &message, &reference).is_err(),
            "a deployed type-11 signature must NOT satisfy the Proposal-146 equation; if it \
             does, the strict primitive has been redefined and the official vector corpus no \
             longer means what Plan 346 assumes",
        );
    }
}

/// Inversion row 2: the deployed profile rejects every strict signature.
///
/// Symmetric to the row above, and the one that would catch a "just try both" verifier
/// leaking into the crypto layer. The bounded ELS2 owner is the only place both are tried,
/// and it does so under an explicit policy rather than as a fallback.
#[test]
fn the_deployed_profile_rejects_a_strict_signature() {
    let blinded_public = i2pr_crypto::red25519::derive_blinded_public_key(&shared_blinded_key());
    let message = message_of(&i2pd_cases()[0]);
    let strict =
        i2pr_crypto::red25519::sign_with_nonce(&shared_blinded_key(), &message, &nonce(0x5a))
            .expect("strict sign");

    assert!(
        deployed::verify_blinded(&blinded_public, &message, &strict).is_err(),
        "the deployed profile must NOT accept a Proposal-146 signature; if it does, the two \
         transcripts are not disjoint and the Plan 346 classification is unsound",
    );
    verify_blinded(&blinded_public, &message, &strict)
        .expect("the strict primitive still accepts its own signature");
}

/// The deployed profile is not a wildcard: a wrong message and a tampered signature all
/// fail. A verifier that accepted anything would satisfy the interoperability rows above
/// for the wrong reason.
#[test]
fn the_deployed_profile_rejects_a_wrong_message_or_tampered_body() {
    let message = message_of(&i2pd_cases()[0]);
    let good =
        deployed::sign_with_nonce(&shared_blinded_key(), &message, &nonce(0x11)).expect("sign");
    let key = i2pr_crypto::red25519::derive_blinded_public_key(&shared_blinded_key());
    deployed::verify_blinded(&key, &message, &good).expect("the real signature verifies");

    // Wrong message.
    let mut wrong = message.clone();
    wrong.push(0xff);
    assert!(
        deployed::verify_blinded(&key, &wrong, &good).is_err(),
        "a one-byte change to the signed region must invalidate the signature"
    );

    // Tampered signature: flipping any byte of R or S must invalidate it.
    for index in [0_usize, 31, 32, 63] {
        let mut bytes = *good.as_bytes();
        bytes[index] ^= 0x01;
        let tampered = Red25519Signature::from_bytes(bytes);
        assert!(
            deployed::verify_blinded(&key, &message, &tampered).is_err(),
            "a flipped byte at {index} must invalidate the deployed signature"
        );
    }
}

/// A different blinded key must not verify under the first key's signature.
#[test]
fn the_deployed_profile_rejects_the_wrong_blinded_key() {
    let message = message_of(&i2pd_cases()[0]);
    let signature =
        deployed::sign_with_nonce(&shared_blinded_key(), &message, &nonce(0x22)).expect("sign");

    let mut other_bytes = decode32(SHARED_BLINDED_SCALAR);
    // A canonical, different scalar: bump the first byte and keep it below L.
    other_bytes[0] = other_bytes[0].wrapping_add(0x10);
    let other = BlindedPrivateScalar::from_bytes(other_bytes).expect("canonical");
    let other_public = i2pr_crypto::red25519::derive_blinded_public_key(&other);
    assert_ne!(
        other_public.as_bytes(),
        i2pr_crypto::red25519::derive_blinded_public_key(&shared_blinded_key()).as_bytes(),
        "the two test keys must differ"
    );

    assert!(
        deployed::verify_blinded(&other_public, &message, &signature).is_err(),
        "a signature must not verify under a different blinded key"
    );
}

/// Randomized signing still requires a CSPRNG and still verifies every time.
///
/// The deployed transcript takes an 80-byte random nonce, the same shape as the strict
/// one. A profile that reused a fixed nonce would be a private-key leak, so the row is
/// about both halves: no repetition, and no rejection.
#[test]
fn deployed_signing_is_randomized_and_always_verifies() {
    let mut rng = rand_chacha::ChaCha20Rng::from_seed([0x5c_u8; 32]);
    let message = message_of(&i2pd_cases()[0]);
    let key = shared_blinded_key();
    let public = i2pr_crypto::red25519::derive_blinded_public_key(&key);

    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let signature = deployed::sign(&key, &message, &mut rng).expect("sign");
        assert!(
            seen.insert(*signature.as_bytes()),
            "deployed signing must not repeat a signature"
        );
        deployed::verify_blinded(&public, &message, &signature)
            .expect("every deployed signature must verify");
    }
}

/// A failing random source is a randomness error, never a silently deterministic signature.
#[test]
fn a_failed_random_source_is_reported_rather_than_downgraded() {
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

    let key = shared_blinded_key();
    let message = message_of(&i2pd_cases()[0]);
    assert_eq!(
        deployed::sign(&key, &message, &mut Failing).expect_err("must fail"),
        i2pr_crypto::red25519::Red25519Error::RandomnessUnavailable,
        "a broken CSPRNG must surface as a randomness error, not as a signature over a \
         zero/reused nonce"
    );
}

/// The deployed profile bounds its own message length.
///
/// Unlike the strict transcript there is no length frame to overflow, so this ceiling is an
/// i2pr resource bound. It is checked at the ceiling and one byte past it.
#[test]
fn the_deployed_profile_bounds_its_own_message_length() {
    let key = shared_blinded_key();
    let public = i2pr_crypto::red25519::derive_blinded_public_key(&key);
    let at_limit = vec![0x5a_u8; deployed::MAX_DEPLOYED_MESSAGE_LENGTH];
    let signature =
        deployed::sign_with_nonce(&key, &at_limit, &nonce(0x33)).expect("sign at limit");
    deployed::verify_blinded(&public, &at_limit, &signature).expect("verifies at the ceiling");

    let over = vec![0x5a_u8; deployed::MAX_DEPLOYED_MESSAGE_LENGTH + 1];
    assert_eq!(
        deployed::sign_with_nonce(&key, &over, &nonce(0x33)).expect_err("must refuse"),
        i2pr_crypto::red25519::Red25519Error::MessageTooLong {
            actual: over.len(),
            maximum: deployed::MAX_DEPLOYED_MESSAGE_LENGTH,
        }
    );
    assert_eq!(
        deployed::verify_blinded(&public, &over, &signature).expect_err("must refuse"),
        i2pr_crypto::red25519::Red25519Error::MessageTooLong {
            actual: over.len(),
            maximum: deployed::MAX_DEPLOYED_MESSAGE_LENGTH,
        }
    );
}

/// The common signature layer still has no type-11 path at all.
///
/// `i2pr_crypto::verify_signature` refuses anything that is not the router signing type,
/// so the deployed transcript is structurally unreachable from it — not merely unused.
/// This row is the runtime half of
/// `scripts/check-els2-type11-transcript-boundary.sh`.
#[test]
fn the_common_signature_layer_has_no_type11_path() {
    use i2pr_proto::{SignatureValue, SigningKeyType, SigningPublicKey};

    let message = message_of(&i2pd_cases()[0]);
    let signature =
        deployed::sign_with_nonce(&shared_blinded_key(), &message, &nonce(0x44)).expect("sign");
    let key = i2pr_crypto::red25519::derive_blinded_public_key(&shared_blinded_key());

    let signing_key =
        SigningPublicKey::new(SigningKeyType::RedDsaSha512Ed25519, key.as_bytes().to_vec())
            .expect("type 11 key");
    let value = SignatureValue::new(
        SigningKeyType::RedDsaSha512Ed25519,
        signature.as_bytes().to_vec(),
    )
    .expect("type 11 signature");

    assert!(
        i2pr_crypto::verify_signature(&signing_key, &message, &value).is_err(),
        "the generic signature layer must not verify a type-11 signature by any transcript; \
         a pass here would mean a dual-transcript verifier had been added to the common layer"
    );
}

/// A malformed signature is a non-match, not a panic and not a retryable error.
#[test]
fn a_malformed_signature_is_a_clean_non_match() {
    let key = i2pr_crypto::red25519::derive_blinded_public_key(&shared_blinded_key());
    let message = message_of(&i2pd_cases()[0]);
    for length in [0_usize, 1, 63, 65, 128] {
        assert_eq!(
            Red25519Signature::decode(&vec![0_u8; length]),
            Err(
                i2pr_crypto::red25519::Red25519Error::InvalidSignatureLength {
                    actual: length,
                    expected: 64,
                }
            ),
            "a {length}-byte signature must be refused on length alone"
        );
    }
    // A 64-byte value that is not a valid encoding is still a clean refusal.
    let garbage = Red25519Signature::from_bytes([0xff; 64]);
    assert!(
        deployed::verify_blinded(&key, &message, &garbage).is_err(),
        "an undecodable R must be refused, not accepted or panicked on"
    );
}
