//! Plan 508 — the out-of-crate live lane for the transit
//! creator-correlation retention window.
//!
//! The lane drives the **public** daemon API only: the transit owner's
//! install, sweep and count methods, and its production inbound entry
//! point `handle_inbound`. It reaches into no private module and adds no
//! accessor of its own.
//!
//! The feasibility artefact
//! (`transit_creator_retention_feasibility.rs`) proves the **mechanism** —
//! that the window is derivable and that the facts are bounded. This lane
//! proves the **behaviour**: that a fact inside its window still bypasses
//! transit, that a fact past its window does not, and that the sweep bounds
//! the facts over many simulated builds.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value the lane's own code draws comes
//!   from a seeded ChaCha8 stream.
//! - **An explicit clock.** Every decision the lane asserts is taken on an
//!   explicit millisecond clock, so the lane's outcome does not depend on
//!   the moment it runs.
//! - **No environment variable, no `#[ignore]`, no stock peer.**
//! - **No unbounded waiting.** The lane never sleeps to make an assertion
//!   pass.
//! - **The production inbound path.** The bypass is observed through
//!   `handle_inbound`, the same entry point the production pump uses.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::exploratory_build::{
    BUILD_DEADLINE_MS, BUILD_MESSAGE_REPLAY_RETENTION_MS, CREATOR_CORRELATION_RETENTION_MS,
};
use i2pr_daemon::router_i2np::{RouterDeliveryService, generate_controlled_identity};
use i2pr_daemon::transit_compose::{TransitBuildService, TransitHopMaterial};
use i2pr_daemon::transit_owner::{LiveInboundOutcome, TransitDataDisposition, TransitLiveOwner};
use i2pr_daemon::transit_participation::{
    TransitParticipationBinding, TransitParticipationOwner, TransitParticipationPolicy,
};
use i2pr_proto::{Date, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelDataMessage};
use i2pr_runtime::{
    BandwidthGovernor, BandwidthGovernorConfig, CancellationToken, Ssu2InboundI2np,
    Ssu2RuntimeConfig,
};
use i2pr_tunnel::build_crypto::EPHEMERAL_KEY_LEN;
use i2pr_tunnel::{TransitAdmissionPolicy, TransitMode};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane starts at.
const NOW_MS: u64 = 1_700_000_000_000;

/// The number of simulated builds the bound is measured over.
const BUILDS: u64 = 256;

fn router_delivery() -> RouterDeliveryService {
    let mut rng = ChaCha8Rng::seed_from_u64(0x509);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", 44_001).expect("controlled identity");
    let runtime = i2pr_runtime::Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), identity)
        .expect("runtime");
    RouterDeliveryService::new(runtime)
}

fn service_for_test() -> TransitBuildService {
    let hop_material = TransitHopMaterial::new(
        [0xA3; EPHEMERAL_KEY_LEN],
        i2pr_proto::Hash::from_bytes([0x55; 32]),
    );
    let policy = TransitAdmissionPolicy::new(true, TransitMode::Accepting, 4, 2, 2, 1, 64, None)
        .expect("policy");
    TransitBuildService::new(hop_material, policy, 4, router_delivery()).expect("service")
}

/// The M11 qualification posture.
///
/// The lane qualifies the *machinery*, so it installs the loosest bounded
/// posture: the transit service's own admission policy stays the governing
/// limit, and the posture's share budget is wide enough that the lane's
/// cells are never refused for share.
fn participation_for_test() -> TransitParticipationBinding {
    let governor = BandwidthGovernor::new(BandwidthGovernorConfig {
        inbound_bytes_per_second: 1024 * 1024,
        outbound_bytes_per_second: 1024 * 1024,
        burst_bytes: 64 * 1024,
        control_reserve_percent: 10,
        max_pending_requests: 16,
        max_pending_per_peer: 2,
    })
    .expect("valid governor");
    let policy = TransitParticipationPolicy::new(
        100,
        i2pr_daemon::config::MAX_TRANSIT_PARTICIPATING_TUNNELS,
        i2pr_daemon::config::MAX_TRANSIT_PEER_PARTICIPATION_CAP,
    )
    .expect("bounded posture");
    let owner = TransitParticipationOwner::new(policy, Some(&governor), 100).expect("posture");
    TransitParticipationBinding::new(owner, Some(governor))
}

fn live_owner_for_test() -> TransitLiveOwner<ChaCha8Rng> {
    TransitLiveOwner::new_disabled(
        router_delivery(),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0x509),
    )
}

fn test_peer() -> i2pr_transport::PeerId {
    i2pr_transport::PeerId::from_bytes([0x99; 32])
}

fn test_link() -> i2pr_transport::LinkId {
    i2pr_transport::LinkId::new(7).expect("link")
}

fn inbound_with(bytes: Vec<u8>) -> Ssu2InboundI2np {
    Ssu2InboundI2np {
        link_id: test_link(),
        peer: test_peer(),
        bytes,
    }
}

/// One TunnelData message on `tunnel_id`, expiring a minute after
/// `handled_at_ms`.
///
/// The message's own I2NP expiration matters: a message whose expiration
/// has passed is ignored by the owner before any ownership check, so the
/// lane would measure the wrong thing.
fn tunnel_data_message(tunnel_id: u32, handled_at_ms: u64) -> Vec<u8> {
    let cell = TunnelDataMessage {
        tunnel_id,
        data: [0xAA; 1024],
    };
    let body = I2npBody::TunnelData(Box::new(cell));
    I2npMessage::new_standard(
        0xBEEF,
        Date::from_millis(handled_at_ms.saturating_add(60_000)),
        body,
    )
    .expect("message")
    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode")
}

/// The expiry, in seconds, the owner stores for a fact installed at
/// `now_ms`.
fn expiry_seconds(now_ms: u64) -> u64 {
    now_ms
        .saturating_add(CREATOR_CORRELATION_RETENTION_MS)
        .saturating_add(999)
        / 1_000
}

/// Whether the owner bypassed transit for the message.
fn bypassed(
    outcome: &Result<LiveInboundOutcome, i2pr_daemon::transit_owner::TransitLiveError>,
) -> bool {
    matches!(
        outcome,
        Ok(LiveInboundOutcome::Data(
            TransitDataDisposition::CreatorOwned
        ))
    )
}

/// Plan 508 requirement 1: a fact inside its window still bypasses transit.
///
/// The bypass is the invariant the facts exist for, and the window must
/// never be shorter than the coordinator's own memory.
#[test]
fn a_fact_inside_the_window_still_bypasses_transit() {
    let mut owner = live_owner_for_test();
    owner.enable(service_for_test(), participation_for_test());
    owner.install_creator_data(0x1000, NOW_MS);

    let outcome = owner.handle_inbound(
        &inbound_with(tunnel_data_message(0x1000, NOW_MS)),
        NOW_MS,
        NOW_MS / 1_000,
    );
    assert!(
        bypassed(&outcome),
        "a fact inside its window must bypass transit, got {outcome:?}"
    );
}

/// Plan 508 requirement 2: a fact is **never** declined early.
///
/// The lane checks the last instant the fact is inside its window.
#[test]
fn a_fact_at_the_last_live_instant_still_bypasses_transit() {
    let mut owner = live_owner_for_test();
    owner.enable(service_for_test(), participation_for_test());
    owner.install_creator_data(0x1000, NOW_MS);

    let last_live_ms = expiry_seconds(NOW_MS) * 1_000 - 1;
    let outcome = owner.handle_inbound(
        &inbound_with(tunnel_data_message(0x1000, last_live_ms)),
        last_live_ms,
        last_live_ms / 1_000,
    );
    assert!(
        bypassed(&outcome),
        "a fact at the last live instant must still bypass transit, got {outcome:?}"
    );
}

/// Plan 508 requirement 3: an expired fact is declined.
///
/// The bypass is a *decline*, so an expired fact is the benign error: the
/// owner stops bypassing and the message takes the transit path.
#[test]
fn an_expired_fact_no_longer_bypasses_transit() {
    let mut owner = live_owner_for_test();
    owner.enable(service_for_test(), participation_for_test());
    owner.install_creator_data(0x1000, NOW_MS);

    let expired_ms = expiry_seconds(NOW_MS) * 1_000;
    let outcome = owner.handle_inbound(
        &inbound_with(tunnel_data_message(0x1000, expired_ms)),
        expired_ms,
        expired_ms / 1_000,
    );
    assert!(
        !bypassed(&outcome),
        "an expired fact must not bypass transit, got {outcome:?}"
    );
}

/// Plan 508 requirement 4: the sweep bounds the facts over many builds.
///
/// The insert-only scheme the plan replaces would have held every fact.
#[test]
fn the_sweep_bounds_the_facts_over_many_builds() {
    let mut owner = live_owner_for_test();
    let step = CREATOR_CORRELATION_RETENTION_MS / 10;
    for build in 0..BUILDS {
        let now_ms = NOW_MS + build * step;
        owner.install_creator_data(0x1000 + build as u32, now_ms);
    }
    let now_ms = NOW_MS + BUILDS * step;
    owner.expire_creator_correlation(now_ms);

    let counts = owner.creator_correlation_counts();
    assert!(
        counts.data < BUILDS as usize,
        "the facts must be bounded, held {} of {BUILDS}",
        counts.data
    );
}

/// Plan 508 requirement 5: the counts are the observability seam.
#[test]
fn the_counts_are_the_observability_seam() {
    let mut owner = live_owner_for_test();
    owner.install_creator_data(0x1000, NOW_MS);
    owner.install_creator_gateway(0x2000, NOW_MS);
    owner.install_creator_build(test_peer(), 0x51A7_1001, NOW_MS);

    let counts = owner.creator_correlation_counts();
    assert_eq!(counts.data, 1);
    assert_eq!(counts.gateways, 1);
    assert_eq!(counts.builds, 1);

    owner.expire_creator_correlation(NOW_MS + CREATOR_CORRELATION_RETENTION_MS.saturating_mul(2));
    let counts = owner.creator_correlation_counts();
    assert_eq!(counts.data, 0);
    assert_eq!(counts.gateways, 0);
    assert_eq!(counts.builds, 0);
}

/// Plan 508 requirement 6: the window is the coordinator's own bound.
#[test]
fn the_window_covers_the_coordinator_s_own_bounds() {
    assert!(
        CREATOR_CORRELATION_RETENTION_MS
            >= BUILD_DEADLINE_MS.saturating_add(BUILD_MESSAGE_REPLAY_RETENTION_MS),
        "the window must cover the build deadline and the replay window"
    );
}
