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
    sam: RawSamConfig,
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

const fn default_sam_enabled() -> bool {
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

/// Immutable normalized configuration snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
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
    /// SAM v3.1 service settings.
    pub sam: SamConfig,
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
        Self::parse(&contents).map_err(super::error::DaemonError::from)
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
        let raw: RawConfig = toml::from_str(contents).map_err(ConfigError::Parse)?;
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
        let sam = normalize_sam(&raw.sam)?;

        Ok(Self {
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
            sam,
        })
    }
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

fn normalize_sam(raw: &RawSamConfig) -> Result<SamConfig, ConfigError> {
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
    Ok(SamConfig {
        enabled: raw.enabled,
        bind_address,
        port: raw.port,
        limits,
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

/// Configuration parse and semantic-validation failures.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// TOML syntax or schema decoding failed.
    #[error("configuration parse failed: {0}")]
    Parse(#[source] toml::de::Error),
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
}

impl ConfigError {
    /// Maps the failure to the stable daemon exit-code category.
    pub const fn exit_code(&self) -> super::error::ExitCode {
        match self {
            Self::Parse(_) | Self::UnsupportedSchemaVersion { .. } => {
                super::error::ExitCode::ConfigParse
            }
            Self::Semantic { .. } => super::error::ExitCode::ConfigSemantic,
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
max_buffered_bytes = 1024

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
}
