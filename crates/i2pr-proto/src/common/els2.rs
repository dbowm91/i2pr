//! DatabaseStore type 5: the outer layer of an encrypted LeaseSet2.
//!
//! An encrypted LeaseSet2 is **not** a LeaseSet2 with extra fields. It is a
//! distinct record whose outer layer is deliberately different in kind from
//! the ordinary LeaseSet2 header: it carries no Destination, no properties
//! mapping, and no encryption keys, because none of those may be visible to a
//! floodfill. What it does carry is a daily **blinded** signing public key, the
//! two timestamps, the encrypted payload, and a Red25519 signature.
//!
//! ```text
//! Layer 0 (outer, plaintext, in this order)
//!   store type          1      taken from the DatabaseStore message, not the record,
//!                              but covered by the signature
//!   blinded sigtype     2      big endian, always 11
//!   blinded public key  n      length implied by the sigtype
//!   published           4      big endian seconds since the epoch
//!   expires             2      big endian offset from published, 18.2 h maximum
//!   flags               2      bit 0 = offline keys present, other bits zero
//!   transient expires   4      present only with offline keys
//!   transient sigtype   2      present only with offline keys
//!   transient key       n      present only with offline keys
//!   offline signature   m      over the three fields above, verified with the
//!                              blinded public key
//!   lenOuterCiphertext  2      big endian
//!   outerCiphertext     that   outerSalt(32) || ChaCha20(layer 1)
//!   signature           m      over everything above starting at the store-type
//!                              byte; transient key when offline keys are present
//! ```
//!
//! This module owns framing only. It does not derive the credential or
//! subcredential, does not derive layer keys, does not encrypt or decrypt, does
//! not verify a signature, and does not apply freshness policy. Those belong to
//! the NetDB layer, which is also where the record is validated and stored.
//!
//! The one piece of semantics kept here is [`EncryptedLeaseSet2::signed_bytes`]:
//! the exact byte region a verifier must cover. It exists because the store-type
//! byte is *not* part of the stored record but *is* part of the signature, and
//! a verifier that omits it would accept a record that no other implementation
//! produced.

use super::*;

/// The DatabaseStore type code for an encrypted LeaseSet2.
pub const ENCRYPTED_LEASE_SET2_STORE_TYPE: u8 = 5;
/// The DatabaseStore type code of the standard LeaseSet2 that may appear as
/// layer 2 of an encrypted LeaseSet2.
pub const INNER_LEASE_SET2_STORE_TYPE: u8 = 3;
/// The DatabaseStore type code of the MetaLeaseSet that may appear as layer 2
/// of an encrypted LeaseSet2.
pub const INNER_META_LEASE_SET2_STORE_TYPE: u8 = 7;
/// The only blinded signing key type an encrypted LeaseSet2 may use.
pub const ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE: SigningKeyType =
    SigningKeyType::RedDsaSha512Ed25519;
/// The only unblinded signing key types an encrypted service may use.
pub const ENCRYPTED_LEASE_SET2_UNBLINDED_SIGTYPES: [SigningKeyType; 2] = [
    SigningKeyType::EdDsaSha512Ed25519,
    SigningKeyType::RedDsaSha512Ed25519,
];

/// Flag bit 0 of the layer-0 flags field: an offline (transient) key block is
/// present, and the record signature is made with the transient key.
pub const ENCRYPTED_LEASE_SET2_FLAG_OFFLINE_KEYS: u16 = 0x0001;
/// Flag bits of the layer-0 flags field that must be zero.
///
/// The specification pins every other bit to zero "for compatibility with
/// future uses", so a set bit is a malformed record rather than a feature to
/// ignore.
pub const ENCRYPTED_LEASE_SET2_FLAGS_RESERVED_MASK: u16 = 0xfffe;
/// The largest value the two-byte `expires` offset can express, and therefore
/// the largest expiration window a single record can claim (18.2 hours).
pub const ENCRYPTED_LEASE_SET2_MAX_EXPIRES_OFFSET: u32 = u16::MAX as u32;
/// Size of the outer and inner layer salts.
pub const ENCRYPTED_LEASE_SET2_SALT_LENGTH: usize = 32;
/// Smallest outer-ciphertext length: a salt plus one byte of layer-1 plaintext.
pub const ENCRYPTED_LEASE_SET2_MIN_OUTER_CIPHERTEXT_LENGTH: usize =
    ENCRYPTED_LEASE_SET2_SALT_LENGTH + 1;

/// Layer-0 flags of an encrypted LeaseSet2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncryptedLeaseSet2Flags(u16);

impl EncryptedLeaseSet2Flags {
    /// Wraps a raw two-byte flags value without interpreting it.
    pub const fn from_raw(value: u16) -> Self {
        Self(value)
    }

    /// Returns the raw two-byte value.
    pub const fn as_raw(self) -> u16 {
        self.0
    }

    /// Returns whether the offline (transient) key block is present.
    pub const fn has_offline_keys(self) -> bool {
        self.0 & ENCRYPTED_LEASE_SET2_FLAG_OFFLINE_KEYS != 0
    }

    /// Returns whether any reserved bit is set.
    pub const fn has_reserved_bits(self) -> bool {
        self.0 & ENCRYPTED_LEASE_SET2_FLAGS_RESERVED_MASK != 0
    }
}

/// The optional offline-key block of an encrypted LeaseSet2 layer 0.
///
/// The block is in the clear, so anyone scraping floodfills can correlate a
/// service across days if the owner reuses the transient key. The
/// specification's mitigation is a fresh transient key per day, which is an
/// operational property; i2pr parses and verifies the block correctly but does
/// not claim key-batching as a capability, because no file format or I2CP
/// extension exists to deliver pre-generated daily keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedLeaseSet2OfflineKeys {
    expires_seconds: u32,
    sigtype: SigningKeyType,
    public_key: Vec<u8>,
    signed_bytes: Vec<u8>,
    signature: Vec<u8>,
}

impl EncryptedLeaseSet2OfflineKeys {
    /// Constructs an offline-key block from already-validated fields.
    pub fn new(
        expires_seconds: u32,
        sigtype: SigningKeyType,
        public_key: Vec<u8>,
        signature: Vec<u8>,
    ) -> Result<Self, CodecError> {
        let expected_key = sigtype.public_key_len().ok_or(unsupported(
            0,
            "offline transient sigtype",
            u64::from(sigtype.code()),
        ))?;
        if public_key.len() != expected_key {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: public_key.len(),
                maximum: expected_key,
                context: "encrypted LeaseSet2 transient public key",
            });
        }
        let expected_signature = sigtype.signature_len().ok_or(unsupported(
            0,
            "offline transient sigtype",
            u64::from(sigtype.code()),
        ))?;
        if signature.len() != expected_signature {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: signature.len(),
                maximum: expected_signature,
                context: "encrypted LeaseSet2 offline signature",
            });
        }
        let mut signed_bytes = Vec::with_capacity(4 + 2 + public_key.len());
        signed_bytes.extend_from_slice(&expires_seconds.to_be_bytes());
        signed_bytes.extend_from_slice(&sigtype.code().to_be_bytes());
        signed_bytes.extend_from_slice(&public_key);
        Ok(Self {
            expires_seconds,
            sigtype,
            public_key,
            signed_bytes,
            signature,
        })
    }

    /// Returns the absolute expiry of the delegated transient key.
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }

    /// Returns the transient signing key type.
    pub const fn sigtype(&self) -> SigningKeyType {
        self.sigtype
    }

    /// Borrows the transient signing public key.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Borrows the exact bytes the offline signature covers.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }

    /// Borrows the offline signature.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

/// The outer layer of a type-5 encrypted LeaseSet2 record.
///
/// Every field is public protocol material, so the type is `Clone` and
/// `Debug`. The encrypted payload is *not* a plaintext LeaseSet2 and is never
/// interpreted by this module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedLeaseSet2 {
    blinded_sigtype: SigningKeyType,
    blinded_public_key: Vec<u8>,
    published_seconds: u32,
    expires_offset_seconds: u16,
    flags: EncryptedLeaseSet2Flags,
    offline_keys: Option<EncryptedLeaseSet2OfflineKeys>,
    outer_ciphertext: Vec<u8>,
    signature: Vec<u8>,
    signed_bytes: Vec<u8>,
}

impl EncryptedLeaseSet2 {
    /// Constructs a record from already-validated fields.
    ///
    /// The `signature` is attached as the final field; the signed region is
    /// rebuilt exactly, including the leading store-type byte that is not
    /// stored in the record, so a producer that signs
    /// [`Self::signature_preimage`] produces the expected bytes.
    pub fn new(
        blinded_sigtype: SigningKeyType,
        blinded_public_key: Vec<u8>,
        published_seconds: u32,
        expires_offset_seconds: u16,
        offline_keys: Option<EncryptedLeaseSet2OfflineKeys>,
        outer_ciphertext: Vec<u8>,
        signature: Vec<u8>,
    ) -> Result<Self, CodecError> {
        let expected_key = blinded_sigtype.public_key_len().ok_or(unsupported(
            0,
            "blinded sigtype",
            u64::from(blinded_sigtype.code()),
        ))?;
        if blinded_public_key.len() != expected_key {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: blinded_public_key.len(),
                maximum: expected_key,
                context: "encrypted LeaseSet2 blinded public key",
            });
        }
        let expected_signature = blinded_sigtype.signature_len().ok_or(unsupported(
            0,
            "blinded sigtype",
            u64::from(blinded_sigtype.code()),
        ))?;
        if signature.len() != expected_signature {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: signature.len(),
                maximum: expected_signature,
                context: "encrypted LeaseSet2 signature",
            });
        }
        if outer_ciphertext.len() < ENCRYPTED_LEASE_SET2_MIN_OUTER_CIPHERTEXT_LENGTH {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: outer_ciphertext.len(),
                maximum: ENCRYPTED_LEASE_SET2_MIN_OUTER_CIPHERTEXT_LENGTH,
                context: "encrypted LeaseSet2 outer ciphertext",
            });
        }
        let flags = if offline_keys.is_some() {
            EncryptedLeaseSet2Flags::from_raw(ENCRYPTED_LEASE_SET2_FLAG_OFFLINE_KEYS)
        } else {
            EncryptedLeaseSet2Flags::from_raw(0)
        };
        let signed_bytes = Self::build_signed_bytes(
            &blinded_sigtype,
            &blinded_public_key,
            published_seconds,
            expires_offset_seconds,
            offline_keys.as_ref(),
            &outer_ciphertext,
        )?;
        Ok(Self {
            blinded_sigtype,
            blinded_public_key,
            published_seconds,
            expires_offset_seconds,
            flags,
            offline_keys,
            outer_ciphertext,
            signature,
            signed_bytes,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_signed_bytes(
        blinded_sigtype: &SigningKeyType,
        blinded_public_key: &[u8],
        published_seconds: u32,
        expires_offset_seconds: u16,
        offline_keys: Option<&EncryptedLeaseSet2OfflineKeys>,
        outer_ciphertext: &[u8],
    ) -> Result<Vec<u8>, CodecError> {
        let ciphertext_len =
            u16::try_from(outer_ciphertext.len()).map_err(|_| CodecError::LengthExceeded {
                offset: 0,
                declared: outer_ciphertext.len(),
                maximum: usize::from(u16::MAX),
                context: "encrypted LeaseSet2 outer ciphertext",
            })?;
        let mut signed = Vec::with_capacity(128 + outer_ciphertext.len());
        signed.push(ENCRYPTED_LEASE_SET2_STORE_TYPE);
        signed.extend_from_slice(&blinded_sigtype.code().to_be_bytes());
        signed.extend_from_slice(blinded_public_key);
        signed.extend_from_slice(&published_seconds.to_be_bytes());
        signed.extend_from_slice(&expires_offset_seconds.to_be_bytes());
        let flags = if offline_keys.is_some() {
            ENCRYPTED_LEASE_SET2_FLAG_OFFLINE_KEYS
        } else {
            0
        };
        signed.extend_from_slice(&flags.to_be_bytes());
        if let Some(offline) = offline_keys {
            signed.extend_from_slice(offline.signed_bytes());
            signed.extend_from_slice(offline.signature());
        }
        signed.extend_from_slice(&ciphertext_len.to_be_bytes());
        signed.extend_from_slice(outer_ciphertext);
        Ok(signed)
    }

    /// Decodes one complete record from a DatabaseStore type-5 body.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, |cursor| {
            let blinded_sigtype = SigningKeyType::from_code(cursor.read_u16()?);
            let key_len = blinded_sigtype.public_key_len().ok_or(unsupported(
                cursor.offset().saturating_sub(2),
                "encrypted LeaseSet2 blinded sigtype",
                u64::from(blinded_sigtype.code()),
            ))?;
            let blinded_public_key = cursor.take(key_len)?.to_vec();
            let published_seconds = cursor.read_u32()?;
            let expires_offset_seconds = cursor.read_u16()?;
            let flags = EncryptedLeaseSet2Flags::from_raw(cursor.read_u16()?);
            if flags.has_reserved_bits() {
                return Err(CodecError::NonCanonical {
                    offset: cursor.offset().saturating_sub(2),
                    context: "encrypted LeaseSet2 reserved flag bit",
                });
            }
            let offline_keys = if flags.has_offline_keys() {
                let expires_seconds = cursor.read_u32()?;
                let transient_sigtype = SigningKeyType::from_code(cursor.read_u16()?);
                let transient_key_len = transient_sigtype.public_key_len().ok_or(unsupported(
                    cursor.offset().saturating_sub(2),
                    "encrypted LeaseSet2 transient sigtype",
                    u64::from(transient_sigtype.code()),
                ))?;
                let transient_public_key = cursor.take(transient_key_len)?.to_vec();
                let signature_len = transient_sigtype.signature_len().ok_or(unsupported(
                    cursor.offset().saturating_sub(2),
                    "encrypted LeaseSet2 transient sigtype",
                    u64::from(transient_sigtype.code()),
                ))?;
                let signature = cursor.take(signature_len)?.to_vec();
                Some(EncryptedLeaseSet2OfflineKeys::new(
                    expires_seconds,
                    transient_sigtype,
                    transient_public_key,
                    signature,
                )?)
            } else {
                None
            };
            let outer_len = usize::from(cursor.read_u16()?);
            if outer_len < ENCRYPTED_LEASE_SET2_MIN_OUTER_CIPHERTEXT_LENGTH {
                return Err(CodecError::LengthExceeded {
                    offset: cursor.offset().saturating_sub(2),
                    declared: outer_len,
                    maximum: ENCRYPTED_LEASE_SET2_MIN_OUTER_CIPHERTEXT_LENGTH,
                    context: "encrypted LeaseSet2 outer ciphertext",
                });
            }
            let outer_ciphertext = cursor.take(outer_len)?.to_vec();
            let signature_len = blinded_sigtype.signature_len().ok_or(unsupported(
                0,
                "blinded sigtype",
                u64::from(blinded_sigtype.code()),
            ))?;
            let signature = cursor.take(signature_len)?.to_vec();
            Self::new(
                blinded_sigtype,
                blinded_public_key,
                published_seconds,
                expires_offset_seconds,
                offline_keys,
                outer_ciphertext,
                signature,
            )
        })
    }

    /// Encodes the stored record body (everything after the store-type byte).
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes[1..])?;
            encoder.write_raw(&self.signature)
        })
    }

    /// Returns the exact byte region the record signature covers.
    ///
    /// The region starts with the store-type byte even though that byte is not
    /// stored in the record: it is taken from the DatabaseStore message, and it
    /// is part of what was signed. A verifier that omits the byte verifies a
    /// different message than the producer signed.
    pub fn signature_preimage(&self) -> Vec<u8> {
        self.signed_bytes.clone()
    }

    /// Borrows the exact signed region, store-type byte included.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }
    /// Borrows the record signature.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// Returns the blinded signing key type.
    pub const fn blinded_sigtype(&self) -> SigningKeyType {
        self.blinded_sigtype
    }

    /// Borrows the blinded signing public key.
    pub fn blinded_public_key(&self) -> &[u8] {
        &self.blinded_public_key
    }

    /// Returns the published timestamp in seconds.
    pub const fn published_seconds(&self) -> u32 {
        self.published_seconds
    }

    /// Returns the expiration offset from `published` in seconds.
    pub const fn expires_offset_seconds(&self) -> u16 {
        self.expires_offset_seconds
    }

    /// Returns the absolute expiration timestamp in seconds.
    ///
    /// The addition is saturating: `published` is a four-byte field and the
    /// offset is two bytes, so a 2106 rollover cannot wrap into a value that
    /// looks like a live record.
    pub const fn expires_seconds(&self) -> u32 {
        self.published_seconds
            .saturating_add(self.expires_offset_seconds as u32)
    }

    /// Returns the layer-0 flags.
    pub const fn flags(&self) -> EncryptedLeaseSet2Flags {
        self.flags
    }

    /// Returns the offline-key block, when present.
    pub const fn offline_keys(&self) -> Option<&EncryptedLeaseSet2OfflineKeys> {
        self.offline_keys.as_ref()
    }

    /// Returns whether the record signature must be verified with the
    /// transient key rather than the blinded key.
    pub const fn signed_by_transient_key(&self) -> bool {
        self.flags.has_offline_keys()
    }

    /// Borrows the outer ciphertext, salt included.
    pub fn outer_ciphertext(&self) -> &[u8] {
        &self.outer_ciphertext
    }

    /// Borrows the outer salt, the first 32 bytes of the outer ciphertext.
    pub fn outer_salt(&self) -> &[u8; ENCRYPTED_LEASE_SET2_SALT_LENGTH] {
        self.outer_ciphertext[..ENCRYPTED_LEASE_SET2_SALT_LENGTH]
            .try_into()
            .expect("outer ciphertext is at least one salt long")
    }
}

/// The exact byte region a type-5 signature covers, obtainable only from a real
/// encrypted-LeaseSet2 structure.
///
/// # Why this type exists
///
/// The deployed type-11 transcript that Java I2P and i2pd use has no hash-domain
/// separator and no prefix-free message-length frame (ADR 0032). Its security therefore
/// rests on the *caller* signing exactly one thing and nothing else. A function that took
/// `&[u8]` would make that a convention; this type makes it structural: the only
/// constructors take a decoded [`EncryptedLeaseSet2`] or
/// [`EncryptedLeaseSet2OfflineKeys`], and there is no `from_bytes`, so no application
/// message and no caller-selected transcript input can reach the ELS2 signer or verifier
/// through this boundary.
///
/// The borrowed region is the same buffer [`EncryptedLeaseSet2::signed_bytes`] returns,
/// store-type byte included. A verifier that omitted that byte would be checking a
/// different message than the producer signed, which is why the byte is not optional here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Els2SignedRegion<'a>(&'a [u8]);

impl<'a> Els2SignedRegion<'a> {
    /// Wraps the signed region of a type-5 record.
    pub fn of_record(record: &'a EncryptedLeaseSet2) -> Self {
        Self(record.signed_bytes())
    }

    /// Wraps the signed region of a type-5 offline-key delegation block.
    pub fn of_offline_keys(offline: &'a EncryptedLeaseSet2OfflineKeys) -> Self {
        Self(offline.signed_bytes())
    }

    /// Borrows the region bytes.
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.0
    }

    /// Returns the region length, for a caller's own bound check.
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the region is empty, which no valid type-5 region ever is.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

    fn blinded_key() -> Vec<u8> {
        (0..32_u8).collect()
    }

    fn build(offline: bool) -> EncryptedLeaseSet2 {
        let offline_keys = offline.then(|| {
            EncryptedLeaseSet2OfflineKeys::new(
                1_700_000_000,
                SigningKeyType::EdDsaSha512Ed25519,
                vec![0x44; 32],
                vec![0x55; 64],
            )
            .expect("offline block")
        });
        EncryptedLeaseSet2::new(
            ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
            blinded_key(),
            1_700_000_000,
            3_600,
            offline_keys,
            vec![0x77; 200],
            vec![0x88; 64],
        )
        .expect("record")
    }

    #[test]
    fn record_round_trips_and_retains_the_signed_region() {
        for offline in [false, true] {
            let record = build(offline);
            let encoded = record.encode_to_vec(MAX).expect("encode");
            let decoded = EncryptedLeaseSet2::decode(&encoded, MAX).expect("decode");
            assert_eq!(decoded, record);
            assert_eq!(decoded.signed_bytes(), record.signed_bytes());
            assert_eq!(decoded.encode_to_vec(MAX).expect("re-encode"), encoded);
            assert_eq!(decoded.flags().has_offline_keys(), offline);
            assert_eq!(decoded.signed_by_transient_key(), offline);
            assert_eq!(decoded.expires_seconds(), 1_700_003_600);
        }
    }

    #[test]
    fn signature_preimage_starts_with_the_store_type_byte() {
        let record = build(false);
        let preimage = record.signature_preimage();
        assert_eq!(preimage[0], ENCRYPTED_LEASE_SET2_STORE_TYPE);
        // The stored body omits the type byte and carries the signature; the
        // preimage carries the type byte and omits the signature.
        let body = record.encode_to_vec(MAX).expect("encode");
        assert_eq!(preimage.len(), body.len() - record.signature().len() + 1);
        assert_eq!(
            &preimage[1..],
            &body[..body.len() - record.signature().len()]
        );
    }

    #[test]
    fn record_is_truncated_rejected_at_every_boundary() {
        let encoded = build(true).encode_to_vec(MAX).expect("encode");
        for end in 0..encoded.len() {
            assert!(
                EncryptedLeaseSet2::decode(&encoded[..end], MAX).is_err(),
                "prefix of {end} bytes was accepted"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut encoded = build(false).encode_to_vec(MAX).expect("encode");
        encoded.push(0);
        assert!(EncryptedLeaseSet2::decode(&encoded, MAX).is_err());
    }

    #[test]
    fn reserved_flag_bits_are_rejected() {
        let mut encoded = build(false).encode_to_vec(MAX).expect("encode");
        // flags sit at offset 2 + 32 + 4 + 2.
        let flags_offset = 2 + 32 + 4 + 2;
        encoded[flags_offset + 1] |= 0x02;
        assert!(matches!(
            EncryptedLeaseSet2::decode(&encoded, MAX),
            Err(CodecError::NonCanonical { .. })
        ));
    }

    #[test]
    fn unknown_blinded_sigtype_is_rejected_before_allocation() {
        let mut encoded = build(false).encode_to_vec(MAX).expect("encode");
        encoded[0] = 0x00;
        encoded[1] = 0x63; // 99, no known public-key length
        assert!(matches!(
            EncryptedLeaseSet2::decode(&encoded, MAX),
            Err(CodecError::Unsupported { .. })
        ));
    }

    #[test]
    fn declared_outer_ciphertext_shorter_than_a_salt_is_rejected() {
        let mut encoded = build(false).encode_to_vec(MAX).expect("encode");
        let len_offset = 2 + 32 + 4 + 2 + 2;
        encoded[len_offset..len_offset + 2].copy_from_slice(&4_u16.to_be_bytes());
        assert!(matches!(
            EncryptedLeaseSet2::decode(&encoded, MAX),
            Err(CodecError::LengthExceeded { .. })
        ));
    }

    #[test]
    fn constructor_rejects_inconsistent_lengths() {
        assert!(matches!(
            EncryptedLeaseSet2::new(
                ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
                vec![0; 31],
                1,
                1,
                None,
                vec![0; 64],
                vec![0; 64]
            ),
            Err(CodecError::LengthExceeded { .. })
        ));
        assert!(matches!(
            EncryptedLeaseSet2::new(
                ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
                vec![0; 32],
                1,
                1,
                None,
                vec![0; 64],
                vec![0; 63]
            ),
            Err(CodecError::LengthExceeded { .. })
        ));
        assert!(matches!(
            EncryptedLeaseSet2::new(
                ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
                vec![0; 32],
                1,
                1,
                None,
                vec![0; 8],
                vec![0; 64]
            ),
            Err(CodecError::LengthExceeded { .. })
        ));
    }

    #[test]
    fn offline_block_keeps_its_own_signed_region() {
        let offline = EncryptedLeaseSet2OfflineKeys::new(
            1_700_000_000,
            SigningKeyType::EdDsaSha512Ed25519,
            vec![0x44; 32],
            vec![0x55; 64],
        )
        .expect("offline block");
        assert_eq!(offline.signed_bytes().len(), 4 + 2 + 32);
        assert_eq!(
            &offline.signed_bytes()[..4],
            &1_700_000_000_u32.to_be_bytes()
        );
        assert_eq!(&offline.signed_bytes()[4..6], &7_u16.to_be_bytes());
        assert!(
            EncryptedLeaseSet2OfflineKeys::new(
                1,
                SigningKeyType::EdDsaSha512Ed25519,
                vec![0; 31],
                vec![0; 64]
            )
            .is_err()
        );
    }
}
