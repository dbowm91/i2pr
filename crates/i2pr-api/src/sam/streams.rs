//! Runtime-neutral SAM 3.1 STREAM socket ownership surface.
//!
//! Plan 138 owns the bounded per-session stream-socket registry that the
//! daemon's loopback listener composes into a TCP STREAM bridge. The
//! module is runtime-neutral: every state machine runs behind a single
//! [`std::sync::Mutex`], the registry never holds secret material, and
//! the data structures compose with the rest of `i2pr-api` without
//! touching Tokio primitives.
//!
//! ## Layout
//!
//! ```text
//! SamStreamRegistry
//!   sessions: HashMap<SamSessionId, SamStreamEntry>
//!
//! SamStreamEntry
//!   next_stream_id: StreamAcceptId           // monotonic per session
//!   attachments:   HashMap<StreamAcceptId, SamStreamAttachment>
//!   pending_accepts: VecDeque<StreamAcceptId> // FIFO ACCEPT waiters
//! ```
//!
//! The registry enforces:
//!
//! - per-session STREAM socket ceiling
//!   ([`SamLimits::max_stream_sockets_per_session`]);
//! - per-session pending ACCEPT ceiling
//!   ([`SamLimits::max_pending_accepts_per_session`]);
//! - exact-once stream-id allocation (a freed slot is never reissued
//!   inside the same session because the SAM wire ID is opaque to the
//!   application and a reissue would risk collision with a lingering
//!   reference);
//! - transactional reservation: an attachment or pending-accept slot
//!   is either fully committed (id visible in `attachments` /
//!   `pending_accepts`) or fully released (no observable change).
//!
//! ## Idempotency
//!
//! `release_attachment` and `release_pending_accept` are idempotent so
//! the daemon's per-stream cleanup path can call them without tracking
//! whether the prior cleanup completed.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use i2pr_client::DestinationId;

use crate::sam::limits::SamLimits;
use crate::sam::session::SamSessionId;

use super::command::StreamAcceptId;

/// State of one STREAM socket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamStreamState {
    /// The socket issued a `STREAM CONNECT` and is waiting for the
    /// underlying Streaming connection to reach `Established`.
    Connecting,
    /// The socket issued a `STREAM ACCEPT` and is registered as a
    /// pending ACCEPT waiter.
    WaitingAccept,
    /// The underlying Streaming connection is established and the
    /// socket has transitioned permanently to raw byte mode.
    Established,
    /// The socket initiated a graceful close (`send_close`).
    Closing,
    /// The socket has released every resource. Terminal.
    Closed,
}

/// Direction of the underlying Streaming connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamStreamDirection {
    /// `STREAM CONNECT` originated the connection.
    Outbound,
    /// `STREAM ACCEPT` was the local endpoint of the connection.
    Inbound,
}

/// One STREAM socket attachment, owned by [`SamStreamEntry`].
///
/// The attachment stores only the metadata needed for telemetry,
/// duplicate teardown detection, and ACCEPT peer-destination emission.
/// Secret material is never held here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamStreamAttachment {
    stream_id: StreamAcceptId,
    direction: SamStreamDirection,
    state: SamStreamState,
    /// Base64 SAM public destination of the remote endpoint, captured
    /// after authentication for ACCEPT sockets. `None` until the
    /// inbound SYN is accepted; always `Some` once established for
    /// CONNECT sockets (the caller supplied the public destination in
    /// the CONNECT command).
    peer_destination_b64: Option<String>,
    /// Destination identifier for the owning local destination. This
    /// is a non-secret view kept only to disambiguate per-stream
    /// cleanup paths.
    owning_destination: DestinationId,
}

impl SamStreamAttachment {
    /// Returns the opaque SAM stream identifier.
    pub const fn stream_id(&self) -> StreamAcceptId {
        self.stream_id
    }

    /// Returns the underlying Streaming direction.
    pub const fn direction(&self) -> SamStreamDirection {
        self.direction
    }

    /// Returns the current state.
    pub const fn state(&self) -> SamStreamState {
        self.state
    }

    /// Returns the cached peer public destination Base64 text.
    pub fn peer_destination_b64(&self) -> Option<&str> {
        self.peer_destination_b64.as_deref()
    }

    /// Returns the owning destination identifier.
    pub const fn owning_destination(&self) -> DestinationId {
        self.owning_destination
    }

    fn set_state(&mut self, state: SamStreamState) {
        self.state = state;
    }

    fn set_peer_destination(&mut self, peer_destination_b64: String) {
        self.peer_destination_b64 = Some(peer_destination_b64);
    }
}

/// Per-session stream-socket bookkeeping.
#[derive(Debug)]
pub struct SamStreamEntry {
    next_stream_id: AtomicU32,
    attachments: HashMap<StreamAcceptId, SamStreamAttachment>,
    /// FIFO queue of `STREAM ACCEPT` waiters, ordered by registration.
    pending_accepts: VecDeque<StreamAcceptId>,
}

impl SamStreamEntry {
    fn new() -> Self {
        // SAM 3.1 reserves stream-id `0` for the wire's "no stream yet"
        // sentinel used inside the Streaming handshake itself. Skip it
        // to keep the SAM-level identifier space unambiguous.
        Self {
            next_stream_id: AtomicU32::new(1),
            attachments: HashMap::new(),
            pending_accepts: VecDeque::new(),
        }
    }

    fn allocate_stream_id(&self) -> StreamAcceptId {
        let candidate = self.next_stream_id.fetch_add(1, Ordering::AcqRel);
        // Stream IDs are u32; if a long-lived session ever exhausts the
        // 2^32-2 allocation window, fall back to `u32::MAX` rather than
        // wrap and collide with an existing live id. The ceiling is
        // not user-visible: the per-session stream-socket ceiling is
        // far smaller.
        if candidate == 0 {
            self.next_stream_id.store(u32::MAX, Ordering::Release);
            0
        } else {
            candidate
        }
    }
}

/// Outcome of [`SamStreamRegistry::register_outbound`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamOutboundAttachment {
    /// The freshly allocated stream id.
    pub stream_id: StreamAcceptId,
    /// The peer public destination the caller supplied, recorded for
    /// telemetry and diagnostics.
    pub peer_destination_b64: Option<String>,
}

/// Outcome of [`SamStreamRegistry::register_inbound_waiter`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SamAcceptWaiter {
    /// The freshly allocated stream id.
    pub stream_id: StreamAcceptId,
}

/// Typed stream-registry failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SamStreamRegistryError {
    /// The supplied session identifier was not registered.
    UnknownSession {
        /// The rejected session identifier.
        session_id: SamSessionId,
    },
    /// The supplied stream id was not attached to the session.
    UnknownStream {
        /// The rejected stream id.
        stream_id: StreamAcceptId,
    },
    /// The per-session STREAM socket ceiling would be exceeded.
    StreamAttachmentsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The per-session pending ACCEPT ceiling would be exceeded.
    PendingAcceptsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The supplied stream id already exists (id reuse after free is
    /// intentionally forbidden).
    DuplicateStreamId {
        /// The colliding stream id.
        stream_id: StreamAcceptId,
    },
    /// The internal mutex was poisoned.
    Poisoned,
}

impl core::fmt::Display for SamStreamRegistryError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownSession { session_id } => {
                write!(formatter, "unknown sam session id {session_id}")
            }
            Self::UnknownStream { stream_id } => {
                write!(formatter, "unknown sam stream id {stream_id}")
            }
            Self::StreamAttachmentsFull { maximum } => {
                write!(formatter, "per-session stream ceiling {maximum} reached")
            }
            Self::PendingAcceptsFull { maximum } => write!(
                formatter,
                "per-session pending accept ceiling {maximum} reached"
            ),
            Self::DuplicateStreamId { stream_id } => {
                write!(formatter, "duplicate sam stream id {stream_id}")
            }
            Self::Poisoned => formatter.write_str("sam stream registry mutex poisoned"),
        }
    }
}

impl std::error::Error for SamStreamRegistryError {}

impl<T> From<std::sync::PoisonError<T>> for SamStreamRegistryError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::Poisoned
    }
}

/// Bounded runtime-neutral SAM 3.1 stream-socket registry.
#[derive(Debug)]
pub struct SamStreamRegistry {
    limits: SamLimits,
    sessions: Mutex<HashMap<SamSessionId, SamStreamEntry>>,
}

impl SamStreamRegistry {
    /// Constructs an empty registry bound to the supplied limits.
    pub fn new(limits: SamLimits) -> Self {
        Self {
            limits,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the configured limits.
    pub const fn limits(&self) -> SamLimits {
        self.limits
    }

    /// Registers a brand-new SAM session with the stream registry.
    /// Idempotent: an existing entry is left untouched.
    pub fn register_session(&self, session_id: SamSessionId) -> Result<(), SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        sessions
            .entry(session_id)
            .or_insert_with(SamStreamEntry::new);
        Ok(())
    }

    /// Removes every stream attachment and pending accept entry
    /// attached to the session. Idempotent.
    pub fn unregister_session(
        &self,
        session_id: &SamSessionId,
    ) -> Result<(), SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        sessions.remove(session_id);
        Ok(())
    }

    /// Returns the number of registered SAM sessions that own at
    /// least one stream attachment or pending accept. Intended for
    /// tests and diagnostics.
    pub fn active_session_count(&self) -> usize {
        self.sessions
            .lock()
            .map(|sessions| {
                sessions
                    .values()
                    .filter(|entry| {
                        !entry.attachments.is_empty() || !entry.pending_accepts.is_empty()
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Returns the number of registered STREAM attachments across
    /// every session.
    pub fn attachment_count(&self) -> usize {
        self.sessions
            .lock()
            .map(|sessions| sessions.values().map(|e| e.attachments.len()).sum())
            .unwrap_or(0)
    }

    /// Returns the number of pending ACCEPT waiters across every
    /// session.
    pub fn pending_accept_count(&self) -> usize {
        self.sessions
            .lock()
            .map(|sessions| sessions.values().map(|e| e.pending_accepts.len()).sum())
            .unwrap_or(0)
    }

    /// Registers a new outbound STREAM attachment for the given
    /// session. Returns the freshly allocated stream id, or fails with
    /// `StreamAttachmentsFull` when the per-session ceiling is reached.
    pub fn register_outbound(
        &self,
        session_id: &SamSessionId,
        owning_destination: DestinationId,
        peer_destination_b64: Option<String>,
    ) -> Result<SamOutboundAttachment, SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get_mut(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        if entry.attachments.len() >= usize::from(self.limits.max_stream_sockets_per_session) {
            return Err(SamStreamRegistryError::StreamAttachmentsFull {
                maximum: self.limits.max_stream_sockets_per_session,
            });
        }
        let stream_id = entry.allocate_stream_id();
        let attachment = SamStreamAttachment {
            stream_id,
            direction: SamStreamDirection::Outbound,
            state: SamStreamState::Connecting,
            peer_destination_b64,
            owning_destination,
        };
        entry.attachments.insert(stream_id, attachment);
        Ok(SamOutboundAttachment {
            stream_id,
            peer_destination_b64: None,
        })
    }

    /// Registers a new inbound STREAM ACCEPT waiter. Returns the
    /// allocated stream id and enqueues the waiter's id in FIFO order.
    pub fn register_inbound_waiter(
        &self,
        session_id: &SamSessionId,
        owning_destination: DestinationId,
    ) -> Result<SamAcceptWaiter, SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get_mut(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        if entry.pending_accepts.len() >= usize::from(self.limits.max_pending_accepts_per_session) {
            return Err(SamStreamRegistryError::PendingAcceptsFull {
                maximum: self.limits.max_pending_accepts_per_session,
            });
        }
        if entry.attachments.len() >= usize::from(self.limits.max_stream_sockets_per_session) {
            return Err(SamStreamRegistryError::StreamAttachmentsFull {
                maximum: self.limits.max_stream_sockets_per_session,
            });
        }
        let stream_id = entry.allocate_stream_id();
        let attachment = SamStreamAttachment {
            stream_id,
            direction: SamStreamDirection::Inbound,
            state: SamStreamState::WaitingAccept,
            peer_destination_b64: None,
            owning_destination,
        };
        entry.attachments.insert(stream_id, attachment);
        entry.pending_accepts.push_back(stream_id);
        Ok(SamAcceptWaiter { stream_id })
    }

    /// Updates the state of a registered attachment. Used by the
    /// daemon's per-stream task as the Streaming handshake progresses.
    pub fn update_state(
        &self,
        session_id: &SamSessionId,
        stream_id: StreamAcceptId,
        state: SamStreamState,
    ) -> Result<(), SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get_mut(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        let attachment = entry
            .attachments
            .get_mut(&stream_id)
            .ok_or(SamStreamRegistryError::UnknownStream { stream_id })?;
        attachment.set_state(state);
        Ok(())
    }

    /// Records the authenticated peer public destination (SAM Base64
    /// text) for an attachment. The text is the value the daemon will
    /// emit on the STREAM ACCEPT pre-raw line in non-SILENT mode.
    pub fn set_peer_destination(
        &self,
        session_id: &SamSessionId,
        stream_id: StreamAcceptId,
        peer_destination_b64: String,
    ) -> Result<(), SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get_mut(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        let attachment = entry
            .attachments
            .get_mut(&stream_id)
            .ok_or(SamStreamRegistryError::UnknownStream { stream_id })?;
        attachment.set_peer_destination(peer_destination_b64);
        Ok(())
    }

    /// Pops the next pending ACCEPT waiter in FIFO order and returns
    /// the stream id assigned to it. Returns `Ok(None)` when no waiter
    /// is currently registered.
    pub fn pop_pending_accept(
        &self,
        session_id: &SamSessionId,
    ) -> Result<Option<StreamAcceptId>, SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get_mut(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        Ok(entry.pending_accepts.pop_front())
    }

    /// Returns a snapshot clone of the attachment metadata for the
    /// supplied stream id.
    pub fn attachment(
        &self,
        session_id: &SamSessionId,
        stream_id: StreamAcceptId,
    ) -> Result<Option<SamStreamAttachment>, SamStreamRegistryError> {
        let sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        Ok(entry.attachments.get(&stream_id).cloned())
    }

    /// Removes an attachment and returns it for telemetry or final
    /// accounting. Idempotent.
    pub fn release_attachment(
        &self,
        session_id: &SamSessionId,
        stream_id: StreamAcceptId,
    ) -> Result<Option<SamStreamAttachment>, SamStreamRegistryError> {
        let mut sessions = self.sessions.lock()?;
        let entry =
            sessions
                .get_mut(session_id)
                .ok_or_else(|| SamStreamRegistryError::UnknownSession {
                    session_id: session_id.clone(),
                })?;
        let removed = entry.attachments.remove(&stream_id);
        if removed.is_some() {
            // The id may have been queued as a pending accept; remove it
            // idempotently from the FIFO queue as well.
            entry.pending_accepts.retain(|id| *id != stream_id);
        }
        Ok(removed)
    }
}

/// Shared, cheaply-cloneable handle to a [`SamStreamRegistry`].
#[derive(Clone, Debug)]
pub struct SamStreamRegistryHandle {
    inner: Arc<SamStreamRegistry>,
}

impl SamStreamRegistryHandle {
    /// Wraps an existing [`SamStreamRegistry`] in a cloneable handle.
    pub fn new(registry: Arc<SamStreamRegistry>) -> Self {
        Self { inner: registry }
    }

    /// Returns the underlying registry.
    pub fn inner(&self) -> &Arc<SamStreamRegistry> {
        &self.inner
    }
}

impl std::ops::Deref for SamStreamRegistryHandle {
    type Target = SamStreamRegistry;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sam::limits::SamLimits;

    fn destination(seed: u8) -> DestinationId {
        let mut bytes = [0_u8; 32];
        bytes[0] = seed;
        DestinationId::from_hash(i2pr_proto::Hash::from_bytes(bytes))
    }

    #[test]
    fn register_outbound_respects_per_session_ceiling() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        registry.register_session(session.clone()).unwrap();
        let ceiling = SamLimits::defaults().max_stream_sockets_per_session;
        for _ in 0..ceiling {
            registry
                .register_outbound(&session, destination(1), None)
                .unwrap();
        }
        let error = registry
            .register_outbound(&session, destination(1), None)
            .unwrap_err();
        assert!(matches!(
            error,
            SamStreamRegistryError::StreamAttachmentsFull { .. }
        ));
    }

    #[test]
    fn register_inbound_waiter_is_fifo() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        registry.register_session(session.clone()).unwrap();
        let first = registry
            .register_inbound_waiter(&session, destination(1))
            .unwrap();
        let second = registry
            .register_inbound_waiter(&session, destination(1))
            .unwrap();
        assert_eq!(
            registry.pop_pending_accept(&session).unwrap(),
            Some(first.stream_id)
        );
        assert_eq!(
            registry.pop_pending_accept(&session).unwrap(),
            Some(second.stream_id)
        );
        assert_eq!(registry.pop_pending_accept(&session).unwrap(), None);
    }

    #[test]
    fn release_attachment_is_idempotent() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        registry.register_session(session.clone()).unwrap();
        let outbound = registry
            .register_outbound(&session, destination(1), None)
            .unwrap();
        let removed = registry
            .release_attachment(&session, outbound.stream_id)
            .unwrap();
        assert!(removed.is_some());
        assert!(
            registry
                .release_attachment(&session, outbound.stream_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn unregister_session_releases_every_attachment() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        registry.register_session(session.clone()).unwrap();
        registry
            .register_outbound(&session, destination(1), None)
            .unwrap();
        registry
            .register_inbound_waiter(&session, destination(1))
            .unwrap();
        registry.unregister_session(&session).unwrap();
        assert_eq!(registry.attachment_count(), 0);
        assert_eq!(registry.pending_accept_count(), 0);
        assert_eq!(registry.active_session_count(), 0);
    }

    #[test]
    fn unknown_session_is_rejected() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        let error = registry
            .register_outbound(&session, destination(1), None)
            .unwrap_err();
        assert!(matches!(
            error,
            SamStreamRegistryError::UnknownSession { .. }
        ));
    }

    #[test]
    fn update_state_records_lifecycle() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        registry.register_session(session.clone()).unwrap();
        let outbound = registry
            .register_outbound(&session, destination(1), None)
            .unwrap();
        registry
            .update_state(&session, outbound.stream_id, SamStreamState::Established)
            .unwrap();
        let snapshot = registry
            .attachment(&session, outbound.stream_id)
            .unwrap()
            .expect("attachment");
        assert_eq!(snapshot.state(), SamStreamState::Established);
    }

    #[test]
    fn set_peer_destination_is_observable() {
        let registry = SamStreamRegistry::new(SamLimits::defaults());
        let session = SamSessionId::new("alpha").unwrap();
        registry.register_session(session.clone()).unwrap();
        let inbound = registry
            .register_inbound_waiter(&session, destination(1))
            .unwrap();
        registry
            .set_peer_destination(&session, inbound.stream_id, "PUB-XYZ".to_owned())
            .unwrap();
        let snapshot = registry
            .attachment(&session, inbound.stream_id)
            .unwrap()
            .expect("attachment");
        assert_eq!(snapshot.peer_destination_b64(), Some("PUB-XYZ"));
    }
}
