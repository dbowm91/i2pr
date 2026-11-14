//! Plan 520's lane — the router publishes its own qualified SSU2 address.
//!
//! The router publishes its own RouterInfo to the network. Plan 520 gives
//! that record an address a peer can **qualify**: the direct SSU2 form,
//! carrying the family capability and the transport's MTU, published only
//! while the router's own reachability evidence admits it.
//!
//! The lane drives the **public** behaviour, through the daemon's own
//! owner and the runtime's own live service — never through a private
//! helper:
//!
//! 1. the opt-in is **off** by default, and the router then publishes
//!    exactly what it published before;
//! 2. the direct form is published when the transport's own material
//!    admits it, and the netdb's **own** qualification accepts the address;
//! 3. the published record carries **no** `f` capability, and the floodfill
//!    permit still gates `caps=f`;
//! 4. the evidence lapsing **withdraws** the address;
//! 5. the re-dated record still carries the address;
//! 6. the floodfill path's record is left alone;
//! 7. every outcome is counted under its own reason.
//!
//! The **transport's** half of the contract — that the direct snapshot is
//! rendered only for a `Reachable` state and that it carries the family
//! capability and the transport's MTU — is the transport's own and is
//! proven in `i2pr-transport-ssu2` and `i2pr-runtime`. The lane proves the
//! daemon's half: that the address the transport admits is the address the
//! netdb accepts, and that the router publishes it only when the evidence
//! says it may.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::router_i2np::{
    Ssu2DaemonHandle, Ssu2DaemonService, generate_controlled_identity,
    refresh_local_router_info_record,
};
use i2pr_daemon::self_publication::{
    SelfPublicationDecision, SelfPublicationOutcome, SelfPublicationOwner, StandDownReason,
};
use i2pr_netdb::{LocalRouterInfoBuilder, is_qualified_ssu2_address};
use i2pr_proto::{Date, Mapping, RouterAddress, RouterInfo};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, Ssu2PublicationMaterial,
    Ssu2PublicationUnavailable,
};
use i2pr_transport::{AddressFamily, ReachabilityState};
use rand_core::OsRng;

/// A loopback-only configuration. The bind port is ephemeral, so the lane
/// never races another test for a port.
const CONFIG: &str = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nbind_ipv6 = \"\"\nport = 0\n";

/// The placeholder port the identity carries before the bind, exactly as
/// the daemon's composition does. The lane re-signs every record it
/// installs with the **bound** port.
const PREBIND_PORT: u16 = 44_001;

/// The transport's own validated MTU for this build.
const TRANSPORT_MTU: &str = "1280";

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

/// The address the transport's direct snapshot produces for the router's
/// bound endpoint.
///
/// The transport emits exactly these options — the family capability, the
/// host, the intro key, the MTU, the port, the static key and the
/// version — and the netdb's qualification demands exactly that set.
fn qualified_address(
    bound: SocketAddr,
    static_public: [u8; 32],
    intro: [u8; 32],
    now: u64,
) -> RouterAddress {
    let options = Mapping::from_entries(vec![
        ("caps".to_owned(), "4".to_owned()),
        ("host".to_owned(), bound.ip().to_string()),
        ("i".to_owned(), i2pr_netdb::encode(&intro).expect("intro")),
        ("mtu".to_owned(), TRANSPORT_MTU.to_owned()),
        ("port".to_owned(), bound.port().to_string()),
        (
            "s".to_owned(),
            i2pr_netdb::encode(&static_public).expect("static"),
        ),
        ("v".to_owned(), "2".to_owned()),
    ])
    .expect("options");
    RouterAddress::new(10, Date::from_millis(now), "SSU2".to_owned(), options).expect("address")
}

/// The router's own address, in the form the daemon publishes when the
/// transport admits no direct endpoint: the bound endpoint, the version
/// and the two keys, with no family capability and no MTU.
///
/// This is the form the composition has always published. It is **not**
/// a form the netdb's own qualification accepts, and it is not the form
/// Plan 520 publishes: the plan publishes the transport's qualified
/// address, and reverts to this one when the evidence lapses.
fn plain_address(
    bound: SocketAddr,
    static_public: [u8; 32],
    intro: [u8; 32],
    now: u64,
) -> RouterAddress {
    let options = Mapping::from_entries(vec![
        ("host".to_owned(), bound.ip().to_string()),
        ("i".to_owned(), i2pr_netdb::encode(&intro).expect("intro")),
        ("port".to_owned(), bound.port().to_string()),
        (
            "s".to_owned(),
            i2pr_netdb::encode(&static_public).expect("static"),
        ),
        ("v".to_owned(), "2".to_owned()),
    ])
    .expect("options");
    RouterAddress::new(10, Date::from_millis(now), "SSU2".to_owned(), options).expect("address")
}

/// The transport's material for the address above.
fn material(address: RouterAddress) -> Ssu2PublicationMaterial {
    Ssu2PublicationMaterial {
        address,
        form: i2pr_runtime::Ssu2PublicationForm::Direct,
        advertises_introducer_role: false,
        evidence_expires_at: Duration::from_secs(3_600),
        reachability: ReachabilityState::Reachable,
        bound_family: AddressFamily::Ipv4,
    }
}

/// The live daemon-owned SSU2 service the lane publishes through.
struct Fixture {
    bundle: RouterIdentityBundle,
    handle: Ssu2DaemonHandle,
    scope: ChildScope,
    token: CancellationToken,
    static_public: [u8; 32],
    intro: [u8; 32],
}

impl Fixture {
    async fn start() -> Self {
        let config = Config::parse(CONFIG).expect("valid config");
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let identity = generate_controlled_identity(&bundle, "127.0.0.1", PREBIND_PORT)
            .expect("controlled identity");
        let static_public =
            X25519PrivateKey::from_bytes(identity.static_secret_bytes).public_bytes();
        let intro = *identity.intro_key.as_bytes();
        let service = Ssu2DaemonService::new(&config.ssu2, identity).expect("daemon-owned service");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
        let handle = service
            .start(&scope, &config.ssu2)
            .await
            .expect("daemon bind");
        Self {
            bundle,
            handle,
            scope,
            token,
            static_public,
            intro,
        }
    }

    /// The router's own bound endpoint.
    fn bound(&self) -> SocketAddr {
        self.handle
            .local_v4()
            .or(self.handle.local_v6())
            .expect("bound endpoint")
    }

    /// The transport's own direct address for the router's bound endpoint.
    fn address(&self, now: u64) -> RouterAddress {
        qualified_address(self.bound(), self.static_public, self.intro, now)
    }

    /// The router's own record, naming the **bound** endpoint, as the
    /// composition builds it after the bind.
    fn plain_record(&self, now: u64) -> Vec<u8> {
        let address = plain_address(self.bound(), self.static_public, self.intro, now);
        let options = i2pr_netdb::controlled_router_options().expect("options");
        let info = self
            .bundle
            .sign_router_info(Date::from_millis(now), vec![address], Vec::new(), options)
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

/// The decoded addresses of a record.
fn addresses_of(encoded: &[u8]) -> Vec<RouterAddress> {
    let info = RouterInfo::decode(encoded, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    info.addresses().to_vec()
}

/// The record's own capability string.
fn caps_of(encoded: &[u8]) -> Option<String> {
    let info = RouterInfo::decode(encoded, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    info.options().get("caps").map(|caps| caps.to_owned())
}

#[test]
fn the_opt_in_is_off_by_default() {
    let config = Config::parse(CONFIG).expect("valid config");
    assert!(
        !config.ssu2.advertise,
        "the publication switch must default to off"
    );
    let owner = SelfPublicationOwner::new(config.ssu2.advertise);
    assert!(!owner.enabled());
}

#[tokio::test]
async fn the_router_publishes_no_address_when_the_opt_in_is_off() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let installed = fixture.install_plain(now);
    let owner = SelfPublicationOwner::new(false);
    let outcome = owner.refresh(&fixture.bundle, fixture.handle.service(), now);
    assert_eq!(
        outcome,
        SelfPublicationOutcome::Disabled,
        "the opt-in is off, so the owner changes nothing"
    );
    assert_eq!(
        fixture.handle.service().installed_local_router_info(),
        Some(installed),
        "the published record must stand untouched while the opt-in is off"
    );
    assert_eq!(
        owner.counters().snapshot().disabled,
        1,
        "the refusal must be counted under its own reason"
    );
    assert_eq!(owner.counters().snapshot().published, 0);
    fixture.shutdown().await;
}

#[tokio::test]
async fn the_direct_form_is_published_when_the_evidence_admits_it() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let installed = fixture.install_plain(now);
    let owner = SelfPublicationOwner::new(true);
    let address = fixture.address(now);
    assert!(
        is_qualified_ssu2_address(&address),
        "the transport's own form must satisfy the netdb's qualification"
    );
    let decision = owner.decide(
        &fixture.bundle,
        Some(&installed),
        Ok(&material(address.clone())),
        now,
    );
    let SelfPublicationDecision::Install(encoded) = decision else {
        panic!("the direct form must be installable: {decision:?}");
    };
    let addresses = addresses_of(&encoded);
    assert_eq!(
        addresses.len(),
        1,
        "the published record carries exactly one address"
    );
    assert_eq!(addresses[0].transport_style(), "SSU2");
    assert_eq!(
        addresses[0].options().get("caps"),
        Some("4"),
        "the address's capability must be exactly the family letter"
    );
    assert_eq!(addresses[0].options().get("mtu"), Some(TRANSPORT_MTU));
    assert_eq!(
        addresses[0].options().get("port"),
        Some(fixture.bound().port().to_string().as_str())
    );
    assert_eq!(
        addresses[0].options().get("host"),
        Some(fixture.bound().ip().to_string().as_str()),
        "the published endpoint must be the router's own bound endpoint"
    );
    assert!(
        is_qualified_ssu2_address(&addresses[0]),
        "the netdb's own qualification must accept the published address"
    );
    // The record is installed, and the installed record is the published one.
    fixture
        .handle
        .service()
        .install_local_router_info(encoded.clone(), now)
        .expect("addressed record installed");
    let installed = fixture
        .handle
        .service()
        .installed_local_router_info()
        .expect("installed");
    assert_eq!(addresses_of(&installed), addresses);
    fixture.shutdown().await;
}

#[tokio::test]
async fn the_published_record_carries_no_f_capability() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let installed = fixture.install_plain(now);
    let owner = SelfPublicationOwner::new(true);
    let address = fixture.address(now);
    let decision = owner.decide(
        &fixture.bundle,
        Some(&installed),
        Ok(&material(address.clone())),
        now,
    );
    let SelfPublicationDecision::Install(encoded) = decision else {
        panic!("the direct form must be installable: {decision:?}");
    };
    let caps = caps_of(&encoded);
    assert!(
        caps.as_deref().is_none_or(|caps| !caps.contains('f')),
        "the ordinary record must carry no floodfill capability: {caps:?}"
    );
    // The floodfill builders still require the permit: the ordinary
    // builder refuses `caps=f` outright, and the addressed builder takes
    // no permit and can mint none.
    let builder = LocalRouterInfoBuilder::new(&fixture.bundle);
    let flooding = Mapping::from_entries(vec![("caps".to_owned(), "f".to_owned())])
        .expect("floodfill options");
    assert!(
        builder.build(Date::from_millis(now), flooding).is_err(),
        "the ordinary builder must never mint the floodfill capability"
    );
    let options = i2pr_netdb::controlled_router_options().expect("options");
    let local = builder
        .build_with_ssu2_address(Date::from_millis(now), options, address)
        .expect("addressed record");
    assert!(
        local.router_info().options().get("caps").is_none(),
        "the addressed builder must not carry a capability string"
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn the_address_is_withdrawn_when_the_evidence_lapses() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let installed = fixture.install_plain(now);
    let owner = SelfPublicationOwner::new(true);
    let address = fixture.address(now);
    let SelfPublicationDecision::Install(encoded) = owner.decide(
        &fixture.bundle,
        Some(&installed),
        Ok(&material(address)),
        now,
    ) else {
        panic!("the direct form must be installable");
    };
    fixture
        .handle
        .service()
        .install_local_router_info(encoded.clone(), now)
        .expect("addressed record installed");
    // The evidence lapses.
    let decision = owner.decide(
        &fixture.bundle,
        Some(&encoded),
        Err(Ssu2PublicationUnavailable::ReachabilityUnqualified),
        now,
    );
    let SelfPublicationDecision::Install(withdrawn) = decision else {
        panic!("the withdrawal must be installable: {decision:?}");
    };
    assert!(
        addresses_of(&withdrawn).is_empty(),
        "the lapsed address must be withdrawn, not kept"
    );
    // The withdrawal is counted under its own reason.
    let counters = owner.counters().snapshot();
    assert_eq!(counters.withheld_reachability_unqualified, 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn the_redated_record_still_carries_the_address() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let installed = fixture.install_plain(now);
    let owner = SelfPublicationOwner::new(true);
    let address = fixture.address(now);
    let SelfPublicationDecision::Install(encoded) = owner.decide(
        &fixture.bundle,
        Some(&installed),
        Ok(&material(address)),
        now,
    ) else {
        panic!("the direct form must be installable");
    };
    fixture
        .handle
        .service()
        .install_local_router_info(encoded, now)
        .expect("addressed record installed");
    // Plan 518's re-dating must not change the record's shape.
    //
    // Plan 547: the re-dating path carries the router's NTCP2 endpoint and
    // the scope that endpoint is admitted in. This record names **no** NTCP2
    // address, so it keeps the plain install form; the endpoint is absent
    // and the plain form's own gate requires exactly that.
    let fresh = refresh_local_router_info_record(
        &fixture.bundle,
        &fixture.handle.publication_handle(),
        now + 1_000,
        None,
        i2pr_runtime::Ntcp2DialScope::Loopback,
    )
    .expect("re-dated record");
    let addresses = addresses_of(&fresh);
    assert_eq!(
        addresses.len(),
        1,
        "the re-dated record must still carry the address"
    );
    assert_eq!(addresses[0].options().get("caps"), Some("4"));
    fixture.shutdown().await;
}

#[tokio::test]
async fn the_floodfill_record_is_left_alone() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let address = fixture.address(now);
    // The floodfill path's own record: the same qualified address, under
    // a capability string that carries `f`.
    let flooding = Mapping::from_entries(vec![
        ("caps".to_owned(), "fN".to_owned()),
        ("netId".to_owned(), "2".to_owned()),
    ])
    .expect("floodfill options");
    let info = fixture
        .bundle
        .sign_router_info(
            Date::from_millis(now),
            vec![address.clone()],
            Vec::new(),
            flooding,
        )
        .expect("sign");
    let floodfill = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    fixture
        .handle
        .service()
        .install_local_router_info(floodfill.clone(), now)
        .expect("floodfill record installed");
    let owner = SelfPublicationOwner::new(true);
    let decision = owner.decide(
        &fixture.bundle,
        Some(&floodfill),
        Ok(&material(address)),
        now,
    );
    assert_eq!(
        decision,
        SelfPublicationDecision::StoodDown(StandDownReason::FloodfillRecord),
        "the floodfill path owns its record's address"
    );
    assert_eq!(owner.counters().snapshot().stood_down_floodfill, 1);
    // The installed record stands.
    assert_eq!(
        fixture.handle.service().installed_local_router_info(),
        Some(floodfill)
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn every_outcome_is_counted_under_its_own_reason() {
    let fixture = Fixture::start().await;
    let now = now_ms();
    let installed = fixture.install_plain(now);
    let owner = SelfPublicationOwner::new(true);
    // One refusal per reason the transport can give.
    for reason in [
        Ssu2PublicationUnavailable::Closed,
        Ssu2PublicationUnavailable::NoBoundSocket,
        Ssu2PublicationUnavailable::StateUnavailable,
        Ssu2PublicationUnavailable::ReachabilityUnqualified,
        Ssu2PublicationUnavailable::InvalidPublication,
        Ssu2PublicationUnavailable::NoIntroducers,
    ] {
        let _ = owner.decide(&fixture.bundle, Some(&installed), Err(reason), now);
    }
    let counters = owner.counters().snapshot();
    assert_eq!(counters.withheld_closed, 1);
    assert_eq!(counters.withheld_no_bound_socket, 1);
    assert_eq!(counters.withheld_state_unavailable, 1);
    assert_eq!(counters.withheld_reachability_unqualified, 1);
    assert_eq!(counters.withheld_invalid_publication, 1);
    assert_eq!(counters.withheld_no_introducers, 1);
    assert_eq!(
        counters.withheld_reachability_unqualified
            + counters.withheld_closed
            + counters.withheld_no_bound_socket
            + counters.withheld_state_unavailable
            + counters.withheld_invalid_publication
            + counters.withheld_no_introducers,
        6,
        "every refusal must be counted under its own reason"
    );
    fixture.shutdown().await;
}
