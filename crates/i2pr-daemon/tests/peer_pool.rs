//! Plan 522's lane — the router holds a bounded pool of outbound peer
//! sessions.
//!
//! The router used to dial per need and hold nothing between needs, so the
//! publication's ordinary path was the handshake path. Plan 522 gives the
//! router a **bounded pool** of outbound sessions to the peers it already
//! knows, and the transport an **outbound-only** keepalive that holds a
//! pooled session open.
//!
//! The lane drives the **public** behaviour of the daemon's own product
//! surface:
//!
//! 1. the opt-in is **off** by default, and a router with it off plans no
//!    dial and reads no population;
//! 2. the opt-in requires the SSU2 service, and is refused without it;
//! 3. the opt-in is refused when the session's idle timeout cannot hold the
//!    keepalive's unanswered deadline;
//! 4. the pool plans a dial to a peer it knows, and reuses a session it
//!    already holds;
//! 5. a peer whose session lapsed is planned again, and the lapse is
//!    counted;
//! 6. a candidate beyond the pool's ceiling is **refused** and counted, and
//!    the per-period dial budget bounds the dials;
//! 7. a candidate the version gate refuses is never planned;
//! 8. every dial outcome is counted under its own reason, and nothing is
//!    skipped silently;
//! 9. the pool **establishes a real session** to a peer it knows, over a
//!    real SSU2 handshake between two daemon-owned services on loopback;
//! 10. a peer with no dialable address is refused under its own reason;
//! 11. a router with the opt-in off establishes **no** session;
//! 12. the keepalive's interval reaches the runtime **only** when the pool
//!     is on;
//! 13. the counters reach the inspection plane.
//!
//! The lane names no peer in a diagnostic and retains no payload: the
//! counters are the receipt, and the receipt is the whole content.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol_inspection::{InspectionHandles, PeerPoolCounterCell, ServiceEndpoint};
use i2pr_daemon::peer_pool::{
    MAX_DIALS_PER_PERIOD, MAX_POOLED_PEERS, PeerPoolCounters, PeerPoolDialOutcome, PeerPoolOwner,
};
use i2pr_daemon::peer_pool_task::{PeerPoolTask, run_peer_pool_task};
use i2pr_daemon::router_i2np::{Ssu2DaemonHandle, Ssu2DaemonService};
use i2pr_netdb::{
    RouterHash, RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, RouterInfo};
use i2pr_runtime::{
    CancellationReason, CancellationToken, ChildFailurePolicy, ChildScope, IntroKey,
    Ssu2IdentityMaterial, Ssu2PublicKey,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{SeedableRng, TryRngCore};

/// The bounded dial timeout the lane's dials use.
const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
/// The bounded deadline the lane's waits use.
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
/// The lane's polling interval.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// The bounded window the "the pool is off" row watches.
const QUIET_WINDOW: Duration = Duration::from_secs(1);

/// The opt-in is off by default, so the default tree changes in no way.
const DEFAULT_CONFIG: &str =
    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";

/// The pool needs the SSU2 service, and the refusal is explicit.
const POOL_WITHOUT_SERVICE: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = false\npeer_pool = true\n";

/// The keepalive's unanswered deadline cannot fit inside this idle timeout.
const POOL_WITH_SHORT_IDLE: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\npeer_pool = true\nidle_timeout_ms = 180000\n";

/// The enabled posture the lane drives.
const POOL_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\npeer_pool = true\n";

/// A published API version **at** the tree's floodfill floor.
const AT_FLOOR_VERSION: &str = "0.9.62";

/// A published API version **below** the tree's floodfill floor.
const BELOW_FLOOR_VERSION: &str = "0.9.58";

/// The floodfill capability flag.
const FLOODFILL_CAPS: &str = "f";

/// The seed base the lane's deterministic identities are drawn from.
const SEED_BASE: u64 = 0x522_0000;

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

/// One peer's own identity and its SSU2 material.
struct PeerIdentity {
    bundle: RouterIdentityBundle,
    hash: RouterHash,
    proto_hash: Hash,
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    intro: IntroKey,
}

/// Draws one peer's identity deterministically.
fn make_identity(seed: u64) -> PeerIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let proto_hash = bundle.identity().hash().expect("hash");
    let hash = i2pr_netdb::router_hash(bundle.identity()).expect("router hash");
    let static_secret = X25519PrivateKey::generate(&mut rng).expect("static");
    let static_bytes = *static_secret.secret_bytes();
    let static_public =
        i2pr_runtime::Ssu2PublicKey::new(static_secret.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    loop {
        rng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let intro = i2pr_runtime::IntroKey::new(intro_bytes);
    PeerIdentity {
        bundle,
        hash,
        proto_hash,
        static_bytes,
        static_public,
        intro,
    }
}

/// Signs a peer's RouterInfo with an SSU2 address on `port`.
///
/// The same identity signs **two** records in this lane: the one the peer's
/// own service carries, and the one the dialling router's store holds. They
/// differ only in the address's port, and the router hash is computed over
/// the **identity**, so both records name the same router.
fn sign_router_info(
    peer: &PeerIdentity,
    port: u16,
    caps: Option<&str>,
    version: Option<&str>,
    with_address: bool,
) -> RouterInfo {
    // The **address** carries the transport's own options and the RouterInfo
    // carries the router's own options. They are separate mappings: the
    // address parser refuses an option it does not know, so a router option
    // inside the address would make the whole record undiallable.
    let address_options = if with_address {
        Mapping::from_entries(vec![
            ("host".to_string(), "127.0.0.1".to_string()),
            ("port".to_string(), port.to_string()),
            ("v".to_string(), "2".to_string()),
            (
                "s".to_string(),
                i2p_b64_encode(peer.static_public.as_bytes()),
            ),
            ("i".to_string(), i2p_b64_encode(peer.intro.as_bytes())),
        ])
        .expect("address options")
    } else {
        Mapping::empty()
    };
    let mut router_entries: Vec<(String, String)> = Vec::new();
    if let Some(caps) = caps {
        router_entries.push(("caps".to_string(), caps.to_string()));
    }
    if let Some(version) = version {
        router_entries.push(("router.version".to_string(), version.to_string()));
    }
    let router_options = Mapping::from_entries(router_entries).expect("router options");
    let addresses = if with_address {
        vec![
            RouterAddress::new(
                10,
                Date::from_millis(9_999_999_999_999),
                "SSU2".to_string(),
                address_options,
            )
            .expect("address"),
        ]
    } else {
        Vec::new()
    };
    peer.bundle
        .sign_router_info(
            Date::from_millis(wall_ms().saturating_sub(1_000)),
            addresses,
            Vec::new(),
            router_options,
        )
        .expect("sign")
}

/// Validates one signed record the way the router's own store does.
fn validated_record(
    peer: &PeerIdentity,
    port: u16,
    caps: Option<&str>,
    version: Option<&str>,
    with_address: bool,
) -> ValidatedRouterInfo {
    let info = sign_router_info(peer, port, caps, version, with_address);
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(wall_ms())),
    )
    .expect("validate")
}

/// One daemon-owned SSU2 service: the router's own transport.
struct Fixture {
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
}

impl Fixture {
    async fn start(config: &i2pr_daemon::config::Ssu2Config, peer: &PeerIdentity) -> Self {
        let info = sign_router_info(peer, 44_101, None, None, true);
        let router_info = info
            .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode");
        let service = Ssu2DaemonService::new(
            config,
            Ssu2IdentityMaterial {
                router_hash: peer.proto_hash,
                static_secret_bytes: peer.static_bytes,
                intro_key: peer.intro,
                router_info,
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
        }
    }

    fn bound(&self) -> std::net::SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    async fn shutdown(self) {
        self.handle.shutdown();
        self.scope.shutdown().await;
        self.token.cancel(CancellationReason::TestHarnessTeardown);
    }
}

/// The pool's own world: the bootstrap store, the counter cell and the task.
struct PoolHarness {
    cell: Arc<PeerPoolCounterCell>,
    scope: ChildScope,
    token: CancellationToken,
}

impl PoolHarness {
    fn start(
        records: Vec<ValidatedRouterInfo>,
        enabled: bool,
        handle: i2pr_daemon::router_i2np::Ssu2PublicationHandle,
        local_hash: RouterHash,
        profile: i2pr_daemon::config::Ssu2QualificationProfile,
    ) -> Self {
        let config = Config::parse(DEFAULT_CONFIG).expect("config");
        let bootstrap = Arc::new(StdMutex::new(Bootstrap::new(
            RouterInfoStoreConfig::default(),
            config.reseed.clone(),
        )));
        {
            let mut guard = bootstrap.lock().expect("bootstrap");
            for record in records {
                guard.store_mut().insert(record);
            }
        }
        let cell: Arc<PeerPoolCounterCell> = Arc::new(StdMutex::new(PeerPoolCounters::default()));
        let task = PeerPoolTask {
            owner: PeerPoolOwner::new(enabled),
            handle,
            bootstrap: Some(Arc::clone(&bootstrap)),
            local_hash: Some(local_hash),
            dial_timeout: DIAL_TIMEOUT,
            qualification_profile: profile,
            counters_cell: Arc::clone(&cell),
        };
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        scope
            .spawn(move |cancellation| async move { run_peer_pool_task(task, cancellation).await })
            .expect("the pool's task is spawned");
        Self { cell, scope, token }
    }

    fn counters(&self) -> PeerPoolCounters {
        *self.cell.lock().expect("counters")
    }

    /// Waits, with a bounded deadline, until the counters satisfy `wanted`.
    async fn wait_for(
        &self,
        wanted: impl Fn(&PeerPoolCounters) -> bool,
        what: &str,
    ) -> PeerPoolCounters {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            let counters = self.counters();
            if wanted(&counters) {
                return counters;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{what} did not happen: {counters:?}"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn shutdown(self) {
        self.scope.shutdown().await;
        self.token.cancel(CancellationReason::TestHarnessTeardown);
    }
}

/// A store holding one eligible floodfill, and nothing else.
fn store_with_one_floodfill(peer: &PeerIdentity, port: u16) -> RouterInfoStore {
    let mut store = RouterInfoStore::default();
    store.insert(validated_record(
        peer,
        port,
        Some(FLOODFILL_CAPS),
        Some(AT_FLOOR_VERSION),
        true,
    ));
    store
}

// ---------------------------------------------------------------- the opt-in

#[test]
fn the_opt_in_is_off_by_default() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config");
    assert!(
        !config.ssu2.peer_pool,
        "the peer-pool switch must default to off"
    );
    let peer = make_identity(SEED_BASE);
    let local = make_identity(SEED_BASE + 999);
    let store = store_with_one_floodfill(&peer, 44_101);
    let mut owner = PeerPoolOwner::new(config.ssu2.peer_pool);
    assert!(!owner.enabled());
    let planned = owner.plan(&store, &local.hash, &[]);
    assert!(
        planned.is_empty(),
        "a router with the opt-in off plans no dial"
    );
    assert_eq!(
        owner.counters().population_reads,
        0,
        "a router with the opt-in off reads no population"
    );
    assert_eq!(owner.counters().dials_started, 0);
    assert_eq!(owner.counters().dials_established, 0);
}

#[test]
fn the_opt_in_requires_the_ssu2_service() {
    let error = Config::parse(POOL_WITHOUT_SERVICE).expect_err("must be refused");
    let message = error.to_string();
    assert!(
        message.contains("ssu2.peer_pool"),
        "the refusal must name the field, got: {message}"
    );
}

#[test]
fn the_opt_in_is_refused_when_the_idle_timeout_cannot_hold_the_keepalive() {
    let error = Config::parse(POOL_WITH_SHORT_IDLE).expect_err("must be refused");
    let message = error.to_string();
    assert!(
        message.contains("ssu2.idle_timeout_ms"),
        "the refusal must name the field it refuses, got: {message}"
    );
    assert!(
        message.contains("keepalive"),
        "the refusal must name the keepalive it collides with, got: {message}"
    );
}

#[test]
fn the_enabled_posture_is_accepted() {
    let config = Config::parse(POOL_ENABLED).expect("valid config");
    assert!(config.ssu2.peer_pool);
    assert!(config.ssu2.enabled);
}

// --------------------------------------------------------- the pool's own plan

#[test]
fn the_pool_plans_a_dial_to_a_peer_it_knows() {
    let peer = make_identity(SEED_BASE + 1);
    let local = make_identity(SEED_BASE + 1 + 900);
    let store = store_with_one_floodfill(&peer, 44_101);
    let mut owner = PeerPoolOwner::new(true);
    let planned = owner.plan(&store, &local.hash, &[]);
    assert_eq!(planned, vec![peer.hash], "the pool dials the peer it knows");
    let counters = *owner.counters();
    assert_eq!(counters.population_reads, 1);
    assert_eq!(counters.candidates_examined, 1);
    assert_eq!(counters.candidates_refused, 0);
    assert_eq!(counters.dials_started, 1);
    assert_eq!(counters.dials_established, 0);
    assert_eq!(counters.ceiling_refused, 0);
    assert_eq!(counters.budget_refused, 0);
    assert_eq!(counters.dial_in_flight, 0);
}

#[test]
fn the_pool_reuses_a_session_it_already_holds() {
    let peer = make_identity(SEED_BASE + 2);
    let local = make_identity(SEED_BASE + 2 + 900);
    let store = store_with_one_floodfill(&peer, 44_101);
    let mut owner = PeerPoolOwner::new(true);
    let planned = owner.plan(&store, &local.hash, &[]);
    assert_eq!(planned.len(), 1);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Established);

    // The peer now holds a session, so the pool plans nothing for it.
    let planned = owner.plan(&store, &local.hash, &[*peer.hash.as_hash()]);
    assert!(
        planned.is_empty(),
        "a peer that already holds a session is never dialled again"
    );
    let counters = *owner.counters();
    assert_eq!(counters.sessions_reused, 1);
    assert_eq!(counters.peers_admitted, 1);
    assert_eq!(
        counters.dials_started, 1,
        "the pool dials the peer only once"
    );
}

#[test]
fn the_pool_replans_a_peer_whose_session_lapsed() {
    let peer = make_identity(SEED_BASE + 3);
    let local = make_identity(SEED_BASE + 3 + 900);
    let store = store_with_one_floodfill(&peer, 44_101);
    let mut owner = PeerPoolOwner::new(true);
    owner.plan(&store, &local.hash, &[]);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Established);
    owner.plan(&store, &local.hash, &[*peer.hash.as_hash()]);

    // The session is gone, so the pool plans the peer again and counts the
    // lapse under its own reason.
    let planned = owner.plan(&store, &local.hash, &[]);
    assert_eq!(
        planned,
        vec![peer.hash],
        "a peer whose session lapsed is planned again"
    );
    let counters = *owner.counters();
    assert_eq!(counters.sessions_lapsed, 1);
    assert_eq!(counters.dials_started, 2);
}

#[test]
fn the_pool_refuses_a_candidate_beyond_its_ceiling() {
    let mut store = RouterInfoStore::default();
    for index in 0..20_u64 {
        let peer = make_identity(SEED_BASE + 100 + index);
        store.insert(validated_record(
            &peer,
            44_101,
            Some(FLOODFILL_CAPS),
            Some(AT_FLOOR_VERSION),
            true,
        ));
    }
    let local = make_identity(SEED_BASE + 999);
    let mut owner = PeerPoolOwner::new(true);
    let planned = owner.plan(&store, &local.hash, &[]);
    assert_eq!(
        planned.len(),
        MAX_DIALS_PER_PERIOD,
        "the pool never starts more dials than its per-period budget"
    );
    let counters = *owner.counters();
    assert_eq!(counters.candidates_examined, 20);
    assert_eq!(
        counters.ceiling_refused, 5,
        "a candidate beyond the ceiling is refused and counted"
    );
    assert_eq!(
        counters.budget_refused, 13,
        "a candidate beyond the per-period budget is refused and counted"
    );
    assert_eq!(counters.dials_started, MAX_DIALS_PER_PERIOD as u64);
    assert_eq!(counters.peers_admitted, 0);
}

#[test]
fn the_pool_never_exceeds_its_ceiling() {
    let mut store = RouterInfoStore::default();
    for index in 0..40_u64 {
        let peer = make_identity(SEED_BASE + 200 + index);
        store.insert(validated_record(
            &peer,
            44_101,
            Some(FLOODFILL_CAPS),
            Some(AT_FLOOR_VERSION),
            true,
        ));
    }
    let local = make_identity(SEED_BASE + 998);
    let mut owner = PeerPoolOwner::new(true);
    owner.plan(&store, &local.hash, &[]);
    assert!(
        owner.peers().len() <= MAX_POOLED_PEERS,
        "the pool's population never grows past its ceiling"
    );
    let counters = *owner.counters();
    assert_eq!(
        counters.candidates_examined, 32,
        "the selection's own ceiling bounds the candidates the pool sees"
    );
    assert_eq!(
        counters.ceiling_refused, 17,
        "a candidate beyond the ceiling is refused and counted"
    );
    assert!(counters.budget_refused >= 1);
}

#[test]
fn the_pool_refuses_a_candidate_the_version_gate_refuses() {
    let peer = make_identity(SEED_BASE + 4);
    let mut store = RouterInfoStore::default();
    store.insert(validated_record(
        &peer,
        44_101,
        Some(FLOODFILL_CAPS),
        Some(BELOW_FLOOR_VERSION),
        true,
    ));
    let local = make_identity(SEED_BASE + 997);
    let mut owner = PeerPoolOwner::new(true);
    let planned = owner.plan(&store, &local.hash, &[]);
    assert!(
        planned.is_empty(),
        "a candidate the version gate refuses is never planned"
    );
    let counters = *owner.counters();
    assert_eq!(counters.candidates_examined, 1);
    assert_eq!(
        counters.candidates_refused, 1,
        "the version gate's refusal is counted, never skipped silently"
    );
    assert_eq!(counters.dials_started, 0);
}

#[test]
fn the_pool_counts_every_dial_outcome_under_its_own_reason() {
    let peer = make_identity(SEED_BASE + 5);
    let local = make_identity(SEED_BASE + 5 + 900);
    let store = store_with_one_floodfill(&peer, 44_101);
    let mut owner = PeerPoolOwner::new(true);
    owner.plan(&store, &local.hash, &[]);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Failed);
    owner.plan(&store, &local.hash, &[]);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Refused);
    owner.plan(&store, &local.hash, &[]);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Cancelled);
    owner.plan(&store, &local.hash, &[]);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Established);

    let counters = *owner.counters();
    assert_eq!(counters.dials_started, 4);
    assert_eq!(counters.dials_failed, 1);
    assert_eq!(counters.dials_refused, 1);
    assert_eq!(counters.dials_cancelled, 1);
    assert_eq!(counters.dials_established, 1);
    let by_reason = counters.counts_by_reason();
    assert_eq!(by_reason.get("dials-failed"), Some(&1));
    assert_eq!(by_reason.get("dials-refused"), Some(&1));
    assert_eq!(by_reason.get("dials-cancelled"), Some(&1));
    assert_eq!(by_reason.get("dials-established"), Some(&1));
}

#[test]
fn a_cancelled_dial_releases_the_peer() {
    let peer = make_identity(SEED_BASE + 6);
    let local = make_identity(SEED_BASE + 6 + 900);
    let store = store_with_one_floodfill(&peer, 44_101);
    let mut owner = PeerPoolOwner::new(true);
    owner.plan(&store, &local.hash, &[]);
    owner.record_dial_outcome(&peer.hash, PeerPoolDialOutcome::Cancelled);
    // The peer is released, so the next plan may dial it again.
    let planned = owner.plan(&store, &local.hash, &[]);
    assert_eq!(
        planned,
        vec![peer.hash],
        "a cancelled dial must not wedge the pool"
    );
    assert_eq!(owner.counters().dial_in_flight, 0);
}

// -------------------------------------------------- the pool against the wire

#[tokio::test]
async fn the_pool_establishes_a_session_to_a_peer_it_knows() {
    let config = Config::parse(POOL_ENABLED).expect("valid config").ssu2;

    // The peer the pool will hold a session to.
    let peer_b = make_identity(SEED_BASE + 10);
    let b = Fixture::start(&config, &peer_b).await;

    // The router that holds the pool.
    let peer_a = make_identity(SEED_BASE + 11);
    let a = Fixture::start(&config, &peer_a).await;

    // The record the router's store holds for the peer: the same identity,
    // with the address the peer actually bound.
    let record = validated_record(
        &peer_b,
        b.bound().port(),
        Some(FLOODFILL_CAPS),
        Some(AT_FLOOR_VERSION),
        true,
    );
    let harness = PoolHarness::start(
        vec![record],
        config.peer_pool,
        a.handle.publication_handle(),
        peer_a.hash,
        config.qualification_profile,
    );

    let counters = harness
        .wait_for(
            |counters| counters.dials_established >= 1,
            "the pool established its session",
        )
        .await;
    assert_eq!(counters.dials_started, 1);
    assert_eq!(counters.dials_failed, 0);
    assert_eq!(counters.dials_refused, 0);
    assert_eq!(
        counters.peers_admitted, 1,
        "the pool's own established dial admits the peer"
    );
    // The pool has not read its population again since the dial, so the
    // reuse of a held session is not counted yet. The reuse path itself is
    // proven by the owner-level row above.
    assert_eq!(counters.sessions_reused, 0);

    // The session is real: the dialer holds one link and the peer holds the
    // other end.
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while a.handle.snapshot().active_sessions < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the dialer never counted the session"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    while b.handle.snapshot().active_sessions < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the peer never counted the session"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // The pool names no peer in its receipt: the counters are the whole
    // content.
    let by_reason = counters.counts_by_reason();
    assert!(by_reason.contains_key("dials-established"));

    harness.shutdown().await;
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn the_pool_counts_a_peer_with_no_dialable_address() {
    let config = Config::parse(POOL_ENABLED).expect("valid config").ssu2;

    let peer_b = make_identity(SEED_BASE + 12);
    let b = Fixture::start(&config, &peer_b).await;
    let peer_a = make_identity(SEED_BASE + 13);
    let a = Fixture::start(&config, &peer_a).await;

    // The peer is an eligible floodfill, but it publishes **no** address, so
    // the transport's own scope refuses the target.
    let record = validated_record(
        &peer_b,
        b.bound().port(),
        Some(FLOODFILL_CAPS),
        Some(AT_FLOOR_VERSION),
        false,
    );
    let harness = PoolHarness::start(
        vec![record],
        config.peer_pool,
        a.handle.publication_handle(),
        peer_a.hash,
        config.qualification_profile,
    );

    let counters = harness
        .wait_for(
            |counters| counters.dials_failed + counters.dials_refused >= 1,
            "the pool refused the unaddressed peer",
        )
        .await;
    assert_eq!(counters.dials_started, 1);
    assert_eq!(counters.dials_established, 0);
    assert_eq!(
        counters.dials_failed + counters.dials_refused,
        1,
        "the refusal is counted under its own reason"
    );
    assert_eq!(a.handle.snapshot().active_sessions, 0);

    harness.shutdown().await;
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn a_router_with_the_opt_in_off_establishes_no_session() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config").ssu2;

    let peer_b = make_identity(SEED_BASE + 14);
    let b = Fixture::start(&config, &peer_b).await;
    let peer_a = make_identity(SEED_BASE + 15);
    let a = Fixture::start(&config, &peer_a).await;

    let record = validated_record(
        &peer_b,
        b.bound().port(),
        Some(FLOODFILL_CAPS),
        Some(AT_FLOOR_VERSION),
        true,
    );
    let harness = PoolHarness::start(
        vec![record],
        config.peer_pool,
        a.handle.publication_handle(),
        peer_a.hash,
        config.qualification_profile,
    );

    // The pool had a bounded window to plan a dial and did nothing.
    tokio::time::sleep(QUIET_WINDOW).await;
    let counters = harness.counters();
    assert_eq!(counters.population_reads, 0);
    assert_eq!(counters.dials_started, 0);
    assert_eq!(counters.dials_established, 0);
    assert_eq!(a.handle.snapshot().active_sessions, 0);
    assert_eq!(b.handle.snapshot().active_sessions, 0);

    harness.shutdown().await;
    a.shutdown().await;
    b.shutdown().await;
}

// -------------------------------------------------- the keepalive and the plane

#[test]
fn the_keepalive_reaches_the_runtime_only_when_the_pool_is_on() {
    let off = Config::parse(DEFAULT_CONFIG).expect("valid config").ssu2;
    let runtime_off =
        i2pr_daemon::router_i2np::ssu2_runtime_config_from(&off).expect("runtime config");
    assert!(
        runtime_off.deadlines.keepalive.is_none(),
        "a router without the pool sends no keepalive at all"
    );

    let on = Config::parse(POOL_ENABLED).expect("valid config").ssu2;
    let runtime_on =
        i2pr_daemon::router_i2np::ssu2_runtime_config_from(&on).expect("runtime config");
    assert_eq!(
        runtime_on.deadlines.keepalive,
        Some(Duration::from_millis(i2pr_runtime::KEEPALIVE_INTERVAL_MS)),
        "the keepalive's interval is the transport's own constant"
    );
}

#[test]
fn the_counters_reach_the_inspection_plane() {
    let inspection = InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    );
    let cell: Arc<PeerPoolCounterCell> = Arc::new(StdMutex::new(PeerPoolCounters::default()));
    inspection.publish_peer_pool_counters(Arc::clone(&cell));

    let counters = PeerPoolCounters {
        dials_started: 3,
        dials_established: 2,
        ..PeerPoolCounters::default()
    };
    *cell.lock().expect("counters") = counters;

    let snapshot = inspection
        .peer_pool_counters_snapshot()
        .expect("the counters are published");
    assert_eq!(snapshot.dials_started, 3);
    assert_eq!(snapshot.dials_established, 2);
    let by_reason = snapshot.counts_by_reason();
    assert_eq!(by_reason.get("dials-started"), Some(&3));
    assert_eq!(by_reason.get("dials-established"), Some(&2));
}
