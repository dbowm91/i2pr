//! HKDF-SHA256 key derivation helper.
//!
//! Plan 108 owns a bounded, allocation-light HKDF-SHA256 helper the
//! tunnel-build seam and other protocol-neutral primitives can share.
//! The helper exposes the standard RFC 5869 extract-and-expand
//! interface; callers supply `salt`, `ikm`, `info`, and an output
//! length, and receive a single fixed-size derived key.
//!
//! HKDF helpers in this crate are deliberately limited to a maximum
//! output length (255 * 32 bytes for HMAC-SHA256) and reject `L >
//! 255 * HashLen` at compile time. Callers needing more than 8160
//! bytes of derived material must chain successive calls.
//!
//! No new I2P-specific semantics belong here; the helper is protocol
//! neutral and may be reused by other crates that need a deterministic
//! HKDF-SHA256 derivation.

#![forbid(unsafe_code)]

use hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::CryptoError;

type HmacSha256 = Hmac<Sha256>;

/// Maximum output length accepted by [`hkdf_sha256_extract_and_expand`].
pub const MAX_HKDF_OUTPUT_LEN: usize = 255 * 32;

/// HKDF helper errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HkdfError {
    /// The caller requested more than [`MAX_HKDF_OUTPUT_LEN`] bytes.
    #[error("HKDF output length {requested} exceeds maximum {maximum}")]
    OutputLengthExceeded {
        /// Actual requested length.
        requested: usize,
        /// Maximum accepted length.
        maximum: usize,
    },
    /// The HMAC primitive refused a key length.
    #[error("HKDF key length rejected by HMAC primitive")]
    InvalidKeyLength,
}

impl From<HkdfError> for CryptoError {
    fn from(_value: HkdfError) -> Self {
        CryptoError::Protocol(i2pr_proto::CodecError::InvalidFieldValue {
            offset: 0,
            context: "HKDF derivation",
        })
    }
}

/// Performs the full HKDF-SHA256 extract-and-expand derivation as a
/// one-shot call.
///
/// `salt` may be empty; `ikm` is the input keying material;
/// `info` carries the protocol-specific context label. The returned
/// buffer has length `output_len` and is zeroized on drop.
pub fn hkdf_sha256_extract_and_expand(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
    output_len: usize,
) -> Result<Zeroizing<Vec<u8>>, HkdfError> {
    if output_len > MAX_HKDF_OUTPUT_LEN {
        return Err(HkdfError::OutputLengthExceeded {
            requested: output_len,
            maximum: MAX_HKDF_OUTPUT_LEN,
        });
    }
    let prk = hkdf_extract(salt, ikm)?;
    let okm = hkdf_expand(&prk, info, output_len)?;
    Ok(Zeroizing::new(okm))
}

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> Result<Zeroizing<[u8; 32]>, HkdfError> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(salt).map_err(|_| HkdfError::InvalidKeyLength)?;
    mac.update(ikm);
    let mut prk = Zeroizing::new([0_u8; 32]);
    prk.copy_from_slice(&mac.finalize().into_bytes());
    Ok(prk)
}

fn hkdf_expand(prk: &[u8; 32], info: &[u8], output_len: usize) -> Result<Vec<u8>, HkdfError> {
    let mut output = Vec::with_capacity(output_len);
    let mut previous: Option<Vec<u8>> = None;
    let mut counter: u8 = 1;
    while output.len() < output_len {
        let mut mac =
            <HmacSha256 as Mac>::new_from_slice(prk).map_err(|_| HkdfError::InvalidKeyLength)?;
        if let Some(ref previous_bytes) = previous {
            mac.update(previous_bytes);
        }
        mac.update(info);
        mac.update(&[counter]);
        let block = mac.finalize().into_bytes();
        previous = Some(block.to_vec());
        output.extend_from_slice(&block);
        counter = counter
            .checked_add(1)
            .ok_or(HkdfError::OutputLengthExceeded {
                requested: output_len,
                maximum: MAX_HKDF_OUTPUT_LEN,
            })?;
    }
    output.truncate(output_len);
    Ok(output)
}

/// Convenience wrapper that derives exactly 32 bytes and copies them
/// into a fixed-size array.
pub fn hkdf_sha256_32(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
) -> Result<Zeroizing<[u8; 32]>, HkdfError> {
    let derived = hkdf_sha256_extract_and_expand(salt, ikm, info, 32)?;
    let mut bytes = Zeroizing::new([0_u8; 32]);
    bytes.copy_from_slice(&derived);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hkdf_extract_and_expand_is_deterministic() {
        let salt = b"some-salt";
        let ikm = b"some-input-keying";
        let info = b"some-context";
        let first = hkdf_sha256_extract_and_expand(salt, ikm, info, 64).expect("hkdf");
        let second = hkdf_sha256_extract_and_expand(salt, ikm, info, 64).expect("hkdf");
        assert_eq!(&first[..], &second[..]);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn hkdf_inputs_change_output() {
        let a = hkdf_sha256_extract_and_expand(b"salt", b"input", b"info", 32).expect("a");
        let b = hkdf_sha256_extract_and_expand(b"salt-2", b"input", b"info", 32).expect("b");
        let c = hkdf_sha256_extract_and_expand(b"salt", b"input-2", b"info", 32).expect("c");
        let d = hkdf_sha256_extract_and_expand(b"salt", b"input", b"info-2", 32).expect("d");
        assert_ne!(&a[..], &b[..]);
        assert_ne!(&a[..], &c[..]);
        assert_ne!(&a[..], &d[..]);
    }

    #[test]
    fn hkdf_32_wrapper_returns_fixed_size_buffer() {
        let result = hkdf_sha256_32(&[], b"input", b"context").expect("hkdf");
        assert_eq!(result.len(), 32);
        let again = hkdf_sha256_32(&[], b"input", b"context").expect("hkdf");
        assert_eq!(&*result, &*again);
    }

    #[test]
    fn hkdf_extract_then_expand_matches_one_shot() {
        let salt = b"test-salt";
        let ikm = b"test-ikm";
        let info = b"test-info";
        let prk = hkdf_extract(salt, ikm).expect("extract");
        let expanded = hkdf_expand(&prk, info, 96).expect("expand");
        let one_shot = hkdf_sha256_extract_and_expand(salt, ikm, info, 96).expect("hkdf");
        assert_eq!(expanded.as_slice(), &one_shot[..]);
    }

    #[test]
    fn hkdf_rejects_oversized_outputs() {
        let outcome =
            hkdf_sha256_extract_and_expand(&[], b"input", b"info", MAX_HKDF_OUTPUT_LEN + 1);
        assert!(matches!(
            outcome,
            Err(HkdfError::OutputLengthExceeded { .. })
        ));
    }

    #[test]
    fn hkdf_zero_length_output_is_allowed() {
        let result = hkdf_sha256_extract_and_expand(b"salt", b"ikm", b"info", 0).expect("hkdf");
        assert!(result.is_empty());
    }
}
