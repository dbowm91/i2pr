//! I2CP Mapping helpers over the canonical common codec.
//!
//! The I2CP specification reuses the common-structures Mapping: a
//! two-byte big-endian body length followed by `key=value;` entries
//! whose keys and values are one-byte-length-prefixed UTF-8 strings
//! (each at most 255 bytes). [`i2pr_proto::Mapping`] already enforces
//! canonical sorted-key order with duplicate rejection, which is
//! exactly the signing representation SessionConfig requires.
//!
//! Two decode postures exist because the specification differs per
//! message: SessionConfig options must be sorted (the router verifies
//! the signature over the canonical bytes), while the GetDate
//! authentication mapping and HostReply option mapping explicitly may
//! arrive unsorted. Lenient inputs are parsed pair-by-pair under the
//! same ceilings and then normalized through
//! [`Mapping::from_entries`], so every stored mapping is canonical
//! even when the wire order was not.

use i2pr_proto::{CodecError, DecodeCursor, Mapping};

use super::error::I2cpError;

/// Maximum accepted I2CP mapping body in bytes (protocol-imposed).
pub const MAX_I2CP_MAPPING_BODY_BYTES: usize = u16::MAX as usize;

/// Maximum UTF-8 byte length of one I2CP mapping key or value.
pub const MAX_I2CP_MAPPING_TEXT_BYTES: usize = u8::MAX as usize;

/// Decodes a complete mapping that must already be in canonical
/// sorted-key order (SessionConfig posture).
///
/// Returns the mapping and the consumed byte count so callers can
/// continue parsing an enclosing body.
pub fn split_mapping_strict(input: &[u8]) -> Result<(Mapping, usize), I2cpError> {
    match Mapping::decode(input, input.len().max(1)) {
        Ok(mapping) => {
            // `decode` is strict: success means the whole input was
            // exactly one canonical mapping.
            Ok((mapping, input.len()))
        }
        Err(CodecError::TrailingBytes { offset, .. }) => {
            let (prefix, _) = input.split_at(offset);
            let mapping = Mapping::decode(prefix, prefix.len().max(1))
                .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
            Ok((mapping, offset))
        }
        Err(source) => Err(I2cpError::malformed("i2cp mapping", source)),
    }
}

/// Decodes a complete mapping without requiring sorted-key wire order
/// (GetDate authentication and HostReply option posture).
///
/// Pairs are parsed under the same ceilings, then normalized through
/// [`Mapping::from_entries`]; duplicates still fail. Returns the
/// canonical mapping and the consumed byte count.
pub fn split_mapping_lenient(input: &[u8]) -> Result<(Mapping, usize), I2cpError> {
    if input.len() < 2 {
        return Err(I2cpError::malformed(
            "i2cp mapping",
            CodecError::Truncated {
                offset: 0,
                needed: 2,
                remaining: input.len(),
            },
        ));
    }
    let body_len = usize::from(u16::from_be_bytes([input[0], input[1]]));
    if body_len > MAX_I2CP_MAPPING_BODY_BYTES {
        return Err(I2cpError::malformed(
            "i2cp mapping",
            CodecError::LengthExceeded {
                offset: 0,
                declared: body_len,
                maximum: MAX_I2CP_MAPPING_BODY_BYTES,
                context: "mapping body",
            },
        ));
    }
    let total = 2usize.checked_add(body_len).ok_or(I2cpError::malformed(
        "i2cp mapping",
        CodecError::ArithmeticOverflow {
            offset: 0,
            context: "mapping total length",
        },
    ))?;
    if input.len() < total {
        return Err(I2cpError::malformed(
            "i2cp mapping",
            CodecError::Truncated {
                offset: 0,
                needed: total,
                remaining: input.len(),
            },
        ));
    }
    let body = &input[2..total];
    let mut cursor = DecodeCursor::new(body, body.len().max(1))
        .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
    // Entry wire format mirrors the canonical codec exactly
    // (one-byte-length-prefixed key, `=`, one-byte-length-prefixed
    // value, `;`); only the sorted-order requirement is lifted here.
    // Normalization through `from_entries` re-sorts and still
    // rejects duplicates.
    let mut entries = Vec::new();
    while !cursor.is_empty() {
        let key = cursor
            .read_utf8_u8(MAX_I2CP_MAPPING_TEXT_BYTES)
            .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
        let separator = cursor
            .read_u8()
            .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
        if separator != b'=' {
            return Err(I2cpError::malformed(
                "i2cp mapping",
                CodecError::InvalidFieldValue {
                    offset: cursor.offset().saturating_sub(1),
                    context: "mapping separator",
                },
            ));
        }
        let value = cursor
            .read_utf8_u8(MAX_I2CP_MAPPING_TEXT_BYTES)
            .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
        let terminator = cursor
            .read_u8()
            .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
        if terminator != b';' {
            return Err(I2cpError::malformed(
                "i2cp mapping",
                CodecError::InvalidFieldValue {
                    offset: cursor.offset().saturating_sub(1),
                    context: "mapping terminator",
                },
            ));
        }
        entries.push((key.to_owned(), value.to_owned()));
    }
    let mapping = Mapping::from_entries(entries)
        .map_err(|source| I2cpError::malformed("i2cp mapping", source))?;
    Ok((mapping, total))
}

/// Encodes a mapping in its canonical signing representation.
pub fn encode_mapping(mapping: &Mapping) -> Result<Vec<u8>, I2cpError> {
    mapping
        .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
        .map_err(|source| I2cpError::malformed("i2cp mapping", source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping_of(pairs: &[(&str, &str)]) -> Mapping {
        Mapping::from_entries(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        )
        .expect("mapping")
    }

    #[test]
    fn strict_round_trip_reports_consumed_prefix() {
        let mapping = mapping_of(&[("a", "1"), ("b", "2")]);
        let encoded = encode_mapping(&mapping).expect("encode");
        let mut input = encoded.clone();
        input.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let (decoded, consumed) = split_mapping_strict(&input).expect("decode");
        assert_eq!(decoded, mapping);
        assert_eq!(consumed, encoded.len());
        assert_eq!(&input[consumed..], &[0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn strict_rejects_unsorted_wire_order() {
        // Entries are length-prefixed (`len key = len value ;`); `b`
        // before `a` violates canonical order.
        let unsorted: Vec<u8> = vec![
            0, 12, 1, b'b', b'=', 1, b'2', b';', 1, b'a', b'=', 1, b'1', b';',
        ];
        assert!(matches!(
            split_mapping_strict(&unsorted),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn lenient_normalizes_unsorted_wire_order() {
        let unsorted: Vec<u8> = vec![
            0, 12, 1, b'b', b'=', 1, b'2', b';', 1, b'a', b'=', 1, b'1', b';',
        ];
        let (mapping, consumed) = split_mapping_lenient(&unsorted).expect("decode");
        assert_eq!(consumed, unsorted.len());
        assert_eq!(mapping, mapping_of(&[("a", "1"), ("b", "2")]));
        // Canonical re-encoding is sorted regardless of wire order.
        let sorted: Vec<u8> = vec![
            0, 12, 1, b'a', b'=', 1, b'1', b';', 1, b'b', b'=', 1, b'2', b';',
        ];
        assert_eq!(encode_mapping(&mapping).expect("encode"), sorted);
    }

    #[test]
    fn duplicate_key_rejected_in_both_postures() {
        let duplicate: Vec<u8> = vec![
            0, 12, 1, b'a', b'=', 1, b'1', b';', 1, b'a', b'=', 1, b'2', b';',
        ];
        assert!(matches!(
            split_mapping_strict(&duplicate),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            split_mapping_lenient(&duplicate),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn declared_body_without_bytes_is_malformed() {
        // Declares a 65535-byte body but supplies none: truncation,
        // reported without allocating the declared body.
        let raw = vec![0xffu8, 0xff];
        assert!(matches!(
            split_mapping_lenient(&raw),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn truncated_mapping_is_malformed() {
        assert!(matches!(
            split_mapping_lenient(&[0x00]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            split_mapping_strict(&[]),
            Err(I2cpError::Malformed { .. })
        ));
    }
}
