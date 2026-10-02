//! Plan 283 controlled peer-test evidence acceptance.
//!
//! A full three-party peer-test exchange over real loopback UDP
//! promotes Alice from the loopback-homogeneous `CandidateReachable`
//! ceiling to `Reachable` with a direct publication address. Every
//! corroborating fact is wire-real: all seven messages cross real
//! datagrams between three distinct bound sockets, and the verdict
//! carries exactly what Charlie observed.
//!
//! Coverage:
//! - happy path: bind + Address report + Confirmed peer test yields
//!   `Reachable`, direct material for the bound endpoint, and an
//!   installable RouterInfo;
//! - default-config refusal: nothing is recorded or driven without
//!   the controlled permit;
//! - API negatives: role/endpoint binding, single-flight, transport
//!   separation, nonce matching, and no-session cases all fail closed.

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, ControlledPeerTestError,
    ControlledPeerTestOutcome, ControlledPeerTestParams, Ssu2IdentityMaterial, Ssu2RuntimeConfig,
    Ssu2RuntimeService, Ssu2SocketConfig,
};
use i2pr_transport::{PeerId, ReachabilityState};
use i2pr_transport_ssu2::{IntroKey, Ssu2PublicKey, constants};
use rand_core::{OsRng, TryRngCore};

const STEP_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
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

struct AliceKeys {
    bundle: RouterIdentityBundle,
    hash_bytes: [u8; 32],
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
}

fn make_alice_keys() -> AliceKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash_bytes = *bundle.identity().hash().expect("hash").as_bytes();
    let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_key.secret_bytes();
    let static_public = Ssu2PublicKey::new(static_key.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
    if intro_bytes.iter().all(|byte| *byte == 0) {
        intro_bytes[0] = 1;
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
    AliceKeys {
        bundle,
        hash_bytes,
        static_bytes,
        static_public,
        intro,
        router_info,
    }
}

struct AliceFixture {
    service: Ssu2RuntimeService,
    scope_token: CancellationToken,
    scope: ChildScope,
    addr: SocketAddr,
    keys: AliceKeys,
}

async fn start_alice(config: Ssu2RuntimeConfig) -> AliceFixture {
    let keys = make_alice_keys();
    let service = Ssu2RuntimeService::new(
        config,
        Ssu2IdentityMaterial {
            router_hash: Hash::from_bytes(keys.hash_bytes),
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .expect("service");
    let scope_token = CancellationToken::new();
    let scope = ChildScope::for_test(&scope_token, ChildFailurePolicy::FailParent);
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
    let addr = handle.local_v4().expect("bound v4");
    AliceFixture {
        service,
        scope_token,
        scope,
        addr,
        keys,
    }
}

async fn shutdown_alice(fixture: AliceFixture) {
    fixture.service.shutdown();
    let report = fixture.scope.shutdown().await;
    assert!(report.joined() >= 1, "loop task joined");
    drop(fixture.scope_token);
}

fn controlled_config() -> Ssu2RuntimeConfig {
    Ssu2RuntimeConfig {
        explicit_bind_corroboration: true,
        controlled_peer_test: true,
        ..Ssu2RuntimeConfig::default()
    }
}

async fn run_exchange(fixture: &AliceFixture) -> ControlledPeerTestOutcome {
    let alice_sign = |preimage: &[u8]| -> Result<Vec<u8>, ControlledPeerTestError> {
        fixture
            .keys
            .bundle
            .signing_key()
            .sign(preimage)
            .map(|signature| signature.as_bytes().to_vec())
            .map_err(|_| ControlledPeerTestError::ExchangeCrypto)
    };
    let token = CancellationToken::new();
    let params = ControlledPeerTestParams {
        scope: &fixture.scope,
        alice: &fixture.service,
        alice_hash: fixture.keys.hash_bytes,
        alice_addr: fixture.addr,
        alice_static_public: fixture.keys.static_public,
        alice_intro: fixture.keys.intro,
        alice_signing_public: fixture
            .keys
            .bundle
            .signing_key()
            .public_key()
            .expect("alice signing public"),
        alice_sign: &alice_sign,
        cancellation: &token,
        step_timeout: STEP_TIMEOUT,
        poll_interval: POLL_INTERVAL,
    };
    i2pr_runtime::run_controlled_peer_test(params)
        .await
        .expect("controlled exchange confirms")
}

#[tokio::test]
async fn controlled_exchange_promotes_loopback_to_reachable_with_direct_material() {
    let fixture = start_alice(controlled_config()).await;

    // Floor: no evidence, no material.
    assert_eq!(
        fixture.service.snapshot().reachability,
        ReachabilityState::Unknown
    );
    assert!(
        fixture
            .service
            .publication_material(wall_secs().saturating_mul(1000))
            .is_err()
    );

    // Class 0 alone stays below the floor.
    fixture
        .service
        .note_explicit_bind_for_controlled_qualification()
        .expect("explicit bind records");
    assert!(
        fixture
            .service
            .publication_material(wall_secs().saturating_mul(1000))
            .is_err()
    );

    // The exchange adds class 1 (genuine Address report) and class 3
    // (Confirmed peer test): the triple reaches above floor.
    let outcome = run_exchange(&fixture).await;
    match outcome {
        ControlledPeerTestOutcome::Confirmed {
            observed,
            evidence_peers,
            ..
        } => {
            assert_eq!(observed, fixture.addr);
            assert_eq!(evidence_peers, 2);
        }
    }
    assert_eq!(
        fixture.service.snapshot().reachability,
        ReachabilityState::Reachable
    );
    let material = fixture
        .service
        .publication_material(wall_secs().saturating_mul(1000))
        .expect("qualified publication material");
    assert_eq!(material.reachability, ReachabilityState::Reachable);
    let parsed = i2pr_runtime::Ssu2RouterAddress::parse(&material.address).expect("strict address");
    let endpoint = parsed.endpoint().expect("direct endpoint");
    assert_eq!(endpoint.socket_addr(), fixture.addr);
    assert_eq!(
        parsed.static_public_key().as_bytes(),
        fixture.keys.static_public.as_bytes()
    );
    assert_eq!(
        parsed.intro_key().expect("intro key").as_bytes(),
        fixture.keys.intro.as_bytes()
    );

    // The material installs as a live RouterInfo (activation order).
    let options = Mapping::from_entries(vec![
        ("host".to_string(), fixture.addr.ip().to_string()),
        ("port".to_string(), fixture.addr.port().to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(fixture.keys.static_public.as_bytes()),
        ),
        (
            "i".to_string(),
            i2p_b64_encode(fixture.keys.intro.as_bytes()),
        ),
    ])
    .expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let info = fixture
        .keys
        .bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let encoded = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    fixture
        .service
        .install_local_router_info(encoded, wall_secs().saturating_mul(1000))
        .expect("install matching local identity and SSU2 binding");
    assert_eq!(fixture.service.snapshot().local_router_info_generation, 1);

    shutdown_alice(fixture).await;
}

#[tokio::test]
async fn controlled_exchange_is_refused_without_the_permit() {
    let fixture = start_alice(Ssu2RuntimeConfig::default()).await;
    let outcome = {
        let alice_sign = |preimage: &[u8]| -> Result<Vec<u8>, ControlledPeerTestError> {
            fixture
                .keys
                .bundle
                .signing_key()
                .sign(preimage)
                .map(|signature| signature.as_bytes().to_vec())
                .map_err(|_| ControlledPeerTestError::ExchangeCrypto)
        };
        let token = CancellationToken::new();
        let params = ControlledPeerTestParams {
            scope: &fixture.scope,
            alice: &fixture.service,
            alice_hash: fixture.keys.hash_bytes,
            alice_addr: fixture.addr,
            alice_static_public: fixture.keys.static_public,
            alice_intro: fixture.keys.intro,
            alice_signing_public: fixture
                .keys
                .bundle
                .signing_key()
                .public_key()
                .expect("alice signing public"),
            alice_sign: &alice_sign,
            cancellation: &token,
            step_timeout: STEP_TIMEOUT,
            poll_interval: POLL_INTERVAL,
        };
        i2pr_runtime::run_controlled_peer_test(params).await
    };
    assert_eq!(outcome, Err(ControlledPeerTestError::NotPermitted));
    assert_eq!(
        fixture.service.snapshot().reachability,
        ReachabilityState::Unknown
    );
    shutdown_alice(fixture).await;
}

#[tokio::test]
async fn controlled_apis_fail_closed() {
    let fixture = start_alice(controlled_config()).await;
    let peer = PeerId::from_hash(Hash::from_bytes([0x77; 32]));

    // No live test: every call fails without recording.
    assert_eq!(
        fixture.service.register_controlled_peer_signer(
            [0x01; 32],
            &fixture
                .keys
                .bundle
                .signing_key()
                .public_key()
                .expect("public"),
        ),
        Ok(())
    );
    assert_eq!(
        fixture.service.start_controlled_peer_test(
            0,
            i2pr_transport_ssu2::PeerTestRole::Bob,
            fixture.keys.hash_bytes,
            [0x02; 32],
            [0x03; 32],
            i2pr_transport_ssu2::Ssu2Endpoint::new(fixture.addr.ip(), fixture.addr.port())
                .expect("endpoint"),
        ),
        Err(ControlledPeerTestError::BindingMismatch)
    );

    // Start a real Alice test, then exercise the guards around it.
    let alice_endpoint =
        i2pr_transport_ssu2::Ssu2Endpoint::new(fixture.addr.ip(), fixture.addr.port())
            .expect("endpoint");
    fixture
        .service
        .start_controlled_peer_test(
            4242,
            i2pr_transport_ssu2::PeerTestRole::Alice,
            fixture.keys.hash_bytes,
            [0x02; 32],
            [0x03; 32],
            alice_endpoint,
        )
        .expect("start");
    assert_eq!(
        fixture.service.start_controlled_peer_test(
            4243,
            i2pr_transport_ssu2::PeerTestRole::Alice,
            fixture.keys.hash_bytes,
            [0x02; 32],
            [0x03; 32],
            alice_endpoint,
        ),
        Err(ControlledPeerTestError::AlreadyRunning)
    );
    assert_eq!(
        fixture.service.controlled_test_outcome(9999),
        Err(ControlledPeerTestError::NonceMismatch)
    );
    assert_eq!(
        fixture.service.take_controlled_forward(9999),
        Err(ControlledPeerTestError::NonceMismatch)
    );
    assert_eq!(fixture.service.controlled_test_outcome(4242), Ok(None));
    // No session for the peer: nothing to queue onto.
    assert_eq!(
        fixture
            .service
            .queue_controlled_peer_test(peer, peer_test_msg1(&fixture)),
        Err(ControlledPeerTestError::NoSession)
    );
    assert_eq!(
        fixture.service.queue_controlled_address(peer),
        Err(ControlledPeerTestError::NoSession)
    );
    // Msg 1 travels in-session only.
    assert_eq!(
        fixture.service.send_controlled_peer_test(
            fixture.addr,
            &fixture.keys.intro,
            peer_test_msg1(&fixture),
        ),
        Err(ControlledPeerTestError::WrongTransport)
    );
    fixture
        .service
        .finish_controlled_peer_test(4242)
        .expect("finish");
    assert_eq!(
        fixture.service.finish_controlled_peer_test(4242),
        Err(ControlledPeerTestError::NoControlledTest)
    );
    assert_eq!(
        fixture.service.controlled_test_state(4242),
        Err(ControlledPeerTestError::NoControlledTest)
    );
    shutdown_alice(fixture).await;
}

fn peer_test_msg1(fixture: &AliceFixture) -> i2pr_transport_ssu2::PeerTestBlock {
    let endpoint = i2pr_transport_ssu2::Ssu2Endpoint::new(fixture.addr.ip(), fixture.addr.port())
        .expect("endpoint");
    i2pr_transport_ssu2::PeerTestBlock::new(
        1,
        0,
        None,
        2,
        4242,
        1_700_000_000,
        endpoint,
        vec![0xCD; 64],
    )
    .expect("msg1 block")
}
