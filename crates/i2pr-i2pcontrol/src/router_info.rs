//! Historical typed RouterInfo owner selectors and internal return types.
//!
//! These normalized names route internally to established owners. They are
//! not public wire spellings and are not accepted by the canonical endpoint;
//! see [`crate::proposal_wire`] for the external base API and Proposal 170
//! inventories. Unavailable sources fail whole-request rather than fabricate
//! zero/empty (Plan 288 rule).

use crate::errors::ContractError;

/// Declared return type metadata for one selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReturnType {
    /// UTF-8 string.
    String,
    /// Signed 64-bit integer.
    Integer,
    /// Boolean.
    Boolean,
    /// Bounded list.
    List,
    /// Bounded map/object.
    Map,
}

/// Exact frozen selector inventory in canonical order (30 entries).
pub const ROUTER_INFO_SELECTORS: [&str; 30] = [
    "router.version",
    "router.api_version",
    "router.uptime",
    "router.status",
    "router.network_id",
    "router.hash",
    "netdb.known_peers",
    "netdb.active_peers",
    "netdb.floodfill_mode",
    "transport.ntcp2.active_peers",
    "transport.ssu2.active_sessions",
    "transport.reachability",
    "transport.errors",
    "tunnel.exploratory.count",
    "tunnel.client.count",
    "tunnel.participating.count",
    "tunnel.build_queue",
    "tunnel.success_rate",
    "tunnel.bandwidth",
    "addressbook.private",
    "addressbook.local",
    "addressbook.router",
    "addressbook.published",
    "addressbook.subscriptions",
    "addressbook.config",
    "logs.recent",
    "news.feed",
    "network.clock_skew",
    "network.banned_peers",
    "network.rates",
];

/// Typed selector handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterInfoSelector {
    /// Router version string.
    RouterVersion,
    /// Control-plane API version declaration.
    RouterApiVersion,
    /// Uptime in seconds.
    RouterUptime,
    /// Liveness status string.
    RouterStatus,
    /// Network id.
    RouterNetworkId,
    /// Router hash (truncated/redacted per privacy review).
    RouterHash,
    /// Known NetDB peer count/identifiers.
    NetDbKnownPeers,
    /// Active NetDB peer view.
    NetDbActivePeers,
    /// Floodfill mode string.
    NetDbFloodfillMode,
    /// Active NTCP2 peer snapshot.
    Ntcp2ActivePeers,
    /// Active SSU2 session snapshot.
    Ssu2ActiveSessions,
    /// Reachability/testing state.
    Reachability,
    /// Transport error state.
    TransportErrors,
    /// Exploratory tunnel count.
    ExploratoryCount,
    /// Client tunnel count.
    ClientCount,
    /// Participating/transit tunnel count.
    ParticipatingCount,
    /// Build queue depth.
    BuildQueue,
    /// Success-rate metric.
    SuccessRate,
    /// Bandwidth snapshot.
    Bandwidth,
    /// Private address-book entries (Plan 294 source).
    AddressBookPrivate,
    /// Local address-book entries (Plan 294 source).
    AddressBookLocal,
    /// Router address-book entries (Plan 294 source).
    AddressBookRouter,
    /// Published address-book entries (Plan 294 source).
    AddressBookPublished,
    /// Subscription set (Plan 294 source).
    AddressBookSubscriptions,
    /// Address-book config (Plan 294 source).
    AddressBookConfig,
    /// Bounded recent-log view (Plan 295 source).
    LogsRecent,
    /// Authenticated router-news view (Plan 295 source).
    NewsFeed,
    /// Clock-skew observation (nullable/neutral where permitted).
    ClockSkew,
    /// Banned-peer view (explicit empty only with a ban owner).
    BannedPeers,
    /// Rate snapshot.
    Rates,
}

impl RouterInfoSelector {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::RouterVersion => "router.version",
            Self::RouterApiVersion => "router.api_version",
            Self::RouterUptime => "router.uptime",
            Self::RouterStatus => "router.status",
            Self::RouterNetworkId => "router.network_id",
            Self::RouterHash => "router.hash",
            Self::NetDbKnownPeers => "netdb.known_peers",
            Self::NetDbActivePeers => "netdb.active_peers",
            Self::NetDbFloodfillMode => "netdb.floodfill_mode",
            Self::Ntcp2ActivePeers => "transport.ntcp2.active_peers",
            Self::Ssu2ActiveSessions => "transport.ssu2.active_sessions",
            Self::Reachability => "transport.reachability",
            Self::TransportErrors => "transport.errors",
            Self::ExploratoryCount => "tunnel.exploratory.count",
            Self::ClientCount => "tunnel.client.count",
            Self::ParticipatingCount => "tunnel.participating.count",
            Self::BuildQueue => "tunnel.build_queue",
            Self::SuccessRate => "tunnel.success_rate",
            Self::Bandwidth => "tunnel.bandwidth",
            Self::AddressBookPrivate => "addressbook.private",
            Self::AddressBookLocal => "addressbook.local",
            Self::AddressBookRouter => "addressbook.router",
            Self::AddressBookPublished => "addressbook.published",
            Self::AddressBookSubscriptions => "addressbook.subscriptions",
            Self::AddressBookConfig => "addressbook.config",
            Self::LogsRecent => "logs.recent",
            Self::NewsFeed => "news.feed",
            Self::ClockSkew => "network.clock_skew",
            Self::BannedPeers => "network.banned_peers",
            Self::Rates => "network.rates",
        }
    }

    /// Declared return type metadata.
    pub const fn return_type(self) -> ReturnType {
        match self {
            Self::RouterVersion
            | Self::RouterStatus
            | Self::RouterHash
            | Self::NetDbFloodfillMode
            | Self::Reachability
            | Self::NewsFeed => ReturnType::String,
            Self::RouterApiVersion
            | Self::RouterUptime
            | Self::RouterNetworkId
            | Self::ClockSkew => ReturnType::Integer,
            Self::NetDbKnownPeers
            | Self::NetDbActivePeers
            | Self::Ntcp2ActivePeers
            | Self::Ssu2ActiveSessions
            | Self::TransportErrors
            | Self::ExploratoryCount
            | Self::ClientCount
            | Self::ParticipatingCount
            | Self::BuildQueue
            | Self::BannedPeers => ReturnType::List,
            Self::SuccessRate
            | Self::Bandwidth
            | Self::AddressBookPrivate
            | Self::AddressBookLocal
            | Self::AddressBookRouter
            | Self::AddressBookPublished
            | Self::AddressBookSubscriptions
            | Self::AddressBookConfig
            | Self::LogsRecent
            | Self::Rates => ReturnType::Map,
        }
    }

    /// Whether this selector reads the canonical AddressBook owner
    /// (unavailable until Plan 294 closes).
    pub const fn is_address_book(self) -> bool {
        matches!(
            self,
            Self::AddressBookPrivate
                | Self::AddressBookLocal
                | Self::AddressBookRouter
                | Self::AddressBookPublished
                | Self::AddressBookSubscriptions
                | Self::AddressBookConfig
        )
    }

    /// Parses an exact selector spelling (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "router.version" => Ok(Self::RouterVersion),
            "router.api_version" => Ok(Self::RouterApiVersion),
            "router.uptime" => Ok(Self::RouterUptime),
            "router.status" => Ok(Self::RouterStatus),
            "router.network_id" => Ok(Self::RouterNetworkId),
            "router.hash" => Ok(Self::RouterHash),
            "netdb.known_peers" => Ok(Self::NetDbKnownPeers),
            "netdb.active_peers" => Ok(Self::NetDbActivePeers),
            "netdb.floodfill_mode" => Ok(Self::NetDbFloodfillMode),
            "transport.ntcp2.active_peers" => Ok(Self::Ntcp2ActivePeers),
            "transport.ssu2.active_sessions" => Ok(Self::Ssu2ActiveSessions),
            "transport.reachability" => Ok(Self::Reachability),
            "transport.errors" => Ok(Self::TransportErrors),
            "tunnel.exploratory.count" => Ok(Self::ExploratoryCount),
            "tunnel.client.count" => Ok(Self::ClientCount),
            "tunnel.participating.count" => Ok(Self::ParticipatingCount),
            "tunnel.build_queue" => Ok(Self::BuildQueue),
            "tunnel.success_rate" => Ok(Self::SuccessRate),
            "tunnel.bandwidth" => Ok(Self::Bandwidth),
            "addressbook.private" => Ok(Self::AddressBookPrivate),
            "addressbook.local" => Ok(Self::AddressBookLocal),
            "addressbook.router" => Ok(Self::AddressBookRouter),
            "addressbook.published" => Ok(Self::AddressBookPublished),
            "addressbook.subscriptions" => Ok(Self::AddressBookSubscriptions),
            "addressbook.config" => Ok(Self::AddressBookConfig),
            "logs.recent" => Ok(Self::LogsRecent),
            "news.feed" => Ok(Self::NewsFeed),
            "network.clock_skew" => Ok(Self::ClockSkew),
            "network.banned_peers" => Ok(Self::BannedPeers),
            "network.rates" => Ok(Self::Rates),
            _ => {
                for known in ROUTER_INFO_SELECTORS {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }
}
