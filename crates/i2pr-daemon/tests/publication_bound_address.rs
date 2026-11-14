//! Plan 546's lane — the publication interlock is an operator precondition.
//!
//! The router could publish an SSU2 address only when bound to **loopback**,
//! so the address it published was `127.0.0.1` and publication stayed inside a
//! loopback lab. The loopback arm was never an architectural invariant: it
//! exists to stop the router publishing an address it cannot serve, because a
//! router that advertises a reachability it does not have gets blocked by the
//! peers it wastes.
//!
//! This lane proves the **shape of the relaxation**:
//!
//! - the default configuration still refuses every non-loopback bind;
//! - loopback is publishable without any opt-in, as it always was;
//! - a private or public bind becomes publishable **only** with the operator's
//!   explicit key;
//! - an unbound or unspecified endpoint is never publishable;
//! - the floodfill role accepts the runtime's material only when the same key
//!   is set, refuses it with the same reason when it is not, and never invents
//!   material the runtime did not produce.
//!
//! The lane uses fixed seeds, an explicit clock, no socket, no environment
//! variable, no `#[ignore]`, and no sleeping.
//!
//! What the lane does **not** claim: that the published address is reachable.
//! That is the operator's precondition (`ssu2.publish_bound_address`) and the
//! plan records it as such; behind NAT the claim would be false, which is why
//! the key is off by default and why the plan names a peer-test successor.

#![forbid(unsafe_code)]

use i2pr_daemon::config::Config;
use i2pr_daemon::floodfill::{NormalIneligibility, NormalReadiness, evaluate_normal_eligibility};
use i2pr_proto::{Date, Mapping, RouterAddress};
use i2pr_runtime::{
    Ssu2PublicationForm, Ssu2PublicationMaterial, Ssu2RuntimeConfig, may_publish_bound_address,
};
use i2pr_transport::ReachabilityState;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

/// The clock the lane's addresses are dated around.
const NOW_MS: u64 = 1_700_000_000_000;

/// A loopback endpoint.
fn loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

/// A private-LAN endpoint, the shape this plan's measurement uses.
fn private_lan(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 182, 20)), port)
}

/// A publicly routable endpoint.
fn public(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)), port)
}

/// The router's own SSU2 address for `bound`, in the qualified direct form.
fn address(bound: SocketAddr) -> RouterAddress {
    let options = Mapping::from_entries(vec![
        ("caps".to_owned(), "4".to_owned()),
        ("host".to_owned(), bound.ip().to_string()),
        (
            "i".to_owned(),
            i2pr_netdb::encode(&[0x11; 32]).expect("intro"),
        ),
        ("mtu".to_owned(), "1280".to_owned()),
        ("port".to_owned(), bound.port().to_string()),
        (
            "s".to_owned(),
            i2pr_netdb::encode(&[0x22; 32]).expect("static"),
        ),
        ("v".to_owned(), "2".to_owned()),
    ])
    .expect("options");
    RouterAddress::new(10, Date::from_millis(NOW_MS), "SSU2".to_owned(), options).expect("address")
}

/// The transport's material for `bound`.
fn material(bound: SocketAddr) -> Ssu2PublicationMaterial {
    Ssu2PublicationMaterial {
        address: address(bound),
        form: Ssu2PublicationForm::Direct,
        advertises_introducer_role: false,
        evidence_expires_at: Duration::from_secs(3_600),
        reachability: ReachabilityState::Reachable,
        bound_family: i2pr_transport::AddressFamily::Ipv4,
    }
}

/// Readiness in which every measured signal is healthy.
fn ready<'a>(
    material: Option<&'a Ssu2PublicationMaterial>,
    bound_address_publishable: bool,
) -> NormalReadiness<'a> {
    NormalReadiness {
        material,
        bound_address_publishable,
        wall_now_ms: NOW_MS,
        netdb_ready: true,
        storage_ready: true,
        maintenance_ready: true,
        resource_headroom: true,
        clock_sane: true,
        supervision_healthy: true,
    }
}

#[test]
fn the_default_runtime_config_refuses_a_bound_address() {
    assert!(
        !Ssu2RuntimeConfig::default().publish_bound_address,
        "the opt-in must be off in the default configuration"
    );
}

#[test]
fn a_loopback_bind_is_publishable_without_the_opt_in() {
    assert!(
        may_publish_bound_address(loopback(19_100), false),
        "loopback publication must not require the operator's key"
    );
}

#[test]
fn a_private_bind_is_refused_without_the_opt_in() {
    assert!(
        !may_publish_bound_address(private_lan(19_100), false),
        "a private bind must not be publishable without the key"
    );
}

#[test]
fn a_private_bind_is_publishable_with_the_opt_in() {
    assert!(
        may_publish_bound_address(private_lan(19_100), true),
        "the operator's key must admit the bound address"
    );
}

#[test]
fn a_public_bind_is_publishable_with_the_opt_in() {
    assert!(
        may_publish_bound_address(public(19_100), true),
        "the same key must admit a public bind, which live testing needs"
    );
}

#[test]
fn an_unbound_port_is_never_publishable() {
    for opt_in in [false, true] {
        assert!(
            !may_publish_bound_address(private_lan(0), opt_in),
            "port 0 must never be publishable, opt-in or not"
        );
        assert!(
            !may_publish_bound_address(loopback(0), opt_in),
            "loopback with port 0 must never be publishable"
        );
    }
}

#[test]
fn an_unspecified_bind_is_never_publishable() {
    let unspecified = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 19_100);
    for opt_in in [false, true] {
        assert!(
            !may_publish_bound_address(unspecified, opt_in),
            "0.0.0.0 must never be publishable, opt-in or not"
        );
    }
}

#[test]
fn the_configuration_key_is_off_unless_the_operator_sets_it() {
    let absent = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
    assert!(
        !Config::parse(absent)
            .expect("strict profile")
            .ssu2
            .publish_bound_address,
        "an absent key must parse as the default, which is off"
    );
    let present = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\npublish_bound_address = true\n";
    assert!(
        Config::parse(present)
            .expect("strict profile")
            .ssu2
            .publish_bound_address,
        "the key must be honoured when the operator sets it"
    );
}

#[test]
fn the_floodfill_refuses_the_address_without_the_opt_in() {
    let material = material(private_lan(19_100));
    let (snapshot, reasons) = evaluate_normal_eligibility(true, &ready(Some(&material), false));
    assert!(
        !snapshot.eligible(),
        "a private address must not qualify without the key"
    );
    assert!(
        reasons.contains(&NormalIneligibility::AddressUnqualified),
        "the refusal must name the address, not something else: {reasons:?}"
    );
}

#[test]
fn the_floodfill_accepts_the_runtime_material_with_the_opt_in() {
    let material = material(private_lan(19_100));
    let (snapshot, reasons) = evaluate_normal_eligibility(true, &ready(Some(&material), true));
    assert!(
        snapshot.eligible(),
        "every signal is healthy and the operator opted in: {reasons:?}"
    );
    assert!(
        reasons.is_empty(),
        "an eligible evaluation carries no reason"
    );
}

#[test]
fn the_floodfill_does_not_invent_material_from_the_opt_in() {
    // The key says "the address the runtime produced may be published". It
    // does not say "publish something"; with no material there is no address,
    // so the role must still refuse for want of one.
    let (snapshot, reasons) = evaluate_normal_eligibility(true, &ready(None, true));
    assert!(
        !snapshot.eligible(),
        "the opt-in must not make an absent address eligible"
    );
    assert!(
        reasons.contains(&NormalIneligibility::AddressUnqualified),
        "an absent address must still be named as the reason: {reasons:?}"
    );
}

#[test]
fn the_reachability_gate_is_untouched_by_the_opt_in() {
    // A material whose reachability is not `Reachable` is refused whatever the
    // operator said about the address: the relaxation admits an address, not
    // an unreachable one.
    let mut material = material(private_lan(19_100));
    material.reachability = ReachabilityState::Firewalled;
    let (snapshot, reasons) = evaluate_normal_eligibility(true, &ready(Some(&material), true));
    assert!(
        !snapshot.eligible(),
        "an unreachable address must not qualify even with the key"
    );
    assert!(
        reasons.contains(&NormalIneligibility::ReachabilityUnconfirmed),
        "the refusal must name reachability: {reasons:?}"
    );
}

/// Asserts that `readiness` refuses and names `expected`.
fn assert_refuses_with(readiness: &NormalReadiness<'_>, expected: NormalIneligibility) {
    let (snapshot, reasons) = evaluate_normal_eligibility(true, readiness);
    assert!(
        !snapshot.eligible(),
        "{expected:?} must still refuse with the opt-in set"
    );
    assert!(
        reasons.contains(&expected),
        "{expected:?} must be named: {reasons:?}"
    );
}

#[test]
fn the_opt_in_changes_nothing_about_the_other_signals() {
    // Sweep each measured signal off, one at a time, with the key set: each
    // must still refuse, and each must name itself.
    let material = material(private_lan(19_100));

    let mut readiness = ready(Some(&material), true);
    readiness.netdb_ready = false;
    assert_refuses_with(&readiness, NormalIneligibility::NetdbNotReady);

    let mut readiness = ready(Some(&material), true);
    readiness.storage_ready = false;
    assert_refuses_with(&readiness, NormalIneligibility::StorageNotReady);

    let mut readiness = ready(Some(&material), true);
    readiness.maintenance_ready = false;
    assert_refuses_with(&readiness, NormalIneligibility::MaintenanceNotReady);

    let mut readiness = ready(Some(&material), true);
    readiness.resource_headroom = false;
    assert_refuses_with(&readiness, NormalIneligibility::ResourceExhausted);

    let mut readiness = ready(Some(&material), true);
    readiness.clock_sane = false;
    assert_refuses_with(&readiness, NormalIneligibility::ClockUnsane);

    let mut readiness = ready(Some(&material), true);
    readiness.supervision_healthy = false;
    assert_refuses_with(&readiness, NormalIneligibility::SupervisionUnhealthy);
}
