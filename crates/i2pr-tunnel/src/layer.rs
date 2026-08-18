//! AES-256 tunnel layer transforms.
//!
//! Plan 116 §3.5 and §15 own the canonical participant layer
//! transform and the inverse creator / inbound-endpoint transforms.
//! The transforms operate over fixed-size buffers and use the
//! per-hop `LayerKeys::layer_key()` and `iv_key()` derived from the
//! Plan 109 short-build KDF chain.
//!
//! The module also owns the duplicate fingerprint function and a
//! bounded exact-match replay window the transit role uses.
//!
//! ```text
//! participant_forward:
//!     working_iv = AES256-ECB-ENC(ivKey, received_iv)
//!     new_data   = AES256-CBC-ENC(layerKey, working_iv, received_data)
//!     next_iv    = AES256-ECB-ENC(ivKey, working_iv)
//!
//! creator_inverse_one_hop (one reverse hop step):
//!     working_iv = AES256-ECB-DEC(ivKey, received_iv)
//!     new_data   = AES256-CBC-DEC(layerKey, working_iv, received_data)
//!     prev_iv    = AES256-ECB-DEC(ivKey, working_iv)
//! ```
//!
//! Proposal 153 (ChaCha tunnel layer encryption) is explicitly out
//! of scope. The established tunnel data plane uses AES.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    unused_imports,
    clippy::manual_range_contains,
    clippy::type_complexity,
    clippy::needless_borrow,
    missing_docs
)]

use std::collections::BTreeSet;
use std::fmt;

use aes::Aes256;
use aes::cipher::block_padding::NoPadding;
use aes::cipher::{BlockDecrypt, BlockDecryptMut, BlockEncrypt, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};
use thiserror::Error;
use zeroize::Zeroize;

use crate::build_crypto::LayerKeys;

/// Length of the AES-256 IV carried in the TunnelData header.
pub const TUNNEL_IV_LEN: usize = 16;

/// Length of the AES-256 CBC ciphertext block (1008 bytes) after
/// the IV.
pub const TUNNEL_PAYLOAD_LEN: usize = 1008;

/// Hard ceiling on the bounded replay window size.
pub const MAX_DUPLICATE_WINDOW: usize = 1024;

type Aes256CbcEnc = Encryptor<Aes256>;
type Aes256CbcDec = Decryptor<Aes256>;

/// AES-256 ECB encryptor over a single 16-byte block.
#[inline]
fn aes256_ecb_encrypt_block(key: &[u8; 32], block: &[u8; TUNNEL_IV_LEN]) -> [u8; TUNNEL_IV_LEN] {
    use aes::cipher::KeyInit;
    let cipher = Aes256::new_from_slice(key).expect("AES-256 key is exactly 32 bytes");
    let mut out = [0_u8; TUNNEL_IV_LEN];
    out.copy_from_slice(block);
    let mut block_ref = aes::cipher::generic_array::GenericArray::clone_from_slice(&out);
    cipher.encrypt_block(&mut block_ref);
    out.copy_from_slice(&block_ref);
    out
}

/// AES-256 ECB decryptor over a single 16-byte block.
#[inline]
fn aes256_ecb_decrypt_block(key: &[u8; 32], block: &[u8; TUNNEL_IV_LEN]) -> [u8; TUNNEL_IV_LEN] {
    use aes::cipher::KeyInit;
    let cipher = Aes256::new_from_slice(key).expect("AES-256 key is exactly 32 bytes");
    let mut out = [0_u8; TUNNEL_IV_LEN];
    out.copy_from_slice(block);
    let mut block_ref = aes::cipher::generic_array::GenericArray::clone_from_slice(&out);
    cipher.decrypt_block(&mut block_ref);
    out.copy_from_slice(&block_ref);
    out
}

#[inline]
fn aes256_cbc_encrypt(
    key: &[u8; 32],
    iv: &[u8; TUNNEL_IV_LEN],
    plaintext: &[u8; TUNNEL_PAYLOAD_LEN],
) -> [u8; TUNNEL_PAYLOAD_LEN] {
    let cipher = Aes256CbcEnc::new(key.into(), iv.into());
    let mut buf = [0_u8; TUNNEL_PAYLOAD_LEN];
    buf.copy_from_slice(plaintext);
    let len = cipher
        .encrypt_padded_mut::<NoPadding>(&mut buf, TUNNEL_PAYLOAD_LEN)
        .expect("in-place 1008-byte CBC encrypt")
        .len();
    debug_assert_eq!(len, TUNNEL_PAYLOAD_LEN);
    buf
}

#[inline]
fn aes256_cbc_decrypt(
    key: &[u8; 32],
    iv: &[u8; TUNNEL_IV_LEN],
    ciphertext: &[u8; TUNNEL_PAYLOAD_LEN],
) -> [u8; TUNNEL_PAYLOAD_LEN] {
    let cipher = Aes256CbcDec::new(key.into(), iv.into());
    let mut buf = [0_u8; TUNNEL_PAYLOAD_LEN];
    buf.copy_from_slice(ciphertext);
    let len = cipher
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .expect("in-place 1008-byte CBC decrypt")
        .len();
    debug_assert_eq!(len, TUNNEL_PAYLOAD_LEN);
    buf
}

/// Pure fixed-size AES-256 tunnel layer transforms.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TunnelLayerTransform;

impl TunnelLayerTransform {
    /// Constructs the transform.
    pub const fn new() -> Self {
        Self
    }

    /// Applies one participant layer transform.
    ///
    /// The function operates over fixed-size buffers and never
    /// allocates. It returns the next `(next_iv, next_data)` pair
    /// the participant hands to the next hop.
    pub fn participant_forward(
        layer_keys: &LayerKeys,
        received_iv: &[u8; TUNNEL_IV_LEN],
        received_data: &[u8; TUNNEL_PAYLOAD_LEN],
    ) -> ([u8; TUNNEL_IV_LEN], [u8; TUNNEL_PAYLOAD_LEN]) {
        let working_iv = aes256_ecb_encrypt_block(layer_keys.iv_key(), received_iv);
        let new_data = aes256_cbc_encrypt(layer_keys.layer_key(), &working_iv, received_data);
        let next_iv = aes256_ecb_encrypt_block(layer_keys.iv_key(), &working_iv);
        (next_iv, new_data)
    }

    /// Applies one inverse layer transform (creator
    /// preprocessing or inbound endpoint processing). The function
    /// decrypts one layer and exposes the prior `(iv, data)` pair
    /// in the direction the data plane expects.
    pub fn creator_inverse_one_hop(
        layer_keys: &LayerKeys,
        received_iv: &[u8; TUNNEL_IV_LEN],
        received_data: &[u8; TUNNEL_PAYLOAD_LEN],
    ) -> ([u8; TUNNEL_IV_LEN], [u8; TUNNEL_PAYLOAD_LEN]) {
        let working_iv = aes256_ecb_decrypt_block(layer_keys.iv_key(), received_iv);
        let new_data = aes256_cbc_decrypt(layer_keys.layer_key(), &working_iv, received_data);
        let prev_iv = aes256_ecb_decrypt_block(layer_keys.iv_key(), &working_iv);
        (prev_iv, new_data)
    }

    /// Applies the multi-hop creator inverse chain over the hops
    /// in reverse path order. The function returns the
    /// preprocessed `(iv, data)` the OBEP exposes.
    pub fn outbound_preprocess(
        hops_reverse: &[&LayerKeys],
        iv: [u8; TUNNEL_IV_LEN],
        data: [u8; TUNNEL_PAYLOAD_LEN],
    ) -> ([u8; TUNNEL_IV_LEN], [u8; TUNNEL_PAYLOAD_LEN]) {
        let mut current_iv = iv;
        let mut current_data = data;
        for keys in hops_reverse {
            let (next_iv, next_data) =
                Self::creator_inverse_one_hop(keys, &current_iv, &current_data);
            current_iv = next_iv;
            current_data = next_data;
        }
        (current_iv, current_data)
    }

    /// Applies the multi-hop inbound endpoint inverse chain over
    /// the hops in reverse path order. The function returns the
    /// preprocessed `(iv, data)` the local inbound endpoint
    /// exposes.
    pub fn inbound_endpoint_decrypt(
        hops_reverse: &[&LayerKeys],
        received_iv: [u8; TUNNEL_IV_LEN],
        received_data: [u8; TUNNEL_PAYLOAD_LEN],
    ) -> ([u8; TUNNEL_IV_LEN], [u8; TUNNEL_PAYLOAD_LEN]) {
        Self::outbound_preprocess(hops_reverse, received_iv, received_data)
    }
}

/// Duplicate fingerprint the participant computes and the
/// duplicate window records. The function is the canonical
/// XOR of the received IV with the first 16 bytes of the received
/// ciphertext.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DuplicateToken([u8; TUNNEL_IV_LEN]);

impl DuplicateToken {
    /// Constructs a token from the received IV and data.
    pub fn compute(
        received_iv: &[u8; TUNNEL_IV_LEN],
        received_data: &[u8; TUNNEL_PAYLOAD_LEN],
    ) -> Self {
        let mut out = [0_u8; TUNNEL_IV_LEN];
        for index in 0..TUNNEL_IV_LEN {
            out[index] = received_iv[index] ^ received_data[index];
        }
        Self(out)
    }
}

impl fmt::Display for DuplicateToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("DuplicateToken")
            .field(&self.0)
            .finish()
    }
}

impl Zeroize for DuplicateToken {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

/// Bounded exact-match replay window.
#[derive(Debug)]
pub struct DuplicateWindow {
    capacity: usize,
    tokens: BTreeSet<DuplicateToken>,
}

impl DuplicateWindow {
    /// Constructs a bounded window with the supplied capacity. The
    /// capacity is clamped to [`MAX_DUPLICATE_WINDOW`].
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.min(MAX_DUPLICATE_WINDOW);
        Self {
            capacity,
            tokens: BTreeSet::new(),
        }
    }

    /// Returns the bounded capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the current observed token count.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Returns whether the window is empty.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Returns whether the supplied token has already been
    /// observed.
    pub fn contains(&self, token: &DuplicateToken) -> bool {
        self.tokens.contains(token)
    }

    /// Observes one token. Returns `Ok(true)` for the first
    /// observation, `Ok(false)` for an exact duplicate already in
    /// the window, and `Err(CapacityExceeded)` when the window is
    /// at capacity and the token is not present.
    pub fn observe(&mut self, token: DuplicateToken) -> Result<bool, DuplicateWindowError> {
        if self.tokens.contains(&token) {
            return Ok(false);
        }
        if self.tokens.len() >= self.capacity {
            return Err(DuplicateWindowError::CapacityExceeded {
                capacity: self.capacity,
            });
        }
        self.tokens.insert(token);
        Ok(true)
    }

    /// Removes every observed token.
    pub fn clear(&mut self) {
        self.tokens.clear();
    }
}

impl Default for DuplicateWindow {
    fn default() -> Self {
        Self::new(MAX_DUPLICATE_WINDOW)
    }
}

/// Failure categories for [`DuplicateWindow`].
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DuplicateWindowError {
    /// The window reached capacity and refused to retain a new
    /// token.
    #[error("duplicate window at capacity {capacity}")]
    CapacityExceeded {
        /// Configured capacity.
        capacity: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(seed: u8) -> LayerKeys {
        LayerKeys::new(
            [seed; 32],
            [seed.wrapping_add(1); 32],
            [seed.wrapping_add(2); 32],
        )
    }

    #[test]
    fn one_hop_round_trip() {
        let layer_keys = keys(0x11);
        let original_iv = [0xAA_u8; TUNNEL_IV_LEN];
        let original_data = [0xBB_u8; TUNNEL_PAYLOAD_LEN];
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&layer_keys, &original_iv, &original_data);
        let (prev_iv, prev_data) =
            TunnelLayerTransform::creator_inverse_one_hop(&layer_keys, &next_iv, &next_data);
        assert_eq!(prev_iv, original_iv);
        assert_eq!(prev_data, original_data);
    }

    #[test]
    fn multi_hop_outbound_inverse_chain_round_trip() {
        let keys_a = keys(0x21);
        let keys_b = keys(0x22);
        let keys_c = keys(0x23);
        let original_iv = [0x55_u8; TUNNEL_IV_LEN];
        let original_data: [u8; TUNNEL_PAYLOAD_LEN] =
            std::array::from_fn(|i| (i.wrapping_mul(3).wrapping_add(7)) as u8);
        // Outbound creator preprocessing over hops in reverse order.
        let (iv, data) = TunnelLayerTransform::outbound_preprocess(
            &[&keys_c, &keys_b, &keys_a],
            original_iv,
            original_data,
        );
        // Each remote participant applies one forward hop in path order.
        let (iv_after_a, data_after_a) =
            TunnelLayerTransform::participant_forward(&keys_a, &iv, &data);
        let (iv_after_b, data_after_b) =
            TunnelLayerTransform::participant_forward(&keys_b, &iv_after_a, &data_after_a);
        let (iv_after_c, data_after_c) =
            TunnelLayerTransform::participant_forward(&keys_c, &iv_after_b, &data_after_b);
        assert_eq!(iv_after_c, original_iv);
        assert_eq!(data_after_c, original_data);
    }

    #[test]
    fn forward_then_inverse_restores_input() {
        let keys_a = keys(0x51);
        let keys_b = keys(0x52);
        let original_iv = [0x12_u8; TUNNEL_IV_LEN];
        let original_data = [0x34_u8; TUNNEL_PAYLOAD_LEN];
        // Path forward A then B.
        let (iv_a, data_a) =
            TunnelLayerTransform::participant_forward(&keys_a, &original_iv, &original_data);
        let (iv_b, data_b) = TunnelLayerTransform::participant_forward(&keys_b, &iv_a, &data_a);
        // Then inverse B then A.
        let (iv_after_b, data_after_b) =
            TunnelLayerTransform::creator_inverse_one_hop(&keys_b, &iv_b, &data_b);
        let (iv_after_a, data_after_a) =
            TunnelLayerTransform::creator_inverse_one_hop(&keys_a, &iv_after_b, &data_after_b);
        assert_eq!(iv_after_a, original_iv);
        assert_eq!(data_after_a, original_data);
    }

    #[test]
    fn multi_hop_forward_then_multi_hop_inverse_restores_input() {
        let keys_a = keys(0x71);
        let keys_b = keys(0x72);
        let keys_c = keys(0x73);
        let original_iv = [0xAB_u8; TUNNEL_IV_LEN];
        let original_data: [u8; TUNNEL_PAYLOAD_LEN] =
            std::array::from_fn(|i| (i.wrapping_mul(7).wrapping_add(13)) as u8);
        // Remote forwards A, B, C in path order.
        let (iv_a, data_a) =
            TunnelLayerTransform::participant_forward(&keys_a, &original_iv, &original_data);
        let (iv_b, data_b) = TunnelLayerTransform::participant_forward(&keys_b, &iv_a, &data_a);
        let (iv_c, data_c) = TunnelLayerTransform::participant_forward(&keys_c, &iv_b, &data_b);
        // Local creator/endpoint inverse over C, B, A in reverse.
        let (iv_after_c, data_after_c) =
            TunnelLayerTransform::creator_inverse_one_hop(&keys_c, &iv_c, &data_c);
        let (iv_after_b, data_after_b) =
            TunnelLayerTransform::creator_inverse_one_hop(&keys_b, &iv_after_c, &data_after_c);
        let (iv_after_a, data_after_a) =
            TunnelLayerTransform::creator_inverse_one_hop(&keys_a, &iv_after_b, &data_after_b);
        assert_eq!(iv_after_a, original_iv);
        assert_eq!(data_after_a, original_data);
    }

    #[test]
    fn wrong_layer_key_does_not_reproduce_plaintext() {
        let layer_keys = keys(0x41);
        let wrong = keys(0x42);
        let original_iv = [0x12_u8; TUNNEL_IV_LEN];
        let original_data = [0x34_u8; TUNNEL_PAYLOAD_LEN];
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&layer_keys, &original_iv, &original_data);
        let (prev_iv, prev_data) =
            TunnelLayerTransform::creator_inverse_one_hop(&wrong, &next_iv, &next_data);
        assert_ne!(prev_iv, original_iv);
        assert_ne!(prev_data, original_data);
    }

    #[test]
    fn wrong_iv_key_does_not_reproduce_plaintext() {
        let layer_keys = keys(0x61);
        let wrong = keys(0x71);
        let original_iv = [0x55_u8; TUNNEL_IV_LEN];
        let original_data = [0x77_u8; TUNNEL_PAYLOAD_LEN];
        let (next_iv, next_data) =
            TunnelLayerTransform::participant_forward(&layer_keys, &original_iv, &original_data);
        // The wrong key must fail to recover the original.
        let (prev_iv, prev_data) =
            TunnelLayerTransform::creator_inverse_one_hop(&wrong, &next_iv, &next_data);
        assert_ne!(prev_iv, original_iv);
        assert_ne!(prev_data, original_data);
    }

    #[test]
    fn duplicate_token_is_iv_xor_first_data_block() {
        let iv = [0x01_u8; TUNNEL_IV_LEN];
        let mut data = [0x02_u8; TUNNEL_PAYLOAD_LEN];
        for byte in data.iter_mut().take(TUNNEL_IV_LEN) {
            *byte = 0x03;
        }
        let token = DuplicateToken::compute(&iv, &data);
        let expected: [u8; TUNNEL_IV_LEN] = std::array::from_fn(|i| iv[i] ^ data[i]);
        assert_eq!(token.0, expected);
    }

    #[test]
    fn duplicate_window_observes_first_token() {
        let mut window = DuplicateWindow::new(2);
        let token = DuplicateToken::compute(&[0xAA; TUNNEL_IV_LEN], &[0xBB; TUNNEL_PAYLOAD_LEN]);
        assert!(window.observe(token).expect("first"));
        assert_eq!(window.len(), 1);
    }

    #[test]
    fn duplicate_window_rejects_duplicate() {
        let mut window = DuplicateWindow::new(2);
        let token = DuplicateToken::compute(&[0xAA; TUNNEL_IV_LEN], &[0xBB; TUNNEL_PAYLOAD_LEN]);
        assert!(window.observe(token).expect("first"));
        let result = window.observe(token).expect("duplicate");
        assert!(!result);
    }

    #[test]
    fn duplicate_window_fails_closed_at_capacity() {
        let mut window = DuplicateWindow::new(1);
        let token_a = DuplicateToken::compute(&[0x01; TUNNEL_IV_LEN], &[0x02; TUNNEL_PAYLOAD_LEN]);
        let token_b = DuplicateToken::compute(&[0x03; TUNNEL_IV_LEN], &[0x04; TUNNEL_PAYLOAD_LEN]);
        assert!(window.observe(token_a).expect("first"));
        let error = window.observe(token_b).unwrap_err();
        assert!(matches!(
            error,
            DuplicateWindowError::CapacityExceeded { .. }
        ));
    }

    #[test]
    fn swapping_iv_and_first_data_produces_equivalent_token() {
        // Token = iv XOR first_data_block. Swapping the IV and
        // the first 16 bytes of data produces an identical XOR,
        // therefore an identical token. The duplicate window
        // therefore treats the swapped cell as a duplicate.
        let mut window = DuplicateWindow::new(2);
        let iv = [0xAA_u8; TUNNEL_IV_LEN];
        let mut data = [0_u8; TUNNEL_PAYLOAD_LEN];
        for (idx, byte) in data.iter_mut().enumerate() {
            *byte = if idx < TUNNEL_IV_LEN { 0xBB } else { 0xCC };
        }
        let original_token = DuplicateToken::compute(&iv, &data);
        let mut swapped_iv = [0_u8; TUNNEL_IV_LEN];
        swapped_iv.copy_from_slice(&data[..TUNNEL_IV_LEN]);
        let mut swapped_data = [0_u8; TUNNEL_PAYLOAD_LEN];
        for idx in 0..TUNNEL_PAYLOAD_LEN {
            swapped_data[idx] = if idx < TUNNEL_IV_LEN {
                iv[idx]
            } else {
                data[idx]
            };
        }
        let swapped_token = DuplicateToken::compute(&swapped_iv, &swapped_data);
        assert_eq!(swapped_token, original_token);
        window.observe(original_token).expect("first");
        let observed = window.observe(swapped_token).expect("second");
        // The swapped cell is treated as a duplicate because
        // its token is identical to the original observation.
        assert!(!observed);
    }
}
