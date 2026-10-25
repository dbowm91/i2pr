//! Plan 502 evidence lane: the transit data-plane owner in the product
//! composition, observed through the daemon's public API.
//!
//! The lane proves three things and nothing else:
//!
//! 1. **The default configuration installs no transit owner.** The
//!    composition constructs nothing at all — no service, no counters,
//!    no owner.
//! 2. **The opt-in configuration installs an enabled owner.** The same
//!    composition code path produces a live, enabled owner.
//! 3. **The default publication reports no participation.** The three
//!    Proposal-170 transit selectors report `participating = false` and
//!    zeros, and the zeros are the **absence** of a measurement: the
//!    `Disabled` posture owns no counters. No zero was substituted for a
//!    missing measurement.
//!
//! The lane needs no peer, no network, no public bind, and no external
//! router.

#![forbid(unsafe_code)]

use i2pr_crypto::{OsRng, RouterIdentityBundle};
use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol_inspection::proposal_transit_volume;
use i2pr_daemon::router_i2np::{RouterDeliveryService, generate_controlled_identity};
use i2pr_daemon::transit_composition::{TransitCompositionInputs, compose_transit_owner};
use i2pr_daemon::transit_volume::TransitParticipation;
use i2pr_runtime::{
    BandwidthGovernor, BandwidthGovernorConfig, CancellationToken, Ssu2RuntimeConfig,
    Ssu2RuntimeService,
};

const MINIMAL: &str = "\
schema_version = 1

[router]
data_dir = \"./state\"
";

/// The default configuration: the operator did not opt in.
fn default_config() -> Config {
    Config::parse(MINIMAL).expect("default config")
}

/// The opted-in configuration with an enforced bandwidth share.
fn opted_in_config() -> Config {
    let text = format!(
        "{MINIMAL}\n[ssu2]\nenabled = true\n[bandwidth]\nenabled = true\ninbound_bytes_per_second = 1048576\noutbound_bytes_per_second = 1048576\nshare_percent = 50\n[transit]\nparticipation = true\nshare_limit = 25\nparticipating_tunnels = 4\npeer_participation_cap = 2\n"
    );
    Config::parse(&text).expect("valid transit posture")
}

fn governor() -> BandwidthGovernor {
    BandwidthGovernor::new(BandwidthGovernorConfig {
        inbound_bytes_per_second: 1024 * 1024,
        outbound_bytes_per_second: 1024 * 1024,
        burst_bytes: 64 * 1024,
        control_reserve_percent: 10,
        max_pending_requests: 16,
        max_pending_per_peer: 2,
    })
    .expect("valid governor")
}

fn identity() -> RouterIdentityBundle {
    RouterIdentityBundle::generate(&mut OsRng).expect("identity")
}

fn delivery(bundle: &RouterIdentityBundle) -> RouterDeliveryService {
    let controlled =
        generate_controlled_identity(bundle, "127.0.0.1", 44_002).expect("controlled identity");
    let runtime =
        Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), controlled).expect("runtime");
    RouterDeliveryService::new(runtime)
}

/// The default configuration constructs **nothing**: the composition
/// returns no owner and never needs a delivery seam, a governor, or an
/// identity.
#[test]
fn the_default_configuration_installs_no_transit_owner() {
    let config = default_config();
    assert!(
        !config.transit.participation,
        "the default configuration must not opt in"
    );
    let inputs = TransitCompositionInputs {
        transit: config.transit,
        bandwidth: config.bandwidth,
        governor: None,
        delivery: None,
        cancellation: None,
        identity: None,
    };
    let owner = compose_transit_owner(&inputs).expect("the default path refuses nothing");
    assert!(
        owner.is_none(),
        "the default configuration must install no transit owner"
    );
}

/// The opt-in configuration installs an **enabled** owner through the
/// same composition code path the product runs.
#[test]
fn the_opted_in_configuration_installs_an_enabled_transit_owner() {
    let config = opted_in_config();
    let governor = governor();
    let bundle = identity();
    let inputs = TransitCompositionInputs {
        transit: config.transit,
        bandwidth: config.bandwidth,
        governor: Some(&governor),
        delivery: Some(delivery(&bundle)),
        cancellation: Some(CancellationToken::new()),
        identity: Some(&bundle),
    };
    let mut owner = compose_transit_owner(&inputs)
        .expect("the opted-in path constructs an owner")
        .expect("the opted-in path installs an owner");
    assert!(
        owner.is_enabled(),
        "the opted-in owner must be enabled, not a disabled shell"
    );
    assert!(
        owner.volume_counters().is_some(),
        "an installed owner owns the service's own volume counters"
    );
    // The owner is cancelled and drained deterministically on shutdown,
    // using the outer owner's real cancellation token.
    owner.cancel();
}

/// The default publication reports no participation, and the three
/// Proposal-170 transit selectors report zeros **because the `Disabled`
/// posture owns no counters** — not because a zero was substituted for a
/// missing measurement.
#[test]
fn the_default_publication_reports_no_participation_and_zeros() {
    let config = default_config();
    let (_graph, inspection) = i2pr_daemon::build_daemon_graph_with_inspection(&config)
        .expect("the default composition builds");
    let posture = inspection
        .transit_participation()
        .expect("the composition root publishes the transit posture");
    assert!(
        matches!(posture, TransitParticipation::Disabled),
        "the default posture owns no counters: {posture:?}"
    );
    // The `Disabled` posture owns no counters, so the selectors project
    // the absence of a measurement as zero.
    assert!(
        posture.counters().is_none(),
        "the disabled posture must own no counters"
    );
    assert_eq!(
        proposal_transit_volume("i2p.router.net.total.transit.bytes", &inspection)
            .expect("the byte selector has a value"),
        serde_json::json!(0)
    );
    assert_eq!(
        proposal_transit_volume("i2p.router.net.bw.transit.15s", &inspection)
            .expect("the bandwidth selector has a value"),
        serde_json::json!(0)
    );
    assert_eq!(
        proposal_transit_volume("i2p.router.net.tunnels.shareratio", &inspection)
            .expect("the shareratio selector has a value"),
        serde_json::json!(0.0)
    );
}
