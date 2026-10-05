//! The type-11 transcript divergence, measured against the pinned Java I2P engine.
//!
//! `red25519_plain_ed25519_divergence.rs` pins the boundary with i2pd's committed bytes. This
//! file pins the same boundary with **executed** Java I2P output, so the Java half of the Plan 335
//! live lane rests on a measurement rather than on a source reading.
//!
//! What was executed on 2026-10-05, against `i2p/i2p.i2p` at
//! `93eef5db87fae48025de00c0eb9b669e97b92149`: the pinned
//! `net.i2p.crypto.eddsa` subtree was compiled unmodified with `javac` and driven by an
//! independently written harness that builds the type-11 key the way I2P's own
//! `SigUtil.cvtToJavaEdDSAKey` does. The only two helpers supplied by that harness are
//! `net.i2p.util.RandomSource` and `net.i2p.data.DataHelper` — the subtree calls
//! `RandomSource.getInstance().nextBytes(t)` for the Zcash nonce and
//! `DataHelper.eqCT` to compare R, and nothing else outside itself and the JDK.
//!
//! The measured result, on one key and one message shared with the i2pd fixture:
//!
//! | verifier \ signature | i2pr (spec form) | i2pd | Java I2P |
//! |---|---|---|---|
//! | i2pr  | ACCEPT         | REJECT | REJECT |
//! | i2pd  | REJECT         | ACCEPT | ACCEPT |
//! | Java | REJECT         | ACCEPT | ACCEPT |
//!
//! Two consequences are recorded here. First, Java I2P and i2pd are **interchangeable**: they
//! verify each other's type-11 signatures. Second, the reference engine is byte-for-byte a *Zcash*
//! RedDSA implementation, which is what its own class comment cites, and I2P's Red25519 is that
//! plus the `I2P_Red25519H(x)` domain and 2-byte length framing. So the divergence is a
//! construction mismatch with a name collision behind it, not an i2pr defect and not a missing
//! feature in either reference.
//!
//! Because every encrypted LeaseSet2 record carries its outer signature under the blinded key,
//! whose sigtype is always 11, no specification-conformant type-5 record can be verified by
//! either reference. That is the localized blocker in the Plan 335 closure record.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use i2pr_crypto::red25519::{Red25519PublicKey, Red25519Signature, verify_blinded};

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

fn field(case: &serde_json::Value, name: &str) -> String {
    case[name]
        .as_str()
        .unwrap_or_else(|| panic!("{name}"))
        .to_string()
}

fn first_case(document: &str, schema: &str, revision: &str) -> serde_json::Value {
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
        .first()
        .cloned()
        .expect("the differential must not be empty")
}

fn java_case() -> serde_json::Value {
    first_case(
        JAVA,
        "i2pr-red25519-java-differential/1",
        "93eef5db87fae48025de00c0eb9b669e97b92149",
    )
}

fn i2pd_case() -> serde_json::Value {
    first_case(
        I2PD,
        "i2pr-red25519-i2pd-differential/1",
        "2c694149fa6996eaeb23e378d5f83c9d3232c22f",
    )
}

/// Java I2P derives the same blinded public key as i2pd and as i2pr from the same blinded scalar.
///
/// This is the row that keeps the classification honest in the other direction. The references are
/// not missing Red25519: blinding agrees byte for byte across all three implementations, which is
/// why the divergence is narrow enough to call a transcript mismatch rather than an absent feature.
#[test]
fn java_reproduces_the_same_blinded_public_key_as_i2pd() {
    let java = java_case();
    let i2pd = i2pd_case();

    assert_eq!(
        field(&java, "blinded_public_key"),
        field(&i2pd, "blinded_public_key"),
        "Java I2P and i2pd must derive the same blinded public key from the same blinded scalar"
    );
    assert_eq!(
        field(&java, "signed_message"),
        field(&i2pd, "signed_message"),
        "the two fixtures must share one signed message, so that only the signature can differ"
    );
    // Blinding is not the disagreement. A and the message are identical; the transcripts are not.
    assert_ne!(
        field(&java, "signature"),
        field(&i2pd, "signature"),
        "two references signing the same key and message with different transcripts must produce \
         different signatures"
    );
}

/// Java's RedDSA signature is a valid **plain Ed25519** signature.
///
/// `RedDSAEngine` overrides `digestInitSign` and nothing else, so its verify path is the inherited
/// `EdDSAEngine` challenge hash. A signature that engine accepts must therefore satisfy the plain
/// Ed25519 equation — which is the mechanism, and is why Java accepts i2pd's type-11 signature.
#[test]
fn a_java_reddsa_signature_is_a_plain_ed25519_signature() {
    let case = java_case();
    let blinded_public = decode32(&field(&case, "blinded_public_key"));
    let message_hex = field(&case, "signed_message");
    let message = decode(&message_hex, message_hex.len() / 2);
    let signature: [u8; 64] = decode(&field(&case, "signature"), 64)
        .as_slice()
        .try_into()
        .expect("64 bytes");

    VerifyingKey::from_bytes(&blinded_public)
        .expect("plain key")
        .verify(&message, &Signature::from_bytes(&signature))
        .expect(
            "Java's own type-11 signature must satisfy the plain Ed25519 equation; if it does \
                 not, Java is applying a nonstandard verify path and the Plan 335 classification \
                 must be re-evaluated",
        );
}

/// Java's type-11 signature does not verify under the specification's equation, and i2pr rejects it.
///
/// This is the executed form of the Plan 335 blocker. `verify_blinded` is the specification's
/// `I2P_Red25519H(x)` construction; a reference that signs with the bare Zcash transcript cannot
/// satisfy it, and therefore no specification-conformant type-5 record is verifiable by Java I2P.
#[test]
fn a_java_reddsa_signature_does_not_verify_under_the_specification_equation() {
    let case = java_case();
    let blinded_public =
        Red25519PublicKey::from_bytes(decode32(&field(&case, "blinded_public_key")));
    let message_hex = field(&case, "signed_message");
    let message = decode(&message_hex, message_hex.len() / 2);
    let reference = Red25519Signature::decode(&decode(&field(&case, "signature"), 64))
        .expect("reference signature");

    assert!(
        verify_blinded(&blinded_public, &message, &reference).is_err(),
        "Java's type-11 signature must NOT verify under the specification equation; if it now \
         does, Java I2P gained specification conformance and the Plan 335 blocked status must be \
         re-evaluated rather than carried forward"
    );
}

/// The divergence is symmetric across both references, and neither can read an i2pr record.
///
/// Java's signature is a plain Ed25519 signature that the specification equation rejects, and
/// i2pd's is the same. Row 1 of `red25519_plain_ed25519_divergence.rs` establishes the other
/// direction — that an i2pr signature is not a plain Ed25519 signature — so together the two files
/// cover every cell of the three-way matrix without needing the reference builds in CI.
#[test]
fn both_references_sign_with_one_transcript_and_i2pr_with_another() {
    for case in [java_case(), i2pd_case()] {
        let blinded_public = decode32(&field(&case, "blinded_public_key"));
        let message_hex = field(&case, "signed_message");
        let message = decode(&message_hex, message_hex.len() / 2);
        let reference: [u8; 64] = decode(&field(&case, "signature"), 64)
            .as_slice()
            .try_into()
            .expect("64 bytes");

        assert!(
            VerifyingKey::from_bytes(&blinded_public)
                .expect("plain key")
                .verify(&message, &Signature::from_bytes(&reference))
                .is_ok(),
            "a reference type-11 signature must satisfy the plain Ed25519 equation"
        );

        let specification = Red25519PublicKey::from_bytes(blinded_public);
        let parsed = Red25519Signature::decode(&reference).expect("reference signature");
        assert!(
            verify_blinded(&specification, &message, &parsed).is_err(),
            "a reference type-11 signature must NOT satisfy the specification equation"
        );
    }
}
