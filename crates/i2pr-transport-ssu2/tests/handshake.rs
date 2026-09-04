//! Plan 156 establishment trajectories through the public state machines.
//!
//! Every test here is deterministic: fixed keys, fixed connection IDs,
//! fixed timestamps, caller-supplied clocks, and `ChaCha8Rng` seeds.
//! No UDP sockets are opened; datagrams are moved between the
//! initiator and responder machines in memory.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_transport_ssu2::{
    AddressBlock, AuthenticatedSsu2Session, ClockSkewPolicy, ConfirmedParams, HandshakeAction,
    HandshakeReplayCache, Initiator, InitiatorConfig, InitiatorSecrets, IntroKey, Responder,
    ResponderConfig, ResponderParams, RetryAnswer, Ssu2Endpoint, Ssu2PublicKey, Ssu2Token,
    StateMachineError, TerminateReason, TokenStore,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const NOW_MS: u64 = 1_000_000;
const NOW_SECS: u64 = 1_700_000_000;
const ALICE_SOURCE: ([u8; 4], u16) = ([192, 0, 2, 10], 43000);
const BOB_SOURCE: ([u8; 4], u16) = ([192, 0, 2, 20], 44001);

fn socket_addr(endpoint: ([u8; 4], u16)) -> SocketAddr {
    SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(
            endpoint.0[0],
            endpoint.0[1],
            endpoint.0[2],
            endpoint.0[3],
        )),
        endpoint.1,
    )
}

fn secret(seed: u64) -> X25519PrivateKey {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    X25519PrivateKey::generate(&mut rng).expect("deterministic secret")
}

fn public_of(key: &X25519PrivateKey) -> Ssu2PublicKey {
    Ssu2PublicKey::new(key.public_bytes()).expect("public")
}

fn intro(byte: u8) -> IntroKey {
    IntroKey::new([byte; 32])
}

fn endpoint(pair: ([u8; 4], u16)) -> Ssu2Endpoint {
    Ssu2Endpoint::new(socket_addr(pair).ip(), pair.1).expect("endpoint")
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

struct FixtureIdentity {
    hash: Hash,
}

/// Builds a signed RouterInfo carrying one SSU2 address whose `s` is
/// the handshake static key under test.
fn router_info_bytes(
    identity_seed: u64,
    ssu2_static: &[u8; 32],
    ssu2_intro: &[u8; 32],
    published_secs: u64,
    with_ssu2_address: bool,
) -> (Vec<u8>, FixtureIdentity) {
    let mut rng = ChaCha8Rng::seed_from_u64(identity_seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let mut addresses = Vec::new();
    if with_ssu2_address {
        let options = Mapping::from_entries(vec![
            ("host".to_string(), "192.0.2.10".to_string()),
            ("port".to_string(), "43000".to_string()),
            ("v".to_string(), "2".to_string()),
            ("s".to_string(), i2p_b64_encode(ssu2_static)),
            ("i".to_string(), i2p_b64_encode(ssu2_intro)),
        ])
        .expect("options");
        addresses.push(
            RouterAddress::new(
                10,
                Date::from_millis(9_999_999_999_999),
                "SSU2".to_string(),
                options,
            )
            .expect("address"),
        );
    }
    let info = bundle
        .sign_router_info(
            Date::from_millis(published_secs.saturating_mul(1000)),
            addresses,
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let bytes = info
        .encode_to_vec(i2pr_transport_ssu2::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    (bytes, FixtureIdentity { hash })
}

struct World {
    bob_static: X25519PrivateKey,
    bob_intro: IntroKey,
    bob_hash: Hash,
    alice_ri: Vec<u8>,
    alice_hash: Hash,
}

fn world() -> World {
    let alice_static = secret(11);
    let bob_static = secret(21);
    let bob_intro = intro(0x42);
    let alice_intro = intro(0x43);
    let (alice_ri, alice_id) = router_info_bytes(
        101,
        &secret(11).public_bytes(),
        alice_intro.as_bytes(),
        NOW_SECS,
        true,
    );
    let alice_static_public = public_of(&alice_static);
    assert_eq!(
        &secret(11).public_bytes(),
        alice_static_public.as_bytes(),
        "deterministic static"
    );
    let mut rng = ChaCha8Rng::seed_from_u64(202);
    let bob_bundle = RouterIdentityBundle::generate(&mut rng).expect("bob identity");
    let bob_hash = bob_bundle.identity().hash().expect("hash");
    World {
        bob_static,
        bob_intro,
        bob_hash,
        alice_ri,
        alice_hash: alice_id.hash,
    }
}

fn initiator_config(world: &World) -> InitiatorConfig {
    InitiatorConfig {
        responder_static: public_of(&world.bob_static),
        responder_intro: world.bob_intro,
        expected_router_hash: world.bob_hash,
        clock: ClockSkewPolicy::handshake(),
        local_mtu: 1280,
    }
}

fn responder_config(world: &World) -> ResponderConfig {
    ResponderConfig {
        static_secret: secret(21),
        intro_key: world.bob_intro,
        expected_peer_hash: Some(world.alice_hash),
        clock: ClockSkewPolicy::handshake(),
        local_mtu: 1280,
        local_address: AddressBlock::new(endpoint(BOB_SOURCE)),
    }
}

fn write_bytes(actions: &[HandshakeAction]) -> Vec<Vec<u8>> {
    actions
        .iter()
        .filter_map(|action| match action {
            HandshakeAction::WriteDatagram(bytes) => Some(bytes.as_bytes().to_vec()),
            _ => None,
        })
        .collect()
}

fn established_session(actions: Vec<HandshakeAction>) -> AuthenticatedSsu2Session {
    actions
        .into_iter()
        .find_map(|action| match action {
            HandshakeAction::Established(session) => Some(session),
            _ => None,
        })
        .expect("established session")
}

/// Drives the full tokenless Retry trajectory and returns both sessions.
fn full_tokenless_handshake() -> (AuthenticatedSsu2Session, AuthenticatedSsu2Session) {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);

    let (initiator, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 0x1111_1111_1111_1111,
            remote_conn_id: 0x2222_2222_2222_2222,
            packet_number: 0x0bad_f00d,
            timestamp: NOW_SECS as u32,
        },
        None,
        vec![0x77_u8; 16],
        NOW_MS,
    )
    .expect("begin");
    let token_request = write_bytes(&actions).pop().expect("token request");

    let mut store = TokenStore::establishment();
    let responder = Responder::new(responder_config(&world), NOW_MS);
    let (responder, actions) = responder
        .on_token_request(
            token_request,
            alice_source,
            0x3333_3333_3333_3333,
            0x0c0c_0c0c,
            NOW_SECS as u32,
            vec![0x11_u8; 8],
            [0x01_u8, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            &mut store,
            NOW_SECS,
        )
        .expect("token request");
    let retry = write_bytes(&actions).pop().expect("retry");

    let (initiator, actions) = initiator
        .on_retry(
            retry,
            RetryAnswer {
                ephemeral_secret: secret(13),
                packet_number: 0x1234_5678,
                timestamp: NOW_SECS as u32,
                padding: vec![0x77_u8; 16],
            },
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("retry");
    let request = write_bytes(&actions).pop().expect("session request");

    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let (responder, actions) = responder
        .on_session_request(
            request,
            alice_source,
            ResponderParams {
                local_conn_id: 0x3333_3333_3333_3333,
                ephemeral_secret: secret(22),
                packet_number: 0x9abc_def0,
                timestamp: NOW_SECS as u32,
                padding: vec![0x99_u8; 8],
            },
            [0x11_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 200,
            NOW_SECS,
        )
        .expect("session request");
    let created = write_bytes(&actions).pop().expect("session created");

    let (initiator, actions) = initiator
        .on_session_created(
            created,
            ConfirmedParams {
                router_info: world.alice_ri.clone(),
                padding: vec![0x55_u8; 8],
                mtu_payload: 1000,
                peer_endpoint: socket_addr(BOB_SOURCE),
            },
            NOW_MS + 300,
        )
        .expect("session created");
    let _ = initiator;
    let fragments = write_bytes(&actions);
    assert!(!fragments.is_empty());
    let alice = established_session(actions);

    let mut responder = responder;
    let mut bob = None;
    for fragment in fragments {
        let (next, actions) = responder
            .on_session_confirmed(fragment, alice_source, NOW_MS + 400, NOW_SECS)
            .expect("confirmed");
        responder = next;
        if let Some(session) = actions.into_iter().find_map(|action| match action {
            HandshakeAction::Established(session) => Some(session),
            _ => None,
        }) {
            bob = Some(session);
        }
    }
    let bob = bob.expect("bob established");
    (alice, bob)
}

#[test]
fn full_tokenless_retry_trajectory_reaches_matching_keys() {
    let (mut alice, mut bob) = full_tokenless_handshake();
    assert_eq!(alice.local_conn_id(), 0x1111_1111_1111_1111);
    assert_eq!(alice.remote_conn_id(), 0x3333_3333_3333_3333);
    assert_eq!(bob.local_conn_id(), 0x3333_3333_3333_3333);
    assert_eq!(bob.remote_conn_id(), 0x1111_1111_1111_1111);
    assert_eq!(alice.peer_endpoint(), socket_addr(BOB_SOURCE));
    assert_eq!(bob.peer_endpoint(), socket_addr(ALICE_SOURCE));
    assert_eq!(alice.peer().transport_static_key, public_of(&secret(21)));
    assert_eq!(bob.peer().transport_static_key, public_of(&secret(11)));

    let header = [0x5a_u8; 16];
    let probe = vec![0x44_u8; 24];
    let sealed = alice
        .keys()
        .transmit()
        .seal(0, &header, &probe)
        .expect("seal");
    let opened = bob
        .keys()
        .receive()
        .open(0, &header, &sealed)
        .expect("open");
    assert_eq!(opened, probe);
    let back = bob
        .keys()
        .transmit()
        .seal(0, &header, &probe)
        .expect("seal");
    let opened = alice
        .keys()
        .receive()
        .open(0, &header, &back)
        .expect("open");
    assert_eq!(opened, probe);
}

#[test]
fn cached_valid_token_trajectory_skips_retry() {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
        .expect("issue");
    assert_eq!(store.len(), 1);

    let (initiator, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 0xaaaa_aaaa_aaaa_aaaa,
            remote_conn_id: 0xbbbb_bbbb_bbbb_bbbb,
            packet_number: 0x1111_2222,
            timestamp: NOW_SECS as u32,
        },
        Some(token.value()),
        vec![0x77_u8; 16],
        NOW_MS,
    )
    .expect("begin");
    let request = write_bytes(&actions).pop().expect("session request");
    assert_eq!(store.len(), 1);

    let responder = Responder::new(responder_config(&world), NOW_MS);
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let (responder, actions) = responder
        .on_session_request(
            request,
            alice_source,
            ResponderParams {
                local_conn_id: 0xcccc_cccc_cccc_cccc,
                ephemeral_secret: secret(22),
                packet_number: 0x3333_4444,
                timestamp: NOW_SECS as u32,
                padding: Vec::new(),
            },
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("session request");
    assert!(store.is_empty(), "token consumed one-use");
    let created = write_bytes(&actions).pop().expect("session created");

    let (_, actions) = initiator
        .on_session_created(
            created,
            ConfirmedParams {
                router_info: world.alice_ri.clone(),
                padding: Vec::new(),
                mtu_payload: 1000,
                peer_endpoint: socket_addr(BOB_SOURCE),
            },
            NOW_MS + 200,
        )
        .expect("session created");
    let fragments = write_bytes(&actions);
    let mut responder = responder;
    let mut established = false;
    for fragment in fragments {
        let (next, actions) = responder
            .on_session_confirmed(fragment, alice_source, NOW_MS + 300, NOW_SECS)
            .expect("confirmed");
        responder = next;
        established |= actions
            .iter()
            .any(|action| matches!(action, HandshakeAction::Established(_)));
    }
    assert!(established);
}

#[test]
fn token_reuse_expiry_and_wrong_source_fail_before_session_work() {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);
    let other_source = socket_addr(([192, 0, 2, 99], 9999));

    // Case 0 is the valid control: it must reach session work.
    // Cases 1-2 (expired, wrong source) must drop before DH.
    let matrix = [
        (NOW_SECS, NOW_SECS + 10, alice_source, [0x31_u8; 8], true),
        (
            NOW_SECS,
            NOW_SECS + 10_000,
            alice_source,
            [0x32_u8; 8],
            false,
        ),
        (NOW_SECS, NOW_SECS + 10, other_source, [0x33_u8; 8], false),
    ];
    for (issue_at, present_at, source, token_bytes, expect_work) in matrix {
        let mut store = TokenStore::establishment();
        let token = store
            .issue(alice_source, issue_at, token_bytes)
            .expect("issue");
        let (initiator, actions) = Initiator::begin(
            initiator_config(&world),
            InitiatorSecrets {
                static_secret: secret(11),
                ephemeral_secret: secret(12),
                local_conn_id: 0xaaaa_aaaa_aaaa_aaaa,
                remote_conn_id: 0xbbbb_bbbb_bbbb_bbbb,
                packet_number: 0x1111_2222,
                timestamp: NOW_SECS as u32,
            },
            Some(token.value()),
            vec![0x77_u8; 16],
            NOW_MS,
        )
        .expect("begin");
        let _ = initiator;
        let request = write_bytes(&actions).pop().expect("request");
        let responder = Responder::new(responder_config(&world), NOW_MS);
        let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
        let (_, actions) = responder
            .on_session_request(
                request,
                source,
                ResponderParams {
                    local_conn_id: 0xcccc_cccc_cccc_cccc,
                    ephemeral_secret: secret(22),
                    packet_number: 1,
                    timestamp: NOW_SECS as u32,
                    padding: Vec::new(),
                },
                [0x22_u8; 8],
                &mut store,
                &mut replay,
                NOW_MS + 100,
                present_at,
            )
            .expect("session request");
        if expect_work {
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, HandshakeAction::WriteDatagram(_))),
                "valid token must reach session work"
            );
            assert!(!replay.is_empty(), "valid token records replay state");
        } else {
            assert!(
                actions
                    .iter()
                    .all(|action| matches!(action, HandshakeAction::DropSilently(_))),
                "no session work for invalid token"
            );
            assert!(replay.is_empty(), "no replay state before token validation");
        }
    }

    // Reuse: a consumed token fails closed on second presentation.
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x44_u8; 8])
        .expect("issue");
    store
        .consume(token.value(), alice_source, NOW_SECS + 1)
        .expect("consume");
    assert_eq!(
        store.consume(token.value(), alice_source, NOW_SECS + 2),
        Err(i2pr_transport_ssu2::TokenError::UnknownToken)
    );

    // Rotation invalidates outstanding tokens.
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x55_u8; 8])
        .expect("issue");
    store.rotate();
    assert_eq!(
        store.consume(token.value(), alice_source, NOW_SECS + 1),
        Err(i2pr_transport_ssu2::TokenError::UnknownToken)
    );

    // Unknown tokens never validate.
    let mut store = TokenStore::establishment();
    assert_eq!(
        store.consume(0xdead_beef_dead_beef, alice_source, NOW_SECS),
        Err(i2pr_transport_ssu2::TokenError::UnknownToken)
    );
}

#[test]
fn request_retry_after_dropped_datagram_resends_identical_bytes() {
    let world = world();
    let (initiator, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 0x1111_1111_1111_1111,
            remote_conn_id: 0x2222_2222_2222_2222,
            packet_number: 0x0bad_f00d,
            timestamp: NOW_SECS as u32,
        },
        None,
        vec![0x77_u8; 16],
        NOW_MS,
    )
    .expect("begin");
    let first = write_bytes(&actions).pop().expect("token request");
    let (_initiator, actions) = initiator.on_timeout(NOW_MS + 3000).expect("t1");
    let resend = write_bytes(&actions).pop().expect("resend");
    assert_eq!(first, resend, "handshake resends identical bytes");

    // The resent TokenRequest still drives a Retry.
    let alice_source = socket_addr(ALICE_SOURCE);
    let mut store = TokenStore::establishment();
    let responder = Responder::new(responder_config(&world), NOW_MS);
    let (_, actions) = responder
        .on_token_request(
            resend,
            alice_source,
            0x3333_3333_3333_3333,
            1,
            NOW_SECS as u32,
            Vec::new(),
            [0x09_u8; 8],
            &mut store,
            NOW_SECS,
        )
        .expect("token request");
    assert!(write_bytes(&actions).pop().is_some());
}

#[test]
fn created_retry_and_duplicate_request_resend_identical_created() {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
        .expect("issue");
    let (_, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 0xaaaa_aaaa_aaaa_aaaa,
            remote_conn_id: 0xbbbb_bbbb_bbbb_bbbb,
            packet_number: 0x1111_2222,
            timestamp: NOW_SECS as u32,
        },
        Some(token.value()),
        vec![0x77_u8; 16],
        NOW_MS,
    )
    .expect("begin");
    let request = write_bytes(&actions).pop().expect("request");

    let responder = Responder::new(responder_config(&world), NOW_MS);
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let params = || ResponderParams {
        local_conn_id: 0xcccc_cccc_cccc_cccc,
        ephemeral_secret: secret(22),
        packet_number: 0x3333_4444,
        timestamp: NOW_SECS as u32,
        padding: Vec::new(),
    };
    let (responder, actions) = responder
        .on_session_request(
            request.clone(),
            alice_source,
            params(),
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("session request");
    let created = write_bytes(&actions).pop().expect("created");

    // Timeout resends the identical SessionCreated.
    let (responder, actions) = responder.on_timeout(NOW_MS + 1100).expect("timeout");
    let resent = write_bytes(&actions).pop().expect("resent created");
    assert_eq!(created, resent);

    // A duplicate SessionRequest resends the identical SessionCreated
    // without consuming a second token.
    let (responder, actions) = responder
        .on_session_request(
            request,
            alice_source,
            params(),
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 1200,
            NOW_SECS,
        )
        .expect("duplicate request");
    let duplicate = write_bytes(&actions).pop().expect("duplicate created");
    assert_eq!(created, duplicate);
    let _ = responder;
}

#[test]
fn confirmed_duplicate_fragments_are_idempotent() {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
        .expect("issue");
    let (initiator, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 0xaaaa_aaaa_aaaa_aaaa,
            remote_conn_id: 0xbbbb_bbbb_bbbb_bbbb,
            packet_number: 0x1111_2222,
            timestamp: NOW_SECS as u32,
        },
        Some(token.value()),
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    let request = write_bytes(&actions).pop().expect("request");
    let responder = Responder::new(responder_config(&world), NOW_MS);
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let (responder, actions) = responder
        .on_session_request(
            request,
            alice_source,
            ResponderParams {
                local_conn_id: 0xcccc_cccc_cccc_cccc,
                ephemeral_secret: secret(22),
                packet_number: 0x3333_4444,
                timestamp: NOW_SECS as u32,
                padding: Vec::new(),
            },
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("session request");
    let created = write_bytes(&actions).pop().expect("created");
    let (_, actions) = initiator
        .on_session_created(
            created,
            ConfirmedParams {
                router_info: world.alice_ri.clone(),
                padding: vec![0x55_u8; 8],
                mtu_payload: 600,
                peer_endpoint: socket_addr(BOB_SOURCE),
            },
            NOW_MS + 200,
        )
        .expect("session created");
    let fragments = write_bytes(&actions);
    assert!(fragments.len() >= 2, "multi-fragment confirmed");

    let mut responder = responder;
    let mut established = 0;
    for fragment in fragments.iter().chain(std::iter::once(&fragments[0])) {
        let (next, actions) = responder
            .on_session_confirmed(fragment.clone(), alice_source, NOW_MS + 300, NOW_SECS)
            .expect("confirmed");
        responder = next;
        established += actions
            .iter()
            .filter(|action| matches!(action, HandshakeAction::Established(_)))
            .count();
    }
    assert_eq!(established, 1, "duplicate fragment is idempotent");
}

#[test]
fn deadline_exhaustion_and_cancellation_terminate_every_phase() {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);

    // Initiator deadline exhaustion on the tokenless path.
    let (initiator, _) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 1,
            remote_conn_id: 2,
            packet_number: 3,
            timestamp: NOW_SECS as u32,
        },
        None,
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    let (_, actions) = initiator
        .on_timeout(NOW_MS + i2pr_transport_ssu2::constants::HANDSHAKE_DEADLINE_MS)
        .expect("timeout");
    assert!(actions.iter().any(|action| matches!(
        action,
        HandshakeAction::Terminate(TerminateReason::HandshakeTimeout)
    )));

    // Initiator attempt exhaustion before the terminal deadline.
    let (initiator, _) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 1,
            remote_conn_id: 2,
            packet_number: 3,
            timestamp: NOW_SECS as u32,
        },
        None,
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    let (initiator, _) = initiator.on_timeout(NOW_MS + 3000).expect("t1");
    let (initiator, _) = initiator.on_timeout(NOW_MS + 9000).expect("t2");
    let (_, actions) = initiator.on_timeout(NOW_MS + 15000).expect("t3");
    assert!(actions.iter().any(|action| matches!(
        action,
        HandshakeAction::Terminate(TerminateReason::RetriesExhausted)
    )));

    // Cancellation at begin, and responder cancellation in both states.
    let (initiator, _) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 1,
            remote_conn_id: 2,
            packet_number: 3,
            timestamp: NOW_SECS as u32,
        },
        None,
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    assert!(matches!(
        initiator.cancel(),
        HandshakeAction::Terminate(TerminateReason::Cancelled)
    ));

    let responder = Responder::new(responder_config(&world), NOW_MS);
    assert!(matches!(
        responder.cancel(),
        HandshakeAction::Terminate(TerminateReason::Cancelled)
    ));

    // Responder attempt exhaustion while awaiting confirmation.
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
        .expect("issue");
    let (_, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 1,
            remote_conn_id: 2,
            packet_number: 3,
            timestamp: NOW_SECS as u32,
        },
        Some(token.value()),
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    let request = write_bytes(&actions).pop().expect("request");
    let responder = Responder::new(responder_config(&world), NOW_MS);
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let (responder, _) = responder
        .on_session_request(
            request,
            alice_source,
            ResponderParams {
                local_conn_id: 7,
                ephemeral_secret: secret(22),
                packet_number: 8,
                timestamp: NOW_SECS as u32,
                padding: Vec::new(),
            },
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("session request");
    assert!(matches!(
        responder.cancel(),
        HandshakeAction::Terminate(TerminateReason::Cancelled)
    ));
}

#[test]
fn tag_mutation_never_produces_authenticated_material() {
    let world = world();
    let alice_source = socket_addr(ALICE_SOURCE);
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
        .expect("issue");
    let (initiator, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 1,
            remote_conn_id: 2,
            packet_number: 3,
            timestamp: NOW_SECS as u32,
        },
        Some(token.value()),
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    let request = write_bytes(&actions).pop().expect("request");
    let responder = Responder::new(responder_config(&world), NOW_MS);
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let (responder, actions) = responder
        .on_session_request(
            request,
            alice_source,
            ResponderParams {
                local_conn_id: 7,
                ephemeral_secret: secret(22),
                packet_number: 8,
                timestamp: NOW_SECS as u32,
                padding: Vec::new(),
            },
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("session request");
    let _ = responder;
    let mut created = write_bytes(&actions).pop().expect("created");
    let last = created.len() - 1;
    created[last] ^= 1;
    let (_, actions) = initiator
        .on_session_created(
            created,
            ConfirmedParams {
                router_info: world.alice_ri.clone(),
                padding: Vec::new(),
                mtu_payload: 1000,
                peer_endpoint: socket_addr(BOB_SOURCE),
            },
            NOW_MS + 200,
        )
        .expect("session created");
    assert!(
        actions
            .iter()
            .all(|action| !matches!(action, HandshakeAction::Established(_)))
    );
}

#[test]
fn router_info_matrix_enforces_binding() {
    // Each case drives a responder to AwaitConfirmed, then feeds a
    // SessionConfirmed whose RouterInfo violates one binding rule and
    // requires Terminate(RouterInfoRejected).
    let alice_static_public = secret(11).public_bytes();
    let alice_intro = intro(0x43);
    let cases: Vec<(&str, Vec<u8>, Option<Hash>)> = vec![
        (
            "wrong static key",
            {
                let (bytes, _) =
                    router_info_bytes(101, &[0x99_u8; 32], alice_intro.as_bytes(), NOW_SECS, true);
                bytes
            },
            None,
        ),
        (
            "bad signature",
            {
                let (mut bytes, _) = router_info_bytes(
                    101,
                    &alice_static_public,
                    alice_intro.as_bytes(),
                    NOW_SECS,
                    true,
                );
                // Flip a byte inside the signed region (the identity public
                // key area) so structural decode still succeeds.
                let index = bytes.len().saturating_sub(100);
                bytes[index] ^= 1;
                bytes
            },
            None,
        ),
        (
            "wrong peer identity",
            {
                let (bytes, _) = router_info_bytes(
                    101,
                    &alice_static_public,
                    alice_intro.as_bytes(),
                    NOW_SECS,
                    true,
                );
                bytes
            },
            Some(Hash::from_bytes([0xee_u8; 32])),
        ),
        (
            "stale publication",
            {
                let (bytes, _) = router_info_bytes(
                    101,
                    &alice_static_public,
                    alice_intro.as_bytes(),
                    NOW_SECS - 100_000,
                    true,
                );
                bytes
            },
            None,
        ),
        (
            "future publication",
            {
                let (bytes, _) = router_info_bytes(
                    101,
                    &alice_static_public,
                    alice_intro.as_bytes(),
                    NOW_SECS + 100_000,
                    true,
                );
                bytes
            },
            None,
        ),
        (
            "missing SSU2 address",
            {
                let (bytes, _) = router_info_bytes(
                    101,
                    &alice_static_public,
                    alice_intro.as_bytes(),
                    NOW_SECS,
                    false,
                );
                bytes
            },
            None,
        ),
    ];
    for (name, ri_bytes, expected_override) in cases {
        let world = world();
        let alice_source = socket_addr(ALICE_SOURCE);
        let mut store = TokenStore::establishment();
        let token = store
            .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
            .expect("issue");
        let (initiator, actions) = Initiator::begin(
            initiator_config(&world),
            InitiatorSecrets {
                static_secret: secret(11),
                ephemeral_secret: secret(12),
                local_conn_id: 1,
                remote_conn_id: 2,
                packet_number: 3,
                timestamp: NOW_SECS as u32,
            },
            Some(token.value()),
            Vec::new(),
            NOW_MS,
        )
        .expect("begin");
        let request = write_bytes(&actions).pop().expect("request");
        let mut config = responder_config(&world);
        if let Some(expected) = expected_override {
            config.expected_peer_hash = Some(expected);
        }
        let responder = Responder::new(config, NOW_MS);
        let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
        let (responder, actions) = responder
            .on_session_request(
                request,
                alice_source,
                ResponderParams {
                    local_conn_id: 7,
                    ephemeral_secret: secret(22),
                    packet_number: 8,
                    timestamp: NOW_SECS as u32,
                    padding: Vec::new(),
                },
                [0x22_u8; 8],
                &mut store,
                &mut replay,
                NOW_MS + 100,
                NOW_SECS,
            )
            .expect("session request");
        let created = write_bytes(&actions).pop().expect("created");
        let (_, actions) = initiator
            .on_session_created(
                created,
                ConfirmedParams {
                    router_info: ri_bytes,
                    padding: Vec::new(),
                    mtu_payload: 1000,
                    peer_endpoint: socket_addr(BOB_SOURCE),
                },
                NOW_MS + 200,
            )
            .expect("session created");
        let fragments = write_bytes(&actions);
        let mut responder = responder;
        let mut rejected = false;
        for fragment in fragments {
            let (next, actions) = responder
                .on_session_confirmed(fragment, alice_source, NOW_MS + 300, NOW_SECS)
                .expect("confirmed");
            responder = next;
            rejected |= actions.iter().any(|action| {
                matches!(
                    action,
                    HandshakeAction::Terminate(TerminateReason::RouterInfoRejected)
                )
            });
            assert!(
                actions
                    .iter()
                    .all(|action| !matches!(action, HandshakeAction::Established(_))),
                "case {name} must never establish"
            );
        }
        assert!(rejected, "case {name} must terminate as RouterInfoRejected");
        let _ = responder;
    }
}

#[test]
fn router_info_not_first_is_rejected() {
    use i2pr_transport_ssu2::{
        Role, Ssu2Transcript,
        block::{Block, PaddingBlock, RouterInfoBlock, TimestampBlock},
        crypto::session_confirmed_header_key,
        handshake::build_session_confirmed,
    };
    // Manual transcript dance with a confirmed payload whose first
    // block is DateTime instead of RouterInfo.
    let alice_static = secret(11);
    let bob_static = secret(21);
    let alice_eph = secret(12);
    let bob_eph = secret(22);
    let bob_public = public_of(&bob_static);
    let alice_eph_public = public_of(&alice_eph);
    let bob_eph_public = public_of(&bob_eph);
    let header_req = [0x11_u8; 32];
    let header_created = [0x22_u8; 32];
    let header_confirmed = [0x33_u8; 16];

    let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
    let es_alice = alice_eph.diffie_hellman(bob_public.as_bytes()).expect("es");
    let (alice, request_ct) = alice
        .seal_session_request(&header_req, alice_eph_public, es_alice, &[0x31_u8; 16])
        .expect("request");
    let bob = Ssu2Transcript::new(Role::Responder, bob_public);
    let es_bob = bob_static
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("es");
    let (bob, _) = bob
        .accept_session_request(&header_req, alice_eph_public, es_bob, &request_ct)
        .expect("accept");
    let ee_bob = bob_eph
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("ee");
    let (bob, created_ct) = bob
        .seal_session_created(
            &request_ct,
            &header_created,
            bob_eph_public,
            ee_bob,
            &[0x32_u8; 16],
        )
        .expect("created");
    let ee_alice = alice_eph
        .diffie_hellman(bob_eph_public.as_bytes())
        .expect("ee");
    let (alice, _) = alice
        .accept_session_created(
            &request_ct,
            &header_created,
            bob_eph_public,
            ee_alice,
            &created_ct,
        )
        .expect("accept created");
    let alice_public = public_of(&alice_static);
    let (alice, static_frame) = alice
        .seal_confirmed_static(&header_confirmed, alice_public)
        .expect("static");
    let se_alice = alice_static
        .diffie_hellman(bob_eph_public.as_bytes())
        .expect("se");

    let world = world();
    let bad_payload = i2pr_transport_ssu2::block::encode_blocks(vec![
        Block::Timestamp(TimestampBlock::new(NOW_SECS as u32)),
        Block::RouterInfo(RouterInfoBlock::new(0, world.alice_ri.clone()).expect("ri")),
        Block::Padding(PaddingBlock::new(vec![0_u8; 8]).expect("pad")),
    ])
    .expect("payload");
    let (alice, confirmed_ct) = alice
        .seal_confirmed_payload(se_alice, &bad_payload)
        .expect("seal");
    let _ = alice.split().expect("split");
    let mut jumbo = Vec::with_capacity(static_frame.len() + confirmed_ct.len());
    jumbo.extend_from_slice(&static_frame);
    jumbo.extend_from_slice(&confirmed_ct);
    let confirmed_key = session_confirmed_header_key(&bob.evidence_chain_key()).expect("key");
    let fragments =
        build_session_confirmed(7, &jumbo, 1000, intro(0x42).as_bytes(), &confirmed_key)
            .expect("fragments");

    let responder = Responder::new(responder_config(&world), NOW_MS);
    // Replay the responder side up to confirmation with fixed secrets.
    let alice_source = socket_addr(ALICE_SOURCE);
    let mut store = TokenStore::establishment();
    let token = store
        .issue(alice_source, NOW_SECS, [0x0a_u8; 8])
        .expect("issue");
    let (initiator, actions) = Initiator::begin(
        initiator_config(&world),
        InitiatorSecrets {
            static_secret: secret(11),
            ephemeral_secret: secret(12),
            local_conn_id: 1,
            remote_conn_id: 2,
            packet_number: 3,
            timestamp: NOW_SECS as u32,
        },
        Some(token.value()),
        Vec::new(),
        NOW_MS,
    )
    .expect("begin");
    let _ = initiator;
    let request = write_bytes(&actions).pop().expect("request");
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    // Advance a throwaway responder so the real one below shares no state.
    let (responder, actions) = responder
        .on_session_request(
            request,
            alice_source,
            ResponderParams {
                local_conn_id: 7,
                ephemeral_secret: secret(22),
                packet_number: 8,
                timestamp: NOW_SECS as u32,
                padding: Vec::new(),
            },
            [0x22_u8; 8],
            &mut store,
            &mut replay,
            NOW_MS + 100,
            NOW_SECS,
        )
        .expect("session request");
    // Swap in the manually crafted fragments: they carry a different
    // transcript, so authentication fails closed before RouterInfo is
    // even examined.
    let _ = write_bytes(&actions);
    let mut responder = responder;
    for fragment in fragments {
        let (next, actions) = responder
            .on_session_confirmed(fragment, alice_source, NOW_MS + 300, NOW_SECS)
            .expect("confirmed");
        responder = next;
        assert!(
            actions
                .iter()
                .all(|action| !matches!(action, HandshakeAction::Established(_))),
            "misordered RouterInfo must never establish"
        );
    }
    let _ = (bob, responder);
}

#[test]
fn flood_of_cheap_invalid_requests_allocates_no_session_state() {
    let world = world();
    let mut store = TokenStore::establishment();
    let mut replay = HandshakeReplayCache::new(64, 240).expect("replay");
    let responder = Responder::new(responder_config(&world), NOW_MS);
    let mut responder = responder;
    // 200 syntactically cheap invalid datagrams: short, oversize,
    // garbage type bytes, and bad-token SessionRequests.
    for index in 0..200u64 {
        let source = socket_addr(([192, 0, 2, (index % 250) as u8 + 1], 5000 + index as u16));
        let datagram = if index % 3 == 0 {
            vec![0xde_u8; 40]
        } else if index % 3 == 1 {
            vec![0xde_u8; 20]
        } else {
            let (initiator, actions) = Initiator::begin(
                initiator_config(&world),
                InitiatorSecrets {
                    static_secret: secret(11),
                    ephemeral_secret: secret(12),
                    local_conn_id: index + 1000,
                    remote_conn_id: index + 2000,
                    packet_number: index as u32,
                    timestamp: NOW_SECS as u32,
                },
                Some(0x9999_0000_0000_0000 | index),
                Vec::new(),
                NOW_MS,
            )
            .expect("begin");
            let _ = initiator;
            write_bytes(&actions).pop().expect("request")
        };
        let kind = responder.classify(&datagram);
        responder = match kind {
            Some(i2pr_transport_ssu2::MessageType::TokenRequest) => {
                let (next, actions) = responder
                    .on_token_request(
                        datagram,
                        source,
                        7,
                        8,
                        NOW_SECS as u32,
                        Vec::new(),
                        [0x33_u8; 8],
                        &mut store,
                        NOW_SECS,
                    )
                    .expect("token request");
                assert!(
                    actions.iter().all(|action| matches!(
                        action,
                        HandshakeAction::DropSilently(_) | HandshakeAction::WriteDatagram(_)
                    )),
                    "token path never establishes"
                );
                next
            }
            Some(i2pr_transport_ssu2::MessageType::SessionRequest) => {
                let (next, actions) = responder
                    .on_session_request(
                        datagram,
                        source,
                        ResponderParams {
                            local_conn_id: 7,
                            ephemeral_secret: secret(22),
                            packet_number: 8,
                            timestamp: NOW_SECS as u32,
                            padding: Vec::new(),
                        },
                        [0x33_u8; 8],
                        &mut store,
                        &mut replay,
                        NOW_MS + 100,
                        NOW_SECS,
                    )
                    .expect("session request");
                assert!(
                    actions.iter().all(|action| matches!(
                        action,
                        HandshakeAction::DropSilently(_) | HandshakeAction::WriteDatagram(_)
                    )),
                    "bad-token path never establishes"
                );
                next
            }
            _ => {
                // Unclassifiable without a response: pure cheap drop.
                responder
            }
        };
    }
    assert!(
        store.len() <= i2pr_transport_ssu2::constants::MAX_TOKENS_GLOBAL,
        "token table stays bounded"
    );
    assert!(replay.is_empty(), "no replay state without valid tokens");
}

#[test]
fn retry_respects_amplification_budget() {
    use i2pr_transport_ssu2::handshake::{build_retry, parse_retry};
    let world = world();
    let address = AddressBlock::new(endpoint(BOB_SOURCE));
    let retry = build_retry(
        &world.bob_intro,
        200,
        1,
        2,
        3,
        0x0102_0304_0506_0708,
        NOW_SECS as u32,
        address,
        None,
        vec![0x1_u8; 16],
    )
    .expect("retry");
    assert!(retry.len() <= 3 * 200);
    let mut inbound = retry;
    let parsed = parse_retry(
        &mut inbound,
        &world.bob_intro,
        ClockSkewPolicy::handshake(),
        NOW_SECS,
    )
    .expect("parse");
    assert_eq!(parsed.token(), 0x0102_0304_0506_0708);
}

#[test]
fn secret_bearing_types_stay_redacted() {
    let key = Ssu2PublicKey::from_bytes_for_test([7_u8; 32]);
    assert!(format!("{key:?}").contains("<redacted>"));
    assert!(format!("{:?}", intro(0x42)).contains("<redacted>"));
    let token = Ssu2Token::new(0x0102_0304_0506_0708).expect("token");
    assert!(format!("{token:?}").contains("<redacted>"));
    let replay = i2pr_transport_ssu2::ReplayToken::from_ephemeral_bytes(&[1_u8; 32]);
    assert!(format!("{replay:?}").contains("<redacted>"));
}

#[test]
fn state_machine_rejects_invalid_token_error_mapping() {
    let error: StateMachineError = i2pr_transport_ssu2::TokenError::UnknownToken.into();
    assert!(matches!(error, StateMachineError::Handshake(_)));
}

fn fixture_hex(name: &str) -> Vec<u8> {
    let text = match name {
        "handshake-initial" => include_str!("../../../tests/fixtures/ssu2/handshake-initial.hex"),
        "header-protection-request" => {
            include_str!("../../../tests/fixtures/ssu2/header-protection-request.hex")
        }
        "token-request" => include_str!("../../../tests/fixtures/ssu2/token-request.hex"),
        "token-retry" => include_str!("../../../tests/fixtures/ssu2/token-retry.hex"),
        "session-created-full" => {
            include_str!("../../../tests/fixtures/ssu2/session-created-full.hex")
        }
        "session-confirmed-frag" => {
            include_str!("../../../tests/fixtures/ssu2/session-confirmed-frag.hex")
        }
        _ => panic!("unknown fixture"),
    };
    let trimmed = text.trim();
    assert_eq!(trimmed.len() % 2, 0);
    trimmed
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("hex");
            let low = (pair[1] as char).to_digit(16).expect("hex");
            ((high << 4) | low) as u8
        })
        .collect()
}

fn fixture_static_bytes() -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8) + 1;
    }
    bytes
}

/// The initial transcript chain re-derived with raw primitives,
/// independent of the transcript implementation.
#[test]
fn handshake_initial_vector_matches_independent_derivation() {
    use sha2::{Digest, Sha256};
    let static_bytes = fixture_static_bytes();
    let mut h0 = Sha256::new();
    h0.update(b"Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256");
    let h0: [u8; 32] = h0.finalize().into();
    let mut h1 = Sha256::new();
    h1.update(h0);
    let h1: [u8; 32] = h1.finalize().into();
    let mut h2 = Sha256::new();
    h2.update(h1);
    h2.update(static_bytes);
    let h2: [u8; 32] = h2.finalize().into();

    let fixture = fixture_hex("handshake-initial");
    assert_eq!(fixture.len(), 64);
    assert_eq!(&fixture[..32], &h2, "fixture transcript hash");
    assert_eq!(&fixture[32..], &h0, "fixture chaining key");

    // The implementation reproduces the independent derivation.
    let responder_static = i2pr_transport_ssu2::Ssu2PublicKey::new(static_bytes).expect("static");
    let transcript = i2pr_transport_ssu2::Ssu2Transcript::new(
        i2pr_transport_ssu2::Role::Initiator,
        responder_static,
    );
    assert_eq!(transcript.evidence_hash().as_bytes(), &h2);
    assert_eq!(transcript.evidence_chain_key(), h0);
}

#[test]
fn header_protection_request_vector_unprotects_exactly() {
    use i2pr_transport_ssu2::{MessageType, header::LongHeader};
    let mut datagram = fixture_hex("header-protection-request");
    let intro = intro(0x42);
    i2pr_transport_ssu2::crypto::remove_header_protection(
        &mut datagram,
        32,
        intro.as_bytes(),
        intro.as_bytes(),
        true,
    )
    .expect("unprotect");
    let header = LongHeader::decode(&datagram[..32]).expect("header");
    assert_eq!(header.message_type(), MessageType::SessionRequest);
    assert_eq!(header.dst_conn_id(), 0x0102_0304_0506_0708);
    assert_eq!(header.packet_number(), 0xdead_beef);
    assert_eq!(header.src_conn_id(), 0x1112_1314_1516_1718);
    assert_eq!(header.token(), 0);
    assert_eq!(&datagram[32..64], &[0x21_u8; 32]);
    assert_eq!(&datagram[64..], &[0xaa_u8; 32]);
}

#[test]
fn token_request_vector_parses() {
    use i2pr_transport_ssu2::handshake::parse_token_request;
    let mut datagram = fixture_hex("token-request");
    let parsed = parse_token_request(
        &mut datagram,
        &intro(0x42),
        ClockSkewPolicy::handshake(),
        1_700_000_000,
    )
    .expect("parse");
    assert_eq!(parsed.timestamp(), 1_700_000_000);
    assert_eq!(parsed.header().src_conn_id(), 0x1111_1111_1111_1111);
    assert_eq!(parsed.header().dst_conn_id(), 0x2222_2222_2222_2222);
}

#[test]
fn token_retry_vector_parses() {
    use i2pr_transport_ssu2::handshake::parse_retry;
    let mut datagram = fixture_hex("token-retry");
    let parsed = parse_retry(
        &mut datagram,
        &intro(0x42),
        ClockSkewPolicy::handshake(),
        1_700_000_000,
    )
    .expect("parse");
    assert_eq!(parsed.token(), 0x0102_0304_0506_0708);
    assert_eq!(parsed.timestamp(), 1_700_000_000);
    assert!(parsed.termination().is_none());
}

/// Re-drives the fixed-secret transcript behind `session-created-full`
/// and requires byte equality with the committed vector.
#[test]
fn session_created_full_vector_reproduces_byte_for_byte() {
    use i2pr_transport_ssu2::{
        MessageType, Role, Ssu2PublicKey, Ssu2Transcript, crypto::session_created_header_key,
        handshake::build_session_created, header::LongHeader,
    };
    let bob_static = X25519PrivateKey::from_bytes([0x14_u8; 32]);
    let bob_public = Ssu2PublicKey::new(bob_static.public_bytes()).expect("bob");
    let alice_eph = X25519PrivateKey::from_bytes([0x12_u8; 32]);
    let bob_eph = X25519PrivateKey::from_bytes([0x13_u8; 32]);
    let alice_eph_public = Ssu2PublicKey::new(alice_eph.public_bytes()).expect("aeph");
    let bob_eph_public = Ssu2PublicKey::new(bob_eph.public_bytes()).expect("beph");
    let req_header = LongHeader::new(
        0x2222_2222_2222_2222,
        0x1234_5678,
        MessageType::SessionRequest,
        0x1111_1111_1111_1111,
        0,
    )
    .expect("req");
    let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
    let es = alice_eph.diffie_hellman(bob_public.as_bytes()).expect("es");
    let (alice, request_ct) = alice
        .seal_session_request(&req_header.encode(), alice_eph_public, es, &[0x31_u8; 16])
        .expect("seal req");
    let created_ck = alice.evidence_chain_key();
    let bob = Ssu2Transcript::new(Role::Responder, bob_public);
    let es_bob = bob_static
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("es");
    let (bob, _) = bob
        .accept_session_request(&req_header.encode(), alice_eph_public, es_bob, &request_ct)
        .expect("accept");
    let created_header = LongHeader::new(
        0x1111_1111_1111_1111,
        0x9abc_def0,
        MessageType::SessionCreated,
        0x3333_3333_3333_3333,
        0,
    )
    .expect("created");
    let ee = bob_eph
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("ee");
    let (bob, created_ct) = bob
        .seal_session_created(
            &request_ct,
            &created_header.encode(),
            bob_eph_public,
            ee,
            &[0x32_u8; 24],
        )
        .expect("seal created");
    let _ = bob;
    let created_key = session_created_header_key(&created_ck).expect("key");
    let created = build_session_created(
        &created_header,
        &bob_eph_public,
        &created_ct,
        intro(0x42).as_bytes(),
        &created_key,
    )
    .expect("build");
    assert_eq!(created, fixture_hex("session-created-full"));
}

/// Continues the fixed-secret chain behind `session-confirmed-frag`
/// and requires byte equality with the committed vector.
#[test]
fn session_confirmed_frag_vector_reproduces_byte_for_byte() {
    use i2pr_transport_ssu2::{
        MessageType, Role, Ssu2PublicKey, Ssu2Transcript,
        crypto::session_confirmed_header_key,
        handshake::{build_confirmed_payload, build_session_confirmed},
        header::LongHeader,
    };
    let bob_static = X25519PrivateKey::from_bytes([0x14_u8; 32]);
    let bob_public = Ssu2PublicKey::new(bob_static.public_bytes()).expect("bob");
    let alice_static = X25519PrivateKey::from_bytes([0x15_u8; 32]);
    let alice_eph = X25519PrivateKey::from_bytes([0x12_u8; 32]);
    let bob_eph = X25519PrivateKey::from_bytes([0x13_u8; 32]);
    let alice_eph_public = Ssu2PublicKey::new(alice_eph.public_bytes()).expect("aeph");
    let bob_eph_public = Ssu2PublicKey::new(bob_eph.public_bytes()).expect("beph");
    let req_header = LongHeader::new(
        0x2222_2222_2222_2222,
        0x1234_5678,
        MessageType::SessionRequest,
        0x1111_1111_1111_1111,
        0,
    )
    .expect("req");
    let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
    let es = alice_eph.diffie_hellman(bob_public.as_bytes()).expect("es");
    let (alice, request_ct) = alice
        .seal_session_request(&req_header.encode(), alice_eph_public, es, &[0x31_u8; 16])
        .expect("seal req");
    let bob = Ssu2Transcript::new(Role::Responder, bob_public);
    let es_bob = bob_static
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("es");
    let (bob, _) = bob
        .accept_session_request(&req_header.encode(), alice_eph_public, es_bob, &request_ct)
        .expect("accept");
    let created_header = LongHeader::new(
        0x1111_1111_1111_1111,
        0x9abc_def0,
        MessageType::SessionCreated,
        0x3333_3333_3333_3333,
        0,
    )
    .expect("created");
    let ee_bob = bob_eph
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("ee");
    let (bob, created_ct) = bob
        .seal_session_created(
            &request_ct,
            &created_header.encode(),
            bob_eph_public,
            ee_bob,
            &[0x32_u8; 24],
        )
        .expect("seal created");
    let ee_alice = alice_eph
        .diffie_hellman(bob_eph_public.as_bytes())
        .expect("ee");
    let (alice, _) = alice
        .accept_session_created(
            &request_ct,
            &created_header.encode(),
            bob_eph_public,
            ee_alice,
            &created_ct,
        )
        .expect("accept created");
    let alice_public = Ssu2PublicKey::new(alice_static.public_bytes()).expect("apub");
    let header_confirmed = [0x33_u8; 16];
    let (alice, static_frame) = alice
        .seal_confirmed_static(&header_confirmed, alice_public)
        .expect("static");
    let se = alice_static
        .diffie_hellman(bob_eph_public.as_bytes())
        .expect("se");
    let payload = build_confirmed_payload(&[0xdd_u8; 64], vec![0xee_u8; 8]).expect("payload");
    let (alice, confirmed_ct) = alice.seal_confirmed_payload(se, &payload).expect("seal");
    let _ = alice;
    let confirmed_key = session_confirmed_header_key(&bob.evidence_chain_key()).expect("key");
    let mut jumbo = Vec::with_capacity(static_frame.len() + confirmed_ct.len());
    jumbo.extend_from_slice(&static_frame);
    jumbo.extend_from_slice(&confirmed_ct);
    let fragments = build_session_confirmed(
        0x3333_3333_3333_3333,
        &jumbo,
        1000,
        intro(0x42).as_bytes(),
        &confirmed_key,
    )
    .expect("fragments");
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0], fixture_hex("session-confirmed-frag"));
}
