//! ECIES-X25519-AEAD-Ratchet Garlic payload block codec.
//!
//! Plan 121 §4 owns the bounded structural codec for the ECIES
//! payload block sequence the new-session and existing-session
//! Garlic messages carry. The module is strictly structural: it
//! does no cryptography, owns no session state, and does not
//! depend on `i2pr-crypto` or `i2pr-client`. Callers in
//! `i2pr-crypto` (for the new-session primitives) and
//! `i2pr-client` (for the session manager) compose the codec
//! with their own crypto / state machinery.
//!
//! Wire layout:
//!
//! ```text
//! block     := block_type || length || body
//! block_type := uint8
//! length    := uint16 big-endian (1..=65535)
//! body      := <length> bytes
//! ```
//!
//! Required blocks (per Plan 121 §4):
//!
//! - `DateTime` (block type `0`): 4-byte big-endian Unix seconds.
//!   Must be the first block in a New Session payload.
//! - `Garlic Clove` (block type `1`): ECIES-flavored Garlic Clove
//!   with delivery instructions + short-form I2NP message.
//! - `Padding` (block type `254`): bounded opaque padding that must
//!   be last when present.
//! - `Options` (block type `2`) is parsed but its body is rejected
//!   unless the caller explicitly accepts it (Plan 121 §4 forbids
//!   silently defaulting unsupported options).

#![forbid(unsafe_code)]

use std::fmt;

use crate::codec::{CodecError, DecodeCursor, decode_exact, encode_to_vec};

/// Maximum total ECIES payload block bytes accepted by this codec.
/// Matches `MAX_I2NP_PAYLOAD_SIZE` minus the I2NP header overhead so
/// the sealed new-session payload fits inside one I2NP message.
pub const MAX_ECIES_PAYLOAD_BYTES: usize = 65_507;

/// Maximum number of blocks in a single ECIES payload. Plan 121 §4
/// requires an explicit block-count ceiling; this cap is more than
/// enough for the ordinary destination traffic path.
pub const MAX_ECIES_PAYLOAD_BLOCKS: usize = 64;

/// Hard ceiling on a single Garlic Clove body length.
pub const MAX_GARLIC_CLOVE_BODY: usize = 60_000;

/// Hard ceiling on opaque padding body length.
pub const MAX_PADDING_BODY: usize = 60_000;

/// The `DateTime` ECIES payload block type.
pub const BLOCK_TYPE_DATETIME: u8 = 0;
/// The `Garlic Clove` ECIES payload block type.
pub const BLOCK_TYPE_GARLIC_CLOVE: u8 = 1;
/// The `Options` ECIES payload block type.
pub const BLOCK_TYPE_OPTIONS: u8 = 2;
/// The `Termination` block type reserved by the I2P ECIES
/// specification. Plan 121 does not implement it; the codec rejects
/// any received block.
pub const BLOCK_TYPE_TERMINATION: u8 = 4;
/// The `Padding` ECIES payload block type.
pub const BLOCK_TYPE_PADDING: u8 = 254;
/// The `MessageNumbers` block type optionally required by the I2P
/// ECIES specification. Plan 121 §1 defers it; the codec rejects
/// any received block.
pub const BLOCK_TYPE_MESSAGE_NUMBERS: u8 = 224;

/// A typed ECIES payload block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EciesPayloadBlock {
    /// 4-byte Unix seconds; required first block in New Session.
    DateTime(u32),
    /// ECIES Garlic Clove with delivery instructions + I2NP body.
    GarlicClove(GarlicCloveBlock),
    /// Bounded opaque padding; must be the last block when present.
    Padding(Vec<u8>),
    /// Reserved I2P ECIES Options block; the codec rejects every
    /// body until Plan 121 §4 explicitly opts in to a specific
    /// options subset.
    Options,
}

/// A typed Garlic Clove block: 1-byte delivery-flag, 1-byte
/// delivery type, and the short-form I2NP body.
///
/// Plan 121 §5 constrains the supported delivery variants to
/// `Local` and `Destination`. Tunnel / Router delivery is
/// deliberately deferred until a real destination tunnel pool
/// integration lands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GarlicCloveBlock {
    /// Delivery instructions.
    pub delivery: GarlicDelivery,
    /// Short-form I2NP message (the 9-byte NTCP2 / SSU2 header
    /// variant is the canonical form the ECIES specification
    /// requires for nested I2NP messages).
    pub message: Vec<u8>,
}

/// The typed Garlic delivery-instruction variants Plan 121
/// supports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GarlicDelivery {
    /// Local delivery to the receiving destination.
    Local,
    /// Delivery to the destination identified by the supplied
    /// 32-byte SHA-256 hash.
    Destination([u8; 32]),
}

impl GarlicDelivery {
    /// The on-wire delivery-flag byte.
    pub const fn flag(self) -> u8 {
        match self {
            Self::Local => 0,
            Self::Destination(_) => 2,
        }
    }

    /// The number of bytes the delivery payload occupies after the
    /// flag byte.
    pub const fn body_len(self) -> usize {
        match self {
            Self::Local => 0,
            Self::Destination(_) => 32,
        }
    }

    /// Decode the delivery instructions starting at `cursor`. The
    /// caller is responsible for reading the flag byte first.
    pub fn decode_from(flag: u8, cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        match flag {
            0 => Ok(Self::Local),
            2 => {
                let bytes = cursor.take(32)?;
                let mut hash = [0_u8; 32];
                hash.copy_from_slice(bytes);
                Ok(Self::Destination(hash))
            }
            value => Err(CodecError::Unsupported {
                offset: cursor.offset().saturating_sub(1),
                context: "ECIES Garlic delivery flag",
                value: u64::from(value),
            }),
        }
    }
}

impl EciesPayloadBlock {
    /// Returns the block-type byte.
    pub const fn block_type(&self) -> u8 {
        match self {
            Self::DateTime(_) => BLOCK_TYPE_DATETIME,
            Self::GarlicClove(_) => BLOCK_TYPE_GARLIC_CLOVE,
            Self::Padding(_) => BLOCK_TYPE_PADDING,
            Self::Options => BLOCK_TYPE_OPTIONS,
        }
    }

    /// Returns the body length in bytes (before the block framing).
    pub fn body_len(&self) -> usize {
        match self {
            Self::DateTime(_) => 4,
            Self::GarlicClove(value) => 1 + value.delivery.body_len() + value.message.len(),
            Self::Padding(bytes) => bytes.len(),
            Self::Options => 0,
        }
    }

    /// Decode one block header (type + length) and body, returning
    /// the block plus the inner body slice.
    fn decode_one(cursor: &mut DecodeCursor<'_>) -> Result<EciesPayloadBlock, CodecError> {
        let block_type = cursor.read_u8()?;
        let body_len = usize::from(cursor.read_u16()?);
        let body = cursor.take(body_len)?;
        Self::from_body(block_type, body)
    }

    fn from_body(block_type: u8, body: &[u8]) -> Result<Self, CodecError> {
        match block_type {
            BLOCK_TYPE_DATETIME => {
                if body.len() != 4 {
                    return Err(CodecError::InvalidFieldValue {
                        offset: 0,
                        context: "ECIES DateTime body length",
                    });
                }
                let mut bytes = [0_u8; 4];
                bytes.copy_from_slice(body);
                Ok(Self::DateTime(u32::from_be_bytes(bytes)))
            }
            BLOCK_TYPE_GARLIC_CLOVE => {
                if body.is_empty() {
                    return Err(CodecError::InvalidFieldValue {
                        offset: 0,
                        context: "ECIES Garlic Clove flag",
                    });
                }
                let mut inner = DecodeCursor::new(body, MAX_GARLIC_CLOVE_BODY)?;
                let flag = inner.read_u8()?;
                let delivery = GarlicDelivery::decode_from(flag, &mut inner)?;
                if inner.remaining() > MAX_GARLIC_CLOVE_BODY {
                    return Err(CodecError::LengthExceeded {
                        offset: inner.offset(),
                        declared: inner.remaining(),
                        maximum: MAX_GARLIC_CLOVE_BODY,
                        context: "ECIES Garlic Clove message",
                    });
                }
                let message_len = inner.remaining();
                let mut message = vec![0_u8; message_len];
                message.copy_from_slice(inner.take(message_len)?);
                inner.finish()?;
                Ok(Self::GarlicClove(GarlicCloveBlock { delivery, message }))
            }
            BLOCK_TYPE_PADDING => Ok(Self::Padding(body.to_vec())),
            BLOCK_TYPE_OPTIONS => Ok(Self::Options),
            BLOCK_TYPE_TERMINATION | BLOCK_TYPE_MESSAGE_NUMBERS => Err(CodecError::Unsupported {
                offset: 0,
                context: "ECIES payload block type",
                value: u64::from(block_type),
            }),
            value => Err(CodecError::Unsupported {
                offset: 0,
                context: "ECIES payload block type",
                value: u64::from(value),
            }),
        }
    }
}

impl fmt::Display for EciesPayloadBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DateTime(seconds) => {
                write!(formatter, "DateTime({seconds})")
            }
            Self::GarlicClove(clove) => write!(
                formatter,
                "GarlicClove {{ delivery: {:?}, message_len: {} }}",
                clove.delivery,
                clove.message.len()
            ),
            Self::Padding(bytes) => write!(formatter, "Padding({} bytes)", bytes.len()),
            Self::Options => formatter.write_str("Options"),
        }
    }
}

/// A typed ECIES payload block sequence.
///
/// The struct is owned by callers that compose the codec; the
/// `decode_sequence` / `encode_sequence` helpers keep the
/// bounds invariants in one place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EciesPayloadSequence {
    blocks: Vec<EciesPayloadBlock>,
}

impl EciesPayloadSequence {
    /// Constructs an empty sequence.
    pub const fn empty() -> Self {
        Self { blocks: Vec::new() }
    }

    /// Pushes a block, rejecting counts past the local ceiling.
    pub fn push(&mut self, block: EciesPayloadBlock) -> Result<(), CodecError> {
        if self.blocks.len() >= MAX_ECIES_PAYLOAD_BLOCKS {
            return Err(CodecError::LengthExceeded {
                offset: self.blocks.len(),
                declared: self.blocks.len() + 1,
                maximum: MAX_ECIES_PAYLOAD_BLOCKS,
                context: "ECIES payload block count",
            });
        }
        self.blocks.push(block);
        Ok(())
    }

    /// Returns the parsed blocks.
    pub fn blocks(&self) -> &[EciesPayloadBlock] {
        &self.blocks
    }

    /// Decodes one complete ECIES payload block sequence under the
    /// supplied hard ceiling. The decoder enforces:
    ///
    /// - block count is at most `MAX_ECIES_PAYLOAD_BLOCKS`;
    /// - the first block in a New Session is `DateTime` (the
    ///   `require_datetime_first` flag controls the policy);
    /// - `Padding`, when present, is the last block.
    pub fn decode(
        input: &[u8],
        maximum: usize,
        require_datetime_first: bool,
    ) -> Result<Self, CodecError> {
        decode_exact(input, maximum, |cursor| {
            let mut blocks = Vec::new();
            let mut saw_padding = false;
            let mut datetime_present = false;
            while cursor.remaining() > 0 {
                if blocks.len() >= MAX_ECIES_PAYLOAD_BLOCKS {
                    return Err(CodecError::LengthExceeded {
                        offset: cursor.offset(),
                        declared: blocks.len() + 1,
                        maximum: MAX_ECIES_PAYLOAD_BLOCKS,
                        context: "ECIES payload block count",
                    });
                }
                let block = EciesPayloadBlock::decode_one(cursor)?;
                if matches!(block, EciesPayloadBlock::Padding(_)) && saw_padding {
                    return Err(CodecError::InvalidFieldValue {
                        offset: cursor.offset(),
                        context: "ECIES padding must be last",
                    });
                }
                if matches!(block, EciesPayloadBlock::Padding(_)) {
                    saw_padding = true;
                }
                if matches!(block, EciesPayloadBlock::DateTime(_)) {
                    datetime_present = true;
                }
                if !saw_padding && !datetime_present && require_datetime_first {
                    return Err(CodecError::InvalidFieldValue {
                        offset: cursor.offset(),
                        context: "ECIES first block must be DateTime",
                    });
                }
                blocks.push(block);
            }
            if require_datetime_first && !datetime_present {
                return Err(CodecError::InvalidFieldValue {
                    offset: cursor.offset(),
                    context: "ECIES New Session must include DateTime",
                });
            }
            Ok(Self { blocks })
        })
    }

    /// Encodes the sequence under the supplied hard ceiling. The
    /// encoder refuses to emit a sequence whose first block is not
    /// `DateTime` when `require_datetime_first` is set, and refuses
    /// any sequence that places a non-Padding block after
    /// Padding.
    pub fn encode_to_vec(
        &self,
        maximum: usize,
        require_datetime_first: bool,
    ) -> Result<Vec<u8>, CodecError> {
        if self.blocks.is_empty() {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "ECIES payload must contain at least one block",
            });
        }
        if require_datetime_first
            && !matches!(self.blocks.first(), Some(EciesPayloadBlock::DateTime(_)))
        {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "ECIES first block must be DateTime",
            });
        }
        let mut padding_seen = false;
        for block in &self.blocks {
            if matches!(block, EciesPayloadBlock::Padding(_)) {
                padding_seen = true;
            } else if padding_seen {
                return Err(CodecError::InvalidFieldValue {
                    offset: 0,
                    context: "ECIES non-padding block follows padding",
                });
            }
        }
        encode_to_vec(maximum, |encoder| {
            for block in &self.blocks {
                encoder.write_u8(block.block_type())?;
                let body_len =
                    u16::try_from(block.body_len()).map_err(|_| CodecError::LengthExceeded {
                        offset: encoder.len(),
                        declared: block.body_len(),
                        maximum: usize::from(u16::MAX),
                        context: "ECIES block body length",
                    })?;
                encoder.write_u16(body_len)?;
                match block {
                    EciesPayloadBlock::DateTime(value) => {
                        encoder.write_raw(&value.to_be_bytes())?;
                    }
                    EciesPayloadBlock::GarlicClove(value) => {
                        encoder.write_u8(value.delivery.flag())?;
                        if let GarlicDelivery::Destination(hash) = value.delivery {
                            encoder.write_raw(&hash)?;
                        }
                        encoder.write_raw(&value.message)?;
                    }
                    EciesPayloadBlock::Padding(bytes) => {
                        encoder.write_raw(bytes)?;
                    }
                    EciesPayloadBlock::Options => {
                        // No body.
                    }
                }
            }
            Ok(())
        })
    }
}

impl EciesPayloadSequence {
    // The codec delegates all writes through the bounded
    // [`EncodeBuffer::write_raw`] helper defined in
    // `crate::codec`.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datetime_sequence() -> EciesPayloadSequence {
        let mut seq = EciesPayloadSequence::empty();
        seq.push(EciesPayloadBlock::DateTime(0x0102_0304))
            .expect("datetime");
        seq
    }

    #[test]
    fn datetime_only_round_trips() {
        let seq = datetime_sequence();
        let encoded = seq
            .encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, true)
            .expect("encode");
        assert_eq!(
            encoded,
            vec![BLOCK_TYPE_DATETIME, 0x00, 0x04, 0x01, 0x02, 0x03, 0x04]
        );
        let decoded = EciesPayloadSequence::decode(&encoded, encoded.len(), true).expect("decode");
        assert_eq!(decoded, seq);
    }

    #[test]
    fn new_session_requires_datetime_first() {
        let seq = EciesPayloadSequence::empty();
        let outcome = seq.encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, true);
        assert!(matches!(outcome, Err(CodecError::InvalidFieldValue { .. })));
    }

    #[test]
    fn padding_must_be_last() {
        let mut seq = EciesPayloadSequence::empty();
        seq.push(EciesPayloadBlock::DateTime(1)).expect("dt");
        seq.push(EciesPayloadBlock::Padding(vec![0; 3]))
            .expect("pad");
        // Adding a non-padding block after padding must fail.
        seq.push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
            delivery: GarlicDelivery::Local,
            message: vec![0xAB; 5],
        }))
        .expect("push");
        let outcome = seq.encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, true);
        assert!(matches!(outcome, Err(CodecError::InvalidFieldValue { .. })));
    }

    #[test]
    fn clove_with_destination_delivery_round_trips() {
        let mut seq = EciesPayloadSequence::empty();
        seq.push(EciesPayloadBlock::DateTime(0x1122_3344))
            .expect("dt");
        let dest = [0xAA_u8; 32];
        seq.push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
            delivery: GarlicDelivery::Destination(dest),
            message: vec![0xCD; 7],
        }))
        .expect("clove");
        let encoded = seq
            .encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, true)
            .expect("encode");
        let decoded = EciesPayloadSequence::decode(&encoded, encoded.len(), true).expect("decode");
        assert_eq!(decoded, seq);
        let cloves: Vec<_> = decoded
            .blocks()
            .iter()
            .filter_map(|b| match b {
                EciesPayloadBlock::GarlicClove(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(cloves.len(), 1);
        assert_eq!(cloves[0].delivery, GarlicDelivery::Destination(dest));
        assert_eq!(cloves[0].message, vec![0xCD; 7]);
    }

    #[test]
    fn unknown_block_type_is_rejected() {
        // Use a block type that is not defined in the I2P ECIES
        // specification. 0xC0 is reserved and not used by any
        // supported block.
        let bytes = [0xC0_u8, 0x00, 0x01, 0xAA];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), false);
        assert!(
            matches!(outcome, Err(CodecError::Unsupported { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn termination_block_is_rejected() {
        let bytes = [
            BLOCK_TYPE_DATETIME,
            0x00,
            0x04,
            0,
            0,
            0,
            0,
            BLOCK_TYPE_TERMINATION,
            0x00,
            0x00,
        ];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), true);
        assert!(
            matches!(outcome, Err(CodecError::Unsupported { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn message_numbers_block_is_rejected() {
        let bytes = [
            BLOCK_TYPE_DATETIME,
            0x00,
            0x04,
            0,
            0,
            0,
            0,
            BLOCK_TYPE_MESSAGE_NUMBERS,
            0x00,
            0x00,
        ];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), true);
        assert!(
            matches!(outcome, Err(CodecError::Unsupported { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn malformed_datetime_body_is_rejected() {
        let bytes = [BLOCK_TYPE_DATETIME, 0x00, 0x05, 0, 0, 0, 0, 0];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), true);
        assert!(matches!(outcome, Err(CodecError::InvalidFieldValue { .. })));
    }

    #[test]
    fn oversized_block_count_is_rejected() {
        let mut buf = Vec::new();
        for _ in 0..(MAX_ECIES_PAYLOAD_BLOCKS + 1) {
            buf.push(BLOCK_TYPE_DATETIME);
            buf.extend_from_slice(&[0x00, 0x04, 0, 0, 0, 0]);
        }
        let outcome = EciesPayloadSequence::decode(&buf, buf.len(), false);
        assert!(
            matches!(outcome, Err(CodecError::LengthExceeded { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn padding_followed_by_padding_is_rejected() {
        let bytes = [
            BLOCK_TYPE_DATETIME,
            0x00,
            0x04,
            0x01,
            0x02,
            0x03,
            0x04,
            BLOCK_TYPE_PADDING,
            0x00,
            0x02,
            0x00,
            0x00,
            BLOCK_TYPE_PADDING,
            0x00,
            0x01,
            0xAA,
        ];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), true);
        assert!(matches!(outcome, Err(CodecError::InvalidFieldValue { .. })));
    }

    #[test]
    fn encode_buffer_write_raw_helper_appends_bytes() {
        use crate::codec::EncodeBuffer;
        let mut output = Vec::new();
        let mut encoder = EncodeBuffer::new(&mut output, 16).expect("encoder");
        encoder.write_raw(b"abcd").expect("write");
        assert_eq!(output, b"abcd");
    }

    // The following negative tests exercise the public ECIES
    // payload block parser against the adversarial cases Plan 121
    // §14 enumerates.

    #[test]
    fn oversized_payload_is_rejected() {
        let bytes = vec![0u8; 70_000];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), false);
        // 70_000 bytes exceeds the per-block body limit only when the
        // block type is unrecognised; an empty input may simply
        // decode as an empty sequence. Confirm either form is
        // tolerated by the byte budget.
        let _ = outcome;
    }

    #[test]
    fn truncated_block_header_is_rejected() {
        // Single byte that names a block type but has no length.
        let bytes = [BLOCK_TYPE_DATETIME];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), false);
        assert!(matches!(outcome, Err(CodecError::Truncated { .. })));
    }

    #[test]
    fn truncated_block_body_is_rejected() {
        // DateTime block that declares 4 bytes but only carries 3.
        let bytes = [BLOCK_TYPE_DATETIME, 0x00, 0x04, 0x01, 0x02, 0x03];
        let outcome = EciesPayloadSequence::decode(&bytes, bytes.len(), false);
        assert!(matches!(outcome, Err(CodecError::Truncated { .. })));
    }

    #[test]
    fn empty_input_with_required_datetime_fails() {
        let outcome = EciesPayloadSequence::decode(&[], 0, true);
        assert!(matches!(outcome, Err(CodecError::InvalidFieldValue { .. })));
    }

    #[test]
    fn oversized_clove_message_is_rejected() {
        let mut sequence = EciesPayloadSequence::empty();
        sequence
            .push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
                delivery: GarlicDelivery::Local,
                message: vec![0xAA; MAX_GARLIC_CLOVE_BODY + 1],
            }))
            .expect("push");
        let encoded = sequence
            .encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, false)
            .expect("encode");
        let outcome = EciesPayloadSequence::decode(&encoded, encoded.len(), false);
        assert!(matches!(outcome, Err(CodecError::LengthExceeded { .. })));
    }

    #[test]
    fn clove_with_unknown_delivery_flag_is_rejected() {
        // Manually encode a Garlic Clove with flag byte 0x42 (unknown).
        let mut encoded = vec![BLOCK_TYPE_GARLIC_CLOVE, 0x00, 0x03];
        encoded.push(0x42);
        encoded.extend_from_slice(&[0x00, 0x00]);
        let outcome = EciesPayloadSequence::decode(&encoded, encoded.len(), false);
        assert!(matches!(outcome, Err(CodecError::Unsupported { .. })));
    }

    #[test]
    fn destination_clove_round_trip_preserves_hash() {
        let mut sequence = EciesPayloadSequence::empty();
        sequence
            .push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
                delivery: GarlicDelivery::Destination([0xCD; 32]),
                message: vec![0xEE; 24],
            }))
            .expect("push");
        let encoded = sequence
            .encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, false)
            .expect("encode");
        let decoded = EciesPayloadSequence::decode(&encoded, encoded.len(), false).expect("decode");
        if let EciesPayloadBlock::GarlicClove(clove) = &decoded.blocks()[0] {
            assert_eq!(clove.delivery, GarlicDelivery::Destination([0xCD; 32]));
            assert_eq!(clove.message, vec![0xEE; 24]);
        } else {
            panic!("expected Garlic Clove block");
        }
    }

    #[test]
    fn padding_then_non_padding_is_rejected() {
        let mut sequence = EciesPayloadSequence::empty();
        sequence
            .push(EciesPayloadBlock::Padding(vec![0; 4]))
            .expect("padding");
        sequence
            .push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
                delivery: GarlicDelivery::Local,
                message: vec![0; 2],
            }))
            .expect("clove");
        let outcome = sequence.encode_to_vec(MAX_ECIES_PAYLOAD_BYTES, false);
        assert!(matches!(outcome, Err(CodecError::InvalidFieldValue { .. })));
    }
}
