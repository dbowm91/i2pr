//! Plan 537's lane -- the relay requester must ask a peer for a relay
//! **without holding the router's NetDB lock**, and its refusal to ask must
//! never be **skipped silently**.
//!
//! Plan 531 built the requester role: the router that cannot reach a peer
//! directly asks the peer's published introducer to relay toward it. The
//! composition root drove that outbound step **inside the bootstrap's mutex**
//! -- the router's NetDB lock -- for the whole step: the step scans the
//! **entire** validated store for candidates, reads up to sixteen candidates'
//! published introducer sets, **signs** the RelayRequest with the router's own
//! key, writes the transport's requester table and queues the request onto the
//! introducer's session.
//!
//! Worse, the last two take the **transport's own mutexes** while the NetDB
//! lock is held: the peer-relay service's requester table and the runtime's
//! state lock. The router's NetDB lock therefore **wraps** the runtime's state
//! lock, where the runtime's own order is sockets -> state.
//!
//! And the step was **gated on the daemon holding a `Bootstrap`**: a router
//! without one did not ask, did not refuse and did not count, so a request
//! that could have been made left no trace at all.
//!
//! The lane drives the **production** material source
//! ([`ValidatedStoreRequesterSource`]) over the **production** requester owner,
//! and it measures the lock:
//!
//! - The request is signed through a signer that **probes the router's NetDB
//!   lock at the moment it signs**. The probe must find the lock free: each
//!   material read is taken under the lock and the guard is released before
//!   the read returns, so the signature and the transport's writes happen with
//!   no lock held.
//! - The measurement is **not vacuous**: a second signer that *holds* the lock
//!   while it signs is observed as holding it, so the probe can tell the two
//!   apart.
//! - The outbound step **runs without a store**: the request is refused with
//!   the typed reason, and counted under its own reason.
//! - A **poisoned** lock is the same typed refusal.
//! - The request is still produced correctly when the store is present: the
//!   requester's behaviour is unchanged.
//! - The reads **mutate nothing** and write no peer profile.
//! - Every refusal the outbound step can reach is typed and counted.
//!
//! The lane asserts counts, states and forms only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;
use i2pr_daemon::relay_requester::{
    LocalIdentitySigner, RelayRequestSigner, RelayRequesterDecision, RelayRequesterOwner,
    RelayRequesterRefusal, ValidatedStoreRequesterSource, drive_relay_request,
};
use i2pr_netdb::{RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2DialTarget, Ssu2Endpoint,
    Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2ServiceHandle,
    Ssu2SocketConfig,
};
use i2pr_transport::PeerId;
use rand_core::{OsRng, SeedableRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The relay tag Charlie publishes.
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
        Self {
            bundle,
            peer: PeerId::from_hash(hash),
            hash,
            static_bytes,
            static_public,
            static_public_bytes,
            intro,
            intro_bytes,
        }
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

/// The router's NetDB, as the composition root holds it: the bootstrap's
/// mutex around the bootstrap's validated store.
fn netdb(records: Vec<ValidatedRouterInfo>) -> Arc<Mutex<Bootstrap>> {
    let mut bootstrap = Bootstrap::new(
        RouterInfoStoreConfig::default(),
        ReseedConfig {
            enabled: false,
            max_sources: 0,
            max_su3_bytes: 0,
            sources: Vec::new(),
        },
    );
    for record in records {
        bootstrap.store_mut().insert(record);
    }
    Arc::new(Mutex::new(bootstrap))
}

/// A signer that **probes the router's NetDB lock** at the moment it signs
/// the request.
///
/// The probe is the measurement: the request is signed after the material
/// reads return, so the lock must be free at that moment. The observation is
/// a flag, and the signer still produces a real signature, so the transport's
/// own verification is unchanged.
struct LockProbeRequestSigner {
    inner: Arc<dyn RelayRequestSigner>,
    netdb: Arc<Mutex<Bootstrap>>,
    free_at_signing: Arc<AtomicBool>,
}

impl LockProbeRequestSigner {
    fn new(
        inner: Arc<dyn RelayRequestSigner>,
        netdb: Arc<Mutex<Bootstrap>>,
        free_at_signing: Arc<AtomicBool>,
    ) -> Self {
        Self {
            inner,
            netdb,
            free_at_signing,
        }
    }
}

impl RelayRequestSigner for LockProbeRequestSigner {
    fn sign_relay_request(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        // The probe: `try_lock` fails while **any** holder has the lock,
        // including this thread, so a free lock is observed exactly.
        let free = self.netdb.try_lock().is_ok();
        self.free_at_signing.store(free, Ordering::SeqCst);
        self.inner.sign_relay_request(preimage)
    }
}

/// A signer that **holds** the router's NetDB lock while it signs.
///
/// This is the lane's control: the probe must be able to tell a lock held
/// across the request from a lock that was released before it.
struct LockHolderRequestSigner {
    inner: Arc<dyn RelayRequestSigner>,
    netdb: Arc<Mutex<Bootstrap>>,
    free_at_signing: Arc<AtomicBool>,
}

impl LockHolderRequestSigner {
    fn new(
        inner: Arc<dyn RelayRequestSigner>,
        netdb: Arc<Mutex<Bootstrap>>,
        free_at_signing: Arc<AtomicBool>,
    ) -> Self {
        Self {
            inner,
            netdb,
            free_at_signing,
        }
    }
}

impl RelayRequestSigner for LockHolderRequestSigner {
    fn sign_relay_request(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        // The control takes the lock with `try_lock`, never with `lock`: a
        // regression that already holds the lock would otherwise block for
        // ever, and a control that hangs proves nothing. A lock that cannot
        // be taken is already held, which is exactly what the probe must
        // observe.
        let Ok(guard) = self.netdb.try_lock() else {
            self.free_at_signing.store(false, Ordering::SeqCst);
            return self.inner.sign_relay_request(preimage);
        };
        let free = self.netdb.try_lock().is_ok();
        self.free_at_signing.store(free, Ordering::SeqCst);
        let signature = self.inner.sign_relay_request(preimage);
        drop(guard);
        signature
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

/// The local endpoint the requester published.
fn local_endpoint_of(fixture: &Fixture) -> Ssu2Endpoint {
    Ssu2Endpoint::from_socket_addr(fixture.bound()).expect("endpoint")
}

/// The three routers the lane drives, with the requester's session to the
/// introducer already established.
struct Wired {
    alice: Fixture,
    bob: Fixture,
    charlie: Fixture,
    /// The router's NetDB, as the composition root holds it.
    netdb: Arc<Mutex<Bootstrap>>,
}

impl Wired {
    /// The peers the requester holds a session with: only the introducer.
    fn live(&self) -> Vec<PeerId> {
        vec![self.bob.keys.peer]
    }

    /// The requester's own signer.
    fn signer(&self) -> Arc<dyn RelayRequestSigner> {
        Arc::new(LocalIdentitySigner::new(Arc::clone(
            &self.alice.keys.bundle,
        )))
    }

    async fn shutdown(self) {
        self.alice.shutdown().await;
        self.bob.shutdown().await;
        self.charlie.shutdown().await;
    }
}

/// Wires the three routers and their one real session.
async fn wire() -> Wired {
    // Alice is the requester, Bob the introducer, Charlie the target.
    let alice_config = Ssu2RuntimeConfig {
        relay_requester: true,
        ..Ssu2RuntimeConfig::default()
    };
    let bob_config = Ssu2RuntimeConfig {
        introducer_service: true,
        ..Ssu2RuntimeConfig::default()
    };
    let alice = Fixture::start(alice_config, RouterKeys::new()).await;
    let bob = Fixture::start(bob_config, RouterKeys::new()).await;
    let charlie = Fixture::start(Ssu2RuntimeConfig::default(), RouterKeys::new()).await;

    // The router's NetDB holds the three validated records: Charlie
    // publishes Bob as its introducer, and Bob publishes a direct
    // address so the introducer resolves.
    let netdb = netdb(vec![
        validated_record(&alice.keys, alice.keys.direct_address(alice.bound())),
        validated_record(&bob.keys, bob.keys.direct_address(bob.bound())),
        validated_record(
            &charlie.keys,
            charlie.keys.introducer_address(&bob.keys.hash, RELAY_TAG),
        ),
    ]);

    // One real session: the requester with the introducer. The request
    // rides it, so the carriage can carry the block.
    dial(&alice, &bob).await;
    wait_for_active(&alice, 1).await;

    Wired {
        alice,
        bob,
        charlie,
        netdb,
    }
}

// ------------------------------------------------------------ the lock

/// The request is signed with the router's NetDB lock **free**.
///
/// The material reads are taken under the lock and the guard is released
/// before each read returns, so the signature -- and the transport's writes
/// that follow it -- happen with no lock held.
#[tokio::test]
async fn the_request_is_signed_with_the_netdb_lock_free() {
    let wired = wire().await;
    let free_at_signing = Arc::new(AtomicBool::new(false));
    let signer = Arc::new(LockProbeRequestSigner::new(
        wired.signer(),
        Arc::clone(&wired.netdb),
        Arc::clone(&free_at_signing),
    ));
    let mut owner = RelayRequesterOwner::new(true, wired.alice.service.clone(), signer);
    let source = ValidatedStoreRequesterSource::new(Some(&wired.netdb));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");

    let decision = drive_relay_request(
        &mut owner,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert!(
        matches!(decision, Some(RelayRequesterDecision::Request(_))),
        "the router asked nobody: {decision:?}"
    );
    assert!(
        free_at_signing.load(Ordering::SeqCst),
        "the request was signed with the router's NetDB lock held"
    );
    assert_eq!(owner.counters().requests_started, 1);

    wired.shutdown().await;
}

/// The measurement is **not vacuous**: a signer that holds the lock while it
/// signs is observed as holding it.
#[tokio::test]
async fn the_lock_measurement_detects_a_lock_held_across_the_request() {
    let wired = wire().await;
    let free_at_signing = Arc::new(AtomicBool::new(true));
    let signer = Arc::new(LockHolderRequestSigner::new(
        wired.signer(),
        Arc::clone(&wired.netdb),
        Arc::clone(&free_at_signing),
    ));
    let mut owner = RelayRequesterOwner::new(true, wired.alice.service.clone(), signer);
    let source = ValidatedStoreRequesterSource::new(Some(&wired.netdb));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");

    let decision = drive_relay_request(
        &mut owner,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert!(
        matches!(decision, Some(RelayRequesterDecision::Request(_))),
        "the router asked nobody: {decision:?}"
    );
    assert!(
        !free_at_signing.load(Ordering::SeqCst),
        "the probe must be able to see a lock held across the request"
    );

    wired.shutdown().await;
}

// ------------------------------------------------- the typed refusal

/// The outbound step **runs** when the router holds no store: the request
/// is refused with the typed reason, counted under its own reason, and never
/// skipped silently.
#[tokio::test]
async fn the_outbound_step_runs_without_a_store_and_counts_the_typed_refusal() {
    let wired = wire().await;
    let mut owner = RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());
    let source = ValidatedStoreRequesterSource::new(None);
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");

    let decision = drive_relay_request(
        &mut owner,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert_eq!(
        decision,
        Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::StoreUnavailable
        )),
        "the step was skipped instead of refused"
    );
    assert_eq!(
        owner.counters().refusals_store_unavailable,
        1,
        "the refusal was not counted under its own reason"
    );
    assert_eq!(owner.counters().requests_started, 0);

    wired.shutdown().await;
}

/// A **poisoned** NetDB lock is the same typed refusal: the step still runs,
/// the refusal is still counted, and nothing is skipped.
#[tokio::test]
async fn a_poisoned_netdb_lock_is_the_same_typed_refusal() {
    let wired = wire().await;
    let mut owner = RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());

    // Poison the router's NetDB lock on purpose.
    let poisoner = Arc::clone(&wired.netdb);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = poisoner.lock().expect("the lock is free");
        panic!("poison the router's NetDB lock on purpose");
    }));
    assert!(outcome.is_err(), "the lock was poisoned on purpose");

    let source = ValidatedStoreRequesterSource::new(Some(&wired.netdb));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let decision = drive_relay_request(
        &mut owner,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert_eq!(
        decision,
        Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::StoreUnavailable
        )),
        "the step was skipped instead of refused"
    );
    assert_eq!(
        owner.counters().refusals_store_unavailable,
        1,
        "a poisoned lock is the same typed refusal"
    );
    assert_eq!(owner.counters().requests_started, 0);

    wired.shutdown().await;
}

// ------------------------------------------------- the unchanged behaviour

/// The request is still produced correctly when the store is present: the
/// requester's behaviour is unchanged.
#[tokio::test]
async fn the_request_is_still_produced_correctly_with_a_store() {
    let wired = wire().await;
    let mut owner = RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());
    let source = ValidatedStoreRequesterSource::new(Some(&wired.netdb));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");

    let decision = drive_relay_request(
        &mut owner,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    let Some(RelayRequesterDecision::Request(candidate)) = decision else {
        panic!("the router asked nobody: {decision:?}");
    };
    // The request names the introducer Charlie published, and the target
    // Charlie itself.
    assert_eq!(
        candidate.introducer.hash().as_bytes(),
        wired.bob.keys.hash.as_bytes()
    );
    assert_eq!(
        candidate.target.hash().as_bytes(),
        wired.charlie.keys.hash.as_bytes()
    );
    assert_eq!(candidate.tag, RELAY_TAG);

    let counters = owner.counters();
    assert_eq!(counters.requests_started, 1, "the request was not started");
    assert_eq!(counters.refusals_store_unavailable, 0);
    assert_eq!(counters.refusals_no_eligible_target, 0);
    assert_eq!(counters.refusals_introducer_unknown, 0);
    assert_eq!(counters.refusals_signing, 0);
    assert_eq!(counters.refusals_transport, 0);
    assert_eq!(counters.refusals_carriage, 0);

    wired.shutdown().await;
}

/// The reads **mutate nothing** and write no peer profile.
#[tokio::test]
async fn the_reads_mutate_nothing_and_write_no_peer_profile() {
    let wired = wire().await;
    let mut owner = RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());
    let source = ValidatedStoreRequesterSource::new(Some(&wired.netdb));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");

    // The snapshots take the lock with `try_lock`, never with `lock`: a
    // regression that holds the router's NetDB lock would otherwise block
    // for ever, and a lane that hangs proves nothing. A lock that cannot
    // be taken is a failure, and it is reported as one.
    let before = {
        let guard = wired
            .netdb
            .try_lock()
            .expect("the router's NetDB lock is free");
        guard.store().stats()
    };
    let hashes_before: Vec<[u8; 32]> = {
        let guard = wired
            .netdb
            .try_lock()
            .expect("the router's NetDB lock is free");
        guard
            .store()
            .iter()
            .map(|(hash, _)| *hash.as_hash().as_bytes())
            .collect()
    };

    drive_relay_request(
        &mut owner,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );

    let after = {
        let guard = wired
            .netdb
            .try_lock()
            .expect("the router's NetDB lock is free");
        guard.store().stats()
    };
    let hashes_after: Vec<[u8; 32]> = {
        let guard = wired
            .netdb
            .try_lock()
            .expect("the router's NetDB lock is free");
        guard
            .store()
            .iter()
            .map(|(hash, _)| *hash.as_hash().as_bytes())
            .collect()
    };
    assert_eq!(before, after, "the reads changed the router's NetDB");
    assert_eq!(
        hashes_before, hashes_after,
        "the reads changed the router's peer population"
    );
    assert_eq!(
        after.record_count, 3,
        "the router's NetDB holds three peers"
    );

    wired.shutdown().await;
}

/// Every refusal the outbound step can reach is typed and counted, and each
/// is counted under **its own** reason.
#[tokio::test]
async fn every_refusal_is_typed_and_counted() {
    let wired = wire().await;

    // A **disabled** owner refuses every window.
    let mut disabled = RelayRequesterOwner::new(false, wired.alice.service.clone(), wired.signer());
    let source = ValidatedStoreRequesterSource::new(Some(&wired.netdb));
    let mut rng = rand_chacha::ChaCha8Rng::try_from_os_rng().expect("entropy");
    let decision = drive_relay_request(
        &mut disabled,
        &source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert_eq!(
        decision,
        Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::Disabled
        ))
    );
    assert_eq!(disabled.counters().refusals_disabled, 1);
    assert_eq!(disabled.counters().refusals_store_unavailable, 0);

    // A router that names no bound endpoint cannot present one.
    let mut nameless = RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());
    let decision = drive_relay_request(
        &mut nameless,
        &source,
        &wired.live(),
        &mut rng,
        None,
        wall_ms(),
        wall_secs(),
    );
    assert_eq!(
        decision,
        Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::NoLocalEndpoint
        ))
    );
    assert_eq!(nameless.counters().refusals_no_local_endpoint, 1);
    assert_eq!(nameless.counters().refusals_disabled, 0);

    // A store that holds no eligible target says so under its own reason.
    let empty = netdb(Vec::new());
    let mut empty_owner =
        RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());
    let empty_source = ValidatedStoreRequesterSource::new(Some(&empty));
    let decision = drive_relay_request(
        &mut empty_owner,
        &empty_source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert_eq!(
        decision,
        Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::NoEligibleTarget
        ))
    );
    assert_eq!(empty_owner.counters().refusals_no_eligible_target, 1);
    assert_eq!(empty_owner.counters().refusals_store_unavailable, 0);

    // A target that publishes an introducer the store does not hold is a
    // distinct refusal, not a silent skip.
    let dave = RouterKeys::new();
    let unknown = RouterKeys::new();
    let dave_netdb = netdb(vec![validated_record(
        &dave,
        dave.introducer_address(&unknown.hash, RELAY_TAG),
    )]);
    let mut dave_owner =
        RelayRequesterOwner::new(true, wired.alice.service.clone(), wired.signer());
    let dave_source = ValidatedStoreRequesterSource::new(Some(&dave_netdb));
    let decision = drive_relay_request(
        &mut dave_owner,
        &dave_source,
        &wired.live(),
        &mut rng,
        Some(local_endpoint_of(&wired.alice)),
        wall_ms(),
        wall_secs(),
    );
    assert_eq!(
        decision,
        Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::NoEligibleTarget
        ))
    );
    assert_eq!(
        dave_owner.counters().refusals_introducer_unknown,
        1,
        "the unknown introducer was not counted under its own reason"
    );
    assert_eq!(dave_owner.counters().refusals_store_unavailable, 0);
    assert_eq!(dave_owner.counters().requests_started, 0);

    wired.shutdown().await;
}
