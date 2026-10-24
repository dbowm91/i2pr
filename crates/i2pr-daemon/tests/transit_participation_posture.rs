//! Plan 492 offline controls for the transit participation posture.
//!
//! These are the *offline* controls: they need no peer, no network, no
//! public bind, and no external router. They prove the posture's
//! invariants against the production owner directly, so a drift in the
//! policy is caught without a lane.
//!
//! The controls are:
//!
//! 1. **The default configuration does not participate.** The default
//!    posture refuses everything, and the refusal is counted.
//! 2. **An opt-in requires an enforced share.** Without an enabled
//!    bandwidth policy the configuration is refused; without a governor the
//!    owner refuses every request rather than treating the share limit as
//!    advisory.
//! 3. **The share limit is enforced.** Exceeding it refuses the relay and
//!    the bytes do not grow.
//! 4. **The participating-tunnel count is enforced** and counted.
//! 5. **The per-peer participation cap is enforced** and counted.
//! 6. **Revocation is prompt and observable.** A policy change, a
//!    withdrawn permit, and a health loss each stop participation, count
//!    the stop, and record it.
//! 7. **A restart re-derives the accounting from zero** and starts from
//!    the configured posture.
//! 8. **The receipts carry counts only** and are bounded.

#![forbid(unsafe_code)]

use i2pr_daemon::config::{
    Config, ConfigError, DEFAULT_TRANSIT_PARTICIPATING_TUNNELS, DEFAULT_TRANSIT_PEER_PARTICIPATION_CAP,
    DEFAULT_TRANSIT_SHARE_LIMIT, MAX_TRANSIT_SHARE_LIMIT_PERCENT,
};
use i2pr_daemon::transit_participation::{
    MAX_TRANSIT_PARTICIPATION_RECEIPTS, MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS,
    TransitParticipationBinding, TransitParticipationOwner, TransitParticipationPolicy,
    TransitReceiptKind, TransitRefusalReason, TransitRevocationReason, TRANSIT_SHARE_WINDOW_SECONDS,
};
use i2pr_proto::Hash;
use i2pr_runtime::{BandwidthGovernor, BandwidthGovernorConfig};

const MINIMAL: &str = "\
schema_version = 1

[router]
data_dir = \"./state\"
";

fn governor(outbound: u64) -> BandwidthGovernor {
    BandwidthGovernor::new(BandwidthGovernorConfig {
        inbound_bytes_per_second: outbound,
        outbound_bytes_per_second: outbound,
        burst_bytes: 64 * 1024,
        control_reserve_percent: 10,
        max_pending_requests: 16,
        max_pending_per_peer: 2,
    })
    .expect("valid governor")
}

fn peer(byte: u8) -> Hash {
    Hash::from_bytes([byte; 32])
}

/// A configuration with the transit posture enabled and an enforced share.
fn enabled_config(share_limit: u8, tunnels: usize, peer_cap: usize) -> Config {
    let text = format!(
        "{MINIMAL}\n[ssu2]\nenabled = true\n[bandwidth]\nenabled = true\ninbound_bytes_per_second = 1048576\noutbound_bytes_per_second = 1048576\nshare_percent = 50\n[transit]\nparticipation = true\nshare_limit = {share_limit}\nparticipating_tunnels = {tunnels}\npeer_participation_cap = {peer_cap}\n"
    );
    Config::parse(&text).expect("valid transit posture")
}

#[test]
fn the_default_configuration_does_not_participate() {
    let default = Config::parse(MINIMAL).expect("default config");
    assert!(!default.transit.participation);
    assert_eq!(default.transit.share_limit, DEFAULT_TRANSIT_SHARE_LIMIT);
    assert_eq!(
        default.transit.participating_tunnels,
        DEFAULT_TRANSIT_PARTICIPATING_TUNNELS
    );
    assert_eq!(
        default.transit.peer_participation_cap,
        DEFAULT_TRANSIT_PEER_PARTICIPATION_CAP
    );

    let mut owner = TransitParticipationOwner::from_config(&default.transit, None, 0)
        .expect("the default posture is well formed");
    assert!(!owner.is_enabled());
    assert_eq!(
        owner.admit(&peer(1), 0, 0, 0),
        Err(TransitRefusalReason::ParticipationDisabled)
    );
    assert_eq!(
        owner.charge_relay(1_024, 0),
        Err(TransitRefusalReason::ParticipationDisabled)
    );
    assert_eq!(owner.refusals().total(), 2);
    assert_eq!(owner.bytes_relayed(), 0);
    assert_eq!(owner.participating_tunnels(), 0);
}

#[test]
fn an_opt_in_requires_an_enforced_share() {
    let without_governor = format!("{MINIMAL}\n[transit]\nparticipation = true\n");
    assert!(matches!(
        Config::parse(&without_governor),
        Err(ConfigError::Semantic {
            field: "transit.participation",
            ..
        })
    ));

    // With the configuration accepted but no governor present, the owner
    // still refuses: the share limit is never advisory.
    let config = enabled_config(25, 4, 2);
    let mut owner = TransitParticipationOwner::from_config(&config.transit, None, 50)
        .expect("the posture is well formed");
    assert!(owner.is_enabled());
    assert_eq!(owner.share_bytes_per_second(), 0);
    assert_eq!(
        owner.admit(&peer(1), 0, 0, 0),
        Err(TransitRefusalReason::NoEnforcedPermit)
    );
}

#[test]
fn the_share_limit_is_enforced_rather_than_growing() {
    let config = enabled_config(25, 4, 2);
    let governor = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");
    let budget = owner.share_bytes_per_second();
    assert_eq!(budget, 128 * 1024);

    assert!(owner.charge_relay(budget, 0).is_ok());
    assert_eq!(
        owner.charge_relay(1, 0),
        Err(TransitRefusalReason::ShareLimitExceeded)
    );
    assert_eq!(owner.bytes_relayed(), budget);
    assert_eq!(
        owner.refusals().count(TransitRefusalReason::ShareLimitExceeded),
        1
    );

    // The next window starts from zero rather than carrying the debt.
    assert!(owner.charge_relay(budget, TRANSIT_SHARE_WINDOW_SECONDS).is_ok());
    assert_eq!(owner.bytes_relayed(), budget * 2);
}

#[test]
fn the_participating_tunnel_count_is_enforced_and_counted() {
    let config = enabled_config(25, 2, 2);
    let governor = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");

    assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
    assert!(owner.admit(&peer(2), 1, 0, 0).is_ok());
    assert_eq!(
        owner.admit(&peer(3), 2, 0, 0),
        Err(TransitRefusalReason::ParticipatingTunnelLimit)
    );
    assert_eq!(
        owner.refusals()
            .count(TransitRefusalReason::ParticipatingTunnelLimit),
        1
    );
    assert_eq!(owner.participating_tunnels(), 2);
}

#[test]
fn the_per_peer_participation_cap_is_enforced_and_counted() {
    let config = enabled_config(25, 8, 1);
    let governor = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");

    assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
    assert_eq!(
        owner.admit(&peer(1), 1, 1, 0),
        Err(TransitRefusalReason::PeerParticipationCap)
    );
    assert_eq!(
        owner.refusals().count(TransitRefusalReason::PeerParticipationCap),
        1
    );
    // Another peer is still admitted: the cap is per peer.
    assert!(owner.admit(&peer(2), 1, 0, 0).is_ok());
}

#[test]
fn a_policy_change_to_off_stops_participation_and_is_recorded() {
    let config = enabled_config(25, 4, 2);
    let governor = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");
    assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());

    let off = Config::parse(&format!("{MINIMAL}\n[transit]\n")).expect("default posture");
    assert!(owner.apply_config(&off.transit, None, 0));
    assert!(!owner.is_enabled());
    assert_eq!(owner.participating_tunnels(), 0);
    assert_eq!(owner.stops(), 1);
    assert_eq!(
        owner.admit(&peer(2), 0, 0, 0),
        Err(TransitRefusalReason::ParticipationDisabled)
    );

    let receipts = owner.receipts();
    assert!(
        receipts
            .iter()
            .any(|receipt| receipt.kind == TransitReceiptKind::Stopped),
        "a stop must be recorded"
    );
}

#[test]
fn a_withdrawn_permit_stops_participation_and_is_recorded() {
    let config = enabled_config(25, 4, 2);
    let issuing = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&issuing), 50).expect("owner");
    assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());

    // The live governor is replaced: the permit the owner holds is stale.
    let replacement = governor(1024 * 1024);
    assert!(!owner.revalidate(Some(&replacement)));
    assert_eq!(
        owner.revocation(),
        Some(TransitRevocationReason::PermitWithdrawn)
    );
    assert_eq!(owner.participating_tunnels(), 0);
    assert_eq!(owner.stops(), 1);
    assert_eq!(owner.admit(&peer(2), 0, 0, 0), Err(TransitRefusalReason::Revoked));
}

#[test]
fn a_health_loss_stops_participation_and_is_recorded() {
    let config = enabled_config(25, 4, 2);
    let issuing = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&issuing), 50).expect("owner");
    assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());

    // The issuing governor is gone: the permit is no longer healthy.
    drop(issuing);
    assert!(!owner.revalidate(None));
    assert_eq!(owner.revocation(), Some(TransitRevocationReason::HealthLost));
    assert_eq!(owner.participating_tunnels(), 0);
    assert_eq!(owner.stops(), 1);
}

#[test]
fn a_restart_re_derives_the_accounting_from_zero() {
    let config = enabled_config(25, 4, 2);
    let governor = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");
    assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
    assert!(owner.charge_relay(4_096, 0).is_ok());
    assert_eq!(owner.bytes_relayed(), 4_096);

    let restarted =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");
    assert_eq!(restarted.bytes_relayed(), 0);
    assert_eq!(restarted.window_bytes(), 0);
    assert_eq!(restarted.participating_tunnels(), 0);
    assert_eq!(restarted.peers_participating(), 0);
    assert_eq!(restarted.snapshot().admitted, 0);
    assert_eq!(restarted.receipt_count(), 0);
}

#[test]
fn the_receipts_are_bounded_and_carry_counts_only() {
    let config = enabled_config(25, 32, 32);
    let governor = governor(1024 * 1024);
    let mut owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");
    for index in 0..(MAX_TRANSIT_PARTICIPATION_RECEIPTS + 10) {
        assert!(owner.admit(&peer(index as u8), 0, 0, 0).is_ok());
    }
    assert_eq!(owner.receipt_count(), MAX_TRANSIT_PARTICIPATION_RECEIPTS);
    for receipt in owner.receipts() {
        assert!(receipt.participating_tunnels <= 32);
        assert!(receipt.peers_participating <= MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS);
        assert!(receipt.bytes_relayed <= owner.share_bytes_per_second());
    }
}

#[test]
fn the_binding_revalidates_the_permit_at_the_point_of_use() {
    let config = enabled_config(25, 4, 2);
    let live_governor = governor(1024 * 1024);
    let owner = TransitParticipationOwner::from_config(
        &config.transit,
        Some(&live_governor),
        50,
    )
    .expect("owner");
    let mut binding = TransitParticipationBinding::new(owner, Some(live_governor));
    assert!(binding.revalidate());
    assert!(binding.snapshot().enabled);

    // A binding without the issuing governor cannot keep the permit
    // healthy: the point of use refuses and the stop is recorded.
    let config = enabled_config(25, 4, 2);
    let issuing = governor(1024 * 1024);
    let owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&issuing), 50).expect("owner");
    drop(issuing);
    let mut binding = TransitParticipationBinding::new(owner, None);
    assert!(!binding.revalidate());
    assert_eq!(
        binding.snapshot().revocation,
        Some(TransitRevocationReason::HealthLost)
    );
    assert_eq!(binding.snapshot().stops, 1);
}

#[test]
fn the_policy_bounds_are_named_and_bounded() {
    // The share limit is a percentage: zero and over one hundred are both
    // refused.
    assert!(TransitParticipationPolicy::new(0, 4, 2).is_err());
    assert!(
        TransitParticipationPolicy::new(MAX_TRANSIT_SHARE_LIMIT_PERCENT + 1, 4, 2).is_err()
    );
    assert!(TransitParticipationPolicy::new(100, 4, 2).is_ok());
    // The tunnel count and the per-peer cap are bounded, and the per-peer
    // cap may not exceed the router-wide count.
    assert!(TransitParticipationPolicy::new(25, 0, 2).is_err());
    assert!(TransitParticipationPolicy::new(25, 33, 2).is_err());
    assert!(TransitParticipationPolicy::new(25, 4, 0).is_err());
    assert!(TransitParticipationPolicy::new(25, 4, 9).is_err());
    assert!(TransitParticipationPolicy::new(25, 4, 5).is_err());
    assert!(TransitParticipationPolicy::new(25, 4, 4).is_ok());
}
