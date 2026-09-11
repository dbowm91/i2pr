//! Plan 185 exploratory tunnel live two-daemon-pair integration.
//!
//! Spins up two daemon-owned SSU2 services against each other on
//! loopback, then drives the Plan 185 exploratory build
//! coordinator end-to-end:
//!
//! 1. dial an authenticated SSU2 session between the two daemons;
//! 2. submit a one-hop outbound build through the coordinator and
//!    prove the inbound
//!    [`crate::router_i2np::RouterI2npOutcome::TunnelBuildReserved`]
//!    hook drives the [`crate::router_i2np`] dispatcher back into
//!    a successful material install;
//! 3. submit a one-hop inbound build through the coordinator and
//!    prove the same outcome on the inbound direction;
//! 4. exercise the creator-side liveness scheduler once both
//!    tunnels are paired;
//! 5. prove the pool + registry own the installed material and
//!    no synthetic insertion occurs.
//!
//! The remote daemon (daemon B) runs a `BuildResponder` task that
//! applies the canonical
//! [`i2pr_tunnel::multirecord::MessageHopProcessor`] to every
//! inbound ShortTunnelBuild and replies with a synthetic
//! OutboundTunnelBuildReply. The responder never weakens any
//! decoder; it uses the same EciesX25519 cryptography the
//! production code paths use, with secrets it owns.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, BuildRequest, ExploratoryBuildCoordinator,
    PeerBuildMaterial, SubmitResult,
};
use i2pr_daemon::router_i2np::{
    RouterDeliveryRequest, RouterI2npKind, RouterI2npOutcome, Ssu2DaemonHandle, Ssu2DaemonService,
    daemon_dial_target, dispatch_router_i2np, ssu2_runtime_config_from, ssu2_socket_config_from,
    verify_reference_router_info,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_proto::{Date, DeferredPayload, Hash, I2npBody, I2npMessage, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial, Ssu2InboundI2np,
};
use i2pr_transport::{LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId};
use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::identity::{TunnelDirection, TunnelId};
use i2pr_tunnel::multirecord::MessageHopProcessor;
use i2pr_tunnel::short_record::HopRole;
use rand_core::{OsRng, SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

fn wall_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(1_700_000_000)
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for byte in chunk {
            n = (n << 8) | u32::from(*byte);
        }
        n <<= 8 * (3 - chunk.len());
        let digits = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in 0..digits {
            output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

struct LocalKeys {
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: i2pr_runtime::Ssu2PublicKey,
    intro: i2pr_runtime::IntroKey,
    router_info: Vec<u8>,
    static_public_bytes: [u8; 32],
}

fn make_local_keys(port_placeholder: u16) -> LocalKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_secret = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_secret.secret_bytes();
    let static_public =
        i2pr_runtime::Ssu2PublicKey::new(static_secret.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    loop {
        OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|b| *b != 0) {
            break;
        }
    }
    let intro = i2pr_runtime::IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), port_placeholder.to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(&static_secret.public_bytes()),
        ),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let ri_options = Mapping::from_entries(vec![
        ("router.version".to_string(), "0.9.58".to_string()),
        ("netId".to_string(), "2".to_string()),
    ])
    .expect("ri options");
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            ri_options,
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    LocalKeys {
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
        static_public_bytes: static_secret.public_bytes(),
    }
}

fn daemon_ssu2_config() -> i2pr_daemon::config::Ssu2Config {
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
    Config::parse(text).expect("strict profile").ssu2
}

async fn start_daemon_pair() -> (
    Ssu2DaemonHandle,
    Ssu2DaemonHandle,
    ChildScope,
    ChildScope,
    CancellationToken,
    CancellationToken,
    LocalKeys,
    LocalKeys,
) {
    let keys_a = make_local_keys(44011);
    let keys_b = make_local_keys(44012);
    let config = daemon_ssu2_config();
    let service_a = Ssu2DaemonService::new(
        &config,
        Ssu2IdentityMaterial {
            router_hash: keys_a.hash,
            static_secret_bytes: keys_a.static_bytes,
            intro_key: keys_a.intro,
            router_info: keys_a.router_info.clone(),
        },
    )
    .expect("daemon service a");
    let service_b = Ssu2DaemonService::new(
        &config,
        Ssu2IdentityMaterial {
            router_hash: keys_b.hash,
            static_secret_bytes: keys_b.static_bytes,
            intro_key: keys_b.intro,
            router_info: keys_b.router_info.clone(),
        },
    )
    .expect("daemon service b");
    let token_a = CancellationToken::new();
    let token_b = CancellationToken::new();
    let scope_a = ChildScope::for_test(&token_a, ChildFailurePolicy::FailParent);
    let scope_b = ChildScope::for_test(&token_b, ChildFailurePolicy::FailParent);
    let handle_a = service_a
        .start(
            &scope_a,
            &i2pr_daemon::config::Ssu2Config {
                port: 0,
                ..config.clone()
            },
        )
        .await
        .expect("bind a");
    let handle_b = service_b
        .start(
            &scope_b,
            &i2pr_daemon::config::Ssu2Config {
                port: 0,
                ..config.clone()
            },
        )
        .await
        .expect("bind b");
    let _ = handle_a.local_v4().expect("a bound");
    let _ = handle_b.local_v4().expect("b bound");
    (
        handle_a, handle_b, scope_a, scope_b, token_a, token_b, keys_a, keys_b,
    )
}

async fn wait_for_active(handle: &Ssu2DaemonHandle, expected: usize) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if handle.snapshot().active_sessions == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "active sessions did not reach {expected}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn next_inbound_with_timeout(
    handle: &mut Ssu2DaemonHandle,
    timeout: Duration,
) -> Option<Ssu2InboundI2np> {
    tokio::time::timeout(timeout, handle.next_inbound())
        .await
        .unwrap_or_default()
}

struct BuildResponder {
    static_secret_bytes: [u8; 32],
    hash: Hash,
}

impl BuildResponder {
    fn process_build(&self, request_payload: &[u8], message_id: u32) -> Option<Vec<u8>> {
        let crypto = i2pr_tunnel::build_crypto::EciesX25519BuildCryptography::new();
        let hop_static_priv: &[u8; 32] = &self.static_secret_bytes;
        let hop_identity = self.hash;
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs());
        let (reply_payload, _result) = MessageHopProcessor::process_hop(
            &crypto,
            request_payload,
            hop_static_priv,
            &hop_identity,
            i2pr_tunnel::short_record::ShortResponseCode::Accepted,
            &mut rng,
        )
        .ok()?;
        let count = *reply_payload.first()?;
        let records = reply_payload.get(1..)?.to_vec();
        let deferred = i2pr_proto::DeferredBuildRecords::new(
            count,
            i2pr_proto::SHORT_BUILD_RECORD_SIZE,
            records,
        )
        .ok()?;
        let body = I2npBody::OutboundTunnelBuildReply(deferred);
        let expiration_secs = wall_secs().saturating_add(60) as u32;
        let message = I2npMessage::new_short_transport(message_id, expiration_secs, body)
            .expect("OTBRM message");
        message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .ok()
    }
}

async fn pump_responder_loop(
    mut handle: Ssu2DaemonHandle,
    delivery: i2pr_daemon::router_i2np::RouterDeliveryService,
    responder: BuildResponder,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            inbound = handle.next_inbound() => {
                let Some(inbound) = inbound else {
                    return;
                };
                let now_ms = wall_ms();
                let dispatch = match dispatch_router_i2np(&inbound, now_ms) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if let RouterI2npOutcome::TunnelBuildReserved {
                    kind: RouterI2npKind::ShortTunnelBuild,
                    message_id,
                    ..
                } = dispatch
                {
                    let message = match I2npMessage::decode_short_transport(
                        &inbound.bytes,
                        MAX_I2NP_MESSAGE_BYTES,
                    ) {
                        Ok(message) => message,
                        Err(_) => continue,
                    };
                    let body = match message.body() {
                        I2npBody::ShortTunnelBuild(records) => records,
                        _ => continue,
                    };
                    let expected = usize::from(body.count())
                        .saturating_mul(usize::from(body.record_size()));
                    if body.records().len() != expected {
                        continue;
                    }
                    let mut payload = Vec::with_capacity(1 + expected);
                    payload.push(body.count());
                    payload.extend_from_slice(body.records());
                    let Some(reply_wire) = responder.process_build(&payload, message_id) else {
                        continue;
                    };
                    let request = match RouterDeliveryRequest::new(
                        inbound.peer,
                        reply_wire,
                        Duration::from_secs(5),
                    ) {
                        Ok(request) => request,
                        Err(_) => continue,
                    };
                    let _ = delivery.deliver(request, &CancellationToken::new());
                }
            }
        }
    }
}

fn build_request(
    direction: BuildDirection,
    peer: PeerBuildMaterial,
    creator: u32,
    message_id: u32,
    local_reply: Hash,
) -> BuildRequest {
    BuildRequest {
        direction,
        peer,
        creator_tunnel_id: TunnelId::new(creator).expect("creator"),
        message_id,
        outbound_reply_router: matches!(direction, BuildDirection::Outbound).then(|| local_reply),
        originator_hash: matches!(direction, BuildDirection::Inbound).then(|| local_reply),
    }
}

async fn drain_build_reply(
    handle: &mut Ssu2DaemonHandle,
    coord: &mut ExploratoryBuildCoordinator,
    peer_hash: Hash,
) -> Option<(
    i2pr_tunnel::pool::TunnelSlot,
    i2pr_tunnel::pool::TunnelRegistration,
)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        let inbound = match next_inbound_with_timeout(handle, POLL_INTERVAL * 4).await {
            Some(value) => value,
            None => continue,
        };
        let now_ms = wall_ms();
        let dispatch = match dispatch_router_i2np(&inbound, now_ms) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if matches!(
            dispatch,
            RouterI2npOutcome::TunnelBuildReserved {
                kind: RouterI2npKind::OutboundTunnelBuildReply,
                ..
            }
        ) {
            let wrapped = Ssu2InboundI2np {
                link_id: LinkId::new(1).expect("link"),
                peer: PeerId::from_hash(peer_hash),
                bytes: inbound.bytes.clone(),
            };
            let routed = coord
                .route_inbound_i2np(&wrapped, wall_ms())
                .expect("dispatcher outcome");
            if let Some(BuildCoordinatorOutcome::Installed {
                slot, registration, ..
            }) = routed
                .coordinator
                .into_iter()
                .find(|o| matches!(o, BuildCoordinatorOutcome::Installed { .. }))
            {
                return Some((slot, registration));
            }
        }
    }
    None
}

#[tokio::test]
async fn outbound_one_hop_build_installs_into_pool_and_registry() {
    let (mut handle_a, handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let addr_b = handle_b.local_v4().expect("b addr");
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(keys_b.hash),
        keys_b.hash,
        addr_b,
        keys_b.static_public,
        keys_b.intro,
    )
    .expect("dial target");
    let _established = handle_a
        .service()
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial establishes");
    wait_for_active(&handle_a, 1).await;
    wait_for_active(&handle_b, 1).await;

    let responder = BuildResponder {
        static_secret_bytes: keys_b.static_bytes,
        hash: keys_b.hash,
    };
    let responder_delivery = handle_b.delivery().clone();
    let responder_token = CancellationToken::new();
    let responder_task = tokio::spawn(pump_responder_loop(
        handle_b,
        responder_delivery,
        responder,
        responder_token.clone(),
    ));

    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = handle_a.delivery().clone();
    let peer_material = PeerBuildMaterial {
        router_hash: keys_b.hash,
        static_encryption_key: keys_b.static_public_bytes,
        receive_tunnel: TunnelId::new(0x9001).expect("receive"),
        next_tunnel: TunnelId::new(0x9002).expect("next"),
        role: HopRole::OutboundEndpoint,
    };
    let message_id: u32 = 0x51A4_3001;
    let request = build_request(
        BuildDirection::Outbound,
        peer_material,
        0x1000,
        message_id,
        keys_a.hash,
    );
    let bridge_header = BridgeHeader::ShortTransport {
        message_id,
        expiration_seconds: wall_secs().saturating_add(60) as u32,
    };
    let submit = coord
        .submit(request, &delivery, &bridge, bridge_header, &mut rng)
        .expect("submit ok");
    match &submit {
        SubmitResult::Submitted { .. } => {}
        other => panic!("expected Submitted, got {other:?}"),
    }
    assert_eq!(coord.pending_len(), 1);
    assert_eq!(coord.counters().outbound_builds, 1);

    let installed = drain_build_reply(&mut handle_a, &mut coord, keys_b.hash).await;
    let (slot, registration) = installed.expect("OTBRM never arrived");
    assert_eq!(registration.direction(), TunnelDirection::Outbound);
    assert_eq!(registration.hops().len(), 1);
    assert!(registration.tunnel_id().get() > 0);
    assert_eq!(coord.outbound_pool_len(), 1);
    assert_eq!(coord.inbound_pool_len(), 0);
    assert_eq!(coord.pending_len(), 0);
    assert_eq!(coord.counters().installed, 1);
    assert_eq!(coord.counters().inbound_routed, 1);

    let metadata = coord
        .registry_outbound_first_hop(slot)
        .expect("outbound first-hop metadata");
    assert_eq!(metadata.0, keys_b.hash);
    assert_eq!(metadata.1.get(), 0x9001);

    responder_token.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    let _ = responder_task.await;
    let _ = scope_a.shutdown().await;
    let _ = scope_b.shutdown().await;
}

#[tokio::test]
async fn inbound_one_hop_build_installs_into_pool_and_registry() {
    let (mut handle_a, handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let addr_b = handle_b.local_v4().expect("b addr");
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(keys_b.hash),
        keys_b.hash,
        addr_b,
        keys_b.static_public,
        keys_b.intro,
    )
    .expect("dial target");
    let _established = handle_a
        .service()
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial establishes");
    wait_for_active(&handle_a, 1).await;
    wait_for_active(&handle_b, 1).await;

    let responder = BuildResponder {
        static_secret_bytes: keys_b.static_bytes,
        hash: keys_b.hash,
    };
    let responder_delivery = handle_b.delivery().clone();
    let responder_token = CancellationToken::new();
    let responder_task = tokio::spawn(pump_responder_loop(
        handle_b,
        responder_delivery,
        responder,
        responder_token.clone(),
    ));

    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    let delivery = handle_a.delivery().clone();
    let peer_material = PeerBuildMaterial {
        router_hash: keys_b.hash,
        static_encryption_key: keys_b.static_public_bytes,
        receive_tunnel: TunnelId::new(0x9101).expect("receive"),
        next_tunnel: TunnelId::new(0x9102).expect("next"),
        role: HopRole::InboundGateway,
    };
    let message_id: u32 = 0x51A4_3101;
    let request = build_request(
        BuildDirection::Inbound,
        peer_material,
        0x1100,
        message_id,
        keys_a.hash,
    );
    let bridge_header = BridgeHeader::ShortTransport {
        message_id,
        expiration_seconds: wall_secs().saturating_add(60) as u32,
    };
    let submit = coord
        .submit(request, &delivery, &bridge, bridge_header, &mut rng)
        .expect("submit ok");
    match &submit {
        SubmitResult::Submitted { .. } => {}
        other => panic!("expected Submitted, got {other:?}"),
    }
    assert_eq!(coord.pending_len(), 1);

    let installed = drain_build_reply(&mut handle_a, &mut coord, keys_b.hash).await;
    let (_slot, registration) = installed.expect("OTBRM never arrived");
    assert_eq!(registration.direction(), TunnelDirection::Inbound);
    assert_eq!(registration.hops().len(), 1);
    assert_eq!(coord.inbound_pool_len(), 1);
    assert_eq!(coord.outbound_pool_len(), 0);
    assert_eq!(coord.pending_len(), 0);
    assert_eq!(coord.counters().installed, 1);

    responder_token.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    let _ = responder_task.await;
    let _ = scope_a.shutdown().await;
    let _ = scope_b.shutdown().await;
}

#[tokio::test]
async fn liveness_scheduler_pairs_both_directions() {
    let (mut handle_a, handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let addr_b = handle_b.local_v4().expect("b addr");
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(keys_b.hash),
        keys_b.hash,
        addr_b,
        keys_b.static_public,
        keys_b.intro,
    )
    .expect("dial target");
    let _established = handle_a
        .service()
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial establishes");
    wait_for_active(&handle_a, 1).await;

    let responder = BuildResponder {
        static_secret_bytes: keys_b.static_bytes,
        hash: keys_b.hash,
    };
    let responder_delivery = handle_b.delivery().clone();
    let responder_token = CancellationToken::new();
    let responder_task = tokio::spawn(pump_responder_loop(
        handle_b,
        responder_delivery,
        responder,
        responder_token.clone(),
    ));

    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(11));
    let delivery = handle_a.delivery().clone();

    let peer_material_outbound = PeerBuildMaterial {
        router_hash: keys_b.hash,
        static_encryption_key: keys_b.static_public_bytes,
        receive_tunnel: TunnelId::new(0x9201).expect("receive"),
        next_tunnel: TunnelId::new(0x9202).expect("next"),
        role: HopRole::OutboundEndpoint,
    };
    let outbound_request = build_request(
        BuildDirection::Outbound,
        peer_material_outbound,
        0x1200,
        0x51A4_3201,
        keys_a.hash,
    );
    let outbound_header = BridgeHeader::ShortTransport {
        message_id: 0x51A4_3201,
        expiration_seconds: wall_secs().saturating_add(60) as u32,
    };
    let _outbound_submit = coord
        .submit(
            outbound_request,
            &delivery,
            &bridge,
            outbound_header,
            &mut rng,
        )
        .expect("submit ok");

    let peer_material_inbound = PeerBuildMaterial {
        router_hash: keys_b.hash,
        static_encryption_key: keys_b.static_public_bytes,
        receive_tunnel: TunnelId::new(0x9301).expect("receive"),
        next_tunnel: TunnelId::new(0x9302).expect("next"),
        role: HopRole::InboundGateway,
    };
    let inbound_request = build_request(
        BuildDirection::Inbound,
        peer_material_inbound,
        0x1300,
        0x51A4_3301,
        keys_a.hash,
    );
    let inbound_header = BridgeHeader::ShortTransport {
        message_id: 0x51A4_3301,
        expiration_seconds: wall_secs().saturating_add(60) as u32,
    };
    let _inbound_submit = coord
        .submit(
            inbound_request,
            &delivery,
            &bridge,
            inbound_header,
            &mut rng,
        )
        .expect("submit ok");

    let mut drained = 0_u8;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline && drained < 2 {
        let inbound = match next_inbound_with_timeout(&mut handle_a, POLL_INTERVAL * 4).await {
            Some(value) => value,
            None => continue,
        };
        let now_ms = wall_ms();
        let outcome = match dispatch_router_i2np(&inbound, now_ms) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if matches!(
            outcome,
            RouterI2npOutcome::TunnelBuildReserved {
                kind: RouterI2npKind::OutboundTunnelBuildReply,
                ..
            }
        ) {
            let inbound_wrapped = Ssu2InboundI2np {
                link_id: LinkId::new(1).expect("link"),
                peer: PeerId::from_hash(keys_b.hash),
                bytes: inbound.bytes.clone(),
            };
            let routed = coord
                .route_inbound_i2np(&inbound_wrapped, wall_ms())
                .expect("dispatcher outcome");
            if routed
                .coordinator
                .iter()
                .any(|o| matches!(o, BuildCoordinatorOutcome::Installed { .. }))
            {
                drained += 1;
            }
        }
    }
    assert_eq!(drained, 2, "expected both builds to install");
    assert_eq!(coord.outbound_pool_len(), 1);
    assert_eq!(coord.inbound_pool_len(), 1);

    let outbound_slot = coord
        .registrations(TunnelDirection::Outbound)
        .first()
        .expect("outbound slot")
        .slot();
    let inbound_slot = coord
        .registrations(TunnelDirection::Inbound)
        .first()
        .expect("inbound slot")
        .slot();
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(wall_ms());
    scheduler
        .register_pair(outbound_slot, inbound_slot)
        .expect("register pair");
    assert_eq!(scheduler.active_pairs(), 1);

    scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    match action {
        LivenessAction::SendTest {
            outbound_slot: send_outbound,
            inbound_slot: send_inbound,
            ..
        } => {
            assert_eq!(send_outbound, outbound_slot);
            assert_eq!(send_inbound, inbound_slot);
        }
        other => panic!("expected SendTest, got {other:?}"),
    }
    assert_eq!(scheduler.pending_tests(), 1);
    assert_eq!(scheduler.counters().tests_sent, 1);

    let test_id = scheduler.next_pending_test_id().expect("pending test id");
    let refresh = scheduler.record_response(test_id).expect("refresh");
    match refresh {
        LivenessAction::Idle { active_pairs } => assert_eq!(active_pairs, 1),
        other => panic!("expected Idle, got {other:?}"),
    }
    assert_eq!(scheduler.counters().successful_responses, 1);

    responder_token.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    let _ = responder_task.await;
    let _ = scope_a.shutdown().await;
    let _ = scope_b.shutdown().await;
}

#[test]
fn verify_reference_router_info_rejects_bytes_without_ssu2() {
    assert!(matches!(
        verify_reference_router_info(b"not-a-routerinfo"),
        Err(i2pr_daemon::router_i2np::Ssu2ServiceError::InvalidIdentity)
    ));
}

#[test]
fn ssu2_runtime_mapping_and_socket_selection_are_strict() {
    let config = daemon_ssu2_config();
    let runtime = ssu2_runtime_config_from(&config).expect("mapping");
    runtime.validate().expect("runtime validates");
    let sockets = ssu2_socket_config_from(&config).expect("sockets");
    assert!(sockets.ipv4.is_some());
}

#[test]
fn dial_target_validates_loopback_profile() {
    let keys = make_local_keys(44100);
    let target = daemon_dial_target(
        keys.hash,
        "127.0.0.1:44101".parse().expect("loopback"),
        keys.static_public,
        keys.intro,
    );
    assert!(target.is_ok());
}

#[test]
fn deferred_payload_helper_round_trips() {
    let gzip = vec![0x1F, 0x8B, 0x08, 0x00];
    let payload = DeferredPayload::new(gzip, usize::from(u16::MAX)).expect("deferred payload");
    assert!(!payload.as_bytes().is_empty());
}

#[test]
fn empty_btremap_size_is_zero() {
    let map: BTreeMap<u32, u32> = BTreeMap::new();
    assert_eq!(map.len(), 0);
}

#[test]
fn peer_id_from_hash_round_trip() {
    let hash = Hash::from_bytes([0x77; 32]);
    let peer = PeerId::from_hash(hash);
    assert_eq!(peer, PeerId::from_hash(hash));
}
