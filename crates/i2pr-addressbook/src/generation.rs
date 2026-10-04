//! Deterministic versioned generations: the persisted form.
//!
//! A generation is one complete bounded state: four books, the
//! subscription set, and the typed owner configuration, plus a
//! saturating revision. Serialization is deterministic JSON (fixed
//! field order, `BTreeMap` ordering throughout). Decoding re-validates
//! everything a live mutation would — hostnames, destinations,
//! counts, config values — so a corrupt or hostile file can never
//! activate partially: any failure rejects the whole generation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::book::{AddressBook, BookKind};
use crate::error::AddressBookError;

/// Generation format version (reject anything else, fail closed).
pub const GENERATION_VERSION: u32 = 1;
/// Maximum entries stored per book.
pub const MAX_ENTRIES_PER_BOOK: usize = 1000;
/// Maximum accepted generation bytes (bounds hostile files).
pub const MAX_GENERATION_BYTES: usize = 24_000_000;

/// Serializable generation shape (field order is the byte order).
#[derive(Serialize, Deserialize)]
struct GenerationShape {
    version: u32,
    revision: u64,
    private: BTreeMap<String, String>,
    local: BTreeMap<String, String>,
    router: BTreeMap<String, String>,
    published: BTreeMap<String, String>,
    subscriptions: Vec<String>,
    config: ConfigShape,
}

/// Serializable configuration shape (fixed field order).
#[derive(Serialize, Deserialize)]
struct ConfigShape {
    book_artifacts: [String; 4],
    subscriptions_artifact: String,
    refresh_interval_hours: u64,
    proxy_host: Option<String>,
    proxy_port: Option<u16>,
    theme: String,
    log_file: String,
    log_level: String,
    lookup_timeout_secs: u64,
    max_entries: usize,
    #[serde(default)]
    should_publish: bool,
    #[serde(default = "default_etags_artifact")]
    etags_artifact: String,
    #[serde(default = "default_last_modified_artifact")]
    last_modified_artifact: String,
}

fn default_etags_artifact() -> String {
    "subscriptions.etags".to_owned()
}
fn default_last_modified_artifact() -> String {
    "subscriptions.last-modified".to_owned()
}

impl ConfigShape {
    fn capture(config: &crate::config::AddressBookConfig) -> Self {
        Self {
            book_artifacts: config.book_artifacts.clone(),
            subscriptions_artifact: config.subscriptions_artifact.clone(),
            refresh_interval_hours: config.refresh_interval_hours,
            proxy_host: config.proxy_host.clone(),
            proxy_port: config.proxy_port,
            theme: config.theme.clone(),
            log_file: config.log_file.clone(),
            log_level: config.log_level.name().to_owned(),
            lookup_timeout_secs: config.lookup_timeout_secs,
            max_entries: config.max_entries,
            should_publish: config.should_publish,
            etags_artifact: config.etags_artifact.clone(),
            last_modified_artifact: config.last_modified_artifact.clone(),
        }
    }
}

/// Encodes committed state deterministically.
pub fn encode_generation(book: &AddressBook) -> Vec<u8> {
    let shape = GenerationShape {
        version: GENERATION_VERSION,
        revision: book.revision(),
        private: listed(book, BookKind::Private),
        local: listed(book, BookKind::Local),
        router: listed(book, BookKind::Router),
        published: listed(book, BookKind::Published),
        subscriptions: book.subscriptions().urls().to_vec(),
        config: ConfigShape::capture(book.config()),
    };
    serde_json::to_vec(&shape).expect("in-memory state serializes")
}

fn listed(book: &AddressBook, kind: BookKind) -> BTreeMap<String, String> {
    book.list(kind)
        .into_iter()
        .map(|(name, destination)| (name.as_str().to_owned(), destination))
        .collect()
}

/// Decodes and fully re-validates one generation. Any structural,
/// version, hostname, destination, count, or config failure rejects
/// the whole generation.
pub fn decode_generation(bytes: &[u8]) -> Result<AddressBook, AddressBookError> {
    if bytes.len() > MAX_GENERATION_BYTES {
        return Err(AddressBookError::InvalidGeneration);
    }
    let shape: GenerationShape =
        serde_json::from_slice(bytes).map_err(|_| AddressBookError::InvalidGeneration)?;
    if shape.version != GENERATION_VERSION {
        return Err(AddressBookError::InvalidGeneration);
    }
    let mut book = AddressBook::new();
    {
        let mut control = book.control();
        // Configuration first so the stored entry ceiling governs the
        // book admission below.
        let config_map: BTreeMap<String, String> = [
            ("private_book", shape.config.book_artifacts[0].clone()),
            ("local_book", shape.config.book_artifacts[1].clone()),
            ("router_book", shape.config.book_artifacts[2].clone()),
            ("published_book", shape.config.book_artifacts[3].clone()),
            ("subscriptions", shape.config.subscriptions_artifact.clone()),
            (
                "refresh_interval",
                shape.config.refresh_interval_hours.to_string(),
            ),
            (
                "proxy_host",
                shape.config.proxy_host.clone().unwrap_or_default(),
            ),
            (
                "proxy_port",
                shape
                    .config
                    .proxy_port
                    .map(|port| port.to_string())
                    .unwrap_or_default(),
            ),
            ("theme", shape.config.theme.clone()),
            ("log_file", shape.config.log_file.clone()),
            ("log_level", shape.config.log_level.clone()),
            (
                "lookup_timeout",
                shape.config.lookup_timeout_secs.to_string(),
            ),
            ("max_entries", shape.config.max_entries.to_string()),
            ("should_publish", shape.config.should_publish.to_string()),
            ("etags", shape.config.etags_artifact.clone()),
            ("last_modified", shape.config.last_modified_artifact.clone()),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
        control
            .apply_config(&config_map)
            .map_err(|_| AddressBookError::InvalidGeneration)?;
        for (entries, kind) in [
            (&shape.private, BookKind::Private),
            (&shape.local, BookKind::Local),
            (&shape.router, BookKind::Router),
            (&shape.published, BookKind::Published),
        ] {
            if entries.len() > MAX_ENTRIES_PER_BOOK {
                return Err(AddressBookError::InvalidGeneration);
            }
            for (name, destination) in entries {
                // Re-admission runs the same validators a live mutation
                // would; any deviation rejects the whole generation.
                control
                    .apply_entry(crate::book::EntryMutation {
                        book: kind,
                        hostname: name.clone(),
                        destination: Some(destination.clone()),
                        delete: false,
                    })
                    .map_err(|_| AddressBookError::InvalidGeneration)?;
            }
        }
        control
            .replace_subscriptions(&shape.subscriptions)
            .map_err(|_| AddressBookError::InvalidGeneration)?;
    }
    // Restore the persisted revision exactly (mutations bump from here).
    book.set_revision(shape.revision);
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::EntryMutation;
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
    fn round_trip_preserves_state() {
        let mut book = AddressBook::new();
        {
            let mut control = book.control();
            control
                .apply_entry(EntryMutation {
                    book: BookKind::Router,
                    hostname: "r.i2p".to_owned(),
                    destination: Some(destination_text()),
                    delete: false,
                })
                .expect("entry");
            control
                .replace_subscriptions(&["http://example.i2p/hosts.txt".to_owned()])
                .expect("subscriptions");
            let mut config = BTreeMap::new();
            config.insert("theme".to_owned(), "midnight".to_owned());
            config.insert("max_entries".to_owned(), "42".to_owned());
            control.apply_config(&config).expect("config");
        }
        let revision = book.revision();
        assert!(revision > 0);
        let bytes = encode_generation(&book);
        // Deterministic: same state, same bytes.
        assert_eq!(bytes, encode_generation(&book));
        let restored = decode_generation(&bytes).expect("decode");
        assert_eq!(restored.revision(), revision);
        assert_eq!(restored.book_len(BookKind::Router), 1);
        assert_eq!(
            restored.subscriptions().urls(),
            &["http://example.i2p/hosts.txt".to_owned()]
        );
        assert_eq!(restored.config().theme, "midnight");
        assert_eq!(restored.config().max_entries, 42);
        assert_eq!(encode_generation(&restored), bytes);
    }

    #[test]
    fn hostile_generations_fail_whole() {
        let mut book = AddressBook::new();
        book.control()
            .apply_entry(EntryMutation {
                book: BookKind::Private,
                hostname: "a.i2p".to_owned(),
                destination: Some(destination_text()),
                delete: false,
            })
            .expect("entry");
        let mut bytes = encode_generation(&book);
        // Wrong version.
        let mut as_value: serde_json::Value =
            serde_json::from_slice(&encode_generation(&book)).expect("json");
        as_value["version"] = serde_json::json!(2);
        assert_eq!(
            decode_generation(&serde_json::to_vec(&as_value).expect("json")),
            Err(AddressBookError::InvalidGeneration)
        );
        // Truncated bytes.
        bytes.truncate(bytes.len() / 2);
        assert_eq!(
            decode_generation(&bytes),
            Err(AddressBookError::InvalidGeneration)
        );
        // Not JSON at all.
        assert_eq!(
            decode_generation(b"not json"),
            Err(AddressBookError::InvalidGeneration)
        );
        // Over the byte ceiling.
        assert_eq!(
            decode_generation(&vec![b' '; MAX_GENERATION_BYTES + 1]),
            Err(AddressBookError::InvalidGeneration)
        );
        // Hostile entry smuggled into JSON.
        let mut hostile: serde_json::Value =
            serde_json::from_slice(&encode_generation(&book)).expect("json");
        hostile["private"]["evil..i2p"] = serde_json::json!("QUJD");
        assert_eq!(
            decode_generation(&serde_json::to_vec(&hostile).expect("json")),
            Err(AddressBookError::InvalidGeneration)
        );
    }
}
