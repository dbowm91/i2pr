//! Plan 294 versioned address-book generation persistence adapter.
//!
//! The adapter stores opaque generation bytes only: it never parses
//! JSON, hostnames, or destinations. `i2pr-addressbook` owns
//! serialization and validation (`encode_generation` /
//! `decode_generation`); `i2pr-daemon` composes the two halves and
//! decides activation.
//!
//! Layout under one state directory (fixed filenames, no caller path
//! input):
//!
//! - `addressbook.current.json`: the latest published generation.
//! - `addressbook.backup.json`: the previous generation (last-known-good
//!   rollback copy).
//!
//! Publication is crash-atomic: write + sync a temporary file, rotate
//! the previous current over the backup (POSIX atomic rename), rename
//! the temporary file over current, then sync the directory. A failed
//! publication leaves the prior current untouched. Loads reject
//! symlinks, non-regular files, permissive modes, and over-ceiling
//! sizes before reading a byte.

#![forbid(unsafe_code)]

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{
    create_temporary_file, ensure_secure_directory, sync_directory, validate_existing_directory,
};

/// State subdirectory name under the router data directory.
pub const ADDRESSBOOK_STATE_SUBDIR: &str = "addressbook";
/// Latest published generation filename.
pub const ADDRESSBOOK_CURRENT_FILE_NAME: &str = "addressbook.current.json";
/// Previous-generation rollback filename.
pub const ADDRESSBOOK_BACKUP_FILE_NAME: &str = "addressbook.backup.json";
/// Maximum bytes read from a generation file (above the crate's own
/// generation ceiling so storage never truncates a legal generation).
pub const MAX_ADDRESSBOOK_GENERATION_FILE_SIZE: usize = 32_000_000;

/// Typed errors from the address-book generation adapter.
#[derive(Debug, Error)]
pub enum AddressBookGenerationStorageError {
    /// A filesystem operation failed.
    #[error("address-book generation storage {operation} failed: {source}")]
    Io {
        /// Static filesystem operation category.
        operation: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A path is a symlink or another unsafe filesystem object.
    #[error("address-book generation path is not a regular non-symlink path")]
    UnsafePath,
    /// A file or directory exposes generation material too broadly.
    #[error("address-book generation permissions are too permissive")]
    InsecurePermissions,
    /// A generation file exceeds the read ceiling.
    #[error("address-book generation file exceeds {maximum} bytes")]
    TooLarge {
        /// Actual or declared size.
        actual: usize,
        /// Maximum accepted size.
        maximum: usize,
    },
}

fn storage_io(operation: &'static str, source: io::Error) -> AddressBookGenerationStorageError {
    AddressBookGenerationStorageError::Io { operation, source }
}

/// Opaque versioned-generation file store for one state directory.
#[derive(Clone, Debug)]
pub struct AddressBookGenerationStore {
    dir: PathBuf,
}

impl AddressBookGenerationStore {
    /// Binds the store to a state directory (no filesystem effect;
    /// [`Self::prepare`] creates and hardens it).
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
        }
    }

    /// Creates or validates the state directory (0700, no symlinks).
    pub fn prepare(&self) -> Result<(), AddressBookGenerationStorageError> {
        ensure_secure_directory(&self.dir).map_err(map_storage_error)
    }

    /// Publishes one complete generation atomically, rotating the
    /// previous current generation into the backup slot. Failure
    /// leaves the prior current file untouched.
    pub fn publish(&self, generation: &[u8]) -> Result<(), AddressBookGenerationStorageError> {
        self.prepare()?;
        let (temporary_path, mut temporary) =
            create_temporary_file(&self.dir, "addressbook").map_err(map_storage_error)?;
        let result = (|| {
            temporary
                .write_all(generation)
                .map_err(|source| storage_io("write temporary generation", source))?;
            temporary
                .sync_all()
                .map_err(|source| storage_io("sync temporary generation", source))?;
            drop(temporary);
            let current = self.dir.join(ADDRESSBOOK_CURRENT_FILE_NAME);
            if fs::symlink_metadata(&current).is_ok() {
                let backup = self.dir.join(ADDRESSBOOK_BACKUP_FILE_NAME);
                fs::rename(&current, &backup)
                    .map_err(|source| storage_io("rotate backup generation", source))?;
            }
            fs::rename(&temporary_path, &current)
                .map_err(|source| storage_io("install current generation", source))?;
            sync_directory(&self.dir).map_err(map_storage_error)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    /// Loads the current generation (`None` when never published).
    pub fn load_current(&self) -> Result<Option<Vec<u8>>, AddressBookGenerationStorageError> {
        self.load_slot(ADDRESSBOOK_CURRENT_FILE_NAME)
    }

    /// Loads the backup generation (`None` when never rotated).
    pub fn load_backup(&self) -> Result<Option<Vec<u8>>, AddressBookGenerationStorageError> {
        self.load_slot(ADDRESSBOOK_BACKUP_FILE_NAME)
    }

    fn load_slot(
        &self,
        file_name: &str,
    ) -> Result<Option<Vec<u8>>, AddressBookGenerationStorageError> {
        match fs::symlink_metadata(&self.dir) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(storage_io("inspect generation directory", source)),
            Ok(_) => {}
        }
        validate_existing_directory(&self.dir).map_err(map_storage_error)?;
        let path = self.dir.join(file_name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(storage_io("inspect generation file", source)),
        };
        validate_generation_file_metadata(&metadata)?;
        let length = usize::try_from(metadata.len()).map_err(|_| {
            AddressBookGenerationStorageError::TooLarge {
                actual: usize::MAX,
                maximum: MAX_ADDRESSBOOK_GENERATION_FILE_SIZE,
            }
        })?;
        if length > MAX_ADDRESSBOOK_GENERATION_FILE_SIZE {
            return Err(AddressBookGenerationStorageError::TooLarge {
                actual: length,
                maximum: MAX_ADDRESSBOOK_GENERATION_FILE_SIZE,
            });
        }
        let mut file =
            File::open(&path).map_err(|source| storage_io("open generation file", source))?;
        let mut bytes = Vec::with_capacity(length.min(1_048_576));
        file.read_to_end(&mut bytes)
            .map_err(|source| storage_io("read generation file", source))?;
        if bytes.len() > MAX_ADDRESSBOOK_GENERATION_FILE_SIZE {
            return Err(AddressBookGenerationStorageError::TooLarge {
                actual: bytes.len(),
                maximum: MAX_ADDRESSBOOK_GENERATION_FILE_SIZE,
            });
        }
        Ok(Some(bytes))
    }
}

fn validate_generation_file_metadata(
    metadata: &fs::Metadata,
) -> Result<(), AddressBookGenerationStorageError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AddressBookGenerationStorageError::UnsafePath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 || mode & 0o400 == 0 {
            return Err(AddressBookGenerationStorageError::InsecurePermissions);
        }
    }
    Ok(())
}

fn map_storage_error(source: super::StorageError) -> AddressBookGenerationStorageError {
    match source {
        super::StorageError::Io { operation, source } => {
            AddressBookGenerationStorageError::Io { operation, source }
        }
        super::StorageError::UnsafePath => AddressBookGenerationStorageError::UnsafePath,
        super::StorageError::AlreadyExists => AddressBookGenerationStorageError::UnsafePath,
        super::StorageError::InsecurePermissions => {
            AddressBookGenerationStorageError::InsecurePermissions
        }
        super::StorageError::TooLarge { actual, maximum } => {
            AddressBookGenerationStorageError::TooLarge { actual, maximum }
        }
        super::StorageError::Truncated
        | super::StorageError::TrailingBytes
        | super::StorageError::Malformed { .. }
        | super::StorageError::UnsupportedVersion { .. }
        | super::StorageError::UnsupportedAlgorithm { .. }
        | super::StorageError::Integrity
        | super::StorageError::Crypto(_) => AddressBookGenerationStorageError::UnsafePath,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(directory: &tempfile::TempDir) -> AddressBookGenerationStore {
        AddressBookGenerationStore::new(&directory.path().join(ADDRESSBOOK_STATE_SUBDIR))
    }

    #[test]
    fn publish_load_and_rotation_round_trip() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(&directory);
        assert_eq!(store.load_current().expect("empty current"), None);
        assert_eq!(store.load_backup().expect("empty backup"), None);
        store.publish(b"generation-one").expect("publish one");
        assert_eq!(
            store.load_current().expect("current one"),
            Some(b"generation-one".to_vec())
        );
        assert_eq!(store.load_backup().expect("no backup yet"), None);
        store.publish(b"generation-two").expect("publish two");
        assert_eq!(
            store.load_current().expect("current two"),
            Some(b"generation-two".to_vec())
        );
        assert_eq!(
            store.load_backup().expect("backup one"),
            Some(b"generation-one".to_vec())
        );
        // A third publication rotates again; the middle generation survives.
        store.publish(b"generation-three").expect("publish three");
        assert_eq!(
            store.load_backup().expect("backup two"),
            Some(b"generation-two".to_vec())
        );
    }

    #[test]
    fn symlink_and_special_files_are_rejected() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(&directory);
        store.prepare().expect("prepare");
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = directory.path().join("target.json");
            std::fs::write(&target, b"planted").expect("target");
            let link = directory
                .path()
                .join(ADDRESSBOOK_STATE_SUBDIR)
                .join(ADDRESSBOOK_CURRENT_FILE_NAME);
            symlink(&target, &link).expect("symlink");
            assert!(matches!(
                store.load_current(),
                Err(AddressBookGenerationStorageError::UnsafePath)
            ));
        }
    }

    #[test]
    fn oversized_files_fail_before_read() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(&directory);
        store.prepare().expect("prepare");
        let path = directory
            .path()
            .join(ADDRESSBOOK_STATE_SUBDIR)
            .join(ADDRESSBOOK_CURRENT_FILE_NAME);
        let file = File::create(&path).expect("sparse file");
        file.set_len((MAX_ADDRESSBOOK_GENERATION_FILE_SIZE + 1) as u64)
            .expect("truncate");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("permissions");
        }
        assert!(matches!(
            store.load_current(),
            Err(AddressBookGenerationStorageError::TooLarge { .. })
        ));
    }
}
