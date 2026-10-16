//! Plan 480 local qualification receipts: permission-gated collector,
//! direct NetDB product-effect fixture, and negative provenance matrix.
//!
//! Loopback only, controlled profile only. The collector projects drained
//! observer events into phase labels plus counts; transient message IDs are
//! never exported. `live_qualification` stays `"not-run"`: a locally
//! observed sequence is not a two-host claim. vocabulary cross-reference:
//! `tests/integration/ssu2/plan476_executor.py::LOCAL_PHASES`.

use std::io::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::GzEncoder;
use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::{Config, ReseedConfig};
use i2pr_daemon::floodfill::{
    FloodfillCoordinator, FloodfillCoordinatorPolicy, FloodfillDispatchOutcome, NormalReadiness,
    evaluate_normal_eligibility,
};
use i2pr_daemon::router_i2np::{
    FloodfillControlBody, MAX_ROUTER_I2NP_BYTES, RouterDeliveryOutcome, RouterDeliveryRequest,
    RouterI2npKind, RouterI2npOutcome, Ssu2DaemonService,
};
use i2pr_daemon::ssu2_qualification_receipts::{
    LOCAL_PHASES, LocalQualificationPermit, LocalQualificationProfile, LocalReceiptCollector,
};
use i2pr_netdb::{
    FloodfillResourcePolicy, FloodfillRoleState, FloodfillStoreEffect, FloodfillStorePolicy,
    FloodfillTime, ReplicationPolicy, RouterInfoStoreConfig, ServerNetDbConfig,
    UnsolicitedStoreOutcome, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{
    DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload, Hash, I2npBody, I2npMessage,
    MAX_COMMON_STRUCTURE_SIZE, MAX_I2NP_PAYLOAD_SIZE, Mapping, RouterAddress,
};
use i2pr_runtime::{
    CancellationReason, CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial,
    Ssu2PublicationMaterial, Ssu2QualificationEventKind,
};
use i2pr_transport::{AddressFamily, PeerId, ReachabilityState, TerminationCategory};
use rand_chacha::ChaCha8Rng;
use rand_core::{SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Collector, permit, receipt, and phase vocabulary live in
/// production (`i2pr_daemon::ssu2_qualification_receipts`): the Plan
/// 476 per-host executor drains through that interface, not through a
/// test-local copy. These tests pin its gates over live sessions.
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(1_700_000_000)
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
}

fn make_local_keys(port_placeholder: u16) -> LocalKeys {
    let bundle = RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_key = X25519PrivateKey::generate(&mut rand_core::OsRng).expect("static");
    let static_bytes = *static_key.secret_bytes();
    let static_public =
        i2pr_runtime::Ssu2PublicKey::new(static_key.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    loop {
        rand_core::OsRng
            .try_fill_bytes(&mut intro_bytes)
            .expect("rng");
        if intro_bytes.iter().any(|b| *b != 0) {
            break;
        }
    }
    let intro = i2pr_runtime::IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), port_placeholder.to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
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
    }
}

/// Deterministic third-party signed RouterInfo store, mirroring the
/// daemon bootstrap unit fixture (fresh identity, compressed encoding).
/// The record carries the live network ID and a router version so the
/// floodfill serving owner accepts it; the bootstrap owner is
/// network-agnostic and keeps accepting it.
fn signed_router_store(seed: u64, now_ms: u64) -> DatabaseStoreMessage {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let options = Mapping::from_entries(vec![
        ("router.version".to_string(), "0.9.58".to_string()),
        ("netId".to_string(), "2".to_string()),
    ])
    .expect("record options");
    let info = bundle
        .sign_router_info(Date::from_millis(now_ms), Vec::new(), Vec::new(), options)
        .expect("signed RouterInfo");
    let validated = ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(now_ms)),
    )
    .expect("validated RouterInfo");
    let encoded = validated
        .encoded(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encoded RouterInfo");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&encoded).expect("compress RouterInfo");
    let compressed = encoder.finish().expect("finish compression");
    let payload = DeferredPayload::new(compressed, MAX_I2NP_PAYLOAD_SIZE).expect("bounded payload");
    DatabaseStoreMessage {
        key: Hash::from_bytes(*validated.key().as_bytes()),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(payload),
    }
}

fn daemon_ssu2_config() -> i2pr_daemon::config::Ssu2Config {
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
    Config::parse(text).expect("strict profile").ssu2
}

async fn start_daemon_pair() -> (
    i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    i2pr_daemon::router_i2np::Ssu2DaemonHandle,
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

async fn wait_for_active(handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle, expected: usize) {
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

async fn dial_a_to_b(
    handle_a: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    handle_b: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    keys_b: &LocalKeys,
) -> i2pr_runtime::Ssu2EstablishedLink {
    let addr_b = handle_b.local_v4().expect("b addr");
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(keys_b.hash),
        keys_b.hash,
        addr_b,
        keys_b.static_public,
        keys_b.intro,
    )
    .expect("dial target");
    let established = handle_a
        .service()
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("daemon dial establishes");
    assert!(!established.used_cached_token);
    wait_for_active(handle_a, 1).await;
    wait_for_active(handle_b, 1).await;
    established
}

fn deliver_short(
    handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    peer: PeerId,
    message_id: u32,
    body: I2npBody,
) {
    let expiration = wall_secs().saturating_add(60).min(u64::from(u32::MAX)) as u32;
    let message =
        I2npMessage::new_short_transport(message_id, expiration, body).expect("short message");
    let wire = message
        .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
        .expect("encode");
    let request = RouterDeliveryRequest::new(peer, wire, Duration::from_secs(5)).expect("request");
    assert_eq!(
        handle
            .delivery()
            .deliver(request, &CancellationToken::new()),
        RouterDeliveryOutcome::Accepted,
        "live delivery must be accepted"
    );
}

/// Receives dispatches on `handle` until the requested control kind arrives.
async fn next_control(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    kind: RouterI2npKind,
) -> RouterI2npOutcome {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "dispatcher never received {kind:?}"
        );
        match tokio::time::timeout(POLL_INTERVAL * 4, handle.next_dispatched(wall_ms())).await {
            Ok(Some(Ok(outcome))) => {
                if matches!(
                    &outcome,
                    RouterI2npOutcome::RouterControl {
                        kind: got,
                        ..
                    } if *got == kind
                ) {
                    return outcome;
                }
                continue;
            }
            Ok(Some(Err(_))) => continue,
            Ok(None) => panic!("service closed during exchange"),
            Err(_) => continue,
        }
    }
}

fn permitted_collector() -> LocalReceiptCollector {
    LocalReceiptCollector::new(LocalQualificationPermit {
        profile: LocalQualificationProfile::ControlledLoopback,
        consent: true,
    })
    .expect("controlled permit")
}

async fn shutdown_pair(
    handle_a: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    handle_b: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    scope_a: ChildScope,
    scope_b: ChildScope,
) {
    handle_a.shutdown();
    handle_b.shutdown();
    let _ = scope_a.shutdown().await;
    let _ = scope_b.shutdown().await;
}

#[tokio::test]
async fn local_collector_projects_session_counts_without_transient_ids() {
    let (handle_a, mut handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let peer_a = PeerId::from_hash(keys_a.hash);
    let peer_b = PeerId::from_hash(keys_b.hash);
    let _established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");

    // Two distinct transient message IDs must project to identical
    // receipts: the export carries no ID material.
    for message_id in [0xD15C_1001_u32, 0xD15C_1002_u32] {
        let body =
            i2pr_proto::DeliveryStatusMessage::new(0x51A4_1001, Date::from_millis(wall_ms()));
        deliver_short(
            &handle_a,
            peer_b,
            message_id,
            I2npBody::DeliveryStatus(body),
        );
        let dispatched = next_control(&mut handle_b, RouterI2npKind::DeliveryStatus).await;
        assert_eq!(dispatched.peer(), peer_a);
        assert_eq!(dispatched.message_id(), Some(message_id));
    }
    // Drain once for both messages: identical phase counts prove the
    // transient IDs did not leak into the projection.
    let first = permitted_collector()
        .collect(&observer_b)
        .expect("collect session receipt");
    assert!(first.session_observed);
    assert!(!first.product_observed);
    assert_eq!(first.live_qualification, "not-run");
    assert!(
        first
            .phases
            .iter()
            .any(|(phase, _)| *phase == "peer_authenticated")
    );
    assert!(
        first
            .phases
            .iter()
            .any(|(phase, _)| *phase == "request_received")
    );
    assert!(
        first
            .phases
            .iter()
            .all(|(phase, _)| LOCAL_PHASES.contains(phase))
    );
    // Phase order in the export follows the canonical vocabulary order.
    let positions: Vec<usize> = first
        .phases
        .iter()
        .map(|(phase, _)| {
            LOCAL_PHASES
                .iter()
                .position(|p| p == phase)
                .expect("known phase")
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "export order must follow canonical phase order"
    );
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}

#[tokio::test]
async fn daemon_product_hook_accepts_bootstrap_insert_over_live_session() {
    let (handle_a, mut handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let peer_a = PeerId::from_hash(keys_a.hash);
    let peer_b = PeerId::from_hash(keys_b.hash);
    let _established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");

    // A genuine third-party signed RouterInfo store traverses the live
    // authenticated session and is ingested by the real bootstrap owner —
    // the same owner call the daemon composition loop makes.
    let now_ms = wall_ms();
    let store = signed_router_store(0x4801, now_ms);
    let message_id = 0xD15C_2001_u32;
    deliver_short(
        &handle_a,
        peer_b,
        message_id,
        I2npBody::DatabaseStore(Box::new(store.clone())),
    );
    let dispatched = next_control(&mut handle_b, RouterI2npKind::DatabaseStore).await;
    assert_eq!(dispatched.peer(), peer_a);
    assert_eq!(dispatched.message_id(), Some(message_id));

    let config = ReseedConfig {
        enabled: false,
        max_sources: 4,
        max_su3_bytes: 1024,
        sources: Vec::new(),
    };
    let mut bootstrap = Bootstrap::new(RouterInfoStoreConfig::default(), config);
    let outcome = bootstrap
        .ingest_authenticated_router_store(&store, now_ms)
        .expect("valid authenticated store");
    let product_effect = matches!(outcome, UnsolicitedStoreOutcome::Inserted(_));
    assert!(product_effect, "bootstrap owner must insert the store");

    // The exact tuple the daemon composition hook passes to the observer
    // is accepted, and the drain carries the product-effect event bound
    // to the same opaque commitment and link generation as the ingress.
    let link_id = dispatched.link_id();
    assert!(observer_b.record_product_effect(peer_a, link_id, message_id));
    let (events, overflowed) = observer_b.drain();
    assert!(!overflowed);
    let ingress = events
        .iter()
        .find(|event| {
            event.kind() == Ssu2QualificationEventKind::AuthenticatedI2npReceived
                && event.message_id() == Some(message_id)
        })
        .expect("authenticated ingress recorded");
    let effect = events
        .iter()
        .find(|event| event.kind() == Ssu2QualificationEventKind::I2npProductEffectAccepted)
        .expect("product effect recorded");
    assert_eq!(effect.peer_commitment(), ingress.peer_commitment());
    assert_eq!(effect.link_generation(), ingress.link_generation());
    assert_eq!(effect.message_id(), ingress.message_id());
    assert_eq!(
        observer_b.peer_commitment(peer_a),
        Some(ingress.peer_commitment())
    );
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}

#[tokio::test]
async fn fabricated_product_claims_fail_closed() {
    let (handle_a, mut handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let peer_a = PeerId::from_hash(keys_a.hash);
    let peer_b = PeerId::from_hash(keys_b.hash);
    let _established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");

    // Product claim before any authenticated ingress is refused.
    let unestablished = i2pr_transport::LinkId::new(1).expect("link");
    assert!(!observer_b.record_product_effect(peer_a, unestablished, 0xD15C_3001));

    let message_id = 0xD15C_3002_u32;
    let body = i2pr_proto::DeliveryStatusMessage::new(0x51A4_3002, Date::from_millis(wall_ms()));
    deliver_short(
        &handle_a,
        peer_b,
        message_id,
        I2npBody::DeliveryStatus(body),
    );
    let dispatched = next_control(&mut handle_b, RouterI2npKind::DeliveryStatus).await;
    let link_id = dispatched.link_id();
    let stranger = PeerId::from_hash(Hash::from_bytes([0x79; 32]));
    let other_link = i2pr_transport::LinkId::new(link_id.value().saturating_add(7)).expect("link");
    // Wrong peer, wrong link generation, and wrong message ID all fail.
    assert!(!observer_b.record_product_effect(stranger, link_id, message_id));
    assert!(!observer_b.record_product_effect(peer_a, other_link, message_id));
    assert!(!observer_b.record_product_effect(peer_a, link_id, message_id ^ 0xFF));
    // The genuine tuple still succeeds exactly once: no replay.
    assert!(observer_b.record_product_effect(peer_a, link_id, message_id));
    assert!(!observer_b.record_product_effect(peer_a, link_id, message_id));
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}

#[tokio::test]
async fn cancelled_delivery_emits_no_ingress() {
    let (handle_a, handle_b, scope_a, scope_b, _ta, _tb, _ka, keys_b) = start_daemon_pair().await;
    let peer_b = PeerId::from_hash(keys_b.hash);
    let _established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");
    let _ = observer_b.drain();

    let expiration = wall_secs().saturating_add(60).min(u64::from(u32::MAX)) as u32;
    let body = i2pr_proto::DeliveryStatusMessage::new(0x51A4_4001, Date::from_millis(wall_ms()));
    let message =
        I2npMessage::new_short_transport(0xD15C_4001, expiration, I2npBody::DeliveryStatus(body))
            .expect("short message");
    let wire = message
        .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
        .expect("encode");
    let request =
        RouterDeliveryRequest::new(peer_b, wire, Duration::from_secs(5)).expect("request");
    let cancelled = CancellationToken::new();
    cancelled.cancel(CancellationReason::OperatorRequest);
    assert_eq!(
        handle_a.delivery().deliver(request, &cancelled),
        RouterDeliveryOutcome::Cancelled
    );
    let (events, overflowed) = observer_b.drain();
    assert!(!overflowed);
    assert!(
        events
            .iter()
            .all(|event| event.message_id() != Some(0xD15C_4001)),
        "cancelled delivery must leave no ingress trace"
    );
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}

#[tokio::test]
async fn link_loss_clears_pending_product_claim() {
    let (handle_a, mut handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let peer_a = PeerId::from_hash(keys_a.hash);
    let peer_b = PeerId::from_hash(keys_b.hash);
    let established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");

    let message_id = 0xD15C_5001_u32;
    let body = i2pr_proto::DeliveryStatusMessage::new(0x51A4_5001, Date::from_millis(wall_ms()));
    deliver_short(
        &handle_a,
        peer_b,
        message_id,
        I2npBody::DeliveryStatus(body),
    );
    let dispatched = next_control(&mut handle_b, RouterI2npKind::DeliveryStatus).await;
    let link_id = dispatched.link_id();

    // Closing the session must clear the pending ingress: the later
    // product claim fails even though the ingress was genuine.
    established.link.close(TerminationCategory::LocalShutdown);
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let (events, _) = observer_b.drain();
        if events
            .iter()
            .any(|event| event.kind() == Ssu2QualificationEventKind::SessionClosed)
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "session close never observed"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    assert!(!observer_b.record_product_effect(peer_a, link_id, message_id));
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}

#[tokio::test]
async fn session_overflow_fails_receipt() {
    let (handle_a, mut handle_b, scope_a, scope_b, _ta, _tb, _ka, keys_b) =
        start_daemon_pair().await;
    let peer_b = PeerId::from_hash(keys_b.hash);
    let _established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");

    // Flood past the 512-event bound while draining the handoff queue so
    // backpressure, not queue-full, is what the observer records.
    let mut accepted = 0_u32;
    for index in 0..600_u32 {
        let body =
            i2pr_proto::DeliveryStatusMessage::new(0x51A4_6000, Date::from_millis(wall_ms()));
        let expiration = wall_secs().saturating_add(60).min(u64::from(u32::MAX)) as u32;
        let message = I2npMessage::new_short_transport(
            0xD15C_6000 + index,
            expiration,
            I2npBody::DeliveryStatus(body),
        )
        .expect("short message");
        let wire = message
            .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode");
        let request =
            RouterDeliveryRequest::new(peer_b, wire, Duration::from_secs(5)).expect("request");
        match handle_a
            .delivery()
            .deliver(request, &CancellationToken::new())
        {
            RouterDeliveryOutcome::Accepted => accepted += 1,
            RouterDeliveryOutcome::QueueFull => {}
            other => panic!("unexpected delivery outcome: {other:?}"),
        }
        if index % 8 == 7 {
            for _ in 0..8 {
                match tokio::time::timeout(POLL_INTERVAL, handle_b.next_dispatched(wall_ms())).await
                {
                    Ok(Some(_)) => {}
                    _ => break,
                }
            }
        }
    }
    assert!(
        accepted >= 513,
        "need enough accepted sends, got {accepted}"
    );
    // The overflow flag is sticky: the collector must refuse the batch
    // rather than project a truncated sequence.
    let collector = permitted_collector();
    assert_eq!(
        collector.collect(&observer_b).unwrap_err(),
        "receipt-overflow"
    );
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}

#[test]
fn permit_without_consent_is_refused() {
    let collector = LocalReceiptCollector::new(LocalQualificationPermit {
        profile: LocalQualificationProfile::ControlledLoopback,
        consent: false,
    });
    assert!(collector.is_err());
}

#[test]
fn disabled_profile_has_no_observer_to_collect() {
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n";
    let config = Config::parse(text).expect("defaults");
    let bundle = RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let identity =
        i2pr_daemon::router_i2np::generate_controlled_identity(&bundle, "127.0.0.1", 44013)
            .expect("controlled identity");
    // Construction itself refuses the default-off profile, so no service
    // — and therefore no observer — can exist to collect from.
    assert!(
        Ssu2DaemonService::new(
            &config.ssu2,
            Ssu2IdentityMaterial {
                router_hash: identity.router_hash,
                static_secret_bytes: identity.static_secret_bytes,
                intro_key: identity.intro_key,
                router_info: identity.router_info,
            },
        )
        .is_err(),
        "default-off profile must refuse service construction"
    );
}

/// Publication material shaped like the runtime's output for a
/// documentation (TEST-NET-2) address. Documentation ranges pass the
/// eligibility spelling check without implying reachability, and no
/// packet is ever sent to them: activation is proven, then a live
/// loopback session carries the floodfill message.
fn documentation_material() -> Ssu2PublicationMaterial {
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "198.51.100.7".to_string()),
        ("port".to_string(), "44831".to_string()),
        ("mtu".to_string(), "1472".to_string()),
        ("v".to_string(), "2".to_string()),
        ("caps".to_string(), "4".to_string()),
        ("s".to_string(), i2p_b64_encode(&[7_u8; 32])),
        ("i".to_string(), i2p_b64_encode(&[9_u8; 32])),
    ])
    .expect("documentation address options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("documentation router address");
    Ssu2PublicationMaterial {
        address,
        evidence_expires_at: Duration::from_secs(3600),
        reachability: ReachabilityState::Reachable,
        bound_family: AddressFamily::Ipv4,
    }
}

/// Drives a fresh serving coordinator to the Active role through the
/// real eligibility/role transitions (documentation-address material,
/// all measured signals true). No install or publish step runs here:
/// those are composition-owned and already covered by the activation
/// unit tests; the owner decision under test starts at Active.
fn activate_serving_coordinator(
    material: &Ssu2PublicationMaterial,
    now_ms: u64,
) -> FloodfillCoordinator {
    let mut coordinator = FloodfillCoordinator::new(
        Hash::from_bytes([0x48; 32]),
        FloodfillCoordinatorPolicy::default(),
        FloodfillStorePolicy::default(),
        ServerNetDbConfig::default(),
        FloodfillResourcePolicy::default(),
        ReplicationPolicy::default(),
    )
    .expect("bounded coordinator");
    let readiness = NormalReadiness {
        material: Some(material),
        wall_now_ms: now_ms,
        netdb_ready: true,
        storage_ready: true,
        maintenance_ready: true,
        resource_headroom: true,
        clock_sane: true,
        supervision_healthy: true,
    };
    let (snapshot, reasons) = evaluate_normal_eligibility(true, &readiness);
    assert!(
        snapshot.eligible() && reasons.is_empty(),
        "documentation material must be eligible"
    );
    coordinator.update_eligibility(snapshot);
    assert!(coordinator.begin_activation());
    assert!(coordinator.complete_activation());
    assert_eq!(coordinator.role_state(), FloodfillRoleState::Active);
    coordinator
}

#[tokio::test]
async fn daemon_product_hook_accepts_floodfill_served_store_over_live_session() {
    let (handle_a, mut handle_b, scope_a, scope_b, _ta, _tb, keys_a, keys_b) =
        start_daemon_pair().await;
    let peer_a = PeerId::from_hash(keys_a.hash);
    let peer_b = PeerId::from_hash(keys_b.hash);
    let _established = dial_a_to_b(&handle_a, &handle_b, &keys_b).await;
    let observer_b = handle_b
        .qualification_observer()
        .expect("explicit controlled profile observer");

    // The serving owner reaches Active through the real role
    // transitions; the live session under observation is untouched.
    let now_ms = wall_ms();
    let material = documentation_material();
    let mut coordinator = activate_serving_coordinator(&material, now_ms);

    // A genuine third-party signed store traverses the live
    // authenticated session, then the real serving owner decides.
    let store = signed_router_store(0x4802, now_ms);
    let message_id = 0xD15C_7001_u32;
    deliver_short(
        &handle_a,
        peer_b,
        message_id,
        I2npBody::DatabaseStore(Box::new(store.clone())),
    );
    let dispatched = next_control(&mut handle_b, RouterI2npKind::DatabaseStore).await;
    assert_eq!(dispatched.peer(), peer_a);
    assert_eq!(dispatched.message_id(), Some(message_id));
    let link_id = dispatched.link_id();

    // The composition's served arm is `Store(Stored { .. })`: the owner
    // accepted the record for service. `handle_authenticated_decoded`
    // is the exact owner entry point the daemon composition loop calls
    // through `handle_decoded_inbound`.
    let outcome = coordinator
        .handle_authenticated_decoded(
            peer_a,
            link_id,
            dispatched,
            Some(FloodfillControlBody::DatabaseStore(store)),
            FloodfillTime {
                wall_ms: now_ms,
                monotonic_ms: now_ms,
            },
        )
        .expect("owner dispatch");
    assert!(
        matches!(
            outcome,
            FloodfillDispatchOutcome::Store(FloodfillStoreEffect::Stored { .. })
        ),
        "serving owner must store the valid record"
    );

    // The exact hook tuple is accepted, and the collector projects the
    // served product effect in the same epoch as its ingress.
    assert!(observer_b.record_product_effect(peer_a, link_id, message_id));
    let receipt = permitted_collector()
        .collect(&observer_b)
        .expect("collect served receipt");
    assert!(receipt.product_observed);
    assert_eq!(receipt.live_qualification, "not-run");
    assert!(
        receipt
            .phases
            .iter()
            .any(|(phase, _)| *phase == "product_effect_observed")
    );
    shutdown_pair(&handle_a, &handle_b, scope_a, scope_b).await;
}
