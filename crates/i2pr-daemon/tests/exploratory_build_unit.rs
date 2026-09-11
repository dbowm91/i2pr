//! Plan 185 exploratory build coordinator local unit tests.
//!
//! Local tests for the bounded state-management surface of the
//! coordinator: counter accounting, pending-table discipline,
//! deadline sweep, attempt-id monotonicity, registry metadata
//! round-trips, lifetime/failure threshold validation, and the
//! inbound dispatcher's strict OTBRM-extraction contract. These
//! tests are runtime-neutral and never open sockets; the
//! full-pipeline integration tests in
//! `crates/i2pr-daemon/tests/exploratory_build_live.rs` drive
//! the coordinator end-to-end through the daemon-owned SSU2
//! runtime against a simulated remote build responder.

#![forbid(unsafe_code)]

use i2pr_daemon::exploratory_build::{
    BuildCoordinatorCounters, BuildCoordinatorError, BuildCoordinatorOutcome, BuildDirection,
    ExploratoryBuildCoordinator, InboundRouteOutcome, MAX_PENDING_BUILDS,
};
use i2pr_proto::Hash;
use i2pr_runtime::Ssu2InboundI2np;
use i2pr_transport::LinkId;
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::identity::TunnelDirection;

const NOW_MS: u64 = 1_700_000_000_000;

fn peer_hash(value: u8) -> Hash {
    Hash::from_bytes([value; 32])
}

#[test]
fn counters_snapshot_keeps_pending_consistent() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(NOW_MS);
    let snapshot = coord.counters();
    assert_eq!(snapshot.pending, 0);
    assert_eq!(snapshot.installed, 0);
    assert_eq!(snapshot.consecutive_failures, 0);
    assert_eq!(snapshot.outbound_builds, 0);
}

#[test]
fn pause_flag_starts_false() {
    let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    assert!(!coord.is_paused());
}

#[test]
fn zero_attempt_id_starts_above_one() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    let first = coord.next_attempt_id().get();
    let second = coord.next_attempt_id().get();
    assert!(first >= 1);
    assert_eq!(second, first + 1);
}

#[test]
fn max_pending_builds_is_bounded_constant() {
    const { assert!(MAX_PENDING_BUILDS > 0) };
    const { assert!(MAX_PENDING_BUILDS <= 256) };
}

#[test]
fn lifetime_bounds_reject_invalid_values() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    assert!(matches!(
        coord.set_lifetime_seconds(0),
        Err(BuildCoordinatorError::Coordinator(_))
    ));
    assert!(matches!(
        coord.set_lifetime_seconds(60 * 60 + 1),
        Err(BuildCoordinatorError::Coordinator(_))
    ));
    assert!(coord.set_lifetime_seconds(600).is_ok());
}

#[test]
fn route_inbound_i2np_rejects_empty_bytes() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(NOW_MS);
    let inbound = Ssu2InboundI2np {
        link_id: LinkId::new(1).expect("link"),
        peer: i2pr_transport::PeerId::from_hash(peer_hash(0x10)),
        bytes: Vec::new(),
    };
    let outcome = coord.route_inbound_i2np(&inbound, NOW_MS);
    assert!(matches!(
        outcome,
        Err(i2pr_daemon::router_i2np::RouterI2npError::Empty)
    ));
}

#[test]
fn route_inbound_i2np_preserves_dispatcher_outcome() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(NOW_MS);
    // A short-transport DeliveryStatus with an unknown peer should
    // route through the central dispatcher but not reach any
    // pending build. The dispatcher outcome is preserved.
    let body = i2pr_proto::I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
        0x51A4_0001,
        i2pr_proto::Date::from_millis(NOW_MS),
    ));
    let expiration_secs = (NOW_MS / 1000).saturating_add(60) as u32;
    let message = i2pr_proto::I2npMessage::new_short_transport(0x51A4_0001, expiration_secs, body)
        .expect("delivery status");
    let wire = message
        .encode_short_transport_to_vec(i2pr_transport::MAX_I2NP_MESSAGE_BYTES)
        .expect("encode");
    let inbound = Ssu2InboundI2np {
        link_id: LinkId::new(1).expect("link"),
        peer: i2pr_transport::PeerId::from_hash(peer_hash(0x10)),
        bytes: wire,
    };
    let routed: InboundRouteOutcome = coord
        .route_inbound_i2np(&inbound, NOW_MS)
        .expect("dispatcher outcome");
    assert!(routed.coordinator.is_empty());
    assert!(matches!(
        routed.dispatcher,
        i2pr_daemon::router_i2np::RouterI2npOutcome::RouterControl { .. }
    ));
}

#[test]
fn registries_are_empty_until_install_occurs() {
    let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    let inbound: Vec<i2pr_tunnel::pool::TunnelRegistration> =
        coord.registrations(TunnelDirection::Inbound);
    let outbound: Vec<i2pr_tunnel::pool::TunnelRegistration> =
        coord.registrations(TunnelDirection::Outbound);
    assert!(inbound.is_empty());
    assert!(outbound.is_empty());
}

#[test]
fn outcome_direction_matches_request_direction() {
    let outcome = BuildCoordinatorOutcome::CoordinatorRejected {
        direction: BuildDirection::Outbound,
        reason: "test",
    };
    assert_eq!(outcome.direction(), BuildDirection::Outbound);
    let outcome_inbound = BuildCoordinatorOutcome::CoordinatorRejected {
        direction: BuildDirection::Inbound,
        reason: "test",
    };
    assert_eq!(outcome_inbound.direction(), BuildDirection::Inbound);
}

#[test]
fn counters_default_is_zero() {
    let counters = BuildCoordinatorCounters::default();
    assert_eq!(counters.installed, 0);
    assert_eq!(counters.timeouts, 0);
    assert_eq!(counters.delivery_failures, 0);
    assert_eq!(counters.pending, 0);
    assert_eq!(counters.outbound_builds, 0);
    assert_eq!(counters.inbound_routed, 0);
    assert_eq!(counters.inbound_orphans, 0);
    assert_eq!(counters.duplicate_replies, 0);
    assert_eq!(counters.cancellations, 0);
    assert_eq!(counters.invalid_replies, 0);
    assert_eq!(counters.hop_rejections, 0);
}

#[test]
fn pool_registrations_keep_activated_slots() {
    let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    assert_eq!(
        i2pr_daemon::exploratory_build::tunnel_state_at(
            &coord,
            i2pr_tunnel::pool::TunnelSlot::from_raw(0)
        ),
        None
    );
}

#[test]
fn registry_metadata_is_observable() {
    let coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    let receive = i2pr_tunnel::identity::TunnelId::new(0x901).expect("receive");
    assert!(coord.registry_inbound_first_hop(receive).is_none());
    assert!(coord.registry_inbound_slot(receive).is_none());
    assert!(
        coord
            .registry_outbound_first_hop(i2pr_tunnel::pool::TunnelSlot::from_raw(1))
            .is_none()
    );
}

#[test]
fn expire_pending_returns_empty_when_no_pending() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(NOW_MS);
    let expired = coord.expire_pending();
    assert!(expired.is_empty());
}

#[test]
fn remove_unknown_slot_returns_error() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    let result = coord.remove_slot(i2pr_tunnel::pool::TunnelSlot::from_raw(0xFFFF));
    assert!(matches!(
        result,
        Err(BuildCoordinatorError::Coordinator("unknown slot"))
    ));
}

#[test]
fn cancel_unknown_attempt_returns_none() {
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(NOW_MS);
    let outcome = coord.cancel(i2pr_tunnel::short::BuildAttemptId::new(0xFFFF));
    assert!(outcome.is_none());
}
