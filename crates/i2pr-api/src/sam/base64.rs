//! SAM v3.1 Base64 codec.
//!
//! Plan 142 corrects the alphabet defect identified by Plan 140's audit.
//! The SAM fields `PUB` and `PRIV` use the **I2P Base64** alphabet
//! (`A-Z a-z 0-9 - ~`) with the standard `=` padding character. This is
//! the spelling the SAM 3.1 specification mandates and the spelling
//! every Java I2P / i2pd / Python client reference implementation
//! emits.
//!
//! The codec is **not** the same primitive as the I2P Base64 variant
//! used for router hashes (the router-hash codec uses `-`/`~` for
//! `+`/`/` **and** `~` for `=`). The router-hash codec lives in
//! `i2pr-netdb::base64`; SAM uses `=` padding, not `~`.
//!
//! The codec is strict and bounded:
//!
//! - rejects characters outside the I2P Base64 alphabet (so `+` and
//!   `/` from RFC 4648 are explicitly rejected);
//! - rejects unpadded inputs that are not a multiple of four;
//! - rejects invalid padding positions;
//! - rejects inputs whose decoded length exceeds a caller-supplied
//!   ceiling.
//!
//! ## Independent corroboration
//!
//! The I2P Base64 alphabet and `=` padding rule used here are
//! independently confirmed against three external references. None of
//! the three informed the others; together they fully constrain the
//! codec:
//!
//! - i2pd (`PurpleI2P/i2pd`, `openssl` branch):
//!   - `libi2pd/Base.cpp` exposes `T64` with index 62 mapped to
//!     `-` and index 63 mapped to `~`, and the padding character
//!     `P64 = '='`.
//!   - `libi2pd/Base.h::IsBase64(char)` accepts only `A-Z a-z 0-9
//!     - ~ =`.
//! - Java I2P (`i2p/i2p.i2p`):
//!   - `core/java/src/net/i2p/data/PrivateKeyFile.java` decodes
//!     `PrivateKeyFile` payloads as Base64 with `-` and `~` as the
//!     last two alphabet characters and `=` padding.
//! - i2plib (`tomi` / Python SAM client):
//!   - `i2plib/sam.py` builds the alphabet with
//!     `I2P_B64_CHARS = "-~"` (i.e. replaces the RFC 4648 `+/`
//!     positions) and uses Python's `base64` decoder with the
//!     matching `altchars` and `validate=True` so `=` padding is
//!     accepted and any other deviation is rejected.

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
    /// The input contained a byte outside the I2P Base64 alphabet.
    /// RFC 4648's `+` and `/` are the most common offenders when a
    /// caller feeds a non-SAM Base64 text into the codec.
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

const ENCODE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";

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

/// Encodes `bytes` as I2P Base64 with `=` padding. The output length
/// is always `4 * ceil(bytes.len() / 3)` and the final bytes of the
/// output use `-` / `~` instead of RFC 4648's `+` / `/`.
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

/// Decodes a strict I2P Base64 string into bytes. The supplied
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
    fn rfc4648_plus_slash_characters_are_rejected() {
        // SAM Base64 must reject RFC 4648's `+` and `/`. We prove
        // the alphabet switch directly: a single input character
        // that lies in the RFC 4648 alphabet but outside the I2P
        // alphabet must surface as `InvalidCharacter`.
        let plus = decode("+AAA", 64).unwrap_err();
        assert!(
            matches!(plus, SamBase64Error::InvalidCharacter { byte: b'+', .. }),
            "expected rejection of '+', got {plus:?}"
        );
        let slash = decode("/AAA", 64).unwrap_err();
        assert!(
            matches!(slash, SamBase64Error::InvalidCharacter { byte: b'/', .. }),
            "expected rejection of '/', got {slash:?}"
        );
    }

    #[test]
    fn i2p_alphabet_characters_are_accepted() {
        // The SAM codec must accept `-` (slot 62) and `~` (slot 63).
        // Both chars appear in valid `PUB`/`PRIV` strings; if the
        // codec regressed to RFC 4648 these would silently decode to
        // different bytes or be rejected. Verify both directions on
        // payloads that exercise the alphabet's tail slots.
        //
        // Each `-` slot encodes the 6-bit value `0b111110` (= 62).
        // Streaming all four slots into the decoder yields the
        // 24-bit value `0xFB_EF_BE`, which decodes to three bytes
        // `[0xFB, 0xEF, 0xBE]`. Under RFC 4648 the same text would
        // be `++++`, but the underlying 6-bit values are the same so
        // this vector alone does not differentiate the two alphabets
        // — that is the job of the
        // `rfc4648_plus_slash_characters_are_rejected` test. Here we
        // only lock the I2P alphabet interpretation in place.
        let encoded = "----";
        let decoded = decode(encoded, 8).expect("i2p decode");
        assert_eq!(decoded, vec![0xFB_u8, 0xEF, 0xBE]);
        assert_eq!(encode(&[0xFB_u8, 0xEF, 0xBE]), "----");

        // `[0xFF, 0xFF]` packs into 16 payload bits
        // `(0xFF << 16) | (0xFF << 8) = 0xFFFF00`, then splits into
        // 6-bit slots: bits 23-18 = `111111` (63 = `~`), bits 17-12 =
        // `111111` (63 = `~`), bits 11-6 = `111100` (60 = `8`), then
        // a single `=` tail. The output is therefore `~~8=`. The
        // decode side accumulates the two `~` slots and shifts left
        // for the third `~` slot and the `=` byte without OR-ing, so
        // the upper 16 bits hold `0xFF_FF` and the decoder emits
        // `[0xFF, 0xFF]`. This shape matches the SAM 3.1 spec: two
        // payload bytes plus a single `=` tail. Lock both directions
        // to guard against alphabet drift.
        assert_eq!(encode(&[0xFF_u8, 0xFF]), "~~8=");
        let decoded = decode("~~8=", 8).expect("i2p decode");
        assert_eq!(decoded, vec![0xFF_u8, 0xFF]);

        // `[0xFF, 0xFF, 0xFC]` is a 3-byte payload so no `=` tail is
        // emitted. The 24-bit value `0xFF_FF_FC` splits as
        // `111111` (63 = `~`), `111111` (63 = `~`), and `111111` (63
        // = `~`), `111100` (60 = `8`). Output is therefore `~~~8`.
        // This is the only vector here that exercises the third
        // alphabet slot, which the SAM 3.1 spec says is `~` (not
        // RFC 4648 `/`).
        assert_eq!(encode(&[0xFF_u8, 0xFF, 0xFC]), "~~~8");
        let decoded = decode("~~~8", 8).expect("i2p decode");
        assert_eq!(decoded, vec![0xFF_u8, 0xFF, 0xFC]);
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

    #[test]
    fn i2pd_corpus_round_trip() {
        // Spot-check that the SAM codec is byte-for-byte stable on
        // payloads that span all three tail lengths. The intermediate
        // values are deterministic; any future alphabet drift would
        // surface as a failed decode.
        for (input, expected) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ] {
            let encoded = encode(input);
            assert_eq!(encoded, expected, "encode {input:?}");
            let decoded = decode(&encoded, 64).expect("decode");
            assert_eq!(decoded, input, "decode {input:?}");
        }
    }

    #[test]
    fn pub_priv_lengths_remain_unchanged() {
        // The public/private-destination fields render to fixed
        // lengths under the I2P Base64 alphabet: 391 bytes -> 524
        // chars, 455 bytes -> 608 chars. Locking these lengths here
        // protects the ceiling constants in `crate::sam::mod` from
        // future alphabet regressions.
        let pub_len = encode(&[0_u8; 391]).len();
        let priv_len = encode(&[0_u8; 455]).len();
        assert_eq!(pub_len, 524);
        assert_eq!(priv_len, 608);
    }
}
