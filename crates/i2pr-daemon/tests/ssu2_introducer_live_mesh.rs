//! Plan 499 (WP5) — the live three-party SSU2 relay introduction.
//!
//! Alice asks Bob for an introduction to Charlie.
//!
//! - **Alice** is the requester. She asks Bob for an introduction and
//!   presents the relay tag.
//! - **Bob** is this router: the introducer. His runtime holds the
//!   sessions, his daemon role owner admits the request, verifies Alice's
//!   signature, produces a Relay Intro, and later verifies Charlie's Relay
//!   Response and forwards it.
//! - **Charlie** is the target. He asks Bob for a relay tag, receives one,
//!   receives Bob's Relay Intro and answers with a Relay Response.
//!
//! Every byte of the introduction crosses a real loopback UDP socket
//! through the real runtime; the lane proves the introduction *completes on
//! the wire*, not merely that the role owner decided.
//!
//! The only seam the test supplies is the daemon's **pull** seam over the
//! runtime's relay handoff (`RelayEventSource`), because the daemon's
//! production `NormalSsu2Owner` is private to the daemon crate. The signer
//! resolver is a **test** resolver: the production `NetDbSignerResolver`
//! answers only from the NetDB's validated RouterInfo store, which needs a
//! `Bootstrap` holding validated RouterInfos — a separate concern with its
//! own rows.
//!
//! Assertions are counts and digests only: no key, tag, peer hash,
//! signature or payload is ever printed.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{OsRng, RouterIdentityBundle, X25519PrivateKey, verify_signature};
use i2pr_daemon::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerConfig, Ssu2IntroducerService,
};
use i2pr_daemon::ssu2_relay_drain::{
    RelayCarriage, RelayDrainOutcome, RelayEventSource, RuntimeRelayCarriage, drain_relay_events,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, SignatureValue, SigningPublicKey};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, RelayRequestBlock,
    RelayResponseBlock, Ssu2DialTarget, Ssu2Endpoint, Ssu2EstablishedLink, Ssu2IdentityMaterial,
    Ssu2InboundRelay, Ssu2PublicKey, Ssu2RelayEvent, Ssu2RuntimeConfig, Ssu2RuntimeService,
    Ssu2ServiceHandle, Ssu2SocketConfig, constants, relay_request_preimage,
    relay_response_preimage,
};
use i2pr_transport::PeerId;
use rand_core::TryRngCore;

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Bounded window used for the one negative assertion (no tag is issued).
const NEGATIVE_WINDOW: Duration = Duration::from_secs(2);
/// SSU2 relay version. The block codec accepts 1 or 2 only.
const RELAY_VERSION: u8 = 2;
/// Bound on the CSPRNG draws that may be rejected as degenerate.
const MAX_RANDOM_ATTEMPTS: usize = 16;

/// Wall-clock Unix seconds and milliseconds, read together.
///
/// The relay freshness window and the tag lifetime are wall-clock
/// (`RELAY_MAX_CLOCK_SKEW_SECONDS`), so the timestamps the peers put in
/// their blocks come from the same clock the runtime reads.
fn wall_clock() -> (u64, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(1_700_000_000));
    (now.as_secs(), now.as_millis() as u64)
}

/// One test router's identity material.
///
/// Unlike the runtime's own live-UDP harness, this **keeps** the
/// [`RouterIdentityBundle`]: Alice signs her Relay Request with it and
/// Charlie signs his Relay Response with it. Dropping the bundle is the
/// classic cause of a puzzling "invalid signature" failure.
struct RouterKeys {
    peer: PeerId,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
    bundle: RouterIdentityBundle,
}

impl RouterKeys {
    /// The router's 32-byte hash, as raw bytes.
    fn hash_bytes(&self) -> [u8; 32] {
        *self.hash.as_bytes()
    }
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
            let offset = 18 - 6 * index;
            output.push(ALPHABET[((n >> offset) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

/// Builds one test router identity: fresh OS randomness, an Ed25519 router
/// identity, a transport static key, an intro key, and a signed RouterInfo
/// carrying a matching SSU2 address.
///
/// The literal endpoint inside the RouterInfo is a placeholder: dial
/// addresses travel out-of-band, and the runtime binds peer identity
/// through the static key, never the advertised endpoint.
fn make_router_keys() -> RouterKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_key.secret_bytes();
    let static_public = Ssu2PublicKey::new(static_key.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    for _ in 0..MAX_RANDOM_ATTEMPTS {
        OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    assert!(
        intro_bytes.iter().any(|byte| *byte != 0),
        "the intro key must not be all zero"
    );
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
            Date::from_millis(wall_clock().0.saturating_mul(1000)),
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
        bundle,
    }
}

/// One live router: its runtime service, the scope that owns its tasks,
/// the handle that owns the inbound handoff receivers, and its identity.
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

/// Starts a router with the introducer role switched on.
///
/// The default is off; with it off every relay request and every tag
/// request is refused with `ServiceDisabled`.
async fn start_introducer(keys: RouterKeys) -> Fixture {
    start_fixture_with_config(
        Ssu2RuntimeConfig {
            introducer_service: true,
            ..Ssu2RuntimeConfig::default()
        },
        keys,
    )
    .await
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

/// Polls the runtime's relay handoff until an event matching `predicate`
/// arrives.
///
/// Events that do not match are consumed: the handoff is a single ordered
/// stream, and the lane only ever expects one tag before one intro (or one
/// response). The wait is bounded by a deadline and polls; it never sleeps
/// for a fixed interval and never loops without a bound.
async fn wait_for_relay_event(
    handle: &mut Ssu2ServiceHandle,
    predicate: impl Fn(&Ssu2RelayEvent) -> bool,
) -> Ssu2InboundRelay {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        while let Some(event) = handle.try_next_relay_event() {
            if predicate(&event.event) {
                return event;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the relay handoff never delivered the expected event"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Asserts that **no** relay event matching `predicate` arrives inside a
/// bounded window.
///
/// This is the only negative wait in the lane. It returns as soon as a
/// matching event shows up (which fails the assertion), so it never holds
/// the test for the whole window when the expectation is violated.
async fn assert_no_relay_event(
    handle: &mut Ssu2ServiceHandle,
    predicate: impl Fn(&Ssu2RelayEvent) -> bool,
) {
    let deadline = tokio::time::Instant::now() + NEGATIVE_WINDOW;
    loop {
        while let Some(event) = handle.try_next_relay_event() {
            assert!(
                !predicate(&event.event),
                "the relay handoff delivered an event that must not arrive"
            );
        }
        if tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// The daemon's pull seam over the runtime's live relay handoff.
///
/// The production owner (`NormalSsu2Owner`) is private to the daemon
/// crate, so the lane supplies the same pull shape over the runtime's own
/// handle. The seam cannot enqueue into the runtime's relay channel: it
/// only reads what the authenticated transport path already delivered.
struct HandleSource<'a> {
    handle: &'a mut Ssu2ServiceHandle,
}

impl RelayEventSource for HandleSource<'_> {
    fn try_next_relay_event(&mut self) -> Option<Ssu2InboundRelay> {
        self.handle.try_next_relay_event()
    }
}

/// A test signer resolver.
///
/// The production `NetDbSignerResolver` answers only from the NetDB's
/// validated RouterInfo store. This lane exercises the role owner against
/// the resolver seam itself, with the peers' own signing keys.
struct MeshSigners {
    keys: HashMap<[u8; 32], SigningPublicKey>,
}

impl MeshSigners {
    fn new(peers: &[&RouterKeys]) -> Self {
        let mut keys = HashMap::new();
        for peer in peers {
            keys.insert(
                peer.hash_bytes(),
                peer.bundle.identity().signing_key().clone(),
            );
        }
        Self { keys }
    }
}

impl IntroducerSignerResolver for MeshSigners {
    fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey> {
        self.keys.get(peer_hash).cloned()
    }
}

/// Drains the introducer's live relay handoff until the role owner admits
/// or refuses an event.
///
/// The pass is bounded by the daemon's own per-pass bound; this loop keeps
/// passing until the role owner decides something, and is bounded by a
/// deadline so a silent handoff fails instead of hanging.
async fn drain_until_decided(
    handle: &mut Ssu2ServiceHandle,
    owner: &mut Ssu2IntroducerService,
    signers: &dyn IntroducerSignerResolver,
    carriage: &dyn RelayCarriage,
) -> RelayDrainOutcome {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let (now_secs, now_ms) = wall_clock();
        let mut source = HandleSource {
            handle: &mut *handle,
        };
        let outcome = drain_relay_events(&mut source, owner, signers, carriage, now_ms, now_secs);
        if outcome.admitted > 0 || outcome.refused > 0 {
            return outcome;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the introducer's relay handoff never delivered a decidable event"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// A fresh, nonzero nonce.
///
/// The transport treats the same nonce, owner and tag as an idempotent
/// replay rather than a second introduction, so every request needs a
/// nonce that has not been used on this mesh.
fn fresh_nonce() -> u32 {
    let mut bytes = [0_u8; 4];
    for _ in 0..MAX_RANDOM_ATTEMPTS {
        OsRng.try_fill_bytes(&mut bytes).expect("rng");
        let nonce = u32::from_be_bytes(bytes);
        if nonce != 0 {
            return nonce;
        }
    }
    panic!("the CSPRNG did not produce a nonzero nonce");
}

/// A fresh, nonzero response token.
fn fresh_token() -> u64 {
    let mut bytes = [0_u8; 8];
    for _ in 0..MAX_RANDOM_ATTEMPTS {
        OsRng.try_fill_bytes(&mut bytes).expect("rng");
        let token = u64::from_be_bytes(bytes);
        if token != 0 {
            return token;
        }
    }
    panic!("the CSPRNG did not produce a nonzero token");
}

/// The loopback endpoint of a live router.
fn loopback_endpoint(fixture: &Fixture) -> Ssu2Endpoint {
    Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::LOCALHOST), fixture.addr().port())
        .expect("loopback endpoint")
}

/// Plan 499 (WP5): the whole introduction, over real loopback UDP.
///
/// The chain is one causal story — Charlie's tag, Alice's request, Bob's
/// intro, Charlie's response, Bob's forward, Alice's verification — so it
/// is one test. Splitting it would lose the proof.
#[tokio::test]
async fn live_three_party_relay_introduction_completes_over_real_udp() {
    // --- Step 1: three live routers. -----------------------------------
    // Bob runs the introducer role; the default is off.
    let mut alice = start_fixture(make_router_keys()).await;
    let mut bob = start_introducer(make_router_keys()).await;
    let mut charlie = start_fixture(make_router_keys()).await;

    let bob_hash = bob.keys.hash_bytes();
    let charlie_hash = charlie.keys.hash_bytes();
    let alice_hash = alice.keys.hash_bytes();

    // --- Step 2: two sessions on Bob. -----------------------------------
    // Bob's registry is keyed by `PeerId`, so the responder side registers
    // the session too: Bob ends up with both peers.
    let _alice_link = dial(&alice, &bob.keys, bob.addr()).await;
    let _charlie_link = dial(&charlie, &bob.keys, bob.addr()).await;
    wait_for_active(&alice, 1).await;
    wait_for_active(&charlie, 1).await;
    wait_for_active(&bob, 2).await;

    // --- Step 3: Charlie obtains a tag. ---------------------------------
    // Charlie asks Bob for a relay tag. Bob's runtime observes the request
    // on the authenticated session, mints a tag, binds it to the session
    // peer (Charlie — the tag names the *target*), and answers on the
    // same session.
    charlie
        .service
        .queue_relay_tag_request(PeerId::from_hash(bob.keys.hash))
        .expect("queue the relay tag request");

    let tag_event = wait_for_relay_event(&mut charlie.handle, |event| {
        matches!(event, Ssu2RelayEvent::Tag(_))
    })
    .await;
    let tag = match tag_event.event {
        Ssu2RelayEvent::Tag(tag) => tag,
        _ => unreachable!("the predicate only accepts a tag"),
    };
    assert!(tag != 0, "the minted relay tag must be nonzero");
    assert!(
        bob.service.relay_tag_owner(tag, wall_clock().0) == Some(charlie_hash),
        "the tag must resolve to the target's router hash"
    );

    // --- Step 4: Alice presents the tag. -------------------------------
    // Alice signs the specification's request preimage over **Bob's** and
    // **Charlie's** hashes. Alice's own hash is not in the preimage: her
    // identity comes from the session and only selects the verification
    // key, so the request is not cryptographically bound to her hash.
    let nonce = fresh_nonce();
    let (now_secs, _now_ms) = wall_clock();
    let timestamp = u32::try_from(now_secs).expect("timestamp fits u32");
    let alice_endpoint = loopback_endpoint(&alice);
    let request_preimage = relay_request_preimage(
        &bob_hash,
        &charlie_hash,
        nonce,
        tag,
        timestamp,
        RELAY_VERSION,
        alice_endpoint,
    );
    let request_signature = alice
        .keys
        .bundle
        .signing_key()
        .sign(&request_preimage)
        .expect("sign the relay request");
    let request = RelayRequestBlock::new(
        nonce,
        tag,
        timestamp,
        RELAY_VERSION,
        alice_endpoint,
        request_signature.as_bytes().to_vec(),
    )
    .expect("relay request block");
    alice
        .service
        .queue_relay_request(PeerId::from_hash(bob.keys.hash), request)
        .expect("queue the relay request");

    // --- Step 5: Bob admits the request and produces the intro. ---------
    let mut owner = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Ssu2IntroducerConfig::default()
        },
        bob.service.clone(),
        bob_hash,
    );
    let signers = MeshSigners::new(&[&alice.keys, &charlie.keys]);
    let carriage = RuntimeRelayCarriage::new(bob.service.clone());

    let request_outcome =
        drain_until_decided(&mut bob.handle, &mut owner, &signers, &carriage).await;
    assert_eq!(request_outcome.drained, 1, "one relay event was drained");
    assert_eq!(request_outcome.admitted, 1, "the request was admitted");
    assert_eq!(request_outcome.refused, 0, "the request was not refused");
    assert_eq!(
        request_outcome.carried, 1,
        "the produced intro was put on Bob's session with Charlie"
    );
    assert!(
        request_outcome.carriage_refusals.is_empty(),
        "the carriage refused to carry the produced intro"
    );
    assert_eq!(owner.receipt().requests_admitted, 1);
    assert_eq!(owner.receipt().intros_produced, 1);

    // --- Step 6: Charlie receives the intro and answers. ----------------
    let intro_event = wait_for_relay_event(&mut charlie.handle, |event| {
        matches!(event, Ssu2RelayEvent::Intro(_))
    })
    .await;
    let intro = match intro_event.event {
        Ssu2RelayEvent::Intro(intro) => intro,
        _ => unreachable!("the predicate only accepts an intro"),
    };
    assert!(
        *intro.alice_hash() == alice_hash,
        "the intro must name Alice as the requester"
    );
    assert!(intro.relay_tag() == tag, "the intro must carry the tag");
    assert!(intro.nonce() == nonce, "the intro must carry the nonce");
    assert!(
        intro.version() == RELAY_VERSION,
        "the intro must carry the relay version"
    );

    // Charlie signs the specification's response preimage over **Bob's**
    // hash, the nonce he was given, his timestamp, the version and his own
    // endpoint.
    let (now_secs, _now_ms) = wall_clock();
    let timestamp = u32::try_from(now_secs).expect("timestamp fits u32");
    let charlie_endpoint = loopback_endpoint(&charlie);
    let response_preimage = relay_response_preimage(
        &bob_hash,
        nonce,
        timestamp,
        RELAY_VERSION,
        Some(charlie_endpoint),
    );
    let response_signature = charlie
        .keys
        .bundle
        .signing_key()
        .sign(&response_preimage)
        .expect("sign the relay response");
    let response = RelayResponseBlock::accept(
        nonce,
        timestamp,
        RELAY_VERSION,
        charlie_endpoint,
        response_signature.as_bytes().to_vec(),
        fresh_token(),
    )
    .expect("relay response block");
    charlie
        .service
        .queue_relay_response(PeerId::from_hash(bob.keys.hash), response)
        .expect("queue the relay response");

    // --- Step 7: Bob verifies the response and forwards it. --------------
    let response_outcome =
        drain_until_decided(&mut bob.handle, &mut owner, &signers, &carriage).await;
    assert_eq!(response_outcome.drained, 1, "one relay event was drained");
    assert_eq!(response_outcome.admitted, 1, "the response was admitted");
    assert_eq!(response_outcome.refused, 0, "the response was not refused");
    assert_eq!(
        response_outcome.carried, 1,
        "the verified response was put on Bob's session with Alice"
    );
    assert!(
        response_outcome.carriage_refusals.is_empty(),
        "the carriage refused to carry the verified response"
    );
    assert_eq!(owner.receipt().responses_forwarded, 1);

    // --- Step 8: Alice receives the response. ---------------------------
    let response_event = wait_for_relay_event(&mut alice.handle, |event| {
        matches!(event, Ssu2RelayEvent::Response(_))
    })
    .await;
    let received = match response_event.event {
        Ssu2RelayEvent::Response(block) => block,
        _ => unreachable!("the predicate only accepts a response"),
    };
    assert!(
        received.nonce() == nonce,
        "the response must correlate with the request's nonce"
    );
    assert!(
        received.code().is_accept(),
        "the response must be an acceptance"
    );

    // The sharpest assertion in the lane: the response's signature must
    // verify under **Charlie's** public key over the exact preimage the
    // transport uses. This proves the response really came from Charlie and
    // was not forged.
    let charlie_public = charlie.keys.bundle.identity().signing_key().clone();
    let verification_preimage = relay_response_preimage(
        &bob_hash,
        received.nonce(),
        received.timestamp(),
        received.version(),
        received.endpoint(),
    );
    let value = SignatureValue::new(charlie_public.key_type(), received.signature().to_vec())
        .expect("signature value");
    assert!(
        verify_signature(&charlie_public, &verification_preimage, &value).is_ok(),
        "the response signature must verify under Charlie's public key"
    );

    // --- Step 9: teardown. ----------------------------------------------
    alice.shutdown_scope().await;
    bob.shutdown_scope().await;
    charlie.shutdown_scope().await;
}

/// Plan 499 (WP5): the introducer role is **off by default**.
///
/// With `introducer_service` false, Bob observes Charlie's tag request on
/// the live session and mints no tag: nothing is issued, nothing is
/// observed by the requester, and no tag event reaches the handoff.
///
/// The lane is asynchronous because the property is about a live session:
/// the request has to cross a real UDP socket for the refusal to mean
/// anything.
#[tokio::test]
async fn the_introducer_role_is_off_by_default_so_no_tag_is_issued() {
    let bob = start_fixture(make_router_keys()).await;
    let mut charlie = start_fixture(make_router_keys()).await;

    let _charlie_link = dial(&charlie, &bob.keys, bob.addr()).await;
    wait_for_active(&bob, 1).await;
    wait_for_active(&charlie, 1).await;

    charlie
        .service
        .queue_relay_tag_request(PeerId::from_hash(bob.keys.hash))
        .expect("queue the relay tag request");

    // The request crossed the wire and Bob observed it on the
    // authenticated session. Waiting for the observation makes the
    // "no tag" conclusion independent of a timing window.
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if bob.service.snapshot().relay_tag_requests >= 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the tag request was never observed on Bob's live session"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    assert_eq!(
        bob.service.snapshot().relay_tags_issued,
        0,
        "no relay tag may be issued while the introducer role is off"
    );
    assert_eq!(
        charlie.service.snapshot().relay_tags_observed,
        0,
        "the requester may not observe a tag while the introducer role is off"
    );
    assert_no_relay_event(&mut charlie.handle, |event| {
        matches!(event, Ssu2RelayEvent::Tag(_))
    })
    .await;

    bob.shutdown_scope().await;
    charlie.shutdown_scope().await;
}

/// Plan 499 (WP5): the relay tag is bound to the **target**, not to the
/// peer that happened to ask.
///
/// Two peers each ask Bob for a tag. Each tag must resolve to its own
/// requester's router hash and to no other, and an unknown tag must
/// resolve to nothing at all. This is the assertion that catches a tag
/// bound to the wrong peer.
#[tokio::test]
async fn the_issued_tag_resolves_to_its_own_target_and_to_nothing_else() {
    let bob = start_introducer(make_router_keys()).await;
    let mut charlie = start_fixture(make_router_keys()).await;
    let mut dave = start_fixture(make_router_keys()).await;

    let charlie_hash = charlie.keys.hash_bytes();
    let dave_hash = dave.keys.hash_bytes();

    let _charlie_link = dial(&charlie, &bob.keys, bob.addr()).await;
    let _dave_link = dial(&dave, &bob.keys, bob.addr()).await;
    wait_for_active(&bob, 2).await;
    wait_for_active(&charlie, 1).await;
    wait_for_active(&dave, 1).await;

    charlie
        .service
        .queue_relay_tag_request(PeerId::from_hash(bob.keys.hash))
        .expect("queue Charlie's tag request");
    dave.service
        .queue_relay_tag_request(PeerId::from_hash(bob.keys.hash))
        .expect("queue Dave's tag request");

    let charlie_tag = match wait_for_relay_event(&mut charlie.handle, |event| {
        matches!(event, Ssu2RelayEvent::Tag(_))
    })
    .await
    .event
    {
        Ssu2RelayEvent::Tag(tag) => tag,
        _ => unreachable!("the predicate only accepts a tag"),
    };
    let dave_tag = match wait_for_relay_event(&mut dave.handle, |event| {
        matches!(event, Ssu2RelayEvent::Tag(_))
    })
    .await
    .event
    {
        Ssu2RelayEvent::Tag(tag) => tag,
        _ => unreachable!("the predicate only accepts a tag"),
    };

    assert!(charlie_tag != 0, "the minted relay tag must be nonzero");
    assert!(dave_tag != 0, "the minted relay tag must be nonzero");
    assert!(
        charlie_tag != dave_tag,
        "a relay tag must identify exactly one target"
    );

    let now_secs = wall_clock().0;
    assert!(
        bob.service.relay_tag_owner(charlie_tag, now_secs) == Some(charlie_hash),
        "Charlie's tag must resolve to Charlie's router hash"
    );
    assert!(
        bob.service.relay_tag_owner(charlie_tag, now_secs) != Some(dave_hash),
        "Charlie's tag must not resolve to Dave's router hash"
    );
    assert!(
        bob.service.relay_tag_owner(dave_tag, now_secs) == Some(dave_hash),
        "Dave's tag must resolve to Dave's router hash"
    );
    assert!(
        bob.service.relay_tag_owner(dave_tag, now_secs) != Some(charlie_hash),
        "Dave's tag must not resolve to Charlie's router hash"
    );
    assert!(
        bob.service.relay_tag_owner(0xDEAD_BEEF, now_secs).is_none(),
        "an unknown relay tag must resolve to nothing"
    );

    bob.shutdown_scope().await;
    charlie.shutdown_scope().await;
    dave.shutdown_scope().await;
}
