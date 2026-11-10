//! Plan 530's lane — the router must record its own bound address.
//!
//! The transport's reachability model says `Reachable` needs **three distinct
//! supporting classes**. Two of them had ordinary producers: Plan 529's
//! handshake observation (class 1) and Plan 521's peer test (class 3). The
//! third — the router's own configured bind (class 0) — had exactly **one**
//! recorder, and the tree's only caller was the controlled floodfill
//! activation. So a router running in ordinary operation could never publish a
//! direct SSU2 address, however many peers observed it and however many peer
//! tests confirmed.
//!
//! The lane drives the **ordinary** path — a daemon-owned SSU2 service, bound
//! to a real loopback socket, with the daemon's existing opt-in — and proves:
//!
//! 1. the **ordinary** bind records the explicit-bind class: one corroborating
//!    class, and a state far below the floor;
//! 2. the recorder is **silent** without the opt-in: no class at all, and the
//!    state stays `Unknown`;
//! 3. the bind and the peer's observed external address yield **exactly** the
//!    corroboration floor — `CandidateReachable`, never `Reachable`;
//! 4. the bind, the observation and a **confirmed** peer test together reach
//!    `Reachable`, and the direct publication material is produced;
//! 5. a **wildcard** bind names no address and records nothing;
//! 6. the **corroboration** floor is unchanged;
//! 7. the transport's reachability **model** is untouched.
//!
//! The lane proves the corroboration **structure** -- how many distinct
//! classes the router holds, and what state that earns. It deliberately does
//! not name the class: the runtime's public surface carries a corroboration
//! **count**, never a class list, because the transport's model is the
//! transport's own. The **guard** is the authority for the class itself --
//! that the recorder writes `LocalConfiguredBind`, and that the transport's
//! class table maps it to class 0, distinct from the observation's class and
//! the peer test's. Together the two say that the ordinary bind contributes
//! class 0 and that the three classes of the triple are distinct.
//!
//! The lane asserts counts, states and forms only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::peer_test::{PeerTestInbound, PeerTestOwner};
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, Ssu2DaemonHandle, Ssu2DaemonService,
    decode_authenticated_router_i2np,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationReason, CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial,
    Ssu2PublicationForm, Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2SocketConfig,
};
use i2pr_transport::{AddressFamily, MIN_CORROBORATION_FLOOR, ReachabilityState};
use rand_core::{OsRng, SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// The daemon's existing opt-in: the SSU2 service and the router's own peer
/// test. The peer test is **off** by default; the lane turns it on because it
/// needs the third class.
const ENABLED_CONFIG: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\npeer_test = true\n";

fn wall_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
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
        for _ in 0..(4 - digits) {
            output.push('=');
        }
    }
    output
}

/// One router's own keys and its signed record.
struct LocalKeys {
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: i2pr_runtime::Ssu2PublicKey,
    intro: i2pr_runtime::IntroKey,
    router_info: Vec<u8>,
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
        if intro_bytes.iter().any(|byte| *byte != 0) {
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
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            Mapping::empty(),
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

/// The daemon's peer reference.
type PeerIdOf = i2pr_transport::PeerId;

fn daemon_ssu2_config(text: &str) -> i2pr_daemon::config::Ssu2Config {
    Config::parse(text).expect("strict profile").ssu2
}

/// One daemon-owned SSU2 service: the router's own transport, in the ordinary
/// composition, with the daemon's existing opt-in already mapped.
struct Fixture {
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
    keys: LocalKeys,
}

impl Fixture {
    async fn start(config: &i2pr_daemon::config::Ssu2Config, keys: LocalKeys) -> Self {
        let service = Ssu2DaemonService::new(
            config,
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info.clone(),
            },
        )
        .expect("daemon-owned service");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let handle = service
            .start(
                &scope,
                &i2pr_daemon::config::Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("daemon bind");
        Self {
            handle,
            scope,
            token,
            keys,
        }
    }

    fn bound(&self) -> std::net::SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    fn peer(&self) -> PeerIdOf {
        PeerIdOf::from_hash(self.keys.hash)
    }

    async fn shutdown(self) {
        self.handle.shutdown();
        self.scope.shutdown().await;
        self.token.cancel(CancellationReason::TestHarnessTeardown);
    }
}

/// Starts two daemon-owned services and establishes a session between them.
async fn start_pair() -> (Fixture, Fixture) {
    let config = daemon_ssu2_config(ENABLED_CONFIG);
    let a = Fixture::start(&config, make_local_keys(44_201)).await;
    let b = Fixture::start(&config, make_local_keys(44_202)).await;
    let target = i2pr_runtime::Ssu2DialTarget::new(
        b.peer(),
        b.keys.hash,
        b.bound(),
        b.keys.static_public,
        b.keys.intro,
    )
    .expect("dial target");
    a.handle
        .service()
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial establishes");
    wait_for_active(&a.handle, 1).await;
    wait_for_active(&b.handle, 1).await;
    (a, b)
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

/// Receives one inbound message, or fails the lane.
async fn next_inbound(handle: &mut Ssu2DaemonHandle) -> i2pr_runtime::Ssu2InboundI2np {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if let Some(inbound) = handle.next_inbound().await {
            return inbound;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no inbound message arrived"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Delivers one encoded message to a peer over an established session.
fn deliver(handle: &Ssu2DaemonHandle, peer: PeerIdOf, bytes: Vec<u8>) -> RouterDeliveryOutcome {
    let request = RouterDeliveryRequest::new(peer, bytes, DELIVERY_TIMEOUT).expect("request");
    handle
        .delivery()
        .deliver(request, &CancellationToken::new())
}

/// A deterministic source of probe identifiers for the lane.
fn lane_rng() -> rand_chacha::ChaCha8Rng {
    rand_chacha::ChaCha8Rng::from_seed([0x53; 32])
}

/// A runtime-owned service with the **runtime's own** default configuration,
/// whose `explicit_bind_corroboration` is `false`.
async fn runtime_service_with(config: Ssu2RuntimeConfig, ipv4: &str) -> Ssu2RuntimeService {
    let keys = make_local_keys(44_301);
    let service = Ssu2RuntimeService::new(
        config,
        Ssu2IdentityMaterial {
            router_hash: keys.hash,
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .expect("runtime service");
    let token: &'static CancellationToken = Box::leak(Box::new(CancellationToken::new()));
    let scope = ChildScope::for_test(token, ChildFailurePolicy::FailParent);
    service
        .start(
            &scope,
            Ssu2SocketConfig {
                ipv4: Some(ipv4.parse().expect("bind")),
                ipv6: None,
            },
        )
        .await
        .expect("bind");
    service
}

// ------------------------------------------------------------ the recorder

#[tokio::test]
async fn the_ordinary_bind_records_the_explicit_bind_class() {
    let config = daemon_ssu2_config(ENABLED_CONFIG);
    let fixture = Fixture::start(&config, make_local_keys(44_401)).await;

    let evidence = fixture.handle.service().reachability_evidence();
    assert_eq!(
        evidence.corroboration, 1,
        "the router's own bound socket is exactly one corroborating class"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::ObservedUnconfirmed,
        "one class is far below the floor"
    );
    assert_eq!(evidence.family, AddressFamily::Ipv4);
    assert!(
        evidence.expires_at > Duration::ZERO,
        "the record must carry an expiry"
    );

    fixture.shutdown().await;
}

#[tokio::test]
async fn the_recorder_is_silent_without_the_opt_in() {
    // The runtime's own default leaves `explicit_bind_corroboration` off.
    let service = runtime_service_with(Ssu2RuntimeConfig::default(), "127.0.0.1:0").await;

    let evidence = service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 0,
        "a runtime that did not ask for explicit-bind corroboration records nothing"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::Unknown,
        "without the opt-in the router holds no evidence at all"
    );
}

#[tokio::test]
async fn a_wildcard_bind_names_no_address_and_records_nothing() {
    let config = Ssu2RuntimeConfig {
        explicit_bind_corroboration: true,
        ..Ssu2RuntimeConfig::default()
    };
    let service = runtime_service_with(config, "0.0.0.0:0").await;

    let evidence = service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 0,
        "a wildcard bind names no address, so it is not a configured bind address"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::Unknown,
        "the router claims no address it cannot name"
    );
}

// ------------------------------------------------------------ the triple

#[tokio::test]
async fn the_bind_and_the_peer_observation_yield_exactly_the_floor() {
    let (a, b) = start_pair().await;

    let evidence = a.handle.service().reachability_evidence();
    assert_eq!(
        evidence.corroboration, 2,
        "the bind and the peer's observed external address are two classes"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::CandidateReachable,
        "two classes are exactly the corroboration floor, not above it"
    );
    assert_ne!(
        evidence.state,
        ReachabilityState::Reachable,
        "the floor still needs a third class"
    );

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn the_bind_the_observation_and_the_peer_test_reach_reachable() {
    let (mut a, mut b) = start_pair().await;

    // The router's own peer test: A probes, B answers, A matches the echo.
    let mut owner_a = PeerTestOwner::new(true);
    let mut rng = lane_rng();
    let probe = owner_a
        .start_probe(b.peer(), &mut rng, wall_ms(), wall_ms() + 60_000)
        .expect("probe");
    assert_eq!(
        deliver(&a.handle, b.peer(), probe.bytes.clone()),
        RouterDeliveryOutcome::Accepted
    );

    let inbound = next_inbound(&mut b.handle).await;
    let decoded = decode_authenticated_router_i2np(&inbound, wall_ms()).expect("decode");
    let mut owner_b = PeerTestOwner::new(true);
    let answer = match owner_b.handle_inbound(&decoded.outcome(), 1, wall_ms() + 60_000, wall_ms())
    {
        PeerTestInbound::Answer(answer) => answer,
        other => panic!("expected an answer, got {other:?}"),
    };
    assert_eq!(
        deliver(&b.handle, a.peer(), answer.bytes),
        RouterDeliveryOutcome::Accepted
    );

    let echo = next_inbound(&mut a.handle).await;
    let decoded = decode_authenticated_router_i2np(&echo, wall_ms()).expect("decode");
    let confirmed =
        match owner_a.handle_inbound(&decoded.outcome(), 2, wall_ms() + 60_000, wall_ms()) {
            PeerTestInbound::Completed(confirmed) => confirmed,
            other => panic!("expected a completion, got {other:?}"),
        };

    // The daemon hands the runtime the typed outcome; the runtime decides.
    a.handle
        .service()
        .note_confirmed_peer_test(confirmed.link_id, confirmed.proof)
        .expect("the runtime records the confirmed round trip");

    let evidence = a.handle.service().reachability_evidence();
    assert_eq!(
        evidence.corroboration, 3,
        "the confirmed peer test is the third distinct class"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::Reachable,
        "three distinct supporting classes reach the state the direct form needs"
    );

    // And the direct form is now publishable.
    let material = a
        .handle
        .service()
        .publication_material(wall_ms())
        .expect("qualified publication material");
    assert_eq!(material.form, Ssu2PublicationForm::Direct);
    assert_eq!(material.reachability, ReachabilityState::Reachable);
    let parsed = i2pr_runtime::Ssu2RouterAddress::parse(&material.address).expect("strict address");
    let endpoint = parsed.endpoint().expect("direct endpoint");
    assert_eq!(endpoint.socket_addr(), a.bound());

    a.shutdown().await;
    b.shutdown().await;
}

// ------------------------------------------------------------ the model

#[test]
fn the_corroboration_floor_is_unchanged() {
    assert_eq!(
        MIN_CORROBORATION_FLOOR, 2,
        "the corroboration floor stays at two distinct supporting classes"
    );
    let policy = i2pr_transport::ReachabilityPolicy::default();
    assert_eq!(
        policy.min_corroboration(),
        2,
        "the transport's own default corroboration threshold is untouched"
    );
    assert!(
        !policy.configured_direct_allowed(),
        "the transport's own default still refuses an explicit configuration to corroborate"
    );
}

#[test]
fn the_transports_reachability_model_is_untouched() {
    // The five signal kinds and their five corroboration classes are the
    // transport's, and this plan does not move them.
    use i2pr_transport::{PeerTestOutcomeKind, ReachabilitySignal, ReachabilityTracker};
    let policy = i2pr_transport::ReachabilityPolicy::new(
        2,
        i2pr_transport::MIN_CONTRADICTION_FLOOR,
        i2pr_transport::DEFAULT_OBSERVATION_TTL,
        true,
    );
    let mut tracker = ReachabilityTracker::new(policy).expect("tracker");
    let now = Duration::from_secs(500);
    tracker.record(
        ReachabilitySignal::LocalConfiguredBind {
            family: AddressFamily::Ipv4,
        },
        now,
    );
    tracker.record(
        ReachabilitySignal::AuthenticatedPeerObservedExternalAddress {
            family: AddressFamily::Ipv4,
        },
        now,
    );
    assert_eq!(
        tracker.state(),
        ReachabilityState::CandidateReachable,
        "two classes are the floor, and the model still demands a third"
    );
    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::Confirmed,
        },
        now,
    );
    assert_eq!(
        tracker.state(),
        ReachabilityState::Reachable,
        "the third distinct class is the only thing that reaches the state"
    );
}
