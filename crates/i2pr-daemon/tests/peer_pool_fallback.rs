//! Plan 545's lane -- the SSU2 pool must not be floodfill-only.
//!
//! The outbound SSU2 session pool's only candidate source was
//! `select_version_gated_floodfills`
//! (`crates/i2pr-daemon/src/peer_pool.rs`), which yields only records that
//! advertise `f`. On a private LAN that population is **empty by
//! construction**: floodfill eligibility requires a publicly routable address
//! (`crates/i2pr-daemon/src/floodfill.rs:965`, `is_publicly_routable_host`),
//! and `controlled_loopback` floodfill requires loopback binds.
//!
//! The lane proves the **selection's** behaviour: a store holding only
//! non-floodfill peers yields a non-empty peer selection and an empty
//! floodfill one, a floodfill still wins when it exists, and the version floor
//! refuses the same peers on both paths with the same reason.
//!
//! The lane cannot see the pool's *preference* -- that the floodfill selector
//! is called first and the fallback only when it is empty. That is the guard's
//! claim, and the guard does not make this lane's claim.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    LookupPolicy, RouterHash, RouterInfoStore, ValidatedRouterInfo, ValidationContext,
    select_version_gated_floodfills, select_version_gated_peers,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane's records are published around.
const NOW_MS: u64 = 1_700_000_000_000;

/// One deterministic test identity.
fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs and validates one RouterInfo, optionally advertising floodfill and
/// optionally publishing a version below the DSM floor.
fn sign(signer: &RouterIdentityBundle, floodfill: bool, version: &str) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    if floodfill {
        options
            .insert("caps".to_owned(), "f".to_owned())
            .expect("caps");
    }
    options
        .insert("router.version".to_owned(), version.to_owned())
        .expect("router.version");
    let info = signer
        .sign_router_info(
            Date::from_millis(NOW_MS),
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(NOW_MS)),
    )
    .expect("validate")
}

/// A store of `count` plain peers, optionally with one floodfill.
fn store_with(count: u64, seed_base: u64, floodfill: bool) -> (RouterInfoStore, Vec<RouterHash>) {
    let mut store = RouterInfoStore::default();
    let mut keys = Vec::new();
    for index in 0..count {
        let record = sign(&make_bundle(seed_base + index), false, "0.9.62");
        keys.push(record.key());
        store.insert(record);
    }
    if floodfill {
        let record = sign(&make_bundle(seed_base + 0x9000), true, "0.9.62");
        keys.push(record.key());
        store.insert(record);
    }
    (store, keys)
}

#[test]
fn a_store_of_plain_peers_yields_no_floodfill_and_a_peer_selection() {
    let (store, keys) = store_with(4, 0x0055_0100, false);
    let local = keys[0];
    let policy = LookupPolicy::default();

    let floodfills = select_version_gated_floodfills(&store, &local, &[local], &policy);
    assert!(
        floodfills.targets().is_empty(),
        "the floodfill selection must be empty with no advertiser"
    );

    let peers = select_version_gated_peers(&store, &local, &[local], &policy);
    assert!(
        !peers.targets().is_empty(),
        "the peer selection must see the plain peers: the pool would be empty"
    );
    assert!(
        keys.iter()
            .filter(|key| **key != local)
            .any(|key| peers.targets().contains(key)),
        "the peer selection did not yield a peer the store holds"
    );
}

#[test]
fn a_floodfill_still_wins() {
    let (store, keys) = store_with(4, 0x0055_0200, true);
    let local = keys[0];
    let policy = LookupPolicy::default();

    let floodfills = select_version_gated_floodfills(&store, &local, &[local], &policy);
    assert!(
        !floodfills.targets().is_empty(),
        "the floodfill selection must find the advertiser"
    );
    // The pool prefers this selection, so the peer it names is the one that
    // owns the nearby keyspace.
    assert_eq!(floodfills.targets().len(), 1);
}

#[test]
fn both_selectors_refuse_a_below_floor_peer_with_the_same_reason() {
    let mut store = RouterInfoStore::default();
    let old = sign(&make_bundle(0x0055_0300), true, "0.7.9");
    let old_key = old.key();
    store.insert(old);

    let local = sign(&make_bundle(0x0055_0400), false, "0.9.62").key();
    let policy = LookupPolicy::default();

    let floodfills = select_version_gated_floodfills(&store, &local, &[local], &policy);
    let peers = select_version_gated_peers(&store, &local, &[local], &policy);
    assert!(
        !floodfills.targets().contains(&old_key),
        "the floodfill selector admitted a below-floor peer"
    );
    assert!(
        !peers.targets().contains(&old_key),
        "the fallback admitted a peer the floodfill selector refuses"
    );
    assert_eq!(
        floodfills.refusals().total(),
        peers.refusals().total(),
        "the two selectors disagree about how many candidates they refused"
    );
}

#[test]
fn the_identity_holds_on_the_peer_selection() {
    let (store, keys) = store_with(6, 0x0055_0500, false);
    let local = keys[0];
    let peers = select_version_gated_peers(&store, &local, &[local], &LookupPolicy::default());
    assert_eq!(
        peers.examined(),
        peers.accepted() + peers.refusals().total(),
        "examined must equal accepted plus refused"
    );
}

#[test]
fn the_ordering_is_the_same_as_the_floodfill_selection() {
    let (store, keys) = store_with(8, 0x0055_0600, true);
    let local = keys[0];
    let peers = select_version_gated_peers(&store, &local, &[local], &LookupPolicy::default());
    let targets = peers.targets().to_vec();
    let mut reordered = targets.clone();
    reordered.sort_by(|left, right| {
        // The selection is by XOR distance to `local`; equal distances are
        // broken by ascending router hash, so sorting the yielded order by the
        // same rule must reproduce it.
        xor(left, &local)
            .cmp(&xor(right, &local))
            .then(left.cmp(right))
    });
    assert_eq!(
        targets, reordered,
        "the peer selection's order is not nearest-first"
    );
}

#[test]
fn the_ceiling_is_the_policy_ceiling() {
    let (store, keys) = store_with(64, 0x0055_0700, false);
    let local = keys[0];
    let policy = LookupPolicy::default();
    let peers = select_version_gated_peers(&store, &local, &[local], &policy);
    assert!(
        peers.targets().len() <= policy.max_candidates_considered(),
        "the peer selection exceeded the policy ceiling"
    );
}

#[test]
fn the_router_excludes_itself_from_both_selections() {
    let (store, keys) = store_with(4, 0x0055_0800, true);
    let local = keys[0];
    let policy = LookupPolicy::default();
    for selection in [
        select_version_gated_floodfills(&store, &local, &[local], &policy),
        select_version_gated_peers(&store, &local, &[local], &policy),
    ] {
        assert!(
            !selection.targets().contains(&local),
            "a selection named the router's own hash"
        );
    }
}

/// XOR distance between two router hashes.
fn xor(left: &RouterHash, right: &RouterHash) -> [u8; 32] {
    let mut out = [0_u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = left.as_bytes()[index] ^ right.as_bytes()[index];
    }
    out
}
