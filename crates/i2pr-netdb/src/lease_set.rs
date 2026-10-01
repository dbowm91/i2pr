//! Cryptographic/freshness validation and bounded storage for classic LeaseSets.

use std::collections::BTreeMap;

use i2pr_crypto::{CryptoError, verify_meta_lease_set, verify_signature};
use i2pr_proto::{LeaseSet, MAX_COMMON_STRUCTURE_SIZE, MetaLeaseSet};
use thiserror::Error;

use crate::lease_set2::{DestinationHash, LeaseSetDisclosureBlock};

/// Caller-supplied validation time and maximum encoded length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSetValidationContext {
    pub now_ms: u64,
    pub max_encoded_len: usize,
}

impl LeaseSetValidationContext {
    pub const fn new(now_ms: u64) -> Self {
        Self {
            now_ms,
            max_encoded_len: MAX_COMMON_STRUCTURE_SIZE,
        }
    }
}

/// Failure while validating a classic LeaseSet for server-authority storage.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum LeaseSetValidationError {
    #[error("LeaseSet encoded length exceeds configured maximum")]
    EncodedTooLarge,
    #[error("LeaseSet destination hash does not match DatabaseStore key")]
    DestinationMismatch,
    #[error("LeaseSet has no leases")]
    NoLeases,
    #[error("LeaseSet leases are all expired")]
    Expired,
    #[error("LeaseSet signature is invalid")]
    InvalidSignature,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Codec(#[from] i2pr_proto::CodecError),
}

/// A classic LeaseSet that passed signature, key binding, and lease freshness checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedLeaseSet {
    key: DestinationHash,
    value: LeaseSet,
    version_ms: u64,
    encoded_len: usize,
}

impl ValidatedLeaseSet {
    pub fn validate(
        value: LeaseSet,
        expected: Option<DestinationHash>,
        context: LeaseSetValidationContext,
    ) -> Result<Self, LeaseSetValidationError> {
        let encoded_len = value.encode_to_vec(context.max_encoded_len)?.len();
        if encoded_len > context.max_encoded_len {
            return Err(LeaseSetValidationError::EncodedTooLarge);
        }
        let key = DestinationHash::from_hash(value.destination().hash()?);
        if expected.is_some_and(|expected| expected != key) {
            return Err(LeaseSetValidationError::DestinationMismatch);
        }
        verify_signature(
            value.destination().signing_key(),
            value.signed_bytes(),
            value.signature(),
        )
        .map_err(|error| match error {
            CryptoError::InvalidSignature => LeaseSetValidationError::InvalidSignature,
            other => LeaseSetValidationError::Crypto(other),
        })?;
        let version_ms = value
            .leases()
            .iter()
            .map(|lease| lease.end_date().as_millis())
            .min()
            .ok_or(LeaseSetValidationError::NoLeases)?;
        if version_ms <= context.now_ms {
            return Err(LeaseSetValidationError::Expired);
        }
        Ok(Self {
            key,
            value,
            version_ms,
            encoded_len,
        })
    }
    pub const fn key(&self) -> DestinationHash {
        self.key
    }
    pub const fn version_ms(&self) -> u64 {
        self.version_ms
    }
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }
    pub const fn value(&self) -> &LeaseSet {
        &self.value
    }
}

/// Insertion result for the classic LeaseSet store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSetInsertOutcome {
    Inserted,
    Replaced,
    Idempotent,
    Conflict,
    Stale,
    CapacityExceeded,
}

/// Independent count/byte quotas for classic LeaseSets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSetStoreConfig {
    pub max_records: usize,
    pub max_total_encoded_bytes: usize,
}
impl Default for LeaseSetStoreConfig {
    fn default() -> Self {
        Self {
            max_records: 2_048,
            max_total_encoded_bytes: 2 * 1024 * 1024,
        }
    }
}

/// Bounded classic LeaseSet store with deterministic earliest-expiry replacement.
#[derive(Debug, Default)]
pub struct LeaseSetStore {
    records: BTreeMap<DestinationHash, ValidatedLeaseSet>,
    total_bytes: usize,
    config: LeaseSetStoreConfig,
}

impl LeaseSetStore {
    pub fn with_config(config: LeaseSetStoreConfig) -> Self {
        Self {
            records: BTreeMap::new(),
            total_bytes: 0,
            config,
        }
    }
    pub fn insert(&mut self, candidate: ValidatedLeaseSet) -> LeaseSetInsertOutcome {
        let key = candidate.key();
        let size = candidate.encoded_len();
        if let Some(old) = self.records.get(&key) {
            if candidate.version_ms() < old.version_ms() {
                return LeaseSetInsertOutcome::Stale;
            }
            if candidate.version_ms() == old.version_ms() {
                return if candidate == *old {
                    LeaseSetInsertOutcome::Idempotent
                } else {
                    LeaseSetInsertOutcome::Conflict
                };
            }
            let next = self
                .total_bytes
                .checked_sub(old.encoded_len())
                .and_then(|v| v.checked_add(size));
            if next.is_none_or(|v| v > self.config.max_total_encoded_bytes) {
                return LeaseSetInsertOutcome::CapacityExceeded;
            }
            self.total_bytes = next.expect("checked above");
            self.records.insert(key, candidate);
            return LeaseSetInsertOutcome::Replaced;
        }
        let next = self.total_bytes.checked_add(size);
        if self.records.len() >= self.config.max_records
            || next.is_none_or(|v| v > self.config.max_total_encoded_bytes)
        {
            return LeaseSetInsertOutcome::CapacityExceeded;
        }
        self.total_bytes = next.expect("checked above");
        self.records.insert(key, candidate);
        LeaseSetInsertOutcome::Inserted
    }
    pub fn get(&self, key: &DestinationHash) -> Option<&ValidatedLeaseSet> {
        self.records.get(key)
    }
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn encoded_bytes(&self) -> usize {
        self.total_bytes
    }
    pub fn remove(&mut self, key: &DestinationHash) -> bool {
        if let Some(record) = self.records.remove(key) {
            self.total_bytes = self
                .total_bytes
                .checked_sub(record.encoded_len())
                .expect("classic LeaseSet accounting");
            true
        } else {
            false
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (&DestinationHash, &ValidatedLeaseSet)> {
        self.records.iter()
    }
}

/// A MetaLeaseSet that passed signature, key-binding, and expiration validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedMetaLeaseSet {
    key: DestinationHash,
    value: MetaLeaseSet,
    published_seconds: u32,
    expires_seconds: u32,
    encoded_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetaLeaseSetInsertOutcome {
    Inserted,
    Replaced,
    Idempotent,
    Conflict,
    Stale,
    CapacityExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetaLeaseSetStoreConfig {
    pub max_records: usize,
    pub max_total_encoded_bytes: usize,
}
impl Default for MetaLeaseSetStoreConfig {
    fn default() -> Self {
        Self {
            max_records: 1_024,
            max_total_encoded_bytes: 2 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct MetaLeaseSetStore {
    records: BTreeMap<DestinationHash, ValidatedMetaLeaseSet>,
    total_bytes: usize,
    config: MetaLeaseSetStoreConfig,
}
impl Default for MetaLeaseSetStore {
    fn default() -> Self {
        Self::with_config(MetaLeaseSetStoreConfig::default())
    }
}
impl MetaLeaseSetStore {
    pub fn with_config(config: MetaLeaseSetStoreConfig) -> Self {
        Self {
            records: BTreeMap::new(),
            total_bytes: 0,
            config,
        }
    }
    pub fn insert(&mut self, candidate: ValidatedMetaLeaseSet) -> MetaLeaseSetInsertOutcome {
        let key = candidate.key();
        let size = candidate.encoded_len();
        if let Some(old) = self.records.get(&key) {
            if candidate.published_seconds() < old.published_seconds() {
                return MetaLeaseSetInsertOutcome::Stale;
            }
            if candidate.published_seconds() == old.published_seconds() {
                return if candidate == *old {
                    MetaLeaseSetInsertOutcome::Idempotent
                } else {
                    MetaLeaseSetInsertOutcome::Conflict
                };
            }
            let next = self
                .total_bytes
                .checked_sub(old.encoded_len())
                .and_then(|v| v.checked_add(size));
            if next.is_none_or(|v| v > self.config.max_total_encoded_bytes) {
                return MetaLeaseSetInsertOutcome::CapacityExceeded;
            }
            self.total_bytes = next.expect("checked above");
            self.records.insert(key, candidate);
            return MetaLeaseSetInsertOutcome::Replaced;
        }
        let next = self.total_bytes.checked_add(size);
        if self.records.len() >= self.config.max_records
            || next.is_none_or(|v| v > self.config.max_total_encoded_bytes)
        {
            return MetaLeaseSetInsertOutcome::CapacityExceeded;
        }
        self.total_bytes = next.expect("checked above");
        self.records.insert(key, candidate);
        MetaLeaseSetInsertOutcome::Inserted
    }
    pub fn remove(&mut self, key: &DestinationHash) -> bool {
        if let Some(record) = self.records.remove(key) {
            self.total_bytes = self
                .total_bytes
                .checked_sub(record.encoded_len())
                .expect("MetaLeaseSet accounting");
            true
        } else {
            false
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (&DestinationHash, &ValidatedMetaLeaseSet)> {
        self.records.iter()
    }
    pub fn get(&self, key: &DestinationHash) -> Option<&ValidatedMetaLeaseSet> {
        self.records.get(key)
    }
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn encoded_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl ValidatedMetaLeaseSet {
    pub fn validate(
        value: MetaLeaseSet,
        expected: Option<DestinationHash>,
        context: LeaseSetValidationContext,
    ) -> Result<Self, LeaseSetValidationError> {
        let encoded_len = value.encode_to_vec(context.max_encoded_len)?.len();
        if encoded_len > context.max_encoded_len {
            return Err(LeaseSetValidationError::EncodedTooLarge);
        }
        let destination = value.header().destination();
        let key = DestinationHash::from_hash(destination.hash()?);
        if expected.is_some_and(|expected| expected != key) {
            return Err(LeaseSetValidationError::DestinationMismatch);
        }
        verify_meta_lease_set(&value).map_err(|error| match error {
            CryptoError::InvalidSignature => LeaseSetValidationError::InvalidSignature,
            other => LeaseSetValidationError::Crypto(other),
        })?;
        let published_seconds = value.header().published_seconds();
        let expires_seconds = value.header().expires_seconds();
        let now_seconds = u32::try_from(context.now_ms / 1000).unwrap_or(u32::MAX);
        if value
            .header()
            .offline_signature()
            .is_some_and(|offline| offline.expires_seconds() <= now_seconds)
        {
            return Err(LeaseSetValidationError::Expired);
        }
        if expires_seconds <= now_seconds
            || !value
                .entries()
                .iter()
                .any(|entry| entry.end_date().as_seconds() > now_seconds)
        {
            return Err(LeaseSetValidationError::Expired);
        }
        Ok(Self {
            key,
            value,
            published_seconds,
            expires_seconds,
            encoded_len,
        })
    }
    pub const fn key(&self) -> DestinationHash {
        self.key
    }
    pub const fn published_seconds(&self) -> u32 {
        self.published_seconds
    }
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }
    pub const fn value(&self) -> &MetaLeaseSet {
        &self.value
    }
    pub fn is_unpublished(&self) -> bool {
        self.value.header().flags().is_unpublished()
    }
    pub fn disclosure_block(&self) -> Option<LeaseSetDisclosureBlock> {
        let flags = self.value.header().flags();
        if flags.is_unpublished() {
            Some(LeaseSetDisclosureBlock::Unpublished)
        } else if flags.is_blinded_on_publication() {
            Some(LeaseSetDisclosureBlock::BlindedPublicationDeferred)
        } else {
            None
        }
    }
    pub fn is_blinded_on_publication(&self) -> bool {
        self.value.header().flags().is_blinded_on_publication()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::{ROUTER_SIGNING_KEY_TYPE, RouterIdentityBundle};
    use i2pr_proto::{
        CryptoKeyType, Date, Date32, Hash, Lease, LeaseSet2Flags, LeaseSet2Header, Mapping,
        MetaLease, MetaLeaseSet, PublicKey, SignatureValue,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn record() -> (LeaseSet, RouterIdentityBundle) {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let signer = RouterIdentityBundle::generate(&mut rng).unwrap();
        let destination =
            i2pr_proto::Destination::new(signer.identity().key_and_cert().clone()).unwrap();
        let encryption = PublicKey::new(CryptoKeyType::ElGamal, vec![0x55; 256]).unwrap();
        let signing = destination.signing_key().clone();
        let lease = Lease::new(
            Hash::from_bytes([0x22; 32]),
            7,
            Date::from_millis(2_000_000),
        );
        let placeholder = SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0; 64]).unwrap();
        let unsigned = LeaseSet::new(
            destination.clone(),
            encryption.clone(),
            signing.clone(),
            vec![lease.clone()],
            placeholder,
        )
        .unwrap();
        let signature = signer.signing_key().sign(unsigned.signed_bytes()).unwrap();
        let value =
            LeaseSet::new(destination, encryption, signing, vec![lease], signature).unwrap();
        (value, signer)
    }

    #[test]
    fn validates_signature_key_and_expiry_then_stores_with_bounded_capacity() {
        let (value, _) = record();
        let now = 1_000_000;
        let validated =
            ValidatedLeaseSet::validate(value, None, LeaseSetValidationContext::new(now)).unwrap();
        let mut store = LeaseSetStore::with_config(LeaseSetStoreConfig {
            max_records: 1,
            max_total_encoded_bytes: 4096,
        });
        assert_eq!(
            store.insert(validated.clone()),
            LeaseSetInsertOutcome::Inserted
        );
        assert_eq!(store.insert(validated), LeaseSetInsertOutcome::Idempotent);
        assert_eq!(store.len(), 1);
        let (expired, _) = record();
        assert_eq!(
            ValidatedLeaseSet::validate(expired, None, LeaseSetValidationContext::new(3_000_000)),
            Err(LeaseSetValidationError::Expired)
        );
    }

    #[test]
    fn classic_leaseset_rejects_signature_and_destination_key_mismatch() {
        let (value, _) = record();
        let mut signature_bytes = value.signature().as_bytes().to_vec();
        signature_bytes[0] ^= 1;
        let bad_signature = SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, signature_bytes).unwrap();
        let malformed = LeaseSet::new(
            value.destination().clone(),
            value.encryption_key().clone(),
            value.signing_key().clone(),
            value.leases().to_vec(),
            bad_signature,
        )
        .unwrap();
        assert_eq!(
            ValidatedLeaseSet::validate(malformed, None, LeaseSetValidationContext::new(1_000_000))
                .unwrap_err(),
            LeaseSetValidationError::InvalidSignature
        );

        let (value, _) = record();
        assert_eq!(
            ValidatedLeaseSet::validate(
                value,
                Some(DestinationHash::from_hash(Hash::from_bytes([0x99; 32]))),
                LeaseSetValidationContext::new(1_000_000),
            )
            .unwrap_err(),
            LeaseSetValidationError::DestinationMismatch
        );
    }

    #[test]
    fn metaleaseset_codec_signature_domain_freshness_and_store() {
        let mut rng = ChaCha8Rng::seed_from_u64(84);
        let signer = RouterIdentityBundle::generate(&mut rng).unwrap();
        let destination =
            i2pr_proto::Destination::new(signer.identity().key_and_cert().clone()).unwrap();
        let now = 1_000_u32;
        let header =
            LeaseSet2Header::new(destination.clone(), now, 3600, LeaseSet2Flags::from_raw(0))
                .unwrap();
        let entry = MetaLease::new(
            Hash::from_bytes([0x33; 32]),
            3,
            1,
            Date32::from_seconds(now + 1200),
        )
        .unwrap();
        let placeholder = SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0; 64]).unwrap();
        let unsigned = MetaLeaseSet::new(
            header.clone(),
            Mapping::empty(),
            vec![entry],
            vec![],
            placeholder,
        )
        .unwrap();
        let signature = signer
            .signing_key()
            .sign(&unsigned.signature_preimage())
            .unwrap();
        let record =
            MetaLeaseSet::new(header, Mapping::empty(), vec![entry], vec![], signature).unwrap();
        let encoded = record.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE).unwrap();
        let decoded = MetaLeaseSet::decode(&encoded, MAX_COMMON_STRUCTURE_SIZE).unwrap();
        assert_eq!(decoded, record);
        assert_eq!(decoded.signature_preimage()[0], 7);
        let database_store = i2pr_proto::DatabaseStoreMessage {
            key: destination.hash().unwrap(),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: i2pr_proto::DatabaseStoreData::MetaLeaseSet(Box::new(decoded.clone())),
        };
        let envelope = i2pr_proto::I2npMessage::new_standard(
            1,
            i2pr_proto::Date::from_millis(0),
            i2pr_proto::I2npBody::DatabaseStore(Box::new(database_store)),
        )
        .unwrap();
        let wire = envelope
            .encode_standard_to_vec(MAX_COMMON_STRUCTURE_SIZE)
            .unwrap();
        let reparsed =
            i2pr_proto::I2npMessage::decode_standard(&wire, MAX_COMMON_STRUCTURE_SIZE).unwrap();
        assert!(
            matches!(reparsed.body(), i2pr_proto::I2npBody::DatabaseStore(store) if matches!(store.data, i2pr_proto::DatabaseStoreData::MetaLeaseSet(_)))
        );
        let validated = ValidatedMetaLeaseSet::validate(
            decoded,
            None,
            LeaseSetValidationContext::new(u64::from(now) * 1000),
        )
        .unwrap();
        let mut store = MetaLeaseSetStore::default();
        assert_eq!(
            store.insert(validated.clone()),
            MetaLeaseSetInsertOutcome::Inserted
        );
        assert_eq!(
            store.insert(validated),
            MetaLeaseSetInsertOutcome::Idempotent
        );
        assert_eq!(store.len(), 1);
    }
}
