//! Plan 497 — the routing table's consumer seam.
//!
//! Plans 488 and 489 built the peer-profile store, the floodfill-indexed
//! routing table and the optimized lookup plan, and both recorded that
//! nothing in the daemon called them. Plan 497 is the consumer phase: the
//! NetDB tunnel-building coordinator asks the routing table for the
//! floodfills it queries, and the flat store stays the authority.
//!
//! The lane holds three peers:
//!
//! - **alpha** — a floodfill that is in the routing table **and** in the
//!   flat store. It is the peer the table-driven lookup must ask.
//! - **zeta** — a floodfill that is in the flat store **only**. It is the
//!   discriminator: a lookup that drew its candidates from the flat store
//!   would ask zeta — the routing key is zeta's own key, so the flat
//!   store's nearest-first choice is unambiguously zeta — and a lookup that
//!   drew them from the routing table must not.
//! - **theta** — the lookup's target. It is excluded from the candidates.
//!
//! The lane asserts:
//!
//! - with the table attached, the lookup asks **alpha** and never zeta;
//! - without the table, the lookup asks **zeta** — the control that proves
//!   the discriminator is sharp;
//! - with the table disabled, the lookup falls back to zeta and the
//!   fallback is counted by reason;
//! - with an empty table, the lookup falls back to zeta and the fallback is
//!   counted by reason;
//! - with the table attached, the coordinator's own candidate list is
//!   unchanged — the table changes what the lookup asks, not what the
//!   store holds.
//!
//! The table may only **reorder** the flat store's selection, never narrow
//! it. The row that proves the candidate *set* survives the reorder lives
//! in the coordinator's own unit tests, which can drive the lookup past its
//! first attempt; the rows here prove the public-API behaviour.
//!
//! The lane is entirely in-process: it opens no socket, binds no port and
//! spawns no process.

use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{
    RoutingTableConfig, RoutingTableConsumer, RoutingTableDriver,
};
use i2pr_daemon::netdb_tunnels::NetDbTunnelCoordinator;
use i2pr_netdb::{
    LookupAction, LookupPolicy, ReplyPath, RouterHash, RouterInfoStoreConfig, router_hash,
};
use i2pr_proto::{Date, MAX_COMMON_STRUCTURE_SIZE, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the lane peers.
const LOCAL_SEED: u64 = 0x4970_0001;
const ALPHA_SEED: u64 = 0x4970_0002;
const ZETA_SEED: u64 = 0x4970_0003;
const THETA_SEED: u64 = 0x4970_0004;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_000;

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
    router_hash(signer.identity()).expect("hash")
}

/// Signs a floodfill RouterInfo and returns its encoded bytes.
fn floodfill_bytes(signer: &RouterIdentityBundle) -> Vec<u8> {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    let published = Date::from_millis(T0);
    let info = signer
        .sign_router_info(
            published,
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("sign");
    info.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode")
}

/// A reply path for the exploratory reply.
fn reply_path() -> ReplyPath {
    ReplyPath::new(RouterHash::from_bytes([0x49; 32]), 7).expect("reply path")
}

/// Builds a coordinator holding alpha and zeta as floodfill advertisers.
fn coordinator_with(
    alpha: &RouterIdentityBundle,
    zeta: &RouterIdentityBundle,
) -> NetDbTunnelCoordinator {
    let mut coordinator =
        NetDbTunnelCoordinator::new(LookupPolicy::default(), RouterInfoStoreConfig::default());
    coordinator.advance_time(T0);
    coordinator
        .bootstrap_reference_router_info(&floodfill_bytes(alpha), Date::from_millis(T0))
        .expect("alpha is admitted");
    coordinator
        .bootstrap_reference_router_info(&floodfill_bytes(zeta), Date::from_millis(T0))
        .expect("zeta is admitted");
    coordinator
}

/// Builds an enabled routing-table consumer holding alpha only.
fn consumer_with_alpha(
    alpha: &RouterIdentityBundle,
    local: &RouterIdentityBundle,
) -> RoutingTableConsumer {
    let mut driver = RoutingTableDriver::new(
        key_of(local),
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    let record = i2pr_netdb::ValidatedRouterInfo::from_router_info(
        i2pr_proto::RouterInfo::decode(&floodfill_bytes(alpha), MAX_COMMON_STRUCTURE_SIZE)
            .expect("decode"),
        None,
        i2pr_netdb::ValidationContext::new(Date::from_millis(T0)),
    )
    .expect("validate");
    driver.index(&record);
    RoutingTableConsumer::new(driver, true)
}

/// The peer a lookup asked.
fn asked_peer(action: &LookupAction) -> RouterHash {
    match action {
        LookupAction::SendDatabaselookup { peer, .. } => *peer,
        other => panic!("the lookup must be dispatchable: {other:?}"),
    }
}

#[test]
fn the_routing_table_selects_the_lookup_peer_and_the_flat_store_does_not() {
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let theta = bundle(THETA_SEED);

    let consumer = Arc::new(consumer_with_alpha(&alpha, &local));
    let mut coordinator = coordinator_with(&alpha, &zeta).with_routing_table(Arc::clone(&consumer));

    // The routing key is zeta's own key, so the flat store's nearest-first
    // choice is unambiguously zeta. The table holds alpha only.
    let (_, action) = coordinator
        .begin_tunnel_lookup(key_of(&theta), &key_of(&zeta), reply_path())
        .expect("the lookup is dispatchable");
    let peer = asked_peer(&action);
    assert_eq!(
        peer,
        key_of(&alpha),
        "the lookup must ask the peer the routing table selected"
    );
    assert_ne!(
        peer,
        key_of(&zeta),
        "the lookup must not ask the store-only peer"
    );

    let counters = consumer.counters();
    assert_eq!(counters.answered, 1, "the table answered the lookup");
    assert_eq!(
        counters.fell_back(),
        0,
        "the table must not have fallen back"
    );
}

#[test]
fn without_the_routing_table_the_flat_store_selects() {
    // The control: the same setup with no table attached must ask zeta.
    // Without this row the discriminator above would prove nothing.
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let theta = bundle(THETA_SEED);

    let mut coordinator = coordinator_with(&alpha, &zeta);
    let (_, action) = coordinator
        .begin_tunnel_lookup(key_of(&theta), &key_of(&zeta), reply_path())
        .expect("the lookup is dispatchable");
    assert_eq!(
        asked_peer(&action),
        key_of(&zeta),
        "the flat store must select zeta when the table is absent"
    );
}

#[test]
fn a_disabled_table_falls_back_to_the_flat_store_and_counts_the_reason() {
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let theta = bundle(THETA_SEED);

    let consumer = Arc::new({
        let mut driver = RoutingTableDriver::new(
            key_of(&local),
            Date::from_millis(T0),
            RoutingTableConfig::default(),
        );
        driver.advance_time(T0);
        RoutingTableConsumer::new(driver, false)
    });
    let mut coordinator = coordinator_with(&alpha, &zeta).with_routing_table(Arc::clone(&consumer));

    let (_, action) = coordinator
        .begin_tunnel_lookup(key_of(&theta), &key_of(&zeta), reply_path())
        .expect("the lookup is dispatchable");
    assert_eq!(
        asked_peer(&action),
        key_of(&zeta),
        "a disabled table must fall back to the flat store"
    );
    let counters = consumer.counters();
    assert_eq!(counters.answered, 0, "a disabled table must not answer");
    assert_eq!(
        counters.fell_back_disabled, 1,
        "the fallback must be counted by reason"
    );
    assert_eq!(counters.fell_back_no_candidate, 0);
}

#[test]
fn an_empty_table_falls_back_to_the_flat_store_and_counts_the_reason() {
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let theta = bundle(THETA_SEED);

    // The table is enabled but holds no peer for the key: the local
    // identity is not a floodfill, so it never enters the table.
    let consumer = Arc::new({
        let mut driver = RoutingTableDriver::new(
            key_of(&local),
            Date::from_millis(T0),
            RoutingTableConfig::default(),
        );
        driver.advance_time(T0);
        RoutingTableConsumer::new(driver, true)
    });
    let mut coordinator = coordinator_with(&alpha, &zeta).with_routing_table(Arc::clone(&consumer));

    let (_, action) = coordinator
        .begin_tunnel_lookup(key_of(&theta), &key_of(&zeta), reply_path())
        .expect("the lookup is dispatchable");
    assert_eq!(
        asked_peer(&action),
        key_of(&zeta),
        "an empty table must fall back to the flat store"
    );
    let counters = consumer.counters();
    assert_eq!(counters.answered, 0, "an empty table must not answer");
    assert_eq!(
        counters.fell_back_no_candidate, 1,
        "the fallback must be counted by reason"
    );
    assert_eq!(counters.fell_back_disabled, 0);
}

#[test]
fn the_routing_table_does_not_change_what_the_store_holds() {
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let theta = bundle(THETA_SEED);

    let consumer = Arc::new(consumer_with_alpha(&alpha, &local));
    let mut coordinator = coordinator_with(&alpha, &zeta).with_routing_table(Arc::clone(&consumer));
    let (_, action) = coordinator
        .begin_tunnel_lookup(key_of(&theta), &key_of(&zeta), reply_path())
        .expect("the lookup is dispatchable");
    let _ = asked_peer(&action);

    // The table changes what the lookup asks, not what the authoritative
    // store holds. The flat store still carries both floodfills.
    assert_eq!(coordinator.store().len(), 2, "the store must be unchanged");
    assert_eq!(coordinator.floodfill_count(), 2);
    assert_eq!(consumer.len(), 1, "the table holds alpha only");
}

#[test]
fn the_routing_table_does_not_narrow_the_flat_store_selection() {
    // The no-regression row: the table reorders the flat store's
    // selection, so the selection the coordinator computes through the
    // authoritative store is the same set with and without the table.
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let theta = bundle(THETA_SEED);

    let without_table = coordinator_with(&alpha, &zeta);
    let with_table = coordinator_with(&alpha, &zeta)
        .with_routing_table(Arc::new(consumer_with_alpha(&alpha, &local)));

    let plain = without_table.candidate_hashes(&key_of(&theta), &key_of(&zeta));
    let tabled = with_table.candidate_hashes(&key_of(&theta), &key_of(&zeta));
    assert_eq!(
        plain, tabled,
        "the table must not change the flat store's candidate set"
    );
    assert_eq!(
        plain,
        vec![key_of(&zeta), key_of(&alpha)],
        "the flat store's nearest-first selection is zeta then alpha"
    );
}
