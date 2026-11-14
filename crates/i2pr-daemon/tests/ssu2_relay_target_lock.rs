//! Plan 536's lane -- the relay target role must answer a relay
//! introduction **without holding the router's NetDB lock**, and its drain
//! must never be **skipped silently**.
//!
//! Plan 532 built the target role: the router that an introducer names
//! receives the Relay Intro, verifies it through the transport's own target
//! state machine, signs the accept and stages the hole punch. But the
//! composition root drained that handoff **inside the bootstrap's mutex** --
//! the router's NetDB lock -- for the whole pass, and the pass is bounded at
//! sixteen answers, each of which signs an accept. And the drain was gated
//! on the daemon **holding** a `Bootstrap` at all: a router without one did
//! not drain, did not refuse and did not count, so an introduction that
//! arrived and could not be answered left no trace.
//!
//! The lane drives the **production** material source
//! ([`ValidatedStoreMaterialSource`]) over the **production** drain and the
//! **production** target owner, and it measures the lock:
//!
//! - The accept is signed through a signer that **probes the router's NetDB
//!   lock at the moment it signs**. The probe must find the lock free: the
//!   material read is taken under the lock and the guard is released before
//!   the answer is produced.
//! - The measurement is **not vacuous**: a second signer that *holds* the
//!   lock while it signs is observed as holding it, so the probe can tell
//!   the two apart.
//! - The drain **runs without a store**: the introduction is pulled, refused
//!   with the typed reason, and counted under its own reason.
//! - The answer is still produced correctly when the store is present: the
//!   target role's behaviour is unchanged.
//! - The read **mutates nothing** and writes no peer profile.
//! - Every refusal the drain can reach is typed and counted.
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
use i2pr_daemon::relay_requester::LocalIdentitySigner;
use i2pr_daemon::relay_target::{
    RelayTargetMaterialSource, RelayTargetOutcome, RelayTargetOwner, RelayTargetRefusal,
    RelayTargetSigner, ValidatedStoreMaterialSource, drive_relay_target,
};
use i2pr_daemon::ssu2_relay_drain::{TargetEventSource, drain_target_events};
use i2pr_netdb::{RouterHash, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, SigningPublicKey};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, RelayIntroBlock, Ssu2DialTarget,
    Ssu2Endpoint, Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RuntimeConfig, Ssu2RuntimeService,
    Ssu2ServiceHandle, Ssu2SocketConfig,
};
use i2pr_transport::{LinkId, PeerId};
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The relay tag the introduction names.
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

/// The lane's target source: it yields the events the lane hands it, one
/// per pull, and then nothing.
struct SingleTarget {
    events: Vec<i2pr_runtime::Ssu2InboundTarget>,
}

impl TargetEventSource for SingleTarget {
    fn try_next_target_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundTarget> {
        if self.events.is_empty() {
            None
        } else {
            Some(self.events.remove(0))
        }
    }
}

/// A signer that **probes the router's NetDB lock** at the moment it signs
/// the accept.
///
/// The probe is the measurement: the accept is signed after the material
/// read returns, so the lock must be free at that moment. The observation is
/// a flag, and the signer still produces a real signature, so the transport's
/// own verification is unchanged.
struct LockProbeSigner {
    inner: Arc<dyn RelayTargetSigner>,
    netdb: Arc<Mutex<Bootstrap>>,
    free_at_signing: Arc<AtomicBool>,
}

impl LockProbeSigner {
    fn new(
        inner: Arc<dyn RelayTargetSigner>,
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

impl RelayTargetSigner for LockProbeSigner {
    fn sign_relay_response(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        // The probe: `try_lock` fails while **any** holder has the lock,
        // including this thread, so a free lock is observed exactly.
        let free = self.netdb.try_lock().is_ok();
        self.free_at_signing.store(free, Ordering::SeqCst);
        self.inner.sign_relay_response(preimage)
    }
}

/// A signer that **holds** the router's NetDB lock while it signs.
///
/// This is the lane's control: the probe must be able to tell a lock held
/// across the answer from a lock that was released before it.
struct LockHolderSigner {
    inner: Arc<dyn RelayTargetSigner>,
    netdb: Arc<Mutex<Bootstrap>>,
    free_at_signing: Arc<AtomicBool>,
}

impl LockHolderSigner {
    fn new(
        inner: Arc<dyn RelayTargetSigner>,
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

impl RelayTargetSigner for LockHolderSigner {
    fn sign_relay_response(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        let guard = self.netdb.lock().expect("the lock is free before signing");
        let free = self.netdb.try_lock().is_ok();
        self.free_at_signing.store(free, Ordering::SeqCst);
        let signature = self.inner.sign_relay_response(preimage);
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

/// The three routers the lane drives, with the target's session to the
/// introducer already established.
struct Wired {
    alice: Fixture,
    bob: Fixture,
    charlie: Fixture,
    /// The router's NetDB, as the composition root holds it.
    netdb: Arc<Mutex<Bootstrap>>,
}

impl Wired {
    /// The one introduction the target is asked to answer.
    fn event(&self, nonce: u32) -> i2pr_runtime::Ssu2InboundTarget {
        let endpoint = local_endpoint_of(&self.alice);
        let timestamp = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        let block = build_intro(
            &self.alice.keys,
            &self.bob.keys.hash,
            &self.charlie.keys.hash,
            nonce,
            RELAY_TAG,
            timestamp,
            endpoint,
        );
        i2pr_runtime::Ssu2InboundTarget {
            link_id: LinkId::new(1).expect("link"),
            peer: self.bob.keys.peer,
            block,
        }
    }

    /// The target's owner, with the router's own identity as the signer.
    fn owner(&self, signer: Arc<dyn RelayTargetSigner>) -> RelayTargetOwner {
        RelayTargetOwner::new(true, self.charlie.service.clone(), signer)
    }

    async fn shutdown(self) {
        self.alice.shutdown().await;
        self.bob.shutdown().await;
        self.charlie.shutdown().await;
    }
}

/// Wires the three routers and establishes the target's session with the
/// introducer, so the accept can be put on the introducer's session.
async fn wire() -> Wired {
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

    // The router's NetDB holds the requester's validated record.
    let netdb = netdb(vec![validated_record(
        &alice.keys,
        alice.keys.direct_address(alice.bound()),
    )]);

    Wired {
        alice,
        bob,
        charlie,
        netdb,
    }
}

// ------------------------------------------------------------ the lock

/// The accept is signed with the router's NetDB lock **free**.
///
/// The material read is taken under the bootstrap's mutex and the guard is
/// released before the read returns, so the accept is signed and staged with
/// no lock held. The probe observes the lock at the moment the accept is
/// signed.
#[tokio::test]
async fn the_answer_is_signed_with_the_netdb_lock_free() {
    let wired = wire().await;
    let event = wired.event(0x00C0_FFEE);
    let free_at_signing = Arc::new(AtomicBool::new(false));
    let signer = Arc::new(LockProbeSigner::new(
        Arc::new(LocalIdentitySigner::new(Arc::clone(
            &wired.charlie.keys.bundle,
        ))),
        Arc::clone(&wired.netdb),
        Arc::clone(&free_at_signing),
    ));
    let mut owner = wired.owner(signer);

    let source = ValidatedStoreMaterialSource::new(Some(&wired.netdb));
    let outcome = drive_relay_target(&mut owner, &source, &event, wall_secs(), wall_ms());
    assert_eq!(
        outcome,
        RelayTargetOutcome::Answered,
        "the introduction is answered"
    );
    assert!(
        free_at_signing.load(Ordering::SeqCst),
        "the router's NetDB lock was free when the accept was signed"
    );
    assert_eq!(owner.counters().accepts_queued, 1);
    assert_eq!(owner.counters().hole_punches_staged, 1);

    // The lock is free **after** the answer too: the read released it, and
    // the answer did not take it back.
    assert!(
        wired.netdb.try_lock().is_ok(),
        "the router's NetDB lock is free after the answer"
    );

    wired.shutdown().await;
}

/// The measurement is not vacuous: a signer that **holds** the lock while
/// it signs is observed as holding it.
///
/// Without this control the probe above could pass for a probe that always
/// reports "free".
#[tokio::test]
async fn the_lock_measurement_detects_a_lock_held_across_the_answer() {
    let wired = wire().await;
    let event = wired.event(0x00C0_FFEF);
    let free_at_signing = Arc::new(AtomicBool::new(true));
    let signer = Arc::new(LockHolderSigner::new(
        Arc::new(LocalIdentitySigner::new(Arc::clone(
            &wired.charlie.keys.bundle,
        ))),
        Arc::clone(&wired.netdb),
        Arc::clone(&free_at_signing),
    ));
    let mut owner = wired.owner(signer);

    let source = ValidatedStoreMaterialSource::new(Some(&wired.netdb));
    let outcome = drive_relay_target(&mut owner, &source, &event, wall_secs(), wall_ms());
    assert_eq!(
        outcome,
        RelayTargetOutcome::Answered,
        "the answer is still produced"
    );
    assert!(
        !free_at_signing.load(Ordering::SeqCst),
        "a lock held across the answer is observed as held"
    );

    wired.shutdown().await;
}

// ------------------------------------------------------------ the drain

/// The drain **runs** when the router holds no store, and the introduction
/// is refused with the typed reason and counted under its own reason.
///
/// Before Plan 536 the whole drain was skipped when the daemon held no
/// `Bootstrap`: the introduction sat in the handoff, nothing was refused,
/// nothing was counted and the operator saw nothing.
#[tokio::test]
async fn the_drain_runs_without_a_store_and_counts_the_typed_refusal() {
    let wired = wire().await;
    let event = wired.event(0x00C0_FFEE);
    let mut owner = wired.owner(Arc::new(LocalIdentitySigner::new(Arc::clone(
        &wired.charlie.keys.bundle,
    ))));

    // The router holds **no** validated store.
    let source = ValidatedStoreMaterialSource::new(None);
    let mut events = SingleTarget {
        events: vec![event],
    };
    let outcome = drain_target_events(&mut events, &mut owner, &source, wall_secs(), wall_ms());

    assert_eq!(outcome.drained, 1, "the drain was not skipped");
    assert_eq!(outcome.answered, 0, "nothing was answered");
    assert_eq!(outcome.refused, 1, "the introduction was refused");
    assert_eq!(outcome.replays, 0);
    assert_eq!(
        owner.counters().intros_received,
        1,
        "the introduction was seen"
    );
    assert_eq!(
        owner.counters().refusals_store_unavailable,
        1,
        "the refusal is counted under its own reason"
    );
    assert_eq!(
        owner.counters().intros_verified,
        0,
        "nothing was verified without a store"
    );
    assert_eq!(owner.counters().accepts_queued, 0);
    assert_eq!(owner.counters().hole_punches_staged, 0);

    // The refusal is a **local** condition: it is about this router, and it
    // is never written into the requester's profile.
    assert_eq!(
        RelayTargetRefusal::StoreUnavailable.reason(),
        "store-unavailable",
        "the refusal has its own reason key"
    );
    assert_eq!(
        wired.netdb.lock().expect("the lock is free").store().len(),
        1,
        "the requester's record is untouched"
    );

    wired.shutdown().await;
}

/// A **poisoned** NetDB lock is the same typed refusal: the drain still
/// runs, the refusal is still counted, and nothing is skipped.
#[tokio::test]
async fn a_poisoned_netdb_lock_is_the_same_typed_refusal() {
    let wired = wire().await;
    let event = wired.event(0x00C0_FFED);
    let mut owner = wired.owner(Arc::new(LocalIdentitySigner::new(Arc::clone(
        &wired.charlie.keys.bundle,
    ))));

    // Poison the router's NetDB lock on purpose.
    let poisoner = Arc::clone(&wired.netdb);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = poisoner.lock().expect("the lock is free");
        panic!("poison the router's NetDB lock on purpose");
    }));
    assert!(outcome.is_err(), "the lock was poisoned on purpose");

    let source = ValidatedStoreMaterialSource::new(Some(&wired.netdb));
    let mut events = SingleTarget {
        events: vec![event],
    };
    let outcome = drain_target_events(&mut events, &mut owner, &source, wall_secs(), wall_ms());
    assert_eq!(outcome.drained, 1, "the drain was not skipped");
    assert_eq!(outcome.refused, 1, "the introduction was refused");
    assert_eq!(
        owner.counters().refusals_store_unavailable,
        1,
        "a poisoned lock is the same typed refusal"
    );
    assert_eq!(owner.counters().accepts_queued, 0);
    assert_eq!(owner.counters().hole_punches_staged, 0);

    wired.shutdown().await;
}

// ------------------------------------------------------------ the answer

/// The answer is still produced correctly when the store is present: the
/// target role's behaviour is unchanged.
#[tokio::test]
async fn the_answer_is_still_produced_correctly_with_a_store() {
    let wired = wire().await;
    let event = wired.event(0x00C0_FFEE);
    let mut owner = wired.owner(Arc::new(LocalIdentitySigner::new(Arc::clone(
        &wired.charlie.keys.bundle,
    ))));

    let source = ValidatedStoreMaterialSource::new(Some(&wired.netdb));
    let outcome = drive_relay_target(&mut owner, &source, &event, wall_secs(), wall_ms());
    assert_eq!(
        outcome,
        RelayTargetOutcome::Answered,
        "the introduction is answered"
    );
    assert_eq!(owner.counters().intros_received, 1);
    assert_eq!(owner.counters().intros_verified, 1);
    assert_eq!(owner.counters().intros_answered, 1);
    assert_eq!(
        owner.counters().accepts_queued,
        1,
        "the accept was put on the introducer's session"
    );
    assert_eq!(
        owner.counters().hole_punches_staged,
        1,
        "the hole punch was staged toward the requester"
    );

    // The transport's own replay suppression is unchanged: the second copy
    // of the same introduction produces no second accept and no second
    // hole punch.
    let outcome = drive_relay_target(&mut owner, &source, &event, wall_secs(), wall_ms());
    assert_eq!(
        outcome,
        RelayTargetOutcome::ReplaySuppressed,
        "the replay is suppressed"
    );
    assert_eq!(owner.counters().replays_suppressed, 1);
    assert_eq!(owner.counters().accepts_queued, 1);
    assert_eq!(owner.counters().hole_punches_staged, 1);

    wired.shutdown().await;
}

// ------------------------------------------------------------ the read

/// The read mutates nothing and writes no peer profile.
#[tokio::test]
async fn the_read_mutates_nothing_and_writes_no_peer_profile() {
    let wired = wire().await;
    let requester = *wired.alice.keys.hash.as_bytes();
    let before = wired.netdb.lock().expect("the lock is free").store().len();

    let source = ValidatedStoreMaterialSource::new(Some(&wired.netdb));
    let material = source
        .read_requester_material(requester)
        .expect("the requester's material is read");
    assert_eq!(
        material.signing_key, wired.alice.keys.signing_public,
        "the material is the requester's own"
    );

    // A requester this router does not hold is refused, and the refusal
    // mutates nothing either.
    let outcome = source.read_requester_material([0x77; 32]);
    assert!(
        matches!(outcome, Err(RelayTargetRefusal::RequesterUnknown)),
        "an unknown requester is refused"
    );

    let after = wired.netdb.lock().expect("the lock is free").store().len();
    assert_eq!(after, before, "the read never mutates the store");

    // No peer profile was rewritten. The guard is scoped, so it is never
    // held across the teardown's await.
    {
        let guard = wired.netdb.lock().expect("the lock is free");
        let record = guard
            .store()
            .get(&RouterHash::from_bytes(requester))
            .expect("the requester's record");
        assert_eq!(
            record.router_info().router_identity().signing_key(),
            &wired.alice.keys.signing_public,
            "a peer's record is never rewritten"
        );
    }

    wired.shutdown().await;
}

// ------------------------------------------------------------ the refusals

/// Every refusal the drain can reach is typed and counted, and none of them
/// writes anything into a peer's profile.
#[tokio::test]
async fn every_refusal_is_typed_and_counted_and_writes_nothing() {
    let wired = wire().await;
    let event = wired.event(0x00C0_FFEE);
    let router_netdb = Arc::clone(&wired.netdb);

    // A **disabled** owner refuses every introduction and reads nothing.
    let mut disabled = RelayTargetOwner::new(
        false,
        wired.charlie.service.clone(),
        Arc::new(LocalIdentitySigner::new(Arc::clone(
            &wired.charlie.keys.bundle,
        ))),
    );
    let source = ValidatedStoreMaterialSource::new(Some(&router_netdb));
    let outcome = drive_relay_target(&mut disabled, &source, &event, wall_secs(), wall_ms());
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

    // An **enabled** owner whose router holds no store refuses under its own
    // reason.
    let mut without_store = wired.owner(Arc::new(LocalIdentitySigner::new(Arc::clone(
        &wired.charlie.keys.bundle,
    ))));
    let absent = ValidatedStoreMaterialSource::new(None);
    let outcome = drive_relay_target(&mut without_store, &absent, &event, wall_secs(), wall_ms());
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::StoreUnavailable),
        "an absent store is refused under its own reason"
    );
    assert_eq!(without_store.counters().refusals_store_unavailable, 1);

    // An **enabled** owner whose router holds a store but not the
    // requester refuses under the requester's own reason -- and the
    // refusal is a **local** condition that is never written into the
    // requester's profile.
    let empty = netdb(vec![]);
    let mut unknown = wired.owner(Arc::new(LocalIdentitySigner::new(Arc::clone(
        &wired.charlie.keys.bundle,
    ))));
    let source = ValidatedStoreMaterialSource::new(Some(&empty));
    let outcome = drive_relay_target(&mut unknown, &source, &event, wall_secs(), wall_ms());
    assert_eq!(
        outcome,
        RelayTargetOutcome::Refused(RelayTargetRefusal::RequesterUnknown),
        "an unknown requester is refused"
    );
    assert_eq!(unknown.counters().refusals_requester_unknown, 1);

    // Every reason key is distinct, so a refusal can be counted by reason.
    let reasons = [
        RelayTargetRefusal::Disabled.reason(),
        RelayTargetRefusal::NoLocalEndpoint.reason(),
        RelayTargetRefusal::RequesterUnknown.reason(),
        RelayTargetRefusal::RequesterNoSsu2Address.reason(),
        RelayTargetRefusal::RequesterUnparsableAddress.reason(),
        RelayTargetRefusal::RequesterNoIntroKey.reason(),
        RelayTargetRefusal::StoreUnavailable.reason(),
        RelayTargetRefusal::Replay.reason(),
        RelayTargetRefusal::SigningRefused.reason(),
        RelayTargetRefusal::TransportRefused.reason(),
        RelayTargetRefusal::CarriageRefused.reason(),
        RelayTargetRefusal::InvalidSignature.reason(),
        RelayTargetRefusal::StaleTimestamp.reason(),
        RelayTargetRefusal::TooManyRequests.reason(),
        RelayTargetRefusal::NoRandomness.reason(),
        RelayTargetRefusal::DestinationRefused.reason(),
        RelayTargetRefusal::StagingRefused.reason(),
        RelayTargetRefusal::EstablishmentRefused.reason(),
    ];
    let mut unique: Vec<&str> = reasons.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        reasons.len(),
        "every refusal is counted under its own reason"
    );

    // Nothing was written into any peer's record. The guard is scoped, so
    // it is never held across the teardown's await.
    {
        let guard = wired.netdb.lock().expect("the lock is free");
        let record = guard
            .store()
            .get(&RouterHash::from_bytes(*wired.alice.keys.hash.as_bytes()))
            .expect("the requester's record");
        assert_eq!(
            record.router_info().router_identity().signing_key(),
            &wired.alice.keys.signing_public,
            "a peer's record is never rewritten"
        );
        assert_eq!(
            guard.store().len(),
            1,
            "a peer's profile is never rewritten"
        );
    }

    wired.shutdown().await;
}
