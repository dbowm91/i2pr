//! Plan 512 out-of-crate lane for the destination-role tunnel liveness.
//!
//! Plan 487's F3 recorded that the destination inbound pool has no
//! transit-hop failure detection: a dead transit hop is invisible to the
//! router until the tunnel expires, so a destination whose tunnels die
//! with a hop is not replenished until the lease expires.
//!
//! This lane proves the **behaviour** the Plan 512 mechanism is supposed to
//! produce, through the **public** daemon API only — no private internals, no
//! `pub(crate)` reach-ins.
//!
//! The lane is local and deterministic:
//!
//! - fixed seeds (a fixed-seed ChaCha);
//! - an explicit `now_ms` everywhere — the scheduler takes the clock as an
//!   argument and the lane never reads a wall clock;
//! - no socket, no environment variable, no `#[ignore]`, no stock peer, and
//!   no sleeping.
//!
//! What the lane proves, one assertion per test:
//!
//! 1. the destination population pairs a destination's own outbound role with
//!    its own inbound role;
//! 2. the population never pairs **across** destinations — a dead outbound
//!    must never fail another destination's inbound;
//! 3. a destination with no outbound role produces no pair;
//! 4. the destination probe reads the inbound's gateway route from the shared
//!    registry, and refuses a receive id the registry does not hold;
//! 5. the destination composition does **not** impose the exploratory
//!    same-peer precondition, because the destination role's two directions
//!    are built in separate rounds from separate peer selections;
//! 6. a *matching* authenticated response proves the destination pair;
//! 7. a pair that has **never** answered is **never** torn down — the
//!    proven-pair gate, the safety property;
//! 8. a **proven** pair that misses two consecutive responses is reported
//!    unhealthy;
//! 9. the failure decision fails the destination's inbound pool slot;
//! 10. the destination's **last** inbound tunnel is never failed, and the
//!     refusal is counted;
//! 11. a failing slot without a binding is not a destination failure;
//! 12. failing the destination's tunnel removes its lease from the pool, and
//!     the changed lease set is the *tunnel set change* that must be
//!     republished;
//! 13. the binding table is bounded.
//!
//! One behaviour is **not** drivable through the public API and is reported
//! rather than papered over: the product's own destination-role binding table
//! is private and is filled by the live build path, which needs a socket. The
//! table's *shape* and the decision it drives are public and are proved here;
//! the wiring that fills it is covered by the structural guard
//! (`destination_liveness_boundary.rs`).

#![forbid(unsafe_code)]

use i2pr_client::{
    DestinationConfig, DestinationId, DestinationIdentity, DestinationTunnelPool,
    InboundLeaseSource, LeaseSetDecision, LeaseSetLifecycle, LeaseSetRotationCause,
};
use i2pr_daemon::exploratory_build::{BuildDirection, ExploratoryBuildCoordinator};
use i2pr_daemon::router_i2np::MAX_ROUTER_I2NP_BYTES;
use i2pr_daemon::service_product::{
    DestinationLivenessFailureDecision, DestinationRoleBinding, MAX_DESTINATION_ROLE_BINDINGS,
    destination_liveness_failure_decision, destination_liveness_pairs,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LIVENESS_PROBE_MESSAGE_ID_BASE, LIVENESS_RESPONSE_TIMEOUT_MS,
    LivenessAction, LivenessConfig, TunnelLivenessScheduler, delivery_status_message,
    route_inbound_with_liveness,
};
use i2pr_proto::{Date, Hash, I2npMessage};
use i2pr_runtime::Ssu2InboundI2np;
use i2pr_transport::{LinkId, PeerId};
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::established::{
    EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use i2pr_tunnel::pool::TunnelSlot;
use rand_chacha::ChaCha8Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};

/// The lane's fixed instant. Every clock read in this lane is derived from
/// this constant; no wall clock is ever read.
const NOW_MS: u64 = 1_700_000_000_000;
/// The same instant in seconds, for the APIs that count in seconds.
const NOW_SECONDS: u64 = NOW_MS / 1000;
/// The fixed seed of the lane's ChaCha.
const SEED: u64 = 0x5041_4E35_0512_0001;
/// The peer the destination role's outbound tunnel terminates at.
const OUTBOUND_PEER_BYTE: u8 = 0x51;
/// The peer the destination role's inbound tunnel's gateway is. It is
/// deliberately **not** the outbound's peer: the destination role's two
/// directions are built in separate rounds from separate peer selections.
const GATEWAY_PEER_BYTE: u8 = 0x52;

/// Builds a coordinator with the lane's fixed clock.
fn coordinator() -> ExploratoryBuildCoordinator {
    let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coordinator.advance_time(NOW_MS);
    coordinator
}

/// The layer keys of one synthetic hop.
fn layer_keys(seed: u8) -> i2pr_tunnel::build_crypto::LayerKeys {
    i2pr_tunnel::build_crypto::LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

/// A peer by its distinguishing byte.
fn peer(byte: u8) -> TunnelPeer {
    TunnelPeer::from_hash(Hash::from_bytes([byte; 32]))
}

/// The established outbound material of a destination role.
///
/// The outbound tunnel terminates at the outbound peer, which is the path the
/// liveness probe leaves through.
fn outbound_material(peer_byte: u8, seed: u8) -> EstablishedTunnel {
    let hops = vec![EstablishedHop::terminal(
        peer(peer_byte),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x0A00 + u32::from(seed)).expect("tunnel id"),
        layer_keys(0x10),
    )];
    EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0B00 + u32::from(seed)).expect("tunnel id"),
        hops,
        NOW_SECONDS,
        None,
        None,
    )
    .expect("outbound established")
}

/// The established inbound material of a destination role.
///
/// The inbound tunnel's gateway is the gateway peer, and its local endpoint
/// receive id is the registry's key for the role's typed gateway route.
fn inbound_material(peer_byte: u8, seed: u8) -> (EstablishedTunnel, TunnelId) {
    let local_receive = TunnelId::new(0x0C00 + u32::from(seed)).expect("tunnel id");
    let gateway_tunnel = TunnelId::new(0x0D00 + u32::from(seed)).expect("tunnel id");
    let hops = vec![EstablishedHop::with_next(
        peer(peer_byte),
        EstablishedRole::InboundGateway,
        gateway_tunnel,
        layer_keys(0x20),
        EstablishedNextHop::new(peer(peer_byte), local_receive),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0E00 + u32::from(seed)).expect("tunnel id"),
        hops,
        NOW_SECONDS,
        Some((peer(peer_byte), gateway_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established");
    (tunnel, local_receive)
}

/// The lane's first destination id.
fn destination_a() -> DestinationId {
    DestinationId::from_hash(Hash::from_bytes([0x31; 32]))
}

/// The lane's second destination id.
fn destination_b() -> DestinationId {
    DestinationId::from_hash(Hash::from_bytes([0x32; 32]))
}

/// Installs one destination-role outbound and one destination-role inbound in
/// the shared registry and returns the coordinator's role slots.
fn install_destination_role(
    coordinator: &mut ExploratoryBuildCoordinator,
    destination: u8,
) -> (TunnelSlot, TunnelSlot, TunnelId) {
    let outbound_slot = coordinator
        .activate_destination_tunnel(
            BuildDirection::Outbound,
            outbound_material(OUTBOUND_PEER_BYTE, destination),
            NOW_SECONDS,
        )
        .expect("destination outbound role");
    let (inbound_tunnel, local_receive) = inbound_material(GATEWAY_PEER_BYTE, destination);
    let inbound_slot = coordinator
        .activate_destination_tunnel(BuildDirection::Inbound, inbound_tunnel, NOW_SECONDS)
        .expect("destination inbound role");
    (outbound_slot, inbound_slot, local_receive)
}

/// Builds one binding table holding one destination's outbound and inbound
/// roles.
fn one_destination_bindings() -> std::collections::BTreeMap<TunnelSlot, DestinationRoleBinding> {
    let mut bindings = std::collections::BTreeMap::new();
    bindings.insert(
        TunnelSlot::from_raw(0x8000_0001),
        DestinationRoleBinding {
            destination_id: destination_a(),
            pool_slot: TunnelSlot::from_raw(11),
            direction: BuildDirection::Outbound,
            local_receive_tunnel: None,
        },
    );
    bindings.insert(
        TunnelSlot::from_raw(0x8000_0002),
        DestinationRoleBinding {
            destination_id: destination_a(),
            pool_slot: TunnelSlot::from_raw(12),
            direction: BuildDirection::Inbound,
            local_receive_tunnel: Some(TunnelId::new(0x0C01).expect("tunnel id")),
        },
    );
    bindings
}

/// Builds one inbound `DeliveryStatus` echo that acknowledges the supplied
/// message id.
fn delivery_status_inbound(acknowledged_message_id: u32, now_ms: u64) -> Ssu2InboundI2np {
    let body = delivery_status_message(acknowledged_message_id, now_ms);
    let expiration_ms = now_ms.saturating_add(LIVENESS_RESPONSE_TIMEOUT_MS);
    let message = I2npMessage::new_standard(0x0BAD_C0DE, Date::from_millis(expiration_ms), body)
        .expect("delivery status message");
    let bytes = message
        .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
        .expect("encoded delivery status");
    Ssu2InboundI2np {
        link_id: LinkId::new(1).expect("link id"),
        peer: PeerId::from_hash(Hash::from_bytes([OUTBOUND_PEER_BYTE; 32])),
        bytes,
    }
}

/// Builds a scheduler with the lane's one registered destination pair.
fn scheduler_with_one_pair() -> TunnelLivenessScheduler {
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(NOW_MS);
    scheduler
        .register_pair(
            TunnelSlot::from_raw(0x8000_0001),
            TunnelSlot::from_raw(0x8000_0002),
        )
        .expect("register pair");
    scheduler
}

/// Drives the scheduler past the first-test delay and returns the probe's
/// acknowledged-message id.
fn drive_first_test(scheduler: &mut TunnelLivenessScheduler) -> u32 {
    scheduler.advance_time(NOW_MS + FIRST_LIVENESS_DELAY_MS + 1);
    match scheduler.drive() {
        LivenessAction::SendTest {
            probe_message_id, ..
        } => probe_message_id,
        other => panic!("expected a SendTest action, got {other:?}"),
    }
}

/// Plan 512 requirement 1: the destination population pairs a destination's own
/// outbound role with its own inbound role.
#[test]
fn the_destination_population_pairs_the_outbound_and_the_inbound_of_one_destination() {
    let bindings = one_destination_bindings();
    let pairs = destination_liveness_pairs(&bindings);
    assert_eq!(
        pairs.len(),
        1,
        "one destination with one outbound and one inbound must produce one pair"
    );
    assert_eq!(pairs[0].outbound_slot, TunnelSlot::from_raw(0x8000_0001));
    assert_eq!(pairs[0].inbound_slot, TunnelSlot::from_raw(0x8000_0002));
    assert_eq!(
        pairs[0].local_receive_tunnel,
        TunnelId::new(0x0C01).expect("tunnel id"),
        "the pair must carry the inbound's local receive id"
    );
}

/// Plan 512 requirement 2: the population never pairs **across** destinations.
///
/// A cross-destination pair would let one destination's dead outbound fail
/// another destination's inbound, which is a false failure the plan must not
/// produce.
#[test]
fn the_destination_population_never_pairs_across_destinations() {
    let mut bindings = one_destination_bindings();
    bindings.insert(
        TunnelSlot::from_raw(0x8000_0003),
        DestinationRoleBinding {
            destination_id: destination_b(),
            pool_slot: TunnelSlot::from_raw(13),
            direction: BuildDirection::Outbound,
            local_receive_tunnel: None,
        },
    );
    bindings.insert(
        TunnelSlot::from_raw(0x8000_0004),
        DestinationRoleBinding {
            destination_id: destination_b(),
            pool_slot: TunnelSlot::from_raw(14),
            direction: BuildDirection::Inbound,
            local_receive_tunnel: Some(TunnelId::new(0x0C02).expect("tunnel id")),
        },
    );
    let pairs = destination_liveness_pairs(&bindings);
    assert_eq!(pairs.len(), 2, "two destinations must produce two pairs");
    for pair in &pairs {
        let outbound = bindings
            .get(&pair.outbound_slot)
            .expect("the outbound binding must exist");
        let inbound = bindings
            .get(&pair.inbound_slot)
            .expect("the inbound binding must exist");
        assert_eq!(
            outbound.destination_id, inbound.destination_id,
            "a pair must never join two destinations"
        );
    }
}

/// Plan 512 requirement 3: a destination with no outbound role produces no
/// pair, and an inbound with no local receive id is not paired either.
#[test]
fn the_destination_population_is_empty_without_a_complete_destination_role() {
    let mut bindings = one_destination_bindings();
    // Drop the outbound: an inbound with no path out cannot be probed.
    bindings.remove(&TunnelSlot::from_raw(0x8000_0001));
    assert!(
        destination_liveness_pairs(&bindings).is_empty(),
        "an inbound without an outbound must not produce a pair"
    );

    // An inbound whose local receive id is missing cannot be addressed.
    let mut bindings = one_destination_bindings();
    if let Some(binding) = bindings.get_mut(&TunnelSlot::from_raw(0x8000_0002)) {
        binding.local_receive_tunnel = None;
    }
    assert!(
        destination_liveness_pairs(&bindings).is_empty(),
        "an inbound without a local receive id must not produce a pair"
    );
}

/// Plan 512 requirement 4: the destination probe reads the inbound's gateway
/// route from the shared registry.
#[test]
fn the_destination_probe_reads_the_gateways_route_from_the_shared_registry() {
    let mut coordinator = coordinator();
    let (outbound_slot, _inbound_slot, local_receive) =
        install_destination_role(&mut coordinator, 1);
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);

    let cells = coordinator
        .compose_destination_liveness_probe(
            outbound_slot,
            local_receive,
            LIVENESS_PROBE_MESSAGE_ID_BASE,
            NOW_MS,
            &mut rng,
        )
        .expect("the destination probe must compose");
    assert!(
        !cells.is_empty(),
        "the destination probe must produce at least one cell"
    );
    let (target_peer, wire, _message_id) = &cells[0];
    assert_eq!(
        *target_peer,
        PeerId::from_hash(Hash::from_bytes([OUTBOUND_PEER_BYTE; 32])),
        "the probe must leave through the outbound tunnel's first hop"
    );
    assert!(!wire.is_empty(), "the probe must carry a wire payload");

    // A receive id the registry does not hold cannot be addressed: the
    // composition refuses rather than guessing a gateway.
    let unknown = TunnelId::new(0x7FFF_FFFF).expect("tunnel id");
    assert!(
        coordinator
            .compose_destination_liveness_probe(
                outbound_slot,
                unknown,
                LIVENESS_PROBE_MESSAGE_ID_BASE,
                NOW_MS,
                &mut rng,
            )
            .is_err(),
        "a receive id the registry does not hold must be refused"
    );
}

/// Plan 512 requirement 5: the destination composition does **not** impose the
/// exploratory same-peer precondition.
///
/// The destination role's two directions are built in separate rounds from
/// separate peer selections, so requiring one shared peer — as the
/// exploratory composition does — would leave the destination population
/// empty.
#[test]
fn the_destination_composition_does_not_require_the_pair_to_share_one_peer() {
    let mut coordinator = coordinator();
    let (outbound_slot, _inbound_slot, local_receive) =
        install_destination_role(&mut coordinator, 1);
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);

    // The outbound's first hop and the inbound's gateway are different
    // peers by construction.
    let (outbound_first_hop, _) = coordinator
        .registry_outbound_first_hop(outbound_slot)
        .expect("the outbound role must be in the registry");
    assert_eq!(
        outbound_first_hop,
        Hash::from_bytes([OUTBOUND_PEER_BYTE; 32]),
        "the outbound must terminate at the outbound peer"
    );

    // The composition succeeds for that pair.
    assert!(
        coordinator
            .compose_destination_liveness_probe(
                outbound_slot,
                local_receive,
                LIVENESS_PROBE_MESSAGE_ID_BASE,
                NOW_MS,
                &mut rng,
            )
            .is_ok(),
        "a destination pair whose tunnels do not share one peer must still compose"
    );

    // The exploratory composition, by contrast, refuses a pair that does
    // not share one peer — and it cannot even find the destination role's
    // inbound, because that inbound lives in the shared registry and not in
    // the exploratory pool.
    let exploratory = coordinator.compose_liveness_probe(
        outbound_slot,
        TunnelSlot::from_raw(0x8000_0002),
        LIVENESS_PROBE_MESSAGE_ID_BASE,
        NOW_MS,
        &mut rng,
    );
    assert!(
        exploratory.is_err(),
        "the exploratory composition must not accept a destination-role pair"
    );
}

/// Plan 512 requirement 6: a *matching* authenticated response proves the
/// destination pair.
#[test]
fn a_matching_authenticated_response_proves_the_destination_pair() {
    let mut coordinator = coordinator();
    let mut scheduler = scheduler_with_one_pair();
    let probe_message_id = drive_first_test(&mut scheduler);

    assert_eq!(
        scheduler.counters().unproven_pairs,
        1,
        "the pair starts unproven"
    );
    let inbound = delivery_status_inbound(probe_message_id, NOW_MS);
    let routed = route_inbound_with_liveness(&mut coordinator, &mut scheduler, &inbound, NOW_MS)
        .expect("the inbound route must succeed");
    assert!(
        routed.refreshed.is_some(),
        "the matching echo must refresh the pending test"
    );
    assert_eq!(
        scheduler.counters().unproven_pairs,
        0,
        "the answered pair must no longer count as unproven"
    );
    assert_eq!(scheduler.counters().successful_responses, 1);
}

/// Plan 512 requirement 7: a pair that has **never** answered is never torn
/// down — the proven-pair gate, the safety property the plan inherits from
/// Plan 504.
#[test]
fn an_unproven_destination_pair_is_never_torn_down() {
    let mut scheduler = scheduler_with_one_pair();
    drive_first_test(&mut scheduler);

    let mut now = NOW_MS + FIRST_LIVENESS_DELAY_MS + 1;
    for _ in 0..2 {
        now = now.saturating_add(LIVENESS_RESPONSE_TIMEOUT_MS + 1);
        scheduler.advance_time(now);
        let action = scheduler.drive();
        assert!(
            !matches!(action, LivenessAction::MarkUnhealthy { .. }),
            "an unproven pair must never be marked unhealthy, got {action:?}"
        );
    }
    assert_eq!(
        scheduler.active_pairs(),
        1,
        "the unproven pair must still be tracked"
    );
    assert_eq!(scheduler.counters().unhealthy_marks, 0);
    assert!(
        scheduler.counters().unproven_failures >= 1,
        "the withheld teardown must be counted so the gap is visible"
    );
}

/// Plan 512 requirement 8: a **proven** pair that misses two consecutive
/// responses is reported unhealthy, with the pair's slots so the pump can
/// route the failure to the destination that owns the inbound.
#[test]
fn a_proven_destination_pair_that_misses_two_consecutive_responses_is_reported_unhealthy() {
    let mut coordinator = coordinator();
    let mut scheduler = scheduler_with_one_pair();
    let probe_message_id = drive_first_test(&mut scheduler);

    // The peer answers once, so the pair becomes proven.
    let inbound = delivery_status_inbound(probe_message_id, NOW_MS);
    route_inbound_with_liveness(&mut coordinator, &mut scheduler, &inbound, NOW_MS)
        .expect("the inbound route must succeed");
    assert_eq!(scheduler.counters().unproven_pairs, 0);

    // The peer then stops answering.
    let mut now = NOW_MS + FIRST_LIVENESS_DELAY_MS + 1;
    let mut marked = None;
    for _ in 0..4 {
        now = now.saturating_add(LIVENESS_RESPONSE_TIMEOUT_MS + 1);
        scheduler.advance_time(now);
        if let LivenessAction::MarkUnhealthy {
            outbound_slot,
            inbound_slot,
            consecutive_failures,
        } = scheduler.drive()
        {
            marked = Some((outbound_slot, inbound_slot, consecutive_failures));
            break;
        }
    }
    let (outbound_slot, inbound_slot, consecutive_failures) =
        marked.expect("a proven pair that misses its responses must be marked unhealthy");
    assert_eq!(outbound_slot, TunnelSlot::from_raw(0x8000_0001));
    assert_eq!(inbound_slot, TunnelSlot::from_raw(0x8000_0002));
    assert!(
        consecutive_failures >= 2,
        "the failure threshold must be reached, got {consecutive_failures}"
    );
    assert_eq!(
        scheduler.active_pairs(),
        0,
        "the marked pair must be removed from the scheduler"
    );
    assert_eq!(scheduler.counters().unhealthy_marks, 1);
}

/// Plan 512 requirement 9: the failure decision fails the destination's
/// inbound pool slot.
#[test]
fn the_failure_decision_fails_the_destinations_inbound_pool_slot() {
    let bindings = one_destination_bindings();
    let binding = bindings
        .get(&TunnelSlot::from_raw(0x8000_0002))
        .expect("the inbound binding must exist");
    let decision = destination_liveness_failure_decision(Some(binding), 2);
    assert_eq!(
        decision,
        DestinationLivenessFailureDecision::Fail {
            pool_slot: TunnelSlot::from_raw(12)
        },
        "a destination with more than one inbound must have its inbound failed"
    );
}

/// Plan 512 requirement 10: the destination's **last** inbound tunnel is never
/// failed, exactly as i2pd 2.61.0 refuses to fail the last tunnel
/// (`libi2pd/TunnelPool.cpp:360`).
#[test]
fn the_last_inbound_tunnel_of_a_destination_is_never_failed() {
    let bindings = one_destination_bindings();
    let binding = bindings
        .get(&TunnelSlot::from_raw(0x8000_0002))
        .expect("the inbound binding must exist");
    let decision = destination_liveness_failure_decision(Some(binding), 1);
    assert_eq!(
        decision,
        DestinationLivenessFailureDecision::LastTunnelRefused,
        "the last inbound tunnel must never be failed"
    );
    assert_eq!(
        destination_liveness_failure_decision(Some(binding), 0),
        DestinationLivenessFailureDecision::LastTunnelRefused,
        "a destination with no inbound tunnel must never be failed either"
    );
}

/// Plan 512 requirement 11: a failing slot without a binding is not a
/// destination failure.
#[test]
fn a_failing_slot_without_a_binding_is_not_a_destination_failure() {
    assert_eq!(
        destination_liveness_failure_decision(None, 3),
        DestinationLivenessFailureDecision::Unbound,
        "an unbound slot must not be failed as a destination tunnel"
    );
}

/// Plan 512 requirement 12: failing the destination's tunnel removes its lease
/// from the pool, and the changed lease set is the *tunnel set change* that
/// must be republished.
///
/// The republication itself is the daemon's existing path: the tick notices
/// that the installed lease set no longer matches the pool's usable set and
/// marks the destination for republication. What this lane proves is the
/// *cause* — that a failed tunnel changes the lease set, and that the change
/// is classified as the tunnel-set change rather than the margin-driven
/// rotation.
#[test]
fn failing_the_destination_tunnel_changes_the_lease_set_and_must_be_republished() {
    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    let (first_tunnel, _) = inbound_material(GATEWAY_PEER_BYTE, 1);
    let (second_tunnel, _) = inbound_material(GATEWAY_PEER_BYTE, 2);
    let first_slot = pool
        .register_inbound(first_tunnel.into_extracted(), NOW_SECONDS)
        .expect("first inbound");
    pool.register_inbound(second_tunnel.into_extracted(), NOW_SECONDS)
        .expect("second inbound");

    let leases_before = pool.inbound_lease_sources(NOW_SECONDS);
    assert_eq!(leases_before.len(), 2, "both inbounds must be advertised");

    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let identity = DestinationIdentity::generate(&mut rng).expect("destination identity");
    let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());

    assert_eq!(
        lifecycle
            .refresh(&identity, &leases_before, NOW_SECONDS)
            .expect("first refresh"),
        LeaseSetDecision::Regenerate(LeaseSetRotationCause::InitialGeneration),
        "the first lease set must be generated"
    );
    assert_eq!(
        lifecycle
            .refresh(&identity, &leases_before, NOW_SECONDS)
            .expect("second refresh"),
        LeaseSetDecision::Retain,
        "an unchanged lease set must be retained"
    );

    // The liveness failure drops one tunnel from the pool.
    assert!(
        pool.mark_failed(first_slot),
        "the failed tunnel must be removed from the pool"
    );
    let leases_after = pool.inbound_lease_sources(NOW_SECONDS);
    assert_eq!(
        leases_after.len(),
        1,
        "the failed tunnel's lease must leave the advertised set"
    );
    assert_eq!(
        lifecycle
            .refresh(&identity, &leases_after, NOW_SECONDS)
            .expect("third refresh"),
        LeaseSetDecision::Regenerate(LeaseSetRotationCause::TunnelSetChanged),
        "a changed tunnel set must be republished"
    );
}

/// Plan 512 requirement 13: the binding table is bounded.
#[test]
fn the_binding_table_is_bounded() {
    let ceiling = MAX_DESTINATION_ROLE_BINDINGS;
    assert!(ceiling > 0, "the binding ceiling must be a real bound");
    assert!(
        ceiling <= 4096,
        "the binding ceiling must stay a small, bounded budget"
    );

    // The population can never exceed the binding table's size.
    let mut bindings = std::collections::BTreeMap::new();
    for index in 0..MAX_DESTINATION_ROLE_BINDINGS {
        let slot = TunnelSlot::from_raw(0x8000_0000 + index as u32);
        bindings.insert(
            slot,
            DestinationRoleBinding {
                destination_id: destination_a(),
                pool_slot: TunnelSlot::from_raw(index as u32),
                direction: if index % 2 == 0 {
                    BuildDirection::Outbound
                } else {
                    BuildDirection::Inbound
                },
                local_receive_tunnel: (index % 2 == 1)
                    .then(|| TunnelId::new(0x0C00 + index as u32).expect("tunnel id")),
            },
        );
    }
    let pairs = destination_liveness_pairs(&bindings);
    assert!(
        pairs.len() <= MAX_DESTINATION_ROLE_BINDINGS,
        "the population must never exceed the binding ceiling"
    );
}

/// The lane's own RNG, so a caller can derive deterministic material.
fn lane_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(SEED)
}

/// A helper that keeps the `CryptoRng` bound honest for the lane's own
/// helpers.
fn _assert_rng_bounds<R: CryptoRng + RngCore>(_rng: &R) {}

#[test]
fn the_lane_rng_is_deterministic() {
    let mut rng = lane_rng();
    _assert_rng_bounds(&rng);
    let first: [u8; 8] = {
        let mut buffer = [0_u8; 8];
        rng.fill_bytes(&mut buffer);
        buffer
    };
    let mut rng = lane_rng();
    let second: [u8; 8] = {
        let mut buffer = [0_u8; 8];
        rng.fill_bytes(&mut buffer);
        buffer
    };
    assert_eq!(first, second, "the lane's RNG must be deterministic");
}

/// The lane's lease sources must be the pool's own — never fabricated.
#[test]
fn the_lease_sources_are_the_pool_s_own() {
    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    let (tunnel, _) = inbound_material(GATEWAY_PEER_BYTE, 1);
    let slot = pool
        .register_inbound(tunnel.into_extracted(), NOW_SECONDS)
        .expect("inbound");
    let sources: Vec<InboundLeaseSource> = pool.inbound_lease_sources(NOW_SECONDS);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].slot(), slot);
    assert_eq!(
        sources[0].gateway(),
        Hash::from_bytes([GATEWAY_PEER_BYTE; 32]),
        "the advertised gateway must be the tunnel's own gateway"
    );
}
