//! Plan 282 controlled floodfill local acceptance over real loopback SSU2.
//!
//! Two (or three) live daemon SSU2 services exchange real floodfill I2NP traffic:
//!
//! - lookup direct replies route to the request-supplied `from` gateway, never the
//!   immediate authenticated peer (row 3);
//! - store acknowledgements carry the DatabaseStore reply token to the requested
//!   gateway, direct and tunnel-nested (rows 5 and 6);
//! - supplied-key ECIES tunnel replies unwrap to the expected body (row 4);
//! - direct replication reaches established targets, dials validated targets under
//!   deadline/concurrency bounds, and never falls back to a tunnel (rows 7, 8, 9);
//! - owner cancellation drains the bounded queue, accounts every outcome, and
//!   releases all coordinator leases (row 12).
//!
//! Activation, health withdrawal, and RouterInfo generation rows (2, 11, 13) are
//! blocked on above-floor reachability evidence unavailable to static
//! loopback-homogeneous traffic (see the Plan 282 stop record); the default-off
//! profile (row 14) is covered by coordinator unit tests. No external harness,
//! no reference router, and no public advertisement are involved.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey, open_netdb_ecies_reply};
use i2pr_daemon::config::Config;
use i2pr_daemon::floodfill::{
    FloodfillCoordinator, FloodfillCoordinatorPolicy, FloodfillDaemonEffect,
    FloodfillDeliveryOutcome, FloodfillOwnerExit, deliver_floodfill_effect_with_dial,
    run_floodfill_owner,
};
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, Ssu2DaemonHandle, Ssu2DaemonService,
    daemon_dial_target,
};
use i2pr_netdb::{
    FloodfillEligibilitySnapshot, FloodfillResourcePolicy, FloodfillStoreEffect,
    FloodfillStorePolicy, FloodfillTime, InboundProvenance, NetDbNamespace, RecordProvenance,
    ReplicationPolicy, ServerNetDbConfig, StorePurpose, ValidatedNetDbRecord, ValidatedRouterInfo,
    ValidationContext,
};
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload, Hash,
    I2npBody, I2npMessage, Mapping, ReplyEncryption, ReplySecret, RouterAddress, RouterInfo,
};
use i2pr_runtime::{
    CancellationReason, CancellationToken, ChildFailurePolicy, ChildScope, IntroKey,
    Ssu2IdentityMaterial, Ssu2InboundI2np,
};
use i2pr_transport::{LinkId, PeerId};
use rand_core::{OsRng, TryRngCore};

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RECORD_AGE_MS: u64 = 3_600_000;
const MAX_EFFECT_BYTES: usize = 64 * 1024;

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
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

struct FloodKeys {
    bundle: RouterIdentityBundle,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: i2pr_runtime::Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
}

fn make_keys() -> FloodKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_secret = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_secret.secret_bytes();
    let static_public =
        i2pr_runtime::Ssu2PublicKey::new(static_secret.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    loop {
        OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let intro = IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "1".to_string()),
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
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    FloodKeys {
        bundle,
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
    }
}

struct LiveService {
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
    keys: FloodKeys,
}

impl LiveService {
    fn addr(&self) -> std::net::SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    async fn shutdown(self) {
        self.token.cancel(CancellationReason::OperatorRequest);
        self.scope.shutdown().await;
    }
}

async fn start_service(keys: FloodKeys) -> LiveService {
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
    let base = Config::parse(text).expect("strict profile").ssu2;
    let service = Ssu2DaemonService::new(
        &base,
        Ssu2IdentityMaterial {
            router_hash: keys.hash,
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let handle = service
        .start(
            &scope,
            &i2pr_daemon::config::Ssu2Config {
                port: 0,
                ..base.clone()
            },
        )
        .await
        .expect("bind");
    LiveService {
        handle,
        scope,
        token,
        keys,
    }
}

async fn dial(from: &LiveService, to: &LiveService) {
    let target = daemon_dial_target(
        to.keys.hash,
        to.addr(),
        to.keys.static_public,
        to.keys.intro,
    )
    .expect("dial target");
    from.handle
        .dial(target, DELIVERY_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial");
}

async fn wait_active(handle: &Ssu2DaemonHandle, expected: usize, what: &str) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if handle.snapshot().active_sessions == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} sessions did not reach {expected}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn next_inbound(handle: &mut Ssu2DaemonHandle, what: &str) -> Ssu2InboundI2np {
    tokio::time::timeout(WAIT_TIMEOUT, handle.next_inbound())
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
        .unwrap_or_else(|| panic!("inbound closed waiting for {what}"))
}

fn active_coordinator(local: Hash) -> FloodfillCoordinator {
    let mut coordinator = FloodfillCoordinator::new(
        local,
        FloodfillCoordinatorPolicy::default(),
        FloodfillStorePolicy::default(),
        ServerNetDbConfig::default(),
        FloodfillResourcePolicy::default(),
        ReplicationPolicy::default(),
    )
    .expect("bounded coordinator");
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
    coordinator
}

fn floodfill_time() -> FloodfillTime {
    FloodfillTime {
        wall_ms: wall_ms(),
        monotonic_ms: 1,
    }
}

fn send_i2np(
    from: &LiveService,
    to_hash: Hash,
    message_id: u32,
    body: I2npBody,
    cancel: &CancellationToken,
) {
    let bytes = I2npMessage::new_standard(message_id, Date::from_millis(wall_ms() + 30_000), body)
        .expect("envelope")
        .encode_standard_to_vec(MAX_EFFECT_BYTES)
        .expect("encode");
    let request = RouterDeliveryRequest::new(PeerId::from_hash(to_hash), bytes, DELIVERY_TIMEOUT)
        .expect("request");
    assert_eq!(
        from.handle.delivery().deliver(request, cancel),
        RouterDeliveryOutcome::Accepted
    );
}

fn gzip_router_info(info: &i2pr_proto::RouterInfo) -> Vec<u8> {
    let encoded = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&encoded).expect("gzip");
    encoder.finish().expect("finish")
}

/// Signs a fresh floodfill-flagged RouterInfo for `keys` bound to its live endpoint.
fn floodfill_router_info(keys: &FloodKeys, addr: std::net::SocketAddr) -> RouterInfo {
    let options = Mapping::from_entries(vec![
        ("host".to_string(), addr.ip().to_string()),
        ("port".to_string(), addr.port().to_string()),
        ("v".to_string(), "2".to_string()),
        ("mtu".to_string(), "1280".to_string()),
        ("caps".to_string(), "4".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(keys.static_public.as_bytes()),
        ),
        ("i".to_string(), i2p_b64_encode(keys.intro.as_bytes())),
    ])
    .expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let ri_options =
        Mapping::from_entries(vec![("caps".to_string(), "f".to_string())]).expect("caps");
    keys.bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            ri_options,
        )
        .expect("sign")
}

/// Inserts `info` into the coordinator NetDB as an answerable flood replica.
fn seed_replica(coordinator: &mut FloodfillCoordinator, info: &RouterInfo) {
    let key = i2pr_netdb::router_hash(info.router_identity()).expect("hash");
    let validated = ValidatedRouterInfo::from_router_info(
        info.clone(),
        Some(key),
        ValidationContext::new(Date::from_millis(wall_ms())),
    )
    .expect("valid replica");
    coordinator
        .netdb_mut()
        .insert(
            ValidatedNetDbRecord::RouterInfo(validated),
            RecordProvenance {
                namespace: NetDbNamespace::MainRouter,
                inbound: InboundProvenance::AuthenticatedDirectPeer,
                purpose: StorePurpose::FloodReplica,
                observed_at_ms: wall_ms(),
            },
        )
        .expect("insert replica");
}

fn compressed_store(
    signer: &RouterIdentityBundle,
    key: Hash,
    reply_token: u32,
    reply_gateway: Option<Hash>,
    reply_tunnel_id: Option<u32>,
) -> DatabaseStoreMessage {
    let info = signer
        .sign_router_info(
            Date::from_millis(wall_ms()),
            Vec::new(),
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let compressed = gzip_router_info(&info);
    let compressed_len = compressed.len();
    DatabaseStoreMessage {
        key,
        reply_token,
        reply_tunnel_id,
        reply_gateway,
        data: DatabaseStoreData::RouterInfoCompressed(
            DeferredPayload::new(compressed, compressed_len).expect("payload"),
        ),
    }
}

#[tokio::test]
async fn lookup_direct_route_reaches_supplied_from_not_immediate_peer() {
    let a1 = start_service(make_keys()).await;
    let mut a2 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    dial(&a1, &b).await;
    dial(&a2, &b).await;
    wait_active(&b.handle, 2, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    let cancel = CancellationToken::new();

    // a1 delivers a lookup whose request-supplied gateway is a2, not a1.
    let lookup = DatabaseLookupMessage {
        key: Hash::from_bytes([0xA1; 32]),
        from: a2.keys.hash,
        delivery_flag: false,
        reply_tunnel_id: None,
        lookup_type: 0,
        excluded_peers: Vec::new(),
        reply_encryption: ReplyEncryption::None,
    };
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0001,
        I2npBody::DatabaseLookup(Box::new(lookup)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "lookup at b").await;
    assert_eq!(inbound.peer.hash(), a1.keys.hash);
    let time = floodfill_time();
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, time)
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Lookup(Ok(()))
    ));
    let leased = coordinator.pop_effect().expect("lookup reply effect");
    let delivery = deliver_floodfill_effect_with_dial(
        &b.handle,
        coordinator.netdb(),
        leased,
        wall_ms(),
        MAX_RECORD_AGE_MS,
        &cancel,
    )
    .await;
    assert!(matches!(
        delivery,
        FloodfillDeliveryOutcome::Delivered(RouterDeliveryOutcome::Accepted)
    ));
    // The reply arrives at a2 (the request-supplied gateway), never at a1.
    let reply = next_inbound(&mut a2.handle, "direct reply at a2").await;
    let message =
        I2npMessage::decode_standard(&reply.bytes, MAX_EFFECT_BYTES).expect("decode reply");
    match message.into_body() {
        I2npBody::DatabaseSearchReply(search) => {
            assert_eq!(search.key, Hash::from_bytes([0xA1; 32]))
        }
        other => panic!("expected DatabaseSearchReply, got {other:?}"),
    }
    a1.shutdown().await;
    a2.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn store_direct_ack_carries_reply_token_to_gateway() {
    let a1 = start_service(make_keys()).await;
    let mut a2 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    dial(&a1, &b).await;
    dial(&a2, &b).await;
    wait_active(&b.handle, 2, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    let cancel = CancellationToken::new();

    let stored = make_keys();
    let store = compressed_store(
        &stored.bundle,
        stored.hash,
        0xBEEF11,
        Some(a2.keys.hash),
        Some(0),
    );
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0002,
        I2npBody::DatabaseStore(Box::new(store)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "store at b").await;
    let time = floodfill_time();
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, time)
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Store(
            FloodfillStoreEffect::Stored { .. }
        )
    ));
    let leased = coordinator.pop_effect().expect("store ack effect");
    let delivery = deliver_floodfill_effect_with_dial(
        &b.handle,
        coordinator.netdb(),
        leased,
        wall_ms(),
        MAX_RECORD_AGE_MS,
        &cancel,
    )
    .await;
    assert!(matches!(
        delivery,
        FloodfillDeliveryOutcome::Delivered(RouterDeliveryOutcome::Accepted)
    ));
    let reply = next_inbound(&mut a2.handle, "direct ack at a2").await;
    let message = I2npMessage::decode_standard(&reply.bytes, MAX_EFFECT_BYTES).expect("decode ack");
    match message.into_body() {
        I2npBody::DeliveryStatus(status) => assert_eq!(status.message_id, 0xBEEF11),
        other => panic!("expected DeliveryStatus, got {other:?}"),
    }
    a1.shutdown().await;
    a2.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn store_tunnel_ack_nests_in_gateway_tunnel() {
    let a1 = start_service(make_keys()).await;
    let mut a2 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    dial(&a1, &b).await;
    dial(&a2, &b).await;
    wait_active(&b.handle, 2, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    let cancel = CancellationToken::new();

    let stored = make_keys();
    let store = compressed_store(
        &stored.bundle,
        stored.hash,
        0xBEEF22,
        Some(a2.keys.hash),
        Some(0x4455),
    );
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0003,
        I2npBody::DatabaseStore(Box::new(store)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "store at b").await;
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, floodfill_time())
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Store(
            FloodfillStoreEffect::Stored { .. }
        )
    ));
    let leased = coordinator.pop_effect().expect("tunnel ack effect");
    let delivery = deliver_floodfill_effect_with_dial(
        &b.handle,
        coordinator.netdb(),
        leased,
        wall_ms(),
        MAX_RECORD_AGE_MS,
        &cancel,
    )
    .await;
    assert!(matches!(
        delivery,
        FloodfillDeliveryOutcome::Delivered(RouterDeliveryOutcome::Accepted)
    ));
    // The outer message reaches the requested gateway naming the exact tunnel id.
    let reply = next_inbound(&mut a2.handle, "tunnel ack at a2").await;
    let outer = I2npMessage::decode_standard(&reply.bytes, MAX_EFFECT_BYTES).expect("decode outer");
    match outer.into_body() {
        I2npBody::TunnelGateway(gateway) => {
            assert_eq!(gateway.tunnel_id, 0x4455);
            match gateway.message.into_body() {
                I2npBody::DeliveryStatus(status) => assert_eq!(status.message_id, 0xBEEF22),
                other => panic!("expected nested DeliveryStatus, got {other:?}"),
            }
        }
        other => panic!("expected TunnelGateway, got {other:?}"),
    }
    a1.shutdown().await;
    a2.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn tunnel_lookup_reply_opens_to_expected_body() {
    let a1 = start_service(make_keys()).await;
    let mut a2 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    dial(&a1, &b).await;
    dial(&a2, &b).await;
    wait_active(&b.handle, 2, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    let cancel = CancellationToken::new();

    let reply_key = ReplySecret::from_bytes([0x5E; 32]);
    let reply_tag = ReplySecret::from_bytes([0x6F; 8]);
    let lookup = DatabaseLookupMessage {
        key: Hash::from_bytes([0xA4; 32]),
        from: a2.keys.hash,
        delivery_flag: true,
        reply_tunnel_id: Some(0x7172),
        lookup_type: 0,
        excluded_peers: Vec::new(),
        reply_encryption: ReplyEncryption::Ecies {
            reply_key: reply_key.clone(),
            reply_tags: vec![reply_tag.clone()],
        },
    };
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0004,
        I2npBody::DatabaseLookup(Box::new(lookup)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "ECIES lookup at b").await;
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, floodfill_time())
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Lookup(Ok(()))
    ));
    let leased = coordinator.pop_effect().expect("tunnel reply effect");
    let delivery = deliver_floodfill_effect_with_dial(
        &b.handle,
        coordinator.netdb(),
        leased,
        wall_ms(),
        MAX_RECORD_AGE_MS,
        &cancel,
    )
    .await;
    assert!(matches!(
        delivery,
        FloodfillDeliveryOutcome::Delivered(RouterDeliveryOutcome::Accepted)
    ));
    // Exactly one Existing Session payload inside one Garlic inside one TunnelGateway,
    // and the requester fixture opens it to the expected lookup body.
    let reply = next_inbound(&mut a2.handle, "tunnel reply at a2").await;
    let outer = I2npMessage::decode_standard(&reply.bytes, MAX_EFFECT_BYTES).expect("decode outer");
    let gateway = match outer.into_body() {
        I2npBody::TunnelGateway(gateway) => gateway,
        other => panic!("expected TunnelGateway, got {other:?}"),
    };
    assert_eq!(gateway.tunnel_id, 0x7172);
    let garlic = match gateway.message.into_body() {
        I2npBody::Garlic(garlic) => garlic,
        other => panic!("expected exactly one Garlic, got {other:?}"),
    };
    let wire = garlic.payload.as_bytes();
    assert!(wire.len() > 8, "tag-prefixed reply payload");
    assert_eq!(&wire[..8], reply_tag.as_bytes(), "session tag prefix");
    let opened = open_netdb_ecies_reply(&reply_key, &reply_tag, &wire[8..]).expect("open reply");
    // The opened bytes are the service's reply body encoding (not a full envelope).
    let expected = I2npBody::DatabaseSearchReply(i2pr_proto::DatabaseSearchReplyMessage {
        key: Hash::from_bytes([0xA4; 32]),
        peer_hashes: Vec::new(),
        from: b.keys.hash,
    })
    .encode_to_vec(MAX_EFFECT_BYTES)
    .expect("encode expected body");
    assert_eq!(opened, expected, "decrypts to the expected lookup body");
    a1.shutdown().await;
    a2.shutdown().await;
    b.shutdown().await;
}

/// Drains at most `limit` queued effects, delivering the first DirectFlood that
/// targets `want` and asserting every drained effect is accounted.
async fn drain_to_direct_flood(
    coordinator: &mut FloodfillCoordinator,
    handle: &Ssu2DaemonHandle,
    want: Hash,
    cancel: &CancellationToken,
    limit: usize,
) {
    let mut flooded = false;
    for _ in 0..limit {
        let Some(leased) = coordinator.pop_effect() else {
            break;
        };
        let is_want = matches!(
            &leased.effect,
            Some(FloodfillDaemonEffect::DirectFlood { action }) if action.peer == want
        );
        let delivery = deliver_floodfill_effect_with_dial(
            handle,
            coordinator.netdb(),
            leased,
            wall_ms(),
            MAX_RECORD_AGE_MS,
            cancel,
        )
        .await;
        if is_want {
            assert!(matches!(
                delivery,
                FloodfillDeliveryOutcome::Delivered(RouterDeliveryOutcome::Accepted)
            ));
            flooded = true;
        }
    }
    assert!(flooded, "planned direct flood to the wanted peer");
}

#[tokio::test]
async fn replication_reaches_established_target_directly() {
    let a1 = start_service(make_keys()).await;
    let mut a2 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    dial(&a1, &b).await;
    dial(&a2, &b).await;
    wait_active(&b.handle, 2, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    // a2 is the only floodfill candidate, so the replication plan targets it.
    seed_replica(
        &mut coordinator,
        &floodfill_router_info(&a2.keys, a2.addr()),
    );
    let cancel = CancellationToken::new();

    let stored = make_keys();
    // Token-bearing publisher store: only publisher stores plan replication.
    let store = compressed_store(
        &stored.bundle,
        stored.hash,
        0xF10D05,
        Some(a1.keys.hash),
        Some(0),
    );
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0005,
        I2npBody::DatabaseStore(Box::new(store)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "store at b").await;
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, floodfill_time())
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Store(
            FloodfillStoreEffect::Stored { .. }
        )
    ));
    drain_to_direct_flood(&mut coordinator, &b.handle, a2.keys.hash, &cancel, 8).await;
    // The zero-token store arrives over the established session, never a tunnel.
    let reply = next_inbound(&mut a2.handle, "direct flood at a2").await;
    let message =
        I2npMessage::decode_standard(&reply.bytes, MAX_EFFECT_BYTES).expect("decode flood");
    match message.into_body() {
        I2npBody::DatabaseStore(store) => {
            assert_eq!(store.key, stored.hash);
            assert_eq!(store.reply_token, 0);
            assert_eq!(store.reply_gateway, None);
            assert_eq!(store.reply_tunnel_id, None);
        }
        other => panic!("expected direct DatabaseStore, got {other:?}"),
    }
    a1.shutdown().await;
    a2.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn replication_dials_validated_target_under_bounds() {
    let a1 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    let mut c = start_service(make_keys()).await;
    dial(&a1, &b).await;
    wait_active(&b.handle, 1, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    // c never dialed b: the only path is a bounded dial from c's validated RouterInfo.
    seed_replica(&mut coordinator, &floodfill_router_info(&c.keys, c.addr()));
    let cancel = CancellationToken::new();

    let stored = make_keys();
    // Token-bearing publisher store: only publisher stores plan replication.
    let store = compressed_store(
        &stored.bundle,
        stored.hash,
        0xF10D06,
        Some(a1.keys.hash),
        Some(0),
    );
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0006,
        I2npBody::DatabaseStore(Box::new(store)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "store at b").await;
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, floodfill_time())
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Store(
            FloodfillStoreEffect::Stored { .. }
        )
    ));
    drain_to_direct_flood(&mut coordinator, &b.handle, c.keys.hash, &cancel, 8).await;
    let reply = next_inbound(&mut c.handle, "dialed flood at c").await;
    let message =
        I2npMessage::decode_standard(&reply.bytes, MAX_EFFECT_BYTES).expect("decode flood");
    match message.into_body() {
        I2npBody::DatabaseStore(store) => {
            assert_eq!(store.key, stored.hash);
            assert_eq!(store.reply_token, 0);
        }
        other => panic!("expected direct DatabaseStore, got {other:?}"),
    }
    a1.shutdown().await;
    b.shutdown().await;
    c.shutdown().await;
}

#[tokio::test]
async fn failed_direct_flood_terminates_without_tunnel() {
    let a1 = start_service(make_keys()).await;
    let mut b = start_service(make_keys()).await;
    let c = start_service(make_keys()).await;
    dial(&a1, &b).await;
    wait_active(&b.handle, 1, "b").await;
    let mut coordinator = active_coordinator(b.keys.hash);
    seed_replica(&mut coordinator, &floodfill_router_info(&c.keys, c.addr()));
    let cancel = CancellationToken::new();

    let stored = make_keys();
    // Token-bearing publisher store: only publisher stores plan replication.
    let store = compressed_store(
        &stored.bundle,
        stored.hash,
        0xF10D07,
        Some(a1.keys.hash),
        Some(0),
    );
    send_i2np(
        &a1,
        b.keys.hash,
        0xA1_0007,
        I2npBody::DatabaseStore(Box::new(store)),
        &cancel,
    );
    let inbound = next_inbound(&mut b.handle, "store at b").await;
    let outcome = coordinator
        .handle_authenticated_i2np(&inbound, floodfill_time())
        .expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::floodfill::FloodfillDispatchOutcome::Store(
            FloodfillStoreEffect::Stored { .. }
        )
    ));
    let mut flood = None;
    for _ in 0..8 {
        let Some(leased) = coordinator.pop_effect() else {
            break;
        };
        let is_flood = matches!(
            &leased.effect,
            Some(FloodfillDaemonEffect::DirectFlood { action }) if action.peer == c.keys.hash
        );
        if is_flood {
            flood = Some(leased);
            break;
        }
        // Drain the accompanying acknowledgement to its live gateway first.
        let delivery = deliver_floodfill_effect_with_dial(
            &b.handle,
            coordinator.netdb(),
            leased,
            wall_ms(),
            MAX_RECORD_AGE_MS,
            &cancel,
        )
        .await;
        assert!(matches!(
            delivery,
            FloodfillDeliveryOutcome::Delivered(RouterDeliveryOutcome::Accepted)
        ));
    }
    let leased = flood.expect("planned direct flood to c");
    // The target disappears: the bounded dial must fail with a typed outcome.
    c.shutdown().await;
    let outcome = tokio::time::timeout(
        Duration::from_secs(45),
        deliver_floodfill_effect_with_dial(
            &b.handle,
            coordinator.netdb(),
            leased,
            wall_ms(),
            MAX_RECORD_AGE_MS,
            &cancel,
        ),
    )
    .await
    .expect("bounded dial terminates");
    assert!(
        !matches!(outcome, FloodfillDeliveryOutcome::Delivered(_)),
        "dead target must not report delivery"
    );
    assert!(coordinator.pop_effect().is_none());
    a1.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn owner_cancel_drains_bounded_effects_and_releases_budget() {
    let b = start_service(make_keys()).await;
    let mut coordinator = active_coordinator(b.keys.hash);
    // Queue two lookup replies without delivering them.
    for index in 0u8..2u8 {
        let lookup = DatabaseLookupMessage {
            key: Hash::from_bytes([0xC0 + index; 32]),
            from: Hash::from_bytes([0xD0 + index; 32]),
            delivery_flag: false,
            reply_tunnel_id: None,
            lookup_type: 0,
            excluded_peers: Vec::new(),
            reply_encryption: ReplyEncryption::None,
        };
        coordinator
            .handle_lookup(
                PeerId::from_bytes([0xE0; 32]),
                LinkId::new(1).expect("link"),
                &lookup,
                floodfill_time(),
            )
            .expect("queue lookup reply");
    }
    assert_eq!(coordinator.stats().queued_effects, 2);
    let cancel = CancellationToken::new();
    cancel.cancel(CancellationReason::OperatorRequest);
    let (exit, stats) = run_floodfill_owner(
        b.handle,
        coordinator,
        cancel,
        Duration::from_millis(50),
        MAX_RECORD_AGE_MS,
        64,
        Duration::from_secs(5),
    )
    .await
    .expect("owner exits on cancel");
    assert_eq!(exit, FloodfillOwnerExit::RequestedShutdown);
    // Both queued effects drained exactly once; the queue and budget return to baseline.
    assert_eq!(stats.queued_effects, 0);
    assert_eq!(stats.queued_bytes, 0);
    assert_eq!(
        stats.delivered_effects + stats.failed_effects,
        2,
        "every drained effect is accounted"
    );
    b.token.cancel(CancellationReason::OperatorRequest);
    b.scope.shutdown().await;
}
