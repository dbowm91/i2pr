//! Bounded, runtime-neutral provenance and namespace policy for M12 NetDB serving.
//!
//! Provenance is kept separate from record validation. A validated record is not implicitly
//! eligible for answering, replication, or persistence.

use std::collections::BTreeMap;
use std::fmt;

use i2pr_proto::Hash;

/// Opaque identifier for an isolated client NetDB namespace.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClientNamespaceId(u64);

impl ClientNamespaceId {
    /// Creates a namespace id from a locally assigned non-secret integer.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Debug for ClientNamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientNamespaceId(..)")
    }
}

/// NetDB data isolation boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NetDbNamespace {
    MainRouter,
    Client(ClientNamespaceId),
}

/// How a record entered the local NetDB.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboundProvenance {
    AuthenticatedDirectPeer,
    RouterTunnel,
    ClientTunnel,
    Local,
}

/// Purpose of a received/stored record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorePurpose {
    PublishedStore,
    LookupResponse,
    FloodReplica,
    LocalPublication,
}

/// Publicly observable record identity. The digest is deliberately redacted in Debug.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecordId {
    record_type: u8,
    key: Hash,
}

impl RecordId {
    pub const fn new(record_type: u8, key: Hash) -> Self {
        Self { record_type, key }
    }
    pub const fn record_type(&self) -> u8 {
        self.record_type
    }
    pub const fn key(&self) -> &Hash {
        &self.key
    }
}

impl fmt::Debug for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordId")
            .field("record_type", &self.record_type)
            .field("key", &"[redacted]")
            .finish()
    }
}

/// Provenance attached to one validated record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordProvenance {
    pub namespace: NetDbNamespace,
    pub inbound: InboundProvenance,
    pub purpose: StorePurpose,
    pub observed_at_ms: u64,
}

/// Resource limits for provenance metadata, independent of record bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvenanceLimits {
    pub max_records: usize,
    pub max_accounted_bytes: usize,
}

impl Default for ProvenanceLimits {
    fn default() -> Self {
        Self {
            max_records: 8_192,
            max_accounted_bytes: 512 * 1024,
        }
    }
}

/// Stable typed result of a policy check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Eligibility {
    Allowed,
    WrongNamespace,
    LookupResponseOnly,
    ClientTunnelOnly,
    ReplicaCannotReflood,
    Expired,
    NotPublished,
    CapacityExceeded,
}

/// Bounded metadata index. Replacement swaps metadata in the same synchronous operation.
#[derive(Debug)]
pub struct ProvenanceIndex {
    entries: BTreeMap<RecordId, RecordProvenance>,
    accounted_bytes: usize,
    limits: ProvenanceLimits,
}

impl Default for ProvenanceIndex {
    fn default() -> Self {
        Self::new(ProvenanceLimits::default())
    }
}

impl ProvenanceIndex {
    pub fn new(limits: ProvenanceLimits) -> Self {
        Self {
            entries: BTreeMap::new(),
            accounted_bytes: 0,
            limits,
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn accounted_bytes(&self) -> usize {
        self.accounted_bytes
    }
    pub fn get(&self, id: &RecordId) -> Option<&RecordProvenance> {
        self.entries.get(id)
    }

    /// Iterates metadata in one namespace only, omitting stale entries and all records without
    /// answer eligibility. This is metadata-only; a caller must still retrieve the matching
    /// validated record from its corresponding store before disclosure.
    pub fn eligible_records(
        &self,
        namespace: NetDbNamespace,
        now_ms: u64,
        max_age_ms: u64,
    ) -> impl Iterator<Item = (&RecordId, &RecordProvenance)> {
        self.entries.iter().filter(move |(id, provenance)| {
            provenance.namespace == namespace
                && self.evaluate(id, namespace, now_ms, max_age_ms, false) == Eligibility::Allowed
        })
    }

    /// Attach or atomically replace provenance. Quota rejection leaves current state unchanged.
    pub fn insert(
        &mut self,
        id: RecordId,
        provenance: RecordProvenance,
    ) -> Result<(), Eligibility> {
        const ENTRY_COST: usize = 64;
        let old = usize::from(self.entries.contains_key(&id)) * ENTRY_COST;
        if old == 0 && self.entries.len() >= self.limits.max_records {
            return Err(Eligibility::CapacityExceeded);
        }
        let next = self
            .accounted_bytes
            .checked_sub(old)
            .and_then(|v| v.checked_add(ENTRY_COST))
            .ok_or(Eligibility::CapacityExceeded)?;
        if next > self.limits.max_accounted_bytes {
            return Err(Eligibility::CapacityExceeded);
        }
        self.entries.insert(id, provenance);
        self.accounted_bytes = next;
        Ok(())
    }

    pub fn remove(&mut self, id: &RecordId) -> bool {
        if self.entries.remove(id).is_some() {
            self.accounted_bytes -= 64;
            true
        } else {
            false
        }
    }

    pub fn prune(&mut self, now_ms: u64, max_age_ms: u64) -> usize {
        let stale: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(id, value)| {
                (now_ms.saturating_sub(value.observed_at_ms) > max_age_ms).then_some(*id)
            })
            .collect();
        stale.into_iter().filter(|id| self.remove(id)).count()
    }

    pub fn may_answer_router_lookup(
        &self,
        id: &RecordId,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Eligibility {
        self.evaluate(id, NetDbNamespace::MainRouter, now_ms, max_age_ms, false)
    }
    pub fn may_answer_client_lookup(
        &self,
        id: &RecordId,
        namespace: ClientNamespaceId,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Eligibility {
        self.evaluate(
            id,
            NetDbNamespace::Client(namespace),
            now_ms,
            max_age_ms,
            false,
        )
    }
    pub fn may_replicate(&self, id: &RecordId, now_ms: u64, max_age_ms: u64) -> Eligibility {
        let Some(p) = self.entries.get(id) else {
            return Eligibility::NotPublished;
        };
        if p.namespace != NetDbNamespace::MainRouter {
            return Eligibility::WrongNamespace;
        }
        if now_ms.saturating_sub(p.observed_at_ms) > max_age_ms {
            return Eligibility::Expired;
        }
        if p.purpose == StorePurpose::LookupResponse {
            return Eligibility::LookupResponseOnly;
        }
        if p.purpose == StorePurpose::FloodReplica {
            return Eligibility::ReplicaCannotReflood;
        }
        if p.inbound == InboundProvenance::ClientTunnel {
            return Eligibility::ClientTunnelOnly;
        }
        if p.purpose == StorePurpose::PublishedStore
            && p.inbound == InboundProvenance::AuthenticatedDirectPeer
        {
            return Eligibility::Allowed;
        }
        Eligibility::NotPublished
    }
    pub fn may_persist(&self, id: &RecordId, now_ms: u64, max_age_ms: u64) -> Eligibility {
        self.evaluate(id, NetDbNamespace::MainRouter, now_ms, max_age_ms, true)
    }
    fn evaluate(
        &self,
        id: &RecordId,
        namespace: NetDbNamespace,
        now_ms: u64,
        max_age_ms: u64,
        persistence: bool,
    ) -> Eligibility {
        let Some(p) = self.entries.get(id) else {
            return Eligibility::NotPublished;
        };
        if p.namespace != namespace {
            return Eligibility::WrongNamespace;
        }
        if now_ms.saturating_sub(p.observed_at_ms) > max_age_ms {
            return Eligibility::Expired;
        }
        if p.purpose == StorePurpose::LookupResponse {
            return Eligibility::LookupResponseOnly;
        }
        if !persistence && p.purpose == StorePurpose::FloodReplica {
            return Eligibility::ReplicaCannotReflood;
        }
        if p.inbound == InboundProvenance::ClientTunnel {
            return Eligibility::ClientTunnelOnly;
        }
        if matches!(
            p.purpose,
            StorePurpose::PublishedStore | StorePurpose::LocalPublication
        ) {
            return Eligibility::Allowed;
        }
        Eligibility::NotPublished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> RecordId {
        let mut bytes = [0; 32];
        bytes[0] = n;
        RecordId::new(2, Hash::from_bytes(bytes))
    }
    fn p(
        namespace: NetDbNamespace,
        inbound: InboundProvenance,
        purpose: StorePurpose,
    ) -> RecordProvenance {
        RecordProvenance {
            namespace,
            inbound,
            purpose,
            observed_at_ms: 10,
        }
    }

    #[test]
    fn isolates_namespaces_and_denies_response_and_replica_promotion() {
        let client = ClientNamespaceId::new(7);
        let mut index = ProvenanceIndex::default();
        index
            .insert(
                id(1),
                p(
                    NetDbNamespace::MainRouter,
                    InboundProvenance::AuthenticatedDirectPeer,
                    StorePurpose::PublishedStore,
                ),
            )
            .unwrap();
        index
            .insert(
                id(2),
                p(
                    NetDbNamespace::Client(client),
                    InboundProvenance::ClientTunnel,
                    StorePurpose::LookupResponse,
                ),
            )
            .unwrap();
        index
            .insert(
                id(3),
                p(
                    NetDbNamespace::MainRouter,
                    InboundProvenance::AuthenticatedDirectPeer,
                    StorePurpose::FloodReplica,
                ),
            )
            .unwrap();
        assert_eq!(
            index.may_answer_client_lookup(&id(1), client, 11, 100),
            Eligibility::WrongNamespace
        );
        assert_eq!(
            index.may_answer_router_lookup(&id(2), 11, 100),
            Eligibility::WrongNamespace
        );
        assert_eq!(index.may_replicate(&id(1), 11, 100), Eligibility::Allowed);
        assert_eq!(
            index.may_replicate(&id(2), 11, 100),
            Eligibility::WrongNamespace
        );
        assert_eq!(
            index.may_replicate(&id(3), 11, 100),
            Eligibility::ReplicaCannotReflood
        );
    }

    #[test]
    fn quota_rejection_and_replacement_are_atomic_and_debug_is_redacted() {
        let mut index = ProvenanceIndex::new(ProvenanceLimits {
            max_records: 1,
            max_accounted_bytes: 64,
        });
        let main = p(
            NetDbNamespace::MainRouter,
            InboundProvenance::AuthenticatedDirectPeer,
            StorePurpose::PublishedStore,
        );
        index.insert(id(1), main).unwrap();
        assert!(index.insert(id(2), main).is_err());
        assert_eq!(index.get(&id(1)), Some(&main));
        let replacement = p(
            NetDbNamespace::Client(ClientNamespaceId::new(9)),
            InboundProvenance::ClientTunnel,
            StorePurpose::LookupResponse,
        );
        index.insert(id(1), replacement).unwrap();
        assert_eq!(
            index.may_answer_router_lookup(&id(1), 11, 100),
            Eligibility::WrongNamespace
        );
        assert!(!format!("{:?}", id(1)).contains("010000"));
    }

    #[test]
    fn time_policy_expires_deterministically() {
        let mut index = ProvenanceIndex::default();
        index
            .insert(
                id(1),
                p(
                    NetDbNamespace::MainRouter,
                    InboundProvenance::Local,
                    StorePurpose::LocalPublication,
                ),
            )
            .unwrap();
        assert_eq!(
            index.may_answer_router_lookup(&id(1), 111, 100),
            Eligibility::Expired
        );
        assert_eq!(index.prune(111, 100), 1);
    }
}
