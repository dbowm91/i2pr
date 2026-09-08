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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_api::i2cp::{
    BandwidthLimits, Clock, ConnectionStateMachine, CreateLeaseSet2, DestroySession, Disconnect,
    FrameDecoder, GetDate, I2cpAction, Message, PROTOCOL_BYTE, ProjectedPolicy, SessionId,
    SessionRegistry, SessionRegistryLimits, SessionStatus, SessionStatusCode,
    VerifiedSessionConfig, decode_typed, encode_frame, project_options, verify_session_config,
};
use i2pr_api::i2cp::{
    DestReply, DestReplyBody, HostReply, HostReplyResult, SessionConfig, SessionConfigLimits,
};
use i2pr_client::{
    DestinationConfig, DestinationId, DestinationPublic, DestinationRegistry, DestinationRuntime,
    InboundDecryptionCapability, RegistryConfig,
};
use i2pr_crypto::X25519_KEY_LENGTH;
use i2pr_proto::Hash;
use i2pr_runtime::{CancellationToken, ChildScope};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OwnedSemaphorePermit;
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

/// The complete state the I2CP service exposes to the daemon supervisor.
#[derive(Debug)]
pub struct I2cpServiceState {
    config: I2cpConfig,
    session_registry: Mutex<SessionRegistry>,
    destination_registry: Arc<Mutex<DestinationRegistry>>,
    destinations: Mutex<HashMap<Hash, I2cpDestinationEntry>>,
    next_connection: AtomicU64,
    active_connections: Mutex<HashMap<u32, I2cpConnection>>,
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
        Ok(Self {
            config,
            session_registry,
            destination_registry,
            destinations: Mutex::new(HashMap::new()),
            next_connection: AtomicU64::new(1),
            active_connections: Mutex::new(HashMap::new()),
        })
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
/// Plan 167 only needs the destination to be registered so that
/// CreateLeaseSet2 has a runtime to install into; we deliberately defer
/// the full option projection into the destination configuration to
/// Plan 169.
fn projected_to_config(_projected: ProjectedPolicy) -> DestinationConfig {
    DestinationConfig::balanced()
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
                        FrameOutcome::Close => return Ok(()),
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum FrameOutcome {
    Continue,
    Reply(Box<Message>),
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
        Message::ReconfigureSession(_reconfigure) => {
            handle_reconfigure_session(state, connection_id, machine, per_connection_session).await
        }
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
            Message::BandwidthLimits(zero_bandwidth_limits()),
        ))),
        Message::DestLookup(_lookup) => Ok(FrameOutcome::Reply(Box::new(Message::DestReply(
            DestReply {
                body: DestReplyBody::LegacyEmpty,
            },
        )))),
        Message::HostLookup(_lookup) => Ok(FrameOutcome::Reply(Box::new(Message::HostReply(
            HostReply {
                session: SessionId::new(0),
                request_id: i2pr_api::i2cp::HostRequestId::new(0),
                result: HostReplyResult::Failure,
                destination: None,
                options: None,
            },
        )))),
        Message::SendMessage(_) | Message::SendMessageExpires(_) => {
            Err(I2cpConnectionError::StateMachine(format!(
                "message type {} deferred to Plan 168",
                message_label(message.message_type())
            )))
        }
    }
}

fn handle_get_date(
    machine: &mut ConnectionStateMachine,
    get_date: GetDate,
) -> Result<FrameOutcome, I2cpConnectionError> {
    let set_date = machine
        .handle_get_date(&get_date)
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
            Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(reply))))
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
    _state: &Arc<I2cpServiceState>,
    _connection_id: u32,
    _machine: &mut ConnectionStateMachine,
    per_connection_session: &mut Option<SessionId>,
) -> Result<FrameOutcome, I2cpConnectionError> {
    if per_connection_session.is_none() {
        return Err(I2cpConnectionError::StateMachine(
            "ReconfigureSession before CreateSession".to_owned(),
        ));
    }
    let reply = SessionStatus {
        session: per_connection_session.unwrap_or_else(|| SessionId::new(0)),
        status: SessionStatusCode::Refused,
    };
    Ok(FrameOutcome::Reply(Box::new(Message::SessionStatus(reply))))
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

fn zero_bandwidth_limits() -> BandwidthLimits {
    BandwidthLimits {
        client_inbound: 0,
        client_outbound: 0,
        router_inbound: 0,
        router_inbound_burst: 0,
        router_outbound: 0,
        router_outbound_burst: 0,
        router_burst_time: 0,
        reserved: [0u32; 9],
    }
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
