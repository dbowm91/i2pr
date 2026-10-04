//! Opaque bounded cache for authenticated-content source records.
//!
//! This storage layer never considers bytes trusted. Callers must validate
//! a loaded record again before publishing it to a live consumer.

#![forbid(unsafe_code)]

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{create_temporary_file, ensure_secure_directory, validate_existing_directory};

pub const VERIFIED_CONTENT_CACHE_DIR: &str = "news";
pub const VERIFIED_CONTENT_CACHE_FILE: &str = "verified-content.cache";
pub const VERIFIED_CONTENT_CACHE_BACKUP: &str = "verified-content.backup";
pub const MAX_VERIFIED_CONTENT_CACHE_BYTES: usize = 8 * 1024 * 1024 + 16 * 1024;

#[derive(Debug, Error)]
pub enum VerifiedContentCacheError {
    #[error("verified-content cache {operation} failed: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("verified-content cache path is not a regular non-symlink path")]
    UnsafePath,
    #[error("verified-content cache permissions are too permissive")]
    InsecurePermissions,
    #[error("verified-content cache record size {actual} exceeds {maximum}")]
    TooLarge { actual: usize, maximum: usize },
}

fn cache_io(operation: &'static str, source: io::Error) -> VerifiedContentCacheError {
    VerifiedContentCacheError::Io { operation, source }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedContentCacheStore {
    dir: PathBuf,
}

impl VerifiedContentCacheStore {
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join(VERIFIED_CONTENT_CACHE_DIR),
        }
    }

    pub fn prepare(&self) -> Result<(), VerifiedContentCacheError> {
        ensure_secure_directory(&self.dir).map_err(map_storage_error)
    }

    pub fn publish(&self, record: &[u8]) -> Result<(), VerifiedContentCacheError> {
        if record.len() > MAX_VERIFIED_CONTENT_CACHE_BYTES {
            return Err(VerifiedContentCacheError::TooLarge {
                actual: record.len(),
                maximum: MAX_VERIFIED_CONTENT_CACHE_BYTES,
            });
        }
        self.prepare()?;
        let (temporary_path, mut temporary) =
            create_temporary_file(&self.dir, "verified-content").map_err(map_storage_error)?;
        let install = (|| {
            temporary
                .write_all(record)
                .map_err(|source| cache_io("write temporary record", source))?;
            temporary
                .sync_all()
                .map_err(|source| cache_io("sync temporary record", source))?;
            drop(temporary);
            let current = self.dir.join(VERIFIED_CONTENT_CACHE_FILE);
            if fs::symlink_metadata(&current).is_ok() {
                validate_cache_file(&current)?;
                fs::rename(&current, self.dir.join(VERIFIED_CONTENT_CACHE_BACKUP))
                    .map_err(|source| cache_io("rotate backup record", source))?;
            }
            fs::rename(&temporary_path, &current)
                .map_err(|source| cache_io("install current record", source))?;
            super::sync_directory(&self.dir).map_err(map_storage_error)
        })();
        if install.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        install
    }

    pub fn load_current(&self) -> Result<Option<Vec<u8>>, VerifiedContentCacheError> {
        self.load_slot(VERIFIED_CONTENT_CACHE_FILE)
    }

    pub fn load_backup(&self) -> Result<Option<Vec<u8>>, VerifiedContentCacheError> {
        self.load_slot(VERIFIED_CONTENT_CACHE_BACKUP)
    }

    fn load_slot(&self, name: &str) -> Result<Option<Vec<u8>>, VerifiedContentCacheError> {
        match fs::symlink_metadata(&self.dir) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(cache_io("inspect cache directory", source)),
            Ok(_) => {}
        }
        validate_existing_directory(&self.dir).map_err(map_storage_error)?;
        let path = self.dir.join(name);
        match fs::symlink_metadata(&path) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(cache_io("inspect cache record", source)),
            Ok(_) => {}
        }
        validate_cache_file(&path)?;
        let file = File::open(&path).map_err(|source| cache_io("open cache record", source))?;
        let mut record = Vec::new();
        file.take((MAX_VERIFIED_CONTENT_CACHE_BYTES + 1) as u64)
            .read_to_end(&mut record)
            .map_err(|source| cache_io("read cache record", source))?;
        if record.len() > MAX_VERIFIED_CONTENT_CACHE_BYTES {
            return Err(VerifiedContentCacheError::TooLarge {
                actual: record.len(),
                maximum: MAX_VERIFIED_CONTENT_CACHE_BYTES,
            });
        }
        Ok(Some(record))
    }
}

fn validate_cache_file(path: &Path) -> Result<(), VerifiedContentCacheError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| cache_io("inspect cache file", source))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(VerifiedContentCacheError::UnsafePath);
    }
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_VERIFIED_CONTENT_CACHE_BYTES {
        return Err(VerifiedContentCacheError::TooLarge {
            actual: size,
            maximum: MAX_VERIFIED_CONTENT_CACHE_BYTES,
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 || mode & 0o400 == 0 {
            return Err(VerifiedContentCacheError::InsecurePermissions);
        }
    }
    Ok(())
}

fn map_storage_error(error: super::StorageError) -> VerifiedContentCacheError {
    match error {
        super::StorageError::Io { operation, source } => {
            VerifiedContentCacheError::Io { operation, source }
        }
        super::StorageError::UnsafePath | super::StorageError::AlreadyExists => {
            VerifiedContentCacheError::UnsafePath
        }
        super::StorageError::InsecurePermissions => VerifiedContentCacheError::InsecurePermissions,
        super::StorageError::TooLarge { actual, maximum } => {
            VerifiedContentCacheError::TooLarge { actual, maximum }
        }
        other => cache_io("storage operation", io::Error::other(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_and_rotates_opaque_records() {
        let data_dir = tempfile::tempdir().unwrap();
        let store = VerifiedContentCacheStore::in_data_dir(data_dir.path());
        assert_eq!(store.load_current().unwrap(), None);
        store.publish(b"record-one").unwrap();
        store.publish(b"record-two").unwrap();
        assert_eq!(store.load_current().unwrap(), Some(b"record-two".to_vec()));
        assert_eq!(store.load_backup().unwrap(), Some(b"record-one".to_vec()));
    }

    #[test]
    fn rejects_oversized_records_and_symlink_loads() {
        let data_dir = tempfile::tempdir().unwrap();
        let store = VerifiedContentCacheStore::in_data_dir(data_dir.path());
        assert!(matches!(
            store.publish(&vec![0; MAX_VERIFIED_CONTENT_CACHE_BYTES + 1]),
            Err(VerifiedContentCacheError::TooLarge { .. })
        ));
        store.prepare().unwrap();
        let path = store.dir.join(VERIFIED_CONTENT_CACHE_FILE);
        #[cfg(unix)]
        std::os::unix::fs::symlink(data_dir.path().join("absent"), &path).unwrap();
        #[cfg(unix)]
        assert!(matches!(
            store.load_current(),
            Err(VerifiedContentCacheError::UnsafePath)
        ));
    }
}
