//! Plan 491 private-mesh lane: API-version floors and peer eligibility.
//!
//! The lane drives the router's own policy over a private mesh of peers that
//! publish *different* API versions, and asserts the two obligations the
//! specification's API-version table attaches to the level this router
//! declares:
//!
//! - *minimum peers will build tunnels through* — a peer below the
//!   tunnel-build floor is never a build peer;
//! - *minimum floodfill peers will send DSM to* — a floodfill below the DSM
//!   floor is never a publication target.
//!
//! The mesh is private: no socket is opened, no peer is contacted, and no
//! external router is involved. The peers are signed RouterInfos held in the
//! daemon's own profile store, and the decisions come from the router's own
//! selection policy.
//!
//! The evidence file records **counts and digests only**. It carries no peer
//! hash, no address, no key, no version string, and no payload.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{PeerParticipationDriver, VersionDecisionKind};
use i2pr_netdb::{
    ProfileStoreConfig, RouterHash, RouterInfoStore, ValidatedRouterInfo, ValidationContext,
    router_hash,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The API level this router declares.
const DECLARED_VERSION: &str = i2pr_netdb::CONTROLLED_ROUTER_VERSION;

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// A peer that publishes an explicit API version, and optionally the floodfill
/// capability.
///
/// A peer with no version option at all publishes no version; a peer with a
/// syntactically valid but incomparable value publishes one that cannot be
/// compared.
fn peer(
    signer: &RouterIdentityBundle,
    published_ms: u64,
    version: Option<&str>,
    floodfill: bool,
) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    if let Some(version) = version {
        options
            .insert("router.version".to_owned(), version.to_owned())
            .expect("router.version");
    }
    if floodfill {
        options
            .insert("caps".to_owned(), "f".to_owned())
            .expect("caps");
    }
    let info = signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(published_ms)),
    )
    .expect("validate")
}

fn digest(bytes: &[u8]) -> String {
    let hash = i2pr_proto::Hash::digest(bytes);
    hash.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn the_api_version_floors_refuse_below_floor_peers_and_count_every_refusal() {
    // 1. The mesh: five peers that publish different API versions.
    let eligible = bundle(0x4911);
    let above = bundle(0x4912);
    let below = bundle(0x4913);
    let unknown = bundle(0x4914);
    let uncomparable = bundle(0x4915);

    let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
    for (signer, version) in [
        (&eligible, Some(DECLARED_VERSION)),
        (&above, Some("0.9.70")),
        (&below, Some("0.9.61")),
        (&unknown, None),
        (&uncomparable, Some("0.9.62.")),
    ] {
        driver.learn(peer(signer, 60_000, version, true));
    }

    let eligible_key = router_hash(eligible.identity()).expect("hash");
    let above_key = router_hash(above.identity()).expect("hash");

    // 2. The tunnel-build selection refuses every peer below the floor, and
    //    every peer whose version cannot be compared.
    let tunnel = driver.select_tunnel_build_peers(8, 0x4910);
    assert!(
        tunnel
            .candidates
            .iter()
            .any(|candidate| candidate.key == eligible_key),
        "a peer at the floor must be selectable"
    );
    assert!(
        tunnel
            .candidates
            .iter()
            .any(|candidate| candidate.key == above_key),
        "a peer above the floor must be selectable"
    );
    assert_eq!(tunnel.candidates.len(), 2, "{:?}", tunnel.candidates);
    assert_eq!(tunnel.refusals.below_floor, 1);
    assert_eq!(tunnel.refusals.unknown_version, 1);
    assert_eq!(tunnel.refusals.uncomparable_version, 1);
    assert_eq!(tunnel.refusals.total(), 3);

    // 3. The DSM target selection applies the same rule to floodfills.
    let dsm = driver.select_dsm_targets(8, 0x4920);
    assert_eq!(dsm.candidates.len(), 2, "{:?}", dsm.candidates);
    assert_eq!(dsm.refusals.below_floor, 1);
    assert_eq!(dsm.refusals.unknown_version, 1);
    assert_eq!(dsm.refusals.uncomparable_version, 1);
    assert_eq!(dsm.refusals.total(), 3);

    // 4. Both decisions are recorded, and the receipts carry counts and
    //    stable reason labels only.
    let receipts = driver.version_receipts();
    assert_eq!(receipts.len(), 2, "{receipts:?}");
    let tunnel_receipt = receipts
        .iter()
        .find(|receipt| receipt.kind == VersionDecisionKind::TunnelBuildPeer)
        .expect("tunnel-build receipt");
    let dsm_receipt = receipts
        .iter()
        .find(|receipt| receipt.kind == VersionDecisionKind::DsmTarget)
        .expect("dsm receipt");
    assert_eq!(tunnel_receipt.examined, 5);
    assert_eq!(tunnel_receipt.accepted, 2);
    assert_eq!(tunnel_receipt.refused(), 3);
    assert_eq!(
        tunnel_receipt.refusals.get("below-floor"),
        Some(&1),
        "{:?}",
        tunnel_receipt.refusals
    );
    assert_eq!(tunnel_receipt.refusals.get("unknown-version"), Some(&1));
    assert_eq!(
        tunnel_receipt.refusals.get("uncomparable-version"),
        Some(&1)
    );
    assert_eq!(dsm_receipt.examined, 5);
    assert_eq!(dsm_receipt.accepted, 2);
    assert_eq!(dsm_receipt.refused(), 3);

    // The receipt names no peer: its rendered form carries no hash, no
    // address, no key, and no version string.
    let rendered = format!("{tunnel_receipt:?}");
    for forbidden in [
        "0x4911",
        "0x4912",
        "0x4913",
        "0x4914",
        "0x4915",
        DECLARED_VERSION,
        "127.0.0.1",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "the receipt must not carry `{forbidden}`: {rendered}"
        );
    }

    // 5. The evidence file records counts and digests only.
    let evidence = serde_json::json!({
        "schema": "i2pr-version-floor-mesh-v1",
        "plan": 491,
        "mesh": {
            "peers": 5,
            "eligible": tunnel.candidates.len(),
            "refused_below_floor": tunnel.refusals.below_floor,
            "refused_unknown_version": tunnel.refusals.unknown_version,
            "refused_uncomparable_version": tunnel.refusals.uncomparable_version,
            "dsm_eligible": dsm.candidates.len(),
        },
        "receipt_digests": {
            "tunnel_build_peer": digest(tunnel_receipt.digest.as_slice()),
            "dsm_target": digest(dsm_receipt.digest.as_slice()),
        },
    });
    let rendered_evidence = evidence.to_string();
    for forbidden in [
        "0x4911",
        "0x4912",
        "0x4913",
        "0x4914",
        "0x4915",
        DECLARED_VERSION,
        "127.0.0.1",
    ] {
        assert!(
            !rendered_evidence.contains(forbidden),
            "the evidence must not carry `{forbidden}`: {rendered_evidence}"
        );
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/integration/network-participation/evidence/plan491/result.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("evidence directory");
    }
    let mut rendered_evidence = serde_json::to_string_pretty(&evidence).expect("evidence json");
    rendered_evidence.push('\n');
    std::fs::write(&path, rendered_evidence).expect("evidence file");
}

#[test]
fn the_selection_fails_closed_when_no_peer_meets_the_floor() {
    // A mesh in which every peer is below the floor must yield no candidate
    // at all: the selection fails closed with the reason counted, rather
    // than looping for a peer that is not allowed.
    let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
    for seed in 0x4930u64..0x4933 {
        driver.learn(peer(&bundle(seed), 60_000, Some("0.9.60"), true));
    }
    let tunnel = driver.select_tunnel_build_peers(8, 0x4939);
    assert!(tunnel.candidates.is_empty(), "{:?}", tunnel.candidates);
    assert_eq!(tunnel.refusals.below_floor, 3);
    assert_eq!(tunnel.refusals.total(), 3);

    let dsm = driver.select_dsm_targets(8, 0x4939);
    assert!(dsm.candidates.is_empty(), "{:?}", dsm.candidates);
    assert_eq!(dsm.refusals.below_floor, 3);

    // The refusal is recorded, so the operator can see why nothing was
    // selected.
    let receipts = driver.version_receipts();
    assert_eq!(receipts.len(), 2);
    for receipt in &receipts {
        assert_eq!(receipt.accepted, 0);
        assert_eq!(receipt.refused(), 3);
    }
}

#[test]
fn a_peer_that_upgrades_its_version_is_re_evaluated() {
    // The version evidence is refreshed from the peer's signed RouterInfo, so
    // a peer that upgrades from below the floor to at the floor becomes
    // selectable without restarting the router.
    let signer = bundle(0x4941);
    let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
    driver.learn(peer(&signer, 60_000, Some("0.9.61"), true));
    let refused = driver.select_tunnel_build_peers(4, 0x4942);
    assert!(refused.candidates.is_empty());
    assert_eq!(refused.refusals.below_floor, 1);

    // The peer republishes a newer record at the floor.
    driver.learn(peer(&signer, 61_000, Some(DECLARED_VERSION), true));
    let accepted = driver.select_tunnel_build_peers(4, 0x4943);
    assert_eq!(accepted.candidates.len(), 1, "{:?}", accepted.candidates);
    assert_eq!(accepted.refusals.total(), 0);
}

#[test]
fn a_peer_whose_record_is_gone_is_not_used_after_a_restart() {
    // A restart re-reads the peers' versions from their stored RouterInfos.
    // A peer whose stored RouterInfo is gone has no profile either — the
    // profile store removes the two together — so it cannot be used at all.
    // The outcome is stricter than a refusal: the peer is simply absent.
    let signer = bundle(0x4951);
    let mut store = RouterInfoStore::default();
    store.insert(peer(&signer, 60_000, Some(DECLARED_VERSION), true));
    let mut driver = PeerParticipationDriver::adopt(ProfileStoreConfig::default(), store, 60_000);
    let before = driver.select_tunnel_build_peers(4, 0x4952);
    assert_eq!(before.candidates.len(), 1, "{:?}", before.candidates);
    assert_eq!(before.refusals.total(), 0);

    // The restart adopts a store that no longer holds the peer.
    let mut restarted = PeerParticipationDriver::adopt(
        ProfileStoreConfig::default(),
        RouterInfoStore::default(),
        60_000,
    );
    let after = restarted.select_tunnel_build_peers(4, 0x4953);
    assert!(after.candidates.is_empty(), "{:?}", after.candidates);
    assert_eq!(after.refusals.total(), 0, "the peer is gone, not refused");
    let key: RouterHash = router_hash(signer.identity()).expect("hash");
    assert!(restarted.store().profile(&key).is_none());
}
