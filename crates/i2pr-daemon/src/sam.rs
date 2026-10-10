//! Plan 137 supervised SAM v3.1 service with loopback and private stream origins.
//!
//! The service is the single composition root for the SAM protocol in
//! `i2pr`. It owns:
//!
//! - the optional loopback [`TcpListener`] admission adapter;
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
//! maps the API into a Tokio-driven supervised service. Listener and trusted
//! private streams use the same connection driver; a private origin cannot
//! request host-target FORWARD.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    server_state::dispatch_with_server_max as dispatch_command_state,
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
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::SamConfig;

pub mod datagram_udp;
pub mod fabric;
pub mod faults;
pub mod raw_stream;
pub mod streams;
pub use fabric::{
    DeliverySweepCounters, LocalDeliveryDegradation, LocalDestinationProduct,
    LocalTransportRequest, SamLocalProductFabric, degrade_to_reason,
};
pub use faults::{FaultPacketClass, SamDeliveryFaultCounters, SamDeliveryFaultProfile};
pub(crate) use raw_stream::RawStreamCleanup;
pub use raw_stream::{
    RawDirection, RawStreamError, RawStreamHandoff, RawStreamHandoffResolved, RawStreamOutcome,
    SamAsyncStream, SamIoStream, run_raw_stream,
};
pub use streams::{
    BridgeDeliveryError, BridgeDiagnostics, InboundTunnelBuildError, InboundTunnelFactory,
    SamBridgeBuildError, SamDestinationBridge, SamDestinationHandle, SamDestinations,
    bridge_to_peer, build_sam_destination_bridge,
};

/// Returns the process-local monotonic clock used by the streaming
/// managers. Keeping this clock in the SAM composition root gives the
/// raw socket drivers and per-destination delivery drivers one
/// comparable timeline for send-window and delayed-ACK deadlines.
pub(crate) fn streaming_now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    Instant::now()
        .duration_since(start)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Returns the current protocol-time seconds used for LeaseSet2
/// validation and publication. This is deliberately separate from the
/// process-local monotonic Streaming clock.
pub(crate) fn sam_now_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u32::try_from(duration.as_secs()).ok())
        .unwrap_or(1)
}

/// Complete session-creation input for the SAM lifecycle owner.
pub(crate) struct SessionCreateExecution<'a> {
    pub session_id: SamSessionId,
    pub style: i2pr_api::sam::session_create::SessionCreateStyle,
    pub destination_source: i2pr_api::sam::session_create::DestinationSource,
    pub from_port: u16,
    pub to_port: u16,
    pub children: &'a ChildScope,
    pub cancellation: CancellationToken,
}

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
    /// Failed to bind a loopback SAM TCP or UDP listener.
    #[error("failed to bind SAM socket on {address}: {source}")]
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

/// Failure to register a supervised per-destination SAM driver.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum DestinationDriverSpawnError {
    /// A destination already owns its single driver.
    #[error("destination driver already running")]
    AlreadyRunning,
    /// The driver registry mutex was poisoned.
    #[error("destination driver registry mutex poisoned")]
    RegistryLocked,
    /// The child scope rejected the task.
    #[error("child scope rejected destination driver: {0}")]
    Scope(#[from] i2pr_runtime::ChildScopeError),
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
    max_supported_version: i2pr_api::sam::version::SamVersion,
    /// Bound loopback UDP bridge, installed by `bind` before service readiness.
    udp_socket: Mutex<Option<Arc<UdpSocket>>>,
    connection_permits: Arc<tokio::sync::Semaphore>,
    session_registry: Arc<SamSessionRegistry>,
    destination_registry: Arc<Mutex<DestinationRegistry>>,
    streaming_pools: Arc<Mutex<StreamingPools>>,
    sam_destinations: Arc<Mutex<streams::SamDestinations>>,
    stream_registry: Arc<SamStreamRegistry>,
    forwardings: Arc<Mutex<HashMap<SamSessionId, ForwardRegistration>>>,
    next_forward_owner: AtomicU64,
    destination_config: DestinationConfig,
    /// Plan 147 §8 step 5: per-destination outbound-signal notifications.
    /// `send_data_segment` and `deliver_outbound` notify the entry;
    /// the corresponding per-destination driver task wakes and drains
    /// the outbound queue, polls retransmits, and polls acks.
    outbound_notify: Arc<Mutex<HashMap<DestinationId, Arc<tokio::sync::Notify>>>>,
    /// Plan 147 §8 step 4: per-destination established-signal
    /// notifications. `execute_stream_connect` and
    /// `execute_stream_accept` `await` on the entry; the per-destination
    /// driver task notifies when it observes `ConnectionState::Established`.
    established_notify: Arc<Mutex<HashMap<DestinationId, Arc<tokio::sync::Notify>>>>,
    /// One explicit cancellation capability per live destination driver.
    /// Session teardown cancels it before removing the bridge.
    destination_drivers: Arc<Mutex<HashMap<DestinationId, CancellationToken>>>,
    /// Latest bounded delivery accounting for each live destination.
    delivery_counters: Arc<Mutex<HashMap<DestinationId, DeliverySweepCounters>>>,
    /// Plan 151 §8 deterministic pre-start delivery fault profile.
    /// Inert by default; tests install an armed profile before the
    /// listener starts serving. Consulted once per drained delivery
    /// sweep. Never retains payloads, identities, or keys.
    fault_profile: Arc<Mutex<SamDeliveryFaultProfile>>,
    /// Plan 294 canonical address-book resolver cell. Empty unless
    /// the composition root installs the active subsystem's shared
    /// handle; session-registry and Base32 paths always precede it.
    addressbook: Mutex<crate::addressbook::SharedAddressBook>,
}

impl SamServiceState {
    /// Constructs a new SAM service state from a validated
    /// configuration.
    pub fn new(config: SamConfig) -> Result<Self, SamServiceError> {
        Self::new_with_max_supported_version(config, i2pr_api::sam::version::MAX_SUPPORTED_VERSION)
    }

    pub(crate) fn new_with_max_supported_version(
        config: SamConfig,
        max_supported_version: i2pr_api::sam::version::SamVersion,
    ) -> Result<Self, SamServiceError> {
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
        let outbound_notify = Arc::new(Mutex::new(HashMap::new()));
        let established_notify = Arc::new(Mutex::new(HashMap::new()));
        let destination_drivers = Arc::new(Mutex::new(HashMap::new()));
        let delivery_counters = Arc::new(Mutex::new(HashMap::new()));
        let fault_profile = Arc::new(Mutex::new(SamDeliveryFaultProfile::disabled()));
        let connection_permits = Arc::new(tokio::sync::Semaphore::new(usize::from(
            config.limits.max_clients,
        )));
        Ok(Self {
            config,
            max_supported_version,
            udp_socket: Mutex::new(None),
            connection_permits,
            session_registry,
            destination_registry,
            streaming_pools,
            sam_destinations,
            stream_registry,
            forwardings,
            next_forward_owner: AtomicU64::new(1),
            destination_config,
            outbound_notify,
            established_notify,
            destination_drivers,
            delivery_counters,
            fault_profile,
            addressbook: Mutex::new(crate::addressbook::SharedAddressBook::new()),
        })
    }

    /// Installs the canonical address-book resolver cell (Plan 294).
    /// The installed clone shares one `Arc` with the subsystem, so
    /// later commits propagate without re-installation.
    pub fn set_addressbook_handle(&self, handle: crate::addressbook::SharedAddressBook) {
        if let Ok(mut slot) = self.addressbook.lock() {
            *slot = handle;
        }
    }

    /// Looks up one `.i2p` hostname in the canonical address book
    /// (`None` when the subsystem is inactive or the name is absent).
    pub fn addressbook_lookup(&self, name: &str) -> Option<i2pr_addressbook::ResolvedEntry> {
        self.addressbook
            .lock()
            .ok()
            .and_then(|slot| slot.lookup(name))
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

    /// Returns (or lazily creates) the outbound-signal
    /// [`tokio::sync::Notify`] associated with the supplied
    /// destination. The Plan 147 per-destination driver task
    /// `await`s on this handle; `send_data_segment` and
    /// `deliver_outbound` call [`Self::notify_outbound_signal`].
    pub fn outbound_signal(&self, destination_id: DestinationId) -> Arc<tokio::sync::Notify> {
        let mut map = self
            .outbound_notify
            .lock()
            .expect("outbound notify map poisoned");
        map.entry(destination_id)
            .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
            .clone()
    }

    /// Wakes any `await`er on the supplied destination's outbound
    /// signal. Idempotent; safe to call when no driver is registered.
    pub fn notify_outbound_signal(&self, destination_id: DestinationId) {
        let notify = self.outbound_signal(destination_id);
        notify.notify_one();
    }

    /// Returns (or lazily creates) the established-signal
    /// [`tokio::sync::Notify`] associated with the supplied
    /// destination. `execute_stream_connect` and
    /// `execute_stream_accept` `await` on this handle while polling
    /// for `ConnectionState::Established`; the per-destination driver
    /// task calls [`Self::notify_established_signal`] after observing
    /// the transition.
    pub fn established_signal(&self, destination_id: DestinationId) -> Arc<tokio::sync::Notify> {
        let mut map = self
            .established_notify
            .lock()
            .expect("established notify map poisoned");
        map.entry(destination_id)
            .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
            .clone()
    }

    /// Looks up a locally-owned peer destination by hash and returns
    /// its public SAM Base64 text. Used by `execute_stream_accept` to
    /// populate the non-silent ACCEPT peer-Destination line; the value
    /// always comes from the peer's `Arc<DestinationIdentity>`
    /// (Plan 149 §9) so the daemon never fabricates metadata from a
    /// request string or a test fixture.
    pub fn local_peer_public_destination(&self, peer_hash: &[u8; 32]) -> Option<String> {
        let handle = self
            .sam_destinations
            .lock()
            .expect("sam destinations poisoned")
            .lookup_by_peer_hash(peer_hash)?;
        let identity = handle.with(|bridge| bridge.identity());
        let wrapper = i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(
            identity.as_ref(),
        )
        .ok()?;
        Some(wrapper.encode_public_base64())
    }

    /// Wakes any `await`er on the supplied destination's established
    /// signal. Idempotent.
    pub fn notify_established_signal(&self, destination_id: DestinationId) {
        let notify = self.established_signal(destination_id);
        notify.notify_one();
    }

    /// Removes the per-destination outbound / established signal
    /// entries. Called from [`Self::teardown_session`].
    fn drop_destination_signals(&self, destination_id: DestinationId) {
        if let Ok(mut map) = self.outbound_notify.lock() {
            map.remove(&destination_id);
        }
        if let Ok(mut map) = self.established_notify.lock() {
            map.remove(&destination_id);
        }
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

    /// Returns the cumulative typed delivery-sweep counters for a
    /// destination. Sweeps accumulate with saturating arithmetic so
    /// one typed failure remains observable after later clean
    /// sweeps; the entry is removed on session teardown. Payloads
    /// and peer identities are never retained.
    pub fn delivery_counters(&self, destination_id: DestinationId) -> DeliverySweepCounters {
        self.delivery_counters
            .lock()
            .ok()
            .and_then(|counters| counters.get(&destination_id).copied())
            .unwrap_or_default()
    }

    fn record_delivery_counters(
        &self,
        destination_id: DestinationId,
        counters: DeliverySweepCounters,
    ) {
        if let Ok(mut entries) = self.delivery_counters.lock() {
            entries
                .entry(destination_id)
                .or_default()
                .saturating_add_assign(counters);
        }
    }

    /// Installs the Plan 151 §8 deterministic delivery fault profile.
    /// Test-only: call before the listener starts serving. After
    /// startup, behavior-driving interactions must remain SAM TCP
    /// only; tests read back [`Self::fault_counters`] to prove each
    /// armed fault actually fired.
    pub fn install_test_fault_profile(&self, profile: SamDeliveryFaultProfile) {
        if let Ok(mut current) = self.fault_profile.lock() {
            *current = profile;
        }
    }

    /// Returns the non-secret observed fault counters. Payloads and
    /// peer identities are never retained.
    pub fn fault_counters(&self) -> SamDeliveryFaultCounters {
        self.fault_profile
            .lock()
            .ok()
            .map(|profile| profile.counters())
            .unwrap_or_default()
    }

    /// Disarms the ceiling drop arm of the installed fault profile.
    /// Test-only: used by the retransmission-ceiling test before
    /// closing the stream so termination control flows normally.
    pub fn disarm_test_fault_ceiling(&self) {
        if let Ok(mut profile) = self.fault_profile.lock() {
            profile.disarm_drop_all_data_ack();
        }
    }

    /// Returns the fault-profile handle for the delivery path.
    pub(crate) fn fault_profile_handle(&self) -> Arc<Mutex<SamDeliveryFaultProfile>> {
        Arc::clone(&self.fault_profile)
    }

    /// Returns the loopback bind address.
    pub fn bind_address(&self) -> SocketAddr {
        let address: IpAddr = self.config.bind_address;
        SocketAddr::new(address, self.config.port)
    }

    /// Returns the bound SAM Datagram/Raw UDP address, if the listener has
    /// started binding. Primarily exposed so loopback integration clients can
    /// use an ephemeral test port.
    pub fn udp_bind_address(&self) -> Option<SocketAddr> {
        self.udp_socket.lock().ok()?.as_ref()?.local_addr().ok()
    }

    /// Executes one full session-creation transaction. The supplied
    /// destination source is either a freshly-generated TRANSIENT
    /// identity or a strict-decoded imported `SamPrivateDestination`.
    /// The source is consumed so secret material is never cloned.
    ///
    /// Plan 149 §5: a successful transaction self-composes the entire
    /// localhost STREAM product from protocol commands alone. By the
    /// time this returns `Ok`, the following have been installed
    /// before any caller sees the `SESSION STATUS RESULT=OK` line:
    ///
    /// 1. one private `DestinationIdentity` allocation wrapped in
    ///    `Arc<DestinationIdentity>` and shared with the
    ///    `DestinationRuntime`;
    /// 2. one validated signed `LeaseSet2` plus an outbound role and
    ///    per-destination inbound-tunnel factory from
    ///    [`SamLocalProductFabric`];
    /// 3. one `SamDestinationBridge` installed in the SAM bridge
    ///    registry;
    /// 4. one per-destination runtime driver task spawned under
    ///    `children` with `cancellation` as its parent.
    ///
    /// Every failure before the commit rolls the registries, the
    /// secret allocation, the product material, the bridge, the
    /// stream session, and (on driver-spawn failure) the bridge back
    /// to the pre-create baseline. The function never leaves a half-
    /// composed session.
    pub(crate) fn execute_session_create(
        self: &Arc<Self>,
        request: SessionCreateExecution<'_>,
    ) -> Result<SessionCreateApplied, SessionCreateError> {
        let SessionCreateExecution {
            session_id,
            style,
            destination_source,
            from_port,
            to_port,
            children,
            cancellation,
        } = request;
        use i2pr_api::sam::session_create::DestinationSource;

        // Step 1: decode or generate the destination identity. The
        // identity is the single private allocation this transaction
        // produces; everything below wraps an `Arc` to it.
        let identity = match destination_source {
            DestinationSource::Transient => {
                let mut rng = OsRng;
                DestinationIdentity::generate(&mut rng)
                    .map_err(|_| SessionCreateError::RandomnessUnavailable)?
            }
            DestinationSource::Imported(wrapper) => wrapper
                .into_identity()
                .map_err(|_| SessionCreateError::InvalidPrivateDestination)?,
        };
        let public_destination_b64 = encode_public_for(&identity);
        let private_destination_b64 = encode_private_for(&identity);
        let destination_id = identity.id();
        let identity_arc = Arc::new(identity);

        // Step 2: reserve the SAM session slot. Failure aborts the
        // transaction before we allocate any product material.
        let reservation = self
            .session_registry
            .reserve_session_with_style(
                session_id.clone(),
                destination_id,
                from_port,
                to_port,
                style,
            )
            .map_err(map_registry_error)?;

        // Step 3: prepare the localhost product material. A failure
        // here rolls the reservation back; nothing else has been
        // touched.
        let fabric = SamLocalProductFabric::new();
        let now_seconds = sam_now_seconds();
        let product = match fabric.prepare_for_destination(identity_arc.as_ref(), now_seconds) {
            Ok(product) => product,
            Err(_error) => {
                self.session_registry.rollback_reservation(&reservation);
                return Err(SessionCreateError::I2pError);
            }
        };

        // Step 4: build the destination runtime around the shared
        // identity Arc. Failure rolls the reservation and discards
        // the product material.
        let runtime = match DestinationRuntime::with_shared_identity(
            Arc::clone(&identity_arc),
            self.destination_config,
        ) {
            Ok(runtime) => runtime,
            Err(error) => {
                drop(product);
                self.session_registry.rollback_reservation(&reservation);
                return Err(SessionCreateError::DestinationRuntime(error.to_string()));
            }
        };
        let insert_result = match self.destination_registry.lock() {
            Ok(mut destinations) => destinations.insert(runtime),
            Err(_) => {
                drop(product);
                self.session_registry.rollback_reservation(&reservation);
                return Err(SessionCreateError::DestinationRegistryLocked);
            }
        };
        if let Err(error) = insert_result {
            drop(product);
            self.session_registry.rollback_reservation(&reservation);
            return Err(match error {
                i2pr_client::RegistryError::DuplicateDestination { .. } => {
                    SessionCreateError::DuplicateDestination
                }
                i2pr_client::RegistryError::CapacityExceeded { maximum } => {
                    SessionCreateError::DestinationsFull { maximum }
                }
                i2pr_client::RegistryError::CommandQueueFull { maximum } => {
                    SessionCreateError::CommandQueueFull { maximum }
                }
                _ => SessionCreateError::I2pError,
            });
        }

        // Step 5: install the per-destination StreamingManager pool.
        let pool_install = match self.streaming_pools.lock() {
            Ok(mut pools) => pools.install(destination_id),
            Err(_) => {
                drop(product);
                self.teardown_session(&session_id, destination_id);
                return Err(SessionCreateError::StreamingPoolsLocked);
            }
        };
        if pool_install.is_err() {
            drop(pool_install);
            drop(product);
            self.teardown_session(&session_id, destination_id);
            return Err(SessionCreateError::I2pError);
        }

        // Step 6: register the stream-session slot.
        if style == i2pr_api::sam::session_create::SessionCreateStyle::Stream
            && let Err(error) = self.stream_registry.register_session(session_id.clone())
        {
            drop(error);
            drop(product);
            self.teardown_session(&session_id, destination_id);
            return Err(SessionCreateError::I2pError);
        }

        // Step 7: build and install the SAM destination bridge. The
        // bridge shares the identity Arc with the runtime.
        let LocalDestinationProduct {
            outbound_role,
            inbound_tunnel_factory,
            validated_lease_set2,
            lease_set2,
        } = product;
        let bridge = SamDestinationBridge::with_shared_identity(
            Arc::clone(&identity_arc),
            lease_set2,
            outbound_role,
            now_seconds,
            i2pr_client::streaming::config::StreamingConfig::balanced(),
        );
        let bridge_handle = match self.sam_destinations.lock() {
            Ok(mut destinations) => destinations.install(destination_id, bridge),
            Err(_) => {
                self.teardown_session(&session_id, destination_id);
                return Err(SessionCreateError::SamDestinationsLocked);
            }
        };
        let _ = bridge_handle.install_inbound_tunnel_factory(inbound_tunnel_factory);

        // Step 8: spawn the per-destination runtime driver. A spawn
        // failure rolls the bridge and stream state back so we never
        // leave a destination with a bridge but no driver.
        if let Err(_error) = self.spawn_destination_driver(destination_id, children, cancellation) {
            self.teardown_session(&session_id, destination_id);
            return Err(SessionCreateError::I2pError);
        }

        // Step 9: commit the SAM reservation with the cached public
        // destination Base64 text.
        let entry = match self
            .session_registry
            .commit_reservation(&reservation, public_destination_b64.clone())
        {
            Ok(entry) => entry,
            Err(error) => {
                self.teardown_session(&session_id, destination_id);
                return Err(map_registry_error(error));
            }
        };

        // Identity Arc is now owned by both the runtime and the bridge.
        // Drop our local reference; the underlying allocation lives on.
        drop(identity_arc);
        // The product material has been moved into the bridge
        // (`outbound_role`, `lease_set2`) and the bridge's
        // `inbound_tunnel_factory`. The `validated_lease_set2` remains
        // owned by this scope; drop it explicitly to keep the lifecycle
        // obvious.
        drop(validated_lease_set2);

        Ok(SessionCreateApplied {
            session_id,
            destination_id,
            public_destination_b64: entry.public_destination_b64().to_owned(),
            private_destination_b64: private_destination_b64.clone(),
            style,
        })
    }

    /// Tears a session down exactly once. Called from the
    /// control-socket teardown path and from the supervisor shutdown
    /// path. Idempotent.
    pub fn teardown_session(&self, session_id: &SamSessionId, destination_id: DestinationId) {
        for child_id in self.session_registry.subsession_ids(session_id) {
            self.remove_subsession(session_id, &child_id);
        }
        if let Ok(mut drivers) = self.destination_drivers.lock()
            && let Some(driver) = drivers.remove(&destination_id)
        {
            let _ = driver.cancel(i2pr_core::CancellationReason::ParentScope);
        }
        self.teardown_forwards_for_session(session_id);
        let _ = self.session_registry.remove_by_session(session_id);
        self.teardown_subsession_streams(session_id, destination_id);
        if let Ok(mut destinations) = self.destination_registry.lock() {
            destinations.remove(&destination_id);
        }
        if let Ok(mut pools) = self.streaming_pools.lock() {
            pools.remove(&destination_id);
        }
        if let Ok(mut bridges) = self.sam_destinations.lock() {
            bridges.remove(destination_id);
        }
        self.drop_destination_signals(destination_id);
        if let Ok(mut counters) = self.delivery_counters.lock() {
            counters.remove(&destination_id);
        }
    }

    /// Registers a child session on a live PRIMARY without creating another
    /// destination or tunnel pool.
    pub fn add_subsession(
        &self,
        primary_id: &SamSessionId,
        request: &i2pr_api::sam::command::SessionAddRequest,
    ) -> Result<(), String> {
        let child_id = SamSessionId::new(request.id.clone())
            .ok_or_else(|| "subsession ID is invalid".to_owned())?;
        let child = self
            .session_registry
            .add_subsession(
                primary_id,
                child_id.clone(),
                i2pr_api::sam::registry::SamSubsessionConfig {
                    style: request.style,
                    from_port: request.from_port,
                    to_port: request.to_port,
                    listen_port: request.listen_port,
                    host_port: request.port,
                    host_address: request.host,
                    raw_header: request.raw_header,
                    protocol: request.protocol,
                    listen_protocol: request.listen_protocol,
                },
            )
            .map_err(|error| error.to_string())?;
        if child.style() == i2pr_api::sam::session_create::SessionCreateStyle::Stream
            && let Err(error) = self.stream_registry.register_session(child_id.clone())
        {
            let _ = self
                .session_registry
                .remove_subsession(primary_id, &child_id);
            return Err(error.to_string());
        }
        Ok(())
    }

    /// Encodes the shared private destination on demand for SAM's status reply.
    pub fn private_destination_for_session(&self, session_id: &SamSessionId) -> Option<String> {
        let entry = self.session_registry.get(session_id)?;
        let destinations = self.sam_destinations.lock().ok()?;
        let bridge = destinations.get(entry.destination_id())?;
        Some(bridge.with(|bridge| encode_private_for(bridge.identity().as_ref())))
    }

    /// Removes one child and its owned stream state while preserving sibling
    /// subsessions and the PRIMARY destination.
    pub fn remove_subsession(&self, primary_id: &SamSessionId, child_id: &SamSessionId) -> bool {
        let entry = self.session_registry.get(child_id);
        let Some(entry) = entry.filter(|entry| entry.parent_session_id() == Some(primary_id))
        else {
            return false;
        };
        let destination_id = entry.destination_id();
        if self
            .session_registry
            .remove_subsession(primary_id, child_id)
            .ok()
            .flatten()
            .is_none()
        {
            return false;
        }
        self.teardown_forwards_for_session(child_id);
        self.teardown_subsession_streams(child_id, destination_id);
        true
    }

    /// Sends one managed-app datagram through the child selected on the same
    /// PRIMARY. The operation has no host endpoint: routing uses only the
    /// child's protocol/ports and the canonical Destination datagram owner.
    pub(crate) fn send_managed_datagram(
        &self,
        primary_id: &SamSessionId,
        child_id: &SamSessionId,
        request: i2pr_client::datagram::DatagramSendRequest,
    ) -> Result<(), ManagedDatagramError> {
        let entry = self
            .session_registry
            .get(child_id)
            .filter(|entry| entry.parent_session_id() == Some(primary_id))
            .ok_or(ManagedDatagramError::UnknownChild)?;
        if protocol_for_datagram_style(entry.style()).is_none()
            || entry.protocol() != request.protocol
        {
            return Err(ManagedDatagramError::StyleMismatch);
        }
        let destination_id = entry.destination_id();
        let destinations = self
            .sam_destinations
            .lock()
            .map_err(|_| ManagedDatagramError::Unavailable)?;
        let bridge = destinations
            .get(destination_id)
            .ok_or(ManagedDatagramError::Unavailable)?;
        bridge
            .with(|bridge| {
                let identity = bridge.identity();
                bridge.datagrams_mut().send(identity.as_ref(), &request)
            })
            .map_err(ManagedDatagramError::Datagram)?;
        self.notify_outbound_signal(destination_id);
        Ok(())
    }

    /// Drains events only for the selected child. Sibling protocols and I2P
    /// destination ports remain queued for their own bounded operation.
    pub(crate) fn receive_managed_datagram(
        &self,
        primary_id: &SamSessionId,
        child_id: &SamSessionId,
    ) -> Result<Option<i2pr_client::datagram::DatagramReceiveEvent>, ManagedDatagramError> {
        let entry = self
            .session_registry
            .get(child_id)
            .filter(|entry| entry.parent_session_id() == Some(primary_id))
            .ok_or(ManagedDatagramError::UnknownChild)?;
        let protocol = entry.listen_protocol();
        let destinations = self
            .sam_destinations
            .lock()
            .map_err(|_| ManagedDatagramError::Unavailable)?;
        let bridge = destinations
            .get(entry.destination_id())
            .ok_or(ManagedDatagramError::Unavailable)?;
        let mut event = bridge.with(|bridge| {
            bridge
                .datagrams_mut()
                .pop_received_for_listener(protocol, entry.listen_port())
        });
        if let Some(event) = event.as_mut()
            && entry.style() == i2pr_api::sam::session_create::SessionCreateStyle::Raw
        {
            event.payload.clone_from(&event.raw_payload);
            event.from_hash = [0; 32];
            event.from_destination = None;
            event.sender_authenticated = false;
            event.options = None;
        }
        Ok(event)
    }

    fn handle_udp_bridge_packet(&self, bytes: &[u8], peer: SocketAddr) -> Result<(), &'static str> {
        if !peer.ip().is_loopback() {
            return Err("non-loopback UDP source");
        }
        let packet = datagram_udp::parse_packet(bytes).map_err(|error| match error {
            datagram_udp::DatagramPacketError::UnsupportedMessageControl => {
                "unsupported optional SAM UDP message control"
            }
            _ => "invalid SAM UDP packet",
        })?;
        let child_id = SamSessionId::new(packet.id).ok_or("invalid SAM UDP session ID")?;
        let entry = self
            .session_registry
            .get(&child_id)
            .ok_or("unknown SAM UDP session ID")?;
        let primary_id = entry
            .parent_session_id()
            .cloned()
            .ok_or("SAM UDP requires a PRIMARY child")?;
        let raw = entry.style() == i2pr_api::sam::session_create::SessionCreateStyle::Raw;
        let protocol = if raw {
            entry.protocol()
        } else {
            protocol_for_datagram_style(entry.style()).ok_or("SAM UDP session is not datagram")?
        };
        if raw {
            if packet.protocol.is_some_and(|value| value != protocol) {
                return Err("unsupported RAW protocol");
            }
        } else if packet.protocol.is_some() {
            return Err("PROTOCOL is valid only for RAW");
        }
        let destination_hash = resolve_sam_udp_destination(self, packet.destination)
            .ok_or("unresolved SAM UDP destination")?;
        let request = i2pr_client::datagram::DatagramSendRequest {
            destination_hash,
            source_port: packet.from_port.unwrap_or(entry.from_port()),
            destination_port: packet.to_port.unwrap_or(entry.to_port()),
            protocol,
            payload: packet.payload.to_vec(),
            options: None,
        };
        self.send_managed_datagram(&primary_id, &child_id, request)
            .map_err(|_| "SAM UDP datagram was rejected")
    }

    fn forward_host_datagrams(&self, destination_id: DestinationId) {
        let Some(socket) = self.udp_socket.lock().ok().and_then(|slot| slot.clone()) else {
            return;
        };
        let events = self
            .sam_destinations
            .lock()
            .ok()
            .and_then(|destinations| destinations.get(destination_id))
            .map(|bridge| bridge.with(|bridge| bridge.datagrams_mut().drain_received()))
            .unwrap_or_default();
        for event in events {
            let Some(entry) = self.session_registry.inbound_datagram_session(
                destination_id,
                event.protocol,
                event.destination_port,
            ) else {
                continue;
            };
            let Some(port) = entry.host_port().filter(|port| *port != 0) else {
                continue;
            };
            let raw_session =
                entry.style() == i2pr_api::sam::session_create::SessionCreateStyle::Raw;
            let Some(packet) =
                datagram_udp::encode_received(&event, raw_session, entry.raw_header())
            else {
                continue;
            };
            let target = SocketAddr::new(
                entry.host_address().unwrap_or(self.config.bind_address),
                port,
            );
            if let Err(error) = socket.try_send_to(&packet, target) {
                debug!(
                    protocol = event.protocol,
                    source_port = event.source_port,
                    destination_port = event.destination_port,
                    error = %error,
                    "SAM UDP forwarding failed"
                );
            }
        }
    }

    fn teardown_subsession_streams(
        &self,
        session_id: &SamSessionId,
        destination_id: DestinationId,
    ) {
        let attachments = self
            .stream_registry
            .unregister_session(session_id)
            .unwrap_or_default();
        if let Some(bridge) = self
            .sam_destinations
            .lock()
            .ok()
            .and_then(|destinations| destinations.get(destination_id))
        {
            bridge.with(|bridge| {
                for attachment in attachments {
                    let Some(connection_id) = attachment.connection_id() else {
                        continue;
                    };
                    let manager = match attachment.direction() {
                        i2pr_api::sam::streams::SamStreamDirection::Outbound => {
                            bridge.streaming_mut()
                        }
                        i2pr_api::sam::streams::SamStreamDirection::Inbound => {
                            bridge.receiver_streaming_mut()
                        }
                    };
                    let _ = manager.remove_connection(connection_id);
                }
            });
        }
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
        if !bind_address.ip().is_loopback() {
            return Err(SamServiceError::InvalidConfig(
                "SAM listeners must bind a loopback address".to_owned(),
            ));
        }
        let listener =
            TcpListener::bind(bind_address)
                .await
                .map_err(|source| SamServiceError::Bind {
                    address: bind_address,
                    source,
                })?;
        let bound_address = listener.local_addr().unwrap_or(bind_address);
        self.bind_udp_socket(bound_address.ip()).await?;
        Ok((listener, bound_address))
    }

    async fn bind_udp_socket(&self, bind_ip: IpAddr) -> Result<Arc<UdpSocket>, SamServiceError> {
        if let Some(socket) = self.udp_socket.lock().ok().and_then(|slot| slot.clone()) {
            return Ok(socket);
        }
        let address = SocketAddr::new(bind_ip, self.config.udp_port);
        let socket = Arc::new(
            UdpSocket::bind(address)
                .await
                .map_err(|source| SamServiceError::Bind { address, source })?,
        );
        let mut slot = self.udp_socket.lock().map_err(|_| {
            SamServiceError::InvalidConfig("SAM UDP socket lock poisoned".to_owned())
        })?;
        if let Some(existing) = slot.as_ref() {
            return Ok(Arc::clone(existing));
        }
        *slot = Some(Arc::clone(&socket));
        Ok(socket)
    }

    /// Spawns the Plan 147 per-destination runtime driver task for
    /// `destination_id`. The driver wakes on the destination's
    /// outbound-signal [`tokio::sync::Notify`] and drains queued
    /// `TransportSendRequest`s through the Plan 129 local seam into
    /// the registered peer bridge (looked up by destination hash via
    /// [`streams::SamDestinations::lookup_by_peer_hash`]). It also
    /// polls the canonical + receiver-mirror `StreamingManager`s for
    /// retransmits and acks on a fixed cadence, and notifies the
    /// destination's established-signal whenever it observes a
    /// connection that has transitioned to
    /// [`i2pr_client::streaming::connection::ConnectionState::Established`].
    ///
    /// The driver is owned by `children` and terminates when the
    /// cancellation token fires.
    pub fn spawn_destination_driver(
        self: &Arc<Self>,
        destination_id: DestinationId,
        children: &ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), DestinationDriverSpawnError> {
        let state = Arc::clone(self);
        let destination_for_task = destination_id;
        let driver_cancellation = cancellation.child_token();
        {
            let mut drivers = self
                .destination_drivers
                .lock()
                .map_err(|_| DestinationDriverSpawnError::RegistryLocked)?;
            if drivers.contains_key(&destination_id) {
                return Err(DestinationDriverSpawnError::AlreadyRunning);
            }
            drivers.insert(destination_id, driver_cancellation.clone());
        }
        let spawn_result = children.spawn(move |task_cancellation| async move {
            run_destination_driver(
                state,
                destination_for_task,
                driver_cancellation,
                task_cancellation,
            )
            .await;
            Ok(())
        });
        if let Err(error) = spawn_result {
            if let Ok(mut drivers) = self.destination_drivers.lock() {
                drivers.remove(&destination_id);
            }
            return Err(error.into());
        }
        Ok(())
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
        let udp_socket = self.bind_udp_socket(bind_address.ip()).await?;
        let udp_address = udp_socket.local_addr().unwrap_or(bind_address);
        info!(
            address = %bind_address,
            udp_address = %udp_address,
            max_clients = self.config.limits.max_clients,
            max_sessions = self.config.limits.max_sessions,
            "SAM loopback TCP and UDP listeners bound"
        );

        let client_permits = Arc::clone(&self.connection_permits);
        let mut datagram_buffer = [0_u8; 65_507];

        let child_token = cancellation.child_token();
        loop {
            tokio::select! {
                biased;
                _ = child_token.cancelled() => {
                    debug!("sam listener cancellation observed");
                    break;
                }
                received = udp_socket.recv_from(&mut datagram_buffer) => {
                    match received {
                        Ok((length, peer)) => {
                            if let Err(reason) = self.handle_udp_bridge_packet(
                                &datagram_buffer[..length],
                                peer,
                            ) {
                                debug!(reason, "SAM UDP datagram rejected");
                            }
                        }
                        Err(error) => warn!(error = %error, "SAM UDP receive failed"),
                    }
                }
                accept = listener.accept() => {
                    let (stream, peer) = match accept {
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
                    let origin = SamConnectionOrigin::Loopback {
                        peer_ip: peer.ip(),
                    };
                    let state = Arc::clone(&self);
                    let child_token_for_task = child_token.clone();
                    let permit_for_task: OwnedSemaphorePermit = permit;
                    let children_for_task = children.clone();
                    if let Err(error) = children_for_task.clone().spawn(move |task_cancellation| {
                        let raw_scope = children_for_task.clone();
                        async move {
                            handle_connection(
                                state,
                                Box::new(stream),
                                origin,
                                permit_for_task,
                                task_cancellation,
                                child_token_for_task,
                                raw_scope,
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

    /// Drives one private SAM connection without binding or connecting a
    /// host socket. The caller owns task supervision; admission uses the
    /// same bounded connection ceiling as the loopback listener.
    #[allow(dead_code)] // The registered Plan 355 gateway is the production caller.
    pub(crate) async fn drive_private_connection(
        self: Arc<Self>,
        stream: SamIoStream,
        cancellation: CancellationToken,
        service_cancellation: CancellationToken,
        raw_children: ChildScope,
    ) -> Result<(), PrivateConnectionAdmissionError> {
        let permit = Arc::clone(&self.connection_permits)
            .try_acquire_owned()
            .map_err(|_| PrivateConnectionAdmissionError::AtCapacity)?;
        handle_connection(
            self,
            stream,
            SamConnectionOrigin::ManagedAppPrivate,
            permit,
            cancellation,
            service_cancellation,
            raw_children,
        )
        .await;
        Ok(())
    }

    /// Drives the dedicated manager-protocol datagram stream. Each request is
    /// length-prefixed and bounded before allocation; no request names a host
    /// address or can create a socket.
    pub(crate) async fn drive_private_datagram_connection(
        self: Arc<Self>,
        mut stream: SamIoStream,
        cancellation: CancellationToken,
        service_cancellation: CancellationToken,
    ) {
        loop {
            let mut length = [0_u8; 4];
            let read = tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = service_cancellation.cancelled() => break,
                result = stream.read_exact(&mut length) => result,
            };
            if read.is_err() {
                break;
            }
            let declared = u32::from_be_bytes(length) as usize;
            if declared == 0 || declared > i2pr_app_manager_proto::MAX_DATA_FRAME_BYTES {
                break;
            }
            let mut request_bytes = vec![0_u8; declared];
            let read = tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = service_cancellation.cancelled() => break,
                result = stream.read_exact(&mut request_bytes) => result,
            };
            if read.is_err() {
                break;
            }
            let reply = match i2pr_app_manager_proto::datagram::DatagramRequest::decode(
                &request_bytes,
            ) {
                Ok(i2pr_app_manager_proto::datagram::DatagramRequest::Send {
                    primary_id,
                    child_id,
                    destination_hash,
                    source_port,
                    destination_port,
                    protocol,
                    options,
                    payload,
                }) => match (SamSessionId::new(primary_id), SamSessionId::new(child_id)) {
                    (Some(primary), Some(child)) => {
                        let options = if options.is_empty() {
                            Some(None)
                        } else {
                            i2pr_proto::Mapping::decode(
                                &options,
                                i2pr_app_manager_proto::datagram::MAX_DATAGRAM_OPTIONS_BYTES,
                            )
                            .ok()
                            .map(Some)
                        };
                        match options {
                            Some(options) => {
                                let request = i2pr_client::datagram::DatagramSendRequest {
                                    destination_hash,
                                    source_port,
                                    destination_port,
                                    protocol,
                                    payload,
                                    options,
                                };
                                if self
                                    .send_managed_datagram(&primary, &child, request)
                                    .is_ok()
                                {
                                    i2pr_app_manager_proto::datagram::DatagramReply::Sent
                                } else {
                                    i2pr_app_manager_proto::datagram::DatagramReply::Rejected
                                }
                            }
                            None => i2pr_app_manager_proto::datagram::DatagramReply::Rejected,
                        }
                    }
                    _ => i2pr_app_manager_proto::datagram::DatagramReply::Rejected,
                },
                Ok(i2pr_app_manager_proto::datagram::DatagramRequest::Receive {
                    primary_id,
                    child_id,
                }) => match (SamSessionId::new(primary_id), SamSessionId::new(child_id)) {
                    (Some(primary), Some(child)) => {
                        match self.receive_managed_datagram(&primary, &child) {
                            Ok(Some(event)) => {
                                let options = event.options.as_ref().and_then(|mapping| mapping.encode_to_vec(i2pr_app_manager_proto::datagram::MAX_DATAGRAM_OPTIONS_BYTES).ok()).unwrap_or_default();
                                i2pr_app_manager_proto::datagram::DatagramReply::Event(
                                    i2pr_app_manager_proto::datagram::DatagramEvent {
                                        from_hash: event.from_hash,
                                        sender_authenticated: event.sender_authenticated,
                                        source_port: event.source_port,
                                        destination_port: event.destination_port,
                                        protocol: event.protocol,
                                        received_at_ms: event.received_at_ms,
                                        options,
                                        payload: event.payload,
                                    },
                                )
                            }
                            Ok(None) => i2pr_app_manager_proto::datagram::DatagramReply::Empty,
                            Err(_) => i2pr_app_manager_proto::datagram::DatagramReply::Rejected,
                        }
                    }
                    _ => i2pr_app_manager_proto::datagram::DatagramReply::Rejected,
                },
                Err(_) => i2pr_app_manager_proto::datagram::DatagramReply::Rejected,
            };
            let Ok(encoded) = reply.encode() else { break };
            if write_managed_datagram_reply(&mut stream, &encoded)
                .await
                .is_err()
            {
                break;
            }
        }
    }
}

async fn write_managed_datagram_reply(stream: &mut SamIoStream, payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len()).map_err(|_| io::ErrorKind::InvalidData)?;
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await
}

/// Admission failure for the narrow private SAM connection seam.
#[allow(dead_code)] // Consumed by the registered Plan 355 gateway.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PrivateConnectionAdmissionError {
    /// The configured per-service concurrent connection ceiling is full.
    AtCapacity,
}

#[allow(dead_code)] // ManagedAppPrivate is activated by the registered Plan 355 gateway.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SamConnectionOrigin {
    Loopback { peer_ip: IpAddr },
    ManagedAppPrivate,
}

fn encode_public_for(identity: &DestinationIdentity) -> String {
    let wrapper =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(identity)
            .expect("identity is fresh and should round-trip through the SAM codec");
    wrapper.encode_public_base64()
}

fn protocol_for_datagram_style(
    style: i2pr_api::sam::session_create::SessionCreateStyle,
) -> Option<u8> {
    use i2pr_api::sam::session_create::SessionCreateStyle;
    match style {
        SessionCreateStyle::Datagram => Some(i2pr_client::datagram::DATAGRAM1_PROTOCOL),
        SessionCreateStyle::Raw => Some(i2pr_client::datagram::RAW_DATAGRAM_PROTOCOL),
        SessionCreateStyle::Datagram2 => Some(i2pr_client::datagram::DATAGRAM2_PROTOCOL),
        SessionCreateStyle::Datagram3 => Some(i2pr_client::datagram::DATAGRAM3_PROTOCOL),
        SessionCreateStyle::Stream | SessionCreateStyle::Primary => None,
    }
}

fn resolve_sam_udp_destination(state: &SamServiceState, name: &str) -> Option<[u8; 32]> {
    use i2pr_api::sam::naming::{decode_b32_destination_hash, resolve_public_destination};

    if let Ok(canonical) = resolve_public_destination(name) {
        return decode_public_destination_hash(&canonical);
    }
    if let Ok(hash) = decode_b32_destination_hash(name) {
        return Some(hash);
    }
    state
        .addressbook_lookup(name)
        .and_then(|entry| decode_public_destination_hash(&entry.destination))
}

fn decode_public_destination_hash(value: &str) -> Option<[u8; 32]> {
    let bytes = i2pr_api::sam::base64::decode(value, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).ok()?;
    let destination =
        i2pr_proto::Destination::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).ok()?;
    Some(*destination.hash().ok()?.as_bytes())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ManagedDatagramError {
    #[error("unknown child for PRIMARY")]
    UnknownChild,
    #[error("datagram protocol does not match child style")]
    StyleMismatch,
    #[error("datagram owner is unavailable")]
    Unavailable,
    #[error(transparent)]
    Datagram(#[from] i2pr_client::datagram::DatagramError),
}

fn encode_private_for(identity: &DestinationIdentity) -> String {
    let wrapper =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(identity)
            .expect("identity is fresh and should round-trip through the SAM codec");
    wrapper.encode_base64()
}

fn encode_destination_public(destination: &i2pr_proto::Destination) -> Option<String> {
    let bytes = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .ok()?;
    Some(i2pr_api::sam::base64::encode(&bytes))
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
        SamSessionRegistryError::SubsessionsFull { maximum } => {
            SessionCreateError::SubsessionsFull { maximum }
        }
        SamSessionRegistryError::SubsessionListenConflict { .. } => SessionCreateError::I2pError,
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
    /// A PRIMARY reached its child-session ceiling.
    #[error("PRIMARY subsession ceiling {maximum} reached")]
    SubsessionsFull {
        /// Accepted ceiling.
        maximum: usize,
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
    /// The SAM destination-bridge registry mutex was poisoned.
    #[error("SAM destination registry mutex poisoned")]
    SamDestinationsLocked,
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
            | Self::SamDestinationsLocked
            | Self::StreamingPoolsLocked
            | Self::SamRegistryLocked
            | Self::CounterOverflow
            | Self::UnknownSession(_)
            | Self::I2pError => ReplyResult::I2pError,
            Self::SessionsFull { .. }
            | Self::SubsessionsFull { .. }
            | Self::StreamAttachmentsFull { .. }
            | Self::DestinationsFull { .. }
            | Self::CommandQueueFull { .. } => ReplyResult::I2pError,
        }
    }
}

/// Plan 147 §8: the per-connection transition state produced by
/// `dispatch_command`. The connection loop reads this and either
/// continues parsing commands, closes the socket, or hands the socket
/// off to the raw-mode driver task.
pub enum ConnectionDisposition {
    /// Continue parsing SAM command lines on this connection. The
    /// optional reply (when `Some`) has already been written to the
    /// socket by `dispatch_command`.
    Continue {
        /// Next state to feed back into the dispatch loop.
        next_state: ServerConnectionState,
    },
    /// Close the connection without transitioning to raw mode. Any
    /// reply has already been written.
    Close,
    /// Plan 147 §8: the runtime reports a successful STREAM CONNECT or
    /// STREAM ACCEPT that must transition to raw byte mode. The
    /// connection loop constructs the [`RawStreamHandoff`] from this
    /// payload plus the line reader's buffered bytes, transfers the
    /// `TcpStream` ownership to the raw driver, and exits.
    RawTransition(RawTransitionPayload),
}

/// Plan 147 §8: data the connection loop needs to construct the
/// [`RawStreamHandoff`] when `dispatch_command` reports
/// `ConnectionDisposition::RawTransition`. The `TcpStream` is not
/// inside this struct because `dispatch_command` still owns the
/// socket at the moment it returns.
#[derive(Debug)]
pub struct RawTransitionPayload {
    /// Direction (CONNECT vs ACCEPT).
    pub direction: RawDirection,
    /// SAM stream attachment id.
    pub attachment_id: u32,
    /// Owning session id.
    pub session_id: SamSessionId,
    /// Owning destination id.
    pub destination_id: DestinationId,
    /// Streaming connection id on the destination's `StreamingManager`.
    pub connection_id: i2pr_client::streaming::connection::ConnectionId,
    /// Peer destination.
    pub peer_destination: i2pr_client::streaming::manager::RemoteDestination,
    /// `true` when the SAM `SILENT=true` option was supplied.
    pub silent: bool,
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
    mut stream: SamIoStream,
    origin: SamConnectionOrigin,
    _connection_permit: OwnedSemaphorePermit,
    cancellation: CancellationToken,
    service_cancellation: CancellationToken,
    raw_children: ChildScope,
) {
    let connection_owner = state.next_forward_owner();
    let hello_timeout = state.config.limits.hello_timeout;
    let command_timeout = state.config.limits.command_timeout;
    let connection_cancellation = cancellation.child_token();

    let mut reader = LineReader::new();
    let mut connection_state = ServerConnectionState::AwaitHello;
    // Plan 147 §8: when `dispatch_command` reports a raw-mode
    // transition the connection loop hands the `TcpStream` over to
    // `run_raw_stream` and exits. The `Vec<u8>` here captures any
    // bytes the `LineReader` buffered past the final command
    // newline; the raw driver emits them as the first TCP->Streaming
    // payload.
    let mut pending_raw: Option<RawTransitionPayload> = None;

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
                let disposition = dispatch_command(
                    state.clone(),
                    connection_state.clone(),
                    &outcome,
                    &mut stream,
                    origin,
                    true,
                    connection_owner,
                    &raw_children,
                    connection_cancellation.clone(),
                )
                .await?;
                apply_disposition(
                    disposition,
                    &mut connection_state,
                    &mut pending_raw,
                )?;
            }
        }

        while connection_state == ServerConnectionState::AwaitHello || !connection_state.is_closed()
        {
            if pending_raw.is_some() {
                break;
            }
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
                    let disposition = dispatch_command(
                        state.clone(),
                        connection_state.clone(),
                        &outcome,
                        &mut stream,
                        origin,
                        false,
                        connection_owner,
                        &raw_children,
                        connection_cancellation.clone(),
                    )
                    .await?;
                    apply_disposition(
                        disposition,
                        &mut connection_state,
                        &mut pending_raw,
                    )?;
                }
            }
        }
        Ok(())
    }
    .await;

    if let Err(ref error) = result {
        debug!(error = %error, "sam connection ended with error");
    }

    if pending_raw.is_none() {
        let _ = connection_cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
    }
    state.teardown_forward_owner(connection_owner);

    // Plan 147 §8: when this handle_connection loop exits because
    // the connection transitioned to raw mode, the bridge must
    // stay installed — the dedicated raw-stream driver
    // (`run_raw_stream`) takes ownership of the socket and keeps
    // routing bytes through the same `bridge_to_peer` seam.
    // Calling `teardown_session` here would `bridges.remove(...)`
    // and orphan the runtime driver, dropping every queued SYN
    // response before it ever reaches the peer.
    if let ServerConnectionState::SessionControl {
        session_id,
        destination_id,
        ..
    } = &connection_state
        && pending_raw.is_none()
    {
        state.teardown_session(session_id, *destination_id);
    }

    // Plan 147 §8: on `ConnectionDisposition::RawTransition` the
    // connection task transfers the `TcpStream` ownership to
    // `run_raw_stream`. We drain the line reader's buffered bytes
    // (Plan 147 §6) before the handoff so no byte can ever be
    // mis-parsed as a SAM command after the socket enters raw mode.
    if let Some(payload) = pending_raw.take() {
        let initial_raw_bytes = reader.take_buffered();
        let handoff = RawStreamHandoff {
            stream,
            session_id: payload.session_id,
            destination_id: payload.destination_id,
            attachment_id: payload.attachment_id,
            connection_id: payload.connection_id,
            peer_destination: payload.peer_destination,
            initial_raw_bytes,
            silent: payload.silent,
            direction: payload.direction,
        };
        let raw_cancellation = cancellation.child_token();
        let cleanup = RawStreamCleanup {
            session_id: handoff.session_id.clone(),
            destination_id: handoff.destination_id,
            attachment_id: handoff.attachment_id,
            connection_id: handoff.connection_id,
            peer_destination: handoff.peer_destination.clone(),
            direction: handoff.direction,
        };
        // Plan 151: when the scope rejects the raw driver (live task
        // ceiling), unwind the just-allocated attachment through the
        // normal finish path instead of leaking it. The peer observes
        // a prompt CLOSE/EOF rather than a hung stream, and the last
        // release still tears the session down.
        let cleanup_on_spawn_failure = cleanup.clone();
        let state_for_spawn_failure = Arc::clone(&state);
        if let Err(error) = raw_children.spawn(move |_task_cancellation| {
            let task_cancellation = _task_cancellation;
            let raw_cancellation = raw_cancellation;
            let state = Arc::clone(&state);
            async move {
                let raw_result = tokio::select! {
                    biased;
                    _ = task_cancellation.cancelled() => Ok(()),
                    result = run_raw_stream(
                        Arc::clone(&state),
                        handoff,
                        raw_cancellation,
                    ) => result,
                };
                if let Err(error) = &raw_result {
                    warn!(?error, "raw stream driver ended with error");
                }
                state.finish_raw_stream(cleanup, raw_result.is_err());
                Ok(())
            }
        }) {
            warn!(?error, "failed to spawn raw stream driver task");
            state_for_spawn_failure.finish_raw_stream(cleanup_on_spawn_failure, false);
        }
        return;
    }

    let _ = stream.shutdown().await;
}

/// Plan 147 §8: applies one `ConnectionDisposition` produced by
/// `dispatch_command` to the per-connection loop state.
fn apply_disposition(
    disposition: ConnectionDisposition,
    connection_state: &mut ServerConnectionState,
    pending_raw: &mut Option<RawTransitionPayload>,
) -> Result<(), ConnectionFailure> {
    match disposition {
        ConnectionDisposition::Continue { next_state } => {
            *connection_state = next_state;
            Ok(())
        }
        ConnectionDisposition::Close => {
            *connection_state = ServerConnectionState::Closed;
            Ok(())
        }
        ConnectionDisposition::RawTransition(payload) => {
            *pending_raw = Some(payload);
            Ok(())
        }
    }
}

async fn receive_command(
    stream: &mut SamIoStream,
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

#[allow(clippy::too_many_arguments)]
async fn dispatch_command(
    state: Arc<SamServiceState>,
    state_conn: ServerConnectionState,
    outcome: &CommandOutcome,
    stream: &mut SamIoStream,
    origin: SamConnectionOrigin,
    _is_first: bool,
    connection_owner: u64,
    raw_children: &ChildScope,
    session_cancellation: CancellationToken,
) -> Result<ConnectionDisposition, ConnectionFailure> {
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
        return Ok(ConnectionDisposition::Continue { next_state: next });
    }

    let dispatch = dispatch_command_state(state_conn.clone(), outcome, state.max_supported_version);
    match dispatch {
        DispatchOutcome::Stay { reply } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
        DispatchOutcome::Advance { state, reply } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(ConnectionDisposition::Continue { next_state: state })
        }
        DispatchOutcome::Close { reply, .. } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(ConnectionDisposition::Close)
        }
        DispatchOutcome::Malformed { reply, .. } => {
            write_reply(stream, &reply).await?;
            Ok(ConnectionDisposition::Close)
        }
        DispatchOutcome::Unsupported { reply, .. } => {
            write_reply(stream, &reply).await?;
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
        DispatchOutcome::RequireSessionCreate { request } => {
            let request = *request;
            let style = request.style;
            let from_port = request.from_port;
            let to_port = request.to_port;
            let id = match SamSessionId::new(request.id.clone()) {
                Some(id) => id,
                None => {
                    let reply = Reply::Session(SessionStatus::error(
                        ReplyResult::I2pError,
                        Some("session id rejected by registry validator".to_owned()),
                    ));
                    write_reply(stream, &reply).await?;
                    return Ok(ConnectionDisposition::Continue {
                        next_state: state_conn,
                    });
                }
            };
            if !matches!(state_conn, ServerConnectionState::UtilityReady { .. }) {
                let reply = Reply::Session(SessionStatus::error(
                    ReplyResult::I2pError,
                    Some("SESSION CREATE before HELLO".to_owned()),
                ));
                write_reply(stream, &reply).await?;
                return Ok(ConnectionDisposition::Continue {
                    next_state: state_conn,
                });
            }
            let apply_result = match state.execute_session_create(SessionCreateExecution {
                session_id: id,
                style,
                destination_source: request.destination,
                from_port,
                to_port,
                children: raw_children,
                cancellation: session_cancellation,
            }) {
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
                DispatchOutcome::Advance { state, .. } => {
                    Ok(ConnectionDisposition::Continue { next_state: state })
                }
                _ => Ok(ConnectionDisposition::Continue {
                    next_state: state_conn,
                }),
            }
        }
        DispatchOutcome::RequireSessionAdd {
            primary_id,
            request,
        } => {
            let result = state.add_subsession(&primary_id, &request);
            let reply = match result {
                Ok(()) => Reply::Session(SessionStatus::ok_id(&request.id)),
                Err(message) => {
                    Reply::Session(SessionStatus::error(ReplyResult::I2pError, Some(message)))
                }
            };
            write_reply(stream, &reply).await?;
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
        DispatchOutcome::RequireSessionRemove {
            primary_id,
            child_id,
        } => {
            let child_id = SamSessionId::new(child_id);
            let removed_id = child_id.as_ref().map(|id| id.as_str().to_owned());
            let removed = child_id
                .as_ref()
                .is_some_and(|child_id| state.remove_subsession(&primary_id, child_id));
            let reply = if removed {
                Reply::Session(SessionStatus::ok_id(
                    removed_id.as_deref().unwrap_or_default(),
                ))
            } else {
                Reply::Session(SessionStatus::error(
                    ReplyResult::InvalidId,
                    Some("unknown subsession ID for this PRIMARY".to_owned()),
                ))
            };
            write_reply(stream, &reply).await?;
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
        DispatchOutcome::RequireStreamConnect { request } => {
            let request = *request;
            let outcome = execute_stream_connect(state.clone(), request).await;
            let outcome = apply_stream_connect_outcome(outcome);
            handle_stream_connect_outcome(outcome, stream, state_conn).await
        }
        DispatchOutcome::RequireStreamAccept { request } => {
            let request = *request;
            let outcome = execute_stream_accept(state.clone(), request).await;
            let outcome = i2pr_api::sam::server_state::apply_stream_accept_outcome(outcome);
            handle_stream_connect_outcome(outcome, stream, state_conn).await
        }
        DispatchOutcome::RequireStreamForward { request } => {
            let request = *request;
            let session_id = SamSessionId::new(request.session_id.clone());
            let mut outcome = match origin {
                SamConnectionOrigin::ManagedAppPrivate => {
                    Err(i2pr_api::sam::server_state::StreamForwardFailed {
                        result: ReplyResult::I2pError,
                        message: "STREAM FORWARD is unavailable on private connections".to_owned(),
                    })
                }
                SamConnectionOrigin::Loopback { peer_ip } => {
                    execute_stream_forward(&state, request, peer_ip, connection_owner)
                }
            };
            if outcome.is_ok()
                && let Some(session_id) = session_id
                && let Some(registration) = state.forward_registration(&session_id)
                && let Err(error) = spawn_forward_worker(
                    &state,
                    registration,
                    raw_children,
                    session_cancellation.clone(),
                )
            {
                state.teardown_forward_owner(connection_owner);
                outcome = Err(i2pr_api::sam::server_state::StreamForwardFailed {
                    result: ReplyResult::I2pError,
                    message: format!("forward worker spawn failed: {error}"),
                });
            }
            let outcome = apply_stream_forward_outcome(outcome);
            if let Some(reply) = outcome.reply() {
                write_reply(stream, reply).await?;
            }
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
        DispatchOutcome::RequireNamingLookup { request } => {
            let request = *request;
            let requested_name = request.name.clone();
            let outcome = execute_naming_lookup(&state, state_conn.clone(), request);
            let outcome = apply_naming_lookup_outcome(requested_name, outcome);
            if let Some(reply) = outcome.reply() {
                write_reply(stream, reply).await?;
            }
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
        DispatchOutcome::StreamRawMode { stream_id, .. } => {
            let _ = stream_id;
            Ok(ConnectionDisposition::Continue {
                next_state: state_conn,
            })
        }
    }
}

/// Plan 147 §8 step 1 / Plan 149 §9: convert the STREAM CONNECT/ACCEPT
/// `DispatchOutcome` into a `ConnectionDisposition`. On `StreamRawMode`
/// the dispatcher writes the SAM `STREAM STATUS RESULT=OK` reply (and
/// the authenticated peer Destination line for non-silent ACCEPT),
/// then signals the connection loop to construct a `RawStreamHandoff`
/// and hand the socket over to the dedicated raw-mode driver. The
/// connection loop MUST NOT touch the socket after it observes
/// `ConnectionDisposition::RawTransition`.
///
/// Plan 149 §9 freezes the byte-exact raw transition:
///
/// - `STREAM CONNECT SILENT=false`:
///   `STREAM STATUS RESULT=OK\n<raw bytes>`
/// - `STREAM CONNECT SILENT=true`:
///   `<raw bytes>` (no OK line; failure closes without false success)
/// - `STREAM ACCEPT SILENT=false`:
///   `STREAM STATUS RESULT=OK\n<authenticated peer public Destination>\n<raw bytes>`
/// - `STREAM ACCEPT SILENT=true`:
///   `<raw bytes>` (no OK line and no peer-Destination line)
async fn handle_stream_connect_outcome(
    outcome: DispatchOutcome,
    stream: &mut SamIoStream,
    state_conn: ServerConnectionState,
) -> Result<ConnectionDisposition, ConnectionFailure> {
    use i2pr_api::sam::reply::StreamStatus;
    use i2pr_api::sam::server_state::StreamRawTransition;
    match outcome {
        DispatchOutcome::StreamRawMode {
            stream_id,
            transition,
        } => {
            let _ = stream_id;
            let (
                direction,
                destination_id,
                session_id,
                connection_id,
                peer_destination,
                silent,
                peer_destination_b64,
            ) = match transition {
                StreamRawTransition::Connect {
                    destination_id,
                    session_id,
                    connection_id,
                    peer_destination,
                    silent,
                    ..
                } => (
                    RawDirection::Outbound,
                    destination_id,
                    session_id,
                    connection_id,
                    peer_destination,
                    silent,
                    None,
                ),
                StreamRawTransition::Accept {
                    destination_id,
                    session_id,
                    connection_id,
                    peer_destination,
                    peer_destination_b64,
                    silent,
                    ..
                } => (
                    RawDirection::Inbound,
                    destination_id,
                    session_id,
                    connection_id,
                    peer_destination,
                    silent,
                    peer_destination_b64,
                ),
            };
            // Plan 149 §9: write the OK line only when the request
            // was not silent. Once written, the raw driver owns every
            // subsequent byte.
            if !silent {
                stream
                    .write_all(b"STREAM STATUS RESULT=OK\n")
                    .await
                    .map_err(ConnectionFailure::Io)?;
                stream.flush().await.map_err(ConnectionFailure::Io)?;
                if let Some(peer_b64) = peer_destination_b64.as_deref() {
                    let line = format!("DESTINATION={peer_b64}\n");
                    stream
                        .write_all(line.as_bytes())
                        .await
                        .map_err(ConnectionFailure::Io)?;
                    stream.flush().await.map_err(ConnectionFailure::Io)?;
                }
            }
            let _ = StreamStatus::ok;
            // The SAM attachment id is allocated by SamStreamRegistry;
            // it is not required to equal the Streaming connection id.
            let attachment_id = stream_id;
            Ok(ConnectionDisposition::RawTransition(RawTransitionPayload {
                direction,
                attachment_id,
                session_id,
                destination_id,
                connection_id,
                peer_destination,
                silent,
            }))
        }
        DispatchOutcome::Close { reply, .. } => {
            if let Some(reply) = reply {
                write_reply(stream, &reply).await?;
            }
            Ok(ConnectionDisposition::Close)
        }
        _ => Ok(ConnectionDisposition::Continue {
            next_state: state_conn,
        }),
    }
}

async fn write_reply(stream: &mut SamIoStream, reply: &Reply) -> Result<(), ConnectionFailure> {
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
    let value = if name.eq_ignore_ascii_case("ME") {
        let session_id = match connection {
            ServerConnectionState::SessionControl { session_id, .. } => session_id,
            _ => {
                return Err(NamingLookupFailed {
                    result: ReplyResult::InvalidName,
                    message: "NAME=ME requires a session context".to_owned(),
                });
            }
        };
        state
            .session_registry()
            .get(&session_id)
            .map(|entry| entry.public_destination_b64().to_owned())
            .ok_or_else(|| NamingLookupFailed {
                result: ReplyResult::InvalidId,
                message: "session context no longer exists".to_owned(),
            })?
    } else if let Ok(value) = resolve_public_destination(&name) {
        value
    } else if name.to_ascii_lowercase().ends_with(".b32.i2p") {
        match decode_b32_destination_hash(&name) {
            Ok(hash) => {
                let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes(hash));
                state
                    .session_registry()
                    .public_destination_for_destination(&destination_id)
                    .ok_or_else(|| NamingLookupFailed {
                        result: ReplyResult::KeyNotFound,
                        message: "destination is not present in the local naming surface"
                            .to_owned(),
                    })?
            }
            Err(_) => {
                return Err(NamingLookupFailed {
                    result: ReplyResult::InvalidKey,
                    message: "invalid Base32 destination name".to_owned(),
                });
            }
        }
    } else {
        // Plan 294: ordinary `.i2p` names consult the canonical address
        // book when the subsystem is active; session-registry and Base32
        // paths above always precede it. Inactive or absent stays
        // KeyNotFound, exactly as before Plan 294. One trailing dot is
        // the canonical DNS root marker and strips before the suffix
        // check (the owner canonicalizes identically).
        let bare = name.strip_suffix('.').unwrap_or(&name);
        if bare.to_ascii_lowercase().ends_with(".i2p") {
            if let Some(entry) = state.addressbook_lookup(&name) {
                entry.destination
            } else {
                return Err(NamingLookupFailed {
                    result: ReplyResult::KeyNotFound,
                    message: "name is unavailable in the local naming surface".to_owned(),
                });
            }
        } else {
            // Non-`.i2p` names never reach naming authorities.
            return Err(NamingLookupFailed {
                result: ReplyResult::InvalidKey,
                message: "name is unavailable in the local naming surface".to_owned(),
            });
        }
    };

    let options = if request.include_lease_set_options {
        let hash = decode_public_destination_hash(&value).ok_or_else(|| NamingLookupFailed {
            result: ReplyResult::InvalidKey,
            message: "resolved value is not a complete public Destination".to_owned(),
        })?;
        let resolved = state
            .sam_destinations()
            .lock()
            .expect("sam destinations poisoned")
            .resolve_local_lease_set2(&hash, sam_now_seconds())
            .map_err(|error| NamingLookupFailed {
                result: ReplyResult::I2pError,
                message: format!("local LeaseSet2 validation failed: {error}"),
            })?;
        let Some((lease_set, _destination_id)) = resolved else {
            return Err(NamingLookupFailed {
                result: ReplyResult::LeaseSetNotFound,
                message: "LeaseSet is not available in the local naming surface".to_owned(),
            });
        };
        lease_set
            .lease_set2()
            .options()
            .entries()
            .iter()
            .map(|entry| (entry.key().to_owned(), entry.value().to_owned()))
            .collect()
    } else {
        Vec::new()
    };
    Ok(NamingLookupApplied { value, options })
}

/// Executes a `STREAM CONNECT` request after the HELLO handshake.
///
/// The function validates the session id, decodes the supplied
/// public destination, reserves a per-session stream attachment
/// slot, opens the outbound Streaming connection via the
/// destination bridge, and lets the supervised destination driver
/// route queued `TransportSendRequest`s through the local product
/// fabric and `StreamingDestinationAdapter`.
///
/// Plan 138 §7 forbids emitting `STREAM STATUS RESULT=OK` before
/// the underlying Streaming connection is `Established`. The
/// self-composed local product driver drives the SYN response
/// inbound; this function returns only once the connection state is
/// `Established` (or once a bounded wait expires, in which case the
/// reply is `RESULT=TIMEOUT`).
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
        silent,
        from_port,
        to_port,
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
    if entry.style() != i2pr_api::sam::session_create::SessionCreateStyle::Stream {
        return Err(StreamConnectFailed {
            result: ReplyResult::InvalidId,
            message: "STREAM CONNECT requires a STREAM session or subsession ID".to_owned(),
        });
    }
    let destination_id = entry.destination_id();
    let default_from_port = entry.from_port();
    let default_to_port = entry.to_port();

    // Plan 223: decode the supplied Destination for both legacy shapes.
    // ElGamal/type-0 Destinations carry filler, not the X25519 static key;
    // the active key is resolved via the local LS2 directory below.
    // X25519/type-4 Destinations (pre-223) still carry the static directly
    // for backwards compatibility with non-local remotes.
    let (target_destination_id, signing_public_key) =
        match streams::decode_destination_id_and_signing(&destination) {
            Ok(pair) => pair,
            Err(_) => {
                return Err(StreamConnectFailed {
                    result: ReplyResult::InvalidKey,
                    message: "could not decode DESTINATION".to_owned(),
                });
            }
        };
    let destination_hash = *target_destination_id.as_hash().as_bytes();

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

    // Plan 149 §7: when the target destination is locally owned, resolve
    // the peer's validated LeaseSet2 through the SAM service's local
    // directory and install it into the sender's routing state. This is
    // the production analog of the Plan 147 test's manual cross-install.
    // We never call the private `install_remote_lease_set2` from a test
    // here; the SAM service owns the local directory and only hands out
    // records it has validated itself. Unknown remote destinations skip
    // this path entirely. Plan 223: the LS2 X25519 key is also the
    // ECIES static for ElGamal Destinations.
    let local_resolution: Option<i2pr_netdb::ValidatedLeaseSet2> = match state
        .sam_destinations()
        .lock()
        .expect("sam destinations poisoned")
        .resolve_local_lease_set2(&destination_hash, sam_now_seconds())
    {
        Ok(Some((validated, _local_id))) => Some(validated),
        Ok(None) => None,
        Err(error) => {
            return Err(StreamConnectFailed {
                result: ReplyResult::InvalidKey,
                message: format!("local peer lease set validation failed: {error}"),
            });
        }
    };
    // Resolve the ECIES static: prefer the local LS2 X25519 key when the
    // peer is locally owned; otherwise fall back to the Destination field
    // for X25519 remotes. ElGamal non-local remotes fail closed (no NetDB
    // LS2 lookup in the loopback-only SAM path).
    let static_public: [u8; 32] = if let Some(validated) = local_resolution.as_ref() {
        match validated.lease_set2().usable_x25519_key() {
            Ok(key) => {
                let bytes = key.as_bytes();
                let mut out = [0_u8; 32];
                out.copy_from_slice(&bytes[..32]);
                out
            }
            Err(_) => {
                return Err(StreamConnectFailed {
                    result: ReplyResult::InvalidKey,
                    message: "local peer lease set has no X25519 key".to_owned(),
                });
            }
        }
    } else {
        match streams::decode_destination_triple(&destination) {
            Ok((_, _, static_key)) => static_key,
            Err(_) => {
                return Err(StreamConnectFailed {
                    result: ReplyResult::InvalidKey,
                    message: "could not decode DESTINATION".to_owned(),
                });
            }
        }
    };
    // Build the RemoteDestination for the StreamingManager.
    let remote = i2pr_client::streaming::manager::RemoteDestination {
        destination_hash,
        signing_public_key,
        static_public_key: static_public,
    };
    if let Some(validated) = local_resolution {
        // Clone for install (local_resolution was borrowed above for the
        // static key; re-resolve is unnecessary — move the owned value).
        let install = bridge.with(|bridge| {
            bridge
                .routing_mut()
                .install_remote_lease_set2(validated)
                .map(|_| ())
        });
        if let Err(error) = install {
            return Err(StreamConnectFailed {
                result: ReplyResult::I2pError,
                message: format!("local peer lease set install failed: {error}"),
            });
        }
    }

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
    //
    // Plan 147 §11: production SAM CONNECT uses the OS CSPRNG, never
    // a deterministic seed. The streaming manager's `connect` accepts
    // any `CryptoRng + RngCore`; we use the same `UnwrapMut(OsRng)`
    // wrap the runtime delivery path uses.
    let now_ms = streaming_now_ms();
    let local_port = from_port.unwrap_or(default_from_port);
    let remote_port = to_port.unwrap_or(default_to_port);
    let mut connect_outcome: Result<
        i2pr_client::streaming::manager::ConnectOutcome,
        i2pr_client::streaming::manager::StreamingManagerError,
    > = Ok(i2pr_client::streaming::manager::ConnectOutcome::ConnectionTableFull);
    let mut os_rng = OsRng;
    bridge.with(|bridge| {
        let local_identity = bridge.identity();
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
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
    // Plan 147 §7: kick the per-destination driver so it drains
    // the newly queued SYN immediately instead of waiting for the
    // driver ticker. The driver will notify the established signal
    // once the SYN response arrives back on the same manager.
    state.notify_outbound_signal(destination_id);
    let stream_id = connection_id.raw();

    // Plan 147 §7: STREAM CONNECT must observe the real
    // `ConnectionState::Established` transition before it returns
    // OK. The SYN was queued on the local outbound queue and the
    // per-destination driver task will pick it up and deliver it
    // through the Plan 143 `bridge_to_peer` seam. We park here on
    // the destination's established-signal until the manager
    // transitions to Established or the deadline expires.
    //
    // The deadline defaults to a bounded 30 seconds; production
    // callers that want a tighter limit must extend the limit
    // surface explicitly.
    let deadline = Duration::from_secs(20);
    let established_notify = state.established_signal(destination_id);
    let established = wait_for_established(
        state.clone(),
        destination_id,
        connection_id,
        established_notify,
        deadline,
    )
    .await;
    if !established {
        let _ = bridge.with(|bridge| bridge.streaming_mut().remove_connection(connection_id));
        return Err(StreamConnectFailed {
            result: ReplyResult::Timeout,
            message: format!("STREAM CONNECT did not reach Established within {deadline:?}"),
        });
    }

    let stream_id_value = attachment.stream_id;
    if state
        .stream_registry
        .attach_connection(&session_id, stream_id_value, connection_id)
        .is_err()
    {
        let _ = bridge.with(|bridge| bridge.streaming_mut().remove_connection(connection_id));
        return Err(StreamConnectFailed {
            result: ReplyResult::I2pError,
            message: "STREAM CONNECT attachment ownership was lost".to_owned(),
        });
    }
    let _ = state.stream_registry.update_state(
        &session_id,
        stream_id_value,
        SamStreamState::Established,
    );
    attachment.retain();
    Ok(StreamConnectApplied {
        stream_id: u32::try_from(stream_id).unwrap_or(u32::MAX),
        destination_id,
        session_id: session_id.clone(),
        connection_id,
        peer_destination: remote,
        silent: silent.unwrap_or(false),
    })
}

/// Plan 147 §7: parks the caller until `manager.get_connection(id)`
/// reports `ConnectionState::Established`, or returns `false` once
/// `deadline` expires. The destination's runtime driver notifies
/// this wait whenever a connection in the destination transitions to
/// Established; the loop also polls every `tick` so the wait does
/// not depend on a missed notify.
async fn wait_for_established(
    state: Arc<SamServiceState>,
    destination_id: DestinationId,
    connection_id: i2pr_client::streaming::connection::ConnectionId,
    notify: Arc<tokio::sync::Notify>,
    deadline: Duration,
) -> bool {
    let started = std::time::Instant::now();
    loop {
        let state_now = {
            let destinations_arc = state.sam_destinations();
            if let Ok(destinations) = destinations_arc.lock() {
                let canonical = destinations.get(destination_id).and_then(|handle| {
                    handle.with(|bridge| {
                        bridge
                            .streaming()
                            .get_connection(connection_id)
                            .map(|conn| conn.state())
                    })
                });
                let receiver = if canonical.is_none() {
                    destinations.get(destination_id).and_then(|handle| {
                        handle.with(|bridge| {
                            bridge
                                .receiver_streaming()
                                .get_connection(connection_id)
                                .map(|conn| conn.state())
                        })
                    })
                } else {
                    None
                };
                canonical.or(receiver)
            } else {
                None
            }
        };
        match state_now {
            Some(i2pr_client::streaming::connection::ConnectionState::Established) => {
                return true;
            }
            Some(
                i2pr_client::streaming::connection::ConnectionState::Closed
                | i2pr_client::streaming::connection::ConnectionState::Reset,
            )
            | None => return false,
            Some(_) => {}
        }
        let remaining = deadline.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return false;
        }
        let tick = remaining.min(Duration::from_millis(20));
        tokio::select! {
            biased;
            _ = notify.notified() => {}
            _ = tokio::time::sleep(tick) => {}
        }
    }
}

/// Executes a `STREAM ACCEPT` request after the HELLO handshake.
///
/// The function validates the session id, ensures a wildcard
/// Streaming listener is bound on the per-destination bridge, and
/// reports `STREAM STATUS RESULT=OK`. The actual inbound SYN
/// observation is driven by the self-composed local product fabric.
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

    let StreamAcceptRequest { session_id, silent } = request;

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
    if entry.style() != i2pr_api::sam::session_create::SessionCreateStyle::Stream {
        return Err(StreamAcceptFailed {
            result: ReplyResult::InvalidId,
            message: "STREAM ACCEPT requires a STREAM session or subsession ID".to_owned(),
        });
    }
    let destination_id = entry.destination_id();
    let listen_port = entry.listen_port();

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
    let attachment = StreamAttachmentLease {
        registry: Arc::clone(&state.stream_registry),
        session_id: session_id.clone(),
        stream_id: waiter.stream_id,
        release_on_drop: true,
    };

    // Plan 147 §7: bind the listener on the BRIDGE's receiver-mirror
    // `StreamingManager` (the manager that actually receives the
    // inbound SYN through the local seam), not on the SESSION CREATE
    // `streaming_pools` manager — those are two separate instances.
    let listener_result = {
        let destinations_arc = state.sam_destinations();
        let destinations = destinations_arc.lock().expect("sam destinations poisoned");
        destinations.get(destination_id).map(|handle| {
            handle.with(|bridge| {
                let manager = bridge.receiver_streaming_mut();
                manager.listen(listen_port)
            })
        })
    };
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

    // Plan 147 §7: STREAM ACCEPT must observe an inbound SYN, drive
    // the SYN response through `accept_inbound_syn`, and observe the
    // local connection transition to `Established` before returning
    // OK. Park here on the destination's established-signal until the
    // receiver mirror reaches Established or the deadline expires.
    let deadline = Duration::from_secs(20);
    let established_notify = state.established_signal(destination_id);
    state.notify_outbound_signal(destination_id);
    let (inbound_connection_id, peer_destination, peer_destination_b64) =
        match wait_for_accept_established(
            state.clone(),
            destination_id,
            listen_port,
            established_notify,
            deadline,
        )
        .await
        {
            Some(value) => value,
            None => {
                return Err(StreamAcceptFailed {
                    result: ReplyResult::Timeout,
                    message: format!(
                        "STREAM ACCEPT did not observe an inbound SYN within {deadline:?}"
                    ),
                });
            }
        };

    if !state
        .stream_registry
        .claim_pending_accept(&session_id, waiter.stream_id)
        .unwrap_or(false)
    {
        return Err(StreamAcceptFailed {
            result: ReplyResult::I2pError,
            message: "STREAM ACCEPT waiter was claimed or released unexpectedly".to_owned(),
        });
    }

    if state
        .stream_registry
        .attach_connection(&session_id, waiter.stream_id, inbound_connection_id)
        .is_err()
    {
        let destinations = state.sam_destinations();
        if let Ok(destinations) = destinations.lock()
            && let Some(bridge) = destinations.get(destination_id)
        {
            bridge.with(|bridge| {
                let _ = bridge
                    .receiver_streaming_mut()
                    .remove_connection(inbound_connection_id);
            });
        }
        return Err(StreamAcceptFailed {
            result: ReplyResult::I2pError,
            message: "STREAM ACCEPT attachment ownership was lost".to_owned(),
        });
    }

    let _ = state.stream_registry.update_state(
        &session_id,
        waiter.stream_id,
        SamStreamState::Established,
    );
    let peer_destination_b64 = peer_destination_b64
        .or_else(|| state.local_peer_public_destination(&peer_destination.destination_hash));
    attachment.retain();
    Ok(StreamAcceptApplied {
        stream_id: waiter.stream_id,
        destination_id,
        session_id: session_id.clone(),
        connection_id: inbound_connection_id,
        peer_destination,
        peer_destination_b64,
        silent: silent.unwrap_or(false),
    })
}

/// Plan 147 §7: parks the caller until the receiver mirror's
/// listener backlog on port 0 produces an inbound connection that
/// transitions to `ConnectionState::Established`. Returns the
/// connection id and the authenticated peer destination (extracted
/// from the inbound SYN) on success, `None` on timeout.
async fn wait_for_accept_established(
    state: Arc<SamServiceState>,
    destination_id: DestinationId,
    listen_port: u16,
    notify: Arc<tokio::sync::Notify>,
    deadline: Duration,
) -> Option<(
    i2pr_client::streaming::connection::ConnectionId,
    i2pr_client::streaming::manager::RemoteDestination,
    Option<String>,
)> {
    let started = std::time::Instant::now();
    let mut accepted: Option<(
        i2pr_client::streaming::connection::ConnectionId,
        i2pr_client::streaming::manager::RemoteDestination,
        Option<String>,
    )> = None;
    loop {
        let now_established: Option<(
            i2pr_client::streaming::connection::ConnectionId,
            i2pr_client::streaming::manager::RemoteDestination,
            Option<String>,
        )> = {
            // Plan 147: poll the BRIDGE's receiver-mirror
            // StreamingManager (installed in `sam_destinations`),
            // not the `streaming_pools` manager — those are two
            // separate instances.
            let destinations_arc = state.sam_destinations();
            let destinations = destinations_arc.lock().expect("sam destinations poisoned");
            let handle_opt = destinations.get(destination_id);
            let handle = match handle_opt {
                Some(h) => h,
                None => return None,
            };
            let mut os_rng = OsRng;
            handle.with(|bridge| {
                let local_identity = bridge.identity().clone();
                let mut rng = rand_core::UnwrapMut(&mut os_rng);
                let manager = bridge.receiver_streaming_mut();
                if let Some((cid, _, _)) = accepted.as_ref() {
                    let state = manager.get_connection(*cid).map(|c| c.state());
                    if matches!(
                        state,
                        Some(i2pr_client::streaming::connection::ConnectionState::Established)
                    ) {
                        return accepted.clone();
                    }
                    return None;
                }
                if manager.listener_backlog(listen_port) == 0 {
                    return None;
                }
                let cid = match manager.accept(listen_port) {
                    Some(cid) => cid,
                    None => return None,
                };
                let conn = match manager.get_connection(cid) {
                    Some(conn) => conn,
                    None => return None,
                };
                let remote_port = conn.remote_port();
                let local_port = conn.local_port();
                let peer_signing = conn.peer_signing_key().clone();
                let peer_hash = *conn.peer_destination_hash();
                let full_peer_destination = conn.peer_destination().cloned();
                let peer_static_public = full_peer_destination
                    .as_ref()
                    .and_then(|destination| destination.public_key().as_bytes().try_into().ok())
                    .unwrap_or([0_u8; 32]);
                let advertised = i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD;
                let request = match manager.accept_inbound_syn(
                    local_identity.as_ref(),
                    &i2pr_client::streaming::manager::RemoteDestination {
                        destination_hash: peer_hash,
                        signing_public_key: peer_signing.clone(),
                        static_public_key: peer_static_public,
                    },
                    cid,
                    local_port,
                    remote_port,
                    advertised,
                    streaming_now_ms(),
                    &mut rng,
                ) {
                    Ok(request) => request,
                    Err(_) => return None,
                };
                manager.queue_outbound_packet(request);
                let peer = i2pr_client::streaming::manager::RemoteDestination {
                    destination_hash: peer_hash,
                    signing_public_key: peer_signing,
                    static_public_key: peer_static_public,
                };
                let peer_destination_b64 = full_peer_destination
                    .as_ref()
                    .and_then(encode_destination_public);
                accepted = Some((cid, peer.clone(), peer_destination_b64.clone()));
                // Plan 147 §7: wake the per-destination driver so it
                // routes the just-queued SYN response back to the
                // peer through the local seam. Without this, the
                // ticker is only a fallback wake source.
                state.notify_outbound_signal(destination_id);
                Some((cid, peer, peer_destination_b64))
            })
        };
        if let Some((cid, peer, peer_destination_b64)) = now_established {
            // Re-check Established on a clean lock to avoid races.
            let destinations_arc = state.sam_destinations();
            let destinations = destinations_arc.lock().expect("sam destinations poisoned");
            let final_state = destinations.get(destination_id).and_then(|handle| {
                handle.with(|bridge| {
                    bridge
                        .receiver_streaming()
                        .get_connection(cid)
                        .map(|c| c.state())
                })
            });
            if matches!(
                final_state,
                Some(i2pr_client::streaming::connection::ConnectionState::Established)
            ) {
                return Some((cid, peer, peer_destination_b64));
            }
        }
        let remaining = deadline.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return None;
        }
        let tick = remaining.min(Duration::from_millis(20));
        tokio::select! {
            biased;
            _ = notify.notified() => {}
            _ = tokio::time::sleep(tick) => {}
        }
    }
}

/// Binds the receiver-mirror wildcard listener used by the supervised
/// FORWARD worker. FORWARD has no command-mode ACCEPT socket to perform this
/// setup, so the worker owns the listener lifecycle directly.
fn ensure_forward_listener(
    state: &SamServiceState,
    destination_id: DestinationId,
) -> Result<(), String> {
    let destinations_arc = state.sam_destinations();
    let destinations = destinations_arc
        .lock()
        .map_err(|_| "sam destinations poisoned".to_owned())?;
    match destinations.get(destination_id) {
        Some(handle) => handle.with(|bridge| match bridge.receiver_streaming_mut().listen(0) {
            Ok(_)
            | Err(i2pr_client::streaming::manager::StreamingManagerError::PortAlreadyInUse) => {
                Ok(())
            }
            Err(error) => Err(format!("listener bind failed: {error}")),
        }),
        None => Err("no streaming manager for destination".to_owned()),
    }
}

/// Parks a FORWARD worker until one inbound SYN is accepted and the receiver
/// mirror has completed its local half of the handshake. This is the same
/// authenticated SYN path as STREAM ACCEPT, but the worker retains the
/// connection rather than replying on a SAM command socket.
async fn wait_for_forward_established(
    state: Arc<SamServiceState>,
    destination_id: DestinationId,
    notify: Arc<tokio::sync::Notify>,
    cancellation: CancellationToken,
    deadline: Duration,
) -> Option<(
    i2pr_client::streaming::connection::ConnectionId,
    i2pr_client::streaming::manager::RemoteDestination,
    Option<String>,
)> {
    let started = Instant::now();
    let mut accepted: Option<(
        i2pr_client::streaming::connection::ConnectionId,
        i2pr_client::streaming::manager::RemoteDestination,
        Option<String>,
    )> = None;
    loop {
        if cancellation.is_cancelled() {
            return None;
        }
        let observed = {
            let destinations_arc = state.sam_destinations();
            let destinations = destinations_arc.lock().ok()?;
            let handle = destinations.get(destination_id)?;
            let mut os_rng = OsRng;
            handle.with(|bridge| {
                let local_identity = bridge.identity().clone();
                let mut rng = rand_core::UnwrapMut(&mut os_rng);
                let manager = bridge.receiver_streaming_mut();
                if let Some((connection_id, _, _)) = accepted.as_ref() {
                    if matches!(
                        manager
                            .get_connection(*connection_id)
                            .map(|connection| connection.state()),
                        Some(i2pr_client::streaming::connection::ConnectionState::Established)
                    ) {
                        return accepted.clone();
                    }
                    return None;
                }
                if manager.listener_backlog(0) == 0 {
                    return None;
                }
                let connection_id = manager.accept(0)?;
                let connection = manager.get_connection(connection_id)?;
                let remote_port = connection.remote_port();
                let local_port = connection.local_port();
                let peer_signing = connection.peer_signing_key().clone();
                let peer_hash = *connection.peer_destination_hash();
                let full_peer_destination = connection.peer_destination().cloned();
                let peer_static_public = full_peer_destination
                    .as_ref()
                    .and_then(|destination| destination.public_key().as_bytes().try_into().ok())
                    .unwrap_or([0_u8; 32]);
                let peer = i2pr_client::streaming::manager::RemoteDestination {
                    destination_hash: peer_hash,
                    signing_public_key: peer_signing,
                    static_public_key: peer_static_public,
                };
                let request = manager
                    .accept_inbound_syn(
                        local_identity.as_ref(),
                        &peer,
                        connection_id,
                        local_port,
                        remote_port,
                        i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
                        streaming_now_ms(),
                        &mut rng,
                    )
                    .ok()?;
                manager.queue_outbound_packet(request);
                let peer_destination_b64 = full_peer_destination
                    .as_ref()
                    .and_then(encode_destination_public);
                accepted = Some((connection_id, peer.clone(), peer_destination_b64.clone()));
                state.notify_outbound_signal(destination_id);
                Some((connection_id, peer, peer_destination_b64))
            })
        };
        if let Some((connection_id, peer, peer_destination_b64)) = observed {
            let destinations_arc = state.sam_destinations();
            let destinations = destinations_arc.lock().ok()?;
            let final_state = destinations.get(destination_id).and_then(|handle| {
                handle.with(|bridge| {
                    bridge
                        .receiver_streaming()
                        .get_connection(connection_id)
                        .map(|connection| connection.state())
                })
            });
            if matches!(
                final_state,
                Some(i2pr_client::streaming::connection::ConnectionState::Established)
            ) {
                return Some((connection_id, peer, peer_destination_b64));
            }
        }
        let remaining = deadline.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return None;
        }
        let tick = remaining.min(Duration::from_millis(20));
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return None,
            _ = notify.notified() => {}
            _ = tokio::time::sleep(tick) => {}
        }
    }
}

/// Creates two connected loopback sockets for the internal FORWARD bridge.
/// The sockets never leave this process: one is owned by the normal raw
/// STREAM driver and the other by the bounded local-target bridge.
async fn local_socket_pair() -> io::Result<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let address = listener.local_addr()?;
    let (connected, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
    let connected = connected?;
    let (accepted, _) = accepted?;
    Ok((accepted, connected))
}

/// Owns the runtime FORWARD path for one control socket. Each accepted
/// Streaming connection gets a bounded SAM attachment, the same raw byte
/// driver used by STREAM ACCEPT, and a local target bridge. The loop remains
/// alive for sequential independent streams until the FORWARD owner closes.
fn spawn_forward_worker(
    state: &Arc<SamServiceState>,
    registration: ForwardRegistration,
    children: &ChildScope,
    session_cancellation: CancellationToken,
) -> Result<(), i2pr_runtime::ChildScopeError> {
    let state = Arc::clone(state);
    children.spawn(move |task_cancellation| async move {
        run_forward_worker(state, registration, session_cancellation, task_cancellation).await;
        Ok(())
    })
}

async fn run_forward_worker(
    state: Arc<SamServiceState>,
    registration: ForwardRegistration,
    session_cancellation: CancellationToken,
    task_cancellation: CancellationToken,
) {
    debug!(session_id = %registration.session_id, "forward worker started");
    let Some(entry) = state.session_registry().get(&registration.session_id) else {
        warn!(session_id = %registration.session_id, "forward worker session disappeared");
        return;
    };
    let destination_id = entry.destination_id();
    if let Err(error) = ensure_forward_listener(&state, destination_id) {
        warn!(session_id = %registration.session_id, error, "forward listener setup failed");
        return;
    }
    let established_notify = state.established_signal(destination_id);
    loop {
        if session_cancellation.is_cancelled()
            || task_cancellation.is_cancelled()
            || state
                .forward_registration(&registration.session_id)
                .is_none_or(|current| current.owner != registration.owner)
        {
            break;
        }
        let attachment = match state
            .stream_registry()
            .register_forward_attachment(&registration.session_id, destination_id)
        {
            Ok(attachment) => attachment,
            Err(error) => {
                warn!(session_id = %registration.session_id, error = %error, "forward attachment allocation failed");
                break;
            }
        };
        let accepted = wait_for_forward_established(
            Arc::clone(&state),
            destination_id,
            Arc::clone(&established_notify),
            session_cancellation.clone(),
            Duration::from_secs(20),
        )
        .await;
        let Some((connection_id, peer_destination, peer_destination_b64)) = accepted else {
            let _ = state
                .stream_registry()
                .release_attachment(&registration.session_id, attachment.stream_id);
            break;
        };
        debug!(session_id = %registration.session_id, connection_id = connection_id.raw(), "forward worker accepted stream");
        let _ = state.stream_registry().update_state(
            &registration.session_id,
            attachment.stream_id,
            SamStreamState::Established,
        );
        let (raw_stream, bridge_stream) = match local_socket_pair().await {
            Ok(pair) => pair,
            Err(error) => {
                warn!(session_id = %registration.session_id, error = %error, "forward internal socket pair failed");
                let cleanup = RawStreamCleanup {
                    session_id: registration.session_id.clone(),
                    destination_id,
                    attachment_id: attachment.stream_id,
                    connection_id,
                    peer_destination: peer_destination.clone(),
                    direction: RawDirection::Inbound,
                };
                state.finish_raw_stream(cleanup, true);
                break;
            }
        };
        let handoff = RawStreamHandoff {
            stream: Box::new(raw_stream),
            session_id: registration.session_id.clone(),
            destination_id,
            attachment_id: attachment.stream_id,
            connection_id,
            peer_destination: peer_destination.clone(),
            initial_raw_bytes: Vec::new(),
            silent: registration.silent,
            direction: RawDirection::Inbound,
        };
        let cleanup = RawStreamCleanup {
            session_id: handoff.session_id.clone(),
            destination_id: handoff.destination_id,
            attachment_id: handoff.attachment_id,
            connection_id: handoff.connection_id,
            peer_destination: handoff.peer_destination.clone(),
            direction: handoff.direction,
        };
        let stream_cancellation = task_cancellation.child_token();
        let raw_future = run_raw_stream(Arc::clone(&state), handoff, stream_cancellation.clone());
        let bridge_future = state.bridge_forwarded_stream(
            &registration.session_id,
            bridge_stream,
            peer_destination_b64.as_deref(),
            stream_cancellation.clone(),
        );
        tokio::pin!(raw_future);
        tokio::pin!(bridge_future);
        let reset = tokio::select! {
            biased;
            _ = session_cancellation.cancelled() => {
                let _ = stream_cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
                let _ = raw_future.await;
                let _ = bridge_future.await;
                false
            }
            _ = task_cancellation.cancelled() => {
                let _ = stream_cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
                let _ = raw_future.await;
                let _ = bridge_future.await;
                false
            }
            raw_result = &mut raw_future => {
                let reset = raw_result.is_err();
                let _ = stream_cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
                let bridge_result = bridge_future.await;
                reset || bridge_result.is_err()
            }
            bridge_result = &mut bridge_future => {
                let reset = bridge_result.is_err();
                let _ = stream_cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
                let raw_result = raw_future.await;
                reset || raw_result.is_err()
            }
        };
        debug!(session_id = %registration.session_id, connection_id = connection_id.raw(), reset, "forward worker stream ended");
        state.finish_raw_stream(cleanup, reset);
    }
}

/// Plan 147 §8 step 5/6: per-destination runtime driver loop.
///
/// Wakes on the destination's outbound-signal `Notify`. Drains
/// `TransportSendRequest`s from both the canonical and
/// receiver-mirror `StreamingManager`s and routes each through the
/// Plan 129 local seam (`bridge_to_peer`) into the registered peer
/// bridge. Polls retransmits and acks on a fixed cadence. Wakes any
/// `await`er on the established-signal whenever a connection in this
/// destination has transitioned to
/// `ConnectionState::Established`.
async fn run_destination_driver(
    state: Arc<SamServiceState>,
    destination_id: DestinationId,
    cancellation: CancellationToken,
    task_cancellation: CancellationToken,
) {
    debug!(destination = ?destination_id, "destination driver starting");
    let outbound_notify = state.outbound_signal(destination_id);
    let established_notify = state.established_signal(destination_id);
    // Initial wake so the driver immediately drains anything that
    // was queued before the task was spawned.
    outbound_notify.notify_one();
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        // Yield first so a freshly-notified outbound signal isn't
        // immediately re-queued behind a single-threaded scheduler's
        // tick. The raw stream driver writes to the outbound queue
        // and then notifies; without the yield the runtime driver
        // may have already drained before the queue had the new
        // packet.
        tokio::task::yield_now().await;
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            _ = task_cancellation.cancelled() => break,
            _ = outbound_notify.notified() => {}
            _ = ticker.tick() => {}
        }
        let now_ms = streaming_now_ms();
        let mut sweep = state
            .deliver_outbound(destination_id, sam_now_seconds(), now_ms)
            .unwrap_or_default();
        // The bridge owns the two live managers. Poll both exactly once;
        // `poll_retransmits` and `poll_acks` enqueue their own requests.
        let bridge_established_now = state
            .sam_destinations()
            .lock()
            .ok()
            .and_then(|destinations| {
                destinations.get(destination_id).map(|handle| {
                    handle.with(|bridge| {
                        bridge.poll_streaming_timers(now_ms);
                        bridge.has_established_connection()
                    })
                })
            })
            .unwrap_or(false);
        let second_sweep = state
            .deliver_outbound(destination_id, sam_now_seconds(), now_ms)
            .unwrap_or_default();
        sweep.saturating_add_assign(second_sweep);
        state.record_delivery_counters(destination_id, sweep);
        state.forward_host_datagrams(destination_id);
        if let Some(reason) = degrade_to_reason(sweep) {
            debug!(
                destination = ?destination_id,
                delivered = sweep.delivered,
                missing_factory = sweep.missing_factory,
                factory_exhausted = sweep.factory_exhausted,
                unknown_peer = sweep.unknown_peer,
                delivery_failed = sweep.delivery_failed,
                reason = %reason,
                "destination driver observed typed local-delivery degradation"
            );
        }
        if bridge_established_now {
            established_notify.notify_one();
        }
        // Yield to the runtime so other tasks (raw stream drivers,
        // outbound notifications, established waits) get a fair
        // share of the single-threaded scheduler.
        tokio::task::yield_now().await;
    }
    if let Ok(mut drivers) = state.destination_drivers.lock() {
        drivers.remove(&destination_id);
    }
    debug!(destination = ?destination_id, "destination driver stopped");
}

/// Handles a `DEST GENERATE` command after HELLO has been negotiated.
/// Replies with the public-and-private DEST REPLY using the runtime
/// CSPRNG. Returns the same connection state.
async fn handle_dest_generate(
    state_conn: ServerConnectionState,
    stream: &mut SamIoStream,
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
mod private_connection_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn private_config() -> SamConfig {
        SamConfig {
            enabled: false,
            bind_address: "127.0.0.1".parse().expect("loopback IP"),
            port: 0,
            udp_port: 0,
            limits: SamLimits::loopback_test_profile(),
        }
    }

    async fn read_line(stream: &mut tokio::io::DuplexStream) -> String {
        let mut line = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).await.expect("reply byte");
            line.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        String::from_utf8(line).expect("SAM reply is UTF-8")
    }

    #[tokio::test]
    async fn private_connection_uses_sam_driver_without_listener_and_denies_forward() {
        let mut config = private_config();
        config.limits.hello_timeout = Duration::MAX;
        config.limits.command_timeout = Duration::MAX;
        let state = Arc::new(SamServiceState::new(config).expect("service state"));
        let parent = CancellationToken::new();
        let children = ChildScope::for_test(&parent, i2pr_runtime::ChildFailurePolicy::FailParent);
        let (mut client, router) = tokio::io::duplex(4096);
        let driver = tokio::spawn(Arc::clone(&state).drive_private_connection(
            Box::new(router),
            parent.child_token(),
            parent.clone(),
            children,
        ));

        client
            .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
            .await
            .expect("write HELLO");
        assert!(
            read_line(&mut client)
                .await
                .contains("HELLO REPLY RESULT=OK")
        );
        client
            .write_all(b"STREAM FORWARD ID=private PORT=1 HOST=127.0.0.1\n")
            .await
            .expect("write FORWARD");
        let reply = read_line(&mut client).await;
        assert!(
            reply.contains("RESULT=I2P_ERROR"),
            "unexpected reply: {reply}"
        );
        assert!(
            state
                .forwardings
                .lock()
                .expect("forwardings lock")
                .is_empty()
        );

        drop(client);
        driver
            .await
            .expect("connection task join")
            .expect("admission");
        assert!(
            state
                .forwardings
                .lock()
                .expect("forwardings lock")
                .is_empty()
        );
    }

    #[test]
    fn raw_handoff_accepts_in_memory_transport_type() {
        let (_client, router) = tokio::io::duplex(64);
        let _: SamIoStream = Box::new(router);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_api::sam::session_create::DestinationSource;

    async fn read_sam_line(stream: &mut tokio::net::TcpStream) -> String {
        use tokio::io::AsyncReadExt;
        let mut line = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).await.expect("SAM line byte");
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
        }
        String::from_utf8(line).expect("SAM reply is UTF-8")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn staged_33_profile_runs_primary_child_commands_over_loopback_tcp() {
        use tokio::io::AsyncWriteExt;

        let config = SamConfig {
            enabled: true,
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 0,
            udp_port: 0,
            limits: SamLimits::loopback_test_profile(),
        };
        let state = Arc::new(
            SamServiceState::new_with_max_supported_version(
                config,
                i2pr_api::sam::version::SamVersion::const_new(3, 3),
            )
            .expect("staged SAM 3.3 state"),
        );
        let (listener, address) = state.bind(state.bind_address()).await.expect("bind");
        let cancellation = CancellationToken::new();
        let children =
            ChildScope::for_test(&cancellation, i2pr_runtime::ChildFailurePolicy::FailParent);
        let serving_state = Arc::clone(&state);
        let serving_scope = children.clone();
        let serving_cancellation = cancellation.clone();
        children
            .spawn(move |_| async move {
                let _ = serving_state
                    .serve(listener, serving_scope, serving_cancellation)
                    .await;
                Ok(())
            })
            .expect("serve SAM listener");

        let mut control = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect control socket");
        control
            .write_all(b"HELLO VERSION MIN=3.1 MAX=3.3\n")
            .await
            .expect("HELLO");
        assert_eq!(
            read_sam_line(&mut control).await,
            "HELLO REPLY RESULT=OK VERSION=3.3"
        );
        control
            .write_all(b"SESSION CREATE STYLE=PRIMARY ID=primary DESTINATION=TRANSIENT\n")
            .await
            .expect("create primary");
        let created = read_sam_line(&mut control).await;
        assert!(created.starts_with("SESSION STATUS RESULT=OK DESTINATION="));
        assert_eq!(state.session_registry().session_count(), 1);

        control
            .write_all(b"SESSION ADD STYLE=STREAM ID=mail FROM_PORT=25 TO_PORT=110\n")
            .await
            .expect("add STREAM child");
        let added = read_sam_line(&mut control).await;
        assert_eq!(added, "SESSION STATUS RESULT=OK ID=\"mail\"");
        let primary = SamSessionId::new("primary").expect("primary id");
        let child = SamSessionId::new("mail").expect("child id");
        let primary_entry = state.session_registry().get(&primary).expect("primary");
        let child_entry = state.session_registry().get(&child).expect("child");
        assert_eq!(child_entry.destination_id(), primary_entry.destination_id());
        assert_eq!((child_entry.from_port(), child_entry.to_port()), (25, 110));

        control
            .write_all(b"SESSION REMOVE ID=mail\n")
            .await
            .expect("remove child");
        assert_eq!(
            read_sam_line(&mut control).await,
            "SESSION STATUS RESULT=OK ID=\"mail\""
        );
        assert!(state.session_registry().get(&child).is_none());
        assert!(state.session_registry().get(&primary).is_some());

        control
            .write_all(b"SESSION ADD STYLE=RAW ID=raw PORT=7655\n")
            .await
            .expect("add RAW child");
        assert_eq!(
            read_sam_line(&mut control).await,
            "SESSION STATUS RESULT=OK ID=\"raw\""
        );
        drop(control);
        for _ in 0..64 {
            if state.session_registry().session_count() == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(state.session_registry().session_count(), 0);
        assert!(state.destination_registry().lock().unwrap().is_empty());
        cancellation.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let _ = children.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn staged_33_stream_children_route_nonzero_and_maximum_ports_over_tcp() {
        use tokio::io::AsyncWriteExt;

        let state = Arc::new(
            SamServiceState::new_with_max_supported_version(
                SamConfig {
                    enabled: true,
                    bind_address: "127.0.0.1".parse().unwrap(),
                    port: 0,
                    udp_port: 0,
                    limits: SamLimits::loopback_test_profile(),
                },
                i2pr_api::sam::version::SamVersion::const_new(3, 3),
            )
            .expect("staged SAM 3.3 state"),
        );
        let (listener, address) = state.bind(state.bind_address()).await.expect("bind");
        let cancellation = CancellationToken::new();
        let children =
            ChildScope::for_test(&cancellation, i2pr_runtime::ChildFailurePolicy::FailParent);
        let serving_state = Arc::clone(&state);
        let serving_scope = children.clone();
        let serving_cancellation = cancellation.clone();
        children
            .spawn(move |_| async move {
                let _ = serving_state
                    .serve(listener, serving_scope, serving_cancellation)
                    .await;
                Ok(())
            })
            .expect("serve SAM listener");

        let mut peer = tokio::net::TcpStream::connect(address)
            .await
            .expect("peer control");
        peer.write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("peer HELLO");
        assert!(read_sam_line(&mut peer).await.contains("VERSION=3.3"));
        peer.write_all(b"SESSION CREATE STYLE=PRIMARY ID=peer DESTINATION=TRANSIENT\n")
            .await
            .expect("create peer primary");
        assert!(
            read_sam_line(&mut peer)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        peer.write_all(b"SESSION ADD STYLE=STREAM ID=pop FROM_PORT=110\n")
            .await
            .expect("add port 110 listener");
        assert!(read_sam_line(&mut peer).await.contains("ID=\"pop\""));
        peer.write_all(b"SESSION ADD STYLE=STREAM ID=max-listener FROM_PORT=65535\n")
            .await
            .expect("add maximum-port listener");
        assert!(
            read_sam_line(&mut peer)
                .await
                .contains("ID=\"max-listener\"")
        );
        peer.write_all(b"NAMING LOOKUP NAME=ME\n")
            .await
            .expect("lookup peer Destination");
        let naming = read_sam_line(&mut peer).await;
        let peer_public = naming
            .split_whitespace()
            .find_map(|field| field.strip_prefix("VALUE="))
            .expect("peer public Destination")
            .trim_matches('"')
            .to_owned();

        let mut primary = tokio::net::TcpStream::connect(address)
            .await
            .expect("primary control");
        primary
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("primary HELLO");
        assert!(read_sam_line(&mut primary).await.contains("VERSION=3.3"));
        primary
            .write_all(b"SESSION CREATE STYLE=PRIMARY ID=client DESTINATION=TRANSIENT\n")
            .await
            .expect("create client primary");
        assert!(
            read_sam_line(&mut primary)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        primary
            .write_all(b"SESSION ADD STYLE=STREAM ID=mail FROM_PORT=25\n")
            .await
            .expect("add mail child");
        assert!(read_sam_line(&mut primary).await.contains("ID=\"mail\""));
        primary
            .write_all(b"SESSION ADD STYLE=STREAM ID=max-client FROM_PORT=65535\n")
            .await
            .expect("add maximum-port client child");
        assert!(
            read_sam_line(&mut primary)
                .await
                .contains("ID=\"max-client\"")
        );

        let datagram_host = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("bind one shared host datagram endpoint");
        let host_port = datagram_host.local_addr().expect("host UDP port").port();
        let datagram_children = [
            ("dgram17", "DATAGRAM", 17_u8, 1017_u16),
            ("dgram19", "DATAGRAM2", 19_u8, 1019_u16),
            ("dgram20", "DATAGRAM3", 20_u8, 1020_u16),
            ("raw18", "RAW", 18_u8, 1018_u16),
            ("raw42", "RAW", 42_u8, 1042_u16),
        ];
        for (id, style, protocol, listen_port) in datagram_children {
            let extra = if style == "RAW" {
                format!(" PROTOCOL={protocol} LISTEN_PROTOCOL={protocol}")
            } else {
                String::new()
            };
            let command = format!(
                "SESSION ADD STYLE={style} ID={id} PORT={host_port} LISTEN_PORT={listen_port}{extra}\n"
            );
            primary
                .write_all(command.as_bytes())
                .await
                .expect("add shared-primary datagram child");
            assert!(
                read_sam_line(&mut primary)
                    .await
                    .contains(&format!("ID=\"{id}\"")),
                "add {style} child {id}"
            );
        }
        primary
            .write_all(b"NAMING LOOKUP NAME=ME\n")
            .await
            .expect("lookup shared PRIMARY Destination");
        let client_public = read_sam_line(&mut primary)
            .await
            .split_ascii_whitespace()
            .find_map(|field| field.strip_prefix("VALUE="))
            .expect("shared PRIMARY public Destination")
            .trim_matches('"')
            .to_owned();
        let primary_id = SamSessionId::new("client").expect("client primary ID");
        let destination_id = state
            .session_registry
            .get(&primary_id)
            .expect("client PRIMARY registry entry")
            .destination_id();
        let destination_hash = *destination_id.as_hash().as_bytes();
        for (id, _, _, _) in datagram_children {
            let entry = state
                .session_registry
                .get(&SamSessionId::new(id).expect("datagram child ID"))
                .expect("shared-primary datagram child");
            assert_eq!(entry.destination_id(), destination_id);
            assert_eq!(entry.parent_session_id(), Some(&primary_id));
        }

        // The child default TO_PORT is zero. Omitting the per-stream override
        // must not reach the listener on port 110.
        let mut omitted = tokio::net::TcpStream::connect(address)
            .await
            .expect("omitted-port socket");
        omitted
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("omitted-port HELLO");
        assert!(read_sam_line(&mut omitted).await.contains("VERSION=3.3"));
        let command = format!("STREAM CONNECT ID=mail DESTINATION={peer_public}\n");
        omitted
            .write_all(command.as_bytes())
            .await
            .expect("connect without TO_PORT");
        assert!(read_sam_line(&mut omitted).await.contains("RESULT=TIMEOUT"));

        async fn exchange_at_port(
            address: SocketAddr,
            child_id: &str,
            peer_id: &str,
            peer_public: &str,
            port: u16,
        ) {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut accept = tokio::net::TcpStream::connect(address)
                .await
                .expect("accept socket");
            accept
                .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
                .await
                .expect("accept HELLO");
            assert!(read_sam_line(&mut accept).await.contains("VERSION=3.3"));
            let accept_command = format!("STREAM ACCEPT ID={peer_id}\n");
            accept
                .write_all(accept_command.as_bytes())
                .await
                .expect("STREAM ACCEPT");

            let mut connect = tokio::net::TcpStream::connect(address)
                .await
                .expect("connect socket");
            connect
                .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
                .await
                .expect("connect HELLO");
            assert!(read_sam_line(&mut connect).await.contains("VERSION=3.3"));
            let connect_command =
                format!("STREAM CONNECT ID={child_id} DESTINATION={peer_public} TO_PORT={port}\n");
            connect
                .write_all(connect_command.as_bytes())
                .await
                .expect("STREAM CONNECT");
            let connect_status = read_sam_line(&mut connect).await;
            assert!(
                connect_status.starts_with("STREAM STATUS RESULT=OK"),
                "CONNECT failed at target port {port}: {connect_status}"
            );
            assert!(
                read_sam_line(&mut accept)
                    .await
                    .starts_with("STREAM STATUS RESULT=OK")
            );
            assert!(read_sam_line(&mut accept).await.starts_with("DESTINATION="));

            let payload = format!("port-{port}").into_bytes();
            connect.write_all(&payload).await.expect("send payload");
            let mut received = vec![0_u8; payload.len()];
            accept
                .read_exact(&mut received)
                .await
                .expect("receive payload");
            assert_eq!(received, payload);
        }

        exchange_at_port(address, "mail", "pop", &peer_public, 110).await;
        exchange_at_port(
            address,
            "max-client",
            "max-listener",
            &peer_public,
            u16::MAX,
        )
        .await;

        // Exercise all protocol children through the same live PRIMARY that
        // just completed the port-aware STREAM exchange. The real loopback UDP
        // bridge carries host sends; the canonical manager handles inbound
        // envelopes and forwards the selected child response to that endpoint.
        for (id, _style, protocol, _listen_port) in datagram_children {
            let payload = format!("same-primary-outbound-{protocol}").into_bytes();
            let protocol_option = if protocol == 18 || protocol == 42 {
                format!(" PROTOCOL={protocol}")
            } else {
                String::new()
            };
            let packet =
                format!("3.3 {id} {client_public} FROM_PORT=77 TO_PORT=88{protocol_option}\n");
            let packet = [packet.as_bytes(), payload.as_slice()].concat();
            state
                .handle_udp_bridge_packet(
                    &packet,
                    SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), host_port),
                )
                .unwrap_or_else(|error| panic!("route host UDP datagram through {id}: {error}"));
            let request = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    let request = state
                        .sam_destinations
                        .lock()
                        .unwrap()
                        .get(destination_id)
                        .expect("shared-primary destination")
                        .with(|bridge| bridge.datagrams_mut().drain_outbound())
                        .pop();
                    if let Some(request) = request {
                        break request;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("loopback SAM datagram reaches canonical owner");
            assert_eq!(request.destination_hash, destination_hash);
            assert_eq!((request.source_port, request.destination_port), (77, 88));
            let envelope =
                i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                    .expect("shared-primary I2CP datagram envelope");
            assert_eq!(envelope.protocol, protocol);
            if protocol == 18 || protocol == 42 {
                assert_eq!(envelope.payload, payload);
            } else {
                assert!(!envelope.payload.is_empty());
            }
        }

        let mut rng = i2pr_crypto::OsRng;
        let sender = DestinationIdentity::generate(&mut rng).expect("inbound datagram sender");
        for (_id, _style, protocol, listen_port) in datagram_children {
            let payload = format!("same-primary-inbound-{protocol}").into_bytes();
            let mut sender_manager = i2pr_client::datagram::DatagramManager::new();
            let transport = sender_manager
                .send(
                    &sender,
                    &i2pr_client::datagram::DatagramSendRequest {
                        destination_hash,
                        source_port: 99,
                        destination_port: listen_port,
                        protocol,
                        payload: payload.clone(),
                        options: None,
                    },
                )
                .expect("encode valid inbound protocol child payload");
            let envelope = i2pr_proto::streaming::decode_client_payload(
                &transport.application_payload,
                65_536,
            )
            .expect("decode inbound datagram envelope");
            state
                .sam_destinations
                .lock()
                .unwrap()
                .get(destination_id)
                .expect("shared-primary destination")
                .with(|bridge| {
                    bridge.datagrams_mut().process_inbound_at(
                        i2pr_client::datagram::DatagramInboundRequest {
                            protocol: envelope.protocol,
                            source_port: envelope.source_port,
                            destination_port: envelope.destination_port,
                            payload: &envelope.payload,
                            transport_sender: *sender.id().as_hash().as_bytes(),
                            recipient_hash: destination_hash,
                            now_ms: u64::from(protocol),
                            now_seconds: 1,
                        },
                    )
                })
                .expect("canonical manager accepts inbound protocol");
            state.forward_host_datagrams(destination_id);
            let mut received = vec![0_u8; 65_507];
            let (length, _) = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                datagram_host.recv_from(&mut received),
            )
            .await
            .expect("inbound datagram forwarded to shared host endpoint")
            .expect("receive forwarded datagram");
            let received = &received[..length];
            match protocol {
                17 | 19 | 20 => {
                    let newline = received.iter().position(|byte| *byte == b'\n').unwrap();
                    assert!(
                        received[..newline]
                            .windows(10)
                            .any(|window| window == b"FROM_PORT=")
                    );
                    assert!(received.ends_with(&payload));
                }
                18 | 42 => assert_eq!(received, payload),
                _ => unreachable!(),
            }
        }

        drop((peer, primary));
        for _ in 0..64 {
            if state.session_registry().session_count() == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(state.session_registry().session_count(), 0);
        cancellation.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let _ = children.shutdown().await;
    }

    #[test]
    fn sam_service_state_can_be_constructed_with_disabled_profile() {
        let config = SamConfig {
            enabled: false,
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 0,
            udp_port: 0,
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

    #[tokio::test]
    async fn udp_bridge_routes_raw_subsession_without_accepting_non_loopback_peers() {
        let state = Arc::new(
            SamServiceState::new(SamConfig {
                enabled: false,
                bind_address: "127.0.0.1".parse().unwrap(),
                port: 0,
                udp_port: 0,
                limits: SamLimits::loopback_test_profile(),
            })
            .expect("state"),
        );
        assert!(
            state.udp_bind_address().is_none(),
            "private SAM state has no UDP listener"
        );
        let parent = CancellationToken::new();
        let children = ChildScope::for_test(&parent, i2pr_runtime::ChildFailurePolicy::FailParent);
        let primary = SamSessionId::new("primary").unwrap();
        let applied = state
            .execute_session_create(SessionCreateExecution {
                session_id: primary.clone(),
                style: i2pr_api::sam::session_create::SessionCreateStyle::Primary,
                destination_source: DestinationSource::Transient,
                from_port: 0,
                to_port: 0,
                children: &children,
                cancellation: parent.child_token(),
            })
            .expect("create primary");
        let request = i2pr_api::sam::command::SessionAddRequest {
            id: "raw42".to_owned(),
            style: i2pr_api::sam::session_create::SessionCreateStyle::Raw,
            from_port: 7,
            to_port: 8,
            port: Some(7655),
            host: Some(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            raw_header: true,
            protocol: 42,
            listen_protocol: 43,
            listen_port: 9,
        };
        state
            .add_subsession(&primary, &request)
            .expect("add RAW child");
        let packet = format!(
            "3.3 raw42 {} FROM_PORT=11 TO_PORT=12 PROTOCOL=42\npayload",
            applied.public_destination_b64
        );
        state
            .handle_udp_bridge_packet(
                packet.as_bytes(),
                SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 7655),
            )
            .expect("route loopback UDP datagram");
        let destination_id = state
            .session_registry
            .get(&SamSessionId::new("raw42").unwrap())
            .expect("child entry")
            .destination_id();
        let outbound = state
            .sam_destinations
            .lock()
            .unwrap()
            .get(destination_id)
            .expect("bridge")
            .with(|bridge| bridge.datagrams_mut().drain_outbound());
        assert_eq!(outbound.len(), 1);
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&outbound[0].application_payload, 65_536)
                .expect("outbound I2CP payload");
        assert_eq!(envelope.protocol, 42);
        assert_eq!((envelope.source_port, envelope.destination_port), (11, 12));
        assert_eq!(envelope.payload, b"payload");

        let raw_d1 = i2pr_api::sam::command::SessionAddRequest {
            id: "raw17".to_owned(),
            style: i2pr_api::sam::session_create::SessionCreateStyle::Raw,
            from_port: 0,
            to_port: 0,
            port: Some(7656),
            host: None,
            raw_header: false,
            protocol: 42,
            listen_protocol: i2pr_client::datagram::DATAGRAM1_PROTOCOL,
            listen_port: 10,
        };
        state
            .add_subsession(&primary, &raw_d1)
            .expect("add RAW listener for protocol 17");
        let mut rng = i2pr_crypto::OsRng;
        let sender = DestinationIdentity::generate(&mut rng).expect("sender identity");
        let mut sender_datagrams = i2pr_client::datagram::DatagramManager::new();
        let outbound_d1 = sender_datagrams
            .send(
                &sender,
                &i2pr_client::datagram::DatagramSendRequest {
                    destination_hash: *applied.destination_id.as_hash().as_bytes(),
                    source_port: 30,
                    destination_port: 10,
                    protocol: i2pr_client::datagram::DATAGRAM1_PROTOCOL,
                    payload: b"private raw receiver".to_vec(),
                    options: None,
                },
            )
            .expect("encode protocol 17 message");
        let datagram1 =
            i2pr_proto::streaming::decode_client_payload(&outbound_d1.application_payload, 65_536)
                .expect("decode sender client payload");
        state
            .sam_destinations
            .lock()
            .unwrap()
            .get(applied.destination_id)
            .expect("receiver bridge")
            .with(|bridge| {
                bridge.datagrams_mut().process_inbound_at(
                    i2pr_client::datagram::DatagramInboundRequest {
                        protocol: datagram1.protocol,
                        source_port: datagram1.source_port,
                        destination_port: datagram1.destination_port,
                        payload: &datagram1.payload,
                        transport_sender: *sender.id().as_hash().as_bytes(),
                        recipient_hash: *applied.destination_id.as_hash().as_bytes(),
                        now_ms: 1,
                        now_seconds: 1,
                    },
                )
            })
            .expect("I2CP datagram payload accepted");
        let received = state
            .receive_managed_datagram(&primary, &SamSessionId::new("raw17").unwrap())
            .expect("private RAW receive operation")
            .expect("raw event");
        assert_eq!(received.protocol, i2pr_client::datagram::DATAGRAM1_PROTOCOL);
        assert_eq!(received.payload, datagram1.payload);
        assert!(!received.sender_authenticated);
        assert_eq!(received.from_hash, [0; 32]);

        let rejected = state.handle_udp_bridge_packet(
            packet.as_bytes(),
            SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 1)), 7655),
        );
        assert_eq!(rejected, Err("non-loopback UDP source"));
        state.teardown_session(&primary, applied.destination_id);
        parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let _ = children.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn primary_datagram_children_share_destination_and_route_protocols_17_through_20() {
        use i2pr_client::datagram::{
            DATAGRAM1_PROTOCOL, DATAGRAM2_PROTOCOL, DATAGRAM3_PROTOCOL, DatagramInboundRequest,
            DatagramManager, DatagramSendRequest, RAW_DATAGRAM_PROTOCOL,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let state = Arc::new(
            SamServiceState::new(SamConfig {
                enabled: false,
                bind_address: "127.0.0.1".parse().unwrap(),
                port: 0,
                udp_port: 0,
                limits: SamLimits::loopback_test_profile(),
            })
            .expect("state"),
        );
        let owner = CancellationToken::new();
        let children = ChildScope::for_test(&owner, i2pr_runtime::ChildFailurePolicy::FailParent);
        let primary_id = SamSessionId::new("primary").expect("primary ID");
        let primary = state
            .execute_session_create(SessionCreateExecution {
                session_id: primary_id.clone(),
                style: i2pr_api::sam::session_create::SessionCreateStyle::Primary,
                destination_source: DestinationSource::Transient,
                from_port: 0,
                to_port: 0,
                children: &children,
                cancellation: owner.child_token(),
            })
            .expect("create shared primary");
        let destination_hash = *primary.destination_id.as_hash().as_bytes();

        let child_specs = [
            (
                "dgram1",
                i2pr_api::sam::session_create::SessionCreateStyle::Datagram,
                DATAGRAM1_PROTOCOL,
                1017,
            ),
            (
                "dgram2",
                i2pr_api::sam::session_create::SessionCreateStyle::Datagram2,
                DATAGRAM2_PROTOCOL,
                1019,
            ),
            (
                "dgram3",
                i2pr_api::sam::session_create::SessionCreateStyle::Datagram3,
                DATAGRAM3_PROTOCOL,
                1020,
            ),
            (
                "raw18",
                i2pr_api::sam::session_create::SessionCreateStyle::Raw,
                RAW_DATAGRAM_PROTOCOL,
                1018,
            ),
            (
                "raw42",
                i2pr_api::sam::session_create::SessionCreateStyle::Raw,
                42,
                1042,
            ),
        ];
        for (id, style, protocol, listen_port) in child_specs {
            state
                .add_subsession(
                    &primary_id,
                    &i2pr_api::sam::command::SessionAddRequest {
                        id: id.to_owned(),
                        style,
                        from_port: 0,
                        to_port: 0,
                        port: Some(7655),
                        host: None,
                        raw_header: false,
                        protocol,
                        listen_protocol: protocol,
                        listen_port,
                    },
                )
                .unwrap_or_else(|error| panic!("add child {id}: {error}"));
            let entry = state
                .session_registry
                .get(&SamSessionId::new(id).expect("child ID"))
                .expect("child registration");
            assert_eq!(entry.destination_id(), primary.destination_id);
        }

        // Exercise the ordinary SAM UDP packet surface for each child and
        // confirm the canonical destination manager emits protocols 17–20
        // plus an opaque application-defined RAW protocol under one identity.
        let loopback = SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 45_678);
        for (id, _style, protocol, _listen_port) in child_specs {
            let payload = format!("outbound-{protocol}").into_bytes();
            let protocol_option = if protocol == RAW_DATAGRAM_PROTOCOL || protocol == 42 {
                format!(" PROTOCOL={protocol}")
            } else {
                String::new()
            };
            let packet = format!(
                "3.3 {id} {} FROM_PORT=77 TO_PORT=88{protocol_option}\n",
                primary.public_destination_b64
            );
            let packet = [packet.as_bytes(), payload.as_slice()].concat();
            state
                .handle_udp_bridge_packet(&packet, loopback)
                .unwrap_or_else(|error| panic!("send through {id}: {error}"));
            let request = state
                .sam_destinations
                .lock()
                .unwrap()
                .get(primary.destination_id)
                .expect("primary destination bridge")
                .with(|bridge| bridge.datagrams_mut().drain_outbound())
                .pop()
                .expect("canonical datagram outbound");
            assert_eq!(request.destination_hash, destination_hash);
            assert_eq!((request.source_port, request.destination_port), (77, 88));
            let envelope =
                i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                    .expect("I2CP datagram envelope");
            assert_eq!(envelope.protocol, protocol);
            assert_eq!((envelope.source_port, envelope.destination_port), (77, 88));
        }

        // Build valid protocol 17/19/20 envelopes from a different sender,
        // inject all protocols into the same canonical receive owner, then
        // prove each SAM child selects only its configured I2CP protocol and
        // destination port.
        let mut rng = i2pr_crypto::OsRng;
        let sender = DestinationIdentity::generate(&mut rng).expect("sender identity");
        for (_id, _style, protocol, listen_port) in child_specs {
            let payload = format!("inbound-{protocol}").into_bytes();
            let mut sender_manager = DatagramManager::new();
            let transport = sender_manager
                .send(
                    &sender,
                    &DatagramSendRequest {
                        destination_hash,
                        source_port: 99,
                        destination_port: listen_port,
                        protocol,
                        payload: payload.clone(),
                        options: None,
                    },
                )
                .unwrap_or_else(|error| panic!("encode protocol {protocol}: {error}"));
            let envelope = i2pr_proto::streaming::decode_client_payload(
                &transport.application_payload,
                65_536,
            )
            .expect("sender I2CP datagram envelope");
            state
                .sam_destinations
                .lock()
                .unwrap()
                .get(primary.destination_id)
                .expect("primary destination bridge")
                .with(|bridge| {
                    bridge
                        .datagrams_mut()
                        .process_inbound_at(DatagramInboundRequest {
                            protocol: envelope.protocol,
                            source_port: envelope.source_port,
                            destination_port: envelope.destination_port,
                            payload: &envelope.payload,
                            transport_sender: *sender.id().as_hash().as_bytes(),
                            recipient_hash: destination_hash,
                            now_ms: u64::from(protocol),
                            now_seconds: 1,
                        })
                })
                .unwrap_or_else(|error| panic!("accept protocol {protocol}: {error}"));
        }

        for (id, _style, protocol, _listen_port) in child_specs {
            let child_id = SamSessionId::new(id).expect("child ID");
            let received = state
                .receive_managed_datagram(&primary_id, &child_id)
                .unwrap_or_else(|error| panic!("receive through {id}: {error}"))
                .expect("child receives matching protocol and port");
            assert_eq!(received.protocol, protocol);
            assert_eq!(received.payload, format!("inbound-{protocol}").as_bytes());
            assert_eq!(
                received.destination_port,
                child_specs
                    .iter()
                    .find(|(candidate, ..)| *candidate == id)
                    .expect("child specification")
                    .3
            );
            assert_eq!(
                received.sender_authenticated,
                protocol == DATAGRAM1_PROTOCOL || protocol == DATAGRAM2_PROTOCOL
            );
        }
        assert_eq!(
            state
                .sam_destinations
                .lock()
                .unwrap()
                .get(primary.destination_id)
                .expect("one shared bridge")
                .with(|bridge| bridge.datagrams().queue_len()),
            0,
            "every inbound event is consumed by exactly its matching child"
        );

        async fn private_request(
            stream: &mut tokio::io::DuplexStream,
            request: i2pr_app_manager_proto::datagram::DatagramRequest,
        ) -> i2pr_app_manager_proto::datagram::DatagramReply {
            let bytes = request.encode().expect("encode private datagram request");
            stream
                .write_all(&(bytes.len() as u32).to_be_bytes())
                .await
                .expect("write private datagram length");
            stream
                .write_all(&bytes)
                .await
                .expect("write private datagram request");
            let mut length = [0_u8; 4];
            stream
                .read_exact(&mut length)
                .await
                .expect("read private datagram reply length");
            let mut reply = vec![0_u8; u32::from_be_bytes(length) as usize];
            stream
                .read_exact(&mut reply)
                .await
                .expect("read private datagram reply");
            i2pr_app_manager_proto::datagram::DatagramReply::decode(&reply)
                .expect("decode private datagram reply")
        }

        let (mut private_client, private_router) = tokio::io::duplex(16 * 1024);
        let private_state = Arc::clone(&state);
        let private_cancellation = owner.child_token();
        let private_service_cancellation = owner.child_token();
        let private_driver = tokio::spawn(async move {
            private_state
                .drive_private_datagram_connection(
                    Box::new(private_router),
                    private_cancellation,
                    private_service_cancellation,
                )
                .await;
        });
        let remote_destination = [0xA5; 32];
        for (id, _style, protocol, _listen_port) in child_specs {
            assert_eq!(
                private_request(
                    &mut private_client,
                    i2pr_app_manager_proto::datagram::DatagramRequest::Send {
                        primary_id: "primary".to_owned(),
                        child_id: id.to_owned(),
                        destination_hash: remote_destination,
                        source_port: 300,
                        destination_port: 400,
                        protocol,
                        options: Vec::new(),
                        payload: format!("private-outbound-{protocol}").into_bytes(),
                    },
                )
                .await,
                i2pr_app_manager_proto::datagram::DatagramReply::Sent,
                "private protocol {protocol} must use the child-scoped canonical sender"
            );
            let request = state
                .sam_destinations
                .lock()
                .unwrap()
                .get(primary.destination_id)
                .expect("shared primary bridge")
                .with(|bridge| bridge.datagrams_mut().drain_outbound())
                .pop()
                .expect("private request reached canonical datagram manager");
            assert_eq!(request.destination_hash, remote_destination);
            assert_eq!((request.source_port, request.destination_port), (300, 400));
            let envelope =
                i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                    .expect("private canonical I2CP envelope");
            assert_eq!(envelope.protocol, protocol);
            let mut outbound_receiver = DatagramManager::new();
            outbound_receiver
                .process_inbound_at(DatagramInboundRequest {
                    protocol: envelope.protocol,
                    source_port: envelope.source_port,
                    destination_port: envelope.destination_port,
                    payload: &envelope.payload,
                    transport_sender: destination_hash,
                    recipient_hash: remote_destination,
                    now_ms: u64::from(protocol) + 7_000,
                    now_seconds: 8,
                })
                .unwrap_or_else(|error| panic!("verify private outbound {protocol}: {error}"));
            let outbound_event = outbound_receiver
                .pop_received_for_listener(protocol, 400)
                .expect("private outbound envelope decodes at its I2CP recipient");
            assert!(
                outbound_event.payload == format!("private-outbound-{protocol}").as_bytes(),
                "private outbound payload mismatch for protocol {protocol}"
            );

            // Inject a valid signed/enveloped inbound message into the same
            // private destination and retrieve it through the typed service.
            let payload = format!("private-inbound-{protocol}").into_bytes();
            let mut sender_manager = DatagramManager::new();
            let transport = sender_manager
                .send(
                    &sender,
                    &DatagramSendRequest {
                        destination_hash,
                        source_port: 501,
                        destination_port: _listen_port,
                        protocol,
                        payload: payload.clone(),
                        options: None,
                    },
                )
                .unwrap_or_else(|error| panic!("encode private inbound {protocol}: {error}"));
            let incoming = i2pr_proto::streaming::decode_client_payload(
                &transport.application_payload,
                65_536,
            )
            .expect("private inbound I2CP envelope");
            state
                .sam_destinations
                .lock()
                .unwrap()
                .get(primary.destination_id)
                .expect("shared primary bridge")
                .with(|bridge| {
                    bridge
                        .datagrams_mut()
                        .process_inbound_at(DatagramInboundRequest {
                            protocol: incoming.protocol,
                            source_port: incoming.source_port,
                            destination_port: incoming.destination_port,
                            payload: &incoming.payload,
                            transport_sender: *sender.id().as_hash().as_bytes(),
                            recipient_hash: destination_hash,
                            now_ms: u64::from(protocol) + 5_000,
                            now_seconds: 6,
                        })
                })
                .unwrap_or_else(|error| panic!("accept private inbound {protocol}: {error}"));
            let event = match private_request(
                &mut private_client,
                i2pr_app_manager_proto::datagram::DatagramRequest::Receive {
                    primary_id: "primary".to_owned(),
                    child_id: id.to_owned(),
                },
            )
            .await
            {
                i2pr_app_manager_proto::datagram::DatagramReply::Event(event) => event,
                other => panic!("private receive for protocol {protocol}: {other:?}"),
            };
            assert_eq!(event.protocol, protocol);
            assert_eq!(event.source_port, 501);
            assert_eq!(event.destination_port, _listen_port);
            assert!(
                event.payload == payload,
                "private inbound payload mismatch for protocol {protocol}"
            );
            assert_eq!(
                event.sender_authenticated,
                protocol == DATAGRAM1_PROTOCOL || protocol == DATAGRAM2_PROTOCOL
            );
            if protocol == RAW_DATAGRAM_PROTOCOL || protocol == 42 {
                assert_eq!(event.from_hash, [0; 32]);
                assert!(!event.sender_authenticated);
            }
            assert!(event.options.is_empty());
        }
        drop(private_client);
        private_driver.await.expect("private datagram driver join");

        state.teardown_session(&primary_id, primary.destination_id);
        assert!(state.session_registry.session_count() == 0);
        assert!(state.destination_registry.lock().unwrap().is_empty());
        owner.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let _ = children.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the exact-pinned Java I2P 2.13.0 SAM client classpath"]
    async fn pinned_java_213_client_uses_sam33_primary_and_receives_datagram() {
        use i2pr_client::datagram::{
            DATAGRAM1_PROTOCOL, DatagramInboundRequest, DatagramManager, DatagramSendRequest,
        };
        use tokio::process::Command;

        const PRIMARY_ID: &str = "primarySink";
        const DATAGRAM_HOST_PORT: u16 = 9999;
        let classpath = std::env::var("I2PR_SAM_368_JAVA_CLIENT_CLASSPATH")
            .expect("Java lane supplies its exact-pinned SAM client classpath");
        let temp = tempfile::tempdir().expect("temporary Java client state");
        let key_path = temp.path().join("destination.txt");
        let sink_path = temp.path().join("sink");
        let stdout_path = temp.path().join("java.stdout");
        let stderr_path = temp.path().join("java.stderr");
        let java_log_path = temp.path().join("java.log");
        let java_log_property = format!("-DloggerFilenameOverride={}", java_log_path.display());
        std::fs::create_dir(&sink_path).expect("sink directory");

        let state = Arc::new(
            SamServiceState::new_with_max_supported_version(
                SamConfig {
                    enabled: true,
                    bind_address: "127.0.0.1".parse().unwrap(),
                    port: 0,
                    udp_port: 0,
                    limits: SamLimits::loopback_test_profile(),
                },
                i2pr_api::sam::version::SamVersion::const_new(3, 3),
            )
            .expect("staged SAM 3.3 state"),
        );
        let (listener, address) = state.bind(state.bind_address()).await.expect("bind");
        let cancellation = CancellationToken::new();
        let children =
            ChildScope::for_test(&cancellation, i2pr_runtime::ChildFailurePolicy::FailParent);
        let serving_state = Arc::clone(&state);
        let serving_scope = children.clone();
        let serving_cancellation = cancellation.clone();
        children
            .spawn(move |_| async move {
                let _ = serving_state
                    .serve(listener, serving_scope, serving_cancellation)
                    .await;
                Ok(())
            })
            .expect("serve staged SAM endpoint");

        let mut java = Command::new("java")
            .args([
                java_log_property.as_str(),
                "-cp",
                classpath.as_str(),
                "net.i2p.sam.client.SAMStreamSink",
                "-x",
                "-m",
                "1",
                "-b",
                "127.0.0.1",
                "-p",
            ])
            .arg(address.port().to_string())
            .arg(&key_path)
            .arg(&sink_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(
                std::fs::File::create(&stdout_path).expect("Java stdout log"),
            ))
            .stderr(std::process::Stdio::from(
                std::fs::File::create(&stderr_path).expect("Java stderr log"),
            ))
            .kill_on_drop(true)
            .spawn()
            .expect("start pinned Java I2P SAM client");

        let required_children = ["stream98", "dg97", "dg96", "raw94"];
        tokio::time::timeout(std::time::Duration::from_secs(45), async {
            loop {
                if let Some(status) = java.try_wait().expect("check Java client") {
                    let diagnostics = [&stdout_path, &stderr_path, &java_log_path]
                        .into_iter()
                        .filter_map(|path| std::fs::read(path).ok())
                        .flat_map(|bytes| {
                            String::from_utf8_lossy(&bytes)
                                .lines()
                                .take(24)
                                .map(|line| {
                                    line.split_ascii_whitespace()
                                        .map(|word| {
                                            if word.len() > 80 {
                                                "<redacted>"
                                            } else {
                                                word
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                        .join(" | ");
                    panic!("pinned Java SAM client exited before PRIMARY setup (status={status}); {diagnostics}");
                }
                let ids = state.session_registry.session_ids();
                if ids.iter().any(|id| id == PRIMARY_ID)
                    && required_children
                        .iter()
                        .all(|required| ids.iter().any(|id| id == required))
                    && !ids.iter().any(|id| id == "stream99" || id == "raw95")
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("Java 2.13.0 PRIMARY and child command matrix completes");

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let sink_ready = std::fs::read_dir(&sink_path)
                    .expect("read Java sink directory")
                    .next()
                    .is_some();
                if key_path.is_file() && sink_ready {
                    break;
                }
                if java.try_wait().expect("check Java client").is_some() {
                    panic!("pinned Java SAM client exited after session setup");
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("Java NAMING LOOKUP NAME=ME and Datagram1 receiver startup");

        let primary_id = SamSessionId::new(PRIMARY_ID).expect("primary ID");
        let (datagram_child, destination_id, destination_hash, destination, listen_port) = state
            .session_registry
            .with_entries(|sessions| {
                sessions
                    .iter()
                    .find(|(_, entry)| {
                        entry.parent_session_id() == Some(&primary_id)
                            && entry.style()
                                == i2pr_api::sam::session_create::SessionCreateStyle::Datagram
                            && entry.host_port() == Some(DATAGRAM_HOST_PORT)
                    })
                    .map(|(id, entry)| {
                        (
                            id.as_str().to_owned(),
                            entry.destination_id(),
                            *entry.destination_id().as_hash().as_bytes(),
                            entry.public_destination_b64().to_owned(),
                            entry.listen_port(),
                        )
                    })
            })
            .expect("session registry snapshot")
            .expect("Java DATAGRAM child with its loopback receive port");

        let mut rng = i2pr_crypto::OsRng;
        let sender = DestinationIdentity::generate(&mut rng).expect("datagram sender");
        let payload = b"pinned-java-sam33-datagram";
        let mut sender_manager = DatagramManager::new();
        let transport = sender_manager
            .send(
                &sender,
                &DatagramSendRequest {
                    destination_hash,
                    source_port: 77,
                    destination_port: listen_port,
                    protocol: DATAGRAM1_PROTOCOL,
                    payload: payload.to_vec(),
                    options: None,
                },
            )
            .expect("encode authenticated Java-bound Datagram1");
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&transport.application_payload, 65_536)
                .expect("decode Java-bound I2CP payload");
        state
            .sam_destinations
            .lock()
            .unwrap()
            .get(destination_id)
            .expect("Java destination owner")
            .with(|bridge| {
                bridge
                    .datagrams_mut()
                    .process_inbound_at(DatagramInboundRequest {
                        protocol: envelope.protocol,
                        source_port: envelope.source_port,
                        destination_port: envelope.destination_port,
                        payload: &envelope.payload,
                        transport_sender: *sender.id().as_hash().as_bytes(),
                        recipient_hash: destination_hash,
                        now_ms: 1,
                        now_seconds: 1,
                    })
            })
            .expect("accept valid Java-bound datagram");
        state.forward_host_datagrams(destination_id);

        let java_received = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let found = std::fs::read_dir(&sink_path)
                    .expect("read Java sink directory")
                    .filter_map(Result::ok)
                    .any(|entry| std::fs::read(entry.path()).is_ok_and(|bytes| bytes == payload));
                if found {
                    break;
                }
                if java.try_wait().expect("check Java client").is_some() {
                    panic!("pinned Java client stopped before receiving datagram");
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await;
        if java_received.is_err() {
            let sink_sizes = std::fs::read_dir(&sink_path)
                .expect("read Java sink directory")
                .filter_map(Result::ok)
                .filter_map(|entry| entry.metadata().ok().map(|metadata| metadata.len()))
                .collect::<Vec<_>>();
            let diagnostic = [&stdout_path, &stderr_path, &java_log_path]
                .into_iter()
                .filter_map(|path| std::fs::read(path).ok())
                .flat_map(|bytes| {
                    String::from_utf8_lossy(&bytes)
                        .lines()
                        .filter(|line| line.contains("ERROR") || line.contains("WARN"))
                        .take(12)
                        .map(|line| {
                            line.split_ascii_whitespace()
                                .map(|word| if word.len() > 80 { "<redacted>" } else { word })
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .join(" | ");
            panic!(
                "pinned Java client did not receive datagram: sessions={}, sink_file_sizes={sink_sizes:?}, java_alive={}, {diagnostic}",
                state.session_registry.session_count(),
                java.try_wait().expect("check Java client").is_none(),
            );
        }

        java.kill().await.expect("stop pinned Java SAM client");
        let _ = java.wait().await;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while state.session_registry.session_count() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Java PRIMARY owner loss tears down all children");
        assert!(state.destination_registry.lock().unwrap().is_empty());
        cancellation.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let _ = children.shutdown().await;
        let _ = datagram_child;
        let _ = destination;
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the exact-pinned Java I2P 2.13.0 SAM client classpath"]
    async fn pinned_java_213_primary_datagrams_and_stream_share_one_destination() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        const PRIMARY_ID: &str = "javaProbe";
        let classpath = std::env::var("I2PR_SAM_368_JAVA_CLIENT_CLASSPATH")
            .expect("Java lane supplies its exact-pinned SAM client classpath");
        let state = Arc::new(
            SamServiceState::new_with_max_supported_version(
                SamConfig {
                    enabled: true,
                    bind_address: "127.0.0.1".parse().unwrap(),
                    port: 0,
                    udp_port: 0,
                    limits: SamLimits::loopback_test_profile(),
                },
                i2pr_api::sam::version::SamVersion::const_new(3, 3),
            )
            .expect("staged SAM 3.3 state"),
        );
        let (listener, address) = state.bind(state.bind_address()).await.expect("bind");
        let cancellation = CancellationToken::new();
        let children =
            ChildScope::for_test(&cancellation, i2pr_runtime::ChildFailurePolicy::FailParent);
        let serving_state = Arc::clone(&state);
        let serving_scope = children.clone();
        let serving_cancellation = cancellation.clone();
        children
            .spawn(move |_| async move {
                let _ = serving_state
                    .serve(listener, serving_scope, serving_cancellation)
                    .await;
                Ok(())
            })
            .expect("serve staged SAM endpoint");

        let mut java = Command::new("java")
            .args(["-cp", classpath.as_str(), "Sam368PrimaryProbe", "127.0.0.1"])
            .arg(address.port().to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("start Java SAM wire probe");
        let stdout = java.stdout.take().expect("Java probe stdout");
        let mut stdout = BufReader::new(stdout);
        let mut ready = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            stdout.read_line(&mut ready),
        )
        .await
        .expect("Java primary child matrix deadline")
        .expect("read Java probe status");
        if ready.is_empty() {
            let status = java.wait().await.expect("wait for failed Java probe");
            let mut diagnostic = String::new();
            if let Some(mut stderr) = java.stderr.take() {
                stderr
                    .read_to_string(&mut diagnostic)
                    .await
                    .expect("read sanitized Java probe diagnostic");
            }
            panic!("Java probe exited before readiness ({status}): {diagnostic}");
        }
        assert_eq!(ready.trim(), "java_sam33_primary_child_matrix=passed");

        let primary_id = SamSessionId::new(PRIMARY_ID).expect("primary ID");
        let expected = [
            (
                "stream",
                i2pr_api::sam::session_create::SessionCreateStyle::Stream,
            ),
            (
                "datagram",
                i2pr_api::sam::session_create::SessionCreateStyle::Datagram,
            ),
            (
                "datagram2",
                i2pr_api::sam::session_create::SessionCreateStyle::Datagram2,
            ),
            (
                "datagram3",
                i2pr_api::sam::session_create::SessionCreateStyle::Datagram3,
            ),
        ];
        state
            .session_registry
            .with_entries(|sessions| {
                let primary = sessions
                    .iter()
                    .find(|(id, _)| id.as_str() == PRIMARY_ID)
                    .expect("Java-created PRIMARY");
                for (id, style) in expected {
                    let child = sessions
                        .iter()
                        .find(|(session_id, _)| session_id.as_str() == id)
                        .unwrap_or_else(|| panic!("Java-created {id} child"));
                    assert_eq!(child.1.style(), style, "Java child {id} style");
                    assert_eq!(
                        child.1.parent_session_id(),
                        Some(&primary_id),
                        "Java child {id} belongs to the one PRIMARY"
                    );
                    assert_eq!(
                        child.1.destination_id(),
                        primary.1.destination_id(),
                        "Java child {id} shares the PRIMARY Destination"
                    );
                }
                assert!(
                    !sessions.iter().any(|(id, entry)| {
                        id.as_str() == "raw"
                            && entry.style()
                                == i2pr_api::sam::session_create::SessionCreateStyle::Raw
                    }),
                    "Java SESSION REMOVE removed the RAW child"
                );
            })
            .expect("Java primary child registry snapshot");

        let destination_id = state
            .session_registry
            .get(&primary_id)
            .expect("Java PRIMARY destination")
            .destination_id();
        let destination_hash = *destination_id.as_hash().as_bytes();
        let mut rng = i2pr_crypto::OsRng;
        let sender = DestinationIdentity::generate(&mut rng).expect("Java datagram sender");
        for (protocol, destination_port) in [(17_u8, 18117_u16), (19, 18119), (20, 18120)] {
            let payload = format!("java-protocol-{protocol}").into_bytes();
            let mut manager = i2pr_client::datagram::DatagramManager::new();
            let transport = manager
                .send(
                    &sender,
                    &i2pr_client::datagram::DatagramSendRequest {
                        destination_hash,
                        source_port: 99,
                        destination_port,
                        protocol,
                        payload,
                        options: None,
                    },
                )
                .expect("encode Java-bound datagram");
            let envelope = i2pr_proto::streaming::decode_client_payload(
                &transport.application_payload,
                65_536,
            )
            .expect("decode Java-bound I2CP payload");
            state
                .sam_destinations
                .lock()
                .unwrap()
                .get(destination_id)
                .expect("Java shared datagram owner")
                .with(|bridge| {
                    bridge.datagrams_mut().process_inbound_at(
                        i2pr_client::datagram::DatagramInboundRequest {
                            protocol: envelope.protocol,
                            source_port: envelope.source_port,
                            destination_port: envelope.destination_port,
                            payload: &envelope.payload,
                            transport_sender: *sender.id().as_hash().as_bytes(),
                            recipient_hash: destination_hash,
                            now_ms: u64::from(protocol),
                            now_seconds: 1,
                        },
                    )
                })
                .expect("queue Java-bound datagram on the shared owner");
        }
        state.forward_host_datagrams(destination_id);
        let mut datagram_status = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            stdout.read_line(&mut datagram_status),
        )
        .await
        .expect("Java DATAGRAM1/2/3 receive deadline")
        .expect("read Java datagram status");
        assert_eq!(
            datagram_status.trim(),
            "java_sam33_datagram_17_19_20_receive=passed"
        );

        // A second primary supplies the remote STREAM listener. The Java
        // created STREAM child is selected by its ID on a fresh SAM data
        // connection, exactly as the PRIMARY control/data split requires.
        let mut peer_control = TcpStream::connect(address)
            .await
            .expect("peer PRIMARY control");
        peer_control
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("peer HELLO");
        assert!(
            read_sam_line(&mut peer_control)
                .await
                .contains("VERSION=3.3")
        );
        peer_control
            .write_all(b"SESSION CREATE STYLE=PRIMARY ID=javaPeer DESTINATION=TRANSIENT\n")
            .await
            .expect("create peer PRIMARY");
        assert!(
            read_sam_line(&mut peer_control)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        peer_control
            .write_all(b"SESSION ADD STYLE=STREAM ID=javaPeerStream FROM_PORT=18111\n")
            .await
            .expect("add peer STREAM child");
        assert!(
            read_sam_line(&mut peer_control)
                .await
                .contains("javaPeerStream")
        );
        peer_control
            .write_all(b"NAMING LOOKUP NAME=ME\n")
            .await
            .expect("lookup peer Destination");
        let peer_public = read_sam_line(&mut peer_control)
            .await
            .split_ascii_whitespace()
            .find_map(|field| field.strip_prefix("VALUE="))
            .expect("peer public Destination")
            .trim_matches('"')
            .to_owned();

        let mut accept = TcpStream::connect(address)
            .await
            .expect("peer STREAM accept control");
        accept
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("accept HELLO");
        assert!(read_sam_line(&mut accept).await.contains("VERSION=3.3"));
        accept
            .write_all(b"STREAM ACCEPT ID=javaPeerStream\n")
            .await
            .expect("accept Java primary child stream");

        let mut connect = TcpStream::connect(address)
            .await
            .expect("Java child STREAM connect control");
        connect
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("connect HELLO");
        assert!(read_sam_line(&mut connect).await.contains("VERSION=3.3"));
        let connect_command =
            format!("STREAM CONNECT ID=stream DESTINATION={peer_public} TO_PORT=18111\n");
        connect
            .write_all(connect_command.as_bytes())
            .await
            .expect("connect through Java PRIMARY child");
        assert!(
            read_sam_line(&mut connect)
                .await
                .starts_with("STREAM STATUS RESULT=OK")
        );
        assert!(
            read_sam_line(&mut accept)
                .await
                .starts_with("STREAM STATUS RESULT=OK")
        );
        assert!(read_sam_line(&mut accept).await.starts_with("DESTINATION="));
        let stream_payload = b"java-primary-child-stream-roundtrip";
        connect
            .write_all(stream_payload)
            .await
            .expect("send Java child stream bytes");
        let mut stream_received = vec![0_u8; stream_payload.len()];
        accept
            .read_exact(&mut stream_received)
            .await
            .expect("receive Java child stream bytes");
        assert_eq!(stream_received, stream_payload);
        drop((accept, connect, peer_control));

        let mut stdin = java.stdin.take().expect("Java probe control input");
        stdin.write_all(b"close\n").await.expect("close Java probe");
        drop(stdin);
        let status = tokio::time::timeout(std::time::Duration::from_secs(10), java.wait())
            .await
            .expect("Java probe exits after primary close")
            .expect("wait for Java probe");
        assert!(status.success(), "Java probe process exits successfully");
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while state.session_registry.session_count() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Java PRIMARY control loss removes remaining children");
        assert!(state.destination_registry.lock().unwrap().is_empty());
        cancellation.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let _ = children.shutdown().await;
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

    #[test]
    fn plan294_naming_lookup_consults_the_canonical_owner() {
        use crate::addressbook::{AddressBookManager, AddressBookSubsystemConfig};
        use i2pr_api::sam::naming::NamingLookupRequest;
        let config = SamConfig {
            enabled: false,
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 0,
            udp_port: 0,
            limits: SamLimits::defaults(),
        };
        let state = SamServiceState::new(config).expect("state");
        let lookup = |name: &str| {
            execute_naming_lookup(
                &state,
                ServerConnectionState::AwaitHello,
                NamingLookupRequest {
                    name: name.to_owned(),
                    include_lease_set_options: false,
                },
            )
        };
        // Inactive subsystem: ordinary `.i2p` names stay KeyNotFound.
        assert!(lookup("absent.i2p").is_err());
        // Active subsystem: committed entries resolve through the same
        // owner the control plane mutates.
        let directory = tempfile::tempdir().expect("temp directory");
        let manager = AddressBookManager::activate(AddressBookSubsystemConfig {
            enabled: true,
            state_dir: directory.path().join("addressbook"),
        });
        assert!(manager.is_active());
        let mut bytes = vec![0u8; 384];
        bytes.extend_from_slice(&[5u8, 0, 4, 0, 7, 0, 4]);
        let destination = i2pr_api::sam::base64::encode(&bytes);
        manager
            .apply_entry(i2pr_addressbook::EntryMutation {
                book: i2pr_addressbook::BookKind::Local,
                hostname: "sam-peer.i2p".to_owned(),
                destination: Some(destination.clone()),
                delete: false,
            })
            .expect("entry");
        state.set_addressbook_handle(manager.shared());
        let applied = lookup("sam-peer.i2p").expect("address-book hit");
        assert_eq!(applied.value, destination);
        // Case and trailing-dot forms canonicalize identically.
        let applied = lookup("SAM-PEER.I2P.").expect("canonical hit");
        assert_eq!(applied.value, destination);
        // Non-book names and non-`.i2p` names keep their verdicts.
        assert!(lookup("missing.i2p").is_err());
        assert!(lookup("not-a-name").is_err());
    }
}
