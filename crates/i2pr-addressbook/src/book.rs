//! Four administrative books, precedence lookup, and the control handle.
//!
//! Precedence is fixed: private, then local, then router, then
//! published, then subscription-derived entries. The same hostname may
//! intentionally appear in multiple books; lookup uses the first match.
//! Mutation goes only through [`AddressBookControl`]; reads and
//! snapshots stay available on [`AddressBook`] itself.

use std::collections::BTreeMap;

use crate::config::AddressBookConfig;
use crate::destination_text::validate_destination_text;
use crate::error::AddressBookError;
use crate::generation::MAX_ENTRIES_PER_BOOK;
use crate::hostname::{Hostname, parse_hostname};
use crate::subscription::SubscriptionSet;

/// One of the four administrative books, in lookup-precedence order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookKind {
    /// Operator-private entries (highest precedence).
    Private,
    /// Locally pinned entries.
    Local,
    /// Router-distributed entries.
    Router,
    /// Published entries (lowest book precedence).
    Published,
}

impl BookKind {
    /// Canonical wire spelling (matches the frozen book inventory).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Local => "local",
            Self::Router => "router",
            Self::Published => "published",
        }
    }

    /// Precedence index (lower wins; matches snapshot array order).
    pub(crate) const fn precedence_index(self) -> usize {
        match self {
            Self::Private => 0,
            Self::Local => 1,
            Self::Router => 2,
            Self::Published => 3,
        }
    }

    /// Parses an exact book spelling.
    pub fn parse(name: &str) -> Result<Self, AddressBookError> {
        match name {
            "private" => Ok(Self::Private),
            "local" => Ok(Self::Local),
            "router" => Ok(Self::Router),
            "published" => Ok(Self::Published),
            _ => Err(AddressBookError::MalformedField),
        }
    }
}

/// Where a resolved destination came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Provenance {
    /// Operator book entry (highest precedence first).
    Book(BookKind),
    /// Subscription-derived entry (consulted last).
    Subscribed,
}

/// One resolved naming answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEntry {
    /// Canonical hostname that was looked up.
    pub hostname: Hostname,
    /// Stored full-Destination text.
    pub destination: String,
    /// Which table answered.
    pub provenance: Provenance,
}

/// One entry-mutation request (already field-split by the caller).
pub struct EntryMutation {
    /// Target book.
    pub book: BookKind,
    /// Raw hostname text.
    pub hostname: String,
    /// Raw destination text (`None` for deletions).
    pub destination: Option<String>,
    /// Delete flag (presence selects deletion regardless of value).
    pub delete: bool,
}

/// Outcome of one committed entry mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryOutcome {
    /// A new hostname was stored.
    Created,
    /// An existing hostname was replaced.
    Updated,
    /// An existing hostname was removed.
    Deleted,
}

/// Canonical address-book state. Read methods are available directly;
/// mutation requires [`AddressBookControl`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressBook {
    books: [BTreeMap<Hostname, String>; 4],
    subscribed: BTreeMap<Hostname, String>,
    subscriptions: SubscriptionSet,
    config: AddressBookConfig,
    revision: u64,
}

impl AddressBook {
    /// Empty state with default configuration at revision zero.
    pub fn new() -> Self {
        Self {
            books: Default::default(),
            subscribed: BTreeMap::new(),
            subscriptions: SubscriptionSet::new(),
            config: AddressBookConfig::default(),
            revision: 0,
        }
    }

    /// Read-only mutation handle guard: mutations apply through this
    /// handle only, so call sites name the administrative path.
    pub fn control(&mut self) -> AddressBookControl<'_> {
        AddressBookControl { inner: self }
    }

    /// Looks up one hostname under precedence order. Invalid names
    /// resolve to `None` (never an error on the lookup path).
    pub fn lookup(&self, raw: &str) -> Option<ResolvedEntry> {
        let hostname = parse_hostname(raw).ok()?;
        for book in [
            BookKind::Private,
            BookKind::Local,
            BookKind::Router,
            BookKind::Published,
        ] {
            if let Some(destination) = self.books[book.precedence_index()].get(&hostname) {
                return Some(ResolvedEntry {
                    hostname,
                    destination: destination.clone(),
                    provenance: Provenance::Book(book),
                });
            }
        }
        self.subscribed.get(&hostname).map(|destination| ResolvedEntry {
            hostname,
            destination: destination.clone(),
            provenance: Provenance::Subscribed,
        })
    }

    /// Lists one book's entries in hostname order.
    pub fn list(&self, book: BookKind) -> Vec<(Hostname, String)> {
        self.books[book.precedence_index()]
            .iter()
            .map(|(name, destination)| (name.clone(), destination.clone()))
            .collect()
    }

    /// Entry count of one book.
    pub fn book_len(&self, book: BookKind) -> usize {
        self.books[book.precedence_index()].len()
    }

    /// Subscription-derived entry count.
    pub fn subscribed_len(&self) -> usize {
        self.subscribed.len()
    }

    /// Subscription-derived table (snapshot capture only).
    pub(crate) fn derived_table(&self) -> &BTreeMap<Hostname, String> {
        &self.subscribed
    }

    /// Current subscription set.
    pub fn subscriptions(&self) -> &SubscriptionSet {
        &self.subscriptions
    }

    /// Current configuration.
    pub fn config(&self) -> &AddressBookConfig {
        &self.config
    }

    /// Committed revision (saturating increment per mutation).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Restores a persisted revision (generation decode only).
    pub(crate) fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

impl Default for AddressBook {
    fn default() -> Self {
        Self::new()
    }
}

/// Administrative mutation handle: the only path that changes
/// committed address-book state.
pub struct AddressBookControl<'a> {
    inner: &'a mut AddressBook,
}

impl AddressBookControl<'_> {
    /// Applies one entry mutation atomically: the hostname and (when
    /// present) destination validate fully before any state changes.
    /// A present delete flag selects deletion even with a false-ish
    /// value; combining delete with a destination fails the whole
    /// request rather than guessing.
    pub fn apply_entry(&mut self, mutation: EntryMutation) -> Result<EntryOutcome, AddressBookError> {
        if mutation.delete && mutation.destination.is_some() {
            return Err(AddressBookError::MixedShapes);
        }
        let hostname = parse_hostname(&mutation.hostname)?;
        if mutation.delete {
            let removed = self.inner.books[mutation.book.precedence_index()].remove(&hostname);
            if removed.is_none() {
                return Err(AddressBookError::UnknownHostname);
            }
            self.inner.bump_revision();
            return Ok(EntryOutcome::Deleted);
        }
        let destination_text = mutation.destination.ok_or(AddressBookError::MalformedField)?;
        let decoded = validate_destination_text(&destination_text)?;
        let _ = decoded;
        let book = &mut self.inner.books[mutation.book.precedence_index()];
        let created = !book.contains_key(&hostname);
        if created {
            let ceiling = self
                .inner
                .config
                .max_entries
                .clamp(1, MAX_ENTRIES_PER_BOOK);
            if book.len() >= ceiling {
                return Err(AddressBookError::BookFull);
            }
        }
        book.insert(hostname, destination_text);
        self.inner.bump_revision();
        Ok(if created {
            EntryOutcome::Created
        } else {
            EntryOutcome::Updated
        })
    }

    /// Replaces the whole subscription set atomically: every URL
    /// validates before the swap. Returns whether the set changed.
    pub fn replace_subscriptions(&mut self, urls: &[String]) -> Result<bool, AddressBookError> {
        let next = SubscriptionSet::checked(urls)?;
        let changed = next != self.inner.subscriptions;
        if changed {
            self.inner.subscriptions = next;
            self.inner.bump_revision();
        }
        Ok(changed)
    }

    /// Replaces subscription-derived entries wholesale (ingestion
    /// output). Operator books are untouched, so operator deletions
    /// can never resurrect from a later download.
    pub fn replace_derived(&mut self, entries: BTreeMap<Hostname, String>) -> Result<bool, AddressBookError> {
        let changed = entries != self.inner.subscribed;
        if changed {
            self.inner.subscribed = entries;
            self.inner.bump_revision();
        }
        Ok(changed)
    }

    /// Applies a whole `SetConfig` map atomically: every key parses
    /// and every value validates before the swap. Unknown keys fail
    /// the request with the configuration untouched.
    pub fn apply_config(
        &mut self,
        entries: &BTreeMap<String, String>,
    ) -> Result<bool, AddressBookError> {
        let next = self.inner.config.checked_update(entries)?;
        // A tighter entry ceiling must still admit current state.
        for book in [
            BookKind::Private,
            BookKind::Local,
            BookKind::Router,
            BookKind::Published,
        ] {
            if self.inner.books[book.precedence_index()].len() > next.max_entries {
                return Err(AddressBookError::InvalidConfigValue);
            }
        }
        let changed = next != self.inner.config;
        if changed {
            self.inner.config = next;
            self.inner.bump_revision();
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64ct::Encoding;

    fn destination_text() -> String {
        // 384-byte key area + type-5 (Ed25519, X25519) key certificate.
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

    fn entry(book: BookKind, hostname: &str, destination: Option<String>, delete: bool) -> EntryMutation {
        EntryMutation {
            book,
            hostname: hostname.to_owned(),
            destination,
            delete,
        }
    }

    #[test]
    fn upsert_and_delete_round_trip() {
        let mut book = AddressBook::new();
        let destination = destination_text();
        let mut control = book.control();
        assert_eq!(
            control
                .apply_entry(entry(BookKind::Private, "a.i2p", Some(destination.clone()), false))
                .expect("create"),
            EntryOutcome::Created
        );
        assert_eq!(
            control
                .apply_entry(entry(BookKind::Private, "a.i2p", Some(destination.clone()), false))
                .expect("update"),
            EntryOutcome::Updated
        );
        assert_eq!(
            control
                .apply_entry(entry(BookKind::Private, "a.i2p", None, true))
                .expect("delete"),
            EntryOutcome::Deleted
        );
        assert_eq!(
            control.apply_entry(entry(BookKind::Private, "a.i2p", None, true)),
            Err(AddressBookError::UnknownHostname)
        );
    }

    #[test]
    fn delete_with_destination_is_a_shape_error() {
        let mut book = AddressBook::new();
        let destination = destination_text();
        let error = book
            .control()
            .apply_entry(entry(BookKind::Local, "a.i2p", Some(destination), true))
            .expect_err("mixed shapes fail");
        assert_eq!(error, AddressBookError::MixedShapes);
        assert!(book.lookup("a.i2p").is_none());
    }

    #[test]
    fn precedence_is_private_local_router_published_subscribed() {
        let mut book = AddressBook::new();
        let destination = destination_text();
        let mut control = book.control();
        for kind in [BookKind::Published, BookKind::Router, BookKind::Local, BookKind::Private] {
            control
                .apply_entry(entry(kind, "same.i2p", Some(destination.clone()), false))
                .expect("insert");
        }
        control
            .replace_derived(BTreeMap::from([(
                parse_hostname("same.i2p").expect("name"),
                destination.clone(),
            )]))
            .expect("derived");
        let resolved = book.lookup("same.i2p").expect("hit");
        assert_eq!(resolved.provenance, Provenance::Book(BookKind::Private));
        // Removing higher books reveals the next layer each time.
        for (kind, expected) in [
            (BookKind::Private, Provenance::Book(BookKind::Local)),
            (BookKind::Local, Provenance::Book(BookKind::Router)),
            (BookKind::Router, Provenance::Book(BookKind::Published)),
            (BookKind::Published, Provenance::Subscribed),
        ] {
            book.control()
                .apply_entry(entry(kind, "same.i2p", None, true))
                .expect("delete");
            let resolved = book.lookup("same.i2p").expect("next layer");
            assert_eq!(resolved.provenance, expected);
        }
    }

    #[test]
    fn same_name_lives_in_many_books() {
        let mut book = AddressBook::new();
        let destination = destination_text();
        for kind in [BookKind::Private, BookKind::Published] {
            book.control()
                .apply_entry(entry(kind, "dup.i2p", Some(destination.clone()), false))
                .expect("insert");
        }
        assert_eq!(book.book_len(BookKind::Private), 1);
        assert_eq!(book.book_len(BookKind::Published), 1);
    }

    #[test]
    fn invalid_inputs_change_nothing() {
        let mut book = AddressBook::new();
        let before = book.revision();
        assert_eq!(
            book.control()
                .apply_entry(entry(BookKind::Private, "not a name", None, true)),
            Err(AddressBookError::InvalidHostname)
        );
        assert_eq!(
            book.control().apply_entry(entry(
                BookKind::Private,
                "a.i2p",
                Some("nope".to_owned()),
                false
            )),
            Err(AddressBookError::InvalidDestination)
        );
        assert_eq!(book.revision(), before);
        assert!(book.lookup("a.i2p").is_none());
    }

    #[test]
    fn unknown_book_spelling_fails() {
        assert_eq!(BookKind::parse("Private"), Err(AddressBookError::MalformedField));
        assert_eq!(BookKind::parse("router").expect("book"), BookKind::Router);
    }
}
