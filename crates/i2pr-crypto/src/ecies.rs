//! The I2P ECIES-X25519-AEAD-Ratchet destination session layer
//! (Plan 126).
//!
//! This module implements the bounded Milestone 6 subset of the
//! current I2P end-to-end encryption specification for repliable
//! destination traffic (`https://i2p.net/en/docs/specs/ecies/`,
//! proposal 144):
//!
//! - bound New Session (reply expected),
//! - New Session Reply,
//! - Existing Session,
//! - type-4 X25519 LeaseSet2 static keys.
//!
//! The Noise initializer is the literal I2P contract
//! `Noise_IKelg2+hs2_25519_ChaChaPoly_SHA256` (40 bytes). Alice's
//! destination static X25519 public key travels encrypted and
//! authenticated inside the New Session static-key section, exactly
//! as the specification requires for bound sessions. New Session
//! Reply and Existing Session messages are classified by their
//! leading 8-byte session tags; there are no message-type bytes on
//! the wire.
//!
//! The KDF/transcript ordering, the post-handshake Noise Split into
//! `k_ab` / `k_ba`, the `"AttachPayloadKDF"` New Session Reply
//! payload key, and the session-tag/symmetric-key ratchets follow the
//! normative equations cross-checked against the pinned reference
//! implementations (see `specs/references/ecies-destination-ratchet.md`).
//!
//! Explicitly outside this module:
//!
//! - session lifecycle, replay caches, tag-window bookkeeping, and
//!   per-destination policy live in `i2pr_client::session`;
//! - Garlic payload blocks live in `i2pr_proto::ecies_payload`;
//! - destination identity ownership lives in
//!   `i2pr_client::identity`; the seal/open functions only borrow
//!   raw X25519 key slices;
//! - unbound/one-time New Sessions, multicast, post-quantum hybrid
//!   ratchets, MessageNumbers/Options/Termination blocks, and the
//!   optional NextKey DH-ratchet policy are not supported and fail
//!   closed with typed errors.

#![forbid(unsafe_code)]

use core::fmt;

use rand_core::TryCryptoRng;
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::HkdfError;

/// The literal I2P Noise protocol name for the destination ECIES
/// KDF. The 40-byte US-ASCII string includes the `+hs2` handshake
/// suffix the current specification and every deployed reference
/// implementation use.
pub const ECIES_NOISE_PROTOCOL_NAME: &[u8] = b"Noise_IKelg2+hs2_25519_ChaChaPoly_SHA256";

/// Length of an Elligator2 representative / ephemeral public key.
pub const REPRESENTATIVE_LENGTH: usize = 32;
/// Length of a static destination X25519 public key (crypto type 4).
pub const STATIC_PUBLIC_LENGTH: usize = 32;
/// Length of an ECIES session tag.
pub const SESSION_TAG_LENGTH: usize = 8;
/// Length of a ChaCha20-Poly1305 authentication tag.
pub const AEAD_TAG_LEN: usize = 16;
/// Length of a ChaCha20-Poly1305 nonce (IETF variant).
pub const AEAD_NONCE_LEN: usize = 12;
/// Length of an HKDF-SHA256 output chunk.
pub const HKDF_OUTPUT_LEN: usize = 32;

/// Encrypted bound New Session overhead: 32-byte Elligator2
/// representative + 48-byte encrypted static-key section (including
/// its MAC) + 16-byte payload-section MAC floor. Total message
/// length is `BOUND_NEW_SESSION_MIN_LENGTH + payload_len`.
pub const BOUND_NEW_SESSION_MIN_LENGTH: usize =
    REPRESENTATIVE_LENGTH + STATIC_PUBLIC_LENGTH + AEAD_TAG_LEN + AEAD_TAG_LEN;
/// New Session Reply overhead: 8-byte tag + 32-byte Elligator2
/// representative + 16-byte empty key-section MAC + 16-byte payload
/// MAC floor.
pub const NEW_SESSION_REPLY_MIN_LENGTH: usize =
    SESSION_TAG_LENGTH + REPRESENTATIVE_LENGTH + AEAD_TAG_LEN + AEAD_TAG_LEN;
/// Existing Session overhead: 8-byte tag + 16-byte payload MAC
/// floor.
pub const EXISTING_SESSION_MIN_LENGTH: usize = SESSION_TAG_LENGTH + AEAD_TAG_LEN;

/// The maximum payload accepted by the seam. The specification
/// permits up to 65535-byte payloads inside a single I2NP Garlic
/// message; the seam caps at one bounded I2NP body so `u16` payload
/// lengths never overflow inside `i2pr-proto`.
pub const MAX_NEW_SESSION_CIPHERTEXT: usize = 65_507;

/// Hard ceiling on the number of tags a single tag set may issue
/// (the specification maximum is 65535).
const MAX_TAG_SET_INDEX: u64 = 65_535;
/// Hard ceiling on the symmetric keys a single tag set retains at
/// once. The manager-level look-ahead window is far smaller; this is
/// the primitive-level memory guard.
const MAX_TAG_SET_RETAINED_KEYS: usize = 4_096;

/// Errors returned by the ECIES seam.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EciesError {
    /// The Elligator2 representative could not be decoded. About half
    /// of all 32-byte strings are valid representatives; the receiver
    /// must request a retry rather than treat this as an attack.
    #[error("ECIES Elligator2 representative could not be decoded")]
    ElligatorDecode,
    /// A supplied public or private key was the forbidden all-zero
    /// value.
    #[error("ECIES key material is the forbidden all-zero value")]
    AllZeroKey,
    /// A Diffie-Hellman operation produced the forbidden all-zero
    /// shared secret.
    #[error("ECIES Diffie-Hellman produced the forbidden all-zero shared secret")]
    InvalidSharedSecret,
    /// ChaCha20-Poly1305 authentication failed during open.
    #[error("ECIES AEAD authentication failed")]
    AuthenticationFailed,
    /// ChaCha20-Poly1305 encryption failed (highly unexpected).
    #[error("ECIES AEAD encryption failed")]
    EncryptionFailed,
    /// A supplied message exceeded the local ceiling.
    #[error("ECIES message length {actual} exceeds ceiling {maximum}")]
    CiphertextTooLarge {
        /// Actual message length.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// A supplied message was shorter than its structural minimum.
    #[error("ECIES message length {actual} is below the required minimum {minimum}")]
    CiphertextTooShort {
        /// Actual message length.
        actual: usize,
        /// Structural minimum.
        minimum: usize,
    },
    /// A decrypted bound New Session carried an all-zero (unbound)
    /// static-key section. Unbound sessions are outside the supported
    /// Milestone 6 subset and never interpreted as bound traffic.
    #[error("ECIES unbound (zero static key) New Session is not supported")]
    UnboundNewSessionNotSupported,
    /// The tag set issued its maximum number of tags.
    #[error("ECIES tag set exhausted its maximum tag index")]
    TagSetExhausted,
    /// The requested tag-set symmetric key index exceeded the local
    /// retention ceiling.
    #[error("ECIES tag set symmetric key index {index} exceeds the retained ceiling {maximum}")]
    TagSetIndexBeyondCeiling {
        /// Requested index.
        index: u64,
        /// Retention ceiling.
        maximum: usize,
    },
    /// The HKDF helper returned an error (impossible at the
    /// configured limits).
    #[error("ECIES HKDF derivation failed: {0}")]
    Hkdf(HkdfError),
    /// The supplied cryptographic RNG failed.
    #[error("ECIES cryptographic randomness unavailable")]
    RandomnessUnavailable,
}

/// An Elligator2 representative for an ECIES ephemeral public key.
///
/// The representative is the 32-byte on-wire encoding of an X25519
/// public key (`X25519(esk, G)`). The wrapper is non-secret and may
/// be copied or serialized.
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
/// The secret is the 32-byte ephemeral scalar used for the
/// handshake Diffie-Hellman operations; the representative is the
/// on-wire encoding the receiver decodes back into the same
/// Montgomery public key. The secret zeroizes on drop; the struct
/// does not implement `Clone` or byte-revealing `Debug`.
pub struct EciesEphemeralKeypair {
    secret_seed: EciesEphemeralSecret,
    representative: EciesEphemeralRepresentative,
}

impl EciesEphemeralKeypair {
    /// Generates a new ephemeral keypair for production wire use.
    ///
    /// Plan 131: the on-wire Elligator2 representation is randomized
    /// exactly as current deployed Java I2P / i2pd encoders randomize
    /// it — the tweak's low bit selects the inverse-map pre-image
    /// branch (between `u` and `u+A`) and the tweak's top two bits
    /// populate the encoded representative's free bits
    /// (`ENCODE_ELG2`: `encodedKey[31] |= randomByte & 0xc0`). The
    /// canonical decoded X25519 public key (and therefore every
    /// transcript hash and Diffie-Hellman operation) is unaffected
    /// by the randomized branch or the randomized high bits because
    /// every conforming decoder masks the high bits off before
    /// mapping (`DECODE_ELG2`) and the inverse-map branch
    /// represents the same Montgomery `u`-coordinate.
    ///
    /// The Elligator2 mapping succeeds for roughly half of the
    /// 32-byte public points from a CSPRNG; this constructor retries
    /// on the rare non-representable failure rather than corrupting
    /// the handshake. Attempts are hard-bounded and entropy failures
    /// surface as [`EciesError::RandomnessUnavailable`].
    pub fn generate<R: TryCryptoRng + ?Sized>(rng: &mut R) -> Result<Self, EciesError> {
        const MAX_ATTEMPTS: usize = 64;
        for _ in 0..MAX_ATTEMPTS {
            let mut seed_bytes = Zeroizing::new([0_u8; REPRESENTATIVE_LENGTH]);
            rng.try_fill_bytes(&mut *seed_bytes)
                .map_err(|_| EciesError::RandomnessUnavailable)?;
            let mut tweak = [0_u8; 1];
            rng.try_fill_bytes(&mut tweak)
                .map_err(|_| EciesError::RandomnessUnavailable)?;
            if let Some(candidate) = Self::build(*seed_bytes, tweak[0]) {
                return Ok(candidate);
            }
        }
        Err(EciesError::RandomnessUnavailable)
    }

    /// Builds a keypair from an explicit seed with a **fixed**
    /// representation choice (tweak `0`: branch bit 0, high bits
    /// `00`). This is the deterministic test/vector constructor
    /// only: its representatives carry the implementation-fixed
    /// high-bit pattern `00` and the implementation-fixed branch
    /// bit, which is suitable for frozen KDF/Noise vectors but
    /// must never be used for production on-wire anonymity.
    /// Production callers must use [`Self::generate`]. Returns
    /// `None` when the seed does not map to a decodable
    /// representative; callers must retry with a fresh seed.
    pub fn from_seed_bytes(seed: [u8; REPRESENTATIVE_LENGTH]) -> Option<Self> {
        Self::build(seed, 0)
    }

    /// Builds a keypair from an explicit seed and an explicit
    /// 8-bit tweak. The deterministic-vector constructor accepts
    /// the tweak so frozen Plan 126 fixtures may continue to use
    /// `from_seed_bytes(seed)` (= `build(seed, 0)`) while a future
    /// branch-aware test may exercise other high-bit / branch
    /// combinations through this entry point. Production callers
    /// must not invoke this directly; they must use
    /// [`Self::generate`] which draws a fresh CSPRNG seed and
    /// tweak for every attempt.
    pub fn from_seed_bytes_with_tweak(
        seed: [u8; REPRESENTATIVE_LENGTH],
        tweak: u8,
    ) -> Option<Self> {
        Self::build(seed, tweak)
    }

    /// Shared construction seam. Derives the canonical (RFC 7748-
    /// clamped) X25519 public point, then delegates to the
    /// reviewed [`elligator2::to_representative`] primitive for the
    /// Elligator2 inverse map. The primitive takes the Montgomery
    /// `u`-coordinate as input and a full 8-bit tweak:
    ///
    /// - `tweak & 0x01` selects between the two deployed-reference
    ///   inverse-map branches `sqrt(-u/(2*(u+A)))` and
    ///   `sqrt(-(u+A)/(2*u))`;
    /// - `tweak & 0xc0` populates the two free representation bits
    ///   per `ENCODE_ELG2`.
    ///
    /// Decoders mask the high bits off before mapping and the two
    /// branches decode to the same Montgomery `u`-coordinate, so
    /// the secret → representative mapping changes no protocol
    /// value. The encoded representative is the on-wire ephemeral
    /// public key.
    fn build(seed: [u8; REPRESENTATIVE_LENGTH], tweak: u8) -> Option<Self> {
        // The seed is the X25519 scalar after RFC 7748 clamping.
        // The public point is `X25519(clamp(seed), basepoint)`;
        // we pass that Montgomery `u`-coordinate to the reviewed
        // Elligator2 inverse-map primitive.
        let clamped = clamp_x25519_seed(&seed);
        let public_point = X25519PublicKey::from(&StaticSecret::from(clamped)).to_bytes();
        if public_point.iter().all(|byte| *byte == 0) {
            return None;
        }
        let representative_opt = elligator2::to_representative(&public_point, tweak);
        let representative = representative_opt?;
        if representative.iter().all(|byte| *byte == 0) {
            return None;
        }
        // The deterministic test/vector constructor pins the
        // canonicalized secret to the clamped form so every frozen
        // Plan 126 KDF/Noise vector remains byte-for-byte stable.
        // Production generation does not consult this field, only
        // the representative.
        let mut canonical_seed = seed;
        canonical_seed[REPRESENTATIVE_LENGTH - 1] &= 0x3f;
        Some(Self {
            secret_seed: EciesEphemeralSecret(canonical_seed),
            representative: EciesEphemeralRepresentative(representative),
        })
    }

    /// Returns the secret seed (zeroizing owner).
    pub const fn secret(&self) -> &EciesEphemeralSecret {
        &self.secret_seed
    }

    /// Returns the Elligator2 representative (the on-wire ephemeral
    /// public key bytes).
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
/// Montgomery public-key bytes. The reverse map is total (every
/// 32-byte string decodes to some point) and the reviewed
/// `elligator2::from_representative` primitive masks off the two
/// free high bits before mapping, so the result is invariant to
/// the random bits ORed in at encode time.
pub fn decode_representative(
    representative: &EciesEphemeralRepresentative,
) -> Result<[u8; REPRESENTATIVE_LENGTH], EciesError> {
    let bytes = representative.as_bytes();
    if bytes.iter().all(|byte| *byte == 0) {
        return Err(EciesError::ElligatorDecode);
    }
    let recovered = elligator2::from_representative(bytes);
    if recovered.iter().all(|byte| *byte == 0) {
        return Err(EciesError::ElligatorDecode);
    }
    Ok(recovered)
}

/// Computes the Montgomery public key for a scalar seed using the
/// RFC 7748 clamping rules.
#[cfg(test)]
pub(crate) fn ephemeral_public_key_for_test(
    seed: &[u8; REPRESENTATIVE_LENGTH],
) -> [u8; REPRESENTATIVE_LENGTH] {
    let clamped = clamp_x25519_seed(seed);
    X25519PublicKey::from(&StaticSecret::from(clamped)).to_bytes()
}

/// Performs an X25519 Diffie-Hellman between a private scalar and a
/// peer public key, rejecting all-zero inputs and results.
fn diffie_hellman_checked(
    private: &[u8; REPRESENTATIVE_LENGTH],
    peer_public: &[u8; REPRESENTATIVE_LENGTH],
) -> Result<Zeroizing<[u8; HKDF_OUTPUT_LEN]>, EciesError> {
    if private.iter().all(|byte| *byte == 0) || peer_public.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    let secret = StaticSecret::from(*private);
    let peer = X25519PublicKey::from(*peer_public);
    let shared = secret.diffie_hellman(&peer);
    if shared.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(EciesError::InvalidSharedSecret);
    }
    let mut out = Zeroizing::new([0_u8; HKDF_OUTPUT_LEN]);
    out.copy_from_slice(shared.as_bytes());
    Ok(out)
}

/// Applies the X25519 scalar clamping rules to a 32-byte seed: clear
/// the low three bits, clear the high bit of byte 31, and set the
/// second-highest bit of byte 31. Production DH paths rely on the
/// RFC 7748 clamping performed inside `x25519-dalek`; this explicit
/// form backs the deterministic public-key assertions and the
/// representative builder.
fn clamp_x25519_seed(seed: &[u8; REPRESENTATIVE_LENGTH]) -> [u8; REPRESENTATIVE_LENGTH] {
    let mut out = *seed;
    out[0] &= 248;
    out[REPRESENTATIVE_LENGTH - 1] &= 127;
    out[REPRESENTATIVE_LENGTH - 1] |= 64;
    out
}

/// The Noise symmetric state for one ECIES handshake: the chaining
/// key plus the running transcript hash.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub(crate) struct EciesNoiseState {
    chaining_key: [u8; HKDF_OUTPUT_LEN],
    transcript_hash: [u8; HKDF_OUTPUT_LEN],
}

impl EciesNoiseState {
    /// Initializes the state exactly as the specification requires:
    ///
    /// ```text
    /// h  = SHA256(protocol_name)
    /// ck = h
    /// h  = SHA256(h)              // MixHash(null prologue)
    /// ```
    pub fn new() -> Self {
        let h0 = sha256(ECIES_NOISE_PROTOCOL_NAME);
        let prologue = sha256(&h0);
        Self {
            chaining_key: h0,
            transcript_hash: prologue,
        }
    }

    /// Returns the current chaining key.
    pub fn chaining_key(&self) -> [u8; HKDF_OUTPUT_LEN] {
        self.chaining_key
    }

    /// Returns the current transcript hash.
    pub fn transcript_hash(&self) -> [u8; HKDF_OUTPUT_LEN] {
        self.transcript_hash
    }

    /// Overwrites both components (used when a handshake step must be
    /// applied to an explicitly restored state).
    pub(crate) fn set_from(
        &mut self,
        chaining_key: &[u8; HKDF_OUTPUT_LEN],
        transcript_hash: &[u8; HKDF_OUTPUT_LEN],
    ) {
        self.chaining_key = *chaining_key;
        self.transcript_hash = *transcript_hash;
    }

    /// `MixHash(d)`: `h = SHA256(h || d)`.
    pub fn mix_hash(&mut self, data: &[u8]) {
        let mut hasher = Sha256::new();
        hasher.update(self.transcript_hash);
        hasher.update(data);
        self.transcript_hash.copy_from_slice(&hasher.finalize());
    }

    /// `MixKey(d)`: `[ck, k] = HKDF(ck, d, "", 64)`; returns `k` for
    /// the immediately-following AEAD operation (nonce resets to 0).
    pub fn mix_key(&mut self, ikm: &[u8]) -> Result<[u8; HKDF_OUTPUT_LEN], EciesError> {
        let derived = crate::hkdf_sha256_extract_and_expand(&self.chaining_key, ikm, b"", 64)
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
        Ok(aead_key)
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

impl Default for EciesNoiseState {
    fn default() -> Self {
        Self::new()
    }
}

fn sha256(data: &[u8]) -> [u8; HKDF_OUTPUT_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut out = [0_u8; HKDF_OUTPUT_LEN];
    out.copy_from_slice(&digest);
    out
}

/// Builds the IETF ChaCha20-Poly1305 nonce for a message-number
/// index: four zero bytes followed by the little-endian index.
fn aead_nonce(index: u64) -> Nonce {
    let mut nonce = [0_u8; AEAD_NONCE_LEN];
    nonce[4..].copy_from_slice(&index.to_le_bytes());
    Nonce::from(nonce)
}

/// Encrypts `plaintext` under `key` with nonce `index` and associated
/// data `ad`, returning ciphertext || MAC.
fn aead_encrypt(
    key: &[u8; HKDF_OUTPUT_LEN],
    index: u64,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, EciesError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .encrypt(
            &aead_nonce(index),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| EciesError::EncryptionFailed)
}

/// Decrypts `ciphertext` (including its MAC) under `key` with nonce
/// `index` and associated data `ad`.
fn aead_decrypt(
    key: &[u8; HKDF_OUTPUT_LEN],
    index: u64,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, EciesError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            &aead_nonce(index),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| EciesError::AuthenticationFailed)
}

/// One issued entry of a tag set: the cleartext 8-byte session tag
/// plus the 0-based index selecting its symmetric key and AEAD nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EciesTagSetEntry {
    /// The 0-based message index (key and nonce selector).
    pub index: u32,
    /// The 8-byte cleartext session tag.
    pub tag: [u8; SESSION_TAG_LENGTH],
}

/// One direction of the paired post-handshake session: the
/// session-tag ratchet and the independent symmetric-key ratchet the
/// specification derives with `DH_INITIALIZE` +
/// `NextSessionTagRatchet`.
///
/// Tags are 1-based on the wire (the first issued entry carries
/// `tag_1`) while keys and nonces are 0-based: the Nth issued entry
/// pairs `tag_N` with `key_{N-1}` and nonce `N-1`, matching the
/// reference implementations.
///
/// The type owns live key material: it is not `Clone`, it zeroizes
/// on drop, and every consuming operation takes `&mut self` so a
/// caller cannot silently advance a copy and lose the ratchet state.
pub struct EciesTagSet {
    tag_chain_key: [u8; HKDF_OUTPUT_LEN],
    tag_constant: [u8; HKDF_OUTPUT_LEN],
    symm_chain_key: [u8; HKDF_OUTPUT_LEN],
    symm_keys: Vec<[u8; HKDF_OUTPUT_LEN]>,
    /// Chain index of `symm_keys[0]`; advances when trimmed keys are
    /// dropped so retained keys keep their absolute ratchet indices.
    symm_keys_base: u32,
    next_index: u32,
}

impl fmt::Debug for EciesTagSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EciesTagSet")
            .field("next_index", &self.next_index)
            .field("retained_keys", &self.symm_keys.len())
            .finish()
    }
}

impl Zeroize for EciesTagSet {
    fn zeroize(&mut self) {
        self.tag_chain_key.zeroize();
        self.tag_constant.zeroize();
        self.symm_chain_key.zeroize();
        for key in &mut self.symm_keys {
            key.zeroize();
        }
        self.symm_keys.clear();
        self.next_index.zeroize();
    }
}

impl ZeroizeOnDrop for EciesTagSet {}

impl EciesTagSet {
    /// `DH_INITIALIZE(rootKey, k)`:
    ///
    /// ```text
    /// keydata = HKDF(rootKey, k, "KDFDHRatchetStep", 64)
    /// _nextRootKey = keydata[0:31]
    /// keydata = HKDF(keydata[32:63], ZEROLEN, "TagAndKeyGenKeys", 64)
    /// sessTag_ck = keydata[0:31]
    /// symmKey_ck = keydata[32:63]
    /// ```
    pub fn dh_initialize(
        root_key: &[u8; HKDF_OUTPUT_LEN],
        k: &[u8; HKDF_OUTPUT_LEN],
    ) -> Result<Self, EciesError> {
        let step = crate::hkdf_sha256_extract_and_expand(root_key, k, b"KDFDHRatchetStep", 64)
            .map_err(EciesError::Hkdf)?;
        let mut ck = [0_u8; HKDF_OUTPUT_LEN];
        ck.copy_from_slice(&step[HKDF_OUTPUT_LEN..]);
        let generated = crate::hkdf_sha256_extract_and_expand(&ck, b"", b"TagAndKeyGenKeys", 64)
            .map_err(EciesError::Hkdf)?;
        let mut tag_chain_key = [0_u8; HKDF_OUTPUT_LEN];
        tag_chain_key.copy_from_slice(&generated[..HKDF_OUTPUT_LEN]);
        let mut symm_chain_key = [0_u8; HKDF_OUTPUT_LEN];
        symm_chain_key.copy_from_slice(&generated[HKDF_OUTPUT_LEN..]);
        Ok(Self {
            tag_chain_key,
            tag_constant: [0_u8; HKDF_OUTPUT_LEN],
            symm_chain_key,
            symm_keys: Vec::new(),
            symm_keys_base: 0,
            next_index: 0,
        })
    }

    /// Builds the New Session Reply tag set from the final New
    /// Session chaining key:
    ///
    /// ```text
    /// tagsetKey = HKDF(chainKey, ZEROLEN, "SessionReplyTags", 32)
    /// tagset_nsr = DH_INITIALIZE(chainKey, tagsetKey)
    /// ```
    pub fn new_session_reply_tag_set(
        ns_chaining_key: &[u8; HKDF_OUTPUT_LEN],
    ) -> Result<Self, EciesError> {
        let tagset_key = crate::hkdf_sha256_extract_and_expand(
            ns_chaining_key,
            b"",
            b"SessionReplyTags",
            HKDF_OUTPUT_LEN,
        )
        .map_err(EciesError::Hkdf)?;
        let mut tagset_key_array = [0_u8; HKDF_OUTPUT_LEN];
        tagset_key_array.copy_from_slice(&tagset_key);
        Self::dh_initialize(ns_chaining_key, &tagset_key_array)
    }

    /// `NextSessionTagRatchet()`:
    ///
    /// ```text
    /// keydata = HKDF(sessTag_ck, ZEROLEN, "STInitialization", 64)
    /// sessTag_chainKey = keydata[0:31]
    /// SESSTAG_CONSTANT = keydata[32:63]
    /// ```
    ///
    /// Must be called once after [`Self::dh_initialize`] before any
    /// entries are issued.
    pub fn begin_tag_ratchet(&mut self) -> Result<(), EciesError> {
        let initialized = crate::hkdf_sha256_extract_and_expand(
            &self.tag_chain_key,
            b"",
            b"STInitialization",
            64,
        )
        .map_err(EciesError::Hkdf)?;
        self.tag_chain_key
            .copy_from_slice(&initialized[..HKDF_OUTPUT_LEN]);
        self.tag_constant
            .copy_from_slice(&initialized[HKDF_OUTPUT_LEN..]);
        self.next_index = 0;
        Ok(())
    }

    /// Issues the next `(index, tag)` entry: one `RATCHET_TAG` step.
    /// The returned `index` is the pre-increment counter value, which
    /// selects the symmetric key and AEAD nonce for the message that
    /// carries this tag.
    pub fn next_entry(&mut self) -> Result<EciesTagSetEntry, EciesError> {
        if u64::from(self.next_index) + 1 >= MAX_TAG_SET_INDEX {
            return Err(EciesError::TagSetExhausted);
        }
        let derived = crate::hkdf_sha256_extract_and_expand(
            &self.tag_chain_key,
            &self.tag_constant,
            b"SessionTagKeyGen",
            64,
        )
        .map_err(EciesError::Hkdf)?;
        self.tag_chain_key
            .copy_from_slice(&derived[..HKDF_OUTPUT_LEN]);
        let mut tag = [0_u8; SESSION_TAG_LENGTH];
        tag.copy_from_slice(&derived[HKDF_OUTPUT_LEN..HKDF_OUTPUT_LEN + SESSION_TAG_LENGTH]);
        let index = self.next_index;
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or(EciesError::TagSetExhausted)?;
        Ok(EciesTagSetEntry { index, tag })
    }

    /// Returns the number of entries already issued.
    pub fn issued_entries(&self) -> u32 {
        self.next_index
    }

    /// Returns (deriving on demand) the symmetric key for `index`.
    /// Each step is one `RATCHET_KEY` round:
    ///
    /// ```text
    /// keydata = HKDF(symmKey_chainKey, ZEROLEN, "SymmetricRatchet", 64)
    /// ```
    pub fn symm_key(&mut self, index: u32) -> Result<&[u8; HKDF_OUTPUT_LEN], EciesError> {
        if index as usize >= MAX_TAG_SET_RETAINED_KEYS {
            return Err(EciesError::TagSetIndexBeyondCeiling {
                index: u64::from(index),
                maximum: MAX_TAG_SET_RETAINED_KEYS,
            });
        }
        while self.symm_keys_base + self.symm_keys.len() as u32 <= index {
            let derived = crate::hkdf_sha256_extract_and_expand(
                &self.symm_chain_key,
                b"",
                b"SymmetricRatchet",
                64,
            )
            .map_err(EciesError::Hkdf)?;
            self.symm_chain_key
                .copy_from_slice(&derived[..HKDF_OUTPUT_LEN]);
            let mut key = [0_u8; HKDF_OUTPUT_LEN];
            key.copy_from_slice(&derived[HKDF_OUTPUT_LEN..]);
            self.symm_keys.push(key);
        }
        let position = (index - self.symm_keys_base) as usize;
        Ok(&self.symm_keys[position])
    }

    /// Drops all retained symmetric keys below `index` so skipped or
    /// consumed traffic cannot accumulate memory.
    pub fn trim_keys_below(&mut self, index: u32) {
        let drop = (index.saturating_sub(self.symm_keys_base) as usize).min(self.symm_keys.len());
        self.symm_keys.drain(..drop);
        self.symm_keys_base += drop as u32;
    }
}

/// The bound New Session message: the canonical I2P encrypted-data
/// layout with binding.
///
/// ```text
/// Elligator2-encoded Alice ephemeral public key     32 bytes, clear
/// Alice static public key section                   32 bytes encrypted
/// Poly1305 MAC for static-key section               16 bytes
/// payload section                                   variable encrypted
/// Poly1305 MAC for payload section                  16 bytes
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundNewSessionMessage {
    /// The Elligator2 representative of Alice's fresh ephemeral key.
    pub representative: EciesEphemeralRepresentative,
    /// The encrypted 32-byte Alice static public key plus its
    /// 16-byte MAC (48 bytes).
    pub encrypted_static_section: Vec<u8>,
    /// The encrypted Garlic payload blocks plus their 16-byte MAC.
    pub encrypted_payload_section: Vec<u8>,
}

impl BoundNewSessionMessage {
    /// Validates field shapes.
    pub fn new(
        representative: EciesEphemeralRepresentative,
        encrypted_static_section: Vec<u8>,
        encrypted_payload_section: Vec<u8>,
    ) -> Result<Self, EciesError> {
        if encrypted_static_section.len() != STATIC_PUBLIC_LENGTH + AEAD_TAG_LEN {
            return Err(EciesError::CiphertextTooShort {
                actual: encrypted_static_section.len(),
                minimum: STATIC_PUBLIC_LENGTH + AEAD_TAG_LEN,
            });
        }
        if encrypted_payload_section.len() < AEAD_TAG_LEN {
            return Err(EciesError::CiphertextTooShort {
                actual: encrypted_payload_section.len(),
                minimum: AEAD_TAG_LEN,
            });
        }
        Ok(Self {
            representative,
            encrypted_static_section,
            encrypted_payload_section,
        })
    }

    /// Encodes the message under the supplied hard ceiling.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, EciesError> {
        let total = REPRESENTATIVE_LENGTH
            + self.encrypted_static_section.len()
            + self.encrypted_payload_section.len();
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        let mut output = Vec::with_capacity(total);
        output.extend_from_slice(self.representative.as_bytes());
        output.extend_from_slice(&self.encrypted_static_section);
        output.extend_from_slice(&self.encrypted_payload_section);
        Ok(output)
    }

    /// Decodes a bound New Session message from the complete
    /// encrypted-data region (the enclosing I2NP Garlic envelope
    /// provides framing).
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, EciesError> {
        if input.len() < BOUND_NEW_SESSION_MIN_LENGTH {
            return Err(EciesError::CiphertextTooShort {
                actual: input.len(),
                minimum: BOUND_NEW_SESSION_MIN_LENGTH,
            });
        }
        let mut representative = [0_u8; REPRESENTATIVE_LENGTH];
        representative.copy_from_slice(&input[..REPRESENTATIVE_LENGTH]);
        let static_end = REPRESENTATIVE_LENGTH + STATIC_PUBLIC_LENGTH + AEAD_TAG_LEN;
        let encrypted_static_section = input[REPRESENTATIVE_LENGTH..static_end].to_vec();
        let encrypted_payload_section = input[static_end..].to_vec();
        if input.len() > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: input.len(),
                maximum,
            });
        }
        Self::new(
            EciesEphemeralRepresentative(representative),
            encrypted_static_section,
            encrypted_payload_section,
        )
    }
}

/// The New Session Reply message Bob sends back to Alice.
///
/// ```text
/// session tag                         8 bytes, clear
/// Bob Elligator2 ephemeral key       32 bytes, clear
/// key-section MAC (zero-length data) 16 bytes
/// payload section                    variable encrypted
/// payload-section MAC                16 bytes
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSessionReplyMessage {
    /// The reply session tag from the NSR tag set.
    pub tag: [u8; SESSION_TAG_LENGTH],
    /// The Elligator2 representative of Bob's fresh ephemeral key.
    pub representative: EciesEphemeralRepresentative,
    /// The Poly1305 MAC over the zero-length authenticated key
    /// section.
    pub key_section_mac: [u8; AEAD_TAG_LEN],
    /// The encrypted Garlic payload blocks plus their 16-byte MAC.
    pub encrypted_payload_section: Vec<u8>,
}

impl NewSessionReplyMessage {
    /// Validates field shapes.
    pub fn new(
        tag: [u8; SESSION_TAG_LENGTH],
        representative: EciesEphemeralRepresentative,
        key_section_mac: [u8; AEAD_TAG_LEN],
        encrypted_payload_section: Vec<u8>,
    ) -> Result<Self, EciesError> {
        if encrypted_payload_section.len() < AEAD_TAG_LEN {
            return Err(EciesError::CiphertextTooShort {
                actual: encrypted_payload_section.len(),
                minimum: AEAD_TAG_LEN,
            });
        }
        Ok(Self {
            tag,
            representative,
            key_section_mac,
            encrypted_payload_section,
        })
    }

    /// Encodes the message under the supplied hard ceiling.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, EciesError> {
        let total =
            NEW_SESSION_REPLY_MIN_LENGTH + self.encrypted_payload_section.len() - AEAD_TAG_LEN;
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        let mut output = Vec::with_capacity(total);
        output.extend_from_slice(&self.tag);
        output.extend_from_slice(self.representative.as_bytes());
        output.extend_from_slice(&self.key_section_mac);
        output.extend_from_slice(&self.encrypted_payload_section);
        Ok(output)
    }

    /// Decodes a New Session Reply message from the complete
    /// encrypted-data region.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, EciesError> {
        if input.len() < NEW_SESSION_REPLY_MIN_LENGTH {
            return Err(EciesError::CiphertextTooShort {
                actual: input.len(),
                minimum: NEW_SESSION_REPLY_MIN_LENGTH,
            });
        }
        if input.len() > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: input.len(),
                maximum,
            });
        }
        let mut tag = [0_u8; SESSION_TAG_LENGTH];
        tag.copy_from_slice(&input[..SESSION_TAG_LENGTH]);
        let rep_start = SESSION_TAG_LENGTH;
        let mut representative = [0_u8; REPRESENTATIVE_LENGTH];
        representative.copy_from_slice(&input[rep_start..rep_start + REPRESENTATIVE_LENGTH]);
        let mac_start = rep_start + REPRESENTATIVE_LENGTH;
        let mut key_section_mac = [0_u8; AEAD_TAG_LEN];
        key_section_mac.copy_from_slice(&input[mac_start..mac_start + AEAD_TAG_LEN]);
        let encrypted_payload_section = input[mac_start + AEAD_TAG_LEN..].to_vec();
        Self::new(
            tag,
            EciesEphemeralRepresentative(representative),
            key_section_mac,
            encrypted_payload_section,
        )
    }
}

/// The Existing Session message: tag plus encrypted payload.
///
/// ```text
/// session tag                         8 bytes, clear
/// payload section                    variable encrypted
/// payload-section MAC                16 bytes
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingSessionMessage {
    /// The inbound session tag the receiver consumes.
    pub tag: [u8; SESSION_TAG_LENGTH],
    /// The encrypted Garlic payload blocks plus their 16-byte MAC.
    pub encrypted_payload_section: Vec<u8>,
}

impl ExistingSessionMessage {
    /// Validates field shapes.
    pub fn new(
        tag: [u8; SESSION_TAG_LENGTH],
        encrypted_payload_section: Vec<u8>,
    ) -> Result<Self, EciesError> {
        if encrypted_payload_section.len() < AEAD_TAG_LEN {
            return Err(EciesError::CiphertextTooShort {
                actual: encrypted_payload_section.len(),
                minimum: AEAD_TAG_LEN,
            });
        }
        Ok(Self {
            tag,
            encrypted_payload_section,
        })
    }

    /// Encodes the message under the supplied hard ceiling.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, EciesError> {
        let total = SESSION_TAG_LENGTH + self.encrypted_payload_section.len();
        if total > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: total,
                maximum,
            });
        }
        let mut output = Vec::with_capacity(total);
        output.extend_from_slice(&self.tag);
        output.extend_from_slice(&self.encrypted_payload_section);
        Ok(output)
    }

    /// Decodes an Existing Session message from the complete
    /// encrypted-data region.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, EciesError> {
        if input.len() < EXISTING_SESSION_MIN_LENGTH {
            return Err(EciesError::CiphertextTooShort {
                actual: input.len(),
                minimum: EXISTING_SESSION_MIN_LENGTH,
            });
        }
        if input.len() > maximum {
            return Err(EciesError::CiphertextTooLarge {
                actual: input.len(),
                maximum,
            });
        }
        let mut tag = [0_u8; SESSION_TAG_LENGTH];
        tag.copy_from_slice(&input[..SESSION_TAG_LENGTH]);
        Self::new(tag, input[SESSION_TAG_LENGTH..].to_vec())
    }
}

/// Alice-side context retained after sealing a bound New Session.
///
/// The context carries everything required to authenticate Bob's New
/// Session Reply later: the post-payload chaining key (which roots
/// the NSR reply tag set), the transcript hash (which the NSR
/// handshake continues), Alice's ephemeral secret (for the NSR
/// `ee` transition), and Bob's static public key.
///
/// The type owns secret material: it is not `Clone`, it zeroizes on
/// drop, and `Debug` never reveals bytes.
pub struct BoundNewSessionSender {
    chaining_key: [u8; HKDF_OUTPUT_LEN],
    transcript_hash: [u8; HKDF_OUTPUT_LEN],
    ephemeral_secret: EciesEphemeralSecret,
    bob_static_public: [u8; STATIC_PUBLIC_LENGTH],
}

impl fmt::Debug for BoundNewSessionSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundNewSessionSender")
            .field("bob_static_public", &"<redacted>")
            .finish()
    }
}

impl Drop for BoundNewSessionSender {
    fn drop(&mut self) {
        self.chaining_key.zeroize();
        self.transcript_hash.zeroize();
        self.bob_static_public.zeroize();
    }
}

impl BoundNewSessionSender {
    /// Returns Bob's static public key the session was sealed to.
    pub const fn bob_static_public(&self) -> &[u8; STATIC_PUBLIC_LENGTH] {
        &self.bob_static_public
    }

    /// Builds the receive-side NSR tag set for this pending session:
    /// the tag Bob's New Session Reply will carry.
    pub fn reply_tag_set(&self) -> Result<EciesTagSet, EciesError> {
        let mut tag_set = EciesTagSet::new_session_reply_tag_set(&self.chaining_key)?;
        tag_set.begin_tag_ratchet()?;
        Ok(tag_set)
    }

    /// Borrows the retained handshake pieces for the reply-opening
    /// path.
    pub(crate) fn handshake_parts(&self) -> (&[u8; 32], &[u8; 32], &EciesEphemeralSecret) {
        (
            &self.chaining_key,
            &self.transcript_hash,
            &self.ephemeral_secret,
        )
    }
}

/// Seals a bound New Session message to Bob.
///
/// `alice_static_secret` / `alice_ephemeral` derive from the local
/// destination identity: Alice's published type-4 X25519 key is the
/// key encrypted inside the static-key section, so Bob can match it
/// against her LeaseSet2 before routing replies. The function
/// returns the wire message plus the sender context the caller must
/// retain until the New Session Reply arrives.
///
/// Transcript sequence (exact specification order):
///
/// ```text
/// h = SHA256(protocol_name); ck = h; h = SHA256(h)
/// MixHash(bpk); MixHash(aepk)
/// es: k = HKDF(ck, DH(aesk, bpk), "", 64)[32:63]; n=0; ad=h
///     ENCRYPT(k, apk) ; MixHash(ciphertext || MAC)
/// ss: k = HKDF(ck, DH(ask, bpk), "", 64)[32:63]; n=0; ad=h
///     ENCRYPT(k, payload) ; MixHash(ciphertext || MAC)
/// ```
pub fn seal_bound_new_session(
    alice_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    alice_ephemeral: &EciesEphemeralKeypair,
    bob_static_public: &[u8; STATIC_PUBLIC_LENGTH],
    payload: &[u8],
) -> Result<(BoundNewSessionMessage, BoundNewSessionSender), EciesError> {
    if alice_static_secret.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    if bob_static_public.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    if payload.len() > MAX_NEW_SESSION_CIPHERTEXT {
        return Err(EciesError::CiphertextTooLarge {
            actual: payload.len(),
            maximum: MAX_NEW_SESSION_CIPHERTEXT,
        });
    }

    // The transcript hashes the decoded Montgomery public key, not
    // the representative; deriving it through the same decode path
    // the receiver uses guarantees both sides hash identical bytes.
    let alice_ephemeral_public = decode_representative(&alice_ephemeral.representative())?;

    let mut state = EciesNoiseState::new();
    state.mix_hash(bob_static_public);
    state.mix_hash(&alice_ephemeral_public);

    // es: encrypt Alice's destination static public key.
    let es_shared = diffie_hellman_checked(alice_ephemeral.secret().as_bytes(), bob_static_public)?;
    let static_section_key = state.mix_key(es_shared.as_ref())?;
    let alice_static_public =
        X25519PublicKey::from(&StaticSecret::from(*alice_static_secret)).to_bytes();
    let encrypted_static_section = aead_encrypt(
        &static_section_key,
        0,
        &alice_static_public,
        &state.transcript_hash(),
    )?;
    state.mix_hash(&encrypted_static_section);

    // ss: encrypt the payload.
    let ss_shared = diffie_hellman_checked(alice_static_secret, bob_static_public)?;
    let payload_key = state.mix_key(ss_shared.as_ref())?;
    let encrypted_payload_section =
        aead_encrypt(&payload_key, 0, payload, &state.transcript_hash())?;
    state.mix_hash(&encrypted_payload_section);

    let message = BoundNewSessionMessage::new(
        alice_ephemeral.representative(),
        encrypted_static_section,
        encrypted_payload_section,
    )?;
    let sender = BoundNewSessionSender {
        chaining_key: state.chaining_key(),
        transcript_hash: state.transcript_hash(),
        ephemeral_secret: EciesEphemeralSecret::from_bytes(*alice_ephemeral.secret().as_bytes()),
        bob_static_public: *bob_static_public,
    };
    Ok((message, sender))
}

/// Bob-side responder context retained after opening a bound New
/// Session. It carries the post-payload chaining key and transcript
/// hash (the NSR handshake continues both), Alice's decoded
/// ephemeral public key (the NSR `ee` transition), and the
/// authenticated Alice static public key.
pub struct NewSessionResponder {
    chaining_key: [u8; HKDF_OUTPUT_LEN],
    transcript_hash: [u8; HKDF_OUTPUT_LEN],
    pub(crate) alice_ephemeral_public: [u8; REPRESENTATIVE_LENGTH],
    /// The authenticated Alice static X25519 public key recovered
    /// from the decrypted static-key section.
    pub alice_static_public: [u8; STATIC_PUBLIC_LENGTH],
}

impl fmt::Debug for NewSessionResponder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewSessionResponder")
            .finish_non_exhaustive()
    }
}

impl Drop for NewSessionResponder {
    fn drop(&mut self) {
        self.chaining_key.zeroize();
        self.transcript_hash.zeroize();
        self.alice_ephemeral_public.zeroize();
        self.alice_static_public.zeroize();
    }
}

/// A successful bound New Session open.
pub struct OpenedBoundNewSession {
    /// The decrypted Garlic payload blocks.
    pub payload: Vec<u8>,
    /// The responder context required to emit the New Session Reply.
    pub responder: NewSessionResponder,
}

impl fmt::Debug for OpenedBoundNewSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenedBoundNewSession")
            .field("payload_len", &self.payload.len())
            .field("responder", &self.responder)
            .finish()
    }
}

/// Opens a bound New Session message addressed to Bob.
///
/// On success Bob learns the authenticated Alice static X25519
/// public key from the decrypted static-key section; the full
/// Destination binding happens in the routing layer by matching that
/// key against Alice's validated LeaseSet2. An all-zero decrypted
/// static section (the unbound form) is rejected with
/// [`EciesError::UnboundNewSessionNotSupported`].
pub fn open_bound_new_session(
    bob_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    bob_static_public: &[u8; STATIC_PUBLIC_LENGTH],
    message: &BoundNewSessionMessage,
) -> Result<OpenedBoundNewSession, EciesError> {
    if bob_static_secret.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    if bob_static_public.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    let alice_ephemeral_public = decode_representative(&message.representative)?;

    let mut state = EciesNoiseState::new();
    state.mix_hash(bob_static_public);
    state.mix_hash(&alice_ephemeral_public);

    // es: decrypt the static-key section.
    let es_shared = diffie_hellman_checked(bob_static_secret, &alice_ephemeral_public)?;
    let static_section_key = state.mix_key(es_shared.as_ref())?;
    let alice_static_plain = aead_decrypt(
        &static_section_key,
        0,
        &message.encrypted_static_section,
        &state.transcript_hash(),
    )?;
    state.mix_hash(&message.encrypted_static_section);

    if alice_static_plain.iter().all(|byte| *byte == 0) {
        return Err(EciesError::UnboundNewSessionNotSupported);
    }
    let mut alice_static_public = [0_u8; STATIC_PUBLIC_LENGTH];
    alice_static_public.copy_from_slice(&alice_static_plain);

    // ss: decrypt the payload.
    let ss_shared = diffie_hellman_checked(bob_static_secret, &alice_static_public)?;
    let payload_key = state.mix_key(ss_shared.as_ref())?;
    let payload = aead_decrypt(
        &payload_key,
        0,
        &message.encrypted_payload_section,
        &state.transcript_hash(),
    )?;
    state.mix_hash(&message.encrypted_payload_section);

    Ok(OpenedBoundNewSession {
        payload,
        responder: NewSessionResponder {
            chaining_key: state.chaining_key(),
            transcript_hash: state.transcript_hash(),
            alice_ephemeral_public,
            alice_static_public,
        },
    })
}

/// The complete result of sealing a New Session Reply: the wire
/// message plus both post-split directions ready for Existing
/// Session traffic.
pub struct SealedNewSessionReply {
    /// The encoded reply message.
    pub message: NewSessionReplyMessage,
    /// The local outbound direction (Bob to Alice): ready to send.
    pub outbound_tag_set: EciesTagSet,
    /// The peer's direction toward the local side (Alice to Bob):
    /// the root the caller uses for its inbound receive window.
    pub inbound_tag_set: EciesTagSet,
}

impl fmt::Debug for SealedNewSessionReply {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedNewSessionReply")
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

/// Seals the New Session Reply from Bob's retained responder
/// context.
///
/// Sequence (exact specification order, continuing the New Session
/// transcript):
///
/// ```text
/// tag = tagset_nsr.GET_NEXT_ENTRY()
/// MixHash(tag); MixHash(bepk)
/// ee: k = HKDF(ck, DH(besk, aepk), "", 64)[32:63]
/// se: k = HKDF(ck, DH(besk, apk), "", 64)[32:63]; n=0; ad=h
///     ENCRYPT(k, ZEROLEN) -> 16-byte key-section MAC ; MixHash(MAC)
/// split(): k_ab/k_ba = HKDF(ck, ZEROLEN, "", 64)
/// tagset_ab = DH_INITIALIZE(ck, k_ab)   // Alice -> Bob
/// tagset_ba = DH_INITIALIZE(ck, k_ba)   // Bob -> Alice
/// k_payload = HKDF(k_ba, ZEROLEN, "AttachPayloadKDF", 32); n=0; ad=h
///     ENCRYPT(k_payload, payload)
/// ```
pub fn seal_new_session_reply<R: TryCryptoRng + ?Sized>(
    responder: &NewSessionResponder,
    bob_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    payload: &[u8],
    rng: &mut R,
) -> Result<SealedNewSessionReply, EciesError> {
    if bob_static_secret.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    if payload.len() > MAX_NEW_SESSION_CIPHERTEXT {
        return Err(EciesError::CiphertextTooLarge {
            actual: payload.len(),
            maximum: MAX_NEW_SESSION_CIPHERTEXT,
        });
    }

    // Reply tag from the NSR tag set rooted in the New Session
    // chaining key.
    let mut nsr_tag_set = EciesTagSet::new_session_reply_tag_set(&responder.chaining_key)?;
    nsr_tag_set.begin_tag_ratchet()?;
    let reply_entry = nsr_tag_set.next_entry()?;

    let bob_ephemeral = EciesEphemeralKeypair::generate(rng)?;
    let bob_ephemeral_public = decode_representative(&bob_ephemeral.representative())?;

    let mut state = EciesNoiseState::new();
    state.set_from(&responder.chaining_key, &responder.transcript_hash);
    state.mix_hash(&reply_entry.tag);
    state.mix_hash(&bob_ephemeral_public);

    // ee
    let ee_shared = diffie_hellman_checked(
        bob_ephemeral.secret().as_bytes(),
        &responder.alice_ephemeral_public,
    )?;
    state.mix_key(ee_shared.as_ref())?;
    // se
    let se_shared = diffie_hellman_checked(
        bob_ephemeral.secret().as_bytes(),
        &responder.alice_static_public,
    )?;
    let key_section_key = state.mix_key(se_shared.as_ref())?;

    // Zero-length authenticated key section.
    let key_section_mac = aead_encrypt(&key_section_key, 0, b"", &state.transcript_hash())?;
    let mut key_section_mac_array = [0_u8; AEAD_TAG_LEN];
    key_section_mac_array.copy_from_slice(&key_section_mac);
    state.mix_hash(&key_section_mac);

    // Noise Split into the two directional ES tag sets.
    let split = crate::hkdf_sha256_extract_and_expand(&state.chaining_key(), b"", b"", 64)
        .map_err(EciesError::Hkdf)?;
    let mut k_ab = [0_u8; HKDF_OUTPUT_LEN];
    k_ab.copy_from_slice(&split[..HKDF_OUTPUT_LEN]);
    let mut k_ba = [0_u8; HKDF_OUTPUT_LEN];
    k_ba.copy_from_slice(&split[HKDF_OUTPUT_LEN..]);

    let mut outbound_tag_set = EciesTagSet::dh_initialize(&state.chaining_key(), &k_ba)?;
    outbound_tag_set.begin_tag_ratchet()?;
    let mut inbound_tag_set = EciesTagSet::dh_initialize(&state.chaining_key(), &k_ab)?;
    inbound_tag_set.begin_tag_ratchet()?;

    // NSR payload key.
    let attach =
        crate::hkdf_sha256_extract_and_expand(&k_ba, b"", b"AttachPayloadKDF", HKDF_OUTPUT_LEN)
            .map_err(EciesError::Hkdf)?;
    let mut payload_key = [0_u8; HKDF_OUTPUT_LEN];
    payload_key.copy_from_slice(&attach);
    let encrypted_payload_section =
        aead_encrypt(&payload_key, 0, payload, &state.transcript_hash())?;
    payload_key.zeroize();

    let message = NewSessionReplyMessage::new(
        reply_entry.tag,
        bob_ephemeral.representative(),
        key_section_mac_array,
        encrypted_payload_section,
    )?;
    Ok(SealedNewSessionReply {
        message,
        outbound_tag_set,
        inbound_tag_set,
    })
}

/// The complete result of opening a New Session Reply: the decrypted
/// payload plus both post-split directions.
pub struct OpenedNewSessionReply {
    /// The decrypted Garlic payload blocks.
    pub payload: Vec<u8>,
    /// The local outbound direction (Alice to Bob): ready to send.
    pub outbound_tag_set: EciesTagSet,
    /// The peer's direction toward the local side (Bob to Alice):
    /// the root the caller uses for its inbound receive window.
    pub inbound_tag_set: EciesTagSet,
}

/// Opens a New Session Reply against Alice's retained sender
/// context.
///
/// The reply tag participates in the transcript, so a wrong or
/// replayed tag fails authentication without leaking plaintext. On
/// success the caller receives both directional Existing Session tag
/// sets exactly as Bob derived them.
pub fn open_new_session_reply(
    sender: &BoundNewSessionSender,
    alice_static_secret: &[u8; STATIC_PUBLIC_LENGTH],
    reply: &NewSessionReplyMessage,
) -> Result<OpenedNewSessionReply, EciesError> {
    if alice_static_secret.iter().all(|byte| *byte == 0) {
        return Err(EciesError::AllZeroKey);
    }
    let (chaining_key, transcript_hash, ephemeral_secret) = sender.handshake_parts();

    let bob_ephemeral_public = decode_representative(&reply.representative)?;

    let mut state = EciesNoiseState::new();
    state.set_from(chaining_key, transcript_hash);
    state.mix_hash(&reply.tag);
    state.mix_hash(&bob_ephemeral_public);

    // ee
    let ee_shared = diffie_hellman_checked(ephemeral_secret.as_bytes(), &bob_ephemeral_public)?;
    state.mix_key(ee_shared.as_ref())?;
    // se
    let se_shared = diffie_hellman_checked(alice_static_secret, &bob_ephemeral_public)?;
    let key_section_key = state.mix_key(se_shared.as_ref())?;

    // Verify the zero-length authenticated key section.
    let verified = aead_decrypt(
        &key_section_key,
        0,
        &reply.key_section_mac,
        &state.transcript_hash(),
    )?;
    if !verified.is_empty() {
        return Err(EciesError::AuthenticationFailed);
    }
    state.mix_hash(&reply.key_section_mac);

    // Noise Split into the two directional ES tag sets.
    let split = crate::hkdf_sha256_extract_and_expand(&state.chaining_key(), b"", b"", 64)
        .map_err(EciesError::Hkdf)?;
    let mut k_ab = [0_u8; HKDF_OUTPUT_LEN];
    k_ab.copy_from_slice(&split[..HKDF_OUTPUT_LEN]);
    let mut k_ba = [0_u8; HKDF_OUTPUT_LEN];
    k_ba.copy_from_slice(&split[HKDF_OUTPUT_LEN..]);

    let mut outbound_tag_set = EciesTagSet::dh_initialize(&state.chaining_key(), &k_ab)?;
    outbound_tag_set.begin_tag_ratchet()?;
    let mut inbound_tag_set = EciesTagSet::dh_initialize(&state.chaining_key(), &k_ba)?;
    inbound_tag_set.begin_tag_ratchet()?;

    // NSR payload key.
    let attach =
        crate::hkdf_sha256_extract_and_expand(&k_ba, b"", b"AttachPayloadKDF", HKDF_OUTPUT_LEN)
            .map_err(EciesError::Hkdf)?;
    let mut payload_key = [0_u8; HKDF_OUTPUT_LEN];
    payload_key.copy_from_slice(&attach);
    let payload = aead_decrypt(
        &payload_key,
        0,
        &reply.encrypted_payload_section,
        &state.transcript_hash(),
    )?;
    payload_key.zeroize();

    Ok(OpenedNewSessionReply {
        payload,
        outbound_tag_set,
        inbound_tag_set,
    })
}

/// Encrypts one Existing Session message on the supplied direction.
///
/// The Nth call issues the Nth tag-set entry and encrypts with the
/// matching symmetric key and message-number nonce; the 8-byte tag
/// is the AEAD associated data.
pub fn seal_existing_session(
    tag_set: &mut EciesTagSet,
    payload: &[u8],
) -> Result<ExistingSessionMessage, EciesError> {
    if payload.len() > MAX_NEW_SESSION_CIPHERTEXT {
        return Err(EciesError::CiphertextTooLarge {
            actual: payload.len(),
            maximum: MAX_NEW_SESSION_CIPHERTEXT,
        });
    }
    let entry = tag_set.next_entry()?;
    let key = tag_set.symm_key(entry.index)?;
    let encrypted_payload_section = aead_encrypt(key, u64::from(entry.index), payload, &entry.tag)?;
    Ok(ExistingSessionMessage {
        tag: entry.tag,
        encrypted_payload_section,
    })
}

/// Decrypts one Existing Session message against the supplied
/// direction at the tag-window index the caller resolved.
///
/// The caller (session manager) owns tag lookup and replay
/// suppression; this function re-verifies the tag binding through
/// the AEAD associated data.
pub fn open_existing_session(
    tag_set: &mut EciesTagSet,
    index: u32,
    message: &ExistingSessionMessage,
) -> Result<Vec<u8>, EciesError> {
    if message.encrypted_payload_section.len() < AEAD_TAG_LEN {
        return Err(EciesError::CiphertextTooShort {
            actual: message.encrypted_payload_section.len(),
            minimum: AEAD_TAG_LEN,
        });
    }
    let key_copy: [u8; HKDF_OUTPUT_LEN] = *tag_set.symm_key(index)?;
    let payload = aead_decrypt(
        &key_copy,
        u64::from(index),
        &message.encrypted_payload_section,
        &message.tag,
    )?;
    tag_set.trim_keys_below(index.saturating_sub(1));
    Ok(payload)
}

/// Frozen intermediate-value conformance vectors for the Plan 126
/// destination ratchet (Plan 126 §11.2).
///
/// The constants were generated once by an independent Python
/// reference implementation of the normative specification
/// (`cryptography`-based, no shared code with this module) and are
/// never recomputed. The production primitives must reproduce them
/// byte-for-byte or the conformance tests fail. Provenance is
/// recorded in `specs/references/ecies-destination-ratchet.md`.
#[cfg(test)]
pub(crate) mod fixed_vectors {
    use super::{REPRESENTATIVE_LENGTH, SESSION_TAG_LENGTH, STATIC_PUBLIC_LENGTH};

    /// Frozen private seeds. The ephemeral seeds are canonical
    /// Elligator2-representable counter patterns.
    pub const ALICE_STATIC_SECRET: [u8; STATIC_PUBLIC_LENGTH] = [0x11_u8; STATIC_PUBLIC_LENGTH];
    pub const BOB_STATIC_SECRET: [u8; STATIC_PUBLIC_LENGTH] = [0x22_u8; STATIC_PUBLIC_LENGTH];
    pub const ALICE_EPHEMERAL_SEED: [u8; REPRESENTATIVE_LENGTH] = [0x18_u8; REPRESENTATIVE_LENGTH];
    pub const BOB_EPHEMERAL_SEED: [u8; REPRESENTATIVE_LENGTH] = [0x1b_u8; REPRESENTATIVE_LENGTH];

    pub const PAYLOAD_NS: &[u8] = &[
        0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
        0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e,
        0x5f, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    ];
    pub const PAYLOAD_NSR: &[u8] = &[0x5a_u8; 24];

    pub const PROTOCOL_NAME_HASH: [u8; 32] = [
        0x4c, 0xaf, 0x11, 0xef, 0x2c, 0x8e, 0x36, 0x56, 0x4c, 0x53, 0xe8, 0x88, 0x85, 0x06, 0x4d,
        0xba, 0xac, 0xbe, 0x00, 0x54, 0xad, 0x17, 0x8f, 0x80, 0x79, 0xa6, 0x46, 0x82, 0x7e, 0x6e,
        0xe4, 0x0c,
    ];

    pub const NULL_PROLOGUE_HASH: [u8; 32] = [
        0x9c, 0xcf, 0x85, 0x2c, 0xc9, 0x3b, 0xb9, 0x50, 0x44, 0x41, 0xe9, 0x50, 0xe0, 0x1d, 0x52,
        0x32, 0x2e, 0x0d, 0x47, 0xad, 0xd1, 0xe9, 0xa5, 0x55, 0xf7, 0x55, 0xb5, 0x69, 0xae, 0x18,
        0x3b, 0x5c,
    ];

    pub const NS_H_AFTER_BPK: [u8; 32] = [
        0x4a, 0x44, 0x36, 0x99, 0x67, 0x67, 0xad, 0xda, 0x65, 0x6b, 0x24, 0xc0, 0x3c, 0x69, 0xf7,
        0x5e, 0x0e, 0x2b, 0x7a, 0xc6, 0x75, 0x70, 0x45, 0x7b, 0xd4, 0x05, 0xf4, 0xdf, 0x49, 0xc0,
        0x2c, 0x20,
    ];

    pub const NS_H_AFTER_AEPK: [u8; 32] = [
        0x76, 0x9a, 0xeb, 0xfd, 0x95, 0x51, 0x35, 0x1b, 0x47, 0x55, 0xc4, 0xcb, 0x1c, 0x2c, 0x3d,
        0x4b, 0x6d, 0xdf, 0xd7, 0x26, 0xf8, 0xc2, 0x72, 0x0e, 0xcf, 0xe3, 0x04, 0x58, 0x14, 0xb0,
        0xb3, 0xa4,
    ];

    pub const NS_ES_CHAIN_KEY: [u8; 32] = [
        0x9d, 0x64, 0x01, 0x2d, 0x73, 0xec, 0x28, 0xfc, 0xb0, 0xbb, 0x94, 0xad, 0xae, 0x5a, 0x9a,
        0x29, 0xfc, 0x5f, 0x88, 0x13, 0x74, 0x12, 0x15, 0xa9, 0xef, 0x6b, 0xf9, 0xa0, 0xff, 0xce,
        0xbb, 0xfd,
    ];

    pub const NS_ES_PAYLOAD_KEY: [u8; 32] = [
        0x96, 0x0c, 0x1d, 0x08, 0x08, 0xb7, 0x05, 0x31, 0xc6, 0x03, 0xdb, 0xa5, 0x33, 0x03, 0x70,
        0x28, 0xe1, 0xab, 0x26, 0xe3, 0xe5, 0xf1, 0x3f, 0xc5, 0x8e, 0x46, 0x72, 0xd3, 0x3c, 0xf5,
        0xc8, 0x8a,
    ];

    pub const NS_STATIC_SECTION: [u8; 48] = [
        0x25, 0x89, 0x23, 0xb1, 0xca, 0xe7, 0xad, 0x52, 0x70, 0xe0, 0xae, 0x8d, 0x44, 0x25, 0x08,
        0xf1, 0x5b, 0x65, 0x82, 0xcc, 0x6a, 0x71, 0xea, 0x6a, 0x44, 0x7b, 0xab, 0xd5, 0xe1, 0x0f,
        0x15, 0x59, 0xd2, 0xfc, 0xca, 0xee, 0x49, 0x55, 0x8e, 0xe8, 0x9a, 0x72, 0x6d, 0xba, 0x55,
        0x29, 0x50, 0xd0,
    ];

    pub const NS_H_AFTER_STATIC: [u8; 32] = [
        0x71, 0xa0, 0x18, 0xe3, 0x9a, 0x04, 0x02, 0x7e, 0xf2, 0x02, 0xb1, 0xf0, 0x5c, 0x94, 0x68,
        0x4d, 0x8a, 0x3a, 0x17, 0x83, 0x63, 0x30, 0xa9, 0x48, 0xa0, 0xf9, 0x67, 0x16, 0x74, 0x3f,
        0x89, 0x75,
    ];

    pub const NS_SS_CHAIN_KEY: [u8; 32] = [
        0x91, 0xd2, 0x6e, 0x2b, 0xf8, 0x1d, 0xea, 0x49, 0x99, 0xf7, 0xf9, 0x06, 0xdb, 0x64, 0xbf,
        0xfc, 0x78, 0xb6, 0x28, 0x26, 0xbf, 0x1c, 0x93, 0x80, 0xde, 0x22, 0x88, 0xeb, 0x4e, 0x9b,
        0x83, 0x92,
    ];

    pub const NS_SS_PAYLOAD_KEY: [u8; 32] = [
        0x27, 0x8b, 0x8e, 0x75, 0xc1, 0x4e, 0xef, 0x65, 0xc8, 0x19, 0x4f, 0xca, 0x35, 0xc1, 0x9f,
        0xc4, 0xc1, 0xca, 0x37, 0x29, 0x3b, 0x52, 0x17, 0xa5, 0xcd, 0x36, 0x98, 0x5a, 0x04, 0xe6,
        0x18, 0x83,
    ];

    pub const NS_PAYLOAD_SECTION: &[u8] = &[
        0xb7, 0x81, 0x44, 0x7e, 0x6f, 0x2e, 0x12, 0x27, 0x24, 0x94, 0x38, 0x3f, 0x79, 0xc1, 0xec,
        0x34, 0x6f, 0xcc, 0x79, 0xe0, 0xd8, 0x80, 0x29, 0x7f, 0xe1, 0x61, 0x64, 0xc8, 0xfb, 0x35,
        0xbc, 0x6e, 0x71, 0x8b, 0xfe, 0x56, 0x10, 0x46, 0x8b, 0x03, 0x7e, 0xac, 0xf0, 0xc0, 0xf2,
        0x14, 0x35, 0x91, 0x7e, 0x4d, 0x0b, 0x39, 0x91, 0xd1, 0xa4, 0xba,
    ];

    pub const NS_H_FINAL: [u8; 32] = [
        0xc2, 0x57, 0xd6, 0xdb, 0x7d, 0xbd, 0x67, 0x89, 0x0c, 0xa8, 0x1d, 0x54, 0x75, 0x04, 0x94,
        0xd2, 0x93, 0x2d, 0x8c, 0xb4, 0xab, 0xf2, 0x75, 0x64, 0x2e, 0x86, 0x26, 0xca, 0x55, 0xee,
        0xe5, 0x5f,
    ];

    pub const NSR_TAGSET_KEY: [u8; 32] = [
        0x88, 0x8b, 0xb3, 0x2e, 0x9c, 0xbe, 0xc4, 0x4c, 0x5b, 0xcc, 0x03, 0x50, 0xad, 0xe8, 0x87,
        0xe3, 0x39, 0xba, 0x49, 0x6f, 0x04, 0x80, 0xd3, 0xd4, 0x9a, 0xe5, 0x53, 0x68, 0x29, 0x7c,
        0xd8, 0x84,
    ];

    pub const NSR_TAG_1: [u8; SESSION_TAG_LENGTH] =
        [0xf6, 0x0c, 0x89, 0x8d, 0x90, 0x6a, 0xad, 0x88];

    pub const NSR_H_AFTER_TAG: [u8; 32] = [
        0x53, 0xd6, 0x90, 0x60, 0xc3, 0x22, 0x2b, 0xc4, 0x29, 0x26, 0x44, 0x07, 0x98, 0x5b, 0x13,
        0x6a, 0x3c, 0xe6, 0xc1, 0x10, 0x7f, 0x45, 0xfd, 0xf3, 0x73, 0x88, 0xa1, 0x24, 0xe6, 0x93,
        0x6e, 0xc8,
    ];

    pub const NSR_H_AFTER_BEPK: [u8; 32] = [
        0xe0, 0x39, 0xd2, 0x66, 0x1b, 0xea, 0x47, 0x28, 0xfe, 0x2d, 0x7d, 0xb5, 0xfe, 0x06, 0x5b,
        0x4a, 0x79, 0xab, 0x1a, 0x8a, 0x7a, 0x8a, 0xa4, 0x1b, 0x53, 0x2f, 0x53, 0x09, 0x0b, 0x20,
        0x4d, 0xc3,
    ];

    pub const NSR_EE_CHAIN_KEY: [u8; 32] = [
        0x2e, 0x85, 0x86, 0x86, 0x25, 0xde, 0x82, 0xc9, 0xc1, 0x52, 0x75, 0x21, 0x30, 0xf8, 0x26,
        0x61, 0x3a, 0x7f, 0xb7, 0x38, 0x57, 0xeb, 0x54, 0x5d, 0x28, 0x2d, 0xd6, 0x44, 0x3f, 0xe8,
        0x70, 0x11,
    ];

    pub const NSR_SE_CHAIN_KEY: [u8; 32] = [
        0x9e, 0x8e, 0x77, 0x41, 0x7a, 0xa4, 0x3b, 0x44, 0x5d, 0x46, 0x1c, 0x25, 0x53, 0x4e, 0xb2,
        0xca, 0x1c, 0xb4, 0xa5, 0x1c, 0xab, 0x99, 0x9c, 0x63, 0x82, 0xb6, 0x95, 0xe7, 0xfd, 0xfb,
        0xde, 0x7a,
    ];

    pub const NSR_KEY_SECTION_MAC: [u8; 16] = [
        0xbb, 0xc6, 0xba, 0xb8, 0x73, 0xdd, 0xd8, 0xcb, 0xc1, 0x2e, 0x46, 0x37, 0x7e, 0x34, 0x7f,
        0xa9,
    ];

    pub const SPLIT_K_AB: [u8; 32] = [
        0xa8, 0xcc, 0x3a, 0x43, 0xc7, 0xcf, 0x15, 0xd0, 0x60, 0xec, 0x0b, 0x58, 0x4f, 0x34, 0x0c,
        0x33, 0xe6, 0x55, 0x4c, 0xdb, 0x6f, 0xe8, 0x1c, 0x4d, 0x58, 0xac, 0x99, 0x33, 0xfe, 0xc1,
        0xe2, 0x56,
    ];

    pub const SPLIT_K_BA: [u8; 32] = [
        0xbb, 0xdd, 0x67, 0xde, 0xc7, 0xa6, 0x63, 0xac, 0x21, 0x97, 0xb3, 0x90, 0x3b, 0x9a, 0xb0,
        0xb4, 0x6a, 0x3b, 0x56, 0x3a, 0xf2, 0x1a, 0xae, 0x6d, 0xfc, 0x07, 0x37, 0xde, 0x22, 0x83,
        0x2f, 0x24,
    ];

    pub const NSR_ATTACH_PAYLOAD_KEY: [u8; 32] = [
        0x81, 0x93, 0xbd, 0x38, 0x16, 0xe6, 0xf7, 0x8a, 0x30, 0xf6, 0x92, 0xef, 0xce, 0xa6, 0x5c,
        0xf6, 0xcd, 0x14, 0xa9, 0xcf, 0x97, 0x7e, 0xb2, 0x82, 0xf5, 0xc5, 0x23, 0x6d, 0xd0, 0xfa,
        0x98, 0xbc,
    ];

    pub const NSR_PAYLOAD_SECTION: &[u8] = &[
        0x5d, 0xe0, 0x5f, 0xa0, 0x75, 0x6d, 0x02, 0xec, 0x45, 0x32, 0x2c, 0xe5, 0xde, 0x56, 0x87,
        0xc5, 0xd7, 0xff, 0xaf, 0x7f, 0x2b, 0x44, 0x7c, 0x42, 0x53, 0xae, 0xef, 0xe3, 0x70, 0x0f,
        0xe6, 0xbc, 0xdc, 0x94, 0x40, 0x9d, 0xb8, 0x62, 0x18, 0xbd,
    ];

    pub const ES_TAG_AB_1: [u8; SESSION_TAG_LENGTH] =
        [0x30, 0x50, 0xbc, 0x0c, 0x34, 0xbc, 0x21, 0x20];
    pub const ES_TAG_AB_2: [u8; SESSION_TAG_LENGTH] =
        [0x6b, 0xff, 0xe2, 0x89, 0xf7, 0x08, 0xde, 0xf4];
    pub const ES_KEY_AB_0: [u8; 32] = [
        0xd4, 0xab, 0xec, 0xf1, 0x6f, 0x09, 0xb8, 0xec, 0x5a, 0x8c, 0x7d, 0xd9, 0x43, 0x55, 0x52,
        0x6e, 0x56, 0xb1, 0x23, 0xf0, 0x7d, 0xbe, 0xaf, 0x20, 0x40, 0x66, 0xbc, 0x2b, 0x88, 0xab,
        0x44, 0xb9,
    ];

    pub const ES_KEY_AB_1: [u8; 32] = [
        0x33, 0xe7, 0x8d, 0x3b, 0x3e, 0x26, 0x4e, 0xc7, 0x58, 0x60, 0xe3, 0xf6, 0x33, 0xbe, 0xcb,
        0x63, 0x72, 0x22, 0xdb, 0xa0, 0xfd, 0xc5, 0x6b, 0xff, 0x16, 0x99, 0xb6, 0x67, 0x33, 0xc7,
        0x32, 0xbc,
    ];

    pub const ES_TAG_BA_1: [u8; SESSION_TAG_LENGTH] =
        [0x07, 0x45, 0xc1, 0x83, 0x6f, 0x39, 0x1b, 0x77];
    pub const ES_KEY_BA_0: [u8; 32] = [
        0x9f, 0xe1, 0x81, 0x15, 0x49, 0xbd, 0xf9, 0x0e, 0x24, 0x76, 0xc1, 0xd3, 0x1d, 0x25, 0x05,
        0x09, 0xd1, 0x5d, 0x20, 0x03, 0x66, 0x92, 0xfe, 0x54, 0x27, 0xd6, 0x33, 0x81, 0x74, 0xbf,
        0xd9, 0xc3,
    ];

    pub const ES_AB_MSG1_CIPHERTEXT: &[u8] = &[
        0x83, 0x80, 0xf1, 0x68, 0x96, 0x22, 0x5e, 0x76, 0x40, 0xbe, 0xd9, 0x7f, 0xd9, 0xd9, 0xfc,
        0xff, 0x8c, 0xb7, 0xb0, 0x10, 0x8c, 0x37, 0x2b, 0x8f, 0xc7, 0xb2, 0x2a, 0x86, 0x0a, 0xcd,
        0x49, 0x90,
    ];

    pub const ES_AB_MSG2_CIPHERTEXT: &[u8] = &[
        0x23, 0x32, 0x73, 0x99, 0x30, 0x52, 0x32, 0x7b, 0xc2, 0xf4, 0x20, 0xc0, 0x86, 0x8e, 0x1d,
        0x3b, 0x14, 0xa8, 0xd1, 0xb9, 0xd9, 0x68, 0x92, 0xa1, 0x34, 0x38, 0x8b, 0xf2, 0x6d, 0x3f,
        0x43, 0x6d,
    ];
}

#[cfg(test)]
mod tests {
    use super::fixed_vectors as fv;
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};
    use x25519_dalek::PublicKey as X25519PublicKey;

    fn vector_rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(0x126)
    }

    fn frozen_alice_ephemeral() -> EciesEphemeralKeypair {
        EciesEphemeralKeypair::from_seed_bytes(fv::ALICE_EPHEMERAL_SEED).expect("alice eph")
    }

    /// Derives the X25519 public key for a private seed (RFC 7748
    /// clamping included), matching the Python reference's
    /// `pub_of` helper.
    fn static_public(seed: &[u8; STATIC_PUBLIC_LENGTH]) -> [u8; STATIC_PUBLIC_LENGTH] {
        X25519PublicKey::from(&StaticSecret::from(*seed)).to_bytes()
    }

    #[test]
    fn protocol_name_hash_matches_independent_reference() {
        assert_eq!(sha256(ECIES_NOISE_PROTOCOL_NAME), fv::PROTOCOL_NAME_HASH);
        assert_eq!(ECIES_NOISE_PROTOCOL_NAME.len(), 40);
    }

    #[test]
    fn null_prologue_hash_matches_independent_reference() {
        let state = EciesNoiseState::new();
        assert_eq!(state.chaining_key(), fv::PROTOCOL_NAME_HASH);
        assert_eq!(state.transcript_hash(), fv::NULL_PROLOGUE_HASH);
    }

    #[test]
    fn bound_new_session_seal_matches_independent_reference() {
        let alice_ephemeral = frozen_alice_ephemeral();
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let (message, sender) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            fv::PAYLOAD_NS,
        )
        .expect("seal ns");

        assert_eq!(
            message.encrypted_static_section,
            fv::NS_STATIC_SECTION.to_vec()
        );
        assert_eq!(
            message.encrypted_payload_section,
            fv::NS_PAYLOAD_SECTION.to_vec()
        );

        // The retained chaining key must root the NSR reply tag set
        // exactly as the independent reference derives it.
        let mut reply_tags = sender.reply_tag_set().expect("reply tag set");
        let entry = reply_tags.next_entry().expect("first nsr tag");
        assert_eq!(entry.tag, fv::NSR_TAG_1);
        assert_eq!(entry.index, 0);
        assert_eq!(sender.bob_static_public(), &bob_public);
    }

    #[test]
    fn new_session_reply_and_split_match_independent_reference() {
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let (ns_message, _sender) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &frozen_alice_ephemeral(),
            &bob_public,
            fv::PAYLOAD_NS,
        )
        .expect("seal ns");

        let opened = open_bound_new_session(&fv::BOB_STATIC_SECRET, &bob_public, &ns_message)
            .expect("open ns");
        assert_eq!(opened.payload, fv::PAYLOAD_NS);
        let expected_alice_public =
            X25519PublicKey::from(&StaticSecret::from(fv::ALICE_STATIC_SECRET)).to_bytes();
        assert_eq!(opened.responder.alice_static_public, expected_alice_public);

        // Drive Bob's reply with the frozen NSR ephemeral through the
        // production helpers so every intermediate is exercised.
        let bob_ephemeral =
            EciesEphemeralKeypair::from_seed_bytes(fv::BOB_EPHEMERAL_SEED).expect("bob eph");
        let bepk = decode_representative(&bob_ephemeral.representative()).expect("decode");

        let mut nsr_tag_set =
            EciesTagSet::new_session_reply_tag_set(&opened.responder.chaining_key)
                .expect("nsr tagset");
        nsr_tag_set.begin_tag_ratchet().expect("ratchet");
        let reply_entry = nsr_tag_set.next_entry().expect("reply tag");
        assert_eq!(reply_entry.tag, fv::NSR_TAG_1);

        let mut state = EciesNoiseState::new();
        state.set_from(
            &opened.responder.chaining_key,
            &opened.responder.transcript_hash,
        );
        state.mix_hash(&reply_entry.tag);
        assert_eq!(state.transcript_hash(), fv::NSR_H_AFTER_TAG);
        state.mix_hash(&bepk);
        assert_eq!(state.transcript_hash(), fv::NSR_H_AFTER_BEPK);

        let ee_shared = diffie_hellman_checked(
            bob_ephemeral.secret().as_bytes(),
            &opened.responder.alice_ephemeral_public,
        )
        .expect("ee");
        state.mix_key(ee_shared.as_ref()).expect("mix ee");
        assert_eq!(state.chaining_key(), fv::NSR_EE_CHAIN_KEY);

        let se_shared = diffie_hellman_checked(
            bob_ephemeral.secret().as_bytes(),
            &opened.responder.alice_static_public,
        )
        .expect("se");
        let key_section_key = state.mix_key(se_shared.as_ref()).expect("mix se");
        assert_eq!(state.chaining_key(), fv::NSR_SE_CHAIN_KEY);

        let key_section_mac =
            aead_encrypt(&key_section_key, 0, b"", &state.transcript_hash()).expect("mac");
        assert_eq!(key_section_mac, fv::NSR_KEY_SECTION_MAC.to_vec());
        state.mix_hash(&key_section_mac);

        let split = crate::hkdf_sha256_extract_and_expand(&state.chaining_key(), b"", b"", 64)
            .expect("split");
        assert_eq!(&split[..HKDF_OUTPUT_LEN], &fv::SPLIT_K_AB[..]);
        assert_eq!(&split[HKDF_OUTPUT_LEN..], &fv::SPLIT_K_BA[..]);

        let attach = crate::hkdf_sha256_extract_and_expand(
            &split[HKDF_OUTPUT_LEN..],
            b"",
            b"AttachPayloadKDF",
            HKDF_OUTPUT_LEN,
        )
        .expect("attach");
        assert_eq!(&attach[..], &fv::NSR_ATTACH_PAYLOAD_KEY[..]);

        let mut payload_key = [0_u8; HKDF_OUTPUT_LEN];
        payload_key.copy_from_slice(&attach);
        let encrypted_payload_section =
            aead_encrypt(&payload_key, 0, fv::PAYLOAD_NSR, &state.transcript_hash())
                .expect("nsr payload");
        assert_eq!(encrypted_payload_section, fv::NSR_PAYLOAD_SECTION.to_vec());

        // Directional ES tag sets.
        let mut k_ab = [0_u8; HKDF_OUTPUT_LEN];
        k_ab.copy_from_slice(&split[..HKDF_OUTPUT_LEN]);
        let mut k_ba = [0_u8; HKDF_OUTPUT_LEN];
        k_ba.copy_from_slice(&split[HKDF_OUTPUT_LEN..]);
        let mut ab = EciesTagSet::dh_initialize(&state.chaining_key(), &k_ab).expect("ab");
        ab.begin_tag_ratchet().expect("ab ratchet");
        let ab1 = ab.next_entry().expect("ab1");
        assert_eq!(ab1.tag, fv::ES_TAG_AB_1);
        let k_ab0: [u8; HKDF_OUTPUT_LEN] = *ab.symm_key(0).expect("k0");
        assert_eq!(k_ab0, fv::ES_KEY_AB_0);
        let ab2 = ab.next_entry().expect("ab2");
        assert_eq!(ab2.tag, fv::ES_TAG_AB_2);
        let k_ab1: [u8; HKDF_OUTPUT_LEN] = *ab.symm_key(1).expect("k1");
        assert_eq!(k_ab1, fv::ES_KEY_AB_1);

        let mut ba = EciesTagSet::dh_initialize(&state.chaining_key(), &k_ba).expect("ba");
        ba.begin_tag_ratchet().expect("ba ratchet");
        let ba1 = ba.next_entry().expect("ba1");
        assert_eq!(ba1.tag, fv::ES_TAG_BA_1);
        let k_ba0: [u8; HKDF_OUTPUT_LEN] = *ba.symm_key(0).expect("bak0");
        assert_eq!(k_ba0, fv::ES_KEY_BA_0);

        // Existing Session message bodies.
        let es1 = aead_encrypt(&fv::ES_KEY_AB_0, 0, &fv::PAYLOAD_NS[..16], &ab1.tag).expect("es1");
        assert_eq!(es1, fv::ES_AB_MSG1_CIPHERTEXT.to_vec());
        let es2 = aead_encrypt(&fv::ES_KEY_AB_1, 1, &fv::PAYLOAD_NS[..16], &ab2.tag).expect("es2");
        assert_eq!(es2, fv::ES_AB_MSG2_CIPHERTEXT.to_vec());
    }

    #[test]
    fn full_handshake_round_trips_through_production_functions() {
        let mut rng = vector_rng();
        let mut bob_secret = [0_u8; STATIC_PUBLIC_LENGTH];
        rng.fill_bytes(&mut bob_secret);
        let bob_public = static_public(&bob_secret);

        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("alice eph");
        let (ns_message, sender) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            b"hello-bob",
        )
        .expect("seal ns");

        let opened =
            open_bound_new_session(&bob_secret, &bob_public, &ns_message).expect("open ns");
        assert_eq!(opened.payload, b"hello-bob");

        let reply =
            seal_new_session_reply(&opened.responder, &bob_secret, b"ack-from-bob", &mut rng)
                .expect("seal nsr");

        // The reply tag must be present in Alice's pending window.
        let mut pending_window = sender.reply_tag_set().expect("pending window");
        let matched = pending_window.next_entry().expect("window entry");
        assert_eq!(matched.tag, reply.message.tag);

        let reply_opened =
            open_new_session_reply(&sender, &fv::ALICE_STATIC_SECRET, &reply.message)
                .expect("open nsr");
        assert_eq!(reply_opened.payload, b"ack-from-bob");

        // Bidirectional Existing Session traffic.
        let mut alice_out = reply_opened.outbound_tag_set;
        let mut alice_in = reply_opened.inbound_tag_set;
        let mut bob_out = reply.outbound_tag_set;
        let mut bob_in = reply.inbound_tag_set;

        let es_a1 = seal_existing_session(&mut alice_out, b"a-to-b-one").expect("es a1");
        // Outbound ES tags come from the split-derived tag set, never
        // from the one-shot NSR reply-tag window.
        assert_ne!(es_a1.tag, matched.tag);
        let decoded = open_existing_session(&mut bob_in, 0, &es_a1).expect("bob");
        assert_eq!(decoded, b"a-to-b-one");

        let es_b1 = seal_existing_session(&mut bob_out, b"b-to-a-one").expect("es b1");
        let b_idx = bob_out.issued_entries() - 1;
        let decoded = open_existing_session(&mut alice_in, b_idx, &es_b1).expect("alice");
        assert_eq!(decoded, b"b-to-a-one");

        let es_a2 = seal_existing_session(&mut alice_out, b"a-to-b-two").expect("es a2");
        assert_ne!(es_a2.tag, es_a1.tag);
        let decoded = open_existing_session(&mut bob_in, 1, &es_a2).expect("bob 2");
        assert_eq!(decoded, b"a-to-b-two");

        let es_b2 = seal_existing_session(&mut bob_out, b"b-to-a-two").expect("es b2");
        let decoded = open_existing_session(&mut alice_in, b_idx + 1, &es_b2).expect("alice 2");
        assert_eq!(decoded, b"b-to-a-two");
    }

    #[test]
    fn wire_layout_offsets_match_specification() {
        let mut rng = vector_rng();
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let (ns, _) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            &[0xAB_u8; 37],
        )
        .expect("seal");
        let encoded = ns
            .encode_to_vec(MAX_NEW_SESSION_CIPHERTEXT)
            .expect("encode");
        // Length is 96 + payload length per specification section 1b.
        assert_eq!(encoded.len(), 96 + 37);
        assert_eq!(
            &encoded[..REPRESENTATIVE_LENGTH],
            ns.representative.as_bytes()
        );
        assert_eq!(&encoded[32..80], &ns.encrypted_static_section[..]);
        assert_eq!(&encoded[80..], &ns.encrypted_payload_section[..]);
        assert_eq!(ns.encrypted_static_section.len(), 48);
        let decoded =
            BoundNewSessionMessage::decode(&encoded, MAX_NEW_SESSION_CIPHERTEXT).expect("decode");
        assert_eq!(decoded, ns);

        let err = BoundNewSessionMessage::decode(&encoded[..95], MAX_NEW_SESSION_CIPHERTEXT)
            .expect_err("truncated");
        assert!(matches!(
            err,
            EciesError::CiphertextTooShort {
                actual: 95,
                minimum: 96
            }
        ));
    }

    #[test]
    fn reply_wire_layout_offsets_match_specification() {
        let mut rng = vector_rng();
        let mut bob_secret = [0_u8; STATIC_PUBLIC_LENGTH];
        rng.fill_bytes(&mut bob_secret);
        let bob_public = static_public(&bob_secret);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let (ns, _) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            b"payload",
        )
        .expect("seal");
        let opened = open_bound_new_session(&bob_secret, &bob_public, &ns).expect("open");
        let reply = seal_new_session_reply(&opened.responder, &bob_secret, &[7_u8; 11], &mut rng)
            .expect("nsr");
        let encoded = reply
            .message
            .encode_to_vec(MAX_NEW_SESSION_CIPHERTEXT)
            .expect("enc");
        // Total length is 72 + payload length per section 1g.
        assert_eq!(encoded.len(), 72 + 11);
        assert_eq!(&encoded[..8], &reply.message.tag[..]);
        assert_eq!(&encoded[8..40], reply.message.representative.as_bytes());
        assert_eq!(&encoded[40..56], &reply.message.key_section_mac[..]);
        assert_eq!(&encoded[56..], &reply.message.encrypted_payload_section[..]);
        let decoded =
            NewSessionReplyMessage::decode(&encoded, MAX_NEW_SESSION_CIPHERTEXT).expect("dec");
        assert_eq!(decoded, reply.message);
    }

    #[test]
    fn existing_session_wire_layout_matches_specification() {
        let mut tag_set =
            EciesTagSet::dh_initialize(&fv::SPLIT_K_AB, &fv::SPLIT_K_BA).expect("tagset");
        tag_set.begin_tag_ratchet().expect("ratchet");
        let message = seal_existing_session(&mut tag_set, b"abc").expect("seal es");
        let encoded = message
            .encode_to_vec(MAX_NEW_SESSION_CIPHERTEXT)
            .expect("enc");
        assert_eq!(encoded.len(), 8 + 3 + 16);
        assert_eq!(&encoded[..8], &message.tag[..]);
        let decoded =
            ExistingSessionMessage::decode(&encoded, MAX_NEW_SESSION_CIPHERTEXT).expect("dec");
        assert_eq!(decoded, message);
        let err = ExistingSessionMessage::decode(&encoded[..23], MAX_NEW_SESSION_CIPHERTEXT)
            .expect_err("short");
        assert!(matches!(
            err,
            EciesError::CiphertextTooShort { minimum: 24, .. }
        ));
    }

    #[test]
    fn wrong_bob_static_secret_fails_authentication() {
        let mut rng = vector_rng();
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let (message, _) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            b"secret",
        )
        .expect("seal");
        let mut wrong_secret = [0x77_u8; STATIC_PUBLIC_LENGTH];
        rng.fill_bytes(&mut wrong_secret[8..]);
        let err = open_bound_new_session(&wrong_secret, &bob_public, &message)
            .expect_err("wrong key must fail");
        assert!(matches!(err, EciesError::AuthenticationFailed));
    }

    #[test]
    fn tampered_static_or_payload_sections_fail() {
        let mut rng = vector_rng();
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let (message, _) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            b"secret",
        )
        .expect("seal");

        let mut tampered_static = message.clone();
        tampered_static.encrypted_static_section[3] ^= 1;
        assert!(matches!(
            open_bound_new_session(&fv::BOB_STATIC_SECRET, &bob_public, &tampered_static),
            Err(EciesError::AuthenticationFailed)
        ));

        let mut tampered_payload = message.clone();
        let last = tampered_payload.encrypted_payload_section.len() - 1;
        tampered_payload.encrypted_payload_section[last] ^= 1;
        assert!(matches!(
            open_bound_new_session(&fv::BOB_STATIC_SECRET, &bob_public, &tampered_payload),
            Err(EciesError::AuthenticationFailed)
        ));
    }

    #[test]
    fn wrong_nsr_tag_fails_authentication() {
        let mut rng = vector_rng();
        let mut bob_secret = [0_u8; STATIC_PUBLIC_LENGTH];
        rng.fill_bytes(&mut bob_secret);
        let bob_public = static_public(&bob_secret);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let (ns, sender) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            b"x",
        )
        .expect("seal");
        let opened = open_bound_new_session(&bob_secret, &bob_public, &ns).expect("open");
        let reply =
            seal_new_session_reply(&opened.responder, &bob_secret, b"y", &mut rng).expect("nsr");
        let mut wrong_tag = reply.message.clone();
        wrong_tag.tag[0] ^= 1;
        assert!(matches!(
            open_new_session_reply(&sender, &fv::ALICE_STATIC_SECRET, &wrong_tag),
            Err(EciesError::AuthenticationFailed)
        ));
    }

    #[test]
    fn unbound_new_session_is_rejected_typed() {
        // Hand-craft an unbound New Session: the static-key section
        // encrypts 32 zero bytes. Only the in-module test may build
        // this shape; production callers have no API for it.
        let mut rng = vector_rng();
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let aepk = decode_representative(&alice_ephemeral.representative()).expect("decode");

        let mut state = EciesNoiseState::new();
        state.mix_hash(&bob_public);
        state.mix_hash(&aepk);
        let es_shared =
            diffie_hellman_checked(alice_ephemeral.secret().as_bytes(), &bob_public).expect("es");
        let static_key = state.mix_key(es_shared.as_ref()).expect("mix");
        let zeros = [0_u8; STATIC_PUBLIC_LENGTH];
        let static_section =
            aead_encrypt(&static_key, 0, &zeros, &state.transcript_hash()).expect("enc");
        state.mix_hash(&static_section);
        // The unbound payload uses nonce 1 under the same section key.
        let payload =
            aead_encrypt(&static_key, 1, b"unbound", &state.transcript_hash()).expect("enc");
        let message =
            BoundNewSessionMessage::new(alice_ephemeral.representative(), static_section, payload)
                .expect("build");
        let err = open_bound_new_session(&fv::BOB_STATIC_SECRET, &bob_public, &message)
            .expect_err("unbound must be typed-rejected");
        assert_eq!(err, EciesError::UnboundNewSessionNotSupported);
    }

    #[test]
    fn new_session_transcript_steps_match_independent_reference() {
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let alice_ephemeral = frozen_alice_ephemeral();
        let aepk = decode_representative(&alice_ephemeral.representative()).expect("decode");

        let mut state = EciesNoiseState::new();
        state.mix_hash(&bob_public);
        assert_eq!(state.transcript_hash(), fv::NS_H_AFTER_BPK);
        state.mix_hash(&aepk);
        assert_eq!(state.transcript_hash(), fv::NS_H_AFTER_AEPK);

        // es
        let es_shared =
            diffie_hellman_checked(alice_ephemeral.secret().as_bytes(), &bob_public).expect("es");
        let static_section_key = state.mix_key(es_shared.as_ref()).expect("mix");
        assert_eq!(state.chaining_key(), fv::NS_ES_CHAIN_KEY);
        assert_eq!(static_section_key, fv::NS_ES_PAYLOAD_KEY);
        let alice_static_public = static_public(&fv::ALICE_STATIC_SECRET);
        let static_section = aead_encrypt(
            &static_section_key,
            0,
            &alice_static_public,
            &state.transcript_hash(),
        )
        .expect("enc");
        assert_eq!(static_section, fv::NS_STATIC_SECTION.to_vec());
        state.mix_hash(&static_section);
        assert_eq!(state.transcript_hash(), fv::NS_H_AFTER_STATIC);

        // ss
        let ss_shared = diffie_hellman_checked(&fv::ALICE_STATIC_SECRET, &bob_public).expect("ss");
        let payload_key = state.mix_key(ss_shared.as_ref()).expect("mix");
        assert_eq!(state.chaining_key(), fv::NS_SS_CHAIN_KEY);
        assert_eq!(payload_key, fv::NS_SS_PAYLOAD_KEY);
        let payload_section =
            aead_encrypt(&payload_key, 0, fv::PAYLOAD_NS, &state.transcript_hash()).expect("enc");
        assert_eq!(payload_section, fv::NS_PAYLOAD_SECTION.to_vec());
        state.mix_hash(&payload_section);
        assert_eq!(state.transcript_hash(), fv::NS_H_FINAL);
    }

    #[test]
    fn reply_tag_set_root_matches_independent_reference() {
        let tagset_key = crate::hkdf_sha256_extract_and_expand(
            &fv::NS_SS_CHAIN_KEY,
            b"",
            b"SessionReplyTags",
            HKDF_OUTPUT_LEN,
        )
        .expect("hkdf");
        assert_eq!(&tagset_key[..], &fv::NSR_TAGSET_KEY[..]);
    }

    #[test]
    fn existing_session_nonce_construction_is_index_based() {
        for (index, expected_byte4) in [(0_u32, 0_u8), (1, 1), (255, 255), (256, 0)] {
            let nonce = aead_nonce(u64::from(index));
            assert_eq!(nonce.len(), 12);
            assert!(nonce.iter().take(4).all(|byte| *byte == 0));
            assert_eq!(nonce[4], expected_byte4);
        }
    }

    #[test]
    fn keypair_representative_round_trip_recovers_public_key() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
        let recovered = decode_representative(&keypair.representative()).expect("decodes");
        let expected = ephemeral_public_key_for_test(keypair.secret().as_bytes());
        assert_eq!(recovered, expected);
    }

    // ---- Plan 131: production Elligator2 representation randomization ----

    /// Independent pure-Python reference fixtures (Plan 131 Phase A1).
    ///
    /// Generated once by `DECODE_ELG2`/`ENCODE_ELG2` implemented
    /// directly from the current I2P ECIES specification plus the
    /// Java I2P `Elligator2.java` branch structure — entirely
    /// independent of i2pr and of `elligator2`. The full generator
    /// (including a pure-Python RFC 7748 X25519 ladder cross-checked
    /// against the Python `cryptography` library) is recorded in
    /// `specs/references/ecies-destination-ratchet.md`.
    mod reference_fixtures {
        pub const X_COORDINATE: [u8; 32] = [
            0xf1, 0xbc, 0x54, 0x94, 0x20, 0x1d, 0xec, 0xa9, 0x39, 0x31, 0xa9, 0x42, 0x38, 0xfb,
            0xcd, 0x65, 0x7b, 0x06, 0x60, 0x15, 0xb9, 0x3c, 0x0a, 0x40, 0xbd, 0xa2, 0xf7, 0x57,
            0xff, 0xfb, 0xae, 0x42,
        ];
        /// Java I2P/i2pd alternative-branch encoding with `tweak & 1 == 0`
        /// and `tweak & 0xc0 == 0`. Computed by the independent
        /// Python reference against the same X25519 public point as
        /// `REPRESENTATIVE_TRUE_BRANCH`.
        pub const REPRESENTATIVE_FALSE_BRANCH: [u8; 32] = [
            0x41, 0x78, 0x20, 0xeb, 0x10, 0x86, 0x41, 0xea, 0xac, 0xa4, 0x9c, 0x4a, 0xd8, 0x82,
            0x99, 0x7e, 0xeb, 0x7d, 0xfa, 0x85, 0x13, 0x00, 0x5e, 0x08, 0x6d, 0x1c, 0x6c, 0x09,
            0xbd, 0x90, 0x2e, 0x0e,
        ];
        /// Java I2P/i2pd alternative-branch encoding with `tweak & 1 == 1`
        /// and `tweak & 0xc0 == 0`. Computed by the independent
        /// Python reference against the same X25519 public point as
        /// `REPRESENTATIVE_FALSE_BRANCH`.
        pub const REPRESENTATIVE_TRUE_BRANCH: [u8; 32] = [
            0xfb, 0xce, 0x79, 0xc9, 0x96, 0xc0, 0x77, 0xa9, 0xee, 0xc2, 0xff, 0xed, 0x41, 0x4b,
            0x75, 0x27, 0x11, 0x28, 0x8a, 0x10, 0x14, 0xd5, 0xb0, 0x78, 0xd0, 0xf8, 0x2e, 0x31,
            0x66, 0x25, 0xe3, 0x27,
        ];
        /// All four high-bit variants for the false-branch encoding,
        /// computed by the independent Python reference against the
        /// same X25519 public point. All four decode to
        /// `X_COORDINATE`.
        pub const REPRESENTATIVE_HIGH_00: [u8; 32] = [
            0x41, 0x78, 0x20, 0xeb, 0x10, 0x86, 0x41, 0xea, 0xac, 0xa4, 0x9c, 0x4a, 0xd8, 0x82,
            0x99, 0x7e, 0xeb, 0x7d, 0xfa, 0x85, 0x13, 0x00, 0x5e, 0x08, 0x6d, 0x1c, 0x6c, 0x09,
            0xbd, 0x90, 0x2e, 0x0e,
        ];
        pub const REPRESENTATIVE_HIGH_40: [u8; 32] = [
            0x41, 0x78, 0x20, 0xeb, 0x10, 0x86, 0x41, 0xea, 0xac, 0xa4, 0x9c, 0x4a, 0xd8, 0x82,
            0x99, 0x7e, 0xeb, 0x7d, 0xfa, 0x85, 0x13, 0x00, 0x5e, 0x08, 0x6d, 0x1c, 0x6c, 0x09,
            0xbd, 0x90, 0x2e, 0x4e,
        ];
        pub const REPRESENTATIVE_HIGH_80: [u8; 32] = [
            0x41, 0x78, 0x20, 0xeb, 0x10, 0x86, 0x41, 0xea, 0xac, 0xa4, 0x9c, 0x4a, 0xd8, 0x82,
            0x99, 0x7e, 0xeb, 0x7d, 0xfa, 0x85, 0x13, 0x00, 0x5e, 0x08, 0x6d, 0x1c, 0x6c, 0x09,
            0xbd, 0x90, 0x2e, 0x8e,
        ];
        pub const REPRESENTATIVE_HIGH_C0: [u8; 32] = [
            0x41, 0x78, 0x20, 0xeb, 0x10, 0x86, 0x41, 0xea, 0xac, 0xa4, 0x9c, 0x4a, 0xd8, 0x82,
            0x99, 0x7e, 0xeb, 0x7d, 0xfa, 0x85, 0x13, 0x00, 0x5e, 0x08, 0x6d, 0x1c, 0x6c, 0x09,
            0xbd, 0x90, 0x2e, 0xce,
        ];
    }

    #[test]
    fn production_generator_decodes_to_the_exact_intended_public_key() {
        // Plan 131 B3.2/B3.6: every production representative must
        // decode to the canonical X25519 public key the secret will
        // actually use for DH; randomized branch or high bits never
        // change the transcript's decoded point.
        let mut rng = ChaCha8Rng::seed_from_u64(0x130A);
        for _ in 0..64 {
            let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
            let recovered = decode_representative(&keypair.representative()).expect("decodes");
            let expected = ephemeral_public_key_for_test(keypair.secret().as_bytes());
            assert_eq!(recovered, expected);
        }
    }

    #[test]
    fn production_generator_randomizes_the_two_high_representation_bits() {
        // Plan 131 B3.4: over a deterministic seeded-CSPRNG sample
        // the two most significant on-wire bits are not fixed and
        // all four values occur. This is a regression/fingerprint
        // test with a fixed seed, not a randomness certification.
        let mut rng = ChaCha8Rng::seed_from_u64(0x130B);
        let mut observed = [false; 4];
        for _ in 0..256 {
            let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
            let top = keypair.representative().as_bytes()[31] >> 6;
            observed[usize::from(top)] = true;
        }
        assert!(
            observed.iter().all(|seen| *seen),
            "all four high-bit values must occur across the sample, got {observed:?}"
        );
    }

    #[test]
    fn production_generator_randomizes_the_inverse_map_branch_bit() {
        // Plan 131 B3.5: production generation draws both branch and
        // high-bit randomization from the CSPRNG. Over a deterministic
        // sample every branch observation occurs. This is a
        // fingerprint-regression test, not a randomness certification.
        let mut rng = ChaCha8Rng::seed_from_u64(0x131A);
        let mut branch_observed = [false; 2];
        let mut high_observed = [false; 4];
        for _ in 0..256 {
            let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
            let rep = keypair.representative();
            let bytes = rep.as_bytes();
            branch_observed[usize::from(bytes[0] & 0x01)] = true;
            high_observed[usize::from(bytes[31] >> 6)] = true;
        }
        assert!(
            branch_observed.iter().all(|seen| *seen),
            "both inverse-map branches must occur across the sample, got {branch_observed:?}"
        );
        assert!(
            high_observed.iter().all(|seen| *seen),
            "all four high-bit values must occur across the sample, got {high_observed:?}"
        );
    }

    #[test]
    fn deterministic_constructor_keeps_the_fixed_vector_representation() {
        // Plan 131 B3.1: the deterministic constructor still produces
        // the implementation-fixed high-bit pattern (`00`) and the
        // implementation-fixed branch bit so every frozen Plan 126
        // KDF/Noise vector remains reproducible. It is documented as
        // non-production precisely because of this.
        let keypair = EciesEphemeralKeypair::from_seed_bytes(fv::ALICE_EPHEMERAL_SEED)
            .expect("deterministic keypair");
        assert_eq!(keypair.representative().as_bytes()[31] & 0xc0, 0);
        let recovered = decode_representative(&keypair.representative()).expect("decodes");
        let expected = ephemeral_public_key_for_test(keypair.secret().as_bytes());
        assert_eq!(recovered, expected);
    }

    #[test]
    fn reference_high_bit_variants_all_decode_to_the_same_public_key() {
        // Plan 131 B3.3/A1: representatives produced by the normative
        // ENCODE_ELG2 randomization (all four high-bit values) decode
        // through i2pr's decoder to exactly the intended public key.
        for representative in [
            reference_fixtures::REPRESENTATIVE_HIGH_00,
            reference_fixtures::REPRESENTATIVE_HIGH_40,
            reference_fixtures::REPRESENTATIVE_HIGH_80,
            reference_fixtures::REPRESENTATIVE_HIGH_C0,
        ] {
            let decoded = decode_representative(&EciesEphemeralRepresentative(representative))
                .expect("reference randomized representative decodes");
            assert_eq!(decoded, reference_fixtures::X_COORDINATE);
        }
    }

    #[test]
    fn both_reference_encode_branches_decode_to_the_same_public_key() {
        // Plan 131 B3.5: Java I2P and i2pd additionally randomize the
        // Elligator2 pre-image branch on encode. Both branches are
        // canonical least-square-root representatives after their
        // normalization step, and both decode through i2pr to the
        // same Montgomery u-coordinate. The frozen fixtures were
        // produced by the independent Python reference, not by i2pr
        // or by the Rust dependency.
        let false_branch = decode_representative(&EciesEphemeralRepresentative(
            reference_fixtures::REPRESENTATIVE_FALSE_BRANCH,
        ))
        .expect("false-branch representative decodes");
        let true_branch = decode_representative(&EciesEphemeralRepresentative(
            reference_fixtures::REPRESENTATIVE_TRUE_BRANCH,
        ))
        .expect("true-branch representative decodes");
        assert_eq!(false_branch, reference_fixtures::X_COORDINATE);
        assert_eq!(true_branch, reference_fixtures::X_COORDINATE);
        assert_ne!(
            reference_fixtures::REPRESENTATIVE_FALSE_BRANCH,
            reference_fixtures::REPRESENTATIVE_TRUE_BRANCH,
            "the two branches are distinct encodings of one point"
        );
    }

    #[test]
    fn from_seed_bytes_with_tweak_produces_distinct_but_decoding_invariant_branches() {
        // Plan 131 B3.5: the explicit-tweak constructor exercises the
        // same API surface as production. The two branch values must
        // produce distinct on-wire bytes and decode to the same
        // Montgomery u-coordinate. Roughly half of all X25519 public
        // points are encodable in any given branch, so the test only
        // requires that encodable seeds prove the branch invariance
        // and distinctness.
        let mut rng = ChaCha8Rng::seed_from_u64(0x131B);
        let mut compared = 0_usize;
        let mut attempted = 0_usize;
        while compared < 8 && attempted < 256 {
            attempted += 1;
            let mut seed = [0_u8; REPRESENTATIVE_LENGTH];
            rng.fill_bytes(&mut seed);
            let low_branch = EciesEphemeralKeypair::from_seed_bytes_with_tweak(seed, 0);
            let high_branch = EciesEphemeralKeypair::from_seed_bytes_with_tweak(seed, 1);
            match (low_branch, high_branch) {
                (Some(low), Some(high)) => {
                    let low_decoded =
                        decode_representative(&low.representative()).expect("low-branch decodes");
                    let high_decoded =
                        decode_representative(&high.representative()).expect("high-branch decodes");
                    assert_eq!(low_decoded, high_decoded);
                    assert_ne!(
                        low.representative().as_bytes(),
                        high.representative().as_bytes(),
                        "the two branch encodings must produce different on-wire bytes"
                    );
                    compared += 1;
                }
                (None, None) => continue,
                (a, b) => panic!(
                    "branch encodability must be point-dependent, not tweak-dependent; \
                     got {a:?} / {b:?}"
                ),
            }
        }
        assert!(
            compared >= 4,
            "expected at least 4 encodable seed samples after {attempted} attempts, got {compared}"
        );
    }

    #[test]
    fn production_generator_accepts_entropy_failure_as_typed_error() {
        // A failing CSPRNG surfaces RandomnessUnavailable rather than
        // panicking or silently degrading to a fixed representation.
        #[derive(Debug)]
        struct FailingRng;
        #[derive(Debug)]
        struct FailingRngError;
        impl core::fmt::Display for FailingRngError {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("failing rng")
            }
        }
        impl rand_core::TryRngCore for FailingRng {
            type Error = FailingRngError;

            fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
                Err(FailingRngError)
            }

            fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
                Err(FailingRngError)
            }

            fn try_fill_bytes(&mut self, _: &mut [u8]) -> Result<(), Self::Error> {
                Err(FailingRngError)
            }
        }
        impl rand_core::TryCryptoRng for FailingRng {}
        let outcome = EciesEphemeralKeypair::generate(&mut FailingRng);
        assert_eq!(outcome.err(), Some(EciesError::RandomnessUnavailable));
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
        assert_eq!(
            decode_representative(&representative).err(),
            Some(EciesError::ElligatorDecode)
        );
    }

    #[test]
    fn diffie_hellman_rejects_all_zero_inputs() {
        let mut rng = ChaCha8Rng::seed_from_u64(13);
        let keypair = EciesEphemeralKeypair::generate(&mut rng).expect("keypair");
        let zero_pub = [0_u8; REPRESENTATIVE_LENGTH];
        assert_eq!(
            diffie_hellman_checked(keypair.secret().as_bytes(), &zero_pub).err(),
            Some(EciesError::AllZeroKey)
        );
        assert_eq!(
            diffie_hellman_checked(&zero_pub, &zero_pub).err(),
            Some(EciesError::AllZeroKey)
        );
    }

    #[test]
    fn random_representatives_fail_decode_or_succeed_without_panic() {
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
        for _ in 0..128 {
            let mut bytes = [0_u8; REPRESENTATIVE_LENGTH];
            rng.fill_bytes(&mut bytes);
            let rep = EciesEphemeralRepresentative(bytes);
            let outcome = decode_representative(&rep);
            assert!(outcome.is_ok() || matches!(outcome.err(), Some(EciesError::ElligatorDecode)));
        }
    }

    #[test]
    fn tag_set_issues_distinct_tags_and_advances_keys() {
        let mut tag_set =
            EciesTagSet::dh_initialize(&fv::SPLIT_K_AB, &fv::SPLIT_K_BA).expect("tagset");
        tag_set.begin_tag_ratchet().expect("ratchet");
        let first = tag_set.next_entry().expect("entry");
        let second = tag_set.next_entry().expect("entry");
        assert_ne!(first.tag, second.tag);
        assert_eq!(first.index, 0);
        assert_eq!(second.index, 1);
        let key0: [u8; HKDF_OUTPUT_LEN] = *tag_set.symm_key(0).expect("k0");
        let key1: [u8; HKDF_OUTPUT_LEN] = *tag_set.symm_key(1).expect("k1");
        assert_ne!(key0, key1);
        tag_set.trim_keys_below(1);
        let trimmed_again: [u8; HKDF_OUTPUT_LEN] = *tag_set.symm_key(1).expect("still derivable");
        let mut fresh =
            EciesTagSet::dh_initialize(&fv::SPLIT_K_AB, &fv::SPLIT_K_BA).expect("fresh");
        fresh.begin_tag_ratchet().expect("ratchet");
        let fresh_key: [u8; HKDF_OUTPUT_LEN] = *fresh.symm_key(1).expect("k1");
        assert_eq!(trimmed_again, fresh_key);
    }

    #[test]
    fn debug_outputs_never_reveal_key_material() {
        let mut rng = ChaCha8Rng::seed_from_u64(21);
        let bob_public = static_public(&fv::BOB_STATIC_SECRET);
        let alice_ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("eph");
        let (ns, sender) = seal_bound_new_session(
            &fv::ALICE_STATIC_SECRET,
            &alice_ephemeral,
            &bob_public,
            b"x",
        )
        .expect("seal");
        let sender_rendered = format!("{sender:?}");
        assert!(sender_rendered.contains("<redacted>"));
        assert!(!sender_rendered.contains("chaining_key"));

        let tag_set_rendered = format!("{:?}", sender.reply_tag_set().expect("tagset"));
        assert!(!tag_set_rendered.contains("tag_chain_key"));
        assert!(!tag_set_rendered.contains("symm_chain_key"));

        let ns_rendered = format!("{ns:?}");
        assert!(ns_rendered.contains("representative"));
    }
}
