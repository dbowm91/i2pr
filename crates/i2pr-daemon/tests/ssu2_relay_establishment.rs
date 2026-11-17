//! Plan 533's lane -- the router must establish a session with the peer it
//! reached through the relay.
//!
//! Plan 531 gave the SSU2 relay protocol's **requester** side and Plan 532
//! gave its **target** side, and Plan 532's lane drove all three routers
//! over real loopback UDP. But the exchange ended at a hole punch: the
//! requester never opened a session with the peer it had been relayed to,
//! and the accept's token had **no consumer** -- the target drew it from
//! the daemon's own randomness while the only possible consumer is the
//! target's responder, whose authority is the runtime's token table.
//!
//! This lane drives the whole exchange and the establishment that follows
//! it over real loopback UDP between **three** real routers:
//!
//! ```text
//! Alice ──Relay Request──▶ Bob ──Relay Intro──▶ Charlie
//! Alice ◀─Relay Response── Bob ◀─Relay Response── Charlie
//! Alice ◀────Hole Punch──── Charlie
//! Alice ──────SSU2 Session──▶ Charlie
//! ```
//!
//! Every datagram crosses a real socket, every session is a real
//! authenticated session, and every decision happens inside the production
//! code under test -- the daemon's requester owner, the daemon's target
//! owner, the daemon's introducer owner, the daemon's relay drains, the
//! daemon's relay-establishment owner and task, and the runtime's own
//! session loop.
//!
//! # What the lane proves
//!
//! * The accept carries a token the target's **own responder** issued, and
//!   the responder consumes it: the session is established **on the token**,
//!   which is only possible when the responder's table held the presented
//!   value.
//! * A hole punch that arrives **before** its accept is **held** by the
//!   requester owner and presented again once the accept is correlated.
//!   The transport requires the accept first, and the punch travels one
//!   hop while the accept travels two, so this is the ordinary case.
//! * The requester reaches the peer it was relayed to: the session is
//!   authenticated and both routers hold it.
//! * The token is **one-use** and **source-bound**: the responder's own
//!   table refuses a second presentation and a presentation from another
//!   address, and the refusal is counted by the runtime.
//! * Every refusal is typed and counted, and no refusal writes anything
//!   into a peer's record.
//!
//! The lane asserts counts, states and forms only: no key, tag, peer hash,
//! signature, token or payload is ever printed.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::{ReseedConfig, Ssu2QualificationProfile};
use i2pr_daemon::relay_establishment::{
    RELAY_ESTABLISHMENT_QUEUE, RelayEstablishmentCounters, RelayEstablishmentOwner,
    prepare_relay_establishment,
};
use i2pr_daemon::relay_establishment_task::{RelayEstablishmentTask, run_relay_establishment_task};
use i2pr_daemon::relay_requester::{
    LocalIdentitySigner, RelayRequesterDecision, RelayRequesterOwner, drive_relay_request,
};
use i2pr_daemon::relay_target::RelayTargetOwner;
use i2pr_daemon::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerConfig, Ssu2IntroducerService,
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
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2DialTarget, Ssu2Endpoint,
    Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2ServiceHandle,
    Ssu2SocketConfig,
};
use i2pr_transport::{PeerId, ReachabilityState};
use rand_core::{OsRng, SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The bounded window a refused token presentation is given.
///
/// It is deliberately shorter than the runtime's cached-token grace
/// (1.5 s): the lane asserts that the responder **refused** the token,
/// not that the dial eventually fell back to the tokenless path.
const REFUSED_TOKEN_TIMEOUT: Duration = Duration::from_millis(400);

/// The relay tag Bob issues for Charlie and Charlie publishes.
const RELAY_TAG: u32 = 0x0BAD_F00D;

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
        let router_info = keys
            .bundle
            .sign_router_info(
                Date::from_millis(wall_ms()),
                vec![keys.direct_address("127.0.0.1:43000".parse().expect("addr"))],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        let router_info = router_info
            .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode");
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

/// The lane's requester source over a runtime handle.
///
/// The source presents the **hole punch first** and holds the accept back
/// until the punch has been presented. The transport's requester table
/// requires the accept before a hole punch, so this is the ordering the
/// production owner must survive -- and it is the ordering the wire
/// produces on loopback, where the punch travels one hop and the accept
/// travels two.
///
/// The hold is the lane's control of the **order of presentation**, not a
/// workaround: the deferral itself, the re-drive and the establishment are
/// production code.
struct RequesterSource<'a> {
    handle: &'a mut Ssu2ServiceHandle,
    held_response: Option<i2pr_runtime::Ssu2InboundRequesterEvent>,
    punch_presented: bool,
}

impl RequesterEventSource for RequesterSource<'_> {
    fn try_next_requester_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundRequesterEvent> {
        if self.punch_presented
            && let Some(response) = self.held_response.take()
        {
            return Some(response);
        }
        let event = self.handle.try_next_requester_event()?;
        match &event {
            i2pr_runtime::Ssu2InboundRequesterEvent::HolePunch(_) => {
                self.punch_presented = true;
                return Some(event);
            }
            i2pr_runtime::Ssu2InboundRequesterEvent::Response(_) => {
                if !self.punch_presented {
                    self.held_response = Some(event);
                    return self.handle.try_next_requester_event();
                }
            }
        }
        Some(event)
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

/// The three routers, their stores and their owners, with the two real
/// sessions established and Bob's introduction already forwarded.
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
    /// The establishment's handoff and its counter cell.
    handoff: tokio::sync::mpsc::Sender<i2pr_daemon::relay_requester::RelayEstablishment>,
    counters: Arc<StdMutex<RelayEstablishmentCounters>>,
    /// The task's cancellation.
    task_token: CancellationToken,
    /// The task's own scope.
    task_scope: ChildScope,
}

impl Wired {
    async fn shutdown(self) {
        self.task_token
            .cancel(i2pr_runtime::CancellationReason::TestHarnessTeardown);
        self.task_scope.shutdown().await;
        self.alice.shutdown().await;
        self.bob.shutdown().await;
        self.charlie.shutdown().await;
    }

    /// Reads the establishment's counters.
    fn counters(&self) -> RelayEstablishmentCounters {
        *self.counters.lock().expect("counter cell")
    }

    /// Hands the published establishment to the task, exactly as the
    /// daemon's relay tick does.
    ///
    /// The material is returned to the caller so a row can examine it.
    fn publish_establishment(
        &mut self,
    ) -> Option<i2pr_daemon::relay_requester::RelayEstablishment> {
        let (nonce, establishment) = self.requester.take_establishment()?;
        if let Err(error) = self.handoff.try_send(establishment) {
            let returned = match error {
                tokio::sync::mpsc::error::TrySendError::Full(item) => item,
                tokio::sync::mpsc::error::TrySendError::Closed(item) => item,
            };
            self.requester
                .return_establishment(nonce, returned, wall_ms());
            return None;
        }
        Some(establishment)
    }
}

/// Wires the three routers and drives the exchange as far as Bob's
/// forwarded introduction.
async fn wire() -> Wired {
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
            relay_target: true,
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
    let alice_record = validated_record(&alice.keys, alice.keys.direct_address(alice.bound()));
    let bob_record = validated_record(&bob.keys, bob.keys.direct_address(bob.bound()));
    let charlie_direct =
        validated_record(&charlie.keys, charlie.keys.direct_address(charlie.bound()));
    let charlie_published = validated_record(
        &charlie.keys,
        charlie.keys.introducer_address(&bob.keys.hash, RELAY_TAG),
    );
    let mut requester_records = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    requester_records.insert(alice_record.clone());
    requester_records.insert(bob_record.clone());
    requester_records.insert(charlie_published.clone());
    let mut resolver_store = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    resolver_store.insert(alice_record.clone());
    resolver_store.insert(charlie_direct.clone());
    let mut target_records = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
    target_records.insert(alice_record.clone());
    let resolver = StoreResolver {
        store: resolver_store,
    };

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

    // --- the establishment's own task --------------------------------------
    //
    // The bootstrap is the router's authoritative store, exactly as the
    // daemon's composition holds it. The task reads the target's
    // validated material from it and dials.
    let bootstrap = Arc::new(StdMutex::new(i2pr_daemon::bootstrap::Bootstrap::new(
        RouterInfoStoreConfig::default(),
        ReseedConfig {
            enabled: false,
            max_sources: 0,
            max_su3_bytes: 0,
            sources: Vec::new(),
        },
    )));
    {
        let mut guard = bootstrap.lock().expect("bootstrap");
        let store = guard.store_mut();
        store.insert(alice_record.clone());
        store.insert(bob_record.clone());
        store.insert(charlie_published.clone());
    }
    let (handoff, receiver) = tokio::sync::mpsc::channel(RELAY_ESTABLISHMENT_QUEUE);
    let counters = Arc::new(StdMutex::new(RelayEstablishmentCounters::default()));
    let task_token = CancellationToken::new();
    let task_scope = ChildScope::for_test(&task_token, ChildFailurePolicy::FailParent);
    let task = RelayEstablishmentTask {
        owner: RelayEstablishmentOwner::new(true),
        service: alice.service.clone(),
        bootstrap: Some(Arc::clone(&bootstrap)),
        qualification_profile: Ssu2QualificationProfile::ControlledLoopback,
        dial_timeout: DIAL_TIMEOUT,
        receiver,
        counters_cell: Arc::clone(&counters),
        // Plan 534: this lane drives the establishment alone. The local
        // observation the establishment records is Plan 534's own subject.
        relay_sessions: None,
    };
    task_scope
        .spawn(move |cancellation| async move {
            run_relay_establishment_task(task, cancellation).await
        })
        .expect("the establishment task is spawned under its own scope");

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
        handoff,
        counters,
        task_token,
        task_scope,
    }
}

/// Drives the exchange to the point where the requester holds a ready
/// establishment.
async fn complete_exchange(wired: &mut Wired) -> RequesterDrainOutcome {
    // --- Charlie answers the introduction ----------------------------------
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
    assert_eq!(
        wired.target.counters().tokens_issued,
        1,
        "the accept carried one issued token"
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
    let mut source = RequesterSource {
        handle: &mut wired.alice.handle,
        held_response: None,
        punch_presented: false,
    };
    let mut totals = RequesterDrainOutcome::default();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while wired.requester.counters().hole_punches_ready < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the requester never completed the exchange"
        );
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
        totals.deferred += outcome.deferred;
        totals.firewalled += outcome.firewalled;
        if outcome.drained == 0 {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
    totals
}

/// Waits until the requester holds a session with the target.
async fn wait_for_peer_session(fixture: &Fixture, peer: PeerId) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture
            .service
            .active_peer_hashes()
            .iter()
            .any(|hash| *hash == peer.hash())
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the router never held a session with the peer it reached"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ------------------------------------------------------------ the opt-in

/// The requester role stays an explicit opt-in, and the establishment
/// adds **no** configuration key of its own.
#[test]
fn the_establishment_rides_the_requester_role_and_adds_no_key() {
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

// ------------------------------------------------------------ the exchange

/// The exchange completes, the early hole punch is held and re-driven, and
/// the establishment opens an authenticated session with the target.
#[tokio::test]
async fn the_requester_establishes_a_session_with_the_peer_it_reached() {
    let mut wired = wire().await;

    // The corroboration the router holds **before** the exchange. The
    // relay's success may never add a corroborating class, so the count
    // must be exactly the same afterwards.
    let before = wired.alice.service.reachability_evidence();
    assert!(
        before.corroboration < 2,
        "one session is far below the corroboration floor"
    );

    let totals = complete_exchange(&mut wired).await;

    // The hole punch arrived before its accept and was **held**, not
    // refused: the transport requires the accept first.
    assert!(
        wired.requester.counters().punches_deferred >= 1,
        "the early hole punch was held"
    );
    assert!(totals.deferred >= 1, "the drain counted the deferral");
    assert_eq!(totals.refused, 0, "the requester refused nothing");
    assert_eq!(
        wired.requester.counters().hole_punches_ready,
        1,
        "the held punch was re-driven and the exchange completed"
    );
    assert_eq!(
        wired.requester.counters().firewalled_recorded,
        1,
        "the relay-firewalled fact was recorded once"
    );

    // The establishment is published and the task performs it.
    assert!(
        wired.publish_establishment().is_some(),
        "the completed exchange published an establishment"
    );
    wait_for_peer_session(&wired.alice, wired.charlie.keys.peer).await;
    wait_for_peer_session(&wired.charlie, wired.alice.keys.peer).await;

    let counters = wired.counters();
    assert_eq!(
        counters.establishments_received, 1,
        "the task took one establishment"
    );
    assert_eq!(
        counters.establishments_established, 1,
        "the establishment reached an authenticated session"
    );
    assert_eq!(
        counters.establishments_with_token, 1,
        "the session was established on the accept's token"
    );
    assert_eq!(counters.refusals_handshake_failed, 0);

    // The responder consumed the token: the session was established **on**
    // the token, and the responder refused no token at all.
    assert_eq!(
        wired.charlie.service.snapshot().token_rejections,
        0,
        "the responder refused no token"
    );

    // The relay's success is a contradiction, not a corroboration.
    let after = wired.alice.service.reachability_evidence();
    assert_eq!(
        after.corroboration, before.corroboration,
        "a relayed peer adds no corroborating class"
    );
    assert_eq!(
        after.state,
        ReachabilityState::Firewalled,
        "the relay's success is a contradiction, not a reachability"
    );

    wired.shutdown().await;
}

/// The accept carries a token the target's **own responder** issued, and
/// the responder consumes it.
#[tokio::test]
async fn the_accept_carries_a_token_the_targets_own_responder_issued() {
    let mut wired = wire().await;
    complete_exchange(&mut wired).await;

    // The target's owner issued the token through the runtime's
    // responder table, and the runtime's table is the only authority on a
    // presented token.
    assert_eq!(
        wired.target.counters().tokens_issued,
        1,
        "one token was issued for the establishment"
    );
    assert_eq!(
        wired.target.counters().refusals_establishment_refused,
        0,
        "the responder's table refused nothing"
    );
    assert_eq!(
        wired.charlie.service.snapshot().token_rejections,
        0,
        "the responder refused no token"
    );

    // The establishment's own material: the accept's endpoint and the
    // accept's token.
    let (nonce, establishment) = wired
        .requester
        .take_establishment()
        .expect("the exchange published an establishment");
    assert_ne!(nonce, 0, "the correlation is nonzero");
    assert_eq!(
        establishment.target,
        *wired.charlie.keys.hash.as_bytes(),
        "the establishment names the peer the router was relayed to"
    );
    assert_ne!(establishment.token, 0, "the token is nonzero");
    assert_eq!(
        establishment.endpoint,
        local_endpoint_of(&wired.charlie),
        "the endpoint is the target's own bound endpoint"
    );

    // The dial target is built from the **validated** store and obeys the
    // qualification profile.
    let target = prepare_relay_establishment(
        &wired.requester_records,
        &establishment,
        Ssu2QualificationProfile::ControlledLoopback,
    )
    .expect("the dial target is prepared");
    assert_eq!(target.expected_router_hash(), wired.charlie.keys.hash);
    assert_eq!(target.address(), wired.charlie.bound());

    wired.shutdown().await;
}

/// The token is **one-use**: the responder refuses a second presentation.
#[tokio::test]
async fn the_token_is_one_use_and_the_responder_refuses_the_second() {
    let mut wired = wire().await;
    complete_exchange(&mut wired).await;

    let establishment = wired
        .publish_establishment()
        .expect("the exchange published an establishment");
    wait_for_peer_session(&wired.alice, wired.charlie.keys.peer).await;

    let target = prepare_relay_establishment(
        &wired.requester_records,
        &establishment,
        Ssu2QualificationProfile::ControlledLoopback,
    )
    .expect("the dial target is prepared");

    // The first presentation consumed the token. A second presentation of
    // the **same** token must be refused by the responder's own table.
    let before = wired.charlie.service.snapshot().token_rejections;
    let refused = wired
        .alice
        .service
        .dial_ssu2_with_token(
            target,
            establishment.token,
            REFUSED_TOKEN_TIMEOUT,
            &CancellationToken::new(),
        )
        .await;
    assert!(
        refused.is_err(),
        "the second presentation established no session"
    );
    let after = wired.charlie.service.snapshot().token_rejections;
    assert!(
        after > before,
        "the responder refused the reused token and counted it"
    );

    wired.shutdown().await;
}

/// The token is **source-bound**: the responder refuses it from another
/// address.
#[tokio::test]
async fn a_token_presented_from_another_address_is_refused() {
    let mut wired = wire().await;
    complete_exchange(&mut wired).await;

    let establishment = wired
        .publish_establishment()
        .expect("the exchange published an establishment");
    wait_for_peer_session(&wired.alice, wired.charlie.keys.peer).await;

    // A token issued for **Bob's** address, presented by Alice. The
    // responder's table binds a token to the exact source address it was
    // issued to, so Alice's presentation is refused.
    let foreign = wired
        .charlie
        .service
        .issue_relay_establishment_token(wired.bob.bound(), wall_secs())
        .expect("the responder issues a token for another address");
    assert_ne!(foreign, establishment.token);

    let target = prepare_relay_establishment(
        &wired.requester_records,
        &establishment,
        Ssu2QualificationProfile::ControlledLoopback,
    )
    .expect("the dial target is prepared");
    let before = wired.charlie.service.snapshot().token_rejections;
    let refused = wired
        .alice
        .service
        .dial_ssu2_with_token(
            target,
            foreign,
            REFUSED_TOKEN_TIMEOUT,
            &CancellationToken::new(),
        )
        .await;
    assert!(
        refused.is_err(),
        "the presentation from another address established no session"
    );
    let after = wired.charlie.service.snapshot().token_rejections;
    assert!(
        after > before,
        "the responder refused the foreign token and counted it"
    );

    wired.shutdown().await;
}

// ------------------------------------------------------------ the refusals

/// Every refusal is typed and counted, and none of them writes into a
/// peer's record.
#[tokio::test]
async fn every_refusal_is_typed_and_counted_and_writes_nothing() {
    let wired = wire().await;

    // A target the router does not hold is refused under its own reason.
    let unknown = i2pr_daemon::relay_requester::RelayEstablishment {
        target: [0x5A; 32],
        endpoint: local_endpoint_of(&wired.charlie),
        token: 0x0102_0304_0506_0708,
    };
    let refused = prepare_relay_establishment(
        &wired.requester_records,
        &unknown,
        Ssu2QualificationProfile::ControlledLoopback,
    );
    assert!(refused.is_err(), "an unknown target is refused");
    let reason = refused.expect_err("the refusal is typed");
    assert_eq!(reason.reason(), "target-unknown");

    // A disabled owner refuses every establishment.
    let mut disabled = RelayEstablishmentOwner::new(false);
    disabled.refuse(i2pr_daemon::relay_establishment::RelayEstablishmentRefusal::Disabled);
    assert_eq!(disabled.counters().refusals_disabled, 1);
    assert_eq!(disabled.counters().establishments_established, 0);

    // The reads never mutate the store.
    let before = wired.requester_records.len();
    let _ = prepare_relay_establishment(
        &wired.requester_records,
        &unknown,
        Ssu2QualificationProfile::ControlledLoopback,
    );
    assert_eq!(
        wired.requester_records.len(),
        before,
        "the read never mutates the store"
    );

    wired.shutdown().await;
}

// ------------------------------------------------------------ the lane

/// The lane's routers are distinct.
#[test]
fn the_lanes_routers_are_distinct() {
    let alice = RouterKeys::new();
    let bob = RouterKeys::new();
    let charlie = RouterKeys::new();
    assert_ne!(alice.hash, bob.hash);
    assert_ne!(bob.hash, charlie.hash);
    assert_ne!(alice.hash, charlie.hash);
    assert_ne!(alice.static_public_bytes, bob.static_public_bytes);
    assert_ne!(alice.intro_bytes, charlie.intro_bytes);
    assert_ne!(
        alice.signing_public.as_bytes(),
        charlie.signing_public.as_bytes()
    );
}
