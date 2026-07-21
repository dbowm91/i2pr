//! Permission-hardened persistence for the local router identity.
//!
//! The format is intentionally independent of Rust layout and serde. Version
//! 1 stores the two private seeds, their derived public keys, algorithm IDs,
//! fixed lengths, and a SHA-256 integrity value. It is not encrypted at rest;
//! filesystem permissions and operator backup handling are the Milestone 1
//! threat-model boundary.

#![forbid(unsafe_code)]

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use i2pr_crypto::{
    CryptoError, PRIVATE_KEY_LENGTH, ROUTER_CRYPTO_KEY_TYPE, ROUTER_SIGNING_KEY_TYPE,
    RouterIdentityBundle, TransportStaticKey, X25519PrivateKey, constant_time_eq, sha256,
};
use rand_core::TryCryptoRng;
use thiserror::Error;
use zeroize::Zeroizing;

/// The only private identity filename used by the explicit CLI lifecycle.
pub const IDENTITY_FILE_NAME: &str = "router.identity";
/// Maximum bytes read from an identity file before parsing.
pub const MAX_IDENTITY_FILE_SIZE: usize = 4096;
/// Version of the explicit private identity format.
pub const IDENTITY_FORMAT_VERSION: u16 = 2;

/// The private NTCP2 static-key and obfuscation-IV filename.
pub const NTCP2_TRANSPORT_KEY_FILE_NAME: &str = "ntcp2.static.key";
/// Maximum bytes read from the NTCP2 static-key file before parsing.
pub const MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE: usize = 4096;
/// Version of the NTCP2 static-key and IV format.
pub const NTCP2_TRANSPORT_KEY_FORMAT_VERSION: u16 = 1;

const MAGIC: &[u8; 8] = b"I2PRID\0\0";
const CHECKSUM_LENGTH: usize = 32;
const PUBLIC_KEY_LENGTH: usize = 32;
const HEADER_LENGTH: usize = 24;
const PAYLOAD_LENGTH: usize = PRIVATE_KEY_LENGTH * 4;
const IDENTITY_PADDING_LENGTH: usize = 384 - PRIVATE_KEY_LENGTH - PRIVATE_KEY_LENGTH;
const IDENTITY_FILE_LENGTH: usize =
    HEADER_LENGTH + PAYLOAD_LENGTH + IDENTITY_PADDING_LENGTH + CHECKSUM_LENGTH;
const RESERVED_HEADER: u16 = 0;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const NTCP2_MAGIC: &[u8; 8] = b"I2PRN2K\0";
const NTCP2_HEADER_LENGTH: usize = 20;
const NTCP2_PUBLIC_KEY_LENGTH: usize = 32;
const NTCP2_IV_LENGTH: usize = 16;
const NTCP2_CHECKSUM_LENGTH: usize = 32;
const NTCP2_FILE_LENGTH: usize = NTCP2_HEADER_LENGTH
    + PRIVATE_KEY_LENGTH
    + NTCP2_PUBLIC_KEY_LENGTH
    + NTCP2_IV_LENGTH
    + NTCP2_CHECKSUM_LENGTH;

/// Errors returned while creating, loading, validating, or atomically storing
/// a private router identity.
#[derive(Debug, Error)]
pub enum StorageError {
    /// A filesystem operation failed without retaining secret bytes.
    #[error("identity storage {operation} failed: {source}")]
    Io {
        /// Static filesystem operation category.
        operation: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// The target path is a symlink or another unsafe filesystem object.
    #[error("identity storage path is not a regular non-symlink path")]
    UnsafePath,
    /// The target file already exists; generation never overwrites it.
    #[error("router identity already exists")]
    AlreadyExists,
    /// A file or directory has permissions that expose identity material.
    #[error("identity storage permissions are too permissive")]
    InsecurePermissions,
    /// The file exceeds the caller-independent parser ceiling.
    #[error("identity file exceeds {maximum} bytes")]
    TooLarge {
        /// Actual or declared size.
        actual: usize,
        /// Maximum accepted size.
        maximum: usize,
    },
    /// The input ended before the explicit format was complete.
    #[error("identity file is truncated")]
    Truncated,
    /// The input contains bytes outside the exact version-1 format.
    #[error("identity file contains trailing bytes")]
    TrailingBytes,
    /// A fixed field did not match the version-1 format.
    #[error("identity file is malformed: {context}")]
    Malformed {
        /// Static field category.
        context: &'static str,
    },
    /// The file version is not supported.
    #[error("unsupported identity file version {actual}")]
    UnsupportedVersion {
        /// Version read from the file.
        actual: u16,
    },
    /// The file selected an algorithm outside the generation policy.
    #[error("unsupported identity algorithm {algorithm} for {context}")]
    UnsupportedAlgorithm {
        /// Numeric protocol algorithm identifier.
        algorithm: u16,
        /// Static field category.
        context: &'static str,
    },
    /// The checksum or derived public material did not match.
    #[error("identity file integrity check failed")]
    Integrity,
    /// The cryptographic bundle could not be reconstructed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

/// A local identity store bound to one exact file path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityStore {
    path: PathBuf,
}

impl IdentityStore {
    /// Creates a store for an exact identity path without touching the filesystem.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Creates a store using the explicit identity filename under a data directory.
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(IDENTITY_FILE_NAME))
    }

    /// Returns the configured identity path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Creates or validates the private data directory without creating identity state.
    pub fn prepare_directory(data_dir: &Path) -> Result<(), StorageError> {
        ensure_secure_directory(data_dir)
    }

    /// Saves a new identity and refuses to replace an existing file.
    pub fn save_new(&self, bundle: &RouterIdentityBundle) -> Result<(), StorageError> {
        let encoded = encode_identity(bundle)?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        ensure_secure_directory(parent)?;
        reject_existing_target(&self.path)?;

        let (temporary_path, mut temporary) = create_temporary_file(parent, "router.identity")?;
        let result = (|| {
            temporary
                .write_all(encoded.as_slice())
                .map_err(|source| storage_io("write temporary identity", source))?;
            temporary
                .sync_all()
                .map_err(|source| storage_io("sync temporary identity", source))?;
            drop(temporary);

            // A no-replace hard-link install is atomic and prevents a concurrent
            // generator from replacing the first successfully committed identity.
            // The temporary file and destination are in one directory/filesystem.
            fs::hard_link(&temporary_path, &self.path).map_err(|source| {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    StorageError::AlreadyExists
                } else {
                    storage_io("install identity", source)
                }
            })?;
            fs::remove_file(&temporary_path)
                .map_err(|source| storage_io("remove temporary identity", source))?;
            sync_directory(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    /// Compatibility spelling for the explicit create-only operation.
    pub fn save(&self, bundle: &RouterIdentityBundle) -> Result<(), StorageError> {
        self.save_new(bundle)
    }

    /// Loads and fully revalidates an existing identity file.
    pub fn load(&self) -> Result<RouterIdentityBundle, StorageError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        validate_existing_directory(parent)?;
        let metadata = fs::symlink_metadata(&self.path)
            .map_err(|source| storage_io("inspect identity", source))?;
        validate_identity_file_metadata(&metadata)?;
        let length = usize::try_from(metadata.len()).map_err(|_| StorageError::TooLarge {
            actual: usize::MAX,
            maximum: MAX_IDENTITY_FILE_SIZE,
        })?;
        if length > MAX_IDENTITY_FILE_SIZE {
            return Err(StorageError::TooLarge {
                actual: length,
                maximum: MAX_IDENTITY_FILE_SIZE,
            });
        }
        let mut file =
            File::open(&self.path).map_err(|source| storage_io("open identity", source))?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(length));
        file.read_to_end(&mut bytes)
            .map_err(|source| storage_io("read identity", source))?;
        if bytes.len() > MAX_IDENTITY_FILE_SIZE {
            return Err(StorageError::TooLarge {
                actual: bytes.len(),
                maximum: MAX_IDENTITY_FILE_SIZE,
            });
        }
        decode_identity(&bytes)
    }
}

/// Independently generated NTCP2 static key and its published obfuscation IV.
///
/// The private key is owned by the zeroizing `i2pr-crypto` wrapper. The IV is
/// public protocol material but is persisted beside the key so an immediate
/// restart cannot silently change the RouterAddress contract.
pub struct TransportStaticKeyMaterial {
    key: TransportStaticKey,
    iv: [u8; NTCP2_IV_LENGTH],
}

impl TransportStaticKeyMaterial {
    /// Generates independent static key and IV material from an injected RNG.
    pub fn generate<R: TryCryptoRng + ?Sized>(rng: &mut R) -> Result<Self, StorageError> {
        let key = X25519PrivateKey::generate(rng)?;
        let mut iv = Zeroizing::new([0_u8; NTCP2_IV_LENGTH]);
        rng.try_fill_bytes(&mut *iv)
            .map_err(|_| StorageError::Crypto(CryptoError::RandomnessUnavailable))?;
        Ok(Self { key, iv: *iv })
    }

    /// Reconstructs material for deterministic tests or an explicit migration.
    pub fn from_parts(key: TransportStaticKey, iv: [u8; NTCP2_IV_LENGTH]) -> Self {
        Self { key, iv }
    }

    /// Borrows the private static-key owner.
    pub const fn key(&self) -> &TransportStaticKey {
        &self.key
    }

    /// Borrows the published AES obfuscation IV.
    pub const fn iv(&self) -> &[u8; NTCP2_IV_LENGTH] {
        &self.iv
    }

    /// Consumes the validated record into the private key owner and public IV.
    ///
    /// This is the explicit handoff used by a runtime composition root after
    /// loading disposable state. The private key remains owned by the
    /// zeroizing wrapper; callers cannot clone it through this API.
    pub fn into_parts(self) -> (TransportStaticKey, [u8; NTCP2_IV_LENGTH]) {
        (self.key, self.iv)
    }
}

/// A create-only store for NTCP2 static key material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportStaticKeyStore {
    path: PathBuf,
}

impl TransportStaticKeyStore {
    /// Creates a store for an exact NTCP2 key path without touching disk.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Creates a store under the existing private router data directory.
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(NTCP2_TRANSPORT_KEY_FILE_NAME))
    }

    /// Returns the configured key path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Generates and atomically saves new material without replacement.
    pub fn generate_new<R: TryCryptoRng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> Result<TransportStaticKeyMaterial, StorageError> {
        let material = TransportStaticKeyMaterial::generate(rng)?;
        self.save_new(&material)?;
        Ok(material)
    }

    /// Saves material with atomic no-replace semantics.
    pub fn save_new(&self, material: &TransportStaticKeyMaterial) -> Result<(), StorageError> {
        let encoded = encode_ntcp2_transport_key(material)?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        ensure_secure_directory(parent)?;
        reject_existing_target(&self.path)?;
        let (temporary_path, mut temporary) = create_temporary_file(parent, "ntcp2.static.key")?;
        let result = (|| {
            temporary
                .write_all(encoded.as_slice())
                .map_err(|source| storage_io("write temporary NTCP2 key", source))?;
            temporary
                .sync_all()
                .map_err(|source| storage_io("sync temporary NTCP2 key", source))?;
            drop(temporary);
            fs::hard_link(&temporary_path, &self.path).map_err(|source| {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    StorageError::AlreadyExists
                } else {
                    storage_io("install NTCP2 key", source)
                }
            })?;
            fs::remove_file(&temporary_path)
                .map_err(|source| storage_io("remove temporary NTCP2 key", source))?;
            sync_directory(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    /// Loads and fully validates existing NTCP2 static material.
    pub fn load(&self) -> Result<TransportStaticKeyMaterial, StorageError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        validate_existing_directory(parent)?;
        let metadata = fs::symlink_metadata(&self.path)
            .map_err(|source| storage_io("inspect NTCP2 key", source))?;
        validate_identity_file_metadata(&metadata)?;
        let length = usize::try_from(metadata.len()).map_err(|_| StorageError::TooLarge {
            actual: usize::MAX,
            maximum: MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE,
        })?;
        if length > MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE {
            return Err(StorageError::TooLarge {
                actual: length,
                maximum: MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE,
            });
        }
        let mut file =
            File::open(&self.path).map_err(|source| storage_io("open NTCP2 key", source))?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(length));
        file.read_to_end(&mut bytes)
            .map_err(|source| storage_io("read NTCP2 key", source))?;
        if bytes.len() > MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE {
            return Err(StorageError::TooLarge {
                actual: bytes.len(),
                maximum: MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE,
            });
        }
        decode_transport_static_key(&bytes)
    }
}

/// Decodes one bounded, exact-format NTCP2 static-key record.
///
/// This pure entry point is used by the isolated fuzz harness and by callers
/// that already own the file-policy boundary. It does not log, retain, or
/// expose the decoded private key.
pub fn decode_transport_static_key(
    bytes: &[u8],
) -> Result<TransportStaticKeyMaterial, StorageError> {
    if bytes.len() > MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE {
        return Err(StorageError::TooLarge {
            actual: bytes.len(),
            maximum: MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE,
        });
    }
    decode_ntcp2_transport_key(bytes)
}

fn storage_io(operation: &'static str, source: io::Error) -> StorageError {
    StorageError::Io { operation, source }
}

fn encode_identity(bundle: &RouterIdentityBundle) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    let signing_public = bundle.identity().signing_key().as_bytes();
    let encryption_public = bundle.identity().public_key().as_bytes();
    if signing_public.len() != PUBLIC_KEY_LENGTH || encryption_public.len() != PUBLIC_KEY_LENGTH {
        return Err(StorageError::Integrity);
    }
    let padding = bundle.identity().padding();
    if padding.len() != IDENTITY_PADDING_LENGTH {
        return Err(StorageError::Integrity);
    }

    let mut bytes = Vec::with_capacity(IDENTITY_FILE_LENGTH);
    bytes.extend_from_slice(MAGIC);
    push_u16(&mut bytes, IDENTITY_FORMAT_VERSION);
    push_u16(&mut bytes, RESERVED_HEADER);
    push_u16(&mut bytes, ROUTER_SIGNING_KEY_TYPE.code());
    push_u16(&mut bytes, ROUTER_CRYPTO_KEY_TYPE.code());
    push_u16(&mut bytes, PRIVATE_KEY_LENGTH as u16);
    push_u16(&mut bytes, PRIVATE_KEY_LENGTH as u16);
    push_u16(&mut bytes, PUBLIC_KEY_LENGTH as u16);
    push_u16(&mut bytes, PUBLIC_KEY_LENGTH as u16);
    bytes.extend_from_slice(bundle.signing_key().secret_bytes());
    bytes.extend_from_slice(bundle.encryption_key().secret_bytes());
    bytes.extend_from_slice(signing_public);
    bytes.extend_from_slice(encryption_public);
    bytes.extend_from_slice(padding);
    let checksum = sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    Ok(Zeroizing::new(bytes))
}

fn decode_identity(bytes: &[u8]) -> Result<RouterIdentityBundle, StorageError> {
    if bytes.len() < IDENTITY_FILE_LENGTH {
        return Err(StorageError::Truncated);
    }
    if bytes.len() > IDENTITY_FILE_LENGTH {
        return Err(StorageError::TrailingBytes);
    }

    let mut reader = Reader::new(bytes);
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(StorageError::Malformed { context: "magic" });
    }
    let version = reader.u16()?;
    if version != IDENTITY_FORMAT_VERSION {
        return Err(StorageError::UnsupportedVersion { actual: version });
    }
    if reader.u16()? != RESERVED_HEADER {
        return Err(StorageError::Malformed {
            context: "reserved header",
        });
    }
    let signing_algorithm = reader.u16()?;
    if signing_algorithm != ROUTER_SIGNING_KEY_TYPE.code() {
        return Err(StorageError::UnsupportedAlgorithm {
            algorithm: signing_algorithm,
            context: "signing key",
        });
    }
    let encryption_algorithm = reader.u16()?;
    if encryption_algorithm != ROUTER_CRYPTO_KEY_TYPE.code() {
        return Err(StorageError::UnsupportedAlgorithm {
            algorithm: encryption_algorithm,
            context: "encryption key",
        });
    }
    for context in [
        "signing private length",
        "encryption private length",
        "signing public length",
        "encryption public length",
    ] {
        if reader.u16()? != PRIVATE_KEY_LENGTH as u16 {
            return Err(StorageError::Malformed { context });
        }
    }

    let signing_private = reader.array::<PRIVATE_KEY_LENGTH>()?;
    let encryption_private = reader.array::<PRIVATE_KEY_LENGTH>()?;
    let signing_public = reader.array::<PUBLIC_KEY_LENGTH>()?;
    let encryption_public = reader.array::<PUBLIC_KEY_LENGTH>()?;
    let padding = Zeroizing::new(reader.take(IDENTITY_PADDING_LENGTH)?.to_vec());
    let stored_checksum = reader.array::<CHECKSUM_LENGTH>()?;
    reader.finish()?;

    let expected_checksum = sha256(&bytes[..IDENTITY_FILE_LENGTH - CHECKSUM_LENGTH]);
    if !constant_time_eq(&*stored_checksum, expected_checksum.as_bytes()) {
        return Err(StorageError::Integrity);
    }

    let bundle = RouterIdentityBundle::from_private_bytes_with_padding(
        *signing_private,
        *encryption_private,
        padding,
    )?;
    if !constant_time_eq(bundle.identity().signing_key().as_bytes(), &*signing_public)
        || !constant_time_eq(
            bundle.identity().public_key().as_bytes(),
            &*encryption_public,
        )
    {
        return Err(StorageError::Integrity);
    }
    Ok(bundle)
}

fn encode_ntcp2_transport_key(
    material: &TransportStaticKeyMaterial,
) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    let public = material.key().public_bytes();
    let mut bytes = Vec::with_capacity(NTCP2_FILE_LENGTH);
    bytes.extend_from_slice(NTCP2_MAGIC);
    push_u16(&mut bytes, NTCP2_TRANSPORT_KEY_FORMAT_VERSION);
    push_u16(&mut bytes, RESERVED_HEADER);
    push_u16(&mut bytes, ROUTER_CRYPTO_KEY_TYPE.code());
    push_u16(&mut bytes, PRIVATE_KEY_LENGTH as u16);
    push_u16(&mut bytes, NTCP2_PUBLIC_KEY_LENGTH as u16);
    push_u16(&mut bytes, NTCP2_IV_LENGTH as u16);
    bytes.extend_from_slice(material.key().secret_bytes());
    bytes.extend_from_slice(&public);
    bytes.extend_from_slice(material.iv());
    let checksum = sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    if bytes.len() != NTCP2_FILE_LENGTH {
        return Err(StorageError::Malformed {
            context: "NTCP2 encoded length",
        });
    }
    Ok(Zeroizing::new(bytes))
}

fn decode_ntcp2_transport_key(bytes: &[u8]) -> Result<TransportStaticKeyMaterial, StorageError> {
    if bytes.len() < NTCP2_FILE_LENGTH {
        return Err(StorageError::Truncated);
    }
    if bytes.len() > NTCP2_FILE_LENGTH {
        return Err(StorageError::TrailingBytes);
    }
    let mut reader = Reader::new(bytes);
    if reader.take(NTCP2_MAGIC.len())? != NTCP2_MAGIC {
        return Err(StorageError::Malformed {
            context: "NTCP2 magic",
        });
    }
    let version = reader.u16()?;
    if version != NTCP2_TRANSPORT_KEY_FORMAT_VERSION {
        return Err(StorageError::UnsupportedVersion { actual: version });
    }
    if reader.u16()? != RESERVED_HEADER {
        return Err(StorageError::Malformed {
            context: "NTCP2 reserved header",
        });
    }
    let algorithm = reader.u16()?;
    if algorithm != ROUTER_CRYPTO_KEY_TYPE.code() {
        return Err(StorageError::UnsupportedAlgorithm {
            algorithm,
            context: "NTCP2 static key",
        });
    }
    if reader.u16()? != PRIVATE_KEY_LENGTH as u16 {
        return Err(StorageError::Malformed {
            context: "NTCP2 private key length",
        });
    }
    if reader.u16()? != NTCP2_PUBLIC_KEY_LENGTH as u16 {
        return Err(StorageError::Malformed {
            context: "NTCP2 public key length",
        });
    }
    if reader.u16()? != NTCP2_IV_LENGTH as u16 {
        return Err(StorageError::Malformed {
            context: "NTCP2 IV length",
        });
    }
    let private = reader.array::<PRIVATE_KEY_LENGTH>()?;
    let public = reader.array::<NTCP2_PUBLIC_KEY_LENGTH>()?;
    let iv = reader.array::<NTCP2_IV_LENGTH>()?;
    let stored_checksum = reader.array::<NTCP2_CHECKSUM_LENGTH>()?;
    reader.finish()?;
    let expected_checksum = sha256(&bytes[..NTCP2_FILE_LENGTH - NTCP2_CHECKSUM_LENGTH]);
    if !constant_time_eq(&*stored_checksum, expected_checksum.as_bytes()) {
        return Err(StorageError::Integrity);
    }
    let key = X25519PrivateKey::from_bytes(*private);
    if !constant_time_eq(&key.public_bytes(), &*public) {
        return Err(StorageError::Integrity);
    }
    Ok(TransportStaticKeyMaterial::from_parts(key, *iv))
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn reject_existing_target(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StorageError::UnsafePath),
        Ok(_) => Err(StorageError::AlreadyExists),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(storage_io("inspect identity target", source)),
    }
}

fn create_temporary_file(parent: &Path, prefix: &str) -> Result<(PathBuf, File), StorageError> {
    for _ in 0..16 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{prefix}.{:?}.{counter}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(storage_io("create temporary identity", source)),
        }
    }
    Err(storage_io(
        "choose temporary identity path",
        io::Error::new(io::ErrorKind::AlreadyExists, "temporary name collision"),
    ))
}

fn ensure_secure_directory(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StorageError::UnsafePath);
            }
            validate_directory_permissions(&metadata)
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let parent_metadata = fs::symlink_metadata(parent)
                .map_err(|source| storage_io("inspect identity directory parent", source))?;
            if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
                return Err(StorageError::UnsafePath);
            }
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(path)
                .map_err(|source| storage_io("create identity directory", source))?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|source| storage_io("inspect identity directory", source))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StorageError::UnsafePath);
            }
            validate_directory_permissions(&metadata)
        }
        Err(source) => Err(storage_io("inspect identity directory", source)),
    }
}

fn validate_existing_directory(path: &Path) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| storage_io("inspect identity directory", source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StorageError::UnsafePath);
    }
    validate_directory_permissions(&metadata)
}

fn validate_directory_permissions(metadata: &Metadata) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StorageError::InsecurePermissions);
        }
    }
    Ok(())
}

fn validate_identity_file_metadata(metadata: &Metadata) -> Result<(), StorageError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StorageError::UnsafePath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 || mode & 0o400 == 0 {
            return Err(StorageError::InsecurePermissions);
        }
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        File::open(path)
            .map_err(|source| storage_io("open identity directory for sync", source))?
            .sync_all()
            .map_err(|source| storage_io("sync identity directory", source))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], StorageError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(StorageError::Malformed { context: "length" })?;
        if end > self.input.len() {
            return Err(StorageError::Truncated);
        }
        let value = &self.input[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, StorageError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| StorageError::Truncated)?,
        ))
    }

    fn array<const N: usize>(&mut self) -> Result<Zeroizing<[u8; N]>, StorageError> {
        let mut value = Zeroizing::new([0_u8; N]);
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }

    fn finish(&self) -> Result<(), StorageError> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(StorageError::TrailingBytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;
    use std::thread;
    use tempfile::tempdir;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("test identity")
    }

    fn encoded(bundle: &RouterIdentityBundle) -> Vec<u8> {
        encode_identity(bundle).expect("encode identity").to_vec()
    }

    fn store(directory: &tempfile::TempDir) -> IdentityStore {
        let data_dir = directory.path().join("state");
        IdentityStore::prepare_directory(&data_dir).expect("private state directory");
        IdentityStore::in_data_dir(&data_dir)
    }

    fn write_fixture(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("harden fixture");
        }
    }

    #[test]
    fn save_load_round_trip_preserves_public_identity() {
        let directory = tempdir().expect("directory");
        let store = store(&directory);
        let original = bundle(1);
        store.save_new(&original).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded.identity(), original.identity());
        assert_eq!(
            loaded.signing_key().secret_bytes(),
            original.signing_key().secret_bytes()
        );
        assert_eq!(
            loaded.encryption_key().secret_bytes(),
            original.encryption_key().secret_bytes()
        );
    }

    #[test]
    fn existing_identity_is_never_replaced() {
        let directory = tempdir().expect("directory");
        let store = store(&directory);
        let original = bundle(2);
        store.save_new(&original).expect("save");
        let before = fs::read(store.path()).expect("read");
        assert!(matches!(
            store.save_new(&bundle(3)),
            Err(StorageError::AlreadyExists)
        ));
        assert_eq!(fs::read(store.path()).expect("read"), before);
    }

    #[test]
    fn truncation_at_every_boundary_is_rejected() {
        let directory = tempdir().expect("directory");
        let store = store(&directory);
        let bytes = encoded(&bundle(4));
        for end in 0..bytes.len() {
            write_fixture(store.path(), &bytes[..end]);
            assert!(store.load().is_err(), "truncated identity must fail");
        }
    }

    #[test]
    fn maximum_and_maximum_plus_one_are_bounded() {
        let directory = tempdir().expect("directory");
        let store = store(&directory);
        let bytes = encoded(&bundle(5));
        let mut maximum = vec![0_u8; MAX_IDENTITY_FILE_SIZE];
        maximum[..bytes.len()].copy_from_slice(&bytes);
        write_fixture(store.path(), &maximum);
        assert!(matches!(store.load(), Err(StorageError::TrailingBytes)));
        write_fixture(store.path(), &vec![0_u8; MAX_IDENTITY_FILE_SIZE + 1]);
        assert!(matches!(store.load(), Err(StorageError::TooLarge { .. })));
    }

    #[test]
    fn checksum_version_and_public_material_mutations_are_rejected() {
        let directory = tempdir().expect("directory");
        let store = store(&directory);
        let bytes = encoded(&bundle(6));

        let mut corrupt = bytes.clone();
        corrupt[HEADER_LENGTH] ^= 1;
        write_fixture(store.path(), &corrupt);
        assert!(matches!(store.load(), Err(StorageError::Integrity)));

        let mut unsupported = bytes.clone();
        unsupported[8..10].copy_from_slice(&3_u16.to_be_bytes());
        write_fixture(store.path(), &unsupported);
        assert!(matches!(
            store.load(),
            Err(StorageError::UnsupportedVersion { actual: 3 })
        ));

        let mut public_mismatch = bytes;
        public_mismatch[HEADER_LENGTH + PRIVATE_KEY_LENGTH * 2] ^= 1;
        let checksum = sha256(&public_mismatch[..IDENTITY_FILE_LENGTH - CHECKSUM_LENGTH]);
        public_mismatch[IDENTITY_FILE_LENGTH - CHECKSUM_LENGTH..]
            .copy_from_slice(checksum.as_bytes());
        write_fixture(store.path(), &public_mismatch);
        assert!(matches!(store.load(), Err(StorageError::Integrity)));
    }

    #[cfg(unix)]
    #[test]
    fn generated_permissions_are_private_and_symlinks_are_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("directory");
        let store = store(&directory);
        store.save_new(&bundle(7)).expect("save");
        let file_mode = fs::metadata(store.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
        let directory_mode = fs::metadata(store.path().parent().expect("state parent"))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode & 0o077, 0);

        let link_directory = tempdir().expect("link directory");
        let target = link_directory.path().join("target");
        fs::create_dir(&target).expect("target");
        let link = link_directory.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        assert!(matches!(
            IdentityStore::prepare_directory(&link),
            Err(StorageError::UnsafePath)
        ));

        let linked_store = IdentityStore::in_data_dir(&link);
        assert!(matches!(linked_store.load(), Err(StorageError::UnsafePath)));
    }

    #[cfg(unix)]
    #[test]
    fn new_directories_are_private_and_missing_intermediates_are_not_created() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("directory");
        let new_state = directory.path().join("new-state");
        IdentityStore::prepare_directory(&new_state).expect("create private directory");
        let mode = fs::metadata(&new_state)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);

        let permissive = directory.path().join("permissive");
        fs::create_dir(&permissive).expect("permissive directory");
        fs::set_permissions(&permissive, fs::Permissions::from_mode(0o755))
            .expect("set permissive mode");
        assert!(matches!(
            IdentityStore::prepare_directory(&permissive),
            Err(StorageError::InsecurePermissions)
        ));

        let missing_parent = directory.path().join("missing-parent");
        let nested = missing_parent.join("child");
        assert!(IdentityStore::prepare_directory(&nested).is_err());
        assert!(!missing_parent.exists());
    }

    #[test]
    fn concurrent_create_only_writes_have_one_winner() {
        let directory = tempdir().expect("directory");
        let store = store(&directory);
        let mut workers = Vec::new();
        for seed in 0..8 {
            let store = store.clone();
            workers.push(thread::spawn(move || store.save_new(&bundle(seed))));
        }
        let successes = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .filter(Result::is_ok)
            .count();
        assert_eq!(successes, 1);
        store.load().expect("winner remains loadable");
    }

    fn transport_store(directory: &tempfile::TempDir) -> TransportStaticKeyStore {
        let data_dir = directory.path().join("state");
        IdentityStore::prepare_directory(&data_dir).expect("private state directory");
        TransportStaticKeyStore::in_data_dir(&data_dir)
    }

    fn transport_material(seed: u64) -> TransportStaticKeyMaterial {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        TransportStaticKeyMaterial::generate(&mut rng).expect("transport material")
    }

    fn decode_hex_fixture(value: &str) -> Vec<u8> {
        value
            .split_whitespace()
            .flat_map(|part| part.as_bytes().chunks_exact(2))
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).expect("hex high");
                let low = (pair[1] as char).to_digit(16).expect("hex low");
                ((high << 4) | low) as u8
            })
            .collect()
    }

    #[test]
    fn ntcp2_static_key_and_iv_round_trip_without_identity_coupling() {
        let directory = tempdir().expect("directory");
        let store = transport_store(&directory);
        let material = transport_material(21);
        store.save_new(&material).expect("save transport material");
        let loaded = store.load().expect("load transport material");
        assert_eq!(loaded.key().public_bytes(), material.key().public_bytes());
        assert_eq!(loaded.iv(), material.iv());
        assert_ne!(
            store.path(),
            IdentityStore::in_data_dir(store.path().parent().expect("parent")).path()
        );
    }

    #[test]
    fn committed_ntcp2_static_key_fixture_loads_strictly() {
        let directory = tempdir().expect("directory");
        let store = transport_store(&directory);
        let bytes = decode_hex_fixture(include_str!(
            "../../../tests/fixtures/ntcp2/crypto/storage-static-key.hex"
        ));
        assert_eq!(bytes.len(), NTCP2_FILE_LENGTH);
        write_fixture(store.path(), &bytes);
        let loaded = store.load().expect("fixture load");
        assert_eq!(
            loaded.iv(),
            &[
                0x23, 0x22, 0x5e, 0xc6, 0x7a, 0x4e, 0x5d, 0x69, 0xc0, 0xb8, 0xfc, 0xb1, 0x01, 0x68,
                0x6f, 0x29,
            ]
        );
    }

    #[test]
    fn ntcp2_static_key_rejects_mutations_and_replacement() {
        let directory = tempdir().expect("directory");
        let store = transport_store(&directory);
        let material = transport_material(22);
        store.save_new(&material).expect("save transport material");
        let before = fs::read(store.path()).expect("read transport material");
        assert!(matches!(
            store.save_new(&transport_material(23)),
            Err(StorageError::AlreadyExists)
        ));
        assert_eq!(
            fs::read(store.path()).expect("read transport material"),
            before
        );

        let mut checksum = before.clone();
        checksum[NTCP2_HEADER_LENGTH] ^= 1;
        write_fixture(store.path(), &checksum);
        assert!(matches!(store.load(), Err(StorageError::Integrity)));

        let mut public_mismatch = before.clone();
        public_mismatch[NTCP2_HEADER_LENGTH + PRIVATE_KEY_LENGTH] ^= 1;
        let checksum = sha256(&public_mismatch[..NTCP2_FILE_LENGTH - NTCP2_CHECKSUM_LENGTH]);
        public_mismatch[NTCP2_FILE_LENGTH - NTCP2_CHECKSUM_LENGTH..]
            .copy_from_slice(checksum.as_bytes());
        write_fixture(store.path(), &public_mismatch);
        assert!(matches!(store.load(), Err(StorageError::Integrity)));

        let mut version = before.clone();
        version[8..10].copy_from_slice(&2_u16.to_be_bytes());
        write_fixture(store.path(), &version);
        assert!(matches!(
            store.load(),
            Err(StorageError::UnsupportedVersion { actual: 2 })
        ));

        for end in 0..before.len() {
            write_fixture(store.path(), &before[..end]);
            assert!(store.load().is_err(), "truncated transport key must fail");
        }
    }

    #[cfg(unix)]
    #[test]
    fn ntcp2_static_key_store_has_private_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("directory");
        let store = transport_store(&directory);
        store
            .save_new(&transport_material(24))
            .expect("save transport material");
        let mode = fs::metadata(store.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
