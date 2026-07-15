//! Bounded deferred and opaque payload values.

use super::*;

/// A bounded byte sequence whose semantic interpretation belongs to a later
/// milestone. Its debug representation intentionally omits the bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct DeferredPayload {
    bytes: Vec<u8>,
}

impl DeferredPayload {
    /// Creates a bounded deferred value.
    pub fn new(bytes: Vec<u8>, maximum: usize) -> Result<Self, CodecError> {
        if bytes.len() > maximum {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: bytes.len(),
                maximum,
                context: "deferred I2NP payload",
            });
        }
        Ok(Self { bytes })
    }

    /// Returns the retained bytes to a caller that explicitly owns deferred
    /// interpretation.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for DeferredPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeferredPayload")
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// A bounded opaque body used for Data and Garlic messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueMessageBody {
    /// Deferred payload bytes.
    pub payload: DeferredPayload,
}
