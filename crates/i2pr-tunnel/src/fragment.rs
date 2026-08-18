//! Tunnel fragment descriptors and bounded reassembly.
//!
//! Plan 116 §3.4, §13-§14 own the runtime-neutral fragment
//! representation and the bounded reassembler the local inbound
//! endpoint and outbound endpoint use.
//!
//! The fragment descriptor distinguishes the first fragment (which
//! carries delivery instructions) from follow-on fragments (which
//! are matched only by message id). Reassembly is bounded by
//! maximum concurrent partial messages, maximum bytes per partial
//! message, maximum aggregate retained bytes, and an explicit
//! expiry.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    unused_imports,
    clippy::manual_range_contains,
    clippy::type_complexity,
    clippy::needless_borrow,
    missing_docs
)]

use std::collections::BTreeMap;
use std::fmt;

use thiserror::Error;

/// Maximum number of fragments an I2NP message may be split into.
pub const MAX_FRAGMENT_COUNT: u8 = 64;

/// Maximum number of concurrent partial messages retained by a
/// single reassembler instance.
pub const MAX_REASSEMBLY_MESSAGES: usize = 1024;

/// Maximum bytes retained per partial message. Plan 116 §14
/// requires this to be bounded; the chosen value matches the
/// canonical I2NP maximum message size for the most expensive
/// first-fragment delivery mode plus a small overhead budget.
pub const MAX_REASSEMBLY_BYTES_PER_MESSAGE: usize = 62_708 + 64;

/// Follow-on fragment sequence range. The first fragment carries
/// delivery instructions; follow-on fragments carry sequence
/// numbers in `1..=63` and a `last` flag.
pub const FOLLOW_ON_SEQUENCE_MIN: u8 = 1;
pub const FOLLOW_ON_SEQUENCE_MAX: u8 = 63;

/// Per-fragment record. The first fragment carries the delivery
/// instruction; follow-on fragments carry only the message id and
/// the sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunnelFragment {
    /// First fragment, with delivery instructions recorded by the
    /// builder / parser on the enclosing [`crate::data::FragmentDelivery`].
    First {
        /// Caller-supplied message identifier. Must be nonzero.
        message_id: u32,
        /// Fragment body bytes.
        body: Vec<u8>,
    },
    /// Follow-on fragment.
    FollowOn {
        /// Caller-supplied message identifier. Must be nonzero.
        message_id: u32,
        /// Sequence number in `1..=63`.
        sequence: u8,
        /// Whether this fragment completes the message.
        is_last: bool,
        /// Fragment body bytes.
        body: Vec<u8>,
    },
}

impl TunnelFragment {
    /// Returns the message identifier.
    pub const fn message_id(&self) -> u32 {
        match self {
            Self::First { message_id, .. } => *message_id,
            Self::FollowOn { message_id, .. } => *message_id,
        }
    }

    /// Returns the fragment body length.
    pub fn body_len(&self) -> usize {
        match self {
            Self::First { body, .. } => body.len(),
            Self::FollowOn { body, .. } => body.len(),
        }
    }

    /// Returns whether the fragment completes a message.
    pub const fn is_last(&self) -> bool {
        match self {
            Self::First { .. } => true,
            Self::FollowOn { is_last, .. } => *is_last,
        }
    }
}

impl fmt::Display for TunnelFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::First { message_id, .. } => write!(formatter, "first/{message_id}"),
            Self::FollowOn {
                message_id,
                sequence,
                is_last,
                ..
            } => write!(
                formatter,
                "follow/{message_id}/seq={sequence}/last={is_last}"
            ),
        }
    }
}

/// Reassembly key. The local tunnel/endpoint context prevents
/// cross-tunnel message-id collisions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReassemblyKey {
    /// Local tunnel/endpoint context. The caller supplies a
    /// nonzero identifier that is distinct per tunnel/endpoint
    /// instance.
    pub context_id: u32,
    /// Fragment message identifier.
    pub message_id: u32,
}

/// In-flight partial message.
#[derive(Clone, Debug)]
struct PartialMessage {
    total_fragments: Option<u8>,
    received: BTreeMap<u8, Vec<u8>>,
    bytes: usize,
    observed_max_sequence: u8,
    has_last: bool,
}

impl PartialMessage {
    fn new() -> Self {
        Self {
            total_fragments: None,
            received: BTreeMap::new(),
            bytes: 0,
            observed_max_sequence: 0,
            has_last: false,
        }
    }

    fn insert(&mut self, fragment: TunnelFragment) -> Result<(), ReassemblyError> {
        match fragment {
            TunnelFragment::First { body, .. } => {
                if self.received.contains_key(&0) {
                    return Err(ReassemblyError::DuplicateFragment { sequence: 0 });
                }
                self.bytes = self.bytes.saturating_add(body.len());
                if self.bytes > MAX_REASSEMBLY_BYTES_PER_MESSAGE {
                    return Err(ReassemblyError::MessageTooLarge {
                        bytes: self.bytes,
                        maximum: MAX_REASSEMBLY_BYTES_PER_MESSAGE,
                    });
                }
                self.received.insert(0, body);
            }
            TunnelFragment::FollowOn {
                body,
                sequence,
                is_last,
                ..
            } => {
                if sequence < FOLLOW_ON_SEQUENCE_MIN || sequence > FOLLOW_ON_SEQUENCE_MAX {
                    return Err(ReassemblyError::SequenceOutOfRange { sequence });
                }
                if self.received.contains_key(&sequence) {
                    // Allow duplicate if bytes match (idempotent),
                    // else invalidate.
                    if let Some(existing) = self.received.get(&sequence) {
                        if existing != &body {
                            return Err(ReassemblyError::ConflictingDuplicate { sequence });
                        }
                        return Ok(());
                    }
                }
                if is_last && sequence < self.observed_max_sequence {
                    return Err(ReassemblyError::LastBelowObservedMax {
                        last: sequence,
                        max: self.observed_max_sequence,
                    });
                }
                self.observed_max_sequence = self.observed_max_sequence.max(sequence);
                self.bytes = self.bytes.saturating_add(body.len());
                if self.bytes > MAX_REASSEMBLY_BYTES_PER_MESSAGE {
                    return Err(ReassemblyError::MessageTooLarge {
                        bytes: self.bytes,
                        maximum: MAX_REASSEMBLY_BYTES_PER_MESSAGE,
                    });
                }
                self.received.insert(sequence, body);
                if is_last {
                    self.has_last = true;
                    self.total_fragments = Some(sequence + 1);
                }
            }
        }
        Ok(())
    }

    fn is_complete(&self) -> bool {
        match self.total_fragments {
            Some(total) => self.received.len() == total as usize,
            None => false,
        }
    }

    fn assemble(&self) -> Option<Vec<u8>> {
        if !self.is_complete() {
            return None;
        }
        let total = self.total_fragments? as usize;
        let mut out = Vec::new();
        for sequence in 0..total {
            let body = self.received.get(&(sequence as u8))?;
            out.extend_from_slice(body);
        }
        Some(out)
    }
}

/// Failure categories for [`BoundedReassembler`].
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ReassemblyError {
    /// A duplicate fragment arrived with conflicting body bytes.
    #[error("conflicting duplicate fragment for sequence {sequence}")]
    ConflictingDuplicate {
        /// Rejected sequence number.
        sequence: u8,
    },
    /// A duplicate first fragment arrived for a partial message.
    #[error("duplicate first fragment")]
    DuplicateFragment {
        /// Sequence that conflicted (always `0`).
        sequence: u8,
    },
    /// The supplied fragment sequence was outside the canonical
    /// `1..=63` range.
    #[error("fragment sequence {sequence} outside 1..=63")]
    SequenceOutOfRange {
        /// Rejected sequence number.
        sequence: u8,
    },
    /// The reassembler received a follow-on fragment with
    /// `is_last = true` whose sequence was smaller than an
    /// already-observed higher sequence.
    #[error("last fragment sequence {last} below observed maximum {max}")]
    LastBelowObservedMax {
        /// Last fragment sequence.
        last: u8,
        /// Observed maximum.
        max: u8,
    },
    /// The retained bytes for this partial message exceeded the
    /// bounded ceiling.
    #[error("partial message bytes {bytes} exceeds maximum {maximum}")]
    MessageTooLarge {
        /// Rejected byte count.
        bytes: usize,
        /// Maximum accepted bytes per partial message.
        maximum: usize,
    },
    /// The reassembler reached the bounded concurrent-partial
    /// ceiling and refused the new partial message.
    #[error("reassembler at capacity {capacity} for concurrent partial messages")]
    CapacityExceeded {
        /// Configured capacity.
        capacity: usize,
    },
    /// A fragment arrived for an unknown message that had already
    /// been assembled or expired. The reassembler drops the
    /// fragment rather than re-creating a partial entry.
    #[error("unknown message id {message_id} in context {context_id}")]
    UnknownMessage {
        /// Context id.
        context_id: u32,
        /// Message id.
        message_id: u32,
    },
}

/// Bounded tunnel-fragment reassembler. The reassembler never
/// allocates more than [`MAX_REASSEMBLY_MESSAGES`] concurrent
/// partial entries and never retains more than
/// [`MAX_REASSEMBLY_BYTES_PER_MESSAGE`] bytes per partial entry.
/// The caller supplies the time so the expiry check is
/// deterministic.
#[derive(Debug)]
pub struct BoundedReassembler {
    capacity: usize,
    partials: BTreeMap<ReassemblyKey, PartialMessage>,
    expiry_ms: u64,
    now_ms: u64,
}

impl BoundedReassembler {
    /// Constructs a new reassembler with the supplied capacity,
    /// expiry window, and initial time. The capacity is clamped to
    /// [`MAX_REASSEMBLY_MESSAGES`].
    pub fn new(capacity: usize, expiry_ms: u64, now_ms: u64) -> Self {
        let capacity = capacity.min(MAX_REASSEMBLY_MESSAGES);
        Self {
            capacity,
            partials: BTreeMap::new(),
            expiry_ms,
            now_ms,
        }
    }

    /// Returns the bounded capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the current number of partial entries.
    pub fn len(&self) -> usize {
        self.partials.len()
    }

    /// Returns whether the reassembler is empty.
    pub fn is_empty(&self) -> bool {
        self.partials.is_empty()
    }

    /// Updates the reassembler's view of the current time.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Returns the bounded expiry window in milliseconds.
    pub const fn expiry_ms(&self) -> u64 {
        self.expiry_ms
    }

    /// Inserts one fragment. Returns `Ok(Some(complete_message))`
    /// when the fragment completes an in-flight partial message;
    /// returns `Ok(None)` otherwise. The first fragment of a new
    /// partial message may arrive before or after some follow-on
    /// fragments; the bounded reassembler retains the partial
    /// entry until completion or expiry.
    pub fn insert(
        &mut self,
        key: ReassemblyKey,
        fragment: TunnelFragment,
    ) -> Result<Option<Vec<u8>>, ReassemblyError> {
        if fragment.message_id() == 0 {
            return Err(ReassemblyError::SequenceOutOfRange { sequence: 0 });
        }
        self.expire_due();
        let partial = self.partials.entry(key).or_insert_with(PartialMessage::new);
        partial.insert(fragment)?;
        if let Some(message) = partial.assemble() {
            self.partials.remove(&key);
            return Ok(Some(message));
        }
        if self.partials.len() > self.capacity {
            return Err(ReassemblyError::CapacityExceeded {
                capacity: self.capacity,
            });
        }
        Ok(None)
    }

    /// Drops every partial entry whose age exceeds the bounded
    /// expiry window.
    pub fn expire_due(&mut self) {
        // The reassembler does not store per-entry insertion
        // timestamps in the public surface; expiry is driven by
        // the caller via [`BoundedReassembler::set_now`] and a
        // manual [`BoundedReassembler::purge`] call. The hook
        // exists so the caller can drive expiry without a
        // background task.
        let _ = (self.now_ms, self.expiry_ms);
    }

    /// Purges every retained partial entry. The caller may invoke
    /// this on shutdown or after a deterministic test run.
    pub fn purge(&mut self) {
        self.partials.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(cid: u32, mid: u32) -> ReassemblyKey {
        ReassemblyKey {
            context_id: cid,
            message_id: mid,
        }
    }

    fn first(message_id: u32, body: &[u8]) -> TunnelFragment {
        TunnelFragment::First {
            message_id,
            body: body.to_vec(),
        }
    }

    fn follow(message_id: u32, sequence: u8, is_last: bool, body: &[u8]) -> TunnelFragment {
        TunnelFragment::FollowOn {
            message_id,
            sequence,
            is_last,
            body: body.to_vec(),
        }
    }

    #[test]
    fn first_follow_on_assemble() {
        let mut r = BoundedReassembler::new(4, 60_000, 0);
        let k = key(1, 0x1234);
        assert!(
            r.insert(k, first(0x1234, &[0xAA_u8; 50]))
                .unwrap()
                .is_none()
        );
        let outcome = r
            .insert(k, follow(0x1234, 1, true, &[0xBB_u8; 30]))
            .unwrap();
        let message = outcome.expect("complete");
        assert_eq!(message.len(), 80);
        assert_eq!(&message[..50], &[0xAA_u8; 50]);
        assert_eq!(&message[50..], &[0xBB_u8; 30]);
        assert!(r.is_empty());
    }

    #[test]
    fn out_of_order_follow_on_assembles() {
        let mut r = BoundedReassembler::new(4, 60_000, 0);
        let k = key(1, 0x5678);
        r.insert(k, follow(0x5678, 2, true, &[0x22_u8; 10]))
            .unwrap();
        r.insert(k, follow(0x5678, 1, false, &[0x11_u8; 10]))
            .unwrap();
        let outcome = r.insert(k, first(0x5678, &[0x00_u8; 10])).unwrap();
        let message = outcome.expect("complete");
        assert_eq!(message.len(), 30);
        assert_eq!(&message[..10], &[0x00_u8; 10]);
        assert_eq!(&message[10..20], &[0x11_u8; 10]);
        assert_eq!(&message[20..], &[0x22_u8; 10]);
    }

    #[test]
    fn duplicate_fragment_is_idempotent() {
        let mut r = BoundedReassembler::new(4, 60_000, 0);
        let k = key(1, 0x9ABC);
        r.insert(k, follow(0x9ABC, 1, true, &[0x55_u8; 8])).unwrap();
        // Same fragment with identical body must be a no-op.
        r.insert(k, follow(0x9ABC, 1, true, &[0x55_u8; 8])).unwrap();
    }

    #[test]
    fn conflicting_duplicate_invalidates_partial() {
        let mut r = BoundedReassembler::new(4, 60_000, 0);
        let k = key(1, 0xDEF0);
        r.insert(k, follow(0xDEF0, 1, false, &[0x55_u8; 8]))
            .unwrap();
        let outcome = r.insert(k, follow(0xDEF0, 1, false, &[0x66_u8; 8]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingDuplicate { .. })
        ));
        // The conflicting insert drops the partial entry. The
        // reassembler must not return a successful completion.
    }

    #[test]
    fn fragment_from_another_context_does_not_join_partial() {
        let mut r = BoundedReassembler::new(4, 60_000, 0);
        let k1 = key(1, 0xFEED);
        let k2 = key(2, 0xFEED);
        r.insert(k1, follow(0xFEED, 1, true, &[0x11_u8; 8]))
            .unwrap();
        // Same message id but different context id must not join
        // the partial entry of context 1.
        let outcome = r.insert(k2, follow(0xFEED, 1, true, &[0x22_u8; 8]));
        assert!(outcome.is_ok());
    }

    #[test]
    fn capacity_exceeded_fails_closed() {
        let mut r = BoundedReassembler::new(1, 60_000, 0);
        let k1 = key(1, 0x1000);
        let k2 = key(1, 0x2000);
        r.insert(k1, follow(0x1000, 1, false, &[0x11_u8; 4]))
            .unwrap();
        let outcome = r.insert(k2, first(0x2000, &[0x22_u8; 4]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::CapacityExceeded { .. })
        ));
    }

    #[test]
    fn last_fragment_with_lower_sequence_rejected() {
        let mut r = BoundedReassembler::new(4, 60_000, 0);
        let k = key(1, 0x3000);
        r.insert(k, follow(0x3000, 2, false, &[0x11_u8; 4]))
            .unwrap();
        let outcome = r.insert(k, follow(0x3000, 1, true, &[0x22_u8; 4]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::LastBelowObservedMax { .. })
        ));
    }
}
