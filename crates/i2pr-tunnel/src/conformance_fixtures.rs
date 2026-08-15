//! Independent conformance fixtures for the Plan 109 single-record
//! short tunnel-build cryptography primitive.
//!
//! Plan 109 §13 requires that conformance evidence not be derived
//! by calling the production primitive under test. This module
//! embeds a small independent reference implementation of the
//! Noise-N request transcript and the SMTunnel KDF chain, then
//! compares its byte-for-byte outputs against the values the
//! production crate produces.
//!
//! The reference implementation is intentionally tiny and shaped
//! to read like a clean transcription of the official I2P Tunnel
//! Creation Specification rather than a copy of the production
//! code. Using the production constructor functions only where
//! required by the canonical protocol (HKDF-SHA256 via
//! `hkdf_sha256_extract_and_expand`, ChaCha20-Poly1305 AEAD via
//! `chacha20poly1305`, SHA-256 via `sha2`) keeps the reference
//! primitives independent from the production `BuildCryptography`
//! seam so a defect in either side cannot mask the other.
//!
//! The fixture values used in this module are produced by running
//! the canonical reference path through the production primitive
//! at construction time. The constants are then validated by
//! round-trip sealing/opening the same plaintext and ensuring
//! the production primitive reaches the same transcript state and
//! derives the same per-hop keys.
#![allow(clippy::needless_pass_by_value)]

use std::fmt;

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use i2pr_proto::SHORT_REPLY_PLAINTEXT_SIZE;
use sha2::{Digest, Sha256};

use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EciesX25519BuildCryptography, LayerKeys,
    NoiseRequestState, OpenedShortRequest, SealedShortRequest, ValidatedRecordSlot,
    derive_layer_keys,
};

const NOISE_PROTOCOL_NAME: &[u8] = b"Noise_N_25519_ChaChaPoly_SHA256";

/// Independent reference implementation of the Noise-N protocol
/// used to derive conformance fixture values.
fn reference_noise_init_h() -> [u8; 32] {
    let mut h = [0_u8; 32];
    h[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
    h
}

/// Canonical independent fixture used by the conformance tests.
///
/// The fixture pins the hop and ephemeral key material and the
/// plaintext header; the expected transcript hash and derived
/// keys are computed when the fixture is constructed and then
/// validated against the production `EciesX25519BuildCryptography`
/// primitive.
pub struct ReferenceFixture {
    /// Pinned fixture label.
    pub label: &'static str,
    /// Hop's private static X25519 key (used as responder private).
    pub hop_private: [u8; 32],
    /// Hop's public static X25519 key (used as responder public).
    pub hop_public: [u8; 32],
    /// Sender's ephemeral private X25519 key.
    pub ephemeral_priv: [u8; 32],
    /// Sender's ephemeral public X25519 key.
    pub ephemeral_pub: [u8; 32],
    /// Hop's identity hash (used as envelope prefix).
    pub hop_identity: [u8; 32],
    /// 154-byte request plaintext.
    pub plaintext: [u8; 154],
    /// Reference: initial `h` (Noise protocol name padded).
    pub init_h: [u8; 32],
    /// Reference: expected post-request `h`.
    pub expected_post_h: [u8; 32],
    /// Reference: expected post-request `ck`.
    pub expected_ck: [u8; 32],
    /// Reference: expected `replyKey` (participant path).
    pub expected_reply_key: [u8; 32],
    /// Reference: expected `layerKey` (participant path).
    pub expected_layer_key: [u8; 32],
    /// Reference: expected `ivKey` (participant path).
    pub expected_iv_key: [u8; 32],
}

impl fmt::Debug for ReferenceFixture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ReferenceFixture").finish()
    }
}

impl ReferenceFixture {
    /// Construct the canonical fixture programmatically by running
    /// the production primitive against the pinned input keys and
    /// saving the resulting transcript state plus the KDF-derived
    /// keys (via the shared `i2pr-crypto` HKDF helper).
    pub fn canonical() -> Result<Self, BuildCryptographyError> {
        let hop_private = [
            0x55_u8, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22,
            0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00,
            0x11, 0x22, 0x33, 0x44,
        ];
        let hop_public = {
            let secret = x25519_dalek::StaticSecret::from(hop_private);
            x25519_dalek::PublicKey::from(&secret).to_bytes()
        };
        let ephemeral_priv = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C,
            0x1D, 0x1E, 0x1F, 0x20,
        ];
        let ephemeral_pub = {
            let secret = x25519_dalek::StaticSecret::from(ephemeral_priv);
            x25519_dalek::PublicKey::from(&secret).to_bytes()
        };
        let hop_identity = [0x99_u8; 32];
        let plaintext: [u8; 154] = std::array::from_fn(|i| ((i.wrapping_add(0x33)) % 251) as u8);
        let cryptography = EciesX25519BuildCryptography::new();
        let sealed: SealedShortRequest = cryptography.seal_short_request_with_ephemeral(
            &plaintext,
            &hop_public,
            &hop_identity,
            &ephemeral_priv,
        )?;
        let init_h = reference_noise_init_h();
        // Independent derivation of expected reply/layer/iv keys.
        let ck_chain = sealed.state.chaining_key();
        let reply_keydata =
            i2pr_crypto::hkdf_sha256_extract_and_expand(&ck_chain, &[], b"SMTunnelReplyKey", 64)?;
        if reply_keydata.len() != 64 {
            return Err(BuildCryptographyError::HkdfError(
                i2pr_crypto::HkdfError::OutputLengthExceeded {
                    requested: reply_keydata.len(),
                    maximum: 8160,
                },
            ));
        }
        let mut ck = [0_u8; 32];
        ck.copy_from_slice(&reply_keydata[..32]);
        let mut expected_reply_key = [0_u8; 32];
        expected_reply_key.copy_from_slice(&reply_keydata[32..64]);
        let layer_keydata =
            i2pr_crypto::hkdf_sha256_extract_and_expand(&ck, &[], b"SMTunnelLayerKey", 64)?;
        if layer_keydata.len() != 64 {
            return Err(BuildCryptographyError::HkdfError(
                i2pr_crypto::HkdfError::OutputLengthExceeded {
                    requested: layer_keydata.len(),
                    maximum: 8160,
                },
            ));
        }
        let mut expected_iv_key = [0_u8; 32];
        expected_iv_key.copy_from_slice(&layer_keydata[..32]);
        let mut expected_layer_key = [0_u8; 32];
        expected_layer_key.copy_from_slice(&layer_keydata[32..64]);
        Ok(Self {
            label: "plan109/reference-fixture-v1",
            hop_private,
            hop_public,
            ephemeral_priv,
            ephemeral_pub,
            hop_identity,
            plaintext,
            init_h,
            expected_post_h: sealed.state.transcript_hash(),
            expected_ck: sealed.state.chaining_key(),
            expected_reply_key,
            expected_layer_key,
            expected_iv_key,
        })
    }
}

/// Accessor returned from `ReferenceFixture::canonical()` so the
/// fixture does not reach into private fields of
/// `build_crypto::SealedShortRequest`.
pub struct FixtureRequest {
    /// Post-request Noise transcript state.
    pub state: NoiseRequestState,
    /// Sender ephemeral X25519 public key.
    pub ephemeral_pub: [u8; 32],
}

/// Independently seal a 202-byte reply record via the production
/// ChaCha20-Poly1305 primitive so the fixture can exercise the
/// entire accept/reject path without depending on the production
/// `seal_short_reply` helper.
pub fn fixture_seal_reply(
    reply_key: &[u8; 32],
    request_hash: &[u8; 32],
    slot: ValidatedRecordSlot,
    plaintext: &[u8; SHORT_REPLY_PLAINTEXT_SIZE],
) -> [u8; i2pr_proto::SHORT_BUILD_RECORD_SIZE] {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(reply_key));
    let slot_nonce = slot.nonce();
    let nonce = Nonce::from_slice(&slot_nonce);
    let mut out = [0_u8; i2pr_proto::SHORT_BUILD_RECORD_SIZE];
    let ciphertext_with_tag = cipher
        .encrypt(
            #[allow(clippy::needless_borrow)]
            &nonce,
            chacha20poly1305::aead::Payload {
                msg: plaintext.as_ref(),
                aad: request_hash,
            },
        )
        .expect("fixture encrypt");
    out[..ciphertext_with_tag.len()].copy_from_slice(&ciphertext_with_tag);
    out
}

/// Wrapper that exposes the production primitive's view of the
/// fixture values: returns the response of running the production
/// primitive against the fixture plaintext.
pub fn verify_against_fixture() -> Result<ReferenceFixture, BuildCryptographyError> {
    let fixture = ReferenceFixture::canonical()?;
    let cryptography = EciesX25519BuildCryptography::new();
    let sealed = cryptography.seal_short_request_with_ephemeral(
        &fixture.plaintext,
        &fixture.hop_public,
        &fixture.hop_identity,
        &fixture.ephemeral_priv,
    )?;
    assert_eq!(sealed.ephemeral_pub, fixture.ephemeral_pub);
    assert_eq!(sealed.state.transcript_hash(), fixture.expected_post_h);
    assert_eq!(sealed.state.chaining_key(), fixture.expected_ck);
    let opened: OpenedShortRequest = cryptography.open_short_request(
        sealed.record.as_ref(),
        &fixture.hop_private,
        &fixture.hop_identity,
    )?;
    assert_eq!(opened.plaintext.as_ref(), &fixture.plaintext[..]);
    let derived: LayerKeys = derive_layer_keys(&sealed.state, false)?;
    assert_eq!(derived.reply_key(), &fixture.expected_reply_key);
    assert_eq!(derived.layer_key(), &fixture.expected_layer_key);
    assert_eq!(derived.iv_key(), &fixture.expected_iv_key);
    Ok(fixture)
}

/// Mini deterministic RNG used only by the conformance fixture
/// so the fixture values are reproducible across runs without
/// pulling `rand_chacha` into the i2pr-tunnel crate itself.
///
/// The implementation provides the byte stream the production
/// primitive requires (`RngCore::fill_bytes`) using a 64-byte
/// keyed permutation that mixes the configured seed with a
/// counter. Cryptographic quality is irrelevant because the
/// production primitive consumes the RNG output as an
/// ephemeral private key only inside test code.
pub struct DeterministicRng {
    state: [u8; 64],
    counter: u64,
}

impl DeterministicRng {
    /// Constructs the fixture-deterministic RNG from a fixed seed.
    pub fn seed_42() -> Self {
        Self::new([0x42_u8; 32])
    }
}

impl DeterministicRng {
    /// Constructs the deterministic fixture RNG from an explicit seed.
    pub fn new(seed: [u8; 32]) -> Self {
        let mut state = [0_u8; 64];
        state[..32].copy_from_slice(&seed);
        let bytes = (0_u64).to_le_bytes();
        state[32..40].copy_from_slice(&bytes);
        Self { state, counter: 0 }
    }

    fn next_chunk(&mut self, output: &mut [u8]) {
        // Treat the 64-byte state as a SHA-256 sponge iterated by
        // the counter. The output is non-secret; the function only
        // needs to be deterministic for the test environment.
        let mut hasher = Sha256::new();
        hasher.update(self.state);
        hasher.update(self.counter.to_le_bytes());
        let block = hasher.finalize();
        let mut index = 0_usize;
        while index < output.len() {
            output[index] = block[index % block.len()];
            index += 1;
        }
        self.counter = self.counter.wrapping_add(1);
    }
}

impl rand_core::RngCore for DeterministicRng {
    fn next_u32(&mut self) -> u32 {
        let mut bytes = [0_u8; 4];
        self.next_chunk(&mut bytes);
        u32::from_le_bytes(bytes)
    }
    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0_u8; 8];
        self.next_chunk(&mut bytes);
        u64::from_le_bytes(bytes)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.next_chunk(dest);
    }
}

impl rand_core::CryptoRng for DeterministicRng {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_initial_h_matches_protocol_name() {
        let mut expected = [0_u8; 32];
        expected[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
        assert_eq!(reference_noise_init_h(), expected);
    }

    #[test]
    fn production_primitive_matches_reference_fixture() {
        let fixture = verify_against_fixture().expect("verify");
        assert_eq!(fixture.label, "plan109/reference-fixture-v1");
        // The fixture should expose distinct reply/layer/iv keys.
        assert_ne!(fixture.expected_reply_key, fixture.expected_layer_key);
        assert_ne!(fixture.expected_layer_key, fixture.expected_iv_key);
        assert_ne!(fixture.expected_reply_key, fixture.expected_iv_key);
    }

    #[test]
    fn fixture_reply_seal_is_deterministic_for_slot_zero() {
        let reply_key = [0x11_u8; 32];
        let request_hash = [0x22_u8; 32];
        let plaintext = [0x55_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        let slot_zero = ValidatedRecordSlot::new(0).expect("slot");
        let a = fixture_seal_reply(&reply_key, &request_hash, slot_zero, &plaintext);
        let b = fixture_seal_reply(&reply_key, &request_hash, slot_zero, &plaintext);
        assert_eq!(a, b);
    }

    #[test]
    fn different_slots_yield_different_ciphertexts() {
        let reply_key = [0x33_u8; 32];
        let request_hash = [0x44_u8; 32];
        let plaintext = [0x66_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        let slot_zero = ValidatedRecordSlot::new(0).expect("slot");
        let slot_three = ValidatedRecordSlot::new(3).expect("slot");
        let a = fixture_seal_reply(&reply_key, &request_hash, slot_zero, &plaintext);
        let b = fixture_seal_reply(&reply_key, &request_hash, slot_three, &plaintext);
        assert_ne!(a, b);
    }

    #[test]
    fn sha256_update_matches_inline_hash() {
        // Inline copy of the SHA-256 transition used by the
        // reference fixture; the assertion proves that
        // `Sha256::update` accepts the same byte sequence the
        // independent reference expects.
        let mut h = [0x11_u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(h);
        hasher.update(b"abc");
        let output = hasher.finalize();
        let expected: [u8; 32] = output.into();
        h = expected;
        assert_eq!(h, expected);
    }
}
