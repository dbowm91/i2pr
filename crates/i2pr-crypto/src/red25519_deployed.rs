//! The **deployed** type-11 signature transcript: what Java I2P and i2pd actually compute.
//!
//! # Why this module exists separately from [`crate::red25519`]
//!
//! I2P has two written descriptions of a signature type 11. Proposal 146 and the standalone
//! Red25519 specification define
//!
//! ```text
//! HStar(m) = SHA-512("I2P_Red25519H(x)" || T || ENCODE_POINT(vk) || len_u16(m) || m)
//! ```
//!
//! while the Encrypted LeaseSet2 specification describes the randomized RedDSA form that the
//! two deployed routers have implemented since 2019:
//!
//! ```text
//! r = SHA-512(T || ENCODE_POINT(vk) || m) mod L
//! R = [r]B
//! c = SHA-512(R || ENCODE_POINT(vk) || m) mod L
//! S = (r + c*a) mod L
//! ```
//!
//! The two differ. The strict form has an explicit hash-domain separator and a prefix-free
//! two-byte message-length frame; the deployed form has neither. There is **no wire
//! discriminator**: a type-5 record names signature type 11 and carries a 64-byte signature,
//! and nothing on the wire says which of the two transcripts produced it.
//!
//! This module implements the deployed form. It does **not** redefine, relax, or replace the
//! strict primitive: `i2pr_crypto::red25519::{sign, verify, sign_with_nonce, verify_blinded}`
//! stay Proposal-146 strict and keep passing every official Red25519 vector byte for byte. The
//! two share their field, scalar, point, and equation code through `pub(crate)` helpers in
//! [`crate::red25519`] rather than duplicating curve arithmetic, so they cannot drift apart in
//! arithmetic even though they deliberately differ in transcript.
//!
//! # Scope discipline (ADR 0032, Plan 346)
//!
//! Nothing in this module is reachable from the common signature layer. The generic
//! [`crate::verify_signature`] path and `SigningKeyType::RedDsaSha512Ed25519` continue to mean
//! exactly one thing — Proposal-146 strict — because a generic "try both type-11 algorithms"
//! verifier would silently downgrade every non-ELS2 type-11 verification in the router, which
//! is the failure mode ADR 0032 exists to prevent. The only sanctioned caller of this module is
//! the encrypted-LeaseSet2 type-5 owner, which supplies the transcript with a bounded,
//! constructible-once signed region. `scripts/check-els2-type11-transcript-boundary.sh` is the
//! static guard for that claim.
//!
//! # Security posture
//!
//! The deployed transcript is a **compatibility profile, not equivalent security semantics**.
//! It omits the domain separator and length framing, so the bounded ELS2 owner — not this
//! module — is responsible for keeping the signed region typed, length-capped, and
//! caller-unselectable. This module still applies a hard message ceiling of its own so no
//! unbounded buffer reaches SHA-512 through this path.
//!
//! The implementation is written from the Encrypted LeaseSet2 specification text and from
//! measured reference output; no Java I2P or i2pd source is copied here.

use curve25519_dalek::{constants::ED25519_BASEPOINT_TABLE, scalar::Scalar};
use rand_core::TryCryptoRng;
use sha2::{Digest, Sha512};
use zeroize::Zeroize;

use crate::red25519::{
    BlindedPrivateScalar, PUBLIC_KEY_LENGTH, Red25519Error, Red25519PublicKey, Red25519Signature,
    SIGNATURE_LENGTH, SIGNING_NONCE_LENGTH, check_equation, public_bytes_of, split_signature,
};

/// Hard ceiling on the message this transcript will hash.
///
/// The deployed form has no length prefix, so the protocol itself imposes no encoding ceiling
/// here; this is an i2pr resource bound. It is deliberately generous relative to the largest
/// encrypted LeaseSet2 record (a few KiB) and exists so this path can never be used to hash an
/// unbounded buffer. Callers are additionally bounded by the ELS2 record ceiling before they
/// reach here.
pub const MAX_DEPLOYED_MESSAGE_LENGTH: usize = 65_534;

fn check_message_length(message: &[u8]) -> Result<(), Red25519Error> {
    if message.len() > MAX_DEPLOYED_MESSAGE_LENGTH {
        return Err(Red25519Error::MessageTooLong {
            actual: message.len(),
            maximum: MAX_DEPLOYED_MESSAGE_LENGTH,
        });
    }
    Ok(())
}

/// The deployed challenge scalar: `SHA-512(R || ENCODE_POINT(vk) || m) mod L`.
///
/// Note this is the same challenge hash as the strict transcript's second `HStar` call *except*
/// for the `I2P_Red25519H(x)` prefix and the length frame, so a signature made under one
/// transcript is not a valid signature under the other even though the equations agree.
fn challenge(r_bytes: &[u8; PUBLIC_KEY_LENGTH], public_bytes: &[u8], message: &[u8]) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(r_bytes);
    hasher.update(public_bytes);
    hasher.update(message);
    let digest: [u8; 64] = hasher.finalize().into();
    Scalar::from_bytes_mod_order_wide(&digest)
}

/// Signs a message with a blinded type-11 key under the deployed transcript.
///
/// Every signature is randomized: the same key over the same message yields different bytes.
/// The nonce must come from a CSPRNG; [`sign_with_nonce`] exists only so a differential test
/// can reproduce a transcript exactly and must never be used with a fixed nonce.
pub fn sign<R: TryCryptoRng + ?Sized>(
    blinded_key: &BlindedPrivateScalar,
    message: &[u8],
    rng: &mut R,
) -> Result<Red25519Signature, Red25519Error> {
    let mut nonce = [0_u8; SIGNING_NONCE_LENGTH];
    rng.try_fill_bytes(&mut nonce)
        .map_err(|_| Red25519Error::RandomnessUnavailable)?;
    let signature = sign_with_nonce(blinded_key, message, &nonce);
    nonce.zeroize();
    signature
}

/// Signs a message with a caller-supplied 80-byte nonce under the deployed transcript.
///
/// Test-only: a fixed or repeated nonce leaks the private key.
pub fn sign_with_nonce(
    blinded_key: &BlindedPrivateScalar,
    message: &[u8],
    nonce: &[u8; SIGNING_NONCE_LENGTH],
) -> Result<Red25519Signature, Red25519Error> {
    check_message_length(message)?;
    let public_bytes = public_bytes_of(blinded_key);
    let mut hasher = Sha512::new();
    hasher.update(nonce);
    hasher.update(public_bytes);
    hasher.update(message);
    let digest: [u8; 64] = hasher.finalize().into();
    let r = Scalar::from_bytes_mod_order_wide(&digest);
    let big_r = (ED25519_BASEPOINT_TABLE * &r).compress().to_bytes();
    let c = challenge(&big_r, &public_bytes, message);
    let s = r + c * blinded_key.as_scalar();
    let mut out = [0_u8; SIGNATURE_LENGTH];
    out[..PUBLIC_KEY_LENGTH].copy_from_slice(&big_r);
    out[PUBLIC_KEY_LENGTH..].copy_from_slice(&s.to_bytes());
    Ok(Red25519Signature::from_bytes(out))
}

/// Verifies a deployed-transcript type-11 signature for a blinded public key.
///
/// Returns a distinct error from the strict verifier so a caller cannot mistake one profile's
/// acceptance for the other's.
pub fn verify_blinded(
    blinded_key: &Red25519PublicKey,
    message: &[u8],
    signature: &Red25519Signature,
) -> Result<(), Red25519Error> {
    check_message_length(message)?;
    let public_point = blinded_key
        .as_point()
        .map_err(|_| Red25519Error::BlindedSignatureVerificationFailed)?;
    let Some(r_bytes) = split_signature(signature) else {
        return Err(Red25519Error::BlindedSignatureVerificationFailed);
    };
    // The transcript hashes ENCODE_POINT(vk), so it always uses the canonical encoding.
    let public_bytes = public_point.compress().to_bytes();
    let c = challenge(&r_bytes, &public_bytes, message);
    check_equation(public_point, &r_bytes, signature, &c)
        .map_err(|()| Red25519Error::BlindedSignatureVerificationFailed)
}
