//! Plan 119 §10: LeaseSet2 validation, freshness policy, and bounded
//! in-memory store.
//!
//! `ValidatedLeaseSet2` is constructed only through
//! `ValidatedLeaseSet2::from_lease_set2`. The constructor enforces the
//! full Plan 119 Phase H checklist:
//!
//! ```text
//! signature valid
//! DatabaseStore key matches Destination hash
//! ordinary supported LS2 flags/policy
//! at least one usable X25519 encryption key
//! lease count nonzero and bounded
//! lease expirations sane and not all expired
//! published/expires fields checked with caller-supplied now
//! entry total size bounded
//! ```
//!
//! The store mirrors `RouterInfoStore`'s bounded record-count and
//! aggregate-byte accounting but is keyed by `DestinationHash` and
//! indexed independently of the RouterInfo store so one entry class
//! cannot starve the other.

use std::collections::BTreeMap;

use i2pr_crypto::{CryptoError, verify_lease_set2};
use i2pr_proto::{CryptoKeyType, Date32, Hash, LeaseSet2, MAX_COMMON_STRUCTURE_SIZE};
use thiserror::Error;

/// The NetDB-tracked SHA-256 hash of a Standard LeaseSet2.
///
/// The hash is derived from the canonical encoded Destination embedded
/// in the LS2 (the same hash used by the I2P NetDB as the LeaseSet2
/// key on the wire).
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DestinationHash(Hash);

impl DestinationHash {
    /// Constructs a `DestinationHash` from an already-derived
    /// destination hash.
    pub const fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Returns the underlying protocol hash.
    pub const fn as_hash(&self) -> &Hash {
        &self.0
    }

    /// Returns the raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl core::fmt::Debug for DestinationHash {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("DestinationHash(..)")
    }
}

/// Errors produced by [`ValidatedLeaseSet2::from_lease_set2`].
#[derive(Debug, Error, Eq, PartialEq)]
pub enum LeaseSet2ValidationError {
    /// The LeaseSet2 exceeded the caller-supplied encoded length cap.
    #[error("leaseSet2 encoded length {actual} exceeds {maximum}-byte limit")]
    EncodedTooLarge {
        /// Actual encoded length.
        actual: usize,
        /// Maximum length accepted by the validator.
        maximum: usize,
    },
    /// The signature did not verify against the contained destination
    /// signing key and the retained signed bytes.
    #[error("leaseSet2 signature verification failed")]
    InvalidSignature,
    /// The expected destination hash did not match the contained
    /// destination.
    #[error("leaseSet2 destination hash mismatch")]
    DestinationMismatch,
    /// The LeaseSet2 carried no X25519 encryption key usable for
    /// ECIES-X25519-AEAD-Ratchet routing.
    #[error("leaseSet2 carries no usable X25519 encryption key")]
    NoUsableX25519,
    /// The LeaseSet2 carried a duplicate X25519 encryption key.
    #[error("leaseSet2 carries more than one X25519 encryption key")]
    DuplicateX25519,
    /// Every Lease2 in the LeaseSet2 was already expired relative to
    /// the supplied `now`.
    #[error("leaseSet2 carries no unexpired leases")]
    AllLeasesExpired,
    /// The LeaseSet2's `published` timestamp was further into the
    /// future than the tolerated clock skew.
    #[error("leaseSet2 published skew {skew_secs}s exceeds {max_skew_secs}s")]
    ExcessiveFuture {
        /// Future skew in seconds.
        skew_secs: u64,
        /// Maximum future skew accepted.
        max_skew_secs: u64,
    },
    /// The LeaseSet2's absolute expires timestamp fell in the past
    /// relative to the supplied `now`.
    #[error("leaseSet2 already expired at {expires_secs}s vs now {now_secs}s")]
    Expired {
        /// Expires timestamp.
        expires_secs: u32,
        /// Current local time.
        now_secs: u32,
    },
    /// The encoded LeaseSet2 length overflowed the validator's
    /// checked arithmetic.
    #[error("leaseSet2 length arithmetic overflow")]
    ArithmeticOverflow,
    /// The underlying cryptographic or protocol codec failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

/// Caller-supplied freshness policy for `LeaseSet2` validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSet2ValidationPolicy {
    /// Maximum future skew tolerated between the local clock and the
    /// published timestamp when accepting a LeaseSet2.
    pub max_future_skew_seconds: u64,
    /// Maximum encoded length for a single LeaseSet2 in bytes.
    pub max_encoded_len: usize,
}

impl Default for LeaseSet2ValidationPolicy {
    fn default() -> Self {
        Self {
            max_future_skew_seconds: 60 * 60,
            max_encoded_len: MAX_COMMON_STRUCTURE_SIZE,
        }
    }
}

impl LeaseSet2ValidationPolicy {
    /// Constructs a custom policy.
    pub const fn new(max_future_skew_seconds: u64, max_encoded_len: usize) -> Self {
        Self {
            max_future_skew_seconds,
            max_encoded_len,
        }
    }
}

/// Caller-supplied validation context.
///
/// `now_seconds` is the current local time as an I2P seconds-since-epoch
/// timestamp. The validator never reads the wall clock itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSet2ValidationContext {
    /// Current local time as I2P seconds since epoch.
    pub now_seconds: u32,
    /// Validation policy.
    pub policy: LeaseSet2ValidationPolicy,
}

impl LeaseSet2ValidationContext {
    /// Creates a context with the default policy.
    pub const fn new(now_seconds: u32) -> Self {
        Self {
            now_seconds,
            policy: LeaseSet2ValidationPolicy {
                max_future_skew_seconds: 60 * 60,
                max_encoded_len: MAX_COMMON_STRUCTURE_SIZE,
            },
        }
    }

    /// Creates a context with a custom policy.
    pub const fn with_policy(now_seconds: u32, policy: LeaseSet2ValidationPolicy) -> Self {
        Self {
            now_seconds,
            policy,
        }
    }
}

/// A `LeaseSet2` that has passed every Plan 119 validation gate.
///
/// The type is constructed only through
/// [`ValidatedLeaseSet2::from_lease_set2`] so no caller can bypass
/// cryptographic or temporal validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedLeaseSet2 {
    key: DestinationHash,
    lease_set2: LeaseSet2,
    encoded_len: usize,
}

impl ValidatedLeaseSet2 {
    /// Validates a `LeaseSet2` and returns the wrapped record on
    /// success. The `expected_key` argument is optional; when supplied
    /// it must equal the derived `DestinationHash` or validation fails
    /// with `LeaseSet2ValidationError::DestinationMismatch`.
    ///
    /// Validation order is the fail-closed sequence from Plan 119
    /// Phase H: length → key derivation → expected-key check → signature
    /// verification → crypto policy → freshness policy → wrap.
    pub fn from_lease_set2(
        lease_set2: LeaseSet2,
        expected_key: Option<DestinationHash>,
        context: LeaseSet2ValidationContext,
    ) -> Result<Self, LeaseSet2ValidationError> {
        let signed_len = lease_set2.signed_bytes().len();
        let signature_len = lease_set2.signature().as_bytes().len();
        let encoded_len = signed_len
            .checked_add(signature_len)
            .ok_or(LeaseSet2ValidationError::ArithmeticOverflow)?;
        if encoded_len > context.policy.max_encoded_len {
            return Err(LeaseSet2ValidationError::EncodedTooLarge {
                actual: encoded_len,
                maximum: context.policy.max_encoded_len,
            });
        }

        let key = match lease_set2.key_hash() {
            Ok(hash) => DestinationHash::from_hash(hash),
            Err(error) => {
                return Err(LeaseSet2ValidationError::Crypto(CryptoError::Protocol(
                    error,
                )));
            }
        };
        if let Some(expected) = expected_key
            && expected != key
        {
            return Err(LeaseSet2ValidationError::DestinationMismatch);
        }

        if let Err(error) = verify_lease_set2(&lease_set2) {
            return Err(match error {
                CryptoError::InvalidSignature => LeaseSet2ValidationError::InvalidSignature,
                other => LeaseSet2ValidationError::Crypto(other),
            });
        }

        // Crypto policy: at least one usable X25519 encryption key.
        match lease_set2.usable_x25519_key() {
            Ok(_) => {}
            Err(error) => {
                return Err(match error {
                    i2pr_proto::LeaseSet2KeySelectionError::NoKeys
                    | i2pr_proto::LeaseSet2KeySelectionError::X25519NotFound => {
                        LeaseSet2ValidationError::NoUsableX25519
                    }
                    i2pr_proto::LeaseSet2KeySelectionError::DuplicateX25519 => {
                        LeaseSet2ValidationError::DuplicateX25519
                    }
                });
            }
        }

        // Freshness: published not too far in the future, expires not
        // already in the past, at least one lease unexpired.
        let now_secs = context.now_seconds;
        let published_secs = lease_set2.published_seconds();
        if published_secs > now_secs {
            let skew_secs = u64::from(published_secs.saturating_sub(now_secs));
            if skew_secs > context.policy.max_future_skew_seconds {
                return Err(LeaseSet2ValidationError::ExcessiveFuture {
                    skew_secs,
                    max_skew_secs: context.policy.max_future_skew_seconds,
                });
            }
        }
        let expires_secs = lease_set2.expires_seconds();
        if expires_secs <= now_secs {
            return Err(LeaseSet2ValidationError::Expired {
                expires_secs,
                now_secs,
            });
        }
        let any_unexpired = lease_set2
            .leases()
            .iter()
            .any(|lease| lease.end_date().as_seconds() > now_secs);
        if !any_unexpired {
            return Err(LeaseSet2ValidationError::AllLeasesExpired);
        }

        Ok(Self {
            key,
            lease_set2,
            encoded_len,
        })
    }

    /// Returns the canonical destination hash.
    pub const fn key(&self) -> DestinationHash {
        self.key
    }

    /// Borrows the underlying `LeaseSet2`.
    pub const fn lease_set2(&self) -> &LeaseSet2 {
        &self.lease_set2
    }

    /// Returns the encoded length that contributed to store accounting.
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    /// Returns the published timestamp in seconds.
    pub fn published_seconds(&self) -> u32 {
        self.lease_set2.published_seconds()
    }

    /// Returns the encoded LeaseSet2 bytes used to represent the
    /// record on the wire or in a persistent cache.
    pub fn encoded(&self, maximum: usize) -> Result<Vec<u8>, i2pr_proto::CodecError> {
        self.lease_set2.encode_to_vec(maximum)
    }
}

/// Configuration for [`LeaseSet2Store::with_config`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSet2StoreConfig {
    /// Maximum number of records retained by the store.
    pub max_records: usize,
    /// Maximum total encoded bytes retained by the store.
    pub max_total_encoded_bytes: usize,
}

impl Default for LeaseSet2StoreConfig {
    fn default() -> Self {
        Self {
            max_records: 4_096,
            max_total_encoded_bytes: 4 * 1024 * 1024,
        }
    }
}

impl LeaseSet2StoreConfig {
    /// Constructs a custom configuration.
    pub const fn new(max_records: usize, max_total_encoded_bytes: usize) -> Self {
        Self {
            max_records,
            max_total_encoded_bytes,
        }
    }
}

/// Privacy-safe aggregate statistics returned by the LeaseSet2 store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct LeaseSet2StoreStats {
    /// Number of records currently retained.
    pub record_count: usize,
    /// Total encoded bytes currently retained.
    pub total_encoded_bytes: usize,
    /// Configured record ceiling.
    pub max_records: usize,
    /// Configured byte ceiling.
    pub max_total_encoded_bytes: usize,
}

/// Outcome of a [`LeaseSet2Store::insert`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSet2InsertOutcome {
    /// The record was inserted as a new entry.
    Inserted,
    /// A byte-identical record was already present; the insert is a
    /// no-op.
    Idempotent,
    /// An older record for the same DestinationHash was replaced by a
    /// strictly newer published record.
    Replaced,
    /// A record with the same DestinationHash and publication timestamp
    /// but different bytes was rejected; the existing record is
    /// preserved.
    Conflict,
    /// A strictly older record for the same DestinationHash was rejected.
    StaleReplacement,
    /// The store is at capacity; the candidate was rejected without
    /// mutating existing state.
    CapacityExceeded,
}

/// A bounded in-memory store of validated LeaseSet2 records.
///
/// The store accepts only [`ValidatedLeaseSet2`] values; callers
/// cannot insert an unchecked record. Plan 119 §10 forbids an
/// `insert_unchecked` bypass.
#[derive(Debug)]
pub struct LeaseSet2Store {
    records: BTreeMap<DestinationHash, ValidatedLeaseSet2>,
    total_encoded_bytes: usize,
    config: LeaseSet2StoreConfig,
}

impl Default for LeaseSet2Store {
    fn default() -> Self {
        Self::with_config(LeaseSet2StoreConfig::default())
    }
}

impl LeaseSet2Store {
    /// Constructs a store with a custom configuration.
    pub fn with_config(config: LeaseSet2StoreConfig) -> Self {
        Self {
            records: BTreeMap::new(),
            total_encoded_bytes: 0,
            config,
        }
    }

    /// Inserts a validated record and returns the outcome.
    ///
    /// Replacement semantics mirror Plan 119 §10.4: a newer `published`
    /// replaces an older one; equal published + identical bytes is
    /// idempotent; equal published + different bytes is a conflict;
    /// older is `StaleReplacement`.
    pub fn insert(&mut self, validated: ValidatedLeaseSet2) -> LeaseSet2InsertOutcome {
        let key = validated.key();
        let encoded_len = validated.encoded_len();

        if let Some(existing) = self.records.get(&key) {
            let existing_published = existing.published_seconds();
            let incoming_published = validated.published_seconds();
            if incoming_published < existing_published {
                return LeaseSet2InsertOutcome::StaleReplacement;
            }
            if incoming_published == existing_published {
                if same_record_bytes(existing, &validated) {
                    return LeaseSet2InsertOutcome::Idempotent;
                }
                return LeaseSet2InsertOutcome::Conflict;
            }
            // incoming > existing; replacement path.
            if encoded_len > existing.encoded_len() {
                let extra = encoded_len - existing.encoded_len();
                if self
                    .total_encoded_bytes
                    .checked_add(extra)
                    .is_none_or(|total| total > self.config.max_total_encoded_bytes)
                {
                    return LeaseSet2InsertOutcome::CapacityExceeded;
                }
            }
            self.total_encoded_bytes = self
                .total_encoded_bytes
                .checked_sub(existing.encoded_len())
                .expect("accounting underflow on replace");
            self.total_encoded_bytes = self
                .total_encoded_bytes
                .checked_add(encoded_len)
                .expect("accounting overflow on replace");
            self.records.insert(key, validated);
            return LeaseSet2InsertOutcome::Replaced;
        }

        if self.records.len() >= self.config.max_records {
            return LeaseSet2InsertOutcome::CapacityExceeded;
        }
        if self
            .total_encoded_bytes
            .checked_add(encoded_len)
            .is_none_or(|total| total > self.config.max_total_encoded_bytes)
        {
            return LeaseSet2InsertOutcome::CapacityExceeded;
        }
        self.total_encoded_bytes = self
            .total_encoded_bytes
            .checked_add(encoded_len)
            .expect("accounting overflow on insert");
        self.records.insert(key, validated);
        LeaseSet2InsertOutcome::Inserted
    }

    /// Returns the record for the supplied key, if any.
    pub fn get(&self, key: &DestinationHash) -> Option<&ValidatedLeaseSet2> {
        self.records.get(key)
    }

    /// Returns whether the store currently retains a record for the
    /// supplied key.
    pub fn contains(&self, key: &DestinationHash) -> bool {
        self.records.contains_key(key)
    }

    /// Removes the record for the supplied key.
    pub fn remove(&mut self, key: &DestinationHash) -> bool {
        match self.records.remove(key) {
            Some(removed) => {
                self.total_encoded_bytes = self
                    .total_encoded_bytes
                    .checked_sub(removed.encoded_len())
                    .expect("accounting underflow on remove");
                true
            }
            None => false,
        }
    }

    /// Returns the number of records currently retained.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Returns the aggregate encoded byte count.
    pub fn encoded_bytes(&self) -> usize {
        self.total_encoded_bytes
    }

    /// Returns the store configuration.
    pub fn config(&self) -> LeaseSet2StoreConfig {
        self.config
    }

    /// Returns a privacy-safe aggregate snapshot.
    pub fn stats(&self) -> LeaseSet2StoreStats {
        LeaseSet2StoreStats {
            record_count: self.records.len(),
            total_encoded_bytes: self.total_encoded_bytes,
            max_records: self.config.max_records,
            max_total_encoded_bytes: self.config.max_total_encoded_bytes,
        }
    }

    /// Iterates records in canonical DestinationHash order.
    pub fn iter(&self) -> impl Iterator<Item = (&DestinationHash, &ValidatedLeaseSet2)> {
        self.records.iter()
    }
}

fn same_record_bytes(left: &ValidatedLeaseSet2, right: &ValidatedLeaseSet2) -> bool {
    left.lease_set2().signed_bytes() == right.lease_set2().signed_bytes()
        && left.lease_set2().signature().as_bytes() == right.lease_set2().signature().as_bytes()
}

// Suppress the unused CryptoKeyType import until the re-export cycle
// tightens.
#[allow(dead_code)]
const _CRYPTO_KEY_TYPE_DOC: CryptoKeyType = CryptoKeyType::X25519;
#[allow(dead_code)]
const _DATE32_DOC: Date32 = Date32::from_seconds(0);

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::{ROUTER_SIGNING_KEY_TYPE, RouterIdentityBundle};
    use i2pr_proto::{
        CryptoKeyType, Date32, Destination, Hash, Lease2, LeaseSet2, LeaseSet2EncryptionKey,
        LeaseSet2Flags, LeaseSet2Header, Mapping, SignatureValue,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn dummy_pub() -> Vec<u8> {
        vec![0x55; 32]
    }

    fn destination_for(signer: &RouterIdentityBundle) -> Destination {
        Destination::new(signer.identity().key_and_cert().clone()).expect("destination")
    }

    fn build_ls2(
        signer: &RouterIdentityBundle,
        published_seconds: u32,
        expires_offset_seconds: u16,
        leases: Vec<Lease2>,
    ) -> LeaseSet2 {
        let destination = destination_for(signer);
        let header = LeaseSet2Header::new(
            destination,
            published_seconds,
            expires_offset_seconds,
            LeaseSet2Flags::from_raw(0),
        )
        .expect("header");
        let options = Mapping::empty();
        let encryption_keys = vec![
            LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, dummy_pub())
                .expect("encryption key"),
        ];
        let placeholder =
            SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64]).expect("placeholder");
        let unsigned = LeaseSet2::new(header, options, encryption_keys, leases, placeholder)
            .expect("unsigned ls2");
        let preimage = unsigned.signature_preimage();
        let signature = signer.signing_key().sign(&preimage).expect("sign");
        LeaseSet2::new(
            unsigned.header().clone(),
            unsigned.options().clone(),
            unsigned.encryption_keys().to_vec(),
            unsigned.leases().to_vec(),
            signature,
        )
        .expect("ls2")
    }

    fn validate(b: &RouterIdentityBundle, published_seconds: u32) -> ValidatedLeaseSet2 {
        let leases = vec![Lease2::new(
            Hash::from_bytes([0x11; 32]),
            7,
            Date32::from_seconds(published_seconds + 600),
        )];
        let ls2 = build_ls2(b, published_seconds, 3600, leases);
        ValidatedLeaseSet2::from_lease_set2(
            ls2,
            None,
            LeaseSet2ValidationContext::new(published_seconds),
        )
        .expect("validate")
    }

    fn key_for(signer: &RouterIdentityBundle) -> DestinationHash {
        DestinationHash::from_hash(destination_for(signer).hash().expect("hash"))
    }

    #[test]
    fn first_insert_returns_inserted_and_updates_accounting() {
        let signer = bundle(0x400);
        let mut store = LeaseSet2Store::default();
        let validated = validate(&signer, 1_000);
        let key = validated.key();
        let bytes = validated.encoded_len();
        assert_eq!(store.insert(validated), LeaseSet2InsertOutcome::Inserted);
        assert_eq!(store.len(), 1);
        assert_eq!(store.encoded_bytes(), bytes);
        assert!(store.contains(&key));
        assert_eq!(store.stats().record_count, 1);
        assert_eq!(store.stats().total_encoded_bytes, bytes);
    }

    #[test]
    fn byte_identical_reinsert_is_idempotent() {
        let signer = bundle(0x401);
        let mut store = LeaseSet2Store::default();
        let validated = validate(&signer, 1_000);
        store.insert(validated.clone());
        let outcome = store.insert(validated);
        assert_eq!(outcome, LeaseSet2InsertOutcome::Idempotent);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn newer_published_replaces_existing() {
        let signer = bundle(0x402);
        let mut store = LeaseSet2Store::default();
        store.insert(validate(&signer, 1_000));
        store.insert(validate(&signer, 1_500));
        assert_eq!(store.len(), 1);
        let record = store.get(&key_for(&signer)).expect("present");
        assert_eq!(record.published_seconds(), 1_500);
    }

    #[test]
    fn older_published_replacement_rejected_as_stale() {
        let signer = bundle(0x403);
        let mut store = LeaseSet2Store::default();
        store.insert(validate(&signer, 1_500));
        let outcome = store.insert(validate(&signer, 1_000));
        assert_eq!(outcome, LeaseSet2InsertOutcome::StaleReplacement);
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.get(&key_for(&signer)).unwrap().published_seconds(),
            1_500
        );
    }

    #[test]
    fn wrong_database_store_key_rejected() {
        let signer = bundle(0x404);
        let other = bundle(0x405);
        let validated = validate(&signer, 1_000);
        let bogus = DestinationHash::from_hash(
            *crate::router_info::router_hash(other.identity())
                .expect("hash")
                .as_hash(),
        );
        let error = ValidatedLeaseSet2::from_lease_set2(
            validated.lease_set2().clone(),
            Some(bogus),
            LeaseSet2ValidationContext::new(1_000),
        )
        .unwrap_err();
        assert_eq!(error, LeaseSet2ValidationError::DestinationMismatch);
    }

    #[test]
    fn invalid_signature_rejected() {
        let signer = bundle(0x406);
        let validated = validate(&signer, 1_000);
        let ls2 = validated.lease_set2().clone();
        let bad =
            SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0xab; 64]).expect("bad signature");
        let ls2 = LeaseSet2::new(
            ls2.header().clone(),
            ls2.options().clone(),
            ls2.encryption_keys().to_vec(),
            ls2.leases().to_vec(),
            bad,
        )
        .expect("ls2");
        let error =
            ValidatedLeaseSet2::from_lease_set2(ls2, None, LeaseSet2ValidationContext::new(1_000))
                .unwrap_err();
        assert!(matches!(error, LeaseSet2ValidationError::InvalidSignature));
    }

    #[test]
    fn expired_ls2_rejected() {
        let signer = bundle(0x407);
        let validated = build_ls2(
            &signer,
            0,
            600,
            vec![Lease2::new(
                Hash::from_bytes([0x22; 32]),
                7,
                Date32::from_seconds(60),
            )],
        );
        let error = ValidatedLeaseSet2::from_lease_set2(
            validated,
            None,
            LeaseSet2ValidationContext::new(60 * 60),
        )
        .unwrap_err();
        assert!(matches!(error, LeaseSet2ValidationError::Expired { .. }));
    }

    #[test]
    fn all_leases_expired_rejected() {
        let signer = bundle(0x408);
        let validated = build_ls2(
            &signer,
            0,
            60_000,
            vec![Lease2::new(
                Hash::from_bytes([0x22; 32]),
                7,
                Date32::from_seconds(10),
            )],
        );
        let error = ValidatedLeaseSet2::from_lease_set2(
            validated,
            None,
            LeaseSet2ValidationContext::new(60),
        )
        .unwrap_err();
        assert!(matches!(error, LeaseSet2ValidationError::AllLeasesExpired));
    }

    #[test]
    fn duplicate_is_idempotent() {
        let signer = bundle(0x409);
        let mut store = LeaseSet2Store::default();
        let v = validate(&signer, 1_000);
        store.insert(v.clone());
        assert_eq!(store.insert(v), LeaseSet2InsertOutcome::Idempotent);
    }

    #[test]
    fn record_count_quota_rejects_extra_inserts() {
        let signer = bundle(0x40a);
        let mut store = LeaseSet2Store::with_config(LeaseSet2StoreConfig::new(1, usize::MAX));
        store.insert(validate(&signer, 1_000));
        let other = bundle(0x40b);
        let outcome = store.insert(validate(&other, 1_000));
        assert_eq!(outcome, LeaseSet2InsertOutcome::CapacityExceeded);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn remove_releases_accounting() {
        let signer = bundle(0x40c);
        let mut store = LeaseSet2Store::default();
        let validated = validate(&signer, 1_000);
        let key = validated.key();
        let bytes = validated.encoded_len();
        store.insert(validated);
        assert_eq!(store.encoded_bytes(), bytes);
        assert!(store.remove(&key));
        assert_eq!(store.encoded_bytes(), 0);
        assert!(store.is_empty());
    }
}
