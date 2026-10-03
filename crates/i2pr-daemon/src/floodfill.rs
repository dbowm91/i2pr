//! Controlled floodfill role composition. Normal daemon configuration never constructs a permit.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use i2pr_netdb::{
    DirectFloodAction, FloodfillAck, FloodfillEligibilitySnapshot, FloodfillIngress,
    FloodfillLookupEffect, FloodfillPeerView, FloodfillResourceBudget, FloodfillResourcePolicy,
    FloodfillRoleController, FloodfillRoleEffect, FloodfillRoleState, FloodfillStoreEffect,
    FloodfillStorePolicy, FloodfillStoreService, FloodfillTime, ReplicationPlanner,
    ReplicationPolicy, ServerNetDb, ServerNetDbConfig, controlled_router_options,
    is_qualified_ssu2_address,
};
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseStoreMessage, Date, DeferredPayload, Hash, I2npBody,
    I2npMessage, OpaqueMessageBody, RouterInfo, TunnelGatewayMessage,
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
                let Some(message) = encode_transport(body, now_ms) else {
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
                let Some(garlic) = encode_inner_standard(garlic, now_ms) else {
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
                let Some(message) = encode_transport(body, now_ms) else {
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
                encode_transport(I2npBody::DatabaseStore(Box::new(action.message)), now_ms)
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
        None | Some(0) => encode_transport(body, now_ms),
        Some(tunnel_id) => {
            let nested = encode_inner_standard(body, now_ms)?;
            let message = I2npMessage::decode_standard(&nested, MAX_FLOODFILL_EFFECT_BYTES).ok()?;
            encode_transport(
                I2npBody::TunnelGateway(Box::new(TunnelGatewayMessage {
                    tunnel_id,
                    message: Box::new(message),
                })),
                now_ms,
            )
        }
    }
}

/// Encodes one outbound floodfill effect in the 9-byte NTCP2/SSU2 short-transport form.
///
/// Plan 302: the SSU2 session layer (`queue_i2np_message`) reads every outbound byte
/// string as `[type(1)][message id(4)][expiration seconds(4)][body]`. The previous
/// standard-header encoding placed a u64 millisecond expiration at bytes 5-12, so the
/// exact-pinned reference read the high 4 bytes of millisecond time as seconds
/// (year 1970) and dropped every floodfill reply as expired. The seconds conversion
/// fails closed on overflow, mirroring `outbound_lookup::encode_transport_tunnel_data`.
fn encode_transport(body: I2npBody, now_ms: u64) -> Option<Vec<u8>> {
    let id = NEXT_FLOODFILL_MESSAGE_ID.fetch_add(1, Ordering::Relaxed);
    let expiration_ms = now_ms.checked_add(30_000)?;
    let expiration_seconds = u32::try_from(expiration_ms / 1000).ok()?;
    I2npMessage::new_short_transport(id, expiration_seconds, body)
        .ok()?
        .encode_short_transport_to_vec(MAX_FLOODFILL_EFFECT_BYTES)
        .ok()
}

/// Encodes the tunnel-wrapped inner clove in standard form.
///
/// The inner message travels embedded inside the TunnelGateway body, where the
/// reference parses a full standard-header message (the same shape the
/// tunnel-construction paths already emit and assert). Only the outer envelope
/// that crosses the SSU2 session boundary uses [`encode_transport`].
fn encode_inner_standard(body: I2npBody, now_ms: u64) -> Option<Vec<u8>> {
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
    /// The installed floodfill RouterInfo bytes (`caps=fR`: the `R`
    /// rides on the peer-test-confirmed reachability proof, Plan 306 /
    /// ADR 0030).
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
    // Plan 306 / ADR 0030: the Confirmed outcome just proved inbound
    // SSU2 acceptance of our bound address by an independent reference,
    // so `R` is truthful in controlled/loopback scope for this
    // activation. The proof is minted here — the only non-test mint
    // site, confined by `scripts/check-m12-floodfill-boundaries.sh` —
    // and travels with the activation into the proof-gated builder.
    let reachability_proof = i2pr_netdb::LoopbackReachabilityProof::attest_confirmed_peer_test();
    let material = service
        .publication_material(params.wall_now_ms)
        .map_err(ControlledActivationError::PublicationFailed)?;
    let snapshot = FloodfillEligibilitySnapshot {
        controlled_qualification_permit: true,
        qualified_ssu2_address: true,
        direct_reachability: true,
        ..params.base_eligibility
    };
    let tail = activation_tail(ActivationInput {
        coordinator: params.coordinator,
        bundle: params.bundle,
        snapshot,
        address: material.address.clone(),
        reachability_proof: Some(reachability_proof),
        service,
        wall_now_ms: params.wall_now_ms,
    });
    let tail = match tail {
        Ok(tail) => tail,
        Err(ActivationTailError::Ineligible) => {
            return Err(ControlledActivationError::EligibilityFailed);
        }
        Err(ActivationTailError::Activation) => {
            return Err(ControlledActivationError::ActivationFailed);
        }
        Err(ActivationTailError::Build(error)) => {
            return Err(ControlledActivationError::BuildFailed(error));
        }
        Err(ActivationTailError::Install) => {
            return Err(ControlledActivationError::InstallFailed);
        }
        Err(ActivationTailError::Publish) => {
            return Err(ControlledActivationError::PublishFailed);
        }
    };
    Ok(ControlledActivation {
        permit: tail.permit,
        router_info: tail.router_info,
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
#[derive(Debug, Eq, PartialEq)]
enum LocalPublishError {
    PublishFailed,
}

fn publish_local_router_info(
    coordinator: &mut FloodfillCoordinator,
    encoded: &[u8],
    _permit: &i2pr_netdb::FloodfillAdvertisementPermit,
    time: FloodfillTime,
) -> Result<i2pr_netdb::RouterHash, LocalPublishError> {
    if !matches!(
        coordinator.role_state(),
        FloodfillRoleState::Active | FloodfillRoleState::Draining
    ) {
        return Err(LocalPublishError::PublishFailed);
    }
    let info = RouterInfo::decode(encoded, MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| LocalPublishError::PublishFailed)?;
    let key = i2pr_netdb::router_hash(info.router_identity())
        .map_err(|_| LocalPublishError::PublishFailed)?;
    if key != i2pr_netdb::RouterHash::from_hash(coordinator.local_router()) {
        return Err(LocalPublishError::PublishFailed);
    }
    let validated = i2pr_netdb::ValidatedRouterInfo::from_router_info(
        info,
        Some(key),
        i2pr_netdb::ValidationContext::new(Date::from_millis(time.wall_ms)),
    )
    .map_err(|_| LocalPublishError::PublishFailed)?;
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
        .map_err(|_| LocalPublishError::PublishFailed)?;
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
    withdrawal_tail(WithdrawalInput {
        handle: params.handle,
        coordinator: params.coordinator,
        bundle: params.bundle,
        permit: params.permit,
        address: params.address.clone(),
        snapshot: params.snapshot,
        wall_now_ms: params.wall_now_ms,
        drain_timeout: params.drain_timeout,
        max_record_age_ms: params.max_record_age_ms,
        cancellation: params.cancellation,
    })
    .await
    .map_err(|_| ControlledActivationError::WithdrawalFailed)
}

/// Categorical reason the normal floodfill path is not Active.
///
/// Fixed variants only: status output carries no peer identifiers,
/// addresses, or key material — only which measured signal failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalIneligibility {
    /// The operator did not opt in (`[floodfill].enabled = false`).
    NotRequested,
    /// No live publication material exists, or its SSU2 address is
    /// not a publicly reachable address other routers could dial.
    AddressUnqualified,
    /// Reachability is not corroborated `Reachable`.
    ReachabilityUnconfirmed,
    /// The serving NetDB is not ready.
    NetdbNotReady,
    /// Persistent storage is not ready.
    StorageNotReady,
    /// The maintenance loop is not running.
    MaintenanceNotReady,
    /// Queue backpressure leaves no headroom.
    ResourceExhausted,
    /// The wall clock is outside the sane range.
    ClockUnsane,
    /// Supervision is not healthy.
    SupervisionUnhealthy,
    /// The last config re-read failed to parse; previous intent kept.
    ConfigInvalid,
    /// The last activation attempt failed closed.
    ActivationFailed,
    /// The last withdrawal attempt failed; the role stays Draining.
    WithdrawalFailed,
}

impl NormalIneligibility {
    /// Stable operator-facing label for status output.
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotRequested => "not-requested",
            Self::AddressUnqualified => "address-unqualified",
            Self::ReachabilityUnconfirmed => "reachability-unconfirmed",
            Self::NetdbNotReady => "netdb-not-ready",
            Self::StorageNotReady => "storage-not-ready",
            Self::MaintenanceNotReady => "maintenance-not-ready",
            Self::ResourceExhausted => "resource-exhausted",
            Self::ClockUnsane => "clock-unsane",
            Self::SupervisionUnhealthy => "supervision-unhealthy",
            Self::ConfigInvalid => "config-invalid",
            Self::ActivationFailed => "activation-failed",
            Self::WithdrawalFailed => "withdrawal-failed",
        }
    }
}

impl std::fmt::Display for NormalIneligibility {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Measured readiness inputs for one normal-path evaluation tick.
///
/// The daemon service gathers these from live state on every bounded
/// tick; tests inject them. `material` is the same live publication
/// material the activation build consumes, so evaluation and build
/// cannot disagree within a tick.
pub struct NormalReadiness<'a> {
    /// Current live SSU2 publication material, if the runtime could
    /// produce any on this tick.
    pub material: Option<&'a i2pr_runtime::Ssu2PublicationMaterial>,
    /// Wall-clock milliseconds of this tick.
    pub wall_now_ms: u64,
    /// The serving NetDB passed persistence revalidation.
    pub netdb_ready: bool,
    /// Persistent storage is writable and was revalidated.
    pub storage_ready: bool,
    /// The maintenance loop is running.
    pub maintenance_ready: bool,
    /// Queue occupancy leaves activation headroom.
    pub resource_headroom: bool,
    /// The wall clock is within the sane range.
    pub clock_sane: bool,
    /// Supervision has not requested shutdown.
    pub supervision_healthy: bool,
}

/// Lower sane wall-clock bound in milliseconds (2021-01-01) and upper
/// bound (year 5138, matching the 32-bit SSU2 expiration horizon with
/// margin). Values outside fail `clock_sane` closed.
const MIN_SANE_WALL_MS: u64 = 1_609_459_200_000;
const MAX_SANE_WALL_MS: u64 = 99_999_999_999_999;

/// Returns true when the wall clock is within the sane range.
pub const fn is_sane_wall_ms(wall_now_ms: u64) -> bool {
    wall_now_ms >= MIN_SANE_WALL_MS && wall_now_ms <= MAX_SANE_WALL_MS
}

/// Returns true when the host is an address other routers could
/// plausibly dial: not loopback, unspecified, multicast, private,
/// link-local, or broadcast. Documentation/reserved ranges pass by
/// spelling; corroborated reachability remains the real guard, and on
/// the current loopback-only tree the loopback arm already rejects
/// every address the runtime can produce.
fn is_publicly_routable_host(host: std::net::IpAddr) -> bool {
    match host {
        std::net::IpAddr::V4(host) => {
            !(host.is_loopback()
                || host.is_unspecified()
                || host.is_multicast()
                || host.is_private()
                || host.is_link_local()
                || host.is_broadcast())
        }
        std::net::IpAddr::V6(host) => {
            !(host.is_loopback()
                || host.is_unspecified()
                || host.is_multicast()
                || host.is_unicast_link_local()
                || is_unique_local(&host))
        }
    }
}

/// Returns true for IPv6 unique-local (`fc00::/7`) addresses.
fn is_unique_local(host: &std::net::Ipv6Addr) -> bool {
    (host.segments()[0] & 0xfe00) == 0xfc00
}

/// Returns true when the publication material carries a qualified
/// SSU2 address on a publicly routable host. The runtime's material
/// is loopback-scoped on the current tree, so this is false for every
/// address the runtime can produce there — normal activation stays
/// unreachable by construction until a public bind exists.
fn is_normal_qualified_address(material: &i2pr_runtime::Ssu2PublicationMaterial) -> bool {
    if !is_qualified_ssu2_address(&material.address) {
        return false;
    }
    let Some(host) = material.address.options().get("host") else {
        return false;
    };
    let Ok(host) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    is_publicly_routable_host(host)
}

/// Evaluates normal-path eligibility from measured signals plus
/// explicit operator intent.
///
/// `controlled_qualification_permit` carries the operator opt-in: on
/// the normal path the field gates activation on authorization, and
/// the authorization is the operator's explicit request on a build
/// whose floodfill surface passed two-family qualification (Plan 279
/// closure, recorded in `specs/support.toml`). Intent alone never
/// activates — every other field is measured, and the snapshot
/// AND-gate still decides. The returned reasons name each failing
/// signal categorically for status reporting.
pub fn evaluate_normal_eligibility(
    intent: bool,
    readiness: &NormalReadiness<'_>,
) -> (FloodfillEligibilitySnapshot, Vec<NormalIneligibility>) {
    let mut reasons = Vec::new();
    if !intent {
        reasons.push(NormalIneligibility::NotRequested);
    }
    let qualified_address = readiness.material.is_some_and(is_normal_qualified_address);
    if !qualified_address {
        reasons.push(NormalIneligibility::AddressUnqualified);
    }
    let reachable = readiness.material.is_some_and(|material| {
        material.reachability == i2pr_transport::ReachabilityState::Reachable
    });
    if !reachable {
        reasons.push(NormalIneligibility::ReachabilityUnconfirmed);
    }
    if !readiness.netdb_ready {
        reasons.push(NormalIneligibility::NetdbNotReady);
    }
    if !readiness.storage_ready {
        reasons.push(NormalIneligibility::StorageNotReady);
    }
    if !readiness.maintenance_ready {
        reasons.push(NormalIneligibility::MaintenanceNotReady);
    }
    if !readiness.resource_headroom {
        reasons.push(NormalIneligibility::ResourceExhausted);
    }
    if !readiness.clock_sane {
        reasons.push(NormalIneligibility::ClockUnsane);
    }
    if !readiness.supervision_healthy {
        reasons.push(NormalIneligibility::SupervisionUnhealthy);
    }
    (
        FloodfillEligibilitySnapshot {
            controlled_qualification_permit: intent,
            qualified_ssu2_address: qualified_address,
            direct_reachability: reachable,
            netdb_ready: readiness.netdb_ready,
            storage_ready: readiness.storage_ready,
            maintenance_ready: readiness.maintenance_ready,
            resource_headroom: readiness.resource_headroom,
            clock_sane: readiness.clock_sane,
            supervision_healthy: readiness.supervision_healthy,
        },
        reasons,
    )
}

/// A prepared normal activation: the role reached Active and the
/// permit-gated floodfill RouterInfo was built and encoded, before
/// the runtime install. Splitting preparation from installation lets
/// tests prove the eligible-to-`f` decision without a live socket;
/// the live composition installs immediately after preparing.
pub struct PreparedNormalActivation {
    /// The opaque advertisement permit minted by the role controller.
    pub permit: i2pr_netdb::FloodfillAdvertisementPermit,
    /// The encoded floodfill RouterInfo bytes (`caps=f`).
    pub router_info: Vec<u8>,
    /// The publicly reachable SSU2 address the record advertises.
    pub address: i2pr_proto::RouterAddress,
}

/// Prepares a normal activation from measured readiness: intent plus
/// eligibility plus role begin/complete plus the permit-gated build.
/// Fails closed with categorical reasons; installs nothing.
pub fn prepare_normal_activation(
    coordinator: &mut FloodfillCoordinator,
    bundle: &i2pr_crypto::RouterIdentityBundle,
    intent: bool,
    readiness: &NormalReadiness<'_>,
) -> Result<PreparedNormalActivation, NormalActivationError> {
    if !intent {
        return Err(NormalActivationError::NotRequested);
    }
    let Some(material) = readiness.material else {
        return Err(NormalActivationError::EligibilityFailed(vec![
            NormalIneligibility::AddressUnqualified,
            NormalIneligibility::ReachabilityUnconfirmed,
        ]));
    };
    let (snapshot, reasons) = evaluate_normal_eligibility(intent, readiness);
    if !snapshot.eligible() {
        return Err(NormalActivationError::EligibilityFailed(reasons));
    }
    let effect = coordinator.update_eligibility(snapshot);
    let opened = effect == FloodfillRoleEffect::ReadyToActivate
        || (effect == FloodfillRoleEffect::None
            && coordinator.role_state() == FloodfillRoleState::Eligible);
    if !opened || coordinator.role_state() != FloodfillRoleState::Eligible {
        return Err(NormalActivationError::EligibilityFailed(reasons));
    }
    if !coordinator.begin_activation() || !coordinator.complete_activation() {
        coordinator.fail_activation();
        return Err(NormalActivationError::ActivationFailed);
    }
    let permit = coordinator.advertisement_permit().ok_or_else(|| {
        coordinator.fail_activation();
        NormalActivationError::ActivationFailed
    })?;
    let built = i2pr_netdb::LocalRouterInfoBuilder::new(bundle)
        .build_floodfill(
            Date::from_millis(readiness.wall_now_ms),
            controlled_router_options().map_err(|error| {
                coordinator.fail_activation();
                NormalActivationError::BuildFailed(error)
            })?,
            material.address.clone(),
            &permit,
        )
        .map_err(|error| {
            coordinator.fail_activation();
            NormalActivationError::BuildFailed(error)
        })?;
    let encoded = built
        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| {
            coordinator.fail_activation();
            NormalActivationError::BuildFailed(i2pr_netdb::LocalRouterInfoError::InvalidMapping {
                context: "encode",
            })
        })?;
    Ok(PreparedNormalActivation {
        permit,
        router_info: encoded,
        address: material.address.clone(),
    })
}

/// Inputs for [`activate_normal`].
pub struct NormalActivationParams<'a> {
    /// The live daemon SSU2 service (installs the record).
    pub handle: &'a crate::router_i2np::Ssu2DaemonHandle,
    /// The coordinator to activate.
    pub coordinator: &'a mut FloodfillCoordinator,
    /// The daemon identity (signs the floodfill RouterInfo).
    pub bundle: &'a i2pr_crypto::RouterIdentityBundle,
    /// Explicit operator intent (`[floodfill].enabled`).
    pub intent: bool,
    /// Measured readiness gathered on this tick.
    pub readiness: NormalReadiness<'a>,
    /// Cooperative cancellation.
    pub cancellation: &'a i2pr_runtime::CancellationToken,
}

/// A completed normal activation.
pub struct NormalActivation {
    /// The opaque advertisement permit minted by the role controller.
    /// Thread it into withdrawal; it cannot be forged.
    pub permit: i2pr_netdb::FloodfillAdvertisementPermit,
    /// The installed floodfill RouterInfo bytes (`caps=f`).
    pub router_info: Vec<u8>,
    /// The publicly reachable SSU2 address the record advertises.
    pub address: i2pr_proto::RouterAddress,
}

/// Runs the normal activation sequence (Plan 279 §4).
///
/// Explicit intent plus measured live readiness, then the shared
/// activation tail (eligibility update, role begin/complete,
/// permit-gated build, atomic install, local publish). Any failure
/// fails the role closed and returns the typed error; the runtime
/// keeps its previous RouterInfo.
pub async fn activate_normal(
    params: NormalActivationParams<'_>,
) -> Result<NormalActivation, NormalActivationError> {
    if params.cancellation.is_cancelled() {
        return Err(NormalActivationError::Cancelled);
    }
    if !params.intent {
        return Err(NormalActivationError::NotRequested);
    }
    let Some(material) = params.readiness.material else {
        return Err(NormalActivationError::EligibilityFailed(vec![
            NormalIneligibility::AddressUnqualified,
            NormalIneligibility::ReachabilityUnconfirmed,
        ]));
    };
    let (snapshot, reasons) = evaluate_normal_eligibility(params.intent, &params.readiness);
    if !snapshot.eligible() {
        return Err(NormalActivationError::EligibilityFailed(reasons));
    }
    let tail = activation_tail(ActivationInput {
        coordinator: params.coordinator,
        bundle: params.bundle,
        snapshot,
        address: material.address.clone(),
        // Normal path: no peer-test evidence, so no `R` (ADR 0030).
        reachability_proof: None,
        service: params.handle.service(),
        wall_now_ms: params.readiness.wall_now_ms,
    });
    let tail = match tail {
        Ok(tail) => tail,
        Err(ActivationTailError::Ineligible) => {
            return Err(NormalActivationError::EligibilityFailed(reasons));
        }
        Err(ActivationTailError::Activation) => {
            return Err(NormalActivationError::ActivationFailed);
        }
        Err(ActivationTailError::Build(error)) => {
            return Err(NormalActivationError::BuildFailed(error));
        }
        Err(ActivationTailError::Install) => {
            return Err(NormalActivationError::InstallFailed);
        }
        Err(ActivationTailError::Publish) => {
            return Err(NormalActivationError::PublishFailed);
        }
    };
    Ok(NormalActivation {
        permit: tail.permit,
        router_info: tail.router_info,
        address: material.address.clone(),
    })
}

/// A prepared normal withdrawal: the same qualified address rebuilt
/// without `caps=f`, before the runtime install. Splitting
/// preparation from installation lets tests prove the
/// withdrawal-form decision without a live socket.
pub struct PreparedNormalWithdrawal {
    /// The encoded withdrawal RouterInfo bytes (no `caps=f`).
    pub router_info: Vec<u8>,
}

/// Prepares a normal withdrawal: applies the loss snapshot (stopping
/// admission), then rebuilds the same qualified address without
/// `caps=f`. The caller must hold a valid loss snapshot (at least one
/// field false); an eligible snapshot fails closed without mutation.
pub fn prepare_normal_withdrawal(
    coordinator: &mut FloodfillCoordinator,
    bundle: &i2pr_crypto::RouterIdentityBundle,
    permit: &i2pr_netdb::FloodfillAdvertisementPermit,
    address: &i2pr_proto::RouterAddress,
    snapshot: FloodfillEligibilitySnapshot,
    wall_now_ms: u64,
) -> Result<PreparedNormalWithdrawal, NormalActivationError> {
    if snapshot.eligible() {
        return Err(NormalActivationError::InvalidConfig);
    }
    let effect = coordinator.update_eligibility(snapshot);
    if effect != FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        || coordinator.role_state() != FloodfillRoleState::Draining
    {
        return Err(NormalActivationError::WithdrawalFailed);
    }
    let built = i2pr_netdb::LocalRouterInfoBuilder::new(bundle)
        .build_floodfill_withdrawal(
            Date::from_millis(wall_now_ms),
            controlled_router_options().map_err(|_| NormalActivationError::WithdrawalFailed)?,
            address.clone(),
            permit,
        )
        .map_err(|_| NormalActivationError::WithdrawalFailed)?;
    let encoded = built
        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| NormalActivationError::WithdrawalFailed)?;
    Ok(PreparedNormalWithdrawal {
        router_info: encoded,
    })
}

/// Inputs for [`withdraw_normal`].
pub struct NormalWithdrawalParams<'a> {
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

/// Runs the normal health-withdrawal sequence (Plan 279 §4C–§4D).
///
/// Failing eligibility stops admission (Draining), then the same
/// qualified address is re-installed and re-published without
/// `caps=f`, then queued effects drain bounded to the deadline, then
/// the role reaches Disabled. Leftover effects or a refused drain
/// fail closed with the role left Draining for a retry.
pub async fn withdraw_normal(
    params: NormalWithdrawalParams<'_>,
) -> Result<(), NormalActivationError> {
    if params.cancellation.is_cancelled() {
        return Err(NormalActivationError::Cancelled);
    }
    if params.drain_timeout.is_zero() || params.drain_timeout > MAX_CONTROLLED_DRAIN {
        return Err(NormalActivationError::InvalidConfig);
    }
    if params.snapshot.eligible() {
        return Err(NormalActivationError::InvalidConfig);
    }
    withdrawal_tail(WithdrawalInput {
        handle: params.handle,
        coordinator: params.coordinator,
        bundle: params.bundle,
        permit: params.permit,
        address: params.address.clone(),
        snapshot: params.snapshot,
        wall_now_ms: params.wall_now_ms,
        drain_timeout: params.drain_timeout,
        max_record_age_ms: params.max_record_age_ms,
        cancellation: params.cancellation,
    })
    .await
    .map_err(|error| match error {
        WithdrawalTailError::StillEligible => NormalActivationError::InvalidConfig,
        WithdrawalTailError::WithdrawalFailed => NormalActivationError::WithdrawalFailed,
    })
}

/// What one service evaluation tick asks the owner to do next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillServiceStep {
    /// Nothing to do: Disabled and ineligible, or Active and healthy.
    Idle,
    /// The role just became Eligible: run activation.
    Activate,
    /// Eligibility was lost (or intent withdrawn): run withdrawal.
    Withdraw,
}

/// Operator-facing status: requested intent, effective role, and the
/// categorical reasons the role is not Active. Fixed reason
/// variants only — no peer identifiers, addresses, or key material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FloodfillServiceStatus {
    /// Whether the operator requested floodfill evaluation.
    pub requested: bool,
    /// The effective role state.
    pub role: FloodfillRoleState,
    /// Categorical reasons the role is not Active (empty when Active).
    pub reasons: Vec<NormalIneligibility>,
}

/// Long-lived normal-path floodfill service state (Plan 279 §4).
///
/// Owns the coordinator and the operator intent, evaluates measured
/// readiness on every bounded tick, and reports the requested /
/// effective / categorical-reason status. Activation and withdrawal
/// apply through the same permit-gated compositions as the
/// controlled lane; this state never constructs authority itself.
/// Restart always begins Disabled: no stale advertisement survives a
/// restart, and recovery requires a fresh eligible tick.
pub struct FloodfillServiceState {
    coordinator: FloodfillCoordinator,
    intent: bool,
    config_invalid: bool,
    active_address: Option<i2pr_proto::RouterAddress>,
    active_permit: Option<i2pr_netdb::FloodfillAdvertisementPermit>,
    reasons: Vec<NormalIneligibility>,
    pending_snapshot: Option<FloodfillEligibilitySnapshot>,
    last_queue_full_drops: u64,
}

impl FloodfillServiceState {
    /// Creates the service state around a fresh Disabled coordinator
    /// and empty serving NetDB: no stale records are served after a
    /// restart, and activation requires fresh eligibility.
    pub fn new(local_router: Hash, intent: bool) -> Result<Self, i2pr_netdb::ReplicationError> {
        Ok(Self {
            coordinator: FloodfillCoordinator::new(
                local_router,
                FloodfillCoordinatorPolicy::default(),
                FloodfillStorePolicy::default(),
                ServerNetDbConfig::default(),
                FloodfillResourcePolicy::default(),
                ReplicationPolicy::default(),
            )?,
            intent,
            config_invalid: false,
            active_address: None,
            active_permit: None,
            reasons: Vec::new(),
            pending_snapshot: None,
            last_queue_full_drops: 0,
        })
    }

    /// Returns the current operator intent.
    pub const fn intent(&self) -> bool {
        self.intent
    }

    /// Sets the operator intent (driven by config re-reads).
    pub fn set_intent(&mut self, intent: bool) {
        self.intent = intent;
    }

    /// Re-reads operator intent from configuration text. Pure parse:
    /// on success the intent updates and the call reports whether it
    /// changed; on failure the previous intent is kept untouched and
    /// the status records `ConfigInvalid` (fail-closed, no mutation).
    pub fn refresh_intent(
        &mut self,
        config_text: &str,
    ) -> Result<bool, crate::config::ConfigError> {
        match crate::config::Config::parse(config_text) {
            Ok(config) => {
                self.config_invalid = false;
                let changed = self.intent != config.floodfill.enabled;
                self.intent = config.floodfill.enabled;
                Ok(changed)
            }
            Err(error) => {
                self.config_invalid = true;
                Err(error)
            }
        }
    }

    /// Returns the operator-facing status snapshot.
    pub fn status(&self) -> FloodfillServiceStatus {
        let mut reasons = self.reasons.clone();
        if self.config_invalid && !reasons.contains(&NormalIneligibility::ConfigInvalid) {
            reasons.push(NormalIneligibility::ConfigInvalid);
        }
        FloodfillServiceStatus {
            requested: self.intent,
            role: self.coordinator.role_state(),
            reasons,
        }
    }

    /// Returns the currently installed floodfill address, if the role
    /// activated and has not yet withdrawn.
    pub fn active_address(&self) -> Option<&i2pr_proto::RouterAddress> {
        self.active_address.as_ref()
    }

    /// Evaluates one tick: applies the measured snapshot to the role
    /// controller, records categorical reasons, and reports the step
    /// the owner must run next. The snapshot is stashed for the
    /// matching apply call; a second evaluation overwrites it, so the
    /// owner must act on each tick before evaluating again.
    pub fn evaluate(&mut self, readiness: &NormalReadiness<'_>) -> FloodfillServiceStep {
        let (snapshot, reasons) = evaluate_normal_eligibility(self.intent, readiness);
        self.reasons = reasons;
        self.coordinator.update_eligibility(snapshot);
        self.pending_snapshot = Some(snapshot);
        match self.coordinator.role_state() {
            FloodfillRoleState::Eligible => FloodfillServiceStep::Activate,
            FloodfillRoleState::Draining => FloodfillServiceStep::Withdraw,
            _ => FloodfillServiceStep::Idle,
        }
    }

    /// Applies a pending activation: prepares the permit-gated build
    /// from the stashed eligible snapshot, installs it on the
    /// runtime, and publishes it locally. Returns the installed
    /// RouterInfo bytes. Fails closed without a stashed eligible
    /// snapshot. The minted permit is retained alongside the active
    /// address so the matching withdrawal can rebuild the
    /// withdrawal form; both clear when the role leaves Draining.
    /// The permit is memory-only (never serialized or logged) and its
    /// `Debug` is redacted.
    pub async fn apply_activation(
        &mut self,
        handle: &crate::router_i2np::Ssu2DaemonHandle,
        bundle: &i2pr_crypto::RouterIdentityBundle,
        readiness: &NormalReadiness<'_>,
        cancellation: &i2pr_runtime::CancellationToken,
    ) -> Result<Vec<u8>, NormalActivationError> {
        if cancellation.is_cancelled() {
            return Err(NormalActivationError::Cancelled);
        }
        let snapshot = self
            .pending_snapshot
            .take()
            .ok_or(NormalActivationError::InvalidConfig)?;
        if !snapshot.eligible() {
            return Err(NormalActivationError::InvalidConfig);
        }
        // The coordinator already holds this exact snapshot from
        // `evaluate` (same tick, same material): preparation performs
        // begin/complete against the Eligible role it opened.
        let intent = self.intent;
        let prepared = prepare_normal_activation(&mut self.coordinator, bundle, intent, readiness)
            .inspect_err(|_| {
                self.reasons = vec![NormalIneligibility::ActivationFailed];
            })?;
        if handle
            .service()
            .install_local_router_info(prepared.router_info.clone(), readiness.wall_now_ms)
            .is_err()
        {
            self.coordinator.fail_activation();
            self.reasons = vec![NormalIneligibility::ActivationFailed];
            return Err(NormalActivationError::InstallFailed);
        }
        let time = FloodfillTime {
            wall_ms: readiness.wall_now_ms,
            monotonic_ms: readiness.wall_now_ms,
        };
        if publish_local_router_info(
            &mut self.coordinator,
            &prepared.router_info,
            &prepared.permit,
            time,
        )
        .is_err()
        {
            self.coordinator.fail_activation();
            self.reasons = vec![NormalIneligibility::ActivationFailed];
            return Err(NormalActivationError::PublishFailed);
        }
        self.active_address = Some(prepared.address);
        self.active_permit = Some(prepared.permit);
        Ok(prepared.router_info)
    }

    /// Builds the withdrawal record for the active address without
    /// touching the runtime. Split out so the withdrawal-form
    /// decision is testable without a socket.
    fn build_withdrawal_record(
        bundle: &i2pr_crypto::RouterIdentityBundle,
        permit: &i2pr_netdb::FloodfillAdvertisementPermit,
        address: &i2pr_proto::RouterAddress,
        wall_now_ms: u64,
    ) -> Result<Vec<u8>, NormalActivationError> {
        let built = i2pr_netdb::LocalRouterInfoBuilder::new(bundle)
            .build_floodfill_withdrawal(
                Date::from_millis(wall_now_ms),
                controlled_router_options().map_err(|_| NormalActivationError::WithdrawalFailed)?,
                address.clone(),
                permit,
            )
            .map_err(|_| NormalActivationError::WithdrawalFailed)?;
        built
            .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
            .map_err(|_| NormalActivationError::WithdrawalFailed)
    }

    /// Applies a pending withdrawal: reinstalls the same qualified
    /// address without `caps=f`, republishes it locally, drains the
    /// bounded queue, and completes the drain. Fails closed without
    /// a stashed loss snapshot or without an installed advertisement.
    pub async fn apply_withdrawal(
        &mut self,
        handle: &crate::router_i2np::Ssu2DaemonHandle,
        bundle: &i2pr_crypto::RouterIdentityBundle,
        wall_now_ms: u64,
        drain_timeout: Duration,
        max_record_age_ms: u64,
        cancellation: &i2pr_runtime::CancellationToken,
    ) -> Result<(), NormalActivationError> {
        if cancellation.is_cancelled() {
            return Err(NormalActivationError::Cancelled);
        }
        if drain_timeout.is_zero() || drain_timeout > MAX_CONTROLLED_DRAIN {
            return Err(NormalActivationError::InvalidConfig);
        }
        let snapshot = self
            .pending_snapshot
            .take()
            .ok_or(NormalActivationError::InvalidConfig)?;
        if snapshot.eligible() {
            return Err(NormalActivationError::InvalidConfig);
        }
        if self.coordinator.role_state() != FloodfillRoleState::Draining {
            // `evaluate` already moved an Active/Eligible role to
            // Draining on this loss snapshot; anything else means the
            // caller acted on a stale step.
            return Err(NormalActivationError::InvalidConfig);
        }
        // `evaluate` already applied this exact loss snapshot (stopping
        // admission): re-applying it would move Draining to Disabled
        // and break `complete_drain`, so the reinstall below works
        // directly against the Draining role.
        let (address, permit) = match (self.active_address.clone(), self.active_permit.clone()) {
            (Some(address), Some(permit)) => (Some(address), Some(permit)),
            (None, None) => (None, None),
            _ => return Err(NormalActivationError::InvalidConfig),
        };
        if let (Some(address), Some(permit)) = (address, permit) {
            let encoded = Self::build_withdrawal_record(bundle, &permit, &address, wall_now_ms)?;
            if handle
                .service()
                .install_local_router_info(encoded.clone(), wall_now_ms)
                .is_err()
            {
                self.reasons = vec![NormalIneligibility::WithdrawalFailed];
                return Err(NormalActivationError::WithdrawalFailed);
            }
            let time = FloodfillTime {
                wall_ms: wall_now_ms,
                monotonic_ms: wall_now_ms,
            };
            if publish_local_router_info(&mut self.coordinator, &encoded, &permit, time).is_err() {
                self.reasons = vec![NormalIneligibility::WithdrawalFailed];
                return Err(NormalActivationError::WithdrawalFailed);
            }
        }
        // Without an installed advertisement there is no record to
        // replace (withdrew from Eligible): only the bounded queue
        // drains before `complete_drain`.
        let deadline = tokio::time::Instant::now() + drain_timeout;
        while let Some(effect) = self.coordinator.pop_effect() {
            if tokio::time::Instant::now() >= deadline || cancellation.is_cancelled() {
                self.reasons = vec![NormalIneligibility::WithdrawalFailed];
                return Err(NormalActivationError::WithdrawalFailed);
            }
            let outcome = deliver_floodfill_effect_with_dial(
                handle,
                self.coordinator.netdb(),
                effect,
                wall_now_ms,
                max_record_age_ms,
                cancellation,
            )
            .await;
            self.coordinator.record_delivery(outcome);
        }
        if !self.coordinator.complete_drain() {
            self.reasons = vec![NormalIneligibility::WithdrawalFailed];
            return Err(NormalActivationError::WithdrawalFailed);
        }
        self.active_address = None;
        self.active_permit = None;
        Ok(())
    }

    /// Routes one authenticated inbound I2NP handoff through the
    /// coordinator. While the role is not Active the coordinator
    /// reports `Ignored` and the caller falls through to the existing
    /// dispatcher; while Active, Store/Lookup effects are queued for
    /// the owner's bounded delivery. Never synthesizes traffic.
    pub fn handle_inbound(
        &mut self,
        inbound: &i2pr_runtime::Ssu2InboundI2np,
        time: FloodfillTime,
    ) -> Result<FloodfillDispatchOutcome, crate::router_i2np::RouterI2npError> {
        self.coordinator.handle_authenticated_i2np(inbound, time)
    }

    /// Pops one queued daemon effect for bounded delivery.
    pub fn pop_effect(&mut self) -> Option<FloodfillLeasedEffect> {
        self.coordinator.pop_effect()
    }

    /// Records one effect-drain outcome on the coordinator.
    pub fn record_delivery(&mut self, outcome: FloodfillDeliveryOutcome) {
        self.coordinator.record_delivery(outcome)
    }

    /// Returns the current role state.
    pub fn role_state(&self) -> FloodfillRoleState {
        self.coordinator.role_state()
    }

    /// Runs one bounded maintenance batch over the serving NetDB.
    pub fn maintenance_tick(
        &mut self,
        now_ms: u64,
        max_age_ms: u64,
        batch_size: usize,
    ) -> i2pr_netdb::MaintenanceBatch {
        self.coordinator
            .maintenance_tick(now_ms, max_age_ms, batch_size)
    }

    /// Returns true when the bounded queue shows no new backpressure
    /// drops since the last tick and occupancy sits below half of
    /// policy. A single transient queued effect does not cost
    /// headroom; fresh drops or a half-full queue do.
    pub fn check_headroom(&mut self) -> bool {
        let stats = self.coordinator.stats();
        let fresh = stats.queue_full == self.last_queue_full_drops;
        self.last_queue_full_drops = stats.queue_full;
        fresh
            && stats.queued_effects < self.coordinator.policy.max_queued_effects / 2
            && stats.queued_bytes < self.coordinator.policy.max_queued_bytes / 2
    }

    /// Delivers every queued effect through the bounded direct path,
    /// recording each outcome. The queue itself is policy-bounded, so
    /// this always terminates; each delivery observes cancellation.
    pub async fn drain_effects(
        &mut self,
        handle: &crate::router_i2np::Ssu2DaemonHandle,
        wall_now_ms: u64,
        max_record_age_ms: u64,
        cancellation: &i2pr_runtime::CancellationToken,
    ) {
        while let Some(effect) = self.coordinator.pop_effect() {
            let outcome = deliver_floodfill_effect_with_dial(
                handle,
                self.coordinator.netdb(),
                effect,
                wall_now_ms,
                max_record_age_ms,
                cancellation,
            )
            .await;
            self.coordinator.record_delivery(outcome);
        }
    }
}

/// Failure of the normal-path activation or withdrawal composition.
///
/// Every variant fails closed exactly like its controlled counterpart:
/// the runtime keeps its previously installed non-`f` RouterInfo and the
/// role never reports Active. Display strings name the normal path so
/// operator logs never confuse the two compositions.
#[derive(Debug, Eq, PartialEq)]
pub enum NormalActivationError {
    /// The caller cancelled before any state mutated.
    Cancelled,
    /// The operator did not opt in; nothing was attempted.
    NotRequested,
    /// A bound failed validation, or no evaluation tick preceded apply.
    InvalidConfig,
    /// Live publication material was unavailable.
    PublicationFailed(i2pr_runtime::Ssu2PublicationUnavailable),
    /// The measured snapshot did not open activation; carries the
    /// categorical reasons for status reporting.
    EligibilityFailed(Vec<NormalIneligibility>),
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

impl std::fmt::Display for NormalActivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "normal activation cancelled",
            Self::NotRequested => "normal floodfill was not requested",
            Self::InvalidConfig => "normal activation bound is invalid",
            Self::PublicationFailed(_) => "normal publication material unavailable",
            Self::EligibilityFailed(_) => "normal eligibility did not open activation",
            Self::ActivationFailed => "normal role activation failed",
            Self::BuildFailed(_) => "normal RouterInfo build failed",
            Self::InstallFailed => "normal RouterInfo install failed",
            Self::PublishFailed => "normal local publish failed",
            Self::WithdrawalFailed => "normal withdrawal failed",
        })
    }
}

impl std::error::Error for NormalActivationError {}

/// Shared tail of both activation compositions: eligibility update,
/// role begin/complete, permit-gated floodfill build, atomic install,
/// local publish (with the withdrawal-form self-heal on publish
/// failure so a failed activation can never leave a live
/// advertisement behind).
struct ActivationTail {
    permit: i2pr_netdb::FloodfillAdvertisementPermit,
    router_info: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
enum ActivationTailError {
    Ineligible,
    Activation,
    Build(i2pr_netdb::LocalRouterInfoError),
    Install,
    Publish,
}

struct ActivationInput<'a> {
    coordinator: &'a mut FloodfillCoordinator,
    bundle: &'a i2pr_crypto::RouterIdentityBundle,
    snapshot: FloodfillEligibilitySnapshot,
    address: i2pr_proto::RouterAddress,
    /// Peer-test-confirmed reachability proof (Plan 306, ADR 0030).
    /// `Some` on the controlled path (where `activate_controlled`
    /// minted it beside a `Confirmed` outcome), `None` on the normal
    /// path, which stays `caps=f`. The option is the evidence switch:
    /// `R` is supplied only when the decided evidence holds.
    reachability_proof: Option<i2pr_netdb::LoopbackReachabilityProof>,
    service: &'a i2pr_runtime::Ssu2RuntimeService,
    wall_now_ms: u64,
}

fn activation_tail(input: ActivationInput<'_>) -> Result<ActivationTail, ActivationTailError> {
    let effect = input.coordinator.update_eligibility(input.snapshot);
    // A fresh eligible tick opens Disabled/Failed via ReadyToActivate;
    // a tick that already opened Eligible (the service evaluated first
    // so status reasons exist before the apply call) re-asserts None
    // while staying Eligible. Both may proceed; any other combination
    // (including Draining, which must drain before re-activating)
    // fails closed.
    let opened = effect == FloodfillRoleEffect::ReadyToActivate
        || (effect == FloodfillRoleEffect::None
            && input.coordinator.role_state() == FloodfillRoleState::Eligible);
    if !opened || input.coordinator.role_state() != FloodfillRoleState::Eligible {
        return Err(ActivationTailError::Ineligible);
    }
    if !input.coordinator.begin_activation() || !input.coordinator.complete_activation() {
        input.coordinator.fail_activation();
        return Err(ActivationTailError::Activation);
    }
    let permit = input.coordinator.advertisement_permit().ok_or_else(|| {
        input.coordinator.fail_activation();
        ActivationTailError::Activation
    })?;
    let built = match input.reachability_proof.as_ref() {
        // Controlled path with peer-test-confirmed reachability:
        // advertise `fR` (Plan 306, ADR 0030).
        Some(proof) => i2pr_netdb::LocalRouterInfoBuilder::new(input.bundle)
            .build_floodfill_reachable(
                Date::from_millis(input.wall_now_ms),
                controlled_router_options().map_err(|error| {
                    input.coordinator.fail_activation();
                    ActivationTailError::Build(error)
                })?,
                input.address.clone(),
                &permit,
                proof,
            )
            .map_err(|error| {
                input.coordinator.fail_activation();
                ActivationTailError::Build(error)
            })?,
        // Normal path (or any future path without the decided
        // evidence): `caps=f`, unchanged.
        None => i2pr_netdb::LocalRouterInfoBuilder::new(input.bundle)
            .build_floodfill(
                Date::from_millis(input.wall_now_ms),
                controlled_router_options().map_err(|error| {
                    input.coordinator.fail_activation();
                    ActivationTailError::Build(error)
                })?,
                input.address.clone(),
                &permit,
            )
            .map_err(|error| {
                input.coordinator.fail_activation();
                ActivationTailError::Build(error)
            })?,
    };
    let encoded = built
        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| {
            input.coordinator.fail_activation();
            ActivationTailError::Build(i2pr_netdb::LocalRouterInfoError::InvalidMapping {
                context: "encode",
            })
        })?;
    if input
        .service
        .install_local_router_info(encoded.clone(), input.wall_now_ms)
        .is_err()
    {
        input.coordinator.fail_activation();
        return Err(ActivationTailError::Install);
    }
    let time = FloodfillTime {
        wall_ms: input.wall_now_ms,
        monotonic_ms: input.wall_now_ms,
    };
    if publish_local_router_info(input.coordinator, &encoded, &permit, time).is_err() {
        // The fR-RI is already installed on the runtime: install the
        // same-address non-f record (the permit-gated withdrawal form)
        // so a publish failure cannot leave a live advertisement behind
        // (Plan 283 invariant 6). Best-effort: shutdown racing it owns
        // the sockets anyway. Nothing is published to the coordinator
        // NetDB; the failed activation publishes nothing.
        if let Ok(withdrawn) = controlled_router_options().and_then(|options| {
            i2pr_netdb::LocalRouterInfoBuilder::new(input.bundle)
                .build_floodfill_withdrawal(
                    Date::from_millis(input.wall_now_ms),
                    options,
                    input.address.clone(),
                    &permit,
                )
                .and_then(|built| {
                    built
                        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
                        .map_err(|_| i2pr_netdb::LocalRouterInfoError::InvalidMapping {
                            context: "encode",
                        })
                })
        }) {
            let _ = input
                .service
                .install_local_router_info(withdrawn, input.wall_now_ms);
        }
        input.coordinator.fail_activation();
        return Err(ActivationTailError::Publish);
    }
    Ok(ActivationTail {
        permit,
        router_info: encoded,
    })
}

/// Shared tail of both withdrawal compositions: eligibility loss stops
/// admission (Draining), the same qualified address is re-installed
/// and re-published without `caps=f`, queued effects drain bounded to
/// the deadline, then the role reaches Disabled. Leftover effects or
/// a refused drain fail closed with the role left Draining for retry.
#[derive(Debug, Eq, PartialEq)]
enum WithdrawalTailError {
    StillEligible,
    WithdrawalFailed,
}

struct WithdrawalInput<'a> {
    handle: &'a crate::router_i2np::Ssu2DaemonHandle,
    coordinator: &'a mut FloodfillCoordinator,
    bundle: &'a i2pr_crypto::RouterIdentityBundle,
    permit: &'a i2pr_netdb::FloodfillAdvertisementPermit,
    address: i2pr_proto::RouterAddress,
    snapshot: FloodfillEligibilitySnapshot,
    wall_now_ms: u64,
    drain_timeout: Duration,
    max_record_age_ms: u64,
    cancellation: &'a i2pr_runtime::CancellationToken,
}

async fn withdrawal_tail(input: WithdrawalInput<'_>) -> Result<(), WithdrawalTailError> {
    if input.snapshot.eligible() {
        return Err(WithdrawalTailError::StillEligible);
    }
    let effect = input.coordinator.update_eligibility(input.snapshot);
    if effect != FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        || input.coordinator.role_state() != FloodfillRoleState::Draining
    {
        return Err(WithdrawalTailError::WithdrawalFailed);
    }
    let built = i2pr_netdb::LocalRouterInfoBuilder::new(input.bundle)
        .build_floodfill_withdrawal(
            Date::from_millis(input.wall_now_ms),
            controlled_router_options().map_err(|_| WithdrawalTailError::WithdrawalFailed)?,
            input.address.clone(),
            input.permit,
        )
        .map_err(|_| WithdrawalTailError::WithdrawalFailed)?;
    let encoded = built
        .encoded(MAX_CONTROLLED_ROUTER_INFO_BYTES)
        .map_err(|_| WithdrawalTailError::WithdrawalFailed)?;
    if input
        .handle
        .service()
        .install_local_router_info(encoded.clone(), input.wall_now_ms)
        .is_err()
    {
        return Err(WithdrawalTailError::WithdrawalFailed);
    }
    let time = FloodfillTime {
        wall_ms: input.wall_now_ms,
        monotonic_ms: input.wall_now_ms,
    };
    if publish_local_router_info(input.coordinator, &encoded, input.permit, time).is_err() {
        return Err(WithdrawalTailError::WithdrawalFailed);
    }
    let deadline = tokio::time::Instant::now() + input.drain_timeout;
    while let Some(effect) = input.coordinator.pop_effect() {
        if tokio::time::Instant::now() >= deadline || input.cancellation.is_cancelled() {
            return Err(WithdrawalTailError::WithdrawalFailed);
        }
        let outcome = deliver_floodfill_effect_with_dial(
            input.handle,
            input.coordinator.netdb(),
            effect,
            input.wall_now_ms,
            input.max_record_age_ms,
            input.cancellation,
        )
        .await;
        input.coordinator.record_delivery(outcome);
    }
    if input.coordinator.stats().queued_effects > 0 {
        return Err(WithdrawalTailError::WithdrawalFailed);
    }
    if !input.coordinator.complete_drain() {
        return Err(WithdrawalTailError::WithdrawalFailed);
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
        // Plan 302 pin: the outer envelope must be the 9-byte short-transport form the
        // SSU2 session layer reads, never the 16-byte standard header whose millisecond
        // expiration the reference misreads as seconds and drops as expired.
        assert_short_transport_form(&bytes, 1000);
        let outer = I2npMessage::decode_short_transport(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
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
        assert_short_transport_form(&bytes, 1000);
        let outer = I2npMessage::decode_short_transport(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
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
        assert_short_transport_form(&bytes, 1000);
        let message = I2npMessage::decode_short_transport(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
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
        assert_short_transport_form(&bytes, 1000);
        let message = I2npMessage::decode_short_transport(&bytes, MAX_FLOODFILL_EFFECT_BYTES)
            .expect("direct replication");
        let I2npBody::DatabaseStore(store) = message.into_body() else {
            panic!("replication must be a direct DatabaseStore")
        };
        assert_eq!(store.reply_token, 0);
        assert_eq!(store.reply_tunnel_id, None);
        assert_eq!(store.reply_gateway, None);
    }

    /// Plan 302 pin: asserts `bytes` carry the 9-byte short-transport header with a
    /// sane seconds expiration derived from `now_ms`, and reject the 16-byte
    /// standard parse. The pre-fix encoder fails this pin: its millisecond
    /// expiration makes the session-layer seconds read land in 1970.
    fn assert_short_transport_form(bytes: &[u8], now_ms: u64) {
        assert!(
            I2npMessage::decode_standard(bytes, MAX_FLOODFILL_EFFECT_BYTES).is_err(),
            "floodfill reply must not parse as a standard-header message"
        );
        let message = I2npMessage::decode_short_transport(bytes, MAX_FLOODFILL_EFFECT_BYTES)
            .expect("short-transport reply");
        let i2pr_proto::I2npHeader::ShortTransport {
            expiration_seconds, ..
        } = message.header()
        else {
            panic!("short-transport header expected");
        };
        let now_seconds = u32::try_from(now_ms / 1000).expect("test clock fits u32 seconds");
        assert!(
            expiration_seconds >= now_seconds
                && expiration_seconds <= now_seconds.saturating_add(60),
            "expiration {expiration_seconds:?} must sit within a bounded horizon of now {now_seconds}"
        );
        // The session layer reads exactly these offsets (`session.rs`
        // `queue_i2np_message`): raw[5..9] as big-endian seconds.
        assert_eq!(
            u32::from_be_bytes(bytes[5..9].try_into().expect("header present")),
            expiration_seconds,
            "session-layer seconds must match the decoded header"
        );
    }

    #[test]
    fn transport_encoding_fails_closed_on_expiration_overflow() {
        let body = I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
            1,
            Date::from_millis(1),
        ));
        // Millisecond clock past the u32 seconds range: the division still
        // overflows u32, so no envelope may be produced.
        let beyond_u32_seconds = (u64::from(u32::MAX) + 1) * 1000;
        assert!(encode_transport(body, beyond_u32_seconds).is_none());
        assert!(
            encode_transport(
                I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
                    1,
                    Date::from_millis(1),
                )),
                u64::MAX
            )
            .is_none()
        );
    }

    #[test]
    fn reference_short_header_layout_matches_session_parse_offsets() {
        // Plan 302 cross-check, built without our encoder: the exact-pinned
        // reference (`I2NPProtocol.h` short-header offsets, consumed by
        // `SSU2Session::HandleI2NPMsg`) lays out `[type(1)][message id(4 BE)]
        // [expiration seconds(4 BE)][body]`. The session layer parses exactly
        // raw[5..9] as big-endian seconds; this locks that contract.
        let message_id: u32 = 0x4045_4060;
        let expiration_seconds: u32 = 1_760_000_000;
        let body = [0x11, 0x22, 0x33];
        let mut wire = Vec::with_capacity(9 + body.len());
        wire.push(i2pr_proto::MessageType::DeliveryStatus.code());
        wire.extend_from_slice(&message_id.to_be_bytes());
        wire.extend_from_slice(&expiration_seconds.to_be_bytes());
        wire.extend_from_slice(&body);
        assert_eq!(wire.len(), 12);
        assert_eq!(
            u32::from_be_bytes(wire[5..9].try_into().expect("layout")),
            expiration_seconds
        );
        assert_eq!(
            u32::from_be_bytes(wire[1..5].try_into().expect("layout")),
            message_id
        );
        // The same offsets must NOT read as a sane standard header: bytes
        // 5..13 mix seconds with body, so the standard parse rejects.
        assert!(I2npMessage::decode_standard(&wire, MAX_FLOODFILL_EFFECT_BYTES).is_err());
    }

    // ------------------------------------------ Plan 279 normal path ----

    use i2pr_crypto::RouterIdentityBundle;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    const TEST_WALL_MS: u64 = 1_769_000_000_000;

    fn test_bundle() -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(0x2790_2790_2790_2790);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn i2p_b64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let value = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[((value >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((value >> 12) & 0x3f) as usize] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[((value >> 6) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[(value & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    /// Builds publication material shaped like the runtime's output for
    /// the given host. The keys are fixed nonzero test bytes; no
    /// network is involved.
    fn test_material(host: &str, reachable: bool) -> i2pr_runtime::Ssu2PublicationMaterial {
        let caps = if host.contains(':') { "6" } else { "4" };
        let options = i2pr_proto::Mapping::from_entries(vec![
            ("host".to_string(), host.to_string()),
            ("port".to_string(), "44831".to_string()),
            ("mtu".to_string(), "1472".to_string()),
            ("v".to_string(), "2".to_string()),
            ("caps".to_string(), caps.to_string()),
            ("s".to_string(), i2p_b64(&[7; 32])),
            ("i".to_string(), i2p_b64(&[9; 32])),
        ])
        .expect("test address options");
        let address = i2pr_proto::RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("test router address");
        i2pr_runtime::Ssu2PublicationMaterial {
            address,
            evidence_expires_at: Duration::from_secs(3600),
            reachability: if reachable {
                i2pr_transport::ReachabilityState::Reachable
            } else {
                i2pr_transport::ReachabilityState::Unknown
            },
            bound_family: if host.contains(':') {
                i2pr_transport::AddressFamily::Ipv6
            } else {
                i2pr_transport::AddressFamily::Ipv4
            },
        }
    }

    fn ready(material: Option<&i2pr_runtime::Ssu2PublicationMaterial>) -> NormalReadiness<'_> {
        NormalReadiness {
            material,
            wall_now_ms: TEST_WALL_MS,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: is_sane_wall_ms(TEST_WALL_MS),
            supervision_healthy: true,
        }
    }

    #[test]
    fn normal_default_intent_off_never_activates() {
        // Plan 279 §10: default-off. Even with a publicly reachable
        // address and every measured signal true, no intent means no
        // eligibility, no role movement, no permit.
        let material = test_material("198.51.100.7", true);
        let readiness = ready(Some(&material));
        let (snapshot, reasons) = evaluate_normal_eligibility(false, &readiness);
        assert!(!snapshot.eligible());
        assert_eq!(reasons, vec![NormalIneligibility::NotRequested]);
        let mut state =
            FloodfillServiceState::new(Hash::from_bytes([3; 32]), false).expect("service state");
        assert_eq!(state.evaluate(&readiness), FloodfillServiceStep::Idle);
        assert_eq!(state.role_state(), FloodfillRoleState::Disabled);
        let status = state.status();
        assert!(!status.requested);
        assert_eq!(status.role, FloodfillRoleState::Disabled);
        assert_eq!(status.reasons, vec![NormalIneligibility::NotRequested]);
        // Preparation without intent fails before touching the role.
        let mut coordinator = coordinator();
        assert!(matches!(
            prepare_normal_activation(&mut coordinator, &test_bundle(), false, &readiness),
            Err(NormalActivationError::NotRequested)
        ));
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);
    }

    #[test]
    fn normal_loopback_material_stays_disabled_without_f() {
        // Plan 279 §10: explicit opt-in but ineligible. A loopback
        // address can never be publicly reachable, so the role stays
        // Disabled with categorical reasons and no permit exists.
        let material = test_material("127.0.0.1", true);
        let readiness = ready(Some(&material));
        let (snapshot, reasons) = evaluate_normal_eligibility(true, &readiness);
        assert!(!snapshot.eligible());
        assert!(reasons.contains(&NormalIneligibility::AddressUnqualified));
        assert!(!reasons.contains(&NormalIneligibility::NotRequested));
        let mut state =
            FloodfillServiceState::new(Hash::from_bytes([3; 32]), true).expect("service state");
        assert_eq!(state.evaluate(&readiness), FloodfillServiceStep::Idle);
        assert_eq!(state.role_state(), FloodfillRoleState::Disabled);
        let mut coordinator = coordinator();
        assert!(matches!(
            prepare_normal_activation(&mut coordinator, &test_bundle(), true, &readiness),
            Err(NormalActivationError::EligibilityFailed(_))
        ));
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);
    }

    #[test]
    fn normal_unconfirmed_reachability_stays_disabled() {
        // A publicly routable address without corroborated
        // reachability still cannot activate.
        let material = test_material("198.51.100.7", false);
        let readiness = ready(Some(&material));
        let (snapshot, reasons) = evaluate_normal_eligibility(true, &readiness);
        assert!(!snapshot.eligible());
        assert!(!reasons.contains(&NormalIneligibility::AddressUnqualified));
        assert!(reasons.contains(&NormalIneligibility::ReachabilityUnconfirmed));
    }

    #[test]
    fn normal_each_lost_signal_names_its_reason() {
        // Every measured field maps to exactly one categorical reason;
        // the snapshot AND-gate stays closed while any is set.
        let material = test_material("198.51.100.7", true);
        for reason in [
            NormalIneligibility::NetdbNotReady,
            NormalIneligibility::StorageNotReady,
            NormalIneligibility::MaintenanceNotReady,
            NormalIneligibility::ResourceExhausted,
            NormalIneligibility::ClockUnsane,
            NormalIneligibility::SupervisionUnhealthy,
        ] {
            let mut readiness = ready(Some(&material));
            match reason {
                NormalIneligibility::NetdbNotReady => readiness.netdb_ready = false,
                NormalIneligibility::StorageNotReady => readiness.storage_ready = false,
                NormalIneligibility::MaintenanceNotReady => readiness.maintenance_ready = false,
                NormalIneligibility::ResourceExhausted => readiness.resource_headroom = false,
                NormalIneligibility::ClockUnsane => readiness.clock_sane = false,
                NormalIneligibility::SupervisionUnhealthy => readiness.supervision_healthy = false,
                _ => unreachable!("table covers only measured signals"),
            }
            let (snapshot, reasons) = evaluate_normal_eligibility(true, &readiness);
            assert!(!snapshot.eligible());
            assert_eq!(reasons, vec![reason]);
        }
        // No material at all names both the address and reachability gaps.
        let readiness = ready(None);
        let (snapshot, reasons) = evaluate_normal_eligibility(true, &readiness);
        assert!(!snapshot.eligible());
        assert_eq!(
            reasons,
            vec![
                NormalIneligibility::AddressUnqualified,
                NormalIneligibility::ReachabilityUnconfirmed,
            ]
        );
    }

    #[test]
    fn normal_eligible_prepares_caps_f_record() {
        // Plan 279 §10: eligible intent prepares a signed `caps=f`
        // record through the real permit-gated builder — no socket.
        let material = test_material("198.51.100.7", true);
        let readiness = ready(Some(&material));
        let (snapshot, reasons) = evaluate_normal_eligibility(true, &readiness);
        assert!(snapshot.eligible());
        assert!(reasons.is_empty());
        let mut state =
            FloodfillServiceState::new(Hash::from_bytes([3; 32]), true).expect("service state");
        assert_eq!(state.evaluate(&readiness), FloodfillServiceStep::Activate);
        assert_eq!(state.role_state(), FloodfillRoleState::Eligible);
        let bundle = test_bundle();
        let mut coordinator = coordinator();
        let prepared = prepare_normal_activation(&mut coordinator, &bundle, true, &readiness)
            .expect("prepare");
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Active);
        let decoded = RouterInfo::decode(&prepared.router_info, MAX_CONTROLLED_ROUTER_INFO_BYTES)
            .expect("decode prepared RouterInfo");
        let caps = decoded
            .capabilities()
            .expect("capabilities")
            .expect("RouterInfo carries capabilities");
        assert!(
            caps.as_str().contains('f'),
            "eligible preparation must advertise f, got {}",
            caps.as_str()
        );
        assert_eq!(prepared.address.options().get("host"), Some("198.51.100.7"));
    }

    #[test]
    fn normal_withdrawal_prepare_removes_f_and_keeps_address() {
        // Plan 279 §10: health loss withdraws `f` while keeping the
        // same qualified address. Mirrors the controlled drain
        // semantics without a socket.
        let material = test_material("198.51.100.7", true);
        let readiness = ready(Some(&material));
        let bundle = test_bundle();
        let mut coordinator = coordinator();
        let prepared = prepare_normal_activation(&mut coordinator, &bundle, true, &readiness)
            .expect("prepare");
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Active);
        // Health loss: reachability evidence lapses.
        let lapsed = test_material("198.51.100.7", false);
        let loss = ready(Some(&lapsed));
        let (snapshot, _) = evaluate_normal_eligibility(true, &loss);
        assert!(!snapshot.eligible());
        let withdrawn = prepare_normal_withdrawal(
            &mut coordinator,
            &bundle,
            &prepared.permit,
            &prepared.address,
            snapshot,
            TEST_WALL_MS,
        )
        .expect("withdraw");
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Draining);
        let decoded = RouterInfo::decode(&withdrawn.router_info, MAX_CONTROLLED_ROUTER_INFO_BYTES)
            .expect("decode withdrawal RouterInfo");
        let caps = decoded.capabilities().expect("capabilities");
        assert!(
            caps.as_ref()
                .is_none_or(|caps| !caps.as_str().contains('f')),
            "withdrawal must not advertise f"
        );
        assert_eq!(
            decoded.addresses()[0].options().get("host"),
            Some("198.51.100.7")
        );
        // An eligible snapshot is rejected without mutation.
        let (snapshot, _) = evaluate_normal_eligibility(true, &readiness);
        assert!(matches!(
            prepare_normal_withdrawal(
                &mut coordinator,
                &bundle,
                &prepared.permit,
                &prepared.address,
                snapshot,
                TEST_WALL_MS,
            ),
            Err(NormalActivationError::InvalidConfig)
        ));
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Draining);
    }

    #[test]
    fn normal_recovery_requires_fresh_eligibility_not_stale_state() {
        // Plan 279 §10: after a drain completes, the role is Disabled
        // and only a fresh eligible tick re-opens activation. Stale
        // Active state is never restored.
        let material = test_material("198.51.100.7", true);
        let readiness = ready(Some(&material));
        let bundle = test_bundle();
        let mut coordinator = coordinator();
        let prepared = prepare_normal_activation(&mut coordinator, &bundle, true, &readiness)
            .expect("prepare");
        let (loss_snapshot, _) =
            evaluate_normal_eligibility(true, &ready(Some(&test_material("198.51.100.7", false))));
        let _ = prepare_normal_withdrawal(
            &mut coordinator,
            &bundle,
            &prepared.permit,
            &prepared.address,
            loss_snapshot,
            TEST_WALL_MS,
        )
        .expect("withdraw");
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Draining);
        assert!(coordinator.complete_drain());
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);
        assert!(coordinator.advertisement_permit().is_none());
        // Fresh eligibility re-opens activation from Disabled.
        let (snapshot, _) = evaluate_normal_eligibility(true, &readiness);
        assert_eq!(
            coordinator.update_eligibility(snapshot),
            FloodfillRoleEffect::ReadyToActivate
        );
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Eligible);
        assert!(coordinator.begin_activation());
        assert!(coordinator.complete_activation());
        assert_eq!(coordinator.role_state(), FloodfillRoleState::Active);
    }

    #[test]
    fn normal_status_reports_requested_effective_and_reasons() {
        // Plan 279 §4F: requested intent, effective role, categorical
        // reasons. Reason labels are stable and carry no peer data.
        let material = test_material("127.0.0.1", true);
        let readiness = ready(Some(&material));
        let mut state =
            FloodfillServiceState::new(Hash::from_bytes([3; 32]), true).expect("service state");
        assert_eq!(state.evaluate(&readiness), FloodfillServiceStep::Idle);
        let status = state.status();
        assert!(status.requested);
        assert_eq!(status.role, FloodfillRoleState::Disabled);
        assert_eq!(
            status.reasons,
            vec![NormalIneligibility::AddressUnqualified]
        );
        assert_eq!(
            NormalIneligibility::AddressUnqualified.label(),
            "address-unqualified"
        );
    }

    #[test]
    fn normal_refresh_intent_applies_valid_change_and_freezes_on_invalid() {
        // Plan 279 §4D/§4E: a valid true->false re-read applies
        // immediately (next tick withdraws); an invalid re-read keeps
        // the previous intent and records ConfigInvalid without
        // mutating anything else.
        let mut state =
            FloodfillServiceState::new(Hash::from_bytes([3; 32]), true).expect("service state");
        let valid_off = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n[floodfill]\nenabled = false\n";
        assert!(state.refresh_intent(valid_off).expect("valid re-read"));
        assert!(!state.intent());
        assert!(!state.refresh_intent(valid_off).expect("unchanged re-read"));
        let invalid = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[floodfill]\nenabled = false\nauto = true\n";
        assert!(state.refresh_intent(invalid).is_err());
        assert!(!state.intent());
        assert!(
            state
                .status()
                .reasons
                .contains(&NormalIneligibility::ConfigInvalid)
        );
        // A valid re-read clears the flag.
        assert!(!state.refresh_intent(valid_off).expect("recovery re-read"));
        assert!(
            !state
                .status()
                .reasons
                .contains(&NormalIneligibility::ConfigInvalid)
        );
    }
}
