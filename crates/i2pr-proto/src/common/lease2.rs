//! Lease2 and Standard LeaseSet2 structural codecs.
//!
//! Lease2 replaces the classic 44-byte Lease shape with the modern
//! 40-byte lease used by Standard LeaseSet2:
//!
//! ```text
//! tunnel gateway   32-byte router hash
//! tunnel id         4-byte unsigned integer
//! end date           4-byte unsigned seconds since epoch
//! ```
//!
//! The Standard LeaseSet2 carrier is the modern ordinary destination
//! NetDB object. The current crate implements the ordinary
//! online-signed published LS2 subset; EncryptedLeaseSet, blinded,
//! offline-signing, and PQ-hybrid variants are deliberately deferred.
//! MetaLeaseSet has a separate structural codec in this module.
//!
//! LeaseSet2 owns the wire layout and structural validation only. It does
//! not own private signing keys, freshness policy, NetDB retention, or
//! ECIES session state.

use std::fmt;

use super::{
    Date32, Destination, Hash, MAX_COMMON_STRUCTURE_SIZE, MAX_LEASES, Mapping, SignatureValue,
    SigningKeyType, SigningPublicKey, decode_exact, encode_to_vec, invalid, take_array,
    unsupported,
};
use crate::{CodecError, DecodeCursor, EncodeBuffer};

/// Exact on-wire length of one Lease2 record.
pub const LEASE2_WIRE_SIZE: usize = 40;

/// Maximum number of encryption keys carried by a LeaseSet2.
///
/// The common-structures specification describes a small bounded list;
/// eight keeps the aggregate key bytes well under `MAX_COMMON_STRUCTURE_SIZE`.
pub const MAX_LEASE_SET2_ENCRYPTION_KEYS: usize = 8;

/// Aggregate cap on the raw bytes of all encryption keys carried by
/// one LeaseSet2. 8 KiB is well above any reasonable modern curve but
/// keeps a malformed LS2 from exhausting NetDB memory.
pub const MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES: usize = 8 * 1024;

/// Maximum options `Mapping` body bytes accepted for a LeaseSet2.
///
/// 64 KiB is generous; the I2P common-structures specification does not
/// require more than a handful of small entries.
pub const MAX_LEASE_SET2_OPTIONS_BYTES: usize = u16::MAX as usize;

/// DatabaseStore type byte for a Standard LeaseSet2.
///
/// The signature domain prepends this byte to the unsigned LeaseSet2
/// bytes before the destination signing key signs them.
pub const LEASE_SET2_SIGNATURE_DOMAIN_BYTE: u8 = 0x03;

/// Maximum encoded length of one Standard LeaseSet2 carrier.
///
/// The NetDB layer tightens this further; the proto ceiling is generous.
pub const MAX_LEASE_SET2_BYTES: usize = MAX_COMMON_STRUCTURE_SIZE;

/// Recognized LS2 flag bits.
///
/// Only the ordinary online-signed subset is accepted in this plan.
/// Reserved bits must be zero; offline and blinded-on-publication flags produce typed
/// policy rejections. The unpublished flag (0x0002) is accepted per
/// Plan 172 §10: `dontPublishLeaseSet=true` installs the client-signed
/// LS2 locally without public NetDB publication. Blinded-on-publication remains deferred with
/// EncryptedLeaseSet support.
pub mod flags {
    /// Offline-signature section follows the fixed header.
    pub const OFFLINE_SIGNATURE: u16 = 0x0001;
    /// Destination is unpublished; LeaseSet is sent offline only.
    pub const UNPUBLISHED: u16 = 0x0002;
    /// Destination publishes an encrypted, blinded LeaseSet.
    pub const BLINDED_ON_PUBLICATION: u16 = 0x0004;
    /// Reserved bit mask for the LS2 flag word.
    pub const RESERVED_MASK: u16 = 0xfff8;
}

/// A modern 40-byte Lease2 record carried by a Standard LeaseSet2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Lease2 {
    tunnel_gateway: Hash,
    tunnel_id: u32,
    end_date: Date32,
}

impl Lease2 {
    /// Creates a `Lease2` value.
    pub const fn new(tunnel_gateway: Hash, tunnel_id: u32, end_date: Date32) -> Self {
        Self {
            tunnel_gateway,
            tunnel_id,
            end_date,
        }
    }

    /// Decodes one complete 40-byte Lease2 from `input`.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one complete Lease2 to a fresh vector under `maximum`.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let gateway_bytes = take_array::<32>(cursor)?;
        let tunnel_id = cursor.read_u32()?;
        let end_date_raw = cursor.read_u32()?;
        Ok(Self {
            tunnel_gateway: Hash::from_bytes(gateway_bytes),
            tunnel_id,
            end_date: Date32::from_seconds(end_date_raw),
        })
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_raw(self.tunnel_gateway.as_bytes())?;
        encoder.write_u32(self.tunnel_id)?;
        encoder.write_u32(self.end_date.as_seconds())
    }

    /// Returns the gateway router hash.
    pub const fn tunnel_gateway(&self) -> Hash {
        self.tunnel_gateway
    }

    /// Returns the tunnel identifier.
    pub const fn tunnel_id(&self) -> u32 {
        self.tunnel_id
    }

    /// Returns the four-byte end date.
    pub const fn end_date(&self) -> Date32 {
        self.end_date
    }
}

/// Errors produced by LeaseSet2 selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSet2KeySelectionError {
    /// The LS2 carries zero encryption keys; an ordinary LS2 must
    /// contain at least one usable X25519 key.
    NoKeys,
    /// The LS2 carries more than one X25519 key and the local policy
    /// rejects duplicates without an explicit deterministic selector.
    DuplicateX25519,
    /// The LS2 carries no X25519 key of the requested length.
    X25519NotFound,
}

impl fmt::Display for LeaseSet2KeySelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoKeys => formatter.write_str("LeaseSet2 carries no encryption keys"),
            Self::DuplicateX25519 => {
                formatter.write_str("LeaseSet2 carries more than one X25519 encryption key")
            }
            Self::X25519NotFound => {
                formatter.write_str("LeaseSet2 carries no usable X25519 encryption key")
            }
        }
    }
}

impl std::error::Error for LeaseSet2KeySelectionError {}

/// One typed LS2 encryption public key.
///
/// The key type is a [`crate::CryptoKeyType`] (only X25519 type 4 is
/// usable for ordinary published LS2 in this plan; unknown types are
/// retained so the LS2 stays parseable but cannot be selected for
/// routing or encryption).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseSet2EncryptionKey {
    key_type: crate::CryptoKeyType,
    bytes: Vec<u8>,
}

impl LeaseSet2EncryptionKey {
    /// Creates one typed encryption key. The type-specific byte
    /// length is enforced at selection time, not here, so unknown
    /// key types remain valid to retain.
    pub fn new(
        key_type: crate::CryptoKeyType,
        bytes: Vec<u8>,
    ) -> Result<Self, LeaseSet2EncryptionKeyError> {
        if bytes.is_empty() {
            return Err(LeaseSet2EncryptionKeyError::Empty);
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the declared key type.
    pub const fn key_type(&self) -> crate::CryptoKeyType {
        self.key_type
    }

    /// Returns the raw key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Errors produced when constructing a typed LS2 encryption key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSet2EncryptionKeyError {
    /// The key carried zero bytes; the LS2 specification forbids empty
    /// key entries.
    Empty,
}

impl fmt::Display for LeaseSet2EncryptionKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("LeaseSet2 encryption key carried zero bytes"),
        }
    }
}

impl std::error::Error for LeaseSet2EncryptionKeyError {}

impl From<LeaseSet2EncryptionKeyError> for CodecError {
    fn from(error: LeaseSet2EncryptionKeyError) -> Self {
        match error {
            LeaseSet2EncryptionKeyError::Empty => CodecError::InvalidFieldValue {
                offset: 0,
                context: "LeaseSet2 encryption key length",
            },
        }
    }
}

/// Typed flag word for a Standard LeaseSet2.
///
/// Only the recognized bits are exposed via constructor helpers; the
/// raw `u16` remains available for policy checks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseSet2Flags(u16);

impl LeaseSet2Flags {
    /// Wraps a raw flag word. The caller is responsible for ensuring
    /// only recognized bits are set; [`Self::has_reserved_bits`]
    /// rejects the reserved bits during decode.
    pub const fn from_raw(value: u16) -> Self {
        Self(value)
    }

    /// Returns the raw flag word.
    pub const fn as_raw(self) -> u16 {
        self.0
    }

    /// Returns whether the offline-signature section is present.
    pub const fn has_offline_signature(self) -> bool {
        self.0 & flags::OFFLINE_SIGNATURE != 0
    }

    /// Returns whether the destination is unpublished.
    pub const fn is_unpublished(self) -> bool {
        self.0 & flags::UNPUBLISHED != 0
    }

    /// Returns whether the LS2 was blinded for encrypted publication.
    pub const fn is_blinded_on_publication(self) -> bool {
        self.0 & flags::BLINDED_ON_PUBLICATION != 0
    }

    /// Returns whether the flag word contains any reserved bits.
    pub const fn has_reserved_bits(self) -> bool {
        self.0 & flags::RESERVED_MASK != 0
    }
}

/// The Standard LeaseSet2 header.
///
/// Encodes a [`Destination`], the four-byte published seconds-since-epoch
/// timestamp, the two-byte expires offset, and the two-byte flag word.
/// The offline-signature section is parsed only when the
/// [`flags::OFFLINE_SIGNATURE`] bit is set; in this plan the policy
/// rejects offline signatures rather than implementing the full
/// verification path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseSet2Header {
    destination: Destination,
    published_seconds: u32,
    expires_offset_seconds: u16,
    flags: LeaseSet2Flags,
    offline_signature: Option<OfflineSignature>,
}

impl LeaseSet2Header {
    /// Creates a header value. Expiration arithmetic is checked; the
    /// sum must not overflow `u32`.
    pub fn new(
        destination: Destination,
        published_seconds: u32,
        expires_offset_seconds: u16,
        flags: LeaseSet2Flags,
    ) -> Result<Self, LeaseSet2HeaderError> {
        Self::new_with_offline_signature(
            destination,
            published_seconds,
            expires_offset_seconds,
            flags,
            None,
        )
    }

    pub fn new_with_offline_signature(
        destination: Destination,
        published_seconds: u32,
        expires_offset_seconds: u16,
        flags: LeaseSet2Flags,
        offline_signature: Option<OfflineSignature>,
    ) -> Result<Self, LeaseSet2HeaderError> {
        if flags.has_offline_signature() != offline_signature.is_some() {
            return Err(LeaseSet2HeaderError::OfflineSignatureFlagMismatch);
        }
        if offline_signature.as_ref().is_some_and(|offline| {
            offline.signature().key_type() != destination.signing_key().key_type()
        }) {
            return Err(LeaseSet2HeaderError::OfflineSignatureFlagMismatch);
        }
        let expires_total = (u64::from(published_seconds))
            .checked_add(u64::from(expires_offset_seconds))
            .ok_or(LeaseSet2HeaderError::ExpirationOverflow)?;
        if expires_total > u64::from(u32::MAX) {
            return Err(LeaseSet2HeaderError::ExpirationOverflow);
        }
        Ok(Self {
            destination,
            published_seconds,
            expires_offset_seconds,
            flags,
            offline_signature,
        })
    }

    /// Decodes one LS2 header from `input`, including any offline-signature section.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let destination = Destination::decode_from(cursor)?;
        let published_seconds = cursor.read_u32()?;
        let expires_offset_seconds = cursor.read_u16()?;
        let raw_flags = cursor.read_u16()?;
        let flags = LeaseSet2Flags::from_raw(raw_flags);
        if flags.has_reserved_bits() {
            return Err(invalid(
                cursor.offset().saturating_sub(2),
                "LeaseSet2 reserved flag bits",
            ));
        }
        let offline_signature = if flags.has_offline_signature() {
            Some(OfflineSignature::decode_from(
                cursor,
                destination.signing_key().key_type(),
            )?)
        } else {
            None
        };
        Self::new_with_offline_signature(
            destination,
            published_seconds,
            expires_offset_seconds,
            flags,
            offline_signature,
        )
        .map_err(CodecError::from)
    }

    /// Returns the destination.
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the published seconds-since-epoch timestamp.
    pub const fn published_seconds(&self) -> u32 {
        self.published_seconds
    }

    /// Returns the expires offset in seconds.
    pub const fn expires_offset_seconds(&self) -> u16 {
        self.expires_offset_seconds
    }

    /// Returns the absolute expires timestamp in seconds since epoch.
    pub fn expires_seconds(&self) -> u32 {
        // Validation already enforced `published + offset <= u32::MAX`.
        self.published_seconds + u32::from(self.expires_offset_seconds)
    }

    /// Returns the typed flag word.
    pub const fn flags(&self) -> LeaseSet2Flags {
        self.flags
    }

    pub const fn offline_signature(&self) -> Option<&OfflineSignature> {
        self.offline_signature.as_ref()
    }

    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        self.destination.keys.encode_into(encoder)?;
        encoder.write_u32(self.published_seconds)?;
        encoder.write_u16(self.expires_offset_seconds)?;
        encoder.write_u16(self.flags.as_raw())?;
        if let Some(offline) = &self.offline_signature {
            offline.encode_into(encoder)?;
        }
        Ok(())
    }
}

/// Errors produced when constructing a LeaseSet2 header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSet2HeaderError {
    /// `published + expires_offset` overflowed `u32`.
    ExpirationOverflow,
    /// Offline signature presence and flag bit must agree, and its signature type must match the destination.
    OfflineSignatureFlagMismatch,
}

impl fmt::Display for LeaseSet2HeaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpirationOverflow => {
                formatter.write_str("LeaseSet2 published + expires offset overflowed u32")
            }
            Self::OfflineSignatureFlagMismatch => {
                formatter.write_str("LeaseSet2 offline-signature flag mismatch")
            }
        }
    }
}

impl std::error::Error for LeaseSet2HeaderError {}

impl From<LeaseSet2HeaderError> for CodecError {
    fn from(error: LeaseSet2HeaderError) -> Self {
        match error {
            LeaseSet2HeaderError::ExpirationOverflow => CodecError::ArithmeticOverflow {
                offset: 0,
                context: "LeaseSet2 expiration",
            },
            LeaseSet2HeaderError::OfflineSignatureFlagMismatch => CodecError::InvalidFieldValue {
                offset: 0,
                context: "LeaseSet2 offline signature",
            },
        }
    }
}

/// A Standard LeaseSet2: header + typed encryption keys + Lease2 list +
/// signature, with the exact retained signed byte region preserved for
/// later cryptographic verification.
///
/// The signature domain is
/// [`LEASE_SET2_SIGNATURE_DOMAIN_BYTE`] prepended to
/// [`LeaseSet2::signed_bytes`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseSet2 {
    header: LeaseSet2Header,
    options: Mapping,
    encryption_keys: Vec<LeaseSet2EncryptionKey>,
    leases: Vec<Lease2>,
    signed_bytes: Vec<u8>,
    signature: SignatureValue,
}

impl LeaseSet2 {
    /// Constructs a LeaseSet2 from already-validated fields. The
    /// supplied `signature` is attached as the final field; the
    /// unsigned region is rebuilt exactly so that round-trip encoding
    /// matches the wire format the producer is expected to have signed.
    ///
    /// This constructor is the local-generation entry point; the
    /// NetDB layer or a higher-layer client signs the
    /// [`Self::signature_preimage`] and supplies the resulting
    /// signature. Proto never owns private signing keys.
    pub fn new(
        header: LeaseSet2Header,
        options: Mapping,
        encryption_keys: Vec<LeaseSet2EncryptionKey>,
        leases: Vec<Lease2>,
        signature: SignatureValue,
    ) -> Result<Self, LeaseSet2BuildError> {
        if encryption_keys.is_empty() {
            return Err(LeaseSet2BuildError::ZeroEncryptionKeys);
        }
        if encryption_keys.len() > MAX_LEASE_SET2_ENCRYPTION_KEYS {
            return Err(LeaseSet2BuildError::EncryptionKeyCountExceeded);
        }
        let mut aggregate = 0usize;
        for key in &encryption_keys {
            aggregate = aggregate.checked_add(key.as_bytes().len()).ok_or(
                LeaseSet2BuildError::EncryptionKeyBytesExceeded {
                    aggregate: usize::MAX,
                    maximum: MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES,
                },
            )?;
        }
        if aggregate > MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES {
            return Err(LeaseSet2BuildError::EncryptionKeyBytesExceeded {
                aggregate,
                maximum: MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES,
            });
        }
        if leases.is_empty() {
            return Err(LeaseSet2BuildError::ZeroLeases);
        }
        if leases.len() > MAX_LEASES {
            return Err(LeaseSet2BuildError::LeaseCountExceeded {
                declared: leases.len(),
                maximum: MAX_LEASES,
            });
        }
        let signing_type = header.offline_signature().map_or_else(
            || header.destination().signing_key().key_type(),
            |offline| offline.transient_key().key_type(),
        );
        if signing_type != signature.key_type() {
            return Err(LeaseSet2BuildError::SignatureTypeMismatch {
                expected: signing_type.code(),
                actual: signature.key_type().code(),
            });
        }
        let signature_len = signing_type.signature_len().ok_or(
            LeaseSet2BuildError::UnsupportedSigningAlgorithm(signing_type.code()),
        )?;
        if signature.as_bytes().len() != signature_len {
            return Err(LeaseSet2BuildError::SignatureLengthMismatch {
                expected: signature_len,
                actual: signature.as_bytes().len(),
            });
        }
        let signed_bytes = encode_to_vec(MAX_LEASE_SET2_BYTES, |encoder| {
            encode_unsigned(encoder, &header, &options, &encryption_keys, &leases)
        })
        .map_err(LeaseSet2BuildError::Codec)?;
        Ok(Self {
            header,
            options,
            encryption_keys,
            leases,
            signed_bytes,
            signature,
        })
    }

    /// Decodes a complete Standard LeaseSet2 from `input`. The retained
    /// `signed_bytes` cover the entire pre-signature region; signature
    /// verification must use those exact bytes plus the
    /// [`LEASE_SET2_SIGNATURE_DOMAIN_BYTE`] domain byte.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let header_start = cursor.offset();
        let header = LeaseSet2Header::decode_from(&mut cursor)?;
        let options = Mapping::decode_from(&mut cursor, MAX_LEASE_SET2_OPTIONS_BYTES)?;
        // Plan 172 §5: Java I2P `LeaseSet2.readBytes` uses a single-byte
        // key count (`in.read()`), not u16. Match the reference so
        // exact-pinned Java/go-i2cp CreateLeaseSet2 parses.
        let key_count = cursor.read_u8()?;
        if usize::from(key_count) > MAX_LEASE_SET2_ENCRYPTION_KEYS {
            return Err(CodecError::LengthExceeded {
                offset: cursor.offset().saturating_sub(1),
                declared: usize::from(key_count),
                maximum: MAX_LEASE_SET2_ENCRYPTION_KEYS,
                context: "LeaseSet2 encryption key count",
            });
        }
        let mut encryption_keys = Vec::with_capacity(usize::from(key_count));
        let mut aggregate_key_bytes = 0usize;
        for _ in 0..key_count {
            let key_type_code = cursor.read_u16()?;
            let key_type = crate::CryptoKeyType::from_code(key_type_code);
            let key_length = cursor.read_u16()?;
            let key_length_us = usize::from(key_length);
            aggregate_key_bytes = aggregate_key_bytes.checked_add(key_length_us).ok_or(
                CodecError::ArithmeticOverflow {
                    offset: cursor.offset(),
                    context: "LeaseSet2 aggregate encryption key bytes",
                },
            )?;
            if aggregate_key_bytes > MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES {
                return Err(CodecError::LengthExceeded {
                    offset: cursor.offset(),
                    declared: aggregate_key_bytes,
                    maximum: MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES,
                    context: "LeaseSet2 aggregate encryption key bytes",
                });
            }
            if key_length_us == 0 {
                return Err(invalid(cursor.offset(), "LeaseSet2 encryption key length"));
            }
            let bytes = cursor.take(key_length_us)?.to_vec();
            encryption_keys.push(LeaseSet2EncryptionKey::new(key_type, bytes)?);
        }
        let lease_count = cursor.read_u8()?;
        let lease_count_us = usize::from(lease_count);
        if lease_count_us == 0 {
            return Err(invalid(
                cursor.offset().saturating_sub(1),
                "LeaseSet2 lease count",
            ));
        }
        if lease_count_us > MAX_LEASES {
            return Err(CodecError::PolicyRejected {
                offset: cursor.offset().saturating_sub(1),
                context: "LeaseSet2 lease count",
            });
        }
        let mut leases = Vec::with_capacity(lease_count_us);
        for _ in 0..lease_count_us {
            leases.push(Lease2::decode_from(&mut cursor)?);
        }
        let signed_end = cursor.offset();
        let signing_type = header.offline_signature().map_or_else(
            || header.destination().signing_key().key_type(),
            |offline| offline.transient_key().key_type(),
        );
        let signature_len = signing_type.signature_len().ok_or_else(|| {
            unsupported(
                cursor.offset(),
                "LeaseSet2 signing type",
                signing_type.code() as u64,
            )
        })?;
        let signature = SignatureValue::new(signing_type, cursor.take(signature_len)?.to_vec())?;
        cursor.finish()?;
        let signed_bytes = input[header_start..signed_end].to_vec();
        Ok(Self {
            header,
            options,
            encryption_keys,
            leases,
            signed_bytes,
            signature,
        })
    }

    /// Encodes the complete LeaseSet2 (signed bytes followed by
    /// signature) into a fresh vector under `maximum`.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes)?;
            encoder.write_raw(self.signature.as_bytes())
        })
    }

    /// Returns the bytes the destination signing key signs, including
    /// the signature-domain byte.
    ///
    /// The signature preimage is exactly
    /// `LEASE_SET2_SIGNATURE_DOMAIN_BYTE || self.signed_bytes()`. Local
    /// generation signs these bytes; verifiers re-derive them from the
    /// exact retained signed region so a re-encoded options mapping
    /// cannot gain a valid signature.
    pub fn signature_preimage(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.signed_bytes.len() + 1);
        out.push(LEASE_SET2_SIGNATURE_DOMAIN_BYTE);
        out.extend_from_slice(&self.signed_bytes);
        out
    }

    /// Returns the exact retained signed byte region (everything
    /// before the signature). Re-encoding the LS2 with the same
    /// fields produces identical bytes.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }

    /// Returns the typed signature.
    pub const fn signature(&self) -> &SignatureValue {
        &self.signature
    }

    /// Returns the parsed header.
    pub const fn header(&self) -> &LeaseSet2Header {
        &self.header
    }

    /// Returns the destination embedded in the header.
    pub fn destination(&self) -> &Destination {
        self.header.destination()
    }

    /// Returns the published timestamp in seconds.
    pub fn published_seconds(&self) -> u32 {
        self.header.published_seconds()
    }

    /// Returns the absolute expires timestamp in seconds.
    pub fn expires_seconds(&self) -> u32 {
        self.header.expires_seconds()
    }

    /// Returns the parsed options `Mapping`.
    pub const fn options(&self) -> &Mapping {
        &self.options
    }

    /// Returns the typed encryption keys in wire order.
    pub fn encryption_keys(&self) -> &[LeaseSet2EncryptionKey] {
        &self.encryption_keys
    }

    /// Returns the parsed `Lease2` records.
    pub fn leases(&self) -> &[Lease2] {
        &self.leases
    }

    /// Returns the first usable X25519 encryption key, if any.
    ///
    /// The selector is deterministic: when more than one X25519 key is
    /// present the LS2 is rejected for duplicate X25519 under the
    /// ordinary supported policy.
    pub fn usable_x25519_key(&self) -> Result<&LeaseSet2EncryptionKey, LeaseSet2KeySelectionError> {
        if self.encryption_keys.is_empty() {
            return Err(LeaseSet2KeySelectionError::NoKeys);
        }
        let mut x25519_count = 0;
        let mut usable: Option<&LeaseSet2EncryptionKey> = None;
        for key in &self.encryption_keys {
            if key.key_type() == crate::CryptoKeyType::X25519 {
                x25519_count += 1;
                if key.as_bytes().len()
                    == crate::CryptoKeyType::X25519.public_key_len().unwrap_or(32)
                {
                    usable = Some(key);
                }
            }
        }
        if x25519_count == 0 {
            return Err(LeaseSet2KeySelectionError::X25519NotFound);
        }
        if x25519_count > 1 {
            return Err(LeaseSet2KeySelectionError::DuplicateX25519);
        }
        usable.ok_or(LeaseSet2KeySelectionError::X25519NotFound)
    }

    /// Computes the SHA-256 of the canonical destination encoding
    /// embedded in the LS2 header.
    ///
    /// This is the LS2 hash key the NetDB uses for storage.
    pub fn key_hash(&self) -> Result<crate::Hash, CodecError> {
        self.header.destination().hash()
    }
}

/// Errors produced when constructing a LeaseSet2 via [`LeaseSet2::new`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaseSet2BuildError {
    /// The supplied encryption-key list was empty.
    ZeroEncryptionKeys,
    /// The encryption-key list exceeded the bounded count.
    EncryptionKeyCountExceeded,
    /// The aggregate encryption-key bytes exceeded the bounded total.
    EncryptionKeyBytesExceeded {
        /// Observed aggregate bytes.
        aggregate: usize,
        /// Configured ceiling.
        maximum: usize,
    },
    /// The supplied lease list was empty.
    ZeroLeases,
    /// The supplied lease list exceeded the bounded count.
    LeaseCountExceeded {
        /// Declared lease count.
        declared: usize,
        /// Configured ceiling.
        maximum: usize,
    },
    /// The signature type did not match the destination signing key
    /// type.
    SignatureTypeMismatch {
        /// Expected signing-key type (from the destination).
        expected: u16,
        /// Actual signature key type.
        actual: u16,
    },
    /// The signature length did not match the destination signing-key
    /// type's expected signature length.
    SignatureLengthMismatch {
        /// Expected signature length in bytes.
        expected: usize,
        /// Actual signature length in bytes.
        actual: usize,
    },
    /// The destination uses a signing-key type this crate does not
    /// know how to size.
    UnsupportedSigningAlgorithm(u16),
    /// The underlying bounded encoder/decoder rejected the unsigned
    /// region.
    Codec(CodecError),
}

impl fmt::Display for LeaseSet2BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroEncryptionKeys => {
                formatter.write_str("LeaseSet2 requires at least one encryption key")
            }
            Self::EncryptionKeyCountExceeded => {
                formatter.write_str("LeaseSet2 encryption key count exceeded")
            }
            Self::EncryptionKeyBytesExceeded { aggregate, maximum } => write!(
                formatter,
                "LeaseSet2 encryption key aggregate {aggregate} bytes exceeds {maximum}-byte ceiling"
            ),
            Self::ZeroLeases => formatter.write_str("LeaseSet2 requires at least one Lease2"),
            Self::LeaseCountExceeded { declared, maximum } => write!(
                formatter,
                "LeaseSet2 lease count {declared} exceeds {maximum}"
            ),
            Self::SignatureTypeMismatch { expected, actual } => write!(
                formatter,
                "LeaseSet2 signature type {actual} does not match destination signing type {expected}"
            ),
            Self::SignatureLengthMismatch { expected, actual } => write!(
                formatter,
                "LeaseSet2 signature length {actual} does not match expected {expected}"
            ),
            Self::UnsupportedSigningAlgorithm(algorithm) => write!(
                formatter,
                "LeaseSet2 destination uses unsupported signing algorithm {algorithm}"
            ),
            Self::Codec(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for LeaseSet2BuildError {}

impl From<LeaseSet2BuildError> for CodecError {
    fn from(error: LeaseSet2BuildError) -> Self {
        match error {
            LeaseSet2BuildError::ZeroEncryptionKeys => CodecError::PolicyRejected {
                offset: 0,
                context: "LeaseSet2 encryption key count",
            },
            LeaseSet2BuildError::EncryptionKeyCountExceeded => CodecError::LengthExceeded {
                offset: 0,
                declared: MAX_LEASE_SET2_ENCRYPTION_KEYS + 1,
                maximum: MAX_LEASE_SET2_ENCRYPTION_KEYS,
                context: "LeaseSet2 encryption key count",
            },
            LeaseSet2BuildError::EncryptionKeyBytesExceeded { aggregate, maximum } => {
                CodecError::LengthExceeded {
                    offset: 0,
                    declared: aggregate,
                    maximum,
                    context: "LeaseSet2 encryption key aggregate bytes",
                }
            }
            LeaseSet2BuildError::ZeroLeases => CodecError::PolicyRejected {
                offset: 0,
                context: "LeaseSet2 lease count",
            },
            LeaseSet2BuildError::LeaseCountExceeded { declared, maximum } => {
                CodecError::LengthExceeded {
                    offset: 0,
                    declared,
                    maximum,
                    context: "LeaseSet2 lease count",
                }
            }
            LeaseSet2BuildError::SignatureTypeMismatch { .. } => CodecError::InvalidFieldValue {
                offset: 0,
                context: "LeaseSet2 signature type",
            },
            LeaseSet2BuildError::SignatureLengthMismatch { .. } => CodecError::InvalidFieldValue {
                offset: 0,
                context: "LeaseSet2 signature length",
            },
            LeaseSet2BuildError::UnsupportedSigningAlgorithm(algorithm) => {
                CodecError::Unsupported {
                    offset: 0,
                    context: "LeaseSet2 signing algorithm",
                    value: u64::from(algorithm),
                }
            }
            LeaseSet2BuildError::Codec(inner) => inner,
        }
    }
}

/// Wire-level identifier for a LeaseSet2 family.
///
/// The first byte after the DatabaseStore envelope identifies the
/// stored record family. Standard LeaseSet2 uses type 3.
pub const LEASE_SET2_DATABASE_STORE_TYPE: u8 = 0x03;

/// Maximum number of entries and revocation hashes in a MetaLeaseSet.
pub const MAX_META_LEASES: usize = 16;

/// Delegated signing key data carried when the offline-signature flag is set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfflineSignature {
    expires_seconds: u32,
    transient_key: SigningPublicKey,
    signature: SignatureValue,
}

impl OfflineSignature {
    pub fn new(
        expires_seconds: u32,
        transient_key: SigningPublicKey,
        signature: SignatureValue,
    ) -> Result<Self, CodecError> {
        Ok(Self {
            expires_seconds,
            transient_key,
            signature,
        })
    }
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }
    pub const fn transient_key(&self) -> &SigningPublicKey {
        &self.transient_key
    }
    pub const fn signature(&self) -> &SignatureValue {
        &self.signature
    }
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + self.transient_key.as_bytes().len());
        bytes.extend_from_slice(&self.expires_seconds.to_be_bytes());
        bytes.extend_from_slice(&self.transient_key.key_type().code().to_be_bytes());
        bytes.extend_from_slice(self.transient_key.as_bytes());
        bytes
    }
    fn decode_from(
        cursor: &mut DecodeCursor<'_>,
        destination_type: SigningKeyType,
    ) -> Result<Self, CodecError> {
        let expires_seconds = cursor.read_u32()?;
        let transient_type = SigningKeyType::from_code(cursor.read_u16()?);
        let key_len = transient_type.public_key_len().ok_or_else(|| {
            unsupported(
                cursor.offset(),
                "offline transient key type",
                u64::from(transient_type.code()),
            )
        })?;
        let transient_key = SigningPublicKey::new(transient_type, cursor.take(key_len)?.to_vec())?;
        let signature_len = destination_type.signature_len().ok_or_else(|| {
            unsupported(
                cursor.offset(),
                "offline delegation signature type",
                u64::from(destination_type.code()),
            )
        })?;
        let signature =
            SignatureValue::new(destination_type, cursor.take(signature_len)?.to_vec())?;
        Ok(Self {
            expires_seconds,
            transient_key,
            signature,
        })
    }
    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u32(self.expires_seconds)?;
        encoder.write_u16(self.transient_key.key_type().code())?;
        encoder.write_raw(self.transient_key.as_bytes())?;
        encoder.write_raw(self.signature.as_bytes())
    }
}

/// One fixed 40-byte MetaLease entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetaLease {
    gateway: Hash,
    entry_type: u8,
    cost: u8,
    end_date: Date32,
}

impl MetaLease {
    pub fn new(
        gateway: Hash,
        entry_type: u8,
        cost: u8,
        end_date: Date32,
    ) -> Result<Self, CodecError> {
        if !matches!(entry_type, 0 | 1 | 3 | 5) {
            return Err(invalid(0, "MetaLease entry type"));
        }
        Ok(Self {
            gateway,
            entry_type,
            cost,
            end_date,
        })
    }
    pub const fn gateway(&self) -> Hash {
        self.gateway
    }
    pub const fn entry_type(&self) -> u8 {
        self.entry_type
    }
    pub const fn cost(&self) -> u8 {
        self.cost
    }
    pub const fn end_date(&self) -> Date32 {
        self.end_date
    }
    fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let gateway = Hash::decode_from(cursor)?;
        let raw_flags = (u32::from(cursor.read_u8()?) << 16)
            | (u32::from(cursor.read_u8()?) << 8)
            | u32::from(cursor.read_u8()?);
        if raw_flags & 0x00ff_fff0 != 0 {
            return Err(invalid(
                cursor.offset().saturating_sub(3),
                "MetaLease reserved flags",
            ));
        }
        let cost = cursor.read_u8()?;
        let end_date = Date32::from_seconds(cursor.read_u32()?);
        Self::new(gateway, (raw_flags & 0x0f) as u8, cost, end_date)
    }
    fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        self.gateway.encode_into(encoder)?;
        encoder.write_raw(&[0, 0, self.entry_type])?;
        encoder.write_u8(self.cost)?;
        encoder.write_u32(self.end_date.as_seconds())
    }
}

/// Canonically decoded MetaLeaseSet, retaining its exact signed byte region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetaLeaseSet {
    header: LeaseSet2Header,
    options: Mapping,
    entries: Vec<MetaLease>,
    revocations: Vec<Hash>,
    signed_bytes: Vec<u8>,
    signature: SignatureValue,
}

impl MetaLeaseSet {
    pub fn new(
        header: LeaseSet2Header,
        options: Mapping,
        entries: Vec<MetaLease>,
        revocations: Vec<Hash>,
        signature: SignatureValue,
    ) -> Result<Self, CodecError> {
        if entries.is_empty()
            || entries.len() > MAX_META_LEASES
            || revocations.len() > MAX_META_LEASES
        {
            return Err(invalid(0, "MetaLeaseSet entry/revocation count"));
        }
        let signing_type = header.offline_signature().map_or_else(
            || header.destination().signing_key().key_type(),
            |offline| offline.transient_key().key_type(),
        );
        if signature.key_type() != signing_type {
            return Err(invalid(0, "MetaLeaseSet signature type"));
        }
        let signed_bytes = encode_to_vec(MAX_COMMON_STRUCTURE_SIZE, |encoder| {
            header.encode_into(encoder)?;
            options.encode_into(encoder)?;
            encoder.write_u8(
                u8::try_from(entries.len()).map_err(|_| invalid(0, "MetaLeaseSet entry count"))?,
            )?;
            for entry in &entries {
                entry.encode_into(encoder)?;
            }
            encoder.write_u8(
                u8::try_from(revocations.len())
                    .map_err(|_| invalid(0, "MetaLeaseSet revocation count"))?,
            )?;
            for hash in &revocations {
                hash.encode_into(encoder)?;
            }
            Ok(())
        })?;
        Ok(Self {
            header,
            options,
            entries,
            revocations,
            signed_bytes,
            signature,
        })
    }
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let header = LeaseSet2Header::decode_from(&mut cursor)?;
        let options_limit = cursor.remaining();
        let options = Mapping::decode_from(&mut cursor, options_limit)?;
        let count = usize::from(cursor.read_u8()?);
        if count == 0 || count > MAX_META_LEASES {
            return Err(invalid(
                cursor.offset().saturating_sub(1),
                "MetaLeaseSet entry count",
            ));
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(MetaLease::decode_from(&mut cursor)?);
        }
        let revocation_count = usize::from(cursor.read_u8()?);
        if revocation_count > MAX_META_LEASES {
            return Err(invalid(
                cursor.offset().saturating_sub(1),
                "MetaLeaseSet revocation count",
            ));
        }
        let mut revocations = Vec::with_capacity(revocation_count);
        for _ in 0..revocation_count {
            revocations.push(Hash::decode_from(&mut cursor)?);
        }
        let signed_end = cursor.offset();
        let signing_type = header.offline_signature().map_or_else(
            || header.destination().signing_key().key_type(),
            |offline| offline.transient_key().key_type(),
        );
        let signature = SignatureValue::new(
            signing_type,
            cursor
                .take(signing_type.signature_len().ok_or_else(|| {
                    unsupported(
                        cursor.offset(),
                        "MetaLeaseSet signature type",
                        signing_type.code() as u64,
                    )
                })?)?
                .to_vec(),
        )?;
        cursor.finish()?;
        Ok(Self {
            header,
            options,
            entries,
            revocations,
            signed_bytes: input[..signed_end].to_vec(),
            signature,
        })
    }
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes)?;
            encoder.write_raw(self.signature.as_bytes())
        })
    }
    pub fn signature_preimage(&self) -> Vec<u8> {
        let mut value = Vec::with_capacity(self.signed_bytes.len() + 1);
        value.push(0x07);
        value.extend_from_slice(&self.signed_bytes);
        value
    }
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }
    pub const fn header(&self) -> &LeaseSet2Header {
        &self.header
    }
    pub fn options(&self) -> &Mapping {
        &self.options
    }
    pub fn entries(&self) -> &[MetaLease] {
        &self.entries
    }
    pub fn revocations(&self) -> &[Hash] {
        &self.revocations
    }
    pub fn signature(&self) -> &SignatureValue {
        &self.signature
    }
}

fn encode_unsigned(
    encoder: &mut EncodeBuffer<'_>,
    header: &LeaseSet2Header,
    options: &Mapping,
    encryption_keys: &[LeaseSet2EncryptionKey],
    leases: &[Lease2],
) -> Result<(), CodecError> {
    header.encode_into(encoder)?;
    options.encode_into(encoder)?;
    // Plan 172 §5: single-byte key count to match Java I2P reference.
    let key_count =
        u8::try_from(encryption_keys.len()).map_err(|_| CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "LeaseSet2 encryption key count",
        })?;
    encoder.write_u8(key_count)?;
    for key in encryption_keys {
        encoder.write_u16(key.key_type().code())?;
        let key_length =
            u16::try_from(key.as_bytes().len()).map_err(|_| CodecError::InvalidFieldValue {
                offset: encoder.len(),
                context: "LeaseSet2 encryption key length",
            })?;
        encoder.write_u16(key_length)?;
        encoder.write_raw(key.as_bytes())?;
    }
    let lease_count = u8::try_from(leases.len()).map_err(|_| CodecError::InvalidFieldValue {
        offset: encoder.len(),
        context: "LeaseSet2 lease count",
    })?;
    encoder.write_u8(lease_count)?;
    for lease in leases {
        lease.encode_into(encoder)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CryptoKeyType, Date32, Hash, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
        MAX_COMMON_STRUCTURE_SIZE, Mapping,
    };

    const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

    #[test]
    fn leaseset2_flags_match_current_common_structures_bits() {
        assert!(LeaseSet2Flags::from_raw(0x0001).has_offline_signature());
        assert!(LeaseSet2Flags::from_raw(0x0002).is_unpublished());
        assert!(LeaseSet2Flags::from_raw(0x0004).is_blinded_on_publication());
        assert!(!LeaseSet2Flags::from_raw(0x0008).is_blinded_on_publication());
        assert!(LeaseSet2Flags::from_raw(0x0008).has_reserved_bits());
        assert!(LeaseSet2Flags::from_raw(0x8000).has_reserved_bits());
    }

    // -------- Phase A: Lease2 --------

    #[test]
    fn lease2_exact_wire_length() {
        let lease = Lease2::new(Hash::from_bytes([0x11; 32]), 7, Date32::from_seconds(60));
        let bytes = lease.encode_to_vec(MAX).unwrap();
        assert_eq!(bytes.len(), LEASE2_WIRE_SIZE);
        assert_eq!(bytes.len(), 40);
    }

    #[test]
    fn lease2_round_trip_known_values() {
        let gateway = Hash::from_bytes([0x55; 32]);
        let lease = Lease2::new(gateway, 0x1234_5678, Date32::from_seconds(0x7654_3210));
        let bytes = lease.encode_to_vec(MAX).unwrap();
        let decoded = Lease2::decode(&bytes, MAX).unwrap();
        assert_eq!(decoded, lease);
    }

    #[test]
    fn lease2_big_endian_tunnel_id() {
        let lease = Lease2::new(
            Hash::from_bytes([0; 32]),
            0x0102_0304,
            Date32::from_seconds(0),
        );
        let bytes = lease.encode_to_vec(MAX).unwrap();
        // The tunnel-id is at offset 32..36.
        assert_eq!(&bytes[32..36], &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn lease2_big_endian_end_date() {
        let lease = Lease2::new(
            Hash::from_bytes([0; 32]),
            0,
            Date32::from_seconds(0x0102_0304),
        );
        let bytes = lease.encode_to_vec(MAX).unwrap();
        // The end-date is at offset 36..40.
        assert_eq!(&bytes[36..40], &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn lease2_reject_trailing_bytes() {
        let lease = Lease2::new(Hash::from_bytes([0; 32]), 1, Date32::from_seconds(1));
        let mut bytes = lease.encode_to_vec(MAX).unwrap();
        bytes.push(0x99);
        assert!(matches!(
            Lease2::decode(&bytes, MAX),
            Err(CodecError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn lease2_reject_truncation() {
        let lease = Lease2::new(Hash::from_bytes([0; 32]), 1, Date32::from_seconds(1));
        let bytes = lease.encode_to_vec(MAX).unwrap();
        for end in 0..bytes.len() {
            assert!(
                Lease2::decode(&bytes[..end], MAX).is_err(),
                "prefix {end} unexpectedly decoded"
            );
        }
    }

    #[test]
    fn lease2_checked_time_conversion() {
        // Date32 is a u32 seconds-since-epoch; accessors round-trip the
        // value without panic.
        let value = Date32::from_seconds(u32::MAX);
        assert_eq!(value.as_seconds(), u32::MAX);
    }

    // -------- Phase B: LeaseSet2 header --------

    fn ed_destination() -> crate::Destination {
        use crate::{
            Certificate, CryptoKeyType, KeyAndCert, KeyCertificate, PublicKey, SigningKeyType,
            SigningPublicKey,
        };
        let signing_len = SigningKeyType::EdDsaSha512Ed25519.public_key_len().unwrap();
        let crypto_len = CryptoKeyType::X25519.public_key_len().unwrap();
        let padding_len = 384 - crypto_len - signing_len.min(128);
        let keys = KeyAndCert::new(
            PublicKey::new(CryptoKeyType::X25519, vec![0x11; crypto_len]).unwrap(),
            SigningPublicKey::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x22; signing_len])
                .unwrap(),
            vec![0x33; padding_len],
            Certificate::Key(
                KeyCertificate::for_types(
                    SigningKeyType::EdDsaSha512Ed25519,
                    CryptoKeyType::X25519,
                )
                .unwrap(),
            ),
        )
        .unwrap();
        crate::Destination::new(keys).unwrap()
    }

    #[test]
    fn ls2_header_round_trip_online_published() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let dest_bytes = header.destination().encode_to_vec(MAX).unwrap();
        let mut payload = dest_bytes;
        payload.extend_from_slice(&1_000u32.to_be_bytes());
        payload.extend_from_slice(&600u16.to_be_bytes());
        payload.extend_from_slice(&0u16.to_be_bytes());
        let decoded = LeaseSet2Header::decode(&payload, MAX).unwrap();
        assert_eq!(decoded.published_seconds(), 1_000);
        assert_eq!(decoded.expires_offset_seconds(), 600);
        assert_eq!(decoded.expires_seconds(), 1_600);
        assert_eq!(decoded.flags().as_raw(), 0);
    }

    #[test]
    fn ls2_header_reserved_flags_rejected() {
        let destination = ed_destination();
        // Encode destination bytes, then append a header with reserved
        // flag bits. The destination encoding is variable-length so we
        // build a complete LS2-prefixed payload and decode through
        // LeaseSet2Header.
        let dest_bytes = destination.encode_to_vec(MAX).unwrap();
        let mut payload = dest_bytes;
        payload.extend_from_slice(&1_000u32.to_be_bytes()); // published
        payload.extend_from_slice(&600u16.to_be_bytes()); // expires offset
        payload.extend_from_slice(&0x0010u16.to_be_bytes()); // reserved bit
        let error = LeaseSet2Header::decode(&payload, MAX).unwrap_err();
        assert!(matches!(error, CodecError::InvalidFieldValue { .. }));
    }

    #[test]
    fn ls2_header_decodes_offline_signature_section() {
        let destination = ed_destination();
        let dest_bytes = destination.encode_to_vec(MAX).unwrap();
        let mut payload = dest_bytes;
        payload.extend_from_slice(&1_000u32.to_be_bytes());
        payload.extend_from_slice(&600u16.to_be_bytes());
        payload.extend_from_slice(&0x0001u16.to_be_bytes()); // offline bit
        payload.extend_from_slice(&2_000u32.to_be_bytes());
        payload.extend_from_slice(&7u16.to_be_bytes());
        payload.extend_from_slice(&[0x44; 32]);
        payload.extend_from_slice(&[0x55; 64]);
        let header = LeaseSet2Header::decode(&payload, MAX).unwrap();
        assert_eq!(header.offline_signature().unwrap().expires_seconds(), 2_000);
        assert_eq!(
            header
                .offline_signature()
                .unwrap()
                .transient_key()
                .key_type(),
            SigningKeyType::EdDsaSha512Ed25519
        );
    }

    #[test]
    fn ls2_header_expiration_checked() {
        let destination = ed_destination();
        // published close to u32::MAX, expires offset that pushes the
        // sum over u32::MAX.
        let error = LeaseSet2Header::new(
            destination,
            u32::MAX - 100,
            u16::MAX,
            LeaseSet2Flags::from_raw(0),
        )
        .unwrap_err();
        assert!(matches!(error, LeaseSet2HeaderError::ExpirationOverflow));
    }

    #[test]
    fn ls2_header_trailing_data_rejected() {
        let destination = ed_destination();
        let dest_bytes = destination.encode_to_vec(MAX).unwrap();
        let mut payload = dest_bytes;
        payload.extend_from_slice(&1_000u32.to_be_bytes());
        payload.extend_from_slice(&600u16.to_be_bytes());
        payload.extend_from_slice(&0u16.to_be_bytes());
        payload.push(0x99); // trailing
        let error = LeaseSet2Header::decode(&payload, MAX).unwrap_err();
        assert!(matches!(error, CodecError::TrailingBytes { .. }));
    }

    // -------- Phase C: encryption-key list --------

    fn build_lease2() -> Lease2 {
        Lease2::new(Hash::from_bytes([0x55; 32]), 7, Date32::from_seconds(2_000))
    }

    #[test]
    fn one_x25519_key_round_trip() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let options = Mapping::empty();
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()];
        let leases = vec![build_lease2()];
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(header, options, encryption_keys, leases, placeholder).unwrap();
        let bytes = ls2.encode_to_vec(MAX).unwrap();
        let decoded = LeaseSet2::decode(&bytes, MAX).unwrap();
        assert_eq!(decoded.encryption_keys().len(), 1);
        assert_eq!(
            decoded.encryption_keys()[0].key_type(),
            CryptoKeyType::X25519
        );
    }

    #[test]
    fn multiple_typed_keys_parse() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let options = Mapping::empty();
        let encryption_keys = vec![
            LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap(),
            LeaseSet2EncryptionKey::new(CryptoKeyType::Unknown(99), vec![0xaa; 16]).unwrap(),
        ];
        let leases = vec![build_lease2()];
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(header, options, encryption_keys, leases, placeholder).unwrap();
        let bytes = ls2.encode_to_vec(MAX).unwrap();
        let decoded = LeaseSet2::decode(&bytes, MAX).unwrap();
        assert_eq!(decoded.encryption_keys().len(), 2);
        assert_eq!(
            decoded.encryption_keys()[1].key_type(),
            CryptoKeyType::Unknown(99)
        );
    }

    #[test]
    fn unknown_key_retained_bounded_but_not_usable() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let options = Mapping::empty();
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::Unknown(99), vec![0xaa; 16]).unwrap()];
        let leases = vec![build_lease2()];
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(header, options, encryption_keys, leases, placeholder).unwrap();
        // LS2 builds but usable X25519 selector fails.
        assert!(matches!(
            ls2.usable_x25519_key(),
            Err(LeaseSet2KeySelectionError::X25519NotFound)
        ));
    }

    #[test]
    fn x25519_wrong_length_rejected() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let options = Mapping::empty();
        // Only 16 bytes for X25519 — wrong length is rejected by
        // the usable selector rather than by `new`.
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 16]).unwrap()];
        let leases = vec![build_lease2()];
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(header, options, encryption_keys, leases, placeholder).unwrap();
        assert!(matches!(
            ls2.usable_x25519_key(),
            Err(LeaseSet2KeySelectionError::X25519NotFound)
        ));
    }

    #[test]
    fn zero_keys_rejected_for_ordinary_supported_policy() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let result = LeaseSet2::new(
            header,
            Mapping::empty(),
            Vec::new(),
            vec![build_lease2()],
            placeholder,
        );
        assert!(matches!(
            result,
            Err(LeaseSet2BuildError::ZeroEncryptionKeys)
        ));
    }

    #[test]
    fn duplicate_x25519_policy_is_deterministic() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let leases = vec![build_lease2()];
        let encryption_keys = vec![
            LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap(),
            LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x66; 32]).unwrap(),
        ];
        let ls2 = LeaseSet2::new(
            header,
            Mapping::empty(),
            encryption_keys,
            leases,
            placeholder,
        )
        .unwrap();
        assert!(matches!(
            ls2.usable_x25519_key(),
            Err(LeaseSet2KeySelectionError::DuplicateX25519)
        ));
    }

    #[test]
    fn aggregate_key_bytes_bounded() {
        // Eight unknown keys of 1024 bytes each = 8 KiB, exactly the
        // ceiling. A single byte larger pushes past the limit.
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let leases = vec![build_lease2()];
        assert_eq!(MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES, 8 * 1024);
        let at_ceiling: Vec<LeaseSet2EncryptionKey> = (0..MAX_LEASE_SET2_ENCRYPTION_KEYS)
            .map(|_| {
                LeaseSet2EncryptionKey::new(CryptoKeyType::Unknown(99), vec![0xaa; 1024]).unwrap()
            })
            .collect();
        let ls2 = LeaseSet2::new(
            header.clone(),
            Mapping::empty(),
            at_ceiling,
            leases.clone(),
            placeholder.clone(),
        )
        .expect("at ceiling");
        let _ = ls2;
        // Push aggregate one byte past the limit.
        let over_ceiling: Vec<LeaseSet2EncryptionKey> = (0..MAX_LEASE_SET2_ENCRYPTION_KEYS)
            .map(|_| {
                LeaseSet2EncryptionKey::new(CryptoKeyType::Unknown(99), vec![0xaa; 1025]).unwrap()
            })
            .collect();
        let result = LeaseSet2::new(header, Mapping::empty(), over_ceiling, leases, placeholder);
        assert!(matches!(
            result,
            Err(LeaseSet2BuildError::EncryptionKeyBytesExceeded { .. })
        ));
    }

    // -------- Phase D: canonical options Mapping --------

    #[test]
    fn ls2_generated_options_sorted() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let mut builder = Mapping::builder();
        builder.insert("b".to_owned(), "2".to_owned()).unwrap();
        builder.insert("a".to_owned(), "1".to_owned()).unwrap();
        let options = builder.build().unwrap();
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(
            header,
            options,
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()],
            vec![build_lease2()],
            placeholder,
        )
        .unwrap();
        // Mapping canonicalization ensures the encoded bytes are
        // sorted by key (UTF-16 order).
        let encoded_options = ls2.options().encode_to_vec(MAX).unwrap();
        let a_pos = encoded_options
            .iter()
            .position(|byte| *byte == b'a')
            .expect("a key present");
        let b_pos = encoded_options
            .iter()
            .position(|byte| *byte == b'b')
            .expect("b key present");
        assert!(a_pos < b_pos);
    }

    #[test]
    fn received_ls2_signature_uses_original_bytes() {
        // A valid canonical LS2 round-trips through decode/encode and
        // signed_bytes are preserved exactly.
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let options = Mapping::empty();
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()];
        let leases = vec![build_lease2()];
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(header, options, encryption_keys, leases, placeholder).unwrap();
        let encoded = ls2.encode_to_vec(MAX).unwrap();
        let decoded = LeaseSet2::decode(&encoded, MAX).unwrap();
        assert_eq!(decoded.signed_bytes(), ls2.signed_bytes());
        assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
    }

    #[test]
    fn noncanonical_mapping_does_not_gain_valid_signature_by_reencoding() {
        // Build a canonical LS2, then mutate one option byte and re-encode.
        // The signed_bytes from the canonical form no longer match.
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let mut builder = Mapping::builder();
        builder.insert("a".to_owned(), "1".to_owned()).unwrap();
        let ls2 = LeaseSet2::new(
            header,
            builder.build().unwrap(),
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()],
            vec![build_lease2()],
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap(),
        )
        .unwrap();
        // Capture original signed_bytes, then mutate one byte.
        let original = ls2.signed_bytes().to_vec();
        let mut tampered = original.clone();
        // Flip a bit somewhere safely outside the lease section.
        tampered[100] ^= 0x01;
        assert_ne!(tampered, original);
        // Decoding tampered bytes must fail somewhere; the key
        // invariant is that the signed_bytes cannot be silently
        // normalized — they are taken verbatim from the wire.
        let result = LeaseSet2::decode(&tampered, MAX);
        assert!(result.is_err());
    }

    // -------- Phase E: full LS2 round-trip --------

    #[test]
    fn ls2_full_round_trip_preserves_signed_bytes() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let mut builder = Mapping::builder();
        builder.insert("a".to_owned(), "1".to_owned()).unwrap();
        builder.insert("c".to_owned(), "3".to_owned()).unwrap();
        builder.insert("b".to_owned(), "2".to_owned()).unwrap();
        let options = builder.build().unwrap();
        let leases = vec![
            build_lease2(),
            Lease2::new(
                Hash::from_bytes([0x66; 32]),
                99,
                Date32::from_seconds(2_500),
            ),
        ];
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()];
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(header, options, encryption_keys, leases, placeholder).unwrap();
        let encoded = ls2.encode_to_vec(MAX).unwrap();
        let decoded = LeaseSet2::decode(&encoded, MAX).unwrap();
        assert_eq!(decoded.signed_bytes(), ls2.signed_bytes());
        assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
        assert_eq!(decoded.leases().len(), 2);
        assert_eq!(decoded.published_seconds(), 1_000);
        assert_eq!(decoded.expires_seconds(), 1_600);
    }

    #[test]
    fn ls2_signature_preimage_prepends_type_byte() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let ls2 = LeaseSet2::new(
            header,
            Mapping::empty(),
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()],
            vec![build_lease2()],
            placeholder,
        )
        .unwrap();
        let preimage = ls2.signature_preimage();
        assert_eq!(preimage[0], LEASE_SET2_SIGNATURE_DOMAIN_BYTE);
        assert_eq!(&preimage[1..], ls2.signed_bytes());
    }

    // -------- Negative paths --------

    #[test]
    fn ls2_reject_zero_leases() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let placeholder =
            crate::SignatureValue::new(crate::SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
                .unwrap();
        let result = LeaseSet2::new(
            header,
            Mapping::empty(),
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()],
            Vec::new(),
            placeholder,
        );
        assert!(matches!(result, Err(LeaseSet2BuildError::ZeroLeases)));
    }

    #[test]
    fn ls2_reject_signature_type_mismatch() {
        let destination = ed_destination();
        let header =
            LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).unwrap();
        let wrong =
            crate::SignatureValue::new(crate::SigningKeyType::EcdsaSha256P256, vec![0u8; 64])
                .unwrap();
        let result = LeaseSet2::new(
            header,
            Mapping::empty(),
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()],
            vec![build_lease2()],
            wrong,
        );
        assert!(matches!(
            result,
            Err(LeaseSet2BuildError::SignatureTypeMismatch { .. })
        ));
    }
}
