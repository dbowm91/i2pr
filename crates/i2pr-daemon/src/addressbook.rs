//! Plan 294 canonical AddressBook runtime owner (daemon half).
//!
//! [`AddressBookManager`] owns the validated `[addressbook]`
//! configuration, the committed [`AddressBook`],
//! opaque generation persistence through
//! [`AddressBookGenerationStore`],
//! the published read-only resolver snapshot shared with SAM,
//! service-tunnel, and inspection consumers, the bounded refresh queue
//! plus diagnostic artifact, and first-activation snapshot import.
//!
//! Activation rule (documented, tested):
//!
//! - Disabled (default): the manager never touches the filesystem and
//!   publishes no resolver. SAM naming, static aliases, and control
//!   getters behave exactly as before Plan 294.
//! - Enabled: load current, then backup; any decodable generation
//!   activates. With neither file present, bounded per-book snapshot
//!   artifacts are imported when present (a corrupt artifact fails
//!   activation); the resulting generation is published. Corrupt
//!   current *and* backup fail the subsystem into an inactive
//!   sticky-error state: getters report unavailable-with-reason and
//!   resolvers stay unpublished (legacy naming), never partial or
//!   fabricated state.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use i2pr_addressbook::{
    AddressBook, AddressBookResolver, AddressBookSnapshot, EntryMutation, RefreshDiagnostic,
    RefreshOutcome, RefreshQueue, RefreshReason, ResolvedEntry, decode_generation,
    encode_generation, ingest_subscription_body,
};
use i2pr_storage::AddressBookGenerationStore;

/// Maximum bytes read from one snapshot artifact file.
pub const MAX_SNAPSHOT_ARTIFACT_BYTES: usize = 8_000_000;
/// Maximum bytes retained in the diagnostic artifact.
pub const MAX_DIAGNOSTIC_ARTIFACT_BYTES: u64 = 65_536;
/// Retained tail when the diagnostic artifact is truncated.
pub const DIAGNOSTIC_ARTIFACT_RETAIN_BYTES: u64 = 32_768;

/// Validated `[addressbook]` subsystem configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressBookSubsystemConfig {
    /// Whether the subsystem activates at startup.
    pub enabled: bool,
    /// State directory (resolved; never touched while disabled).
    pub state_dir: PathBuf,
}

/// Narrow shared resolver cell: one `Arc` shared by the manager and
/// every consumer, so commits propagate without re-installation.
#[derive(Clone, Debug, Default)]
pub struct SharedAddressBook {
    inner: Arc<RwLock<Option<AddressBookResolver>>>,
}

impl SharedAddressBook {
    /// Empty cell (no resolver published).
    pub fn new() -> Self {
        Self::default()
    }

    /// Publishes a fresh resolver snapshot.
    pub fn publish(&self, resolver: AddressBookResolver) {
        if let Ok(mut slot) = self.inner.write() {
            *slot = Some(resolver);
        }
    }

    /// Looks up one hostname (`None` when unpublished or absent).
    pub fn lookup(&self, name: &str) -> Option<ResolvedEntry> {
        self.inner
            .read()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|resolver| resolver.lookup(name)))
    }

    /// Clones the published snapshot, if any.
    pub fn snapshot_cloned(&self) -> Option<AddressBookSnapshot> {
        self.inner
            .read()
            .ok()
            .and_then(|slot| slot.as_ref().map(|resolver| resolver.snapshot().clone()))
    }

    /// Published revision, if any.
    pub fn revision(&self) -> Option<u64> {
        self.inner
            .read()
            .ok()
            .and_then(|slot| slot.as_ref().map(AddressBookResolver::revision))
    }
}

/// Errors from address-book administration (static strings only; no
/// hostnames, destinations, URLs, or values are ever rendered).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddressBookManagerError {
    /// The subsystem is disabled or failed activation.
    Inactive,
    /// Generation persistence failed (detail is traced, not returned).
    StoreUnavailable,
    /// The mutation itself was rejected.
    Rejected(i2pr_addressbook::AddressBookError),
}

impl core::fmt::Display for AddressBookManagerError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Inactive => formatter.write_str("address-book subsystem is not active"),
            Self::StoreUnavailable => formatter.write_str("address-book generation store failed"),
            Self::Rejected(error) => write!(formatter, "address-book request rejected: {error}"),
        }
    }
}

/// Committed-manager state behind one lock.
struct ManagerState {
    active: bool,
    addressbook: AddressBook,
    refresh: RefreshQueue,
    activation_error: Option<&'static str>,
}

/// Canonical AddressBook runtime owner.
pub struct AddressBookManager {
    config: AddressBookSubsystemConfig,
    store: AddressBookGenerationStore,
    state: Mutex<ManagerState>,
    shared: SharedAddressBook,
}

impl std::fmt::Debug for AddressBookManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AddressBookManager")
            .field("config", &self.config)
            .field("active", &self.is_active())
            .finish_non_exhaustive()
    }
}

impl AddressBookManager {
    /// Activates the subsystem under the activation rule. Disabled
    /// managers never touch the filesystem.
    pub fn activate(config: AddressBookSubsystemConfig) -> Self {
        let store = AddressBookGenerationStore::new(&config.state_dir);
        let shared = SharedAddressBook::new();
        if !config.enabled {
            return Self {
                config,
                store,
                state: Mutex::new(ManagerState {
                    active: false,
                    addressbook: AddressBook::new(),
                    refresh: RefreshQueue::new(),
                    activation_error: None,
                }),
                shared,
            };
        }
        let mut manager = Self {
            config,
            store,
            state: Mutex::new(ManagerState {
                active: false,
                addressbook: AddressBook::new(),
                refresh: RefreshQueue::new(),
                activation_error: None,
            }),
            shared,
        };
        match manager.load_or_import() {
            Ok(()) => manager.publish_snapshot(),
            Err(reason) => manager.deactivate_with(reason),
        }
        manager
    }

    /// Loads current, then backup, then first-activation import. A
    /// present-but-invalid current falls back to the backup (the
    /// last-known-good copy exists for exactly this case); only a
    /// missing current with no usable backup reaches import. The
    /// fresh/imported generation publishes when no current exists.
    fn load_or_import(&mut self) -> Result<(), &'static str> {
        let current = self
            .store
            .load_current()
            .map_err(|_| "generation current unreadable")?;
        if let Some(bytes) = current {
            if let Ok(book) = decode_generation(&bytes) {
                self.set_committed(book);
                return Ok(());
            }
            let backup = self
                .store
                .load_backup()
                .map_err(|_| "generation backup unreadable")?;
            if let Some(backup_bytes) = backup {
                let book =
                    decode_generation(&backup_bytes).map_err(|_| "generation backup invalid")?;
                self.set_committed(book);
                return Ok(());
            }
            return Err("generation current invalid");
        }
        let backup = self
            .store
            .load_backup()
            .map_err(|_| "generation backup unreadable")?;
        if let Some(bytes) = backup {
            let book = decode_generation(&bytes).map_err(|_| "generation backup invalid")?;
            self.set_committed(book);
            return Ok(());
        }
        let mut book = AddressBook::new();
        let imported = import_snapshot_artifacts(&self.config.state_dir, &mut book)
            .map_err(|_| "snapshot artifact invalid")?;
        let _ = imported;
        let bytes = encode_generation(&book);
        self.store
            .publish(&bytes)
            .map_err(|_| "initial generation unpublished")?;
        self.set_committed(book);
        Ok(())
    }

    fn set_committed(&mut self, book: AddressBook) {
        if let Ok(mut state) = self.state.lock() {
            state.addressbook = book;
            state.active = true;
            state.activation_error = None;
        }
    }

    fn publish_snapshot(&self) {
        let snapshot = self
            .state
            .lock()
            .ok()
            .map(|state| AddressBookSnapshot::capture(&state.addressbook));
        if let Some(snapshot) = snapshot {
            self.shared.publish(AddressBookResolver::new(snapshot));
        }
    }

    fn deactivate_with(&mut self, reason: &'static str) {
        if let Ok(mut state) = self.state.lock() {
            state.active = false;
            state.addressbook = AddressBook::new();
            state.activation_error = Some(reason);
        }
    }

    /// Whether the subsystem is active (disabled or failed otherwise).
    pub fn is_active(&self) -> bool {
        self.state.lock().ok().is_some_and(|state| state.active)
    }

    /// Sticky activation failure, if the subsystem is enabled but down.
    pub fn activation_error(&self) -> Option<&'static str> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.activation_error)
    }

    /// Shared resolver cell (clones share one `Arc`).
    pub fn shared(&self) -> SharedAddressBook {
        self.shared.clone()
    }

    /// Committed revision, if active.
    pub fn revision(&self) -> Option<u64> {
        self.state.lock().ok().and_then(|state| {
            if state.active {
                Some(state.addressbook.revision())
            } else {
                None
            }
        })
    }

    /// Live refresh cadence in hours (drives the worker timer).
    pub fn refresh_interval_hours(&self) -> u64 {
        self.state
            .lock()
            .ok()
            .map(|state| state.addressbook.config().refresh_interval_hours)
            .unwrap_or(24)
    }

    /// Applies one entry mutation transactionally: persistence failure
    /// rolls the in-memory commit back.
    pub fn apply_entry(
        &self,
        mutation: EntryMutation,
    ) -> Result<i2pr_addressbook::EntryOutcome, AddressBookManagerError> {
        self.transact(|book| book.control().apply_entry(mutation))
    }

    /// Replaces subscriptions transactionally. A changed set is
    /// drained through [`Self::run_queued_refreshes`] by the caller;
    /// the queue coalesces concurrent commits to the newest set.
    pub fn replace_subscriptions(&self, urls: &[String]) -> Result<bool, AddressBookManagerError> {
        self.transact(|book| book.control().replace_subscriptions(urls))
    }

    /// Applies a whole `SetConfig` map transactionally.
    pub fn apply_config(
        &self,
        entries: &BTreeMap<String, String>,
    ) -> Result<bool, AddressBookManagerError> {
        self.transact(|book| book.control().apply_config(entries))
    }

    /// Ingests one subscription body transactionally (fetch is
    /// daemon-composed; tests and the worker drive this seam).
    pub fn ingest_body(&self, body: &[u8]) -> Result<IngestReport, AddressBookManagerError> {
        let entries = ingest_subscription_body(body).map_err(AddressBookManagerError::Rejected)?;
        let ingested = entries.len();
        let changed = self.transact(|book| book.control().replace_derived(entries))?;
        Ok(IngestReport { ingested, changed })
    }

    /// Runs one refresh attempt end-to-end for `set`: with no
    /// downloader owner the fetch stage reports unavailable (never a
    /// socket, never an error that rewrites history).
    pub fn run_refresh_once(&self, reason: RefreshReason) -> RefreshDiagnostic {
        let (active, url_count) = self
            .state
            .lock()
            .ok()
            .map(|state| (state.active, state.addressbook.subscriptions().urls().len()))
            .unwrap_or((false, 0));
        if !active {
            return RefreshDiagnostic {
                reason,
                outcome: RefreshOutcome::DownloaderUnavailable,
                url_count,
            };
        }
        // No downloader owner is composed: the fetch stage is
        // explicitly unavailable. Ingestion stays reachable through
        // ingest_body for bodies obtained by other means.
        RefreshDiagnostic {
            reason,
            outcome: RefreshOutcome::DownloaderUnavailable,
            url_count,
        }
    }

    /// Advances the refresh queue after one attempt: promotes a
    /// coalesced pending set and returns it for immediate fetching.
    pub fn refresh_finished(&self) -> Option<i2pr_addressbook::SubscriptionSet> {
        self.state
            .lock()
            .ok()
            .and_then(|mut state| state.refresh.finish_active())
    }

    /// Enqueues the committed subscription set for refresh. Returns
    /// `true` when the queue was idle and the caller should drain,
    /// `false` when the set coalesced (or the subsystem is down).
    pub fn enqueue_current_for_refresh(&self, reason: RefreshReason) -> bool {
        let mut state = match self.state.lock() {
            Ok(state) if state.active => state,
            _ => return false,
        };
        let subscriptions = state.addressbook.subscriptions().clone();
        state.refresh.push(subscriptions, reason)
    }

    /// Takes the queued set for immediate fetching, if any.
    pub fn take_refresh_work(&self) -> Option<i2pr_addressbook::SubscriptionSet> {
        self.state
            .lock()
            .ok()
            .and_then(|mut state| state.refresh.take_active())
    }

    /// Runs the queued refresh chain to idle: every attempt records
    /// its diagnostic to the bounded artifact.
    pub fn run_queued_refreshes(&self, reason: RefreshReason) {
        if !self.enqueue_current_for_refresh(reason) && self.take_refresh_work().is_none() {
            return;
        }
        while let Some(_set) = self.take_refresh_work() {
            let diagnostic = self.run_refresh_once(reason);
            self.record_diagnostic(&diagnostic);
            let _ = self.refresh_finished();
        }
    }

    /// Records one diagnostic to the bounded artifact (level-gated;
    /// artifact only, never global tracing).
    pub fn record_diagnostic(&self, diagnostic: &RefreshDiagnostic) {
        let (level, log_file) = match self.state.lock() {
            Ok(state) if state.active => (
                state.addressbook.config().log_level,
                state.addressbook.config().log_file.clone(),
            ),
            _ => return,
        };
        if !level_allows(level, &diagnostic.outcome) {
            return;
        }
        append_bounded_line(&self.config.state_dir, &log_file, &diagnostic.line());
    }

    /// Renders the committed books for getters (hostname order).
    pub fn books_view(&self) -> Option<[Vec<(String, String)>; 4]> {
        self.state.lock().ok().and_then(|state| {
            if !state.active {
                return None;
            }
            Some([
                rendered_book(&state.addressbook, i2pr_addressbook::BookKind::Private),
                rendered_book(&state.addressbook, i2pr_addressbook::BookKind::Local),
                rendered_book(&state.addressbook, i2pr_addressbook::BookKind::Router),
                rendered_book(&state.addressbook, i2pr_addressbook::BookKind::Published),
            ])
        })
    }

    /// Renders committed subscriptions + config for getters.
    pub fn config_view(&self) -> Option<(Vec<String>, BTreeMap<String, String>)> {
        self.state.lock().ok().and_then(|state| {
            if !state.active {
                return None;
            }
            Some((
                state.addressbook.subscriptions().urls().to_vec(),
                state.addressbook.config().rendered_entries(),
            ))
        })
    }

    /// Runs `f` against committed state and persists + republishes on
    /// success; persistence failure rolls the commit back.
    fn transact<T>(
        &self,
        f: impl FnOnce(&mut AddressBook) -> Result<T, i2pr_addressbook::AddressBookError>,
    ) -> Result<T, AddressBookManagerError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AddressBookManagerError::Inactive)?;
        if !state.active {
            return Err(AddressBookManagerError::Inactive);
        }
        let rollback = state.addressbook.clone();
        let output = f(&mut state.addressbook).map_err(AddressBookManagerError::Rejected)?;
        let bytes = encode_generation(&state.addressbook);
        if self.store.publish(&bytes).is_err() {
            state.addressbook = rollback;
            return Err(AddressBookManagerError::StoreUnavailable);
        }
        let snapshot = AddressBookSnapshot::capture(&state.addressbook);
        drop(state);
        self.shared.publish(AddressBookResolver::new(snapshot));
        Ok(output)
    }
}

/// Outcome of one ingested subscription body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IngestReport {
    /// Entries the body produced.
    pub ingested: usize,
    /// Whether derived entries changed.
    pub changed: bool,
}

fn rendered_book(book: &AddressBook, kind: i2pr_addressbook::BookKind) -> Vec<(String, String)> {
    book.list(kind)
        .into_iter()
        .map(|(name, destination)| (name.as_str().to_owned(), destination))
        .collect()
}

/// Whether `level` records `outcome` in the artifact.
fn level_allows(level: i2pr_addressbook::LogLevel, outcome: &RefreshOutcome) -> bool {
    use i2pr_addressbook::LogLevel as Level;
    let event: Level = match outcome {
        RefreshOutcome::Committed { .. } => Level::Info,
        RefreshOutcome::DownloaderUnavailable => Level::Warn,
        RefreshOutcome::IngestRejected | RefreshOutcome::FetchFailed => Level::Error,
    };
    level_rank(level) >= level_rank(event)
}

fn level_rank(level: i2pr_addressbook::LogLevel) -> u8 {
    use i2pr_addressbook::LogLevel as Level;
    match level {
        Level::Off => 0,
        Level::Error => 1,
        Level::Warn => 2,
        Level::Info => 3,
        Level::Debug => 4,
    }
}

/// Appends one line to the confined diagnostic artifact, truncating to
/// the retained tail past the ceiling. Failures are silent: diagnostics
/// never fail the subsystem.
fn append_bounded_line(state_dir: &Path, log_file: &str, line: &str) {
    let path = state_dir.join(log_file);
    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&path) {
        Ok(file) => file,
        Err(_) => return,
    };
    // Symlink/special-file guard on the opened path.
    if fs::symlink_metadata(&path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return;
    }
    let _ = writeln!(file, "{line}");
    drop(file);
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && metadata.len() > MAX_DIAGNOSTIC_ARTIFACT_BYTES
        && let Ok(contents) = fs::read(&path)
    {
        let keep = contents
            .len()
            .saturating_sub(DIAGNOSTIC_ARTIFACT_RETAIN_BYTES as usize);
        let _ = fs::write(&path, &contents[keep..]);
    }
}

/// Imports first-activation per-book snapshot artifacts: JSON objects
/// mapping hostname to destination text. Missing files mean empty
/// books; any present-but-invalid file fails the whole import.
fn import_snapshot_artifacts(
    state_dir: &Path,
    book: &mut AddressBook,
) -> Result<usize, &'static str> {
    use i2pr_addressbook::BookKind;
    let artifacts = book.config().book_artifacts.clone();
    let mut imported = 0;
    for (artifact, kind) in artifacts.iter().zip([
        BookKind::Private,
        BookKind::Local,
        BookKind::Router,
        BookKind::Published,
    ]) {
        let path = state_dir.join(artifact);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("snapshot artifact unreadable"),
        };
        if bytes.len() > MAX_SNAPSHOT_ARTIFACT_BYTES {
            return Err("snapshot artifact over bound");
        }
        if fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return Err("snapshot artifact is not a regular file");
        }
        let map: BTreeMap<String, String> =
            serde_json::from_slice(&bytes).map_err(|_| "snapshot artifact malformed")?;
        for (name, destination) in &map {
            book.control()
                .apply_entry(EntryMutation {
                    book: kind,
                    hostname: name.clone(),
                    destination: Some(destination.clone()),
                    delete: false,
                })
                .map_err(|_| "snapshot artifact entry invalid")?;
            imported += 1;
        }
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config(dir: &Path) -> AddressBookSubsystemConfig {
        AddressBookSubsystemConfig {
            enabled: true,
            state_dir: dir.join("addressbook"),
        }
    }

    fn disabled_config(dir: &Path) -> AddressBookSubsystemConfig {
        AddressBookSubsystemConfig {
            enabled: false,
            state_dir: dir.join("addressbook"),
        }
    }

    fn destination_text() -> String {
        // 384-byte key area + type-5 (Ed25519, X25519) key certificate.
        let mut bytes = vec![0u8; 384];
        bytes.extend_from_slice(&[5u8, 0, 4, 0, 7, 0, 4]);
        i2pr_api::sam::base64::encode(&bytes)
    }

    fn entry(
        book: i2pr_addressbook::BookKind,
        hostname: &str,
        destination: Option<String>,
        delete: bool,
    ) -> EntryMutation {
        EntryMutation {
            book,
            hostname: hostname.to_owned(),
            destination,
            delete,
        }
    }

    #[test]
    fn disabled_manager_never_touches_the_filesystem() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = AddressBookManager::activate(disabled_config(directory.path()));
        assert!(!manager.is_active());
        assert_eq!(manager.activation_error(), None);
        assert_eq!(manager.revision(), None);
        assert!(manager.shared().lookup("anything.i2p").is_none());
        assert_eq!(
            manager.apply_entry(entry(
                i2pr_addressbook::BookKind::Private,
                "a.i2p",
                Some(destination_text()),
                false
            )),
            Err(AddressBookManagerError::Inactive)
        );
        // No file, directory, or side effect anywhere under the data dir.
        assert_eq!(fs::read_dir(directory.path()).expect("read dir").count(), 0);
    }

    #[test]
    fn crud_persists_and_reloads_with_stable_revision() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = AddressBookManager::activate(enabled_config(directory.path()));
        assert!(manager.is_active());
        let destination = destination_text();
        manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Local,
                "Example.I2P",
                Some(destination.clone()),
                false,
            ))
            .expect("create");
        let revision = manager.revision().expect("revision");
        assert!(revision > 0);
        // Canonicalization is visible through lookup and getters.
        let resolved = manager.shared().lookup("example.i2p.").expect("lookup");
        assert_eq!(resolved.destination, destination);
        let books = manager.books_view().expect("books");
        assert_eq!(books[1].len(), 1);
        assert_eq!(books[1][0].0, "example.i2p");
        // A fresh manager over the same directory restores everything.
        drop(manager);
        let reloaded = AddressBookManager::activate(enabled_config(directory.path()));
        assert!(reloaded.is_active());
        assert_eq!(reloaded.revision(), Some(revision));
        let resolved = reloaded.shared().lookup("example.i2p").expect("lookup");
        assert_eq!(resolved.destination, destination);
    }

    #[test]
    fn corrupt_current_falls_back_to_backup() {
        let directory = tempfile::tempdir().expect("tempdir");
        let config = enabled_config(directory.path());
        let manager = AddressBookManager::activate(config.clone());
        manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Router,
                "good.i2p",
                Some(destination_text()),
                false,
            ))
            .expect("entry");
        manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Router,
                "good.i2p",
                None,
                true,
            ))
            .expect("delete makes a second generation");
        drop(manager);
        // Corrupt the current generation; the backup restores (it still
        // holds the pre-delete generation with the entry).
        let current = config.state_dir.join("addressbook.current.json");
        fs::write(&current, b"{not json").expect("corrupt current");
        let reloaded = AddressBookManager::activate(config.clone());
        assert!(reloaded.is_active());
        assert!(reloaded.shared().lookup("good.i2p").is_some());
        // Both corrupt: inactive with a sticky error, nothing published.
        fs::write(
            config.state_dir.join("addressbook.backup.json"),
            b"{not json either",
        )
        .expect("corrupt backup");
        let dead = AddressBookManager::activate(config);
        assert!(!dead.is_active());
        assert!(dead.activation_error().is_some());
        assert_eq!(dead.revision(), None);
        assert!(dead.shared().lookup("good.i2p").is_none());
        assert!(dead.books_view().is_none());
    }

    #[test]
    fn snapshot_import_runs_once_on_first_activation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let config = enabled_config(directory.path());
        fs::create_dir_all(&config.state_dir).expect("state dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config.state_dir, fs::Permissions::from_mode(0o700))
                .expect("permissions");
        }
        let mut artifact = BTreeMap::new();
        artifact.insert("seed.i2p".to_owned(), destination_text());
        fs::write(
            config.state_dir.join("private.json"),
            serde_json::to_vec(&artifact).expect("artifact"),
        )
        .expect("write artifact");
        let manager = AddressBookManager::activate(config.clone());
        assert!(manager.is_active());
        assert!(manager.shared().lookup("seed.i2p").is_some());
        // The import published a current generation: a reload keeps the
        // entry even after the artifact disappears.
        fs::remove_file(config.state_dir.join("private.json")).expect("remove");
        drop(manager);
        let reloaded = AddressBookManager::activate(config);
        assert!(reloaded.shared().lookup("seed.i2p").is_some());
    }

    #[test]
    fn corrupt_snapshot_artifact_fails_activation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let config = enabled_config(directory.path());
        fs::create_dir_all(&config.state_dir).expect("state dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config.state_dir, fs::Permissions::from_mode(0o700))
                .expect("permissions");
        }
        fs::write(config.state_dir.join("local.json"), b"nope").expect("write");
        let manager = AddressBookManager::activate(config);
        assert!(!manager.is_active());
        assert!(manager.activation_error().is_some());
    }

    #[test]
    fn subscriptions_enqueue_refresh_and_ingest_commits() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = AddressBookManager::activate(enabled_config(directory.path()));
        // Unchanged set: no refresh, no revision bump.
        let changed = manager.replace_subscriptions(&[]).expect("empty set");
        assert!(!changed);
        let changed = manager
            .replace_subscriptions(&["http://example.i2p/hosts.txt".to_owned()])
            .expect("replace");
        assert!(changed);
        manager.run_queued_refreshes(i2pr_addressbook::RefreshReason::SubscriptionsReplaced);
        // A second commit drains identically through the same chain.
        let changed = manager
            .replace_subscriptions(&["http://other.i2p/hosts.txt".to_owned()])
            .expect("replace again");
        assert!(changed);
        manager.run_queued_refreshes(i2pr_addressbook::RefreshReason::SubscriptionsReplaced);
        // Both chained runs recorded their unavailable diagnostics.
        let log_contents =
            fs::read_to_string(directory.path().join("addressbook").join("addressbook.log"))
                .expect("artifact");
        assert_eq!(
            log_contents
                .lines()
                .filter(|line| line.contains("downloader-unavailable"))
                .count(),
            2
        );
        // Ingestion commits derived entries last in precedence.
        let destination = destination_text();
        let body = format!("sub.i2p={destination}\n");
        let report = manager.ingest_body(body.as_bytes()).expect("ingest");
        assert_eq!(report.ingested, 1);
        assert!(report.changed);
        let resolved = manager.shared().lookup("sub.i2p").expect("derived");
        assert_eq!(
            resolved.provenance,
            i2pr_addressbook::Provenance::Subscribed
        );
        // Operator books shadow derived entries without touching them.
        manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Published,
                "sub.i2p",
                Some(destination.clone()),
                false,
            ))
            .expect("shadow");
        let resolved = manager.shared().lookup("sub.i2p").expect("shadowed");
        assert_eq!(
            resolved.provenance,
            i2pr_addressbook::Provenance::Book(i2pr_addressbook::BookKind::Published)
        );
        manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Published,
                "sub.i2p",
                None,
                true,
            ))
            .expect("unshadow");
        let resolved = manager.shared().lookup("sub.i2p").expect("derived again");
        assert_eq!(
            resolved.provenance,
            i2pr_addressbook::Provenance::Subscribed
        );
        // Invalid bodies fail whole without touching derived state.
        assert!(manager.ingest_body(b"junk\n").is_err());
        assert!(manager.shared().lookup("sub.i2p").is_some());
    }

    #[test]
    fn refresh_without_downloader_is_unavailable_and_logged() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = AddressBookManager::activate(enabled_config(directory.path()));
        let diagnostic = manager.run_refresh_once(i2pr_addressbook::RefreshReason::Manual);
        assert_eq!(
            diagnostic.outcome,
            i2pr_addressbook::RefreshOutcome::DownloaderUnavailable
        );
        assert!(!diagnostic.line().contains("http"));
        manager.record_diagnostic(&diagnostic);
        let log_path = enabled_config(directory.path())
            .state_dir
            .join("addressbook.log");
        let contents = fs::read_to_string(&log_path).expect("artifact");
        assert!(contents.contains("downloader-unavailable"));
        // Inactive managers record nothing.
        let idle = AddressBookManager::activate(disabled_config(directory.path()));
        idle.record_diagnostic(&diagnostic);
        assert_eq!(
            fs::read_dir(directory.path()).expect("read dir").count(),
            1,
            "only the active state dir may exist"
        );
    }

    #[test]
    fn config_tightening_below_current_counts_fails() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = AddressBookManager::activate(enabled_config(directory.path()));
        manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Private,
                "a.i2p",
                Some(destination_text()),
                false,
            ))
            .expect("entry");
        let mut entries = BTreeMap::new();
        entries.insert("max_entries".to_owned(), "1".to_owned());
        assert!(manager.apply_config(&entries).expect("ceiling 1 holds one"));
        entries.insert("max_entries".to_owned(), "0".to_owned());
        assert!(manager.apply_config(&entries).is_err());
        // Failed tightening leaves the committed config untouched.
        let (_, config) = manager.config_view().expect("config");
        assert_eq!(config["max_entries"], "1");
    }

    #[test]
    fn manager_errors_carry_no_request_values() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = AddressBookManager::activate(enabled_config(directory.path()));
        let hostile = "evil-value-that-must-never-echo.i2p";
        let error = manager
            .apply_entry(entry(
                i2pr_addressbook::BookKind::Private,
                hostile,
                Some("not-a-destination-hunter2".to_owned()),
                false,
            ))
            .expect_err("invalid destination");
        let rendered = error.to_string();
        assert!(!rendered.contains(hostile));
        assert!(!rendered.contains("hunter2"));
    }
}
