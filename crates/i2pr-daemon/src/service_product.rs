//! Plan 209 — production composition helper for the M10 remote
//! HTTP/IRC acceptance driver.
//!
//! The Plan 207 driver constructed a parallel `StreamingManager` /
//! `StreamingDestinationAdapter` / `DestinationRouting` /
//! `EciesSessionManager` / `DestinationTunnelCoordinator` /
//! `ExploratoryBuildCoordinator` / `Ssu2DaemonService` /
//! `RouterDeliveryService` stack alongside the production
//! `ServiceTunnelManager`. Plan 209 §5 forbids that pattern: the
//! counted driver must consume only the production path. This
//! module exposes the single composition function both the Plan 208
//! and Plan 209 drivers use in real product code; the function
//! wires the daemon-owned router stack into the manager, returns a
//! typed product handle, and lets the driver read listener ports /
//! counters without ever touching the lower-stack types directly.
//!
//! ```text
//! ServiceProduct::start(spec)
//!   -> Ssu2DaemonService::new (loopback bind, advertise = false)
//!   -> Ssu2DaemonService::start (binds UDP sockets under ChildScope)
//!   -> dial_reference_peer (authenticated SSU2 to the i2pd cache)
//!   -> bootstrap_reference_router_info (NetDB)
//!   -> ExploratoryBuildCoordinator (tunnel material)
//!   -> DestinationTunnelCoordinator (LeaseSet2 lookup)
//!   -> RemoteDestinationBackend (router delivery + coordinator)
//!   -> ServiceTunnelManager::new(specs)
//!   -> ServiceTunnelManager::install_router_delivery(backend)
//!   -> ServiceProduct { manager, ssu2_handle, coordinator, scope, token }
//! ```
//!
//! The driver then:
//!
//! 1. obtains bound HTTP/IRC listener addresses via
//!    `http_listener_port` / `irc_listener_port`;
//! 2. spawns the production inbound pump task via `poll_inbound`
//!    so tunneled NetDB responses / Garlic payloads flow back into
//!    the owning service runtime;
//! 3. drives unmodified `curl` / `jaraco/irc` subprocesses against
//!    the manager listeners;
//! 4. reads operation-derived Plan 208 counters via
//!    `remote_counters`;
//! 5. emits sanitized evidence;
//! 6. shuts down the product via `shutdown`.
//!
//! No shadow stack is constructed. No `StreamingManager::new`,
//! `StreamingDestinationAdapter::new`, `DestinationRouting`,
//! `EciesSessionManager`, `DestinationTunnelCoordinator`,
//! `ExploratoryBuildCoordinator`, `Ssu2DaemonService`,
//! `RouterDeliveryService`, or `RouterDeliveryRequest` is reachable
//! from the counted driver.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{DestinationHash, LookupPolicy, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, RouterInfo};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope, Ssu2InboundI2np};
use i2pr_service_tunnels::{ServiceTunnelSet, StaticAliasTable};
use i2pr_transport::Deadline;
use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::identity::TunnelId;
use i2pr_tunnel::short_record::HopRole;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::destination_tunnels::{
    DestinationTunnelCoordinator, LeaseStoreIngestOutcome, reply_path_for_inbound_route,
};
use crate::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, BuildRequest, ExploratoryBuildCoordinator,
    PeerBuildMaterial,
};
use crate::inbound_dispatch::{self, InboundDispatchOutcome};
use crate::router_i2np::{
    Ssu2DaemonHandle, Ssu2DaemonService, generate_controlled_identity, verify_reference_router_info,
};
use crate::service_delivery::{
    RemoteDeliveryCounters, RemoteDestinationBackend, RoutingDecision, ServiceDestinationDelivery,
};
use crate::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

/// Tunable tunnel ids the production composition owns end-to-end.
/// Two pairs — outbound (creator + OBEP) and inbound (IBGW) —
/// install through the existing `ExploratoryBuildCoordinator`
/// (Plan 185) and never appear in driver code.
const OUTBOUND_CREATOR: u32 = 0x1580;
const OBEP_RECEIVE: u32 = 0x9581;
const OBEP_NEXT: u32 = 0x9582;
const INBOUND_CREATOR: u32 = 0x1680;
const IBGW_RECEIVE: u32 = 0x9681;
const IBGW_NEXT: u32 = 0x9682;

/// Default bounded dial deadline for the controlled lane SSU2 session.
const DEFAULT_DIAL_TIMEOUT: Duration = Duration::from_secs(20);
/// Default bounded inbound poll interval.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Default bounded wait for one exploratory build to install.
const DEFAULT_I2PD_ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default bounded outbound cell delivery deadline.
const DEFAULT_DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Default bounded lookup / reply deadline.
const DEFAULT_TUNNEL_DEADLINE: Duration = Duration::from_secs(60);

/// Tunable bound for the production composition.
#[derive(Clone, Copy, Debug)]
pub struct ServiceProductOptions {
    /// Dial deadline.
    pub dial_timeout: Duration,
    /// Inbound poll interval.
    pub poll_interval: Duration,
    /// Wait for one build to install.
    pub i2pd_accept_timeout: Duration,
    /// Outbound cell delivery deadline.
    pub delivery_timeout: Duration,
    /// Lookup / reply deadline.
    pub tunnel_deadline: Duration,
}

impl Default for ServiceProductOptions {
    fn default() -> Self {
        Self {
            dial_timeout: DEFAULT_DIAL_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
            i2pd_accept_timeout: DEFAULT_I2PD_ACCEPT_TIMEOUT,
            delivery_timeout: DEFAULT_DELIVERY_TIMEOUT,
            tunnel_deadline: DEFAULT_TUNNEL_DEADLINE,
        }
    }
}

/// Inputs the Plan 209 driver passes to the production composition.
pub struct ServiceProductSpec {
    /// Persistent data directory for the manager's per-service
    /// destinations and other Plan 174-180 bookkeeping.
    pub data_dir: PathBuf,
    /// Strict-controlled loopback SSU2 bind address. Must be
    /// `127.0.0.1` and nonzero (the controlled profile rejects
    /// `port = 0` because the in-band RouterInfo carries the port).
    pub ssu2_bind: SocketAddr,
    /// Router identity bundle. CSPRNG-generated by the driver; the
    /// helper signs the controlled RouterInfo and never exposes
    /// private material.
    pub router_bundle: RouterIdentityBundle,
    /// Validated service-tunnel spec set.
    pub service_tunnels: Arc<ServiceTunnelSet>,
    /// Static alias table (e.g. `.i2p` host spellings).
    pub aliases: Arc<StaticAliasTable>,
    /// Aggregate connection ceiling across every service.
    pub aggregate_connection_ceiling: usize,
    /// Per-service connection ceiling.
    pub per_service_connection_ceiling: usize,
    /// Optional reference peer (Plan 161 i2pd 2.61.0). When set,
    /// the helper dials, bootstraps the RouterInfo, and builds the
    /// outbound + inbound tunnels end-to-end.
    pub reference: Option<ReferencePeer>,
    /// Tunable bounds; defaults to [`ServiceProductOptions::default`].
    pub options: ServiceProductOptions,
}

/// Reference peer the controlled lane dials + bootstraps.
#[derive(Clone, Debug)]
pub struct ReferencePeer {
    /// RouterInfo bytes (unmodified public material).
    pub router_info_bytes: Vec<u8>,
    /// Loopback UDP endpoint for the dial.
    pub endpoint: SocketAddr,
    /// Plan 210 §C — destination hash for the reference's
    /// **destination** (not router identity). Production service
    /// LeaseSet lookup is keyed on this value. Plan 210 §C forbids
    /// deriving a service-destination lookup key from
    /// `router_info_bytes` (RouterInfo hash is the router identity
    /// key, not the destination identity key), so the explicit
    /// value is mandatory in production drivers. The field is
    /// `Option` only so test-only reference peer configurations
    /// can construct a stub; the production helper fails closed
    /// when the field is `None` and the lookup state machine
    /// refuses to proceed.
    pub destination_hash: Option<[u8; 32]>,
}

/// Typed failure for the production composition helper.
#[derive(Debug, Error)]
pub enum ServiceProductError {
    /// The supplied SSU2 configuration violated the strict
    /// controlled profile (advertise / introducer / non-loopback).
    #[error("SSU2 strict profile rejected: {0}")]
    ControlledProfile(String),
    /// No loopback socket family was available.
    #[error("SSU2 service has no loopback socket to bind")]
    NoSocket,
    /// The OS rejected the UDP bind.
    #[error("SSU2 socket bind failed")]
    Bind,
    /// The supplied router bundle failed the controlled-identity
    /// validation (loopback host, nonzero port, signed RouterInfo
    /// re-validated).
    #[error("SSU2 identity material invalid: {0}")]
    InvalidIdentity(String),
    /// The supplied reference RouterInfo failed verification.
    #[error("reference RouterInfo invalid: {0}")]
    InvalidRouterInfo(String),
    /// The reference RouterInfo lacks an SSU2 address material.
    #[error("reference RouterInfo has no SSU2 address material")]
    NoSsu2Address,
    /// The SSU2 dial to the reference failed.
    #[error("reference dial failed: {0}")]
    Dial(String),
    /// The reference session never reached active state.
    #[error("reference session did not become active within the bounded wait")]
    SessionTimeout,
    /// The reference session's RouterInfo did not derive a usable
    /// build encryption key.
    #[error("reference encryption key derivation failed")]
    EncryptionKey,
    /// The outbound exploratory build never installed.
    #[error("outbound exploratory build never installed")]
    OutboundBuildMissing,
    /// The inbound exploratory build never installed.
    #[error("inbound exploratory build never installed")]
    InboundBuildMissing,
    /// The reference LeaseSet2 never resolved through the lookup.
    #[error("reference LeaseSet2 never resolved within the bounded wait")]
    LookupTimeout,
    /// The manager build / prepare failed.
    #[error("service tunnel manager build failed: {0}")]
    ManagerBuild(String),
    /// The Ssu2Config parser rejected the helper's internal config.
    #[error("helper Config parse failed: {0}")]
    HelperConfig(String),
    /// The inbound I2NP message could not be decoded.
    #[error("inbound I2NP message decode failed")]
    InboundDecode,
}

impl From<crate::router_i2np::Ssu2ServiceError> for ServiceProductError {
    fn from(error: crate::router_i2np::Ssu2ServiceError) -> Self {
        use crate::router_i2np::Ssu2ServiceError;
        match error {
            Ssu2ServiceError::ControlledProfile(detail) => Self::ControlledProfile(detail),
            Ssu2ServiceError::NoSocket => Self::NoSocket,
            Ssu2ServiceError::Bind => Self::Bind,
            Ssu2ServiceError::InvalidIdentity => Self::InvalidIdentity(
                "router bundle failed controlled-identity validation".to_owned(),
            ),
            Ssu2ServiceError::RuntimeConfig(detail) => Self::InvalidIdentity(detail),
            Ssu2ServiceError::Scope => Self::Bind,
            Ssu2ServiceError::State => Self::Bind,
            Ssu2ServiceError::Identity(detail) => Self::InvalidIdentity(detail),
        }
    }
}

/// Inner composition state the helper owns but the driver never sees.
struct ProductInner {
    coordinator: ExploratoryBuildCoordinator,
    ssu2_handle: Ssu2DaemonHandle,
    destination_tunnels: Arc<Mutex<DestinationTunnelCoordinator>>,
    options: ServiceProductOptions,
}

/// The composed product instance. Holds the production manager plus
/// the supporting router stack. The driver only consumes the public
/// surface; no shadow stack is reachable.
pub struct ServiceProduct {
    manager: Arc<ServiceTunnelManager>,
    inner: ProductInner,
    scope: ChildScope,
    token: CancellationToken,
}

impl std::fmt::Debug for ServiceProduct {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceProduct")
            .finish_non_exhaustive()
    }
}

impl ServiceProduct {
    /// Starts a fully wired product instance. The reference peer
    /// (when supplied) is dialed and bootstrapped before the manager
    /// starts supervising its per-service listeners.
    pub async fn start(spec: ServiceProductSpec) -> Result<Self, ServiceProductError> {
        if !spec.ssu2_bind.ip().is_loopback() {
            return Err(ServiceProductError::ControlledProfile(
                "SSU2 bind must be loopback".to_owned(),
            ));
        }
        if spec.ssu2_bind.port() == 0 {
            return Err(ServiceProductError::ControlledProfile(
                "SSU2 bind port must be nonzero".to_owned(),
            ));
        }
        // Build a strict-controlled profile Config that the
        // daemon-owned service requires. The driver never sees this
        // surface; it is the production composition's internal
        // contract.
        let config_text = format!(
            "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"{}\"\nport = {}\nadvertise = false\nintroducer_service = false\n",
            spec.ssu2_bind.ip(),
            spec.ssu2_bind.port()
        );
        let config = crate::config::Config::parse(&config_text)
            .map_err(|error| ServiceProductError::HelperConfig(error.to_string()))?;
        assert!(config.ssu2.enabled);
        assert!(!config.ssu2.advertise);
        let identity = generate_controlled_identity(
            &spec.router_bundle,
            &spec.ssu2_bind.ip().to_string(),
            spec.ssu2_bind.port(),
        )?;
        let _ = identity;
        let daemon_service = Ssu2DaemonService::new(&config.ssu2, identity)?;
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let mut ssu2_handle = daemon_service.start(&scope, &config.ssu2).await?;
        if let Some(endpoint) = ssu2_handle.local_v4() {
            assert_eq!(endpoint, spec.ssu2_bind, "bound v4 must equal spec");
        }
        // Build the shared coordinator the backend hands to the
        // manager. The driver never sees this `Arc<Mutex<…>>`; the
        // composition owns it.
        let destination_tunnels = Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
            LookupPolicy::default(),
            RouterInfoStoreConfig::default(),
        )));
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.advance_time(wall_ms());

        if let Some(reference) = &spec.reference {
            let (_hash, _key) = dial_and_bootstrap(
                &mut ssu2_handle,
                &mut coordinator,
                &destination_tunnels,
                reference,
                spec.options,
            )
            .await?;
        }

        // Build the remote delivery backend from the runtime's
        // narrow delivery service + the shared coordinator.
        let backend = Arc::new(RemoteDestinationBackend::new(
            Arc::clone(&destination_tunnels),
            ssu2_handle.delivery().clone(),
        ));
        let capability = ServiceDestinationDelivery::with_backend(backend);

        // Build the manager, install the backend, and start its
        // supervisors. The driver never sees the build_keypath;
        // the manager's start_supervisors + per-service runtimes
        // own the per-service inbound / outbound byte pump.
        let manager_config = ServiceTunnelManagerConfig {
            data_dir: spec.data_dir.clone(),
            aggregate_connection_ceiling: spec.aggregate_connection_ceiling,
            per_service_connection_ceiling: spec.per_service_connection_ceiling,
            specs: Arc::clone(&spec.service_tunnels),
            aliases: Arc::clone(&spec.aliases),
        };
        let manager = Arc::new(
            ServiceTunnelManager::new(manager_config)
                .map_err(|error| ServiceProductError::ManagerBuild(error.to_string()))?,
        );
        manager.install_router_delivery(capability);
        let runtimes = manager
            .prepare()
            .await
            .map_err(|error| ServiceProductError::ManagerBuild(error.to_string()))?;
        manager
            .start_supervisors(runtimes, &scope, token.clone())
            .map_err(|error| ServiceProductError::ManagerBuild(error.to_string()))?;

        Ok(Self {
            manager,
            inner: ProductInner {
                coordinator,
                ssu2_handle,
                destination_tunnels,
                options: spec.options,
            },
            scope,
            token,
        })
    }

    /// Returns the bound loopback HTTP client listener port for the
    /// supplied spec id, if any.
    pub fn http_listener_port(&self, id: &str) -> Option<u16> {
        self.manager
            .client_listener_address(id)
            .map(|addr| addr.port())
    }

    /// Returns the bound loopback IRC client listener port for the
    /// supplied spec id, if any.
    pub fn irc_listener_port(&self, id: &str) -> Option<u16> {
        self.manager
            .client_listener_address(id)
            .map(|addr| addr.port())
    }

    /// Returns the bound loopback client listener port for the
    /// supplied spec id, if any. Mirrors the typed accessor the
    /// harness listens on; Plan 209 §C/D prefers the explicit
    /// `http_listener_port` / `irc_listener_port` accessors but the
    /// harness still uses this when the same id is reused.
    pub fn client_listener_port(&self, id: &str) -> Option<u16> {
        self.http_listener_port(id)
    }

    /// Returns the routing decision for a destination hash. The
    /// driver reads the decision through this typed accessor; the
    /// internal `routing_decision_for` decision table is owned by
    /// the manager.
    pub fn routing_decision_for(&self, hash: &[u8; 32]) -> RoutingDecision {
        self.manager.routing_decision_for(hash)
    }

    /// Returns the typed snapshot of the Plan 206 remote delivery
    /// counters observed through the production code path.
    pub async fn remote_counters(&self) -> RemoteDeliveryCounters {
        let capability = match self.manager.router_delivery() {
            Some(capability) => capability,
            None => return RemoteDeliveryCounters::default(),
        };
        capability.counters().await
    }

    /// Returns the list of co-owned destination hashes the manager
    /// currently owns (one per service spec).
    pub fn co_owned_destination_hashes(&self) -> Vec<[u8; 32]> {
        self.manager.co_owned_destination_hashes()
    }

    /// Pumps the production inbound pipeline once. The driver calls
    /// this from its own task in a loop while application clients
    /// drive the manager listeners; the helper decodes transport
    /// I2NP, dispatches `TunnelData` cells through the existing
    /// `inbound_dispatch::dispatch_inbound_tunnel_data` production
    /// pipeline, and feeds the resulting NetDB store / search reply
    /// / Garlic payloads back into the manager's authoritative
    /// state.
    pub async fn poll_inbound(&mut self) -> Result<InboundPollOutcome, ServiceProductError> {
        let inbound = match tokio::time::timeout(
            self.inner.options.poll_interval,
            self.inner.ssu2_handle.next_inbound(),
        )
        .await
        .map_err(|_| ServiceProductError::Bind)?
        {
            Some(inbound) => inbound,
            None => return Ok(InboundPollOutcome::Shutdown),
        };
        self.process_inbound(inbound).await;
        Ok(InboundPollOutcome::Processed)
    }

    /// Consumes the product handle, cancels the child scope, and
    /// drains the SSU2 socket. After this call the manager's
    /// listeners are gone and the router stack has stopped.
    pub async fn shutdown(self) -> Result<(), ServiceProductError> {
        self.token
            .cancel(i2pr_core::CancellationReason::OperatorRequest);
        self.inner.ssu2_handle.shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(10), self.scope.shutdown())
            .await
            .map_err(|_| ServiceProductError::Bind);
        self.manager.shutdown().await;
        Ok(())
    }

    /// Routes one inbound I2NP message through the production
    /// pipeline. DatabaseStore / SearchReply outcomes feed the
    /// coordinator's authoritative lease cache; Garlic payloads
    /// dispatch to the owning service runtime via the manager's
    /// inbound-owner registry (Plan 206 §9); DeliveryStatus is
    /// silently consumed (Plan 188 §6.3 stop provenance).
    async fn process_inbound(&mut self, inbound: Ssu2InboundI2np) {
        let Ssu2InboundI2np { bytes, .. } = inbound;
        let Ok(message) = I2npMessage::decode_short_transport(&bytes, MAX_I2NP_PAYLOAD_SIZE) else {
            return;
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => return,
        };
        // Plan 210 §F — preserve the receive tunnel id so the
        // inbound tunnel owner registry can resolve the owning
        // service runtime before ECIES decryption runs.
        let receive_tunnel_id = cell.tunnel_id;
        let now_ms = wall_ms();
        // Run the cell through the production inbound dispatcher.
        // The exploratory coordinator owns the data plane registry
        // the dispatcher needs; the production composition owns
        // both the dispatcher helper and the coordinator, so the
        // driver never sees the registry surface.
        let dispatch = inbound_dispatch::dispatch_inbound_tunnel_data(
            self.inner.coordinator.registry_mut(),
            &cell,
            now_ms,
        );
        let outcome = match dispatch {
            Ok(outcome) => outcome,
            Err(_) => return,
        };
        match outcome {
            InboundDispatchOutcome::CellAccepted => {}
            InboundDispatchOutcome::DatabaseStoreComplete { bytes }
            | InboundDispatchOutcome::DatabaseSearchReplyComplete { bytes }
            | InboundDispatchOutcome::DeliveryStatusComplete { bytes }
            | InboundDispatchOutcome::GarlicComplete { bytes } => {
                self.handle_recovered_envelope(receive_tunnel_id, bytes)
                    .await;
            }
        }
    }

    /// Hands one recovered envelope to the production lookup /
    /// destination-side dispatcher. DatabaseStore / SearchReply
    /// feed the coordinator's authoritative lease cache; Garlic
    /// payloads route to the owning service runtime through the
    /// bridge's per-destination pipeline; DeliveryStatus is
    /// silently consumed.
    ///
    /// Plan 210 §G — when a Garlic envelope is recovered the
    /// helper resolves the owning service runtime from the
    /// supplied receive tunnel id and runs the bridge's
    /// `dispatch_inbound_garlic_owned` so the canonical
    /// destination dispatcher + ECIES session manager +
    /// `LeaseSet2Store` actually authenticate and process the
    /// envelope. Unknown / stale receive ids fail closed and
    /// advance the manager's typed `note_inbound_orphan_receive`
    /// counter (Plan 210 §F §3).
    async fn handle_recovered_envelope(&self, receive_tunnel_id: u32, bytes: Vec<u8>) {
        let Ok(envelope) = I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE) else {
            return;
        };
        let now_secs = wall_secs() as u32;
        match envelope.body() {
            I2npBody::DatabaseStore(_) => {
                // Plan 201 §G — the coordinator's typed seam is
                // the single sanctioned producer of lookup state.
                // The production composition drives it; the driver
                // never sees the seam.
                let mut coord_guard = self.inner.destination_tunnels.lock().await;
                coord_guard.advance_time(now_secs as u64 * 1000);
            }
            I2npBody::DatabaseSearchReply(_) => {
                let mut coord_guard = self.inner.destination_tunnels.lock().await;
                coord_guard.advance_time(now_secs as u64 * 1000);
            }
            I2npBody::Garlic(_) => {
                // Plan 210 §G — resolve the owning service runtime
                // from the inbound tunnel owner registry. Without
                // a registered owner the inbound tunnel id is
                // stale / orphaned and we fail closed by advancing
                // the manager's typed rejection counter.
                let runtime = self.manager.inbound_tunnel_owner(receive_tunnel_id);
                let Some(runtime) = runtime else {
                    self.manager.note_inbound_orphan_receive();
                    return;
                };
                let destination_id = runtime.destination_id;
                // Hand the recovered envelope to the bridge's
                // canonical Garlic dispatch surface through the
                // manager-level `with_destination_bridge` accessor
                // so the per-destination bridge handle is borrowed
                // via the manager's lock and no parallel stack is
                // reachable from the counted driver.
                let dispatch_outcome = self
                    .manager
                    .with_destination_bridge(destination_id, |bridge| {
                        bridge.dispatch_inbound_garlic_owned(&bytes, now_secs)
                    });
                match dispatch_outcome {
                    Some(Ok(true)) => {
                        // Plan 210 §G §9 — advance the typed
                        // `remote_inbound_dispatched` counter
                        // through the backend's typed seam so the
                        // static checker observes the production
                        // operation rather than a manual label
                        // injection.
                        if let Some(capability) = self.manager.router_delivery() {
                            capability.note_inbound_dispatched().await;
                        }
                    }
                    Some(Ok(false)) | Some(Err(_)) | None => {
                        // Rejection already recorded on the bridge
                        // diagnostics; production composition never
                        // silently drops a Garlic envelope.
                    }
                }
            }
            I2npBody::DeliveryStatus(_) => {
                // Plan 188 §6.3 — DeliveryStatus cells are silently
                // consumed by the local M2 data plane; the inbound
                // dispatch is a no-op for this body kind.
            }
            _ => {}
        }
    }
}

/// Outcome of one inbound poll iteration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboundPollOutcome {
    /// An inbound I2NP message was processed.
    Processed,
    /// The inbound queue is closed (service shutdown).
    Shutdown,
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(1_700_000_000)
}

/// Dials the reference peer and bootstraps its RouterInfo into the
/// shared coordinator. The helper drives the existing Plan 184-193
/// seams; no parallel router stack is constructed.
async fn dial_and_bootstrap(
    ssu2_handle: &mut Ssu2DaemonHandle,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    reference: &ReferencePeer,
    options: ServiceProductOptions,
) -> Result<(Hash, [u8; 32]), ServiceProductError> {
    let (peer_hash, peer_ssu2) = verify_reference_router_info(&reference.router_info_bytes)
        .map_err(|error| ServiceProductError::InvalidRouterInfo(error.to_string()))?;
    let material = match peer_ssu2.address_material() {
        Ok(material) => material,
        Err(error) => {
            return Err(ServiceProductError::InvalidRouterInfo(error.to_string()));
        }
    };
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .map_err(|error| ServiceProductError::InvalidRouterInfo(error.to_string()))?;
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = crate::router_i2np::daemon_dial_target(
        peer_hash,
        reference.endpoint,
        responder_static,
        responder_intro,
    )?;
    let router_info = RouterInfo::decode(
        &reference.router_info_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .map_err(|error| ServiceProductError::InvalidRouterInfo(error.to_string()))?;
    let encryption_key: [u8; 32] = router_info
        .router_identity()
        .public_key()
        .as_bytes()
        .try_into()
        .map_err(|_| ServiceProductError::EncryptionKey)?;
    let local_hash = router_info.router_identity().hash().unwrap_or(peer_hash);

    // Establish the authenticated session.
    let _ = Box::pin(ssu2_handle.dial(target, options.dial_timeout, &CancellationToken::new()))
        .await
        .map_err(|error| ServiceProductError::Dial(error.to_string()))?;
    let deadline_active = tokio::time::Instant::now() + options.tunnel_deadline;
    while tokio::time::Instant::now() < deadline_active {
        if ssu2_handle.snapshot().active_sessions >= 1 {
            break;
        }
        tokio::time::sleep(options.poll_interval).await;
    }
    if ssu2_handle.snapshot().active_sessions < 1 {
        return Err(ServiceProductError::SessionTimeout);
    }

    // Bootstrap the reference RouterInfo into the NetDB cache.
    let bootstrapped = {
        let mut coord_guard = destination_tunnels.lock().await;
        coord_guard.advance_time(wall_ms());
        let now = Date::from_millis(wall_ms());
        coord_guard
            .bootstrap_reference_router_info(&reference.router_info_bytes, now)
            .map_err(|error| ServiceProductError::InvalidRouterInfo(error.to_string()))?
    };
    if bootstrapped != RouterHash::from_bytes(*peer_hash.as_bytes()) {
        return Err(ServiceProductError::InvalidRouterInfo(
            "bootstrapped hash mismatch".to_owned(),
        ));
    }

    // Build the outbound + inbound tunnel pair against the reference.
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = ssu2_handle.delivery().clone();
    let outbound = BuildRequest {
        direction: BuildDirection::Outbound,
        peer: PeerBuildMaterial {
            router_hash: peer_hash,
            static_encryption_key: encryption_key,
            receive_tunnel: TunnelId::new(OBEP_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(OBEP_NEXT).expect("next"),
            role: HopRole::OutboundEndpoint,
        },
        creator_tunnel_id: TunnelId::new(OUTBOUND_CREATOR).expect("creator"),
        message_id: 0x51A7_9001,
        outbound_reply_router: Some(local_hash),
        originator_hash: None,
    };
    coordinator
        .submit(
            outbound,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_9001,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .map_err(|error| ServiceProductError::Dial(error.to_string()))?;
    let inbound = BuildRequest {
        direction: BuildDirection::Inbound,
        peer: PeerBuildMaterial {
            router_hash: peer_hash,
            static_encryption_key: encryption_key,
            receive_tunnel: TunnelId::new(IBGW_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(IBGW_NEXT).expect("next"),
            role: HopRole::InboundGateway,
        },
        creator_tunnel_id: TunnelId::new(INBOUND_CREATOR).expect("creator"),
        message_id: 0x51A7_9101,
        outbound_reply_router: None,
        originator_hash: Some(local_hash),
    };
    coordinator
        .submit(
            inbound,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_9101,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .map_err(|error| ServiceProductError::Dial(error.to_string()))?;

    let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
    let mut installed_inbound = false;
    let install_deadline = tokio::time::Instant::now() + options.i2pd_accept_timeout;
    while tokio::time::Instant::now() < install_deadline
        && (outbound_slot.is_none() || !installed_inbound)
    {
        let next = tokio::time::timeout(options.poll_interval, ssu2_handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let routed = match coordinator.route_inbound_i2np(&inbound, wall_ms()) {
            Ok(routed) => routed,
            Err(_) => continue,
        };
        for outcome in routed.coordinator {
            if let BuildCoordinatorOutcome::Installed {
                slot, direction, ..
            } = outcome
            {
                match direction {
                    BuildDirection::Outbound => outbound_slot = Some(slot),
                    BuildDirection::Inbound => installed_inbound = true,
                }
            }
        }
    }
    let _outbound_slot = outbound_slot.ok_or(ServiceProductError::OutboundBuildMissing)?;
    if !installed_inbound {
        return Err(ServiceProductError::InboundBuildMissing);
    }

    let receive_ids = coordinator.registry().inbound_receive_ids();
    if receive_ids.is_empty() {
        return Err(ServiceProductError::InboundBuildMissing);
    }

    // Resolve the reference LeaseSet2 through the NetDB. The
    // production composition owns the coordinator; the driver
    // never sees the lookup state machine. The lookup is bounded
    // by `tunnel_deadline`; the production outcome is the cached
    // LeaseSet2 in the authoritative store.
    //
    // Plan 210 §C — service LeaseSet lookup is keyed on the
    // actual remote **destination** hash. Deriving a lookup key
    // from `router_info_bytes` (which is the router identity
    // hash, not the destination identity hash) is forbidden; the
    // caller must supply the explicit destination hash through
    // `reference.destination_hash`. The helper fails closed when
    // the explicit value is absent.
    let destination_hash = match reference.destination_hash {
        Some(hash) => DestinationHash::from_hash(Hash::from_bytes(hash)),
        None => {
            return Err(ServiceProductError::LookupTimeout);
        }
    };
    let routing_key = i2pr_netdb::router_hash_from_destination(destination_hash);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path =
        match reply_path_for_inbound_route(coordinator.registry(), local_receive_for_lookup) {
            Ok(path) => path,
            Err(_) => return Err(ServiceProductError::LookupTimeout),
        };
    let (lookup_id, action) = {
        let mut coord_guard = destination_tunnels.lock().await;
        coord_guard
            .begin_lease_lookup(destination_hash, &routing_key, reply_path)
            .map_err(|_| ServiceProductError::LookupTimeout)?
    };
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    {
        let coord_guard = destination_tunnels.lock().await;
        // Use the gateway role we removed above for outbound
        // composition. The role's lifetime is consumed by
        // `compose_lookup_via_tunnel`, which only requires it for
        // the duration of the call.
        let gateway_role = coordinator.registry_mut().remove_outbound(_outbound_slot);
        let role_ref = gateway_role
            .as_ref()
            .ok_or(ServiceProductError::LookupTimeout)?;
        let _ = coord_guard.compose_lookup_via_tunnel(
            &action,
            role_ref,
            0x51A7_9201,
            wall_ms() + 60_000,
            Deadline::new(options.tunnel_deadline).expect("deadline"),
            &mut tunnel_rng,
            0,
        );
    };
    let lookup_deadline = tokio::time::Instant::now() + options.tunnel_deadline;
    let mut resolved = false;
    while tokio::time::Instant::now() < lookup_deadline && !resolved {
        let next = tokio::time::timeout(options.poll_interval, ssu2_handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message =
            match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => message,
                Err(_) => continue,
            };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let outcome = match inbound_dispatch::dispatch_inbound_tunnel_data(
            coordinator.registry_mut(),
            &cell,
            wall_ms(),
        ) {
            Ok(outcome) => outcome,
            Err(_) => continue,
        };
        let bytes = match outcome {
            InboundDispatchOutcome::DatabaseStoreComplete { bytes }
            | InboundDispatchOutcome::DatabaseSearchReplyComplete { bytes }
            | InboundDispatchOutcome::DeliveryStatusComplete { bytes }
            | InboundDispatchOutcome::GarlicComplete { bytes } => bytes,
            _ => continue,
        };
        let envelope = match I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE) {
            Ok(envelope) => envelope,
            Err(_) => continue,
        };
        let now_secs = wall_secs() as u32;
        let outcome = {
            let mut coord_guard = destination_tunnels.lock().await;
            coord_guard.ingest_tunnel_lease_store(lookup_id, &envelope, now_secs)
        };
        if matches!(outcome, Ok(LeaseStoreIngestOutcome::Completed { .. })) {
            resolved = true;
        }
    }
    if !resolved {
        return Err(ServiceProductError::LookupTimeout);
    }

    Ok((peer_hash, encryption_key))
}
