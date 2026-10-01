//! Controlled floodfill role composition. Normal daemon configuration never constructs a permit.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use i2pr_netdb::{
    DirectFloodAction, FloodfillAck, FloodfillEligibilitySnapshot, FloodfillIngress,
    FloodfillLookupEffect, FloodfillPeerView, FloodfillResourceBudget, FloodfillResourcePolicy,
    FloodfillRoleController, FloodfillRoleEffect, FloodfillRoleState, FloodfillStoreEffect,
    FloodfillStorePolicy, FloodfillStoreService, FloodfillTime, ReplicationPlanner,
    ReplicationPolicy, ServerNetDb, ServerNetDbConfig,
};
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseStoreMessage, Date, DeferredPayload, Hash, I2npBody,
    I2npMessage, OpaqueMessageBody, TunnelGatewayMessage,
};
use i2pr_transport::{LinkId, PeerId};

#[derive(Debug)]
pub enum FloodfillDaemonEffect {
    StoreAck {
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

/// One queued effect whose coordinator capacity leases remain held until the effect is fully
/// accepted or fails. Dropping it on cancellation/teardown releases both leases.
pub struct FloodfillLeasedEffect {
    pub effect: Option<FloodfillDaemonEffect>,
    _slot: i2pr_netdb::ResourceLease,
    _bytes: i2pr_netdb::ResourceLease,
}

/// Typed bounded outcome from draining one floodfill effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillDeliveryOutcome {
    Delivered(crate::router_i2np::RouterDeliveryOutcome),
    InvalidRoute,
    InvalidEffect,
    EncodingFailure,
    DirectDialFailed,
    EffectExpired,
}

static NEXT_FLOODFILL_MESSAGE_ID: AtomicU32 = AtomicU32::new(0xF120_0000);
const FLOODFILL_REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FLOODFILL_EFFECT_BYTES: usize = crate::router_i2np::MAX_ROUTER_I2NP_BYTES;

/// Encodes one coordinator effect and submits it through the existing bounded SSU2 delivery
/// service. The peer selected by an I2NP reply route is independent of the authenticated source.
/// No tunnel fallback is possible for direct replication.
pub fn deliver_floodfill_effect(
    delivery: &crate::router_i2np::RouterDeliveryService,
    mut leased: FloodfillLeasedEffect,
    now_ms: u64,
    cancellation: &i2pr_runtime::CancellationToken,
) -> FloodfillDeliveryOutcome {
    let (peer, bytes) = match encode_floodfill_effect(&mut leased, now_ms) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    let request = match crate::router_i2np::RouterDeliveryRequest::new(
        peer,
        bytes,
        FLOODFILL_REPLY_TIMEOUT,
    ) {
        Ok(request) => request,
        Err(_) => return FloodfillDeliveryOutcome::EncodingFailure,
    };
    FloodfillDeliveryOutcome::Delivered(delivery.deliver(request, cancellation))
}

/// Drains one effect with the daemon's bounded direct-delivery policy. It first reuses an active
/// session; on a direct miss it resolves only the requested peer through validated main-router
/// NetDB state and performs one supervised SSU2 dial under the runtime's admission/deadline
/// limits. The effect leases remain held across the await and are released on every return path.
pub async fn deliver_floodfill_effect_with_dial(
    service: &crate::router_i2np::Ssu2DaemonHandle,
    netdb: &ServerNetDb,
    mut leased: FloodfillLeasedEffect,
    now_ms: u64,
    max_record_age_ms: u64,
    cancellation: &i2pr_runtime::CancellationToken,
) -> FloodfillDeliveryOutcome {
    let (peer, bytes) = match encode_floodfill_effect(&mut leased, now_ms) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    let make_request = || {
        crate::router_i2np::RouterDeliveryRequest::new(peer, bytes.clone(), FLOODFILL_REPLY_TIMEOUT)
    };
    let first = match make_request() {
        Ok(request) => service.delivery().deliver(request, cancellation),
        Err(_) => return FloodfillDeliveryOutcome::EncodingFailure,
    };
    if first != crate::router_i2np::RouterDeliveryOutcome::NoActiveSession {
        return FloodfillDeliveryOutcome::Delivered(first);
    }
    let target = match validated_dial_target(netdb, peer, now_ms, max_record_age_ms) {
        Some(target) => target,
        None => return FloodfillDeliveryOutcome::InvalidRoute,
    };
    if service
        .dial(target, FLOODFILL_REPLY_TIMEOUT, cancellation)
        .await
        .is_err()
    {
        return FloodfillDeliveryOutcome::DirectDialFailed;
    }
    let request = match make_request() {
        Ok(request) => request,
        Err(_) => return FloodfillDeliveryOutcome::EncodingFailure,
    };
    FloodfillDeliveryOutcome::Delivered(service.delivery().deliver(request, cancellation))
}

fn validated_dial_target(
    netdb: &ServerNetDb,
    peer: PeerId,
    now_ms: u64,
    max_record_age_ms: u64,
) -> Option<i2pr_runtime::Ssu2DialTarget> {
    let info = netdb
        .router_info_for_answer(
            &i2pr_netdb::RouterHash::from_hash(peer.hash()),
            now_ms,
            max_record_age_ms,
        )
        .ok()??
        .router_info();
    for address in info.addresses() {
        if address.transport_style() != "SSU2" {
            continue;
        }
        let Ok(parsed) = i2pr_runtime::Ssu2RouterAddress::parse(address) else {
            continue;
        };
        let Some(endpoint) = parsed.endpoint() else {
            continue;
        };
        let Some(intro) = parsed.intro_key() else {
            continue;
        };
        let static_public =
            i2pr_runtime::Ssu2PublicKey::new(*parsed.static_public_key().as_bytes()).ok()?;
        let intro = i2pr_runtime::IntroKey::new(*intro.as_bytes());
        return crate::router_i2np::daemon_dial_target(
            peer.hash(),
            std::net::SocketAddr::new(endpoint.ip(), endpoint.port()),
            static_public,
            intro,
        )
        .ok();
    }
    None
}

fn encode_floodfill_effect(
    leased: &mut FloodfillLeasedEffect,
    now_ms: u64,
) -> Result<(PeerId, Vec<u8>), FloodfillDeliveryOutcome> {
    let Some(effect) = leased.effect.take() else {
        return Err(FloodfillDeliveryOutcome::InvalidEffect);
    };
    let (peer, bytes) = match effect {
        FloodfillDaemonEffect::StoreAck { ack } => {
            if ack.reply_token == 0 || ack.message.message_id != ack.reply_token {
                return Err(FloodfillDeliveryOutcome::InvalidEffect);
            }
            let Some(gateway) = ack.reply_gateway else {
                return Err(FloodfillDeliveryOutcome::InvalidRoute);
            };
            let body = I2npBody::DeliveryStatus(ack.message);
            let Some(message) = wrap_reply_body(body, ack.reply_tunnel_id, now_ms) else {
                return Err(FloodfillDeliveryOutcome::EncodingFailure);
            };
            (i2pr_transport::PeerId::from_hash(gateway), message)
        }
        FloodfillDaemonEffect::LookupReply { intent, .. } => match intent {
            i2pr_netdb::FloodfillReplyIntent::Direct { peer, body } => {
                let Some(message) = encode_standard(body, now_ms) else {
                    return Err(FloodfillDeliveryOutcome::EncodingFailure);
                };
                (i2pr_transport::PeerId::from_hash(peer), message)
            }
            i2pr_netdb::FloodfillReplyIntent::Tunnel {
                gateway,
                tunnel_id,
                payload,
                protection,
            } => {
                if tunnel_id == 0 || protection != i2pr_netdb::ReplyProtection::SuppliedKeyEcies {
                    return Err(FloodfillDeliveryOutcome::InvalidRoute);
                }
                let Ok(payload) = DeferredPayload::new(payload, MAX_FLOODFILL_EFFECT_BYTES) else {
                    return Err(FloodfillDeliveryOutcome::EncodingFailure);
                };
                let garlic = I2npBody::Garlic(OpaqueMessageBody { payload });
                let Some(garlic) = encode_standard(garlic, now_ms) else {
                    return Err(FloodfillDeliveryOutcome::EncodingFailure);
                };
                let Ok(inner) = I2npMessage::decode_standard(&garlic, MAX_FLOODFILL_EFFECT_BYTES)
                else {
                    return Err(FloodfillDeliveryOutcome::EncodingFailure);
                };
                let body = I2npBody::TunnelGateway(Box::new(TunnelGatewayMessage {
                    tunnel_id,
                    message: Box::new(inner),
                }));
                let Some(message) = encode_standard(body, now_ms) else {
                    return Err(FloodfillDeliveryOutcome::EncodingFailure);
                };
                (i2pr_transport::PeerId::from_hash(gateway), message)
            }
        },
        FloodfillDaemonEffect::DirectFlood { mut action } => {
            if now_ms > action.deadline_ms {
                return Err(FloodfillDeliveryOutcome::EffectExpired);
            }
            match action.message.data {
                i2pr_proto::DatabaseStoreData::Deferred {
                    store_type: i2pr_proto::DatabaseStoreType::RouterInfo,
                    ref payload,
                } => {
                    action.message.data =
                        i2pr_proto::DatabaseStoreData::RouterInfoCompressed(payload.clone());
                }
                i2pr_proto::DatabaseStoreData::Deferred {
                    store_type: i2pr_proto::DatabaseStoreType::EncryptedLeaseSet,
                    ..
                } => return Err(FloodfillDeliveryOutcome::InvalidEffect),
                _ => {}
            }
            if action.message.reply_token != 0
                || action.message.reply_gateway.is_some()
                || action.message.reply_tunnel_id.is_some()
            {
                return Err(FloodfillDeliveryOutcome::InvalidEffect);
            }
            let Some(message) =
                encode_standard(I2npBody::DatabaseStore(Box::new(action.message)), now_ms)
            else {
                return Err(FloodfillDeliveryOutcome::EncodingFailure);
            };
            (i2pr_transport::PeerId::from_hash(action.peer), message)
        }
    };
    Ok((peer, bytes))
}

fn wrap_reply_body(body: I2npBody, reply_tunnel_id: Option<u32>, now_ms: u64) -> Option<Vec<u8>> {
    match reply_tunnel_id {
        None | Some(0) => encode_standard(body, now_ms),
        Some(tunnel_id) => {
            let nested = encode_standard(body, now_ms)?;
            let message = I2npMessage::decode_standard(&nested, MAX_FLOODFILL_EFFECT_BYTES).ok()?;
            encode_standard(
                I2npBody::TunnelGateway(Box::new(TunnelGatewayMessage {
                    tunnel_id,
                    message: Box::new(message),
                })),
                now_ms,
            )
        }
    }
}

fn encode_standard(body: I2npBody, now_ms: u64) -> Option<Vec<u8>> {
    let id = NEXT_FLOODFILL_MESSAGE_ID.fetch_add(1, Ordering::Relaxed);
    let expiration = now_ms.checked_add(30_000)?;
    I2npMessage::new_standard(id, Date::from_millis(expiration), body)
        .ok()?
        .encode_standard_to_vec(MAX_FLOODFILL_EFFECT_BYTES)
        .ok()
}

pub enum FloodfillDispatchOutcome {
    Ignored(crate::router_i2np::RouterI2npOutcome),
    Store(FloodfillStoreEffect),
    Lookup(Result<(), i2pr_netdb::LookupFailure>),
}

/// Exit reason for the single supervised floodfill owner future.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillOwnerExit {
    RequestedShutdown,
    InboundClosed,
}

/// Fixed ceiling for one floodfill maintenance batch.
pub const MAX_FLOODFILL_MAINTENANCE_BATCH: usize = 256;

/// Bounded failures starting the supervised owner.
#[derive(Debug, thiserror::Error)]
pub enum FloodfillOwnerError {
    #[error("floodfill owner maintenance policy is outside its bound")]
    InvalidMaintenancePolicy,
    #[error("floodfill I2NP dispatch failed: {0}")]
    Dispatch(#[from] crate::router_i2np::RouterI2npError),
}

/// Drives one coordinator as a single bounded daemon owner. The caller must start this future
/// under the runtime's child scope after persistence revalidation and signed RouterInfo
/// installation. There is no task per inbound message or queued effect; direct dials are
/// serialized through one bounded worker path.
pub async fn run_floodfill_owner(
    mut service: crate::router_i2np::Ssu2DaemonHandle,
    mut coordinator: FloodfillCoordinator,
    cancellation: i2pr_runtime::CancellationToken,
    maintenance_period: Duration,
    max_record_age_ms: u64,
    maintenance_batch_size: usize,
) -> Result<FloodfillOwnerExit, FloodfillOwnerError> {
    if maintenance_period.is_zero()
        || maintenance_period > Duration::from_secs(3600)
        || maintenance_batch_size == 0
        || maintenance_batch_size > MAX_FLOODFILL_MAINTENANCE_BATCH
    {
        return Err(FloodfillOwnerError::InvalidMaintenancePolicy);
    }
    let mut maintenance = tokio::time::interval(maintenance_period);
    maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let started = tokio::time::Instant::now();
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                service.shutdown();
                return Ok(FloodfillOwnerExit::RequestedShutdown);
            }
            inbound = service.next_inbound() => {
                let Some(inbound) = inbound else {
                    return Ok(FloodfillOwnerExit::InboundClosed);
                };
                let wall_ms = wall_clock_ms();
                let time = FloodfillTime {
                    wall_ms,
                    monotonic_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                };
                let _ = coordinator.handle_authenticated_i2np(&inbound, time)?;
                while let Some(effect) = coordinator.pop_effect() {
                    let _ = deliver_floodfill_effect_with_dial(
                        &service,
                        coordinator.netdb(),
                        effect,
                        wall_ms,
                        max_record_age_ms,
                        &cancellation,
                    ).await;
                }
            }
            _ = maintenance.tick() => {
                let wall_ms = wall_clock_ms();
                let _ = coordinator.maintenance_tick(wall_ms, max_record_age_ms, maintenance_batch_size);
            }
        }
    }
}

fn wall_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
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
        Self::new_with_netdb(
            local_router,
            ServerNetDb::with_config(netdb_config),
            policy,
            store_policy,
            resource_policy,
            replication_policy,
        )
    }

    /// Creates the coordinator around a preloaded ServerNetDb. Startup may use this after
    /// Plan 276 persistence has revalidated each restored record and narrowed its provenance.
    pub fn new_with_netdb(
        local_router: Hash,
        netdb: ServerNetDb,
        policy: FloodfillCoordinatorPolicy,
        store_policy: FloodfillStorePolicy,
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
            netdb,
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
        _link: LinkId,
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
            self.enqueue(FloodfillDaemonEffect::StoreAck { ack: *ack }, 64);
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
        let (outcome, body) =
            crate::router_i2np::dispatch_router_i2np_with_floodfill_body(inbound, time.wall_ms)?;
        if !self.role.accepts_new_work() {
            return Ok(FloodfillDispatchOutcome::Ignored(outcome));
        }
        let message_id = match &outcome {
            crate::router_i2np::RouterI2npOutcome::RouterControl { message_id, .. } => *message_id,
            _ => return Ok(FloodfillDispatchOutcome::Ignored(outcome)),
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
            None => Ok(FloodfillDispatchOutcome::Ignored(outcome)),
        }
    }

    pub fn handle_lookup(
        &mut self,
        peer: PeerId,
        link: LinkId,
        lookup: &DatabaseLookupMessage,
        time: FloodfillTime,
    ) -> Result<(), i2pr_netdb::LookupFailure> {
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

    pub fn pop_effect(&mut self) -> Option<FloodfillLeasedEffect> {
        let (effect, bytes, slot, byte_lease) = self.effects.pop_front()?;
        self.queued_bytes = self.queued_bytes.saturating_sub(bytes);
        Some(FloodfillLeasedEffect {
            effect: Some(effect),
            _slot: slot,
            _bytes: byte_lease,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{DatabaseStoreData, DatabaseStoreType, DeferredPayload};

    fn enable(coordinator: &mut FloodfillCoordinator) {
        coordinator.update_eligibility(FloodfillEligibilitySnapshot {
            controlled_qualification_permit: true,
            qualified_ssu2_address: true,
            direct_reachability: true,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: true,
            supervision_healthy: true,
        });
        assert!(coordinator.begin_activation());
        assert!(coordinator.complete_activation());
    }

    fn coordinator() -> FloodfillCoordinator {
        FloodfillCoordinator::new(
            Hash::from_bytes([9; 32]),
            FloodfillCoordinatorPolicy {
                max_queued_effects: 1,
                max_queued_bytes: 128 * 1024,
                max_peer_candidates: 2,
                max_peer_scan_work: 2,
                max_record_age_ms: 60_000,
            },
            FloodfillStorePolicy::default(),
            ServerNetDbConfig::default(),
            FloodfillResourcePolicy {
                queued_effects: 1,
                queued_bytes: 128 * 1024,
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

    #[test]
    fn authenticated_lookup_routes_to_supplied_from_for_direct_and_tunnel() {
        let mut coordinator = coordinator();
        enable(&mut coordinator);
        let authenticated_peer = PeerId::from_bytes([1; 32]);
        let gateway = Hash::from_bytes([3; 32]);
        let mut lookup = DatabaseLookupMessage {
            key: Hash::from_bytes([4; 32]),
            from: gateway,
            delivery_flag: false,
            reply_tunnel_id: None,
            lookup_type: 0,
            excluded_peers: Vec::new(),
            reply_encryption: i2pr_proto::ReplyEncryption::None,
        };
        let time = FloodfillTime {
            wall_ms: 1,
            monotonic_ms: 1,
        };
        coordinator
            .handle_lookup(authenticated_peer, LinkId::new(1).unwrap(), &lookup, time)
            .expect("direct route may differ from authenticated peer");
        let Some(FloodfillLeasedEffect {
            effect: Some(FloodfillDaemonEffect::LookupReply { intent, .. }),
            ..
        }) = coordinator.pop_effect()
        else {
            panic!("lookup effect")
        };
        assert!(
            matches!(intent, i2pr_netdb::FloodfillReplyIntent::Direct { peer, .. } if peer == gateway)
        );

        lookup.delivery_flag = true;
        lookup.reply_tunnel_id = Some(9);
        lookup.reply_encryption = i2pr_proto::ReplyEncryption::Ecies {
            reply_key: i2pr_proto::ReplySecret::from_bytes([0x33; 32]),
            reply_tags: vec![i2pr_proto::ReplySecret::from_bytes([0x44; 8])],
        };
        // The typed effect keeps the supplied route; authenticated peer is only provenance.
        assert_eq!(
            coordinator.handle_lookup(authenticated_peer, LinkId::new(1).unwrap(), &lookup, time),
            Ok(())
        );
        let Some(FloodfillLeasedEffect {
            effect: Some(FloodfillDaemonEffect::LookupReply { intent, .. }),
            ..
        }) = coordinator.pop_effect()
        else {
            panic!("tunnel lookup effect")
        };
        assert!(
            matches!(intent, i2pr_netdb::FloodfillReplyIntent::Tunnel { gateway: target, tunnel_id: 9, .. } if target == gateway)
        );
    }

    #[test]
    fn effect_encoder_preserves_ack_token_and_single_garlic_tunnel_nesting() {
        let mut coordinator = coordinator();
        let gateway = Hash::from_bytes([0x31; 32]);
        let ack = FloodfillAck {
            reply_token: 0xAABB_CCDD,
            reply_tunnel_id: Some(0x1234),
            reply_gateway: Some(gateway),
            message: i2pr_proto::DeliveryStatusMessage::new(0xAABB_CCDD, Date::from_millis(1000)),
        };
        assert!(coordinator.enqueue(FloodfillDaemonEffect::StoreAck { ack }, 64));
        let mut leased = coordinator.pop_effect().expect("leased ack");
        let (peer, bytes) = encode_floodfill_effect(&mut leased, 1000).expect("encode ack");
        assert_eq!(peer.hash(), gateway);
        let outer = I2npMessage::decode_standard(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
            .expect("outer gateway");
        let I2npBody::TunnelGateway(outer_gateway) = outer.into_body() else {
            panic!("ack route must use TunnelGateway")
        };
        assert_eq!(outer_gateway.tunnel_id, 0x1234);
        let I2npBody::DeliveryStatus(status) = outer_gateway.message.into_body() else {
            panic!("nested ack body")
        };
        assert_eq!(status.message_id, 0xAABB_CCDD);
        drop(leased);

        let tunnel_payload = vec![0x55; 96];
        assert!(coordinator.enqueue(
            FloodfillDaemonEffect::LookupReply {
                requester: PeerId::from_hash(Hash::from_bytes([0x44; 32])),
                inbound_link: LinkId::new(7).unwrap(),
                intent: i2pr_netdb::FloodfillReplyIntent::Tunnel {
                    gateway,
                    tunnel_id: 0x5678,
                    payload: tunnel_payload.clone(),
                    protection: i2pr_netdb::ReplyProtection::SuppliedKeyEcies,
                },
            },
            128,
        ));
        let mut leased = coordinator.pop_effect().expect("leased tunnel reply");
        let (peer, bytes) = encode_floodfill_effect(&mut leased, 1000).expect("encode reply");
        assert_eq!(peer.hash(), gateway);
        let outer = I2npMessage::decode_standard(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
            .expect("outer gateway");
        let I2npBody::TunnelGateway(outer_gateway) = outer.into_body() else {
            panic!("lookup route must use TunnelGateway")
        };
        assert_eq!(outer_gateway.tunnel_id, 0x5678);
        let I2npBody::Garlic(garlic) = outer_gateway.message.into_body() else {
            panic!("exactly one Garlic wrapper")
        };
        assert_eq!(garlic.payload.as_bytes(), tunnel_payload);
    }

    #[test]
    fn effect_encoder_routes_direct_ack_and_replication_without_tunnel_fallback() {
        let mut coordinator = coordinator();
        let gateway = Hash::from_bytes([0x61; 32]);
        let ack = FloodfillAck {
            reply_token: 0x1020_3040,
            reply_tunnel_id: None,
            reply_gateway: Some(gateway),
            message: i2pr_proto::DeliveryStatusMessage::new(0x1020_3040, Date::from_millis(1000)),
        };
        assert!(coordinator.enqueue(FloodfillDaemonEffect::StoreAck { ack }, 64));
        let mut leased = coordinator.pop_effect().expect("leased direct ack");
        let (peer, bytes) = encode_floodfill_effect(&mut leased, 1000).expect("encode ack");
        assert_eq!(peer.hash(), gateway);
        let message = I2npMessage::decode_standard(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
            .expect("direct acknowledgement");
        let I2npBody::DeliveryStatus(status) = message.into_body() else {
            panic!("direct ack must not be tunnel wrapped")
        };
        assert_eq!(status.message_id, 0x1020_3040);
        drop(leased);

        let mut action = action();
        action.deadline_ms = 5_000;
        let flood_peer = action.peer;
        assert!(coordinator.enqueue(FloodfillDaemonEffect::DirectFlood { action }, 128));
        let mut leased = coordinator.pop_effect().expect("leased replication");
        let (peer, bytes) = encode_floodfill_effect(&mut leased, 1000).expect("encode flood");
        assert_eq!(peer.hash(), flood_peer);
        let message = I2npMessage::decode_standard(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
            .expect("direct replication");
        let I2npBody::DatabaseStore(store) = message.into_body() else {
            panic!("replication must be a direct DatabaseStore")
        };
        assert_eq!(store.reply_token, 0);
        assert_eq!(store.reply_gateway, None);
        assert_eq!(store.reply_tunnel_id, None);
    }
}
