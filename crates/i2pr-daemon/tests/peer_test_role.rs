//! Plan 521's lane — the router runs its own peer test.
//!
//! The ordinary router can run an I2NP peer test against a peer it already
//! holds a session with, and a **confirmed** exchange records the router's
//! **own** reachability. That is Plan 520's open **Medium** finding **M1**:
//! the published SSU2 address needs three distinct corroboration classes, and
//! the third -- a confirmed peer test -- was recorded in production only by
//! the controlled floodfill activation.
//!
//! The lane drives the **public** behaviour through the daemon's own owner
//! and the runtime's own live service, over a **real** SSU2 session between
//! two daemon-owned services on loopback:
//!
//! 1. the opt-in is **off** by default, and a router with it off sends no
//!    probe;
//! 2. the opt-in requires the SSU2 service, and is refused without it;
//! 3. the responder answers an inbound peer test with the probed fields
//!    echoed **unchanged**, back to the peer that sent it;
//! 4. a probe that is answered is **matched** and completes with the locally
//!    measured elapsed time;
//! 5. a confirmed probe records the router's **own** reachability through the
//!    runtime's own recorder, and the transport's tracker reaches
//!    `Reachable` once the other two classes are present;
//! 6. a probe that is not answered times out, is counted, and records **no**
//!    signal;
//! 7. an unmatched echo matches nothing;
//! 8. the table's ceiling **refuses** a probe rather than evicting one;
//! 9. the responder records nothing about itself;
//! 10. a repeated key is answered at most once, so the reply traffic a peer
//!     can provoke is bounded by the router's own table.
//!
//! The **transport's** corroboration policy is the transport's own and is not
//! changed here: the lane shows that a confirmed peer test supplies the
//! **missing** class, and the transport still decides when three classes are
//! enough.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::peer_test::{
    MAX_OUTSTANDING_PEER_TESTS, MAX_RESPONDED_PEER_TESTS, PeerTestInbound, PeerTestKey,
    PeerTestOwner, PeerTestOwnerError,
};
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, RouterI2npOutcome, Ssu2DaemonHandle,
    Ssu2DaemonService, decode_authenticated_router_i2np,
};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, Mapping, RouterAddress, TunnelTestMessage};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial, Ssu2InboundI2np,
};
use i2pr_transport::{
    AddressFamily, DEFAULT_OBSERVATION_TTL, PeerTestOutcomeKind, ReachabilityPolicy,
    ReachabilitySignal, ReachabilityState, ReachabilityTracker,
};
use rand_core::{OsRng, SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// The opt-in is off by default, so the default tree changes in no way.
const DEFAULT_CONFIG: &str =
    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";

/// The peer test needs the SSU2 service, and the refusal is explicit.
const PEER_TEST_WITHOUT_SERVICE: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = false\npeer_test = true\n";

/// The enabled posture the lane drives.
const PEER_TEST_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\npeer_test = true\n";

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
        for _ in digits..4 {
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

fn daemon_ssu2_config(text: &str) -> i2pr_daemon::config::Ssu2Config {
    Config::parse(text).expect("strict profile").ssu2
}

/// One daemon-owned SSU2 service, the router's own transport.
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
        self.token
            .cancel(i2pr_runtime::CancellationReason::TestHarnessTeardown);
    }
}

/// The daemon's peer reference.
type PeerIdOf = i2pr_transport::PeerId;

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
async fn next_inbound(handle: &mut Ssu2DaemonHandle) -> Ssu2InboundI2np {
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

/// Starts two daemon-owned services and establishes a session between them.
async fn start_pair() -> (Fixture, Fixture) {
    let config = daemon_ssu2_config(PEER_TEST_ENABLED);
    let keys_a = make_local_keys(44_101);
    let keys_b = make_local_keys(44_102);
    let a = Fixture::start(&config, keys_a).await;
    let b = Fixture::start(&config, keys_b).await;
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

/// A deterministic source of probe identifiers for the lane.
fn lane_rng() -> rand_chacha::ChaCha8Rng {
    rand_chacha::ChaCha8Rng::from_seed([0x52; 32])
}

// ---------------------------------------------------------------- the opt-in

#[test]
fn the_opt_in_is_off_by_default() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config");
    assert!(
        !config.ssu2.peer_test,
        "the peer-test switch must default to off"
    );
    let mut owner = PeerTestOwner::new(config.ssu2.peer_test);
    assert!(!owner.enabled());
    let mut rng = lane_rng();
    assert_eq!(
        owner.start_probe(
            PeerIdOf::from_hash(Hash::from_bytes([1; 32])),
            &mut rng,
            wall_ms(),
            wall_ms() + 60_000,
        ),
        Err(PeerTestOwnerError::Disabled),
        "a router with the opt-in off sends no probe"
    );
    assert_eq!(owner.counters().probes_refused_disabled, 1);
    assert_eq!(owner.outstanding(), 0);
}

#[test]
fn the_opt_in_requires_the_ssu2_service() {
    let error = Config::parse(PEER_TEST_WITHOUT_SERVICE).expect_err("must be refused");
    let message = error.to_string();
    assert!(
        message.contains("ssu2.peer_test"),
        "the refusal must name the field, got: {message}"
    );
}

#[test]
fn the_enabled_posture_is_accepted() {
    let config = Config::parse(PEER_TEST_ENABLED).expect("valid config");
    assert!(config.ssu2.peer_test);
    assert!(config.ssu2.enabled);
}

// ------------------------------------------------------------ the responder

#[tokio::test]
async fn the_responder_answers_with_the_probed_fields_unchanged() {
    let (mut a, mut b) = start_pair().await;

    // A originates the probe.
    let mut owner_a = PeerTestOwner::new(true);
    let mut rng = lane_rng();
    let probe = owner_a
        .start_probe(b.peer(), &mut rng, wall_ms(), wall_ms() + 60_000)
        .expect("probe");
    assert_eq!(
        deliver(&a.handle, b.peer(), probe.bytes.clone()),
        RouterDeliveryOutcome::Accepted,
        "the probe must travel over the established session"
    );

    // B receives it, decodes it **once**, and owes an answer.
    let inbound = next_inbound(&mut b.handle).await;
    let decoded = decode_authenticated_router_i2np(&inbound, wall_ms()).expect("decode");
    assert!(
        matches!(decoded.outcome(), RouterI2npOutcome::PeerTest { .. }),
        "the inbound message must be classified as a peer test"
    );
    let mut owner_b = PeerTestOwner::new(true);
    let answer = match owner_b.handle_inbound(
        &decoded.outcome(),
        0x2222_3333,
        wall_ms() + 60_000,
        wall_ms(),
    ) {
        PeerTestInbound::Answer(answer) => answer,
        other => panic!("expected an answer, got {other:?}"),
    };
    assert_eq!(
        answer.peer,
        a.peer(),
        "the answer travels back to the peer that sent the test"
    );
    assert_eq!(owner_b.counters().answers_built, 1);
    assert_eq!(
        owner_b.counters().probes_completed,
        0,
        "the responder records nothing about itself"
    );

    // The answer repeats the probed fields unchanged.
    let message = I2npMessage::decode_standard(&answer.bytes, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode");
    assert_eq!(
        message.body(),
        &I2npBody::TunnelTest(TunnelTestMessage {
            msg_id: probe.key.msg_id,
            timestamp: probe.key.timestamp,
        }),
        "the answer must repeat the probed identifier and timestamp unchanged"
    );

    // B sends it back to A.
    assert_eq!(
        deliver(&b.handle, a.peer(), answer.bytes),
        RouterDeliveryOutcome::Accepted
    );

    // A receives the answer and completes its own probe.
    let echo = next_inbound(&mut a.handle).await;
    let decoded = decode_authenticated_router_i2np(&echo, wall_ms()).expect("decode");
    let confirmed = match owner_a.handle_inbound(
        &decoded.outcome(),
        0x3333_4444,
        wall_ms() + 60_000,
        wall_ms(),
    ) {
        PeerTestInbound::Completed(confirmed) => confirmed,
        other => panic!("expected a completion, got {other:?}"),
    };
    assert_eq!(confirmed.peer, b.peer());
    assert_eq!(owner_a.counters().probes_completed, 1);
    assert_eq!(
        owner_a.counters().answers_built,
        0,
        "the originator owes the peer no answer to its own echo"
    );
    assert_eq!(owner_a.outstanding(), 0);

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn a_confirmed_exchange_records_the_router_s_own_reachability() {
    let (mut a, mut b) = start_pair().await;

    // The runtime's own recorder is the only authority, and before the
    // exchange it has recorded nothing.
    assert_eq!(a.handle.service().confirmed_peer_test_count(), 0);
    let before = a.handle.service().snapshot().reachability;

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

    assert_eq!(
        a.handle.service().confirmed_peer_test_count(),
        1,
        "the router's own confirmed peer test must be recorded once"
    );
    let elapsed = a.handle.service().last_confirmed_peer_test_ms();
    assert!(
        elapsed < 60_000,
        "the measured round-trip time must be observable and bounded by the exchange"
    );
    let after = a.handle.service().snapshot().reachability;
    assert_ne!(
        after, before,
        "the router's own reachability must have moved on the confirmed exchange"
    );

    // The responder recorded nothing about itself.
    assert_eq!(b.handle.service().confirmed_peer_test_count(), 0);
    assert_eq!(
        b.handle.service().snapshot().reachability,
        b.handle.service().snapshot().reachability,
        "the responder's own reachability is untouched by answering"
    );

    a.shutdown().await;
    b.shutdown().await;
}

#[test]
fn the_transport_s_tracker_reaches_reachable_with_the_confirmed_peer_test() {
    // The transport's policy is the transport's own. The lane shows that a
    // confirmed peer test supplies the **missing** class: with the configured
    // bind and the peer's external-address observation already present, the
    // tracker needs the peer test to reach `Reachable`.
    let policy = ReachabilityPolicy::new(2, DEFAULT_OBSERVATION_TTL, true);
    let mut tracker = ReachabilityTracker::new(policy).expect("tracker");
    let start = Duration::from_secs(1_000);
    tracker.record(
        ReachabilitySignal::LocalConfiguredBind {
            family: AddressFamily::Ipv4,
        },
        start,
    );
    tracker.record(
        ReachabilitySignal::AuthenticatedPeerObservedExternalAddress {
            family: AddressFamily::Ipv4,
        },
        start,
    );
    assert_ne!(
        tracker.snapshot(start).state,
        ReachabilityState::Reachable,
        "two classes must not be enough"
    );

    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::Confirmed,
        },
        start,
    );
    let snapshot = tracker.snapshot(start);
    assert_eq!(
        snapshot.state,
        ReachabilityState::Reachable,
        "the confirmed peer test supplies the third class"
    );
    assert_eq!(snapshot.corroboration, 3);
    assert_eq!(snapshot.family, AddressFamily::Ipv4);
}

// ------------------------------------------------------------ the failures

#[test]
fn a_probe_that_is_not_answered_records_no_signal() {
    let mut owner = PeerTestOwner::new(true);
    let mut rng = lane_rng();
    let peer = PeerIdOf::from_hash(Hash::from_bytes([2; 32]));
    let probe = owner
        .start_probe(peer, &mut rng, wall_ms(), wall_ms() + 60_000)
        .expect("probe");
    let late = wall_ms() + i2pr_daemon::peer_test::DEFAULT_PEER_TEST_TIMEOUT_MS + 1;
    assert_eq!(
        owner.handle_inbound(
            &RouterI2npOutcome::PeerTest {
                header: i2pr_daemon::router_i2np::RouterI2npHeaderKind::Standard,
                message_id: 1,
                expiration_ms: late,
                peer_test_msg_id: probe.key.msg_id,
                peer_test_timestamp: probe.key.timestamp,
                peer,
                link_id: i2pr_transport::LinkId::new(7).expect("link"),
                encoded_len: 32,
            },
            1,
            late,
            late,
        ),
        PeerTestInbound::Ignored
    );
    assert_eq!(owner.counters().probes_completed, 0);
    assert_eq!(owner.counters().answers_built, 0);
    assert_eq!(owner.counters().probes_timed_out, 1);
}

#[test]
fn an_unmatched_echo_matches_nothing() {
    let mut owner = PeerTestOwner::new(true);
    let mut rng = lane_rng();
    let peer = PeerIdOf::from_hash(Hash::from_bytes([3; 32]));
    owner
        .start_probe(peer, &mut rng, wall_ms(), wall_ms() + 60_000)
        .expect("probe");
    let outcome = RouterI2npOutcome::PeerTest {
        header: i2pr_daemon::router_i2np::RouterI2npHeaderKind::Standard,
        message_id: 1,
        expiration_ms: wall_ms(),
        peer_test_msg_id: 0xDEAD_BEEF,
        peer_test_timestamp: 0x1234_5678_9ABC_DEF0,
        peer,
        link_id: i2pr_transport::LinkId::new(7).expect("link"),
        encoded_len: 32,
    };
    match owner.handle_inbound(&outcome, 1, wall_ms() + 60_000, wall_ms()) {
        PeerTestInbound::Answer(answer) => {
            assert_eq!(
                answer.peer, peer,
                "the peer that sent the test is still owed an answer"
            );
        }
        other => panic!("an unmatched probe is still owed an answer, got {other:?}"),
    }
    assert_eq!(
        owner.counters().probes_completed,
        0,
        "an echo that matches no probe of ours completes nothing"
    );
    assert_eq!(owner.counters().echoes_unmatched, 1);
    assert_eq!(
        owner.outstanding(),
        1,
        "the outstanding probe survives an echo that did not match it"
    );
}

#[test]
fn the_table_ceiling_refuses_a_probe_rather_than_evicting_one() {
    let mut owner = PeerTestOwner::new(true);
    let mut rng = lane_rng();
    for index in 0..MAX_OUTSTANDING_PEER_TESTS {
        owner
            .start_probe(
                PeerIdOf::from_hash(Hash::from_bytes([index as u8; 32])),
                &mut rng,
                wall_ms(),
                wall_ms() + 60_000,
            )
            .expect("probe below the ceiling");
    }
    assert_eq!(
        owner.start_probe(
            PeerIdOf::from_hash(Hash::from_bytes([0xEE; 32])),
            &mut rng,
            wall_ms(),
            wall_ms() + 60_000,
        ),
        Err(PeerTestOwnerError::Full)
    );
    assert_eq!(owner.counters().probes_refused_full, 1);
    assert_eq!(owner.outstanding(), MAX_OUTSTANDING_PEER_TESTS);
}

#[test]
fn a_repeated_key_is_answered_at_most_once() {
    let mut owner = PeerTestOwner::new(true);
    let peer = PeerIdOf::from_hash(Hash::from_bytes([4; 32]));
    let outcome = RouterI2npOutcome::PeerTest {
        header: i2pr_daemon::router_i2np::RouterI2npHeaderKind::Standard,
        message_id: 1,
        expiration_ms: wall_ms(),
        peer_test_msg_id: 0xABCD,
        peer_test_timestamp: 42,
        peer,
        link_id: i2pr_transport::LinkId::new(7).expect("link"),
        encoded_len: 32,
    };
    assert!(matches!(
        owner.handle_inbound(&outcome, 1, wall_ms() + 60_000, wall_ms()),
        PeerTestInbound::Answer(_)
    ));
    assert_eq!(
        owner.handle_inbound(&outcome, 2, wall_ms() + 60_000, wall_ms()),
        PeerTestInbound::Ignored
    );
    assert_eq!(owner.counters().answers_built, 1);
    assert_eq!(owner.counters().answers_suppressed_repeat, 1);
    assert_eq!(owner.responded(), 1);
    assert!(owner.responded() <= MAX_RESPONDED_PEER_TESTS);
}

/// The probe's key is what the peer echoes, and it is drawn at random so a
/// peer cannot make its own probe look like this router's echo.
#[test]
fn the_probed_timestamp_is_not_the_clock() {
    let mut owner = PeerTestOwner::new(true);
    let mut rng = lane_rng();
    let now = wall_ms();
    let probe = owner
        .start_probe(
            PeerIdOf::from_hash(Hash::from_bytes([5; 32])),
            &mut rng,
            now,
            now + 60_000,
        )
        .expect("probe");
    assert_ne!(probe.key.timestamp, now);
    assert_ne!(probe.key.timestamp, 0);
    assert_ne!(probe.key.msg_id, 0);
    assert_eq!(
        probe.key,
        PeerTestKey::new(probe.key.msg_id, probe.key.timestamp)
    );
}

/// The message identifier of an answer is drawn fresh and is never zero.
#[test]
fn the_answer_identifier_is_fresh_and_never_zero() {
    let mut rng = lane_rng();
    for _ in 0..64 {
        let identifier = i2pr_daemon::peer_test::fresh_message_identifier(&mut rng).expect("id");
        assert_ne!(identifier, 0);
    }
}
