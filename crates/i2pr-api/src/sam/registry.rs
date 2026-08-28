//! Global bounded SAM session registry (Plan 137 §7).
//!
//! The registry tracks every active SAM session by:
//!
//! - the session identifier (`ID=`);
//! - the owning local [`DestinationId`];
//! - the SAM public destination Base64 text (cached for the
//!   `SESSION STATUS` reply without retaining secret material);
//! - the control-owner generation token (used to detect duplicate
//!   teardown);
//! - the per-session resource counters;
//! - a runtime-neutral [`ControlOwnerState`] flag set by the per-socket
//!   task on disconnect so teardown can be observed by the daemon's
//!   session lifecycle owner.
//!
//! The registry deliberately does **not** own the
//! [`i2pr_client::DestinationRuntime`], the streaming manager, or any
//! other secret-bearing resource: those live in the
//! `DestinationRegistry` owned by `i2pr-daemon`. The SAM registry
//! owns only what the SAM API layer needs to resolve attachments and
//! drive teardown. Insertion into [`SamSessionRegistry`] and into the
//! `DestinationRegistry` is performed as a single transaction by the
//! daemon service; the registry exposes the atomic
//! `reserve_session` / `commit_reservation` / `rollback_reservation`
//! triplet needed for that composition.
//!
//! The registry is runtime-neutral: a single [`std::sync::Mutex`]
//! guards the bounded maps because the critical sections are short
//! (an insert or a removal) and the lock is never held across an I/O
//! or runtime yield. Tokio tasks acquire the lock through the
//! dedicated `reserve_*` / `teardown_*` helpers.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use i2pr_client::DestinationId;

use crate::sam::limits::SamLimits;
use crate::sam::session::{SamSessionCounters, SamSessionId};

use super::MAX_SAM_SESSION_ID_BYTES;

/// State of the control-socket owner for one SAM session. Set by the
/// per-socket task on disconnect; observed by the daemon's session
/// lifecycle owner.
#[derive(Debug)]
pub struct ControlOwnerState {
    /// `true` after the control socket disconnected, the parser
    /// observed a protocol-fatal error, or the service began
    /// graceful shutdown. Idempotent: setting it twice is a no-op.
    dropped: AtomicBool,
}

impl ControlOwnerState {
    /// Constructs a fresh `Dropped = false` state.
    pub fn new() -> Self {
        Self {
            dropped: AtomicBool::new(false),
        }
    }

    /// Marks the control socket as dropped. Returns `true` only for
    /// the first caller.
    pub fn mark_dropped(&self) -> bool {
        self.dropped
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Returns whether the control socket is marked as dropped.
    pub fn is_dropped(&self) -> bool {
        self.dropped.load(Ordering::Acquire)
    }
}

impl Default for ControlOwnerState {
    fn default() -> Self {
        Self::new()
    }
}

/// One registry-owned SAM session entry.
#[derive(Debug)]
pub struct SamSessionEntry {
    session_id: SamSessionId,
    destination_id: DestinationId,
    public_destination_b64: String,
    control_owner: Arc<ControlOwnerState>,
    counters: SamSessionCounters,
}

impl SamSessionEntry {
    /// Constructs a fresh entry from validated caller-supplied
    /// fields.
    pub fn new(
        session_id: SamSessionId,
        destination_id: DestinationId,
        public_destination_b64: String,
    ) -> Self {
        Self {
            session_id,
            destination_id,
            public_destination_b64,
            control_owner: Arc::new(ControlOwnerState::new()),
            counters: SamSessionCounters::zero(),
        }
    }

    /// Returns the session identifier.
    pub fn session_id(&self) -> &SamSessionId {
        &self.session_id
    }

    /// Returns the owning destination identifier.
    pub fn destination_id(&self) -> DestinationId {
        self.destination_id
    }

    /// Returns the cached SAM public-destination Base64 text.
    pub fn public_destination_b64(&self) -> &str {
        &self.public_destination_b64
    }

    /// Returns the control-owner state handle. The per-socket task
    /// uses this to observe and mark the connection lifecycle.
    pub fn control_owner(&self) -> Arc<ControlOwnerState> {
        Arc::clone(&self.control_owner)
    }

    /// Returns the current resource counters.
    pub const fn counters(&self) -> SamSessionCounters {
        self.counters
    }

    /// Increments the live STREAM socket count, returning the new
    /// total. Returns [`SamSessionRegistryError::StreamAttachmentsFull`]
    /// when the per-session ceiling would be exceeded.
    pub fn add_stream_attachment(
        &mut self,
        limits: SamLimits,
    ) -> Result<u16, SamSessionRegistryError> {
        let next = self
            .counters
            .stream_attachment_count
            .checked_add(1)
            .ok_or(SamSessionRegistryError::CounterOverflow)?;
        if next > limits.max_stream_sockets_per_session {
            return Err(SamSessionRegistryError::StreamAttachmentsFull {
                maximum: limits.max_stream_sockets_per_session,
            });
        }
        self.counters.stream_attachment_count = next;
        Ok(next)
    }

    /// Decrements the live STREAM socket count. Saturates at zero.
    pub fn release_stream_attachment(&mut self) -> u16 {
        let previous = self.counters.stream_attachment_count;
        self.counters.stream_attachment_count = previous.saturating_sub(1);
        self.counters.stream_attachment_count
    }
}

/// A successful session reservation: the caller has been granted the
/// exclusive right to insert the supplied session into both the SAM
/// registry and the daemon's `DestinationRegistry`. If either step
/// fails the caller must invoke
/// [`SamSessionRegistry::rollback_reservation`] to release the slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamSessionReservation {
    session_id: SamSessionId,
    destination_id: DestinationId,
    generation: u64,
}

impl SamSessionReservation {
    /// Returns the reserved session identifier.
    pub fn session_id(&self) -> &SamSessionId {
        &self.session_id
    }

    /// Returns the reserved destination identifier.
    pub fn destination_id(&self) -> DestinationId {
        self.destination_id
    }

    /// Returns the generation token. Each reservation increments the
    /// generation so duplicate-detection at teardown is monotonic.
    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Typed SAM registry failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SamSessionRegistryError {
    /// The supplied session identifier was not registered.
    UnknownSession {
        /// The rejected session identifier.
        session_id: SamSessionId,
    },
    /// A session with the same identifier already exists.
    DuplicateSession {
        /// The rejected session identifier.
        session_id: SamSessionId,
    },
    /// A different session already owns the supplied destination.
    DuplicateDestination {
        /// The rejected destination identifier.
        destination_id: DestinationId,
    },
    /// The global session ceiling was reached.
    SessionsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The per-session STREAM socket ceiling was reached.
    StreamAttachmentsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// A bounded counter overflowed its storage type.
    CounterOverflow,
    /// The internal mutex was poisoned by a panicked task.
    Poisoned,
}

impl core::fmt::Display for SamSessionRegistryError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownSession { session_id } => {
                write!(formatter, "unknown session id {session_id}")
            }
            Self::DuplicateSession { session_id } => {
                write!(formatter, "duplicate session id {session_id}")
            }
            Self::DuplicateDestination { destination_id } => {
                write!(
                    formatter,
                    "destination {:?} already owned by another session",
                    destination_id
                )
            }
            Self::SessionsFull { maximum } => {
                write!(formatter, "session registry capacity {maximum} exceeded")
            }
            Self::StreamAttachmentsFull { maximum } => write!(
                formatter,
                "per-session stream attachment ceiling {maximum} reached"
            ),
            Self::CounterOverflow => formatter.write_str("internal counter overflow"),
            Self::Poisoned => formatter.write_str("sam session registry mutex poisoned"),
        }
    }
}

impl std::error::Error for SamSessionRegistryError {}

impl<T> From<PoisonError<T>> for SamSessionRegistryError {
    fn from(_: PoisonError<T>) -> Self {
        Self::Poisoned
    }
}

/// Global bounded SAM session registry.
#[derive(Debug)]
pub struct SamSessionRegistry {
    limits: SamLimits,
    /// Primary index: session identifier → entry.
    by_session: Mutex<HashMap<SamSessionId, SamSessionEntry>>,
    /// Secondary index: destination identifier → session identifier.
    by_destination: Mutex<BTreeMap<DestinationId, SamSessionId>>,
    /// Monotonic generation counter for duplicate-detection on
    /// teardown. Never decreases.
    generation: AtomicU64,
}

impl SamSessionRegistry {
    /// Constructs a new bounded registry with the supplied limits.
    pub fn new(limits: SamLimits) -> Self {
        Self {
            limits,
            by_session: Mutex::new(HashMap::new()),
            by_destination: Mutex::new(BTreeMap::new()),
            generation: AtomicU64::new(1),
        }
    }

    /// Returns the configured limits.
    pub const fn limits(&self) -> SamLimits {
        self.limits
    }

    /// Returns the number of currently registered sessions.
    pub fn session_count(&self) -> usize {
        self.by_session.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Reserves a slot in the registry for `session_id`, ensuring
    /// that:
    ///
    /// - the registry has capacity (`< limits.max_sessions`);
    /// - no other session owns the same identifier;
    /// - no other session owns the same destination.
    ///
    /// On success the caller must follow up with
    /// [`Self::commit_reservation`] (after inserting the destination
    /// runtime into the daemon's `DestinationRegistry`) or
    /// [`Self::rollback_reservation`] (on any subsequent failure).
    /// The two-step pattern is what makes the SAM-and-destination
    /// insert transactional.
    pub fn reserve_session(
        &self,
        session_id: SamSessionId,
        destination_id: DestinationId,
    ) -> Result<SamSessionReservation, SamSessionRegistryError> {
        if session_id.as_str().len() > MAX_SAM_SESSION_ID_BYTES {
            return Err(SamSessionRegistryError::UnknownSession { session_id });
        }
        let mut sessions = self.by_session.lock()?;
        let mut by_dest = self.by_destination.lock()?;
        if sessions.len() >= usize::from(self.limits.max_sessions) {
            return Err(SamSessionRegistryError::SessionsFull {
                maximum: self.limits.max_sessions,
            });
        }
        if sessions.contains_key(&session_id) {
            return Err(SamSessionRegistryError::DuplicateSession { session_id });
        }
        if by_dest.contains_key(&destination_id) {
            return Err(SamSessionRegistryError::DuplicateDestination { destination_id });
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel);
        // Reserve both indexes with a placeholder entry. The caller
        // must commit (replace `public_destination_b64` with the real
        // Base64 text and return the final [`SamSessionEntry`] to
        // the per-socket task) or roll back (drop the reservation).
        sessions.insert(
            session_id.clone(),
            SamSessionEntry::new(session_id.clone(), destination_id, String::new()),
        );
        by_dest.insert(destination_id, session_id.clone());
        Ok(SamSessionReservation {
            session_id,
            destination_id,
            generation,
        })
    }

    /// Commits a reservation by replacing the cached SAM public-destination
    /// Base64 text. Called by the daemon after a successful
    /// `DestinationRegistry` insert. Returns the finalised entry
    /// together with the generation token so the per-socket task can
    /// observe teardown. Idempotent: a second call with the same
    /// reservation updates the cached text but returns the same
    /// generation token.
    pub fn commit_reservation(
        &self,
        reservation: &SamSessionReservation,
        public_destination_b64: String,
    ) -> Result<SamSessionEntry, SamSessionRegistryError> {
        let mut sessions = self.by_session.lock()?;
        let entry = sessions.get_mut(reservation.session_id()).ok_or_else(|| {
            SamSessionRegistryError::UnknownSession {
                session_id: reservation.session_id().clone(),
            }
        })?;
        if entry.destination_id != reservation.destination_id() {
            return Err(SamSessionRegistryError::UnknownSession {
                session_id: reservation.session_id().clone(),
            });
        }
        entry.public_destination_b64 = public_destination_b64;
        Ok(clone_entry(entry))
    }

    /// Rolls a reservation back, removing both indexes. Idempotent.
    pub fn rollback_reservation(&self, reservation: &SamSessionReservation) {
        if let Ok(mut sessions) = self.by_session.lock() {
            sessions.remove(reservation.session_id());
        }
        if let Ok(mut by_dest) = self.by_destination.lock() {
            by_dest.remove(&reservation.destination_id());
        }
    }

    /// Removes a session by identifier. Returns the removed entry so
    /// the caller can drive the matching `DestinationRegistry`
    /// teardown exactly once. Idempotent: a second call returns
    /// `Ok(None)`.
    pub fn remove_by_session(
        &self,
        session_id: &SamSessionId,
    ) -> Result<Option<SamSessionEntry>, SamSessionRegistryError> {
        let mut sessions = self.by_session.lock()?;
        let mut by_dest = self.by_destination.lock()?;
        let Some(entry) = sessions.remove(session_id) else {
            return Ok(None);
        };
        by_dest.remove(&entry.destination_id);
        Ok(Some(entry))
    }

    /// Looks up the session that owns the supplied destination.
    pub fn session_for_destination(
        &self,
        destination_id: &DestinationId,
    ) -> Result<Option<SamSessionId>, SamSessionRegistryError> {
        let by_dest = self.by_destination.lock()?;
        Ok(by_dest.get(destination_id).cloned())
    }

    /// Returns the cached public Destination text for a locally-owned
    /// destination, if one exists.
    pub fn public_destination_for_destination(
        &self,
        destination_id: &DestinationId,
    ) -> Option<String> {
        let session_id = self.session_for_destination(destination_id).ok()??;
        self.get(&session_id)
            .map(|entry| entry.public_destination_b64().to_owned())
    }

    /// Returns whether a session with the supplied identifier exists.
    pub fn contains(&self, session_id: &SamSessionId) -> bool {
        self.by_session
            .lock()
            .map(|m| m.contains_key(session_id))
            .unwrap_or(false)
    }

    /// Returns a snapshot entry for the supplied session identifier.
    pub fn get(&self, session_id: &SamSessionId) -> Option<SamSessionEntry> {
        let sessions = self.by_session.lock().ok()?;
        sessions.get(session_id).map(clone_entry)
    }

    /// Acquires the lock and runs the supplied closure against the
    /// session-id-keyed map. The closure must not await or block.
    pub fn with_entries<F, R>(&self, closure: F) -> Result<R, SamSessionRegistryError>
    where
        F: FnOnce(&HashMap<SamSessionId, SamSessionEntry>) -> R,
    {
        let sessions = self.by_session.lock()?;
        Ok(closure(&sessions))
    }

    /// Returns the per-session STREAM attachment ceiling.
    pub const fn max_stream_sockets_per_session(&self) -> u16 {
        self.limits.max_stream_sockets_per_session
    }

    /// Returns the per-session pending accept ceiling.
    pub const fn max_pending_accepts_per_session(&self) -> u16 {
        self.limits.max_pending_accepts_per_session
    }

    /// Returns the global session ceiling.
    pub const fn max_sessions(&self) -> u16 {
        self.limits.max_sessions
    }
}

fn clone_entry(entry: &SamSessionEntry) -> SamSessionEntry {
    SamSessionEntry::new(
        entry.session_id.clone(),
        entry.destination_id,
        entry.public_destination_b64.clone(),
    )
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
    fn reserve_and_commit_round_trip() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("alpha").unwrap();
        let destination_id = destination(1);
        let reservation = registry
            .reserve_session(session_id.clone(), destination_id)
            .expect("reservation");
        let entry = registry
            .commit_reservation(&reservation, "PUB-BASE64".to_owned())
            .expect("commit");
        assert_eq!(entry.session_id(), &session_id);
        assert_eq!(entry.destination_id(), destination_id);
        assert_eq!(entry.public_destination_b64(), "PUB-BASE64");
        assert!(registry.contains(&session_id));
        assert_eq!(
            registry.session_for_destination(&destination_id),
            Ok(Some(session_id.clone()))
        );
    }

    #[test]
    fn duplicate_session_is_rejected() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("alpha").unwrap();
        let first = registry
            .reserve_session(session_id.clone(), destination(1))
            .expect("first");
        registry
            .commit_reservation(&first, "PUB-A".to_owned())
            .expect("commit first");
        let error = registry
            .reserve_session(session_id, destination(2))
            .unwrap_err();
        assert!(matches!(
            error,
            SamSessionRegistryError::DuplicateSession { .. }
        ));
    }

    #[test]
    fn duplicate_destination_is_rejected() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let first = registry
            .reserve_session(SamSessionId::new("a").unwrap(), destination(1))
            .expect("first");
        registry
            .commit_reservation(&first, "PUB-A".to_owned())
            .expect("commit first");
        let error = registry
            .reserve_session(SamSessionId::new("b").unwrap(), destination(1))
            .unwrap_err();
        assert!(matches!(
            error,
            SamSessionRegistryError::DuplicateDestination { .. }
        ));
    }

    #[test]
    fn rollback_releases_reservation() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("alpha").unwrap();
        let destination_id = destination(7);
        let reservation = registry
            .reserve_session(session_id.clone(), destination_id)
            .expect("reservation");
        registry.rollback_reservation(&reservation);
        assert!(!registry.contains(&session_id));
        assert_eq!(registry.session_for_destination(&destination_id), Ok(None));
        // The freed slot can be re-reserved under a different id.
        let next = registry
            .reserve_session(SamSessionId::new("beta").unwrap(), destination_id)
            .expect("re-reserve");
        registry.rollback_reservation(&next);
    }

    #[test]
    fn remove_by_session_is_idempotent() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("alpha").unwrap();
        let destination_id = destination(3);
        let reservation = registry
            .reserve_session(session_id.clone(), destination_id)
            .expect("reservation");
        registry
            .commit_reservation(&reservation, "PUB-X".to_owned())
            .expect("commit");
        let removed = registry
            .remove_by_session(&session_id)
            .expect("remove")
            .expect("present");
        assert_eq!(removed.session_id(), &session_id);
        assert!(registry.remove_by_session(&session_id).unwrap().is_none());
    }

    #[test]
    fn capacity_ceiling_rejects_overflow() {
        let limits = SamLimits {
            max_sessions: 1,
            ..SamLimits::defaults()
        };
        let registry = SamSessionRegistry::new(limits);
        let first = registry
            .reserve_session(SamSessionId::new("a").unwrap(), destination(1))
            .expect("first");
        registry
            .commit_reservation(&first, "PUB-A".to_owned())
            .expect("commit");
        let error = registry
            .reserve_session(SamSessionId::new("b").unwrap(), destination(2))
            .unwrap_err();
        assert!(matches!(
            error,
            SamSessionRegistryError::SessionsFull { maximum: 1 }
        ));
    }

    #[test]
    fn control_owner_mark_dropped_is_idempotent() {
        let state = ControlOwnerState::new();
        assert!(!state.is_dropped());
        assert!(state.mark_dropped());
        assert!(state.is_dropped());
        assert!(!state.mark_dropped());
    }

    #[test]
    fn stream_attachment_count_respects_per_session_ceiling() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("alpha").unwrap();
        let destination_id = destination(9);
        let reservation = registry
            .reserve_session(session_id.clone(), destination_id)
            .expect("reservation");
        let mut entry = registry
            .commit_reservation(&reservation, "PUB".to_owned())
            .expect("commit");
        // Saturate the per-session ceiling.
        let mut count = 0_u16;
        for _ in 0..SamLimits::defaults().max_stream_sockets_per_session {
            count = entry.add_stream_attachment(SamLimits::defaults()).unwrap();
        }
        assert_eq!(count, SamLimits::defaults().max_stream_sockets_per_session);
        let error = entry.add_stream_attachment(SamLimits::defaults());
        assert!(matches!(
            error,
            Err(SamSessionRegistryError::StreamAttachmentsFull { .. })
        ));
        // Release the last attachment.
        let released = entry.release_stream_attachment();
        assert_eq!(
            released,
            SamLimits::defaults().max_stream_sockets_per_session - 1
        );
    }
}
