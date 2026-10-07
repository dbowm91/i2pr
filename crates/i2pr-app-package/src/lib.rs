//! Signed immutable managed-application packages and their local content store.
//!
//! A valid package proves possession of a publisher key and binds payload bytes
//! to that key. It does not make the publisher trusted and it grants no launch
//! authority. The crate has filesystem access only to an explicitly supplied
//! store root; it owns no network or process capability.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use ed25519_dalek::{Signature, VerifyingKey};
use i2pr_app_proto::{AppId, AppVersion, MAX_MANIFEST_BYTES, Manifest, PackagePath};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::{CompressionMethod, ZipArchive};

pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PAYLOAD_FILES: usize = 4096;
pub const MAX_PAYLOAD_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_TOTAL_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_INVENTORY_BYTES: usize = 1024 * 1024;
const SIGNATURE_PREFIX: &[u8] = b"I2PR-APP-PACKAGE-V1\0";
const REQUIRED_METADATA: [&str; 4] = [
    "manifest.json",
    "inventory.json",
    "publisher.ed25519",
    "signature.ed25519",
];

static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("I/O operation failed")]
    Io(#[from] io::Error),
    #[error("package archive is invalid")]
    InvalidArchive,
    #[error("package limit exceeded: {0}")]
    Limit(&'static str),
    #[error("package metadata is invalid")]
    InvalidMetadata,
    #[error("publisher signature is invalid")]
    InvalidSignature,
    #[error("package identity conflicts with an installed artifact")]
    VersionConflict,
    #[error("package does not exist")]
    NotFound,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub publisher_key_id: String,
    pub app_id: AppId,
    pub version: AppVersion,
    pub artifact_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryRecord {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Clone, Debug)]
pub struct VerifiedPackage {
    pub identity: PackageIdentity,
    pub manifest: Manifest,
    pub inventory: Vec<InventoryRecord>,
    pub publisher_key: [u8; 32],
    pub artifact_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u16,
    publisher_key_id: String,
    app_id: String,
    version: String,
    artifact_sha256: String,
    generation: u64,
}

#[derive(Clone, Debug)]
pub struct InstalledPackage {
    pub identity: PackageIdentity,
    pub manifest: Manifest,
    pub inventory: Vec<InventoryRecord>,
    pub path: PathBuf,
}

/// Verify a `.i2prapp` file without writing or executing anything.
pub fn verify_file(path: &Path) -> Result<VerifiedPackage, PackageError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(PackageError::Limit("archive bytes"));
    }
    let stage_dir = std::env::temp_dir().join(format!("i2pr-verify-{}", next_nonce()));
    fs::create_dir(&stage_dir)?;
    private_dir(&stage_dir)?;
    let _cleanup = TemporaryStage(stage_dir.clone());
    let stage_file = stage_dir.join("package.i2prapp");
    let artifact = hex(&copy_bounded(path, &stage_file)?);
    let mut archive = open_archive(&stage_file)?;
    verify_archive(&mut archive, Some(artifact))
}

/// Local content store. Every path returned from this API is derived from
/// validated identifiers and digest strings, never archive path spelling.
pub struct PackageStore {
    root: PathBuf,
}

impl PackageStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, PackageError> {
        let root = root.into();
        fs::create_dir_all(root.join("packages"))?;
        fs::create_dir_all(root.join(".staging"))?;
        private_dir(&root)?;
        private_dir(&root.join("packages"))?;
        private_dir(&root.join(".staging"))?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("admin.lock"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Copy the mutable input once, then verify and materialize only that
    /// manager-owned staged copy. Commit is a single directory rename.
    pub fn install(&self, source: &Path) -> Result<InstalledPackage, PackageError> {
        let lock = open_lock(&self.root.join("admin.lock"))?;
        lock.lock()?;
        self.install_locked(source)
    }

    fn install_locked(&self, source: &Path) -> Result<InstalledPackage, PackageError> {
        let input_meta = fs::metadata(source)?;
        if !input_meta.is_file() || input_meta.len() > MAX_ARCHIVE_BYTES {
            return Err(PackageError::Limit("archive bytes"));
        }
        let nonce = next_nonce();
        let stage_file = self.root.join(".staging").join(format!("{nonce}.i2prapp"));
        let stage_dir = self.root.join(".staging").join(format!("{nonce}.dir"));
        let copy_result = match copy_bounded(source, &stage_file) {
            Ok(digest) => digest,
            Err(error) => {
                let _ = fs::remove_file(&stage_file);
                return Err(error);
            }
        };
        let artifact = hex(&copy_result);
        let result = (|| {
            let mut archive = open_archive(&stage_file)?;
            let verified = verify_archive(&mut archive, Some(artifact.clone()))?;
            fs::create_dir(&stage_dir)?;
            private_dir(&stage_dir)?;
            materialize(&mut archive, &verified.inventory, &stage_dir)?;
            write_metadata(&stage_dir, &mut archive)?;
            let receipt = Receipt {
                schema_version: 1,
                publisher_key_id: verified.identity.publisher_key_id.clone(),
                app_id: verified.identity.app_id.as_str().to_owned(),
                version: verified.identity.version.as_str().to_owned(),
                artifact_sha256: verified.artifact_sha256.clone(),
                generation: 1,
            };
            write_private_file(
                &stage_dir.join("receipt.json"),
                &serde_json::to_vec(&receipt).map_err(|_| PackageError::InvalidMetadata)?,
            )?;
            sync_tree(&stage_dir)?;
            freeze_tree(&stage_dir)?;

            let app_version_dir = self
                .root
                .join("packages")
                .join(&verified.identity.publisher_key_id)
                .join(verified.identity.app_id.as_str())
                .join(verified.identity.version.as_str());
            fs::create_dir_all(&app_version_dir)?;
            private_dir(&app_version_dir)?;
            let final_path = app_version_dir.join(&verified.identity.artifact_sha256);
            if final_path.exists() {
                return self.verify_installed(&final_path);
            }
            if fs::read_dir(&app_version_dir)?.next().is_some() {
                return Err(PackageError::VersionConflict);
            }
            fs::rename(&stage_dir, &final_path)?;
            sync_dir(&app_version_dir)?;
            Ok(installed_from(&final_path, verified))
        })();
        let _ = fs::remove_file(stage_file);
        let _ = fs::remove_dir_all(stage_dir);
        result
    }

    pub fn list(&self) -> Result<Vec<InstalledPackage>, PackageError> {
        let mut packages = Vec::new();
        let base = self.root.join("packages");
        for publisher in safe_dirs(&base)? {
            for app in safe_dirs(&publisher)? {
                for version in safe_dirs(&app)? {
                    for artifact in safe_dirs(&version)? {
                        if let Ok(package) = self.verify_installed(&artifact) {
                            packages.push(package);
                        }
                    }
                }
            }
        }
        packages.sort_by(|a, b| a.identity.cmp(&b.identity));
        Ok(packages)
    }

    pub fn verify_installed(&self, path: &Path) -> Result<InstalledPackage, PackageError> {
        let manifest_bytes = read_bounded(&path.join("manifest.json"), MAX_MANIFEST_BYTES)?;
        let inventory_bytes = read_bounded(&path.join("inventory.json"), MAX_INVENTORY_BYTES)?;
        let key_bytes = read_bounded(&path.join("publisher.ed25519"), 32)?;
        let sig_bytes = read_bounded(&path.join("signature.ed25519"), 64)?;
        let manifest =
            Manifest::decode(&manifest_bytes).map_err(|_| PackageError::InvalidMetadata)?;
        let inventory = parse_inventory(&inventory_bytes)?;
        let publisher_key: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| PackageError::InvalidMetadata)?;
        let signature: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| PackageError::InvalidSignature)?;
        let key =
            VerifyingKey::from_bytes(&publisher_key).map_err(|_| PackageError::InvalidSignature)?;
        let publisher_key_id = hex(&Sha256::digest(publisher_key));
        if manifest.publisher_id.as_str() != publisher_key_id {
            return Err(PackageError::InvalidSignature);
        }
        let transcript = signature_transcript(&manifest_bytes, &inventory_bytes, &publisher_key)?;
        key.verify_strict(&transcript, &Signature::from_bytes(&signature))
            .map_err(|_| PackageError::InvalidSignature)?;
        validate_inventory_manifest(&manifest, &inventory)?;
        verify_materialized_payload(path, &inventory)?;
        let digest = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or(PackageError::InvalidMetadata)?
            .to_owned();
        if !is_digest(&digest) {
            return Err(PackageError::InvalidMetadata);
        }
        let receipt_bytes = read_bounded(&path.join("receipt.json"), 4096)?;
        let receipt: Receipt =
            serde_json::from_slice(&receipt_bytes).map_err(|_| PackageError::InvalidMetadata)?;
        if receipt.schema_version != 1
            || receipt.generation == 0
            || receipt.publisher_key_id != publisher_key_id
            || receipt.app_id != manifest.app_id.as_str()
            || receipt.version != manifest.version.as_str()
            || receipt.artifact_sha256 != digest
        {
            return Err(PackageError::InvalidMetadata);
        }
        let version_path = path.parent().ok_or(PackageError::InvalidMetadata)?;
        let app_path = version_path.parent().ok_or(PackageError::InvalidMetadata)?;
        let publisher_path = app_path.parent().ok_or(PackageError::InvalidMetadata)?;
        if version_path.file_name().and_then(|v| v.to_str()) != Some(manifest.version.as_str())
            || app_path.file_name().and_then(|v| v.to_str()) != Some(manifest.app_id.as_str())
            || publisher_path.file_name().and_then(|v| v.to_str())
                != Some(publisher_key_id.as_str())
        {
            return Err(PackageError::InvalidMetadata);
        }
        let identity = PackageIdentity {
            publisher_key_id,
            app_id: manifest.app_id.clone(),
            version: manifest.version.clone(),
            artifact_sha256: digest.clone(),
        };
        Ok(InstalledPackage {
            identity,
            manifest,
            inventory,
            path: path.to_owned(),
        })
    }

    pub fn remove_unreferenced(
        &self,
        identity: &PackageIdentity,
        referenced: &BTreeSet<PackageIdentity>,
    ) -> Result<(), PackageError> {
        let lock = open_lock(&self.root.join("admin.lock"))?;
        lock.lock()?;
        validate_identity(identity)?;
        if referenced.contains(identity) {
            return Err(PackageError::VersionConflict);
        }
        let path = self.package_path(identity)?;
        if !path.exists() {
            return Err(PackageError::NotFound);
        }
        make_removable(&path)?;
        fs::remove_dir_all(path)?;
        Ok(())
    }

    pub fn package_path(&self, identity: &PackageIdentity) -> Result<PathBuf, PackageError> {
        validate_identity(identity)?;
        Ok(self
            .root
            .join("packages")
            .join(&identity.publisher_key_id)
            .join(identity.app_id.as_str())
            .join(identity.version.as_str())
            .join(&identity.artifact_sha256))
    }

    /// Remove only direct children of the exact staging namespace. Symlinks are
    /// unlinked as directory entries and are never traversed.
    pub fn cleanup_staging(&self) -> Result<usize, PackageError> {
        let lock = open_lock(&self.root.join("admin.lock"))?;
        lock.lock()?;
        let staging = self.root.join(".staging");
        let mut removed = 0;
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() && !kind.is_symlink() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
            removed += 1;
        }
        Ok(removed)
    }
}

fn open_archive(path: &Path) -> Result<ZipArchive<File>, PackageError> {
    let mut file = File::open(path)?;
    validate_central_directory(&mut file)?;
    file.seek(SeekFrom::Start(0))?;
    ZipArchive::new(file).map_err(|_| PackageError::InvalidArchive)
}

/// Validate raw central-directory records before `zip` builds its name index.
/// ZIP 8.6's index hides duplicate central records with the same filename, so
/// duplicate rejection must happen at the byte layer.
fn validate_central_directory(file: &mut File) -> Result<(), PackageError> {
    const EOCD_BYTES: usize = 22;
    const MAX_ZIP_COMMENT: usize = 65_535;
    const CENTRAL_HEADER_BYTES: usize = 46;
    const MAX_ARCHIVE_NAME_BYTES: usize = 8 + 512;

    let file_len = file.metadata()?.len();
    if file_len > MAX_ARCHIVE_BYTES || file_len < EOCD_BYTES as u64 {
        return Err(PackageError::InvalidArchive);
    }
    let tail_len = file_len.min((EOCD_BYTES + MAX_ZIP_COMMENT) as u64) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0_u8; tail_len];
    file.read_exact(&mut tail)?;
    let eocd_tail_offset = tail
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .ok_or(PackageError::InvalidArchive)?;
    if eocd_tail_offset + EOCD_BYTES > tail.len() {
        return Err(PackageError::InvalidArchive);
    }
    let eocd = &tail[eocd_tail_offset..];
    let comment_len = le_u16(eocd, 20)? as usize;
    if eocd_tail_offset + EOCD_BYTES + comment_len != tail.len() {
        return Err(PackageError::InvalidArchive);
    }
    let eocd_absolute = file_len - tail_len as u64 + eocd_tail_offset as u64;
    let disk = le_u16(eocd, 4)?;
    let central_disk = le_u16(eocd, 6)?;
    let disk_entries = le_u16(eocd, 8)?;
    let total_entries = le_u16(eocd, 10)?;
    let central_size = le_u32(eocd, 12)? as u64;
    let central_offset = le_u32(eocd, 16)? as u64;
    if disk != 0
        || central_disk != 0
        || disk_entries != total_entries
        || disk_entries == u16::MAX
        || central_size == u32::MAX as u64
        || central_offset == u32::MAX as u64
        || total_entries as usize > MAX_PAYLOAD_FILES + REQUIRED_METADATA.len()
        || central_offset.checked_add(central_size) != Some(eocd_absolute)
    {
        return Err(PackageError::InvalidArchive);
    }

    let mut names = BTreeSet::new();
    let mut cursor = central_offset;
    let mut total_payload = 0_u64;
    for _ in 0..total_entries {
        if cursor
            .checked_add(CENTRAL_HEADER_BYTES as u64)
            .is_none_or(|end| end > eocd_absolute)
        {
            return Err(PackageError::InvalidArchive);
        }
        file.seek(SeekFrom::Start(cursor))?;
        let mut header = [0_u8; CENTRAL_HEADER_BYTES];
        file.read_exact(&mut header)?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(PackageError::InvalidArchive);
        }
        let flags = le_u16(&header, 8)?;
        let method = le_u16(&header, 10)?;
        let name_len = le_u16(&header, 28)? as usize;
        let extra_len = le_u16(&header, 30)? as usize;
        let record_comment_len = le_u16(&header, 32)? as usize;
        let entry_disk = le_u16(&header, 34)?;
        let compressed_size = le_u32(&header, 20)?;
        let uncompressed_size = le_u32(&header, 24)?;
        let local_offset = le_u32(&header, 42)?;
        if name_len == 0
            || name_len > MAX_ARCHIVE_NAME_BYTES
            || entry_disk != 0
            || flags & ((1 << 0) | (1 << 6) | (1 << 13)) != 0
            || method != 0
            || compressed_size == u32::MAX
            || uncompressed_size == u32::MAX
            || local_offset == u32::MAX
            || local_offset as u64 >= central_offset
        {
            return Err(PackageError::InvalidArchive);
        }
        let record_end = cursor
            .checked_add(CENTRAL_HEADER_BYTES as u64)
            .and_then(|v| v.checked_add(name_len as u64))
            .and_then(|v| v.checked_add(extra_len as u64))
            .and_then(|v| v.checked_add(record_comment_len as u64))
            .ok_or(PackageError::InvalidArchive)?;
        if record_end > eocd_absolute {
            return Err(PackageError::InvalidArchive);
        }
        let mut name = vec![0_u8; name_len];
        file.read_exact(&mut name)?;
        if !name.is_ascii() || !names.insert(name.clone()) {
            return Err(PackageError::InvalidArchive);
        }
        let mut extra = vec![0_u8; extra_len];
        file.read_exact(&mut extra)?;
        let mut extra_cursor = 0;
        while extra_cursor < extra.len() {
            if extra.len() - extra_cursor < 4 {
                return Err(PackageError::InvalidArchive);
            }
            let tag = le_u16(&extra, extra_cursor)?;
            let size = le_u16(&extra, extra_cursor + 2)? as usize;
            extra_cursor = extra_cursor
                .checked_add(4)
                .and_then(|v| v.checked_add(size))
                .ok_or(PackageError::InvalidArchive)?;
            if extra_cursor > extra.len() || matches!(tag, 0x0001 | 0x7075) {
                return Err(PackageError::InvalidArchive);
            }
        }
        if name.starts_with(b"payload/") {
            let compressed = compressed_size as u64;
            let uncompressed = uncompressed_size as u64;
            if compressed != uncompressed || uncompressed > MAX_PAYLOAD_FILE_BYTES {
                return Err(PackageError::Limit("payload file bytes"));
            }
            total_payload = total_payload
                .checked_add(uncompressed)
                .ok_or(PackageError::Limit("total payload bytes"))?;
            if total_payload > MAX_TOTAL_PAYLOAD_BYTES {
                return Err(PackageError::Limit("total payload bytes"));
            }
        }
        file.seek(SeekFrom::Start(record_end))?;
        cursor = record_end;
    }
    if cursor != eocd_absolute {
        return Err(PackageError::InvalidArchive);
    }
    Ok(())
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(PackageError::InvalidArchive)?;
    Ok(u16::from_le_bytes(
        value.try_into().expect("fixed-size slice"),
    ))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(PackageError::InvalidArchive)?;
    Ok(u32::from_le_bytes(
        value.try_into().expect("fixed-size slice"),
    ))
}

fn verify_archive<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    artifact: Option<String>,
) -> Result<VerifiedPackage, PackageError> {
    let mut names = BTreeSet::new();
    let mut entries = BTreeMap::<String, usize>::new();
    if archive.len() > MAX_PAYLOAD_FILES + REQUIRED_METADATA.len() {
        return Err(PackageError::Limit("entry count"));
    }
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| PackageError::InvalidArchive)?;
        let name = file.name().to_owned();
        if !names.insert(name.clone())
            || file.is_dir()
            || file.compression() != CompressionMethod::Stored
            || file.encrypted()
        {
            return Err(PackageError::InvalidArchive);
        }
        if let Some(mode) = file.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 {
                return Err(PackageError::InvalidArchive);
            }
        }
        if REQUIRED_METADATA.contains(&name.as_str()) {
            entries.insert(name, index);
        } else if let Some(raw) = name.strip_prefix("payload/") {
            let path = PackagePath::parse(raw).map_err(|_| PackageError::InvalidMetadata)?;
            if reserved_path_component(path.as_str()) {
                return Err(PackageError::InvalidMetadata);
            }
            entries.insert(name, index);
        } else {
            return Err(PackageError::InvalidArchive);
        }
    }
    if REQUIRED_METADATA
        .iter()
        .any(|name| !entries.contains_key(*name))
    {
        return Err(PackageError::InvalidMetadata);
    }
    let manifest_bytes = read_zip_entry(archive, entries["manifest.json"], MAX_MANIFEST_BYTES)?;
    let inventory_bytes = read_zip_entry(archive, entries["inventory.json"], MAX_INVENTORY_BYTES)?;
    let publisher_bytes = read_zip_entry(archive, entries["publisher.ed25519"], 32)?;
    let signature_bytes = read_zip_entry(archive, entries["signature.ed25519"], 64)?;
    if publisher_bytes.len() != 32 || signature_bytes.len() != 64 {
        return Err(PackageError::InvalidMetadata);
    }
    let manifest = Manifest::decode(&manifest_bytes).map_err(|_| PackageError::InvalidMetadata)?;
    let inventory = parse_inventory(&inventory_bytes)?;
    let publisher_key: [u8; 32] = publisher_bytes
        .try_into()
        .map_err(|_| PackageError::InvalidMetadata)?;
    let key =
        VerifyingKey::from_bytes(&publisher_key).map_err(|_| PackageError::InvalidSignature)?;
    let publisher_key_id = hex(&Sha256::digest(publisher_key));
    if manifest.publisher_id.as_str() != publisher_key_id {
        return Err(PackageError::InvalidSignature);
    }
    let transcript = signature_transcript(&manifest_bytes, &inventory_bytes, &publisher_key)?;
    key.verify_strict(
        &transcript,
        &Signature::from_slice(&signature_bytes).map_err(|_| PackageError::InvalidSignature)?,
    )
    .map_err(|_| PackageError::InvalidSignature)?;
    validate_inventory_manifest(&manifest, &inventory)?;

    let payloads = entries
        .iter()
        .filter(|(name, _)| name.starts_with("payload/"))
        .collect::<Vec<_>>();
    if payloads.len() != inventory.len() {
        return Err(PackageError::InvalidMetadata);
    }
    let mut total = 0_u64;
    for record in &inventory {
        total = total
            .checked_add(record.size)
            .ok_or(PackageError::Limit("payload total"))?;
        if record.size > MAX_PAYLOAD_FILE_BYTES || total > MAX_TOTAL_PAYLOAD_BYTES {
            return Err(PackageError::Limit("payload bytes"));
        }
        let name = format!("payload/{}", record.path);
        let index = entries.get(&name).ok_or(PackageError::InvalidMetadata)?;
        let mut entry = archive
            .by_index(*index)
            .map_err(|_| PackageError::InvalidArchive)?;
        if entry.size() != record.size {
            return Err(PackageError::InvalidMetadata);
        }
        let mut hasher = Sha256::new();
        let mut count = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .ok_or(PackageError::Limit("payload size"))?;
            if count > record.size {
                return Err(PackageError::InvalidMetadata);
            }
            hasher.update(&buffer[..n]);
        }
        if count != record.size || hex(&hasher.finalize()) != record.sha256 {
            return Err(PackageError::InvalidMetadata);
        }
    }
    let artifact_sha256 = artifact.unwrap_or_default();
    Ok(VerifiedPackage {
        identity: PackageIdentity {
            publisher_key_id,
            app_id: manifest.app_id.clone(),
            version: manifest.version.clone(),
            artifact_sha256: artifact_sha256.clone(),
        },
        manifest,
        inventory,
        publisher_key,
        artifact_sha256,
    })
}

fn parse_inventory(bytes: &[u8]) -> Result<Vec<InventoryRecord>, PackageError> {
    if bytes.len() > MAX_INVENTORY_BYTES {
        return Err(PackageError::Limit("inventory bytes"));
    }
    let values: Vec<InventoryRecord> =
        serde_json::from_slice(bytes).map_err(|_| PackageError::InvalidMetadata)?;
    if values.len() > MAX_PAYLOAD_FILES {
        return Err(PackageError::Limit("payload files"));
    }
    let mut previous: Option<&str> = None;
    let mut folded = BTreeSet::new();
    let mut total = 0_u64;
    for value in &values {
        let path =
            PackagePath::parse(value.path.clone()).map_err(|_| PackageError::InvalidMetadata)?;
        if reserved_path_component(path.as_str()) || !is_digest(&value.sha256) {
            return Err(PackageError::InvalidMetadata);
        }
        if previous.is_some_and(|p| p.as_bytes() >= value.path.as_bytes()) {
            return Err(PackageError::InvalidMetadata);
        }
        previous = Some(&value.path);
        if !folded.insert(value.path.to_ascii_lowercase()) {
            return Err(PackageError::InvalidMetadata);
        }
        if value.size > MAX_PAYLOAD_FILE_BYTES {
            return Err(PackageError::Limit("payload file bytes"));
        }
        total = total
            .checked_add(value.size)
            .ok_or(PackageError::Limit("payload total"))?;
        if total > MAX_TOTAL_PAYLOAD_BYTES {
            return Err(PackageError::Limit("payload total"));
        }
    }
    Ok(values)
}

fn validate_inventory_manifest(
    manifest: &Manifest,
    inventory: &[InventoryRecord],
) -> Result<(), PackageError> {
    let indexed = inventory
        .iter()
        .map(|r| (r.path.as_str(), r))
        .collect::<BTreeMap<_, _>>();
    for entry in &manifest.entrypoints {
        if !indexed
            .get(entry.path.as_str())
            .is_some_and(|r| r.executable)
        {
            return Err(PackageError::InvalidMetadata);
        }
    }
    if let Some(ui) = &manifest.ui
        && !indexed.contains_key(ui.entrypoint.as_str())
    {
        return Err(PackageError::InvalidMetadata);
    }
    Ok(())
}

fn materialize<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    inventory: &[InventoryRecord],
    root: &Path,
) -> Result<(), PackageError> {
    for record in inventory {
        let rel =
            PackagePath::parse(record.path.clone()).map_err(|_| PackageError::InvalidMetadata)?;
        let dest = root.join("payload").join(rel.as_str());
        let parent = dest.parent().ok_or(PackageError::InvalidMetadata)?;
        fs::create_dir_all(parent)?;
        private_dir(parent)?;
        let name = format!("payload/{}", record.path);
        let mut entry = archive
            .by_name(&name)
            .map_err(|_| PackageError::InvalidArchive)?;
        let mut output = create_private_file(&dest)?;
        let mut hasher = Sha256::new();
        let copied = io::copy(&mut entry, &mut HashWriter::new(&mut output, &mut hasher))?;
        output.sync_all()?;
        if copied != record.size || hex(&hasher.finalize()) != record.sha256 {
            return Err(PackageError::InvalidMetadata);
        }
        set_payload_mode(&dest, record.executable)?;
    }
    Ok(())
}

struct HashWriter<'a, W> {
    writer: &'a mut W,
    hasher: &'a mut Sha256,
}
impl<'a, W> HashWriter<'a, W> {
    fn new(writer: &'a mut W, hasher: &'a mut Sha256) -> Self {
        Self { writer, hasher }
    }
}
impl<W: Write> Write for HashWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(bytes)?;
        self.hasher.update(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn write_metadata<R: Read + io::Seek>(
    root: &Path,
    archive: &mut ZipArchive<R>,
) -> Result<(), PackageError> {
    for name in REQUIRED_METADATA {
        let entry = archive
            .by_name(name)
            .map_err(|_| PackageError::InvalidArchive)?;
        let cap = match name {
            "manifest.json" => MAX_MANIFEST_BYTES,
            "inventory.json" => MAX_INVENTORY_BYTES,
            "publisher.ed25519" => 32,
            _ => 64,
        };
        let mut file = create_private_file(&root.join(name))?;
        io::copy(&mut entry.take((cap + 1) as u64), &mut file)?;
        file.sync_all()?;
    }
    Ok(())
}

fn verify_materialized_payload(
    root: &Path,
    inventory: &[InventoryRecord],
) -> Result<(), PackageError> {
    let mut expected = BTreeSet::new();
    for record in inventory {
        let path =
            PackagePath::parse(record.path.clone()).map_err(|_| PackageError::InvalidMetadata)?;
        let full = root.join("payload").join(path.as_str());
        let meta = fs::symlink_metadata(&full)?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != record.size {
            return Err(PackageError::InvalidMetadata);
        }
        let mut file = File::open(&full)?;
        let mut hasher = Sha256::new();
        io::copy(
            &mut file,
            &mut HashWriter::new(&mut io::sink(), &mut hasher),
        )?;
        if hex(&hasher.finalize()) != record.sha256 {
            return Err(PackageError::InvalidMetadata);
        }
        expected.insert(record.path.clone());
    }
    fn walk(dir: &Path, prefix: &str, expected: &mut BTreeSet<String>) -> Result<(), PackageError> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| PackageError::InvalidMetadata)?;
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_symlink() {
                return Err(PackageError::InvalidMetadata);
            }
            if kind.is_dir() {
                walk(&entry.path(), &path, expected)?;
            } else if !kind.is_file() || !expected.remove(&path) {
                return Err(PackageError::InvalidMetadata);
            }
        }
        Ok(())
    }
    let payload = root.join("payload");
    if payload.exists() {
        walk(&payload, "", &mut expected)?;
    }
    if !expected.is_empty() {
        return Err(PackageError::InvalidMetadata);
    }
    Ok(())
}

fn signature_transcript(
    manifest: &[u8],
    inventory: &[u8],
    key: &[u8; 32],
) -> Result<Vec<u8>, PackageError> {
    let ml = u32::try_from(manifest.len()).map_err(|_| PackageError::Limit("manifest bytes"))?;
    let il = u32::try_from(inventory.len()).map_err(|_| PackageError::Limit("inventory bytes"))?;
    let capacity = SIGNATURE_PREFIX
        .len()
        .checked_add(4 + manifest.len())
        .and_then(|n| n.checked_add(4 + inventory.len() + key.len()))
        .ok_or(PackageError::Limit("transcript"))?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(SIGNATURE_PREFIX);
    out.extend_from_slice(&ml.to_be_bytes());
    out.extend_from_slice(manifest);
    out.extend_from_slice(&il.to_be_bytes());
    out.extend_from_slice(inventory);
    out.extend_from_slice(key);
    Ok(out)
}

fn copy_bounded(source: &Path, destination: &Path) -> Result<[u8; 32], PackageError> {
    let mut input = File::open(source)?;
    let mut output = create_private_file(destination)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or(PackageError::Limit("archive bytes"))?;
        if total > MAX_ARCHIVE_BYTES {
            return Err(PackageError::Limit("archive bytes"));
        }
        output.write_all(&buffer[..n])?;
        hasher.update(&buffer[..n]);
    }
    output.sync_all()?;
    Ok(hasher.finalize().into())
}

fn installed_from(path: &Path, verified: VerifiedPackage) -> InstalledPackage {
    InstalledPackage {
        identity: verified.identity,
        manifest: verified.manifest,
        inventory: verified.inventory,
        path: path.to_owned(),
    }
}
fn read_zip_entry<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    max: usize,
) -> Result<Vec<u8>, PackageError> {
    let entry = archive
        .by_index(index)
        .map_err(|_| PackageError::InvalidArchive)?;
    if entry.size() > max as u64 {
        return Err(PackageError::Limit("metadata bytes"));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take((max + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(PackageError::Limit("metadata bytes"));
    }
    Ok(bytes)
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, PackageError> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > max as u64 {
        return Err(PackageError::Limit("metadata bytes"));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(path)?
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(PackageError::Limit("metadata bytes"));
    }
    Ok(bytes)
}
fn safe_dirs(path: &Path) -> Result<Vec<PathBuf>, PackageError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let t = entry.file_type()?;
        if t.is_dir() && !t.is_symlink() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}
fn reserved_path_component(path: &str) -> bool {
    path.split('/').any(|part| {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    })
}
fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate_identity(identity: &PackageIdentity) -> Result<(), PackageError> {
    if !is_digest(&identity.publisher_key_id) || !is_digest(&identity.artifact_sha256) {
        return Err(PackageError::InvalidMetadata);
    }
    Ok(())
}
fn hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(value.len() * 2);
    for b in value {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}
fn next_nonce() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
    )
}
fn open_lock(path: &Path) -> Result<File, PackageError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), PackageError> {
    let mut file = create_private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn set_payload_mode(path: &Path, executable: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if executable { 0o500 } else { 0o400 }),
        )?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
    }
    Ok(())
}
fn sync_dir(path: &Path) -> io::Result<()> {
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
fn sync_tree(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() && !kind.is_symlink() {
            sync_tree(&entry.path())?;
        }
    }
    sync_dir(path)
}
fn freeze_tree(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsafe staged file type",
            ));
        }
        if kind.is_dir() {
            freeze_tree(&entry.path())?;
            set_directory_mode(&entry.path(), true)?;
        } else {
            set_read_only_file(&entry.path())?;
        }
    }
    set_directory_mode(path, true)
}
fn make_removable(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "symlink in immutable package",
            ));
        }
        if kind.is_dir() {
            make_removable(&entry.path())?;
        } else if kind.is_file() {
            make_file_writable(&entry.path())?;
        }
    }
    set_directory_mode(path, true)
}
fn set_directory_mode(path: &Path, writable: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if writable { 0o700 } else { 0o500 }),
        )?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, writable);
    }
    Ok(())
}
fn set_read_only_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = fs::metadata(path)?.permissions().mode() & 0o111;
        fs::set_permissions(path, fs::Permissions::from_mode(0o400 | executable))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
fn make_file_writable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
struct TemporaryStage(PathBuf);

impl Drop for TemporaryStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn package(path: &Path, payload: &[u8], version: &str) {
        package_profile(
            path,
            payload,
            version,
            CompressionMethod::Stored,
            &[],
            false,
        );
    }

    fn package_profile(
        path: &Path,
        payload: &[u8],
        version: &str,
        compression: CompressionMethod,
        extra: &[(&str, &[u8])],
        corrupt_signature: bool,
    ) {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let key = signing.verifying_key().to_bytes();
        let publisher = hex(&Sha256::digest(key));
        let manifest = serde_json::json!({
            "schema_version": 1, "app_id": "sample.app", "publisher_id": publisher,
            "version": version, "name": "sample", "description": "", "host_protocol_min": {"major":1,"minor":0},
            "host_protocol_max": {"major":1,"minor":0}, "entrypoints": [{"target":"x86_64-unknown-linux-gnu","path":"bin/app"}],
            "requested_capabilities": [], "resources": [], "ui": null, "autostart_requested": false, "restart_requested": false
        });
        let manifest = serde_json::to_vec(&manifest).unwrap();
        let inventory = serde_json::to_vec(&vec![InventoryRecord {
            path: "bin/app".into(),
            size: payload.len() as u64,
            sha256: hex(&Sha256::digest(payload)),
            executable: true,
        }])
        .unwrap();
        let mut sig = signing
            .sign(&signature_transcript(&manifest, &inventory, &key).unwrap())
            .to_bytes();
        if corrupt_signature {
            sig[0] ^= 1;
        }
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(compression);
        for (name, bytes) in [
            ("manifest.json", manifest.as_slice()),
            ("inventory.json", inventory.as_slice()),
            ("publisher.ed25519", key.as_slice()),
            ("signature.ed25519", sig.as_slice()),
            ("payload/bin/app", payload),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        for (name, bytes) in extra {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn duplicate_payload_central_record(path: &Path) {
        let mut bytes = fs::read(path).unwrap();
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .unwrap();
        let central_offset =
            u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        let central_size =
            u32::from_le_bytes(bytes[eocd + 12..eocd + 16].try_into().unwrap()) as usize;
        let central_end = central_offset + central_size;
        let mut cursor = central_offset;
        let mut duplicate = None;
        while cursor < central_end {
            assert_eq!(&bytes[cursor..cursor + 4], b"PK\x01\x02");
            let name_len =
                u16::from_le_bytes(bytes[cursor + 28..cursor + 30].try_into().unwrap()) as usize;
            let extra_len =
                u16::from_le_bytes(bytes[cursor + 30..cursor + 32].try_into().unwrap()) as usize;
            let comment_len =
                u16::from_le_bytes(bytes[cursor + 32..cursor + 34].try_into().unwrap()) as usize;
            let record_len = 46 + name_len + extra_len + comment_len;
            if &bytes[cursor + 46..cursor + 46 + name_len] == b"payload/bin/app" {
                duplicate = Some(bytes[cursor..cursor + record_len].to_vec());
                break;
            }
            cursor += record_len;
        }
        let duplicate = duplicate.unwrap();
        bytes.splice(eocd..eocd, duplicate.iter().copied());
        let eocd = eocd + duplicate.len();
        let entries_on_disk = u16::from_le_bytes(bytes[eocd + 8..eocd + 10].try_into().unwrap());
        let entries_total = u16::from_le_bytes(bytes[eocd + 10..eocd + 12].try_into().unwrap());
        bytes[eocd + 8..eocd + 10].copy_from_slice(&(entries_on_disk + 1).to_le_bytes());
        bytes[eocd + 10..eocd + 12].copy_from_slice(&(entries_total + 1).to_le_bytes());
        bytes[eocd + 12..eocd + 16]
            .copy_from_slice(&((central_size + duplicate.len()) as u32).to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn signed_package_verifies_and_installs_immutably() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("test.i2prapp");
        package(&input, b"hello", "1.0");
        let verified = verify_file(&input).unwrap();
        assert_eq!(verified.manifest.app_id.as_str(), "sample.app");
        let store = PackageStore::open(tmp.path().join("store")).unwrap();
        let first = store.install(&input).unwrap();
        let again = store.install(&input).unwrap();
        assert_eq!(first.identity, again.identity);
        assert_eq!(store.list().unwrap().len(), 1);
        store.verify_installed(&first.path).unwrap();
    }

    #[test]
    fn signature_transcript_has_a_stable_ed25519_golden() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let key = signing.verifying_key().to_bytes();
        let manifest = br#"{"schema_version":1,"app_id":"golden.app"}"#;
        let inventory = b"[]";
        let transcript = signature_transcript(manifest, inventory, &key).unwrap();
        let signature = signing.sign(&transcript).to_bytes();
        assert_eq!(
            hex(&Sha256::digest(&transcript)),
            "835ba3477b01737f0708db55ed15d0471aae4219a84a968f311cc2c815b6b687"
        );
        assert_eq!(
            hex(&signature),
            "6c94150caa8662be04a5fceefb6a1467c69e588cdf0f9847e0bee30c081c2578490d3e3115dc570ee4676e77a3b3cfe5887b4206342cec3fe1a887c48af37409"
        );
        assert!(
            signing
                .verifying_key()
                .verify_strict(&transcript, &Signature::from_bytes(&signature))
                .is_ok()
        );
    }

    #[test]
    fn staged_copy_is_the_only_package_bytes_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.i2prapp");
        let staged = tmp.path().join("staged.i2prapp");
        package(&source, b"original", "1.0");
        let digest = hex(&copy_bounded(&source, &staged).unwrap());
        package(&source, b"replaced after copy", "2.0");
        let mut archive = ZipArchive::new(File::open(&staged).unwrap()).unwrap();
        let verified = verify_archive(&mut archive, Some(digest.clone())).unwrap();
        assert_eq!(verified.identity.version.as_str(), "1.0");
        assert_eq!(verified.artifact_sha256, digest);
    }

    #[test]
    fn different_artifact_for_same_publisher_app_version_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.zip");
        let b = tmp.path().join("b.zip");
        package(&a, b"one", "1.0");
        package(&b, b"two", "1.0");
        let store = PackageStore::open(tmp.path().join("store")).unwrap();
        store.install(&a).unwrap();
        assert!(matches!(
            store.install(&b),
            Err(PackageError::VersionConflict)
        ));
    }

    #[test]
    fn malformed_signature_path_and_compression_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let bad_signature = tmp.path().join("bad-signature.i2prapp");
        package_profile(
            &bad_signature,
            b"hello",
            "1.0",
            CompressionMethod::Stored,
            &[],
            true,
        );
        assert!(matches!(
            verify_file(&bad_signature),
            Err(PackageError::InvalidSignature)
        ));
        let store = PackageStore::open(tmp.path().join("bad-signature-store")).unwrap();
        assert!(matches!(
            store.install(&bad_signature),
            Err(PackageError::InvalidSignature)
        ));
        assert!(store.list().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(store.root.join(".staging")).unwrap().count(),
            0
        );

        let traversal = tmp.path().join("traversal.i2prapp");
        package_profile(
            &traversal,
            b"hello",
            "1.0",
            CompressionMethod::Stored,
            &[("payload/../escape", b"bad")],
            false,
        );
        assert!(verify_file(&traversal).is_err());

        let compressed = tmp.path().join("compressed.i2prapp");
        package_profile(
            &compressed,
            b"hello",
            "1.0",
            CompressionMethod::Deflated,
            &[],
            false,
        );
        assert!(verify_file(&compressed).is_err());
    }

    #[test]
    fn duplicate_payload_entries_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let duplicate = tmp.path().join("duplicate.i2prapp");
        package(&duplicate, b"hello", "1.0");
        duplicate_payload_central_record(&duplicate);
        let result = verify_file(&duplicate);
        assert!(matches!(result, Err(PackageError::InvalidArchive)));
    }

    #[cfg(unix)]
    #[test]
    fn abandoned_staging_cleanup_unlinks_symlinks_without_following_them() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), b"safe").unwrap();
        let store = PackageStore::open(tmp.path().join("store")).unwrap();
        symlink(&outside, store.root.join(".staging/escape")).unwrap();
        assert_eq!(store.cleanup_staging().unwrap(), 1);
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"safe");
        assert!(!store.root.join(".staging/escape").exists());
    }

    #[test]
    fn tampered_payload_does_not_verify_after_install() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("a.zip");
        package(&input, b"hello", "1.0");
        let store = PackageStore::open(tmp.path().join("store")).unwrap();
        let installed = store.install(&input).unwrap();
        let manifest = installed.path.join("manifest.json");
        make_file_writable(&manifest).unwrap();
        let mut bytes = fs::read(&manifest).unwrap();
        bytes.push(b' ');
        fs::write(manifest, bytes).unwrap();
        assert!(store.verify_installed(&installed.path).is_err());
    }
}
