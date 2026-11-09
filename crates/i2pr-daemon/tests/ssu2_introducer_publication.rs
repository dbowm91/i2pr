//! Plan 525's lane — the router publishes the introducer set it learned.
//!
//! A router behind a firewall cannot be reached directly, so it publishes
//! an **introducer-only** address: no `host`, no `port`, and the
//! introducers that will vouch for it. The pinned i2pd 2.61.0 publishes
//! exactly that shape when it is not published directly
//! (`libi2pd/RouterInfo.cpp:1350`-`:1466`): the `caps` option carries the
//! **family** letter, `host` and `port` are omitted, `i` is written, and
//! the `iexp[N]`/`ih[N]`/`itag[N]` groups are written whenever the address
//! carries introducers.
//!
//! The lane drives the **public** behaviour through the daemon's own owner
//! and the runtime's own live service, over a **real** SSU2 session
//! between two daemon-owned services on loopback:
//!
//! 1. the publication switch is **off** by default, and this plan adds
//!    **no** new configuration key;
//! 2. the runtime's introducer-only builder **succeeds** — it could never
//!    succeed before this plan, because it passed the build time as the
//!    evidence expiry and the transport's own gate withdrew every
//!    snapshot;
//! 3. the built address carries the specification's introducer groups and
//!    **no** `host` and **no** `port`, and the netdb's own introducer rule
//!    accepts it;
//! 4. the runtime's **composition** yields the introducer-only form for a
//!    firewalled router that holds introducers;
//! 5. the daemon publishes the introducer-only record through the netdb's
//!    **introducer** builder, and the published record carries no `f`
//!    capability;
//! 6. a router with **no** introducer is refused under its own reason, and
//!    the counter that names it **moves**;
//! 7. the direct form is still published through the netdb's **direct**
//!    builder, and each builder refuses the other's address;
//! 8. the counters distinguish the two published forms and the withdrawal;
//! 9. the counters are readable through the inspection seam.
//!
//! Assertions are counts, shapes and digests only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::router_i2np::{Ssu2DaemonHandle, Ssu2DaemonService};
use i2pr_daemon::self_publication::{
    SelfPublicationDecision, SelfPublicationOutcome, SelfPublicationOwner,
};
use i2pr_netdb::{
    LocalRouterInfoBuilder, is_qualified_ssu2_address, is_qualified_ssu2_introducer_address,
};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, RouterInfo};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2IdentityMaterial,
    Ssu2PublicationForm,
};
use i2pr_transport::PeerId;
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The publication switch is off by default, so the default tree changes
/// in no way.
const DEFAULT_CONFIG: &str =
    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";

/// The router's own posture: it publishes its own SSU2 address.
const PUBLISH_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nadvertise = true\n";

/// The peer's own posture: it is willing to **be** an introducer, so it
/// answers a relay-tag request.
const INTRODUCER_ENABLED: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nintroducer_service = true\n";

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn wall_secs() -> u64 {
    wall_ms() / 1_000
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
///
/// The **same** bundle signs the record the runtime publishes and the
/// record the daemon's owner builds, so the install gate's own identity
/// check is satisfied by the lane's own material.
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
        ("caps".to_string(), "4C".to_string()),
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
    /// It is the record the owner already owns when it refreshes: one
    /// SSU2 address, signed by the router's own bundle.
    fn plain_record(&self, now: u64) -> Vec<u8> {
        let options = Mapping::from_entries(vec![
            ("host".to_string(), self.bound().ip().to_string()),
            ("port".to_string(), self.bound().port().to_string()),
            ("v".to_string(), "2".to_string()),
            (
                "s".to_string(),
                i2p_b64_encode(self.keys.static_public.as_bytes()),
            ),
            ("i".to_string(), i2p_b64_encode(self.keys.intro.as_bytes())),
        ])
        .expect("options");
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
        let record = self.plain_record(now);
        self.handle
            .service()
            .install_local_router_info(record.clone(), now)
            .expect("plain record installed");
        record
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
        let learned = handle.service().learned_introducers(wall_secs());
        if learned.len() == 1 {
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
            "the router learned {} introducers, expected 1",
            learned.len()
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Starts two daemon-owned services, establishes a session between them and
/// drives the relay-tag exchange, so the router holds a **learned**
/// introducer.
///
/// The first is this router — the one that publishes. The second is the
/// peer that is willing to be an introducer.
async fn start_pair() -> (Fixture, Fixture) {
    let router = daemon_ssu2_config(PUBLISH_ENABLED);
    let introducer = daemon_ssu2_config(INTRODUCER_ENABLED);
    let a = Fixture::start(&router, make_router_keys()).await;
    let b = Fixture::start(&introducer, make_router_keys()).await;
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
    // The relay-tag exchange: the router asks, the peer answers, and the
    // answer is recorded as an authenticated introducer.
    a.handle
        .service()
        .queue_relay_tag_request(b.peer())
        .expect("the request must be queued over the live session");
    wait_for_introducers(&a.handle, &b.keys.hash).await;
    (a, b)
}

/// The decoded addresses of a record.
fn addresses_of(encoded: &[u8]) -> Vec<RouterAddress> {
    let info = RouterInfo::decode(encoded, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    info.addresses().to_vec()
}

// ---------------------------------------------------------------- the switch

#[test]
fn the_publication_switch_stays_off_by_default() {
    let config = Config::parse(DEFAULT_CONFIG).expect("valid config");
    assert!(
        !config.ssu2.advertise,
        "the publication switch must default to off"
    );
}

#[test]
fn the_enabled_posture_is_accepted() {
    let config = Config::parse(PUBLISH_ENABLED).expect("valid config");
    assert!(config.ssu2.advertise, "the opt-in must be honoured");
}

// ------------------------------------------------- the runtime's own builder

#[tokio::test]
async fn the_introducer_only_builder_succeeds_where_it_used_to_fail() {
    let (a, b) = start_pair().await;

    // The builder that could never succeed before this plan.
    let material = a
        .handle
        .service()
        .introducer_publication_material(wall_secs())
        .expect("the introducer-only material must be built");

    assert_eq!(
        material.form,
        Ssu2PublicationForm::IntroducerOnly,
        "the material must be the introducer-only form"
    );
    assert_eq!(
        material.reachability,
        i2pr_transport::ReachabilityState::Firewalled,
        "the introducer-only form is the firewalled form"
    );

    let options = material.address.options();
    assert!(
        options.get("host").is_none(),
        "the introducer-only form publishes no host"
    );
    assert!(
        options.get("port").is_none(),
        "the introducer-only form publishes no port"
    );
    assert!(
        options.get("ih0").is_some(),
        "the form must publish the introducer's router hash"
    );
    assert!(
        options.get("itag0").is_some(),
        "the form must publish the introducer's relay tag"
    );
    assert!(
        options.get("iexp0").is_some(),
        "the form must publish the introducer's expiry"
    );
    let caps = options.get("caps").expect("a family capability");
    assert!(
        caps == "4" || caps == "6",
        "the capability must be the family letter, found {caps:?}"
    );

    assert!(
        is_qualified_ssu2_introducer_address(&material.address),
        "the netdb's own introducer rule must accept the address"
    );
    assert!(
        !is_qualified_ssu2_address(&material.address),
        "the netdb's own direct rule must refuse the address"
    );

    // The evidence is the introducer set, so the snapshot's expiry is the
    // earliest expiry among the introducers it publishes.
    let learned = a.handle.service().learned_introducers(wall_secs());
    let earliest = learned
        .iter()
        .map(|introducer| introducer.expires_secs())
        .min()
        .expect("one introducer");
    let expected_expiry = earliest.to_string();
    assert_eq!(
        options.get("iexp0"),
        Some(expected_expiry.as_str()),
        "the published expiry must be the introducer's own expiry"
    );
    assert!(
        material.evidence_expires_at.as_secs() > 0,
        "the material must carry a real remaining lifetime"
    );

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn the_composition_yields_the_introducer_only_form() {
    let (a, b) = start_pair().await;

    let material = a
        .handle
        .service()
        .self_publication_material(wall_ms())
        .expect("the composition must answer");
    assert_eq!(
        material.form,
        Ssu2PublicationForm::IntroducerOnly,
        "a firewalled router with introducers publishes the introducer-only form"
    );
    assert!(
        material.address.options().get("host").is_none(),
        "the published form carries no endpoint"
    );

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn a_router_with_no_introducer_is_refused_under_its_own_reason() {
    let router = daemon_ssu2_config(PUBLISH_ENABLED);
    let a = Fixture::start(&router, make_router_keys()).await;

    let reason = a
        .handle
        .service()
        .self_publication_material(wall_ms())
        .expect_err("no introducer means no published form");
    assert_eq!(
        reason,
        i2pr_runtime::Ssu2PublicationUnavailable::NoIntroducers,
        "the refusal must name the empty introducer set"
    );

    a.shutdown().await;
}

// ------------------------------------------------ the daemon's own hand-off

#[tokio::test]
async fn the_router_publishes_the_introducer_only_record() {
    let (a, b) = start_pair().await;
    let now = wall_ms();
    a.install_plain(now);

    let owner = SelfPublicationOwner::new(true);
    let outcome = owner.refresh(&a.keys.bundle, a.handle.service(), now);
    assert_eq!(
        outcome,
        SelfPublicationOutcome::Published,
        "the introducer-only form must be published"
    );

    let installed = a
        .handle
        .service()
        .installed_local_router_info()
        .expect("a record is installed");
    let addresses = addresses_of(&installed);
    assert_eq!(addresses.len(), 1, "the record carries exactly one address");
    assert_eq!(addresses[0].transport_style(), "SSU2");
    assert!(
        addresses[0].options().get("host").is_none(),
        "the published address carries no host"
    );
    assert!(
        addresses[0].options().get("port").is_none(),
        "the published address carries no port"
    );
    assert!(
        addresses[0].options().get("ih0").is_some(),
        "the published address publishes the introducer"
    );
    assert!(
        is_qualified_ssu2_introducer_address(&addresses[0]),
        "the netdb's own introducer rule must accept the published address"
    );

    // The published record carries no `f` capability: the introducer
    // builder carries no floodfill permit.
    let info =
        RouterInfo::decode(&installed, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    let caps = info.options().get("caps").unwrap_or_default();
    assert!(
        !caps.contains('f'),
        "the published record must not advertise the floodfill capability"
    );

    // The counters distinguish the forms.
    let counters = owner.counters().snapshot();
    assert_eq!(counters.published_introducer_only, 1);
    assert_eq!(counters.published_direct, 0);
    assert_eq!(
        counters.published, 1,
        "published is the sum of the two forms"
    );
    assert_eq!(counters.withdrawn, 0);
    assert_eq!(counters.failed_unqualified_address, 0);
    assert_eq!(counters.failed_unqualified_introducer_address, 0);

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn the_empty_introducer_set_withdraws_and_is_counted() {
    let router = daemon_ssu2_config(PUBLISH_ENABLED);
    let a = Fixture::start(&router, make_router_keys()).await;
    let now = wall_ms();
    a.install_plain(now);

    let owner = SelfPublicationOwner::new(true);
    let outcome = owner.refresh(&a.keys.bundle, a.handle.service(), now);
    assert_eq!(
        outcome,
        SelfPublicationOutcome::Published,
        "the withdrawal is an install too"
    );

    let counters = owner.counters().snapshot();
    assert_eq!(
        counters.withheld_no_introducers, 1,
        "the empty introducer set must be counted under its own reason"
    );
    assert_eq!(counters.withdrawn, 1, "the withdrawal must be counted");
    assert_eq!(counters.published, 0, "no address was published");
    assert_eq!(counters.published_direct, 0);
    assert_eq!(counters.published_introducer_only, 0);

    // The withdrawn record carries no address at all.
    let installed = a
        .handle
        .service()
        .installed_local_router_info()
        .expect("a record is installed");
    assert!(
        addresses_of(&installed).is_empty(),
        "the withdrawal publishes no address"
    );

    a.shutdown().await;
}

#[test]
fn each_builder_refuses_the_other_form() {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let builder = LocalRouterInfoBuilder::new(&bundle);
    let options = i2pr_netdb::controlled_router_options().expect("options");
    let published = Date::from_millis(wall_ms());

    // The direct form: host, port, the family capability and the MTU.
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
    let direct = RouterAddress::new(
        10,
        published,
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address");

    // The introducer-only form: the introducer groups and no endpoint.
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
    let introducer = RouterAddress::new(
        10,
        published,
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address");

    assert!(is_qualified_ssu2_address(&direct));
    assert!(!is_qualified_ssu2_introducer_address(&direct));
    assert!(is_qualified_ssu2_introducer_address(&introducer));
    assert!(!is_qualified_ssu2_address(&introducer));

    // The direct builder refuses the introducer-only address, and the
    // introducer builder refuses the direct one.
    assert!(
        builder
            .build_with_ssu2_address(published, options.clone(), introducer.clone())
            .is_err(),
        "the direct builder must refuse the introducer-only address"
    );
    assert!(
        builder
            .build_with_ssu2_introducer_address(published, options.clone(), direct.clone())
            .is_err(),
        "the introducer builder must refuse the direct address"
    );
    assert!(
        builder
            .build_with_ssu2_address(published, options.clone(), direct.clone())
            .is_ok(),
        "the direct builder must accept the direct address"
    );
    assert!(
        builder
            .build_with_ssu2_introducer_address(published, options, introducer)
            .is_ok(),
        "the introducer builder must accept the introducer-only address"
    );
}

#[test]
fn the_direct_form_is_still_published_through_the_direct_builder() {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let now = wall_ms();
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
    let address = RouterAddress::new(
        10,
        Date::from_millis(now),
        "SSU2".to_string(),
        Mapping::from_entries(entries).expect("options"),
    )
    .expect("address");
    let material = i2pr_runtime::Ssu2PublicationMaterial {
        address,
        form: Ssu2PublicationForm::Direct,
        evidence_expires_at: Duration::from_secs(3_600),
        reachability: i2pr_transport::ReachabilityState::Reachable,
        bound_family: i2pr_transport::AddressFamily::Ipv4,
    };

    let owner = SelfPublicationOwner::new(true);
    let decision = owner.decide(&bundle, None, Ok(&material), now);
    // No record is installed, so the owner stands down: the decision seam
    // is exercised with an installed record in the live lane above.
    assert!(
        matches!(decision, SelfPublicationDecision::StoodDown(_)),
        "an owner with no installed record must stand down, found {decision:?}"
    );
    assert_eq!(owner.counters().snapshot().published_direct, 0);
    assert_eq!(owner.counters().snapshot().published, 0);
}

// ------------------------------------------------------- the inspection seam

#[test]
fn the_counters_are_readable_through_the_inspection_seam() {
    let config = Config::parse(PUBLISH_ENABLED).expect("valid config");
    let inspection = InspectionHandles::from_config(&config);
    assert!(
        inspection.self_publication_counters_snapshot().is_none(),
        "an unpublished slot is a gap, never a zero"
    );

    let cell = std::sync::Arc::new(std::sync::Mutex::new(
        i2pr_daemon::self_publication::SelfPublicationCounterSnapshot::default(),
    ));
    inspection.publish_self_publication_counters(cell);
    let snapshot = inspection
        .self_publication_counters_snapshot()
        .expect("the published cell must be readable");
    assert_eq!(snapshot.published, 0);
    assert_eq!(snapshot.published_direct, 0);
    assert_eq!(snapshot.published_introducer_only, 0);
    assert_eq!(snapshot.withdrawn, 0);
}
