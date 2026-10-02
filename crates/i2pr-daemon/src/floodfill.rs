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
    I2npMessage, Mapping, OpaqueMessageBody, RouterInfo, TunnelGatewayMessage,
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
/// serialized through one bounded worker path. Every drained effect records its typed
/// outcome on the coordinator. On cancellation the owner drains the bounded remainder of
/// the queue (each delivery observes the cancelled token and returns immediately) until
/// `drain_timeout` elapses, then shuts the service down and exits. The final coordinator
/// stats are returned alongside the exit reason so tests own the delivery accounting.
pub async fn run_floodfill_owner(
    mut service: crate::router_i2np::Ssu2DaemonHandle,
    mut coordinator: FloodfillCoordinator,
    cancellation: i2pr_runtime::CancellationToken,
    maintenance_period: Duration,
    max_record_age_ms: u64,
    maintenance_batch_size: usize,
    drain_timeout: Duration,
) -> Result<(FloodfillOwnerExit, FloodfillCoordinatorStats), FloodfillOwnerError> {
    if maintenance_period.is_zero()
        || maintenance_period > Duration::from_secs(3600)
        || maintenance_batch_size == 0
        || maintenance_batch_size > MAX_FLOODFILL_MAINTENANCE_BATCH
        || drain_timeout.is_zero()
        || drain_timeout > Duration::from_secs(60)
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
                let deadline = tokio::time::Instant::now() + drain_timeout;
                while let Some(effect) = coordinator.pop_effect() {
                    if tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    let outcome = deliver_floodfill_effect_with_dial(
                        &service,
                        coordinator.netdb(),
                        effect,
                        wall_clock_ms(),
                        max_record_age_ms,
                        &cancellation,
                    ).await;
                    coordinator.record_delivery(outcome);
                }
                service.shutdown();
                return Ok((FloodfillOwnerExit::RequestedShutdown, coordinator.stats()));
            }
            inbound = service.next_inbound() => {
                let Some(inbound) = inbound else {
                    return Ok((FloodfillOwnerExit::InboundClosed, coordinator.stats()));
                };
                let wall_ms = wall_clock_ms();
                let time = FloodfillTime {
                    wall_ms,
                    monotonic_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                };
                let _ = coordinator.handle_authenticated_i2np(&inbound, time)?;
                while let Some(effect) = coordinator.pop_effect() {
                    let outcome = deliver_floodfill_effect_with_dial(
                        &service,
                        coordinator.netdb(),
                        effect,
                        wall_ms,
                        max_record_age_ms,
                        &cancellation,
                    ).await;
                    coordinator.record_delivery(outcome);
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

/// Maximum RouterInfo bytes the controlled composition installs or
/// publishes. Mirrors the runtime install bound; the establishment
/// decode cap is stricter and fires first.
const MAX_CONTROLLED_ROUTER_INFO_BYTES: usize = 16 * 1024;
/// Upper bound for one controlled activation/withdrawal drain.
const MAX_CONTROLLED_DRAIN: Duration = Duration::from_secs(60);

/// Failure of the controlled activation or withdrawal composition.
///
/// Every variant fails closed: the runtime keeps the previously
/// installed non-`f` RouterInfo (installs replace atomically only on
/// success) and the role never reports Active. Variants after
/// `begin_activation` additionally fail the role controller so a
/// partial activation cannot linger in Activating.
#[derive(Debug, Eq, PartialEq)]
pub enum ControlledActivationError {
    /// The caller cancelled before any evidence was recorded.
    Cancelled,
    /// A timeout, deadline, or record-age bound failed validation.
    InvalidConfig,
    /// Explicit-bind recording failed.
    BindFailed(i2pr_runtime::Ssu2PublicationUnavailable),
    /// The peer-test evidence exchange failed or did not confirm.
    EvidenceFailed(i2pr_runtime::ControlledPeerTestError),
    /// No above-floor publication material was available.
    PublicationFailed(i2pr_runtime::Ssu2PublicationUnavailable),
    /// The eligibility snapshot did not open activation.
    EligibilityFailed,
    /// Role begin/complete or the advertisement permit failed.
    ActivationFailed,
    /// The floodfill RouterInfo build failed.
    BuildFailed(i2pr_netdb::LocalRouterInfoError),
    /// The RouterInfo install failed; the role was failed closed.
    InstallFailed,
    /// The local publish failed; the role was failed closed.
    PublishFailed,
    /// The withdrawal sequence failed; the role stays Draining and
    /// the caller retries with a fresh loss snapshot.
    WithdrawalFailed,
}

impl std::fmt::Display for ControlledActivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "controlled activation cancelled",
            Self::InvalidConfig => "controlled activation bound is invalid",
            Self::BindFailed(_) => "controlled explicit-bind recording failed",
            Self::EvidenceFailed(_) => "controlled peer-test exchange failed",
            Self::PublicationFailed(_) => "controlled publication material unavailable",
            Self::EligibilityFailed => "controlled eligibility did not open activation",
            Self::ActivationFailed => "controlled role activation failed",
            Self::BuildFailed(_) => "controlled RouterInfo build failed",
            Self::InstallFailed => "controlled RouterInfo install failed",
            Self::PublishFailed => "controlled local publish failed",
            Self::WithdrawalFailed => "controlled withdrawal failed",
        })
    }
}

impl std::error::Error for ControlledActivationError {}

/// Inputs for [`activate_controlled`].
///
/// The coordinator is borrowed mutably: no owner loop may run
/// concurrently with activation. Production wiring would hold the
/// coordinator behind shared ownership; the current composition has
/// no production caller and tests drive it exclusively.
pub struct ControlledActivationParams<'a> {
    /// Owner scope for the ephemeral peer-test helper tasks.
    pub scope: &'a i2pr_runtime::ChildScope,
    /// The live daemon SSU2 service (Alice).
    pub handle: &'a crate::router_i2np::Ssu2DaemonHandle,
    /// The coordinator to activate.
    pub coordinator: &'a mut FloodfillCoordinator,
    /// The daemon identity (signs the floodfill RouterInfo and the
    /// Alice peer-test blocks; key bytes never leave the bundle).
    pub bundle: &'a i2pr_crypto::RouterIdentityBundle,
    /// Alice's transport static public key (helper dial targets).
    pub alice_static_public: i2pr_runtime::Ssu2PublicKey,
    /// Alice's intro key (seals helper-to-Alice test blocks).
    pub alice_intro: i2pr_runtime::IntroKey,
    /// Non-evidence eligibility fields (storage, maintenance,
    /// headroom, clock, supervision, NetDB readiness). The three
    /// evidence fields are forced by the composition only after the
    /// evidence exists.
    pub base_eligibility: FloodfillEligibilitySnapshot,
    /// Wall-clock milliseconds for material, build, and install.
    pub wall_now_ms: u64,
    /// Per-step deadline for the evidence exchange.
    pub step_timeout: Duration,
    /// Poll interval while awaiting exchange transitions.
    pub poll_interval: Duration,
    /// Cooperative cancellation.
    pub cancellation: &'a i2pr_runtime::CancellationToken,
}

/// A completed controlled activation.
pub struct ControlledActivation {
    /// The opaque advertisement permit minted by the role
    /// controller. Thread it into withdrawal; it cannot be forged.
    pub permit: i2pr_netdb::FloodfillAdvertisementPermit,
    /// The installed floodfill RouterInfo bytes (`caps=f`).
    pub router_info: Vec<u8>,
    /// The above-floor publication material that opened activation.
    pub material: i2pr_runtime::Ssu2PublicationMaterial,
}

/// Runs the controlled activation sequence (Plan 283 §5).
///
/// Explicit-bind recording, then the peer-test evidence exchange,
/// then above-floor publication material, then eligibility, then
/// role begin/complete, then the permit-gated floodfill RouterInfo
/// build, then the atomic install, then the local publish. Any
/// failure after `begin_activation` fails the role closed and returns
/// the typed error; the runtime keeps its previous RouterInfo.
pub async fn activate_controlled(
    params: ControlledActivationParams<'_>,
) -> Result<ControlledActivation, ControlledActivationError> {
    if params.cancellation.is_cancelled() {
        return Err(ControlledActivationError::Cancelled);
    }
    if params.step_timeout.is_zero()
        || params.poll_interval.is_zero()
        || params.poll_interval > params.step_timeout
    {
        return Err(ControlledActivationError::InvalidConfig);
    }
    let service = params.handle.service();
    service
        .note_explicit_bind_for_controlled_qualification()
        .map_err(ControlledActivationError::BindFailed)?;
    let alice_hash = params.bundle.identity().hash().map_err(|_| {
        ControlledActivationError::EvidenceFailed(
            i2pr_runtime::ControlledPeerTestError::ExchangeCrypto,
        )
    })?;
    let alice_signing_public = params.bundle.signing_key().public_key().map_err(|_| {
        ControlledActivationError::EvidenceFailed(
            i2pr_runtime::ControlledPeerTestError::ExchangeCrypto,
        )
    })?;
    let bundle = params.bundle;
    let alice_sign = |preimage: &[u8]| -> Result<Vec<u8>, i2pr_runtime::ControlledPeerTestError> {
        bundle
            .signing_key()
            .sign(preimage)
            .map(|signature| signature.as_bytes().to_vec())
            .map_err(|_| i2pr_runtime::ControlledPeerTestError::ExchangeCrypto)
    };
    let bound = params
        .handle
        .local_v4()
        .or(params.handle.local_v6())
        .ok_or(ControlledActivationError::PublicationFailed(
            i2pr_runtime::Ssu2PublicationUnavailable::NoBoundSocket,
        ))?;
    let outcome = i2pr_runtime::run_controlled_peer_test(i2pr_runtime::ControlledPeerTestParams {
        scope: params.scope,
        alice: service,
        alice_hash: *alice_hash.as_bytes(),
        alice_addr: bound,
        alice_static_public: params.alice_static_public,
        alice_intro: params.alice_intro,
        alice_signing_public,
        alice_sign: &alice_sign,
        cancellation: params.cancellation,
        step_timeout: params.step_timeout,
        poll_interval: params.poll_interval,
    })
    .await
    .map_err(ControlledActivationError::EvidenceFailed)?;
    if !matches!(
        outcome,
        i2pr_runtime::ControlledPeerTestOutcome::Confirmed { .. }
    ) {
        return Err(ControlledActivationError::EvidenceFailed(
            i2pr_runtime::ControlledPeerTestError::ExchangeNotConfirmed,
        ));
    }
    let material = service
        .publication_material(params.wall_now_ms)
        .map_err(ControlledActivationError::PublicationFailed)?;
    let snapshot = FloodfillEligibilitySnapshot {
        controlled_qualification_permit: true,
        qualified_ssu2_address: true,
        direct_reachability: true,
        ..params.base_eligibility
    };
    let effect = params.coordinator.update_eligibility(snapshot);
    if effect != FloodfillRoleEffect::ReadyToActivate
        || params.coordinator.role_state() != FloodfillRoleState::Eligible
    {
        return Err(ControlledActivationError::EligibilityFailed);
    }
    if !params.coordinator.begin_activation() || !params.coordinator.complete_activation() {
        params.coordinator.fail_activation();
        return Err(ControlledActivationError::ActivationFailed);
    }
    let permit = params.coordinator.advertisement_permit().ok_or_else(|| {
        params.coordinator.fail_activation();
        ControlledActivationError::ActivationFailed
    })?;
    let built = i2pr_netdb::LocalRouterInfoBuilder::new(params.bundle)
        .build_floodfill(
            Date::from_millis(params.wall_now_ms),
            Mapping::empty(),
            material.address.clone(),
            &permit,
        )
        .map_err(|error| {
            params.coordinator.fail_activation();
            ControlledActivationError::BuildFailed(error)
        })?;
    let encoded = built
        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| {
            params.coordinator.fail_activation();
            ControlledActivationError::BuildFailed(
                i2pr_netdb::LocalRouterInfoError::InvalidMapping { context: "encode" },
            )
        })?;
    if service
        .install_local_router_info(encoded.clone(), params.wall_now_ms)
        .is_err()
    {
        params.coordinator.fail_activation();
        return Err(ControlledActivationError::InstallFailed);
    }
    let time = FloodfillTime {
        wall_ms: params.wall_now_ms,
        monotonic_ms: params.wall_now_ms,
    };
    if publish_local_router_info(params.coordinator, &encoded, &permit, time).is_err() {
        params.coordinator.fail_activation();
        return Err(ControlledActivationError::PublishFailed);
    }
    Ok(ControlledActivation {
        permit,
        router_info: encoded,
        material,
    })
}

/// Stores our own RouterInfo in the coordinator NetDB as a local
/// publication so floodfill lookups serve it.
///
/// Requires the opaque activation permit plus an Active or Draining
/// role: no other path can publish, and the permit cannot be forged.
/// Peer origination of the record belongs to Plan 278 (real
/// floodfill peers); here the planner honestly yields no reflood
/// actions for a locally-published record.
fn publish_local_router_info(
    coordinator: &mut FloodfillCoordinator,
    encoded: &[u8],
    _permit: &i2pr_netdb::FloodfillAdvertisementPermit,
    time: FloodfillTime,
) -> Result<i2pr_netdb::RouterHash, ControlledActivationError> {
    if !matches!(
        coordinator.role_state(),
        FloodfillRoleState::Active | FloodfillRoleState::Draining
    ) {
        return Err(ControlledActivationError::PublishFailed);
    }
    let info = RouterInfo::decode(encoded, MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| ControlledActivationError::PublishFailed)?;
    let key = i2pr_netdb::router_hash(info.router_identity())
        .map_err(|_| ControlledActivationError::PublishFailed)?;
    if key != i2pr_netdb::RouterHash::from_hash(coordinator.local_router()) {
        return Err(ControlledActivationError::PublishFailed);
    }
    let validated = i2pr_netdb::ValidatedRouterInfo::from_router_info(
        info,
        Some(key),
        i2pr_netdb::ValidationContext::new(Date::from_millis(time.wall_ms)),
    )
    .map_err(|_| ControlledActivationError::PublishFailed)?;
    coordinator
        .netdb_mut()
        .insert(
            i2pr_netdb::ValidatedNetDbRecord::RouterInfo(validated),
            i2pr_netdb::RecordProvenance {
                namespace: i2pr_netdb::NetDbNamespace::MainRouter,
                inbound: i2pr_netdb::InboundProvenance::Local,
                purpose: i2pr_netdb::StorePurpose::LocalPublication,
                observed_at_ms: time.wall_ms,
            },
        )
        .map_err(|_| ControlledActivationError::PublishFailed)?;
    Ok(key)
}

/// Inputs for [`withdraw_controlled`].
pub struct ControlledWithdrawalParams<'a> {
    /// The live daemon SSU2 service.
    pub handle: &'a crate::router_i2np::Ssu2DaemonHandle,
    /// The Active coordinator to withdraw.
    pub coordinator: &'a mut FloodfillCoordinator,
    /// The daemon identity (re-signs the withdrawal RouterInfo).
    pub bundle: &'a i2pr_crypto::RouterIdentityBundle,
    /// The permit threaded through the activation record.
    pub permit: &'a i2pr_netdb::FloodfillAdvertisementPermit,
    /// The qualified SSU2 address to keep (without `caps=f`).
    pub address: i2pr_proto::RouterAddress,
    /// The health-loss snapshot (at least one field false).
    pub snapshot: FloodfillEligibilitySnapshot,
    /// Wall-clock milliseconds for build and install.
    pub wall_now_ms: u64,
    /// Bound for draining queued effects before `complete_drain`.
    pub drain_timeout: Duration,
    /// Maximum record age for drain deliveries.
    pub max_record_age_ms: u64,
    /// Cooperative cancellation.
    pub cancellation: &'a i2pr_runtime::CancellationToken,
}

/// Runs the controlled health-withdrawal sequence (Plan 283 §5).
///
/// Failing eligibility stops admission (Draining), then the same
/// qualified address is re-installed and re-published without
/// `caps=f`, then queued effects drain bounded to the deadline, then
/// the role reaches Disabled. Leftover effects or a refused drain
/// fail closed with the role left Draining for a retry.
pub async fn withdraw_controlled(
    params: ControlledWithdrawalParams<'_>,
) -> Result<(), ControlledActivationError> {
    if params.cancellation.is_cancelled() {
        return Err(ControlledActivationError::Cancelled);
    }
    if params.drain_timeout.is_zero() || params.drain_timeout > MAX_CONTROLLED_DRAIN {
        return Err(ControlledActivationError::InvalidConfig);
    }
    if params.snapshot.eligible() {
        return Err(ControlledActivationError::InvalidConfig);
    }
    let effect = params.coordinator.update_eligibility(params.snapshot);
    if effect != FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        || params.coordinator.role_state() != FloodfillRoleState::Draining
    {
        return Err(ControlledActivationError::WithdrawalFailed);
    }
    let built = i2pr_netdb::LocalRouterInfoBuilder::new(params.bundle)
        .build_floodfill_withdrawal(
            Date::from_millis(params.wall_now_ms),
            Mapping::empty(),
            params.address.clone(),
            params.permit,
        )
        .map_err(|_| ControlledActivationError::WithdrawalFailed)?;
    let encoded = built
        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| ControlledActivationError::WithdrawalFailed)?;
    if params
        .handle
        .service()
        .install_local_router_info(encoded.clone(), params.wall_now_ms)
        .is_err()
    {
        return Err(ControlledActivationError::WithdrawalFailed);
    }
    let time = FloodfillTime {
        wall_ms: params.wall_now_ms,
        monotonic_ms: params.wall_now_ms,
    };
    if publish_local_router_info(params.coordinator, &encoded, params.permit, time).is_err() {
        return Err(ControlledActivationError::WithdrawalFailed);
    }
    let deadline = tokio::time::Instant::now() + params.drain_timeout;
    while let Some(effect) = params.coordinator.pop_effect() {
        if tokio::time::Instant::now() >= deadline || params.cancellation.is_cancelled() {
            return Err(ControlledActivationError::WithdrawalFailed);
        }
        let outcome = deliver_floodfill_effect_with_dial(
            params.handle,
            params.coordinator.netdb(),
            effect,
            params.wall_now_ms,
            params.max_record_age_ms,
            params.cancellation,
        )
        .await;
        params.coordinator.record_delivery(outcome);
    }
    if params.coordinator.stats().queued_effects > 0 {
        return Err(ControlledActivationError::WithdrawalFailed);
    }
    if !params.coordinator.complete_drain() {
        return Err(ControlledActivationError::WithdrawalFailed);
    }
    Ok(())
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
    pub delivered_effects: u64,
    pub failed_effects: u64,
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
    /// Completes a bounded drain: Draining reaches Disabled.
    pub fn complete_drain(&mut self) -> bool {
        self.role.complete_drain()
    }
    pub fn advertisement_permit(&self) -> Option<i2pr_netdb::FloodfillAdvertisementPermit> {
        self.role.advertisement_permit()
    }
    /// Fails a started activation closed: the role leaves Active and
    /// never advertises again until a fresh eligible activation. The
    /// previously installed RouterInfo is untouched (installs replace
    /// atomically only on success).
    pub fn fail_activation(&mut self) -> FloodfillRoleEffect {
        self.role.fail()
    }
    /// Returns the local router hash this coordinator serves.
    pub const fn local_router(&self) -> Hash {
        self.local_router
    }
    pub fn stats(&self) -> FloodfillCoordinatorStats {
        FloodfillCoordinatorStats {
            queued_effects: self.effects.len(),
            queued_bytes: self.queued_bytes,
            ..self.stats
        }
    }

    /// Records one effect-drain outcome. Delivered outcomes count the effect as
    /// delivered; every other typed outcome counts it as failed. Leases release
    /// exactly once through the drained `FloodfillLeasedEffect` on every path.
    pub fn record_delivery(&mut self, outcome: FloodfillDeliveryOutcome) {
        if matches!(outcome, FloodfillDeliveryOutcome::Delivered(_)) {
            self.stats.delivered_effects = self.stats.delivered_effects.saturating_add(1);
        } else {
            self.stats.failed_effects = self.stats.failed_effects.saturating_add(1);
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

    fn eligible_snapshot() -> FloodfillEligibilitySnapshot {
        FloodfillEligibilitySnapshot {
            controlled_qualification_permit: true,
            qualified_ssu2_address: true,
            direct_reachability: true,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: true,
            supervision_healthy: true,
        }
    }

    #[test]
    fn ordinary_configuration_without_permit_or_evidence_stays_disabled() {
        let mut coordinator = coordinator();
        // An ordinary snapshot without the controlled permit never
        // opens activation, even with every other field true.
        let mut ordinary = eligible_snapshot();
        ordinary.controlled_qualification_permit = false;
        assert_eq!(
            coordinator.update_eligibility(ordinary),
            FloodfillRoleEffect::None
        );
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);
        assert!(!coordinator.begin_activation());
        assert!(!coordinator.complete_activation());
        assert!(coordinator.advertisement_permit().is_none());
        // A failing health field on an otherwise eligible snapshot
        // also stays out: evidence alone never activates.
        let mut failing = eligible_snapshot();
        failing.supervision_healthy = false;
        assert_eq!(
            coordinator.update_eligibility(failing),
            FloodfillRoleEffect::None
        );
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);
        assert!(coordinator.advertisement_permit().is_none());
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
    fn delivery_outcomes_accounted_and_leases_release_on_every_drop_path() {
        let mut coordinator = coordinator();
        let mut live = action();
        live.deadline_ms = 5_000;
        assert!(coordinator.enqueue(FloodfillDaemonEffect::DirectFlood { action: live }, 128));
        let mut leased = coordinator.pop_effect().expect("leased effect");
        // Leases travel with the effect: the coordinator queue is empty but the
        // budget still shows the held effect until the lease drops.
        assert_eq!(coordinator.resources().snapshot().queued_effects, 1);
        let (peer, _) = encode_floodfill_effect(&mut leased, 1000).expect("encode");
        assert_eq!(peer.hash(), action().peer);
        // Double drain fails closed: the effect was consumed by the first encode.
        assert_eq!(
            encode_floodfill_effect(&mut leased, 1000),
            Err(FloodfillDeliveryOutcome::InvalidEffect)
        );
        coordinator.record_delivery(FloodfillDeliveryOutcome::InvalidEffect);
        drop(leased);
        assert_eq!(coordinator.resources().snapshot().queued_effects, 0);
        assert_eq!(coordinator.resources().snapshot().queued_bytes, 0);
        assert_eq!(coordinator.stats().failed_effects, 1);
        assert_eq!(coordinator.stats().delivered_effects, 0);
        coordinator.record_delivery(FloodfillDeliveryOutcome::Delivered(
            crate::router_i2np::RouterDeliveryOutcome::Accepted,
        ));
        assert_eq!(coordinator.stats().delivered_effects, 1);
        assert_eq!(coordinator.stats().failed_effects, 1);
    }

    #[test]
    fn invalid_ack_route_and_unknown_dial_target_fail_closed() {
        let mut coordinator = coordinator();
        // Ack without a reply gateway has no route.
        let ack = FloodfillAck {
            reply_token: 0x99,
            reply_tunnel_id: None,
            reply_gateway: None,
            message: i2pr_proto::DeliveryStatusMessage::new(0x99, Date::from_millis(1000)),
        };
        assert!(coordinator.enqueue(FloodfillDaemonEffect::StoreAck { ack }, 64));
        let mut leased = coordinator.pop_effect().expect("leased ack");
        assert_eq!(
            encode_floodfill_effect(&mut leased, 1000),
            Err(FloodfillDeliveryOutcome::InvalidRoute)
        );
        drop(leased);
        // Unknown reply peer resolves to no dial target in an empty NetDB.
        assert!(
            validated_dial_target(
                coordinator.netdb(),
                PeerId::from_bytes([0x77; 32]),
                1000,
                60_000
            )
            .is_none()
        );
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
