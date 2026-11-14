//! Plan 547's lane — an SSU2-only router must be able to prove its own
//! reachability, and a re-dated dual-transport record must be installable.
//!
//! Plan 546 relaxed the publication policy so that a bound address *may* be
//! published, and left the corroboration that policy needs with nowhere to
//! come from: the router's own peer test lived inside the destination-group
//! product, which is built only when a destination group can exist at all.
//! A router that runs **only** SSU2 therefore had no owner, sent no probe,
//! and could never corroborate its own reachability.
//!
//! The lane drives the **production** owner — `PeerTestRuntime`, beside the
//! SSU2 service — over a **real** SSU2 session between two daemon-owned
//! services on loopback, and asserts four things:
//!
//! 1. the opt-in is **off** by default, and a router with it off owns no
//!    peer test at all: it sends no probe and answers no probe;
//! 2. a completed exchange makes the router's **own** bound address
//!    publishable — the confirmed peer test supplies the **third**
//!    corroboration class that nothing else supplies, and the router's
//!    publication material changes from refused to accepted;
//! 3. the responder records **nothing** about itself, and a **contradiction**
//!    vetoes the publication, because a corroboration that can only say yes
//!    is an assertion with extra steps; and
//! 4. a dual-transport record re-dates and installs through the scoped form,
//!    while a record whose NTCP2 address names a **different** endpoint is
//!    still refused: the install gate's own test keeps its meaning.
//!
//! The lane drives the **public** behaviour: the runtime's own recorder, the
//! runtime's own publication material and the daemon's own re-dating helper.
//! It names no private field of the owner.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use i2pr_crypto::{OsRng, RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::peer_test::PeerTestOwnerCounters;
use i2pr_daemon::peer_test_runtime::PeerTestRuntime;
use i2pr_daemon::router_i2np::{
    Ssu2DaemonHandle, Ssu2DaemonService, decode_authenticated_router_i2np,
    generate_controlled_identity, refresh_local_router_info_record,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2Endpoint, Ssu2InboundI2np,
    Ssu2PublicKey,
};
use i2pr_transport::{PeerId, ReachabilityState};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// The default posture: the peer test is off.
const DEFAULT_CONFIG: &str =
    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";

/// The posture under test: the peer test is on.
const PEER_TEST_CONFIG: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\npeer_test = true\n";

/// The placeholder port the identity's own record names before the bind.
///
/// The record's real address is built after the bind, from the bound
/// endpoint; this placeholder only has to be a legal port.
const PLACEHOLDER_PORT: u16 = 44_101;

/// The NTCP2 endpoint the dual-transport record names.
const NTCP2_ENDPOINT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 19_101);

/// A **different** NTCP2 endpoint, inside the same scope.
///
/// The scope admits it, so a refusal can only come from the endpoint test
/// itself and not from the scope.
const OTHER_NTCP2_ENDPOINT: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 19_102);

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

/// One daemon-owned SSU2 service: the router's own transport.
struct Fixture {
    bundle: RouterIdentityBundle,
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
    hash: Hash,
    static_public: Ssu2PublicKey,
    intro: IntroKey,
}

impl Fixture {
    async fn start() -> Self {
        let config = Config::parse(PEER_TEST_CONFIG)
            .expect("strict profile")
            .ssu2;
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = bundle.identity().hash().expect("hash");
        let identity =
            generate_controlled_identity(&bundle, "127.0.0.1", PLACEHOLDER_PORT).expect("identity");
        let static_public = Ssu2PublicKey::new(
            X25519PrivateKey::from_bytes(identity.static_secret_bytes).public_bytes(),
        )
        .expect("static public");
        let intro = identity.intro_key;
        let service = Ssu2DaemonService::new(&config, identity).expect("daemon-owned service");
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
            bundle,
            handle,
            scope,
            token,
            hash,
            static_public,
            intro,
        }
    }

    /// The router's own bound endpoint.
    fn bound(&self) -> SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    fn peer(&self) -> PeerId {
        PeerId::from_hash(self.hash)
    }

    /// The router's own record, naming **both** transports.
    ///
    /// The SSU2 address names the router's **bound** endpoint and the
    /// router's **own** keys, and the NTCP2 address names the separately
    /// bound NTCP2 endpoint. This is the shape the private-LAN and
    /// controlled qualification profiles build.
    fn dual_transport_record(&self, ntcp2_endpoint: SocketAddr, now: u64) -> Vec<u8> {
        let expiration = Date::from_millis(now.saturating_add(3_600_000));
        let ssu2 = RouterAddress::new(
            10,
            expiration,
            "SSU2".to_owned(),
            Mapping::from_entries(vec![
                ("host".to_owned(), self.bound().ip().to_string()),
                ("port".to_owned(), self.bound().port().to_string()),
                ("v".to_owned(), "2".to_owned()),
                (
                    "s".to_owned(),
                    i2p_b64_encode(self.static_public.as_bytes()),
                ),
                ("i".to_owned(), i2p_b64_encode(self.intro.as_bytes())),
            ])
            .expect("ssu2 options"),
        )
        .expect("ssu2 address");
        // The NTCP2 address names the separately bound endpoint. Its own
        // transport material is the router's persisted NTCP2 identity, so
        // the record is the shape the composition really builds.
        let material = self.ntcp2_material();
        let ntcp2 = RouterAddress::new(
            10,
            expiration,
            "NTCP2".to_owned(),
            Mapping::from_entries(vec![
                ("host".to_owned(), ntcp2_endpoint.ip().to_string()),
                ("port".to_owned(), ntcp2_endpoint.port().to_string()),
                ("v".to_owned(), "2".to_owned()),
                (
                    "s".to_owned(),
                    i2p_b64_encode(&material.transport().key().public_bytes()),
                ),
                ("i".to_owned(), i2p_b64_encode(material.transport().iv())),
            ])
            .expect("ntcp2 options"),
        )
        .expect("ntcp2 address");
        // The record carries the **same** router options the identity's own
        // record carries, so the install gate's own `netId` comparison is
        // satisfied by a real record and not by a special case.
        let options = i2pr_netdb::controlled_router_options().expect("router options");
        let info = self
            .bundle
            .sign_router_info(
                Date::from_millis(now),
                vec![ssu2, ntcp2],
                Vec::new(),
                options,
            )
            .expect("sign");
        info.encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode")
    }

    /// The router's persisted NTCP2 identity material.
    fn ntcp2_material(&self) -> i2pr_storage::Ntcp2TransportIdentityMaterial {
        let dir = std::env::temp_dir().join(format!(
            "i2pr-547-ntcp2-{}-{}",
            std::process::id(),
            self.hash.as_bytes()[..4]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        let store = i2pr_storage::Ntcp2TransportIdentityStore::in_data_dir(&dir);
        store
            .load_or_generate(*self.hash.as_bytes(), &mut OsRng)
            .expect("ntcp2 identity material")
    }

    async fn shutdown(self) {
        self.handle.shutdown();
        self.scope.shutdown().await;
        self.token
            .cancel(i2pr_runtime::CancellationReason::TestHarnessTeardown);
    }
}

/// The daemon's peer reference.
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

/// Starts two daemon-owned services and establishes a session between them.
async fn start_pair() -> (Fixture, Fixture) {
    let a = Fixture::start().await;
    let b = Fixture::start().await;
    let target =
        i2pr_runtime::Ssu2DialTarget::new(b.peer(), b.hash, b.bound(), b.static_public, b.intro)
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

/// The owner's live counters.
fn counters(runtime: &PeerTestRuntime) -> PeerTestOwnerCounters {
    runtime
        .counters_cell()
        .lock()
        .map(|cell| *cell)
        .unwrap_or_default()
}

// ------------------------------------------------------------ the opt-in

#[test]
fn the_opt_in_is_off_by_default_and_the_owner_then_does_not_exist() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config");
    assert!(
        !config.ssu2.peer_test,
        "the peer-test switch must default to off"
    );
    let runtime = PeerTestRuntime::new(config.ssu2.peer_test);
    assert!(
        !runtime.enabled(),
        "a router with the opt-in off owns no peer test at all"
    );
    let counters = counters(&runtime);
    assert_eq!(counters.probes_started, 0);
    assert_eq!(counters.answers_built, 0);
}

// --------------------------------------------- the corroborated exchange

#[tokio::test]
async fn a_completed_exchange_makes_the_bound_address_publishable() {
    let (mut a, mut b) = start_pair().await;
    let mut a_runtime = PeerTestRuntime::new(true);
    let mut b_runtime = PeerTestRuntime::new(true);

    // Before the exchange the router has the bind and the validated path,
    // and **two** corroboration classes are not enough: the publication is
    // refused. This is the gap Plan 547 exists to close.
    assert_eq!(
        a.handle.snapshot().reachability,
        ReachabilityState::CandidateReachable,
        "two corroboration classes must not be enough"
    );
    assert!(
        a.handle.publication_material(wall_ms()).is_err(),
        "the router must refuse to publish its address before it corroborates itself"
    );

    // A drives its own peer test over the session it already holds.
    a_runtime.drive(&a.handle, &a.token, wall_ms());
    assert_eq!(
        counters(&a_runtime).probes_started,
        1,
        "the router must start exactly one probe"
    );

    // B receives it and answers.
    let inbound = next_inbound(&mut b.handle).await;
    let decoded = decode_authenticated_router_i2np(&inbound, wall_ms()).expect("decode");
    b_runtime.accept_inbound(&decoded.outcome(), &b.handle, &b.token, wall_ms());
    assert_eq!(
        counters(&b_runtime).answers_built,
        1,
        "the responder must answer the peer's own test"
    );

    // A receives the echo and completes its probe.
    let echo = next_inbound(&mut a.handle).await;
    let decoded = decode_authenticated_router_i2np(&echo, wall_ms()).expect("decode");
    a_runtime.accept_inbound(&decoded.outcome(), &a.handle, &a.token, wall_ms());
    assert_eq!(
        a.handle.service().confirmed_peer_test_count(),
        1,
        "the router's own confirmed peer test must be recorded once"
    );

    // The third corroboration class is the peer test, and now the router's
    // own bound address is publishable.
    assert_eq!(
        a.handle.snapshot().reachability,
        ReachabilityState::Reachable,
        "the confirmed peer test supplies the third corroboration class"
    );
    assert!(
        a.handle.publication_material(wall_ms()).is_ok(),
        "a corroborated router must be able to publish its own bound address"
    );

    // The responder records **nothing** about itself.
    assert_eq!(
        b.handle.service().confirmed_peer_test_count(),
        0,
        "the responder must record no corroboration of its own"
    );
    assert_eq!(
        b.handle.snapshot().reachability,
        ReachabilityState::CandidateReachable,
        "answering a peer's test must not corroborate the responder"
    );
    assert!(
        b.handle.publication_material(wall_ms()).is_err(),
        "the responder must still refuse to publish"
    );

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn a_router_with_the_opt_in_off_answers_nobody() {
    let (a, mut b) = start_pair().await;
    let mut a_runtime = PeerTestRuntime::new(true);
    // B's operator has not asked for reachability evidence, so B owns no
    // peer test and answers nobody.
    let mut b_runtime = PeerTestRuntime::new(false);

    a_runtime.drive(&a.handle, &a.token, wall_ms());
    let inbound = next_inbound(&mut b.handle).await;
    let decoded = decode_authenticated_router_i2np(&inbound, wall_ms()).expect("decode");
    b_runtime.accept_inbound(&decoded.outcome(), &b.handle, &b.token, wall_ms());

    assert_eq!(
        counters(&b_runtime).answers_built,
        0,
        "a router with the opt-in off must answer no peer test"
    );
    assert_eq!(
        a.handle.service().confirmed_peer_test_count(),
        0,
        "an unanswered probe records no corroboration"
    );
    assert_eq!(
        a.handle.snapshot().reachability,
        ReachabilityState::CandidateReachable,
        "the router must still refuse to publish"
    );

    a.shutdown().await;
    b.shutdown().await;
}

// ------------------------------------------------ the contradicting outcome

#[tokio::test]
async fn a_contradiction_vetoes_the_publication() {
    let (mut a, mut b) = start_pair().await;
    let mut a_runtime = PeerTestRuntime::new(true);
    let mut b_runtime = PeerTestRuntime::new(true);

    a_runtime.drive(&a.handle, &a.token, wall_ms());
    let inbound = next_inbound(&mut b.handle).await;
    let decoded = decode_authenticated_router_i2np(&inbound, wall_ms()).expect("decode");
    b_runtime.accept_inbound(&decoded.outcome(), &b.handle, &b.token, wall_ms());
    let echo = next_inbound(&mut a.handle).await;
    let decoded = decode_authenticated_router_i2np(&echo, wall_ms()).expect("decode");
    a_runtime.accept_inbound(&decoded.outcome(), &a.handle, &a.token, wall_ms());
    assert!(
        a.handle.publication_material(wall_ms()).is_ok(),
        "the corroborated router must publish"
    );

    // Now the honest bad news: an authenticated peer sees this router at an
    // address the router does **not** own. The runtime records the
    // contradiction as this router's own firewalled fact, and the
    // contradiction takes precedence over the corroboration.
    let elsewhere =
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)), 9_999).expect("endpoint");
    let state = a.handle.service().note_observed_external_address(elsewhere);
    assert_eq!(
        state,
        ReachabilityState::Firewalled,
        "a contradiction must put the router behind a firewall"
    );
    assert!(
        a.handle.publication_material(wall_ms()).is_err(),
        "a contradicted router must refuse to publish its address"
    );

    a.shutdown().await;
    b.shutdown().await;
}

// --------------------------------------------------- the dual-transport record

#[tokio::test]
async fn a_dual_transport_record_re_dates_and_installs() {
    let fixture = Fixture::start().await;
    let now = wall_ms();
    let scope = i2pr_runtime::Ntcp2DialScope::Loopback;
    let record = fixture.dual_transport_record(NTCP2_ENDPOINT, now);
    fixture
        .handle
        .service()
        .install_scoped_dual_router_info(record.clone(), now, NTCP2_ENDPOINT, scope)
        .expect("the dual-transport record installs");
    assert_eq!(
        fixture.handle.service().installed_local_router_info(),
        Some(record),
        "the dual-transport record must be the installed one"
    );

    // The re-dating path carries the endpoint, so the re-dated record
    // installs too. Before Plan 547 the re-dating path passed no endpoint
    // and the install was refused with `InvalidPublication`.
    let fresh = refresh_local_router_info_record(
        &fixture.bundle,
        &fixture.handle.publication_handle(),
        now + 1_000,
        Some(NTCP2_ENDPOINT),
        scope,
    )
    .expect("re-dated record");
    assert_eq!(
        fixture.handle.service().installed_local_router_info(),
        Some(fresh.clone()),
        "the re-dated dual-transport record must be installed"
    );

    fixture.shutdown().await;
}

#[tokio::test]
async fn a_record_whose_ntcp2_address_names_another_endpoint_is_still_refused() {
    let fixture = Fixture::start().await;
    let now = wall_ms();
    let scope = i2pr_runtime::Ntcp2DialScope::Loopback;
    let record = fixture.dual_transport_record(NTCP2_ENDPOINT, now);
    fixture
        .handle
        .service()
        .install_scoped_dual_router_info(record.clone(), now, NTCP2_ENDPOINT, scope)
        .expect("the dual-transport record installs");

    // The other endpoint is **inside** the same scope, so the refusal can
    // only come from the endpoint test itself: the install gate still
    // compares the record's NTCP2 address with the separately bound
    // endpoint, and the plumbing supplies the endpoint rather than
    // skipping the test.
    let fresh = refresh_local_router_info_record(
        &fixture.bundle,
        &fixture.handle.publication_handle(),
        now + 1_000,
        Some(OTHER_NTCP2_ENDPOINT),
        scope,
    )
    .expect("re-dated record");
    assert_ne!(
        fresh, record,
        "the re-dated record must differ from the installed one"
    );
    assert_eq!(
        fixture.handle.service().installed_local_router_info(),
        Some(record),
        "a record naming another endpoint must not be installed"
    );

    fixture.shutdown().await;
}
