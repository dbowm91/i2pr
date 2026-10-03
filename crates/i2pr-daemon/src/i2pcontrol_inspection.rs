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

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use i2pr_i2pcontrol::{
    ClientService, ROUTER_INFO_SOURCE_MATRIX, RouterInfoSelector, SourceAvailability, service_row,
    source_row,
};

use crate::i2cp::I2cpServiceState;
use crate::sam::SamServiceState;
use crate::service_tunnels::ServiceTunnelManager;

/// Control-plane API version declared by `router.api_version`.
pub const ROUTER_API_VERSION: u64 = 1;

/// Liveness string reported by `router.status` while dispatch executes.
pub const ROUTER_STATUS_RUNNING: &str = "running";

/// Router version string reported by `router.version`: this router's own
/// crate version, never another router's release string.
pub const ROUTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum entries in one published hash/peer list.
pub const MAX_INSPECTION_LIST: usize = 1024;

/// Maximum entries in a published rates map.
pub const MAX_INSPECTION_RATES: usize = 8;

/// Maximum bytes of a published mode/state string.
pub const MAX_INSPECTION_STATE_STRING: usize = 32;

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
    /// Published `(succeeded, attempted)` tunnel-build counters or `None`.
    success: Option<(u64, u64)>,
    /// Published `(inbound_bps, outbound_bps)` or `None`.
    bandwidth: Option<(u64, u64)>,
    /// Published clock-skew observation or `None`.
    clock_skew: Option<i64>,
    /// Published rate entries or `None`.
    rates: Option<BTreeMap<String, u64>>,
}

/// Rejection of an over-ceiling or malformed publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishError {
    /// A list exceeded [`MAX_INSPECTION_LIST`].
    ListOverBound {
        /// Selector the list belongs to.
        key: &'static str,
    },
    /// A rates map exceeded [`MAX_INSPECTION_RATES`].
    RatesOverBound,
    /// A state string exceeded its ceiling.
    StringOverBound {
        /// Selector the string belongs to.
        key: &'static str,
    },
    /// A router hash was not 44-char I2P base64.
    MalformedHash,
}

impl core::fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ListOverBound { key } => {
                write!(formatter, "inspection list over ceiling for {key}")
            }
            Self::RatesOverBound => write!(formatter, "inspection rates map over ceiling"),
            Self::StringOverBound { key } => {
                write!(formatter, "inspection string over ceiling for {key}")
            }
            Self::MalformedHash => write!(formatter, "inspection router hash malformed"),
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
        Self::new(
            config.network.network_id,
            sam_endpoint,
            i2cp_endpoint,
            startup_services,
        )
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
            return Err(PublishError::MalformedHash);
        }
        if let Ok(mut published) = self.published.lock() {
            published.router_hash = Some(hash_b64.to_owned());
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
    #[allow(clippy::too_many_arguments)]
    pub fn publish_transport(
        &self,
        ntcp2_peers: Vec<String>,
        ssu2_sessions: Vec<String>,
        reachability: &str,
        errors: Vec<String>,
        clock_skew: i64,
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
            published.clock_skew = Some(clock_skew);
        }
        Ok(())
    }

    /// Publishes bounded tunnel views.
    #[allow(clippy::too_many_arguments)]
    pub fn publish_tunnels(
        &self,
        exploratory: u64,
        client_tunnels: u64,
        participating: u64,
        build_queue: u64,
        succeeded: u64,
        attempted: u64,
        inbound_bps: u64,
        outbound_bps: u64,
    ) -> Result<(), PublishError> {
        if let Ok(mut published) = self.published.lock() {
            published.exploratory = Some(exploratory);
            published.client_tunnels = Some(client_tunnels);
            published.participating = Some(participating);
            published.build_queue = Some(build_queue);
            published.success = Some((succeeded, attempted));
            published.bandwidth = Some((inbound_bps, outbound_bps));
        }
        Ok(())
    }

    /// Publishes bounded rate entries (deterministic key order).
    pub fn publish_rates(&self, rates: BTreeMap<String, u64>) -> Result<(), PublishError> {
        if rates.len() > MAX_INSPECTION_RATES {
            return Err(PublishError::RatesOverBound);
        }
        for key in rates.keys() {
            if key.is_empty() || key.len() > 128 {
                return Err(PublishError::StringOverBound {
                    key: "network.rates",
                });
            }
        }
        if let Ok(mut published) = self.published.lock() {
            published.rates = Some(rates);
        }
        Ok(())
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
            .field("rates_published", &snapshots.rates.is_some())
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
    if serde_json::to_vec(&value).map(|bytes| bytes.len()).unwrap_or(usize::MAX)
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
    if serde_json::to_vec(&value).map(|bytes| bytes.len()).unwrap_or(usize::MAX)
        > MAX_ADDRESSBOOK_SUBSCRIPTION_BYTES
    {
        return Err(internal());
    }
    Ok(value)
}

/// Renders the committed thirteen-key configuration map.
fn addressbook_config_value(
    handles: &InspectionHandles,
    row: &i2pr_i2pcontrol::SourceRow,
) -> Result<serde_json::Value, InspectionGap> {
    let snapshot = addressbook_cells(handles, row)?;
    let mut map = serde_json::Map::with_capacity(snapshot.config_entries().len());
    for (key, value) in snapshot.config_entries() {
        map.insert(key.clone(), serde_json::Value::String(value.clone()));
    }
    Ok(serde_json::Value::Object(map))
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
    /// A selector key carrying a non-null value. The canonical select
    /// form is `"key": null`; values carry no defined meaning.
    NonNullValue(String),
}

/// Validates the RouterInfo select form: `Token` plus selector keys with
/// null values. Returns the selection in canonical matrix order; the
/// JSON response object itself sorts keys lexicographically
/// (`serde_json::Map`), so both orders are deterministic regardless of
/// request order.
pub fn select_router_info(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<RouterInfoSelector>, SelectError> {
    use i2pr_i2pcontrol::RouterInfoSelector;
    let mut selected = Vec::new();
    for (key, value) in params {
        if key == "Token" {
            continue;
        }
        let selector = RouterInfoSelector::parse(key)
            .map_err(|_| SelectError::UnknownKey(truncated_key(key)))?;
        if !value.is_null() {
            return Err(SelectError::NonNullValue(key.clone()));
        }
        if !selected.contains(&selector) {
            selected.push(selector);
        }
    }
    selected.sort_by_key(|selector| i2pr_i2pcontrol::selector_index(*selector));
    Ok(selected)
}

/// Validates the ClientServicesInfo select form with the same rules.
pub fn select_client_services(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<ClientService>, SelectError> {
    let mut selected = Vec::new();
    for (key, value) in params {
        if key == "Token" {
            continue;
        }
        let service =
            ClientService::parse(key).map_err(|_| SelectError::UnknownKey(truncated_key(key)))?;
        if !value.is_null() {
            return Err(SelectError::NonNullValue(key.clone()));
        }
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
        RouterInfoSelector::RouterVersion => Ok(serde_json::Value::String(ROUTER_VERSION.to_owned())),
        RouterInfoSelector::RouterApiVersion => {
            Ok(serde_json::Value::from(ROUTER_API_VERSION))
        }
        RouterInfoSelector::RouterUptime => Ok(serde_json::Value::from(uptime_secs)),
        RouterInfoSelector::RouterStatus => {
            Ok(serde_json::Value::String(ROUTER_STATUS_RUNNING.to_owned()))
        }
        RouterInfoSelector::RouterNetworkId => Ok(serde_json::Value::from(handles.network_id())),
        RouterInfoSelector::RouterHash => snapshots.router_hash.map(serde_json::Value::String).ok_or_else(
            || match row.availability {
                SourceAvailability::PublishedGated { owner, owner_plan } => gap(owner, owner_plan),
                _ => gap("bootstrap identity", "288"),
            },
        ),
        RouterInfoSelector::NetDbKnownPeers => snapshots
            .netdb_known
            .map(|peers| strings_value(&peers))
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::NetDbActivePeers => snapshots
            .netdb_active
            .map(|peers| strings_value(&peers))
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::NetDbFloodfillMode => snapshots
            .floodfill_mode
            .map(|mode| serde_json::Value::String(mode.as_str().to_owned()))
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::Ntcp2ActivePeers => snapshots
            .ntcp2_peers
            .map(|peers| strings_value(&peers))
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::Ssu2ActiveSessions => snapshots
            .ssu2_sessions
            .map(|sessions| strings_value(&sessions))
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::Reachability => snapshots
            .reachability
            .map(serde_json::Value::String)
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::TransportErrors => snapshots
            .transport_errors
            .map(|errors| strings_value(&errors))
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::ExploratoryCount => snapshots
            .exploratory
            .map(count_value)
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::ClientCount => snapshots
            .client_tunnels
            .map(count_value)
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::ParticipatingCount => snapshots
            .participating
            .map(count_value)
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::BuildQueue => snapshots
            .build_queue
            .map(count_value)
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::SuccessRate => snapshots
            .success
            .map(|(succeeded, attempted)| {
                serde_json::json!({"succeeded": succeeded, "attempted": attempted})
            })
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::Bandwidth => snapshots
            .bandwidth
            .map(|(inbound_bps, outbound_bps)| {
                serde_json::json!({"inbound_bps": inbound_bps, "outbound_bps": outbound_bps})
            })
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::AddressBookPrivate => {
            addressbook_book_value(handles, 0, &row)
        }
        RouterInfoSelector::AddressBookLocal => addressbook_book_value(handles, 1, &row),
        RouterInfoSelector::AddressBookRouter => addressbook_book_value(handles, 2, &row),
        RouterInfoSelector::AddressBookPublished => addressbook_book_value(handles, 3, &row),
        RouterInfoSelector::AddressBookSubscriptions => {
            addressbook_subscriptions_value(handles, &row)
        }
        RouterInfoSelector::AddressBookConfig => addressbook_config_value(handles, &row),
        RouterInfoSelector::LogsRecent
        | RouterInfoSelector::NewsFeed
        | RouterInfoSelector::BannedPeers => Err(unavailable_gap(row)),
        RouterInfoSelector::ClockSkew => snapshots
            .clock_skew
            .map(serde_json::Value::from)
            .ok_or_else(|| gated_gap(row)),
        RouterInfoSelector::Rates => snapshots
            .rates
            .map(|rates| {
                let entries: serde_json::Map<String, serde_json::Value> = rates
                    .into_iter()
                    .map(|(key, value)| (key, serde_json::Value::from(value)))
                    .collect();
                serde_json::Value::Object(entries)
            })
            .ok_or_else(|| gated_gap(row)),
    }
}

/// Builds the gap for a publish-gated row from the matrix row.
fn gated_gap(row: i2pr_i2pcontrol::SourceRow) -> InspectionGap {
    match row.availability {
        SourceAvailability::PublishedGated { owner, owner_plan } => InspectionGap {
            key: row.key,
            owner_plan,
            owner,
        },
        SourceAvailability::Unavailable { owner_plan, .. } => InspectionGap {
            key: row.key,
            owner_plan,
            owner: row.owner,
        },
        _ => InspectionGap {
            key: row.key,
            owner_plan: "288",
            owner: row.owner,
        },
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
        use i2pr_i2pcontrol::RouterInfoSelector;
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
    fn plan288_unavailable_selectors_fail_with_owning_plan() {
        use i2pr_i2pcontrol::RouterInfoSelector;
        let handles = test_handles();
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
        ] {
            let gap = router_info_result(selector, &handles, 0).expect_err("unavailable");
            assert_eq!(gap.owner_plan, plan, "wrong owner for {}", gap.key);
            assert_eq!(gap.key, selector.name());
        }
        // The ban row records the missing ban facility explicitly.
        let gap = router_info_result(RouterInfoSelector::BannedPeers, &handles, 0)
            .expect_err("no ban owner");
        assert!(
            gap.message().contains("Plan 295"),
            "message: {}",
            gap.message()
        );
    }

    #[test]
    fn plan288_gated_dynamics_fail_until_published() {
        use i2pr_i2pcontrol::RouterInfoSelector;
        let handles = test_handles();
        for selector in [
            RouterInfoSelector::NetDbKnownPeers,
            RouterInfoSelector::NetDbActivePeers,
            RouterInfoSelector::NetDbFloodfillMode,
            RouterInfoSelector::Ntcp2ActivePeers,
            RouterInfoSelector::Ssu2ActiveSessions,
            RouterInfoSelector::Reachability,
            RouterInfoSelector::TransportErrors,
            RouterInfoSelector::ExploratoryCount,
            RouterInfoSelector::ClientCount,
            RouterInfoSelector::ParticipatingCount,
            RouterInfoSelector::BuildQueue,
            RouterInfoSelector::SuccessRate,
            RouterInfoSelector::Bandwidth,
            RouterInfoSelector::ClockSkew,
            RouterInfoSelector::Rates,
        ] {
            let gap = router_info_result(selector, &handles, 0).expect_err("gated");
            assert_eq!(gap.owner_plan, "295", "wrong owner for {}", gap.key);
        }
    }

    #[test]
    fn plan288_published_dynamics_answer_after_state_transitions() {
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
                -3,
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
        assert_eq!(
            value(RouterInfoSelector::ClockSkew, &handles, 0),
            serde_json::json!(-3)
        );
        handles
            .publish_tunnels(2, 0, 1, 4, 9, 10, 1000, 2000)
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
        assert_eq!(
            value(RouterInfoSelector::SuccessRate, &handles, 0),
            serde_json::json!({"succeeded": 9, "attempted": 10})
        );
        assert_eq!(
            value(RouterInfoSelector::Bandwidth, &handles, 0),
            serde_json::json!({"inbound_bps": 1000, "outbound_bps": 2000})
        );
        let mut rates = BTreeMap::new();
        rates.insert("inbound".to_owned(), 7_u64);
        handles.publish_rates(rates).expect("rates publish");
        assert_eq!(
            value(RouterInfoSelector::Rates, &handles, 0),
            serde_json::json!({"inbound": 7})
        );
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
    fn plan288_publication_rejects_over_ceiling() {
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
            handles.publish_transport(oversized.clone(), Vec::new(), "ok", Vec::new(), 0),
            Err(PublishError::ListOverBound {
                key: "transport.ntcp2.active_peers"
            })
        );
        assert_eq!(
            handles.publish_transport(Vec::new(), Vec::new(), "", Vec::new(), 0),
            Err(PublishError::StringOverBound {
                key: "transport.reachability"
            })
        );
        assert_eq!(
            handles.publish_transport(Vec::new(), Vec::new(), &"x".repeat(33), Vec::new(), 0),
            Err(PublishError::StringOverBound {
                key: "transport.reachability"
            })
        );
        let mut rates = BTreeMap::new();
        for n in 0..MAX_INSPECTION_RATES + 1 {
            rates.insert(format!("rate-{n}"), n as u64);
        }
        assert_eq!(
            handles.publish_rates(rates),
            Err(PublishError::RatesOverBound)
        );
        // Rejected publications leave rows gated (no partial state).
        assert!(router_info_result(RouterInfoSelector::NetDbKnownPeers, &handles, 0).is_err());
        assert!(router_info_result(RouterInfoSelector::Rates, &handles, 0).is_err());
    }

    #[test]
    fn plan288_select_form_validation() {
        use i2pr_i2pcontrol::RouterInfoSelector;
        // Empty selection (token only) is valid and empty.
        let params: serde_json::Map<String, serde_json::Value> =
            [("Token".to_owned(), serde_json::json!("t"))]
                .into_iter()
                .collect();
        assert_eq!(
            select_router_info(&params).expect("empty valid"),
            Vec::new()
        );
        // Canonical order regardless of request order; duplicates collapse.
        let params: serde_json::Map<String, serde_json::Value> = [
            ("Token".to_owned(), serde_json::json!("t")),
            ("router.uptime".to_owned(), serde_json::Value::Null),
            ("router.version".to_owned(), serde_json::Value::Null),
            ("router.version".to_owned(), serde_json::Value::Null),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            select_router_info(&params).expect("ordered"),
            vec![
                RouterInfoSelector::RouterVersion,
                RouterInfoSelector::RouterUptime
            ]
        );
        // Non-null values carry no defined meaning and are rejected.
        let params: serde_json::Map<String, serde_json::Value> = [
            ("Token".to_owned(), serde_json::json!("t")),
            ("router.version".to_owned(), serde_json::json!(true)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            select_router_info(&params),
            Err(SelectError::NonNullValue("router.version".to_owned()))
        );
        // Unknown keys fail, including every base-compatibility key: the
        // `i2p.*` vocabulary is structurally disjoint from Proposal keys.
        for key in [
            "bogus",
            "i2p.router.uptime",
            "i2p.router.version",
            "i2p.router.netdb.knownpeers",
            "Router.Version",
            "ROUTER.VERSION",
        ] {
            let params: serde_json::Map<String, serde_json::Value> = [
                ("Token".to_owned(), serde_json::json!("t")),
                (key.to_owned(), serde_json::Value::Null),
            ]
            .into_iter()
            .collect();
            assert_eq!(
                select_router_info(&params),
                Err(SelectError::UnknownKey(key.to_owned())),
                "key {key} must not select"
            );
        }
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
    fn plan288_matrix_census_matches_contract() {
        assert_eq!(matrix_census(), (11, 16, 3, 0));
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
