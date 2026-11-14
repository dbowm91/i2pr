//! Plan 508 — the feasibility artefact for the transit creator-correlation
//! retention window.
//!
//! The artefact answers the plan's own question with running code, before
//! the closure record claims anything about it:
//!
//! - **the window is derivable** from the coordinator's own bounds, so
//!   there is one source of truth and nothing to configure;
//! - **the facts are bounded** over the router's lifetime, where the
//!   insert-only scheme they replaced was not;
//! - **a live fact is never declined early**, which is the invariant the
//!   facts exist for: the router's own traffic must never be claimed as
//!   transit work;
//! - **an expired fact is declined**, which is the benign direction.
//!
//! Properties of the artefact:
//!
//! - **Fixed seeds.** Every random value is drawn from a seeded ChaCha8
//!   stream.
//! - **An explicit clock.** Every decision is taken on an explicit
//!   millisecond clock, so the artefact's outcome does not depend on the
//!   moment it runs.
//! - **The public API only.** The artefact drives
//!   [`TransitLiveOwner`]'s public install, sweep and count methods and
//!   reaches into no private module.
//! - **No environment variable, no `#[ignore]`, no stock peer.**
//! - **Two sensitivity controls.** Each control computes the answer a
//!   *wrong* implementation would give and asserts that the observed
//!   answer differs, so the bound cannot be satisfied vacuously.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::exploratory_build::{
    BUILD_DEADLINE_MS, BUILD_MESSAGE_REPLAY_RETENTION_MS, CREATOR_CORRELATION_RETENTION_MS,
};
use i2pr_daemon::router_i2np::{RouterDeliveryService, generate_controlled_identity};
use i2pr_daemon::transit_owner::TransitLiveOwner;
use i2pr_runtime::CancellationToken;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the artefact starts at.
const NOW_MS: u64 = 1_700_000_000_000;

/// The number of simulated builds the bound is measured over.
const BUILDS: u64 = 512;

fn owner() -> TransitLiveOwner<ChaCha8Rng> {
    let mut rng = ChaCha8Rng::seed_from_u64(0x508);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", 44_001).expect("controlled identity");
    let runtime =
        i2pr_runtime::Ssu2RuntimeService::new(i2pr_runtime::Ssu2RuntimeConfig::default(), identity)
            .expect("runtime");
    TransitLiveOwner::new_disabled(
        RouterDeliveryService::new(runtime),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0x508),
    )
}

/// A peer the artefact installs facts for.
fn peer() -> i2pr_transport::PeerId {
    i2pr_transport::PeerId::from_bytes([0x77; 32])
}

/// The expiry, in seconds, the owner stores for a fact installed at
/// `now_ms`. The artefact recomputes it rather than reading the owner's
/// private state, so the boundary it asserts is the boundary the owner
/// promised.
fn expiry_seconds(now_ms: u64) -> u64 {
    now_ms
        .saturating_add(CREATOR_CORRELATION_RETENTION_MS)
        .saturating_add(999)
        / 1_000
}

/// The window is derivable: it covers the build deadline and the
/// coordinator's own replay window, so it is a function of constants the
/// tree already enforces.
#[test]
fn the_window_covers_the_build_deadline_and_the_replay_window() {
    assert!(
        CREATOR_CORRELATION_RETENTION_MS
            >= BUILD_DEADLINE_MS.saturating_add(BUILD_MESSAGE_REPLAY_RETENTION_MS),
        "the window must cover the build deadline and the replay window, got {CREATOR_CORRELATION_RETENTION_MS}"
    );
}

/// The facts are bounded over the router's lifetime.
///
/// The artefact installs one fact per simulated build, spread across ten
/// windows, and sweeps at the end. Only the facts installed inside the
/// last window may survive.
#[test]
fn the_facts_are_bounded_over_many_builds() {
    let mut owner = owner();
    // The step is floored at one millisecond so that the artefact still
    // compiles when the window collapses: a control that breaks the
    // *build* proves less than a control that breaks the *assertion*.
    let step = (CREATOR_CORRELATION_RETENTION_MS / 10).max(1);
    for build in 0..BUILDS {
        let now_ms = NOW_MS + build * step;
        owner.install_creator_build(peer(), build as u32, now_ms);
        owner.install_creator_data(0x1000 + build as u32, now_ms);
        owner.install_creator_gateway(0x2000 + build as u32, now_ms);
    }
    let now_ms = NOW_MS + BUILDS * step;
    owner.expire_creator_correlation(now_ms);

    let counts = owner.creator_correlation_counts();
    let total = counts.builds + counts.data + counts.gateways;
    let window_ms = CREATOR_CORRELATION_RETENTION_MS;
    let per_window = (window_ms / step).max(1) as usize;
    let expected = (BUILDS as usize) % per_window.max(1) * 3;
    assert!(
        total <= expected + per_window * 3,
        "the facts must be bounded by the window, held {total} for {BUILDS} builds"
    );
    assert!(
        total < (BUILDS as usize) * 3,
        "the insert-only scheme would have held every fact"
    );
}

/// A live fact is never declined early.
///
/// The artefact installs a fact and checks the owner's count at the last
/// instant the fact is inside its window.
#[test]
fn a_live_fact_is_never_declined_early() {
    let mut owner = owner();
    owner.install_creator_data(0x1000, NOW_MS);
    let expiry = expiry_seconds(NOW_MS);
    let last_live_ms = expiry * 1_000 - 1;
    owner.expire_creator_correlation(last_live_ms);
    assert_eq!(
        owner.creator_correlation_counts().data,
        1,
        "a fact inside its window must survive the sweep"
    );
}

/// An expired fact is declined.
#[test]
fn an_expired_fact_is_declined() {
    let mut owner = owner();
    owner.install_creator_data(0x1000, NOW_MS);
    let expiry = expiry_seconds(NOW_MS);
    owner.expire_creator_correlation(expiry * 1_000);
    assert_eq!(
        owner.creator_correlation_counts().data,
        0,
        "a fact past its window must be dropped"
    );
}

/// The sweep drops only the expired facts.
#[test]
fn the_sweep_drops_only_the_expired_facts() {
    let mut owner = owner();
    owner.install_creator_data(0x1000, NOW_MS);
    owner.install_creator_data(0x2000, NOW_MS + CREATOR_CORRELATION_RETENTION_MS);
    let between = NOW_MS + CREATOR_CORRELATION_RETENTION_MS / 2;
    owner.expire_creator_correlation(between);
    assert_eq!(
        owner.creator_correlation_counts().data,
        2,
        "a fact installed after the sweep's clock is still live"
    );
    let after = NOW_MS + CREATOR_CORRELATION_RETENTION_MS * 2;
    owner.expire_creator_correlation(after);
    assert_eq!(
        owner.creator_correlation_counts().data,
        0,
        "both facts are past their windows"
    );
}

/// **Sensitivity control.** The bound assertion must not be satisfied by an
/// implementation with a zero window.
///
/// A zero window would expire every fact at once, so the surviving count
/// would be zero. The observed count is not zero, so the artefact's bound
/// is sensitive to the window.
#[test]
fn the_bound_assertion_is_sensitive_to_a_zero_window() {
    let mut owner = owner();
    for build in 0..32u64 {
        owner.install_creator_build(peer(), build as u32, NOW_MS);
    }
    owner.expire_creator_correlation(NOW_MS);
    let observed = owner.creator_correlation_counts().builds;
    let zero_window_answer = 0usize;
    assert_ne!(
        observed, zero_window_answer,
        "a zero window would have dropped every fact"
    );
}

/// **Sensitivity control.** The bound assertion must not be satisfied by an
/// implementation that never sweeps.
///
/// The insert-only scheme the plan replaces would hold every fact. The
/// observed count is smaller, so the sweep is doing the work.
#[test]
fn the_bound_assertion_is_sensitive_to_a_missing_sweep() {
    let mut owner = owner();
    for build in 0..32u64 {
        owner.install_creator_build(peer(), build as u32, NOW_MS);
    }
    let before_sweep = owner.creator_correlation_counts().builds;
    owner.expire_creator_correlation(NOW_MS + CREATOR_CORRELATION_RETENTION_MS * 2);
    let after_sweep = owner.creator_correlation_counts().builds;
    assert_eq!(before_sweep, 32, "every install is counted");
    assert!(
        after_sweep < before_sweep,
        "the sweep must drop the facts that expired, held {after_sweep} of {before_sweep}"
    );
}
