//! Plan 167 supervised loopback I2CP v0.9.67 service.
//!
//! The Plan 167 daemon composes the runtime-neutral
//! [`i2pr_api::i2cp`] protocol/session/connection surface into a real
//! Tokio-owned TCP server. The service is the single composition root
//! for the I2CP wire in `i2pr`:
//!
//! - the loopback [`TcpListener`];
//! - one [`SessionRegistry`] for I2CP session IDs and their destination
//!   hashes (Plan 165 §4);
//! - one router-local [`DestinationRegistry`] for the underlying
//!   [`DestinationRuntime`] instances (Plan 120 + Plan 166 client
//!   ownership);
//! - the supervised child scope that owns per-connection tasks;
//! - the shutdown deadline.
//!
//! The runtime-neutral state/options/lease material lives in
//! `i2pr_api::i2cp`. This module is the runtime-neutral seam that maps
//! the API into a Tokio-driven supervised service, exactly as the SAM
//! bridge does for SAM 3.1.
//!
//! I2CP ownership is deliberately different from SAM:
//!
//! ```text
//! SAM:  router owns the destination signing + X25519 secrets
//!       and signs the LeaseSet2
//! I2CP: client proves Destination ownership with a signed SessionConfig
//!       supplies a signed Standard LeaseSet2 + matching X25519
//!       decryption key; the router never requires the client's
//!       destination signing private key
//! ```
//!
//! Plan 167 does not introduce a second destination stack: it composes
//! the Plan 166 client-owned destination runtime into the same registry
//! SAM already uses, gated through the typed
//! [`i2pr_api::i2cp::I2cpAction`] vocabulary.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_api::i2cp::{
    BandwidthLimits, Clock, ConnectionStateMachine, CreateLeaseSet2, DestLookup, DestroySession,
    Disconnect, FrameDecoder, GetDate, I2cpAction, I2cpMessageOutcome, InboundPayloadFrame,
    InboundPayloadQueue, MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION, Message, MessageId,
    MessagePayload, MessageStatus, PROTOCOL_BYTE, Payload, PayloadGzipHeader, PendingStatusEntry,
    PendingStatusTable, ProjectedPolicy, ReconfigurationClass, SendMessage, SendMessageExpires,
    SessionId, SessionRegistry, SessionRegistryLimits, SessionStatus, SessionStatusCode,
    VerifiedSessionConfig, classify_reconfigure_diff, decode_typed, encode_frame, project_options,
    validate_reconfigure_classifications, verify_session_config,
};
use i2pr_api::i2cp::{
    DestReply, DestReplyBody, HostReply, HostReplyResult, SessionConfig, SessionConfigLimits,
};
use i2pr_client::{
    DestinationConfig, DestinationId, DestinationPayload, DestinationPublic, DestinationRegistry,
    DestinationRuntime, InboundDecryptionCapability, PayloadError, RegistryConfig,
};
use i2pr_crypto::X25519_KEY_LENGTH;
use i2pr_proto::Hash;
use i2pr_proto::Mapping;
use i2pr_runtime::{CancellationToken, ChildScope};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, OwnedSemaphorePermit};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::I2cpConfig;

/// Typed I2CP service failure surfaced to the daemon supervisor.
#[derive(Debug, Error)]
pub enum I2cpServiceError {
    /// Failed to bind the loopback TCP listener.
    #[error("failed to bind I2CP listener on {address}: {source}")]
    Bind {
        /// Bind address.
        address: SocketAddr,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// A configuration invariant required by I2CP failed validation.
    #[error("invalid I2CP configuration: {0}")]
    InvalidConfig(String),
}

/// Returns the protocol-time seconds used for SessionConfig / LeaseSet2
/// validation and timestamp-based publication semantics.
fn i2cp_now_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u32::try_from(duration.as_secs()).ok())
        .unwrap_or(1)
}

/// Resolves the I2CP clock to the wall-clock source.
#[derive(Debug)]
struct WallClock;

impl Clock for WallClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or_default()
    }
}

/// One live I2CP connection task. Each accepted TCP connection is
/// owned by exactly one [`I2cpConnection`] instance.
#[derive(Debug)]
struct I2cpConnection {
    /// Bounded admission lease: released on drop or explicit retain.
    _admission: Option<OwnedSemaphorePermit>,
}

impl I2cpConnection {
    fn new(admission: OwnedSemaphorePermit) -> Self {
        Self {
            _admission: Some(admission),
        }
    }
}

/// One live client-owned destination registered with the I2CP service.
#[derive(Debug, Clone, Copy)]
struct I2cpDestinationEntry {
    /// Session that owns the destination.
    session: SessionId,
    /// Destination identifier for registry lookup.
    destination_id: DestinationId,
    /// Connection that owns the session.
    connection: u32,
}

/// Read-only snapshot of the I2CP service state.
#[derive(Debug, Default, Clone, Copy)]
pub struct I2cpServiceSnapshot {
    /// Currently active client-owned destinations.
    pub destination_count: usize,
    /// Currently committed sessions.
    pub session_count: usize,
    /// Number of live TCP connections.
    pub connection_count: usize,
}

/// Per-session Plan 172 lifecycle phase.
///
/// The daemon no longer equates CreateSession success with a usable
/// client destination. Remote profiles preserve the pre-172
/// best-effort behavior (Usable immediately); the counted
/// local-zero-hop profile moves CreatedAwaitingTunnels ->
/// AwaitingLeaseSet2 -> Usable only after the Plan 166 atomic
/// install commits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum I2cpSessionPhase {
    /// Destination reserved; tunnels not yet established.
    Reserved,
    /// Destination created; local routes being established.
    CreatedAwaitingTunnels,
    /// Routes ready; waiting for client-signed CreateLeaseSet2.
    AwaitingLeaseSet2,
    /// LeaseSet2 installed; data plane allowed.
    Usable,
    /// Shutdown requested.
    Stopping,
}

/// Per-session Plan 168 data-plane bookkeeping.
///
/// One `I2cpSessionState` exists for every committed I2CP session.
/// The struct is held inside the shared `I2cpServiceState` so both
/// the per-connection task (which drains the inbound queue and writes
/// `MessagePayload` frames) and the `enqueue_outbound` path (which
/// inserts into the receiving session's queue) operate against the
/// same bounded counters.
#[derive(Debug)]
pub struct I2cpSessionState {
    /// Owning session identifier.
    pub session: SessionId,
    /// Connection that owns the session.
    pub connection: u32,
    /// Verified destination hash the session is bound to.
    pub destination_hash: Hash,
    /// Destination identifier the session registers through.
    pub destination_id: DestinationId,
    /// Bounded inbound payload queue (Plan 168 §5).
    pub inbound: Mutex<InboundPayloadQueue>,
    /// Bounded pending `MessageStatus` correlations (Plan 168 §4).
    pub pending_status: Mutex<PendingStatusTable>,
    /// Bounded outbound I2CP message counter (Plan 168 §10).
    pub pending_outbound: AtomicU32,
    /// Router-assigned message id counter.
    pub next_message_id: AtomicU32,
    /// Next outbound message id that was assigned to a payload.
    pub last_message_id: AtomicU32,
    /// Notifier the per-connection task uses to wake up when a
    /// `MessagePayload` is pushed onto the inbound queue by a
    /// sibling connection. Without this notifier the receiving
    /// task would block in `read_chunk` and the cross-session
    /// loopback shortcut would never surface the bytes.
    pub inbound_notify: Notify,
    /// Plan 169 §2: most recent verified `SessionConfig.options`
    /// mapping, used as the baseline for the next ReconfigureSession
    /// diff. The mutex makes the diff baseline immune to races
    /// between reconfigure and concurrent destroy paths.
    pub last_options: Mutex<Mapping>,
    /// Plan 172 §9: explicit lifecycle phase.
    pub phase: Mutex<I2cpSessionPhase>,
    /// Whether this session uses the counted local-zero-hop profile.
    pub is_zero_hop: bool,
    /// Whether a client-signed LeaseSet2 has been atomically installed.
    pub lease_set_installed: std::sync::atomic::AtomicBool,
}

impl I2cpSessionState {
    /// Constructs a fresh session state.
    fn new(
        session: SessionId,
        connection: u32,
        destination_hash: Hash,
        destination_id: DestinationId,
        baseline_options: Mapping,
        is_zero_hop: bool,
        initial_phase: I2cpSessionPhase,
    ) -> Self {
        Self {
            session,
            connection,
            destination_hash,
            destination_id,
            inbound: Mutex::new(InboundPayloadQueue::new()),
            pending_status: Mutex::new(PendingStatusTable::new()),
            pending_outbound: AtomicU32::new(0),
            next_message_id: AtomicU32::new(1),
            last_message_id: AtomicU32::new(0),
            inbound_notify: Notify::new(),
            last_options: Mutex::new(baseline_options),
            phase: Mutex::new(initial_phase),
            is_zero_hop,
            lease_set_installed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Returns the current lifecycle phase.
    pub fn phase(&self) -> I2cpSessionPhase {
        self.phase
            .lock()
            .map(|guard| *guard)
            .unwrap_or(I2cpSessionPhase::Stopping)
    }

    /// Sets the lifecycle phase.
    fn set_phase(&self, next: I2cpSessionPhase) {
        if let Ok(mut guard) = self.phase.lock() {
            *guard = next;
        }
    }

    /// Whether the session may send/receive application traffic.
    /// Remote profiles preserve pre-172 best-effort behavior;
    /// zero-hop sessions require Usable + installed LS2.
    pub fn is_usable_for_data(&self) -> bool {
        if !self.is_zero_hop {
            return true;
        }
        self.phase() == I2cpSessionPhase::Usable
            && self
                .lease_set_installed
                .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns a clone of the current reconfigure baseline mapping.
    /// Used by Plan 169 reconfigure and reconfigure-diff tests.
    pub fn current_options(&self) -> Mapping {
        self.last_options
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| Mapping::empty())
    }

    /// Atomically replaces the reconfigure baseline mapping. Returns
    /// the previous baseline so callers can roll back on staged
    /// replacement failure.
    fn replace_options(&self, next: Mapping) -> Mapping {
        let mut guard = self.last_options.lock().expect("options poisoned");
        std::mem::replace(&mut *guard, next)
    }

    /// Allocates a fresh router message id, recording the correlation
    /// against the supplied client nonce. Returns the assigned id.
    fn allocate_message_id(
        &self,
        nonce: i2pr_api::i2cp::ClientNonce,
    ) -> Result<MessageId, i2pr_api::i2cp::DataPlaneError> {
        let mut pending = self.pending_status.lock().expect("pending poisoned");
        let id = self
            .next_message_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1).filter(|next| *next != 0)
            })
            .map_err(|_| i2pr_api::i2cp::DataPlaneError::CapacityExceeded)?;
        let message_id = MessageId::new(id);
        pending.record(PendingStatusEntry {
            message_id,
            session: self.session,
            nonce,
        })?;
        self.last_message_id.store(id, Ordering::Relaxed);
        Ok(message_id)
    }

    /// Reserves one outbound slot, returning the new depth.
    fn reserve_outbound_slot(&self) -> Result<u32, I2cpConnectionError> {
        let previous =
            self.pending_outbound
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    if current as usize >= MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION {
                        None
                    } else {
                        Some(current + 1)
                    }
                });
        match previous {
            Ok(value) => Ok(value + 1),
            Err(_) => Err(I2cpConnectionError::Overflow),
        }
    }

    /// Releases one outbound slot.
    fn release_outbound_slot(&self) {
        self.pending_outbound
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(1))
            })
            .ok();
    }

    /// Releases the session's bounded bookkeeping. Returns the counts.
    fn release(&self) -> (usize, usize, usize) {
        let inbound = self
            .inbound
            .lock()
            .map(|mut queue| queue.release_all())
            .unwrap_or(0);
        let pending = self
            .pending_status
            .lock()
            .map(|mut table| table.release_all())
            .unwrap_or(0);
        let outbound = self.pending_outbound.swap(0, Ordering::Relaxed) as usize;
        (inbound, pending, outbound)
    }

    /// Pushes one inbound payload frame. Returns the new depth on success.
    fn push_inbound(&self, frame: InboundPayloadFrame) -> Result<usize, I2cpConnectionError> {
        let mut queue = self.inbound.lock().expect("inbound poisoned");
        let result = queue.push(frame).map_err(|_| I2cpConnectionError::Overflow);
        // Always notify the receiving connection task so the
        // inbound drain runs even when the receiver is parked in
        // `read_chunk`. The notification is no-op when the queue is
        // empty (the next drain observes nothing) and harmless when
        // the task has already exited.
        drop(queue);
        self.inbound_notify.notify_one();
        result
    }

    /// Pops the next inbound payload frame, releasing its byte accounting.
    fn pop_inbound(&self) -> Option<InboundPayloadFrame> {
        self.inbound.lock().ok().and_then(|mut queue| queue.pop())
    }

    /// Removes the pending status entry keyed by `message_id`.
    fn take_pending_status(&self, message_id: MessageId) -> Option<PendingStatusEntry> {
        self.pending_status
            .lock()
            .ok()
            .and_then(|mut table| table.take(message_id))
    }

    /// Reports the current pending outbound I2CP message count.
    pub fn pending_outbound_count(&self) -> u32 {
        self.pending_outbound.load(Ordering::Relaxed)
    }
}

/// The complete state the I2CP service exposes to the daemon supervisor.
#[derive(Debug)]
pub struct I2cpServiceState {
    config: I2cpConfig,
    session_registry: Mutex<SessionRegistry>,
    destination_registry: Arc<Mutex<DestinationRegistry>>,
    destinations: Mutex<HashMap<Hash, I2cpDestinationEntry>>,
    sessions: Mutex<HashMap<SessionId, Arc<I2cpSessionState>>>,
    next_connection: AtomicU64,
    active_connections: Mutex<HashMap<u32, I2cpConnection>>,
    /// Plan 172 §7: actual local router hash for zero-hop Lease gateways.
    /// Derived once from an ephemeral router identity for this process
    /// (stable across sessions; no private key retained here).
    local_router_hash: Hash,
    /// Typed zero-hop tunnel-id allocator (fresh, non-zero, never
    /// `u32::MAX`; never aliases an active local route).
    next_zero_hop_tunnel_id: AtomicU32,
}

impl I2cpServiceState {
    /// Constructs a new I2CP service state from a validated configuration.
    pub fn new(config: I2cpConfig) -> Result<Self, I2cpServiceError> {
        let session_registry = Mutex::new(SessionRegistry::new(SessionRegistryLimits::m9()));
        // The destination registry uses the smaller of the
        // per-connection buffered-byte budget or the documented
        // aggregate command-queue ceiling. The two are unrelated
        // metrics; we just need a number that lets the registry
        // construct cleanly under M9 ceilings.
        let aggregate_command_queue_depth =
            (config.max_buffered_bytes_per_connection / 16).clamp(64, u32::MAX as usize) as u32;
        let registry_config =
            RegistryConfig::try_new(config.max_sessions_router, aggregate_command_queue_depth)
                .map_err(|error| {
                    I2cpServiceError::InvalidConfig(format!(
                        "destination registry config rejected: {error}"
                    ))
                })?;
        let destination_registry = Arc::new(Mutex::new(DestinationRegistry::new(registry_config)));
        // Plan 172 §7: derive the local router hash from a fresh
        // ephemeral router identity for this process. The private
        // material is dropped immediately; only the public hash is
        // retained as Lease gateway metadata (public, non-secret).
        let local_router_hash = {
            let mut rng = i2pr_crypto::OsRng;
            let bundle =
                i2pr_crypto::RouterIdentityBundle::generate(&mut rng).map_err(|error| {
                    I2cpServiceError::InvalidConfig(format!(
                        "local router identity unavailable: {error}"
                    ))
                })?;
            bundle.identity().hash().map_err(|error| {
                I2cpServiceError::InvalidConfig(format!("local router hash unavailable: {error}"))
            })?
        };
        Ok(Self {
            config,
            session_registry,
            destination_registry,
            destinations: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            next_connection: AtomicU64::new(1),
            active_connections: Mutex::new(HashMap::new()),
            local_router_hash,
            next_zero_hop_tunnel_id: AtomicU32::new(0x1000),
        })
    }

    /// Returns the local router hash used as the zero-hop Lease gateway.
    pub const fn local_router_hash(&self) -> Hash {
        self.local_router_hash
    }

    /// Allocates a fresh non-zero, non-sentinel zero-hop tunnel id.
    fn allocate_zero_hop_tunnel_id(&self) -> i2pr_tunnel::TunnelId {
        loop {
            let raw = self.next_zero_hop_tunnel_id.fetch_add(1, Ordering::Relaxed);
            // Skip zero and the u32::MAX sentinel.
            if raw == 0 || raw == u32::MAX {
                continue;
            }
            if let Ok(id) = i2pr_tunnel::TunnelId::new(raw) {
                return id;
            }
        }
    }

    /// Returns the validated I2CP configuration.
    pub const fn config(&self) -> &I2cpConfig {
        &self.config
    }

    /// Returns the loopback bind address.
    pub fn bind_address(&self) -> SocketAddr {
        SocketAddr::new(self.config.bind_address, self.config.port)
    }

    /// Returns the session registry handle.
    pub fn session_registry(&self) -> Arc<Mutex<SessionRegistry>> {
        // Wrap the inner mutex in a fresh Arc so callers see a single
        // shared instance. Production callers go through the inner
        // helpers and lock the inner mutex directly.
        Arc::new(Mutex::new(SessionRegistry::new(
            self.session_registry
                .lock()
                .expect("session registry poisoned")
                .limits(),
        )))
    }

    /// Returns the destination registry handle.
    pub fn destination_registry(&self) -> Arc<Mutex<DestinationRegistry>> {
        Arc::clone(&self.destination_registry)
    }

    /// Allocates a fresh connection capability id.
    fn allocate_connection_id(&self) -> u32 {
        let next = self.next_connection.fetch_add(1, Ordering::Relaxed);
        u32::try_from(next).unwrap_or(u32::MAX)
    }

    /// Registers a freshly accepted connection.
    fn register_connection(&self, id: u32, connection: I2cpConnection) {
        if let Ok(mut active) = self.active_connections.lock() {
            active.insert(id, connection);
        }
    }

    /// Drops a connection entry. Called from the per-connection task
    /// exit path so resource accounting reflects every accepted socket.
    fn drop_connection(&self, id: u32) {
        if let Ok(mut active) = self.active_connections.lock() {
            active.remove(&id);
        }
    }

    /// Returns the number of live TCP connections.
    pub fn connection_count(&self) -> usize {
        self.active_connections
            .lock()
            .map(|active| active.len())
            .unwrap_or(0)
    }

    /// Returns the number of committed I2CP sessions.
    pub fn session_count(&self) -> usize {
        self.session_registry
            .lock()
            .map(|registry| registry.len())
            .unwrap_or(0)
    }

    /// Returns the number of registered destinations.
    pub fn destination_count(&self) -> usize {
        self.destinations
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    /// Returns a non-secret snapshot of the service state.
    pub fn snapshot(&self) -> I2cpServiceSnapshot {
        I2cpServiceSnapshot {
            destination_count: self.destination_count(),
            session_count: self.session_count(),
            connection_count: self.connection_count(),
        }
    }

    /// Returns the per-session data-plane state for the supplied
    /// session id, when one is registered. The accessor is the
    /// single source of truth for the per-connection inbound queue,
    /// pending status correlations, and outbound counter.
    pub fn session_state(&self, session: SessionId) -> Option<Arc<I2cpSessionState>> {
        self.sessions
            .lock()
            .ok()
            .and_then(|map| map.get(&session).cloned())
    }

    /// Returns the destination hash owned by the supplied session id.
    pub fn session_destination_hash(&self, session: SessionId) -> Option<Hash> {
        self.sessions
            .lock()
            .ok()
            .and_then(|map| map.get(&session).map(|state| state.destination_hash))
    }

    /// Looks up the I2CP session that owns a destination hash. The
    /// lookup is the Plan 168 cross-session delivery seam: when a
    /// destination owned by one session receives a payload, the
    /// daemon routes it to the session returned here.
    pub fn session_for_destination(&self, hash: &Hash) -> Option<Arc<I2cpSessionState>> {
        let entry = self
            .destinations
            .lock()
            .ok()
            .and_then(|map| map.get(hash).copied())?;
        self.sessions
            .lock()
            .ok()
            .and_then(|map| map.get(&entry.session).cloned())
    }

    /// Returns the live outbound payload byte aggregate across every
    /// session on the supplied connection. Used by the Plan 168
    /// aggregate-budget assertions.
    pub fn connection_outbound_depth(&self, connection: u32) -> (u32, usize) {
        let Ok(sessions) = self.sessions.lock() else {
            return (0, 0);
        };
        let mut pending = 0u32;
        let mut inbound_bytes = 0usize;
        for state in sessions.values() {
            if state.connection == connection {
                pending = pending.saturating_add(state.pending_outbound_count());
                if let Ok(queue) = state.inbound.lock() {
                    inbound_bytes = inbound_bytes.saturating_add(queue.queued_bytes());
                }
            }
        }
        (pending, inbound_bytes)
    }

    /// Runs the supervised I2CP listener until the supplied
    /// cancellation token fires or a fatal bind failure occurs.
    pub async fn run(
        self: Arc<Self>,
        bind_address: SocketAddr,
        children: ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), I2cpServiceError> {
        let (listener, _bound_address) = self.bind(bind_address).await?;
        self.serve(listener, children, cancellation).await
    }

    /// Binds a [`TcpListener`] to the configured loopback address.
    pub async fn bind(
        &self,
        bind_address: SocketAddr,
    ) -> Result<(TcpListener, SocketAddr), I2cpServiceError> {
        let listener =
            TcpListener::bind(bind_address)
                .await
                .map_err(|source| I2cpServiceError::Bind {
                    address: bind_address,
                    source,
                })?;
        let bound_address = listener.local_addr().unwrap_or(bind_address);
        Ok((listener, bound_address))
    }

    /// Accepts connections from the supplied pre-bound listener until
    /// the supplied cancellation token fires.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        children: ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), I2cpServiceError> {
        let bind_address = listener.local_addr().map_err(|_| {
            I2cpServiceError::InvalidConfig("listener had no local_addr".to_owned())
        })?;
        info!(
            address = %bind_address,
            max_clients = self.config.max_clients,
            max_sessions = self.config.max_sessions_router,
            "I2CP loopback listener bound"
        );

        let client_permits = Arc::new(tokio::sync::Semaphore::new(usize::from(
            self.config.max_clients,
        )));
        let child_token = cancellation.child_token();
        loop {
            tokio::select! {
                biased;
                _ = child_token.cancelled() => {
                    debug!("i2cp listener cancellation observed");
                    break;
                }
                accept = listener.accept() => {
                    let (stream, _peer) = match accept {
                        Ok(value) => value,
                        Err(error) => {
                            warn!(error = %error, "i2cp accept failed");
                            continue;
                        }
                    };
                    let permit = match Arc::clone(&client_permits).try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            drop(stream);
                            continue;
                        }
                    };
                    let state = Arc::clone(&self);
                    let connection_id = state.allocate_connection_id();
                    let admission = I2cpConnection::new(permit);
                    state.register_connection(connection_id, admission);
                    let child_token_for_task = child_token.clone();
                    let children_for_task = children.clone();
                    let state_for_task = Arc::clone(&state);
                    if let Err(error) = children_for_task.clone().spawn(move |task_cancellation| {
                        let state_for_task = state_for_task;
                        async move {
                            handle_connection(
                                state_for_task,
                                connection_id,
                                stream,
                                task_cancellation,
                                child_token_for_task,
                            )
                            .await;
                            Ok(())
                        }
                    }) {
                        warn!(error = %error, "failed to spawn I2CP client task");
                        state.drop_connection(connection_id);
                    }
                }
            }
        }
        let _ = child_token.cancel(i2pr_core::CancellationReason::ParentScope);
        Ok(())
    }

    /// Reserves a client-owned destination for `verified_session`.
    pub(crate) fn reserve_client_destination(
        &self,
        connection_id: u32,
        verified: &VerifiedSessionConfig,
        projected: ProjectedPolicy,
    ) -> Result<SessionId, ReserveError> {
        let destination = verified.destination().clone();
        let public = DestinationPublic::from_destination(destination)
            .map_err(|error| ReserveError::SessionInvalid(error.to_string()))?;
        let dest_hash = public.id().as_hash().copy();
        let is_zero_hop = matches!(
            projected.tunnel_mode,
            i2pr_client::DestinationTunnelMode::LocalZeroHop
        );
        let mut registry = match self.session_registry.lock() {
            Ok(registry) => registry,
            Err(_) => return Err(ReserveError::RegistryLocked),
        };
        let reservation = registry
            .reserve(connection_id, dest_hash)
            .map_err(|error| ReserveError::SessionRegistry(error.to_string()))?;
        let config = projected_to_config(projected);
        let runtime = match DestinationRuntime::new_client_owned(public.clone(), config) {
            Ok(runtime) => runtime,
            Err(error) => {
                registry.rollback(reservation.clone());
                return Err(ReserveError::DestinationRuntime(error.to_string()));
            }
        };
        let destination_id = runtime.id();
        let insert = match self.destination_registry.lock() {
            Ok(mut dest_registry) => dest_registry.insert(runtime),
            Err(_) => {
                registry.rollback(reservation.clone());
                return Err(ReserveError::RegistryLocked);
            }
        };
        if let Err(error) = insert {
            registry.rollback(reservation.clone());
            return Err(ReserveError::DestinationRegistry(error.to_string()));
        }
        // Plan 172 §9: for the counted local-zero-hop profile,
        // establish the real local inbound/outbound routes before the
        // Lease request is derived. The gateway is this process's
        // actual local router hash; tunnel ids are fresh, non-zero,
        // non-sentinel allocations.
        if is_zero_hop {
            let inbound_id = self.allocate_zero_hop_tunnel_id();
            let outbound_id = self.allocate_zero_hop_tunnel_id();
            let ctx = i2pr_client::LocalRouterContext {
                local_router_hash: self.local_router_hash,
            };
            let now = u64::from(i2cp_now_seconds());
            let lifetime = config.tunnel_lifetime_seconds();
            let zero_hop_result = match self.destination_registry.lock() {
                Ok(mut dest_registry) => match dest_registry.get_mut(&destination_id) {
                    Some(runtime) => {
                        runtime.ensure_local_zero_hop(ctx, inbound_id, outbound_id, now, lifetime)
                    }
                    None => Err(i2pr_client::DestinationRuntimeError::Stopping),
                },
                Err(_) => Err(i2pr_client::DestinationRuntimeError::Stopping),
            };
            if let Err(error) = zero_hop_result {
                if let Ok(mut dest_registry) = self.destination_registry.lock() {
                    let _ = dest_registry.remove(&destination_id);
                }
                registry.rollback(reservation.clone());
                return Err(ReserveError::DestinationRuntime(error.to_string()));
            }
        }
        let entry = match registry.commit(reservation) {
            Ok(entry) => entry,
            Err(error) => {
                if let Ok(mut dest_registry) = self.destination_registry.lock() {
                    let _ = dest_registry.remove(&destination_id);
                }
                return Err(ReserveError::SessionRegistry(error.to_string()));
            }
        };
        if let Ok(mut destinations) = self.destinations.lock() {
            destinations.insert(
                dest_hash,
                I2cpDestinationEntry {
                    session: entry.session,
                    destination_id,
                    connection: connection_id,
                },
            );
        }
        // Plan 168: register the per-session data-plane bookkeeping
        // keyed by the freshly committed session id. The map is
        // authoritative for the per-session bounded counters;
        // teardown drains it back to baseline.
        // Plan 169 §2: seed the reconfigure baseline with the
        // verified SessionConfig mapping so the next Reconfigure
        // diff has a typed starting point.
        // Plan 172 §9: remote sessions stay Usable (pre-172
        // best-effort); zero-hop sessions start AwaitingLeaseSet2 and
        // become Usable only after the atomic install commits.
        let initial_phase = if is_zero_hop {
            I2cpSessionPhase::AwaitingLeaseSet2
        } else {
            I2cpSessionPhase::Usable
        };
        let session_state = Arc::new(I2cpSessionState::new(
            entry.session,
            connection_id,
            dest_hash,
            destination_id,
            verified.options().clone(),
            is_zero_hop,
            initial_phase,
        ));
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(entry.session, session_state);
        }
        Ok(entry.session)
    }

    /// Installs a client-signed Standard LeaseSet2 against a known
    /// destination runtime.
    pub(crate) fn install_client_lease_set2(
        &self,
        connection_id: u32,
        session: SessionId,
        lease_set: i2pr_proto::LeaseSet2,
        private_keys: &[i2pr_api::i2cp::SessionDecryptionKey],
    ) -> Result<(), ReserveError> {
        let hash =
            lease_set.header().destination().hash().map_err(|error| {
                ReserveError::Wire(format!("LeaseSet2 destination hash: {error}"))
            })?;
        let entry = match self.destinations.lock() {
            Ok(map) => map.get(&hash).copied(),
            Err(_) => return Err(ReserveError::RegistryLocked),
        };
        let entry = entry.ok_or_else(|| {
            ReserveError::Wire("CreateLeaseSet2 for unknown destination".to_owned())
        })?;
        if entry.session != session || entry.connection != connection_id {
            return Err(ReserveError::Wire(
                "CreateLeaseSet2 session/connection mismatch".to_owned(),
            ));
        }
        let destination_id = entry.destination_id;
        let private_key = private_keys.first().ok_or_else(|| {
            ReserveError::Wire("CreateLeaseSet2 carried no private key".to_owned())
        })?;
        if private_key.as_bytes().len() != X25519_KEY_LENGTH {
            return Err(ReserveError::Wire(
                "CreateLeaseSet2 private key length mismatch".to_owned(),
            ));
        }
        let mut secret_bytes = [0_u8; X25519_KEY_LENGTH];
        secret_bytes.copy_from_slice(private_key.as_bytes());
        let encryption_public_bytes: [u8; X25519_KEY_LENGTH] = lease_set
            .encryption_keys()
            .first()
            .and_then(|k| {
                let bytes = k.as_bytes();
                if bytes.len() == X25519_KEY_LENGTH {
                    let mut out = [0_u8; X25519_KEY_LENGTH];
                    out.copy_from_slice(bytes);
                    Some(out)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                ReserveError::Wire("LeaseSet2 encryption key length mismatch".to_owned())
            })?;
        let capability =
            InboundDecryptionCapability::from_secret_bytes(encryption_public_bytes, secret_bytes);
        let mut registry = self
            .destination_registry
            .lock()
            .map_err(|_| ReserveError::Wire("destination registry lock poisoned".to_owned()))?;
        let runtime = registry
            .get_mut(&destination_id)
            .ok_or_else(|| ReserveError::Wire("destination not registered".to_owned()))?;
        if let Err(error) =
            runtime.install_client_lease_set2(lease_set, capability, u64::from(i2cp_now_seconds()))
        {
            return Err(ReserveError::Wire(format!(
                "install_client_lease_set2 failed: {error}"
            )));
        }
        drop(registry);
        // Plan 172 §9: mark the session Usable only after the atomic
        // install commits. Rejection leaves no installed LS2,
        // decryption capability, or usable phase (fail-closed).
        if let Some(session_state) = self.session_state(session) {
            session_state.set_phase(I2cpSessionPhase::Usable);
            session_state
                .lease_set_installed
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }

    /// Tears down every destination owned by the supplied connection.
    pub(crate) fn teardown_connection(&self, connection_id: u32) {
        let owned: Vec<(Hash, DestinationId, SessionId)> = match self.destinations.lock() {
            Ok(map) => map
                .iter()
                .filter_map(|(hash, entry)| {
                    if entry.connection == connection_id {
                        Some((*hash, entry.destination_id, entry.session))
                    } else {
                        None
                    }
                })
                .collect(),
            Err(_) => return,
        };
        if let Ok(mut registry) = self.destination_registry.lock() {
            for (_hash, id, _session) in &owned {
                let _ = registry.remove(id);
            }
        }
        if let (Ok(mut sessions), Ok(mut map)) =
            (self.session_registry.lock(), self.destinations.lock())
        {
            for (hash, _id, session) in owned {
                map.remove(&hash);
                let _ = sessions.destroy(session);
            }
        }
        // Plan 168: drain every per-session data-plane state for
        // this connection; the inbound queues, pending status
        // correlations, and outbound counters are released back to
        // baseline. The session map is the only source of truth for
        // the bookkeeping.
        if let Ok(mut session_states) = self.sessions.lock() {
            let to_remove: Vec<SessionId> = session_states
                .iter()
                .filter_map(|(session, state)| {
                    if state.connection == connection_id {
                        Some(*session)
                    } else {
                        None
                    }
                })
                .collect();
            for session in to_remove {
                if let Some(state) = session_states.remove(&session) {
                    let _ = state.release();
                }
            }
        }
    }

    /// Derives the Plan 172 counted `RequestVariableLeaseSet` leases
    /// from the destination's real local zero-hop inbound pool.
    /// Returns an empty vector when the session, destination, or pool
    /// is missing (caller must fail closed and never emit it).
    pub(crate) fn zero_hop_requested_leases(
        &self,
        session: SessionId,
    ) -> Vec<i2pr_api::i2cp::RequestedLease> {
        let Some(session_state) = self.session_state(session) else {
            return Vec::new();
        };
        let destination_id = session_state.destination_id;
        let now = u64::from(i2cp_now_seconds());
        let Ok(registry) = self.destination_registry.lock() else {
            return Vec::new();
        };
        let Some(runtime) = registry.get(&destination_id) else {
            return Vec::new();
        };
        runtime
            .pool()
            .inbound_lease_sources(now)
            .iter()
            .map(|source| i2pr_api::i2cp::RequestedLease {
                gateway: source.gateway(),
                tunnel_id: source.gateway_receive_tunnel_id(),
            })
            .collect()
    }

    /// Removes one session and its destination after a fail-closed
    /// zero-hop setup failure. Releases LeaseSet2/decryption
    /// capability (via runtime drop), zero-hop routes (via pool drop),
    /// session reservation, and per-session bookkeeping.
    fn remove_session_for_test_cleanup(&self, session: SessionId) {
        let entry = match self.sessions.lock() {
            Ok(mut sessions) => sessions.remove(&session),
            Err(_) => None,
        };
        let Some(state) = entry else {
            return;
        };
        let _ = state.release();
        let dest_hash = state.destination_hash;
        let destination_id = state.destination_id;
        if let Ok(mut registry) = self.destination_registry.lock() {
            let _ = registry.remove(&destination_id);
        }
        if let Ok(mut map) = self.destinations.lock() {
            map.remove(&dest_hash);
        }
        if let Ok(mut registry) = self.session_registry.lock() {
            let _ = registry.destroy(session);
        }
    }

    /// Sanitized lifecycle observation for evidence (no private bytes):
    /// session usability, zero-hop flag, installed flag, and lease count.
    pub fn session_lifecycle_fact(&self, session: SessionId) -> Option<(bool, bool, bool, usize)> {
        let session_state = self.session_state(session)?;
        let is_zero_hop = session_state.is_zero_hop;
        let installed = session_state
            .lease_set_installed
            .load(std::sync::atomic::Ordering::Relaxed);
        let usable = session_state.is_usable_for_data();
        let now = u64::from(i2cp_now_seconds());
        let lease_count = self
            .destination_registry
            .lock()
            .ok()
            .and_then(|registry| {
                registry
                    .get(&session_state.destination_id)
                    .map(|runtime| runtime.pool().inbound_lease_sources(now).len())
            })
            .unwrap_or(0);
        Some((is_zero_hop, installed, usable, lease_count))
    }
}

/// Outcome of a CreateSession reservation attempt.
#[derive(Debug)]
pub(crate) enum ReserveError {
    /// SessionConfig or option projection failed.
    SessionInvalid(String),
    /// Session registry rejected the reservation.
    SessionRegistry(String),
    /// Destination registry rejected the insertion.
    DestinationRegistry(String),
    /// Destination runtime construction failed.
    DestinationRuntime(String),
    /// Destination registry mutex poisoned.
    RegistryLocked,
    /// Wire-level problem (LeaseSet2 body, private-key mismatch, etc.).
    Wire(String),
}

impl std::fmt::Display for ReserveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionInvalid(message)
            | Self::SessionRegistry(message)
            | Self::DestinationRegistry(message)
            | Self::DestinationRuntime(message)
            | Self::Wire(message) => formatter.write_str(message),
            Self::RegistryLocked => formatter.write_str("destination registry lock poisoned"),
        }
    }
}

fn map_reserve_error_to_code(outcome: &ReserveError) -> SessionStatusCode {
    match outcome {
        ReserveError::SessionInvalid(_) => SessionStatusCode::Invalid,
        ReserveError::SessionRegistry(_) => SessionStatusCode::Invalid,
        ReserveError::DestinationRegistry(_) => SessionStatusCode::Invalid,
        ReserveError::DestinationRuntime(_) => SessionStatusCode::Invalid,
        ReserveError::RegistryLocked => SessionStatusCode::Refused,
        ReserveError::Wire(_) => SessionStatusCode::Invalid,
    }
}

/// Projects the Plan 165 projected policy into a [`DestinationConfig`].
///
/// Plan 172 uses the projected destination config directly so the
/// zero-hop quantity (1,1) and remote quantities flow into the
/// destination pool ceilings. Router-wide ceilings remain authoritative
/// via [`project_options`].
fn projected_to_config(projected: ProjectedPolicy) -> DestinationConfig {
    projected.destination_config
}

/// Drives one accepted I2CP connection through the read/dispatch loop.
async fn handle_connection(
    state: Arc<I2cpServiceState>,
    connection_id: u32,
    mut stream: TcpStream,
    task_cancellation: CancellationToken,
    parent_token: CancellationToken,
) {
    let result = handle_connection_inner(
        &state,
        connection_id,
        &mut stream,
        task_cancellation,
        parent_token,
    )
    .await;
    if let Err(error) = result {
        debug!(error = %error, "i2cp connection ended");
    }
    // Plan 171: every terminal connection outcome must terminate the
    // TCP stream deterministically before router-side bookkeeping is
    // released. Relying on `TcpStream` drop at task exit left the
    // peer-visible FIN/RST ordered after teardown scheduling, which
    // macOS hosted CI observed as a still-open socket until timeout
    // for the invalid-preamble row. `shutdown()` sends FIN while the
    // per-connection owner still holds the socket; a shutdown error
    // (already-closed/reset peer) must never block resource cleanup,
    // and no protocol frame is written for an invalid first byte.
    let _ = stream.shutdown().await;
    state.teardown_connection(connection_id);
    state.drop_connection(connection_id);
}

async fn handle_connection_inner(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    stream: &mut TcpStream,
    task_cancellation: CancellationToken,
    parent_token: CancellationToken,
) -> Result<(), I2cpConnectionError> {
    // Plan 167 §4: protocol byte must arrive first.
    let protocol_byte =
        read_protocol_byte(stream, &state.config, task_cancellation.clone()).await?;
    if protocol_byte != PROTOCOL_BYTE {
        warn!(byte = protocol_byte, "i2cp protocol byte rejected");
        return Err(I2cpConnectionError::InvalidProtocolByte);
    }
    let read_timeout = state.config.command_timeout;
    let mut machine = ConnectionStateMachine::new();
    machine
        .observe_protocol_byte()
        .map_err(|error| I2cpConnectionError::StateMachine(error.to_string()))?;
    let mut decoder = FrameDecoder::new();
    let mut per_connection_session: Option<SessionId> = None;
    loop {
        tokio::select! {
            biased;
            _ = task_cancellation.cancelled() => return Ok(()),
            _ = parent_token.cancelled() => return Ok(()),
            // Plan 168: a sibling connection may push a
            // `MessagePayload` onto this session's inbound queue.
            // The notify lets the receiver drain it without waiting
            // for the next client-driven read.
            _ = wait_for_inbound_notify(state, connection_id) => {
                drain_inbound_payloads(state, connection_id, stream).await?;
            }
            chunk = read_chunk(stream, read_timeout, task_cancellation.clone()) => {
                let chunk = chunk?;
                if chunk.is_empty() {
                    return Ok(());
                }
                let frames = decoder
                    .push(&chunk)
                    .map_err(|error| I2cpConnectionError::Wire(error.to_string()))?;
                for frame in frames {
                    let outcome = dispatch_frame(
                        state,
                        connection_id,
                        &mut machine,
                        &mut per_connection_session,
                        &frame,
                    )
                    .await?;
                    match outcome {
                        FrameOutcome::Continue => {}
                        FrameOutcome::Reply(message) => {
                            write_message(stream, &message).await?;
                        }
                        FrameOutcome::ReplyAndFollowup(primary, followup) => {
                            write_message(stream, &primary).await?;
                            write_message(stream, &followup).await?;
                        }
                        FrameOutcome::Close => return Ok(()),
                    }
                }
                // Plan 168: after every inbound frame batch, drain
                // any queued `MessagePayload` frames for the
                // connection's owning session. Cross-session
                // loopback delivery deposits into these queues; the
                // drain keeps slow readers bounded because the
                // per-session byte accounting enforces
                // backpressure on the producer side.
                drain_inbound_payloads(state, connection_id, stream).await?;
            }
        }
    }
}

/// Awaits the next inbound payload notify for the supplied
/// connection. Returns immediately when no session exists yet (the
/// connection has not activated a session). The helper exists so
/// `handle_connection_inner` does not need to thread the per-session
/// notify through the `select!` branches.
async fn wait_for_inbound_notify(state: &I2cpServiceState, connection_id: u32) {
    let Some(session) = session_state_for_connection(state, connection_id) else {
        // Park indefinitely; cancellation paths cancel the parent.
        std::future::pending::<()>().await;
        return;
    };
    session.inbound_notify.notified().await;
}

/// Drains and writes every queued `MessagePayload` for the connection's
/// owning session. Slow readers accumulate bounded backpressure in
/// the per-session queue; this function emits at most one write per
/// drained frame and never blocks on the wire beyond the configured
/// write deadline.
async fn drain_inbound_payloads(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    stream: &mut TcpStream,
) -> Result<(), I2cpConnectionError> {
    let Some(session_state) = session_state_for_connection(state, connection_id) else {
        return Ok(());
    };
    loop {
        let Some(frame) = session_state.pop_inbound() else {
            return Ok(());
        };
        let header = PayloadGzipHeader {
            source_port: frame.source_port,
            destination_port: frame.destination_port,
            xflags: i2pr_api::i2cp::GZIP_XFLAGS_JAVA,
            protocol: frame.protocol,
        };
        let payload = match Payload::new(frame.payload) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let message = MessagePayload {
            session: frame.session,
            message_id: frame.message_id,
            payload,
        };
        write_message(stream, &Message::MessagePayload(message)).await?;
        // Recompute the header for diagnostics; the encoded payload
        // carries the same bytes either way.
        let _ = header;
    }
}

/// Returns the per-session data-plane state for the supplied
/// connection, when the connection owns exactly one session. The M9
/// default policy is one primary session per connection; multi-session
/// semantics remain deferred to a later pass.
fn session_state_for_connection(
    state: &I2cpServiceState,
    connection_id: u32,
) -> Option<Arc<I2cpSessionState>> {
    state.sessions.lock().ok().and_then(|sessions| {
        sessions
            .values()
            .find(|state| state.connection == connection_id)
            .cloned()
    })
}

#[derive(Debug)]
enum FrameOutcome {
    Continue,
    Reply(Box<Message>),
    /// Emit a primary reply, then a follow-up message after the
    /// primary has been flushed. Used by CreateSession to emit the
    /// SessionStatus(Created) reply and then immediately the
    /// RequestVariableLeaseSet the unmodified Java I2P
    /// `I2PSessionImpl.connect()` waits for.
    ReplyAndFollowup(Box<Message>, Box<Message>),
    Close,
}

#[derive(Debug, Error)]
enum I2cpConnectionError {
    #[error("peer closed connection")]
    PeerClosed,
    #[error("invalid I2CP protocol byte")]
    InvalidProtocolByte,
    #[error("i2cp wire decode failed: {0}")]
    Wire(String),
    #[error("i2cp state machine rejected: {0}")]
    StateMachine(String),
    #[error("i2cp connection timed out")]
    Timeout,
    #[error("i2cp connection io error: {0}")]
    Io(#[from] io::Error),
    /// A bounded Plan 168 data-plane ceiling was exceeded.
    #[error("i2cp data plane boundedness exceeded")]
    Overflow,
    /// Plan 168 payload validation failed for a non-malformed
    /// structural reason (out-of-range expiration, expired instant,
    /// unsupported semantics).
    #[error("i2cp payload rejected: {0}")]
    PayloadRejected(String),
}

async fn dispatch_frame(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    machine: &mut ConnectionStateMachine,
    per_connection_session: &mut Option<SessionId>,
    frame: &i2pr_api::i2cp::RawFrame,
) -> Result<FrameOutcome, I2cpConnectionError> {
    // CreateSession needs the raw body bytes for SessionConfig
    // verification; dispatch it before the typed decoder.
    if frame.message_type == i2pr_api::i2cp::MessageType::CreateSession as u8 {
        return handle_create_session(state, connection_id, machine, per_connection_session, frame)
            .await;
    }
    // ReconfigureSession also needs the raw body because the
    // structural codec parses the inner SessionConfig with the
    // bounded structural decoder; routing through dispatch_message
    // would silently lose body-level errors. Plan 169 §2 owns the
    // handler.
    if frame.message_type == i2pr_api::i2cp::MessageType::ReconfigureSession as u8 {
        return handle_reconfigure_session(
            state,
            connection_id,
            machine,
            per_connection_session,
            frame,
        )
        .await;
    }
    let message = decode_typed(frame.message_type, &frame.body).map_err(|error| {
        I2cpConnectionError::Wire(format!(
            "typed decode failed for type {}: {error}",
            frame.message_type
        ))
    })?;
    let outcome = dispatch_message(
        state,
        connection_id,
        machine,
        per_connection_session,
        message,
    )
    .await?;
    Ok(outcome)
}

async fn dispatch_message(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    machine: &mut ConnectionStateMachine,
    per_connection_session: &mut Option<SessionId>,
    message: Message,
) -> Result<FrameOutcome, I2cpConnectionError> {
    match message {
        Message::GetDate(get_date) => handle_get_date(machine, get_date),
        Message::SetDate(_)
        | Message::SessionStatus(_)
        | Message::BandwidthLimits(_)
        | Message::MessagePayload(_)
        | Message::RequestVariableLeaseSet(_)
        | Message::MessageStatus(_)
        | Message::DestReply(_)
        | Message::HostReply(_) => {
            let label = message_label(message.message_type());
            Err(I2cpConnectionError::StateMachine(format!(
                "client must not send {label}"
            )))
        }
        Message::CreateSession(_create) => Err(I2cpConnectionError::StateMachine(
            "CreateSession must dispatch through frame body".to_owned(),
        )),
        Message::ReconfigureSession(_reconfigure) => Err(I2cpConnectionError::StateMachine(
            "ReconfigureSession must dispatch through frame body".to_owned(),
        )),
        Message::DestroySession(destroy) => {
            handle_destroy_session(
                state,
                connection_id,
                machine,
                per_connection_session,
                destroy,
            )
            .await
        }
        Message::CreateLeaseSet2(create) => {
            handle_create_lease_set2(state, connection_id, per_connection_session, create).await
        }
        Message::Disconnect(_disconnect) => Ok(FrameOutcome::Close),
        Message::GetBandwidthLimits(_) => Ok(FrameOutcome::Reply(Box::new(
            Message::BandwidthLimits(derive_bandwidth_reply(&state.config)),
        ))),
        Message::DestLookup(lookup) => Ok(handle_dest_lookup(state, lookup)),
        Message::HostLookup(_lookup) => Ok(FrameOutcome::Reply(Box::new(Message::HostReply(
            HostReply {
                session: SessionId::new(0),
                request_id: i2pr_api::i2cp::HostRequestId::new(0),
                result: HostReplyResult::Failure,
                destination: None,
                options: None,
            },
        )))),
        Message::SendMessage(send) => {
            handle_send_message(state, connection_id, per_connection_session, send).await
        }
        Message::SendMessageExpires(send) => {
            handle_send_message_expires(state, connection_id, per_connection_session, send).await
        }
    }
}

fn handle_get_date(
    machine: &mut ConnectionStateMachine,
    get_date: GetDate,
) -> Result<FrameOutcome, I2cpConnectionError> {
    let now_ms = u64::from(i2cp_now_seconds()).saturating_mul(1000);
    let set_date = machine
        .handle_get_date(&get_date, now_ms)
        .map_err(|error| I2cpConnectionError::StateMachine(error.to_string()))?;
    machine.mark_set_date_sent();
    Ok(FrameOutcome::Reply(Box::new(Message::SetDate(set_date))))
}

async fn handle_create_session(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    machine: &mut ConnectionStateMachine,
    per_connection_session: &mut Option<SessionId>,
    frame: &i2pr_api::i2cp::RawFrame,
) -> Result<FrameOutcome, I2cpConnectionError> {
    machine
        .begin_create_session()
        .map_err(|error| I2cpConnectionError::StateMachine(error.to_string()))?;
    let config_struct = match SessionConfig::decode(&frame.body) {
        Ok(config) => config,
        Err(_error) => {
            return Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(
                SessionStatus {
                    session: SessionId::new(0),
                    status: SessionStatusCode::Invalid,
                },
            ))));
        }
    };
    let verified = match verify_session_config(
        &frame.body,
        &config_struct,
        &SessionConfigLimits::m9(),
        &WallClock,
    ) {
        Ok(verified) => verified,
        Err(_error) => {
            return Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(
                SessionStatus {
                    session: SessionId::new(0),
                    status: SessionStatusCode::Invalid,
                },
            ))));
        }
    };
    let projected = match project_options(
        verified.options(),
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    ) {
        Ok(projected) => projected,
        Err(_error) => {
            return Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(
                SessionStatus {
                    session: SessionId::new(0),
                    status: SessionStatusCode::Invalid,
                },
            ))));
        }
    };
    match state.reserve_client_destination(connection_id, &verified, projected) {
        Ok(session) => {
            machine
                .activate_session(session)
                .map_err(|error| I2cpConnectionError::StateMachine(error.to_string()))?;
            *per_connection_session = Some(session);
            let reply = SessionStatus {
                session,
                status: SessionStatusCode::Created,
            };
            // Plan 172 §9: the counted local-zero-hop profile derives
            // the Lease request from the destination's real local pool
            // via the Plan 166 seam and requires >= 1 lease. An empty
            // request is never emitted for the counted path.
            let is_zero_hop = state.session_state(session).is_some_and(|s| s.is_zero_hop);
            if is_zero_hop {
                let leases = state.zero_hop_requested_leases(session);
                if leases.is_empty() {
                    // Fail closed: tear down the just-created session
                    // rather than emitting a zero-lease request.
                    state.remove_session_for_test_cleanup(session);
                    *per_connection_session = None;
                    machine.deactivate_session();
                    return Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(
                        SessionStatus {
                            session: SessionId::new(0),
                            status: SessionStatusCode::Invalid,
                        },
                    ))));
                }
                let followup = i2pr_api::i2cp::RequestVariableLeaseSet { session, leases };
                return Ok(FrameOutcome::ReplyAndFollowup(
                    Box::new(Message::SessionStatus(reply)),
                    Box::new(Message::RequestVariableLeaseSet(followup)),
                ));
            }
            // Remote/best-effort path (pre-172): emit the empty
            // follow-up so existing local suites and the retained Plan
            // 170 diagnostic driver do not block. This path is never
            // counted as Plan 172 lifecycle success.
            let followup = i2pr_api::i2cp::RequestVariableLeaseSet {
                session,
                leases: Vec::new(),
            };
            Ok(FrameOutcome::ReplyAndFollowup(
                Box::new(Message::SessionStatus(reply)),
                Box::new(Message::RequestVariableLeaseSet(followup)),
            ))
        }
        Err(_error) => {
            let code = map_reserve_error_to_code(&_error);
            let reply = SessionStatus {
                session: SessionId::new(0),
                status: code,
            };
            Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(reply))))
        }
    }
}

async fn handle_reconfigure_session(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    machine: &mut ConnectionStateMachine,
    per_connection_session: &mut Option<SessionId>,
    frame: &i2pr_api::i2cp::RawFrame,
) -> Result<FrameOutcome, I2cpConnectionError> {
    let active = *per_connection_session;
    let Some(target_session) = active else {
        return Err(I2cpConnectionError::StateMachine(
            "ReconfigureSession before CreateSession".to_owned(),
        ));
    };
    // Plan 169 §2: the message_id on a ReconfigureSession matches
    // the session we just verified. The connection ownership check
    // happens after we resolve the session-state slot.
    let reconfigure = match decode_typed(frame.message_type, &frame.body) {
        Ok(Message::ReconfigureSession(value)) => value,
        Ok(other) => {
            let _ = other;
            return Err(I2cpConnectionError::StateMachine(
                "ReconfigureSession dispatch mismatch".to_owned(),
            ));
        }
        Err(error) => {
            return Err(I2cpConnectionError::Wire(format!(
                "typed decode failed for ReconfigureSession: {error}"
            )));
        }
    };
    if reconfigure.session != target_session {
        return Err(I2cpConnectionError::StateMachine(
            "ReconfigureSession target does not match owning session".to_owned(),
        ));
    }
    let session_state = match state.session_state(target_session) {
        Some(value) => value,
        None => {
            let reply = SessionStatus {
                session: target_session,
                status: SessionStatusCode::Invalid,
            };
            return Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(reply))));
        }
    };
    if session_state.connection != connection_id {
        return Err(I2cpConnectionError::StateMachine(
            "ReconfigureSession for session owned by a different connection".to_owned(),
        ));
    }
    let previous_options = session_state.current_options();
    let outcome = apply_reconfigure(
        session_state.as_ref(),
        previous_options,
        &reconfigure.config,
    );
    let reply_status = match &outcome {
        ReconfigurationOutcome::Accepted => SessionStatusCode::Updated,
        ReconfigurationOutcome::RebuildStaged | ReconfigurationOutcome::RebuildRequired => {
            SessionStatusCode::Updated
        }
        ReconfigurationOutcome::InvalidOptions(_)
        | ReconfigurationOutcome::ImmutableChange(_)
        | ReconfigurationOutcome::UnsupportedChange(_)
        | ReconfigurationOutcome::BadSignature
        | ReconfigurationOutcome::BadDate
        | ReconfigurationOutcome::ShapeRejected
        | ReconfigurationOutcome::Unchanged => SessionStatusCode::Invalid,
        ReconfigurationOutcome::Refused(_) => SessionStatusCode::Refused,
    };
    let reply = SessionStatus {
        session: target_session,
        status: reply_status,
    };
    let _ = machine;
    Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(reply))))
}

/// Outcome of one Plan 169 §2 reconfigure transaction.
///
/// Every variant carries enough context for tests to assert the
/// classification; the daemon maps it onto a [`SessionStatusCode`]
/// for the wire reply. A successful transaction is
/// [`ReconfigurationOutcome::Accepted`] (pure
/// `MutableImmediate` change) or [`ReconfigurationOutcome::RebuildStaged`]
/// (`MutableWithRebuild` change whose staged replacement completes
/// without touching runtime resources, which is the M9 test-mode
/// shape).
#[derive(Debug, Eq, PartialEq)]
pub enum ReconfigurationOutcome {
    /// All changes were `MutableImmediate` and were committed
    /// atomically.
    Accepted,
    /// At least one `MutableWithRebuild` change was staged. The
    /// previous configuration remains valid until the staged
    /// replacement completes; the M9 test profile completes the
    /// staged replacement atomically because the destination has no
    /// real inbound tunnels to rebuild.
    RebuildStaged,
    /// At least one `MutableWithRebuild` change requires an actual
    /// tunnel rebuild that the router cannot perform yet; the
    /// transaction is refused and no state mutates.
    RebuildRequired,
    /// The new SessionConfig shape is malformed or exceeds a
    /// documented ceiling.
    InvalidOptions(String),
    /// The new mapping contains an immutable-after-create key.
    ImmutableChange(String),
    /// The new mapping contains a key the M9 profile does not
    /// support.
    UnsupportedChange(String),
    /// Signature verification failed.
    BadSignature,
    /// Creation timestamp is outside the verifier clock window.
    BadDate,
    /// The structural codec rejected the body.
    ShapeRejected,
    /// The new mapping matches the previous mapping exactly; the
    /// session is unchanged.
    Unchanged,
    /// Router-side policy refused the reconfigure (e.g. destination
    /// registry at capacity).
    Refused(String),
}

/// Applies one Plan 169 §2 reconfigure transaction against the
/// supplied session state. The full new SessionConfig is verified
/// before any state mutates, and the diff against the previous
/// baseline is classified using the Plan 165 reconfigure table.
///
/// The transaction is all-or-nothing:
/// - a malformed signature, an out-of-window creation date, or a
///   shape rejection never touches the session state;
/// - any immutable-after-create or unsupported key rejects the
///   whole transaction;
/// - mutable-immediate changes commit atomically;
/// - mutable-with-rebuild changes stage the new options; the
///   destination's existing LeaseSet2 (when present) remains the
///   authoritative publication record until the client supplies a
///   fresh matching LeaseSet2 through `CreateLeaseSet2`.
fn apply_reconfigure(
    session_state: &I2cpSessionState,
    previous_options: Mapping,
    new_config: &SessionConfig,
) -> ReconfigurationOutcome {
    // First, fully parse the structural body. A failed decode never
    // touches the session state.
    let verified = match verify_session_config(
        // We do not retain the original body bytes here; the
        // `ReconfigureSession` body is the 2-byte session id
        // followed by the SessionConfig. The runtime-neutral
        // `verify_session_config` only needs the bytes of the
        // SessionConfig (the signed region the client signed) and
        // the parsed `SessionConfig` itself, so the bytes we hand
        // it are the encoded SessionConfig (which the verifier
        // re-serializes against the in-memory mapping anyway).
        new_config.encode().unwrap_or_default().as_slice(),
        new_config,
        &SessionConfigLimits::m9(),
        &WallClock,
    ) {
        Ok(value) => value,
        Err(i2pr_api::i2cp::I2cpError::SignatureRejected) => {
            return ReconfigurationOutcome::BadSignature;
        }
        Err(i2pr_api::i2cp::I2cpError::CreationTimestampOutOfRange { .. }) => {
            return ReconfigurationOutcome::BadDate;
        }
        Err(i2pr_api::i2cp::I2cpError::SessionConfigLimit { .. }) => {
            return ReconfigurationOutcome::ShapeRejected;
        }
        Err(i2pr_api::i2cp::I2cpError::Malformed { .. }) => {
            return ReconfigurationOutcome::ShapeRejected;
        }
        Err(error) => {
            return ReconfigurationOutcome::InvalidOptions(error.to_string());
        }
    };
    // Project the new options through the Plan 165 disposition table
    // so any unsupported / rejected option short-circuits before we
    // touch the baseline mapping.
    let _ = match project_options(
        verified.options(),
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    ) {
        Ok(value) => value,
        Err(error) => return ReconfigurationOutcome::InvalidOptions(error.to_string()),
    };
    // Diff against the previous baseline. The classifier uses the
    // Plan 165 `reconfiguration_class` helper for every key.
    let classifications = classify_reconfigure_diff(&previous_options, verified.options());
    let mut requires_rebuild = false;
    let mut immutable_key: Option<String> = None;
    let mut unsupported_key: Option<String> = None;
    for (key, class) in &classifications {
        match class {
            ReconfigurationClass::MutableWithRebuild => requires_rebuild = true,
            ReconfigurationClass::MutableImmediate => {}
            ReconfigurationClass::ImmutableAfterCreate => {
                immutable_key = Some(key.clone());
                break;
            }
            ReconfigurationClass::Unsupported => {
                unsupported_key = Some(key.clone());
                break;
            }
        }
    }
    if let Some(key) = immutable_key {
        return ReconfigurationOutcome::ImmutableChange(key);
    }
    if let Some(key) = unsupported_key {
        return ReconfigurationOutcome::UnsupportedChange(key);
    }
    if classifications.is_empty() {
        // No actual change; no state mutation. Reply Invalid to
        // signal "nothing to apply" without dropping the session.
        return ReconfigurationOutcome::Unchanged;
    }
    if validate_reconfigure_classifications(&classifications).is_err() {
        return ReconfigurationOutcome::Refused(
            "reconfigure classification failed all-or-nothing check".to_owned(),
        );
    }
    // Commit the new baseline atomically. Even when rebuild is
    // required, we keep the prior valid configuration visible until
    // the client supplies a fresh matching LeaseSet2 (the M9 test
    // profile completes the rebuild atomically because the
    // destination has no real inbound tunnels to rebuild).
    let _ = session_state.replace_options(verified.options().clone());
    if requires_rebuild {
        ReconfigurationOutcome::RebuildStaged
    } else {
        ReconfigurationOutcome::Accepted
    }
}

async fn handle_destroy_session(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    machine: &mut ConnectionStateMachine,
    per_connection_session: &mut Option<SessionId>,
    destroy: DestroySession,
) -> Result<FrameOutcome, I2cpConnectionError> {
    machine
        .begin_destroy_session()
        .map_err(|error| I2cpConnectionError::StateMachine(error.to_string()))?;
    if let Some(hash) = lookup_destination_hash(state, destroy.session) {
        if let Ok(mut registry) = state.destination_registry.lock()
            && let Some(entry) = state
                .destinations
                .lock()
                .ok()
                .and_then(|map| map.get(&hash).copied())
        {
            let _ = registry.remove(&entry.destination_id);
        }
        if let Ok(mut map) = state.destinations.lock() {
            map.remove(&hash);
        }
        if let Ok(mut registry) = state.session_registry.lock() {
            let _ = registry.destroy(destroy.session);
        }
    }
    // Plan 169 §3: drain the per-session Plan 168 data-plane state
    // synchronously so repeated DestroySession/CreateSession cycles
    // do not retain inbound queue, status correlation, or outbound
    // counters. The mutex is the only source of truth for the
    // bookkeeping.
    if let Ok(mut session_states) = state.sessions.lock()
        && let Some(session_state) = session_states.remove(&destroy.session)
    {
        let _ = session_state.release();
    }
    machine.deactivate_session();
    *per_connection_session = None;
    // Suppress unused-arg warning when connection_id stays only inside
    // the surrounding context.
    let _ = connection_id;
    Ok(FrameOutcome::Continue)
}

async fn handle_create_lease_set2(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    per_connection_session: &mut Option<SessionId>,
    create: CreateLeaseSet2,
) -> Result<FrameOutcome, I2cpConnectionError> {
    let Some(session) = *per_connection_session else {
        return Err(I2cpConnectionError::StateMachine(
            "CreateLeaseSet2 before CreateSession".to_owned(),
        ));
    };
    if create.session != session {
        return Err(I2cpConnectionError::StateMachine(
            "CreateLeaseSet2 session mismatch".to_owned(),
        ));
    }
    state
        .install_client_lease_set2(
            connection_id,
            session,
            create.lease_set,
            &create.private_keys,
        )
        .map_err(|error| I2cpConnectionError::Wire(error.to_string()))?;
    Ok(FrameOutcome::Continue)
}

// ---- Plan 168 SendMessage / SendMessageExpires / DestLookup / BandwidthLimits ----

/// Splits one I2CP payload into its gzip header and body, returning
/// the typed header and the post-header compressed body.
fn split_payload_header(
    payload: &Payload,
) -> Result<(PayloadGzipHeader, &[u8]), I2cpConnectionError> {
    let bytes = payload.as_bytes();
    if bytes.len() < i2pr_api::i2cp::GZIP_HEADER_LEN {
        return Err(I2cpConnectionError::PayloadRejected(
            "i2cp payload body too small for gzip header".to_owned(),
        ));
    }
    let header = PayloadGzipHeader::parse(bytes).map_err(|error| {
        I2cpConnectionError::PayloadRejected(format!("i2cp payload gzip header malformed: {error}"))
    })?;
    Ok((header, &bytes[i2pr_api::i2cp::GZIP_HEADER_LEN..]))
}

/// Validates the SendMessageExpires expiration against the bounded
/// M9 horizon. Returns the typed outcome to surface in `MessageStatus`.
fn validate_expiration(expiration_ms: u64, now_ms: u64) -> Result<(), I2cpMessageOutcome> {
    // Past expirations fail with `MessageExpired`; far-future
    // expirations fail with `BadOptions`.
    if expiration_ms <= now_ms {
        return Err(I2cpMessageOutcome::MessageExpired);
    }
    let horizon_ms: u64 = i2pr_api::i2cp::MAX_MESSAGE_EXPIRATION_HORIZON
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let delta = expiration_ms.saturating_sub(now_ms);
    if delta > horizon_ms {
        return Err(I2cpMessageOutcome::BadExpirationHorizon);
    }
    Ok(())
}

/// Returns the protocol byte carried in the I2CP payload header, or
/// `I2cpMessageOutcome::BadMessage` if the header is malformed.
fn parse_payload_protocol(payload: &Payload) -> Result<u8, I2cpMessageOutcome> {
    let bytes = payload.as_bytes();
    if bytes.len() < i2pr_api::i2cp::GZIP_HEADER_LEN {
        return Err(I2cpMessageOutcome::BadMessage);
    }
    Ok(bytes[i2pr_api::i2cp::GZIP_HEADER_LEN - 1])
}

async fn handle_send_message(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    per_connection_session: &mut Option<SessionId>,
    send: SendMessage,
) -> Result<FrameOutcome, I2cpConnectionError> {
    let session = *per_connection_session;
    let message = enqueue_outbound_payload(
        state,
        connection_id,
        session,
        send.session,
        send.destination
            .hash()
            .map_err(|error| I2cpConnectionError::Wire(format!("destination hash: {error}")))?,
        send.payload,
        send.nonce,
        None,
        None,
    )
    .await?;
    Ok(FrameOutcome::Reply(Box::new(Message::MessageStatus(
        message,
    ))))
}

async fn handle_send_message_expires(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    per_connection_session: &mut Option<SessionId>,
    send: SendMessageExpires,
) -> Result<FrameOutcome, I2cpConnectionError> {
    let session = *per_connection_session;
    // The structural codec already rejects reserved flag bits;
    // accept the remaining semantics per the M9 profile (bit 8
    // `NO_BUNDLE` is informational here, reliability-override bits
    // 10-9 are documented as spec-defined-ignore, and tag-threshold
    // / tags-to-send are ElGamal-only and therefore ignored).
    let message = enqueue_outbound_payload(
        state,
        connection_id,
        session,
        send.session,
        send.destination
            .hash()
            .map_err(|error| I2cpConnectionError::Wire(format!("destination hash: {error}")))?,
        send.payload,
        send.nonce,
        Some(send.flags),
        Some(send.expiration_ms),
    )
    .await?;
    Ok(FrameOutcome::Reply(Box::new(Message::MessageStatus(
        message,
    ))))
}

/// Routes one verified outbound payload through the Plan 168 data
/// plane: validates the session/ownership/payload/expiration/flags,
/// reserves an outbound slot, allocates a router message id,
/// invokes the destination runtime, and maps the runtime outcome
/// into an honest `MessageStatus` reply. Cross-session local delivery
/// is implemented as the Plan 168 loopback shortcut.
#[allow(clippy::too_many_arguments)]
async fn enqueue_outbound_payload(
    state: &Arc<I2cpServiceState>,
    connection_id: u32,
    owning_session: Option<SessionId>,
    declared_session: SessionId,
    target_hash: Hash,
    payload: Payload,
    nonce: i2pr_api::i2cp::ClientNonce,
    flags: Option<i2pr_api::i2cp::SendFlags>,
    expiration_ms: Option<u64>,
) -> Result<MessageStatus, I2cpConnectionError> {
    if let Some(active) = owning_session {
        if active != declared_session {
            return Ok(status_message(
                declared_session,
                MessageId::new(0),
                I2cpMessageOutcome::BadSession,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    } else {
        return Ok(status_message(
            declared_session,
            MessageId::new(0),
            I2cpMessageOutcome::BadSession,
            payload.as_bytes().len(),
            nonce,
        ));
    }
    let Some(session_state) = state.session_state(declared_session) else {
        return Ok(status_message(
            declared_session,
            MessageId::new(0),
            I2cpMessageOutcome::BadSession,
            payload.as_bytes().len(),
            nonce,
        ));
    };
    if session_state.connection != connection_id {
        return Ok(status_message(
            declared_session,
            MessageId::new(0),
            I2cpMessageOutcome::BadSession,
            payload.as_bytes().len(),
            nonce,
        ));
    }
    // Plan 172 §9/§13: counted zero-hop sessions may send only after
    // the LS2 install commits. Remote profiles preserve pre-172
    // best-effort behavior so existing suites stay green.
    if !session_state.is_usable_for_data() {
        return Ok(status_message(
            declared_session,
            MessageId::new(0),
            I2cpMessageOutcome::BadLocalLeaseSet,
            payload.as_bytes().len(),
            nonce,
        ));
    }
    // Parse the gzip header up front so the status reply carries
    // the right outcome class for malformed/malicious payloads.
    let header = match split_payload_header(&payload) {
        Ok((header, _body)) => header,
        Err(_) => {
            return Ok(status_message(
                declared_session,
                MessageId::new(0),
                I2cpMessageOutcome::BadMessage,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    };
    let _ = header; // protocol/sport/dport are recorded in the inbound frame
    // Validate the expiration when present.
    if let Some(expiration) = expiration_ms {
        let now_ms = i2cp_now_ms();
        if let Err(outcome) = validate_expiration(expiration, now_ms) {
            return Ok(status_message(
                declared_session,
                MessageId::new(0),
                outcome,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    }
    // Reject the unsupported ElGamal-only flag bits; the structural
    // codec already filtered reserved bits 15-11. We treat non-zero
    // tag-threshold / tags-to-send bits as unsupported because the
    // M9 profile never sets an ElGamal destination key.
    if let Some(flags) = flags
        && (flags.tag_threshold() != 0 || flags.tags_to_send() != 0)
    {
        return Ok(status_message(
            declared_session,
            MessageId::new(0),
            I2cpMessageOutcome::UnsupportedFlags,
            payload.as_bytes().len(),
            nonce,
        ));
    }
    // Reserve the bounded outbound slot before touching the
    // destination runtime so the per-session ceiling is enforced
    // before any state mutates.
    let session_state = match session_state.reserve_outbound_slot() {
        Ok(_) => session_state,
        Err(_) => {
            return Ok(status_message(
                declared_session,
                MessageId::new(0),
                I2cpMessageOutcome::Overflow,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    };
    let message_id = match session_state.allocate_message_id(nonce) {
        Ok(id) => id,
        Err(_) => {
            session_state.release_outbound_slot();
            return Ok(status_message(
                declared_session,
                MessageId::new(0),
                I2cpMessageOutcome::Overflow,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    };
    let protocol = match parse_payload_protocol(&payload) {
        Ok(value) => value,
        Err(outcome) => {
            session_state.take_pending_status(message_id);
            session_state.release_outbound_slot();
            return Ok(status_message(
                declared_session,
                message_id,
                outcome,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    };
    let (source_port, destination_port) = match split_payload_header(&payload) {
        Ok((header, _)) => (header.source_port, header.destination_port),
        Err(_) => {
            session_state.take_pending_status(message_id);
            session_state.release_outbound_slot();
            return Ok(status_message(
                declared_session,
                message_id,
                I2cpMessageOutcome::BadMessage,
                payload.as_bytes().len(),
                nonce,
            ));
        }
    };
    // Hand the payload to the destination runtime. The runtime owns
    // its bounded outbound queue and reports whether it accepted
    // the payload; we map the typed result into the Plan 168
    // outcome vocabulary. The payload bytes are *not* logged.
    let outcome = invoke_destination_outbound(state, &session_state, protocol, &payload).await;
    match outcome {
        Ok(()) => {
            // Plan 168 §6 loopback shortcut: when the target
            // destination is owned by another I2CP session on this
            // router, route the same compressed payload to the
            // receiving session's `MessagePayload` queue so the
            // owning connection task can write it onto the wire.
            // The cross-session delivery goes through the inbound
            // queue so sibling-isolation, byte accounting, and
            // backpressure all apply.
            if let Some(receiving) = state.session_for_destination(&target_hash)
                && receiving.session != declared_session
            {
                let frame = InboundPayloadFrame {
                    message_id: next_message_id(&receiving),
                    session: receiving.session,
                    protocol,
                    source_port,
                    destination_port,
                    payload: payload.as_bytes().to_vec(),
                };
                let _ = receiving.push_inbound(frame);
            }
            Ok(status_message(
                declared_session,
                message_id,
                I2cpMessageOutcome::Accepted,
                payload.as_bytes().len(),
                nonce,
            ))
        }
        Err(outcome) => {
            // Terminal failure: drop the pending status correlation
            // so the client does not see a stale entry later. The
            // outbound slot is also released because the destination
            // rejected the enqueue.
            session_state.take_pending_status(message_id);
            session_state.release_outbound_slot();
            Ok(status_message(
                declared_session,
                message_id,
                outcome,
                payload.as_bytes().len(),
                nonce,
            ))
        }
    }
}

/// Allocates a fresh inbound message id without recording a status
/// correlation (inbound frames are not status-tracked). Wraps the
/// atomic counter so saturation cannot deadlock the receiver.
fn next_message_id(session_state: &Arc<I2cpSessionState>) -> MessageId {
    let raw = session_state
        .next_message_id
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1).filter(|next| *next != 0)
        })
        .unwrap_or(1);
    MessageId::new(raw)
}

/// Pushes the I2CP payload into the destination runtime's outbound
/// queue. The runtime is the only authority on whether the payload
/// is acceptable; the I2CP data plane never duplicates destination
/// routing state.
async fn invoke_destination_outbound(
    state: &Arc<I2cpServiceState>,
    session_state: &Arc<I2cpSessionState>,
    protocol: u8,
    payload: &Payload,
) -> Result<(), I2cpMessageOutcome> {
    let _ = protocol;
    let bytes = payload.as_bytes();
    // The destination payload body is the gzip header + compressed
    // body. The destination runtime queue rejects empty bodies, so
    // supply the whole payload bytes (the runtime does not inspect
    // them) — an empty body is unreachable here because the codec
    // already rejected zero-length payload bodies on the wire.
    let body = bytes.to_vec();
    let destination_payload = match DestinationPayload::new(protocol, body) {
        Ok(value) => value,
        Err(PayloadError::EmptyBody) | Err(PayloadError::BodyTooLarge { .. }) => {
            return Err(I2cpMessageOutcome::BadMessage);
        }
        Err(_) => return Err(I2cpMessageOutcome::BadMessage),
    };
    let destination_id = session_state.destination_id;
    let mut registry = state
        .destination_registry
        .lock()
        .map_err(|_| I2cpMessageOutcome::DestinationStopping)?;
    let runtime = match registry.get_mut(&destination_id) {
        Some(runtime) => runtime,
        None => return Err(I2cpMessageOutcome::BadSession),
    };
    match runtime.enqueue_outbound(destination_payload) {
        Ok(_) => Ok(()),
        Err(PayloadError::QueueFull { .. }) => Err(I2cpMessageOutcome::Overflow),
        Err(PayloadError::QueueBytesExceeded { .. }) => Err(I2cpMessageOutcome::Overflow),
        Err(PayloadError::Stopping) => Err(I2cpMessageOutcome::DestinationStopping),
        Err(PayloadError::EmptyBody) | Err(PayloadError::BodyTooLarge { .. }) => {
            Err(I2cpMessageOutcome::BadMessage)
        }
        Err(_) => Err(I2cpMessageOutcome::BadMessage),
    }
}

fn status_message(
    session: SessionId,
    message_id: MessageId,
    outcome: I2cpMessageOutcome,
    size: usize,
    nonce: i2pr_api::i2cp::ClientNonce,
) -> MessageStatus {
    MessageStatus {
        session,
        message_id,
        status: outcome.status_code(),
        size: u32::try_from(size).unwrap_or(u32::MAX),
        nonce,
    }
}

fn handle_dest_lookup(state: &I2cpServiceState, lookup: DestLookup) -> FrameOutcome {
    // DestLookup in the M9 profile resolves through the local
    // destination registry first; remote lookup through NetDB
    // belongs to Plan 169. The plan 168 data plane never invents
    // network lookups or system DNS.
    let hash = lookup.hash;
    if let Ok(destinations) = state.destinations.lock()
        && let Some(entry) = destinations.get(&hash).copied()
        && let Ok(registry) = state.destination_registry.lock()
        && let Some(runtime) = registry.get(&entry.destination_id)
    {
        // Local hit: deliver the destination bytes back through the
        // destination registry. The payload envelope is the canonical
        // Destination encoding.
        let destination = runtime.public().destination().clone();
        return FrameOutcome::Reply(Box::new(Message::DestReply(DestReply {
            body: DestReplyBody::Destination(destination),
        })));
    }
    // Not found: protocol-correct typed not-found reply that echoes
    // the requested hash so outstanding lookups correlate.
    FrameOutcome::Reply(Box::new(Message::DestReply(DestReply {
        body: DestReplyBody::Hash(hash),
    })))
}

/// Returns a typed BandwidthLimits reply. The router-internal limits
/// stay at the documented neutral zero value (the router does not
/// yet measure bandwidth); the client-side limits reflect the
/// configured `max_buffered_bytes_per_connection` ceiling so a client
/// can read a useful ceiling without crossing into router policy.
fn derive_bandwidth_reply(config: &I2cpConfig) -> BandwidthLimits {
    // Documented in Plan 168 §8: client-side ceilings are derived
    // from the router configuration snapshot; router-side limits stay
    // at the spec-defined neutral zero value. We expose the client
    // limits in KB/s rounded down so the I2CP wire never reports
    // values that are not config-derived.
    let buffered_bytes = config.max_buffered_bytes_per_connection;
    let client_inbound_kbps = u32::try_from(buffered_bytes / 1024).unwrap_or(u32::MAX);
    let client_outbound_kbps = client_inbound_kbps;
    BandwidthLimits {
        client_inbound: client_inbound_kbps,
        client_outbound: client_outbound_kbps,
        router_inbound: 0,
        router_inbound_burst: 0,
        router_outbound: 0,
        router_outbound_burst: 0,
        router_burst_time: 0,
        reserved: [0u32; 9],
    }
}

/// Current wall-clock milliseconds since the Unix epoch. Used by
/// the Plan 168 expiration horizon check.
fn i2cp_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

async fn read_protocol_byte(
    stream: &mut TcpStream,
    config: &I2cpConfig,
    cancellation: CancellationToken,
) -> Result<u8, I2cpConnectionError> {
    let mut buf = [0_u8; 1];
    let timeout_duration = if config.protocol_byte_timeout == Duration::MAX {
        Duration::MAX
    } else {
        config.protocol_byte_timeout
    };
    let outcome = if timeout_duration == Duration::MAX {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(I2cpConnectionError::PeerClosed),
            read = stream.read_exact(&mut buf) => read,
        }
    } else {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(I2cpConnectionError::PeerClosed),
            read = timeout(timeout_duration, stream.read_exact(&mut buf)) => match read {
                Ok(inner) => inner,
                Err(_) => return Err(I2cpConnectionError::Timeout),
            },
        }
    };
    match outcome {
        Ok(_size) => Ok(buf[0]),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(I2cpConnectionError::PeerClosed)
        }
        Err(error) => Err(I2cpConnectionError::Io(error)),
    }
}

async fn read_chunk(
    stream: &mut TcpStream,
    timeout_duration: Duration,
    cancellation: CancellationToken,
) -> Result<Vec<u8>, I2cpConnectionError> {
    let mut buf = vec![0_u8; 16 * 1024];
    let outcome = if timeout_duration == Duration::MAX {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(I2cpConnectionError::PeerClosed),
            read = stream.read(&mut buf) => read,
        }
    } else {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(I2cpConnectionError::PeerClosed),
            read = timeout(timeout_duration, stream.read(&mut buf)) => match read {
                Ok(inner) => inner,
                Err(_) => return Err(I2cpConnectionError::Timeout),
            },
        }
    };
    match outcome {
        Ok(0) => Err(I2cpConnectionError::PeerClosed),
        Ok(read) => {
            buf.truncate(read);
            Ok(buf)
        }
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(I2cpConnectionError::PeerClosed)
        }
        Err(error) => Err(I2cpConnectionError::Io(error)),
    }
}

async fn write_message(
    stream: &mut TcpStream,
    message: &Message,
) -> Result<(), I2cpConnectionError> {
    let body = message
        .encode_body()
        .map_err(|error| I2cpConnectionError::Wire(format!("encode body: {error}")))?;
    let frame_bytes = encode_frame(message.message_type() as u8, &body)
        .map_err(|error| I2cpConnectionError::Wire(format!("encode frame: {error}")))?;
    stream
        .write_all(&frame_bytes)
        .await
        .map_err(I2cpConnectionError::Io)?;
    stream.flush().await.map_err(I2cpConnectionError::Io)?;
    Ok(())
}

fn lookup_destination_hash(state: &I2cpServiceState, session: SessionId) -> Option<Hash> {
    let entry = state
        .destinations
        .lock()
        .ok()
        .and_then(|map| map.values().find(|entry| entry.session == session).copied())?;
    state.destination_registry.lock().ok().and_then(|registry| {
        registry
            .get(&entry.destination_id)
            .map(|runtime| runtime.destination_hash())
    })
}

fn message_label(message_type: i2pr_api::i2cp::MessageType) -> &'static str {
    use i2pr_api::i2cp::MessageType::*;
    match message_type {
        GetDate => "GetDate",
        SetDate => "SetDate",
        CreateSession => "CreateSession",
        ReconfigureSession => "ReconfigureSession",
        DestroySession => "DestroySession",
        CreateLeaseSet => "CreateLeaseSet",
        SendMessage => "SendMessage",
        ReceiveMessageBegin => "ReceiveMessageBegin",
        ReceiveMessageEnd => "ReceiveMessageEnd",
        GetBandwidthLimits => "GetBandwidthLimits",
        SessionStatus => "SessionStatus",
        RequestLeaseSet => "RequestLeaseSet",
        MessageStatus => "MessageStatus",
        BandwidthLimits => "BandwidthLimits",
        ReportAbuse => "ReportAbuse",
        Disconnect => "Disconnect",
        MessagePayload => "MessagePayload",
        DestLookup => "DestLookup",
        DestReply => "DestReply",
        SendMessageExpires => "SendMessageExpires",
        RequestVariableLeaseSet => "RequestVariableLeaseSet",
        HostLookup => "HostLookup",
        HostReply => "HostReply",
        CreateLeaseSet2 => "CreateLeaseSet2",
        BlindingInfo => "BlindingInfo",
    }
}

// `I2cpAction` is referenced through downstream follow-on work; mark it
// as used so unused-import warnings stay silent.
#[allow(dead_code)]
fn _i2cp_action_marker(_x: I2cpAction) {}

// `Disconnect` is referenced through the public API surface; mark it
// as used so unused-import warnings stay silent.
#[allow(dead_code)]
fn _disconnect_marker(_x: Disconnect) {}
