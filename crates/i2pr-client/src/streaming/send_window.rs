#![allow(dead_code)]

//! Outbound send window management.
//!
//! The send window tracks unacked packets, enforces the configured
//! maximum in-flight count, and provides backpressure to the
//! application writer.

use std::collections::{BTreeMap, BTreeSet};

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

/// Outcome of a NACK-aware cumulative acknowledgement (Plan 130 §7
/// D4). Mirrors the reference contract: packets at or below
/// `ack_through` are cleared unless they were explicitly NACKed;
/// duplicate acknowledgements are idempotent and never regress state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AckOutcome {
    /// Entries removed because this acknowledgement covered them.
    pub newly_acked: Vec<UnackedEntry>,
    /// Sequences still tracked that the receiver explicitly NACKed
    /// (they remain eligible for retransmission).
    pub retained_nacks: Vec<u32>,
    /// The acknowledgement carried no new information (`ackThrough`
    /// below the already-highest acknowledged sequence).
    pub duplicate_ack: bool,
}

/// The first application sequence number on an established stream
/// (Plan 130 §6 C1). I2P Streaming reserves sequence 0 for the SYN,
/// the SYN response, and the plain-ACK control form; ordinary post-
/// SYN data begins at sequence 1 and increments by one per message
/// except plain ACKs and retransmissions.
pub const FIRST_APPLICATION_SEQUENCE: u32 = 1;

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
    /// Creates a new send window with the given configuration. The
    /// first application packet receives sequence **1**; sequence 0
    /// is owned by the SYN / SYN-response / plain-ACK forms.
    pub fn new(config: SendWindowConfig) -> Self {
        Self {
            config,
            unacked: BTreeMap::new(),
            next_sequence: FIRST_APPLICATION_SEQUENCE,
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

    /// Applies one inbound cumulative acknowledgement with its NACK
    /// list (Plan 130 §7 D4, reference contract from the Java I2P
    /// `Connection.ackPackets` behavior and the Streaming
    /// specification):
    ///
    /// - every tracked packet at or below `ack_through` is cleared,
    ///   **except** sequences listed in `nacks`, which stay tracked;
    /// - `ack_through == 0` is a valid acknowledgement of sequence 0
    ///   (the SYN / SYN-response slot) — validity is decided by the
    ///   caller from packet flags, never by this numeric value;
    /// - NACKed entries below the lowest NACK bound the cumulative
    ///   floor (`last_ack` never advances past `lowest_nack - 1`);
    /// - duplicate acknowledgements (`ackThrough` below the recorded
    ///   floor) are idempotent: nothing is removed and no state
    ///   regresses;
    /// - NACK values at or above `ack_through` violate the wire
    ///   contract ("sequence numbers less than ackThrough") and are
    ///   ignored fail-closed.
    pub fn acknowledge(&mut self, ack_through: u32, nacks: &[u32]) -> AckOutcome {
        let nack_set: BTreeSet<u32> = nacks
            .iter()
            .copied()
            .filter(|&sequence| sequence < ack_through)
            .collect();
        let duplicate_ack = ack_through < self.last_ack;
        let mut outcome = AckOutcome {
            newly_acked: Vec::new(),
            retained_nacks: Vec::new(),
            duplicate_ack,
        };
        if duplicate_ack {
            return outcome;
        }
        let keys: Vec<u32> = self
            .unacked
            .range(..=ack_through)
            .map(|(&key, _)| key)
            .collect();
        for key in keys {
            if nack_set.contains(&key) {
                outcome.retained_nacks.push(key);
                continue;
            }
            if let Some(entry) = self.unacked.remove(&key) {
                self.unacked_bytes = self.unacked_bytes.saturating_sub(entry.payload_len);
                outcome.newly_acked.push(entry);
            }
        }
        match nack_set.iter().next() {
            Some(&lowest) => {
                self.last_ack = self.last_ack.max(lowest.saturating_sub(1));
            }
            None => {
                self.last_ack = self.last_ack.max(ack_through);
            }
        }
        outcome
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
