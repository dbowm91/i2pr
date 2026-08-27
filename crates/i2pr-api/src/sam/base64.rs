//! SAM v3.1 standard Base64 codec.
//!
//! Plan 136 mandates the standard RFC 4648 alphabet (`A-Z a-z 0-9 + /`)
//! with standard `=` padding. This is **not** the I2P Base64 variant
//! (`-`/`?` with `~` padding) used for router hashes; that variant
//! lives in `i2pr-netdb::base64`.
//!
//! The codec is strict and bounded:
//!
//! - rejects characters outside the RFC 4648 alphabet;
//! - rejects unpadded inputs that are not a multiple of four;
//! - rejects invalid padding positions;
//! - rejects inputs whose decoded length exceeds a caller-supplied
//!   ceiling.

use thiserror::Error;

/// Errors emitted by the SAM Base64 codec.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SamBase64Error {
    /// The input length was not a multiple of four.
    #[error("sam base64 input length {actual} is not a multiple of 4")]
    InvalidLength {
        /// Actual length.
        actual: usize,
    },
    /// The input contained a byte outside the RFC 4648 alphabet.
    #[error("sam base64 character {byte:#x} at index {index} is outside the alphabet")]
    InvalidCharacter {
        /// Index of the offending byte.
        index: usize,
        /// Offending byte value.
        byte: u8,
    },
    /// Padding (`=`) appeared at an index that is not allowed by the
    /// strict variant.
    #[error("sam base64 padding at index {index} is not in the final quantised position")]
    InvalidPadding {
        /// Index of the offending padding byte.
        index: usize,
    },
    /// The decoded length would exceed the supplied ceiling.
    #[error("sam base64 decoded length {actual} exceeds {maximum}")]
    DecodedTooLarge {
        /// Actual decoded length.
        actual: usize,
        /// Maximum accepted.
        maximum: usize,
    },
}

const ENCODE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode_alphabet() -> [u8; 256] {
    let mut table = [0xFF_u8; 256];
    for (index, byte) in ENCODE.iter().enumerate() {
        table[*byte as usize] = index as u8;
    }
    table
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

/// Encodes `bytes` as RFC 4648 Base64 with `=` padding. The output
/// length is always `4 * ceil(bytes.len() / 3)`.
pub fn encode(bytes: &[u8]) -> String {
    let total = bytes.len();
    let tail = total % 3;
    let mut out = String::with_capacity(total.div_ceil(3) * 4);
    let main_end = if tail == 0 { total } else { total - tail };
    for chunk in bytes[..main_end].chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        let n = ((u32::from(b0)) << 16) | ((u32::from(b1)) << 8) | u32::from(b2);
        let c0 = ENCODE[((n >> 18) & 0x3F) as usize];
        let c1 = ENCODE[((n >> 12) & 0x3F) as usize];
        let c2 = ENCODE[((n >> 6) & 0x3F) as usize];
        let c3 = ENCODE[(n & 0x3F) as usize];
        out.push(c0 as char);
        out.push(c1 as char);
        out.push(c2 as char);
        out.push(c3 as char);
    }
    if tail == 2 {
        let b0 = bytes[total - 2];
        let b1 = bytes[total - 1];
        let n = ((u32::from(b0)) << 16) | ((u32::from(b1)) << 8);
        let c0 = ENCODE[((n >> 18) & 0x3F) as usize];
        let c1 = ENCODE[((n >> 12) & 0x3F) as usize];
        let c2 = ENCODE[((n >> 6) & 0x3F) as usize];
        out.push(c0 as char);
        out.push(c1 as char);
        out.push(c2 as char);
        out.push('=');
    } else if tail == 1 {
        let b0 = bytes[total - 1];
        let n = u32::from(b0) << 16;
        let c0 = ENCODE[((n >> 18) & 0x3F) as usize];
        let c1 = ENCODE[((n >> 12) & 0x3F) as usize];
        out.push(c0 as char);
        out.push(c1 as char);
        out.push('=');
        out.push('=');
    }
    out
}

/// Decodes a strict RFC 4648 Base64 string into bytes. The supplied
/// `maximum` ceiling bounds the decoded length.
pub fn decode(input: &str, maximum: usize) -> Result<Vec<u8>, SamBase64Error> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(SamBase64Error::InvalidLength {
            actual: bytes.len(),
        });
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let mut accum: u32 = 0;
        let mut pad_count = 0;
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                if index != 2 && index != 3 {
                    return Err(SamBase64Error::InvalidPadding { index });
                }
                pad_count += 1;
                accum <<= 6;
                continue;
            }
            if pad_count > 0 {
                return Err(SamBase64Error::InvalidPadding { index });
            }
            let value = alphabet_value(*byte)
                .ok_or(SamBase64Error::InvalidCharacter { index, byte: *byte })?;
            accum = (accum << 6) | value;
        }
        let tail = 3 - pad_count;
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
        if out.len() > maximum {
            return Err(SamBase64Error::DecodedTooLarge {
                actual: out.len(),
                maximum,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_known_rfc4648_vector() {
        let encoded = encode(b"foobar");
        assert_eq!(encoded, "Zm9vYmFy");
        let decoded = decode(&encoded, 64).expect("decode");
        assert_eq!(decoded, b"foobar");
    }

    #[test]
    fn round_trip_for_each_tail_length() {
        for bytes in [
            b"" as &[u8],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
        ] {
            let encoded = encode(bytes);
            let decoded = decode(&encoded, 64).expect("decode");
            assert_eq!(decoded, bytes);
        }
    }

    #[test]
    fn padding_with_equals_is_required() {
        let encoded_short = encode(b"f");
        assert!(encoded_short.ends_with("=="));
        let encoded_two = encode(b"fo");
        assert!(encoded_two.ends_with('='));
        assert!(!encoded_two.ends_with("=="));
    }

    #[test]
    fn unknown_character_is_rejected() {
        let error = decode("AB*D", 64).unwrap_err();
        assert!(matches!(error, SamBase64Error::InvalidCharacter { .. }));
    }

    #[test]
    fn wrong_length_is_rejected() {
        let error = decode("ABC", 64).unwrap_err();
        assert!(matches!(error, SamBase64Error::InvalidLength { .. }));
    }

    #[test]
    fn decoded_length_ceiling_is_enforced() {
        let encoded = encode(&[0x42_u8; 16]);
        let error = decode(&encoded, 8).unwrap_err();
        assert!(matches!(
            error,
            SamBase64Error::DecodedTooLarge { maximum: 8, .. }
        ));
    }

    #[test]
    fn padding_in_middle_of_chunk_is_rejected() {
        // A chunk where `=` appears at index 1 (not a valid padding
        // position) is rejected.
        let error = decode("A=BC", 64).unwrap_err();
        assert!(matches!(error, SamBase64Error::InvalidPadding { .. }));
    }
}
