//! Plan 175 §3 persistent router-owned service destination identity storage.
//!
//! Each enabled server-tunnel service owns one stable router-owned
//! destination. Its private material (Ed25519 signing seed, X25519
//! static inbound secret, exact destination padding) is persisted
//! through this seam so the destination survives daemon restarts
//! without rotation. Service destinations are deliberately distinct
//! from the router identity:
//!
//! - the router identity (`IdentityStore`) signs the local RouterInfo;
//! - service destinations advertise the LeaseSet2 the server tunnel
//!   exposes through I2P Streaming.
//!
//! The format is independent of Rust layout and serde:
//!
//! | Region | Size | Contents |
//! | --- | ---: | --- |
//! | Magic | 8 | `I2PRSD\0\0` |
//! | Header | 16 | version, reserved, signing algorithm, crypto algorithm, reserved |
//! | Payload | 448 | Ed25519 seed, X25519 static secret, signing public key, X25519 public key, destination padding |
//! | Integrity | 32 | SHA-256 over header+payload |
//!
//! All integers are big-endian and fixed-width. Version 1 accepts
//! only Ed25519 signing (type 7) and X25519 static keys (type 4),
//! exact-length payload, reserved bits zero, exact total consumption,
//! and the public keys derived from the stored private seeds.
//!
//! Like the router identity store, the service destination store is
//! atomic and no-replace. A reload/reconcile never generates a new
//! identity merely because a component restart occurred. Corruption
//! or wrong-version files fail closed; identity rotation is never a
//! side effect of reload.

#![forbid(unsafe_code)]

use std::fs::{self, File, Metadata};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use i2pr_crypto::{
    CryptoError, PRIVATE_KEY_LENGTH, ROUTER_CRYPTO_KEY_TYPE, ROUTER_SIGNING_KEY_TYPE,
    X25519_KEY_LENGTH, constant_time_eq, sha256,
};
use rand_core::TryCryptoRng;
use thiserror::Error;
use zeroize::Zeroizing;

use super::{create_temporary_file, ensure_secure_directory, sync_directory};

/// File basename for service destinations under the per-service private directory.
pub const SERVICE_DESTINATION_FILE_NAME: &str = "destination.identity";
/// Subdirectory under the router data dir holding all service destinations.
pub const SERVICE_DESTINATIONS_SUBDIR: &str = "service_destinations";
/// Maximum bytes read from a service destination file before parsing.
pub const MAX_SERVICE_DESTINATION_FILE_SIZE: usize = 4096;
/// Version of the explicit private service destination format.
pub const SERVICE_DESTINATION_FORMAT_VERSION: u16 = 1;

const SERVICE_DESTINATION_MAGIC: &[u8; 8] = b"I2PRSD\0\0";
const SERVICE_DESTINATION_RESERVED_HEADER: u16 = 0;
const SERVICE_DESTINATION_HEADER_LENGTH: usize = 16;
const SERVICE_DESTINATION_PAYLOAD_LENGTH: usize = PRIVATE_KEY_LENGTH
    + X25519_KEY_LENGTH
    + PRIVATE_KEY_LENGTH
    + X25519_KEY_LENGTH
    + crate::IDENTITY_PADDING_LENGTH;
const SERVICE_DESTINATION_INTEGRITY_LENGTH: usize = 32;
const SERVICE_DESTINATION_FILE_LENGTH: usize = SERVICE_DESTINATION_HEADER_LENGTH
    + SERVICE_DESTINATION_PAYLOAD_LENGTH
    + SERVICE_DESTINATION_INTEGRITY_LENGTH;

/// Typed errors returned while creating, loading, validating, or atomically
/// storing a private service destination identity.
#[derive(Debug, Error)]
pub enum ServiceDestinationStorageError {
    /// A filesystem operation failed without retaining secret bytes.
    #[error("service destination storage {operation} failed: {source}")]
    Io {
        /// Static filesystem operation category.
        operation: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// The target path is a symlink or another unsafe filesystem object.
    #[error("service destination storage path is not a regular non-symlink path")]
    UnsafePath,
    /// The file already exists; generation never overwrites it.
    #[error("service destination already exists")]
    AlreadyExists,
    /// A file or directory has permissions that expose identity material.
    #[error("service destination storage permissions are too permissive")]
    InsecurePermissions,
    /// The file exceeds the caller-independent parser ceiling.
    #[error("service destination file exceeds {maximum} bytes")]
    TooLarge {
        /// Actual or declared size.
        actual: usize,
        /// Maximum accepted size.
        maximum: usize,
    },
    /// The input ended before the explicit format was complete.
    #[error("service destination file is truncated")]
    Truncated,
    /// The input contains bytes outside the exact version-1 format.
    #[error("service destination file contains trailing bytes")]
    TrailingBytes,
    /// A fixed field did not match the version-1 format.
    #[error("service destination file is malformed: {context}")]
    Malformed {
        /// Static field category.
        context: &'static str,
    },
    /// The file version is not supported.
    #[error("unsupported service destination file version {actual}")]
    UnsupportedVersion {
        /// Version read from the file.
        actual: u16,
    },
    /// The file selected an algorithm outside the generation policy.
    #[error("unsupported service destination algorithm {algorithm} for {context}")]
    UnsupportedAlgorithm {
        /// Numeric protocol algorithm identifier.
        algorithm: u16,
        /// Static field category.
        context: &'static str,
    },
    /// The checksum or derived public material did not match.
    #[error("service destination file integrity check failed")]
    Integrity,
    /// The cryptographic bundle could not be reconstructed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// The supplied service identifier is malformed.
    #[error("invalid service destination id '{value}': {reason}")]
    InvalidId {
        /// Rejected value (truncated to a bounded prefix).
        value: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
}

/// A loaded service destination identity.
pub struct ServiceDestinationRecord {
    /// Ed25519 signing seed (32 bytes).
    signing_seed: Zeroizing<[u8; PRIVATE_KEY_LENGTH]>,
    /// X25519 static inbound secret (32 bytes).
    static_secret: Zeroizing<[u8; X25519_KEY_LENGTH]>,
    /// Exact destination padding.
    padding: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for ServiceDestinationRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceDestinationRecord")
            .field("signing_seed", &"<redacted>")
            .field("static_secret", &"<redacted>")
            .field("padding", &"<redacted>")
            .finish()
    }
}

impl ServiceDestinationRecord {
    /// Borrows the Ed25519 signing seed.
    pub fn signing_seed(&self) -> &[u8; PRIVATE_KEY_LENGTH] {
        &self.signing_seed
    }

    /// Borrows the X25519 static inbound secret.
    pub fn static_secret(&self) -> &[u8; X25519_KEY_LENGTH] {
        &self.static_secret
    }

    /// Borrows the destination padding buffer.
    pub fn padding(&self) -> &[u8] {
        &self.padding
    }
}

/// One explicit on-disk service destination store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceDestinationStore {
    path: PathBuf,
}

impl ServiceDestinationStore {
    /// Creates a store for the canonical `destination.identity` file
    /// inside the private per-service directory under the router data dir.
    ///
    /// `service_id` must already be a validated service tunnel
    /// identifier (the `i2pr-service-tunnels` `ServiceTunnelId`
    /// parser owns that contract). The caller is responsible for
    /// that validation; this seam only constructs the path and rejects
    /// any value that violates the storage identifier contract.
    pub fn for_service(
        data_dir: &Path,
        service_id: &str,
    ) -> Result<Self, ServiceDestinationStorageError> {
        validate_service_id(service_id)?;
        let subdir = data_dir.join(SERVICE_DESTINATIONS_SUBDIR).join(service_id);
        Ok(Self {
            path: subdir.join(SERVICE_DESTINATION_FILE_NAME),
        })
    }

    /// Creates a store for an exact service destination path. No
    /// validation of the path is performed; callers must own the
    /// filesystem boundary.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the configured path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the parent directory the store expects.
    pub fn parent(&self) -> &Path {
        self.path.parent().unwrap_or_else(|| Path::new("."))
    }

    /// Returns `true` when a file is already present on disk.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Ensures the parent private directory exists with `0o700`
    /// permissions and validates its safety before any other
    /// operation touches it.
    pub fn prepare_directory(&self) -> Result<(), ServiceDestinationStorageError> {
        ensure_secure_directory(self.parent()).map_err(map_storage)
    }

    /// Saves a new service destination identity and refuses to
    /// replace an existing file.
    pub fn save_new(
        &self,
        record: &ServiceDestinationRecord,
    ) -> Result<(), ServiceDestinationStorageError> {
        let encoded = encode_service_destination(record)?;
        ensure_recursive_service_destination_dirs(self.parent())?;
        reject_existing_target(&self.path)?;
        let parent = self.parent();
        let (temporary_path, mut temporary) =
            create_temporary_file(parent, "destination.identity").map_err(map_storage)?;
        let result = (|| {
            temporary
                .write_all(encoded.as_slice())
                .map_err(|source| service_io("write temporary service destination", source))?;
            temporary
                .sync_all()
                .map_err(|source| service_io("sync temporary service destination", source))?;
            drop(temporary);
            fs::hard_link(&temporary_path, &self.path).map_err(|source| {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    ServiceDestinationStorageError::AlreadyExists
                } else {
                    service_io("install service destination", source)
                }
            })?;
            fs::remove_file(&temporary_path)
                .map_err(|source| service_io("remove temporary service destination", source))?;
            let _ = sync_directory(parent);
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    /// Loads and fully revalidates an existing service destination
    /// file. The returned record owns redacted secret material.
    pub fn load(&self) -> Result<ServiceDestinationRecord, ServiceDestinationStorageError> {
        let parent = self.parent();
        validate_existing_directory(parent).map_err(map_storage)?;
        let metadata = fs::symlink_metadata(&self.path)
            .map_err(|source| service_io("inspect service destination", source))?;
        validate_service_destination_metadata(&metadata)?;
        let length = usize::try_from(metadata.len()).map_err(|_| {
            ServiceDestinationStorageError::TooLarge {
                actual: usize::MAX,
                maximum: MAX_SERVICE_DESTINATION_FILE_SIZE,
            }
        })?;
        if length > MAX_SERVICE_DESTINATION_FILE_SIZE {
            return Err(ServiceDestinationStorageError::TooLarge {
                actual: length,
                maximum: MAX_SERVICE_DESTINATION_FILE_SIZE,
            });
        }
        let mut file = File::open(&self.path)
            .map_err(|source| service_io("open service destination", source))?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(length));
        file.read_to_end(&mut bytes)
            .map_err(|source| service_io("read service destination", source))?;
        if bytes.len() > MAX_SERVICE_DESTINATION_FILE_SIZE {
            return Err(ServiceDestinationStorageError::TooLarge {
                actual: bytes.len(),
                maximum: MAX_SERVICE_DESTINATION_FILE_SIZE,
            });
        }
        decode_service_destination(&bytes)
    }

    /// Generates fresh OS-CSPRNG-backed material, atomically persists
    /// it through [`Self::save_new`], and returns the freshly
    /// created record. The caller owns the returned material; the
    /// store never retains secret copies.
    pub fn generate_new<R: TryCryptoRng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> Result<ServiceDestinationRecord, ServiceDestinationStorageError> {
        let record = generate_record(rng)?;
        self.save_new(&record)?;
        Ok(record)
    }
}

fn validate_service_id(value: &str) -> Result<(), ServiceDestinationStorageError> {
    if value.is_empty() {
        return Err(ServiceDestinationStorageError::InvalidId {
            value: String::new(),
            reason: "must not be empty",
        });
    }
    if value.len() > 64 {
        return Err(ServiceDestinationStorageError::InvalidId {
            value: value.to_owned(),
            reason: "exceeds the service id ceiling",
        });
    }
    if value.bytes().any(|b| b == 0 || b <= 0x20 || b == 0x7f) {
        return Err(ServiceDestinationStorageError::InvalidId {
            value: value.to_owned(),
            reason: "must not contain NUL, control, or whitespace",
        });
    }
    if value.contains('/') || value.contains('\\') || value.contains('.') {
        return Err(ServiceDestinationStorageError::InvalidId {
            value: value.to_owned(),
            reason: "must not contain path separators or dots",
        });
    }
    let first = value.as_bytes()[0];
    if !first.is_ascii_alphanumeric() {
        return Err(ServiceDestinationStorageError::InvalidId {
            value: value.to_owned(),
            reason: "must start with an alphanumeric byte",
        });
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(ServiceDestinationStorageError::InvalidId {
            value: value.to_owned(),
            reason: "must use a-z0-9, hyphen, or underscore",
        });
    }
    Ok(())
}

fn generate_record<R: TryCryptoRng + ?Sized>(
    rng: &mut R,
) -> Result<ServiceDestinationRecord, ServiceDestinationStorageError> {
    let mut signing_seed = Zeroizing::new([0_u8; PRIVATE_KEY_LENGTH]);
    let mut static_secret = Zeroizing::new([0_u8; X25519_KEY_LENGTH]);
    if rng.try_fill_bytes(&mut *signing_seed).is_err()
        || rng.try_fill_bytes(&mut *static_secret).is_err()
    {
        return Err(ServiceDestinationStorageError::Crypto(
            CryptoError::RandomnessUnavailable,
        ));
    }
    let mut padding = Zeroizing::new(vec![0_u8; crate::IDENTITY_PADDING_LENGTH]);
    if rng.try_fill_bytes(&mut padding).is_err() {
        return Err(ServiceDestinationStorageError::Crypto(
            CryptoError::RandomnessUnavailable,
        ));
    }
    Ok(ServiceDestinationRecord {
        signing_seed,
        static_secret,
        padding,
    })
}

fn encode_service_destination(
    record: &ServiceDestinationRecord,
) -> Result<Zeroizing<Vec<u8>>, ServiceDestinationStorageError> {
    if record.padding().len() != crate::IDENTITY_PADDING_LENGTH {
        return Err(ServiceDestinationStorageError::Malformed {
            context: "padding length",
        });
    }
    // Derive public keys for the stored integrity hash so any
    // corruption in the private seeds is detectable without ever
    // returning the public material outside the decoder.
    let signing_public = derive_signing_public(record.signing_seed())?;
    let encryption_public = derive_x25519_public(record.static_secret())?;
    let mut bytes = Vec::with_capacity(SERVICE_DESTINATION_FILE_LENGTH);
    bytes.extend_from_slice(SERVICE_DESTINATION_MAGIC);
    push_u16(&mut bytes, SERVICE_DESTINATION_FORMAT_VERSION);
    push_u16(&mut bytes, SERVICE_DESTINATION_RESERVED_HEADER);
    push_u16(&mut bytes, ROUTER_SIGNING_KEY_TYPE.code());
    push_u16(&mut bytes, ROUTER_CRYPTO_KEY_TYPE.code());
    bytes.extend_from_slice(record.signing_seed().as_ref());
    bytes.extend_from_slice(record.static_secret().as_ref());
    bytes.extend_from_slice(&signing_public);
    bytes.extend_from_slice(&encryption_public);
    bytes.extend_from_slice(record.padding().as_ref());
    let checksum = sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    if bytes.len() != SERVICE_DESTINATION_FILE_LENGTH {
        return Err(ServiceDestinationStorageError::Malformed {
            context: "service destination encoded length",
        });
    }
    Ok(Zeroizing::new(bytes))
}

fn decode_service_destination(
    bytes: &[u8],
) -> Result<ServiceDestinationRecord, ServiceDestinationStorageError> {
    if bytes.len() < SERVICE_DESTINATION_FILE_LENGTH {
        return Err(ServiceDestinationStorageError::Truncated);
    }
    if bytes.len() > SERVICE_DESTINATION_FILE_LENGTH {
        return Err(ServiceDestinationStorageError::TrailingBytes);
    }
    let mut reader = Reader::new(bytes);
    if reader.take(SERVICE_DESTINATION_MAGIC.len())? != SERVICE_DESTINATION_MAGIC {
        return Err(ServiceDestinationStorageError::Malformed { context: "magic" });
    }
    let version = reader.u16()?;
    if version != SERVICE_DESTINATION_FORMAT_VERSION {
        return Err(ServiceDestinationStorageError::UnsupportedVersion { actual: version });
    }
    if reader.u16()? != SERVICE_DESTINATION_RESERVED_HEADER {
        return Err(ServiceDestinationStorageError::Malformed {
            context: "reserved header",
        });
    }
    let signing_algorithm = reader.u16()?;
    if signing_algorithm != ROUTER_SIGNING_KEY_TYPE.code() {
        return Err(ServiceDestinationStorageError::UnsupportedAlgorithm {
            algorithm: signing_algorithm,
            context: "signing key",
        });
    }
    let encryption_algorithm = reader.u16()?;
    if encryption_algorithm != ROUTER_CRYPTO_KEY_TYPE.code() {
        return Err(ServiceDestinationStorageError::UnsupportedAlgorithm {
            algorithm: encryption_algorithm,
            context: "static key",
        });
    }
    let signing_seed_bytes = reader.array::<PRIVATE_KEY_LENGTH>()?;
    let static_secret_bytes = reader.array::<X25519_KEY_LENGTH>()?;
    let signing_public_bytes = reader.array::<PRIVATE_KEY_LENGTH>()?;
    let encryption_public_bytes = reader.array::<X25519_KEY_LENGTH>()?;
    let padding_bytes = reader.take(crate::IDENTITY_PADDING_LENGTH)?;
    let stored_checksum = reader.array::<SERVICE_DESTINATION_INTEGRITY_LENGTH>()?;
    reader.finish()?;
    let expected_checksum =
        sha256(&bytes[..SERVICE_DESTINATION_FILE_LENGTH - SERVICE_DESTINATION_INTEGRITY_LENGTH]);
    if !constant_time_eq(&*stored_checksum, expected_checksum.as_bytes()) {
        return Err(ServiceDestinationStorageError::Integrity);
    }
    let derived_signing = derive_signing_public(&signing_seed_bytes)?;
    let derived_x25519 = derive_x25519_public(&static_secret_bytes)?;
    if !constant_time_eq(&*signing_public_bytes, &derived_signing)
        || !constant_time_eq(&*encryption_public_bytes, &derived_x25519)
    {
        return Err(ServiceDestinationStorageError::Integrity);
    }
    let mut padding = Zeroizing::new(vec![0_u8; padding_bytes.len()]);
    padding.copy_from_slice(padding_bytes);
    Ok(ServiceDestinationRecord {
        signing_seed: signing_seed_bytes,
        static_secret: static_secret_bytes,
        padding,
    })
}

fn derive_signing_public(
    seed: &[u8; PRIVATE_KEY_LENGTH],
) -> Result<[u8; PRIVATE_KEY_LENGTH], ServiceDestinationStorageError> {
    use i2pr_crypto::SigningPrivateKey;
    let key = SigningPrivateKey::from_bytes(*seed);
    let public = key.public_key()?;
    let bytes = public.as_bytes();
    let mut out = [0_u8; PRIVATE_KEY_LENGTH];
    if bytes.len() != PRIVATE_KEY_LENGTH {
        return Err(ServiceDestinationStorageError::Malformed {
            context: "derived signing public length",
        });
    }
    out.copy_from_slice(bytes);
    Ok(out)
}

fn derive_x25519_public(
    secret: &[u8; X25519_KEY_LENGTH],
) -> Result<[u8; X25519_KEY_LENGTH], ServiceDestinationStorageError> {
    use i2pr_crypto::X25519PrivateKey;
    let key = X25519PrivateKey::from_bytes(*secret);
    Ok(key.public_bytes())
}

fn service_io(operation: &'static str, source: io::Error) -> ServiceDestinationStorageError {
    ServiceDestinationStorageError::Io { operation, source }
}

fn ensure_recursive_service_destination_dirs(
    path: &Path,
) -> Result<(), ServiceDestinationStorageError> {
    let mut current = Some(path.to_path_buf());
    let mut ancestors = Vec::new();
    while let Some(dir) = current {
        match fs::symlink_metadata(&dir) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(ServiceDestinationStorageError::UnsafePath);
                }
                break;
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                ancestors.push(dir.clone());
                current = dir.parent().map(Path::to_path_buf);
            }
            Err(source) => return Err(service_io("inspect service destination parent", source)),
        }
    }
    for dir in ancestors.iter().rev() {
        create_secure_directory(dir)?;
    }
    Ok(())
}

fn create_secure_directory(path: &Path) -> Result<(), ServiceDestinationStorageError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let parent_metadata = fs::symlink_metadata(parent)
                .map_err(|source| service_io("inspect service destination parent", source))?;
            if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
                return Err(ServiceDestinationStorageError::UnsafePath);
            }
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(path)
                .map_err(|source| service_io("create service destination directory", source))?;
            fs::symlink_metadata(path)
                .map_err(|source| service_io("inspect service destination directory", source))?
        }
        Err(source) => return Err(service_io("inspect service destination directory", source)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ServiceDestinationStorageError::UnsafePath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(ServiceDestinationStorageError::InsecurePermissions);
        }
    }
    Ok(())
}

fn map_storage(error: super::StorageError) -> ServiceDestinationStorageError {
    match error {
        super::StorageError::Io { operation, source } => {
            ServiceDestinationStorageError::Io { operation, source }
        }
        super::StorageError::UnsafePath => ServiceDestinationStorageError::UnsafePath,
        super::StorageError::InsecurePermissions => {
            ServiceDestinationStorageError::InsecurePermissions
        }
        super::StorageError::TooLarge { actual, maximum } => {
            ServiceDestinationStorageError::TooLarge { actual, maximum }
        }
        other => ServiceDestinationStorageError::Io {
            operation: "service destination storage",
            source: io::Error::other(other.to_string()),
        },
    }
}

fn reject_existing_target(path: &Path) -> Result<(), ServiceDestinationStorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ServiceDestinationStorageError::UnsafePath)
        }
        Ok(_) => Err(ServiceDestinationStorageError::AlreadyExists),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(service_io("inspect service destination target", source)),
    }
}

fn validate_existing_directory(path: &Path) -> Result<(), super::StorageError> {
    super::validate_existing_directory(path)
}

fn validate_service_destination_metadata(
    metadata: &Metadata,
) -> Result<(), ServiceDestinationStorageError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ServiceDestinationStorageError::UnsafePath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 || mode & 0o400 == 0 {
            return Err(ServiceDestinationStorageError::InsecurePermissions);
        }
    }
    Ok(())
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ServiceDestinationStorageError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ServiceDestinationStorageError::Malformed { context: "length" })?;
        if end > self.input.len() {
            return Err(ServiceDestinationStorageError::Truncated);
        }
        let value = &self.input[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, ServiceDestinationStorageError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| ServiceDestinationStorageError::Truncated)?,
        ))
    }

    fn array<const N: usize>(
        &mut self,
    ) -> Result<Zeroizing<[u8; N]>, ServiceDestinationStorageError> {
        let mut value = Zeroizing::new([0_u8; N]);
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }

    fn finish(&self) -> Result<(), ServiceDestinationStorageError> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(ServiceDestinationStorageError::TrailingBytes)
        }
    }
}

/// Pure entry point used by the storage-correctness test lane and by
/// future callers that already own the filesystem boundary.
pub fn decode_service_destination_bytes(
    bytes: &[u8],
) -> Result<ServiceDestinationRecord, ServiceDestinationStorageError> {
    decode_service_destination(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;
    use tempfile::tempdir;

    fn record(seed: u64) -> ServiceDestinationRecord {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        generate_record(&mut rng).expect("record")
    }

    fn store(dir: &Path, id: &str) -> ServiceDestinationStore {
        ServiceDestinationStore::for_service(dir, id).expect("store")
    }

    #[test]
    fn save_load_round_trip_preserves_secrets() {
        let directory = tempdir().expect("directory");
        let data_dir = directory.path();
        let s = store(data_dir, "alpha");
        let original = record(1);
        s.save_new(&original).expect("save");
        let loaded = s.load().expect("load");
        assert_eq!(
            loaded.signing_seed().as_ref(),
            original.signing_seed().as_ref()
        );
        assert_eq!(
            loaded.static_secret().as_ref(),
            original.static_secret().as_ref()
        );
        assert_eq!(loaded.padding().len(), original.padding().len());
        assert_eq!(loaded.padding(), original.padding());
    }

    #[test]
    fn existing_destination_is_never_replaced() {
        let directory = tempdir().expect("directory");
        let s = store(directory.path(), "alpha");
        let original = record(2);
        s.save_new(&original).expect("save");
        let before = fs::read(s.path()).expect("read");
        assert!(matches!(
            s.save_new(&record(3)),
            Err(ServiceDestinationStorageError::AlreadyExists)
        ));
        assert_eq!(fs::read(s.path()).expect("read"), before);
    }

    #[test]
    fn truncate_at_every_boundary_is_rejected() {
        let directory = tempdir().expect("directory");
        let s = store(directory.path(), "alpha");
        let original = record(4);
        s.save_new(&original).expect("save");
        let bytes = fs::read(s.path()).expect("read");
        for end in 0..bytes.len() {
            let tmp = directory.path().join(format!("truncated-{end}"));
            #[cfg(unix)]
            {
                use std::fs::OpenOptions;
                use std::os::unix::fs::OpenOptionsExt;
                let mut opts = OpenOptions::new();
                opts.write(true).create_new(true).mode(0o600);
                let mut f = opts.open(&tmp).expect("open tmp");
                f.write_all(&bytes[..end]).expect("write tmp");
                f.sync_all().ok();
                drop(f);
            }
            #[cfg(not(unix))]
            {
                fs::write(&tmp, &bytes[..end]).expect("write tmp");
            }
            let truncated_store = ServiceDestinationStore::new(&tmp);
            assert!(
                truncated_store.load().is_err(),
                "truncated service destination must fail at end {end}"
            );
        }
    }

    #[test]
    fn checksum_version_and_public_material_mutations_are_rejected() {
        let directory = tempdir().expect("directory");
        // Storage test pre-condition: tighten the tempdir to 0o700 so
        // the per-directory permission check (inherited from the
        // router identity store) is satisfied by our scratch space.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
                .expect("tighten tempdir");
        }
        let s = store(directory.path(), "alpha");
        let original = record(5);
        s.save_new(&original).expect("save");
        let bytes = fs::read(s.path()).expect("read");

        let mut corrupt = bytes.clone();
        corrupt[SERVICE_DESTINATION_HEADER_LENGTH] ^= 1;
        write_raw(&directory.path().join("corrupt"), &corrupt);
        assert!(matches!(
            ServiceDestinationStore::new(directory.path().join("corrupt")).load(),
            Err(ServiceDestinationStorageError::Integrity)
        ));

        let mut unsupported = bytes.clone();
        unsupported[8..10].copy_from_slice(&3_u16.to_be_bytes());
        write_raw(&directory.path().join("unsupported-version"), &unsupported);
        assert!(matches!(
            ServiceDestinationStore::new(directory.path().join("unsupported-version")).load(),
            Err(ServiceDestinationStorageError::UnsupportedVersion { actual: 3 })
        ));

        let mut public_mismatch = bytes.clone();
        // Flip a byte in the embedded signing public key region.
        let public_offset =
            SERVICE_DESTINATION_HEADER_LENGTH + PRIVATE_KEY_LENGTH + X25519_KEY_LENGTH;
        public_mismatch[public_offset] ^= 1;
        // Recompute the integrity over the changed bytes.
        let checksum = sha256(
            &public_mismatch
                [..SERVICE_DESTINATION_FILE_LENGTH - SERVICE_DESTINATION_INTEGRITY_LENGTH],
        );
        public_mismatch[SERVICE_DESTINATION_FILE_LENGTH - SERVICE_DESTINATION_INTEGRITY_LENGTH..]
            .copy_from_slice(checksum.as_bytes());
        write_raw(&directory.path().join("public-mismatch"), &public_mismatch);
        assert!(matches!(
            ServiceDestinationStore::new(directory.path().join("public-mismatch")).load(),
            Err(ServiceDestinationStorageError::Integrity)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn generated_permissions_are_private_and_symlinks_are_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("directory");
        let s = store(directory.path(), "alpha");
        s.save_new(&record(6)).expect("save");
        let file_mode = fs::metadata(s.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
        let dir_mode = fs::metadata(s.parent())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode & 0o077, 0);

        let link_directory = tempdir().expect("link directory");
        let target = link_directory.path().join("target");
        fs::create_dir(&target).expect("target dir");
        let link = link_directory.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let link_store = ServiceDestinationStore::for_service(&link, "alpha").expect("store");
        assert!(matches!(
            link_store.save_new(&record(7)),
            Err(ServiceDestinationStorageError::UnsafePath)
                | Err(ServiceDestinationStorageError::InsecurePermissions)
        ));
    }

    #[test]
    fn invalid_service_id_is_rejected() {
        let directory = tempdir().expect("directory");
        for bad in ["", "Has Space", "..", "/", "alpha.b32", "alpha/B", "UPPER"] {
            assert!(ServiceDestinationStore::for_service(directory.path(), bad).is_err());
        }
        let ok = ServiceDestinationStore::for_service(directory.path(), "alpha-1");
        assert!(ok.is_ok());
    }

    fn write_raw(path: &Path, bytes: &[u8]) {
        use std::fs::OpenOptions;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true).mode(0o600);
            let mut f = opts.open(path).expect("open");
            f.write_all(bytes).expect("write");
            f.sync_all().ok();
            drop(f);
        }
        #[cfg(not(unix))]
        {
            use std::os::unix::fs::OpenOptionsExt as _; // import-unused lint suppression
            let _ = OpenOptionsExt::mode;
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            let mut f = opts.open(path).expect("open");
            f.write_all(bytes).expect("write");
            f.sync_all().ok();
            drop(f);
        }
    }
}
