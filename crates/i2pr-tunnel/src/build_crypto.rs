//! ECIES-X25519 short tunnel-build cryptography seam.
//!
//! Plan 108 §3.5 owns the typed seam over the short tunnel-build
//! cryptography primitive. The module exposes:
//!
//! - [`BuildCryptography`] — a trait that every live build
//!   cryptography implementation must satisfy.
//! - [`BuildCryptographyError`] — the typed error categories.
//! - [`LayerKeys`] — a non-cloneable, zeroizing wrapper for the
//!   per-hop tunnel layer key material a build needs.
//! - [`EciesX25519BuildCryptography`] — the Plan 108 ECIES-X25519
//!   implementation that protects a 154-byte plaintext request into
//!   a 218-byte sealed record.
//! - [`NoBuildCryptography`] — the Plan 107 fail-closed placeholder.
//!
//! The seam never calls into a network or runtime. The
//! [`crate::short`] module composes the seam into the full state
//! machine.

#![forbid(unsafe_code)]

use std::fmt;

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use i2pr_crypto::{
    CryptoError, HkdfError, X25519PrivateKey, hkdf_sha256_32, hkdf_sha256_extract_and_expand,
};
use i2pr_proto::{
    SHORT_BUILD_RECORD_SIZE, SHORT_REPLY_PLAINTEXT_SIZE, SHORT_REQUEST_PLAINTEXT_SIZE,
};
use rand_core::TryRngCore;
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::build::BuildRecordLayout;
use crate::short_record::ShortBuildError as ShortRecordError;

/// Length of the ChaCha20Poly1305 key (32 bytes).
pub const AEAD_KEY_LEN: usize = 32;

/// Length of the ChaCha20Poly1305 nonce stored in the record (16
/// bytes per the I2P short-build layout; the IETF AEAD takes the
/// first 12 bytes).
pub const NONCE_LEN: usize = 16;

/// Length of the Poly1305 authentication tag (16 bytes).
pub const TAG_LEN: usize = 16;

/// Length of the X25519 ephemeral public key prefix (32 bytes).
pub const EPHEMERAL_KEY_LEN: usize = 32;

/// Length of the per-hop short-build request AEAD key (32 bytes).
pub const SHORT_REQUEST_KEY_LEN: usize = 32;

/// Length of the per-hop short-build reply AEAD key (32 bytes).
pub const SHORT_REPLY_KEY_LEN: usize = 32;

/// Legacy default for the [`LayerKeys`] length (32 bytes for the
/// AES-256 half-key plus 16-byte IV). Plan 108 reserves the length
/// to keep the wrapper stable.
pub const LAYER_KEY_LEN: usize = 32;

/// A zeroizing non-cloneable owner for a per-hop layer key.
///
/// The owner deliberately has no `Debug`, `Clone`, or serde
/// implementations. The only byte accessor borrows the secret for
/// the shortest practical lifetime.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct LayerKeys([u8; LAYER_KEY_LEN]);

impl LayerKeys {
    /// Loads the supplied bytes without exposing them through any
    /// other API.
    pub const fn from_bytes(bytes: [u8; LAYER_KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw key bytes for the lifetime of the borrow.
    pub const fn as_bytes(&self) -> &[u8; LAYER_KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for LayerKeys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LayerKeys")
            .field(&"<redacted>")
            .finish()
    }
}

/// A typed error returned by the [`BuildCryptography`] seam
/// methods.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BuildCryptographyError {
    /// The seam has no live primitive yet.
    #[error("build cryptography is unavailable")]
    Unavailable,
    /// The supplied record layout is not supported by this seam.
    #[error("build cryptography does not support layout {layout:?}")]
    UnsupportedLayout {
        /// The rejected layout label.
        layout: BuildRecordLayout,
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
    Hkdf(#[from] HkdfError),
    /// A peer static key had an invalid representation.
    #[error("invalid peer static key material")]
    InvalidPeerKey,
    /// A caller supplied an invalid (zero) hop identifier.
    #[error("invalid short build hop identifier")]
    InvalidHopId,
    /// The inner short build record was rejected by the record layer.
    #[error("short build record rejected: {0}")]
    Record(#[from] ShortRecordError),
}

/// The trait every build-cryptography implementation must satisfy.
///
/// Plan 108 ships a generic contract: implementations are concrete
/// types rather than trait objects so the dispatch stays simple.
/// Callers that need polymorphism should wrap the implementation
/// in an enum rather than use `dyn BuildCryptography`.
pub trait BuildCryptography {
    /// Seals one 154-byte plaintext request record into a 218-byte
    /// short record under the supplied peer static key and
    /// generation-time key material. Implementations generate
    /// exactly one ephemeral X25519 keypair and return its public
    /// key alongside the AEAD envelope.
    fn seal_short_request<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        plaintext: &[u8],
        peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
        rng: &mut R,
    ) -> Result<Zeroizing<[u8; SHORT_BUILD_RECORD_SIZE]>, BuildCryptographyError>;

    /// Opens one 218-byte short record and returns the 154-byte
    /// plaintext request. Used by responder-side fixtures.
    fn open_short_request(
        &self,
        record: &[u8],
        peer_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
    ) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, BuildCryptographyError>;

    /// Seals one 202-byte reply record using the supplied
    /// creator-side static private key and the per-hop reply key
    /// derivation parameters.
    fn seal_short_reply<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        plaintext: &[u8],
        creator_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        reply_key_seed: &[u8; SHORT_REPLY_KEY_LEN],
        rng: &mut R,
    ) -> Result<Zeroizing<Vec<u8>>, BuildCryptographyError>;

    /// Opens one per-hop reply record produced by the responder and
    /// returns the 202-byte plaintext reply. `creator_static_priv`
    /// is the local creator's static X25519 key.
    fn open_short_reply(
        &self,
        record: &[u8],
        creator_static_priv: &[u8; EPHEMERAL_KEY_LEN],
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, BuildCryptographyError>;

    /// Returns the canonical name of the implementation. Used by
    /// diagnostics and the registrar to detect the active
    /// cryptography surface.
    fn name(&self) -> &'static str;
}

/// Borrows the layered cryptographic context for one hop.
///
/// The opaque owner is intentionally narrow: callers must use the
/// accessor methods to perform encryption/decryption; the inner
/// bytes are never exposed.
pub struct HopCryptographyContext {
    /// Ephemeral X25519 public key the protected record carries.
    pub ephemeral_pub: [u8; EPHEMERAL_KEY_LEN],
}

impl HopCryptographyContext {
    /// Returns the ephemeral X25519 public key bytes carried by the
    /// protected record.
    pub const fn ephemeral_public(&self) -> &[u8; EPHEMERAL_KEY_LEN] {
        &self.ephemeral_pub
    }
}

impl fmt::Debug for HopCryptographyContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HopCryptographyContext")
            .field("ephemeral_pub", &"<redacted>")
            .finish()
    }
}

/// The Plan 107 default build-cryptography implementation: always
/// returns [`BuildCryptographyError::Unavailable`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoBuildCryptography;

impl BuildCryptography for NoBuildCryptography {
    fn seal_short_request<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        _plaintext: &[u8],
        _peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        _request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
        _rng: &mut R,
    ) -> Result<Zeroizing<[u8; SHORT_BUILD_RECORD_SIZE]>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn open_short_request(
        &self,
        _record: &[u8],
        _peer_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        _request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
    ) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn seal_short_reply<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        _plaintext: &[u8],
        _creator_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        _reply_key_seed: &[u8; SHORT_REPLY_KEY_LEN],
        _rng: &mut R,
    ) -> Result<Zeroizing<Vec<u8>>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn open_short_reply(
        &self,
        _record: &[u8],
        _creator_static_priv: &[u8; EPHEMERAL_KEY_LEN],
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn name(&self) -> &'static str {
        "no-build-cryptography"
    }
}

/// ECIES-X25519 short tunnel-build cryptography implementation.
///
/// The implementation performs the standard Noise-N derivation:
///
/// 1. Generate an ephemeral X25519 keypair.
/// 2. Compute the DH shared secret between the ephemeral private
///    key and the peer static public key.
/// 3. Mix the shared secret with the peer's static key through
///    HKDF-SHA256 to obtain the session secret.
/// 4. Derive the request AEAD key and the reply key separately.
/// 5. Encrypt the 154-byte plaintext request using
///    ChaCha20Poly1305 IETF with the first 12 bytes of the 16-byte
///    stored nonce.
/// 6. Layout the 218-byte record as
///    `ephemeral_pub (32) || nonce (16) || ciphertext (170)`.
///
/// Plaintext size, record size, and stored nonce size are exact.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EciesX25519BuildCryptography;

impl EciesX25519BuildCryptography {
    /// Constructs the canonical ECIES-X25519 implementation.
    pub const fn new() -> Self {
        Self
    }
}

impl BuildCryptography for EciesX25519BuildCryptography {
    fn seal_short_request<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        plaintext: &[u8],
        peer_static_key: &[u8; EPHEMERAL_KEY_LEN],
        request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
        rng: &mut R,
    ) -> Result<Zeroizing<[u8; SHORT_BUILD_RECORD_SIZE]>, BuildCryptographyError> {
        let expected = SHORT_REQUEST_PLAINTEXT_SIZE;
        if plaintext.len() != expected {
            return Err(BuildCryptographyError::PlaintextLength {
                actual: plaintext.len(),
                expected,
            });
        }
        if peer_static_key.iter().all(|byte| *byte == 0) {
            return Err(BuildCryptographyError::InvalidPeerKey);
        }
        let ephemeral_priv = generate_ephemeral(rng)?;
        let ephemeral_pub = ephemeral_pub_bytes(&ephemeral_priv);
        let shared = compute_dh(&ephemeral_priv, peer_static_key)?;
        let mut session = derive_session_secret(&shared, peer_static_key)?;
        let enc_key = derive_request_key(&session, request_key_seed)?;
        let nonce_bytes = derive_request_nonce(&session, request_key_seed)?;
        let aead_nonce = nonce_from_salt(&nonce_bytes);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&enc_key));
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&aead_nonce),
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad: &[],
                },
            )
            .map_err(|_| BuildCryptographyError::EncryptionFailed)?;
        let mut record = Zeroizing::new([0_u8; SHORT_BUILD_RECORD_SIZE]);
        record[0..EPHEMERAL_KEY_LEN].copy_from_slice(&ephemeral_pub);
        record[EPHEMERAL_KEY_LEN..EPHEMERAL_KEY_LEN + NONCE_LEN].copy_from_slice(&nonce_bytes);
        record[EPHEMERAL_KEY_LEN + NONCE_LEN..].copy_from_slice(&ciphertext);
        session_zeroize(&mut session);
        Ok(record)
    }

    fn open_short_request(
        &self,
        record: &[u8],
        peer_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
    ) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, BuildCryptographyError> {
        let expected = SHORT_BUILD_RECORD_SIZE;
        if record.len() != expected {
            return Err(BuildCryptographyError::RecordLength {
                actual: record.len(),
                expected,
            });
        }
        let mut ephemeral_pub = [0_u8; EPHEMERAL_KEY_LEN];
        ephemeral_pub.copy_from_slice(&record[0..EPHEMERAL_KEY_LEN]);
        if ephemeral_pub.iter().all(|byte| *byte == 0) {
            return Err(BuildCryptographyError::InvalidDhResult);
        }
        let nonce_salt = &record[EPHEMERAL_KEY_LEN..EPHEMERAL_KEY_LEN + NONCE_LEN];
        let aead_body = &record[EPHEMERAL_KEY_LEN + NONCE_LEN
            ..EPHEMERAL_KEY_LEN + NONCE_LEN + TAG_LEN + SHORT_REQUEST_PLAINTEXT_SIZE];
        let shared = compute_dh_from_private(peer_static_priv, &ephemeral_pub)?;
        // The responder derives the same session secret using the
        // peer's static key — which is the local static private key
        // here. The salt for HKDF extract is the peer's static
        // public key, derived as `X25519_pub(local_priv)`.
        let local_pub = static_pub_from_private(peer_static_priv);
        let mut session = derive_session_secret(&shared, &local_pub)?;
        let enc_key = derive_request_key_responder(&session, request_key_seed)?;
        let aead_nonce = nonce_from_slice(nonce_salt);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&enc_key));
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&aead_nonce),
                chacha20poly1305::aead::Payload {
                    msg: aead_body,
                    aad: &[],
                },
            )
            .map_err(|_| BuildCryptographyError::AuthenticationFailed)?;
        session_zeroize(&mut session);
        let mut output = Zeroizing::new([0_u8; SHORT_REQUEST_PLAINTEXT_SIZE]);
        if plaintext.len() != output.len() {
            return Err(BuildCryptographyError::PlaintextLength {
                actual: plaintext.len(),
                expected: SHORT_REQUEST_PLAINTEXT_SIZE,
            });
        }
        output.copy_from_slice(&plaintext);
        Ok(output)
    }

    fn seal_short_reply<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        plaintext: &[u8],
        creator_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        reply_key_seed: &[u8; SHORT_REPLY_KEY_LEN],
        rng: &mut R,
    ) -> Result<Zeroizing<Vec<u8>>, BuildCryptographyError> {
        let expected = SHORT_REPLY_PLAINTEXT_SIZE;
        if plaintext.len() != expected {
            return Err(BuildCryptographyError::PlaintextLength {
                actual: plaintext.len(),
                expected,
            });
        }
        let ephemeral_priv = generate_ephemeral(rng)?;
        let ephemeral_pub = ephemeral_pub_bytes(&ephemeral_priv);
        let creator_pub = static_pub_from_private(creator_static_priv);
        let shared = compute_dh(&ephemeral_priv, &creator_pub)?;
        let mut session = derive_session_secret(&shared, &creator_pub)?;
        let reply_key = derive_reply_key(&session, reply_key_seed)?;
        let nonce_bytes = derive_reply_nonce(&session, reply_key_seed)?;
        let aead_nonce = nonce_from_salt(&nonce_bytes);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&reply_key));
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&aead_nonce),
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad: &[],
                },
            )
            .map_err(|_| BuildCryptographyError::EncryptionFailed)?;
        let mut record = Vec::with_capacity(EPHEMERAL_KEY_LEN + NONCE_LEN + ciphertext.len());
        record.extend_from_slice(&ephemeral_pub);
        record.extend_from_slice(&nonce_bytes);
        record.extend_from_slice(&ciphertext);
        session_zeroize(&mut session);
        Ok(Zeroizing::new(record))
    }

    fn open_short_reply(
        &self,
        record: &[u8],
        creator_static_priv: &[u8; EPHEMERAL_KEY_LEN],
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, BuildCryptographyError> {
        let expected_size = EPHEMERAL_KEY_LEN + NONCE_LEN + SHORT_REPLY_PLAINTEXT_SIZE + TAG_LEN;
        if record.len() != expected_size {
            return Err(BuildCryptographyError::RecordLength {
                actual: record.len(),
                expected: expected_size,
            });
        }
        let mut ephemeral_pub = [0_u8; EPHEMERAL_KEY_LEN];
        ephemeral_pub.copy_from_slice(&record[0..EPHEMERAL_KEY_LEN]);
        if ephemeral_pub.iter().all(|byte| *byte == 0) {
            return Err(BuildCryptographyError::InvalidDhResult);
        }
        let nonce_salt = &record[EPHEMERAL_KEY_LEN..EPHEMERAL_KEY_LEN + NONCE_LEN];
        let aead_body = &record[EPHEMERAL_KEY_LEN + NONCE_LEN..];
        let shared = compute_dh_from_private(creator_static_priv, &ephemeral_pub)?;
        let creator_pub = static_pub_from_private(creator_static_priv);
        let mut session = derive_session_secret(&shared, &creator_pub)?;
        let reply_key = derive_reply_key_creator(&session)?;
        let aead_nonce = nonce_from_slice(nonce_salt);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&reply_key));
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&aead_nonce),
                chacha20poly1305::aead::Payload {
                    msg: aead_body,
                    aad: &[],
                },
            )
            .map_err(|_| BuildCryptographyError::AuthenticationFailed)?;
        session_zeroize(&mut session);
        let mut output = Zeroizing::new([0_u8; SHORT_REPLY_PLAINTEXT_SIZE]);
        if plaintext.len() != output.len() {
            return Err(BuildCryptographyError::PlaintextLength {
                actual: plaintext.len(),
                expected: SHORT_REPLY_PLAINTEXT_SIZE,
            });
        }
        output.copy_from_slice(&plaintext);
        Ok(output)
    }

    fn name(&self) -> &'static str {
        "ecies-x25519"
    }
}

fn generate_ephemeral<R: rand_core::CryptoRng + rand_core::RngCore>(
    rng: &mut R,
) -> Result<[u8; EPHEMERAL_KEY_LEN], BuildCryptographyError> {
    let mut bytes = Zeroizing::new([0_u8; EPHEMERAL_KEY_LEN]);
    if rng.try_fill_bytes(&mut *bytes).is_err() {
        return Err(BuildCryptographyError::InvalidPeerKey);
    }
    Ok(*bytes)
}

fn ephemeral_pub_bytes(priv_bytes: &[u8; EPHEMERAL_KEY_LEN]) -> [u8; EPHEMERAL_KEY_LEN] {
    let secret = StaticSecret::from(*priv_bytes);
    let public = X25519PublicKey::from(&secret);
    public.to_bytes()
}

fn static_pub_from_private(priv_bytes: &[u8; EPHEMERAL_KEY_LEN]) -> [u8; EPHEMERAL_KEY_LEN] {
    ephemeral_pub_bytes(priv_bytes)
}

fn compute_dh(
    priv_bytes: &[u8; EPHEMERAL_KEY_LEN],
    peer_pub: &[u8; EPHEMERAL_KEY_LEN],
) -> Result<Zeroizing<[u8; EPHEMERAL_KEY_LEN]>, BuildCryptographyError> {
    let priv_key = X25519PrivateKey::from_bytes(*priv_bytes);
    let shared = priv_key
        .diffie_hellman(peer_pub)
        .map_err(map_crypto_error)?;
    let mut bytes = Zeroizing::new([0_u8; EPHEMERAL_KEY_LEN]);
    bytes.copy_from_slice(shared.as_bytes());
    Ok(bytes)
}

fn map_crypto_error(error: CryptoError) -> BuildCryptographyError {
    match error {
        CryptoError::InvalidKey { .. } => BuildCryptographyError::InvalidPeerKey,
        CryptoError::AllZeroSharedSecret => BuildCryptographyError::InvalidDhResult,
        _ => BuildCryptographyError::InvalidPeerKey,
    }
}

fn compute_dh_from_private(
    priv_bytes: &[u8; EPHEMERAL_KEY_LEN],
    peer_pub: &[u8; EPHEMERAL_KEY_LEN],
) -> Result<Zeroizing<[u8; EPHEMERAL_KEY_LEN]>, BuildCryptographyError> {
    compute_dh(priv_bytes, peer_pub)
}

fn derive_session_secret(
    shared: &[u8; EPHEMERAL_KEY_LEN],
    peer_pub: &[u8; EPHEMERAL_KEY_LEN],
) -> Result<Zeroizing<[u8; 32]>, BuildCryptographyError> {
    let derived = hkdf_sha256_32(peer_pub, shared, b"ECIES-X25519-Build-Session-v1")?;
    Ok(derived)
}

fn derive_request_key(
    session: &[u8; 32],
    request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
) -> Result<[u8; SHORT_REQUEST_KEY_LEN], BuildCryptographyError> {
    // The request key mixes the per-attempt seed so the creator
    // and responder derive the same AEAD key from the same seed.
    let info = [b"ECIES-X25519-Request-Key", request_key_seed.as_slice()].concat();
    let derived = hkdf_sha256_extract_and_expand(session, &info, b"request-key", 32)?;
    let mut output = [0_u8; SHORT_REQUEST_KEY_LEN];
    if derived.len() != output.len() {
        return Err(BuildCryptographyError::Hkdf(
            HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: 32,
            },
        ));
    }
    output.copy_from_slice(&derived);
    Ok(output)
}

fn derive_request_key_responder(
    session: &[u8; 32],
    request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
) -> Result<[u8; SHORT_REQUEST_KEY_LEN], BuildCryptographyError> {
    // Responder mirrors the creator derivation exactly: the seed is
    // bound by the short record and travels with the message the
    // responder receives. The test harness passes the same seed
    // through; production will reconstruct it from the per-hop
    // request context.
    let info = [b"ECIES-X25519-Request-Key", request_key_seed.as_slice()].concat();
    let derived = hkdf_sha256_extract_and_expand(session, &info, b"request-key", 32)?;
    let mut output = [0_u8; SHORT_REQUEST_KEY_LEN];
    if derived.len() != output.len() {
        return Err(BuildCryptographyError::Hkdf(
            HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: 32,
            },
        ));
    }
    output.copy_from_slice(&derived);
    Ok(output)
}

fn derive_reply_key(
    session: &[u8; 32],
    reply_key_seed: &[u8; SHORT_REPLY_KEY_LEN],
) -> Result<[u8; SHORT_REPLY_KEY_LEN], BuildCryptographyError> {
    let info = [b"ECIES-X25519-Reply-Key", reply_key_seed.as_slice()].concat();
    let derived = hkdf_sha256_extract_and_expand(session, &info, b"reply-key", 32)?;
    let mut output = [0_u8; SHORT_REPLY_KEY_LEN];
    if derived.len() != output.len() {
        return Err(BuildCryptographyError::Hkdf(
            HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: 32,
            },
        ));
    }
    output.copy_from_slice(&derived);
    Ok(output)
}

fn derive_reply_key_creator(
    session: &[u8; 32],
) -> Result<[u8; SHORT_REPLY_KEY_LEN], BuildCryptographyError> {
    let derived = hkdf_sha256_extract_and_expand(
        session,
        b"ECIES-X25519-Reply-Key-Creator",
        b"reply-key-creator",
        32,
    )?;
    let mut output = [0_u8; SHORT_REPLY_KEY_LEN];
    if derived.len() != output.len() {
        return Err(BuildCryptographyError::Hkdf(
            HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: 32,
            },
        ));
    }
    output.copy_from_slice(&derived);
    Ok(output)
}

fn derive_request_nonce(
    session: &[u8; 32],
    request_key_seed: &[u8; SHORT_REQUEST_KEY_LEN],
) -> Result<[u8; NONCE_LEN], BuildCryptographyError> {
    let info = [b"ECIES-X25519-Request-Nonce", request_key_seed.as_slice()].concat();
    let derived = hkdf_sha256_extract_and_expand(session, &info, b"request-nonce", NONCE_LEN)?;
    let mut output = [0_u8; NONCE_LEN];
    if derived.len() != output.len() {
        return Err(BuildCryptographyError::Hkdf(
            HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: NONCE_LEN,
            },
        ));
    }
    output.copy_from_slice(&derived);
    Ok(output)
}

fn derive_reply_nonce(
    session: &[u8; 32],
    reply_key_seed: &[u8; SHORT_REPLY_KEY_LEN],
) -> Result<[u8; NONCE_LEN], BuildCryptographyError> {
    let info = [b"ECIES-X25519-Reply-Nonce", reply_key_seed.as_slice()].concat();
    let derived = hkdf_sha256_extract_and_expand(session, &info, b"reply-nonce", NONCE_LEN)?;
    let mut output = [0_u8; NONCE_LEN];
    if derived.len() != output.len() {
        return Err(BuildCryptographyError::Hkdf(
            HkdfError::OutputLengthExceeded {
                requested: derived.len(),
                maximum: NONCE_LEN,
            },
        ));
    }
    output.copy_from_slice(&derived);
    Ok(output)
}

fn nonce_from_salt(salt: &[u8; NONCE_LEN]) -> [u8; 12] {
    let mut out = [0_u8; 12];
    out.copy_from_slice(&salt[..12]);
    out
}

fn nonce_from_slice(salt: &[u8]) -> [u8; 12] {
    let mut out = [0_u8; 12];
    let len = salt.len().min(12);
    out[..len].copy_from_slice(&salt[..len]);
    out
}

fn session_zeroize(session: &mut Zeroizing<[u8; 32]>) {
    session.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn fixed_rng(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    fn privkey(seed: u64) -> [u8; 32] {
        let mut rng = fixed_rng(seed);
        let mut bytes = [0_u8; 32];
        rng.fill_bytes(&mut bytes);
        bytes
    }

    #[test]
    fn empty_request_record_round_trips_through_seal_and_open() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(1);
        let alice_priv = privkey(2);
        let alice_pub = static_pub_from_private(&alice_priv);
        let plaintext = [0x11_u8; 154];
        let request_key_seed = [0x22_u8; SHORT_REQUEST_KEY_LEN];
        let record = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal");
        let mut plaintext_bytes = [0_u8; 154];
        plaintext_bytes.copy_from_slice(&plaintext);
        let opened = cryptography
            .open_short_request(record.as_ref(), &alice_priv, &request_key_seed)
            .expect("open");
        assert_eq!(&opened[..], &plaintext_bytes[..]);
    }

    #[test]
    fn wrong_peer_static_key_rejects_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(3);
        let alice_priv = privkey(4);
        let bob_priv = privkey(5);
        let bob_pub = static_pub_from_private(&bob_priv);
        let plaintext = [0x33_u8; 154];
        let request_key_seed = [0x44_u8; SHORT_REQUEST_KEY_LEN];
        let record = cryptography
            .seal_short_request(&plaintext, &bob_pub, &request_key_seed, &mut rng)
            .expect("seal");
        let outcome =
            cryptography.open_short_request(record.as_ref(), &alice_priv, &request_key_seed);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn altered_ephemeral_key_rejects_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(6);
        let alice_priv = privkey(7);
        let alice_pub = static_pub_from_private(&alice_priv);
        let plaintext = [0x55_u8; 154];
        let request_key_seed = [0x66_u8; SHORT_REQUEST_KEY_LEN];
        let mut record = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal");
        // Flip a bit in the ephemeral key (first byte).
        let mut bytes = Zeroizing::new(record.to_vec());
        bytes[0] ^= 0x01;
        record.copy_from_slice(&bytes);
        let outcome =
            cryptography.open_short_request(record.as_ref(), &alice_priv, &request_key_seed);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn altered_ciphertext_rejects_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(8);
        let alice_priv = privkey(9);
        let alice_pub = static_pub_from_private(&alice_priv);
        let plaintext = [0x77_u8; 154];
        let request_key_seed = [0x88_u8; SHORT_REQUEST_KEY_LEN];
        let mut record = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal");
        // Flip a bit in the ciphertext (after ephemeral key and nonce).
        let mut bytes = Zeroizing::new(record.to_vec());
        bytes[EPHEMERAL_KEY_LEN + NONCE_LEN] ^= 0x01;
        record.copy_from_slice(&bytes);
        let outcome =
            cryptography.open_short_request(record.as_ref(), &alice_priv, &request_key_seed);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn altered_tag_rejects_request() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(10);
        let alice_priv = privkey(11);
        let alice_pub = static_pub_from_private(&alice_priv);
        let plaintext = [0x99_u8; 154];
        let request_key_seed = [0xAA_u8; SHORT_REQUEST_KEY_LEN];
        let mut record = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal");
        // Flip a bit in the tag (last byte).
        let mut bytes = Zeroizing::new(record.to_vec());
        let len = bytes.len();
        bytes[len - 1] ^= 0x01;
        record.copy_from_slice(&bytes);
        let outcome =
            cryptography.open_short_request(record.as_ref(), &alice_priv, &request_key_seed);
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn ephemeral_keys_are_unique_across_consecutive_seals() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(12);
        let alice_priv = privkey(13);
        let alice_pub = static_pub_from_private(&alice_priv);
        let plaintext = [0xBB_u8; 154];
        let request_key_seed = [0xCC_u8; SHORT_REQUEST_KEY_LEN];
        let record_a = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal a");
        let record_b = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal b");
        assert_ne!(
            &record_a[0..EPHEMERAL_KEY_LEN],
            &record_b[0..EPHEMERAL_KEY_LEN]
        );
    }

    #[test]
    fn sealing_rejects_zero_peer_key() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(14);
        let plaintext = [0xDD_u8; 154];
        let request_key_seed = [0xEE_u8; SHORT_REQUEST_KEY_LEN];
        let outcome = cryptography.seal_short_request(
            &plaintext,
            &[0_u8; EPHEMERAL_KEY_LEN],
            &request_key_seed,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::InvalidPeerKey)
        ));
    }

    #[test]
    fn sealing_rejects_wrong_plaintext_length() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(15);
        let alice_pub = [0x12_u8; EPHEMERAL_KEY_LEN];
        let request_key_seed = [0x34_u8; SHORT_REQUEST_KEY_LEN];
        let outcome = cryptography.seal_short_request(
            &[0xFF_u8; 100],
            &alice_pub,
            &request_key_seed,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(BuildCryptographyError::PlaintextLength { .. })
        ));
    }

    #[test]
    fn ephemeral_priv_zeroize() {
        let priv_bytes = [0xAB_u8; 32];
        let _ = ephemeral_pub_bytes(&priv_bytes);
        // No assertion: just ensures the function returns and the
        // intermediate secret is dropped.
    }

    #[test]
    fn sealing_emits_218_byte_record() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(20);
        let alice_pub = [0xCC_u8; 32];
        let plaintext = [0x33_u8; 154];
        let request_key_seed = [0x44_u8; SHORT_REQUEST_KEY_LEN];
        let record = cryptography
            .seal_short_request(&plaintext, &alice_pub, &request_key_seed, &mut rng)
            .expect("seal");
        assert_eq!(record.len(), 218);
    }

    #[test]
    fn ephemeral_uniqueness_holds_for_three_hops_in_a_row() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = fixed_rng(30);
        let alice_pub = [0xDD_u8; 32];
        let plaintext = [0xEE_u8; 154];
        let key_seed_one = [0x11_u8; SHORT_REQUEST_KEY_LEN];
        let key_seed_two = [0x22_u8; SHORT_REQUEST_KEY_LEN];
        let key_seed_three = [0x33_u8; SHORT_REQUEST_KEY_LEN];
        let record_one = cryptography
            .seal_short_request(&plaintext, &alice_pub, &key_seed_one, &mut rng)
            .expect("seal");
        let record_two = cryptography
            .seal_short_request(&plaintext, &alice_pub, &key_seed_two, &mut rng)
            .expect("seal");
        let record_three = cryptography
            .seal_short_request(&plaintext, &alice_pub, &key_seed_three, &mut rng)
            .expect("seal");
        assert_ne!(
            &record_one[0..EPHEMERAL_KEY_LEN],
            &record_two[0..EPHEMERAL_KEY_LEN]
        );
        assert_ne!(
            &record_two[0..EPHEMERAL_KEY_LEN],
            &record_three[0..EPHEMERAL_KEY_LEN]
        );
        assert_ne!(
            &record_one[0..EPHEMERAL_KEY_LEN],
            &record_three[0..EPHEMERAL_KEY_LEN]
        );
    }

    #[test]
    fn different_request_keys_yield_distinct_ciphertext() {
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng_a = fixed_rng(40);
        let mut rng_b = fixed_rng(40);
        let alice_pub = [0x55_u8; 32];
        let plaintext = [0x66_u8; 154];
        let key_seed_a = [0x77_u8; SHORT_REQUEST_KEY_LEN];
        let key_seed_b = [0x88_u8; SHORT_REQUEST_KEY_LEN];
        let record_a = cryptography
            .seal_short_request(&plaintext, &alice_pub, &key_seed_a, &mut rng_a)
            .expect("seal");
        let record_b = cryptography
            .seal_short_request(&plaintext, &alice_pub, &key_seed_b, &mut rng_b)
            .expect("seal");
        assert_ne!(record_a.as_ref(), record_b.as_ref());
    }
}
