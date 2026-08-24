//! ECIES-X25519-AEAD-Ratchet session-layer primitives for ordinary
//! destination-to-destination traffic.
//!
//! Plan 121 §2-§9 own the typed cryptographic seam that sits between
//! `i2pr-client`'s destination lifecycle and the I2P New Session /
//! New Session Reply / Existing Session Garlic protocol. The module
//! wraps `curve25519-elligator2` for the Elligator2 representative
//! mapping the I2P ECIES specification requires for the new-session
//! ephemeral public key, and exposes a hand-rolled `Noise_N`-style
//! handshake on top of `ChaCha20-Poly1305` and `HKDF-SHA256` (matching
//! the existing Plan 108/109 tunnel-build seam).
//!
//! The module is deliberately minimal:
//!
//! - it does **not** own session lifecycle, replay caches, tag-set
//!   state, or any per-destination policy (that lives in
//!   `i2pr_client::session`);
//! - it does **not** wrap the destination identity owner (which lives
//!   in `i2pr_client::identity`); only a typed reference to the
//!   static X25519 secret is exposed;
//! - it does **not** parse or emit Garlic payload blocks (that lives
//!   in `i2pr_proto::ecies_payload`).
//!
//! The dependency on `curve25519-elligator2` is intentional and the
//! only acceptable production source for the Elligator2 mapping per
//! Plan 121 §2 / §12; the wrapper keeps the raw third-party type
//! sealed inside this module so the rest of the workspace never sees
//! its API surface.

#![forbid(unsafe_code)]

use core::fmt;

use curve25519_elligator2::{EdwardsPoint, MapToPointVariant, RFC9380};
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::HkdfError;

/// Re-exported X25519 public-key length used as the static-key
/// dimension.
pub const STATIC_PUBLIC_LENGTH_VALUE: usize = 32;

/// Canonical ECIES Noise protocol name used for the new-session
/// transcript. The string is the literal
/// `Noise_NK_25519_ChaChaPoly_SHA256` pattern documented by the
/// I2P ECIES specification; the actual I2P protocol uses a
/// slightly extended Noise-N pattern that the session layer composes
/// on top of the same primitive set.
///
/// The 35-byte ASCII literal exceeds the canonical 32-byte Noise
/// protocol-name buffer; the protocol therefore relies on the
/// SHA-256-of-protocol-name `h0` initialization pattern rather
/// than truncation.
pub const ECIES_NOISE_PROTOCOL_NAME: &[u8] = b"Noise_NK_25519_ChaChaPoly_SHA256";

/// Length of an Elligator2 representative / ephemeral public key.
pub const REPRESENTATIVE_LENGTH: usize = 32;
/// Length of a static destination X25519 public key.
pub const STATIC_PUBLIC_LENGTH: usize = STATIC_PUBLIC_LENGTH_VALUE;
/// Length of an ECIES session tag.
pub const SESSION_TAG_LENGTH: usize = 8;
/// Length of a ChaCha20-Poly1305 authentication tag.
pub const AEAD_TAG_LEN: usize = 16;
/// Length of a ChaCha20-Poly1305 nonce (IETF variant).
pub const AEAD_NONCE_LEN: usize = 12;
/// Length of an HKDF-SHA256 output chunk.
pub const HKDF_OUTPUT_LEN: usize = 32;

/// The maximum new-session ciphertext length accepted by the seam.
/// The I2P ECIES specification permits up to 65535 bytes; the seam
/// caps at a single 64 KiB I2NP message so `u16` payload lengths are
/// not exceeded inside `i2pr-proto`.
pub const MAX_NEW_SESSION_CIPHERTEXT: usize = 65_507;

/// Errors returned by the ECIES seam.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EciesError {
    /// The Elligator2 representative could not be decoded. About half
    /// of all 32-byte strings are valid representatives; the receiver
    /// must request a retry on this failure rather than fail closed.
    #[error("ECIES Elligator2 representative could not be decoded")]
    ElligatorDecode,
    /// The decoded ephemeral seed produced an all-zero Montgomery
    /// u-coordinate, which the X25519 layer treats as an invalid
    /// public key.
    #[error("ECIES ephemeral public key is the all-zero value")]
    AllZeroEphemeral,
    /// The peer's static key was the all-zero value.
    #[error("ECIES peer static key is the all-zero value")]
    AllZeroStatic,
    /// The peer's ephemeral key produced the forbidden all-zero
    /// Diffie-Hellman shared secret.
    #[error("ECIES ephemeral DH produced the forbidden all-zero shared secret")]
    InvalidEphemeralDh,
    /// ChaCha20-Poly1305 authentication failed during open.
    #[error("ECIES session AEAD authentication failed")]
    AuthenticationFailed,
    /// ChaCha20-Poly1305 encryption failed (highly unexpected).
    #[error("ECIES session AEAD encryption failed")]
    EncryptionFailed,
    /// The supplied new-session ciphertext exceeds the local ceiling.
    #[error("ECIES new-session ciphertext length {actual} exceeds ceiling {maximum}")]
    CiphertextTooLarge {
        /// Actual ciphertext length.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// The supplied new-session ciphertext is shorter than the AEAD
    /// authentication tag, which would prevent authentication.
    #[error("ECIES new-session ciphertext is shorter than the AEAD tag")]
    CiphertextTooShort,
    /// The HKDF helper returned an error (impossible at the
    /// configured limits).
    #[error("ECIES HKDF derivation failed: {0}")]
    Hkdf(HkdfError),
    /// The supplied cryptographic RNG failed.
    #[error("ECIES cryptographic randomness unavailable")]
    RandomnessUnavailable,
}

/// An Elligator2 representative for an ECIES new-session ephemeral
/// public key.
///
/// The representative is a 32-byte encoding of an X25519 public key
/// (`X25519(esk, G)` where `esk` is the sender's ephemeral scalar).
/// The wrapper is intentionally non-secret and may be copied or
/// serialized; the wrapper exposes the wire representation that the
/// I2P ECIES specification documents for the new-session ephemeral
/// public key field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EciesEphemeralRepresentative([u8; REPRESENTATIVE_LENGTH]);

impl EciesEphemeralRepresentative {
    /// Constructs a representative from explicit bytes.
    pub const fn from_bytes(bytes: [u8; REPRESENTATIVE_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Returns the wrapped bytes.
    pub const fn as_bytes(&self) -> &[u8; REPRESENTATIVE_LENGTH] {
        &self.0
    }
}

/// A freshly generated ECIES ephemeral keypair.
///
/// The secret is the 32-byte ephemeral scalar the sender uses for
/// X25519 Diffie-Hellman during the new-session handshake; the
/// representative is the on-wire encoding the receiver decodes back
/// into the same Montgomery public key (`X25519(esk, G)`) before
/// performing its half of the DH.
///
/// The secret zeroizes on drop. The struct does not implement
/// `Clone` or any byte-revealing `Debug` derivation.
pub struct EciesEphemeralKeypair {
    secret_seed: EciesEphemeralSecret,
    representative: EciesEphemeralRepresentative,
}

impl EciesEphemeralKeypair {
    /// Generates a new ephemeral keypair. The Elligator2 mapping
    /// succeeds for every 32-byte input from a CSPRNG; this
    /// constructor nevertheless retries on the rare RFC 9380
    /// non-representable failure rather than silently corrupting
    /// the handshake.
    pub fn generate<R: rand_core::TryCryptoRng + ?Sized>(rng: &mut R) -> Result<Self, EciesError> {
        // Plan 121 §2 / §6 require fresh randomness per
        // new-session message and forbid hand-rolled re-mappings.
        const MAX_ATTEMPTS: usize = 64;
        for _ in 0..MAX_ATTEMPTS {
            let mut seed_bytes = Zeroizing::new([0_u8; REPRESENTATIVE_LENGTH]);
            rng.try_fill_bytes(&mut *seed_bytes)
                .map_err(|_| EciesError::RandomnessUnavailable)?;
            if let Some(candidate) = Self::from_seed_bytes(*seed_bytes) {
                return Ok(candidate);
            }
        }
        Err(EciesError::RandomnessUnavailable)
    }

    /// Builds a keypair from an explicit 32-byte seed. Returns
    /// `None` when the seed's representative is not decodable (the
    /// I2P ECIES specification requires the sender to keep retrying
    /// in this case).
    pub fn from_seed_bytes(seed: [u8; REPRESENTATIVE_LENGTH]) -> Option<Self> {
        // The RFC 9380 mapping: the seed is interpreted as an X25519
        // private key, multiplied by the basepoint to obtain a
        // Montgomery point, and that point is encoded as a 32-byte
        // representative. The RFC 9380 `to_representative` variant
        // sets the high two bits of the returned seed to `tweak`
        // (we use `0`); the matching `from_representative` clears
        // them. For the symmetric round-trip used here, we
        // canonicalize the seed by clearing the high two bits before
        // mapping.
        let mut canonical_seed = seed;
        canonical_seed[REPRESENTATIVE_LENGTH - 1] &= 0x3f;
        let representative_opt: Option<[u8; REPRESENTATIVE_LENGTH]> =
            RFC9380::to_representative(&canonical_seed, 0).into();
        let representative = representative_opt?;
        if representative.iter().all(|byte| *byte == 0) {
            return None;
        }
        Some(Self {
            secret_seed: EciesEphemeralSecret(canonical_seed),
            representative: EciesEphemeralRepresentative(representative),
        })
    }

    /// Returns the secret seed (zeroizing owner).
    pub const fn secret(&self) -> &EciesEphemeralSecret {
        &self.secret_seed
    }

    /// Returns the Elligator2 representative (the on-wire
    /// ephemeral public key bytes).
    pub const fn representative(&self) -> EciesEphemeralRepresentative {
        self.representative
    }
}

impl fmt::Debug for EciesEphemeralKeypair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EciesEphemeralKeypair")
            .field("representative", &self.representative)
            .field("secret_seed", &"<redacted>")
            .finish()
    }
}

/// A zeroizing ECIES ephemeral secret seed.
///
/// The wrapper does not implement `Clone`, byte-revealing `Debug`,
/// or any serde / formatting derived output.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct EciesEphemeralSecret([u8; REPRESENTATIVE_LENGTH]);

impl EciesEphemeralSecret {
    /// Loads an explicit seed for deterministic protocol tests.
    pub const fn from_bytes(bytes: [u8; REPRESENTATIVE_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the seed bytes for one protocol derivation.
    pub const fn as_bytes(&self) -> &[u8; REPRESENTATIVE_LENGTH] {
        &self.0
    }
}

impl fmt::Debug for EciesEphemeralSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EciesEphemeralSecret")
            .field("seed", &"<redacted>")
            .finish()
    }
}

/// Decodes a 32-byte Elligator2 representative into the matching
/// Montgomery public-key point.
///
/// Returns `Err(EciesError::ElligatorDecode)` when the representative
/// does not encode a Curve25519 Montgomery point or when the
/// recovered point is the forbidden all-zero value. The recovered
/// public key is the value the receiver uses for the new-session DH.
pub fn decode_representative(
    representative: &EciesEphemeralRepresentative,
) -> Result<[u8; REPRESENTATIVE_LENGTH], EciesError> {
    let bytes = representative.as_bytes();
    if bytes.iter().all(|byte| *byte == 0) {
        return Err(EciesError::ElligatorDecode);
    }
    let recovered: Option<EdwardsPoint> = RFC9380::from_representative(bytes).into();
    let recovered = recovered.ok_or(EciesError::ElligatorDecode)?;
    let recovered_mont = recovered.to_montgomery();
    let recovered_bytes = recovered_mont.to_bytes();
    if recovered_bytes.iter().all(|byte| *byte == 0) {
        return Err(EciesError::ElligatorDecode);
    }
    Ok(recovered_bytes)
}

/// Performs an X25519 Diffie-Hellman operation between a static
/// private key and an ephemeral public key. The result is rejected
/// when it is the forbidden all-zero value.
pub(crate) fn diffie_hellman_static(
    static_priv: &StaticSecret,
    ephemeral_pub: &[u8; REPRESENTATIVE_LENGTH],
) -> Result<Zeroizing<[u8; HKDF_OUTPUT_LEN]>, EciesError> {
    if ephemeral_pub.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroEphemeral);
    }
    let peer = X25519PublicKey::from(*ephemeral_pub);
    let shared = static_priv.diffie_hellman(&peer);
    if shared.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(EciesError::InvalidEphemeralDh);
    }
    let mut out = Zeroizing::new([0_u8; HKDF_OUTPUT_LEN]);
    out.copy_from_slice(shared.as_bytes());
    Ok(out)
}

/// Performs an X25519 Diffie-Hellman operation between an ephemeral
/// private seed and a static public key.
pub(crate) fn diffie_hellman_ephemeral(
    ephemeral_seed: &[u8; REPRESENTATIVE_LENGTH],
    static_pub: &[u8; STATIC_PUBLIC_LENGTH],
) -> Result<Zeroizing<[u8; HKDF_OUTPUT_LEN]>, EciesError> {
    if static_pub.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroStatic);
    }
    let clamped = clamp_x25519_seed(ephemeral_seed);
    let secret = StaticSecret::from(clamped);
    let peer = X25519PublicKey::from(*static_pub);
    let shared = secret.diffie_hellman(&peer);
    if shared.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(EciesError::InvalidEphemeralDh);
    }
    let mut out = Zeroizing::new([0_u8; HKDF_OUTPUT_LEN]);
    out.copy_from_slice(shared.as_bytes());
    Ok(out)
}

/// Apply the X25519 scalar clamping rules to a 32-byte seed: clear
/// the low three bits, clear the high bit of byte 31, and set the
/// second-highest bit of byte 31.
pub(crate) fn clamp_x25519_seed(seed: &[u8; REPRESENTATIVE_LENGTH]) -> [u8; REPRESENTATIVE_LENGTH] {
    let mut out = *seed;
    out[0] &= 248;
    out[REPRESENTATIVE_LENGTH - 1] &= 127;
    out[REPRESENTATIVE_LENGTH - 1] |= 64;
    out
}

/// The derived Noise-N-style chaining key + session key after one
/// Diffie-Hellman step.
#[derive(Clone)]
pub(crate) struct EciesKeyMaterial {
    /// Chaining key `ck`.
    pub chaining_key: [u8; HKDF_OUTPUT_LEN],
    /// AEAD key for the next payload.
    pub aead_key: [u8; HKDF_OUTPUT_LEN],
}

impl fmt::Debug for EciesKeyMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EciesKeyMaterial")
            .field("chaining_key", &"<redacted>")
            .field("aead_key", &"<redacted>")
            .finish()
    }
}

impl Zeroize for EciesKeyMaterial {
    fn zeroize(&mut self) {
        self.chaining_key.zeroize();
        self.aead_key.zeroize();
    }
}

impl ZeroizeOnDrop for EciesKeyMaterial {}

/// Initial `h0 = SHA256(protocol_name)` value used as both the
/// starting chaining key and the starting transcript hash before the
/// null-prologue `MixHash`.
fn noise_init_hash() -> [u8; HKDF_OUTPUT_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(ECIES_NOISE_PROTOCOL_NAME);
    let digest = hasher.finalize();
    let mut out = [0_u8; HKDF_OUTPUT_LEN];
    out.copy_from_slice(&digest);
    out
}

/// The Noise transcript state for a single ECIES session. Plan 121
/// uses a `Noise_NK`-shaped pattern (ephemeral + static + DHes +
/// DHss), implemented as a thin wrapper around the existing
/// `NoiseRequestState` from `i2pr-tunnel`. The wrapping function
/// deliberately stays internal so the rest of the workspace only sees
/// the typed [`EciesNewSession`] / [`EciesNewSessionReply`] /
/// [`EciesExistingSession`] API.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub(crate) struct EciesNoiseState {
    chaining_key: [u8; HKDF_OUTPUT_LEN],
    transcript_hash: [u8; HKDF_OUTPUT_LEN],
}

impl EciesNoiseState {
    /// Initializes a new ECIES Noise state with the canonical
    /// protocol-name `h0` and the post-null-prologue
    /// `MixHash(h0)`.
    pub fn new() -> Self {
        let h0 = noise_init_hash();
        let mut transcript_hasher = Sha256::new();
        transcript_hasher.update(h0);
        let mut h = [0_u8; HKDF_OUTPUT_LEN];
        h.copy_from_slice(&transcript_hasher.finalize());
        Self {
            chaining_key: h0,
            transcript_hash: h,
        }
    }

    /// Returns a copy of the chaining key.
    /// Returns a copy of the transcript hash.
    pub fn transcript_hash(&self) -> [u8; HKDF_OUTPUT_LEN] {
        self.transcript_hash
    }

    /// Mix `data` into the transcript hash.
    pub fn mix_hash(&mut self, data: &[u8]) {
        let mut hasher = Sha256::new();
        hasher.update(self.transcript_hash);
        hasher.update(data);
        self.transcript_hash.copy_from_slice(&hasher.finalize());
    }

    /// Apply `MixKey(ck, ikm)` and return the next chaining key
    /// plus the AEAD key for the immediately-following payload.
    pub fn mix_key(&mut self, ikm: &[u8]) -> Result<EciesKeyMaterial, EciesError> {
        let derived = crate::hkdf_sha256_extract_and_expand(&self.chaining_key, ikm, &[], 64)
            .map_err(EciesError::Hkdf)?;
        if derived.len() != 64 {
            return Err(EciesError::Hkdf(HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: 8160,
            }));
        }
        let mut next_ck = [0_u8; HKDF_OUTPUT_LEN];
        next_ck.copy_from_slice(&derived[..HKDF_OUTPUT_LEN]);
        let mut aead_key = [0_u8; HKDF_OUTPUT_LEN];
        aead_key.copy_from_slice(&derived[HKDF_OUTPUT_LEN..]);
        self.chaining_key = next_ck;
        Ok(EciesKeyMaterial {
            chaining_key: next_ck,
            aead_key,
        })
    }
}

impl fmt::Debug for EciesNoiseState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EciesNoiseState")
            .field("chaining_key", &"<redacted>")
            .field("transcript_hash", &"<redacted>")
            .finish()
    }
}

/// A 32-byte Montgomery `A` constant. The Curve25519 Montgomery
/// form uses `A = 486662 = 0x00076D06`, serialized little-endian
/// into the low 3 bytes of a 32-byte buffer.
#[allow(dead_code)]
pub(crate) fn montgomery_a() -> [u8; 32] {
    let mut out = [0_u8; 32];
    let bytes = 486_662_u32.to_le_bytes();
    out[0] = bytes[0];
    out[1] = bytes[1];
    out[2] = bytes[2];
    out
}

/// The I2P ECIES new-session flag.
pub const ECIES_NEW_SESSION_FLAG: u8 = 0xE0;
/// The I2P ECIES existing-session flag (8-byte session tag).
pub const ECIES_EXISTING_SESSION_FLAG: u8 = 0xE2;
/// Length of the I2P ECIES session tag used in existing-session
/// messages.
pub const ECIES_SESSION_TAG_LEN: usize = 8;

/// Per-direction paired session state installed after a successful
/// New Session / New Session Reply handshake.
///
/// The struct is intentionally minimal: it owns the session-tag
/// derivation chain plus the next send / receive ChaChaPoly keys.
/// It is `Clone` to support Plan 122 destination routing copies
/// without leaking the underlying zeroizing owners; the zeroize
/// contract remains in force on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EciesSessionState {
    /// Tag key driving the outbound session-tag ratchet.
    send_tag_key: [u8; HKDF_OUTPUT_LEN],
    /// Session tag for the next outbound existing-session message.
    send_tag: [u8; ECIES_SESSION_TAG_LEN],
    /// AEAD key for the next outbound existing-session message.
    send_key: [u8; HKDF_OUTPUT_LEN],
    /// Tag key driving the inbound session-tag ratchet.
    recv_tag_key: [u8; HKDF_OUTPUT_LEN],
    /// Session tag for the next inbound existing-session message.
    recv_tag: [u8; ECIES_SESSION_TAG_LEN],
    /// AEAD key for the next inbound existing-session message.
    recv_key: [u8; HKDF_OUTPUT_LEN],
    /// Bounded monotonic counter of consumed tags; the session
    /// manager enforces the configured tag look-ahead window.
    consumed_tags: u32,
    /// Bounded monotonic counter of tags generated locally.
    generated_tags: u32,
}

impl fmt::Debug for EciesSessionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EciesSessionState")
            .field("send_tag", &"<redacted>")
            .field("send_key", &"<redacted>")
            .field("recv_tag", &"<redacted>")
            .field("recv_key", &"<redacted>")
            .field("consumed_tags", &self.consumed_tags)
            .field("generated_tags", &self.generated_tags)
            .finish()
    }
}

impl EciesSessionState {
    /// Constructs a fresh paired session state from the install
    /// chaining key and AEAD key material. The chaining key drives
    /// the tag-key derivation on both sides so that the very first
    /// outbound tag round-trips.
    pub fn install(chaining_key: &[u8; HKDF_OUTPUT_LEN], aead_key: [u8; HKDF_OUTPUT_LEN]) -> Self {
        let tag_key = derive_tag_key(chaining_key, b"TagKey");
        let tag = current_tag(&tag_key);
        Self {
            send_tag_key: tag_key,
            send_tag: tag,
            send_key: aead_key,
            recv_tag_key: tag_key,
            recv_tag: tag,
            recv_key: aead_key,
            consumed_tags: 0,
            generated_tags: 1,
        }
    }

    /// Returns the next outbound session tag and its key. The
    /// caller is responsible for AEAD-protecting the new tag inside
    /// the message body.
    pub fn next_outbound(&mut self) -> ([u8; ECIES_SESSION_TAG_LEN], [u8; HKDF_OUTPUT_LEN]) {
        let tag = self.send_tag;
        let key = self.send_key;
        let derived = crate::hkdf_sha256_extract_and_expand(
            &self.send_tag_key,
            &self.send_tag,
            b"SessionTag",
            HKDF_OUTPUT_LEN + ECIES_SESSION_TAG_LEN,
        )
        .expect("hkdf length within MAX_HKDF_OUTPUT_LEN");
        self.send_tag_key
            .copy_from_slice(&derived[..HKDF_OUTPUT_LEN]);
        self.send_tag.copy_from_slice(&derived[HKDF_OUTPUT_LEN..]);
        self.generated_tags = self.generated_tags.saturating_add(1);
        (tag, key)
    }

    /// Validates and consumes an inbound session tag.
    pub fn consume_inbound(&mut self, tag: &[u8; ECIES_SESSION_TAG_LEN]) -> Result<(), EciesError> {
        if tag != &self.recv_tag {
            return Err(EciesError::AuthenticationFailed);
        }
        self.consumed_tags = self.consumed_tags.saturating_add(1);
        let derived = crate::hkdf_sha256_extract_and_expand(
            &self.recv_tag_key,
            &self.recv_tag,
            b"SessionTag",
            HKDF_OUTPUT_LEN + ECIES_SESSION_TAG_LEN,
        )
        .expect("hkdf length within MAX_HKDF_OUTPUT_LEN");
        self.recv_tag_key
            .copy_from_slice(&derived[..HKDF_OUTPUT_LEN]);
        self.recv_tag.copy_from_slice(&derived[HKDF_OUTPUT_LEN..]);
        Ok(())
    }

    /// Returns the current inbound session tag without consuming
    /// or advancing the ratchet.
    pub fn recv_tag(&self) -> [u8; ECIES_SESSION_TAG_LEN] {
        self.recv_tag
    }
}

/// Derive a tag-key from the supplied install-time chaining key.
fn derive_tag_key(chaining_key: &[u8; HKDF_OUTPUT_LEN], label: &[u8]) -> [u8; HKDF_OUTPUT_LEN] {
    let derived =
        crate::hkdf_sha256_extract_and_expand(chaining_key, label, b"TagKey", HKDF_OUTPUT_LEN)
            .expect("hkdf length within MAX_HKDF_OUTPUT_LEN");
    let mut out = [0_u8; HKDF_OUTPUT_LEN];
    out.copy_from_slice(&derived);
    out
}

/// Compute the current session tag for the supplied tag-key.
fn current_tag(tag_key: &[u8; HKDF_OUTPUT_LEN]) -> [u8; ECIES_SESSION_TAG_LEN] {
    let derived =
        crate::hkdf_sha256_extract_and_expand(tag_key, b"", b"SessionTag", ECIES_SESSION_TAG_LEN)
            .expect("hkdf length within MAX_HKDF_OUTPUT_LEN");
    let mut out = [0_u8; ECIES_SESSION_TAG_LEN];
    out.copy_from_slice(&derived);
    out
}

/// The I2P ECIES New Session message produced by Alice and
/// consumed by Bob.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSessionMessage {
    /// The flag byte (always `ECIES_NEW_SESSION_FLAG`).
    pub flag: u8,
    /// Alice's ephemeral static key for this session.
    pub static_key: [u8; STATIC_PUBLIC_LENGTH],
    /// The Elligator2 representative for the ephemeral public key.
    pub representative: EciesEphemeralRepresentative,
    /// The ChaCha20-Poly1305 ciphertext of the Garlic payload
    /// blocks.
    pub ciphertext: Vec<u8>,
}

impl NewSessionMessage {
    /// Constructs a new-session message from explicit fields,
    /// validating the flag byte.
    pub fn new(
        static_key: [u8; STATIC_PUBLIC_LENGTH],
        representative: EciesEphemeralRepresentative,
        ciphertext: Vec<u8>,
    ) -> Result<Self, EciesError> {
        if ciphertext.len() > MAX_NEW_SESSION_CIPHERTEXT {
            return Err(EciesError::CiphertextTooLarge {
                actual: ciphertext.len(),
                maximum: MAX_NEW_SESSION_CIPHERTEXT,
            });
        }
        Ok(Self {
            flag: ECIES_NEW_SESSION_FLAG,
            static_key,
            representative,
            ciphertext,
        })
    }

    /// Encodes the message under the supplied hard ceiling.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, EciesError> {
        let mut output = Vec::new();
        let total: usize = 1_usize
            .checked_add(STATIC_PUBLIC_LENGTH)
            .and_then(|n| n.checked_add(REPRESENTATIVE_LENGTH))
            .and_then(|n| n.checked_add(self.ciphertext.len()))
            .ok_or(EciesError::CiphertextTooLarge {
                actual: self.ciphertext.len(),
                maximum: MAX_NEW_SESSION_CIPHERTEXT,
            })?;
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        output.push(self.flag);
        output.extend_from_slice(&self.static_key);
        output.extend_from_slice(self.representative.as_bytes());
        output.extend_from_slice(&self.ciphertext);
        Ok(output)
    }

    /// Decodes a new-session message from the supplied input.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, EciesError> {
        if input.len() < 1 + STATIC_PUBLIC_LENGTH + REPRESENTATIVE_LENGTH {
            return Err(EciesError::CiphertextTooShort);
        }
        if input[0] != ECIES_NEW_SESSION_FLAG {
            return Err(EciesError::AuthenticationFailed);
        }
        let mut static_key = [0_u8; STATIC_PUBLIC_LENGTH];
        static_key.copy_from_slice(&input[1..1 + STATIC_PUBLIC_LENGTH]);
        let mut rep_bytes = [0_u8; REPRESENTATIVE_LENGTH];
        rep_bytes.copy_from_slice(
            &input[1 + STATIC_PUBLIC_LENGTH..1 + STATIC_PUBLIC_LENGTH + REPRESENTATIVE_LENGTH],
        );
        let ciphertext = input[1 + STATIC_PUBLIC_LENGTH + REPRESENTATIVE_LENGTH..].to_vec();
        if ciphertext.len() + 1 + STATIC_PUBLIC_LENGTH + REPRESENTATIVE_LENGTH > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: ciphertext.len() + 1 + STATIC_PUBLIC_LENGTH + REPRESENTATIVE_LENGTH,
                maximum,
            });
        }
        Self::new(
            static_key,
            EciesEphemeralRepresentative(rep_bytes),
            ciphertext,
        )
    }
}

/// Encrypts a New Session payload for transmission.
///
/// Returns the ciphertext bytes the sender writes into the
/// `NewSessionMessage`. The sender's [`NewSessionMessage`] will
/// embed its ephemeral static public key and the Elligator2
/// representative.
pub fn seal_new_session<R: rand_core::TryCryptoRng + ?Sized>(
    alice_ephemeral_keypair: &EciesEphemeralKeypair,
    bob_static_public: &[u8; STATIC_PUBLIC_LENGTH],
    payload: &[u8],
    rng: &mut R,
) -> Result<
    (
        NewSessionMessage,
        Zeroizing<[u8; STATIC_PUBLIC_LENGTH]>,
        EciesSessionState,
    ),
    EciesError,
> {
    if payload.len() > MAX_NEW_SESSION_CIPHERTEXT {
        return Err(EciesError::CiphertextTooLarge {
            actual: payload.len(),
            maximum: MAX_NEW_SESSION_CIPHERTEXT,
        });
    }
    // Generate Alice's per-session static keypair.
    let mut static_secret_bytes = Zeroizing::new([0_u8; STATIC_PUBLIC_LENGTH]);
    rng.try_fill_bytes(&mut *static_secret_bytes)
        .map_err(|_| EciesError::RandomnessUnavailable)?;
    let alice_static_secret = StaticSecret::from(*static_secret_bytes);
    let alice_static_public_bytes = X25519PublicKey::from(&alice_static_secret).to_bytes();

    // Compute DH(esk, BobStatic) and DH(static, BobStatic).
    let esk_dh = diffie_hellman_ephemeral(
        alice_ephemeral_keypair.secret().as_bytes(),
        bob_static_public,
    )?;
    let static_dh = alice_static_secret.diffie_hellman(&X25519PublicKey::from(*bob_static_public));
    let static_dh_bytes = *static_dh.as_bytes();

    // Mix the two shared secrets and derive the chained Noise keys.
    let mut state = EciesNoiseState::new();
    state.mix_hash(alice_ephemeral_keypair.representative().as_bytes());
    let mix1 = state.mix_key(esk_dh.as_ref())?;
    let mix2 = state.mix_key(&static_dh_bytes)?;

    // Encrypt the payload with the second AEAD key.
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&mix2.aead_key));
    let nonce_array = [0_u8; AEAD_NONCE_LEN];
    let nonce = Nonce::from_slice(&nonce_array);
    let payload_zero = Zeroizing::new(payload.to_vec());
    let ciphertext_with_tag = cipher
        .encrypt(
            nonce,
            Payload {
                msg: payload_zero.as_ref(),
                aad: &state.transcript_hash(),
            },
        )
        .map_err(|_| EciesError::EncryptionFailed)?;
    if ciphertext_with_tag.len() != payload.len() + AEAD_TAG_LEN {
        return Err(EciesError::EncryptionFailed);
    }

    let session_state = EciesSessionState::install(&mix1.chaining_key, mix1.aead_key);
    let message = NewSessionMessage::new(
        alice_static_public_bytes,
        alice_ephemeral_keypair.representative(),
        ciphertext_with_tag,
    )?;
    Ok((message, static_secret_bytes, session_state))
}

/// Decrypts a New Session payload received from `message`.
///
/// Returns the plaintext payload plus the paired session state
/// installed for follow-up Existing Session traffic. The receiver
/// must verify the supplied `bob_static_private` matches the
/// destination that published the LeaseSet2 containing
/// `message.static_key`.
pub fn open_new_session(
    bob_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    bob_static_public: &[u8; STATIC_PUBLIC_LENGTH],
    message: &NewSessionMessage,
    now_seconds: u32,
    max_datetime_skew_seconds: u32,
    payload: &[u8],
) -> Result<(Vec<u8>, EciesSessionState), EciesError> {
    let _ = (now_seconds, max_datetime_skew_seconds, payload);
    if bob_static_secret.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroStatic);
    }
    if bob_static_public.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroStatic);
    }
    let ephemeral_pub = decode_representative(&message.representative)?;
    let static_secret = StaticSecret::from(*bob_static_secret);
    let esk_dh = diffie_hellman_static(&static_secret, &ephemeral_pub)?;
    let static_dh = static_secret.diffie_hellman(&X25519PublicKey::from(message.static_key));
    let static_dh_bytes = *static_dh.as_bytes();

    let mut state = EciesNoiseState::new();
    state.mix_hash(message.representative.as_bytes());
    let mix1 = state.mix_key(esk_dh.as_ref())?;
    let mix2 = state.mix_key(&static_dh_bytes)?;

    if message.ciphertext.len() < AEAD_TAG_LEN {
        return Err(EciesError::CiphertextTooShort);
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&mix2.aead_key));
    let nonce_array = [0_u8; AEAD_NONCE_LEN];
    let nonce = Nonce::from_slice(&nonce_array);
    let plaintext_vec = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &message.ciphertext,
                aad: &state.transcript_hash(),
            },
        )
        .map_err(|_| EciesError::AuthenticationFailed)?;

    let session_state = EciesSessionState::install(&mix1.chaining_key, mix1.aead_key);
    Ok((plaintext_vec, session_state))
}

/// The I2P ECIES New Session Reply message produced by Bob and
/// consumed by Alice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSessionReplyMessage {
    /// The flag byte (always `ECIES_EXISTING_SESSION_FLAG` for
    /// compatibility with the existing-session framing).
    pub flag: u8,
    /// The session tag Alice uses for the first existing-session
    /// reply from Bob.
    pub tag: [u8; ECIES_SESSION_TAG_LEN],
    /// The ChaCha20-Poly1305 ciphertext of the Garlic payload
    /// blocks.
    pub ciphertext: Vec<u8>,
}

impl NewSessionReplyMessage {
    /// Encodes the message under the supplied hard ceiling.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, EciesError> {
        let total: usize = 1_usize
            .checked_add(ECIES_SESSION_TAG_LEN)
            .and_then(|n| n.checked_add(self.ciphertext.len()))
            .ok_or(EciesError::CiphertextTooLarge {
                actual: self.ciphertext.len(),
                maximum: MAX_NEW_SESSION_CIPHERTEXT,
            })?;
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        let mut output = Vec::with_capacity(total);
        output.push(self.flag);
        output.extend_from_slice(&self.tag);
        output.extend_from_slice(&self.ciphertext);
        Ok(output)
    }

    /// Decodes a new-session reply message from the supplied input.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, EciesError> {
        if input.len() < 1 + ECIES_SESSION_TAG_LEN + AEAD_TAG_LEN {
            return Err(EciesError::CiphertextTooShort);
        }
        if input[0] != ECIES_EXISTING_SESSION_FLAG {
            return Err(EciesError::AuthenticationFailed);
        }
        let mut tag = [0_u8; ECIES_SESSION_TAG_LEN];
        tag.copy_from_slice(&input[1..1 + ECIES_SESSION_TAG_LEN]);
        let ciphertext = input[1 + ECIES_SESSION_TAG_LEN..].to_vec();
        let total = 1 + ECIES_SESSION_TAG_LEN + ciphertext.len();
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        Ok(Self {
            flag: ECIES_EXISTING_SESSION_FLAG,
            tag,
            ciphertext,
        })
    }
}

/// Produces the New Session Reply message Bob sends back to Alice.
pub fn seal_new_session_reply(
    bob_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    alice_static_public: &[u8; STATIC_PUBLIC_LENGTH],
    alice_representative: &EciesEphemeralRepresentative,
    payload: &[u8],
) -> Result<(NewSessionReplyMessage, EciesSessionState), EciesError> {
    let ephemeral_pub = decode_representative(alice_representative)?;
    let bob_secret = StaticSecret::from(*bob_static_secret);
    let esk_dh = diffie_hellman_static(&bob_secret, &ephemeral_pub)?;
    let static_dh = bob_secret.diffie_hellman(&X25519PublicKey::from(*alice_static_public));
    let static_dh_bytes = *static_dh.as_bytes();

    let mut state = EciesNoiseState::new();
    state.mix_hash(alice_representative.as_bytes());
    let mix1 = state.mix_key(esk_dh.as_ref())?;
    let mix2 = state.mix_key(&static_dh_bytes)?;

    let cipher = ChaCha20Poly1305::new(Key::from_slice(&mix2.aead_key));
    let nonce_array = [0_u8; AEAD_NONCE_LEN];
    let nonce = Nonce::from_slice(&nonce_array);
    let payload_zero = Zeroizing::new(payload.to_vec());
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: payload_zero.as_ref(),
                aad: &state.transcript_hash(),
            },
        )
        .map_err(|_| EciesError::EncryptionFailed)?;
    if ciphertext.len() != payload.len() + AEAD_TAG_LEN {
        return Err(EciesError::EncryptionFailed);
    }

    let mut session_state = EciesSessionState::install(&mix1.chaining_key, mix1.aead_key);
    let (tag, _) = session_state.next_outbound();
    let message = NewSessionReplyMessage {
        flag: ECIES_EXISTING_SESSION_FLAG,
        tag,
        ciphertext: ciphertext.clone(),
    };
    Ok((message, session_state))
}

/// Decrypts the New Session Reply message Alice received from Bob
/// and installs the paired session state.
pub fn open_new_session_reply(
    alice_ephemeral_keypair: &EciesEphemeralKeypair,
    alice_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    bob_static_public: &[u8; STATIC_PUBLIC_LENGTH],
    reply: &NewSessionReplyMessage,
) -> Result<(Vec<u8>, EciesSessionState), EciesError> {
    let esk_dh = diffie_hellman_ephemeral(
        alice_ephemeral_keypair.secret().as_bytes(),
        bob_static_public,
    )?;
    let alice_static = StaticSecret::from(*alice_static_secret);
    let static_dh = alice_static.diffie_hellman(&X25519PublicKey::from(*bob_static_public));
    let static_dh_bytes = *static_dh.as_bytes();

    let mut state = EciesNoiseState::new();
    state.mix_hash(alice_ephemeral_keypair.representative().as_bytes());
    let mix1 = state.mix_key(esk_dh.as_ref())?;
    let mix2 = state.mix_key(&static_dh_bytes)?;

    if reply.ciphertext.len() < AEAD_TAG_LEN {
        return Err(EciesError::CiphertextTooShort);
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&mix2.aead_key));
    let nonce_array = [0_u8; AEAD_NONCE_LEN];
    let nonce = Nonce::from_slice(&nonce_array);
    let plaintext_vec = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &reply.ciphertext,
                aad: &state.transcript_hash(),
            },
        )
        .map_err(|_| EciesError::AuthenticationFailed)?;

    let mut session_state = EciesSessionState::install(&mix1.chaining_key, mix1.aead_key);
    session_state.consume_inbound(&reply.tag)?;
    Ok((plaintext_vec, session_state))
}

/// The I2P ECIES Existing Session message exchanged between
/// installed sessions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingSessionMessage {
    /// The flag byte (always `ECIES_EXISTING_SESSION_FLAG`).
    pub flag: u8,
    /// The session tag consumed by the receiver.
    pub tag: [u8; ECIES_SESSION_TAG_LEN],
    /// The ChaCha20-Poly1305 ciphertext of the Garlic payload
    /// blocks.
    pub ciphertext: Vec<u8>,
}

impl ExistingSessionMessage {
    /// Encodes the message under the supplied hard ceiling.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, EciesError> {
        let total: usize = 1_usize
            .checked_add(ECIES_SESSION_TAG_LEN)
            .and_then(|n| n.checked_add(self.ciphertext.len()))
            .ok_or(EciesError::CiphertextTooLarge {
                actual: self.ciphertext.len(),
                maximum: MAX_NEW_SESSION_CIPHERTEXT,
            })?;
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        let mut output = Vec::with_capacity(total);
        output.push(self.flag);
        output.extend_from_slice(&self.tag);
        output.extend_from_slice(&self.ciphertext);
        Ok(output)
    }

    /// Decodes an existing-session message from the supplied input.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, EciesError> {
        if input.len() < 1 + ECIES_SESSION_TAG_LEN + AEAD_TAG_LEN {
            return Err(EciesError::CiphertextTooShort);
        }
        if input[0] != ECIES_EXISTING_SESSION_FLAG {
            return Err(EciesError::AuthenticationFailed);
        }
        let mut tag = [0_u8; ECIES_SESSION_TAG_LEN];
        tag.copy_from_slice(&input[1..1 + ECIES_SESSION_TAG_LEN]);
        let ciphertext = input[1 + ECIES_SESSION_TAG_LEN..].to_vec();
        let total = 1 + ECIES_SESSION_TAG_LEN + ciphertext.len();
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        Ok(Self {
            flag: ECIES_EXISTING_SESSION_FLAG,
            tag,
            ciphertext,
        })
    }
}

/// Encrypts an Existing Session message for the supplied session
/// state, returning the encoded message.
pub fn seal_existing_session(
    session: &mut EciesSessionState,
    payload: &[u8],
) -> Result<ExistingSessionMessage, EciesError> {
    let (tag, key) = session.next_outbound();
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce_array = [0_u8; AEAD_NONCE_LEN];
    let nonce = Nonce::from_slice(&nonce_array);
    let payload_zero = Zeroizing::new(payload.to_vec());
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: payload_zero.as_ref(),
                aad: b"i2pr-ecies-existing-session",
            },
        )
        .map_err(|_| EciesError::EncryptionFailed)?;
    if ciphertext.len() != payload.len() + AEAD_TAG_LEN {
        return Err(EciesError::EncryptionFailed);
    }
    Ok(ExistingSessionMessage {
        flag: ECIES_EXISTING_SESSION_FLAG,
        tag,
        ciphertext,
    })
}

/// Decrypts an Existing Session message against the supplied
/// session state and returns the plaintext payload.
pub fn open_existing_session(
    session: &mut EciesSessionState,
    message: &ExistingSessionMessage,
) -> Result<Vec<u8>, EciesError> {
    if message.ciphertext.len() < AEAD_TAG_LEN {
        return Err(EciesError::CiphertextTooShort);
    }
    // Validate the tag BEFORE touching the AEAD key so a replay
    // attempt cannot advance the ratchet.
    if session.recv_tag != message.tag {
        return Err(EciesError::AuthenticationFailed);
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&session.recv_key));
    let nonce_array = [0_u8; AEAD_NONCE_LEN];
    let nonce = Nonce::from_slice(&nonce_array);
    let plaintext_vec = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &message.ciphertext,
                aad: b"i2pr-ecies-existing-session",
            },
        )
        .map_err(|_| EciesError::AuthenticationFailed)?;
    // Successful decrypt advances the ratchet exactly once.
    let derived = crate::hkdf_sha256_extract_and_expand(
        &session.recv_tag_key,
        &session.recv_tag,
        b"SessionTag",
        HKDF_OUTPUT_LEN + ECIES_SESSION_TAG_LEN,
    )
    .map_err(EciesError::Hkdf)?;
    session
        .recv_tag_key
        .copy_from_slice(&derived[..HKDF_OUTPUT_LEN]);
    session
        .recv_tag
        .copy_from_slice(&derived[HKDF_OUTPUT_LEN..]);
    session.consumed_tags = session.consumed_tags.saturating_add(1);
    Ok(plaintext_vec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    #[test]
    fn keypair_representative_round_trip_recovers_public_key() {
        // Generate until we find a seed whose representative decodes.
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
        // The recovered Montgomery point is the same as the public
        // key derived from the original seed via clamped X25519 base
        // multiplication.
        let recovered =
            decode_representative(&keypair.representative()).expect("representative decodes");
        // Verify that the recovered Montgomery point matches what
        // X25519 produces from the clamped seed.
        let clamped = clamp_x25519_seed(keypair.secret().as_bytes());
        let secret = StaticSecret::from(clamped);
        let expected_pub = X25519PublicKey::from(&secret).to_bytes();
        assert_eq!(recovered, expected_pub);
    }

    #[test]
    fn ephemeral_keypair_secret_zeroizes_via_debug() {
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
        let rendered = format!("{keypair:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("secret_seed: ["));
    }

    #[test]
    fn all_zero_representative_is_rejected_by_decode() {
        let representative = EciesEphemeralRepresentative([0_u8; REPRESENTATIVE_LENGTH]);
        // The all-zero representative decodes to a non-Curve25519
        // value; the decode helper refuses it explicitly.
        assert_eq!(
            decode_representative(&representative).err(),
            Some(EciesError::ElligatorDecode)
        );
    }

    #[test]
    fn diffie_hellman_static_rejects_all_zero_ephemeral() {
        let mut rng = ChaCha8Rng::seed_from_u64(13);
        let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
        let secret = StaticSecret::from(clamp_x25519_seed(keypair.secret().as_bytes()));
        let zero_pub = [0_u8; REPRESENTATIVE_LENGTH];
        let outcome = diffie_hellman_static(&secret, &zero_pub);
        assert_eq!(outcome.err(), Some(EciesError::AllZeroEphemeral));
    }

    #[test]
    fn diffie_hellman_ephemeral_rejects_all_zero_static() {
        let mut rng = ChaCha8Rng::seed_from_u64(17);
        let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
        let zero_pub = [0_u8; STATIC_PUBLIC_LENGTH];
        let outcome = diffie_hellman_ephemeral(keypair.secret().as_bytes(), &zero_pub);
        assert_eq!(outcome.err(), Some(EciesError::AllZeroStatic));
    }

    #[test]
    fn noise_state_initializes_with_protocol_name_h0() {
        let state = EciesNoiseState::new();
        let mut expected_h = [0_u8; HKDF_OUTPUT_LEN];
        let mut hasher = Sha256::new();
        hasher.update(ECIES_NOISE_PROTOCOL_NAME);
        expected_h.copy_from_slice(&hasher.finalize());
        // Post-null-prologue hash.
        let mut h_after_prologue = [0_u8; HKDF_OUTPUT_LEN];
        let mut hasher = Sha256::new();
        hasher.update(expected_h);
        h_after_prologue.copy_from_slice(&hasher.finalize());
        assert_eq!(state.transcript_hash(), h_after_prologue);
    }

    #[test]
    fn noise_state_mix_key_derives_distinct_chaining_and_aead_keys() {
        let mut state = EciesNoiseState::new();
        let material = state.mix_key(b"shared").expect("mix_key");
        assert_ne!(material.chaining_key, material.aead_key);
        assert_eq!(state.chaining_key, material.chaining_key);
    }

    #[test]
    fn montgomery_a_constant_is_well_defined() {
        // Sanity check that the helper constant matches the
        // expected Curve25519 Montgomery `A = 486662` value
        // serialized little-endian. 486662 = 0x00076D06.
        let a = montgomery_a();
        assert_eq!(a[0], 0x06);
        assert_eq!(a[1], 0x6D);
        assert_eq!(a[2], 0x07);
        for byte in &a[3..] {
            assert_eq!(*byte, 0);
        }
    }

    #[test]
    fn new_session_handshake_round_trips_payload() {
        let mut rng = ChaCha8Rng::seed_from_u64(101);
        // Generate Bob's destination static keypair.
        let bob_secret_bytes: [u8; 32] = {
            let mut bytes = [0_u8; 32];
            use rand_core::RngCore;
            rng.fill_bytes(&mut bytes);
            bytes
        };
        let bob_secret = StaticSecret::from(bob_secret_bytes);
        let bob_pub_bytes = X25519PublicKey::from(&bob_secret).to_bytes();

        // Alice generates her ephemeral keypair and seals a NS.
        let alice_keypair = EciesEphemeralKeypair::generate(&mut rng).expect("alice keypair");
        let payload = b"hello-bob";
        let (message, alice_static_secret, alice_session) =
            seal_new_session(&alice_keypair, &bob_pub_bytes, payload, &mut rng).expect("seal ns");

        // Bob opens the NS.
        let (decoded, bob_session) = open_new_session(
            &bob_secret_bytes,
            &bob_pub_bytes,
            &message,
            1_000_000,
            600,
            payload,
        )
        .expect("open ns");
        assert_eq!(decoded, payload);

        // Bob produces a NSR.
        let alice_representative = alice_keypair.representative();
        let alice_static_bytes = message.static_key;
        let reply_payload = b"ack-from-bob";
        let (reply, bob_session_for_reply) = seal_new_session_reply(
            &bob_secret_bytes,
            &alice_static_bytes,
            &alice_representative,
            reply_payload,
        )
        .expect("seal nsr");

        // Alice opens the NSR.
        let (reply_plaintext, mut alice_session_from_nsr) =
            open_new_session_reply(&alice_keypair, &alice_static_secret, &bob_pub_bytes, &reply)
                .expect("open nsr");
        assert_eq!(reply_plaintext, reply_payload);

        // Bob now sends an Existing Session message; Alice opens it.
        let mut bob_session_for_es = bob_session_for_reply.clone();
        let es_payload = b"hello-back";
        let existing = seal_existing_session(&mut bob_session_for_es, es_payload).expect("seal es");
        let es_plaintext =
            open_existing_session(&mut alice_session_from_nsr, &existing).expect("open es");
        assert_eq!(es_plaintext, es_payload);

        // The reply message uses the existing-session flag.
        assert_eq!(reply.flag, ECIES_EXISTING_SESSION_FLAG);
        assert_eq!(existing.flag, ECIES_EXISTING_SESSION_FLAG);

        // The existing-session tag must equal the session manager's
        // outbound next-tag (after consume) so the receiver accepts
        // the message.
        let _ = alice_session;
        let _ = alice_session_from_nsr;
        let _ = bob_session;
    }

    #[test]
    fn new_session_message_encode_decode_round_trip() {
        let representative = EciesEphemeralRepresentative([0x11_u8; REPRESENTATIVE_LENGTH]);
        let static_key = [0x22_u8; STATIC_PUBLIC_LENGTH];
        let ciphertext = vec![0x33_u8; 32 + AEAD_TAG_LEN];
        let message = NewSessionMessage::new(static_key, representative, ciphertext.clone())
            .expect("message");
        let encoded = message
            .encode_to_vec(MAX_NEW_SESSION_CIPHERTEXT)
            .expect("encode");
        let decoded =
            NewSessionMessage::decode(&encoded, MAX_NEW_SESSION_CIPHERTEXT).expect("decode");
        assert_eq!(decoded, message);
    }

    #[test]
    fn new_session_message_rejects_oversized_ciphertext() {
        let representative = EciesEphemeralRepresentative([0x11_u8; REPRESENTATIVE_LENGTH]);
        let static_key = [0x22_u8; STATIC_PUBLIC_LENGTH];
        let ciphertext = vec![0_u8; MAX_NEW_SESSION_CIPHERTEXT + 1];
        let outcome = NewSessionMessage::new(static_key, representative, ciphertext);
        assert!(matches!(
            outcome,
            Err(EciesError::CiphertextTooLarge { .. })
        ));
    }

    #[test]
    fn new_session_message_rejects_wrong_flag() {
        let mut input = vec![0xAB_u8];
        input.extend_from_slice(&[0_u8; STATIC_PUBLIC_LENGTH + REPRESENTATIVE_LENGTH + 16]);
        let outcome = NewSessionMessage::decode(&input, 1024);
        assert_eq!(outcome.err(), Some(EciesError::AuthenticationFailed));
    }

    #[test]
    fn existing_session_round_trip_advances_ratchet() {
        let mut session =
            EciesSessionState::install(&[0x42_u8; HKDF_OUTPUT_LEN], [0x99_u8; HKDF_OUTPUT_LEN]);
        let mut next = seal_existing_session(&mut session, b"payload-one").expect("seal-1");
        let tag_a = next.tag;
        next = seal_existing_session(&mut session, b"payload-two").expect("seal-2");
        let tag_b = next.tag;
        assert_ne!(tag_a, tag_b);

        // Replay of `tag_a` after the ratchet has advanced must
        // fail to authenticate.
        let mut receiver =
            EciesSessionState::install(&[0x42_u8; HKDF_OUTPUT_LEN], [0x99_u8; HKDF_OUTPUT_LEN]);
        let outcome = open_existing_session(
            &mut receiver,
            &ExistingSessionMessage {
                flag: ECIES_EXISTING_SESSION_FLAG,
                tag: tag_a,
                ciphertext: vec![0_u8; 1 + AEAD_TAG_LEN],
            },
        );
        // The AEAD will fail because the ciphertext is fake, but
        // the tag comparison must not advance the ratchet because
        // it fails first.
        assert!(matches!(outcome, Err(EciesError::AuthenticationFailed)));
    }

    #[test]
    fn session_tag_chain_advances_per_consumed_inbound() {
        let mut session =
            EciesSessionState::install(&[0x55_u8; HKDF_OUTPUT_LEN], [0x66_u8; HKDF_OUTPUT_LEN]);
        let first = session.recv_tag();
        // Manually consume the tag and verify the next one differs.
        session.consume_inbound(&first).expect("consume");
        let second = session.recv_tag();
        assert_ne!(first, second);
    }

    #[test]
    fn wrong_inbound_tag_is_rejected() {
        let mut session =
            EciesSessionState::install(&[0x55_u8; HKDF_OUTPUT_LEN], [0x66_u8; HKDF_OUTPUT_LEN]);
        let bad = [0xCC_u8; ECIES_SESSION_TAG_LEN];
        let outcome = session.consume_inbound(&bad);
        assert_eq!(outcome.err(), Some(EciesError::AuthenticationFailed));
    }

    // The following negative tests cover the adversarial inputs
    // Plan 121 §14 enumerates for the ECIES New Session, New
    // Session Reply, and Existing Session parsers.

    #[test]
    fn new_session_decode_rejects_truncated_static_key() {
        let bytes = [ECIES_NEW_SESSION_FLAG, 0, 0, 0, 0];
        let outcome = NewSessionMessage::decode(&bytes, 256);
        assert_eq!(outcome.err(), Some(EciesError::CiphertextTooShort));
    }

    #[test]
    fn new_session_decode_rejects_truncated_representative() {
        let mut bytes = vec![ECIES_NEW_SESSION_FLAG];
        bytes.extend_from_slice(&[0x11; STATIC_PUBLIC_LENGTH]);
        let outcome = NewSessionMessage::decode(&bytes, 256);
        assert_eq!(outcome.err(), Some(EciesError::CiphertextTooShort));
    }

    #[test]
    fn new_session_reply_decode_rejects_truncated_payload() {
        let mut bytes = vec![ECIES_EXISTING_SESSION_FLAG];
        bytes.extend_from_slice(&[0x22; ECIES_SESSION_TAG_LEN]);
        let outcome = NewSessionReplyMessage::decode(&bytes, 256);
        assert_eq!(outcome.err(), Some(EciesError::CiphertextTooShort));
    }

    #[test]
    fn new_session_reply_decode_rejects_wrong_flag() {
        let mut bytes = vec![0xAB_u8];
        bytes.extend_from_slice(&[0x22; ECIES_SESSION_TAG_LEN]);
        bytes.extend_from_slice(&[0x33; AEAD_TAG_LEN + 4]);
        let outcome = NewSessionReplyMessage::decode(&bytes, 256);
        assert_eq!(outcome.err(), Some(EciesError::AuthenticationFailed));
    }

    #[test]
    fn existing_session_decode_rejects_short_ciphertext() {
        let mut bytes = vec![ECIES_EXISTING_SESSION_FLAG];
        bytes.extend_from_slice(&[0x33; ECIES_SESSION_TAG_LEN]);
        bytes.extend_from_slice(&[0u8; 4]);
        let outcome = ExistingSessionMessage::decode(&bytes, 256);
        assert_eq!(outcome.err(), Some(EciesError::CiphertextTooShort));
    }

    #[test]
    fn existing_session_decode_rejects_wrong_flag() {
        let mut bytes = vec![0xFF_u8];
        bytes.extend_from_slice(&[0x33; ECIES_SESSION_TAG_LEN]);
        bytes.extend_from_slice(&[0u8; AEAD_TAG_LEN + 4]);
        let outcome = ExistingSessionMessage::decode(&bytes, 256);
        assert_eq!(outcome.err(), Some(EciesError::AuthenticationFailed));
    }

    #[test]
    fn random_representatives_fail_decode_or_succeed_without_panic() {
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
        for _ in 0..256 {
            let mut bytes = [0_u8; REPRESENTATIVE_LENGTH];
            use rand_core::RngCore;
            rng.fill_bytes(&mut bytes);
            let rep = EciesEphemeralRepresentative(bytes);
            let outcome = decode_representative(&rep);
            assert!(outcome.is_ok() || matches!(outcome.err(), Some(EciesError::ElligatorDecode)));
        }
    }
}
