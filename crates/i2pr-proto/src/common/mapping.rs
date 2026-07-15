//! Immutable canonical mappings and their bounded builder.

use super::*;

/// One UTF-8 mapping entry.
#[derive(Clone, Eq, PartialEq)]
pub struct MappingEntry {
    key: String,
    value: String,
}

impl MappingEntry {
    /// Creates an entry after checking protocol string bounds.
    pub fn new(key: String, value: String) -> Result<Self, CodecError> {
        validate_text(&key, true, "mapping key")?;
        validate_text(&value, true, "mapping value")?;
        Ok(Self { key, value })
    }

    /// Returns the entry key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the entry value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for MappingEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MappingEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .finish()
    }
}

/// A sorted, duplicate-free I2P Mapping.
#[derive(Clone, Eq, PartialEq)]
pub struct Mapping {
    entries: Vec<MappingEntry>,
}

impl Mapping {
    /// Returns an empty mapping.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Builds a canonical mapping from owned key/value pairs.
    pub fn from_entries(entries: Vec<(String, String)>) -> Result<Self, CodecError> {
        let mut entries = entries
            .into_iter()
            .map(|(key, value)| MappingEntry::new(key, value))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| java_string_cmp(&left.key, &right.key));
        for pair in entries.windows(2) {
            if pair[0].key == pair[1].key {
                return Err(CodecError::DuplicateField {
                    offset: 0,
                    context: "mapping key",
                });
            }
        }
        let mapping = Self { entries };
        mapping.encoded_body_len()?;
        Ok(mapping)
    }

    /// Starts a canonical mapping builder.
    pub fn builder() -> MappingBuilder {
        MappingBuilder {
            entries: Vec::new(),
        }
    }

    /// Returns entries in canonical wire order.
    pub fn entries(&self) -> &[MappingEntry] {
        &self.entries
    }

    /// Looks up a key without exposing mutable map state.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .binary_search_by(|entry| java_string_cmp(&entry.key, key))
            .ok()
            .map(|index| self.entries[index].value.as_str())
    }

    /// Decodes and validates a complete Mapping.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, |cursor| Self::decode_from(cursor, maximum))
    }

    pub(super) fn decode_from(
        cursor: &mut DecodeCursor<'_>,
        maximum: usize,
    ) -> Result<Self, CodecError> {
        let body_len = usize::from(cursor.read_u16()?);
        let body_limit = maximum.min(MAX_MAPPING_BODY_SIZE);
        if body_len > body_limit {
            return Err(CodecError::LengthExceeded {
                offset: cursor.offset().saturating_sub(2),
                declared: body_len,
                maximum: body_limit,
                context: "mapping body",
            });
        }
        let body = cursor.take(body_len)?;
        let mut inner = DecodeCursor::new(body, body_len)?;
        let mut entries: Vec<MappingEntry> = Vec::new();
        while !inner.is_empty() {
            let key_offset = inner.offset();
            let key = inner.read_utf8_u8(u8::MAX as usize)?.to_owned();
            if inner.read_u8()? != b'=' {
                return Err(invalid(
                    inner.offset().saturating_sub(1),
                    "mapping separator",
                ));
            }
            let value = inner.read_utf8_u8(u8::MAX as usize)?.to_owned();
            if inner.read_u8()? != b';' {
                return Err(invalid(
                    inner.offset().saturating_sub(1),
                    "mapping terminator",
                ));
            }
            let entry = MappingEntry::new(key, value)?;
            if let Some(previous) = entries.last() {
                match java_string_cmp(previous.key(), entry.key()) {
                    Ordering::Equal => {
                        return Err(CodecError::DuplicateField {
                            offset: key_offset,
                            context: "mapping key",
                        });
                    }
                    Ordering::Greater => {
                        return Err(CodecError::NonCanonical {
                            offset: key_offset,
                            context: "mapping key order",
                        });
                    }
                    Ordering::Less => {}
                }
            }
            entries.push(entry);
        }
        inner.finish()?;
        Ok(Self { entries })
    }

    /// Encodes a complete canonical Mapping.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn encoded_body_len(&self) -> Result<usize, CodecError> {
        let mut total = 0usize;
        for entry in &self.entries {
            total = total
                .checked_add(1 + entry.key.len() + 1 + 1 + entry.value.len() + 1)
                .ok_or(CodecError::ArithmeticOverflow {
                    offset: 0,
                    context: "mapping body length",
                })?;
        }
        if total > MAX_MAPPING_BODY_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: total,
                maximum: MAX_MAPPING_BODY_SIZE,
                context: "mapping body",
            });
        }
        Ok(total)
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        let body_len = self.encoded_body_len()?;
        let body_len = u16::try_from(body_len).map_err(|_| invalid(0, "mapping body length"))?;
        encoder.write_u16(body_len)?;
        for entry in &self.entries {
            encoder.write_utf8_u8(entry.key(), u8::MAX as usize)?;
            encoder.write_raw(b"=")?;
            encoder.write_utf8_u8(entry.value(), u8::MAX as usize)?;
            encoder.write_raw(b";")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Mapping {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Mapping")
            .field(&self.entries)
            .finish()
    }
}

/// Builder for an immutable canonical Mapping.
pub struct MappingBuilder {
    entries: Vec<(String, String)>,
}

impl MappingBuilder {
    /// Adds an entry. Duplicate detection is finalized by [`Self::build`].
    pub fn insert(&mut self, key: String, value: String) -> Result<(), CodecError> {
        MappingEntry::new(key.clone(), value.clone())?;
        self.entries.push((key, value));
        Ok(())
    }

    /// Validates and returns the immutable canonical mapping.
    pub fn build(self) -> Result<Mapping, CodecError> {
        Mapping::from_entries(self.entries)
    }
}
