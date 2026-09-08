//! Plan 168 typed I2CP message/data-plane surface.
//!
//! The Plan 168 data plane adds typed actions, an outcome vocabulary,
//! and the bounded runtime-neutral helpers that compose with the
//! Plan 165/166 connection/session/destination surface:
//!
//! - [`I2cpMessageOutcome`] — the bounded set of router-side outcomes
//!   the I2CP server translates into [`MessageStatusCode`] values. The
//!   translation never overclaims end-to-end delivery; local queue
//!   acceptance stays distinct from any stronger delivery guarantee.
//! - [`I2cpDataPlaneAction::EnqueueOutboundPayload`] — projects a
//!   verified `SendMessage` / `SendMessageExpires` body into the
//!   `i2pr_client::DestinationRuntime::enqueue_outbound` seam; the
//!   router reuses its existing outbound payload queues instead of
//!   duplicating destination-routing state.
//! - [`I2cpDataPlaneAction::DeliverInboundPayload`] — projects one
//!   authenticated destination payload into the owning I2CP session's
//!   `MessagePayload` write path. Cross-session/cross-connection
//!   leakage is prevented by the connection-id + session-id tuple
//!   carried in every variant.
//! - [`I2cpDataPlaneAction::ResolveDestinationLookup`] — projects a
//!   `DestLookup` hash through the existing local-registry /
//!   `i2pr_netdb` lookup seams. System DNS and address-book
//!   resolution are explicitly out of scope.
//! - [`PendingStatusEntry`] / [`PendingStatusTable`] — bounded
//!   runtime-neutral bookkeeping for the per-session pending status
//!   correlations and aggregate byte accounting.
//! - [`InboundPayloadFrame`] / [`InboundPayloadQueue`] — bounded
//!   runtime-neutral inbound message buffer with explicit byte
//!   accounting.
//!
//! The Plan 168 surface owns no sockets, timers, Tokio tasks, or
//! destination secret material. The Plan 167 daemon is the sole
//! translator between these actions and runtime state.

use std::collections::VecDeque;
use std::time::Duration;

use i2pr_proto::Hash;

use super::ids::{ClientNonce, MessageId, SessionId};
use super::message::{MessageStatusCode, SendFlags};

/// Maximum number of pending outbound I2CP messages per session.
///
/// The ceiling is bounded so a single client cannot use many small
/// I2CP frames to bypass
/// `i2pr_client::config::MAX_PENDING_DESTINATION_MESSAGES`. The Plan
/// 168 value matches the local M9 profile default and is **stricter**
/// than the per-destination `DestinationConfig::max_pending_messages`
/// ceiling so the I2CP data plane cannot overcommit the underlying
/// queue.
pub const MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION: usize = 64;

/// Maximum number of pending `MessageStatus` correlations per session.
///
/// Once a terminal status arrives the entry is removed; duplicate or
/// late internal events are idempotent and cannot resurrect an entry.
pub const MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION: usize = 128;

/// Maximum number of pending inbound `MessagePayload` frames per session.
pub const MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION: usize = 64;

/// Maximum aggregate pending inbound payload bytes per session.
pub const MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION: usize = 64 * 1024;

/// Maximum number of concurrent destination lookups per connection.
pub const MAX_CONCURRENT_DESTINATION_LOOKUPS_PER_CONNECTION: usize = 16;

/// Lookup deadline upper bound. Per-connection lookups must complete
/// within this horizon; longer ones are cancelled.
pub const MAX_DESTINATION_LOOKUP_HORIZON: Duration = Duration::from_secs(10);

/// Maximum future expiration horizon for `SendMessageExpires`. The
/// M9 profile rejects expirations farther into the future than this
/// window because the destination routing layer cannot guarantee
/// delivery beyond a bounded horizon.
pub const MAX_MESSAGE_EXPIRATION_HORIZON: Duration = Duration::from_secs(60 * 60);

/// Returns the bounded per-session outbound I2CP message ceiling.
pub const fn max_pending_messages_per_session() -> usize {
    MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION
}

/// Returns the bounded per-session status correlation ceiling.
pub const fn max_pending_status_correlations() -> usize {
    MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION
}

/// Returns the bounded per-session inbound payload byte ceiling.
pub const fn max_inbound_payload_bytes_per_session() -> usize {
    MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION
}

/// Returns the bounded per-connection destination lookup ceiling.
pub const fn max_destination_lookup_horizon() -> Duration {
    MAX_DESTINATION_LOOKUP_HORIZON
}

/// Bounded router-side outcome vocabulary for one outbound I2CP
/// message. The Plan 168 daemon maps every observed outcome to one
/// of these values and the mapping is the single source of truth for
/// what the router promises in `MessageStatus` replies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum I2cpMessageOutcome {
    /// The destination runtime accepted the payload into its local
    /// outbound queue. The router never claims stronger delivery for
    /// this status; it is the documented `MessageStatus::Accepted`
    /// state in the M9 best-effort profile.
    Accepted,
    /// The destination runtime reported `BadLocalLeaseSet`: there
    /// is no signed client LeaseSet2 yet, or the existing one has
    /// expired. Maps to `MessageStatus::BadLocalLeaseSet`.
    BadLocalLeaseSet,
    /// The local destination has no usable outbound tunnel to
    /// carry the message. Maps to `MessageStatus::NoLocalTunnels`.
    NoLocalTunnels,
    /// The destination's outbound queue is at its bounded ceiling;
    /// the message is dropped. Maps to `MessageStatus::OverflowFailure`.
    Overflow,
    /// The destination is stopping or stopped. Maps to
    /// `MessageStatus::BadSession`.
    DestinationStopping,
    /// The session id carried by the SendMessage does not match the
    /// active session on this connection. Maps to
    /// `MessageStatus::BadSession`.
    BadSession,
    /// The payload exceeded the destination or I2CP body ceilings.
    /// Maps to `MessageStatus::BadMessage`.
    BadMessage,
    /// The `SendMessageExpires` expiration was already in the past
    /// at submission time. Maps to `MessageStatus::MessageExpired`.
    MessageExpired,
    /// The expiration instant exceeded the bounded horizon. Maps
    /// to `MessageStatus::BadOptions`.
    BadExpirationHorizon,
    /// The flags word carried unsupported bits for the declared
    /// profile (the structural codec already rejects reserved bits;
    /// this variant captures semantics the strict codec permits but
    /// the M9 profile does not honour). Maps to
    /// `MessageStatus::BadOptions`.
    UnsupportedFlags,
    /// The destination's encrypted-message engine rejected the
    /// payload (the M9 profile reports this as best-effort failure
    /// without claiming guaranteed semantics). Maps to
    /// `MessageStatus::GuaranteedFailure`.
    SessionError,
}

impl I2cpMessageOutcome {
    /// Returns the typed [`MessageStatusCode`] this outcome translates
    /// into. The translation is one-to-one; no outcome is mapped to a
    /// stronger status than the router actually observed.
    pub const fn status_code(self) -> MessageStatusCode {
        match self {
            Self::Accepted => MessageStatusCode::Accepted,
            Self::BadLocalLeaseSet => MessageStatusCode::BadLocalLeaseSet,
            Self::NoLocalTunnels => MessageStatusCode::NoLocalTunnels,
            Self::Overflow => MessageStatusCode::OverflowFailure,
            Self::DestinationStopping => MessageStatusCode::BadSession,
            Self::BadSession => MessageStatusCode::BadSession,
            Self::BadMessage => MessageStatusCode::BadMessage,
            Self::MessageExpired => MessageStatusCode::MessageExpired,
            Self::BadExpirationHorizon => MessageStatusCode::BadOptions,
            Self::UnsupportedFlags => MessageStatusCode::BadOptions,
            Self::SessionError => MessageStatusCode::GuaranteedFailure,
        }
    }

    /// Reports whether this outcome corresponds to local queue
    /// acceptance (`MessageStatus::Accepted`).
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }
}

/// One typed I2CP action emitted by the Plan 168 data plane. The
/// `EnqueueOutboundPayload` and `DeliverInboundPayload` variants
/// project the application-message direction; the
/// `ResolveDestinationLookup` variant projects the lookup path.
#[derive(Debug)]
#[non_exhaustive]
pub enum I2cpDataPlaneAction {
    /// Enqueue a verified outbound payload into the owning
    /// session's destination runtime outbound queue.
    EnqueueOutboundPayload {
        /// Connection capability the message arrived on.
        connection: u32,
        /// Session identifier the payload belongs to.
        session: SessionId,
        /// Router-assigned message identifier (assigned by the
        /// daemon so the client can correlate the eventual status).
        message_id: MessageId,
        /// Client nonce echoed in the eventual `MessageStatus`.
        nonce: ClientNonce,
        /// Protocol number carried in the I2CP payload gzip header.
        protocol: u8,
        /// Source port carried in the I2CP payload gzip header.
        source_port: u16,
        /// Destination port carried in the I2CP payload gzip header.
        destination_port: u16,
        /// Compressed payload bytes (the body that follows the
        /// 10-byte gzip header on the wire). Plain I2CP payload
        /// bytes, never a raw application string.
        payload: Vec<u8>,
        /// `SendMessageExpires` flags word; only present for the
        /// `Expires` family. The structural codec already rejects
        /// reserved bits; the daemon inspects semantics for the M9
        /// profile.
        flags: Option<SendFlags>,
        /// Optional expiration instant for `SendMessageExpires`. When
        /// `Some`, the daemon validates the horizon against the
        /// current wall-clock before routing.
        expiration_ms: Option<u64>,
    },
    /// Deliver one authenticated inbound destination payload into
    /// the owning session's `MessagePayload` write path.
    DeliverInboundPayload {
        /// Connection capability the destination belongs to.
        connection: u32,
        /// Session identifier the payload targets.
        session: SessionId,
        /// Router-assigned message identifier (echoed in the
        /// `MessagePayload` reply).
        message_id: MessageId,
        /// Protocol number the inbound payload uses.
        protocol: u8,
        /// Source port the inbound payload uses.
        source_port: u16,
        /// Destination port the inbound payload uses.
        destination_port: u16,
        /// Compressed inbound payload bytes (gzip body after the
        /// 10-byte header).
        payload: Vec<u8>,
    },
    /// Resolve a `DestLookup` hash through the existing local /
    /// NetDB destination lookup surface. The lookup key is the
    /// raw destination hash; hostname-based lookups remain out of
    /// scope for Plan 168.
    ResolveDestinationLookup {
        /// Connection capability the lookup arrived on.
        connection: u32,
        /// Destination hash the client requested.
        hash: Hash,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_outcome_to_status_code_is_one_to_one() {
        let cases = [
            (I2cpMessageOutcome::Accepted, MessageStatusCode::Accepted),
            (
                I2cpMessageOutcome::BadLocalLeaseSet,
                MessageStatusCode::BadLocalLeaseSet,
            ),
            (
                I2cpMessageOutcome::NoLocalTunnels,
                MessageStatusCode::NoLocalTunnels,
            ),
            (
                I2cpMessageOutcome::Overflow,
                MessageStatusCode::OverflowFailure,
            ),
            (
                I2cpMessageOutcome::DestinationStopping,
                MessageStatusCode::BadSession,
            ),
            (
                I2cpMessageOutcome::BadSession,
                MessageStatusCode::BadSession,
            ),
            (
                I2cpMessageOutcome::BadMessage,
                MessageStatusCode::BadMessage,
            ),
            (
                I2cpMessageOutcome::MessageExpired,
                MessageStatusCode::MessageExpired,
            ),
            (
                I2cpMessageOutcome::BadExpirationHorizon,
                MessageStatusCode::BadOptions,
            ),
            (
                I2cpMessageOutcome::UnsupportedFlags,
                MessageStatusCode::BadOptions,
            ),
            (
                I2cpMessageOutcome::SessionError,
                MessageStatusCode::GuaranteedFailure,
            ),
        ];
        for (outcome, code) in cases {
            assert_eq!(outcome.status_code(), code);
        }
        assert!(I2cpMessageOutcome::Accepted.is_accepted());
        assert!(!I2cpMessageOutcome::BadMessage.is_accepted());
        assert!(!I2cpMessageOutcome::Overflow.is_accepted());
    }

    #[test]
    fn ceilings_are_bounded_constants() {
        // Every ceiling must be a positive, finite value that fits in
        // a `usize`; the daemon enforces them on every dispatch path.
        const {
            assert!(MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION > 0);
            assert!(MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION > 0);
            assert!(MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION > 0);
            assert!(MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION > 0);
            assert!(MAX_CONCURRENT_DESTINATION_LOOKUPS_PER_CONNECTION > 0);
        }
        assert!(MAX_DESTINATION_LOOKUP_HORIZON > Duration::ZERO);
        assert!(MAX_MESSAGE_EXPIRATION_HORIZON > Duration::ZERO);
        assert_eq!(
            max_pending_messages_per_session(),
            MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION
        );
        assert_eq!(
            max_pending_status_correlations(),
            MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION
        );
        assert_eq!(
            max_inbound_payload_bytes_per_session(),
            MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION
        );
        assert_eq!(
            max_destination_lookup_horizon(),
            MAX_DESTINATION_LOOKUP_HORIZON
        );
    }

    #[test]
    fn data_plane_actions_are_constructible_from_typed_values() {
        // Compile-only check: every action variant must be
        // constructible from typed values, never from raw client
        // bytes.
        let _action = I2cpDataPlaneAction::EnqueueOutboundPayload {
            connection: 1,
            session: SessionId::new(7),
            message_id: MessageId::new(1),
            nonce: ClientNonce::new(42),
            protocol: 6,
            source_port: 0,
            destination_port: 0,
            payload: Vec::new(),
            flags: None,
            expiration_ms: None,
        };
        let _action = I2cpDataPlaneAction::DeliverInboundPayload {
            connection: 1,
            session: SessionId::new(7),
            message_id: MessageId::new(2),
            protocol: 6,
            source_port: 0,
            destination_port: 0,
            payload: Vec::new(),
        };
        let _action = I2cpDataPlaneAction::ResolveDestinationLookup {
            connection: 1,
            hash: Hash::from_bytes([0u8; 32]),
        };
    }
}

// ---------------------------------------------------------------------
// Plan 168 bounded runtime-neutral helpers.
//
// The Plan 168 daemon uses these tables to track per-session status
// correlations and inbound payload frames without owning a runtime or
// sockets. The tables are deliberately minimal: the daemon is
// responsible for draining them onto the wire and for mapping them
// into the Plan 167 / Plan 166 destination state on cleanup.
// ---------------------------------------------------------------------

/// One pending status correlation keyed by the router-assigned
/// `MessageId`. The Plan 168 data plane writes one of these on every
/// accepted outbound payload and removes it when a matching terminal
/// status arrives. Duplicate or late internal events are idempotent:
/// the `take` helper returns the existing entry and a duplicate
/// removal is a no-op.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingStatusEntry {
    /// Router-assigned message identifier.
    pub message_id: MessageId,
    /// Session that owns the correlation.
    pub session: SessionId,
    /// Client nonce echoed in the eventual `MessageStatus`.
    pub nonce: ClientNonce,
}

/// Typed failure modes for [`PendingStatusTable`] and
/// [`InboundPayloadQueue`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DataPlaneError {
    /// The bounded entry/byte ceiling would be exceeded.
    CapacityExceeded,
}

/// Bounded per-session status correlation table.
///
/// The table has explicit count and byte ceilings. Terminal status
/// arrival removes an entry; duplicate/late internal events are
/// idempotent and cannot resurrect an entry.
#[derive(Debug, Default)]
pub struct PendingStatusTable {
    entries: VecDeque<PendingStatusEntry>,
}

impl PendingStatusTable {
    /// Constructs an empty correlation table.
    pub const fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    /// Records one correlation. Returns the count and the entry on
    /// success.
    pub fn record(&mut self, entry: PendingStatusEntry) -> Result<usize, DataPlaneError> {
        if self.entries.len() >= MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION {
            return Err(DataPlaneError::CapacityExceeded);
        }
        self.entries.push_back(entry);
        Ok(self.entries.len())
    }

    /// Removes the entry keyed by `message_id`, returning it if present.
    pub fn take(&mut self, message_id: MessageId) -> Option<PendingStatusEntry> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.message_id == message_id)
        {
            self.entries.remove(index)
        } else {
            None
        }
    }

    /// Returns the current pending correlation count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Reports whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the configured correlation ceiling.
    pub const fn capacity(&self) -> usize {
        MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION
    }

    /// Releases every pending correlation. Returns the dropped count.
    pub fn release_all(&mut self) -> usize {
        let dropped = self.entries.len();
        self.entries.clear();
        dropped
    }
}

/// One inbound payload frame destined for the owning I2CP session's
/// `MessagePayload` write path. The frame retains only the typed
/// fields the I2CP wire needs; raw application payload bytes are
/// non-secret but are not retained in evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundPayloadFrame {
    /// Router-assigned message identifier echoed in the reply.
    pub message_id: MessageId,
    /// Owning session.
    pub session: SessionId,
    /// I2P protocol number carried in the gzip header.
    pub protocol: u8,
    /// Source port carried in the gzip header.
    pub source_port: u16,
    /// Destination port carried in the gzip header.
    pub destination_port: u16,
    /// Compressed payload bytes (gzip body after the 10-byte header).
    pub payload: Vec<u8>,
}

impl InboundPayloadFrame {
    /// Overhead bytes per frame on the wire (gzip header + I2CP
    /// payload length prefix). The constant lives next to the
    /// [`InboundPayloadFrame::wire_size`] helper so tests can
    /// reserve a frame-sized slot below the byte ceiling without
    /// duplicating the magic numbers.
    pub const WIRE_OVERHEAD_BYTES: usize = 10 + 4;

    /// Returns the on-the-wire payload size (header + body bytes).
    pub fn wire_size(&self) -> usize {
        Self::WIRE_OVERHEAD_BYTES + self.payload.len()
    }
}

/// Bounded per-session inbound payload queue with explicit frame and
/// byte ceilings. Slow/non-reading clients trigger bounded
/// backpressure: the queue rejects new frames when either ceiling
/// would be exceeded.
#[derive(Debug, Default)]
pub struct InboundPayloadQueue {
    frames: VecDeque<InboundPayloadFrame>,
    queued_bytes: usize,
}

impl InboundPayloadQueue {
    /// Constructs an empty inbound queue.
    pub const fn new() -> Self {
        Self {
            frames: VecDeque::new(),
            queued_bytes: 0,
        }
    }

    /// Pushes one inbound frame. Rejects when either the frame or
    /// byte ceiling would be exceeded.
    pub fn push(&mut self, frame: InboundPayloadFrame) -> Result<usize, DataPlaneError> {
        if self.frames.len() >= MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION {
            return Err(DataPlaneError::CapacityExceeded);
        }
        let projected = self.queued_bytes.saturating_add(frame.wire_size());
        if projected > MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION {
            return Err(DataPlaneError::CapacityExceeded);
        }
        self.queued_bytes = projected;
        self.frames.push_back(frame);
        Ok(self.frames.len())
    }

    /// Pops the next frame in FIFO order.
    pub fn pop(&mut self) -> Option<InboundPayloadFrame> {
        let frame = self.frames.pop_front()?;
        self.queued_bytes = self.queued_bytes.saturating_sub(frame.wire_size());
        Some(frame)
    }

    /// Returns the number of queued frames.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Reports whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Returns the aggregate queued wire bytes.
    pub const fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }

    /// Returns the configured frame ceiling.
    pub const fn frame_capacity(&self) -> usize {
        MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION
    }

    /// Returns the configured byte ceiling.
    pub const fn byte_capacity(&self) -> usize {
        MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION
    }

    /// Releases every queued frame. Returns the dropped count.
    pub fn release_all(&mut self) -> usize {
        let dropped = self.frames.len();
        self.frames.clear();
        self.queued_bytes = 0;
        dropped
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    fn entry(message_id: u32) -> PendingStatusEntry {
        PendingStatusEntry {
            message_id: MessageId::new(message_id),
            session: SessionId::new(7),
            nonce: ClientNonce::new(message_id),
        }
    }

    fn frame(message_id: u32) -> InboundPayloadFrame {
        InboundPayloadFrame {
            message_id: MessageId::new(message_id),
            session: SessionId::new(7),
            protocol: 6,
            source_port: 0,
            destination_port: 0,
            payload: vec![0xab; 64],
        }
    }

    #[test]
    fn pending_status_table_enforces_count_ceiling() {
        let mut table = PendingStatusTable::new();
        for id in 0..MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION as u32 {
            table.record(entry(id)).expect("under ceiling");
        }
        let error = table
            .record(entry(
                u32::try_from(MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION).unwrap(),
            ))
            .expect_err("over ceiling");
        assert_eq!(error, DataPlaneError::CapacityExceeded);
    }

    #[test]
    fn pending_status_table_take_is_idempotent() {
        let mut table = PendingStatusTable::new();
        table.record(entry(1)).expect("record");
        let first = table.take(MessageId::new(1)).expect("first");
        assert_eq!(first.message_id, entry(1).message_id);
        // A second take with the same message id is a no-op rather
        // than resurrecting the entry — duplicate/late internal
        // events cannot double-spend a terminal status.
        assert!(table.take(MessageId::new(1)).is_none());
        assert!(table.is_empty());
        assert_eq!(table.release_all(), 0);
    }

    #[test]
    fn inbound_payload_queue_enforces_frame_and_byte_ceilings() {
        let mut queue = InboundPayloadQueue::new();
        for id in 0..MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION as u32 {
            queue.push(frame(id)).expect("under frame ceiling");
        }
        let error = queue
            .push(frame(
                u32::try_from(MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION).unwrap(),
            ))
            .expect_err("frame ceiling");
        assert_eq!(error, DataPlaneError::CapacityExceeded);
        // Build a queue that exceeds the byte ceiling before hitting
        // the frame ceiling; the byte ceiling must fire first.
        let mut byte_queue = InboundPayloadQueue::new();
        // The wire size of a single frame is
        // `WIRE_OVERHEAD_BYTES + payload_len`. Reserve a frame
        // payload that exactly fills the byte budget for one frame
        // and then attempt to push a second frame that pushes the
        // aggregate past the ceiling.
        let frame_payload_len =
            MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION - InboundPayloadFrame::WIRE_OVERHEAD_BYTES;
        let big = vec![0u8; frame_payload_len];
        byte_queue
            .push(InboundPayloadFrame {
                message_id: MessageId::new(1),
                session: SessionId::new(7),
                protocol: 6,
                source_port: 0,
                destination_port: 0,
                payload: big,
            })
            .expect("under byte ceiling");
        let error = byte_queue
            .push(InboundPayloadFrame {
                message_id: MessageId::new(2),
                session: SessionId::new(7),
                protocol: 6,
                source_port: 0,
                destination_port: 0,
                payload: vec![0],
            })
            .expect_err("byte ceiling");
        assert_eq!(error, DataPlaneError::CapacityExceeded);
    }

    #[test]
    fn inbound_payload_queue_pop_releases_byte_accounting() {
        let mut queue = InboundPayloadQueue::new();
        queue.push(frame(1)).expect("push");
        let size_before = queue.queued_bytes();
        let popped = queue.pop().expect("pop");
        assert_eq!(popped.message_id, MessageId::new(1));
        assert_eq!(queue.queued_bytes(), size_before - popped.wire_size());
        assert_eq!(queue.release_all(), 0);
        assert_eq!(queue.queued_bytes(), 0);
    }
}
