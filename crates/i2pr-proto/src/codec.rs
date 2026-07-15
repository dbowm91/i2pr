//! Bounded primitive codec mechanics for protocol-facing crates.
//!
//! The cursor borrows its input and never copies bytes while decoding. Every
//! length limit is supplied by the caller before a length-prefixed value is
//! taken, and owned output is written only after the caller-visible output
//! limit has been checked. Fixed-width integers use network byte order
//! (big-endian). String lengths count UTF-8 bytes, not Unicode scalar values.
//!
//! [`decode_exact`] is the strict top-level entry point: successful decoders
//! must consume the complete input. These mechanics intentionally do not
//! implement common I2P structures or protocol support claims. Their error
//! categories follow [`specs/CONFORMANCE.md`](../../../specs/CONFORMANCE.md).

use std::fmt;
use std::str;

use crate::ProtocolErrorKind;

/// A structured, bounded error from primitive decoding or encoding.
///
/// Context is a static category chosen by the implementation. Errors never
/// retain or print attacker-controlled payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    /// The input ended before the requested field was complete.
    Truncated {
        /// Offset at which the incomplete field began.
        offset: usize,
        /// Number of bytes requested by the operation.
        needed: usize,
        /// Number of bytes available at `offset`.
        remaining: usize,
    },
    /// A declared or produced length exceeds an explicit caller limit.
    LengthExceeded {
        /// Offset associated with the declaration or output operation.
        offset: usize,
        /// Declared or requested length.
        declared: usize,
        /// Caller-provided maximum length.
        maximum: usize,
        /// Static field category.
        context: &'static str,
    },
    /// A checked offset, length, or output-size calculation overflowed.
    ArithmeticOverflow {
        /// Offset at which the calculation was attempted.
        offset: usize,
        /// Static description of the calculation.
        context: &'static str,
    },
    /// A byte sequence was not valid UTF-8.
    InvalidUtf8 {
        /// Offset of the UTF-8 field bytes.
        offset: usize,
    },
    /// A value cannot be represented by the selected protocol field.
    InvalidFieldValue {
        /// Offset associated with the field.
        offset: usize,
        /// Static field category.
        context: &'static str,
    },
    /// The input is structurally valid but not in the required canonical form.
    NonCanonical {
        /// Offset associated with the noncanonical field.
        offset: usize,
        /// Static field category.
        context: &'static str,
    },
    /// A requested type or algorithm is not implemented by this crate.
    Unsupported {
        /// Offset associated with the type or algorithm identifier.
        offset: usize,
        /// Static type category.
        context: &'static str,
        /// Numeric identifier, when one is available.
        value: u64,
    },
    /// A strict top-level decoder found bytes after the expected value.
    TrailingBytes {
        /// Offset at which trailing bytes begin.
        offset: usize,
        /// Number of trailing bytes.
        remaining: usize,
    },
    /// A field or mapping key was repeated where uniqueness is required.
    DuplicateField {
        /// Offset of the duplicate field.
        offset: usize,
        /// Static field category.
        context: &'static str,
    },
    /// A structurally valid value was rejected by an explicit policy bound.
    PolicyRejected {
        /// Offset associated with the policy decision.
        offset: usize,
        /// Static policy category.
        context: &'static str,
    },
}

impl CodecError {
    /// Returns the broad bootstrap category corresponding to this error.
    pub const fn kind(self) -> ProtocolErrorKind {
        match self {
            Self::LengthExceeded { .. } | Self::PolicyRejected { .. } => {
                ProtocolErrorKind::LimitExceeded
            }
            Self::Unsupported { .. } => ProtocolErrorKind::Unsupported,
            Self::Truncated { .. }
            | Self::ArithmeticOverflow { .. }
            | Self::InvalidUtf8 { .. }
            | Self::InvalidFieldValue { .. }
            | Self::NonCanonical { .. }
            | Self::TrailingBytes { .. }
            | Self::DuplicateField { .. } => ProtocolErrorKind::Malformed,
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated {
                offset,
                needed,
                remaining,
            } => write!(
                formatter,
                "truncated input at offset {offset}: need {needed} bytes, have {remaining}"
            ),
            Self::LengthExceeded {
                offset,
                declared,
                maximum,
                context,
            } => write!(
                formatter,
                "{context} length {declared} exceeds {maximum}-byte limit at offset {offset}"
            ),
            Self::ArithmeticOverflow { offset, context } => {
                write!(formatter, "checked {context} overflow at offset {offset}")
            }
            Self::InvalidUtf8 { offset } => {
                write!(formatter, "invalid UTF-8 at offset {offset}")
            }
            Self::InvalidFieldValue { offset, context } => {
                write!(formatter, "invalid {context} value at offset {offset}")
            }
            Self::NonCanonical { offset, context } => {
                write!(formatter, "noncanonical {context} at offset {offset}")
            }
            Self::Unsupported {
                offset,
                context,
                value,
            } => write!(
                formatter,
                "unsupported {context} value {value} at offset {offset}"
            ),
            Self::TrailingBytes { offset, remaining } => write!(
                formatter,
                "trailing bytes at offset {offset}: {remaining} bytes remain"
            ),
            Self::DuplicateField { offset, context } => {
                write!(formatter, "duplicate {context} at offset {offset}")
            }
            Self::PolicyRejected { offset, context } => {
                write!(formatter, "policy rejected {context} at offset {offset}")
            }
        }
    }
}

impl std::error::Error for CodecError {}

/// A read-only cursor over a borrowed protocol byte slice.
#[derive(Clone, Copy, Debug)]
pub struct DecodeCursor<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> DecodeCursor<'a> {
    /// Creates a cursor at offset zero after enforcing `maximum` input bytes.
    pub fn new(input: &'a [u8], maximum: usize) -> Result<Self, CodecError> {
        if input.len() > maximum {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: input.len(),
                maximum,
                context: "cursor input",
            });
        }
        Ok(Self { input, offset: 0 })
    }

    /// Returns the current byte offset.
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the number of bytes remaining without panicking on a synthetic
    /// out-of-range test offset.
    pub fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.offset)
    }

    /// Returns whether no bytes remain.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Takes exactly `length` bytes without copying them.
    pub fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CodecError::ArithmeticOverflow {
                offset: self.offset,
                context: "cursor offset",
            })?;

        if end > self.input.len() {
            return Err(CodecError::Truncated {
                offset: self.offset,
                needed: length,
                remaining: self.remaining(),
            });
        }

        let start = self.offset;
        self.offset = end;
        Ok(&self.input[start..end])
    }

    /// Reads an unsigned one-byte integer.
    pub fn read_u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    /// Reads an unsigned two-byte big-endian integer.
    pub fn read_u16(&mut self) -> Result<u16, CodecError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Reads an unsigned four-byte big-endian integer.
    pub fn read_u32(&mut self) -> Result<u32, CodecError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads an unsigned eight-byte big-endian integer.
    pub fn read_u64(&mut self) -> Result<u64, CodecError> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_length_prefixed(
        &mut self,
        length: usize,
        maximum: usize,
    ) -> Result<&'a [u8], CodecError> {
        if length > maximum {
            return Err(CodecError::LengthExceeded {
                offset: self.offset,
                declared: length,
                maximum,
                context: "length-prefixed field",
            });
        }
        self.take(length)
    }

    /// Reads an 8-bit-length-prefixed byte slice under `maximum`.
    pub fn read_bytes_u8(&mut self, maximum: usize) -> Result<&'a [u8], CodecError> {
        let length = usize::from(self.read_u8()?);
        self.read_length_prefixed(length, maximum)
    }

    /// Reads a 16-bit-length-prefixed byte slice under `maximum`.
    pub fn read_bytes_u16(&mut self, maximum: usize) -> Result<&'a [u8], CodecError> {
        let length = usize::from(self.read_u16()?);
        self.read_length_prefixed(length, maximum)
    }

    /// Reads a 32-bit-length-prefixed byte slice under `maximum`.
    pub fn read_bytes_u32(&mut self, maximum: usize) -> Result<&'a [u8], CodecError> {
        let length =
            usize::try_from(self.read_u32()?).map_err(|_| CodecError::ArithmeticOverflow {
                offset: self.offset,
                context: "32-bit length conversion",
            })?;
        self.read_length_prefixed(length, maximum)
    }

    /// Reads an 8-bit-length-prefixed UTF-8 string under `maximum` bytes.
    pub fn read_utf8_u8(&mut self, maximum: usize) -> Result<&'a str, CodecError> {
        let bytes = self.read_bytes_u8(maximum)?;
        Self::read_utf8(bytes, self.offset.saturating_sub(bytes.len()))
    }

    /// Reads a 16-bit-length-prefixed UTF-8 string under `maximum` bytes.
    pub fn read_utf8_u16(&mut self, maximum: usize) -> Result<&'a str, CodecError> {
        let bytes = self.read_bytes_u16(maximum)?;
        Self::read_utf8(bytes, self.offset.saturating_sub(bytes.len()))
    }

    /// Reads a 32-bit-length-prefixed UTF-8 string under `maximum` bytes.
    pub fn read_utf8_u32(&mut self, maximum: usize) -> Result<&'a str, CodecError> {
        let bytes = self.read_bytes_u32(maximum)?;
        Self::read_utf8(bytes, self.offset.saturating_sub(bytes.len()))
    }

    fn read_utf8(bytes: &'a [u8], offset: usize) -> Result<&'a str, CodecError> {
        str::from_utf8(bytes).map_err(|_| CodecError::InvalidUtf8 { offset })
    }

    /// Requires that the cursor consumed the complete input.
    pub fn finish(self) -> Result<(), CodecError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes {
                offset: self.offset,
                remaining: self.remaining(),
            })
        }
    }
}

/// Strictly decodes one value and rejects input beyond that value.
///
/// `maximum` is mandatory at the call site so a protocol layer cannot
/// accidentally hide an unlimited input policy in this helper.
pub fn decode_exact<'a, T, F>(input: &'a [u8], maximum: usize, decode: F) -> Result<T, CodecError>
where
    F: FnOnce(&mut DecodeCursor<'a>) -> Result<T, CodecError>,
{
    let mut cursor = DecodeCursor::new(input, maximum)?;
    let value = decode(&mut cursor)?;
    cursor.finish()?;
    Ok(value)
}

/// A bounded encoder that appends to a caller-provided byte vector.
#[derive(Debug)]
pub struct EncodeBuffer<'a> {
    output: &'a mut Vec<u8>,
    start_len: usize,
    maximum: usize,
}

impl<'a> EncodeBuffer<'a> {
    /// Creates an encoder whose total output length cannot exceed `maximum`.
    pub fn new(output: &'a mut Vec<u8>, maximum: usize) -> Result<Self, CodecError> {
        if output.len() > maximum {
            return Err(CodecError::LengthExceeded {
                offset: output.len(),
                declared: output.len(),
                maximum,
                context: "encoder output",
            });
        }
        Ok(Self {
            start_len: output.len(),
            output,
            maximum,
        })
    }

    /// Returns the number of bytes appended by this encoder.
    pub fn len(&self) -> usize {
        self.output.len() - self.start_len
    }

    /// Returns whether no bytes have been appended by this encoder.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        let new_length =
            self.output
                .len()
                .checked_add(bytes.len())
                .ok_or(CodecError::ArithmeticOverflow {
                    offset: self.output.len(),
                    context: "encoder output length",
                })?;
        if new_length > self.maximum {
            return Err(CodecError::LengthExceeded {
                offset: self.output.len(),
                declared: new_length,
                maximum: self.maximum,
                context: "encoder output",
            });
        }
        self.output.extend_from_slice(bytes);
        Ok(())
    }

    /// Writes an unsigned one-byte integer.
    pub fn write_u8(&mut self, value: u8) -> Result<(), CodecError> {
        self.write(&[value])
    }

    /// Writes an unsigned two-byte big-endian integer.
    pub fn write_u16(&mut self, value: u16) -> Result<(), CodecError> {
        self.write(&value.to_be_bytes())
    }

    /// Writes an unsigned four-byte big-endian integer.
    pub fn write_u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.write(&value.to_be_bytes())
    }

    /// Writes an unsigned eight-byte big-endian integer.
    pub fn write_u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.write(&value.to_be_bytes())
    }

    fn write_length_prefixed(
        &mut self,
        bytes: &[u8],
        maximum: usize,
        width_maximum: usize,
        context: &'static str,
    ) -> Result<(), CodecError> {
        if bytes.len() > maximum {
            return Err(CodecError::LengthExceeded {
                offset: self.output.len(),
                declared: bytes.len(),
                maximum,
                context,
            });
        }
        if bytes.len() > width_maximum {
            return Err(CodecError::InvalidFieldValue {
                offset: self.output.len(),
                context,
            });
        }

        match width_maximum {
            0xff => self.write_u8(bytes.len() as u8)?,
            0xffff => self.write_u16(bytes.len() as u16)?,
            value if value == u32::MAX as usize => self.write_u32(bytes.len() as u32)?,
            _ => {
                return Err(CodecError::InvalidFieldValue {
                    offset: self.output.len(),
                    context,
                });
            }
        }
        self.write(bytes)
    }

    /// Writes an 8-bit-length-prefixed byte slice under `maximum`.
    pub fn write_bytes_u8(&mut self, bytes: &[u8], maximum: usize) -> Result<(), CodecError> {
        self.write_length_prefixed(bytes, maximum, u8::MAX as usize, "u8-prefixed field")
    }

    /// Writes a 16-bit-length-prefixed byte slice under `maximum`.
    pub fn write_bytes_u16(&mut self, bytes: &[u8], maximum: usize) -> Result<(), CodecError> {
        self.write_length_prefixed(bytes, maximum, u16::MAX as usize, "u16-prefixed field")
    }

    /// Writes a 32-bit-length-prefixed byte slice under `maximum`.
    pub fn write_bytes_u32(&mut self, bytes: &[u8], maximum: usize) -> Result<(), CodecError> {
        self.write_length_prefixed(bytes, maximum, u32::MAX as usize, "u32-prefixed field")
    }

    /// Writes an 8-bit-length-prefixed UTF-8 string under `maximum` bytes.
    pub fn write_utf8_u8(&mut self, value: &str, maximum: usize) -> Result<(), CodecError> {
        self.write_length_prefixed(value.as_bytes(), maximum, u8::MAX as usize, "UTF-8 field")
    }

    /// Writes a 16-bit-length-prefixed UTF-8 string under `maximum` bytes.
    pub fn write_utf8_u16(&mut self, value: &str, maximum: usize) -> Result<(), CodecError> {
        self.write_length_prefixed(value.as_bytes(), maximum, u16::MAX as usize, "UTF-8 field")
    }

    /// Writes a 32-bit-length-prefixed UTF-8 string under `maximum` bytes.
    pub fn write_utf8_u32(&mut self, value: &str, maximum: usize) -> Result<(), CodecError> {
        self.write_length_prefixed(value.as_bytes(), maximum, u32::MAX as usize, "UTF-8 field")
    }

    /// Returns the underlying output vector after encoding is complete.
    pub fn finish(self) -> &'a mut Vec<u8> {
        self.output
    }
}

/// Encodes into a fresh vector with an explicit total output limit.
pub fn encode_to_vec<F>(maximum: usize, encode: F) -> Result<Vec<u8>, CodecError>
where
    F: FnOnce(&mut EncodeBuffer<'_>) -> Result<(), CodecError>,
{
    let mut output = Vec::new();
    let mut encoder = EncodeBuffer::new(&mut output, maximum)?;
    encode(&mut encoder)?;
    Ok(output)
}

#[cfg(test)]
mod test_support {
    /// Returns every strict-prefix truncation of `input`.
    pub(super) fn truncation_prefixes(input: &[u8]) -> impl Iterator<Item = &[u8]> {
        (0..input.len()).map(move |end| &input[..end])
    }

    /// Appends a deterministic byte for strict trailing-byte tests.
    pub(super) fn append_trailing_byte(input: &[u8], byte: u8) -> Vec<u8> {
        let mut output = input.to_vec();
        output.push(byte);
        output
    }

    /// Flips one selected bit for mutation tests.
    pub(super) fn flip_bit(input: &[u8], index: usize, mask: u8) -> Vec<u8> {
        let mut output = input.to_vec();
        output[index] ^= mask;
        output
    }

    /// Returns a bounded deterministic byte sequence without a dependency on
    /// a production random-number generator.
    pub(super) fn bounded_deterministic_bytes(
        seed: u64,
        requested: usize,
        maximum: usize,
    ) -> Vec<u8> {
        let mut state = seed;
        (0..requested.min(maximum))
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                (state >> 56) as u8
            })
            .collect()
    }

    /// Returns a minimal invalid UTF-8 field with an 8-bit length prefix.
    pub(super) fn invalid_utf8_u8() -> Vec<u8> {
        vec![2, 0xff, 0xfe]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec, test_support,
    };
    use crate::ProtocolErrorKind;

    #[test]
    fn cursor_reads_network_order_integers() {
        let input = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde];
        let mut cursor = DecodeCursor::new(&input, input.len()).unwrap();
        assert_eq!(cursor.read_u8().unwrap(), 0x12);
        assert_eq!(cursor.read_u16().unwrap(), 0x3456);
        assert_eq!(cursor.read_u32().unwrap(), 0x789a_bcde);
        assert!(cursor.is_empty());
    }

    #[test]
    fn cursor_reads_u64_and_borrowed_bytes_without_copying() {
        let input = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let mut cursor = DecodeCursor::new(&input, input.len()).unwrap();
        assert_eq!(cursor.read_u64().unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(cursor.offset(), input.len());
    }

    #[test]
    fn cursor_constructor_enforces_explicit_input_limit() {
        let error = DecodeCursor::new(&[1, 2, 3], 2).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));
    }

    #[test]
    fn every_truncation_prefix_is_classified_as_truncated() {
        let encoded = [0, 0, 0, 7, 3, b'a', b'b', b'c'];
        for prefix in test_support::truncation_prefixes(&encoded) {
            let error = decode_exact(prefix, encoded.len(), |cursor| {
                cursor.read_u32()?;
                cursor.read_bytes_u8(3).map(|_| ())
            })
            .unwrap_err();
            assert!(matches!(error, CodecError::Truncated { .. }), "{error}");
        }
    }

    #[test]
    fn empty_input_never_panics_and_is_truncated() {
        let error = decode_exact(&[], 0, |cursor| cursor.read_u8()).unwrap_err();
        assert!(matches!(error, CodecError::Truncated { .. }));
    }

    #[test]
    fn checked_offset_overflow_is_reported() {
        let mut cursor = DecodeCursor {
            input: &[],
            offset: usize::MAX - 1,
        };
        let error = cursor.take(2).unwrap_err();
        assert_eq!(
            error,
            CodecError::ArithmeticOverflow {
                offset: usize::MAX - 1,
                context: "cursor offset"
            }
        );
        assert_eq!(cursor.remaining(), 0);
    }

    #[test]
    fn bounded_length_prefixed_bytes_reject_oversized_declarations() {
        let mut cursor = DecodeCursor::new(&[4, 1, 2, 3, 4], 5).unwrap();
        let error = cursor.read_bytes_u8(3).unwrap_err();
        assert!(matches!(
            error,
            CodecError::LengthExceeded {
                declared: 4,
                maximum: 3,
                ..
            }
        ));

        let mut cursor = DecodeCursor::new(&[0, 4, 1, 2, 3, 4], 6).unwrap();
        let error = cursor.read_bytes_u16(3).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));
    }

    #[test]
    fn bounded_u32_length_conversion_and_input_limit_are_checked() {
        let mut cursor = DecodeCursor::new(&[0xff, 0xff, 0xff, 0xff], 4).unwrap();
        let error = cursor.read_bytes_u32(4).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));

        let mut cursor = DecodeCursor::new(&[0, 0, 0, 5, 1, 2, 3, 4, 5], 9).unwrap();
        let error = cursor.read_bytes_u32(4).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));

        let error = decode_exact(&[1, 2, 3], 2, |_| Ok(())).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));
    }

    #[test]
    fn utf8_helpers_validate_bytes_after_checking_length() {
        let mut cursor = DecodeCursor::new(&[2, 0xc3, 0xa9], 3).unwrap();
        assert_eq!(cursor.read_utf8_u8(2).unwrap(), "é");

        let invalid = test_support::invalid_utf8_u8();
        let mut cursor = DecodeCursor::new(&invalid, invalid.len()).unwrap();
        let error = cursor.read_utf8_u8(2).unwrap_err();
        assert!(matches!(error, CodecError::InvalidUtf8 { offset: 1 }));
    }

    #[test]
    fn strict_decode_rejects_trailing_bytes() {
        let input = [0x2a];
        let with_trailing = test_support::append_trailing_byte(&input, 0x99);
        let error = decode_exact(&with_trailing, 2, |cursor| cursor.read_u8()).unwrap_err();
        assert_eq!(
            error,
            CodecError::TrailingBytes {
                offset: 1,
                remaining: 1
            }
        );
    }

    #[test]
    fn encoder_writes_deterministic_big_endian_bytes() {
        let output = encode_to_vec(16, |encoder| {
            encoder.write_u8(0x12)?;
            encoder.write_u16(0x3456)?;
            encoder.write_u32(0x789a_bcde)?;
            encoder.write_u64(0xfedc_ba98_7654_3210)
        })
        .unwrap();
        assert_eq!(
            output,
            [
                0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32,
                0x10
            ]
        );
    }

    #[test]
    fn encoder_counts_exact_output_and_rejects_output_limit() {
        let mut output = Vec::new();
        let mut encoder = EncodeBuffer::new(&mut output, 3).unwrap();
        encoder.write_u16(0x1234).unwrap();
        assert_eq!(encoder.len(), 2);
        let error = encoder.write_u16(0x5678).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));
        assert_eq!(output, [0x12, 0x34]);
    }

    #[test]
    fn every_length_prefixed_encoder_checks_caller_limit() {
        let bytes = [1, 2, 3];
        let output = encode_to_vec(4, |encoder| encoder.write_bytes_u8(&bytes, 3)).unwrap();
        assert_eq!(output, [3, 1, 2, 3]);

        let error = encode_to_vec(4, |encoder| encoder.write_bytes_u8(&bytes, 2)).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));

        let output = encode_to_vec(5, |encoder| encoder.write_bytes_u16(&bytes, 3)).unwrap();
        assert_eq!(output, [0, 3, 1, 2, 3]);

        let error = encode_to_vec(5, |encoder| encoder.write_bytes_u16(&bytes, 2)).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));

        let output = encode_to_vec(7, |encoder| encoder.write_bytes_u32(&bytes, 3)).unwrap();
        assert_eq!(output, [0, 0, 0, 3, 1, 2, 3]);

        let error = encode_to_vec(7, |encoder| encoder.write_bytes_u32(&bytes, 2)).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));
    }

    #[test]
    fn length_prefix_width_rejects_unrepresentable_values() {
        let too_long = vec![0_u8; 256];
        let error =
            encode_to_vec(300, |encoder| encoder.write_bytes_u8(&too_long, 300)).unwrap_err();
        assert!(matches!(error, CodecError::InvalidFieldValue { .. }));

        let too_long = vec![0_u8; 65_536];
        let error = encode_to_vec(70_000, |encoder| encoder.write_bytes_u16(&too_long, 70_000))
            .unwrap_err();
        assert!(matches!(error, CodecError::InvalidFieldValue { .. }));
    }

    #[test]
    fn encoder_utf8_lengths_count_bytes() {
        let output = encode_to_vec(3, |encoder| encoder.write_utf8_u8("é", 2)).unwrap();
        assert_eq!(output, [2, 0xc3, 0xa9]);

        let error = encode_to_vec(3, |encoder| encoder.write_utf8_u8("é", 1)).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));

        let output = encode_to_vec(6, |encoder| encoder.write_utf8_u16("ok", 2)).unwrap();
        assert_eq!(output, [0, 2, b'o', b'k']);
        let error = encode_to_vec(4, |encoder| encoder.write_utf8_u16("ok", 1)).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));

        let output = encode_to_vec(8, |encoder| encoder.write_utf8_u32("ok", 2)).unwrap();
        assert_eq!(output, [0, 0, 0, 2, b'o', b'k']);
        let error = encode_to_vec(6, |encoder| encoder.write_utf8_u32("ok", 1)).unwrap_err();
        assert!(matches!(error, CodecError::LengthExceeded { .. }));
    }

    #[test]
    fn test_support_bit_mutation_is_deterministic() {
        assert_eq!(test_support::flip_bit(&[0x00, 0x01], 1, 0x04), [0x00, 0x05]);
        assert_eq!(test_support::bounded_deterministic_bytes(7, 5, 3).len(), 3);
    }

    #[test]
    fn all_error_categories_have_safe_stable_display_and_broad_kind() {
        let errors = [
            CodecError::Truncated {
                offset: 1,
                needed: 2,
                remaining: 0,
            },
            CodecError::LengthExceeded {
                offset: 1,
                declared: 4,
                maximum: 3,
                context: "field",
            },
            CodecError::ArithmeticOverflow {
                offset: 1,
                context: "offset",
            },
            CodecError::InvalidUtf8 { offset: 1 },
            CodecError::InvalidFieldValue {
                offset: 1,
                context: "field",
            },
            CodecError::NonCanonical {
                offset: 1,
                context: "field",
            },
            CodecError::Unsupported {
                offset: 1,
                context: "algorithm",
                value: 7,
            },
            CodecError::TrailingBytes {
                offset: 1,
                remaining: 1,
            },
            CodecError::DuplicateField {
                offset: 1,
                context: "key",
            },
            CodecError::PolicyRejected {
                offset: 1,
                context: "timestamp",
            },
        ];

        for error in errors {
            let message = error.to_string();
            assert!(!message.contains("["));
            assert_eq!(format!("{error}"), message);
        }
        assert_eq!(errors[0].kind(), ProtocolErrorKind::Malformed);
        assert_eq!(errors[1].kind(), ProtocolErrorKind::LimitExceeded);
        assert_eq!(errors[6].kind(), ProtocolErrorKind::Unsupported);
        assert_eq!(errors[9].kind(), ProtocolErrorKind::LimitExceeded);
    }

    #[test]
    fn decoder_and_encoder_round_trip_bounded_primitives() {
        let encoded = encode_to_vec(16, |encoder| {
            encoder.write_u16(0x1234)?;
            encoder.write_utf8_u8("ok", 2)
        })
        .unwrap();
        let decoded = decode_exact(&encoded, 16, |cursor| {
            let number = cursor.read_u16()?;
            let text = cursor.read_utf8_u8(2)?;
            Ok((number, text))
        })
        .unwrap();
        assert_eq!(decoded, (0x1234, "ok"));
    }
}
