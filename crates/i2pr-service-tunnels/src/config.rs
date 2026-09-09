//! Plan 174 §4/§6 runtime-neutral service-tunnel configuration model.
//!
//! Bounded typed identifiers, kinds, destination policy, listener and
//! target shapes, resource limits, timeouts, and the validated set.
//! All counts, lengths, and deadlines have hard typed ceilings.
//!
//! No sockets, no Tokio, no filesystem access. The daemon parses TOML
//! and projects raw values into these types; this crate only
//! validates structure and policy.

#![forbid(unsafe_code)]

use crate::destination::DestinationRef;
use crate::errors::ServiceTunnelError;

/// Maximum configured service tunnels in one set.
pub const MAX_SERVICE_TUNNELS: usize = 32;
/// Maximum static aliases in one set.
pub const MAX_STATIC_ALIASES: usize = 64;
/// Maximum service tunnel identifier length in bytes.
pub const MAX_SERVICE_ID_LEN: usize = 64;
/// Maximum shared client group identifier length in bytes.
pub const MAX_GROUP_ID_LEN: usize = 64;
/// Maximum active connections for one service.
pub const MAX_ACTIVE_CONNECTIONS_PER_SERVICE: usize = 128;
/// Maximum aggregate active connections across all services.
pub const MAX_ACTIVE_CONNECTIONS_AGGREGATE: usize = 1024;
/// Maximum buffered bytes per direction for one connection.
pub const MAX_BUFFERED_BYTES_PER_DIRECTION: usize = 1_048_576;
/// Minimum buffered bytes per direction for one connection.
pub const MIN_BUFFERED_BYTES_PER_DIRECTION: usize = 1024;
/// Maximum configured target list count for one service.
pub const MAX_CONFIGURED_TARGETS: usize = 8;
/// Maximum Unix socket path length in bytes.
pub const MAX_UNIX_PATH_LEN: usize = 255;
/// Minimum/maximum connect deadline in milliseconds.
pub const MIN_CONNECT_TIMEOUT_MS: u64 = 1_000;
/// Maximum connect deadline in milliseconds.
pub const MAX_CONNECT_TIMEOUT_MS: u64 = 120_000;
/// Minimum read deadline in milliseconds.
pub const MIN_READ_TIMEOUT_MS: u64 = 1_000;
/// Maximum read deadline in milliseconds.
pub const MAX_READ_TIMEOUT_MS: u64 = 600_000;
/// Minimum write deadline in milliseconds.
pub const MIN_WRITE_TIMEOUT_MS: u64 = 1_000;
/// Maximum write deadline in milliseconds.
pub const MAX_WRITE_TIMEOUT_MS: u64 = 600_000;
/// Minimum shutdown deadline in milliseconds.
pub const MIN_SHUTDOWN_TIMEOUT_MS: u64 = 1_000;
/// Maximum shutdown deadline in milliseconds.
pub const MAX_SHUTDOWN_TIMEOUT_MS: u64 = 30_000;

/// A validated service tunnel identifier.
///
/// Bounded ASCII `a-z0-9-_`, 1–64 bytes, starting with an
/// alphanumeric byte. No NUL, control, whitespace, path separator,
/// or `.i2p` suffix confusion.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ServiceTunnelId(String);

impl ServiceTunnelId {
    /// Parses and validates one identifier.
    pub fn parse(value: &str) -> Result<Self, ServiceTunnelError> {
        if value.is_empty() {
            return Err(ServiceTunnelError::InvalidId {
                value: String::new(),
                reason: "must not be empty",
            });
        }
        if value.len() > MAX_SERVICE_ID_LEN {
            return Err(ServiceTunnelError::InvalidId {
                value: truncated(value),
                reason: "exceeds the service id ceiling",
            });
        }
        if value.bytes().any(|b| b == 0 || b <= 0x20 || b == 0x7f) {
            return Err(ServiceTunnelError::InvalidId {
                value: truncated(value),
                reason: "must not contain NUL, control, or whitespace",
            });
        }
        if value.contains('/') || value.contains('\\') || value.contains('.') {
            return Err(ServiceTunnelError::InvalidId {
                value: truncated(value),
                reason: "must not contain path separators or dots",
            });
        }
        let first = value.as_bytes()[0];
        if !(first.is_ascii_alphanumeric()) {
            return Err(ServiceTunnelError::InvalidId {
                value: truncated(value),
                reason: "must start with an alphanumeric byte",
            });
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ServiceTunnelError::InvalidId {
                value: truncated(value),
                reason: "must use a-z0-9, hyphen, or underscore",
            });
        }
        // Identifiers are case-sensitive lower-case by convention;
        // upper-case is rejected to keep configuration canonical.
        if value.bytes().any(|b| b.is_ascii_uppercase()) {
            return Err(ServiceTunnelError::InvalidId {
                value: truncated(value),
                reason: "must be lower-case",
            });
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated shared client group identifier.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ServiceClientGroupId(String);

impl ServiceClientGroupId {
    /// Parses and validates one group identifier.
    pub fn parse(value: &str) -> Result<Self, ServiceTunnelError> {
        ServiceTunnelId::parse(value)
            .map(|id| Self(id.0))
            .map_err(|err| match err {
                ServiceTunnelError::InvalidId { value, reason } => {
                    ServiceTunnelError::InvalidGroup { value, reason }
                }
                other => other,
            })
    }

    /// Returns the group identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Typed service tunnel kinds for Milestone 10.
///
/// Every variant parses from its kebab-case configuration spelling.
/// No listener is active in Plan 174; the daemon rejects
/// `enabled = true` entries as not-yet-available until Plan 175.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ServiceTunnelKind {
    /// Generic TCP client tunnel.
    GenericClient,
    /// Generic TCP server tunnel.
    GenericServer,
    /// HTTP `.i2p` proxy client.
    HttpClient,
    /// SOCKS5 `.i2p` CONNECT client.
    Socks5Client,
    /// IRC client tunnel profile.
    IrcClient,
    /// IRC server tunnel profile.
    IrcServer,
}

impl ServiceTunnelKind {
    /// Parses one kebab-case kind spelling.
    pub fn parse(value: &str) -> Result<Self, ServiceTunnelError> {
        match value {
            "generic-client" => Ok(Self::GenericClient),
            "generic-server" => Ok(Self::GenericServer),
            "http-client" => Ok(Self::HttpClient),
            "socks5-client" => Ok(Self::Socks5Client),
            "irc-client" => Ok(Self::IrcClient),
            "irc-server" => Ok(Self::IrcServer),
            _ => Err(ServiceTunnelError::InvalidKind {
                value: truncated(value),
                reason: "must be generic-client, generic-server, http-client, socks5-client, irc-client, or irc-server",
            }),
        }
    }

    /// Returns the canonical kebab-case spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GenericClient => "generic-client",
            Self::GenericServer => "generic-server",
            Self::HttpClient => "http-client",
            Self::Socks5Client => "socks5-client",
            Self::IrcClient => "irc-client",
            Self::IrcServer => "irc-server",
        }
    }

    /// Returns `true` for server-side kinds that terminate at a
    /// loopback/Unix target.
    pub fn is_server(self) -> bool {
        matches!(self, Self::GenericServer | Self::IrcServer)
    }

    /// Returns `true` for client-side kinds that originate from a
    /// loopback listener.
    pub fn is_client(self) -> bool {
        !self.is_server()
    }
}

/// Destination ownership policy for one service.
///
/// `Dedicated` creates one destination/pool per service. A shared
/// client destination is only possible through an explicit bounded
/// `SharedClientGroup`; sharing is never implicit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestinationPolicy {
    /// One destination/pool per service.
    Dedicated,
    /// Shared named client destination group.
    SharedClientGroup(ServiceClientGroupId),
}

impl DestinationPolicy {
    /// Returns `true` for the dedicated policy.
    pub fn is_dedicated(&self) -> bool {
        matches!(self, Self::Dedicated)
    }
}

/// Runtime-neutral loopback listener specification.
///
/// M10 client-facing TCP listeners bind loopback only. The daemon
/// validates the loopback invariant; this type carries the validated
/// address so the invariant is explicit in the type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LocalListenerSpec {
    /// Loopback bind address (`127.0.0.1` or `::1`).
    pub address: std::net::IpAddr,
    /// Bind port. `0` selects an ephemeral port in tests.
    pub port: u16,
}

impl LocalListenerSpec {
    /// Validates one loopback listener address and port.
    pub fn parse(address: std::net::IpAddr, port: u16) -> Result<Self, ServiceTunnelError> {
        if !address.is_loopback() {
            return Err(ServiceTunnelError::InvalidListener {
                value: format!("{address}:{port}"),
                reason: "local listeners must bind loopback in M10",
            });
        }
        Ok(Self { address, port })
    }

    /// Parses one `ip:port` socket string and validates loopback.
    pub fn parse_socket(value: &str) -> Result<Self, ServiceTunnelError> {
        let socket: std::net::SocketAddr =
            value
                .parse()
                .map_err(|_| ServiceTunnelError::InvalidListener {
                    value: truncated(value),
                    reason: "must be a valid ip:port socket address",
                })?;
        Self::parse(socket.ip(), socket.port())
    }

    /// Returns the socket address.
    pub fn socket(self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(self.address, self.port)
    }
}

/// Server-side local target for generic/IRC server tunnels.
///
/// Generic server TCP targets must be loopback. Unix targets are
/// carried only as normalized path values here; platform and socket
/// validation happens in the daemon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerTarget {
    /// Loopback TCP target.
    LoopbackTcp(std::net::SocketAddr),
    /// Unix socket path (normalized value only).
    UnixPath(String),
}

impl ServerTarget {
    /// Parses one target string: `unix:/path` or `ip:port`.
    pub fn parse(value: &str) -> Result<Self, ServiceTunnelError> {
        if value.is_empty() {
            return Err(ServiceTunnelError::InvalidTarget {
                value: String::new(),
                reason: "must not be empty",
            });
        }
        if value.len() > MAX_UNIX_PATH_LEN + "unix:".len() + 64 {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(value),
                reason: "exceeds the server target ceiling",
            });
        }
        if value.bytes().any(|b| b == 0 || b < 0x20 || b == 0x7f) {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(value),
                reason: "must not contain NUL or control",
            });
        }
        if let Some(path) = value.strip_prefix("unix:") {
            return Self::parse_unix_path(path, value);
        }
        let socket: std::net::SocketAddr =
            value
                .parse()
                .map_err(|_| ServiceTunnelError::InvalidTarget {
                    value: truncated(value),
                    reason: "must be unix:/path or a loopback ip:port",
                })?;
        if !socket.ip().is_loopback() {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(value),
                reason: "server TCP targets must be loopback in M10",
            });
        }
        Ok(Self::LoopbackTcp(socket))
    }

    fn parse_unix_path(path: &str, full: &str) -> Result<Self, ServiceTunnelError> {
        if path.is_empty() {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(full),
                reason: "unix target path must not be empty",
            });
        }
        if path.len() > MAX_UNIX_PATH_LEN {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(full),
                reason: "unix target path exceeds the ceiling",
            });
        }
        if path.bytes().any(|b| b == 0) {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(full),
                reason: "unix target path must not contain NUL",
            });
        }
        if !path.starts_with('/') {
            return Err(ServiceTunnelError::InvalidTarget {
                value: truncated(full),
                reason: "unix target path must be absolute",
            });
        }
        Ok(Self::UnixPath(path.to_owned()))
    }
}

/// Central hard ceilings for service-tunnel resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceResourceLimits {
    /// Maximum active connections for one service.
    pub max_active_connections_per_service: usize,
    /// Maximum aggregate active connections.
    pub max_active_connections_aggregate: usize,
    /// Maximum buffered bytes per direction for one connection.
    pub max_buffered_bytes_per_direction: usize,
    /// Maximum configured target list count for one service.
    pub max_configured_targets: usize,
}

impl ServiceResourceLimits {
    /// Returns the Plan 174 default ceilings.
    pub fn defaults() -> Self {
        Self {
            max_active_connections_per_service: MAX_ACTIVE_CONNECTIONS_PER_SERVICE,
            max_active_connections_aggregate: MAX_ACTIVE_CONNECTIONS_AGGREGATE,
            max_buffered_bytes_per_direction: 65_536,
            max_configured_targets: MAX_CONFIGURED_TARGETS,
        }
    }

    /// Validates ceilings against the hard maxima.
    pub fn validate(self) -> Result<Self, ServiceTunnelError> {
        if self.max_active_connections_per_service == 0
            || self.max_active_connections_per_service > MAX_ACTIVE_CONNECTIONS_PER_SERVICE
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_active_connections_per_service",
                reason: "must be within 1..=128",
            });
        }
        if self.max_active_connections_aggregate == 0
            || self.max_active_connections_aggregate > MAX_ACTIVE_CONNECTIONS_AGGREGATE
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_active_connections_aggregate",
                reason: "must be within 1..=1024",
            });
        }
        if self.max_active_connections_per_service > self.max_active_connections_aggregate {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: "<limits>".to_owned(),
                reason: "per-service ceiling must not exceed the aggregate ceiling",
            });
        }
        if self.max_buffered_bytes_per_direction < MIN_BUFFERED_BYTES_PER_DIRECTION
            || self.max_buffered_bytes_per_direction > MAX_BUFFERED_BYTES_PER_DIRECTION
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_buffered_bytes_per_direction",
                reason: "must be within 1024..=1048576",
            });
        }
        if self.max_configured_targets == 0 || self.max_configured_targets > MAX_CONFIGURED_TARGETS
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_configured_targets",
                reason: "must be within 1..=8",
            });
        }
        Ok(self)
    }
}

/// Central hard ceilings for service-tunnel deadlines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceTimeouts {
    /// Connect deadline in milliseconds.
    pub connect_timeout_ms: u64,
    /// Read deadline in milliseconds.
    pub read_timeout_ms: u64,
    /// Write deadline in milliseconds.
    pub write_timeout_ms: u64,
    /// Shutdown/drain deadline in milliseconds.
    pub shutdown_timeout_ms: u64,
}

impl ServiceTimeouts {
    /// Returns the Plan 174 default deadlines.
    pub fn defaults() -> Self {
        Self {
            connect_timeout_ms: 10_000,
            read_timeout_ms: 60_000,
            write_timeout_ms: 60_000,
            shutdown_timeout_ms: 5_000,
        }
    }

    /// Validates deadline ranges.
    pub fn validate(self) -> Result<Self, ServiceTunnelError> {
        if !(MIN_CONNECT_TIMEOUT_MS..=MAX_CONNECT_TIMEOUT_MS).contains(&self.connect_timeout_ms) {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "connect_timeout_ms",
                reason: "must be within 1000..=120000",
            });
        }
        if !(MIN_READ_TIMEOUT_MS..=MAX_READ_TIMEOUT_MS).contains(&self.read_timeout_ms) {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "read_timeout_ms",
                reason: "must be within 1000..=600000",
            });
        }
        if !(MIN_WRITE_TIMEOUT_MS..=MAX_WRITE_TIMEOUT_MS).contains(&self.write_timeout_ms) {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "write_timeout_ms",
                reason: "must be within 1000..=600000",
            });
        }
        if !(MIN_SHUTDOWN_TIMEOUT_MS..=MAX_SHUTDOWN_TIMEOUT_MS).contains(&self.shutdown_timeout_ms)
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "shutdown_timeout_ms",
                reason: "must be within 1000..=30000",
            });
        }
        Ok(self)
    }
}

/// One validated service-tunnel specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceTunnelSpec {
    /// Service identifier.
    pub id: ServiceTunnelId,
    /// Service kind.
    pub kind: ServiceTunnelKind,
    /// Whether the service is enabled.
    pub enabled: bool,
    /// Local loopback listener (client kinds).
    pub listener: Option<LocalListenerSpec>,
    /// Local server target (server kinds).
    pub target: Option<ServerTarget>,
    /// Configured target list (explicit bounded selection).
    pub targets: Vec<ServerTarget>,
    /// Remote I2P destination reference (client kinds).
    pub destination: Option<DestinationRef>,
    /// Destination ownership policy.
    pub policy: DestinationPolicy,
    /// Per-service active-connection ceiling.
    pub max_connections: usize,
    /// Per-direction buffered-byte ceiling.
    pub max_buffered_bytes_per_direction: usize,
    /// Service deadlines.
    pub timeouts: ServiceTimeouts,
    /// HTTP-specific profile options. Mandatory for `HttpClient`
    /// kinds; ignored otherwise.
    pub http_options: Option<crate::http::HttpClientOptions>,
    /// SOCKS5-specific profile options. Mandatory for
    /// `Socks5Client` kinds; ignored otherwise.
    pub socks5_options: Option<crate::socks5::Socks5ClientOptions>,
    /// IRC-specific profile options. Mandatory for `IrcClient`
    /// kinds; ignored otherwise.
    pub irc_options: Option<crate::irc::IrcClientOptions>,
}

impl ServiceTunnelSpec {
    /// Validates internal consistency for one specification.
    pub fn validate(&self) -> Result<(), ServiceTunnelError> {
        if self.max_connections == 0 || self.max_connections > MAX_ACTIVE_CONNECTIONS_PER_SERVICE {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_connections",
                reason: "must be within 1..=128",
            });
        }
        if self.max_buffered_bytes_per_direction < MIN_BUFFERED_BYTES_PER_DIRECTION
            || self.max_buffered_bytes_per_direction > MAX_BUFFERED_BYTES_PER_DIRECTION
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_buffered_bytes_per_direction",
                reason: "must be within 1024..=1048576",
            });
        }
        self.timeouts.validate()?;
        if self.targets.len() > MAX_CONFIGURED_TARGETS {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "targets",
                reason: "exceeds the configured target list ceiling",
            });
        }
        let id = self.id.as_str().to_owned();
        match self.kind {
            ServiceTunnelKind::GenericClient
            | ServiceTunnelKind::HttpClient
            | ServiceTunnelKind::Socks5Client
            | ServiceTunnelKind::IrcClient => {
                if self.listener.is_none() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "client service kinds require a loopback listener",
                    });
                }
                if self.target.is_some() || !self.targets.is_empty() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "client service kinds must not carry a server target",
                    });
                }
                if self.destination.is_none() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "client service kinds require a destination reference",
                    });
                }
            }
            ServiceTunnelKind::GenericServer | ServiceTunnelKind::IrcServer => {
                if self.listener.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "server service kinds must not carry a local listener",
                    });
                }
                if self.target.is_none() && self.targets.is_empty() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "server service kinds require a loopback or unix target",
                    });
                }
                if self.destination.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "server service kinds must not carry a remote destination reference",
                    });
                }
                if matches!(self.policy, DestinationPolicy::SharedClientGroup(_)) {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "server service kinds require a dedicated destination",
                    });
                }
            }
        }
        // Plan 176: http-client must carry HTTP profile options;
        // non-HTTP kinds must not.
        match self.kind {
            ServiceTunnelKind::HttpClient => {
                let options = self.http_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "http-client requires http_options",
                    }
                })?;
                options.validate()?;
            }
            _ => {
                if self.http_options.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "http_options must not be set for non-HTTP kinds",
                    });
                }
            }
        }
        // Plan 177: socks5-client must carry SOCKS5 profile options;
        // non-SOCKS5 kinds must not.
        match self.kind {
            ServiceTunnelKind::Socks5Client => {
                let options = self.socks5_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "socks5-client requires socks5_options",
                    }
                })?;
                options.validate()?;
            }
            _ => {
                if self.socks5_options.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "socks5_options must not be set for non-SOCKS5 kinds",
                    });
                }
            }
        }
        // Plan 178: irc-client must carry IRC profile options;
        // non-IRC kinds must not.
        match self.kind {
            ServiceTunnelKind::IrcClient => {
                let options = self.irc_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "irc-client requires irc_options",
                    }
                })?;
                options.validate()?;
            }
            _ => {
                if self.irc_options.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "irc_options must not be set for non-IRC kinds",
                    });
                }
            }
        }
        Ok(())
    }
}

/// A validated set of service-tunnel specifications.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServiceTunnelSet {
    /// Service specifications in configuration order.
    pub tunnels: Vec<ServiceTunnelSpec>,
}

impl ServiceTunnelSet {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates duplicate identifiers, duplicate listeners, and
    /// per-service contradictions before daemon state changes.
    pub fn validate(&self) -> Result<(), ServiceTunnelError> {
        if self.tunnels.len() > MAX_SERVICE_TUNNELS {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "service_count",
                reason: "exceeds the service count ceiling",
            });
        }
        let mut ids = std::collections::HashSet::new();
        let mut listeners = std::collections::HashSet::new();
        for spec in &self.tunnels {
            if !ids.insert(spec.id.as_str().to_owned()) {
                return Err(ServiceTunnelError::DuplicateId {
                    value: spec.id.as_str().to_owned(),
                });
            }
            spec.validate()?;
            if let Some(listener) = spec.listener
                && !listeners.insert(listener.socket())
            {
                return Err(ServiceTunnelError::DuplicateListener {
                    value: listener.socket().to_string(),
                });
            }
        }
        // Aggregate connection ceilings must fit the global budget.
        let aggregate: usize = self.tunnels.iter().map(|spec| spec.max_connections).sum();
        if aggregate > MAX_ACTIVE_CONNECTIONS_AGGREGATE {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "aggregate_connections",
                reason: "per-service ceilings exceed the aggregate ceiling",
            });
        }
        Ok(())
    }

    /// Returns the number of configured services.
    pub fn len(&self) -> usize {
        self.tunnels.len()
    }

    /// Returns `true` when no services are configured.
    pub fn is_empty(&self) -> bool {
        self.tunnels.is_empty()
    }
}

fn truncated(value: &str) -> String {
    const MAX: usize = 128;
    if value.len() <= MAX {
        value.to_owned()
    } else {
        format!("{}…", &value[..MAX])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_spec(id: &str, listener: &str, destination: &str) -> ServiceTunnelSpec {
        ServiceTunnelSpec {
            id: ServiceTunnelId::parse(id).expect("id"),
            kind: ServiceTunnelKind::GenericClient,
            enabled: false,
            listener: Some(LocalListenerSpec::parse_socket(listener).expect("listener")),
            target: None,
            targets: Vec::new(),
            destination: Some(DestinationRef::parse(destination).expect("destination")),
            policy: DestinationPolicy::Dedicated,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
        }
    }

    fn canonical_b32() -> String {
        format!("{}.b32.i2p", "a".repeat(52))
    }

    #[test]
    fn all_service_kinds_parse_as_typed_values() {
        let cases = [
            ("generic-client", ServiceTunnelKind::GenericClient),
            ("generic-server", ServiceTunnelKind::GenericServer),
            ("http-client", ServiceTunnelKind::HttpClient),
            ("socks5-client", ServiceTunnelKind::Socks5Client),
            ("irc-client", ServiceTunnelKind::IrcClient),
            ("irc-server", ServiceTunnelKind::IrcServer),
        ];
        for (text, expected) in cases {
            assert_eq!(ServiceTunnelKind::parse(text).expect("kind"), expected);
            assert_eq!(expected.as_str(), text);
        }
        assert!(ServiceTunnelKind::parse("http-server").is_err());
        assert!(ServiceTunnelKind::parse("GENERIC-CLIENT").is_err());
        assert!(ServiceTunnelKind::parse("").is_err());
    }

    #[test]
    fn duplicate_ids_rejected() {
        let first = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        let second = client_spec("alpha", "127.0.0.1:8081", &canonical_b32());
        let set = ServiceTunnelSet {
            tunnels: vec![first, second],
        };
        assert!(matches!(
            set.validate(),
            Err(ServiceTunnelError::DuplicateId { .. })
        ));
    }

    #[test]
    fn duplicate_listeners_rejected() {
        let first = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        let second = client_spec("beta", "127.0.0.1:8080", &canonical_b32());
        let set = ServiceTunnelSet {
            tunnels: vec![first, second],
        };
        assert!(matches!(
            set.validate(),
            Err(ServiceTunnelError::DuplicateListener { .. })
        ));
    }

    #[test]
    fn non_loopback_listener_rejected() {
        assert!(LocalListenerSpec::parse_socket("0.0.0.0:8080").is_err());
        assert!(LocalListenerSpec::parse_socket("192.168.1.10:8080").is_err());
        assert!(LocalListenerSpec::parse_socket("8.8.8.8:53").is_err());
        assert!(LocalListenerSpec::parse_socket("127.0.0.1:8080").is_ok());
        assert!(LocalListenerSpec::parse_socket("[::1]:8080").is_ok());
    }

    #[test]
    fn non_loopback_server_target_rejected() {
        assert!(ServerTarget::parse("192.168.1.10:8080").is_err());
        assert!(ServerTarget::parse("0.0.0.0:8080").is_err());
        assert!(ServerTarget::parse("127.0.0.1:8080").is_ok());
        assert!(ServerTarget::parse("[::1]:8080").is_ok());
        assert!(ServerTarget::parse("unix:/run/i2pr/service.sock").is_ok());
        assert!(ServerTarget::parse("unix:relative/path.sock").is_err());
        assert!(ServerTarget::parse("unix:").is_err());
    }

    #[test]
    fn contradictory_options_rejected() {
        // Client without destination.
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.destination = None;
        assert!(spec.validate().is_err());
        // Client with server target.
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.target = Some(ServerTarget::parse("127.0.0.1:9090").expect("target"));
        assert!(spec.validate().is_err());
        // Server with shared policy.
        let server = ServiceTunnelSpec {
            id: ServiceTunnelId::parse("srv").expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: false,
            listener: None,
            target: Some(ServerTarget::parse("127.0.0.1:9090").expect("target")),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::SharedClientGroup(
                ServiceClientGroupId::parse("group").expect("group"),
            ),
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
        };
        assert!(server.validate().is_err());
    }

    #[test]
    fn bounds_enforced() {
        assert!(ServiceTunnelId::parse("").is_err());
        assert!(ServiceTunnelId::parse(&"a".repeat(MAX_SERVICE_ID_LEN + 1)).is_err());
        assert!(ServiceTunnelId::parse("UPPER").is_err());
        assert!(ServiceTunnelId::parse("has space").is_err());
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.max_connections = MAX_ACTIVE_CONNECTIONS_PER_SERVICE + 1;
        assert!(spec.validate().is_err());
        spec.max_connections = 16;
        spec.max_buffered_bytes_per_direction = MAX_BUFFERED_BYTES_PER_DIRECTION + 1;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn http_client_requires_options() {
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.kind = ServiceTunnelKind::HttpClient;
        assert!(spec.validate().is_err());
        spec.http_options = Some(crate::http::HttpClientOptions::default());
        spec.validate().expect("http options validate");
        // Non-HTTP kinds must not carry http_options.
        spec.kind = ServiceTunnelKind::GenericClient;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn socks5_client_requires_options() {
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.kind = ServiceTunnelKind::Socks5Client;
        assert!(spec.validate().is_err());
        spec.socks5_options = Some(crate::socks5::Socks5ClientOptions::default());
        spec.validate().expect("socks5 options validate");
        // Non-SOCKS5 kinds must not carry socks5_options.
        spec.kind = ServiceTunnelKind::GenericClient;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn irc_client_requires_options() {
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.kind = ServiceTunnelKind::IrcClient;
        assert!(spec.validate().is_err());
        spec.irc_options = Some(crate::irc::IrcClientOptions::default());
        spec.validate().expect("irc options validate");
        // Non-IRC kinds must not carry irc_options.
        spec.kind = ServiceTunnelKind::GenericClient;
        assert!(spec.validate().is_err());
        // IRC options with bad allowed host must reject.
        spec.kind = ServiceTunnelKind::IrcClient;
        let mut bad_options = crate::irc::IrcClientOptions::default();
        bad_options.allowed_hosts.insert("example.com".to_owned());
        spec.irc_options = Some(bad_options);
        assert!(spec.validate().is_err());
    }
}
