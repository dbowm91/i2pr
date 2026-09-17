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
use crate::sam::streams::RouterNetworkSummary;
use crate::service_delivery::{
    RemoteDeliveryCounters, RemoteDestinationBackend, RoutingDecision, ServiceDestinationDelivery,
};
use crate::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

/// Plan 212 §9 — per-process tunnel-id allocator for real
/// per-service Destination tunnel material.
///
/// Rules (no global mutable static, no external crate):
/// - never emit zero;
/// - never reuse an id still present in the registry (the caller
///   checks the registry; the allocator guarantees process-local
///   disjointness across every build request);
/// - allocate a disjoint set for every build request;
/// - bounded/wrapping collision search;
/// - deterministic unit tests allowed with an injected seed/start.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan212TunnelIdAllocator {
    next: u32,
}

impl Plan212TunnelIdAllocator {
    /// Creates an allocator starting at `start` (non-zero start
    /// preferred; zero start wraps to 1 on first allocation).
    pub(crate) const fn new(start: u32) -> Self {
        Self { next: start }
    }

    /// Allocates one non-zero tunnel id, wrapping on overflow and
    /// skipping zero. The caller must still verify the id is not
    /// present in the registry and retry on collision (bounded).
    pub(crate) fn allocate(&mut self) -> u32 {
        loop {
            let candidate = self.next.wrapping_add(1);
            // Wrapping add from u32::MAX lands on 0; skip it.
            self.next = if candidate == 0 { 1 } else { candidate };
            if self.next != 0 {
                return self.next;
            }
        }
    }

    /// Allocates `count` disjoint non-zero ids.
    pub(crate) fn allocate_set(&mut self, count: usize) -> Vec<u32> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(self.allocate());
        }
        out
    }
}

/// Tunable tunnel ids the production composition owns end-to-end.
/// Two pairs — outbound (creator + OBEP) and inbound (IBGW) —
/// install through the existing `ExploratoryBuildCoordinator`
/// (Plan 185) and never appear in driver code.
///
/// Plan 212 §9 — the fixed constants below remain the default seed
/// for the first service only; additional services allocate
/// disjoint ids through [`Plan212TunnelIdAllocator`] so multiple
/// service Destinations coexist in one process.
/// Retained first-service seed values (Plan 209). Plan 212 §9
/// allocates disjoint ids per service via
/// [`Plan212TunnelIdAllocator`]; the constants below document the
/// historical single-pair layout and remain the allocator's
/// reference seed base.
#[allow(dead_code)]
const OUTBOUND_CREATOR: u32 = 0x1580;
#[allow(dead_code)]
const OBEP_RECEIVE: u32 = 0x9581;
#[allow(dead_code)]
const OBEP_NEXT: u32 = 0x9582;
#[allow(dead_code)]
const INBOUND_CREATOR: u32 = 0x1680;
#[allow(dead_code)]
const IBGW_RECEIVE: u32 = 0x9681;
#[allow(dead_code)]
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
///
/// Plan 212 §8 — `ReferencePeer` is router transport/bootstrap
/// metadata only. It must not represent one application
/// Destination: router bootstrap (`dial_and_bootstrap`) does only
/// RouterInfo verification, authoritative RouterInfo bootstrap,
/// authenticated SSU2 dial/session establishment, and peer material
/// preparation for real tunnel builds. Application LeaseSet2
/// lookup is a separate per-service operation
/// (`resolve_remote_destination_for_service`) keyed by the
/// actual remote Destination hash derived from the service's
/// `DestinationRef` (Base32Hash / ConfiguredDestination /
/// StaticAlias); local co-owned hashes stay local and never enter
/// the remote path. HTTP and IRC targets resolve independently in
/// the same product instance.
#[derive(Clone, Debug)]
pub struct ReferencePeer {
    /// RouterInfo bytes (unmodified public material).
    pub router_info_bytes: Vec<u8>,
    /// Loopback UDP endpoint for the dial.
    pub endpoint: SocketAddr,
}

/// Plan 213 §C1 — public addressing material for one service
/// Destination.
///
/// All three forms describe the same identity: `destination_b64`
/// is the canonical public Destination base64,
/// `destination_hash` is SHA-256 over those public bytes, and
/// `destination_b32` is the canonical `<52-char base32>.b32.i2p`
/// form of that hash. No private material is carried.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceDestinationPublicInfo {
    /// SHA-256 of the canonical public Destination encoding.
    pub destination_hash: [u8; 32],
    /// Canonical `<52-char base32>.b32.i2p` form of the hash.
    pub destination_b32: String,
    /// Canonical public Destination base64.
    pub destination_b64: String,
}

/// Plan 213 §C1 — bounded RFC 4648 base32 encoder (lowercase, no
/// padding) for exactly 32 bytes. 256 bits encode as 51 full
/// 5-bit groups plus one final group holding the remaining bit,
/// yielding the canonical 52-character I2P base32 label. No
/// allocation beyond the returned label; no external crate.
fn plan213_base32_encode_32(bytes: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(52);
    let mut accumulator: u32 = 0;
    let mut bits: u8 = 0;
    for byte in bytes.iter() {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((accumulator >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((accumulator << (5 - bits)) & 31) as usize] as char);
    }
    out
}

#[cfg(test)]
mod plan213_public_info_tests {
    //! Plan 213 §C1/§15 — focused unit rows for the public
    //! service-Destination material surface (no network, routine
    //! CI): the base32 label encoding is canonical, and the
    //! hash/b32/base64 triple stays mutually consistent.

    use super::plan213_base32_encode_32;

    #[test]
    fn plan213_base32_zero_hash_is_canonical() {
        assert_eq!(plan213_base32_encode_32(&[0_u8; 32]), "a".repeat(52));
    }

    #[test]
    fn plan213_base32_ones_hash_is_canonical() {
        // 256 bits = 51 full 5-bit groups of ones plus one final
        // group holding the single remaining one bit padded with
        // four zero bits (10000b = index 16 = 'q').
        assert_eq!(
            plan213_base32_encode_32(&[0xFF_u8; 32]),
            format!("{}q", "7".repeat(51))
        );
    }

    #[test]
    fn plan213_base32_label_round_trips_through_config_parser() {
        let hash = *i2pr_crypto::sha256(b"plan213-public-info-vector").as_bytes();
        let label = plan213_base32_encode_32(&hash);
        assert_eq!(label.len(), 52);
        assert!(
            label
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'2'..=b'7'))
        );
        let b32 = format!("{label}.b32.i2p");
        let parsed =
            i2pr_service_tunnels::DestinationRef::parse(&b32).expect("canonical b32 parses");
        match parsed {
            i2pr_service_tunnels::DestinationRef::Base32Hash {
                hash: parsed_hash, ..
            } => assert_eq!(parsed_hash, hash),
            _ => panic!("canonical b32 must parse as Base32Hash"),
        }
    }
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
    /// Per-service router-backed provisioning failed (real
    /// outbound/inbound build, LS2 derivation, install, owner
    /// registration, target lookup, or server publication).
    #[error("service router provisioning failed: {0}")]
    Provisioning(String),
    /// Router-backed network state is missing or expired for a
    /// counted remote path.
    #[error("router network state missing or expired: {0}")]
    RouterState(String),
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

/// Plan 212 §15 — decodes one inbound SSU2 I2NP envelope with
/// the established production dispatcher ordering
/// (standard-first / short-transport-fallback, mirroring
/// `router_i2np::dispatch_router_i2np`). Do not add a third
/// independent decoder implementation.
fn decode_inbound_ssu2_i2np(bytes: &[u8]) -> Result<I2npMessage, String> {
    match I2npMessage::decode_standard(bytes, MAX_I2NP_PAYLOAD_SIZE) {
        Ok(message) => Ok(message),
        Err(standard_err) => {
            match I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => Ok(message),
                Err(_) => Err(format!("inbound I2NP decode failed: {standard_err:?}")),
            }
        }
    }
}

impl ServiceProduct {
    /// Starts a fully wired product instance.
    ///
    /// Plan 212 §7 ordering:
    /// 1. validate controlled SSU2 config;
    /// 2. start shared SSU2 service;
    /// 3. bootstrap/verify reference RouterInfo only;
    /// 4. construct ServiceTunnelManager;
    /// 5. `manager.prepare()` to create service identities/listeners
    ///    WITHOUT starting supervisors;
    /// 6. for every enabled remote-capable service runtime: build
    ///    real outbound + inbound tunnel material, derive the
    ///    service's real local LS2, install router-backed state on
    ///    that exact service runtime, register real inbound receive
    ///    tunnel ownership, resolve configured remote target LS2
    ///    (client profiles), publish local LS2 when server
    ///    reachability requires it;
    /// 7. only after all required services are network-ready: start
    ///    service supervisors;
    /// 8. fail atomically if any mandatory provisioning fails (no
    ///    half-started application listeners accepting traffic).
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
        // composition owns it. One router-wide SSU2 service, one
        // router delivery service, one destination coordinator, one
        // exploratory coordinator, one authoritative NetDB store —
        // services consume typed handles/material, never owned
        // transports (Plan 212 §4).
        let destination_tunnels = Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
            LookupPolicy::default(),
            RouterInfoStoreConfig::default(),
        )));
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.advance_time(wall_ms());

        // Plan 212 §8 — router bootstrap only (no application
        // LeaseSet2 lookup). Application lookup is per-service
        // after `prepare()` so HTTP and IRC resolve independent
        // target hashes in the same product instance.
        //
        // Plan 213 P213-C — our controlled router hash comes from
        // the same bundle that signed the controlled RouterInfo
        // (never from the reference RouterInfo: that hash
        // addresses the reference itself and would route our
        // build replies away from us).
        let local_router_hash = spec.router_bundle.identity().hash().map_err(|error| {
            ServiceProductError::InvalidIdentity(format!("router hash: {error:?}"))
        })?;
        let router_peer = if let Some(reference) = &spec.reference {
            Some(
                dial_and_bootstrap_router_only(
                    &mut ssu2_handle,
                    &destination_tunnels,
                    reference,
                    local_router_hash,
                    spec.options,
                )
                .await?,
            )
        } else {
            None
        };

        // Build the remote delivery backend from the runtime's
        // narrow delivery service + the shared coordinator.
        let backend = Arc::new(RemoteDestinationBackend::new(
            Arc::clone(&destination_tunnels),
            ssu2_handle.delivery().clone(),
        ));
        let capability = ServiceDestinationDelivery::with_backend(backend);

        // Build the manager, install the backend, and prepare
        // service identities/listeners WITHOUT starting
        // supervisors yet.
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

        // Plan 212 §7 step 6 — per-service real network
        // provisioning before any supervisor accepts application
        // traffic. When no reference peer is configured (local
        // product) this is a no-op and supervisors start
        // immediately.
        if let Some(peer) = router_peer {
            let mut allocator = Plan212TunnelIdAllocator::new(0x51A7_9300);
            if let Err(error) = provision_all_service_router_material(
                &manager,
                &mut coordinator,
                &destination_tunnels,
                &mut ssu2_handle,
                &peer,
                &runtimes,
                spec.options,
                &mut allocator,
            )
            .await
            {
                // Fail atomically: tear down staged listeners and
                // the router stack so no half-started listener
                // accepts traffic after a provisioning failure.
                manager.shutdown().await;
                token.cancel(i2pr_core::CancellationReason::OperatorRequest);
                ssu2_handle.shutdown();
                let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
                return Err(error);
            }
        }

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

    /// Returns the manager's typed inbound-orphan-receive count
    /// (Plan 210 §F). The counter advances only when a recovered
    /// Garlic envelope arrives on a receive TunnelId with no
    /// registered owning service runtime. The qualification driver
    /// snapshots it per direction and requires a zero delta.
    pub fn inbound_orphan_receives(&self) -> u64 {
        self.manager.inbound_orphan_receives() as u64
    }

    /// Returns the list of co-owned destination hashes the manager
    /// currently owns (one per service spec).
    pub fn co_owned_destination_hashes(&self) -> Vec<[u8; 32]> {
        self.manager.co_owned_destination_hashes()
    }

    /// Returns the public addressing material for one service
    /// Destination, if the manager currently owns it.
    ///
    /// Plan 213 §C1 — the external reference needs the i2pr
    /// GenericServer public Destination for its ordinary
    /// `STREAM CONNECT`. This is the narrowest read-only product
    /// surface for that need: it reuses the existing manager-level
    /// [`ServiceTunnelManager::service_destination_b64`] public
    /// encoding (derived from the service `DestinationIdentity`
    /// public structure) and derives the hash/b32 forms from those
    /// same public bytes. No private key, ECIES session secret,
    /// tunnel layer key, or SSU2 private material crosses this
    /// boundary; a server Destination is necessarily public
    /// addressing material.
    pub fn service_destination_public_info(
        &self,
        spec_id: &str,
    ) -> Option<ServiceDestinationPublicInfo> {
        let destination_b64 = self.manager.service_destination_b64(spec_id)?;
        let public_bytes = i2pr_api::sam::base64::decode(&destination_b64, 4096).ok()?;
        let destination_hash = *i2pr_crypto::sha256(&public_bytes).as_bytes();
        let destination_b32 = format!("{}.b32.i2p", plan213_base32_encode_32(&destination_hash));
        Some(ServiceDestinationPublicInfo {
            destination_hash,
            destination_b32,
            destination_b64,
        })
    }

    /// Returns the router-backed network summary for one service
    /// spec, if router-backed state is installed.
    ///
    /// Plan 213 §E2 — the qualification driver derives
    /// real-outbound/inbound installed, inbound receive count, and
    /// expiry facts from this typed product summary instead of
    /// asserting literal success. Read-only; never exposes secret
    /// material.
    pub fn service_router_network_summary(&self, spec_id: &str) -> Option<RouterNetworkSummary> {
        let destination_id = self.manager.service_destination_id(spec_id)?;
        self.manager.service_router_network_summary(destination_id)
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
    ///
    /// Plan 212 §15 — the outer SSU2 I2NP decode mirrors the
    /// established production dispatcher ordering
    /// (standard-first / short-transport-fallback). If the SSU2
    /// runtime guarantees short transport here, that guarantee is
    /// documented by the fallback still succeeding; both forms are
    /// accepted without a third decoder implementation.
    async fn process_inbound(&mut self, inbound: Ssu2InboundI2np) {
        let Ssu2InboundI2np { bytes, .. } = inbound;
        let Ok(message) = decode_inbound_ssu2_i2np(&bytes) else {
            return;
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            I2npBody::Garlic(_) => {
                self.handle_direct_garlic(&message, &bytes).await;
                return;
            }
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

    /// Handles one direct (non-tunneled) Garlic message from the
    /// SSU2 session. Gw-self peers (like the controlled reference)
    /// send Garlic replies directly instead of via tunnels when
    /// their outbound tunnel pool is not ready; dropping them would
    /// stall every fast-lane handshake. The message is normalized
    /// to the standard encoding the canonical dispatcher requires,
    /// then attributed by bounded try-each over the committed
    /// service set: ECIES authentication rejects cross-service
    /// misattribution without state effects, and per-service
    /// dispatch fails closed without router state.
    async fn handle_direct_garlic(&mut self, message: &I2npMessage, raw_bytes: &[u8]) {
        use i2pr_proto::I2npHeader;
        let standard_bytes: Vec<u8> = match message.header() {
            I2npHeader::Standard { .. } => raw_bytes.to_vec(),
            I2npHeader::ShortTransport { .. } => {
                // Re-frame the short header as standard (same body,
                // same message id, seconds-to-millis expiration).
                // ShortSsu never arrives here (decode rejects it).
                let (message_id, expiration_secs) = match message.header() {
                    I2npHeader::ShortTransport {
                        message_id,
                        expiration_seconds,
                        ..
                    } => (message_id, expiration_seconds),
                    _ => return,
                };
                let rebuilt = match I2npMessage::new_standard(
                    message_id,
                    Date::from_millis(u64::from(expiration_secs).saturating_mul(1_000)),
                    match message.body() {
                        I2npBody::Garlic(body) => I2npBody::Garlic(body.clone()),
                        _ => return,
                    },
                ) {
                    Ok(rebuilt) => rebuilt,
                    Err(_) => return,
                };
                match rebuilt.encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE) {
                    Ok(bytes) => bytes,
                    Err(_) => return,
                }
            }
            _ => return,
        };
        let now_secs = wall_secs() as u32;
        let now_ms = wall_ms();
        // Bounded try-each over the committed service set (client
        // and server). The first ECIES-authenticated dispatch wins;
        // unauthenticated candidates fail closed with no state
        // effects, so attribution is exact.
        for (destination_id, is_server) in self.manager.router_service_candidates() {
            let report = match self.manager.dispatch_router_inbound_to_canonical_streaming(
                destination_id,
                &standard_bytes,
                now_secs,
                now_ms,
                is_server,
            ) {
                Ok(report) => report,
                Err(_) => continue,
            };
            if !report.garlic_authenticated {
                continue;
            }
            if report.streaming_packets_accepted > 0
                && let Some(capability) = self.manager.router_delivery()
            {
                capability.note_inbound_dispatched().await;
            }
            return;
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
    /// supplied receive tunnel id. Plan 212 §14 completes the
    /// path: the recovered envelope authenticates through the
    /// router-backed destination session state, every dequeued
    /// destination payload enters
    /// `StreamingDestinationAdapter::receive` against the SAME
    /// canonical service `StreamingManager`, and
    /// `remote_inbound_dispatched` advances ONLY after >=1
    /// Streaming payload was accepted. Unknown / stale receive
    /// ids fail closed and advance the manager's typed
    /// `note_inbound_orphan_receive` counter (Plan 210 §F §3).
    async fn handle_recovered_envelope(&self, receive_tunnel_id: u32, bytes: Vec<u8>) {
        let Ok(envelope) = I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE) else {
            return;
        };
        let now_secs = wall_secs() as u32;
        let now_ms = wall_ms();
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
                // Plan 210 §F + Plan 212 §14 — resolve the owning
                // service runtime from the inbound tunnel owner
                // registry (receive TunnelId -> owner, selected
                // before ECIES decryption). Without a registered
                // owner the id is stale / orphaned: fail closed.
                let runtime = self.manager.inbound_tunnel_owner(receive_tunnel_id);
                let Some(runtime) = runtime else {
                    self.manager.note_inbound_orphan_receive();
                    return;
                };
                let destination_id = runtime.destination_id;
                // Plan 213: server profiles feed the receiver mirror
                // (the polled server loop accepts there); clients feed
                // the canonical manager that owns the outbound state.
                let is_server = self.manager.spec_is_server(&runtime.spec_id);
                // Plan 212 §14 — authenticate + drain + receive
                // into the canonical service StreamingManager via
                // the manager wrapper (which also wakes the
                // existing outbound delivery driver when receive
                // queued a response).
                let report = match self.manager.dispatch_router_inbound_to_canonical_streaming(
                    destination_id,
                    &bytes,
                    now_secs,
                    now_ms,
                    is_server,
                ) {
                    Ok(report) => report,
                    Err(_) => {
                        return;
                    }
                };
                // Plan 212 §14 step 8 — advance the typed
                // `remote_inbound_dispatched` counter ONLY after
                // >=1 Streaming payload was accepted through the
                // typed backend seam. Garlic auth without payload
                // never increments it.
                if report.streaming_packets_accepted > 0
                    && let Some(capability) = self.manager.router_delivery()
                {
                    capability.note_inbound_dispatched().await;
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

/// Plan 212 §8 — router peer material prepared by router-only
/// bootstrap. Carries only transport/bootstrap facts; never an
/// application Destination hash.
#[derive(Clone, Copy, Debug)]
struct RouterPeerMaterial {
    peer_hash: Hash,
    encryption_key: [u8; 32],
    local_hash: Hash,
}

/// Plan 212 §8 — dials the reference peer and bootstraps its
/// RouterInfo into the shared coordinator. Router bootstrap only:
/// RouterInfo verification, authoritative RouterInfo bootstrap,
/// authenticated SSU2 dial/session establishment, and peer material
/// preparation for real tunnel builds. It must NOT perform an
/// application LeaseSet2 lookup; per-service lookup is
/// `resolve_remote_destination_for_service`.
///
/// `local_router_hash` is OUR controlled router hash (the caller
/// derives it from the router bundle that signed the controlled
/// RouterInfo). It feeds `outbound_reply_router` /
/// `originator_hash` on every build request so the reference
/// routes build replies back to us. Deriving it from the
/// reference RouterInfo instead would address our replies to the
/// reference itself (Plan 213 P213-C corrective: the transcript
/// showed the reference creating our endpoint while no install
/// ever arrived).
async fn dial_and_bootstrap_router_only(
    ssu2_handle: &mut Ssu2DaemonHandle,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    reference: &ReferencePeer,
    local_router_hash: Hash,
    options: ServiceProductOptions,
) -> Result<RouterPeerMaterial, ServiceProductError> {
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
    // Router bootstrap only — no tunnel builds, no application
    // LeaseSet2 lookup here. Per-service tunnel material is
    // provisioned after `manager.prepare()` so the real roles bind
    // to the owning service Destination runtime (Plan 212 §7).
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

    Ok(RouterPeerMaterial {
        peer_hash,
        encryption_key,
        local_hash: local_router_hash,
    })
}

/// Plan 212 §8 — separate bounded operation for each service
/// target: ordinary remote LeaseSet2 lookup by the actual remote
/// Destination hash, using the service's real outbound role and
/// real inbound reply route.
///
/// Steps (per service target):
/// 1. compute the routing key via the existing NetDB helper;
/// 2. select the service's real inbound reply route;
/// 3. `DestinationTunnelCoordinator::begin_lease_lookup`;
/// 4. compose lookup via the service's real router-backed outbound
///    role using `compose_lookup_via_tunnel`;
/// 5. pump returned TunnelData through `inbound_dispatch`;
/// 6. ingest DatabaseStore / SearchReply through the coordinator;
/// 7. require a validated cached LS2 for exactly the target hash;
/// 8. install that validated LS2 into that service's router-backed
///    `DestinationRouting`.
///
/// Cache sharing at the router-wide coordinator is allowed;
/// per-service routing installation is still required because each
/// service owns independent ECIES/Streaming state. Never pre-seed
/// the counted target LS2 directly in the service routing table.
///
/// Plan 213 P213-E — the lookup brackets the typed resolution
/// accounting surface: `begin_resolution` advances
/// `remote_lookup_started` through the typed seam before any
/// network I/O, and the outcome reports `remote_lookup_succeeded`
/// / `remote_lookup_failed` through the documented observation
/// surface once the validated record is (or is not) installed.
/// Without this bracket the real provision-time lookup ran
/// silently and the qualification's lookup rows could only be
/// asserted, never observed.
#[allow(clippy::too_many_arguments)]
async fn resolve_remote_destination_for_service(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    service_destination: i2pr_client::DestinationId,
    destination_hash: DestinationHash,
    options: ServiceProductOptions,
) -> Result<(), ServiceProductError> {
    let capability = manager.router_delivery();
    let target_bytes = *destination_hash.as_bytes();
    let now_ms = wall_ms();
    let deadline_ms = now_ms.saturating_add(
        options
            .tunnel_deadline
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    );
    let resolution = match &capability {
        Some(capability) => Some(
            capability
                .begin_resolution(target_bytes, now_ms, deadline_ms)
                .await
                .map_err(|error| {
                    ServiceProductError::Provisioning(format!("lookup resolution: {error:?}"))
                })?,
        ),
        None => None,
    };
    let outcome = resolve_remote_destination_for_service_inner(
        manager,
        coordinator,
        destination_tunnels,
        ssu2_handle,
        service_destination,
        destination_hash,
        options,
    )
    .await;
    if let Some(capability) = &capability {
        if let Some(resolution) = resolution {
            capability.complete_resolution(resolution.id).await;
        }
        capability
            .record_observation(if outcome.is_ok() {
                "remote_lookup_succeeded"
            } else {
                "remote_lookup_failed"
            })
            .await;
    }
    outcome
}

#[allow(clippy::too_many_arguments)]
async fn resolve_remote_destination_for_service_inner(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    service_destination: i2pr_client::DestinationId,
    destination_hash: DestinationHash,
    options: ServiceProductOptions,
) -> Result<(), ServiceProductError> {
    let routing_key = i2pr_netdb::router_hash_from_destination(destination_hash);
    // Select the service's real inbound reply route from its
    // installed router-backed receive ids (never hard-coded Plan
    // 193 constants).
    let receive_ids = manager
        .with_destination_bridge(service_destination, |bridge| {
            bridge.router_inbound_receive_ids()
        })
        .unwrap_or_default();
    let local_receive_for_lookup = receive_ids.first().copied().ok_or_else(|| {
        ServiceProductError::Provisioning("service has no inbound receive ids".to_owned())
    })?;
    let local_receive = TunnelId::new(local_receive_for_lookup)
        .map_err(|_| ServiceProductError::Provisioning("invalid receive tunnel id".to_owned()))?;
    let reply_path = reply_path_for_inbound_route(coordinator.registry(), local_receive)
        .map_err(|_| ServiceProductError::LookupTimeout)?;
    let (lookup_id, action) = {
        let mut coord_guard = destination_tunnels.lock().await;
        coord_guard
            .begin_lease_lookup(destination_hash, &routing_key, reply_path)
            .map_err(|_| ServiceProductError::LookupTimeout)?
    };
    // Compose the lookup through the service's real router-backed
    // outbound role (borrow by reference; no removal, no
    // placeholder) and dispatch the cells through the shared
    // router delivery service. The borrow stays inside the bridge
    // closure so installation ownership never moves.
    //
    // Bounded NetDB-level re-query: a floodfill that has not yet
    // settled the reply tunnel's inbound side answers into a
    // black hole (observed against exact-pinned i2pd 2.61.0 in an
    // isolated lane: the reply is gatewayed toward transit that
    // never completes instead of the reply tunnel). Re-sending
    // the same composed query after a bounded settle delay lets a
    // later copy land once the reference side has settled; this
    // is ordinary floodfill-churn tolerance, not a wire change.
    // At most MAX_LOOKUP_ATTEMPTS transmissions per service, each
    // with a bounded pump window; pump accounting is cumulative
    // across attempts for the timeout attribution below.
    const MAX_LOOKUP_ATTEMPTS: u64 = 3;
    const LOOKUP_ATTEMPT_WINDOW: Duration = Duration::from_secs(25);
    const LOOKUP_SETTLE_DELAY: Duration = Duration::from_secs(5);
    let mut resolved = false;
    let mut attempts: u64 = 0;
    let mut inbound_seen: u64 = 0;
    let mut decode_ok: u64 = 0;
    let mut tunnel_data: u64 = 0;
    let mut dispatch_ok: u64 = 0;
    let mut envelope_ok: u64 = 0;
    let mut ingest_completed: u64 = 0;
    let mut ingest_continued: u64 = 0;
    // Direct (non-tunneled) body census: Gw-self peers may answer
    // outside the reply tunnel, which this loop intentionally
    // skips; the census tells a no-tunneldata timeout apart from
    // a silent transport.
    let mut direct_garlic: u64 = 0;
    let mut direct_store: u64 = 0;
    let mut direct_search_reply: u64 = 0;
    let mut direct_delivery_status: u64 = 0;
    let mut direct_other: u64 = 0;
    // I2NP type-number census for non-tunneled traffic (type
    // numbers only, no payload). Bounded: at most one entry per
    // observed type number.
    let mut direct_types: Vec<(u8, u64)> = Vec::new();
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    while !resolved && attempts < MAX_LOOKUP_ATTEMPTS {
        attempts += 1;
        {
            let coord_guard = destination_tunnels.lock().await;
            let delivery = ssu2_handle.delivery().clone();
            // Compose inside the bridge borrow: the bridge exposes
            // the router-backed outbound role by reference; the
            // coordinator composes the DatabaseLookup cells; the
            // delivery service carries them to the first hop.
            let deadline = Deadline::new(options.tunnel_deadline)
                .map_err(|_| ServiceProductError::LookupTimeout)?;
            let dispatch = manager
                .with_destination_bridge(service_destination, |bridge| {
                    let role = bridge.router_outbound_role_ref()?;
                    if !bridge.has_router_network_state(wall_ms()) {
                        return None;
                    }
                    let (dispatch, _proof) = coord_guard
                        .compose_lookup_via_tunnel(
                            &action,
                            role.role(),
                            0x51A7_9201,
                            wall_ms() + 60_000,
                            deadline,
                            &mut tunnel_rng,
                            0,
                        )
                        .ok()?;
                    Some(dispatch)
                })
                .flatten()
                .ok_or(ServiceProductError::LookupTimeout)?;
            for cell_delivery in &dispatch.deliveries {
                let request = crate::router_i2np::RouterDeliveryRequest::new(
                    cell_delivery.target(),
                    cell_delivery.message_bytes().to_vec(),
                    options.delivery_timeout,
                )
                .map_err(|_| ServiceProductError::LookupTimeout)?;
                let outcome = delivery.deliver(request, &CancellationToken::new());
                if !matches!(outcome, crate::router_i2np::RouterDeliveryOutcome::Accepted) {
                    return Err(ServiceProductError::LookupTimeout);
                }
            }
        };
        // Pump returned TunnelData through the existing inbound_dispatch
        // path and ingest DatabaseStore / SearchReply until the
        // validated LS2 for exactly the target hash is cached.
        // Bounded outcome accounting attributes a timeout to the
        // exact stage (transport / decode / dispatch / envelope /
        // ingest) without logging key material or payload bytes.
        let attempt_deadline = tokio::time::Instant::now() + LOOKUP_ATTEMPT_WINDOW;
        while tokio::time::Instant::now() < attempt_deadline && !resolved {
            let next =
                tokio::time::timeout(options.poll_interval, ssu2_handle.next_inbound()).await;
            let Ok(Some(inbound)) = next else {
                continue;
            };
            inbound_seen += 1;
            let message = match decode_inbound_ssu2_i2np(&inbound.bytes) {
                Ok(message) => message,
                Err(_) => continue,
            };
            decode_ok += 1;
            let cell = match message.body() {
                I2npBody::TunnelData(cell) => cell.clone(),
                I2npBody::Garlic(_) => {
                    direct_garlic += 1;
                    continue;
                }
                I2npBody::DatabaseStore(_) => {
                    direct_store += 1;
                    continue;
                }
                I2npBody::DatabaseSearchReply(_) => {
                    direct_search_reply += 1;
                    continue;
                }
                I2npBody::DeliveryStatus(_) => {
                    direct_delivery_status += 1;
                    continue;
                }
                _ => {
                    direct_other += 1;
                    let number = message.body().message_type().code();
                    if let Some(slot) = direct_types.iter_mut().find(|slot| slot.0 == number) {
                        slot.1 = slot.1.saturating_add(1);
                    } else if direct_types.len() < 16 {
                        direct_types.push((number, 1));
                    }
                    continue;
                }
            };
            tunnel_data += 1;
            let outcome = match inbound_dispatch::dispatch_inbound_tunnel_data(
                coordinator.registry_mut(),
                &cell,
                wall_ms(),
            ) {
                Ok(outcome) => outcome,
                Err(_) => continue,
            };
            dispatch_ok += 1;
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
            envelope_ok += 1;
            let now_secs = wall_secs() as u32;
            let outcome = {
                let mut coord_guard = destination_tunnels.lock().await;
                coord_guard.ingest_tunnel_lease_store(lookup_id, &envelope, now_secs)
            };
            if matches!(outcome, Ok(LeaseStoreIngestOutcome::Completed { .. })) {
                ingest_completed += 1;
                resolved = true;
            } else {
                ingest_continued += 1;
            }
        }
        if !resolved && attempts < MAX_LOOKUP_ATTEMPTS {
            tokio::time::sleep(LOOKUP_SETTLE_DELAY).await;
        }
    }
    if !resolved {
        // Bounded attribution for the qualification lane: report
        // the Plan 201 §G counter classes so a timeout
        // distinguishes replies-never-recovered (all zero) from
        // key-mismatch, decode, and signature rejection outcomes.
        // Hashes and ephemeral tunnel ids only; no key material
        // or payload bytes.
        let counters = destination_tunnels.lock().await.counters();
        let mut target_hex = String::with_capacity(64);
        for byte in destination_hash.as_bytes().iter() {
            target_hex.push_str(&format!("{byte:02x}"));
        }
        let registry_ids: Vec<u32> = coordinator
            .registry()
            .inbound_receive_ids()
            .iter()
            .map(|id| id.get())
            .collect();
        let mut gateway_hex = String::with_capacity(16);
        for byte in reply_path.gateway().as_bytes().iter().take(8) {
            gateway_hex.push_str(&format!("{byte:02x}"));
        }
        let encoded_reply_tunnel = reply_path.tunnel_id();
        return Err(ServiceProductError::Provisioning(format!(
            "lease lookup unresolved target={target_hex} attempts={} decoded={} decode_rejected={} signature_rejected={} key_match={} key_mismatch={} mismatched={} malformed={} started={} succeeded={} pump_inbound={} pump_decode={} pump_tunneldata={} pump_dispatch={} pump_envelope={} pump_ingested={} pump_continued={} direct_garlic={} direct_store={} direct_searchreply={} direct_deliverystatus={} direct_other={} direct_types=[{}] reply_tunnel={} encoded_gateway={} encoded_reply_tunnel={} registry_receive_count={} registry_receive_ids=[{}]",
            attempts,
            counters.ls2_records_decoded,
            counters.ls2_records_decode_rejected,
            counters.ls2_records_signature_rejected,
            counters.lookup_key_matches,
            counters.lookup_key_mismatches,
            counters.mismatched_rejected,
            counters.malformed_rejected,
            counters.lookups_started,
            counters.lookups_succeeded,
            inbound_seen,
            decode_ok,
            tunnel_data,
            dispatch_ok,
            envelope_ok,
            ingest_completed,
            ingest_continued,
            direct_garlic,
            direct_store,
            direct_search_reply,
            direct_delivery_status,
            direct_other,
            direct_types
                .iter()
                .map(|(number, count)| format!("{number}:{count}"))
                .collect::<Vec<_>>()
                .join(","),
            local_receive_for_lookup,
            gateway_hex,
            encoded_reply_tunnel,
            registry_ids.len(),
            registry_ids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )));
    }
    // Require the validated cached LS2 for exactly the target hash
    // and install it into that service's router-backed routing.
    let cached = {
        let coord_guard = destination_tunnels.lock().await;
        coord_guard.lease_store().get(&destination_hash).cloned()
    };
    let cached = cached.ok_or(ServiceProductError::LookupTimeout)?;
    let validated = cached.clone();
    let now_ms = wall_ms();
    let now_secs = wall_secs() as u32;
    manager
        .with_destination_bridge(service_destination, |bridge| {
            bridge.install_remote_lease_set2_into_router_state(validated, now_ms)
        })
        .ok_or_else(|| ServiceProductError::Provisioning("service destination missing".to_owned()))?
        .map_err(ServiceProductError::Provisioning)?;
    let _ = now_secs;
    Ok(())
}

/// Plan 212 §7 step 6 — provisions every enabled remote-capable
/// service runtime with real router-backed network state.
///
/// For each service runtime:
/// 1. issue a real outbound build through the shared coordinator;
/// 2. wait for `Installed`; take the real gateway role via
///    `remove_outbound` + `DestinationOutboundRole::from_role`;
/// 3. build a real inbound tunnel; capture installed registry
///    metadata; construct `InboundLeaseSource::from_parts` from
///    installed route metadata (never hard-coded constants);
/// 4. build the signed Standard LS2 via
///    `build_signed_lease_set2` + validate via `ValidatedLeaseSet2`;
/// 5. install router-backed state on that exact service runtime;
/// 6. register real inbound receive ownership
///    (`register_inbound_tunnel_owner` per receive id +
///    `register_inbound_destination_owner`);
/// 7. resolve configured remote target LS2 per service
///    (client profiles);
/// 8. publish local LS2 when server reachability requires it.
///
/// Tunnel ids allocate disjointly per service via
/// [`Plan212TunnelIdAllocator`]. A failed provisioning pass fails
/// atomically (caller tears down staged listeners).
#[allow(clippy::too_many_arguments)]
async fn provision_all_service_router_material(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    peer: &RouterPeerMaterial,
    runtimes: &[Arc<crate::service_tunnels::ServiceRuntime>],
    options: ServiceProductOptions,
    allocator: &mut Plan212TunnelIdAllocator,
) -> Result<(), ServiceProductError> {
    use i2pr_client::{
        DestinationRouting, DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager,
    };
    use i2pr_client::{InboundLeaseSource, build_signed_lease_set2};
    // Collect per-service target hashes first so HTTP and IRC
    // resolve independently; a missing/invalid target fails that
    // service only when the profile requires remote lookup
    // (client profiles). Server profiles skip lookup but still
    // require real outbound/inbound + LS2 + owner registration.
    for runtime in runtimes {
        let spec_id = runtime.spec_id.clone();
        // Allocate a disjoint tunnel-id set for this service.
        // Layout per service: outbound creator, OBEP receive, OBEP
        // next, inbound creator, IBGW receive, IBGW next.
        let ids = allocator.allocate_set(6);
        let (ob_creator, obep_receive, obep_next, ib_creator, ibgw_receive, ibgw_next) =
            (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]);
        // Skip zero (allocator never emits it) and skip ids still
        // present in the registry (bounded retry).
        for id in [
            ob_creator,
            obep_receive,
            obep_next,
            ib_creator,
            ibgw_receive,
            ibgw_next,
        ] {
            if id == 0 {
                return Err(ServiceProductError::Provisioning(format!(
                    "{spec_id}: allocator emitted zero tunnel id"
                )));
            }
        }
        let bridge = ShortBuildI2npBridge::new();
        let mut rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(u64::from(ob_creator)));
        let delivery = ssu2_handle.delivery().clone();
        // Plan 213 — mask the low two bits before OR-ing the
        // direction bit so outbound (`| 0x01`) and inbound
        // (`| 0x02`) message ids stay distinct for every service
        // (later allocator ids already carry low bits).
        let message_base: u32 = (ob_creator ^ 0x51A7_0000) & !0x03;
        let outbound = BuildRequest {
            direction: BuildDirection::Outbound,
            peer: PeerBuildMaterial {
                router_hash: peer.peer_hash,
                static_encryption_key: peer.encryption_key,
                receive_tunnel: TunnelId::new(obep_receive).map_err(|_| {
                    ServiceProductError::Provisioning(format!("{spec_id}: bad OBEP receive id"))
                })?,
                next_tunnel: TunnelId::new(obep_next).map_err(|_| {
                    ServiceProductError::Provisioning(format!("{spec_id}: bad OBEP next id"))
                })?,
                role: HopRole::OutboundEndpoint,
            },
            creator_tunnel_id: TunnelId::new(ob_creator).map_err(|_| {
                ServiceProductError::Provisioning(format!("{spec_id}: bad outbound creator id"))
            })?,
            message_id: message_base | 0x01,
            outbound_reply_router: Some(peer.local_hash),
            originator_hash: None,
        };
        coordinator
            .submit(
                outbound,
                &delivery,
                &bridge,
                BridgeHeader::ShortTransport {
                    message_id: message_base | 0x01,
                    expiration_seconds: wall_secs().saturating_add(60) as u32,
                },
                &mut rng,
            )
            .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
        let inbound = BuildRequest {
            direction: BuildDirection::Inbound,
            peer: PeerBuildMaterial {
                router_hash: peer.peer_hash,
                static_encryption_key: peer.encryption_key,
                receive_tunnel: TunnelId::new(ibgw_receive).map_err(|_| {
                    ServiceProductError::Provisioning(format!("{spec_id}: bad IBGW receive id"))
                })?,
                next_tunnel: TunnelId::new(ibgw_next).map_err(|_| {
                    ServiceProductError::Provisioning(format!("{spec_id}: bad IBGW next id"))
                })?,
                role: HopRole::InboundGateway,
            },
            creator_tunnel_id: TunnelId::new(ib_creator).map_err(|_| {
                ServiceProductError::Provisioning(format!("{spec_id}: bad inbound creator id"))
            })?,
            message_id: message_base | 0x02,
            outbound_reply_router: None,
            originator_hash: Some(peer.local_hash),
        };
        coordinator
            .submit(
                inbound,
                &delivery,
                &bridge,
                BridgeHeader::ShortTransport {
                    message_id: message_base | 0x02,
                    expiration_seconds: wall_secs().saturating_add(60) as u32,
                },
                &mut rng,
            )
            .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
        // Wait for both installs.
        let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
        let mut installed_inbound = false;
        let install_deadline = tokio::time::Instant::now() + options.i2pd_accept_timeout;
        while tokio::time::Instant::now() < install_deadline
            && (outbound_slot.is_none() || !installed_inbound)
        {
            let next =
                tokio::time::timeout(options.poll_interval, ssu2_handle.next_inbound()).await;
            let Ok(Some(inbound_msg)) = next else {
                continue;
            };
            let routed = match coordinator.route_inbound_i2np(&inbound_msg, wall_ms()) {
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
        let outbound_slot = outbound_slot.ok_or(ServiceProductError::OutboundBuildMissing)?;
        if !installed_inbound {
            return Err(ServiceProductError::InboundBuildMissing);
        }
        // Take the real gateway role for this service (never
        // `dummy_outbound_tunnel`, `random_outbound_tunnel`,
        // `LocalZeroHop`, fabric, or fixtures).
        let gateway_role = coordinator
            .registry_mut()
            .remove_outbound(outbound_slot)
            .ok_or_else(|| {
                ServiceProductError::Provisioning(format!("{spec_id}: outbound role missing"))
            })?;
        let now_ms = wall_ms();
        let outbound_role = i2pr_client::DestinationOutboundRole::from_role(
            gateway_role,
            now_ms.saturating_add(10 * 60_000),
        );
        // Capture installed inbound route metadata for this
        // service. The local receive ids for the just-installed
        // inbound tunnel are the registry's newest entries; select
        // the ones matching our IBGW receive/next allocation by
        // consulting `inbound_gateway_route` for each candidate.
        // Fall back to scanning `inbound_receive_ids` for the
        // most recent entry when the exact mapping is ambiguous
        // in the controlled single-peer lane.
        let registry_receive_ids = coordinator.registry().inbound_receive_ids();
        if registry_receive_ids.is_empty() {
            return Err(ServiceProductError::InboundBuildMissing);
        }
        // Prefer the receive id whose gateway route matches our
        // peer; otherwise take the last installed id (controlled
        // lane has one inbound tunnel per service, installed in
        // order).
        let mut chosen_local_receive: Option<TunnelId> = None;
        for candidate in registry_receive_ids.iter().rev() {
            if let Some(route) = coordinator.registry().inbound_gateway_route(*candidate)
                && route.gateway_router == peer.peer_hash
            {
                chosen_local_receive = Some(*candidate);
                break;
            }
        }
        let chosen_local_receive =
            chosen_local_receive.unwrap_or(registry_receive_ids[registry_receive_ids.len() - 1]);
        let route = coordinator
            .registry()
            .inbound_gateway_route(chosen_local_receive)
            .ok_or_else(|| {
                ServiceProductError::Provisioning(format!("{spec_id}: inbound route missing"))
            })?;
        let slot = coordinator
            .registry()
            .inbound_slot(chosen_local_receive)
            .ok_or_else(|| {
                ServiceProductError::Provisioning(format!("{spec_id}: inbound slot missing"))
            })?;
        let now_secs = wall_secs();
        let tunnel_expires = now_secs.saturating_add(600);
        let advertised_expires = now_secs.saturating_add(540);
        let lease_source = InboundLeaseSource::from_parts(
            slot,
            route.gateway_router,
            route.gateway_receive_tunnel.get(),
            tunnel_expires,
            advertised_expires,
        );
        // Build the service Destination's real Standard LS2 from
        // real inbound lease metadata (never fabric leases).
        let (service_identity, service_destination_id) = manager
            .with_destination_bridge(runtime.destination_id, |bridge| {
                (bridge.identity(), bridge.identity_id())
            })
            .ok_or_else(|| {
                ServiceProductError::Provisioning(format!("{spec_id}: bridge missing"))
            })?;
        let published_secs = u32::try_from(now_secs).unwrap_or(u32::MAX);
        let lease_set2 = build_signed_lease_set2(
            &service_identity,
            std::slice::from_ref(&lease_source),
            published_secs,
        )
        .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
        let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
            lease_set2.clone(),
            Some(service_identity.id().as_netdb_key()),
            i2pr_netdb::LeaseSet2ValidationContext::new(published_secs),
        )
        .map_err(|error| ServiceProductError::Provisioning(format!("{error:?}")))?;
        let inbound_receive_ids = vec![chosen_local_receive.get()];
        let material = crate::sam::streams::RouterDestinationNetworkState::new(
            service_destination_id,
            DestinationRouting::new(DestinationRoutingConfig::balanced()),
            EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_role,
            lease_set2,
            validated,
            inbound_receive_ids.clone(),
            now_ms.saturating_add(10 * 60_000),
            tunnel_expires.saturating_mul(1000),
        );
        manager
            .install_service_router_material(service_destination_id, material, now_ms)
            .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
        // Production inbound owner registration lifecycle: every
        // live receive id + the destination owner map to this
        // runtime. Drain/replacement/shutdown remove them (the
        // manager's reconcile/shutdown paths call the matching
        // unregister helpers).
        for receive_id in &inbound_receive_ids {
            manager
                .register_inbound_tunnel_owner(*receive_id, Arc::clone(runtime))
                .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
        }
        let destination_hash_bytes = *service_identity.id().as_hash().as_bytes();
        manager
            .register_inbound_destination_owner(destination_hash_bytes, Arc::clone(runtime))
            .await
            .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
    }
    // Per-service remote target lookup (client profiles) + server
    // LS2 publication. Resolved after ALL services hold real
    // material so multi-service targets coexist. Failures here
    // fail the provisioning pass atomically (caller tears down).
    //
    // Plan 213 §C/§7 — server profiles publish their real local
    // LS2 unconditionally so independent routers can initiate
    // toward them through ordinary LeaseSet lookup (Direction B).
    // A server spec carries no configured destination reference
    // (`spec_reference_for_service` returns `None`), so gating
    // publication on that lookup would skip every real server.
    for runtime in runtimes {
        let spec_id = runtime.spec_id.clone();
        if manager.spec_is_server(&spec_id) {
            publish_service_ls2_for_service(
                manager,
                destination_tunnels,
                ssu2_handle,
                runtime.destination_id,
                options,
            )
            .await
            .map_err(|error| {
                ServiceProductError::Provisioning(format!("{spec_id} publication: {error:?}"))
            })?;
            continue;
        }
        // Look up the spec's configured destination reference from
        // the manager's committed specs.
        let Some(reference) = manager.spec_reference_for_service(&spec_id) else {
            continue;
        };
        let Some(target_hash) = manager.remote_target_hash_for_reference(&reference) else {
            // Local co-owned hash -> stay local, do not enter the
            // remote path.
            continue;
        };
        resolve_remote_destination_for_service(
            manager,
            coordinator,
            destination_tunnels,
            ssu2_handle,
            runtime.destination_id,
            DestinationHash::from_hash(Hash::from_bytes(target_hash)),
            options,
        )
        .await?;
    }
    Ok(())
}

/// Plan 212 §13 — server-side local LS2 publication through the
/// existing publication state machine.
///
/// For each server Destination: use the real signed LS2 from
/// provisioning, invoke the existing
/// `DestinationTunnelCoordinator` publication state machine,
/// compose the DatabaseStore through the service's real outbound
/// tunnel, require ACK/publication success semantics, retain/retry
/// only through existing bounded coordinator policy. No second
/// publication implementation exists in service tunnels.
/// Client-only Destinations never call this path merely to receive
/// replies (their real LS2 rides in Streaming establishment).
///
/// Plan 213 §C — the recorded intent alone never stores bytes at
/// the floodfill, so the dispatch below composes the publication
/// cells through the service's real router-backed outbound role
/// (mirroring `resolve_remote_destination_for_service`) and hands
/// every cell to the shared router delivery service. Transport
/// admission (`Accepted`) is the bounded success signal, matching
/// the proven destination external lane.
async fn publish_service_ls2_for_service(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    service_destination: i2pr_client::DestinationId,
    options: ServiceProductOptions,
) -> Result<(), ServiceProductError> {
    // Fetch the real signed LS2 from the router-backed state
    // through the dedicated bridge accessor that clones the public
    // LS2 only (never secret material, never diagnostics).
    let lease_set2 = manager
        .with_destination_bridge(service_destination, |bridge| {
            bridge.router_ls2_for_publication()
        })
        .flatten();
    let Some(lease_set2) = lease_set2 else {
        return Err(ServiceProductError::RouterState(
            "router-backed LS2 missing for publication".to_owned(),
        ));
    };
    // Derive the store key from the LS2's contained destination
    // (destination-derived, never floodfill).
    let destination_hash =
        lease_set2.header().destination().hash().map_err(|_| {
            ServiceProductError::Provisioning("LS2 destination hash failed".to_owned())
        })?;
    // The publication state machine requires a floodfill peer. The
    // controlled lane provisions the reference as floodfill; pick
    // the first floodfill candidate for the LS2 routing key from
    // the authoritative store. When no floodfill is known, fail
    // closed (no direct transport, no synthetic publication).
    let floodfill = {
        let coord_guard = destination_tunnels.lock().await;
        let target = DestinationHash::from_hash(destination_hash);
        let routing_key = i2pr_netdb::router_hash_from_destination(target);
        coord_guard
            .candidate_hashes(&target, &routing_key)
            .first()
            .copied()
            .ok_or_else(|| {
                ServiceProductError::Provisioning("no floodfill for publication".to_owned())
            })?
    };
    let store_message = i2pr_proto::DatabaseStoreMessage {
        key: destination_hash,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(lease_set2)),
    };
    let request_id = {
        let mut coord_guard = destination_tunnels.lock().await;
        coord_guard
            .begin_ls2_publication(store_message, floodfill)
            .map_err(|_| ServiceProductError::Provisioning("publication begin failed".to_owned()))?
    };
    // Compose the DatabaseStore through the service's real
    // router-backed outbound tunnel (borrow stays inside the bridge
    // closure, mirroring `resolve_remote_destination_for_service`).
    // The dispatch is handed to the shared router delivery service;
    // no second publication implementation exists.
    let floodfill_hash = Hash::from_bytes(*floodfill.as_bytes());
    let mut publication_rng = ChaCha8Rng::seed_from_u64(
        wall_secs()
            .wrapping_add(u64::from(request_id as u32))
            .wrapping_add(0x2130),
    );
    let dispatch = {
        let mut coord_guard = destination_tunnels.lock().await;
        let deadline = Deadline::new(options.tunnel_deadline)
            .map_err(|_| ServiceProductError::Provisioning("publication deadline".to_owned()))?;
        manager
            .with_destination_bridge(service_destination, |bridge| {
                let role = bridge.router_outbound_role_ref()?;
                if !bridge.has_router_network_state(wall_ms()) {
                    return None;
                }
                let (dispatch, _proof) = coord_guard
                    .compose_ls2_publication_via_tunnel(
                        request_id,
                        floodfill_hash,
                        role.role(),
                        0x51A7_9301,
                        wall_ms().saturating_add(60_000),
                        deadline,
                        &mut publication_rng,
                        wall_ms(),
                    )
                    .ok()?;
                Some(dispatch)
            })
            .flatten()
            .ok_or_else(|| {
                ServiceProductError::Provisioning("publication compose failed".to_owned())
            })?
    };
    let delivery = ssu2_handle.delivery().clone();
    for cell_delivery in &dispatch.deliveries {
        let request = crate::router_i2np::RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            options.delivery_timeout,
        )
        .map_err(|_| ServiceProductError::Provisioning("publication delivery".to_owned()))?;
        let outcome = delivery.deliver(request, &CancellationToken::new());
        if !matches!(outcome, crate::router_i2np::RouterDeliveryOutcome::Accepted) {
            return Err(ServiceProductError::Provisioning(
                "publication transport rejected".to_owned(),
            ));
        }
    }
    Ok(())
}
