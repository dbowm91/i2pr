//! Persistent local administrator policy for managed applications.
//!
//! This crate stores administrator decisions and turns them into validated
//! launch decisions, but cannot construct `i2pr-appd::LaunchAuthority`. Every
//! selected package is reverified before a decision is returned.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use i2pr_app_package::{InstalledPackage, PackageError, PackageIdentity, PackageStore};
use i2pr_app_proto::{AppId, Capability, LaunchProfile, MAX_CAPABILITIES};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PUBLISHERS: usize = 4096;
const MAX_APPS: usize = 16384;
const MAX_GENERATIONS: usize = 2;
const MAX_CONNECTIONS: u32 = 128;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum StateError {
    #[error("managed-app state I/O failed")]
    Io(#[from] io::Error),
    #[error("managed-app policy is malformed")]
    InvalidState,
    #[error("managed-app policy has an unsupported schema")]
    UnsupportedSchema,
    #[error("managed-app policy generation is exhausted")]
    GenerationExhausted,
    #[error("managed-app policy is busy because the runtime is active")]
    RuntimeBusy,
    #[error("package verification failed: {0}")]
    Package(#[from] PackageError),
    #[error("publisher is not trusted")]
    Untrusted,
    #[error("application has no selected package")]
    NoSelection,
    #[error("requested capability is not grantable or was not requested")]
    CapabilityNotRequested,
    #[error("selected package is invalid or unavailable")]
    SelectedPackageUnavailable,
    #[error("Secured launch profile is unavailable")]
    SecuredUnavailable,
    #[error("UnsafeDirect requires explicit administrator acknowledgement")]
    UnsafeDirectAcknowledgementRequired,
    #[error("package is selected by persistent policy")]
    PackageSelected,
    #[error("target platform is not supported by the package")]
    UnsupportedTarget,
    #[error("package requests an unsupported resource")]
    UnsupportedResource,
    #[error("application policy does not exist")]
    AppPolicyNotFound,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCeilings {
    pub memory_bytes: Option<u64>,
    pub open_files: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppPolicy {
    pub publisher_id: String,
    pub app_id: AppId,
    pub selected: Option<PackageIdentity>,
    pub granted_capabilities: Vec<Capability>,
    pub launch_profile: Option<LaunchProfile>,
    pub autostart: bool,
    pub max_connections: u32,
    pub resource_ceilings: ResourceCeilings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyState {
    pub schema_version: u16,
    pub generation: u64,
    pub trusted_publishers: Vec<String>,
    pub apps: Vec<AppPolicy>,
}

impl Default for PolicyState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            generation: 0,
            trusted_publishers: Vec::new(),
            apps: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LaunchDecision {
    pub generation: u64,
    pub package: InstalledPackage,
    pub capabilities: Vec<Capability>,
    pub launch_profile: LaunchProfile,
    pub max_connections: u32,
    pub memory_bytes: u64,
    pub open_files: u32,
    pub target: String,
    pub entrypoint: String,
    /// Manager-owned persistent writable storage for this trusted app identity.
    pub data_root: PathBuf,
}

pub struct AppStateStore {
    root: PathBuf,
    package_store: PackageStore,
}

impl AppStateStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StateError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        private_dir(&root)?;
        let package_store = PackageStore::open(&root)?;
        fs::create_dir_all(root.join("policy/generations"))?;
        fs::create_dir_all(root.join("policy/.staging"))?;
        private_dir(&root.join("policy"))?;
        private_dir(&root.join("policy/generations"))?;
        private_dir(&root.join("policy/.staging"))?;
        Ok(Self {
            root,
            package_store,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn packages(&self) -> &PackageStore {
        &self.package_store
    }

    pub fn load(&self) -> Result<PolicyState, StateError> {
        load_state(&self.root)
    }

    /// Acquire the lifetime runtime lock. Appd holds this guard until process
    /// teardown; administrator mutations use the nonblocking counterpart.
    pub fn lock_runtime(&self) -> Result<RuntimeLock, StateError> {
        let file = open_lock(&self.root.join("runtime.lock"))?;
        file.lock()?;
        Ok(RuntimeLock { _file: file })
    }

    pub fn try_lock_runtime(&self) -> Result<RuntimeLock, StateError> {
        let file = open_lock(&self.root.join("runtime.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(RuntimeLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(StateError::RuntimeBusy),
            Err(std::fs::TryLockError::Error(error)) => Err(StateError::Io(error)),
        }
    }

    /// Apply one persistent policy mutation while appd is stopped. The runtime
    /// lock is acquired first and nonblocking, then the shared package/admin
    /// transaction lock, so mutation cannot race launch selection.
    pub fn mutate<T>(
        &self,
        operation: impl FnOnce(&mut PolicyState) -> Result<T, StateError>,
    ) -> Result<T, StateError> {
        let _runtime = self.try_lock_runtime()?;
        let admin = open_lock(&self.root.join("admin.lock"))?;
        admin.lock()?;
        cleanup_policy_staging(&self.root)?;
        let mut state = load_state(&self.root)?;
        let value = operation(&mut state)?;
        validate_state(&state)?;
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(StateError::GenerationExhausted)?;
        state.generation = generation;
        commit_state(&self.root, &state)?;
        drop(admin);
        Ok(value)
    }

    pub fn install_package(&self, source: &Path) -> Result<InstalledPackage, StateError> {
        let _runtime = self.try_lock_runtime()?;
        Ok(self.package_store.install(source)?)
    }

    pub fn remove_package(&self, identity: &PackageIdentity) -> Result<(), StateError> {
        let _runtime = self.try_lock_runtime()?;
        let state = self.load()?;
        if state
            .apps
            .iter()
            .any(|app| app.selected.as_ref() == Some(identity))
        {
            return Err(StateError::PackageSelected);
        }
        self.package_store
            .remove_unreferenced(identity, &BTreeSet::new())?;
        Ok(())
    }

    pub fn inspect_app(&self, publisher: &str, app_id: &AppId) -> Result<AppPolicy, StateError> {
        let state = self.load()?;
        state
            .apps
            .into_iter()
            .find(|app| app.publisher_id == publisher && &app.app_id == app_id)
            .ok_or(StateError::AppPolicyNotFound)
    }

    /// Resolve one selected application from a single immutable policy
    /// snapshot. Call once per autostart record; failures are per-app so callers
    /// can continue with sibling apps.
    pub fn resolve(
        &self,
        state: &PolicyState,
        app: &AppPolicy,
        target: &str,
    ) -> Result<LaunchDecision, StateError> {
        validate_state(state)?;
        if state.generation == 0
            || !app.autostart
            || !state.apps.iter().any(|candidate| candidate == app)
        {
            return Err(StateError::InvalidState);
        }
        let identity = app.selected.as_ref().ok_or(StateError::NoSelection)?;
        if state
            .trusted_publishers
            .binary_search(&app.publisher_id)
            .is_err()
        {
            return Err(StateError::Untrusted);
        }
        if app.publisher_id != identity.publisher_key_id || app.app_id != identity.app_id {
            return Err(StateError::SelectedPackageUnavailable);
        }
        let package_path = self.package_store.package_path(identity)?;
        let package = self
            .package_store
            .verify_installed(&package_path)
            .map_err(|_| StateError::SelectedPackageUnavailable)?;
        if package.identity != *identity {
            return Err(StateError::SelectedPackageUnavailable);
        }
        let profile = app.launch_profile.ok_or(StateError::NoSelection)?;
        let entrypoints = package
            .manifest
            .entrypoints
            .iter()
            .filter(|entry| entry.target == target)
            .collect::<Vec<_>>();
        if entrypoints.len() != 1 {
            return Err(StateError::UnsupportedTarget);
        }
        let entrypoint = entrypoints[0].path.as_str().to_owned();
        if !package
            .inventory
            .iter()
            .any(|record| record.path == entrypoints[0].path.as_str() && record.executable)
        {
            return Err(StateError::SelectedPackageUnavailable);
        }
        for request in &package.manifest.resources {
            if !matches!(request.name.as_str(), "memory_bytes" | "open_files") {
                return Err(StateError::UnsupportedResource);
            }
        }
        let requested = package
            .manifest
            .requested_capabilities
            .iter()
            .map(|r| r.capability)
            .collect::<BTreeSet<_>>();
        if app.granted_capabilities.iter().any(|cap| {
            !requested.contains(cap) || !matches!(cap, Capability::Sam | Capability::I2cp)
        }) {
            return Err(StateError::CapabilityNotRequested);
        }
        let request = |name: &str| {
            package
                .manifest
                .resources
                .iter()
                .find(|r| r.name == name)
                .map(|r| r.requested)
                .unwrap_or(0)
        };
        let memory_bytes =
            request("memory_bytes").min(app.resource_ceilings.memory_bytes.unwrap_or(0));
        let open_files = request("open_files")
            .min(app.resource_ceilings.open_files.unwrap_or(0))
            .min(u32::MAX as u64) as u32;
        if profile == LaunchProfile::Secured && (memory_bytes == 0 || open_files == 0) {
            return Err(StateError::UnsupportedResource);
        }
        let data_root = self.application_data_root(&app.publisher_id, &app.app_id)?;
        Ok(LaunchDecision {
            generation: state.generation,
            package,
            capabilities: app.granted_capabilities.clone(),
            launch_profile: profile,
            max_connections: app.max_connections.min(MAX_CONNECTIONS),
            memory_bytes,
            open_files,
            target: target.to_owned(),
            entrypoint,
            data_root,
        })
    }

    /// Return the stable private data directory for one trusted publisher/app
    /// pair. The package version and launch instance are intentionally absent.
    pub fn application_data_root(
        &self,
        publisher_id: &str,
        app_id: &AppId,
    ) -> Result<PathBuf, StateError> {
        if !is_digest(publisher_id) {
            return Err(StateError::InvalidState);
        }
        let base = self.root.join("app-data");
        fs::create_dir_all(&base)?;
        private_dir(&base)?;
        let publisher = base.join(publisher_id);
        fs::create_dir_all(&publisher)?;
        private_dir(&publisher)?;
        let app = publisher.join(app_id.as_str());
        match fs::symlink_metadata(&app) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(StateError::InvalidState);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&app)?;
            }
            Err(error) => return Err(StateError::Io(error)),
        }
        private_dir(&app)?;
        let canonical_root = self.root.canonicalize()?;
        let canonical_app = app.canonicalize()?;
        if !canonical_app.starts_with(canonical_root.join("app-data")) {
            return Err(StateError::InvalidState);
        }
        Ok(canonical_app)
    }
}

pub struct RuntimeLock {
    _file: File,
}

pub fn validate_state(state: &PolicyState) -> Result<(), StateError> {
    if state.schema_version != 1 {
        return Err(StateError::UnsupportedSchema);
    }
    if state.trusted_publishers.len() > MAX_PUBLISHERS || state.apps.len() > MAX_APPS {
        return Err(StateError::InvalidState);
    }
    let mut previous: Option<&str> = None;
    for publisher in &state.trusted_publishers {
        if !is_digest(publisher) || previous.is_some_and(|p| p >= publisher.as_str()) {
            return Err(StateError::InvalidState);
        }
        previous = Some(publisher);
    }
    let mut prior_key: Option<(&str, &str)> = None;
    for app in &state.apps {
        if !is_digest(&app.publisher_id)
            || app.max_connections == 0
            || app.max_connections > MAX_CONNECTIONS
            || app.granted_capabilities.len() > MAX_CAPABILITIES
        {
            return Err(StateError::InvalidState);
        }
        let key = (app.publisher_id.as_str(), app.app_id.as_str());
        if prior_key.is_some_and(|p| p >= key) {
            return Err(StateError::InvalidState);
        }
        prior_key = Some(key);
        if app.granted_capabilities.windows(2).any(|w| w[0] >= w[1])
            || app
                .granted_capabilities
                .iter()
                .any(|c| !matches!(c, Capability::Sam | Capability::I2cp))
        {
            return Err(StateError::InvalidState);
        }
        if let Some(selected) = &app.selected
            && (selected.publisher_key_id != app.publisher_id || selected.app_id != app.app_id)
        {
            return Err(StateError::InvalidState);
        }
        if app.autostart
            && (app.selected.is_none()
                || app.launch_profile.is_none()
                || state
                    .trusted_publishers
                    .binary_search(&app.publisher_id)
                    .is_err())
        {
            return Err(StateError::InvalidState);
        }
        if app
            .resource_ceilings
            .memory_bytes
            .is_some_and(|v| v > i2pr_app_proto::MAX_RESOURCE_REQUEST)
            || app
                .resource_ceilings
                .open_files
                .is_some_and(|v| v > i2pr_app_proto::MAX_RESOURCE_REQUEST)
        {
            return Err(StateError::InvalidState);
        }
    }
    Ok(())
}

pub fn target_triple() -> Option<&'static str> {
    #[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
    {
        return Some("x86_64-unknown-linux-gnu");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "gnu"))]
    {
        return Some("aarch64-unknown-linux-gnu");
    }
    #[cfg(all(target_arch = "arm", target_os = "linux", target_abi = "eabihf"))]
    {
        return Some("armv7-unknown-linux-gnueabihf");
    }
    #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
    {
        return Some("x86_64-apple-darwin");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    {
        return Some("aarch64-apple-darwin");
    }
    #[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
    {
        return Some("x86_64-pc-windows-msvc");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "windows", target_env = "msvc"))]
    {
        return Some("aarch64-pc-windows-msvc");
    }
    #[allow(unreachable_code)]
    None
}

fn load_state(root: &Path) -> Result<PolicyState, StateError> {
    let generations = root.join("policy/generations");
    let mut committed = Vec::new();
    for entry in fs::read_dir(&generations)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if !kind.is_dir() || kind.is_symlink() {
            return Err(StateError::InvalidState);
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| StateError::InvalidState)?;
        if name.len() != 20 || !name.bytes().all(|b| b.is_ascii_digit()) {
            return Err(StateError::InvalidState);
        }
        let generation: u64 = name.parse().map_err(|_| StateError::InvalidState)?;
        committed.push((generation, entry.path()));
    }
    committed.sort_by_key(|(n, _)| *n);
    let Some((generation, dir)) = committed.last() else {
        return Ok(PolicyState::default());
    };
    if *generation == 0 {
        return Err(StateError::InvalidState);
    }
    let path = dir.join("state.json");
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_STATE_BYTES {
        return Err(StateError::InvalidState);
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(path)?
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(StateError::InvalidState);
    }
    let state: PolicyState =
        serde_json::from_slice(&bytes).map_err(|_| StateError::InvalidState)?;
    validate_state(&state)?;
    if state.generation != *generation {
        return Err(StateError::InvalidState);
    }
    Ok(state)
}

fn commit_state(root: &Path, state: &PolicyState) -> Result<(), StateError> {
    let bytes = serde_json::to_vec(state).map_err(|_| StateError::InvalidState)?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(StateError::InvalidState);
    }
    let staging = root.join("policy/.staging");
    let stage = staging.join(format!(
        "{}-{}-{}",
        std::process::id(),
        state.generation,
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&stage)?;
    private_dir(&stage)?;
    let state_file = create_private_file(&stage.join("state.json"))?;
    let mut state_file = state_file;
    state_file.write_all(&bytes)?;
    state_file.sync_all()?;
    sync_dir(&stage)?;
    let final_dir = root
        .join("policy/generations")
        .join(format!("{:020}", state.generation));
    fs::rename(&stage, &final_dir)?;
    sync_dir(&final_dir)?;
    sync_dir(&root.join("policy/generations"))?;
    prune_generations(root)?;
    Ok(())
}

fn prune_generations(root: &Path) -> Result<(), StateError> {
    let path = root.join("policy/generations");
    let mut dirs = fs::read_dir(&path)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    dirs.sort();
    let remove_count = dirs.len().saturating_sub(MAX_GENERATIONS);
    for dir in dirs.into_iter().take(remove_count) {
        fs::remove_dir_all(dir)?;
    }
    sync_dir(&path)?;
    Ok(())
}

fn cleanup_policy_staging(root: &Path) -> Result<(), StateError> {
    let staging = root.join("policy/.staging");
    for entry in fs::read_dir(&staging)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() && !kind.is_symlink() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    sync_dir(&staging)?;
    Ok(())
}

fn open_lock(path: &Path) -> Result<File, StateError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn create_private_file(path: &Path) -> Result<File, StateError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn private_dir(path: &Path) -> Result<(), StateError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn sync_dir(path: &Path) -> Result<(), StateError> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn publisher(c: char) -> String {
        std::iter::repeat_n(c, 64).collect()
    }

    fn signed_package(path: &Path, version: &str, requested: &[Capability]) {
        let signing = SigningKey::from_bytes(&[9; 32]);
        let key = signing.verifying_key().to_bytes();
        let fingerprint = sha256_hex(&key);
        let manifest = serde_json::json!({
            "schema_version": 1,
            "app_id": "sample.app",
            "publisher_id": fingerprint,
            "version": version,
            "name": "sample",
            "description": "",
            "host_protocol_min": {"major": 1, "minor": 0},
            "host_protocol_max": {"major": 1, "minor": 0},
            "entrypoints": [{"target": target_triple().unwrap(), "path": "bin/app"}],
            "requested_capabilities": requested.iter().map(|c| serde_json::to_value(i2pr_app_proto::RequestedCapability { capability: *c }).unwrap()).collect::<Vec<_>>(),
            "resources": [
                {"name": "memory_bytes", "requested": 4096},
                {"name": "open_files", "requested": 100}
            ],
            "ui": null,
            "autostart_requested": true,
            "restart_requested": true
        });
        let manifest = serde_json::to_vec(&manifest).unwrap();
        let inventory = serde_json::json!([{
            "path": "bin/app",
            "size": 4,
            "sha256": sha256_hex(b"app!"),
            "executable": true
        }]);
        let inventory = serde_json::to_vec(&inventory).unwrap();
        let mut transcript = b"I2PR-APP-PACKAGE-V1\0".to_vec();
        transcript.extend_from_slice(&(manifest.len() as u32).to_be_bytes());
        transcript.extend_from_slice(&manifest);
        transcript.extend_from_slice(&(inventory.len() as u32).to_be_bytes());
        transcript.extend_from_slice(&inventory);
        transcript.extend_from_slice(&key);
        let signature = signing.sign(&transcript).to_bytes();
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("manifest.json", manifest.as_slice()),
            ("inventory.json", inventory.as_slice()),
            ("publisher.ed25519", key.as_slice()),
            ("signature.ed25519", signature.as_slice()),
            ("payload/bin/app", b"app!".as_slice()),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn first_mutation_commits_generation_and_retains_two() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path()).unwrap();
        for ch in ['a', 'b', 'c'] {
            store
                .mutate(|state| {
                    state.trusted_publishers.push(publisher(ch));
                    state.trusted_publishers.sort();
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(store.load().unwrap().generation, 3);
        assert_eq!(
            fs::read_dir(tmp.path().join("policy/generations"))
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    fn application_data_root_is_persistent_private_and_identity_scoped() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path().join("state")).unwrap();
        let app = AppId::parse("same-app").unwrap();
        let publisher_a = publisher('a');
        let publisher_b = publisher('b');
        let a = store.application_data_root(&publisher_a, &app).unwrap();
        let b = store.application_data_root(&publisher_b, &app).unwrap();
        assert_ne!(a, b);
        std::fs::write(a.join("persistent.db"), b"state").unwrap();
        drop(store);
        let reopened = AppStateStore::open(tmp.path().join("state")).unwrap();
        let after_restart = reopened.application_data_root(&publisher_a, &app).unwrap();
        assert_eq!(a, after_restart);
        assert_eq!(
            std::fs::read(after_restart.join("persistent.db")).unwrap(),
            b"state"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&after_restart)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn application_data_root_rejects_a_symlink_leaf() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path().join("state")).unwrap();
        let app = AppId::parse("linked-app").unwrap();
        let parent = store.root().join("app-data").join(publisher('c'));
        std::fs::create_dir_all(&parent).unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, parent.join("linked-app")).unwrap();
        assert!(matches!(
            store.application_data_root(&publisher('c'), &app),
            Err(StateError::InvalidState)
        ));
    }

    #[test]
    fn malformed_highest_generation_does_not_roll_back() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path()).unwrap();
        store.mutate(|_| Ok(())).unwrap();
        store.mutate(|_| Ok(())).unwrap();
        fs::write(
            tmp.path()
                .join("policy/generations/00000000000000000002/state.json"),
            b"{}",
        )
        .unwrap();
        assert!(matches!(store.load(), Err(StateError::InvalidState)));
    }

    #[test]
    fn staging_is_ignored_and_schema_is_strict() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path()).unwrap();
        fs::create_dir_all(tmp.path().join("policy/.staging/crash-cut")).unwrap();
        fs::write(
            tmp.path().join("policy/.staging/crash-cut/state.json"),
            b"not committed",
        )
        .unwrap();
        assert_eq!(store.load().unwrap(), PolicyState::default());

        let generation = tmp.path().join("policy/generations/00000000000000000001");
        fs::create_dir(&generation).unwrap();
        fs::write(
            generation.join("state.json"),
            br#"{"schema_version":1,"generation":1,"trusted_publishers":[],"apps":[],"future":true}"#,
        )
        .unwrap();
        assert!(matches!(store.load(), Err(StateError::InvalidState)));
        fs::write(
            generation.join("state.json"),
            br#"{"schema_version":1,"schema_version":1,"generation":1,"trusted_publishers":[],"apps":[]}"#,
        )
        .unwrap();
        assert!(matches!(store.load(), Err(StateError::InvalidState)));
    }

    #[test]
    fn generation_exhaustion_is_typed_and_does_not_write() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path()).unwrap();
        let generation = tmp.path().join("policy/generations/18446744073709551615");
        fs::create_dir(&generation).unwrap();
        let state = PolicyState {
            generation: u64::MAX,
            ..PolicyState::default()
        };
        fs::write(
            generation.join("state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.mutate(|_| Ok(())),
            Err(StateError::GenerationExhausted)
        ));
        assert_eq!(store.load().unwrap().generation, u64::MAX);
    }

    #[test]
    fn policy_requires_canonical_sorted_trust_and_grants() {
        let state = PolicyState {
            trusted_publishers: vec![publisher('b'), publisher('a')],
            ..PolicyState::default()
        };
        assert!(validate_state(&state).is_err());
    }

    #[test]
    fn runtime_lock_excludes_offline_policy_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path()).unwrap();
        let _lock = store.lock_runtime().unwrap();
        assert!(matches!(
            store.mutate(|_| Ok(())),
            Err(StateError::RuntimeBusy)
        ));
    }

    #[test]
    fn launch_resolution_keeps_trust_selection_and_grants_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path().join("state")).unwrap();
        let v1 = tmp.path().join("v1.i2prapp");
        let v2 = tmp.path().join("v2.i2prapp");
        signed_package(&v1, "1.0", &[Capability::Sam]);
        signed_package(&v2, "2.0", &[Capability::Sam, Capability::I2cp]);
        let p1 = store.packages().install(&v1).unwrap();
        let p2 = store.packages().install(&v2).unwrap();
        let fingerprint = p1.identity.publisher_key_id.clone();
        let app_id = p1.identity.app_id.clone();

        store
            .mutate(|state| {
                state.apps.push(AppPolicy {
                    publisher_id: fingerprint.clone(),
                    app_id: app_id.clone(),
                    selected: Some(p1.identity.clone()),
                    granted_capabilities: Vec::new(),
                    launch_profile: Some(LaunchProfile::UnsafeDirect),
                    autostart: false,
                    max_connections: 4,
                    resource_ceilings: ResourceCeilings {
                        memory_bytes: Some(2048),
                        open_files: Some(32),
                    },
                });
                Ok(())
            })
            .unwrap();
        let state = store.load().unwrap();
        assert!(matches!(
            store.resolve(&state, &state.apps[0], target_triple().unwrap()),
            Err(StateError::InvalidState)
        ));

        store
            .mutate(|state| {
                state.trusted_publishers.push(fingerprint.clone());
                state.apps[0].autostart = true;
                Ok(())
            })
            .unwrap();
        let state = store.load().unwrap();
        let decision = store
            .resolve(&state, &state.apps[0], target_triple().unwrap())
            .unwrap();
        assert!(decision.capabilities.is_empty());
        assert_eq!(decision.memory_bytes, 2048);
        assert_eq!(decision.open_files, 32);

        store
            .mutate(|state| {
                state.apps[0].granted_capabilities = vec![Capability::Sam];
                state.apps[0].selected = Some(p2.identity.clone());
                Ok(())
            })
            .unwrap();
        let state = store.load().unwrap();
        let decision = store
            .resolve(&state, &state.apps[0], target_triple().unwrap())
            .unwrap();
        assert_eq!(decision.capabilities, [Capability::Sam]);
        assert_eq!(decision.package.identity, p2.identity);

        store
            .mutate(|state| {
                state.apps[0].launch_profile = Some(LaunchProfile::Secured);
                Ok(())
            })
            .unwrap();
        let state = store.load().unwrap();
        let secured = store
            .resolve(&state, &state.apps[0], target_triple().unwrap())
            .unwrap();
        assert_eq!(secured.launch_profile, LaunchProfile::Secured);
        assert!(secured.data_root.is_dir());
        assert_eq!(
            secured.data_root,
            store.application_data_root(&fingerprint, &app_id).unwrap(),
            "data identity is stable when the package version changes"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let payload = p2.path.join("payload/bin/app");
            fs::set_permissions(&payload, fs::Permissions::from_mode(0o600)).unwrap();
            fs::write(&payload, b"evil").unwrap();
            let state = store.load().unwrap();
            assert!(matches!(
                store.resolve(&state, &state.apps[0], target_triple().unwrap()),
                Err(StateError::SelectedPackageUnavailable)
            ));
        }
    }
}
