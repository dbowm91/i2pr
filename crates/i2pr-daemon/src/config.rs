//! Strict versioned configuration parsing and side-effect-free normalization.

use std::fs;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

/// Only schema version understood by this bootstrap.
pub const CURRENT_SCHEMA_VERSION: u64 = 1;
/// Default task budget used when `[limits]` is omitted.
pub const DEFAULT_MAX_TASKS: u64 = 4_096;
/// Default buffered-byte budget used when `[limits]` is omitted.
pub const DEFAULT_MAX_BUFFERED_BYTES: u64 = 67_108_864;
const MAX_ALLOWED_TASKS: u64 = 1_000_000;
const MAX_ALLOWED_BUFFERED_BYTES: u64 = 1_u64 << 40;
const MAX_ALLOWED_DURATION_SECS: u64 = 3_600;
const MAX_ALLOWED_PREFIX_IPV4: u8 = 32;
const MAX_ALLOWED_PREFIX_IPV6: u8 = 128;
const MAX_ALLOWED_NETDB_RECORDS: u64 = 65_536;
const MAX_ALLOWED_NETDB_ENCODED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ALLOWED_RESEED_SOURCES: usize = 16;
const MAX_ALLOWED_RESEED_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ALLOWED_BOOTSTRAP_RECORDS: u64 = 65_536;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    schema_version: u64,
    router: RawRouterConfig,
    #[serde(default)]
    logging: RawLoggingConfig,
    #[serde(default)]
    limits: RawLimitsConfig,
    #[serde(default)]
    network: RawNetworkConfig,
    #[serde(default)]
    transport: RawTransportConfig,
    #[serde(default)]
    netdb: RawNetDbConfig,
    #[serde(default)]
    reseed: RawReseedConfig,
    #[serde(default)]
    console: RawConsoleConfig,
    #[serde(default)]
    app_runtime: RawAppRuntimeConfig,
    #[serde(default)]
    sam: RawSamConfig,
    #[serde(default)]
    ssu2: RawSsu2Config,
    #[serde(default)]
    i2cp: RawI2cpConfig,
    #[serde(default)]
    i2pcontrol: RawI2pControlConfig,
    #[serde(default)]
    service_tunnels: RawServiceTunnelsConfig,
    #[serde(default)]
    addressbook: RawAddressBookConfig,
    #[serde(default)]
    news: RawNewsConfig,
    #[serde(default)]
    floodfill: RawFloodfillConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRouterConfig {
    data_dir: String,
    #[serde(default = "default_profile")]
    profile: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLoggingConfig {
    #[serde(default = "default_filter")]
    filter: String,
    #[serde(default = "default_log_format")]
    format: String,
}

impl Default for RawLoggingConfig {
    fn default() -> Self {
        Self {
            filter: default_filter(),
            format: default_log_format(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLimitsConfig {
    #[serde(default = "default_max_tasks")]
    max_tasks: u64,
    #[serde(default = "default_max_buffered_bytes")]
    max_buffered_bytes: u64,
}

impl Default for RawLimitsConfig {
    fn default() -> Self {
        Self {
            max_tasks: default_max_tasks(),
            max_buffered_bytes: default_max_buffered_bytes(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetworkConfig {
    #[serde(default = "default_bind_address")]
    bind_address: String,
    #[serde(default = "default_listen_port")]
    listen_port: u16,
    #[serde(default = "default_network_id")]
    network_id: u16,
}

impl Default for RawNetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            listen_port: default_listen_port(),
            network_id: default_network_id(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTransportConfig {
    #[serde(default)]
    ntcp2: RawNtcp2Config,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNtcp2Config {
    #[serde(default = "default_ntcp2_enabled")]
    enabled: bool,
    #[serde(default = "default_ntcp2_connect_timeout_ms")]
    connect_timeout_ms: u64,
    #[serde(default = "default_ntcp2_handshake_timeout_ms")]
    handshake_timeout_ms: u64,
    #[serde(default = "default_ntcp2_read_idle_timeout_ms")]
    read_idle_timeout_ms: u64,
    #[serde(default = "default_ntcp2_write_timeout_ms")]
    write_timeout_ms: u64,
    #[serde(default = "default_ntcp2_queue_wait_timeout_ms")]
    queue_wait_timeout_ms: u64,
    #[serde(default = "default_ntcp2_drain_timeout_ms")]
    drain_timeout_ms: u64,
    #[serde(default = "default_ntcp2_max_active_links")]
    max_active_links: usize,
    #[serde(default = "default_ntcp2_max_replay_entries")]
    max_replay_entries: usize,
    #[serde(default = "default_ntcp2_ipv4_prefix")]
    ipv4_prefix: u8,
    #[serde(default = "default_ntcp2_ipv6_prefix")]
    ipv6_prefix: u8,
}

impl Default for RawNtcp2Config {
    fn default() -> Self {
        Self {
            enabled: default_ntcp2_enabled(),
            connect_timeout_ms: default_ntcp2_connect_timeout_ms(),
            handshake_timeout_ms: default_ntcp2_handshake_timeout_ms(),
            read_idle_timeout_ms: default_ntcp2_read_idle_timeout_ms(),
            write_timeout_ms: default_ntcp2_write_timeout_ms(),
            queue_wait_timeout_ms: default_ntcp2_queue_wait_timeout_ms(),
            drain_timeout_ms: default_ntcp2_drain_timeout_ms(),
            max_active_links: default_ntcp2_max_active_links(),
            max_replay_entries: default_ntcp2_max_replay_entries(),
            ipv4_prefix: default_ntcp2_ipv4_prefix(),
            ipv6_prefix: default_ntcp2_ipv6_prefix(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetDbConfig {
    #[serde(default = "default_netdb_enabled")]
    enabled: bool,
    #[serde(default = "default_netdb_max_records")]
    max_records: u64,
    #[serde(default = "default_netdb_max_encoded_bytes")]
    max_encoded_bytes: u64,
    #[serde(default = "default_netdb_min_router_infos")]
    min_router_infos: u64,
    #[serde(default = "default_netdb_min_floodfill_advertisers")]
    min_floodfill_advertisers: u64,
}

impl Default for RawNetDbConfig {
    fn default() -> Self {
        Self {
            enabled: default_netdb_enabled(),
            max_records: default_netdb_max_records(),
            max_encoded_bytes: default_netdb_max_encoded_bytes(),
            min_router_infos: default_netdb_min_router_infos(),
            min_floodfill_advertisers: default_netdb_min_floodfill_advertisers(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReseedConfig {
    #[serde(default = "default_reseed_enabled")]
    enabled: bool,
    #[serde(default = "default_reseed_max_sources")]
    max_sources: usize,
    #[serde(default = "default_reseed_max_su3_bytes")]
    max_su3_bytes: u64,
    #[serde(default)]
    sources: Vec<RawReseedSource>,
}

impl Default for RawReseedConfig {
    fn default() -> Self {
        Self {
            enabled: default_reseed_enabled(),
            max_sources: default_reseed_max_sources(),
            max_su3_bytes: default_reseed_max_su3_bytes(),
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReseedSource {
    #[serde(default)]
    signer_id: String,
    #[serde(default)]
    certificate_path: String,
}

/// Raw Plan 167 I2CP listener configuration.
///
/// M9 (Milestone 9) I2CP listens on the conventional `127.0.0.1:7654`
/// loopback port. The listener is disabled by default; non-loopback
/// addresses are rejected during semantic validation. Independent
/// remote-client exposure, TLS, and credentialed authentication
/// remain deferred.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawI2cpConfig {
    #[serde(default = "default_i2cp_enabled")]
    enabled: bool,
    #[serde(default = "default_i2cp_bind_address")]
    bind_address: String,
    #[serde(default = "default_i2cp_port")]
    port: u16,
    #[serde(default = "default_i2cp_max_clients")]
    max_clients: u16,
    #[serde(default = "default_i2cp_max_sessions_per_connection")]
    max_sessions_per_connection: u16,
    #[serde(default = "default_i2cp_max_sessions_router")]
    max_sessions_router: u16,
    #[serde(default = "default_i2cp_max_buffered_bytes_per_connection")]
    max_buffered_bytes_per_connection: usize,
    #[serde(default = "default_i2cp_max_pending_writes_per_connection")]
    max_pending_writes_per_connection: u32,
    #[serde(default = "default_i2cp_protocol_byte_timeout_ms")]
    protocol_byte_timeout_ms: u64,
    #[serde(default = "default_i2cp_command_timeout_ms")]
    command_timeout_ms: u64,
    #[serde(default = "default_i2cp_shutdown_timeout_ms")]
    shutdown_timeout_ms: u64,
}

impl Default for RawI2cpConfig {
    fn default() -> Self {
        Self {
            enabled: default_i2cp_enabled(),
            bind_address: default_i2cp_bind_address(),
            port: default_i2cp_port(),
            max_clients: default_i2cp_max_clients(),
            max_sessions_per_connection: default_i2cp_max_sessions_per_connection(),
            max_sessions_router: default_i2cp_max_sessions_router(),
            max_buffered_bytes_per_connection: default_i2cp_max_buffered_bytes_per_connection(),
            max_pending_writes_per_connection: default_i2cp_max_pending_writes_per_connection(),
            protocol_byte_timeout_ms: default_i2cp_protocol_byte_timeout_ms(),
            command_timeout_ms: default_i2cp_command_timeout_ms(),
            shutdown_timeout_ms: default_i2cp_shutdown_timeout_ms(),
        }
    }
}

/// Operator password for the Plan 287 I2PControl listener.
///
/// The inner value never appears in `Debug` output, logs, or snapshots:
/// formatting emits a fixed redaction marker. Comparison uses the bounded
/// constant-time helper in `crate::i2pcontrol`.
#[derive(Clone, Default, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct I2pControlPassword(String);

impl I2pControlPassword {
    /// Borrows the password bytes for bounded constant-time comparison.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Password length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no password is configured.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for I2pControlPassword {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("I2pControlPassword([redacted])")
    }
}

/// Raw Plan 287 I2PControl service configuration.
///
/// The listener is disabled by default and loopback-only by default.
/// `password` has no insecure factory default: enabling the service with an
/// empty or missing password fails semantic validation before bind.
/// Explicit TLS material (`certificate` + `private_key`) is optional for
/// loopback binds (managed ephemeral self-signed TLS is used instead) and
/// mandatory for any non-loopback bind. There is no plaintext fallback.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawI2pControlConfig {
    #[serde(default = "default_i2pcontrol_enabled")]
    enabled: bool,
    #[serde(default = "default_i2pcontrol_bind_address")]
    bind_address: String,
    #[serde(default = "default_i2pcontrol_port")]
    port: u16,
    #[serde(default)]
    password: I2pControlPassword,
    #[serde(default)]
    certificate: String,
    #[serde(default)]
    private_key: String,
    #[serde(default = "default_i2pcontrol_max_connections")]
    max_connections: u32,
    #[serde(default = "default_i2pcontrol_max_body_bytes")]
    max_body_bytes: usize,
    #[serde(default = "default_i2pcontrol_request_deadline_ms")]
    request_deadline_ms: u64,
    #[serde(default = "default_i2pcontrol_shutdown_timeout_ms")]
    shutdown_timeout_ms: u64,
}

impl Default for RawI2pControlConfig {
    fn default() -> Self {
        Self {
            enabled: default_i2pcontrol_enabled(),
            bind_address: default_i2pcontrol_bind_address(),
            port: default_i2pcontrol_port(),
            password: I2pControlPassword::default(),
            certificate: String::new(),
            private_key: String::new(),
            max_connections: default_i2pcontrol_max_connections(),
            max_body_bytes: default_i2pcontrol_max_body_bytes(),
            request_deadline_ms: default_i2pcontrol_request_deadline_ms(),
            shutdown_timeout_ms: default_i2pcontrol_shutdown_timeout_ms(),
        }
    }
}

/// Raw router-console block.
///
/// `deny_unknown_fields` keeps a misspelled console key a configuration
/// error instead of a silently ignored setting.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConsoleConfig {
    #[serde(default = "default_console_enabled")]
    enabled: bool,
    #[serde(default = "default_console_bind_address")]
    bind_address: String,
    #[serde(default = "default_console_port")]
    port: u16,
    #[serde(default = "default_console_theme")]
    theme: String,
    #[serde(default = "default_console_max_connections")]
    max_connections: u32,
    #[serde(default = "default_console_auth")]
    auth: bool,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    password_hash: Option<String>,
    #[serde(default = "default_console_session_idle_secs")]
    session_idle_secs: u64,
    #[serde(default = "default_console_session_absolute_secs")]
    session_absolute_secs: u64,
    #[serde(default = "default_console_max_sessions")]
    max_sessions: usize,
    #[serde(default = "default_console_login_max_failures")]
    login_max_failures: u32,
    #[serde(default = "default_console_login_window_secs")]
    login_window_secs: u64,
}

/// A hand-written `Debug` so the credential never reaches a log.
///
/// The derived output would contain the plaintext password, and this struct
/// is nested inside the `Debug` of the whole raw configuration.
impl std::fmt::Debug for RawConsoleConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RawConsoleConfig")
            .field("enabled", &self.enabled)
            .field("bind_address", &self.bind_address)
            .field("port", &self.port)
            .field("theme", &self.theme)
            .field("max_connections", &self.max_connections)
            .field("auth", &self.auth)
            .field("password", &"<redacted>")
            .field("password_hash", &"<redacted>")
            .field("session_idle_secs", &self.session_idle_secs)
            .field("session_absolute_secs", &self.session_absolute_secs)
            .field("max_sessions", &self.max_sessions)
            .field("login_max_failures", &self.login_max_failures)
            .field("login_window_secs", &self.login_window_secs)
            .finish()
    }
}

impl Default for RawConsoleConfig {
    fn default() -> Self {
        Self {
            enabled: default_console_enabled(),
            bind_address: default_console_bind_address(),
            port: default_console_port(),
            theme: default_console_theme(),
            max_connections: default_console_max_connections(),
            auth: default_console_auth(),
            password: None,
            password_hash: None,
            session_idle_secs: default_console_session_idle_secs(),
            session_absolute_secs: default_console_session_absolute_secs(),
            max_sessions: default_console_max_sessions(),
            login_max_failures: default_console_login_max_failures(),
            login_window_secs: default_console_login_window_secs(),
        }
    }
}

/// Raw managed-application runtime block (Plan 369).
///
/// This block carries **only** an activation switch. Plan 369 §4 and invariant 9
/// make the manager executable a distribution-owned sibling of the daemon, so
/// there is deliberately no `command`, `path`, `binary`, or `args` field here:
/// a configurable executable path would let a configuration file choose which
/// process the router hands a trusted capability transport to, which is exactly
/// the authority handoff the inherited transport is supposed to prove.
///
/// `deny_unknown_fields` means a misspelled or out-of-policy key such as
/// `manager_path` is a hard configuration error rather than a silently ignored
/// setting, so an operator who believes they redirected the manager learns
/// that they did not.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAppRuntimeConfig {
    #[serde(default = "default_app_runtime_enabled")]
    enabled: bool,
}

impl Default for RawAppRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: default_app_runtime_enabled(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSamConfig {
    #[serde(default = "default_sam_enabled")]
    enabled: bool,
    #[serde(default = "default_sam_bind_address")]
    bind_address: String,
    #[serde(default = "default_sam_port")]
    port: u16,
    #[serde(default = "default_sam_max_clients")]
    max_clients: u16,
    #[serde(default = "default_sam_max_sessions")]
    max_sessions: u16,
    #[serde(default = "default_sam_stream_sockets_per_session")]
    max_stream_sockets_per_session: u16,
    #[serde(default = "default_sam_pending_accepts_per_session")]
    max_pending_accepts_per_session: u16,
    #[serde(default = "default_sam_buffered_bytes_per_stream_direction")]
    max_buffered_bytes_per_stream_direction: usize,
    #[serde(default = "default_sam_hello_timeout_ms")]
    hello_timeout_ms: u64,
    #[serde(default = "default_sam_command_timeout_ms")]
    command_timeout_ms: u64,
    #[serde(default = "default_sam_shutdown_timeout_ms")]
    shutdown_timeout_ms: u64,
}

impl Default for RawSamConfig {
    fn default() -> Self {
        Self {
            enabled: default_sam_enabled(),
            bind_address: default_sam_bind_address(),
            port: default_sam_port(),
            max_clients: default_sam_max_clients(),
            max_sessions: default_sam_max_sessions(),
            max_stream_sockets_per_session: default_sam_stream_sockets_per_session(),
            max_pending_accepts_per_session: default_sam_pending_accepts_per_session(),
            max_buffered_bytes_per_stream_direction:
                default_sam_buffered_bytes_per_stream_direction(),
            hello_timeout_ms: default_sam_hello_timeout_ms(),
            command_timeout_ms: default_sam_command_timeout_ms(),
            shutdown_timeout_ms: default_sam_shutdown_timeout_ms(),
        }
    }
}

/// Raw Plan 158 SSU2 runtime configuration.
///
/// The surface is intentionally narrow: protocol-tuning constants (token
/// quotas, reassembly quotas, resend schedules) stay pinned in
/// `i2pr-transport-ssu2::constants` and are not exposed as knobs.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSsu2Config {
    #[serde(default = "default_ssu2_enabled")]
    enabled: bool,
    #[serde(default = "default_ssu2_bind_ipv4")]
    bind_ipv4: String,
    #[serde(default = "default_ssu2_bind_ipv6")]
    bind_ipv6: String,
    #[serde(default = "default_ssu2_port")]
    port: u16,
    #[serde(default = "default_ssu2_advertise")]
    advertise: bool,
    #[serde(default = "default_ssu2_introducer_service")]
    introducer_service: bool,
    #[serde(default = "default_ssu2_max_pending_handshakes")]
    max_pending_handshakes: usize,
    #[serde(default = "default_ssu2_max_active_sessions")]
    max_active_sessions: usize,
    #[serde(default = "default_ssu2_max_pending_per_ip")]
    max_pending_per_ip: usize,
    #[serde(default = "default_ssu2_max_pending_per_subnet")]
    max_pending_per_subnet: usize,
    #[serde(default = "default_ssu2_max_datagram_queue_items")]
    max_datagram_queue_items: usize,
    #[serde(default = "default_ssu2_max_datagram_queue_bytes")]
    max_datagram_queue_bytes: u64,
    #[serde(default = "default_ssu2_max_inbound_i2np_queue")]
    max_inbound_i2np_queue: usize,
    #[serde(default = "default_ssu2_handshake_timeout_ms")]
    handshake_timeout_ms: u64,
    #[serde(default = "default_ssu2_idle_timeout_ms")]
    idle_timeout_ms: u64,
    #[serde(default = "default_ssu2_scheduler_poll_max_ms")]
    scheduler_poll_max_ms: u64,
}

impl Default for RawSsu2Config {
    fn default() -> Self {
        Self {
            enabled: default_ssu2_enabled(),
            bind_ipv4: default_ssu2_bind_ipv4(),
            bind_ipv6: default_ssu2_bind_ipv6(),
            port: default_ssu2_port(),
            advertise: default_ssu2_advertise(),
            introducer_service: default_ssu2_introducer_service(),
            max_pending_handshakes: default_ssu2_max_pending_handshakes(),
            max_active_sessions: default_ssu2_max_active_sessions(),
            max_pending_per_ip: default_ssu2_max_pending_per_ip(),
            max_pending_per_subnet: default_ssu2_max_pending_per_subnet(),
            max_datagram_queue_items: default_ssu2_max_datagram_queue_items(),
            max_datagram_queue_bytes: default_ssu2_max_datagram_queue_bytes(),
            max_inbound_i2np_queue: default_ssu2_max_inbound_i2np_queue(),
            handshake_timeout_ms: default_ssu2_handshake_timeout_ms(),
            idle_timeout_ms: default_ssu2_idle_timeout_ms(),
            scheduler_poll_max_ms: default_ssu2_scheduler_poll_max_ms(),
        }
    }
}

/// Raw service-tunnel configuration.
///
/// Disabled by default and loopback-only. Configuration parsing
/// validates enabled specifications; daemon graph construction
/// rejects them until the normal-daemon Destination-group provider
/// owns their listeners and router traffic.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServiceTunnelsConfig {
    #[serde(default = "default_service_tunnels_enabled")]
    enabled: bool,
    #[serde(default = "default_service_tunnels_max_active_connections")]
    max_active_connections: usize,
    #[serde(default = "default_service_tunnels_max_buffered_bytes_per_direction")]
    max_buffered_bytes_per_direction: usize,
    #[serde(default = "default_service_tunnels_connect_timeout_ms")]
    connect_timeout_ms: u64,
    #[serde(default = "default_service_tunnels_read_timeout_ms")]
    read_timeout_ms: u64,
    #[serde(default = "default_service_tunnels_write_timeout_ms")]
    write_timeout_ms: u64,
    #[serde(default = "default_service_tunnels_shutdown_timeout_ms")]
    shutdown_timeout_ms: u64,
    #[serde(default)]
    tunnel: Vec<RawServiceTunnelEntry>,
    #[serde(default)]
    alias: Vec<RawServiceTunnelAlias>,
    /// Plan 297: explicit local TLS identity/trust policy for
    /// server `use_ssl` dials. Absent means no TLS policy: `use_ssl`
    /// tunnels fail before connecting.
    #[serde(default)]
    tls: Option<RawServiceTlsConfig>,
}

impl Default for RawServiceTunnelsConfig {
    fn default() -> Self {
        Self {
            enabled: default_service_tunnels_enabled(),
            max_active_connections: default_service_tunnels_max_active_connections(),
            max_buffered_bytes_per_direction:
                default_service_tunnels_max_buffered_bytes_per_direction(),
            connect_timeout_ms: default_service_tunnels_connect_timeout_ms(),
            read_timeout_ms: default_service_tunnels_read_timeout_ms(),
            write_timeout_ms: default_service_tunnels_write_timeout_ms(),
            shutdown_timeout_ms: default_service_tunnels_shutdown_timeout_ms(),
            tunnel: Vec::new(),
            alias: Vec::new(),
            tls: None,
        }
    }
}

/// Raw `[service_tunnels.tls]` explicit TLS identity/trust policy
/// (Plan 297). The identity pair is optional (used as the client
/// certificate only when the loopback target requests client
/// authentication); verification needs pinned end-entity
/// certificates, explicit trust roots, or both — never ambient
/// system roots, and there is no unauthenticated opt-in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServiceTlsConfig {
    /// PEM certificate for the endpoint identity (requires
    /// `private_key_path`).
    #[serde(default)]
    certificate_path: Option<std::path::PathBuf>,
    /// PEM private key for the endpoint identity (requires
    /// `certificate_path`).
    #[serde(default)]
    private_key_path: Option<std::path::PathBuf>,
    /// PEM bundle of pinned end-entity certificates, used as
    /// trust anchors.
    #[serde(default)]
    pinned_certificates_path: Option<std::path::PathBuf>,
    /// PEM bundle of explicit trust roots.
    #[serde(default)]
    trust_roots_path: Option<std::path::PathBuf>,
}

/// One raw `[[service_tunnels.tunnel]]` entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServiceTunnelEntry {
    id: String,
    kind: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    listener: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    destination: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    inbound_port: Option<u16>,
    #[serde(default)]
    max_connections: Option<usize>,
    #[serde(default)]
    max_buffered_bytes_per_direction: Option<usize>,
    /// Plan 291: loopback UDP media endpoint for `streamr-server`
    /// (media source) and `streamr-client` (media target)
    /// (`ip:port`, explicit; no silent default for where media
    /// enters or exits).
    #[serde(default)]
    local_udp: Option<String>,
}

/// One raw `[[service_tunnels.alias]]` static alias mapping.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServiceTunnelAlias {
    name: String,
    target: String,
}

/// Raw Plan 294 `[addressbook]` configuration.
///
/// Disabled by default. While disabled the subsystem never touches the
/// filesystem; `state_dir` only resolves to a path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAddressBookConfig {
    #[serde(default = "default_addressbook_enabled")]
    enabled: bool,
    #[serde(default)]
    state_dir: Option<String>,
}

impl Default for RawAddressBookConfig {
    fn default() -> Self {
        Self {
            enabled: default_addressbook_enabled(),
            state_dir: None,
        }
    }
}

/// Raw signed-router-news source configuration. It is disabled unless the
/// operator supplies a source, a pinned signer certificate, and proxy.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNewsConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    source_url: String,
    #[serde(default)]
    signer_id: String,
    #[serde(default)]
    certificate_path: String,
    #[serde(default = "default_news_proxy_host")]
    proxy_host: String,
    #[serde(default = "default_news_proxy_port")]
    proxy_port: u16,
    #[serde(default = "default_news_max_su3_bytes")]
    max_su3_bytes: u64,
    #[serde(default = "default_news_refresh_interval_secs")]
    refresh_interval_secs: u64,
}

impl Default for RawNewsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            source_url: String::new(),
            signer_id: String::new(),
            certificate_path: String::new(),
            proxy_host: default_news_proxy_host(),
            proxy_port: default_news_proxy_port(),
            max_su3_bytes: default_news_max_su3_bytes(),
            refresh_interval_secs: default_news_refresh_interval_secs(),
        }
    }
}

/// Raw Plan 279 normal floodfill opt-in.
///
/// The surface is a single intent boolean. It never carries advertisement
/// authority: `enabled = true` only asks the daemon to evaluate normal-path
/// eligibility, and `caps=f` is published only when every eligibility signal
/// holds. Default is false; the subsystem stays off unless the operator
/// explicitly opts in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFloodfillConfig {
    #[serde(default = "default_floodfill_enabled")]
    enabled: bool,
}

impl Default for RawFloodfillConfig {
    fn default() -> Self {
        Self {
            enabled: default_floodfill_enabled(),
        }
    }
}

fn default_profile() -> String {
    String::from("balanced")
}

fn default_filter() -> String {
    String::from("info")
}

fn default_log_format() -> String {
    String::from("text")
}

const fn default_max_tasks() -> u64 {
    DEFAULT_MAX_TASKS
}

const fn default_max_buffered_bytes() -> u64 {
    DEFAULT_MAX_BUFFERED_BYTES
}

fn default_bind_address() -> String {
    String::from("0.0.0.0")
}

const fn default_listen_port() -> u16 {
    9150
}

const fn default_network_id() -> u16 {
    2
}

const fn default_ntcp2_enabled() -> bool {
    false
}

const fn default_ntcp2_connect_timeout_ms() -> u64 {
    5_000
}

const fn default_ntcp2_handshake_timeout_ms() -> u64 {
    30_000
}

const fn default_ntcp2_read_idle_timeout_ms() -> u64 {
    120_000
}

const fn default_ntcp2_write_timeout_ms() -> u64 {
    30_000
}

const fn default_ntcp2_queue_wait_timeout_ms() -> u64 {
    5_000
}

const fn default_ntcp2_drain_timeout_ms() -> u64 {
    5_000
}

const fn default_ntcp2_max_active_links() -> usize {
    128
}

const fn default_ntcp2_max_replay_entries() -> usize {
    256
}

const fn default_ntcp2_ipv4_prefix() -> u8 {
    24
}

const fn default_ntcp2_ipv6_prefix() -> u8 {
    64
}

const fn default_netdb_enabled() -> bool {
    true
}

const fn default_netdb_max_records() -> u64 {
    4_096
}

const fn default_netdb_max_encoded_bytes() -> u64 {
    4 * 1024 * 1024
}

const fn default_netdb_min_router_infos() -> u64 {
    50
}

const fn default_netdb_min_floodfill_advertisers() -> u64 {
    5
}

const fn default_reseed_enabled() -> bool {
    false
}

const fn default_reseed_max_sources() -> usize {
    4
}

const fn default_reseed_max_su3_bytes() -> u64 {
    8 * 1024 * 1024
}

const fn default_news_proxy_port() -> u16 {
    4444
}

const fn default_news_max_su3_bytes() -> u64 {
    8 * 1024 * 1024
}

const fn default_news_refresh_interval_secs() -> u64 {
    6 * 60 * 60
}

fn default_news_proxy_host() -> String {
    String::from("127.0.0.1")
}

const fn default_console_enabled() -> bool {
    false
}

/// The managed-application runtime is off unless an operator turns it on.
///
/// This is Plan 369 §5's activation switch. The default is `false` for the same
/// reason SAM, I2CP, I2PControl, service tunnels, and the console are off by
/// default: the feature is experimental, unsandboxed (Plan 369 §2 refuses
/// `Secured` because no qualified backend exists), and must not be reachable
/// without an explicit operator decision.
const fn default_app_runtime_enabled() -> bool {
    false
}

fn default_console_bind_address() -> String {
    "127.0.0.1".to_string()
}

fn default_console_port() -> u16 {
    7070
}

fn default_console_theme() -> String {
    i2pr_console::theme::DEFAULT_THEME_NAME.to_string()
}

fn default_console_max_connections() -> u32 {
    16
}

fn default_console_auth() -> bool {
    false
}

fn default_console_session_idle_secs() -> u64 {
    900
}

fn default_console_session_absolute_secs() -> u64 {
    28_800
}

fn default_console_max_sessions() -> usize {
    32
}

fn default_console_login_max_failures() -> u32 {
    5
}

fn default_console_login_window_secs() -> u64 {
    300
}

fn default_sam_enabled() -> bool {
    false
}

fn default_sam_bind_address() -> String {
    String::from("127.0.0.1")
}

const fn default_sam_port() -> u16 {
    7656
}

const fn default_sam_max_clients() -> u16 {
    16
}

const fn default_sam_max_sessions() -> u16 {
    16
}

const fn default_sam_stream_sockets_per_session() -> u16 {
    16
}

const fn default_sam_pending_accepts_per_session() -> u16 {
    16
}

const fn default_sam_buffered_bytes_per_stream_direction() -> usize {
    64 * 1024
}

const fn default_sam_hello_timeout_ms() -> u64 {
    10_000
}

const fn default_sam_command_timeout_ms() -> u64 {
    60_000
}

const fn default_sam_shutdown_timeout_ms() -> u64 {
    5_000
}

// --- Plan 167 I2CP defaults: disabled, loopback-only. ---

fn default_i2cp_enabled() -> bool {
    false
}

fn default_i2cp_bind_address() -> String {
    String::from("127.0.0.1")
}

const fn default_i2cp_port() -> u16 {
    7654
}

const fn default_i2cp_max_clients() -> u16 {
    16
}

const fn default_i2cp_max_sessions_per_connection() -> u16 {
    1
}

const fn default_i2cp_max_sessions_router() -> u16 {
    16
}

const fn default_i2cp_max_buffered_bytes_per_connection() -> usize {
    64 * 1024
}

const fn default_i2cp_max_pending_writes_per_connection() -> u32 {
    64
}

const fn default_i2cp_protocol_byte_timeout_ms() -> u64 {
    10_000
}

const fn default_i2cp_command_timeout_ms() -> u64 {
    60_000
}

const fn default_i2cp_shutdown_timeout_ms() -> u64 {
    5_000
}

const MAX_I2CP_CLIENTS: u16 = 256;
const MAX_I2CP_SESSIONS_PER_CONNECTION: u16 = 16;
const MAX_I2CP_SESSIONS_ROUTER: u16 = 256;
const MAX_I2CP_BUFFERED_BYTES_PER_CONNECTION: usize = 1024 * 1024;
const MAX_I2CP_PENDING_WRITES_PER_CONNECTION: u32 = 4096;
const MIN_I2CP_PROTOCOL_BYTE_TIMEOUT_MS: u64 = 1_000;
const MAX_I2CP_PROTOCOL_BYTE_TIMEOUT_MS: u64 = 60_000;
const MIN_I2CP_COMMAND_TIMEOUT_MS: u64 = 5_000;
const MAX_I2CP_COMMAND_TIMEOUT_MS: u64 = 3_600_000;
const MIN_I2CP_SHUTDOWN_TIMEOUT_MS: u64 = 1_000;
const MAX_I2CP_SHUTDOWN_TIMEOUT_MS: u64 = 30_000;

// --- Plan 287 I2PControl defaults: disabled, loopback-only, no password. ---

fn default_i2pcontrol_enabled() -> bool {
    false
}

fn default_i2pcontrol_bind_address() -> String {
    String::from("127.0.0.1")
}

const fn default_i2pcontrol_port() -> u16 {
    7650
}

const fn default_i2pcontrol_max_connections() -> u32 {
    64
}

const fn default_i2pcontrol_max_body_bytes() -> usize {
    1_048_576
}

const fn default_i2pcontrol_request_deadline_ms() -> u64 {
    5_000
}

const fn default_i2pcontrol_shutdown_timeout_ms() -> u64 {
    2_000
}

/// Hard compile-time maxima bounding every `[i2pcontrol]` resource knob.
const MAX_I2PCONTROL_CONNECTIONS: u32 = 256;
const MAX_I2PCONTROL_BODY_BYTES: usize = 1_048_576;
const MIN_I2PCONTROL_REQUEST_DEADLINE_MS: u64 = 1_000;
const MAX_I2PCONTROL_REQUEST_DEADLINE_MS: u64 = 60_000;
const MIN_I2PCONTROL_SHUTDOWN_TIMEOUT_MS: u64 = 500;
const MAX_I2PCONTROL_SHUTDOWN_TIMEOUT_MS: u64 = 30_000;
const MAX_I2PCONTROL_PASSWORD_BYTES: usize = 1_024;

// --- Plan 158 SSU2 defaults: disabled, loopback-only, non-advertised. ---

fn default_ssu2_enabled() -> bool {
    false
}

fn default_ssu2_bind_ipv4() -> String {
    String::from("127.0.0.1")
}

fn default_ssu2_bind_ipv6() -> String {
    String::new()
}

const fn default_ssu2_port() -> u16 {
    0
}

const fn default_ssu2_advertise() -> bool {
    false
}

const fn default_ssu2_introducer_service() -> bool {
    false
}

const fn default_ssu2_max_pending_handshakes() -> usize {
    64
}

const fn default_ssu2_max_active_sessions() -> usize {
    64
}

const fn default_ssu2_max_pending_per_ip() -> usize {
    4
}

const fn default_ssu2_max_pending_per_subnet() -> usize {
    16
}

const fn default_ssu2_max_datagram_queue_items() -> usize {
    256
}

const fn default_ssu2_max_datagram_queue_bytes() -> u64 {
    1024 * 1024
}

const fn default_ssu2_max_inbound_i2np_queue() -> usize {
    64
}

const fn default_ssu2_handshake_timeout_ms() -> u64 {
    20_000
}

const fn default_ssu2_idle_timeout_ms() -> u64 {
    300_000
}

const fn default_ssu2_scheduler_poll_max_ms() -> u64 {
    200
}

const MAX_SSU2_PENDING_HANDSHAKES: usize = 1024;
const MAX_SSU2_ACTIVE_SESSIONS: usize = 1024;
const MAX_SSU2_PENDING_PER_IP: usize = 64;
const MAX_SSU2_PENDING_PER_SUBNET: usize = 256;
const MAX_SSU2_DATAGRAM_QUEUE_ITEMS: usize = 4096;
const MAX_SSU2_DATAGRAM_QUEUE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SSU2_INBOUND_I2NP_QUEUE: usize = 1024;
const MIN_SSU2_HANDSHAKE_TIMEOUT_MS: u64 = 5_000;
const MAX_SSU2_HANDSHAKE_TIMEOUT_MS: u64 = 60_000;
const MIN_SSU2_SCHEDULER_POLL_MAX_MS: u64 = 10;
const MAX_SSU2_SCHEDULER_POLL_MAX_MS: u64 = 1_000;
const MIN_SSU2_SERVICE_PORT: u16 = 1024;

// --- Plan 174 service-tunnel defaults: disabled, loopback-only, no listener. ---

const fn default_service_tunnels_enabled() -> bool {
    false
}

const fn default_addressbook_enabled() -> bool {
    false
}

/// Plan 279 normal floodfill opt-in default: off. The operator must
/// explicitly set `enabled = true`, and even then the daemon only
/// evaluates eligibility — it never forces the Active role.
const fn default_floodfill_enabled() -> bool {
    false
}

const fn default_service_tunnels_max_active_connections() -> usize {
    128
}

const fn default_service_tunnels_max_buffered_bytes_per_direction() -> usize {
    65_536
}

const fn default_service_tunnels_connect_timeout_ms() -> u64 {
    10_000
}

const fn default_service_tunnels_read_timeout_ms() -> u64 {
    60_000
}

const fn default_service_tunnels_write_timeout_ms() -> u64 {
    60_000
}

const fn default_service_tunnels_shutdown_timeout_ms() -> u64 {
    5_000
}

/// Normalized Plan 167 I2CP listener configuration.
///
/// M9 I2CP listens on the conventional `127.0.0.1:7654` loopback port.
/// `enabled = false` keeps the production daemon silent; integration
/// tests opt in by setting the flag explicitly. Non-loopback bind
/// addresses are rejected during semantic validation, the same as the
/// SAM listener. TLS and credentialed authentication are explicitly
/// deferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct I2cpConfig {
    /// Whether the I2CP listener is enabled.
    pub enabled: bool,
    /// Loopback bind IP. Non-loopback is rejected.
    pub bind_address: IpAddr,
    /// Bind port. `0` selects an ephemeral port (integration tests).
    pub port: u16,
    /// Maximum concurrent TCP clients accepted by the loopback listener.
    pub max_clients: u16,
    /// Maximum active sessions owned by a single TCP connection.
    pub max_sessions_per_connection: u16,
    /// Maximum active sessions across every I2CP connection.
    pub max_sessions_router: u16,
    /// Aggregate buffered-byte ceiling per connection read buffer.
    pub max_buffered_bytes_per_connection: usize,
    /// Maximum queued outbound frames per connection.
    pub max_pending_writes_per_connection: u32,
    /// Deadline for the opening protocol byte.
    pub protocol_byte_timeout: Duration,
    /// Per-command/idle deadline.
    pub command_timeout: Duration,
    /// Graceful shutdown deadline.
    pub shutdown_timeout: Duration,
}

impl I2cpConfig {
    /// Returns the loopback bind address.
    pub const fn bind_socket(&self) -> SocketAddr {
        SocketAddr::new(self.bind_address, self.port)
    }

    /// Returns the loopback-only integration-test profile with
    /// `Duration::MAX` deadlines that disable timer races under
    /// `tokio::time::test-util`.
    pub const fn loopback_test_profile(
        max_clients: u16,
        max_sessions_router: u16,
        max_buffered_bytes_per_connection: usize,
        max_pending_writes_per_connection: u32,
    ) -> Self {
        Self {
            enabled: true,
            bind_address: IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            port: 0,
            max_clients,
            max_sessions_per_connection: 1,
            max_sessions_router,
            max_buffered_bytes_per_connection,
            max_pending_writes_per_connection,
            protocol_byte_timeout: Duration::MAX,
            command_timeout: Duration::MAX,
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

/// Normalized Plan 287 I2PControl HTTPS/JSON-RPC listener configuration.
///
/// Disabled by default. The loopback default bind is `127.0.0.1:7650`
/// with `::1` supported explicitly. Enabling with an empty or missing
/// password fails validation before bind. Managed ephemeral self-signed
/// TLS covers loopback identities only; any non-loopback bind requires a
/// complete explicit certificate/private-key pair and fails validation
/// before listener bind or managed-certificate side effects. There is no
/// plaintext fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct I2pControlConfig {
    /// Whether the I2PControl listener is enabled.
    pub enabled: bool,
    /// Bind IP. Non-loopback requires explicit TLS material.
    pub bind_address: IpAddr,
    /// Bind port. `0` selects an ephemeral port (integration tests).
    pub port: u16,
    /// Operator password for `Authenticate` (redacted in `Debug`; zeroized in use).
    pub password: I2pControlPassword,
    /// Optional explicit certificate PEM path (requires `private_key`).
    pub certificate: Option<PathBuf>,
    /// Optional explicit private-key PEM path (requires `certificate`).
    pub private_key: Option<PathBuf>,
    /// Maximum concurrent accepted TLS connections.
    pub max_connections: u32,
    /// Maximum HTTP request body in bytes (≤ 1 MiB hard cap).
    pub max_body_bytes: usize,
    /// Per-request deadline.
    pub request_deadline: Duration,
    /// Graceful shutdown deadline.
    pub shutdown_timeout: Duration,
}

impl I2pControlConfig {
    /// Returns the configured bind address.
    pub const fn bind_socket(&self) -> SocketAddr {
        SocketAddr::new(self.bind_address, self.port)
    }

    /// Whether the bind address is a loopback identity eligible for
    /// managed self-signed TLS.
    pub fn is_loopback_bind(&self) -> bool {
        self.bind_address.is_loopback()
    }

    /// Whether explicit operator-owned TLS material is configured.
    pub fn has_explicit_tls(&self) -> bool {
        self.certificate.is_some() && self.private_key.is_some()
    }
}

/// Normalized router policy placeholder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterProfile {
    /// The only profile with defined bootstrap semantics.
    Balanced,
}

/// Normalized logging format placeholder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    /// Human-readable line-oriented logging.
    Text,
}

/// Normalized router configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterConfig {
    /// Data directory path, validated but never created by this milestone.
    pub data_dir: PathBuf,
    /// Selected future router policy profile.
    pub profile: RouterProfile,
}

/// Normalized logging configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoggingConfig {
    /// Tracing filter expression retained for future runtime initialization.
    pub filter: String,
    /// Selected output format.
    pub format: LogFormat,
}

/// Normalized initial resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitsConfig {
    /// Maximum supervised tasks.
    pub max_tasks: u64,
    /// Maximum buffered bytes.
    pub max_buffered_bytes: u64,
}

/// Normalized network configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkConfig {
    /// IP address to bind listeners to.
    pub bind_address: IpAddr,
    /// TCP port for NTCP2 listeners.
    pub listen_port: u16,
    /// I2P network identifier.
    pub network_id: u16,
}

impl NetworkConfig {
    /// Returns the socket address for binding.
    pub fn listen_socket(&self) -> SocketAddr {
        SocketAddr::new(self.bind_address, self.listen_port)
    }
}

/// Normalized NTCP2 transport configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ntcp2Config {
    /// Whether NTCP2 transport is enabled.
    pub enabled: bool,
    /// TCP connect timeout.
    pub connect_timeout: Duration,
    /// Total handshake timeout.
    pub handshake_timeout: Duration,
    /// Read-idle timeout.
    pub read_idle_timeout: Duration,
    /// Write timeout.
    pub write_timeout: Duration,
    /// Queue admission timeout.
    pub queue_wait_timeout: Duration,
    /// Graceful duplicate/link drain timeout.
    pub drain_timeout: Duration,
    /// Maximum active links.
    pub max_active_links: usize,
    /// Maximum replay entries.
    pub max_replay_entries: usize,
    /// IPv4 prefix width for subnet accounting.
    pub ipv4_prefix: u8,
    /// IPv6 prefix width for subnet accounting.
    pub ipv6_prefix: u8,
}

/// Normalized transport configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportConfig {
    /// NTCP2 transport settings.
    pub ntcp2: Ntcp2Config,
}

/// Normalized NetDB configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetDbConfig {
    /// Whether the local NetDB and cache loader are active.
    pub enabled: bool,
    /// Maximum records retained by the in-memory store.
    pub max_records: usize,
    /// Maximum aggregate encoded bytes retained by the in-memory store.
    pub max_encoded_bytes: usize,
    /// Minimum record count required for the cache-sufficient state.
    pub min_router_infos: usize,
    /// Minimum floodfill advertiser count required for the
    /// `ready-for-network-integration` state.
    pub min_floodfill_advertisers: usize,
}

/// Normalized reseed configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReseedConfig {
    /// Whether reseed may run during bootstrap.
    pub enabled: bool,
    /// Maximum number of configured reseed sources.
    pub max_sources: usize,
    /// Maximum SU3 bundle bytes any single acquisition may consume.
    pub max_su3_bytes: usize,
    /// Configured trust-source list (operator-supplied).
    pub sources: Vec<ReseedSourceConfig>,
}

/// One trust-store entry for a Plan 104 SU3 reseed signer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReseedSourceConfig {
    /// Human-readable signer identifier carried in the SU3 header.
    pub signer_id: String,
    /// Filesystem path to the matching DER X.509 certificate.
    pub certificate_path: PathBuf,
}

/// Validated, opt-in signed NEWS feed configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewsConfig {
    pub enabled: bool,
    pub source_url: Option<String>,
    pub signer_id: Option<String>,
    pub certificate_path: Option<PathBuf>,
    pub proxy_host: IpAddr,
    pub proxy_port: u16,
    pub max_su3_bytes: usize,
    pub refresh_interval: Duration,
}

/// Normalized managed-application runtime settings (Plan 369).
///
/// Experimental and disabled by default. Enabling this starts one supervised,
/// restartable `i2pr-appd` child over an anonymous inherited transport. It does
/// **not** enable a listener, does not grant any application capability, and
/// does not make managed-app v1 a supported protocol — Plan 369 WP2's manager
/// refuses every request because there is no launch-authority owner yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppRuntimeConfig {
    /// Whether the supervised manager process is started at all.
    pub enabled: bool,
}

/// Normalizes the managed-application runtime block.
///
/// The block has no runtime ceilings to check, because it owns no listener, no
/// port, and no per-connection budget: its only bounded resources are the
/// supervisor's restart policy and the manager's own frame limits. Validation
/// therefore reduces to the activation switch itself.
fn normalize_app_runtime(raw: &RawAppRuntimeConfig) -> AppRuntimeConfig {
    AppRuntimeConfig {
        enabled: raw.enabled,
    }
}

/// Normalized router-console service configuration (Plan 356).
///
/// The console is experimental, loopback-only, and disabled by default.
/// There is no authentication field in this struct by design: Plan 357
/// adds the browser security policy, and it must not be reachable through
/// configuration alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsoleConfig {
    /// Whether the console listener is started at all.
    pub enabled: bool,
    /// Bind address; always loopback after normalization.
    pub bind_address: IpAddr,
    /// TCP port. `0` requests an OS-assigned ephemeral port, which the
    /// loopback tests rely on.
    pub port: u16,
    /// Bundled theme identifier, validated at configuration time.
    pub theme: String,
    /// Maximum concurrent browser connections.
    pub max_connections: u32,
    /// Whether a console password is required.
    pub auth: bool,
    /// Argon2id verifier for the console password.
    ///
    /// Redacted in `Debug`: a stored hash is an offline attack verifier.
    pub password_hash: Option<ConsolePasswordHash>,
    /// Maximum concurrent console sessions.
    pub max_sessions: usize,
    /// Session idle expiry in seconds.
    pub session_idle_secs: u64,
    /// Session absolute lifetime in seconds.
    pub session_absolute_secs: u64,
    /// Failed logins permitted inside the window.
    pub login_max_failures: u32,
    /// Login throttle window in seconds.
    pub login_window_secs: u64,
}

/// An Argon2id console password verifier.
///
/// Wrapped so it cannot be printed: a stored hash is an offline attack
/// verifier, and a configuration dump is a normal thing to paste into a
/// bug report.
#[derive(Clone, Eq, PartialEq)]
pub struct ConsolePasswordHash(String);

impl ConsolePasswordHash {
    /// Wraps a PHC hash string.
    pub fn new(hash: String) -> Self {
        Self(hash)
    }

    /// Returns the hash text for the verifier owner.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ConsolePasswordHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ConsolePasswordHash(<redacted>)")
    }
}

impl ConsoleConfig {
    /// Returns the socket address the listener should bind.
    pub fn bind_socket(&self) -> SocketAddr {
        SocketAddr::new(self.bind_address, self.port)
    }
}

/// Normalized SAM v3.1 service configuration (Plan 137).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamConfig {
    /// Whether the loopback SAM listener is enabled.
    pub enabled: bool,
    /// Loopback bind IP. Non-loopback values are rejected.
    pub bind_address: IpAddr,
    /// Bind port. `0` selects an ephemeral port (used by integration tests).
    pub port: u16,
    /// Validated service limits.
    pub limits: SamLimits,
}

impl SamConfig {
    /// Returns the loopback bind address.
    pub const fn bind_socket(&self) -> SocketAddr {
        SocketAddr::new(self.bind_address, self.port)
    }
}

/// Re-export of the Plan 137 service-limits type.
pub use i2pr_api::sam::limits::SamLimits;

/// Normalized Plan 184 SSU2 runtime configuration.
///
/// The daemon parses and validates this surface and starts the
/// daemon-owned SSU2 service only under the strict Plan 184
/// controlled profile: loopback bind, `advertise = false`,
/// `introducer_service = false`, no wildcard/non-loopback exposure.
/// Broader publication/exposure remains rejected fail-closed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ssu2Config {
    /// Whether the SSU2 UDP runtime is enabled (strict controlled profile only).
    pub enabled: bool,
    /// IPv4 bind literal, or `None` when the IPv4 socket is disabled.
    pub bind_ipv4: Option<IpAddr>,
    /// IPv6 bind literal, or `None` when the IPv6 socket is disabled.
    pub bind_ipv6: Option<IpAddr>,
    /// Bind port. `0` selects an ephemeral port (integration tests);
    /// a configured service port must be in the normal range.
    pub port: u16,
    /// Whether this router advertises an SSU2 address (always false).
    pub advertise: bool,
    /// Whether introducer service is offered (always false).
    pub introducer_service: bool,
    /// Maximum pending (unauthenticated) handshakes.
    pub max_pending_handshakes: usize,
    /// Maximum active (authenticated) sessions.
    pub max_active_sessions: usize,
    /// Maximum pending handshakes for one exact IP.
    pub max_pending_per_ip: usize,
    /// Maximum pending handshakes for one subnet prefix.
    pub max_pending_per_subnet: usize,
    /// Maximum staged outbound datagrams awaiting socket write.
    pub max_datagram_queue_items: usize,
    /// Maximum staged outbound datagram bytes awaiting socket write.
    pub max_datagram_queue_bytes: usize,
    /// Maximum inbound authenticated I2NP messages awaiting dispatch.
    pub max_inbound_i2np_queue: usize,
    /// Total handshake timeout.
    pub handshake_timeout: Duration,
    /// Data-phase idle timeout.
    pub idle_timeout: Duration,
    /// Upper bound for one central-scheduler sleep.
    pub scheduler_poll_max: Duration,
}

/// Immutable normalized configuration snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Filesystem path this snapshot was loaded from, when known.
    /// [`Config::parse`] leaves it `None` (pure text in, no I/O);
    /// [`Config::load`] records the path so long-lived services can
    /// re-read operator intent without re-resolving it. The path is
    /// provenance only: it never affects equality of intent.
    pub source_path: Option<std::path::PathBuf>,
    /// Schema version accepted by the parser.
    pub schema_version: u64,
    /// Router-specific settings.
    pub router: RouterConfig,
    /// Logging settings.
    pub logging: LoggingConfig,
    /// Initial resource limits.
    pub limits: LimitsConfig,
    /// Network settings.
    pub network: NetworkConfig,
    /// Transport settings.
    pub transport: TransportConfig,
    /// NetDB settings.
    pub netdb: NetDbConfig,
    /// Reseed settings.
    pub reseed: ReseedConfig,
    /// Signed NEWS source settings (disabled by default).
    pub news: NewsConfig,
    /// SAM v3.1 service settings.
    pub sam: SamConfig,
    /// SSU2 UDP runtime settings (Plan 158; disabled, loopback-only).
    pub ssu2: Ssu2Config,
    /// I2CP listener settings (Plan 167; disabled, loopback-only).
    pub console: ConsoleConfig,
    /// Managed-application runtime settings (Plan 369; disabled by default,
    /// no listener, distribution-owned manager binary).
    pub app_runtime: AppRuntimeConfig,
    pub i2cp: I2cpConfig,
    /// I2PControl listener settings (Plan 287; disabled, loopback-only TLS).
    pub i2pcontrol: I2pControlConfig,
    /// Service-tunnel settings (Plan 174; disabled, loopback-only,
    /// no listener yet).
    pub service_tunnels: ServiceTunnelsConfig,
    /// Address-book settings (Plan 294; disabled by default; no
    /// filesystem effect while disabled).
    pub addressbook: crate::addressbook::AddressBookSubsystemConfig,
    /// Normal floodfill opt-in (Plan 279; default off, intent only).
    pub floodfill: FloodfillConfig,
}

/// Normalized Plan 279 normal floodfill opt-in.
///
/// This is intent, not authority: `enabled = true` asks the daemon to
/// evaluate normal-path eligibility on every bounded tick. The
/// `i2pr_netdb::FloodfillEligibilitySnapshot` AND-gate still decides, `caps=f`
/// is built only through the permit-gated builder after the role
/// reaches Active, and any eligibility loss withdraws the
/// advertisement. The struct carries no key material, no permit, and
/// no reference to the advertisement builders (see
/// `scripts/check-m12-floodfill-boundaries.sh`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillConfig {
    /// Whether the operator requests floodfill eligibility evaluation.
    pub enabled: bool,
}

/// Normalized service-tunnel configuration.
///
/// The surface is strict, disabled by default, and loopback-only
/// for local listeners. Enabled specifications pass configuration
/// validation but daemon graph construction rejects activation until
/// a normal-daemon Destination-group provider owns them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceTunnelsConfig {
    /// Whether the service-tunnel subsystem is enabled.
    pub enabled: bool,
    /// Central resource ceilings.
    pub limits: i2pr_service_tunnels::ServiceResourceLimits,
    /// Central deadline ceilings.
    pub timeouts: i2pr_service_tunnels::ServiceTimeouts,
    /// Validated service specifications.
    pub tunnels: i2pr_service_tunnels::ServiceTunnelSet,
    /// Validated static alias table.
    pub aliases: i2pr_service_tunnels::StaticAliasTable,
    /// Explicit local TLS identity/trust policy for server
    /// `use_ssl` dials (Plan 297; `None` means no policy and
    /// `use_ssl` tunnels fail before connecting).
    pub tls_policy: Option<crate::service_tunnels_tls::TlsPolicyHandle>,
}

impl Config {
    /// Loads, validates, and normalizes a TOML configuration without mutation.
    pub fn load(path: &Path) -> Result<Self, super::error::DaemonError> {
        let contents = fs::read_to_string(path).map_err(|source| {
            super::error::DaemonError::ConfigUnavailable {
                path: path.to_path_buf(),
                source,
            }
        })?;
        let mut config = Self::parse(&contents).map_err(super::error::DaemonError::from)?;
        // Plan 352 D2: a config carrying a plaintext password must not be
        // readable by anyone but its owner. Checked *after* parsing so a
        // secret-free config is never rejected -- the gate is conditional on
        // there actually being a secret to protect.
        if !config.i2pcontrol.password.is_empty() {
            check_secret_file_permissions(path)?;
        }
        config.source_path = Some(path.to_path_buf());
        Ok(config)
    }

    /// Minimal text-only config used by tests that exercise
    /// semantic-validation paths without needing a real data
    /// directory on disk. The supplied `data_dir` is recorded as-is
    /// (the parser never creates it).
    #[cfg(test)]
    pub fn default_for_test_with_data_dir(data_dir: &str) -> String {
        format!("schema_version = 1\n[router]\ndata_dir = {data_dir:?}\n")
    }

    /// Minimal text-only config used by tests that do not need a
    /// specific data directory.
    #[cfg(test)]
    pub fn default_for_test() -> String {
        Self::default_for_test_with_data_dir("./state")
    }

    /// Parses, validates, and normalizes TOML configuration text.
    pub fn parse(contents: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(contents)
            .map_err(|error| ConfigError::Parse(RedactedTomlError::new(error, contents)))?;
        if raw.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchemaVersion {
                actual: raw.schema_version,
            });
        }

        let data_dir = normalize_data_dir(&raw.router.data_dir)?;
        let profile = match raw.router.profile.as_str() {
            "balanced" => RouterProfile::Balanced,
            _ => {
                return Err(ConfigError::Semantic {
                    field: "router.profile",
                    reason: "must be \"balanced\" in this milestone",
                });
            }
        };
        if raw.logging.filter.trim().is_empty() {
            return Err(ConfigError::Semantic {
                field: "logging.filter",
                reason: "must not be empty",
            });
        }
        if raw.logging.filter.len() > 128 {
            return Err(ConfigError::Semantic {
                field: "logging.filter",
                reason: "must not exceed 128 bytes",
            });
        }
        let format = match raw.logging.format.as_str() {
            "text" => LogFormat::Text,
            _ => {
                return Err(ConfigError::Semantic {
                    field: "logging.format",
                    reason: "must be \"text\" in this milestone",
                });
            }
        };
        validate_limit("limits.max_tasks", raw.limits.max_tasks, MAX_ALLOWED_TASKS)?;
        validate_limit(
            "limits.max_buffered_bytes",
            raw.limits.max_buffered_bytes,
            MAX_ALLOWED_BUFFERED_BYTES,
        )?;

        let bind_address = raw
            .network
            .bind_address
            .parse()
            .map_err(|_| ConfigError::Semantic {
                field: "network.bind_address",
                reason: "must be a valid IP address",
            })?;
        if raw.network.listen_port == 0 {
            return Err(ConfigError::Semantic {
                field: "network.listen_port",
                reason: "must be greater than zero",
            });
        }
        if raw.network.network_id == 0 {
            return Err(ConfigError::Semantic {
                field: "network.network_id",
                reason: "must be greater than zero",
            });
        }

        let ntcp2 = normalize_ntcp2(&raw.transport.ntcp2)?;
        let netdb = normalize_netdb(&raw.netdb)?;
        let reseed = normalize_reseed(&raw.reseed, &netdb)?;
        let news = normalize_news(&raw.news)?;
        let console = normalize_console(&raw.console, &raw.limits)?;
        let app_runtime = normalize_app_runtime(&raw.app_runtime);
        let sam = normalize_sam(&raw.sam, &raw.limits)?;
        let ssu2 = normalize_ssu2(&raw.ssu2)?;
        let i2cp = normalize_i2cp(&raw.i2cp, &raw.limits)?;
        let i2pcontrol = normalize_i2pcontrol(&raw.i2pcontrol, &raw.limits)?;
        let service_tunnels = normalize_service_tunnels(&raw.service_tunnels, &raw.limits)?;
        let addressbook = normalize_addressbook(&raw.addressbook, &data_dir)?;
        let floodfill = normalize_floodfill(&raw.floodfill, &ssu2, &netdb)?;

        Ok(Self {
            source_path: None,
            schema_version: raw.schema_version,
            router: RouterConfig { data_dir, profile },
            logging: LoggingConfig {
                filter: raw.logging.filter,
                format,
            },
            limits: LimitsConfig {
                max_tasks: raw.limits.max_tasks,
                max_buffered_bytes: raw.limits.max_buffered_bytes,
            },
            network: NetworkConfig {
                bind_address,
                listen_port: raw.network.listen_port,
                network_id: raw.network.network_id,
            },
            transport: TransportConfig { ntcp2 },
            netdb,
            reseed,
            console,
            app_runtime,
            news,
            sam,
            ssu2,
            i2cp,
            i2pcontrol,
            service_tunnels,
            addressbook,
            floodfill,
        })
    }
}

/// Normalizes the Plan 287 `[i2pcontrol]` block.
///
/// Fail-closed order: bind shape, TLS-material pairing, non-loopback TLS
/// requirement, password presence when enabled, then resource ceilings.
/// Every rejection happens before any listener bind or managed-certificate
/// side effect.
fn normalize_i2pcontrol(
    raw: &RawI2pControlConfig,
    global: &RawLimitsConfig,
) -> Result<I2pControlConfig, ConfigError> {
    let bind_address: IpAddr = raw
        .bind_address
        .parse()
        .map_err(|_| ConfigError::Semantic {
            field: "i2pcontrol.bind_address",
            reason: "must be a valid IP address",
        })?;
    let certificate = if raw.certificate.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(raw.certificate.trim()))
    };
    let private_key = if raw.private_key.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(raw.private_key.trim()))
    };
    // Half-configured TLS always fails, enabled or not: there is no
    // fallback from bad explicit material to managed TLS.
    if certificate.is_some() != private_key.is_some() {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.certificate",
            reason: "certificate and private_key must both be set or both be empty",
        });
    }
    // Non-loopback (including wildcard) binds require a complete explicit
    // certificate/private-key pair before anything else happens.
    if !bind_address.is_loopback() && certificate.is_none() {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.bind_address",
            reason: "non-loopback bind requires explicit certificate and private_key",
        });
    }
    if raw.enabled {
        if raw.password.is_empty() {
            return Err(ConfigError::Semantic {
                field: "i2pcontrol.password",
                reason: "must be non-empty when the I2PControl listener is enabled",
            });
        }
        if raw.password.len() > MAX_I2PCONTROL_PASSWORD_BYTES {
            return Err(ConfigError::Semantic {
                field: "i2pcontrol.password",
                reason: "must not exceed 1024 bytes",
            });
        }
    }
    if raw.max_connections == 0 || raw.max_connections > MAX_I2PCONTROL_CONNECTIONS {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.max_connections",
            reason: "must be within 1..=256",
        });
    }
    if raw.max_body_bytes == 0 || raw.max_body_bytes > MAX_I2PCONTROL_BODY_BYTES {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.max_body_bytes",
            reason: "must be within 1..=1048576",
        });
    }
    if raw.request_deadline_ms < MIN_I2PCONTROL_REQUEST_DEADLINE_MS
        || raw.request_deadline_ms > MAX_I2PCONTROL_REQUEST_DEADLINE_MS
    {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.request_deadline_ms",
            reason: "must be within 1000..=60000",
        });
    }
    if raw.shutdown_timeout_ms < MIN_I2PCONTROL_SHUTDOWN_TIMEOUT_MS
        || raw.shutdown_timeout_ms > MAX_I2PCONTROL_SHUTDOWN_TIMEOUT_MS
    {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.shutdown_timeout_ms",
            reason: "must be within 500..=30000",
        });
    }
    // The I2PControl connection/body budget must fit the router-wide
    // ceilings so an optional disabled-by-default listener can never
    // overcommit global resources.
    let aggregate_budget = u64::from(raw.max_connections).saturating_mul(raw.max_body_bytes as u64);
    if u64::from(raw.max_connections) > global.max_tasks
        || aggregate_budget > global.max_buffered_bytes
    {
        return Err(ConfigError::Semantic {
            field: "i2pcontrol.aggregate",
            reason: "I2PControl connection and body ceilings exceed router-wide budgets",
        });
    }
    Ok(I2pControlConfig {
        enabled: raw.enabled,
        bind_address,
        port: raw.port,
        password: raw.password.clone(),
        certificate,
        private_key,
        max_connections: raw.max_connections,
        max_body_bytes: raw.max_body_bytes,
        request_deadline: Duration::from_millis(raw.request_deadline_ms),
        shutdown_timeout: Duration::from_millis(raw.shutdown_timeout_ms),
    })
}

fn normalize_i2cp(
    raw: &RawI2cpConfig,
    global: &RawLimitsConfig,
) -> Result<I2cpConfig, ConfigError> {
    let bind_address: IpAddr = raw
        .bind_address
        .parse()
        .map_err(|_| ConfigError::Semantic {
            field: "i2cp.bind_address",
            reason: "must be a valid IP address",
        })?;
    if !bind_address.is_loopback() {
        return Err(ConfigError::Semantic {
            field: "i2cp.bind_address",
            reason: "must be a loopback address while remote I2CP exposure is unsupported",
        });
    }
    if raw.max_clients == 0 {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_clients",
            reason: "must be greater than zero",
        });
    }
    if raw.max_clients > MAX_I2CP_CLIENTS {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_clients",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.max_sessions_per_connection == 0 {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_sessions_per_connection",
            reason: "must be greater than zero",
        });
    }
    if raw.max_sessions_per_connection > MAX_I2CP_SESSIONS_PER_CONNECTION {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_sessions_per_connection",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.max_sessions_router == 0 {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_sessions_router",
            reason: "must be greater than zero",
        });
    }
    if raw.max_sessions_router > MAX_I2CP_SESSIONS_ROUTER {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_sessions_router",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.max_buffered_bytes_per_connection == 0 {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_buffered_bytes_per_connection",
            reason: "must be greater than zero",
        });
    }
    if raw.max_buffered_bytes_per_connection > MAX_I2CP_BUFFERED_BYTES_PER_CONNECTION {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_buffered_bytes_per_connection",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.max_pending_writes_per_connection == 0 {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_pending_writes_per_connection",
            reason: "must be greater than zero",
        });
    }
    if raw.max_pending_writes_per_connection > MAX_I2CP_PENDING_WRITES_PER_CONNECTION {
        return Err(ConfigError::Semantic {
            field: "i2cp.max_pending_writes_per_connection",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    let protocol_byte_timeout = Duration::from_millis(raw.protocol_byte_timeout_ms);
    let command_timeout = Duration::from_millis(raw.command_timeout_ms);
    let shutdown_timeout = Duration::from_millis(raw.shutdown_timeout_ms);

    if protocol_byte_timeout.is_zero()
        || raw.protocol_byte_timeout_ms < MIN_I2CP_PROTOCOL_BYTE_TIMEOUT_MS
        || raw.protocol_byte_timeout_ms > MAX_I2CP_PROTOCOL_BYTE_TIMEOUT_MS
    {
        return Err(ConfigError::Semantic {
            field: "i2cp.protocol_byte_timeout_ms",
            reason: "must be within 1000..=60000",
        });
    }
    if command_timeout.is_zero()
        || raw.command_timeout_ms < MIN_I2CP_COMMAND_TIMEOUT_MS
        || raw.command_timeout_ms > MAX_I2CP_COMMAND_TIMEOUT_MS
    {
        return Err(ConfigError::Semantic {
            field: "i2cp.command_timeout_ms",
            reason: "must be within 5000..=3600000",
        });
    }
    if shutdown_timeout.is_zero()
        || raw.shutdown_timeout_ms < MIN_I2CP_SHUTDOWN_TIMEOUT_MS
        || raw.shutdown_timeout_ms > MAX_I2CP_SHUTDOWN_TIMEOUT_MS
    {
        return Err(ConfigError::Semantic {
            field: "i2cp.shutdown_timeout_ms",
            reason: "must be within 1000..=30000",
        });
    }

    // Plan 167 §2: the I2CP listener must not silently exceed the
    // router-wide task or buffered-byte ceilings. Tests exercise a
    // 64 KiB per-connection read ceiling while keeping the router-wide
    // budget far below the SAM defaults.
    let per_connection_budget = u64::from(raw.max_clients)
        .saturating_mul(u64::from(raw.max_sessions_per_connection))
        .saturating_mul(raw.max_buffered_bytes_per_connection as u64);
    let aggregate_budget =
        u64::from(raw.max_clients).saturating_mul(raw.max_buffered_bytes_per_connection as u64);
    if u64::from(raw.max_clients) > global.max_tasks
        || aggregate_budget > global.max_buffered_bytes
        || per_connection_budget > global.max_buffered_bytes
    {
        return Err(ConfigError::Semantic {
            field: "i2cp.aggregate",
            reason: "I2CP client and buffered-byte ceilings exceed router-wide budgets",
        });
    }

    Ok(I2cpConfig {
        enabled: raw.enabled,
        bind_address,
        port: raw.port,
        max_clients: raw.max_clients,
        max_sessions_per_connection: raw.max_sessions_per_connection,
        max_sessions_router: raw.max_sessions_router,
        max_buffered_bytes_per_connection: raw.max_buffered_bytes_per_connection,
        max_pending_writes_per_connection: raw.max_pending_writes_per_connection,
        protocol_byte_timeout,
        command_timeout,
        shutdown_timeout,
    })
}

fn normalize_service_tunnels(
    raw: &RawServiceTunnelsConfig,
    global: &RawLimitsConfig,
) -> Result<ServiceTunnelsConfig, ConfigError> {
    use i2pr_service_tunnels::{
        DestinationGroupId, DestinationPolicy, LocalListenerSpec, ServerTarget,
        ServiceResourceLimits, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind,
        ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
    };

    // Central ceilings validate first; the crate owns the hard maxima.
    let limits = ServiceResourceLimits {
        max_active_connections_per_service:
            i2pr_service_tunnels::MAX_ACTIVE_CONNECTIONS_PER_SERVICE
                .min(raw.max_active_connections.max(1)),
        max_active_connections_aggregate: raw.max_active_connections,
        max_buffered_bytes_per_direction: raw.max_buffered_bytes_per_direction,
        max_configured_targets: i2pr_service_tunnels::MAX_CONFIGURED_TARGETS,
    };
    // Validate aggregate and per-direction ceilings explicitly so
    // daemon field names appear in diagnostics.
    if raw.max_active_connections == 0
        || raw.max_active_connections > i2pr_service_tunnels::MAX_ACTIVE_CONNECTIONS_AGGREGATE
    {
        return Err(ConfigError::Semantic {
            field: "service_tunnels.max_active_connections",
            reason: "must be within 1..=1024",
        });
    }
    if raw.max_buffered_bytes_per_direction < i2pr_service_tunnels::MIN_BUFFERED_BYTES_PER_DIRECTION
        || raw.max_buffered_bytes_per_direction
            > i2pr_service_tunnels::MAX_BUFFERED_BYTES_PER_DIRECTION
    {
        return Err(ConfigError::Semantic {
            field: "service_tunnels.max_buffered_bytes_per_direction",
            reason: "must be within 1024..=1048576",
        });
    }
    let timeouts = ServiceTimeouts {
        connect_timeout_ms: raw.connect_timeout_ms,
        read_timeout_ms: raw.read_timeout_ms,
        write_timeout_ms: raw.write_timeout_ms,
        shutdown_timeout_ms: raw.shutdown_timeout_ms,
        streaming_connect_delay_ms: None,
        delay_open: false,
    };
    timeouts.validate().map_err(|_| ConfigError::Semantic {
        field: "service_tunnels.timeouts",
        reason: "service tunnel deadline out of range",
    })?;
    limits.validate().map_err(|_| ConfigError::Semantic {
        field: "service_tunnels.limits",
        reason: "service tunnel resource ceiling out of range",
    })?;

    // Router-wide budget guard: service tunnels must not silently
    // exceed the global task or buffered-byte ceilings.
    if (raw.max_active_connections as u64) > global.max_tasks {
        return Err(ConfigError::Semantic {
            field: "service_tunnels.aggregate",
            reason: "service tunnel connection ceiling exceeds router-wide tasks",
        });
    }
    let aggregate_bytes = (raw.max_active_connections as u64)
        .saturating_mul(raw.max_buffered_bytes_per_direction as u64)
        .saturating_mul(2);
    if aggregate_bytes > global.max_buffered_bytes {
        return Err(ConfigError::Semantic {
            field: "service_tunnels.aggregate",
            reason: "service tunnel buffered-byte ceiling exceeds router-wide budget",
        });
    }

    // Static alias table.
    let mut aliases = StaticAliasTable::new();
    for entry in &raw.alias {
        let target = i2pr_service_tunnels::DestinationRef::parse(&entry.target).map_err(|_| {
            ConfigError::Semantic {
                field: "service_tunnels.alias.target",
                reason: "alias target is malformed",
            }
        })?;
        aliases
            .insert(&entry.name, target)
            .map_err(|_| ConfigError::Semantic {
                field: "service_tunnels.alias",
                reason: "alias is malformed, duplicate, or exceeds the ceiling",
            })?;
    }

    // Service specifications.
    let mut specs = Vec::with_capacity(raw.tunnel.len());
    for entry in &raw.tunnel {
        let id = ServiceTunnelId::parse(&entry.id).map_err(|_| ConfigError::Semantic {
            field: "service_tunnels.tunnel.id",
            reason: "service tunnel id is malformed",
        })?;
        let kind = ServiceTunnelKind::parse(&entry.kind).map_err(|_| ConfigError::Semantic {
            field: "service_tunnels.tunnel.kind",
            reason: "service tunnel kind is unsupported",
        })?;
        let listener = entry
            .listener
            .as_deref()
            .map(LocalListenerSpec::parse_socket)
            .transpose()
            .map_err(|_| ConfigError::Semantic {
                field: "service_tunnels.tunnel.listener",
                reason: "local listener must be a loopback ip:port",
            })?;
        let target = entry
            .target
            .as_deref()
            .map(ServerTarget::parse)
            .transpose()
            .map_err(|_| ConfigError::Semantic {
                field: "service_tunnels.tunnel.target",
                reason: "server target must be loopback ip:port or unix:/path",
            })?;
        let mut targets = Vec::with_capacity(entry.targets.len());
        for raw_target in &entry.targets {
            let parsed = ServerTarget::parse(raw_target).map_err(|_| ConfigError::Semantic {
                field: "service_tunnels.tunnel.targets",
                reason: "server target must be loopback ip:port or unix:/path",
            })?;
            targets.push(parsed);
        }
        let destination = entry
            .destination
            .as_deref()
            .map(i2pr_service_tunnels::DestinationRef::parse)
            .transpose()
            .map_err(|_| ConfigError::Semantic {
                field: "service_tunnels.tunnel.destination",
                reason: "destination reference is malformed",
            })?;
        let policy = match entry.group.as_deref() {
            None => DestinationPolicy::Dedicated,
            Some(group) => {
                let parsed =
                    DestinationGroupId::parse(group).map_err(|_| ConfigError::Semantic {
                        field: "service_tunnels.tunnel.group",
                        reason: "destination group id is malformed",
                    })?;
                DestinationPolicy::SharedGroup(parsed)
            }
        };
        let max_connections = entry.max_connections.unwrap_or(16);
        let max_buffered = entry
            .max_buffered_bytes_per_direction
            .unwrap_or(raw.max_buffered_bytes_per_direction);
        let http_options = if matches!(
            kind,
            ServiceTunnelKind::HttpClient | ServiceTunnelKind::HttpBidirServer
        ) {
            Some(i2pr_service_tunnels::HttpClientOptions::defaults())
        } else {
            None
        };
        let socks5_options = if matches!(
            kind,
            ServiceTunnelKind::Socks5Client | ServiceTunnelKind::SocksIrc
        ) {
            Some(i2pr_service_tunnels::Socks5ClientOptions::defaults())
        } else {
            None
        };
        let irc_options = if matches!(
            kind,
            ServiceTunnelKind::IrcClient | ServiceTunnelKind::SocksIrc
        ) {
            Some(i2pr_service_tunnels::IrcClientOptions::defaults())
        } else {
            None
        };
        // Plan 290: the strict CONNECT profile carries its own
        // CONNECT-only port policy; the HTTP server profile carries
        // no options (fixed secure filter over `HttpLimits`).
        let connect_options = if matches!(kind, ServiceTunnelKind::ConnectClient) {
            Some(i2pr_service_tunnels::ConnectClientOptions::defaults())
        } else {
            None
        };
        // Plan 291: the Streamr halves carry validated UDP
        // endpoints plus freeze-default cadence/ceiling policy.
        // Endpoints are explicit per half (no silent default for
        // where media enters or exits); loopback shape is enforced
        // by `StreamrOptions::validate`.
        let streamr_options = if matches!(
            kind,
            ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
        ) {
            let local_udp = entry
                .local_udp
                .as_deref()
                .map(|text| {
                    text.parse::<std::net::SocketAddr>()
                        .map_err(|_| ConfigError::Semantic {
                            field: "service_tunnels.tunnel.udp",
                            reason: "local UDP endpoint must be a loopback ip:port",
                        })
                })
                .transpose()?;
            let options = i2pr_service_tunnels::StreamrOptions {
                local_udp,
                ..i2pr_service_tunnels::StreamrOptions::default()
            };
            Some(options)
        } else {
            None
        };
        let spec = ServiceTunnelSpec {
            id,
            kind,
            enabled: entry.enabled,
            listener,
            target,
            targets,
            destination,
            policy,
            inbound_port: entry.inbound_port,
            max_connections,
            max_buffered_bytes_per_direction: max_buffered,
            timeouts,
            shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: i2pr_service_tunnels::IdlePolicy::disabled(),
            access: i2pr_service_tunnels::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: i2pr_service_tunnels::HttpServerPolicy::default(),
            http_options,
            socks5_options,
            irc_options,
            connect_options,
            streamr_options,
        };
        spec.validate().map_err(|err| match err {
            i2pr_service_tunnels::ServiceTunnelError::DuplicateId { .. }
            | i2pr_service_tunnels::ServiceTunnelError::DuplicateListener { .. } => {
                ConfigError::Semantic {
                    field: "service_tunnels.tunnel",
                    reason: "duplicate service id or listener",
                }
            }
            i2pr_service_tunnels::ServiceTunnelError::InvalidListener { .. } => {
                ConfigError::Semantic {
                    field: "service_tunnels.tunnel.listener",
                    reason: "local listener must be a loopback ip:port",
                }
            }
            i2pr_service_tunnels::ServiceTunnelError::InvalidTarget { .. } => {
                ConfigError::Semantic {
                    field: "service_tunnels.tunnel.target",
                    reason: "server target must be loopback ip:port or unix:/path",
                }
            }
            i2pr_service_tunnels::ServiceTunnelError::InvalidDestinationRef { .. }
            | i2pr_service_tunnels::ServiceTunnelError::InvalidAlias { .. } => {
                ConfigError::Semantic {
                    field: "service_tunnels.tunnel.destination",
                    reason: "destination reference is malformed",
                }
            }
            i2pr_service_tunnels::ServiceTunnelError::ContradictoryOptions { .. } => {
                ConfigError::Semantic {
                    field: "service_tunnels.tunnel",
                    reason: "service tunnel options are contradictory",
                }
            }
            _ => ConfigError::Semantic {
                field: "service_tunnels.tunnel",
                reason: "service tunnel specification exceeds a ceiling",
            },
        })?;
        specs.push(spec);
    }
    let set = ServiceTunnelSet { tunnels: specs };
    set.validate().map_err(|_| ConfigError::Semantic {
        field: "service_tunnels.tunnel",
        reason: "duplicate service id, duplicate listener, or ceiling exceeded",
    })?;

    // Plan 175 §13/§6 + Plan 176 §13 + Plan 177 §13 + Plan 178 §13
    // + Plan 179 §14 + Plan 290 + Plan 291: `generic-client`,
    // `generic-server`, `http-client`, `socks5-client`,
    // `irc-client`, `irc-server`, `connect-client`, `socks-irc`,
    // `http-server`, `http-bidir-server`, `streamr-client`, and
    // `streamr-server` tunnels may activate after their plans land.
    // The `ServiceTunnelKind` enum is closed; the match is
    // exhaustive, so every known kind is accepted and the loop
    // exists as a documented invariant.
    for spec in set.tunnels.iter().filter(|spec| spec.enabled) {
        match spec.kind {
            i2pr_service_tunnels::ServiceTunnelKind::GenericClient
            | i2pr_service_tunnels::ServiceTunnelKind::GenericServer
            | i2pr_service_tunnels::ServiceTunnelKind::HttpClient
            | i2pr_service_tunnels::ServiceTunnelKind::Socks5Client
            | i2pr_service_tunnels::ServiceTunnelKind::IrcClient
            | i2pr_service_tunnels::ServiceTunnelKind::IrcServer
            | i2pr_service_tunnels::ServiceTunnelKind::ConnectClient
            | i2pr_service_tunnels::ServiceTunnelKind::SocksIrc
            | i2pr_service_tunnels::ServiceTunnelKind::HttpServer
            | i2pr_service_tunnels::ServiceTunnelKind::HttpBidirServer
            | i2pr_service_tunnels::ServiceTunnelKind::StreamrClient
            | i2pr_service_tunnels::ServiceTunnelKind::StreamrServer => {}
        }
    }

    Ok(ServiceTunnelsConfig {
        enabled: raw.enabled,
        limits,
        timeouts,
        tunnels: set,
        aliases,
        tls_policy: raw
            .tls
            .as_ref()
            .map(normalize_service_tls)
            .transpose()?
            .map(crate::service_tunnels_tls::TlsPolicyHandle::new),
    })
}

/// Normalizes the Plan 297 `[service_tunnels.tls]` block.
///
/// File I/O happens here at configuration load (fail fast, before
/// any bind), never per connection. Error reasons are static;
/// paths and key material never enter diagnostics.
fn normalize_service_tls(
    raw: &RawServiceTlsConfig,
) -> Result<crate::service_tunnels_tls::ServiceTlsPolicy, ConfigError> {
    use crate::service_tunnels_tls::read_tls_file;
    if raw.certificate_path.is_some() != raw.private_key_path.is_some() {
        return Err(ConfigError::Semantic {
            field: "service_tunnels.tls",
            reason: "certificate_path and private_key_path are both required",
        });
    }
    let identity = match (&raw.certificate_path, &raw.private_key_path) {
        (Some(cert_path), Some(key_path)) => Some((
            read_tls_file("certificate", cert_path).map_err(tls_config_error)?,
            read_tls_file("private key", key_path).map_err(tls_config_error)?,
        )),
        (None, None) => None,
        // The completeness check above excludes the mixed cases.
        (Some(_), None) | (None, Some(_)) => {
            return Err(ConfigError::Semantic {
                field: "service_tunnels.tls",
                reason: "certificate_path and private_key_path are both required",
            });
        }
    };
    let roots_pem = raw
        .trust_roots_path
        .as_ref()
        .map(|path| read_tls_file("trust roots", path).map_err(tls_config_error))
        .transpose()?;
    let pins_pem = raw
        .pinned_certificates_path
        .as_ref()
        .map(|path| read_tls_file("pinned certificates", path).map_err(tls_config_error))
        .transpose()?;
    crate::service_tunnels_tls::ServiceTlsPolicy::from_parts(identity, pins_pem, roots_pem)
        .map_err(tls_config_error)
}

/// Maps a TLS policy failure to a static config reason (never echoes
/// paths, pins, or key material).
fn tls_config_error(error: crate::service_tunnels_tls::ServiceTlsError) -> ConfigError {
    use crate::service_tunnels_tls::ServiceTlsError as Tls;
    let reason = match error {
        Tls::IncompleteIdentity => "certificate_path and private_key_path are both required",
        Tls::CannotRead { .. } => "a configured TLS file cannot be read",
        Tls::CannotParse { .. } => "a configured TLS file does not parse",
        Tls::EmptyChain => "the certificate file holds no certificate",
        Tls::EmptyRoots => "a trust bundle file holds no certificate",
        Tls::BadCertificate => "a certificate does not parse as X.509",
        Tls::IdentityMismatch => "the certificate and key do not combine",
        Tls::VerifiesNothing => "the policy verifies nothing",
        Tls::NoPolicy => "no TLS policy is installed",
        Tls::Handshake(_) => "the TLS handshake failed",
    };
    ConfigError::Semantic {
        field: "service_tunnels.tls",
        reason,
    }
}

/// Normalizes the Plan 294 `[addressbook]` block.
///
/// Shape-only validation: `enabled` defaults false; `state_dir` must
/// be non-empty without NUL bytes and resolves against the router
/// data directory when relative. Existence/permission checks happen
/// at activation (failure deactivates with a sticky error), never
/// here: parsing must not touch the filesystem.
fn normalize_addressbook(
    raw: &RawAddressBookConfig,
    data_dir: &Path,
) -> Result<crate::addressbook::AddressBookSubsystemConfig, ConfigError> {
    let state_dir = match raw.state_dir.as_deref() {
        None => data_dir.join(i2pr_storage::ADDRESSBOOK_STATE_SUBDIR),
        Some(value) => {
            if value.trim().is_empty() || value.contains('\0') {
                return Err(ConfigError::Semantic {
                    field: "addressbook.state_dir",
                    reason: "must be a non-empty path without NUL bytes",
                });
            }
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                data_dir.join(path)
            }
        }
    };
    Ok(crate::addressbook::AddressBookSubsystemConfig {
        enabled: raw.enabled,
        state_dir,
    })
}

fn normalize_data_dir(value: &str) -> Result<PathBuf, ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::Semantic {
            field: "router.data_dir",
            reason: "must not be empty",
        });
    }
    let path = PathBuf::from(value);
    match fs::metadata(&path) {
        Ok(metadata) if !metadata.is_dir() => Err(ConfigError::Semantic {
            field: "router.data_dir",
            reason: "existing path is not a directory",
        }),
        Ok(_) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path),
        Err(_) => Err(ConfigError::Semantic {
            field: "router.data_dir",
            reason: "existing path cannot be inspected",
        }),
    }
}

/// Normalizes the console block.
///
/// Three rules are enforced here rather than at the listener:
///
/// - the bind address must be loopback (Plan 356, like every other
///   non-public listener in this router);
/// - the theme must be a compiled-in identifier, so `i2pr check-config`
///   rejects a typo instead of silently serving the default palette;
/// - the connection ceiling must sit inside an explicit range and inside
///   the router-wide task budget.
fn normalize_console(
    raw: &RawConsoleConfig,
    global: &RawLimitsConfig,
) -> Result<ConsoleConfig, ConfigError> {
    let bind_address: IpAddr = raw
        .bind_address
        .parse()
        .map_err(|_| ConfigError::Semantic {
            field: "console.bind_address",
            reason: "must be a valid IP address",
        })?;
    if !bind_address.is_loopback() {
        return Err(ConfigError::Semantic {
            field: "console.bind_address",
            reason: "must be a loopback address; the console has no remote-exposure design",
        });
    }
    // Runtime ceilings only bind a console that is actually started. A
    // disabled console must not consume budget or fail validation, or it
    // would shadow the error attribution of the subsystem that *does* own
    // the budget.
    if raw.enabled {
        if !(1..=64).contains(&raw.max_connections) {
            return Err(ConfigError::Semantic {
                field: "console.max_connections",
                reason: "must be between 1 and 64",
            });
        }
        if u64::from(raw.max_connections) > global.max_tasks {
            return Err(ConfigError::Semantic {
                field: "console.max_connections",
                reason: "exceeds the router-wide task budget",
            });
        }
    }
    let theme =
        i2pr_console::theme::ThemeName::resolve(&raw.theme).map_err(|_| ConfigError::Semantic {
            field: "console.theme",
            reason: "must be a bundled console theme identifier",
        })?;

    // Authenticated mode requires real credential material. An empty
    // password is refused rather than accepted, so `auth = true` can never
    // mean "anyone may sign in".
    let mut password_hash: Option<ConsolePasswordHash> = None;
    if raw.auth {
        if let Some(hash) = raw.password_hash.as_deref() {
            // Validated here so a malformed supplied hash fails
            // `check-config` instead of the listener.
            i2pr_console::security::auth::verifier_from_hash(hash).map_err(|error| {
                ConfigError::Semantic {
                    field: "console.password_hash",
                    reason: match error {
                        i2pr_console::security::auth::PasswordError::UnsupportedAlgorithm => {
                            "must be an Argon2id PHC hash"
                        }
                        i2pr_console::security::auth::PasswordError::UnsafeHashParameters => {
                            "must use parameters inside the console's accepted range"
                        }
                        _ => "must be a valid Argon2id PHC hash",
                    },
                }
            })?;
            password_hash = Some(ConsolePasswordHash::new(hash.to_string()));
        } else if let Some(password) = raw.password.as_deref() {
            let secret = i2pr_console::ConsoleSecret::new(password);
            let mut derived =
                i2pr_console::security::auth::derive_from_password(secret).map_err(|error| {
                    ConfigError::Semantic {
                        field: "console.password",
                        reason: match error {
                            i2pr_console::security::auth::PasswordError::EmptyPassword => {
                                "must not be empty when auth is enabled"
                            }
                            _ => "could not be converted to a password verifier",
                        },
                    }
                })?;
            let verifier = derived.verifier();
            password_hash = Some(ConsolePasswordHash::new(
                verifier.hash_for_audit().to_string(),
            ));
            // Drop the plaintext verifier material before returning.
            derived.forget_temporary();
        } else {
            return Err(ConfigError::Semantic {
                field: "console.password",
                reason: "auth = true requires either a password or a password_hash",
            });
        }
    } else if raw.password.is_some() || raw.password_hash.is_some() {
        return Err(ConfigError::Semantic {
            field: "console.password",
            reason: "credential material is only accepted when auth = true",
        });
    }

    // Session bounds are validated by the console owner, so the same
    // ceilings apply whether they arrive from configuration or a test. Like
    // the connection ceiling, they only bind an enabled console.
    if raw.enabled
        && i2pr_console::SessionLimits::validate(
            raw.max_sessions,
            std::time::Duration::from_secs(raw.session_idle_secs),
            std::time::Duration::from_secs(raw.session_absolute_secs),
        )
        .is_none()
    {
        return Err(ConfigError::Semantic {
            field: "console.sessions",
            reason: "session ceilings or lifetimes are outside the accepted range",
        });
    }
    if raw.enabled && (raw.login_max_failures == 0 || raw.login_window_secs == 0) {
        return Err(ConfigError::Semantic {
            field: "console.login_throttle",
            reason: "must request at least one failure inside a non-empty window",
        });
    }

    Ok(ConsoleConfig {
        enabled: raw.enabled,
        bind_address,
        port: raw.port,
        theme: theme.as_str().to_string(),
        max_connections: raw.max_connections,
        auth: raw.auth,
        password_hash,
        max_sessions: raw.max_sessions,
        session_idle_secs: raw.session_idle_secs,
        session_absolute_secs: raw.session_absolute_secs,
        login_max_failures: raw.login_max_failures,
        login_window_secs: raw.login_window_secs,
    })
}

fn normalize_sam(raw: &RawSamConfig, global: &RawLimitsConfig) -> Result<SamConfig, ConfigError> {
    let bind_address: IpAddr = raw
        .bind_address
        .parse()
        .map_err(|_| ConfigError::Semantic {
            field: "sam.bind_address",
            reason: "must be a valid IP address",
        })?;
    // Plan 137 §3: a non-loopback bind address must be rejected
    // outright. Remote exposure requires a future authenticated
    // security design.
    if !bind_address.is_loopback() {
        return Err(ConfigError::Semantic {
            field: "sam.bind_address",
            reason: "must be a loopback address while remote SAM exposure is unsupported",
        });
    }
    let limits = i2pr_api::sam::limits::SamLimits::validate(i2pr_api::sam::limits::SamLimits {
        enabled: raw.enabled,
        max_clients: raw.max_clients,
        max_sessions: raw.max_sessions,
        max_stream_sockets_per_session: raw.max_stream_sockets_per_session,
        max_pending_accepts_per_session: raw.max_pending_accepts_per_session,
        max_buffered_bytes_per_stream_direction: raw.max_buffered_bytes_per_stream_direction,
        hello_timeout: Duration::from_millis(raw.hello_timeout_ms),
        command_timeout: Duration::from_millis(raw.command_timeout_ms),
        shutdown_timeout: Duration::from_millis(raw.shutdown_timeout_ms),
    })
    .map_err(|_| ConfigError::Semantic {
        field: "sam.limits",
        reason: "configuration failed SAM limits validation",
    })?;
    let stream_budget = u64::from(limits.max_sessions)
        .saturating_mul(u64::from(limits.max_stream_sockets_per_session))
        .saturating_mul(limits.max_buffered_bytes_per_stream_direction as u64)
        .saturating_mul(2);
    if u64::from(limits.max_clients) > global.max_tasks || stream_budget > global.max_buffered_bytes
    {
        return Err(ConfigError::Semantic {
            field: "sam.aggregate",
            reason: "SAM client and stream-buffer ceilings exceed router-wide budgets",
        });
    }
    Ok(SamConfig {
        enabled: raw.enabled,
        bind_address,
        port: raw.port,
        limits,
    })
}

/// Parses one SSU2 bind literal: empty means the family socket stays
/// disabled, otherwise the literal must parse as the expected family
/// and must be loopback while non-loopback exposure is unsupported.
fn parse_ssu2_bind(
    field: &'static str,
    value: &str,
    ipv6: bool,
) -> Result<Option<IpAddr>, ConfigError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let address: IpAddr = value.parse().map_err(|_| ConfigError::Semantic {
        field,
        reason: "must be a valid IP address or empty to disable",
    })?;
    let family_ok = match address {
        IpAddr::V4(_) => !ipv6,
        IpAddr::V6(_) => ipv6,
    };
    if !family_ok {
        return Err(ConfigError::Semantic {
            field,
            reason: "address family does not match its bind field",
        });
    }
    if !address.is_loopback() {
        return Err(ConfigError::Semantic {
            field,
            reason: "must be a loopback address while non-loopback SSU2 exposure is unsupported",
        });
    }
    Ok(Some(address))
}

fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    // Plan 184 §3: the daemon accepts `enabled = true` only under the
    // strict controlled profile (loopback bind, non-advertised, no
    // introducer/relay publication). Broader exposure stays rejected
    // fail-closed; protocol semantics are unchanged from Plan 161.
    if raw.advertise {
        return Err(ConfigError::Semantic {
            field: "ssu2.advertise",
            reason: "SSU2 address publication is unavailable in this milestone",
        });
    }
    if raw.introducer_service {
        return Err(ConfigError::Semantic {
            field: "ssu2.introducer_service",
            reason: "SSU2 introducer service is unavailable in this milestone",
        });
    }
    let bind_ipv4 = parse_ssu2_bind("ssu2.bind_ipv4", &raw.bind_ipv4, false)?;
    let bind_ipv6 = parse_ssu2_bind("ssu2.bind_ipv6", &raw.bind_ipv6, true)?;
    // Plan 184 §3 strict controlled profile: an enabled service must
    // bind at least one loopback family and must never advertise or
    // offer introducer service (both rejected above). `parse_ssu2_bind`
    // already rejects wildcard/non-loopback literals fail-closed.
    if raw.enabled && bind_ipv4.is_none() && bind_ipv6.is_none() {
        return Err(ConfigError::Semantic {
            field: "ssu2.enabled",
            reason: "SSU2 activation requires at least one loopback bind address",
        });
    }
    // Port 0 selects an ephemeral port for integration tests; a
    // configured service port must be in the normal (non-privileged) range.
    if raw.port != 0 && raw.port < MIN_SSU2_SERVICE_PORT {
        return Err(ConfigError::Semantic {
            field: "ssu2.port",
            reason: "must be 0 for an ephemeral port or at least 1024",
        });
    }
    validate_limit(
        "ssu2.max_pending_handshakes",
        raw.max_pending_handshakes as u64,
        MAX_SSU2_PENDING_HANDSHAKES as u64,
    )?;
    validate_limit(
        "ssu2.max_active_sessions",
        raw.max_active_sessions as u64,
        MAX_SSU2_ACTIVE_SESSIONS as u64,
    )?;
    validate_limit(
        "ssu2.max_pending_per_ip",
        raw.max_pending_per_ip as u64,
        MAX_SSU2_PENDING_PER_IP as u64,
    )?;
    validate_limit(
        "ssu2.max_pending_per_subnet",
        raw.max_pending_per_subnet as u64,
        MAX_SSU2_PENDING_PER_SUBNET as u64,
    )?;
    validate_limit(
        "ssu2.max_datagram_queue_items",
        raw.max_datagram_queue_items as u64,
        MAX_SSU2_DATAGRAM_QUEUE_ITEMS as u64,
    )?;
    validate_limit(
        "ssu2.max_datagram_queue_bytes",
        raw.max_datagram_queue_bytes,
        MAX_SSU2_DATAGRAM_QUEUE_BYTES,
    )?;
    validate_limit(
        "ssu2.max_inbound_i2np_queue",
        raw.max_inbound_i2np_queue as u64,
        MAX_SSU2_INBOUND_I2NP_QUEUE as u64,
    )?;
    if raw.max_pending_per_ip > raw.max_pending_handshakes
        || raw.max_pending_per_subnet > raw.max_pending_handshakes
    {
        return Err(ConfigError::Semantic {
            field: "ssu2.pending_scopes",
            reason: "per-IP and per-subnet ceilings must not exceed max_pending_handshakes",
        });
    }
    if raw.handshake_timeout_ms < MIN_SSU2_HANDSHAKE_TIMEOUT_MS
        || raw.handshake_timeout_ms > MAX_SSU2_HANDSHAKE_TIMEOUT_MS
    {
        return Err(ConfigError::Semantic {
            field: "ssu2.handshake_timeout_ms",
            reason: "must be within 5000..=60000",
        });
    }
    validate_duration(
        "ssu2.idle_timeout_ms",
        Duration::from_millis(raw.idle_timeout_ms),
    )?;
    if raw.scheduler_poll_max_ms < MIN_SSU2_SCHEDULER_POLL_MAX_MS
        || raw.scheduler_poll_max_ms > MAX_SSU2_SCHEDULER_POLL_MAX_MS
    {
        return Err(ConfigError::Semantic {
            field: "ssu2.scheduler_poll_max_ms",
            reason: "must be within 10..=1000",
        });
    }
    Ok(Ssu2Config {
        enabled: raw.enabled,
        bind_ipv4,
        bind_ipv6,
        port: raw.port,
        advertise: raw.advertise,
        introducer_service: raw.introducer_service,
        max_pending_handshakes: raw.max_pending_handshakes,
        max_active_sessions: raw.max_active_sessions,
        max_pending_per_ip: raw.max_pending_per_ip,
        max_pending_per_subnet: raw.max_pending_per_subnet,
        max_datagram_queue_items: raw.max_datagram_queue_items,
        max_datagram_queue_bytes: raw.max_datagram_queue_bytes as usize,
        max_inbound_i2np_queue: raw.max_inbound_i2np_queue,
        handshake_timeout: Duration::from_millis(raw.handshake_timeout_ms),
        idle_timeout: Duration::from_millis(raw.idle_timeout_ms),
        scheduler_poll_max: Duration::from_millis(raw.scheduler_poll_max_ms),
    })
}

fn validate_limit(field: &'static str, value: u64, maximum: u64) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError::Semantic {
            field,
            reason: "must be greater than zero",
        });
    }
    if value > maximum {
        return Err(ConfigError::Semantic {
            field,
            reason: "exceeds the bootstrap safety limit",
        });
    }
    Ok(())
}

fn normalize_ntcp2(raw: &RawNtcp2Config) -> Result<Ntcp2Config, ConfigError> {
    if raw.enabled {
        return Err(ConfigError::Semantic {
            field: "transport.ntcp2.enabled",
            reason: "normal-daemon NTCP2 activation is unavailable while support is experimental",
        });
    }

    let connect_timeout = Duration::from_millis(raw.connect_timeout_ms);
    let handshake_timeout = Duration::from_millis(raw.handshake_timeout_ms);
    let read_idle_timeout = Duration::from_millis(raw.read_idle_timeout_ms);
    let write_timeout = Duration::from_millis(raw.write_timeout_ms);
    let queue_wait_timeout = Duration::from_millis(raw.queue_wait_timeout_ms);
    let drain_timeout = Duration::from_millis(raw.drain_timeout_ms);

    validate_duration("transport.ntcp2.connect_timeout_ms", connect_timeout)?;
    validate_duration("transport.ntcp2.handshake_timeout_ms", handshake_timeout)?;
    validate_duration("transport.ntcp2.read_idle_timeout_ms", read_idle_timeout)?;
    validate_duration("transport.ntcp2.write_timeout_ms", write_timeout)?;
    validate_duration("transport.ntcp2.queue_wait_timeout_ms", queue_wait_timeout)?;
    validate_duration("transport.ntcp2.drain_timeout_ms", drain_timeout)?;

    if raw.max_active_links == 0 {
        return Err(ConfigError::Semantic {
            field: "transport.ntcp2.max_active_links",
            reason: "must be greater than zero",
        });
    }
    if raw.max_replay_entries == 0 {
        return Err(ConfigError::Semantic {
            field: "transport.ntcp2.max_replay_entries",
            reason: "must be greater than zero",
        });
    }
    if raw.ipv4_prefix > MAX_ALLOWED_PREFIX_IPV4 {
        return Err(ConfigError::Semantic {
            field: "transport.ntcp2.ipv4_prefix",
            reason: "exceeds maximum prefix width",
        });
    }
    if raw.ipv6_prefix > MAX_ALLOWED_PREFIX_IPV6 {
        return Err(ConfigError::Semantic {
            field: "transport.ntcp2.ipv6_prefix",
            reason: "exceeds maximum prefix width",
        });
    }

    Ok(Ntcp2Config {
        enabled: raw.enabled,
        connect_timeout,
        handshake_timeout,
        read_idle_timeout,
        write_timeout,
        queue_wait_timeout,
        drain_timeout,
        max_active_links: raw.max_active_links,
        max_replay_entries: raw.max_replay_entries,
        ipv4_prefix: raw.ipv4_prefix,
        ipv6_prefix: raw.ipv6_prefix,
    })
}

fn validate_duration(field: &'static str, value: Duration) -> Result<(), ConfigError> {
    if value.is_zero() {
        return Err(ConfigError::Semantic {
            field,
            reason: "must be greater than zero",
        });
    }
    if value > Duration::from_secs(MAX_ALLOWED_DURATION_SECS) {
        return Err(ConfigError::Semantic {
            field,
            reason: "exceeds the bootstrap safety limit",
        });
    }
    Ok(())
}

fn normalize_netdb(raw: &RawNetDbConfig) -> Result<NetDbConfig, ConfigError> {
    if raw.max_records == 0 {
        return Err(ConfigError::Semantic {
            field: "netdb.max_records",
            reason: "must be greater than zero",
        });
    }
    if raw.max_records > MAX_ALLOWED_NETDB_RECORDS {
        return Err(ConfigError::Semantic {
            field: "netdb.max_records",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.max_encoded_bytes == 0 {
        return Err(ConfigError::Semantic {
            field: "netdb.max_encoded_bytes",
            reason: "must be greater than zero",
        });
    }
    if raw.max_encoded_bytes > MAX_ALLOWED_NETDB_ENCODED_BYTES {
        return Err(ConfigError::Semantic {
            field: "netdb.max_encoded_bytes",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.min_router_infos > MAX_ALLOWED_BOOTSTRAP_RECORDS {
        return Err(ConfigError::Semantic {
            field: "netdb.min_router_infos",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.min_floodfill_advertisers > raw.min_router_infos {
        return Err(ConfigError::Semantic {
            field: "netdb.min_floodfill_advertisers",
            reason: "must not exceed netdb.min_router_infos",
        });
    }
    Ok(NetDbConfig {
        enabled: raw.enabled,
        max_records: raw.max_records as usize,
        max_encoded_bytes: raw.max_encoded_bytes as usize,
        min_router_infos: raw.min_router_infos as usize,
        min_floodfill_advertisers: raw.min_floodfill_advertisers as usize,
    })
}

fn normalize_floodfill(
    raw: &RawFloodfillConfig,
    ssu2: &Ssu2Config,
    netdb: &NetDbConfig,
) -> Result<FloodfillConfig, ConfigError> {
    // Intent without the serving substrate is a configuration error,
    // not a silent no-op: the operator must enable the SSU2 socket the
    // floodfill role serves on and the NetDB it serves. Eligibility
    // itself is still evaluated at runtime and may keep the role
    // Disabled (for example on a loopback-only bind, which can never
    // present a publicly reachable address).
    if raw.enabled && !ssu2.enabled {
        return Err(ConfigError::Semantic {
            field: "floodfill.enabled",
            reason: "floodfill cannot be enabled while ssu2.enabled is false",
        });
    }
    if raw.enabled && !netdb.enabled {
        return Err(ConfigError::Semantic {
            field: "floodfill.enabled",
            reason: "floodfill cannot be enabled while netdb.enabled is false",
        });
    }
    Ok(FloodfillConfig {
        enabled: raw.enabled,
    })
}

fn normalize_reseed(
    raw: &RawReseedConfig,
    netdb: &NetDbConfig,
) -> Result<ReseedConfig, ConfigError> {
    if raw.max_sources == 0 {
        return Err(ConfigError::Semantic {
            field: "reseed.max_sources",
            reason: "must be greater than zero",
        });
    }
    if raw.max_sources > MAX_ALLOWED_RESEED_SOURCES {
        return Err(ConfigError::Semantic {
            field: "reseed.max_sources",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.max_su3_bytes == 0 {
        return Err(ConfigError::Semantic {
            field: "reseed.max_su3_bytes",
            reason: "must be greater than zero",
        });
    }
    if raw.max_su3_bytes > MAX_ALLOWED_RESEED_BYTES {
        return Err(ConfigError::Semantic {
            field: "reseed.max_su3_bytes",
            reason: "exceeds the bootstrap safety limit",
        });
    }
    if raw.enabled && !netdb.enabled {
        return Err(ConfigError::Semantic {
            field: "reseed.enabled",
            reason: "reseed cannot be enabled while netdb.enabled is false",
        });
    }
    if raw.sources.len() > raw.max_sources {
        return Err(ConfigError::Semantic {
            field: "reseed.sources",
            reason: "exceeds reseed.max_sources",
        });
    }
    let mut sources = Vec::with_capacity(raw.sources.len());
    for (index, raw_source) in raw.sources.iter().enumerate() {
        if raw_source.signer_id.is_empty() {
            return Err(ConfigError::Semantic {
                field: "reseed.sources",
                reason: "signer identifier must not be empty",
            });
        }
        if raw_source.signer_id.len() > 256 {
            return Err(ConfigError::Semantic {
                field: "reseed.sources",
                reason: "signer identifier exceeds 256 bytes",
            });
        }
        if raw_source.certificate_path.is_empty() {
            return Err(ConfigError::Semantic {
                field: "reseed.sources",
                reason: "certificate path must not be empty",
            });
        }
        let path = PathBuf::from(&raw_source.certificate_path);
        sources.push(ReseedSourceConfig {
            signer_id: raw_source.signer_id.clone(),
            certificate_path: path,
        });
        let _ = index;
    }
    Ok(ReseedConfig {
        enabled: raw.enabled,
        max_sources: raw.max_sources,
        max_su3_bytes: raw.max_su3_bytes as usize,
        sources,
    })
}

fn normalize_news(raw: &RawNewsConfig) -> Result<NewsConfig, ConfigError> {
    if raw.max_su3_bytes == 0 || raw.max_su3_bytes > default_news_max_su3_bytes() {
        return Err(ConfigError::Semantic {
            field: "news.max_su3_bytes",
            reason: "must be between 1 byte and the 8 MiB NEWS ceiling",
        });
    }
    if !(60..=7 * 24 * 60 * 60).contains(&raw.refresh_interval_secs) {
        return Err(ConfigError::Semantic {
            field: "news.refresh_interval_secs",
            reason: "must be between 60 seconds and 7 days",
        });
    }
    let proxy_host = raw
        .proxy_host
        .parse::<IpAddr>()
        .map_err(|_| ConfigError::Semantic {
            field: "news.proxy_host",
            reason: "must be a loopback IP address",
        })?;
    if !proxy_host.is_loopback() || raw.proxy_port == 0 {
        return Err(ConfigError::Semantic {
            field: "news.proxy_host",
            reason: "the configured proxy must have a loopback address and nonzero port",
        });
    }
    let source_url = if raw.source_url.is_empty() {
        None
    } else {
        i2pr_addressbook::validate_subscription_url(&raw.source_url).map_err(|_| {
            ConfigError::Semantic {
                field: "news.source_url",
                reason: "must be a bounded HTTP(S) URL on an .i2p host",
            }
        })?;
        Some(raw.source_url.clone())
    };
    let signer_id = if raw.signer_id.is_empty() {
        None
    } else if raw.signer_id.len() > i2pr_su3::MAX_SIGNER_ID_BYTES
        || raw
            .signer_id
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(ConfigError::Semantic {
            field: "news.signer_id",
            reason: "must be a bounded printable signer identifier",
        });
    } else {
        Some(raw.signer_id.clone())
    };
    let certificate_path = if raw.certificate_path.is_empty() {
        None
    } else {
        Some(PathBuf::from(&raw.certificate_path))
    };
    if raw.enabled && (source_url.is_none() || signer_id.is_none() || certificate_path.is_none()) {
        return Err(ConfigError::Semantic {
            field: "news",
            reason: "enabled NEWS requires source_url, signer_id, and certificate_path",
        });
    }
    Ok(NewsConfig {
        enabled: raw.enabled,
        source_url,
        signer_id,
        certificate_path,
        proxy_host,
        proxy_port: raw.proxy_port,
        max_su3_bytes: raw.max_su3_bytes as usize,
        refresh_interval: Duration::from_secs(raw.refresh_interval_secs),
    })
}

/// A TOML decode failure whose rendered form never carries source content.
///
/// # Why this type exists
///
/// `toml::de::Error`'s own `Display` renders the **offending source line**:
/// `toml-1.1.6/src/de/error.rs` writes `line_num | ` followed by the whole
/// `content` of that line. Its `message()` is not a safe substitute either — a
/// `deny_unknown_fields` rejection embeds the rejected key *and* its value
/// (a `deny_unknown_fields` rejection renders the rejected key *and* its value).
///
/// So redaction cannot be a filter over the upstream message. This type keeps
/// only the diagnostic position, computed from the byte span, and drops the
/// content. An operator still learns **which line failed**; they do not learn
/// what was on it.
///
/// The underlying error is retained and returned by `Error::source`, so a caller
/// that legitimately needs the full text can still reach it. That is a
/// deliberate trade: the only current render path is `eprintln!("error: {e}")`
/// in `main.rs`, which formats the top-level `Display` and does **not** walk the
/// source chain. `scripts/check-config-secret-hygiene.sh` asserts that no
/// chain-walking printer (`{:#}`, `anyhow`, `+`) is added to this path.
pub struct RedactedTomlError {
    /// Line number, 1-based, when the span could be resolved.
    line: Option<usize>,
    /// Column number, 1-based, when the span could be resolved.
    column: Option<usize>,
    /// Retained only for `Error::source`; never rendered by this type.
    source: toml::de::Error,
}

impl RedactedTomlError {
    /// Wraps `error`, resolving its byte span to a line/column pair.
    ///
    /// `contents` is the exact source that was decoded; it is used only to
    /// count newlines and is never retained.
    pub fn new(error: toml::de::Error, contents: &str) -> Self {
        let (line, column) = match error.span() {
            Some(span) => position_of(contents, span.start),
            None => (None, None),
        };
        Self {
            line,
            column,
            source: error,
        }
    }

    /// Line the failure was anchored at, when known.
    pub const fn line(&self) -> Option<usize> {
        self.line
    }

    /// Column the failure was anchored at, when known.
    pub const fn column(&self) -> Option<usize> {
        self.column
    }
}

/// Resolves a byte offset to a 1-based `(line, column)` pair.
fn position_of(contents: &str, offset: usize) -> (Option<usize>, Option<usize>) {
    if offset > contents.len() {
        return (None, None);
    }
    let before = &contents.as_bytes()[..offset];
    let line = before.iter().filter(|b| **b == b'\n').count() + 1;
    let column = match before.iter().rposition(|b| *b == b'\n') {
        Some(index) => offset - index,
        None => offset + 1,
    };
    (Some(line), Some(column))
}

impl std::fmt::Display for RedactedTomlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(
                formatter,
                "TOML parse error at line {line}, column {column}"
            )?,
            _ => formatter.write_str("TOML parse error")?,
        }
        formatter.write_str(" [source content redacted]")
    }
}

impl std::fmt::Debug for RedactedTomlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, formatter)
    }
}

impl std::error::Error for RedactedTomlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// The Plan 352 D2 permission decision, deliberately independent of platform.
///
/// # Why this is not inline in the `cfg` branches
///
/// Reading a mode is platform-specific, but the *decision* is not. Keeping the
/// decision here means both outcomes — including the "no mode available" refusal
/// — are testable on any host, instead of the non-POSIX branch being dead code
/// that a Linux CI run can never execute. A `cfg`-gated row that cannot run on
/// the host is a row that has never failed, which is a comment, not a test.
///
/// `mode` is `None` exactly when the platform exposes no POSIX mode to check.
pub fn secret_file_permission_verdict(path: &Path, mode: Option<u32>) -> Result<(), ConfigError> {
    match mode {
        None => Err(ConfigError::InsecureConfigPermissionsUnsupported {
            path: path.to_path_buf(),
        }),
        Some(mode) if mode & 0o077 != 0 => Err(ConfigError::InsecureConfigPermissions {
            path: path.to_path_buf(),
            mode,
        }),
        Some(_) => Ok(()),
    }
}

/// Rejects a configuration file that holds a password but is readable by
/// group or other (Plan 352 D2).
///
/// Uses the same `& 0o077` idiom already enforced for every other secret file
/// in the tree (`i2pr-storage/src/lib.rs:1083`,
/// `i2pr-daemon/src/i2pcontrol_tunnels.rs:1022`). The config was the one
/// secret-bearing file with no gate.
#[cfg(unix)]
fn check_secret_file_permissions(path: &Path) -> Result<(), super::error::DaemonError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata =
        fs::metadata(path).map_err(|source| super::error::DaemonError::ConfigUnavailable {
            path: path.to_path_buf(),
            source,
        })?;
    let mode = metadata.permissions().mode() & 0o7777;
    secret_file_permission_verdict(path, Some(mode)).map_err(super::error::DaemonError::from)
}

/// Non-POSIX platforms have no mode to check, and Plan 352 forbids *silently
/// passing*. So a password-bearing config is refused outright there, naming the
/// file, rather than accepted on the assumption that an unexamined platform is
/// safe.
///
/// This is a real behaviour change on Windows: enabling `[i2pcontrol]` with a
/// password now requires the secret to move out of the config (see
/// `outbound_secret.rs` for the sealing mechanism) or the platform gate to be
/// designed. It is recorded as such in the Plan 352 closure record.
#[cfg(not(unix))]
fn check_secret_file_permissions(path: &Path) -> Result<(), super::error::DaemonError> {
    secret_file_permission_verdict(path, None).map_err(super::error::DaemonError::from)
}

/// Configuration parse and semantic-validation failures.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// TOML syntax or schema decoding failed.
    ///
    /// The rendered form is position-only; see [`RedactedTomlError`].
    #[error("configuration parse failed: {0}")]
    Parse(#[source] RedactedTomlError),
    /// The file used a schema version not understood by this binary.
    #[error("unsupported schema_version {actual}; expected {CURRENT_SCHEMA_VERSION}")]
    UnsupportedSchemaVersion { actual: u64 },
    /// A decoded field violated a semantic invariant.
    #[error("invalid {field}: {reason}")]
    Semantic {
        /// Dot-separated configuration field.
        field: &'static str,
        /// Bounded reason suitable for human diagnostics.
        reason: &'static str,
    },
    /// The file holds a password but is readable by group or other.
    ///
    /// Carries the **path** (not a secret) and the observed mode, so the
    /// operator is told what to fix without any file content being echoed.
    #[error(
        "configuration file {path} holds a password and is group- or world-readable \
         (mode {mode:o}); expected 0600"
    )]
    InsecureConfigPermissions {
        /// The offending configuration file.
        path: PathBuf,
        /// Observed POSIX permission bits.
        mode: u32,
    },
    /// The file holds a password on a platform with no POSIX mode to check.
    ///
    /// Recorded rather than silently passing: refusing is the only direction
    /// that cannot be mistaken for a check that succeeded.
    #[error(
        "configuration file {path} holds a password but this platform exposes no POSIX \
         file mode to check; refusing to accept a secret-bearing configuration without a \
         permission gate"
    )]
    InsecureConfigPermissionsUnsupported {
        /// The offending configuration file.
        path: PathBuf,
    },
}

impl ConfigError {
    /// Maps the failure to the stable daemon exit-code category.
    pub const fn exit_code(&self) -> super::error::ExitCode {
        match self {
            Self::Parse(_) | Self::UnsupportedSchemaVersion { .. } => {
                super::error::ExitCode::ConfigParse
            }
            Self::Semantic { .. } | Self::InsecureConfigPermissions { .. } => {
                super::error::ExitCode::ConfigSemantic
            }
            Self::InsecureConfigPermissionsUnsupported { .. } => {
                super::error::ExitCode::ConfigSemantic
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    const VALID: &str = r#"
schema_version = 1

[router]
data_dir = "./state"
profile = "balanced"

[logging]
filter = "info"
format = "text"

[limits]
max_tasks = 16
max_buffered_bytes = 67108864

[network]
bind_address = "127.0.0.1"
listen_port = 9150
network_id = 2

[transport.ntcp2]
enabled = false
connect_timeout_ms = 5000
handshake_timeout_ms = 30000
read_idle_timeout_ms = 120000
write_timeout_ms = 30000
queue_wait_timeout_ms = 5000
drain_timeout_ms = 5000
max_active_links = 128
max_replay_entries = 256
ipv4_prefix = 24
ipv6_prefix = 64
"#;

    const MINIMAL: &str = r#"
schema_version = 1

[router]
data_dir = "./state"
"#;

    #[test]
    fn valid_config_normalizes_without_creating_data_dir() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid defaults");
        assert_eq!(config.limits.max_tasks, DEFAULT_MAX_TASKS);
        assert_eq!(config.logging.format, LogFormat::Text);
        assert!(!path.exists());
    }

    #[test]
    fn signed_news_is_disabled_by_default_and_requires_complete_explicit_trust() {
        let default = Config::parse(MINIMAL).expect("default config");
        assert!(!default.news.enabled);
        assert_eq!(default.news.source_url, None);
        assert_eq!(default.news.signer_id, None);
        let incomplete = format!(
            "{MINIMAL}\n[news]\nenabled = true\nsource_url = \"http://news.i2p/feed.su3\"\n"
        );
        assert!(matches!(
            Config::parse(&incomplete),
            Err(ConfigError::Semantic { field: "news", .. })
        ));
        let complete = format!(
            "{MINIMAL}\n[news]\nenabled = true\nsource_url = \"https://news.i2p/feed.su3\"\nsigner_id = \"router-news\"\ncertificate_path = \"./news-signers/router.der\"\n"
        );
        let configured = Config::parse(&complete).expect("explicit NEWS config");
        assert!(configured.news.enabled);
        assert_eq!(
            configured.news.source_url.as_deref(),
            Some("https://news.i2p/feed.su3")
        );
    }

    #[test]
    fn signed_news_rejects_clearnet_sources_and_non_loopback_proxies() {
        let clearnet = format!(
            "{MINIMAL}\n[news]\nenabled = true\nsource_url = \"https://news.example/feed.su3\"\nsigner_id = \"router-news\"\ncertificate_path = \"./router.der\"\n"
        );
        assert!(matches!(
            Config::parse(&clearnet),
            Err(ConfigError::Semantic {
                field: "news.source_url",
                ..
            })
        ));
        let public_proxy = format!("{MINIMAL}\n[news]\nproxy_host = \"0.0.0.0\"\n");
        assert!(matches!(
            Config::parse(&public_proxy),
            Err(ConfigError::Semantic {
                field: "news.proxy_host",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_are_rejected_at_each_level() {
        let root = format!("{VALID}\nunknown = true\n");
        assert!(matches!(Config::parse(&root), Err(ConfigError::Parse(_))));
        let nested = format!("{VALID}\n[limits]\nunknown = true\n");
        assert!(matches!(Config::parse(&nested), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn semantic_validation_identifies_bad_values() {
        let invalid = VALID.replace("max_tasks = 16", "max_tasks = 0");
        assert!(matches!(
            Config::parse(&invalid),
            Err(ConfigError::Semantic {
                field: "limits.max_tasks",
                ..
            })
        ));
        let unsupported = VALID.replace("schema_version = 1", "schema_version = 2");
        assert!(matches!(
            Config::parse(&unsupported),
            Err(ConfigError::UnsupportedSchemaVersion { actual: 2 })
        ));
    }

    #[test]
    fn sam_aggregate_limits_cannot_exceed_router_budgets() {
        let too_few_tasks = VALID.replace("max_tasks = 16", "max_tasks = 15");
        assert!(matches!(
            Config::parse(&too_few_tasks),
            Err(ConfigError::Semantic {
                field: "sam.aggregate",
                ..
            })
        ));

        let too_few_bytes =
            VALID.replace("max_buffered_bytes = 67108864", "max_buffered_bytes = 1024");
        assert!(matches!(
            Config::parse(&too_few_bytes),
            Err(ConfigError::Semantic {
                field: "sam.aggregate",
                ..
            })
        ));
    }

    /// Plan 287 `[i2pcontrol]` defaults: disabled, loopback, no password.
    const I2PCONTROL_BASE: &str = "schema_version = 1\n[router]\ndata_dir = \"state\"\n";

    #[test]
    fn i2pcontrol_defaults_are_disabled_loopback_and_passwordless() {
        let config = Config::parse(I2PCONTROL_BASE).expect("defaults parse");
        assert!(!config.i2pcontrol.enabled);
        assert_eq!(config.i2pcontrol.bind_socket().port(), 7650);
        assert!(config.i2pcontrol.is_loopback_bind());
        assert!(!config.i2pcontrol.has_explicit_tls());
        assert!(config.i2pcontrol.password.is_empty());
        // The redacted password never leaks through Debug.
        assert!(format!("{:?}", config.i2pcontrol).contains("[redacted]"));
    }

    #[test]
    fn i2pcontrol_enabled_requires_a_password() {
        let text = format!("{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "i2pcontrol.password",
                ..
            })
        ));
        let text =
            format!("{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\npassword = \"operator\"\n");
        let config = Config::parse(&text).expect("password enables");
        assert_eq!(config.i2pcontrol.password.as_str(), "operator");
    }

    #[test]
    fn i2pcontrol_non_loopback_requires_explicit_tls() {
        let text = format!(
            "{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\nbind_address = \"0.0.0.0\"\npassword = \"operator\"\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "i2pcontrol.bind_address",
                ..
            })
        ));
        let half = format!(
            "{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\npassword = \"operator\"\ncertificate = \"/tmp/c.pem\"\n"
        );
        assert!(matches!(
            Config::parse(&half),
            Err(ConfigError::Semantic {
                field: "i2pcontrol.certificate",
                ..
            })
        ));
        // Complete explicit material parses (loading happens at service
        // construction, before bind).
        let full = format!(
            "{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\nbind_address = \"0.0.0.0\"\npassword = \"operator\"\ncertificate = \"/tmp/c.pem\"\nprivate_key = \"/tmp/k.pem\"\n"
        );
        let config = Config::parse(&full).expect("explicit TLS parses");
        assert!(config.i2pcontrol.has_explicit_tls());
        assert!(!config.i2pcontrol.is_loopback_bind());
    }

    #[test]
    fn i2pcontrol_resource_knobs_respect_hard_maxima() {
        let text = format!(
            "{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\npassword = \"operator\"\nmax_connections = 257\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "i2pcontrol.max_connections",
                ..
            })
        ));
        let text = format!(
            "{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\npassword = \"operator\"\nmax_body_bytes = 1048577\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "i2pcontrol.max_body_bytes",
                ..
            })
        ));
        let text = format!(
            "{I2PCONTROL_BASE}[i2pcontrol]\nenabled = true\npassword = \"operator\"\nrequest_deadline_ms = 61\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "i2pcontrol.request_deadline_ms",
                ..
            })
        ));
    }

    #[test]
    fn existing_data_file_is_rejected_without_mutation() {
        let directory = tempdir().expect("temp directory");
        let file = directory.path().join("file");
        fs::write(&file, b"fixture").expect("write fixture");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            file.to_string_lossy()
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "router.data_dir",
                ..
            })
        ));
        assert_eq!(fs::read(&file).expect("fixture remains"), b"fixture");
    }

    #[test]
    fn valid_config_with_defaults_for_network_and_transport() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid defaults");
        assert_eq!(
            config.network.bind_address,
            "0.0.0.0".parse::<IpAddr>().unwrap()
        );
        assert_eq!(config.network.listen_port, 9150);
        assert_eq!(config.network.network_id, 2);
        assert!(!config.transport.ntcp2.enabled);
        assert_eq!(
            config.transport.ntcp2.connect_timeout,
            Duration::from_millis(5000)
        );
        assert_eq!(config.transport.ntcp2.max_active_links, 128);
        assert_eq!(config.transport.ntcp2.ipv4_prefix, 24);
    }

    #[test]
    fn network_config_validates_bind_address() {
        let text = format!("{}\n[network]\nbind_address = \"not-an-ip\"\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "network.bind_address",
                ..
            })
        ));
    }

    #[test]
    fn network_config_rejects_zero_port() {
        let text = format!("{}\n[network]\nlisten_port = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "network.listen_port",
                ..
            })
        ));
    }

    #[test]
    fn network_config_rejects_zero_network_id() {
        let text = format!("{}\n[network]\nnetwork_id = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "network.network_id",
                ..
            })
        ));
    }

    #[test]
    fn ntcp2_config_rejects_zero_connect_timeout() {
        let text = format!("{}\n[transport.ntcp2]\nconnect_timeout_ms = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "transport.ntcp2.connect_timeout_ms",
                ..
            })
        ));
    }

    #[test]
    fn ntcp2_config_rejects_zero_max_active_links() {
        let text = format!("{}\n[transport.ntcp2]\nmax_active_links = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "transport.ntcp2.max_active_links",
                ..
            })
        ));
    }

    #[test]
    fn ntcp2_config_rejects_invalid_ipv4_prefix() {
        let text = format!("{}\n[transport.ntcp2]\nipv4_prefix = 33\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "transport.ntcp2.ipv4_prefix",
                ..
            })
        ));
    }

    #[test]
    fn ntcp2_config_rejects_invalid_ipv6_prefix() {
        let text = format!("{}\n[transport.ntcp2]\nipv6_prefix = 129\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "transport.ntcp2.ipv6_prefix",
                ..
            })
        ));
    }

    #[test]
    fn unknown_network_fields_are_rejected() {
        let text = format!("{}\n[network]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn unknown_transport_fields_are_rejected() {
        let text = format!("{}\n[transport]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn unknown_ntcp2_fields_are_rejected() {
        let text = format!("{}\n[transport.ntcp2]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn network_listen_socket_is_computed_correctly() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[network]\nbind_address = \"127.0.0.1\"\nlisten_port = 9150\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid config");
        assert_eq!(
            config.network.listen_socket(),
            "127.0.0.1:9150".parse().unwrap()
        );
    }

    #[test]
    fn omitted_ntcp2_section_means_disabled() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid config");
        assert!(!config.transport.ntcp2.enabled);
    }

    #[test]
    fn explicit_ntcp2_enabled_false_is_accepted() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[transport.ntcp2]\nenabled = false\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("explicit false should be accepted");
        assert!(!config.transport.ntcp2.enabled);
    }

    #[test]
    fn explicit_ntcp2_enabled_true_is_rejected() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[transport.ntcp2]\nenabled = true\n",
            path.to_string_lossy()
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "transport.ntcp2.enabled",
                ..
            })
        ));
    }

    #[test]
    fn ntcp2_tuning_fields_do_not_activate_when_disabled() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[transport.ntcp2]\nconnect_timeout_ms = 1000\nhandshake_timeout_ms = 5000\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("tuning without enabled should be accepted");
        assert!(!config.transport.ntcp2.enabled);
    }

    #[test]
    fn netdb_defaults_apply_when_section_is_omitted() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid defaults");
        assert!(config.netdb.enabled);
        assert_eq!(config.netdb.max_records, 4_096);
        assert_eq!(config.netdb.max_encoded_bytes, 4 * 1024 * 1024);
        assert_eq!(config.netdb.min_router_infos, 50);
        assert_eq!(config.netdb.min_floodfill_advertisers, 5);
    }

    #[test]
    fn netdb_rejects_zero_max_records() {
        let text = format!("{}\n[netdb]\nmax_records = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "netdb.max_records",
                ..
            })
        ));
    }

    #[test]
    fn netdb_rejects_excessive_max_records() {
        let text = format!("{}\n[netdb]\nmax_records = 100000\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "netdb.max_records",
                ..
            })
        ));
    }

    #[test]
    fn netdb_rejects_zero_max_encoded_bytes() {
        let text = format!("{}\n[netdb]\nmax_encoded_bytes = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "netdb.max_encoded_bytes",
                ..
            })
        ));
    }

    #[test]
    fn netdb_rejects_floodfill_min_exceeding_record_min() {
        let text = format!(
            "{}\n[netdb]\nmin_router_infos = 10\nmin_floodfill_advertisers = 20\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "netdb.min_floodfill_advertisers",
                ..
            })
        ));
    }

    #[test]
    fn reseed_defaults_apply_when_section_is_omitted() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid defaults");
        assert!(!config.reseed.enabled);
        assert_eq!(config.reseed.max_sources, 4);
        assert_eq!(config.reseed.max_su3_bytes, 8 * 1024 * 1024);
        assert!(config.reseed.sources.is_empty());
    }

    #[test]
    fn reseed_rejects_zero_max_sources() {
        let text = format!("{}\n[reseed]\nmax_sources = 0\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "reseed.max_sources",
                ..
            })
        ));
    }

    #[test]
    fn reseed_rejects_excessive_max_su3_bytes() {
        let text = format!("{}\n[reseed]\nmax_su3_bytes = 33554432\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "reseed.max_su3_bytes",
                ..
            })
        ));
    }

    #[test]
    fn reseed_rejects_enable_when_netdb_disabled() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[netdb]\nenabled = false\n[reseed]\nenabled = true\n",
            path.to_string_lossy()
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "reseed.enabled",
                ..
            })
        ));
    }

    #[test]
    fn reseed_rejects_too_many_configured_sources() {
        let text = format!(
            "{}\n[reseed]\nmax_sources = 2\n[[reseed.sources]]\nsigner_id = \"a\"\ncertificate_path = \"x.pem\"\n[[reseed.sources]]\nsigner_id = \"b\"\ncertificate_path = \"y.pem\"\n[[reseed.sources]]\nsigner_id = \"c\"\ncertificate_path = \"z.pem\"\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "reseed.sources",
                ..
            })
        ));
    }

    #[test]
    fn reseed_rejects_empty_signer_id() {
        let text = format!(
            "{}\n[reseed]\nenabled = true\n[[reseed.sources]]\nsigner_id = \"\"\ncertificate_path = \"x.pem\"\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "reseed.sources",
                ..
            })
        ));
    }

    #[test]
    fn reseed_accepts_valid_source_entry() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let cert = directory.path().join("signer.pem");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[reseed]\nenabled = true\n[[reseed.sources]]\nsigner_id = \"trusted\"\ncertificate_path = {:?}\n",
            path.to_string_lossy(),
            cert.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid reseed source");
        assert!(config.reseed.enabled);
        assert_eq!(config.reseed.sources.len(), 1);
        assert_eq!(config.reseed.sources[0].signer_id, "trusted");
        assert_eq!(config.reseed.sources[0].certificate_path, cert);
    }

    #[test]
    fn unknown_netdb_fields_are_rejected() {
        let text = format!("{}\n[netdb]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn unknown_reseed_fields_are_rejected() {
        let text = format!("{}\n[reseed]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn ssu2_defaults_are_disabled_loopback_and_non_advertised() {
        let config = Config::parse(MINIMAL).expect("valid defaults");
        assert!(!config.ssu2.enabled);
        assert_eq!(
            config.ssu2.bind_ipv4,
            Some("127.0.0.1".parse::<IpAddr>().expect("loopback"))
        );
        assert_eq!(config.ssu2.bind_ipv6, None);
        assert_eq!(config.ssu2.port, 0);
        assert!(!config.ssu2.advertise);
        assert!(!config.ssu2.introducer_service);
        assert_eq!(config.ssu2.max_pending_handshakes, 64);
        assert_eq!(config.ssu2.max_active_sessions, 64);
        assert_eq!(config.ssu2.max_pending_per_ip, 4);
        assert_eq!(config.ssu2.max_pending_per_subnet, 16);
        assert_eq!(config.ssu2.handshake_timeout, Duration::from_millis(20_000));
        assert_eq!(config.ssu2.idle_timeout, Duration::from_millis(300_000));
    }

    #[test]
    fn ssu2_enabled_true_accepts_strict_loopback_profile() {
        // Plan 184 §3: `enabled = true` is accepted only with loopback
        // bind, non-advertised, no introducer service.
        let text = format!("{}\n[ssu2]\nenabled = true\n", MINIMAL);
        let config = Config::parse(&text).expect("strict controlled profile");
        assert!(config.ssu2.enabled);
        assert!(!config.ssu2.advertise);
        assert!(!config.ssu2.introducer_service);
        assert!(config.ssu2.bind_ipv4.is_some_and(|ip| ip.is_loopback()));
    }

    #[test]
    fn ssu2_enabled_true_requires_loopback_bind() {
        // Both families disabled while enabled must fail closed.
        let text = format!(
            "{}\n[ssu2]\nenabled = true\nbind_ipv4 = \"\"\nbind_ipv6 = \"\"\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "ssu2.enabled",
                ..
            })
        ));
    }

    #[test]
    fn ssu2_enabled_true_rejects_broad_exposure() {
        // advertise/introducer stay rejected even under the controlled
        // profile; no wildcard/non-loopback bind is reachable because
        // `parse_ssu2_bind` rejects it fail-closed.
        for extra in [
            "advertise = true",
            "introducer_service = true",
            "bind_ipv4 = \"0.0.0.0\"",
            "bind_ipv4 = \"192.0.2.1\"",
        ] {
            let text = format!("{}\n[ssu2]\nenabled = true\n{}\n", MINIMAL, extra);
            assert!(
                Config::parse(&text).is_err(),
                "broad exposure `{extra}` must be rejected"
            );
        }
    }

    #[test]
    fn floodfill_defaults_to_disabled_opt_in() {
        // Plan 279 §4A: normal floodfill is default-off. A minimal
        // config carries no intent, so the role can never activate.
        let config = Config::parse(MINIMAL).expect("valid defaults");
        assert!(!config.floodfill.enabled);
    }

    #[test]
    fn floodfill_enabled_true_is_intent_not_authority() {
        // Plan 279 §4B: explicit opt-in parses on the strict
        // loopback profile. It requests eligibility evaluation; the
        // runtime still decides (a loopback bind can never present a
        // publicly reachable address, so the role stays Disabled).
        let text = format!(
            "{}\n[ssu2]\nenabled = true\n[floodfill]\nenabled = true\n",
            MINIMAL
        );
        let config = Config::parse(&text).expect("explicit opt-in parses");
        assert!(config.floodfill.enabled);
    }

    #[test]
    fn floodfill_enabled_true_requires_serving_substrate() {
        // Intent without the SSU2 socket or the NetDB it would serve
        // is a configuration error, not a silent no-op.
        let no_ssu2 = format!("{}\n[floodfill]\nenabled = true\n", MINIMAL);
        assert!(matches!(
            Config::parse(&no_ssu2),
            Err(ConfigError::Semantic {
                field: "floodfill.enabled",
                ..
            })
        ));
        let no_netdb = format!(
            "{}\n[ssu2]\nenabled = true\n[netdb]\nenabled = false\n[floodfill]\nenabled = true\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&no_netdb),
            Err(ConfigError::Semantic {
                field: "floodfill.enabled",
                ..
            })
        ));
    }

    #[test]
    fn floodfill_rejects_unknown_fields() {
        // Plan 279 §4E: a typo'd reload/start config fails before any
        // listener or state mutates (parse is pure and total).
        let text = format!("{}\n[floodfill]\nenabled = true\nauto = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn ssu2_advertise_and_introducer_are_rejected() {
        let text = format!("{}\n[ssu2]\nadvertise = true\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "ssu2.advertise",
                ..
            })
        ));
        let text = format!("{}\n[ssu2]\nintroducer_service = true\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "ssu2.introducer_service",
                ..
            })
        ));
    }

    #[test]
    fn ssu2_bind_fields_reject_non_loopback_and_wrong_family() {
        for (field, value) in [
            ("bind_ipv4", "\"192.0.2.1\""),
            ("bind_ipv4", "\"::1\""),
            ("bind_ipv6", "\"2001:db8::1\""),
            ("bind_ipv6", "\"127.0.0.1\""),
            ("bind_ipv4", "\"not-an-ip\""),
        ] {
            let text = format!("{}\n[ssu2]\n{} = {}\n", MINIMAL, field, value);
            assert!(
                matches!(Config::parse(&text), Err(ConfigError::Semantic { .. })),
                "field {field} value {value} must be rejected"
            );
        }
        // Empty disables the family; loopback ::1 enables IPv6 tests.
        let text = format!(
            "{}\n[ssu2]\nbind_ipv4 = \"\"\nbind_ipv6 = \"::1\"\n",
            MINIMAL
        );
        let config = Config::parse(&text).expect("family disable/enable");
        assert_eq!(config.ssu2.bind_ipv4, None);
        assert_eq!(
            config.ssu2.bind_ipv6,
            Some("::1".parse::<IpAddr>().expect("loopback"))
        );
    }

    #[test]
    fn ssu2_port_zero_is_ephemeral_and_low_ports_rejected() {
        let text = format!("{}\n[ssu2]\nport = 0\n", MINIMAL);
        assert_eq!(Config::parse(&text).expect("ephemeral").ssu2.port, 0);
        let text = format!("{}\n[ssu2]\nport = 9150\n", MINIMAL);
        assert_eq!(Config::parse(&text).expect("service").ssu2.port, 9150);
        let text = format!("{}\n[ssu2]\nport = 80\n", MINIMAL);
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "ssu2.port",
                ..
            })
        ));
    }

    #[test]
    fn ssu2_limits_and_timeouts_enforce_ceilings() {
        for (field, value) in [
            ("max_pending_handshakes", "0"),
            ("max_pending_handshakes", "1025"),
            ("max_active_sessions", "0"),
            ("max_pending_per_ip", "65"),
            ("max_pending_per_subnet", "257"),
            ("max_datagram_queue_items", "4097"),
            ("max_datagram_queue_bytes", "134217729"),
            ("max_inbound_i2np_queue", "0"),
            ("handshake_timeout_ms", "4999"),
            ("handshake_timeout_ms", "60001"),
            ("idle_timeout_ms", "0"),
            ("scheduler_poll_max_ms", "9"),
            ("scheduler_poll_max_ms", "1001"),
        ] {
            let text = format!("{}\n[ssu2]\n{} = {}\n", MINIMAL, field, value);
            assert!(
                Config::parse(&text).is_err(),
                "field {field} value {value} must be rejected"
            );
        }
        // Per-scope ceilings must not exceed the global pending ceiling.
        let text = format!(
            "{}\n[ssu2]\nmax_pending_handshakes = 4\nmax_pending_per_ip = 5\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic {
                field: "ssu2.pending_scopes",
                ..
            })
        ));
    }

    #[test]
    fn unknown_ssu2_fields_are_rejected() {
        let text = format!("{}\n[ssu2]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    // --- Plan 174 service-tunnel foundation ---

    fn service_b32() -> String {
        format!("{}.b32.i2p", "a".repeat(52))
    }

    #[test]
    fn service_tunnels_disabled_by_default() {
        let config = Config::parse(MINIMAL).expect("valid defaults");
        assert!(!config.service_tunnels.enabled);
        assert!(config.service_tunnels.tunnels.is_empty());
        assert!(config.service_tunnels.aliases.is_empty());
    }

    #[test]
    fn unknown_service_tunnels_fields_are_rejected() {
        let text = format!("{}\n[service_tunnels]\nunknown = true\n", MINIMAL);
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"a\"\nkind = \"generic-client\"\nunknown = true\n",
            MINIMAL
        );
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn non_loopback_local_listener_rejected() {
        for listener in ["0.0.0.0:8080", "192.168.1.10:8080", "8.8.8.8:53"] {
            let text = format!(
                "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nlistener = \"{}\"\ndestination = \"{}\"\n",
                MINIMAL,
                listener,
                service_b32()
            );
            assert!(
                matches!(
                    Config::parse(&text),
                    Err(ConfigError::Semantic {
                        field: "service_tunnels.tunnel.listener",
                        ..
                    })
                ),
                "listener {listener} must be rejected"
            );
        }
        // Loopback listeners validate structurally (disabled tunnels
        // are accepted; enabled tunnels are rejected as
        // not-yet-available in a separate test).
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nlistener = \"127.0.0.1:8080\"\ndestination = \"{}\"\n",
            MINIMAL,
            service_b32()
        );
        Config::parse(&text).expect("loopback listener validates");
    }

    #[test]
    fn non_loopback_server_tcp_target_rejected() {
        for target in ["192.168.1.10:9090", "0.0.0.0:9090"] {
            let text = format!(
                "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\ntarget = \"{}\"\n",
                MINIMAL, target
            );
            assert!(
                matches!(
                    Config::parse(&text),
                    Err(ConfigError::Semantic {
                        field: "service_tunnels.tunnel.target",
                        ..
                    })
                ),
                "target {target} must be rejected"
            );
        }
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\ntarget = \"127.0.0.1:9090\"\n",
            MINIMAL
        );
        Config::parse(&text).expect("loopback target validates");
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\ntarget = \"unix:/run/i2pr/service.sock\"\n",
            MINIMAL
        );
        Config::parse(&text).expect("unix target validates as a path value");
    }

    #[test]
    fn service_group_config_applies_to_server_destinations() {
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"web\"\nkind = \"generic-server\"\ntarget = \"127.0.0.1:9090\"\ngroup = \"public-services\"\ninbound_port = 80\n",
            MINIMAL
        );
        let config = Config::parse(&text).expect("explicit server group is valid");
        assert!(matches!(
            &config.service_tunnels.tunnels.tunnels[0].policy,
            i2pr_service_tunnels::DestinationPolicy::SharedGroup(group)
                if group.as_str() == "public-services"
        ));
    }

    #[test]
    fn enabled_service_tunnel_accepted_for_generic_client() {
        // Plan 175 enables generic-client and generic-server; only
        // HTTP/SOCKS/IRC remain not-yet-available.
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nenabled = true\nlistener = \"127.0.0.1:8080\"\ndestination = \"{}\"\n",
            MINIMAL,
            service_b32()
        );
        let config = Config::parse(&text).expect("generic-client must accept");
        assert_eq!(config.service_tunnels.tunnels.len(), 1);
        assert!(config.service_tunnels.tunnels.tunnels[0].enabled);
    }

    #[test]
    fn enabled_non_generic_service_tunnel_rejected_as_not_yet_available() {
        // Plan 179: irc-server is now accepted. There is no
        // further kind kept behind the not-yet-available gate;
        // a hypothetical kind remains rejected so the gate test
        // stays meaningful.
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"web-server\"\nenabled = true\ntarget = \"127.0.0.1:9090\"\n",
            MINIMAL
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
    }

    #[test]
    fn enabled_http_client_service_tunnel_is_accepted() {
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"http-client\"\nenabled = true\nlistener = \"127.0.0.1:8080\"\ndestination = \"{}\"\n",
            MINIMAL,
            service_b32()
        );
        let config = Config::parse(&text).expect("http-client must accept");
        assert!(config.service_tunnels.tunnels.tunnels[0].enabled);
    }

    #[test]
    fn enabled_socks5_client_service_tunnel_is_accepted() {
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"socks5-client\"\nenabled = true\nlistener = \"127.0.0.1:8080\"\ndestination = \"{}\"\n",
            MINIMAL,
            service_b32()
        );
        let config = Config::parse(&text).expect("socks5-client must accept");
        assert!(config.service_tunnels.tunnels.tunnels[0].enabled);
    }

    #[test]
    fn enabled_irc_client_service_tunnel_is_accepted() {
        // Plan 178 enables irc-client as a real listener kind.
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"irc-client\"\nenabled = true\nlistener = \"127.0.0.1:8080\"\ndestination = \"{}\"\n",
            MINIMAL,
            service_b32()
        );
        let config = Config::parse(&text).expect("irc-client must accept");
        assert!(config.service_tunnels.tunnels.tunnels[0].enabled);
    }

    #[test]
    fn enabled_irc_server_service_tunnel_is_accepted() {
        // Plan 179 enables irc-server as a real server kind.
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"irc-server\"\nenabled = true\ntarget = \"127.0.0.1:9090\"\n",
            MINIMAL
        );
        let config = Config::parse(&text).expect("irc-server must accept");
        assert!(config.service_tunnels.tunnels.tunnels[0].enabled);
    }

    #[test]
    fn duplicate_service_ids_and_listeners_rejected() {
        let b32 = service_b32();
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nlistener = \"127.0.0.1:8080\"\ndestination = \"{b32}\"\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nlistener = \"127.0.0.1:8081\"\ndestination = \"{b32}\"\n",
            MINIMAL
        );
        assert!(Config::parse(&text).is_err());
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nlistener = \"127.0.0.1:8080\"\ndestination = \"{b32}\"\n[[service_tunnels.tunnel]]\nid = \"beta\"\nkind = \"generic-client\"\nlistener = \"127.0.0.1:8080\"\ndestination = \"{b32}\"\n",
            MINIMAL
        );
        assert!(Config::parse(&text).is_err());
    }

    #[test]
    fn service_tunnel_bounds_enforced() {
        // Aggregate ceiling.
        let text = format!(
            "{}\n[service_tunnels]\nmax_active_connections = 0\n",
            MINIMAL
        );
        assert!(Config::parse(&text).is_err());
        let text = format!(
            "{}\n[service_tunnels]\nmax_active_connections = 1025\n",
            MINIMAL
        );
        assert!(Config::parse(&text).is_err());
        // Per-direction buffered bytes.
        let text = format!(
            "{}\n[service_tunnels]\nmax_buffered_bytes_per_direction = 0\n",
            MINIMAL
        );
        assert!(Config::parse(&text).is_err());
        // Unknown kind.
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"http-server\"\n",
            MINIMAL
        );
        assert!(Config::parse(&text).is_err());
    }

    #[test]
    fn addressbook_section_defaults_to_disabled() {
        let config = Config::parse(MINIMAL).expect("minimal parses");
        assert!(!config.addressbook.enabled);
        assert_eq!(
            config.addressbook.state_dir,
            config.router.data_dir.join("addressbook")
        );
    }

    #[test]
    fn addressbook_section_resolves_state_dir() {
        // Explicit relative directories resolve under the data dir.
        let text = format!("{MINIMAL}\n[addressbook]\nenabled = true\nstate_dir = \"books\"\n");
        let config = Config::parse(&text).expect("relative dir");
        assert!(config.addressbook.enabled);
        assert_eq!(
            config.addressbook.state_dir,
            config.router.data_dir.join("books")
        );
        // Absolute directories pass through.
        let text =
            format!("{MINIMAL}\n[addressbook]\nenabled = true\nstate_dir = \"/tmp/abs-books\"\n");
        let config = Config::parse(&text).expect("absolute dir");
        assert_eq!(
            config.addressbook.state_dir,
            PathBuf::from("/tmp/abs-books")
        );
        // Empty and NUL-bearing directories fail shape validation.
        let text = format!("{MINIMAL}\n[addressbook]\nstate_dir = \"   \"\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
        // Unknown keys fail closed.
        let text = format!("{MINIMAL}\n[addressbook]\nfetch_command = \"curl\"\n");
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    /// Writes a fresh self-signed PEM identity pair into the
    /// directory for TLS policy tests, returning the paths plus a
    /// pinned-certificates bundle holding the same certificate.
    fn write_tls_identity(dir: &std::path::Path) -> (PathBuf, PathBuf, PathBuf) {
        let certified =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).expect("fixture cert");
        let cert_path = dir.join("tls-cert.pem");
        let key_path = dir.join("tls-key.pem");
        let pins_path = dir.join("tls-pins.pem");
        std::fs::write(&cert_path, certified.cert.pem().as_bytes()).expect("write cert");
        std::fs::write(&key_path, certified.key_pair.serialize_pem().as_bytes())
            .expect("write key");
        std::fs::write(&pins_path, certified.cert.pem().as_bytes()).expect("write pins");
        (cert_path, key_path, pins_path)
    }

    #[test]
    fn service_tls_policy_parses_pins_identity_and_rejections() {
        let directory = tempdir().expect("temp directory");
        let (cert_path, key_path, pins_path) = write_tls_identity(directory.path());
        // Pinned certificates alone build a verifying policy.
        let text = format!(
            "{MINIMAL}\n[service_tunnels.tls]\npinned_certificates_path = {:?}\n",
            pins_path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("pin policy parses");
        let policy = config.service_tunnels.tls_policy.expect("policy present");
        assert_eq!(policy.policy().verify_mode(), "pin");
        assert!(policy.policy().identity_expires_unix().is_none());
        // A provisioned identity loads with a real expiry.
        let text = format!(
            "{MINIMAL}\n[service_tunnels.tls]\ncertificate_path = {:?}\nprivate_key_path = {:?}\npinned_certificates_path = {:?}\n",
            cert_path.to_string_lossy(),
            key_path.to_string_lossy(),
            pins_path.to_string_lossy(),
        );
        let config = Config::parse(&text).expect("identity policy parses");
        let policy = config.service_tunnels.tls_policy.expect("policy present");
        let expires = policy
            .policy()
            .identity_expires_unix()
            .expect("expiry surfaces");
        assert!(expires > 1_700_000_000, "expiry is a real Unix time");
        // Absent policy stays absent.
        let config = Config::parse(MINIMAL).expect("minimal parses");
        assert!(config.service_tunnels.tls_policy.is_none());
        // Lone identity halves and empty policies fail.
        let text = format!(
            "{MINIMAL}\n[service_tunnels.tls]\ncertificate_path = {:?}\n",
            cert_path.to_string_lossy()
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
        let text = format!("{MINIMAL}\n[service_tunnels.tls]\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
        // Unknown keys fail closed.
        let text = format!("{MINIMAL}\n[service_tunnels.tls]\ntrust_anchor = true\n");
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn console_defaults_to_disabled_on_loopback() {
        let config = Config::parse(MINIMAL).expect("minimal config parses");
        assert!(!config.console.enabled, "console must be off by default");
        assert!(config.console.bind_address.is_loopback());
        assert_eq!(
            config.console.theme,
            i2pr_console::theme::DEFAULT_THEME_NAME.to_string()
        );
        assert!(config.console.max_connections > 0);
        assert!(config.console.bind_socket().ip().is_loopback());
    }

    #[test]
    fn console_block_is_read_when_present() {
        let text = format!(
            "{MINIMAL}\n[console]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 7071\ntheme = \"i2pr-midnight\"\nmax_connections = 4\n"
        );
        let config = Config::parse(&text).expect("console block parses");
        assert!(config.console.enabled);
        assert_eq!(config.console.port, 7071);
        assert_eq!(config.console.theme, "i2pr-midnight");
        assert_eq!(config.console.max_connections, 4);
    }

    #[test]
    fn console_rejects_non_loopback_bind_address() {
        for address in ["0.0.0.0", "10.0.0.1", "192.168.1.10"] {
            let text =
                format!("{MINIMAL}\n[console]\nenabled = true\nbind_address = \"{address}\"\n");
            assert!(
                matches!(Config::parse(&text), Err(ConfigError::Semantic { .. })),
                "{address} must be rejected"
            );
        }
        // A non-IP literal is also rejected.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\nbind_address = \"localhost\"\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
    }

    #[test]
    fn console_rejects_unknown_theme_identifiers() {
        let text = format!("{MINIMAL}\n[console]\nenabled = true\ntheme = \"no-such-palette\"\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
    }

    #[test]
    fn console_rejects_out_of_range_connection_ceilings() {
        for value in ["0", "65"] {
            let text = format!("{MINIMAL}\n[console]\nenabled = true\nmax_connections = {value}\n");
            assert!(
                matches!(Config::parse(&text), Err(ConfigError::Semantic { .. })),
                "{value} must be rejected"
            );
        }
        // A value that does not fit the field type fails closed at decode
        // time rather than being truncated into the accepted range.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\nmax_connections = 4294967296\n");
        assert!(Config::parse(&text).is_err());
    }

    #[test]
    fn console_rejects_unknown_keys() {
        let text = format!("{MINIMAL}\n[console]\nenabled = true\npublic_expose = true\n");
        assert!(matches!(Config::parse(&text), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn console_connection_ceiling_respects_the_router_task_budget() {
        // A budget smaller than the console ceiling must refuse the
        // configuration even though 64 is inside the console's own range.
        let text = format!(
            "{MINIMAL}\n[limits]\nmax_tasks = 8\nmax_buffered_bytes = 1048576\n\
             \n[console]\nenabled = true\nmax_connections = 16\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
        // The same ceiling fits a default-sized budget.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\nmax_connections = 16\n");
        assert!(Config::parse(&text).is_ok());
    }

    #[test]
    fn console_auth_requires_real_credential_material() {
        // `auth = true` with nothing configured must fail rather than
        // silently accepting an empty password.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\nauth = true\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));

        // An explicitly empty password is refused for the same reason.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\nauth = true\npassword = \"\"\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));

        // A real password is converted to a verifier at parse time and the
        // plaintext does not survive into the normalized configuration.
        let text =
            format!("{MINIMAL}\n[console]\nenabled = true\nauth = true\npassword = \"hunter2\"\n");
        let config = Config::parse(&text).expect("console auth parses");
        assert!(config.console.auth);
        let hash = config
            .console
            .password_hash
            .as_ref()
            .expect("verifier present")
            .as_str()
            .to_string();
        assert!(
            hash.starts_with("$argon2id$"),
            "unexpected verifier form: {hash}"
        );
        // The plaintext must not be recoverable from the snapshot.
        assert!(!format!("{config:?}").contains("hunter2"));
        assert!(!hash.contains("hunter2"));
    }

    #[test]
    fn console_prehashed_verifier_is_validated_at_parse_time() {
        let text = format!(
            "{MINIMAL}\n[console]\nenabled = true\nauth = true\npassword_hash = \"not-a-phc-string\"\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));

        // A non-Argon2id algorithm is refused.
        let text = format!(
            "{MINIMAL}\n[console]\nenabled = true\nauth = true\npassword_hash = \"$argon2i$v=19$m=65536,t=2,p=1$c29tZXNhbHQ$CTFhFdXPJO1aFaMaO6Mm5c8y7cJHAph8ArZWb2GRPPc\"\n"
        );
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
    }

    #[test]
    fn console_credential_without_auth_is_a_configuration_error() {
        // Credentials present while auth is off would be a dormant
        // backdoor waiting for a flag flip.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\npassword = \"hunter2\"\n");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::Semantic { .. })
        ));
    }

    #[test]
    fn console_session_and_throttle_bounds_are_validated() {
        // Zero sessions, an absolute window shorter than the idle window,
        // and a zero-failure throttle are all refused.
        let cases = [
            "max_sessions = 0\n",
            "session_idle_secs = 40000\n",
            "session_absolute_secs = 60\n",
            "login_max_failures = 0\n",
            "login_window_secs = 0\n",
        ];
        for case in cases {
            let text = format!("{MINIMAL}\n[console]\nenabled = true\n{case}");
            assert!(
                matches!(Config::parse(&text), Err(ConfigError::Semantic { .. })),
                "{case} must be refused"
            );
        }
        // The defaults are accepted.
        let text = format!("{MINIMAL}\n[console]\nenabled = true\n");
        let config = Config::parse(&text).expect("defaults parse");
        assert_eq!(config.console.max_sessions, 32);
        assert!(!config.console.auth);
        assert!(config.console.password_hash.is_none());
    }
}
