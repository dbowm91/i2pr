//! Plan 500: the daemon's routing driver counts the lookup engine's peer
//! budget exhaustion as its own reason.
//!
//! The engine now stops a lookup that spent its per-lookup peer budget
//! and reports `LookupFinalState::PeerBudgetExhausted`. That is a
//! different event from the candidate-list exhaustion
//! (`LookupFinalState::PeerExhausted`), and the driver's receipt must
//! say so: two counted reasons, not one bucket.
//!
//! The lane is entirely in-process: it opens no socket, binds no port
//! and spawns no process. The receipt it inspects stays counts-only —
//! it never carries a peer's hash, its key material or a payload.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{RoutingDispatch, RoutingTableConfig, RoutingTableDriver};
use i2pr_netdb::{
    ExplorationBudget, ExplorationPolicy, LookupFinalState, ReplyPath, RouterHash, RouterInfoStore,
    ValidatedRouterInfo, ValidationContext, daily_routing_key, router_hash,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic seed the lane draws its peers from.
const SEED_BASE: u64 = 0x500_1000;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_000;

/// The engine's default per-lookup peer budget.
const PEER_BUDGET: usize = 8;

/// The exploration's own peer budget. It is deliberately larger than the
/// engine's peer budget, so the *engine's* budget is the one that ends
/// the exploration and not the exploration's own counter.
const EXPLORATION_MAX_PEERS: usize = 16;

/// The number of floodfills the budget-exhaustion lane indexes. It is
/// larger than the engine's peer budget, so candidates are still left
/// when the budget runs out.
const BUDGET_LANE_PEERS: u64 = 10;

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn floodfill(signer: &RouterIdentityBundle) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    let published = Date::from_millis(T0);
    let info = signer
        .sign_router_info(
            published,
            Vec::new(),
            Vec::new(),
            options.build().expect("mapping"),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
    router_hash(signer.identity()).expect("hash")
}

fn routing_key_for(hash: &RouterHash) -> RouterHash {
    let key = daily_routing_key(hash, Date::from_millis(T0)).expect("routing key");
    RouterHash::from_hash(key)
}

/// A digest rendered as lowercase hex.
fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Builds a driver whose exploration budget is wider than the engine's
/// peer budget.
fn driver_with_exploration_budget() -> RoutingTableDriver {
    let local = bundle(SEED_BASE);
    let config = RoutingTableConfig {
        exploration: ExplorationPolicy {
            budget: ExplorationBudget {
                max_peers: EXPLORATION_MAX_PEERS,
                deadline_ms: 30_000,
            },
            ..ExplorationPolicy::default()
        },
        ..RoutingTableConfig::default()
    };
    let mut driver = RoutingTableDriver::new(key_of(&local), Date::from_millis(T0), config);
    driver.advance_time(T0);
    driver.set_reply_path(Some(
        ReplyPath::new(RouterHash::from_bytes([0x49; 32]), 7).expect("reply path"),
    ));
    driver
}

/// Indexes `count` floodfills into the driver's table and into the
/// store, and returns the store.
fn index_floodfills(
    driver: &mut RoutingTableDriver,
    store: &mut RouterInfoStore,
    count: u64,
) -> Vec<RouterHash> {
    let mut keys = Vec::new();
    for index in 0..count {
        let signer = bundle(SEED_BASE + 1 + index);
        let record = floodfill(&signer);
        store.insert(record.clone());
        driver.index(&record);
        keys.push(record.key());
    }
    keys
}

#[test]
fn the_receipt_counts_the_peer_budget_exhaustion_apart_from_the_candidate_exhaustion() {
    // ---- The budget exhaustion -----------------------------------------
    let mut driver = driver_with_exploration_budget();
    let mut store = RouterInfoStore::default();
    let peers = index_floodfills(&mut driver, &mut store, BUDGET_LANE_PEERS);
    assert_eq!(
        driver.table().len() as u64,
        BUDGET_LANE_PEERS,
        "every floodfill must enter the table"
    );

    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let search_routing_key = routing_key_for(&search_key);
    let dispatch = driver.begin_exploration(&store, search_key, search_routing_key);
    assert!(
        matches!(dispatch, RoutingDispatch::Send { .. }),
        "the exploration must be dispatchable: {dispatch:?}"
    );
    // Seven more attempts fill the engine's peer budget.
    for attempt in 1..PEER_BUDGET {
        let dispatch = driver.exploration_next(&store);
        assert!(
            matches!(dispatch, RoutingDispatch::Send { .. }),
            "attempt {attempt} must be dispatchable: {dispatch:?}"
        );
    }
    // The next advance finds the engine's peer budget spent. The
    // exploration's own budget is wider, so it is the engine's budget
    // that ends the exploration.
    let dispatch = driver.exploration_next(&store);
    let RoutingDispatch::Terminal { final_state, .. } = dispatch else {
        panic!("the peer budget must end the exploration: {dispatch:?}");
    };
    assert_eq!(
        final_state,
        LookupFinalState::PeerBudgetExhausted,
        "the driver must report the engine's peer budget exhaustion"
    );

    let receipts = driver.receipts();
    let terminal = receipts
        .iter()
        .find(|receipt| receipt.kind.kind_label() == "terminal")
        .expect("a terminal receipt");
    assert_eq!(
        terminal.terminal_counts.get("peer-budget-exhausted"),
        Some(&1),
        "the budget exhaustion is counted under its own label"
    );
    assert_eq!(
        terminal.terminal_counts.get("peer-exhausted"),
        None,
        "the budget exhaustion must not land in the candidate-exhaustion bucket"
    );

    // ---- The candidate exhaustion --------------------------------------
    // The control: one candidate, so the candidate list runs out before
    // the engine's peer budget does.
    let mut driver = driver_with_exploration_budget();
    let mut store = RouterInfoStore::default();
    index_floodfills(&mut driver, &mut store, 1);

    let dispatch = driver.begin_exploration(&store, search_key, search_routing_key);
    assert!(
        matches!(dispatch, RoutingDispatch::Send { .. }),
        "the exploration must be dispatchable: {dispatch:?}"
    );
    let dispatch = driver.exploration_next(&store);
    let RoutingDispatch::Terminal { final_state, .. } = dispatch else {
        panic!("the candidate list must end the exploration: {dispatch:?}");
    };
    assert_eq!(
        final_state,
        LookupFinalState::PeerExhausted,
        "a candidate-list exhaustion keeps its own final state"
    );

    let receipts = driver.receipts();
    let terminal = receipts
        .iter()
        .find(|receipt| receipt.kind.kind_label() == "terminal")
        .expect("a terminal receipt");
    assert_eq!(
        terminal.terminal_counts.get("peer-exhausted"),
        Some(&1),
        "the candidate exhaustion is counted under its own label"
    );
    assert_eq!(
        terminal.terminal_counts.get("peer-budget-exhausted"),
        None,
        "the candidate exhaustion must not land in the peer-budget bucket"
    );

    // ---- The receipt stays counts-only ---------------------------------
    let rendered = format!("{receipts:?}");
    for peer in &peers {
        assert!(
            !rendered.contains(&hex(peer.as_bytes())),
            "a receipt must not carry a peer hash"
        );
    }
}
