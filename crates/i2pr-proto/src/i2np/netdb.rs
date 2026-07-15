//! Structural NetDB message bodies and redacted reply secrets.

use super::*;

/// DatabaseStore type bits supported by the current specification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseStoreType {
    /// Compressed RouterInfo.
    RouterInfo,
    /// Classic LeaseSet.
    LeaseSet,
    /// LeaseSet2, whose semantics are deferred.
    LeaseSet2,
    /// EncryptedLeaseSet, whose semantics are deferred.
    EncryptedLeaseSet,
    /// MetaLeaseSet, whose semantics are deferred.
    MetaLeaseSet,
}

impl DatabaseStoreType {
    pub(super) fn from_code(code: u8) -> Result<Self, CodecError> {
        match code {
            0 => Ok(Self::RouterInfo),
            1 => Ok(Self::LeaseSet),
            3 => Ok(Self::LeaseSet2),
            5 => Ok(Self::EncryptedLeaseSet),
            7 => Ok(Self::MetaLeaseSet),
            other => Err(CodecError::Unsupported {
                offset: 0,
                context: "DatabaseStore type",
                value: u64::from(other),
            }),
        }
    }

    pub(super) const fn code(self) -> u8 {
        match self {
            Self::RouterInfo => 0,
            Self::LeaseSet => 1,
            Self::LeaseSet2 => 3,
            Self::EncryptedLeaseSet => 5,
            Self::MetaLeaseSet => 7,
        }
    }
}

/// DatabaseStore payload after fixed fields and body framing are validated.
#[derive(Clone, Eq, PartialEq)]
pub enum DatabaseStoreData {
    /// A gzip-compressed RouterInfo retained without decompression.
    RouterInfoCompressed(DeferredPayload),
    /// A structurally decoded classic LeaseSet.
    LeaseSet(Box<LeaseSet>),
    /// A recognized later LeaseSet-family type retained for a later decoder.
    Deferred {
        /// The recognized type identifier.
        store_type: DatabaseStoreType,
        /// The uncompressed payload bytes.
        payload: DeferredPayload,
    },
}

impl fmt::Debug for DatabaseStoreData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RouterInfoCompressed(payload) => formatter
                .debug_tuple("RouterInfoCompressed")
                .field(payload)
                .finish(),
            Self::LeaseSet(value) => formatter.debug_tuple("LeaseSet").field(value).finish(),
            Self::Deferred {
                store_type,
                payload,
            } => formatter
                .debug_struct("Deferred")
                .field("store_type", store_type)
                .field("payload", payload)
                .finish(),
        }
    }
}

/// A non-cloneable, zeroizing reply key or session tag.
///
/// This narrow wrapper exists only for the structurally parsed
/// `DatabaseLookup` reply fields. It provides memory hygiene and redacted
/// formatting; it does not implement reply encryption, key derivation, or
/// any general secret-management API.
#[derive(Eq, PartialEq)]
pub struct ReplySecret<const N: usize> {
    bytes: Zeroizing<[u8; N]>,
}

impl<const N: usize> ReplySecret<N> {
    /// Creates a reply-secret value from protocol bytes.
    pub fn from_bytes(bytes: [u8; N]) -> Self {
        Self {
            bytes: Zeroizing::new(bytes),
        }
    }

    pub(super) fn from_zeroizing(bytes: Zeroizing<[u8; N]>) -> Self {
        Self { bytes }
    }

    /// Borrows the secret for the shortest practical encoding interval.
    pub fn as_bytes(&self) -> &[u8; N] {
        &self.bytes
    }
}

impl<const N: usize> fmt::Debug for ReplySecret<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReplySecret")
            .field("length", &N)
            .finish()
    }
}

/// DatabaseStore message body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseStoreMessage {
    /// Hash key of the stored object.
    pub key: Hash,
    /// Optional delivery-status request token.
    pub reply_token: u32,
    /// Optional reply tunnel identifier, present with a nonzero token.
    pub reply_tunnel_id: Option<u32>,
    /// Optional reply gateway hash, present with a nonzero token.
    pub reply_gateway: Option<Hash>,
    /// Stored record body.
    pub data: DatabaseStoreData,
}

/// Reply encryption material in DatabaseLookup.
#[derive(Eq, PartialEq)]
pub enum ReplyEncryption {
    /// No encrypted reply requested.
    None,
    /// Legacy ElGamal/AES reply material.
    ElGamal {
        /// Session key bytes.
        reply_key: ReplySecret<32>,
        /// 32-byte session tags.
        reply_tags: Vec<ReplySecret<32>>,
    },
    /// ECIES reply material.
    Ecies {
        /// Session key bytes.
        reply_key: ReplySecret<32>,
        /// 8-byte session tags.
        reply_tags: Vec<ReplySecret<8>>,
    },
}

impl fmt::Debug for ReplyEncryption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("None"),
            Self::ElGamal { reply_tags, .. } => formatter
                .debug_struct("ElGamal")
                .field("reply_key", &"<redacted>")
                .field("tag_count", &reply_tags.len())
                .finish(),
            Self::Ecies { reply_tags, .. } => formatter
                .debug_struct("Ecies")
                .field("reply_key", &"<redacted>")
                .field("tag_count", &reply_tags.len())
                .finish(),
        }
    }
}

/// DatabaseLookup message body.
#[derive(Debug, Eq, PartialEq)]
pub struct DatabaseLookupMessage {
    /// Hash key to look up.
    pub key: Hash,
    /// Requester or reply-gateway hash.
    pub from: Hash,
    /// Whether the reply is sent through a tunnel.
    pub delivery_flag: bool,
    /// Reply tunnel identifier when `delivery_flag` is set.
    pub reply_tunnel_id: Option<u32>,
    /// Lookup type bits (0=any, 1=LeaseSet, 2=RouterInfo, 3=exploration).
    pub lookup_type: u8,
    /// Excluded peer hashes.
    pub excluded_peers: Vec<Hash>,
    /// Optional reply encryption fields.
    pub reply_encryption: ReplyEncryption,
}

/// DatabaseSearchReply message body.
#[derive(Debug, Eq, PartialEq)]
pub struct DatabaseSearchReplyMessage {
    /// Hash key that was searched.
    pub key: Hash,
    /// Bounded peer hashes near the key.
    pub peer_hashes: Vec<Hash>,
    /// Unauthenticated sender hash.
    pub from: Hash,
}
