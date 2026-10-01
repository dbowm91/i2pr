//! Provenance-aware, main-router server-authority record store.

use std::collections::BTreeMap;

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
}

impl ValidatedNetDbRecord {
    pub fn id(&self) -> RecordId {
        match self {
            Self::RouterInfo(value) => RecordId::new(0, *value.key().as_hash()),
            Self::LeaseSet(value) => RecordId::new(1, *value.key().as_hash()),
            Self::LeaseSet2(value) => RecordId::new(3, *value.key().as_hash()),
            Self::MetaLeaseSet(value) => RecordId::new(7, *value.key().as_hash()),
        }
    }
    pub fn encoded_len(&self) -> usize {
        match self {
            Self::RouterInfo(value) => value.encoded_len(),
            Self::LeaseSet(value) => value.encoded_len(),
            Self::LeaseSet2(value) => value.encoded_len(),
            Self::MetaLeaseSet(value) => value.encoded_len(),
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
    Rejected,
    CapacityExceeded,
}

/// Synchronous record/provenance owner for future server-side NetDB paths.
#[derive(Debug)]
pub struct ServerNetDb {
    router_info: RouterInfoStore,
    leases: LeaseSetStore,
    leases2: LeaseSet2Store,
    meta_leases: MetaLeaseSetStore,
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
                _ => ServerInsertOutcome::Rejected,
            },
            ValidatedNetDbRecord::LeaseSet(value) => match self.leases.insert(value) {
                LeaseSetInsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                LeaseSetInsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                LeaseSetInsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                _ => ServerInsertOutcome::Rejected,
            },
            ValidatedNetDbRecord::LeaseSet2(value) => match self.leases2.insert(value) {
                LeaseSet2InsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                LeaseSet2InsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                LeaseSet2InsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                _ => ServerInsertOutcome::Rejected,
            },
            ValidatedNetDbRecord::MetaLeaseSet(value) => match self.meta_leases.insert(value) {
                MetaLeaseSetInsertOutcome::Inserted => ServerInsertOutcome::Inserted,
                MetaLeaseSetInsertOutcome::Replaced => ServerInsertOutcome::Replaced,
                MetaLeaseSetInsertOutcome::Idempotent => ServerInsertOutcome::Idempotent,
                _ => ServerInsertOutcome::Rejected,
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
        Ok(self.router_info.get(key))
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
        Ok(self.leases.get(&key))
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
        if value.is_some_and(|record| record.disclosure_block().is_some()) {
            return Err(ProvenanceEligibility::NotPublished);
        }
        Ok(value)
    }
    pub fn may_replicate(
        &self,
        id: &RecordId,
        now_ms: u64,
        max_age_ms: u64,
    ) -> ProvenanceEligibility {
        self.provenance.may_replicate(id, now_ms, max_age_ms)
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
}
