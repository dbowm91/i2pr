//! NTCP2 cryptographic composition over reviewed primitive crates.
//!
//! This module deliberately implements only the single I2P Noise transcript
//! composition required by NTCP2. X25519, AES, ChaCha20-Poly1305, SHA-256,
//! HMAC, and SipHash are delegated to reviewed dependencies; the code here
//! sequences them, owns bounded state, and enforces protocol labels and
//! nonce transitions. Complete message parsing and socket behavior belong to
//! later plans.

#![allow(clippy::module_name_repetitions)]

use aes::{Aes256, cipher::BlockDecrypt, cipher::BlockEncrypt, cipher::KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, aead::Aead};
use hmac::{Hmac, Mac};
use i2pr_crypto::{X25519SharedSecret, constant_time_eq, sha256};
use sha2::Sha256;
use siphasher::sip::SipHasher24;
use std::hash::Hasher;
use thiserror::Error;
use zeroize::Zeroize;

use crate::constants;

type HmacSha256 = Hmac<Sha256>;

/// A typed NTCP2 public-key value.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PublicKeyBytes([u8; constants::KEY_LENGTH]);

impl PublicKeyBytes {
    /// Constructs a public key and rejects the all-zero low-order encoding.
    pub fn new(bytes: [u8; constants::KEY_LENGTH]) -> Result<Self, Ntcp2CryptoError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(Ntcp2CryptoError::InvalidPublicKey);
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

impl std::fmt::Debug for PublicKeyBytes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("PublicKeyBytes")
            .field(&"<redacted>")
            .finish()
    }
}

/// Transcript hash material, which is public evidence but not a secret key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptHash([u8; constants::HASH_LENGTH]);

impl TranscriptHash {
    /// Borrows the exact SHA-256 digest bytes.
    pub const fn as_bytes(&self) -> &[u8; constants::HASH_LENGTH] {
        &self.0
    }
}

/// Explicit Noise role used for transmit/receive key assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// The party that sends SessionRequest and SessionConfirmed.
    Initiator,
    /// The party that sends SessionCreated.
    Responder,
}

/// Typed errors from bounded NTCP2 cryptographic operations.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum Ntcp2CryptoError {
    /// A public value had an invalid exact length or representation.
    #[error("invalid NTCP2 public key")]
    InvalidPublicKey,
    /// A supplied field exceeded its protocol maximum.
    #[error("NTCP2 field exceeds its bounded maximum")]
    FieldTooLarge,
    /// A frame length was outside the authenticated wire range.
    #[error("NTCP2 frame length {length} is outside 16..=65535")]
    FrameLengthOutOfRange {
        /// The rejected clear frame length.
        length: u16,
    },
    /// A nonce would exceed the last permitted counter value.
    #[error("NTCP2 nonce counter exhausted")]
    NonceExhausted,
    /// The authenticated encryption operation failed.
    #[error("NTCP2 authenticated encryption failed")]
    EncryptionFailed,
    /// The authenticated ciphertext or associated data was invalid.
    #[error("NTCP2 authentication failed")]
    AuthenticationFailed,
    /// A consuming transcript operation was called in the wrong state.
    #[error("NTCP2 transcript operation is not valid in this state")]
    InvalidState,
    /// A role-specific transcript operation was called by the wrong role.
    #[error("NTCP2 transcript operation is invalid for this role")]
    WrongRole,
    /// The static key revealed in SessionConfirmed did not match the bound key.
    #[error("NTCP2 peer static key mismatch")]
    PeerStaticMismatch,
    /// A KDF input could not be represented by the selected HMAC wrapper.
    #[error("NTCP2 KDF input rejected")]
    KdfInput,
}

/// A zeroizing ChaCha20-Poly1305 key owner.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct AeadKey([u8; constants::KEY_LENGTH]);

impl AeadKey {
    fn from_bytes(bytes: [u8; constants::KEY_LENGTH]) -> Self {
        Self(bytes)
    }
}

/// A zeroizing Noise chaining-key owner.
#[derive(Zeroize)]
#[zeroize(drop)]
struct ChainKey([u8; constants::HASH_LENGTH]);

/// A checked nonce counter. The forbidden `2^64 - 1` value is never emitted.
struct NonceCounter(u64);

impl NonceCounter {
    const fn new() -> Self {
        Self(0)
    }

    fn next(&mut self) -> Result<[u8; constants::NONCE_LENGTH], Ntcp2CryptoError> {
        if self.0 > constants::MAX_NONCE {
            return Err(Ntcp2CryptoError::NonceExhausted);
        }
        let counter = self.0;
        self.0 = counter
            .checked_add(1)
            .ok_or(Ntcp2CryptoError::NonceExhausted)?;
        let mut nonce = [0_u8; constants::NONCE_LENGTH];
        nonce[4..].copy_from_slice(&counter.to_le_bytes());
        Ok(nonce)
    }
}

/// An owned, consuming ChaCha20-Poly1305 state with checked nonce use.
pub struct CipherState {
    key: AeadKey,
    nonce: NonceCounter,
}

impl CipherState {
    fn new(key: AeadKey) -> Self {
        Self {
            key,
            nonce: NonceCounter::new(),
        }
    }

    /// Constructs a deterministic cipher owner for local data-phase tests.
    #[doc(hidden)]
    pub fn from_key_for_test(key: [u8; constants::KEY_LENGTH]) -> Self {
        Self::new(AeadKey::from_bytes(key))
    }

    /// Encrypts one bounded payload and consumes one nonce value.
    pub fn seal(
        &mut self,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, Ntcp2CryptoError> {
        if plaintext.len() > constants::MAX_FRAME_PLAINTEXT {
            return Err(Ntcp2CryptoError::FieldTooLarge);
        }
        let nonce = self.nonce.next()?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key.0));
        cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad: associated_data,
                },
            )
            .map_err(|_| Ntcp2CryptoError::EncryptionFailed)
    }

    /// Authenticates and decrypts one bounded payload, consuming one nonce.
    pub fn open(
        &mut self,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, Ntcp2CryptoError> {
        if ciphertext.len() < constants::AUTH_TAG_LENGTH
            || ciphertext.len() > constants::MAX_FRAME_LENGTH
        {
            return Err(Ntcp2CryptoError::FieldTooLarge);
        }
        let nonce = self.nonce.next()?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key.0));
        cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                chacha20poly1305::aead::Payload {
                    msg: ciphertext,
                    aad: associated_data,
                },
            )
            .map_err(|_| Ntcp2CryptoError::AuthenticationFailed)
    }
}

/// Stateful AES-256-CBC obfuscation for the two NTCP2 ephemeral fields.
pub struct AesObfuscationState {
    key: [u8; constants::KEY_LENGTH],
    chain: [u8; constants::AES_BLOCK_LENGTH],
}

impl AesObfuscationState {
    /// Starts the state with the router hash and published 16-byte IV.
    pub const fn new(
        router_hash: [u8; constants::KEY_LENGTH],
        iv: [u8; constants::AES_BLOCK_LENGTH],
    ) -> Self {
        Self {
            key: router_hash,
            chain: iv,
        }
    }

    /// Encrypts one 32-byte ephemeral key and advances the CBC state.
    pub fn encrypt(&mut self, plaintext: &PublicKeyBytes) -> PublicKeyBytes {
        let mut ciphertext = [0_u8; constants::KEY_LENGTH];
        for (input, output) in plaintext
            .as_bytes()
            .chunks_exact(16)
            .zip(ciphertext.chunks_exact_mut(16))
        {
            let mut block = [0_u8; 16];
            for (index, value) in input.iter().enumerate() {
                block[index] = *value ^ self.chain[index];
            }
            let cipher = Aes256::new_from_slice(&self.key).expect("fixed AES-256 key length");
            let mut block = aes::cipher::Block::<Aes256>::clone_from_slice(&block);
            cipher.encrypt_block(&mut block);
            output.copy_from_slice(&block);
            self.chain.copy_from_slice(&block);
        }
        PublicKeyBytes::from_bytes_for_test(ciphertext)
    }

    /// Decrypts one 32-byte ephemeral key and advances the CBC state.
    pub fn decrypt(
        &mut self,
        ciphertext: &PublicKeyBytes,
    ) -> Result<PublicKeyBytes, Ntcp2CryptoError> {
        let mut plaintext = [0_u8; constants::KEY_LENGTH];
        for (input, output) in ciphertext
            .as_bytes()
            .chunks_exact(16)
            .zip(plaintext.chunks_exact_mut(16))
        {
            let previous = self.chain;
            let cipher = Aes256::new_from_slice(&self.key).expect("fixed AES-256 key length");
            let mut block = aes::cipher::Block::<Aes256>::clone_from_slice(input);
            cipher.decrypt_block(&mut block);
            for (index, value) in block.iter().enumerate() {
                output[index] = *value ^ previous[index];
            }
            self.chain.copy_from_slice(input);
        }
        PublicKeyBytes::new(plaintext)
    }
}

/// Directional SipHash-2-4 state for obfuscated frame lengths.
pub struct SipHashState {
    key1: u64,
    key2: u64,
    iv: [u8; constants::AES_BLOCK_LENGTH / 2],
}

impl SipHashState {
    fn new(material: &[u8; constants::HASH_LENGTH]) -> Self {
        let mut iv = [0_u8; 8];
        iv.copy_from_slice(&material[16..24]);
        Self {
            key1: u64::from_le_bytes(material[0..8].try_into().expect("fixed key slice")),
            key2: u64::from_le_bytes(material[8..16].try_into().expect("fixed key slice")),
            iv,
        }
    }

    /// Constructs deterministic length state for local data-phase tests.
    #[doc(hidden)]
    pub fn from_material_for_test(material: [u8; constants::HASH_LENGTH]) -> Self {
        Self::new(&material)
    }

    fn next_mask(&mut self) -> u16 {
        let mut hasher = SipHasher24::new_with_keys(self.key1, self.key2);
        hasher.write(&self.iv);
        let next_iv = hasher.finish().to_le_bytes();
        self.iv = next_iv;
        u16::from_le_bytes([next_iv[0], next_iv[1]])
    }

    /// Obfuscates one clear, valid frame length and advances state.
    pub fn obfuscate_length(&mut self, length: u16) -> Result<u16, Ntcp2CryptoError> {
        if !(16..=u16::MAX).contains(&length) {
            return Err(Ntcp2CryptoError::FrameLengthOutOfRange { length });
        }
        Ok(length ^ self.next_mask())
    }

    /// Deobfuscates one wire length and validates the clear result.
    ///
    /// The wire value may be any `u16`; validating it before XOR would reject
    /// valid frames whose obfuscated prefix happens to be below 16.
    pub fn deobfuscate_length(&mut self, obfuscated: u16) -> Result<u16, Ntcp2CryptoError> {
        let length = obfuscated ^ self.next_mask();
        if !(16..=u16::MAX).contains(&length) {
            return Err(Ntcp2CryptoError::FrameLengthOutOfRange { length });
        }
        Ok(length)
    }

    /// Compatibility alias for callers that are obfuscating a clear length.
    pub fn mask_length(&mut self, length: u16) -> Result<u16, Ntcp2CryptoError> {
        self.obfuscate_length(length)
    }
}

/// Final directional keys produced by the Noise Split and SipHash KDF.
pub struct SplitKeys {
    transmit: CipherState,
    receive: CipherState,
    transmit_lengths: SipHashState,
    receive_lengths: SipHashState,
}

impl SplitKeys {
    /// Borrows the transmit cipher state for one data-phase frame.
    pub const fn transmit(&mut self) -> &mut CipherState {
        &mut self.transmit
    }

    /// Borrows the receive cipher state for one data-phase frame.
    pub const fn receive(&mut self) -> &mut CipherState {
        &mut self.receive
    }

    /// Borrows the transmit frame-length mask state.
    pub const fn transmit_lengths(&mut self) -> &mut SipHashState {
        &mut self.transmit_lengths
    }

    /// Borrows the receive frame-length mask state.
    pub const fn receive_lengths(&mut self) -> &mut SipHashState {
        &mut self.receive_lengths
    }

    /// Consumes the combined key owner into independent directional parts.
    pub fn into_parts(self) -> (CipherState, CipherState, SipHashState, SipHashState) {
        (
            self.transmit,
            self.receive,
            self.transmit_lengths,
            self.receive_lengths,
        )
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TranscriptStage {
    Initial,
    Message1Complete,
    Message1Padded,
    Message2Complete,
    Message2Padded,
    StaticEncrypted,
    Confirmed,
}

/// A consuming NTCP2 transcript for the three handshake messages.
pub struct Transcript {
    role: Role,
    peer_static: PublicKeyBytes,
    hash: TranscriptHash,
    chaining_key: ChainKey,
    cipher: Option<CipherState>,
    stage: TranscriptStage,
}

impl Transcript {
    /// Initializes the symmetric state and binds the responder static key.
    pub fn new(role: Role, responder_static: PublicKeyBytes) -> Self {
        let protocol_hash = protocol_name_hash();
        let mut hash = protocol_hash;
        hash = sha256_concat(&hash, constants::PROLOGUE);
        hash = sha256_concat(&hash, responder_static.as_bytes());
        Self {
            role,
            peer_static: responder_static,
            hash: TranscriptHash(hash),
            chaining_key: ChainKey(protocol_hash),
            cipher: None,
            stage: TranscriptStage::Initial,
        }
    }

    /// Returns the role selected at construction.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Returns the responder static key bound into this transcript.
    pub const fn responder_static(&self) -> PublicKeyBytes {
        self.peer_static
    }

    /// Returns the current transcript hash for deterministic evidence.
    #[doc(hidden)]
    pub const fn evidence_hash(&self) -> TranscriptHash {
        self.hash
    }

    /// Mixes one public byte region into the transcript hash.
    pub fn mix_hash(mut self, bytes: &[u8]) -> Self {
        self.hash = TranscriptHash(sha256_concat(&self.hash.0, bytes));
        self
    }

    /// Mixes cleartext handshake padding after the associated encrypted frame.
    pub fn mix_padding(mut self, padding: &[u8]) -> Result<Self, Ntcp2CryptoError> {
        if padding.len()
            > constants::MAX_SESSION_REQUEST_PADDING.max(constants::MAX_SESSION_CREATED_PADDING)
        {
            return Err(Ntcp2CryptoError::FieldTooLarge);
        }
        if !matches!(
            self.stage,
            TranscriptStage::Message1Complete | TranscriptStage::Message2Complete
        ) {
            return Err(Ntcp2CryptoError::InvalidState);
        }
        if !padding.is_empty() {
            self.hash = TranscriptHash(sha256_concat(&self.hash.0, padding));
        }
        self.stage = match self.stage {
            TranscriptStage::Message1Complete => TranscriptStage::Message1Padded,
            TranscriptStage::Message2Complete => TranscriptStage::Message2Padded,
            _ => unreachable!("stage checked above"),
        };
        Ok(self)
    }

    /// Performs the initiator's SessionRequest cryptographic portion.
    pub fn session_request(
        self,
        ephemeral_public: PublicKeyBytes,
        shared_secret: X25519SharedSecret,
        options: &[u8],
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Initiator || self.stage != TranscriptStage::Initial {
            return Err(if self.role != Role::Initiator {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let state = self
            .mix_ephemeral(ephemeral_public)
            .mix_key(shared_secret)?;
        let (mut state, ciphertext) = state.encrypt_and_hash(options)?;
        state.stage = TranscriptStage::Message1Complete;
        Ok((state, ciphertext))
    }

    /// Performs the responder's SessionRequest cryptographic portion.
    pub fn accept_session_request(
        self,
        ephemeral_public: PublicKeyBytes,
        shared_secret: X25519SharedSecret,
        ciphertext: &[u8],
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Responder || self.stage != TranscriptStage::Initial {
            return Err(if self.role != Role::Responder {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let state = self
            .mix_ephemeral(ephemeral_public)
            .mix_key(shared_secret)?;
        let (mut state, plaintext) = state.decrypt_and_hash(ciphertext)?;
        state.stage = TranscriptStage::Message1Complete;
        Ok((state, plaintext))
    }

    /// Performs the responder's SessionCreated cryptographic portion.
    pub fn session_created(
        self,
        ephemeral_public: PublicKeyBytes,
        shared_secret: X25519SharedSecret,
        options: &[u8],
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Responder || self.stage != TranscriptStage::Message1Padded {
            return Err(if self.role != Role::Responder {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let state = self
            .mix_ephemeral(ephemeral_public)
            .mix_key(shared_secret)?;
        let (mut state, ciphertext) = state.encrypt_and_hash(options)?;
        state.stage = TranscriptStage::Message2Complete;
        Ok((state, ciphertext))
    }

    /// Performs the initiator's SessionCreated cryptographic portion.
    pub fn accept_session_created(
        self,
        ephemeral_public: PublicKeyBytes,
        shared_secret: X25519SharedSecret,
        ciphertext: &[u8],
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Initiator || self.stage != TranscriptStage::Message1Padded {
            return Err(if self.role != Role::Initiator {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let state = self
            .mix_ephemeral(ephemeral_public)
            .mix_key(shared_secret)?;
        let (mut state, plaintext) = state.decrypt_and_hash(ciphertext)?;
        state.stage = TranscriptStage::Message2Complete;
        Ok((state, plaintext))
    }

    /// Encrypts Alice's static key for SessionConfirmed part one.
    pub fn encrypt_static(
        self,
        static_public: PublicKeyBytes,
        shared_secret: X25519SharedSecret,
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Initiator || self.stage != TranscriptStage::Message2Padded {
            return Err(if self.role != Role::Initiator {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let mut state = self;
        let associated_data = state.hash.0;
        let cipher = state
            .cipher
            .as_mut()
            .ok_or(Ntcp2CryptoError::InvalidState)?;
        let ciphertext = cipher.seal(static_public.as_bytes(), &associated_data)?;
        state.hash = TranscriptHash(sha256_concat(&associated_data, &ciphertext));
        let mut state = state.mix_key(shared_secret)?;
        state.stage = TranscriptStage::StaticEncrypted;
        Ok((state, ciphertext))
    }

    /// Decrypts and validates Alice's static key for SessionConfirmed part one.
    pub fn decrypt_static(
        self,
        expected_static: PublicKeyBytes,
        shared_secret: X25519SharedSecret,
        ciphertext: &[u8],
    ) -> Result<(Self, PublicKeyBytes), Ntcp2CryptoError> {
        if self.role != Role::Responder || self.stage != TranscriptStage::Message2Padded {
            return Err(if self.role != Role::Responder {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let mut state = self;
        let associated_data = state.hash.0;
        let cipher = state
            .cipher
            .as_mut()
            .ok_or(Ntcp2CryptoError::InvalidState)?;
        let plaintext = cipher.open(ciphertext, &associated_data)?;
        state.hash = TranscriptHash(sha256_concat(&associated_data, ciphertext));
        let bytes: [u8; constants::KEY_LENGTH] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| Ntcp2CryptoError::InvalidPublicKey)?;
        let static_public = PublicKeyBytes::new(bytes)?;
        if !constant_time_eq(static_public.as_bytes(), expected_static.as_bytes()) {
            return Err(Ntcp2CryptoError::PeerStaticMismatch);
        }
        let mut state = state.mix_key(shared_secret)?;
        state.stage = TranscriptStage::StaticEncrypted;
        Ok((state, static_public))
    }

    /// Decrypts SessionConfirmed part one before RouterInfo binding is known.
    ///
    /// The responder must first recover Alice's static key, then decrypt the
    /// RouterInfo that authenticates and binds that key. Callers must compare
    /// the returned public value with the validated RouterInfo before treating
    /// the handshake as authenticated.
    pub fn decrypt_static_unchecked(
        self,
        ciphertext: &[u8],
    ) -> Result<(Self, PublicKeyBytes), Ntcp2CryptoError> {
        if self.role != Role::Responder || self.stage != TranscriptStage::Message2Padded {
            return Err(if self.role != Role::Responder {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let mut state = self;
        let associated_data = state.hash.0;
        let cipher = state
            .cipher
            .as_mut()
            .ok_or(Ntcp2CryptoError::InvalidState)?;
        let plaintext = cipher.open(ciphertext, &associated_data)?;
        state.hash = TranscriptHash(sha256_concat(&associated_data, ciphertext));
        let bytes: [u8; constants::KEY_LENGTH] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| Ntcp2CryptoError::InvalidPublicKey)?;
        let static_public = PublicKeyBytes::new(bytes)?;
        Ok((state, static_public))
    }

    /// Completes SessionConfirmed part one after an unchecked static-key read.
    ///
    /// This is separate because the responder must recover Alice's static
    /// public key before it can compute the `se` shared secret.
    pub fn mix_static_secret(
        self,
        shared_secret: X25519SharedSecret,
    ) -> Result<Self, Ntcp2CryptoError> {
        if self.role != Role::Responder || self.stage != TranscriptStage::Message2Padded {
            return Err(if self.role != Role::Responder {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let mut state = self.mix_key(shared_secret)?;
        state.stage = TranscriptStage::StaticEncrypted;
        Ok(state)
    }

    /// Encrypts SessionConfirmed part two and completes the handshake transcript.
    pub fn encrypt_confirmed_payload(
        self,
        payload: &[u8],
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Initiator || self.stage != TranscriptStage::StaticEncrypted {
            return Err(if self.role != Role::Initiator {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let (mut state, ciphertext) = self.encrypt_and_hash(payload)?;
        state.stage = TranscriptStage::Confirmed;
        Ok((state, ciphertext))
    }

    /// Decrypts SessionConfirmed part two and completes the handshake transcript.
    pub fn decrypt_confirmed_payload(
        self,
        ciphertext: &[u8],
    ) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        if self.role != Role::Responder || self.stage != TranscriptStage::StaticEncrypted {
            return Err(if self.role != Role::Responder {
                Ntcp2CryptoError::WrongRole
            } else {
                Ntcp2CryptoError::InvalidState
            });
        }
        let (mut state, plaintext) = self.decrypt_and_hash(ciphertext)?;
        state.stage = TranscriptStage::Confirmed;
        Ok((state, plaintext))
    }

    /// Consumes the handshake state and derives directional data-phase keys.
    pub fn split(self) -> Result<SplitKeys, Ntcp2CryptoError> {
        if self.stage != TranscriptStage::Confirmed || self.cipher.is_none() {
            return Err(Ntcp2CryptoError::InvalidState);
        }
        let temp_key = hmac_sha256(&self.chaining_key.0, &[])?;
        let k_ab = hmac_sha256(&temp_key, &[1])?;
        let mut k_ba_input = Vec::with_capacity(constants::HASH_LENGTH + 1);
        k_ba_input.extend_from_slice(&k_ab);
        k_ba_input.push(2);
        let k_ba = hmac_sha256(&temp_key, &k_ba_input)?;

        let mut ask_input = Vec::with_capacity(constants::ASK_LABEL.len() + 1);
        ask_input.extend_from_slice(constants::ASK_LABEL);
        ask_input.push(1);
        let ask_master = hmac_sha256(&temp_key, &ask_input)?;
        let mut sip_input =
            Vec::with_capacity(constants::HASH_LENGTH + constants::SIPHASH_LABEL.len());
        sip_input.extend_from_slice(&self.hash.0);
        sip_input.extend_from_slice(constants::SIPHASH_LABEL);
        let sip_temp = hmac_sha256(&ask_master, &sip_input)?;
        let sip_master = hmac_sha256(&sip_temp, &[1])?;
        let sip_temp = hmac_sha256(&sip_master, &[])?;
        let sip_ab = hmac_sha256(&sip_temp, &[1])?;
        let mut sip_ba_input = Vec::with_capacity(constants::HASH_LENGTH + 1);
        sip_ba_input.extend_from_slice(&sip_ab);
        sip_ba_input.push(2);
        let sip_ba = hmac_sha256(&sip_temp, &sip_ba_input)?;

        let (transmit_key, receive_key, transmit_sip, receive_sip) = match self.role {
            Role::Initiator => (k_ab, k_ba, sip_ab, sip_ba),
            Role::Responder => (k_ba, k_ab, sip_ba, sip_ab),
        };
        Ok(SplitKeys {
            transmit: CipherState::new(AeadKey::from_bytes(transmit_key)),
            receive: CipherState::new(AeadKey::from_bytes(receive_key)),
            transmit_lengths: SipHashState::new(&transmit_sip),
            receive_lengths: SipHashState::new(&receive_sip),
        })
    }

    fn mix_ephemeral(mut self, ephemeral_public: PublicKeyBytes) -> Self {
        self.hash = TranscriptHash(sha256_concat(&self.hash.0, ephemeral_public.as_bytes()));
        self
    }

    fn mix_key(mut self, shared_secret: X25519SharedSecret) -> Result<Self, Ntcp2CryptoError> {
        let temp_key = hmac_sha256(&self.chaining_key.0, shared_secret.as_bytes())?;
        let next_chain = hmac_sha256(&temp_key, &[1])?;
        let mut cipher_input = Vec::with_capacity(constants::HASH_LENGTH + 1);
        cipher_input.extend_from_slice(&next_chain);
        cipher_input.push(2);
        let cipher_key = hmac_sha256(&temp_key, &cipher_input)?;
        self.chaining_key = ChainKey(next_chain);
        self.cipher = Some(CipherState::new(AeadKey::from_bytes(cipher_key)));
        Ok(self)
    }

    fn encrypt_and_hash(mut self, plaintext: &[u8]) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        let cipher = self.cipher.as_mut().ok_or(Ntcp2CryptoError::InvalidState)?;
        let ciphertext = cipher.seal(plaintext, &self.hash.0)?;
        self.hash = TranscriptHash(sha256_concat(&self.hash.0, &ciphertext));
        Ok((self, ciphertext))
    }

    fn decrypt_and_hash(mut self, ciphertext: &[u8]) -> Result<(Self, Vec<u8>), Ntcp2CryptoError> {
        let cipher = self.cipher.as_mut().ok_or(Ntcp2CryptoError::InvalidState)?;
        let plaintext = cipher.open(ciphertext, &self.hash.0)?;
        self.hash = TranscriptHash(sha256_concat(&self.hash.0, ciphertext));
        Ok((self, plaintext))
    }
}

fn protocol_name_hash() -> [u8; constants::HASH_LENGTH] {
    sha256(constants::PROTOCOL_NAME).as_bytes().to_owned()
}

fn sha256_concat(
    left: &[u8; constants::HASH_LENGTH],
    right: &[u8],
) -> [u8; constants::HASH_LENGTH] {
    let mut bytes = Vec::with_capacity(left.len() + right.len());
    bytes.extend_from_slice(left);
    bytes.extend_from_slice(right);
    sha256(&bytes).as_bytes().to_owned()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<[u8; constants::HASH_LENGTH], Ntcp2CryptoError> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(key).map_err(|_| Ntcp2CryptoError::KdfInput)?;
    mac.update(data);
    let result = mac.finalize().into_bytes();
    result
        .as_slice()
        .try_into()
        .map_err(|_| Ntcp2CryptoError::KdfInput)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::X25519PrivateKey;

    fn vector_row(id: &str) -> Vec<Vec<u8>> {
        let line = include_str!("../../../tests/fixtures/ntcp2/crypto/vectors.tsv")
            .lines()
            .find(|line| line.starts_with(id))
            .expect("fixture row");
        line.split_whitespace()
            .skip(2)
            .map(|value| {
                value
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        let high = (pair[0] as char).to_digit(16).expect("hex high");
                        let low = (pair[1] as char).to_digit(16).expect("hex low");
                        ((high << 4) | low) as u8
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn nonce_boundaries_are_checked_before_reuse() {
        let mut counter = NonceCounter(constants::MAX_NONCE);
        assert_eq!(
            counter.next().expect("last permitted nonce")[4..],
            constants::MAX_NONCE.to_le_bytes()
        );
        assert_eq!(counter.next(), Err(Ntcp2CryptoError::NonceExhausted));
    }

    #[test]
    fn aes_state_round_trips_two_ephemeral_fields() {
        let hash = [0x11_u8; 32];
        let iv = [0x22_u8; 16];
        let x = PublicKeyBytes::from_bytes_for_test([0x33_u8; 32]);
        let y = PublicKeyBytes::from_bytes_for_test([0x44_u8; 32]);
        let mut alice = AesObfuscationState::new(hash, iv);
        let encrypted_x = alice.encrypt(&x);
        let mut bob = AesObfuscationState::new(hash, iv);
        assert_eq!(bob.decrypt(&encrypted_x).expect("decrypt X"), x);
        let encrypted_y = alice.encrypt(&y);
        assert_eq!(bob.decrypt(&encrypted_y).expect("decrypt Y"), y);
    }

    #[test]
    fn transcript_roles_cross_match_and_split() {
        let responder = X25519PrivateKey::from_bytes([0x42_u8; 32]);
        let initiator = X25519PrivateKey::from_bytes([0x24_u8; 32]);
        let initiator_ephemeral = X25519PrivateKey::from_bytes([0x13_u8; 32]);
        let responder_ephemeral = X25519PrivateKey::from_bytes([0x31_u8; 32]);
        let responder_public = PublicKeyBytes::new(responder.public_bytes()).expect("Bob public");
        let initiator_public = PublicKeyBytes::new(initiator.public_bytes()).expect("Alice public");
        let x_public = PublicKeyBytes::new(initiator_ephemeral.public_bytes()).expect("X public");
        let y_public = PublicKeyBytes::new(responder_ephemeral.public_bytes()).expect("Y public");
        let es_a = initiator_ephemeral
            .diffie_hellman(&responder.public_bytes())
            .expect("es");
        let es_b = responder
            .diffie_hellman(&initiator_ephemeral.public_bytes())
            .expect("es");
        let (alice, request) = Transcript::new(Role::Initiator, responder_public)
            .session_request(x_public, es_a, b"request")
            .expect("request");
        let request_vector = vector_row("session-request-aead");
        assert_eq!(request, request_vector[0]);
        let (bob, request_plain) = Transcript::new(Role::Responder, responder_public)
            .accept_session_request(x_public, es_b, &request)
            .expect("accept request");
        assert_eq!(request_plain, b"request");
        let alice = alice
            .mix_padding(&[0xaa, 0xbb, 0xcc])
            .expect("request padding");
        let bob = bob
            .mix_padding(&[0xaa, 0xbb, 0xcc])
            .expect("request padding");
        let ee_a = initiator_ephemeral
            .diffie_hellman(&responder_ephemeral.public_bytes())
            .expect("ee");
        let ee_b = responder_ephemeral
            .diffie_hellman(&initiator_ephemeral.public_bytes())
            .expect("ee");
        let (bob, created) = bob
            .session_created(y_public, ee_b, b"created")
            .expect("created");
        let created_cipher_vector = vector_row("session-created-aead");
        assert_eq!(created, created_cipher_vector[0]);
        let (alice, created_plain) = alice
            .accept_session_created(y_public, ee_a, &created)
            .expect("accept created");
        assert_eq!(created_plain, b"created");
        let alice = alice.mix_padding(&[1, 2, 3, 4]).expect("created padding");
        let bob = bob.mix_padding(&[1, 2, 3, 4]).expect("created padding");
        let created_hash_vector = vector_row("session-created-aead");
        assert_eq!(
            alice.evidence_hash().as_bytes(),
            created_hash_vector[1].as_slice()
        );
        let se_a = initiator
            .diffie_hellman(&responder_ephemeral.public_bytes())
            .expect("se");
        let se_b = responder_ephemeral
            .diffie_hellman(&initiator.public_bytes())
            .expect("se");
        let (alice, encrypted_static) = alice
            .encrypt_static(initiator_public, se_a)
            .expect("static");
        let static_vector = vector_row("session-confirmed-static-aead");
        assert_eq!(encrypted_static, static_vector[0]);
        let (bob, decrypted_static) = bob
            .decrypt_static(initiator_public, se_b, &encrypted_static)
            .expect("static");
        assert_eq!(decrypted_static, initiator_public);
        let (alice, confirmed) = alice
            .encrypt_confirmed_payload(b"confirmed")
            .expect("confirmed");
        let confirmed_vector = vector_row("session-confirmed-payload-aead");
        assert_eq!(confirmed, confirmed_vector[0]);
        let (bob, confirmed_plain) = bob
            .decrypt_confirmed_payload(&confirmed)
            .expect("confirmed");
        assert_eq!(confirmed_plain, b"confirmed");
        assert_eq!(alice.evidence_hash(), bob.evidence_hash());
        let final_vector = vector_row("transcript-final-hash");
        assert_eq!(alice.evidence_hash().as_bytes(), final_vector[1].as_slice());
        let mut alice_split = alice.split().expect("Alice split");
        let mut bob_split = bob.split().expect("Bob split");
        let frame = alice_split
            .transmit()
            .seal(b"data", &[])
            .expect("seal data");
        assert_eq!(
            bob_split.receive().open(&frame, &[]).expect("open data"),
            b"data"
        );
    }

    #[test]
    fn wrong_static_key_is_typed_and_secret_debug_is_redacted() {
        let responder = PublicKeyBytes::new([1_u8; 32]).expect("public");
        let transcript = Transcript::new(Role::Initiator, responder);
        assert_eq!(transcript.role(), Role::Initiator);
        assert!(format!("{responder:?}").contains("redacted"));
        assert_eq!(
            PublicKeyBytes::new([0_u8; 32]),
            Err(Ntcp2CryptoError::InvalidPublicKey)
        );
    }

    #[test]
    fn aead_tag_mutation_is_rejected() {
        let mut sender = CipherState::new(AeadKey::from_bytes([0x55_u8; 32]));
        let mut receiver = CipherState::new(AeadKey::from_bytes([0x55_u8; 32]));
        let mut ciphertext = sender.seal(b"authenticated", b"header").expect("seal");
        ciphertext[0] ^= 1;
        assert_eq!(
            receiver.open(&ciphertext, b"header"),
            Err(Ntcp2CryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn independent_fixture_values_match_every_primitive_wrapper() {
        let public_a = vector_row("x25519-alice-public");
        let public_b = vector_row("x25519-bob-public");
        let shared = vector_row("x25519-shared");
        assert_eq!(
            X25519PrivateKey::from_bytes(public_a[0].as_slice().try_into().expect("private"))
                .public_bytes()
                .as_slice(),
            public_a[1].as_slice()
        );
        assert_eq!(
            X25519PrivateKey::from_bytes(public_b[0].as_slice().try_into().expect("private"))
                .public_bytes()
                .as_slice(),
            public_b[1].as_slice()
        );
        assert_eq!(
            X25519PrivateKey::from_bytes(shared[0].as_slice().try_into().expect("private"))
                .diffie_hellman(&shared[1].as_slice().try_into().expect("peer"))
                .expect("shared secret")
                .as_bytes(),
            shared[2].as_slice()
        );

        let protocol = vector_row("protocol-name-hash");
        assert_eq!(
            sha256(constants::PROTOCOL_NAME).as_bytes(),
            protocol[1].as_slice()
        );
        let initial = vector_row("transcript-initial-hash");
        let responder = PublicKeyBytes::new(initial[0].as_slice().try_into().expect("key"))
            .expect("responder key");
        assert_eq!(
            Transcript::new(Role::Initiator, responder)
                .evidence_hash()
                .as_bytes(),
            initial[1].as_slice()
        );

        let aead = vector_row("chacha20poly1305-seal");
        let mut cipher = CipherState::new(AeadKey::from_bytes(
            aead[0].as_slice().try_into().expect("AEAD key"),
        ));
        assert_eq!(cipher.seal(&aead[3], &aead[2]).expect("AEAD seal"), aead[4]);

        let aes = vector_row("aes-cbc-ephemeral");
        let mut obfuscator = AesObfuscationState::new(
            aes[0].as_slice().try_into().expect("AES key"),
            aes[1].as_slice().try_into().expect("AES IV"),
        );
        let x = PublicKeyBytes::new(aes[2].as_slice().try_into().expect("X")).expect("X public");
        let y = PublicKeyBytes::new(aes[4].as_slice().try_into().expect("Y")).expect("Y public");
        let encrypted_x = obfuscator.encrypt(&x);
        assert_eq!(encrypted_x.as_bytes(), aes[3].as_slice());
        let encrypted_y = obfuscator.encrypt(&y);
        assert_eq!(encrypted_y.as_bytes(), aes[5].as_slice());

        let split_vectors = vector_row("split-kdf");
        let transcript = Transcript {
            role: Role::Initiator,
            peer_static: PublicKeyBytes::from_bytes_for_test([1_u8; 32]),
            hash: TranscriptHash(split_vectors[1].as_slice().try_into().expect("hash")),
            chaining_key: ChainKey(split_vectors[0].as_slice().try_into().expect("chain")),
            cipher: Some(CipherState::new(AeadKey::from_bytes([0_u8; 32]))),
            stage: TranscriptStage::Confirmed,
        };
        let split = transcript.split().expect("split");
        assert_eq!(split.transmit.key.0, split_vectors[2].as_slice());
        assert_eq!(split.receive.key.0, split_vectors[3].as_slice());
        let expected_tx_sip =
            SipHashState::new(&split_vectors[4].as_slice().try_into().expect("sip"));
        let expected_rx_sip =
            SipHashState::new(&split_vectors[5].as_slice().try_into().expect("sip"));
        assert_eq!(split.transmit_lengths.key1, expected_tx_sip.key1);
        assert_eq!(split.transmit_lengths.key2, expected_tx_sip.key2);
        assert_eq!(split.transmit_lengths.iv, expected_tx_sip.iv);
        assert_eq!(split.receive_lengths.key1, expected_rx_sip.key1);
        assert_eq!(split.receive_lengths.key2, expected_rx_sip.key2);
        assert_eq!(split.receive_lengths.iv, expected_rx_sip.iv);
    }
}
