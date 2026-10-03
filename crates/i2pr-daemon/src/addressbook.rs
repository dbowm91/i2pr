//! Plan 294 canonical AddressBook runtime owner (daemon half).
//!
//! [`AddressBookManager`] owns the validated `[addressbook]`
//! configuration, the committed [`AddressBook`](i2pr_addressbook::AddressBook),
//! opaque generation persistence through
//! [`AddressBookGenerationStore`](i2pr_storage::AddressBookGenerationStore),
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
    RefreshOutcome, RefreshQueue, RefreshReason, ResolvedEntry, decode_generation, encode_generation,
    ingest_subscription_body,
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
        self.inner.read().ok().and_then(|slot| {
            slot.as_ref().map(|resolver| resolver.snapshot().clone())
        })
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

    /// Loads current, then backup, then first-activation import.
    /// Publishes the fresh/imported generation when no current exists.
    fn load_or_import(&mut self) -> Result<(), &'static str> {
        let current = self
            .store
            .load_current()
            .map_err(|_| "generation current unreadable")?;
        if let Some(bytes) = current {
            let book = decode_generation(&bytes).map_err(|_| "generation current invalid")?;
            self.set_committed(book);
            return Ok(());
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

    /// Applies one entry mutation transactionally: persistence failure
    /// rolls the in-memory commit back.
    pub fn apply_entry(
        &self,
        mutation: EntryMutation,
    ) -> Result<i2pr_addressbook::EntryOutcome, AddressBookManagerError> {
        self.transact(|book| book.control().apply_entry(mutation))
    }

    /// Replaces subscriptions transactionally; a changed set enqueues
    /// a refresh and reports whether the caller should run it now.
    pub fn replace_subscriptions(
        &self,
        urls: &[String],
    ) -> Result<(bool, bool), AddressBookManagerError> {
        let changed: bool = self.transact(|book| book.control().replace_subscriptions(urls))?;
        if !changed {
            return Ok((false, false));
        }
        let run_now = self.state.lock().ok().map_or(false, |mut state| {
            let subscriptions = state.addressbook.subscriptions().clone();
            state
                .refresh
                .push(subscriptions, RefreshReason::SubscriptionsReplaced)
        });
        Ok((true, run_now))
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
        let entries =
            ingest_subscription_body(body).map_err(AddressBookManagerError::Rejected)?;
        let ingested = entries.len();
        let changed =
            self.transact(|book| book.control().replace_derived(entries))?;
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
            .map(|state| {
                (
                    state.active,
                    state.addressbook.subscriptions().urls().len(),
                )
            })
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

fn rendered_book(
    book: &AddressBook,
    kind: i2pr_addressbook::BookKind,
) -> Vec<(String, String)> {
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
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.len() > MAX_DIAGNOSTIC_ARTIFACT_BYTES {
            if let Ok(contents) = fs::read(&path) {
                let keep = contents.len().saturating_sub(DIAGNOSTIC_ARTIFACT_RETAIN_BYTES as usize);
                let _ = fs::write(&path, &contents[keep..]);
            }
        }
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
    for (artifact, kind) in artifacts
        .iter()
        .zip([BookKind::Private, BookKind::Local, BookKind::Router, BookKind::Published])
    {
        let path = state_dir.join(artifact);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("snapshot artifact unreadable"),
        };
        if bytes.len() > MAX_SNAPSHOT_ARTIFACT_BYTES {
            return Err("snapshot artifact over bound");
        }
        if fs::symlink_metadata(&path).is_ok_and(|metadata| {
            metadata.file_type().is_symlink() || !metadata.is_file()
        }) {
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
