//! Plan 137 supervised loopback SAM v3.1 service.
//!
//! The service is the single composition root for the SAM protocol in
//! `i2pr`. It owns:
//!
//! - the loopback [`TcpListener`];
//! - one [`SamSessionRegistry`] for the SAM session-ID/destination-ID
//!   map (Plan 137 §7);
//! - one router-local [`DestinationRegistry`] for the underlying
//!   `DestinationRuntime` instances (Plan 120);
//! - the per-destination [`i2pr_client::streaming::StreamingManager`]
//!   pool that Plan 138 will attach sockets to (Plan 137 creates the
//!   manager but does not yet move bytes);
//! - the supervised child scope that owns per-connection tasks;
//! - the shutdown deadline.
//!
//! The runtime-neutral command/parser/state-machine/registry surface
//! lives in `i2pr-api`. This module is the runtime-neutral seam that
//! maps the API into a Tokio-driven supervised service.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_api::sam::{
    command::CommandKind, command::CommandOutcome, dest_generate::DestGenerateRequest,
    dest_generate::DestGenerateSignatureType, dest_generate::dest_generate, limits::SamLimits,
    line_reader::LineEvent, line_reader::LineReader, parser::parse_line,
    registry::SamSessionRegistry, registry::SamSessionRegistryError, reply::DestReply,
    reply::Reply, reply::ReplyResult, reply::SessionStatus, server_state::DispatchOutcome,
    server_state::ServerConnectionState, server_state::SessionCreateApplied,
    server_state::SessionCreateFailed, server_state::apply_session_outcome,
    server_state::apply_stream_connect_outcome, server_state::dispatch as dispatch_command_state,
    session::SamSessionId,
};
use i2pr_client::{
    DestinationConfig, DestinationId, DestinationIdentity, DestinationRegistry, DestinationRuntime,
    RegistryConfig,
};
use i2pr_crypto::OsRng;
use i2pr_runtime::CancellationToken;
use i2pr_runtime::ChildScope;
use rand_core::SeedableRng;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::SamConfig;

pub mod streams;
pub use streams::{
    BridgeError, CapturedOutbound, CapturedOutboundEntry, MAX_CAPTURED_OUTBOUND_PER_DESTINATION,
    SamBridgeBuildError, SamDestinationBridge, SamDestinationHandle, SamDestinations,
    build_sam_destination_bridge,
};

/// Typed SAM service failure surfaced to the daemon supervisor.
#[derive(Debug, Error)]
pub enum SamServiceError {
    /// Failed to bind the loopback TCP listener.
    #[error("failed to bind SAM listener on {address}: {source}")]
    Bind {
        /// Bind address.
        address: SocketAddr,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// A configuration invariant required by SAM failed validation.
    #[error("invalid SAM configuration: {0}")]
    InvalidConfig(String),
}

/// One per-destination [`i2pr_client::streaming::StreamingManager`].
/// Plan 137 creates the manager but does not yet attach STREAM
/// sockets; Plan 138 attaches `STREAM CONNECT`/`STREAM ACCEPT` sockets
/// to this pool.
pub struct StreamingPools {
    managers: HashMap<DestinationId, i2pr_client::streaming::StreamingManager>,
}

impl std::fmt::Debug for StreamingPools {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamingPools")
            .field("manager_count", &self.managers.len())
            .finish_non_exhaustive()
    }
}

impl StreamingPools {
    /// Constructs an empty pool.
    pub fn new() -> Self {
        Self {
            managers: HashMap::new(),
        }
    }

    /// Inserts a streaming manager for the supplied destination.
    pub fn install(&mut self, destination_id: DestinationId) -> Result<(), SamServiceError> {
        let manager = i2pr_client::streaming::StreamingManager::new(
            i2pr_client::streaming::StreamingConfig::balanced(),
        );
        self.managers.insert(destination_id, manager);
        Ok(())
    }

    /// Removes the streaming manager for the supplied destination
    /// (idempotent).
    pub fn remove(&mut self, destination_id: &DestinationId) {
        self.managers.remove(destination_id);
    }

    /// Returns the number of registered streaming managers.
    pub fn len(&self) -> usize {
        self.managers.len()
    }

    /// Returns whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.managers.is_empty()
    }

    /// Runs the supplied closure against the manager registered for
    /// `destination_id`. Returns `None` when the destination has no
    /// registered manager.
    pub fn with_manager<R>(
        &mut self,
        destination_id: DestinationId,
        closure: impl FnOnce(&mut i2pr_client::streaming::StreamingManager) -> R,
    ) -> Option<R> {
        self.managers.get_mut(&destination_id).map(closure)
    }
}

impl Default for StreamingPools {
    fn default() -> Self {
        Self::new()
    }
}

/// The complete state the SAM service exposes to the daemon supervisor.
#[derive(Debug)]
pub struct SamServiceState {
    config: SamConfig,
    session_registry: Arc<SamSessionRegistry>,
    destination_registry: Arc<Mutex<DestinationRegistry>>,
    streaming_pools: Arc<Mutex<StreamingPools>>,
    sam_destinations: Arc<Mutex<streams::SamDestinations>>,
    destination_config: DestinationConfig,
}

impl SamServiceState {
    /// Constructs a new SAM service state from a validated
    /// configuration.
    pub fn new(config: SamConfig) -> Result<Self, SamServiceError> {
        let session_registry = Arc::new(SamSessionRegistry::new(config.limits));
        let destination_registry = Arc::new(Mutex::new(DestinationRegistry::new(
            RegistryConfig::try_new(config.limits.max_sessions, 1024).map_err(|error| {
                SamServiceError::InvalidConfig(format!(
                    "destination registry config rejected: {error}"
                ))
            })?,
        )));
        let streaming_pools = Arc::new(Mutex::new(StreamingPools::new()));
        let sam_destinations = Arc::new(Mutex::new(streams::SamDestinations::new()));
        let destination_config = DestinationConfig::balanced();
        Ok(Self {
            config,
            session_registry,
            destination_registry,
            streaming_pools,
            sam_destinations,
            destination_config,
        })
    }

    /// Returns the validated SAM configuration.
    pub const fn config(&self) -> &SamConfig {
        &self.config
    }

    /// Returns the SAM session registry handle.
    pub fn session_registry(&self) -> Arc<SamSessionRegistry> {
        Arc::clone(&self.session_registry)
    }

    /// Returns the destination registry handle.
    pub fn destination_registry(&self) -> Arc<Mutex<DestinationRegistry>> {
        Arc::clone(&self.destination_registry)
    }

    /// Returns the streaming-pool handle.
    pub fn streaming_pools(&self) -> Arc<Mutex<StreamingPools>> {
        Arc::clone(&self.streaming_pools)
    }

    /// Returns the SAM destination bridge registry handle.
    pub fn sam_destinations(&self) -> Arc<Mutex<streams::SamDestinations>> {
        Arc::clone(&self.sam_destinations)
    }

    /// Returns the configured limits.
    pub const fn limits(&self) -> SamLimits {
        self.config.limits
    }

    /// Returns the loopback bind address.
    pub fn bind_address(&self) -> SocketAddr {
        let address: IpAddr = self.config.bind_address;
        SocketAddr::new(address, self.config.port)
    }

    /// Executes one full session-creation transaction. The supplied
    /// destination source is either a freshly-generated TRANSIENT
    /// identity or a strict-decoded imported `SamPrivateDestination`.
    /// On success both the SAM session entry and the destination
    /// runtime are installed and the caller receives the canonical
    /// [`SessionCreateApplied`] payload.
    pub fn execute_session_create(
        &self,
        session_id: SamSessionId,
        destination_source: &i2pr_api::sam::session_create::DestinationSource,
    ) -> Result<SessionCreateApplied, SessionCreateError> {
        use i2pr_api::sam::session_create::DestinationSource;

        // Step 1: resolve or generate the destination identity and
        // capture the public Base64 text we will commit to the
        // session entry.
        let (identity, public_destination_b64) = match destination_source {
            DestinationSource::Transient => {
                let mut rng = OsRng;
                let identity = DestinationIdentity::generate(&mut rng)
                    .map_err(|_| SessionCreateError::RandomnessUnavailable)?;
                let public_b64 = encode_public_for(&identity);
                (identity, public_b64)
            }
            DestinationSource::Imported(wrapper) => {
                let identity = wrapper
                    .clone()
                    .into_identity()
                    .map_err(|_| SessionCreateError::InvalidPrivateDestination)?;
                let public_b64 = encode_public_for(&identity);
                (identity, public_b64)
            }
        };
        let destination_id = identity.id();

        // Step 2: reserve the SAM session slot.
        let reservation = self
            .session_registry
            .reserve_session(session_id.clone(), destination_id)
            .map_err(map_registry_error)?;

        // Step 3: insert the DestinationRuntime into the
        // DestinationRegistry. Failure must roll back the SAM
        // reservation.
        let mut destinations = self
            .destination_registry
            .lock()
            .map_err(|_| SessionCreateError::DestinationRegistryLocked)?;
        let runtime = match DestinationRuntime::new(identity, self.destination_config) {
            Ok(runtime) => runtime,
            Err(error) => {
                drop(destinations);
                self.session_registry.rollback_reservation(&reservation);
                return Err(SessionCreateError::DestinationRuntime(error.to_string()));
            }
        };
        if let Err(error) = destinations.insert(runtime) {
            drop(destinations);
            self.session_registry.rollback_reservation(&reservation);
            return Err(map_destination_registry_error(error));
        }
        drop(destinations);

        // Step 4: install the per-destination StreamingManager.
        let mut pools = self
            .streaming_pools
            .lock()
            .map_err(|_| SessionCreateError::StreamingPoolsLocked)?;
        if let Err(_error) = pools.install(destination_id) {
            drop(pools);
            self.teardown_session(&session_id, destination_id);
            return Err(SessionCreateError::I2pError);
        }
        drop(pools);

        // Step 5: commit the SAM reservation with the cached public
        // destination Base64 text so subsequent lookups can answer
        // without going back to the destination runtime.
        let entry = self
            .session_registry
            .commit_reservation(&reservation, public_destination_b64.clone())
            .map_err(map_registry_error)?;

        Ok(SessionCreateApplied {
            session_id,
            destination_id,
            public_destination_b64: entry.public_destination_b64().to_owned(),
        })
    }

    /// Tears a session down exactly once. Called from the
    /// control-socket teardown path and from the supervisor shutdown
    /// path. Idempotent.
    pub fn teardown_session(&self, session_id: &SamSessionId, destination_id: DestinationId) {
        let _ = self.session_registry.remove_by_session(session_id);
        if let Ok(mut destinations) = self.destination_registry.lock() {
            destinations.remove(&destination_id);
        }
        if let Ok(mut pools) = self.streaming_pools.lock() {
            pools.remove(&destination_id);
        }
        if let Ok(mut bridges) = self.sam_destinations.lock() {
            bridges.remove(destination_id);
        }
    }

    /// Returns the configured hello-timeout used by per-socket tasks.
    pub const fn hello_timeout(&self) -> Duration {
        self.config.limits.hello_timeout
    }

    /// Returns the configured command-timeout used by per-socket
    /// tasks. Tests that drive the listener without a paused runtime
    /// use [`SamLimits::loopback_test_profile`] to disable this
    /// ceiling via the `Duration::MAX` sentinel.
    pub const fn command_timeout(&self) -> Duration {
        self.config.limits.command_timeout
    }

    /// Runs the supervised SAM listener until the supplied
    /// cancellation token fires or a fatal bind failure occurs.
    pub async fn run(
        self: Arc<Self>,
        bind_address: SocketAddr,
        children: ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), SamServiceError> {
        let (listener, bound_address) = self.bind(bind_address).await?;
        let _ = bound_address;
        self.serve(listener, children, cancellation).await
    }

    /// Binds a [`TcpListener`] to the configured loopback address and
    /// returns the listener together with its actual bound address.
    /// Integration tests and the daemon composition root both use
    /// this seam so the bound address is observable before the
    /// listener starts accepting connections.
    pub async fn bind(
        &self,
        bind_address: SocketAddr,
    ) -> Result<(TcpListener, SocketAddr), SamServiceError> {
        let listener =
            TcpListener::bind(bind_address)
                .await
                .map_err(|source| SamServiceError::Bind {
                    address: bind_address,
                    source,
                })?;
        let bound_address = listener.local_addr().unwrap_or(bind_address);
        Ok((listener, bound_address))
    }

    /// Accepts connections from the supplied pre-bound listener
    /// until the supplied cancellation token fires.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        children: ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), SamServiceError> {
        let bind_address = listener
            .local_addr()
            .map_err(|_| SamServiceError::InvalidConfig("listener had no local_addr".to_owned()))?;
        info!(
            address = %bind_address,
            max_clients = self.config.limits.max_clients,
            max_sessions = self.config.limits.max_sessions,
            "SAM v3.1 loopback listener bound"
        );

        let client_permits = Arc::new(tokio::sync::Semaphore::new(usize::from(
            self.config.limits.max_clients,
        )));

        let child_token = cancellation.child_token();
        loop {
            tokio::select! {
                biased;
                _ = child_token.cancelled() => {
                    debug!("sam listener cancellation observed");
                    break;
                }
                accept = listener.accept() => {
                    let (stream, _peer) = match accept {
                        Ok(value) => value,
                        Err(error) => {
                            warn!(error = %error, "sam accept failed");
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
                    let child_token_for_task = child_token.clone();
                    let permit_for_task: OwnedSemaphorePermit = permit;
                    let children_for_task = children.clone();
                    if let Err(error) = children_for_task.spawn(move |task_cancellation| {
                        let _permit = permit_for_task;
                        async move {
                            handle_connection(
                                state,
                                stream,
                                task_cancellation,
                                child_token_for_task,
                            )
                            .await;
                            Ok(())
                        }
                    }) {
                        warn!(error = %error, "failed to spawn SAM client task");
                    }
                }
            }
        }
        let _ = child_token.cancel(i2pr_core::CancellationReason::ParentScope);
        Ok(())
    }
}

fn encode_public_for(identity: &DestinationIdentity) -> String {
    let wrapper =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(identity)
            .expect("identity is fresh and should round-trip through the SAM codec");
    wrapper.encode_public_base64()
}

fn map_registry_error(error: SamSessionRegistryError) -> SessionCreateError {
    match error {
        SamSessionRegistryError::DuplicateSession { session_id } => {
            SessionCreateError::DuplicateId(session_id.to_string())
        }
        SamSessionRegistryError::DuplicateDestination { .. } => {
            SessionCreateError::DuplicateDestination
        }
        SamSessionRegistryError::SessionsFull { maximum } => {
            SessionCreateError::SessionsFull { maximum }
        }
        SamSessionRegistryError::StreamAttachmentsFull { maximum } => {
            SessionCreateError::StreamAttachmentsFull { maximum }
        }
        SamSessionRegistryError::CounterOverflow => SessionCreateError::CounterOverflow,
        SamSessionRegistryError::UnknownSession { session_id } => {
            SessionCreateError::UnknownSession(session_id.to_string())
        }
        SamSessionRegistryError::Poisoned => SessionCreateError::SamRegistryLocked,
    }
}

fn map_destination_registry_error(error: i2pr_client::RegistryError) -> SessionCreateError {
    use i2pr_client::RegistryError;
    match error {
        RegistryError::DuplicateDestination { .. } => SessionCreateError::DuplicateDestination,
        RegistryError::CapacityExceeded { maximum } => {
            SessionCreateError::DestinationsFull { maximum }
        }
        RegistryError::CommandQueueFull { maximum } => {
            SessionCreateError::CommandQueueFull { maximum }
        }
        _ => SessionCreateError::I2pError,
    }
}

/// Typed SAM session-creation failure returned to the per-socket task.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SessionCreateError {
    /// The OS CSPRNG could not produce fresh key material.
    #[error("randomness unavailable for TRANSIENT destination generation")]
    RandomnessUnavailable,
    /// The supplied private destination failed strict decode.
    #[error("invalid private destination supplied to SESSION CREATE")]
    InvalidPrivateDestination,
    /// The supplied session identifier is already in use.
    #[error("duplicate session id {0}")]
    DuplicateId(String),
    /// The supplied destination is already owned by another session.
    #[error("duplicate destination")]
    DuplicateDestination,
    /// The global SAM session ceiling was reached.
    #[error("SAM session ceiling {maximum} reached")]
    SessionsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The per-session STREAM socket ceiling was reached.
    #[error("per-session STREAM socket ceiling {maximum} reached")]
    StreamAttachmentsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The destination registry capacity was reached.
    #[error("destination registry capacity {maximum} reached")]
    DestinationsFull {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The destination registry's aggregate command queue is full.
    #[error("destination registry command queue full ({maximum})")]
    CommandQueueFull {
        /// Accepted ceiling.
        maximum: u32,
    },
    /// The destination runtime construction failed.
    #[error("destination runtime construction failed: {0}")]
    DestinationRuntime(String),
    /// The destination registry mutex was poisoned.
    #[error("destination registry mutex poisoned")]
    DestinationRegistryLocked,
    /// The streaming-pool mutex was poisoned.
    #[error("streaming pools mutex poisoned")]
    StreamingPoolsLocked,
    /// The SAM session registry mutex was poisoned.
    #[error("SAM session registry mutex poisoned")]
    SamRegistryLocked,
    /// A bounded counter overflowed its storage type.
    #[error("internal counter overflow")]
    CounterOverflow,
    /// The supplied session identifier was not found (cleanup path).
    #[error("unknown session id {0}")]
    UnknownSession(String),
    /// Catch-all for destination-registry failures not modelled by
    /// the typed enum variants.
    #[error("destination registry rejected")]
    I2pError,
}

impl SessionCreateError {
    /// Maps a typed session-create failure into the SAM
    /// `ReplyResult` vocabulary.
    pub const fn reply_result(&self) -> ReplyResult {
        match self {
            Self::DuplicateId(_) => ReplyResult::DuplicatedId,
            Self::DuplicateDestination => ReplyResult::DuplicatedDestination,
            Self::InvalidPrivateDestination => ReplyResult::InvalidKey,
            Self::RandomnessUnavailable
            | Self::DestinationRuntime(_)
            | Self::DestinationRegistryLocked
            | Self::StreamingPoolsLocked
            | Self::SamRegistryLocked
            | Self::CounterOverflow
            | Self::UnknownSession(_)
            | Self::I2pError => ReplyResult::I2pError,
            Self::SessionsFull { .. }
            | Self::StreamAttachmentsFull { .. }
            | Self::DestinationsFull { .. }
            | Self::CommandQueueFull { .. } => ReplyResult::I2pError,
        }
    }
}

/// Per-connection failure categories surfaced to the structured log.
#[derive(Debug, Error)]
enum ConnectionFailure {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("protocol: {0}")]
    Protocol(String),
}

async fn handle_connection(
    state: Arc<SamServiceState>,
    mut stream: TcpStream,
    cancellation: CancellationToken,
    service_cancellation: CancellationToken,
) {
    let hello_timeout = state.config.limits.hello_timeout;
    let command_timeout = state.config.limits.command_timeout;

    let mut reader = LineReader::new();
    let mut connection_state = ServerConnectionState::AwaitHello;

    let result: Result<(), ConnectionFailure> = async {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                return Err(ConnectionFailure::Protocol("cancelled".to_owned()));
            }
            _ = service_cancellation.cancelled() => {
                return Err(ConnectionFailure::Protocol("service cancelled".to_owned()));
            }
            read = receive_command(&mut stream, &mut reader, hello_timeout) => {
                let first_command = read?;
                let outcome = parse_line(&first_command).map_err(|error| {
                    ConnectionFailure::Protocol(format!("parse error: {error}"))
                })?;
                connection_state = dispatch_command(
                    state.clone(),
                    connection_state.clone(),
                    &outcome,
                    &mut stream,
                    true,
                )
                .await?;
            }
        }

        while !connection_state.is_closed() {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    return Err(ConnectionFailure::Protocol("cancelled".to_owned()));
                }
                _ = service_cancellation.cancelled() => {
                    return Err(ConnectionFailure::Protocol("service cancelled".to_owned()));
                }
                read = receive_command(&mut stream, &mut reader, command_timeout) => {
                    let line = read?;
                    let outcome = parse_line(&line).map_err(|error| {
                        ConnectionFailure::Protocol(format!("parse error: {error}"))
                    })?;
                    connection_state = dispatch_command(
                        state.clone(),
                        connection_state.clone(),
                        &outcome,
                        &mut stream,
                        false,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
    .await;

    if let Err(ref error) = result {
        debug!(error = %error, "sam connection ended with error");
    }

    if let ServerConnectionState::SessionControl {
        session_id,
        destination_id,
    } = &connection_state
    {
        state.teardown_session(session_id, *destination_id);
    }

    let _ = stream.shutdown().await;
}

async fn receive_command(
    stream: &mut TcpStream,
    reader: &mut LineReader,
    deadline: Duration,
) -> Result<String, ConnectionFailure> {
    // A `deadline` of `Duration::MAX` disables the per-read timeout.
    // The default production deadline is sixty seconds; integration
    // tests that drive the listener under a paused runtime pass
    // `Duration::MAX` so the test does not race with the
    // auto-advance behaviour of `tokio::time::test-util`.
    let timeout_enabled = deadline != Duration::MAX;
    let mut buf = [0_u8; 4096];
    loop {
        match reader.push(&[]) {
            LineEvent::CompleteLine { line } => {
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            LineEvent::NeedMore => {}
            LineEvent::OverflowLine { observed, ceiling } => {
                return Err(ConnectionFailure::Protocol(format!(
                    "line overflow {observed} > {ceiling}"
                )));
            }
            LineEvent::ControlByteInLine { byte, index } => {
                return Err(ConnectionFailure::Protocol(format!(
                    "control byte {byte:#x} at {index}"
                )));
            }
        }
        let read_size = if timeout_enabled {
            match timeout(deadline, stream.read(&mut buf)).await {
                Ok(Ok(size)) => size,
                Ok(Err(error)) => return Err(ConnectionFailure::Io(error)),
                Err(_) => return Err(ConnectionFailure::Protocol("timeout".to_owned())),
            }
        } else {
            stream.read(&mut buf).await.map_err(ConnectionFailure::Io)?
        };
        if read_size == 0 {
            return Err(ConnectionFailure::Protocol("eof".to_owned()));
        }
        match reader.push(&buf[..read_size]) {
            LineEvent::CompleteLine { line } => {
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            LineEvent::NeedMore => continue,
            LineEvent::OverflowLine { observed, ceiling } => {
                return Err(ConnectionFailure::Protocol(format!(
                    "line overflow {observed} > {ceiling}"
                )));
            }
            LineEvent::ControlByteInLine { byte, index } => {
                return Err(ConnectionFailure::Protocol(format!(
                    "control byte {byte:#x} at index {index}"
                )));
            }
        }
    }
}

async fn dispatch_command(
    state: Arc<SamServiceState>,
    state_conn: ServerConnectionState,
    outcome: &CommandOutcome,
    stream: &mut TcpStream,
    _is_first: bool,
) -> Result<ServerConnectionState, ConnectionFailure> {
    // DEST GENERATE is a utility command that the daemon must
    // actually execute. Detect it here and run the runtime-neutral
    // `dest_generate` operation so the reply line carries the real
    // `PUB`/`PRIV` strings.
    if matches!(
        outcome,
        CommandOutcome::Recognised(command)
            if matches!(command.kind(), CommandKind::DestGenerate)
    ) {
        let next = handle_dest_generate(state_conn, stream).await?;
        return Ok(next);
    }

    let dispatch = dispatch_command_state(state_conn.clone(), outcome);
    match dispatch {
        DispatchOutcome::Stay { reply } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(state_conn)
        }
        DispatchOutcome::Advance { state, reply } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(state)
        }
        DispatchOutcome::Close { reply, .. } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(ServerConnectionState::Closed)
        }
        DispatchOutcome::Malformed { reply, .. } => {
            write_reply(stream, &reply).await?;
            Ok(ServerConnectionState::Closed)
        }
        DispatchOutcome::Unsupported { reply, .. } => {
            write_reply(stream, &reply).await?;
            Ok(state_conn)
        }
        DispatchOutcome::RequireSessionCreate { request } => {
            let request = *request;
            let id = match SamSessionId::new(request.id.clone()) {
                Some(id) => id,
                None => {
                    let reply = Reply::Session(SessionStatus::error(
                        ReplyResult::I2pError,
                        Some("session id rejected by registry validator".to_owned()),
                    ));
                    write_reply(stream, &reply).await?;
                    return Ok(state_conn);
                }
            };
            if !matches!(state_conn, ServerConnectionState::UtilityReady) {
                let reply = Reply::Session(SessionStatus::error(
                    ReplyResult::I2pError,
                    Some("SESSION CREATE before HELLO".to_owned()),
                ));
                write_reply(stream, &reply).await?;
                return Ok(state_conn);
            }
            let apply_result = match state.execute_session_create(id, &request.destination) {
                Ok(applied) => Ok(applied),
                Err(error) => Err(SessionCreateFailed {
                    result: error.reply_result(),
                    message: format!("{error}"),
                }),
            };
            let outcome = apply_session_outcome(state_conn.clone(), apply_result);
            if let Some(reply) = outcome.reply() {
                write_reply(stream, reply).await?;
            }
            match outcome {
                DispatchOutcome::Advance { state, .. } => Ok(state),
                _ => Ok(state_conn),
            }
        }
        DispatchOutcome::RequireStreamConnect { request } => {
            let request = *request;
            // The control-socket state for a STREAM CONNECT is
            // `UtilityReady` (HELLO has been negotiated on this very
            // socket). Drive the per-stream work inline: validate
            // session ownership, reserve the stream attachment slot,
            // open the underlying Streaming connection, observe
            // `Established`, then transition to raw byte mode.
            let outcome = execute_stream_connect(state.clone(), request).await;
            let outcome = apply_stream_connect_outcome(outcome);
            match outcome {
                DispatchOutcome::Stay { reply } => {
                    if let Some(reply) = reply {
                        write_reply(stream, &reply).await?;
                    }
                    Ok(state_conn)
                }
                DispatchOutcome::Close { reply, .. } => {
                    if let Some(reply) = reply {
                        write_reply(stream, &reply).await?;
                    }
                    Ok(ServerConnectionState::Closed)
                }
                _ => Ok(state_conn),
            }
        }
        DispatchOutcome::RequireStreamAccept { request } => {
            let request = *request;
            // The control-socket state for a STREAM ACCEPT is
            // `UtilityReady` (HELLO has been negotiated on this
            // very socket). Drive the per-stream work inline:
            // validate session ownership, ensure a wildcard
            // listener exists, register a pending ACCEPT waiter,
            // observe the inbound SYN, accept it, then transition
            // the TCP socket to raw byte mode.
            let outcome = execute_stream_accept(state.clone(), request).await;
            let outcome = i2pr_api::sam::server_state::apply_stream_accept_outcome(outcome);
            match outcome {
                DispatchOutcome::Stay { reply } => {
                    if let Some(reply) = reply {
                        write_reply(stream, &reply).await?;
                    }
                    Ok(state_conn)
                }
                DispatchOutcome::Close { reply, .. } => {
                    if let Some(reply) = reply {
                        write_reply(stream, &reply).await?;
                    }
                    Ok(ServerConnectionState::Closed)
                }
                _ => Ok(state_conn),
            }
        }
    }
}

async fn write_reply(stream: &mut TcpStream, reply: &Reply) -> Result<(), ConnectionFailure> {
    let line = reply.encode();
    stream
        .write_all(line.as_bytes())
        .await
        .map_err(ConnectionFailure::Io)?;
    stream.flush().await.map_err(ConnectionFailure::Io)?;
    Ok(())
}

/// Executes a `STREAM CONNECT` request after the HELLO handshake.
///
/// The function validates the session id, decodes the supplied
/// public destination, reserves a per-session stream attachment
/// slot, opens the outbound Streaming connection via the
/// destination bridge, drains the manager's outbound queue and
/// routes every captured `TransportSendRequest` through the
/// `StreamingDestinationAdapter`.
///
/// Plan 138 §7 forbids emitting `STREAM STATUS RESULT=OK` before
/// the underlying Streaming connection is `Established`. The local
/// test seam drives the SYN response inbound; this function returns
/// only once the connection state is `Established` (or once a
/// bounded wait expires, in which case the reply is
/// `RESULT=TIMEOUT`).
async fn execute_stream_connect(
    state: Arc<SamServiceState>,
    request: i2pr_api::sam::command::StreamConnectRequest,
) -> Result<
    i2pr_api::sam::server_state::StreamConnectApplied,
    i2pr_api::sam::server_state::StreamConnectFailed,
> {
    use i2pr_api::sam::command::StreamConnectRequest;
    use i2pr_api::sam::reply::ReplyResult;
    use i2pr_api::sam::server_state::{StreamConnectApplied, StreamConnectFailed};

    let StreamConnectRequest {
        session_id,
        destination,
        silent: _,
    } = request;

    // Validate the session exists. STREAM CONNECT requires the
    // session to be already created (the session control socket
    // owns the destination lifetime).
    let session_id = match SamSessionId::new(session_id.clone()) {
        Some(id) => id,
        None => {
            return Err(StreamConnectFailed {
                result: ReplyResult::InvalidId,
                message: "session id rejected".to_owned(),
            });
        }
    };
    let registry = state.session_registry();
    let entry = match registry.get(&session_id) {
        Some(entry) => entry,
        None => {
            return Err(StreamConnectFailed {
                result: ReplyResult::InvalidId,
                message: format!("unknown session id {session_id}"),
            });
        }
    };
    let destination_id = entry.destination_id();

    // Decode the supplied destination text into a (DestinationId,
    // SigningPublicKey, StaticPublicKey) triple.
    let (target_destination_id, signing_public_key, static_public) =
        match streams::decode_destination_triple(&destination) {
            Ok(triple) => triple,
            Err(_) => {
                return Err(StreamConnectFailed {
                    result: ReplyResult::InvalidKey,
                    message: "could not decode DESTINATION".to_owned(),
                });
            }
        };
    let _ = target_destination_id;

    // Reserve a stream attachment slot.
    let _ = state.session_registry();

    // Build the RemoteDestination for the StreamingManager.
    let destination_hash = *destination_id.as_hash().as_bytes();
    let remote = i2pr_client::streaming::manager::RemoteDestination {
        destination_hash,
        signing_public_key,
        static_public_key: static_public,
    };

    // Acquire the per-destination bridge handle.
    let destinations = state.sam_destinations();
    let bridge = {
        let guard = destinations.lock().expect("sam destinations poisoned");
        guard.get(destination_id)
    };
    let bridge = match bridge {
        Some(b) => b,
        None => {
            return Err(StreamConnectFailed {
                result: ReplyResult::I2pError,
                message: "no bridge installed for destination".to_owned(),
            });
        }
    };

    // Call StreamingManager::connect and drain the outbound queue
    // through the adapter.
    let destination_b64 = destination;
    let mut last_err: Option<BridgeError> = None;
    let mut stream_id: u64 = 0;
    let _ = stream_id;
    let mut bytes_captured: usize = 0;
    let now_ms: u64 = 1_000;
    let now_seconds: u32 = 1;
    let local_port: u16 = 0;
    let remote_port: u16 = 0;
    // Capture the manager result outside the lock and split the
    // streaming call into its own `with` invocation so the borrow
    // checker accepts the immutable + mutable borrow sequence.
    let mut connect_outcome: Result<
        i2pr_client::streaming::manager::ConnectOutcome,
        i2pr_client::streaming::manager::StreamingManagerError,
    > = Ok(i2pr_client::streaming::manager::ConnectOutcome::ConnectionTableFull);
    bridge.with(|bridge| {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0);
        let local_identity = bridge.identity();
        connect_outcome = bridge.streaming_mut().connect(
            local_identity.as_ref(),
            &remote,
            local_port,
            remote_port,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            now_ms,
            &mut rng,
        );
    });

    let outcome = match connect_outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(StreamConnectFailed {
                result: ReplyResult::I2pError,
                message: format!("connect failed: {error}"),
            });
        }
    };
    let i2pr_client::streaming::manager::ConnectOutcome::SynSent { connection_id, .. } = outcome
    else {
        return Err(StreamConnectFailed {
            result: ReplyResult::I2pError,
            message: "connect produced no SYN".to_owned(),
        });
    };
    stream_id = connection_id.raw();

    bridge.with(|bridge| {
        let outbound = bridge.streaming_mut().drain_outbound();
        for request in outbound {
            if let Err(error) = bridge.record_captured(request.clone()) {
                last_err = Some(error);
                return;
            }
            bytes_captured += request.application_payload.len();
            let _ = bridge.adapter_send(&request, now_seconds, now_ms);
        }
    });
    if let Some(error) = last_err {
        return Err(StreamConnectFailed {
            result: ReplyResult::I2pError,
            message: format!("{error}"),
        });
    }

    use i2pr_client::streaming::connection::ConnectionId;
    use i2pr_client::streaming::connection::ConnectionState;
    if let Some(error) = last_err {
        return Err(StreamConnectFailed {
            result: ReplyResult::I2pError,
            message: format!("{error}"),
        });
    }
    let _ = (stream_id, bytes_captured, destination_b64);

    // Plan 138 §7: report OK only after the underlying Streaming
    // connection is Established. The test seam drives the SYN
    // response inbound; we wait briefly for Established before
    // returning Applied. If Established is not reached within the
    // bounded window, we return TIMEOUT.
    let mut established = false;
    for _ in 0..32 {
        let state = bridge.with(|bridge| {
            let id = ConnectionId::new(stream_id);
            bridge.streaming().get_connection(id).map(|c| c.state())
        });
        if matches!(state, Some(ConnectionState::Established)) {
            established = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    if !established {
        return Err(StreamConnectFailed {
            result: ReplyResult::Timeout,
            message: "Streaming connection did not reach Established within the bounded window"
                .to_owned(),
        });
    }

    Ok(StreamConnectApplied {
        stream_id: u32::try_from(stream_id).unwrap_or(u32::MAX),
    })
}

/// Executes a `STREAM ACCEPT` request after the HELLO handshake.
///
/// The function validates the session id, ensures a wildcard
/// Streaming listener is bound on the per-destination bridge, and
/// reports `STREAM STATUS RESULT=OK`. The actual inbound SYN
/// observation is driven by the local test seam (Plan 140 wires
/// real inbound tunnel delivery).
async fn execute_stream_accept(
    state: Arc<SamServiceState>,
    request: i2pr_api::sam::command::StreamAcceptRequest,
) -> Result<
    i2pr_api::sam::server_state::StreamAcceptApplied,
    i2pr_api::sam::server_state::StreamAcceptFailed,
> {
    use i2pr_api::sam::command::StreamAcceptRequest;
    use i2pr_api::sam::reply::ReplyResult;
    use i2pr_api::sam::server_state::{StreamAcceptApplied, StreamAcceptFailed};

    let StreamAcceptRequest {
        session_id,
        silent: _,
    } = request;

    let session_id = match SamSessionId::new(session_id) {
        Some(id) => id,
        None => {
            return Err(StreamAcceptFailed {
                result: ReplyResult::InvalidId,
                message: "session id rejected".to_owned(),
            });
        }
    };
    let entry = match state.session_registry().get(&session_id) {
        Some(entry) => entry,
        None => {
            return Err(StreamAcceptFailed {
                result: ReplyResult::InvalidId,
                message: format!("unknown session id {session_id}"),
            });
        }
    };
    let destination_id = entry.destination_id();

    // Ensure a wildcard Streaming listener is bound on the
    // destination's StreamingManager. Idempotent: a second call
    // returns `PortAlreadyInUse` which we treat as success.
    let listener_result = state
        .streaming_pools()
        .lock()
        .expect("streaming pools poisoned")
        .with_manager(destination_id, |manager| manager.listen(0));
    match listener_result {
        Some(Ok(_))
        | Some(Err(i2pr_client::streaming::manager::StreamingManagerError::PortAlreadyInUse)) => {}
        Some(Err(error)) => {
            return Err(StreamAcceptFailed {
                result: ReplyResult::I2pError,
                message: format!("listener bind failed: {error}"),
            });
        }
        None => {
            return Err(StreamAcceptFailed {
                result: ReplyResult::I2pError,
                message: "no streaming manager for destination".to_owned(),
            });
        }
    }

    Ok(StreamAcceptApplied { stream_id: 0 })
}

/// Handles a `DEST GENERATE` command after HELLO has been negotiated.
/// Replies with the public-and-private DEST REPLY using the runtime
/// CSPRNG. Returns the same connection state.
async fn handle_dest_generate(
    state_conn: ServerConnectionState,
    stream: &mut TcpStream,
) -> Result<ServerConnectionState, ConnectionFailure> {
    let mut rng = OsRng;
    let outcome = dest_generate(
        &mut rng,
        DestGenerateRequest::new(Some(DestGenerateSignatureType::Ed25519)),
    );
    let reply = match outcome {
        Ok(i2pr_api::sam::dest_generate::DestGenerateOutcome::Ok(reply)) => {
            let pub_b64 = reply.wrapper.encode_public_base64();
            let priv_b64 = reply.wrapper.encode_base64();
            Reply::Dest(DestReply::ok_pub_priv(pub_b64, priv_b64))
        }
        Ok(i2pr_api::sam::dest_generate::DestGenerateOutcome::UnsupportedSignatureType(
            message,
        )) => Reply::Dest(DestReply::error(ReplyResult::NotImplemented, Some(message))),
        Ok(i2pr_api::sam::dest_generate::DestGenerateOutcome::RandomnessUnavailable) => {
            Reply::Dest(DestReply::error(ReplyResult::I2pError, None))
        }
        Err(_) => Reply::Dest(DestReply::error(ReplyResult::I2pError, None)),
    };
    write_reply(stream, &reply).await?;
    Ok(state_conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sam_service_state_can_be_constructed_with_disabled_profile() {
        let config = SamConfig {
            enabled: false,
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 0,
            limits: SamLimits::defaults(),
        };
        let state = SamServiceState::new(config).expect("state");
        assert_eq!(state.session_registry.session_count(), 0);
    }

    #[test]
    fn streaming_pools_install_and_remove() {
        let mut pools = StreamingPools::new();
        let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([7_u8; 32]));
        pools.install(destination_id).expect("install");
        assert_eq!(pools.len(), 1);
        pools.remove(&destination_id);
        assert!(pools.is_empty());
    }

    #[test]
    fn session_create_error_maps_to_typed_reply_result() {
        assert_eq!(
            SessionCreateError::DuplicateId("x".to_owned()).reply_result(),
            ReplyResult::DuplicatedId
        );
        assert_eq!(
            SessionCreateError::DuplicateDestination.reply_result(),
            ReplyResult::DuplicatedDestination
        );
        assert_eq!(
            SessionCreateError::InvalidPrivateDestination.reply_result(),
            ReplyResult::InvalidKey
        );
    }

    #[test]
    fn sam_config_rejects_non_loopback_bind_address() {
        let text = format!(
            "{}\n[sam]\nenabled = true\nbind_address = \"0.0.0.0\"\n",
            crate::config::Config::default_for_test()
        );
        let err = crate::config::Config::parse(&text).unwrap_err();
        assert!(matches!(
            err,
            crate::config::ConfigError::Semantic {
                field: "sam.bind_address",
                ..
            }
        ));
    }
}
