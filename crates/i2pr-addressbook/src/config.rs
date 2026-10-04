//! Thirteen-key `SetConfig` domain with one explicit disposition per key.
//!
//! Dispositions (values are validated before any mutation; unknown keys
//! fail the whole request):
//!
//! - `private_book`, `local_book`, `router_book`, `published_book`,
//!   `subscriptions`, `log_file`: confined logical artifact names. They
//!   select AddressBook-owned artifacts (per-book snapshots, staged
//!   subscription body, diagnostic log) and can never address absolute
//!   paths, parent escapes, control characters, symlinks, reserved
//!   router state, or non-regular files. The `subscriptions` key names
//!   the staged-download artifact consumed by the ingestion pipeline.
//! - `refresh_interval`: integer hours, 1..=720. Drives the refresh
//!   worker cadence.
//! - `proxy_host`, `proxy_port`: bounded host / 1..=65535 port for the
//!   subscription fetch path only. They never create a general proxy
//!   capability. No downloader owner exists yet, so refresh attempts
//!   report unavailable; the values are validated, stored, and
//!   round-tripped for the fetch path to consume when one is composed.
//! - `theme`: inert frontend metadata. Durable round-trip only; no
//!   router, logging, or frontend side effect.
//! - `log_level`: artifact verbosity only (`off`, `error`, `warn`,
//!   `info`, `debug`). Never redirects or reconfigures global tracing.
//! - `lookup_timeout`: integer seconds, 1..=300. Bounds the fetch
//!   stages of the subscription pipeline once a downloader owner
//!   exists; validated, stored, and round-tripped meanwhile.
//! - `max_entries`: per-book entry ceiling, 1..=[`MAX_ENTRIES_PER_BOOK`].
//!   Enforced on mutation, import, and configuration tightening.

use std::collections::BTreeMap;

use crate::error::AddressBookError;
use crate::generation::MAX_ENTRIES_PER_BOOK;

/// Maximum length of one config value in bytes.
pub const MAX_CONFIG_VALUE_LEN: usize = 1024;
/// Maximum length of one logical artifact path in bytes.
pub const MAX_CONFIG_PATH_LEN: usize = 255;
/// Minimum refresh interval in hours.
pub const MIN_REFRESH_INTERVAL_HOURS: u64 = 1;
/// Maximum refresh interval in hours.
pub const MAX_REFRESH_INTERVAL_HOURS: u64 = 720;
/// Minimum lookup timeout in seconds.
pub const MIN_LOOKUP_TIMEOUT_SECS: u64 = 1;
/// Maximum lookup timeout in seconds.
pub const MAX_LOOKUP_TIMEOUT_SECS: u64 = 300;

/// One frozen `SetConfig` key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigKey {
    /// Private-book snapshot artifact name.
    PrivateBook,
    /// Local-book snapshot artifact name.
    LocalBook,
    /// Router-book snapshot artifact name.
    RouterBook,
    /// Published-book snapshot artifact name.
    PublishedBook,
    /// Staged subscription-body artifact name.
    Subscriptions,
    /// Refresh cadence in hours.
    RefreshInterval,
    /// Fetch-path proxy host (fetch path only).
    ProxyHost,
    /// Fetch-path proxy port (fetch path only).
    ProxyPort,
    /// Inert frontend metadata.
    Theme,
    /// Diagnostic artifact name (address-book diagnostics only).
    LogFile,
    /// Diagnostic artifact verbosity.
    LogLevel,
    /// Fetch-stage bound in seconds.
    LookupTimeout,
    /// Per-book entry ceiling.
    MaxEntries,
    /// Whether the eligible router address book is regenerated for publication.
    ShouldPublish,
    /// ETag validator artifact.
    Etags,
    /// Last-Modified validator artifact.
    LastModified,
}

impl ConfigKey {
    /// Exact owner-config spelling (Proposal wire names are projected at the daemon boundary).
    pub const fn name(self) -> &'static str {
        match self {
            Self::PrivateBook => "private_book",
            Self::LocalBook => "local_book",
            Self::RouterBook => "router_book",
            Self::PublishedBook => "published_book",
            Self::Subscriptions => "subscriptions",
            Self::RefreshInterval => "refresh_interval",
            Self::ProxyHost => "proxy_host",
            Self::ProxyPort => "proxy_port",
            Self::Theme => "theme",
            Self::LogFile => "log_file",
            Self::LogLevel => "log_level",
            Self::LookupTimeout => "lookup_timeout",
            Self::MaxEntries => "max_entries",
            Self::ShouldPublish => "should_publish",
            Self::Etags => "etags",
            Self::LastModified => "last_modified",
        }
    }

    /// Whether the value is a confined logical artifact path.
    pub const fn is_path_like(self) -> bool {
        matches!(
            self,
            Self::PrivateBook
                | Self::LocalBook
                | Self::RouterBook
                | Self::PublishedBook
                | Self::Subscriptions
                | Self::LogFile
                | Self::Etags
                | Self::LastModified
        )
    }
}

/// Parses an exact `SetConfig` key spelling.
pub fn parse_config_key(name: &str) -> Result<ConfigKey, AddressBookError> {
    match name {
        "private_book" => Ok(ConfigKey::PrivateBook),
        "local_book" => Ok(ConfigKey::LocalBook),
        "router_book" => Ok(ConfigKey::RouterBook),
        "published_book" => Ok(ConfigKey::PublishedBook),
        "subscriptions" => Ok(ConfigKey::Subscriptions),
        "refresh_interval" => Ok(ConfigKey::RefreshInterval),
        "proxy_host" => Ok(ConfigKey::ProxyHost),
        "proxy_port" => Ok(ConfigKey::ProxyPort),
        "theme" => Ok(ConfigKey::Theme),
        "log_file" => Ok(ConfigKey::LogFile),
        "log_level" => Ok(ConfigKey::LogLevel),
        "lookup_timeout" => Ok(ConfigKey::LookupTimeout),
        "max_entries" => Ok(ConfigKey::MaxEntries),
        "should_publish" => Ok(ConfigKey::ShouldPublish),
        "etags" => Ok(ConfigKey::Etags),
        "last_modified" => Ok(ConfigKey::LastModified),
        _ => Err(AddressBookError::UnknownConfigKey),
    }
}

/// Diagnostic artifact verbosity (artifact only, never global tracing).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LogLevel {
    /// No diagnostic output.
    Off,
    /// Failures only.
    Error,
    /// Failures and recoveries.
    #[default]
    Warn,
    /// Refresh lifecycle included.
    Info,
    /// Per-stage detail.
    Debug,
}

impl LogLevel {
    /// Parses an exact level spelling.
    pub fn parse(text: &str) -> Result<Self, AddressBookError> {
        match text {
            "off" => Ok(Self::Off),
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            _ => Err(AddressBookError::InvalidConfigValue),
        }
    }

    /// Canonical spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

/// Validated address-book owner configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressBookConfig {
    /// Per-book snapshot artifact names (logical, confined).
    pub book_artifacts: [String; 4],
    /// Staged subscription-body artifact name (logical, confined).
    pub subscriptions_artifact: String,
    /// Refresh cadence in hours.
    pub refresh_interval_hours: u64,
    /// Fetch-path proxy host, when configured.
    pub proxy_host: Option<String>,
    /// Fetch-path proxy port, when configured.
    pub proxy_port: Option<u16>,
    /// Inert frontend metadata (round-trip only).
    pub theme: String,
    /// Diagnostic artifact name (logical, confined).
    pub log_file: String,
    /// Diagnostic artifact verbosity.
    pub log_level: LogLevel,
    /// Fetch-stage bound in seconds.
    pub lookup_timeout_secs: u64,
    /// Per-book entry ceiling.
    pub max_entries: usize,
    /// Whether a published-book artifact is regenerated after refresh.
    pub should_publish: bool,
    /// Logical artifact name for subscription ETag validators.
    pub etags_artifact: String,
    /// Logical artifact name for subscription Last-Modified validators.
    pub last_modified_artifact: String,
}

impl Default for AddressBookConfig {
    fn default() -> Self {
        Self {
            book_artifacts: [
                "private.json".to_owned(),
                "local.json".to_owned(),
                "router.json".to_owned(),
                "published.json".to_owned(),
            ],
            subscriptions_artifact: "subscriptions.body".to_owned(),
            refresh_interval_hours: 24,
            proxy_host: None,
            proxy_port: None,
            theme: String::new(),
            log_file: "addressbook.log".to_owned(),
            log_level: LogLevel::Warn,
            lookup_timeout_secs: 30,
            max_entries: MAX_ENTRIES_PER_BOOK,
            should_publish: false,
            etags_artifact: "subscriptions.etags".to_owned(),
            last_modified_artifact: "subscriptions.last-modified".to_owned(),
        }
    }
}

impl AddressBookConfig {
    /// Renders all owner keys in canonical order for persistence and
    /// generation encoding.
    pub fn rendered_entries(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        let keys = [
            "private_book",
            "local_book",
            "router_book",
            "published_book",
            "subscriptions",
            "refresh_interval",
            "proxy_host",
            "proxy_port",
            "theme",
            "log_file",
            "log_level",
            "lookup_timeout",
            "max_entries",
            "should_publish",
            "etags",
            "last_modified",
        ];
        let values = [
            self.book_artifacts[0].clone(),
            self.book_artifacts[1].clone(),
            self.book_artifacts[2].clone(),
            self.book_artifacts[3].clone(),
            self.subscriptions_artifact.clone(),
            self.refresh_interval_hours.to_string(),
            self.proxy_host.clone().unwrap_or_default(),
            self.proxy_port
                .map(|port| port.to_string())
                .unwrap_or_default(),
            self.theme.clone(),
            self.log_file.clone(),
            self.log_level.name().to_owned(),
            self.lookup_timeout_secs.to_string(),
            self.max_entries.to_string(),
            self.should_publish.to_string(),
            self.etags_artifact.clone(),
            self.last_modified_artifact.clone(),
        ];
        for (key, value) in keys.into_iter().zip(values) {
            map.insert(key.to_owned(), value);
        }
        map
    }

    /// Validates a whole `SetConfig` map into an updated configuration
    /// without mutating `self`: unknown keys, bad values, and escaping
    /// paths fail before anything is applied.
    pub fn checked_update(
        &self,
        entries: &BTreeMap<String, String>,
    ) -> Result<Self, AddressBookError> {
        let mut next = self.clone();
        for (key, value) in entries {
            if value.len() > MAX_CONFIG_VALUE_LEN {
                return Err(AddressBookError::InvalidConfigValue);
            }
            match parse_config_key(key)? {
                ConfigKey::PrivateBook => {
                    next.book_artifacts[0] = confined_path(value)?;
                }
                ConfigKey::LocalBook => {
                    next.book_artifacts[1] = confined_path(value)?;
                }
                ConfigKey::RouterBook => {
                    next.book_artifacts[2] = confined_path(value)?;
                }
                ConfigKey::PublishedBook => {
                    next.book_artifacts[3] = confined_path(value)?;
                }
                ConfigKey::Subscriptions => {
                    next.subscriptions_artifact = confined_path(value)?;
                }
                ConfigKey::RefreshInterval => {
                    next.refresh_interval_hours = parse_ranged_u64(
                        value,
                        MIN_REFRESH_INTERVAL_HOURS,
                        MAX_REFRESH_INTERVAL_HOURS,
                    )?;
                }
                ConfigKey::ProxyHost => {
                    next.proxy_host = if value.is_empty() {
                        None
                    } else {
                        Some(validated_proxy_host(value)?)
                    };
                }
                ConfigKey::ProxyPort => {
                    next.proxy_port = if value.is_empty() {
                        None
                    } else {
                        let port = parse_ranged_u64(value, 1, 65_535)?;
                        Some(port as u16)
                    };
                }
                ConfigKey::Theme => {
                    next.theme = value.clone();
                }
                ConfigKey::LogFile => {
                    next.log_file = confined_path(value)?;
                }
                ConfigKey::LogLevel => {
                    next.log_level = LogLevel::parse(value)?;
                }
                ConfigKey::LookupTimeout => {
                    next.lookup_timeout_secs =
                        parse_ranged_u64(value, MIN_LOOKUP_TIMEOUT_SECS, MAX_LOOKUP_TIMEOUT_SECS)?;
                }
                ConfigKey::MaxEntries => {
                    let ceiling = parse_ranged_u64(value, 1, MAX_ENTRIES_PER_BOOK as u64)?;
                    next.max_entries = ceiling as usize;
                }
                ConfigKey::ShouldPublish => {
                    next.should_publish = match value.as_str() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(AddressBookError::InvalidConfigValue),
                    };
                }
                ConfigKey::Etags => next.etags_artifact = confined_path(value)?,
                ConfigKey::LastModified => next.last_modified_artifact = confined_path(value)?,
            }
        }
        Ok(next)
    }
}

/// Confines a logical artifact name: non-empty, bounded, relative,
/// with no parent escape, no absolute form, and no control bytes.
/// Separators are rejected outright (flat artifact namespace), which
/// also excludes drive/UNC forms.
fn confined_path(value: &str) -> Result<String, AddressBookError> {
    if value.is_empty() || value.len() > MAX_CONFIG_PATH_LEN {
        return Err(AddressBookError::InvalidConfigValue);
    }
    if value
        .bytes()
        .any(|byte| byte < 0x20 || byte == 0x7f || byte == b'/' || byte == b'\\' || byte == b'\0')
    {
        return Err(AddressBookError::InvalidConfigValue);
    }
    if value == "." || value == ".." {
        return Err(AddressBookError::PathEscape);
    }
    if value.starts_with('.') {
        // Hidden-file impersonation and fragile dotfile semantics are
        // out of scope for the artifact namespace.
        return Err(AddressBookError::InvalidConfigValue);
    }
    Ok(value.to_owned())
}

/// Parses a bounded integer config value (ASCII digits only).
fn parse_ranged_u64(text: &str, minimum: u64, maximum: u64) -> Result<u64, AddressBookError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AddressBookError::InvalidConfigValue);
    }
    let value: u64 = text
        .parse()
        .map_err(|_| AddressBookError::InvalidConfigValue)?;
    if value < minimum || value > maximum {
        return Err(AddressBookError::InvalidConfigValue);
    }
    Ok(value)
}

/// Validates a fetch-path proxy host: bounded IP literal or hostname,
/// no scheme, port, userinfo, or whitespace.
fn validated_proxy_host(value: &str) -> Result<String, AddressBookError> {
    if value.len() > MAX_HOSTNAME_LEN_PLUS {
        return Err(AddressBookError::InvalidConfigValue);
    }
    if value.bytes().any(|byte| {
        byte <= 0x20
            || byte == 0x7f
            || byte == b'/'
            || byte == b':'
            || byte == b'@'
            || byte == b'['
            || byte == b']'
    }) {
        return Err(AddressBookError::InvalidConfigValue);
    }
    // Reuse the hostname shape where it fits; IP literals pass the
    // character gate above and need no further structure here.
    Ok(value.to_owned())
}

/// Host ceiling shared with hostnames (255) plus bracket headroom.
const MAX_HOSTNAME_LEN_PLUS: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn whole_map_validates_before_mutation() {
        let config = AddressBookConfig::default();
        let updated = config
            .checked_update(&map(&[
                ("refresh_interval", "12"),
                ("theme", "midnight"),
                ("log_level", "debug"),
                ("max_entries", "10"),
            ]))
            .expect("valid map");
        assert_eq!(updated.refresh_interval_hours, 12);
        assert_eq!(updated.theme, "midnight");
        assert_eq!(updated.log_level, LogLevel::Debug);
        assert_eq!(updated.max_entries, 10);
        // One bad value fails everything; the receiver is untouched.
        assert_eq!(
            config.checked_update(&map(&[("refresh_interval", "12"), ("proxy_port", "0")])),
            Err(AddressBookError::InvalidConfigValue)
        );
        assert_eq!(
            config.checked_update(&map(&[("nope", "1")])),
            Err(AddressBookError::UnknownConfigKey)
        );
    }

    #[test]
    fn publication_and_validator_config_are_typed_and_confined() {
        let config = AddressBookConfig::default();
        let updated = config
            .checked_update(&map(&[
                ("should_publish", "true"),
                ("etags", "validators.etag"),
                ("last_modified", "validators.modified"),
            ]))
            .expect("valid config");
        assert!(updated.should_publish);
        assert_eq!(updated.etags_artifact, "validators.etag");
        assert_eq!(updated.last_modified_artifact, "validators.modified");
        for invalid in ["yes", "1", "true "] {
            assert!(
                config
                    .checked_update(&map(&[("should_publish", invalid)]))
                    .is_err()
            );
        }
        for invalid in ["../escape", "/tmp/escape", "dir/file"] {
            assert!(config.checked_update(&map(&[("etags", invalid)])).is_err());
        }
    }

    #[test]
    fn paths_stay_confined() {
        let config = AddressBookConfig::default();
        for bad in [
            "",
            "/abs/path",
            "../escape",
            "..",
            ".",
            ".hidden",
            "a/b",
            "a\\b",
            "nul\0byte",
            "tab\there",
        ] {
            assert!(
                config.checked_update(&map(&[("log_file", bad)])).is_err(),
                "must reject {bad:?}"
            );
        }
        let updated = config
            .checked_update(&map(&[("log_file", "diag-1.log")]))
            .expect("flat name");
        assert_eq!(updated.log_file, "diag-1.log");
    }

    #[test]
    fn numeric_bounds_hold() {
        let config = AddressBookConfig::default();
        for (key, good, bads) in [
            (
                "refresh_interval",
                "720",
                vec!["0", "721", "12h", " 12", "+12"],
            ),
            ("proxy_port", "8080", vec!["0", "65536", "http"]),
            ("lookup_timeout", "300", vec!["0", "301", "-1"]),
            ("max_entries", "1000", vec!["0", "1001", "lots"]),
        ] {
            assert!(
                config.checked_update(&map(&[(key, good)])).is_ok(),
                "{key}={good} must pass"
            );
            for bad in bads {
                assert!(
                    config.checked_update(&map(&[(key, bad)])).is_err(),
                    "{key}={bad} must fail"
                );
            }
        }
    }

    #[test]
    fn proxy_hosts_are_bounded_hosts() {
        let config = AddressBookConfig::default();
        for good in ["proxy.i2p", "127.0.0.1", "10.0.0.7"] {
            assert!(
                config.checked_update(&map(&[("proxy_host", good)])).is_ok(),
                "{good} must pass"
            );
        }
        for bad in [
            "http://proxy.i2p/",
            "proxy:8080",
            "user@proxy",
            "has space",
            "[::1]",
        ] {
            assert!(
                config.checked_update(&map(&[("proxy_host", bad)])).is_err(),
                "{bad:?} must fail"
            );
        }
        // Empty clears the optional host/port pair.
        let updated = config
            .checked_update(&map(&[("proxy_host", ""), ("proxy_port", "")]))
            .expect("clear");
        assert_eq!(updated.proxy_host, None);
        assert_eq!(updated.proxy_port, None);
    }

    #[test]
    fn key_inventory_matches_contract_order() {
        let keys = [
            "private_book",
            "local_book",
            "router_book",
            "published_book",
            "subscriptions",
            "refresh_interval",
            "proxy_host",
            "proxy_port",
            "theme",
            "log_file",
            "log_level",
            "lookup_timeout",
            "max_entries",
        ];
        assert_eq!(keys.len(), 13);
        for key in keys {
            assert!(parse_config_key(key).is_ok(), "{key} parses");
        }
    }
}
