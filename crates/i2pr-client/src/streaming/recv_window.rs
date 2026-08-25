#![allow(dead_code)]

//! Inbound receive window management.
//!
//! The receive window tracks incoming packets, detects duplicates,
//! provides in-order delivery, generates NACKs for missing packets,
//! and enforces hard bounds on the reorder buffer.

use std::collections::BTreeMap;

use crate::streaming::config::StreamingConfig;

/// Hard ceiling on NACKs carried by one packet. The wire NACK-count
/// field is one byte, so at most 255 NACKs are representable; the
/// reorder window bound keeps practical counts far below this.
pub const MAX_NACK_COUNT: usize = 255;

/// The next application sequence a fresh post-handshake receive
/// window expects (Plan 130 §6 C2). Sequence 0 was consumed by the
/// SYN / SYN-response handshake; ordinary data begins at 1. A
/// non-SYN sequence-zero packet is the plain-ACK control form and
/// never enters this window.
pub const FIRST_EXPECTED_APPLICATION_SEQUENCE: u32 = 1;

/// Configuration for the receive window derived from the streaming
/// configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecvWindowConfig {
    /// Maximum number of packets in the reorder buffer.
    pub max_window_packets: u16,
}

impl RecvWindowConfig {
    /// Builds a recv window config from the streaming config.
    pub fn from_config(config: &StreamingConfig) -> Self {
        Self {
            max_window_packets: config.max_recv_window_packets,
        }
    }
}

/// A packet held in the reorder buffer waiting for in-order delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderEntry {
    /// Packet sequence number.
    pub sequence: u32,
    /// Payload bytes.
    pub payload: Vec<u8>,
}

/// Decision returned by the receive window policy after evaluating an
/// incoming packet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecvWindowDecision {
    /// The packet was accepted and delivered in order.
    Delivered {
        /// Sequence number of the delivered packet.
        sequence: u32,
        /// Ordered payloads delivered by this receive: the in-order
        /// packet first, then any contiguous reorder-buffer entries in
        /// ascending sequence order. Plan 129 requires the receiver to
        /// observe the original application byte order after a
        /// reorder, so the drained entries are surfaced instead of
        /// being discarded.
        delivered: Vec<ReorderEntry>,
    },
    /// The packet was accepted into the reorder buffer.
    Buffered {
        /// Sequence number buffered.
        sequence: u32,
    },
    /// The packet is a duplicate and was not re-delivered.
    Duplicate {
        /// Duplicate sequence number.
        sequence: u32,
    },
    /// The packet arrived too far ahead and was dropped.
    TooFarAhead {
        /// Sequence number of the dropped packet.
        sequence: u32,
    },
}

/// Manages the inbound receive window. Detects duplicates, buffers
/// out-of-order packets, and provides in-order delivery.
#[derive(Debug)]
pub struct RecvWindowPolicy {
    config: RecvWindowConfig,
    /// Reorder buffer keyed by sequence number.
    reorder: BTreeMap<u32, ReorderEntry>,
    /// Next expected sequence number for in-order delivery.
    next_expected: u32,
    /// Number of packets delivered so far.
    delivered_count: u64,
    /// Highest sequence number received so far, including out-of-order
    /// buffered packets (the reference `ackThrough` source).
    highest_received: Option<u32>,
}

impl RecvWindowPolicy {
    /// Creates a new receive window policy. The window expects the
    /// first application packet at sequence **1** (Plan 130 §6 C2);
    /// sequence 0 belonged to the handshake.
    pub fn new(config: RecvWindowConfig) -> Self {
        Self {
            config,
            reorder: BTreeMap::new(),
            next_expected: FIRST_EXPECTED_APPLICATION_SEQUENCE,
            delivered_count: 0,
            highest_received: None,
        }
    }

    /// Creates a new receive window from the streaming config.
    pub fn from_streaming_config(config: &StreamingConfig) -> Self {
        Self::new(RecvWindowConfig::from_config(config))
    }

    /// Processes an incoming packet and returns the delivery decision.
    pub fn receive(&mut self, sequence: u32, payload: Vec<u8>) -> RecvWindowDecision {
        // Duplicate or already delivered.
        if sequence < self.next_expected {
            return RecvWindowDecision::Duplicate { sequence };
        }

        self.highest_received = Some(match self.highest_received {
            Some(highest) => highest.max(sequence),
            None => sequence,
        });

        // Exactly the expected sequence: deliver and drain any
        // contiguous packets from the reorder buffer.
        if sequence == self.next_expected {
            let mut delivered = vec![ReorderEntry { sequence, payload }];
            self.next_expected = self.next_expected.wrapping_add(1);
            self.delivered_count = self.delivered_count.saturating_add(1);
            // Drain contiguous packets from the reorder buffer.
            while let Some(entry) = self.reorder.remove(&self.next_expected) {
                self.next_expected = self.next_expected.wrapping_add(1);
                self.delivered_count = self.delivered_count.saturating_add(1);
                delivered.push(entry);
            }
            return RecvWindowDecision::Delivered {
                sequence,
                delivered,
            };
        }

        // Too far ahead: drop.
        let window_size = self.config.max_window_packets as u32;
        let max_sequence = self.next_expected.saturating_add(window_size);
        if sequence >= max_sequence {
            return RecvWindowDecision::TooFarAhead { sequence };
        }

        // Within the reorder window: buffer.
        self.reorder
            .insert(sequence, ReorderEntry { sequence, payload });
        RecvWindowDecision::Buffered { sequence }
    }

    /// Returns the next expected sequence number.
    pub fn next_expected(&self) -> u32 {
        self.next_expected
    }

    /// Returns the highest sequence number received so far, including
    /// out-of-order buffered packets.
    pub fn highest_received(&self) -> Option<u32> {
        self.highest_received
    }

    /// Returns the acknowledgement state this side must attach to its
    /// next outbound packet (Plan 130 §7 D5, reference contract from
    /// the Java I2P `MessageInputStream.updateAcks` behavior):
    ///
    /// - `ack_through` is the **highest received** sequence number,
    ///   including out-of-order buffered packets — not the contiguous
    ///   delivery point. Sequence 0 is a valid value (it acknowledges
    ///   the handshake);
    /// - `nacks` lists the missing sequences strictly between the
    ///   contiguous delivery point and that highest received packet.
    ///   The list is bounded by the reorder window and by the wire
    ///   one-byte NACK-count ceiling; attacker-controlled gaps beyond
    ///   the window are dropped before they can allocate anything.
    ///
    /// The caller decides from packet flags (`NO_ACK`, plain-ACK form)
    /// whether the returned state is attached or ignored.
    pub fn ack_view(&self) -> (u32, Vec<u32>) {
        match self.highest_received {
            None => (0, Vec::new()),
            Some(highest) => {
                if highest < self.next_expected {
                    (self.next_expected.saturating_sub(1), Vec::new())
                } else {
                    (
                        highest,
                        self.missing_sequences_bounded(highest, MAX_NACK_COUNT),
                    )
                }
            }
        }
    }

    /// Bounded variant of [`Self::missing_sequences`] used for wire
    /// NACK generation.
    fn missing_sequences_bounded(&self, ack_through: u32, limit: usize) -> Vec<u32> {
        let mut missing = Vec::new();
        let mut seq = self.next_expected;
        while seq < ack_through {
            if missing.len() >= limit {
                break;
            }
            if !self.reorder.contains_key(&seq) {
                missing.push(seq);
            }
            seq = seq.wrapping_add(1);
        }
        missing
    }

    /// Returns the number of packets in the reorder buffer.
    pub fn reorder_count(&self) -> usize {
        self.reorder.len()
    }

    /// Returns the total number of in-order packets delivered.
    pub fn delivered_count(&self) -> u64 {
        self.delivered_count
    }

    /// Returns the set of missing sequence numbers between the
    /// last delivered and the given `ack_through` (exclusive),
    /// bounded by the wire NACK-count ceiling.
    pub fn missing_sequences(&self, ack_through: u32) -> Vec<u32> {
        self.missing_sequences_bounded(ack_through, MAX_NACK_COUNT)
    }
}
