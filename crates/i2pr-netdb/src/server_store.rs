//! Provenance-aware, main-router server-authority record store.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;

use flate2::Compression;
use flate2::GzBuilder;
use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, DeferredPayload};

use crate::els2::{BlindedStorageKey, Els2InsertOutcome, Els2Store, ValidatedEncryptedLeaseSet2};
use crate::lease_set2::DestinationHash;
use crate::{
    InsertOutcome, LeaseSet2InsertOutcome, LeaseSet2Store, LeaseSetInsertOutcome, LeaseSetStore,
    MetaLeaseSetInsertOutcome, MetaLeaseSetStore, NetDbNamespace, ProvenanceEligibility,
    ProvenanceIndex, RecordId, RecordProvenance, RouterHash, RouterInfoStore, ValidatedLeaseSet,
    ValidatedLeaseSet2, ValidatedMetaLeaseSet, ValidatedRouterInfo,
};

/// Validated record classes admitted to main-router server-authority storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidatedNetDbRecord {
    RouterInfo(ValidatedRouterInfo),
    LeaseSet(ValidatedLeaseSet),
    LeaseSet2(ValidatedLeaseSet2),
    MetaLeaseSet(ValidatedMetaLeaseSet),
    /// A validated type-5 encrypted LeaseSet2, filed under its blinded storage key.
    ///
    /// A floodfill holds this record opaquely. It never learns the destination's
    /// unblinded public key, so it cannot derive the subcredential and cannot
    /// decrypt the payload; serving the stored bytes is the whole job.
    EncryptedLeaseSet2(ValidatedEncryptedLeaseSet2),
}

impl ValidatedNetDbRecord {
    pub fn id(&self) -> RecordId {
        match self {
            Self::RouterInfo(value) => RecordId::new(0, *value.key().as_hash()),
            Self::LeaseSet(value) => RecordId::new(1, *value.key().as_hash()),
            Self::LeaseSet2(value) => RecordId::new(3, *value.key().as_hash()),
            Self::MetaLeaseSet(value) => RecordId::new(7, *value.key().as_hash()),
            Self::EncryptedLeaseSet2(value) => RecordId::new(5, *value.storage_key().as_hash()),
        }
    }
    pub fn encoded_len(&self) -> usize {
        match self {
            Self::RouterInfo(value) => value.encoded_len(),
            Self::LeaseSet(value) => value.encoded_len(),
            Self::LeaseSet2(value) => value.encoded_len(),
            Self::MetaLeaseSet(value) => value.encoded_len(),
            Self::EncryptedLeaseSet2(value) => value.encoded_len(),
        }
    }
}

/// Global quota independent of each record-class store's own ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerNetDbConfig {
    pub max_records: usize,
    pub max_total_bytes: usize,
}
impl Default for ServerNetDbConfig {
    fn default() -> Self {
        Self {
            max_records: 8_192,
            max_total_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Typed result of inserting a validated record with explicit provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerInsertOutcome {
    Inserted,
    Replaced,
    Idempotent,
    Conflict,
    Stale,
    CapacityExceeded,
}

/// Synchronous record/provenance owner for future server-side NetDB paths.
#[derive(Debug)]
pub struct ServerNetDb {
    router_info: RouterInfoStore,
    leases: LeaseSetStore,
    leases2: LeaseSet2Store,
    meta_leases: MetaLeaseSetStore,
    els2: Els2Store,
    provenance: ProvenanceIndex,
    sizes: BTreeMap<RecordId, usize>,
    total_bytes: usize,
    config: ServerNetDbConfig,
}

impl Default for ServerNetDb {
    fn default() -> Self {
        Self::with_config(ServerNetDbConfig::default())
    }
}
impl ServerNetDb {
    pub fn with_config(config: ServerNetDbConfig) -> Self {
        Self {
            router_info: RouterInfoStore::default(),
            leases: LeaseSetStore::default(),
            leases2: LeaseSet2Store::default(),
            meta_leases: MetaLeaseSetStore::default(),
            els2: Els2Store::default(),
            provenance: ProvenanceIndex::default(),
            sizes: BTreeMap::new(),
            total_bytes: 0,
            config,
        }
    }
    pub fn insert(
        &mut self,
        record: ValidatedNetDbRecord,
        provenance: RecordProvenance,
    ) -> Result<ServerInsertOutcome, ProvenanceEligibility> {
        if provenance.namespace != NetDbNamespace::MainRouter {
            return Err(ProvenanceEligibility::WrongNamespace);
        }
        let id = record.id();
        self.provenance.can_insert(&id)?;
        let size = record.encoded_len();
        let old_size = self.sizes.get(&id).copied().unwrap_or(0);
        let next_bytes = self
            .total_bytes
            .checked_sub(old_size)
            .and_then(|value| value.checked_add(size));
        if (old_size == 0 && self.sizes.len() >= self.config.max_records)
            || next_bytes.is_none_or(|value| value > self.config.max_total_bytes)
        {
            return Ok(ServerInsertOutcome::CapacityExceeded);
        }
        let result = match record {
            ValidatedNetDbRecord::RouterInfo(value) => match self.router_info.insert(value) {
                InsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                InsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                InsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                InsertOutcome::Conflict => ServerInsertOutcome::Conflict,
                InsertOutcome::StaleReplacement => ServerInsertOutcome::Stale,
                InsertOutcome::CapacityExceeded => ServerInsertOutcome::CapacityExceeded,
            },
            ValidatedNetDbRecord::LeaseSet(value) => match self.leases.insert(value) {
                LeaseSetInsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                LeaseSetInsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                LeaseSetInsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                LeaseSetInsertOutcome::Conflict => ServerInsertOutcome::Conflict,
                LeaseSetInsertOutcome::Stale => ServerInsertOutcome::Stale,
                LeaseSetInsertOutcome::CapacityExceeded => ServerInsertOutcome::CapacityExceeded,
            },
            ValidatedNetDbRecord::LeaseSet2(value) => match self.leases2.insert(value) {
                LeaseSet2InsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                LeaseSet2InsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                LeaseSet2InsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                LeaseSet2InsertOutcome::Conflict => ServerInsertOutcome::Conflict,
                LeaseSet2InsertOutcome::StaleReplacement => ServerInsertOutcome::Stale,
                LeaseSet2InsertOutcome::CapacityExceeded => ServerInsertOutcome::CapacityExceeded,
            },
            ValidatedNetDbRecord::MetaLeaseSet(value) => match self.meta_leases.insert(value) {
                MetaLeaseSetInsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                MetaLeaseSetInsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                MetaLeaseSetInsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                MetaLeaseSetInsertOutcome::Conflict => ServerInsertOutcome::Conflict,
                MetaLeaseSetInsertOutcome::Stale => ServerInsertOutcome::Stale,
                MetaLeaseSetInsertOutcome::CapacityExceeded => {
                    ServerInsertOutcome::CapacityExceeded
                }
            },
            ValidatedNetDbRecord::EncryptedLeaseSet2(value) => match self.els2.insert(value) {
                Els2InsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                Els2InsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                Els2InsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                Els2InsertOutcome::Conflict => ServerInsertOutcome::Conflict,
                Els2InsertOutcome::StaleReplacement => ServerInsertOutcome::Stale,
                Els2InsertOutcome::CapacityExceeded => ServerInsertOutcome::CapacityExceeded,
            },
        };
        if matches!(
            result,
            ServerInsertOutcome::Inserted
                | ServerInsertOutcome::Replaced
                | ServerInsertOutcome::Idempotent
        ) {
            self.provenance.insert(id, provenance)?;
            self.total_bytes = next_bytes.expect("quota checked before synchronous update");
            self.sizes.insert(id, size);
        }
        Ok(result)
    }
    pub fn router_info_for_answer(
        &self,
        key: &RouterHash,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Result<Option<&ValidatedRouterInfo>, ProvenanceEligibility> {
        let id = RecordId::new(0, *key.as_hash());
        if self
            .provenance
            .may_answer_router_lookup(&id, now_ms, max_age_ms)
            != ProvenanceEligibility::Allowed
        {
            return Err(self
                .provenance
                .may_answer_router_lookup(&id, now_ms, max_age_ms));
        }
        let value = self.router_info.get(key);
        if value.is_some_and(|record| {
            now_ms.saturating_sub(record.published().as_millis()) > max_age_ms
        }) {
            return Err(ProvenanceEligibility::Expired);
        }
        if value.is_some_and(|record| {
            record
                .router_info()
                .capabilities()
                .ok()
                .flatten()
                .is_some_and(|caps| caps.as_str().contains('H'))
        }) {
            return Err(ProvenanceEligibility::NotPublished);
        }
        Ok(value)
    }

    /// Encodes one currently answer-eligible record into a token-zero DatabaseStore body.
    pub fn database_store_for_answer(
        &self,
        record_type: u8,
        key: i2pr_proto::Hash,
        now_ms: u64,
        max_age_ms: u64,
        max_encoded_bytes: usize,
    ) -> Result<Option<DatabaseStoreMessage>, ProvenanceEligibility> {
        let data = match record_type {
            0 => {
                let router_key = RouterHash::from_hash(key);
                let Some(record) = self.router_info_for_answer(&router_key, now_ms, max_age_ms)?
                else {
                    return Ok(None);
                };
                let raw = record
                    .router_info()
                    .encode_to_vec(max_encoded_bytes)
                    .map_err(|_| ProvenanceEligibility::CapacityExceeded)?;
                let mut compressor = GzBuilder::new()
                    .mtime(0)
                    .operating_system(0xff)
                    .write(Vec::new(), Compression::best());
                compressor
                    .write_all(&raw)
                    .map_err(|_| ProvenanceEligibility::CapacityExceeded)?;
                let compressed = compressor
                    .finish()
                    .map_err(|_| ProvenanceEligibility::CapacityExceeded)?;
                DatabaseStoreData::RouterInfoCompressed(
                    DeferredPayload::new(compressed, max_encoded_bytes)
                        .map_err(|_| ProvenanceEligibility::CapacityExceeded)?,
                )
            }
            1 => {
                let Some(record) =
                    self.lease_set_for_answer(DestinationHash::from_hash(key), now_ms, max_age_ms)?
                else {
                    return Ok(None);
                };
                DatabaseStoreData::LeaseSet(Box::new(record.value().clone()))
            }
            3 => {
                let Some(record) = self.lease_set2_for_answer(
                    DestinationHash::from_hash(key),
                    now_ms,
                    max_age_ms,
                )?
                else {
                    return Ok(None);
                };
                DatabaseStoreData::LeaseSet2(Box::new(record.lease_set2().clone()))
            }
            7 => {
                let Some(record) = self.meta_lease_set_for_answer(
                    DestinationHash::from_hash(key),
                    now_ms,
                    max_age_ms,
                )?
                else {
                    return Ok(None);
                };
                DatabaseStoreData::MetaLeaseSet(Box::new(record.value().clone()))
            }
            5 => {
                let Some(record) = self.encrypted_lease_set2_for_answer(
                    BlindedStorageKey::from_hash(key),
                    now_ms,
                    max_age_ms,
                )?
                else {
                    return Ok(None);
                };
                DatabaseStoreData::EncryptedLeaseSet(Box::new(record.record().clone()))
            }
            _ => return Ok(None),
        };
        let size = match &data {
            DatabaseStoreData::RouterInfoCompressed(value) => value.as_bytes().len(),
            DatabaseStoreData::LeaseSet(value) => value
                .encode_to_vec(max_encoded_bytes)
                .map_err(|_| ProvenanceEligibility::CapacityExceeded)?
                .len(),
            DatabaseStoreData::LeaseSet2(value) => value
                .encode_to_vec(max_encoded_bytes)
                .map_err(|_| ProvenanceEligibility::CapacityExceeded)?
                .len(),
            DatabaseStoreData::MetaLeaseSet(value) => value
                .encode_to_vec(max_encoded_bytes)
                .map_err(|_| ProvenanceEligibility::CapacityExceeded)?
                .len(),
            DatabaseStoreData::EncryptedLeaseSet(value) => value
                .encode_to_vec(max_encoded_bytes)
                .map_err(|_| ProvenanceEligibility::CapacityExceeded)?
                .len(),
            DatabaseStoreData::Deferred { .. } => return Ok(None),
        };
        if size > max_encoded_bytes {
            return Err(ProvenanceEligibility::CapacityExceeded);
        }
        Ok(Some(DatabaseStoreMessage {
            key,
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data,
        }))
    }

    /// Returns validated non-hidden RouterInfo candidates isolated to the main namespace.
    pub fn router_info_candidates(
        &self,
        now_ms: u64,
        max_age_ms: u64,
        want_floodfill: bool,
        excluded: &BTreeSet<i2pr_proto::Hash>,
        maximum_work: usize,
    ) -> Vec<RouterHash> {
        self.provenance
            .eligible_records(NetDbNamespace::MainRouter, now_ms, max_age_ms)
            .take(maximum_work)
            .filter_map(|(id, _)| {
                if id.record_type() != 0 || excluded.contains(id.key()) {
                    return None;
                }
                let key = RouterHash::from_hash(*id.key());
                let record = self
                    .router_info_for_answer(&key, now_ms, max_age_ms)
                    .ok()??;
                let caps = record.router_info().capabilities().ok().flatten();
                if caps
                    .as_ref()
                    .is_some_and(|value| value.as_str().contains('H'))
                {
                    return None;
                }
                let is_floodfill = caps
                    .as_ref()
                    .is_some_and(|value| value.as_str().contains('f'));
                (is_floodfill == want_floodfill).then_some(key)
            })
            .collect()
    }
    pub fn lease_set_for_answer(
        &self,
        key: DestinationHash,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Result<Option<&ValidatedLeaseSet>, ProvenanceEligibility> {
        let id = RecordId::new(1, *key.as_hash());
        let decision = self
            .provenance
            .may_answer_router_lookup(&id, now_ms, max_age_ms);
        if decision != ProvenanceEligibility::Allowed {
            return Err(decision);
        }
        let value = self.leases.get(&key);
        if value.is_some_and(|record| record.version_ms() <= now_ms) {
            return Err(ProvenanceEligibility::Expired);
        }
        Ok(value)
    }
    pub fn lease_set2_for_answer(
        &self,
        key: DestinationHash,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Result<Option<&ValidatedLeaseSet2>, ProvenanceEligibility> {
        let id = RecordId::new(3, *key.as_hash());
        let decision = self
            .provenance
            .may_answer_router_lookup(&id, now_ms, max_age_ms);
        if decision != ProvenanceEligibility::Allowed {
            return Err(decision);
        }
        let value = self.leases2.get(&key);
        let now_seconds = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
        if value.is_some_and(|record| {
            record.lease_set2().expires_seconds() <= now_seconds
                || record
                    .lease_set2()
                    .leases()
                    .iter()
                    .all(|lease| lease.end_date().as_seconds() <= now_seconds)
                || record
                    .lease_set2()
                    .header()
                    .offline_signature()
                    .is_some_and(|offline| offline.expires_seconds() <= now_seconds)
        }) {
            return Err(ProvenanceEligibility::Expired);
        }
        if value.is_some_and(|record| record.disclosure_block().is_some()) {
            return Err(ProvenanceEligibility::NotPublished);
        }
        Ok(value)
    }
    pub fn meta_lease_set_for_answer(
        &self,
        key: DestinationHash,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Result<Option<&ValidatedMetaLeaseSet>, ProvenanceEligibility> {
        let id = RecordId::new(7, *key.as_hash());
        let decision = self
            .provenance
            .may_answer_router_lookup(&id, now_ms, max_age_ms);
        if decision != ProvenanceEligibility::Allowed {
            return Err(decision);
        }
        let value = self.meta_leases.get(&key);
        let now_seconds = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
        if value.is_some_and(|record| {
            record.expires_seconds() <= now_seconds
                || record
                    .value()
                    .entries()
                    .iter()
                    .all(|entry| entry.end_date().as_seconds() <= now_seconds)
                || record
                    .value()
                    .header()
                    .offline_signature()
                    .is_some_and(|offline| offline.expires_seconds() <= now_seconds)
        }) {
            return Err(ProvenanceEligibility::Expired);
        }
        if value.is_some_and(|record| record.disclosure_block().is_some()) {
            return Err(ProvenanceEligibility::NotPublished);
        }
        Ok(value)
    }
    /// Returns a type-5 record when it is still answer-eligible.
    ///
    /// Eligibility is freshness only. Unlike an ordinary LeaseSet2 there is no
    /// disclosure flag to consult: a type-5 record is publishable by
    /// construction, because it contains no plaintext routing information and
    /// cannot be read by the floodfill serving it.
    pub fn encrypted_lease_set2_for_answer(
        &self,
        key: BlindedStorageKey,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Result<Option<&ValidatedEncryptedLeaseSet2>, ProvenanceEligibility> {
        let id = RecordId::new(5, *key.as_hash());
        let decision = self
            .provenance
            .may_answer_router_lookup(&id, now_ms, max_age_ms);
        if decision != ProvenanceEligibility::Allowed {
            return Err(decision);
        }
        let value = self.els2.get(&key);
        let now_seconds = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
        if value.is_some_and(|record| {
            record.expires_seconds() <= now_seconds
                || record
                    .record()
                    .offline_keys()
                    .is_some_and(|offline| offline.expires_seconds() <= now_seconds)
        }) {
            return Err(ProvenanceEligibility::Expired);
        }
        Ok(value)
    }
    pub fn may_replicate(
        &self,
        id: &RecordId,
        now_ms: u64,
        max_age_ms: u64,
    ) -> ProvenanceEligibility {
        let eligibility = self.provenance.may_replicate(id, now_ms, max_age_ms);
        if eligibility != ProvenanceEligibility::Allowed {
            return eligibility;
        }
        match id.record_type() {
            0 => self
                .router_info
                .get(&RouterHash::from_hash(*id.key()))
                .map(|record| {
                    let age = now_ms.saturating_sub(record.published().as_millis());
                    let hidden = record
                        .router_info()
                        .capabilities()
                        .ok()
                        .flatten()
                        .is_some_and(|caps| caps.as_str().contains('H'));
                    if hidden || age > max_age_ms || age > 60 * 60 * 1000 {
                        ProvenanceEligibility::NotPublished
                    } else {
                        ProvenanceEligibility::Allowed
                    }
                })
                .unwrap_or(ProvenanceEligibility::NotPublished),
            3 => {
                self.leases2
                    .get(&DestinationHash::from_hash(*id.key()))
                    .filter(|record| {
                        record.disclosure_block().is_none()
                            && record.lease_set2().expires_seconds()
                                > u32::try_from(now_ms / 1000).unwrap_or(u32::MAX)
                            && record.lease_set2().leases().iter().any(|lease| {
                                lease.end_date().as_seconds()
                                    > u32::try_from(now_ms / 1000).unwrap_or(u32::MAX)
                            })
                            && record.lease_set2().header().offline_signature().is_none_or(
                                |offline| {
                                    offline.expires_seconds()
                                        > u32::try_from(now_ms / 1000).unwrap_or(u32::MAX)
                                },
                            )
                    })
                    .map(|_| ProvenanceEligibility::Allowed)
                    .unwrap_or(ProvenanceEligibility::NotPublished)
            }
            7 => self
                .meta_leases
                .get(&DestinationHash::from_hash(*id.key()))
                .filter(|record| {
                    record.disclosure_block().is_none()
                        && record.expires_seconds()
                            > u32::try_from(now_ms / 1000).unwrap_or(u32::MAX)
                        && record.value().entries().iter().any(|entry| {
                            entry.end_date().as_seconds()
                                > u32::try_from(now_ms / 1000).unwrap_or(u32::MAX)
                        })
                        && record
                            .value()
                            .header()
                            .offline_signature()
                            .is_none_or(|offline| {
                                offline.expires_seconds()
                                    > u32::try_from(now_ms / 1000).unwrap_or(u32::MAX)
                            })
                })
                .map(|_| ProvenanceEligibility::Allowed)
                .unwrap_or(ProvenanceEligibility::NotPublished),
            1 => self
                .leases
                .get(&DestinationHash::from_hash(*id.key()))
                .filter(|record| record.version_ms() > now_ms)
                .map(|_| ProvenanceEligibility::Allowed)
                .unwrap_or(ProvenanceEligibility::NotPublished),
            // Replication of a type-5 record is freshness-only for the same
            // reason serving is: it holds no plaintext routing information.
            5 => self
                .els2
                .get(&BlindedStorageKey::from_hash(*id.key()))
                .filter(|record| {
                    let now_seconds = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
                    record.expires_seconds() > now_seconds
                        && record
                            .record()
                            .offline_keys()
                            .is_none_or(|offline| offline.expires_seconds() > now_seconds)
                })
                .map(|_| ProvenanceEligibility::Allowed)
                .unwrap_or(ProvenanceEligibility::NotPublished),
            _ => ProvenanceEligibility::NotPublished,
        }
    }
    pub fn may_persist(
        &self,
        id: &RecordId,
        now_ms: u64,
        max_age_ms: u64,
    ) -> ProvenanceEligibility {
        self.provenance.may_persist(id, now_ms, max_age_ms)
    }
    pub fn record_count(&self) -> usize {
        self.sizes.len()
    }
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Removes one record and its provenance/byte accounting as a single synchronous operation.
    pub fn remove_record(&mut self, id: &RecordId) -> bool {
        let removed = match id.record_type() {
            0 => self.router_info.remove(&RouterHash::from_hash(*id.key())),
            1 => self.leases.remove(&DestinationHash::from_hash(*id.key())),
            3 => self.leases2.remove(&DestinationHash::from_hash(*id.key())),
            7 => self
                .meta_leases
                .remove(&DestinationHash::from_hash(*id.key())),
            5 => self.els2.remove(&BlindedStorageKey::from_hash(*id.key())),
            _ => false,
        };
        if removed {
            let size = self.sizes.remove(id).unwrap_or(0);
            self.total_bytes = self
                .total_bytes
                .checked_sub(size)
                .expect("server NetDB accounting");
            self.provenance.remove(id);
        }
        removed
    }

    /// Expires at most `limit` records after `after`; the caller retains the returned cursor and
    /// resumes with it on the next tick. Expired payload bytes and provenance metadata are removed
    /// synchronously. A short batch marks the current pass complete.
    pub fn maintenance_batch(
        &mut self,
        after: Option<RecordId>,
        limit: usize,
        now_ms: u64,
        max_age_ms: u64,
    ) -> MaintenanceBatch {
        let ids = self.provenance.ids_after(after, limit.saturating_add(1));
        let complete = ids.len() <= limit;
        let examined: Vec<_> = ids.into_iter().take(limit).collect();
        let mut expired = 0;
        for id in &examined {
            let age_expired = self
                .provenance
                .get(id)
                .is_none_or(|p| now_ms.saturating_sub(p.observed_at_ms) > max_age_ms);
            let time_expired = match id.record_type() {
                0 => self
                    .router_info
                    .get(&RouterHash::from_hash(*id.key()))
                    .is_none_or(|r| now_ms.saturating_sub(r.published().as_millis()) > max_age_ms),
                1 => self
                    .leases
                    .get(&DestinationHash::from_hash(*id.key()))
                    .is_none_or(|r| r.version_ms() <= now_ms),
                3 => self
                    .leases2
                    .get(&DestinationHash::from_hash(*id.key()))
                    .is_none_or(|r| {
                        let now = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
                        r.lease_set2().expires_seconds() <= now
                            || r.lease_set2()
                                .leases()
                                .iter()
                                .all(|lease| lease.end_date().as_seconds() <= now)
                            || r.lease_set2()
                                .header()
                                .offline_signature()
                                .is_some_and(|offline| offline.expires_seconds() <= now)
                    }),
                7 => self
                    .meta_leases
                    .get(&DestinationHash::from_hash(*id.key()))
                    .is_none_or(|r| {
                        let now = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
                        r.expires_seconds() <= now
                            || r.value()
                                .entries()
                                .iter()
                                .all(|entry| entry.end_date().as_seconds() <= now)
                            || r.value()
                                .header()
                                .offline_signature()
                                .is_some_and(|offline| offline.expires_seconds() <= now)
                    }),
                _ => true,
            };
            if age_expired || time_expired {
                expired += usize::from(self.remove_record(id));
            }
        }
        MaintenanceBatch {
            next: if complete {
                None
            } else {
                examined.last().copied()
            },
            examined: examined.len(),
            expired,
            complete,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenanceBatch {
    pub next: Option<RecordId>,
    pub examined: usize,
    pub expired: usize,
    pub complete: bool,
}

/// Caller-time trigger for daily routing-key rollover maintenance. Backwards clock movement does
/// not trigger work; the owner retains its prior UTC day and checks again on a later tick.
pub fn daily_rollover_due(last_ms: u64, now_ms: u64) -> bool {
    now_ms >= last_ms && now_ms / 86_400_000 > last_ms / 86_400_000
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;

    #[test]
    fn maintenance_batch_is_bounded_and_marks_short_pass_complete() {
        let mut db = ServerNetDb::default();
        let first = db.maintenance_batch(None, 1, 100, 1000);
        assert_eq!(first.examined, 0);
        assert_eq!(first.expired, 0);
        assert!(first.complete);
        assert_eq!(db.total_bytes, 0);
        assert!(!daily_rollover_due(86_400_001, 86_399_999));
        assert!(daily_rollover_due(86_399_999, 86_400_000));
    }
}
