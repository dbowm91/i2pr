//! Raw ChaCha20 keystream for the I2P layer-encryption primitive.
//!
//! Plan 332 needs the raw RFC 7539 §2.4 stream cipher, not an AEAD: the
//! encrypted LeaseSet2 layers derive a fresh key from a fresh random salt on
//! every publication, so integrity is supplied by the Red25519 signature that
//! covers the whole record rather than by a per-layer authenticator.
//!
//! Two details are normative and are enforced here rather than left to each
//! caller:
//!
//! - The initial block counter is **1**, not 0. The I2P specification pins
//!   "the initial counter set to 1" for the encrypted LeaseSet2 layer stream,
//!   and both pinned references do the same. A caller cannot select a counter.
//! - The nonce is 12 bytes, so the cipher is the RFC 8439 variant, not the
//!   original 8-byte-nonce construction.
//!
//! Reusing a `(key, nonce)` pair is a keystream-reuse failure. This module
//! cannot detect it; the ELS2 layer key derivation generates a fresh random
//! 32-byte salt per layer per publication, which is what makes the fixed
//! counter safe.

#![forbid(unsafe_code)]

use chacha20::ChaCha20;
use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// ChaCha20 key length in bytes (`S_KEY_LEN` in the encrypted LeaseSet2 specification).
pub const CHACHA20_KEY_LENGTH: usize = 32;
/// ChaCha20 nonce length in bytes (`S_IV_LEN` in the encrypted LeaseSet2 specification).
///
/// A 12-byte nonce selects the RFC 8439 construction; the RFC 7539-original
/// 8-byte variant is not part of any I2P layer derivation.
pub const CHACHA20_NONCE_LENGTH: usize = 12;
/// ChaCha20 block size in bytes.
pub const CHACHA20_BLOCK_LENGTH: usize = 64;
/// The pinned initial block counter for I2P layer encryption.
///
/// RFC 8439 §2.4.2 and every I2P layer derivation start the keystream at
/// block 1. `chacha20::ChaCha20::new` starts at block 0, so the wrapper seeks
/// forward by exactly one block before applying the keystream.
pub const LAYER_INITIAL_BLOCK_COUNTER: u32 = 1;

/// Errors returned by the raw ChaCha20 layer primitive.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ChachaError {
    /// A supplied key or nonce had the wrong length.
    #[error("ChaCha20 {component} length {actual} is not {expected}")]
    InvalidLength {
        /// Which operand was wrong.
        component: &'static str,
        /// Actual length.
        actual: usize,
        /// Required length.
        expected: usize,
    },
}

/// A ChaCha20 layer key that erases itself on drop.
///
/// The type exists so a derived layer key cannot be copied into a log line, a
/// `Debug` transcript, or a long-lived structure. It is deliberately not
/// `Clone`, has no `Debug`/`Display`, and has no serde implementation. The
/// inner [`Zeroizing`] buffer supplies erase-on-drop; no derive is needed.
pub struct LayerCipherKey(Zeroizing<[u8; CHACHA20_KEY_LENGTH]>);

impl LayerCipherKey {
    /// Wraps 32 key bytes.
    pub fn from_bytes(bytes: [u8; CHACHA20_KEY_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrows the key bytes.
    ///
    /// The borrow is intended for the shortest practical interval: a single
    /// keystream application, not a structure lifetime.
    pub fn as_bytes(&self) -> &[u8; CHACHA20_KEY_LENGTH] {
        &self.0
    }

    /// Erases the key immediately instead of waiting for the drop.
    pub fn zeroize_now(&mut self) {
        self.0.as_mut().zeroize();
    }
}

/// Applies the I2P layer keystream to `data` in place.
///
/// `data` is both ciphertext and output: ChaCha20 is a stream cipher, so the
/// same call encrypts and decrypts. The initial block counter is always
/// [`LAYER_INITIAL_BLOCK_COUNTER`].
///
/// # Note on key material in memory
///
/// The RFC 8439 cipher object copies the key into its internal state, and that
/// state cannot be explicitly erased through the reviewed `chacha20` API. The
/// key is therefore erased by this module's owner ([`LayerCipherKey`]) as soon
/// as the borrow ends, which matches the existing SSU2 keystream usage. No
/// copy is ever formatted, serialized, or logged.
pub fn chacha20_xor_layer(
    key: &LayerCipherKey,
    nonce: &[u8; CHACHA20_NONCE_LENGTH],
    data: &mut [u8],
) -> Result<(), ChachaError> {
    let key_bytes: &[u8; CHACHA20_KEY_LENGTH] = key.as_bytes();
    if key_bytes.len() != CHACHA20_KEY_LENGTH {
        return Err(ChachaError::InvalidLength {
            component: "key",
            actual: key_bytes.len(),
            expected: CHACHA20_KEY_LENGTH,
        });
    }
    if nonce.len() != CHACHA20_NONCE_LENGTH {
        return Err(ChachaError::InvalidLength {
            component: "nonce",
            actual: nonce.len(),
            expected: CHACHA20_NONCE_LENGTH,
        });
    }
    let mut cipher = ChaCha20::new(key_bytes.into(), nonce.into());
    cipher.seek(LAYER_INITIAL_BLOCK_COUNTER as u64 * CHACHA20_BLOCK_LENGTH as u64);
    cipher.apply_keystream(data);
    Ok(())
}

/// Applies the layer keystream and returns the transformed buffer.
///
/// Convenient for tests and for one-shot layer construction; production
/// callers that already own a buffer should prefer [`chacha20_xor_layer`] so
/// the plaintext allocation is visibly temporary.
pub fn chacha20_xor_layer_owned(
    key: &LayerCipherKey,
    nonce: &[u8; CHACHA20_NONCE_LENGTH],
    input: &[u8],
) -> Result<Vec<u8>, ChachaError> {
    let mut buffer = input.to_vec();
    chacha20_xor_layer(key, nonce, &mut buffer)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 8439 §2.4.2 encryption example. The vector is chosen deliberately:
    /// its documented initial counter is 1, which is the value the I2P layer
    /// streams pin, so it simultaneously proves the counter and the keystream.
    const RFC8439_KEY: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];
    /// RFC 8439 §2.4.2 nonce: `00 00 00 00 00 00 00 4a 00 00 00 00`.
    const RFC8439_NONCE: [u8; 12] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00,
    ];
    const RFC8439_PLAINTEXT: &[u8] = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    const RFC8439_CIPHERTEXT_HEAD: [u8; 16] = [
        0x6e, 0x2e, 0x35, 0x9a, 0x25, 0x68, 0xf9, 0x80, 0x41, 0xba, 0x07, 0x28, 0xdd, 0x0d, 0x69,
        0x81,
    ];

    /// The RFC 8439 §2.4.2 nonce's keystream when the initial block counter is
    /// 0 instead of the pinned 1, over the first 16 plaintext bytes.
    ///
    /// This is an i2pr control value, not a published vector: it exists so a
    /// regression that silently drops the counter seek fails loudly. The
    /// §2.4.2 assertion above already distinguishes the two counters, because
    /// that published ciphertext is only reachable at counter 1.
    const COUNTER_ZERO_CONTROL: [u8; 16] = [
        0xe3, 0x64, 0x7a, 0x29, 0xde, 0xd3, 0x15, 0x28, 0xef, 0x56, 0xba, 0xc7, 0x0f, 0x7a, 0x7a,
        0xc3,
    ];

    #[test]
    fn rfc8439_section_2_4_2_vector_matches() {
        let key = LayerCipherKey::from_bytes(RFC8439_KEY);
        let produced =
            chacha20_xor_layer_owned(&key, &RFC8439_NONCE, RFC8439_PLAINTEXT).expect("xor");
        assert_eq!(RFC8439_PLAINTEXT.len(), produced.len());
        assert_eq!(&produced[..16], &RFC8439_CIPHERTEXT_HEAD);
    }

    #[test]
    fn published_vector_is_unreachable_at_block_counter_zero() {
        // Recompute the same §2.4.2 stream at counter 0. A zero-counter build
        // would produce this control value and would therefore already fail the
        // published-vector assertion; this test names the divergence explicitly
        // so the failure mode is documented rather than inferred.
        let mut control = RFC8439_PLAINTEXT[..16].to_vec();
        let mut cipher = ChaCha20::new(
            RFC8439_KEY.as_slice().into(),
            RFC8439_NONCE.as_slice().into(),
        );
        cipher.apply_keystream(&mut control);
        assert_eq!(&control[..], &COUNTER_ZERO_CONTROL);
        assert_ne!(&control[..], &RFC8439_CIPHERTEXT_HEAD);
    }

    #[test]
    fn keystream_is_symmetric() {
        let key = LayerCipherKey::from_bytes([7_u8; 32]);
        let nonce = [9_u8; 12];
        let plaintext = b"inner lease set payload".to_vec();
        let ciphertext = chacha20_xor_layer_owned(&key, &nonce, &plaintext).expect("encrypt");
        assert_ne!(ciphertext, plaintext);
        let mut buffer = ciphertext.clone();
        chacha20_xor_layer(&key, &nonce, &mut buffer).expect("decrypt");
        assert_eq!(buffer, plaintext);
    }

    #[test]
    fn block_counter_one_is_not_block_counter_zero() {
        // Prove the seek actually happens: block 0 and block 1 of the RFC 8439
        // example keystream differ, so a missing seek would be observable.
        let key = LayerCipherKey::from_bytes(RFC8439_KEY);
        let mut zero_counter = RFC8439_PLAINTEXT.to_vec();
        let mut cipher = ChaCha20::new(
            RFC8439_KEY.as_slice().into(),
            RFC8439_NONCE.as_slice().into(),
        );
        cipher.apply_keystream(&mut zero_counter);
        let one_counter =
            chacha20_xor_layer_owned(&key, &RFC8439_NONCE, RFC8439_PLAINTEXT).expect("xor");
        assert_ne!(zero_counter, one_counter);
    }

    #[test]
    fn distinct_keys_and_nonces_produce_distinct_keystreams() {
        let nonce = [1_u8; 12];
        let a =
            chacha20_xor_layer_owned(&LayerCipherKey::from_bytes([1_u8; 32]), &nonce, b"payload")
                .expect("a");
        let b =
            chacha20_xor_layer_owned(&LayerCipherKey::from_bytes([2_u8; 32]), &nonce, b"payload")
                .expect("b");
        let c = chacha20_xor_layer_owned(
            &LayerCipherKey::from_bytes([1_u8; 32]),
            &[2_u8; 12],
            b"payload",
        )
        .expect("c");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn empty_input_is_accepted() {
        let key = LayerCipherKey::from_bytes([3_u8; 32]);
        assert!(
            chacha20_xor_layer_owned(&key, &[0_u8; 12], &[])
                .expect("empty")
                .is_empty()
        );
    }
}
