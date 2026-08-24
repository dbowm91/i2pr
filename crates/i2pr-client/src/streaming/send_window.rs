#![allow(dead_code)]

//! Outbound send window management.
//!
//! The send window tracks unacked packets, enforces the configured
//! maximum in-flight count, and provides backpressure to the
//! application writer.

use std::collections::BTreeMap;

use crate::streaming::config::StreamingConfig;

/// Configuration for the send window derived from the streaming
/// configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SendWindowConfig {
    /// Maximum number of unacked packets in flight.
    pub max_window_packets: u16,
    /// Maximum total unacked bytes.
    pub max_window_bytes: usize,
}

impl SendWindowConfig {
    /// Builds a send window config from the streaming config.
    pub fn from_config(config: &StreamingConfig) -> Self {
        Self {
            max_window_packets: config.max_send_window_packets,
            max_window_bytes: (config.max_send_window_packets as usize)
                * crate::streaming::config::MAX_PACKET_PAYLOAD_BYTES,
        }
    }
}

/// An entry in the unacked packet map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnackedEntry {
    /// Packet sequence number.
    pub sequence: u32,
    /// Payload length in bytes.
    pub payload_len: usize,
    /// Timestamp when the packet was first sent (ms).
    pub sent_at_ms: u64,
    /// Number of times this packet has been retransmitted.
    pub retransmit_count: u32,
}

/// Decision returned by the send window policy after evaluating a
/// candidate packet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SendWindowDecision {
    /// The packet may be enqueued.
    Accept,
    /// The window is full; the application must wait.
    Backpressure,
}

/// Manages the outbound send window. Tracks unacked packets,
/// provides backpressure, and enforces hard bounds.
#[derive(Debug)]
pub struct SendWindowPolicy {
    config: SendWindowConfig,
    /// Map of sequence -> unacked entry.
    unacked: BTreeMap<u32, UnackedEntry>,
    /// Next sequence number to assign.
    next_sequence: u32,
    /// Total unacked bytes.
    unacked_bytes: usize,
    /// Last acknowledged sequence (cumulative).
    last_ack: u32,
}

impl SendWindowPolicy {
    /// Creates a new send window with the given configuration.
    pub fn new(config: SendWindowConfig) -> Self {
        Self {
            config,
            unacked: BTreeMap::new(),
            next_sequence: 0,
            unacked_bytes: 0,
            last_ack: 0,
        }
    }

    /// Creates a new send window from the streaming config.
    pub fn from_streaming_config(config: &StreamingConfig) -> Self {
        Self::new(SendWindowConfig::from_config(config))
    }

    /// Evaluates whether a new packet of the given size can be
    /// enqueued.
    pub fn evaluate(&self, payload_len: usize) -> SendWindowDecision {
        let would_be_count = self.unacked.len().saturating_add(1);
        let would_be_bytes = self.unacked_bytes.saturating_add(payload_len);
        if would_be_count > self.config.max_window_packets as usize
            || would_be_bytes > self.config.max_window_bytes
        {
            SendWindowDecision::Backpressure
        } else {
            SendWindowDecision::Accept
        }
    }

    /// Assigns the next sequence number and records the packet in the
    /// unacked map.
    pub fn enqueue(&mut self, payload_len: usize, now_ms: u64) -> Result<u32, SendWindowDecision> {
        if self.evaluate(payload_len) == SendWindowDecision::Backpressure {
            return Err(SendWindowDecision::Backpressure);
        }
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.unacked_bytes = self.unacked_bytes.saturating_add(payload_len);
        self.unacked.insert(
            seq,
            UnackedEntry {
                sequence: seq,
                payload_len,
                sent_at_ms: now_ms,
                retransmit_count: 0,
            },
        );
        Ok(seq)
    }

    /// Marks a packet as acknowledged and removes it from the unacked
    /// map. Returns all entries that were removed (may be multiple if
    /// `ack_through` covers a range).
    pub fn ack_through(&mut self, ack_through: u32) -> Vec<UnackedEntry> {
        let mut removed = Vec::new();
        let keys: Vec<u32> = self
            .unacked
            .range(..=ack_through)
            .map(|(&k, _)| k)
            .collect();
        for key in keys {
            if let Some(entry) = self.unacked.remove(&key) {
                self.unacked_bytes = self.unacked_bytes.saturating_sub(entry.payload_len);
                removed.push(entry);
            }
        }
        if ack_through > self.last_ack {
            self.last_ack = ack_through;
        }
        removed
    }

    /// Returns the entries that have exceeded the given RTO threshold.
    pub fn expired_entries(&self, now_ms: u64, rto_ms: u64) -> Vec<u32> {
        self.unacked
            .iter()
            .filter(|(_, entry)| now_ms.saturating_sub(entry.sent_at_ms) >= rto_ms)
            .map(|(&seq, _)| seq)
            .collect()
    }

    /// Bumps the retransmit count for a given sequence.
    pub fn mark_retransmitted(&mut self, sequence: u32, now_ms: u64) {
        if let Some(entry) = self.unacked.get_mut(&sequence) {
            entry.retransmit_count = entry.retransmit_count.saturating_add(1);
            entry.sent_at_ms = now_ms;
        }
    }

    /// Removes a packet from the unacked map (e.g. after exhausting
    /// retransmissions). Returns the removed entry.
    pub fn remove(&mut self, sequence: u32) -> Option<UnackedEntry> {
        let entry = self.unacked.remove(&sequence)?;
        self.unacked_bytes = self.unacked_bytes.saturating_sub(entry.payload_len);
        Some(entry)
    }

    /// Returns the number of unacked packets.
    pub fn unacked_count(&self) -> usize {
        self.unacked.len()
    }

    /// Returns the total unacked bytes.
    pub fn unacked_bytes(&self) -> usize {
        self.unacked_bytes
    }

    /// Returns the last cumulative ack.
    pub fn last_ack(&self) -> u32 {
        self.last_ack
    }

    /// Returns the next sequence number that will be assigned.
    pub fn next_sequence(&self) -> u32 {
        self.next_sequence
    }

    /// Returns a reference to an unacked entry by sequence.
    pub fn get_unacked(&self, sequence: u32) -> Option<&UnackedEntry> {
        self.unacked.get(&sequence)
    }
}
