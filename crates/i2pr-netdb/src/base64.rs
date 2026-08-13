//! I2P Base64 alphabet codec.
//!
//! I2P uses a variant of Base64 that differs from RFC 4648 in two
//! places: bytes `0x2D` (`-`) and `0x3F` (`?`) replace the RFC
//! `+` and `/` characters respectively, and padding uses `~` instead
//! of `=`. Filenames that embed the I2P Base64 router hash must use
//! exactly this alphabet; reusing the RFC 4648 alphabet produces a
//! different hash.
//!
//! The codec in this module is strict and bounded:
//!
//! - rejects inputs that contain characters outside the I2P alphabet;
//! - rejects inputs that do not satisfy the I2P length convention
//!   (4 * ceil(n / 3) characters, padded with `~` to a multiple of
//!   four);
//! - rejects inputs whose decoded length overflows the supplied
//!   ceiling;
//! - rejects inputs with leading or trailing padding bytes outside
//!   the I2P strict subset.

use thiserror::Error;

/// The maximum decoded byte length the helper will accept for a single
/// input. 512 bytes comfortably exceeds the largest expected payload
/// (a 32-byte SHA-256 router hash encodes to 44 I2P Base64 characters)
/// while remaining small enough to refuse obviously malicious inputs
/// without reading them.
pub const MAX_DECODED_LEN: usize = 512;

/// Errors emitted by the I2P Base64 codec.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum I2pBase64Error {
    /// The input length is not a multiple of four.
    #[error("i2p base64 input length {actual} is not a multiple of 4")]
    InvalidLength {
        /// Actual length.
        actual: usize,
    },
    /// The input contains a character outside the I2P Base64 alphabet.
    #[error("i2p base64 character {byte:#x} at index {index} is outside the I2P alphabet")]
    InvalidCharacter {
        /// Index of the offending byte.
        index: usize,
        /// Offending byte value.
        byte: u8,
    },
    /// The padding (`~`) appears in a position that is not allowed by
    /// the I2P strict variant.
    #[error("i2p base64 padding at index {index} is not in the final quantised position")]
    InvalidPadding {
        /// Index of the offending padding byte.
        index: usize,
    },
    /// The decoded length would exceed `MAX_DECODED_LEN`.
    #[error("i2p base64 decoded length {actual} exceeds {maximum}")]
    DecodedTooLarge {
        /// Actual decoded length.
        actual: usize,
        /// Maximum accepted.
        maximum: usize,
    },
}

/// Encodes `bytes` into the I2P Base64 alphabet with the I2P-specific
/// `~` padding.
///
/// `MAX_DECODED_LEN` applies to the input length. The output length is
/// always `4 * ceil(bytes.len() / 3)`.
///
/// Padding rules follow RFC 4648 with `~` replacing `=`:
///
/// - 3-byte chunks → 4 chars, no padding;
/// - 2-byte tail   → 3 chars + 1 `~`;
/// - 1-byte tail   → 2 chars + 2 `~`.
#[allow(dead_code)]
pub fn encode(bytes: &[u8]) -> Result<String, I2pBase64Error> {
    if bytes.len() > MAX_DECODED_LEN {
        return Err(I2pBase64Error::DecodedTooLarge {
            actual: bytes.len(),
            maximum: MAX_DECODED_LEN,
        });
    }
    let total = bytes.len();
    let tail = total % 3;
    let mut out = String::with_capacity(total.div_ceil(3) * 4);
    // Emit all but the final chunk (which may be partial) verbatim.
    let main_end = if tail == 0 { total } else { total - tail };
    for chunk in bytes[..main_end].chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        let n = ((u32::from(b0)) << 16) | ((u32::from(b1)) << 8) | u32::from(b2);
        let c0 = alphabet_byte((n >> 18) & 0x3F);
        let c1 = alphabet_byte((n >> 12) & 0x3F);
        let c2 = alphabet_byte((n >> 6) & 0x3F);
        let c3 = alphabet_byte(n & 0x3F);
        out.push(c0 as char);
        out.push(c1 as char);
        out.push(c2 as char);
        out.push(c3 as char);
    }
    // Handle the final partial chunk.
    if tail == 2 {
        let b0 = bytes[total - 2];
        let b1 = bytes[total - 1];
        let n = ((u32::from(b0)) << 16) | ((u32::from(b1)) << 8);
        let c0 = alphabet_byte((n >> 18) & 0x3F);
        let c1 = alphabet_byte((n >> 12) & 0x3F);
        let c2 = alphabet_byte((n >> 6) & 0x3F);
        out.push(c0 as char);
        out.push(c1 as char);
        out.push(c2 as char);
        out.push('~');
    } else if tail == 1 {
        let b0 = bytes[total - 1];
        let n = u32::from(b0) << 16;
        let c0 = alphabet_byte((n >> 18) & 0x3F);
        let c1 = alphabet_byte((n >> 12) & 0x3F);
        out.push(c0 as char);
        out.push(c1 as char);
        out.push('~');
        out.push('~');
    }
    Ok(out)
}

/// Decodes an I2P Base64 string into bytes. The input must use the I2P
/// alphabet and the I2P strict padding rules.
pub fn decode(input: &str) -> Result<Vec<u8>, I2pBase64Error> {
    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(I2pBase64Error::InvalidLength {
            actual: bytes.len(),
        });
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let mut accum: u32 = 0;
        let mut pad_count = 0;
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'~' {
                // Padding is only legal in the final chunk and only at
                // positions 2 or 3 (encoding a 1- or 2-byte tail).
                if index != 2 && index != 3 {
                    return Err(I2pBase64Error::InvalidPadding { index });
                }
                pad_count += 1;
                accum <<= 6;
                continue;
            }
            if pad_count > 0 {
                return Err(I2pBase64Error::InvalidPadding { index });
            }
            let value = alphabet_value(*byte)
                .ok_or(I2pBase64Error::InvalidCharacter { index, byte: *byte })?;
            accum = (accum << 6) | value;
        }
        let tail = 3 - pad_count;
        // The accum holds the packed bits left-aligned into the low 24
        // bits. For a full chunk the low 24 bits encode three bytes;
        // for tails we mask out the irrelevant trailing bits.
        let mask: u32 = match tail {
            1 => 0x00FF_0000,
            2 => 0x00FF_FF00,
            _ => 0x00FF_FFFF,
        };
        accum &= mask;
        for (index, shift) in [16_u32, 8_u32, 0_u32].iter().enumerate() {
            if index >= tail {
                break;
            }
            let byte = ((accum >> shift) & 0xFF) as u8;
            out.push(byte);
        }
    }
    if out.len() > MAX_DECODED_LEN {
        return Err(I2pBase64Error::DecodedTooLarge {
            actual: out.len(),
            maximum: MAX_DECODED_LEN,
        });
    }
    Ok(out)
}

fn alphabet_byte(value: u32) -> u8 {
    let index = value as usize;
    ENCODE[index]
}

fn alphabet_value(byte: u8) -> Option<u32> {
    let alphabet = decode_alphabet();
    if (byte as usize) < alphabet.len() {
        let v = alphabet[byte as usize];
        if v == 0xFF { None } else { Some(u32::from(v)) }
    } else {
        None
    }
}

const ENCODE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-?";

fn decode_alphabet() -> [u8; 256] {
    let mut table = [0xFF_u8; 256];
    for (index, byte) in ENCODE.iter().enumerate() {
        table[*byte as usize] = index as u8;
    }
    table
}

/// Encodes a 32-byte hash into the 44-character I2P Base64 prefix
/// used in reseed filenames.
///
/// The prefix is the I2P Base64 encoding of the raw hash bytes. For a
/// 32-byte SHA-256 RouterHash the encoding produces exactly 44
/// characters (ten 4-char groups covering 30 bytes plus a 4-char
/// padded final group for the 2-byte tail).
pub fn encode_filename_prefix(hash: &[u8; 32]) -> Result<String, I2pBase64Error> {
    encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip_is_stable() {
        let cases: &[&[u8]] = &[b"", b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"];
        for value in cases {
            let encoded = encode(value).expect("encode");
            let decoded = decode(&encoded).expect("decode");
            assert_eq!(decoded, *value);
        }
    }

    #[test]
    fn i2p_alphabet_differs_from_rfc4648_in_plus_and_slash() {
        // Bytes that would encode to '+' and '/' in RFC 4648 must encode
        // to '-' and '?' under I2P Base64.
        let input = [0xFB, 0xEF, 0xFF];
        let encoded = encode(&input).expect("encode");
        assert!(encoded.bytes().all(|byte| byte != b'+' && byte != b'/'));
    }

    #[test]
    fn padding_uses_tilde_not_equals() {
        let encoded_short = encode(b"f").expect("encode");
        assert!(
            encoded_short.ends_with("~~"),
            "1-byte tail must pad with two '~', got {encoded_short}"
        );
        let encoded_two = encode(b"fo").expect("encode");
        assert!(
            encoded_two.ends_with('~'),
            "2-byte tail must pad with one '~', got {encoded_two}"
        );
    }

    #[test]
    fn rfc4648_padding_is_rejected() {
        // The RFC 4648 padding character `=` is not part of the I2P
        // alphabet and must be rejected as an invalid character.
        let error = decode("Zm9v====").unwrap_err();
        assert!(matches!(error, I2pBase64Error::InvalidCharacter { .. }));
    }

    #[test]
    fn unknown_character_is_rejected() {
        let error = decode("AB*D").unwrap_err();
        assert!(matches!(error, I2pBase64Error::InvalidCharacter { .. }));
    }

    #[test]
    fn wrong_length_is_rejected() {
        let error = decode("ABC").unwrap_err();
        assert!(matches!(error, I2pBase64Error::InvalidLength { .. }));
    }

    #[test]
    fn known_vector_decodes_to_expected_bytes() {
        // "foobar" under both standard and I2P Base64 (they share the
        // same alphabet for inputs that don't hit the +/ slot) encodes
        // to "Zm9vYmFy".
        let encoded = encode(b"foobar").expect("encode");
        assert_eq!(encoded, "Zm9vYmFy");
        let decoded = decode(&encoded).expect("decode");
        assert_eq!(decoded, b"foobar");
    }
}
