//! Plan 288 machine-readable legacy-source matrix: one row per normalized
//! RouterInfo selector (30) and ClientServicesInfo service (6). Plan 322's
//! canonical Proposal additions are inventoried separately by
//! [`proposal_router_info_source_matrix`].
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
use crate::proposal_wire::{PROPOSAL_ROUTER_INFO_FIELDS, ProposalValueType};
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

/// Source and availability status for one canonical Proposal 170
/// RouterInfo addition. `evidence_test` is absent until a source-specific
/// test proves the row; missing evidence is represented rather than
/// replaced with a test that only checks the inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposalSourceRow {
    /// Exact canonical Proposal key.
    pub key: &'static str,
    /// Proposal-declared JSON type.
    pub value_type: ProposalValueType,
    /// Authoritative subsystem or explicit pending owner.
    pub owner: &'static str,
    /// Bounded snapshot/read method.
    pub snapshot: &'static str,
    /// Maximum collection cardinality (zero for scalar values).
    pub max_items: usize,
    /// Maximum encoded bytes for the individual value.
    pub max_bytes: usize,
    /// Sensitivity and redaction rule.
    pub sensitivity: &'static str,
    /// Freshness semantics.
    pub freshness: &'static str,
    /// Current source availability.
    pub availability: SourceAvailability,
    /// Test proving this source row, when implemented.
    pub evidence_test: Option<&'static str>,
}

/// Current source authority for all 43 canonical RouterInfo additions.
/// This is generated in Proposal order so inventory and source coverage
/// cannot silently drift apart. Rows without a source remain explicitly
/// unavailable and carry no fabricated evidence identifier.
pub fn proposal_router_info_source_matrix() -> Vec<ProposalSourceRow> {
    PROPOSAL_ROUTER_INFO_FIELDS
        .iter()
        .map(|field| proposal_source_row(field.key, field.value_type))
        .collect()
}

fn proposal_source_row(key: &'static str, value_type: ProposalValueType) -> ProposalSourceRow {
    let list = matches!(
        value_type,
        ProposalValueType::StringList
            | ProposalValueType::ObjectList
            | ProposalValueType::NestedObject
    );
    let (
        owner,
        snapshot,
        max_items,
        max_bytes,
        sensitivity,
        freshness,
        availability,
        evidence_test,
    ) = match key {
        "i2p.router.id" => (
            "bootstrap identity",
            "published RouterHash snapshot",
            0,
            64,
            "public router hash",
            "published at bootstrap",
            SourceAvailability::PermittedNeutral {
                reason: "Proposal permits null before identity publication",
            },
            Some("router_info_hash_gated_then_published_over_wire"),
        ),
        "i2p.router.clockskew" => (
            "clock-skew observer",
            "no peer-skew sample is currently collected",
            0,
            8,
            "public aggregate",
            "null until an observation exists",
            SourceAvailability::PermittedNeutral {
                reason: "Proposal permits null when no peer-skew sample exists",
            },
            Some("router_info_proposal_selection_over_wire"),
        ),
        "i2p.router.info" => (
            "local RouterInfo publisher",
            "no serialized local RouterInfo is published",
            0,
            1_048_576,
            "public router information",
            "null until locally serialized RouterInfo is published",
            SourceAvailability::PermittedNeutral {
                reason: "Proposal permits null when no local RouterInfo is available",
            },
            Some("batch_isolation_with_inspection"),
        ),
        "i2p.router.logs" => (
            "daemon LogRing",
            "bounded redacted ring snapshot",
            256,
            65_536,
            "messages redacted before retention",
            "snapshot at request time",
            SourceAvailability::Available,
            Some("authenticated_router_info_logs_clear_clears_ring_and_returns_success"),
        ),
        "i2p.router.logs.clear" => (
            "daemon LogRing",
            "authenticated atomic ring clear",
            0,
            16,
            "mutation; no log content returned",
            "immediate at request time",
            SourceAvailability::Available,
            Some("authenticated_router_info_logs_clear_clears_ring_and_returns_success"),
        ),
        "i2p.router.net.total.received.bytes" | "i2p.router.net.total.sent.bytes" => (
            "SSU2 runtime counters",
            "ControlMetrics cumulative I2NP byte totals",
            0,
            20,
            "aggregate transport counters",
            "latest registered SSU2 sample; excludes uncovered transports",
            SourceAvailability::PublishedGated {
                owner: "SSU2 cumulative byte counters",
                owner_plan: "322",
            },
            Some("canonical_transport_totals_are_served_from_published_metrics"),
        ),
        "i2p.router.netdb.peers" | "i2p.router.netdb.peers.list" => (
            "NetDB inspection snapshot",
            "bounded known-peer hash snapshot",
            1024,
            65_536,
            "public router hashes",
            "latest published NetDB snapshot",
            SourceAvailability::Available,
            Some("differential_corpus_against_production_composition"),
        ),
        "i2p.router.net.tunnels.totalsuccessrate" => (
            "ControlMetrics cumulative build outcomes",
            "succeeded / attempted; unavailable until attempted > 0",
            0,
            24,
            "public aggregate ratio",
            "cumulative at request time",
            SourceAvailability::PublishedGated {
                owner: "ControlMetrics cumulative build outcomes",
                owner_plan: "322",
            },
            Some("proposal_total_success_rate_requires_attempts_and_reads_metrics"),
        ),
        "i2p.router.netdb.activepeers.list" => (
            "NetDB inspection snapshot",
            "bounded active-peer hash snapshot",
            1024,
            65_536,
            "public router hashes",
            "latest published NetDB snapshot",
            SourceAvailability::Available,
            Some("differential_corpus_against_production_composition"),
        ),
        "i2p.router.net.tunnels.i2ptunnel" => (
            "service tunnel inventory",
            "startup definitions plus live manager overlay",
            64,
            65_536,
            "local service names, kinds, loopback binds, and lifecycle state",
            "point-in-time inventory snapshot",
            SourceAvailability::Available,
            Some("proposal_i2ptunnel_summaries_are_bounded_and_canonical"),
        ),
        "i2p.router.addressbook.private.list"
        | "i2p.router.addressbook.local.list"
        | "i2p.router.addressbook.router.list"
        | "i2p.router.addressbook.published.list"
        | "i2p.router.addressbook.subscriptions"
        | "i2p.router.addressbook.config" => (
            "Plan 321 AddressBookManager",
            "committed generation snapshot",
            1024,
            4_500_000,
            "private/local names and destinations; bounded serialized values",
            "one committed AddressBook generation",
            SourceAvailability::PublishedGated {
                owner: "active AddressBookManager",
                owner_plan: "321",
            },
            Some("plan294_addressbook_method_drives_the_canonical_owner"),
        ),
        _ => (
            "Plan 322 source not implemented",
            "no authoritative bounded snapshot is wired",
            if list { 1024 } else { 0 },
            if list { 1_048_576 } else { 128 },
            "not available until a truthful owner is established",
            "unavailable",
            SourceAvailability::Unavailable {
                owner_plan: "322",
                reason: "no authoritative source is wired; zero/empty would be fabricated",
            },
            None,
        ),
    };
    ProposalSourceRow {
        key,
        value_type,
        owner,
        snapshot,
        max_items,
        max_bytes,
        sensitivity,
        freshness,
        availability,
        evidence_test,
    }
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
        snapshot: "bounded known-peer hash list attested at composition (empty: no learning paths in the default graph)",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public router hashes only; bounded count",
        freshness: "attested at composition; gaps (never fabricates) before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_netdb_known_peers_attested",
    },
    SourceRow {
        key: "netdb.active_peers",
        return_type: ReturnType::List,
        owner: "i2pr-netdb",
        snapshot: "bounded active-peer hash list attested at composition (empty: no learning paths in the default graph)",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public router hashes only; bounded count",
        freshness: "attested at composition; gaps (never fabricates) before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_netdb_active_peers_attested",
    },
    SourceRow {
        key: "netdb.floodfill_mode",
        return_type: ReturnType::String,
        owner: "i2pr-netdb",
        snapshot: "floodfill mode declaration (disabled: normal configuration never constructs a permit)",
        max_items: 0,
        max_bytes: 32,
        sensitivity: "public",
        freshness: "static per configuration; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_netdb_floodfill_mode_declared",
    },
    SourceRow {
        key: "transport.ntcp2.active_peers",
        return_type: ReturnType::List,
        owner: "transport owners",
        snapshot: "bounded NTCP2 peer list (empty: activation guard holds, no NTCP2 service exists)",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public peer identifiers only; bounded count",
        freshness: "attested at composition; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_ntcp2_peers_empty_under_guard",
    },
    SourceRow {
        key: "transport.ssu2.active_sessions",
        return_type: ReturnType::List,
        owner: "transport owners",
        snapshot: "live SSU2 session peer hashes via the cloned runtime service; attested empty when no SSU2 service registers",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public peer identifiers only; bounded count; no session secrets",
        freshness: "point-in-time per request when live, attested at composition otherwise",
        availability: SourceAvailability::Available,
        test_id: "plan295_ssu2_sessions_live_or_attested",
    },
    SourceRow {
        key: "transport.reachability",
        return_type: ReturnType::String,
        owner: "transport owners",
        snapshot: "reachability declaration (loopback-only: transports bind loopback only)",
        max_items: 0,
        max_bytes: 32,
        sensitivity: "public",
        freshness: "static per configuration; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_reachability_loopback_declaration",
    },
    SourceRow {
        key: "transport.errors",
        return_type: ReturnType::List,
        owner: "transport owners",
        snapshot: "live SSU2 non-zero error counters (name=value); attested empty when no SSU2 service registers",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "public counters only; no peer addresses or key material",
        freshness: "point-in-time per request when live, attested at composition otherwise",
        availability: SourceAvailability::Available,
        test_id: "plan295_transport_errors_live_or_attested",
    },
    SourceRow {
        key: "tunnel.exploratory.count",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded exploratory tunnel count entries attested at composition (zero: no coordinators in the default graph)",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "attested at composition; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_exploratory_count_attested",
    },
    SourceRow {
        key: "tunnel.client.count",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded client tunnel count entries attested at composition (zero: no coordinators in the default graph)",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "attested at composition; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_client_count_attested",
    },
    SourceRow {
        key: "tunnel.participating.count",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded participating tunnel count entries attested at composition (zero: no transit owner in the default graph)",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "attested at composition; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_participating_count_attested",
    },
    SourceRow {
        key: "tunnel.build_queue",
        return_type: ReturnType::List,
        owner: "tunnel owners",
        snapshot: "bounded build-queue depth entries attested at composition (zero: no build coordinators in the default graph)",
        max_items: 1024,
        max_bytes: 16384,
        sensitivity: "public counts only",
        freshness: "attested at composition; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_build_queue_attested",
    },
    SourceRow {
        key: "tunnel.success_rate",
        return_type: ReturnType::Map,
        owner: "tunnel owners",
        snapshot: "rolling build-outcome pair from the control metrics owner ((0, 0): no build reporters in the default graph)",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "public counters only",
        freshness: "rolling metric owned by the metrics owner; ticks per request (O(1), no scan)",
        availability: SourceAvailability::Available,
        test_id: "plan295_success_rate_from_metrics",
    },
    SourceRow {
        key: "tunnel.bandwidth",
        return_type: ReturnType::Map,
        owner: "transport owners",
        snapshot: "rolling bandwidth pair from the control metrics owner (SSU2 counters once observed)",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "public counters only",
        freshness: "rolling metric owned by the metrics owner; ticks per request (O(1), no scan)",
        availability: SourceAvailability::Available,
        test_id: "plan295_bandwidth_from_metrics",
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
        owner: "daemon log ring (Plan 295)",
        snapshot: "live redacted ring snapshot (INFO and above, secret markers replaced, 256 entries, 192 B lines)",
        max_items: 256,
        max_bytes: 65536,
        sensitivity: "redacted log lines only; secret-marker messages replaced, DEBUG/TRACE excluded by policy",
        freshness: "point-in-time per request; gaps (never fabricates) before ring publication",
        availability: SourceAvailability::Available,
        test_id: "plan295_logs_recent_from_ring",
    },
    SourceRow {
        key: "news.feed",
        return_type: ReturnType::String,
        owner: "router news (none served)",
        snapshot: "never served: i2pr fetches no router news and holds no authenticated feed",
        max_items: 0,
        max_bytes: 65536,
        sensitivity: "unavailable",
        freshness: "unavailable",
        availability: SourceAvailability::Unavailable {
            owner_plan: "295",
            reason: "i2pr serves no router news by design; no authenticated source exists; explicit Plan 295 unavailable policy",
        },
        test_id: "plan295_news_feed_unsupported_by_design",
    },
    SourceRow {
        key: "network.clock_skew",
        return_type: ReturnType::Integer,
        owner: "i2pcontrol contract",
        snapshot: "neutral constant 0 (declared, never measured: no peer clocks observed)",
        max_items: 0,
        max_bytes: 8,
        sensitivity: "public",
        freshness: "static neutral",
        availability: SourceAvailability::PermittedNeutral {
            reason: "Proposal permits null average peer skew with no peers; integer wire encodes unmeasured as 0",
        },
        test_id: "plan295_clock_skew_neutral",
    },
    SourceRow {
        key: "network.banned_peers",
        return_type: ReturnType::List,
        owner: "explicit ban ledger (Plan 295)",
        snapshot: "attested ban set (empty: no reporters and no ban criteria exist)",
        max_items: 1024,
        max_bytes: 65536,
        sensitivity: "peer identifiers would be public, but the ledger holds none",
        freshness: "attested at composition; gaps before attestation",
        availability: SourceAvailability::Available,
        test_id: "plan295_banned_peers_attested_empty",
    },
    SourceRow {
        key: "network.rates",
        return_type: ReturnType::Map,
        owner: "control metrics owner (Plan 295)",
        snapshot: "rolling rate map from the control metrics owner (ssu2.* entries once observed; empty otherwise)",
        max_items: 8,
        max_bytes: 1024,
        sensitivity: "public counters only",
        freshness: "rolling metric owned by the metrics owner; ticks per request (O(1), no scan)",
        availability: SourceAvailability::Available,
        test_id: "plan295_rates_from_metrics",
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
/// protocol justification, and Plan 288 records none. Plan 295
/// justifies exactly one: `network.clock_skew` emits the neutral
/// constant because the Proposal permits a null average peer skew
/// and no peer clocks are ever observed.
pub const SOURCE_MATRIX_NEUTRAL_COUNT: usize = 1;

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
