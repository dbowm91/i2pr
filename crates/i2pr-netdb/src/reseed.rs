//! SU3 / reseed verification and ingestion.
//!
//! This module owns the Plan 104 SU3/reseed pipeline. It is **pure**:
//! it never opens sockets, never touches the filesystem, never logs.
//! It consumes raw bytes, validates the SU3 container structure,
//! verifies the SU3 signature against an explicitly configured signer
//! trust set, processes the inner ZIP archive under explicit limits,
//! validates every RouterInfo through Plan 103, and returns a typed
//! report.
//!
//! Failure policy:
//!
//! - SU3 trust failure, malformed archive structure, or aggregate
//!   archive limit failure: zero records accepted, zero NetDB state
//!   mutated.
//! - An individual RouterInfo failure inside an otherwise authentic
//!   archive: rejected record counted, archive scan continues, no
//!   in-memory store mutation.
//!
//! The composition owner (`i2pr-netdb-persist`) translates the typed
//! reports into `RouterInfoStore::insert` calls.

use std::fmt;

use i2pr_proto::{MAX_COMMON_STRUCTURE_SIZE, RouterInfo};
use thiserror::Error;
use x509_parser::prelude::*;
use x509_parser::public_key::PublicKey;

use crate::router_info::{ValidatedRouterInfo, ValidationContext, router_hash};

/// The Plan 104 single-file SU3 ceiling. The current reseed SU3 bundle
/// is comfortably below 1 MiB; 8 MiB is the largest size we will
/// accept without explicit operator override.
pub const MAX_SU3_BYTES: usize = 8 * 1024 * 1024;

/// The Plan 104 maximum number of ZIP entries inside a single SU3
/// archive.
pub const MAX_ARCHIVE_ENTRIES: usize = 4096;

/// The Plan 104 maximum cumulative uncompressed RouterInfo bytes
/// accepted from a single archive.
pub const MAX_ARCHIVE_ROUTERINFO_BYTES: u64 = 32 * 1024 * 1024;

/// The Plan 104 maximum uncompressed bytes accepted for any single ZIP
/// entry.
pub const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 256 * 1024;

/// The Plan 104 maximum cumulative uncompressed bytes across the whole
/// archive.
pub const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;

/// The Plan 104 ceiling for signer identifier length (UTF-8 bytes).
pub const MAX_SIGNER_ID_LEN: usize = 256;

/// The Plan 104 ceiling for the SU3 version field length.
pub const MAX_VERSION_LEN: usize = 16;

/// The Plan 104 expected SU3 file type for reseed bundles.
pub const RESEED_FILE_TYPE: u8 = 2; // ZIP

/// The Plan 104 expected SU3 content type for reseed bundles.
pub const RESEED_CONTENT_TYPE: u8 = 0; // RESEED

/// The Plan 104 supported signature types.
///
/// The reseed protocol uses RSA-SHA512-4096 (signature type 6).
/// Verification is delegated to a reviewed RSA implementation; this
/// crate does not implement RSA primitives itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReseedSignatureType {
    /// RSA over SHA-512 with a 4096-bit modulus; signature type 6.
    RsaSha512_4096,
}

impl ReseedSignatureType {
    /// Returns the I2P signature-type wire code.
    pub const fn wire_code(self) -> u16 {
        match self {
            ReseedSignatureType::RsaSha512_4096 => 6,
        }
    }

    /// Returns the signature length in bytes for the supplied key
    /// length in bits.
    pub const fn signature_len(self, key_bits: usize) -> usize {
        match self {
            ReseedSignatureType::RsaSha512_4096 => key_bits.div_ceil(8),
        }
    }
}

/// The Plan 104 signer trust entry.
///
/// The signer identifier is the human-readable string that appears in
/// the SU3 header. The certificate bytes are the DER-encoded X.509
/// certificate carried out-of-band by the operator. Validity is enforced
/// from the parsed `NotBefore` / `NotAfter` fields.
#[derive(Clone, Debug)]
pub struct ReseedSignerId(String);

impl ReseedSignerId {
    /// Constructs a signer identifier after validating its length.
    pub fn new(value: &str) -> Result<Self, ReseedTrustError> {
        if value.is_empty() {
            return Err(ReseedTrustError::EmptySignerId);
        }
        if value.len() > MAX_SIGNER_ID_LEN {
            return Err(ReseedTrustError::SignerIdTooLong {
                actual: value.len(),
                maximum: MAX_SIGNER_ID_LEN,
            });
        }
        if value.as_bytes().contains(&0) {
            return Err(ReseedTrustError::SignerIdContainsNull);
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the signer identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReseedSignerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for ReseedSignerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// One parsed signer trust entry. The DER bytes are retained so the
/// caller can also persist them for review; only the parsed public key
/// is used for verification.
#[derive(Clone)]
pub struct TrustedSigner {
    /// The signer identifier matched against SU3 header bytes.
    pub signer_id: ReseedSignerId,
    /// The DER-encoded X.509 certificate carrying the trusted key.
    pub certificate_der: Vec<u8>,
    /// The permitted signature type. Only this algorithm may be used.
    pub signature_type: ReseedSignatureType,
    /// The parsed RSA public key modulus.
    pub modulus: Vec<u8>,
    /// The parsed RSA public key exponent.
    pub exponent: Vec<u8>,
    /// Validity interval start (Unix seconds).
    pub not_before: u64,
    /// Validity interval end (Unix seconds).
    pub not_after: u64,
}

impl fmt::Debug for TrustedSigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedSigner")
            .field("signer_id", &self.signer_id)
            .field("signature_type", &self.signature_type)
            .field("modulus_len", &self.modulus.len())
            .field("exponent_len", &self.exponent.len())
            .field("not_before", &self.not_before)
            .field("not_after", &self.not_after)
            .finish_non_exhaustive()
    }
}

/// Errors emitted while parsing or trusting SU3 signer material.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReseedTrustError {
    /// The signer identifier was the empty string.
    #[error("reseed signer identifier is empty")]
    EmptySignerId,
    /// The signer identifier exceeded `MAX_SIGNER_ID_LEN` bytes.
    #[error("reseed signer identifier length {actual} exceeds {maximum}")]
    SignerIdTooLong {
        /// Actual length.
        actual: usize,
        /// Maximum length.
        maximum: usize,
    },
    /// The signer identifier contained a NUL byte.
    #[error("reseed signer identifier contains a NUL byte")]
    SignerIdContainsNull,
    /// The supplied DER bytes could not be parsed as an X.509
    /// certificate.
    #[error("reseed signer certificate is not parseable as DER X.509")]
    CertificateParse,
    /// The certificate key type is not RSA.
    #[error("reseed signer key type {algorithm_oid} is not RSA")]
    UnsupportedKeyType {
        /// OID string of the algorithm.
        algorithm_oid: String,
    },
    /// The supplied certificate is not currently valid.
    #[error("reseed signer certificate is outside its validity interval {not_before}..{not_after}")]
    CertificateNotValid {
        /// Validity interval start.
        not_before: u64,
        /// Validity interval end.
        not_after: u64,
    },
}

/// Errors emitted while parsing an SU3 bundle.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReseedParseError {
    /// The SU3 magic header was not present.
    #[error("input does not start with the SU3 magic bytes")]
    MagicMismatch,
    /// The SU3 format version is outside the supported set.
    #[error("SU3 format version {actual} is not supported")]
    UnsupportedFormatVersion {
        /// Actual version read from the SU3 header.
        actual: u8,
    },
    /// A reserved SU3 field was not zero.
    #[error("SU3 reserved field at offset {offset} was non-zero")]
    NonZeroReserved {
        /// Byte offset of the offending field.
        offset: usize,
    },
    /// The SU3 signature length did not match the signer key length.
    #[error("SU3 signature length {actual} does not match the expected {expected}")]
    SignatureLengthMismatch {
        /// Actual signature length.
        actual: usize,
        /// Expected signature length.
        expected: usize,
    },
    /// A length field overflowed the file or the `MAX_SU3_BYTES` cap.
    #[error("SU3 length field {field} {actual} exceeds {maximum}")]
    LengthExceeded {
        /// Name of the offending length field.
        field: &'static str,
        /// Actual declared length.
        actual: u64,
        /// Maximum accepted length.
        maximum: u64,
    },
    /// The total SU3 size declared by the length fields exceeded the
    /// remaining bytes in the input.
    #[error("SU3 input truncated: needed {needed}, remaining {remaining}")]
    Truncated {
        /// Number of bytes needed.
        needed: u64,
        /// Number of bytes remaining in the input.
        remaining: u64,
    },
    /// The signer identifier could not be decoded as UTF-8.
    #[error("SU3 signer identifier is not valid UTF-8")]
    InvalidSignerId,
    /// The version field could not be decoded as UTF-8.
    #[error("SU3 version field is not valid UTF-8")]
    InvalidVersion,
    /// The signature type is not in the Plan 104 allowlist.
    #[error("SU3 signature type {actual} is not supported")]
    UnsupportedSignatureType {
        /// Actual signature type.
        actual: u16,
    },
    /// The file type is not `RESEED_FILE_TYPE` (ZIP).
    #[error("SU3 file type {actual} is not the reseed ZIP type")]
    UnsupportedFileType {
        /// Actual file type.
        actual: u8,
    },
    /// The content type is not `RESEED_CONTENT_TYPE` (RESEED).
    #[error("SU3 content type {actual} is not the RESEED type")]
    UnsupportedContentType {
        /// Actual content type.
        actual: u8,
    },
    /// The archive contained more entries than `MAX_ARCHIVE_ENTRIES`.
    #[error("SU3 archive contains more than {maximum} entries")]
    ArchiveEntriesExceeded {
        /// Maximum entries accepted.
        maximum: usize,
    },
    /// The archive consumed more uncompressed bytes than the bound.
    #[error("SU3 archive cumulative uncompressed bytes exceed {maximum}")]
    ArchiveUncompressedBytesExceeded {
        /// Maximum accepted.
        maximum: u64,
    },
    /// The archive consumed more RouterInfo bytes than the bound.
    #[error("SU3 archive cumulative RouterInfo bytes exceed {maximum}")]
    ArchiveRouterInfoBytesExceeded {
        /// Maximum accepted.
        maximum: u64,
    },
    /// A single archive entry exceeded `MAX_ENTRY_UNCOMPRESSED_BYTES`.
    #[error("SU3 archive entry {name} exceeds {maximum} uncompressed bytes")]
    EntryTooLarge {
        /// Entry filename.
        name: String,
        /// Maximum accepted.
        maximum: u64,
    },
    /// An archive entry used an unsupported compression method.
    #[error("SU3 archive entry {name} uses unsupported compression method {method}")]
    UnsupportedCompressionMethod {
        /// Entry filename.
        name: String,
        /// Compression method id.
        method: u16,
    },
    /// An archive entry contained path separators or absolute paths.
    #[error("SU3 archive entry {name} has an invalid path")]
    InvalidPath {
        /// Entry filename.
        name: String,
    },
    /// Duplicate archive entry names.
    #[error("SU3 archive contains duplicate entry {name}")]
    DuplicateEntry {
        /// Duplicate entry name.
        name: String,
    },
    /// The input bytes could not be decoded as a ZIP archive.
    #[error("SU3 archive could not be decoded as a ZIP file")]
    ZipDecode,
    /// The SU3 signature did not verify.
    #[error("SU3 signature verification failed")]
    SignatureInvalid,
    /// The signer identifier is not in the configured trust set.
    #[error("SU3 signer identifier is not in the configured trust set")]
    UnknownSigner,
    /// The signer certificate was outside its validity interval at the
    /// supplied verification time.
    #[error("SU3 signer certificate is outside its validity interval at the supplied now")]
    CertificateExpired,
    /// The expected signed bytes region length is inconsistent.
    #[error("SU3 signed byte region length does not match signature length")]
    SignedRegionMismatch,
}

/// Errors emitted while validating RouterInfo entries inside a verified
/// SU3 archive. Currently surfaced only as `String` reports; retained
/// for future typed-state expansion.
#[derive(Debug, Error, Eq, PartialEq)]
#[allow(dead_code)]
pub enum ReseedEntryError {
    /// The entry filename did not contain an I2P Base64 router hash
    /// prefix.
    #[error("reseed entry {name} does not carry a recognizable router hash prefix")]
    InvalidFilename {
        /// Entry filename.
        name: String,
    },
    /// The decoded filename hash did not match the contained
    /// RouterInfo identity.
    #[error("reseed entry {name} router hash does not match the contained RouterIdentity")]
    RouterHashMismatch {
        /// Entry filename.
        name: String,
        /// Expected hash extracted from the filename.
        expected: String,
        /// Actual hash derived from the RouterIdentity.
        actual: String,
    },
    /// The contained RouterInfo failed to decode or validate.
    #[error("reseed entry {name} router info failed validation: {context}")]
    RouterInfoInvalid {
        /// Entry filename.
        name: String,
        /// Static failure category.
        context: &'static str,
    },
}

/// Tunable limits for the SU3/reseed pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReseedLimits {
    /// Maximum SU3 file size in bytes.
    pub max_su3_bytes: usize,
    /// Maximum archive entry count.
    pub max_archive_entries: usize,
    /// Maximum cumulative uncompressed bytes.
    pub max_archive_uncompressed_bytes: u64,
    /// Maximum cumulative RouterInfo bytes.
    pub max_archive_router_info_bytes: u64,
    /// Maximum per-entry uncompressed bytes.
    pub max_entry_uncompressed_bytes: u64,
    /// Maximum single RouterInfo encoded bytes (passed through to the
    /// validator).
    pub max_router_info_encoded_bytes: usize,
}

impl Default for ReseedLimits {
    fn default() -> Self {
        Self {
            max_su3_bytes: MAX_SU3_BYTES,
            max_archive_entries: MAX_ARCHIVE_ENTRIES,
            max_archive_uncompressed_bytes: MAX_ARCHIVE_UNCOMPRESSED_BYTES,
            max_archive_router_info_bytes: MAX_ARCHIVE_ROUTERINFO_BYTES,
            max_entry_uncompressed_bytes: MAX_ENTRY_UNCOMPRESSED_BYTES,
            max_router_info_encoded_bytes: MAX_COMMON_STRUCTURE_SIZE,
        }
    }
}

/// Per-entry outcome reported by the SU3/reseed pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReseedEntryState {
    /// The entry was accepted and produced a `ValidatedRouterInfo`.
    Accepted,
    /// The entry filename failed to parse or match its identity.
    RejectedFilename,
    /// The entry bytes failed to decode as a RouterInfo.
    RejectedDecode,
    /// The decoded RouterInfo failed Plan 103 validation.
    RejectedValidation,
}

/// Per-entry report produced by the SU3/reseed pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReseedEntryReport {
    /// The entry filename as recorded in the archive.
    pub name: String,
    /// The outcome for this entry.
    pub state: ReseedEntryState,
    /// Optional error detail for typed failures.
    pub error: Option<String>,
}

/// Typed outcome of `verify_su3` / `verify_su3_archive`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReseedVerifyOutcome {
    /// The bundle verified and yielded one or more validated records.
    Accepted {
        /// Number of accepted entries.
        accepted: usize,
    },
    /// The bundle could not be trusted (magic, signer, signature,
    /// certificate validity, or aggregate archive limit).
    RejectedTrust {
        /// Typed failure category.
        reason: &'static str,
    },
}

/// Full SU3 verification report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReseedVerifyReport {
    /// Overall outcome.
    pub outcome: ReseedVerifyOutcome,
    /// Per-entry report (only present for `Accepted` or `RejectedArchive`).
    pub entries: Vec<ReseedEntryReport>,
    /// The accepted `ValidatedRouterInfo` records. The composition
    /// owner passes each through the normal `RouterInfoStore::insert`
    /// path.
    pub accepted: Vec<ValidatedRouterInfo>,
}

/// Parsed SU3 header and signed bytes region. Internal-only; used to
/// drive the verifier without exposing mutable references to the input.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ParsedSu3 {
    content_length: usize,
    signature_type: ReseedSignatureType,
    content_type: u8,
    file_type: u8,
    signer_id: ReseedSignerId,
    version: String,
    /// Offset of the start of the signed content (i.e. the first byte
    /// after the SU3 header).
    content_offset: usize,
    /// Offset of the start of the signature.
    signature_offset: usize,
    /// Total SU3 length including the trailing signature.
    total_length: usize,
}

/// Parses an SU3 header from `input`.
///
/// Returns the parsed header plus the byte range of the signed content
/// and signature. The caller owns the bounds-checked input.
fn parse_su3_header(input: &[u8]) -> Result<ParsedSu3, ReseedParseError> {
    // SU3 header layout (little-endian for the length fields):
    //
    //  magic[6] = "I2Psu3"
    //  format_version: u8
    //  reserved1: u8 (must be 0)
    //  reserved2: u8 (must be 0)
    //  reserved3: u8 (must be 0)
    //  signature_type: u16
    //  signature_length: u16
    //  content_length: u32 (little-endian)
    //  file_type: u8
    //  content_type: u8
    //  reserved4: u8 (must be 0)
    //  reserved5: u8 (must be 0)
    //  reserved6: u8 (must be 0)
    //  version_length: u16
    //  version[version_length]
    //  signer_id_length: u16
    //  signer_id[signer_id_length]
    //  content[content_length]
    //  signature[signature_length]
    const HEADER_PREFIX: usize = 6 + 1 + 1 + 1 + 1 + 2 + 2 + 4 + 1 + 1 + 1 + 1 + 1 + 2;
    const MAGIC: &[u8; 6] = b"I2Psu3";
    if input.len() < HEADER_PREFIX {
        return Err(ReseedParseError::Truncated {
            needed: HEADER_PREFIX as u64,
            remaining: input.len() as u64,
        });
    }
    if &input[..6] != MAGIC {
        return Err(ReseedParseError::MagicMismatch);
    }
    let format_version = input[6];
    if format_version != 1 {
        return Err(ReseedParseError::UnsupportedFormatVersion {
            actual: format_version,
        });
    }
    for offset in [7_usize, 8, 9] {
        if input[offset] != 0 {
            return Err(ReseedParseError::NonZeroReserved { offset });
        }
    }
    let signature_type_code = read_u16_le(&input[10..12]);
    let signature_length = read_u16_le(&input[12..14]) as usize;
    let content_length = read_u32_le(&input[14..18]) as u64;
    let file_type = input[18];
    let content_type = input[19];
    for offset in [20_usize, 21, 22] {
        if input[offset] != 0 {
            return Err(ReseedParseError::NonZeroReserved { offset });
        }
    }
    let version_length = read_u16_le(&input[23..25]) as usize;

    if file_type != RESEED_FILE_TYPE {
        return Err(ReseedParseError::UnsupportedFileType { actual: file_type });
    }
    if content_type != RESEED_CONTENT_TYPE {
        return Err(ReseedParseError::UnsupportedContentType {
            actual: content_type,
        });
    }

    let signature_type = match signature_type_code {
        6 => ReseedSignatureType::RsaSha512_4096,
        other => {
            return Err(ReseedParseError::UnsupportedSignatureType { actual: other });
        }
    };

    if content_length > MAX_SU3_BYTES as u64 {
        return Err(ReseedParseError::LengthExceeded {
            field: "content",
            actual: content_length,
            maximum: MAX_SU3_BYTES as u64,
        });
    }
    if signature_length > MAX_SU3_BYTES {
        return Err(ReseedParseError::LengthExceeded {
            field: "signature",
            actual: signature_length as u64,
            maximum: MAX_SU3_BYTES as u64,
        });
    }

    let mut offset = HEADER_PREFIX;
    offset = offset
        .checked_add(version_length)
        .ok_or(ReseedParseError::Truncated {
            needed: u64::MAX,
            remaining: input.len() as u64,
        })?;
    if offset > input.len() {
        return Err(ReseedParseError::Truncated {
            needed: offset as u64,
            remaining: input.len() as u64,
        });
    }
    let version_bytes = &input[HEADER_PREFIX..offset];
    let version = std::str::from_utf8(version_bytes)
        .map_err(|_| ReseedParseError::InvalidVersion)?
        .to_owned();
    if version.is_empty() || version.len() > MAX_VERSION_LEN {
        return Err(ReseedParseError::InvalidVersion);
    }

    let signer_length_offset = offset;
    if offset + 2 > input.len() {
        return Err(ReseedParseError::Truncated {
            needed: (offset + 2) as u64,
            remaining: input.len() as u64,
        });
    }
    let signer_id_length =
        read_u16_le(&input[signer_length_offset..signer_length_offset + 2]) as usize;
    if signer_id_length == 0 || signer_id_length > MAX_SIGNER_ID_LEN {
        return Err(ReseedParseError::InvalidSignerId);
    }
    offset = offset
        .checked_add(2 + signer_id_length)
        .ok_or(ReseedParseError::Truncated {
            needed: u64::MAX,
            remaining: input.len() as u64,
        })?;
    if offset > input.len() {
        return Err(ReseedParseError::Truncated {
            needed: offset as u64,
            remaining: input.len() as u64,
        });
    }
    let signer_id_bytes = &input[signer_length_offset + 2..offset];
    let signer_id_str =
        std::str::from_utf8(signer_id_bytes).map_err(|_| ReseedParseError::InvalidSignerId)?;
    let signer_id =
        ReseedSignerId::new(signer_id_str).map_err(|_| ReseedParseError::InvalidSignerId)?;

    let content_offset = offset;
    let content_end =
        content_offset
            .checked_add(content_length as usize)
            .ok_or(ReseedParseError::Truncated {
                needed: u64::MAX,
                remaining: input.len() as u64,
            })?;
    if content_end > input.len() {
        return Err(ReseedParseError::Truncated {
            needed: content_end as u64,
            remaining: input.len() as u64,
        });
    }
    let signature_offset = content_end;
    let total_length =
        signature_offset
            .checked_add(signature_length)
            .ok_or(ReseedParseError::Truncated {
                needed: u64::MAX,
                remaining: input.len() as u64,
            })?;
    if total_length > input.len() {
        return Err(ReseedParseError::Truncated {
            needed: total_length as u64,
            remaining: input.len() as u64,
        });
    }

    Ok(ParsedSu3 {
        content_length: content_length as usize,
        signature_type,
        content_type,
        file_type,
        signer_id,
        version,
        content_offset,
        signature_offset,
        total_length,
    })
}

fn read_u16_le(bytes: &[u8]) -> u16 {
    let mut value = 0_u16;
    for (index, byte) in bytes.iter().take(2).enumerate() {
        value |= u16::from(*byte) << (8 * index);
    }
    value
}

fn read_u32_le(bytes: &[u8]) -> u32 {
    let mut value = 0_u32;
    for (index, byte) in bytes.iter().take(4).enumerate() {
        value |= u32::from(*byte) << (8 * index);
    }
    value
}

/// Parses an SU3 bundle without verifying its signature. Returns the
/// header plus the inner archive bytes. Intended for callers that
/// already obtained verified bytes through a separate trust path, for
/// test harnesses, and for diagnostic inspection of untrusted input.
///
/// The function performs every structural check the full verifier does
/// except for the cryptographic signature check.
pub fn parse_su3(input: &[u8]) -> Result<ParsedSu3, ReseedParseError> {
    parse_su3_header(input)
}

/// Result of an SU3 verification that returned valid records. The
/// composition owner passes each through the normal store API.
#[derive(Clone, Debug)]
pub struct ReseedVerifiedBundle {
    /// Validated RouterInfo records ready for store insertion.
    pub validated: Vec<ValidatedRouterInfo>,
    /// Per-entry outcome report.
    pub entries: Vec<ReseedEntryReport>,
}

/// A trust set for reseed SU3 signers.
///
/// The set maps signer identifiers to their parsed certificates and
/// validity intervals. Verification requires the signer identifier in
/// the SU3 header to match a configured entry, the certificate to be
/// currently valid, and the signature type/algorithm to match.
#[derive(Clone, Debug, Default)]
pub struct ReseedSignerTrustSet {
    entries: Vec<TrustedSigner>,
}

impl ReseedSignerTrustSet {
    /// Constructs an empty trust set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a parsed signer to the trust set. Duplicate signer
    /// identifiers overwrite the prior entry; this preserves the
    /// "later entry wins" semantic used by operator overrides.
    pub fn add(&mut self, signer: TrustedSigner) {
        self.entries
            .retain(|existing| existing.signer_id.0 != signer.signer_id.0);
        self.entries.push(signer);
    }

    /// Returns the number of trusted signers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the trust set is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up the trust entry for `signer_id`, returning the parsed
    /// public key and validity interval.
    pub fn lookup(&self, signer_id: &ReseedSignerId) -> Option<&TrustedSigner> {
        self.entries
            .iter()
            .find(|entry| entry.signer_id.0 == signer_id.0)
    }
}

/// Parses a DER-encoded X.509 certificate into a `TrustedSigner`.
///
/// The certificate must carry an RSA public key, and the supplied
/// `not_before` / `not_after` values must fall inside the certificate
/// validity interval; the verifier cross-checks against these values at
/// verification time.
#[allow(dead_code)]
pub fn trust_signer_from_certificate(
    signer_id: ReseedSignerId,
    certificate_der: Vec<u8>,
    signature_type: ReseedSignatureType,
    not_before: u64,
    not_after: u64,
) -> Result<TrustedSigner, ReseedTrustError> {
    let (_, certificate) = X509Certificate::from_der(&certificate_der)
        .map_err(|_| ReseedTrustError::CertificateParse)?;
    let (spki, cert_not_before_i, cert_not_after_i) = {
        let tbs = &certificate.tbs_certificate;
        (
            &tbs.subject_pki,
            certificate.validity.not_before.timestamp(),
            certificate.validity.not_after.timestamp(),
        )
    };
    let algorithm_oid = spki.algorithm.algorithm.to_id_string();
    let public_key = match spki.parsed() {
        Ok(key) => key,
        Err(_) => {
            return Err(ReseedTrustError::UnsupportedKeyType { algorithm_oid });
        }
    };
    let rsa_pubkey = match public_key {
        PublicKey::RSA(rsa) => rsa,
        _ => {
            return Err(ReseedTrustError::UnsupportedKeyType { algorithm_oid });
        }
    };
    let modulus = rsa_pubkey.modulus.to_vec();
    let exponent = rsa_pubkey.exponent.to_vec();
    let cert_not_before = u64::try_from(cert_not_before_i).unwrap_or(0);
    let cert_not_after = u64::try_from(cert_not_after_i).unwrap_or(0);
    if not_before < cert_not_before || not_after > cert_not_after {
        return Err(ReseedTrustError::CertificateNotValid {
            not_before: cert_not_before,
            not_after: cert_not_after,
        });
    }
    Ok(TrustedSigner {
        signer_id,
        certificate_der,
        signature_type,
        modulus,
        exponent,
        not_before,
        not_after,
    })
}

/// Verifies the SU3 signature of `input` against the configured trust
/// set at the supplied verification time.
///
/// The function performs the full Plan 104 §5 signature verification:
/// SU3 header parsing, signer trust lookup, RSA-SHA512-4096 signature
/// verification over the exact signed bytes region (header bytes
/// through end of content), and an archive decode pass that returns the
/// validated `RouterInfoStore` records.
pub fn verify_su3_with_signers(
    input: &[u8],
    trust: &ReseedSignerTrustSet,
    now_seconds: u64,
    limits: ReseedLimits,
    validation_context: ValidationContext,
) -> Result<ReseedVerifyReport, ReseedParseError> {
    let parsed = parse_su3_header(input)?;
    let signed = &input[..parsed.signature_offset];
    let signature = &input[parsed.signature_offset..parsed.total_length];
    let trusted = match trust.lookup(&parsed.signer_id) {
        Some(trusted) => trusted,
        None => {
            return Ok(ReseedVerifyReport {
                outcome: ReseedVerifyOutcome::RejectedTrust {
                    reason: "unknown_signer",
                },
                entries: Vec::new(),
                accepted: Vec::new(),
            });
        }
    };
    if trusted.signature_type != parsed.signature_type {
        return Err(ReseedParseError::UnsupportedSignatureType {
            actual: trusted.signature_type.wire_code(),
        });
    }
    if now_seconds < trusted.not_before || now_seconds > trusted.not_after {
        return Ok(ReseedVerifyReport {
            outcome: ReseedVerifyOutcome::RejectedTrust {
                reason: "certificate_expired",
            },
            entries: Vec::new(),
            accepted: Vec::new(),
        });
    }
    let expected_signature_len = trusted
        .signature_type
        .signature_len(trusted.modulus.len() * 8);
    if signature.len() != expected_signature_len {
        return Err(ReseedParseError::SignatureLengthMismatch {
            actual: signature.len(),
            expected: expected_signature_len,
        });
    }
    verify_rsa_sha512_signature(signed, signature, trusted)
        .map_err(|_| ReseedParseError::SignatureInvalid)?;
    verify_su3_archive(
        &input[parsed.content_offset..parsed.content_offset + parsed.content_length],
        limits,
        validation_context,
    )
}

/// Verifies the SU3 signature without a configured trust set. The
/// caller must supply the parsed trust entry directly.
pub fn verify_su3(
    input: &[u8],
    signer: &TrustedSigner,
    now_seconds: u64,
    limits: ReseedLimits,
    validation_context: ValidationContext,
) -> Result<ReseedVerifyReport, ReseedParseError> {
    let parsed = parse_su3_header(input)?;
    let signed = &input[..parsed.signature_offset];
    let signature = &input[parsed.signature_offset..parsed.total_length];
    if signer.signature_type != parsed.signature_type {
        return Err(ReseedParseError::UnsupportedSignatureType {
            actual: signer.signature_type.wire_code(),
        });
    }
    if now_seconds < signer.not_before || now_seconds > signer.not_after {
        return Ok(ReseedVerifyReport {
            outcome: ReseedVerifyOutcome::RejectedTrust {
                reason: "certificate_expired",
            },
            entries: Vec::new(),
            accepted: Vec::new(),
        });
    }
    let expected_signature_len = signer
        .signature_type
        .signature_len(signer.modulus.len() * 8);
    if signature.len() != expected_signature_len {
        return Err(ReseedParseError::SignatureLengthMismatch {
            actual: signature.len(),
            expected: expected_signature_len,
        });
    }
    if signer.signer_id.0 != parsed.signer_id.0 {
        return Ok(ReseedVerifyReport {
            outcome: ReseedVerifyOutcome::RejectedTrust {
                reason: "unknown_signer",
            },
            entries: Vec::new(),
            accepted: Vec::new(),
        });
    }
    verify_rsa_sha512_signature(signed, signature, signer)
        .map_err(|_| ReseedParseError::SignatureInvalid)?;
    verify_su3_archive(
        &input[parsed.content_offset..parsed.content_offset + parsed.content_length],
        limits,
        validation_context,
    )
}

fn verify_rsa_sha512_signature(
    signed: &[u8],
    signature: &[u8],
    signer: &TrustedSigner,
) -> Result<(), ReseedParseError> {
    use sad_rsa::pkcs1v15::{Signature, VerifyingKey};
    use sad_rsa::sha2::Sha512;
    use sad_rsa::signature::Verifier;

    // sad-rsa's `RsaPublicKey::new` accepts `BoxedUint` (a
    // fixed-precision heap integer from `crypto-bigint`) rather than
    // `num-bigint::BigUint`. Convert with the bit-precision the modulus
    // bytes already encode.
    let modulus_bits = (signer.modulus.len() as u32).checked_mul(8).unwrap_or(0);
    let exponent_bits = (signer.exponent.len() as u32).checked_mul(8).unwrap_or(0);
    let n = match sad_rsa::BoxedUint::from_be_slice(&signer.modulus, modulus_bits) {
        Ok(n) => n,
        Err(_) => return Err(ReseedParseError::SignatureInvalid),
    };
    let e = match sad_rsa::BoxedUint::from_be_slice(&signer.exponent, exponent_bits) {
        Ok(e) => e,
        Err(_) => return Err(ReseedParseError::SignatureInvalid),
    };
    let key = match sad_rsa::RsaPublicKey::new(n, e) {
        Ok(key) => key,
        Err(_) => return Err(ReseedParseError::SignatureInvalid),
    };
    let verifying_key = VerifyingKey::<Sha512>::new(key);
    let signature_obj = match Signature::try_from(signature) {
        Ok(s) => s,
        Err(_) => return Err(ReseedParseError::SignatureInvalid),
    };
    if verifying_key.verify(signed, &signature_obj).is_err() {
        return Err(ReseedParseError::SignatureInvalid);
    }
    Ok(())
}

/// Verifies the inner ZIP archive against the Plan 104 limits and
/// validates every RouterInfo through Plan 103.
///
/// The function never mutates external state: the composition owner
/// owns the insertion into the in-memory store. Aggregate limits stop
/// the scan deterministically and produce a typed rejection.
pub fn verify_su3_archive(
    archive: &[u8],
    limits: ReseedLimits,
    validation_context: ValidationContext,
) -> Result<ReseedVerifyReport, ReseedParseError> {
    if archive.len() > limits.max_su3_bytes {
        return Err(ReseedParseError::LengthExceeded {
            field: "archive",
            actual: archive.len() as u64,
            maximum: limits.max_su3_bytes as u64,
        });
    }
    let cursor = std::io::Cursor::new(archive);
    let mut zip = match zip::ZipArchive::new(cursor) {
        Ok(zip) => zip,
        Err(_) => return Err(ReseedParseError::ZipDecode),
    };
    let entry_count = zip.len();
    if entry_count > limits.max_archive_entries {
        return Err(ReseedParseError::ArchiveEntriesExceeded {
            maximum: limits.max_archive_entries,
        });
    }
    let mut entries: Vec<ReseedEntryReport> = Vec::with_capacity(entry_count);
    let mut accepted: Vec<ValidatedRouterInfo> = Vec::new();
    let mut cumulative_uncompressed: u64 = 0;
    let mut cumulative_router_info: u64 = 0;
    let mut seen_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for index in 0..entry_count {
        let mut file = zip
            .by_index(index)
            .map_err(|_| ReseedParseError::ZipDecode)?;
        let raw_name = file.name().to_owned();
        if raw_name.contains('/') || raw_name.contains('\\') || raw_name.contains(':') {
            return Err(ReseedParseError::InvalidPath { name: raw_name });
        }
        if !seen_names.insert(raw_name.clone()) {
            return Err(ReseedParseError::DuplicateEntry { name: raw_name });
        }
        if file.is_dir() {
            // Plan 104 rejects archive directories outright.
            return Err(ReseedParseError::InvalidPath { name: raw_name });
        }
        let method = file.compression();
        if !matches!(
            method,
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            // `CompressionMethod` is a non-unit enum so it cannot be
            // cast directly; the `to_u16` method (deprecated for
            // future-proofing) remains the documented way to recover
            // the wire method code for unknown variants.
            #[allow(deprecated)]
            let method_code = method.to_u16();
            return Err(ReseedParseError::UnsupportedCompressionMethod {
                name: raw_name,
                method: method_code,
            });
        }
        let declared = file.size();
        if declared > limits.max_entry_uncompressed_bytes {
            return Err(ReseedParseError::EntryTooLarge {
                name: raw_name,
                maximum: limits.max_entry_uncompressed_bytes,
            });
        }
        let mut bytes = Vec::with_capacity(declared as usize);
        use std::io::Read;
        if file.read_to_end(&mut bytes).is_err() {
            return Err(ReseedParseError::ZipDecode);
        }
        if bytes.len() as u64 > limits.max_entry_uncompressed_bytes {
            return Err(ReseedParseError::EntryTooLarge {
                name: raw_name,
                maximum: limits.max_entry_uncompressed_bytes,
            });
        }
        cumulative_uncompressed = cumulative_uncompressed.saturating_add(bytes.len() as u64);
        if cumulative_uncompressed > limits.max_archive_uncompressed_bytes {
            return Err(ReseedParseError::ArchiveUncompressedBytesExceeded {
                maximum: limits.max_archive_uncompressed_bytes,
            });
        }
        let report = validate_ri_entry(&raw_name, &bytes, validation_context, limits);
        match report.state {
            ReseedEntryState::Accepted => {
                cumulative_router_info = cumulative_router_info.saturating_add(bytes.len() as u64);
                if cumulative_router_info > limits.max_archive_router_info_bytes {
                    return Err(ReseedParseError::ArchiveRouterInfoBytesExceeded {
                        maximum: limits.max_archive_router_info_bytes,
                    });
                }
                let validated = report_validated(&raw_name, &bytes, validation_context)?;
                entries.push(ReseedEntryReport {
                    name: raw_name.clone(),
                    state: ReseedEntryState::Accepted,
                    error: None,
                });
                accepted.push(validated);
            }
            _ => entries.push(report),
        }
    }
    Ok(ReseedVerifyReport {
        outcome: ReseedVerifyOutcome::Accepted {
            accepted: accepted.len(),
        },
        entries,
        accepted,
    })
}

fn report_validated(
    raw_name: &str,
    bytes: &[u8],
    context: ValidationContext,
) -> Result<ValidatedRouterInfo, ReseedParseError> {
    // Decode + validate and re-extract the ValidatedRouterInfo. This is
    // a small double-pass cost on the success path; the negative path
    // never reaches here.
    let info = RouterInfo::decode(bytes, MAX_COMMON_STRUCTURE_SIZE)
        .map_err(|_| ReseedParseError::ZipDecode)?;
    let expected = router_info_filename_hash(raw_name).ok_or(ReseedParseError::Truncated {
        needed: 0,
        remaining: 0,
    })?;
    let validated = ValidatedRouterInfo::from_router_info(info, Some(expected), context)
        .map_err(|_| ReseedParseError::SignedRegionMismatch)?;
    Ok(validated)
}

fn validate_ri_entry(
    name: &str,
    bytes: &[u8],
    context: ValidationContext,
    _limits: ReseedLimits,
) -> ReseedEntryReport {
    // Filename must encode the I2P Base64 hash of the contained
    // RouterIdentity; we accept the standard 52-char encoding (32-byte
    // SHA-256 hash).
    let expected_hash = match router_info_filename_hash(name) {
        Some(hash) => hash,
        None => {
            return ReseedEntryReport {
                name: name.to_owned(),
                state: ReseedEntryState::RejectedFilename,
                error: Some("filename does not encode a router hash".to_owned()),
            };
        }
    };
    let info = match RouterInfo::decode(bytes, MAX_COMMON_STRUCTURE_SIZE) {
        Ok(info) => info,
        Err(_) => {
            return ReseedEntryReport {
                name: name.to_owned(),
                state: ReseedEntryState::RejectedDecode,
                error: Some("router info bytes failed to decode".to_owned()),
            };
        }
    };
    let actual_hash = match router_hash(info.router_identity()) {
        Ok(hash) => hash,
        Err(_) => {
            return ReseedEntryReport {
                name: name.to_owned(),
                state: ReseedEntryState::RejectedValidation,
                error: Some("router identity hash could not be derived".to_owned()),
            };
        }
    };
    if expected_hash != actual_hash {
        return ReseedEntryReport {
            name: name.to_owned(),
            state: ReseedEntryState::RejectedValidation,
            error: Some("filename hash does not match contained identity".to_owned()),
        };
    }
    match ValidatedRouterInfo::from_router_info(info, Some(expected_hash), context) {
        Ok(_) => ReseedEntryReport {
            name: name.to_owned(),
            state: ReseedEntryState::Accepted,
            error: None,
        },
        Err(error) => ReseedEntryReport {
            name: name.to_owned(),
            state: ReseedEntryState::RejectedValidation,
            error: Some(format!("{error}")),
        },
    }
}

/// Extracts the canonical I2P Base64 router hash from a reseed entry
/// filename.
///
/// Reseed entry filenames are conventionally `<base64hash>.b32` or
/// `<base64hash>` where the prefix is the I2P Base64 encoding of the
/// router hash. The helper accepts the base64 prefix (44 characters
/// for a 32-byte SHA-256 hash: ten 4-char groups covering 30 bytes
/// plus a 4-char final group with `~` padding for the 2-byte tail)
/// and returns the matching `RouterHash`.
pub fn router_info_filename_hash(name: &str) -> Option<crate::router_info::RouterHash> {
    let stem = name
        .strip_suffix(".b32")
        .or_else(|| name.strip_suffix(".ri"))
        .unwrap_or(name);
    // The 32-byte SHA-256 hash encodes to 44 I2P Base64 characters.
    if stem.len() < 44 {
        return None;
    }
    let prefix = &stem[..44];
    let decoded = match crate::base64::decode(prefix) {
        Ok(bytes) => bytes,
        Err(_) => return None,
    };
    if decoded.len() != 32 {
        return None;
    }
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&decoded);
    Some(crate::router_info::RouterHash::from_bytes(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping};
    use sad_rsa::pkcs1v15::{SigningKey, VerifyingKey};
    use sad_rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
    use sad_rsa::traits::PublicKeyParts;

    // sad-rsa 0.2 uses `rand_core` 0.10. Use a deterministic
    // `rand_chacha` 0.10 `ChaCha8Rng` so signer/verifier agree on
    // a stable seeded byte stream.
    use sad_rsa::rand_core as sad_rand_core;

    fn sad_rng(seed: u64) -> rand_chacha_10::ChaCha8Rng {
        use sad_rand_core::SeedableRng;
        rand_chacha_10::ChaCha8Rng::seed_from_u64(seed)
    }

    fn bundle(seed: u64) -> RouterIdentityBundle {
        use rand_core::SeedableRng;
        let mut i2rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut i2rng).expect("identity")
    }

    fn rsa_2048_pair(seed: u64) -> sad_rsa::RsaPrivateKey {
        let mut rng = sad_rng(seed);
        sad_rsa::RsaPrivateKey::new(&mut rng, 2048).expect("RSA key")
    }

    fn rsa_2048_public(seed: u64) -> (Vec<u8>, Vec<u8>) {
        let private = rsa_2048_pair(seed);
        let public = sad_rsa::RsaPublicKey::from(&private);
        let n = public.n_bytes().to_vec();
        let e = public.e_bytes().to_vec();
        (n, e)
    }

    fn sign_with_rsa_2048(seed: u64, message: &[u8]) -> Vec<u8> {
        let private = rsa_2048_pair(seed);
        let signing_key = SigningKey::<sad_rsa::sha2::Sha512>::new(private);
        let mut rng = sad_rng(seed);
        let sig: sad_rsa::pkcs1v15::Signature = signing_key.sign_with_rng(&mut rng, message);
        sig.to_bytes().to_vec()
    }

    fn verify_rsa_2048(seed: u64, message: &[u8], signature: &[u8]) -> Result<(), String> {
        let (n, e) = rsa_2048_public(seed);
        let n_bits = (n.len() as u32).checked_mul(8).unwrap_or(2048);
        let e_bits = (e.len() as u32).checked_mul(8).unwrap_or(32);
        let n_int =
            sad_rsa::BoxedUint::from_be_slice(&n, n_bits).map_err(|e| format!("modulus: {e}"))?;
        let e_int =
            sad_rsa::BoxedUint::from_be_slice(&e, e_bits).map_err(|e| format!("exponent: {e}"))?;
        let public =
            sad_rsa::RsaPublicKey::new(n_int, e_int).map_err(|e| format!("public key: {e}"))?;
        let vk = VerifyingKey::<sad_rsa::sha2::Sha512>::new(public);
        let sig = sad_rsa::pkcs1v15::Signature::try_from(signature)
            .map_err(|e| format!("signature: {e}"))?;
        vk.verify(message, &sig).map_err(|e| format!("verify: {e}"))
    }

    fn make_archive(records: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buf);
            let mut zip = zip::ZipWriter::new(cursor);
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, payload) in records {
                zip.start_file(*name, opts).expect("start file");
                std::io::Write::write_all(&mut zip, payload).expect("write payload");
            }
            zip.finish().expect("finish zip");
        }
        buf
    }

    fn router_info_fixture(seed: u64, now_ms: u64) -> (String, Vec<u8>) {
        let signer = bundle(seed);
        let info = signer
            .sign_router_info(
                Date::from_millis(now_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        let hash = crate::router_hash(info.router_identity()).expect("hash");
        let prefix = crate::base64::encode(hash.as_bytes()).expect("encode");
        let encoded = info
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        (format!("{prefix}.b32"), encoded)
    }

    fn make_su3(_archive_seed: u64, signer_seed: u64, archive: &[u8]) -> Vec<u8> {
        let signer_id_str = format!("test-signer-{signer_seed}");
        // 2048-bit RSA produces a 256-byte PKCS#1 v1.5 signature.
        let sig_len: u16 = 256;
        let mut header = Vec::new();
        header.extend_from_slice(b"I2Psu3");
        header.push(1); // format version
        header.extend_from_slice(&[0, 0, 0]); // reserved
        header.extend_from_slice(&6_u16.to_le_bytes()); // signature type
        header.extend_from_slice(&sig_len.to_le_bytes()); // signature length
        header.extend_from_slice(&(archive.len() as u32).to_le_bytes()); // content length
        header.push(RESEED_FILE_TYPE);
        header.push(RESEED_CONTENT_TYPE);
        header.extend_from_slice(&[0, 0, 0]); // reserved
        header.extend_from_slice(&1_u16.to_le_bytes()); // version length
        header.push(b'1');
        header.extend_from_slice(&(signer_id_str.len() as u16).to_le_bytes());
        header.extend_from_slice(signer_id_str.as_bytes());
        header.extend_from_slice(archive);
        let sig = sign_with_rsa_2048(signer_seed, &header);
        assert_eq!(
            sig.len(),
            sig_len as usize,
            "RSA-2048 signature must be 256 bytes"
        );
        header.extend_from_slice(&sig);
        header
    }

    #[test]
    fn parse_su3_rejects_non_magic_input() {
        // 25 bytes is the minimum header length; pad the bad magic to
        // ensure the parser reaches the magic comparison.
        let mut bytes = b"NOT_AN_SU3_AT_ALL".to_vec();
        bytes.extend_from_slice(&[0; 8]);
        let error = parse_su3(&bytes).unwrap_err();
        assert!(matches!(error, ReseedParseError::MagicMismatch));
    }

    #[test]
    fn parse_su3_rejects_unsupported_format_version() {
        // Build a minimal valid header with format version 2.
        let mut bytes = b"I2Psu3".to_vec();
        bytes.push(2); // format version
        bytes.extend_from_slice(&[0, 0, 0]); // reserved
        bytes.extend_from_slice(&6_u16.to_le_bytes()); // signature type
        bytes.extend_from_slice(&512_u16.to_le_bytes()); // signature length
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // content length
        bytes.push(RESEED_FILE_TYPE);
        bytes.push(RESEED_CONTENT_TYPE);
        bytes.extend_from_slice(&[0, 0, 0]); // reserved
        bytes.extend_from_slice(&0_u16.to_le_bytes()); // version length
        bytes.extend_from_slice(&0_u16.to_le_bytes()); // signer length
        let error = parse_su3(&bytes).unwrap_err();
        assert!(matches!(
            error,
            ReseedParseError::UnsupportedFormatVersion { actual: 2 }
        ));
    }

    #[test]
    fn parse_su3_rejects_wrong_file_or_content_type() {
        let mut bytes = b"I2Psu3".to_vec();
        bytes.push(1); // format version
        bytes.extend_from_slice(&[0, 0, 0]); // reserved
        bytes.extend_from_slice(&6_u16.to_le_bytes()); // signature type
        bytes.extend_from_slice(&512_u16.to_le_bytes()); // signature length
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // content length
        bytes.push(0); // wrong file type
        bytes.push(RESEED_CONTENT_TYPE);
        bytes.extend_from_slice(&[0, 0, 0]); // reserved
        bytes.extend_from_slice(&0_u16.to_le_bytes()); // version length
        bytes.extend_from_slice(&0_u16.to_le_bytes()); // signer length
        let error = parse_su3(&bytes).unwrap_err();
        assert!(matches!(
            error,
            ReseedParseError::UnsupportedFileType { actual: 0 }
        ));
    }

    #[test]
    fn router_info_filename_hash_accepts_known_prefix() {
        let hash = crate::router_info::RouterHash::from_bytes([0xAB; 32]);
        let encoded = crate::base64::encode(hash.as_bytes()).expect("encode");
        let filename = format!("{encoded}.b32");
        let parsed = router_info_filename_hash(&filename).expect("parsed");
        assert_eq!(parsed, hash);
    }

    #[test]
    fn router_info_filename_hash_rejects_short_prefix() {
        let result = router_info_filename_hash("tooshort.b32");
        assert!(result.is_none());
    }

    /// A deterministic "now" timestamp for all reseed tests so the
    /// RouterInfo publication dates and validation-context timestamps
    /// stay within the Plan 103 freshness window.
    const TEST_NOW_MS: u64 = 1_700_000_000_000; // 2023-11-14T22:13:20Z

    #[test]
    fn su3_end_to_end_validates_and_ingests() {
        // Build a deterministic 2048-bit RSA test signer.
        let signer_seed = 0xAA01_u64;
        let signer_id_str = format!("test-signer-{signer_seed}");

        // Build two valid RouterInfo records. Both are published at
        // TEST_NOW_MS so the validator accepts them.
        let (filename1, encoded1) = router_info_fixture(0xBB01, TEST_NOW_MS);
        let (filename2, encoded2) = router_info_fixture(0xBB02, TEST_NOW_MS);

        // Build the ZIP archive.
        let archive = make_archive(&[(&filename1, &encoded1[..]), (&filename2, &encoded2[..])]);

        // Build the SU3 bundle with RSA-2048 signature.
        let su3 = make_su3(0xCC01, signer_seed, &archive);

        // Parse the SU3 header.
        let parsed = parse_su3(&su3).expect("parse");
        assert_eq!(parsed.signature_type, ReseedSignatureType::RsaSha512_4096);
        assert_eq!(parsed.signer_id.as_str(), signer_id_str);

        // Verify the RSA signature over the signed region.
        let signed_region = &su3[..parsed.signature_offset];
        let signature = &su3[parsed.signature_offset..parsed.total_length];
        verify_rsa_2048(signer_seed, signed_region, signature).expect("RSA signature must verify");

        // Set up a trust set with the signer.
        let mut trust = ReseedSignerTrustSet::new();
        let (modulus, exponent) = rsa_2048_public(signer_seed);
        trust.add(TrustedSigner {
            signer_id: ReseedSignerId::new(&signer_id_str).expect("id"),
            certificate_der: Vec::new(),
            signature_type: ReseedSignatureType::RsaSha512_4096,
            modulus,
            exponent,
            not_before: 1_000_000_000,
            not_after: 2_000_000_000,
        });

        // Verify the SU3 bundle through the full trust + signature path.
        let report = verify_su3_with_signers(
            &su3,
            &trust,
            TEST_NOW_MS / 1000,
            ReseedLimits::default(),
            ValidationContext::new(Date::from_millis(TEST_NOW_MS)),
        )
        .expect("verify_su3_with_signers");

        match &report.outcome {
            ReseedVerifyOutcome::Accepted { accepted } => {
                assert_eq!(*accepted, 2, "both RouterInfos must be accepted");
            }
            other => panic!("expected Accepted, got: {other:?}"),
        }

        // Verify that the accepted records can be inserted into a store.
        let mut store = crate::store::RouterInfoStore::default();
        for validated in &report.accepted {
            let outcome = store.insert(validated.clone());
            assert!(
                matches!(outcome, crate::store::InsertOutcome::Inserted),
                "expected Inserted, got: {outcome:?}"
            );
        }
        assert_eq!(store.len(), 2);

        // Verify that an expired signer is rejected.
        let expired_report = verify_su3_with_signers(
            &su3,
            &trust,
            3_000_000_000, // after not_after
            ReseedLimits::default(),
            ValidationContext::new(Date::from_millis(TEST_NOW_MS)),
        )
        .expect("verify with expired signer");
        assert!(matches!(
            expired_report.outcome,
            ReseedVerifyOutcome::RejectedTrust {
                reason: "certificate_expired"
            }
        ));

        // Verify that an unknown signer is rejected.
        let empty_trust = ReseedSignerTrustSet::new();
        let unknown_report = verify_su3_with_signers(
            &su3,
            &empty_trust,
            TEST_NOW_MS / 1000,
            ReseedLimits::default(),
            ValidationContext::new(Date::from_millis(TEST_NOW_MS)),
        )
        .expect("verify with unknown signer");
        assert!(matches!(
            unknown_report.outcome,
            ReseedVerifyOutcome::RejectedTrust {
                reason: "unknown_signer"
            }
        ));
    }

    #[test]
    fn su3_rejects_tampered_content() {
        let signer_seed = 0xAA02_u64;
        let (filename1, encoded1) = router_info_fixture(0xBB03, TEST_NOW_MS);
        let archive = make_archive(&[(&filename1, &encoded1[..])]);
        let mut su3 = make_su3(0xCC02, signer_seed, &archive);

        // Tamper with one byte in the content region (after the
        // header, before the signature). The verification must reject
        // this tampered bundle.
        let parsed = parse_su3(&su3).expect("parse");
        let tamper_offset = parsed.content_offset + 1;
        su3[tamper_offset] ^= 0xFF;

        let mut trust = ReseedSignerTrustSet::new();
        let (modulus, exponent) = rsa_2048_public(signer_seed);
        trust.add(TrustedSigner {
            signer_id: ReseedSignerId::new(&format!("test-signer-{signer_seed}")).expect("id"),
            certificate_der: Vec::new(),
            signature_type: ReseedSignatureType::RsaSha512_4096,
            modulus,
            exponent,
            not_before: 1_000_000_000,
            not_after: 2_000_000_000,
        });

        let result = verify_su3_with_signers(
            &su3,
            &trust,
            TEST_NOW_MS / 1000,
            ReseedLimits::default(),
            ValidationContext::new(Date::from_millis(TEST_NOW_MS)),
        );
        assert!(result.is_err(), "tampered SU3 must not verify");
    }

    #[test]
    fn su3_rejects_archive_with_invalid_router_info() {
        let signer_seed = 0xAA03_u64;
        // Build a valid RouterInfo. The good_filename is derived from
        // the RouterIdentity hash via I2P Base64.
        let (_good_filename, good_encoded) = router_info_fixture(0xBB04, TEST_NOW_MS);
        // Build a wrong I2P Base64 filename. We encode a 32-byte
        // hash that is all zeros — a value that is extremely unlikely
        // to match the RouterIdentity hash of the fixture.
        let wrong_hash = [0x00_u8; 32];
        let wrong_base64 = crate::base64::encode(&wrong_hash).expect("encode wrong hash");
        let wrong_name = format!("{wrong_base64}.b32");
        let archive = make_archive(&[(&wrong_name, &good_encoded[..])]);
        let su3 = make_su3(0xCC03, signer_seed, &archive);

        let mut trust = ReseedSignerTrustSet::new();
        let (modulus, exponent) = rsa_2048_public(signer_seed);
        trust.add(TrustedSigner {
            signer_id: ReseedSignerId::new(&format!("test-signer-{signer_seed}")).expect("id"),
            certificate_der: Vec::new(),
            signature_type: ReseedSignatureType::RsaSha512_4096,
            modulus,
            exponent,
            not_before: 1_000_000_000,
            not_after: 2_000_000_000,
        });

        let report = verify_su3_with_signers(
            &su3,
            &trust,
            TEST_NOW_MS / 1000,
            ReseedLimits::default(),
            ValidationContext::new(Date::from_millis(TEST_NOW_MS)),
        )
        .expect("verify");

        // The bundle should verify (signature is valid) but the
        // RouterInfo validation should fail for the misnamed entry.
        match &report.outcome {
            ReseedVerifyOutcome::Accepted { accepted } => {
                assert_eq!(*accepted, 0, "no entries should be accepted");
            }
            other => panic!("expected Accepted with 0 entries, got: {other:?}"),
        }
        // The per-entry report should show the validation failure.
        assert!(!report.entries.is_empty());
        let entry = &report.entries[0];
        assert_eq!(entry.name, wrong_name);
        assert_eq!(entry.state, ReseedEntryState::RejectedValidation);
    }
}
