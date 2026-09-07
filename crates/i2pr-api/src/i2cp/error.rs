//! Typed I2CP wire errors.
//!
//! Every variant is a closed classification: truncation, ceiling
//! violations, unknown/deprecated/unsupported message types, and
//! malformed known bodies (with the originating [`CodecError`]
//! retained for offset-level diagnosis). No variant panics, logs
//! secrets, or retains attacker-controlled bytes.

use i2pr_proto::CodecError;
use thiserror::Error;

/// Errors produced by the runtime-neutral I2CP wire layer.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum I2cpError {
    /// The opening protocol byte was not `0x2a`.
    #[error("invalid I2CP protocol byte: expected 0x2a, observed {actual:#04x}")]
    InvalidProtocolByte {
        /// The observed first byte.
        actual: u8,
    },
    /// A declared body length exceeds the I2CP ceiling.
    #[error("I2CP body length {declared} exceeds {maximum}-byte ceiling")]
    BodyTooLarge {
        /// The declared body length.
        declared: usize,
        /// The enforced ceiling.
        maximum: usize,
    },
    /// The peer sent a message type with no assigned I2CP meaning.
    #[error("unknown I2CP message type {raw}")]
    UnknownMessageType {
        /// The raw type byte.
        raw: u8,
    },
    /// The peer sent a deprecated message type (legacy CreateLeaseSet,
    /// ReceiveMessageBegin/End, ReportAbuse, RequestLeaseSet).
    #[error("deprecated I2CP message type {raw}")]
    DeprecatedMessageType {
        /// The raw type byte.
        raw: u8,
    },
    /// The peer sent a message type outside the M9 profile
    /// (currently BlindingInfo for blinded destinations).
    #[error("unsupported I2CP message type {raw}")]
    UnsupportedMessageType {
        /// The raw type byte.
        raw: u8,
    },
    /// Fewer bytes are available than a complete header or body needs.
    #[error("incomplete I2CP input: need {needed} more bytes")]
    Incomplete {
        /// Number of additional bytes required to make progress.
        needed: usize,
    },
    /// A known message body failed structural decoding.
    #[error("malformed {message}: {source}")]
    Malformed {
        /// Static message name for the failing codec.
        message: &'static str,
        /// The underlying codec failure.
        source: CodecError,
    },
}

impl I2cpError {
    /// Wraps a [`CodecError`] with the failing message name.
    pub const fn malformed(message: &'static str, source: CodecError) -> Self {
        Self::Malformed { message, source }
    }
}
