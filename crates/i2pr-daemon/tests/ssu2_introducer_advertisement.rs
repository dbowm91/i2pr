//! Plan 526's lane — the router advertises its own introducer role.
//!
//! The specification's Relay capability is advertised with a `"C"` in the
//! address's `caps` option: "Relay — As advertised with a `"C"` in the
//! `"caps"` parameter", and the option's alphabet is `[B,C,4,6]`. The pinned
//! i2pd 2.61.0 publishes that letter on the **published** form only
//! (`libi2pd/RouterInfo.cpp:1350`-`:1373`), and its peer-side check demands
//! an endpoint for an introducer (`libi2pd/RouterInfo.cpp:1059`).
//!
//! The lane drives the **public** behaviour through the daemon's own owner
//! and the runtime's own live service:
//!
//! 1. the introducer role's switch is **off** by default, and this plan adds
//!    **no** new configuration key;
//! 2. the runtime's role is live **only** behind the explicit opt-in;
//! 3. the capability the router publishes carries the role letter **only**
//!    while the role is live, and the letter rides the family letter;
//! 4. the **introducer-only** form never carries the role letter, even for a
//!    router whose role is live and which holds introducers;
//! 5. the netdb's **third** rule accepts the role-bearing address and the
//!    **first** two rules refuse it;
//! 6. the netdb's **third** builder signs the role-bearing address and the
//!    **first** two builders refuse it;
//! 7. the daemon publishes the role-bearing record through the third builder
//!    and counts it under its own reason;
//! 8. the runtime's install gate **refuses** a role-bearing record while the
//!    role is not live, so a stale letter cannot reach the wire;
//! 9. the **composition** reaches the role owner behind the opt-in: the
//!    router starts with the role enabled, which it could not do before this
//!    plan, because the composition refused the role outright.
//!
//! Assertions are counts, shapes and digests only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::router_i2np::{Ssu2DaemonHandle, Ssu2DaemonService};
use i2pr_daemon::self_publication::{
    SelfPublicationCounterSnapshot, SelfPublicationOutcome, SelfPublicationOwner,
};
use i2pr_netdb::{
    LocalRouterInfoBuilder, is_qualified_ssu2_address, is_qualified_ssu2_introducer_address,
    is_qualified_ssu2_relay_role_address,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, RouterInfo};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2IdentityMaterial,
    Ssu2PublicationForm, Ssu2PublicationMaterial,
};
use i2pr_transport::PeerId;
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long the black-box composition row holds the router up before it
/// interrupts it.
const COMPOSITION_HOLD: Duration = Duration::from_secs(3);

/// Upper bound on how long the composition row waits for the child to exit
/// on its own.
const COMPOSITION_EXIT_DEADLINE: Duration = Duration::from_secs(20);

/// The introducer role's switch is off by default, so the default tree
/// changes in no way.
const DEFAULT_CONFIG: &str =
    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";

/// The router's own posture: it publishes its own SSU2 address.
const PUBLISH_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nadvertise = true\n";

/// The router's own posture, with the introducer role **on**: it serves the
/// role and therefore advertises it.
const ROLE_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nadvertise = true\nintroducer_service = true\n";

/// The peer's own posture: it is willing to **be** an introducer, so it
/// answers a relay-tag request.
const INTRODUCER_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nintroducer_service = true\n";

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
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

/// One test router's identity material, its signed record and the signing
/// bundle the daemon's owner needs.
struct RouterKeys {
    bundle: RouterIdentityBundle,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: i2pr_runtime::Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
}

/// Builds one router's identity material and its published record.
fn make_router_keys() -> RouterKeys {
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
    entries.sort();
    let options = Mapping::from_entries(entries).expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    // The initial record carries the **controlled** options, so the
    // install gate's own `netId` check compares like with like when the
    // owner replaces the record.
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            i2pr_netdb::controlled_router_options().expect("options"),
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    RouterKeys {
        bundle,
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
    }
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
        PeerId::from_hash(self.keys.hash)
    }

    /// The router's own ordinary record, naming the **bound** endpoint, as
    /// the composition builds it after the bind.
    ///
    /// The capability string is the caller's, so the lane can install a
    /// role-bearing record and a plain one from the same fixture.
    fn record_with_caps(&self, caps: Option<&str>, now: u64) -> Vec<u8> {
        let mut entries = vec![
            ("host".to_string(), self.bound().ip().to_string()),
            ("port".to_string(), self.bound().port().to_string()),
            ("v".to_string(), "2".to_string()),
            (
                "s".to_string(),
                i2p_b64_encode(self.keys.static_public.as_bytes()),
            ),
            ("i".to_string(), i2p_b64_encode(self.keys.intro.as_bytes())),
        ];
        if let Some(caps) = caps {
            entries.push(("caps".to_string(), caps.to_string()));
        }
        entries.sort();
        let options = Mapping::from_entries(entries).expect("options");
        let address = RouterAddress::new(10, Date::from_millis(now), "SSU2".to_string(), options)
            .expect("address");
        let router_options = i2pr_netdb::controlled_router_options().expect("options");
        let info = self
            .keys
            .bundle
            .sign_router_info(
                Date::from_millis(now),
                vec![address],
                Vec::new(),
                router_options,
            )
            .expect("sign");
        info.encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode")
    }

    /// Installs the router's own ordinary record, as the composition does.
    fn install_plain(&self, now: u64) -> Vec<u8> {
        let record = self.record_with_caps(None, now);
        self.handle
            .service()
            .install_local_router_info(record.clone(), now)
            .expect("plain record installed");
        record
    }

    /// The material the router would publish for itself, with the given
    /// capability string and the given role claim.
    ///
    /// The address names the router's **own** bound endpoint and the
    /// router's **own** keys, so the runtime's install gate admits it.
    fn material(&self, caps: &str, advertises_role: bool, now: u64) -> Ssu2PublicationMaterial {
        let mut entries = vec![
            ("caps".to_string(), caps.to_string()),
            ("host".to_string(), self.bound().ip().to_string()),
            ("i".to_string(), i2p_b64_encode(self.keys.intro.as_bytes())),
            ("mtu".to_string(), "1280".to_string()),
            ("port".to_string(), self.bound().port().to_string()),
            (
                "s".to_string(),
                i2p_b64_encode(self.keys.static_public.as_bytes()),
            ),
            ("v".to_string(), "2".to_string()),
        ];
        entries.sort();
        let address = RouterAddress::new(
            10,
            Date::from_millis(now),
            "SSU2".to_string(),
            Mapping::from_entries(entries).expect("options"),
        )
        .expect("address");
        Ssu2PublicationMaterial {
            address,
            form: Ssu2PublicationForm::Direct,
            advertises_introducer_role: advertises_role,
            evidence_expires_at: Duration::from_secs(3_600),
            reachability: i2pr_transport::ReachabilityState::Reachable,
            bound_family: i2pr_transport::AddressFamily::Ipv4,
        }
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

/// Waits until the router has learned exactly one introducer.
async fn wait_for_introducers(handle: &Ssu2DaemonHandle, issuer: &Hash) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let learned = handle.service().learned_introducers(wall_ms() / 1_000);
        if learned.len() == 1 {
            assert_eq!(
                learned[0].peer_hash(),
                issuer.as_bytes(),
                "the introducer must be bound to the peer that issued the tag"
            );
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the router learned {} introducers, expected 1",
            learned.len()
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Starts two daemon-owned services and establishes a session between them.
async fn start_pair(router: &str, peer: &str) -> (Fixture, Fixture) {
    let a = Fixture::start(&daemon_ssu2_config(router), make_router_keys()).await;
    let b = Fixture::start(&daemon_ssu2_config(peer), make_router_keys()).await;
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

/// The decoded addresses of a record.
fn addresses_of(encoded: &[u8]) -> Vec<RouterAddress> {
    let info = RouterInfo::decode(encoded, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    info.addresses().to_vec()
}

// ---------------------------------------------------------------- the switch

#[test]
fn the_introducer_role_switch_stays_off_by_default() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config");
    assert!(
        !config.ssu2.introducer_service,
        "the introducer role's switch must default to off"
    );
    assert!(
        !config.ssu2.advertise,
        "the publication switch must default to off"
    );
}

#[test]
fn the_role_enabled_posture_is_accepted() {
    let config = Config::parse(ROLE_ENABLED).expect("valid config");
    assert!(
        config.ssu2.introducer_service,
        "the role's opt-in must be honoured"
    );
    assert!(
        config.ssu2.advertise,
        "the publication opt-in must be honoured"
    );
}

// --------------------------------------------------------- the role's liveness

#[tokio::test]
async fn the_role_is_live_only_behind_the_opt_in() {
    let without = Fixture::start(&daemon_ssu2_config(PUBLISH_ENABLED), make_router_keys()).await;
    assert!(
        !without.handle.service().introducer_service_enabled(),
        "the role must not be live behind the default posture"
    );
    without.shutdown().await;

    let with = Fixture::start(&daemon_ssu2_config(ROLE_ENABLED), make_router_keys()).await;
    assert!(
        with.handle.service().introducer_service_enabled(),
        "the role must be live behind the explicit opt-in"
    );
    with.shutdown().await;
}

// ------------------------------------------- the runtime's own composition

#[tokio::test]
async fn the_published_capability_carries_the_role_letter_only_while_the_role_is_live() {
    let without = Fixture::start(&daemon_ssu2_config(PUBLISH_ENABLED), make_router_keys()).await;
    let caps = without
        .handle
        .service()
        .advertised_capabilities()
        .expect("the capability");
    assert_eq!(
        caps.as_str(),
        "4",
        "without the role the capability must be the family letter alone"
    );
    assert!(
        !caps.supports_relay(),
        "without the role the router must not advertise the introducer role"
    );
    without.shutdown().await;

    let with = Fixture::start(&daemon_ssu2_config(ROLE_ENABLED), make_router_keys()).await;
    let caps = with
        .handle
        .service()
        .advertised_capabilities()
        .expect("the capability");
    assert_eq!(
        caps.as_str(),
        "4C",
        "with the role the capability must be the family letter followed by the role letter"
    );
    assert!(
        caps.supports_relay(),
        "with the role the router must advertise the introducer role"
    );
    with.shutdown().await;
}

#[tokio::test]
async fn the_introducer_only_form_never_carries_the_role_letter() {
    // This router has the role **live** and it holds an introducer, so the
    // introducer-only form is the form it would publish. The form publishes
    // no endpoint, so it must not carry the role letter.
    let (a, b) = start_pair(ROLE_ENABLED, INTRODUCER_ENABLED).await;
    a.handle
        .service()
        .queue_relay_tag_request(b.peer())
        .expect("the request must be queued over the live session");
    wait_for_introducers(&a.handle, &b.keys.hash).await;

    let material = a
        .handle
        .service()
        .introducer_publication_material(wall_ms() / 1_000)
        .expect("the introducer-only material must be built");
    assert_eq!(material.form, Ssu2PublicationForm::IntroducerOnly);
    assert!(
        !material.advertises_introducer_role,
        "the introducer-only form must never advertise the role"
    );
    let caps = material.address.options().get("caps").expect("caps");
    assert!(
        caps == "4" || caps == "6",
        "the capability must be the family letter alone, found {caps:?}"
    );
    assert!(
        material.address.options().get("host").is_none(),
        "the introducer-only form publishes no host"
    );
    assert!(
        is_qualified_ssu2_introducer_address(&material.address),
        "the netdb's own introducer rule must accept the address"
    );

    a.shutdown().await;
    b.shutdown().await;
}

// ---------------------------------------------------- the netdb's third rule

/// The role-bearing direct address the rule must accept.
fn role_address(now: u64) -> RouterAddress {
    let mut entries = vec![
        ("caps".to_string(), "4C".to_string()),
        ("host".to_string(), "127.0.0.1".to_string()),
        ("i".to_string(), i2p_b64_encode(&[9_u8; 32])),
        ("mtu".to_string(), "1280".to_string()),
        ("port".to_string(), "12345".to_string()),
        ("s".to_string(), i2p_b64_encode(&[7_u8; 32])),
        ("v".to_string(), "2".to_string()),
    ];
    entries.sort();
    RouterAddress::new(
        10,
        Date::from_millis(now),
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address")
}

/// The direct address without the role letter.
fn plain_address(now: u64) -> RouterAddress {
    let mut entries = vec![
        ("caps".to_string(), "4".to_string()),
        ("host".to_string(), "127.0.0.1".to_string()),
        ("i".to_string(), i2p_b64_encode(&[9_u8; 32])),
        ("mtu".to_string(), "1280".to_string()),
        ("port".to_string(), "12345".to_string()),
        ("s".to_string(), i2p_b64_encode(&[7_u8; 32])),
        ("v".to_string(), "2".to_string()),
    ];
    entries.sort();
    RouterAddress::new(
        10,
        Date::from_millis(now),
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address")
}

/// The introducer-only address.
fn introducer_address(now: u64) -> RouterAddress {
    let mut entries = vec![
        ("caps".to_string(), "4".to_string()),
        ("i".to_string(), i2p_b64_encode(&[9_u8; 32])),
        ("iexp0".to_string(), "1800000000".to_string()),
        ("ih0".to_string(), i2p_b64_encode(&[5_u8; 32])),
        ("itag0".to_string(), "12345".to_string()),
        ("mtu".to_string(), "1280".to_string()),
        ("s".to_string(), i2p_b64_encode(&[7_u8; 32])),
        ("v".to_string(), "2".to_string()),
    ];
    entries.sort();
    RouterAddress::new(
        10,
        Date::from_millis(now),
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address")
}

#[test]
fn the_netdb_third_rule_accepts_the_role_bearing_address() {
    let now = wall_ms();
    let role = role_address(now);

    assert!(
        is_qualified_ssu2_relay_role_address(&role),
        "the third rule must accept the role-bearing address"
    );
    assert!(
        !is_qualified_ssu2_address(&role),
        "the first rule must still refuse the role-bearing address"
    );
    assert!(
        !is_qualified_ssu2_introducer_address(&role),
        "the introducer rule must refuse the role-bearing address"
    );
}

#[test]
fn the_netdb_third_rule_is_exact() {
    let now = wall_ms();
    let plain = plain_address(now);
    assert!(
        !is_qualified_ssu2_relay_role_address(&plain),
        "the third rule must refuse the address without the role letter"
    );
    assert!(
        is_qualified_ssu2_address(&plain),
        "the first rule must still accept the plain address"
    );
    assert!(
        !is_qualified_ssu2_relay_role_address(&introducer_address(now)),
        "the third rule must refuse the introducer-only address"
    );

    // Every other capability string is refused: the rule is exact.
    for caps in ["BC", "4CB", "46C", "C", "4", "6", "4C6", ""] {
        let mut entries = vec![
            ("caps".to_string(), caps.to_string()),
            ("host".to_string(), "127.0.0.1".to_string()),
            ("i".to_string(), i2p_b64_encode(&[9_u8; 32])),
            ("mtu".to_string(), "1280".to_string()),
            ("port".to_string(), "12345".to_string()),
            ("s".to_string(), i2p_b64_encode(&[7_u8; 32])),
            ("v".to_string(), "2".to_string()),
        ];
        entries.sort();
        let address = RouterAddress::new(
            10,
            Date::from_millis(now),
            "SSU2".to_string(),
            Mapping::from_entries(entries).expect("options"),
        )
        .expect("address");
        assert!(
            !is_qualified_ssu2_relay_role_address(&address),
            "the third rule must refuse the capability string {caps:?}"
        );
    }
}

#[test]
fn the_third_builder_signs_the_role_bearing_address_and_the_first_two_refuse() {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let builder = LocalRouterInfoBuilder::new(&bundle);
    let options = i2pr_netdb::controlled_router_options().expect("options");
    let published = Date::from_millis(wall_ms());
    let role = role_address(wall_ms());
    let plain = plain_address(wall_ms());
    let introducer = introducer_address(wall_ms());

    assert!(
        builder
            .build_with_ssu2_relay_role_address(published, options.clone(), role.clone())
            .is_ok(),
        "the third builder must accept the role-bearing address"
    );
    assert!(
        builder
            .build_with_ssu2_address(published, options.clone(), role.clone())
            .is_err(),
        "the first builder must refuse the role-bearing address"
    );
    assert!(
        builder
            .build_with_ssu2_introducer_address(published, options.clone(), role.clone())
            .is_err(),
        "the introducer builder must refuse the role-bearing address"
    );
    assert!(
        builder
            .build_with_ssu2_address(published, options.clone(), plain.clone())
            .is_ok(),
        "the first builder must still accept the plain address"
    );
    assert!(
        builder
            .build_with_ssu2_introducer_address(published, options.clone(), introducer)
            .is_ok(),
        "the introducer builder must still accept the introducer-only address"
    );
}

// ------------------------------------------------------ the daemon's hand-off

#[tokio::test]
async fn the_router_publishes_the_role_bearing_record_and_counts_it() {
    let a = Fixture::start(&daemon_ssu2_config(ROLE_ENABLED), make_router_keys()).await;
    let now = wall_ms();
    a.install_plain(now);

    let owner = SelfPublicationOwner::new(true);
    let material = a.material("4C", true, now);
    let outcome = owner.apply(
        &a.keys.bundle,
        a.handle.service().installed_local_router_info().as_deref(),
        Ok(&material),
        a.handle.service(),
        now,
    );
    assert_eq!(
        outcome,
        SelfPublicationOutcome::Published,
        "the role-bearing direct form must be published"
    );

    let installed = a
        .handle
        .service()
        .installed_local_router_info()
        .expect("a record is installed");
    let addresses = addresses_of(&installed);
    assert_eq!(addresses.len(), 1, "the record carries exactly one address");
    assert_eq!(addresses[0].transport_style(), "SSU2");
    assert_eq!(
        addresses[0].options().get("caps"),
        Some("4C"),
        "the published address must advertise the introducer role"
    );
    assert!(
        is_qualified_ssu2_relay_role_address(&addresses[0]),
        "the netdb's own third rule must accept the published address"
    );

    // The published record carries no `f` capability: the third builder
    // carries no floodfill permit.
    let info =
        RouterInfo::decode(&installed, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    let caps = info.options().get("caps").unwrap_or_default();
    assert!(
        !caps.contains('f'),
        "the published record must not advertise the floodfill capability"
    );

    // The counters distinguish the three published forms.
    let counters = owner.counters().snapshot();
    assert_eq!(counters.published_direct_relay_role, 1);
    assert_eq!(counters.published_direct, 0);
    assert_eq!(counters.published_introducer_only, 0);
    assert_eq!(
        counters.published, 1,
        "published is the sum of the three published forms"
    );
    assert_eq!(counters.withdrawn, 0);
    assert_eq!(counters.failed_unqualified_address, 0);
    assert_eq!(counters.failed_unqualified_introducer_address, 0);
    assert_eq!(counters.failed_unqualified_relay_role_address, 0);

    a.shutdown().await;
}

#[tokio::test]
async fn a_router_without_the_role_publishes_no_role_letter() {
    let a = Fixture::start(&daemon_ssu2_config(PUBLISH_ENABLED), make_router_keys()).await;
    let now = wall_ms();
    a.install_plain(now);

    let owner = SelfPublicationOwner::new(true);
    let material = a.material("4", false, now);
    let outcome = owner.apply(
        &a.keys.bundle,
        a.handle.service().installed_local_router_info().as_deref(),
        Ok(&material),
        a.handle.service(),
        now,
    );
    assert_eq!(outcome, SelfPublicationOutcome::Published);

    let installed = a
        .handle
        .service()
        .installed_local_router_info()
        .expect("a record is installed");
    let addresses = addresses_of(&installed);
    assert_eq!(addresses.len(), 1);
    assert_eq!(
        addresses[0].options().get("caps"),
        Some("4"),
        "the published address must carry the family letter alone"
    );

    let counters = owner.counters().snapshot();
    assert_eq!(counters.published_direct, 1);
    assert_eq!(counters.published_direct_relay_role, 0);
    assert_eq!(counters.published, 1);

    a.shutdown().await;
}

#[tokio::test]
async fn the_role_rule_s_refusal_is_counted_under_its_own_reason() {
    let a = Fixture::start(&daemon_ssu2_config(ROLE_ENABLED), make_router_keys()).await;
    let now = wall_ms();
    a.install_plain(now);

    // The material claims the role, but its address carries a capability
    // string the third rule refuses.
    let owner = SelfPublicationOwner::new(true);
    let mut material = a.material("4C", true, now);
    let mut entries = vec![
        ("caps".to_string(), "4".to_string()),
        ("host".to_string(), a.bound().ip().to_string()),
        ("i".to_string(), i2p_b64_encode(a.keys.intro.as_bytes())),
        ("mtu".to_string(), "1280".to_string()),
        ("port".to_string(), a.bound().port().to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(a.keys.static_public.as_bytes()),
        ),
        ("v".to_string(), "2".to_string()),
    ];
    entries.sort();
    material.address = RouterAddress::new(
        10,
        Date::from_millis(now),
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address");

    let outcome = owner.apply(
        &a.keys.bundle,
        a.handle.service().installed_local_router_info().as_deref(),
        Ok(&material),
        a.handle.service(),
        now,
    );
    assert!(
        matches!(outcome, SelfPublicationOutcome::Failed(_)),
        "the third rule's refusal must fail the publication, found {outcome:?}"
    );

    let counters = owner.counters().snapshot();
    assert_eq!(
        counters.failed_unqualified_relay_role_address, 1,
        "the third rule's refusal must be counted under its own reason"
    );
    assert_eq!(counters.failed_unqualified_address, 0);
    assert_eq!(counters.published, 0);

    a.shutdown().await;
}

// --------------------------------------------------------- the install gate

#[tokio::test]
async fn the_install_gate_refuses_a_stale_role_letter() {
    let a = Fixture::start(&daemon_ssu2_config(PUBLISH_ENABLED), make_router_keys()).await;
    let now = wall_ms();
    let stale = a.record_with_caps(Some("4C"), now);

    // The role is **not** live, so the record's claim cannot be honoured
    // and the gate refuses it outright.
    assert!(
        a.handle
            .service()
            .install_local_router_info(stale.clone(), now)
            .is_err(),
        "a role-bearing record must be refused while the role is not live"
    );
    let installed = a
        .handle
        .service()
        .installed_local_router_info()
        .expect("the seeded record");
    assert_ne!(installed, stale, "the refused record must not be installed");

    a.shutdown().await;
}

#[tokio::test]
async fn the_install_gate_accepts_a_role_letter_while_the_role_is_live() {
    let a = Fixture::start(&daemon_ssu2_config(ROLE_ENABLED), make_router_keys()).await;
    let now = wall_ms();
    let role = a.record_with_caps(Some("4C"), now);

    assert!(
        a.handle
            .service()
            .install_local_router_info(role.clone(), now)
            .is_ok(),
        "a role-bearing record must be installed while the role is live"
    );
    let installed = a
        .handle
        .service()
        .installed_local_router_info()
        .expect("the record is installed");
    assert_eq!(
        installed, role,
        "the installed record must be the role-bearing one"
    );

    a.shutdown().await;
}

// ------------------------------------------------ the inspection seam's shape

#[test]
fn the_counter_snapshot_carries_the_third_published_form() {
    let snapshot = SelfPublicationCounterSnapshot::default();
    assert_eq!(snapshot.published, 0);
    assert_eq!(snapshot.published_direct, 0);
    assert_eq!(snapshot.published_direct_relay_role, 0);
    assert_eq!(snapshot.published_introducer_only, 0);
}

// ----------------------------------------------------- the composition's reach

/// The child's output is drained on a reader thread: a full pipe buffer
/// would block the router before it could start.
fn drain(reader: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let mut reader = reader;
        let _ = reader.read_to_string(&mut buffer);
        buffer
    })
}

/// A spawned `i2pr run` child whose output is drained on reader threads.
struct RunningRouter {
    child: Child,
    stdout: Option<std::thread::JoinHandle<String>>,
    stderr: Option<std::thread::JoinHandle<String>>,
}

impl RunningRouter {
    fn spawn(config: &Path) -> RunningRouter {
        let mut child = Command::new(env!("CARGO_BIN_EXE_i2pr"))
            .args(["run", "--config"])
            .arg(config)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn i2pr run");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        RunningRouter {
            child,
            stdout: Some(drain(stdout)),
            stderr: Some(drain(stderr)),
        }
    }

    /// Returns the exit status once the child exits on its own, or `None`
    /// if the deadline passes first.
    fn wait_for_exit(&mut self, deadline: Duration) -> Option<ExitStatus> {
        let started = Instant::now();
        while started.elapsed() < deadline {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return Some(status),
                None => std::thread::sleep(POLL_INTERVAL),
            }
        }
        None
    }

    /// Requests SIGINT, which `run_daemon` treats as operator shutdown.
    #[cfg(unix)]
    fn interrupt(&self) {
        let _ = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status();
    }

    #[cfg(not(unix))]
    fn interrupt(&self) {
        let _ = self.child.kill();
    }

    /// Joins both reader threads and returns everything the child printed.
    fn finish(mut self) -> (ExitStatus, String) {
        let status = self
            .wait_for_exit(COMPOSITION_EXIT_DEADLINE)
            .unwrap_or_else(|| {
                let _ = self.child.kill();
                self.child.wait().expect("reap killed child")
            });
        let stdout = self
            .stdout
            .take()
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        let stderr = self
            .stderr
            .take()
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        (status, format!("{stdout}\n{stderr}"))
    }
}

impl Drop for RunningRouter {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("set mode");
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

#[test]
fn the_daemon_starts_with_the_introducer_role_enabled() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    set_mode(&data_dir, 0o700);
    let config = directory.path().join("router.toml");
    // The SSU2 service runs with the introducer role **on**. Before this
    // plan the composition refused the role outright, so the router could
    // not start in this configuration at all.
    let body = format!(
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nbind_ipv6 = \"\"\nintroducer_service = true\n",
        data_dir.to_string_lossy()
    );
    std::fs::write(&config, body).expect("write config");

    let generated = Command::new(env!("CARGO_BIN_EXE_i2pr"))
        .args(["identity", "generate", "--config"])
        .arg(&config)
        .output()
        .expect("run identity generate");
    assert!(
        generated.status.success(),
        "identity generate failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let mut child = RunningRouter::spawn(&config);
    // The router must still be running: a service that refused to start
    // would have taken the router down with it.
    std::thread::sleep(COMPOSITION_HOLD);
    assert!(
        child.wait_for_exit(Duration::from_millis(1)).is_none(),
        "the router must still be running with the introducer role enabled"
    );

    child.interrupt();
    let (status, output) = child.finish();
    assert!(
        status.success(),
        "expected a clean exit, got {status}\n{output}"
    );
    assert!(
        !output.contains("SSU2 service requires the strict controlled profile"),
        "the composition must not refuse the introducer role:\n{output}"
    );
}
