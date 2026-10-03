//! Plan 289 TunnelManager control state and existing service-runtime adapter.
//!
//! This module implements durable administrative ownership over the one
//! existing M10 [`ServiceTunnelManager`]. It never creates a second
//! service/destination/tunnel runtime: every lifecycle action validates
//! a candidate control-owned definition, stages a versioned persistence
//! generation, reconciles the existing manager, publishes the durable
//! generation, and returns success only when durable intent and the
//! authoritative runtime generation agree.
//!
//! Ownership model: `StartupOwned` definitions come from daemon TOML and
//! are inspectable but never mutated here; `ControlOwned` definitions
//! originate from TunnelManager and persist beneath the dedicated
//! `i2pcontrol/tunnels/` state root. Name collisions across classes fail
//! closed, and control state never rewrites `router.toml` or ordinary
//! service-tunnel files.
//!
//! Supported 289 option subset (each with a real effect; every other
//! supplied option fails explicitly as unsupported, never accepted
//! inertly). Key spellings are the frozen Proposal universe from
//! `i2pr-i2pcontrol::TUNNEL_OPTIONS`:
//! - `target_destination` — client kinds: remote [`DestinationRef`];
//! - `target_host` + `target_port` — server kinds: loopback `ip:port`
//!   target, both required (no silent default for where tunneled
//!   traffic exits; `unix:` hosts rejected as not-yet-supported);
//! - `listen_host` — client kinds: loopback IP (default `127.0.0.1`);
//! - `listen_port` — client kinds: `0..=65535` (default `0`, ephemeral);
//! - `start_on_load` — `true`/`false` (default `true`);
//! - `max_streams` — mapped onto the per-service connection ceiling
//!   `1..=128` (default `16`).
//!
//! Profile options for `httpclient`/`socks`/`ircclient` use the reviewed
//! M10 defaults; profile tuning belongs to Plans 290/292.
//!
//! Secret-classified options are all unsupported in Plan 289: they are
//! rejected before storage, so no secret ever reaches the generation
//! files, responses, errors, logs, or `Debug` output.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_i2pcontrol::tunnel::validate_tunnel_name;
use i2pr_i2pcontrol::tunnel_matrix::{CellDisposition, disposition_for};
use i2pr_i2pcontrol::{TunnelAction, TunnelManagerRequest, TunnelType};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, LocalListenerSpec, MAX_SERVICE_TUNNELS,
    MAX_TUNNEL_LENGTH_HOPS, MAX_TUNNEL_QUANTITY, ServerTarget, ServiceTunnelId, ServiceTunnelKind,
    ServiceTunnelSet, ServiceTunnelSpec, TunnelShaping,
};

use crate::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

/// Schema version of the control generation files.
pub const CONTROL_SCHEMA_VERSION: u8 = 1;

/// State root beneath the router data directory.
pub const CONTROL_STATE_SUBDIR: &str = "i2pcontrol";
/// Tunnel control generations beneath the state root.
pub const CONTROL_TUNNELS_SUBDIR: &str = "tunnels";
/// Pointer file naming the published generation.
pub const CONTROL_POINTER_FILE: &str = "current.json";
/// Maximum bytes of one generation file.
pub const MAX_GENERATION_BYTES: usize = 1_048_576;
/// Bounded drain deadline for control reconciles.
///
/// The control-plane drain policy is immediate release: administrative
/// stop/restart/delete frees listeners at once so ports recycle
/// deterministically (stop-then-start and rename sequences rebind the
/// same socket without waiting out a grace window). In-flight
/// connections observe cancellation promptly. This is the bounded
/// policy the transaction coordinator applies; it differs
/// deliberately from the product default, which favors connection
/// grace over port reuse.
pub const CONTROL_DRAIN_DEADLINE: Duration = Duration::ZERO;
/// Hard deadline for reconcile-back after a publish failure.
pub const CONTROL_ROLLBACK_DEADLINE: Duration = Duration::from_secs(5);
/// Plan 289 supported option subset (real effect each; see module docs).
pub const SUPPORTED_289_OPTIONS: [&str; 7] = [
    "target_destination",
    "target_host",
    "target_port",
    "listen_host",
    "listen_port",
    "start_on_load",
    "max_streams",
];
/// Plan 291 Streamr option keys with real effects. `remote_udp_host`
/// stays unsupported until Plan 292 assigns it meaning (never
/// accepted inertly).
pub const SUPPORTED_291_OPTIONS: [&str; 7] = [
    "local_udp_host",
    "local_udp_port",
    "target_i2p_port",
    "streamr_subscribe_interval",
    "streamr_expiry",
    "streamr_max_subscribers",
    "streamr_payload_limit",
];
/// Plan 292 option keys: shaping, lifecycle, proxy authentication,
/// access policy, presentation policy, and the Streamr sink redirect.
/// Membership admits a key to definitions; each key additionally
/// needs a `build_control_spec` owner arm (the `other` arm still
/// rejects ownerless keys before any allocation).
pub const SUPPORTED_292_OPTIONS: [&str; 21] = [
    "tunnel_length",
    "tunnel_quantity",
    "inbound_length",
    "outbound_length",
    "inbound_quantity",
    "outbound_quantity",
    "profile",
    "interactive",
    "idle_timeout",
    "close_on_idle",
    "new_dest_on_idle",
    "reduce_on_idle",
    "proxy_username",
    "proxy_password",
    "access_list",
    "white_list",
    "black_list",
    "address_helper",
    "jump_list",
    "unique_local_address",
    "remote_udp_host",
];
/// Default per-service connection ceiling for control-created tunnels.
pub const DEFAULT_CONTROL_MAX_CONNECTIONS: usize = 16;

/// Definition provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelProvenance {
    /// Originated from daemon TOML/static configuration. Inspectable;
    /// I2PControl mutation is rejected.
    StartupOwned,
    /// Originated from TunnelManager; persisted under the control root.
    ControlOwned,
}

impl TunnelProvenance {
    /// Canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartupOwned => "startup",
            Self::ControlOwned => "control",
        }
    }
}

/// One control-owned tunnel definition (durable intent).
#[derive(Clone, Eq, PartialEq)]
pub struct ControlDefinition {
    /// Tunnel name (unique across both ownership classes).
    pub name: String,
    /// Proposal tunnel type.
    pub tunnel_type: TunnelType,
    /// Normalized option map (289 subset only).
    pub options: BTreeMap<String, String>,
    /// Persisted start-at-startup intent (distinct from running state).
    pub start_on_load: bool,
}

impl core::fmt::Debug for ControlDefinition {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Keys are schema; values are operator data and never printed.
        // Secret-classified keys can never be present (rejected before
        // storage), and this redaction holds regardless.
        formatter
            .debug_struct("ControlDefinition")
            .field("name", &self.name)
            .field("tunnel_type", &self.tunnel_type)
            .field("option_keys", &self.options.keys().collect::<Vec<_>>())
            .field("start_on_load", &self.start_on_load)
            .finish()
    }
}

/// Durable control error. Messages name keys and kinds, never values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlError {
    /// Tunnel name unknown to either ownership class.
    UnknownTunnel(String),
    /// Name collides across ownership classes or within control state.
    NameCollision(String),
    /// Attempted mutation of a startup-owned definition.
    StartupMutationRejected(String),
    /// Proposal type without a Plan 290 runtime backend.
    UnsupportedType(String),
    /// Option outside the Plan 289 supported subset.
    UnsupportedOption(String),
    /// Supported option with an unparsable value.
    InvalidOption {
        /// Option key.
        option: String,
        /// Static reason (never echoes the value).
        reason: &'static str,
    },
    /// Options contradict the tunnel kind.
    ContradictoryOptions {
        /// Tunnel name.
        name: String,
        /// Static reason.
        reason: &'static str,
    },
    /// Malformed request envelope detail (static reason only).
    InvalidRequest(&'static str),
    /// Aggregate candidate set violates M10 ceilings.
    AggregateRejected(&'static str),
    /// Persistence failure.
    Store(StoreError),
    /// Runtime staging/reconcile failure (manager untouched on staging
    /// failure; message carries the manager's static reason only).
    Manager(&'static str),
    /// Durable publication failed after a successful runtime transition;
    /// reconcile-back was attempted (see `reconciled_back`).
    PublishFailed {
        /// Static reason.
        reason: &'static str,
        /// Whether reconcile-back to the prior generation succeeded.
        reconciled_back: bool,
    },
    /// Control state is unavailable (service constructed standalone).
    Unavailable,
}

impl core::fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownTunnel(name) => write!(formatter, "unknown tunnel {name}"),
            Self::NameCollision(name) => write!(formatter, "tunnel name collision for {name}"),
            Self::StartupMutationRejected(name) => {
                write!(
                    formatter,
                    "startup-owned tunnel {name} rejects I2PControl mutation"
                )
            }
            Self::UnsupportedType(kind) => {
                write!(
                    formatter,
                    "tunnel type {kind} has no Plan 290 backend (Plan 290)"
                )
            }
            Self::UnsupportedOption(option) => {
                write!(formatter, "option {option} is not supported in Plan 289")
            }
            Self::InvalidOption { option, reason } => {
                write!(formatter, "invalid option {option}: {reason}")
            }
            Self::ContradictoryOptions { name, reason } => {
                write!(formatter, "contradictory options for {name}: {reason}")
            }
            Self::InvalidRequest(reason) => {
                write!(formatter, "invalid TunnelManager request: {reason}")
            }
            Self::AggregateRejected(reason) => {
                write!(formatter, "candidate aggregate rejected: {reason}")
            }
            Self::Store(error) => write!(formatter, "control store failed: {error}"),
            Self::Manager(reason) => write!(formatter, "runtime transition failed: {reason}"),
            Self::PublishFailed {
                reason,
                reconciled_back,
            } => write!(
                formatter,
                "durable publication failed ({reason}); reconciled back: {reconciled_back} (Plan 289)"
            ),
            Self::Unavailable => write!(
                formatter,
                "TunnelManager control state unavailable (Plan 289)"
            ),
        }
    }
}

impl ControlError {
    /// JSON-RPC code: validation failures are invalid params; store,
    /// runtime, and publication failures are internal errors.
    pub const fn wire_code(&self) -> i64 {
        match self {
            Self::Store(_) | Self::Manager(_) | Self::PublishFailed { .. } | Self::Unavailable => {
                -32603
            }
            _ => -32602,
        }
    }
}

/// Persistence failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// Filesystem I/O failure (static reason only).
    Io(&'static str),
    /// Generation file fails schema validation.
    Corrupt(&'static str),
    /// Byte ceiling exceeded.
    OverBound,
    /// Path escape or separator attempt.
    PathEscape,
    /// Symlink or special file where a regular file/dir is required.
    NotRegular,
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(reason) => write!(formatter, "storage I/O failed: {reason}"),
            Self::Corrupt(reason) => write!(formatter, "generation corrupt: {reason}"),
            Self::OverBound => write!(formatter, "generation over byte ceiling"),
            Self::PathEscape => write!(formatter, "path escape rejected"),
            Self::NotRegular => write!(formatter, "symlink or special file rejected"),
        }
    }
}

/// Serializable generation file (deterministic key order).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct GenerationFile {
    schema_version: u8,
    generation_id: u64,
    definitions: Vec<StoredDefinition>,
}

/// Serializable definition row.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredDefinition {
    name: String,
    tunnel_type: String,
    options: BTreeMap<String, String>,
    start_on_load: bool,
}

/// Serializable pointer file.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct PointerFile {
    schema_version: u8,
    current: u64,
}

/// A loaded generation: id plus ordered definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedGeneration {
    /// Published generation id (`0` when no generation exists yet).
    pub generation_id: u64,
    /// Definitions in name order.
    pub definitions: Vec<ControlDefinition>,
}

/// Versioned, recoverable control generation store.
///
/// Layout beneath `<data_dir>/i2pcontrol/tunnels/`:
/// `staged-<id>.json` files (never loadable), `generation-<id>.json`
/// files (published only), plus a `current.json` pointer. Staging
/// writes the next staged file without touching published state;
/// publication renames the staged file into place and then moves the
/// pointer atomically. Recovery reads the pointer target, falling back
/// to the newest valid published generation file; retention keeps the
/// current plus prior published generation and removes older files.
#[derive(Clone, Debug)]
pub struct ControlStore {
    root: PathBuf,
}

impl ControlStore {
    /// Opens (creating) the control state root with owner-only
    /// permissions. Rejects symlink roots.
    pub fn open(data_dir: &Path) -> Result<Self, StoreError> {
        let root = data_dir
            .join(CONTROL_STATE_SUBDIR)
            .join(CONTROL_TUNNELS_SUBDIR);
        reject_symlink(data_dir)?;
        create_dir_secure(&root)?;
        reject_symlink(&root)?;
        Ok(Self { root })
    }

    /// State root (tests).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Next generation id: one above the highest id present in
    /// published or staged files (monotonic across restarts; gaps from
    /// retention are harmless).
    fn next_id(&self) -> Result<u64, StoreError> {
        let mut highest = 0_u64;
        for id in self.list_published_ids()? {
            highest = highest.max(id);
        }
        for id in self.list_staged_ids()? {
            highest = highest.max(id);
        }
        highest.checked_add(1).ok_or(StoreError::OverBound)
    }

    /// Lists published generation ids present on disk, ascending.
    fn list_published_ids(&self) -> Result<Vec<u64>, StoreError> {
        self.list_ids("generation-", ".json")
    }

    /// Lists staged (never loadable) generation ids, ascending.
    fn list_staged_ids(&self) -> Result<Vec<u64>, StoreError> {
        self.list_ids("staged-", ".json")
    }

    /// Lists ids for one filename shape, sorted ascending.
    fn list_ids(&self, prefix: &str, suffix: &str) -> Result<Vec<u64>, StoreError> {
        let mut ids = Vec::new();
        let entries = std::fs::read_dir(&self.root).map_err(|_| StoreError::Io("read root"))?;
        for entry in entries {
            let entry = entry.map_err(|_| StoreError::Io("read entry"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = parse_suffixed_name(&name, prefix, suffix) {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Loads the published generation with prior-generation recovery.
    pub fn load(&self) -> Result<LoadedGeneration, StoreError> {
        // Prefer the pointer target when it parses and validates.
        if let Ok(pointer_bytes) = read_bounded(&self.pointer_path())
            && let Ok(pointer) = serde_json::from_slice::<PointerFile>(&pointer_bytes)
            && pointer.schema_version == CONTROL_SCHEMA_VERSION
            && let Ok(definitions) = self.load_generation(pointer.current)
        {
            return Ok(LoadedGeneration {
                generation_id: pointer.current,
                definitions,
            });
        }
        // Fall back to the newest valid published generation file.
        // Staged files are invisible here by construction.
        let mut ids = self.list_published_ids()?;
        ids.reverse();
        for id in ids {
            if let Ok(definitions) = self.load_generation(id) {
                return Ok(LoadedGeneration {
                    generation_id: id,
                    definitions,
                });
            }
        }
        Ok(LoadedGeneration {
            generation_id: 0,
            definitions: Vec::new(),
        })
    }

    /// Loads and validates one generation file.
    fn load_generation(&self, id: u64) -> Result<Vec<ControlDefinition>, StoreError> {
        let path = self.generation_path(id);
        reject_symlink(&path)?;
        let bytes = read_bounded(&path)?;
        let file: GenerationFile =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt("unparsable JSON"))?;
        if file.schema_version != CONTROL_SCHEMA_VERSION {
            return Err(StoreError::Corrupt("schema version"));
        }
        if file.generation_id != id {
            return Err(StoreError::Corrupt("id mismatch"));
        }
        if file.definitions.len() > MAX_SERVICE_TUNNELS {
            return Err(StoreError::Corrupt("definition ceiling"));
        }
        let mut definitions = Vec::with_capacity(file.definitions.len());
        let mut names = BTreeSet::new();
        for stored in file.definitions {
            validate_tunnel_name(&stored.name).map_err(|_| StoreError::Corrupt("name"))?;
            if !names.insert(stored.name.clone()) {
                return Err(StoreError::Corrupt("duplicate name"));
            }
            if stored.options.len() > 64 {
                return Err(StoreError::Corrupt("option ceiling"));
            }
            let tunnel_type =
                TunnelType::parse(&stored.tunnel_type).map_err(|_| StoreError::Corrupt("type"))?;
            definitions.push(ControlDefinition {
                name: stored.name,
                tunnel_type,
                options: stored.options,
                start_on_load: stored.start_on_load,
            });
        }
        Ok(definitions)
    }

    /// Stages a candidate generation as a never-loadable staged file.
    /// Returns the staged generation id.
    pub fn stage(
        &self,
        definitions: &BTreeMap<String, ControlDefinition>,
    ) -> Result<u64, StoreError> {
        if definitions.len() > MAX_SERVICE_TUNNELS {
            return Err(StoreError::OverBound);
        }
        let id = self.next_id()?;
        let stored: Vec<StoredDefinition> = definitions
            .values()
            .map(|definition| StoredDefinition {
                name: definition.name.clone(),
                tunnel_type: definition.tunnel_type.name().to_owned(),
                options: definition.options.clone(),
                start_on_load: definition.start_on_load,
            })
            .collect();
        let file = GenerationFile {
            schema_version: CONTROL_SCHEMA_VERSION,
            generation_id: id,
            definitions: stored,
        };
        let bytes = serde_json::to_vec(&file).map_err(|_| StoreError::Corrupt("serialization"))?;
        if bytes.len() > MAX_GENERATION_BYTES {
            return Err(StoreError::OverBound);
        }
        write_atomic(&self.staged_path(id), &bytes)?;
        Ok(id)
    }

    /// Publishes a staged generation id: moves the pointer first (so a
    /// crash before the rename falls back to the prior generation),
    /// then renames the staged file into place, then enforces bounded
    /// retention (current plus prior kept).
    pub fn publish(&self, id: u64) -> Result<(), StoreError> {
        let staged = self.staged_path(id);
        reject_symlink(&staged)?;
        read_bounded(&staged)?;
        let pointer = PointerFile {
            schema_version: CONTROL_SCHEMA_VERSION,
            current: id,
        };
        let bytes =
            serde_json::to_vec(&pointer).map_err(|_| StoreError::Corrupt("serialization"))?;
        write_atomic(&self.pointer_path(), &bytes)?;
        // The rename follows the pointer move: a crash between the two
        // loads the prior generation via scan fallback, and the next
        // publish converges (pointer already names this id).
        if std::fs::rename(&staged, self.generation_path(id)).is_err() {
            return Err(StoreError::Io("rename staged"));
        }
        self.enforce_retention(id)?;
        Ok(())
    }

    /// Removes published generation files older than the prior generation.
    fn enforce_retention(&self, current: u64) -> Result<(), StoreError> {
        for id in self.list_published_ids()? {
            if id < current.saturating_sub(1) {
                let _ = std::fs::remove_file(self.generation_path(id));
            }
        }
        Ok(())
    }

    /// Removes every staged (never-loadable) file plus, defensively,
    /// any published file newer than the pointer. Called at startup
    /// before loading; the load path itself never deletes.
    pub fn cleanup_orphans(&self) {
        let Ok(staged) = self.list_staged_ids() else {
            return;
        };
        for id in staged {
            let _ = std::fs::remove_file(self.staged_path(id));
        }
        let current = self.pointer_current().unwrap_or(0);
        let Ok(published) = self.list_published_ids() else {
            return;
        };
        for id in published {
            if id > current {
                let _ = std::fs::remove_file(self.generation_path(id));
            }
        }
    }

    /// Reads the pointer's current id without validation fallback.
    fn pointer_current(&self) -> Result<u64, StoreError> {
        let bytes = read_bounded(&self.pointer_path())?;
        let pointer: PointerFile =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt("pointer"))?;
        if pointer.schema_version != CONTROL_SCHEMA_VERSION {
            return Err(StoreError::Corrupt("pointer schema"));
        }
        Ok(pointer.current)
    }

    /// Current published generation id (`0` when nothing published yet).
    pub fn current_id(&self) -> u64 {
        self.load().map(|loaded| loaded.generation_id).unwrap_or(0)
    }

    /// Generation file path (no existence check).
    fn generation_path(&self, id: u64) -> PathBuf {
        self.root.join(format!("generation-{id}.json"))
    }

    /// Staged file path (never loadable; no existence check).
    fn staged_path(&self, id: u64) -> PathBuf {
        self.root.join(format!("staged-{id}.json"))
    }

    /// Pointer file path.
    fn pointer_path(&self) -> PathBuf {
        self.root.join(CONTROL_POINTER_FILE)
    }
}

/// Parses `generation-<id>.json` / `staged-<id>.json` file names.
fn parse_suffixed_name(name: &str, prefix: &str, suffix: &str) -> Option<u64> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse::<u64>()
        .ok()
}

/// Reads a file with the generation byte ceiling enforced.
fn read_bounded(path: &Path) -> Result<Vec<u8>, StoreError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| StoreError::Io("stat"))?;
    if metadata.file_type().is_symlink() {
        return Err(StoreError::NotRegular);
    }
    if !metadata.is_file() {
        return Err(StoreError::NotRegular);
    }
    if metadata.len() > MAX_GENERATION_BYTES as u64 {
        return Err(StoreError::OverBound);
    }
    std::fs::read(path).map_err(|_| StoreError::Io("read"))
}

/// Rejects symlink roots/files (no TOCTOU claim across the check/use
/// boundary; generation files are content-validated on load regardless).
fn reject_symlink(path: &Path) -> Result<(), StoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::NotRegular),
        Ok(_) => Ok(()),
        // Absent paths are created fresh below; nothing to reject.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(StoreError::Io("stat")),
    }
}

/// Creates a directory (and parents) with owner-only permissions on
/// unix. Non-unix platforms keep default permissions.
fn create_dir_secure(path: &Path) -> Result<(), StoreError> {
    std::fs::create_dir_all(path).map_err(|_| StoreError::Io("create dir"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| StoreError::Io("permissions"))?;
    }
    Ok(())
}

/// Writes bytes atomically: temp file + sync + rename + directory sync.
/// Temp files carry a hidden prefix so generation scans ignore them.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or(StoreError::PathEscape)?;
    let temp = parent.join(format!(".tmp-{}-{}", std::process::id(), temp_counter(),));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp).map_err(|_| StoreError::Io("create temp"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Io("permissions"))?;
        }
        file.write_all(bytes)
            .map_err(|_| StoreError::Io("write temp"))?;
        file.sync_all().map_err(|_| StoreError::Io("sync temp"))?;
    }
    std::fs::rename(&temp, path).map_err(|_| {
        let _ = std::fs::remove_file(&temp);
        StoreError::Io("rename")
    })?;
    sync_dir(parent)
}

/// Best-effort directory sync (durability of the rename itself).
fn sync_dir(dir: &Path) -> Result<(), StoreError> {
    let file = std::fs::File::open(dir).map_err(|_| StoreError::Io("open dir"))?;
    file.sync_all().map_err(|_| StoreError::Io("sync dir"))?;
    Ok(())
}

/// Process-wide temp-file counter (disambiguates concurrent writers).
fn temp_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Maps a Proposal type onto the existing M10 runtime family.
/// Plan 290 added the four composed families over the existing
/// manager and shared primitives; Plan 291 adds the two Streamr
/// families over the repliable-datagram substrate. All twelve
/// Proposal types map; unknown spellings never reach this point
/// (the contract parser rejects them).
pub fn map_tunnel_type(tunnel_type: TunnelType) -> Result<ServiceTunnelKind, ControlError> {
    match tunnel_type {
        TunnelType::Client => Ok(ServiceTunnelKind::GenericClient),
        TunnelType::Server => Ok(ServiceTunnelKind::GenericServer),
        TunnelType::HttpClient => Ok(ServiceTunnelKind::HttpClient),
        TunnelType::Socks => Ok(ServiceTunnelKind::Socks5Client),
        TunnelType::IrcClient => Ok(ServiceTunnelKind::IrcClient),
        TunnelType::IrcServer => Ok(ServiceTunnelKind::IrcServer),
        // Plan 290: composed families over the existing M10
        // manager and shared primitives.
        TunnelType::ConnectClient => Ok(ServiceTunnelKind::ConnectClient),
        TunnelType::SocksIrc => Ok(ServiceTunnelKind::SocksIrc),
        TunnelType::HttpServer => Ok(ServiceTunnelKind::HttpServer),
        TunnelType::HttpBidirServer => Ok(ServiceTunnelKind::HttpBidirServer),
        // Plan 291: Streamr families over the repliable-datagram
        // substrate. The match is exhaustive over the frozen
        // twelve-type inventory, so a thirteenth type fails at
        // compile time (fail-closed); runtime rejection of
        // unmapped types stays in `normalize_definition` and the
        // supervisor gates through `has_plan291_backend`.
        TunnelType::StreamrClient => Ok(ServiceTunnelKind::StreamrClient),
        TunnelType::StreamrServer => Ok(ServiceTunnelKind::StreamrServer),
    }
}

/// Parses one `true`/`false` option value.
fn parse_bool_option(option: &str, value: &str) -> Result<bool, ControlError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "must be true or false",
        }),
    }
}

/// Parses a Proposal tunnel length (Plan 292 shaping: 1..=3).
/// Length 0 is rejected: service destinations run in Remote tunnel
/// mode and the destination policy does not permit zero-hop pools.
fn parse_shaping_length(option: &str, value: &str) -> Result<u8, ControlError> {
    let length = value
        .parse::<u8>()
        .map_err(|_| ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel length must be an integer within 1..=3",
        })?;
    if length == 0 || length > MAX_TUNNEL_LENGTH_HOPS {
        return Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel length must be within 1..=3 (zero-hop is not permitted for service destinations)",
        });
    }
    Ok(length)
}

/// Parses a Proposal tunnel quantity (Plan 292 shaping: 1..=6).
fn parse_shaping_quantity(option: &str, value: &str) -> Result<u8, ControlError> {
    let quantity = value
        .parse::<u8>()
        .map_err(|_| ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel quantity must be an integer within 1..=6",
        })?;
    if quantity == 0 || quantity > MAX_TUNNEL_QUANTITY {
        return Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel quantity must be within 1..=6",
        });
    }
    Ok(quantity)
}

/// Builds a validated [`ServiceTunnelSpec`] from a control definition.
///
/// Only options with a real owner are accepted (Plans 289, 291, and
/// the Plan 292 shaping slice so far); every other supplied option
/// fails as unsupported (never accepted inertly). Secret-classified
/// options are rejected even though the subset contains none, so a
/// future subset extension cannot silently persist secrets.
pub fn build_control_spec(
    definition: &ControlDefinition,
) -> Result<ServiceTunnelSpec, ControlError> {
    let kind = map_tunnel_type(definition.tunnel_type)?;
    let id = ServiceTunnelId::parse(&definition.name)
        .map_err(|_| ControlError::InvalidRequest("stored name fails service id validation"))?;
    let mut destination: Option<DestinationRef> = None;
    let mut target_host: Option<std::net::IpAddr> = None;
    let mut target_port: Option<u16> = None;
    let mut listen_host: Option<std::net::IpAddr> = None;
    let mut listen_port: u16 = 0;
    let mut max_connections = DEFAULT_CONTROL_MAX_CONNECTIONS;
    // Plan 292 shaping inputs (symmetric defaults plus per-direction
    // overrides; ranges enforced per key at parse time).
    let mut symmetric_length: Option<u8> = None;
    let mut symmetric_quantity: Option<u8> = None;
    let mut inbound_length: Option<u8> = None;
    let mut outbound_length: Option<u8> = None;
    let mut inbound_quantity: Option<u8> = None;
    let mut outbound_quantity: Option<u8> = None;
    // Plan 292 profile selection (OR-combined effective bit).
    let mut profile_interactive = false;
    // Plan 291 Streamr inputs (validated per kind below; ranges
    // enforced by `StreamrOptions::validate` through the final
    // spec validation).
    let mut local_udp_host: Option<std::net::IpAddr> = None;
    let mut local_udp_port: Option<u16> = None;
    let mut target_i2p_port: u16 = 0;
    let mut subscribe_interval_ms: Option<u64> = None;
    let mut subscription_expiry_ms: Option<u64> = None;
    let mut max_subscribers: Option<usize> = None;
    let mut payload_limit_bytes: Option<usize> = None;
    for (key, value) in &definition.options {
        match key.as_str() {
            "target_destination" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericClient
                        | ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::IrcClient
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                        | ServiceTunnelKind::StreamrClient
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_destination applies to client kinds only",
                    });
                }
                destination = Some(DestinationRef::parse(value).map_err(|_| {
                    ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "destination reference malformed",
                    }
                })?);
            }
            "target_host" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_host applies to server kinds only",
                    });
                }
                if value.starts_with("unix:") {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "unix targets are not yet supported",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "target_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "target_host must be loopback",
                    });
                }
                target_host = Some(address);
            }
            "target_port" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_port applies to server kinds only",
                    });
                }
                target_port =
                    Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "target_port must be 0..=65535",
                            })?,
                    );
            }
            "listen_host" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::StreamrClient
                        | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "listen_host applies to client kinds only",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "listen_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "listen_host must be loopback",
                    });
                }
                listen_host = Some(address);
            }
            "listen_port" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::StreamrClient
                        | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "listen_port applies to client kinds only",
                    });
                }
                listen_port = value
                    .parse::<u16>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "listen_port must be 0..=65535",
                    })?;
            }
            "start_on_load" => {
                // Validated here so malformed values fail before any
                // side effect; the coordinator owns the persisted
                // intent on the definition, not the spec.
                let _ = parse_bool_option(key, value)?;
            }
            "max_streams" => {
                max_connections =
                    value
                        .parse::<usize>()
                        .map_err(|_| ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "max_streams must be a positive integer",
                        })?;
                if max_connections == 0 || max_connections > 128 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "max_streams must be within 1..=128",
                    });
                }
            }
            // Plan 292: pool shaping. `tunnel_length` and
            // `tunnel_quantity` are symmetric defaults; the
            // per-direction keys override. Proposal bounds bind
            // (length 1..=3, quantity 1..=6); length 0 is rejected
            // because service destinations run in Remote tunnel mode.
            "tunnel_length" => {
                symmetric_length = Some(parse_shaping_length(key, value)?);
            }
            "inbound_length" => {
                inbound_length = Some(parse_shaping_length(key, value)?);
            }
            "outbound_length" => {
                outbound_length = Some(parse_shaping_length(key, value)?);
            }
            "tunnel_quantity" => {
                symmetric_quantity = Some(parse_shaping_quantity(key, value)?);
            }
            "inbound_quantity" => {
                inbound_quantity = Some(parse_shaping_quantity(key, value)?);
            }
            "outbound_quantity" => {
                outbound_quantity = Some(parse_shaping_quantity(key, value)?);
            }
            // Plan 292: streaming profile. Only "interactive" is
            // special (PR6 reference: it constrains the streaming
            // window); any other value must be the explicit "bulk"
            // default. The boolean is OR-combined: either knob
            // selects the interactive windows.
            "profile" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "profile applies to streaming kinds only",
                    });
                }
                match value.as_str() {
                    "interactive" => profile_interactive = true,
                    "bulk" => {}
                    _ => {
                        return Err(ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "profile must be \"bulk\" or \"interactive\"",
                        });
                    }
                }
            }
            "interactive" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "interactive applies to streaming kinds only",
                    });
                }
                if parse_bool_option(key, value)? {
                    profile_interactive = true;
                }
            }
            // Plan 291: Streamr UDP endpoints and cadence policy.
            // The loopback shape is enforced here; numeric ranges
            // are enforced by `StreamrOptions::validate` through
            // the final spec validation.
            "local_udp_host" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "local_udp_host applies to Streamr kinds only",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "local_udp_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "local_udp_host must be loopback",
                    });
                }
                local_udp_host = Some(address);
            }
            "local_udp_port" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "local_udp_port applies to Streamr kinds only",
                    });
                }
                local_udp_port =
                    Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "local_udp_port must be 0..=65535",
                            })?,
                    );
            }
            "target_i2p_port" => {
                if !matches!(kind, ServiceTunnelKind::StreamrClient) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_i2p_port applies to streamr-client only",
                    });
                }
                target_i2p_port =
                    value
                        .parse::<u16>()
                        .map_err(|_| ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "target_i2p_port must be 0..=65535",
                        })?;
            }
            "streamr_subscribe_interval" => {
                if !matches!(kind, ServiceTunnelKind::StreamrClient) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_subscribe_interval applies to streamr-client only",
                    });
                }
                subscribe_interval_ms =
                    Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_subscribe_interval must be an integer",
                            })?,
                    );
            }
            "streamr_expiry" => {
                if !matches!(kind, ServiceTunnelKind::StreamrServer) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_expiry applies to streamr-server only",
                    });
                }
                subscription_expiry_ms =
                    Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_expiry must be an integer",
                            })?,
                    );
            }
            "streamr_max_subscribers" => {
                if !matches!(kind, ServiceTunnelKind::StreamrServer) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_max_subscribers applies to streamr-server only",
                    });
                }
                max_subscribers =
                    Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_max_subscribers must be an integer",
                            })?,
                    );
            }
            "streamr_payload_limit" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_payload_limit applies to Streamr kinds only",
                    });
                }
                payload_limit_bytes =
                    Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_payload_limit must be an integer",
                            })?,
                    );
            }
            other => {
                // Secret-classified keys are rejected here even though
                // they never reach storage: belt and suspenders against
                // secret persistence. Blocked and corrective-pending
                // keys name their owning plan.
                return Err(ControlError::UnsupportedOption(rejected_option_reason(
                    definition.tunnel_type,
                    other,
                )));
            }
        }
    }
    // Per-kind required-field completion with real defaults.
    // Server targets require both halves explicitly: no silent default
    // for where tunneled traffic exits.
    // Plan 292: resolve shaping. Per-direction lengths must agree:
    // the pool uses a single hop length and the control plane must
    // not silently drop a requested value. Per-key ranges were
    // enforced at parse time, so the literal below is in-bounds
    // (re-checked by the final spec validation).
    let inbound_length = inbound_length.or(symmetric_length);
    let outbound_length = outbound_length.or(symmetric_length);
    if let (Some(inbound), Some(outbound)) = (inbound_length, outbound_length) {
        if inbound != outbound {
            return Err(ControlError::ContradictoryOptions {
                name: definition.name.clone(),
                reason: "inbound_length and outbound_length differ; the pool uses a single hop length",
            });
        }
    }
    let shaping = TunnelShaping {
        inbound_quantity: inbound_quantity.or(symmetric_quantity).unwrap_or(2),
        outbound_quantity: outbound_quantity.or(symmetric_quantity).unwrap_or(2),
        length_hops: inbound_length.or(outbound_length).unwrap_or(2),
    };
    let target = match kind {
        ServiceTunnelKind::GenericServer
        | ServiceTunnelKind::IrcServer
        | ServiceTunnelKind::HttpServer
        | ServiceTunnelKind::HttpBidirServer => Some(match (target_host, target_port) {
            (Some(host), Some(port)) => {
                ServerTarget::LoopbackTcp(std::net::SocketAddr::new(host, port))
            }
            _ => {
                return Err(ControlError::InvalidRequest(
                    "server kinds require target_host and target_port",
                ));
            }
        }),
        _ => None,
    };
    let listener = match kind {
        ServiceTunnelKind::GenericClient
        | ServiceTunnelKind::HttpClient
        | ServiceTunnelKind::Socks5Client
        | ServiceTunnelKind::IrcClient
        | ServiceTunnelKind::ConnectClient
        | ServiceTunnelKind::SocksIrc
        | ServiceTunnelKind::HttpBidirServer => Some(
            LocalListenerSpec::parse(
                listen_host.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
                listen_port,
            )
            .map_err(|_| ControlError::InvalidOption {
                option: "listen_host".to_owned(),
                reason: "listener must be loopback",
            })?,
        ),
        ServiceTunnelKind::GenericServer
        | ServiceTunnelKind::IrcServer
        | ServiceTunnelKind::HttpServer
        | ServiceTunnelKind::StreamrClient
        | ServiceTunnelKind::StreamrServer => None,
    };
    if listener.is_some() && destination.is_none() {
        // The bidirectional profile is the exception: its client
        // half resolves per-request destinations like http-client
        // and only the server half publishes, so no remote
        // destination reference is carried.
        if !matches!(kind, ServiceTunnelKind::HttpBidirServer) {
            return Err(ControlError::InvalidRequest(
                "client kinds require the target_destination option",
            ));
        }
    }
    let (http_options, socks5_options, irc_options, connect_options, streamr_options) = match kind {
        ServiceTunnelKind::HttpClient | ServiceTunnelKind::HttpBidirServer => (
            Some(i2pr_service_tunnels::HttpClientOptions::defaults()),
            None,
            None,
            None,
            None,
        ),
        ServiceTunnelKind::Socks5Client => (
            None,
            Some(i2pr_service_tunnels::Socks5ClientOptions::defaults()),
            None,
            None,
            None,
        ),
        ServiceTunnelKind::SocksIrc => (
            None,
            Some(i2pr_service_tunnels::Socks5ClientOptions::defaults()),
            Some(i2pr_service_tunnels::IrcClientOptions::defaults()),
            None,
            None,
        ),
        ServiceTunnelKind::IrcClient => (
            None,
            None,
            Some(i2pr_service_tunnels::IrcClientOptions::defaults()),
            None,
            None,
        ),
        ServiceTunnelKind::ConnectClient => (
            None,
            None,
            None,
            Some(i2pr_service_tunnels::ConnectClientOptions::defaults()),
            None,
        ),
        // Plan 291: Streamr endpoints are explicit (no silent
        // default for where media enters or exits); cadence
        // policy defaults to the freeze and honors supplied
        // overrides through spec validation.
        ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer => {
            let local_udp = match (local_udp_host, local_udp_port) {
                (Some(host), Some(port)) => Some(std::net::SocketAddr::new(host, port)),
                _ => {
                    return Err(ControlError::InvalidRequest(
                        "streamr kinds require local_udp_host and local_udp_port",
                    ));
                }
            };
            let options = i2pr_service_tunnels::StreamrOptions {
                local_udp,
                target_i2p_port,
                subscribe_interval_ms: subscribe_interval_ms
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_SUBSCRIBE_INTERVAL_MS),
                subscription_expiry_ms: subscription_expiry_ms
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_SUBSCRIPTION_EXPIRY_MS),
                max_subscribers: max_subscribers
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_MAX_SUBSCRIBERS),
                payload_limit_bytes: payload_limit_bytes
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_PAYLOAD_LIMIT_BYTES),
            };
            (None, None, None, None, Some(options))
        }
        _ => (None, None, None, None, None),
    };
    let spec = ServiceTunnelSpec {
        id,
        kind,
        enabled: false,
        listener,
        target,
        targets: Vec::new(),
        destination,
        policy: DestinationPolicy::Dedicated,
        max_connections,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        shaping,
        streaming_interactive: profile_interactive,
        http_options,
        socks5_options,
        irc_options,
        connect_options,
        streamr_options,
    };
    spec.validate()
        .map_err(|error| ControlError::InvalidRequest(static_spec_reason(error)))?;
    Ok(spec)
}

/// Converts a spec validation failure into a static reason (never
/// echoes spec values).
fn static_spec_reason(error: i2pr_service_tunnels::ServiceTunnelError) -> &'static str {
    use i2pr_service_tunnels::ServiceTunnelError as E;
    match error {
        E::ExceedsCeiling { .. } => "spec exceeds a service ceiling",
        E::ContradictoryOptions { .. } => "spec options contradict the kind",
        E::DuplicateId { .. } => "duplicate service id",
        E::DuplicateListener { .. } => "duplicate listener",
        E::InvalidKind { .. } => "invalid service kind",
        E::InvalidListener { .. } => "invalid listener",
        E::InvalidTarget { .. } => "invalid server target",
        E::InvalidDestinationRef { .. } => "invalid destination reference",
        E::InvalidId { .. } => "invalid service id",
        _ => "spec validation failed",
    }
}

/// Validates one control definition's options against the 289 subset and
/// builds the normalized definition. Unknown-universe keys cannot arrive
/// here (the envelope rejects them); known-but-unsupported keys fail.
/// Names why an ownerless option key is rejected, pointing at the
/// owning plan for blocked and corrective-pending matrix cells. Only
/// inventory keys reach here (the request envelope rejects unknown
/// keys), so echoing the key name leaks no value.
fn rejected_option_reason(tunnel_type: TunnelType, key: &str) -> String {
    match disposition_for(tunnel_type.canonical_index(), key) {
        Some(CellDisposition::BlockedPrimitive { plan, primitive }) => {
            format!("{key} is not yet supported (Plan {plan} owns {primitive})")
        }
        Some(CellDisposition::CorrectivePending { plan, reason }) => {
            format!("{key} is not yet supported (Plan {plan} owns {reason})")
        }
        _ => key.to_owned(),
    }
}

pub fn normalize_definition(
    name: &str,
    tunnel_type: TunnelType,
    options: &BTreeMap<String, String>,
    start_on_load: bool,
) -> Result<ControlDefinition, ControlError> {
    validate_tunnel_name(name).map_err(|_| ControlError::InvalidRequest("invalid tunnel name"))?;
    if !tunnel_type.has_plan291_backend() {
        return Err(ControlError::UnsupportedType(tunnel_type.name().to_owned()));
    }
    for key in options.keys() {
        if !SUPPORTED_289_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_291_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_292_OPTIONS.contains(&key.as_str())
        {
            return Err(ControlError::UnsupportedOption(rejected_option_reason(
                tunnel_type,
                key,
            )));
        }
    }
    let mut explicit_start_on_load = start_on_load;
    if let Some(value) = options.get("start_on_load") {
        explicit_start_on_load = parse_bool_option("start_on_load", value)?;
    }
    let definition = ControlDefinition {
        name: name.to_owned(),
        tunnel_type,
        options: options.clone(),
        start_on_load: explicit_start_on_load,
    };
    // Validate the full mapping now, before any side effect. The
    // returned spec is discarded; the coordinator rebuilds it.
    let _ = build_control_spec(&definition)?;
    Ok(definition)
}

/// One supervised control runtime: the single M10 manager holding only
/// control-owned specs, plus the coordinator state around it.
///
/// All mutations serialize on `op_lock` (a Tokio mutex: it is held
/// across reconcile awaits, and every control future stays `Send`).
/// Reads (`get`, status) never take the op lock: they observe
/// point-in-time snapshots with explicit transitioning flags.
///
/// Transaction order per mutation: validate the complete candidate
/// before any side effect → update the in-memory mirror → stage the
/// persistence generation → reconcile the manager → start supervisors
/// for added/replaced runtimes → publish the durable generation →
/// verify agreement. A supervisor failure reconciles back without
/// publishing; a publish failure reconciles back under a hard deadline.
/// Staged-but-unpublished files are orphans; retention and startup
/// cleanup remove them.
pub struct TunnelControlState {
    /// Durable generation store.
    store: ControlStore,
    /// Startup-owned inventory (inspectable, never mutated).
    startup: ServiceTunnelSet,
    /// The one M10 manager (control-owned specs only).
    manager: Arc<ServiceTunnelManager>,
    /// Serializes all mutations (same-name and cross-name).
    op_lock: tokio::sync::Mutex<()>,
    /// Current durable intent mirror (rebuilt from the store at startup).
    definitions: Mutex<BTreeMap<String, ControlDefinition>>,
    /// Names with current running intent.
    running: Mutex<BTreeSet<String>>,
    /// Names with live supervisors per manager generation.
    supervised: Mutex<HashMap<String, u64>>,
    /// Names with a recorded transition failure (bounded static reasons).
    failed: Mutex<HashMap<String, &'static str>>,
    /// Names with a transition in flight (observed as starting/stopping).
    transitioning: Mutex<BTreeSet<String>>,
    /// Supervisor scope installed at startup (None before startup).
    children: Mutex<Option<i2pr_runtime::ChildScope>>,
    /// Cancellation installed at startup (None before startup).
    cancellation: Mutex<Option<i2pr_runtime::CancellationToken>>,
}

impl TunnelControlState {
    /// Builds control state over an explicit store, startup inventory,
    /// and manager. Definitions load lazily at [`Self::startup`].
    pub fn new(
        store: ControlStore,
        startup: ServiceTunnelSet,
        manager: Arc<ServiceTunnelManager>,
    ) -> Self {
        Self {
            store,
            startup,
            manager,
            op_lock: tokio::sync::Mutex::new(()),
            definitions: Mutex::new(BTreeMap::new()),
            running: Mutex::new(BTreeSet::new()),
            supervised: Mutex::new(HashMap::new()),
            failed: Mutex::new(HashMap::new()),
            transitioning: Mutex::new(BTreeSet::new()),
            children: Mutex::new(None),
            cancellation: Mutex::new(None),
        }
    }

    /// Builds control state from the validated daemon configuration:
    /// store beneath the router data dir, startup inventory from the
    /// configured service set, and a fresh manager holding no specs yet
    /// (control generations arrive through the store at startup).
    pub fn for_config(config: &crate::config::Config) -> Result<Self, ControlError> {
        let store = ControlStore::open(&config.router.data_dir).map_err(ControlError::Store)?;
        let manager_config = ServiceTunnelManagerConfig {
            data_dir: config.router.data_dir.clone(),
            aggregate_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_aggregate,
            per_service_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_per_service,
            specs: Arc::new(ServiceTunnelSet::new()),
            aliases: Arc::new(config.service_tunnels.aliases.clone()),
        };
        let manager = Arc::new(
            ServiceTunnelManager::new(manager_config)
                .map_err(|error| ControlError::Manager(static_manager_reason(&error)))?,
        );
        Ok(Self::new(
            store,
            config.service_tunnels.tunnels.clone(),
            manager,
        ))
    }

    /// Current published store generation (`0` when nothing published).
    pub fn store_generation(&self) -> u64 {
        self.store.current_id()
    }

    /// Daemon startup: removes orphan staged generations, loads and
    /// validates the newest durable control generation (recovering the
    /// prior generation on corruption), detects collisions against
    /// startup-owned names before starting anything, reconciles
    /// StartOnLoad definitions with supported backends, and starts
    /// their supervisors under the supplied scope.
    ///
    /// Failures isolate per definition: every valid StartOnLoad
    /// definition starts even when a sibling fails. Returns the
    /// per-definition failure list (empty on full success). Never
    /// rotates a persistent server identity: server destinations reuse
    /// the manager's persisted service-destination records.
    pub async fn startup(
        &self,
        children: &i2pr_runtime::ChildScope,
        cancellation: &i2pr_runtime::CancellationToken,
    ) -> Vec<(String, &'static str)> {
        {
            *lock(&self.children) = Some(children.clone());
            *lock(&self.cancellation) = Some(cancellation.clone());
        }
        self.store.cleanup_orphans();
        let loaded = match self.store.load() {
            Ok(loaded) => loaded,
            Err(_) => LoadedGeneration {
                generation_id: 0,
                definitions: Vec::new(),
            },
        };
        let mut failures = Vec::new();
        let mut definitions = BTreeMap::new();
        for definition in loaded.definitions {
            if self.startup_name(&definition.name).is_some() {
                failures.push((definition.name.clone(), "startup name collision"));
                continue;
            }
            if build_control_spec(&definition).is_err() {
                failures.push((definition.name.clone(), "stored definition invalid"));
                continue;
            }
            definitions.insert(definition.name.clone(), definition);
        }
        *lock(&self.definitions) = definitions;
        let start: Vec<String> = lock(&self.definitions)
            .values()
            .filter(|definition| {
                definition.start_on_load && definition.tunnel_type.has_plan291_backend()
            })
            .map(|definition| definition.name.clone())
            .collect();
        if start.is_empty() {
            return failures;
        }
        // Bulk fast path: one transaction for every StartOnLoad
        // definition, then per-definition isolation on failure.
        lock(&self.running).extend(start.iter().cloned());
        for name in &start {
            lock(&self.transitioning).insert(name.clone());
        }
        let guard = self.op_lock.lock().await;
        match self.commit_locked(&guard, &start, true).await {
            Ok(_) => {
                for name in &start {
                    self.record_success(name);
                }
            }
            Err(_) => {
                // Isolated fallback: reset running intent and start
                // each definition alone so one bad sibling cannot block
                // the rest.
                lock(&self.running).clear();
                for name in &start {
                    lock(&self.transitioning).remove(name);
                }
                drop(guard);
                for name in start {
                    if let Err(error) = self.start_inner(&name).await {
                        failures.push((name, static_control_reason(&error)));
                    }
                }
            }
        }
        failures
    }

    /// Stops every control runtime (manager-wide shutdown).
    pub async fn shutdown(&self) {
        self.manager.shutdown().await;
    }

    /// Finds a startup-owned spec kind by name.
    fn startup_name(&self, name: &str) -> Option<ServiceTunnelKind> {
        self.startup
            .tunnels
            .iter()
            .find(|spec| spec.id.as_str() == name)
            .map(|spec| spec.kind)
    }

    /// Builds the manager candidate set from the running subset of
    /// the durable mirror. Stopped definitions are excluded entirely:
    /// the manager only ever sees running specs, so staging never
    /// binds a socket for a stopped tunnel and stop transitions drain
    /// without restaging. (`enabled` is always true in the candidate;
    /// persisted intent lives on the definition.)
    fn candidate_set(&self) -> Result<ServiceTunnelSet, ControlError> {
        let mut tunnels = Vec::new();
        {
            let definitions = lock(&self.definitions);
            let running = lock(&self.running);
            tunnels.reserve(definitions.len());
            for definition in definitions.values() {
                if !running.contains(&definition.name) {
                    continue;
                }
                let mut spec = build_control_spec(definition)?;
                spec.enabled = true;
                tunnels.push(spec);
            }
        }
        let candidate = ServiceTunnelSet { tunnels };
        candidate
            .validate()
            .map_err(|error| ControlError::AggregateRejected(static_manager_reason2(&error)))?;
        // Cross-class bind collisions fail closed even though startup
        // services never run under this manager: a listener address is
        // a machine-wide exclusive resource.
        self.reject_cross_class_collisions(&candidate)?;
        Ok(candidate)
    }

    /// Rejects listener collisions between the candidate control set
    /// and startup-owned listeners.
    fn reject_cross_class_collisions(
        &self,
        candidate: &ServiceTunnelSet,
    ) -> Result<(), ControlError> {
        use std::collections::HashSet;
        let mut startup_listeners = HashSet::new();
        for spec in &self.startup.tunnels {
            if let Some(listener) = spec.listener {
                startup_listeners.insert(listener.socket());
            }
        }
        for spec in &candidate.tunnels {
            if let Some(listener) = spec.listener
                && startup_listeners.contains(&listener.socket())
            {
                return Err(ControlError::NameCollision(spec.id.as_str().to_owned()));
            }
        }
        Ok(())
    }

    /// Rejects a single spec's listener against startup-owned
    /// listeners. Applied at create/edit time even for stopped
    /// definitions (the candidate set only carries running specs, so
    /// this check is the fail-closed guard for stopped rows shadowing
    /// startup-owned sockets).
    fn check_listener_against_startup(&self, spec: &ServiceTunnelSpec) -> Result<(), ControlError> {
        let Some(listener) = spec.listener else {
            return Ok(());
        };
        for startup in &self.startup.tunnels {
            if startup.listener.map(|owned| owned.socket()) == Some(listener.socket()) {
                return Err(ControlError::NameCollision(spec.id.as_str().to_owned()));
            }
        }
        Ok(())
    }

    /// Whether an option edit on a running definition restages its
    /// runtime (listener/destination/target/kind change). Restaging
    /// rebinds sockets, so it travels as stop-plus-start; in-place
    /// changes (resource ceilings, intent flags) commit directly.
    fn edit_restages(prior: &ControlDefinition, candidate: &ControlDefinition) -> bool {
        use i2pr_service_tunnels::{DiffClass, diff_spec};
        let mut old_spec = match build_control_spec(prior) {
            Ok(spec) => spec,
            Err(_) => return true,
        };
        let mut new_spec = match build_control_spec(candidate) {
            Ok(spec) => spec,
            Err(_) => return true,
        };
        old_spec.enabled = true;
        new_spec.enabled = true;
        matches!(
            diff_spec(&old_spec, &new_spec),
            DiffClass::ReplaceListener | DiffClass::ReplaceDestination
        )
    }

    /// Core transaction under the op lock for the `names` subset:
    /// stages persistence, reconciles the manager, starts supervisors
    /// for added/replaced runtimes, publishes the generation, prunes
    /// supervision records, and verifies agreement.
    ///
    /// When `probe_ports` is set, listener sockets for `names` are
    /// briefly awaited before staging (see [`Self::await_port_release`]).
    /// A supervisor failure reconciles back without publishing; a
    /// publish failure reconciles back under a hard deadline. Either
    /// way the in-memory mirror is the caller's to roll back.
    /// Returns the published store generation id plus the manager diff.
    async fn commit_locked(
        &self,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
        names: &[String],
        probe_ports: bool,
    ) -> Result<(u64, Vec<i2pr_service_tunnels::ServiceDiff>), ControlError> {
        // Release previously drained runtimes first so their listeners
        // are free before staging binds (immediate-release policy).
        self.manager.reap_expired_drains();
        if probe_ports {
            self.await_port_release(names).await;
        }
        let mirror = lock(&self.definitions).clone();
        let staged = self.store.stage(&mirror).map_err(ControlError::Store)?;
        let candidate = self.candidate_set()?;
        let outcome = self
            .manager
            .reconcile(Arc::new(candidate), CONTROL_DRAIN_DEADLINE)
            .await
            .map_err(|error| ControlError::Manager(static_manager_reason(&error)))?;
        if let Err(error) = self.sync_names(names, &outcome.diff).await {
            let _ = self.reconcile_back().await;
            return Err(error);
        }
        if let Err(store_error) = self.store.publish(staged) {
            let back = self.reconcile_back().await;
            return Err(ControlError::PublishFailed {
                reason: match store_error {
                    StoreError::Io(reason) => reason,
                    StoreError::Corrupt(reason) => reason,
                    StoreError::OverBound => "generation over ceiling",
                    StoreError::PathEscape => "path escape",
                    StoreError::NotRegular => "symlink or special file",
                },
                reconciled_back: back,
            });
        }
        // Prune supervision records for names that no longer run and
        // clear transitioning flags for this transaction's names.
        {
            let running_now = lock(&self.running).clone();
            lock(&self.supervised).retain(|name, _| running_now.contains(name));
        }
        for name in names {
            lock(&self.transitioning).remove(name);
        }
        self.verify_agreement()?;
        Ok((staged, outcome.diff))
    }

    /// Best-effort wait for listener sockets to free before staging.
    ///
    /// Drained runtimes release their sockets only after their
    /// cancelled supervisor tasks exit, which races staging binds on
    /// stop-then-start and restart sequences. This probes each intended
    /// socket briefly (bounded `40 × 50 ms`); a freed socket proceeds
    /// immediately, while a still-held socket proceeds to staging
    /// anyway so genuine conflicts surface as bind errors there. Never
    /// fails by itself.
    async fn await_port_release(&self, names: &[String]) {
        const ATTEMPTS: usize = 40;
        const STEP: Duration = Duration::from_millis(50);
        for name in names {
            let Some(socket) = self.intended_listener(name) else {
                continue;
            };
            for _ in 0..ATTEMPTS {
                match tokio::net::TcpListener::bind(socket).await {
                    Ok(probe) => {
                        drop(probe);
                        break;
                    }
                    Err(_) => tokio::time::sleep(STEP).await,
                }
            }
        }
    }

    /// Intended client-listener socket for a mirror definition (`None`
    /// when the definition carries no listener or the options do not
    /// parse; staging validation reports those).
    fn intended_listener(&self, name: &str) -> Option<std::net::SocketAddr> {
        let definitions = lock(&self.definitions);
        let definition = definitions.get(name)?;
        let port: u16 = definition.options.get("listen_port")?.parse().ok()?;
        let host: std::net::IpAddr = definition
            .options
            .get("listen_host")
            .and_then(|text| text.parse().ok())
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        Some(std::net::SocketAddr::new(host, port))
    }

    /// Reconciles the manager back to the current candidate (used after
    /// supervisor or publish failures). Returns whether runtime and
    /// durable intent agree afterwards.
    async fn reconcile_back(&self) -> bool {
        let candidate = match self.candidate_set() {
            Ok(candidate) => candidate,
            Err(_) => return false,
        };
        if self
            .manager
            .reconcile(Arc::new(candidate), CONTROL_ROLLBACK_DEADLINE)
            .await
            .is_err()
        {
            return false;
        }
        self.verify_agreement().is_ok()
    }

    /// Verifies durable intent and runtime generation agree: every
    /// running name has a manager runtime and no extra runtimes exist.
    fn verify_agreement(&self) -> Result<(), ControlError> {
        let running = lock(&self.running);
        for name in running.iter() {
            if !self.manager.has_runtime(name) {
                return Err(ControlError::Manager("runtime missing after transition"));
            }
        }
        for runtime in self.manager.all_service_runtimes() {
            if !running.contains(&runtime.spec_id) {
                return Err(ControlError::Manager("extra runtime after transition"));
            }
        }
        Ok(())
    }

    /// Starts supervisors for diff-added/replaced runtimes in `names`.
    /// Continues past individual supervisor failures (per-definition
    /// isolation) and reports the first failure; every failure is
    /// recorded in the failed map regardless. Without an installed
    /// scope (unit scope) runtimes commit without loops.
    async fn sync_names(
        &self,
        names: &[String],
        diff: &[i2pr_service_tunnels::ServiceDiff],
    ) -> Result<(), ControlError> {
        use i2pr_service_tunnels::DiffClass;
        let generation = self.manager.committed_generation_id().unwrap_or(0);
        let scope = match (
            lock(&self.children).clone(),
            lock(&self.cancellation).clone(),
        ) {
            (Some(children), Some(cancellation)) => Some((children, cancellation)),
            _ => None,
        };
        let mut first_error: Option<ControlError> = None;
        for entry in diff {
            if !names.iter().any(|name| name == &entry.id) {
                continue;
            }
            if !matches!(
                entry.class,
                DiffClass::Add | DiffClass::ReplaceListener | DiffClass::ReplaceDestination
            ) {
                continue;
            }
            let Some(runtime) = self.manager.service_runtime_for_spec(&entry.id) else {
                continue;
            };
            let Some((children, cancellation)) = scope.clone() else {
                continue;
            };
            if let Err(error) =
                self.manager
                    .start_supervisors(vec![runtime], &children, cancellation.clone())
            {
                let reason = static_manager_reason(&error);
                lock(&self.failed).insert(entry.id.clone(), reason);
                if first_error.is_none() {
                    first_error = Some(ControlError::Manager(reason));
                }
                continue;
            }
            lock(&self.supervised).insert(entry.id.clone(), generation);
            lock(&self.failed).remove(&entry.id);
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    /// Records a successful transition: clears failure, drops the
    /// transitioning flag.
    fn record_success(&self, name: &str) {
        lock(&self.failed).remove(name);
        lock(&self.transitioning).remove(name);
    }

    /// Reads one control definition.
    fn definition(&self, name: &str) -> Option<ControlDefinition> {
        lock(&self.definitions).get(name).cloned()
    }

    /// Classifies one control-owned name for `get`.
    fn control_status(
        &self,
        name: &str,
        definition: &ControlDefinition,
    ) -> i2pr_i2pcontrol::TunnelStatus {
        use i2pr_i2pcontrol::TunnelStatus;
        if !definition.tunnel_type.has_plan291_backend() {
            return TunnelStatus::Unsupported;
        }
        let transitioning = lock(&self.transitioning).contains(name);
        let running = lock(&self.running).contains(name);
        if transitioning {
            return if running {
                TunnelStatus::Starting
            } else {
                TunnelStatus::Stopping
            };
        }
        if running && lock(&self.failed).contains_key(name) {
            return TunnelStatus::Failed;
        }
        if running && self.manager.has_runtime(name) {
            return TunnelStatus::Running;
        }
        if running {
            return TunnelStatus::Failed;
        }
        TunnelStatus::Stopped
    }

    /// Serializes one control-owned `get` response: actual runtime state
    /// reported separately from persisted intent.
    fn control_response(&self, name: &str, definition: &ControlDefinition) -> serde_json::Value {
        use i2pr_i2pcontrol::SECRET_OPTIONS;
        let status = self.control_status(name, definition);
        let running = lock(&self.running).contains(name);
        let bind = self
            .manager
            .client_listener_address(name)
            .map(|address| address.to_string())
            .or_else(|| {
                definition.options.get("listen_port").map(|port| {
                    let address = definition
                        .options
                        .get("listen_host")
                        .cloned()
                        .unwrap_or_else(|| "127.0.0.1".to_owned());
                    format!("{address}:{port}")
                })
            });
        let destination = self.manager.service_destination_b64(name);
        let connections = self.manager.active_connections(name);
        // Secret-classified options never appear, even though the 289
        // subset cannot store them: filter by key, key-only display.
        let mut options = serde_json::Map::new();
        for (key, value) in &definition.options {
            if SECRET_OPTIONS.contains(&key.as_str()) {
                options.insert(
                    key.clone(),
                    serde_json::Value::String("[redacted]".to_owned()),
                );
            } else {
                options.insert(key.clone(), serde_json::Value::String(value.clone()));
            }
        }
        serde_json::json!({
            "name": name,
            "provenance": TunnelProvenance::ControlOwned.as_str(),
            "type": definition.tunnel_type.name(),
            "options": options,
            "start_on_load": definition.start_on_load,
            "running": running,
            "status": status.name(),
            "generation": self.store_generation(),
            "bind": bind,
            "destination": destination,
            "active_connections": connections,
        })
    }

    /// Serializes one startup-owned summary (inspectable, immutable).
    fn startup_response(&self, name: &str) -> Result<serde_json::Value, ControlError> {
        let spec = self
            .startup
            .tunnels
            .iter()
            .find(|spec| spec.id.as_str() == name)
            .ok_or_else(|| ControlError::UnknownTunnel(name.to_owned()))?;
        Ok(serde_json::json!({
            "name": name,
            "provenance": TunnelProvenance::StartupOwned.as_str(),
            "kind": spec.kind.as_str(),
            "enabled": spec.enabled,
            "listener": spec.listener.map(|listener| listener.socket().to_string()),
        }))
    }

    /// `get`: actual runtime state plus persisted intent. Absent name
    /// returns the bounded whole inventory (startup summaries plus
    /// control-owned detail).
    pub fn get(&self, name: Option<&str>) -> Result<serde_json::Value, ControlError> {
        if let Some(name) = name {
            if let Some(definition) = self.definition(name) {
                return Ok(self.control_response(name, &definition));
            }
            if self.startup_name(name).is_some() {
                return self.startup_response(name);
            }
            return Err(ControlError::UnknownTunnel(name.to_owned()));
        }
        let mut startup = serde_json::Map::new();
        for spec in &self.startup.tunnels {
            startup.insert(
                spec.id.as_str().to_owned(),
                self.startup_response(spec.id.as_str())?,
            );
        }
        let mut control = serde_json::Map::new();
        for (name, definition) in lock(&self.definitions).clone() {
            control.insert(name.clone(), self.control_response(&name, &definition));
        }
        Ok(serde_json::json!({"startup": startup, "control": control}))
    }

    /// `create`: validates the complete candidate before any side
    /// effect, stages persistence, reconciles, publishes, and starts
    /// supervisors for running intent.
    pub async fn create(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("create requires name"))?;
        let tunnel_type = request
            .tunnel_type
            .ok_or(ControlError::InvalidRequest("create requires type"))?;
        if self.definition(name).is_some() || self.startup_name(name).is_some() {
            return Err(ControlError::NameCollision(name.to_owned()));
        }
        let start_on_load = request
            .options
            .get("start_on_load")
            .map(|value| value == "true")
            .unwrap_or(true);
        let definition = normalize_definition(name, tunnel_type, &request.options, start_on_load)?;
        let probe = build_control_spec(&definition)?;
        self.check_listener_against_startup(&probe)?;
        let running_before = lock(&self.running).clone();
        lock(&self.definitions).insert(name.to_owned(), definition);
        // Creation carries running intent when the definition should
        // run now: explicit `start_on_load` intent starts immediately.
        if lock(&self.definitions)
            .get(name)
            .map(|definition| definition.start_on_load)
            .unwrap_or(false)
        {
            lock(&self.running).insert(name.to_owned());
        }
        lock(&self.transitioning).insert(name.to_owned());
        let probe = self.is_running(name);
        match self.commit_locked(&guard, &[name.to_owned()], probe).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(transition_response(name, generation, self.is_running(name)))
            }
            Err(error) => {
                self.rollback_state(name, None, &running_before);
                Err(error)
            }
        }
    }

    /// `edit`: mutates a control-owned definition (options and/or
    /// rename). Type changes are rejected by the envelope.
    pub async fn edit(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("edit requires name"))?;
        let prior = self
            .definition(name)
            .ok_or_else(|| unknown_or_startup(name, &self.startup))?;
        let target_name = request.new_name.as_deref().unwrap_or(name);
        if target_name != name
            && (self.definition(target_name).is_some() || self.startup_name(target_name).is_some())
        {
            return Err(ControlError::NameCollision(target_name.to_owned()));
        }
        // Merge options over the prior definition, then normalize the
        // complete candidate before any side effect.
        let mut merged = prior.options.clone();
        for (key, value) in &request.options {
            merged.insert(key.clone(), value.clone());
        }
        let start_on_load = merged
            .get("start_on_load")
            .map(|value| value == "true")
            .unwrap_or(prior.start_on_load);
        let running_before = lock(&self.running).clone();
        if target_name != name {
            // Rename travels as delete-plus-create across separate
            // commits so the old listener is released (reaped) before
            // the new runtime binds the same socket. Every crash window
            // preserves both definitions (extra stopped definitions at
            // worst); nothing is ever lost between commits.
            return self
                .rename_locked(&guard, name, target_name, &prior, &merged, &running_before)
                .await;
        }
        let candidate =
            normalize_definition(target_name, prior.tunnel_type, &merged, start_on_load)?;
        let probe = build_control_spec(&candidate)?;
        self.check_listener_against_startup(&probe)?;
        let running_before = lock(&self.running).clone();
        let was_running = running_before.contains(target_name);
        lock(&self.definitions).insert(target_name.to_owned(), candidate.clone());
        lock(&self.transitioning).insert(target_name.to_owned());
        // A running definition whose edit restages its runtime travels
        // as stop-plus-start: staging rebinds sockets while the old
        // runtime still holds them, so the old runtime must drain (and
        // be reaped) before the new one stages.
        if was_running && Self::edit_restages(&prior, &candidate) {
            return self
                .edit_restage_locked(&guard, target_name, &prior, &running_before)
                .await;
        }
        match self
            .commit_locked(&guard, &[target_name.to_owned()], false)
            .await
        {
            Ok((generation, _)) => {
                self.record_success(target_name);
                Ok(transition_response(
                    target_name,
                    generation,
                    self.is_running(target_name),
                ))
            }
            Err(error) => {
                self.rollback_state(target_name, Some((name, prior)), &running_before);
                Err(error)
            }
        }
    }

    /// Restaging edit on a running definition: stop phase drains the
    /// old runtime, start phase reaps (freeing the socket) and stages
    /// the new runtime. A stop-phase failure rolls the mirror fully
    /// back; a start-phase failure leaves the candidate committed but
    /// stopped with running intent restored, so the status reports
    /// failed and the next start converges.
    async fn edit_restage_locked(
        &self,
        guard: &tokio::sync::MutexGuard<'_, ()>,
        name: &str,
        prior: &ControlDefinition,
        running_before: &BTreeSet<String>,
    ) -> Result<serde_json::Value, ControlError> {
        lock(&self.running).remove(name);
        if let Err(error) = self.commit_locked(guard, &[name.to_owned()], false).await {
            self.rollback_state(name, Some((name, prior.clone())), running_before);
            return Err(error);
        }
        lock(&self.running).insert(name.to_owned());
        match self.commit_locked(guard, &[name.to_owned()], true).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(transition_response(name, generation, true))
            }
            Err(error) => {
                lock(&self.running).insert(name.to_owned());
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// Rename as crash-safe delete-plus-create: phase one commits the
    /// new stopped definition beside the old one, phase two removes
    /// the old definition (releasing its listener), phase three starts
    /// the new definition when the old one ran. Every intermediate
    /// generation preserves both definitions; a crash between commits
    /// leaves an extra stopped definition at worst, never a loss.
    async fn rename_locked(
        &self,
        guard: &tokio::sync::MutexGuard<'_, ()>,
        name: &str,
        target_name: &str,
        prior: &ControlDefinition,
        merged: &BTreeMap<String, String>,
        running_before: &BTreeSet<String>,
    ) -> Result<serde_json::Value, ControlError> {
        let was_running = running_before.contains(name);
        let start_on_load = merged
            .get("start_on_load")
            .map(|value| value == "true")
            .unwrap_or(prior.start_on_load);
        let candidate =
            normalize_definition(target_name, prior.tunnel_type, merged, start_on_load)?;
        let probe = build_control_spec(&candidate)?;
        self.check_listener_against_startup(&probe)?;
        // Phase one: add the new definition stopped.
        lock(&self.definitions).insert(target_name.to_owned(), candidate);
        lock(&self.transitioning).insert(target_name.to_owned());
        if let Err(error) = self
            .commit_locked(guard, &[target_name.to_owned()], false)
            .await
        {
            self.rollback_state(target_name, None, running_before);
            return Err(error);
        }
        // Phase two: remove the old definition.
        lock(&self.definitions).remove(name);
        lock(&self.running).remove(name);
        lock(&self.supervised).remove(name);
        lock(&self.failed).remove(name);
        if let Err(error) = self.commit_locked(guard, &[name.to_owned()], false).await {
            // Restore the old definition: the store still publishes a
            // generation containing both rows (phase one published), so
            // the mirror is rebuilt to match it exactly.
            lock(&self.definitions).insert(name.to_owned(), prior.clone());
            if was_running {
                lock(&self.running).insert(name.to_owned());
            }
            lock(&self.transitioning).remove(target_name);
            lock(&self.transitioning).remove(name);
            return Err(error);
        }
        lock(&self.transitioning).remove(name);
        // Phase three: start the new definition when the old one ran.
        if was_running {
            lock(&self.running).insert(target_name.to_owned());
            match self
                .commit_locked(guard, &[target_name.to_owned()], true)
                .await
            {
                Ok((generation, _)) => {
                    self.record_success(target_name);
                    Ok(transition_response(target_name, generation, true))
                }
                Err(error) => {
                    lock(&self.running).remove(target_name);
                    lock(&self.transitioning).remove(target_name);
                    Err(error)
                }
            }
        } else {
            self.record_success(target_name);
            Ok(transition_response(
                target_name,
                self.store_generation(),
                false,
            ))
        }
    }

    /// `delete`: removes a control-owned definition and drains its runtime.
    pub async fn delete(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("delete requires name"))?;
        let prior = self
            .definition(name)
            .ok_or_else(|| unknown_or_startup(name, &self.startup))?;
        let running_before = lock(&self.running).clone();
        lock(&self.definitions).remove(name);
        lock(&self.running).remove(name);
        lock(&self.failed).remove(name);
        lock(&self.transitioning).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], false).await {
            Ok((generation, _)) => {
                lock(&self.supervised).remove(name);
                self.record_success(name);
                Ok(transition_response(name, generation, false))
            }
            Err(error) => {
                self.rollback_state(name, Some((name, prior)), &running_before);
                Err(error)
            }
        }
    }

    /// `start`: runtime action over the exact control-owned definition.
    /// Already-running names answer the current state without a new
    /// generation.
    pub async fn start(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("start requires name"))?;
        if self.definition(name).is_none() {
            return Err(unknown_or_startup(name, &self.startup));
        }
        if self.is_running(name) {
            return Ok(transition_response(name, self.store_generation(), true));
        }
        self.start_inner(name)
            .await
            .map(|generation| transition_response(name, generation, true))
    }

    /// `stop`: runtime action over the exact control-owned definition.
    /// Already-stopped names answer without a new generation.
    pub async fn stop(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("stop requires name"))?;
        if self.definition(name).is_none() {
            return Err(unknown_or_startup(name, &self.startup));
        }
        if !self.is_running(name) {
            lock(&self.transitioning).remove(name);
            return Ok(transition_response(name, self.store_generation(), false));
        }
        let running_before = lock(&self.running).clone();
        lock(&self.running).remove(name);
        lock(&self.failed).remove(name);
        lock(&self.transitioning).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], false).await {
            Ok((generation, _)) => {
                lock(&self.supervised).remove(name);
                self.record_success(name);
                Ok(transition_response(name, generation, false))
            }
            Err(error) => {
                *lock(&self.running) = running_before;
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// `restart`: stop then start over the exact control-owned
    /// definition under one op lock. Server identities persist across
    /// the transition through the manager's service-destination records.
    pub async fn restart(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("restart requires name"))?;
        if self.definition(name).is_none() {
            return Err(unknown_or_startup(name, &self.startup));
        }
        let running_before = lock(&self.running).clone();
        lock(&self.running).remove(name);
        lock(&self.failed).remove(name);
        lock(&self.transitioning).insert(name.to_owned());
        if let Err(error) = self.commit_locked(&guard, &[name.to_owned()], false).await {
            *lock(&self.running) = running_before;
            lock(&self.transitioning).remove(name);
            return Err(error);
        }
        lock(&self.supervised).remove(name);
        lock(&self.running).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], true).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(transition_response(name, generation, true))
            }
            Err(error) => {
                *lock(&self.running) = running_before;
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// Shared start path (also used at daemon startup): records running
    /// intent, commits, and starts supervisors for the transitioned
    /// diff. Acquires the op lock.
    async fn start_inner(&self, name: &str) -> Result<u64, ControlError> {
        let guard = self.op_lock.lock().await;
        let definition = self
            .definition(name)
            .ok_or_else(|| unknown_or_startup(name, &self.startup))?;
        if !definition.tunnel_type.has_plan291_backend() {
            return Err(ControlError::UnsupportedType(
                definition.tunnel_type.name().to_owned(),
            ));
        }
        let running_before = lock(&self.running).clone();
        lock(&self.running).insert(name.to_owned());
        lock(&self.transitioning).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], true).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(generation)
            }
            Err(error) => {
                *lock(&self.running) = running_before;
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// Restores the durable mirror plus running intent after a failed
    /// commit. Staged-but-unpublished generation files are orphans;
    /// retention and startup cleanup remove them.
    fn rollback_state(
        &self,
        name: &str,
        prior: Option<(&str, ControlDefinition)>,
        running_before: &BTreeSet<String>,
    ) {
        {
            let mut definitions = lock(&self.definitions);
            match prior {
                Some((prior_name, definition)) => {
                    if prior_name != name {
                        definitions.remove(name);
                    }
                    definitions.insert(prior_name.to_owned(), definition);
                }
                None => {
                    definitions.remove(name);
                }
            }
        }
        *lock(&self.running) = running_before.clone();
        lock(&self.transitioning).remove(name);
    }

    /// Whether a name currently carries running intent.
    fn is_running(&self, name: &str) -> bool {
        lock(&self.running).contains(name)
    }

    /// Dispatches one decoded TunnelManager request.
    pub async fn dispatch(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        match request.action {
            TunnelAction::Get => self.get(request.name.as_deref()),
            TunnelAction::Create => self.create(request).await,
            TunnelAction::Edit => self.edit(request).await,
            TunnelAction::Delete => self.delete(request).await,
            TunnelAction::Start => self.start(request).await,
            TunnelAction::Stop => self.stop(request).await,
            TunnelAction::Restart => self.restart(request).await,
        }
    }
}

/// Locks a std mutex, recovering from poisoning with the inner value.
/// Poisoning implies a panicking holder elsewhere; inspection and
/// control prefer degraded continuity over a cascade. Every use is a
/// short critical section with no awaits and no I/O.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Mutation acknowledgement shape.
fn transition_response(name: &str, generation: u64, running: bool) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "running": running,
        "generation": generation,
    })
}

/// Classifies an unknown name as startup-rejected or unknown.
fn unknown_or_startup(name: &str, startup: &ServiceTunnelSet) -> ControlError {
    if startup.tunnels.iter().any(|spec| spec.id.as_str() == name) {
        ControlError::StartupMutationRejected(name.to_owned())
    } else {
        ControlError::UnknownTunnel(name.to_owned())
    }
}

/// Static reason for a daemon manager failure (never echoes values).
fn static_manager_reason(error: &crate::service_tunnels::ServiceTunnelError) -> &'static str {
    use crate::service_tunnels::ServiceTunnelError as E;
    match error {
        E::InvalidConfig(_) => "manager configuration rejected",
        E::DestinationRuntime(_) => "destination runtime failed",
        E::Bind(_) => "listener bind failed",
        E::Storage(_) => "service storage failed",
    }
}

/// Static reason for a runtime-neutral set failure.
fn static_manager_reason2(error: &i2pr_service_tunnels::ServiceTunnelError) -> &'static str {
    use i2pr_service_tunnels::ServiceTunnelError as E;
    match error {
        E::ExceedsCeiling { .. } => "aggregate exceeds a service ceiling",
        E::ContradictoryOptions { .. } => "candidate contradicts a service kind",
        E::DuplicateId { .. } => "duplicate service id",
        E::DuplicateListener { .. } => "duplicate listener",
        _ => "candidate validation failed",
    }
}

/// Static reason for a control failure (recovery paths only).
fn static_control_reason(error: &ControlError) -> &'static str {
    match error {
        ControlError::UnknownTunnel(_) => "unknown tunnel",
        ControlError::NameCollision(_) => "name collision",
        ControlError::StartupMutationRejected(_) => "startup-owned",
        ControlError::UnsupportedType(_) => "unsupported type",
        ControlError::UnsupportedOption(_) => "unsupported option",
        ControlError::InvalidOption { reason, .. } => reason,
        ControlError::ContradictoryOptions { reason, .. } => reason,
        ControlError::InvalidRequest(reason) => reason,
        ControlError::AggregateRejected(reason) => reason,
        ControlError::Store(_) => "store failed",
        ControlError::Manager(reason) => reason,
        ControlError::PublishFailed { reason, .. } => reason,
        ControlError::Unavailable => "control unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager(data_dir: &Path) -> Arc<ServiceTunnelManager> {
        let config = ServiceTunnelManagerConfig {
            data_dir: data_dir.to_path_buf(),
            aggregate_connection_ceiling: 1024,
            per_service_connection_ceiling: 128,
            specs: Arc::new(ServiceTunnelSet::new()),
            aliases: Arc::new(i2pr_service_tunnels::StaticAliasTable::new()),
        };
        Arc::new(ServiceTunnelManager::new(config).expect("manager builds"))
    }

    fn test_control(data_dir: &Path) -> TunnelControlState {
        let store = ControlStore::open(data_dir).expect("store opens");
        TunnelControlState::new(store, ServiceTunnelSet::new(), test_manager(data_dir))
    }

    fn create_request(
        name: &str,
        tunnel_type: TunnelType,
        options: BTreeMap<String, String>,
    ) -> TunnelManagerRequest {
        TunnelManagerRequest {
            action: TunnelAction::Create,
            name: Some(name.to_owned()),
            tunnel_type: Some(tunnel_type),
            new_name: None,
            options,
        }
    }

    fn named_request(action: TunnelAction, name: &str) -> TunnelManagerRequest {
        TunnelManagerRequest {
            action,
            name: Some(name.to_owned()),
            tunnel_type: None,
            new_name: None,
            options: BTreeMap::new(),
        }
    }

    fn client_options(destination: &str, port: u16) -> BTreeMap<String, String> {
        let mut options = BTreeMap::new();
        options.insert("target_destination".to_owned(), destination.to_owned());
        options.insert("listen_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("listen_port".to_owned(), port.to_string());
        options
    }

    fn server_options(target: &str) -> BTreeMap<String, String> {
        let (host, port) = target.rsplit_once(':').expect("host:port");
        let mut options = BTreeMap::new();
        options.insert("target_host".to_owned(), host.to_owned());
        options.insert("target_port".to_owned(), port.to_owned());
        options
    }

    /// Distinct loopback ports for multi-service tests. Each call
    /// reserves a unique block process-wide (atomic counter), so
    /// parallel tests never share a port; the base spreads by process
    /// id so parallel test binaries rarely share a block either.
    /// Services bind them immediately, so the reservation race is
    /// negligible.
    fn distinct_ports(count: u16) -> Vec<u16> {
        use std::sync::atomic::{AtomicU16, Ordering};
        static NEXT_BLOCK: AtomicU16 = AtomicU16::new(0);
        static BASE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
        let base = *BASE.get_or_init(|| 25_000 + (std::process::id() % 5_000) as u16);
        let block = NEXT_BLOCK.fetch_add(count, Ordering::Relaxed);
        (0..count).map(|n| base + (block + n) * 37).collect()
    }

    // All control tests run through a throwaway current-thread runtime:
    // the coordinator awaits manager reconcile and file syncs.
    fn block_on<F, T>(future: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    fn test_scope() -> (i2pr_runtime::ChildScope, i2pr_runtime::CancellationToken) {
        use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
        let parent = CancellationToken::new();
        let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
        (scope, parent)
    }

    #[test]
    fn plan289_store_round_trip_and_recovery() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        assert_eq!(store.current_id(), 0);
        let mut definitions = BTreeMap::new();
        definitions.insert(
            "alpha".to_owned(),
            ControlDefinition {
                name: "alpha".to_owned(),
                tunnel_type: TunnelType::Client,
                options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0),
                start_on_load: true,
            },
        );
        let first = store.stage(&definitions).expect("stage");
        store.publish(first).expect("publish");
        assert_eq!(store.current_id(), first);
        let loaded = store.load().expect("load");
        assert_eq!(loaded.generation_id, first);
        assert_eq!(loaded.definitions.len(), 1);
        assert_eq!(loaded.definitions[0].name, "alpha");
        // Second generation publishes; the first stays as the prior.
        let second = store.stage(&BTreeMap::new()).expect("stage empty");
        store.publish(second).expect("publish");
        assert_eq!(store.current_id(), second);
        // Corrupt the latest file: recovery falls back to the prior.
        std::fs::write(store.generation_path(second), b"{corrupt").expect("corrupt");
        let recovered = store.load().expect("recovery loads");
        assert_eq!(recovered.generation_id, first);
        // Corrupt the pointer too: scan fallback finds the newest valid.
        std::fs::write(store.pointer_path(), b"{corrupt").expect("corrupt pointer");
        let scanned = store.load().expect("scan loads");
        assert_eq!(scanned.generation_id, first);
    }

    #[test]
    fn plan289_store_rejects_symlinks_and_oversized_reads() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        let mut definitions = BTreeMap::new();
        definitions.insert(
            "alpha".to_owned(),
            ControlDefinition {
                name: "alpha".to_owned(),
                tunnel_type: TunnelType::Client,
                options: BTreeMap::new(),
                start_on_load: false,
            },
        );
        let id = store.stage(&definitions).expect("stage");
        store.publish(id).expect("publish");
        // Oversized generation file: bounded read rejects before parse.
        let big = vec![b'x'; MAX_GENERATION_BYTES + 1];
        std::fs::write(store.generation_path(999), &big).expect("write big");
        assert_eq!(store.load().expect("load skips big").generation_id, id);
        std::fs::remove_file(store.generation_path(999)).expect("remove big");
        // Symlink generation file: rejected as non-regular.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(store.generation_path(id), store.generation_path(1000))
                .expect("symlink");
            assert_eq!(store.load().expect("load skips symlink").generation_id, id);
            std::fs::remove_file(store.generation_path(1000)).expect("remove link");
        }
        // Symlink root: open refuses.
        #[cfg(unix)]
        {
            let link_parent = tempfile::tempdir().expect("tempdir");
            let link = link_parent.path().join("linked");
            std::os::unix::fs::symlink(directory.path(), &link).expect("symlink root");
            let root = link.join(CONTROL_STATE_SUBDIR).join(CONTROL_TUNNELS_SUBDIR);
            std::fs::create_dir_all(&root).expect("create through link");
            assert!(
                matches!(ControlStore::open(&link), Err(StoreError::NotRegular)),
                "symlink data dir refused"
            );
        }
    }

    #[test]
    fn plan289_store_enforces_ceilings_and_retention() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        // 33 definitions exceed the 32-service ceiling at stage time.
        let mut definitions = BTreeMap::new();
        for n in 0..MAX_SERVICE_TUNNELS + 1 {
            definitions.insert(
                format!("tunnel-{n:03}"),
                ControlDefinition {
                    name: format!("tunnel-{n:03}"),
                    tunnel_type: TunnelType::Client,
                    options: BTreeMap::new(),
                    start_on_load: false,
                },
            );
        }
        assert_eq!(store.stage(&definitions), Err(StoreError::OverBound));
        // Three publications keep current plus prior only.
        let mut ids = Vec::new();
        for _ in 0..3 {
            let id = store.stage(&BTreeMap::new()).expect("stage");
            store.publish(id).expect("publish");
            ids.push(id);
        }
        assert_eq!(store.current_id(), ids[2]);
        assert!(!store.generation_path(ids[0]).exists(), "oldest pruned");
        assert!(store.generation_path(ids[1]).exists(), "prior kept");
        assert!(store.generation_path(ids[2]).exists(), "current kept");
    }

    #[test]
    fn plan289_store_orphan_cleanup() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        // Two staged files, nothing published: staged files are never
        // loadable, so cleanup drops them all.
        let first = store.stage(&BTreeMap::new()).expect("stage");
        let second = store.stage(&BTreeMap::new()).expect("stage");
        assert_ne!(first, second);
        store.cleanup_orphans();
        assert_eq!(store.list_staged_ids().expect("list").len(), 0);
        assert_eq!(store.current_id(), 0);
        // Publish, then stage an orphan: cleanup drops the staged file
        // while the published generation stays.
        let published = store.stage(&BTreeMap::new()).expect("stage");
        store.publish(published).expect("publish");
        let _ = store.stage(&BTreeMap::new()).expect("stage orphan");
        store.cleanup_orphans();
        assert_eq!(store.list_staged_ids().expect("staged").len(), 0);
        assert_eq!(store.current_id(), published);
    }

    #[test]
    fn plan290_family_mapping_ten_backends() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        for (tunnel_type, kind) in [
            (TunnelType::Client, ServiceTunnelKind::GenericClient),
            (TunnelType::Server, ServiceTunnelKind::GenericServer),
            (TunnelType::HttpClient, ServiceTunnelKind::HttpClient),
            (TunnelType::Socks, ServiceTunnelKind::Socks5Client),
            (TunnelType::IrcClient, ServiceTunnelKind::IrcClient),
            (TunnelType::IrcServer, ServiceTunnelKind::IrcServer),
            // Plan 290: composed families over the existing M10
            // manager and shared primitives.
            (TunnelType::ConnectClient, ServiceTunnelKind::ConnectClient),
            (TunnelType::SocksIrc, ServiceTunnelKind::SocksIrc),
            (TunnelType::HttpServer, ServiceTunnelKind::HttpServer),
            (
                TunnelType::HttpBidirServer,
                ServiceTunnelKind::HttpBidirServer,
            ),
            // Plan 291: Streamr families over the
            // repliable-datagram substrate.
            (TunnelType::StreamrClient, ServiceTunnelKind::StreamrClient),
            (TunnelType::StreamrServer, ServiceTunnelKind::StreamrServer),
        ] {
            assert_eq!(map_tunnel_type(tunnel_type).expect("backend"), kind);
        }
    }

    #[test]
    fn plan291_streamr_control_spec_builds() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        // Server: explicit loopback UDP source plus bounded
        // overrides; no TCP listener or target.
        let mut options = BTreeMap::new();
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5001".to_owned());
        options.insert("streamr_expiry".to_owned(), "90000".to_owned());
        options.insert("streamr_max_subscribers".to_owned(), "5".to_owned());
        let definition = ControlDefinition {
            name: "pub".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("server spec builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrServer);
        let udp = spec
            .streamr_options
            .expect("options")
            .local_udp
            .expect("udp");
        assert_eq!(udp.port(), 5001);
        assert!(spec.listener.is_none());
        assert!(spec.target.is_none());
        // Client: producer destination plus loopback UDP target.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        options.insert("target_i2p_port".to_owned(), "7".to_owned());
        let definition = ControlDefinition {
            name: "sub".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("client spec builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrClient);
        assert!(spec.destination.is_some());
        assert_eq!(spec.streamr_options.expect("options").target_i2p_port, 7);
        // Missing UDP endpoint fails before allocation.
        let definition = ControlDefinition {
            name: "bad".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options: BTreeMap::new(),
            start_on_load: false,
        };
        assert!(build_control_spec(&definition).is_err());
        // Cross-kind keys fail: server-only key on the client,
        // TCP target keys on either Streamr half.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        options.insert("streamr_expiry".to_owned(), "90000".to_owned());
        let definition = ControlDefinition {
            name: "xclient".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        assert!(build_control_spec(&definition).is_err());
    }

    #[test]
    fn plan289_option_subset_has_real_effect() {
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 8180);
        options.insert("max_streams".to_owned(), "24".to_owned());
        options.insert("start_on_load".to_owned(), "false".to_owned());
        let definition = ControlDefinition {
            name: "alpha".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("spec builds");
        assert_eq!(spec.max_connections, 24);
        assert_eq!(spec.listener.expect("listener").socket().port(), 8180);
        assert!(spec.destination.is_some());
        // Defaults: ephemeral port, loopback bind, 16 connections.
        let definition = ControlDefinition {
            name: "beta".to_owned(),
            tunnel_type: TunnelType::Client,
            options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
                .into_iter()
                .filter(|(key, _)| *key != "listen_port" && *key != "listen_host")
                .collect(),
            start_on_load: true,
        };
        let spec = build_control_spec(&definition).expect("defaults build");
        assert_eq!(spec.listener.expect("listener").socket().port(), 0);
        assert_eq!(spec.max_connections, DEFAULT_CONTROL_MAX_CONNECTIONS);
        // Server kinds map target_host/target_port with real effect.
        let options = server_options("127.0.0.1:9090");
        let definition = ControlDefinition {
            name: "srv".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: true,
        };
        let spec = build_control_spec(&definition).expect("server builds");
        assert!(spec.target.is_some());
        assert!(spec.listener.is_none());
    }

    #[test]
    fn plan289_unsupported_options_rejected_before_storage() {
        // Every secret-classified key is rejected even though none can
        // be stored: belt and suspenders against secret persistence.
        for secret in i2pr_i2pcontrol::SECRET_OPTIONS {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(secret.to_owned(), "hunter2".to_owned());
            let error = normalize_definition("alpha", TunnelType::Client, &options, true)
                .expect_err("secret rejected");
            assert_eq!(error, ControlError::UnsupportedOption(secret.to_owned()));
        }
        // Known-but-not-yet-supported options fail explicitly too,
        // naming the owning plan for blocked and corrective cells.
        for key in [
            "outproxy",
            "description",
            "leaseset_type",
            "tunnel_backup_quantity",
        ] {
            if i2pr_i2pcontrol::tunnel_options::find_option(key).is_err() {
                continue;
            }
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), "x".to_owned());
            let error = normalize_definition("alpha", TunnelType::Client, &options, true)
                .expect_err("unsupported rejected");
            assert!(
                matches!(&error, ControlError::UnsupportedOption(message) if message.contains(key)),
                "unexpected error: {error:?}"
            );
        }
        // Contradictions and malformed values fail with static reasons.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        let error = normalize_definition("srv", TunnelType::Server, &options, true)
            .expect_err("destination on server fails");
        assert!(matches!(error, ControlError::ContradictoryOptions { .. }));
        let mut options = BTreeMap::new();
        options.insert("target_host".to_owned(), "unix:/tmp/sock".to_owned());
        options.insert("target_port".to_owned(), "9".to_owned());
        let error = normalize_definition("srv", TunnelType::Server, &options, true)
            .expect_err("unix target fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        let options = client_options("", 0);
        let error = normalize_definition("alpha", TunnelType::Client, &options, true)
            .expect_err("empty destination fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        // Debug never prints values (keys are schema, values are data).
        let definition = ControlDefinition {
            name: "alpha".to_owned(),
            tunnel_type: TunnelType::Client,
            options: client_options("example.i2p", 8180),
            start_on_load: true,
        };
        let debug = format!("{:?}", definition);
        assert!(!debug.contains("example.i2p"), "values redacted: {debug}");
        assert!(!debug.contains("8180"), "values redacted: {debug}");
        assert!(debug.contains("alpha"), "name present: {debug}");
    }

    #[test]
    fn plan292_shaping_options_have_real_effect() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        // Symmetric quantity/length drive the pool projection; the
        // rest reproduces the balanced defaults.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("tunnel_quantity".to_owned(), "4".to_owned());
        options.insert("tunnel_length".to_owned(), "3".to_owned());
        let definition = ControlDefinition {
            name: "shaped".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("shaped spec builds");
        assert_eq!(spec.shaping.inbound_quantity, 4);
        assert_eq!(spec.shaping.outbound_quantity, 4);
        assert_eq!(spec.shaping.length_hops, 3);
        let projected = crate::service_tunnels::ServiceTunnelManager::destination_config_for(&spec);
        assert_eq!(projected.inbound_target(), 4);
        assert_eq!(projected.outbound_target(), 4);
        assert_eq!(projected.length_hops(), 3);
        // Per-direction quantities override independently.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("inbound_quantity".to_owned(), "5".to_owned());
        options.insert("outbound_quantity".to_owned(), "1".to_owned());
        let definition = ControlDefinition {
            name: "split".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("split spec builds");
        assert_eq!(spec.shaping.inbound_quantity, 5);
        assert_eq!(spec.shaping.outbound_quantity, 1);
        assert_eq!(spec.shaping.length_hops, 2);
        // Agreeing per-direction lengths are accepted ...
        let mut options = server_options("127.0.0.1:9090");
        options.insert("inbound_length".to_owned(), "1".to_owned());
        options.insert("outbound_length".to_owned(), "1".to_owned());
        let definition = ControlDefinition {
            name: "srvlen".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("server lengths build");
        assert_eq!(spec.shaping.length_hops, 1);
        // ... while differing lengths fail instead of silently
        // dropping one side.
        let mut options = server_options("127.0.0.1:9090");
        options.insert("inbound_length".to_owned(), "1".to_owned());
        options.insert("outbound_length".to_owned(), "3".to_owned());
        let error = normalize_definition("srvbad", TunnelType::Server, &options, false)
            .expect_err("differing lengths fail");
        assert!(matches!(error, ControlError::ContradictoryOptions { .. }));
        // Bounds: quantity 1..=6, length 1..=3 (zero-hop rejected).
        for (key, value) in [
            ("tunnel_quantity", "0"),
            ("tunnel_quantity", "7"),
            ("inbound_quantity", "0"),
            ("tunnel_length", "0"),
            ("tunnel_length", "4"),
            ("outbound_length", "0"),
        ] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("bad", TunnelType::Client, &options, false)
                .expect_err("shaping bound enforced");
            assert!(
                matches!(error, ControlError::InvalidOption { .. }),
                "unexpected error for {key}={value}: {error:?}"
            );
        }
        // Residuals name their owning plan (in-mask kinds only).
        for (key, value) in [
            ("tunnel_backup_quantity", "x"),
            ("tunnel_variance", "x"),
            ("sig_type", "EDDSA_SHA512_ED25519"),
        ] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("bad", TunnelType::Client, &options, false)
                .expect_err("residual rejected");
            assert!(
                matches!(&error, ControlError::UnsupportedOption(message)
                    if message.contains("Plan 296") || message.contains("Plan 293")),
                "unexpected error for {key}: {error:?}"
            );
        }
        // Outproxy provider residual on an in-mask proxy kind.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert(
            "use_outproxy_plugin".to_owned(),
            "http://outproxy.i2p".to_owned(),
        );
        let error = normalize_definition("httpout", TunnelType::HttpClient, &options, false)
            .expect_err("outproxy rejected");
        assert!(
            matches!(&error, ControlError::UnsupportedOption(message) if message.contains("Plan 293")),
            "unexpected error: {error:?}"
        );
        // use_ssl residual on a server kind names Plan 297.
        let mut options = server_options("127.0.0.1:9090");
        options.insert("use_ssl".to_owned(), "true".to_owned());
        let error = normalize_definition("srvssl", TunnelType::Server, &options, false)
            .expect_err("use_ssl rejected");
        assert!(
            matches!(&error, ControlError::UnsupportedOption(message) if message.contains("Plan 297")),
            "unexpected error: {error:?}"
        );
        // Shaping applies to Streamr kinds too (uniform pool sizing).
        let mut options = BTreeMap::new();
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5001".to_owned());
        options.insert("tunnel_quantity".to_owned(), "3".to_owned());
        let definition = ControlDefinition {
            name: "strm".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("streamr shaping builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrServer);
        assert_eq!(spec.shaping.inbound_quantity, 3);
    }

    #[test]
    fn plan292_profile_selects_streaming_windows() {
        // profile=interactive and interactive=true both select the
        // constrained windows; bulk (or absent) keeps balanced.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("profile".to_owned(), "interactive".to_owned());
        let definition = ControlDefinition {
            name: "prof".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("profile builds");
        assert!(spec.streaming_interactive);
        let selected = crate::service_tunnels::ServiceTunnelManager::streaming_config_for(&spec);
        assert_eq!(selected.max_send_window_packets, 16);
        assert_eq!(selected.max_recv_window_packets, 16);
        assert_eq!(selected.delayed_ack_ms, 100);
        let mut options = server_options("127.0.0.1:9090");
        options.insert("interactive".to_owned(), "true".to_owned());
        let definition = ControlDefinition {
            name: "srvprof".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("interactive builds");
        assert!(spec.streaming_interactive);
        // Explicit bulk and absent keys stay balanced.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("profile".to_owned(), "bulk".to_owned());
        options.insert("interactive".to_owned(), "false".to_owned());
        let definition = ControlDefinition {
            name: "bulk".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("bulk builds");
        assert!(!spec.streaming_interactive);
        let selected = crate::service_tunnels::ServiceTunnelManager::streaming_config_for(&spec);
        assert_eq!(
            selected,
            i2pr_client::streaming::config::StreamingConfig::balanced()
        );
        // Unknown profile values fail instead of storing inertly.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("profile".to_owned(), "turbo".to_owned());
        let error = normalize_definition("badprof", TunnelType::Client, &options, false)
            .expect_err("unknown profile fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        // Streamr kinds have no streaming stack: both keys fail.
        for key in ["profile", "interactive"] {
            let mut options = BTreeMap::new();
            options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
            options.insert("local_udp_port".to_owned(), "5001".to_owned());
            options.insert(
                key.to_owned(),
                if key == "profile" {
                    "interactive"
                } else {
                    "true"
                }
                .to_owned(),
            );
            let error =
                normalize_definition("strmprof", TunnelType::StreamrServer, &options, false)
                    .expect_err("streamr profile fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error for {key}: {error:?}"
            );
        }
    }

    #[test]
    fn plan289_create_get_start_stop_delete_lifecycle() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // Create without running intent: committed but stopped.
        let response = block_on(
            control.create(&create_request("alpha", TunnelType::Client, {
                let mut options = client_options(&destination, 0);
                options.insert("start_on_load".to_owned(), "false".to_owned());
                options
            })),
        )
        .expect("create");
        assert_eq!(response["running"], serde_json::json!(false));
        let generation = response["generation"].as_u64().expect("generation");
        assert!(generation >= 1);
        // Get separates persisted intent from runtime state.
        let response = control.get(Some("alpha")).expect("get");
        assert_eq!(response["provenance"], serde_json::json!("control"));
        assert_eq!(response["type"], serde_json::json!("client"));
        assert_eq!(response["start_on_load"], serde_json::json!(false));
        assert_eq!(response["running"], serde_json::json!(false));
        assert_eq!(response["status"], serde_json::json!("stopped"));
        // Start, stop, delete move running intent with new generations.
        let response =
            block_on(control.start(&named_request(TunnelAction::Start, "alpha"))).expect("start");
        assert_eq!(response["running"], serde_json::json!(true));
        assert!(response["generation"].as_u64().expect("gen") > generation);
        let response = control.get(Some("alpha")).expect("get running");
        assert_eq!(response["status"], serde_json::json!("running"));
        assert_eq!(response["running"], serde_json::json!(true));
        let response =
            block_on(control.stop(&named_request(TunnelAction::Stop, "alpha"))).expect("stop");
        assert_eq!(response["running"], serde_json::json!(false));
        let response = control.get(Some("alpha")).expect("get stopped");
        assert_eq!(response["status"], serde_json::json!("stopped"));
        block_on(control.delete(&named_request(TunnelAction::Delete, "alpha"))).expect("delete");
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
    }

    #[test]
    fn plan289_create_validates_before_side_effects() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        // Malformed destination: no store file, no runtime, empty mirror.
        // (Empty fails; arbitrary bounded strings are valid configured
        // destinations per the M10 reference type.)
        let error = block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options("", 0),
        )))
        .expect_err("invalid fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        assert_eq!(control.store.current_id(), 0, "nothing staged");
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
        // Plan 291: streamrclient validates its required fields
        // before any side effect (missing destination and UDP
        // endpoint: no store file, no runtime, empty mirror).
        let error = block_on(control.create(&create_request(
            "stream",
            TunnelType::StreamrClient,
            BTreeMap::new(),
        )))
        .expect_err("invalid fails");
        assert!(matches!(
            error,
            ControlError::InvalidRequest(_) | ControlError::InvalidOption { .. }
        ));
        assert_eq!(control.store.current_id(), 0);
    }

    #[test]
    fn plan289_collision_and_startup_protection() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(2);
        let mut startup = ServiceTunnelSet::new();
        startup.tunnels.push(
            build_control_spec(&ControlDefinition {
                name: "owned".to_owned(),
                tunnel_type: TunnelType::Client,
                options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), ports[0]),
                start_on_load: false,
            })
            .expect("startup spec"),
        );
        let store = ControlStore::open(directory.path()).expect("open");
        let control = TunnelControlState::new(store, startup, test_manager(directory.path()));
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // Control name collides with a startup-owned name.
        let error = block_on(control.create(&create_request(
            "owned",
            TunnelType::Client,
            client_options(&destination, 0),
        )))
        .expect_err("collision fails");
        assert_eq!(error, ControlError::NameCollision("owned".to_owned()));
        // Startup-owned definitions reject every mutation.
        for action in [
            TunnelAction::Edit,
            TunnelAction::Delete,
            TunnelAction::Start,
            TunnelAction::Stop,
        ] {
            let mut request = named_request(action, "owned");
            if action == TunnelAction::Edit {
                request
                    .options
                    .insert("listen_port".to_owned(), "1".to_owned());
            }
            let error = block_on(control.dispatch(&request)).expect_err("mutation rejected");
            assert_eq!(
                error,
                ControlError::StartupMutationRejected("owned".to_owned())
            );
        }
        // Duplicate control names collide.
        block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("first create");
        let error = block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect_err("duplicate fails");
        assert_eq!(error, ControlError::NameCollision("alpha".to_owned()));
        // Startup summaries stay inspectable.
        let response = control.get(Some("owned")).expect("startup get");
        assert_eq!(response["provenance"], serde_json::json!("startup"));
    }

    #[test]
    fn plan289_edit_rename_and_options() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(3);
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[0]),
        )))
        .expect("create");
        // Edit options: max_connections takes real effect on the spec.
        let mut request = named_request(TunnelAction::Edit, "alpha");
        request
            .options
            .insert("max_streams".to_owned(), "32".to_owned());
        block_on(control.dispatch(&request)).expect("edit");
        let response = control.get(Some("alpha")).expect("get");
        assert_eq!(response["options"]["max_streams"], serde_json::json!("32"));
        // Rename moves the definition; the old name is unknown.
        let mut request = named_request(TunnelAction::Edit, "alpha");
        request.new_name = Some("beta".to_owned());
        block_on(control.dispatch(&request)).expect("rename");
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
        let response = control.get(Some("beta")).expect("renamed get");
        assert_eq!(response["options"]["max_streams"], serde_json::json!("32"));
        // Rename onto an existing name collides.
        block_on(control.create(&create_request(
            "gamma",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("second create");
        let mut request = named_request(TunnelAction::Edit, "beta");
        request.new_name = Some("gamma".to_owned());
        let error = block_on(control.dispatch(&request)).expect_err("rename collision");
        assert_eq!(error, ControlError::NameCollision("gamma".to_owned()));
    }

    #[test]
    fn plan289_start_on_load_vs_running_truthfulness() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(2);
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // start_on_load=false: committed but never running.
        let mut options = client_options(&destination, ports[0]);
        options.insert("start_on_load".to_owned(), "false".to_owned());
        block_on(control.create(&create_request("lazy", TunnelType::Client, options)))
            .expect("create");
        // A fresh coordinator over the same store restarts: lazy stays stopped.
        let fresh = test_control(directory.path());
        let (scope, parent) = test_scope();
        let failures = block_on(fresh.startup(&scope, &parent));
        assert!(failures.is_empty(), "startup clean: {failures:?}");
        let response = fresh.get(Some("lazy")).expect("get");
        assert_eq!(response["start_on_load"], serde_json::json!(false));
        assert_eq!(response["running"], serde_json::json!(false));
        assert_eq!(response["status"], serde_json::json!("stopped"));
        // start_on_load=true: running immediately and across restarts.
        block_on(control.create(&create_request(
            "eager",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("create eager");
        // Release the first coordinator's listeners before the fresh
        // coordinator binds the same sockets (a real restart frees
        // them when the old process exits).
        drop(control);
        let fresh = test_control(directory.path());
        let failures = block_on(fresh.startup(&scope, &parent));
        assert!(failures.is_empty(), "startup clean: {failures:?}");
        let response = fresh.get(Some("eager")).expect("get");
        assert_eq!(response["start_on_load"], serde_json::json!(true));
        assert_eq!(response["running"], serde_json::json!(true));
        assert_eq!(response["status"], serde_json::json!("running"));
        drop(parent);
        drop(scope);
    }

    #[test]
    fn plan289_server_identity_stable_across_restart() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let target = std::net::TcpListener::bind("127.0.0.1:0").expect("target binds");
        let target_address = target.local_addr().expect("target addr");
        let options = server_options(&target_address.to_string());
        block_on(control.create(&create_request("srv", TunnelType::Server, options)))
            .expect("create");
        let before = control.get(Some("srv")).expect("get");
        let before_b64 = before["destination"]
            .as_str()
            .expect("server b64")
            .to_owned();
        assert!(!before_b64.is_empty());
        // Restart preserves the persistent server identity.
        block_on(control.dispatch(&named_request(TunnelAction::Restart, "srv"))).expect("restart");
        let after = control.get(Some("srv")).expect("get after");
        assert_eq!(after["destination"].as_str().expect("b64"), before_b64);
        assert_eq!(after["status"], serde_json::json!("running"));
        // A fresh coordinator over the same store recovers the same identity.
        let fresh = test_control(directory.path());
        let (scope, parent) = test_scope();
        block_on(fresh.startup(&scope, &parent));
        let recovered = fresh.get(Some("srv")).expect("recovered get");
        assert_eq!(recovered["destination"].as_str().expect("b64"), before_b64);
        drop(parent);
        drop(scope);
        drop(target);
    }

    #[test]
    fn plan289_publish_failure_reconciles_back() {
        let directory = tempfile::tempdir().expect("tempdir");
        // Sabotage publication: a directory at the pointer path makes
        // the atomic rename fail deterministically.
        let store = ControlStore::open(directory.path()).expect("open");
        std::fs::create_dir(store.pointer_path()).expect("pointer dir");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            test_manager(directory.path()),
        );
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let error = block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, 0),
        )))
        .expect_err("publish fails");
        match error {
            ControlError::PublishFailed {
                reconciled_back, ..
            } => {
                assert!(reconciled_back, "runtime reconciled back");
            }
            other => panic!("expected PublishFailed, got {other:?}"),
        }
        // Mirror rolled back: unknown, unpublished, no runtime.
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
        assert_eq!(control.store.current_id(), 0);
    }

    #[test]
    fn plan289_concurrent_same_name_serializes_deterministically() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = std::sync::Arc::new(test_control(directory.path()));
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let control = std::sync::Arc::clone(&control);
                let destination = destination.clone();
                std::thread::spawn(move || {
                    block_on(control.create(&create_request(
                        "race",
                        TunnelType::Client,
                        client_options(&destination, 0),
                    )))
                    .is_ok()
                })
            })
            .collect();
        let mut successes = 0;
        for handle in handles {
            if handle.join().expect("join") {
                successes += 1;
            }
        }
        assert_eq!(successes, 1, "exactly one create wins");
        assert!(control.get(Some("race")).is_ok());
    }

    #[test]
    fn plan289_concurrent_different_names_stay_bounded_and_consistent() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = std::sync::Arc::new(test_control(directory.path()));
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let ports = distinct_ports(8);
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let control = std::sync::Arc::clone(&control);
                let destination = destination.clone();
                let port = ports[n as usize];
                std::thread::spawn(move || {
                    block_on(control.create(&create_request(
                        &format!("tunnel-{n}"),
                        TunnelType::Client,
                        client_options(&destination, port),
                    )))
                })
            })
            .collect();
        let mut generations = Vec::new();
        for handle in handles {
            let response = handle.join().expect("join").expect("create succeeds");
            generations.push(response["generation"].as_u64().expect("generation"));
        }
        generations.sort_unstable();
        // One publication per mutation: strictly increasing generations.
        for window in generations.windows(2) {
            assert!(
                window[0] < window[1],
                "generations increase: {generations:?}"
            );
        }
        let inventory = control.get(None).expect("inventory");
        assert_eq!(inventory["control"].as_object().expect("map").len(), 8);
    }

    #[test]
    fn plan289_failed_start_reports_failed_status() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        // Occupy a port: staging the listener bind fails deterministically.
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("occupy");
        let port = occupied.local_addr().expect("addr").port();
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let error = block_on(control.create(&create_request(
            "blocked",
            TunnelType::Client,
            client_options(&destination, port),
        )))
        .expect_err("bind fails");
        assert!(
            matches!(error, ControlError::Manager(_)),
            "manager error: {error:?}"
        );
        // Failed staging leaves no durable or runtime trace.
        assert_eq!(
            control.get(Some("blocked")),
            Err(ControlError::UnknownTunnel("blocked".to_owned()))
        );
        assert_eq!(control.store.current_id(), 0);
        drop(occupied);
    }

    #[test]
    fn plan289_get_inventory_lists_both_classes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(2);
        let mut startup = ServiceTunnelSet::new();
        startup.tunnels.push(
            build_control_spec(&ControlDefinition {
                name: "owned".to_owned(),
                tunnel_type: TunnelType::Client,
                options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), ports[0]),
                start_on_load: false,
            })
            .expect("startup spec"),
        );
        let store = ControlStore::open(directory.path()).expect("open");
        let control = TunnelControlState::new(store, startup, test_manager(directory.path()));
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("create");
        let inventory = control.get(None).expect("inventory");
        assert_eq!(
            inventory["startup"]["owned"]["provenance"],
            serde_json::json!("startup")
        );
        assert_eq!(
            inventory["control"]["alpha"]["provenance"],
            serde_json::json!("control")
        );
        assert_eq!(
            control.get(Some("missing")),
            Err(ControlError::UnknownTunnel("missing".to_owned()))
        );
    }
}
