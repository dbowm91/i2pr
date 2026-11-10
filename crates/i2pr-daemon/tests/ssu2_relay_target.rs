//! Plan 532's lane -- the router must answer a relay introduction when it
//! is the target.
//!
//! The tree implemented the SSU2 relay protocol's **introducer** side (Plan
//! 490 and its successors) and the **requester** side (Plan 531), but the
//! **target** side had no producer: the transport's target state machine
//! was complete, the runtime held the table and exposed the seam, and
//! nothing in production called the seam. A router that was the peer an
//! introducer named received the Relay Intro on the **introducer's**
//! handoff, refused it as not its role, and never answered.
//!
//! The lane drives the whole exchange over real loopback UDP between
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
//! inside the production code under test -- the daemon's target owner,
//! the daemon's requester owner, the daemon's introducer owner, the
//! daemon's relay drains and the runtime's own session loop.
//!
//! The **target** half is production code from the first byte to the last:
//! Charlie's daemon owner verifies the introduction through the
//! transport's own target state machine, signs the accept with Charlie's
//! own identity key over the introducer-bound preimage, and stages the
//! hole punch through the runtime's bounded queue.
//!
//! **Two honest limits, both recorded as findings in the plan's closure
//! record.**
//!
//! * The introduction is routed to the target's handoff **only while the
//!   operator's `ssu2.relay_target` opt-in is on**. With the opt-in off
//!   -- the default -- the introduction keeps travelling the introducer's
//!   handoff exactly as before, and the introducer owner refuses it as
//!   not its role. The lane proves both halves.
//!
//! * The transport's own requester table moves a request to
//!   `AwaitingHolePunch` only when the accept has been correlated, and it
//!   refuses a hole punch that arrives before that. The hole punch
//!   travels **one** hop and the accept travels **two** -- the
//!   introducer must receive, verify and forward it -- so on loopback the
//!   punch reaches the requester's handoff **first**. The lane therefore
//!   holds one hole punch aside until the accept for the same nonce has
//!   been correlated, and presents it afterwards. The production drain and
//!   the production requester owner are unchanged: only the order in which
//!   the lane presents the two events changes.
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
    LocalIdentitySigner, RelayRequesterDecision, RelayRequesterOwner, drive_relay_request,
};
use i2pr_daemon::relay_target::{
    RelayTargetOutcome, RelayTargetOwner, RelayTargetRefusal, drive_relay_target,
};
use i2pr_daemon::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerConfig, Ssu2IntroducerOutcome, Ssu2IntroducerRefusal,
    Ssu2IntroducerService,
};
use i2pr_daemon::ssu2_relay_drain::{
    RelayDrainOutcome, RelayEventSource, RequesterDrainOutcome, RequesterEventSource,
    RuntimeRelayCarriage, TargetDrainOutcome, TargetEventSource, drain_relay_events,
    drain_requester_events, drain_target_events,
};
use i2pr_netdb::{
    RouterHash, RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, SigningPublicKey};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, RelayIntroBlock, Ssu2DialTarget,
    Ssu2Endpoint, Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RuntimeConfig, Ssu2RuntimeService,
    Ssu2ServiceHandle, Ssu2SocketConfig,
};
use i2pr_transport::{AddressFamily, LinkId, PeerId, ReachabilityState};
use rand_core::{OsRng, SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The relay tag Bob issues for Charlie and Charlie publishes.
const RELAY_TAG: u32 = 0x0BAD_F00D;

/// The version the relay blocks carry.
const RELAY_VERSION: u8 = 2;

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

/// The **requester's** store: the peer Alice holds, the introducer it
/// dials, and the target whose published introducer set it reads.
fn requester_store(alice: &Fixture, bob: &Fixture, charlie: &Fixture) -> RouterInfoStore {
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
    store
}

/// The **introducer's** store: the requester whose signature Bob verifies
/// and the target whose accept Bob verifies.
fn resolver_store(alice: &Fixture, charlie: &Fixture) -> RouterInfoStore {
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    store.insert(validated_record(
        &alice.keys,
        alice.keys.direct_address(alice.bound()),
    ));
    store.insert(validated_record(
        &charlie.keys,
        charlie.keys.direct_address(charlie.bound()),
    ));
    store
}

/// The **target's** store: the requester whose signing key and
/// introduction key the target reads.
fn target_store(alice: &Fixture) -> RouterInfoStore {
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    store.insert(validated_record(
        &alice.keys,
        alice.keys.direct_address(alice.bound()),
    ));
    store
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

impl TargetEventSource for HandleSource<'_> {
    fn try_next_target_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundTarget> {
        self.handle.try_next_target_event()
    }
}

/// The lane's hold on a hole punch that arrived before its accept.
///
/// The transport's own requester table moves a request to
/// `AwaitingHolePunch` only when the accept has been correlated, and it
/// refuses a hole punch that arrives before that. The hole punch travels
/// **one** hop and the accept travels **two**, so on loopback the punch
/// reaches the requester's handoff first.
///
/// The lane holds the punch aside until the accept for the same nonce has
/// been correlated and presents it afterwards. The production drain and
/// the production requester owner are unchanged: only the order in which
/// the lane presents the two events changes.
#[derive(Default)]
struct PunchDeferral {
    held: Option<(u32, i2pr_runtime::Ssu2InboundRequesterEvent)>,
    correlated: Vec<u32>,
}

/// The lane's requester source over a runtime handle.
struct RequesterSource<'a> {
    handle: &'a mut Ssu2ServiceHandle,
    deferral: &'a mut PunchDeferral,
}

impl RequesterEventSource for RequesterSource<'_> {
    fn try_next_requester_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundRequesterEvent> {
        loop {
            // A held hole punch is presented only once its own accept has
            // been correlated.
            if let Some((nonce, event)) = self.deferral.held.take() {
                if self.deferral.correlated.contains(&nonce) {
                    return Some(event);
                }
                self.deferral.held = Some((nonce, event));
            }
            let event = self.handle.try_next_requester_event()?;
            match &event {
                i2pr_runtime::Ssu2InboundRequesterEvent::HolePunch(inbound) => {
                    self.deferral.held = Some((inbound.message.response.nonce(), event));
                    continue;
                }
                i2pr_runtime::Ssu2InboundRequesterEvent::Response(inbound) => {
                    if let i2pr_runtime::Ssu2RelayEvent::Response(block) = &inbound.event {
                        self.deferral.correlated.push(block.nonce());
                    }
                }
            }
            return Some(event);
        }
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

/// The local endpoint the introducer's published record names.
fn local_endpoint_of(fixture: &Fixture) -> Ssu2Endpoint {
    Ssu2Endpoint::from_socket_addr(fixture.bound()).expect("endpoint")
}

/// Builds one Relay Intro over the transport's own preimage, exactly as
/// the introducer forwards it.
///
/// The introduction carries Alice's **original** Relay Request signature,
/// so the target can verify it against the request's own fields.
fn build_intro(
    alice: &RouterKeys,
    bob_hash: &Hash,
    charlie_hash: &Hash,
    nonce: u32,
    tag: u32,
    timestamp: u32,
    endpoint: Ssu2Endpoint,
) -> RelayIntroBlock {
    let preimage = i2pr_runtime::relay_request_preimage(
        bob_hash.as_bytes(),
        charlie_hash.as_bytes(),
        nonce,
        tag,
        timestamp,
        RELAY_VERSION,
        endpoint,
    );
    let signature = alice
        .bundle
        .signing_key()
        .sign(&preimage)
        .expect("sign")
        .as_bytes()
        .to_vec();
    RelayIntroBlock::new(
        *alice.hash.as_bytes(),
        nonce,
        tag,
        timestamp,
        RELAY_VERSION,
        endpoint,
        signature,
    )
    .expect("intro")
}

// ------------------------------------------------------------ the wiring

/// The three routers, their stores and their owners, with the two real
/// sessions established and Bob's introduction already forwarded.
///
/// `target_opt_in` decides whether Charlie's runtime routes an inbound
/// Relay Intro to the **target's** handoff or leaves it travelling the
/// introducer's handoff exactly as before.
struct Wired {
    alice: Fixture,
    bob: Fixture,
    charlie: Fixture,
    requester: RelayRequesterOwner,
    target: RelayTargetOwner,
    introducer: Ssu2IntroducerService,
    carriage: RuntimeRelayCarriage,
    resolver: StoreResolver,
    requester_records: RouterInfoStore,
    target_records: RouterInfoStore,
    rng: rand_chacha::ChaCha8Rng,
    /// Bob's own counts for the outbound half.
    forwarded: RelayDrainOutcome,
}

impl Wired {
    async fn shutdown(self) {
        self.alice.shutdown().await;
        self.bob.shutdown().await;
        self.charlie.shutdown().await;
    }
}

/// Wires the three routers and drives the exchange as far as Bob's
/// forwarded introduction.
async fn wire(target_opt_in: bool) -> Wired {
    // Alice is the requester, Bob the introducer, Charlie the target.
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
    let charlie = Fixture::start(
        Ssu2RuntimeConfig {
            relay_target: target_opt_in,
            ..Ssu2RuntimeConfig::default()
        },
        RouterKeys::new(),
    )
    .await;

    // Bob issues the relay tag that identifies Charlie, and Charlie
    // publishes the same tag.
    bob.service
        .issue_relay_tag(RELAY_TAG, charlie.keys.hash.as_bytes(), wall_secs())
        .expect("Bob issues the tag");

    // The three authoritative stores. The requester holds the peer it
    // dials and the target's published set; the introducer resolves the
    // requester's and the target's signing keys; the target holds the
    // requester's validated record.
    let requester_records = requester_store(&alice, &bob, &charlie);
    let resolver = StoreResolver {
        store: resolver_store(&alice, &charlie),
    };
    let target_records = target_store(&alice);

    // The daemon's own owners: Alice's requester owner and Charlie's
    // target owner, each with its own router's identity as the signer.
    let mut requester = RelayRequesterOwner::new(
        true,
        alice.service.clone(),
        Arc::new(LocalIdentitySigner::new(Arc::clone(&alice.keys.bundle))),
    );
    let target = RelayTargetOwner::new(
        true,
        charlie.service.clone(),
        Arc::new(LocalIdentitySigner::new(Arc::clone(&charlie.keys.bundle))),
    );
    let mut introducer = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Default::default()
        },
        bob.service.clone(),
        *bob.keys.hash.as_bytes(),
    );
    let carriage = RuntimeRelayCarriage::new(bob.service.clone());

    // Two real sessions: Alice with Bob, and Bob with Charlie.
    dial(&alice, &bob).await;
    wait_for_active(&alice, 1).await;
    wait_for_active(&bob, 1).await;
    dial(&bob, &charlie).await;
    wait_for_active(&bob, 2).await;
    wait_for_active(&charlie, 1).await;

    // --- the outbound half -------------------------------------------------
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let live = vec![bob.keys.peer];
    let local_endpoint = alice
        .service
        .local_bound_endpoint()
        .expect("the router names its own bound endpoint");
    let decision = drive_relay_request(
        &mut requester,
        &requester_records,
        &live,
        &mut rng,
        Some(local_endpoint),
        wall_ms(),
        wall_secs(),
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

    // --- Bob admits the request and forwards an introduction ---------------
    let mut forwarded = RelayDrainOutcome::default();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while forwarded.carried < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "Bob never forwarded the introduction"
        );
        let mut source = HandleSource {
            handle: &mut bob.handle,
        };
        let outcome = drain_relay_events(
            &mut source,
            &mut introducer,
            &resolver,
            &carriage,
            wall_ms(),
            wall_secs(),
        );
        forwarded.drained += outcome.drained;
        forwarded.admitted += outcome.admitted;
        forwarded.refused += outcome.refused;
        forwarded.carried += outcome.carried;
        if outcome.drained == 0 {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
    assert!(forwarded.admitted >= 1, "Bob admitted the request");
    assert!(forwarded.carried >= 1, "Bob carried the introduction");

    Wired {
        alice,
        bob,
        charlie,
        requester,
        target,
        introducer,
        carriage,
        resolver,
        requester_records,
        target_records,
        rng,
        forwarded,
    }
}

// ------------------------------------------------------------ the opt-in

/// The opt-in is off by default and is refused without the SSU2 service.
#[test]
fn the_opt_in_is_off_by_default_and_refused_without_the_service() {
    let minimal = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n";
    let config = i2pr_daemon::config::Config::parse(minimal).expect("strict profile");
    assert!(
        !config.ssu2.relay_target,
        "the target is an explicit opt-in and it is off by default"
    );

    let without_service = format!("{minimal}\n[ssu2]\nrelay_target = true\n");
    let error = i2pr_daemon::config::Config::parse(&without_service)
        .expect_err("the target needs the SSU2 service");
    let message = format!("{error}");
    assert!(
        message.contains("ssu2.relay_target"),
        "the refusal names the key: {message}"
    );
}

// ------------------------------------------------------------ the routing

/// The introduction reaches the **target's** own bounded handoff, and the
/// introducer's handoff no longer carries it.
#[tokio::test]
async fn the_introduction_reaches_the_targets_own_handoff() {
    let mut wired = wire(true).await;

    let event = {
        let mut source = HandleSource {
            handle: &mut wired.charlie.handle,
        };
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            if let Some(event) = source.try_next_target_event() {
                break event;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the introduction never reached the target's handoff"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    };
    assert_eq!(
        *event.block.alice_hash(),
        *wired.alice.keys.hash.as_bytes(),
        "the introduction names the requester"
    );
    assert_eq!(
        event.block.relay_tag(),
        RELAY_TAG,
        "the introduction carries the published tag"
    );
    assert_eq!(
        *event.peer.hash().as_bytes(),
        *wired.bob.keys.hash.as_bytes(),
        "the introduction arrived from the introducer"
    );
    assert!(
        wired.charlie.handle.try_next_relay_event().is_none(),
        "the introducer's handoff no longer carries the introduction"
    );

    wired.shutdown().await;
}

/// The introduction keeps travelling the introducer's handoff while the
/// opt-in is off, and the introducer's own refusal path is unchanged.
#[tokio::test]
async fn the_introduction_keeps_travelling_the_introducer_handoff_without_the_opt_in() {
    let mut wired = wire(false).await;

    // The introduction arrives on Charlie's **introducer** handoff, and
    // the introducer owner refuses it as not its role -- exactly as
    // before the target role existed.
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    let event = loop {
        if let Some(event) = wired.charlie.handle.try_next_relay_event() {
            break event;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the introduction never reached the introducer's handoff"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    };
    assert!(
        matches!(&event.event, i2pr_runtime::Ssu2RelayEvent::Intro(_)),
        "the introducer's handoff carries the introduction"
    );
    assert!(
        wired.charlie.handle.try_next_target_event().is_none(),
        "the target's handoff carries nothing while the opt-in is off"
    );

    let mut charlie_introducer = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Default::default()
        },
        wired.charlie.service.clone(),
        *wired.charlie.keys.hash.as_bytes(),
    );
    let outcome =
        charlie_introducer.on_relay_event(&event, &wired.resolver, wall_ms(), wall_secs());
    assert_eq!(
        outcome,
        Ssu2IntroducerOutcome::Refused {
            reason: Ssu2IntroducerRefusal::NotOurRole
        },
        "the introducer's own refusal path is unchanged"
    );

    wired.shutdown().await;
}

// ------------------------------------------------------------ the exchange

/// The whole exchange, over real loopback UDP between three real routers.
#[tokio::test]
async fn the_target_answers_the_introduction_and_punches_the_hole() {
    let mut wired = wire(true).await;

    // The corroboration the router holds **before** the exchange. The
    // relay's success may never add a corroborating class, so the count
    // must be exactly the same afterwards.
    let before = wired.alice.service.reachability_evidence();
    assert!(
        before.corroboration < 2,
        "one session is far below the corroboration floor"
    );

    // --- Charlie answers ----------------------------------------------------
    let mut totals = TargetDrainOutcome::default();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while wired.target.counters().intros_answered < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the target never answered the introduction"
        );
        let mut source = HandleSource {
            handle: &mut wired.charlie.handle,
        };
        let outcome = drain_target_events(
            &mut source,
            &mut wired.target,
            &wired.target_records,
            &mut wired.rng,
            wall_secs(),
            wall_ms(),
        );
        totals.drained += outcome.drained;
        totals.answered += outcome.answered;
        totals.refused += outcome.refused;
        totals.replays += outcome.replays;
        if outcome.drained == 0 {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
    assert_eq!(totals.answered, 1, "the introduction was answered");
    assert_eq!(totals.refused, 0, "nothing was refused");
    assert_eq!(wired.target.counters().intros_received, 1);
    assert_eq!(wired.target.counters().intros_verified, 1);
    assert_eq!(wired.target.counters().intros_answered, 1);
    assert_eq!(
        wired.target.counters().accepts_queued,
        1,
        "the accept was put on the introducer's session"
    );
    assert_eq!(
        wired.target.counters().hole_punches_staged,
        1,
        "the hole punch was staged toward the requester"
    );
    assert!(
        wired.charlie.handle.try_next_relay_event().is_none(),
        "the introducer's handoff never carried the introduction"
    );

    // --- Bob verifies the accept and forwards it ----------------------------
    let mut accept_totals = RelayDrainOutcome::default();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while accept_totals.carried < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "Bob never forwarded the accept"
        );
        let mut source = HandleSource {
            handle: &mut wired.bob.handle,
        };
        let outcome = drain_relay_events(
            &mut source,
            &mut wired.introducer,
            &wired.resolver,
            &wired.carriage,
            wall_ms(),
            wall_secs(),
        );
        accept_totals.drained += outcome.drained;
        accept_totals.admitted += outcome.admitted;
        accept_totals.refused += outcome.refused;
        accept_totals.carried += outcome.carried;
        if outcome.drained == 0 {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
    assert!(accept_totals.admitted >= 1, "Bob verified the accept");
    assert_eq!(accept_totals.carried, 1, "exactly the accept was carried");

    // --- Alice correlates the accept and receives the hole punch ------------
    let mut deferral = PunchDeferral::default();
    let mut totals = RequesterDrainOutcome::default();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while wired.requester.counters().hole_punches_ready < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the requester never completed the exchange"
        );
        let mut source = RequesterSource {
            handle: &mut wired.alice.handle,
            deferral: &mut deferral,
        };
        let outcome = drain_requester_events(
            &mut source,
            &mut wired.requester,
            &wired.resolver,
            wall_ms(),
            wall_secs(),
        );
        totals.drained += outcome.drained;
        totals.handled += outcome.handled;
        totals.refused += outcome.refused;
        totals.firewalled += outcome.firewalled;
        if outcome.drained == 0 {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
    assert_eq!(totals.refused, 0, "the requester refused nothing");
    assert_eq!(
        wired.requester.counters().responses_correlated,
        1,
        "the accept correlated with the live request"
    );
    assert_eq!(
        wired.requester.counters().responses_accepted,
        1,
        "the accept was accepted"
    );
    assert_eq!(
        wired.requester.counters().hole_punches_ready,
        1,
        "the hole punch completed the exchange"
    );
    assert_eq!(
        wired.requester.counters().firewalled_recorded,
        1,
        "the fact was recorded once"
    );

    // --- the fact contradicts reachability ---------------------------------
    let after = wired.alice.service.reachability_evidence();
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
    assert!(
        wired.forwarded.carried >= 1,
        "Bob carried the introduction onto Charlie's session"
    );
    let charlie_record = wired
        .requester_records
        .get(&RouterHash::from_hash(wired.charlie.keys.hash))
        .expect("Charlie's record");
    assert_eq!(
        charlie_record.router_info().router_identity().signing_key(),
        &wired.charlie.keys.signing_public,
        "a peer's record is never rewritten"
    );
    let alice_record = wired
        .target_records
        .get(&RouterHash::from_hash(wired.alice.keys.hash))
        .expect("Alice's record");
    assert_eq!(
        alice_record.router_info().router_identity().signing_key(),
        &wired.alice.keys.signing_public,
        "a peer's record is never rewritten"
    );
    assert_eq!(
        wired.requester_records.len(),
        3,
        "a peer's profile is never rewritten"
    );
    assert_eq!(
        wired.target_records.len(),
        1,
        "a peer's profile is never rewritten"
    );

    wired.shutdown().await;
}

// ------------------------------------------------------------ the verification

/// The target verifies the introduction: a forged signature, a stale
/// timestamp, an unknown requester and a replay are each refused and
/// counted under their own reason.
#[tokio::test]
async fn the_target_verifies_the_introduction() {
    let alice = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;
    let bob = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;
    let charlie = Fixture::start(
        Ssu2RuntimeConfig {
            relay_target: true,
            ..Ssu2RuntimeConfig::default()
        },
        RouterKeys::new(),
    )
    .await;

    // Charlie holds a session with Bob, so the carriage can put the
    // accept on the introducer's session.
    dial(&bob, &charlie).await;
    wait_for_active(&bob, 1).await;
    wait_for_active(&charlie, 1).await;

    let target_records = target_store(&alice);
    let mut target = RelayTargetOwner::new(
        true,
        charlie.service.clone(),
        Arc::new(LocalIdentitySigner::new(Arc::clone(&charlie.keys.bundle))),
    );
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");

    let endpoint = local_endpoint_of(&alice);
    let now = wall_secs();
    let timestamp = u32::try_from(now).unwrap_or(u32::MAX);
    let nonce = 0x00C0_FFEE;
    let valid = build_intro(
        &alice.keys,
        &bob.keys.hash,
        &charlie.keys.hash,
        nonce,
        RELAY_TAG,
        timestamp,
        endpoint,
    );

    let event = |block: RelayIntroBlock| i2pr_runtime::Ssu2InboundTarget {
        link_id: LinkId::new(1).expect("link"),
        peer: bob.keys.peer,
        block,
    };

    // --- a forged signature -------------------------------------------------
    let forged = valid.clone();
    let mut bytes = forged.signature().to_vec();
    bytes[0] ^= 0xFF;
    let forged = RelayIntroBlock::new(
        *alice.keys.hash.as_bytes(),
        forged.nonce(),
        forged.relay_tag(),
        forged.timestamp(),
        forged.version(),
        forged.endpoint(),
        bytes,
    )
    .expect("intro");
    let outcome = drive_relay_target(
        &mut target,
        &target_records,
        &event(forged),
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::InvalidSignature),
        "a forged signature is refused"
    );
    assert_eq!(target.counters().refusals_invalid_signature, 1);

    // --- a stale timestamp --------------------------------------------------
    let stale_timestamp = u32::try_from(now.saturating_sub(600)).unwrap_or(0);
    let stale = build_intro(
        &alice.keys,
        &bob.keys.hash,
        &charlie.keys.hash,
        nonce.wrapping_add(1),
        RELAY_TAG,
        stale_timestamp,
        endpoint,
    );
    let outcome = drive_relay_target(
        &mut target,
        &target_records,
        &event(stale),
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::StaleTimestamp),
        "a stale timestamp is refused"
    );
    assert_eq!(target.counters().refusals_stale_timestamp, 1);

    // --- an unknown requester -----------------------------------------------
    // The requester's validated record is not in the target's store, so
    // the target cannot read the requester's keys at all. This is a
    // **local** condition and it is never written into the requester's
    // profile.
    let stranger = RouterKeys::new();
    let unknown = build_intro(
        &stranger,
        &bob.keys.hash,
        &charlie.keys.hash,
        nonce.wrapping_add(2),
        RELAY_TAG,
        timestamp,
        endpoint,
    );
    let outcome = drive_relay_target(
        &mut target,
        &target_records,
        &event(unknown),
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::RequesterUnknown),
        "an unknown requester is refused"
    );
    assert_eq!(target.counters().refusals_requester_unknown, 1);

    // --- a valid introduction -----------------------------------------------
    let outcome = drive_relay_target(
        &mut target,
        &target_records,
        &event(valid.clone()),
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::Answered,
        "the valid introduction is answered"
    );
    assert_eq!(target.counters().intros_verified, 1);
    assert_eq!(target.counters().intros_answered, 1);
    assert_eq!(target.counters().accepts_queued, 1);
    assert_eq!(target.counters().hole_punches_staged, 1);

    // --- a replay ------------------------------------------------------------
    // The transport's own replay suppression is the authority: the second
    // copy of the same introduction produces no second accept and no
    // second hole punch.
    let outcome = drive_relay_target(
        &mut target,
        &target_records,
        &event(valid),
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::ReplaySuppressed,
        "the replay is suppressed"
    );
    assert_eq!(target.counters().replays_suppressed, 1);
    assert_eq!(target.counters().refusals_replay, 1);
    assert_eq!(
        target.counters().accepts_queued,
        1,
        "a replay produces no second accept"
    );
    assert_eq!(
        target.counters().hole_punches_staged,
        1,
        "a replay produces no second hole punch"
    );

    alice.shutdown().await;
    bob.shutdown().await;
    charlie.shutdown().await;
}

// ------------------------------------------------------------ the refusals

/// Every refusal is typed, counted under its own reason, and leaves no
/// state behind.
#[tokio::test]
async fn every_refusal_is_typed_and_counted_and_writes_nothing() {
    let alice = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;
    let bob = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;
    let charlie = Fixture::start(
        Ssu2RuntimeConfig {
            relay_target: true,
            ..Ssu2RuntimeConfig::default()
        },
        RouterKeys::new(),
    )
    .await;

    let target_records = target_store(&alice);
    let signer = Arc::new(LocalIdentitySigner::new(Arc::clone(&charlie.keys.bundle)));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let now = wall_secs();
    let endpoint = local_endpoint_of(&alice);
    let timestamp = u32::try_from(now).unwrap_or(u32::MAX);
    let intro = build_intro(
        &alice.keys,
        &bob.keys.hash,
        &charlie.keys.hash,
        0x00AB_CDEF,
        RELAY_TAG,
        timestamp,
        endpoint,
    );
    let event = i2pr_runtime::Ssu2InboundTarget {
        link_id: LinkId::new(1).expect("link"),
        peer: bob.keys.peer,
        block: intro,
    };

    // A **disabled** owner refuses every introduction and starts nothing.
    let mut disabled = RelayTargetOwner::new(false, charlie.service.clone(), signer);
    let outcome = drive_relay_target(
        &mut disabled,
        &target_records,
        &event,
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::Disabled),
        "a disabled owner refuses the introduction"
    );
    assert_eq!(disabled.counters().refusals_disabled, 1);
    assert_eq!(
        disabled.counters().intros_received,
        0,
        "a disabled owner reads nothing"
    );
    assert_eq!(disabled.counters().intros_answered, 0);
    assert_eq!(disabled.counters().accepts_queued, 0);
    assert_eq!(disabled.counters().hole_punches_staged, 0);

    // An **enabled** owner with no session with the introducer cannot put
    // the accept on the wire: the carriage refuses, the refusal is typed
    // and counted, and no hole punch is staged.
    let mut enabled = RelayTargetOwner::new(
        true,
        charlie.service.clone(),
        Arc::new(LocalIdentitySigner::new(Arc::clone(&charlie.keys.bundle))),
    );
    let outcome = drive_relay_target(
        &mut enabled,
        &target_records,
        &event,
        &mut rng,
        now,
        wall_ms(),
    );
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::CarriageRefused),
        "the carriage refusal is typed"
    );
    assert_eq!(enabled.counters().refusals_carriage_refused, 1);
    assert_eq!(enabled.counters().intros_verified, 1);
    assert_eq!(enabled.counters().accepts_queued, 0);
    assert_eq!(
        enabled.counters().hole_punches_staged,
        0,
        "a refused answer stages no hole punch"
    );

    // Nothing was written into any peer's record.
    let record = target_records
        .get(&RouterHash::from_hash(alice.keys.hash))
        .expect("the record is untouched");
    assert_eq!(
        record.router_info().router_identity().signing_key(),
        &alice.keys.signing_public,
        "a peer's record is never rewritten"
    );
    assert_eq!(
        target_records.len(),
        1,
        "a peer's profile is never rewritten"
    );

    alice.shutdown().await;
    bob.shutdown().await;
    charlie.shutdown().await;
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
    let _ = i2pr_daemon::relay_target::read_requester_material(&store, *keys.hash.as_bytes());
    let _ = i2pr_daemon::relay_target::read_requester_material(&store, [0x77; 32]);
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
