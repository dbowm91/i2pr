//! Plan 288 machine-readable source matrix: one row per exact Proposal 170
//! RouterInfo selector (30) and ClientServicesInfo service (6).
//!
//! Each row records the wire key, return type, authoritative owner
//! subsystem, snapshot method, cardinality/encoded-byte ceiling,
//! sensitivity/redaction rule, freshness semantics, availability state,
//! and the fixture/test identifier that proves the row. Availability is
//! never derived from whether a serializer exists: [`SourceAvailability`]
//! distinguishes live sources from publish-gated, permitted-neutral, and
//! unavailable rows.
//!
//! Truthfulness rules (Plan 288 §Source contract):
//! - a request selecting an [`SourceAvailability::Unavailable`] row fails
//!   the whole request explicitly; no partial response is emitted;
//! - a [`SourceAvailability::PublishedGated`] row fails the whole request
//!   until its owner publishes a snapshot; empty/zero is valid only after
//!   the authoritative owner was queried and reported an actual
//!   empty/zero state;
//! - [`SourceAvailability::PermittedNeutral`] rows are the only rows that
//!   may emit a protocol-permitted neutral value without a live source,
//!   and each one requires an explicit protocol justification. Plan 288
//!   closes with zero permitted-neutral rows (strictness is the default;
//!   Plan 295 must justify each neutrality it introduces).
//!
//! Base-compatibility separation (verified read-only against pinned i2pd
//! `2d57d3f6783efbfebde6c5b03f29e6c231a84d6b`,
//! `daemon/I2PControlHandlers.cpp`): the adopted base form selects with
//! `i2p.router.*` / `i2p.router.net.*` keys while the Proposal-direct form
//! below uses unprefixed `router.*` / `netdb.*` / `transport.*` /
//! `tunnel.*` / `addressbook.*` / `logs.*` / `news.*` / `network.*` keys.
//! The two vocabularies are disjoint by construction, so an `i2p.*` key
//! can never select a Proposal serializer. Unknown keys (including every
//! `i2p.*` base key) are rejected as invalid params, which is stricter
//! than i2pd's skip-and-log behavior; the strictness is deliberate and
//! recorded here.

use crate::client_services::{CLIENT_SERVICES, ClientService};
use crate::router_info::{ROUTER_INFO_SELECTORS, ReturnType, RouterInfoSelector};

/// Availability state of one matrix row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceAvailability {
    /// A live source answers every request (static constants and
    /// configuration truth included: the build version, the API
    /// declaration, and validated configuration are authoritative
    /// owners for their rows).
    Available,
    /// The row has a typed snapshot owner, but the owner must publish
    /// before the row can answer. Until publication the whole request
    /// fails explicitly. Residual publication wiring belongs to the
    /// plan named in `owner_plan`.
    PublishedGated {
        /// Owning subsystem that must publish (e.g. `"netdb"`).
        owner: &'static str,
        /// Plan that owns the residual publication wiring.
        owner_plan: &'static str,
    },
    /// The protocol permits an explicit neutral value without a live
    /// source. Each use requires the recorded justification; Plan 288
    /// records none (see [`SOURCE_MATRIX_NEUTRAL_COUNT`]).
    PermittedNeutral {
        /// Why the neutral value is protocol-permitted.
        reason: &'static str,
    },
    /// No source exists yet. Selecting this row fails the whole request
    /// with the owning-plan marker. The `reason` records why an empty
    /// or zero value would be fabrication rather than truth.
    Unavailable {
        /// Plan that owns a truthful source for this row.
        owner_plan: &'static str,
        /// Why no neutral value may be emitted.
        reason: &'static str,
    },
}

/// One machine-readable source-matrix row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRow {
    /// Exact wire key.
    pub key: &'static str,
    /// Declared return type.
    pub return_type: ReturnType,
    /// Authoritative owner subsystem.
    pub owner: &'static str,
    /// Snapshot method that supplies the value.
    pub snapshot: &'static str,
    /// Maximum collection cardinality for list/map rows (`0` for
    /// scalar rows).
    pub max_items: usize,
    /// Maximum encoded bytes for the serialized value.
    pub max_bytes: usize,
    /// Sensitivity and redaction rule.
    pub sensitivity: &'static str,
    /// Freshness semantics.
    pub freshness: &'static str,
    /// Availability state.
    pub availability: SourceAvailability,
    /// Fixture/test identifier proving the row.
    pub test_id: &'static str,
}

/// Exact RouterInfo source matrix in canonical selector order (30 rows).
///
/// Row proofs live in `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`
/// (builders) and `crates/i2pr-daemon/tests/i2pcontrol_inspection.rs`
/// (wire); contract-shape proofs live in
/// `crates/i2pr-i2pcontrol/tests/contract.rs`.
pub const ROUTER_INFO_SOURCE_MATRIX: [SourceRow; 30] = [
    SourceRow {
        key: "router.version",
        return_type: ReturnType::String,
        owner: "daemon build",
        snapshot: "ROUTER_VERSION (crate version constant)",
        max_items: 0,
        max_bytes: 64,
        sensitivity: "public; release string of this router, never mimics another router",
        freshness: "static per build",
        availability: SourceAvailability::Available,
        test_id: "plan288_router_version_is_crate_version",
    },
    SourceRow {
        key: "router.api_version",
        return_type: ReturnType::Integer,
        owner: "i2pcontrol contract",
        snapshot: "SUPPORTED_API_VERSION (constant 1)",
        max_items: 0,
        max_bytes: 8,
        sensitivity: "public",
        freshness: "static per contract",
        availability: SourceAvailability::Available,
        test_id: "plan288_router_api_version_is_one",
    },
    SourceRow {
        key: "router.uptime",
        return_type: ReturnType::Integer,
        owner: "daemon i2pcontrol service",
        snapshot: "service monotonic epoch (seconds, saturating)",
        max_items: 0,
        max_bytes: 8,
        sensitivity: "public; control-plane uptime, not wall-clock boot time",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_router_uptime_advances_monotonically",
    },
    SourceRow {
        key: "router.status",
        return_type: ReturnType::String,
        owner: "daemon i2pcontrol service",
        snapshot: "dispatch liveness constant (\"running\" while dispatch executes)",
        max_items: 0,
        max_bytes: 16,
        sensitivity: "public",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_router_status_is_running_while_serving",
    },
    SourceRow {
        key: "router.network_id",
        return_type: ReturnType::Integer,
        owner: "daemon configuration",
        snapshot: "validated network.network_id",
        max_items: 0,
        max_bytes: 8,
        sensitivity: "public",
        freshness: "static per configuration",
        availability: SourceAvailability::Available,
        test_id: "plan288_router_network_id_matches_config",
    },
    SourceRow {
        key: "router.hash",
        return_type: ReturnType::String,
        owner: "daemon bootstrap identity",
        snapshot: "published local RouterHash (I2P base64, 44 chars)",
        max_items: 0,
        max_bytes: 64,
        sensitivity: "public router hash; never derived from secrets at request time",
        freshness: "static per identity; published once bootstrap owns it",
        availability: SourceAvailability::PublishedGated {
            owner: "bootstrap identity",
            owner_plan: "288",
        },
        test_id: "plan288_router_hash_gated_until_identity_published",
    },
    SourceRow {
        key: "netdb.known_peers",
        return_type: ReturnType::List,
        owner: "i2pr-netdb",
        snapshot: "bounded known-peer hash list",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public router hashes only; bounded count",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "netdb",
            owner_plan: "295",
        },
        test_id: "plan288_netdb_known_peers_gated_until_published",
    },
    SourceRow {
        key: "netdb.active_peers",
        return_type: ReturnType::List,
        owner: "i2pr-netdb",
        snapshot: "bounded active-peer hash list",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public router hashes only; bounded count",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "netdb",
            owner_plan: "295",
        },
        test_id: "plan288_netdb_active_peers_gated_until_published",
    },
    SourceRow {
        key: "netdb.floodfill_mode",
        return_type: ReturnType::String,
        owner: "i2pr-netdb",
        snapshot: "floodfill mode declaration",
        max_items: 0,
        max_bytes: 32,
        sensitivity: "public",
        freshness: "static per configuration",
        availability: SourceAvailability::PublishedGated {
            owner: "netdb",
            owner_plan: "295",
        },
        test_id: "plan288_netdb_floodfill_mode_gated_until_published",
    },
    SourceRow {
        key: "transport.ntcp2.active_peers",
        return_type: ReturnType::List,
        owner: "transport owners",
        snapshot: "bounded NTCP2 peer list (none while the activation guard holds)",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public peer identifiers only; bounded count",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "transport",
            owner_plan: "295",
        },
        test_id: "plan288_ntcp2_peers_gated_until_published",
    },
    SourceRow {
        key: "transport.ssu2.active_sessions",
        return_type: ReturnType::List,
        owner: "transport owners",
        snapshot: "bounded SSU2 session list",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public peer identifiers only; bounded count; no session secrets",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "transport",
            owner_plan: "295",
        },
        test_id: "plan288_ssu2_sessions_gated_until_published",
    },
    SourceRow {
        key: "transport.reachability",
        return_type: ReturnType::String,
        owner: "transport owners",
        snapshot: "reachability/testing state declaration",
        max_items: 0,
        max_bytes: 32,
        sensitivity: "public",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "transport",
            owner_plan: "295",
        },
        test_id: "plan288_reachability_gated_until_published",
    },
    SourceRow {
        key: "transport.errors",
        return_type: ReturnType::List,
        owner: "transport owners",
        snapshot: "bounded transport error list",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public counters only; no peer addresses or key material",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "transport",
            owner_plan: "295",
        },
        test_id: "plan288_transport_errors_gated_until_published",
    },
    SourceRow {
        key: "tunnel.exploratory.count",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded exploratory tunnel count entries",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "tunnels",
            owner_plan: "295",
        },
        test_id: "plan288_exploratory_count_gated_until_published",
    },
    SourceRow {
        key: "tunnel.client.count",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded client tunnel count entries",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "tunnels",
            owner_plan: "295",
        },
        test_id: "plan288_client_count_gated_until_published",
    },
    SourceRow {
        key: "tunnel.participating.count",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded participating tunnel count entries",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "tunnels",
            owner_plan: "295",
        },
        test_id: "plan288_participating_count_gated_until_published",
    },
    SourceRow {
        key: "tunnel.build_queue",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded build-queue depth entries",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "tunnels",
            owner_plan: "295",
        },
        test_id: "plan288_build_queue_gated_until_published",
    },
    SourceRow {
        key: "tunnel.success_rate",
        return_type: ReturnType::Map,
        owner: "tunnel owners",
        snapshot: "request-independent bounded success-rate metric",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "public counters only",
        freshness: "rolling metric owned by the metric owner, not computed on demand",
        availability: SourceAvailability::PublishedGated {
            owner: "tunnels",
            owner_plan: "295",
        },
        test_id: "plan288_success_rate_gated_until_published",
    },
    SourceRow {
        key: "tunnel.bandwidth",
        return_type: ReturnType::Map,
        owner: "transport owners",
        snapshot: "bounded bandwidth snapshot",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "public counters only",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "transport",
            owner_plan: "295",
        },
        test_id: "plan288_bandwidth_gated_until_published",
    },
    SourceRow {
        key: "addressbook.private",
        return_type: ReturnType::Map,
        owner: "canonical AddressBook",
        snapshot: "committed generation",
        max_items: 1000,
        max_bytes: 4_500_000,
        sensitivity: "naming material; owned by Plan 294 redaction rules",
        freshness: "per committed generation",
        availability: SourceAvailability::Available,
        test_id: "plan294_addressbook_private_available",
    },
    SourceRow {
        key: "addressbook.local",
        return_type: ReturnType::Map,
        owner: "canonical AddressBook",
        snapshot: "committed generation",
        max_items: 1000,
        max_bytes: 4_500_000,
        sensitivity: "naming material; owned by Plan 294 redaction rules",
        freshness: "per committed generation",
        availability: SourceAvailability::Available,
        test_id: "plan294_addressbook_local_available",
    },
    SourceRow {
        key: "addressbook.router",
        return_type: ReturnType::Map,
        owner: "canonical AddressBook",
        snapshot: "committed generation",
        max_items: 1000,
        max_bytes: 4_500_000,
        sensitivity: "naming material; owned by Plan 294 redaction rules",
        freshness: "per committed generation",
        availability: SourceAvailability::Available,
        test_id: "plan294_addressbook_router_available",
    },
    SourceRow {
        key: "addressbook.published",
        return_type: ReturnType::Map,
        owner: "canonical AddressBook",
        snapshot: "committed generation",
        max_items: 1000,
        max_bytes: 4_500_000,
        sensitivity: "naming material; owned by Plan 294 redaction rules",
        freshness: "per committed generation",
        availability: SourceAvailability::Available,
        test_id: "plan294_addressbook_published_available",
    },
    SourceRow {
        key: "addressbook.subscriptions",
        return_type: ReturnType::Map,
        owner: "canonical AddressBook",
        snapshot: "committed generation",
        max_items: 16,
        max_bytes: 65_536,
        sensitivity: "naming material; owned by Plan 294 redaction rules",
        freshness: "per committed generation",
        availability: SourceAvailability::Available,
        test_id: "plan294_addressbook_subscriptions_available",
    },
    SourceRow {
        key: "addressbook.config",
        return_type: ReturnType::Map,
        owner: "canonical AddressBook",
        snapshot: "committed generation",
        max_items: 13,
        max_bytes: 16_384,
        sensitivity: "naming material; owned by Plan 294 redaction rules",
        freshness: "per committed generation",
        availability: SourceAvailability::Available,
        test_id: "plan294_addressbook_config_available",
    },
    SourceRow {
        key: "logs.recent",
        return_type: ReturnType::Map,
        owner: "logging subsystem (Plan 295)",
        snapshot: "no safe bounded log owner exists",
        max_items: 256,
        max_bytes: 65536,
        sensitivity: "log content may carry secrets; no owner has approved a bounded view",
        freshness: "unavailable",
        availability: SourceAvailability::Unavailable {
            owner_plan: "295",
            reason: "fabricating an empty log view would hide router state; residual Plan 295 row",
        },
        test_id: "plan288_logs_recent_unavailable_until_295",
    },
    SourceRow {
        key: "news.feed",
        return_type: ReturnType::String,
        owner: "router news (Plan 295)",
        snapshot: "no authenticated news owner exists",
        max_items: 0,
        max_bytes: 65536,
        sensitivity: "unavailable",
        freshness: "unavailable",
        availability: SourceAvailability::Unavailable {
            owner_plan: "295",
            reason: "no authenticated news source exists; residual Plan 295 row",
        },
        test_id: "plan288_news_feed_unavailable_until_295",
    },
    SourceRow {
        key: "network.clock_skew",
        return_type: ReturnType::Integer,
        owner: "transport owners",
        snapshot: "clock-skew observation (neutral disposition undecided)",
        max_items: 0,
        max_bytes: 8,
        sensitivity: "public",
        freshness: "point-in-time per request",
        availability: SourceAvailability::PublishedGated {
            owner: "transport",
            owner_plan: "295",
        },
        test_id: "plan288_clock_skew_gated_until_published",
    },
    SourceRow {
        key: "network.banned_peers",
        return_type: ReturnType::List,
        owner: "ban facility (none exists)",
        snapshot: "no ban owner exists",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "peer identifiers would be public, but no owner may attest them",
        freshness: "unavailable",
        availability: SourceAvailability::Unavailable {
            owner_plan: "295",
            reason: "i2pr has no ban facility; an authoritative empty result requires an explicit ban owner first",
        },
        test_id: "plan288_banned_peers_unavailable_without_ban_owner",
    },
    SourceRow {
        key: "network.rates",
        return_type: ReturnType::Map,
        owner: "rate owner (Plan 295)",
        snapshot: "no bounded rate owner exists",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "public counters only",
        freshness: "unavailable",
        availability: SourceAvailability::PublishedGated {
            owner: "rates",
            owner_plan: "295",
        },
        test_id: "plan288_rates_gated_until_published",
    },
];

/// Exact ClientServicesInfo source matrix in canonical service order
/// (6 rows). Every row is answerable: disabled/absent services report
/// truthful disabled state rather than failing, because service
/// enablement is authoritative configuration truth.
pub const CLIENT_SERVICES_SOURCE_MATRIX: [SourceRow; 6] = [
    SourceRow {
        key: "I2PTunnel",
        return_type: ReturnType::Map,
        owner: "service-tunnel inventory",
        snapshot: "startup specs plus control-owned inventory plus live manager overlay",
        max_items: 256,
        max_bytes: 65536,
        sensitivity: "service names and public binds only; no destination secrets, no peer addresses",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_i2ptunnel_reports_startup_inventory",
    },
    SourceRow {
        key: "HTTPProxy",
        return_type: ReturnType::Map,
        owner: "service-tunnel inventory",
        snapshot: "http-client specs plus live manager overlay",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "bind metadata only; no proxy credentials",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_httpproxy_reports_actual_state",
    },
    SourceRow {
        key: "SOCKS",
        return_type: ReturnType::Map,
        owner: "service-tunnel inventory",
        snapshot: "socks5-client specs plus live manager overlay",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "bind metadata only; no proxy credentials",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_socks_reports_actual_state",
    },
    SourceRow {
        key: "SAM",
        return_type: ReturnType::Map,
        owner: "SAM bridge",
        snapshot: "SAM config plus bounded live session list",
        max_items: 64,
        max_bytes: 16384,
        sensitivity: "session ids and counts only; no private destinations, no socket peer addresses",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_sam_reports_enabled_and_bounded_sessions",
    },
    SourceRow {
        key: "BOB",
        return_type: ReturnType::Map,
        owner: "contract constant",
        snapshot: "deliberate constant (no BOB service exists; Java BOB deprecated)",
        max_items: 0,
        max_bytes: 64,
        sensitivity: "public constant",
        freshness: "static",
        availability: SourceAvailability::Available,
        test_id: "plan288_bob_is_deliberate_constant",
    },
    SourceRow {
        key: "I2CP",
        return_type: ReturnType::Map,
        owner: "I2CP bridge",
        snapshot: "I2CP config plus bounded live session counts",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "counts only; no secret material",
        freshness: "point-in-time per request",
        availability: SourceAvailability::Available,
        test_id: "plan288_i2cp_reports_enabled_and_counts",
    },
];

/// Number of permitted-neutral RouterInfo rows closed by Plan 288.
///
/// Strictness is the default: every neutrality requires an explicit
/// protocol justification, and Plan 288 records none.
pub const SOURCE_MATRIX_NEUTRAL_COUNT: usize = 0;

/// Looks up the matrix row for one selector.
pub const fn source_row(selector: RouterInfoSelector) -> SourceRow {
    ROUTER_INFO_SOURCE_MATRIX[selector_index(selector)]
}

/// Looks up the matrix row for one service.
pub const fn service_row(service: ClientService) -> SourceRow {
    CLIENT_SERVICES_SOURCE_MATRIX[service_index(service)]
}

/// Canonical matrix index of one selector (matches
/// [`ROUTER_INFO_SELECTORS`] order).
pub const fn selector_index(selector: RouterInfoSelector) -> usize {
    match selector {
        RouterInfoSelector::RouterVersion => 0,
        RouterInfoSelector::RouterApiVersion => 1,
        RouterInfoSelector::RouterUptime => 2,
        RouterInfoSelector::RouterStatus => 3,
        RouterInfoSelector::RouterNetworkId => 4,
        RouterInfoSelector::RouterHash => 5,
        RouterInfoSelector::NetDbKnownPeers => 6,
        RouterInfoSelector::NetDbActivePeers => 7,
        RouterInfoSelector::NetDbFloodfillMode => 8,
        RouterInfoSelector::Ntcp2ActivePeers => 9,
        RouterInfoSelector::Ssu2ActiveSessions => 10,
        RouterInfoSelector::Reachability => 11,
        RouterInfoSelector::TransportErrors => 12,
        RouterInfoSelector::ExploratoryCount => 13,
        RouterInfoSelector::ClientCount => 14,
        RouterInfoSelector::ParticipatingCount => 15,
        RouterInfoSelector::BuildQueue => 16,
        RouterInfoSelector::SuccessRate => 17,
        RouterInfoSelector::Bandwidth => 18,
        RouterInfoSelector::AddressBookPrivate => 19,
        RouterInfoSelector::AddressBookLocal => 20,
        RouterInfoSelector::AddressBookRouter => 21,
        RouterInfoSelector::AddressBookPublished => 22,
        RouterInfoSelector::AddressBookSubscriptions => 23,
        RouterInfoSelector::AddressBookConfig => 24,
        RouterInfoSelector::LogsRecent => 25,
        RouterInfoSelector::NewsFeed => 26,
        RouterInfoSelector::ClockSkew => 27,
        RouterInfoSelector::BannedPeers => 28,
        RouterInfoSelector::Rates => 29,
    }
}

/// Canonical matrix index of one service (matches [`CLIENT_SERVICES`]
/// order).
pub const fn service_index(service: ClientService) -> usize {
    match service {
        ClientService::I2pTunnel => 0,
        ClientService::HttpProxy => 1,
        ClientService::Socks => 2,
        ClientService::Sam => 3,
        ClientService::Bob => 4,
        ClientService::I2cp => 5,
    }
}

/// Asserts the matrix mirrors the frozen inventories key-for-key in
/// canonical order.
pub fn matrix_mirrors_inventories() -> bool {
    if ROUTER_INFO_SOURCE_MATRIX.len() != ROUTER_INFO_SELECTORS.len() {
        return false;
    }
    if CLIENT_SERVICES_SOURCE_MATRIX.len() != CLIENT_SERVICES.len() {
        return false;
    }
    let mut index = 0;
    for selector in [
        RouterInfoSelector::RouterVersion,
        RouterInfoSelector::RouterApiVersion,
        RouterInfoSelector::RouterUptime,
        RouterInfoSelector::RouterStatus,
        RouterInfoSelector::RouterNetworkId,
        RouterInfoSelector::RouterHash,
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
        RouterInfoSelector::AddressBookPrivate,
        RouterInfoSelector::AddressBookLocal,
        RouterInfoSelector::AddressBookRouter,
        RouterInfoSelector::AddressBookPublished,
        RouterInfoSelector::AddressBookSubscriptions,
        RouterInfoSelector::AddressBookConfig,
        RouterInfoSelector::LogsRecent,
        RouterInfoSelector::NewsFeed,
        RouterInfoSelector::ClockSkew,
        RouterInfoSelector::BannedPeers,
        RouterInfoSelector::Rates,
    ] {
        let row = ROUTER_INFO_SOURCE_MATRIX[index];
        if row.key != ROUTER_INFO_SELECTORS[index]
            || row.key != selector.name()
            || row.return_type != selector.return_type()
            || selector_index(selector) != index
        {
            return false;
        }
        index += 1;
    }
    if index != ROUTER_INFO_SELECTORS.len() {
        return false;
    }
    let mut service_idx = 0;
    for service in [
        ClientService::I2pTunnel,
        ClientService::HttpProxy,
        ClientService::Socks,
        ClientService::Sam,
        ClientService::Bob,
        ClientService::I2cp,
    ] {
        let row = CLIENT_SERVICES_SOURCE_MATRIX[service_idx];
        if row.key != CLIENT_SERVICES[service_idx]
            || row.key != service.name()
            || service_index(service) != service_idx
        {
            return false;
        }
        service_idx += 1;
    }
    service_idx == CLIENT_SERVICES.len()
}
