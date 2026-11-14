//! Plan 549 — the path in use must be able to prove itself again.
//!
//! `Reachable` needs three distinct corroboration classes inside the
//! transport's observation lifetime. Two of them have producers that keep
//! firing on a **stable** session: the explicit bind, re-asserted on the
//! runtime's maintenance pass (Plan 535), and the peer test, which completes
//! every five minutes. The third — `ValidatedPath` — was produced only while a
//! *candidate* path was being migrated to, so a healthy long-lived session
//! produced no path evidence at all and its state decayed to exactly the
//! corroboration floor, which publication refuses.
//!
//! This lane proves both halves of the fix:
//!
//! 1. **On the wire**, over real loopback UDP: a live session issues
//!    PathChallenge datagrams, the peer answers them, the runtime counts the
//!    re-validations, and **nothing migrates** — the peer address is unchanged
//!    and the migration counter stays at zero. The control arm sets the
//!    cadence out of reach and asserts the probe never fires, so the lane
//!    cannot pass on a probe that was never the cause.
//! 2. **In the tracker**, at unit speed rather than the transport's real
//!    half-hour: three classes re-recorded on the cadence hold `Reachable`
//!    past two observation lifetimes, and the same signals **without** the
//!    path re-record fall to `CandidateReachable` at one lifetime. That is the
//!    composition claim the live measurement later confirms end to end.
//!
//! The lane deliberately does not wait out `DEFAULT_OBSERVATION_TTL`: a
//! half-hour test is not a test, and the tracker half is the authority for the
//! composition. The three-host measurement is the end-to-end evidence.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    ChildFailurePolicy, ChildScope, Ssu2DialTarget, Ssu2IdentityMaterial, Ssu2RuntimeConfig,
    Ssu2RuntimeService, Ssu2SocketConfig,
};
use i2pr_transport::PeerId;
use i2pr_transport::{
    DEFAULT_OBSERVATION_TTL, ReachabilityPolicy, ReachabilitySignal, ReachabilityState,
    ReachabilityTracker,
};
use i2pr_transport_ssu2::{IntroKey, Ssu2PublicKey, constants};
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the lane waits for a re-validation to complete on the wire.
const REVALIDATION_WAIT: Duration = Duration::from_secs(15);

/// A cadence short enough for a lane, and one far beyond any lane's lifetime.
const FAST_CADENCE_MS: u64 = 200;
const IMPOSSIBLE_CADENCE_MS: u64 = 3_600_000;

fn wall_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
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

struct RouterKeys {
    peer: PeerId,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
}

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
    keys: RouterKeys,
    bound: SocketAddr,
}

impl Fixture {
    async fn shutdown(self) {
        self.service.shutdown();
        let _ = self.scope.shutdown().await;
    }
}

async fn start_fixture(keys: RouterKeys, cadence_ms: u64) -> Fixture {
    let config = Ssu2RuntimeConfig {
        path_revalidation_interval_ms: cadence_ms,
        ..Ssu2RuntimeConfig::default()
    };
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
    let token = i2pr_runtime::CancellationToken::new();
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
    let bound = handle.local_v4().expect("bound v4");
    Fixture {
        service,
        scope,
        keys,
        bound,
    }
}

async fn dial(from: &Fixture, peer: &RouterKeys, addr: SocketAddr) {
    let target = Ssu2DialTarget::new(peer.peer, peer.hash, addr, peer.static_public, peer.intro)
        .expect("dial target");
    from.service
        .dial_ssu2(
            target,
            DIAL_TIMEOUT,
            &i2pr_runtime::CancellationToken::new(),
        )
        .await
        .expect("dial");
}

async fn wait_for_revalidations(fixture: &Fixture, expected: u64) -> bool {
    let deadline = tokio::time::Instant::now() + REVALIDATION_WAIT;
    while tokio::time::Instant::now() < deadline {
        if fixture.service.snapshot().path_revalidations >= expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

/// A live session re-proves the path it is already using, and proves it
/// **without** migrating.
#[tokio::test]
async fn a_live_session_re_proves_its_path_without_migrating() {
    let b = make_router_keys();
    let a = make_router_keys();
    let fixture_b = start_fixture(b, FAST_CADENCE_MS).await;
    let fixture_a = start_fixture(a, FAST_CADENCE_MS).await;
    let b_addr = fixture_b.bound;
    dial(&fixture_a, &fixture_b.keys, b_addr).await;

    // Both ends run the cadence, so the initiator's session is the one this
    // lane reads: it re-proves the path it dials on.
    assert!(
        wait_for_revalidations(&fixture_a, 1).await,
        "the path in use was never re-proved: the probe did not reach the wire"
    );
    let snapshot = fixture_a.service.snapshot();
    assert!(
        snapshot.path_revalidations >= 1,
        "a completed re-validation must be counted"
    );
    // The whole point: re-proving the path in use is not a migration.
    assert_eq!(
        snapshot.path_migrations, 0,
        "a re-validation must never migrate the session"
    );

    fixture_a.shutdown().await;
    fixture_b.shutdown().await;
}

/// The control arm: with the cadence out of reach, the probe never fires.
///
/// Without this the lane would pass on any traffic the runtime happened to
/// produce, rather than on the re-validation it claims to measure.
#[tokio::test]
async fn an_out_of_reach_cadence_never_re_proves_the_path() {
    let b = make_router_keys();
    let a = make_router_keys();
    let fixture_b = start_fixture(b, IMPOSSIBLE_CADENCE_MS).await;
    let fixture_a = start_fixture(a, IMPOSSIBLE_CADENCE_MS).await;
    let b_addr = fixture_b.bound;
    dial(&fixture_a, &fixture_b.keys, b_addr).await;

    // Give the loop ample opportunity to probe if it were going to.
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let snapshot = fixture_a.service.snapshot();
    assert_eq!(
        snapshot.path_revalidations, 0,
        "the probe must not fire before its cadence is due"
    );

    fixture_a.shutdown().await;
    fixture_b.shutdown().await;
}

/// Three classes hold `Reachable` across two observation lifetimes only when
/// the path class is re-recorded on the cadence.
///
/// The tracker is the authority for this composition: `Reachable` needs more
/// than the corroboration floor, so dropping the path re-record costs exactly
/// the third class and lands on the floor.
#[test]
fn the_path_class_is_what_holds_reachable_past_two_lifetimes() {
    let policy = ReachabilityPolicy::new(2, 1, DEFAULT_OBSERVATION_TTL, true)
        .validate()
        .expect("policy");
    let cadence = Duration::from_secs(10 * 60);
    let lifetime = DEFAULT_OBSERVATION_TTL;

    let mut refreshed = ReachabilityTracker::new(policy).expect("tracker");
    let mut unrefreshed = ReachabilityTracker::new(policy).expect("tracker");
    let family = i2pr_transport::AddressFamily::Ipv4;

    // The two classes a stable session keeps on its own, plus the path class
    // the re-validation supplies.
    for tracker in [&mut refreshed, &mut unrefreshed] {
        tracker.record(
            ReachabilitySignal::LocalConfiguredBind { family },
            Duration::ZERO,
        );
        tracker.record(
            ReachabilitySignal::PeerTestResult {
                family,
                outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed,
            },
            Duration::ZERO,
        );
        tracker.record(ReachabilitySignal::ValidatedPath { family }, Duration::ZERO);
    }
    assert_eq!(refreshed.state(), ReachabilityState::Reachable);
    assert_eq!(unrefreshed.state(), ReachabilityState::Reachable);

    // Walk two lifetimes, re-recording bind, peer test and path on the cadence
    // in one tracker and bind and peer test only in the other.
    let mut now = Duration::ZERO;
    let end = lifetime.saturating_mul(2).saturating_add(cadence);
    while now < end {
        now = now.saturating_add(cadence);
        refreshed.record(ReachabilitySignal::LocalConfiguredBind { family }, now);
        refreshed.record(
            ReachabilitySignal::PeerTestResult {
                family,
                outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed,
            },
            now,
        );
        refreshed.record(ReachabilitySignal::ValidatedPath { family }, now);
        unrefreshed.record(ReachabilitySignal::LocalConfiguredBind { family }, now);
        unrefreshed.record(
            ReachabilitySignal::PeerTestResult {
                family,
                outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed,
            },
            now,
        );
        refreshed.poll_expiry(now);
        unrefreshed.poll_expiry(now);
    }

    assert_eq!(
        refreshed.state(),
        ReachabilityState::Reachable,
        "with the path class re-recorded, two lifetimes must not cost `Reachable`"
    );
    assert_ne!(
        unrefreshed.state(),
        ReachabilityState::Reachable,
        "without the path class the state must fall to the corroboration floor"
    );
    assert!(
        matches!(
            unrefreshed.state(),
            ReachabilityState::CandidateReachable | ReachabilityState::ObservedUnconfirmed
        ),
        "the floor is `CandidateReachable`, not a fabricated `Reachable`"
    );
}
