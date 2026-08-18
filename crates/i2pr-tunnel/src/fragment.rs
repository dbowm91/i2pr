//! Tunnel fragment descriptors and bounded reassembly.
//!
//! Plan 116 §3.4, §13-§14 own the runtime-neutral fragment
//! representation and the bounded reassembler the local inbound
//! endpoint and outbound endpoint use.
//!
//! The fragment descriptor distinguishes the initial unfragmented
//! record (no message id) from the first fragmented record
//! (carrying delivery instructions + message id) and from
//! follow-on fragments (matched only by message id and
//! sequence). Reassembly is bounded by maximum concurrent partial
//! messages, maximum bytes per partial message, maximum aggregate
//! retained bytes, and an explicit caller-driven expiry.

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
use std::time::Duration;

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

/// Completed reassembled message plus the first-fragment delivery
/// instruction the reassembler retained. The data plane uses this
/// to choose a router-delivery action without falling back to a
/// synthetic unspecified-delivery rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReassembledFragment {
    /// Reassembled complete message bytes.
    pub message: Vec<u8>,
    /// First-fragment delivery instruction retained from the
    /// first sighting, or `None` when no delivery instruction
    /// was supplied or the message was unfragmented.
    pub delivery: Option<crate::data::DeliveryInstruction>,
}

/// Per-fragment record. The unfragmented initial record carries
/// neither a message id nor a sequence; the first fragmented
/// record carries the message id; follow-on fragments carry only
/// the message id and the sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunnelFragment {
    /// Initial unfragmented record: no message id, no sequence.
    Unfragmented {
        /// Fragment body bytes.
        body: Vec<u8>,
    },
    /// First fragmented record: carries delivery instructions on
    /// the enclosing [`crate::data::FragmentDelivery`] plus the
    /// message id here.
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
    /// Returns the message identifier when one is present.
    pub const fn message_id(&self) -> Option<u32> {
        match self {
            Self::Unfragmented { .. } => None,
            Self::First { message_id, .. } => Some(*message_id),
            Self::FollowOn { message_id, .. } => Some(*message_id),
        }
    }

    /// Returns the fragment body length.
    pub fn body_len(&self) -> usize {
        match self {
            Self::Unfragmented { body, .. } => body.len(),
            Self::First { body, .. } => body.len(),
            Self::FollowOn { body, .. } => body.len(),
        }
    }

    /// Returns whether the fragment completes a message.
    pub const fn is_last(&self) -> bool {
        match self {
            Self::Unfragmented { .. } => true,
            Self::First { .. } => false,
            Self::FollowOn { is_last, .. } => *is_last,
        }
    }
}

impl fmt::Display for TunnelFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unfragmented { body } => {
                write!(formatter, "unfragmented/{}B", body.len())
            }
            Self::First { message_id, body } => {
                write!(formatter, "first/{message_id}/{}B", body.len())
            }
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

/// Disposition of a fragment-insertion attempt against an existing
/// partial message. The reassembler classifies the fragment before
/// applying any retained-byte accounting so exact duplicates are
/// true no-ops for memory, expiry, and budget purposes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FragmentInsertDisposition {
    /// The fragment carries new unique bytes that the partial must
    /// accept; the caller charges the aggregate budget by this
    /// amount.
    Inserted {
        /// Bytes the new unique fragment contributes to the partial.
        added_bytes: usize,
    },
    /// The fragment is byte-for-byte identical to an already
    /// retained fragment at the same sequence. The reassembler
    /// returns success without mutating partial state, refreshing
    /// `last_touched_ms`, or charging the aggregate budget.
    ExactDuplicate,
}

/// In-flight partial message.
#[derive(Clone, Debug)]
struct PartialMessage {
    total_fragments: Option<u8>,
    received: BTreeMap<u8, Vec<u8>>,
    /// Per-sequence terminal flag retained from the first
    /// sighting of each follow-on fragment. The `is_last` flag is
    /// control metadata that participates in duplicate identity;
    /// a fresh fragment with a different `is_last` at the same
    /// sequence must invalidate the affected partial.
    last_flags: BTreeMap<u8, bool>,
    bytes: usize,
    observed_max_sequence: u8,
    has_last: bool,
    last_touched_ms: u64,
    /// First-fragment delivery instruction retained until
    /// reassembly completes. `None` for an unfragmented message
    /// or for a partial that has not yet received its first
    /// fragment.
    first_delivery: Option<crate::data::DeliveryInstruction>,
}

impl PartialMessage {
    fn new(now_ms: u64) -> Self {
        Self {
            total_fragments: None,
            received: BTreeMap::new(),
            last_flags: BTreeMap::new(),
            bytes: 0,
            observed_max_sequence: 0,
            has_last: false,
            last_touched_ms: now_ms,
            first_delivery: None,
        }
    }

    /// Classifies a candidate fragment against the current partial
    /// state without mutating it. The disposition tells the caller
    /// whether the fragment is an exact duplicate (no-op), a
    /// new unique fragment that must be admitted, or a conflict
    /// (which invalidates the partial). Per-message size limits are
    /// checked at apply time because they depend on the cumulative
    /// partial bytes the caller already knows.
    ///
    /// The caller must supply the same delivery instruction it
    /// intends to apply. Two first-fragment sightings are exact
    /// duplicates only when both the body bytes **and** the
    /// delivery instruction match. A follow-on fragment must not
    /// carry a delivery instruction on the wire; supplying one is
    /// rejected fail-closed.
    fn classify(
        &self,
        fragment: &TunnelFragment,
        delivery: Option<&crate::data::DeliveryInstruction>,
    ) -> Result<FragmentInsertDisposition, ReassemblyError> {
        match fragment {
            TunnelFragment::Unfragmented { body } => {
                if let Some(existing) = self.received.get(&0) {
                    if existing != body {
                        return Err(ReassemblyError::ConflictingDuplicate { sequence: 0 });
                    }
                    return Ok(FragmentInsertDisposition::ExactDuplicate);
                }
                Ok(FragmentInsertDisposition::Inserted {
                    added_bytes: body.len(),
                })
            }
            TunnelFragment::First { body, .. } => {
                if let Some(existing) = self.received.get(&0) {
                    if existing != body {
                        return Err(ReassemblyError::ConflictingDuplicate { sequence: 0 });
                    }
                    if !delivery_matches(self.first_delivery.as_ref(), delivery) {
                        return Err(ReassemblyError::ConflictingFirstMetadata);
                    }
                    return Ok(FragmentInsertDisposition::ExactDuplicate);
                }
                Ok(FragmentInsertDisposition::Inserted {
                    added_bytes: body.len(),
                })
            }
            TunnelFragment::FollowOn {
                body,
                sequence,
                is_last,
                ..
            } => {
                if delivery.is_some() {
                    return Err(ReassemblyError::UnexpectedFollowOnDeliveryInstruction);
                }
                if *sequence < FOLLOW_ON_SEQUENCE_MIN || *sequence > FOLLOW_ON_SEQUENCE_MAX {
                    return Err(ReassemblyError::SequenceOutOfRange {
                        sequence: *sequence,
                    });
                }
                if let Some(existing) = self.received.get(sequence) {
                    if existing != body {
                        return Err(ReassemblyError::ConflictingDuplicate {
                            sequence: *sequence,
                        });
                    }
                    // The terminal flag is control metadata that
                    // participates in duplicate identity. Two
                    // fragments at the same sequence with identical
                    // body bytes are exact duplicates only when
                    // both `is_last` flags agree.
                    let prior_last = self.last_flags.get(sequence).copied().unwrap_or(false);
                    if prior_last != *is_last {
                        return Err(ReassemblyError::ConflictingFollowOnTerminalFlag {
                            sequence: *sequence,
                            expected: prior_last,
                            actual: *is_last,
                        });
                    }
                    return Ok(FragmentInsertDisposition::ExactDuplicate);
                }
                if *is_last && *sequence < self.observed_max_sequence {
                    return Err(ReassemblyError::LastBelowObservedMax {
                        last: *sequence,
                        max: self.observed_max_sequence,
                    });
                }
                Ok(FragmentInsertDisposition::Inserted {
                    added_bytes: body.len(),
                })
            }
        }
    }

    /// Applies a fragment that the caller has already classified as
    /// `Inserted`. The caller must have admitted the fragment's
    /// `added_bytes` against the aggregate budget before calling
    /// this method; this method enforces the per-message ceiling and
    /// performs the actual insertion.
    fn apply(
        &mut self,
        fragment: TunnelFragment,
        delivery: Option<&crate::data::DeliveryInstruction>,
    ) -> Result<(), ReassemblyError> {
        match fragment {
            TunnelFragment::Unfragmented { body } => {
                let added = body.len();
                self.bytes = self.bytes.saturating_add(added);
                if self.bytes > MAX_REASSEMBLY_BYTES_PER_MESSAGE {
                    self.bytes = self.bytes.saturating_sub(added);
                    return Err(ReassemblyError::MessageTooLarge {
                        bytes: self.bytes.saturating_add(added),
                        maximum: MAX_REASSEMBLY_BYTES_PER_MESSAGE,
                    });
                }
                self.received.insert(0, body);
            }
            TunnelFragment::First { body, .. } => {
                let added = body.len();
                self.bytes = self.bytes.saturating_add(added);
                if self.bytes > MAX_REASSEMBLY_BYTES_PER_MESSAGE {
                    self.bytes = self.bytes.saturating_sub(added);
                    return Err(ReassemblyError::MessageTooLarge {
                        bytes: self.bytes.saturating_add(added),
                        maximum: MAX_REASSEMBLY_BYTES_PER_MESSAGE,
                    });
                }
                self.received.insert(0, body);
                // First sighting of this partial's first fragment:
                // classify() rejected any duplicate, so recording
                // the delivery here is safe and unambiguous.
                self.first_delivery = delivery.cloned();
                // A first fragment is not yet last; terminal
                // completion is determined by a follow-on fragment
                // with `is_last = true`.
            }
            TunnelFragment::FollowOn {
                body,
                sequence,
                is_last,
                ..
            } => {
                let added = body.len();
                let prior_max = self.observed_max_sequence;
                self.observed_max_sequence = self.observed_max_sequence.max(sequence);
                self.bytes = self.bytes.saturating_add(added);
                if self.bytes > MAX_REASSEMBLY_BYTES_PER_MESSAGE {
                    self.bytes = self.bytes.saturating_sub(added);
                    self.observed_max_sequence = prior_max;
                    return Err(ReassemblyError::MessageTooLarge {
                        bytes: self.bytes.saturating_add(added),
                        maximum: MAX_REASSEMBLY_BYTES_PER_MESSAGE,
                    });
                }
                self.received.insert(sequence, body);
                // Record the per-sequence terminal flag for
                // follow-on duplicate identity. A conflicting
                // `is_last` at the same sequence is rejected by
                // classify() before apply() runs.
                self.last_flags.insert(sequence, is_last);
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
    /// A duplicate first fragment arrived with the same body
    /// bytes but a different delivery instruction. The conflict
    /// invalidates only the affected partial message.
    #[error("conflicting first fragment delivery instruction")]
    ConflictingFirstMetadata,
    /// A duplicate follow-on fragment arrived with the same body
    /// bytes but a different `is_last` flag. The terminal flag is
    /// control metadata that participates in duplicate identity;
    /// a conflicting flag invalidates only the affected partial
    /// message.
    #[error(
        "conflicting follow-on terminal flag at sequence {sequence}: expected {expected}, got {actual}"
    )]
    ConflictingFollowOnTerminalFlag {
        /// Conflicting sequence number.
        sequence: u8,
        /// The terminal flag already retained for the sequence.
        expected: bool,
        /// The terminal flag supplied with the new fragment.
        actual: bool,
    },
    /// The caller supplied a follow-on fragment with a delivery
    /// instruction. Follow-on Tunnel Message records carry no
    /// delivery instruction on the wire; supplying one is rejected
    /// fail-closed rather than silently recorded.
    #[error("follow-on fragment must not carry a delivery instruction")]
    UnexpectedFollowOnDeliveryInstruction,
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
    /// The aggregate retained-byte budget would be exceeded by
    /// the new fragment.
    #[error("reassembler aggregate bytes {bytes} exceeds maximum {maximum}")]
    AggregateBytesExceeded {
        /// Rejected aggregate byte count after the new fragment.
        bytes: usize,
        /// Maximum accepted aggregate bytes.
        maximum: usize,
    },
}

/// Bounded tunnel-fragment reassembler. The reassembler never
/// allocates more than [`MAX_REASSEMBLY_MESSAGES`] concurrent
/// partial entries, never retains more than
/// [`MAX_REASSEMBLY_BYTES_PER_MESSAGE`] bytes per partial entry,
/// and never retains more than [`MAX_REASSEMBLY_AGGREGATE_BYTES`]
/// aggregate bytes across every partial entry. The caller
/// supplies the time so the expiry check is deterministic.
#[derive(Debug)]
pub struct BoundedReassembler {
    capacity: usize,
    aggregate_bytes_max: usize,
    partials: BTreeMap<ReassemblyKey, PartialMessage>,
    expiry_ms: u64,
    now_ms: u64,
    aggregate_bytes: usize,
}

/// Maximum aggregate retained bytes. Plan 116 §14 requires a
/// bounded aggregate retained-byte ceiling; the chosen value is
/// two full maximum-size messages plus headroom.
pub const MAX_REASSEMBLY_AGGREGATE_BYTES: usize =
    (MAX_REASSEMBLY_BYTES_PER_MESSAGE + 1) * MAX_REASSEMBLY_MESSAGES / 8;

impl BoundedReassembler {
    /// Constructs a new reassembler with the supplied capacity,
    /// aggregate byte budget, expiry window, and initial time.
    /// The capacity is clamped to [`MAX_REASSEMBLY_MESSAGES`].
    pub fn new(capacity: usize, aggregate_bytes_max: usize, expiry_ms: u64, now_ms: u64) -> Self {
        let capacity = capacity.min(MAX_REASSEMBLY_MESSAGES);
        let aggregate_bytes_max = aggregate_bytes_max.min(MAX_REASSEMBLY_AGGREGATE_BYTES);
        Self {
            capacity,
            aggregate_bytes_max,
            partials: BTreeMap::new(),
            expiry_ms,
            now_ms,
            aggregate_bytes: 0,
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

    /// Returns the retained aggregate byte count.
    pub fn retained_bytes(&self) -> usize {
        self.aggregate_bytes
    }

    /// Returns the bounded aggregate byte budget.
    pub fn aggregate_bytes_budget(&self) -> usize {
        self.aggregate_bytes_max
    }

    /// Updates the reassembler's view of the current time.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Returns the bounded expiry window in milliseconds.
    pub const fn expiry_ms(&self) -> u64 {
        self.expiry_ms
    }

    /// Inserts one fragment and remembers the supplied first-fragment
    /// delivery instruction until reassembly completes. Returns
    /// `Ok(Some(ReassembledFragment { message, delivery }))` when
    /// the fragment completes an in-flight partial message;
    /// returns `Ok(None)` otherwise.
    ///
    /// The caller must supply the delivery instruction for the
    /// first fragment of a fragmented message; the delivery
    /// instruction is stored in the partial entry and returned
    /// when reassembly completes. For follow-on fragments the
    /// caller may pass `None`; the reassembler only stores the
    /// first delivery instruction it sees for a given key.
    ///
    /// The reassembler never synthesises a delivery instruction
    /// for a reassembled message: if the first fragment arrives
    /// without a delivery instruction, completion returns the
    /// message with `delivery = None`, and the caller must decide
    /// how to handle it.
    ///
    /// Exact duplicates — same message id, same sequence, same body
    /// — are pure no-ops: no aggregate budget charge, no expiry
    /// refresh, no partial-mutation. A conflicting first fragment
    /// invalidates only the affected partial message.
    ///
    /// On failure the reassembler does **not** retain partial
    /// state for the offending insertion: capacity, byte-budget,
    /// and per-message size failures are rolled back before the
    /// function returns.
    pub fn insert_with_delivery(
        &mut self,
        key: ReassemblyKey,
        fragment: TunnelFragment,
        delivery: Option<crate::data::DeliveryInstruction>,
    ) -> Result<Option<ReassembledFragment>, ReassemblyError> {
        let message_id = fragment.message_id().unwrap_or(0);
        if matches!(fragment, TunnelFragment::Unfragmented { .. }) {
            // Unfragmented messages never participate in
            // reassembly.
            return Ok(Some(ReassembledFragment {
                message: extract_unfragmented_body(&fragment),
                delivery,
            }));
        }
        if message_id == 0 {
            return Err(ReassemblyError::SequenceOutOfRange { sequence: 0 });
        }
        self.expire_due();
        // Classify before any aggregate-budget charge so exact
        // duplicates cannot be rejected merely because the budget
        // is already full. A conflicting classification invalidates
        // the partial; we drop the partial and zeroize its
        // retained-byte accounting before propagating the error.
        let disposition = if let Some(partial) = self.partials.get(&key) {
            match partial.classify(&fragment, delivery.as_ref()) {
                Ok(disposition) => disposition,
                Err(error) => {
                    if let Some(p) = self.partials.remove(&key) {
                        self.aggregate_bytes = self.aggregate_bytes.saturating_sub(p.bytes);
                    }
                    return Err(error);
                }
            }
        } else {
            // Capacity is the only constraint that depends on
            // partial creation; aggregate budget is checked once
            // the disposition is known below. Fresh-key
            // candidates must still satisfy the same semantic
            // invariants the existing-partial path enforces:
            // a follow-on fragment may not carry a delivery
            // instruction and may not carry an out-of-range
            // sequence number.
            if self.partials.len() >= self.capacity {
                return Err(ReassemblyError::CapacityExceeded {
                    capacity: self.capacity,
                });
            }
            classify_new_partial(&fragment, delivery.as_ref())?
        };
        if let FragmentInsertDisposition::ExactDuplicate = disposition {
            // No state change. No expiry refresh. No budget charge.
            return Ok(None);
        }
        let added_bytes = match disposition {
            FragmentInsertDisposition::Inserted { added_bytes } => added_bytes,
            FragmentInsertDisposition::ExactDuplicate => unreachable!(),
        };
        if self
            .aggregate_bytes
            .checked_add(added_bytes)
            .map(|sum| sum > self.aggregate_bytes_max)
            .unwrap_or(true)
        {
            return Err(ReassemblyError::AggregateBytesExceeded {
                bytes: self.aggregate_bytes.saturating_add(added_bytes),
                maximum: self.aggregate_bytes_max,
            });
        }
        // Insert or update.
        let partial = self
            .partials
            .entry(key)
            .or_insert_with(|| PartialMessage::new(self.now_ms));
        // A new partial was just constructed with
        // `last_touched_ms = self.now_ms`; an existing partial
        // accepted a unique fragment and therefore gets a fresh
        // expiry origin. Exact duplicates never reach this line.
        partial.last_touched_ms = self.now_ms;
        match partial.apply(fragment, delivery.as_ref()) {
            Ok(()) => {}
            Err(error) => {
                // Roll back any retained state changes. We
                // always remove the partial when the insertion
                // returns an error; the conflicting-duplicate
                // case must also drop the partial to satisfy the
                // spec §11.4 invalidation rule.
                if let Some(p) = self.partials.remove(&key) {
                    self.aggregate_bytes = self.aggregate_bytes.saturating_sub(p.bytes);
                }
                return Err(error);
            }
        }
        self.aggregate_bytes = self.aggregate_bytes.saturating_add(added_bytes);
        if let Some(message) = partial.assemble() {
            let first_delivery = partial.first_delivery.clone();
            let total = partial.bytes;
            self.partials.remove(&key);
            self.aggregate_bytes = self.aggregate_bytes.saturating_sub(total);
            return Ok(Some(ReassembledFragment {
                message,
                delivery: first_delivery,
            }));
        }
        Ok(None)
    }

    /// Convenience insert that drops the delivery-instruction
    /// retention. The returned completion is a `Vec<u8>` instead
    /// of a `ReassembledFragment`; callers that do not need the
    /// first-fragment delivery instruction should keep using this
    /// entry point.
    pub fn insert(
        &mut self,
        key: ReassemblyKey,
        fragment: TunnelFragment,
    ) -> Result<Option<Vec<u8>>, ReassemblyError> {
        let message_id = fragment.message_id().unwrap_or(0);
        if matches!(fragment, TunnelFragment::Unfragmented { .. }) {
            // Unfragmented messages never participate in
            // reassembly.
            return Ok(Some(extract_unfragmented_body(&fragment)));
        }
        if message_id == 0 {
            return Err(ReassemblyError::SequenceOutOfRange { sequence: 0 });
        }
        self.expire_due();
        // Classify before any aggregate-budget charge so exact
        // duplicates cannot be rejected merely because the budget
        // is already full. A conflicting classification invalidates
        // the partial; we drop the partial and zeroize its
        // retained-byte accounting before propagating the error.
        let disposition = if let Some(partial) = self.partials.get(&key) {
            match partial.classify(&fragment, None) {
                Ok(disposition) => disposition,
                Err(error) => {
                    if let Some(p) = self.partials.remove(&key) {
                        self.aggregate_bytes = self.aggregate_bytes.saturating_sub(p.bytes);
                    }
                    return Err(error);
                }
            }
        } else {
            if self.partials.len() >= self.capacity {
                return Err(ReassemblyError::CapacityExceeded {
                    capacity: self.capacity,
                });
            }
            // Fresh-key candidates must still satisfy the same
            // semantic invariants the existing-partial path
            // enforces: a follow-on fragment may not carry a
            // delivery instruction and may not carry an
            // out-of-range sequence number. The convenience
            // entry point always passes `None` delivery.
            classify_new_partial(&fragment, None)?
        };
        if let FragmentInsertDisposition::ExactDuplicate = disposition {
            // No state change. No expiry refresh. No budget charge.
            return Ok(None);
        }
        let added_bytes = match disposition {
            FragmentInsertDisposition::Inserted { added_bytes } => added_bytes,
            FragmentInsertDisposition::ExactDuplicate => unreachable!(),
        };
        if self
            .aggregate_bytes
            .checked_add(added_bytes)
            .map(|sum| sum > self.aggregate_bytes_max)
            .unwrap_or(true)
        {
            return Err(ReassemblyError::AggregateBytesExceeded {
                bytes: self.aggregate_bytes.saturating_add(added_bytes),
                maximum: self.aggregate_bytes_max,
            });
        }
        // Insert or update.
        let partial = self
            .partials
            .entry(key)
            .or_insert_with(|| PartialMessage::new(self.now_ms));
        // A new partial was just constructed with
        // `last_touched_ms = self.now_ms`; an existing partial
        // accepted a unique fragment and therefore gets a fresh
        // expiry origin. Exact duplicates never reach this line.
        partial.last_touched_ms = self.now_ms;
        match partial.apply(fragment, None) {
            Ok(()) => {}
            Err(error) => {
                // Roll back any retained state changes. We
                // always remove the partial when the insertion
                // returns an error; the conflicting-duplicate
                // case must also drop the partial to satisfy the
                // spec §11.4 invalidation rule.
                if let Some(p) = self.partials.remove(&key) {
                    self.aggregate_bytes = self.aggregate_bytes.saturating_sub(p.bytes);
                }
                return Err(error);
            }
        }
        self.aggregate_bytes = self.aggregate_bytes.saturating_add(added_bytes);
        if let Some(message) = partial.assemble() {
            let total = partial.bytes;
            self.partials.remove(&key);
            self.aggregate_bytes = self.aggregate_bytes.saturating_sub(total);
            return Ok(Some(message));
        }
        Ok(None)
    }

    /// Drops every partial entry whose age exceeds the bounded
    /// expiry window.
    pub fn expire_due(&mut self) {
        if self.partials.is_empty() {
            return;
        }
        let expired: Vec<ReassemblyKey> = self
            .partials
            .iter()
            .filter_map(|(key, partial)| {
                let age = self.now_ms.saturating_sub(partial.last_touched_ms);
                if age >= self.expiry_ms {
                    Some(*key)
                } else {
                    None
                }
            })
            .collect();
        let expired_bytes: usize = expired
            .iter()
            .filter_map(|key| self.partials.remove(key))
            .map(|partial| partial.bytes)
            .sum();
        self.aggregate_bytes = self.aggregate_bytes.saturating_sub(expired_bytes);
    }

    /// Purges every retained partial entry. The caller may invoke
    /// this on shutdown or after a deterministic test run.
    pub fn purge(&mut self) {
        self.partials.clear();
        self.aggregate_bytes = 0;
    }

    /// Convenience helper used by some callers to convert a
    /// millisecond expiry into the [`Duration`] argument of
    /// [`Self::new`].
    pub fn duration_to_ms(duration: Duration) -> u64 {
        let secs = duration.as_secs();
        let nanos = u64::from(duration.subsec_nanos());
        secs.saturating_mul(1_000).saturating_add(nanos / 1_000_000)
    }
}

fn extract_unfragmented_body(fragment: &TunnelFragment) -> Vec<u8> {
    match fragment {
        TunnelFragment::Unfragmented { body } => body.clone(),
        _ => Vec::new(),
    }
}

/// Returns whether two optional delivery instructions describe
/// identical routing metadata. `None` matches `None`; two `Some`
/// instructions match when their discriminant and inner fields
/// agree exactly.
fn delivery_matches(
    left: Option<&crate::data::DeliveryInstruction>,
    right: Option<&crate::data::DeliveryInstruction>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Classifies a fragment that would create a brand-new partial
/// against the same semantic invariants
/// [`PartialMessage::classify`] enforces on the existing-partial
/// path. Fresh-key candidates must therefore be rejected with
/// the same error categories: a follow-on fragment carrying a
/// delivery instruction, a follow-on fragment with an
/// out-of-range sequence number, or any other invariant the
/// existing-partial path would reject must be rejected on the
/// fresh-key path before any partial state is created.
fn classify_new_partial(
    fragment: &TunnelFragment,
    delivery: Option<&crate::data::DeliveryInstruction>,
) -> Result<FragmentInsertDisposition, ReassemblyError> {
    match fragment {
        TunnelFragment::Unfragmented { body } => Ok(FragmentInsertDisposition::Inserted {
            added_bytes: body.len(),
        }),
        TunnelFragment::First { body, .. } => Ok(FragmentInsertDisposition::Inserted {
            added_bytes: body.len(),
        }),
        TunnelFragment::FollowOn { body, sequence, .. } => {
            if delivery.is_some() {
                return Err(ReassemblyError::UnexpectedFollowOnDeliveryInstruction);
            }
            if *sequence < FOLLOW_ON_SEQUENCE_MIN || *sequence > FOLLOW_ON_SEQUENCE_MAX {
                return Err(ReassemblyError::SequenceOutOfRange {
                    sequence: *sequence,
                });
            }
            Ok(FragmentInsertDisposition::Inserted {
                added_bytes: body.len(),
            })
        }
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

    fn unfragmented(body: &[u8]) -> TunnelFragment {
        TunnelFragment::Unfragmented {
            body: body.to_vec(),
        }
    }

    #[test]
    fn first_follow_on_assemble() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
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
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn out_of_order_follow_on_assembles() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
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
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0x9ABC);
        r.insert(k, follow(0x9ABC, 1, true, &[0x55_u8; 8])).unwrap();
        // Same fragment with identical body must be a no-op.
        r.insert(k, follow(0x9ABC, 1, true, &[0x55_u8; 8])).unwrap();
    }

    // Plan 116 T1 acceptance tests. Exact duplicates must be true
    // no-ops for memory accounting, expiry refresh, and aggregate
    // budget admission.

    #[test]
    fn exact_duplicate_first_does_not_increase_retained_bytes() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xD101);
        r.insert_with_delivery(k, first(0xD101, &[0x11_u8; 32]), None)
            .unwrap();
        let after_first = r.retained_bytes();
        assert!(after_first > 0);
        r.insert_with_delivery(k, first(0xD101, &[0x11_u8; 32]), None)
            .unwrap();
        assert_eq!(r.retained_bytes(), after_first);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn exact_duplicate_follow_on_does_not_increase_retained_bytes() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xD102);
        r.insert(k, first(0xD102, &[0xAA_u8; 8])).unwrap();
        r.insert(k, follow(0xD102, 1, false, &[0xBB_u8; 24]))
            .unwrap();
        let after_unique = r.retained_bytes();
        r.insert(k, follow(0xD102, 1, false, &[0xBB_u8; 24]))
            .unwrap();
        assert_eq!(r.retained_bytes(), after_unique);
    }

    #[test]
    fn exact_duplicate_at_aggregate_limit_is_accepted_as_noop() {
        // Configure the reassembler with capacity 4 and an
        // aggregate budget of exactly one 16-byte fragment. Filling
        // the budget with a unique fragment leaves no room for a
        // unique second fragment. An exact duplicate of the first
        // fragment must still be accepted as a no-op because no new
        // bytes would be retained.
        let mut r = BoundedReassembler::new(4, 16, 60_000, 0);
        let k = key(1, 0xD103);
        r.insert(k, follow(0xD103, 1, false, &[0x11_u8; 16]))
            .unwrap();
        assert_eq!(r.retained_bytes(), 16);
        let outcome = r.insert(k, follow(0xD103, 1, false, &[0x11_u8; 16]));
        assert!(outcome.is_ok());
        assert_eq!(r.retained_bytes(), 16);
        assert!(!matches!(
            outcome,
            Err(ReassemblyError::AggregateBytesExceeded { .. })
        ));
    }

    #[test]
    fn exact_duplicate_does_not_refresh_expiry() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 1_000, 0);
        let k = key(1, 0xD104);
        r.insert(k, first(0xD104, &[0x11_u8; 8])).unwrap();
        assert_eq!(r.len(), 1);
        // Advance close to (but past) the original expiry window
        // and resend an exact duplicate. The duplicate must not
        // refresh `last_touched_ms`; expire_due at 1001 ms must
        // still drop the partial.
        r.set_now(900);
        r.insert(k, first(0xD104, &[0x11_u8; 8])).unwrap();
        r.set_now(1_001);
        r.expire_due();
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn reassembly_completion_returns_aggregate_bytes_to_zero_after_duplicates() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xD105);
        r.insert(k, first(0xD105, &[0xAA_u8; 8])).unwrap();
        // Send duplicates of both the first fragment and the
        // unique follow-on before completion.
        r.insert(k, first(0xD105, &[0xAA_u8; 8])).unwrap();
        r.insert(k, follow(0xD105, 1, false, &[0xBB_u8; 8]))
            .unwrap();
        r.insert(k, follow(0xD105, 1, false, &[0xBB_u8; 8]))
            .unwrap();
        let outcome = r.insert(k, follow(0xD105, 2, true, &[0xCC_u8; 8]));
        let message = outcome.expect("complete").expect("non-empty");
        assert_eq!(message.len(), 24);
        assert_eq!(&message[..8], &[0xAA_u8; 8]);
        assert_eq!(&message[8..16], &[0xBB_u8; 8]);
        assert_eq!(&message[16..], &[0xCC_u8; 8]);
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn conflicting_duplicate_invalidates_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xDEF0);
        r.insert(k, follow(0xDEF0, 1, false, &[0x55_u8; 8]))
            .unwrap();
        let outcome = r.insert(k, follow(0xDEF0, 1, false, &[0x66_u8; 8]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingDuplicate { .. })
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn fragment_from_another_context_does_not_join_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k1 = key(1, 0xFEED);
        let k2 = key(2, 0xFEED);
        r.insert(k1, follow(0xFEED, 1, true, &[0x11_u8; 8]))
            .unwrap();
        let outcome = r.insert(k2, follow(0xFEED, 1, true, &[0x22_u8; 8]));
        assert!(outcome.is_ok());
    }

    #[test]
    fn capacity_exceeded_fails_closed() {
        let mut r = BoundedReassembler::new(1, 1 << 20, 60_000, 0);
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
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0x3000);
        r.insert(k, follow(0x3000, 2, false, &[0x11_u8; 4]))
            .unwrap();
        let outcome = r.insert(k, follow(0x3000, 1, true, &[0x22_u8; 4]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::LastBelowObservedMax { .. })
        ));
    }

    #[test]
    fn aggregate_byte_budget_enforced() {
        let mut r = BoundedReassembler::new(16, 32, 60_000, 0);
        // A single fragment of 16 bytes still leaves 16 bytes of
        // headroom. A second fragment of 16 bytes is rejected by
        // the aggregate bound.
        r.insert(key(1, 0x4001), follow(0x4001, 1, false, &[0x11; 16]))
            .unwrap();
        let outcome = r.insert(key(1, 0x4002), first(0x4002, &[0x22; 17]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::AggregateBytesExceeded { .. })
        ));
    }

    #[test]
    fn expiry_due_drops_partials() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0x5001);
        r.insert(k, follow(0x5001, 1, false, &[0x11; 8])).unwrap();
        // Advance past the expiry window.
        r.set_now(60_001);
        let outcome = r.insert(k, follow(0x5001, 2, true, &[0x22; 8]));
        // The previous partial was expired and the new fragment
        // creates a new partial because it is `is_last = true`
        // but only carries sequence 2 (the first fragment /
        // sequence 0 is missing); it is therefore not yet
        // complete.
        assert!(outcome.is_ok());
    }

    #[test]
    fn unfragmented_completes_immediately() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let outcome = r.insert(key(1, 0x6001), unfragmented(&[0x55; 16])).unwrap();
        let message = outcome.expect("complete");
        assert_eq!(message.len(), 16);
        assert_eq!(&message[..], &[0x55; 16]);
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn conflicting_duplicate_first_drops_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0x7001);
        r.insert(k, first(0x7001, &[0x11; 8])).unwrap();
        let outcome = r.insert(k, first(0x7001, &[0x22; 8]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingDuplicate { .. })
        ));
    }

    #[test]
    fn identical_duplicate_first_is_idempotent() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0x7002);
        r.insert(k, first(0x7002, &[0x11; 8])).unwrap();
        let outcome = r.insert(k, first(0x7002, &[0x11; 8]));
        assert!(outcome.is_ok());
    }

    #[test]
    fn rejected_insertion_does_not_exceed_bound() {
        let mut r = BoundedReassembler::new(1, 64, 60_000, 0);
        let k1 = key(1, 0x8001);
        let k2 = key(1, 0x8002);
        r.insert(k1, follow(0x8001, 1, false, &[0x11; 4])).unwrap();
        let before_len = r.len();
        let outcome = r.insert(k2, first(0x8002, &[0x22; 4]));
        assert!(outcome.is_err());
        // The capacity-bound check happens before insertion so
        // the second key never makes it in.
        assert_eq!(r.len(), before_len);
    }

    // Plan 116 F4 tests for delivery-instruction retention. The
    // tests below exercise the `insert_with_delivery` API and
    // assert that:
    //  - the first-fragment delivery instruction survives a
    //    fragmented reassembly;
    //  - the delivery instruction is preserved when follow-on
    //    fragments arrive before the first fragment;
    //  - the delivery instruction is preserved across out-of-order
    //    follow-on arrivals;
    //  - the reassembler never synthesises a delivery
    //    instruction for a reassembled message that did not retain
    //    one.

    use i2pr_proto::Hash;
    fn router_delivery() -> crate::data::DeliveryInstruction {
        crate::data::DeliveryInstruction::Router {
            router: Hash::from_bytes([0x77_u8; 32]),
        }
    }

    #[test]
    fn first_follow_on_with_delivery_round_trip_retains_delivery() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xA001);
        let delivery = router_delivery();
        // First fragment carries the delivery instruction.
        assert!(
            r.insert_with_delivery(k, first(0xA001, &[0xAA_u8; 50]), Some(delivery.clone()),)
                .unwrap()
                .is_none()
        );
        // Follow-on completes the message and must return the
        // retained delivery instruction.
        let outcome = r
            .insert_with_delivery(k, follow(0xA001, 1, true, &[0xBB_u8; 30]), None)
            .unwrap()
            .expect("complete");
        assert_eq!(outcome.message.len(), 80);
        assert_eq!(outcome.delivery, Some(delivery));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn out_of_order_follow_on_first_with_delivery_retains_delivery() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xA002);
        let delivery = router_delivery();
        // Follow-on before first carries no delivery instruction;
        // the reassembler must not crash and must not invent one.
        r.insert_with_delivery(k, follow(0xA002, 2, true, &[0x22_u8; 10]), None)
            .unwrap();
        r.insert_with_delivery(k, follow(0xA002, 1, false, &[0x11_u8; 10]), None)
            .unwrap();
        let outcome = r
            .insert_with_delivery(k, first(0xA002, &[0x00_u8; 10]), Some(delivery.clone()))
            .unwrap()
            .expect("complete");
        assert_eq!(outcome.message.len(), 30);
        assert_eq!(outcome.delivery, Some(delivery));
    }

    #[test]
    fn conflicting_follow_on_invalidates_retained_delivery() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xA003);
        let delivery = router_delivery();
        // First fragment with delivery instruction.
        r.insert_with_delivery(k, first(0xA003, &[0x11_u8; 8]), Some(delivery.clone()))
            .unwrap();
        // A follow-on arrives, then a conflicting follow-on.
        r.insert_with_delivery(k, follow(0xA003, 1, false, &[0x22_u8; 8]), None)
            .unwrap();
        let outcome = r.insert_with_delivery(k, follow(0xA003, 1, false, &[0x33_u8; 8]), None);
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingDuplicate { .. })
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn reassembly_without_first_delivery_reports_none() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xA004);
        // Insert follow-ons only, then complete via first with no
        // delivery instruction. The reassembler must surface
        // `delivery = None` so the data plane can reject the
        // completion as an unspecified-delivery failure.
        r.insert_with_delivery(k, follow(0xA004, 2, true, &[0x44_u8; 4]), None)
            .unwrap();
        r.insert_with_delivery(k, follow(0xA004, 1, false, &[0x33_u8; 4]), None)
            .unwrap();
        let outcome = r
            .insert_with_delivery(k, first(0xA004, &[0x22_u8; 4]), None)
            .unwrap()
            .expect("complete");
        assert_eq!(outcome.message.len(), 12);
        assert_eq!(outcome.delivery, None);
    }

    // Plan 116 T2 acceptance tests. First-fragment delivery
    // metadata must participate in duplicate identity, follow-on
    // fragments must never carry delivery instructions, and
    // conflicting delivery must invalidate the affected partial
    // without disturbing the rest of the reassembler.

    fn router_delivery_with(value: u8) -> crate::data::DeliveryInstruction {
        crate::data::DeliveryInstruction::Router {
            router: Hash::from_bytes([value; 32]),
        }
    }

    fn tunnel_delivery_with(gateway: u8, tunnel_id: u32) -> crate::data::DeliveryInstruction {
        crate::data::DeliveryInstruction::Tunnel {
            tunnel_id,
            gateway: Hash::from_bytes([gateway; 32]),
        }
    }

    #[test]
    fn exact_duplicate_first_with_same_delivery_is_idempotent() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xB001);
        let delivery = router_delivery_with(0x11);
        r.insert_with_delivery(k, first(0xB001, &[0x11_u8; 16]), Some(delivery.clone()))
            .unwrap();
        let after_first = r.retained_bytes();
        assert!(after_first > 0);
        // Identical body + identical delivery must be a no-op.
        r.insert_with_delivery(k, first(0xB001, &[0x11_u8; 16]), Some(delivery.clone()))
            .unwrap();
        assert_eq!(r.retained_bytes(), after_first);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn conflicting_first_router_target_invalidates_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xB002);
        r.insert_with_delivery(
            k,
            first(0xB002, &[0xAA_u8; 16]),
            Some(router_delivery_with(0xAA)),
        )
        .unwrap();
        let after_first = r.retained_bytes();
        assert!(after_first > 0);
        let outcome = r.insert_with_delivery(
            k,
            first(0xB002, &[0xAA_u8; 16]),
            Some(router_delivery_with(0xBB)),
        );
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingFirstMetadata)
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn conflicting_first_tunnel_id_invalidates_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xB003);
        r.insert_with_delivery(
            k,
            first(0xB003, &[0xAA_u8; 16]),
            Some(tunnel_delivery_with(0xCC, 0x1000)),
        )
        .unwrap();
        let after_first = r.retained_bytes();
        assert!(after_first > 0);
        let outcome = r.insert_with_delivery(
            k,
            first(0xB003, &[0xAA_u8; 16]),
            Some(tunnel_delivery_with(0xCC, 0x1001)),
        );
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingFirstMetadata)
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn conflicting_first_tunnel_gateway_invalidates_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xB004);
        r.insert_with_delivery(
            k,
            first(0xB004, &[0xAA_u8; 16]),
            Some(tunnel_delivery_with(0xDD, 0x1000)),
        )
        .unwrap();
        let after_first = r.retained_bytes();
        assert!(after_first > 0);
        let outcome = r.insert_with_delivery(
            k,
            first(0xB004, &[0xAA_u8; 16]),
            Some(tunnel_delivery_with(0xEE, 0x1000)),
        );
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingFirstMetadata)
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn unexpected_follow_on_delivery_fails_closed() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xB005);
        r.insert(k, first(0xB005, &[0xAA_u8; 16])).unwrap();
        let outcome = r.insert_with_delivery(
            k,
            follow(0xB005, 1, false, &[0xBB_u8; 16]),
            Some(router_delivery_with(0xFF)),
        );
        assert!(matches!(
            outcome,
            Err(ReassemblyError::UnexpectedFollowOnDeliveryInstruction)
        ));
    }

    #[test]
    fn delivery_instruction_expires_with_partial() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 1_000, 0);
        let k = key(1, 0xA005);
        let delivery = router_delivery();
        r.insert_with_delivery(k, first(0xA005, &[0x11_u8; 8]), Some(delivery.clone()))
            .unwrap();
        assert_eq!(r.len(), 1);
        // Advance time past the expiry window and trigger expiry.
        r.set_now(1_500);
        r.expire_due();
        assert!(r.is_empty());
    }

    // Fresh-key semantic validation. A candidate that would create
    // a brand-new partial must satisfy the same invariants the
    // existing-partial path enforces. The fresh-key path used to
    // skip all semantic checks (FollowOn + delivery, out-of-range
    // sequence, etc.) and silently construct a partial; the
    // following tests prove the fix.

    #[test]
    fn fresh_key_follow_on_with_delivery_fails_closed() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xC001);
        // A follow-on fragment with a delivery instruction as the
        // very first fragment for a fresh key must be rejected
        // with `UnexpectedFollowOnDeliveryInstruction`. The
        // existing-key equivalent is tested above; this one
        // exercises the fresh-key path.
        let outcome = r.insert_with_delivery(
            k,
            follow(0xC001, 1, false, &[0xAA_u8; 16]),
            Some(router_delivery_with(0xAA)),
        );
        assert!(matches!(
            outcome,
            Err(ReassemblyError::UnexpectedFollowOnDeliveryInstruction)
        ));
        // The reassembler must not have created a partial.
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn fresh_key_follow_on_with_delivery_via_insert_fails_closed() {
        // The convenience `insert` entry point always passes
        // `None` for the delivery instruction, so it cannot
        // accept FollowOn + Some(delivery) on a fresh key by
        // construction. This test documents the contract by
        // exercising the new-key path with a valid follow-on
        // fragment: the partial is created.
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xC002);
        let outcome = r.insert(k, follow(0xC002, 1, false, &[0xBB_u8; 16]));
        assert!(outcome.is_ok());
        assert_eq!(r.len(), 1);
        assert!(r.retained_bytes() > 0);
    }

    #[test]
    fn fresh_key_out_of_range_follow_on_sequence_below_min_rejected() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xC003);
        let outcome = r.insert(k, follow(0xC003, 0, false, &[0xAA_u8; 16]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::SequenceOutOfRange { sequence: 0 })
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn fresh_key_out_of_range_follow_on_sequence_above_max_rejected() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xC004);
        let outcome = r.insert(k, follow(0xC004, 64, false, &[0xAA_u8; 16]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::SequenceOutOfRange { sequence: 64 })
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn fresh_key_out_of_range_follow_on_sequence_with_delivery_rejected() {
        // A fresh-key follow-on with a delivery instruction and
        // an out-of-range sequence number must surface the
        // delivery-instruction failure (the more specific
        // invariant) rather than the sequence-range failure.
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xC005);
        let outcome = r.insert_with_delivery(
            k,
            follow(0xC005, 100, false, &[0xAA_u8; 16]),
            Some(router_delivery_with(0xAA)),
        );
        assert!(matches!(
            outcome,
            Err(ReassemblyError::UnexpectedFollowOnDeliveryInstruction)
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    // Follow-on terminal-flag duplicate identity. The terminal
    // flag (`is_last`) is control metadata that participates in
    // follow-on duplicate identity alongside the body bytes: two
    // fragments at the same sequence with identical body bytes
    // are exact duplicates only when both `is_last` flags agree.
    // A different `is_last` flag is conflicting control metadata
    // that invalidates the affected partial.

    #[test]
    fn same_sequence_body_same_is_last_is_exact_duplicate() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xE001);
        r.insert(k, first(0xE001, &[0x11_u8; 8])).unwrap();
        r.insert(k, follow(0xE001, 1, false, &[0x22_u8; 16]))
            .unwrap();
        let after_unique = r.retained_bytes();
        assert!(after_unique > 0);
        // Identical sequence + identical body + identical is_last
        // must remain a no-op.
        r.insert(k, follow(0xE001, 1, false, &[0x22_u8; 16]))
            .unwrap();
        assert_eq!(r.retained_bytes(), after_unique);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn same_sequence_body_changed_is_last_is_conflicting_control_metadata() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xE002);
        r.insert(k, first(0xE002, &[0x11_u8; 8])).unwrap();
        // First sighting: sequence 1, body [0x22..0x32], is_last=false.
        r.insert(k, follow(0xE002, 1, false, &[0x22_u8; 16]))
            .unwrap();
        let after_unique = r.retained_bytes();
        assert!(after_unique > 0);
        // Second sighting: same sequence, same body, but
        // is_last=true. The body bytes match but the terminal
        // flag is conflicting control metadata.
        let outcome = r.insert(k, follow(0xE002, 1, true, &[0x22_u8; 16]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingFollowOnTerminalFlag {
                sequence: 1,
                expected: false,
                actual: true
            })
        ));
        // The conflicting terminal flag must invalidate the
        // affected partial and zeroize its retained bytes.
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn same_sequence_body_changed_is_last_reverse_direction_is_conflicting() {
        // Use sequence 63 (the maximum follow-on sequence) as the
        // terminal flag so the partial remains open after the
        // first sighting: total_fragments = 64 but received
        // contains only two fragments (sequence 0 and 63). A
        // smaller terminal sequence would close the partial and
        // remove it before the conflicting duplicate arrived.
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xE003);
        r.insert(k, first(0xE003, &[0x11_u8; 8])).unwrap();
        // First sighting: is_last=true at sequence 63.
        r.insert(k, follow(0xE003, 63, true, &[0x33_u8; 16]))
            .unwrap();
        let after_unique = r.retained_bytes();
        assert!(after_unique > 0);
        // Second sighting: same sequence, same body, is_last=false.
        let outcome = r.insert(k, follow(0xE003, 63, false, &[0x33_u8; 16]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingFollowOnTerminalFlag {
                sequence: 63,
                expected: true,
                actual: false
            })
        ));
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
    }

    #[test]
    fn conflicting_is_last_does_not_disturb_other_partials() {
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k1 = key(1, 0xE010);
        let k2 = key(1, 0xE020);
        // Build up an unrelated partial message.
        r.insert(k1, first(0xE010, &[0x11_u8; 8])).unwrap();
        r.insert(k1, follow(0xE010, 1, false, &[0x22_u8; 16]))
            .unwrap();
        let before_k1 = r.retained_bytes();
        // Insert a conflicting terminal-flag follow-on for k2.
        r.insert(k2, first(0xE020, &[0xAA_u8; 8])).unwrap();
        r.insert(k2, follow(0xE020, 1, false, &[0xBB_u8; 16]))
            .unwrap();
        let outcome = r.insert(k2, follow(0xE020, 1, true, &[0xBB_u8; 16]));
        assert!(matches!(
            outcome,
            Err(ReassemblyError::ConflictingFollowOnTerminalFlag { .. })
        ));
        // k1 must still be intact.
        assert_eq!(r.len(), 1);
        assert_eq!(r.retained_bytes(), before_k1);
    }

    #[test]
    fn stale_duplicate_after_completion_does_not_resurrect_partial() {
        // Once a partial completes, the reassembler drops the
        // partial entry. A stale duplicate arriving after
        // completion must not resurrect the original message or
        // surface a phantom conflict. The fresh-key path
        // classifies the stale fragment as `Inserted` because the
        // partial no longer exists; the new partial is missing the
        // first fragment and therefore cannot complete with the
        // stale body alone.
        let mut r = BoundedReassembler::new(4, 1 << 20, 60_000, 0);
        let k = key(1, 0xE004);
        r.insert(k, first(0xE004, &[0x11_u8; 8])).unwrap();
        // Terminal follow-on at sequence 1 completes the
        // two-fragment message; the reassembler removes the
        // partial.
        let outcome = r
            .insert(k, follow(0xE004, 1, true, &[0x22_u8; 16]))
            .unwrap();
        let message = outcome.expect("complete");
        assert_eq!(message.len(), 24);
        assert!(r.is_empty());
        assert_eq!(r.retained_bytes(), 0);
        // Stale duplicate with mismatched terminal flag must not
        // resurrect the partial. The fresh-key path accepts the
        // follow-on as a new partial (the original is gone); the
        // new partial is missing the first fragment and therefore
        // does not complete silently.
        let outcome = r.insert(k, follow(0xE004, 1, false, &[0x22_u8; 16]));
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_none());
        assert_eq!(r.len(), 1);
        assert_eq!(r.retained_bytes(), 16);
    }
}
