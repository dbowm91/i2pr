//! The type-11 signature transcript is the whole interoperability boundary.
//!
//! A type-5 encrypted LeaseSet2 signature is verifiable by an implementation if and only if its
//! hash transcript matches. The pinned references — i2pd and Java I2P — sign type 11 with the
//! bare Zcash form (`SHA-512(T ‖ A ‖ M)`, then `SHA-512(R ‖ A ‖ M)`) and verify type 11 through a
//! **plain Ed25519 verifier**: i2pd's `RedDSA25519Verifier` is a `typedef` of `EDDSA25519Verifier`,
//! and Java's `RedDSAEngine` overrides only `digestInitSign`, never the verify path. i2pr follows
//! the specification's `I2P_Red25519H(x)` domain on both hashes.
//!
//! These rows pin the resulting *symmetric* incompatibility with no external build, so the
//! classification in ADR 0005 and the Plan 335 closure record cannot silently rot into a claim
//! that the references merely "do not implement the domain". They implement the domain fully —
//! blinding, alpha derivation, the storage key, and the ELS2 framing all agree. Only the
//! transcript differs.
//!
//! Row 1 is the direction that had no executable coverage: an i2pr specification-form signature is
//! not a plain Ed25519 signature, so no reference that verifies type 11 with the plain verifier can
//! accept it. Row 2 is the control that proves row 1 is about the transcript and nothing else: the
//! reference's own signature *is* a valid plain Ed25519 signature, which is exactly why i2pd's own
//! verifier accepts it.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use i2pr_crypto::red25519::{
    BlindedPrivateScalar, Red25519PublicKey, Red25519Signature, sign_with_nonce, verify_blinded,
};

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

fn field(case: &serde_json::Value, name: &str) -> String {
    case[name]
        .as_str()
        .unwrap_or_else(|| panic!("{name}"))
        .to_string()
}

fn first_case() -> serde_json::Value {
    let document: serde_json::Value = serde_json::from_str(DIFFERENTIAL).expect("fixture parses");
    assert_eq!(
        document["provenance"]["revision"].as_str(),
        Some("2c694149fa6996eaeb23e378d5f83c9d3232c22f"),
        "the differential is pinned to the reviewed reference revision"
    );
    document["cases"]
        .as_array()
        .expect("cases array")
        .first()
        .cloned()
        .expect("the differential must not be empty")
}

/// A deterministic nonce, so a failure here is reproducible rather than a one-in-2^640 event.
const NONCE: [u8; 80] = [0x5a; 80];

#[test]
fn an_i2pr_specification_signature_is_not_a_plain_ed25519_signature() {
    let case = first_case();
    let blinded_public =
        Red25519PublicKey::from_bytes(decode32(&field(&case, "blinded_public_key")));
    let scalar = BlindedPrivateScalar::from_bytes(decode32(&field(&case, "blinded_private_key")))
        .expect("reference blinded scalar is canonical");
    let message = b"type-11 transcript boundary probe";

    let ours = sign_with_nonce(&scalar, message, &NONCE).expect("i2pr signs");

    // i2pr's own verifier accepts it, so the signature is well formed. The rejection below is
    // therefore about the verifier's transcript, not about a malformed signature.
    verify_blinded(&blinded_public, message, &ours)
        .expect("i2pr must accept its own specification-form signature");

    // This is the direction the repository never executed: the references verify type 11 with a
    // plain Ed25519 verifier, so a specification-form signature must be rejected by one.
    let verifying_key = VerifyingKey::from_bytes(blinded_public.as_bytes()).expect("plain key");
    let signature = Signature::from_bytes(ours.as_bytes());
    assert!(
        verifying_key.verify(message, &signature).is_err(),
        "an i2pr specification-form type-11 signature must NOT verify under a plain Ed25519 \
         verifier; if it now does, the references' type-11 verifier would accept i2pr records and \
         this classified divergence must be re-evaluated"
    );
}

#[test]
fn the_reference_signature_is_a_plain_ed25519_signature() {
    let case = first_case();
    let blinded_public = decode32(&field(&case, "blinded_public_key"));
    let message_hex = field(&case, "signed_message");
    let message = decode(&message_hex, message_hex.len() / 2);
    let reference_bytes: [u8; 64] = decode(&field(&case, "signature"), 64)
        .as_slice()
        .try_into()
        .expect("64 bytes");
    let reference = Signature::from_bytes(&reference_bytes);

    // i2pd routes type 11 through its plain Ed25519 verifier, so a signature it accepts is a
    // valid plain Ed25519 signature. This is the mechanism behind the divergence, and it is
    // observable from committed reference bytes with no reference build.
    VerifyingKey::from_bytes(&blinded_public)
        .expect("plain key")
        .verify(&message, &reference)
        .expect("the reference's own signature must be a valid plain Ed25519 signature");
}

#[test]
fn the_reference_signature_does_not_verify_under_the_specification_equation() {
    let case = first_case();
    let blinded_public =
        Red25519PublicKey::from_bytes(decode32(&field(&case, "blinded_public_key")));
    let message_hex = field(&case, "signed_message");
    let message = decode(&message_hex, message_hex.len() / 2);
    let reference = Red25519Signature::decode(&decode(&field(&case, "signature"), 64))
        .expect("reference signature");

    assert!(
        verify_blinded(&blinded_public, &message, &reference).is_err(),
        "the reference signature must NOT verify under the specification equation; if it now does, \
         the reference gained specification conformance and the Plan 335 classification must be \
         re-evaluated"
    );
}
