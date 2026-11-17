//! Plan 504 out-of-crate lane for the live tunnel-liveness composition.
//!
//! The static guard in `crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs`
//! proves that the *wiring* is present: the composition constructs the
//! scheduler, the live inbound path routes through the liveness seam, the
//! tick drives the scheduler, and the correlation is authenticated. This lane
//! proves the *behaviour* that wiring is supposed to produce, through the
//! **public** daemon API only — no private internals, no `pub(crate)`
//! reach-ins.
//!
//! The lane is local and deterministic:
//!
//! - fixed seeds (a fixed-seed `RouterIdentityBundle`, a fixed-seed ChaCha);
//! - an explicit `now_ms` everywhere — the scheduler takes the clock as an
//!   argument and the lane never reads a wall clock;
//! - no socket: the SSU2 service is constructed but never started, so it binds
//!   nothing and delivers nothing;
//! - no environment variable, no `#[ignore]`, no stock peer, no sleeping.
//!
//! What the lane proves, one assertion per test:
//!
//! 1. the scheduler emits a `DeliveryStatus` test after the first-test delay,
//!    and the probe carries a test-derived acknowledged-message id in the high
//!    half of the `u32` range;
//! 2. a *matching* authenticated response refreshes the pair and advances the
//!    successful-response counter;
//! 3. a response whose id matches no pending test is **not** credited — the
//!    correlation is authenticated, not "any `DeliveryStatus`";
//! 4. a pair that has **never** answered is **not** torn down when its
//!    response times out — Plan 504 invariant 9, the safety property;
//! 5. a **proven** pair that misses two consecutive responses reaches the
//!    threshold, is removed, and the coordinator is asked to rebuild;
//! 6. the pair table and the pending table stay within their ceilings;
//! 7. the liveness counters are reachable through the daemon's public
//!    inspection seam;
//! 8. a **destination** tunnel is never reported as an exploratory liveness
//!    pair, so it can never be tracked;
//! 9. an installed exploratory pair is registered with the scheduler and the
//!    scheduler's active-pair count reflects it.
//!
//! One behaviour is **not** drivable through the public API and is reported
//! rather than papered over: the coordinator's own exploratory pool is
//! private, and the only public path that fills it is a real build exchange
//! with a peer, which needs a socket. The coordinator→scheduler hand-off is
//! therefore covered by the structural guard (rules 1 and 2) and by the
//! socket-based `exploratory_build_live.rs::liveness_scheduler_pairs_both_directions`;
//! this lane proves the scheduler's half of the contract end to end.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::exploratory_build::{
    BuildDirection, ExploratoryBuildCoordinator, ExploratoryRebuildPeer, SubmitResult,
    is_destination_role_slot,
};
use i2pr_daemon::i2pcontrol_inspection::{InspectionHandles, LivenessCounterCell, ServiceEndpoint};
use i2pr_daemon::router_i2np::{MAX_ROUTER_I2NP_BYTES, RouterI2npOutcome, Ssu2DaemonService};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LIVENESS_PROBE_MESSAGE_ID_BASE, LIVENESS_RESPONSE_TIMEOUT_MS,
    LivenessAction, LivenessConfig, LivenessCounters, MAX_LIVENESS_PAIRS,
    MAX_PENDING_LIVENESS_TESTS, TunnelLivenessScheduler, delivery_status_message,
    route_inbound_with_liveness,
};
use i2pr_proto::{Date, Hash, I2npMessage, Mapping, RouterAddress};
use i2pr_runtime::{Ssu2IdentityMaterial, Ssu2InboundI2np};
use i2pr_transport::{LinkId, PeerId};
use i2pr_tunnel::bridge::ShortBuildI2npBridge;
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::established::{
    EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use i2pr_tunnel::pool::{ExploratoryPool, TunnelSlot};
use rand_chacha::ChaCha8Rng;
use rand_core::{SeedableRng, TryRngCore};

/// The lane's fixed instant. Every clock read in this lane is derived from
/// this constant; no wall clock is ever read.
const NOW_MS: u64 = 1_700_000_000_000;
/// The same instant in seconds, for the APIs that count in seconds.
const NOW_SECONDS: u64 = NOW_MS / 1000;
/// The fixed seed of the local identity material.
const IDENTITY_SEED: u64 = 0x5041_4E35_0404_0001;
/// The fixed seed of the ChaCha used for the coordinator's own RNG.
const BUILD_SEED: u64 = 0x5041_4E35_0404_0002;
/// The outbound pool slot of the lane's one exploratory pair.
const OUTBOUND_SLOT: TunnelSlot = TunnelSlot::from_raw(0);
/// The inbound pool slot of the lane's one exploratory pair.
const INBOUND_SLOT: TunnelSlot = TunnelSlot::from_raw(1);
/// The peer the lane's tunnel material is bound to.
const PEER_BYTE: u8 = 0x51;

/// The lane's own local identity, derived from a fixed seed.
struct LocalIdentity {
    hash: Hash,
    static_secret: [u8; 32],
    static_public: [u8; 32],
    intro: [u8; 32],
    router_info: Vec<u8>,
}

/// I2P's base64 alphabet, used by the SSU2 address options.
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

/// Builds the lane's local identity from a fixed seed.
///
/// The RouterInfo is signed with the fixed-seed identity and carries an SSU2
/// address bound to the same static key and intro key, so the daemon-owned
/// runtime accepts it as its own local material.
fn local_identity() -> LocalIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(IDENTITY_SEED);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity bundle");
    let hash = bundle.identity().hash().expect("router hash");
    let static_secret = X25519PrivateKey::generate(&mut rng).expect("static key");
    let static_secret_bytes = *static_secret.secret_bytes();
    let static_public_bytes = static_secret.public_bytes();
    let mut intro = [0_u8; 32];
    loop {
        rng.try_fill_bytes(&mut intro).expect("intro key");
        if intro.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "1".to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_public_bytes)),
        ("i".to_string(), i2p_b64_encode(&intro)),
    ])
    .expect("address options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("ssu2 address");
    let router_info_options = Mapping::from_entries(vec![
        ("router.version".to_string(), "0.9.58".to_string()),
        ("netId".to_string(), "2".to_string()),
    ])
    .expect("router info options");
    let info = bundle
        .sign_router_info(
            Date::from_millis(NOW_MS),
            vec![address],
            Vec::new(),
            router_info_options,
        )
        .expect("signed router info");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encoded router info");
    LocalIdentity {
        hash,
        static_secret: static_secret_bytes,
        static_public: static_public_bytes,
        intro,
        router_info,
    }
}

/// Builds a daemon-owned SSU2 service that never binds a socket.
///
/// The service is constructed, not started: `Ssu2DaemonService::new` creates
/// the runtime without opening a socket, so the lane owns no listener and
/// performs no I/O. The delivery service it hands out therefore refuses every
/// delivery, which is exactly what the rebuild assertion expects.
fn local_delivery_service() -> i2pr_daemon::router_i2np::RouterDeliveryService {
    let mut config = Config::parse(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n",
    )
    .expect("strict profile")
    .ssu2;
    config.bind_ipv4 = Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
    config.port = 0;
    let identity = local_identity();
    let service = Ssu2DaemonService::new(
        &config,
        Ssu2IdentityMaterial {
            router_hash: identity.hash,
            static_secret_bytes: identity.static_secret,
            intro_key: i2pr_runtime::IntroKey::new(identity.intro),
            router_info: identity.router_info,
        },
    )
    .expect("ssu2 service");
    service.delivery()
}

/// Builds a coordinator with the lane's fixed clock.
fn coordinator() -> ExploratoryBuildCoordinator {
    let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coordinator.advance_time(NOW_MS);
    coordinator
}

/// Builds a scheduler with the lane's one registered pair.
fn scheduler_with_one_pair() -> TunnelLivenessScheduler {
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(NOW_MS);
    scheduler
        .register_pair(OUTBOUND_SLOT, INBOUND_SLOT)
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
        peer: PeerId::from_hash(Hash::from_bytes([PEER_BYTE; 32])),
        bytes,
    }
}

/// The layer keys of one synthetic hop.
fn layer_keys(seed: u8) -> i2pr_tunnel::build_crypto::LayerKeys {
    i2pr_tunnel::build_crypto::LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

/// The shared peer of the lane's tunnel material.
fn shared_peer() -> TunnelPeer {
    TunnelPeer::from_hash(Hash::from_bytes([PEER_BYTE; 32]))
}

/// The established outbound material of the lane's exploratory pair.
fn outbound_material() -> i2pr_tunnel::established::EstablishedMaterial {
    let hops = vec![EstablishedHop::terminal(
        shared_peer(),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x0100).expect("tunnel id"),
        layer_keys(0x10),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0200).expect("tunnel id"),
        hops,
        NOW_SECONDS,
        None,
        None,
    )
    .expect("outbound established");
    tunnel.into_extracted()
}

/// The established inbound material of the lane's exploratory pair. Its
/// gateway is the same peer the outbound material terminates at, which is the
/// shape an exploratory liveness pair has.
fn inbound_material() -> i2pr_tunnel::established::EstablishedMaterial {
    let local_receive = TunnelId::new(0x0300).expect("tunnel id");
    let gateway_tunnel = TunnelId::new(0x0400).expect("tunnel id");
    let hops = vec![EstablishedHop::with_next(
        shared_peer(),
        EstablishedRole::InboundGateway,
        gateway_tunnel,
        layer_keys(0x20),
        EstablishedNextHop::new(shared_peer(), local_receive),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500).expect("tunnel id"),
        hops,
        NOW_SECONDS,
        Some((shared_peer(), gateway_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established");
    tunnel.into_extracted()
}

/// Plan 504 requirement 1: the scheduler emits a `DeliveryStatus` test after
/// the first-test delay, and the probe carries the test-derived
/// acknowledged-message id.
///
/// The acknowledged-message id is the only authenticated link between a probe
/// and the answer it causes, so it must be inside the probe's half of the
/// `u32` range: the NetDB publication reply tokens are a monotonic counter
/// that starts at one, and the two spaces must never overlap.
#[test]
fn the_scheduler_emits_a_delivery_status_test_after_the_first_test_delay() {
    let mut scheduler = scheduler_with_one_pair();

    // One millisecond before the delay, no test is due.
    scheduler.advance_time(NOW_MS + FIRST_LIVENESS_DELAY_MS - 1);
    assert!(
        matches!(scheduler.drive(), LivenessAction::Idle { .. }),
        "no test may be due before the first-test delay"
    );
    assert_eq!(scheduler.counters().tests_sent, 0);

    // After the delay, the probe is emitted for the registered pair.
    scheduler.advance_time(NOW_MS + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    let LivenessAction::SendTest {
        outbound_slot,
        inbound_slot,
        test_id,
        probe_message_id,
    } = action
    else {
        panic!("expected a SendTest action, got {action:?}");
    };
    assert_eq!(outbound_slot, OUTBOUND_SLOT);
    assert_eq!(inbound_slot, INBOUND_SLOT);
    assert!(
        probe_message_id >= LIVENESS_PROBE_MESSAGE_ID_BASE,
        "the probe identifier must live in the high half of the u32 range, got {probe_message_id}"
    );
    assert_ne!(
        probe_message_id, 0,
        "the probe identifier must be a usable message id"
    );
    assert_eq!(scheduler.pending_tests(), 1);
    assert_eq!(scheduler.counters().tests_sent, 1);
    assert_eq!(scheduler.next_pending_test_id(), Some(test_id));
}

/// Plan 504 requirement 4: a *matching* authenticated response refreshes the
/// pair and advances the successful-response counter.
///
/// The response travels the real inbound seam — the bytes are a canonical
/// `DeliveryStatus` I2NP message and the dispatcher decodes them — so the
/// acknowledged id is carried end to end.
#[test]
fn a_matching_authenticated_response_refreshes_the_pair_and_advances_the_success_counter() {
    let mut coordinator = coordinator();
    let mut scheduler = scheduler_with_one_pair();
    let probe_message_id = drive_first_test(&mut scheduler);

    let inbound = delivery_status_inbound(probe_message_id, NOW_MS);
    let outcome = route_inbound_with_liveness(&mut coordinator, &mut scheduler, &inbound, NOW_MS)
        .expect("the inbound route must succeed");
    assert!(
        matches!(outcome.dispatcher, RouterI2npOutcome::RouterControl { .. }),
        "the dispatcher must classify the echo as a router control message"
    );
    assert_eq!(
        outcome.dispatcher.acknowledged_message_id(),
        Some(probe_message_id),
        "the dispatcher must carry the acknowledged message id out"
    );
    assert!(
        outcome.refreshed.is_some(),
        "the matching response must refresh the pending test"
    );
    assert_eq!(scheduler.counters().successful_responses, 1);
    assert_eq!(scheduler.counters().unauthenticated_responses, 0);
    assert_eq!(
        scheduler.counters().unproven_pairs,
        0,
        "an authenticated response is the only proof that a pair can answer"
    );
    assert_eq!(scheduler.pending_tests(), 0);
}

/// Plan 504 requirement 5: a response whose id matches no pending test is
/// **not** credited.
///
/// The correlation is authenticated, not "any `DeliveryStatus`": a response
/// that names no pending probe is not evidence, it refreshes no pair and
/// marks no pair proven. It is counted instead, so the unauthenticated
/// correlation is visible rather than silent.
#[test]
fn a_response_carrying_an_unmatched_id_is_not_credited() {
    let mut coordinator = coordinator();
    let mut scheduler = scheduler_with_one_pair();
    drive_first_test(&mut scheduler);

    let unmatched = LIVENESS_PROBE_MESSAGE_ID_BASE.saturating_add(0x100);
    let inbound = delivery_status_inbound(unmatched, NOW_MS);
    let outcome = route_inbound_with_liveness(&mut coordinator, &mut scheduler, &inbound, NOW_MS)
        .expect("the inbound route must succeed");
    assert!(
        outcome.refreshed.is_none(),
        "a response that names no pending probe must not refresh a test"
    );
    assert_eq!(scheduler.counters().successful_responses, 0);
    assert_eq!(scheduler.counters().unauthenticated_responses, 1);
    assert_eq!(
        scheduler.pending_tests(),
        1,
        "an unmatched response must not consume the pending test"
    );
    assert_eq!(
        scheduler.counters().unproven_pairs,
        1,
        "an unmatched response must not prove the pair"
    );
}

/// Plan 504 invariant 9 — the safety property.
///
/// A pair that has **never** answered is not torn down when its response
/// times out. A missing response is indistinguishable from "this peer does
/// not answer", so acting on it would remove a healthy pair the router cannot
/// prove is broken. The withheld teardown is counted instead, so an operator
/// can see that the probe is not being answered.
#[test]
fn an_unproven_pair_is_not_torn_down_when_its_probe_response_times_out() {
    let mut scheduler = scheduler_with_one_pair();
    drive_first_test(&mut scheduler);

    // Two full response timeouts pass with no answer at all.
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
    assert_eq!(
        scheduler.counters().unproven_pairs,
        1,
        "the pair must still count as unproven"
    );
}

/// Plan 504 requirement 7: a **proven** pair that misses two consecutive
/// responses reaches the threshold, is removed, and the coordinator is asked
/// to rebuild it.
///
/// The rebuild request is the coordinator's own `submit` path: the lane
/// asserts that the request was made (the coordinator's own build counter
/// advances), not that a tunnel was built — this lane opens no socket, so the
/// delivery seam refuses the attempt, which is a delivery fact and not a
/// coordination fact.
#[test]
fn a_proven_pair_that_misses_two_consecutive_responses_is_removed_and_the_coordinator_is_asked_to_rebuild()
 {
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
    assert_eq!(outbound_slot, OUTBOUND_SLOT);
    assert_eq!(inbound_slot, INBOUND_SLOT);
    assert!(
        consecutive_failures >= 2,
        "the failure threshold must be reached, got {consecutive_failures}"
    );
    assert_eq!(
        scheduler.active_pairs(),
        0,
        "the marked pair must be removed from the scheduler"
    );
    assert!(
        scheduler.tracked_outbound_slots().is_empty(),
        "the removed pair must not be tracked any more"
    );
    assert_eq!(scheduler.counters().unhealthy_marks, 1);

    // The pump asks the coordinator to rebuild the pair through its own
    // submit path.
    let delivery = local_delivery_service();
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(BUILD_SEED);
    let identity = local_identity();
    let peer = ExploratoryRebuildPeer {
        router_hash: Hash::from_bytes([PEER_BYTE; 32]),
        static_encryption_key: identity.static_public,
    };
    let builds_before = coordinator.counters().outbound_builds;
    let result = coordinator
        .rebuild_exploratory(
            BuildDirection::Outbound,
            peer,
            Some(identity.hash),
            &delivery,
            &bridge,
            &mut rng,
        )
        .expect("the coordinator must accept the rebuild request");
    assert!(
        matches!(
            result,
            SubmitResult::Submitted { .. } | SubmitResult::Rejected { .. }
        ),
        "the rebuild must be a submit outcome, got {result:?}"
    );
    assert!(
        coordinator.counters().outbound_builds > builds_before,
        "the coordinator must have been asked to rebuild the pair"
    );
}

/// Plan 504 requirement 8: the pair table and the pending table stay within
/// their ceilings.
#[test]
fn the_pair_table_and_the_pending_table_stay_within_their_ceilings() {
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(NOW_MS);
    for index in 0..MAX_LIVENESS_PAIRS {
        let outbound = TunnelSlot::from_raw(index as u32);
        let inbound = TunnelSlot::from_raw(0x8000_0000_u32.saturating_add(index as u32));
        scheduler
            .register_pair(outbound, inbound)
            .expect("a pair within the capacity must be accepted");
    }
    assert_eq!(scheduler.active_pairs(), MAX_LIVENESS_PAIRS);
    assert!(
        scheduler
            .register_pair(
                TunnelSlot::from_raw(0x0000_1000),
                TunnelSlot::from_raw(0x8000_1000)
            )
            .is_err(),
        "the pair table must be bounded"
    );
    assert_eq!(
        scheduler.active_pairs(),
        MAX_LIVENESS_PAIRS,
        "a refused registration must not grow the pair table"
    );

    // The pending table is bounded as well, and a pair never holds more
    // than one outstanding test.
    let mut now = NOW_MS;
    for _ in 0..(MAX_PENDING_LIVENESS_TESTS * 2) {
        now = now.saturating_add(FIRST_LIVENESS_DELAY_MS + 1);
        scheduler.advance_time(now);
        let _ = scheduler.drive();
        assert!(
            scheduler.pending_tests() <= MAX_PENDING_LIVENESS_TESTS,
            "the pending table must stay within its ceiling, got {}",
            scheduler.pending_tests()
        );
        assert!(
            scheduler.pending_tests() <= scheduler.active_pairs(),
            "a pair must never hold more than one outstanding test"
        );
    }
}

/// Plan 504 requirement 9: the liveness counters are reachable through the
/// daemon's public inspection seam.
#[test]
fn the_liveness_counters_are_reachable_through_the_public_inspection_seam() {
    let handles = InspectionHandles::new(
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
    assert!(
        handles.liveness_counters_snapshot().is_none(),
        "an unpublished slot is a gap, never a fabricated zero"
    );

    let cell: Arc<LivenessCounterCell> = Arc::new(Mutex::new(LivenessCounters::default()));
    handles.publish_liveness_counters(Arc::clone(&cell));
    assert!(handles.liveness_counters().is_some());

    // The product refreshes the cell on its tick.
    let mut scheduler = scheduler_with_one_pair();
    drive_first_test(&mut scheduler);
    {
        let mut guard = cell.lock().expect("counter cell");
        *guard = scheduler.counters();
    }

    let snapshot = handles
        .liveness_counters_snapshot()
        .expect("the published snapshot must be readable");
    assert_eq!(snapshot, scheduler.counters());
    assert_eq!(snapshot.tests_sent, 1);
    assert_eq!(snapshot.active_pairs, 1);
}

/// Plan 504 requirement 2: a **destination** tunnel is not registered.
///
/// The exploratory/destination distinction is structural, not a slot-number
/// test: a destination role lives in the shared data-plane registry, while
/// the exploratory pair reporting only ever reads the exploratory pool's own
/// registrations. The lane installs a destination-role tunnel and proves it
/// is in the registry while never appearing in the pair reporting.
#[test]
fn a_destination_tunnel_never_appears_in_the_tracked_exploratory_pairs() {
    let mut coordinator = coordinator();

    // Install a destination-role tunnel in the shared registry.
    let hops = vec![EstablishedHop::terminal(
        shared_peer(),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x0600).expect("tunnel id"),
        layer_keys(0x30),
    )];
    let destination_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0700).expect("tunnel id"),
        hops,
        NOW_SECONDS,
        None,
        None,
    )
    .expect("destination established");
    let destination_slot = coordinator
        .activate_destination_tunnel(BuildDirection::Outbound, destination_tunnel, NOW_SECONDS)
        .expect("destination role activation");
    assert!(
        is_destination_role_slot(destination_slot),
        "a destination role slot must live in the destination namespace"
    );

    // The destination role is in the shared registry ...
    assert!(
        coordinator
            .registry_outbound_first_hop(destination_slot)
            .is_some(),
        "the destination role must be in the shared registry"
    );
    // ... but it is not in the exploratory pool, so the pair reporting never
    // reports it.
    assert!(
        coordinator
            .registrations(TunnelDirection::Outbound)
            .is_empty(),
        "a destination role must not be an exploratory pool registration"
    );
    assert!(
        coordinator.exploratory_liveness_pairs().is_empty(),
        "a destination tunnel must never be an exploratory liveness pair"
    );

    // And the scheduler never tracks it.
    let scheduler = scheduler_with_one_pair();
    assert!(
        !scheduler
            .tracked_outbound_slots()
            .contains(&destination_slot),
        "the scheduler must never track a destination-role slot"
    );
}

/// Plan 504 requirement 1: an installed exploratory pair is registered with
/// the scheduler and the scheduler's active-pair count reflects it.
///
/// The pair is installed with the **public** `i2pr_tunnel::pool::ExploratoryPool`
/// — the very pool type `ExploratoryBuildCoordinator` owns — as one outbound
/// and one inbound registration that share a single peer. That is exactly the
/// shape the coordinator reports and the composition hands to the scheduler.
///
/// The coordinator's own pool is private, and the only public path that fills
/// it is a real build exchange with a peer, which needs a socket. The
/// coordinator→scheduler hand-off is therefore covered by the structural guard
/// (`tunnel_liveness_boundary.rs`, rules 1 and 2) and by the socket-based
/// `exploratory_build_live.rs::liveness_scheduler_pairs_both_directions`;
/// this lane proves the scheduler's half of the contract end to end.
#[test]
fn an_installed_exploratory_pair_is_registered_with_the_scheduler_and_counted() {
    let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
    let outbound = pool
        .register_outbound_with_material(outbound_material(), NOW_SECONDS)
        .expect("outbound registration");
    let inbound = pool
        .register_inbound_with_material(inbound_material(), NOW_SECONDS)
        .expect("inbound registration");
    let outbound_slot = outbound.slot();
    let inbound_slot = inbound.slot();
    assert_ne!(outbound_slot, inbound_slot);
    assert_eq!(pool.outbound_registrations().len(), 1);
    assert_eq!(pool.inbound_registrations().len(), 1);
    assert!(
        pool.routing(inbound_slot).is_some(),
        "the installed inbound must carry its public routing metadata"
    );
    // The pair's defining property: both directions share one peer, so the
    // probe leaves through one tunnel and the answer comes back through the
    // other.
    let outbound_gateway = pool
        .routing(outbound_slot)
        .expect("outbound routing metadata")
        .first_hop_router();
    let inbound_gateway = pool
        .routing(inbound_slot)
        .expect("inbound routing metadata")
        .first_hop_router();
    assert_eq!(
        outbound_gateway, inbound_gateway,
        "an exploratory pair's two directions must share one peer"
    );

    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(NOW_MS);
    scheduler
        .register_pair(outbound_slot, inbound_slot)
        .expect("register the installed pair");
    assert_eq!(
        scheduler.active_pairs(),
        1,
        "the active-pair count must reflect the registered pair"
    );
    assert_eq!(scheduler.tracked_outbound_slots(), vec![outbound_slot]);
    assert_eq!(
        scheduler.inbound_slot_for(outbound_slot),
        Some(inbound_slot),
        "the registered pair must keep its inbound response target"
    );
}
