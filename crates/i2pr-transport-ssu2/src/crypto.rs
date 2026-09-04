//! SSU2 v2 Noise XK handshake cryptography (runtime-neutral).
//!
//! This module implements the exact SSU2-specific Noise XK composition
//! over X25519 / ChaCha20-Poly1305 / SHA-256 recorded in
//! [`crate::constants::NOISE_PROTOCOL_NAME`]:
//! `Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256`.
//!
//! Normative traceability: SSU2 specification (I2P website commit
//! `88596022920bdf99f27db27688faf4f204792fcd`, accurate for 0.9.69)
//! sections Noise Protocol Framework, KDF for Session Request, KDF for
//! Session Created and Session Confirmed part 1, KDF for Session
//! Confirmed part 2, KDF for Data Phase, KDF for Retry, KDF for Token
//! Request, and Header Encryption KDF. Proposal 159/165 are
//! historical/design context only.
//!
//! Spec-notation rules applied throughout (the specification uses
//! inclusive end indices):
//!
//! - `keydata[0:31]` / `keydata[32:63]` denote the 32-byte halves
//!   `keydata[0..32]` / `keydata[32..64]`.
//! - `packet[len-24:len-13]` / `packet[len-12:len-1]` denote the
//!   12-byte IV windows `packet[len-24..len-12]` /
//!   `packet[len-12..len]`.
//!
//! Two recorded interpretation notes where the specification text is
//! internally inconsistent:
//!
//! - The SessionRequest/SessionCreated raw-contents sections annotate
//!   the 48-byte header-tail/ephemeral region with `n: 1`. Plan 156
//!   read that annotation as conflicting with the Header Encryption KDF
//!   pseudocode and started the ChaCha20 stream at block counter 0.
//!   Plan 161 independent interop proved that reading wrong: the
//!   exact-pinned i2pd 2.61.0 reference starts the header-protection
//!   stream at block counter 1 (its ChaCha20 primitive hard-codes
//!   `iv[0] = 1`), and with counter 0 every protected header from i2pr
//!   decodes as garbage on the independent side. The `n: 1` annotation
//!   is the initial block counter for every header-protection
//!   ChaCha20 call (both 8-byte masks and the header-tail/ephemeral
//!   stream, handshake and data phase alike). This implementation now
//!   seeks to keystream byte 64 (the first byte of block 1) before
//!   applying keystream; the corrected behavior
//!   is pinned by the regenerated header-protection vectors and the
//!   Plan 161 loopback interop suite.
//! - The retransmission prose mentions SessionConfirmed packet number
//!   1, while the header-layout section mandates all-zero packet
//!   numbers for SessionConfirmed. The header layout governs here
//!   (see `header.rs`); retransmissions resend identical bytes.
//!
//! X25519, ChaCha20-Poly1305, ChaCha20, SHA-256, and HMAC-SHA256 come
//! from reviewed crates (directly or through `i2pr-crypto`); this
//! module only sequences them, owns bounded state, and enforces role,
//! stage, and nonce transitions. No sockets, timers, tasks, or
//! wall-clock reads occur here.

use chacha20::ChaCha20;
use chacha20::cipher::{KeyInit, KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, aead::Aead};
use hmac::{Hmac, Mac};
use i2pr_crypto::{X25519SharedSecret, constant_time_eq, hkdf_sha256_extract_and_expand};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroize;

use crate::constants;

type HmacSha256 = Hmac<Sha256>;

/// All-zero 12-byte ChaCha20 nonce used for the header-tail/ephemeral
/// obfuscation region (Header Encryption KDF: `iv = {0 * 12}`).
const ZERO_NONCE_12: [u8; 12] = [0_u8; 12];
/// Empty input keying material (`ZEROLEN`) for labeled derivations.
const ZERO_IKM: [u8; 0] = [];

/// Typed failures from SSU2 handshake cryptographic operations.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum Ssu2CryptoError {
    /// A public value had an invalid exact length or representation.
    #[error("invalid SSU2 public key")]
    InvalidPublicKey,
    /// A supplied field exceeded its protocol maximum.
    #[error("SSU2 field exceeds its bounded maximum")]
    FieldTooLarge,
    /// A nonce would exceed the last permitted counter value.
    #[error("SSU2 nonce counter exhausted")]
    NonceExhausted,
    /// The authenticated ciphertext or associated data was invalid.
    #[error("SSU2 authentication failed")]
    AuthenticationFailed,
    /// A consuming transcript operation was called in the wrong state.
    #[error("SSU2 transcript operation is not valid in this state")]
    InvalidState,
    /// A role-specific transcript operation was called by the wrong role.
    #[error("SSU2 transcript operation is invalid for this role")]
    WrongRole,
    /// The static key revealed in SessionConfirmed did not match the bound key.
    #[error("SSU2 peer static key mismatch")]
    PeerStaticMismatch,
    /// A KDF input could not be represented by the selected HMAC wrapper.
    #[error("SSU2 KDF input rejected")]
    KdfInput,
    /// An underlying protocol-crypto wrapper rejected its input.
    #[error("SSU2 protocol crypto wrapper rejected input")]
    WrapperRejected,
}

/// Explicit Noise role used for transcript gating and key assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Alice: sends TokenRequest/SessionRequest and SessionConfirmed.
    Initiator,
    /// Bob: sends Retry/SessionCreated.
    Responder,
}

/// A validated 32-byte SSU2 public key (static, intro-derived, or
/// ephemeral). The all-zero encoding is rejected; DH-level validation
/// (all-zero shared secret) is enforced by `i2pr-crypto` at DH time.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Ssu2PublicKey([u8; constants::KEY_LENGTH]);

impl Ssu2PublicKey {
    /// Constructs a public key and rejects the all-zero encoding.
    pub fn new(bytes: [u8; constants::KEY_LENGTH]) -> Result<Self, Ssu2CryptoError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(Ssu2CryptoError::InvalidPublicKey);
        }
        Ok(Self(bytes))
    }

    /// Constructs a public key for deterministic test vectors.
    pub const fn from_bytes_for_test(bytes: [u8; constants::KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the little-endian wire representation.
    pub const fn as_bytes(&self) -> &[u8; constants::KEY_LENGTH] {
        &self.0
    }
}

impl core::fmt::Debug for Ssu2PublicKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_tuple("Ssu2PublicKey")
            .field(&"<redacted>")
            .finish()
    }
}

/// A 32-byte SSU2 introduction key (published in RouterInfo address
/// options; DPI-obfuscation only, not a session secret).
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct IntroKey([u8; constants::KEY_LENGTH]);

impl IntroKey {
    /// Wraps the 32-byte intro key bytes.
    pub const fn new(bytes: [u8; constants::KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw key bytes for one header-protection operation.
    pub const fn as_bytes(&self) -> &[u8; constants::KEY_LENGTH] {
        &self.0
    }
}

impl core::fmt::Debug for IntroKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_tuple("IntroKey")
            .field(&"<redacted>")
            .finish()
    }
}

/// Transcript hash material: public evidence, not a session secret.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptHash([u8; HASH_LENGTH]);

/// SHA-256 output length used for transcript and hash helpers.
const HASH_LENGTH: usize = 32;

impl TranscriptHash {
    /// Borrows the exact SHA-256 digest bytes.
    pub const fn as_bytes(&self) -> &[u8; HASH_LENGTH] {
        &self.0
    }
}

/// A zeroizing Noise chaining-key owner.
#[derive(Zeroize)]
#[zeroize(drop)]
struct ChainKey([u8; HASH_LENGTH]);

/// A zeroizing handshake AEAD key owner (one Noise `MixKey` output).
#[derive(Zeroize)]
#[zeroize(drop)]
struct HandshakeAeadKey([u8; constants::KEY_LENGTH]);

/// A zeroizing data-phase AEAD key owner.
#[derive(Zeroize)]
#[zeroize(drop)]
struct DataAeadKey([u8; constants::KEY_LENGTH]);

/// A checked 64-bit nonce counter. The forbidden `2^64 - 1` value is
/// never emitted (spec: maximum `2^64 - 2`).
struct NonceCounter(u64);

impl NonceCounter {
    const fn new() -> Self {
        Self(0)
    }

    /// Returns the 12-byte AEAD nonce for `counter` without advancing.
    fn nonce_bytes(counter: u64) -> Result<[u8; constants::NONCE_LENGTH], Ssu2CryptoError> {
        if counter > constants::MAX_PACKET_NUMBER {
            return Err(Ssu2CryptoError::NonceExhausted);
        }
        let mut nonce = [0_u8; constants::NONCE_LENGTH];
        nonce[4..].copy_from_slice(&counter.to_le_bytes());
        Ok(nonce)
    }

    fn take_nonce(&mut self) -> Result<[u8; constants::NONCE_LENGTH], Ssu2CryptoError> {
        let counter = self.0;
        let nonce = Self::nonce_bytes(counter)?;
        self.0 = counter
            .checked_add(1)
            .ok_or(Ssu2CryptoError::NonceExhausted)?;
        Ok(nonce)
    }
}

/// One handshake AEAD cipher state (a single Noise `k`/`n` pair).
struct HandshakeCipher {
    key: HandshakeAeadKey,
    nonce: NonceCounter,
}

impl HandshakeCipher {
    fn new(key: HandshakeAeadKey) -> Self {
        Self {
            key,
            nonce: NonceCounter::new(),
        }
    }

    fn seal(
        &mut self,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, Ssu2CryptoError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key.0));
        let nonce_bytes = self.nonce.take_nonce()?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher
            .encrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad: associated_data,
                },
            )
            .map_err(|_| Ssu2CryptoError::FieldTooLarge)
    }

    fn open(
        &mut self,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, Ssu2CryptoError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key.0));
        let nonce_bytes = self.nonce.take_nonce()?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher
            .decrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: ciphertext,
                    aad: associated_data,
                },
            )
            .map_err(|_| Ssu2CryptoError::AuthenticationFailed)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TranscriptStage {
    Initial,
    RequestSealed,
    RequestAccepted,
    CreatedSealed,
    CreatedAccepted,
    StaticSealed,
    StaticAccepted,
    Confirmed,
}

/// A consuming SSU2 Noise XK transcript for the three handshake messages.
///
/// The transcript owns hash/chain/cipher sequencing only. Diffie-Hellman
/// inputs arrive as checked [`X25519SharedSecret`] values computed by the
/// caller (which owns the private keys); this keeps private-key ownership
/// at the composition boundary and this module free of RNG access.
pub struct Ssu2Transcript {
    role: Role,
    peer_static: Ssu2PublicKey,
    hash: TranscriptHash,
    chaining_key: ChainKey,
    /// The `ee` cipher, retained after SessionCreated is sealed or
    /// accepted for the SessionConfirmed static-key frame (the
    /// post-`ee` key at `n = 1`).
    static_cipher: Option<HandshakeCipher>,
    cipher: Option<HandshakeCipher>,
    stage: TranscriptStage,
}

impl Ssu2Transcript {
    /// Initializes the symmetric state and binds the responder static key:
    /// `h = SHA256(protocol_name)`; `ck = h`; `h = SHA256(h)`
    /// (MixHash of the null prologue); `h = SHA256(h || bpk)`.
    pub fn new(role: Role, responder_static: Ssu2PublicKey) -> Self {
        let protocol_hash = protocol_name_hash();
        let hash = sha256_concat(&protocol_hash, &[]);
        let hash = sha256_concat(&hash, responder_static.as_bytes());
        Self {
            role,
            peer_static: responder_static,
            hash: TranscriptHash(hash),
            chaining_key: ChainKey(protocol_hash),
            static_cipher: None,
            cipher: None,
            stage: TranscriptStage::Initial,
        }
    }

    /// Returns the role selected at construction.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Returns the responder static key bound into this transcript.
    pub const fn responder_static(&self) -> Ssu2PublicKey {
        self.peer_static
    }

    /// Returns the current transcript hash for deterministic evidence.
    #[doc(hidden)]
    pub const fn evidence_hash(&self) -> TranscriptHash {
        self.hash
    }

    /// Returns the current chaining key for deterministic evidence.
    #[doc(hidden)]
    pub fn evidence_chain_key(&self) -> [u8; HASH_LENGTH] {
        self.chaining_key.0
    }

    /// Mixes one public byte region into the transcript hash.
    fn mix_hash(&mut self, bytes: &[u8]) {
        self.hash = TranscriptHash(sha256_concat(&self.hash.0, bytes));
    }

    /// Performs the Noise `MixKey` step: `HKDF(ck, input)` splits into
    /// the next chaining key and the next cipher key (nonce reset).
    fn mix_key(&mut self, shared_secret: X25519SharedSecret) -> Result<(), Ssu2CryptoError> {
        let temp_key = hmac_sha256(&self.chaining_key.0, shared_secret.as_bytes())?;
        let next_chain = hmac_sha256(&temp_key, &[1])?;
        let mut cipher_input = [0_u8; HASH_LENGTH + 1];
        cipher_input[..HASH_LENGTH].copy_from_slice(&next_chain);
        cipher_input[HASH_LENGTH] = 2;
        let cipher_key = hmac_sha256(&temp_key, &cipher_input)?;
        self.chaining_key.0 = next_chain;
        let mut key = [0_u8; constants::KEY_LENGTH];
        key.copy_from_slice(&cipher_key);
        self.cipher = Some(HandshakeCipher::new(HandshakeAeadKey(key)));
        Ok(())
    }

    fn encrypt_and_hash(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Ssu2CryptoError> {
        let cipher = self.cipher.as_mut().ok_or(Ssu2CryptoError::InvalidState)?;
        let ciphertext = cipher.seal(plaintext, &self.hash.0)?;
        self.mix_hash(&ciphertext);
        Ok(ciphertext)
    }

    fn decrypt_and_hash(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, Ssu2CryptoError> {
        let cipher = self.cipher.as_mut().ok_or(Ssu2CryptoError::InvalidState)?;
        let plaintext = cipher.open(ciphertext, &self.hash.0)?;
        self.mix_hash(ciphertext);
        Ok(plaintext)
    }

    /// Initiator: seals SessionRequest (`e, es`). Mixes the cleartext
    /// long header, the ephemeral key, the `es` secret, and encrypts the
    /// payload with `n = 0` and `ad = h`.
    pub fn seal_session_request(
        mut self,
        header: &[u8; constants::LONG_HEADER_LENGTH],
        ephemeral_public: Ssu2PublicKey,
        es_secret: X25519SharedSecret,
        payload: &[u8],
    ) -> Result<(Self, Vec<u8>), Ssu2CryptoError> {
        if self.role != Role::Initiator {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::Initial {
            return Err(Ssu2CryptoError::InvalidState);
        }
        check_payload_bounds(payload)?;
        self.mix_hash(header);
        self.mix_hash(ephemeral_public.as_bytes());
        self.mix_key(es_secret)?;
        let ciphertext = self.encrypt_and_hash(payload)?;
        self.stage = TranscriptStage::RequestSealed;
        Ok((self, ciphertext))
    }

    /// Responder: accepts SessionRequest (`e, es`).
    pub fn accept_session_request(
        mut self,
        header: &[u8; constants::LONG_HEADER_LENGTH],
        ephemeral_public: Ssu2PublicKey,
        es_secret: X25519SharedSecret,
        ciphertext: &[u8],
    ) -> Result<(Self, Vec<u8>), Ssu2CryptoError> {
        if self.role != Role::Responder {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::Initial {
            return Err(Ssu2CryptoError::InvalidState);
        }
        check_ciphertext_bounds(ciphertext)?;
        self.mix_hash(header);
        self.mix_hash(ephemeral_public.as_bytes());
        self.mix_key(es_secret)?;
        let plaintext = self.decrypt_and_hash(ciphertext)?;
        self.stage = TranscriptStage::RequestAccepted;
        Ok((self, plaintext))
    }

    /// Responder: seals SessionCreated (`e, ee`). Mixes the cleartext
    /// long header, the responder ephemeral key, the `ee` secret, and
    /// encrypts with `n = 0`, `ad = h`. The request ciphertext is already
    /// in `h` (mixed when the SessionRequest was sealed/accepted) and
    /// must not be mixed again. The `ee` cipher is retained for the
    /// SessionConfirmed static frame.
    pub fn seal_session_created(
        mut self,
        _request_ciphertext: &[u8],
        header: &[u8; constants::LONG_HEADER_LENGTH],
        ephemeral_public: Ssu2PublicKey,
        ee_secret: X25519SharedSecret,
        payload: &[u8],
    ) -> Result<(Self, Vec<u8>), Ssu2CryptoError> {
        if self.role != Role::Responder {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::RequestAccepted {
            return Err(Ssu2CryptoError::InvalidState);
        }
        check_payload_bounds(payload)?;
        self.mix_hash(header);
        self.mix_hash(ephemeral_public.as_bytes());
        self.mix_key(ee_secret)?;
        let ciphertext = self.encrypt_and_hash(payload)?;
        self.static_cipher = self.cipher.take();
        self.stage = TranscriptStage::CreatedSealed;
        Ok((self, ciphertext))
    }

    /// Initiator: accepts SessionCreated (`e, ee`). The request
    /// ciphertext is already in `h` (mixed when the SessionRequest was
    /// sealed) and must not be mixed again.
    pub fn accept_session_created(
        mut self,
        _request_ciphertext: &[u8],
        header: &[u8; constants::LONG_HEADER_LENGTH],
        ephemeral_public: Ssu2PublicKey,
        ee_secret: X25519SharedSecret,
        ciphertext: &[u8],
    ) -> Result<(Self, Vec<u8>), Ssu2CryptoError> {
        if self.role != Role::Initiator {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::RequestSealed {
            return Err(Ssu2CryptoError::InvalidState);
        }
        check_ciphertext_bounds(ciphertext)?;
        self.mix_hash(header);
        self.mix_hash(ephemeral_public.as_bytes());
        self.mix_key(ee_secret)?;
        let plaintext = self.decrypt_and_hash(ciphertext)?;
        self.static_cipher = self.cipher.take();
        self.stage = TranscriptStage::CreatedAccepted;
        Ok((self, plaintext))
    }

    /// Initiator: seals the SessionConfirmed static-key frame (`s`
    /// pattern). Mixes the cleartext first-fragment short header, then
    /// encrypts the 32-byte static public key with the retained `ee`
    /// cipher at `n = 1` and mixes the frame into `h`. The resulting
    /// `ad = h` authenticates the following payload.
    pub fn seal_confirmed_static(
        mut self,
        header: &[u8; constants::SHORT_HEADER_LENGTH],
        static_public: Ssu2PublicKey,
    ) -> Result<
        (
            Self,
            [u8; constants::KEY_LENGTH + constants::AUTH_TAG_LENGTH],
        ),
        Ssu2CryptoError,
    > {
        if self.role != Role::Initiator {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::CreatedAccepted {
            return Err(Ssu2CryptoError::InvalidState);
        }
        self.mix_hash(header);
        let cipher = self
            .static_cipher
            .as_mut()
            .ok_or(Ssu2CryptoError::InvalidState)?;
        let frame = cipher.seal(static_public.as_bytes(), &self.hash.0)?;
        self.mix_hash(&frame);
        let mut output = [0_u8; constants::KEY_LENGTH + constants::AUTH_TAG_LENGTH];
        if frame.len() != output.len() {
            return Err(Ssu2CryptoError::FieldTooLarge);
        }
        output.copy_from_slice(&frame);
        self.static_cipher = None;
        self.stage = TranscriptStage::StaticSealed;
        Ok((self, output))
    }

    /// Responder: opens the SessionConfirmed static-key frame with the
    /// retained `ee` cipher at `n = 1`, after mixing the cleartext
    /// first-fragment short header. The recovered key is explicitly
    /// unchecked here; the caller binds it via `se` and RouterInfo
    /// validation before exposing an authenticated peer.
    pub fn accept_confirmed_static(
        mut self,
        header: &[u8; constants::SHORT_HEADER_LENGTH],
        frame: &[u8],
    ) -> Result<(Self, Ssu2PublicKey), Ssu2CryptoError> {
        if self.role != Role::Responder {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::CreatedSealed {
            return Err(Ssu2CryptoError::InvalidState);
        }
        if frame.len() != constants::KEY_LENGTH + constants::AUTH_TAG_LENGTH {
            return Err(Ssu2CryptoError::AuthenticationFailed);
        }
        self.mix_hash(header);
        let cipher = self
            .static_cipher
            .as_mut()
            .ok_or(Ssu2CryptoError::InvalidState)?;
        let plaintext = cipher.open(frame, &self.hash.0)?;
        self.mix_hash(frame);
        let bytes: [u8; constants::KEY_LENGTH] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| Ssu2CryptoError::InvalidPublicKey)?;
        let static_public = Ssu2PublicKey::new(bytes)?;
        self.static_cipher = None;
        self.stage = TranscriptStage::StaticAccepted;
        Ok((self, static_public))
    }

    /// Initiator: seals the SessionConfirmed payload (`se` pattern).
    pub fn seal_confirmed_payload(
        mut self,
        se_secret: X25519SharedSecret,
        payload: &[u8],
    ) -> Result<(Self, Vec<u8>), Ssu2CryptoError> {
        if self.role != Role::Initiator {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::StaticSealed {
            return Err(Ssu2CryptoError::InvalidState);
        }
        check_payload_bounds(payload)?;
        self.mix_key(se_secret)?;
        let ciphertext = self.encrypt_and_hash(payload)?;
        self.stage = TranscriptStage::Confirmed;
        Ok((self, ciphertext))
    }

    /// Responder: opens the SessionConfirmed payload (`se` pattern).
    pub fn open_confirmed_payload(
        mut self,
        se_secret: X25519SharedSecret,
        ciphertext: &[u8],
    ) -> Result<(Self, Vec<u8>), Ssu2CryptoError> {
        if self.role != Role::Responder {
            return Err(Ssu2CryptoError::WrongRole);
        }
        if self.stage != TranscriptStage::StaticAccepted {
            return Err(Ssu2CryptoError::InvalidState);
        }
        check_ciphertext_bounds(ciphertext)?;
        self.mix_key(se_secret)?;
        let plaintext = self.decrypt_and_hash(ciphertext)?;
        self.stage = TranscriptStage::Confirmed;
        Ok((self, plaintext))
    }

    /// Consumes the handshake state and derives the directional
    /// data-phase key material per the specification KDF for data
    /// phase: `HKDF(ck, ZEROLEN, "", 64)` splits into `k_ab` (Alice to
    /// Bob) and `k_ba` (Bob to Alice); each directional key then
    /// expands via `HKDF(key, ZEROLEN, "HKDFSSU2DataKeys", 64)` into
    /// `(k_data, k_header_2)`. The AEAD cipher uses `k_data`; header
    /// protection uses `k_header_2` with `k_header_1` equal to the
    /// receiver's intro key (supplied by the Plan 157 session layer).
    pub fn split(self) -> Result<Ssu2SplitKeys, Ssu2CryptoError> {
        if self.stage != TranscriptStage::Confirmed {
            return Err(Ssu2CryptoError::InvalidState);
        }
        let keydata = hkdf_64(&self.chaining_key.0, &ZERO_IKM, &[])?;
        let (k_ab, k_ba) = split_halves(&keydata);
        let (tx_key, rx_key) = match self.role {
            Role::Initiator => (k_ab, k_ba),
            Role::Responder => (k_ba, k_ab),
        };
        let (tx_data, tx_header_2) = derive_data_keys(&tx_key)?;
        let (rx_data, rx_header_2) = derive_data_keys(&rx_key)?;
        Ok(Ssu2SplitKeys {
            transmit: DataDirectionKeys {
                cipher: DataCipher::new(DataAeadKey(tx_data)),
                header_key_2: tx_header_2,
            },
            receive: DataDirectionKeys {
                cipher: DataCipher::new(DataAeadKey(rx_data)),
                header_key_2: rx_header_2,
            },
        })
    }
}

/// One directional data-phase key pair: the AEAD cipher (`k_data`)
/// plus the second header-protection key (`k_header_2`). The first
/// header-protection key (`k_header_1`) is the receiver's intro key
/// and is supplied by the session layer, not the transcript.
pub struct DataDirectionKeys {
    cipher: DataCipher,
    header_key_2: [u8; constants::KEY_LENGTH],
}

impl DataDirectionKeys {
    /// Borrows the directional AEAD cipher.
    pub const fn cipher(&mut self) -> &mut DataCipher {
        &mut self.cipher
    }

    /// Returns the directional second header-protection key.
    pub const fn header_key_2(&self) -> &[u8; constants::KEY_LENGTH] {
        &self.header_key_2
    }

    /// Consumes the directional keys into an owned cipher plus the
    /// second header-protection key (crate-internal; the session layer
    /// owns protocol sequencing while key bytes never enter logs).
    pub(crate) fn into_owner(self) -> (DataCipher, [u8; constants::KEY_LENGTH]) {
        (self.cipher, self.header_key_2)
    }
}

/// Directional data-phase key states from the Noise `split()`, each
/// holding the derived `(k_data, k_header_2)` pair for its direction.
pub struct Ssu2SplitKeys {
    transmit: DataDirectionKeys,
    receive: DataDirectionKeys,
}

impl Ssu2SplitKeys {
    /// Borrows the transmit cipher state for one data-phase packet.
    ///
    /// The key is the derived `k_data` per the data-phase KDF.
    pub fn transmit(&mut self) -> &mut DataCipher {
        &mut self.transmit.cipher
    }

    /// Borrows the receive cipher state for one data-phase packet.
    pub fn receive(&mut self) -> &mut DataCipher {
        &mut self.receive.cipher
    }

    /// Borrows the transmit directional keys (cipher plus header key).
    pub const fn transmit_keys(&mut self) -> &mut DataDirectionKeys {
        &mut self.transmit
    }

    /// Borrows the receive directional keys (cipher plus header key).
    pub const fn receive_keys(&mut self) -> &mut DataDirectionKeys {
        &mut self.receive
    }

    /// Consumes the combined keys into independent directional parts.
    pub fn into_parts(self) -> (DataDirectionKeys, DataDirectionKeys) {
        (self.transmit, self.receive)
    }
}

/// One directional data-phase AEAD cipher (`k_data`). Packet-number
/// assignment and replay tracking belong to the `session` data phase;
/// this type only enforces the nonce ceiling and the header binding.
pub struct DataCipher {
    key: DataAeadKey,
}

impl DataCipher {
    fn new(key: DataAeadKey) -> Self {
        Self { key }
    }

    /// Seals one data-phase payload: `n` is the packet number,
    /// `ad` is the 16-byte short header before header protection.
    pub fn seal(
        &self,
        packet_number: u32,
        header: &[u8; constants::SHORT_HEADER_LENGTH],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, Ssu2CryptoError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key.0));
        let nonce_bytes = NonceCounter::nonce_bytes(u64::from(packet_number))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher
            .encrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad: header,
                },
            )
            .map_err(|_| Ssu2CryptoError::FieldTooLarge)
    }

    /// Opens one data-phase payload with the same binding.
    pub fn open(
        &self,
        packet_number: u32,
        header: &[u8; constants::SHORT_HEADER_LENGTH],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, Ssu2CryptoError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key.0));
        let nonce_bytes = NonceCounter::nonce_bytes(u64::from(packet_number))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher
            .decrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: ciphertext,
                    aad: header,
                },
            )
            .map_err(|_| Ssu2CryptoError::AuthenticationFailed)
    }
}

/// Seals a TokenRequest/Retry payload with the responder intro key:
/// `k = bik`, `n` is the header packet number, `ad` is the 32-byte
/// long header before header protection.
pub fn seal_token_payload(
    intro_key: &IntroKey,
    packet_number: u32,
    header: &[u8; constants::LONG_HEADER_LENGTH],
    plaintext: &[u8],
) -> Result<Vec<u8>, Ssu2CryptoError> {
    check_payload_bounds(plaintext)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(intro_key.as_bytes()));
    let nonce_bytes = NonceCounter::nonce_bytes(u64::from(packet_number))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .encrypt(
            nonce,
            chacha20poly1305::aead::Payload {
                msg: plaintext,
                aad: header,
            },
        )
        .map_err(|_| Ssu2CryptoError::FieldTooLarge)
}

/// Opens a TokenRequest/Retry payload with the responder intro key.
pub fn open_token_payload(
    intro_key: &IntroKey,
    packet_number: u32,
    header: &[u8; constants::LONG_HEADER_LENGTH],
    ciphertext: &[u8],
) -> Result<Vec<u8>, Ssu2CryptoError> {
    check_ciphertext_bounds(ciphertext)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(intro_key.as_bytes()));
    let nonce_bytes = NonceCounter::nonce_bytes(u64::from(packet_number))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(
            nonce,
            chacha20poly1305::aead::Payload {
                msg: ciphertext,
                aad: header,
            },
        )
        .map_err(|_| Ssu2CryptoError::AuthenticationFailed)
}

/// Derives the SessionCreated header-protection key:
/// `HKDF(chainKey, ZEROLEN, "SessCreateHeader", 32)`.
pub fn session_created_header_key(
    chaining_key: &[u8; HASH_LENGTH],
) -> Result<[u8; constants::KEY_LENGTH], Ssu2CryptoError> {
    labeled_32(chaining_key, constants::HEADER_LABEL_SESSION_CREATED)
}

/// Derives the SessionConfirmed header-protection key:
/// `HKDF(chainKey, ZEROLEN, "SessionConfirmed", 32)`.
pub fn session_confirmed_header_key(
    chaining_key: &[u8; HASH_LENGTH],
) -> Result<[u8; constants::KEY_LENGTH], Ssu2CryptoError> {
    labeled_32(chaining_key, constants::HEADER_LABEL_SESSION_CONFIRMED)
}

/// Derives data-phase keys from one directional key:
/// `HKDF(key, ZEROLEN, "HKDFSSU2DataKeys", 64)` splits into
/// `(k_data, k_header_2)`.
pub fn derive_data_keys(
    directional_key: &[u8; constants::KEY_LENGTH],
) -> Result<([u8; constants::KEY_LENGTH], [u8; constants::KEY_LENGTH]), Ssu2CryptoError> {
    let keydata = hkdf_64(directional_key, &ZERO_IKM, constants::DATA_KEY_LABEL)?;
    Ok(split_halves(&keydata))
}

/// Applies SSU2 header protection to one outbound datagram in place
/// (Header Encryption KDF, encryption direction).
///
/// `header_length` is 32 for long headers and 16 for short headers;
/// `protect_ephemeral_tail` selects the 48-byte third-part treatment
/// for SessionRequest/SessionCreated (header tail plus ephemeral key)
/// versus the 16-byte treatment for Retry/TokenRequest/PeerTest/
/// HolePunch. Short headers have no third part.
///
/// The datagram must already contain header, ephemeral key (if any),
/// payload ciphertext, and MAC; protection reads the last 24 bytes as
/// IV material, so datagrams shorter than 40 bytes are rejected.
pub fn apply_header_protection(
    datagram: &mut [u8],
    header_length: usize,
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
    protect_ephemeral_tail: bool,
) -> Result<(), Ssu2CryptoError> {
    check_protection_shape(datagram.len(), header_length, protect_ephemeral_tail)?;
    let length = datagram.len();
    let (head, tail) = datagram.split_at_mut(length - 24);
    let iv1: [u8; 12] = tail[..12]
        .try_into()
        .map_err(|_| Ssu2CryptoError::FieldTooLarge)?;
    let mask = chacha_mask(k_header_1, &iv1);
    for (byte, mask) in head[..8].iter_mut().zip(mask.iter()) {
        *byte ^= *mask;
    }
    let iv2: [u8; 12] = tail[12..24]
        .try_into()
        .map_err(|_| Ssu2CryptoError::FieldTooLarge)?;
    let mask = chacha_mask(k_header_2, &iv2);
    for (byte, mask) in head[8..16].iter_mut().zip(mask.iter()) {
        *byte ^= *mask;
    }
    chacha_stream(
        k_header_2,
        &ZERO_NONCE_12,
        &mut datagram[16..protection_tail_end(header_length, protect_ephemeral_tail)],
    );
    Ok(())
}

/// Removes SSU2 header protection from one inbound datagram in place.
/// ChaCha20/XOR protection is symmetric, so removal applies the same
/// operations in reverse order (third part first, then the masks, so
/// the MAC-derived IVs are read before any byte is modified).
pub fn remove_header_protection(
    datagram: &mut [u8],
    header_length: usize,
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
    protect_ephemeral_tail: bool,
) -> Result<(), Ssu2CryptoError> {
    check_protection_shape(datagram.len(), header_length, protect_ephemeral_tail)?;
    chacha_stream(
        k_header_2,
        &ZERO_NONCE_12,
        &mut datagram[16..protection_tail_end(header_length, protect_ephemeral_tail)],
    );
    let length = datagram.len();
    let iv2: [u8; 12] = datagram[length - 12..length]
        .try_into()
        .map_err(|_| Ssu2CryptoError::FieldTooLarge)?;
    let mask = chacha_mask(k_header_2, &iv2);
    for (byte, mask) in datagram[8..16].iter_mut().zip(mask.iter()) {
        *byte ^= *mask;
    }
    let iv1: [u8; 12] = datagram[length - 24..length - 12]
        .try_into()
        .map_err(|_| Ssu2CryptoError::FieldTooLarge)?;
    let mask = chacha_mask(k_header_1, &iv1);
    for (byte, mask) in datagram[..8].iter_mut().zip(mask.iter()) {
        *byte ^= *mask;
    }
    Ok(())
}

fn protection_tail_end(header_length: usize, protect_ephemeral_tail: bool) -> usize {
    if header_length == constants::LONG_HEADER_LENGTH {
        if protect_ephemeral_tail {
            16 + constants::HANDSHAKE_EPHEMERAL_LENGTH + 16
        } else {
            constants::LONG_HEADER_LENGTH
        }
    } else {
        constants::SHORT_HEADER_LENGTH
    }
}

fn check_protection_shape(
    datagram_length: usize,
    header_length: usize,
    protect_ephemeral_tail: bool,
) -> Result<(), Ssu2CryptoError> {
    if header_length != constants::LONG_HEADER_LENGTH
        && header_length != constants::SHORT_HEADER_LENGTH
    {
        return Err(Ssu2CryptoError::FieldTooLarge);
    }
    if protect_ephemeral_tail && header_length != constants::LONG_HEADER_LENGTH {
        return Err(Ssu2CryptoError::FieldTooLarge);
    }
    if datagram_length < constants::MIN_DATAGRAM_LENGTH {
        return Err(Ssu2CryptoError::FieldTooLarge);
    }
    let tail_end = protection_tail_end(header_length, protect_ephemeral_tail);
    if datagram_length
        < tail_end + constants::MIN_HANDSHAKE_PAYLOAD_BYTES + constants::AUTH_TAG_LENGTH
    {
        return Err(Ssu2CryptoError::FieldTooLarge);
    }
    Ok(())
}

/// Computes one 8-byte header-protection mask: the ChaCha20 keystream
/// starting at block counter 1 (spec `n: 1`; independent reference
/// hard-codes the counter word to 1). The seek position is byte 64,
/// i.e. the first byte of keystream block 1.
pub(crate) fn chacha_mask(key: &[u8; constants::KEY_LENGTH], iv: &[u8; 12]) -> [u8; 8] {
    let mut cipher = ChaCha20::new(key.into(), iv.into());
    cipher.seek(64u32);
    let mut mask = [0_u8; 8];
    cipher.apply_keystream(&mut mask);
    mask
}

fn chacha_stream(key: &[u8; constants::KEY_LENGTH], nonce: &[u8; 12], data: &mut [u8]) {
    let mut cipher = ChaCha20::new(key.into(), nonce.into());
    cipher.seek(64u32);
    cipher.apply_keystream(data);
}

/// Returns the initial protocol hash `SHA256(protocol_name)`, which is
/// also the initial chaining key.
pub fn protocol_initial_hash() -> TranscriptHash {
    TranscriptHash(protocol_name_hash())
}

/// Returns the full initial transcript hash after binding the responder
/// static key: `SHA256(SHA256(SHA256(protocol_name) || bpk))` with the
/// null-prologue mix in between.
pub fn initial_transcript_hash(responder_static: &Ssu2PublicKey) -> TranscriptHash {
    let protocol_hash = protocol_name_hash();
    let hash = sha256_concat(&protocol_hash, &[]);
    TranscriptHash(sha256_concat(&hash, responder_static.as_bytes()))
}

fn protocol_name_hash() -> [u8; HASH_LENGTH] {
    let mut hasher = Sha256::new();
    hasher.update(constants::NOISE_PROTOCOL_NAME);
    hasher.finalize().into()
}

fn sha256_concat(left: &[u8; HASH_LENGTH], right: &[u8]) -> [u8; HASH_LENGTH] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<[u8; HASH_LENGTH], Ssu2CryptoError> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(key).map_err(|_| Ssu2CryptoError::KdfInput)?;
    mac.update(data);
    let result = mac.finalize().into_bytes();
    result
        .as_slice()
        .try_into()
        .map_err(|_| Ssu2CryptoError::KdfInput)
}

fn hkdf_64(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<[u8; 64], Ssu2CryptoError> {
    let derived = hkdf_sha256_extract_and_expand(salt, ikm, info, 64)
        .map_err(|_| Ssu2CryptoError::KdfInput)?;
    derived
        .as_slice()
        .try_into()
        .map_err(|_| Ssu2CryptoError::KdfInput)
}

fn split_halves(keydata: &[u8; 64]) -> ([u8; 32], [u8; 32]) {
    let mut first = [0_u8; 32];
    let mut second = [0_u8; 32];
    first.copy_from_slice(&keydata[..32]);
    second.copy_from_slice(&keydata[32..]);
    (first, second)
}

fn labeled_32(
    chaining_key: &[u8; HASH_LENGTH],
    label: &[u8],
) -> Result<[u8; constants::KEY_LENGTH], Ssu2CryptoError> {
    let derived = hkdf_sha256_extract_and_expand(chaining_key, &ZERO_IKM, label, 32)
        .map_err(|_| Ssu2CryptoError::KdfInput)?;
    derived
        .as_slice()
        .try_into()
        .map_err(|_| Ssu2CryptoError::KdfInput)
}

fn check_payload_bounds(payload: &[u8]) -> Result<(), Ssu2CryptoError> {
    if payload.len() < constants::MIN_HANDSHAKE_PAYLOAD_BYTES
        || payload.len() > constants::MAX_HANDSHAKE_PAYLOAD_BYTES
    {
        return Err(Ssu2CryptoError::FieldTooLarge);
    }
    Ok(())
}

fn check_ciphertext_bounds(ciphertext: &[u8]) -> Result<(), Ssu2CryptoError> {
    if ciphertext.len() < constants::MIN_HANDSHAKE_PAYLOAD_BYTES + constants::AUTH_TAG_LENGTH
        || ciphertext.len() > constants::MAX_HANDSHAKE_PAYLOAD_BYTES + constants::AUTH_TAG_LENGTH
    {
        return Err(Ssu2CryptoError::FieldTooLarge);
    }
    Ok(())
}

/// Compares two public values without early exit on equal-length bytes.
pub fn public_eq(left: &[u8], right: &[u8]) -> bool {
    constant_time_eq(left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::X25519PrivateKey;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn test_keys(seed: u64) -> (X25519PrivateKey, Ssu2PublicKey) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let secret = X25519PrivateKey::generate(&mut rng).expect("deterministic secret");
        let public = Ssu2PublicKey::new(secret.public_bytes()).expect("public");
        (secret, public)
    }

    #[test]
    fn public_key_rejects_all_zero_encoding() {
        assert_eq!(
            Ssu2PublicKey::new([0_u8; 32]),
            Err(Ssu2CryptoError::InvalidPublicKey)
        );
        assert!(Ssu2PublicKey::new([9_u8; 32]).is_ok());
    }

    #[test]
    fn protocol_name_hash_matches_spec_identifier() {
        assert_eq!(constants::NOISE_PROTOCOL_NAME_LENGTH, 52);
        let initial = protocol_initial_hash();
        let mut hasher = Sha256::new();
        hasher.update(b"Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256");
        let expected: [u8; 32] = hasher.finalize().into();
        assert_eq!(initial.as_bytes(), &expected);
    }

    #[test]
    fn full_transcript_reaches_matching_split_keys() {
        let (alice_static, alice_public) = test_keys(1);
        let (bob_static, bob_public) = test_keys(2);
        let (alice_eph, alice_eph_public) = test_keys(3);
        let (bob_eph, bob_eph_public) = test_keys(4);

        let request_header = [0x11_u8; constants::LONG_HEADER_LENGTH];
        let created_header = [0x22_u8; constants::LONG_HEADER_LENGTH];
        let request_payload = [0x31_u8; 32];
        let created_payload = [0x32_u8; 48];

        let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
        let bob = Ssu2Transcript::new(Role::Responder, bob_public);

        let es_alice = X25519PrivateKey::from_bytes(*alice_eph.secret_bytes())
            .diffie_hellman(bob_public.as_bytes())
            .expect("es");
        let es_bob = X25519PrivateKey::from_bytes(*bob_static.secret_bytes())
            .diffie_hellman(alice_eph_public.as_bytes())
            .expect("es");
        let (alice, request_ct) = alice
            .seal_session_request(
                &request_header,
                alice_eph_public,
                es_alice,
                &request_payload,
            )
            .expect("seal request");
        let (bob, opened_request) = bob
            .accept_session_request(&request_header, alice_eph_public, es_bob, &request_ct)
            .expect("accept request");
        assert_eq!(opened_request, request_payload);

        let ee_bob = X25519PrivateKey::from_bytes(*bob_eph.secret_bytes())
            .diffie_hellman(alice_eph_public.as_bytes())
            .expect("ee");
        let ee_alice = X25519PrivateKey::from_bytes(*alice_eph.secret_bytes())
            .diffie_hellman(bob_eph_public.as_bytes())
            .expect("ee");
        let (bob, created_ct) = bob
            .seal_session_created(
                &request_ct,
                &created_header,
                bob_eph_public,
                ee_bob,
                &created_payload,
            )
            .expect("seal created");
        let (alice, opened_created) = alice
            .accept_session_created(
                &request_ct,
                &created_header,
                bob_eph_public,
                ee_alice,
                &created_ct,
            )
            .expect("accept created");
        assert_eq!(opened_created, created_payload);

        let confirmed_header = [0x33_u8; constants::SHORT_HEADER_LENGTH];
        let (alice, static_frame) = alice
            .seal_confirmed_static(&confirmed_header, alice_public)
            .expect("static");
        let (bob, recovered) = bob
            .accept_confirmed_static(&confirmed_header, &static_frame)
            .expect("open static");
        assert_eq!(recovered, alice_public);

        let confirmed_payload = [0x33_u8; 64];
        let se_alice = X25519PrivateKey::from_bytes(*alice_static.secret_bytes())
            .diffie_hellman(bob_eph_public.as_bytes())
            .expect("se");
        let se_bob = X25519PrivateKey::from_bytes(*bob_eph.secret_bytes())
            .diffie_hellman(alice_public.as_bytes())
            .expect("se");
        let (alice, confirmed_ct) = alice
            .seal_confirmed_payload(se_alice, &confirmed_payload)
            .expect("seal confirmed");
        let (bob, opened_confirmed) = bob
            .open_confirmed_payload(se_bob, &confirmed_ct)
            .expect("open confirmed");
        assert_eq!(opened_confirmed, confirmed_payload);

        let mut alice_keys = alice.split().expect("alice split");
        let mut bob_keys = bob.split().expect("bob split");
        let probe = [0x44_u8; 24];
        let header = [0x55_u8; constants::SHORT_HEADER_LENGTH];
        let sealed = alice_keys
            .transmit()
            .seal(7, &header, &probe)
            .expect("seal");
        let opened = bob_keys.receive().open(7, &header, &sealed).expect("open");
        assert_eq!(opened, probe);
        let sealed_back = bob_keys.transmit().seal(9, &header, &probe).expect("seal");
        let opened_back = alice_keys
            .receive()
            .open(9, &header, &sealed_back)
            .expect("open");
        assert_eq!(opened_back, probe);
    }

    #[test]
    fn transcript_enforces_roles_and_stages() {
        let (_, bob_public) = test_keys(21);
        let (_, eph_public) = test_keys(22);
        let (eph, _) = test_keys(23);
        let es = eph.diffie_hellman(bob_public.as_bytes()).expect("es");
        let header = [0_u8; constants::LONG_HEADER_LENGTH];

        let responder = Ssu2Transcript::new(Role::Responder, bob_public);
        assert_eq!(
            responder
                .seal_session_request(&header, eph_public, es, &[1_u8; 8])
                .map(|_| ()),
            Err(Ssu2CryptoError::WrongRole)
        );

        let initiator = Ssu2Transcript::new(Role::Initiator, bob_public);
        let (eph2, _) = test_keys(24);
        let es2 = eph2.diffie_hellman(bob_public.as_bytes()).expect("es");
        assert_eq!(
            initiator
                .accept_session_request(&header, eph_public, es2, &[1_u8; 24])
                .map(|_| ()),
            Err(Ssu2CryptoError::WrongRole)
        );

        let initiator = Ssu2Transcript::new(Role::Initiator, bob_public);
        assert_eq!(
            initiator.split().map(|_| ()),
            Err(Ssu2CryptoError::InvalidState)
        );
    }

    #[test]
    fn tag_mutation_and_wrong_key_fail_authentication() {
        let (alice_static, _) = test_keys(31);
        let (bob_static, bob_public) = test_keys(32);
        let (alice_eph, alice_eph_public) = test_keys(33);
        let header = [0x77_u8; constants::LONG_HEADER_LENGTH];
        let payload = [0x5a_u8; 16];

        let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
        let es = X25519PrivateKey::from_bytes(*alice_eph.secret_bytes())
            .diffie_hellman(bob_public.as_bytes())
            .expect("es");
        let (_, request_ct) = alice
            .seal_session_request(&header, alice_eph_public, es, &payload)
            .expect("seal");

        let bob = Ssu2Transcript::new(Role::Responder, bob_public);
        let es_bob = X25519PrivateKey::from_bytes(*bob_static.secret_bytes())
            .diffie_hellman(alice_eph_public.as_bytes())
            .expect("es");
        let mut mutated = request_ct.clone();
        let last = mutated.len() - 1;
        mutated[last] ^= 1;
        assert_eq!(
            bob.accept_session_request(&header, alice_eph_public, es_bob, &mutated)
                .map(|_| ()),
            Err(Ssu2CryptoError::AuthenticationFailed)
        );

        let (_, wrong_public) = test_keys(34);
        let _ = alice_static;
        let wrong = Ssu2Transcript::new(Role::Initiator, wrong_public);
        let (wrong_eph, wrong_eph_public) = test_keys(35);
        let es_wrong = X25519PrivateKey::from_bytes(*wrong_eph.secret_bytes())
            .diffie_hellman(wrong_public.as_bytes())
            .expect("es");
        let (_, wrong_ct) = wrong
            .seal_session_request(&header, wrong_eph_public, es_wrong, &payload)
            .expect("seal");
        let bob = Ssu2Transcript::new(Role::Responder, bob_public);
        let es_bob = X25519PrivateKey::from_bytes(*bob_static.secret_bytes())
            .diffie_hellman(wrong_eph_public.as_bytes())
            .expect("es");
        assert_eq!(
            bob.accept_session_request(&header, wrong_eph_public, es_bob, &wrong_ct)
                .map(|_| ()),
            Err(Ssu2CryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn header_protection_round_trip_is_exact() {
        let k1 = [0x11_u8; 32];
        let k2 = [0x22_u8; 32];
        let mut datagram = vec![0xabu8; 88];
        datagram[12] = constants::MESSAGE_SESSION_REQUEST;
        let original = datagram.clone();
        apply_header_protection(&mut datagram, constants::LONG_HEADER_LENGTH, &k1, &k2, true)
            .expect("protect");
        assert_ne!(datagram, original);
        remove_header_protection(&mut datagram, constants::LONG_HEADER_LENGTH, &k1, &k2, true)
            .expect("unprotect");
        assert_eq!(datagram, original);

        let mut short = vec![0xcdu8; 40];
        apply_header_protection(&mut short, constants::SHORT_HEADER_LENGTH, &k1, &k2, false)
            .expect("protect");
        assert_ne!(short, vec![0xcdu8; 40]);
        remove_header_protection(&mut short, constants::SHORT_HEADER_LENGTH, &k1, &k2, false)
            .expect("unprotect");
        assert_eq!(short, vec![0xcdu8; 40]);
    }

    #[test]
    fn header_protection_rejects_short_and_wrong_keys() {
        let k1 = [0x11_u8; 32];
        let k2 = [0x22_u8; 32];
        let mut datagram = vec![0xabu8; 88];
        apply_header_protection(&mut datagram, constants::LONG_HEADER_LENGTH, &k1, &k2, true)
            .expect("protect");
        let mut wrong = datagram.clone();
        let bad = [0x33_u8; 32];
        assert!(
            remove_header_protection(&mut wrong, constants::LONG_HEADER_LENGTH, &bad, &k2, true)
                .is_ok()
        );
        assert_ne!(wrong[..16], datagram[..16]);
        let mut tiny = vec![0_u8; 39];
        assert_eq!(
            apply_header_protection(&mut tiny, constants::SHORT_HEADER_LENGTH, &k1, &k2, false),
            Err(Ssu2CryptoError::FieldTooLarge)
        );
    }

    #[test]
    fn token_payload_seal_open_round_trip() {
        let intro = IntroKey::new([0x42_u8; 32]);
        let header = [0x99_u8; constants::LONG_HEADER_LENGTH];
        let payload = [0x07_u8; 32];
        let sealed = seal_token_payload(&intro, 1234, &header, &payload).expect("seal");
        let opened = open_token_payload(&intro, 1234, &header, &sealed).expect("open");
        assert_eq!(opened, payload);
        let mut mutated = sealed.clone();
        mutated[0] ^= 1;
        assert_eq!(
            open_token_payload(&intro, 1234, &header, &mutated),
            Err(Ssu2CryptoError::AuthenticationFailed)
        );
        assert_eq!(
            open_token_payload(&intro, 1235, &header, &sealed),
            Err(Ssu2CryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn data_keys_derive_directionally() {
        let ck = [0x66_u8; 32];
        let created = session_created_header_key(&ck).expect("created key");
        let confirmed = session_confirmed_header_key(&ck).expect("confirmed key");
        assert_ne!(created, confirmed);
        let again = session_created_header_key(&ck).expect("created key");
        assert_eq!(created, again);
        let directional = [0x77_u8; 32];
        let (k_data, k_header_2) = derive_data_keys(&directional).expect("data keys");
        assert_ne!(k_data, k_header_2);
    }

    #[test]
    fn nonce_counter_refuses_forbidden_maximum() {
        assert!(NonceCounter::nonce_bytes(constants::MAX_PACKET_NUMBER).is_ok());
        assert_eq!(
            NonceCounter::nonce_bytes(u64::MAX),
            Err(Ssu2CryptoError::NonceExhausted)
        );
    }
}
