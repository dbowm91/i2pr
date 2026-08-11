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
}
