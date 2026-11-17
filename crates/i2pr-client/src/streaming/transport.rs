#![allow(dead_code)]

//! Abstract transport interface for streaming packets.
//!
//! The streaming layer never opens sockets or touches the network
//! directly. It dispatches outbound packets through the
//! [`StreamingTransport`] trait, which the runtime-level integration
//! implements. This keeps the streaming state machine synchronous and
//! deterministic.

use core::fmt;

/// Outcome of a transport send attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportOutcome {
    /// The packet was accepted for delivery.
    Accepted,
    /// The transport rejected the packet.
    Rejected,
}

/// Typed transport failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    /// The transport layer is shutting down.
    #[error("streaming transport shutting down")]
    ShuttingDown,
    /// The destination is unreachable.
    #[error("streaming transport destination unreachable")]
    DestinationUnreachable,
    /// The transport encountered a transient failure.
    #[error("streaming transport transient: {0}")]
    Transient(String),
}

impl fmt::Display for TransportOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accepted => formatter.write_str("accepted"),
            Self::Rejected => formatter.write_str("rejected"),
        }
    }
}

/// The kind of streaming packet a transport request carries.
///
/// Plan 487 WP2: the kind is **producer-owned**. The runtime attributes
/// an acknowledgement to the egress stage from this field instead of
/// re-decoding the wire bytes, so no stage count is ever inferred
/// from another stage's count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundPacketKind {
    /// The initial SYN.
    Syn,
    /// A SYN response (SYN-ACK).
    SynAck,
    /// A data packet carrying application payload.
    Data,
    /// A standalone acknowledgement.
    Ack,
    /// A graceful close.
    Close,
    /// A reset.
    Reset,
    /// A connectionless datagram (protocol 18), which is not a
    /// streaming packet at all.
    Datagram,
}

/// A request to send one streaming packet through the transport layer.
///
/// The transport implementation consumes this request and routes the
/// packet through the Plan 122 destination routing pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportSendRequest {
    /// SHA-256 hash of the remote destination.
    pub destination_hash: [u8; 32],
    /// Source port on the local destination.
    pub source_port: u16,
    /// Destination port on the remote destination.
    pub destination_port: u16,
    /// The serialized streaming packet payload (already wrapped in
    /// protocol-6 client payload framing).
    pub application_payload: Vec<u8>,
    /// Packet sequence number.
    pub sequence: u32,
    /// Sender's stream ID.
    pub send_stream_id: u32,
    /// Receiver's stream ID.
    pub receive_stream_id: u32,
    /// The kind of packet this request carries (Plan 487 WP2).
    pub kind: OutboundPacketKind,
}

/// Abstract interface for sending streaming packets through the
/// destination routing pipeline.
///
/// The streaming state machine is synchronous; the transport adapter
/// queues outbound packets and the runtime flushes them.
pub trait StreamingTransport {
    /// Attempts to send one streaming packet. The implementation may
    /// queue the packet internally and return immediately; the
    /// streaming state machine does not block on network I/O.
    fn send(&self, request: TransportSendRequest) -> Result<TransportOutcome, TransportError>;
}
