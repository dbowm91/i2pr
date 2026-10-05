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
/// positive or fail-closed test proves the row; missing evidence is
/// represented rather than replaced with an inventory-only check.
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
/// cannot silently drift apart. Unavailable rows carry evidence for their
/// typed fail-closed response, not for a fabricated value source.
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
            "bootstrap local RouterInfo publisher",
            "canonical bounded serialized local RouterInfo published after bootstrap",
            0,
            1_048_576,
            "public router information",
            "latest bootstrap local RouterInfo snapshot",
            SourceAvailability::PublishedGated {
                owner: "bootstrap local RouterInfo publisher",
                owner_plan: "322",
            },
            Some("proposal_local_router_info_is_bounded_and_publish_gated"),
        ),
        "i2p.router.news" => (
            "daemon signed NEWS manager",
            "bounded verified SU3 Atom snapshot with validators and last-known-good fallback",
            0,
            524_288,
            "public authenticated router news",
            "last verified feed; refresh status and staleness tracked separately",
            SourceAvailability::PublishedGated {
                owner: "signed NEWS cache",
                owner_plan: "322",
            },
            Some("authenticated_news_verifies_before_parse_and_survives_304_restart"),
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
        "i2p.router.netdb.ntcp.limit" | "i2p.router.netdb.ssu.limit" => (
            "validated transport runtime configuration",
            "configured NTCP2 active-link or SSU2 active-session admission ceiling",
            0,
            20,
            "public local resource ceiling; does not imply enabled or advertised support",
            "fixed by validated daemon configuration at startup",
            SourceAvailability::Available,
            Some("router_info_transport_limits_follow_validated_config_over_wire"),
        ),
        "i2p.router.net.total.transit.bytes" => (
            "controlled TransitBuildService qualification gate",
            "no production transit-byte counter is installed; qualification counters are not a production source",
            0,
            20,
            "aggregate transit volume",
            "unavailable outside the explicitly controlled transit lane",
            SourceAvailability::Unavailable {
                owner_plan: "322",
                reason: "the transit service is not installed in production composition",
            },
            Some("proposal_unavailable_sources_fail_closed_over_wire"),
        ),
        "i2p.router.net.bw.transit.15s" => (
            "transit bandwidth sampler",
            "no production rolling 15-second transit-byte window exists",
            0,
            20,
            "aggregate transit bandwidth",
            "unavailable until a bounded production transit sampler exists",
            SourceAvailability::Unavailable {
                owner_plan: "322",
                reason: "no production transit bandwidth window is maintained",
            },
            Some("proposal_unavailable_sources_fail_closed_over_wire"),
        ),
        "i2p.router.net.tunnels.shareratio" => (
            "transit participation metrics",
            "no production transit volume and tunnel-share numerator/denominator are maintained",
            0,
            8,
            "aggregate participation ratio",
            "unavailable until the router owns production transit metrics",
            SourceAvailability::Unavailable {
                owner_plan: "322",
                reason: "no authoritative transit share-ratio inputs are maintained",
            },
            Some("proposal_unavailable_sources_fail_closed_over_wire"),
        ),
        "i2p.router.net.status.v6" => (
            "SSU2 per-family reachability condition",
            "i2pd-adopted RouterStatus code for IPv6; OK only from a fresh IPv6-qualified snapshot, Firewalled only from corroborated IPv6 unreachability, otherwise Unknown (2); Proxy/Mesh/Stan never emitted",
            0,
            20,
            "public local connectivity state; code only, no endpoint or peer identity",
            "per request from the runtime-owned condition; unsupported by evidence, it reports Unknown",
            SourceAvailability::PublishedGated {
                owner: "ssu2 per-family network condition",
                owner_plan: "339",
            },
            Some("proposal_per_family_network_condition_over_wire"),
        ),
        "i2p.router.net.error" | "i2p.router.net.error.v6" => (
            "SSU2 per-family condition + attested NetDB peer snapshot",
            "i2pd-adopted RouterError code; NoDescriptors (5) on an attested empty NetDB, Offline (2) only when a family was configured but never bound, otherwise None (0); ClockSkew/SymmetricNAT/FullConeNAT never emitted because i2pr owns no such detector",
            0,
            20,
            "public local connectivity diagnostic; code only",
            "per request; an unattested NetDB fails the request closed rather than claiming a code",
            SourceAvailability::PublishedGated {
                owner: "ssu2 per-family network condition",
                owner_plan: "339",
            },
            Some("proposal_per_family_network_condition_over_wire"),
        ),
        "i2p.router.net.testing" | "i2p.router.net.testing.v6" => (
            "SSU2 per-family reachability condition",
            "1 only while a determination is genuinely in progress (ObservedUnconfirmed/CandidateReachable) for that family; 0 otherwise, including a router that has never observed anything",
            0,
            20,
            "public local reachability state; 0 or 1 only",
            "per request from the runtime-owned condition",
            SourceAvailability::PublishedGated {
                owner: "ssu2 per-family network condition",
                owner_plan: "339",
            },
            Some("proposal_per_family_network_condition_over_wire"),
        ),
        "i2p.router.netdb.activepeers.info" | "i2p.router.netdb.peers.info" => (
            "NetDB inspection snapshot",
            "empty serialized list follows an attested empty peer-hash set; nonempty peer sets require serialized RouterInfo data",
            1024,
            65_536,
            "public router information",
            "latest peer snapshot; fails closed when peers exist without serialized data",
            SourceAvailability::PublishedGated {
                owner: "serialized RouterInfo snapshot for known peers",
                owner_plan: "322",
            },
            Some("proposal_empty_router_info_lists_require_empty_attested_peer_sets"),
        ),
        "i2p.router.netdb.activepeers.stats" => (
            "NetDB inspection snapshot",
            "empty stats list follows an attested empty active-peer set; nonempty peers require the stats detail owner",
            1024,
            65_536,
            "public peer statistics",
            "latest active-peer snapshot; fails closed when peers exist without stats",
            SourceAvailability::PublishedGated {
                owner: "active-peer stats detail snapshot",
                owner_plan: "322",
            },
            Some("proposal_empty_peer_stats_and_bans_require_attested_empty_sources"),
        ),
        "i2p.router.netdb.bannedpeers" => (
            "Plan 295 ban ledger",
            "empty details map follows an attested empty ban set; populated hashes require reason and expiry details",
            1024,
            65_536,
            "peer identifiers and ban reasons",
            "latest attested ban set; fails closed when detail is absent",
            SourceAvailability::PublishedGated {
                owner: "ban reason and expiry detail snapshot",
                owner_plan: "322",
            },
            Some("proposal_empty_peer_stats_and_bans_require_attested_empty_sources"),
        ),
        "i2p.router.net.tunnels.successrate" | "i2p.router.net.tunnels.totalsuccessrate" => (
            "ControlMetrics tunnel-build outcomes",
            "latest interval ratio / cumulative ratio; unavailable until attempted > 0",
            0,
            24,
            "public aggregate ratio",
            "cumulative at request time",
            SourceAvailability::PublishedGated {
                owner: "ControlMetrics tunnel-build outcomes",
                owner_plan: "322",
            },
            Some("proposal_success_rates_require_attempts_and_read_metrics"),
        ),
        "i2p.router.net.tunnels.queue" => (
            "Plan 295 tunnel build-queue snapshot",
            "attested scalar build queue depth",
            0,
            20,
            "public aggregate queue depth",
            "attested at composition and read at request time",
            SourceAvailability::Available,
            Some("proposal_tunnel_queue_depth_uses_attested_snapshot"),
        ),
        "i2p.router.net.tunnels.tbmqueue" => (
            "Tunnel Build Message queue owner",
            "independent bounded queue-depth snapshot (zero in the composed graph without a tunnel-build coordinator)",
            0,
            20,
            "public aggregate queue depth",
            "latest attested queue depth",
            SourceAvailability::Available,
            Some("proposal_tbm_queue_depth_uses_independent_attested_snapshot"),
        ),
        "i2p.router.net.tunnels.exploratory.inbound"
        | "i2p.router.net.tunnels.exploratory.outbound"
        | "i2p.router.net.tunnels.exploratory.info.list"
        | "i2p.router.net.tunnels.client.inbound"
        | "i2p.router.net.tunnels.client.outbound"
        | "i2p.router.net.tunnels.client.info.list"
        | "i2p.router.net.tunnels.participating.info" => (
            "Plan 295 tunnel-count snapshot",
            "zero aggregate projects zero directional count or empty details; nonzero requires a missing detail snapshot",
            if list { 1024 } else { 0 },
            if list { 65_536 } else { 20 },
            "public tunnel counts or local tunnel detail",
            "attested aggregate at composition; fail closed if nonzero",
            SourceAvailability::PublishedGated {
                owner: "per-direction and per-tunnel inspection snapshot",
                owner_plan: "322",
            },
            Some("proposal_empty_tunnel_projection_requires_zero_aggregate"),
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
            Some("proposal_unavailable_sources_fail_closed_over_wire"),
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
