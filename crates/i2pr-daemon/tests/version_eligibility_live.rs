//! Plan 503 out-of-crate lane: the live peer-eligibility gate and receipt.
//!
//! Plan 491 made the router honour two API-version obligations. The second
//! one — the minimum floodfill version a peer will send DSM to — was
//! enforced **nowhere in production**: the router's two live `DatabaseStore`
//! sends selected floodfills by XOR distance alone. Plan 503 puts the floor
//! on both live DSM paths and makes the peer-eligibility receipt a live
//! product.
//!
//! This lane drives the **public** daemon API and proves the live gate's
//! behaviour. It reaches into no private internals, opens no socket, reads no
//! clock, and involves no external router. Every peer is a signed RouterInfo
//! built from a fixed seed at a fixed publication instant, so the lane is
//! deterministic.
//!
//! The receipt is the gate's observable product, so the lane asserts both
//! halves of its contract: the counts are the decision's own counts, and the
//! receipt names no peer.
//!
//! The lane is a **local** lane: no environment variable, no `#[ignore]`, no
//! stock peer.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::destination_tunnels::DestinationTunnelCoordinator;
use i2pr_daemon::version_eligibility::{
    VersionDecisionKind, VersionEligibilityOwner, VersionEligibilityReceipt,
};
use i2pr_netdb::{
    DestinationHash, LookupPolicy, MIN_FLOODFILL_VERSION_FOR_DSM,
    MIN_PEER_VERSION_FOR_TUNNEL_BUILD, RouterHash, RouterInfoStoreConfig, VersionRefusalCounts,
};
use i2pr_proto::{
    ApiVersion, Date, Hash, MAX_COMMON_STRUCTURE_SIZE, Mapping, RouterAddress, RouterInfo,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The fixed publication instant of every peer in this lane.
///
/// No wall-clock time is read: the records are published at, and validated
/// against, this instant, so the lane's outcome never depends on the day it
/// runs.
const PUBLISHED_MS: u64 = 60_000;

/// The refusal reason labels the receipts are keyed by.
///
/// The labels are the gate's stable, non-identifying vocabulary.
const UNKNOWN_VERSION: &str = "unknown-version";
const UNCOMPARABLE_VERSION: &str = "uncomparable-version";
const BELOW_FLOOR: &str = "below-floor";

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("a deterministic test identity")
}

/// The at-floor API version for the DSM gate, derived from the floor itself.
///
/// The lane never restates the floor as a literal: it reads the floor the
/// policy derives, so the lane cannot drift from the product.
fn at_floor_dsm_version() -> String {
    MIN_FLOODFILL_VERSION_FOR_DSM.to_string()
}

/// The at-floor API version for the tunnel-build gate.
fn at_floor_build_version() -> String {
    MIN_PEER_VERSION_FOR_TUNNEL_BUILD.to_string()
}

/// One patch level below a floor.
fn below_floor(version: ApiVersion) -> String {
    ApiVersion::new(version.major(), version.minor(), version.patch() - 1).to_string()
}

/// A signed RouterInfo that advertises the floodfill capability and publishes
/// `version` — or publishes no version at all when `version` is `None`.
fn floodfill_info(signer: &RouterIdentityBundle, version: Option<&str>) -> RouterInfo {
    let mut options = Mapping::builder();
    options
        .insert("caps".to_owned(), "f".to_owned())
        .expect("caps");
    if let Some(version) = version {
        options
            .insert("router.version".to_owned(), version.to_owned())
            .expect("router.version");
    }
    signer
        .sign_router_info(
            Date::from_millis(PUBLISHED_MS),
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("a signed RouterInfo")
}

/// A signed RouterInfo a tunnel build may use: it publishes a family, a
/// loopback endpoint, and `version`.
fn buildable_info(signer: &RouterIdentityBundle, version: Option<&str>) -> RouterInfo {
    let mut options = Mapping::builder();
    options
        .insert("family".to_owned(), "f".to_owned())
        .expect("family");
    if let Some(version) = version {
        options
            .insert("router.version".to_owned(), version.to_owned())
            .expect("router.version");
    }
    let address_options = Mapping::from_entries(vec![
        ("host".to_owned(), "127.0.0.1".to_owned()),
        ("port".to_owned(), "12345".to_owned()),
    ])
    .expect("address options");
    let address = RouterAddress::new(
        1,
        Date::from_millis(PUBLISHED_MS),
        "SSU2".to_owned(),
        address_options,
    )
    .expect("a router address");
    signer
        .sign_router_info(
            Date::from_millis(PUBLISHED_MS),
            vec![address],
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("a signed RouterInfo")
}

/// A signed RouterInfo that publishes a usable version but none of the build
/// metadata, so the projection refuses it as *unqualified* rather than as a
/// version refusal.
fn unqualified_info(signer: &RouterIdentityBundle, version: Option<&str>) -> RouterInfo {
    let mut options = Mapping::builder();
    if let Some(version) = version {
        options
            .insert("router.version".to_owned(), version.to_owned())
            .expect("router.version");
    }
    signer
        .sign_router_info(
            Date::from_millis(PUBLISHED_MS),
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("a signed RouterInfo")
}

fn coordinator() -> DestinationTunnelCoordinator {
    DestinationTunnelCoordinator::new(LookupPolicy::default(), RouterInfoStoreConfig::default())
}

/// Holds the record in the coordinator's authoritative store and returns its
/// RouterHash.
fn hold(coordinator: &mut DestinationTunnelCoordinator, info: &RouterInfo) -> RouterHash {
    let encoded = info
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("the encoded RouterInfo");
    coordinator
        .bootstrap_reference_router_info(&encoded, Date::from_millis(PUBLISHED_MS))
        .expect("the reference RouterInfo must be accepted")
}

/// The destination the DSM gate is asked to route towards.
fn destination() -> DestinationHash {
    DestinationHash::from_hash(Hash::from_bytes([0x5a_u8; 32]))
}

/// The local router hash the DSM gate routes from.
fn local_router_hash() -> RouterHash {
    RouterHash::from_bytes([0x11_u8; 32])
}

/// The hex form of a hash, for the identity-leak assertions.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The mesh the DSM gate is asked to decide over: one floodfill at the floor,
/// one above it, one below it, one that publishes no version, and one that
/// publishes a version that cannot be compared.
struct DsmMesh {
    at_floor: RouterHash,
    above: RouterHash,
    below: RouterHash,
    unknown: RouterHash,
    uncomparable: RouterHash,
    owner: Arc<VersionEligibilityOwner>,
    coordinator: DestinationTunnelCoordinator,
}

fn dsm_mesh() -> DsmMesh {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));

    let at_floor = bundle(0x5031);
    let above = bundle(0x5032);
    let below = bundle(0x5033);
    let unknown = bundle(0x5034);
    let uncomparable = bundle(0x5035);

    let at_floor_hash = hold(
        &mut coordinator,
        &floodfill_info(&at_floor, Some(&at_floor_dsm_version())),
    );
    let above_hash = hold(&mut coordinator, &floodfill_info(&above, Some("0.9.70")));
    let below_hash = hold(
        &mut coordinator,
        &floodfill_info(&below, Some(&below_floor(MIN_FLOODFILL_VERSION_FOR_DSM))),
    );
    let unknown_hash = hold(&mut coordinator, &floodfill_info(&unknown, None));
    let uncomparable_hash = hold(
        &mut coordinator,
        &floodfill_info(&uncomparable, Some("0.9.62.")),
    );

    DsmMesh {
        at_floor: at_floor_hash,
        above: above_hash,
        below: below_hash,
        unknown: unknown_hash,
        uncomparable: uncomparable_hash,
        owner,
        coordinator,
    }
}

#[test]
fn a_below_floor_floodfill_is_not_a_dsm_target_and_the_refusal_is_counted() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));
    let below = bundle(0x5041);
    let hash = hold(
        &mut coordinator,
        &floodfill_info(&below, Some(&below_floor(MIN_FLOODFILL_VERSION_FOR_DSM))),
    );

    let selection =
        coordinator.version_gated_candidate_hashes(&destination(), &local_router_hash());

    assert!(
        selection.targets().is_empty(),
        "a floodfill below the DSM floor must not be a target"
    );
    assert!(
        !selection.targets().contains(&hash),
        "the below-floor floodfill must not be a target"
    );
    assert_eq!(
        selection.refusals().below_floor,
        1,
        "the refusal must be counted under the below-floor reason"
    );
    assert_eq!(selection.refusals().total(), 1);

    let receipt = owner.last().expect("a DSM receipt");
    assert_eq!(receipt.kind, VersionDecisionKind::DsmTarget);
    assert_eq!(receipt.examined, 1);
    assert_eq!(receipt.accepted, 0);
    assert_eq!(receipt.refusals.get(BELOW_FLOOR), Some(&1));
}

#[test]
fn an_unknown_version_floodfill_is_refused_under_unknown_version() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));
    let unknown = bundle(0x5042);
    hold(&mut coordinator, &floodfill_info(&unknown, None));

    let selection =
        coordinator.version_gated_candidate_hashes(&destination(), &local_router_hash());

    assert!(
        selection.targets().is_empty(),
        "a floodfill that publishes no version must not be a target"
    );
    assert_eq!(
        selection.refusals().unknown_version,
        1,
        "the refusal must be counted under the unknown-version reason"
    );
    let receipt = owner.last().expect("a DSM receipt");
    assert_eq!(receipt.refusals.get(UNKNOWN_VERSION), Some(&1));
    assert_eq!(receipt.refusals.get(BELOW_FLOOR), Some(&0));
}

#[test]
fn an_uncomparable_version_floodfill_is_refused_under_uncomparable_version() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));
    let uncomparable = bundle(0x5043);
    hold(
        &mut coordinator,
        &floodfill_info(&uncomparable, Some("0.9.62.")),
    );

    let selection =
        coordinator.version_gated_candidate_hashes(&destination(), &local_router_hash());

    assert!(
        selection.targets().is_empty(),
        "a floodfill with an uncomparable version must not be a target"
    );
    assert_eq!(
        selection.refusals().uncomparable_version,
        1,
        "the refusal must be counted under the uncomparable-version reason"
    );
    let receipt = owner.last().expect("a DSM receipt");
    assert_eq!(receipt.refusals.get(UNCOMPARABLE_VERSION), Some(&1));
    assert_eq!(receipt.refusals.get(UNKNOWN_VERSION), Some(&0));
}

#[test]
fn a_floodfill_at_the_floor_is_an_eligible_dsm_target() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));
    let at_floor = bundle(0x5044);
    let hash = hold(
        &mut coordinator,
        &floodfill_info(&at_floor, Some(&at_floor_dsm_version())),
    );

    let selection =
        coordinator.version_gated_candidate_hashes(&destination(), &local_router_hash());

    assert_eq!(
        selection.targets(),
        std::slice::from_ref(&hash),
        "a floodfill at the floor must be a target"
    );
    assert_eq!(
        selection.refusals().total(),
        0,
        "an eligible floodfill must produce no refusal"
    );
    let receipt = owner.last().expect("a DSM receipt");
    assert_eq!(receipt.examined, 1);
    assert_eq!(receipt.accepted, 1);
    assert_eq!(receipt.refused(), 0);
}

#[test]
fn the_receipt_counts_are_the_decision_own_counts() {
    let mesh = dsm_mesh();

    let selection = mesh
        .coordinator
        .version_gated_candidate_hashes(&destination(), &local_router_hash());

    let receipt = mesh.owner.last().expect("a DSM receipt");
    assert_eq!(
        receipt.examined,
        selection.examined(),
        "the receipt must report the decision's own examined count"
    );
    assert_eq!(
        receipt.accepted,
        selection.accepted(),
        "the receipt must report the decision's own accepted count"
    );
    assert_eq!(
        receipt.refusals,
        selection.refusals().counts_by_reason(),
        "the receipt must report the decision's own refusal counts"
    );

    // The refusal map is keyed by the three stable, non-identifying labels.
    assert_eq!(receipt.refusals.len(), 3);
    for label in [UNKNOWN_VERSION, UNCOMPARABLE_VERSION, BELOW_FLOOR] {
        assert!(
            receipt.refusals.contains_key(label),
            "the refusal map must carry `{label}`: {:?}",
            receipt.refusals
        );
    }
    assert_eq!(receipt.refusals.get(BELOW_FLOOR), Some(&1));
    assert_eq!(receipt.refusals.get(UNKNOWN_VERSION), Some(&1));
    assert_eq!(receipt.refusals.get(UNCOMPARABLE_VERSION), Some(&1));

    // The mesh is the one the lane describes: two eligible floodfills, one
    // below the floor, one unknown, one uncomparable.
    assert_eq!(selection.examined(), 5);
    assert_eq!(selection.accepted(), 2);
    assert!(selection.targets().contains(&mesh.at_floor));
    assert!(selection.targets().contains(&mesh.above));
    assert!(!selection.targets().contains(&mesh.below));
    assert!(!selection.targets().contains(&mesh.unknown));
    assert!(!selection.targets().contains(&mesh.uncomparable));
}

#[test]
fn the_dsm_gate_satisfies_examined_equals_accepted_plus_refused() {
    // The identity is exact for the **DSM** gate: the gate has exactly two
    // outcomes, so every examined peer is either accepted or refused by a
    // version reason. It does **not** hold for the tunnel-build gate, whose
    // `examined` is the whole scanned population.
    let mesh = dsm_mesh();

    let selection = mesh
        .coordinator
        .version_gated_candidate_hashes(&destination(), &local_router_hash());

    assert_eq!(
        selection.examined(),
        selection.accepted() + selection.refusals().total(),
        "the DSM gate must examine exactly the accepted and the refused peers"
    );
    let receipt = mesh.owner.last().expect("a DSM receipt");
    assert_eq!(receipt.examined, receipt.accepted + receipt.refused());
}

#[test]
fn a_receipt_carries_no_peer_identity() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));

    let at_floor = bundle(0x5051);
    let below = bundle(0x5052);
    let unknown = bundle(0x5053);
    let at_floor_hash = hold(
        &mut coordinator,
        &floodfill_info(&at_floor, Some(&at_floor_dsm_version())),
    );
    let below_hash = hold(
        &mut coordinator,
        &floodfill_info(&below, Some(&below_floor(MIN_FLOODFILL_VERSION_FOR_DSM))),
    );
    let unknown_hash = hold(&mut coordinator, &floodfill_info(&unknown, None));

    coordinator.version_gated_candidate_hashes(&destination(), &local_router_hash());
    let receipt = owner.last().expect("a DSM receipt");
    let rendered = format!("{receipt:?}");

    // The receipt's fields are the gate, the counts, the reason labels, and
    // the digest — nothing else.
    assert_eq!(receipt.kind, VersionDecisionKind::DsmTarget);
    assert_eq!(receipt.examined, 3);
    assert_eq!(receipt.accepted, 1);

    // Neither the rendered receipt nor its field values may carry a peer
    // hash, an address, a key, or a version string.
    let forbidden = [
        hex(at_floor_hash.as_bytes()),
        hex(below_hash.as_bytes()),
        hex(unknown_hash.as_bytes()),
        hex(at_floor.identity().public_key().as_bytes()),
        hex(below.identity().public_key().as_bytes()),
        hex(unknown.identity().public_key().as_bytes()),
        at_floor_dsm_version(),
        below_floor(MIN_FLOODFILL_VERSION_FOR_DSM),
        "0.9.70".to_owned(),
        "0.9.62.".to_owned(),
        "127.0.0.1".to_owned(),
        "12345".to_owned(),
    ];
    for forbidden in forbidden {
        assert!(
            !rendered.contains(&forbidden),
            "the receipt must not carry `{forbidden}`: {rendered}"
        );
    }
}

#[test]
fn the_receipt_log_is_bounded_and_evicts_the_oldest_entry() {
    let owner = VersionEligibilityOwner::new(2);
    let refusals = VersionRefusalCounts::default();

    owner.record(VersionEligibilityReceipt::dsm_target(1, 1, &refusals));
    owner.record(VersionEligibilityReceipt::dsm_target(2, 1, &refusals));
    owner.record(VersionEligibilityReceipt::dsm_target(3, 1, &refusals));

    assert_eq!(
        owner.receipt_count(),
        2,
        "the log must not grow past its cap"
    );
    let receipts = owner.receipts();
    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[0].examined, 2,
        "the oldest entry must have been evicted"
    );
    assert_eq!(receipts[1].examined, 3);
    assert_eq!(owner.last().expect("the newest receipt").examined, 3);
}

#[test]
fn a_receipt_cap_of_zero_records_nothing() {
    let owner = VersionEligibilityOwner::new(0);
    owner.record(VersionEligibilityReceipt::dsm_target(
        1,
        1,
        &VersionRefusalCounts::default(),
    ));

    assert_eq!(owner.receipt_count(), 0);
    assert!(owner.receipts().is_empty());
    assert!(owner.last().is_none());
}

#[test]
fn the_build_gate_records_a_receipt_with_the_projection_counts() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut coordinator = coordinator().with_version_eligibility(Arc::clone(&owner));

    let eligible = bundle(0x5061);
    let capped = bundle(0x5062);
    let below = bundle(0x5063);
    let unqualified = bundle(0x5064);

    let eligible_hash = hold(
        &mut coordinator,
        &buildable_info(&eligible, Some(&at_floor_build_version())),
    );
    let capped_hash = hold(
        &mut coordinator,
        &buildable_info(&capped, Some(&at_floor_build_version())),
    );
    hold(
        &mut coordinator,
        &buildable_info(
            &below,
            Some(&below_floor(MIN_PEER_VERSION_FOR_TUNNEL_BUILD)),
        ),
    );
    hold(
        &mut coordinator,
        &unqualified_info(&unqualified, Some(&at_floor_build_version())),
    );

    // The capped peer has reached the participation cap, so the projection
    // refuses it — and counts that refusal in the summary.
    for _ in 0..i2pr_netdb::DEFAULT_MAX_PARTICIPATION {
        coordinator.record_hop_selection(&[*capped_hash.as_hash()]);
    }

    let (candidates, summary) = coordinator.destination_peer_candidates();

    assert_eq!(summary.scanned, 4);
    assert_eq!(summary.unqualified, 1);
    assert_eq!(summary.capped_participation, 1);
    assert_eq!(summary.refused_below_version_floor, 1);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].router_hash(),
        *eligible_hash.as_hash(),
        "the accepted candidate must be the eligible peer"
    );

    let receipt = owner.last().expect("a tunnel-build receipt");
    assert_eq!(receipt.kind, VersionDecisionKind::TunnelBuildPeer);
    assert_eq!(
        receipt.examined, summary.scanned as u32,
        "the build receipt must report the projection's scanned count"
    );
    assert_eq!(
        receipt.accepted,
        candidates.len() as u32,
        "the build receipt must report the number of candidates returned"
    );
    assert_eq!(receipt.refusals.get(BELOW_FLOOR), Some(&1));

    // The refusal map carries the three **version** reasons only: the
    // projection's `unqualified` and `capped_participation` reasons are not
    // version refusals and are counted in the summary.
    assert_eq!(receipt.refusals.len(), 3);
    for label in [UNKNOWN_VERSION, UNCOMPARABLE_VERSION, BELOW_FLOOR] {
        assert!(
            receipt.refusals.contains_key(label),
            "the refusal map must carry `{label}`: {:?}",
            receipt.refusals
        );
    }
    assert!(
        !receipt.refusals.contains_key("unqualified"),
        "the receipt must not carry the projection's unqualified reason"
    );
    assert!(
        !receipt.refusals.contains_key("capped_participation"),
        "the receipt must not carry the projection's capped-participation reason"
    );

    // The build gate's `examined` is the whole scanned population, so the
    // DSM gate's identity does **not** hold here.
    assert_ne!(
        receipt.examined,
        receipt.accepted + receipt.refused(),
        "the build gate's examined count is the scanned population"
    );
}

#[test]
fn a_coordinator_without_an_owner_records_nothing() {
    let owner = Arc::new(VersionEligibilityOwner::new(8));
    let mut unowned = coordinator();
    let mut owned = coordinator().with_version_eligibility(Arc::clone(&owner));

    let peer = bundle(0x5071);
    let peer_hash = hold(
        &mut unowned,
        &floodfill_info(&peer, Some(&at_floor_dsm_version())),
    );
    hold(
        &mut owned,
        &floodfill_info(&peer, Some(&at_floor_dsm_version())),
    );

    let unowned_selection =
        unowned.version_gated_candidate_hashes(&destination(), &local_router_hash());
    assert_eq!(
        owner.receipt_count(),
        0,
        "a coordinator without an owner must record nothing anywhere"
    );

    let owned_selection =
        owned.version_gated_candidate_hashes(&destination(), &local_router_hash());
    assert_eq!(
        owner.receipt_count(),
        1,
        "the owned coordinator must record its decision"
    );

    // The receipt is not load-bearing for the decision: both coordinators
    // reach the same targets.
    assert_eq!(unowned_selection, owned_selection);
    assert_eq!(
        unowned_selection.targets(),
        std::slice::from_ref(&peer_hash)
    );
}
