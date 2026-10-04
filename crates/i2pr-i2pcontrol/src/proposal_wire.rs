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

/// Wire-level type for a Proposal 170 TunnelManager field. This inventory is
/// independent of the smaller set of fields with current runtime adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalTunnelValueType {
    /// JSON string.
    String,
    /// JSON integer.
    Integer,
    /// JSON boolean.
    Boolean,
    /// Array of client authorization objects.
    ClientAuthList,
}

/// Integer range declared by the pinned Java Proposal 170 parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposalTunnelIntegerRange {
    /// Exact case-sensitive field.
    pub key: &'static str,
    /// Inclusive lower bound.
    pub minimum: i64,
    /// Inclusive upper bound.
    pub maximum: i64,
}

/// Validation failure for a typed Proposal 170 TunnelManager value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalTunnelValueError {
    /// The JSON type, value bounds, or compound-field shape is invalid.
    Invalid,
}

/// Proposal integer constraints with explicit bounds in Java PR 6.
pub const PROPOSAL_TUNNEL_INTEGER_RANGES: [ProposalTunnelIntegerRange; 19] = [
    range("Port", 1, 65_535),
    range("TargetPort", 1, 65_535),
    range("TunnelLength", 0, 3),
    range("TunnelVariance", -2, 2),
    range("TunnelQuantity", 1, 6),
    range("TunnelBackupQuantity", 0, 3),
    range("ReduceCount", 0, 9),
    range("ReduceTime", 0, 9_999),
    range("CloseTime", 0, 9_999),
    range("NewDest", 0, 2),
    range("MaxConcurrentConns", 0, 100_000),
    range("ClientPerMinute", 0, 100_000),
    range("ClientPerHour", 0, 100_000),
    range("ClientPerDay", 0, 100_000),
    range("TotalInPerMinute", 0, 100_000),
    range("TotalInPerHour", 0, 100_000),
    range("TotalInPerDay", 0, 100_000),
    range("PostLimit", 0, 100_000),
    range("PostLimitTime", 0, 100_000),
];

/// Remaining bounded server-policy integer fields share Java's 0..100000
/// range. Split out to keep the primary table's cardinality auditable.
pub const PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES: [ProposalTunnelIntegerRange; 3] = [
    range("PerClientPeriod", 0, 100_000),
    range("TotalPeriod", 0, 100_000),
    range("TotalBanTime", 0, 100_000),
];

/// Ten exact Proposal `EncryptLeaseSet` values, in specification order.
pub const PROPOSAL_ENCRYPT_LEASE_SET_VALUES: [&str; 10] = [
    "disable",
    "encrypted (aes)",
    "blinded",
    "blinded with lookup password",
    "encrypted (psk)",
    "encrypted with lookup password (psk)",
    "encrypted with per-user key (psk)",
    "encrypted with lookup password and per-user key (psk)",
    "encrypted with per-user key (dh)",
    "encrypted with lookup password and per-user key (dh)",
];

/// Exact wire type lookup for a Proposal TunnelManager option.
pub fn proposal_tunnel_value_type(key: &str) -> Option<ProposalTunnelValueType> {
    use ProposalTunnelValueType::{Boolean, ClientAuthList, Integer, String};
    if !PROPOSAL_TUNNEL_MANAGER_FIELDS.contains(&key) {
        return None;
    }
    Some(match key {
        "All"
        | "StartOnLoad"
        | "Shared"
        | "UseSSL"
        | "UseOutproxyPlugin"
        | "ProxyAuth"
        | "OutproxyAuth"
        | "ConnectDelay"
        | "DelayOpen"
        | "Reduce"
        | "Close"
        | "PersistentClientKey"
        | "AllowUserAgent"
        | "AllowReferer"
        | "AllowAccept"
        | "AllowInternalSSL"
        | "BlockAccessInProxies"
        | "BlockUserAgents"
        | "UniqueLocalAddressPerClient"
        | "BlockReferers"
        | "MultiHoming" => Boolean,
        "Port"
        | "TargetPort"
        | "TunnelLength"
        | "TunnelVariance"
        | "TunnelQuantity"
        | "TunnelBackupQuantity"
        | "NewDest"
        | "ReduceCount"
        | "ReduceTime"
        | "CloseTime"
        | "MaxConcurrentConns"
        | "ClientPerMinute"
        | "ClientPerHour"
        | "ClientPerDay"
        | "TotalInPerMinute"
        | "TotalInPerHour"
        | "TotalInPerDay"
        | "PostLimit"
        | "PostLimitTime"
        | "PerClientPeriod"
        | "TotalPeriod"
        | "TotalBanTime" => Integer,
        "LeaseSetClientAuths" => ClientAuthList,
        _ => String,
    })
}

/// Validates the exact Proposal tunnel option JSON type and documented
/// integer/enumeration bounds without converting values to strings.
pub fn validate_proposal_tunnel_value(
    key: &str,
    value: &serde_json::Value,
) -> Result<(), ProposalTunnelValueError> {
    use ProposalTunnelValueType::{Boolean, ClientAuthList, Integer, String};
    let value_type = proposal_tunnel_value_type(key).ok_or(ProposalTunnelValueError::Invalid)?;
    let invalid = ProposalTunnelValueError::Invalid;
    match (value_type, value) {
        (String, serde_json::Value::String(text))
            if text.len() <= crate::limits::MAX_OPTION_VALUE_LEN =>
        {
            if key == "EncryptLeaseSet"
                && !PROPOSAL_ENCRYPT_LEASE_SET_VALUES.contains(&text.as_str())
            {
                return Err(invalid);
            }
        }
        (Integer, serde_json::Value::Number(number)) if number.is_i64() || number.is_u64() => {
            if let Some(bounds) = PROPOSAL_TUNNEL_INTEGER_RANGES
                .iter()
                .chain(PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES.iter())
                .find(|bounds| bounds.key == key)
            {
                let numeric = number.as_i64().ok_or(invalid)?;
                if !(bounds.minimum..=bounds.maximum).contains(&numeric) {
                    return Err(invalid);
                }
            }
        }
        (Boolean, serde_json::Value::Bool(_)) => {}
        (ClientAuthList, serde_json::Value::Array(items))
            if items.len() <= crate::limits::MAX_LIST_ITEMS =>
        {
            for item in items {
                let object = item.as_object().ok_or(invalid)?;
                if object.len() > 2 {
                    return Err(invalid);
                }
                for field in object.keys() {
                    if !matches!(field.as_str(), "Name" | "name" | "Key" | "key") {
                        return Err(invalid);
                    }
                }
                for name in ["Name", "name", "Key", "key"] {
                    if object.get(name).is_some_and(|value| {
                        value
                            .as_str()
                            .is_none_or(|text| text.len() > crate::limits::MAX_OPTION_VALUE_LEN)
                    }) {
                        return Err(invalid);
                    }
                }
                if object.contains_key("Name") && object.contains_key("name")
                    || object.contains_key("Key") && object.contains_key("key")
                {
                    return Err(invalid);
                }
            }
        }
        _ => return Err(invalid),
    }
    Ok(())
}

const fn range(key: &'static str, minimum: i64, maximum: i64) -> ProposalTunnelIntegerRange {
    ProposalTunnelIntegerRange {
        key,
        minimum,
        maximum,
    }
}

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
