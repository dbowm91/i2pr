//! Read-only resolver handle: the only naming path consumers see.
//!
//! The daemon publishes a fresh [`AddressBookResolver`] on every
//! committed generation. Cloning the resolver shares the underlying
//! snapshot (no per-lookup lock, no per-consumer copy); later commits
//! never disturb an outstanding handle. The resolver exposes lookup
//! only — no mutation, no configuration, no administrative authority.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::book::{BookKind, Provenance, ResolvedEntry};
use crate::hostname::Hostname;

/// One immutable naming snapshot.
#[derive(Clone, Debug)]
pub struct AddressBookSnapshot {
    books: [BTreeMap<Hostname, String>; 4],
    subscribed: BTreeMap<Hostname, String>,
    revision: u64,
}

impl AddressBookSnapshot {
    /// Captures the snapshot from committed state.
    pub fn capture(book: &crate::book::AddressBook) -> Self {
        Self {
            books: [
                book_snapshot(book, BookKind::Private),
                book_snapshot(book, BookKind::Local),
                book_snapshot(book, BookKind::Router),
                book_snapshot(book, BookKind::Published),
            ],
            subscribed: book.derived_table().clone(),
            revision: book.revision(),
        }
    }

    /// Generation revision this snapshot was captured at.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Total entries across books plus derived.
    pub fn entry_count(&self) -> usize {
        self.books.iter().map(BTreeMap::len).sum::<usize>() + self.subscribed.len()
    }
}

fn book_snapshot(
    book: &crate::book::AddressBook,
    kind: BookKind,
) -> BTreeMap<Hostname, String> {
    book.list(kind)
        .into_iter()
        .collect()
}

/// Narrow read-only resolver shared with SAM, service-tunnel, and
/// control consumers.
#[derive(Clone, Debug)]
pub struct AddressBookResolver {
    snapshot: Arc<AddressBookSnapshot>,
}

impl AddressBookResolver {
    /// Wraps one captured snapshot.
    pub fn new(snapshot: AddressBookSnapshot) -> Self {
        Self {
            snapshot: Arc::new(snapshot),
        }
    }

    /// Looks up one hostname under precedence order.
    pub fn lookup(&self, raw: &str) -> Option<ResolvedEntry> {
        let hostname = crate::hostname::parse_hostname(raw).ok()?;
        for kind in [
            BookKind::Private,
            BookKind::Local,
            BookKind::Router,
            BookKind::Published,
        ] {
            if let Some(destination) = self.snapshot.books[kind.precedence_index()].get(&hostname) {
                return Some(ResolvedEntry {
                    hostname,
                    destination: destination.clone(),
                    provenance: Provenance::Book(kind),
                });
            }
        }
        self.snapshot.subscribed.get(&hostname).map(|destination| ResolvedEntry {
            hostname,
            destination: destination.clone(),
            provenance: Provenance::Subscribed,
        })
    }

    /// Snapshot revision (lets consumers detect commits).
    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::{AddressBook, EntryMutation};
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
    fn snapshots_are_isolated_from_later_commits() {
        let mut book = AddressBook::new();
        book.control()
            .apply_entry(EntryMutation {
                book: BookKind::Local,
                hostname: "keep.i2p".to_owned(),
                destination: Some(destination_text()),
                delete: false,
            })
            .expect("insert");
        let resolver = AddressBookResolver::new(AddressBookSnapshot::capture(&book));
        // A later commit (new entry + delete) never disturbs the handle.
        book.control()
            .apply_entry(EntryMutation {
                book: BookKind::Local,
                hostname: "later.i2p".to_owned(),
                destination: Some(destination_text()),
                delete: false,
            })
            .expect("insert");
        book.control()
            .apply_entry(EntryMutation {
                book: BookKind::Local,
                hostname: "keep.i2p".to_owned(),
                destination: None,
                delete: true,
            })
            .expect("delete");
        assert!(resolver.lookup("keep.i2p").is_some());
        assert!(resolver.lookup("later.i2p").is_none());
        assert!(book.lookup("keep.i2p").is_none());
        let fresh = AddressBookResolver::new(AddressBookSnapshot::capture(&book));
        assert!(fresh.lookup("later.i2p").is_some());
        assert!(fresh.revision() > resolver.revision());
    }
}
