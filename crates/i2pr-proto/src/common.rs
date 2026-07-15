//! Bounded codecs for the common I2P identity, addressing, and lease records.
//!
//! This module follows the pinned common-structures source listed in
//! `specs/SOURCES.md` (I2P website commit
//! `88596022920bdf99f27db27688faf4f204792fcd`) and the common-structure
//! dossier in `specs/protocols/01-common-identity-crypto.md`. It implements
//! structural validation and canonical encoding only. It does not implement
//! signatures, encryption, transport state machines, freshness policy, or
//! capability advertisement.
//!
//! Parsed signed records retain the exact bytes preceding their signature.
//! Callers can therefore pass [`RouterInfo::signed_bytes`] or
//! [`LeaseSet::signed_bytes`] to a later cryptographic verifier instead of
//! silently verifying a reserialized semantic value.

use std::{cmp::Ordering, fmt};

use sha2::{Digest, Sha256};

use crate::{CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec};

/// Maximum total size accepted for a common structure by the initial model.
pub const MAX_COMMON_STRUCTURE_SIZE: usize = 1024 * 1024;
/// Maximum body size of a Mapping, excluding its two-byte size field.
pub const MAX_MAPPING_BODY_SIZE: usize = u16::MAX as usize;
/// Maximum number of RouterAddress entries in a RouterInfo.
pub const MAX_ROUTER_ADDRESSES: usize = u8::MAX as usize;
/// Maximum number of classic Lease or Lease2 entries.
pub const MAX_LEASES: usize = 16;
/// Maximum number of encryption keys in the deferred LeaseSet2 model.
pub const MAX_ENCRYPTION_KEYS: usize = 8;

const KEY_AREA_SIZE: usize = 384;
const LEGACY_PUBLIC_KEY_SIZE: usize = 256;
const LEGACY_SIGNING_KEY_SIZE: usize = 128;

fn invalid(offset: usize, context: &'static str) -> CodecError {
    CodecError::InvalidFieldValue { offset, context }
}

fn unsupported(offset: usize, context: &'static str, value: u64) -> CodecError {
    CodecError::Unsupported {
        offset,
        context,
        value,
    }
}

fn take_array<const N: usize>(cursor: &mut DecodeCursor<'_>) -> Result<[u8; N], CodecError> {
    cursor
        .take(N)?
        .try_into()
        .map_err(|_| invalid(cursor.offset(), "fixed-size byte field"))
}

fn java_string_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn validate_text(value: &str, allow_empty: bool, context: &'static str) -> Result<(), CodecError> {
    let length = value.len();
    if (!allow_empty && length == 0) || length > u8::MAX as usize {
        return Err(CodecError::LengthExceeded {
            offset: 0,
            declared: length,
            maximum: u8::MAX as usize,
            context,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(0, context));
    }
    Ok(())
}

/// An eight-byte millisecond timestamp. Zero is the protocol's undefined date.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Date(u64);

impl Date {
    /// Creates a date from milliseconds since the Unix epoch.
    pub const fn from_millis(value: u64) -> Self {
        Self(value)
    }

    /// Returns the encoded millisecond value.
    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Returns whether this date is the protocol's undefined/null value.
    pub const fn is_undefined(self) -> bool {
        self.0 == 0
    }

    /// Decodes one complete Date value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one Date value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self(cursor.read_u64()?))
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u64(self.0)
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Date").field(&self.0).finish()
    }
}

/// A four-byte seconds-since-epoch date used by Lease2-family structures.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Date32(u32);

impl Date32 {
    /// Creates a protocol seconds date.
    pub const fn from_seconds(value: u32) -> Self {
        Self(value)
    }

    /// Returns the encoded seconds value.
    pub const fn as_seconds(self) -> u32 {
        self.0
    }

    /// Decodes one complete Date32 value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one Date32 value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self(cursor.read_u32()?))
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u32(self.0)
    }
}

/// A fixed 32-byte SHA-256 value.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Constructs a hash from exactly 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Computes SHA-256 using the reviewed `sha2` crate.
    pub fn digest(input: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(input);
        Self(hasher.finalize().into())
    }

    /// Returns the raw protocol bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Decodes one complete Hash value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one Hash value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self(take_array(cursor)?))
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_raw(&self.0)
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Hash(..)")
    }
}

/// A protocol signing-key type and its known public/signature lengths.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SigningKeyType {
    DsaSha1,
    EcdsaSha256P256,
    EcdsaSha384P384,
    EcdsaSha512P521,
    RsaSha2562048,
    RsaSha3843072,
    RsaSha5124096,
    EdDsaSha512Ed25519,
    EdDsaSha512Ed25519ph,
    RedDsaSha512Ed25519,
    Unknown(u16),
}

impl SigningKeyType {
    /// Decodes the numeric protocol identifier without mapping unknown values
    /// to a default algorithm.
    pub const fn from_code(value: u16) -> Self {
        match value {
            0 => Self::DsaSha1,
            1 => Self::EcdsaSha256P256,
            2 => Self::EcdsaSha384P384,
            3 => Self::EcdsaSha512P521,
            4 => Self::RsaSha2562048,
            5 => Self::RsaSha3843072,
            6 => Self::RsaSha5124096,
            7 => Self::EdDsaSha512Ed25519,
            8 => Self::EdDsaSha512Ed25519ph,
            11 => Self::RedDsaSha512Ed25519,
            other => Self::Unknown(other),
        }
    }

    /// Returns the numeric protocol identifier.
    pub const fn code(self) -> u16 {
        match self {
            Self::DsaSha1 => 0,
            Self::EcdsaSha256P256 => 1,
            Self::EcdsaSha384P384 => 2,
            Self::EcdsaSha512P521 => 3,
            Self::RsaSha2562048 => 4,
            Self::RsaSha3843072 => 5,
            Self::RsaSha5124096 => 6,
            Self::EdDsaSha512Ed25519 => 7,
            Self::EdDsaSha512Ed25519ph => 8,
            Self::RedDsaSha512Ed25519 => 11,
            Self::Unknown(value) => value,
        }
    }

    /// Returns the encoded public-key length when this type is known.
    pub const fn public_key_len(self) -> Option<usize> {
        match self {
            Self::DsaSha1 => Some(128),
            Self::EcdsaSha256P256 => Some(64),
            Self::EcdsaSha384P384 => Some(96),
            Self::EcdsaSha512P521 => Some(132),
            Self::RsaSha2562048 => Some(256),
            Self::RsaSha3843072 => Some(384),
            Self::RsaSha5124096 => Some(512),
            Self::EdDsaSha512Ed25519 | Self::EdDsaSha512Ed25519ph | Self::RedDsaSha512Ed25519 => {
                Some(32)
            }
            Self::Unknown(_) => None,
        }
    }

    /// Returns the encoded signature length when this type is known.
    pub const fn signature_len(self) -> Option<usize> {
        match self {
            Self::DsaSha1 => Some(40),
            Self::EcdsaSha256P256 => Some(64),
            Self::EcdsaSha384P384 => Some(96),
            Self::EcdsaSha512P521 => Some(132),
            Self::RsaSha2562048 => Some(256),
            Self::RsaSha3843072 => Some(384),
            Self::RsaSha5124096 => Some(512),
            Self::EdDsaSha512Ed25519 | Self::EdDsaSha512Ed25519ph | Self::RedDsaSha512Ed25519 => {
                Some(64)
            }
            Self::Unknown(_) => None,
        }
    }
}

/// A protocol encryption-key type and its known public-key length.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CryptoKeyType {
    ElGamal,
    P256,
    P384,
    P521,
    X25519,
    MlKem512X25519,
    MlKem768X25519,
    MlKem1024X25519,
    Unknown(u16),
}

impl CryptoKeyType {
    /// Decodes a numeric protocol identifier with an explicit unknown path.
    pub const fn from_code(value: u16) -> Self {
        match value {
            0 => Self::ElGamal,
            1 => Self::P256,
            2 => Self::P384,
            3 => Self::P521,
            4 => Self::X25519,
            5 => Self::MlKem512X25519,
            6 => Self::MlKem768X25519,
            7 => Self::MlKem1024X25519,
            other => Self::Unknown(other),
        }
    }

    /// Returns the numeric protocol identifier.
    pub const fn code(self) -> u16 {
        match self {
            Self::ElGamal => 0,
            Self::P256 => 1,
            Self::P384 => 2,
            Self::P521 => 3,
            Self::X25519 => 4,
            Self::MlKem512X25519 => 5,
            Self::MlKem768X25519 => 6,
            Self::MlKem1024X25519 => 7,
            Self::Unknown(value) => value,
        }
    }

    /// Returns the encoded public-key length when known for a public-key field.
    pub const fn public_key_len(self) -> Option<usize> {
        match self {
            Self::ElGamal => Some(256),
            Self::P256 => Some(64),
            Self::P384 => Some(96),
            Self::P521 => Some(132),
            Self::X25519 | Self::MlKem512X25519 | Self::MlKem768X25519 | Self::MlKem1024X25519 => {
                Some(32)
            }
            Self::Unknown(_) => None,
        }
    }

    const fn allowed_in_identity(self) -> bool {
        matches!(self, Self::ElGamal | Self::X25519)
    }
}

/// A validated public encryption key.
#[derive(Clone, Eq, PartialEq)]
pub struct PublicKey {
    key_type: CryptoKeyType,
    bytes: Vec<u8>,
}

impl PublicKey {
    /// Creates a key after checking its algorithm-specific encoded length.
    pub fn new(key_type: CryptoKeyType, bytes: Vec<u8>) -> Result<Self, CodecError> {
        let expected = key_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", key_type.code() as u64))?;
        if bytes.len() != expected {
            return Err(invalid(0, "encryption public key length"));
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the key type.
    pub const fn key_type(&self) -> CryptoKeyType {
        self.key_type
    }

    /// Returns the validated key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicKey")
            .field("key_type", &self.key_type)
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// A validated public signing key.
#[derive(Clone, Eq, PartialEq)]
pub struct SigningPublicKey {
    key_type: SigningKeyType,
    bytes: Vec<u8>,
}

impl SigningPublicKey {
    /// Creates a key after checking its algorithm-specific encoded length.
    pub fn new(key_type: SigningKeyType, bytes: Vec<u8>) -> Result<Self, CodecError> {
        let expected = key_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", key_type.code() as u64))?;
        if bytes.len() != expected {
            return Err(invalid(0, "signing public key length"));
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the key type.
    pub const fn key_type(&self) -> SigningKeyType {
        self.key_type
    }

    /// Returns the validated key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for SigningPublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SigningPublicKey")
            .field("key_type", &self.key_type)
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// A signature with its inferred signing-key type made explicit.
#[derive(Clone, Eq, PartialEq)]
pub struct SignatureValue {
    key_type: SigningKeyType,
    bytes: Vec<u8>,
}

impl SignatureValue {
    /// Creates a signature after checking the type-specific encoded length.
    pub fn new(key_type: SigningKeyType, bytes: Vec<u8>) -> Result<Self, CodecError> {
        let expected = key_type
            .signature_len()
            .ok_or_else(|| unsupported(0, "signature type", key_type.code() as u64))?;
        if bytes.len() != expected {
            return Err(invalid(0, "signature length"));
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the signing-key type.
    pub const fn key_type(&self) -> SigningKeyType {
        self.key_type
    }

    /// Returns the signature bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for SignatureValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignatureValue")
            .field("key_type", &self.key_type)
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// One UTF-8 mapping entry.
#[derive(Clone, Eq, PartialEq)]
pub struct MappingEntry {
    key: String,
    value: String,
}

impl MappingEntry {
    /// Creates an entry after checking protocol string bounds.
    pub fn new(key: String, value: String) -> Result<Self, CodecError> {
        validate_text(&key, true, "mapping key")?;
        validate_text(&value, true, "mapping value")?;
        Ok(Self { key, value })
    }

    /// Returns the entry key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the entry value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for MappingEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MappingEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .finish()
    }
}

/// A sorted, duplicate-free I2P Mapping.
#[derive(Clone, Eq, PartialEq)]
pub struct Mapping {
    entries: Vec<MappingEntry>,
}

impl Mapping {
    /// Returns an empty mapping.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Builds a canonical mapping from owned key/value pairs.
    pub fn from_entries(entries: Vec<(String, String)>) -> Result<Self, CodecError> {
        let mut entries = entries
            .into_iter()
            .map(|(key, value)| MappingEntry::new(key, value))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| java_string_cmp(&left.key, &right.key));
        for pair in entries.windows(2) {
            if pair[0].key == pair[1].key {
                return Err(CodecError::DuplicateField {
                    offset: 0,
                    context: "mapping key",
                });
            }
        }
        let mapping = Self { entries };
        mapping.encoded_body_len()?;
        Ok(mapping)
    }

    /// Starts a canonical mapping builder.
    pub fn builder() -> MappingBuilder {
        MappingBuilder {
            entries: Vec::new(),
        }
    }

    /// Returns entries in canonical wire order.
    pub fn entries(&self) -> &[MappingEntry] {
        &self.entries
    }

    /// Looks up a key without exposing mutable map state.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .binary_search_by(|entry| java_string_cmp(&entry.key, key))
            .ok()
            .map(|index| self.entries[index].value.as_str())
    }

    /// Decodes and validates a complete Mapping.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, |cursor| Self::decode_from(cursor, maximum))
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>, maximum: usize) -> Result<Self, CodecError> {
        let body_len = usize::from(cursor.read_u16()?);
        let body_limit = maximum.min(MAX_MAPPING_BODY_SIZE);
        if body_len > body_limit {
            return Err(CodecError::LengthExceeded {
                offset: cursor.offset().saturating_sub(2),
                declared: body_len,
                maximum: body_limit,
                context: "mapping body",
            });
        }
        let body = cursor.take(body_len)?;
        let mut inner = DecodeCursor::new(body, body_len)?;
        let mut entries: Vec<MappingEntry> = Vec::new();
        while !inner.is_empty() {
            let key_offset = inner.offset();
            let key = inner.read_utf8_u8(u8::MAX as usize)?.to_owned();
            if inner.read_u8()? != b'=' {
                return Err(invalid(
                    inner.offset().saturating_sub(1),
                    "mapping separator",
                ));
            }
            let value = inner.read_utf8_u8(u8::MAX as usize)?.to_owned();
            if inner.read_u8()? != b';' {
                return Err(invalid(
                    inner.offset().saturating_sub(1),
                    "mapping terminator",
                ));
            }
            let entry = MappingEntry::new(key, value)?;
            if let Some(previous) = entries.last() {
                match java_string_cmp(previous.key(), entry.key()) {
                    Ordering::Equal => {
                        return Err(CodecError::DuplicateField {
                            offset: key_offset,
                            context: "mapping key",
                        });
                    }
                    Ordering::Greater => {
                        return Err(CodecError::NonCanonical {
                            offset: key_offset,
                            context: "mapping key order",
                        });
                    }
                    Ordering::Less => {}
                }
            }
            entries.push(entry);
        }
        inner.finish()?;
        Ok(Self { entries })
    }

    /// Encodes a complete canonical Mapping.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn encoded_body_len(&self) -> Result<usize, CodecError> {
        let mut total = 0usize;
        for entry in &self.entries {
            total = total
                .checked_add(1 + entry.key.len() + 1 + 1 + entry.value.len() + 1)
                .ok_or(CodecError::ArithmeticOverflow {
                    offset: 0,
                    context: "mapping body length",
                })?;
        }
        if total > MAX_MAPPING_BODY_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: total,
                maximum: MAX_MAPPING_BODY_SIZE,
                context: "mapping body",
            });
        }
        Ok(total)
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        let body_len = self.encoded_body_len()?;
        let body_len = u16::try_from(body_len).map_err(|_| invalid(0, "mapping body length"))?;
        encoder.write_u16(body_len)?;
        for entry in &self.entries {
            encoder.write_utf8_u8(entry.key(), u8::MAX as usize)?;
            encoder.write_raw(b"=")?;
            encoder.write_utf8_u8(entry.value(), u8::MAX as usize)?;
            encoder.write_raw(b";")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Mapping {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Mapping")
            .field(&self.entries)
            .finish()
    }
}

/// Builder for an immutable canonical Mapping.
pub struct MappingBuilder {
    entries: Vec<(String, String)>,
}

impl MappingBuilder {
    /// Adds an entry. Duplicate detection is finalized by [`Self::build`].
    pub fn insert(&mut self, key: String, value: String) -> Result<(), CodecError> {
        MappingEntry::new(key.clone(), value.clone())?;
        self.entries.push((key, value));
        Ok(())
    }

    /// Validates and returns the immutable canonical mapping.
    pub fn build(self) -> Result<Mapping, CodecError> {
        Mapping::from_entries(self.entries)
    }
}

/// Certificate container kinds.
#[derive(Clone, Eq, PartialEq)]
pub enum Certificate {
    /// The three-byte null certificate.
    Null,
    /// A validated key certificate.
    Key(KeyCertificate),
    /// A bounded but unsupported/deferred certificate payload.
    Unsupported { type_code: u8, payload: Vec<u8> },
}

impl Certificate {
    /// Decodes a complete certificate.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let type_code = cursor.read_u8()?;
        let payload_len = usize::from(cursor.read_u16()?);
        let payload = cursor.take(payload_len)?.to_vec();
        match type_code {
            0 if payload.is_empty() => Ok(Self::Null),
            0 => Err(invalid(0, "null certificate payload")),
            2 if payload.is_empty() => Ok(Self::Unsupported { type_code, payload }),
            3 if payload.len() == 40 || payload.len() == 72 => {
                Ok(Self::Unsupported { type_code, payload })
            }
            5 => Ok(Self::Key(KeyCertificate::decode_payload(&payload)?)),
            1 | 4 => Ok(Self::Unsupported { type_code, payload }),
            other => Ok(Self::Unsupported {
                type_code: other,
                payload,
            }),
        }
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Null => {
                encoder.write_u8(0)?;
                encoder.write_u16(0)
            }
            Self::Key(key) => {
                let payload = key.encode_payload()?;
                encoder.write_u8(5)?;
                encoder.write_u16(
                    u16::try_from(payload.len()).map_err(|_| invalid(0, "key certificate"))?,
                )?;
                encoder.write_raw(&payload)
            }
            Self::Unsupported { type_code, payload } => {
                encoder.write_u8(*type_code)?;
                encoder.write_u16(
                    u16::try_from(payload.len()).map_err(|_| invalid(0, "certificate payload"))?,
                )?;
                encoder.write_raw(payload)
            }
        }
    }

    /// Encodes a complete certificate.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }
}

impl fmt::Debug for Certificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => formatter.write_str("Certificate::Null"),
            Self::Key(key) => formatter
                .debug_tuple("Certificate::Key")
                .field(key)
                .finish(),
            Self::Unsupported { type_code, payload } => formatter
                .debug_struct("Certificate::Unsupported")
                .field("type_code", type_code)
                .field("length", &payload.len())
                .finish(),
        }
    }
}

/// A key certificate's signing and encryption algorithm identifiers.
#[derive(Clone, Eq, PartialEq)]
pub struct KeyCertificate {
    signing_type: SigningKeyType,
    crypto_type: CryptoKeyType,
    excess_signing: Vec<u8>,
    excess_crypto: Vec<u8>,
}

impl KeyCertificate {
    /// Creates a key certificate with explicitly supplied excess key bytes.
    pub fn new(
        signing_type: SigningKeyType,
        crypto_type: CryptoKeyType,
        excess_signing: Vec<u8>,
        excess_crypto: Vec<u8>,
    ) -> Result<Self, CodecError> {
        let signing_len = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?;
        let crypto_len = crypto_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", crypto_type.code() as u64))?;
        if excess_signing.len() != signing_len.saturating_sub(LEGACY_SIGNING_KEY_SIZE)
            || excess_crypto.len() != crypto_len.saturating_sub(LEGACY_PUBLIC_KEY_SIZE)
        {
            return Err(invalid(0, "key certificate excess data length"));
        }
        Ok(Self {
            signing_type,
            crypto_type,
            excess_signing,
            excess_crypto,
        })
    }

    /// Creates the canonical no-excess key certificate for the supplied types.
    pub fn for_types(
        signing_type: SigningKeyType,
        crypto_type: CryptoKeyType,
    ) -> Result<Self, CodecError> {
        let signing_extra = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?
            .saturating_sub(LEGACY_SIGNING_KEY_SIZE);
        let crypto_extra = crypto_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", crypto_type.code() as u64))?
            .saturating_sub(LEGACY_PUBLIC_KEY_SIZE);
        Self::new(
            signing_type,
            crypto_type,
            vec![0; signing_extra],
            vec![0; crypto_extra],
        )
    }

    /// Returns the signing public-key type.
    pub const fn signing_type(&self) -> SigningKeyType {
        self.signing_type
    }

    /// Returns the encryption public-key type.
    pub const fn crypto_type(&self) -> CryptoKeyType {
        self.crypto_type
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(payload, payload.len())?;
        let signing_type = SigningKeyType::from_code(cursor.read_u16()?);
        let crypto_type = CryptoKeyType::from_code(cursor.read_u16()?);
        let signing_len = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?;
        let crypto_len = crypto_type
            .public_key_len()
            .ok_or_else(|| unsupported(2, "encryption key type", crypto_type.code() as u64))?;
        let signing_extra_len = signing_len.saturating_sub(LEGACY_SIGNING_KEY_SIZE);
        let crypto_extra_len = crypto_len.saturating_sub(LEGACY_PUBLIC_KEY_SIZE);
        let excess_signing = cursor.take(signing_extra_len)?.to_vec();
        let excess_crypto = cursor.take(crypto_extra_len)?.to_vec();
        cursor.finish()?;
        Self::new(signing_type, crypto_type, excess_signing, excess_crypto)
    }

    fn encode_payload(&self) -> Result<Vec<u8>, CodecError> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.signing_type.code().to_be_bytes());
        payload.extend_from_slice(&self.crypto_type.code().to_be_bytes());
        payload.extend_from_slice(&self.excess_signing);
        payload.extend_from_slice(&self.excess_crypto);
        Ok(payload)
    }
}

impl fmt::Debug for KeyCertificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyCertificate")
            .field("signing_type", &self.signing_type)
            .field("crypto_type", &self.crypto_type)
            .field("excess_signing_length", &self.excess_signing.len())
            .field("excess_crypto_length", &self.excess_crypto.len())
            .finish()
    }
}

/// The 384-byte key area and certificate used by a RouterIdentity or Destination.
#[derive(Clone, Eq, PartialEq)]
pub struct KeyAndCert {
    public_key: PublicKey,
    signing_key: SigningPublicKey,
    padding: Vec<u8>,
    certificate: Certificate,
}

impl KeyAndCert {
    /// Creates validated key material. Padding is retained because it is part
    /// of the identity/destination hash input.
    pub fn new(
        public_key: PublicKey,
        signing_key: SigningPublicKey,
        padding: Vec<u8>,
        certificate: Certificate,
    ) -> Result<Self, CodecError> {
        let key = Self {
            public_key,
            signing_key,
            padding,
            certificate,
        };
        key.validate()?;
        Ok(key)
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let key_area = cursor.take(KEY_AREA_SIZE)?;
        let certificate = Certificate::decode_from(cursor)?;
        let (public_type, signing_type) = match &certificate {
            Certificate::Null => (CryptoKeyType::ElGamal, SigningKeyType::DsaSha1),
            Certificate::Key(key) => (key.crypto_type(), key.signing_type()),
            Certificate::Unsupported { type_code, .. } => {
                return Err(unsupported(
                    384,
                    "identity certificate",
                    u64::from(*type_code),
                ));
            }
        };
        let public_len = public_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", public_type.code() as u64))?;
        let signing_len = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?;
        if public_len > LEGACY_PUBLIC_KEY_SIZE {
            return Err(unsupported(
                0,
                "identity encryption key layout",
                public_type.code() as u64,
            ));
        }
        let signing_prefix_len = signing_len.min(LEGACY_SIGNING_KEY_SIZE);
        let padding_len = KEY_AREA_SIZE
            .checked_sub(public_len + signing_prefix_len)
            .ok_or_else(|| invalid(0, "identity key area"))?;
        let public_bytes = key_area[..public_len].to_vec();
        let signing_start = KEY_AREA_SIZE - signing_prefix_len;
        let mut signing_bytes = key_area[signing_start..].to_vec();
        if let Certificate::Key(key) = &certificate {
            signing_bytes.extend_from_slice(&key.excess_signing);
            let mut full_public = public_bytes.clone();
            full_public.extend_from_slice(&key.excess_crypto);
            let public_key = PublicKey::new(public_type, full_public)?;
            let signing_key = SigningPublicKey::new(signing_type, signing_bytes)?;
            return Self::new(
                public_key,
                signing_key,
                key_area[public_len..public_len + padding_len].to_vec(),
                certificate,
            );
        }
        let public_key = PublicKey::new(public_type, public_bytes)?;
        let signing_key = SigningPublicKey::new(signing_type, signing_bytes)?;
        Self::new(
            public_key,
            signing_key,
            key_area[public_len..public_len + padding_len].to_vec(),
            certificate,
        )
    }

    /// Decodes one complete key-and-certificate value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one complete key-and-certificate value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn validate(&self) -> Result<(), CodecError> {
        let public_len = self.public_key.key_type().public_key_len().ok_or_else(|| {
            unsupported(
                0,
                "encryption key type",
                self.public_key.key_type().code() as u64,
            )
        })?;
        let signing_len = self
            .signing_key
            .key_type()
            .public_key_len()
            .ok_or_else(|| {
                unsupported(
                    0,
                    "signing key type",
                    self.signing_key.key_type().code() as u64,
                )
            })?;
        if public_len > LEGACY_PUBLIC_KEY_SIZE {
            return Err(unsupported(
                0,
                "identity encryption key layout",
                self.public_key.key_type().code() as u64,
            ));
        }
        let expected_padding = KEY_AREA_SIZE
            .checked_sub(public_len)
            .and_then(|remaining| remaining.checked_sub(signing_len.min(LEGACY_SIGNING_KEY_SIZE)))
            .ok_or_else(|| invalid(0, "identity key area"))?;
        if self.padding.len() != expected_padding {
            return Err(invalid(0, "identity key padding length"));
        }
        match &self.certificate {
            Certificate::Null => {
                if self.public_key.key_type() != CryptoKeyType::ElGamal
                    || self.signing_key.key_type() != SigningKeyType::DsaSha1
                    || self.public_key.as_bytes().len() != LEGACY_PUBLIC_KEY_SIZE
                    || self.signing_key.as_bytes().len() != LEGACY_SIGNING_KEY_SIZE
                    || !self.padding.is_empty()
                {
                    return Err(invalid(0, "null certificate key material"));
                }
            }
            Certificate::Key(key) => {
                if key.crypto_type() != self.public_key.key_type()
                    || key.signing_type() != self.signing_key.key_type()
                {
                    return Err(invalid(0, "key certificate algorithm binding"));
                }
                let public_extra =
                    &self.public_key.as_bytes()[LEGACY_PUBLIC_KEY_SIZE.min(public_len)..];
                let signing_extra =
                    &self.signing_key.as_bytes()[LEGACY_SIGNING_KEY_SIZE.min(signing_len)..];
                if public_extra != key.excess_crypto.as_slice()
                    || signing_extra != key.excess_signing.as_slice()
                {
                    return Err(invalid(0, "key certificate excess key material"));
                }
            }
            Certificate::Unsupported { type_code, .. } => {
                return Err(unsupported(
                    0,
                    "identity certificate",
                    u64::from(*type_code),
                ));
            }
        }
        Ok(())
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        self.validate()?;
        let public_len = self.public_key.as_bytes().len();
        let signing_prefix_len = self
            .signing_key
            .as_bytes()
            .len()
            .min(LEGACY_SIGNING_KEY_SIZE);
        encoder.write_raw(&self.public_key.as_bytes()[..public_len.min(LEGACY_PUBLIC_KEY_SIZE)])?;
        encoder.write_raw(&self.padding)?;
        encoder.write_raw(&self.signing_key.as_bytes()[..signing_prefix_len])?;
        self.certificate.encode_into(encoder)
    }

    /// Returns the validated encryption public key.
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// Returns the validated signing public key.
    pub fn signing_key(&self) -> &SigningPublicKey {
        &self.signing_key
    }

    /// Returns the retained key-area padding bytes.
    pub fn padding(&self) -> &[u8] {
        &self.padding
    }

    /// Returns the certificate.
    pub fn certificate(&self) -> &Certificate {
        &self.certificate
    }
}

impl fmt::Debug for KeyAndCert {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyAndCert")
            .field("public_key", &self.public_key)
            .field("signing_key", &self.signing_key)
            .field("padding_length", &self.padding.len())
            .field("certificate", &self.certificate)
            .finish()
    }
}

/// A router identity, including public key material and its certificate.
#[derive(Clone, Eq, PartialEq)]
pub struct RouterIdentity {
    keys: KeyAndCert,
}

impl RouterIdentity {
    /// Creates an identity and rejects key types not currently valid for router identities.
    pub fn new(keys: KeyAndCert) -> Result<Self, CodecError> {
        if !keys.public_key().key_type().allowed_in_identity() {
            return Err(CodecError::PolicyRejected {
                offset: 0,
                context: "router identity encryption algorithm",
            });
        }
        Ok(Self { keys })
    }

    /// Decodes a complete RouterIdentity.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Self::new(KeyAndCert::decode_from(cursor)?)
    }

    /// Encodes the complete identity.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.keys.encode_into(encoder))
    }

    /// Returns the SHA-256 hash of the exact canonical identity encoding.
    pub fn hash(&self) -> Result<Hash, CodecError> {
        Ok(Hash::digest(
            &self.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)?,
        ))
    }

    /// Returns the identity's encryption public key.
    pub fn public_key(&self) -> &PublicKey {
        self.keys.public_key()
    }

    /// Returns the identity's signing public key.
    pub fn signing_key(&self) -> &SigningPublicKey {
        self.keys.signing_key()
    }

    /// Returns the identity certificate.
    pub fn certificate(&self) -> &Certificate {
        self.keys.certificate()
    }
}

impl fmt::Debug for RouterIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RouterIdentity")
            .field(&self.keys)
            .finish()
    }
}

/// A destination identity. Its public encryption field is structurally retained
/// even though legacy destination encryption does not use it.
#[derive(Clone, Eq, PartialEq)]
pub struct Destination {
    keys: KeyAndCert,
}

impl Destination {
    /// Creates a destination and applies the current structural algorithm policy.
    pub fn new(keys: KeyAndCert) -> Result<Self, CodecError> {
        if !keys.public_key().key_type().allowed_in_identity() {
            return Err(CodecError::PolicyRejected {
                offset: 0,
                context: "destination encryption algorithm",
            });
        }
        Ok(Self { keys })
    }

    /// Decodes a complete Destination.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Self::new(KeyAndCert::decode_from(cursor)?)
    }

    /// Encodes the complete destination.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.keys.encode_into(encoder))
    }

    /// Returns the SHA-256 hash of the exact canonical destination encoding.
    pub fn hash(&self) -> Result<Hash, CodecError> {
        Ok(Hash::digest(
            &self.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)?,
        ))
    }

    /// Returns the destination's encryption public key.
    pub fn public_key(&self) -> &PublicKey {
        self.keys.public_key()
    }

    /// Returns the destination's signing public key.
    pub fn signing_key(&self) -> &SigningPublicKey {
        self.keys.signing_key()
    }

    /// Returns the destination certificate.
    pub fn certificate(&self) -> &Certificate {
        self.keys.certificate()
    }
}

impl fmt::Debug for Destination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Destination")
            .field(&self.keys)
            .finish()
    }
}

/// A transport address containing only protocol data, never a transport object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterAddress {
    cost: u8,
    expiration: Date,
    transport_style: String,
    options: Mapping,
}

impl RouterAddress {
    /// Creates a validated router address.
    pub fn new(
        cost: u8,
        expiration: Date,
        transport_style: String,
        options: Mapping,
    ) -> Result<Self, CodecError> {
        validate_text(&transport_style, false, "transport style")?;
        Ok(Self {
            cost,
            expiration,
            transport_style,
            options,
        })
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>, maximum: usize) -> Result<Self, CodecError> {
        let cost = cursor.read_u8()?;
        let expiration = Date::decode_from(cursor)?;
        let transport_style = cursor.read_utf8_u8(u8::MAX as usize)?.to_owned();
        let options = Mapping::decode_from(cursor, maximum)?;
        Self::new(cost, expiration, transport_style, options)
    }

    /// Decodes one complete RouterAddress.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, |cursor| Self::decode_from(cursor, maximum))
    }

    /// Encodes one complete RouterAddress.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u8(self.cost)?;
        self.expiration.encode_into(encoder)?;
        encoder.write_utf8_u8(&self.transport_style, u8::MAX as usize)?;
        self.options.encode_into(encoder)
    }

    /// Returns the relative transport cost.
    pub const fn cost(&self) -> u8 {
        self.cost
    }

    /// Returns the structural expiration date. Freshness policy is deferred.
    pub const fn expiration(&self) -> Date {
        self.expiration
    }

    /// Returns the opaque transport style identifier.
    pub fn transport_style(&self) -> &str {
        &self.transport_style
    }

    /// Returns canonical transport options.
    pub fn options(&self) -> &Mapping {
        &self.options
    }
}

/// A bounded protocol-version option value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolVersion(String);

impl ProtocolVersion {
    /// Validates a version string without assigning runtime policy to it.
    pub fn new(value: &str) -> Result<Self, CodecError> {
        validate_text(value, false, "router version")?;
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return Err(invalid(0, "router version"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the encoded version text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded, opaque router capability string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capabilities(String);

impl Capabilities {
    /// Validates capability text while allowing future capability letters.
    pub fn new(value: &str) -> Result<Self, CodecError> {
        validate_text(value, true, "router capabilities")?;
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(invalid(0, "router capabilities"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the encoded capability text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_router_options(options: &Mapping) -> Result<(), CodecError> {
    if let Some(version) = options.get("router.version") {
        ProtocolVersion::new(version)?;
    }
    if let Some(capabilities) = options.get("caps") {
        Capabilities::new(capabilities)?;
    }
    Ok(())
}

/// A RouterInfo with its exact signed region retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterInfo {
    router_identity: RouterIdentity,
    published: Date,
    addresses: Vec<RouterAddress>,
    peers: Vec<Hash>,
    options: Mapping,
    signed_bytes: Vec<u8>,
    signature: SignatureValue,
}

impl RouterInfo {
    /// Creates a RouterInfo from validated semantic fields and a typed signature.
    pub fn new(
        router_identity: RouterIdentity,
        published: Date,
        addresses: Vec<RouterAddress>,
        peers: Vec<Hash>,
        options: Mapping,
        signature: SignatureValue,
    ) -> Result<Self, CodecError> {
        if addresses.len() > MAX_ROUTER_ADDRESSES || peers.len() > u8::MAX as usize {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: addresses.len().max(peers.len()),
                maximum: MAX_ROUTER_ADDRESSES,
                context: "RouterInfo entry count",
            });
        }
        if signature.key_type() != router_identity.signing_key().key_type() {
            return Err(invalid(0, "RouterInfo signature type"));
        }
        validate_router_options(&options)?;
        let signed_bytes = encode_to_vec(MAX_COMMON_STRUCTURE_SIZE, |encoder| {
            Self::encode_unsigned(
                encoder,
                &router_identity,
                published,
                &addresses,
                &peers,
                &options,
            )
        })?;
        let total = signed_bytes
            .len()
            .checked_add(signature.as_bytes().len())
            .ok_or(CodecError::ArithmeticOverflow {
                offset: signed_bytes.len(),
                context: "RouterInfo length",
            })?;
        if total > MAX_COMMON_STRUCTURE_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: signed_bytes.len(),
                declared: total,
                maximum: MAX_COMMON_STRUCTURE_SIZE,
                context: "RouterInfo",
            });
        }
        Ok(Self {
            router_identity,
            published,
            addresses,
            peers,
            options,
            signed_bytes,
            signature,
        })
    }

    /// Decodes a complete RouterInfo and retains its exact signed bytes.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        if input.len() > maximum {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: input.len(),
                maximum,
                context: "RouterInfo",
            });
        }
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let router_identity = RouterIdentity::decode_from(&mut cursor)?;
        let published = Date::decode_from(&mut cursor)?;
        let address_count = usize::from(cursor.read_u8()?);
        if address_count > MAX_ROUTER_ADDRESSES {
            return Err(CodecError::PolicyRejected {
                offset: cursor.offset().saturating_sub(1),
                context: "RouterInfo address count",
            });
        }
        let mut addresses = Vec::with_capacity(address_count);
        for _ in 0..address_count {
            addresses.push(RouterAddress::decode_from(&mut cursor, maximum)?);
        }
        let peer_count = usize::from(cursor.read_u8()?);
        let mut peers = Vec::with_capacity(peer_count);
        for _ in 0..peer_count {
            peers.push(Hash::decode_from(&mut cursor)?);
        }
        let options = Mapping::decode_from(&mut cursor, maximum)?;
        validate_router_options(&options)?;
        let signed_end = cursor.offset();
        let signature_len = router_identity
            .signing_key()
            .key_type()
            .signature_len()
            .ok_or_else(|| {
                unsupported(
                    signed_end,
                    "RouterInfo signature type",
                    router_identity.signing_key().key_type().code() as u64,
                )
            })?;
        let signature = SignatureValue::new(
            router_identity.signing_key().key_type(),
            cursor.take(signature_len)?.to_vec(),
        )?;
        cursor.finish()?;
        Ok(Self {
            router_identity,
            published,
            addresses,
            peers,
            options,
            signed_bytes: input[..signed_end].to_vec(),
            signature,
        })
    }

    fn encode_unsigned(
        encoder: &mut EncodeBuffer<'_>,
        router_identity: &RouterIdentity,
        published: Date,
        addresses: &[RouterAddress],
        peers: &[Hash],
        options: &Mapping,
    ) -> Result<(), CodecError> {
        router_identity.keys.encode_into(encoder)?;
        published.encode_into(encoder)?;
        encoder.write_u8(
            u8::try_from(addresses.len()).map_err(|_| invalid(0, "RouterInfo address count"))?,
        )?;
        for address in addresses {
            address.encode_into(encoder)?;
        }
        encoder.write_u8(
            u8::try_from(peers.len()).map_err(|_| invalid(0, "RouterInfo peer count"))?,
        )?;
        for peer in peers {
            peer.encode_into(encoder)?;
        }
        options.encode_into(encoder)
    }

    /// Encodes the exact retained signed region followed by its signature.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes)?;
            encoder.write_raw(self.signature.as_bytes())
        })
    }

    /// Returns the exact bytes covered by the RouterInfo signature.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }

    /// Returns the typed RouterInfo signature.
    pub fn signature(&self) -> &SignatureValue {
        &self.signature
    }

    /// Returns the router identity.
    pub fn router_identity(&self) -> &RouterIdentity {
        &self.router_identity
    }

    /// Returns the publication date; freshness policy is deferred.
    pub const fn published(&self) -> Date {
        self.published
    }

    /// Returns the ordered router addresses.
    pub fn addresses(&self) -> &[RouterAddress] {
        &self.addresses
    }

    /// Returns the optional bounded protocol version.
    pub fn protocol_version(&self) -> Result<Option<ProtocolVersion>, CodecError> {
        self.options
            .get("router.version")
            .map(ProtocolVersion::new)
            .transpose()
    }

    /// Returns the optional bounded capability string.
    pub fn capabilities(&self) -> Result<Option<Capabilities>, CodecError> {
        self.options.get("caps").map(Capabilities::new).transpose()
    }

    /// Returns the complete options mapping.
    pub fn options(&self) -> &Mapping {
        &self.options
    }
}

/// A classic 44-byte Lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    tunnel_gateway: Hash,
    tunnel_id: u32,
    end_date: Date,
}

impl Lease {
    /// Creates a structural Lease. Tunnel ID zero is retained for special cases.
    pub const fn new(tunnel_gateway: Hash, tunnel_id: u32, end_date: Date) -> Self {
        Self {
            tunnel_gateway,
            tunnel_id,
            end_date,
        }
    }

    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self::new(
            Hash::decode_from(cursor)?,
            cursor.read_u32()?,
            Date::decode_from(cursor)?,
        ))
    }

    /// Decodes one complete 44-byte Lease.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one complete Lease.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        self.tunnel_gateway.encode_into(encoder)?;
        encoder.write_u32(self.tunnel_id)?;
        self.end_date.encode_into(encoder)
    }

    /// Returns the gateway hash.
    pub const fn tunnel_gateway(&self) -> Hash {
        self.tunnel_gateway
    }

    /// Returns the tunnel ID.
    pub const fn tunnel_id(&self) -> u32 {
        self.tunnel_id
    }

    /// Returns the expiration date; freshness policy is deferred.
    pub const fn end_date(&self) -> Date {
        self.end_date
    }
}

/// A classic LeaseSet. LeaseSet2-family structures are intentionally deferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseSet {
    destination: Destination,
    encryption_key: PublicKey,
    signing_key: SigningPublicKey,
    leases: Vec<Lease>,
    signed_bytes: Vec<u8>,
    signature: SignatureValue,
}

impl LeaseSet {
    /// Creates a validated classic LeaseSet.
    pub fn new(
        destination: Destination,
        encryption_key: PublicKey,
        signing_key: SigningPublicKey,
        leases: Vec<Lease>,
        signature: SignatureValue,
    ) -> Result<Self, CodecError> {
        if encryption_key.key_type() != CryptoKeyType::ElGamal
            || encryption_key.as_bytes().len() != LEGACY_PUBLIC_KEY_SIZE
        {
            return Err(CodecError::PolicyRejected {
                offset: 0,
                context: "classic LeaseSet encryption key",
            });
        }
        if signing_key.key_type() != destination.signing_key().key_type()
            || signature.key_type() != destination.signing_key().key_type()
        {
            return Err(invalid(0, "LeaseSet signing type"));
        }
        if leases.len() > MAX_LEASES {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: leases.len(),
                maximum: MAX_LEASES,
                context: "LeaseSet lease count",
            });
        }
        let signed_bytes = encode_to_vec(MAX_COMMON_STRUCTURE_SIZE, |encoder| {
            Self::encode_unsigned(
                encoder,
                &destination,
                &encryption_key,
                &signing_key,
                &leases,
            )
        })?;
        let total = signed_bytes
            .len()
            .checked_add(signature.as_bytes().len())
            .ok_or(CodecError::ArithmeticOverflow {
                offset: signed_bytes.len(),
                context: "LeaseSet length",
            })?;
        if total > MAX_COMMON_STRUCTURE_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: signed_bytes.len(),
                declared: total,
                maximum: MAX_COMMON_STRUCTURE_SIZE,
                context: "LeaseSet",
            });
        }
        Ok(Self {
            destination,
            encryption_key,
            signing_key,
            leases,
            signed_bytes,
            signature,
        })
    }

    /// Decodes a complete classic LeaseSet.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let destination = Destination::decode_from(&mut cursor)?;
        let encryption_key = PublicKey::new(
            CryptoKeyType::ElGamal,
            cursor.take(LEGACY_PUBLIC_KEY_SIZE)?.to_vec(),
        )?;
        let signing_type = destination.signing_key().key_type();
        let signing_key = SigningPublicKey::new(
            signing_type,
            cursor
                .take(signing_type.public_key_len().ok_or_else(|| {
                    unsupported(0, "LeaseSet signing type", signing_type.code() as u64)
                })?)?
                .to_vec(),
        )?;
        let lease_count = usize::from(cursor.read_u8()?);
        if lease_count > MAX_LEASES {
            return Err(CodecError::PolicyRejected {
                offset: cursor.offset().saturating_sub(1),
                context: "LeaseSet lease count",
            });
        }
        let mut leases = Vec::with_capacity(lease_count);
        for _ in 0..lease_count {
            leases.push(Lease::decode_from(&mut cursor)?);
        }
        let signed_end = cursor.offset();
        let signature = SignatureValue::new(
            signing_type,
            cursor
                .take(signing_type.signature_len().ok_or_else(|| {
                    unsupported(0, "LeaseSet signature type", signing_type.code() as u64)
                })?)?
                .to_vec(),
        )?;
        cursor.finish()?;
        Ok(Self {
            destination,
            encryption_key,
            signing_key,
            leases,
            signed_bytes: input[..signed_end].to_vec(),
            signature,
        })
    }

    fn encode_unsigned(
        encoder: &mut EncodeBuffer<'_>,
        destination: &Destination,
        encryption_key: &PublicKey,
        signing_key: &SigningPublicKey,
        leases: &[Lease],
    ) -> Result<(), CodecError> {
        destination.keys.encode_into(encoder)?;
        encoder.write_raw(encryption_key.as_bytes())?;
        encoder.write_raw(signing_key.as_bytes())?;
        encoder.write_u8(
            u8::try_from(leases.len()).map_err(|_| invalid(0, "LeaseSet lease count"))?,
        )?;
        for lease in leases {
            lease.encode_into(encoder)?;
        }
        Ok(())
    }

    /// Encodes the exact retained signed region followed by its signature.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes)?;
            encoder.write_raw(self.signature.as_bytes())
        })
    }

    /// Returns the exact bytes covered by the LeaseSet signature.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }

    /// Returns the destination.
    pub fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the encryption key.
    pub fn encryption_key(&self) -> &PublicKey {
        &self.encryption_key
    }

    /// Returns the revocation key material.
    pub fn signing_key(&self) -> &SigningPublicKey {
        &self.signing_key
    }

    /// Returns the leases.
    pub fn leases(&self) -> &[Lease] {
        &self.leases
    }

    /// Returns the typed signature.
    pub fn signature(&self) -> &SignatureValue {
        &self.signature
    }
}

/// LeaseSet-family variants deliberately deferred until their later crypto and
/// NetDB plans define complete semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeferredLeaseSetVariant {
    /// LeaseSet2 and its offline-signature/header semantics.
    LeaseSet2,
    /// MetaLeaseSet records.
    MetaLeaseSet,
    /// EncryptedLeaseSet records.
    EncryptedLeaseSet,
}

/// Decodes only classic LeaseSets from a DatabaseStore type and rejects the
/// later variants explicitly rather than guessing their wire layout.
pub fn decode_lease_set_variant(
    store_type: u8,
    input: &[u8],
    maximum: usize,
) -> Result<LeaseSet, CodecError> {
    match store_type {
        1 => LeaseSet::decode(input, maximum),
        3 => Err(unsupported(0, "LeaseSet2 variant", 3)),
        5 => Err(unsupported(0, "EncryptedLeaseSet variant", 5)),
        7 => Err(unsupported(0, "MetaLeaseSet variant", 7)),
        other => Err(unsupported(
            0,
            "LeaseSet DatabaseStore type",
            u64::from(other),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

    fn key_and_cert(signing_type: SigningKeyType, crypto_type: CryptoKeyType) -> KeyAndCert {
        let public_len = crypto_type.public_key_len().unwrap();
        let signing_len = signing_type.public_key_len().unwrap();
        let certificate =
            if signing_type == SigningKeyType::DsaSha1 && crypto_type == CryptoKeyType::ElGamal {
                Certificate::Null
            } else {
                Certificate::Key(KeyCertificate::for_types(signing_type, crypto_type).unwrap())
            };
        let padding_len = KEY_AREA_SIZE - public_len - signing_len.min(LEGACY_SIGNING_KEY_SIZE);
        KeyAndCert::new(
            PublicKey::new(crypto_type, vec![0x11; public_len]).unwrap(),
            SigningPublicKey::new(signing_type, vec![0x22; signing_len]).unwrap(),
            vec![0x33; padding_len],
            certificate,
        )
        .unwrap()
    }

    fn ed_router_identity() -> RouterIdentity {
        RouterIdentity::new(key_and_cert(
            SigningKeyType::EdDsaSha512Ed25519,
            CryptoKeyType::X25519,
        ))
        .unwrap()
    }

    fn ed_destination() -> Destination {
        Destination::new(key_and_cert(
            SigningKeyType::EdDsaSha512Ed25519,
            CryptoKeyType::X25519,
        ))
        .unwrap()
    }

    #[test]
    fn fixed_mapping_encoding_is_canonical_and_sorted() {
        let mapping = Mapping::from_entries(vec![
            ("b".to_owned(), "2".to_owned()),
            ("a".to_owned(), "1".to_owned()),
        ])
        .unwrap();
        let expected = b"\x00\x0c\x01a=\x011;\x01b=\x012;";
        assert_eq!(mapping.encode_to_vec(MAX).unwrap(), expected);
        assert_eq!(Mapping::decode(expected, MAX).unwrap(), mapping);
    }

    #[test]
    fn mapping_rejects_duplicates_and_noncanonical_order() {
        let duplicate = b"\x00\x0c\x01a=\x011;\x01a=\x012;";
        assert!(matches!(
            Mapping::decode(duplicate, MAX),
            Err(CodecError::DuplicateField { .. })
        ));
        let unsorted = b"\x00\x0c\x01b=\x011;\x01a=\x012;";
        assert!(matches!(
            Mapping::decode(unsorted, MAX),
            Err(CodecError::NonCanonical { .. })
        ));
    }

    #[test]
    fn primitive_vectors_and_unknown_algorithm_paths_are_explicit() {
        assert_eq!(
            Date::from_millis(0x0102_0304_0506_0708)
                .encode_to_vec(8)
                .unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(
            Hash::digest(b"").as_bytes(),
            &[
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]
        );
        assert_eq!(
            SigningKeyType::from_code(0x1234),
            SigningKeyType::Unknown(0x1234)
        );
        assert!(matches!(
            PublicKey::new(CryptoKeyType::Unknown(55), vec![]),
            Err(CodecError::Unsupported { .. })
        ));
        assert!(matches!(
            Certificate::decode(&[5, 0, 4, 0x12, 0x34, 0, 4], MAX),
            Err(CodecError::Unsupported { .. })
        ));
    }

    #[test]
    fn key_certificate_excess_signing_material_round_trips() {
        let signing_type = SigningKeyType::EcdsaSha512P521;
        let crypto_type = CryptoKeyType::ElGamal;
        let signing_bytes = (0..132).map(|value| value as u8).collect::<Vec<_>>();
        let certificate = Certificate::Key(
            KeyCertificate::new(
                signing_type,
                crypto_type,
                signing_bytes[128..].to_vec(),
                Vec::new(),
            )
            .unwrap(),
        );
        let keys = KeyAndCert::new(
            PublicKey::new(crypto_type, vec![0x10; 256]).unwrap(),
            SigningPublicKey::new(signing_type, signing_bytes).unwrap(),
            Vec::new(),
            certificate,
        )
        .unwrap();
        let encoded = keys.encode_to_vec(MAX).unwrap();
        assert_eq!(KeyAndCert::decode(&encoded, MAX).unwrap(), keys);
    }

    #[test]
    fn key_certificate_identity_and_destination_round_trip() {
        let identity = ed_router_identity();
        let encoded = identity.encode_to_vec(MAX).unwrap();
        assert_eq!(encoded.len(), 391);
        assert_eq!(RouterIdentity::decode(&encoded, MAX).unwrap(), identity);
        assert_eq!(
            Destination::decode(&ed_destination().encode_to_vec(MAX).unwrap(), MAX).unwrap(),
            ed_destination()
        );
        assert_ne!(identity.hash().unwrap(), Hash::digest(b""));
    }

    #[test]
    fn identity_truncation_is_rejected_at_every_boundary() {
        let encoded = ed_router_identity().encode_to_vec(MAX).unwrap();
        for end in 0..encoded.len() {
            assert!(
                RouterIdentity::decode(&encoded[..end], MAX).is_err(),
                "prefix {end}"
            );
        }
    }

    #[test]
    fn router_address_round_trip_and_typed_options() {
        let mut builder = Mapping::builder();
        builder
            .insert("host".to_owned(), "127.0.0.1".to_owned())
            .unwrap();
        builder
            .insert("port".to_owned(), "1234".to_owned())
            .unwrap();
        let address = RouterAddress::new(
            10,
            Date::from_millis(0),
            "NTCP2".to_owned(),
            builder.build().unwrap(),
        )
        .unwrap();
        let encoded = address.encode_to_vec(MAX).unwrap();
        assert_eq!(RouterAddress::decode(&encoded, MAX).unwrap(), address);
    }

    #[test]
    fn router_info_retains_signed_region_and_rejects_trailing_bytes() {
        let identity = ed_router_identity();
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), "Nf".to_owned()).unwrap();
        options
            .insert("router.version".to_owned(), "0.9.68".to_owned())
            .unwrap();
        let info = RouterInfo::new(
            identity,
            Date::from_millis(123),
            Vec::new(),
            Vec::new(),
            options.build().unwrap(),
            SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x44; 64]).unwrap(),
        )
        .unwrap();
        let encoded = info.encode_to_vec(MAX).unwrap();
        let decoded = RouterInfo::decode(&encoded, MAX).unwrap();
        assert_eq!(decoded.signed_bytes(), info.signed_bytes());
        assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
        assert_eq!(
            decoded.protocol_version().unwrap().unwrap().as_str(),
            "0.9.68"
        );
        assert_eq!(decoded.capabilities().unwrap().unwrap().as_str(), "Nf");
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(matches!(
            RouterInfo::decode(&trailing, MAX),
            Err(CodecError::Truncated { .. }) | Err(CodecError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn lease_and_classic_leaseset_round_trip() {
        let destination = ed_destination();
        let lease = Lease::new(Hash::from_bytes([0x55; 32]), 7, Date::from_millis(99));
        let lease_bytes = lease.encode_to_vec(MAX).unwrap();
        assert_eq!(lease_bytes.len(), 44);
        assert_eq!(Lease::decode(&lease_bytes, MAX).unwrap(), lease);
        let set = LeaseSet::new(
            destination,
            PublicKey::new(CryptoKeyType::ElGamal, vec![0x66; 256]).unwrap(),
            SigningPublicKey::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x77; 32]).unwrap(),
            vec![lease],
            SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x88; 64]).unwrap(),
        )
        .unwrap();
        let encoded = set.encode_to_vec(MAX).unwrap();
        let decoded = LeaseSet::decode(&encoded, MAX).unwrap();
        assert_eq!(decoded.signed_bytes(), set.signed_bytes());
        assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
        assert!(matches!(
            decode_lease_set_variant(3, &encoded, MAX),
            Err(CodecError::Unsupported { value: 3, .. })
        ));
    }
}
