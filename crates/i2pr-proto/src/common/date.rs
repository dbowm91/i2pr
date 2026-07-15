//! Date primitives; freshness interpretation remains outside this crate.

use super::*;

/// An eight-byte millisecond timestamp. Zero is the protocol's undefined date.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Date(u64);

impl Date {
    /// Creates a date from milliseconds since the Unix epoch.
    pub const fn from_millis(value: u64) -> Self {
        Self(value)
    }

    /// Returns the encoded millisecond value.
    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Returns whether this date is the protocol's undefined/null value.
    pub const fn is_undefined(self) -> bool {
        self.0 == 0
    }

    /// Decodes one complete Date value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one Date value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self(cursor.read_u64()?))
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u64(self.0)
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Date").field(&self.0).finish()
    }
}

/// A four-byte seconds-since-epoch date used by Lease2-family structures.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Date32(u32);

impl Date32 {
    /// Creates a protocol seconds date.
    pub const fn from_seconds(value: u32) -> Self {
        Self(value)
    }

    /// Returns the encoded seconds value.
    pub const fn as_seconds(self) -> u32 {
        self.0
    }

    /// Decodes one complete Date32 value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one Date32 value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self(cursor.read_u32()?))
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u32(self.0)
    }
}
