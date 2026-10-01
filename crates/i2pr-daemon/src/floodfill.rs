//! Controlled floodfill role composition. Normal daemon configuration never constructs a permit.

use std::collections::VecDeque;

use i2pr_netdb::{
    DirectFloodAction, FloodfillAck, FloodfillEligibilitySnapshot, FloodfillIngress,
    FloodfillLookupEffect, FloodfillPeerView, FloodfillResourceBudget, FloodfillResourcePolicy,
    FloodfillRoleController, FloodfillRoleEffect, FloodfillRoleState, FloodfillStoreEffect,
    FloodfillStorePolicy, FloodfillStoreService, FloodfillTime, ReplicationPlanner,
    ReplicationPolicy, ServerNetDb, ServerNetDbConfig,
};
use i2pr_proto::{DatabaseLookupMessage, DatabaseStoreMessage, Hash};
use i2pr_transport::{LinkId, PeerId};

#[derive(Debug)]
pub enum FloodfillDaemonEffect {
    StoreAck {
        peer: PeerId,
        link: LinkId,
        ack: FloodfillAck,
    },
    LookupReply {
        requester: PeerId,
        inbound_link: LinkId,
        intent: i2pr_netdb::FloodfillReplyIntent,
    },
    DirectFlood {
        action: DirectFloodAction,
    },
}

pub enum FloodfillDispatchOutcome {
    Ignored,
    Store(FloodfillStoreEffect),
    Lookup(Result<(), i2pr_netdb::LookupFailure>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillCoordinatorPolicy {
    pub max_queued_effects: usize,
    pub max_queued_bytes: usize,
    pub max_peer_candidates: usize,
    pub max_peer_scan_work: usize,
    pub max_record_age_ms: u64,
}

impl Default for FloodfillCoordinatorPolicy {
    fn default() -> Self {
        Self {
            max_queued_effects: 256,
            max_queued_bytes: 4 * 1024 * 1024,
            max_peer_candidates: 256,
            max_peer_scan_work: 4096,
            max_record_age_ms: 60 * 60 * 1000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FloodfillCoordinatorStats {
    pub accepted_stores: u64,
    pub rejected_inactive: u64,
    pub queue_full: u64,
    pub queued_effects: usize,
    pub queued_bytes: usize,
}

/// Owns floodfill policy and bounded effects. A runtime owner drains effects over authenticated
/// SSU2/direct or explicit tunnel reply routes; replication has no tunnel route in its type.
pub struct FloodfillCoordinator {
    role: FloodfillRoleController,
    store_service: FloodfillStoreService,
    netdb: ServerNetDb,
    planner: ReplicationPlanner,
    resources: FloodfillResourceBudget,
    policy: FloodfillCoordinatorPolicy,
    local_router: Hash,
    effects: VecDeque<(
        FloodfillDaemonEffect,
        usize,
        i2pr_netdb::ResourceLease,
        i2pr_netdb::ResourceLease,
    )>,
    queued_bytes: usize,
    stats: FloodfillCoordinatorStats,
    maintenance_cursor: Option<i2pr_netdb::RecordId>,
}

impl FloodfillCoordinator {
    pub fn new(
        local_router: Hash,
        policy: FloodfillCoordinatorPolicy,
        store_policy: FloodfillStorePolicy,
        netdb_config: ServerNetDbConfig,
        resource_policy: FloodfillResourcePolicy,
        replication_policy: ReplicationPolicy,
    ) -> Result<Self, i2pr_netdb::ReplicationError> {
        if policy.max_queued_effects == 0
            || policy.max_queued_bytes == 0
            || policy.max_peer_candidates == 0
            || policy.max_peer_scan_work < policy.max_peer_candidates
        {
            return Err(i2pr_netdb::ReplicationError::InvalidPolicy);
        }
        Ok(Self {
            role: FloodfillRoleController::new(),
            store_service: FloodfillStoreService::new(store_policy),
            netdb: ServerNetDb::with_config(netdb_config),
            planner: ReplicationPlanner::new(replication_policy)?,
            resources: FloodfillResourceBudget::new(resource_policy),
            policy,
            local_router,
            effects: VecDeque::new(),
            queued_bytes: 0,
            stats: FloodfillCoordinatorStats::default(),
            maintenance_cursor: None,
        })
    }

    pub fn update_eligibility(
        &mut self,
        snapshot: FloodfillEligibilitySnapshot,
    ) -> FloodfillRoleEffect {
        let effect = self.role.update(snapshot);
        // Draining disables admission immediately. The runtime owner may pop remaining bounded
        // effects until its deadline; dropping this coordinator releases all queue leases.
        effect
    }
    pub fn begin_activation(&mut self) -> bool {
        self.role.begin_activation()
    }
    pub fn complete_activation(&mut self) -> bool {
        self.role.complete_activation().is_some()
    }
    pub fn role_state(&self) -> FloodfillRoleState {
        self.role.state()
    }
    pub fn advertisement_permit(&self) -> Option<i2pr_netdb::FloodfillAdvertisementPermit> {
        self.role.advertisement_permit()
    }
    pub fn stats(&self) -> FloodfillCoordinatorStats {
        FloodfillCoordinatorStats {
            queued_effects: self.effects.len(),
            queued_bytes: self.queued_bytes,
            ..self.stats
        }
    }
    pub fn netdb(&self) -> &ServerNetDb {
        &self.netdb
    }
    pub fn netdb_mut(&mut self) -> &mut ServerNetDb {
        &mut self.netdb
    }
    pub fn resources(&self) -> &FloodfillResourceBudget {
        &self.resources
    }

    pub fn maintenance_tick(
        &mut self,
        now_ms: u64,
        max_age_ms: u64,
        batch_size: usize,
    ) -> i2pr_netdb::MaintenanceBatch {
        let batch =
            self.netdb
                .maintenance_batch(self.maintenance_cursor, batch_size, now_ms, max_age_ms);
        self.maintenance_cursor = batch.next;
        batch
    }

    pub fn handle_store(
        &mut self,
        peer: PeerId,
        link: LinkId,
        message_id: u32,
        message: &DatabaseStoreMessage,
        time: FloodfillTime,
    ) -> FloodfillStoreEffect {
        if !self.role.accepts_new_work() {
            self.stats.rejected_inactive = self.stats.rejected_inactive.saturating_add(1);
            return FloodfillStoreEffect::Disabled;
        }
        let Some(_request) = self
            .resources
            .reserve(i2pr_netdb::ResourceKind::ActiveRequest)
        else {
            self.stats.rejected_inactive = self.stats.rejected_inactive.saturating_add(1);
            return FloodfillStoreEffect::CapacityExceeded;
        };
        let effect = self.store_service.handle(
            &mut self.netdb,
            message,
            i2pr_netdb::FloodfillRole::Serving,
            FloodfillIngress::DirectPeer(peer.hash()),
            message_id,
            time,
        );
        if let FloodfillStoreEffect::Stored {
            acknowledgement: Some(ack),
            replication,
            ..
        } = &effect
        {
            self.enqueue(
                FloodfillDaemonEffect::StoreAck {
                    peer,
                    link,
                    ack: *ack,
                },
                64,
            );
            if let Some(candidate) = replication {
                let mut excluded = std::collections::BTreeSet::new();
                excluded.insert(peer.hash());
                let peers = self
                    .netdb
                    .router_info_candidates(
                        time.wall_ms,
                        self.policy.max_record_age_ms,
                        true,
                        &excluded,
                        self.policy.max_peer_scan_work,
                    )
                    .into_iter()
                    .take(self.policy.max_peer_candidates)
                    .map(|peer| FloodfillPeerView {
                        peer,
                        floodfill: true,
                        hidden: false,
                        verified: true,
                    })
                    .collect::<Vec<_>>();
                let plan =
                    self.planner
                        .plan(&self.netdb, *candidate, &peers, self.local_router, time);
                for action in plan.actions {
                    self.enqueue(FloodfillDaemonEffect::DirectFlood { action }, 64 * 1024);
                }
            }
            self.stats.accepted_stores = self.stats.accepted_stores.saturating_add(1);
        }
        effect
    }

    /// Consumes an authenticated SSU2 I2NP handoff only while the controlled role is Active.
    /// Ordinary client NetDB routing remains with its existing dispatcher when this returns
    /// `Ignored`.
    pub fn handle_authenticated_i2np(
        &mut self,
        inbound: &i2pr_runtime::Ssu2InboundI2np,
        time: FloodfillTime,
    ) -> Result<FloodfillDispatchOutcome, crate::router_i2np::RouterI2npError> {
        if !self.role.accepts_new_work() {
            return Ok(FloodfillDispatchOutcome::Ignored);
        }
        let (outcome, body) =
            crate::router_i2np::dispatch_router_i2np_with_floodfill_body(inbound, time.wall_ms)?;
        let message_id = match outcome {
            crate::router_i2np::RouterI2npOutcome::RouterControl { message_id, .. } => message_id,
            _ => return Ok(FloodfillDispatchOutcome::Ignored),
        };
        match body {
            Some(crate::router_i2np::FloodfillControlBody::DatabaseStore(message)) => {
                Ok(FloodfillDispatchOutcome::Store(self.handle_store(
                    inbound.peer,
                    inbound.link_id,
                    message_id,
                    &message,
                    time,
                )))
            }
            Some(crate::router_i2np::FloodfillControlBody::DatabaseLookup(lookup)) => {
                Ok(FloodfillDispatchOutcome::Lookup(self.handle_lookup(
                    inbound.peer,
                    inbound.link_id,
                    &lookup,
                    time,
                )))
            }
            None => Ok(FloodfillDispatchOutcome::Ignored),
        }
    }

    pub fn handle_lookup(
        &mut self,
        peer: PeerId,
        link: LinkId,
        lookup: &DatabaseLookupMessage,
        time: FloodfillTime,
    ) -> Result<(), i2pr_netdb::LookupFailure> {
        if lookup.from != peer.hash() {
            return Err(i2pr_netdb::LookupFailure::InvalidReplyRoute);
        }
        if !self.role.accepts_new_work() {
            return Err(i2pr_netdb::LookupFailure::Disabled);
        }
        let Some(_request) = self
            .resources
            .reserve(i2pr_netdb::ResourceKind::ActiveRequest)
        else {
            return Err(i2pr_netdb::LookupFailure::Throttled);
        };
        let effect = self.store_service.handle_lookup(
            &self.netdb,
            lookup,
            i2pr_netdb::FloodfillRole::Serving,
            self.local_router,
            time,
        );
        match effect {
            FloodfillLookupEffect::Reply(intent) => {
                if self.enqueue(
                    FloodfillDaemonEffect::LookupReply {
                        requester: peer,
                        inbound_link: link,
                        intent,
                    },
                    64 * 1024,
                ) {
                    Ok(())
                } else {
                    Err(i2pr_netdb::LookupFailure::Throttled)
                }
            }
            FloodfillLookupEffect::NoResponse(reason) => Err(reason),
        }
    }

    fn enqueue(&mut self, effect: FloodfillDaemonEffect, bytes: usize) -> bool {
        let Some(next_bytes) = self.queued_bytes.checked_add(bytes) else {
            self.stats.queue_full = self.stats.queue_full.saturating_add(1);
            return false;
        };
        if self.effects.len() >= self.policy.max_queued_effects
            || next_bytes > self.policy.max_queued_bytes
        {
            self.stats.queue_full = self.stats.queue_full.saturating_add(1);
            return false;
        }
        let Some(slot) = self
            .resources
            .reserve(i2pr_netdb::ResourceKind::QueuedEffect)
        else {
            self.stats.queue_full = self.stats.queue_full.saturating_add(1);
            return false;
        };
        let Some(byte_lease) = self
            .resources
            .reserve(i2pr_netdb::ResourceKind::QueuedBytes(bytes))
        else {
            self.stats.queue_full = self.stats.queue_full.saturating_add(1);
            return false;
        };
        self.queued_bytes = next_bytes;
        self.effects.push_back((effect, bytes, slot, byte_lease));
        true
    }

    pub fn pop_effect(&mut self) -> Option<FloodfillDaemonEffect> {
        let (effect, bytes, _slot, _byte_lease) = self.effects.pop_front()?;
        self.queued_bytes = self.queued_bytes.saturating_sub(bytes);
        Some(effect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{DatabaseStoreData, DatabaseStoreType, DeferredPayload};

    fn coordinator() -> FloodfillCoordinator {
        FloodfillCoordinator::new(
            Hash::from_bytes([9; 32]),
            FloodfillCoordinatorPolicy {
                max_queued_effects: 1,
                max_queued_bytes: 128,
                max_peer_candidates: 2,
                max_peer_scan_work: 2,
                max_record_age_ms: 60_000,
            },
            FloodfillStorePolicy::default(),
            ServerNetDbConfig::default(),
            FloodfillResourcePolicy {
                queued_effects: 1,
                queued_bytes: 128,
                ..FloodfillResourcePolicy::default()
            },
            ReplicationPolicy::default(),
        )
        .expect("bounded coordinator")
    }

    fn action() -> DirectFloodAction {
        DirectFloodAction {
            peer: Hash::from_bytes([2; 32]),
            message: DatabaseStoreMessage {
                key: Hash::from_bytes([1; 32]),
                reply_token: 0,
                reply_tunnel_id: None,
                reply_gateway: None,
                data: DatabaseStoreData::Deferred {
                    store_type: DatabaseStoreType::RouterInfo,
                    payload: DeferredPayload::new(vec![1], 1).unwrap(),
                },
            },
            deadline_ms: 1,
            routing_key_class: i2pr_netdb::RoutingKeyClass::Current,
        }
    }

    #[test]
    fn normal_default_stays_off_and_queue_backpressure_releases_budgets() {
        let mut coordinator = coordinator();
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);
        assert!(coordinator.advertisement_permit().is_none());
        assert!(matches!(
            coordinator.handle_store(
                PeerId::from_bytes([1; 32]),
                LinkId::new(1).unwrap(),
                1,
                &action().message,
                FloodfillTime {
                    wall_ms: 1,
                    monotonic_ms: 1
                }
            ),
            FloodfillStoreEffect::Disabled
        ));
        assert!(coordinator.enqueue(FloodfillDaemonEffect::DirectFlood { action: action() }, 128));
        assert!(!coordinator.enqueue(FloodfillDaemonEffect::DirectFlood { action: action() }, 1));
        assert_eq!(coordinator.resources().snapshot().queued_effects, 1);
        assert!(coordinator.pop_effect().is_some());
        assert_eq!(coordinator.resources().snapshot().queued_effects, 0);
        assert_eq!(coordinator.resources().snapshot().queued_bytes, 0);
    }
}
