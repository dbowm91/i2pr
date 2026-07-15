//! Strict versioned configuration parsing and side-effect-free normalization.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    schema_version: u64,
    router: RawRouterConfig,
    #[serde(default)]
    logging: RawLoggingConfig,
    #[serde(default)]
    limits: RawLimitsConfig,
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
}
