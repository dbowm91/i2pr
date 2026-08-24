#![allow(dead_code)]

//! Inbound receive window management.
//!
//! The receive window tracks incoming packets, detects duplicates,
//! provides in-order delivery, generates NACKs for missing packets,
//! and enforces hard bounds on the reorder buffer.

use std::collections::BTreeMap;

use crate::streaming::config::StreamingConfig;

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
}

impl RecvWindowPolicy {
    /// Creates a new receive window policy.
    pub fn new(config: RecvWindowConfig) -> Self {
        Self {
            config,
            reorder: BTreeMap::new(),
            next_expected: 0,
            delivered_count: 0,
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

        // Exactly the expected sequence: deliver and drain any
        // contiguous packets from the reorder buffer.
        if sequence == self.next_expected {
            self.next_expected = self.next_expected.wrapping_add(1);
            self.delivered_count = self.delivered_count.saturating_add(1);
            // Drain contiguous packets from the reorder buffer.
            while let Some(entry) = self.reorder.remove(&self.next_expected) {
                self.next_expected = self.next_expected.wrapping_add(1);
                self.delivered_count = self.delivered_count.saturating_add(1);
                let _ = entry;
            }
            return RecvWindowDecision::Delivered { sequence };
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

    /// Returns the number of packets in the reorder buffer.
    pub fn reorder_count(&self) -> usize {
        self.reorder.len()
    }

    /// Returns the total number of in-order packets delivered.
    pub fn delivered_count(&self) -> u64 {
        self.delivered_count
    }

    /// Returns the set of missing sequence numbers between the
    /// last delivered and the given `ack_through` (exclusive).
    pub fn missing_sequences(&self, ack_through: u32) -> Vec<u32> {
        let mut missing = Vec::new();
        let mut seq = self.next_expected;
        while seq < ack_through {
            if !self.reorder.contains_key(&seq) {
                missing.push(seq);
            }
            seq = seq.wrapping_add(1);
        }
        missing
    }
}
