//! Plan 346: the bounded type-11 signature-profile boundary for encrypted LeaseSet2.
//!
//! # The problem this owns
//!
//! I2P has two written descriptions of a signature type 11 and no wire discriminator between
//! them. Proposal 146 / the standalone Red25519 specification defines
//!
//! ```text
//! HStar(m) = SHA-512("I2P_Red25519H(x)" || T || ENCODE_POINT(vk) || len_u16(m) || m)
//! ```
//!
//! and the Encrypted LeaseSet2 specification describes the randomized RedDSA form that Java I2P
//! and i2pd have both implemented since 2019, which is what [`i2pr_crypto::red25519_deployed`]
//! computes. A type-5 record says "signature type 11" and carries 64 bytes; it never says which
//! transcript produced them. Plan 335 measured that the two families reject each other.
//!
//! # What this module does about it
//!
//! It makes the choice **typed, bounded, and owned**, rather than leaving it implicit in a
//! generic verifier:
//!
//! - the profile is an enum, [`Els2Type11Profile`], not a boolean;
//! - inbound verification classifies a signature as [`Deployed`], [`Strict`], [`None`], or
//!   [`Ambiguous`], and the caller **fails closed** on [`Ambiguous`] instead of picking one;
//! - outbound signing always uses [`Deployed`], because that is the only form a stock Java I2P
//!   or i2pd router can verify;
//! - the signed region is an [`Els2SignedRegion`], which can only be built from a real type-5
//!   record or offline-key block, so no application message and no transcript selector can
//!   reach this path;
//! - nothing here is reachable from `SigningKeyType::RedDsaSha512Ed25519` in the common
//!   signature layer. `scripts/check-els2-type11-transcript-boundary.sh` enforces that
//!   statically.
//!
//! # Security posture (ADR 0032)
//!
//! The deployed transcript lacks Proposal 146's domain separator and prefix-free length
//! framing. That is a real, accepted compatibility tradeoff and **not** equivalent security
//! semantics. The compensating constraints are structural rather than advisory: the signed
//! region is constructible only from a decoded type-5 structure, the record length is bounded
//! before hashing, the transcript is never selected by network or I2PControl input, and no
//! retry or downgrade happens outside this module.

use i2pr_crypto::TryCryptoRng;
use i2pr_crypto::red25519::{
    BlindedPrivateScalar, Red25519Error, Red25519PublicKey, Red25519Signature,
};
use i2pr_proto::Els2SignedRegion;

use crate::els2::Els2Error;

/// Which type-11 transcript an encrypted LeaseSet2 signature was made under.
///
/// This is a four-state classification rather than a success flag because "no transcript
/// matched" and "both matched" mean different things and must not collapse into one boolean.
/// Only the first two are acceptance states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Els2Type11Profile {
    /// The deployed Java I2P / i2pd Encrypted-LeaseSet2 transcript
    /// (`H*(T || publicKey || message)`, EdDSA verification). This is what i2pr emits and
    /// what a stock reference router accepts.
    Deployed,
    /// The Proposal-146 strict transcript (`I2P_Red25519H(x)` domain plus two-byte length
    /// framing). Accepted **only here**, so that records already created under the strict
    /// policy and the independent Emissary oracle stay parseable.
    Strict,
    /// Neither transcript verified: a malformed signature, a wrong key, a wrong message, a
    /// wrong day, or a tampered body.
    None,
    /// Both transcripts verified. This must not happen for a well-formed transcript pair and
    /// is treated as a hard rejection: i2pr refuses the record rather than choosing.
    Ambiguous,
}

impl Els2Type11Profile {
    /// Returns whether this outcome accepts a record.
    pub const fn is_accepted(&self) -> bool {
        matches!(self, Self::Deployed | Self::Strict)
    }

    /// Returns the stable evidence token for this outcome, for logs and evidence rows.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Deployed => "deployed",
            Self::Strict => "strict",
            Self::None => "none",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// Signs a type-5 region with a blinded type-11 key under the deployed profile.
///
/// This is the only way i2pr produces a type-5 outer signature. The strict transcript is
/// deliberately not offered here: a publisher that could choose its transcript would make the
/// on-wire profile a function of local configuration instead of a fixed interoperability
/// policy, and a reference router would silently stop being able to read the record.
///
/// Randomized: the nonce comes from the injected CSPRNG, never from a fixed value.
pub fn sign_type11_deployed<R: TryCryptoRng + ?Sized>(
    blinded_key: &BlindedPrivateScalar,
    region: Els2SignedRegion<'_>,
    rng: &mut R,
) -> Result<Red25519Signature, Els2Error> {
    if region.len() > crate::els2::MAX_ELS2_RECORD_LENGTH {
        return Err(Els2Error::Blinding(Red25519Error::MessageTooLong {
            actual: region.len(),
            maximum: crate::els2::MAX_ELS2_RECORD_LENGTH,
        }));
    }
    i2pr_crypto::red25519_deployed::sign(blinded_key, region.as_bytes(), rng)
        .map_err(Els2Error::Blinding)
}

/// Classifies a type-5 signature against both type-11 transcripts.
///
/// Returns [`Els2Type11Profile::Ambiguous`] if the signature satisfies both equations. The
/// caller must reject that case; [`verify_type11`] is the fail-closed wrapper that does.
///
/// A signature that is not exactly 64 bytes classifies as [`Els2Type11Profile::None`] rather
/// than raising, because a wrong-length signature is a non-match, not a transport error worth
/// retrying.
pub fn classify_type11(
    blinded_public_key: &Red25519PublicKey,
    region: Els2SignedRegion<'_>,
    signature: &[u8],
) -> Els2Type11Profile {
    let Ok(parsed) = Red25519Signature::decode(signature) else {
        return Els2Type11Profile::None;
    };
    let deployed = i2pr_crypto::red25519_deployed::verify_blinded(
        blinded_public_key,
        region.as_bytes(),
        &parsed,
    )
    .is_ok();
    let strict =
        i2pr_crypto::red25519::verify_blinded(blinded_public_key, region.as_bytes(), &parsed)
            .is_ok();
    match (deployed, strict) {
        (true, true) => Els2Type11Profile::Ambiguous,
        (true, false) => Els2Type11Profile::Deployed,
        (false, true) => Els2Type11Profile::Strict,
        (false, false) => Els2Type11Profile::None,
    }
}

/// Which signature form a validated type-5 record's *own* outer signature used.
///
/// A type-5 record is signed either by its blinded type-11 key, or — when the offline-key
/// flag is set — by a transient key the blinded key delegated to. The two cases reach the
/// type-11 policy differently, so they are reported separately instead of collapsing
/// `None` to mean both "type 7" and "no transcript matched".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Els2RecordSignatureProfile {
    /// A type-11 signature, accepted under the bounded policy. The inner value records
    /// which of the two transcripts it satisfied.
    Type11(Els2Type11Profile),
    /// A type-7 Ed25519 transient-key signature whose delegation was itself accepted
    /// under the bounded type-11 policy. The type-7 algorithm is ordinary Ed25519 and
    /// has no second definition, so there is no transcript to name.
    Ed25519Transient,
}

impl Els2RecordSignatureProfile {
    /// Returns the stable evidence token for this outcome.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Type11(profile) => profile.as_str(),
            Self::Ed25519Transient => "ed25519-transient",
        }
    }
}

/// Verifies a type-5 signature, failing closed on an ambiguous transcript match.
///
/// Returns the accepted profile so the ELS2 owner can record which form a record used. Any
/// other outcome is [`Els2Error::Type11SignatureRejected`], which the type-5 validator maps to
/// its own `InvalidSignature` so the wire-visible error does not change shape.
pub fn verify_type11(
    blinded_public_key: &Red25519PublicKey,
    region: Els2SignedRegion<'_>,
    signature: &[u8],
) -> Result<Els2Type11Profile, Els2Error> {
    match classify_type11(blinded_public_key, region, signature) {
        profile if profile.is_accepted() => Ok(profile),
        _ => Err(Els2Error::Type11SignatureRejected),
    }
}
