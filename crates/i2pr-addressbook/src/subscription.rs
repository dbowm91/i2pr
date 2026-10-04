//! Subscription sources: bounded URL set and strict body ingestion.
//!
//! `SetSubscriptions` replaces the complete source set. URLs are
//! HTTP/HTTPS only (bounded host, no credentials); the fetch itself is
//! daemon-composed through the loopback-proxy capability. Body ingestion
//! accepts `hostname=destination` lines (`#` comments and
//! blanks skipped), every line validated, any invalid line failing the
//! whole body, duplicates resolved last-wins.

use std::collections::BTreeMap;

use crate::destination_text::{MAX_DESTINATION_TEXT_LEN, validate_destination_text};
use crate::error::AddressBookError;
use crate::generation::MAX_ENTRIES_PER_BOOK;
use crate::hostname::{Hostname, MAX_HOSTNAME_LEN, parse_hostname};

/// Maximum subscription URLs in one set (mirrors the wire ceiling).
pub const MAX_SUBSCRIPTION_URLS: usize = 16;
/// Maximum subscription URL length in bytes (mirrors the wire ceiling).
pub const MAX_SUBSCRIPTION_URL_LEN: usize = 2048;
/// Maximum accepted subscription body in bytes.
pub const MAX_SUBSCRIPTION_BODY_BYTES: usize = 1_048_576;
/// Maximum entries produced by one subscription body.
pub const MAX_SUBSCRIBED_ENTRIES: usize = MAX_ENTRIES_PER_BOOK;
/// Maximum length of one subscription body line in bytes.
pub const MAX_SUBSCRIPTION_LINE_LEN: usize = 8192;

/// Validated subscription URL set (order-preserving, deduplicated).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionSet {
    urls: Vec<String>,
}

/// Conditional HTTP validators and parsed entries retained for one
/// configured subscription. Entries and validators share the owning
/// generation so a 304 cannot refer to missing or newer content.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionSource {
    /// Last completely validated body, as canonical hostname entries.
    pub entries: BTreeMap<Hostname, String>,
    /// HTTP ETag value, if supplied by the source.
    pub etag: Option<String>,
    /// HTTP Last-Modified value, if supplied by the source.
    pub last_modified: Option<String>,
}

/// Maximum accepted validator value length.
pub const MAX_SUBSCRIPTION_VALIDATOR_LEN: usize = 1024;

/// Validates a conditional validator before persisting or sending it.
pub fn validate_subscription_validator(value: &str) -> Result<(), AddressBookError> {
    if value.len() > MAX_SUBSCRIPTION_VALIDATOR_LEN
        || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return Err(AddressBookError::InvalidConfigValue);
    }
    Ok(())
}

impl SubscriptionSet {
    /// Empty set.
    pub fn new() -> Self {
        Self { urls: Vec::new() }
    }

    /// Validates a whole URL list before accepting it: over-count and
    /// any invalid URL fail with nothing accepted.
    pub fn checked(urls: &[String]) -> Result<Self, AddressBookError> {
        if urls.len() > MAX_SUBSCRIPTION_URLS {
            return Err(AddressBookError::TooManySubscriptions);
        }
        let mut distinct = Vec::with_capacity(urls.len());
        for url in urls {
            validate_subscription_url(url)?;
            if !distinct.contains(url) {
                distinct.push(url.clone());
            }
        }
        Ok(Self { urls: distinct })
    }

    /// Canonical URL list in request order.
    pub fn urls(&self) -> &[String] {
        &self.urls
    }
}

/// Validates one subscription URL: `http`/`https` scheme
/// (case-insensitive), non-empty host, bounded length, no whitespace
/// or control bytes, no userinfo.
pub fn validate_subscription_url(url: &str) -> Result<(), AddressBookError> {
    if url.is_empty() || url.len() > MAX_SUBSCRIPTION_URL_LEN {
        return Err(AddressBookError::InvalidSubscription);
    }
    if url.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return Err(AddressBookError::InvalidSubscription);
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
        .ok_or(AddressBookError::InvalidSubscription)?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(':');
    if host.is_empty() || host.len() > MAX_HOSTNAME_LEN || host.contains('@') {
        return Err(AddressBookError::InvalidSubscription);
    }
    Ok(())
}

/// Ingests one downloaded subscription body into validated entries.
/// Any invalid line fails the whole body; later duplicates replace
/// earlier ones (last-wins, deterministic).
pub fn ingest_subscription_body(
    body: &[u8],
) -> Result<BTreeMap<Hostname, String>, AddressBookError> {
    if body.len() > MAX_SUBSCRIPTION_BODY_BYTES {
        return Err(AddressBookError::BodyOverBound);
    }
    let mut entries = BTreeMap::new();
    for line in body.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }
        if line.len() > MAX_SUBSCRIPTION_LINE_LEN {
            return Err(AddressBookError::BodyOverBound);
        }
        let text = core::str::from_utf8(line).map_err(|_| AddressBookError::InvalidDestination)?;
        let (name, destination) = text
            .split_once('=')
            .ok_or(AddressBookError::MalformedField)?;
        if name.len() > MAX_HOSTNAME_LEN || destination.len() > MAX_DESTINATION_TEXT_LEN {
            return Err(AddressBookError::InvalidDestination);
        }
        let hostname = parse_hostname(name)?;
        validate_destination_text(destination)?;
        if entries.len() >= MAX_SUBSCRIBED_ENTRIES && !entries.contains_key(&hostname) {
            return Err(AddressBookError::ListOverBound);
        }
        entries.insert(hostname, destination.to_owned());
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64ct::Encoding;

    fn destination_text() -> String {
        let mut bytes = vec![0u8; 384];
        bytes.extend_from_slice(&[5u8, 0, 4, 0, 7, 0, 4]);
        let mapped: Vec<u8> = base64ct::Base64::encode_string(&bytes)
            .bytes()
            .map(|byte| match byte {
                b'+' => b'-',
                b'/' => b'~',
                other => other,
            })
            .collect();
        String::from_utf8(mapped).expect("alphabet stays ASCII")
    }

    #[test]
    fn urls_validate_by_scheme_and_host() {
        for good in [
            "http://example.i2p/hosts.txt",
            "https://example.i2p:8443/hosts.txt",
            "HTTP://EXAMPLE.I2P/HOSTS.TXT",
            "http://127.0.0.1/hosts.txt",
        ] {
            assert!(validate_subscription_url(good).is_ok(), "{good} passes");
        }
        for bad in [
            "",
            "ftp://example.i2p/hosts.txt",
            "file:///etc/hosts",
            "http://",
            "http:///path",
            "http://user@example.i2p/hosts.txt",
            "http://example.i2p/has space.txt",
            "example.i2p/hosts.txt",
        ] {
            assert!(validate_subscription_url(bad).is_err(), "{bad:?} fails");
        }
        let many = vec!["http://example.i2p/hosts.txt".to_owned(); MAX_SUBSCRIPTION_URLS + 1];
        assert_eq!(
            SubscriptionSet::checked(&many),
            Err(AddressBookError::TooManySubscriptions)
        );
        // Duplicates collapse; order is request order.
        let set = SubscriptionSet::checked(&[
            "http://b.i2p/h".to_owned(),
            "http://a.i2p/h".to_owned(),
            "http://b.i2p/h".to_owned(),
        ])
        .expect("dedup");
        assert_eq!(
            set.urls(),
            &["http://b.i2p/h".to_owned(), "http://a.i2p/h".to_owned()]
        );
    }

    #[test]
    fn body_ingestion_is_strict() {
        let destination = destination_text();
        let body = format!(
            "# comment\n\nfirst.i2p={destination}\nsecond.i2p={destination}\nfirst.i2p={destination}\n"
        );
        let entries = ingest_subscription_body(body.as_bytes()).expect("ingest");
        assert_eq!(entries.len(), 2);
        // Every invalid line fails the whole body.
        for bad_body in [
            "no-equals-here\n".to_owned(),
            format!("bad name.i2p={destination}\n"),
            "good.i2p=not-a-destination\n".to_owned(),
            "x=".to_owned(),
        ] {
            assert!(
                ingest_subscription_body(bad_body.as_bytes()).is_err(),
                "must reject {bad_body:?}"
            );
        }
    }

    #[test]
    fn body_bounds_hold() {
        let big = vec![b'a'; MAX_SUBSCRIPTION_BODY_BYTES + 1];
        assert_eq!(
            ingest_subscription_body(&big),
            Err(AddressBookError::BodyOverBound)
        );
        let long_line = format!("{}={}\n", "a".repeat(8200), "QUJD");
        assert_eq!(
            ingest_subscription_body(long_line.as_bytes()),
            Err(AddressBookError::BodyOverBound)
        );
    }
}
