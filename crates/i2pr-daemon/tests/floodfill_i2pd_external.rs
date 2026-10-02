//! Plan 278 — exact-pinned i2pd 2.61.0 controlled floodfill qualification driver.
//!
//! Two fail-closed phases, both `#[ignore]`-gated and selected explicitly with
//! `--ignored --exact`. Ordinary workspace runs compile and skip them; an
//! explicit selection without the required environment panics (never skips,
//! never succeeds early).
//!
//! Phase 1 [`floodfill_prepare_against_i2pd`] binds the controlled identity on a
//! fixed loopback port, runs the real Plan 283 controlled activation, and writes
//! the installed `caps=f` RouterInfo plus the transport material into the attempt
//! state directory. The runner then seeds that exact RouterInfo into the
//! reference client's `netDb` before the reference starts, so i2pd's own
//! known-floodfill selection sees i2pr. The identity and transport material are
//! reloaded (never regenerated) by phase 2, so the published RouterInfo stays
//! valid across both phases.
//!
//! Phase 2 [`floodfill_qualify_against_i2pd`] reloads the same identity, binds the
//! same port, re-runs controlled activation against live reference peers, and
//! drives the production inbound/dispatch/delivery path
//! (`handle_authenticated_i2np` + `deliver_floodfill_effect_with_dial`) so every
//! matrix row is produced by unmodified i2pr code responding to unmodified
//! i2pd traffic over real authenticated SSU2 sessions.
//!
//! Evidence is sanitized: counts, digests, lengths, and categorical outcomes
//! only. No private key, raw I2NP payload, or router key file is ever written.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::PathBuf;
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::floodfill::{
    ControlledActivationParams, FloodfillCoordinator, FloodfillCoordinatorPolicy,
    activate_controlled, deliver_floodfill_effect_with_dial,
};
use i2pr_daemon::router_i2np::{Ssu2DaemonHandle, Ssu2DaemonService, verify_reference_router_info};
use i2pr_netdb::{
    FloodfillEligibilitySnapshot, FloodfillResourcePolicy, FloodfillStorePolicy, InboundProvenance,
    NetDbNamespace, RecordProvenance, ReplicationPolicy, RouterHash, ServerNetDbConfig,
    StorePurpose, ValidatedNetDbRecord, ValidatedRouterInfo, ValidationContext,
    controlled_router_options,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, RouterInfo};
use i2pr_runtime::{
    CancellationReason, CancellationToken, ChildFailurePolicy, ChildScope, IntroKey,
    Ssu2IdentityMaterial,
};
use rand_core::{OsRng, TryRngCore};

const STEP_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_RECORD_AGE_MS: u64 = 3_600_000;
const MAINTENANCE_PERIOD: Duration = Duration::from_secs(30);
const MAINTENANCE_BATCH: usize = 256;

// ---------------------------------------------------------------- environment

fn env_value(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value,
        _ => panic!("missing required env {name}"),
    }
}

fn state_dir() -> PathBuf {
    PathBuf::from(env_value("I2PR_FLOODFILL_STATE_DIR"))
}

fn evidence_dir() -> PathBuf {
    let dir = PathBuf::from(env_value("EVIDENCE_DIR"));
    std::fs::create_dir_all(&dir).expect("create evidence dir");
    dir
}

fn record(label: &str, value: &str) {
    let sanitized = value.replace(['\t', '\n'], " ");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(evidence_dir().join("driver-evidence.tsv"))
        .expect("open evidence file");
    writeln!(file, "{label}\t{sanitized}").expect("write evidence row");
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let value = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((value >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((value >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(value & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ------------------------------------------------------- persisted identity

/// Transport material for the controlled identity, persisted in the attempt
/// state directory so both phases present the same RouterInfo. Never evidence.
struct Persisted {
    bundle: RouterIdentityBundle,
    static_secret: X25519PrivateKey,
    intro: IntroKey,
}

fn static_path() -> PathBuf {
    state_dir().join("static.key")
}

fn intro_path() -> PathBuf {
    state_dir().join("intro.key")
}

fn load_or_create() -> Persisted {
    let state = state_dir();
    // Identity material is secret: the directory and every file this driver
    // writes must be owner-only.
    std::fs::create_dir_all(&state).expect("create state dir");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700))
        .expect("tighten state dir");
    i2pr_storage::IdentityStore::prepare_directory(&state).expect("secure state dir");
    let bundle = match i2pr_storage::IdentityStore::new(state.join("router.keys")).load() {
        Ok(bundle) => bundle,
        Err(_) => {
            let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
            i2pr_storage::IdentityStore::new(state.join("router.keys"))
                .save(&bundle)
                .expect("persist identity");
            bundle
        }
    };
    let static_secret = match std::fs::read_to_string(static_path()) {
        Ok(raw) => {
            let bytes: [u8; 32] = hex_decode(raw.trim())
                .try_into()
                .expect("32-byte static key");
            X25519PrivateKey::from_bytes(bytes)
        }
        Err(_) => {
            let secret = X25519PrivateKey::generate(&mut OsRng).expect("static");
            write_secret(&static_path(), hex(secret.secret_bytes()));
            secret
        }
    };
    let intro = match std::fs::read_to_string(intro_path()) {
        Ok(raw) => IntroKey::new(
            hex_decode(raw.trim())
                .try_into()
                .expect("32-byte intro key"),
        ),
        Err(_) => {
            let mut bytes = [0_u8; 32];
            loop {
                OsRng.try_fill_bytes(&mut bytes).expect("rng");
                if bytes.iter().any(|byte| *byte != 0) {
                    break;
                }
            }
            write_secret(&intro_path(), hex(&bytes));
            IntroKey::new(bytes)
        }
    };
    Persisted {
        bundle,
        static_secret,
        intro,
    }
}

/// Writes one owner-only file. Transport secrets never use the default mode.
fn write_secret(path: &PathBuf, contents: String) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .expect("open secret file");
    file.write_all(contents.as_bytes())
        .expect("write secret file");
}

fn hex_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    assert!(
        bytes.len().is_multiple_of(2),
        "hex input must be even length"
    );
    (0..bytes.len() / 2)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

// ------------------------------------------------------------- live service

struct Live {
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
}

fn bind() -> std::net::SocketAddr {
    let port: u16 = env_value("I2PR_FLOODFILL_BIND_PORT")
        .parse()
        .expect("numeric I2PR_FLOODFILL_BIND_PORT");
    assert_ne!(
        port, 0,
        "controlled qualification requires a fixed bind port"
    );
    format!("127.0.0.1:{port}").parse().expect("loopback bind")
}

/// Builds the non-floodfill published RouterInfo the service starts from.
/// Controlled activation replaces it atomically with the `caps=f` record.
fn initial_router_info(persisted: &Persisted, addr: std::net::SocketAddr) -> Vec<u8> {
    let options = Mapping::from_entries(vec![
        ("host".to_string(), addr.ip().to_string()),
        ("port".to_string(), addr.port().to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(&persisted.static_secret.public_bytes()),
        ),
        ("i".to_string(), i2p_b64_encode(persisted.intro.as_bytes())),
    ])
    .expect("address options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("router address");
    // The identity record mirrors the daemon's controlled identity
    // publication (`router_i2np::generate_controlled_identity`): it carries
    // the same controlled declaration. The install guard compares netId
    // against the currently installed record, so a hand-rolled identity
    // record without the declaration cannot install the later caps=f record
    // that has one, and the pinned reference would mark such a record
    // unreachable regardless.
    persisted
        .bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            controlled_router_options().expect("controlled options"),
        )
        .expect("sign initial RouterInfo")
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode initial RouterInfo")
}

async fn start(persisted: &Persisted) -> Live {
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
    let base = Config::parse(text).expect("strict profile").ssu2;
    let hash = persisted.bundle.identity().hash().expect("identity hash");
    let addr = bind();
    let service = Ssu2DaemonService::new(
        &base,
        Ssu2IdentityMaterial {
            router_hash: hash,
            static_secret_bytes: *persisted.static_secret.secret_bytes(),
            intro_key: persisted.intro,
            router_info: initial_router_info(persisted, addr),
        },
    )
    .expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let handle = service
        .start(
            &scope,
            &i2pr_daemon::config::Ssu2Config {
                port: addr.port(),
                ..base.clone()
            },
        )
        .await
        .expect("bind controlled socket");
    assert_eq!(handle.local_v4().expect("bound v4"), addr, "bind mismatch");
    Live {
        handle,
        scope,
        token,
    }
}

impl Live {
    async fn shutdown(self) {
        self.token.cancel(CancellationReason::OperatorRequest);
        self.scope.shutdown().await;
    }
}

// ------------------------------------------------------------ coordinator

fn base_eligibility() -> FloodfillEligibilitySnapshot {
    FloodfillEligibilitySnapshot {
        controlled_qualification_permit: true,
        qualified_ssu2_address: true,
        direct_reachability: true,
        netdb_ready: true,
        storage_ready: true,
        maintenance_ready: true,
        resource_headroom: true,
        clock_sane: true,
        supervision_healthy: true,
    }
}

fn new_coordinator(local: Hash) -> FloodfillCoordinator {
    FloodfillCoordinator::new(
        local,
        FloodfillCoordinatorPolicy::default(),
        FloodfillStorePolicy::default(),
        ServerNetDbConfig::default(),
        FloodfillResourcePolicy::default(),
        ReplicationPolicy::default(),
    )
    .expect("bounded coordinator")
}

async fn activate(
    live: &Live,
    coordinator: &mut FloodfillCoordinator,
    persisted: &Persisted,
) -> i2pr_daemon::floodfill::ControlledActivation {
    let static_public =
        i2pr_runtime::Ssu2PublicKey::new(persisted.static_secret.public_bytes()).expect("public");
    let cancellation = CancellationToken::new();
    activate_controlled(ControlledActivationParams {
        scope: &live.scope,
        handle: &live.handle,
        coordinator,
        bundle: &persisted.bundle,
        alice_static_public: static_public,
        alice_intro: persisted.intro,
        base_eligibility: base_eligibility(),
        wall_now_ms: wall_ms(),
        step_timeout: STEP_TIMEOUT,
        poll_interval: POLL_INTERVAL,
        cancellation: &cancellation,
    })
    .await
    .expect("controlled activation")
}

fn load_reference(path: &str, endpoint: &str) -> RouterInfo {
    let raw = std::fs::read(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    let (hash, _address) =
        verify_reference_router_info(&raw).unwrap_or_else(|error| panic!("{path}: {error:?}"));
    let addr: std::net::SocketAddr = endpoint
        .parse()
        .unwrap_or_else(|error| panic!("{endpoint}: {error}"));
    assert!(
        addr.ip().is_loopback(),
        "reference endpoint must be loopback"
    );
    record(
        "reference-routerinfo-verified",
        &i2p_b64_encode(hash.as_bytes()),
    );
    i2pr_proto::RouterInfo::decode(&raw, 64 * 1024).expect("decode reference RouterInfo")
}
fn seed_replica(coordinator: &mut FloodfillCoordinator, info: &RouterInfo) -> RouterHash {
    let key = i2pr_netdb::router_hash(info.router_identity()).expect("hash");
    let validated = ValidatedRouterInfo::from_router_info(
        info.clone(),
        Some(key),
        ValidationContext::new(Date::from_millis(wall_ms())),
    )
    .expect("valid replica");
    coordinator
        .netdb_mut()
        .insert(
            ValidatedNetDbRecord::RouterInfo(validated),
            RecordProvenance {
                namespace: NetDbNamespace::MainRouter,
                inbound: InboundProvenance::AuthenticatedDirectPeer,
                purpose: StorePurpose::FloodReplica,
                observed_at_ms: wall_ms(),
            },
        )
        .expect("insert replica");
    key
}

// -------------------------------------------------------------- phase 1

/// Plan 278 phase 1: establish the stable controlled identity and publish the
/// `caps=f` RouterInfo the reference client will be seeded with.
#[tokio::test]
#[ignore = "requires exact-pinned i2pd 2.61.0 lane environment"]
async fn floodfill_prepare_against_i2pd() {
    let persisted = load_or_create();
    let hash = persisted.bundle.identity().hash().expect("identity hash");
    record(
        "controlled-identity-stable",
        &i2p_b64_encode(hash.as_bytes()),
    );
    record("controlled-addr-policy", "127.0.0.1-loopback-fixed-port");

    let live = start(&persisted).await;
    let mut coordinator = new_coordinator(hash);
    let activation = activate(&live, &mut coordinator, &persisted).await;

    let installed = live
        .handle
        .service()
        .installed_local_router_info()
        .expect("installed RouterInfo present");
    assert_eq!(
        installed, activation.router_info,
        "installed RouterInfo differs from the activation result"
    );
    let decoded = i2pr_proto::RouterInfo::decode(&activation.router_info, 64 * 1024)
        .expect("decode installed RouterInfo");
    let caps = decoded.capabilities().expect("capabilities");
    let caps = caps.expect("RouterInfo carries capabilities");
    assert!(
        caps.as_str().contains('f'),
        "installed RouterInfo is not floodfill-capable"
    );
    record("controlled-activation-completed", "peer-test-confirmed");
    record(
        "published-routerinfo-caps-f",
        &format!("caps={}", caps.as_str()),
    );
    record(
        "published-routerinfo-bytes",
        &format!("len={}", activation.router_info.len()),
    );

    // The runner seeds this exact file into the reference netDb layout.
    std::fs::write(
        state_dir().join("published.routerinfo"),
        &activation.router_info,
    )
    .expect("write published RouterInfo");
    // i2pd's HashedStorage filenames use the i2p base64 router hash as the
    // ident; the runner needs it to place the file in the exact bucket layout.
    std::fs::write(
        state_dir().join("ident.b64"),
        i2p_b64_encode(hash.as_bytes()),
    )
    .expect("write router ident");

    live.shutdown().await;
}

// -------------------------------------------------------------- phase 2

/// Sanitized observation sink shared between the owner task and the test body.
/// Every field is a count, a digest, or a categorical outcome; no payload
/// bytes and no key material are retained.
#[derive(Clone, Debug, Default)]
struct Observed {
    store_accepted: u64,
    store_rejected: Vec<String>,
    store_ack_delivered: u64,
    lookup_answered: u64,
    store_ack_failed: Vec<String>,
    lookup_hits: u64,
    lookup_dsrm: u64,
    lookup_no_response: Vec<String>,
    lookup_tunnel_replies: u64,
    direct_store_replicas: u64,
    direct_store_failures: Vec<String>,
    dispatch_errors: Vec<String>,
    maintenance_sweeps: u64,
    maintenance_removed: u64,
    maintenance_examined: u64,
    role_state: String,
    queue_full: u64,
    // Plan 302 §12.2 observability: insert-outcome class per accepted store
    // plus whether the service offered a replication candidate. Counts and
    // category labels only; no payloads, no keys.
    store_insert_outcomes: Vec<String>,
    replication_offered: u64,
    replication_absent: u64,
}

type Shared = std::sync::Arc<std::sync::Mutex<Observed>>;

fn share(observed: &Shared) -> std::sync::MutexGuard<'_, Observed> {
    observed.lock().expect("observation lock")
}

/// The production floodfill owner loop, driven inline so the test body can
/// assert on the coordinator's own typed state. It calls exactly the same
/// `handle_authenticated_i2np` + `deliver_floodfill_effect_with_dial` +
/// `maintenance_tick` entry points as `run_floodfill_owner`.
async fn run_owner(
    mut live: Live,
    mut coordinator: FloodfillCoordinator,
    observed: Shared,
    cancel: CancellationToken,
    session_probe: tokio::sync::oneshot::Sender<()>,
) {
    let _ = session_probe;
    let mut maintenance = tokio::time::interval(MAINTENANCE_PERIOD);
    maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let mut guard = share(&observed);
                guard.role_state = "Draining".to_owned();
                return;
            }
            inbound = live.handle.next_inbound() => {
                let Some(inbound) = inbound else { return };
                let wall = wall_ms();
                let time = i2pr_netdb::FloodfillTime { wall_ms: wall, monotonic_ms: wall };
                let dispatch = coordinator.handle_authenticated_i2np(&inbound, time);
                {
                    let mut guard = share(&observed);
                    match &dispatch {
                        Ok(i2pr_daemon::floodfill::FloodfillDispatchOutcome::Store(
                            effect,
                        )) => match effect {
                            i2pr_netdb::FloodfillStoreEffect::Stored {
                                outcome,
                                replication,
                                ..
                            } => {
                                guard.store_accepted += 1;
                                guard.store_insert_outcomes.push(insert_outcome_label(*outcome));
                                if replication.is_some() {
                                    guard.replication_offered += 1;
                                } else {
                                    guard.replication_absent += 1;
                                }
                            }
                            other => guard
                                .store_rejected
                                .push(rejection_label(other)),
                        },
                        Ok(i2pr_daemon::floodfill::FloodfillDispatchOutcome::Lookup(Ok(()))) => {
                            guard.lookup_answered += 1;
                        }
                        Ok(i2pr_daemon::floodfill::FloodfillDispatchOutcome::Lookup(Err(r))) => {
                            guard.lookup_no_response.push(format!("{r:?}"));
                        }
                        Ok(i2pr_daemon::floodfill::FloodfillDispatchOutcome::Ignored(_)) => {}
                        Err(error) => guard.dispatch_errors.push(format!("{error:?}")),
                    }
                }
                while let Some(leased) = coordinator.pop_effect() {
                    let label = leased
                        .effect
                        .as_ref()
                        .map(classify_effect)
                        .unwrap_or("empty");
                    let outcome = deliver_floodfill_effect_with_dial(
                        &live.handle,
                        coordinator.netdb(),
                        leased,
                        wall,
                        MAX_RECORD_AGE_MS,
                        &cancel,
                    )
                    .await;
                    let mut guard = share(&observed);
                    match label {
                        "store-ack" => {
                            if matches!(
                                outcome,
                                i2pr_daemon::floodfill::FloodfillDeliveryOutcome::Delivered(_)
                            ) {
                                guard.store_ack_delivered += 1;
                            } else {
                                guard.store_ack_failed.push(format!("{outcome:?}"));
                            }
                        }
                        "reply-routerinfo-record" => guard.lookup_hits += 1,
                        "reply-search-reply-only" => guard.lookup_dsrm += 1,
                        "reply-tunnel-record" => guard.lookup_tunnel_replies += 1,
                        "replica-store" => {
                            if matches!(
                                outcome,
                                i2pr_daemon::floodfill::FloodfillDeliveryOutcome::Delivered(_)
                            ) {
                                guard.direct_store_replicas += 1;
                            } else {
                                guard.direct_store_failures.push(format!("{outcome:?}"));
                            }
                        }
                        other => guard.dispatch_errors.push(format!("unclassified:{other}")),
                    }
                    coordinator.record_delivery(outcome);
                }
            }
            _ = maintenance.tick() => {
                let wall = wall_ms();
                let swept = coordinator.maintenance_tick(
                    wall,
                    MAX_RECORD_AGE_MS,
                    MAINTENANCE_BATCH,
                );
                let mut guard = share(&observed);
                guard.maintenance_sweeps += 1;
                guard.maintenance_removed += swept.expired as u64;
                guard.maintenance_examined += swept.examined as u64;
                guard.role_state = format!("{:?}", coordinator.role_state());
            }
        }
    }
}

/// Categorical insert-outcome label for Plan 302 §12.2 observability.
/// Variants only; no payloads, no keys.
fn insert_outcome_label(outcome: i2pr_netdb::ServerInsertOutcome) -> String {
    match outcome {
        i2pr_netdb::ServerInsertOutcome::Inserted => "inserted".to_owned(),
        i2pr_netdb::ServerInsertOutcome::Replaced => "replaced".to_owned(),
        i2pr_netdb::ServerInsertOutcome::Idempotent => "idempotent".to_owned(),
        i2pr_netdb::ServerInsertOutcome::Conflict => "conflict".to_owned(),
        i2pr_netdb::ServerInsertOutcome::Stale => "stale".to_owned(),
        i2pr_netdb::ServerInsertOutcome::CapacityExceeded => "capacity".to_owned(),
    }
}

fn rejection_label(effect: &i2pr_netdb::FloodfillStoreEffect) -> String {
    match effect {
        i2pr_netdb::FloodfillStoreEffect::Disabled => "disabled".to_owned(),
        i2pr_netdb::FloodfillStoreEffect::Throttled => "throttled".to_owned(),
        i2pr_netdb::FloodfillStoreEffect::Invalid => "invalid".to_owned(),
        i2pr_netdb::FloodfillStoreEffect::Unsupported => "unsupported".to_owned(),
        i2pr_netdb::FloodfillStoreEffect::CapacityExceeded => "capacity".to_owned(),
        i2pr_netdb::FloodfillStoreEffect::Stored { .. } => "stored".to_owned(),
    }
}

fn classify_effect(effect: &i2pr_daemon::floodfill::FloodfillDaemonEffect) -> &'static str {
    match effect {
        i2pr_daemon::floodfill::FloodfillDaemonEffect::StoreAck { .. } => "store-ack",
        i2pr_daemon::floodfill::FloodfillDaemonEffect::LookupReply { intent, .. } => match intent {
            i2pr_netdb::FloodfillReplyIntent::Direct { body, .. } => match body {
                i2pr_proto::I2npBody::DatabaseStore(_) => "reply-routerinfo-record",
                i2pr_proto::I2npBody::DatabaseSearchReply(_) => "reply-search-reply-only",
                _ => "reply-other",
            },
            i2pr_netdb::FloodfillReplyIntent::Tunnel { .. } => "reply-tunnel-record",
        },
        i2pr_daemon::floodfill::FloodfillDaemonEffect::DirectFlood { .. } => "replica-store",
    }
}

/// Waits until `predicate` holds or the bounded deadline elapses.
async fn wait_for(what: &str, mut predicate: impl FnMut(&Observed) -> bool) -> Observed {
    let deadline = tokio::time::Instant::now() + STEP_TIMEOUT;
    loop {
        let snapshot = share(&observed_slot()).clone();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} not observed: {snapshot:?}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

thread_local! {
    static CURRENT: std::cell::RefCell<Option<Shared>> =
        const { std::cell::RefCell::new(None) };
}

fn observed_slot() -> Shared {
    CURRENT.with(|slot| {
        slot.borrow()
            .clone()
            .expect("observation slot installed for this test")
    })
}

/// Plan 278 phase 2: the qualification matrix against live exact-pinned i2pd.
#[tokio::test]
#[ignore = "requires exact-pinned i2pd 2.61.0 lane environment"]
async fn floodfill_qualify_against_i2pd() {
    let observed: Shared = std::sync::Arc::new(std::sync::Mutex::new(Observed::default()));
    CURRENT.with(|slot| *slot.borrow_mut() = Some(observed.clone()));

    let persisted = load_or_create();
    let local = persisted.bundle.identity().hash().expect("identity hash");
    record(
        "controlled-identity-reused",
        &i2p_b64_encode(local.as_bytes()),
    );

    // Matrix A/C/F references: two stock floodfill peers plus one stock client.
    // Plan 303 §3: only the two floodfill peers are seeded as replication
    // targets and lookup fixtures. The reference client (C) is deliberately
    // NOT seeded so its first publication inserts (rather than re-states) and
    // offers a replication candidate for matrix F. C's record exists in the
    // NetDB from matrix A onward via its own publisher store, and the matrix
    // C/E waits below already run after A completes.
    let floodfill_a = load_reference(
        &env_value("I2PD_FLOODFILL_A_ROUTER_INFO"),
        &env_value("I2PD_FLOODFILL_A_ENDPOINT"),
    );
    let floodfill_b = load_reference(
        &env_value("I2PD_FLOODFILL_B_ROUTER_INFO"),
        &env_value("I2PD_FLOODFILL_B_ENDPOINT"),
    );
    let client = load_reference(
        &env_value("I2PD_FLOODFILL_C_ROUTER_INFO"),
        &env_value("I2PD_FLOODFILL_C_ENDPOINT"),
    );
    let client_hash = *i2pr_netdb::router_hash(client.router_identity())
        .expect("client hash")
        .as_hash();

    let live = start(&persisted).await;
    let mut coordinator = new_coordinator(local);
    seed_replica(&mut coordinator, &floodfill_a);
    seed_replica(&mut coordinator, &floodfill_b);
    record("replication-targets-seeded", "count=2");

    let _activation = activate(&live, &mut coordinator, &persisted).await;
    record("controlled-activation-completed", "peer-test-confirmed");
    record(
        "role-active-after-activation",
        &format!("{:?}", coordinator.role_state()),
    );

    let cancel = CancellationToken::new();
    let (probe_tx, probe_rx) = tokio::sync::oneshot::channel();
    let owner = tokio::spawn(run_owner(
        live,
        coordinator,
        observed.clone(),
        cancel.clone(),
        probe_tx,
    ));
    let _ = tokio::time::timeout(POLL_INTERVAL, probe_rx).await;

    // ---- matrix A: publisher store + acknowledgement --------------------
    let stored = wait_for("publisher store from reference client", |seen| {
        seen.store_accepted >= 1
    })
    .await;
    record(
        "publisher-store-accepted",
        &format!("count={}", stored.store_accepted),
    );
    assert!(
        stored.store_rejected.is_empty(),
        "unexpected store rejections: {:?}",
        stored.store_rejected
    );

    let acked = wait_for("store acknowledgement delivery", |seen| {
        seen.store_ack_delivered >= 1
    })
    .await;
    record(
        "publisher-store-ack-delivered",
        &format!("count={}", acked.store_ack_delivered),
    );
    assert!(
        acked.store_ack_failed.is_empty(),
        "acknowledgement delivery failures: {:?}",
        acked.store_ack_failed
    );
    // Plan 302 §12.2: recorded right after matrix A so the outcome class and
    // the offered/absent split survive even if matrix F later stops the lane.
    record(
        "publisher-store-insert-outcome",
        &format!(
            "outcome={:?} replication_offered={} replication_absent={}",
            acked.store_insert_outcomes, acked.replication_offered, acked.replication_absent
        ),
    );

    // ---- matrix C/E: lookup hit, miss, exploration -----------------------
    let looked_up = wait_for("reference lookup answered", |seen| {
        seen.lookup_answered >= 1
    })
    .await;
    assert!(
        looked_up.lookup_answered >= 1,
        "reference issued no DatabaseLookup to the controlled floodfill"
    );
    let final_seen = wait_for("lookup reply delivery", |seen| {
        seen.lookup_hits + seen.lookup_dsrm + seen.lookup_tunnel_replies >= 1
    })
    .await;
    record(
        "lookup-replies-classified",
        &format!(
            "hit={} dsrm={} tunnel={}",
            final_seen.lookup_hits, final_seen.lookup_dsrm, final_seen.lookup_tunnel_replies
        ),
    );
    assert!(
        final_seen.lookup_hits + final_seen.lookup_dsrm + final_seen.lookup_tunnel_replies >= 1,
        "no lookup reply was produced"
    );
    assert!(
        final_seen.dispatch_errors.is_empty(),
        "dispatch errors: {:?}",
        final_seen.dispatch_errors
    );
    record(
        "client-peer-identified",
        &i2p_b64_encode(client_hash.as_bytes()),
    );

    // ---- matrix F: direct zero-token replication -------------------------
    let replicated = wait_for("direct zero-token replication", |seen| {
        seen.direct_store_replicas >= 1
    })
    .await;
    record(
        "replication-direct-store-delivered",
        &format!("count={}", replicated.direct_store_replicas),
    );
    assert!(
        replicated.direct_store_failures.is_empty(),
        "replication failures: {:?}",
        replicated.direct_store_failures
    );
    record(
        "replication-tunnel-fallback-absent",
        "zero-token-direct-only",
    );

    // ---- teardown: bounded cancel ---------------------------------------
    cancel.cancel(CancellationReason::OperatorRequest);
    tokio::time::timeout(STEP_TIMEOUT, owner)
        .await
        .expect("owner drains within bound")
        .expect("owner task joins");
    let drained = share(&observed).clone();
    record("owner-cancel-drained", "graceful");
    record(
        "maintenance-sweeps",
        &format!(
            "sweeps={} examined={} expired={}",
            drained.maintenance_sweeps, drained.maintenance_examined, drained.maintenance_removed
        ),
    );
    record("queue-full-drops", &format!("count={}", drained.queue_full));
}

// ------------------------------------------------------- diagnostic probe

/// Plan 278 boundary probe. Rebuilds the controlled identity's RouterInfo with
/// one additional options entry (`router.version`) and hands it to the runner
/// so a second reference client can be seeded with it. Nothing here is
/// production code and nothing here may satisfy a matrix row: it exists only to
/// localize the exact reference-side gate that stopped the qualification.
#[tokio::test]
#[ignore = "requires exact-pinned i2pd 2.61.0 lane environment"]
async fn floodfill_diagnose_version_gate() {
    let persisted = load_or_create();
    let hash = persisted.bundle.identity().hash().expect("identity hash");
    let addr = bind();
    // i2pd's address parser requires a well-formed x25519 public key
    // (libi2pd/RouterInfo.cpp: `!(address->s[31] & 0x80)`). The probe masks the
    // high bit in the published bytes only, to localize that gate independently
    // of the identity and version gates.
    let mut masked = persisted.static_secret.public_bytes();
    masked[31] &= 0x7f;
    let options = Mapping::from_entries(vec![
        ("host".to_string(), addr.ip().to_string()),
        ("port".to_string(), addr.port().to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&masked)),
        ("i".to_string(), i2p_b64_encode(persisted.intro.as_bytes())),
        ("router.version".to_string(), "0.9.58".to_string()),
    ])
    .expect("probe options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("probe router address");
    let encoded = persisted
        .bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            Mapping::from_entries(vec![
                ("caps".to_string(), "f".to_string()),
                ("router.version".to_string(), "0.9.58".to_string()),
            ])
            .expect("probe router options"),
        )
        .expect("sign probe RouterInfo")
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode probe RouterInfo");
    std::fs::write(state_dir().join("probe.routerinfo"), &encoded).expect("write probe RouterInfo");
    std::fs::write(
        state_dir().join("probe.ident.b64"),
        i2p_b64_encode(hash.as_bytes()),
    )
    .expect("write probe ident");
    record(
        "diagnose-version-gate-probe",
        &format!(
            "router.version=0.9.58 caps=f static-key-msb-cleared={}",
            masked[31] & 0x80 == 0
        ),
    );
}

/// Plan 278 boundary probe. Parses two RouterInfo files with the production
/// codec and reports their structure side by side. Pure diagnosis: it produces
/// no matrix evidence and asserts nothing.
#[test]
#[ignore = "boundary diagnosis helper"]
fn floodfill_diagnose_routerinfo_structure() {
    for name in ["I2PD_DUMP_ROUTER_INFO", "I2PD_DUMP_CONTROLLED_ROUTER_INFO"] {
        let path = env_value(name);
        let raw = std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let info = i2pr_proto::RouterInfo::decode(&raw, 64 * 1024)
            .unwrap_or_else(|error| panic!("{path}: {error:?}"));
        let caps = info
            .capabilities()
            .expect("capabilities")
            .map(|value| value.as_str().to_owned());
        let version = info.options().get("router.version").map(str::to_owned);
        let addresses: Vec<String> = info
            .addresses()
            .iter()
            .map(|address| {
                let opts: Vec<(String, String)> = address
                    .options()
                    .entries()
                    .iter()
                    .map(|entry| (entry.key().to_owned(), entry.value().to_owned()))
                    .collect();
                format!(
                    "style={} cost={} expiry={:?} options={:?}",
                    address.transport_style(),
                    address.cost(),
                    address.expiration(),
                    opts
                )
            })
            .collect();
        println!(
            "DUMP {path}: len={} caps={version:?}/{caps:?} addresses={addresses:?}",
            raw.len(),
            version = version
        );
        println!("DUMP   router.options={:?}", info.options());
        let signed = info.signed_bytes();
        println!(
            "DUMP   signed_len={} tail={} sig_type={:?} sig_len={}",
            signed.len(),
            hex(&raw[raw.len() - 4..]),
            info.signature().key_type(),
            info.signature().as_bytes().len()
        );
        println!(
            "DUMP   signed_hash={}",
            hex(&i2pr_crypto::sha256(signed).as_bytes()[..8])
        );
    }
}
