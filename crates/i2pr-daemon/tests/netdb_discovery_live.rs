//! Plan 513 out-of-crate lane for the live NetDB discovery spine.
//!
//! Plan 497 recorded that the shared routing table had **no production
//! reader**: the daemon populated and persisted the table, and the
//! coordinator asked it, but the coordinator was never in the daemon's
//! service graph. Plan 513 gives the table a live reader and makes the
//! router's DHT discovery a real, driven cycle.
//!
//! This lane proves the **behaviour** the Plan 513 mechanism is supposed to
//! produce, through the **public** daemon API only — no private internals, no
//! `pub(crate)` reach-ins.
//!
//! The lane is local and deterministic:
//!
//! - fixed seeds (a fixed-seed ChaCha);
//! - an explicit `now_ms` everywhere — the lane never reads a wall clock;
//! - no socket, no environment variable, no `#[ignore]`, no stock peer, and
//!   no sleeping.
//!
//! What the lane proves, one assertion per test:
//!
//! 1. the exploratory lookup's dispatch names a peer and the transport seam;
//! 2. the RouterInfo-kind dispatch rides the **outbound** exploratory tunnel,
//!    never a direct peer link;
//! 3. the RouterInfo-kind request carries the **inbound** exploratory
//!    tunnel's reply path;
//! 4. the routing table's preference **reorders** the flat store's selection
//!    and never **narrows** it;
//! 5. every fallback is counted by reason;
//! 6. an exploration with no reply path is refused, and the refusal is
//!    counted;
//! 7. the exploration's answer queues the named peer for resolution;
//! 8. a named peer enters the routing table as a **suggestion**, never as
//!    signed evidence;
//! 9. a reply from a peer the exploration never asked introduces nothing;
//! 10. the resolved record is ingested into the router's NetDB;
//! 11. the spine's unresolved-peer queue is bounded;
//! 12. a delivery is counted as delivered or failed — never silently;
//! 13. an exploration is **finished** once its reply arrives, so one
//!     answered exploration cannot stall the cycle;
//! 14. a request the wire never answered is **abandoned** when its
//!     deadline passes, and counted — never retried in a loop;
//! 15. a suggestion from a peer the table does **not** hold with signed
//!     evidence is refused, and the refusal is counted.
//!
//! One behaviour is **not** drivable through the public API and is reported
//! rather than papered over: the product's tick that drives the spine needs a
//! live socket and a live transport. The spine's own cycle and the two
//! deliveries' shape are public and are proved here; the wiring that drives
//! them is covered by the structural guard (`netdb_discovery_boundary.rs`).

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{
    DiscoveryDelivery, DiscoveryDispatch, NetDbDiscoverySpine, RoutingTableConfig,
    RoutingTableConsumer, RoutingTableDriver,
};
use i2pr_netdb::{
    LookupAction, LookupPolicy, ReplyPath, RouterHash, RouterInfoStore, RouterInfoStoreConfig,
    ValidatedRouterInfo, ValidationContext, daily_routing_key, router_hash,
};
use i2pr_proto::{
    DatabaseSearchReplyMessage, DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload,
    Mapping,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the lane's peers.
const LOCAL_SEED: u64 = 0x5130_0001;
const ALPHA_SEED: u64 = 0x5130_0002;
const BETA_SEED: u64 = 0x5130_0003;
const GAMMA_SEED: u64 = 0x5130_0004;
const DELTA_SEED: u64 = 0x5130_0005;
const THETA_SEED: u64 = 0x5130_0006;

/// The clock the lane runs on, in milliseconds.
const T0: u64 = 1_700_000_000_000;

/// The maximum encoded size of a `RouterInfo` in the lane.
const MAX_ROUTER_INFO_BYTES: usize = 16 * 1024;

/// The gateway of the lane's inbound exploratory tunnel.
const REPLY_GATEWAY_BYTE: u8 = 0x51;

/// The tunnel id of the lane's inbound exploratory tunnel.
const REPLY_TUNNEL_ID: u32 = 0x0051_3007;

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

/// The lane's inbound exploratory tunnel's reply path.
fn reply_path() -> ReplyPath {
    ReplyPath::new(
        RouterHash::from_bytes([REPLY_GATEWAY_BYTE; 32]),
        REPLY_TUNNEL_ID,
    )
    .expect("reply path")
}

/// Compresses bytes the way a `DatabaseStore` carries a RouterInfo.
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("compress");
    encoder.finish().expect("finish")
}

/// The lane's mesh: a store, a routing table and the spine that joins them.
struct Mesh {
    store: RouterInfoStore,
    table: Arc<RoutingTableConsumer>,
    spine: NetDbDiscoverySpine,
    alpha: RouterHash,
    beta: RouterHash,
    gamma: RouterHash,
    delta: RouterHash,
    theta: RouterHash,
    theta_record: ValidatedRouterInfo,
}

/// Builds the lane's mesh.
///
/// - **alpha** and **gamma** are floodfills in the store *and* in the routing
///   table, with signed evidence;
/// - **beta** is a floodfill in the store **only**. It is the discriminator:
///   a selection narrowed to the table would never ask beta, and a selection
///   that ignored the table's preference would never prefer alpha over it;
/// - **delta** is a peer the exploration's answer names. It is in neither the
///   store nor the table;
/// - **theta** is the resolution's target. Its `RouterInfo` is the record
///   the RouterInfo-kind lookup resolves.
fn mesh() -> Mesh {
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let beta = bundle(BETA_SEED);
    let gamma = bundle(GAMMA_SEED);
    let delta = bundle(DELTA_SEED);
    let theta = bundle(THETA_SEED);

    let alpha_record = validated(&alpha, "Nf");
    let beta_record = validated(&beta, "Nf");
    let gamma_record = validated(&gamma, "Nf");
    let theta_record = validated(&theta, "N");

    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    let _ = store.insert(alpha_record.clone());
    let _ = store.insert(beta_record.clone());
    let _ = store.insert(gamma_record.clone());

    let local_key = key_of(&local);
    let mut driver = RoutingTableDriver::new(
        local_key,
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    // The table's answer is a preference. It is seeded with alpha and gamma
    // only: beta is deliberately absent, so a selection narrowed to the
    // table would drop a peer the flat store could serve.
    driver.index(&alpha_record);
    driver.index(&gamma_record);

    let table = Arc::new(RoutingTableConsumer::new(driver, true));
    let spine = NetDbDiscoverySpine::new(Arc::clone(&table), LookupPolicy::default());
    spine.advance_time(T0);
    spine.set_reply_path(Some(reply_path()));

    Mesh {
        store,
        table,
        spine,
        alpha: alpha_record.key(),
        beta: beta_record.key(),
        gamma: gamma_record.key(),
        delta: key_of(&delta),
        theta: theta_record.key(),
        theta_record,
    }
}

/// The dispatch's request body, or a panic naming the wrong variant.
fn sent(dispatch: &DiscoveryDispatch) -> (RouterHash, i2pr_proto::DatabaseLookupMessage) {
    match dispatch {
        DiscoveryDispatch::Send { peer, message, .. } => (*peer, message.clone()),
        other => panic!("the request must be dispatchable: {other:?}"),
    }
}

/// Plan 513 requirement 1: the exploratory lookup's dispatch names a peer and
/// the transport seam.
///
/// The request is an exploratory `DatabaseLookup` — wire lookup type `3` —
/// addressed **directly** to the floodfill being asked.
#[test]
fn the_exploration_names_the_peer_and_the_transport_seam() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let routing_key = routing_key_for(&search_key);

    let dispatch =
        mesh.spine
            .begin_exploration(mesh.alpha, search_key, routing_key, Some(reply_path()));
    let (peer, message) = sent(&dispatch);

    assert!(
        peer == mesh.alpha || peer == mesh.gamma,
        "the exploration must name a floodfill the router already knows: {peer:?}"
    );
    assert_ne!(
        peer, mesh.beta,
        "the store-only peer must never be the exploration's target"
    );
    assert_eq!(
        message.lookup_type, 3,
        "an exploratory lookup uses wire lookup type 3"
    );
    assert_eq!(message.key, *search_key.as_hash());
    assert!(
        matches!(
            dispatch,
            DiscoveryDispatch::Send {
                delivery: DiscoveryDelivery::Direct,
                ..
            }
        ),
        "an exploratory request is delivered directly: {dispatch:?}"
    );
    assert_eq!(
        mesh.spine.counters().explorations_started,
        1,
        "the composed request is counted"
    );
}

/// Plan 513 requirement 2: the RouterInfo-kind dispatch rides the **outbound**
/// exploratory tunnel, never a direct peer link.
///
/// Plan 102's closed authority amendment forbids substituting a direct
/// `DatabaseLookup` over NTCP2 for the standard exploratory-tunnel path.
#[test]
fn the_routerinfo_request_rides_the_outbound_exploratory_tunnel() {
    let mesh = mesh();
    let routing_key = routing_key_for(&mesh.theta);

    let dispatch =
        mesh.spine
            .begin_peer_resolution(&mesh.store, mesh.theta, routing_key, Some(reply_path()));
    let (_peer, message) = sent(&dispatch);

    assert_eq!(
        message.lookup_type, 2,
        "a RouterInfo lookup is wire lookup type 2"
    );
    assert!(
        matches!(
            dispatch,
            DiscoveryDispatch::Send {
                delivery: DiscoveryDelivery::OutboundExploratoryTunnel,
                ..
            }
        ),
        "a RouterInfo request must ride the outbound exploratory tunnel: {dispatch:?}"
    );
    assert_eq!(
        mesh.spine.counters().resolutions_started,
        1,
        "the composed request is counted"
    );
}

/// Plan 513 requirement 3: the RouterInfo-kind request carries the **inbound**
/// exploratory tunnel's reply path.
///
/// The reply travels the inbound tunnel, so the request must name that
/// tunnel's gateway and its tunnel id.
#[test]
fn the_routerinfo_request_carries_the_inbound_exploratory_tunnels_reply_path() {
    let mesh = mesh();
    let routing_key = routing_key_for(&mesh.theta);

    let dispatch =
        mesh.spine
            .begin_peer_resolution(&mesh.store, mesh.theta, routing_key, Some(reply_path()));
    let (_peer, message) = sent(&dispatch);

    assert!(
        message.delivery_flag,
        "a tunnelled reply must set the delivery flag"
    );
    assert_eq!(
        message.reply_tunnel_id,
        Some(REPLY_TUNNEL_ID),
        "the request must name the inbound exploratory tunnel"
    );
    assert_eq!(
        message.from,
        *RouterHash::from_bytes([REPLY_GATEWAY_BYTE; 32]).as_hash(),
        "the request must name the inbound exploratory tunnel's gateway"
    );
}

/// Plan 513 requirement 4: the routing table's preference **reorders** the
/// flat store's selection and never **narrows** it.
///
/// - With a preference the table holds, the preferred peer is asked first.
/// - With a preference the table does **not** hold, the flat store's own
///   selection still stands: the lookup is never narrowed to nothing.
/// - With no preference at all, the flat store's own selection stands.
#[test]
fn the_table_preference_reorders_and_never_narrows_the_selection() {
    let mesh = mesh();
    let routing_key = routing_key_for(&mesh.theta);

    // A preference the flat store could serve: the preferred peer leads.
    let mut seam = i2pr_daemon::netdb_seam::NetDbSeam::new(LookupPolicy::default());
    let action = seam.begin_routerinfo_lookup(
        &mesh.store,
        1,
        mesh.theta,
        &routing_key,
        Some(&[mesh.gamma]),
        Some(reply_path()),
    );
    let LookupAction::SendDatabaselookup { peer, .. } = action else {
        panic!("the lookup must be dispatchable");
    };
    assert_eq!(
        peer, mesh.gamma,
        "the table's preferred peer must lead the selection"
    );

    // A preference the flat store could **not** serve: the flat store's own
    // selection still stands. The lookup is never narrowed to nothing.
    let unknown = RouterHash::from_bytes([0x7f; 32]);
    let mut seam = i2pr_daemon::netdb_seam::NetDbSeam::new(LookupPolicy::default());
    let action = seam.begin_routerinfo_lookup(
        &mesh.store,
        2,
        mesh.theta,
        &routing_key,
        Some(&[unknown]),
        Some(reply_path()),
    );
    let LookupAction::SendDatabaselookup { peer, .. } = action else {
        panic!("the lookup must still be dispatchable");
    };
    assert!(
        peer != unknown,
        "a peer the flat store could not serve must never be asked"
    );
    assert!(
        peer == mesh.alpha || peer == mesh.beta || peer == mesh.gamma,
        "the flat store's own selection must still stand: {peer:?}"
    );

    // No preference at all: the flat store's own selection stands.
    let mut seam = i2pr_daemon::netdb_seam::NetDbSeam::new(LookupPolicy::default());
    let action = seam.begin_routerinfo_lookup(
        &mesh.store,
        3,
        mesh.theta,
        &routing_key,
        None,
        Some(reply_path()),
    );
    let LookupAction::SendDatabaselookup { peer, .. } = action else {
        panic!("the lookup must be dispatchable");
    };
    assert!(
        peer == mesh.alpha || peer == mesh.beta || peer == mesh.gamma,
        "the flat store's own selection must stand: {peer:?}"
    );
}

/// Plan 513 requirement 5: every fallback is counted by reason.
///
/// A table that is switched off is a **counted** fallback, not a silent one.
#[test]
fn every_fallback_is_counted_by_reason() {
    let local = bundle(LOCAL_SEED);
    let mut driver = RoutingTableDriver::new(
        key_of(&local),
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    let table = Arc::new(RoutingTableConsumer::new(driver, false));
    let spine = NetDbDiscoverySpine::new(Arc::clone(&table), LookupPolicy::default());
    spine.advance_time(T0);
    spine.set_reply_path(Some(reply_path()));

    // The flat store holds one floodfill. The request still starts on the
    // flat store's own selection: a table that is switched off changes the
    // *preference*, never the store.
    let alpha = bundle(ALPHA_SEED);
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    let _ = store.insert(validated(&alpha, "Nf"));
    let target = RouterHash::from_bytes([0x11; 32]);
    let dispatch =
        spine.begin_peer_resolution(&store, target, routing_key_for(&target), Some(reply_path()));

    assert!(
        matches!(dispatch, DiscoveryDispatch::Send { .. }),
        "a table that is switched off does not stop the request: the flat store's own selection stands: {dispatch:?}"
    );
    assert_eq!(
        spine.counters().fell_back_disabled,
        1,
        "the fallback must be counted under its own reason"
    );
    assert_eq!(
        spine.counters().resolutions_started,
        1,
        "the request still starts on the flat store's own selection"
    );
    assert_eq!(
        table.counters().fell_back_disabled,
        1,
        "the table counts its own fallback too"
    );
}

/// Plan 513 requirement 6: an exploration with no reply path is refused, and
/// the refusal is counted.
///
/// Without an inbound exploratory tunnel there is nowhere for the answer to
/// come back to, so the request is refused rather than sent.
#[test]
fn an_exploration_without_a_reply_path_is_refused_and_counted() {
    let mesh = mesh();
    // The router holds no inbound exploratory tunnel at all.
    mesh.spine.set_reply_path(None);
    let search_key = RouterHash::from_bytes([0x5e; 32]);

    let dispatch =
        mesh.spine
            .begin_exploration(mesh.alpha, search_key, routing_key_for(&search_key), None);

    assert!(
        matches!(dispatch, DiscoveryDispatch::NeedsReplyPath { .. }),
        "without a reply path the request must not be composed: {dispatch:?}"
    );
    assert_eq!(
        mesh.spine.counters().fell_back_no_reply_path,
        1,
        "the refusal must be counted"
    );
    assert_eq!(
        mesh.spine.counters().explorations_started,
        0,
        "a refused request is not a started request"
    );
}

/// Plan 513 requirement 7: the exploration's answer queues the named peer for
/// resolution.
///
/// A named peer is a **name**, not a peer: the router cannot ask it until a
/// RouterInfo-kind lookup has returned its signed record.
#[test]
fn the_explorations_answer_queues_the_named_peer_for_resolution() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let dispatch = mesh.spine.begin_exploration(
        mesh.alpha,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: vec![*mesh.delta.as_hash()],
        from: *mesh.alpha.as_hash(),
    };
    let outcome = mesh
        .spine
        .ingest_search_reply(&reply)
        .expect("the exploration reply is ingested");

    assert_eq!(outcome.suggested, 1);
    assert!(
        !outcome.unknown_peer,
        "alpha is the peer the exploration asked"
    );
    assert_eq!(
        mesh.spine.pending_resolutions(),
        vec![mesh.delta],
        "the named peer must be queued for resolution"
    );
    assert_eq!(
        mesh.spine.counters().search_replies_ingested,
        1,
        "the ingested reply is counted"
    );
}

/// Plan 513 requirement 8: a named peer enters the routing table as a
/// **suggestion**, never as signed evidence.
///
/// A suggestion is never evidence: the suggesting peer must itself be
/// indexed with signed evidence, or the table refuses the suggestion.
#[test]
fn a_named_peer_is_offered_to_the_table_as_a_suggestion_not_as_evidence() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let dispatch = mesh.spine.begin_exploration(
        mesh.alpha,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    let (signed_before, suggested_before) = mesh.table.evidence_counts();
    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: vec![*mesh.delta.as_hash()],
        from: *mesh.alpha.as_hash(),
    };
    let outcome = mesh
        .spine
        .ingest_search_reply(&reply)
        .expect("the exploration reply is ingested");

    assert_eq!(outcome.indexed, 1, "alpha's suggestion must be accepted");
    assert_eq!(outcome.refused, 0);
    let (signed_after, suggested_after) = mesh.table.evidence_counts();
    assert_eq!(
        signed_after, signed_before,
        "a suggestion must never become signed evidence"
    );
    assert_eq!(
        suggested_after,
        suggested_before + 1,
        "the named peer must enter the table as a suggestion"
    );
    assert_eq!(
        mesh.spine.counters().suggestions_offered,
        1,
        "the offered suggestion is counted"
    );
}

/// Plan 513 requirement 9: a reply from a peer the exploration never asked
/// introduces nothing.
///
/// The reply's sender is **unauthenticated**, so a reply from an unknown peer
/// must not be able to name new peers into the router's table.
#[test]
fn a_reply_from_a_peer_the_exploration_never_asked_introduces_nothing() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let dispatch = mesh.spine.begin_exploration(
        mesh.alpha,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    let stranger = RouterHash::from_bytes([0x66; 32]);
    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: vec![*mesh.delta.as_hash()],
        from: *stranger.as_hash(),
    };
    let outcome = mesh
        .spine
        .ingest_search_reply(&reply)
        .expect("the reply is ingested and refused");

    assert!(
        outcome.unknown_peer,
        "a reply from a peer the exploration never asked must be flagged"
    );
    assert_eq!(outcome.indexed, 0, "no peer may be introduced");
    assert!(
        mesh.spine.pending_resolutions().is_empty(),
        "an unknown peer's names must not be queued for resolution"
    );
}

/// Plan 513 requirement 10: the resolved record is ingested into the router's
/// NetDB.
///
/// The record is validated by the lookup engine's own rules, and only the
/// lookup's own target may be ingested.
#[test]
fn the_resolved_record_is_ingested_into_the_netdb() {
    let mesh = mesh();
    // The RouterInfo-kind validation clock is advanced by the transport
    // owner on every tick; without it a record published now would be
    // refused as being from the future.
    mesh.spine.set_routerinfo_now_seconds((T0 / 1000) as u32);
    let routing_key = routing_key_for(&mesh.theta);
    let dispatch =
        mesh.spine
            .begin_peer_resolution(&mesh.store, mesh.theta, routing_key, Some(reply_path()));
    let _ = sent(&dispatch);

    assert!(
        !mesh.store.contains(&mesh.theta),
        "the target is not known before the lookup answers"
    );

    let encoded = mesh
        .theta_record
        .encoded(MAX_ROUTER_INFO_BYTES)
        .expect("encode");
    let payload = DeferredPayload::new(gzip(&encoded), MAX_ROUTER_INFO_BYTES).expect("payload");
    let store_message = DatabaseStoreMessage {
        key: *mesh.theta.as_hash(),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(payload),
    };

    let mut store = mesh.store;
    let outcome = mesh
        .spine
        .ingest_routerinfo_store(&mut store, &store_message)
        .expect("the answer is ingested");

    assert!(outcome.resolved, "the record must complete the lookup");
    assert!(
        store.contains(&mesh.theta),
        "the resolved peer must enter the router's NetDB"
    );
    assert_eq!(
        mesh.spine.counters().records_ingested,
        1,
        "the ingested record is counted"
    );
}

/// Plan 513 requirement 11: the spine's unresolved-peer queue is bounded.
#[test]
fn the_spine_is_bounded() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let dispatch = mesh.spine.begin_exploration(
        mesh.alpha,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    // One reply names many peers. The queue must not grow past its bound.
    let many: Vec<i2pr_proto::Hash> = (0u8..64)
        .map(|index| i2pr_proto::Hash::from_bytes([index; 32]))
        .collect();
    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: many,
        from: *mesh.alpha.as_hash(),
    };
    mesh.spine
        .ingest_search_reply(&reply)
        .expect("the exploration reply is ingested");

    assert!(
        mesh.spine.pending_resolutions().len()
            <= i2pr_daemon::netdb_participation::MAX_PENDING_RESOLUTIONS,
        "the unresolved-peer queue must stay bounded"
    );
}

/// Plan 513 requirement 12: a delivery is counted as delivered or failed —
/// never silently.
#[test]
fn a_delivery_is_counted_as_delivered_or_failed() {
    let mesh = mesh();
    assert_eq!(mesh.spine.counters().requests_delivered, 0);

    mesh.spine.record_delivery(true);
    assert_eq!(mesh.spine.counters().requests_delivered, 1);
    assert_eq!(mesh.spine.counters().delivery_failures, 0);

    mesh.spine.record_delivery(false);
    assert_eq!(mesh.spine.counters().requests_delivered, 1);
    assert_eq!(
        mesh.spine.counters().delivery_failures,
        1,
        "a refused delivery must be counted"
    );
}

/// Plan 513 requirement 13: an exploration is finished once its reply
/// arrives.
///
/// One exploration at a time means the next round may only start once the
/// previous one is over. An exploration that is never finished would stall
/// the cycle for good.
#[test]
fn an_exploration_is_finished_once_its_reply_arrives() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let dispatch = mesh.spine.begin_exploration(
        mesh.alpha,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    assert!(
        mesh.spine.exploration_in_flight(),
        "the exploration is in flight until its reply arrives"
    );

    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: vec![*mesh.delta.as_hash()],
        from: *mesh.alpha.as_hash(),
    };
    mesh.spine
        .ingest_search_reply(&reply)
        .expect("the exploration reply is ingested");

    assert!(
        !mesh.spine.exploration_in_flight(),
        "the answered exploration must be finished"
    );
}

/// Plan 513 requirement 14: a request the wire never answered is abandoned
/// when its deadline passes, and counted.
///
/// A request that is never retried in a loop is what keeps the discovery
/// cycle alive when a peer goes silent.
#[test]
fn a_request_the_wire_never_answered_is_abandoned_when_its_deadline_passes() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let dispatch = mesh.spine.begin_exploration(
        mesh.alpha,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    // A day is past every legal exploration deadline.
    mesh.spine.advance_time(T0 + 86_400_000);

    assert!(
        !mesh.spine.exploration_in_flight(),
        "an unanswered exploration must be abandoned"
    );
    assert_eq!(
        mesh.spine.counters().explorations_expired,
        1,
        "the abandoned request must be counted"
    );

    // The resolution's own deadline is the lookup policy's total deadline.
    let target = RouterHash::from_bytes([0x11; 32]);
    let dispatch = mesh.spine.begin_peer_resolution(
        &mesh.store,
        target,
        routing_key_for(&target),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);
    assert!(
        mesh.spine.resolution_in_flight(),
        "the resolution is in flight until its reply arrives"
    );
    // The clock has already moved on, so the resolution's own deadline is
    // later than the exploration's was. A second day passes it too.
    mesh.spine.advance_time(T0 + 172_800_000);
    assert!(
        !mesh.spine.resolution_in_flight(),
        "an unanswered resolution must be abandoned"
    );
    assert_eq!(
        mesh.spine.counters().resolutions_expired,
        1,
        "the abandoned request must be counted"
    );
}

/// Plan 513 requirement 15: a suggestion from a peer the table does not
/// hold with signed evidence is refused.
///
/// A suggestion is never evidence. The table accepts a suggestion only from
/// a peer it already holds with **signed** evidence, so a peer that is
/// merely in the flat store cannot introduce new peers.
#[test]
fn a_suggestion_from_a_peer_the_table_does_not_hold_is_refused() {
    let mesh = mesh();
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    // Beta is in the flat store but **not** in the routing table: it is in
    // the store only, so the table holds no signed evidence for it.
    let dispatch = mesh.spine.begin_exploration(
        mesh.beta,
        search_key,
        routing_key_for(&search_key),
        Some(reply_path()),
    );
    let _ = sent(&dispatch);

    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: vec![*mesh.delta.as_hash()],
        from: *mesh.beta.as_hash(),
    };
    let outcome = mesh
        .spine
        .ingest_search_reply(&reply)
        .expect("the exploration reply is ingested");

    assert_eq!(outcome.suggested, 1, "the reply named one peer");
    assert_eq!(
        outcome.indexed, 0,
        "a suggestion from a peer the table cannot vouch for must be refused"
    );
    assert_eq!(outcome.refused, 1, "the refusal must be counted");
    assert!(
        mesh.spine.pending_resolutions().is_empty(),
        "a refused suggestion must not be queued for resolution"
    );
}
