//! ECIES-X25519 short tunnel-build cryptography seam.
//!
//! Plan 109 §7-§11 own the typed seam over the short tunnel-build
//! cryptography primitive. The module exposes:
//!
//! - [`BuildCryptography`] — a trait that every live build
//!   cryptography implementation must satisfy.
//! - [`BuildCryptographyError`] — the typed error categories.
//! - [`EciesX25519BuildCryptography`] — the Plan 109
//!   Noise-N implementation that protects a 154-byte plaintext
//!   request into a 218-byte sealed envelope and authenticates /
//!   decrypts the 202-byte hop-own reply.
//! - [`NoBuildCryptography`] — the Plan 107 fail-closed placeholder.
//! - [`ValidatedRecordSlot`] and the [`record_slot`] helper for
//!   the per-record hop-own reply nonce domain.
//!
//! The cryptography follows the current official I2P Tunnel
//! Creation Specification: Noise-N (`Noise_N_25519_ChaChaPoly_SHA256`),
//! literal `MixHash`/`MixKey` ordering, peer-static mix, ephemeral
//! mix, `es`, and post-AEAD transcript update. The 218-byte
//! encrypted request envelope is built from a 16-byte truncated
//! hop hash prefix, the 32-byte sender ephemeral X25519 public
//! key, the 154-byte ChaCha20-Poly1305 ciphertext, and the 16-byte
//! authentication tag. The hop-own reply uses the derived
//! `replyKey`, the caller's record slot as the AEAD nonce, and
//! the saved post-request `h` as the AEAD associated data.
//!
//! The seam never calls into a network or runtime. The
//! [`crate::short`] module composes the seam into the full state
//! machine.

#![forbid(unsafe_code)]

use std::fmt;

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use i2pr_crypto::{HkdfError, hkdf_sha256_extract_and_expand};
use rand_core::{CryptoRng, RngCore, TryRngCore};
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use i2pr_proto::{
    SHORT_BUILD_RECORD_SIZE, SHORT_REPLY_PLAINTEXT_SIZE, SHORT_REQUEST_PLAINTEXT_SIZE,
};

/// Length of an X25519 key in bytes.
pub const EPHEMERAL_KEY_LEN: usize = 32;
/// Length of a ChaCha20-Poly1305 AEAD key.
pub const AEAD_KEY_LEN: usize = 32;
/// Length of a ChaCha20-Poly1305 nonce consumed by the IETF API.
pub const AEAD_NONCE_LEN: usize = 12;
/// Length of a ChaCha20-Poly1305 authentication tag.
pub const TAG_LEN: usize = 16;
/// Length of the truncated hop identity-hash prefix in the request envelope.
pub const HASH_PREFIX_LEN: usize = 16;

/// Length of the Noise protocol name used for short tunnel-build
/// request transcripts. The exact 31-byte ASCII literal
/// `Noise_N_25519_ChaChaPoly_SHA256`.
const NOISE_PROTOCOL_NAME: &[u8] = b"Noise_N_25519_ChaChaPoly_SHA256";

/// The Noise transcript state for a single hop's request.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct NoiseRequestState {
    /// Running chaining key after `e → es`.
    ck: [u8; 32],
    /// Running transcript hash after the ciphertext mix.
    h: [u8; 32],
}

impl NoiseRequestState {
    /// Returns a copy of the chaining key.
    pub fn chaining_key(&self) -> [u8; 32] {
        self.ck
    }

    /// Returns a copy of the transcript hash.
    pub fn transcript_hash(&self) -> [u8; 32] {
        self.h
    }

    /// Mix `data` into the transcript hash.
    pub fn mix_hash(&mut self, data: &[u8]) {
        let mut hasher = Sha256::new();
        hasher.update(self.h);
        hasher.update(data);
        let output = hasher.finalize();
        self.h.copy_from_slice(&output);
    }

    /// Perform the spec `MixKey(shared)` derivation.
    pub fn mix_key(&mut self, shared: &[u8]) -> Result<(), BuildCryptographyError> {
        let keydata = mix_key_derive(&self.ck, shared)?;
        self.ck.copy_from_slice(&keydata[..32]);
        Ok(())
    }
}

impl fmt::Debug for NoiseRequestState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NoiseRequestState")
            .field("ck", &"<redacted>")
            .field("h", &"<redacted>")
            .finish()
    }
}

/// Compute the `MixKey(ck, input)` derivation as `HKDF(ck, input, "", 64)`,
/// returning the 64-byte derived material.
fn mix_key_derive(ck: &[u8; 32], input: &[u8]) -> Result<[u8; 64], BuildCryptographyError> {
    let keydata = hkdf_sha256_extract_and_expand(ck, input, &[], 64)?;
    let mut out = [0_u8; 64];
    if keydata.len() != out.len() {
        return Err(BuildCryptographyError::HkdfError(
            HkdfError::OutputLengthExceeded {
                requested: keydata.len(),
                maximum: 8160,
            },
        ));
    }
    out.copy_from_slice(&keydata);
    Ok(out)
}

/// Initial `h` value computed from the protocol name.
///
/// The Noise spec defines `h = protocol_name || 0` (zero-padded to
/// 32 bytes) when the protocol name is shorter than 32 bytes.
fn noise_init_h() -> [u8; 32] {
    debug_assert!(NOISE_PROTOCOL_NAME.len() <= 32);
    let mut h = [0_u8; 32];
    h[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
    h
}

/// Per-record slot for the hop-own reply nonce.
///
/// I2P ChaChaPoly for tunnel build records uses a 12-byte nonce that
/// is zero in its first 11 bytes and carries the record slot in the
/// final byte. Only slot values in `0..=7` are valid; the type
/// rejects every other value at construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedRecordSlot(u8);

impl Zeroize for ValidatedRecordSlot {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for ValidatedRecordSlot {}

impl ValidatedRecordSlot {
    /// Constructs a slot, rejecting any value greater than `7`.
    pub const fn new(value: u8) -> Result<Self, BuildCryptographyError> {
        if value > 7 {
            return Err(BuildCryptographyError::InvalidRecordSlot { value });
        }
        Ok(Self(value))
    }

    /// Returns the slot value in `[0, 7]`.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Returns the 12-byte ChaChaPoly nonce for this slot.
    pub fn nonce(self) -> [u8; AEAD_NONCE_LEN] {
        let mut nonce = [0_u8; AEAD_NONCE_LEN];
        nonce[11] = self.0;
        nonce
    }
}

/// Try a default record slot from an integer in `[0, 7]`.
pub fn record_slot(value: u8) -> Result<ValidatedRecordSlot, BuildCryptographyError> {
    ValidatedRecordSlot::new(value)
}

/// Typed error returned by the [`BuildCryptography`] seam methods.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BuildCryptographyError {
    /// The seam has no live primitive yet.
    #[error("build cryptography is unavailable")]
    Unavailable,
    /// The supplied record layout is not supported by this seam.
    #[error("build cryptography does not support layout {layout}")]
    UnsupportedLayout {
        /// The rejected layout label.
        layout: &'static str,
    },
    /// The supplied plaintext was the wrong length for the layout.
    #[error("plaintext length {actual} does not match layout size {expected}")]
    PlaintextLength {
        /// Actual plaintext length.
        actual: usize,
        /// Expected length.
        expected: usize,
    },
    /// The record length was not the canonical 218 bytes for the
    /// short layout.
    #[error("record length {actual} does not match short record size {expected}")]
    RecordLength {
        /// Actual record length.
        actual: usize,
        /// Expected length.
        expected: usize,
    },
    /// X25519 produced the forbidden all-zero shared secret.
    #[error("ECIES build produced the forbidden all-zero shared secret")]
    InvalidDhResult,
    /// AEAD authentication failed during open.
    #[error("ECIES build AEAD authentication failed")]
    AuthenticationFailed,
    /// AEAD encryption failed.
    #[error("ECIES build AEAD encryption failed")]
    EncryptionFailed,
    /// HKDF returned an error.
    #[error("ECIES build HKDF derivation failed: {0}")]
    HkdfError(HkdfError),
    /// A peer static key had an invalid representation.
    #[error("invalid peer static key material")]
    InvalidPeerKey,
    /// A caller supplied an invalid record slot value.
    #[error("invalid record slot value {value} (must be in 0..=7)")]
    InvalidRecordSlot {
        /// The rejected slot value.
        value: u8,
    },
    /// The supplied hop hash prefix did not match the recipient's identity.
    #[error("hop hash prefix did not match the recipient identity")]
    HashPrefixMismatch,
    /// The supplied record slot, reply key, or saved `h` failed
    /// AEAD authentication.
    #[error("reply AEAD authentication failed")]
    ReplyAuthenticationFailed,
    /// The supplied ephemeral public key was the all-zero value.
    #[error("ephemeral public key is all-zero")]
    AllZeroEphemeral,
    /// The cryptographic RNG was unable to produce output.
    #[error("cryptographic randomness unavailable")]
    RandomnessUnavailable,
    /// The inner short build record was rejected by the record layer.
    #[error("short build record rejected: {0}")]
    Record(#[from] crate::short_record::ShortBuildError),
}

impl From<HkdfError> for BuildCryptographyError {
    fn from(error: HkdfError) -> Self {
        BuildCryptographyError::HkdfError(error)
    }
}

/// Derived per-hop key material produced by `SMTunnel*` KDFs.
///
/// The struct is zeroizing and does not implement `Debug`. The
/// fields are the canonical post-request KDF outputs the I2P
/// specification publishes for layer decryption and reply
/// protection. `Clone` is implemented to support the Plan 110
/// postprocessor path that retains multiple independent views of
/// the same derived material; the zeroize-on-drop contract still
/// wipes the buffer on any drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct LayerKeys {
    /// `replyKey` derived from `SMTunnelReplyKey`.
    reply_key: [u8; 32],
    /// `layerKey` derived from `SMTunnelLayerKey`.
    layer_key: [u8; 32],
    /// `ivKey` derived from the post-`SMTunnelLayerKey` continuation.
    iv_key: [u8; 32],
    /// `garlicReplyKey` derived for OBEP-only continuation.
    garlic_reply_key: Option<[u8; 32]>,
    /// OBEP-only continuation tag prefix (length depends on spec
    /// version; current I2P uses 16 bytes).
    garlic_reply_tag_prefix: Option<[u8; 16]>,
}

impl LayerKeys {
    /// Constructs a `LayerKeys` owner from the validated KDF outputs.
    pub const fn new(reply_key: [u8; 32], layer_key: [u8; 32], iv_key: [u8; 32]) -> Self {
        Self {
            reply_key,
            layer_key,
            iv_key,
            garlic_reply_key: None,
            garlic_reply_tag_prefix: None,
        }
    }

    /// Constructs a `LayerKeys` owner for an OBEP hop, including
    /// the garlic-derived material.
    pub const fn new_obep(
        reply_key: [u8; 32],
        layer_key: [u8; 32],
        iv_key: [u8; 32],
        garlic_reply_key: [u8; 32],
        garlic_reply_tag_prefix: [u8; 16],
    ) -> Self {
        Self {
            reply_key,
            layer_key,
            iv_key,
            garlic_reply_key: Some(garlic_reply_key),
            garlic_reply_tag_prefix: Some(garlic_reply_tag_prefix),
        }
    }

    /// Returns the `replyKey`.
    pub const fn reply_key(&self) -> &[u8; 32] {
        &self.reply_key
    }

    /// Returns the `layerKey`.
    pub const fn layer_key(&self) -> &[u8; 32] {
        &self.layer_key
    }

    /// Returns the `ivKey`.
    pub const fn iv_key(&self) -> &[u8; 32] {
        &self.iv_key
    }

    /// Returns the OBEP garlic reply key when present.
    pub fn garlic_reply_key(&self) -> Option<&[u8; 32]> {
        self.garlic_reply_key.as_ref()
    }

    /// Returns the OBEP garlic reply tag prefix when present.
    pub fn garlic_reply_tag_prefix(&self) -> Option<&[u8; 16]> {
        self.garlic_reply_tag_prefix.as_ref()
    }
}

impl fmt::Debug for LayerKeys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LayerKeys")
            .field("reply_key", &"<redacted>")
            .field("layer_key", &"<redacted>")
            .field("iv_key", &"<redacted>")
            .finish()
    }
}

/// Trait every build-cryptography implementation must satisfy.
///
/// Plan 109 reduces the trait to the four exact operations the
/// I2P specification mandates: seal/open a 218-byte encrypted
/// short request and seal/open a 218-byte hop-own reply record.
pub trait BuildCryptography {
    /// Seals one 154-byte plaintext request record into a 218-byte
    /// short record under the supplied peer static ECIES-X25519
    /// key. Returns the canonical Noise-N request envelope.
    fn seal_short_request<R: CryptoRng + RngCore>(
        &self,
        plaintext: &[u8; SHORT_REQUEST_PLAINTEXT_SIZE],
        peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity_hash: &[u8; 32],
        rng: &mut R,
    ) -> Result<SealedShortRequest, BuildCryptographyError>;

    /// Seals one request using a caller-supplied ephemeral private key,
    /// skipping RNG generation. Used by the independent conformance
    /// fixture so the test can run against fixed expected bytes
    /// without depending on the RNG output of the production path.
    fn seal_short_request_with_ephemeral(
        &self,
        plaintext: &[u8; SHORT_REQUEST_PLAINTEXT_SIZE],
        peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity_hash: &[u8; 32],
        ephemeral_priv: &[u8; EPHEMERAL_KEY_LEN],
    ) -> Result<SealedShortRequest, BuildCryptographyError>;

    /// Opens one 218-byte short record and returns the 154-byte
    /// plaintext request together with the Noise-N post-request
    /// transcript state (for subsequent reply processing).
    fn open_short_request(
        &self,
        record: &[u8],
        peer_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity_hash: &[u8; 32],
    ) -> Result<OpenedShortRequest, BuildCryptographyError>;

    /// Seals one 202-byte reply record using the derived reply key,
    /// the supplied record slot nonce, and the saved post-request
    /// transcript hash.
    fn seal_short_reply(
        &self,
        plaintext: &[u8; SHORT_REPLY_PLAINTEXT_SIZE],
        layer_keys: &LayerKeys,
        request_hash: &[u8; 32],
        slot: ValidatedRecordSlot,
    ) -> Result<[u8; SHORT_BUILD_RECORD_SIZE], BuildCryptographyError>;

    /// Opens one 218-byte hop-own reply record using the derived
    /// reply key, the supplied record slot nonce, and the saved
    /// post-request transcript hash.
    fn open_short_reply(
        &self,
        record: &[u8],
        layer_keys: &LayerKeys,
        request_hash: &[u8; 32],
        slot: ValidatedRecordSlot,
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, BuildCryptographyError>;

    /// Returns the canonical name of the implementation.
    fn name(&self) -> &'static str;
}

/// The result of `seal_short_request` and
/// `seal_short_request_with_ephemeral`.
pub struct SealedShortRequest {
    /// The 218-byte sealed record.
    pub record: Zeroizing<[u8; SHORT_BUILD_RECORD_SIZE]>,
    /// The sender ephemeral X25519 public key carried in the record.
    pub ephemeral_pub: [u8; EPHEMERAL_KEY_LEN],
    /// Post-request Noise transcript state for downstream reply crypto.
    pub state: NoiseRequestState,
}

/// The result of `open_short_request`.
pub struct OpenedShortRequest {
    /// The decrypted 154-byte request plaintext.
    pub plaintext: Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>,
    /// Post-request Noise transcript state for downstream reply crypto.
    pub state: NoiseRequestState,
}

/// The Plan 107 default build-cryptography implementation: always
/// returns [`BuildCryptographyError::Unavailable`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoBuildCryptography;

impl BuildCryptography for NoBuildCryptography {
    fn seal_short_request<R: CryptoRng + RngCore>(
        &self,
        _plaintext: &[u8; SHORT_REQUEST_PLAINTEXT_SIZE],
        _peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        _hop_identity_hash: &[u8; 32],
        _rng: &mut R,
    ) -> Result<SealedShortRequest, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn seal_short_request_with_ephemeral(
        &self,
        _plaintext: &[u8; SHORT_REQUEST_PLAINTEXT_SIZE],
        _peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        _hop_identity_hash: &[u8; 32],
        _ephemeral_priv: &[u8; EPHEMERAL_KEY_LEN],
    ) -> Result<SealedShortRequest, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn open_short_request(
        &self,
        _record: &[u8],
        _peer_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        _hop_identity_hash: &[u8; 32],
    ) -> Result<OpenedShortRequest, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn seal_short_reply(
        &self,
        _plaintext: &[u8; SHORT_REPLY_PLAINTEXT_SIZE],
        _layer_keys: &LayerKeys,
        _request_hash: &[u8; 32],
        _slot: ValidatedRecordSlot,
    ) -> Result<[u8; SHORT_BUILD_RECORD_SIZE], BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn open_short_reply(
        &self,
        _record: &[u8],
        _layer_keys: &LayerKeys,
        _request_hash: &[u8; 32],
        _slot: ValidatedRecordSlot,
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn name(&self) -> &'static str {
        "no-build-cryptography"
    }
}

/// ECIES-X25519 short tunnel-build cryptography implementation
/// using the normative Noise-N transcript.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EciesX25519BuildCryptography;

impl EciesX25519BuildCryptography {
    /// Constructs the canonical ECIES-X25519 implementation.
    pub const fn new() -> Self {
        Self
    }
}

impl BuildCryptography for EciesX25519BuildCryptography {
    fn seal_short_request<R: CryptoRng + RngCore>(
        &self,
        plaintext: &[u8; SHORT_REQUEST_PLAINTEXT_SIZE],
        peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity_hash: &[u8; 32],
        rng: &mut R,
    ) -> Result<SealedShortRequest, BuildCryptographyError> {
        validate_peer_key(peer_static_key)?;
        let mut ephemeral_priv = [0_u8; EPHEMERAL_KEY_LEN];
        rng.try_fill_bytes(&mut ephemeral_priv)
            .map_err(|_| BuildCryptographyError::RandomnessUnavailable)?;
        self.seal_short_request_with_ephemeral(
            plaintext,
            peer_static_key,
            hop_identity_hash,
            &ephemeral_priv,
        )
    }

    fn seal_short_request_with_ephemeral(
        &self,
        plaintext: &[u8; SHORT_REQUEST_PLAINTEXT_SIZE],
        peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity_hash: &[u8; 32],
        ephemeral_priv: &[u8; EPHEMERAL_KEY_LEN],
    ) -> Result<SealedShortRequest, BuildCryptographyError> {
        validate_peer_key(peer_static_key)?;
        // 1. Noise-N initialization.
        let mut state = NoiseRequestState {
            ck: noise_init_h(),
            h: noise_init_h(),
        };
        // 2. Peer static mix.
        state.mix_hash(peer_static_key);
        // 3. Sender ephemeral public key.
        let ephemeral_pub = ephemeral_public(ephemeral_priv);
        state.mix_hash(&ephemeral_pub);
        // 4. `es` + MixKey.
        let shared = compute_dh(ephemeral_priv, peer_static_key)?;
        state.mix_key(&shared)?;
        let mut ephemeral_priv_copy = *ephemeral_priv;
        ephemeral_priv_copy.zeroize();
        // 5. Request AEAD: ChaCha20-Poly1305 with key = k, nonce = 0,
        //    plaintext = the 154-byte request, AD = current h.
        let keydata: [u8; 64] = mix_key_derive(&state.ck, &[])?;
        let key_bytes: [u8; AEAD_KEY_LEN] = keydata[AEAD_KEY_LEN..AEAD_KEY_LEN + AEAD_KEY_LEN]
            .try_into()
            .map_err(|_| BuildCryptographyError::HkdfError(HkdfError::InvalidKeyLength))?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        let nonce_array = [0_u8; AEAD_NONCE_LEN];
        let nonce = Nonce::from_slice(&nonce_array);
        let plaintext_vec = Zeroizing::new(plaintext.to_vec());
        let ciphertext_with_tag = cipher
            .encrypt(
                #[allow(clippy::needless_borrow)]
                &nonce,
                chacha20poly1305::aead::Payload {
                    msg: plaintext_vec.as_ref(),
                    aad: &state.h,
                },
            )
            .map_err(|_| BuildCryptographyError::EncryptionFailed)?;
        let mut ciphertext = Zeroizing::new([0_u8; SHORT_REQUEST_PLAINTEXT_SIZE + TAG_LEN]);
        if ciphertext_with_tag.len() != ciphertext.len() {
            return Err(BuildCryptographyError::EncryptionFailed);
        }
        ciphertext.copy_from_slice(&ciphertext_with_tag);
        // 6. Post-AEAD MixHash: h = SHA256(h || ciphertext_with_tag).
        state.mix_hash(ciphertext.as_ref());
        // 7. Assemble the envelope.
        let mut record = Zeroizing::new([0_u8; SHORT_BUILD_RECORD_SIZE]);
        record[..HASH_PREFIX_LEN].copy_from_slice(&hop_identity_hash[..HASH_PREFIX_LEN]);
        record[HASH_PREFIX_LEN..HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN]
            .copy_from_slice(&ephemeral_pub);
        record[HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN..].copy_from_slice(ciphertext.as_ref());
        Ok(SealedShortRequest {
            record,
            ephemeral_pub,
            state,
        })
    }

    fn open_short_request(
        &self,
        record: &[u8],
        peer_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity_hash: &[u8; 32],
    ) -> Result<OpenedShortRequest, BuildCryptographyError> {
        if record.len() != SHORT_BUILD_RECORD_SIZE {
            return Err(BuildCryptographyError::RecordLength {
                actual: record.len(),
                expected: SHORT_BUILD_RECORD_SIZE,
            });
        }
        // First 16 bytes must equal the truncated hop identity hash.
        if record[..HASH_PREFIX_LEN] != hop_identity_hash[..HASH_PREFIX_LEN] {
            return Err(BuildCryptographyError::HashPrefixMismatch);
        }
        // Parse the 32-byte ephemeral public key.
        let mut ephemeral_pub = [0_u8; EPHEMERAL_KEY_LEN];
        ephemeral_pub
            .copy_from_slice(&record[HASH_PREFIX_LEN..HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN]);
        if ephemeral_pub.iter().all(|byte| *byte == 0) {
            return Err(BuildCryptographyError::AllZeroEphemeral);
        }
        let ciphertext = &record[HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN..];
        // Noise-N initialization.
        let mut state = NoiseRequestState {
            ck: noise_init_h(),
            h: noise_init_h(),
        };
        state.mix_hash(peer_static_priv_to_public(peer_static_priv).as_slice());
        state.mix_hash(&ephemeral_pub);
        // Shared = X25519(localpriv, remote-ephemeral).
        let shared = compute_dh(peer_static_priv, &ephemeral_pub)?;
        state.mix_key(&shared)?;
        let keydata: [u8; 64] = mix_key_derive(&state.ck, &[])?;
        let key_bytes: [u8; AEAD_KEY_LEN] = keydata[AEAD_KEY_LEN..AEAD_KEY_LEN + AEAD_KEY_LEN]
            .try_into()
            .map_err(|_| BuildCryptographyError::HkdfError(HkdfError::InvalidKeyLength))?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        let nonce_array = [0_u8; AEAD_NONCE_LEN];
        let nonce = Nonce::from_slice(&nonce_array);
        let plaintext_vec = cipher
            .decrypt(
                #[allow(clippy::needless_borrow)]
                &nonce,
                chacha20poly1305::aead::Payload {
                    msg: ciphertext,
                    aad: &state.h,
                },
            )
            .map_err(|_| BuildCryptographyError::AuthenticationFailed)?;
        let mut plaintext = Zeroizing::new([0_u8; SHORT_REQUEST_PLAINTEXT_SIZE]);
        if plaintext_vec.len() != SHORT_REQUEST_PLAINTEXT_SIZE {
            return Err(BuildCryptographyError::PlaintextLength {
                actual: plaintext_vec.len(),
                expected: SHORT_REQUEST_PLAINTEXT_SIZE,
            });
        }
        plaintext.copy_from_slice(&plaintext_vec);
        state.mix_hash(ciphertext);
        Ok(OpenedShortRequest { plaintext, state })
    }

    fn seal_short_reply(
        &self,
        plaintext: &[u8; SHORT_REPLY_PLAINTEXT_SIZE],
        layer_keys: &LayerKeys,
        request_hash: &[u8; 32],
        slot: ValidatedRecordSlot,
    ) -> Result<[u8; SHORT_BUILD_RECORD_SIZE], BuildCryptographyError> {
        let key = Zeroizing::new(*layer_keys.reply_key());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
        let slot_nonce = slot.nonce();
        let nonce = Nonce::from_slice(&slot_nonce);
        let ciphertext_with_tag = cipher
            .encrypt(
                #[allow(clippy::needless_borrow)]
                &nonce,
                chacha20poly1305::aead::Payload {
                    msg: plaintext.as_ref(),
                    aad: request_hash,
                },
            )
            .map_err(|_| BuildCryptographyError::EncryptionFailed)?;
        let mut record = [0_u8; SHORT_BUILD_RECORD_SIZE];
        if ciphertext_with_tag.len() != SHORT_REPLY_PLAINTEXT_SIZE + TAG_LEN {
            return Err(BuildCryptographyError::EncryptionFailed);
        }
        record[..ciphertext_with_tag.len()].copy_from_slice(&ciphertext_with_tag);
        Ok(record)
    }

    fn open_short_reply(
        &self,
        record: &[u8],
        layer_keys: &LayerKeys,
        request_hash: &[u8; 32],
        slot: ValidatedRecordSlot,
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, BuildCryptographyError> {
        if record.len() != SHORT_BUILD_RECORD_SIZE {
            return Err(BuildCryptographyError::RecordLength {
                actual: record.len(),
                expected: SHORT_BUILD_RECORD_SIZE,
            });
        }
        let key = Zeroizing::new(*layer_keys.reply_key());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
        let slot_nonce = slot.nonce();
        let nonce = Nonce::from_slice(&slot_nonce);
        let plaintext_vec = cipher
            .decrypt(
                #[allow(clippy::needless_borrow)]
                &nonce,
                chacha20poly1305::aead::Payload {
                    msg: &record[..SHORT_REPLY_PLAINTEXT_SIZE + TAG_LEN],
                    aad: request_hash,
                },
            )
            .map_err(|_| BuildCryptographyError::ReplyAuthenticationFailed)?;
        let mut plaintext = Zeroizing::new([0_u8; SHORT_REPLY_PLAINTEXT_SIZE]);
        if plaintext_vec.len() != SHORT_REPLY_PLAINTEXT_SIZE {
            return Err(BuildCryptographyError::PlaintextLength {
                actual: plaintext_vec.len(),
                expected: SHORT_REPLY_PLAINTEXT_SIZE,
            });
        }
        plaintext.copy_from_slice(&plaintext_vec);
        Ok(plaintext)
    }

    fn name(&self) -> &'static str {
        "ecies-x25519-noise-n"
    }
}

fn validate_peer_key(key: &[u8; EPHEMERAL_KEY_LEN]) -> Result<(), BuildCryptographyError> {
    if key.iter().all(|byte| *byte == 0) {
        return Err(BuildCryptographyError::InvalidPeerKey);
    }
    Ok(())
}

fn ephemeral_public(priv_bytes: &[u8; EPHEMERAL_KEY_LEN]) -> [u8; EPHEMERAL_KEY_LEN] {
    let secret = StaticSecret::from(*priv_bytes);
    let public = X25519PublicKey::from(&secret);
    public.to_bytes()
}

fn peer_static_priv_to_public(priv_bytes: &[u8; EPHEMERAL_KEY_LEN]) -> [u8; EPHEMERAL_KEY_LEN] {
    ephemeral_public(priv_bytes)
}

fn compute_dh(
    priv_bytes: &[u8; EPHEMERAL_KEY_LEN],
    peer_pub: &[u8; EPHEMERAL_KEY_LEN],
) -> Result<[u8; EPHEMERAL_KEY_LEN], BuildCryptographyError> {
    if peer_pub.iter().all(|byte| *byte == 0) {
        return Err(BuildCryptographyError::InvalidDhResult);
    }
    let secret = StaticSecret::from(*priv_bytes);
    let peer = X25519PublicKey::from(*peer_pub);
    let shared = secret.diffie_hellman(&peer);
    if shared.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(BuildCryptographyError::InvalidDhResult);
    }
    let mut out = [0_u8; EPHEMERAL_KEY_LEN];
    out.copy_from_slice(shared.as_bytes());
    Ok(out)
}

/// Derive the post-request `LayerKeys` for a hop.
///
/// Operates on the post-request Noise state. The caller can continue
/// holding the state for further per-hop operations.
pub fn derive_layer_keys(
    post_request_state: &NoiseRequestState,
    is_outbound_endpoint: bool,
) -> Result<LayerKeys, BuildCryptographyError> {
    // SMTunnelReplyKey
    let reply_keydata =
        hkdf_sha256_extract_and_expand(&post_request_state.ck, &[], b"SMTunnelReplyKey", 64)?;
    if reply_keydata.len() != 64 {
        return Err(BuildCryptographyError::HkdfError(
            HkdfError::OutputLengthExceeded {
                requested: reply_keydata.len(),
                maximum: 8160,
            },
        ));
    }
    let mut ck = [0_u8; 32];
    ck.copy_from_slice(&reply_keydata[..32]);
    let mut reply_key = [0_u8; 32];
    reply_key.copy_from_slice(&reply_keydata[32..64]);
    // SMTunnelLayerKey
    let layer_keydata = hkdf_sha256_extract_and_expand(&ck, &[], b"SMTunnelLayerKey", 64)?;
    if layer_keydata.len() != 64 {
        return Err(BuildCryptographyError::HkdfError(
            HkdfError::OutputLengthExceeded {
                requested: layer_keydata.len(),
                maximum: 8160,
            },
        ));
    }
    let mut iv_key = [0_u8; 32];
    let mut layer_key = [0_u8; 32];
    layer_key.copy_from_slice(&layer_keydata[32..64]);
    if !is_outbound_endpoint {
        // Non-OBEP: iv_key is the first 32 bytes.
        iv_key.copy_from_slice(&layer_keydata[..32]);
    } else {
        // OBEP continuation: ck becomes the first 32 bytes.
        ck.copy_from_slice(&layer_keydata[..32]);
        let iv_keydata = hkdf_sha256_extract_and_expand(&ck, &[], b"TunnelLayerIVKey", 64)?;
        if iv_keydata.len() != 64 {
            return Err(BuildCryptographyError::HkdfError(
                HkdfError::OutputLengthExceeded {
                    requested: iv_keydata.len(),
                    maximum: 8160,
                },
            ));
        }
        ck.copy_from_slice(&iv_keydata[..32]);
        iv_key.copy_from_slice(&iv_keydata[32..64]);
        let garlic_keydata = hkdf_sha256_extract_and_expand(&ck, &[], b"RGarlicKeyAndTag", 64)?;
        if garlic_keydata.len() != 64 {
            return Err(BuildCryptographyError::HkdfError(
                HkdfError::OutputLengthExceeded {
                    requested: garlic_keydata.len(),
                    maximum: 8160,
                },
            ));
        }
        let mut tag = [0_u8; 16];
        tag.copy_from_slice(&garlic_keydata[..16]);
        let mut garlic_reply_key = [0_u8; 32];
        garlic_reply_key.copy_from_slice(&garlic_keydata[32..64]);
        return Ok(LayerKeys::new_obep(
            reply_key,
            layer_key,
            iv_key,
            garlic_reply_key,
            tag,
        ));
    }
    Ok(LayerKeys::new(reply_key, layer_key, iv_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn fixed_rng(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    fn privkey(seed: u64) -> [u8; EPHEMERAL_KEY_LEN] {
        let mut rng = fixed_rng(seed);
        let mut bytes = [0_u8; EPHEMERAL_KEY_LEN];
        rng.fill_bytes(&mut bytes);
        bytes
    }

    fn hop_hash() -> [u8; 32] {
        let mut h = [0_u8; 32];
        for (i, slot) in h.iter_mut().enumerate() {
            *slot = (i.wrapping_mul(7)) as u8;
        }
        h
    }

    #[test]
    fn noise_init_h_matches_protocol_name_padded() {
        let mut expected = [0_u8; 32];
        expected[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
        assert_eq!(noise_init_h(), expected);
    }

    #[test]
    fn record_slot_accepts_zero_through_seven() {
        for slot in 0u8..=7 {
            assert_eq!(ValidatedRecordSlot::new(slot).expect("ok").get(), slot);
        }
        assert!(matches!(
            ValidatedRecordSlot::new(8),
            Err(BuildCryptographyError::InvalidRecordSlot { value: 8 })
        ));
        assert!(matches!(
            ValidatedRecordSlot::new(255),
            Err(BuildCryptographyError::InvalidRecordSlot { value: 255 })
        ));
    }

    #[test]
    fn record_slot_nonce_is_zero_padded_then_slot_byte() {
        let slot = ValidatedRecordSlot::new(5).expect("ok");
        let nonce = slot.nonce();
        assert_eq!(nonce[..11], [0_u8; 11]);
        assert_eq!(nonce[11], 5);
    }

    #[test]
    fn seal_and_open_round_trip_for_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(1);
        let responder_priv = privkey(2);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0x11_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        assert_eq!(sealed.record.len(), SHORT_BUILD_RECORD_SIZE);
        // First 16 bytes are the truncated hop hash.
        assert_eq!(sealed.record[..HASH_PREFIX_LEN], hash[..HASH_PREFIX_LEN]);
        // Bytes 16..48 are the ephemeral public key.
        assert_eq!(
            &sealed.record[HASH_PREFIX_LEN..HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN],
            &sealed.ephemeral_pub
        );
        // Bytes 48..218 are the 154 + 16 byte ciphertext+tag.
        assert_eq!(
            sealed.record[HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN..].len(),
            SHORT_REQUEST_PLAINTEXT_SIZE + TAG_LEN
        );

        let opened = cryptography
            .open_short_request(sealed.record.as_ref(), &responder_priv, &hash)
            .expect("open");
        assert_eq!(opened.plaintext.as_ref(), &plaintext[..]);
        assert_eq!(
            opened.state.transcript_hash(),
            sealed.state.transcript_hash()
        );
        assert_eq!(opened.state.chaining_key(), sealed.state.chaining_key());
    }

    #[test]
    fn open_request_rejects_truncated_hop_hash_mismatch() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(3);
        let responder_priv = privkey(4);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0x22_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let mut wrong_hash = hash;
        wrong_hash[0] ^= 0x01;
        let outcome =
            cryptography.open_short_request(sealed.record.as_ref(), &responder_priv, &wrong_hash);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::HashPrefixMismatch)
        ));
    }

    #[test]
    fn altered_ephemeral_key_rejects_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(5);
        let responder_priv = privkey(6);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0x33_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let mut bytes = sealed.record.to_vec();
        bytes[HASH_PREFIX_LEN] ^= 0x01;
        let outcome = cryptography.open_short_request(&bytes, &responder_priv, &hash);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn altered_ciphertext_tag_rejects_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(7);
        let responder_priv = privkey(8);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0x44_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let mut bytes = sealed.record.to_vec();
        let position = HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN + SHORT_REQUEST_PLAINTEXT_SIZE - 1;
        bytes[position] ^= 0x01;
        let outcome = cryptography.open_short_request(&bytes, &responder_priv, &hash);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn ephemeral_keys_are_unique_across_consecutive_seals() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(9);
        let responder_pub = [0x55_u8; EPHEMERAL_KEY_LEN];
        let hash = hop_hash();
        let plaintext = [0x66_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed_a = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal a");
        let sealed_b = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal b");
        assert_ne!(
            sealed_a.ephemeral_pub, sealed_b.ephemeral_pub,
            "ephemeral public keys must differ"
        );
    }

    #[test]
    fn sealing_rejects_zero_peer_key() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(10);
        let hash = hop_hash();
        let plaintext = [0x77_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let outcome = cryptography.seal_short_request(
            &plaintext,
            &[0_u8; EPHEMERAL_KEY_LEN],
            &hash,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::InvalidPeerKey)
        ));
    }

    #[test]
    fn layer_keys_derive_distinct_reply_layer_iv_for_participant() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(11);
        let responder_priv = privkey(12);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0x88_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let derived = derive_layer_keys(&sealed.state, false).expect("derive");
        assert_ne!(derived.reply_key(), derived.layer_key());
        assert_ne!(derived.layer_key(), derived.iv_key());
        assert_ne!(derived.reply_key(), derived.iv_key());
        assert!(derived.garlic_reply_key().is_none());
    }

    #[test]
    fn layer_keys_for_obep_include_garlic_material() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(13);
        let responder_priv = privkey(14);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0x99_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let derived = derive_layer_keys(&sealed.state, true).expect("derive obep");
        assert!(derived.garlic_reply_key().is_some());
        assert!(derived.garlic_reply_tag_prefix().is_some());
    }

    #[test]
    fn reply_round_trip_through_seal_and_open() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(15);
        let responder_priv = privkey(16);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0xAB_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let layer_keys = derive_layer_keys(&sealed.state, false).expect("keys");
        let reply_plaintext: [u8; SHORT_REPLY_PLAINTEXT_SIZE] =
            std::array::from_fn(|i| ((i.wrapping_add(5)) % 251) as u8);
        let slot = ValidatedRecordSlot::new(3).expect("slot");
        let reply_record = cryptography
            .seal_short_reply(
                &reply_plaintext,
                &layer_keys,
                &sealed.state.transcript_hash(),
                slot,
            )
            .expect("seal reply");
        let opened_reply = cryptography
            .open_short_reply(
                &reply_record,
                &layer_keys,
                &sealed.state.transcript_hash(),
                slot,
            )
            .expect("open reply");
        assert_eq!(opened_reply.as_ref(), &reply_plaintext[..]);
    }

    #[test]
    fn reply_open_rejects_wrong_slot() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(17);
        let responder_priv = privkey(18);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0xCC_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let layer_keys = derive_layer_keys(&sealed.state, false).expect("keys");
        let reply_plaintext: [u8; SHORT_REPLY_PLAINTEXT_SIZE] =
            std::array::from_fn(|i| (i.wrapping_mul(3)) as u8);
        let slot_three = ValidatedRecordSlot::new(3).expect("slot");
        let reply_record = cryptography
            .seal_short_reply(
                &reply_plaintext,
                &layer_keys,
                &sealed.state.transcript_hash(),
                slot_three,
            )
            .expect("seal reply");
        let slot_four = ValidatedRecordSlot::new(4).expect("slot");
        let outcome = cryptography.open_short_reply(
            &reply_record,
            &layer_keys,
            &sealed.state.transcript_hash(),
            slot_four,
        );
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::ReplyAuthenticationFailed)
        ));
    }

    #[test]
    fn reply_open_rejects_wrong_request_h() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(19);
        let responder_priv = privkey(20);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0xDD_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let layer_keys = derive_layer_keys(&sealed.state, false).expect("keys");
        let reply_plaintext: [u8; SHORT_REPLY_PLAINTEXT_SIZE] = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        let slot = ValidatedRecordSlot::new(7).expect("slot");
        let reply_record = cryptography
            .seal_short_reply(
                &reply_plaintext,
                &layer_keys,
                &sealed.state.transcript_hash(),
                slot,
            )
            .expect("seal reply");
        let mut wrong_h = sealed.state.transcript_hash();
        wrong_h[0] ^= 0x01;
        let outcome = cryptography.open_short_reply(&reply_record, &layer_keys, &wrong_h, slot);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::ReplyAuthenticationFailed)
        ));
    }

    #[test]
    fn envelope_total_size_is_exactly_218_bytes() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(21);
        let responder_priv = privkey(22);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0xEE_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        assert_eq!(sealed.record.len(), 218);
    }

    #[test]
    fn plan108_envelope_layout_rejected() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(23);
        let responder_priv = privkey(24);
        let responder_pub = ephemeral_public(&responder_priv);
        let hash = hop_hash();
        let plaintext = [0xFF_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, &hash, &mut rng)
            .expect("seal");
        let mut synthetic = vec![0_u8; 218];
        synthetic[HASH_PREFIX_LEN..HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN]
            .copy_from_slice(&sealed.ephemeral_pub);
        synthetic[HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN..]
            .copy_from_slice(&sealed.record[HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN..]);
        let outcome = cryptography.open_short_request(&synthetic, &responder_priv, &hash);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::HashPrefixMismatch)
        ));
    }
}
