//! Plan 288 RouterInfo and ClientServicesInfo inspection plane.
//!
//! This module adapts the runtime-neutral [`i2pr_i2pcontrol`] contract to
//! live router state through narrow inspection handles. Control consumers
//! see point-in-time bounded snapshots or typed commands; they never
//! receive mutable subsystem internals.
//!
//! Handle model: [`InspectionHandles`] carries static configuration truth
//! (network id, service enablement/binds, startup service inventory) plus
//! publish-gated slots for dynamic snapshots. Each live owner publishes
//! its own snapshot exactly once it owns the data; until publication the
//! corresponding selectors fail the whole request explicitly instead of
//! fabricating zero/false/empty values. Service enablement is
//! authoritative configuration truth, so every [`ClientService`] row
//! answers even when its service is disabled.
//!
//! Privacy rules enforced here:
//! - router hashes are public I2P base64 strings supplied by the
//!   bootstrap identity owner, never derived from secrets at request
//!   time;
//! - SAM sessions report bounded non-secret session ids and counts only:
//!   no private destinations, no socket peer addresses (stricter than
//!   the adopted i2pd behavior, which emits socket endpoints);
//! - I2PTunnel entries report names, kinds, enablement, and binds only:
//!   address-book addresses arrive with Plan 294;
//! - `BOB` is a deliberate constant, not a missing-handler fallback.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use i2pr_i2pcontrol::{
    ClientService, PROPOSAL_ROUTER_INFO_FIELDS, ProposalRouterInfoField, ROUTER_INFO_SOURCE_MATRIX,
    RouterInfoSelector, SourceAvailability, service_row, source_row,
};
use i2pr_runtime::Ssu2RuntimeService;

use crate::control_sources::{ControlMetrics, LogRing};
use crate::i2cp::I2cpServiceState;
use crate::sam::SamServiceState;
use crate::service_tunnels::ServiceTunnelManager;

/// Control-plane API version declared by `router.api_version`.
pub const ROUTER_API_VERSION: u64 = 1;

/// Liveness string reported by `router.status` while dispatch executes.
pub const ROUTER_STATUS_RUNNING: &str = "running";

/// Neutral clock-skew value reported by `network.clock_skew`.
///
/// The Proposal permits `null` (no peers to average); the integer
/// wire encodes the unmeasured state as `0` with the Plan 295
/// justification recorded in the source matrix (no peer clocks are
/// ever observed, so this is a declared neutral, never a
/// measurement).
pub const CLOCK_SKEW_NEUTRAL: i64 = 0;

/// Router version string reported by `router.version`: this router's own
/// crate version, never another router's release string.
pub const ROUTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum entries in one published hash/peer list.
pub const MAX_INSPECTION_LIST: usize = 1024;

/// Maximum bytes of a published mode/state string.
pub const MAX_INSPECTION_STATE_STRING: usize = 32;
/// Maximum base64 bytes retained for a local serialized RouterInfo.
pub const MAX_LOCAL_ROUTER_INFO_BASE64_BYTES: usize = 1_048_576;
/// Binary ceiling that keeps base64 expansion within the serialized value cap.
pub const MAX_LOCAL_ROUTER_INFO_BYTES: usize = 786_432;

/// Maximum bytes of a published router-hash string (44-char I2P base64).
pub const MAX_INSPECTION_HASH_STRING: usize = 64;

/// Floodfill mode declaration for `netdb.floodfill_mode`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillMode {
    /// Router does not serve floodfill duty.
    Disabled,
    /// Router serves floodfill duty.
    Floodfill,
}

impl FloodfillMode {
    /// Canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Floodfill => "floodfill",
        }
    }
}

/// Publish-gated dynamic snapshots behind one lock.
#[derive(Clone, Debug, Default)]
struct PublishedSnapshots {
    /// Published local router hash (I2P base64) or `None`.
    router_hash: Option<String>,
    /// Published local serialized RouterInfo (I2P base64) or `None`.
    local_router_info_b64: Option<String>,
    /// Published known-peer hashes or `None`.
    netdb_known: Option<Vec<String>>,
    /// Published active-peer hashes or `None`.
    netdb_active: Option<Vec<String>>,
    /// Published floodfill mode or `None`.
    floodfill_mode: Option<FloodfillMode>,
    /// Published NTCP2 peer identifiers or `None`.
    ntcp2_peers: Option<Vec<String>>,
    /// Published SSU2 session identifiers or `None`.
    ssu2_sessions: Option<Vec<String>>,
    /// Published reachability declaration or `None`.
    reachability: Option<String>,
    /// Published transport error entries or `None`.
    transport_errors: Option<Vec<String>>,
    /// Published exploratory count or `None`.
    exploratory: Option<u64>,
    /// Published client-tunnel count or `None`.
    client_tunnels: Option<u64>,
    /// Published participating count or `None`.
    participating: Option<u64>,
    /// Published build-queue depth or `None`.
    build_queue: Option<u64>,
    /// Published Tunnel Build Message queue depth or `None`.
    tbm_queue: Option<u64>,
    /// Published authoritative ban set or `None` (Plan 295 explicit
    /// ban owner; empty means no bans, never an unowned guess).
    bans: Option<Vec<String>>,
    /// Configured active-link admission limit for NTCP2.
    ntcp2_connection_limit: Option<u64>,
    /// Configured active-session admission limit for SSU2.
    ssu2_connection_limit: Option<u64>,
}

/// Rejection of an over-ceiling or malformed publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishError {
    /// A list exceeded [`MAX_INSPECTION_LIST`].
    ListOverBound {
        /// Selector the list belongs to.
        key: &'static str,
    },
    /// A state string exceeded its ceiling.
    StringOverBound {
        /// Selector the string belongs to.
        key: &'static str,
    },
    /// A router hash was not 44-char I2P base64.
    MalformedHash,
    /// A serialized RouterInfo base64 string was malformed.
    MalformedRouterInfo,
}

impl core::fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ListOverBound { key } => {
                write!(formatter, "inspection list over ceiling for {key}")
            }
            Self::StringOverBound { key } => {
                write!(formatter, "inspection string over ceiling for {key}")
            }
            Self::MalformedHash => write!(formatter, "inspection router hash malformed"),
            Self::MalformedRouterInfo => {
                write!(formatter, "inspection local RouterInfo base64 malformed")
            }
        }
    }
}

/// One startup-owned service entry (inspectable, never mutated here).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupServiceEntry {
    /// Service name from static configuration.
    pub name: String,
    /// Canonical kind spelling (`generic-client`, ...).
    pub kind: &'static str,
    /// `true` for client-side kinds (loopback listener).
    pub client_side: bool,
    /// Whether the static entry is enabled.
    pub enabled: bool,
    /// Configured loopback listener, when the kind carries one.
    pub listener: Option<SocketAddr>,
}

/// Static service endpoint truth (enabled + bind) for SAM/I2CP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceEndpoint {
    /// Whether the bridge listener is enabled.
    pub enabled: bool,
    /// Loopback bind address (`None` when disabled).
    pub bind: Option<SocketAddr>,
}

/// Narrow inspection handles for the control plane.
///
/// Static truth is fixed at construction from validated configuration;
/// dynamic truth arrives through the `publish_*` methods called by the
/// owning services. All locks degrade to empty/unpublished on poisoning:
/// inspection never panics and never fabricates.
pub struct InspectionHandles {
    /// Validated network id.
    network_id: u16,
    /// SAM static endpoint truth.
    sam_endpoint: ServiceEndpoint,
    /// I2CP static endpoint truth.
    i2cp_endpoint: ServiceEndpoint,
    /// Startup-owned service inventory in configuration order.
    startup_services: Vec<StartupServiceEntry>,
    /// Publish-gated dynamic snapshots.
    published: Mutex<PublishedSnapshots>,
    /// Live SAM state published by the SAM factory, when running.
    sam_live: Mutex<Option<Arc<SamServiceState>>>,
    /// Live I2CP state published by the I2CP factory, when running.
    i2cp_live: Mutex<Option<Arc<I2cpServiceState>>>,
    /// Live service-tunnel manager published by its owner, when present.
    manager_live: Mutex<Option<Arc<ServiceTunnelManager>>>,
    /// Canonical address-book resolver cell published by the subsystem
    /// owner, when active (Plan 294).
    addressbook_live: Mutex<crate::addressbook::SharedAddressBook>,
    /// Bounded log ring published by the composition root (Plan 295);
    /// `logs.recent` reads it per request.
    log_live: Mutex<Option<Arc<LogRing>>>,
    /// Rolling control metrics published by the composition root
    /// (Plan 295); success, bandwidth, and rate rows tick and read it
    /// per request.
    metrics_live: Mutex<Option<Arc<ControlMetrics>>>,
    /// Cloned SSU2 runtime service published by the SSU2 factory when
    /// the service is registered (Plan 295); session and error rows
    /// read its cheap snapshot accessors per request.
    ssu2_live: Mutex<Option<Ssu2RuntimeService>>,
    /// Authenticated router NEWS owner, published by the daemon worker.
    news_live: Mutex<Option<Arc<crate::news::NewsManager>>>,
}

impl InspectionHandles {
    /// Builds static truth from validated configuration pieces.
    pub fn new(
        network_id: u16,
        sam_endpoint: ServiceEndpoint,
        i2cp_endpoint: ServiceEndpoint,
        startup_services: Vec<StartupServiceEntry>,
    ) -> Self {
        Self {
            network_id,
            sam_endpoint,
            i2cp_endpoint,
            startup_services,
            published: Mutex::new(PublishedSnapshots::default()),
            sam_live: Mutex::new(None),
            i2cp_live: Mutex::new(None),
            manager_live: Mutex::new(None),
            addressbook_live: Mutex::new(crate::addressbook::SharedAddressBook::new()),
            log_live: Mutex::new(None),
            metrics_live: Mutex::new(None),
            ssu2_live: Mutex::new(None),
            news_live: Mutex::new(None),
        }
    }

    /// Builds handles from the validated daemon configuration.
    ///
    /// Static truth only: service enablement/binds come from the SAM,
    /// I2CP, and service-tunnel blocks; the startup service inventory
    /// mirrors the configured service set in order. No live
    /// owner has published yet.
    pub fn from_config(config: &crate::config::Config) -> Self {
        let sam_endpoint = ServiceEndpoint {
            enabled: config.sam.enabled,
            bind: config.sam.enabled.then(|| config.sam.bind_socket()),
        };
        let i2cp_endpoint = ServiceEndpoint {
            enabled: config.i2cp.enabled,
            bind: config.i2cp.enabled.then(|| config.i2cp.bind_socket()),
        };
        let startup_services = config
            .service_tunnels
            .tunnels
            .tunnels
            .iter()
            .map(|spec| StartupServiceEntry {
                name: spec.id.as_str().to_owned(),
                kind: spec.kind.as_str(),
                client_side: spec.kind.is_client(),
                enabled: spec.enabled,
                listener: spec.listener.map(|listener| listener.socket()),
            })
            .collect();
        let handles = Self::new(
            config.network.network_id,
            sam_endpoint,
            i2cp_endpoint,
            startup_services,
        );
        handles.publish_connection_limits(
            config.transport.ntcp2.max_active_links,
            config.ssu2.max_active_sessions,
        );
        handles
    }

    /// Publishes the validated transport active-connection ceilings.
    ///
    /// These values describe configured admission limits, not current
    /// connection counts or advertised protocol support.
    fn publish_connection_limits(&self, ntcp2: usize, ssu2: usize) {
        if let Ok(mut published) = self.published.lock() {
            published.ntcp2_connection_limit = u64::try_from(ntcp2).ok();
            published.ssu2_connection_limit = u64::try_from(ssu2).ok();
        }
    }

    /// Returns the configured transport active-connection limit for a
    /// canonical Proposal RouterInfo selector.
    pub(crate) fn proposal_connection_limit(&self, key: &str) -> Option<u64> {
        let snapshots = self.snapshots();
        match key {
            "i2p.router.netdb.ntcp.limit" => snapshots.ntcp2_connection_limit,
            "i2p.router.netdb.ssu.limit" => snapshots.ssu2_connection_limit,
            _ => None,
        }
    }

    /// Validated network id.
    pub fn network_id(&self) -> u16 {
        self.network_id
    }

    /// Startup-owned service inventory in configuration order.
    pub fn startup_services(&self) -> &[StartupServiceEntry] {
        &self.startup_services
    }

    /// Publishes the local router hash (44-char I2P base64).
    pub fn publish_router_hash(&self, hash_b64: &str) -> Result<(), PublishError> {
        if hash_b64.len() != 44
            || hash_b64.len() > MAX_INSPECTION_HASH_STRING
            || !hash_b64
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'~')
        {
            return Err(PublishError::MalformedRouterInfo);
        }
        if let Ok(mut published) = self.published.lock() {
            published.router_hash = Some(hash_b64.to_owned());
        }
        Ok(())
    }

    /// Publishes a bounded I2P-base64 local RouterInfo string.
    pub fn publish_local_router_info_b64(&self, info_b64: &str) -> Result<(), PublishError> {
        if info_b64.is_empty() || info_b64.len() > MAX_LOCAL_ROUTER_INFO_BASE64_BYTES {
            return Err(PublishError::StringOverBound {
                key: "i2p.router.info",
            });
        }
        if !info_b64
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'~')
        {
            return Err(PublishError::MalformedHash);
        }
        if let Ok(mut published) = self.published.lock() {
            published.local_router_info_b64 = Some(info_b64.to_owned());
        }
        Ok(())
    }

    /// Publishes bounded NetDB peer views.
    pub fn publish_netdb(
        &self,
        known: Vec<String>,
        active: Vec<String>,
        floodfill_mode: FloodfillMode,
    ) -> Result<(), PublishError> {
        check_list("netdb.known_peers", &known)?;
        check_list("netdb.active_peers", &active)?;
        if let Ok(mut published) = self.published.lock() {
            published.netdb_known = Some(known);
            published.netdb_active = Some(active);
            published.floodfill_mode = Some(floodfill_mode);
        }
        Ok(())
    }

    /// Publishes bounded transport views.
    ///
    /// Plan 295 serves clock skew as the neutral constant instead, so
    /// this call carries no clock observation.
    #[allow(clippy::too_many_arguments)]
    pub fn publish_transport(
        &self,
        ntcp2_peers: Vec<String>,
        ssu2_sessions: Vec<String>,
        reachability: &str,
        errors: Vec<String>,
    ) -> Result<(), PublishError> {
        check_list("transport.ntcp2.active_peers", &ntcp2_peers)?;
        check_list("transport.ssu2.active_sessions", &ssu2_sessions)?;
        check_list("transport.errors", &errors)?;
        check_state_string("transport.reachability", reachability)?;
        if let Ok(mut published) = self.published.lock() {
            published.ntcp2_peers = Some(ntcp2_peers);
            published.ssu2_sessions = Some(ssu2_sessions);
            published.reachability = Some(reachability.to_owned());
            published.transport_errors = Some(errors);
        }
        Ok(())
    }

    /// Publishes bounded tunnel-count views.
    ///
    /// Success, bandwidth, and rate rows read the live Plan 295
    /// metrics owner instead, so this call carries counts only.
    pub fn publish_tunnels(
        &self,
        exploratory: u64,
        client_tunnels: u64,
        participating: u64,
        build_queue: u64,
    ) -> Result<(), PublishError> {
        if let Ok(mut published) = self.published.lock() {
            published.exploratory = Some(exploratory);
            published.client_tunnels = Some(client_tunnels);
            published.participating = Some(participating);
            published.build_queue = Some(build_queue);
        }
        Ok(())
    }

    /// Publishes the independently observed Tunnel Build Message queue depth.
    pub fn publish_tbm_queue(&self, depth: u64) {
        if let Ok(mut published) = self.published.lock() {
            published.tbm_queue = Some(depth);
        }
    }

    /// Publishes the authoritative ban set from the explicit ban
    /// owner (empty means no bans exist, never an unowned guess).
    pub fn publish_bans(&self, banned: Vec<String>) -> Result<(), PublishError> {
        check_list("network.banned_peers", &banned)?;
        if let Ok(mut published) = self.published.lock() {
            published.bans = Some(banned);
        }
        Ok(())
    }

    /// Publishes the bounded log ring (called once by the composition
    /// root; the ring keeps feeding from the tracing layer).
    pub fn publish_log_ring(&self, ring: Arc<LogRing>) {
        if let Ok(mut live) = self.log_live.lock() {
            *live = Some(ring);
        }
    }

    /// Clears the retained redacted ring through its owning lock.
    /// Ordinary tracing output and cumulative eviction diagnostics are
    /// unaffected. `None` means the owner was not published.
    pub fn clear_logs(&self) -> Option<bool> {
        self.log_live
            .lock()
            .ok()
            .and_then(|live| live.clone())
            .map(|ring| ring.clear())
    }

    /// Returns the bounded redacted log lines in chronological order.
    /// The Proposal field exposes only the string list; internal drop
    /// diagnostics remain available through the normalized owner view.
    pub fn recent_logs(&self) -> Option<Vec<String>> {
        self.log_live
            .lock()
            .ok()
            .and_then(|live| live.clone())
            .map(|ring| ring.snapshot().0.iter().map(|line| line.wire()).collect())
    }

    /// Publishes the rolling control metrics (called once by the
    /// composition root).
    pub fn publish_metrics(&self, metrics: Arc<ControlMetrics>) {
        if let Ok(mut live) = self.metrics_live.lock() {
            *live = Some(metrics);
        }
    }

    pub(crate) fn publish_news_manager(&self, manager: Arc<crate::news::NewsManager>) {
        if let Ok(mut slot) = self.news_live.lock() {
            *slot = Some(manager);
        }
    }

    pub(crate) fn proposal_news(&self, now_unix: u64) -> Option<crate::news::NewsSnapshot> {
        self.news_live
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|manager| manager.snapshot(now_unix)))
    }

    /// Publishes the cloned SSU2 runtime service (called by the SSU2
    /// factory when the service registers; cheap `Arc`-backed clone).
    pub fn publish_ssu2(&self, service: Ssu2RuntimeService) {
        if let Ok(mut live) = self.ssu2_live.lock() {
            *live = Some(service);
        }
    }

    /// Publishes the live SAM state (called once by the SAM factory).
    pub fn publish_sam(&self, state: Arc<SamServiceState>) {
        if let Ok(mut live) = self.sam_live.lock() {
            *live = Some(state);
        }
    }

    /// Publishes the live I2CP state (called once by the I2CP factory).
    pub fn publish_i2cp(&self, state: Arc<I2cpServiceState>) {
        if let Ok(mut live) = self.i2cp_live.lock() {
            *live = Some(state);
        }
    }

    /// Publishes the live service-tunnel manager (called by its owner).
    pub fn publish_manager(&self, manager: Arc<ServiceTunnelManager>) {
        if let Ok(mut live) = self.manager_live.lock() {
            *live = Some(manager);
        }
    }

    /// Installs the canonical address-book resolver cell (called once
    /// by the composition root after subsystem activation; the cell
    /// shares one `Arc` with the subsystem, so commits propagate).
    pub fn publish_addressbook(&self, handle: crate::addressbook::SharedAddressBook) {
        if let Ok(mut live) = self.addressbook_live.lock() {
            *live = handle;
        }
    }

    /// Reads the published snapshots without blocking.
    fn snapshots(&self) -> PublishedSnapshots {
        self.published
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

impl core::fmt::Debug for InspectionHandles {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let snapshots = self.snapshots();
        formatter
            .debug_struct("InspectionHandles")
            .field("network_id", &self.network_id)
            .field("sam_endpoint", &self.sam_endpoint)
            .field("i2cp_endpoint", &self.i2cp_endpoint)
            .field("startup_services", &self.startup_services.len())
            .field("router_hash_published", &snapshots.router_hash.is_some())
            .field("netdb_published", &snapshots.netdb_known.is_some())
            .field("transport_published", &snapshots.ntcp2_peers.is_some())
            .field("tunnels_published", &snapshots.exploratory.is_some())
            .field("bans_published", &snapshots.bans.is_some())
            .field(
                "log_live",
                &self
                    .log_live
                    .lock()
                    .map(|live| live.is_some())
                    .unwrap_or(false),
            )
            .field(
                "metrics_live",
                &self
                    .metrics_live
                    .lock()
                    .map(|live| live.is_some())
                    .unwrap_or(false),
            )
            .field(
                "ssu2_live",
                &self
                    .ssu2_live
                    .lock()
                    .map(|live| live.is_some())
                    .unwrap_or(false),
            )
            .field(
                "sam_live",
                &self
                    .sam_live
                    .lock()
                    .map(|live| live.is_some())
                    .unwrap_or(false),
            )
            .field(
                "i2cp_live",
                &self
                    .i2cp_live
                    .lock()
                    .map(|live| live.is_some())
                    .unwrap_or(false),
            )
            .field(
                "manager_live",
                &self
                    .manager_live
                    .lock()
                    .map(|live| live.is_some())
                    .unwrap_or(false),
            )
            .finish()
    }
}

/// Rejects an over-ceiling publication list.
fn check_list(key: &'static str, values: &[String]) -> Result<(), PublishError> {
    if values.len() > MAX_INSPECTION_LIST {
        return Err(PublishError::ListOverBound { key });
    }
    Ok(())
}

/// Maximum entries rendered for one address-book book row (matches
/// the committed book ceiling).
const MAX_ADDRESSBOOK_BOOK_ITEMS: usize = 1000;
/// Maximum serialized bytes rendered for one address-book book row.
const MAX_ADDRESSBOOK_BOOK_BYTES: usize = 4_500_000;
/// Maximum URLs rendered for the subscriptions row.
const MAX_ADDRESSBOOK_SUBSCRIPTION_ITEMS: usize = 16;
/// Maximum serialized bytes rendered for the subscriptions row.
const MAX_ADDRESSBOOK_SUBSCRIPTION_BYTES: usize = 65_536;

/// Reads the published address-book snapshot or gaps with the Plan
/// 294 owner (uninstalled, inactive, or never published).
fn addressbook_cells(
    handles: &InspectionHandles,
    row: &i2pr_i2pcontrol::SourceRow,
) -> Result<i2pr_addressbook::AddressBookSnapshot, InspectionGap> {
    handles
        .addressbook_live
        .lock()
        .ok()
        .and_then(|slot| slot.snapshot_cloned())
        .ok_or(InspectionGap {
            key: row.key,
            owner_plan: "294",
            owner: "canonical AddressBook",
        })
}

/// Renders one address-book book as a hostname-to-destination map.
/// Collection ceilings re-check defensively at serialization: a
/// violation gaps (never truncation, never fabrication).
fn addressbook_book_value(
    handles: &InspectionHandles,
    index: usize,
    row: &i2pr_i2pcontrol::SourceRow,
) -> Result<serde_json::Value, InspectionGap> {
    let internal = || InspectionGap {
        key: row.key,
        owner_plan: "294",
        owner: "canonical AddressBook",
    };
    let snapshot = addressbook_cells(handles, row)?;
    let entries = snapshot.book_entries(index).ok_or_else(internal)?;
    if entries.len() > MAX_ADDRESSBOOK_BOOK_ITEMS {
        return Err(internal());
    }
    let mut map = serde_json::Map::with_capacity(entries.len());
    for (name, destination) in entries {
        map.insert(
            name.as_str().to_owned(),
            serde_json::Value::String(destination.clone()),
        );
    }
    let value = serde_json::Value::Object(map);
    if serde_json::to_vec(&value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > MAX_ADDRESSBOOK_BOOK_BYTES
    {
        return Err(internal());
    }
    Ok(value)
}

/// Renders the committed subscription set as a URL list object.
fn addressbook_subscriptions_value(
    handles: &InspectionHandles,
    row: &i2pr_i2pcontrol::SourceRow,
) -> Result<serde_json::Value, InspectionGap> {
    let internal = || InspectionGap {
        key: row.key,
        owner_plan: "294",
        owner: "canonical AddressBook",
    };
    let snapshot = addressbook_cells(handles, row)?;
    if snapshot.subscription_urls().len() > MAX_ADDRESSBOOK_SUBSCRIPTION_ITEMS {
        return Err(internal());
    }
    let value = serde_json::json!({ "urls": snapshot.subscription_urls() });
    if serde_json::to_vec(&value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > MAX_ADDRESSBOOK_SUBSCRIPTION_BYTES
    {
        return Err(internal());
    }
    Ok(value)
}

/// Renders the committed canonical Proposal 170 thirteen-key map.
fn addressbook_config_value(
    handles: &InspectionHandles,
    row: &i2pr_i2pcontrol::SourceRow,
) -> Result<serde_json::Value, InspectionGap> {
    let snapshot = addressbook_cells(handles, row)?;
    let internal = snapshot.config_entries();
    let mapping = [
        ("subscriptions", "subscriptions"),
        ("update_delay", "refresh_interval"),
        ("published_addressbook", "published_book"),
        ("router_addressbook", "router_book"),
        ("local_addressbook", "local_book"),
        ("private_addressbook", "private_book"),
        ("proxy_port", "proxy_port"),
        ("proxy_host", "proxy_host"),
        ("should_publish", "should_publish"),
        ("etags", "etags"),
        ("last_modified", "last_modified"),
        ("log", "log_file"),
        ("theme", "theme"),
    ];
    let mut map = serde_json::Map::with_capacity(mapping.len());
    for (wire, owner) in mapping {
        if let Some(value) = internal.get(owner) {
            map.insert(wire.to_owned(), serde_json::Value::String(value.clone()));
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// Serializes the six canonical Proposal 170 AddressBook RouterInfo
/// fields from the same committed snapshot used by normal resolution.
pub(crate) fn proposal_addressbook_value(
    key: &'static str,
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    use RouterInfoSelector as Selector;

    let (selector, book_index) = match key {
        "i2p.router.addressbook.private.list" => (Selector::AddressBookPrivate, Some(0)),
        "i2p.router.addressbook.local.list" => (Selector::AddressBookLocal, Some(1)),
        "i2p.router.addressbook.router.list" => (Selector::AddressBookRouter, Some(2)),
        "i2p.router.addressbook.published.list" => (Selector::AddressBookPublished, Some(3)),
        "i2p.router.addressbook.subscriptions" => (Selector::AddressBookSubscriptions, None),
        "i2p.router.addressbook.config" => (Selector::AddressBookConfig, None),
        _ => {
            return Err(InspectionGap {
                key,
                owner_plan: "322",
                owner: "Proposal AddressBook source adapter",
            });
        }
    };
    let mut row = source_row(selector);
    row.key = key;
    let snapshot = addressbook_cells(handles, &row)?;
    let value = if let Some(index) = book_index {
        let entries = snapshot.book_entries(index).ok_or(InspectionGap {
            key,
            owner_plan: "321",
            owner: "canonical AddressBook",
        })?;
        if entries.len() > MAX_ADDRESSBOOK_BOOK_ITEMS {
            return Err(InspectionGap {
                key,
                owner_plan: "321",
                owner: "canonical AddressBook",
            });
        }
        let rows: Vec<serde_json::Value> = entries
            .iter()
            .map(|(hostname, destination)| {
                serde_json::json!({
                    "hostname": hostname.as_str(),
                    "destination": destination,
                })
            })
            .collect();
        serde_json::Value::Array(rows)
    } else if key == "i2p.router.addressbook.subscriptions" {
        let config = snapshot.config_entries();
        let path = config.get("subscriptions").ok_or(InspectionGap {
            key,
            owner_plan: "321",
            owner: "canonical AddressBook configuration",
        })?;
        serde_json::json!({
            "path": path,
            "entries": snapshot.subscription_urls(),
        })
    } else {
        serde_json::json!({
            "path": i2pr_storage::ADDRESSBOOK_CURRENT_FILE_NAME,
            "entries": addressbook_config_value(handles, &row)?,
        })
    };
    if serde_json::to_vec(&value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > MAX_ADDRESSBOOK_BOOK_BYTES
    {
        return Err(InspectionGap {
            key,
            owner_plan: "321",
            owner: "canonical AddressBook",
        });
    }
    Ok(value)
}

/// Reads canonical cumulative SSU2 byte counters through the metrics
/// owner. Values are unavailable until a transport owner publishes a
/// sample; loopback and destination traffic are outside this counter's
/// documented coverage.
pub(crate) fn proposal_transport_total(
    key: &'static str,
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let received_field = match key {
        "i2p.router.net.total.received.bytes" => true,
        "i2p.router.net.total.sent.bytes" => false,
        _ => {
            return Err(InspectionGap {
                key,
                owner_plan: "322",
                owner: "Proposal transport counters",
            });
        }
    };
    let Some((received, sent)) =
        metrics_owner(handles).and_then(|metrics| metrics.transport_totals())
    else {
        return Err(InspectionGap {
            key,
            owner_plan: "322",
            owner: "SSU2 cumulative transport counters",
        });
    };
    let value = if received_field { received } else { sent };
    Ok(serde_json::Value::from(value))
}

/// Reads the cumulative tunnel build success ratio from the existing
/// metrics owner. A ratio is undefined until at least one build attempt
/// has been observed, so the unobserved state stays unavailable.
pub(crate) fn proposal_tunnel_success_rate(
    key: &'static str,
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let recent = match key {
        "i2p.router.net.tunnels.successrate" => true,
        "i2p.router.net.tunnels.totalsuccessrate" => false,
        _ => {
            return Err(InspectionGap {
                key,
                owner_plan: "322",
                owner: "ControlMetrics tunnel-build outcomes",
            });
        }
    };
    let Some((recent_rate, total_rate)) =
        metrics_owner(handles).map(|metrics| metrics.success_rates())
    else {
        return Err(InspectionGap {
            key,
            owner_plan: "322",
            owner: "ControlMetrics tunnel-build outcomes",
        });
    };
    match if recent { recent_rate } else { total_rate } {
        Some(rate) => Ok(serde_json::Value::from(rate)),
        None => Err(InspectionGap {
            key,
            owner_plan: "322",
            owner: "ControlMetrics tunnel-build outcomes",
        }),
    }
}

/// Reads the canonical scalar build-queue depth from the existing
/// attested Plan 295 snapshot. The legacy selector's one-element list
/// representation is an internal compatibility shape only.
pub(crate) fn proposal_tunnel_queue_depth(
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let value = router_info_result(RouterInfoSelector::BuildQueue, handles, 0)?;
    value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_u64)
        .map(serde_json::Value::from)
        .ok_or(InspectionGap {
            key: "i2p.router.net.tunnels.queue",
            owner_plan: "322",
            owner: "attested tunnel build-queue snapshot",
        })
}

/// Returns the published local RouterInfo as Proposal base64, or null
/// while bootstrap has not published a local record.
pub(crate) fn proposal_local_router_info(handles: &InspectionHandles) -> serde_json::Value {
    handles
        .snapshots()
        .local_router_info_b64
        .map(serde_json::Value::String)
        .unwrap_or(serde_json::Value::Null)
}

/// Reads the canonical Tunnel Build Message queue depth from its own
/// attested snapshot; it is not conflated with the tunnel-request queue.
pub(crate) fn proposal_tbm_queue_depth(
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    handles
        .snapshots()
        .tbm_queue
        .map(serde_json::Value::from)
        .ok_or(InspectionGap {
            key: "i2p.router.net.tunnels.tbmqueue",
            owner_plan: "322",
            owner: "attested Tunnel Build Message queue snapshot",
        })
}

/// Serialized RouterInfo lists are empty exactly when their attested
/// peer-hash source is empty. A populated hash snapshot requires a
/// separate serialized RouterInfo owner and therefore fails closed.
pub(crate) fn proposal_empty_router_info_list(
    key: &'static str,
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let selector = match key {
        "i2p.router.netdb.activepeers.info" | "i2p.router.netdb.activepeers.stats" => {
            RouterInfoSelector::NetDbActivePeers
        }
        "i2p.router.netdb.peers.info" => RouterInfoSelector::NetDbKnownPeers,
        _ => {
            return Err(InspectionGap {
                key,
                owner_plan: "322",
                owner: "attested NetDB peer snapshot",
            });
        }
    };
    let gap = InspectionGap {
        key,
        owner_plan: "322",
        owner: "serialized RouterInfo snapshot for known peers",
    };
    let peers = router_info_result(selector, handles, 0).map_err(|_| gap)?;
    if peers.as_array().is_some_and(Vec::is_empty) {
        Ok(serde_json::Value::Array(Vec::new()))
    } else {
        Err(gap)
    }
}

/// Projects an empty banned-peer detail map only when the ban ledger's
/// attested owner reports no entries. Populated hashes need reason and
/// expiry details that the current owner does not retain.
pub(crate) fn proposal_empty_banned_peer_details(
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let key = "i2p.router.netdb.bannedpeers";
    match handles.snapshots().bans {
        Some(bans) if bans.is_empty() => Ok(serde_json::Value::Object(serde_json::Map::new())),
        _ => Err(InspectionGap {
            key,
            owner_plan: "322",
            owner: "ban reason and expiry detail snapshot",
        }),
    }
}

/// Projects canonical tunnel direction counts and detail lists only
/// when the attested aggregate owner reports no tunnels. A nonzero
/// aggregate needs per-direction or per-tunnel data that this graph
/// does not maintain, so those cases remain fail-closed.
pub(crate) fn proposal_empty_tunnel_projection(
    key: &'static str,
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let (selector, list) = match key {
        "i2p.router.net.tunnels.exploratory.inbound"
        | "i2p.router.net.tunnels.exploratory.outbound" => {
            (RouterInfoSelector::ExploratoryCount, false)
        }
        "i2p.router.net.tunnels.exploratory.info.list" => {
            (RouterInfoSelector::ExploratoryCount, true)
        }
        "i2p.router.net.tunnels.client.inbound" | "i2p.router.net.tunnels.client.outbound" => {
            (RouterInfoSelector::ClientCount, false)
        }
        "i2p.router.net.tunnels.client.info.list" => (RouterInfoSelector::ClientCount, true),
        "i2p.router.net.tunnels.participating.info" => {
            (RouterInfoSelector::ParticipatingCount, true)
        }
        _ => {
            return Err(InspectionGap {
                key,
                owner_plan: "322",
                owner: "attested tunnel count snapshot",
            });
        }
    };
    let gap = InspectionGap {
        key,
        owner_plan: "322",
        owner: "per-direction and per-tunnel inspection snapshot",
    };
    let value = router_info_result(selector, handles, 0).map_err(|_| gap)?;
    let count = value
        .as_array()
        .and_then(|items| items.first())
        .and_then(serde_json::Value::as_u64)
        .ok_or(gap)?;
    if count != 0 {
        return Err(gap);
    }
    Ok(if list {
        serde_json::Value::Array(Vec::new())
    } else {
        serde_json::Value::from(0)
    })
}

/// Rejects an over-ceiling publication string.
fn check_state_string(key: &'static str, value: &str) -> Result<(), PublishError> {
    if value.is_empty() || value.len() > MAX_INSPECTION_STATE_STRING {
        return Err(PublishError::StringOverBound { key });
    }
    Ok(())
}

/// Whole-request failure for one ungated selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectionGap {
    /// Requested wire key.
    pub key: &'static str,
    /// Plan that owns a truthful source.
    pub owner_plan: &'static str,
    /// Owner subsystem that must publish.
    pub owner: &'static str,
}

impl InspectionGap {
    /// Exact `-32603` message: names the key, the owning plan, and the
    /// missing owner. Frozen by `plan288_gap_messages_name_owner_and_plan`.
    pub fn message(self) -> String {
        format!(
            "{} not available (Plan {}: {} unpublished)",
            self.key, self.owner_plan, self.owner
        )
    }
}

/// Select-form rejection before dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectError {
    /// An unknown key (including every `i2p.*` base-compatibility key,
    /// which is structurally disjoint from the Proposal vocabulary).
    UnknownKey(String),
}

/// Validates the canonical RouterInfo select form. Selector
/// values carry no meaning; presence selects the field, including empty,
/// null, and benign non-null values. Proposal additions retain proposal
/// order; base API fields follow them in their own frozen order.
pub fn select_router_info(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<&'static ProposalRouterInfoField>, SelectError> {
    let mut selected = Vec::new();
    for key in params.keys() {
        if key == "Token" {
            continue;
        }
        let field = i2pr_i2pcontrol::router_info_field(key)
            .ok_or_else(|| SelectError::UnknownKey(truncated_key(key)))?;
        if !selected
            .iter()
            .any(|prior: &&ProposalRouterInfoField| prior.key == field.key)
        {
            selected.push(field);
        }
    }
    selected.sort_by_key(|field| {
        PROPOSAL_ROUTER_INFO_FIELDS
            .iter()
            .position(|candidate| candidate.key == field.key)
            .or_else(|| {
                i2pr_i2pcontrol::BASE_ROUTER_INFO_FIELDS
                    .iter()
                    .position(|candidate| candidate.key == field.key)
                    .map(|position| PROPOSAL_ROUTER_INFO_FIELDS.len() + position)
            })
            .unwrap_or(usize::MAX)
    });
    Ok(selected)
}

/// Validates the ClientServicesInfo select form with the same rules.
pub fn select_client_services(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<ClientService>, SelectError> {
    let mut selected = Vec::new();
    for key in params.keys() {
        if key == "Token" {
            continue;
        }
        let service =
            ClientService::parse(key).map_err(|_| SelectError::UnknownKey(truncated_key(key)))?;
        if !selected.contains(&service) {
            selected.push(service);
        }
    }
    selected.sort_by_key(|service| i2pr_i2pcontrol::service_index(*service));
    Ok(selected)
}

/// Truncates an unknown key for error reporting (bounded diagnostics).
fn truncated_key(key: &str) -> String {
    const MAX_KEY_ECHO: usize = 128;
    if key.len() <= MAX_KEY_ECHO {
        key.to_owned()
    } else {
        key[..MAX_KEY_ECHO].to_owned()
    }
}

/// Serializes one RouterInfo selector against the published snapshots.
///
/// `uptime_secs` is the control-plane uptime in seconds. Collection
/// ceilings were enforced at publication; serialization re-checks
/// lengths defensively and reports an internal gap (never truncation,
/// never fabrication) on violation.
pub fn router_info_result(
    selector: RouterInfoSelector,
    handles: &InspectionHandles,
    uptime_secs: u64,
) -> Result<serde_json::Value, InspectionGap> {
    let snapshots = handles.snapshots();
    let row = source_row(selector);
    let gap = |owner: &'static str, owner_plan: &'static str| InspectionGap {
        key: row.key,
        owner_plan,
        owner,
    };
    match selector {
        RouterInfoSelector::RouterVersion => {
            Ok(serde_json::Value::String(ROUTER_VERSION.to_owned()))
        }
        RouterInfoSelector::RouterApiVersion => Ok(serde_json::Value::from(ROUTER_API_VERSION)),
        RouterInfoSelector::RouterUptime => Ok(serde_json::Value::from(uptime_secs)),
        RouterInfoSelector::RouterStatus => {
            Ok(serde_json::Value::String(ROUTER_STATUS_RUNNING.to_owned()))
        }
        RouterInfoSelector::RouterNetworkId => Ok(serde_json::Value::from(handles.network_id())),
        RouterInfoSelector::RouterHash => snapshots
            .router_hash
            .map(serde_json::Value::String)
            .ok_or_else(|| match row.availability {
                SourceAvailability::PublishedGated { owner, owner_plan } => gap(owner, owner_plan),
                _ => gap("bootstrap identity", "288"),
            }),
        RouterInfoSelector::NetDbKnownPeers => snapshots
            .netdb_known
            .map(|peers| strings_value(&peers))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::NetDbActivePeers => snapshots
            .netdb_active
            .map(|peers| strings_value(&peers))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::NetDbFloodfillMode => snapshots
            .floodfill_mode
            .map(|mode| serde_json::Value::String(mode.as_str().to_owned()))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::Ntcp2ActivePeers => snapshots
            .ntcp2_peers
            .map(|peers| strings_value(&peers))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::Ssu2ActiveSessions => ssu2_session_strings(handles)
            .map(|sessions| strings_value(&sessions))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::Reachability => snapshots
            .reachability
            .map(serde_json::Value::String)
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::TransportErrors => transport_error_strings(handles)
            .map(|errors| strings_value(&errors))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::ExploratoryCount => snapshots
            .exploratory
            .map(count_value)
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::ClientCount => snapshots
            .client_tunnels
            .map(count_value)
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::ParticipatingCount => snapshots
            .participating
            .map(count_value)
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::BuildQueue => snapshots
            .build_queue
            .map(count_value)
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::SuccessRate => metrics_owner(handles)
            .map(|metrics| {
                let (succeeded, attempted) = metrics.success();
                serde_json::json!({"succeeded": succeeded, "attempted": attempted})
            })
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::Bandwidth => metrics_owner(handles)
            .map(|metrics| {
                let (inbound_bps, outbound_bps) = metrics.bandwidth();
                serde_json::json!({"inbound_bps": inbound_bps, "outbound_bps": outbound_bps})
            })
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::AddressBookPrivate => addressbook_book_value(handles, 0, &row),
        RouterInfoSelector::AddressBookLocal => addressbook_book_value(handles, 1, &row),
        RouterInfoSelector::AddressBookRouter => addressbook_book_value(handles, 2, &row),
        RouterInfoSelector::AddressBookPublished => addressbook_book_value(handles, 3, &row),
        RouterInfoSelector::AddressBookSubscriptions => {
            addressbook_subscriptions_value(handles, &row)
        }
        RouterInfoSelector::AddressBookConfig => addressbook_config_value(handles, &row),
        RouterInfoSelector::LogsRecent => handles
            .log_live
            .lock()
            .ok()
            .and_then(|live| live.clone())
            .map(|ring| {
                let (lines, dropped) = ring.snapshot();
                let entries: Vec<serde_json::Value> = lines
                    .iter()
                    .map(|line| serde_json::Value::String(line.wire()))
                    .collect();
                serde_json::json!({"entries": entries, "dropped": dropped})
            })
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::NewsFeed => Err(unavailable_gap(row)),
        RouterInfoSelector::BannedPeers => snapshots
            .bans
            .map(|banned| strings_value(&banned))
            .ok_or_else(|| unpublished_295(&row)),
        RouterInfoSelector::ClockSkew => Ok(serde_json::Value::from(CLOCK_SKEW_NEUTRAL)),
        RouterInfoSelector::Rates => metrics_owner(handles)
            .map(|metrics| {
                let entries: serde_json::Map<String, serde_json::Value> = metrics
                    .rates_snapshot()
                    .into_iter()
                    .map(|(key, value)| (key, serde_json::Value::from(value)))
                    .collect();
                serde_json::Value::Object(entries)
            })
            .ok_or_else(|| unpublished_295(&row)),
    }
}

/// Builds the gap for an unavailable row from the matrix row.
fn unavailable_gap(row: i2pr_i2pcontrol::SourceRow) -> InspectionGap {
    match row.availability {
        SourceAvailability::Unavailable { owner_plan, .. } => InspectionGap {
            key: row.key,
            owner_plan,
            owner: row.owner,
        },
        SourceAvailability::PublishedGated { owner, owner_plan } => InspectionGap {
            key: row.key,
            owner_plan,
            owner,
        },
        _ => InspectionGap {
            key: row.key,
            owner_plan: "288",
            owner: row.owner,
        },
    }
}

/// Builds the gap for a Plan 295 owner-backed row whose owner has not
/// published yet. Composition installs every owner, so production
/// never takes this path; handles built without composition keep the
/// fail-closed slot instead of fabricating.
fn unpublished_295(row: &i2pr_i2pcontrol::SourceRow) -> InspectionGap {
    InspectionGap {
        key: row.key,
        owner_plan: "295",
        owner: row.owner,
    }
}

/// Reads the published rolling metrics, when installed.
///
/// When the SSU2 service registered, its cumulative counters prime
/// the windows first, so bandwidth/rate rows reflect transport
/// traffic. Coverage is exactly the registered counters (local
/// loopback/destination traffic bypassing them is not counted; the
/// dossier records the boundary).
fn metrics_owner(handles: &InspectionHandles) -> Option<Arc<ControlMetrics>> {
    let metrics = handles
        .metrics_live
        .lock()
        .ok()
        .and_then(|live| live.clone())?;
    if let Some(service) = ssu2_service(handles) {
        let snapshot = service.snapshot();
        metrics.observe_transport(
            snapshot.i2np_received,
            snapshot.i2np_sent,
            snapshot.datagrams_received,
            snapshot.datagrams_sent,
        );
    }
    Some(metrics)
}

/// Reads the published SSU2 runtime service, when the SSU2 service
/// registered.
fn ssu2_service(handles: &InspectionHandles) -> Option<Ssu2RuntimeService> {
    handles.ssu2_live.lock().ok().and_then(|live| live.clone())
}

/// Live SSU2 session identifiers (sorted, deduplicated 44-char I2P
/// base64 peer hashes), falling back to the attested static
/// publication when no SSU2 service registered.
fn ssu2_session_strings(handles: &InspectionHandles) -> Option<Vec<String>> {
    if let Some(service) = ssu2_service(handles) {
        let mut sessions: Vec<String> = service
            .active_peer_hashes()
            .iter()
            .filter_map(|hash| i2pr_netdb::encode(hash.as_bytes()).ok())
            .collect();
        sessions.sort();
        sessions.dedup();
        return Some(sessions);
    }
    handles.snapshots().ssu2_sessions
}

/// Non-zero SSU2 error counters as `name=value` entries in counter
/// order, falling back to the attested static publication when no
/// SSU2 service registered.
fn transport_error_strings(handles: &InspectionHandles) -> Option<Vec<String>> {
    if let Some(service) = ssu2_service(handles) {
        let snapshot = service.snapshot();
        let counters = [
            ("ssu2.cheap_drops", snapshot.cheap_drops),
            ("ssu2.auth_failures", snapshot.auth_failures),
            ("ssu2.protocol_drops", snapshot.protocol_drops),
            ("ssu2.token_rejections", snapshot.token_rejections),
            ("ssu2.replay_rejections", snapshot.replay_rejections),
            ("ssu2.handshake_timeouts", snapshot.handshake_timeouts),
            ("ssu2.fault_drops", snapshot.fault_drops),
            ("ssu2.inbound_queue_drops", snapshot.inbound_queue_drops),
            ("ssu2.staging_drops", snapshot.staging_drops),
            ("ssu2.send_without_link", snapshot.send_without_link),
            ("ssu2.path_rejections", snapshot.path_rejections),
            ("ssu2.path_expirations", snapshot.path_expirations),
            ("ssu2.path_denied", snapshot.path_denied),
        ];
        return Some(
            counters
                .into_iter()
                .filter(|(_, count)| *count > 0)
                .map(|(name, count)| format!("{name}={count}"))
                .collect(),
        );
    }
    handles.snapshots().transport_errors
}

/// Serializes a bounded string list (publication enforced the ceiling).
fn strings_value(values: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        values
            .iter()
            .map(|value| serde_json::Value::String(value.clone()))
            .collect(),
    )
}

/// Serializes a count selector as a one-element list carrying the
/// authoritative count (the frozen return type is `List`).
fn count_value(count: u64) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::Value::from(count)])
}

/// Serializes one ClientServicesInfo service. Infallible: disabled or
/// absent services report truthful disabled state.
pub fn client_service_result(
    service: ClientService,
    handles: &InspectionHandles,
) -> serde_json::Value {
    let _row = service_row(service);
    match service {
        ClientService::I2pTunnel => i2ptunnel_value(handles),
        ClientService::HttpProxy => proxy_value(handles, "httpclient"),
        ClientService::Socks => proxy_value(handles, "socks"),
        ClientService::Sam => sam_value(handles),
        ClientService::Bob => serde_json::json!({"enabled": false}),
        ClientService::I2cp => i2cp_value(handles),
    }
}

/// Bounded I2PTunnel map split client/server from the startup inventory
/// plus the live manager overlay. Address-book addresses are omitted
/// until Plan 294; entries carry names, kinds, enablement, running
/// state, and binds only.
fn i2ptunnel_value(handles: &InspectionHandles) -> serde_json::Value {
    let manager = handles
        .manager_live
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let mut client_entries = serde_json::Map::new();
    let mut server_entries = serde_json::Map::new();
    for entry in &handles.startup_services {
        let running = manager
            .as_ref()
            .map(|live| live.has_runtime(&entry.name))
            .unwrap_or(false);
        let bind = manager
            .as_ref()
            .and_then(|live| live.client_listener_address(&entry.name))
            .or(entry.listener)
            .map(|address| address.to_string());
        let mut object = serde_json::Map::new();
        object.insert(
            "kind".to_owned(),
            serde_json::Value::String(entry.kind.to_owned()),
        );
        object.insert("enabled".to_owned(), serde_json::Value::Bool(entry.enabled));
        object.insert("running".to_owned(), serde_json::Value::Bool(running));
        object.insert(
            "bind".to_owned(),
            bind.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        if entry.client_side {
            client_entries.insert(entry.name.clone(), serde_json::Value::Object(object));
        } else {
            server_entries.insert(entry.name.clone(), serde_json::Value::Object(object));
        }
    }
    // Manager-only runtimes (control-owned, Plan 289) appear even
    // without a startup entry.
    if let Some(live) = manager.as_ref() {
        for runtime in live.all_service_runtimes() {
            let name = runtime.spec_id.clone();
            if client_entries.contains_key(&name) || server_entries.contains_key(&name) {
                continue;
            }
            let mut object = serde_json::Map::new();
            object.insert(
                "kind".to_owned(),
                serde_json::Value::String(runtime.kind.as_str().to_owned()),
            );
            object.insert("enabled".to_owned(), serde_json::Value::Bool(true));
            object.insert("running".to_owned(), serde_json::Value::Bool(true));
            let bind = live
                .client_listener_address(&name)
                .map(|address| address.to_string());
            object.insert(
                "bind".to_owned(),
                bind.map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            // Client/server split for manager-only entries reuses the
            // canonical kind side.
            if runtime.kind.is_server() {
                server_entries.insert(name, serde_json::Value::Object(object));
            } else {
                client_entries.insert(name, serde_json::Value::Object(object));
            }
        }
    }
    serde_json::json!({
        "client": client_entries,
        "server": server_entries,
    })
}

/// Proposal RouterInfo quick summaries from the existing bounded
/// I2PTunnel startup inventory and live manager overlay.
pub(crate) fn proposal_i2ptunnel_summaries(
    handles: &InspectionHandles,
) -> Result<serde_json::Value, InspectionGap> {
    let key = "i2p.router.net.tunnels.i2ptunnel";
    let inventory = i2ptunnel_value(handles);
    let mut rows = Vec::new();
    for side in ["client", "server"] {
        let Some(entries) = inventory.get(side).and_then(serde_json::Value::as_object) else {
            return Err(InspectionGap {
                key,
                owner_plan: "289",
                owner: "service tunnel inventory",
            });
        };
        for (name, summary) in entries {
            let Some(fields) = summary.as_object() else {
                return Err(InspectionGap {
                    key,
                    owner_plan: "289",
                    owner: "service tunnel summary",
                });
            };
            let mut row = fields.clone();
            row.insert("name".to_owned(), serde_json::Value::String(name.clone()));
            row.insert(
                "side".to_owned(),
                serde_json::Value::String(side.to_owned()),
            );
            rows.push((name.clone(), serde_json::Value::Object(row)));
        }
    }
    if rows.len() > i2pr_service_tunnels::MAX_SERVICE_TUNNELS * 2 {
        return Err(InspectionGap {
            key,
            owner_plan: "289",
            owner: "service tunnel inventory ceiling",
        });
    }
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let summaries: Vec<serde_json::Value> = rows.into_iter().map(|(_, row)| row).collect();
    let value = serde_json::Value::Array(summaries);
    if serde_json::to_vec(&value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > 65_536
    {
        return Err(InspectionGap {
            key,
            owner_plan: "289",
            owner: "service tunnel summary byte ceiling",
        });
    }
    Ok(value)
}

/// Actual proxy state for one client profile: enabled iff any matching
/// startup service is enabled, bind from the first enabled listener in
/// configuration order overlaid with live manager binds.
fn proxy_value(handles: &InspectionHandles, profile: &str) -> serde_json::Value {
    let matches = |kind: &str| {
        (profile == "httpclient" && kind == "http-client")
            || (profile == "socks" && kind == "socks5-client")
    };
    let manager = handles
        .manager_live
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let mut enabled = false;
    let mut bind: Option<String> = None;
    let mut count = 0_usize;
    for entry in &handles.startup_services {
        if !matches(entry.kind) {
            continue;
        }
        count += 1;
        if !entry.enabled {
            continue;
        }
        enabled = true;
        if bind.is_none() {
            bind = manager
                .as_ref()
                .and_then(|live| live.client_listener_address(&entry.name))
                .or(entry.listener)
                .map(|address| address.to_string());
        }
    }
    serde_json::json!({
        "enabled": enabled,
        "bind": bind,
        "count": count,
    })
}

/// Actual SAM state: config enablement/bind plus the bounded live
/// session list when the SAM factory has published.
fn sam_value(handles: &InspectionHandles) -> serde_json::Value {
    let live = handles
        .sam_live
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let (session_count, sessions) = live
        .as_ref()
        .map(|state| {
            let registry = state.session_registry();
            let mut ids = registry.session_ids();
            let count = registry.session_count();
            // `session_ids` is already capped at 64; re-check defensively.
            ids.truncate(64);
            (count, ids)
        })
        .unwrap_or((0, Vec::new()));
    serde_json::json!({
        "enabled": handles.sam_endpoint.enabled,
        "bind": handles.sam_endpoint.bind.map(|address| address.to_string()),
        "session_count": session_count,
        "sessions": sessions,
    })
}

/// Actual I2CP state: config enablement/bind plus live counts when the
/// I2CP factory has published.
fn i2cp_value(handles: &InspectionHandles) -> serde_json::Value {
    let live = handles
        .i2cp_live
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let snapshot = live.as_ref().map(|state| state.snapshot());
    serde_json::json!({
        "enabled": handles.i2cp_endpoint.enabled,
        "bind": handles.i2cp_endpoint.bind.map(|address| address.to_string()),
        "session_count": snapshot.as_ref().map(|snapshot| snapshot.session_count).unwrap_or(0),
        "connection_count": snapshot.as_ref().map(|snapshot| snapshot.connection_count).unwrap_or(0),
        "destination_count": snapshot.as_ref().map(|snapshot| snapshot.destination_count).unwrap_or(0),
    })
}

/// Matrix census helper for tests: counts rows per availability class.
pub fn matrix_census() -> (usize, usize, usize, usize) {
    let mut available = 0;
    let mut gated = 0;
    let mut unavailable = 0;
    let mut neutral = 0;
    for row in ROUTER_INFO_SOURCE_MATRIX {
        match row.availability {
            SourceAvailability::Available => available += 1,
            SourceAvailability::PublishedGated { .. } => gated += 1,
            SourceAvailability::Unavailable { .. } => unavailable += 1,
            SourceAvailability::PermittedNeutral { .. } => neutral += 1,
        }
    }
    (available, gated, unavailable, neutral)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_endpoint() -> ServiceEndpoint {
        ServiceEndpoint {
            enabled: false,
            bind: None,
        }
    }

    fn test_handles() -> InspectionHandles {
        InspectionHandles::new(
            2,
            disabled_endpoint(),
            disabled_endpoint(),
            vec![
                StartupServiceEntry {
                    name: "alpha-client".to_owned(),
                    kind: "http-client",
                    client_side: true,
                    enabled: true,
                    listener: Some("127.0.0.1:8080".parse().expect("loopback")),
                },
                StartupServiceEntry {
                    name: "beta-server".to_owned(),
                    kind: "generic-server",
                    client_side: false,
                    enabled: false,
                    listener: None,
                },
                StartupServiceEntry {
                    name: "gamma-socks".to_owned(),
                    kind: "socks5-client",
                    client_side: true,
                    enabled: true,
                    listener: Some("127.0.0.1:8081".parse().expect("loopback")),
                },
            ],
        )
    }

    fn value(
        selector: RouterInfoSelector,
        handles: &InspectionHandles,
        uptime: u64,
    ) -> serde_json::Value {
        router_info_result(selector, handles, uptime).expect("selector answers")
    }

    #[test]
    fn proposal_i2ptunnel_summaries_are_bounded_and_canonical() {
        let handles = test_handles();
        let value = proposal_i2ptunnel_summaries(&handles).expect("summaries are available");
        let rows = value.as_array().expect("object-list shape");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["name"], "alpha-client");
        assert_eq!(rows[0]["side"], "client");
        assert_eq!(rows[0]["kind"], "http-client");
        assert_eq!(rows[0]["enabled"], true);
        assert_eq!(rows[0]["running"], false);
        assert_eq!(rows[0]["bind"], "127.0.0.1:8080");
        assert_eq!(rows[1]["name"], "beta-server");
        assert_eq!(rows[1]["side"], "server");
        assert_eq!(rows[2]["name"], "gamma-socks");
    }

    #[test]
    fn proposal_transport_totals_require_and_read_authoritative_sample() {
        let handles = test_handles();
        assert_eq!(
            proposal_transport_total("i2p.router.net.total.received.bytes", &handles)
                .expect_err("unobserved counters stay unavailable")
                .owner_plan,
            "322"
        );
        let metrics = Arc::new(ControlMetrics::new());
        metrics.observe_transport(1234, 5678, 12, 34);
        handles.publish_metrics(metrics);
        assert_eq!(
            proposal_transport_total("i2p.router.net.total.received.bytes", &handles)
                .expect("received total"),
            serde_json::json!(1234)
        );
        assert_eq!(
            proposal_transport_total("i2p.router.net.total.sent.bytes", &handles)
                .expect("sent total"),
            serde_json::json!(5678)
        );
    }

    #[test]
    fn proposal_success_rates_require_attempts_and_read_metrics() {
        let handles = test_handles();
        assert_eq!(
            proposal_tunnel_success_rate("i2p.router.net.tunnels.successrate", &handles)
                .expect_err("0/0 has no rate")
                .owner_plan,
            "322"
        );
        assert_eq!(
            proposal_tunnel_success_rate("i2p.router.net.tunnels.totalsuccessrate", &handles)
                .expect_err("0/0 has no cumulative rate")
                .owner_plan,
            "322"
        );
        let metrics = Arc::new(ControlMetrics::new());
        metrics.tick_at(std::time::Instant::now());
        metrics.observe_builds(3, 4);
        metrics.tick_at(std::time::Instant::now() + std::time::Duration::from_secs(15));
        handles.publish_metrics(metrics);
        assert_eq!(
            proposal_tunnel_success_rate("i2p.router.net.tunnels.successrate", &handles)
                .expect("recent observed ratio"),
            serde_json::json!(0.75)
        );
        assert_eq!(
            proposal_tunnel_success_rate("i2p.router.net.tunnels.totalsuccessrate", &handles)
                .expect("cumulative observed ratio"),
            serde_json::json!(0.75)
        );
    }

    #[test]
    fn proposal_tunnel_queue_depth_uses_attested_snapshot() {
        let handles = test_handles();
        handles
            .publish_tunnels(0, 0, 0, 7)
            .expect("bounded snapshot publishes");
        assert_eq!(
            proposal_tunnel_queue_depth(&handles).expect("attested queue depth"),
            serde_json::json!(7)
        );
    }

    #[test]
    fn proposal_local_router_info_is_bounded_and_publish_gated() {
        let handles = test_handles();
        assert_eq!(
            proposal_local_router_info(&handles),
            serde_json::Value::Null
        );
        let encoded = i2pr_netdb::encode(b"signed router info").expect("base64 encodes");
        handles
            .publish_local_router_info_b64(&encoded)
            .expect("bounded info publishes");
        assert_eq!(
            proposal_local_router_info(&handles),
            serde_json::Value::String(encoded)
        );
        assert!(matches!(
            handles
                .publish_local_router_info_b64(&"A".repeat(MAX_LOCAL_ROUTER_INFO_BASE64_BYTES + 1)),
            Err(PublishError::StringOverBound {
                key: "i2p.router.info"
            })
        ));
    }

    #[test]
    fn proposal_tbm_queue_depth_uses_independent_attested_snapshot() {
        let handles = test_handles();
        assert_eq!(
            proposal_tbm_queue_depth(&handles)
                .expect_err("unpublished queue is a gap")
                .owner,
            "attested Tunnel Build Message queue snapshot"
        );
        handles
            .publish_tunnels(0, 0, 0, 7)
            .expect("tunnel snapshot");
        handles.publish_tbm_queue(3);
        assert_eq!(
            proposal_tbm_queue_depth(&handles).expect("published TBM queue"),
            serde_json::json!(3)
        );
    }

    #[test]
    fn proposal_empty_router_info_lists_require_empty_attested_peer_sets() {
        let handles = test_handles();
        for key in [
            "i2p.router.netdb.activepeers.info",
            "i2p.router.netdb.peers.info",
        ] {
            assert!(proposal_empty_router_info_list(key, &handles).is_err());
        }
        handles
            .publish_netdb(Vec::new(), Vec::new(), FloodfillMode::Disabled)
            .expect("empty NetDB snapshot publishes");
        for key in [
            "i2p.router.netdb.activepeers.info",
            "i2p.router.netdb.peers.info",
        ] {
            assert_eq!(
                proposal_empty_router_info_list(key, &handles)
                    .expect("empty peer set proves empty info list"),
                serde_json::json!([]),
                "{key}"
            );
        }
        handles
            .publish_netdb(
                vec!["A".repeat(44)],
                vec!["B".repeat(44)],
                FloodfillMode::Disabled,
            )
            .expect("nonempty NetDB snapshot publishes");
        for key in [
            "i2p.router.netdb.activepeers.info",
            "i2p.router.netdb.peers.info",
        ] {
            assert_eq!(
                proposal_empty_router_info_list(key, &handles)
                    .expect_err("hash presence does not provide serialized RouterInfo")
                    .owner_plan,
                "322",
                "{key}"
            );
        }
    }

    #[test]
    fn proposal_empty_peer_stats_and_bans_require_attested_empty_sources() {
        let handles = test_handles();
        assert!(proposal_empty_banned_peer_details(&handles).is_err());
        handles
            .publish_netdb(Vec::new(), Vec::new(), FloodfillMode::Disabled)
            .expect("empty NetDB snapshot publishes");
        assert_eq!(
            proposal_empty_router_info_list("i2p.router.netdb.activepeers.stats", &handles)
                .expect("empty active peer set proves empty stats"),
            serde_json::json!([])
        );
        handles
            .publish_bans(Vec::new())
            .expect("empty ban set publishes");
        assert_eq!(
            proposal_empty_banned_peer_details(&handles).expect("empty ban set proves empty map"),
            serde_json::json!({})
        );
        handles
            .publish_bans(vec!["A".repeat(44)])
            .expect("nonempty ban set publishes");
        assert_eq!(
            proposal_empty_banned_peer_details(&handles)
                .expect_err("ban hashes do not provide detail")
                .owner,
            "ban reason and expiry detail snapshot"
        );
    }

    #[test]
    fn proposal_empty_tunnel_projection_requires_zero_aggregate() {
        let handles = test_handles();
        handles
            .publish_tunnels(0, 0, 0, 0)
            .expect("bounded snapshot publishes");
        for key in [
            "i2p.router.net.tunnels.exploratory.inbound",
            "i2p.router.net.tunnels.exploratory.outbound",
            "i2p.router.net.tunnels.client.inbound",
            "i2p.router.net.tunnels.client.outbound",
        ] {
            assert_eq!(
                proposal_empty_tunnel_projection(key, &handles).expect("zero count is known"),
                serde_json::json!(0),
                "{key}"
            );
        }
        for key in [
            "i2p.router.net.tunnels.exploratory.info.list",
            "i2p.router.net.tunnels.client.info.list",
            "i2p.router.net.tunnels.participating.info",
        ] {
            assert_eq!(
                proposal_empty_tunnel_projection(key, &handles).expect("empty list is known"),
                serde_json::json!([]),
                "{key}"
            );
        }

        handles
            .publish_tunnels(1, 1, 1, 0)
            .expect("bounded nonzero snapshot publishes");
        for key in [
            "i2p.router.net.tunnels.exploratory.inbound",
            "i2p.router.net.tunnels.exploratory.outbound",
            "i2p.router.net.tunnels.exploratory.info.list",
            "i2p.router.net.tunnels.client.inbound",
            "i2p.router.net.tunnels.client.outbound",
            "i2p.router.net.tunnels.client.info.list",
            "i2p.router.net.tunnels.participating.info",
        ] {
            let gap = proposal_empty_tunnel_projection(key, &handles)
                .expect_err("aggregate does not establish direction/details");
            assert_eq!(gap.owner_plan, "322", "{key}");
        }
    }

    #[test]
    fn plan288_static_selectors_report_config_truth() {
        let handles = test_handles();
        assert_eq!(
            value(RouterInfoSelector::RouterVersion, &handles, 0),
            serde_json::Value::String(ROUTER_VERSION.to_owned())
        );
        assert_eq!(ROUTER_VERSION, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            value(RouterInfoSelector::RouterApiVersion, &handles, 0),
            serde_json::json!(1)
        );
        assert_eq!(
            value(RouterInfoSelector::RouterStatus, &handles, 0),
            serde_json::Value::String("running".to_owned())
        );
        assert_eq!(
            value(RouterInfoSelector::RouterNetworkId, &handles, 0),
            serde_json::json!(2)
        );
    }

    #[test]
    fn plan288_router_uptime_advances_monotonically() {
        let handles = test_handles();
        assert_eq!(
            value(RouterInfoSelector::RouterUptime, &handles, 0),
            serde_json::json!(0)
        );
        assert_eq!(
            value(RouterInfoSelector::RouterUptime, &handles, 59),
            serde_json::json!(59)
        );
        assert_eq!(
            value(RouterInfoSelector::RouterUptime, &handles, u64::MAX),
            serde_json::json!(u64::MAX)
        );
    }

    #[test]
    fn plan288_router_hash_gated_until_identity_published() {
        let handles = test_handles();
        let gap = router_info_result(RouterInfoSelector::RouterHash, &handles, 0)
            .expect_err("hash gated before publication");
        assert_eq!(gap.owner_plan, "288");
        assert_eq!(
            gap.message(),
            "router.hash not available (Plan 288: bootstrap identity unpublished)"
        );
        // 44-char I2P base64 publishes; the exact string round-trips.
        let hash = "a".repeat(43) + "~";
        handles
            .publish_router_hash(&hash)
            .expect("valid hash publishes");
        assert_eq!(
            router_info_result(RouterInfoSelector::RouterHash, &handles, 0).expect("answers"),
            serde_json::Value::String(hash)
        );
    }

    #[test]
    fn plan288_hash_publication_rejects_malformed() {
        let handles = test_handles();
        assert_eq!(
            handles.publish_router_hash(""),
            Err(PublishError::MalformedHash)
        );
        assert_eq!(
            handles.publish_router_hash(&"a".repeat(43)),
            Err(PublishError::MalformedHash)
        );
        assert_eq!(
            handles.publish_router_hash(&"a".repeat(45)),
            Err(PublishError::MalformedHash)
        );
        assert_eq!(
            handles.publish_router_hash(&("a".repeat(43) + "=")),
            Err(PublishError::MalformedHash)
        );
        // Rejected publications leave the row gated.
        assert!(
            router_info_result(RouterInfoSelector::RouterHash, &handles, 0).is_err(),
            "rejected hash must not publish"
        );
    }

    #[test]
    fn plan295_owner_rows_gap_until_published() {
        use i2pr_i2pcontrol::RouterInfoSelector;
        let handles = test_handles();
        // AddressBook rows stay 294-owned; every Plan 295 owner-backed
        // row gaps with the 295 marker until composition installs it.
        // `network.clock_skew` is the exception: the neutral constant
        // answers without publication.
        for (selector, plan) in [
            (RouterInfoSelector::AddressBookPrivate, "294"),
            (RouterInfoSelector::AddressBookLocal, "294"),
            (RouterInfoSelector::AddressBookRouter, "294"),
            (RouterInfoSelector::AddressBookPublished, "294"),
            (RouterInfoSelector::AddressBookSubscriptions, "294"),
            (RouterInfoSelector::AddressBookConfig, "294"),
            (RouterInfoSelector::LogsRecent, "295"),
            (RouterInfoSelector::NewsFeed, "295"),
            (RouterInfoSelector::BannedPeers, "295"),
            (RouterInfoSelector::Ssu2ActiveSessions, "295"),
            (RouterInfoSelector::TransportErrors, "295"),
            (RouterInfoSelector::SuccessRate, "295"),
            (RouterInfoSelector::Bandwidth, "295"),
            (RouterInfoSelector::Rates, "295"),
        ] {
            let gap = router_info_result(selector, &handles, 0).expect_err("unpublished");
            assert_eq!(gap.owner_plan, plan, "wrong owner for {}", gap.key);
            assert_eq!(gap.key, selector.name());
        }
        assert_eq!(
            value(RouterInfoSelector::ClockSkew, &handles, 0),
            serde_json::json!(0),
            "neutral constant answers without publication"
        );
        // The ban row records the explicit ban owner once published.
        let gap = router_info_result(RouterInfoSelector::BannedPeers, &handles, 0)
            .expect_err("no ban owner");
        assert!(
            gap.message().contains("Plan 295"),
            "message: {}",
            gap.message()
        );
    }

    #[test]
    fn plan295_static_attestations_gap_until_published() {
        use i2pr_i2pcontrol::RouterInfoSelector;
        let handles = test_handles();
        for selector in [
            RouterInfoSelector::NetDbKnownPeers,
            RouterInfoSelector::NetDbActivePeers,
            RouterInfoSelector::NetDbFloodfillMode,
            RouterInfoSelector::Ntcp2ActivePeers,
            RouterInfoSelector::Reachability,
            RouterInfoSelector::ExploratoryCount,
            RouterInfoSelector::ClientCount,
            RouterInfoSelector::ParticipatingCount,
            RouterInfoSelector::BuildQueue,
        ] {
            let gap = router_info_result(selector, &handles, 0).expect_err("unpublished");
            assert_eq!(gap.owner_plan, "295", "wrong owner for {}", gap.key);
        }
    }

    #[test]
    fn plan295_published_sources_answer_after_attestation() {
        use i2pr_i2pcontrol::RouterInfoSelector;
        let handles = test_handles();
        handles
            .publish_netdb(vec!["peer-a".to_owned()], vec![], FloodfillMode::Disabled)
            .expect("netdb publishes");
        assert_eq!(
            value(RouterInfoSelector::NetDbKnownPeers, &handles, 0),
            serde_json::json!(["peer-a"])
        );
        assert_eq!(
            value(RouterInfoSelector::NetDbActivePeers, &handles, 0),
            serde_json::json!([])
        );
        assert_eq!(
            value(RouterInfoSelector::NetDbFloodfillMode, &handles, 0),
            serde_json::Value::String("disabled".to_owned())
        );
        handles
            .publish_transport(
                vec![],
                vec!["session-1".to_owned()],
                "ok",
                vec!["e1".to_owned()],
            )
            .expect("transport publishes");
        assert_eq!(
            value(RouterInfoSelector::Ssu2ActiveSessions, &handles, 0),
            serde_json::json!(["session-1"])
        );
        assert_eq!(
            value(RouterInfoSelector::Reachability, &handles, 0),
            serde_json::Value::String("ok".to_owned())
        );
        // Clock skew is the neutral constant: no publication exists.
        assert_eq!(
            value(RouterInfoSelector::ClockSkew, &handles, 0),
            serde_json::json!(0)
        );
        handles
            .publish_tunnels(2, 0, 1, 4)
            .expect("tunnels publish");
        // Count selectors emit a one-element list (frozen List shape).
        assert_eq!(
            value(RouterInfoSelector::ExploratoryCount, &handles, 0),
            serde_json::json!([2])
        );
        assert_eq!(
            value(RouterInfoSelector::BuildQueue, &handles, 0),
            serde_json::json!([4])
        );
        // The metrics owner serves success, bandwidth, and rates.
        let metrics = Arc::new(ControlMetrics::new());
        metrics.observe_builds(9, 10);
        handles.publish_metrics(metrics);
        assert_eq!(
            value(RouterInfoSelector::SuccessRate, &handles, 0),
            serde_json::json!({"succeeded": 9, "attempted": 10})
        );
        let bandwidth = value(RouterInfoSelector::Bandwidth, &handles, 0);
        assert_eq!(
            bandwidth,
            serde_json::json!({"inbound_bps": 0, "outbound_bps": 0}),
            "no transport source observed yet"
        );
        assert_eq!(
            value(RouterInfoSelector::Rates, &handles, 0),
            serde_json::json!({}),
            "no transport source observed yet"
        );
        // The log ring serves redacted lines with the drop count.
        let ring = Arc::new(LogRing::new());
        ring.record("INFO", "daemon", "control plane ready");
        ring.record("INFO", "daemon", "session token abc");
        handles.publish_log_ring(ring);
        assert_eq!(
            value(RouterInfoSelector::LogsRecent, &handles, 0),
            serde_json::json!({
                "entries": [
                    "INFO daemon: control plane ready",
                    "INFO daemon: [redacted: secret marker]",
                ],
                "dropped": 0,
            })
        );
        // The ban ledger attests the empty set.
        handles.publish_bans(vec![]).expect("bans publish");
        assert_eq!(
            value(RouterInfoSelector::BannedPeers, &handles, 0),
            serde_json::json!([])
        );
        // Router news stays unavailable by Plan 295 determination.
        let gap = router_info_result(RouterInfoSelector::NewsFeed, &handles, 0)
            .expect_err("news never served");
        assert_eq!(gap.owner_plan, "295");
        // Deterministic transitions: re-publication replaces state.
        handles
            .publish_netdb(vec![], vec!["peer-b".to_owned()], FloodfillMode::Floodfill)
            .expect("republish");
        assert_eq!(
            value(RouterInfoSelector::NetDbKnownPeers, &handles, 0),
            serde_json::json!([])
        );
        assert_eq!(
            value(RouterInfoSelector::NetDbActivePeers, &handles, 0),
            serde_json::json!(["peer-b"])
        );
        assert_eq!(
            value(RouterInfoSelector::NetDbFloodfillMode, &handles, 0),
            serde_json::Value::String("floodfill".to_owned())
        );
    }

    #[test]
    fn plan295_publication_rejects_over_ceiling() {
        let handles = test_handles();
        let oversized: Vec<String> = (0..MAX_INSPECTION_LIST + 1)
            .map(|n| format!("p{n}"))
            .collect();
        assert_eq!(
            handles.publish_netdb(oversized.clone(), Vec::new(), FloodfillMode::Disabled),
            Err(PublishError::ListOverBound {
                key: "netdb.known_peers"
            })
        );
        assert_eq!(
            handles.publish_transport(oversized.clone(), Vec::new(), "ok", Vec::new()),
            Err(PublishError::ListOverBound {
                key: "transport.ntcp2.active_peers"
            })
        );
        assert_eq!(
            handles.publish_transport(Vec::new(), Vec::new(), "", Vec::new()),
            Err(PublishError::StringOverBound {
                key: "transport.reachability"
            })
        );
        assert_eq!(
            handles.publish_transport(Vec::new(), Vec::new(), &"x".repeat(33), Vec::new()),
            Err(PublishError::StringOverBound {
                key: "transport.reachability"
            })
        );
        assert_eq!(
            handles.publish_bans(oversized.clone()),
            Err(PublishError::ListOverBound {
                key: "network.banned_peers"
            })
        );
        // Rejected publications leave rows unpublished (no partial state).
        assert!(router_info_result(RouterInfoSelector::NetDbKnownPeers, &handles, 0).is_err());
        assert!(router_info_result(RouterInfoSelector::BannedPeers, &handles, 0).is_err());
        assert!(router_info_result(RouterInfoSelector::SuccessRate, &handles, 0).is_err());
    }

    #[test]
    fn plan288_select_form_validation() {
        // Empty selection (token only) is valid and empty.
        let params: serde_json::Map<String, serde_json::Value> =
            [("Token".to_owned(), serde_json::json!("t"))]
                .into_iter()
                .collect();
        assert!(select_router_info(&params).expect("empty valid").is_empty());
        // Proposal order regardless of request order. Any selector value
        // selects the field; value semantics are intentionally absent.
        let params: serde_json::Map<String, serde_json::Value> = [
            ("Token".to_owned(), serde_json::json!("t")),
            ("i2p.router.id".to_owned(), serde_json::json!("")),
            ("i2p.router.news".to_owned(), serde_json::Value::Null),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            select_router_info(&params)
                .expect("ordered")
                .iter()
                .map(|field| field.key)
                .collect::<Vec<_>>(),
            vec!["i2p.router.news", "i2p.router.id"]
        );
        // Benign non-null selector values have no defined meaning and are
        // ignored, as specified by presence-based selection.
        let params: serde_json::Map<String, serde_json::Value> = [
            ("Token".to_owned(), serde_json::json!("t")),
            ("i2p.router.news".to_owned(), serde_json::json!(true)),
        ]
        .into_iter()
        .collect();
        assert_eq!(select_router_info(&params).expect("value ignored").len(), 1);
        // Old normalized names are not aliases in the canonical namespace.
        for key in [
            "bogus",
            "router.uptime",
            "router.version",
            "router.status",
            "Router.Version",
            "ROUTER.VERSION",
        ] {
            let params: serde_json::Map<String, serde_json::Value> = [
                ("Token".to_owned(), serde_json::json!("t")),
                (key.to_owned(), serde_json::json!("")),
            ]
            .into_iter()
            .collect();
            assert_eq!(
                select_router_info(&params),
                Err(SelectError::UnknownKey(key.to_owned())),
                "key {key} must not select"
            );
        }
        let base: serde_json::Map<String, serde_json::Value> = [
            ("i2p.router.version".to_owned(), serde_json::Value::Null),
            ("i2p.router.uptime".to_owned(), serde_json::json!("ignored")),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            select_router_info(&base)
                .expect("base API fields remain selected")
                .iter()
                .map(|field| field.key)
                .collect::<Vec<_>>(),
            vec!["i2p.router.version", "i2p.router.uptime"]
        );
        // ClientServicesInfo shares the select rules.
        let params: serde_json::Map<String, serde_json::Value> = [
            ("Token".to_owned(), serde_json::json!("t")),
            ("SAM".to_owned(), serde_json::Value::Null),
            ("BOB".to_owned(), serde_json::Value::Null),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            select_client_services(&params).expect("services select"),
            vec![ClientService::Sam, ClientService::Bob]
        );
        let values: serde_json::Map<String, serde_json::Value> =
            [("I2CP".to_owned(), serde_json::json!(false))]
                .into_iter()
                .collect();
        assert_eq!(
            select_client_services(&values),
            Ok(vec![ClientService::I2cp])
        );
    }

    #[test]
    fn plan288_gap_messages_name_owner_and_plan() {
        let hash_gap = InspectionGap {
            key: "router.hash",
            owner_plan: "288",
            owner: "bootstrap identity",
        };
        assert_eq!(
            hash_gap.message(),
            "router.hash not available (Plan 288: bootstrap identity unpublished)"
        );
        let book_gap = InspectionGap {
            key: "addressbook.private",
            owner_plan: "294",
            owner: "canonical AddressBook",
        };
        assert_eq!(
            book_gap.message(),
            "addressbook.private not available (Plan 294: canonical AddressBook unpublished)"
        );
    }

    #[test]
    fn plan288_bob_is_deliberate_constant() {
        let handles = test_handles();
        assert_eq!(
            client_service_result(ClientService::Bob, &handles),
            serde_json::json!({"enabled": false})
        );
    }

    #[test]
    fn plan288_sam_reports_disabled_truthfully() {
        let handles = test_handles();
        assert_eq!(
            client_service_result(ClientService::Sam, &handles),
            serde_json::json!({
                "enabled": false,
                "bind": null,
                "session_count": 0,
                "sessions": [],
            })
        );
    }

    #[test]
    fn plan288_sam_live_overlay_reports_bounded_sessions() {
        use crate::config::SamConfig;
        use i2pr_api::{SamLimits, SamSessionId};
        use i2pr_client::DestinationId;
        let config = SamConfig {
            enabled: true,
            bind_address: "127.0.0.1".parse().expect("loopback"),
            port: 0,
            limits: SamLimits::default(),
        };
        let state = Arc::new(crate::sam::SamServiceState::new(config).expect("sam builds"));
        let registry = state.session_registry();
        for (name, seed) in [("beta", 2_u8), ("alpha", 1_u8)] {
            let session = SamSessionId::new(name).expect("session id");
            let mut bytes = [0_u8; 32];
            bytes[0] = seed;
            let reservation = registry
                .reserve_session(
                    session,
                    DestinationId::from_hash(i2pr_proto::Hash::from_bytes(bytes)),
                )
                .expect("reserve");
            registry
                .commit_reservation(&reservation, "PUB".to_owned())
                .expect("commit");
        }
        let handles = InspectionHandles::new(
            2,
            ServiceEndpoint {
                enabled: true,
                bind: Some("127.0.0.1:7656".parse().expect("loopback")),
            },
            disabled_endpoint(),
            Vec::new(),
        );
        handles.publish_sam(state);
        let value = client_service_result(ClientService::Sam, &handles);
        assert_eq!(value["enabled"], serde_json::json!(true));
        assert_eq!(value["bind"], serde_json::json!("127.0.0.1:7656"));
        assert_eq!(value["session_count"], serde_json::json!(2));
        // Deterministic sorted order, no destination material, no sockets.
        assert_eq!(value["sessions"], serde_json::json!(["alpha", "beta"]));
        assert!(value.get("sockets").is_none());
        assert!(value.get("destination").is_none());
    }

    #[test]
    fn plan288_i2cp_reports_disabled_then_live_counts() {
        let handles = test_handles();
        assert_eq!(
            client_service_result(ClientService::I2cp, &handles),
            serde_json::json!({
                "enabled": false,
                "bind": null,
                "session_count": 0,
                "connection_count": 0,
                "destination_count": 0,
            })
        );
    }

    #[test]
    fn plan288_i2ptunnel_splits_startup_inventory() {
        let handles = test_handles();
        let value = client_service_result(ClientService::I2pTunnel, &handles);
        let client = value["client"].as_object().expect("client map");
        let server = value["server"].as_object().expect("server map");
        assert_eq!(client.len(), 2);
        assert_eq!(server.len(), 1);
        assert_eq!(
            client["alpha-client"]["kind"],
            serde_json::json!("http-client")
        );
        assert_eq!(client["alpha-client"]["enabled"], serde_json::json!(true));
        assert_eq!(client["alpha-client"]["running"], serde_json::json!(false));
        assert_eq!(
            client["alpha-client"]["bind"],
            serde_json::json!("127.0.0.1:8080")
        );
        // Disabled entries are reported, never omitted.
        assert_eq!(server["beta-server"]["enabled"], serde_json::json!(false));
        assert_eq!(server["beta-server"]["bind"], serde_json::json!(null));
        // No address-book addresses leak before Plan 294.
        assert!(client["alpha-client"].get("address").is_none());
    }

    #[test]
    fn plan288_proxy_reports_first_enabled_bind() {
        let handles = test_handles();
        assert_eq!(
            client_service_result(ClientService::HttpProxy, &handles),
            serde_json::json!({
                "enabled": true,
                "bind": "127.0.0.1:8080",
                "count": 1,
            })
        );
        assert_eq!(
            client_service_result(ClientService::Socks, &handles),
            serde_json::json!({
                "enabled": true,
                "bind": "127.0.0.1:8081",
                "count": 1,
            })
        );
        let empty = InspectionHandles::new(2, disabled_endpoint(), disabled_endpoint(), Vec::new());
        assert_eq!(
            client_service_result(ClientService::HttpProxy, &empty),
            serde_json::json!({"enabled": false, "bind": null, "count": 0})
        );
    }

    #[test]
    fn plan288_debug_redacts_snapshots() {
        let handles = test_handles();
        handles
            .publish_router_hash(&format!("{}~", "q".repeat(43)))
            .expect("hash");
        let debug = format!("{:?}", handles);
        assert!(
            !debug.contains(&"q".repeat(43)),
            "hash value must not appear in Debug"
        );
        assert!(
            debug.contains("router_hash_published"),
            "presence flags are fine"
        );
    }

    #[test]
    fn plan295_matrix_census_matches_contract() {
        assert_eq!(matrix_census(), (27, 1, 1, 1));
    }

    #[test]
    fn plan288_handles_build_from_config() {
        let b32 = format!("{}.b32.i2p", "a".repeat(52));
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = \"state\"\n[service_tunnels]\nenabled = false\n[[service_tunnels.tunnel]]\nid = \"web-client\"\nkind = \"http-client\"\nenabled = true\nlistener = \"127.0.0.1:8180\"\ndestination = \"{b32}\"\n"
        );
        let config = crate::config::Config::parse(&text).expect("config parses");
        let handles = InspectionHandles::from_config(&config);
        assert_eq!(handles.network_id(), 2);
        let services = handles.startup_services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "web-client");
        assert_eq!(services[0].kind, "http-client");
        assert!(services[0].client_side);
        assert!(services[0].enabled);
        // Static service truth flows into ClientServicesInfo.
        assert_eq!(
            client_service_result(ClientService::HttpProxy, &handles)["enabled"],
            serde_json::json!(true)
        );
    }
}
