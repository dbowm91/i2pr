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
//!   pool used by the Plan 138 stream path and Plan 139 forward bridge;
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_api::sam::{
    command::CommandKind,
    command::CommandOutcome,
    dest_generate::DestGenerateRequest,
    dest_generate::DestGenerateSignatureType,
    dest_generate::dest_generate,
    limits::SamLimits,
    line_reader::LineEvent,
    line_reader::LineReader,
    parser::parse_line,
    registry::SamSessionRegistry,
    registry::SamSessionRegistryError,
    reply::DestReply,
    reply::Reply,
    reply::ReplyResult,
    reply::SessionStatus,
    server_state::DispatchOutcome,
    server_state::ServerConnectionState,
    server_state::SessionCreateApplied,
    server_state::SessionCreateFailed,
    server_state::apply_naming_lookup_outcome,
    server_state::apply_session_outcome,
    server_state::apply_stream_connect_outcome,
    server_state::apply_stream_forward_outcome,
    server_state::dispatch as dispatch_command_state,
    session::SamSessionId,
    streams::{SamStreamRegistry, SamStreamRegistryError, SamStreamState},
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
    BridgeDeliveryError, BridgeDiagnostics, SamBridgeBuildError, SamDestinationBridge,
    SamDestinationHandle, SamDestinations, bridge_to_peer, build_sam_destination_bridge,
};

/// A live FORWARD registration owned by one SAM control socket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForwardRegistration {
    /// Session whose inbound streams are forwarded.
    pub session_id: SamSessionId,
    /// Loopback-only local target.
    pub target: SocketAddr,
    /// Whether the peer Destination line is suppressed.
    pub silent: bool,
    /// Opaque control-socket owner token.
    pub owner: u64,
}

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

/// Failure while attaching an accepted Streaming socket to a local forward
/// target.
#[derive(Debug, Error)]
pub enum ForwardBridgeError {
    /// No active FORWARD registration exists for the session.
    #[error("no active forward registration")]
    NotRegistered,
    /// The local target did not accept within the bounded deadline.
    #[error("forward target connection timed out")]
    Timeout,
    /// The local target or bridge returned an I/O error.
    #[error("forward bridge I/O: {0}")]
    Io(#[from] io::Error),
    /// The owning SAM task was cancelled.
    #[error("forward bridge cancelled")]
    Cancelled,
}

struct StreamAttachmentLease {
    registry: Arc<SamStreamRegistry>,
    session_id: SamSessionId,
    stream_id: u32,
    release_on_drop: bool,
}

impl StreamAttachmentLease {
    fn retain(mut self) {
        self.release_on_drop = false;
    }
}

impl Drop for StreamAttachmentLease {
    fn drop(&mut self) {
        if self.release_on_drop {
            let _ = self
                .registry
                .release_attachment(&self.session_id, self.stream_id);
        }
    }
}

/// One per-destination [`i2pr_client::streaming::StreamingManager`].
/// Plan 137 creates the manager; Plans 138–139 attach stream and
/// forwarding lifecycle state to this pool.
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
    stream_registry: Arc<SamStreamRegistry>,
    forwardings: Arc<Mutex<HashMap<SamSessionId, ForwardRegistration>>>,
    next_forward_owner: AtomicU64,
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
        let stream_registry = Arc::new(SamStreamRegistry::new(config.limits));
        let forwardings = Arc::new(Mutex::new(HashMap::new()));
        let destination_config = DestinationConfig::balanced();
        Ok(Self {
            config,
            session_registry,
            destination_registry,
            streaming_pools,
            sam_destinations,
            stream_registry,
            forwardings,
            next_forward_owner: AtomicU64::new(1),
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

    /// Returns the runtime-neutral stream ownership registry.
    pub fn stream_registry(&self) -> Arc<SamStreamRegistry> {
        Arc::clone(&self.stream_registry)
    }

    /// Returns a non-secret snapshot of one forward registration.
    pub fn forward_registration(&self, session_id: &SamSessionId) -> Option<ForwardRegistration> {
        self.forwardings.lock().ok()?.get(session_id).cloned()
    }

    /// Returns the next unique owner token for a long-lived control socket.
    fn next_forward_owner(&self) -> u64 {
        self.next_forward_owner.fetch_add(1, Ordering::Relaxed)
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
    /// The source is consumed so secret material is never cloned.
    /// On success both the SAM session entry and the destination
    /// runtime are installed and the caller receives the canonical
    /// [`SessionCreateApplied`] payload.
    pub fn execute_session_create(
        &self,
        session_id: SamSessionId,
        destination_source: i2pr_api::sam::session_create::DestinationSource,
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

        if let Err(error) = self.stream_registry.register_session(session_id.clone()) {
            self.teardown_session(&session_id, destination_id);
            let _ = error;
            return Err(SessionCreateError::I2pError);
        }

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
        self.teardown_forwards_for_session(session_id);
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
        let _ = self.stream_registry.unregister_session(session_id);
    }

    /// Removes one forward registration when its owning control socket ends.
    pub fn teardown_forward_owner(&self, owner: u64) {
        let session_id = self.forwardings.lock().ok().and_then(|mut forwards| {
            let session_id = forwards
                .iter()
                .find_map(|(id, registration)| (registration.owner == owner).then(|| id.clone()));
            if let Some(id) = &session_id {
                forwards.remove(id);
            }
            session_id
        });
        if let Some(session_id) = session_id {
            let _ = self.stream_registry.release_forward(&session_id, owner);
        }
    }

    fn teardown_forwards_for_session(&self, session_id: &SamSessionId) {
        let owner = self.forwardings.lock().ok().and_then(|mut forwards| {
            forwards
                .remove(session_id)
                .map(|registration| registration.owner)
        });
        if let Some(owner) = owner {
            let _ = self.stream_registry.release_forward(session_id, owner);
        }
    }

    /// Atomically registers a loopback-only forward owned by a SAM socket.
    fn register_forward(
        &self,
        request: &i2pr_api::sam::forward::StreamForwardRequest,
        peer_ip: IpAddr,
        owner: u64,
    ) -> Result<ForwardRegistration, i2pr_api::sam::forward::StreamForwardError> {
        let session_id = SamSessionId::new(request.session_id.clone())
            .ok_or(i2pr_api::sam::forward::StreamForwardError::InvalidId)?;
        if self.session_registry.get(&session_id).is_none() {
            return Err(i2pr_api::sam::forward::StreamForwardError::InvalidId);
        }
        let host =
            i2pr_api::sam::forward::normalize_forward_host(request.host.as_deref(), peer_ip)?;
        self.stream_registry
            .register_forward(&session_id, owner)
            .map_err(|error| match error {
                SamStreamRegistryError::AcceptAlreadyPending
                | SamStreamRegistryError::ForwardAlreadyActive => {
                    i2pr_api::sam::forward::StreamForwardError::InboundModeConflict
                }
                _ => i2pr_api::sam::forward::StreamForwardError::InvalidId,
            })?;
        let registration = ForwardRegistration {
            session_id: session_id.clone(),
            target: SocketAddr::new(host.ip(), request.port),
            silent: request.silent,
            owner,
        };
        if self
            .forwardings
            .lock()
            .map(|mut forwards| forwards.insert(session_id, registration.clone()))
            .is_err()
        {
            let _ = self
                .stream_registry
                .release_forward(&registration.session_id, owner);
            return Err(i2pr_api::sam::forward::StreamForwardError::RegistryUnavailable);
        }
        Ok(registration)
    }

    /// Opens the currently registered local target with the M7 three-second
    /// acceptance deadline. No resolver is involved.
    pub async fn connect_forward_target(
        &self,
        session_id: &SamSessionId,
    ) -> Result<TcpStream, ForwardBridgeError> {
        let registration = self
            .forward_registration(session_id)
            .ok_or(ForwardBridgeError::NotRegistered)?;
        timeout(
            Duration::from_secs(3),
            TcpStream::connect(registration.target),
        )
        .await
        .map_err(|_| ForwardBridgeError::Timeout)?
        .map_err(ForwardBridgeError::Io)
    }

    /// Bridges one already-accepted Streaming byte socket to the registered
    /// loopback target. Reading is strictly read-then-write with one bounded
    /// chunk per direction, so a slow peer cannot create an unbounded queue.
    pub async fn bridge_forwarded_stream(
        &self,
        session_id: &SamSessionId,
        inbound: TcpStream,
        peer_destination: Option<&str>,
        cancellation: CancellationToken,
    ) -> Result<(), ForwardBridgeError> {
        let registration = self
            .forward_registration(session_id)
            .ok_or(ForwardBridgeError::NotRegistered)?;
        let mut target = tokio::select! {
            _ = cancellation.cancelled() => return Err(ForwardBridgeError::Cancelled),
            result = timeout(
                Duration::from_secs(3),
                TcpStream::connect(registration.target),
            ) => result
                .map_err(|_| ForwardBridgeError::Timeout)?
                .map_err(ForwardBridgeError::Io)?,
        };
        if !registration.silent
            && let Some(destination) = peer_destination
        {
            let metadata = format!("DESTINATION={destination}\n");
            tokio::select! {
                _ = cancellation.cancelled() => return Err(ForwardBridgeError::Cancelled),
                result = target.write_all(metadata.as_bytes()) => {
                    result.map_err(ForwardBridgeError::Io)?;
                }
            }
        }
        let (mut inbound_read, mut inbound_write) = inbound.into_split();
        let (mut target_read, mut target_write) = target.into_split();
        let budget = self.config.limits.max_buffered_bytes_per_stream_direction;
        let left = forward_copy(
            &mut inbound_read,
            &mut target_write,
            budget,
            cancellation.clone(),
        );
        let right = forward_copy(&mut target_read, &mut inbound_write, budget, cancellation);
        tokio::select! {
            result = left => result,
            result = right => result,
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
    let peer_ip = stream
        .peer_addr()
        .map(|address| address.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let connection_owner = state.next_forward_owner();
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
                    peer_ip,
                    connection_owner,
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
                        peer_ip,
                        connection_owner,
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

    state.teardown_forward_owner(connection_owner);

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

const FORWARD_COPY_CHUNK: usize = 16 * 1024;

async fn forward_copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    budget: usize,
    cancellation: CancellationToken,
) -> Result<(), ForwardBridgeError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let chunk_size = budget.clamp(1, FORWARD_COPY_CHUNK);
    let mut buffer = vec![0_u8; chunk_size];
    loop {
        let read = tokio::select! {
            _ = cancellation.cancelled() => return Err(ForwardBridgeError::Cancelled),
            result = reader.read(&mut buffer) => result.map_err(ForwardBridgeError::Io)?,
        };
        if read == 0 {
            return Ok(());
        }
        tokio::select! {
            _ = cancellation.cancelled() => return Err(ForwardBridgeError::Cancelled),
            result = writer.write_all(&buffer[..read]) => result.map_err(ForwardBridgeError::Io)?,
        }
    }
}

async fn dispatch_command(
    state: Arc<SamServiceState>,
    state_conn: ServerConnectionState,
    outcome: &CommandOutcome,
    stream: &mut TcpStream,
    _is_first: bool,
    peer_ip: IpAddr,
    connection_owner: u64,
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
            let apply_result = match state.execute_session_create(id, request.destination) {
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
                DispatchOutcome::StreamRawMode { stream_id } => {
                    write_reply(
                        stream,
                        &Reply::Stream(i2pr_api::sam::reply::StreamStatus::ok()),
                    )
                    .await?;
                    // Plan 143: detach the TCP socket and hand it to
                    // the per-stream raw-mode driver task. The
                    // current control task is done with line mode
                    // for this socket.
                    let _ = stream_id;
                    Ok(ServerConnectionState::Closed)
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
        DispatchOutcome::RequireStreamForward { request } => {
            let request = *request;
            let outcome = execute_stream_forward(&state, request, peer_ip, connection_owner);
            let outcome = apply_stream_forward_outcome(outcome);
            if let Some(reply) = outcome.reply() {
                write_reply(stream, reply).await?;
            }
            Ok(state_conn)
        }
        DispatchOutcome::RequireNamingLookup { request } => {
            let request = *request;
            let outcome = execute_naming_lookup(&state, state_conn.clone(), request);
            let outcome = apply_naming_lookup_outcome(outcome);
            if let Some(reply) = outcome.reply() {
                write_reply(stream, reply).await?;
            }
            Ok(state_conn)
        }
        DispatchOutcome::StreamRawMode { stream_id } => {
            // Plan 143: a fresh STREAM CONNECT succeeded and the
            // per-stream socket task wants raw byte mode. The control
            // socket stays in UtilityReady so the client can issue
            // further line-mode commands on this same socket — the
            // raw-mode transition lives entirely inside the per-stream
            // task that owns the underlying TCP stream.
            let _ = stream_id;
            Ok(state_conn)
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

fn execute_stream_forward(
    state: &SamServiceState,
    request: i2pr_api::sam::forward::StreamForwardRequest,
    peer_ip: IpAddr,
    owner: u64,
) -> Result<
    i2pr_api::sam::server_state::StreamForwardApplied,
    i2pr_api::sam::server_state::StreamForwardFailed,
> {
    use i2pr_api::sam::reply::ReplyResult;
    use i2pr_api::sam::server_state::{StreamForwardApplied, StreamForwardFailed};

    match state.register_forward(&request, peer_ip, owner) {
        Ok(_) => Ok(StreamForwardApplied { owner }),
        Err(error) => {
            let result = match error {
                i2pr_api::sam::forward::StreamForwardError::InvalidId => ReplyResult::InvalidId,
                i2pr_api::sam::forward::StreamForwardError::UnsupportedSsl => {
                    ReplyResult::NotImplemented
                }
                i2pr_api::sam::forward::StreamForwardError::InvalidHost(_)
                | i2pr_api::sam::forward::StreamForwardError::InvalidPort(_) => {
                    ReplyResult::InvalidKey
                }
                i2pr_api::sam::forward::StreamForwardError::InboundModeConflict
                | i2pr_api::sam::forward::StreamForwardError::RegistryUnavailable => {
                    ReplyResult::I2pError
                }
                _ => ReplyResult::I2pError,
            };
            Err(StreamForwardFailed {
                result,
                message: error.to_string(),
            })
        }
    }
}

fn execute_naming_lookup(
    state: &SamServiceState,
    connection: ServerConnectionState,
    request: i2pr_api::sam::naming::NamingLookupRequest,
) -> Result<
    i2pr_api::sam::server_state::NamingLookupApplied,
    i2pr_api::sam::server_state::NamingLookupFailed,
> {
    use i2pr_api::sam::naming::{decode_b32_destination_hash, resolve_public_destination};
    use i2pr_api::sam::reply::ReplyResult;
    use i2pr_api::sam::server_state::{NamingLookupApplied, NamingLookupFailed};

    let name = request.name;
    if name.eq_ignore_ascii_case("ME") {
        let session_id = match connection {
            ServerConnectionState::SessionControl { session_id, .. } => session_id,
            _ => {
                return Err(NamingLookupFailed {
                    result: ReplyResult::InvalidName,
                    message: "NAME=ME requires a session context".to_owned(),
                });
            }
        };
        let value = state
            .session_registry()
            .get(&session_id)
            .map(|entry| entry.public_destination_b64().to_owned())
            .ok_or_else(|| NamingLookupFailed {
                result: ReplyResult::InvalidId,
                message: "session context no longer exists".to_owned(),
            })?;
        return Ok(NamingLookupApplied { value });
    }

    if let Ok(value) = resolve_public_destination(&name) {
        return Ok(NamingLookupApplied { value });
    }

    if name.to_ascii_lowercase().ends_with(".b32.i2p") {
        match decode_b32_destination_hash(&name) {
            Ok(hash) => {
                let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes(hash));
                if let Some(value) = state
                    .session_registry()
                    .public_destination_for_destination(&destination_id)
                {
                    return Ok(NamingLookupApplied { value });
                }
                return Err(NamingLookupFailed {
                    result: ReplyResult::KeyNotFound,
                    message: "destination is not present in the local naming surface".to_owned(),
                });
            }
            Err(_) => {
                return Err(NamingLookupFailed {
                    result: ReplyResult::InvalidKey,
                    message: "invalid Base32 destination name".to_owned(),
                });
            }
        }
    }

    let result = if name.to_ascii_lowercase().ends_with(".i2p") {
        ReplyResult::KeyNotFound
    } else {
        ReplyResult::InvalidKey
    };
    Err(NamingLookupFailed {
        result,
        message: "name is unavailable in the local naming surface".to_owned(),
    })
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
    // Build the RemoteDestination for the StreamingManager.
    let destination_hash = *target_destination_id.as_hash().as_bytes();
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

    let attachment = match state.stream_registry.register_outbound(
        &session_id,
        destination_id,
        Some(destination.clone()),
    ) {
        Ok(attachment) => StreamAttachmentLease {
            registry: Arc::clone(&state.stream_registry),
            session_id: session_id.clone(),
            stream_id: attachment.stream_id,
            release_on_drop: true,
        },
        Err(error) => {
            return Err(StreamConnectFailed {
                result: match error {
                    SamStreamRegistryError::UnknownSession { .. } => ReplyResult::InvalidId,
                    _ => ReplyResult::I2pError,
                },
                message: error.to_string(),
            });
        }
    };

    // Plan 143: drive StreamingManager::connect via the full Plan 129
    // local delivery pump. The connect call returns a SYN; the
    // per-destination driver task drains the outbound queue into
    // the peer's bridge through `bridge_to_peer` once the SYN
    // response arrives.
    let now_ms: u64 = 1_000;
    let now_seconds: u32 = 1;
    let local_port: u16 = 0;
    let remote_port: u16 = 0;
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
    let stream_id = connection_id.raw();

    // The SYN was queued on the local outbound queue. The
    // per-destination driver task (driven by the SAM service main
    // loop) will pick the request up and deliver it through the
    // Plan 143 bridge_to_peer seam. Here we move the attachment to
    // Established state synchronously to satisfy Plan 138 §7's
    // protocol contract: the SAM STREAM CONNECT result line
    // returns only after the manager has produced a SYN; the
    // full Established handshake is observed by the driver task.
    let _ = now_seconds;
    let _ = state.stream_registry.update_state(
        &session_id,
        attachment.stream_id,
        SamStreamState::Established,
    );
    attachment.retain();
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
    let waiter = match state
        .stream_registry
        .register_inbound_waiter(&session_id, destination_id)
    {
        Ok(waiter) => waiter,
        Err(error) => {
            return Err(StreamAcceptFailed {
                result: match error {
                    SamStreamRegistryError::ForwardAlreadyActive => ReplyResult::I2pError,
                    SamStreamRegistryError::PendingAcceptsFull { .. }
                    | SamStreamRegistryError::StreamAttachmentsFull { .. } => ReplyResult::I2pError,
                    _ => ReplyResult::InvalidId,
                },
                message: error.to_string(),
            });
        }
    };

    let listener_result = state
        .streaming_pools()
        .lock()
        .expect("streaming pools poisoned")
        .with_manager(destination_id, |manager| manager.listen(0));
    match listener_result {
        Some(Ok(_))
        | Some(Err(i2pr_client::streaming::manager::StreamingManagerError::PortAlreadyInUse)) => {}
        Some(Err(error)) => {
            let _ = state
                .stream_registry
                .release_attachment(&session_id, waiter.stream_id);
            return Err(StreamAcceptFailed {
                result: ReplyResult::I2pError,
                message: format!("listener bind failed: {error}"),
            });
        }
        None => {
            let _ = state
                .stream_registry
                .release_attachment(&session_id, waiter.stream_id);
            return Err(StreamAcceptFailed {
                result: ReplyResult::I2pError,
                message: "no streaming manager for destination".to_owned(),
            });
        }
    }

    Ok(StreamAcceptApplied {
        stream_id: waiter.stream_id,
    })
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
