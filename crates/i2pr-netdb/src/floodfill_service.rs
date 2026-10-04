//! Runtime-neutral inbound DatabaseStore floodfill service.
//!
//! The service owns classification, bounded validation/admission, throttling and typed effects.
//! Dispatch, DeliveryStatus routing and replication transport remain with the daemon/runtime.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use i2pr_crypto::seal_netdb_ecies_reply;
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseSearchReplyMessage, DatabaseStoreData, DatabaseStoreMessage,
    Date, DeliveryStatusMessage, Hash, I2npBody, MAX_DATABASE_LOOKUP_EXCLUDED_PEERS,
    MAX_DATABASE_SEARCH_REPLY_PEERS, MAX_I2NP_PAYLOAD_SIZE, ReplyEncryption, RouterInfo,
};

use crate::{
    BlindedStorageKey, DestinationHash, Els2ValidationContext, InboundProvenance,
    LeaseSet2ValidationContext, LeaseSetValidationContext, NetDbNamespace, RecordId,
    RecordProvenance, RouterHash, ServerInsertOutcome, ServerNetDb, StorePurpose,
    ValidatedEncryptedLeaseSet2, ValidatedLeaseSet, ValidatedLeaseSet2, ValidatedMetaLeaseSet,
    ValidatedNetDbRecord, ValidatedRouterInfo, ValidationContext, decompress_router_info,
};

const MAX_TRACKED_SOURCES: usize = 1024;
const MAX_TRACKED_KEYS: usize = 2048;

/// Floodfill server role gate. Configuration intent alone must not select `Serving`; the daemon
/// derives that state from its lifecycle/readiness authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillRole {
    Disabled,
    Serving,
}

/// Authenticated ingress classification supplied by the owning dispatcher.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum FloodfillIngress {
    DirectPeer(Hash),
    RouterTunnel,
    ClientTunnel,
}

impl fmt::Debug for FloodfillIngress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectPeer(_) => f.write_str("DirectPeer([redacted])"),
            Self::RouterTunnel => f.write_str("RouterTunnel"),
            Self::ClientTunnel => f.write_str("ClientTunnel"),
        }
    }
}

/// Caller-supplied clock values. The service never reads a clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillTime {
    pub wall_ms: u64,
    pub monotonic_ms: u64,
}

/// Hard ceilings and deterministic request windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillStorePolicy {
    pub window_ms: u64,
    pub max_global_requests: u32,
    pub max_source_requests: u32,
    pub max_key_requests: u32,
    pub max_global_bytes: usize,
    pub max_source_bytes: usize,
    pub max_compressed_bytes: usize,
    pub max_record_bytes: usize,
    pub max_crypto_validations: u32,
    pub max_provenance_age_ms: u64,
    pub lookup_reply_peer_count: usize,
    pub max_reply_bytes: usize,
    /// Local RouterHash, when known, to prevent a remote store replacing local self-state.
    pub own_router_hash: Option<Hash>,
}

impl Default for FloodfillStorePolicy {
    fn default() -> Self {
        Self {
            window_ms: 10_000,
            max_global_requests: 4096,
            max_source_requests: 64,
            max_key_requests: 32,
            max_global_bytes: 4 * 1024 * 1024,
            max_source_bytes: 256 * 1024,
            max_compressed_bytes: 64 * 1024,
            max_record_bytes: 64 * 1024,
            max_crypto_validations: 1024,
            max_provenance_age_ms: 24 * 60 * 60 * 1000,
            lookup_reply_peer_count: 3,
            max_reply_bytes: MAX_I2NP_PAYLOAD_SIZE,
            own_router_hash: None,
        }
    }
}

/// Service counters contain aggregate categories only; no peer, key, or payload identifiers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FloodfillStoreStats {
    pub accepted: u64,
    pub replaced: u64,
    pub idempotent: u64,
    pub stale: u64,
    pub conflict: u64,
    pub invalid: u64,
    pub throttled: u64,
    pub capacity: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct WindowCounter {
    start_ms: u64,
    count: u32,
    initialized: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct WindowBytes {
    start_ms: u64,
    bytes: usize,
    initialized: bool,
}

impl WindowBytes {
    fn admit(&mut self, now: u64, window: u64, amount: usize, ceiling: usize) -> bool {
        if !self.initialized || now.saturating_sub(self.start_ms) >= window {
            self.start_ms = now;
            self.bytes = 0;
            self.initialized = true;
        }
        let Some(next) = self.bytes.checked_add(amount) else {
            return false;
        };
        if next > ceiling {
            return false;
        }
        self.bytes = next;
        true
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct ThrottleSource(Hash, Option<u32>);

impl WindowCounter {
    fn admit(&mut self, now: u64, window: u64, ceiling: u32) -> bool {
        if !self.initialized || now.saturating_sub(self.start_ms) >= window {
            self.start_ms = now;
            self.count = 0;
            self.initialized = true;
        }
        if self.count >= ceiling {
            return false;
        }
        self.count += 1;
        true
    }
}

/// Explicit service outcomes. A DeliveryStatus carries the inbound route tuple unchanged; the
/// runtime assigns the outgoing I2NP envelope and dispatches it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FloodfillStoreEffect {
    Disabled,
    Throttled,
    Invalid,
    Unsupported,
    CapacityExceeded,
    Stored {
        record: RecordId,
        outcome: ServerInsertOutcome,
        acknowledgement: Option<FloodfillAck>,
        replication: Option<ReplicationCandidate>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillAck {
    pub reply_token: u32,
    pub reply_tunnel_id: Option<u32>,
    pub reply_gateway: Option<Hash>,
    pub message: DeliveryStatusMessage,
}

/// A handle into server-owned storage, not a transport object or an independently mutable record.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ReplicationCandidate {
    pub record: RecordId,
    /// Authenticated direct ingress peer, retained only for fanout exclusion.
    pub source_peer: Option<Hash>,
}

impl fmt::Debug for ReplicationCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplicationCandidate")
            .field("record", &self.record)
            .field("source_peer", &self.source_peer.map(|_| "[redacted]"))
            .finish()
    }
}

/// Typed server reply route. Tunnel bytes are body bytes for the daemon Garlic/TunnelGateway path.
#[derive(Debug, Eq, PartialEq)]
pub enum FloodfillReplyIntent {
    Direct {
        peer: Hash,
        body: I2npBody,
    },
    Tunnel {
        gateway: Hash,
        tunnel_id: u32,
        payload: Vec<u8>,
        protection: ReplyProtection,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyProtection {
    None,
    SuppliedKeyEcies,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupFailure {
    Disabled,
    Throttled,
    UnsupportedType,
    InvalidReplyRoute,
    UnsupportedEncryption,
    EncodingFailure,
}

#[derive(Debug, Eq, PartialEq)]
pub enum FloodfillLookupEffect {
    NoResponse(LookupFailure),
    Reply(FloodfillReplyIntent),
}

/// The stateful but synchronous DatabaseStore ingestion service.
pub struct FloodfillStoreService {
    policy: FloodfillStorePolicy,
    global: WindowCounter,
    lookup_global: WindowCounter,
    lookup_keys: BTreeMap<Hash, WindowCounter>,
    crypto: WindowCounter,
    global_bytes: WindowBytes,
    sources: BTreeMap<ThrottleSource, WindowCounter>,
    source_bytes: BTreeMap<ThrottleSource, WindowBytes>,
    keys: BTreeMap<RecordId, WindowCounter>,
    stats: FloodfillStoreStats,
}

impl Default for FloodfillStoreService {
    fn default() -> Self {
        Self::new(FloodfillStorePolicy::default())
    }
}

impl FloodfillStoreService {
    pub fn new(policy: FloodfillStorePolicy) -> Self {
        Self {
            policy,
            global: WindowCounter::default(),
            lookup_global: WindowCounter::default(),
            lookup_keys: BTreeMap::new(),
            crypto: WindowCounter::default(),
            global_bytes: WindowBytes::default(),
            sources: BTreeMap::new(),
            source_bytes: BTreeMap::new(),
            keys: BTreeMap::new(),
            stats: FloodfillStoreStats::default(),
        }
    }

    pub const fn stats(&self) -> FloodfillStoreStats {
        self.stats
    }

    /// Serves one bounded lookup from main-router server-authority records.
    pub fn handle_lookup(
        &mut self,
        db: &ServerNetDb,
        lookup: &DatabaseLookupMessage,
        role: FloodfillRole,
        local_router: Hash,
        time: FloodfillTime,
    ) -> FloodfillLookupEffect {
        if role != FloodfillRole::Serving {
            return FloodfillLookupEffect::NoResponse(LookupFailure::Disabled);
        }
        if lookup.excluded_peers.len() > MAX_DATABASE_LOOKUP_EXCLUDED_PEERS
            || self.policy.lookup_reply_peer_count == 0
        {
            return FloodfillLookupEffect::NoResponse(LookupFailure::UnsupportedType);
        }
        if !self.lookup_global.admit(
            time.monotonic_ms,
            self.policy.window_ms,
            self.policy.max_global_requests,
        ) {
            self.stats.throttled = self.stats.throttled.saturating_add(1);
            return FloodfillLookupEffect::NoResponse(LookupFailure::Throttled);
        }
        self.lookup_keys.retain(|_, value| {
            time.monotonic_ms.saturating_sub(value.start_ms) < self.policy.window_ms
        });
        if !self.lookup_keys.contains_key(&lookup.key) && self.lookup_keys.len() >= MAX_TRACKED_KEYS
        {
            self.stats.throttled = self.stats.throttled.saturating_add(1);
            return FloodfillLookupEffect::NoResponse(LookupFailure::Throttled);
        }
        if !self.lookup_keys.entry(lookup.key).or_default().admit(
            time.monotonic_ms,
            self.policy.window_ms,
            self.policy.max_key_requests,
        ) {
            self.stats.throttled = self.stats.throttled.saturating_add(1);
            return FloodfillLookupEffect::NoResponse(LookupFailure::Throttled);
        }
        let body = match self.lookup_body(db, lookup, local_router, time) {
            Ok(Some(body)) => body,
            Ok(None) => return FloodfillLookupEffect::NoResponse(LookupFailure::UnsupportedType),
            Err(error) => return FloodfillLookupEffect::NoResponse(error),
        };
        self.lookup_reply(lookup, body)
    }

    fn lookup_body(
        &self,
        db: &ServerNetDb,
        lookup: &DatabaseLookupMessage,
        local_router: Hash,
        time: FloodfillTime,
    ) -> Result<Option<I2npBody>, LookupFailure> {
        let types: &[u8] = match lookup.lookup_type {
            0 => &[0, 1, 3, 7],
            1 => &[1, 3, 7],
            2 => &[0],
            3 => &[],
            _ => return Ok(None),
        };
        for record_type in types {
            match db.database_store_for_answer(
                *record_type,
                lookup.key,
                time.wall_ms,
                self.policy.max_provenance_age_ms,
                self.policy.max_record_bytes,
            ) {
                Ok(Some(message)) => {
                    return Ok(Some(I2npBody::DatabaseStore(Box::new(message))));
                }
                Ok(None)
                | Err(crate::ProvenanceEligibility::NotPublished)
                | Err(crate::ProvenanceEligibility::Expired)
                | Err(crate::ProvenanceEligibility::WrongNamespace)
                | Err(crate::ProvenanceEligibility::LookupResponseOnly)
                | Err(crate::ProvenanceEligibility::ClientTunnelOnly)
                | Err(crate::ProvenanceEligibility::ReplicaCannotReflood)
                | Err(crate::ProvenanceEligibility::Allowed) => {}
                Err(crate::ProvenanceEligibility::CapacityExceeded) => {
                    return Err(LookupFailure::EncodingFailure);
                }
            }
        }

        let mut excluded: BTreeSet<Hash> = lookup.excluded_peers.iter().copied().collect();
        excluded.insert(lookup.from);
        excluded.insert(local_router);
        let target = RouterHash::from_hash(lookup.key);
        let routing_key = crate::daily_routing_key(&target, Date::from_millis(time.wall_ms))
            .map_err(|_| LookupFailure::EncodingFailure)?;
        let routing_target = RouterHash::from_hash(routing_key);
        let want_floodfill = lookup.lookup_type != 3;
        let mut candidates = db.router_info_candidates(
            time.wall_ms,
            self.policy.max_provenance_age_ms,
            want_floodfill,
            &excluded,
            8192,
        );
        candidates.sort_by_key(|candidate| {
            (
                crate::xor_distance(candidate, &routing_target),
                *candidate.as_hash(),
            )
        });
        candidates.dedup();
        candidates.truncate(
            self.policy
                .lookup_reply_peer_count
                .min(MAX_DATABASE_SEARCH_REPLY_PEERS),
        );
        Ok(Some(I2npBody::DatabaseSearchReply(
            DatabaseSearchReplyMessage {
                key: lookup.key,
                peer_hashes: candidates
                    .into_iter()
                    .map(|candidate| *candidate.as_hash())
                    .collect(),
                from: local_router,
            },
        )))
    }

    fn lookup_reply(
        &self,
        lookup: &DatabaseLookupMessage,
        body: I2npBody,
    ) -> FloodfillLookupEffect {
        if !lookup.delivery_flag {
            if !matches!(lookup.reply_encryption, ReplyEncryption::None) {
                return FloodfillLookupEffect::NoResponse(LookupFailure::UnsupportedEncryption);
            }
            if body.encode_to_vec(self.policy.max_reply_bytes).is_err() {
                return FloodfillLookupEffect::NoResponse(LookupFailure::EncodingFailure);
            }
            return FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
                peer: lookup.from,
                body,
            });
        }
        let Some(tunnel_id) = lookup.reply_tunnel_id.filter(|value| *value != 0) else {
            return FloodfillLookupEffect::NoResponse(LookupFailure::InvalidReplyRoute);
        };
        let Ok(plaintext) = body.encode_to_vec(self.policy.max_reply_bytes) else {
            return FloodfillLookupEffect::NoResponse(LookupFailure::EncodingFailure);
        };
        let (payload, protection) = match &lookup.reply_encryption {
            ReplyEncryption::Ecies {
                reply_key,
                reply_tags,
            } if reply_tags.len() == 1 => {
                match seal_netdb_ecies_reply(reply_key, &reply_tags[0], &plaintext) {
                    Ok(ciphertext)
                        if ciphertext.len().saturating_add(8) <= self.policy.max_reply_bytes =>
                    {
                        let mut wire = Vec::with_capacity(ciphertext.len() + 8);
                        wire.extend_from_slice(reply_tags[0].as_bytes());
                        wire.extend_from_slice(&ciphertext);
                        (wire, ReplyProtection::SuppliedKeyEcies)
                    }
                    Ok(_) => {
                        return FloodfillLookupEffect::NoResponse(LookupFailure::EncodingFailure);
                    }
                    Err(_) => {
                        return FloodfillLookupEffect::NoResponse(
                            LookupFailure::UnsupportedEncryption,
                        );
                    }
                }
            }
            _ => return FloodfillLookupEffect::NoResponse(LookupFailure::UnsupportedEncryption),
        };
        FloodfillLookupEffect::Reply(FloodfillReplyIntent::Tunnel {
            gateway: lookup.from,
            tunnel_id,
            payload,
            protection,
        })
    }

    /// Admits one decoded DatabaseStore. Caller has already authenticated/classified the ingress;
    /// missing or client-only authority fails closed. `message_id` is the received I2NP envelope id.
    pub fn handle(
        &mut self,
        db: &mut ServerNetDb,
        message: &DatabaseStoreMessage,
        role: FloodfillRole,
        ingress: FloodfillIngress,
        _message_id: u32,
        time: FloodfillTime,
    ) -> FloodfillStoreEffect {
        if role != FloodfillRole::Serving {
            return FloodfillStoreEffect::Disabled;
        }
        let (record_type, key) = message_key(message);
        let id = RecordId::new(record_type, key);
        if record_type == 0 && self.policy.own_router_hash == Some(key) {
            self.stats.invalid = self.stats.invalid.saturating_add(1);
            return FloodfillStoreEffect::Invalid;
        }
        if !self.admit_request(ingress, message, id, time.monotonic_ms) {
            self.stats.throttled = self.stats.throttled.saturating_add(1);
            return FloodfillStoreEffect::Throttled;
        }
        if matches!(ingress, FloodfillIngress::ClientTunnel) {
            self.stats.invalid = self.stats.invalid.saturating_add(1);
            return FloodfillStoreEffect::Invalid;
        }
        if record_type == 5 {
            return FloodfillStoreEffect::Unsupported;
        }
        if !self.crypto.admit(
            time.monotonic_ms,
            self.policy.window_ms,
            self.policy.max_crypto_validations,
        ) {
            self.stats.throttled = self.stats.throttled.saturating_add(1);
            return FloodfillStoreEffect::Throttled;
        }

        let validated = match self.validate(message, time) {
            Ok(Some(record)) => record,
            Ok(None) => return FloodfillStoreEffect::Unsupported,
            Err(()) => {
                self.stats.invalid = self.stats.invalid.saturating_add(1);
                return FloodfillStoreEffect::Invalid;
            }
        };
        let purpose = if message.reply_token == 0 {
            StorePurpose::FloodReplica
        } else {
            StorePurpose::PublishedStore
        };
        let inbound = match ingress {
            FloodfillIngress::DirectPeer(_) => InboundProvenance::AuthenticatedDirectPeer,
            FloodfillIngress::RouterTunnel => InboundProvenance::RouterTunnel,
            FloodfillIngress::ClientTunnel => unreachable!("client ingress rejected above"),
        };
        let provenance = RecordProvenance {
            namespace: NetDbNamespace::MainRouter,
            inbound,
            purpose,
            observed_at_ms: time.wall_ms,
        };
        let outcome = match db.insert(validated, provenance) {
            Ok(value) => value,
            Err(_) => {
                self.stats.invalid = self.stats.invalid.saturating_add(1);
                return FloodfillStoreEffect::Invalid;
            }
        };
        match outcome {
            ServerInsertOutcome::CapacityExceeded => {
                self.stats.capacity = self.stats.capacity.saturating_add(1);
                return FloodfillStoreEffect::CapacityExceeded;
            }
            ServerInsertOutcome::Stale | ServerInsertOutcome::Conflict => {
                if outcome == ServerInsertOutcome::Stale {
                    self.stats.stale = self.stats.stale.saturating_add(1);
                } else {
                    self.stats.conflict = self.stats.conflict.saturating_add(1);
                }
                return FloodfillStoreEffect::Stored {
                    record: id,
                    outcome,
                    acknowledgement: None,
                    replication: None,
                };
            }
            ServerInsertOutcome::Inserted => {
                self.stats.accepted = self.stats.accepted.saturating_add(1)
            }
            ServerInsertOutcome::Replaced => {
                self.stats.replaced = self.stats.replaced.saturating_add(1)
            }
            ServerInsertOutcome::Idempotent => {
                self.stats.idempotent = self.stats.idempotent.saturating_add(1)
            }
        }
        let publisher_direct =
            matches!(ingress, FloodfillIngress::DirectPeer(_)) && message.reply_token != 0;
        let acknowledgement = (message.reply_token != 0).then_some(FloodfillAck {
            reply_token: message.reply_token,
            reply_tunnel_id: message.reply_tunnel_id,
            reply_gateway: message.reply_gateway,
            // I2NP DeliveryStatus acknowledges the DatabaseStore reply token, not
            // the enclosing DatabaseStore message id.
            message: DeliveryStatusMessage::new(
                message.reply_token,
                Date::from_millis(time.wall_ms),
            ),
        });
        let replication = (publisher_direct
            && matches!(
                outcome,
                ServerInsertOutcome::Inserted | ServerInsertOutcome::Replaced
            ))
        .then_some(
            db.may_replicate(&id, time.wall_ms, self.policy.max_provenance_age_ms)
                == crate::ProvenanceEligibility::Allowed,
        )
        .filter(|eligible| *eligible)
        .map(|_| ReplicationCandidate {
            record: id,
            source_peer: match ingress {
                FloodfillIngress::DirectPeer(peer) => Some(peer),
                FloodfillIngress::RouterTunnel | FloodfillIngress::ClientTunnel => None,
            },
        });
        FloodfillStoreEffect::Stored {
            record: id,
            outcome,
            acknowledgement,
            replication,
        }
    }

    fn admit_request(
        &mut self,
        ingress: FloodfillIngress,
        message: &DatabaseStoreMessage,
        key: RecordId,
        now: u64,
    ) -> bool {
        self.sources
            .retain(|_, value| now.saturating_sub(value.start_ms) < self.policy.window_ms);
        self.source_bytes
            .retain(|_, value| now.saturating_sub(value.start_ms) < self.policy.window_ms);
        self.keys
            .retain(|_, value| now.saturating_sub(value.start_ms) < self.policy.window_ms);
        if !self
            .global
            .admit(now, self.policy.window_ms, self.policy.max_global_requests)
        {
            return false;
        }
        let bytes = match encoded_size(&message.data, self.policy.max_record_bytes) {
            Some(value) => value,
            None => return false,
        };
        if !self.global_bytes.admit(
            now,
            self.policy.window_ms,
            bytes,
            self.policy.max_global_bytes,
        ) {
            return false;
        }
        let source = match ingress {
            FloodfillIngress::DirectPeer(value) => Some(ThrottleSource(value, None)),
            FloodfillIngress::RouterTunnel | FloodfillIngress::ClientTunnel => message
                .reply_gateway
                .map(|gateway| ThrottleSource(gateway, message.reply_tunnel_id)),
        };
        if let Some(source) = source {
            if !self.sources.contains_key(&source) && self.sources.len() >= MAX_TRACKED_SOURCES {
                return false;
            }
            if !self.sources.entry(source).or_default().admit(
                now,
                self.policy.window_ms,
                self.policy.max_source_requests,
            ) {
                return false;
            }
            if !self.source_bytes.entry(source).or_default().admit(
                now,
                self.policy.window_ms,
                bytes,
                self.policy.max_source_bytes,
            ) {
                return false;
            }
        }
        if !self.keys.contains_key(&key) && self.keys.len() >= MAX_TRACKED_KEYS {
            return false;
        }
        self.keys.entry(key).or_default().admit(
            now,
            self.policy.window_ms,
            self.policy.max_key_requests,
        )
    }

    fn validate(
        &self,
        message: &DatabaseStoreMessage,
        time: FloodfillTime,
    ) -> Result<Option<ValidatedNetDbRecord>, ()> {
        let key = message.key;
        let record = match &message.data {
            DatabaseStoreData::RouterInfoCompressed(payload) => {
                if payload.as_bytes().len() > self.policy.max_compressed_bytes {
                    return Err(());
                }
                let bytes = decompress_router_info(payload.as_bytes()).map_err(|_| ())?;
                if bytes.len() > self.policy.max_record_bytes {
                    return Err(());
                }
                let value =
                    RouterInfo::decode(&bytes, self.policy.max_record_bytes).map_err(|_| ())?;
                let router_key = RouterHash::from_hash(key);
                let value = ValidatedRouterInfo::from_router_info(
                    value,
                    Some(router_key),
                    ValidationContext::new(Date::from_millis(time.wall_ms)),
                )
                .map_err(|_| ())?;
                ValidatedNetDbRecord::RouterInfo(value)
            }
            DatabaseStoreData::LeaseSet(value) => {
                if value.encode_to_vec(self.policy.max_record_bytes).is_err() {
                    return Err(());
                }
                let value = (**value).clone();
                let value = ValidatedLeaseSet::validate(
                    value,
                    Some(DestinationHash::from_hash(key)),
                    LeaseSetValidationContext::new(time.wall_ms),
                )
                .map_err(|_| ())?;
                ValidatedNetDbRecord::LeaseSet(value)
            }
            DatabaseStoreData::LeaseSet2(value) => {
                if value.encode_to_vec(self.policy.max_record_bytes).is_err() {
                    return Err(());
                }
                let value = (**value).clone();
                let now = u32::try_from(time.wall_ms / 1000).map_err(|_| ())?;
                let value = ValidatedLeaseSet2::from_lease_set2(
                    value,
                    Some(DestinationHash::from_hash(key)),
                    LeaseSet2ValidationContext::new(now),
                )
                .map_err(|_| ())?;
                ValidatedNetDbRecord::LeaseSet2(value)
            }
            DatabaseStoreData::MetaLeaseSet(value) => {
                if value.encode_to_vec(self.policy.max_record_bytes).is_err() {
                    return Err(());
                }
                let value = (**value).clone();
                let value = ValidatedMetaLeaseSet::validate(
                    value,
                    Some(DestinationHash::from_hash(key)),
                    LeaseSetValidationContext::new(time.wall_ms),
                )
                .map_err(|_| ())?;
                ValidatedNetDbRecord::MetaLeaseSet(value)
            }
            // A type-5 record is validated and then stored opaquely. The
            // floodfill checks the Red25519 signature over the blinded key and
            // the freshness window, and never derives the subcredential: it
            // cannot, because it never learns the unblinded public key.
            DatabaseStoreData::EncryptedLeaseSet(value) => {
                if value.encode_to_vec(self.policy.max_record_bytes).is_err() {
                    return Err(());
                }
                let value = (**value).clone();
                let now = u32::try_from(time.wall_ms / 1000).map_err(|_| ())?;
                let value = ValidatedEncryptedLeaseSet2::validate(
                    value,
                    Some(BlindedStorageKey::from_hash(key)),
                    Els2ValidationContext::new(now),
                )
                .map_err(|_| ())?;
                ValidatedNetDbRecord::EncryptedLeaseSet2(value)
            }
            DatabaseStoreData::Deferred { .. } => return Ok(None),
        };
        Ok(Some(record))
    }
}

fn encoded_size(data: &DatabaseStoreData, maximum: usize) -> Option<usize> {
    match data {
        DatabaseStoreData::RouterInfoCompressed(value) => Some(value.as_bytes().len()),
        DatabaseStoreData::LeaseSet(value) => value.encode_to_vec(maximum).ok().map(|v| v.len()),
        DatabaseStoreData::LeaseSet2(value) => value.encode_to_vec(maximum).ok().map(|v| v.len()),
        DatabaseStoreData::MetaLeaseSet(value) => {
            value.encode_to_vec(maximum).ok().map(|v| v.len())
        }
        DatabaseStoreData::EncryptedLeaseSet(value) => {
            value.encode_to_vec(maximum).ok().map(|v| v.len())
        }
        DatabaseStoreData::Deferred { payload, .. } => Some(payload.as_bytes().len()),
    }
}

fn message_key(message: &DatabaseStoreMessage) -> (u8, Hash) {
    let record_type = match &message.data {
        DatabaseStoreData::RouterInfoCompressed(_) => 0,
        DatabaseStoreData::LeaseSet(_) => 1,
        DatabaseStoreData::LeaseSet2(_) => 3,
        DatabaseStoreData::MetaLeaseSet(_) => 7,
        DatabaseStoreData::EncryptedLeaseSet(_) => 5,
        DatabaseStoreData::Deferred { .. } => 5,
    };
    (record_type, message.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{
        CryptoKeyType, Date32, DeferredPayload, Destination, Lease, Lease2, LeaseSet, LeaseSet2,
        LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, MAX_COMMON_STRUCTURE_SIZE,
        Mapping, MetaLease, MetaLeaseSet, PublicKey, ReplySecret, SignatureValue,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn router_store(seed: u64, reply_token: u32) -> (DatabaseStoreMessage, Hash) {
        router_store_with_caps(seed, reply_token, None)
    }

    fn router_store_with_caps(
        seed: u64,
        reply_token: u32,
        caps: Option<&str>,
    ) -> (DatabaseStoreMessage, Hash) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let identity = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let options = caps.map_or_else(Mapping::empty, |value| {
            let mut builder = Mapping::builder();
            builder
                .insert("caps".to_owned(), value.to_owned())
                .expect("caps option");
            builder.build().expect("mapping")
        });
        let router_info = identity
            .sign_router_info(Date::from_millis(1), Vec::new(), Vec::new(), options)
            .expect("signed router info");
        let key = crate::router_hash(router_info.router_identity())
            .expect("hash")
            .as_hash()
            .to_owned();
        let bytes = router_info
            .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        use std::io::Write as _;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&bytes).expect("compress");
        let compressed = encoder.finish().expect("finish gzip");
        (
            DatabaseStoreMessage {
                key,
                reply_token,
                reply_tunnel_id: None,
                reply_gateway: None,
                data: DatabaseStoreData::RouterInfoCompressed(
                    DeferredPayload::new(compressed, 64 * 1024).expect("bounded payload"),
                ),
            },
            Hash::from_bytes([seed as u8; 32]),
        )
    }

    fn lease_family_stores(seed: u64) -> Vec<DatabaseStoreMessage> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let signer = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let destination =
            Destination::new(signer.identity().key_and_cert().clone()).expect("destination");
        let destination_hash = destination.hash().unwrap();
        let signing_type = i2pr_crypto::ROUTER_SIGNING_KEY_TYPE;
        let encryption = PublicKey::new(CryptoKeyType::ElGamal, vec![0x55; 256]).unwrap();
        let lease = Lease::new(
            Hash::from_bytes([0x22; 32]),
            7,
            Date::from_millis(2_000_000),
        );
        let unsigned = LeaseSet::new(
            destination.clone(),
            encryption.clone(),
            destination.signing_key().clone(),
            vec![lease.clone()],
            SignatureValue::new(signing_type, vec![0; 64]).unwrap(),
        )
        .unwrap();
        let signature = signer.signing_key().sign(unsigned.signed_bytes()).unwrap();
        let classic = LeaseSet::new(
            destination.clone(),
            encryption,
            destination.signing_key().clone(),
            vec![lease],
            signature,
        )
        .unwrap();
        let classic = DatabaseStoreMessage {
            key: destination_hash,
            reply_token: 9,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::LeaseSet(Box::new(classic)),
        };

        let published = 1000;
        let header = LeaseSet2Header::new(
            destination.clone(),
            published,
            3600,
            LeaseSet2Flags::from_raw(0),
        )
        .unwrap();
        let options = Mapping::empty();
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).unwrap()];
        let leases = vec![Lease2::new(
            Hash::from_bytes([0x33; 32]),
            7,
            Date32::from_seconds(published + 600),
        )];
        let unsigned = LeaseSet2::new(
            header.clone(),
            options.clone(),
            encryption_keys.clone(),
            leases.clone(),
            SignatureValue::new(signing_type, vec![0; 64]).unwrap(),
        )
        .unwrap();
        let signature = signer
            .signing_key()
            .sign(&unsigned.signature_preimage())
            .unwrap();
        let standard = LeaseSet2::new(header, options, encryption_keys, leases, signature).unwrap();
        let standard = DatabaseStoreMessage {
            key: destination_hash,
            reply_token: 10,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::LeaseSet2(Box::new(standard)),
        };

        let header =
            LeaseSet2Header::new(destination, published, 3600, LeaseSet2Flags::from_raw(0))
                .unwrap();
        let entries = vec![
            MetaLease::new(
                Hash::from_bytes([0x44; 32]),
                3,
                1,
                Date32::from_seconds(published + 600),
            )
            .unwrap(),
        ];
        let unsigned = MetaLeaseSet::new(
            header.clone(),
            Mapping::empty(),
            entries.clone(),
            Vec::new(),
            SignatureValue::new(signing_type, vec![0; 64]).unwrap(),
        )
        .unwrap();
        let signature = signer
            .signing_key()
            .sign(&unsigned.signature_preimage())
            .unwrap();
        let meta =
            MetaLeaseSet::new(header, Mapping::empty(), entries, Vec::new(), signature).unwrap();
        let meta = DatabaseStoreMessage {
            key: destination_hash,
            reply_token: 11,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::MetaLeaseSet(Box::new(meta)),
        };
        vec![classic, standard, meta]
    }

    #[test]
    fn window_counters_reset_without_wall_clock_or_sleep() {
        let mut counter = WindowCounter::default();
        assert!(counter.admit(10, 20, 1));
        assert!(!counter.admit(29, 20, 1));
        assert!(counter.admit(30, 20, 1));
    }

    #[test]
    fn diagnostics_have_no_peer_or_record_identifiers() {
        let stats = FloodfillStoreStats {
            throttled: 1,
            ..FloodfillStoreStats::default()
        };
        assert!(!format!("{stats:?}").contains("RouterHash"));
    }

    #[test]
    fn direct_publisher_store_is_validated_acknowledged_and_offered_for_replication() {
        let (message, peer) = router_store(0x273, 7);
        let mut service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        let effect = service.handle(
            &mut db,
            &message,
            FloodfillRole::Serving,
            FloodfillIngress::DirectPeer(peer),
            0x1234,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        let FloodfillStoreEffect::Stored {
            outcome,
            acknowledgement,
            replication,
            ..
        } = effect
        else {
            panic!("valid publisher store must reach validated storage")
        };
        assert_eq!(outcome, ServerInsertOutcome::Inserted);
        let acknowledgement = acknowledgement.expect("nonzero token requests an ack");
        assert_eq!(acknowledgement.reply_token, 7);
        assert_eq!(
            acknowledgement.message.message_id,
            acknowledgement.reply_token
        );
        assert!(replication.is_some());

        let effect = service.handle(
            &mut db,
            &message,
            FloodfillRole::Serving,
            FloodfillIngress::DirectPeer(peer),
            0x1235,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 2,
            },
        );
        let FloodfillStoreEffect::Stored {
            outcome,
            replication,
            ..
        } = effect
        else {
            panic!("duplicate store remains an idempotent admission")
        };
        assert_eq!(outcome, ServerInsertOutcome::Idempotent);
        assert!(replication.is_none());
    }

    #[test]
    fn zero_token_replica_is_stored_without_ack_or_reflood() {
        let (message, peer) = router_store(0x274, 0);
        let mut service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        let effect = service.handle(
            &mut db,
            &message,
            FloodfillRole::Serving,
            FloodfillIngress::DirectPeer(peer),
            42,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        let FloodfillStoreEffect::Stored {
            outcome,
            acknowledgement,
            replication,
            ..
        } = effect
        else {
            panic!("valid zero-token replica must be stored")
        };
        assert_eq!(outcome, ServerInsertOutcome::Inserted);
        assert!(acknowledgement.is_none());
        assert!(replication.is_none());
    }

    #[test]
    fn disabled_role_and_self_key_collision_do_not_mutate_storage() {
        let (message, peer) = router_store(0x275, 4);
        let mut service = FloodfillStoreService::new(FloodfillStorePolicy {
            own_router_hash: Some(message.key),
            ..FloodfillStorePolicy::default()
        });
        let mut db = ServerNetDb::default();
        assert_eq!(
            service.handle(
                &mut db,
                &message,
                FloodfillRole::Disabled,
                FloodfillIngress::DirectPeer(peer),
                1,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 1
                },
            ),
            FloodfillStoreEffect::Disabled
        );
        assert_eq!(
            service.handle(
                &mut db,
                &message,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                1,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 2
                },
            ),
            FloodfillStoreEffect::Invalid
        );
        assert_eq!(db.record_count(), 0);
    }

    #[test]
    fn source_and_key_windows_throttle_then_reset_on_supplied_time() {
        let (message, peer) = router_store(0x276, 7);
        let mut service = FloodfillStoreService::new(FloodfillStorePolicy {
            max_source_requests: 1,
            ..FloodfillStorePolicy::default()
        });
        let mut db = ServerNetDb::default();
        let first = service.handle(
            &mut db,
            &message,
            FloodfillRole::Serving,
            FloodfillIngress::DirectPeer(peer),
            1,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 5,
            },
        );
        assert!(matches!(first, FloodfillStoreEffect::Stored { .. }));
        assert_eq!(
            service.handle(
                &mut db,
                &message,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                2,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 6
                },
            ),
            FloodfillStoreEffect::Throttled
        );
        let reset = service.handle(
            &mut db,
            &message,
            FloodfillRole::Serving,
            FloodfillIngress::DirectPeer(peer),
            3,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 10_005,
            },
        );
        assert!(matches!(reset, FloodfillStoreEffect::Stored { .. }));
    }

    #[test]
    fn byte_and_crypto_windows_are_independent_hard_ceilings() {
        let (message, peer) = router_store(0x279, 3);
        let mut byte_limited = FloodfillStoreService::new(FloodfillStorePolicy {
            max_global_bytes: 1,
            ..FloodfillStorePolicy::default()
        });
        let mut db = ServerNetDb::default();
        assert_eq!(
            byte_limited.handle(
                &mut db,
                &message,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                1,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 1
                },
            ),
            FloodfillStoreEffect::Throttled
        );

        let mut crypto_limited = FloodfillStoreService::new(FloodfillStorePolicy {
            max_crypto_validations: 0,
            ..FloodfillStorePolicy::default()
        });
        assert_eq!(
            crypto_limited.handle(
                &mut db,
                &message,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                1,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 1
                },
            ),
            FloodfillStoreEffect::Throttled
        );
        assert_eq!(db.record_count(), 0);
    }

    #[test]
    fn router_tunnel_ack_route_is_copied_but_never_replication_authority() {
        let (mut message, _) = router_store(0x277, 11);
        let gateway = Hash::from_bytes([0x77; 32]);
        message.reply_tunnel_id = Some(0x12345678);
        message.reply_gateway = Some(gateway);
        let mut service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        let effect = service.handle(
            &mut db,
            &message,
            FloodfillRole::Serving,
            FloodfillIngress::RouterTunnel,
            0x4455,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        let FloodfillStoreEffect::Stored {
            acknowledgement,
            replication,
            ..
        } = effect
        else {
            panic!("valid router-tunnel store must be accepted")
        };
        let ack = acknowledgement.expect("nonzero token routed acknowledgment");
        assert_eq!(ack.reply_token, 11);
        assert_eq!(ack.message.message_id, ack.reply_token);
        assert_eq!(ack.reply_tunnel_id, Some(0x12345678));
        assert_eq!(ack.reply_gateway, Some(gateway));
        assert!(replication.is_none());
    }

    fn lookup(key: Hash, kind: u8) -> DatabaseLookupMessage {
        DatabaseLookupMessage {
            key,
            from: Hash::from_bytes([0xa1; 32]),
            delivery_flag: false,
            reply_tunnel_id: None,
            lookup_type: kind,
            excluded_peers: Vec::new(),
            reply_encryption: ReplyEncryption::None,
        }
    }

    #[test]
    fn lookup_hits_misses_and_exploration_follow_adr_policy() {
        let (store, peer) = router_store(0x280, 3);
        let key = store.key;
        let mut service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        assert!(matches!(
            service.handle(
                &mut db,
                &store,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                1,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 1
                }
            ),
            FloodfillStoreEffect::Stored { .. }
        ));
        let local = Hash::from_bytes([0x99; 32]);
        let hit = service.handle_lookup(
            &db,
            &lookup(key, 2),
            FloodfillRole::Serving,
            local,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 2,
            },
        );
        let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
            body: I2npBody::DatabaseStore(message),
            ..
        }) = hit
        else {
            panic!("RouterInfo hit should return DatabaseStore")
        };
        let DatabaseStoreData::RouterInfoCompressed(compressed) = &message.data else {
            panic!("RouterInfo DatabaseStore must be compressed")
        };
        assert_eq!(
            &compressed.as_bytes()[..10],
            &[0x1f, 0x8b, 0x08, 0, 0, 0, 0, 0, 2, 0xff]
        );
        let miss = service.handle_lookup(
            &db,
            &lookup(key, 1),
            FloodfillRole::Serving,
            local,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 3,
            },
        );
        assert!(matches!(
            miss,
            FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
                body: I2npBody::DatabaseSearchReply(_),
                ..
            })
        ));
        let exploration = service.handle_lookup(
            &db,
            &lookup(key, 3),
            FloodfillRole::Serving,
            local,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 4,
            },
        );
        let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
            body: I2npBody::DatabaseSearchReply(reply),
            ..
        }) = exploration
        else {
            panic!("exploration exact hit must return a search reply")
        };
        assert_eq!(reply.key, key);
        assert_eq!(reply.from, local);
        assert!(reply.peer_hashes.contains(&key));
    }

    #[test]
    fn requested_tunnel_reply_uses_ecies_and_rejects_plaintext_downgrade() {
        let key = Hash::from_bytes([0x55; 32]);
        let mut request = lookup(key, 1);
        request.delivery_flag = true;
        request.reply_tunnel_id = Some(0x1234);
        let reply_key = ReplySecret::from_bytes([0x33; 32]);
        let reply_tag = ReplySecret::from_bytes([0x44; 8]);
        request.reply_encryption = ReplyEncryption::Ecies {
            reply_key: reply_key.clone(),
            reply_tags: vec![reply_tag.clone()],
        };
        let mut service = FloodfillStoreService::default();
        let db = ServerNetDb::default();
        let result = service.handle_lookup(
            &db,
            &request,
            FloodfillRole::Serving,
            Hash::from_bytes([0x66; 32]),
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Tunnel {
            payload,
            protection,
            ..
        }) = result
        else {
            panic!("ECIES tunnel reply expected")
        };
        assert_eq!(protection, ReplyProtection::SuppliedKeyEcies);
        assert_eq!(&payload[..8], reply_tag.as_bytes());
        let plaintext = i2pr_crypto::open_netdb_ecies_reply(&reply_key, &reply_tag, &payload[8..])
            .expect("open reply");
        assert_eq!(plaintext.len(), 65);
        assert_eq!(plaintext[32], 0);
        request.reply_encryption = ReplyEncryption::None;
        assert_eq!(
            service.handle_lookup(
                &db,
                &request,
                FloodfillRole::Serving,
                Hash::from_bytes([0x66; 32]),
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 2
                },
            ),
            FloodfillLookupEffect::NoResponse(LookupFailure::UnsupportedEncryption)
        );
        request.reply_encryption = ReplyEncryption::Ecies {
            reply_key: reply_key.clone(),
            reply_tags: vec![reply_tag.clone(), reply_tag],
        };
        assert_eq!(
            service.handle_lookup(
                &db,
                &request,
                FloodfillRole::Serving,
                Hash::from_bytes([0x66; 32]),
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 3
                },
            ),
            FloodfillLookupEffect::NoResponse(LookupFailure::UnsupportedEncryption)
        );
    }

    #[test]
    fn dsrm_candidates_are_excluded_type_filtered_and_hard_limited() {
        let mut store_service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        let mut all_keys = Vec::new();
        for index in 0..21 {
            let caps = if index == 20 { "R" } else { "f" };
            let (message, authenticated_peer) =
                router_store_with_caps(0x300 + index, 1, Some(caps));
            all_keys.push(message.key);
            assert!(matches!(
                store_service.handle(
                    &mut db,
                    &message,
                    FloodfillRole::Serving,
                    FloodfillIngress::DirectPeer(authenticated_peer),
                    index as u32,
                    FloodfillTime {
                        wall_ms: 1,
                        monotonic_ms: index + 1
                    },
                ),
                FloodfillStoreEffect::Stored { .. }
            ));
        }
        let target = Hash::from_bytes([0xe0; 32]);
        let mut request = lookup(target, 2);
        request.excluded_peers.push(all_keys[0]);
        request.from = all_keys[1];
        let local = all_keys[2];
        let mut lookup_service = FloodfillStoreService::new(FloodfillStorePolicy {
            lookup_reply_peer_count: 64,
            ..FloodfillStorePolicy::default()
        });
        let result = lookup_service.handle_lookup(
            &db,
            &request,
            FloodfillRole::Serving,
            local,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
            body: I2npBody::DatabaseSearchReply(reply),
            ..
        }) = result
        else {
            panic!("miss should return DSRM")
        };
        assert_eq!(reply.peer_hashes.len(), MAX_DATABASE_SEARCH_REPLY_PEERS);
        assert!(!reply.peer_hashes.contains(&all_keys[0]));
        assert!(!reply.peer_hashes.contains(&all_keys[1]));
        assert!(!reply.peer_hashes.contains(&all_keys[2]));
        assert!(reply.peer_hashes.iter().all(|value| *value != all_keys[20]));

        let mut default_service = FloodfillStoreService::default();
        let default_reply = default_service.handle_lookup(
            &db,
            &request,
            FloodfillRole::Serving,
            local,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
            body: I2npBody::DatabaseSearchReply(default_reply),
            ..
        }) = default_reply
        else {
            panic!("regular miss must return default DSRM")
        };
        assert_eq!(default_reply.peer_hashes.len(), 3);

        request.lookup_type = 3;
        request.from = Hash::from_bytes([0xab; 32]);
        request.excluded_peers.clear();
        let exploration = lookup_service.handle_lookup(
            &db,
            &request,
            FloodfillRole::Serving,
            Hash::from_bytes([0xcd; 32]),
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 2,
            },
        );
        let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
            body: I2npBody::DatabaseSearchReply(reply),
            ..
        }) = exploration
        else {
            panic!("exploration returns DSRM")
        };
        assert_eq!(reply.peer_hashes, vec![all_keys[20]]);
    }

    #[test]
    fn repeated_lookup_target_is_throttled_by_the_store_service_policy() {
        let mut service = FloodfillStoreService::new(FloodfillStorePolicy {
            max_key_requests: 1,
            ..FloodfillStorePolicy::default()
        });
        let db = ServerNetDb::default();
        let request = lookup(Hash::from_bytes([0x76; 32]), 2);
        let local = Hash::from_bytes([0x77; 32]);
        let first = service.handle_lookup(
            &db,
            &request,
            FloodfillRole::Serving,
            local,
            FloodfillTime {
                wall_ms: 1,
                monotonic_ms: 1,
            },
        );
        assert!(matches!(first, FloodfillLookupEffect::Reply(_)));
        assert_eq!(
            service.handle_lookup(
                &db,
                &request,
                FloodfillRole::Serving,
                local,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 2
                },
            ),
            FloodfillLookupEffect::NoResponse(LookupFailure::Throttled)
        );
    }

    #[test]
    fn hidden_routerinfo_is_neither_returned_as_a_hit_nor_a_search_candidate() {
        let (message, peer) = router_store_with_caps(0x311, 1, Some("H"));
        let key = message.key;
        let mut service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        assert!(matches!(
            service.handle(
                &mut db,
                &message,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                1,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 1
                },
            ),
            FloodfillStoreEffect::Stored { .. }
        ));
        for kind in [2, 3] {
            let result = service.handle_lookup(
                &db,
                &lookup(key, kind),
                FloodfillRole::Serving,
                Hash::from_bytes([0x91; 32]),
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: kind as u64,
                },
            );
            let FloodfillLookupEffect::Reply(FloodfillReplyIntent::Direct {
                body: I2npBody::DatabaseSearchReply(reply),
                ..
            }) = result
            else {
                panic!("hidden record and candidate must both be suppressed")
            };
            assert!(!reply.peer_hashes.contains(&key));
        }
    }

    #[test]
    fn all_plan_272_lease_record_types_use_the_single_validating_store_path() {
        let peer = Hash::from_bytes([0x28; 32]);
        let mut service = FloodfillStoreService::default();
        let mut db = ServerNetDb::default();
        for (index, message) in lease_family_stores(0x278).iter().enumerate() {
            let effect = service.handle(
                &mut db,
                message,
                FloodfillRole::Serving,
                FloodfillIngress::DirectPeer(peer),
                100 + index as u32,
                FloodfillTime {
                    wall_ms: 1_000_000,
                    monotonic_ms: index as u64 + 1,
                },
            );
            let FloodfillStoreEffect::Stored {
                outcome,
                acknowledgement,
                replication,
                ..
            } = effect
            else {
                panic!("validated type-1/3/7 store should be admitted: {effect:?}");
            };
            assert_eq!(outcome, ServerInsertOutcome::Inserted);
            assert!(acknowledgement.is_some());
            assert!(replication.is_some());
        }
        assert_eq!(db.record_count(), 3);
    }
}
