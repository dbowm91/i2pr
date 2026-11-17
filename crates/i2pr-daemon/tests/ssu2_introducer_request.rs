//! Plan 524's lane — the router asks a peer for a relay tag, and records the
//! introducer that answers.
//!
//! A router behind a firewall cannot be reached directly, so it needs an
//! **introducer**. The pinned i2pd 2.61.0 asks for a relay tag under
//! exactly one condition
//! (`libi2pd/SSU2Session.cpp:753`):
//!
//! ```cpp
//! if (GetRouterStatus () == eRouterStatusFirewalled && m_Address->IsIntroducer ())
//! ```
//!
//! — the router is **firewalled** and the peer **advertises the introducer
//! capability** (`libi2pd/RouterInfo.h:184`).
//!
//! The lane drives the **public** behaviour through the daemon's own owner
//! and the runtime's own live service, over a **real** SSU2 session between
//! two daemon-owned services on loopback:
//!
//! 1. the opt-in is **off** by default, and a router with it off asks
//!    nobody;
//! 2. the opt-in requires the SSU2 service, and is refused without it;
//! 3. a stored peer whose SSU2 address advertises `C` answers the
//!    capability question affirmatively;
//! 4. one that does not, answers negatively, and so do a peer with no
//!    `caps` and a peer whose `caps` does not parse;
//! 5. a peer with no stored record answers negatively, and is **counted**;
//! 6. a firewalled router with an eligible peer asks **exactly one** peer
//!    per window;
//! 7. a reachable router asks nobody;
//! 8. a router with no eligible peer asks nobody, and the refusal is
//!    counted;
//! 9. the router never asks a peer it has no session with;
//! 10. the request rides the **established** session, and the answer that
//!     comes back is recorded as an **authenticated introducer**, bound to
//!     the peer that issued it;
//! 11. the recorded introducer is observable in the table;
//! 12. a tag that arrives for a peer this router did **not** ask yields
//!     **no** record.
//!
//! Assertions are counts and digests only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::introducer_request::{
    IntroducerCapability, IntroducerRequestDecision, IntroducerRequestOwner,
    IntroducerRequestRefusal, MAX_INTRODUCER_CANDIDATES, read_capability,
};
use i2pr_daemon::router_i2np::{Ssu2DaemonHandle, Ssu2DaemonService};
use i2pr_netdb::{RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2IdentityMaterial,
};
use i2pr_transport::PeerId;
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The opt-in is off by default, so the default tree changes in no way.
const DEFAULT_CONFIG: &str =
    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";

/// The relay-tag request needs the SSU2 service, and the refusal is
/// explicit.
const REQUEST_WITHOUT_SERVICE: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = false\nintroducer_request = true\n";

/// The enabled posture the lane drives.
const REQUEST_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nintroducer_request = true\n";

/// The peer's own posture: it is willing to **be** an introducer, so it
/// answers a relay-tag request.
const INTRODUCER_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nintroducer_service = true\n";

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

/// I2P-base64 (alphabet `A-Za-z0-9-~`, `=` padding) encoder for the
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
            let offset = 18 - 6 * index;
            output.push(ALPHABET[((n >> offset) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

/// The shape of one peer's published SSU2 address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    /// An SSU2 address that advertises the introducer capability (`C`).
    Introducer,
    /// An SSU2 address that advertises no introducer capability.
    NotIntroducer,
    /// An SSU2 address with no `caps` option at all.
    NoCaps,
    /// An SSU2 address whose `caps` value does not parse.
    UnparsableCaps,
    /// An address that is not an SSU2 address at all.
    NotSsu2,
}

/// One test router's identity material and its signed record.
struct RouterKeys {
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: i2pr_runtime::Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
    /// The validated form of the very same record, for the store.
    validated: ValidatedRouterInfo,
}

impl RouterKeys {
    /// The router's peer reference.
    fn peer(&self) -> PeerId {
        PeerId::from_hash(self.hash)
    }
}

/// Builds one router's identity material and its published record.
///
/// The record's shape is the lane's only lever on the peer's advertised
/// capability, and the **same** signed bytes are the ones the runtime
/// publishes and the ones the store validates, so the lane never reads a
/// record the router did not publish.
fn make_router_keys(shape: Shape) -> RouterKeys {
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

    let mut entries = vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "12345".to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(&static_secret.public_bytes()),
        ),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ];
    let transport_style = match shape {
        Shape::Introducer => {
            entries.push(("caps".to_string(), "4C".to_string()));
            "SSU2"
        }
        Shape::NotIntroducer => {
            entries.push(("caps".to_string(), "4".to_string()));
            "SSU2"
        }
        Shape::NoCaps => "SSU2",
        Shape::UnparsableCaps => {
            // A duplicated known flag is refused by the transport's strict
            // capability parser.
            entries.push(("caps".to_string(), "CC".to_string()));
            "SSU2"
        }
        Shape::NotSsu2 => "SSU",
    };
    let options = Mapping::from_entries(entries).expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        transport_style.to_string(),
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
    let validated = ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(wall_ms())),
    )
    .expect("validated");
    assert_eq!(
        validated.key().as_bytes(),
        hash.as_bytes(),
        "the validated record and the runtime's identity must name the same router"
    );
    RouterKeys {
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
        validated,
    }
}

/// A store holding the supplied peers' validated records.
fn store_with(peers: &[&RouterKeys]) -> RouterInfoStore {
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    for peer in peers {
        store.insert(peer.validated.clone());
    }
    store
}

/// One daemon-owned SSU2 service, the router's own transport.
struct Fixture {
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
    keys: RouterKeys,
}

impl Fixture {
    async fn start(config: &i2pr_daemon::config::Ssu2Config, keys: RouterKeys) -> Self {
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

    fn peer(&self) -> PeerId {
        self.keys.peer()
    }

    async fn shutdown(self) {
        self.handle.shutdown();
        self.scope.shutdown().await;
        self.token
            .cancel(i2pr_runtime::CancellationReason::TestHarnessTeardown);
    }
}

fn daemon_ssu2_config(text: &str) -> i2pr_daemon::config::Ssu2Config {
    Config::parse(text).expect("strict profile").ssu2
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

/// Waits until the router has learned exactly `expected` introducers.
///
/// The learned introducer must be bound to `issuer`, and its relay tag is
/// never zero.
async fn wait_for_introducers(handle: &Ssu2DaemonHandle, expected: usize, issuer: &Hash) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let learned = handle.service().learned_introducers(wall_secs());
        if learned.len() == expected {
            assert_eq!(
                learned[0].peer_hash(),
                issuer.as_bytes(),
                "the introducer must be bound to the peer that issued the tag"
            );
            assert_ne!(learned[0].relay_tag(), 0, "a relay tag is never zero");
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the router learned {} introducers, expected {expected}",
            learned.len()
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Starts two daemon-owned services and establishes a session between them.
///
/// The first is this router — the one that asks. The second is the peer
/// that is willing to be an introducer.
async fn start_pair() -> (Fixture, Fixture) {
    let requester = daemon_ssu2_config(REQUEST_ENABLED);
    let introducer = daemon_ssu2_config(INTRODUCER_ENABLED);
    let a = Fixture::start(&requester, make_router_keys(Shape::Introducer)).await;
    let b = Fixture::start(&introducer, make_router_keys(Shape::Introducer)).await;
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

/// The peers this router holds a session with.
fn live_peers(handle: &Ssu2DaemonHandle) -> Vec<PeerId> {
    handle
        .service()
        .active_peer_hashes()
        .into_iter()
        .map(PeerId::from_hash)
        .collect()
}

// ---------------------------------------------------------------- the opt-in

#[test]
fn the_opt_in_is_off_by_default() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config");
    assert!(
        !config.ssu2.introducer_request,
        "the relay-tag request switch must default to off"
    );
    let mut owner = IntroducerRequestOwner::new(config.ssu2.introducer_request);
    assert!(!owner.enabled());
    let decision = owner.decide(&[PeerId::from_bytes([1; 32])], true, wall_ms());
    assert_eq!(
        decision,
        IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::Disabled),
        "a router with the opt-in off asks nobody"
    );
    assert_eq!(owner.counters().requests_refused_disabled, 1);
    assert_eq!(owner.counters().requests_queued, 0);
}

#[test]
fn the_opt_in_requires_the_ssu2_service() {
    let error = Config::parse(REQUEST_WITHOUT_SERVICE).expect_err("must be refused");
    let message = error.to_string();
    assert!(
        message.contains("ssu2.introducer_request"),
        "the refusal must name the field, got: {message}"
    );
}

#[test]
fn the_enabled_posture_is_accepted() {
    let config = Config::parse(REQUEST_ENABLED).expect("valid config");
    assert!(config.ssu2.introducer_request);
    assert!(config.ssu2.enabled);
}

// ------------------------------------------------------- the capability read

#[test]
fn the_capability_read_answers_from_the_validated_record() {
    let introducer = make_router_keys(Shape::Introducer);
    let plain = make_router_keys(Shape::NotIntroducer);
    let no_caps = make_router_keys(Shape::NoCaps);
    let unparsable = make_router_keys(Shape::UnparsableCaps);
    let not_ssu2 = make_router_keys(Shape::NotSsu2);
    let store = store_with(&[&introducer, &plain, &no_caps, &unparsable, &not_ssu2]);

    assert_eq!(
        read_capability(&store, introducer.peer()),
        IntroducerCapability::Advertised,
        "a peer that advertises the introducer capability is an introducer"
    );
    assert_eq!(
        read_capability(&store, plain.peer()),
        IntroducerCapability::NotAdvertised,
        "a peer that advertises no introducer capability is not an introducer"
    );
    assert_eq!(
        read_capability(&store, no_caps.peer()),
        IntroducerCapability::NotAdvertised,
        "a peer with no caps option is not an introducer"
    );
    assert_eq!(
        read_capability(&store, unparsable.peer()),
        IntroducerCapability::UnparsableAddress,
        "a peer whose address does not parse is not an introducer"
    );
    assert_eq!(
        read_capability(&store, not_ssu2.peer()),
        IntroducerCapability::NoSsu2Address,
        "a peer with no SSU2 address is not an introducer"
    );
    assert_eq!(
        read_capability(&store, PeerId::from_bytes([0xAB; 32])),
        IntroducerCapability::NoStoredRecord,
        "a peer with no stored record is not an introducer"
    );
}

#[test]
fn every_capability_refusal_is_counted_under_its_own_reason() {
    let introducer = make_router_keys(Shape::Introducer);
    let plain = make_router_keys(Shape::NotIntroducer);
    let store = store_with(&[&introducer, &plain]);
    let mut owner = IntroducerRequestOwner::new(true);

    owner.note_capability(read_capability(&store, introducer.peer()));
    owner.note_capability(read_capability(&store, plain.peer()));
    owner.note_capability(read_capability(&store, PeerId::from_bytes([0xCD; 32])));

    let counters = owner.counters();
    assert_eq!(counters.capability_advertised, 1);
    assert_eq!(counters.capability_refused_not_advertised, 1);
    assert_eq!(counters.capability_refused_no_record, 1);
    assert_eq!(counters.capability_refused_no_ssu2_address, 0);
    assert_eq!(counters.capability_refused_unparsable, 0);
}

// ------------------------------------------------------- the request policy

#[test]
fn a_firewalled_router_asks_exactly_one_peer_per_window() {
    let mut owner = IntroducerRequestOwner::new(true);
    let eligible = [PeerId::from_bytes([7; 32]), PeerId::from_bytes([8; 32])];
    let decision = owner.decide(&eligible, true, wall_ms());
    assert_eq!(
        decision,
        IntroducerRequestDecision::Ask(eligible[0]),
        "the window asks the first eligible peer and no other"
    );
}

#[test]
fn a_reachable_router_asks_nobody() {
    let mut owner = IntroducerRequestOwner::new(true);
    let decision = owner.decide(&[PeerId::from_bytes([7; 32])], false, wall_ms());
    assert_eq!(
        decision,
        IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NotFirewalled),
        "a reachable router needs no introducer"
    );
    assert_eq!(owner.counters().requests_refused_not_firewalled, 1);
    assert_eq!(owner.counters().requests_queued, 0);
}

#[test]
fn a_router_with_no_eligible_peer_asks_nobody_and_counts_the_refusal() {
    let mut owner = IntroducerRequestOwner::new(true);
    let decision = owner.decide(&[], true, wall_ms());
    assert_eq!(
        decision,
        IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer)
    );
    assert_eq!(owner.counters().requests_refused_no_eligible_peer, 1);
    assert_eq!(owner.counters().requests_queued, 0);
}

#[test]
fn the_router_never_asks_a_peer_it_has_no_session_with() {
    let mut owner = IntroducerRequestOwner::new(true);
    let live = [PeerId::from_bytes([7; 32])];
    let window = owner.candidate_window(&live, MAX_INTRODUCER_CANDIDATES);
    assert_eq!(
        window,
        live.to_vec(),
        "the window is drawn from the peers with a live session"
    );
    assert!(
        !window.contains(&PeerId::from_bytes([9; 32])),
        "a peer with no session is never named"
    );
}

#[test]
fn a_second_window_does_not_ask_the_same_peer_again() {
    let mut owner = IntroducerRequestOwner::new(true);
    let live = [PeerId::from_bytes([7; 32])];
    let first = owner.candidate_window(&live, MAX_INTRODUCER_CANDIDATES);
    assert_eq!(first, live.to_vec());
    owner.note_queued(live[0]);
    let second = owner.candidate_window(&live, MAX_INTRODUCER_CANDIDATES);
    assert!(
        second.is_empty(),
        "a peer this router already asked is not asked again"
    );
}

// -------------------------------------------------------- the live acquisition

#[tokio::test]
async fn the_router_asks_an_eligible_peer_and_records_the_introducer() {
    let (a, b) = start_pair().await;

    // The router holds a session with the peer, and only with the peer.
    let live = live_peers(&a.handle);
    assert_eq!(live.len(), 1, "the router holds one session");
    assert_eq!(live[0], b.peer());

    // The peer's validated record advertises the introducer capability.
    let store = store_with(&[&b.keys]);
    assert_eq!(
        read_capability(&store, b.peer()),
        IntroducerCapability::Advertised
    );

    // One window: the owner selects the peer and the daemon's read says it
    // is an introducer.
    let mut owner = IntroducerRequestOwner::new(true);
    let window = owner.candidate_window(&live, MAX_INTRODUCER_CANDIDATES);
    assert_eq!(window, vec![b.peer()]);
    let mut eligible = Vec::new();
    for peer in window {
        let capability = read_capability(&store, peer);
        owner.note_capability(capability);
        if capability.is_introducer() {
            eligible.push(peer);
            break;
        }
    }
    let decision = owner.decide(&eligible, true, wall_ms());
    assert_eq!(decision, IntroducerRequestDecision::Ask(b.peer()));

    // The request rides the established session.
    a.handle
        .service()
        .queue_relay_tag_request(b.peer())
        .expect("the request must be queued over the live session");
    owner.note_queued(b.peer());
    assert_eq!(owner.counters().requests_queued, 1);

    // The peer answers, and the router records the answer as an
    // authenticated introducer bound to the peer that issued it.
    wait_for_introducers(&a.handle, 1, &b.keys.hash).await;
    let snapshot = a.handle.service().snapshot();
    assert_eq!(snapshot.relay_tags_recorded, 1);
    assert_eq!(snapshot.relay_tags_refused_unrequested, 0);

    // The peer issued exactly one tag, bound to the requester.
    assert_eq!(b.handle.service().snapshot().relay_tags_issued, 1);

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn a_tag_for_a_peer_the_router_did_not_ask_yields_no_record() {
    let (a, b) = start_pair().await;

    // Two requests toward the same peer: the peer answers twice, and the
    // second answer arrives after the outstanding set has already given the
    // peer up.
    a.handle
        .service()
        .queue_relay_tag_request(b.peer())
        .expect("first request");
    a.handle
        .service()
        .queue_relay_tag_request(b.peer())
        .expect("second request");

    // The first answer is recorded.
    wait_for_introducers(&a.handle, 1, &b.keys.hash).await;

    // The second answer names a peer this router no longer has outstanding,
    // so it is refused and records nothing.
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let snapshot = a.handle.service().snapshot();
        if snapshot.relay_tags_refused_unrequested >= 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the unsolicited tag was never refused"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    let snapshot = a.handle.service().snapshot();
    assert_eq!(snapshot.relay_tags_recorded, 1, "only one record was made");
    assert_eq!(snapshot.relay_tags_refused_unrequested, 1);
    let learned = a.handle.service().learned_introducers(wall_secs());
    assert_eq!(learned.len(), 1, "the refused tag recorded nothing");

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn a_peer_that_advertises_no_capability_is_never_asked() {
    let requester = daemon_ssu2_config(REQUEST_ENABLED);
    let introducer = daemon_ssu2_config(INTRODUCER_ENABLED);
    let a = Fixture::start(&requester, make_router_keys(Shape::Introducer)).await;
    // The peer is willing to answer, but it does not advertise the
    // introducer capability, so this router must not ask it.
    let b = Fixture::start(&introducer, make_router_keys(Shape::NotIntroducer)).await;
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

    let live = live_peers(&a.handle);
    let store = store_with(&[&b.keys]);
    let mut owner = IntroducerRequestOwner::new(true);
    let window = owner.candidate_window(&live, MAX_INTRODUCER_CANDIDATES);
    let mut eligible = Vec::new();
    for peer in window {
        let capability = read_capability(&store, peer);
        owner.note_capability(capability);
        if capability.is_introducer() {
            eligible.push(peer);
        }
    }
    let decision = owner.decide(&eligible, true, wall_ms());
    assert_eq!(
        decision,
        IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer),
        "a peer that advertises no capability is never asked"
    );
    assert_eq!(owner.counters().requests_queued, 0);
    assert_eq!(
        a.handle.service().snapshot().relay_tags_issued,
        0,
        "no request left the router"
    );

    a.shutdown().await;
    b.shutdown().await;
}
