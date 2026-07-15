//! Bounded owned encoded I2NP messages.

use std::fmt;

use crate::{MAX_I2NP_MESSAGE_BYTES, PeerId};

/// An owned, non-cloneable encoded I2NP message at the authenticated-link
/// boundary.
///
/// The bytes are intentionally not decoded or re-encoded here. This preserves
/// the caller's canonical/authenticated representation while keeping protocol
/// interpretation in `i2pr-proto` and later transport plans. The owner is
/// handed off explicitly with [`EncodedI2npMessage::into_bytes`].
pub struct EncodedI2npMessage {
    bytes: Vec<u8>,
}

impl EncodedI2npMessage {
    /// Takes ownership after enforcing nonzero and maximum wire-size bounds.
    pub fn new(bytes: Vec<u8>) -> Result<Self, PayloadError> {
        if bytes.is_empty() {
            return Err(PayloadError::Empty);
        }
        if bytes.len() > MAX_I2NP_MESSAGE_BYTES {
            return Err(PayloadError::TooLarge {
                actual: bytes.len(),
                maximum: MAX_I2NP_MESSAGE_BYTES,
            });
        }
        Ok(Self { bytes })
    }

    /// Returns the number of encoded bytes without exposing their contents.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the owned message contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns a borrowed view for a runtime-owned write operation.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Hands the encoded owner to the next layer without cloning.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl fmt::Debug for EncodedI2npMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedI2npMessage")
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// Validation failures for an encoded I2NP message owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadError {
    /// An empty message cannot be delivered.
    Empty,
    /// The encoded message exceeds the transport boundary.
    TooLarge {
        /// Supplied encoded length.
        actual: usize,
        /// Transport boundary maximum.
        maximum: usize,
    },
}

impl fmt::Display for PayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("encoded I2NP message must not be empty"),
            Self::TooLarge { maximum, .. } => {
                write!(
                    formatter,
                    "encoded I2NP message exceeds the {maximum}-byte limit"
                )
            }
        }
    }
}

impl std::error::Error for PayloadError {}

// Keep this import visible to rustdoc users searching the boundary for peer
// ownership; no payload type stores a peer itself.
const _: Option<PeerId> = None;
