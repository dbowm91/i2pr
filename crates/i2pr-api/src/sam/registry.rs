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
use crate::sam::session_create::SessionCreateStyle;

use super::MAX_SAM_SESSION_ID_BYTES;

/// Hard per-PRIMARY child-session ceiling.
pub const MAX_SAM_SUBSESSIONS_PER_PRIMARY: usize = 64;

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
    from_port: u16,
    to_port: u16,
    listen_port: u16,
    host_port: Option<u16>,
    host_address: Option<std::net::IpAddr>,
    raw_header: bool,
    protocol: u8,
    listen_protocol: u8,
    style: SessionCreateStyle,
    parent_session_id: Option<SamSessionId>,
}

/// Runtime-neutral configuration for one PRIMARY child entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamSubsessionConfig {
    /// Child style.
    pub style: SessionCreateStyle,
    /// Local I2P source port.
    pub from_port: u16,
    /// Remote I2P destination port.
    pub to_port: u16,
    /// Local I2P listener port for inbound traffic.
    pub listen_port: u16,
    /// Optional ordinary-SAM loopback UDP port.
    pub host_port: Option<u16>,
    /// Optional ordinary-SAM loopback UDP address.
    pub host_address: Option<std::net::IpAddr>,
    /// Whether RAW receive framing includes the I2P header.
    pub raw_header: bool,
    /// Outbound I2CP protocol for RAW.
    pub protocol: u8,
    /// Inbound I2CP listen protocol for RAW.
    pub listen_protocol: u8,
}

struct SamSessionEntryConfig {
    from_port: u16,
    to_port: u16,
    listen_port: u16,
    style: SessionCreateStyle,
    parent_session_id: Option<SamSessionId>,
}

impl SamSessionEntry {
    /// Constructs a fresh entry from validated caller-supplied
    /// fields.
    pub fn new(
        session_id: SamSessionId,
        destination_id: DestinationId,
        public_destination_b64: String,
    ) -> Self {
        Self::new_configured(
            session_id,
            destination_id,
            public_destination_b64,
            SamSessionEntryConfig {
                from_port: 0,
                to_port: 0,
                listen_port: 0,
                style: SessionCreateStyle::Stream,
                parent_session_id: None,
            },
        )
    }

    /// Constructs a fresh entry with inherited I2P port defaults.
    pub fn new_with_ports(
        session_id: SamSessionId,
        destination_id: DestinationId,
        public_destination_b64: String,
        from_port: u16,
        to_port: u16,
    ) -> Self {
        Self::new_configured(
            session_id,
            destination_id,
            public_destination_b64,
            SamSessionEntryConfig {
                from_port,
                to_port,
                listen_port: to_port,
                style: SessionCreateStyle::Stream,
                parent_session_id: None,
            },
        )
    }

    fn new_configured(
        session_id: SamSessionId,
        destination_id: DestinationId,
        public_destination_b64: String,
        config: SamSessionEntryConfig,
    ) -> Self {
        let SamSessionEntryConfig {
            from_port,
            to_port,
            listen_port,
            style,
            parent_session_id,
        } = config;
        Self {
            session_id,
            destination_id,
            public_destination_b64,
            control_owner: Arc::new(ControlOwnerState::new()),
            counters: SamSessionCounters::zero(),
            from_port,
            to_port,
            listen_port,
            host_port: None,
            host_address: None,
            raw_header: false,
            protocol: session_style_protocol(style).unwrap_or(0),
            listen_protocol: session_style_protocol(style).unwrap_or(0),
            style,
            parent_session_id,
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

    /// Returns the default local I2P source port.
    pub const fn from_port(&self) -> u16 {
        self.from_port
    }

    /// Returns the default remote I2P destination port.
    pub const fn to_port(&self) -> u16 {
        self.to_port
    }

    /// Returns the I2P listen port for inbound routing.
    pub const fn listen_port(&self) -> u16 {
        self.listen_port
    }

    /// Optional host UDP bridge port supplied by a loopback SAM client.
    /// Managed-app `sam_datagram` operations never bind or forward to it.
    pub const fn host_port(&self) -> Option<u16> {
        self.host_port
    }

    /// Optional loopback host selected by an ordinary SAM UDP client.
    pub const fn host_address(&self) -> Option<std::net::IpAddr> {
        self.host_address
    }

    /// Whether forwarded RAW datagrams include the SAM 3.2 metadata header.
    pub const fn raw_header(&self) -> bool {
        self.raw_header
    }

    /// Outbound I2CP protocol selected for this session.
    pub const fn protocol(&self) -> u8 {
        self.protocol
    }

    /// Inbound I2CP protocol used to route data to this session.
    pub const fn listen_protocol(&self) -> u8 {
        self.listen_protocol
    }

    /// Returns the negotiated SAM session style.
    pub const fn style(&self) -> SessionCreateStyle {
        self.style
    }

    /// Returns the owning PRIMARY ID for a child session.
    pub fn parent_session_id(&self) -> Option<&SamSessionId> {
        self.parent_session_id.as_ref()
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
    /// The per-PRIMARY child-session ceiling was reached.
    SubsessionsFull {
        /// Accepted child-session ceiling.
        maximum: usize,
    },
    /// A sibling already claims an overlapping inbound I2P protocol/port.
    SubsessionListenConflict {
        /// Conflicting inbound I2P port.
        port: u16,
        /// Conflicting I2P protocol number.
        protocol: u8,
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
            Self::SubsessionsFull { maximum } => {
                write!(
                    formatter,
                    "per-PRIMARY child-session ceiling {maximum} reached"
                )
            }
            Self::SubsessionListenConflict { port, protocol } => write!(
                formatter,
                "PRIMARY already has an inbound child for protocol {protocol} and port {port}"
            ),
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

    /// Returns a bounded deterministic snapshot of live session
    /// identifiers for administrative inspection (Plan 288).
    ///
    /// Identifiers are non-secret operator-chosen labels carrying no
    /// destination key material. The snapshot is sorted, capped at 64
    /// entries, and contains no destinations, sockets, or peer addresses.
    pub fn session_ids(&self) -> Vec<String> {
        const MAX_INSPECTION_SESSION_IDS: usize = 64;
        let mut ids: Vec<String> = self
            .by_session
            .lock()
            .map(|sessions| sessions.keys().map(|id| id.as_str().to_owned()).collect())
            .unwrap_or_default();
        ids.sort();
        ids.truncate(MAX_INSPECTION_SESSION_IDS);
        ids
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
        self.reserve_session_with_ports(session_id, destination_id, 0, 0)
    }

    /// Reserves a session together with inherited I2P port defaults.
    pub fn reserve_session_with_ports(
        &self,
        session_id: SamSessionId,
        destination_id: DestinationId,
        from_port: u16,
        to_port: u16,
    ) -> Result<SamSessionReservation, SamSessionRegistryError> {
        self.reserve_session_with_style(
            session_id,
            destination_id,
            from_port,
            to_port,
            SessionCreateStyle::Stream,
        )
    }

    /// Reserves a standalone or PRIMARY session with its protocol style.
    pub fn reserve_session_with_style(
        &self,
        session_id: SamSessionId,
        destination_id: DestinationId,
        from_port: u16,
        to_port: u16,
        style: SessionCreateStyle,
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
            SamSessionEntry::new_configured(
                session_id.clone(),
                destination_id,
                String::new(),
                SamSessionEntryConfig {
                    from_port,
                    to_port,
                    listen_port: match style {
                        SessionCreateStyle::Primary => 0,
                        SessionCreateStyle::Stream
                        | SessionCreateStyle::Datagram
                        | SessionCreateStyle::Datagram2
                        | SessionCreateStyle::Datagram3
                        | SessionCreateStyle::Raw => from_port,
                    },
                    style,
                    parent_session_id: None,
                },
            ),
        );
        by_dest.insert(destination_id, session_id.clone());
        Ok(SamSessionReservation {
            session_id,
            destination_id,
            generation,
        })
    }

    /// Adds a child entry that shares its PRIMARY's destination and identity.
    pub fn add_subsession(
        &self,
        primary_id: &SamSessionId,
        child_id: SamSessionId,
        config: SamSubsessionConfig,
    ) -> Result<SamSessionEntry, SamSessionRegistryError> {
        let SamSubsessionConfig {
            style,
            from_port,
            to_port,
            listen_port,
            host_port,
            host_address,
            raw_header,
            protocol,
            listen_protocol,
        } = config;
        if child_id.as_str().len() > MAX_SAM_SESSION_ID_BYTES {
            return Err(SamSessionRegistryError::UnknownSession {
                session_id: child_id,
            });
        }
        if !matches!(
            style,
            SessionCreateStyle::Stream
                | SessionCreateStyle::Datagram
                | SessionCreateStyle::Datagram2
                | SessionCreateStyle::Datagram3
                | SessionCreateStyle::Raw
        ) {
            return Err(SamSessionRegistryError::UnknownSession {
                session_id: child_id,
            });
        }
        let mut sessions = self.by_session.lock()?;
        if sessions.len() >= usize::from(self.limits.max_sessions) {
            return Err(SamSessionRegistryError::SessionsFull {
                maximum: self.limits.max_sessions,
            });
        }
        if sessions.contains_key(&child_id) {
            return Err(SamSessionRegistryError::DuplicateSession {
                session_id: child_id,
            });
        }
        let primary =
            sessions
                .get(primary_id)
                .ok_or_else(|| SamSessionRegistryError::UnknownSession {
                    session_id: primary_id.clone(),
                })?;
        if primary.style != SessionCreateStyle::Primary || primary.parent_session_id.is_some() {
            return Err(SamSessionRegistryError::UnknownSession {
                session_id: primary_id.clone(),
            });
        }
        if sessions
            .values()
            .filter(|entry| entry.parent_session_id.as_ref() == Some(primary_id))
            .count()
            >= MAX_SAM_SUBSESSIONS_PER_PRIMARY
        {
            return Err(SamSessionRegistryError::SubsessionsFull {
                maximum: MAX_SAM_SUBSESSIONS_PER_PRIMARY,
            });
        }
        let conflicting = sessions.values().any(|entry| {
            entry.parent_session_id.as_ref() == Some(primary_id)
                && entry.listen_protocol == listen_protocol
                && entry.listen_port == listen_port
        });
        if conflicting {
            return Err(SamSessionRegistryError::SubsessionListenConflict {
                port: listen_port,
                protocol: listen_protocol,
            });
        }
        let mut entry = SamSessionEntry::new_configured(
            child_id.clone(),
            primary.destination_id,
            primary.public_destination_b64.clone(),
            SamSessionEntryConfig {
                from_port,
                to_port,
                listen_port,
                style,
                parent_session_id: Some(primary_id.clone()),
            },
        );
        entry.host_port = host_port;
        entry.host_address = host_address;
        entry.raw_header = raw_header;
        entry.protocol = protocol;
        entry.listen_protocol = listen_protocol;
        sessions.insert(child_id, clone_entry(&entry));
        Ok(entry)
    }

    /// Lists child IDs owned by a PRIMARY in sorted order.
    pub fn subsession_ids(&self, primary_id: &SamSessionId) -> Vec<SamSessionId> {
        let mut ids = self
            .by_session
            .lock()
            .map(|sessions| {
                sessions
                    .values()
                    .filter(|entry| entry.parent_session_id.as_ref() == Some(primary_id))
                    .map(|entry| entry.session_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ids.sort();
        ids
    }

    /// Removes one child only when it belongs to the supplied PRIMARY.
    pub fn remove_subsession(
        &self,
        primary_id: &SamSessionId,
        child_id: &SamSessionId,
    ) -> Result<Option<SamSessionEntry>, SamSessionRegistryError> {
        let mut sessions = self.by_session.lock()?;
        if sessions
            .get(child_id)
            .is_none_or(|entry| entry.parent_session_id.as_ref() != Some(primary_id))
        {
            return Ok(None);
        }
        Ok(sessions.remove(child_id).map(|entry| clone_entry(&entry)))
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
        if entry.parent_session_id.is_none() {
            sessions.retain(|_, child| child.parent_session_id.as_ref() != Some(session_id));
            by_dest.remove(&entry.destination_id);
        }
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

    /// Selects the live datagram subsession for one inbound I2CP protocol
    /// and destination port. Exact listen ports win over a port-zero
    /// wildcard. Children without an ordinary UDP forwarding port remain
    /// available to the private managed-app operation but are skipped here.
    pub fn inbound_datagram_session(
        &self,
        destination_id: DestinationId,
        protocol: u8,
        destination_port: u16,
    ) -> Option<SamSessionEntry> {
        let sessions = self.by_session.lock().ok()?;
        sessions
            .values()
            .filter(|entry| {
                entry.destination_id == destination_id
                    && (entry.listen_protocol == protocol || entry.listen_protocol == 0)
                    && (entry.listen_port == destination_port || entry.listen_port == 0)
                    && entry.host_port.is_some_and(|port| port != 0)
            })
            .min_by_key(|entry| {
                (
                    u8::from(entry.listen_protocol != protocol),
                    u8::from(entry.listen_port != destination_port),
                    entry.session_id.as_str(),
                )
            })
            .map(clone_entry)
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

fn session_style_protocol(style: SessionCreateStyle) -> Option<u8> {
    match style {
        SessionCreateStyle::Stream => Some(i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER),
        SessionCreateStyle::Datagram => Some(i2pr_proto::PROTOCOL_TYPE_DATAGRAM),
        SessionCreateStyle::Raw => Some(i2pr_proto::PROTOCOL_TYPE_RAW),
        SessionCreateStyle::Datagram2 => Some(i2pr_proto::PROTOCOL_TYPE_DATAGRAM2),
        SessionCreateStyle::Datagram3 => Some(i2pr_proto::PROTOCOL_TYPE_DATAGRAM3),
        SessionCreateStyle::Primary => None,
    }
}

fn clone_entry(entry: &SamSessionEntry) -> SamSessionEntry {
    let mut cloned = SamSessionEntry::new_configured(
        entry.session_id.clone(),
        entry.destination_id,
        entry.public_destination_b64.clone(),
        SamSessionEntryConfig {
            from_port: entry.from_port,
            to_port: entry.to_port,
            listen_port: entry.listen_port,
            style: entry.style,
            parent_session_id: entry.parent_session_id.clone(),
        },
    );
    cloned.host_port = entry.host_port;
    cloned.host_address = entry.host_address;
    cloned.raw_header = entry.raw_header;
    cloned.protocol = entry.protocol;
    cloned.listen_protocol = entry.listen_protocol;
    cloned
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
    fn session_port_defaults_survive_commit_and_lookup() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("ports").unwrap();
        let destination_id = destination(77);
        let reservation = registry
            .reserve_session_with_ports(session_id.clone(), destination_id, 25, 110)
            .expect("reserve");
        let entry = registry
            .commit_reservation(&reservation, "PUB".to_owned())
            .expect("commit");
        assert_eq!((entry.from_port(), entry.to_port()), (25, 110));
        let looked_up = registry.get(&session_id).expect("lookup");
        assert_eq!((looked_up.from_port(), looked_up.to_port()), (25, 110));
    }

    #[test]
    fn primary_children_share_destination_and_primary_close_removes_them() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let primary_id = SamSessionId::new("primary").unwrap();
        let destination_id = destination(81);
        let reservation = registry
            .reserve_session_with_style(
                primary_id.clone(),
                destination_id,
                0,
                0,
                SessionCreateStyle::Primary,
            )
            .expect("reserve primary");
        registry
            .commit_reservation(&reservation, "PUB".to_owned())
            .expect("commit primary");
        let child_id = SamSessionId::new("stream-child").unwrap();
        let child = registry
            .add_subsession(
                &primary_id,
                child_id.clone(),
                SamSubsessionConfig {
                    style: SessionCreateStyle::Stream,
                    from_port: 23,
                    to_port: 110,
                    listen_port: 23,
                    host_port: None,
                    host_address: None,
                    raw_header: false,
                    protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
                    listen_protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
                },
            )
            .expect("add child");
        assert_eq!(child.destination_id(), destination_id);
        assert_eq!(child.parent_session_id(), Some(&primary_id));
        assert_eq!((child.from_port(), child.to_port()), (23, 110));
        assert_eq!(child.host_port(), None);
        assert_eq!(
            registry.session_for_destination(&destination_id).unwrap(),
            Some(primary_id.clone())
        );

        let duplicate_listener = registry.add_subsession(
            &primary_id,
            SamSessionId::new("duplicate-listener").unwrap(),
            SamSubsessionConfig {
                style: SessionCreateStyle::Stream,
                from_port: 23,
                to_port: 110,
                listen_port: 23,
                host_port: None,
                host_address: None,
                raw_header: false,
                protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
                listen_protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            },
        );
        assert!(matches!(
            duplicate_listener,
            Err(SamSessionRegistryError::SubsessionListenConflict {
                port: 23,
                protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            })
        ));

        let datagram_child = SamSessionId::new("datagram2-child").unwrap();
        let datagram = registry
            .add_subsession(
                &primary_id,
                datagram_child,
                SamSubsessionConfig {
                    style: SessionCreateStyle::Datagram2,
                    from_port: 55,
                    to_port: 66,
                    listen_port: 55,
                    host_port: Some(7655),
                    host_address: Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
                    raw_header: false,
                    protocol: i2pr_proto::PROTOCOL_TYPE_DATAGRAM2,
                    listen_protocol: i2pr_proto::PROTOCOL_TYPE_DATAGRAM2,
                },
            )
            .expect("add Datagram2 child");
        assert_eq!(datagram.host_port(), Some(7655));
        assert_eq!(datagram.destination_id(), destination_id);

        let raw_id = SamSessionId::new("raw-child").unwrap();
        let raw = registry
            .add_subsession(
                &primary_id,
                raw_id,
                SamSubsessionConfig {
                    style: SessionCreateStyle::Raw,
                    from_port: 7,
                    to_port: 8,
                    listen_port: 9,
                    host_port: Some(7656),
                    host_address: Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
                    raw_header: true,
                    protocol: 42,
                    listen_protocol: 0,
                },
            )
            .expect("add custom RAW child");
        assert_eq!((raw.protocol(), raw.listen_protocol()), (42, 0));
        assert!(raw.raw_header());
        let selected = registry
            .inbound_datagram_session(destination_id, 43, 9)
            .expect("custom raw protocol routes to the child");
        assert_eq!(selected.host_port(), Some(7656));
        assert_eq!(selected.host_address(), raw.host_address());
        let raw_default = registry
            .add_subsession(
                &primary_id,
                SamSessionId::new("raw-default").unwrap(),
                SamSubsessionConfig {
                    style: SessionCreateStyle::Raw,
                    from_port: 0,
                    to_port: 0,
                    listen_port: 0,
                    host_port: Some(7657),
                    host_address: None,
                    raw_header: false,
                    protocol: 18,
                    listen_protocol: 0,
                },
            )
            .expect("add RAW default child");
        let exact = registry
            .inbound_datagram_session(destination_id, i2pr_proto::PROTOCOL_TYPE_DATAGRAM2, 55)
            .expect("specific protocol/port beats RAW default");
        assert_eq!(exact.session_id(), datagram.session_id());
        let fallback = registry
            .inbound_datagram_session(destination_id, 44, 99)
            .expect("RAW default receives unmatched datagram protocols");
        assert_eq!(fallback.session_id(), raw_default.session_id());
        let ambiguous = registry.add_subsession(
            &primary_id,
            SamSessionId::new("ambiguous-raw").unwrap(),
            SamSubsessionConfig {
                style: SessionCreateStyle::Raw,
                from_port: 10,
                to_port: 11,
                listen_port: 9,
                host_port: Some(7657),
                host_address: None,
                raw_header: false,
                protocol: 42,
                listen_protocol: 0,
            },
        );
        assert!(matches!(
            ambiguous,
            Err(SamSessionRegistryError::SubsessionListenConflict { .. })
        ));

        registry
            .remove_by_session(&primary_id)
            .expect("close primary");
        assert!(!registry.contains(&primary_id));
        assert!(!registry.contains(&child_id));
        assert_eq!(
            registry.session_for_destination(&destination_id).unwrap(),
            None
        );
    }

    #[test]
    fn ordinary_stream_session_listens_on_from_port_not_remote_to_port() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        let session_id = SamSessionId::new("mail-stream").unwrap();
        let reservation = registry
            .reserve_session_with_style(
                session_id.clone(),
                destination(82),
                25,
                110,
                SessionCreateStyle::Stream,
            )
            .expect("reserve stream");
        let entry = registry
            .commit_reservation(&reservation, "PUB".to_owned())
            .expect("commit stream");
        assert_eq!(entry.from_port(), 25);
        assert_eq!(entry.to_port(), 110);
        assert_eq!(entry.listen_port(), 25);
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

#[cfg(test)]
mod plan288_tests {
    use super::super::limits::SamLimits;
    use super::*;

    fn destination(seed: u8, salt: u8) -> DestinationId {
        let mut bytes = [0_u8; 32];
        bytes[0] = seed;
        bytes[1] = salt;
        DestinationId::from_hash(i2pr_proto::Hash::from_bytes(bytes))
    }

    #[test]
    fn session_ids_snapshot_is_sorted_and_bounded() {
        let registry = SamSessionRegistry::new(SamLimits::defaults());
        assert!(registry.session_ids().is_empty());
        for (name, seed) in [("gamma", 3_u8), ("alpha", 1_u8), ("beta", 2_u8)] {
            let session = SamSessionId::new(name).expect("session id");
            let reservation = registry
                .reserve_session(session, destination(seed, 0))
                .expect("reserve");
            registry
                .commit_reservation(&reservation, "PUB".to_owned())
                .expect("commit");
        }
        // Sorted deterministic order, no destination material.
        assert_eq!(registry.session_ids(), vec!["alpha", "beta", "gamma"]);
        assert_eq!(registry.session_count(), 3);
    }

    #[test]
    fn session_ids_snapshot_caps_at_sixty_four() {
        let mut limits = SamLimits::defaults();
        limits.max_sessions = 80;
        let registry = SamSessionRegistry::new(SamLimits::validate(limits).expect("limits valid"));
        for n in 0..80_u8 {
            let name = format!("session-{n:03}");
            let session = SamSessionId::new(&name).expect("session id");
            let reservation = registry
                .reserve_session(session, destination(n, 1))
                .expect("reserve");
            registry
                .commit_reservation(&reservation, "PUB".to_owned())
                .expect("commit");
        }
        let ids = registry.session_ids();
        assert_eq!(ids.len(), 64);
        assert_eq!(ids[0], "session-000");
        assert_eq!(ids[63], "session-063");
        assert_eq!(registry.session_count(), 80);
    }
}
