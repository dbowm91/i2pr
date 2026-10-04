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
/// Proposal 170's active `ConnectDelay` default, selected by its boolean.
pub const DEFAULT_STREAMING_CONNECT_DELAY_MS: u64 = 500;
/// Hard bound for a deferred Streaming connect before an empty SYN is sent.
pub const MAX_STREAMING_CONNECT_DELAY_MS: u64 = 5_000;

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

/// A validated Destination linkability-domain identifier.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DestinationGroupId(String);

impl DestinationGroupId {
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

/// Compatibility name for configurations and callers that used
/// the original client-only group concept.
pub type ServiceClientGroupId = DestinationGroupId;

/// Typed service tunnel kinds for Milestone 10.
///
/// Every variant parses from its kebab-case configuration spelling.
/// No listener is active in Plan 174; the daemon rejects
/// `enabled = true` entries as not-yet-available until Plan 175.
/// Plan 290 adds the four composed Proposal 170 families
/// (`connect-client`, `socks-irc`, `http-server`,
/// `http-bidir-server`) over the existing streaming/HTTP/SOCKS/IRC
/// primitives.
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
    /// Strict HTTP CONNECT-only client (Plan 290).
    ConnectClient,
    /// SOCKS negotiation with the IRC client privacy filter on all
    /// post-CONNECT traffic (Plan 290).
    SocksIrc,
    /// Filtered HTTP server (Plan 290).
    HttpServer,
    /// Deprecated bidirectional HTTP server: filtered HTTP server
    /// half plus a no-outproxy HTTP client half under one lifecycle
    /// generation and one persistent server identity (Plan 290).
    HttpBidirServer,
    /// Streamr media subscriber: repliable-datagram subscribes to
    /// a configured producer, raw-datagram media to a loopback UDP
    /// target (Plan 291).
    StreamrClient,
    /// Streamr media publisher: persistent destination, loopback
    /// UDP media source, bounded authenticated subscriber table,
    /// raw-datagram fanout (Plan 291).
    StreamrServer,
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
            "connect-client" => Ok(Self::ConnectClient),
            "socks-irc" => Ok(Self::SocksIrc),
            "http-server" => Ok(Self::HttpServer),
            "http-bidir-server" => Ok(Self::HttpBidirServer),
            "streamr-client" => Ok(Self::StreamrClient),
            "streamr-server" => Ok(Self::StreamrServer),
            _ => Err(ServiceTunnelError::InvalidKind {
                value: truncated(value),
                reason: "must be generic-client, generic-server, http-client, socks5-client, irc-client, irc-server, connect-client, socks-irc, http-server, http-bidir-server, streamr-client, or streamr-server",
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
            Self::ConnectClient => "connect-client",
            Self::SocksIrc => "socks-irc",
            Self::HttpServer => "http-server",
            Self::HttpBidirServer => "http-bidir-server",
            Self::StreamrClient => "streamr-client",
            Self::StreamrServer => "streamr-server",
        }
    }

    /// Returns `true` for server-side kinds that terminate at a
    /// loopback/Unix target.
    ///
    /// Plan 290: `HttpServer` is server-side. `HttpBidirServer`
    /// reports client-side here because it always carries a
    /// loopback listener (its server half is visible through the
    /// dedicated destination identity, not through this bit); the
    /// daemon builds both a listener and a server target for it.
    ///
    /// Plan 291: `StreamrServer` is server-side (it publishes one
    /// persistent destination and terminates inbound subscribes at
    /// a loopback UDP source); the daemon carves it out of the TCP
    /// target path because it carries UDP endpoints, not a TCP
    /// target.
    pub fn is_server(self) -> bool {
        matches!(
            self,
            Self::GenericServer | Self::IrcServer | Self::HttpServer | Self::StreamrServer
        )
    }

    /// Returns `true` for client-side kinds that originate from a
    /// loopback listener.
    pub fn is_client(self) -> bool {
        !self.is_server()
    }
}

/// Destination ownership policy for one service.
///
/// `Dedicated` creates an implicit unique group. `SharedGroup` names
/// an explicit linkability domain that may include client and server
/// services. Sharing is never inferred from service kind or capacity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestinationPolicy {
    /// One destination/pool per service.
    Dedicated,
    /// One persistent destination/pool per client service.
    PersistentClient,
    /// Legacy client-only group spelling, retained for source compatibility.
    SharedClientGroup(ServiceClientGroupId),
    /// Persistent named group of explicitly shared client services.
    PersistentSharedClientGroup(DestinationGroupId),
    /// Shared named Destination linkability domain.
    SharedGroup(DestinationGroupId),
}

/// Collision-free key for an implicit dedicated group or an
/// explicitly configured linkability domain.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum DestinationGroupKey {
    /// Backwards-compatible one-service group created by `Dedicated`.
    Dedicated(ServiceTunnelId),
    /// Explicitly shared linkability domain.
    Explicit(DestinationGroupId),
}

/// Resolved, runtime-neutral Destination-group composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationGroupSpec {
    /// Collision-free group owner key.
    pub key: DestinationGroupKey,
    /// Services intentionally attached to this Destination.
    pub members: Vec<ServiceTunnelId>,
    /// Whether Destination identity must persist across restart.
    pub persistent: bool,
}

impl DestinationPolicy {
    /// Returns `true` for the dedicated policy.
    pub fn is_dedicated(&self) -> bool {
        matches!(self, Self::Dedicated | Self::PersistentClient)
    }

    /// Whether this policy requires the named client identity to
    /// survive router restart.
    pub fn persists_client_identity(&self) -> bool {
        matches!(
            self,
            Self::PersistentClient | Self::PersistentSharedClientGroup(_)
        )
    }

    /// Returns the explicit Destination-group identity when present.
    pub fn group_id(&self, service_id: &ServiceTunnelId) -> DestinationGroupId {
        match self {
            Self::Dedicated | Self::PersistentClient => {
                DestinationGroupId(service_id.as_str().to_owned())
            }
            Self::SharedClientGroup(id)
            | Self::PersistentSharedClientGroup(id)
            | Self::SharedGroup(id) => id.clone(),
        }
    }

    /// Returns the collision-free group owner key for a service.
    pub fn group_key(&self, service_id: &ServiceTunnelId) -> DestinationGroupKey {
        match self {
            Self::Dedicated | Self::PersistentClient => {
                DestinationGroupKey::Dedicated(service_id.clone())
            }
            Self::SharedClientGroup(id)
            | Self::PersistentSharedClientGroup(id)
            | Self::SharedGroup(id) => DestinationGroupKey::Explicit(id.clone()),
        }
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
    /// Optional deferred-connect delay for generic client tunnels.
    /// `None` means initiate the SYN immediately.
    pub streaming_connect_delay_ms: Option<u64>,
}

impl ServiceTimeouts {
    /// Returns the Plan 174 default deadlines.
    pub fn defaults() -> Self {
        Self {
            connect_timeout_ms: 10_000,
            read_timeout_ms: 60_000,
            write_timeout_ms: 60_000,
            shutdown_timeout_ms: 5_000,
            streaming_connect_delay_ms: None,
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
        if self
            .streaming_connect_delay_ms
            .is_some_and(|delay| delay == 0 || delay > MAX_STREAMING_CONNECT_DELAY_MS)
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "streaming_connect_delay_ms",
                reason: "must be within 1..=5000 or absent",
            });
        }
        Ok(self)
    }
}

/// Proposal 170 tunnel quantity ceiling (the pool allows more; the
/// Proposal binds control-plane values to 1..=6).
pub const MAX_TUNNEL_QUANTITY: u8 = 6;
/// Proposal 170 tunnel length ceiling (0..=3 on the wire; 0 is
/// rejected by service-destination policy, see [`TunnelShaping`]).
pub const MAX_TUNNEL_LENGTH_HOPS: u8 = 3;
/// Proposal 170 backup-quantity ceiling (Plan 296): 0..=3 standby
/// tunnels held ready beyond the per-direction quantity target.
pub const MAX_TUNNEL_BACKUP_QUANTITY: u8 = 3;
/// Proposal 170 length-variance bound (Plan 296): per-build hop
/// adjustment sampled uniformly from `-variance..=+variance`.
pub const MAX_TUNNEL_LENGTH_VARIANCE: i8 = 2;
/// Directional pool maximum mirrored from the destination tunnel
/// pool ceiling (Plan 296): a per-direction quantity plus backup
/// may not exceed the tunnels the pool can hold. The destination
/// crate owns the authoritative ceiling; the daemon cross-checks
/// equality in its shaping tests so the mirror cannot drift.
pub const MAX_EFFECTIVE_DIRECTION_TUNNELS: u8 = 8;

/// Validated per-tunnel pool shaping (Plan 292, extended by Plan 296).
///
/// `tunnel_quantity` is the symmetric default; `inbound_quantity` and
/// `outbound_quantity` override per direction. `tunnel_length` is the
/// symmetric default; per-direction lengths must agree because the
/// pool uses a single hop length (the control boundary rejects
/// differing per-direction lengths instead of silently dropping one).
/// Length 0 (zero-hop) is rejected: service destinations run in
/// `Remote` tunnel mode and the destination policy does not permit
/// zero-hop service pools.
///
/// Plan 296: `backup_quantity` (0..=3) holds that many extra tunnels
/// ready beyond each per-direction quantity target; established
/// tunnels past the base target count as standby and promote
/// automatically when a base tunnel fails. The per-direction
/// effective target (`quantity + backup`) may not exceed
/// [`MAX_EFFECTIVE_DIRECTION_TUNNELS`]. `length_variance` (-2..=+2)
/// randomizes each build's hop length around the configured length
/// within the pool hop policy (see the destination crate's build
/// sampler); 0 disables variance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TunnelShaping {
    /// Inbound pool target (1..=6).
    pub inbound_quantity: u8,
    /// Outbound pool target (1..=6).
    pub outbound_quantity: u8,
    /// Shared pool hop length (1..=3).
    pub length_hops: u8,
    /// Standby tunnels held ready per direction (0..=3).
    pub backup_quantity: u8,
    /// Per-build hop-length variance (-2..=+2).
    pub length_variance: i8,
}

impl TunnelShaping {
    /// Shaping that reproduces `DestinationConfig::balanced` exactly.
    pub fn balanced() -> Self {
        Self {
            inbound_quantity: 2,
            outbound_quantity: 2,
            length_hops: 2,
            backup_quantity: 0,
            length_variance: 0,
        }
    }

    /// Validates explicit shaping values.
    pub fn try_new(
        inbound_quantity: u8,
        outbound_quantity: u8,
        length_hops: u8,
        backup_quantity: u8,
        length_variance: i8,
    ) -> Result<Self, ServiceTunnelError> {
        if inbound_quantity == 0 || inbound_quantity > MAX_TUNNEL_QUANTITY {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "inbound_quantity",
                reason: "must be within 1..=6",
            });
        }
        if outbound_quantity == 0 || outbound_quantity > MAX_TUNNEL_QUANTITY {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "outbound_quantity",
                reason: "must be within 1..=6",
            });
        }
        if length_hops == 0 || length_hops > MAX_TUNNEL_LENGTH_HOPS {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "length_hops",
                reason: "must be within 1..=3 (zero-hop is not permitted for service destinations)",
            });
        }
        if backup_quantity > MAX_TUNNEL_BACKUP_QUANTITY {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "backup_quantity",
                reason: "must be within 0..=3",
            });
        }
        if !(-MAX_TUNNEL_LENGTH_VARIANCE..=MAX_TUNNEL_LENGTH_VARIANCE).contains(&length_variance) {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "length_variance",
                reason: "must be within -2..=+2",
            });
        }
        if inbound_quantity.saturating_add(backup_quantity) > MAX_EFFECTIVE_DIRECTION_TUNNELS {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "backup_quantity",
                reason: "inbound quantity plus backup exceeds the pool directional maximum of 8",
            });
        }
        if outbound_quantity.saturating_add(backup_quantity) > MAX_EFFECTIVE_DIRECTION_TUNNELS {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "backup_quantity",
                reason: "outbound quantity plus backup exceeds the pool directional maximum of 8",
            });
        }
        Ok(Self {
            inbound_quantity,
            outbound_quantity,
            length_hops,
            backup_quantity,
            length_variance,
        })
    }
}

/// Round-robin dial start for one multihomed server connection
/// (Plan 296): the `connection_sequence`-th connection starts at
/// `sequence % target_count`, then fails over sequentially. The
/// daemon owns the per-runtime monotonic sequence counter; this
/// helper pins the rotation contract. An empty list selects 0
/// (callers guarantee non-empty; the dial fails closed with no
/// targets).
pub fn multihoming_start_index(connection_sequence: usize, target_count: usize) -> usize {
    if target_count == 0 {
        return 0;
    }
    connection_sequence % target_count
}

/// Minimum idle deadline in milliseconds (Plan 292).
pub const MIN_IDLE_TIMEOUT_MS: u64 = 1_000;
/// Maximum idle deadline in milliseconds (24 hours).
pub const MAX_IDLE_TIMEOUT_MS: u64 = 86_400_000;
/// Default idle deadline applied when idle action flags are set
/// without an explicit timeout (10 minutes, documented i2pr policy).
pub const DEFAULT_IDLE_TIMEOUT_MS: u64 = 600_000;
/// Proposal 170 idle times are expressed in minutes and allow up to
/// 9999 minutes (converted to milliseconds at the control boundary).
pub const MAX_PROPOSAL_IDLE_TIMEOUT_MS: u64 = 9999 * 60 * 1000;

/// Validated per-tunnel idle policy (Plan 292).
///
/// A deadline with no action flag is inert, so it is rejected;
/// action flags without a deadline take [`DEFAULT_IDLE_TIMEOUT_MS`]
/// at the control boundary. `None` (disabled) is the default: every
/// existing tunnel keeps its current always-on behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdlePolicy {
    /// Idle deadline in milliseconds (`None` disables the sweep).
    pub timeout_ms: Option<u64>,
    /// Stop the runtime once idle past the deadline.
    pub close_on_idle: bool,
    /// Rebuild tunnel pools in place once idle past the deadline
    /// (identity stable; ephemeral client identities follow the
    /// existing restart-regeneration behavior).
    pub new_dest_on_idle: bool,
    /// Rotate a dedicated client Destination after its close-on-idle
    /// deadline, while keeping the service active on the new identity.
    pub rotate_destination_on_idle: bool,
    /// Halve pool targets toward one once idle past the deadline
    /// (runtime-only; a restart restores the stored shaping).
    pub reduce_on_idle: bool,
    /// Proposal-specific close deadline in milliseconds. `None` uses
    /// `timeout_ms`; zero means close as soon as the tunnel is idle.
    pub close_timeout_ms: Option<u64>,
    /// Proposal-specific reduce deadline in milliseconds. `None` uses
    /// `timeout_ms`; zero means reduce as soon as the tunnel is idle.
    pub reduce_timeout_ms: Option<u64>,
    /// Number of tunnels removed from each pool on idle reduction. `None`
    /// retains the legacy bounded halving policy.
    pub reduce_count: Option<u8>,
}

impl IdlePolicy {
    /// Disabled policy: no deadline, no actions.
    pub fn disabled() -> Self {
        Self {
            timeout_ms: None,
            close_on_idle: false,
            new_dest_on_idle: false,
            rotate_destination_on_idle: false,
            reduce_on_idle: false,
            close_timeout_ms: None,
            reduce_timeout_ms: None,
            reduce_count: None,
        }
    }

    /// Validates an explicit policy. Rejects out-of-range deadlines.
    /// Inert policies (a deadline with no action) are rejected by
    /// spec validation and the control boundary, which own the
    /// service identifier for the diagnostic.
    pub fn try_new(
        timeout_ms: Option<u64>,
        close_on_idle: bool,
        new_dest_on_idle: bool,
        reduce_on_idle: bool,
    ) -> Result<Self, ServiceTunnelError> {
        if let Some(timeout) = timeout_ms {
            if !(MIN_IDLE_TIMEOUT_MS..=MAX_IDLE_TIMEOUT_MS).contains(&timeout) {
                return Err(ServiceTunnelError::ExceedsCeiling {
                    field: "idle_timeout",
                    reason: "must be within 1000..=86400000 milliseconds",
                });
            }
            if !(close_on_idle || new_dest_on_idle || reduce_on_idle) {
                return Err(ServiceTunnelError::ContradictoryOptions {
                    id: String::new(),
                    reason: "idle_timeout without an idle action is inert",
                });
            }
        }
        Ok(Self {
            timeout_ms,
            close_on_idle,
            new_dest_on_idle,
            rotate_destination_on_idle: false,
            reduce_on_idle,
            close_timeout_ms: None,
            reduce_timeout_ms: None,
            reduce_count: None,
        })
    }

    /// Validates both the legacy idle timeout and Proposal-specific
    /// per-action parameters. Proposal times are allowed to be zero and
    /// reach 9999 minutes, matching the wire contract.
    pub fn validate(self) -> Result<Self, ServiceTunnelError> {
        Self::try_new(
            self.timeout_ms,
            self.close_on_idle,
            self.new_dest_on_idle,
            self.reduce_on_idle,
        )?;
        if self.rotate_destination_on_idle && !self.close_on_idle {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "destination rotation on idle requires close_on_idle",
            });
        }
        if let Some(timeout) = self.close_timeout_ms
            && (!self.close_on_idle || timeout > MAX_PROPOSAL_IDLE_TIMEOUT_MS)
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "close_time requires close_on_idle and must not exceed 9999 minutes",
            });
        }
        if let Some(timeout) = self.reduce_timeout_ms
            && (!self.reduce_on_idle || timeout > MAX_PROPOSAL_IDLE_TIMEOUT_MS)
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "reduce_time requires reduce_on_idle and must not exceed 9999 minutes",
            });
        }
        if self.reduce_count.is_some_and(|_| !self.reduce_on_idle)
            || self.reduce_count.is_some_and(|count| count > 9)
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "reduce_count requires reduce_on_idle and must be within 0..=9",
            });
        }
        Ok(self)
    }

    /// Whether the sweep considers this policy.
    pub fn enabled(self) -> bool {
        (self.timeout_ms.is_some()
            || self.close_timeout_ms.is_some()
            || self.reduce_timeout_ms.is_some())
            && (self.close_on_idle
                || self.new_dest_on_idle
                || self.rotate_destination_on_idle
                || self.reduce_on_idle)
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
    /// I2P destination port for a shared server service. Dedicated
    /// servers default to the wildcard port 0 for compatibility.
    pub inbound_port: Option<u16>,
    /// Per-service active-connection ceiling.
    pub max_connections: usize,
    /// Per-direction buffered-byte ceiling.
    pub max_buffered_bytes_per_direction: usize,
    /// Service deadlines.
    pub timeouts: ServiceTimeouts,
    /// Pool shaping (length/quantity projection into the destination
    /// tunnel pool). Defaults to [`TunnelShaping::balanced`].
    pub shaping: TunnelShaping,
    /// Interactive streaming profile (Plan 292): selects the
    /// constrained-window streaming configuration for
    /// latency-sensitive tunnels. `false` keeps the balanced
    /// windows. Streamr kinds must leave this unset (the datagram
    /// path has no streaming windows).
    pub streaming_interactive: bool,
    /// Idle policy (Plan 292): deadline-gated close, pool rebuild,
    /// or pool reduction for quiet tunnels. Disabled by default.
    pub idle: IdlePolicy,
    /// Inbound peer allow/deny policy (Plan 292). Only server kinds
    /// may carry entries; every other kind must stay empty.
    pub access: crate::access::ServerAccessPolicy,
    /// Per-peer loopback source bind on server-to-target dials
    /// (Plan 292 `unique_local_address`). When set, the server
    /// dials its TCP target from a deterministic 127/8 address
    /// derived from the peer hash instead of the default
    /// wildcard source. Only the masked server kinds
    /// (generic, HTTP server, bidirectional) may set it; the
    /// option has no consuming dial for any other kind.
    pub unique_local_address: bool,
    /// Server target selection across the configured target list
    /// (Plan 296 `multihoming`). When set, each inbound connection
    /// dials a round-robin-selected target with sequential failover
    /// instead of the first target only; the flag requires at
    /// least two configured targets. Unset keeps first-target
    /// failover (configuration-layer fallback, no wire key).
    /// Only the masked server kinds (generic, HTTP server,
    /// bidirectional) may set it.
    pub multihoming: bool,
    /// Garlic reply bundling on the destination delivery path
    /// (Plan 296 `reply_bundling`). When set, the outbound sweep
    /// may carry multiple same-remote application payloads as
    /// multiple data cloves in one New Session Reply garlic
    /// message. Unset keeps one payload per garlic message. All
    /// kinds may set it.
    pub reply_bundling: bool,
    /// TLS to the loopback target on server-to-target dials
    /// (Plan 297 `use_ssl`). When set, the server negotiates TLS
    /// to the configured loopback target using the daemon's
    /// explicit TLS identity/trust policy before proxying
    /// application bytes; verification failure fails the
    /// connection with no plaintext fallback. Unset keeps
    /// plaintext. Only the masked server kinds (generic, HTTP
    /// server, bidirectional) may set it, and only with
    /// loopback-TCP targets.
    pub use_ssl: bool,
    /// HTTP server presentation policy (Plan 292
    /// `address_helper` / `jump_list` gates). Only the HTTP
    /// server kinds consume it; every other kind must carry
    /// the default (both gates open).
    pub http_policy: crate::http::HttpServerPolicy,
    /// HTTP-specific profile options. Mandatory for `HttpClient`
    /// and `HttpBidirServer` (client half) kinds; ignored otherwise.
    pub http_options: Option<crate::http::HttpClientOptions>,
    /// SOCKS5-specific profile options. Mandatory for
    /// `Socks5Client` and `SocksIrc` kinds; ignored otherwise.
    pub socks5_options: Option<crate::socks5::Socks5ClientOptions>,
    /// IRC-specific profile options. Mandatory for `IrcClient` and
    /// `SocksIrc` kinds; ignored otherwise.
    pub irc_options: Option<crate::irc::IrcClientOptions>,
    /// Strict CONNECT profile options. Mandatory for
    /// `ConnectClient` kinds; ignored otherwise.
    pub connect_options: Option<crate::connect::ConnectClientOptions>,
    /// Streamr profile options. Mandatory for `StreamrClient` and
    /// `StreamrServer` kinds; ignored otherwise.
    pub streamr_options: Option<crate::streamr::StreamrOptions>,
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
        if self.timeouts.streaming_connect_delay_ms.is_some()
            && self.kind != ServiceTunnelKind::GenericClient
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: self.id.as_str().to_owned(),
                reason: "streaming connect delay applies to generic client tunnels only",
            });
        }
        TunnelShaping::try_new(
            self.shaping.inbound_quantity,
            self.shaping.outbound_quantity,
            self.shaping.length_hops,
            self.shaping.backup_quantity,
            self.shaping.length_variance,
        )?;
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
            | ServiceTunnelKind::IrcClient
            | ServiceTunnelKind::ConnectClient
            | ServiceTunnelKind::SocksIrc => {
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
            ServiceTunnelKind::GenericServer
            | ServiceTunnelKind::IrcServer
            | ServiceTunnelKind::HttpServer => {
                if matches!(
                    self.policy,
                    DestinationPolicy::PersistentClient
                        | DestinationPolicy::PersistentSharedClientGroup(_)
                ) {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "persistent client-key policy applies to client services only",
                    });
                }
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
                let explicit_group = matches!(
                    self.policy,
                    DestinationPolicy::SharedClientGroup(_)
                        | DestinationPolicy::PersistentSharedClientGroup(_)
                        | DestinationPolicy::SharedGroup(_)
                );
                if explicit_group && self.inbound_port.is_none_or(|port| port == 0) {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "shared server services require a non-zero inbound_port",
                    });
                }
            }
            // Plan 290: the bidirectional HTTP server carries both a
            // loopback client listener and a loopback/unix server
            // target under one lifecycle generation and one
            // persistent server identity. It never carries a remote
            // destination reference: the client half resolves
            // per-request destinations like `http-client` (no
            // outproxy), and only the server half publishes.
            ServiceTunnelKind::HttpBidirServer => {
                if self.listener.is_none() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "http-bidir-server requires a loopback listener for the client half",
                    });
                }
                if self.target.is_none() && self.targets.is_empty() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "http-bidir-server requires a loopback or unix target for the server half",
                    });
                }
                if self.destination.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "http-bidir-server must not carry a remote destination reference",
                    });
                }
                if matches!(
                    self.policy,
                    DestinationPolicy::PersistentClient
                        | DestinationPolicy::PersistentSharedClientGroup(_)
                        | DestinationPolicy::SharedClientGroup(_)
                ) {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "http-bidir-server requires a dedicated destination",
                    });
                }
            }
            // Plan 291: the Streamr subscriber carries no TCP
            // listener and no TCP target. It resolves the
            // configured producer destination, subscribes over
            // repliable datagrams, and forwards raw media to the
            // loopback UDP target. The UDP endpoints live in
            // `streamr_options`, never in `listener`/`target`.
            ServiceTunnelKind::StreamrClient => {
                if self.listener.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-client must not carry a TCP listener",
                    });
                }
                if self.target.is_some() || !self.targets.is_empty() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-client must not carry a TCP server target",
                    });
                }
                if self.destination.is_none() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-client requires the producer destination reference",
                    });
                }
                if matches!(
                    self.policy,
                    DestinationPolicy::SharedClientGroup(_)
                        | DestinationPolicy::PersistentSharedClientGroup(_)
                ) {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-client requires a dedicated destination",
                    });
                }
            }
            // Plan 291: the Streamr publisher carries no TCP
            // listener, no TCP target, and no remote destination.
            // It publishes one persistent destination, receives
            // media on the loopback UDP source, and fans raw media
            // out to authenticated subscribers.
            ServiceTunnelKind::StreamrServer => {
                if self.listener.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-server must not carry a TCP listener",
                    });
                }
                if self.target.is_some() || !self.targets.is_empty() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-server must not carry a TCP server target",
                    });
                }
                if self.destination.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-server must not carry a remote destination reference",
                    });
                }
                if matches!(
                    self.policy,
                    DestinationPolicy::PersistentClient
                        | DestinationPolicy::SharedClientGroup(_)
                        | DestinationPolicy::PersistentSharedClientGroup(_)
                ) {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-server requires a dedicated destination",
                    });
                }
            }
        }
        if self.kind.is_client() && self.inbound_port.is_some() {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: self.id.as_str().to_owned(),
                reason: "inbound_port is only valid for server services",
            });
        }
        // Plan 176 + Plan 290: http-client and the http-bidir-server
        // client half must carry HTTP profile options; non-HTTP
        // kinds must not.
        match self.kind {
            ServiceTunnelKind::HttpClient | ServiceTunnelKind::HttpBidirServer => {
                let options = self.http_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "http-client kinds require http_options",
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
        // Plan 177 + Plan 290: socks5-client and socks-irc must
        // carry SOCKS5 profile options; other kinds must not.
        match self.kind {
            ServiceTunnelKind::Socks5Client | ServiceTunnelKind::SocksIrc => {
                let options = self.socks5_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "socks kinds require socks5_options",
                    }
                })?;
                options.validate()?;
            }
            _ => {
                if self.socks5_options.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "socks5_options must not be set for non-SOCKS kinds",
                    });
                }
            }
        }
        // Plan 178 + Plan 290: irc-client and socks-irc must carry
        // IRC profile options; other kinds must not.
        match self.kind {
            ServiceTunnelKind::IrcClient | ServiceTunnelKind::SocksIrc => {
                let options = self.irc_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "irc kinds require irc_options",
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
        // Plan 290: connect-client must carry strict CONNECT profile
        // options; other kinds must not.
        match self.kind {
            ServiceTunnelKind::ConnectClient => {
                let options = self.connect_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "connect-client requires connect_options",
                    }
                })?;
                options.validate()?;
            }
            _ => {
                if self.connect_options.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "connect_options must not be set for non-CONNECT kinds",
                    });
                }
            }
        }
        // Plan 291: streamr halves must carry Streamr profile
        // options with the half-appropriate UDP endpoint present;
        // other kinds must not.
        match self.kind {
            ServiceTunnelKind::StreamrClient => {
                let options = self.streamr_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "streamr-client requires streamr_options",
                    }
                })?;
                options.validate()?;
                if options.local_udp.is_none() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-client requires the loopback UDP media target",
                    });
                }
            }
            ServiceTunnelKind::StreamrServer => {
                let options = self.streamr_options.as_ref().ok_or_else(|| {
                    ServiceTunnelError::ContradictoryOptions {
                        id: id.clone(),
                        reason: "streamr-server requires streamr_options",
                    }
                })?;
                options.validate()?;
                if options.local_udp.is_none() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr-server requires the loopback UDP media source",
                    });
                }
                if options.remote_sink.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "remote_sink applies to streamr-client only",
                    });
                }
            }
            _ => {
                if self.streamr_options.is_some() {
                    return Err(ServiceTunnelError::ContradictoryOptions {
                        id,
                        reason: "streamr_options must not be set for non-Streamr kinds",
                    });
                }
            }
        }
        // Plan 292: the interactive streaming profile needs the
        // streaming stack, which Streamr kinds do not have.
        if self.streaming_interactive
            && matches!(
                self.kind,
                ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "streaming_interactive must not be set for Streamr kinds",
            });
        }
        // Plan 292: a deadline with no action is inert; name the
        // service (the constructor cannot).
        if (self.idle.timeout_ms.is_some()
            || self.idle.close_timeout_ms.is_some()
            || self.idle.reduce_timeout_ms.is_some())
            && !(self.idle.close_on_idle
                || self.idle.new_dest_on_idle
                || self.idle.rotate_destination_on_idle
                || self.idle.reduce_on_idle)
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "idle_timeout without an idle action is inert",
            });
        }
        self.idle.validate()?;
        if self.idle.rotate_destination_on_idle
            && (!self.idle.close_on_idle
                || !self.kind.is_client()
                || !matches!(&self.policy, DestinationPolicy::Dedicated))
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "Destination rotation on idle requires Close and a dedicated client Destination",
            });
        }
        // Plan 292: only server kinds terminate inbound I2P streams,
        // so only they may carry a peer policy.
        if !self.access.is_empty()
            && !matches!(
                self.kind,
                ServiceTunnelKind::GenericServer
                    | ServiceTunnelKind::HttpServer
                    | ServiceTunnelKind::HttpBidirServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "access lists apply to server kinds only",
            });
        }
        self.access.connection_rates.validate()?;
        if self.access.connection_rates.enabled()
            && !matches!(
                self.kind,
                ServiceTunnelKind::GenericServer
                    | ServiceTunnelKind::HttpServer
                    | ServiceTunnelKind::HttpBidirServer
                    | ServiceTunnelKind::IrcServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "connection-rate controls require a TCP server tunnel",
            });
        }
        // Plan 292: the deterministic source bind consumes the
        // server-to-target dial, which only the masked server
        // kinds perform.
        if self.unique_local_address
            && !matches!(
                self.kind,
                ServiceTunnelKind::GenericServer
                    | ServiceTunnelKind::HttpServer
                    | ServiceTunnelKind::HttpBidirServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "unique_local_address applies to server kinds only",
            });
        }
        // Plan 292: the presentation gates consume the HTTP
        // server filter, which only the HTTP server kinds run.
        if self.http_policy != crate::http::HttpServerPolicy::default()
            && !matches!(
                self.kind,
                ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "HTTP server policy and SpoofedHost apply to HTTP server kinds only",
            });
        }
        if self
            .http_policy
            .spoofed_host
            .as_deref()
            .is_some_and(|host| !crate::http::valid_spoofed_host(host))
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "must be a bounded ASCII DNS hostname",
            });
        }
        if !crate::http::valid_user_agent_rules(&self.http_policy.user_agents) {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "HTTP server User-Agent rules exceed their count or value bounds",
            });
        }
        self.http_policy.post_limits.validate().map_err(|_| {
            ServiceTunnelError::ContradictoryOptions {
                id: id.clone(),
                reason: "HTTP POST limits must be bounded and include their required window/count",
            }
        })?;
        if self.http_policy.block_user_agents && self.http_policy.user_agents.is_empty() {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "HTTP server User-Agent blocking requires at least one rule",
            });
        }
        // Plan 296: multihoming consumes the server-to-target
        // dial target list, which only the masked server kinds
        // perform. The flag additionally requires at least two
        // configured targets: with a single target there is
        // nothing to select across, so accepting it would be
        // inert (never accepted inertly).
        if self.multihoming
            && !matches!(
                self.kind,
                ServiceTunnelKind::GenericServer
                    | ServiceTunnelKind::HttpServer
                    | ServiceTunnelKind::HttpBidirServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "multihoming applies to server kinds only",
            });
        }
        if self.multihoming {
            let target_count =
                usize::from(self.target.is_some()).saturating_add(self.targets.len());
            if target_count < 2 {
                return Err(ServiceTunnelError::ContradictoryOptions {
                    id,
                    reason: "multihoming requires at least two configured targets",
                });
            }
            // Multihoming selection dials loopback TCP in order;
            // Unix-domain targets have no TCP dial, so they cannot
            // take part in selection.
            let unix_target = matches!(self.target, Some(ServerTarget::UnixPath(_)))
                || self
                    .targets
                    .iter()
                    .any(|target| matches!(target, ServerTarget::UnixPath(_)));
            if unix_target {
                return Err(ServiceTunnelError::ContradictoryOptions {
                    id,
                    reason: "multihoming requires loopback-TCP targets",
                });
            }
        }
        // Plan 297: server TLS terminates on the loopback TCP
        // target leg, which only the masked server kinds dial;
        // Unix-domain targets have no TLS handshake.
        if self.use_ssl
            && !matches!(
                self.kind,
                ServiceTunnelKind::GenericServer
                    | ServiceTunnelKind::HttpServer
                    | ServiceTunnelKind::HttpBidirServer
            )
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id,
                reason: "use_ssl applies to server kinds only",
            });
        }
        if self.use_ssl {
            let unix_target = matches!(self.target, Some(ServerTarget::UnixPath(_)))
                || self
                    .targets
                    .iter()
                    .any(|target| matches!(target, ServerTarget::UnixPath(_)));
            if unix_target {
                return Err(ServiceTunnelError::ContradictoryOptions {
                    id,
                    reason: "use_ssl requires loopback-TCP targets",
                });
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
        let mut server_ports = std::collections::HashSet::new();
        for spec in &self.tunnels {
            if !ids.insert(spec.id.as_str().to_owned()) {
                return Err(ServiceTunnelError::DuplicateId {
                    value: spec.id.as_str().to_owned(),
                });
            }
            spec.validate()?;
            if spec.kind.is_server()
                && let Some(port) = spec.inbound_port
                && let DestinationPolicy::SharedClientGroup(group)
                | DestinationPolicy::PersistentSharedClientGroup(group)
                | DestinationPolicy::SharedGroup(group) = &spec.policy
                && !server_ports.insert((group.as_str().to_owned(), port))
            {
                return Err(ServiceTunnelError::ContradictoryOptions {
                    id: spec.id.as_str().to_owned(),
                    reason: "server inbound_port must be unique within a Destination group",
                });
            }
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

    /// Resolves configured services into explicit Destination-group
    /// owners. A group is persistent when any member is a server.
    pub fn destination_groups(&self) -> Vec<DestinationGroupSpec> {
        let mut groups =
            std::collections::BTreeMap::<DestinationGroupKey, DestinationGroupSpec>::new();
        for service in &self.tunnels {
            let key = service.policy.group_key(&service.id);
            let group = groups
                .entry(key.clone())
                .or_insert_with(|| DestinationGroupSpec {
                    key,
                    members: Vec::new(),
                    persistent: false,
                });
            group.members.push(service.id.clone());
            group.persistent |= service.policy.persists_client_identity()
                || service.kind.is_server()
                || matches!(service.kind, ServiceTunnelKind::HttpBidirServer);
        }
        groups.into_values().collect()
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
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }
    }

    fn server_spec(
        id: &str,
        target: &str,
        group: DestinationGroupId,
        inbound_port: u16,
    ) -> ServiceTunnelSpec {
        ServiceTunnelSpec {
            id: ServiceTunnelId::parse(id).expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: false,
            listener: None,
            target: Some(ServerTarget::parse(target).expect("target")),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::SharedGroup(group),
            inbound_port: Some(inbound_port),
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }
    }

    fn canonical_b32() -> String {
        format!("{}.b32.i2p", "a".repeat(52))
    }

    #[test]
    fn streamr_with_interactive_profile_is_contradictory() {
        // Plan 292: Streamr kinds ride the datagram path, so the
        // interactive streaming bit must stay unset for them.
        let base = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        let mut spec = ServiceTunnelSpec {
            kind: ServiceTunnelKind::StreamrServer,
            listener: None,
            target: None,
            destination: None,
            ..base
        };
        let streamr = crate::streamr::StreamrOptions {
            local_udp: Some("127.0.0.1:5001".parse().expect("udp")),
            ..crate::streamr::StreamrOptions::default()
        };
        spec.streamr_options = Some(streamr);
        spec.streaming_interactive = true;
        assert!(spec.validate().is_err());
        spec.streaming_interactive = false;
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn shaping_bounds_follow_proposal_ceilings() {
        // Plan 292: quantity 1..=6, length 1..=3; zero-hop rejected.
        // Plan 296: backup 0..=3, variance -2..=+2, and each
        // direction's quantity plus backup fits the pool maximum.
        let shaped = TunnelShaping::try_new(4, 5, 3, 0, 0).expect("shaping");
        assert_eq!(shaped.inbound_quantity, 4);
        assert_eq!(shaped.outbound_quantity, 5);
        assert_eq!(shaped.length_hops, 3);
        assert_eq!(shaped.backup_quantity, 0);
        assert_eq!(shaped.length_variance, 0);
        for (inbound, outbound, length) in [
            (0, 2, 2),
            (7, 2, 2),
            (2, 0, 2),
            (2, 7, 2),
            (2, 2, 0),
            (2, 2, 4),
        ] {
            assert!(
                TunnelShaping::try_new(inbound, outbound, length, 0, 0).is_err(),
                "shaping ({inbound}, {outbound}, {length}) must fail"
            );
        }
        // Balanced shaping is the pre-292 default.
        assert_eq!(
            TunnelShaping::balanced(),
            TunnelShaping::try_new(2, 2, 2, 0, 0).expect("balanced")
        );
    }

    #[test]
    fn shaping_backup_and_variance_bounds() {
        // Plan 296: Proposal bounds bind (backup 0..=3, variance
        // -2..=+2); per-direction quantity plus backup fits the
        // pool directional maximum of 8.
        let shaped = TunnelShaping::try_new(2, 3, 2, 3, -2).expect("shaping");
        assert_eq!(shaped.backup_quantity, 3);
        assert_eq!(shaped.length_variance, -2);
        let shaped = TunnelShaping::try_new(6, 6, 3, 2, 2).expect("shaping");
        assert_eq!(shaped.backup_quantity, 2);
        assert_eq!(shaped.length_variance, 2);
        for (inbound, outbound, backup, variance) in [
            (2, 2, 4, 0),
            (2, 2, u8::MAX, 0),
            (6, 6, 3, 0),
            (6, 2, 3, 0),
            (2, 6, 3, 0),
            (2, 2, 0, 3),
            (2, 2, 0, -3),
            (2, 2, 0, i8::MAX),
            (2, 2, 0, i8::MIN),
        ] {
            assert!(
                TunnelShaping::try_new(inbound, outbound, 2, backup, variance).is_err(),
                "shaping backup {backup} variance {variance} must fail"
            );
        }
    }

    #[test]
    fn multihoming_needs_server_kind_and_two_targets() {
        // Plan 296: multihoming consumes the server dial target
        // list; client kinds and single-target servers reject it.
        let mut server = ServiceTunnelSpec {
            id: ServiceTunnelId::parse("mh-server").expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: false,
            listener: None,
            target: Some(ServerTarget::LoopbackTcp(
                "127.0.0.1:8080".parse().expect("addr"),
            )),
            targets: vec![ServerTarget::LoopbackTcp(
                "127.0.0.1:8081".parse().expect("addr"),
            )],
            destination: None,
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: true,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        };
        assert!(server.validate().is_ok());
        // A single target leaves nothing to select across.
        server.targets.clear();
        assert!(server.validate().is_err());
        // Unix targets cannot take part in TCP selection.
        server.targets = vec![ServerTarget::UnixPath("/tmp/mh.sock".to_owned())];
        assert!(server.validate().is_err());
        // Client kinds never dial a server target.
        let mut client = client_spec("mh-client", "127.0.0.1:7070", "example.i2p");
        client.multihoming = true;
        assert!(client.validate().is_err());
    }

    #[test]
    fn use_ssl_needs_server_kind_and_tcp_targets() {
        // Plan 297: server TLS terminates on the loopback TCP
        // target leg; client kinds and Unix targets reject it.
        let mut server = ServiceTunnelSpec {
            id: ServiceTunnelId::parse("tls-server").expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: false,
            listener: None,
            target: Some(ServerTarget::LoopbackTcp(
                "127.0.0.1:8443".parse().expect("addr"),
            )),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: true,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        };
        assert!(server.validate().is_ok());
        server.targets = vec![ServerTarget::LoopbackTcp(
            "127.0.0.1:8444".parse().expect("addr"),
        )];
        assert!(server.validate().is_ok());
        server.target = Some(ServerTarget::UnixPath("/tmp/tls.sock".to_owned()));
        assert!(server.validate().is_err());
        let mut client = client_spec("tls-client", "127.0.0.1:7070", "example.i2p");
        client.use_ssl = true;
        assert!(client.validate().is_err());
    }

    #[test]
    fn multihoming_selection_rotates_in_order() {
        // Plan 296: the rotation contract is sequence modulo
        // target count; an empty list selects 0 and fails closed
        // at the dial.
        assert_eq!(multihoming_start_index(0, 3), 0);
        assert_eq!(multihoming_start_index(1, 3), 1);
        assert_eq!(multihoming_start_index(2, 3), 2);
        assert_eq!(multihoming_start_index(3, 3), 0);
        assert_eq!(multihoming_start_index(4, 1), 0);
        assert_eq!(multihoming_start_index(7, 0), 0);
    }

    #[test]
    fn idle_policy_bounds_and_inert_rejection() {
        // Plan 292: deadlines are milliseconds within
        // 1000..=86400000; disabled is the default.
        assert!(!IdlePolicy::disabled().enabled());
        let armed = IdlePolicy::try_new(Some(60_000), true, false, false).expect("armed");
        assert!(armed.enabled());
        for timeout in [0, 999, 86_400_001, u64::MAX] {
            assert!(
                IdlePolicy::try_new(Some(timeout), true, false, false).is_err(),
                "timeout {timeout} must fail"
            );
        }
        // A deadline with no action is inert: the constructor
        // rejects it, and spec validation names the service.
        assert!(IdlePolicy::try_new(Some(60_000), false, false, false).is_err());
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.idle = IdlePolicy {
            timeout_ms: Some(60_000),
            close_on_idle: false,
            new_dest_on_idle: false,
            rotate_destination_on_idle: false,
            reduce_on_idle: false,
            close_timeout_ms: None,
            reduce_timeout_ms: None,
            reduce_count: None,
        };
        assert!(spec.validate().is_err());
        spec.idle = IdlePolicy::try_new(Some(60_000), false, true, false).expect("idle");
        assert!(spec.validate().is_ok());
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
            ("connect-client", ServiceTunnelKind::ConnectClient),
            ("socks-irc", ServiceTunnelKind::SocksIrc),
            ("http-server", ServiceTunnelKind::HttpServer),
            ("http-bidir-server", ServiceTunnelKind::HttpBidirServer),
        ];
        for (text, expected) in cases {
            assert_eq!(ServiceTunnelKind::parse(text).expect("kind"), expected);
            assert_eq!(expected.as_str(), text);
        }
        assert!(ServiceTunnelKind::parse("httpserver").is_err());
        assert!(ServiceTunnelKind::parse("GENERIC-CLIENT").is_err());
        assert!(ServiceTunnelKind::parse("").is_err());
        // Server-side classification covers the generic, IRC, and
        // HTTP server profiles; the bidirectional profile reports
        // client-side (it always carries a loopback listener).
        assert!(ServiceTunnelKind::HttpServer.is_server());
        assert!(!ServiceTunnelKind::HttpServer.is_client());
        assert!(!ServiceTunnelKind::HttpBidirServer.is_server());
        assert!(ServiceTunnelKind::HttpBidirServer.is_client());
        assert!(!ServiceTunnelKind::ConnectClient.is_server());
        assert!(!ServiceTunnelKind::SocksIrc.is_server());
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
    fn explicit_groups_share_across_clients_and_persist_if_any_member_is_server() {
        let group = DestinationGroupId::parse("shared").expect("group");
        let mut http = client_spec("http-one", "127.0.0.1:8081", &canonical_b32());
        http.kind = ServiceTunnelKind::HttpClient;
        http.http_options = Some(crate::http::HttpClientOptions::defaults());
        http.policy = DestinationPolicy::SharedGroup(group.clone());
        let mut socks = client_spec("socks-one", "127.0.0.1:8082", &canonical_b32());
        socks.kind = ServiceTunnelKind::Socks5Client;
        socks.socks5_options = Some(crate::socks5::Socks5ClientOptions::defaults());
        socks.policy = DestinationPolicy::SharedGroup(group.clone());
        let server = server_spec("server-one", "127.0.0.1:8083", group.clone(), 80);
        let second_server = server_spec("server-two", "127.0.0.1:8084", group, 81);
        let set = ServiceTunnelSet {
            tunnels: vec![http, socks, server, second_server],
        };
        set.validate().expect("mixed explicit group is intentional");
        let groups = set.destination_groups();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].persistent);
        assert_eq!(
            groups[0]
                .members
                .iter()
                .map(ServiceTunnelId::as_str)
                .collect::<Vec<_>>(),
            vec!["http-one", "socks-one", "server-one", "server-two"]
        );
    }

    #[test]
    fn shared_server_destination_ports_must_be_unique() {
        let group = DestinationGroupId::parse("shared").expect("group");
        let first = server_spec("server-one", "127.0.0.1:8083", group.clone(), 80);
        let second = server_spec("server-two", "127.0.0.1:8084", group, 80);
        let set = ServiceTunnelSet {
            tunnels: vec![first, second],
        };
        assert!(matches!(
            set.validate(),
            Err(ServiceTunnelError::ContradictoryOptions { .. })
        ));
    }

    #[test]
    fn dedicated_same_kind_services_get_distinct_implicit_groups() {
        let first = client_spec("http-one", "127.0.0.1:8081", &canonical_b32());
        let second = client_spec("http-two", "127.0.0.1:8082", &canonical_b32());
        let set = ServiceTunnelSet {
            tunnels: vec![first, second],
        };
        let groups = set.destination_groups();
        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0].key, groups[1].key);
        assert!(groups.iter().all(|group| !group.persistent));
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
    fn contradictory_options_rejected_and_server_groups_allowed() {
        // Client without destination.
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.destination = None;
        assert!(spec.validate().is_err());
        // Client with server target.
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.target = Some(ServerTarget::parse("127.0.0.1:9090").expect("target"));
        assert!(spec.validate().is_err());
        // Server services may intentionally share a Destination group.
        let server = ServiceTunnelSpec {
            id: ServiceTunnelId::parse("srv").expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: false,
            listener: None,
            target: Some(ServerTarget::parse("127.0.0.1:9090").expect("target")),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::SharedGroup(
                DestinationGroupId::parse("group").expect("group"),
            ),
            inbound_port: Some(9090),
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        };
        assert!(server.validate().is_ok());
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

    #[test]
    fn connect_client_requires_options() {
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.kind = ServiceTunnelKind::ConnectClient;
        assert!(spec.validate().is_err());
        spec.connect_options = Some(crate::connect::ConnectClientOptions::default());
        spec.validate().expect("connect options validate");
        // Non-CONNECT kinds must not carry connect_options.
        spec.kind = ServiceTunnelKind::GenericClient;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn socks_irc_requires_both_option_sets() {
        let mut spec = client_spec("alpha", "127.0.0.1:8080", &canonical_b32());
        spec.kind = ServiceTunnelKind::SocksIrc;
        assert!(spec.validate().is_err());
        spec.socks5_options = Some(crate::socks5::Socks5ClientOptions::default());
        assert!(spec.validate().is_err(), "irc_options still missing");
        spec.irc_options = Some(crate::irc::IrcClientOptions::default());
        spec.validate().expect("socks-irc options validate");
    }

    #[test]
    fn http_server_is_server_sided_without_options() {
        let spec = ServiceTunnelSpec {
            id: ServiceTunnelId::parse("web").expect("id"),
            kind: ServiceTunnelKind::HttpServer,
            enabled: false,
            listener: None,
            target: Some(ServerTarget::parse("127.0.0.1:8080").expect("target")),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        };
        spec.validate().expect("http-server validates");
        // A listener or a remote destination contradicts the server profile.
        let mut bad = spec.clone();
        bad.listener = Some(LocalListenerSpec::parse_socket("127.0.0.1:8081").expect("listener"));
        assert!(bad.validate().is_err());
        let mut bad = spec.clone();
        bad.destination = Some(DestinationRef::parse(&canonical_b32()).expect("destination"));
        assert!(bad.validate().is_err());
    }

    #[test]
    fn http_bidir_server_carries_both_halves() {
        let spec = ServiceTunnelSpec {
            id: ServiceTunnelId::parse("bidir").expect("id"),
            kind: ServiceTunnelKind::HttpBidirServer,
            enabled: false,
            listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:8080").expect("listener")),
            target: Some(ServerTarget::parse("127.0.0.1:8081").expect("target")),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: Some(crate::http::HttpClientOptions::default()),
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        };
        spec.validate().expect("http-bidir-server validates");
        // Missing either half fails.
        let mut bad = spec.clone();
        bad.listener = None;
        assert!(bad.validate().is_err());
        let mut bad = spec.clone();
        bad.target = None;
        assert!(bad.validate().is_err());
        // The client half requires http_options.
        let mut bad = spec.clone();
        bad.http_options = None;
        assert!(bad.validate().is_err());
        // A remote destination reference contradicts the single-identity rule.
        let mut bad = spec.clone();
        bad.destination = Some(DestinationRef::parse(&canonical_b32()).expect("destination"));
        assert!(bad.validate().is_err());
    }

    #[test]
    fn streamr_kinds_parse_with_kebab_spellings() {
        assert_eq!(
            ServiceTunnelKind::parse("streamr-client").expect("parse"),
            ServiceTunnelKind::StreamrClient
        );
        assert_eq!(
            ServiceTunnelKind::parse("streamr-server").expect("parse"),
            ServiceTunnelKind::StreamrServer
        );
        assert_eq!(ServiceTunnelKind::StreamrClient.as_str(), "streamr-client");
        assert_eq!(ServiceTunnelKind::StreamrServer.as_str(), "streamr-server");
        assert!(ServiceTunnelKind::StreamrServer.is_server());
        assert!(!ServiceTunnelKind::StreamrClient.is_server());
    }

    fn streamr_client_spec() -> ServiceTunnelSpec {
        ServiceTunnelSpec {
            id: ServiceTunnelId::parse("streamr-sub").expect("id"),
            kind: ServiceTunnelKind::StreamrClient,
            enabled: false,
            listener: None,
            target: None,
            targets: Vec::new(),
            destination: Some(DestinationRef::parse(&canonical_b32()).expect("destination")),
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: Some(crate::streamr::StreamrOptions {
                local_udp: Some("127.0.0.1:5000".parse().expect("udp")),
                ..crate::streamr::StreamrOptions::default()
            }),
        }
    }

    fn streamr_server_spec() -> ServiceTunnelSpec {
        ServiceTunnelSpec {
            id: ServiceTunnelId::parse("streamr-pub").expect("id"),
            kind: ServiceTunnelKind::StreamrServer,
            enabled: false,
            listener: None,
            target: None,
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 16,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: Some(crate::streamr::StreamrOptions {
                local_udp: Some("127.0.0.1:5001".parse().expect("udp")),
                ..crate::streamr::StreamrOptions::default()
            }),
        }
    }

    #[test]
    fn streamr_client_requires_producer_and_media_target() {
        streamr_client_spec().validate().expect("validates");
        // No options at all fails.
        let mut bad = streamr_client_spec();
        bad.streamr_options = None;
        assert!(bad.validate().is_err());
        // No media endpoint at all fails.
        let mut bad = streamr_client_spec();
        bad.streamr_options.as_mut().expect("options").local_udp = None;
        assert!(bad.validate().is_err());
        // TCP listener/target contradict the datagram profile.
        let mut bad = streamr_client_spec();
        bad.listener = Some(LocalListenerSpec::parse_socket("127.0.0.1:8080").expect("listener"));
        assert!(bad.validate().is_err());
        let mut bad = streamr_client_spec();
        bad.destination = None;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn streamr_server_requires_media_source() {
        streamr_server_spec().validate().expect("validates");
        let mut bad = streamr_server_spec();
        bad.streamr_options = None;
        assert!(bad.validate().is_err());
        let mut bad = streamr_server_spec();
        bad.streamr_options.as_mut().expect("options").local_udp = None;
        assert!(bad.validate().is_err());
        let mut bad = streamr_server_spec();
        bad.destination = Some(DestinationRef::parse(&canonical_b32()).expect("destination"));
        assert!(bad.validate().is_err());
    }

    #[test]
    fn non_streamr_kinds_must_not_carry_streamr_options() {
        let mut spec = streamr_client_spec();
        spec.kind = ServiceTunnelKind::GenericClient;
        spec.listener = Some(LocalListenerSpec::parse_socket("127.0.0.1:8080").expect("listener"));
        assert!(spec.validate().is_err());
    }
}
