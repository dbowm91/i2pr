//! Fixed hash representation and bounded digest mechanics.

use super::*;

/// A fixed 32-byte SHA-256 value.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Constructs a hash from exactly 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Computes SHA-256 using the reviewed `sha2` crate.
    pub fn digest(input: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(input);
        Self(hasher.finalize().into())
    }

    /// Returns the raw protocol bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Decodes one complete Hash value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one Hash value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self(take_array(cursor)?))
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_raw(&self.0)
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Hash(..)")
    }
}
