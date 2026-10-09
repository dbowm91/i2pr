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
//!   -> bounded validated-NetDB candidate projection + DestinationPeerSelector
//!   -> exact-three DestinationBuildRequest through the shared build coordinator
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

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    DestinationHash, LeaseSet2ValidationContext, LookupPolicy, RouterHash, RouterInfoStoreConfig,
    ValidatedLeaseSet2,
};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
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
use tokio::sync::{Mutex, mpsc};

use crate::destination_peers::{DestinationPeerCandidate, select_destination_path_os};
use crate::destination_tunnels::{
    DestinationTunnelCoordinator, LeaseStoreIngestOutcome, reply_path_for_inbound_route,
};
use crate::encrypted_service_resolver::{
    EncryptedServiceConsumerError, EncryptedServiceResolver, bind_inner_to_address,
};
use crate::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, DestinationBuildRequest, ExploratoryBuildCoordinator,
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
use crate::service_tunnels::{
    DeferredActivationRequest, EncryptedTargetStatus, RemoteTargetProjection, ServiceTunnelManager,
    ServiceTunnelManagerConfig,
};

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
/// Exact-three inbound/outbound paths install through the shared
/// `ExploratoryBuildCoordinator` short-build core and never appear in driver code.
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
/// Startup provisioning borrows the normal SSU2 receive queue while waiting
/// for short-build replies. Preserve every consumed message for the regular
/// router/floodfill dispatcher, within explicit message and byte bounds.
const MAX_STARTUP_REPLAY_MESSAGES: usize = 1024;
const MAX_STARTUP_REPLAY_BYTES: usize = 8 * 1024 * 1024;

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
    pub router_bundle: Arc<RouterIdentityBundle>,
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
    /// Plan 381: additional reference peers dialled + bootstrapped
    /// alongside [`ServiceProductSpec::reference`].
    ///
    /// One peer is never enough for the qualified profile: tunnel
    /// builds select exactly three mutually diverse peers
    /// (`destination_peers::QUALIFIED_DESTINATION_HOPS`), and an
    /// external lane with a single reference fails deferred
    /// provisioning with `NoCandidates`. Empty by default; the
    /// primary reference above stays the dial-first peer.
    pub extra_bootstrap_peers: Vec<ReferencePeer>,
    /// Tunable bounds; defaults to [`ServiceProductOptions::default`].
    pub options: ServiceProductOptions,
    /// Canonical address-book resolver cell (Plan 294). Empty by
    /// default; install the active subsystem's shared handle to let
    /// alias misses resolve through the same owner SAM uses.
    pub addressbook: crate::addressbook::SharedAddressBook,
    /// The one shared service manager (Plan 337), when the composition
    /// root has already built it.
    ///
    /// `None` builds a private manager, which is what the controlled
    /// helper and the integration lanes want. Production composition
    /// passes the **same** `Arc` it hands the I2PControl control state,
    /// so exactly one manager owns every service runtime: a
    /// control-created server is delivered and published through the
    /// same path as a startup-configured one, and Plan 289's "one
    /// existing `ServiceTunnelManager`" invariant holds in the source.
    ///
    /// The supplied manager is prepared here and gains the executable
    /// router delivery backend before any control reconcile can run —
    /// the SSU2 service starts ahead of the I2PControl service.
    pub shared_manager: Option<Arc<ServiceTunnelManager>>,
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

#[cfg(test)]
mod normal_daemon_owner_tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_runtime::ChildFailurePolicy;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;
    use tokio::net::TcpListener;

    use crate::router_i2np::Ssu2DaemonService;

    #[tokio::test]
    async fn normal_owner_fails_closed_on_empty_validated_store_and_releases_listener() {
        let directory = tempfile::tempdir().expect("temporary data directory");
        let port_probe = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener port");
        let listener_address = port_probe.local_addr().expect("listener address");
        drop(port_probe);
        let config_text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\n[service_tunnels]\nenabled = true\n[[service_tunnels.tunnel]]\nid = \"empty-store-client\"\nkind = \"generic-client\"\nenabled = true\nlistener = \"{}\"\ndestination = \"{}.b32.i2p\"\n",
            directory.path().to_string_lossy(),
            listener_address,
            "a".repeat(52),
        );
        let config = crate::config::Config::parse(&config_text).expect("valid service config");
        let mut rng = ChaCha8Rng::seed_from_u64(0x318);
        let bundle = Arc::new(RouterIdentityBundle::generate(&mut rng).expect("router identity"));
        let identity =
            crate::router_i2np::generate_controlled_identity(&bundle, "127.0.0.1", 44_001)
                .expect("controlled identity");
        let daemon_service =
            Ssu2DaemonService::new(&config.ssu2, identity).expect("strict controlled SSU2 service");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let handle = daemon_service
            .start(&scope, &config.ssu2)
            .await
            .expect("existing SSU2 owner");
        let endpoint = handle.local_v4().expect("bound loopback endpoint");
        let spec = ServiceProductSpec {
            data_dir: directory.path().to_path_buf(),
            ssu2_bind: endpoint,
            router_bundle: bundle,
            service_tunnels: Arc::new(config.service_tunnels.tunnels.clone()),
            aliases: Arc::new(config.service_tunnels.aliases.clone()),
            aggregate_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_aggregate,
            per_service_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_per_service,
            reference: None,
            extra_bootstrap_peers: Vec::new(),
            options: ServiceProductOptions::default(),
            addressbook: crate::addressbook::AddressBookManager::activate(
                config.addressbook.clone(),
            )
            .shared(),
            shared_manager: None,
        };
        let result = ServiceProduct::start_over_existing_daemon(
            spec,
            handle,
            scope,
            token,
            Vec::new(),
            RouterInfoStoreConfig::new(config.netdb.max_records, config.netdb.max_encoded_bytes),
            crate::service_lifecycle::ServiceLifecycleController::new(),
        )
        .await;
        assert!(matches!(
            result,
            Err(ServiceProductError::DestinationSelection(
                crate::destination_peers::DestinationSelectionError::NoCandidates
            ))
        ));
        let rebound = TcpListener::bind(listener_address)
            .await
            .expect("failed readiness gate must release the staged client listener");
        drop(rebound);
    }

    #[tokio::test]
    async fn delay_open_starts_without_pools_and_activation_fails_closed_without_candidates() {
        let directory = tempfile::tempdir().expect("temporary data directory");
        let port_probe = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener port");
        let listener_address = port_probe.local_addr().expect("listener address");
        drop(port_probe);
        let config_text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\n[service_tunnels]\nenabled = true\n[[service_tunnels.tunnel]]\nid = \"delay-open-client\"\nkind = \"generic-client\"\nenabled = true\nlistener = \"{}\"\ndestination = \"{}.b32.i2p\"\n",
            directory.path().to_string_lossy(),
            listener_address,
            "a".repeat(52),
        );
        let mut config = crate::config::Config::parse(&config_text).expect("valid service config");
        config.service_tunnels.tunnels.tunnels[0]
            .timeouts
            .delay_open = true;
        let mut rng = ChaCha8Rng::seed_from_u64(0x323);
        let bundle = Arc::new(RouterIdentityBundle::generate(&mut rng).expect("router identity"));
        let identity =
            crate::router_i2np::generate_controlled_identity(&bundle, "127.0.0.1", 44_001)
                .expect("controlled identity");
        let daemon_service =
            Ssu2DaemonService::new(&config.ssu2, identity).expect("strict controlled SSU2");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let handle = daemon_service
            .start(&scope, &config.ssu2)
            .await
            .expect("existing SSU2 owner");
        let endpoint = handle.local_v4().expect("bound endpoint");
        let spec = ServiceProductSpec {
            data_dir: directory.path().to_path_buf(),
            ssu2_bind: endpoint,
            router_bundle: bundle,
            service_tunnels: Arc::new(config.service_tunnels.tunnels.clone()),
            aliases: Arc::new(config.service_tunnels.aliases.clone()),
            aggregate_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_aggregate,
            per_service_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_per_service,
            reference: None,
            extra_bootstrap_peers: Vec::new(),
            options: ServiceProductOptions::default(),
            addressbook: crate::addressbook::AddressBookManager::activate(
                config.addressbook.clone(),
            )
            .shared(),
            shared_manager: None,
        };
        let router_infos = Vec::new();
        let mut product = ServiceProduct::start_over_existing_daemon(
            spec,
            handle,
            scope,
            token,
            router_infos,
            RouterInfoStoreConfig::new(config.netdb.max_records, config.netdb.max_encoded_bytes),
            crate::service_lifecycle::ServiceLifecycleController::new(),
        )
        .await
        .expect("deferred groups do not require startup build candidates");
        let runtime = product
            .manager
            .service_runtime_for_spec("delay-open-client")
            .expect("service runtime");
        assert!(
            product
                .inner
                .deferred_destination_ids
                .contains(&runtime.destination_id)
        );
        assert!(
            !product
                .inner
                .destination_ids
                .contains(&runtime.destination_id)
        );

        let manager = Arc::clone(&product.manager);
        let destination_id = runtime.destination_id;
        let cancellation = CancellationToken::new();
        let waiter = tokio::spawn(async move {
            manager
                .ensure_destination_active("delay-open-client", &cancellation, 5_000)
                .await
        });
        tokio::task::yield_now().await;
        product
            .advance_destination_pools()
            .await
            .expect("failed optional activation stays isolated to the requester");
        assert!(waiter.await.expect("activation waiter").is_err());
        assert!(
            product
                .inner
                .deferred_destination_ids
                .contains(&destination_id)
        );
        product
            .shutdown()
            .await
            .expect("product shuts down cleanly");
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
    /// The validated NetDB could not supply one complete qualified path.
    #[error("Destination peer selection failed: {0}")]
    DestinationSelection(#[from] crate::destination_peers::DestinationSelectionError),
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
    destination_ids: Vec<i2pr_client::DestinationId>,
    destination_runtimes: std::collections::HashMap<
        i2pr_client::DestinationId,
        Arc<crate::service_tunnels::ServiceRuntime>,
    >,
    deferred_destination_ids: std::collections::HashSet<i2pr_client::DestinationId>,
    deferred_activation_failures: std::collections::HashMap<i2pr_client::DestinationId, String>,
    deferred_activation_requests: mpsc::Receiver<DeferredActivationRequest>,
    service_generation_id: Option<u64>,
    local_router_hash: Option<Hash>,
    tunnel_id_allocator: Plan212TunnelIdAllocator,
    server_destination_ids: Vec<i2pr_client::DestinationId>,
    publication_pending: std::collections::HashSet<i2pr_client::DestinationId>,
    publication_retry_after: std::collections::HashMap<i2pr_client::DestinationId, u64>,
    publication_attempts: u64,
    publication_accepted: u64,
    publication_failed: u64,
    publication_last_failure_stage: Option<LeasePublicationFailureStage>,
    options: ServiceProductOptions,
    startup_inbound: VecDeque<Ssu2InboundI2np>,
    startup_inbound_bytes: usize,
    lifecycle: crate::service_lifecycle::ServiceLifecycleController,
    retirement: Option<crate::service_lifecycle::RetirementState>,
    lifecycle_clock_origin: tokio::time::Instant,
}

fn queue_startup_inbound(
    queue: &mut VecDeque<Ssu2InboundI2np>,
    queued_bytes: &mut usize,
    inbound: Ssu2InboundI2np,
) -> Result<(), ServiceProductError> {
    let next_bytes = queued_bytes
        .checked_add(inbound.bytes.len())
        .ok_or_else(|| {
            ServiceProductError::Provisioning("startup inbound byte count overflow".to_owned())
        })?;
    if queue.len() >= MAX_STARTUP_REPLAY_MESSAGES || next_bytes > MAX_STARTUP_REPLAY_BYTES {
        return Err(ServiceProductError::Provisioning(
            "startup inbound replay bound exceeded while preserving router dispatch".to_owned(),
        ));
    }
    *queued_bytes = next_bytes;
    queue.push_back(inbound);
    Ok(())
}

fn pop_startup_inbound(
    queue: &mut VecDeque<Ssu2InboundI2np>,
    queued_bytes: &mut usize,
) -> Option<Ssu2InboundI2np> {
    let inbound = queue.pop_front()?;
    *queued_bytes = queued_bytes.saturating_sub(inbound.bytes.len());
    Some(inbound)
}

async fn wait_for_startup_inbound<F>(
    poll_interval: Duration,
    cancellation: &CancellationToken,
    next_inbound: F,
) -> Result<Option<Ssu2InboundI2np>, ServiceProductError>
where
    F: std::future::Future<Output = Option<Ssu2InboundI2np>>,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err(ServiceProductError::Provisioning(
            "Destination-group provisioning cancelled".to_owned(),
        )),
        result = tokio::time::timeout(poll_interval, next_inbound) => {
            Ok(result.unwrap_or_default())
        }
    }
}

#[cfg(test)]
mod startup_inbound_replay_tests {
    use super::{
        MAX_STARTUP_REPLAY_BYTES, pop_startup_inbound, queue_startup_inbound,
        wait_for_startup_inbound,
    };
    use std::collections::VecDeque;
    use std::time::Duration;

    use i2pr_runtime::{CancellationToken, Ssu2InboundI2np};
    use i2pr_transport::{LinkId, PeerId};

    fn inbound(link_id: u64, payload: Vec<u8>) -> Ssu2InboundI2np {
        Ssu2InboundI2np {
            link_id: LinkId::new(link_id).expect("nonzero link id"),
            peer: PeerId::from_bytes([link_id as u8; 32]),
            bytes: payload,
        }
    }

    #[test]
    fn startup_replay_preserves_order_and_fails_closed_at_byte_bound() {
        let mut queue = VecDeque::new();
        let mut queued_bytes = 0;
        queue_startup_inbound(&mut queue, &mut queued_bytes, inbound(1, vec![1, 2]))
            .expect("first message queued");
        queue_startup_inbound(&mut queue, &mut queued_bytes, inbound(2, vec![3]))
            .expect("second message queued");

        assert_eq!(
            pop_startup_inbound(&mut queue, &mut queued_bytes)
                .expect("first replay")
                .bytes,
            [1, 2]
        );
        assert_eq!(queued_bytes, 1);
        assert_eq!(
            pop_startup_inbound(&mut queue, &mut queued_bytes)
                .expect("second replay")
                .bytes,
            [3]
        );
        assert_eq!(queued_bytes, 0, "accounting decreases when replayed");

        let mut bounded = VecDeque::new();
        let mut bounded_bytes = 0;
        let error = queue_startup_inbound(
            &mut bounded,
            &mut bounded_bytes,
            inbound(3, vec![0; MAX_STARTUP_REPLAY_BYTES + 1]),
        )
        .expect_err("oversized startup backlog must fail closed");
        assert!(error.to_string().contains("replay bound exceeded"));
        assert!(bounded.is_empty());
        assert_eq!(bounded_bytes, 0);
    }

    #[tokio::test]
    async fn startup_receive_is_interruptible_by_cancellation() {
        let cancellation = CancellationToken::new();
        cancellation.cancel(i2pr_core::CancellationReason::OperatorRequest);
        let result = wait_for_startup_inbound(
            Duration::from_secs(60),
            &cancellation,
            std::future::pending(),
        )
        .await;
        assert!(
            result
                .expect_err("cancellation aborts startup")
                .to_string()
                .contains("cancelled")
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct ActivatedDestinationBinding {
    pool_slot: i2pr_tunnel::pool::TunnelSlot,
    role_slot: i2pr_tunnel::pool::TunnelSlot,
    expires_at_ms: u64,
}

/// The composed product instance. Holds the production manager plus
/// the supporting router stack. The driver only consumes the public
/// surface; no shadow stack is reachable.
pub struct ServiceProduct {
    manager: Arc<ServiceTunnelManager>,
    inner: ProductInner,
    scope: ChildScope,
    token: CancellationToken,
    readiness: ServiceProductReadiness,
}

/// Typed operational state published by the composed service product.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceProductReadiness {
    /// Local-only qualification product with no router-backed pool request.
    LocalOnly,
    /// Router readiness and every configured Destination group's usable
    /// pool threshold passed before service supervisors were started.
    RouterReadyGroupsUsable { groups: u16 },
    /// Router readiness with one or more intentionally deferred client
    /// groups that activate on first local use.
    RouterReadyWithDeferredGroups {
        ready_groups: u16,
        deferred_groups: u16,
    },
}

/// Coarse local counters for the product-owned LeaseSet publication stage.
/// No destination identifiers, keys, payloads, or tunnel details are exposed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LeasePublicationSnapshot {
    pub attempts: u64,
    pub accepted: u64,
    pub failed: u64,
    pub pending: usize,
    pub last_failure_stage: Option<LeasePublicationFailureStage>,
}

/// Redacted service-Destination pool state used to distinguish a missing
/// publication record from a destination that has not reached lease readiness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestinationProvisioningSnapshot {
    pub inbound_registrations: usize,
    pub usable_inbound_leases: usize,
    pub minimum_usable_inbound: u16,
    pub outbound_registrations: usize,
    pub lease_set_present: bool,
}

fn classify_publication_failure(error: &ServiceProductError) -> LeasePublicationFailureStage {
    if let ServiceProductError::RouterState(detail) = error
        && detail.contains("LS2 missing")
    {
        return LeasePublicationFailureStage::MissingLeaseSet;
    }
    let ServiceProductError::Provisioning(detail) = error else {
        return LeasePublicationFailureStage::Other;
    };
    if detail.contains("LS2 missing") {
        LeasePublicationFailureStage::MissingLeaseSet
    } else if detail.contains("type-5 record construction") {
        LeasePublicationFailureStage::StoreConstruction
    } else if detail.contains("no floodfill") {
        LeasePublicationFailureStage::FloodfillSelection
    } else if detail.contains("publication begin") {
        LeasePublicationFailureStage::PublicationCoordination
    } else if detail.contains("publication compose") {
        LeasePublicationFailureStage::TunnelComposition
    } else if detail.contains("publication transport") || detail.contains("publication delivery") {
        LeasePublicationFailureStage::DeliveryAdmission
    } else {
        LeasePublicationFailureStage::Other
    }
}

/// Bounded, non-secret stage labels for the most recent publication failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeasePublicationFailureStage {
    MissingLeaseSet,
    StoreConstruction,
    FloodfillSelection,
    PublicationCoordination,
    TunnelComposition,
    DeliveryAdmission,
    Other,
}

impl LeasePublicationFailureStage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::MissingLeaseSet => "missing-leaseset",
            Self::StoreConstruction => "store-construction",
            Self::FloodfillSelection => "floodfill-selection",
            Self::PublicationCoordination => "publication-coordination",
            Self::TunnelComposition => "tunnel-composition",
            Self::DeliveryAdmission => "delivery-admission",
            Self::Other => "other",
        }
    }
}

impl ServiceProductReadiness {
    pub(crate) const fn router_ready(self) -> bool {
        matches!(
            self,
            Self::RouterReadyGroupsUsable { groups } if groups > 0
        ) || matches!(
            self,
            Self::RouterReadyWithDeferredGroups {
                ready_groups,
                deferred_groups
            } if ready_groups.saturating_add(deferred_groups) > 0
        )
    }
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
        let router_bootstrap = if let Some(reference) = &spec.reference {
            let material = dial_and_bootstrap_router_only(
                &mut ssu2_handle,
                &destination_tunnels,
                reference,
                local_router_hash,
                spec.options,
            )
            .await?;
            // Plan 381: bootstrap every extra peer into the same
            // authoritative store so deferred provisioning has a
            // full candidate set. Each extra is dialled like the
            // primary; a bad extra fails start the same way a bad
            // primary does rather than provisioning half a mesh.
            for extra in &spec.extra_bootstrap_peers {
                dial_and_bootstrap_router_only(
                    &mut ssu2_handle,
                    &destination_tunnels,
                    extra,
                    local_router_hash,
                    spec.options,
                )
                .await?;
            }
            Some(material)
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
        // supervisors yet. Plan 337: production composition injects the
        // one shared manager so the control plane reconciles onto this
        // instance; the private build stays for the controlled helper.
        let manager = match &spec.shared_manager {
            Some(shared) => Arc::clone(shared),
            None => {
                let manager_config = ServiceTunnelManagerConfig {
                    data_dir: spec.data_dir.clone(),
                    aggregate_connection_ceiling: spec.aggregate_connection_ceiling,
                    per_service_connection_ceiling: spec.per_service_connection_ceiling,
                    specs: Arc::clone(&spec.service_tunnels),
                    aliases: Arc::clone(&spec.aliases),
                };
                Arc::new(
                    ServiceTunnelManager::new(manager_config)
                        .map_err(|error| ServiceProductError::ManagerBuild(error.to_string()))?,
                )
            }
        };
        manager.install_router_delivery(capability);
        manager.set_addressbook_handle(spec.addressbook.clone());
        let runtimes = manager
            .prepare()
            .await
            .map_err(|error| ServiceProductError::ManagerBuild(error.to_string()))?;
        let deferred_destination_ids = if router_bootstrap.is_some() {
            manager.deferred_destination_ids()
        } else {
            std::collections::HashSet::new()
        };
        let deferred_activation_requests =
            manager.take_deferred_activation_requests().ok_or_else(|| {
                ServiceProductError::ManagerBuild(
                    "deferred activation receiver was already taken".to_owned(),
                )
            })?;
        let mut destination_ids = Vec::new();
        let mut destination_runtimes = std::collections::HashMap::new();
        let mut server_destination_ids = Vec::new();
        if router_bootstrap.is_some() {
            for runtime in &runtimes {
                if manager.spec_is_server(&runtime.spec_id)
                    && !server_destination_ids.contains(&runtime.destination_id)
                {
                    server_destination_ids.push(runtime.destination_id);
                }
                destination_runtimes
                    .entry(runtime.destination_id)
                    .or_insert_with(|| Arc::clone(runtime));
                if !deferred_destination_ids.contains(&runtime.destination_id)
                    && !destination_ids.contains(&runtime.destination_id)
                {
                    destination_ids.push(runtime.destination_id);
                }
            }
        }

        // Plan 212 §7 step 6 — per-service real network
        // provisioning before any supervisor accepts application
        // traffic. When no reference peer is configured (local
        // product) this is a no-op and supervisors start
        // immediately.
        let mut tunnel_id_allocator = Plan212TunnelIdAllocator::new(0x51A7_9300);
        let mut startup_inbound = VecDeque::new();
        let mut startup_inbound_bytes = 0;
        let readiness = if router_bootstrap.is_some() && !deferred_destination_ids.is_empty() {
            ServiceProductReadiness::RouterReadyWithDeferredGroups {
                ready_groups: u16::try_from(destination_ids.len()).unwrap_or(u16::MAX),
                deferred_groups: u16::try_from(deferred_destination_ids.len()).unwrap_or(u16::MAX),
            }
        } else if router_bootstrap.is_some() {
            ServiceProductReadiness::RouterReadyGroupsUsable {
                groups: u16::try_from(destination_ids.len()).unwrap_or(u16::MAX),
            }
        } else {
            ServiceProductReadiness::LocalOnly
        };
        let local_router_hash = router_bootstrap.map(|bootstrap| bootstrap.local_hash);
        let provisionable_runtimes = runtimes
            .iter()
            .filter(|runtime| !deferred_destination_ids.contains(&runtime.destination_id))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(peer) = router_bootstrap
            && let Err(error) = provision_all_service_router_material(
                &manager,
                &mut coordinator,
                &destination_tunnels,
                &mut ssu2_handle,
                &peer,
                &provisionable_runtimes,
                spec.options,
                &token,
                &mut tunnel_id_allocator,
                &mut startup_inbound,
                &mut startup_inbound_bytes,
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

        if router_bootstrap.is_some() {
            manager.enable_deferred_activation();
        }

        manager
            .start_supervisors(runtimes, &scope, token.clone())
            .map_err(|error| ServiceProductError::ManagerBuild(error.to_string()))?;
        let service_generation_id = manager.committed_generation_id();

        Ok(Self {
            manager,
            inner: ProductInner {
                coordinator,
                ssu2_handle,
                destination_tunnels,
                destination_ids,
                destination_runtimes,
                deferred_destination_ids,
                deferred_activation_failures: std::collections::HashMap::new(),
                deferred_activation_requests,
                service_generation_id,
                local_router_hash,
                tunnel_id_allocator,
                server_destination_ids,
                publication_pending: std::collections::HashSet::new(),
                publication_retry_after: std::collections::HashMap::new(),
                publication_attempts: 0,
                publication_accepted: 0,
                publication_failed: 0,
                publication_last_failure_stage: None,
                options: spec.options,
                startup_inbound,
                startup_inbound_bytes,
                lifecycle: {
                    let lifecycle = crate::service_lifecycle::ServiceLifecycleController::new();
                    lifecycle.set_status(crate::service_lifecycle::LifecycleStatus::Active);
                    lifecycle
                },
                retirement: None,
                lifecycle_clock_origin: tokio::time::Instant::now(),
            },
            scope,
            token,
            readiness,
        })
    }

    /// Composes Plan 315 groups over the normal daemon's already-running
    /// SSU2 owner. Unlike the qualification constructor, this accepts the
    /// caller-owned handle, runtime child scope, cancellation token, and a
    /// bounded snapshot of validated bootstrap records; it never binds a
    /// second socket or creates a test scope.
    pub(crate) async fn start_over_existing_daemon(
        spec: ServiceProductSpec,
        mut ssu2_handle: Ssu2DaemonHandle,
        scope: ChildScope,
        token: CancellationToken,
        router_infos: Vec<i2pr_netdb::ValidatedRouterInfo>,
        store_config: RouterInfoStoreConfig,
        lifecycle: crate::service_lifecycle::ServiceLifecycleController,
    ) -> Result<Self, ServiceProductError> {
        if !spec.ssu2_bind.ip().is_loopback() || spec.ssu2_bind.port() == 0 {
            token.cancel(i2pr_core::CancellationReason::OperatorRequest);
            ssu2_handle.shutdown();
            let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
            return Err(ServiceProductError::ControlledProfile(
                "existing SSU2 owner must have a nonzero loopback endpoint".to_owned(),
            ));
        }
        let local_router_hash = match spec.router_bundle.identity().hash() {
            Ok(hash) => hash,
            Err(error) => {
                token.cancel(i2pr_core::CancellationReason::OperatorRequest);
                ssu2_handle.shutdown();
                let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
                return Err(ServiceProductError::InvalidIdentity(format!(
                    "router hash: {error:?}"
                )));
            }
        };
        let destination_tunnels = Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
            LookupPolicy::default(),
            store_config,
        )));
        let mut seed_failed = false;
        {
            let mut guard = destination_tunnels.lock().await;
            for record in router_infos {
                match guard.insert_validated_router_info(record) {
                    i2pr_netdb::InsertOutcome::Inserted
                    | i2pr_netdb::InsertOutcome::Replaced
                    | i2pr_netdb::InsertOutcome::Idempotent => {}
                    i2pr_netdb::InsertOutcome::Conflict
                    | i2pr_netdb::InsertOutcome::StaleReplacement
                    | i2pr_netdb::InsertOutcome::CapacityExceeded => {
                        seed_failed = true;
                        break;
                    }
                }
            }
        }
        if seed_failed {
            token.cancel(i2pr_core::CancellationReason::OperatorRequest);
            ssu2_handle.shutdown();
            let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
            return Err(ServiceProductError::Provisioning(
                "validated bootstrap snapshot could not seed the bounded group store".to_owned(),
            ));
        }
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.advance_time(wall_ms());
        let backend = Arc::new(RemoteDestinationBackend::new(
            Arc::clone(&destination_tunnels),
            ssu2_handle.delivery().clone(),
        ));
        let capability = ServiceDestinationDelivery::with_backend(backend);
        let manager_config = ServiceTunnelManagerConfig {
            data_dir: spec.data_dir.clone(),
            aggregate_connection_ceiling: spec.aggregate_connection_ceiling,
            per_service_connection_ceiling: spec.per_service_connection_ceiling,
            specs: Arc::clone(&spec.service_tunnels),
            aliases: Arc::clone(&spec.aliases),
        };
        let manager = match &spec.shared_manager {
            // Plan 337: the one shared manager, built by the composition
            // root and also handed to the I2PControl control state.
            Some(shared) => Arc::clone(shared),
            None => match ServiceTunnelManager::new(manager_config) {
                Ok(manager) => Arc::new(manager),
                Err(error) => {
                    token.cancel(i2pr_core::CancellationReason::OperatorRequest);
                    ssu2_handle.shutdown();
                    let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
                    return Err(ServiceProductError::ManagerBuild(error.to_string()));
                }
            },
        };
        manager.install_router_delivery(capability);
        manager.set_addressbook_handle(spec.addressbook.clone());
        let runtimes = match manager.prepare().await {
            Ok(runtimes) => runtimes,
            Err(error) => {
                manager.shutdown().await;
                token.cancel(i2pr_core::CancellationReason::OperatorRequest);
                ssu2_handle.shutdown();
                let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
                return Err(ServiceProductError::ManagerBuild(error.to_string()));
            }
        };
        let deferred_destination_ids = manager.deferred_destination_ids();
        let deferred_activation_requests =
            manager.take_deferred_activation_requests().ok_or_else(|| {
                ServiceProductError::ManagerBuild(
                    "deferred activation receiver was already taken".to_owned(),
                )
            })?;
        let mut destination_ids = Vec::new();
        let mut destination_runtimes = std::collections::HashMap::new();
        let mut server_destination_ids = Vec::new();
        for runtime in &runtimes {
            if manager.spec_is_server(&runtime.spec_id)
                && !server_destination_ids.contains(&runtime.destination_id)
            {
                server_destination_ids.push(runtime.destination_id);
            }
            destination_runtimes
                .entry(runtime.destination_id)
                .or_insert_with(|| Arc::clone(runtime));
            if !deferred_destination_ids.contains(&runtime.destination_id)
                && !destination_ids.contains(&runtime.destination_id)
            {
                destination_ids.push(runtime.destination_id);
            }
        }
        let mut tunnel_id_allocator = Plan212TunnelIdAllocator::new(0x51A7_9300);
        let mut startup_inbound = VecDeque::new();
        let mut startup_inbound_bytes = 0;
        let peer = RouterBootstrapMaterial {
            local_hash: local_router_hash,
        };
        let provisionable_runtimes = runtimes
            .iter()
            .filter(|runtime| !deferred_destination_ids.contains(&runtime.destination_id))
            .cloned()
            .collect::<Vec<_>>();
        if let Err(error) = provision_all_service_router_material(
            &manager,
            &mut coordinator,
            &destination_tunnels,
            &mut ssu2_handle,
            &peer,
            &provisionable_runtimes,
            spec.options,
            &token,
            &mut tunnel_id_allocator,
            &mut startup_inbound,
            &mut startup_inbound_bytes,
        )
        .await
        {
            manager.shutdown().await;
            token.cancel(i2pr_core::CancellationReason::OperatorRequest);
            ssu2_handle.shutdown();
            let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
            return Err(error);
        }
        manager.enable_deferred_activation();
        if destination_ids.is_empty() && deferred_destination_ids.is_empty() {
            manager.shutdown().await;
            token.cancel(i2pr_core::CancellationReason::OperatorRequest);
            ssu2_handle.shutdown();
            let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
            return Err(ServiceProductError::Provisioning(
                "enabled service configuration produced no Destination groups".to_owned(),
            ));
        }
        let readiness = if deferred_destination_ids.is_empty() {
            ServiceProductReadiness::RouterReadyGroupsUsable {
                groups: u16::try_from(destination_ids.len()).unwrap_or(u16::MAX),
            }
        } else {
            ServiceProductReadiness::RouterReadyWithDeferredGroups {
                ready_groups: u16::try_from(destination_ids.len()).unwrap_or(u16::MAX),
                deferred_groups: u16::try_from(deferred_destination_ids.len()).unwrap_or(u16::MAX),
            }
        };
        if let Err(error) = manager.start_supervisors(runtimes, &scope, token.clone()) {
            manager.shutdown().await;
            token.cancel(i2pr_core::CancellationReason::OperatorRequest);
            ssu2_handle.shutdown();
            let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
            return Err(ServiceProductError::ManagerBuild(error.to_string()));
        }
        lifecycle.set_status(crate::service_lifecycle::LifecycleStatus::Active);
        let service_generation_id = manager.committed_generation_id();
        Ok(Self {
            manager,
            inner: ProductInner {
                coordinator,
                ssu2_handle,
                destination_tunnels,
                destination_ids,
                destination_runtimes,
                deferred_destination_ids,
                deferred_activation_failures: std::collections::HashMap::new(),
                deferred_activation_requests,
                service_generation_id,
                local_router_hash: Some(local_router_hash),
                tunnel_id_allocator,
                server_destination_ids,
                publication_pending: std::collections::HashSet::new(),
                publication_retry_after: std::collections::HashMap::new(),
                publication_attempts: 0,
                publication_accepted: 0,
                publication_failed: 0,
                publication_last_failure_stage: None,
                options: spec.options,
                startup_inbound,
                startup_inbound_bytes,
                lifecycle: lifecycle.clone(),
                retirement: None,
                lifecycle_clock_origin: tokio::time::Instant::now(),
            },
            scope,
            token,
            readiness,
        })
    }

    /// Returns the bound loopback HTTP client listener port for the
    /// supplied spec id, if any.
    pub fn http_listener_port(&self, id: &str) -> Option<u16> {
        self.manager
            .client_listener_address(id)
            .map(|addr| addr.port())
    }

    /// Returns the typed readiness state published after construction.
    pub(crate) const fn readiness(&self) -> ServiceProductReadiness {
        self.readiness
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

    /// Returns the retained diagnostic for a failed deferred activation.
    /// The value is bounded by the single failure entry per destination and
    /// contains the typed provisioning stage, never network payload bytes.
    pub fn deferred_activation_failure(&self, id: &str) -> Option<&str> {
        let destination_id = self.manager.service_runtime_for_spec(id)?.destination_id;
        self.inner
            .deferred_activation_failures
            .get(&destination_id)
            .map(String::as_str)
    }

    /// Reports whether a service's destination group still awaits its first
    /// router-backed activation. This is a bounded status read used by the
    /// live qualification driver to distinguish an activation miss from a
    /// Streaming failure after successful activation.
    pub fn destination_activation_pending(&self, id: &str) -> Option<bool> {
        let runtime = self.manager.service_runtime_for_spec(id)?;
        Some(
            self.inner
                .deferred_destination_ids
                .contains(&runtime.destination_id),
        )
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

    /// Secret-free counters for the local LeaseSet publication handoff.
    /// `accepted` means all outbound cells were admitted by the router
    /// delivery service; it does not claim floodfill storage or retrieval.
    pub fn lease_publication_snapshot(&self) -> LeasePublicationSnapshot {
        LeasePublicationSnapshot {
            attempts: self.inner.publication_attempts,
            accepted: self.inner.publication_accepted,
            failed: self.inner.publication_failed,
            pending: self.inner.publication_pending.len(),
            last_failure_stage: self.inner.publication_last_failure_stage,
        }
    }

    /// Returns bounded, secret-free pool readiness counts for one service.
    pub fn destination_provisioning_snapshot(
        &self,
        spec_id: &str,
    ) -> Option<DestinationProvisioningSnapshot> {
        let destination_id = self.manager.service_destination_id(spec_id)?;
        self.manager
            .with_destination_runtime(destination_id, |runtime| DestinationProvisioningSnapshot {
                inbound_registrations: runtime.inbound_registrations().len(),
                usable_inbound_leases: runtime.inbound_lease_sources(wall_secs()).len(),
                minimum_usable_inbound: runtime.config().minimum_usable_inbound(),
                outbound_registrations: runtime.outbound_registrations().len(),
                lease_set_present: runtime.lease_set().is_some(),
            })
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
        let poll_interval = self.inner.options.poll_interval;
        let inbound = match tokio::time::timeout(poll_interval, self.next_inbound()).await {
            Err(_) => {
                self.advance_destination_pools().await?;
                return Ok(InboundPollOutcome::Processed);
            }
            Ok(inbound) => inbound,
        };
        let inbound = match inbound {
            Some(inbound) => inbound,
            None => return Ok(InboundPollOutcome::Shutdown),
        };
        self.process_inbound(&inbound).await;
        self.advance_destination_pools().await?;
        Ok(InboundPollOutcome::Processed)
    }

    /// Advances every group pool on the product's deterministic wall clock,
    /// removes expired data-plane roles and inbound owners, then refreshes the
    /// group LeaseSet from the remaining usable pool entries.
    pub(crate) async fn advance_destination_pools(&mut self) -> Result<(), ServiceProductError> {
        self.advance_destination_pools_at(wall_ms()).await
    }

    async fn process_deferred_activation_request(&mut self) {
        let request = match self.inner.deferred_activation_requests.try_recv() {
            Ok(request) => request,
            Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                return;
            }
        };
        if request.completion.is_closed() {
            return;
        }
        let result = self
            .activate_deferred_destination(request.destination_id)
            .await;
        let response = result.map_err(|_| "deferred destination provisioning failed".to_owned());
        let _ = request.completion.send(response);
    }

    async fn activate_deferred_destination(
        &mut self,
        destination_id: i2pr_client::DestinationId,
    ) -> Result<(), ServiceProductError> {
        if !self
            .inner
            .deferred_destination_ids
            .contains(&destination_id)
        {
            return self
                .inner
                .deferred_activation_failures
                .get(&destination_id)
                .map_or(Ok(()), |failure| {
                    Err(ServiceProductError::Provisioning(failure.clone()))
                });
        }
        if let Some(failure) = self.inner.deferred_activation_failures.get(&destination_id) {
            return Err(ServiceProductError::Provisioning(failure.clone()));
        }
        let runtime = self
            .inner
            .destination_runtimes
            .get(&destination_id)
            .cloned()
            .ok_or_else(|| {
                ServiceProductError::Provisioning(
                    "deferred destination runtime is unavailable".to_owned(),
                )
            })?;
        let peer = RouterBootstrapMaterial {
            local_hash: self.inner.local_router_hash.ok_or_else(|| {
                ServiceProductError::Provisioning(
                    "deferred destination requires a router-backed product".to_owned(),
                )
            })?,
        };
        let result = provision_all_service_router_material(
            &self.manager,
            &mut self.inner.coordinator,
            &self.inner.destination_tunnels,
            &mut self.inner.ssu2_handle,
            &peer,
            std::slice::from_ref(&runtime),
            self.inner.options,
            &self.token,
            &mut self.inner.tunnel_id_allocator,
            &mut self.inner.startup_inbound,
            &mut self.inner.startup_inbound_bytes,
        )
        .await;
        match result {
            Ok(()) => {
                self.inner.deferred_destination_ids.remove(&destination_id);
                self.inner.destination_ids.push(destination_id);
                Ok(())
            }
            Err(error) => {
                let reason = format!("deferred destination provisioning failed: {error}");
                let _ = self
                    .inner
                    .coordinator
                    .cancel_destination_builds(destination_id);
                self.inner
                    .deferred_activation_failures
                    .insert(destination_id, reason);
                Err(error)
            }
        }
    }

    /// Receives the next message through the one SSU2 inbound owner.
    pub(crate) async fn next_inbound(&mut self) -> Option<Ssu2InboundI2np> {
        if let Some(inbound) = pop_startup_inbound(
            &mut self.inner.startup_inbound,
            &mut self.inner.startup_inbound_bytes,
        ) {
            return Some(inbound);
        }
        self.inner.ssu2_handle.next_inbound().await
    }

    /// Borrows the already-running SSU2 owner for existing daemon
    /// dispatch services such as floodfill evaluation.
    pub(crate) const fn ssu2_handle(&self) -> &Ssu2DaemonHandle {
        &self.inner.ssu2_handle
    }

    /// Deterministic pool-advance seam used by manual-clock tests and by the
    /// production poll wrapper above.
    async fn advance_destination_pools_at(
        &mut self,
        now_ms: u64,
    ) -> Result<(), ServiceProductError> {
        let now_seconds = now_ms / 1000;
        let monotonic_ms = self
            .inner
            .lifecycle_clock_origin
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        if self.inner.lifecycle.command() == crate::service_lifecycle::LifecycleCommand::Hard {
            if let Some(retirement) = self.inner.retirement {
                let hard_stopped = retirement.hard_stop();
                self.inner
                    .lifecycle
                    .set_status(hard_stopped.status(monotonic_ms));
            }
            self.inner.lifecycle.request_hard();
            return Ok(());
        }
        if self.inner.lifecycle.command() == crate::service_lifecycle::LifecycleCommand::Graceful
            && self.inner.retirement.is_none()
        {
            self.manager.stop_admission();
            for destination_id in self.inner.destination_ids.iter().copied() {
                let _ = self
                    .inner
                    .coordinator
                    .cancel_destination_builds(destination_id);
            }
            let published_lease_expiry_wall_ms = self
                .inner
                .destination_ids
                .iter()
                .filter_map(|destination_id| {
                    self.manager
                        .with_destination_runtime(*destination_id, |destination| {
                            destination
                                .inbound_lease_sources(now_seconds)
                                .iter()
                                .map(|source| source.tunnel_expires_seconds())
                                .max()
                        })
                        .flatten()
                })
                .max()
                .map(|expires_seconds| expires_seconds.saturating_mul(1000));
            let published_lease_expiry_ms = published_lease_expiry_wall_ms
                .map(|expiry_ms| monotonic_ms.saturating_add(expiry_ms.saturating_sub(now_ms)));
            self.inner.retirement = Some(crate::service_lifecycle::RetirementState::begin(
                monotonic_ms,
                published_lease_expiry_ms,
                self.manager.aggregate_active_connections(),
            ));
            self.inner.publication_pending.clear();
            self.inner.publication_retry_after.clear();
        }
        if let Some(retirement) = self.inner.retirement {
            let retirement =
                retirement.advance(monotonic_ms, self.manager.aggregate_active_connections());
            self.inner.retirement = Some(retirement);
            self.inner
                .lifecycle
                .set_status(retirement.status(monotonic_ms));
            if matches!(
                retirement,
                crate::service_lifecycle::RetirementState::Drained
                    | crate::service_lifecycle::RetirementState::HardCapReached
                    | crate::service_lifecycle::RetirementState::HardStopped
            ) {
                return Ok(());
            }
        }
        let retiring = self.inner.retirement.is_some();
        let current_generation = self.manager.committed_generation_id();
        if current_generation != self.inner.service_generation_id {
            let previously_known_servers: std::collections::HashSet<_> =
                self.inner.server_destination_ids.iter().copied().collect();
            for destination_id in self.inner.destination_ids.iter().copied() {
                let _ = self
                    .inner
                    .coordinator
                    .cancel_destination_builds(destination_id);
            }
            self.inner.destination_ids.clear();
            self.inner.destination_runtimes.clear();
            self.inner.server_destination_ids.clear();
            self.inner.deferred_destination_ids = self.manager.deferred_destination_ids();
            self.inner.deferred_activation_failures.clear();
            for runtime in self.manager.destination_group_runtimes() {
                let destination_id = runtime.destination_id;
                self.inner
                    .destination_runtimes
                    .insert(destination_id, Arc::clone(&runtime));
                if !self
                    .inner
                    .deferred_destination_ids
                    .contains(&destination_id)
                {
                    self.inner.destination_ids.push(destination_id);
                }
                if self.manager.destination_group_has_server(destination_id) {
                    self.inner.server_destination_ids.push(destination_id);
                }
            }
            self.inner.publication_pending.clear();
            self.inner.publication_retry_after.clear();
            // A committed control generation can add a server destination
            // whose LeaseSet is already installed by the time this driver
            // observes the generation change. In that case the ordinary
            // lease-change detector below sees no delta and would never
            // publish the new service. Schedule each server once on every
            // generation transition; publication remains bounded by the
            // existing retry deadline and is removed after an accepted
            // delivery.
            self.inner.publication_pending.extend(
                self.inner
                    .server_destination_ids
                    .iter()
                    .copied()
                    .filter(|destination_id| !previously_known_servers.contains(destination_id)),
            );
            self.inner.service_generation_id = current_generation;
        }
        self.process_deferred_activation_request().await;
        self.inner.coordinator.advance_time(now_ms);
        for outcome in self.inner.coordinator.expire_pending() {
            if let BuildCoordinatorOutcome::DestinationBuildFailed { destination_id, .. } = outcome
            {
                let _ = self
                    .manager
                    .with_destination_runtime(destination_id, |runtime| {
                        runtime.note_build_failure()
                    });
            }
        }
        let destination_ids = self.inner.destination_ids.clone();
        for destination_id in destination_ids {
            let Some(snapshot) =
                self.manager
                    .with_destination_runtime(destination_id, |destination| {
                        let inbound_registrations = destination.inbound_registrations();
                        let outbound_registrations = destination.outbound_registrations();
                        let progress = destination.advance_time(now_seconds);
                        let lease_sources = destination.inbound_lease_sources(now_seconds);
                        let minimum = destination.config().minimum_usable_inbound();
                        let lease_set = destination
                            .lease_set()
                            .map(|current| current.lease_set2().clone());
                        (
                            inbound_registrations,
                            outbound_registrations,
                            progress,
                            lease_sources,
                            minimum,
                            lease_set,
                        )
                    })
            else {
                let _ = self
                    .inner
                    .coordinator
                    .cancel_destination_builds(destination_id);
                self.inner.destination_runtimes.remove(&destination_id);
                self.inner
                    .destination_ids
                    .retain(|current| *current != destination_id);
                self.inner.publication_pending.remove(&destination_id);
                self.inner.publication_retry_after.remove(&destination_id);
                continue;
            };
            let (
                inbound_registrations,
                outbound_registrations,
                progress,
                sources,
                minimum,
                current_lease_set,
            ) = snapshot;
            let progress = progress.map_err(|error| {
                ServiceProductError::Provisioning(format!("group pool advance: {error}"))
            })?;
            let evicted_slots: std::collections::HashSet<_> =
                progress.evicted_slots.iter().copied().collect();
            for slot in evicted_slots.iter().copied() {
                if let Some(registration) = inbound_registrations
                    .iter()
                    .find(|registration| registration.slot() == slot)
                {
                    let receive_id = registration.tunnel_id();
                    let _ = self
                        .inner
                        .coordinator
                        .registry_mut()
                        .remove_inbound(receive_id);
                    let _ = self
                        .manager
                        .unregister_inbound_tunnel_owner(receive_id.get());
                    let _ = self
                        .manager
                        .with_destination_bridge(destination_id, |bridge| {
                            bridge.remove_router_inbound_receive(receive_id.get())
                        });
                } else if outbound_registrations
                    .iter()
                    .any(|registration| registration.slot() == slot)
                {
                    let _ = self
                        .manager
                        .with_destination_bridge(destination_id, |bridge| {
                            bridge.remove_router_outbound_pool_slot(slot)
                        });
                }
            }
            if retiring {
                // Existing published leases and data-plane registrations age
                // out naturally. Do not refresh publication or replace pool
                // material after the group has entered Retiring.
                continue;
            }
            let inbound_receive_ids: Vec<u32> = inbound_registrations
                .iter()
                .filter(|registration| !evicted_slots.contains(&registration.slot()))
                .map(|registration| registration.tunnel_id().get())
                .collect();
            let current_bridge = self
                .manager
                .with_destination_bridge(destination_id, |bridge| {
                    (
                        bridge.router_ls2_for_publication(),
                        bridge.router_inbound_receive_ids(),
                    )
                })
                .ok_or_else(|| {
                    ServiceProductError::Provisioning(
                        "group bridge missing during refresh".to_owned(),
                    )
                })?;
            let current_lease_set = if sources.len() >= usize::from(minimum) {
                current_lease_set
            } else {
                None
            };
            let lease_changed = current_bridge.0 != current_lease_set;
            let owners_changed = current_bridge.1 != inbound_receive_ids;
            if !lease_changed && !owners_changed {
                self.replenish_group_destination(destination_id).await?;
                continue;
            }
            if lease_changed && self.inner.server_destination_ids.contains(&destination_id) {
                if current_lease_set.is_some() {
                    self.inner.publication_pending.insert(destination_id);
                } else {
                    self.inner.publication_pending.remove(&destination_id);
                    self.inner.publication_retry_after.remove(&destination_id);
                }
            }
            let lease_snapshot = current_lease_set
                .map(|lease_set| {
                    let destination_hash =
                        lease_set.header().destination().hash().map_err(|_| {
                            ServiceProductError::Provisioning(
                                "refreshed LeaseSet destination hash failed".to_owned(),
                            )
                        })?;
                    let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
                        lease_set.clone(),
                        Some(DestinationHash::from_hash(destination_hash)),
                        i2pr_netdb::LeaseSet2ValidationContext::new(
                            u32::try_from(now_seconds).unwrap_or(u32::MAX),
                        ),
                    )
                    .map_err(|error| ServiceProductError::Provisioning(format!("{error:?}")))?;
                    Ok::<_, ServiceProductError>((lease_set, validated))
                })
                .transpose()?;
            let inbound_expires_at_ms = sources
                .iter()
                .map(i2pr_client::InboundLeaseSource::tunnel_expires_seconds)
                .max()
                .unwrap_or(now_seconds)
                .saturating_mul(1000);
            self.manager
                .with_destination_bridge(destination_id, |bridge| {
                    bridge.refresh_router_pool_snapshot(
                        lease_snapshot,
                        inbound_receive_ids,
                        inbound_expires_at_ms,
                        now_ms,
                    )
                })
                .ok_or_else(|| {
                    ServiceProductError::Provisioning(
                        "group bridge missing during refresh".to_owned(),
                    )
                })?
                .map_err(ServiceProductError::Provisioning)?;
            self.replenish_group_destination(destination_id).await?;
        }
        let publication_ids: Vec<_> = self
            .inner
            .publication_pending
            .iter()
            .copied()
            .filter(|destination_id| {
                self.inner
                    .publication_retry_after
                    .get(destination_id)
                    .is_none_or(|retry_after| *retry_after <= now_ms)
            })
            .collect();
        for destination_id in publication_ids {
            self.inner.publication_attempts = self.inner.publication_attempts.saturating_add(1);
            let result = publish_service_ls2_for_service(
                &self.manager,
                &self.inner.destination_tunnels,
                &mut self.inner.ssu2_handle,
                destination_id,
                self.inner.options,
            )
            .await;
            match result {
                Ok(()) => {
                    self.inner.publication_accepted =
                        self.inner.publication_accepted.saturating_add(1);
                    self.inner.publication_last_failure_stage = None;
                    self.inner.publication_pending.remove(&destination_id);
                    self.inner.publication_retry_after.remove(&destination_id);
                }
                Err(error) => {
                    self.inner.publication_failed = self.inner.publication_failed.saturating_add(1);
                    self.inner.publication_last_failure_stage =
                        Some(classify_publication_failure(&error));
                    self.inner
                        .publication_retry_after
                        .insert(destination_id, now_ms.saturating_add(5_000));
                }
            }
        }
        Ok(())
    }

    /// Fills current target deficits through the same bounded Plan 314
    /// selector and build coordinator. One driver owns this map, so pending
    /// attempts coalesce naturally by Destination and direction.
    async fn replenish_group_destination(
        &mut self,
        destination_id: i2pr_client::DestinationId,
    ) -> Result<(), ServiceProductError> {
        let Some(local_router_hash) = self.inner.local_router_hash else {
            return Ok(());
        };
        let Some(runtime) = self
            .inner
            .destination_runtimes
            .get(&destination_id)
            .cloned()
        else {
            let _ = self
                .inner
                .coordinator
                .cancel_destination_builds(destination_id);
            return Ok(());
        };
        let (inbound_deficit, outbound_deficit, concurrency, paused) = self
            .manager
            .with_destination_runtime(destination_id, |destination| {
                let config = destination.config();
                (
                    usize::from(config.inbound_target())
                        .saturating_sub(destination.inbound_registrations().len())
                        .saturating_sub(self.inner.coordinator.pending_destination_direction_len(
                            destination_id,
                            BuildDirection::Inbound,
                        )),
                    usize::from(config.outbound_target())
                        .saturating_sub(destination.outbound_registrations().len())
                        .saturating_sub(self.inner.coordinator.pending_destination_direction_len(
                            destination_id,
                            BuildDirection::Outbound,
                        )),
                    usize::from(config.build_concurrency()),
                    destination.pool().replacement_paused(),
                )
            })
            .ok_or_else(|| {
                ServiceProductError::Provisioning("group runtime missing for replenish".to_owned())
            })?;
        if paused {
            return Ok(());
        }
        let pending = self
            .inner
            .coordinator
            .pending_destination_len(destination_id);
        let group_available = concurrency.saturating_sub(pending);
        let global_available = crate::exploratory_build::MAX_PENDING_BUILDS
            .saturating_sub(self.inner.coordinator.pending_len());
        let submit_limit = group_available.min(global_available);
        let mut submitted = 0;
        for (direction, deficit) in [
            (BuildDirection::Outbound, outbound_deficit),
            (BuildDirection::Inbound, inbound_deficit),
        ] {
            for _ in 0..deficit {
                if submitted >= submit_limit {
                    return Ok(());
                }
                if submit_destination_replacement(
                    &mut self.inner.coordinator,
                    &self.inner.destination_tunnels,
                    &self.inner.ssu2_handle,
                    &mut self.inner.tunnel_id_allocator,
                    destination_id,
                    direction,
                    local_router_hash,
                    &runtime.spec_id,
                    self.inner.options.dial_timeout,
                    &self.token,
                )
                .await
                .is_err()
                {
                    let _ = self
                        .manager
                        .with_destination_runtime(destination_id, |runtime| {
                            runtime.note_build_failure()
                        });
                    return Ok(());
                }
                submitted += 1;
            }
        }
        Ok(())
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
    pub(crate) async fn process_inbound(&mut self, inbound: &Ssu2InboundI2np) {
        if self.inner.retirement.is_none()
            && let Ok(routed) = self
                .inner
                .coordinator
                .route_inbound_i2np(inbound, wall_ms())
        {
            for outcome in routed.coordinator {
                if let BuildCoordinatorOutcome::DestinationBuildFailed { destination_id, .. } =
                    &outcome
                {
                    let _ = self
                        .manager
                        .with_destination_runtime(*destination_id, |runtime| {
                            runtime.note_build_failure()
                        });
                    continue;
                }
                if matches!(
                    &outcome,
                    BuildCoordinatorOutcome::DestinationBuildCancelled { .. }
                ) {
                    continue;
                }
                let BuildCoordinatorOutcome::DestinationEstablished {
                    attempt_id,
                    destination_id,
                    direction,
                    ..
                } = outcome
                else {
                    continue;
                };
                let Some(runtime) = self
                    .inner
                    .destination_runtimes
                    .get(&destination_id)
                    .cloned()
                else {
                    let _ = self
                        .inner
                        .coordinator
                        .cancel_destination_builds(destination_id);
                    continue;
                };
                let Ok(binding) = register_destination_material(
                    &self.manager,
                    &mut self.inner.coordinator,
                    destination_id,
                    attempt_id,
                    direction,
                    wall_secs(),
                ) else {
                    let _ = self
                        .manager
                        .with_destination_runtime(destination_id, |runtime| {
                            runtime.note_build_failure()
                        });
                    continue;
                };
                match direction {
                    BuildDirection::Outbound => {
                        if let Some(role) = self
                            .inner
                            .coordinator
                            .registry_mut()
                            .remove_outbound(binding.role_slot)
                        {
                            let role = i2pr_client::DestinationOutboundRole::from_role(
                                role,
                                binding.expires_at_ms,
                            );
                            let appended =
                                self.manager
                                    .with_destination_bridge(destination_id, |bridge| {
                                        bridge.append_router_outbound_role(binding.pool_slot, role)
                                    });
                            if !matches!(appended, Some(Ok(()))) {
                                let _ = self
                                    .manager
                                    .with_destination_runtime(destination_id, |runtime| {
                                        runtime.mark_tunnel_failed(binding.pool_slot)
                                    });
                            }
                        } else {
                            let _ = self
                                .manager
                                .with_destination_runtime(destination_id, |runtime| {
                                    runtime.mark_tunnel_failed(binding.pool_slot)
                                });
                        }
                    }
                    BuildDirection::Inbound => {
                        let receive_id = self
                            .manager
                            .with_destination_runtime(destination_id, |runtime| {
                                runtime
                                    .tunnel_registration(binding.pool_slot)
                                    .map(|registration| registration.tunnel_id().get())
                            })
                            .flatten();
                        if let Some(receive_id) = receive_id {
                            let owner = self
                                .manager
                                .register_inbound_tunnel_owner(receive_id, Arc::clone(&runtime));
                            let appended = if owner.is_ok() {
                                self.manager
                                    .with_destination_bridge(destination_id, |bridge| {
                                        bridge.append_router_inbound_receive(receive_id)
                                    })
                            } else {
                                None
                            };
                            if owner.is_err() || !matches!(appended, Some(Ok(()))) {
                                if owner.is_ok() {
                                    let _ =
                                        self.manager.unregister_inbound_tunnel_owner(receive_id);
                                }
                                if let Ok(receive_tunnel) = TunnelId::new(receive_id) {
                                    let _ = self
                                        .inner
                                        .coordinator
                                        .registry_mut()
                                        .remove_inbound(receive_tunnel);
                                }
                                let _ = self
                                    .manager
                                    .with_destination_runtime(destination_id, |runtime| {
                                        runtime.mark_tunnel_failed(binding.pool_slot)
                                    });
                            }
                        } else {
                            let _ = self
                                .manager
                                .with_destination_runtime(destination_id, |runtime| {
                                    runtime.mark_tunnel_failed(binding.pool_slot)
                                });
                        }
                    }
                }
            }
        }
        let bytes = &inbound.bytes;
        let Ok(message) = decode_inbound_ssu2_i2np(bytes) else {
            return;
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            I2npBody::Garlic(_) => {
                self.handle_direct_garlic(&message, bytes).await;
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
struct RouterBootstrapMaterial {
    local_hash: Hash,
}

/// Plan 212 §8 — dials the reference peer and bootstraps its
/// RouterInfo into the shared coordinator. Router bootstrap only:
/// RouterInfo verification, authoritative RouterInfo bootstrap,
/// authenticated SSU2 dial/session establishment, and peer material
/// preparation for candidate selection. It must NOT perform an
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
) -> Result<RouterBootstrapMaterial, ServiceProductError> {
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

    Ok(RouterBootstrapMaterial {
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
                    if !bridge.has_router_network_state(wall_ms()) {
                        return None;
                    }
                    let role = bridge.router_outbound_role_ref(wall_ms())?;
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
/// For each distinct Destination group:
/// 1. issue exact-three outbound and inbound builds through the shared
///    coordinator and hand established material to the group's canonical
///    `DestinationRuntime` pool;
/// 2. activate role projections while retaining registration and expiry
///    metadata in that pool;
/// 3. build the signed Standard LS2 only from usable pool lease sources and
///    validate it via `ValidatedLeaseSet2`;
/// 4. install router-backed state on every group member through its shared
///    bridge;
/// 5. register real inbound receive ownership
///    (`register_inbound_tunnel_owner` per receive id +
///    `register_inbound_destination_owner`);
/// 6. resolve configured remote target LS2 per service
///    (client profiles);
/// 7. publish local LS2 when server reachability requires it.
///
/// Tunnel ids allocate disjointly per service via
/// [`Plan212TunnelIdAllocator`]. A failed provisioning pass fails
/// atomically (caller tears down staged listeners).
fn register_destination_material(
    manager: &crate::service_tunnels::ServiceTunnelManager,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_id: i2pr_client::DestinationId,
    attempt_id: i2pr_tunnel::short::BuildAttemptId,
    direction: BuildDirection,
    now_seconds: u64,
) -> Result<ActivatedDestinationBinding, ServiceProductError> {
    let material = coordinator
        .take_destination_material(attempt_id)
        .ok_or_else(|| {
            ServiceProductError::Provisioning(
                "completed Destination build has no established material".to_owned(),
            )
        })?;
    let (pool_slot, tunnel, expires_at_ms) = manager
        .with_destination_runtime(destination_id, |runtime| {
            let expires_at_ms = now_seconds
                .saturating_add(u64::from(runtime.config().tunnel_lifetime_seconds()))
                .saturating_mul(1000);
            let pool_slot = match direction {
                BuildDirection::Inbound => runtime.admit_inbound(material, now_seconds),
                BuildDirection::Outbound => runtime.admit_outbound(material, now_seconds),
            }
            .map_err(|error| error.to_string())?;
            let tunnel = match runtime.activate_tunnel(pool_slot, now_seconds) {
                Ok(tunnel) => tunnel,
                Err(error) => {
                    let _ = runtime.remove_tunnel(pool_slot);
                    return Err(error.to_string());
                }
            };
            Ok::<_, String>((pool_slot, tunnel, expires_at_ms))
        })
        .ok_or_else(|| {
            ServiceProductError::Provisioning(
                "established Destination material has no group runtime".to_owned(),
            )
        })?
        .map_err(ServiceProductError::Provisioning)?;
    if let Err(error) = coordinator.set_lifetime_seconds(
        expires_at_ms
            .saturating_div(1000)
            .saturating_sub(now_seconds)
            .min(u64::from(u32::MAX)) as u32,
    ) {
        let _ = manager
            .with_destination_runtime(destination_id, |runtime| runtime.remove_tunnel(pool_slot));
        return Err(ServiceProductError::Provisioning(error.to_string()));
    }
    let role_slot = match coordinator.activate_destination_tunnel(direction, tunnel, now_seconds) {
        Ok(role_slot) => role_slot,
        Err(error) => {
            let _ = manager.with_destination_runtime(destination_id, |runtime| {
                runtime.remove_tunnel(pool_slot)
            });
            return Err(ServiceProductError::Provisioning(error.to_string()));
        }
    };
    Ok(ActivatedDestinationBinding {
        pool_slot,
        role_slot,
        expires_at_ms,
    })
}

#[allow(clippy::too_many_arguments)]
async fn submit_destination_replacement(
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &Ssu2DaemonHandle,
    allocator: &mut Plan212TunnelIdAllocator,
    destination_id: i2pr_client::DestinationId,
    direction: BuildDirection,
    local_router_hash: Hash,
    spec_id: &str,
    dial_timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<(), ServiceProductError> {
    let (candidates, _) = {
        let guard = destination_tunnels.lock().await;
        guard.destination_peer_candidates()
    };
    let selected = select_destination_path_os(&candidates)?;
    ensure_selected_peer_session(
        destination_tunnels,
        ssu2_handle,
        selected[0].router_hash(),
        dial_timeout,
        cancellation,
    )
    .await?;
    let ids = allocator.allocate_set(10);
    if ids.contains(&0) {
        return Err(ServiceProductError::Provisioning(format!(
            "{spec_id}: allocator emitted zero tunnel id"
        )));
    }
    let message_base = (ids[0] ^ 0x51A7_0000) & !0x03;
    let (roles, receives, nexts, creator, message_id, outbound_reply_router, originator_hash) =
        match direction {
            BuildDirection::Outbound => (
                [
                    HopRole::Participant,
                    HopRole::Participant,
                    HopRole::OutboundEndpoint,
                ],
                [ids[1], ids[2], ids[3]],
                [ids[2], ids[3], ids[4]],
                ids[0],
                message_base | 0x01,
                Some(local_router_hash),
                None,
            ),
            BuildDirection::Inbound => (
                [
                    HopRole::InboundGateway,
                    HopRole::Participant,
                    HopRole::Participant,
                ],
                [ids[6], ids[7], ids[8]],
                [ids[7], ids[8], ids[9]],
                ids[5],
                message_base | 0x02,
                None,
                Some(local_router_hash),
            ),
        };
    let peers = selected_peer_material(selected, roles, receives, nexts, spec_id)?;
    let creator_tunnel_id = TunnelId::new(creator).map_err(|_| {
        ServiceProductError::Provisioning(format!("{spec_id}: invalid creator tunnel id"))
    })?;
    let request = DestinationBuildRequest {
        destination_id,
        direction,
        peers,
        creator_tunnel_id,
        message_id,
        outbound_reply_router,
        originator_hash,
    };
    let mut rng = ChaCha8Rng::try_from_os_rng().map_err(|_| {
        ServiceProductError::Provisioning("operating-system randomness unavailable".to_owned())
    })?;
    let now_ms = wall_ms();
    coordinator.advance_time(now_ms);
    match coordinator
        .submit_destination(
            request,
            &ssu2_handle.delivery().clone(),
            &ShortBuildI2npBridge::new(),
            BridgeHeader::ShortTransport {
                message_id,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?
    {
        crate::exploratory_build::SubmitResult::Submitted { .. } => Ok(()),
        crate::exploratory_build::SubmitResult::Rejected { .. } => {
            Err(ServiceProductError::Provisioning(
                "Destination replacement delivery rejected".to_owned(),
            ))
        }
    }
}

/// Ensures the normal SSU2 owner has an authenticated direct session to a
/// selected first hop. Endpoint and key material are read from the same
/// validated RouterInfo that produced the candidate; only loopback targets
/// can pass the current controlled-profile dial constructor.
async fn ensure_selected_peer_session(
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &Ssu2DaemonHandle,
    router_hash: Hash,
    dial_timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<(), ServiceProductError> {
    let peer_id = i2pr_transport::PeerId::from_hash(router_hash);
    if !ssu2_handle.service().session_peer_links(peer_id).is_empty() {
        return Ok(());
    }
    let record = {
        let guard = destination_tunnels.lock().await;
        guard
            .store()
            .get(&i2pr_netdb::router_hash_from_proto_hash(router_hash))
            .cloned()
    }
    .ok_or_else(|| {
        ServiceProductError::Provisioning(
            "selected first hop is absent from the validated RouterInfo store".to_owned(),
        )
    })?;
    let target_material = record
        .router_info()
        .addresses()
        .iter()
        .filter(|address| address.transport_style() == "SSU2")
        .find_map(|address| {
            let parsed = i2pr_runtime::Ssu2RouterAddress::parse(address).ok()?;
            let endpoint = parsed.endpoint()?;
            let intro = parsed.intro_key()?;
            Some((endpoint, parsed.static_public_key(), intro))
        })
        .ok_or_else(|| {
            ServiceProductError::Provisioning(
                "selected first hop has no dialable validated SSU2 address".to_owned(),
            )
        })?;
    let static_key =
        i2pr_runtime::Ssu2PublicKey::new(*target_material.1.as_bytes()).map_err(|_| {
            ServiceProductError::Provisioning("selected first-hop static key is invalid".to_owned())
        })?;
    let intro_key = i2pr_runtime::IntroKey::new(*target_material.2.as_bytes());
    let address = std::net::SocketAddr::new(target_material.0.ip(), target_material.0.port());
    let target =
        crate::router_i2np::daemon_dial_target(router_hash, address, static_key, intro_key)?;
    ssu2_handle
        .dial(target, dial_timeout, cancellation)
        .await
        .map_err(|_| {
            ServiceProductError::Provisioning(
                "selected first-hop SSU2 session could not be established".to_owned(),
            )
        })?;
    Ok(())
}

fn selected_peer_material(
    candidates: Vec<DestinationPeerCandidate>,
    roles: [HopRole; 3],
    receives: [u32; 3],
    nexts: [u32; 3],
    spec_id: &str,
) -> Result<Vec<PeerBuildMaterial>, ServiceProductError> {
    if candidates.len() != 3 {
        return Err(ServiceProductError::Provisioning(format!(
            "{spec_id}: qualified selector returned a non-three-hop path"
        )));
    }
    candidates
        .into_iter()
        .zip(roles)
        .zip(receives)
        .zip(nexts)
        .map(|(((candidate, role), receive), next)| {
            let receive_tunnel = TunnelId::new(receive).map_err(|_| {
                ServiceProductError::Provisioning(format!("{spec_id}: invalid receive id"))
            })?;
            let next_tunnel = TunnelId::new(next).map_err(|_| {
                ServiceProductError::Provisioning(format!("{spec_id}: invalid next id"))
            })?;
            Ok(PeerBuildMaterial {
                router_hash: candidate.router_hash(),
                static_encryption_key: *candidate.static_encryption_key(),
                receive_tunnel,
                next_tunnel,
                role,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn provision_all_service_router_material(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    peer: &RouterBootstrapMaterial,
    runtimes: &[Arc<crate::service_tunnels::ServiceRuntime>],
    options: ServiceProductOptions,
    cancellation: &CancellationToken,
    allocator: &mut Plan212TunnelIdAllocator,
    startup_inbound: &mut VecDeque<Ssu2InboundI2np>,
    startup_inbound_bytes: &mut usize,
) -> Result<(), ServiceProductError> {
    use i2pr_client::build_signed_lease_set2;
    use i2pr_client::{
        DestinationRouting, DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager,
    };
    // Collect per-service target hashes first so HTTP and IRC
    // resolve independently; a missing/invalid target fails that
    // service only when the profile requires remote lookup
    // (client profiles). Server profiles skip lookup but still
    // require real outbound/inbound + LS2 + owner registration.
    let mut provisioned_destinations = std::collections::HashSet::new();
    for runtime in runtimes {
        let spec_id = runtime.spec_id.clone();
        if !provisioned_destinations.insert(runtime.destination_id) {
            continue;
        }
        // Project from the authoritative validated RouterInfo store and
        // select independently randomized, exact-three paths. A sparse or
        // metadata-incomplete store fails closed before any build submission.
        let (candidates, _projection_summary) = {
            let guard = destination_tunnels.lock().await;
            guard.destination_peer_candidates()
        };
        let (inbound_target, outbound_target) = manager
            .with_destination_runtime(runtime.destination_id, |destination| {
                (
                    destination.config().inbound_target(),
                    destination.config().outbound_target(),
                )
            })
            .ok_or_else(|| {
                ServiceProductError::Provisioning(format!("{spec_id}: group runtime missing"))
            })?;
        let mut outbound_roles = Vec::with_capacity(usize::from(outbound_target));
        let mut inbound_receive_ids = Vec::with_capacity(usize::from(inbound_target));
        let mut pending_outbound = Vec::with_capacity(usize::from(outbound_target));
        for build_index in 0..inbound_target.max(outbound_target) {
            let need_inbound = build_index < inbound_target;
            let outbound_candidates = if build_index < outbound_target {
                Some(select_destination_path_os(&candidates)?)
            } else {
                None
            };
            let inbound_candidates = if build_index < inbound_target {
                Some(select_destination_path_os(&candidates)?)
            } else {
                None
            };

            // Ten disjoint IDs cover each local creator, three remote receives,
            // the outbound terminal return tunnel, and the local inbound endpoint.
            let ids = allocator.allocate_set(10);
            let (ob_creator, ib_creator) = (ids[0], ids[5]);
            // Skip zero (allocator never emits it) and skip ids still
            // present in the registry (bounded retry).
            for id in &ids {
                if *id == 0 {
                    return Err(ServiceProductError::Provisioning(format!(
                        "{spec_id}: allocator emitted zero tunnel id"
                    )));
                }
            }
            let inbound_bridge = ShortBuildI2npBridge::new();
            let outbound_bridge = ShortBuildI2npBridge::new();
            let mut rng = ChaCha8Rng::try_from_os_rng().map_err(|_| {
                ServiceProductError::Provisioning(
                    "operating-system randomness unavailable".to_owned(),
                )
            })?;
            let delivery = ssu2_handle.delivery().clone();
            // Plan 213 — mask the low two bits before OR-ing the
            // direction bit so outbound (`| 0x01`) and inbound
            // (`| 0x02`) message ids stay distinct for every service
            // (later allocator ids already carry low bits).
            let message_base: u32 = (ob_creator ^ 0x51A7_0000) & !0x03;
            let outbound_peers = outbound_candidates
                .map(|selected| {
                    selected_peer_material(
                        selected,
                        [
                            HopRole::Participant,
                            HopRole::Participant,
                            HopRole::OutboundEndpoint,
                        ],
                        [ids[1], ids[2], ids[3]],
                        [ids[2], ids[3], ids[4]],
                        &spec_id,
                    )
                })
                .transpose()?;
            let inbound_peers = inbound_candidates
                .map(|selected| {
                    selected_peer_material(
                        selected,
                        [
                            HopRole::InboundGateway,
                            HopRole::Participant,
                            HopRole::Participant,
                        ],
                        [ids[6], ids[7], ids[8]],
                        [ids[7], ids[8], ids[9]],
                        &spec_id,
                    )
                })
                .transpose()?;
            if let Some(peers) = outbound_peers.as_ref() {
                ensure_selected_peer_session(
                    destination_tunnels,
                    ssu2_handle,
                    peers[0].router_hash,
                    options.dial_timeout,
                    cancellation,
                )
                .await?;
            }
            if let Some(peers) = inbound_peers.as_ref() {
                ensure_selected_peer_session(
                    destination_tunnels,
                    ssu2_handle,
                    peers[0].router_hash,
                    options.dial_timeout,
                    cancellation,
                )
                .await?;
            }
            let inbound_gateway_hash = inbound_peers.as_ref().map(|peers| peers[0].router_hash);
            let outbound_creator = TunnelId::new(ob_creator).map_err(|_| {
                ServiceProductError::Provisioning(format!("{spec_id}: bad outbound creator id"))
            })?;
            let inbound_creator = TunnelId::new(ib_creator).map_err(|_| {
                ServiceProductError::Provisioning(format!("{spec_id}: bad inbound creator id"))
            })?;
            let mut outbound = outbound_peers.map(|peers| DestinationBuildRequest {
                destination_id: runtime.destination_id,
                direction: BuildDirection::Outbound,
                peers,
                creator_tunnel_id: outbound_creator,
                message_id: message_base | 0x01,
                outbound_reply_router: Some(peer.local_hash),
                originator_hash: None,
            });
            if let Some(outbound) = outbound.take() {
                pending_outbound.push((outbound, outbound_bridge, message_base | 0x01));
            }
            let inbound = inbound_peers.map(|peers| DestinationBuildRequest {
                destination_id: runtime.destination_id,
                direction: BuildDirection::Inbound,
                peers,
                creator_tunnel_id: inbound_creator,
                message_id: message_base | 0x02,
                outbound_reply_router: None,
                originator_hash: Some(peer.local_hash),
            });
            if let Some(inbound) = inbound {
                coordinator
                    .submit_destination(
                        inbound,
                        &delivery,
                        &inbound_bridge,
                        BridgeHeader::ShortTransport {
                            message_id: message_base | 0x02,
                            expiration_seconds: wall_secs().saturating_add(60) as u32,
                        },
                        &mut rng,
                    )
                    .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
            }
            // Wait for both installs.
            let mut installed_inbound = false;
            let install_deadline = tokio::time::Instant::now() + options.i2pd_accept_timeout;
            while need_inbound
                && !installed_inbound
                && tokio::time::Instant::now() < install_deadline
            {
                let Some(inbound_msg) = wait_for_startup_inbound(
                    options.poll_interval,
                    cancellation,
                    ssu2_handle.next_inbound(),
                )
                .await?
                else {
                    continue;
                };
                let routed = coordinator.route_inbound_i2np(&inbound_msg, wall_ms());
                queue_startup_inbound(startup_inbound, startup_inbound_bytes, inbound_msg)?;
                let Ok(routed) = routed else {
                    continue;
                };
                for outcome in routed.coordinator {
                    if let BuildCoordinatorOutcome::DestinationEstablished {
                        attempt_id,
                        destination_id,
                        direction,
                        ..
                    } = outcome
                    {
                        if destination_id != runtime.destination_id {
                            return Err(ServiceProductError::Provisioning(
                                "established Destination material named a different group"
                                    .to_owned(),
                            ));
                        }
                        if direction != BuildDirection::Inbound {
                            return Err(ServiceProductError::Provisioning(
                                "inbound startup received a non-inbound build result".to_owned(),
                            ));
                        }
                        let _binding = register_destination_material(
                            manager,
                            coordinator,
                            destination_id,
                            attempt_id,
                            direction,
                            wall_secs(),
                        )?;
                        installed_inbound = true;
                    }
                }
            }
            if need_inbound && !installed_inbound {
                return Err(ServiceProductError::InboundBuildMissing);
            }
            // The local inbound ENDPOINT receive id (ids[9], the last
            // next tunnel) is the exact receive id registered by the
            // established material — it is the id the IBGW addresses
            // inbound traffic to. Resolve it directly; never infer
            // ownership from insertion order or another group's
            // gateway hash. Plan 381: resolving the creator id
            // (ids[5]) instead misses every time — the material is
            // keyed by the endpoint, so a successful build reported
            // `InboundBuildMissing` (observed live against stock i2pd:
            // installed yet unresolvable).
            if need_inbound {
                let local_receive = TunnelId::new(ids[9]).map_err(|_| {
                    ServiceProductError::Provisioning(format!(
                        "{spec_id}: invalid inbound receive id"
                    ))
                })?;
                let route = coordinator
                    .registry()
                    .inbound_gateway_route(local_receive)
                    .ok_or(ServiceProductError::InboundBuildMissing)?;
                if Some(route.gateway_router) != inbound_gateway_hash {
                    return Err(ServiceProductError::Provisioning(format!(
                        "{spec_id}: installed inbound route differs from selected group path"
                    )));
                }
                inbound_receive_ids.push(local_receive.get());
            }
        }
        let now_secs = wall_secs();
        let (lease_sources, minimum_inbound) = manager
            .with_destination_runtime(runtime.destination_id, |destination| {
                (
                    destination.inbound_lease_sources(now_secs),
                    destination.config().minimum_usable_inbound(),
                )
            })
            .ok_or_else(|| {
                ServiceProductError::Provisioning(format!("{spec_id}: group runtime missing"))
            })?;
        if lease_sources.len() < usize::from(minimum_inbound) {
            return Err(ServiceProductError::Provisioning(format!(
                "{spec_id}: group pool is below its usable inbound minimum"
            )));
        }
        let delivery = ssu2_handle.delivery().clone();
        if !pending_outbound.is_empty()
            && !crate::service_lifecycle::wait_for_outbound_pool_start(cancellation).await
        {
            return Err(ServiceProductError::Provisioning(
                "startup cancelled before outbound pool start".to_owned(),
            ));
        }
        for (outbound, bridge, message_id) in pending_outbound {
            let mut rng = ChaCha8Rng::try_from_os_rng().map_err(|_| {
                ServiceProductError::Provisioning(
                    "operating-system randomness unavailable".to_owned(),
                )
            })?;
            coordinator
                .submit_destination(
                    outbound,
                    &delivery,
                    &bridge,
                    BridgeHeader::ShortTransport {
                        message_id,
                        expiration_seconds: wall_secs().saturating_add(60) as u32,
                    },
                    &mut rng,
                )
                .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
            let install_deadline = tokio::time::Instant::now() + options.i2pd_accept_timeout;
            let mut binding = None;
            while binding.is_none() && tokio::time::Instant::now() < install_deadline {
                let Some(inbound_msg) = wait_for_startup_inbound(
                    options.poll_interval,
                    cancellation,
                    ssu2_handle.next_inbound(),
                )
                .await?
                else {
                    continue;
                };
                let routed = coordinator.route_inbound_i2np(&inbound_msg, wall_ms());
                queue_startup_inbound(startup_inbound, startup_inbound_bytes, inbound_msg)?;
                let Ok(routed) = routed else {
                    continue;
                };
                for outcome in routed.coordinator {
                    let BuildCoordinatorOutcome::DestinationEstablished {
                        attempt_id,
                        destination_id,
                        direction,
                        ..
                    } = outcome
                    else {
                        continue;
                    };
                    if destination_id != runtime.destination_id
                        || direction != BuildDirection::Outbound
                    {
                        return Err(ServiceProductError::Provisioning(
                            "outbound-first startup received a mismatched build result".to_owned(),
                        ));
                    }
                    binding = Some(register_destination_material(
                        manager,
                        coordinator,
                        destination_id,
                        attempt_id,
                        direction,
                        wall_secs(),
                    )?);
                }
            }
            let binding = binding.ok_or(ServiceProductError::OutboundBuildMissing)?;
            let gateway_role = coordinator
                .registry_mut()
                .remove_outbound(binding.role_slot)
                .ok_or_else(|| {
                    ServiceProductError::Provisioning(format!("{spec_id}: outbound role missing"))
                })?;
            outbound_roles.push((
                binding.pool_slot,
                i2pr_client::DestinationOutboundRole::from_role(
                    gateway_role,
                    binding.expires_at_ms,
                ),
            ));
        }
        let inbound_expires_at_ms = lease_sources
            .iter()
            .map(i2pr_client::InboundLeaseSource::tunnel_expires_seconds)
            .max()
            .unwrap_or(now_secs)
            .saturating_mul(1000);
        let now_ms = wall_ms();
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
        let lease_set2 = build_signed_lease_set2(&service_identity, &lease_sources, published_secs)
            .map_err(|error| ServiceProductError::Provisioning(error.to_string()))?;
        let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
            lease_set2.clone(),
            Some(service_identity.id().as_netdb_key()),
            i2pr_netdb::LeaseSet2ValidationContext::new(published_secs),
        )
        .map_err(|error| ServiceProductError::Provisioning(format!("{error:?}")))?;
        let outbound_expires_at_ms = outbound_roles
            .iter()
            .map(|(_, role)| role.expires_at_ms())
            .max()
            .unwrap_or(now_ms);
        let material = crate::sam::streams::RouterDestinationNetworkState::new_with_outbound_roles(
            service_destination_id,
            DestinationRouting::new(DestinationRoutingConfig::balanced()),
            EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_roles,
            lease_set2,
            validated,
            inbound_receive_ids.clone(),
            outbound_expires_at_ms,
            inbound_expires_at_ms,
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
    let mut published_destinations = std::collections::HashSet::new();
    for runtime in runtimes {
        let spec_id = runtime.spec_id.clone();
        if manager.spec_is_server(&spec_id) {
            if !published_destinations.insert(runtime.destination_id) {
                continue;
            }
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
        // Plan 351 Gate 1 in force: `ServiceTunnelSpec::validate` has
        // already rejected an encrypted-service address on a spec that is
        // not a `delay_open` client, so by the time this loop runs the
        // `EncryptedService` projection can only belong to a spec whose
        // remote peer fails independently of the product. That is what
        // makes `record_and_continue` below the safe spelling: it is not
        // a failure being tolerated, it is the only permitted failure
        // mode being tolerated.
        let target = match manager.project_remote_target(&reference) {
            RemoteTargetProjection::LocalCoOwned => {
                // Local co-owned hash -> stay local, do not enter the
                // remote path.
                continue;
            }
            RemoteTargetProjection::Remote(destination_hash) => destination_hash,
            RemoteTargetProjection::EncryptedService(address) => {
                // Never propagates. Every failure mode is recorded on the
                // manager's per-service status surface and the loop moves
                // to the next service.
                provision_encrypted_service_target(
                    manager,
                    coordinator,
                    destination_tunnels,
                    ssu2_handle,
                    runtime.destination_id,
                    &spec_id,
                    address,
                    options,
                )
                .await;
                continue;
            }
        };
        resolve_remote_destination_for_service(
            manager,
            coordinator,
            destination_tunnels,
            ssu2_handle,
            runtime.destination_id,
            target,
            options,
        )
        .await?;
    }
    Ok(())
}

/// Plan 351 — provisions one encrypted-service (`.b33`) client target.
///
/// **This function never returns `Err` and never propagates.** The two
/// production callers of [`provision_all_service_router_material`] shut
/// the manager down, cancel the operator token, and shut the SSU2 handle
/// down on any error, so a single unreachable b33 would otherwise take
/// down every configured service in the product. `DelayOpen` clients are
/// the only kind allowed to name one (Gate 1), and they already carry
/// per-destination isolation, so this is exactly where the containment is
/// real rather than asserted.
///
/// Failure is visible through two independent, non-secret surfaces:
/// `ServiceTunnelManager::record_encrypted_target_status`, which the
/// I2PControl `GetStatus` and the daemon log both read, and the bounded
/// `RemoteDeliveryCounters` series in `service_delivery.rs`. Neither
/// carries the lookup secret, the derived storage key, or any fetched
/// payload bytes.
#[allow(clippy::too_many_arguments)]
async fn provision_encrypted_service_target(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    service_destination: i2pr_client::DestinationId,
    spec_id: &str,
    address: i2pr_proto::EncryptedServiceAddress,
    options: ServiceProductOptions,
) {
    // Every outcome is recorded on the manager's per-service status surface
    // before this function returns. `let _ =` rather than `?` is the point of
    // the function: this is the only place a resolution failure is allowed to
    // stop, and stopping here means exactly one `.b33` service is unavailable.
    // Plan 381: success records `Resolved` plus the installed inner hash,
    // because no status row may stay `None` once the branch has run and the
    // delay-open data path needs the hash to connect through. The shape is
    // deliberately `if let Err(status) = ... .map(...)`: the consumer-caller
    // guard pins that spelling as the proof the typed failure is consumed
    // here rather than propagated.
    if let Err(status) = resolve_encrypted_destination_for_service(
        manager,
        coordinator,
        destination_tunnels,
        ssu2_handle,
        service_destination,
        spec_id,
        address,
        options,
    )
    .await
    .map(|destination_hash| {
        manager.record_encrypted_target_status(
            spec_id,
            crate::service_tunnels::EncryptedTargetStatus::Resolved,
        );
        manager.record_encrypted_target_inner_hash(spec_id, destination_hash);
    }) {
        manager.record_encrypted_target_status(spec_id, status);
        if let Some(capability) = manager.router_delivery() {
            capability
                .record_observation("encrypted_target_failed")
                .await;
        }
    }
}

/// Plan 351 — resolves one encrypted-service target and installs its inner
/// LeaseSet2 under the **unblinded** destination hash.
///
/// `Err` is always a `EncryptedTargetStatus`, never a `ServiceProductError`,
/// and never a foreign error string. That is a deliberate narrowing: this path
/// runs where a propagated error would shut the whole product down, so the
/// set of things that can escape it is a closed enum with no ability to carry a
/// secret, a derived key, or a fetched payload.
///
/// The sequence, and why each step is where it is:
///
/// 1. read the installed lookup secret (Gate 2: I2PControl definition options);
/// 2. derive **today's** blinded storage key — the record is filed under this
///    key, and it is not a destination hash;
/// 3. begin the lookup against that key verbatim, through the same
///    coordinator, floodfill policy, reply-path selection, tunnel composition,
///    and delivery service as an ordinary lookup;
/// 4. on a type-5 reply, unwrap through `EncryptedServiceResolver`, which owns
///    ADR 0032's type-11 profile policy;
/// 5. bind the unwrapped record to the `.b33` address — see
///    `encrypted_service_resolver::bind_inner_to_address`;
/// 6. validate it as an ordinary `LeaseSet2` and install under the destination
///    hash that record names.
#[allow(clippy::too_many_arguments)]
async fn resolve_encrypted_destination_for_service(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    coordinator: &mut ExploratoryBuildCoordinator,
    destination_tunnels: &Arc<Mutex<DestinationTunnelCoordinator>>,
    ssu2_handle: &mut Ssu2DaemonHandle,
    service_destination: i2pr_client::DestinationId,
    spec_id: &str,
    address: i2pr_proto::EncryptedServiceAddress,
    options: ServiceProductOptions,
) -> Result<[u8; 32], EncryptedTargetStatus> {
    let now_secs = wall_secs() as u32;
    let now_ms = wall_ms();
    let deadline_ms = now_ms.saturating_add(
        options
            .tunnel_deadline
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    );

    // Step 1 — the consumer credential and the lookup secret.
    //
    // Both are optional and independent. A `.b33` may demand no credential, a
    // credential, a lookup secret, or both; the address's own flags say which
    // of the two it insists on, and the configuration supplies the rest.
    //
    // The lookup secret is *borrowed* as `Option<&str>` through the secret
    // type's single accessor, which Plan 352's guard pins to exactly one call
    // site in the daemon — this one. The credential is **opened** here and
    // nowhere else in the product: this is the single point at which client key
    // material becomes a live value, and it is scoped to this resolution — the
    // opened credential drops at the end of the function, zeroized, and is
    // never stored back.
    let secret = manager.encrypted_target_secret(spec_id);
    let credential = match manager.encrypted_target_credential(spec_id) {
        Some(sealed) => Some(
            sealed
                .open()
                .map_err(|_| EncryptedTargetStatus::ClientCredentialUnusable)?,
        ),
        None => None,
    };

    // Step 2 — one resolver per attempt. It is `!Sync` by construction
    // (`PskClientKey` / `X25519PrivateKey` inside), so it stays on this task's
    // stack and is dropped with it.
    let mut resolver = EncryptedServiceResolver::default();
    let secret_option = secret.as_ref().and_then(|held| held.as_option());
    // Plan 380: the branch is selected by what the router holds, not by what
    // the address asks for. `begin_authorized` is correct for **both** cases —
    // it delegates to the same builder with the address's own
    // `requires_client_key` flag — so a credential supplied to a `.b33` that
    // does not demand one is presented and simply ignored by a record with no
    // authorization block, rather than being rejected for being present. The
    // converse is not symmetric and is the fail-closed case below.
    let (resolve_id, storage_key) = match credential.as_ref() {
        Some(credential) => resolver.begin_authorized(
            &address,
            secret_option,
            credential.as_borrowed(),
            now_secs,
            deadline_ms,
        ),
        None => resolver.begin(&address, secret_option, now_secs, deadline_ms),
    }
    .map_err(|error| match error {
        // The address itself declared `B32_FLAG_REQUIRES_CLIENT_KEY` and no
        // credential is configured. `begin` refuses such an address outright,
        // so this is raised **before** any lookup is composed — no network
        // round trip happens for a configuration that could only be refused.
        EncryptedServiceConsumerError::InvalidAddress(
            i2pr_client::EncryptedLeaseSetError::ClientAuthorizationRequired,
        ) => EncryptedTargetStatus::ClientCredentialRequired,
        // A credential was supplied but could not be retained for the request.
        EncryptedServiceConsumerError::CredentialNotCopyable(_) => {
            EncryptedTargetStatus::ClientCredentialUnusable
        }
        EncryptedServiceConsumerError::NotAuthorized => {
            EncryptedTargetStatus::ClientCredentialRejected
        }
        EncryptedServiceConsumerError::TooManyResolves => EncryptedTargetStatus::DispatchFailed,
        _ => EncryptedTargetStatus::StorageKeyUnavailable,
    })?;
    let storage_key_hash = *storage_key.as_hash();

    // Step 3 — the lookup. `routing_key` is the same value: a blinded storage
    // key has no separate routing key to derive, and inventing one would make
    // floodfill selection disagree with the key the record is filed under.
    let routing_key = RouterHash::from_hash(storage_key_hash);
    let receive_ids = manager
        .with_destination_bridge(service_destination, |bridge| {
            bridge.router_inbound_receive_ids()
        })
        .unwrap_or_default();
    let Some(local_receive_for_lookup) = receive_ids.first().copied() else {
        // Release the slot before returning: every exit path must, so an
        // abandoned future cannot strand the bounded table.
        let _ = resolver.cancel(resolve_id);
        return Err(EncryptedTargetStatus::DispatchFailed);
    };
    let Ok(local_receive) = TunnelId::new(local_receive_for_lookup) else {
        let _ = resolver.cancel(resolve_id);
        return Err(EncryptedTargetStatus::DispatchFailed);
    };
    let Ok(reply_path) = reply_path_for_inbound_route(coordinator.registry(), local_receive) else {
        let _ = resolver.cancel(resolve_id);
        return Err(EncryptedTargetStatus::NoFloodfillCandidate);
    };
    let (_request_id, action) = {
        let mut coord_guard = destination_tunnels.lock().await;
        match coord_guard.begin_encrypted_lease_lookup(storage_key_hash, &routing_key, reply_path) {
            Ok(pair) => pair,
            Err(_) => {
                let _ = resolver.cancel(resolve_id);
                return Err(EncryptedTargetStatus::NoFloodfillCandidate);
            }
        }
    };

    // Bounded re-query, mirroring the ordinary path: a floodfill that has not
    // settled the reply tunnel's inbound side answers into a black hole, so a
    // later copy is sent after a bounded settle delay.
    const MAX_LOOKUP_ATTEMPTS: u64 = 3;
    const LOOKUP_ATTEMPT_WINDOW: Duration = Duration::from_secs(25);
    const LOOKUP_SETTLE_DELAY: Duration = Duration::from_secs(5);

    let mut attempts: u64 = 0;
    let mut resolved_message: Option<i2pr_proto::DatabaseStoreMessage> = None;
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    while resolved_message.is_none() && attempts < MAX_LOOKUP_ATTEMPTS {
        attempts += 1;
        {
            let coord_guard = destination_tunnels.lock().await;
            let delivery = ssu2_handle.delivery().clone();
            let Ok(deadline) = Deadline::new(options.tunnel_deadline) else {
                break;
            };
            let dispatch = manager
                .with_destination_bridge(service_destination, |bridge| {
                    if !bridge.has_router_network_state(wall_ms()) {
                        return None;
                    }
                    let role = bridge.router_outbound_role_ref(wall_ms())?;
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
                .flatten();
            let Some(dispatch) = dispatch else {
                break;
            };
            let mut admitted = true;
            for cell_delivery in &dispatch.deliveries {
                let Ok(request) = crate::router_i2np::RouterDeliveryRequest::new(
                    cell_delivery.target(),
                    cell_delivery.message_bytes().to_vec(),
                    options.delivery_timeout,
                ) else {
                    admitted = false;
                    break;
                };
                let outcome = delivery.deliver(request, &CancellationToken::new());
                if !matches!(outcome, crate::router_i2np::RouterDeliveryOutcome::Accepted) {
                    admitted = false;
                    break;
                }
            }
            if !admitted {
                break;
            }
        }
        let attempt_deadline = tokio::time::Instant::now() + LOOKUP_ATTEMPT_WINDOW;
        while tokio::time::Instant::now() < attempt_deadline && resolved_message.is_none() {
            let next =
                tokio::time::timeout(options.poll_interval, ssu2_handle.next_inbound()).await;
            let Ok(Some(inbound)) = next else {
                continue;
            };
            let Ok(message) = decode_inbound_ssu2_i2np(&inbound.bytes) else {
                continue;
            };
            let cell = match message.body() {
                I2npBody::TunnelData(cell) => cell.clone(),
                _ => continue,
            };
            let Ok(outcome) = inbound_dispatch::dispatch_inbound_tunnel_data(
                coordinator.registry_mut(),
                &cell,
                wall_ms(),
            ) else {
                continue;
            };
            let bytes = match outcome {
                InboundDispatchOutcome::DatabaseStoreComplete { bytes }
                | InboundDispatchOutcome::DatabaseSearchReplyComplete { bytes }
                | InboundDispatchOutcome::DeliveryStatusComplete { bytes }
                | InboundDispatchOutcome::GarlicComplete { bytes } => bytes,
                _ => continue,
            };
            let Ok(envelope) = I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE) else {
                continue;
            };
            let now_secs = wall_secs() as u32;
            let ingest = {
                let mut coord_guard = destination_tunnels.lock().await;
                coord_guard.ingest_tunnel_lease_store(_request_id, &envelope, now_secs)
            };
            match ingest {
                Ok(LeaseStoreIngestOutcome::EncryptedLeaseSet2Ready { message, .. }) => {
                    resolved_message = Some(*message);
                }
                Ok(LeaseStoreIngestOutcome::Completed { .. }) => {
                    // A Standard LeaseSet2 arrived under a blinded storage key.
                    // That is not the service; fail closed rather than
                    // installing a record the `.b33` did not name.
                    return Err(EncryptedTargetStatus::StorageKeyMismatch);
                }
                Ok(LeaseStoreIngestOutcome::Continue) | Ok(LeaseStoreIngestOutcome::Ignored) => {}
                Err(_) => {}
            }
        }
        if resolved_message.is_none() && attempts < MAX_LOOKUP_ATTEMPTS {
            tokio::time::sleep(LOOKUP_SETTLE_DELAY).await;
        }
    }

    let Some(message) = resolved_message else {
        // Nothing installed. The lookup engine already released its slot when
        // it produced the terminal failure; cancel releases the resolver's.
        let _ = resolver.cancel(resolve_id);
        return Err(EncryptedTargetStatus::LookupExhausted);
    };

    // Step 4 — unwrap. `ingest_store` takes the request out of the table before
    // it can fail, so the lease is released on every outcome including a bad
    // reply.
    let resolved = resolver.ingest_store(resolve_id, &message, wall_secs() as u32);
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(error) => {
            return Err(match error {
                EncryptedServiceConsumerError::NotEncryptedLeaseSet
                | EncryptedServiceConsumerError::KeyMismatch => {
                    EncryptedTargetStatus::StorageKeyMismatch
                }
                EncryptedServiceConsumerError::RecordTooLarge => {
                    EncryptedTargetStatus::StorageKeyMismatch
                }
                EncryptedServiceConsumerError::RecordRejected { .. } => {
                    EncryptedTargetStatus::LeaseSetValidationFailed
                }
                EncryptedServiceConsumerError::NotAuthorized => {
                    // Plan 380: a credential was presented and the record's
                    // layer-1 authorization block refused it. Deliberately the
                    // same status whether the key was wrong or the record was
                    // built for a different client — distinguishing them would
                    // make this surface an oracle for probing which credential
                    // a service accepts.
                    EncryptedTargetStatus::ClientCredentialRejected
                }
                _ => EncryptedTargetStatus::UnwrapFailed,
            });
        }
    };

    // Step 5 — bind the unwrapped record to the address. This is the step the
    // b33 format exists to make possible and the step nothing else performs.
    let inner = resolved.inner_lease_set2();
    // Step 5's policy lives with the rest of the ELS2 consumer policy in
    // `encrypted_service_resolver::bind_inner_to_address`, where it is directly
    // testable and where a future second consumer cannot reimplement it wrong.
    let destination_hash = bind_inner_to_address(&address, inner)
        .ok_or(EncryptedTargetStatus::IdentityBindingFailed)?;

    // Step 6 — ordinary validation, then install under the destination hash the
    // inner record itself names. That hash is the service's real Base32
    // address, which is what the rest of the delivery path looks up.
    // Plan 381: the inner of a validated envelope routinely carries
    // BLINDED_ON_PUBLICATION (the destination genuinely publishes
    // blinded), so opt into that flag here — after envelope
    // authentication, inner signature verification, and address
    // binding above. The unencrypted path stays strict.
    let now_secs = wall_secs() as u32;
    let validated = ValidatedLeaseSet2::from_lease_set2(
        inner.clone(),
        Some(destination_hash),
        LeaseSet2ValidationContext::with_policy(
            now_secs,
            i2pr_netdb::LeaseSet2ValidationPolicy {
                allow_blinded_on_publication: true,
                ..Default::default()
            },
        ),
    )
    .map_err(|_| EncryptedTargetStatus::LeaseSetValidationFailed)?;
    manager
        .with_destination_bridge(service_destination, |bridge| {
            bridge.install_remote_lease_set2_into_router_state(validated, wall_ms())
        })
        .ok_or(EncryptedTargetStatus::LeaseSetValidationFailed)?
        .map_err(|_| EncryptedTargetStatus::LeaseSetValidationFailed)?;

    if let Some(capability) = manager.router_delivery() {
        capability
            .record_observation("encrypted_target_resolved")
            .await;
    }
    Ok(*destination_hash.as_bytes())
}

/// Bounded lifetime of a type-5 publication window (Plan 337).
///
/// The outer encrypted-LeaseSet2 record wraps the service's ordinary
/// LeaseSet2; the window is a publication bound, not a lease duration, and
/// the inner record's own publication is what a client reads.
const ELS2_PUBLICATION_WINDOW_SECONDS: u32 = 600;

/// Builds the `DatabaseStore` a service publishes, and the key it is
/// addressed at (Plan 337).
///
/// A service that installed encrypted-LeaseSet2 material on the shared
/// manager publishes a type-5 record at **its own blinded storage key** —
/// the day's key derived from the record's blinded public key, never the
/// destination hash. A service with no material publishes its ordinary
/// LeaseSet2 at the destination hash derived from the record itself. The
/// caller selects the floodfill for whichever key this returns, so the two
/// cases differ in both payload and addressing.
///
/// The rng seed mixes the destination hash so two services publishing in
/// the same second do not reuse an ElGamal nonce stream; it is a
/// publication nonce seed, not key material.
fn service_publication_store(
    manager: &Arc<crate::service_tunnels::ServiceTunnelManager>,
    service_destination: i2pr_client::DestinationId,
    lease_set2: i2pr_proto::LeaseSet2,
    now_seconds: u64,
) -> Result<i2pr_proto::DatabaseStoreMessage, ServiceProductError> {
    let destination_hash =
        lease_set2.header().destination().hash().map_err(|_| {
            ServiceProductError::Provisioning("LS2 destination hash failed".to_owned())
        })?;
    let Some(material) = manager.els2_material_for_destination(service_destination) else {
        return Ok(i2pr_proto::DatabaseStoreMessage {
            key: destination_hash,
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(lease_set2)),
        });
    };
    let now_seconds_u32 = u32::try_from(now_seconds).unwrap_or(u32::MAX);
    // The outer record must not outlive the inner LeaseSet2 it carries.
    let expires_seconds = now_seconds_u32.saturating_add(ELS2_PUBLICATION_WINDOW_SECONDS);
    let mut rng = ChaCha8Rng::seed_from_u64(
        now_seconds.wrapping_add(u64::from(destination_hash.as_bytes()[0] as u32)),
    );
    let (_key, message) = material
        .build_database_store(&lease_set2, now_seconds_u32, expires_seconds, &mut rng)
        .map_err(|error| {
            ServiceProductError::Provisioning(format!("type-5 record construction failed: {error}"))
        })?;
    Ok(message)
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
    // Plan 337: the store key is the record's **own** storage key — the
    // day's blinded key for an encrypted service, the destination hash
    // otherwise — and the floodfill below is selected for whichever it is.
    let store_message =
        service_publication_store(manager, service_destination, lease_set2, wall_secs())?;
    // The publication state machine requires a floodfill peer. The
    // controlled lane provisions the reference as floodfill; pick
    // the first floodfill candidate for the record's routing key from
    // the authoritative store. When no floodfill is known, fail
    // closed (no direct transport, no synthetic publication).
    let floodfill = {
        let coord_guard = destination_tunnels.lock().await;
        let target = DestinationHash::from_hash(store_message.key);
        let routing_key = i2pr_netdb::router_hash_from_destination(target);
        coord_guard
            .candidate_hashes(&target, &routing_key)
            .first()
            .copied()
            .ok_or_else(|| {
                ServiceProductError::Provisioning("no floodfill for publication".to_owned())
            })?
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
                if !bridge.has_router_network_state(wall_ms()) {
                    return None;
                }
                let role = bridge.router_outbound_role_ref(wall_ms())?;
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

#[cfg(test)]
mod plan337_publication {
    use super::*;
    use crate::service_els2::ServiceEls2Material;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_i2pcontrol::proposal_leaseset_mode::{
        LeaseSetClientAuthEntry, LeaseSetSecurityPlan, resolve_encrypt_lease_set_mode,
        resolve_lease_set_security,
    };
    use tempfile::TempDir;

    const NOW: u32 = 1_700_000_000;

    /// A real persistent server spec: `PersistentClient` ownership with a
    /// dedicated group, so the manager writes a persisted identity record and
    /// the ELS2 material is derivable from it. This is the shape a
    /// control-created encrypted server has once Plan 338 lands.
    fn persistent_generic_server(service_id: &str) -> i2pr_service_tunnels::ServiceTunnelSpec {
        i2pr_service_tunnels::ServiceTunnelSpec {
            id: i2pr_service_tunnels::ServiceTunnelId::parse(service_id).expect("id"),
            kind: i2pr_service_tunnels::ServiceTunnelKind::GenericServer,
            enabled: true,
            listener: None,
            target: Some(i2pr_service_tunnels::ServerTarget::LoopbackTcp(
                "127.0.0.1:9".parse().expect("target"),
            )),
            targets: Vec::new(),
            destination: None,
            policy: i2pr_service_tunnels::DestinationPolicy::PersistentClient,
            inbound_port: Some(9_090),
            max_connections: 2,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
            shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: i2pr_service_tunnels::IdlePolicy::disabled(),
            access: i2pr_service_tunnels::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_options: None,
            http_policy: i2pr_service_tunnels::HttpServerPolicy::default(),
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }
    }

    /// A manager holding one **persistent** server spec, prepared so the
    /// service's identity record is written and committed. This is the shape
    /// a control-created encrypted server has once Plan 338 lands, and it is
    /// what makes the ELS2 material derivable at all.
    fn persistent_server_manager(
        data_dir: &std::path::Path,
        service_id: &str,
    ) -> Arc<crate::service_tunnels::ServiceTunnelManager> {
        let set = i2pr_service_tunnels::ServiceTunnelSet {
            tunnels: vec![persistent_generic_server(service_id)],
        };
        let config = crate::service_tunnels::ServiceTunnelManagerConfig {
            data_dir: data_dir.to_path_buf(),
            aggregate_connection_ceiling: 1024,
            per_service_connection_ceiling: 128,
            specs: Arc::new(set),
            aliases: Arc::new(i2pr_service_tunnels::StaticAliasTable::new()),
        };
        let manager = Arc::new(
            crate::service_tunnels::ServiceTunnelManager::new(config).expect("manager builds"),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(manager.prepare())
            .expect("the persistent server prepares");
        assert_eq!(runtime.len(), 1, "one prepared runtime");
        assert!(
            manager
                .service_identity_record(service_id)
                .expect("record resolves")
                .is_some(),
            "a persistent server must have a persisted identity record"
        );
        manager
    }

    fn signed_inner_ls2(signer: &RouterIdentityBundle) -> i2pr_proto::LeaseSet2 {
        let destination = i2pr_proto::Destination::new(signer.identity().key_and_cert().clone())
            .expect("destination");
        let header = i2pr_proto::LeaseSet2Header::new(
            destination,
            NOW,
            600,
            i2pr_proto::LeaseSet2Flags::from_raw(0),
        )
        .expect("header");
        let placeholder =
            i2pr_proto::SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64])
                .expect("placeholder");
        let keys = vec![
            i2pr_proto::LeaseSet2EncryptionKey::new(
                i2pr_proto::CryptoKeyType::X25519,
                vec![0x55; 32],
            )
            .expect("key"),
        ];
        let leases = vec![i2pr_proto::Lease2::new(
            i2pr_proto::Hash::from_bytes([0x11; 32]),
            7,
            i2pr_proto::Date32::from_seconds(NOW + 600),
        )];
        let unsigned = i2pr_proto::LeaseSet2::new(
            header,
            i2pr_proto::Mapping::empty(),
            keys,
            leases,
            placeholder,
        )
        .expect("unsigned");
        let signature = signer
            .signing_key()
            .sign(&unsigned.signature_preimage())
            .expect("sign");
        i2pr_proto::LeaseSet2::new(
            unsigned.header().clone(),
            unsigned.options().clone(),
            unsigned.encryption_keys().to_vec(),
            unsigned.leases().to_vec(),
            signature,
        )
        .expect("signed ls2")
    }

    fn plan_for(mode: &str, secret: Option<&str>, clients: usize) -> LeaseSetSecurityPlan {
        let resolved = resolve_encrypt_lease_set_mode(mode).expect("frozen mode");
        let entries: Vec<LeaseSetClientAuthEntry> = (0..clients)
            .map(|index| {
                LeaseSetClientAuthEntry::new(
                    &format!("c{index}"),
                    &format!("{:02x}", index + 1).repeat(32),
                )
                .expect("valid entry")
            })
            .collect();
        resolve_lease_set_security(Some(&resolved), secret, &entries).expect("valid plan")
    }

    /// A manager over a temp data dir, shaped like the composition root's.
    fn manager(data_dir: &std::path::Path) -> Arc<crate::service_tunnels::ServiceTunnelManager> {
        let config = crate::service_tunnels::ServiceTunnelManagerConfig {
            data_dir: data_dir.to_path_buf(),
            aggregate_connection_ceiling: 1024,
            per_service_connection_ceiling: 128,
            specs: Arc::new(i2pr_service_tunnels::ServiceTunnelSet::new()),
            aliases: Arc::new(i2pr_service_tunnels::StaticAliasTable::new()),
        };
        Arc::new(crate::service_tunnels::ServiceTunnelManager::new(config).expect("manager builds"))
    }

    /// Plan 337: with encrypted-LeaseSet2 material installed, the
    /// publication path files a **type-5** record at the day's **blinded**
    /// storage key — the record's own key, not the destination hash.
    ///
    /// This is the property that makes the mode real, and it is what the
    /// floodfill selection downstream keys on. Before Plan 337 no
    /// control-created service reached this function at all.
    #[test]
    fn an_encrypted_service_publishes_type5_at_its_blinded_storage_key() {
        let dir = TempDir::new().expect("temp dir");
        // A real persistent server spec, committed through the manager, so
        // the identity record exists where the manager says it does. Plan
        // 338 routes the ELS2 material through that same resolution rather
        // than re-deriving a store path here.
        let manager = persistent_server_manager(dir.path(), "encsvc");
        let options = std::collections::BTreeMap::new();
        let plan = plan_for("blinded", None, 0);
        let material: Arc<ServiceEls2Material> = Arc::new(
            crate::service_els2::load_service_els2_material(&manager, "encsvc", &plan, &options)
                .expect("material resolves")
                .expect("blinded mode produces material"),
        );
        manager.install_els2_material("encsvc", Arc::clone(&material));

        let signer = RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("signer");
        let inner = signed_inner_ls2(&signer);
        let destination_hash = inner.header().destination().hash().expect("hash");

        // The manager resolves the material per destination; with no
        // committed runtime there is none, so this row drives the key
        // selection through the spec-keyed lookup the sweep uses.
        let store = {
            let material = manager
                .els2_material_for_spec("encsvc")
                .expect("material is installed");
            let mut rng = ChaCha8Rng::seed_from_u64(u64::from(NOW));
            let (_key, message) = material
                .build_database_store(&inner, NOW, NOW + 600, &mut rng)
                .expect("type-5 record builds");
            message
        };

        let payload = match store.data {
            i2pr_proto::DatabaseStoreData::EncryptedLeaseSet(record) => record,
            other => panic!("blinded mode must publish a type-5 record, got {other:?}"),
        };
        // Addressed at the blinded key, which is derived from the record's
        // own blinded public key and is *not* the destination hash.
        assert_ne!(
            store.key, destination_hash,
            "a type-5 record must not be filed at the destination hash"
        );
        assert_eq!(
            store.key,
            i2pr_crypto::red25519::blinded_storage_key(
                &i2pr_crypto::red25519::Red25519PublicKey::decode(payload.blinded_public_key())
                    .expect("blinded public key")
            ),
            "the store key must be the record's own blinded storage key"
        );
        assert!(!payload.outer_ciphertext().is_empty());
    }

    /// Plan 337: a service with no installed material publishes its
    /// ordinary LeaseSet2 at the destination hash, unchanged. The
    /// encrypted branch must not leak into the ordinary path.
    #[test]
    fn an_ordinary_service_publishes_its_lease_set_at_the_destination_hash() {
        let dir = TempDir::new().expect("temp dir");
        let manager = manager(dir.path());
        let signer = RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("signer");
        let inner = signed_inner_ls2(&signer);
        let destination_hash = inner.header().destination().hash().expect("hash");
        // No material installed, and no committed runtime: the ordinary
        // branch must be taken.
        assert!(manager.els2_material_for_spec("encsvc").is_none());
        let store = service_publication_store(
            &manager,
            i2pr_client::DestinationId::from_hash(i2pr_proto::Hash::from_bytes([0xA1; 32])),
            inner,
            u64::from(NOW),
        )
        .expect("ordinary store builds");
        assert_eq!(store.key, destination_hash);
        assert!(matches!(
            store.data,
            i2pr_proto::DatabaseStoreData::LeaseSet2(_)
        ));
    }
}
