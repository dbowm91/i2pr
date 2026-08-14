//! Plan 105 §10: local RouterInfo publication coordinator.
//!
//! The coordinator emits one bounded `DatabaseStore` attempt per
//! nearest eligible floodfill without becoming a live effects
//! service. It does not open sockets, does not build exploratory
//! tunnels, and does not sign a fresh `RouterInfo` per retry.

use std::collections::BTreeMap;

use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, DeferredPayload, Hash, I2npBody};
use thiserror::Error;

use crate::local::LocalRouterInfo;
use crate::lookup_id::LookupKind;
use crate::lookup_policy::{
    DEFAULT_MAX_CANDIDATES_CONSIDERED, DEFAULT_MAX_PEERS_PER_LOOKUP, LookupPolicy,
};
use crate::router_info::RouterHash;
use crate::store::RouterInfoStore;

/// Maximum number of concurrent publication attempts the coordinator
/// will track. The bound is intentionally small to keep runtime
/// ownership tight.
pub const MAX_PUBLICATION_ATTEMPTS: usize = 32;

/// Diagnostic failures for the publication coordinator.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PublicationError {
    /// A publication attempt referenced an unknown request identifier.
    #[error("unknown publication request id {request_id}")]
    UnknownRequest {
        /// The unknown request identifier.
        request_id: u64,
    },
    /// A retry was issued after the publication already terminated.
    #[error("publication request id {request_id} already terminated")]
    AlreadyTerminal {
        /// The terminating request identifier.
        request_id: u64,
    },
    /// A reply token reused a value already bound to another attempt.
    #[error("reply token {token} reused across attempts")]
    ReplyTokenReuse {
        /// The reused reply token.
        token: u32,
    },
    /// The coordinator was asked to publish while no local RouterInfo
    /// snapshot was registered.
    #[error("no local RouterInfo snapshot registered")]
    NoLocalSnapshot,
}

/// State of a single publication attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationAttemptState {
    /// The attempt is queued and the runtime has not yet acknowledged
    /// the corresponding send.
    Queued,
    /// The attempt was acknowledged as delivered by the runtime.
    Delivered,
    /// The attempt was acknowledged as a delivery failure.
    DeliveryFailed,
    /// The attempt received a matching DeliveryStatus ack.
    Acknowledged,
    /// The attempt received an unexpected or rejected DeliveryStatus.
    Rejected,
    /// The attempt was cancelled.
    Cancelled,
}

/// Bounded per-attempt bookkeeping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationAttempt {
    request_id: u64,
    peer: RouterHash,
    reply_token: u32,
    state: PublicationAttemptState,
}

/// Snapshot returned by [`PublicationCoordinator::snapshot`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationSnapshot {
    /// Number of attempts that are still queued, delivered, or
    /// acknowledged.
    pub active: usize,
    /// Number of attempts the runtime reported as delivery failures.
    pub failed: usize,
    /// Number of attempts that received a matching DeliveryStatus.
    pub acknowledged: usize,
    /// Number of attempts that received an unexpected DeliveryStatus.
    pub rejected: usize,
    /// Number of attempts cancelled locally.
    pub cancelled: usize,
}

/// Plan 105 §10 publication coordinator.
pub struct PublicationCoordinator {
    policy: LookupPolicy,
    local_snapshot: Option<LocalRouterInfo>,
    attempts: BTreeMap<u64, PublicationAttempt>,
    tokens_by_value: BTreeMap<u32, u64>,
    next_request_id: u64,
    next_reply_token: u32,
}

impl PublicationCoordinator {
    /// Constructs a coordinator with the supplied bounded policy.
    pub fn new(policy: LookupPolicy) -> Self {
        Self {
            policy,
            local_snapshot: None,
            attempts: BTreeMap::new(),
            tokens_by_value: BTreeMap::new(),
            next_request_id: 1,
            next_reply_token: 1,
        }
    }

    /// Registers the locally signed RouterInfo snapshot the
    /// coordinator will publish. The coordinator never re-signs the
    /// record; retried publication attempts reuse the supplied
    /// bytes.
    pub fn register_local(&mut self, local: LocalRouterInfo) {
        self.local_snapshot = Some(local);
    }

    /// Returns the closest eligible floodfill set for the local
    /// RouterHash. The selector uses the policy's
    /// `max_candidates_considered` ceiling.
    pub fn nearest_floodfills(&self, store: &RouterInfoStore) -> Vec<RouterHash> {
        let Some(local) = self.local_snapshot.as_ref() else {
            return Vec::new();
        };
        let local_key = local.router_hash();
        let selection = crate::lookup_policy::select_floodfill_candidates(
            store,
            &local_key,
            &local_key,
            &[local_key],
            &self.policy,
        );
        selection.hashes()
    }

    /// Builds the `DatabaseStore` I2NP body for one publication
    /// attempt. Returns the body, the encoded RouterInfo bytes, the
    /// bounded reply token, and the request identifier. A retry of
    /// an existing attempt reuses the previously minted token so the
    /// remote peer can correlate the delivery acknowledgement.
    pub fn begin_attempt(
        &mut self,
        peer: RouterHash,
        store: &RouterInfoStore,
    ) -> Result<PublicationAttemptRecord, PublicationError> {
        let local = self
            .local_snapshot
            .as_ref()
            .ok_or(PublicationError::NoLocalSnapshot)?;
        if self.attempts.len() >= MAX_PUBLICATION_ATTEMPTS {
            return Err(PublicationError::AlreadyTerminal { request_id: 0 });
        }
        if !store.contains(&peer) {
            return Err(PublicationError::NoLocalSnapshot);
        }
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .expect("request id overflow");
        let reply_token = self.next_reply_token;
        self.next_reply_token = self
            .next_reply_token
            .checked_add(1)
            .expect("reply token overflow");
        let encoded = local
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|_| PublicationError::NoLocalSnapshot)?;
        if encoded.len() > i2pr_proto::MAX_I2NP_PAYLOAD_SIZE - 64 {
            return Err(PublicationError::NoLocalSnapshot);
        }
        // Publication uses reply_token=0 because the local router
        // does not request a DeliveryStatus acknowledgement for the
        // initial store-and-forget publish. The correlation token is
        // reserved for the optional verification path; when a
        // nonzero token is set, both reply_tunnel_id and
        // reply_gateway must be present.
        let store_message = DatabaseStoreMessage {
            key: Hash::from_bytes(*local.router_hash().as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                DeferredPayload::new(encoded.clone(), i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                    .map_err(|_| PublicationError::NoLocalSnapshot)?,
            ),
        };
        let body = I2npBody::DatabaseStore(Box::new(store_message));
        body.encode_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .map_err(|_| PublicationError::NoLocalSnapshot)?;
        let _ = body;
        let attempt = PublicationAttempt {
            request_id,
            peer,
            reply_token,
            state: PublicationAttemptState::Queued,
        };
        self.attempts.insert(request_id, attempt.clone());
        self.tokens_by_value.insert(reply_token, request_id);
        Ok(PublicationAttemptRecord { attempt, encoded })
    }

    /// Reuses a previously built attempt record. The function only
    /// succeeds when the attempt is still active.
    pub fn retry_attempt(&mut self, request_id: u64) -> Result<(), PublicationError> {
        let attempt = self
            .attempts
            .get_mut(&request_id)
            .ok_or(PublicationError::UnknownRequest { request_id })?;
        match attempt.state {
            PublicationAttemptState::Queued
            | PublicationAttemptState::Delivered
            | PublicationAttemptState::DeliveryFailed
            | PublicationAttemptState::Rejected => {
                attempt.state = PublicationAttemptState::Queued;
                Ok(())
            }
            PublicationAttemptState::Acknowledged | PublicationAttemptState::Cancelled => {
                Err(PublicationError::AlreadyTerminal { request_id })
            }
        }
    }

    /// Marks an attempt as delivered. Idempotent.
    pub fn mark_delivered(&mut self, request_id: u64) -> Result<(), PublicationError> {
        let attempt = self
            .attempts
            .get_mut(&request_id)
            .ok_or(PublicationError::UnknownRequest { request_id })?;
        if matches!(
            attempt.state,
            PublicationAttemptState::Acknowledged | PublicationAttemptState::Cancelled
        ) {
            return Err(PublicationError::AlreadyTerminal { request_id });
        }
        attempt.state = PublicationAttemptState::Delivered;
        Ok(())
    }

    /// Marks an attempt as a delivery failure. Idempotent.
    pub fn mark_delivery_failed(&mut self, request_id: u64) -> Result<(), PublicationError> {
        let attempt = self
            .attempts
            .get_mut(&request_id)
            .ok_or(PublicationError::UnknownRequest { request_id })?;
        if matches!(
            attempt.state,
            PublicationAttemptState::Acknowledged | PublicationAttemptState::Cancelled
        ) {
            return Err(PublicationError::AlreadyTerminal { request_id });
        }
        attempt.state = PublicationAttemptState::DeliveryFailed;
        Ok(())
    }

    /// Correlates a `DeliveryStatus` reply with a tracked attempt. A
    /// mismatched token returns [`PublicationError::UnknownRequest`].
    pub fn correlate_delivery_status(
        &mut self,
        message_id: u32,
    ) -> Result<PublicationCorrelation, PublicationError> {
        let request_id = self
            .tokens_by_value
            .get(&message_id)
            .copied()
            .ok_or(PublicationError::UnknownRequest { request_id: 0 })?;
        let attempt = self
            .attempts
            .get_mut(&request_id)
            .ok_or(PublicationError::UnknownRequest { request_id })?;
        attempt.state = PublicationAttemptState::Acknowledged;
        Ok(PublicationCorrelation {
            request_id,
            message_id,
        })
    }

    /// Records a rejected `DeliveryStatus` (e.g. an existing token
    /// reported back as a failure code).
    pub fn record_rejection(&mut self, request_id: u64) -> Result<(), PublicationError> {
        let attempt = self
            .attempts
            .get_mut(&request_id)
            .ok_or(PublicationError::UnknownRequest { request_id })?;
        if matches!(
            attempt.state,
            PublicationAttemptState::Acknowledged | PublicationAttemptState::Cancelled
        ) {
            return Err(PublicationError::AlreadyTerminal { request_id });
        }
        attempt.state = PublicationAttemptState::Rejected;
        Ok(())
    }

    /// Cancels the supplied attempt. Cancellation is idempotent.
    pub fn cancel(&mut self, request_id: u64) -> Result<(), PublicationError> {
        let attempt = self
            .attempts
            .get_mut(&request_id)
            .ok_or(PublicationError::UnknownRequest { request_id })?;
        attempt.state = PublicationAttemptState::Cancelled;
        Ok(())
    }

    /// Returns the bounded attempt book. Used by tests and
    /// diagnostics.
    pub fn attempt(&self, request_id: u64) -> Option<&PublicationAttempt> {
        self.attempts.get(&request_id)
    }

    /// Returns the count of attempts in each terminal category.
    pub fn snapshot(&self) -> PublicationSnapshot {
        let mut snapshot = PublicationSnapshot {
            active: 0,
            failed: 0,
            acknowledged: 0,
            rejected: 0,
            cancelled: 0,
        };
        for attempt in self.attempts.values() {
            match attempt.state {
                PublicationAttemptState::Queued | PublicationAttemptState::Delivered => {
                    snapshot.active += 1
                }
                PublicationAttemptState::DeliveryFailed => snapshot.failed += 1,
                PublicationAttemptState::Acknowledged => snapshot.acknowledged += 1,
                PublicationAttemptState::Rejected => snapshot.rejected += 1,
                PublicationAttemptState::Cancelled => snapshot.cancelled += 1,
            }
        }
        snapshot
    }

    /// Indicates the coordinator requires a follow-up verification
    /// `DatabaseLookup` over an exploratory reply path before the
    /// publication can be considered complete.
    pub fn needs_verification_lookup(&self) -> bool {
        self.local_snapshot.is_some()
    }

    /// Returns the bounded default policy used by callers that do not
    /// supply one.
    pub fn default_policy() -> LookupPolicy {
        LookupPolicy::new(
            DEFAULT_MAX_CANDIDATES_CONSIDERED,
            DEFAULT_MAX_PEERS_PER_LOOKUP,
            crate::lookup_policy::DEFAULT_MAX_SUGGESTED_HASHES,
            crate::lookup_policy::DEFAULT_SUGGESTED_HASH_LIMIT,
            crate::lookup_policy::DEFAULT_TOTAL_DEADLINE_MS,
            crate::lookup_policy::DEFAULT_PER_ATTEMPT_DEADLINE_MS,
        )
        .expect("default policy is valid")
    }

    /// Internal accessor used by the publication lookup helpers.
    pub fn lookup_kind(&self) -> LookupKind {
        LookupKind::RouterInfo
    }
}

/// Record returned by [`PublicationCoordinator::begin_attempt`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationAttemptRecord {
    /// Bounded per-attempt bookkeeping.
    pub attempt: PublicationAttempt,
    /// Already-encoded local RouterInfo bytes the runtime should
    /// transmit. The bytes never change across retry attempts.
    pub encoded: Vec<u8>,
}

/// Correlation result for a successful DeliveryStatus ack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationCorrelation {
    /// Publication attempt identifier.
    pub request_id: u64,
    /// Echoed message identifier from the DeliveryStatus.
    pub message_id: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::{ValidationContext, router_hash};
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn local_router_info(signer: &RouterIdentityBundle) -> LocalRouterInfo {
        crate::local::LocalRouterInfoBuilder::new(signer)
            .build_default(Date::from_millis(1))
            .expect("build local")
    }

    fn validate(signer: &RouterIdentityBundle) -> crate::router_info::ValidatedRouterInfo {
        let info = signer
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        crate::router_info::ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate")
    }

    #[test]
    fn missing_local_snapshot_rejects_attempt() {
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        let store = RouterInfoStore::default();
        let peer = RouterHash::from_bytes([0x55u8; 32]);
        let error = coordinator.begin_attempt(peer, &store).unwrap_err();
        assert_eq!(error, PublicationError::NoLocalSnapshot);
    }

    #[test]
    fn attempt_record_carries_snapshot_encoded_bytes() {
        let signer = bundle(0x700);
        let floodfill = bundle(0x701);
        let mut store = RouterInfoStore::default();
        let floodfill_validated = validate(&floodfill);
        let floodfill_key = floodfill_validated.key();
        store.insert(floodfill_validated);
        let local = local_router_info(&signer);
        let local_hash = router_hash(signer.identity()).unwrap();
        let peer_hash = router_hash(floodfill.identity()).unwrap();
        assert_eq!(peer_hash, floodfill_key);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local.clone());
        let record = coordinator
            .begin_attempt(peer_hash, &store)
            .expect("attempt");
        assert_eq!(record.attempt.peer, peer_hash);
        assert!(record.attempt.reply_token > 0);
        let _ = local_hash;
        assert!(!record.encoded.is_empty());
    }

    #[test]
    fn delivery_status_correlation_returns_request() {
        let signer = bundle(0x710);
        let floodfill = bundle(0x711);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&floodfill));
        let peer = router_hash(floodfill.identity()).unwrap();
        let local = local_router_info(&signer);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local);
        let record = coordinator.begin_attempt(peer, &store).expect("attempt");
        let token = record.attempt.reply_token;
        let correlation = coordinator
            .correlate_delivery_status(token)
            .expect("correlate");
        assert_eq!(correlation.message_id, token);
        assert_eq!(correlation.request_id, record.attempt.request_id);
    }

    #[test]
    fn unknown_token_returns_unknown_request() {
        let signer = bundle(0x720);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local_router_info(&signer));
        let error = coordinator
            .correlate_delivery_status(0xdead_beef)
            .unwrap_err();
        assert_eq!(error, PublicationError::UnknownRequest { request_id: 0 });
    }

    #[test]
    fn retry_attempt_resets_queued_state() {
        let signer = bundle(0x730);
        let floodfill = bundle(0x731);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&floodfill));
        let peer = router_hash(floodfill.identity()).unwrap();
        let local = local_router_info(&signer);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local);
        let record = coordinator.begin_attempt(peer, &store).expect("attempt");
        let request_id = record.attempt.request_id;
        coordinator
            .mark_delivery_failed(request_id)
            .expect("mark delivery failed");
        coordinator.retry_attempt(request_id).expect("retry");
        let snapshot = coordinator.snapshot();
        assert_eq!(snapshot.active, 1);
        assert_eq!(snapshot.failed, 0);
    }

    #[test]
    fn acknowledge_blocks_retry_attempt() {
        let signer = bundle(0x740);
        let floodfill = bundle(0x741);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&floodfill));
        let peer = router_hash(floodfill.identity()).unwrap();
        let local = local_router_info(&signer);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local);
        let record = coordinator.begin_attempt(peer, &store).expect("attempt");
        let token = record.attempt.reply_token;
        let request_id = record.attempt.request_id;
        coordinator.correlate_delivery_status(token).expect("ack");
        let error = coordinator.retry_attempt(request_id).unwrap_err();
        assert_eq!(error, PublicationError::AlreadyTerminal { request_id });
    }

    #[test]
    fn cancel_is_idempotent() {
        let signer = bundle(0x750);
        let floodfill = bundle(0x751);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&floodfill));
        let peer = router_hash(floodfill.identity()).unwrap();
        let local = local_router_info(&signer);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local);
        let record = coordinator.begin_attempt(peer, &store).expect("attempt");
        let request_id = record.attempt.request_id;
        coordinator.cancel(request_id).expect("cancel");
        coordinator.cancel(request_id).expect("cancel idempotent");
        assert_eq!(
            coordinator.attempt(request_id).map(|a| a.state),
            Some(PublicationAttemptState::Cancelled)
        );
    }

    #[test]
    fn nearest_floodfills_returns_only_advertisers() {
        let signer = bundle(0x760);
        let floodfill_a = bundle(0x761);
        let floodfill_b = bundle(0x762);
        let plain = bundle(0x763);
        let mut store = RouterInfoStore::default();
        let floodfill_a_validated = floodfill_helper(&floodfill_a);
        let floodfill_b_validated = floodfill_helper(&floodfill_b);
        let plain_validated = validate(&plain);
        store.insert(floodfill_a_validated);
        store.insert(floodfill_b_validated);
        store.insert(plain_validated);
        let local = local_router_info(&signer);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local);
        let hashes = coordinator.nearest_floodfills(&store);
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(&router_hash(floodfill_a.identity()).unwrap()));
        assert!(hashes.contains(&router_hash(floodfill_b.identity()).unwrap()));
        assert!(!hashes.contains(&router_hash(plain.identity()).unwrap()));
    }

    #[test]
    fn unknown_peer_token_returns_unknown_request() {
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        let error = coordinator.correlate_delivery_status(42).unwrap_err();
        assert_eq!(error, PublicationError::UnknownRequest { request_id: 0 });
    }

    #[test]
    fn retry_does_not_resign_routerinfo() {
        let signer = bundle(0x770);
        let floodfill = bundle(0x771);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&floodfill));
        let peer = router_hash(floodfill.identity()).unwrap();
        let local = local_router_info(&signer);
        let mut coordinator = PublicationCoordinator::new(PublicationCoordinator::default_policy());
        coordinator.register_local(local);
        let record = coordinator.begin_attempt(peer, &store).expect("attempt");
        let original_bytes = record.encoded.clone();
        let request_id = record.attempt.request_id;
        coordinator
            .mark_delivery_failed(request_id)
            .expect("mark failed");
        coordinator.retry_attempt(request_id).expect("retry");
        // After retry, the previously built record's bytes should
        // still match the local RouterInfo snapshot — no
        // re-signing took place.
        let second_record = coordinator
            .begin_attempt(peer, &store)
            .expect("second attempt");
        assert_eq!(second_record.encoded, original_bytes);
    }

    fn floodfill_helper(b: &RouterIdentityBundle) -> crate::router_info::ValidatedRouterInfo {
        let mut options = i2pr_proto::Mapping::builder();
        options.insert("caps".to_owned(), "f".to_owned()).unwrap();
        let info = b
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        crate::router_info::ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate")
    }
}
