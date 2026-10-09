//! Deterministic `.i2prapp` construction. The builder has no store, launch,
//! grant, runtime, or network authority. It reopens and verifies its output;
//! the router independently runs its canonical verifier before installation.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use ed25519_dalek::{Signer, SigningKey};
use i2pr_app_proto::{MAX_MANIFEST_BYTES, Manifest, PackagePath};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip_stored::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const PREFIX: &[u8] = b"I2PR-APP-PACKAGE-V1\0";
const MAX_FILES: usize = 4096;
const MAX_FILE: u64 = 256 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_INVENTORY: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("package input is invalid")]
    InvalidInput,
    #[error("package input exceeds a format limit")]
    Limit,
    #[error("package I/O failed")]
    Io(#[from] std::io::Error),
    #[error("package archive failed")]
    Archive(#[from] zip_stored::result::ZipError),
    #[error("constructed package failed verification")]
    Verification,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryRecord {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}

/// Build from exact canonical manifest bytes and a payload directory. The
/// directory tree must contain regular files only; symlinks are refused.
pub fn build_package(
    manifest_bytes: &[u8],
    payload_root: &Path,
    output: &Path,
    signing_key: &SigningKey,
) -> Result<(), BuildError> {
    let mut output_created = false;
    match build_package_inner(
        manifest_bytes,
        payload_root,
        output,
        signing_key,
        &mut output_created,
    ) {
        Ok(()) => Ok(()),
        Err(error) => {
            if output_created {
                let _ = fs::remove_file(output);
            }
            Err(error)
        }
    }
}

fn build_package_inner(
    manifest_bytes: &[u8],
    payload_root: &Path,
    output: &Path,
    signing_key: &SigningKey,
    output_created: &mut bool,
) -> Result<(), BuildError> {
    if manifest_bytes.is_empty() || manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(BuildError::Limit);
    }
    let manifest = Manifest::decode(manifest_bytes).map_err(|_| BuildError::InvalidInput)?;
    let public_key = signing_key.verifying_key().to_bytes();
    if manifest.publisher_id.as_str() != hex(&Sha256::digest(public_key)) {
        return Err(BuildError::InvalidInput);
    }
    let files = collect(payload_root)?;
    if files.len() > MAX_FILES {
        return Err(BuildError::Limit);
    }
    let mut records = Vec::with_capacity(files.len());
    let mut payload = Vec::with_capacity(files.len());
    let mut total = 0_u64;
    for (relative, path) in files {
        let value = PackagePath::parse(relative.clone()).map_err(|_| BuildError::InvalidInput)?;
        if value.as_str().split('/').any(reserved_component) {
            return Err(BuildError::InvalidInput);
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(BuildError::InvalidInput);
        }
        if metadata.len() > MAX_FILE {
            return Err(BuildError::Limit);
        }
        let input = fs::File::open(&path)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        input.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > MAX_FILE {
            return Err(BuildError::InvalidInput);
        }
        let size = u64::try_from(bytes.len()).map_err(|_| BuildError::Limit)?;
        total = total.checked_add(size).ok_or(BuildError::Limit)?;
        if size > MAX_FILE || total > MAX_TOTAL {
            return Err(BuildError::Limit);
        }
        let executable = executable(&metadata);
        if manifest
            .entrypoints
            .iter()
            .any(|entry| entry.path.as_str() == relative)
            && !executable
        {
            return Err(BuildError::InvalidInput);
        }
        records.push(InventoryRecord {
            path: relative,
            size,
            sha256: hex(&Sha256::digest(&bytes)),
            executable,
        });
        payload.push((records.last().expect("record inserted").path.clone(), bytes));
    }
    let inventory = serde_json::to_vec(&records).map_err(|_| BuildError::InvalidInput)?;
    if inventory.len() > MAX_INVENTORY {
        return Err(BuildError::Limit);
    }
    let transcript = transcript(manifest_bytes, &inventory, &public_key)?;
    let signature = signing_key.sign(&transcript).to_bytes();
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    *output_created = true;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in [
        ("manifest.json", manifest_bytes.to_vec()),
        ("inventory.json", inventory),
        ("publisher.ed25519", public_key.to_vec()),
        ("signature.ed25519", signature.to_vec()),
    ] {
        zip.start_file(name, options)?;
        zip.write_all(&bytes)?;
    }
    for (path, bytes) in payload {
        zip.start_file(
            format!("payload/{path}"),
            options.unix_permissions(if records.iter().any(|r| r.path == path && r.executable) {
                0o755
            } else {
                0o644
            }),
        )?;
        zip.write_all(&bytes)?;
    }
    let file = zip.finish()?;
    file.sync_all()?;
    if fs::metadata(output)?.len() > MAX_ARCHIVE {
        return Err(BuildError::Limit);
    }
    verify_generated(
        output,
        &public_key,
        &signature,
        manifest_bytes,
        &manifest,
        signing_key,
    )?;
    Ok(())
}

fn verify_generated(
    path: &Path,
    public: &[u8; 32],
    signature: &[u8; 64],
    manifest: &[u8],
    manifest_value: &Manifest,
    key: &SigningKey,
) -> Result<(), BuildError> {
    use ed25519_dalek::Verifier;
    let mut archive = ZipArchive::new(fs::File::open(path)?)?;
    if archive.len() > MAX_FILES + 4 {
        return Err(BuildError::Verification);
    }
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        if file.is_dir()
            || file.compression() != CompressionMethod::Stored
            || !names.insert(file.name().to_owned())
        {
            return Err(BuildError::Verification);
        }
    }
    let got_manifest =
        read_entry_limited(&mut archive, "manifest.json", MAX_MANIFEST_BYTES as u64)?;
    let got_key = read_entry_limited(&mut archive, "publisher.ed25519", 32)?;
    let got_sig = read_entry_limited(&mut archive, "signature.ed25519", 64)?;
    if got_manifest != manifest || got_key != public || got_sig != signature {
        return Err(BuildError::Verification);
    }
    let inventory_bytes = read_entry_limited(&mut archive, "inventory.json", MAX_INVENTORY as u64)?;
    if inventory_bytes.len() > MAX_INVENTORY {
        return Err(BuildError::Verification);
    }
    let inventory: Vec<InventoryRecord> =
        serde_json::from_slice(&inventory_bytes).map_err(|_| BuildError::Verification)?;
    if inventory.len() + 4 != archive.len() || inventory.len() > MAX_FILES {
        return Err(BuildError::Verification);
    }
    let mut previous: Option<&str> = None;
    let mut folded = BTreeSet::new();
    let index = inventory
        .iter()
        .map(|record| (record.path.as_str(), record))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut total = 0_u64;
    for record in &inventory {
        let path = PackagePath::parse(record.path.clone()).map_err(|_| BuildError::Verification)?;
        if path.as_str().split('/').any(reserved_component)
            || previous.is_some_and(|p| p >= record.path.as_str())
            || !folded.insert(record.path.to_ascii_lowercase())
            || record.sha256.len() != 64
            || !record
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(BuildError::Verification);
        }
        previous = Some(&record.path);
        total = total.checked_add(record.size).ok_or(BuildError::Limit)?;
        if record.size > MAX_FILE || total > MAX_TOTAL || record.sha256.len() != 64 {
            return Err(BuildError::Verification);
        }
        let name = format!("payload/{}", record.path);
        let entry = archive.by_name(&name)?;
        if entry.size() != record.size {
            return Err(BuildError::Verification);
        }
        let mut payload = Vec::new();
        entry
            .take(record.size.saturating_add(1))
            .read_to_end(&mut payload)?;
        if payload.len() as u64 != record.size || hex(&Sha256::digest(&payload)) != record.sha256 {
            return Err(BuildError::Verification);
        }
    }
    for entrypoint in &manifest_value.entrypoints {
        if !index
            .get(entrypoint.path.as_str())
            .is_some_and(|record| record.executable)
        {
            return Err(BuildError::Verification);
        }
    }
    if let Some(ui) = &manifest_value.ui
        && !index.contains_key(ui.entrypoint.as_str())
    {
        return Err(BuildError::Verification);
    }
    key.verifying_key()
        .verify(
            &transcript(manifest, &inventory_bytes, public)?,
            &ed25519_dalek::Signature::from_bytes(signature),
        )
        .map_err(|_| BuildError::Verification)
}

fn read_entry_limited(
    archive: &mut ZipArchive<fs::File>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, BuildError> {
    let mut out = Vec::new();
    let entry = archive.by_name(name)?;
    if entry.size() > limit {
        return Err(BuildError::Verification);
    }
    entry.take(limit + 1).read_to_end(&mut out)?;
    Ok(out)
}
fn collect(root: &Path) -> Result<Vec<(String, PathBuf)>, BuildError> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), BuildError> {
        for item in fs::read_dir(dir)? {
            let item = item?;
            let path = item.path();
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                return Err(BuildError::InvalidInput);
            }
            if meta.is_dir() {
                walk(root, &path, out)?;
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|_| BuildError::InvalidInput)?
                    .to_str()
                    .ok_or(BuildError::InvalidInput)?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                out.push((rel, path));
            } else {
                return Err(BuildError::InvalidInput);
            }
        }
        Ok(())
    }
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(BuildError::InvalidInput);
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    let mut set = BTreeSet::new();
    let mut portable_set = BTreeSet::new();
    if out
        .iter()
        .any(|(p, _)| !set.insert(p.clone()) || !portable_set.insert(p.to_ascii_lowercase()))
    {
        return Err(BuildError::InvalidInput);
    }
    Ok(out)
}
fn executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}
fn transcript(manifest: &[u8], inventory: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, BuildError> {
    let ml = u32::try_from(manifest.len()).map_err(|_| BuildError::Limit)?;
    let il = u32::try_from(inventory.len()).map_err(|_| BuildError::Limit)?;
    let capacity = PREFIX
        .len()
        .checked_add(4 + manifest.len())
        .and_then(|n| n.checked_add(4 + inventory.len() + 32))
        .ok_or(BuildError::Limit)?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(PREFIX);
    out.extend_from_slice(&ml.to_be_bytes());
    out.extend_from_slice(manifest);
    out.extend_from_slice(&il.to_be_bytes());
    out.extend_from_slice(inventory);
    out.extend_from_slice(key);
    Ok(out)
}
fn reserved_component(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    #[test]
    fn deterministic_output_is_accepted_by_router_verifier() {
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("payload");
        fs::create_dir(&payload).unwrap();
        fs::write(payload.join("main"), b"app").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(payload.join("main"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        let key = SigningKey::from_bytes(&[23; 32]);
        let publisher = hex(&Sha256::digest(key.verifying_key().to_bytes()));
        let manifest = format!(
            r#"{{"schema_version":1,"app_id":"sdk.fixture","publisher_id":"{publisher}","version":"1.0.0","name":"SDK fixture","description":"","host_protocol_min":{{"major":1,"minor":0}},"host_protocol_max":{{"major":1,"minor":1}},"entrypoints":[{{"target":"x86_64-unknown-linux-gnu","path":"main"}}],"requested_capabilities":[],"resources":[],"ui":null,"autostart_requested":false,"restart_requested":false}}"#
        );
        let one = dir.path().join("one.i2prapp");
        let two = dir.path().join("two.i2prapp");
        build_package(manifest.as_bytes(), &payload, &one, &key).unwrap();
        build_package(manifest.as_bytes(), &payload, &two, &key).unwrap();
        assert_eq!(fs::read(&one).unwrap(), fs::read(&two).unwrap());
        let first = fs::read(&one).unwrap();
        assert!(build_package(manifest.as_bytes(), &payload, &one, &key).is_err());
        assert_eq!(
            fs::read(&one).unwrap(),
            first,
            "existing output is never overwritten"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_payload_is_refused_without_leaving_output() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("payload");
        fs::create_dir(&payload).unwrap();
        fs::write(dir.path().join("outside"), b"outside").unwrap();
        symlink(dir.path().join("outside"), payload.join("link")).unwrap();
        let key = SigningKey::from_bytes(&[23; 32]);
        let publisher = hex(&Sha256::digest(key.verifying_key().to_bytes()));
        let manifest = format!(
            r#"{{"schema_version":1,"app_id":"sdk.fixture","publisher_id":"{publisher}","version":"1.0.0","name":"SDK fixture","description":"","host_protocol_min":{{"major":1,"minor":0}},"host_protocol_max":{{"major":1,"minor":1}},"entrypoints":[{{"target":"x86_64-unknown-linux-gnu","path":"main"}}],"requested_capabilities":[],"resources":[],"ui":null,"autostart_requested":false,"restart_requested":false}}"#
        );
        let output = dir.path().join("invalid.i2prapp");
        assert!(build_package(manifest.as_bytes(), &payload, &output, &key).is_err());
        assert!(!output.exists());
    }
}
