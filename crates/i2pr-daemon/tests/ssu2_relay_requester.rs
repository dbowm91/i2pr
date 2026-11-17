//! Plan 531's lane — the router must reach a peer through its introducer.
//!
//! The tree implements the SSU2 relay protocol's **introducer** side and
//! the **target's** tag acquisition, but the **requester** side was
//! unwired: the transport's requester state machine, the runtime's
//! relay-request seam and the runtime's hole-punch handler all existed and
//! all had zero production callers. A router could therefore not reach a
//! peer that is only reachable through an introducer, and the transport's
//! contradicting *relay-firewalled* class had no producer at all.
//!
//! The lane drives the **whole** exchange over real loopback UDP between
//! **three** real routers:
//!
//! ```text
//! Alice ──Relay Request──▶ Bob ──Relay Intro──▶ Charlie
//! Alice ◀─Relay Response── Bob ◀─Relay Response── Charlie
//! Alice ◀────Hole Punch──── Charlie
//! ```
//!
//! Alice is the requester, Bob is the introducer and Charlie is the
//! target. Every datagram crosses a real socket, every session is a real
//! authenticated session, and every decision in the exchange happens
//! inside the production code under test — the daemon's requester owner,
//! the daemon's introducer owner, the daemon's relay drain and the
//! runtime's own session loop.
//!
//! **One honest limit.** The *target* side's response emission has no
//! product wiring in this tree: the transport's target role says the
//! caller emits the accept and the hole punch, and no daemon role does
//! that yet. The lane therefore builds Charlie's accept over the
//! transport's own preimage with Charlie's own key — so Bob verifies it
//! for real — and sends the hole punch with the transport's own builder.
//! The **requester** half, which is this plan's subject, is production
//! code from the first byte to the last.
//!
//! The lane asserts counts, states and forms only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::relay_requester::{
    IntroducerEndpoint, LocalIdentitySigner, PublishedIntroducerSet, RelayRequesterDecision,
    RelayRequesterOwner, drive_relay_request, read_introducer_endpoint, read_published_introducers,
};
use i2pr_daemon::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerConfig, Ssu2IntroducerService,
};
use i2pr_daemon::ssu2_relay_drain::{
    RelayEventSource, RequesterEventSource, RuntimeRelayCarriage, drain_relay_events,
    drain_requester_events,
};
use i2pr_netdb::{
    RouterHash, RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, SigningPublicKey};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, RelayResponseBlock,
    Ssu2DialTarget, Ssu2Endpoint, Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RuntimeConfig,
    Ssu2RuntimeService, Ssu2ServiceHandle, Ssu2SocketConfig, build_hole_punch, hole_punch_conn_ids,
    relay_response_preimage,
};
use i2pr_transport::{AddressFamily, PeerId, ReachabilityState};
use rand_core::{OsRng, SeedableRng, TryRngCore};
use tokio::net::UdpSocket;

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The relay tag Bob issues for Charlie and Charlie publishes.
const RELAY_TAG: u32 = 0x0BAD_F00D;

/// The token Charlie's accept carries.
const RELAY_TOKEN: u64 = 0x0A0B_0C0D_0E0F_1011;

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

/// I2P-base64 (alphabet `A-Za-z0-9-~`, `=` padding) encoder for the
/// lane's RouterAddress construction.
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

/// One router's own keys.
struct RouterKeys {
    bundle: Arc<RouterIdentityBundle>,
    peer: PeerId,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    static_public_bytes: [u8; 32],
    intro: IntroKey,
    intro_bytes: [u8; 32],
    signing_public: SigningPublicKey,
}

impl RouterKeys {
    fn new() -> Self {
        let bundle = Arc::new(RouterIdentityBundle::generate(&mut OsRng).expect("identity"));
        let hash = bundle.identity().hash().expect("hash");
        let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
        let static_bytes = *static_key.secret_bytes();
        let static_public_bytes = static_key.public_bytes();
        let static_public = Ssu2PublicKey::new(static_public_bytes).expect("static public");
        let mut intro_bytes = [0_u8; 32];
        loop {
            OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
            if intro_bytes.iter().any(|byte| *byte != 0) {
                break;
            }
        }
        let intro = IntroKey::new(intro_bytes);
        let signing_public = bundle.signing_key().public_key().expect("signing public");
        Self {
            bundle,
            peer: PeerId::from_hash(hash),
            hash,
            static_bytes,
            static_public,
            static_public_bytes,
            intro,
            intro_bytes,
            signing_public,
        }
    }

    /// Builds the router's **direct** SSU2 address, naming its real bound
    /// endpoint.
    fn direct_address(&self, bound: SocketAddr) -> RouterAddress {
        let options = Mapping::from_entries(vec![
            ("host".to_string(), "127.0.0.1".to_string()),
            ("port".to_string(), bound.port().to_string()),
            ("v".to_string(), "2".to_string()),
            ("s".to_string(), i2p_b64_encode(&self.static_public_bytes)),
            ("i".to_string(), i2p_b64_encode(&self.intro_bytes)),
        ])
        .expect("options");
        RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("address")
    }

    /// Builds the router's own signed record, carrying a **direct** SSU2
    /// address bound to its static and introduction keys.
    ///
    /// The literal endpoint inside the record is a placeholder: dial
    /// addresses travel out of band, and the runtime binds peer identity
    /// through the static key, never the advertised endpoint.
    fn router_info_bytes(&self) -> Vec<u8> {
        let info = self
            .bundle
            .sign_router_info(
                Date::from_millis(wall_ms()),
                vec![self.direct_address("127.0.0.1:43000".parse().expect("addr"))],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        info.encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode")
    }

    /// Builds the router's **introducer-only** SSU2 address: no host and
    /// no port, and the published introducer group instead.
    fn introducer_address(&self, introducer_hash: &Hash, tag: u32) -> RouterAddress {
        let options = Mapping::from_entries(vec![
            ("mtu".to_string(), "1280".to_string()),
            ("v".to_string(), "2".to_string()),
            ("caps".to_string(), "4".to_string()),
            ("s".to_string(), i2p_b64_encode(&self.static_public_bytes)),
            ("i".to_string(), i2p_b64_encode(&self.intro_bytes)),
            (
                "ih0".to_string(),
                i2p_b64_encode(introducer_hash.as_bytes()),
            ),
            ("iexp0".to_string(), (wall_secs() + 3_600).to_string()),
            ("itag0".to_string(), tag.to_string()),
        ])
        .expect("options");
        RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("address")
    }
}

/// Signs one router's record and validates it into the authoritative
/// store, exactly as the netdb does on ingest.
fn validated_record(keys: &RouterKeys, address: RouterAddress) -> ValidatedRouterInfo {
    let info = keys
        .bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(wall_ms())),
    )
    .expect("validate")
}

/// A running router: its runtime service, its handle and its keys.
struct Fixture {
    service: Ssu2RuntimeService,
    handle: Ssu2ServiceHandle,
    scope: ChildScope,
    token: CancellationToken,
    keys: RouterKeys,
}

impl Fixture {
    async fn start(config: Ssu2RuntimeConfig, keys: RouterKeys) -> Self {
        let router_info = keys.router_info_bytes();
        let service = Ssu2RuntimeService::new(
            config,
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info,
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
        Self {
            service,
            handle,
            scope,
            token,
            keys,
        }
    }

    fn bound(&self) -> SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    async fn shutdown(self) {
        self.service.shutdown();
        self.scope.shutdown().await;
        self.token
            .cancel(i2pr_runtime::CancellationReason::TestHarnessTeardown);
    }
}

/// The daemon's validated store, with the peers the lane knows.
fn build_store(alice: &Fixture, bob: &Fixture, charlie: &Fixture) -> (RouterInfoStore, Hash, Hash) {
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    store.insert(validated_record(
        &alice.keys,
        alice.keys.direct_address(alice.bound()),
    ));
    store.insert(validated_record(
        &bob.keys,
        bob.keys.direct_address(bob.bound()),
    ));
    store.insert(validated_record(
        &charlie.keys,
        charlie.keys.introducer_address(&bob.keys.hash, RELAY_TAG),
    ));
    (store, bob.keys.hash, charlie.keys.hash)
}

/// A resolver that answers only from the validated store, exactly as the
/// production `NetDbSignerResolver` does.
struct StoreResolver {
    store: RouterInfoStore,
}

impl IntroducerSignerResolver for StoreResolver {
    fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey> {
        let key = RouterHash::from_bytes(*peer_hash);
        self.store
            .get(&key)
            .map(|record| record.router_info().router_identity().signing_key().clone())
    }
}

/// The lane's source over a runtime handle.
struct HandleSource<'a> {
    handle: &'a mut Ssu2ServiceHandle,
}

impl RelayEventSource for HandleSource<'_> {
    fn try_next_relay_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundRelay> {
        self.handle.try_next_relay_event()
    }
}

impl RequesterEventSource for HandleSource<'_> {
    fn try_next_requester_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundRequesterEvent> {
        self.handle.try_next_requester_event()
    }
}

/// Establishes one authenticated session between two fixtures.
async fn dial(from: &Fixture, to: &Fixture) {
    let target = Ssu2DialTarget::new(
        to.keys.peer,
        to.keys.hash,
        to.bound(),
        to.keys.static_public,
        to.keys.intro,
    )
    .expect("dial target");
    from.service
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial");
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

/// Drains Bob's relay handoff until the role owner decides something.
///
/// The drain is a **non-blocking pull**, so a pass that finds nothing is
/// not a decision: the lane waits for the datagram to arrive and drains
/// again, under a bounded deadline.
async fn drain_introducer_until_decided(
    handle: &mut Ssu2ServiceHandle,
    owner: &mut Ssu2IntroducerService,
    signers: &dyn IntroducerSignerResolver,
    carriage: &dyn i2pr_daemon::ssu2_relay_drain::RelayCarriage,
) -> i2pr_daemon::ssu2_relay_drain::RelayDrainOutcome {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let mut source = HandleSource { handle };
        let outcome = drain_relay_events(
            &mut source,
            owner,
            signers,
            carriage,
            wall_ms(),
            wall_secs(),
        );
        if outcome.admitted > 0 || outcome.refused > 0 {
            return outcome;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the introducer owner decided nothing"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Drains the requester's handoff until the owner decides something.
async fn drain_requester_until_decided(
    handle: &mut Ssu2ServiceHandle,
    owner: &mut RelayRequesterOwner,
    signers: &dyn IntroducerSignerResolver,
) -> i2pr_daemon::ssu2_relay_drain::RequesterDrainOutcome {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let mut source = HandleSource { handle };
        let outcome = drain_requester_events(&mut source, owner, signers, wall_ms(), wall_secs());
        if outcome.handled > 0 || outcome.refused > 0 {
            return outcome;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the requester owner decided nothing"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Pulls one relay event from a fixture's handoff, or fails the lane.
async fn next_relay_event(handle: &mut Ssu2ServiceHandle) -> i2pr_runtime::Ssu2InboundRelay {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if let Some(event) = handle.try_next_relay_event() {
            return event;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no relay event arrived"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ------------------------------------------------------------ the opt-in

/// The opt-in is off by default and is refused without the SSU2 service.
#[test]
fn the_opt_in_is_off_by_default_and_refused_without_the_service() {
    let minimal = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n";
    let config = i2pr_daemon::config::Config::parse(minimal).expect("strict profile");
    assert!(
        !config.ssu2.relay_requester,
        "the requester is an explicit opt-in and it is off by default"
    );

    let without_service = format!("{minimal}\n[ssu2]\nrelay_requester = true\n");
    let error = i2pr_daemon::config::Config::parse(&without_service)
        .expect_err("the requester needs the SSU2 service");
    let message = format!("{error}");
    assert!(
        message.contains("ssu2.relay_requester"),
        "the refusal names the key: {message}"
    );
}

// ------------------------------------------------------------ the reads

/// The published introducer set is read out of **validated** material,
/// and a peer that publishes none is never a target.
#[test]
fn only_a_peer_that_publishes_an_introducer_set_is_a_target() {
    let alice = RouterKeys::new();
    let bob = RouterKeys::new();
    let charlie = RouterKeys::new();

    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    store.insert(validated_record(
        &alice,
        alice.direct_address("127.0.0.1:43001".parse().expect("addr")),
    ));
    store.insert(validated_record(
        &bob,
        bob.direct_address("127.0.0.1:43002".parse().expect("addr")),
    ));
    store.insert(validated_record(
        &charlie,
        charlie.introducer_address(&bob.hash, RELAY_TAG),
    ));

    // Charlie publishes Bob as its introducer.
    let published = read_published_introducers(&store, charlie.peer);
    let PublishedIntroducerSet::Published(entries) = published else {
        panic!("the target publishes an introducer set");
    };
    assert_eq!(entries.len(), 1, "the published set is bounded");
    assert_eq!(entries[0].peer_hash, *bob.hash.as_bytes());
    assert_eq!(entries[0].relay_tag, RELAY_TAG);

    // Alice and Bob publish no introducer set at all.
    assert_eq!(
        read_published_introducers(&store, alice.peer),
        PublishedIntroducerSet::NoIntroducers
    );
    assert_eq!(
        read_published_introducers(&store, bob.peer),
        PublishedIntroducerSet::NoIntroducers
    );

    // A peer the store does not hold is not a target either, and the
    // refusal is distinct from the one above.
    assert_eq!(
        read_published_introducers(&store, PeerId::from_bytes([0x77; 32])),
        PublishedIntroducerSet::NoStoredRecord
    );

    // The introducer is resolved by hash from the same store.
    assert!(matches!(
        read_introducer_endpoint(&store, *bob.hash.as_bytes()),
        IntroducerEndpoint::Resolved(_)
    ));
    assert_eq!(
        read_introducer_endpoint(&store, [0x77; 32]),
        IntroducerEndpoint::NoStoredRecord
    );
}

// ------------------------------------------------------------ the exchange

/// The whole exchange, over real loopback UDP between three real routers.
#[tokio::test]
async fn the_router_reaches_a_peer_through_its_introducer() {
    // Alice is the requester, Bob the introducer, Charlie the target.
    let alice_config = Ssu2RuntimeConfig {
        relay_requester: true,
        ..Ssu2RuntimeConfig::default()
    };
    let bob_config = Ssu2RuntimeConfig {
        introducer_service: true,
        ..Ssu2RuntimeConfig::default()
    };
    let mut alice = Fixture::start(alice_config, RouterKeys::new()).await;
    let mut bob = Fixture::start(bob_config, RouterKeys::new()).await;
    let mut charlie = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;

    // Bob issues the relay tag that identifies Charlie, and Charlie
    // publishes the same tag.
    bob.service
        .issue_relay_tag(RELAY_TAG, charlie.keys.hash.as_bytes(), wall_secs())
        .expect("Bob issues the tag");

    // The authoritative store holds the three validated records.
    let (store, _bob_hash, charlie_hash) = build_store(&alice, &bob, &charlie);
    let (resolver_store, _bob_hash, _charlie_hash) = build_store(&alice, &bob, &charlie);
    let resolver = StoreResolver {
        store: resolver_store,
    };

    // Alice's requester owner: the daemon's own policy, with the
    // router's own identity as the signer.
    let signer = Arc::new(LocalIdentitySigner::new(Arc::clone(&alice.keys.bundle)));
    let mut requester = RelayRequesterOwner::new(true, alice.service.clone(), signer);

    // Two real sessions: Alice with Bob, and Bob with Charlie.
    dial(&alice, &bob).await;
    wait_for_active(&alice, 1).await;
    wait_for_active(&bob, 1).await;
    dial(&bob, &charlie).await;
    wait_for_active(&bob, 2).await;
    wait_for_active(&charlie, 1).await;

    // Alice's requester owner is the daemon's; the introducer owner is
    // Bob's.
    let mut introducer = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Default::default()
        },
        bob.service.clone(),
        *bob.keys.hash.as_bytes(),
    );
    let carriage = RuntimeRelayCarriage::new(bob.service.clone());

    // The corroboration the router holds **before** the exchange. The
    // relay's success may never add a corroborating class, so the count
    // must be exactly the same afterwards.
    let before = alice.service.reachability_evidence();
    assert!(
        before.corroboration < 2,
        "one session is far below the corroboration floor"
    );

    // --- the outbound half -------------------------------------------------
    let live = vec![bob.keys.peer];
    let local_endpoint = alice
        .service
        .local_bound_endpoint()
        .expect("the router names its own bound endpoint");
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let now_ms = wall_ms();
    let now_secs = wall_secs();
    let decision = drive_relay_request(
        &mut requester,
        &store,
        &live,
        &mut rng,
        Some(local_endpoint),
        now_ms,
        now_secs,
    )
    .expect("the window ran");
    let RelayRequesterDecision::Request(candidate) = decision else {
        panic!("the owner asked nobody: {decision:?}");
    };
    assert_eq!(candidate.target, charlie.keys.peer);
    assert_eq!(candidate.introducer, bob.keys.peer);
    assert_eq!(candidate.tag, RELAY_TAG, "the tag is the published one");
    assert_eq!(
        candidate.endpoint,
        local_endpoint_of(&bob),
        "the introducer is resolved by hash from the store"
    );
    assert_eq!(
        requester.counters().requests_started,
        1,
        "exactly one request leaves per window"
    );

    // --- Bob admits the request and forwards an introduction ---------------
    let drained =
        drain_introducer_until_decided(&mut bob.handle, &mut introducer, &resolver, &carriage)
            .await;
    assert_eq!(drained.admitted, 1, "Bob admitted the request");
    assert_eq!(drained.carried, 1, "Bob carried the introduction");

    // --- Charlie admits the introduction -----------------------------------
    let charlie_source = HandleSource {
        handle: &mut charlie.handle,
    };
    let intro = loop {
        let event = next_relay_event(charlie_source.handle).await;
        if let i2pr_runtime::Ssu2RelayEvent::Intro(intro) = event.event {
            break intro;
        }
    };
    assert_eq!(
        intro.relay_tag(),
        RELAY_TAG,
        "the introduction carries the published tag"
    );
    assert_eq!(
        *intro.alice_hash(),
        *alice.keys.hash.as_bytes(),
        "the introduction names the requester"
    );
    assert!(
        charlie
            .service
            .on_target_relay_intro(
                &intro,
                bob.keys.hash.as_bytes(),
                charlie_hash.as_bytes(),
                &alice.keys.signing_public,
                wall_secs(),
                wall_ms(),
            )
            .expect("Charlie admits the introduction"),
        "the target admits the forwarded request"
    );

    // --- Charlie answers --------------------------------------------------
    // The target role says the caller emits the accept: the lane builds
    // it over the transport's own preimage with Charlie's own key, so
    // Bob verifies it for real.
    let nonce = intro.nonce();
    let timestamp = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let charlie_endpoint = Ssu2Endpoint::from_socket_addr(charlie.bound()).expect("endpoint");
    let preimage = relay_response_preimage(
        bob.keys.hash.as_bytes(),
        nonce,
        timestamp,
        2,
        Some(charlie_endpoint),
    );
    let signature = charlie
        .keys
        .bundle
        .signing_key()
        .sign(&preimage)
        .expect("sign")
        .as_bytes()
        .to_vec();
    let accept = RelayResponseBlock::accept(
        nonce,
        timestamp,
        2,
        charlie_endpoint,
        signature,
        RELAY_TOKEN,
    )
    .expect("accept");
    charlie
        .service
        .queue_relay_response(bob.keys.peer, accept.clone())
        .expect("Charlie answers Bob");

    // --- Bob verifies and forwards the response ----------------------------
    let drained =
        drain_introducer_until_decided(&mut bob.handle, &mut introducer, &resolver, &carriage)
            .await;
    assert_eq!(drained.admitted, 1, "Bob verified the response");
    assert_eq!(drained.carried, 1, "Bob carried the response to Alice");

    // --- Alice correlates the response -------------------------------------
    let drained = drain_requester_until_decided(&mut alice.handle, &mut requester, &resolver).await;
    assert_eq!(drained.drained, 1, "Alice saw one requester event");
    assert_eq!(drained.handled, 1, "Alice correlated the response");
    assert_eq!(drained.refused, 0, "the response was not refused");
    assert_eq!(
        requester.counters().responses_accepted,
        1,
        "the response was accepted"
    );
    assert_eq!(
        requester.counters().responses_correlated,
        1,
        "the response correlated with the live request"
    );

    // --- Charlie punches the hole ------------------------------------------
    // The hole punch travels **out of session**, sealed under Alice's own
    // introduction key, so it arrives on Alice's socket from a source
    // that holds no session with her.
    let (destination, source) = hole_punch_conn_ids(nonce);
    let punch = build_hole_punch(
        &alice.keys.intro,
        source,
        destination,
        0x7777_7777,
        timestamp,
        charlie_endpoint,
        accept,
    )
    .expect("hole punch");
    let raw = UdpSocket::bind("127.0.0.1:0").await.expect("raw socket");
    raw.send_to(&punch, alice.bound()).await.expect("send");

    let drained = drain_requester_until_decided(&mut alice.handle, &mut requester, &resolver).await;
    assert_eq!(drained.drained, 1, "Alice saw the hole punch");
    assert_eq!(drained.handled, 1, "the hole punch was correlated");
    assert_eq!(
        drained.firewalled, 1,
        "the completed exchange records the relay-firewalled fact"
    );
    assert_eq!(
        requester.counters().hole_punches_ready,
        1,
        "the hole punch completed the exchange"
    );
    assert_eq!(
        requester.counters().firewalled_recorded,
        1,
        "the fact was recorded once"
    );

    // --- the fact contradicts reachability ---------------------------------
    let after = alice.service.reachability_evidence();
    assert_eq!(
        after.corroboration, before.corroboration,
        "the relay's success never adds a corroborating class"
    );
    assert_eq!(
        after.state,
        ReachabilityState::Firewalled,
        "a router that reaches a peer through a relay is not directly reachable"
    );
    assert_eq!(after.family, AddressFamily::Ipv4);

    // --- no refusal wrote into a peer's profile ----------------------------
    let charlie_record = store
        .get(&RouterHash::from_hash(charlie_hash))
        .expect("Charlie's record");
    let published = read_published_introducers(&store, charlie.keys.peer);
    let PublishedIntroducerSet::Published(entries) = published else {
        panic!("the target still publishes an introducer set");
    };
    assert_eq!(
        entries[0].relay_tag, RELAY_TAG,
        "a peer's profile is never rewritten by a local condition"
    );
    assert_eq!(
        charlie_record.router_info().router_identity().signing_key(),
        &charlie.keys.signing_public,
        "a peer's record is never rewritten"
    );

    alice.shutdown().await;
    bob.shutdown().await;
    charlie.shutdown().await;
}

/// The local endpoint the introducer's published record names.
fn local_endpoint_of(fixture: &Fixture) -> Ssu2Endpoint {
    Ssu2Endpoint::from_socket_addr(fixture.bound()).expect("endpoint")
}

// ------------------------------------------------------------ the refusals

/// Every refusal is typed, counted under its own reason, and leaves no
/// state behind.
#[tokio::test]
async fn every_refusal_is_typed_and_counted_and_writes_nothing() {
    let alice = Fixture::start(
        Ssu2RuntimeConfig {
            relay_requester: true,
            ..Ssu2RuntimeConfig::default()
        },
        RouterKeys::new(),
    )
    .await;
    let bob = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;
    let charlie = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;

    let (store, _bob_hash, _charlie_hash) = build_store(&alice, &bob, &charlie);
    let signer = Arc::new(LocalIdentitySigner::new(Arc::clone(&alice.keys.bundle)));

    // A **disabled** owner refuses every window and starts nothing.
    let mut disabled = RelayRequesterOwner::new(false, alice.service.clone(), signer.clone());
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let decision = drive_relay_request(
        &mut disabled,
        &store,
        &[bob.keys.peer],
        &mut rng,
        alice.service.local_bound_endpoint(),
        wall_ms(),
        wall_secs(),
    )
    .expect("the window ran");
    assert!(matches!(
        decision,
        RelayRequesterDecision::Refuse(
            i2pr_daemon::relay_requester::RelayRequesterRefusal::Disabled
        )
    ));
    assert_eq!(disabled.counters().refusals_disabled, 1);
    assert_eq!(disabled.counters().requests_started, 0);

    // An **enabled** owner with no eligible target asks nobody. The
    // candidate pool is the NetDB, so an empty store is the case.
    let empty_store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    let mut enabled = RelayRequesterOwner::new(true, alice.service.clone(), signer);
    let decision = drive_relay_request(
        &mut enabled,
        &empty_store,
        &[bob.keys.peer],
        &mut rng,
        alice.service.local_bound_endpoint(),
        wall_ms(),
        wall_secs(),
    )
    .expect("the window ran");
    assert!(matches!(
        decision,
        RelayRequesterDecision::Refuse(
            i2pr_daemon::relay_requester::RelayRequesterRefusal::NoEligibleTarget
        )
    ));
    assert_eq!(enabled.counters().refusals_no_eligible_target, 1);

    // The router names no bound endpoint, so it can present none.
    let mut nameless = RelayRequesterOwner::new(
        true,
        alice.service.clone(),
        Arc::new(i2pr_daemon::relay_requester::LocalIdentitySigner::new(
            Arc::clone(&alice.keys.bundle),
        )),
    );
    let decision = drive_relay_request(
        &mut nameless,
        &store,
        &[bob.keys.peer],
        &mut rng,
        None,
        wall_ms(),
        wall_secs(),
    )
    .expect("the window ran");
    assert!(matches!(
        decision,
        RelayRequesterDecision::Refuse(
            i2pr_daemon::relay_requester::RelayRequesterRefusal::NoLocalEndpoint
        )
    ));
    assert_eq!(nameless.counters().refusals_no_local_endpoint, 1);

    // Nothing was written into any peer's record.
    for keys in [&alice.keys, &bob.keys, &charlie.keys] {
        let record = store
            .get(&RouterHash::from_hash(keys.hash))
            .expect("the record is untouched");
        assert_eq!(
            record.router_info().router_identity().signing_key(),
            &keys.signing_public,
            "a peer's record is never rewritten"
        );
    }

    alice.shutdown().await;
    bob.shutdown().await;
    charlie.shutdown().await;
}

/// A request whose tag does not identify the target is refused.
#[tokio::test]
async fn a_request_is_refused_when_the_tag_does_not_identify_the_target() {
    let alice = Fixture::start(
        Ssu2RuntimeConfig {
            relay_requester: true,
            ..Ssu2RuntimeConfig::default()
        },
        RouterKeys::new(),
    )
    .await;
    let mut bob = Fixture::start(
        Ssu2RuntimeConfig {
            introducer_service: true,
            ..Ssu2RuntimeConfig::default()
        },
        RouterKeys::new(),
    )
    .await;
    let charlie = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;

    // Bob issues a tag for **Alice**, not for Charlie. A request that
    // presents it names Alice, and Charlie's published tag is a
    // different one.
    bob.service
        .issue_relay_tag(RELAY_TAG, alice.keys.hash.as_bytes(), wall_secs())
        .expect("Bob issues the tag");

    let (store, _bob_hash, _charlie_hash) = build_store(&alice, &bob, &charlie);
    let (resolver_store, _bob_hash, _charlie_hash) = build_store(&alice, &bob, &charlie);
    let resolver = StoreResolver {
        store: resolver_store,
    };
    let signer = Arc::new(LocalIdentitySigner::new(Arc::clone(&alice.keys.bundle)));
    let mut requester = RelayRequesterOwner::new(true, alice.service.clone(), signer);

    dial(&alice, &bob).await;
    wait_for_active(&alice, 1).await;
    wait_for_active(&bob, 1).await;

    let mut introducer = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Default::default()
        },
        bob.service.clone(),
        *bob.keys.hash.as_bytes(),
    );
    let carriage = RuntimeRelayCarriage::new(bob.service.clone());

    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let live = vec![bob.keys.peer];
    let decision = drive_relay_request(
        &mut requester,
        &store,
        &live,
        &mut rng,
        alice.service.local_bound_endpoint(),
        wall_ms(),
        wall_secs(),
    )
    .expect("the window ran");
    let RelayRequesterDecision::Request(candidate) = decision else {
        panic!("the owner asked nobody: {decision:?}");
    };
    assert_eq!(candidate.tag, RELAY_TAG, "the tag is the published one");

    // Bob's tag names Alice, so Bob refuses the request: the tag does
    // not identify the target the request names.
    let drained =
        drain_introducer_until_decided(&mut bob.handle, &mut introducer, &resolver, &carriage)
            .await;
    assert_eq!(drained.admitted, 0, "Bob admitted nothing");
    assert_eq!(drained.refused, 1, "Bob refused the request");
    assert_eq!(drained.carried, 0, "no introduction was carried");

    // The requester's own state is untouched: the response never came.
    assert_eq!(
        requester.counters().responses_correlated,
        0,
        "no response correlated"
    );
    assert_eq!(
        requester.counters().hole_punches_ready,
        0,
        "no hole punch completed the exchange"
    );

    alice.shutdown().await;
    bob.shutdown().await;
    charlie.shutdown().await;
}

/// The transport's reachability model is untouched.
#[test]
fn the_transports_reachability_model_is_untouched() {
    use i2pr_transport::{
        MIN_CONTRADICTION_FLOOR, MIN_CORROBORATION_FLOOR, PeerTestOutcomeKind, ReachabilityPolicy,
        ReachabilitySignal, ReachabilityTracker,
    };
    assert_eq!(
        MIN_CORROBORATION_FLOOR, 2,
        "the corroboration floor stays at two distinct supporting classes"
    );
    assert_eq!(
        MIN_CONTRADICTION_FLOOR, 1,
        "the contradiction floor stays at one distinct contradicting class"
    );
    let policy = ReachabilityPolicy::default();
    assert_eq!(policy.min_corroboration(), 2);
    assert_eq!(policy.min_contradiction(), 1);

    // The relay-firewalled signal is class 4, and it **contradicts**.
    let mut tracker = ReachabilityTracker::new(policy).expect("tracker");
    let now = Duration::from_secs(500);
    tracker.record(
        ReachabilitySignal::RelayFirewalledSignal {
            family: AddressFamily::Ipv4,
        },
        now,
    );
    assert_eq!(
        tracker.state(),
        ReachabilityState::Firewalled,
        "the relay's success is a firewalled verdict"
    );
    assert_eq!(
        tracker.snapshot(now).corroboration,
        0,
        "the relay-firewalled signal never supports reachability"
    );

    // A confirming peer test on top of it does not make the router
    // reachable while the contradiction stands.
    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::Confirmed,
        },
        now,
    );
    assert_eq!(
        tracker.state(),
        ReachabilityState::Firewalled,
        "the contradiction takes precedence"
    );
}

/// The store the lane builds is the authoritative one, and the reads
/// never mutate it.
#[test]
fn the_reads_never_mutate_the_store() {
    let keys = RouterKeys::new();
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    store.insert(validated_record(
        &keys,
        keys.direct_address("127.0.0.1:43003".parse().expect("addr")),
    ));
    let before = store.len();
    let _ = read_published_introducers(&store, keys.peer);
    let _ = read_introducer_endpoint(&store, *keys.hash.as_bytes());
    assert_eq!(store.len(), before, "the read never mutates the store");
}

/// The keys the lane builds are distinct.
#[test]
fn the_lanes_routers_are_distinct() {
    let mut hashes: HashMap<[u8; 32], u8> = HashMap::new();
    for _ in 0..3 {
        let keys = RouterKeys::new();
        let hash = *keys.hash.as_bytes();
        assert!(
            hashes.insert(hash, 1).is_none(),
            "each router has its own identity"
        );
    }
}
