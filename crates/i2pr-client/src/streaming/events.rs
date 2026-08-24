#![allow(dead_code)]

//! Streaming connection events.
//!
//! The streaming layer produces typed events that the manager
//! surfaces to the application layer. Events carry no private
//! material and are safe to log.

/// Observation of an acknowledgement being received or generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AckObservation {
    /// Cumulative acknowledgement sequence (`ackThrough`).
    pub ack_through: u32,
    /// Number of NACKs carried in this acknowledgement.
    pub nack_count: usize,
}

/// An inbound event observed by the receive side of a connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InboundStreamEvent {
    /// An in-order data packet was received and delivered to the
    /// application.
    DataDelivered {
        /// Sequence number delivered.
        sequence: u32,
        /// Number of payload bytes delivered.
        payload_len: usize,
    },
    /// A SYN packet was received for a new inbound connection.
    SynReceived {
        /// Remote stream ID.
        remote_stream_id: u32,
    },
    /// An ACK was received confirming delivery of our outbound data.
    AckReceived(AckObservation),
    /// A duplicate packet was detected and not re-delivered.
    DuplicateDetected {
        /// Duplicate sequence number.
        sequence: u32,
    },
    /// A packet arrived too far ahead and was dropped.
    TooFarAhead {
        /// Sequence number of the dropped packet.
        sequence: u32,
    },
    /// A RESET was received from the remote peer.
    ResetReceived,
    /// A CLOSE was received from the remote peer.
    CloseReceived,
}

/// An outbound event observed by the send side of a connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutboundStreamEvent {
    /// A data packet was queued for transmission.
    DataQueued {
        /// Sequence number assigned.
        sequence: u32,
        /// Number of payload bytes queued.
        payload_len: usize,
    },
    /// A packet was acknowledged by the remote peer.
    Acked(AckObservation),
    /// A packet was retransmitted due to timeout.
    Retransmitted {
        /// Sequence number retransmitted.
        sequence: u32,
    },
    /// A NACK was received indicating the remote peer is missing this
    /// sequence.
    NackReceived {
        /// Missing sequence number reported by the remote peer.
        missing_sequence: u32,
    },
    /// The send window was reduced due to congestion or loss.
    WindowReduced {
        /// New window size in packets.
        new_window: u32,
    },
    /// A RESET was sent.
    ResetSent,
    /// A CLOSE was sent.
    CloseSent,
}

/// Observation of a decoded wire packet for diagnostic/tracing
/// purposes. Does not carry raw payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WirePacketObservation {
    /// Connection identifier (when known).
    pub connection_id: Option<crate::streaming::connection::ConnectionId>,
    /// Sender's stream ID.
    pub send_stream_id: u32,
    /// Receiver's stream ID.
    pub receive_stream_id: u32,
    /// Packet sequence number.
    pub sequence: u32,
    /// Cumulative acknowledgement.
    pub ack_through: u32,
    /// Number of NACKs.
    pub nack_count: usize,
    /// Flag bits.
    pub flags: u16,
    /// Payload length in bytes.
    pub payload_len: usize,
}
