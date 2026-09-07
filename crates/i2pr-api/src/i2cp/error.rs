//! Typed I2CP wire and protocol errors.
//!
//! Every variant is a closed classification: framing, malformed known
//! bodies, message-family classification, and Plan 165
//! connection/session/option failures. No variant panics, logs
//! secrets, or retains attacker-controlled bytes.

use i2pr_proto::CodecError;
use thiserror::Error;

/// Errors produced by the runtime-neutral I2CP wire layer.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
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
    /// The connection observed a message family that its current state
    /// does not permit (Plan 165 connection state machine).
    #[error("I2CP message type {message_type} not allowed in state {state}")]
    IllegalInState {
        /// The state that rejected the message.
        state: &'static str,
        /// The numeric message type byte that arrived.
        message_type: u8,
    },
    /// The GetDate version string was unparsable or unsupported by the
    /// M9 profile.
    #[error("I2CP version negotiation failed: {reason}")]
    VersionNegotiation {
        /// Static reason for the rejection.
        reason: &'static str,
    },
    /// GetDate supplied a username/password or option map that the M9
    /// profile rejects.
    #[error("I2CP authentication/options not supported: {context}")]
    AuthNotSupported {
        /// Static context describing which input was rejected.
        context: &'static str,
    },
    /// The connection observed a duplicate or out-of-order handshake
    /// message (GetDate, SetDate).
    #[error("I2CP handshake ordering rejected: {context}")]
    HandshakeOrdering {
        /// Static description of the violation.
        context: &'static str,
    },
    /// SessionConfig structural limits were exceeded (size, options
    /// count, or per-key/per-value ceiling).
    #[error("SessionConfig limit exceeded: {context} {actual} > {maximum}")]
    SessionConfigLimit {
        /// Static field/category name.
        context: &'static str,
        /// Supplied size.
        actual: usize,
        /// Documented ceiling.
        maximum: usize,
    },
    /// SessionConfig signature verification failed.
    #[error("SessionConfig signature verification failed")]
    SignatureRejected,
    /// SessionConfig creation timestamp falls outside the clock window.
    #[error("SessionConfig creation timestamp {creation_ms} outside [{earliest_ms}, {latest_ms}]")]
    CreationTimestampOutOfRange {
        /// Supplied creation timestamp.
        creation_ms: u64,
        /// Earliest accepted timestamp.
        earliest_ms: u64,
        /// Latest accepted timestamp.
        latest_ms: u64,
    },
    /// SessionConfig destination signing type is not supported by the M9
    /// profile.
    #[error("SessionConfig destination signing type {signing_type} unsupported")]
    UnsupportedSigningType {
        /// Numeric signing-type code.
        signing_type: u16,
    },
    /// SessionConfig destination encryption type is not supported by the
    /// M9 profile.
    #[error("SessionConfig destination encryption type {crypto_type} unsupported")]
    UnsupportedCryptoType {
        /// Numeric encryption-type code.
        crypto_type: u16,
    },
    /// A session option was rejected by the M9 disposition table.
    #[error("SessionConfig option {key} rejected: {reason}")]
    OptionRejected {
        /// Supplied option key.
        key: String,
        /// Static rejection reason.
        reason: &'static str,
    },
    /// A requested option value could not be parsed under the M9
    /// syntax rules (signed, whitespace, overflow, non-digit).
    #[error("SessionConfig option {key} could not be parsed: {reason}")]
    OptionParseFailed {
        /// Supplied option key.
        key: String,
        /// Static reason.
        reason: &'static str,
    },
    /// Session registry rejected an operation (capacity, duplicate, reuse).
    #[error("I2CP session registry rejected: {context}")]
    SessionRegistry {
        /// Static context describing the failure.
        context: &'static str,
    },
    /// Session ID space exhausted or invalid.
    #[error("I2CP session id space exhausted")]
    SessionIdExhausted,
    /// Reconfigure request failed the all-or-nothing validation pass.
    #[error("I2CP reconfigure rejected: {context}")]
    ReconfigureRejected {
        /// Static description.
        context: &'static str,
    },
}

impl I2cpError {
    /// Wraps a [`CodecError`] with the failing message name.
    pub const fn malformed(message: &'static str, source: CodecError) -> Self {
        Self::Malformed { message, source }
    }
}
