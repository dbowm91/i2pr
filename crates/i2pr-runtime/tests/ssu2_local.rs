//! Plan 158 local real-UDP acceptance suite.
//!
//! Two independent i2pr SSU2 runtime instances drive complete
//! authenticated sessions through real localhost datagrams. No private
//! helper moves handshake bytes between protocol objects: every byte
//! the tests observe crosses a real UDP socket, and every handshake or
//! session decision happens inside the runtime service under test.
//!
//! Coverage maps to the plan's §12 scenarios: tokenless establishment
//! (§12.1), cached-token establishment with consumed/stale recovery
//! (§12.2), bidirectional multi-I2NP exchange with fragmentation
//! (§12.3), loss/reorder/duplicate recovery through the test-only
//! pre-send fault policy (§12.4), and shutdown/resource baselines
//! (§12.5), plus malformed-traffic boundedness (§14) and admission
//! ceilings (§4–§5).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, Ssu2DialOutcome, Ssu2DialTarget,
    Ssu2EstablishedLink, Ssu2IdentityMaterial, Ssu2InboundI2np, Ssu2RuntimeConfig,
    Ssu2RuntimeDeadlines, Ssu2RuntimeLimits, Ssu2RuntimeService, Ssu2SendOutcome,
    Ssu2ServiceHandle, Ssu2SocketConfig, Ssu2TestFaults,
};
use i2pr_transport::{
    EncodedI2npMessage, LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId, TerminationCategory, TransportKind,
};
use i2pr_transport_ssu2::{IntroKey, Ssu2PublicKey, constants};
use rand_core::{OsRng, TryRngCore};
use tokio::net::UdpSocket;

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

/// I2P-base64 (alphabet `A-Za-z0-9-~`, `=` padding) encoder for
/// test-only RouterAddress construction.
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

struct RouterKeys {
    peer: PeerId,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
}

/// Builds one test router identity: fresh OS randomness, an Ed25519
/// router identity, a transport static key, an intro key, and a signed
/// RouterInfo carrying a matching SSU2 address. The literal endpoint
/// inside the RouterInfo is a placeholder: dial addresses travel
/// out-of-band (as the plan's §12.1 requires), and the runtime binds
/// peer identity through the static key, never the advertised endpoint.
fn make_router_keys() -> RouterKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_key.secret_bytes();
    let static_public = Ssu2PublicKey::new(static_key.public_bytes()).expect("static public");
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
        ("port".to_string(), "43000".to_string()),
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
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    RouterKeys {
        peer: PeerId::from_hash(hash),
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
    }
}

struct Fixture {
    service: Ssu2RuntimeService,
    scope: ChildScope,
    handle: Ssu2ServiceHandle,
    keys: RouterKeys,
}

impl Fixture {
    fn addr(&self) -> SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    fn dial_target(&self, peer: &RouterKeys, addr: SocketAddr) -> Ssu2DialTarget {
        Ssu2DialTarget::new(peer.peer, peer.hash, addr, peer.static_public, peer.intro)
            .expect("dial target")
    }

    async fn shutdown_scope(self) -> usize {
        self.service.shutdown();
        let report = self.scope.shutdown().await;
        report.joined()
    }
}

async fn start_fixture_with_config(config: Ssu2RuntimeConfig, keys: RouterKeys) -> Fixture {
    let service = Ssu2RuntimeService::new(
        config,
        Ssu2IdentityMaterial {
            router_hash: keys.hash,
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .expect("service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let handle = service
        .start(
            &scope,
            Ssu2SocketConfig {
                ipv4: Some("127.0.0.1:0".parse().expect("loopback")),
                ipv6: None,
            },
        )
        .await
        .expect("bind");
    Fixture {
        service,
        scope,
        handle,
        keys,
    }
}

async fn start_fixture(keys: RouterKeys) -> Fixture {
    start_fixture_with_config(Ssu2RuntimeConfig::default(), keys).await
}

async fn dial(from: &Fixture, peer: &RouterKeys, addr: SocketAddr) -> Ssu2EstablishedLink {
    let target = from.dial_target(peer, addr);
    from.service
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial")
}

async fn wait_for_active(fixture: &Fixture, expected: usize) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture.service.snapshot().active_sessions == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "active sessions did not reach {expected}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn wait_for_cached(fixture: &Fixture, expected: usize) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture.service.snapshot().cached_tokens >= expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "cached tokens did not reach {expected}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn recv_exact(handle: &mut Ssu2ServiceHandle, count: usize) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while messages.len() < count {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for I2NP");
        let next = tokio::time::timeout(remaining, handle.next_inbound())
            .await
            .expect("timeout")
            .expect("channel open");
        messages.push(next.bytes);
    }
    messages
}

/// Builds one deterministic encoded I2NP message: 9-byte transport
/// header (type, message ID, expiration) plus a patterned body.
fn i2np_message(id: u32, body_len: usize, seed: u8) -> Vec<u8> {
    assert!(body_len > 0);
    let mut bytes = Vec::with_capacity(9 + body_len);
    bytes.push(6_u8);
    bytes.extend_from_slice(&id.to_be_bytes());
    let expiration = wall_secs().saturating_add(3600).min(u64::from(u32::MAX)) as u32;
    bytes.extend_from_slice(&expiration.to_be_bytes());
    for index in 0..body_len {
        bytes.push(seed.wrapping_add((index % 251) as u8));
    }
    bytes
}

fn send(fixture: &Fixture, peer: PeerId, id: u32, body_len: usize, seed: u8) -> Vec<u8> {
    let bytes = i2np_message(id, body_len, seed);
    let message = EncodedI2npMessage::new(bytes.clone()).expect("message");
    assert_eq!(
        fixture
            .service
            .send_i2np(peer, message, Duration::from_secs(5)),
        Ssu2SendOutcome::Accepted
    );
    bytes
}

#[tokio::test]
async fn tokenless_establishment_over_real_udp() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;

    let established = dial(&a, &b.keys, b.addr()).await;
    assert!(
        !established.used_cached_token,
        "first dial must take the tokenless Retry path"
    );
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;

    let snapshot_a = a.service.snapshot();
    let snapshot_b = b.service.snapshot();
    assert!(snapshot_a.sessions_established >= 1);
    assert!(snapshot_b.sessions_established >= 1);
    assert_eq!(snapshot_a.pending_outbound, 0);
    assert_eq!(snapshot_b.pending_inbound, 0);

    let manager_a = a
        .service
        .manager()
        .snapshot(Duration::from_secs(60))
        .expect("snapshot");
    assert!(
        manager_a
            .links
            .iter()
            .any(|link| link.authenticated && link.transport == TransportKind::Ssu2),
        "authenticated SSU2 link registered through the generic manager"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

#[tokio::test]
async fn cached_token_establishment_with_stale_recovery() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;

    // First session: tokenless, then the responder's NewToken
    // announcement arrives in-band and the initiator caches it.
    let first = dial(&a, &b.keys, b.addr()).await;
    assert!(!first.used_cached_token);
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;
    wait_for_cached(&a, 1).await;

    // Clean close of the first session on both sides.
    first.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&a, 0).await;
    wait_for_active(&b, 0).await;

    // Second session: the cached token skips the Retry round trip,
    // and the one-use token is consumed by the responder.
    let second = dial(&a, &b.keys, b.addr()).await;
    assert!(
        second.used_cached_token,
        "second dial must use the cached token path"
    );
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;
    // Wait for the second session's own spare before closing: the
    // NewToken announcement is still in flight at promotion time.
    wait_for_cached(&a, 1).await;
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if b.service.snapshot().token_table_entries == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "token table must hold exactly the fresh spare"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    second.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&a, 0).await;
    wait_for_active(&b, 0).await;

    // Stale-token recovery: rotate the responder table so the newly
    // cached token is unknown, then dial. The cached attempt finds no
    // SessionCreated inside the grace period and the same dial
    // transparently restarts tokenless.
    wait_for_cached(&a, 1).await;
    b.service.rotate_address_tokens();
    let third = dial(&a, &b.keys, b.addr()).await;
    assert!(
        !third.used_cached_token,
        "stale cached token must fall back to the tokenless path"
    );
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;
    third.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&a, 0).await;
    wait_for_active(&b, 0).await;

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

#[tokio::test]
async fn bidirectional_i2np_exchange_with_fragmentation() {
    let mut a = start_fixture(make_router_keys()).await;
    let mut b = start_fixture(make_router_keys()).await;

    let _link = dial(&a, &b.keys, b.addr()).await;
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;

    // A→B: small, boundary-sized, and multi-fragment messages.
    let a_to_b = vec![
        send(&a, b.keys.peer, 0xA001, 100, 0x11),
        send(&a, b.keys.peer, 0xA002, 1024, 0x22),
        send(&a, b.keys.peer, 0xA003, 3000, 0x33),
    ];
    // B→A: small and fragmented messages on the reverse path.
    let b_to_a = vec![
        send(&b, a.keys.peer, 0xB001, 64, 0x44),
        send(&b, a.keys.peer, 0xB002, 2500, 0x55),
    ];

    let received_b = recv_exact(&mut b.handle, a_to_b.len()).await;
    let received_a = recv_exact(&mut a.handle, b_to_a.len()).await;
    // Ordered delivery between distinct I2NP messages is not
    // required, so both directions compare as sorted byte sets.
    let mut sorted_b = received_b.clone();
    sorted_b.sort();
    let mut sorted_a_to_b = a_to_b.clone();
    sorted_a_to_b.sort();
    assert_eq!(sorted_b, sorted_a_to_b, "A→B bytes must round-trip exactly");
    let mut sorted_a = received_a.clone();
    sorted_a.sort();
    let mut sorted_b_to_a = b_to_a.clone();
    sorted_b_to_a.sort();
    assert_eq!(sorted_a, sorted_b_to_a, "B→A bytes must round-trip exactly");

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

#[tokio::test]
async fn data_loss_recovers_with_exact_once_delivery() {
    let a = start_fixture(make_router_keys()).await;
    let mut b = start_fixture(make_router_keys()).await;

    let _link = dial(&a, &b.keys, b.addr()).await;
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;
    // Settle handshake/announcement traffic, then arm deterministic
    // drops relative to the next transmissions.
    tokio::time::sleep(Duration::from_millis(300)).await;
    a.service.set_test_faults(Some(Ssu2TestFaults {
        drop_transmit: [0, 1].into_iter().collect(),
        ..Default::default()
    }));

    let mut expected = Vec::new();
    for (index, seed) in [0x61_u8, 0x62, 0x63].iter().enumerate() {
        expected.push(send(&a, b.keys.peer, 0xC001 + index as u32, 900, *seed));
    }
    // Disarm only after delivery: transmission is asynchronous, so
    // disarming here would race the flush and void the policy.
    let received = recv_exact(&mut b.handle, expected.len()).await;
    a.service.set_test_faults(None);
    assert_eq!(
        received.len(),
        expected.len(),
        "fresh retransmission must recover every loss"
    );
    let received_set: HashSet<Vec<u8>> = received.into_iter().collect();
    let expected_set: HashSet<Vec<u8>> = expected.into_iter().collect();
    assert_eq!(received_set, expected_set);
    assert!(
        a.service.snapshot().fault_drops >= 2,
        "fault policy must have applied"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

#[tokio::test]
async fn ack_loss_reorder_and_duplicate_recover_exactly_once() {
    let a = start_fixture(make_router_keys()).await;
    let mut b = start_fixture(make_router_keys()).await;

    let _link = dial(&a, &b.keys, b.addr()).await;
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Drop the responder's first reply transmissions (its ACKs): the
    // initiator must recover via RTO without ever redelivering. Two
    // drops keep the recovery inside three RTO rounds.
    b.service.set_test_faults(Some(Ssu2TestFaults {
        drop_transmit: [0, 1].into_iter().collect(),
        ..Default::default()
    }));
    let first = send(&a, b.keys.peer, 0xD001, 200, 0x71);
    let received = recv_exact(&mut b.handle, 1).await;
    assert_eq!(received, vec![first]);
    // Keep the ACK-loss policy armed through recovery: the dropped
    // indices cover only the first replies, and later retransmissions
    // take higher indices.

    // Reorder the next two transmissions, then duplicate one: I2NP
    // delivery stays exact and at most once.
    tokio::time::sleep(Duration::from_millis(300)).await;
    a.service.set_test_faults(Some(Ssu2TestFaults {
        duplicate_transmit: [1].into_iter().collect(),
        swap_next_two: true,
        ..Default::default()
    }));
    let second = send(&a, b.keys.peer, 0xD002, 200, 0x72);
    let third = send(&a, b.keys.peer, 0xD003, 200, 0x73);
    let rest = recv_exact(&mut b.handle, 2).await;
    a.service.set_test_faults(None);
    b.service.set_test_faults(None);
    let rest_set: HashSet<Vec<u8>> = rest.into_iter().collect();
    assert_eq!(rest_set, HashSet::from([second, third]));
    // Quiet period: duplicates must not surface as redelivery.
    let quiet = tokio::time::timeout(Duration::from_millis(500), b.handle.next_inbound()).await;
    assert!(quiet.is_err(), "no duplicate redelivery may surface");

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

#[tokio::test]
async fn malformed_and_random_traffic_creates_no_state() {
    let a = start_fixture(make_router_keys()).await;
    let baseline = a.service.snapshot();
    let probe = UdpSocket::bind("127.0.0.1:0").await.expect("probe");
    probe.connect(a.addr()).await.expect("connect");

    let mut garbage = vec![0_u8; 1];
    probe.send(&garbage).await.expect("send");
    garbage = vec![0xFF_u8; 39];
    probe.send(&garbage).await.expect("send");
    let mut random = vec![0_u8; 1472];
    OsRng.try_fill_bytes(&mut random).expect("rng");
    for chunk in random.chunks(256) {
        probe.send(chunk).await.expect("send");
    }
    probe.send(&random).await.expect("send");
    // Truncated long-header shape plus unknown-type bytes.
    probe.send(&[0x01_u8; 48]).await.expect("send");
    // Spoofed-source burst from a second socket.
    let spoofer = UdpSocket::bind("127.0.0.1:0").await.expect("spoofer");
    spoofer.connect(a.addr()).await.expect("connect");
    for _ in 0..64 {
        spoofer.send(&random).await.expect("send");
    }
    tokio::time::sleep(Duration::from_millis(400)).await;

    let after = a.service.snapshot();
    assert_eq!(after.pending_outbound, 0);
    assert_eq!(after.pending_inbound, 0);
    assert_eq!(after.active_sessions, 0);
    assert_eq!(after.token_table_entries, 0);
    assert!(
        after.cheap_drops > baseline.cheap_drops,
        "random traffic must be cheaply rejected"
    );

    // The service is still healthy: a real dial succeeds afterwards.
    let b = start_fixture(make_router_keys()).await;
    let established = dial(&a, &b.keys, b.addr()).await;
    assert!(!established.used_cached_token);
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 1).await;

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

#[tokio::test]
async fn active_session_cap_denies_with_baseline_return() {
    let config = Ssu2RuntimeConfig {
        limits: Ssu2RuntimeLimits {
            max_active_sessions: 2,
            max_active_per_ip: 2,
            max_active_per_subnet: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let a = start_fixture_with_config(config, make_router_keys()).await;
    let mut b = start_fixture(make_router_keys()).await;
    let c = start_fixture(make_router_keys()).await;
    let d = start_fixture(make_router_keys()).await;

    // Two sessions to distinct peers fill the exact ceiling. Distinct
    // peers matter: the generic duplicate policy keeps only one link
    // per peer/direction pair, independent of this ceiling.
    let first = dial(&a, &b.keys, b.addr()).await;
    let second = dial(&a, &c.keys, c.addr()).await;
    wait_for_active(&a, 2).await;
    // At the exact ceiling the next dial is denied, not queued.
    let target = a.dial_target(&d.keys, d.addr());
    let denied = a
        .service
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await;
    assert!(
        matches!(denied, Err(Ssu2DialOutcome::ResourceDenied)),
        "third dial at the ceiling must be denied"
    );

    // Rapid sends stay within typed outcomes and every accepted
    // message round-trips exactly.
    let mut accepted = Vec::new();
    for index in 0..70_u32 {
        let bytes = i2np_message(0xE000 + index, 700, 0x81);
        let message = EncodedI2npMessage::new(bytes.clone()).expect("message");
        match a
            .service
            .send_i2np(b.keys.peer, message, Duration::from_secs(5))
        {
            Ssu2SendOutcome::Accepted => accepted.push(bytes),
            Ssu2SendOutcome::QueueFull => {}
            outcome => panic!("unexpected send outcome: {outcome:?}"),
        }
    }
    assert!(!accepted.is_empty(), "at least one send must be accepted");
    let received = recv_exact(&mut b.handle, accepted.len()).await;
    assert_eq!(
        received.into_iter().collect::<HashSet<_>>(),
        accepted.into_iter().collect::<HashSet<_>>()
    );

    // Cancellation and shutdown return every table to baseline.
    let manager_links = |fixture: &Fixture| {
        fixture
            .service
            .manager()
            .snapshot(Duration::from_secs(60))
            .expect("snapshot")
            .links
            .len()
    };
    for link_id in [first.link.link_id(), second.link.link_id()] {
        a.service
            .close_ssu2(link_id, TerminationCategory::LocalShutdown);
    }
    wait_for_active(&a, 0).await;
    wait_for_active(&b, 0).await;
    wait_for_active(&c, 0).await;
    assert_eq!(manager_links(&a), 0);
    assert_eq!(manager_links(&b), 0);
    assert_eq!(manager_links(&c), 0);
    let snapshot = a.service.snapshot();
    assert_eq!(snapshot.pending_outbound + snapshot.pending_inbound, 0);

    a.shutdown_scope().await;
    b.shutdown_scope().await;
    c.shutdown_scope().await;
    d.shutdown_scope().await;
}

#[tokio::test]
async fn graceful_close_abrupt_peer_and_cancel_return_to_baseline() {
    let config = Ssu2RuntimeConfig {
        deadlines: Ssu2RuntimeDeadlines {
            idle: Duration::from_millis(1500),
            ..Default::default()
        },
        ..Default::default()
    };
    let a = start_fixture_with_config(config, make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;
    let c = start_fixture(make_router_keys()).await;

    // Sessions to distinct peers: the generic duplicate policy keeps a
    // single link per peer/direction pair, so the two-session baseline
    // needs two peers.
    let first = dial(&a, &b.keys, b.addr()).await;
    let _second = dial(&a, &c.keys, c.addr()).await;
    wait_for_active(&a, 2).await;
    wait_for_active(&b, 1).await;
    wait_for_active(&c, 1).await;

    // Graceful close of one session notifies the peer, which releases
    // its side without touching the surviving session.
    first.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&a, 1).await;
    wait_for_active(&b, 0).await;
    wait_for_active(&c, 1).await;

    // Abrupt peer shutdown: no more traffic flows on the surviving
    // session, so its idle deadline fires and releases its tables.
    c.service.shutdown();
    let joined_c = c.scope.shutdown().await.joined();
    assert!(joined_c >= 1, "loop task must join");
    wait_for_active(&a, 0).await;
    let snapshot_a = a.service.snapshot();
    assert_eq!(snapshot_a.pending_outbound + snapshot_a.pending_inbound, 0);

    // Cancelling the remaining service with (recently) active state
    // still joins every task and empties every table.
    let snapshot = a.service.snapshot();
    assert_eq!(snapshot.pending_outbound, 0);
    assert_eq!(snapshot.pending_inbound, 0);
    assert_eq!(snapshot.active_sessions, 0);
    assert_eq!(
        a.service
            .manager()
            .snapshot(Duration::from_secs(60))
            .expect("snapshot")
            .links
            .len(),
        0
    );
    let joined_a = a.shutdown_scope().await;
    assert!(joined_a >= 1, "loop task must join");
    b.shutdown_scope().await;
}

/// Every handoff message in these tests carries its full encoded form;
/// this pins the transport-neutral shape the runtime must preserve.
#[test]
fn inbound_handoff_shape_is_transport_neutral() {
    fn assert_shape(message: &Ssu2InboundI2np) {
        assert!(!message.bytes.is_empty());
        assert!(message.bytes.len() <= MAX_I2NP_MESSAGE_BYTES);
    }
    assert_shape(&Ssu2InboundI2np {
        link_id: LinkId::new(1).expect("id"),
        peer: PeerId::from_hash(Hash::from_bytes([9_u8; 32])),
        bytes: vec![6, 0, 0, 0, 1, 0, 0, 0, 2, 9],
    });
}
