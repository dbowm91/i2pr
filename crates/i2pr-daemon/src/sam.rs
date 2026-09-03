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
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::SamConfig;

pub mod fabric;
pub mod raw_stream;
pub mod streams;
pub use fabric::{
    DeliverySweepCounters, LocalDeliveryDegradation, LocalDestinationProduct,
    LocalTransportRequest, SamLocalProductFabric, degrade_to_reason,
};
pub(crate) use raw_stream::RawStreamCleanup;
pub use raw_stream::{
    RawDirection, RawStreamError, RawStreamHandoff, RawStreamHandoffResolved, RawStreamOutcome,
    run_raw_stream,
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
        let outbound_notify = Arc::new(Mutex::new(HashMap::new()));
        let established_notify = Arc::new(Mutex::new(HashMap::new()));
        let destination_drivers = Arc::new(Mutex::new(HashMap::new()));
        let delivery_counters = Arc::new(Mutex::new(HashMap::new()));
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
            outbound_notify,
            established_notify,
            destination_drivers,
            delivery_counters,
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

    /// Returns the latest typed delivery-sweep counters for a
    /// destination. Payloads and peer identities are never retained.
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
            entries.insert(destination_id, counters);
        }
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
    pub fn execute_session_create(
        self: &Arc<Self>,
        session_id: SamSessionId,
        destination_source: i2pr_api::sam::session_create::DestinationSource,
        children: &ChildScope,
        cancellation: CancellationToken,
    ) -> Result<SessionCreateApplied, SessionCreateError> {
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
            .reserve_session(session_id.clone(), destination_id)
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
        if let Err(error) = self.stream_registry.register_session(session_id.clone()) {
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
        })
    }

    /// Tears a session down exactly once. Called from the
    /// control-socket teardown path and from the supervisor shutdown
    /// path. Idempotent.
    pub fn teardown_session(&self, session_id: &SamSessionId, destination_id: DestinationId) {
        if let Ok(mut drivers) = self.destination_drivers.lock()
            && let Some(driver) = drivers.remove(&destination_id)
        {
            let _ = driver.cancel(i2pr_core::CancellationReason::ParentScope);
        }
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
        self.drop_destination_signals(destination_id);
        if let Ok(mut counters) = self.delivery_counters.lock() {
            counters.remove(&destination_id);
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
                    if let Err(error) = children_for_task.clone().spawn(move |task_cancellation| {
                        let _permit = permit_for_task;
                        let raw_scope = children_for_task.clone();
                        async move {
                            handle_connection(
                                state,
                                stream,
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
}

fn encode_public_for(identity: &DestinationIdentity) -> String {
    let wrapper =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(identity)
            .expect("identity is fresh and should round-trip through the SAM codec");
    wrapper.encode_public_base64()
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
    mut stream: TcpStream,
    cancellation: CancellationToken,
    service_cancellation: CancellationToken,
    raw_children: ChildScope,
) {
    let peer_ip = stream
        .peer_addr()
        .map(|address| address.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
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
                    true,
                    peer_ip,
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
                        false,
                        peer_ip,
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

#[allow(clippy::too_many_arguments)]
async fn dispatch_command(
    state: Arc<SamServiceState>,
    state_conn: ServerConnectionState,
    outcome: &CommandOutcome,
    stream: &mut TcpStream,
    _is_first: bool,
    peer_ip: IpAddr,
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

    let dispatch = dispatch_command_state(state_conn.clone(), outcome);
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
            if !matches!(state_conn, ServerConnectionState::UtilityReady) {
                let reply = Reply::Session(SessionStatus::error(
                    ReplyResult::I2pError,
                    Some("SESSION CREATE before HELLO".to_owned()),
                ));
                write_reply(stream, &reply).await?;
                return Ok(ConnectionDisposition::Continue {
                    next_state: state_conn,
                });
            }
            let apply_result = match state.execute_session_create(
                id,
                request.destination,
                raw_children,
                session_cancellation,
            ) {
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
            let mut outcome = execute_stream_forward(&state, request, peer_ip, connection_owner);
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
            let outcome = execute_naming_lookup(&state, state_conn.clone(), request);
            let outcome = apply_naming_lookup_outcome(outcome);
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
    stream: &mut TcpStream,
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

    // Plan 149 §7: when the target destination is locally owned, resolve
    // the peer's validated LeaseSet2 through the SAM service's local
    // directory and install it into the sender's routing state. This is
    // the production analog of the Plan 147 test's manual cross-install.
    // We never call the private `install_remote_lease_set2` from a test
    // here; the SAM service owns the local directory and only hands out
    // records it has validated itself. Unknown remote destinations skip
    // this path entirely.
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
    if let Some(validated) = local_resolution {
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
    let local_port: u16 = 0;
    let remote_port: u16 = 0;
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
                manager.listen(0)
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
                if manager.listener_backlog(0) == 0 {
                    return None;
                }
                let cid = match manager.accept(0) {
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
            stream: raw_stream,
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
