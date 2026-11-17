//! Plan 535 — the router must keep its own bound address counted as
//! reachability evidence.
//!
//! Plan 530 made the router record its **own** bound sockets as the
//! explicit-bind corroboration class. The class is a **standing** fact —
//! the socket is bound and stays bound — but the transport's model holds
//! every signal as a timed observation, so the class **lapses** after the
//! observation lifetime and nothing re-asserts it. A long-running router
//! then loses the one class it could have kept, and `Reachable` is
//! unreachable for ever.
//!
//! The lane drives the **public** runtime API only: the runtime's own
//! service, its own reachability evidence, its own snapshot and the rule
//! the runtime publishes. It reaches into no private module and adds no
//! accessor of its own.
//!
//! Properties of the lane:
//!
//! - **The rule is pure.** The lane proves the rule's truth table directly,
//!   so the "strictly before the record can lapse" property is proved and
//!   not described.
//! - **The lifetime is the runtime's own policy parameter.** The lane
//!   configures a short one, so the end-to-end row can wait past it and
//!   prove the class is **still** counted. The production default is the
//!   transport's own, and the guard pins that.
//! - **A paused clock.** The runtime reads its clock from Tokio's
//!   `Instant`, so a paused test's virtual clock is the runtime's clock:
//!   the wait is virtual, deterministic and instant.
//! - **No environment variable, no `#[ignore]`, no stock peer.** The only
//!   peer is the controlled loopback socket the lane binds itself.
//! - **No unbounded waiting.** Every wait is bounded by a hard deadline; the
//!   lane never sleeps to make an assertion pass, it waits for a state and
//!   fails when the state does not arrive.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationReason, CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial,
    Ssu2RuntimeConfig, Ssu2RuntimeReachability, Ssu2RuntimeService, Ssu2SocketConfig,
    explicit_bind_refresh_due,
};
use i2pr_transport::{
    AddressFamily, MIN_CONTRADICTION_FLOOR, ReachabilityPolicy, ReachabilitySignal,
    ReachabilityState, ReachabilityTracker,
};
use rand_core::{OsRng, TryRngCore};

/// The lane's observation lifetime.
///
/// Short enough that the lane can wait past it and prove the class is
/// still counted, and long enough that the runtime's maintenance pass
/// re-asserts the fact before it can lapse.
const LIFETIME: Duration = Duration::from_secs(2);

/// The bounded wait for a runtime state.
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// The poll interval of a bounded wait.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// The daemon's own SSU2 configuration is **not** consulted by this lane:
// the runtime's own policy parameter is what the lane sets, and the
// guard is the authority for the daemon's surface staying unchanged.

fn wall_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
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
    intro: i2pr_runtime::IntroKey,
    router_info: Vec<u8>,
}

fn make_local_keys() -> LocalKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_secret = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_secret.secret_bytes();
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
        ("port".to_string(), "44401".to_string()),
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
        intro,
        router_info,
    }
}

/// A runtime-owned service under the lane's own observation lifetime.
struct Runtime {
    service: Ssu2RuntimeService,
    scope: ChildScope,
    token: CancellationToken,
}

impl Runtime {
    /// Starts a runtime with the given configuration and binds it on
    /// loopback.
    async fn start(config: Ssu2RuntimeConfig) -> Self {
        let keys = make_local_keys();
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
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        service
            .start(
                &scope,
                Ssu2SocketConfig {
                    ipv4: Some("127.0.0.1:0".parse().expect("bind")),
                    ipv6: None,
                },
            )
            .await
            .expect("bind");
        Self {
            service,
            scope,
            token,
        }
    }

    /// Waits until the runtime's re-assertion counter has reached `expected`.
    async fn wait_for_refreshes(&self, expected: u64) {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            if self.service.snapshot().explicit_bind_refreshes >= expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the re-assertion counter did not reach {expected}"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Releases the runtime and its scope.
    async fn shutdown(self) {
        self.service.shutdown();
        self.scope.shutdown().await;
        self.token.cancel(CancellationReason::TestHarnessTeardown);
    }
}

/// The lane's configuration: the explicit-bind opt-in **on** and the
/// lane's own observation lifetime.
fn lane_config() -> Ssu2RuntimeConfig {
    Ssu2RuntimeConfig {
        explicit_bind_corroboration: true,
        reachability: Ssu2RuntimeReachability {
            observation_ttl: LIFETIME,
        },
        ..Ssu2RuntimeConfig::default()
    }
}

// ------------------------------------------------------------ the rule

/// The rule is **pure**, and its truth table is exactly the stated one: the
/// re-assertion is due once the record has aged past **half** the
/// observation lifetime, which is strictly before the record can lapse.
#[test]
fn the_rule_re_asserts_strictly_before_the_record_can_lapse() {
    let recorded = Duration::from_secs(1_000);
    for lifetime in [
        Duration::from_secs(2),
        Duration::from_secs(60),
        Duration::from_secs(1_800),
    ] {
        let half = lifetime / 2;
        assert!(
            !explicit_bind_refresh_due(
                Some(recorded),
                recorded + half - Duration::from_millis(1),
                lifetime
            ),
            "the re-assertion must not be due before half the lifetime has passed"
        );
        assert!(
            explicit_bind_refresh_due(Some(recorded), recorded + half, lifetime),
            "the re-assertion must be due once half the lifetime has passed"
        );
        assert!(
            recorded + half < recorded + lifetime,
            "the re-assertion must be due strictly before the record can lapse"
        );
        // The property, over a fine grid: the rule's **first** due moment
        // is at or before half the lifetime, so the re-assertion always
        // happens strictly before the record can lapse.
        let mut first_due = None;
        let mut now = Duration::ZERO;
        while now <= recorded + lifetime * 2 {
            if explicit_bind_refresh_due(Some(recorded), now, lifetime) {
                first_due = Some(now);
                break;
            }
            now += Duration::from_millis(1);
        }
        let first_due = first_due.expect("the rule must come due");
        assert!(
            first_due <= recorded + half,
            "the first due moment must be at or before half the lifetime"
        );
        assert!(
            first_due < recorded + lifetime,
            "the re-assertion must be due strictly before the record can lapse"
        );
    }

    // The rule **fails closed**: a fact that was never asserted is due, and
    // a zero lifetime is due on every pass.
    assert!(
        explicit_bind_refresh_due(None, Duration::ZERO, LIFETIME),
        "a fact that was never asserted must be due"
    );
    assert!(
        explicit_bind_refresh_due(Some(recorded), recorded, Duration::ZERO),
        "a zero lifetime must be due on every pass"
    );
}

// ------------------------------------------------------------ the router

/// A router keeps its own class counted **past** the observation lifetime.
///
/// The transport's own expiry still lapses an observation (proved by the
/// next row), so a class that is still counted after the lifetime has
/// elapsed can only be counted because the router re-asserted it.
#[tokio::test(start_paused = true)]
async fn a_router_keeps_its_own_class_counted_past_the_observation_lifetime() {
    let runtime = Runtime::start(lane_config()).await;

    // The class is recorded at bind time, exactly as Plan 530 recorded it.
    let evidence = runtime.service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 1,
        "the router's own bound socket is exactly one corroborating class"
    );
    assert_eq!(evidence.family, AddressFamily::Ipv4);

    // The re-assertion is due at half the lifetime, and the runtime's
    // existing maintenance performs it.
    runtime.wait_for_refreshes(1).await;

    // Now wait well past the observation lifetime. Without the
    // re-assertion the class would have lapsed here.
    tokio::time::sleep(LIFETIME * 2).await;

    let evidence = runtime.service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 1,
        "the router's own class must not lapse while the router runs"
    );
    assert_eq!(
        evidence.family,
        AddressFamily::Ipv4,
        "the class must still name the router's own family"
    );

    runtime.shutdown().await;
}

/// Every re-assertion is counted, and the counter carries counts only.
#[tokio::test(start_paused = true)]
async fn the_re_assertions_are_counted_and_the_counter_carries_counts_only() {
    let runtime = Runtime::start(lane_config()).await;

    runtime.wait_for_refreshes(1).await;
    let first = runtime.service.snapshot().explicit_bind_refreshes;
    assert!(first >= 1, "a re-assertion must be counted");

    // The counter moves as the fact is re-asserted again.
    runtime.wait_for_refreshes(first + 1).await;
    let second = runtime.service.snapshot().explicit_bind_refreshes;
    assert!(
        second > first,
        "every re-assertion must be counted: {first} then {second}"
    );

    runtime.shutdown().await;
}

/// A runtime that did not ask for explicit-bind corroboration re-asserts
/// nothing and records nothing.
#[tokio::test(start_paused = true)]
async fn a_runtime_without_the_opt_in_re_asserts_nothing() {
    // The runtime's own default leaves `explicit_bind_corroboration` off.
    let runtime = Runtime::start(Ssu2RuntimeConfig::default()).await;

    tokio::time::sleep(LIFETIME * 3).await;

    let evidence = runtime.service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 0,
        "a runtime that did not ask for explicit-bind corroboration records nothing"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::Unknown,
        "without the opt-in the router holds no evidence at all"
    );
    assert_eq!(
        runtime.service.snapshot().explicit_bind_refreshes,
        0,
        "a runtime that did not ask for explicit-bind corroboration re-asserts nothing"
    );

    runtime.shutdown().await;
}

/// The transport's reachability model is untouched, and its own expiry still
/// lapses an observation.
///
/// This is the **control** for the end-to-end row: the transport expires a
/// signal it is not told to refresh, so the router's class is still counted
/// past the lifetime only because the router re-asserted it.
#[test]
fn the_transports_reachability_model_is_untouched() {
    let policy = ReachabilityPolicy::new(2, MIN_CONTRADICTION_FLOOR, LIFETIME, true);
    let mut tracker = ReachabilityTracker::new(policy).expect("tracker");
    let start = Duration::from_secs(500);
    tracker.record(
        ReachabilitySignal::LocalConfiguredBind {
            family: AddressFamily::Ipv4,
        },
        start,
    );
    assert_eq!(
        tracker.snapshot(start).corroboration,
        1,
        "the explicit bind is exactly one corroborating class"
    );

    // The transport's own expiry still lapses it: the class is gone once the
    // lifetime has passed and nothing has re-asserted it.
    let after = start + LIFETIME + Duration::from_secs(1);
    tracker.poll_expiry(after);
    assert_eq!(
        tracker.snapshot(after).corroboration,
        0,
        "the transport's own expiry must still lapse an observation"
    );

    // And the model's vocabulary and classes are the transport's own: the
    // three distinct classes are still what reaches `Reachable`.
    let policy = ReachabilityPolicy::new(2, MIN_CONTRADICTION_FLOOR, LIFETIME, true);
    let mut tracker = ReachabilityTracker::new(policy).expect("tracker");
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
    assert_eq!(
        tracker.state(),
        ReachabilityState::CandidateReachable,
        "two classes are the floor, and the model still demands a third"
    );
    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed,
        },
        start,
    );
    assert_eq!(
        tracker.state(),
        ReachabilityState::Reachable,
        "the third distinct class is the only thing that reaches the state"
    );
}
