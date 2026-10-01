//! Versioned, bounded persistence envelope for validated floodfill records.
//!
//! The envelope is integrity-framed, not an authority token: every payload must be decoded and
//! cryptographically revalidated by the caller. Restored provenance is always narrowed to a
//! replica by consumers; reply secrets and source peer identifiers are never serialized.

use i2pr_proto::Hash;
use sha2::{Digest, Sha256};
use thiserror::Error;

use i2pr_netdb::{
    InboundProvenance, NetDbNamespace, RecordProvenance, ServerInsertOutcome, ServerNetDb,
    StorePurpose, ValidatedNetDbRecord, ValidatedRouterInfo, ValidationContext,
};
use i2pr_storage::cache_seam::{ByteCache, CacheError};

const MAGIC: &[u8; 4] = b"I2FF";
pub const FLOODFILL_ENVELOPE_VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 1 + 1 + 1 + 1 + 8 + 4 + 32;
const CHECKSUM_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistedPurpose {
    PublishedStore,
    FloodReplica,
    LocalPublication,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistedIngress {
    DirectPeer,
    RouterTunnel,
    Local,
}

#[derive(Clone, Eq, PartialEq)]
pub struct FloodfillRecordEnvelope {
    pub record_type: u8,
    pub key: Hash,
    pub observed_at_ms: u64,
    pub ingress: PersistedIngress,
    pub purpose: PersistedPurpose,
    pub canonical_bytes: Vec<u8>,
}

impl std::fmt::Debug for FloodfillRecordEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FloodfillRecordEnvelope")
            .field("record_type", &self.record_type)
            .field("key", &"[redacted]")
            .field("observed_at_ms", &self.observed_at_ms)
            .field("purpose", &self.purpose)
            .field("canonical_bytes", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("NetDB cache operation failed: {0}")]
    Cache(#[from] CacheError),
    #[error("invalid floodfill record envelope")]
    InvalidEnvelope,
    #[error("unsupported floodfill record type {0}")]
    UnsupportedType(u8),
    #[error("record envelope exceeds its configured limit")]
    TooLarge,
    #[error("persisted record failed protocol validation")]
    InvalidRecord,
    #[error("restored record was expired or exceeded restore age policy")]
    Expired,
    #[error("restored record exceeded in-memory NetDB capacity")]
    Capacity,
}

impl FloodfillRecordEnvelope {
    pub fn encode(&self, max_bytes: usize) -> Result<Vec<u8>, PersistError> {
        if !matches!(self.record_type, 0 | 1 | 3 | 7) {
            return Err(PersistError::UnsupportedType(self.record_type));
        }
        let payload_len =
            u32::try_from(self.canonical_bytes.len()).map_err(|_| PersistError::TooLarge)?;
        let mut out = Vec::with_capacity(HEADER_LEN + self.canonical_bytes.len() + CHECKSUM_LEN);
        out.extend_from_slice(MAGIC);
        out.push(FLOODFILL_ENVELOPE_VERSION);
        out.push(self.record_type);
        out.push(0); // MainRouter only.
        out.push(match self.ingress {
            PersistedIngress::DirectPeer => 0,
            PersistedIngress::RouterTunnel => 1,
            PersistedIngress::Local => 2,
        });
        out.push(match self.purpose {
            PersistedPurpose::PublishedStore => 0,
            PersistedPurpose::FloodReplica => 1,
            PersistedPurpose::LocalPublication => 2,
        });
        out.extend_from_slice(&self.observed_at_ms.to_be_bytes());
        out.extend_from_slice(&payload_len.to_be_bytes());
        out.extend_from_slice(self.key.as_bytes());
        out.extend_from_slice(&self.canonical_bytes);
        out.extend_from_slice(&Sha256::digest(&out));
        if out.len() > max_bytes {
            return Err(PersistError::TooLarge);
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8], max_bytes: usize, key: Hash) -> Result<Self, PersistError> {
        if bytes.len() < HEADER_LEN + CHECKSUM_LEN
            || bytes.len() > max_bytes
            || &bytes[..4] != MAGIC
            || bytes[4] != FLOODFILL_ENVELOPE_VERSION
            || bytes[6] != 0
        {
            return Err(PersistError::InvalidEnvelope);
        }
        if !matches!(bytes[5], 0 | 1 | 3 | 7) {
            return Err(PersistError::UnsupportedType(bytes[5]));
        }
        let observed_at_ms = u64::from_be_bytes(
            bytes[9..17]
                .try_into()
                .map_err(|_| PersistError::InvalidEnvelope)?,
        );
        let length = u32::from_be_bytes(
            bytes[17..21]
                .try_into()
                .map_err(|_| PersistError::InvalidEnvelope)?,
        ) as usize;
        let stored_key = Hash::from_bytes(
            bytes[21..53]
                .try_into()
                .map_err(|_| PersistError::InvalidEnvelope)?,
        );
        if stored_key != key {
            return Err(PersistError::InvalidEnvelope);
        }
        let end = HEADER_LEN
            .checked_add(length)
            .ok_or(PersistError::InvalidEnvelope)?;
        if end.checked_add(CHECKSUM_LEN) != Some(bytes.len())
            || Sha256::digest(&bytes[..end]).as_slice() != &bytes[end..]
        {
            return Err(PersistError::InvalidEnvelope);
        }
        let ingress = match bytes[7] {
            0 => PersistedIngress::DirectPeer,
            1 => PersistedIngress::RouterTunnel,
            2 => PersistedIngress::Local,
            _ => return Err(PersistError::InvalidEnvelope),
        };
        let purpose = match bytes[8] {
            0 => PersistedPurpose::PublishedStore,
            1 => PersistedPurpose::FloodReplica,
            2 => PersistedPurpose::LocalPublication,
            _ => return Err(PersistError::InvalidEnvelope),
        };
        Ok(Self {
            record_type: bytes[5],
            key,
            observed_at_ms,
            ingress,
            purpose,
            canonical_bytes: bytes[HEADER_LEN..end].to_vec(),
        })
    }
}

/// Byte-cache composition. The caller supplies the canonical record key as the cache filename
/// and must revalidate the decoded record/key binding before insertion.
pub struct FloodfillRecordStore {
    cache: ByteCache,
    max_record_bytes: usize,
}

impl FloodfillRecordStore {
    pub fn new(cache: ByteCache, max_record_bytes: usize) -> Self {
        Self {
            cache,
            max_record_bytes,
        }
    }
    pub fn save(&self, name: &str, record: &FloodfillRecordEnvelope) -> Result<(), PersistError> {
        let bytes = record.encode(self.max_record_bytes)?;
        self.cache.replace(name, &bytes)?;
        Ok(())
    }
    pub fn save_router_info(
        &self,
        name: &str,
        record: &ValidatedRouterInfo,
        observed_at_ms: u64,
        purpose: PersistedPurpose,
    ) -> Result<(), PersistError> {
        let canonical_bytes = record
            .router_info()
            .encode_to_vec(self.max_record_bytes)
            .map_err(|_| PersistError::TooLarge)?;
        self.save(
            name,
            &FloodfillRecordEnvelope {
                record_type: 0,
                key: *record.key().as_hash(),
                observed_at_ms,
                ingress: PersistedIngress::DirectPeer,
                purpose,
                canonical_bytes,
            },
        )
    }
    pub fn load(
        &self,
        name: &str,
        key: Hash,
    ) -> Result<Option<FloodfillRecordEnvelope>, PersistError> {
        self.cache
            .read(name)?
            .map(|bytes| FloodfillRecordEnvelope::decode(&bytes, self.max_record_bytes, key))
            .transpose()
    }

    /// Loads a persisted RouterInfo only after canonical decode, key binding, signature/freshness
    /// validation, and observed-age checks. Restored provenance is narrowed to FloodReplica.
    pub fn load_router_info_into(
        &self,
        name: &str,
        key: Hash,
        db: &mut ServerNetDb,
        context: ValidationContext,
        now_ms: u64,
        max_observed_age_ms: u64,
    ) -> Result<Option<ServerInsertOutcome>, PersistError> {
        self.load_validated_into(name, key, db, now_ms, max_observed_age_ms, |envelope| {
            if envelope.record_type != 0 {
                return Err(());
            }
            let router_info = i2pr_proto::RouterInfo::decode(
                &envelope.canonical_bytes,
                i2pr_proto::MAX_COMMON_STRUCTURE_SIZE,
            )
            .map_err(|_| ())?;
            ValidatedRouterInfo::from_router_info(
                router_info,
                Some(i2pr_netdb::RouterHash::from_hash(key)),
                context,
            )
            .map(ValidatedNetDbRecord::RouterInfo)
            .map_err(|_| ())
        })
    }

    /// Validates and inserts an arbitrary supported record type. The validation closure must
    /// perform the type-specific canonical decode, signature/freshness checks, and key binding.
    /// Persisted provenance is never restored as publisher authority.
    pub fn load_validated_into<F>(
        &self,
        name: &str,
        key: Hash,
        db: &mut ServerNetDb,
        now_ms: u64,
        max_observed_age_ms: u64,
        validate: F,
    ) -> Result<Option<ServerInsertOutcome>, PersistError>
    where
        F: FnOnce(&FloodfillRecordEnvelope) -> Result<ValidatedNetDbRecord, ()>,
    {
        let Some(envelope) = self.load(name, key)? else {
            return Ok(None);
        };
        if now_ms.saturating_sub(envelope.observed_at_ms) > max_observed_age_ms {
            return Err(PersistError::Expired);
        }
        let record = validate(&envelope).map_err(|_| PersistError::InvalidRecord)?;
        let id = record.id();
        if id.record_type() != envelope.record_type || *id.key() != key {
            return Err(PersistError::InvalidRecord);
        }
        let provenance = RecordProvenance {
            namespace: NetDbNamespace::MainRouter,
            inbound: InboundProvenance::RouterTunnel,
            purpose: StorePurpose::FloodReplica,
            observed_at_ms: now_ms,
        };
        match db.insert(record, provenance) {
            Ok(ServerInsertOutcome::CapacityExceeded) => Err(PersistError::Capacity),
            Ok(outcome) => Ok(Some(outcome)),
            Err(_) => Err(PersistError::InvalidRecord),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> FloodfillRecordEnvelope {
        FloodfillRecordEnvelope {
            record_type: 0,
            key: Hash::from_bytes([1; 32]),
            observed_at_ms: 99,
            ingress: PersistedIngress::DirectPeer,
            purpose: PersistedPurpose::PublishedStore,
            canonical_bytes: vec![2, 3, 4],
        }
    }

    #[test]
    fn envelope_round_trips_and_rejects_corruption_version_and_truncation() {
        let original = record();
        let encoded = original.encode(1024).expect("encode");
        assert!(!format!("{original:?}").contains("[2, 3, 4]"));
        assert_eq!(
            FloodfillRecordEnvelope::decode(&encoded, 1024, original.key).expect("decode"),
            original
        );
        assert!(
            FloodfillRecordEnvelope::decode(&encoded[..encoded.len() - 1], 1024, original.key)
                .is_err()
        );
        let mut corrupt = encoded.clone();
        corrupt[HEADER_LEN] ^= 1;
        assert!(FloodfillRecordEnvelope::decode(&corrupt, 1024, original.key).is_err());
        let mut unknown = encoded;
        unknown[4] = 99;
        assert!(matches!(
            FloodfillRecordEnvelope::decode(&unknown, 1024, original.key),
            Err(PersistError::InvalidEnvelope)
        ));
    }

    #[test]
    fn envelope_format_covers_only_the_adr_supported_type_floor() {
        for record_type in [0, 1, 3, 7] {
            let mut value = record();
            value.record_type = record_type;
            let encoded = value.encode(1024).expect("supported record type");
            assert_eq!(
                FloodfillRecordEnvelope::decode(&encoded, 1024, value.key).expect("decode"),
                value
            );
        }
        let mut unsupported = record();
        unsupported.record_type = 5;
        assert!(matches!(
            unsupported.encode(1024),
            Err(PersistError::UnsupportedType(5))
        ));
    }

    #[test]
    fn cache_replacement_preserves_complete_latest_envelope() {
        let directory = tempfile::tempdir().expect("directory");
        std::fs::create_dir_all(directory.path().join("netdb")).expect("netdb parent");
        let cache = ByteCache::in_data_dir(directory.path());
        let store = FloodfillRecordStore::new(cache, 1024);
        let first = record();
        store
            .save(
                "0101010101010101010101010101010101010101010101010101010101010101",
                &first,
            )
            .expect("first");
        let mut second = first.clone();
        second.observed_at_ms += 1;
        second.canonical_bytes.push(5);
        store
            .save(
                "0101010101010101010101010101010101010101010101010101010101010101",
                &second,
            )
            .expect("replace");
        let oversized = FloodfillRecordEnvelope {
            canonical_bytes: vec![9; 1024],
            ..second.clone()
        };
        assert!(matches!(
            store.save(
                "0101010101010101010101010101010101010101010101010101010101010101",
                &oversized
            ),
            Err(PersistError::TooLarge)
        ));
        assert_eq!(
            store
                .load(
                    "0101010101010101010101010101010101010101010101010101010101010101",
                    first.key
                )
                .expect("load"),
            Some(second)
        );
    }
}
