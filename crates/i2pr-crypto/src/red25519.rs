//! Independent i2pr implementation of the I2P Red25519 signature scheme.
//!
//! Red25519 is the RedDSA instantiation that I2P uses for signature type 11 and for the
//! blinded keys of encrypted LeaseSet2 records. This module owns only the I2P-specific
//! composition — domain separation, the `HStar` transcript hash, alpha derivation, key
//! re-randomization, and sign/verify — over the reviewed `curve25519-dalek` curve arithmetic.
//! No field, scalar, point, or bignum operation is implemented locally.
//!
//! Everything here is derived from the frozen normative sources recorded in
//! `specs/references/red25519-clean-room-freeze.md` and the algorithm worksheet in
//! `specs/references/red25519-algorithm-worksheet.md`. That module deliberately owns nothing
//! else: destination lifecycle, base32/base33 addressing, LeaseSet framing, NetDB storage,
//! and Proposal 170 control mapping all belong to their own owners.
//!
//! Secret material ([`Red25519PrivateScalar`], [`BlindingScalar`], [`BlindedPrivateScalar`]) is
//! held in fixed-size non-cloneable wrappers that erase on drop and never implement
//! `Debug`, `Display`, serde, or a general-purpose byte accessor. Randomness is always supplied
//! by the caller through [`rand_core::TryCryptoRng`]; a random-source failure is reported as
//! [`Red25519Error::RandomnessUnavailable`] and is never conflated with a protocol rejection.

use curve25519_dalek::{
    constants::ED25519_BASEPOINT_TABLE, edwards::CompressedEdwardsY, edwards::EdwardsPoint,
    scalar::Scalar,
};
use i2pr_proto::{Hash, SigningKeyType, SigningPublicKey};
use rand_core::TryCryptoRng;
use sha2::{Digest, Sha256, Sha512};
use subtle::ConstantTimeEq;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// The blinded signing key type every I2P blinded key uses: RedDSA over Ed25519.
pub const BLINDED_SIGNING_KEY_TYPE: SigningKeyType = SigningKeyType::RedDsaSha512Ed25519;
/// The only unblinded signing key types that may be blinded for encrypted LeaseSet2 use.
pub const ED25519_SIGNING_KEY_TYPE: SigningKeyType = SigningKeyType::EdDsaSha512Ed25519;

/// Encoded length of a Red25519 private scalar, public key, and HKDF/salt input.
pub const SCALAR_LENGTH: usize = 32;
/// Encoded length of a Red25519 public key.
pub const PUBLIC_KEY_LENGTH: usize = 32;
/// Encoded length of a Red25519 signature.
pub const SIGNATURE_LENGTH: usize = 64;
/// Length of the SHA-256 outputs used by the alpha, credential, and storage-key derivations.
pub const HASH_LENGTH: usize = 32;
/// Length of the UTF-8 `YYYYMMDD` day string that seeds alpha derivation.
pub const BLINDING_DAY_LENGTH: usize = 8;
/// Number of random bytes Red25519 signing mixes into its nonce.
pub const SIGNING_NONCE_LENGTH: usize = 80;
/// The Red25519 `HStar` domain literal, prepended to every hashed transcript.
pub const HSTAR_PREFIX: &[u8] = b"I2P_Red25519H(x)";
/// HKDF `info` label for daily alpha derivation.
pub const ALPHA_HKDF_INFO: &[u8] = b"i2pblinding1";
/// Personalization string for the alpha HKDF salt.
pub const ALPHA_SALT_PERSONALIZATION: &[u8] = b"I2PGenerateAlpha";

/// Largest message the `HStar` length prefix can encode. The specification reserves the
/// remaining length value for a future extension.
pub const MAX_MESSAGE_LENGTH: usize = 65534;

/// Earliest UTC year accepted for a [`BlindingDay`].
///
/// The blinding epoch is not older than the Unix epoch because the day string is the only time
/// input the derivation takes, and a pre-epoch day has no protocol meaning.
pub const MIN_BLINDING_YEAR: u16 = 1970;
/// Latest UTC year accepted for a [`BlindingDay`], matching the four-digit `YYYYMMDD` field.
pub const MAX_BLINDING_YEAR: u16 = 9999;

/// Largest optional lookup-secret string accepted by [`generate_alpha`].
///
/// This is an i2pr resource bound rather than a protocol requirement: the specification defines
/// no maximum. It keeps a control-plane-provided secret from becoming an unbounded KDF input
/// and stays comfortably above any realistic operator secret.
pub const MAX_LOOKUP_SECRET_LENGTH: usize = 256;

/// Errors returned by the Red25519 composition.
///
/// Protocol invalidity and randomness failure are deliberately distinct: a caller that retries
/// on [`Red25519Error::RandomnessUnavailable`] must not also retry on an invalid key, an
/// oversized message, or a failed verification.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum Red25519Error {
    /// The injected cryptographic random source failed.
    #[error("Red25519 randomness unavailable")]
    RandomnessUnavailable,
    /// A message exceeded the `HStar` length-prefix ceiling.
    #[error("Red25519 message length {actual} exceeds ceiling {maximum}")]
    MessageTooLong {
        /// Actual message length.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// A 32-byte value was not a canonical point encoding.
    #[error("invalid Red25519 public key encoding")]
    InvalidPublicKey,
    /// A point has small order, including the identity and the seven non-trivial torsion points.
    ///
    /// Small-order points are rejected wherever a key is derived, blinded, or used as blinding
    /// input. Verification deliberately does not apply this check: the verification equation
    /// multiplies by the cofactor, so such a key cannot forge a valid signature.
    #[error("Red25519 point has small order and cannot be blinded")]
    SmallOrderPoint,
    /// A point was on the curve but outside the prime-order subgroup.
    ///
    /// Blinded keys are required to lie in the prime-order subgroup, so a mixed-order point is
    /// refused rather than re-randomized into something whose subgroup membership depends on
    /// the unknown torsion component.
    #[error("Red25519 point is not in the prime-order subgroup")]
    NotPrimeOrderPoint,
    /// A scalar was not a canonical little-endian residue modulo `L`.
    #[error("Red25519 scalar is not a canonical residue modulo L")]
    NonCanonicalScalar,
    /// A converted type-7 scalar was not in the specification's clamped shape.
    #[error("converted Ed25519 scalar is not in the clamped specification shape")]
    NonCanonicalConvertedScalar,
    /// A signature was not exactly 64 bytes.
    #[error("Red25519 signature length {actual} is not {expected}")]
    InvalidSignatureLength {
        /// Actual signature length.
        actual: usize,
        /// Required length.
        expected: usize,
    },
    /// A signature did not verify against the supplied message and key.
    #[error("Red25519 signature verification failed")]
    SignatureVerificationFailed,
    /// A signature did not verify against the supplied re-randomized key.
    #[error("Red25519 signature failed blinded verification")]
    BlindedSignatureVerificationFailed,
    /// The unblinded signature type cannot be blinded.
    #[error("unsupported unblinded signing key type {algorithm} for Red25519 blinding")]
    UnsupportedSigType {
        /// Numeric protocol algorithm identifier.
        algorithm: u16,
    },
    /// A blinding day was not a valid UTC calendar date.
    #[error("invalid UTC blinding day: {reason}")]
    InvalidBlindingDay {
        /// Static description of the rejected field.
        reason: &'static str,
    },
    /// A lookup secret exceeded the local ceiling.
    #[error("lookup secret length {actual} exceeds ceiling {maximum}")]
    LookupSecretTooLong {
        /// Actual secret length in bytes.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// A public key could not be represented in the common-structure signing key type.
    #[error("Red25519 public key rejected by the protocol type")]
    ProtocolKeyRejected,
    /// The key-derivation helper refused an internally bounded request.
    #[error("Red25519 key derivation failed")]
    KeyDerivationFailed,
}

/// A Red25519 private scalar: an unblinded (type 11) or converted (from type 7) secret key.
///
/// Fixed-size, non-cloneable, erased on drop, and free of formatting or serialization traits.
/// The single byte accessor is explicitly named as secret access.
///
/// The stored bytes are the little-endian representation the specification names, not
/// necessarily a canonical residue. `CONVERT_ED25519_PRIVATE` yields the clamped Ed25519
/// secret scalar, which is routinely `>= L`, so it is kept verbatim and reduced only inside
/// arithmetic. Every Red25519 operation on this value — derivation, blinding, signing — is
/// defined modulo `L`, so the distinction is not observable on the wire; it is preserved only
/// so a converted key is bit-identical to the published specification form.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Red25519PrivateScalar([u8; SCALAR_LENGTH]);

impl Red25519PrivateScalar {
    /// Loads a canonical private scalar from little-endian bytes.
    ///
    /// Used for keys that are canonical by construction: generated type-11 keys, and keys
    /// restored from private storage. An integer `>= L` is rejected rather than reduced, so a
    /// mangled or non-canonical secret never silently becomes a different key.
    pub fn from_bytes(bytes: [u8; SCALAR_LENGTH]) -> Result<Self, Red25519Error> {
        Option::<Scalar>::from(Scalar::from_canonical_bytes(bytes))
            .ok_or(Red25519Error::NonCanonicalScalar)?;
        Ok(Self(bytes))
    }

    /// Loads the exact output of `CONVERT_ED25519_PRIVATE` from a converted type-7 key.
    ///
    /// The value is accepted only in the specification's clamped shape (low three bits of the
    /// first byte clear, top two bits of the last byte `01`), which is what a conversion of an
    /// Ed25519 seed always produces. Use [`Self::from_bytes`] for every other key.
    pub fn from_converted_ed25519_bytes(bytes: [u8; SCALAR_LENGTH]) -> Result<Self, Red25519Error> {
        if bytes[0] & 0b0000_0111 != 0 || bytes[SCALAR_LENGTH - 1] & 0b1100_0000 != 0b0100_0000 {
            return Err(Red25519Error::NonCanonicalScalar);
        }
        Ok(Self(bytes))
    }

    /// Returns whether the stored value is already a canonical residue modulo `L`.
    ///
    /// Only a converted type-7 key may answer `false`; generated, restored, and blinded keys
    /// are canonical by construction.
    pub fn is_canonical(&self) -> bool {
        bool::from(Scalar::from_canonical_bytes(self.0).is_some())
    }

    /// Borrows the little-endian encoding for explicit secret storage.
    pub const fn secret_bytes(&self) -> [u8; SCALAR_LENGTH] {
        self.0
    }

    fn as_scalar(&self) -> Scalar {
        // Arithmetic is defined modulo L; the stored representation is reduced here.
        Scalar::from_bytes_mod_order(self.0)
    }
}

/// The daily blinding scalar `alpha` shared by a destination owner and its authorized clients.
///
/// Always canonical (`< L`), fixed-size, non-cloneable, and erased on drop. It is a secret: a
/// peer that learns it can re-randomize the destination's public key at will.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct BlindingScalar([u8; SCALAR_LENGTH]);

impl BlindingScalar {
    /// Loads a canonical blinding scalar from little-endian bytes.
    pub fn from_bytes(bytes: [u8; SCALAR_LENGTH]) -> Result<Self, Red25519Error> {
        Option::<Scalar>::from(Scalar::from_canonical_bytes(bytes))
            .ok_or(Red25519Error::NonCanonicalScalar)?;
        Ok(Self(bytes))
    }

    /// Borrows the canonical little-endian encoding for explicit secret storage.
    pub const fn secret_bytes(&self) -> [u8; SCALAR_LENGTH] {
        self.0
    }

    fn as_scalar(&self) -> Scalar {
        Scalar::from_bytes_mod_order(self.0)
    }
}

/// A blinded Red25519 private scalar `(a + alpha) mod L` used to sign encrypted LeaseSet2.
///
/// Always canonical, fixed-size, non-cloneable, and erased on drop.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct BlindedPrivateScalar([u8; SCALAR_LENGTH]);

impl BlindedPrivateScalar {
    /// Loads a canonical blinded scalar from little-endian bytes.
    ///
    /// Needed to restore a persisted or published blinded key. Non-canonical input is rejected
    /// rather than reduced, matching the unblinded owner.
    pub fn from_bytes(bytes: [u8; SCALAR_LENGTH]) -> Result<Self, Red25519Error> {
        Option::<Scalar>::from(Scalar::from_canonical_bytes(bytes))
            .ok_or(Red25519Error::NonCanonicalScalar)?;
        Ok(Self(bytes))
    }

    /// Borrows the canonical little-endian encoding for explicit secret storage.
    pub const fn secret_bytes(&self) -> [u8; SCALAR_LENGTH] {
        self.0
    }

    fn as_scalar(&self) -> Scalar {
        Scalar::from_bytes_mod_order(self.0)
    }
}

/// A Red25519 public key: a compressed Edwards point.
///
/// Public material, so it is cloneable, comparable, and printable as a protocol type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Red25519PublicKey([u8; PUBLIC_KEY_LENGTH]);

impl Red25519PublicKey {
    /// Wraps already-validated public key bytes.
    pub const fn from_bytes(bytes: [u8; PUBLIC_KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Validates and wraps compressed point bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, Red25519Error> {
        let array: [u8; PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| Red25519Error::InvalidPublicKey)?;
        Self::decode_array(array)
    }

    fn decode_array(bytes: [u8; PUBLIC_KEY_LENGTH]) -> Result<Self, Red25519Error> {
        point_from_bytes(&bytes)?;
        Ok(Self(bytes))
    }

    /// Returns the compressed point encoding.
    pub const fn as_bytes(&self) -> &[u8; PUBLIC_KEY_LENGTH] {
        &self.0
    }

    /// Converts to the common-structure signing key type (signature type 11).
    pub fn to_signing_public_key(&self) -> Result<SigningPublicKey, Red25519Error> {
        SigningPublicKey::new(BLINDED_SIGNING_KEY_TYPE, self.0.to_vec())
            .map_err(|_| Red25519Error::ProtocolKeyRejected)
    }

    fn as_point(&self) -> Result<EdwardsPoint, Red25519Error> {
        point_from_bytes(&self.0)
    }

    fn prime_order(&self) -> Result<EdwardsPoint, Red25519Error> {
        let point = self.as_point()?;
        if point.is_small_order() {
            return Err(Red25519Error::SmallOrderPoint);
        }
        if !point.is_torsion_free() {
            return Err(Red25519Error::NotPrimeOrderPoint);
        }
        Ok(point)
    }
}

/// A Red25519 signature: `R || S` in the specification's encoding.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Red25519Signature([u8; SIGNATURE_LENGTH]);

impl Red25519Signature {
    /// Wraps an already-validated 64-byte signature.
    pub const fn from_bytes(bytes: [u8; SIGNATURE_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Validates the length and wraps signature bytes without verifying them.
    pub fn decode(bytes: &[u8]) -> Result<Self, Red25519Error> {
        let array: [u8; SIGNATURE_LENGTH] =
            bytes
                .try_into()
                .map_err(|_| Red25519Error::InvalidSignatureLength {
                    actual: bytes.len(),
                    expected: SIGNATURE_LENGTH,
                })?;
        Ok(Self(array))
    }

    /// Returns the exact signature bytes.
    pub const fn as_bytes(&self) -> &[u8; SIGNATURE_LENGTH] {
        &self.0
    }
}

impl core::fmt::Debug for Red25519Signature {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Red25519Signature")
            .field("length", &SIGNATURE_LENGTH)
            .finish_non_exhaustive()
    }
}

/// A validated UTC calendar day used as the eight-byte ASCII `YYYYMMDD` blinding input.
///
/// The type exists so an invalid calendar day cannot reach the key-derivation step: the day is
/// checked once, at construction, and every later use is a fixed 8-byte array.
///
/// The ordering is calendar order and is used by the NetDB blinding schedule to bound a
/// per-day key cache, so it must agree with the `YYYYMMDD` derivation input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlindingDay {
    year: u16,
    month: u8,
    day: u8,
}

impl BlindingDay {
    /// Accepts a UTC year, month, and day, rejecting out-of-range or non-existent dates.
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Result<Self, Red25519Error> {
        if !(MIN_BLINDING_YEAR..=MAX_BLINDING_YEAR).contains(&year) {
            return Err(Red25519Error::InvalidBlindingDay { reason: "year" });
        }
        if !(1..=12).contains(&month) {
            return Err(Red25519Error::InvalidBlindingDay { reason: "month" });
        }
        if day < 1 || day > days_in_month(year, month) {
            return Err(Red25519Error::InvalidBlindingDay { reason: "day" });
        }
        Ok(Self { year, month, day })
    }

    /// Returns the eight-byte ASCII `YYYYMMDD` derivation input.
    pub fn as_bytes(&self) -> [u8; BLINDING_DAY_LENGTH] {
        let mut out = [0_u8; BLINDING_DAY_LENGTH];
        let text = format!("{:04}{:02}{:02}", self.year, self.month, self.day);
        out.copy_from_slice(text.as_bytes());
        out
    }

    /// Returns the validated `(year, month, day)` triple.
    pub const fn as_ymd(&self) -> (u16, u8, u8) {
        (self.year, self.month, self.day)
    }
}

/// The optional UTF-8 blinding secret that participates in `GENERATE_ALPHA`.
///
/// The secret is operator input that must be shared out of band between a
/// publisher and its clients. It is held with erase-on-drop semantics, is not
/// `Clone`, has no `Display`, and has no serde implementation, so it cannot
/// reach a log line or a configuration dump by accident.
///
/// An absent secret and an empty secret are the same derivation input. There is
/// deliberately no silent fallback in the other direction: a configured secret
/// that failed to be applied is an error, never a no-secret derivation.
pub struct LookupSecret(Zeroizing<String>);

impl LookupSecret {
    /// Returns the zero-length secret, i.e. "no secret configured".
    pub fn empty() -> Self {
        Self(Zeroizing::new(String::new()))
    }

    /// Wraps a secret, rejecting anything above [`MAX_LOOKUP_SECRET_LENGTH`].
    ///
    /// The specification defines no maximum. This bound is an i2pr resource
    /// decision, applied here rather than at each call site, so a
    /// control-plane-supplied value cannot become an unbounded key-derivation
    /// input and is rejected rather than hashed silently.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(secret: &str) -> Result<Self, Red25519Error> {
        if secret.len() > MAX_LOOKUP_SECRET_LENGTH {
            return Err(Red25519Error::LookupSecretTooLong {
                actual: secret.len(),
                maximum: MAX_LOOKUP_SECRET_LENGTH,
            });
        }
        Ok(Self(Zeroizing::new(secret.to_owned())))
    }

    /// Returns the borrowed secret, or `None` when no secret is configured.
    ///
    /// Returning `None` for the empty secret keeps [`generate_alpha`] from
    /// distinguishing "no secret" from "empty secret", which the specification
    /// requires to be the same input.
    pub fn as_option(&self) -> Option<&str> {
        if self.0.is_empty() {
            None
        } else {
            Some(self.0.as_str())
        }
    }

    /// Returns whether no secret is configured.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the secret length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl core::fmt::Debug for LookupSecret {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LookupSecret")
            .field("length", &self.0.len())
            .finish()
    }
}

const fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Computes the Red25519 transcript hash `HStar(prefix1, prefix2, m)`, reduced `mod L`.
///
/// The message is length-prefixed with a 2-byte little-endian length, which is what makes
/// SHA-512 safe against length extension for this construction.
fn h_star(prefix1: &[u8], prefix2: &[u8], message: &[u8]) -> Result<Scalar, Red25519Error> {
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(Red25519Error::MessageTooLong {
            actual: message.len(),
            maximum: MAX_MESSAGE_LENGTH,
        });
    }
    let length = u16::try_from(message.len()).map_err(|_| Red25519Error::MessageTooLong {
        actual: message.len(),
        maximum: MAX_MESSAGE_LENGTH,
    })?;
    let mut hasher = Sha512::new();
    hasher.update(HSTAR_PREFIX);
    hasher.update(prefix1);
    hasher.update(prefix2);
    hasher.update(length.to_le_bytes());
    hasher.update(message);
    let digest: [u8; 64] = hasher.finalize().into();
    Ok(Scalar::from_bytes_mod_order_wide(&digest))
}

/// Decodes a compressed point and rejects any non-canonical encoding.
///
/// The field decoder reduces out-of-range `y` values, so a decompressible input is not by
/// itself a canonical encoding. Because the signing and verification transcripts hash the
/// *encoded* public key, a non-canonical encoding is rejected at the door instead of being
/// silently normalized somewhere else in the pipeline.
fn point_from_bytes(bytes: &[u8; PUBLIC_KEY_LENGTH]) -> Result<EdwardsPoint, Red25519Error> {
    let point = CompressedEdwardsY(*bytes)
        .decompress()
        .ok_or(Red25519Error::InvalidPublicKey)?;
    if point.compress().to_bytes() != *bytes {
        return Err(Red25519Error::InvalidPublicKey);
    }
    Ok(point)
}

/// Length of an Ed25519 seed/private key accepted by [`convert_ed25519_private`].
pub const PRIVATE_SEED_LENGTH: usize = crate::PRIVATE_KEY_LENGTH;

/// `CONVERT_ED25519_PRIVATE`: the Red25519 scalar corresponding to an Ed25519 seed.
///
/// This is exactly the RFC 8032 secret scalar, so the converted key derives the same public
/// key; the specification accepts the resulting reduction in key-space entropy for existing
/// destinations.
pub fn convert_ed25519_private(seed: &[u8; PRIVATE_SEED_LENGTH]) -> Red25519PrivateScalar {
    let mut digest: [u8; 64] = Sha512::digest(seed).into();
    let mut scalar = [0_u8; SCALAR_LENGTH];
    scalar.copy_from_slice(&digest[..SCALAR_LENGTH]);
    scalar[0] &= 248;
    scalar[31] = (scalar[31] & 63) | 64;
    digest.zeroize();
    // The clamped value is generally >= L, so it is stored verbatim and reduced inside
    // arithmetic rather than being canonicalized here.
    Red25519PrivateScalar(scalar)
}

/// `GENERATE_PRIVATE`: a uniformly random Red25519 private scalar.
pub fn generate_private<R: TryCryptoRng + ?Sized>(
    rng: &mut R,
) -> Result<Red25519PrivateScalar, Red25519Error> {
    let mut wide = [0_u8; 64];
    rng.try_fill_bytes(&mut wide)
        .map_err(|_| Red25519Error::RandomnessUnavailable)?;
    let scalar = Scalar::from_bytes_mod_order_wide(&wide);
    wide.zeroize();
    Ok(Red25519PrivateScalar(scalar.to_bytes()))
}

/// `DERIVE_PUBLIC(sk) = [sk]B`.
pub fn derive_public_key(sk: &Red25519PrivateScalar) -> Red25519PublicKey {
    encode_point(&(ED25519_BASEPOINT_TABLE * &sk.as_scalar()).compress())
}

/// `DERIVE_PUBLIC(a')` for a blinded private scalar, used when a client holds the blinded key
/// but must confirm it matches the privately derived public key.
pub fn derive_blinded_public_key(sk: &BlindedPrivateScalar) -> Red25519PublicKey {
    encode_point(&(ED25519_BASEPOINT_TABLE * &sk.as_scalar()).compress())
}

fn encode_point(point: &CompressedEdwardsY) -> Red25519PublicKey {
    Red25519PublicKey(point.to_bytes())
}

/// `GENERATE_ALPHA`: the daily blinding scalar for a destination and optional lookup secret.
///
/// `unblinded_sigtype` must be Ed25519 (7) or Red25519 (11); the blinded type is always 11.
/// The derivation is deterministic in `(A, sigtype, UTC day, secret)`, so a client that knows
/// the destination's signing public key and the optional secret re-derives the same `alpha`
/// without any key exchange.
pub fn generate_alpha(
    public_key: &Red25519PublicKey,
    unblinded_sigtype: SigningKeyType,
    day: BlindingDay,
    lookup_secret: Option<&str>,
) -> Result<BlindingScalar, Red25519Error> {
    check_blindable_sigtype(unblinded_sigtype)?;
    if let Some(secret) = lookup_secret
        && secret.len() > MAX_LOOKUP_SECRET_LENGTH
    {
        return Err(Red25519Error::LookupSecretTooLong {
            actual: secret.len(),
            maximum: MAX_LOOKUP_SECRET_LENGTH,
        });
    }
    let datestring = day.as_bytes();
    let mut salt_input = Vec::with_capacity(ALPHA_SALT_PERSONALIZATION.len() + keydata_len());
    salt_input.extend_from_slice(ALPHA_SALT_PERSONALIZATION);
    salt_input.extend_from_slice(&keydata(public_key, unblinded_sigtype));
    let salt: [u8; HASH_LENGTH] = Sha256::digest(&salt_input).into();
    salt_input.zeroize();

    let mut input = Vec::with_capacity(BLINDING_DAY_LENGTH + lookup_secret.map_or(0, str::len));
    input.extend_from_slice(&datestring);
    if let Some(secret) = lookup_secret {
        input.extend_from_slice(secret.as_bytes());
    }
    let seed = crate::hkdf::hkdf_sha256_extract_and_expand(&salt, &input, ALPHA_HKDF_INFO, 64)
        .map_err(|_| Red25519Error::ProtocolKeyRejected)?;
    input.zeroize();
    let mut wide = [0_u8; 64];
    wide.copy_from_slice(seed.as_slice());
    let scalar = Scalar::from_bytes_mod_order_wide(&wide);
    wide.zeroize();
    Ok(BlindingScalar(scalar.to_bytes()))
}

fn keydata_len() -> usize {
    PUBLIC_KEY_LENGTH + 4
}

fn keydata(public_key: &Red25519PublicKey, unblinded_sigtype: SigningKeyType) -> Vec<u8> {
    let mut out = Vec::with_capacity(keydata_len());
    out.extend_from_slice(public_key.as_bytes());
    out.extend_from_slice(&unblinded_sigtype.code().to_be_bytes());
    out.extend_from_slice(&BLINDED_SIGNING_KEY_TYPE.code().to_be_bytes());
    out
}

fn check_blindable_sigtype(sigtype: SigningKeyType) -> Result<(), Red25519Error> {
    if sigtype == ED25519_SIGNING_KEY_TYPE || sigtype == BLINDED_SIGNING_KEY_TYPE {
        Ok(())
    } else {
        Err(Red25519Error::UnsupportedSigType {
            algorithm: sigtype.code(),
        })
    }
}

/// `BLIND_PUBKEY(A, alpha) = A + [alpha]B`, for clients that hold only the destination key.
///
/// The input key must lie in the prime-order subgroup; the resulting blinded key is
/// torsion-free by construction.
pub fn blind_public_key(
    public_key: &Red25519PublicKey,
    alpha: &BlindingScalar,
) -> Result<Red25519PublicKey, Red25519Error> {
    let base = public_key.prime_order()?;
    let blinded = base + (ED25519_BASEPOINT_TABLE * &alpha.as_scalar());
    Ok(encode_point(&blinded.compress()))
}

/// `BLIND_PRIVKEY(a, alpha) = (a + alpha) mod L`, for the destination owner.
pub fn blind_private_key(
    private_key: &Red25519PrivateScalar,
    alpha: &BlindingScalar,
) -> BlindedPrivateScalar {
    BlindedPrivateScalar((private_key.as_scalar() + alpha.as_scalar()).to_bytes())
}

/// Signs a message with a blinded Red25519 key, drawing fresh nonce bytes from `rng`.
///
/// Every signature is randomized: the same key over the same message yields different bytes.
pub fn sign<R: TryCryptoRng + ?Sized>(
    blinded_key: &BlindedPrivateScalar,
    message: &[u8],
    rng: &mut R,
) -> Result<Red25519Signature, Red25519Error> {
    let mut nonce = [0_u8; SIGNING_NONCE_LENGTH];
    rng.try_fill_bytes(&mut nonce)
        .map_err(|_| Red25519Error::RandomnessUnavailable)?;
    let signature = sign_with_nonce(blinded_key, message, &nonce)?;
    nonce.zeroize();
    Ok(signature)
}

/// Signs a message with a caller-supplied 80-byte nonce.
///
/// This exists so vector qualification and adversarial tests can reproduce a transcript exactly.
/// Production signing must use [`sign`], which draws the nonce from an injected CSPRNG: a fixed
/// or repeated nonce leaks the private key.
pub fn sign_with_nonce(
    blinded_key: &BlindedPrivateScalar,
    message: &[u8],
    nonce: &[u8; SIGNING_NONCE_LENGTH],
) -> Result<Red25519Signature, Red25519Error> {
    let public_bytes = (ED25519_BASEPOINT_TABLE * &blinded_key.as_scalar())
        .compress()
        .to_bytes();
    let r = h_star(nonce, &public_bytes, message)?;
    let big_r = encode_point(&(ED25519_BASEPOINT_TABLE * &r).compress());
    let c = h_star(big_r.as_bytes(), &public_bytes, message)?;
    let s = r + c * blinded_key.as_scalar();
    let mut out = [0_u8; SIGNATURE_LENGTH];
    out[..PUBLIC_KEY_LENGTH].copy_from_slice(big_r.as_bytes());
    out[PUBLIC_KEY_LENGTH..].copy_from_slice(&s.to_bytes());
    Ok(Red25519Signature(out))
}

/// Verifies a Red25519 signature over a message for an unblinded key.
pub fn verify(
    public_key: &Red25519PublicKey,
    message: &[u8],
    signature: &Red25519Signature,
) -> Result<(), Red25519Error> {
    verify_equation(public_key, message, signature)
        .map_err(|()| Red25519Error::SignatureVerificationFailed)
}

/// Verifies a Red25519 signature over a message for a blinded key.
///
/// Semantically identical to [`verify`]; the distinct error keeps a caller from confusing an
/// ordinary destination signature with an encrypted-LeaseSet2 signature.
pub fn verify_blinded(
    blinded_key: &Red25519PublicKey,
    message: &[u8],
    signature: &Red25519Signature,
) -> Result<(), Red25519Error> {
    verify_equation(blinded_key, message, signature)
        .map_err(|()| Red25519Error::BlindedSignatureVerificationFailed)
}

fn verify_equation(
    public_key: &Red25519PublicKey,
    message: &[u8],
    signature: &Red25519Signature,
) -> Result<(), ()> {
    let public_point = public_key.as_point().map_err(|_| ())?;
    let r_bytes: [u8; PUBLIC_KEY_LENGTH] = signature.0[..PUBLIC_KEY_LENGTH]
        .try_into()
        .map_err(|_| ())?;
    let big_r = point_from_bytes(&r_bytes).map_err(|_| ())?;
    let s_bytes: [u8; SCALAR_LENGTH] = signature.0[PUBLIC_KEY_LENGTH..]
        .try_into()
        .map_err(|_| ())?;
    let s = Option::<Scalar>::from(Scalar::from_canonical_bytes(s_bytes)).ok_or(())?;
    // vkBytes is ENCODE_POINT(vk), so the transcript always uses the canonical encoding.
    let vk_bytes = public_point.compress().to_bytes();
    let c = h_star(&r_bytes, &vk_bytes, message).map_err(|_| ())?;
    // ((-[S]B) + R + ([c]vk) * h) == identity. Every operand here is public, so the
    // variable-time double-scalar path is appropriate.
    let candidate =
        EdwardsPoint::vartime_double_scalar_mul_basepoint(&c, &public_point, &(-s)) + big_r;
    let cofactored = candidate.mul_by_cofactor();
    if bool::from(cofactored.ct_eq(&EdwardsPoint::default())) {
        Ok(())
    } else {
        Err(())
    }
}

/// The blinded DHT storage key: `SHA-256(0x000b || blinded public key)`.
///
/// The lookup secret deliberately does not enter this value; a wrong secret produces a
/// different blinded key and therefore a lookup miss, never a wrong-record read.
pub fn blinded_storage_key(blinded_public_key: &Red25519PublicKey) -> Hash {
    let mut input = [0_u8; 2 + PUBLIC_KEY_LENGTH];
    input[..2].copy_from_slice(&BLINDED_SIGNING_KEY_TYPE.code().to_be_bytes());
    input[2..].copy_from_slice(blinded_public_key.as_bytes());
    Hash::from_bytes(Sha256::digest(input).into())
}
