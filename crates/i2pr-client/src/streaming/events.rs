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

/// Redacted ingress accounting for inbound SYN-response (SYN-ACK)
/// packets observed by one [`crate::streaming::StreamingManager`].
///
/// Counts only: no stream ids, ports, hashes, or payload material.
/// `observed` advances once per packet the header peek classifies as
/// a SYN response; exactly one of `accepted` / `rejected` advances
/// per observed packet, so `accepted + rejected == observed` always
/// holds. A SYN response that arrives but never establishes (unknown
/// connection, port-tuple mismatch, bad signature) is visible as
/// `observed > accepted`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SynResponseIngressAccounting {
    /// SYN-response packets classified by the header peek.
    pub observed: u64,
    /// Observed packets that transitioned an outbound connection.
    pub accepted: u64,
    /// Observed packets discarded with a typed error.
    pub rejected: u64,
}

impl SynResponseIngressAccounting {
    /// Returns saturating fieldwise deltas from an earlier snapshot.
    pub fn delta_since(self, earlier: Self) -> Self {
        Self {
            observed: self.observed.saturating_sub(earlier.observed),
            accepted: self.accepted.saturating_sub(earlier.accepted),
            rejected: self.rejected.saturating_sub(earlier.rejected),
        }
    }
}

/// Redacted ingress accounting for inbound SYN (originator)
/// packets observed by one [`crate::streaming::StreamingManager`]
/// acting as acceptor.
///
/// Counts only: no stream ids, ports, hashes, or payload material.
/// `observed` advances once per packet the header peek classifies
/// as an inbound SYN; exactly one of `accepted` / `rejected`
/// advances per observed packet, so `accepted + rejected ==
/// observed` always holds. `accepted` means the SYN reached a
/// listener backlog (the application may still need to accept it).
/// `rejected` is additionally split by cause so a refused SYN names
/// its owner (undecodable wire vs policy validation vs signature
/// authentication vs listener selection vs backlog capacity); the
/// five split fields always sum to `rejected`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SynIngressAccounting {
    /// Inbound SYN packets classified by the header peek.
    pub observed: u64,
    /// Observed packets that reached a listener backlog.
    pub accepted: u64,
    /// Observed packets discarded with a typed error.
    pub rejected: u64,
    /// Split of `rejected`: the anonymous decode failed.
    pub rejected_decode: u64,
    /// Split of `rejected`: initial-SYN policy validation failed.
    pub rejected_validation: u64,
    /// Split of `rejected`: FROM/signature authentication failed.
    pub rejected_signature: u64,
    /// Split of `rejected`: no listener claimed the port.
    pub rejected_no_listener: u64,
    /// Split of `rejected`: the listener backlog was full.
    pub rejected_backlog_full: u64,
}

/// Typed inbound-SYN rejection cause for the
/// [`SynIngressAccounting`] split. Counts only; no identities,
/// keys, or payload material.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SynIngressRejectCause {
    /// The anonymous decode failed.
    Decode,
    /// Initial-SYN policy validation failed.
    Validation,
    /// FROM/signature authentication failed.
    Signature,
    /// No listener claimed the destination port.
    NoListener,
    /// The listener backlog was full.
    BacklogFull,
}

impl SynIngressAccounting {
    /// Records one typed inbound-SYN rejection: increments the
    /// aggregate and exactly the matching split field, with
    /// saturating arithmetic. Every `rejected` site must go through
    /// here so the split sum invariant holds structurally.
    pub fn note_rejected(&mut self, cause: SynIngressRejectCause) {
        self.rejected = self.rejected.saturating_add(1);
        match cause {
            SynIngressRejectCause::Decode => {
                self.rejected_decode = self.rejected_decode.saturating_add(1);
            }
            SynIngressRejectCause::Validation => {
                self.rejected_validation = self.rejected_validation.saturating_add(1);
            }
            SynIngressRejectCause::Signature => {
                self.rejected_signature = self.rejected_signature.saturating_add(1);
            }
            SynIngressRejectCause::NoListener => {
                self.rejected_no_listener = self.rejected_no_listener.saturating_add(1);
            }
            SynIngressRejectCause::BacklogFull => {
                self.rejected_backlog_full = self.rejected_backlog_full.saturating_add(1);
            }
        }
    }

    /// Sum of the five split fields. Equals `rejected` when every
    /// rejection site used [`SynIngressAccounting::note_rejected`].
    pub fn split_rejected_sum(&self) -> u64 {
        self.rejected_decode
            .saturating_add(self.rejected_validation)
            .saturating_add(self.rejected_signature)
            .saturating_add(self.rejected_no_listener)
            .saturating_add(self.rejected_backlog_full)
    }

    /// Returns saturating fieldwise deltas from an earlier snapshot.
    pub fn delta_since(self, earlier: Self) -> Self {
        Self {
            observed: self.observed.saturating_sub(earlier.observed),
            accepted: self.accepted.saturating_sub(earlier.accepted),
            rejected: self.rejected.saturating_sub(earlier.rejected),
            rejected_decode: self.rejected_decode.saturating_sub(earlier.rejected_decode),
            rejected_validation: self
                .rejected_validation
                .saturating_sub(earlier.rejected_validation),
            rejected_signature: self
                .rejected_signature
                .saturating_sub(earlier.rejected_signature),
            rejected_no_listener: self
                .rejected_no_listener
                .saturating_sub(earlier.rejected_no_listener),
            rejected_backlog_full: self
                .rejected_backlog_full
                .saturating_sub(earlier.rejected_backlog_full),
        }
    }
}

/// Redacted ingress accounting for established-connection (data /
/// CLOSE / RESET / plain-ACK) packets observed by one
/// [`crate::streaming::StreamingManager`].
///
/// Counts only: no stream ids, ports, hashes, or payload material.
/// `observed` advances once per packet the header peek classifies as
/// established-connection traffic; exactly one of `accepted` /
/// `rejected` advances per observed packet, so `accepted + rejected
/// == observed` always holds. `payload_bytes_admitted` accumulates
/// only application payload bytes the receive window admitted
/// (in-order delivery plus reorder-buffer admission, including
/// backlog drains released by the arriving packet), so a data-phase
/// stall splits into "nothing arrived" (`observed == 0`),
/// "arrived but rejected" (`rejected > 0`), "only control arrived"
/// (`accepted > 0` with `payload_bytes_admitted == 0`), and
/// "payload admitted but never drained" (admitted bytes with no
/// application read).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DataPacketIngressAccounting {
    /// Established-traffic packets classified by the header peek.
    pub observed: u64,
    /// Observed packets the connection processed without error
    /// (includes payload-free control: plain ACKs, CLOSE/RESET).
    pub accepted: u64,
    /// Observed packets discarded with a typed error (unknown
    /// connection, port-tuple mismatch, decode failure, or a
    /// receive-window rejection).
    pub rejected: u64,
    /// Application payload bytes admitted by the receive window.
    pub payload_bytes_admitted: u64,
}

impl DataPacketIngressAccounting {
    /// Returns saturating fieldwise deltas from an earlier snapshot.
    pub fn delta_since(self, earlier: Self) -> Self {
        Self {
            observed: self.observed.saturating_sub(earlier.observed),
            accepted: self.accepted.saturating_sub(earlier.accepted),
            rejected: self.rejected.saturating_sub(earlier.rejected),
            payload_bytes_admitted: self
                .payload_bytes_admitted
                .saturating_sub(earlier.payload_bytes_admitted),
        }
    }
}
