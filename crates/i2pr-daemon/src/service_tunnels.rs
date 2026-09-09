//! Plan 175 Milestone 10 generic client/server service tunnels.
//!
//! The service tunnel manager is the Plan 175 daemon composition root
//! for the bounded, disabled-by-default, loopback-only
//! `generic-client` and `generic-server` tunnels defined by the
//! runtime-neutral `i2pr-service-tunnels` configuration surface.
//!
//! ```text
//! generic client:
//! loopback TCP -> I2P Streaming -> configured remote destination
//!
//! generic server:
//! local I2P destination -> I2P Streaming -> loopback TCP target
//! ```
//!
//! No HTTP, SOCKS, IRC, or other protocol parser exists here. Generic
//! tunnels forward bytes unchanged; higher profiles (HTTP/SOCKS/IRC)
//! belong to Plans 176-179.
//!
//! Plan 175 composes over the existing Plan 149 local destination
//! product path (`SamLocalProductFabric`) plus the Plan 174 shared
//! socket <-> Streaming pump (`run_stream_pump`). Every service
//! destination is a router-owned destination runtime; server tunnels
//! persist their identity through `i2pr-storage`'s
//! `ServiceDestinationStore` so restarts do not rotate the
//! destination, while client tunnels use ephemeral CSPRNG identities.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, ListenerOutcome, RemoteDestination,
};
use i2pr_client::{
    DestinationConfig, DestinationId, DestinationIdentity, DestinationOutboundRole,
    DestinationRegistry, DestinationRuntime, DestinationShutdown, RegistryConfig,
};
use i2pr_crypto::{IDENTITY_PADDING_LENGTH, OsRng, PRIVATE_KEY_LENGTH, X25519_KEY_LENGTH};
use i2pr_netdb::ValidatedLeaseSet2;
use i2pr_proto::{Destination, LeaseSet2};
use i2pr_runtime::{CancellationToken, ChildScope};
use i2pr_service_tunnels::{
    DestinationRef, ServerTarget, ServiceTunnelKind, ServiceTunnelSet, StaticAliasTable,
};
use i2pr_storage::{
    ServiceDestinationRecord, ServiceDestinationStorageError, ServiceDestinationStore,
};
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use crate::destination_streaming::{
    PumpConfig, PumpEndpointError, PumpSendDisposition, StreamPumpEndpoint, run_stream_pump,
};
use crate::sam::fabric::SamLocalProductFabric;
use crate::sam::streams::{
    InboundTunnelFactory, SamDestinationBridge, SamDestinationHandle, SamDestinations,
};
use crate::service_tunnels_http::run_http_client_loop;
use crate::service_tunnels_irc_client::run_irc_client_loop;
use crate::service_tunnels_socks5::run_socks5_client_loop;

/// Process-local monotonic clock used for Streaming deadlines.
pub fn service_streaming_now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    Instant::now()
        .duration_since(start)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Process-local protocol clock used for LeaseSet2 publication.
fn service_now_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u32::try_from(duration.as_secs()).ok())
        .unwrap_or(1)
}

/// Typed service-tunnel manager error.
#[derive(Debug, Error)]
pub enum ServiceTunnelError {
    /// The supplied configuration failed validation.
    #[error("invalid service tunnel configuration: {0}")]
    InvalidConfig(String),
    /// The destination storage layer rejected an operation.
    #[error("service destination storage error: {0}")]
    Storage(ServiceDestinationStorageError),
    /// The destination runtime rejected an operation.
    #[error("destination runtime error: {0}")]
    DestinationRuntime(String),
    /// A TCP bind failed.
    #[error("service tunnel listener bind failed: {0}")]
    Bind(String),
}

/// A validated, M10-enabled service-tunnel configuration.
#[derive(Clone, Debug)]
pub struct ServiceTunnelManagerConfig {
    /// Router data directory used for persistent service destinations.
    pub data_dir: PathBuf,
    /// Aggregate connection ceiling.
    pub aggregate_connection_ceiling: usize,
    /// Per-service connection ceiling.
    pub per_service_connection_ceiling: usize,
    /// Validated service specifications.
    pub specs: Arc<ServiceTunnelSet>,
    /// Validated static alias table.
    pub aliases: Arc<StaticAliasTable>,
}

/// One active service-tunnel runtime.
pub struct ServiceRuntime {
    /// Service spec id (e.g. "alpha-server").
    pub spec_id: String,
    #[allow(dead_code)]
    pub kind: ServiceTunnelKind,
    /// Cancel token for the per-service supervisor loop.
    cancellation: CancellationToken,
    /// Whether the per-service supervisor loop has stopped.
    stopped: Arc<AtomicBool>,
    /// Per-service connection count.
    pub(crate) active_connections: Arc<AtomicUsize>,
    /// Per-service failed-connect counter.
    pub(crate) failed_connects: Arc<AtomicUsize>,
    /// Bridge handle shared by the supervisor and the per-service connection pump.
    bridge: SamDestinationHandle,
    /// The destination id used for this service.
    pub(crate) destination_id: DestinationId,
    /// Listener for client tunnels (loopback TCP).
    pub(crate) client_listener: Option<TcpListener>,
    /// Local TCP target address for server tunnels.
    server_target: Option<SocketAddr>,
    /// I2P destination port for server tunnel Streaming listener.
    server_streaming_port: Option<u16>,
    /// Whether this is a server-side tunnel.
    is_server: bool,
    /// Whether this is an HTTP client tunnel.
    is_http: bool,
    /// Whether this is a SOCKS5 client tunnel.
    is_socks5: bool,
    /// Whether this is an IRC client tunnel.
    is_irc: bool,
}

impl std::fmt::Debug for ServiceRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceRuntime")
            .field("spec_id", &self.spec_id)
            .field("kind", &self.kind)
            .field("stopped", &self.stopped.load(Ordering::Acquire))
            .field(
                "active_connections",
                &self.active_connections.load(Ordering::Acquire),
            )
            .field(
                "failed_connects",
                &self.failed_connects.load(Ordering::Acquire),
            )
            .field("is_server", &self.is_server)
            .finish_non_exhaustive()
    }
}

/// Service tunnel manager composition root.
pub struct ServiceTunnelManager {
    config: ServiceTunnelManagerConfig,
    /// Per-spec-id runtime map.
    runtimes: Mutex<HashMap<String, Arc<ServiceRuntime>>>,
    /// Per-service destination registry shared with the local-delivery seam.
    sam_destinations: Mutex<SamDestinations>,
    /// Destination runtime registry (one per spec).
    destination_registry: Mutex<DestinationRegistry>,
    /// Destination configuration (shared, balanced profile).
    destination_config: DestinationConfig,
    /// Aggregate connection permit semaphore.
    aggregate_permit: Arc<Semaphore>,
}

impl std::fmt::Debug for ServiceTunnelManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceTunnelManager")
            .field(
                "runtimes",
                &self.runtimes.lock().map(|guard| guard.len()).unwrap_or(0),
            )
            .field(
                "aggregate_permits_available",
                &self.aggregate_permit.available_permits(),
            )
            .finish_non_exhaustive()
    }
}

impl ServiceTunnelManager {
    /// Creates a new manager from a validated configuration.
    pub fn new(config: ServiceTunnelManagerConfig) -> Result<Self, ServiceTunnelError> {
        let specs_count = u16::try_from(config.specs.tunnels.len() as u32).map_err(|_| {
            ServiceTunnelError::InvalidConfig("service count exceeds u16 capacity".to_owned())
        })?;
        let aggregate_ceiling = config.aggregate_connection_ceiling;
        Ok(Self {
            config,
            runtimes: Mutex::new(HashMap::new()),
            sam_destinations: Mutex::new(SamDestinations::new()),
            destination_registry: Mutex::new(DestinationRegistry::new(
                RegistryConfig::try_new(specs_count.max(1), 1024).map_err(|error| {
                    ServiceTunnelError::InvalidConfig(format!(
                        "destination registry config rejected: {error}"
                    ))
                })?,
            )),
            destination_config: DestinationConfig::balanced(),
            aggregate_permit: Arc::new(Semaphore::new(aggregate_ceiling)),
        })
    }

    /// Returns the validated configuration.
    pub fn config(&self) -> &ServiceTunnelManagerConfig {
        &self.config
    }

    /// Returns the router data directory.
    pub fn data_dir(&self) -> &std::path::Path {
        &self.config.data_dir
    }

    /// Returns a snapshot of the per-service accounting state.
    pub fn snapshot(&self) -> ServiceTunnelSnapshot {
        let runtimes = match self.runtimes.lock() {
            Ok(guard) => guard,
            Err(_) => return ServiceTunnelSnapshot::empty(),
        };
        let mut active_client = 0_usize;
        let mut active_server = 0_usize;
        let mut active_service_destinations = 0_usize;
        let mut pending_connects = 0_usize;
        let mut buffered_bytes_accounted = 0_u64;
        let mut failed_connects_total = 0_u64;
        let mut ready_services = 0_usize;
        for runtime in runtimes.values() {
            if !runtime.stopped.load(Ordering::Acquire) {
                ready_services = ready_services.saturating_add(1);
            }
            active_service_destinations = active_service_destinations.saturating_add(1);
            let active = runtime.active_connections.load(Ordering::Acquire);
            if runtime.is_server {
                active_server = active_server.saturating_add(active);
            } else {
                active_client = active_client.saturating_add(active);
            }
            buffered_bytes_accounted =
                buffered_bytes_accounted.saturating_add(active.saturating_mul(2048) as u64);
            pending_connects = pending_connects.saturating_add(active.min(2));
            failed_connects_total = failed_connects_total
                .saturating_add(runtime.failed_connects.load(Ordering::Acquire) as u64);
        }
        ServiceTunnelSnapshot {
            configured_services: runtimes.len(),
            ready_services,
            active_client_connections: active_client,
            active_server_connections: active_server,
            active_service_destinations,
            pending_connects,
            buffered_bytes_accounted,
            failed_connects_total,
        }
    }

    /// Prepares all enabled services (creates destinations, binds
    /// loopback TCP listeners, installs the per-service Streaming
    /// listener for server tunnels). The per-service supervisor
    /// loops are not yet started; call [`Self::start_supervisors`]
    /// after construction to begin accepting traffic.
    pub async fn prepare(self: &Arc<Self>) -> Result<Vec<Arc<ServiceRuntime>>, ServiceTunnelError> {
        let mut runtimes = Vec::new();
        let specs = self.config.specs.tunnels.clone();
        for spec in &specs {
            if !spec.enabled {
                continue;
            }
            let runtime = self.build_service_runtime(spec).await?;
            runtimes.push(runtime);
        }
        Ok(runtimes)
    }

    /// Starts the per-service supervisor loops for one prepared list
    /// of runtimes. Each loop lives under `children` and converges
    /// when the `cancellation` token fires.
    pub fn start_supervisors(
        self: &Arc<Self>,
        runtimes: Vec<Arc<ServiceRuntime>>,
        children: &ChildScope,
        _cancellation: CancellationToken,
    ) -> Result<usize, ServiceTunnelError> {
        let mut started = 0_usize;
        for runtime in runtimes {
            let manager_for_task = Arc::clone(self);
            let runtime_for_task = Arc::clone(&runtime);
            let spec_id = runtime.spec_id.clone();
            let spec = self
                .config
                .specs
                .tunnels
                .iter()
                .find(|s| s.id.as_str() == spec_id)
                .cloned()
                .ok_or_else(|| {
                    ServiceTunnelError::InvalidConfig(format!("{spec_id} spec missing for runtime"))
                })?;
            children
                .spawn(move |task_cancellation| {
                    Box::pin(async move {
                        run_service_loop(
                            manager_for_task,
                            runtime_for_task,
                            spec,
                            task_cancellation,
                        )
                        .await;
                        Ok(())
                    })
                })
                .map_err(|error| {
                    ServiceTunnelError::InvalidConfig(format!(
                        "{spec_id} child scope rejected service supervisor: {error}"
                    ))
                })?;
            started = started.saturating_add(1);
        }
        Ok(started)
    }

    /// Stops every service runtime.
    pub async fn shutdown(&self) {
        let runtimes = match self.runtimes.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        for (_, runtime) in runtimes.iter() {
            let _ = runtime
                .cancellation
                .cancel(i2pr_core::CancellationReason::ParentScope);
        }
    }

    /// Returns the public service destination base64 for one service.
    pub fn service_destination_b64(&self, service_id: &str) -> Option<String> {
        let runtimes = self.runtimes.lock().ok()?;
        let runtime = runtimes.get(service_id)?;
        let identity_arc = runtime.bridge.with(|bridge| bridge.identity());
        let wrapper = i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(
            identity_arc.as_ref(),
        )
        .ok()?;
        Some(wrapper.encode_public_base64())
    }

    /// Returns the destination id for one service.
    pub fn service_destination_id(&self, service_id: &str) -> Option<DestinationId> {
        let runtimes = self.runtimes.lock().ok()?;
        runtimes.get(service_id).map(|r| r.destination_id)
    }

    /// Returns the active per-service connection count for one service.
    pub fn active_connections(&self, service_id: &str) -> usize {
        self.runtimes
            .lock()
            .ok()
            .and_then(|runtimes| runtimes.get(service_id).cloned())
            .map(|runtime| runtime.active_connections.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    /// Returns the failed-connect count for one service.
    pub fn failed_connects(&self, service_id: &str) -> usize {
        self.runtimes
            .lock()
            .ok()
            .and_then(|runtimes| runtimes.get(service_id).cloned())
            .map(|runtime| runtime.failed_connects.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    /// Returns `true` when the supplied spec id has a registered runtime.
    pub fn has_runtime(&self, service_id: &str) -> bool {
        self.runtimes
            .lock()
            .ok()
            .is_some_and(|runtimes| runtimes.contains_key(service_id))
    }

    /// Returns the bound loopback address of a client-tunnel listener.
    pub fn client_listener_address(&self, service_id: &str) -> Option<SocketAddr> {
        let runtimes = self.runtimes.lock().ok()?;
        runtimes
            .get(service_id)?
            .client_listener
            .as_ref()?
            .local_addr()
            .ok()
    }

    /// Returns the configured loopback target address of a server tunnel.
    pub fn server_target_address(&self, service_id: &str) -> Option<SocketAddr> {
        let runtimes = self.runtimes.lock().ok()?;
        runtimes.get(service_id)?.server_target
    }

    /// Runs `closure` against the bridge registered for `destination_id`.
    pub fn with_destination_bridge<R>(
        &self,
        destination_id: DestinationId,
        closure: impl FnOnce(&mut SamDestinationBridge) -> R,
    ) -> Option<R> {
        let guard = self
            .sam_destinations
            .lock()
            .expect("sam destinations poisoned");
        guard.get(destination_id).map(|handle| handle.with(closure))
    }

    /// Returns a clone of the aggregate connection permit semaphore.
    pub fn aggregate_permit(&self) -> Arc<Semaphore> {
        Arc::clone(&self.aggregate_permit)
    }

    /// Returns the destination configuration used by the manager.
    pub fn destination_config(&self) -> DestinationConfig {
        self.destination_config
    }

    /// Inserts one DestinationRuntime into the per-service registry.
    pub fn register_destination_runtime(
        &self,
        runtime: DestinationRuntime,
    ) -> Result<(), ServiceTunnelError> {
        let mut registry = self
            .destination_registry
            .lock()
            .expect("destination registry poisoned");
        registry.insert(runtime).map(|_| ()).map_err(|error| {
            ServiceTunnelError::DestinationRuntime(format!("destination registry insert: {error}"))
        })
    }

    /// Removes one DestinationRuntime by id.
    pub fn unregister_destination_runtime(
        &self,
        destination_id: DestinationId,
    ) -> Option<DestinationShutdown> {
        let mut registry = self
            .destination_registry
            .lock()
            .expect("destination registry poisoned");
        registry.remove(&destination_id)
    }

    /// Returns the per-spec-id runtime map for diagnostics.
    pub fn runtime_spec_ids(&self) -> Vec<String> {
        self.runtimes
            .lock()
            .map(|guard| guard.keys().cloned().collect())
            .unwrap_or_default()
    }

    async fn build_service_runtime(
        self: &Arc<Self>,
        spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    ) -> Result<Arc<ServiceRuntime>, ServiceTunnelError> {
        let id_owned = spec.id.as_str().to_owned();
        let is_server = matches!(spec.kind, ServiceTunnelKind::GenericServer);
        let bridge_data = self.create_bridge_for_spec(spec).await?;
        let bridge = {
            let mut sam_destinations = self
                .sam_destinations
                .lock()
                .expect("sam destinations poisoned");
            let bridge = SamDestinationBridge::with_shared_identity(
                Arc::clone(&bridge_data.identity_arc),
                bridge_data.lease_set2,
                bridge_data.outbound_role,
                bridge_data.now_seconds,
            );
            sam_destinations.install(bridge_data.destination_id, bridge)
        };
        let runtime = DestinationRuntime::with_shared_identity(
            Arc::clone(&bridge_data.identity_arc),
            self.destination_config,
        )
        .map_err(|error| {
            ServiceTunnelError::DestinationRuntime(format!("destination runtime: {error}"))
        })?;
        self.register_destination_runtime(runtime)?;
        let server_streaming_port = if is_server { Some(1_u16) } else { None };
        if let Some(port) = server_streaming_port {
            let outcome_result = self
                .with_destination_bridge(bridge_data.destination_id, |bridge| {
                    bridge.receiver_streaming_mut().listen(port)
                });
            let effective: ListenerOutcome = match outcome_result {
                Some(Ok(value)) => value,
                Some(Err(_)) => ListenerOutcome::BacklogFull,
                None => ListenerOutcome::BacklogFull,
            };
            if !matches!(
                effective,
                ListenerOutcome::Listening { .. } | ListenerOutcome::PortAlreadyInUse
            ) {
                return Err(ServiceTunnelError::InvalidConfig(format!(
                    "{id_owned} streaming listener could not bind port {port}: {effective:?}"
                )));
            }
        }
        let client_listener = if !is_server {
            let listener_spec = spec.listener.ok_or_else(|| {
                ServiceTunnelError::InvalidConfig(format!("{id_owned} missing loopback listener"))
            })?;
            Some(
                TcpListener::bind(listener_spec.socket())
                    .await
                    .map_err(|error| ServiceTunnelError::Bind(format!("{id_owned}: {error}")))?,
            )
        } else {
            None
        };
        let server_target = if is_server {
            let target = spec
                .target
                .as_ref()
                .or_else(|| spec.targets.first())
                .ok_or_else(|| {
                    ServiceTunnelError::InvalidConfig(format!(
                        "{id_owned} server tunnel missing loopback target"
                    ))
                })?;
            let socket = match target {
                ServerTarget::LoopbackTcp(addr) => *addr,
                ServerTarget::UnixPath(_) => {
                    return Err(ServiceTunnelError::InvalidConfig(format!(
                        "{id_owned} server tunnel targets Unix-domain sockets as not-yet-supported"
                    )));
                }
            };
            Some(socket)
        } else {
            None
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let failed_connects = Arc::new(AtomicUsize::new(0));
        let is_http = matches!(spec.kind, ServiceTunnelKind::HttpClient);
        let is_socks5 = matches!(spec.kind, ServiceTunnelKind::Socks5Client);
        let is_irc = matches!(spec.kind, ServiceTunnelKind::IrcClient);
        let runtime = Arc::new(ServiceRuntime {
            spec_id: spec.id.as_str().to_owned(),
            kind: spec.kind,
            cancellation: CancellationToken::new(),
            stopped: Arc::clone(&stopped),
            active_connections: Arc::clone(&active_connections),
            failed_connects: Arc::clone(&failed_connects),
            bridge,
            destination_id: bridge_data.destination_id,
            client_listener,
            server_target,
            server_streaming_port,
            is_server,
            is_http,
            is_socks5,
            is_irc,
        });
        let mut runtimes = self.runtimes.lock().expect("runtimes poisoned");
        runtimes.insert(spec.id.as_str().to_owned(), Arc::clone(&runtime));
        Ok(runtime)
    }

    async fn create_bridge_for_spec(
        &self,
        spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    ) -> Result<BridgeData, ServiceTunnelError> {
        let now_seconds = service_now_seconds();
        let identity = if matches!(spec.kind, ServiceTunnelKind::GenericServer) {
            let store =
                ServiceDestinationStore::for_service(&self.config.data_dir, spec.id.as_str())
                    .map_err(ServiceTunnelError::Storage)?;
            let record: ServiceDestinationRecord = if store.exists() {
                store.load().map_err(ServiceTunnelError::Storage)?
            } else {
                let mut rng = OsRng;
                store
                    .generate_new(&mut rng)
                    .map_err(ServiceTunnelError::Storage)?
            };
            identity_from_record(&record, spec.id.as_str())?
        } else {
            let mut rng = OsRng;
            DestinationIdentity::generate(&mut rng).map_err(|error| {
                ServiceTunnelError::InvalidConfig(format!(
                    "{} ephemeral identity generation failed: {error}",
                    spec.id.as_str()
                ))
            })?
        };
        let fabric = SamLocalProductFabric::new();
        let product = fabric
            .prepare_for_destination(&identity, now_seconds)
            .map_err(|error| {
                ServiceTunnelError::InvalidConfig(format!(
                    "{} fabric prepare failed: {error}",
                    spec.id.as_str()
                ))
            })?;
        let destination_id = identity.id();
        Ok(BridgeData {
            identity_arc: Arc::new(identity),
            lease_set2: product.lease_set2,
            outbound_role: product.outbound_role,
            inbound_tunnel_factory: product.inbound_tunnel_factory,
            validated_lease_set2: product.validated_lease_set2,
            now_seconds,
            destination_id,
        })
    }

    /// Looks up the configured destination reference for one client tunnel.
    pub fn resolve_client_destination(
        &self,
        spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    ) -> Result<ClientTarget, DestinationFailure> {
        let reference = spec
            .destination
            .as_ref()
            .ok_or(DestinationFailure::Missing)?;
        self.resolve_reference(reference)
    }

    pub fn resolve_reference(
        &self,
        reference: &DestinationRef,
    ) -> Result<ClientTarget, DestinationFailure> {
        match reference {
            DestinationRef::Base32Hash { label, hash } => {
                if let Some(target) = self.config.aliases.get(label) {
                    return self.resolve_reference(target);
                }
                // Look for a registered local service destination with
                // the matching hash so client/server tunnels owned by
                // the same manager can resolve through the local
                // delivery path without an external LeaseSet lookup.
                if let Some(target) = self.lookup_local_service_destination(hash) {
                    return Ok(target);
                }
                Err(DestinationFailure::LookupRequired {
                    label: label.clone(),
                    hash: *hash,
                })
            }
            DestinationRef::StaticAlias(alias) => {
                let target = self
                    .config
                    .aliases
                    .get(alias)
                    .ok_or_else(|| DestinationFailure::UnknownAlias(alias.clone()))?;
                self.resolve_reference(target)
            }
            DestinationRef::ConfiguredDestination(material) => {
                let bytes = i2pr_api::sam::base64::decode(material, 4096)
                    .map_err(|_| DestinationFailure::InvalidMaterial)?;
                let destination = Destination::decode(&bytes, 4096).map_err(|error| {
                    DestinationFailure::InvalidMaterialDecode(error.to_string())
                })?;
                let signing_key = destination.signing_key().clone();
                let static_public = destination.public_key().as_bytes().to_vec();
                let mut static_out = [0_u8; X25519_KEY_LENGTH];
                let len = static_public.len().min(X25519_KEY_LENGTH);
                static_out[..len].copy_from_slice(&static_public[..len]);
                let destination_hash = *destination
                    .hash()
                    .map_err(|error| DestinationFailure::InvalidMaterialDecode(error.to_string()))?
                    .as_bytes();
                Ok(ClientTarget {
                    destination,
                    remote: RemoteDestination {
                        destination_hash,
                        signing_public_key: signing_key,
                        static_public_key: static_out,
                    },
                })
            }
        }
    }

    /// Looks up a local service destination by its hash.
    pub fn lookup_local_service_destination(&self, hash: &[u8; 32]) -> Option<ClientTarget> {
        let runtimes = self.runtimes.lock().ok()?;
        for (_, runtime) in runtimes.iter() {
            if runtime.destination_id.as_hash().as_bytes() == hash {
                let identity_arc = runtime.bridge.with(|bridge| bridge.identity());
                let destination = identity_arc.destination().clone();
                let signing_key = destination.signing_key().clone();
                let static_public = destination.public_key().as_bytes().to_vec();
                let mut static_out = [0_u8; X25519_KEY_LENGTH];
                let len = static_public.len().min(X25519_KEY_LENGTH);
                static_out[..len].copy_from_slice(&static_public[..len]);
                return Some(ClientTarget {
                    destination,
                    remote: RemoteDestination {
                        destination_hash: *hash,
                        signing_public_key: signing_key,
                        static_public_key: static_out,
                    },
                });
            }
        }
        None
    }
}

fn identity_from_record(
    record: &ServiceDestinationRecord,
    service_id: &str,
) -> Result<DestinationIdentity, ServiceTunnelError> {
    let mut signing_seed = Zeroizing::new([0_u8; PRIVATE_KEY_LENGTH]);
    signing_seed.copy_from_slice(record.signing_seed());
    let mut static_secret = Zeroizing::new([0_u8; X25519_KEY_LENGTH]);
    static_secret.copy_from_slice(record.static_secret());
    let mut padding = Zeroizing::new(vec![0_u8; IDENTITY_PADDING_LENGTH]);
    padding.copy_from_slice(record.padding());
    DestinationIdentity::from_private_bytes(*signing_seed, *static_secret, padding).map_err(
        |error| {
            ServiceTunnelError::InvalidConfig(format!(
                "{service_id} destination reconstruction failed: {error}"
            ))
        },
    )
}

/// Resolved client target material.
#[derive(Clone, Debug)]
pub struct ClientTarget {
    #[allow(dead_code)]
    pub destination: Destination,
    pub remote: RemoteDestination,
}

/// Typed destination resolution failure.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum DestinationFailure {
    /// The service spec did not include a destination reference.
    #[error("client service tunnel missing destination reference")]
    Missing,
    /// A Base32 hash reference requires NetDB lookup that this layer
    /// does not perform.
    #[error(
        "Base32 hash reference {label} requires LeaseSet lookup outside the service tunnel core"
    )]
    LookupRequired { label: String, hash: [u8; 32] },
    /// The alias table did not contain the supplied alias.
    #[error("static alias '{0}' not registered")]
    UnknownAlias(String),
    /// A `ConfiguredDestination` value failed base64 decoding.
    #[error("configured destination base64 decode failed")]
    InvalidMaterial,
    /// A `ConfiguredDestination` value decoded but failed structural
    /// Destination decode.
    #[error("configured destination structural decode failed: {0}")]
    InvalidMaterialDecode(String),
}

/// Intermediate bridge construction payload.
struct BridgeData {
    identity_arc: Arc<DestinationIdentity>,
    lease_set2: LeaseSet2,
    outbound_role: DestinationOutboundRole,
    #[allow(dead_code)]
    inbound_tunnel_factory: Arc<dyn InboundTunnelFactory>,
    #[allow(dead_code)]
    validated_lease_set2: ValidatedLeaseSet2,
    now_seconds: u32,
    destination_id: DestinationId,
}

/// Service tunnel accounting snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServiceTunnelSnapshot {
    /// Number of configured services.
    pub configured_services: usize,
    /// Number of services that finished startup and are listening.
    pub ready_services: usize,
    /// Aggregate active client connections.
    pub active_client_connections: usize,
    /// Aggregate active server connections.
    pub active_server_connections: usize,
    /// Number of distinct service destinations currently active.
    pub active_service_destinations: usize,
    /// Approximate count of connections waiting to complete connect/accept.
    pub pending_connects: usize,
    /// Approximate bytes currently in flight across all services.
    pub buffered_bytes_accounted: u64,
    /// Bounded counter of failed connects (no per-peer labels).
    pub failed_connects_total: u64,
}

impl ServiceTunnelSnapshot {
    /// Returns an empty snapshot.
    pub fn empty() -> Self {
        Self::default()
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Runs one per-service supervisor loop.
async fn run_service_loop(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    spec: i2pr_service_tunnels::ServiceTunnelSpec,
    task_cancellation: CancellationToken,
) {
    let id = runtime.spec_id.clone();
    debug!(service = %id, "service tunnel supervisor entered");
    let result = if runtime.is_server {
        run_server_loop(&manager, &runtime, &spec, &task_cancellation).await
    } else if runtime.is_http {
        run_http_client_loop(&manager, &runtime, &spec, &task_cancellation).await
    } else if runtime.is_socks5 {
        run_socks5_client_loop(&manager, &runtime, &spec, &task_cancellation).await
    } else if runtime.is_irc {
        run_irc_client_loop(&manager, &runtime, &spec, &task_cancellation).await
    } else {
        run_client_loop(&manager, &runtime, &spec, &task_cancellation).await
    };
    if let Err(error) = result {
        warn!(service = %id, error = %error, "service loop failed");
    }
    runtime.stopped.store(true, Ordering::Release);
    debug!(service = %id, "service tunnel supervisor exited");
}

async fn run_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), ServiceTunnelError> {
    let listener = runtime
        .client_listener
        .as_ref()
        .ok_or_else(|| ServiceTunnelError::InvalidConfig("missing client listener".to_owned()))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| ServiceTunnelError::Bind(error.to_string()))?;
    info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "generic client tunnel bound loopback listener"
    );
    let client_target = match manager.resolve_client_destination(spec) {
        Ok(target) => target,
        Err(error) => {
            warn!(
                service = %spec.id.as_str(),
                error = %error,
                "client tunnel destination resolve failed; service will not accept connections"
            );
            let _ = cancellation.cancelled().await;
            return Ok(());
        }
    };
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            accept = listener.accept() => accept,
        };
        let (stream, _peer) = match accept {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    service = %spec.id.as_str(),
                    error = %error,
                    "client listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<OwnedSemaphorePermit> =
            manager.aggregate_permit.clone().try_acquire_owned().ok();
        let Some(aggregate_permit) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "client tunnel aggregate ceiling reached; rejecting connection"
            );
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            drop(stream);
            continue;
        };
        runtime.active_connections.fetch_add(1, Ordering::Relaxed);
        let manager_for_task = Arc::clone(manager);
        let runtime_for_task = Arc::clone(runtime);
        let target_for_task = client_target.clone();
        let cancellation_for_task = cancellation.clone();
        let spec_id_for_log = runtime.spec_id.clone();
        let _aggregate_permit = aggregate_permit;
        tokio::spawn(async move {
            if let Err(error) = run_client_connection(
                manager_for_task,
                runtime_for_task,
                target_for_task,
                stream,
                cancellation_for_task,
            )
            .await
            {
                warn!(service = %spec_id_for_log, error = %error, "client connection failed");
            }
        });
    }
    Ok(())
}

async fn run_server_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    _spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), ServiceTunnelError> {
    let target_socket = runtime.server_target.ok_or_else(|| {
        ServiceTunnelError::InvalidConfig("server tunnel missing loopback target".to_owned())
    })?;
    info!(
        service = %runtime.spec_id,
        target = %target_socket,
        "generic server tunnel bound streaming listener"
    );
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let port = runtime.server_streaming_port.unwrap_or(1_u16);
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            _ = ticker.tick() => {}
        }
        let mut accepted_ids = Vec::new();
        manager.with_destination_bridge(runtime.destination_id, |bridge| {
            while let Some(connection_id) = bridge.receiver_streaming_mut().accept(port) {
                accepted_ids.push(connection_id);
            }
        });
        for connection_id in accepted_ids {
            handle_server_syn(manager, runtime, target_socket, connection_id, cancellation).await;
        }
    }
    Ok(())
}

async fn handle_server_syn(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    target: SocketAddr,
    connection_id: ConnectionId,
    cancellation: &CancellationToken,
) {
    let now_ms = service_streaming_now_ms();
    let identity_arc =
        manager.with_destination_bridge(runtime.destination_id, |bridge| bridge.identity());
    let identity_arc = match identity_arc {
        Some(identity) => identity,
        None => return,
    };
    let _accept_outcome = manager.with_destination_bridge(runtime.destination_id, |bridge| {
        let remote = RemoteDestination {
            destination_hash: [0_u8; 32],
            signing_public_key: identity_arc.destination().signing_key().clone(),
            static_public_key: identity_arc.static_public_bytes(),
        };
        let mut os_rng = OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        bridge.receiver_streaming_mut().accept_inbound_syn(
            identity_arc.as_ref(),
            &remote,
            connection_id,
            0_u16,
            0_u16,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            now_ms,
            &mut rng,
        )
    });
    let aggregate_permit: Option<OwnedSemaphorePermit> =
        manager.aggregate_permit.clone().try_acquire_owned().ok();
    let Some(aggregate_permit) = aggregate_permit else {
        warn!(
            service = %runtime.spec_id,
            "server tunnel aggregate ceiling reached; rejecting inbound SYN"
        );
        runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
        return;
    };
    runtime.active_connections.fetch_add(1, Ordering::Relaxed);
    let _aggregate_permit = aggregate_permit;
    let manager_for_task = Arc::clone(manager);
    let runtime_for_task = Arc::clone(runtime);
    let cancellation_for_task = cancellation.clone();
    let spec_id_for_log = runtime.spec_id.clone();
    tokio::spawn(async move {
        if let Err(error) = run_server_connection(
            manager_for_task,
            runtime_for_task,
            target,
            connection_id,
            cancellation_for_task,
        )
        .await
        {
            warn!(service = %spec_id_for_log, error = %error, "server connection failed");
        }
    });
}

async fn run_server_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    target: SocketAddr,
    connection_id: ConnectionId,
    cancellation: CancellationToken,
) -> Result<(), BoxError> {
    let connect_deadline = lookup_connect_timeout(&manager, &runtime.spec_id);
    let target_stream = match timeout(
        Duration::from_millis(connect_deadline),
        TcpStream::connect(target),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(std::io::Error::other(format!(
                "server target connect failed: {error}"
            ))));
        }
        Err(_) => {
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(std::io::Error::other(
                "server target connect timed out",
            )));
        }
    };
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_server(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
    ));
    let config = PumpConfig::defaults(32 * 1024);
    let pump = run_stream_pump(target_stream, Vec::new(), endpoint, config, cancellation);
    let result = pump.await;
    runtime
        .active_connections
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(1))
        })
        .ok();
    if let Err(error) = result {
        warn!(
            service = %runtime.spec_id,
            connection_id = connection_id.raw(),
            error = %error,
            "server connection pump exited with error"
        );
    }
    manager.with_destination_bridge(runtime.destination_id, |bridge| {
        let _ = bridge
            .receiver_streaming_mut()
            .remove_connection(connection_id);
    });
    Ok(())
}

async fn run_client_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    target: ClientTarget,
    stream: TcpStream,
    cancellation: CancellationToken,
) -> Result<(), BoxError> {
    let connect_timeout_ms = lookup_connect_timeout(&manager, &runtime.spec_id);
    let identity_arc =
        manager.with_destination_bridge(runtime.destination_id, |bridge| bridge.identity());
    let Some(identity_arc) = identity_arc else {
        runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
        return Err(Box::new(std::io::Error::other(
            "client tunnel missing destination identity",
        )));
    };
    let connect_outcome_result = {
        let mut os_rng = OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        manager.with_destination_bridge(runtime.destination_id, |bridge| {
            bridge.streaming_mut().connect(
                identity_arc.as_ref(),
                &target.remote,
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                service_streaming_now_ms(),
                &mut rng,
            )
        })
    };
    let connect_outcome = match connect_outcome_result {
        Some(value) => value,
        None => {
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(std::io::Error::other(
                "client connect produced no outcome",
            )));
        }
    };
    let connection_id = match connect_outcome {
        Ok(ConnectOutcome::SynSent { connection_id, .. }) => connection_id,
        Ok(_) => {
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(std::io::Error::other(
                "client connect produced no SYN",
            )));
        }
        Err(error) => {
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(std::io::Error::other(format!(
                "client connect error: {error}"
            ))));
        }
    };
    if let Err(error) = wait_for_established(
        &manager,
        runtime.destination_id,
        connection_id,
        connect_timeout_ms,
    )
    .await
    {
        runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
        return Err(error);
    }
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_client(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
        target.remote.clone(),
    ));
    let config = PumpConfig::defaults(32 * 1024);
    let pump = run_stream_pump(stream, Vec::new(), endpoint, config, cancellation);
    let result = pump.await;
    runtime
        .active_connections
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(1))
        })
        .ok();
    if let Err(error) = result {
        warn!(
            service = %runtime.spec_id,
            connection_id = connection_id.raw(),
            error = %error,
            "client connection pump exited with error"
        );
    }
    manager.with_destination_bridge(runtime.destination_id, |bridge| {
        let _ = bridge.streaming_mut().remove_connection(connection_id);
    });
    Ok(())
}

async fn wait_for_established(
    manager: &ServiceTunnelManager,
    destination_id: DestinationId,
    connection_id: ConnectionId,
    timeout_ms: u64,
) -> Result<(), BoxError> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let established = manager.with_destination_bridge(destination_id, |bridge| {
            bridge
                .streaming()
                .get_connection(connection_id)
                .is_some_and(|conn| matches!(conn.state(), ConnectionState::Established))
        });
        if established.unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Box::new(std::io::Error::other("deadline reached")));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn lookup_connect_timeout(manager: &ServiceTunnelManager, spec_id: &str) -> u64 {
    manager
        .config()
        .specs
        .tunnels
        .iter()
        .find(|spec| spec.id.as_str() == spec_id)
        .map(|spec| spec.timeouts.connect_timeout_ms)
        .unwrap_or(10_000)
}

/// Streaming pump endpoint adapted to one service-tunnel destination.
pub struct ServicePumpEndpoint {
    manager: Arc<ServiceTunnelManager>,
    destination_id: DestinationId,
    connection_id: ConnectionId,
    remote: Option<RemoteDestination>,
    direction: ServiceDirection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceDirection {
    /// Endpoint serves the client side of a tunnel.
    Client,
    /// Endpoint serves the server side of a tunnel.
    Server,
}

impl ServicePumpEndpoint {
    pub fn new_client(
        manager: Arc<ServiceTunnelManager>,
        destination_id: DestinationId,
        connection_id: ConnectionId,
        remote: RemoteDestination,
    ) -> Self {
        Self {
            manager,
            destination_id,
            connection_id,
            remote: Some(remote),
            direction: ServiceDirection::Client,
        }
    }

    pub fn new_server(
        manager: Arc<ServiceTunnelManager>,
        destination_id: DestinationId,
        connection_id: ConnectionId,
    ) -> Self {
        Self {
            manager,
            destination_id,
            connection_id,
            remote: None,
            direction: ServiceDirection::Server,
        }
    }
}

impl StreamPumpEndpoint for ServicePumpEndpoint {
    fn max_payload_bytes(&self) -> usize {
        self.manager
            .with_destination_bridge(self.destination_id, |bridge| match self.direction {
                ServiceDirection::Client => bridge
                    .streaming()
                    .get_connection(self.connection_id)
                    .map(|c| c.max_payload_size() as usize),
                ServiceDirection::Server => bridge
                    .receiver_streaming()
                    .get_connection(self.connection_id)
                    .map(|c| c.max_payload_size() as usize),
            })
            .flatten()
            .unwrap_or(DEFAULT_ADVERTISED_MAX_PAYLOAD as usize)
            .max(1)
    }

    fn try_send(&self, segment: &[u8]) -> Result<PumpSendDisposition, PumpEndpointError> {
        let Some(remote) = self.remote.as_ref().cloned() else {
            return Err(PumpEndpointError::UnknownConnection);
        };
        let identity_arc = self
            .manager
            .with_destination_bridge(self.destination_id, |bridge| bridge.identity());
        let Some(identity_arc) = identity_arc else {
            return Err(PumpEndpointError::UnknownConnection);
        };
        self.manager
            .with_destination_bridge(self.destination_id, |bridge| {
                let now_ms = service_streaming_now_ms();
                let conn = bridge.streaming().get_connection(self.connection_id);
                let Some(conn) = conn else {
                    return Err(PumpEndpointError::UnknownConnection);
                };
                let local_port = conn.local_port();
                let remote_port = conn.remote_port();
                match bridge.streaming_mut().send_data(
                    self.connection_id,
                    identity_arc.as_ref(),
                    &remote,
                    local_port,
                    remote_port,
                    segment,
                    now_ms,
                ) {
                    Ok(_) => Ok(PumpSendDisposition::Accepted),
                    Err(error) => {
                        let message = error.to_string();
                        if message.contains("SendWindowFull") || message.contains("Congestion") {
                            Ok(PumpSendDisposition::Backpressured)
                        } else if message.contains("UnknownConnection") {
                            Err(PumpEndpointError::UnknownConnection)
                        } else if message.contains("InvalidState") {
                            Err(PumpEndpointError::InvalidState)
                        } else {
                            Err(PumpEndpointError::Streaming(message))
                        }
                    }
                }
            })
            .unwrap_or(Err(PumpEndpointError::UnknownConnection))
    }

    fn drain_delivered(&self) -> Vec<Vec<u8>> {
        self.manager
            .with_destination_bridge(self.destination_id, |bridge| {
                let delivered = match self.direction {
                    ServiceDirection::Client => bridge
                        .streaming_mut()
                        .drain_delivered_for(self.connection_id),
                    ServiceDirection::Server => bridge
                        .receiver_streaming_mut()
                        .drain_delivered_for(self.connection_id),
                };
                delivered
                    .into_iter()
                    .filter_map(|entry| {
                        if entry.bytes.is_empty() {
                            None
                        } else {
                            Some(entry.bytes)
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_terminal(&self) -> bool {
        self.manager
            .with_destination_bridge(self.destination_id, |bridge| {
                let connection = match self.direction {
                    ServiceDirection::Client => {
                        bridge.streaming().get_connection(self.connection_id)
                    }
                    ServiceDirection::Server => bridge
                        .receiver_streaming()
                        .get_connection(self.connection_id),
                };
                connection.is_none_or(|conn| {
                    matches!(
                        conn.state(),
                        ConnectionState::ClosingRemote
                            | ConnectionState::Closed
                            | ConnectionState::Reset
                    )
                })
            })
            .unwrap_or(true)
    }

    fn notify_outbound(&self) {
        let Ok(guard) = self.manager.sam_destinations.lock() else {
            return;
        };
        if let Some(handle) = guard.get(self.destination_id) {
            handle.with(|bridge| {
                bridge.poll_streaming_timers(service_streaming_now_ms());
            });
        }
    }
}

/// Registers the service tunnel manager as a supervised service.
pub fn register_service_tunnel_manager(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    data_dir: PathBuf,
    specs: Arc<ServiceTunnelSet>,
    aliases: Arc<StaticAliasTable>,
    per_service_ceiling: usize,
    aggregate_ceiling: usize,
) -> Result<Arc<ServiceTunnelManager>, ServiceTunnelError> {
    let config = ServiceTunnelManagerConfig {
        data_dir,
        aggregate_connection_ceiling: aggregate_ceiling,
        per_service_connection_ceiling: per_service_ceiling,
        specs,
        aliases,
    };
    let manager = ServiceTunnelManager::new(config)?;
    let manager_arc = Arc::new(manager);
    let manager_shared = Arc::clone(&manager_arc);
    let service_name =
        i2pr_runtime::ServiceName::new("service-tunnels").expect("valid service name");
    builder
        .register(i2pr_runtime::ServiceSpec::new(
            service_name,
            i2pr_core::ServiceClassification::Optional,
            move |ctx| {
                let manager = Arc::clone(&manager_shared);
                let children = ctx.children();
                let cancellation = ctx.cancellation().clone();
                Box::pin(async move {
                    let manager_for_start = Arc::clone(&manager);
                    let runtimes = match manager_for_start.prepare().await {
                        Ok(runtimes) => runtimes,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "service tunnel manager prepare failed: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    if let Err(error) =
                        manager.start_supervisors(runtimes, &children, cancellation.clone())
                    {
                        let detail = i2pr_core::HealthDetail::new(format!(
                            "service tunnel manager start failed: {error}"
                        ))
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
                    cancellation.cancelled().await;
                    manager.shutdown().await;
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|error| {
            ServiceTunnelError::InvalidConfig(format!(
                "failed to register service tunnel manager: {error}"
            ))
        })?;
    Ok(manager_arc)
}
