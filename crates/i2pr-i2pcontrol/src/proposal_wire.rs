//! Canonical Proposal 170 wire inventory.
//!
//! Internal domain names in older modules are not public wire aliases. This
//! table is the default RouterInfo namespace and records Proposal-declared
//! JSON result shapes independently from current source availability.

use crate::router_info::RouterInfoSelector;

/// Exact JSON result shape declared by Proposal 170.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalValueType {
    /// JSON string.
    String,
    /// JSON signed integer.
    Integer,
    /// JSON boolean.
    Boolean,
    /// JSON floating point number.
    Double,
    /// JSON string array.
    StringList,
    /// JSON string-keyed object.
    Object,
    /// JSON array of objects.
    ObjectList,
    /// JSON map of maps.
    NestedObject,
}

/// One canonical RouterInfo selector and its currently available domain
/// adapter, if an existing owner can answer it truthfully.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposalRouterInfoField {
    /// Exact Proposal key.
    pub key: &'static str,
    /// Exact declared return type.
    pub value_type: ProposalValueType,
    /// Existing typed owner adapter. `None` means the selector is valid
    /// wire vocabulary but its truthful source belongs to a later plan.
    pub adapter: Option<RouterInfoSelector>,
}

/// Exact 43-field Proposal 170 RouterInfo addition inventory.
pub const PROPOSAL_ROUTER_INFO_FIELDS: [ProposalRouterInfoField; 43] = [
    field(
        "i2p.router.news",
        ProposalValueType::String,
        Some(RouterInfoSelector::NewsFeed),
    ),
    field(
        "i2p.router.id",
        ProposalValueType::String,
        Some(RouterInfoSelector::RouterHash),
    ),
    field(
        "i2p.router.clockskew",
        ProposalValueType::Integer,
        Some(RouterInfoSelector::ClockSkew),
    ),
    field("i2p.router.info", ProposalValueType::String, None),
    field("i2p.router.logs", ProposalValueType::StringList, None),
    field("i2p.router.logs.clear", ProposalValueType::String, None),
    field(
        "i2p.router.net.total.received.bytes",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.total.sent.bytes",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.total.transit.bytes",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.bw.transit.15s",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.shareratio",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.tunnels.participating.info",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.net.tunnels.i2ptunnel",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.net.tunnels.exploratory.inbound",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.exploratory.outbound",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.exploratory.info.list",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.net.tunnels.client.inbound",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.client.outbound",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.client.info.list",
        ProposalValueType::ObjectList,
        None,
    ),
    field("i2p.router.net.status.v6", ProposalValueType::Integer, None),
    field("i2p.router.net.error", ProposalValueType::Integer, None),
    field("i2p.router.net.error.v6", ProposalValueType::Integer, None),
    field("i2p.router.net.testing", ProposalValueType::Integer, None),
    field(
        "i2p.router.net.testing.v6",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.successrate",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.tunnels.totalsuccessrate",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.tunnels.queue",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.net.tunnels.tbmqueue",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.netdb.peers",
        ProposalValueType::StringList,
        Some(RouterInfoSelector::NetDbKnownPeers),
    ),
    field(
        "i2p.router.netdb.activepeers.info",
        ProposalValueType::StringList,
        None,
    ),
    field(
        "i2p.router.netdb.ntcp.limit",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.netdb.ssu.limit",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.netdb.bannedpeers",
        ProposalValueType::NestedObject,
        None,
    ),
    field(
        "i2p.router.netdb.activepeers.list",
        ProposalValueType::StringList,
        Some(RouterInfoSelector::NetDbActivePeers),
    ),
    field(
        "i2p.router.netdb.peers.list",
        ProposalValueType::StringList,
        Some(RouterInfoSelector::NetDbKnownPeers),
    ),
    field(
        "i2p.router.netdb.peers.info",
        ProposalValueType::StringList,
        None,
    ),
    field(
        "i2p.router.netdb.activepeers.stats",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.addressbook.private.list",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.addressbook.local.list",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.addressbook.router.list",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.addressbook.published.list",
        ProposalValueType::ObjectList,
        None,
    ),
    field(
        "i2p.router.addressbook.subscriptions",
        ProposalValueType::Object,
        None,
    ),
    field(
        "i2p.router.addressbook.config",
        ProposalValueType::Object,
        None,
    ),
];

/// Base API RouterInfo selectors are a separate namespace inventory. These
/// keys are adopted from the base I2PControl API and are not Proposal 170
/// additions; keeping them separate prevents either inventory's count from
/// silently absorbing the other.
pub const BASE_ROUTER_INFO_FIELDS: [ProposalRouterInfoField; 14] = [
    field(
        "i2p.router.version",
        ProposalValueType::String,
        Some(RouterInfoSelector::RouterVersion),
    ),
    field(
        "i2p.router.status",
        ProposalValueType::String,
        Some(RouterInfoSelector::RouterStatus),
    ),
    field("i2p.router.uptime", ProposalValueType::Integer, None),
    field("i2p.router.net.status", ProposalValueType::Integer, None),
    field(
        "i2p.router.net.bw.inbound.1s",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.bw.inbound.15s",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.bw.outbound.1s",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.bw.outbound.15s",
        ProposalValueType::Double,
        None,
    ),
    field(
        "i2p.router.net.tunnels.participating",
        ProposalValueType::Integer,
        Some(RouterInfoSelector::ParticipatingCount),
    ),
    field(
        "i2p.router.netdb.knownpeers",
        ProposalValueType::Integer,
        Some(RouterInfoSelector::NetDbKnownPeers),
    ),
    field(
        "i2p.router.netdb.activepeers",
        ProposalValueType::Integer,
        Some(RouterInfoSelector::NetDbActivePeers),
    ),
    field(
        "i2p.router.netdb.fastpeers",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.netdb.highcapacitypeers",
        ProposalValueType::Integer,
        None,
    ),
    field(
        "i2p.router.netdb.isreseeding",
        ProposalValueType::Boolean,
        None,
    ),
];

/// Canonical Proposal `SetConfig` keys, in specification order.
pub const PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS: [&str; 13] = [
    "subscriptions",
    "update_delay",
    "published_addressbook",
    "router_addressbook",
    "local_addressbook",
    "private_addressbook",
    "proxy_port",
    "proxy_host",
    "should_publish",
    "etags",
    "last_modified",
    "log",
    "theme",
];

/// Proposal 170 TunnelManager's exact top-level field vocabulary. `Type`
/// remains a protocol type discriminator; all other fields are siblings in
/// `params`, never children of a nonstandard `options` object.
pub const PROPOSAL_TUNNEL_MANAGER_FIELDS: &[&str] = &[
    "Name",
    "Action",
    "All",
    "Type",
    "NewName",
    "Port",
    "TargetHost",
    "Host",
    "TargetPort",
    "TargetDestination",
    "Destination",
    "StartOnLoad",
    "Description",
    "ReachableBy",
    "Shared",
    "UseSSL",
    "TunnelLength",
    "TunnelVariance",
    "TunnelQuantity",
    "TunnelBackupQuantity",
    "SigType",
    "EncType",
    "CustomOptions",
    "ProxyList",
    "UseOutproxyPlugin",
    "ProxyAuth",
    "ProxyUsername",
    "ProxyPassword",
    "OutproxyAuth",
    "OutproxyUsername",
    "OutproxyPassword",
    "OutproxyType",
    "SSLProxies",
    "JumpList",
    "ConnectDelay",
    "Profile",
    "DelayOpen",
    "Reduce",
    "ReduceCount",
    "ReduceTime",
    "Close",
    "CloseTime",
    "NewDest",
    "PersistentClientKey",
    "PrivKeyFile",
    "AllowUserAgent",
    "AllowReferer",
    "AllowAccept",
    "AllowInternalSSL",
    "WebsiteHostname",
    "SpoofedHost",
    "BlockAccessInProxies",
    "BlockUserAgents",
    "UserAgents",
    "UniqueLocalAddressPerClient",
    "BlockReferers",
    "MultiHoming",
    "AccessOption",
    "AccessList",
    "FilterFilePath",
    "MaxConcurrentConns",
    "ClientPerMinute",
    "ClientPerHour",
    "ClientPerDay",
    "TotalInPerMinute",
    "TotalInPerHour",
    "TotalInPerDay",
    "PostLimit",
    "PostLimitTime",
    "PerClientPeriod",
    "TotalPeriod",
    "TotalBanTime",
    "EncryptLeaseSet",
    "OptionalLookup",
    "LeaseSetClientAuths",
];

const fn field(
    key: &'static str,
    value_type: ProposalValueType,
    adapter: Option<RouterInfoSelector>,
) -> ProposalRouterInfoField {
    ProposalRouterInfoField {
        key,
        value_type,
        adapter,
    }
}

/// Exact, case-sensitive lookup of one canonical RouterInfo field.
pub fn router_info_field(key: &str) -> Option<&'static ProposalRouterInfoField> {
    PROPOSAL_ROUTER_INFO_FIELDS
        .iter()
        .chain(BASE_ROUTER_INFO_FIELDS.iter())
        .find(|field| field.key == key)
}
