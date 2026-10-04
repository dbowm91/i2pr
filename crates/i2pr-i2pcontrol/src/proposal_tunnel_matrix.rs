//! Canonical Proposal 170 TunnelManager field-by-type ownership census.
//!
//! Unlike the frozen Plan 292 option inventory, this matrix is derived from
//! the complete canonical wire inventory. `OwnerGap` rows are explicit work
//! remaining; they must be zero before Plan 323 can close.

use crate::{PROPOSAL_TUNNEL_MANAGER_FIELDS, TUNNEL_TYPES};

/// Classification of one canonical field/type cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalTunnelCellDisposition {
    /// A runtime, persistence, or control-plane owner consumes the field.
    Apply { owner: &'static str },
    /// Proposal semantics do not apply to this tunnel type.
    NotApplicable { reason: &'static str },
    /// A deep capability plan owns the field.
    DeepPrerequisite { plan: u16, reason: &'static str },
    /// A non-deep field has no operational owner yet.
    OwnerGap { owner_needed: &'static str },
}

/// One generated cell in the canonical field-by-tunnel-type matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposalTunnelMatrixCell {
    /// Canonical Proposal field spelling.
    pub field: &'static str,
    /// Canonical Proposal tunnel type spelling.
    pub tunnel_type: &'static str,
    /// Current disposition.
    pub disposition: ProposalTunnelCellDisposition,
}

const ENVELOPE_FIELDS: &[&str] = &["Name", "Action", "All", "Type", "NewName"];
const CLIENTS: &[&str] = &[
    "client",
    "httpclient",
    "socks",
    "ircclient",
    "connectclient",
    "socksirc",
];
const TCP_SERVERS: &[&str] = &["server", "ircserver", "httpserver", "httpbidirserver"];
const HTTP_CLIENTS: &[&str] = &["httpclient", "httpbidirserver"];
const HTTP_SERVERS: &[&str] = &["httpserver", "httpbidirserver"];
const PROXY_CLIENTS: &[&str] = &["httpclient", "socks", "socksirc"];
const LEASESET_SERVERS: &[&str] = &[
    "server",
    "ircserver",
    "httpserver",
    "httpbidirserver",
    "streamrserver",
];

/// Generates every option field/type cell from the frozen canonical
/// inventories. Top-level operation selectors are excluded because they are
/// request-envelope fields, not tunnel configuration.
pub fn proposal_tunnel_manager_matrix() -> impl Iterator<Item = ProposalTunnelMatrixCell> + Clone {
    PROPOSAL_TUNNEL_MANAGER_FIELDS
        .iter()
        .copied()
        .filter(|field| !ENVELOPE_FIELDS.contains(field))
        .flat_map(|field| {
            TUNNEL_TYPES
                .iter()
                .copied()
                .map(move |tunnel_type| ProposalTunnelMatrixCell {
                    field,
                    tunnel_type,
                    disposition: disposition(field, tunnel_type),
                })
        })
}

fn disposition(field: &str, tunnel_type: &str) -> ProposalTunnelCellDisposition {
    use ProposalTunnelCellDisposition::{Apply, DeepPrerequisite, NotApplicable, OwnerGap};

    let applies = |types: &[&str]| types.contains(&tunnel_type);
    let owner = match field {
        "Description" => Some("control definition metadata and canonical Get projection"),
        "StartOnLoad" => Some("persisted control-definition start intent"),
        "TargetHost" | "Host" | "TargetPort" => {
            return if applies(TCP_SERVERS) {
                Apply {
                    owner: "ServiceTunnelSpec server target and dial policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no loopback TCP server target",
                }
            };
        }
        "Port" | "ReachableBy" => {
            return if applies(CLIENTS) || tunnel_type == "httpbidirserver" {
                Apply {
                    owner: "ServiceTunnelSpec loopback client listener",
                }
            } else {
                NotApplicable {
                    reason: "type has no local TCP client listener",
                }
            };
        }
        "TargetDestination" | "Destination" => {
            return if applies(CLIENTS) || tunnel_type == "streamrclient" {
                Apply {
                    owner: "ServiceTunnelSpec remote I2P destination",
                }
            } else {
                NotApplicable {
                    reason: "type does not dial a remote I2P destination",
                }
            };
        }
        "Shared" => {
            return if applies(CLIENTS) {
                Apply {
                    owner: "explicit shared client Destination group",
                }
            } else {
                NotApplicable {
                    reason: "type does not own a shareable client Destination",
                }
            };
        }
        "PersistentClientKey" | "NewDest" => {
            return if applies(CLIENTS) {
                Apply {
                    owner: "service Destination identity lifecycle",
                }
            } else {
                NotApplicable {
                    reason: "type does not own a client Destination identity",
                }
            };
        }
        "PrivKeyFile" => {
            Some("per-service confined logical key reference and persistent identity store")
        }
        "UseSSL" => {
            return if matches!(tunnel_type, "server" | "httpserver" | "httpbidirserver") {
                Apply {
                    owner: "ServiceTunnelSpec TLS target dial with daemon trust policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no TCP server target dial",
                }
            };
        }
        "TunnelLength" | "TunnelVariance" | "TunnelQuantity" | "TunnelBackupQuantity" => {
            Some("DestinationConfig tunnel shaping and Plan 296 pool policy")
        }
        "SigType" => {
            return DeepPrerequisite {
                plan: 324,
                reason: "destination signing algorithm policy",
            };
        }
        "EncType" => {
            return DeepPrerequisite {
                plan: 324,
                reason: "destination encryption algorithm policy",
            };
        }
        "EncryptLeaseSet"
        | "OptionalLookup"
        | "LeaseSetPassword"
        | "LeaseSetBlindingSecret"
        | "LeaseSetClientAuths" => {
            return if applies(LEASESET_SERVERS) {
                DeepPrerequisite {
                    plan: 326,
                    reason: "encrypted/blinded LeaseSet and client authorization",
                }
            } else {
                NotApplicable {
                    reason: "type does not publish a service LeaseSet",
                }
            };
        }
        "ProxyList" | "UseOutproxyPlugin" | "OutproxyAuth" | "OutproxyUsername"
        | "OutproxyPassword" | "OutproxyType" => {
            return if applies(CLIENTS) {
                DeepPrerequisite {
                    plan: 327,
                    reason: "I2P-routed outproxy provider and routing policy",
                }
            } else {
                NotApplicable {
                    reason: "type does not route client traffic through an outproxy",
                }
            };
        }
        "SSLProxies" => {
            return if applies(PROXY_CLIENTS) {
                DeepPrerequisite {
                    plan: 327,
                    reason: "safe I2P-routed SSL proxy provider semantics",
                }
            } else {
                NotApplicable {
                    reason: "type does not expose a local HTTP/SOCKS proxy",
                }
            };
        }
        "CustomOptions" => {
            return Apply {
                owner: "fail-closed rejection of untyped custom values",
            };
        }
        "ProxyAuth" | "ProxyUsername" | "ProxyPassword" => {
            return if applies(PROXY_CLIENTS) {
                Apply {
                    owner: "bounded local proxy credential verifier",
                }
            } else {
                NotApplicable {
                    reason: "type does not expose an authenticated HTTP/SOCKS proxy",
                }
            };
        }
        "ConnectDelay" => {
            return if tunnel_type == "client" {
                Apply {
                    owner: "bounded first-read/timeout Streaming connect policy",
                }
            } else {
                NotApplicable {
                    reason: "only generic client has this initial-SYN behavior",
                }
            };
        }
        "DelayOpen" => {
            return if applies(CLIENTS) {
                OwnerGap {
                    owner_needed: "deferred service Destination/tunnel activation lifecycle",
                }
            } else {
                NotApplicable {
                    reason: "type does not create a client tunnel on demand",
                }
            };
        }
        "Profile" | "Reduce" | "ReduceCount" | "ReduceTime" | "Close" | "CloseTime" => {
            Some("bounded Streaming profile and service idle lifecycle")
        }
        "AllowUserAgent" | "AllowReferer" | "AllowAccept" | "AllowInternalSSL" => {
            return if applies(HTTP_CLIENTS) {
                Apply {
                    owner: "typed HTTP client request policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no HTTP client request layer",
                }
            };
        }
        "WebsiteHostname"
        | "SpoofedHost"
        | "BlockAccessInProxies"
        | "BlockUserAgents"
        | "UserAgents"
        | "BlockReferers"
        | "JumpList" => {
            return if applies(HTTP_SERVERS) {
                Apply {
                    owner: "bounded HTTP presentation policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no HTTP server presentation layer",
                }
            };
        }
        "AccessOption" | "AccessList" => {
            return if matches!(tunnel_type, "server" | "httpserver" | "httpbidirserver") {
                Apply {
                    owner: "authenticated server peer allow/deny policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no access-list-controlled server acceptor",
                }
            };
        }
        "FilterFilePath" => {
            return if matches!(
                tunnel_type,
                "server" | "ircserver" | "httpserver" | "httpbidirserver"
            ) {
                Apply {
                    owner: "confined filter file and server peer policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no server peer filter",
                }
            };
        }
        "MultiHoming" => {
            return if matches!(tunnel_type, "server" | "httpserver" | "httpbidirserver") {
                OwnerGap {
                    owner_needed: "bounded canonical server target-list owner for failover",
                }
            } else {
                NotApplicable {
                    reason: "type does not dial a multi-target server endpoint",
                }
            };
        }
        "UniqueLocalAddressPerClient" => {
            return if matches!(tunnel_type, "server" | "httpserver" | "httpbidirserver") {
                Apply {
                    owner: "server target dial source-address policy",
                }
            } else {
                NotApplicable {
                    reason: "type has no inbound server target dial",
                }
            };
        }
        "MaxConcurrentConns" => Some("bounded per-service connection admission ceiling"),
        "ClientPerMinute" | "ClientPerHour" | "ClientPerDay" | "TotalInPerMinute"
        | "TotalInPerHour" | "TotalInPerDay" => {
            return if matches!(
                tunnel_type,
                "server" | "ircserver" | "httpserver" | "httpbidirserver" | "streamrserver"
            ) {
                Apply {
                    owner: "authenticated peer/aggregate admission limiter",
                }
            } else {
                NotApplicable {
                    reason: "type does not accept inbound server connections or subscriptions",
                }
            };
        }
        "PostLimit" | "PostLimitTime" | "PerClientPeriod" | "TotalPeriod" | "TotalBanTime" => {
            return if applies(HTTP_SERVERS) {
                Apply {
                    owner: "bounded HTTP server POST limiter",
                }
            } else {
                NotApplicable {
                    reason: "type has no HTTP server request body",
                }
            };
        }
        _ => None,
    };
    match owner {
        Some(owner) => Apply { owner },
        None => OwnerGap {
            owner_needed: "field-specific runtime or persistence owner",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_matrix_covers_every_canonical_option_for_all_twelve_types() {
        let options = PROPOSAL_TUNNEL_MANAGER_FIELDS
            .iter()
            .filter(|field| !ENVELOPE_FIELDS.contains(field))
            .count();
        let cells = proposal_tunnel_manager_matrix().collect::<Vec<_>>();
        assert_eq!(cells.len(), options * TUNNEL_TYPES.len());
        assert!(
            cells
                .iter()
                .all(|cell| TUNNEL_TYPES.contains(&cell.tunnel_type))
        );
        for field in PROPOSAL_TUNNEL_MANAGER_FIELDS
            .iter()
            .filter(|field| !ENVELOPE_FIELDS.contains(field))
        {
            assert_eq!(
                cells.iter().filter(|cell| cell.field == *field).count(),
                TUNNEL_TYPES.len()
            );
        }
        let gaps = cells
            .iter()
            .filter_map(|cell| match cell.disposition {
                ProposalTunnelCellDisposition::OwnerGap { .. } => Some(cell.field),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(gaps, ["DelayOpen", "MultiHoming"].into_iter().collect());
    }
}
