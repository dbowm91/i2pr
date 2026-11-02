//! Plan 514 out-of-crate lane for the live exploratory request's **target**.
//!
//! Plan 513 landed the live NetDB discovery spine, but the request it sent
//! was aimed wrongly: the live path picked the **lowest-hash** floodfill
//! advertiser in the flat store and sent the request to it, so every
//! exploration asked the **same** peer. The routing table's closest-first
//! selection -- the mechanism Plan 489 built and Plan 513 gave a reader --
//! was not consulted on that path at all.
//!
//! This lane proves the corrected behaviour through the **public** daemon
//! API only -- no private internals, no `pub(crate)` reach-ins.
//!
//! The lane is local and deterministic:
//!
//! - fixed seeds (a fixed-seed ChaCha);
//! - an explicit `now_ms` everywhere -- the lane never reads a wall clock;
//! - no socket, no environment variable, no `#[ignore]`, no stock peer, and
//!   no sleeping.
//!
//! The lane computes the expected target **itself**: the closest floodfill is
//! the one whose daily routing key is the smallest XOR distance from the
//! search key's daily routing key, with the lower `RouterHash` breaking a
//! tie. That is the reference's rule (`libi2pd/NetDbRequests.cpp:479`, at
//! the exact pin, and `libi2pd/NetDb.cpp:1312-1316`), computed here from the
//! public `daily_routing_key` rather than read back out of the table, so the
//! assertion is not the implementation checking itself.
//!
//! What the lane proves, one assertion per test:
//!
//! 1. the exploratory request's target is the **closest** floodfill;
//! 2. the target follows the **search key**, not a fixed peer;
//! 3. the target is **not** the lowest-hash floodfill -- the old behaviour;
//! 4. a floodfill the table does not hold is **never** the target;
//! 5. a table that names **no** floodfill refuses the request and counts it;
//! 6. a **disabled** table refuses the request and counts it;
//! 7. a refused request is **never** redirected to an arbitrary peer.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{
    DiscoveryDispatch, NetDbDiscoverySpine, RoutingTableConfig, RoutingTableConsumer,
    RoutingTableDriver,
};
use i2pr_netdb::{
    LookupPolicy, ReplyPath, RouterHash, RouterInfoStore, RouterInfoStoreConfig,
    ValidatedRouterInfo, ValidationContext, daily_routing_key, router_hash,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the lane's peers.
const LOCAL_SEED: u64 = 0x5140_0001;
const ONE_SEED: u64 = 0x5140_0002;
const TWO_SEED: u64 = 0x5140_0003;
const THREE_SEED: u64 = 0x5140_0004;
const FOUR_SEED: u64 = 0x5140_0005;
const FIVE_SEED: u64 = 0x5140_0006;
/// The seed of the floodfill that is in the flat store but **not** in the
/// routing table.
const GHOST_SEED: u64 = 0x5140_0007;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_700_000_000_000;

/// The tunnel id of the lane's inbound exploratory tunnel.
const REPLY_TUNNEL_ID: u32 = 0x0051_4007;

/// The gateway of the lane's inbound exploratory tunnel.
const REPLY_GATEWAY_BYTE: u8 = 0x52;

/// The lane's own identity.
fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// A validated RouterInfo signed by `signer`.
fn validated(signer: &RouterIdentityBundle, caps: &str) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), caps.to_owned()).unwrap();
    let published = Date::from_millis(T0);
    let info = signer
        .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

/// The routing hash of an identity.
fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
    router_hash(signer.identity()).expect("hash")
}

/// The daily routing key of a hash.
fn routing_key_for(hash: &RouterHash) -> RouterHash {
    let key = daily_routing_key(hash, Date::from_millis(T0)).expect("routing key");
    RouterHash::from_hash(key)
}

/// The XOR distance between two keys, byte-wise.
fn xor_distance(left: &RouterHash, right: &RouterHash) -> [u8; 32] {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut out = [0u8; 32];
    for index in 0..32 {
        out[index] = left[index] ^ right[index];
    }
    out
}

/// The floodfill **closest** to the search key, computed by the lane itself.
///
/// The rule is the reference's: the daily routing key of the search key, and
/// the floodfill whose own daily routing key is the smallest XOR distance
/// from it, with the lower `RouterHash` breaking a tie.
fn expected_closest(candidates: &[RouterHash], search_key: &RouterHash) -> RouterHash {
    let target = routing_key_for(search_key);
    let mut best: Option<(RouterHash, [u8; 32])> = None;
    for candidate in candidates {
        let distance = xor_distance(&routing_key_for(candidate), &target);
        let better = match &best {
            None => true,
            Some((best_key, best_distance)) => {
                distance < *best_distance || (distance == *best_distance && candidate < best_key)
            }
        };
        if better {
            best = Some((*candidate, distance));
        }
    }
    best.expect("the lane's candidate set is never empty").0
}

/// The lane's inbound exploratory tunnel's reply path.
fn reply_path() -> ReplyPath {
    ReplyPath::new(
        RouterHash::from_bytes([REPLY_GATEWAY_BYTE; 32]),
        REPLY_TUNNEL_ID,
    )
    .expect("reply path")
}

/// The lane's mesh: a store, a routing table and the spine that joins them.
struct Mesh {
    store: RouterInfoStore,
    spine: NetDbDiscoverySpine,
    /// The floodfills the table holds with signed evidence, in a fixed order.
    floodfills: Vec<RouterHash>,
    /// The floodfill that is in the store only.
    ghost: RouterHash,
}

/// Builds the lane's mesh.
///
/// Five floodfills are in the store **and** in the routing table, with signed
/// evidence. The sixth, the **ghost**, is in the store only: it is the
/// discriminator, and the table must never name it.
fn mesh() -> Mesh {
    let local = bundle(LOCAL_SEED);
    let records: Vec<ValidatedRouterInfo> = [
        ONE_SEED, TWO_SEED, THREE_SEED, FOUR_SEED, FIVE_SEED, GHOST_SEED,
    ]
    .into_iter()
    .map(|seed| validated(&bundle(seed), "Nf"))
    .collect();

    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    for record in &records {
        let _ = store.insert(record.clone());
    }

    let local_key = key_of(&local);
    let mut driver = RoutingTableDriver::new(
        local_key,
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    // The ghost is deliberately **not** indexed: the table holds signed
    // evidence for the other five only.
    let floodfills: Vec<RouterHash> = records[..5].iter().map(|record| record.key()).collect();
    for record in &records[..5] {
        driver.index(record);
    }

    let table = Arc::new(RoutingTableConsumer::new(driver, true));
    let spine = NetDbDiscoverySpine::new(Arc::clone(&table), LookupPolicy::default());
    spine.advance_time(T0);
    spine.set_reply_path(Some(reply_path()));

    Mesh {
        store,
        spine,
        floodfills,
        ghost: records[5].key(),
    }
}

/// The peer the dispatch names, or a panic naming the wrong variant.
fn target_of(dispatch: &DiscoveryDispatch) -> RouterHash {
    match dispatch {
        DiscoveryDispatch::Send { peer, .. } => *peer,
        other => panic!("the request must be dispatchable: {other:?}"),
    }
}

/// The target the exploratory request names for a search key.
///
/// The spine runs **one** exploration at a time -- a second request while
/// one is in flight is refused, and the production cycle awaits the answer
/// rather than replacing it -- so each request is driven on a fresh mesh.
/// The mesh's peers are fixed seeds, so every mesh is the same mesh.
fn target_for(search_key: &RouterHash) -> RouterHash {
    let mesh = mesh();
    let dispatch = mesh.spine.begin_exploration(
        &mesh.store,
        *search_key,
        routing_key_for(search_key),
        Some(reply_path()),
    );
    target_of(&dispatch)
}

/// Plan 514: the exploratory request's target is the **closest** floodfill.
///
/// The lane computes the expected target itself, from the public
/// `daily_routing_key` and the reference's XOR-distance rule, so this is not
/// the table checking its own answer.
#[test]
fn the_exploratory_target_is_the_closest_floodfill() {
    let mesh = mesh();
    for byte in 0u8..8 {
        let search_key = RouterHash::from_bytes([byte; 32]);
        let expected = expected_closest(&mesh.floodfills, &search_key);
        let target = target_for(&search_key);
        assert_eq!(
            target, expected,
            "the exploration must ask the closest floodfill for the key"
        );
    }
}

/// Plan 514: the target follows the **search key**, not a fixed peer.
///
/// Two search keys with different closest floodfills produce two different
/// targets. The old behaviour -- the flat store's iteration order -- named
/// the same peer for every key.
#[test]
fn the_target_follows_the_search_key() {
    let mesh = mesh();
    let mut seen: Vec<(RouterHash, RouterHash)> = Vec::new();
    for byte in 0u8..16 {
        let search_key = RouterHash::from_bytes([byte; 32]);
        let expected = expected_closest(&mesh.floodfills, &search_key);
        let target = target_for(&search_key);
        assert_eq!(target, expected, "the target must be the key's closest");
        seen.push((search_key, target));
    }
    let distinct: std::collections::BTreeSet<RouterHash> =
        seen.iter().map(|(_, target)| *target).collect();
    assert!(
        distinct.len() > 1,
        "different search keys must reach different floodfills: {seen:?}"
    );
}

/// Plan 514: the target is **not** the lowest-hash floodfill.
///
/// This is the old behaviour, pinned as gone. The flat store is a `BTreeMap`
/// keyed by the peer's hash, so its first floodfill advertiser is the
/// lowest-hash one, and the live path used to ask exactly that peer for
/// every exploration.
#[test]
fn the_target_is_not_the_lowest_hash_floodfill() {
    let mesh = mesh();
    let lowest = mesh
        .floodfills
        .iter()
        .copied()
        .min()
        .expect("the lane's floodfill set is never empty");
    // A search key whose closest floodfill is **not** the lowest-hash one.
    // The lane searches for one deterministically rather than assuming it.
    let mut chosen = None;
    for byte in 0u8..=255 {
        let search_key = RouterHash::from_bytes([byte; 32]);
        if expected_closest(&mesh.floodfills, &search_key) != lowest {
            chosen = Some(search_key);
            break;
        }
    }
    let search_key = chosen.expect("some key reaches a peer other than the lowest hash");
    let dispatch = mesh.spine.begin_exploration(
        &mesh.store,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let target = target_of(&dispatch);
    assert_ne!(
        target, lowest,
        "the exploration must not ask the flat store's first floodfill"
    );
}

/// Plan 514: a floodfill the table does not hold is **never** the target.
///
/// The ghost is in the flat store only. The table holds no signed evidence
/// for it, so the table cannot name it -- and the router must not ask it.
#[test]
fn a_store_only_floodfill_is_never_the_target() {
    let mesh = mesh();
    for byte in 0u8..8 {
        let search_key = RouterHash::from_bytes([byte; 32]);
        let target = target_for(&search_key);
        assert_ne!(
            target, mesh.ghost,
            "the table holds no signed evidence for the ghost, so it cannot name it"
        );
        assert!(
            mesh.floodfills.contains(&target),
            "the target must be a floodfill the table holds: {target:?}"
        );
    }
}

/// Plan 514: a table that names **no** floodfill refuses the request.
///
/// The request ends, and the refusal is counted by reason. It is not
/// silently redirected to a peer the table did not name.
#[test]
fn a_table_that_names_no_floodfill_refuses_the_request_and_counts_it() {
    let local = bundle(LOCAL_SEED);
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    // The store holds a floodfill; the table holds none.
    let _ = store.insert(validated(&bundle(GHOST_SEED), "Nf"));

    let mut driver = RoutingTableDriver::new(
        key_of(&local),
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    let table = Arc::new(RoutingTableConsumer::new(driver, true));
    let spine = NetDbDiscoverySpine::new(Arc::clone(&table), LookupPolicy::default());
    spine.advance_time(T0);
    spine.set_reply_path(Some(reply_path()));

    let search_key = RouterHash::from_bytes([0x11; 32]);
    let dispatch = spine.begin_exploration(
        &store,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    assert!(
        matches!(dispatch, DiscoveryDispatch::Terminal { .. }),
        "a table that names no floodfill must end the request: {dispatch:?}"
    );
    assert_eq!(
        spine.counters().explorations_refused_no_target,
        1,
        "the refusal must be counted"
    );
    assert_eq!(
        spine.counters().explorations_started,
        0,
        "a refused request is not a started request"
    );
}

/// Plan 514: a **disabled** table refuses the request and counts it.
///
/// A disabled table is the "off" posture. The refusal is counted under its
/// own reason, so it is never confused with "the table held no candidate".
#[test]
fn a_disabled_table_refuses_the_request_and_counts_it() {
    let local = bundle(LOCAL_SEED);
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    let _ = store.insert(validated(&bundle(ONE_SEED), "Nf"));

    let mut driver = RoutingTableDriver::new(
        key_of(&local),
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    driver.index(&validated(&bundle(ONE_SEED), "Nf"));
    let table = Arc::new(RoutingTableConsumer::new(driver, false));
    let spine = NetDbDiscoverySpine::new(Arc::clone(&table), LookupPolicy::default());
    spine.advance_time(T0);
    spine.set_reply_path(Some(reply_path()));

    let search_key = RouterHash::from_bytes([0x22; 32]);
    let dispatch = spine.begin_exploration(
        &store,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    assert!(
        matches!(dispatch, DiscoveryDispatch::Terminal { .. }),
        "a disabled table must end the request: {dispatch:?}"
    );
    assert_eq!(
        spine.counters().explorations_refused_no_target,
        1,
        "the refusal must be counted"
    );
    // The table counted the fallback under its own reason.
    assert_eq!(
        table.counters().fell_back_disabled,
        1,
        "the table must count the disabled fallback"
    );
}

/// Plan 514: a refused request is **never** redirected to an arbitrary peer.
///
/// The store holds a floodfill the table does not. The request ends; it does
/// not fall back to that peer. This is the difference between *asking the
/// table* and *asking somebody the table did not choose*.
#[test]
fn a_refused_request_is_never_redirected_to_an_arbitrary_peer() {
    let local = bundle(LOCAL_SEED);
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    let _ = store.insert(validated(&bundle(GHOST_SEED), "Nf"));

    let mut driver = RoutingTableDriver::new(
        key_of(&local),
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    let table = Arc::new(RoutingTableConsumer::new(driver, true));
    let spine = NetDbDiscoverySpine::new(Arc::clone(&table), LookupPolicy::default());
    spine.advance_time(T0);
    spine.set_reply_path(Some(reply_path()));

    let search_key = RouterHash::from_bytes([0x33; 32]);
    let dispatch = spine.begin_exploration(
        &store,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    match dispatch {
        DiscoveryDispatch::Send { peer, .. } => {
            panic!("the request must not be sent to {peer:?}, which the table never named");
        }
        other => {
            assert!(
                matches!(other, DiscoveryDispatch::Terminal { .. }),
                "the request must end: {other:?}"
            );
        }
    }
    assert_eq!(
        spine.counters().explorations_refused_no_target,
        1,
        "the refusal must be counted"
    );
}
