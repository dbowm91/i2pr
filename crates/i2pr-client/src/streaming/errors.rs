#![allow(dead_code)]

//! Streaming typed error surface.
//!
//! Every error variant is explicit and typed so callers never have to
//! inspect opaque strings.

use crate::streaming::config::StreamingConfigError;
use i2pr_proto::streaming::ClientPayloadEncodeError;

/// Typed failure surface for the streaming layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StreamingError {
    /// The streaming configuration was invalid.
    #[error("streaming config: {0}")]
    Config(#[from] StreamingConfigError),
    /// The connection has been closed and cannot accept new operations.
    #[error("streaming connection closed")]
    ConnectionClosed,
    /// The connection was reset by the remote peer.
    #[error("streaming connection reset")]
    ConnectionReset,
    /// The connection setup timed out.
    #[error("streaming connection setup timed out")]
    SetupTimeout,
    /// The connection idle timed out.
    #[error("streaming connection idle timed out")]
    IdleTimeout,
    /// The close handshake timed out.
    #[error("streaming close timed out")]
    CloseTimeout,
    /// The wire packet codec rejected the bytes.
    #[error("streaming wire codec: {0}")]
    WireCodec(String),
    /// An invalid state transition was attempted.
    #[error("streaming invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        /// Current state label.
        from: &'static str,
        /// Attempted target state label.
        to: &'static str,
    },
    /// The send window is full and cannot accept more unacked packets.
    #[error("streaming send window full")]
    SendWindowFull,
    /// The receive window is full and cannot accept more reordered packets.
    #[error("streaming receive window full")]
    RecvWindowFull,
    /// The connection has no available stream ID.
    #[error("streaming no available stream ID")]
    NoStreamIdAvailable,
    /// The connection table is full for this destination.
    #[error("streaming connection table full")]
    ConnectionTableFull,
    /// The listener accept backlog is full.
    #[error("streaming listener accept backlog full")]
    ListenerBacklogFull,
    /// The packet signature verification failed.
    #[error("streaming packet signature invalid")]
    SignatureInvalid,
    /// The SYN replay binding did not match the local destination hash.
    #[error("streaming SYN replay binding mismatch")]
    SynReplayBindingMismatch,
    /// The payload exceeds the negotiated maximum packet size.
    #[error("streaming payload exceeds negotiated max packet size")]
    PayloadExceedsMaxPacketSize,
    /// The transport layer rejected the outbound packet.
    #[error("streaming transport error: {0}")]
    Transport(String),
    /// The retransmission budget was exhausted for a packet.
    #[error("streaming retransmit budget exhausted for seq {sequence}")]
    RetransmitBudgetExhausted {
        /// Sequence number of the exhausted packet.
        sequence: u32,
    },
    /// A required field was missing from a decoded packet.
    #[error("streaming missing required field: {0}")]
    MissingField(&'static str),
    /// The connection received a packet for an unknown stream.
    #[error("streaming unknown stream ID")]
    UnknownStream,
    /// Generic congestion control rejection.
    #[error("streaming congestion control rejection")]
    CongestionRejected,
    /// The inbound client payload envelope could not be decoded.
    #[error("streaming inbound envelope: {0}")]
    InboundEnvelope(i2pr_proto::streaming::ClientPayloadDecodeError),
    /// The supplied payload exceeds the negotiated max packet size.
    #[error("streaming payload {actual} exceeds {maximum}-byte max packet size")]
    PayloadTooLarge {
        /// Declared payload length.
        actual: usize,
        /// Allowed maximum.
        maximum: usize,
    },
    /// The outbound client payload envelope could not be encoded.
    #[error("streaming outbound envelope: {0}")]
    OutboundEnvelope(ClientPayloadEncodeError),
}
