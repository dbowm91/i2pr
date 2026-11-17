//! Plan 489 private controlled-mesh lane.
//!
//! The mesh holds six peers:
//!
//! - **local** — the router's own identity; it anchors the routing
//!   table in today's keyspace;
//! - **alpha** — a floodfill that is in the routing table and in the
//!   flat store. It answers both the exploration and the lookup;
//! - **zeta** — a floodfill that is in the flat store **only**. It is
//!   the discriminator: a lookup that drew its candidates from the flat
//!   store would be free to ask zeta, and a lookup that drew them from
//!   the routing table must not;
//! - **delta** — a peer the exploration reply names. It enters the
//!   table on alpha's evidence, with suggested rather than signed
//!   evidence;
//! - **epsilon** — a peer that does not advertise `f`. The table must
//!   refuse it and count the refusal;
//! - **theta** — the lookup's target. Its `RouterInfo` is what alpha
//!   returns in the `DatabaseStore`.
//!
//! The lane asserts that:
//!
//! - the exploration request is an exploratory `DatabaseLookup` — wire
//!   lookup type `3` — and the peers its answer names enter the table;
//! - the optimized lookup asks a peer that is **in the routing table**,
//!   and that peer is the table's own closest-first queryable
//!   candidate, and it is **not** the store-only peer;
//! - the lookup's answer reaches the caller as a completed result
//!   carrying the validated `RouterInfo`;
//! - no receipt and no evidence file carries a peer's hash.
//!
//! The lane is entirely in-process: it opens no socket, binds no port
//! and spawns no process. The evidence file it writes holds **counts
//! and digests only**; it never carries a peer's hash, its address, its
//! key material or a payload.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{
    RoutingDispatch, RoutingLookupOutcome, RoutingTableConfig, RoutingTableDriver,
};
use i2pr_netdb::{
    ReplyPath, ResponseOutcome, RouterHash, RouterInfoStore, RouterInfoStoreConfig,
    ValidatedRouterInfo, ValidationContext, daily_routing_key, router_hash,
};
use i2pr_proto::{
    DatabaseSearchReplyMessage, DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload,
    Mapping,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the mesh peers.
const LOCAL_SEED: u64 = 0x4890_0001;
const ALPHA_SEED: u64 = 0x4890_0002;
const ZETA_SEED: u64 = 0x4890_0003;
const DELTA_SEED: u64 = 0x4890_0004;
const EPSILON_SEED: u64 = 0x4890_0005;
const THETA_SEED: u64 = 0x4890_0006;

/// The clock the mesh runs on, in milliseconds.
const T0: u64 = 1_000;

/// The maximum encoded size of a `RouterInfo` in the lane.
const MAX_ROUTER_INFO_BYTES: usize = 16 * 1024;

fn evidence_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/integration/network-participation/evidence/plan489/result.json")
}

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

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

fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
    router_hash(signer.identity()).expect("hash")
}

fn routing_key_for(hash: &RouterHash) -> RouterHash {
    let key = daily_routing_key(hash, Date::from_millis(T0)).expect("routing key");
    RouterHash::from_hash(key)
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("compress");
    encoder.finish().expect("finish")
}

/// A digest rendered as lowercase hex.
fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn the_mesh_uses_the_routing_table_for_the_lookup_and_the_exploration_grows_it() {
    let local = bundle(LOCAL_SEED);
    let alpha = bundle(ALPHA_SEED);
    let zeta = bundle(ZETA_SEED);
    let delta = bundle(DELTA_SEED);
    let epsilon = bundle(EPSILON_SEED);
    let theta = bundle(THETA_SEED);

    let alpha_record = validated(&alpha, "Nf");
    let zeta_record = validated(&zeta, "Nf");
    let epsilon_record = validated(&epsilon, "N");
    let theta_record = validated(&theta, "N");

    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    let _ = store.insert(alpha_record.clone());
    let _ = store.insert(zeta_record.clone());
    let _ = store.insert(theta_record.clone());

    let mut driver = RoutingTableDriver::new(
        key_of(&local),
        Date::from_millis(T0),
        RoutingTableConfig::default(),
    );
    driver.advance_time(T0);
    driver.set_reply_path(Some(
        ReplyPath::new(RouterHash::from_bytes([0x49; 32]), 7).expect("reply path"),
    ));

    // A peer that does not advertise `f` never enters the table.
    let refused = driver.index(&epsilon_record);
    assert!(
        refused.is_rejected(),
        "a non-floodfill must be refused: {refused:?}"
    );
    assert_eq!(driver.table().len(), 0);

    // Alpha is a floodfill and enters the table with signed evidence.
    driver.index(&alpha_record);
    assert_eq!(driver.table().len(), 1);
    let (signed, suggested) = driver.table().evidence_counts();
    assert_eq!(signed, 1);
    assert_eq!(suggested, 0);

    // Zeta is in the flat store but not in the routing table. It is the
    // discriminator for the whole lane.
    assert!(!driver.table().contains(&zeta_record.key()));

    // ---- The exploration ------------------------------------------------
    let search_key = RouterHash::from_bytes([0x5e; 32]);
    let search_routing_key = routing_key_for(&search_key);
    let dispatch = driver.begin_exploration(&store, search_key, search_routing_key);
    let RoutingDispatch::Send {
        lookup_id,
        peer,
        message,
    } = dispatch
    else {
        panic!("the exploration must be dispatchable: {dispatch:?}");
    };
    assert_eq!(peer, alpha_record.key());
    assert_eq!(
        message.lookup_type, 3,
        "an exploratory lookup uses wire lookup type 3"
    );
    assert_eq!(message.key, *search_key.as_hash());

    // Alpha answers the exploration and names delta.
    let reply = DatabaseSearchReplyMessage {
        key: *search_key.as_hash(),
        peer_hashes: vec![*key_of(&delta).as_hash()],
        from: *alpha_record.key().as_hash(),
    };
    let discovery = driver
        .ingest_search_reply(lookup_id, &reply)
        .expect("the exploration reply is ingested");
    assert_eq!(discovery.suggested, 1);
    assert_eq!(discovery.indexed, 1);
    assert_eq!(discovery.refused, 0);
    assert!(!discovery.unknown_peer);
    assert_eq!(driver.table().len(), 2);
    let (signed, suggested) = driver.table().evidence_counts();
    assert_eq!(signed, 1, "alpha keeps its signed evidence");
    assert_eq!(suggested, 1, "delta enters on alpha's evidence");

    // ---- The optimized lookup -------------------------------------------
    let target = theta_record.key();
    let target_routing_key = routing_key_for(&target);
    let plan = driver.plan(&store, target_routing_key, &[]);
    assert_eq!(
        plan.queryable_hashes(),
        vec![alpha_record.key()],
        "only the table member whose RouterInfo is resolved is queryable"
    );
    assert_eq!(
        plan.pending_hashes(),
        vec![key_of(&delta)],
        "the discovered peer has no resolved RouterInfo yet"
    );

    let outcome = driver.begin_lookup(&store, target, target_routing_key);
    let RoutingLookupOutcome::Send {
        lookup_id,
        peer: asked,
        message: lookup_message,
    } = outcome
    else {
        panic!("the lookup must be dispatchable: {outcome:?}");
    };
    assert_eq!(
        lookup_message.lookup_type, 2,
        "a RouterInfo lookup is type 2"
    );
    assert_eq!(
        asked,
        alpha_record.key(),
        "the candidate must come from the routing table"
    );
    assert_ne!(
        asked,
        zeta_record.key(),
        "the store-only peer must never be asked"
    );
    assert!(
        driver.table().contains(&asked),
        "the peer asked must be a member of the routing table"
    );

    // Alpha answers the lookup with theta's record.
    let encoded = theta_record.encoded(MAX_ROUTER_INFO_BYTES).expect("encode");
    let payload = DeferredPayload::new(gzip(&encoded), MAX_ROUTER_INFO_BYTES).expect("payload");
    let store_message = DatabaseStoreMessage {
        key: *theta_record.key().as_hash(),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(payload),
    };
    let response = driver
        .ingest_store(
            &mut store,
            lookup_id,
            &store_message,
            ValidationContext::new(Date::from_millis(T0)),
        )
        .expect("the answer is ingested");
    let ResponseOutcome::Completed(result) = response else {
        panic!("the answer must reach the caller: {response:?}");
    };
    let i2pr_netdb::LookupResult::Success { router_info, .. } = *result else {
        panic!("the result must carry the validated record");
    };
    assert_eq!(
        router_info.key(),
        theta_record.key(),
        "the caller receives the requested record"
    );

    // ---- The receipts and the evidence ----------------------------------
    let receipts = driver.receipts();
    let rendered = format!("{receipts:?}");
    for peer in [
        alpha_record.key(),
        zeta_record.key(),
        key_of(&delta),
        epsilon_record.key(),
        theta_record.key(),
    ] {
        assert!(
            !rendered.contains(&hex(peer.as_bytes())),
            "a receipt must not carry a peer hash"
        );
    }

    let explored = receipts
        .iter()
        .find(|receipt| receipt.kind.kind_label() == "explored")
        .expect("an explored receipt");
    let discovered = receipts
        .iter()
        .find(|receipt| receipt.kind.kind_label() == "discovered")
        .expect("a discovered receipt");
    let looked_up = receipts
        .iter()
        .find(|receipt| receipt.kind.kind_label() == "looked_up")
        .expect("a looked-up receipt");
    assert_eq!(explored.table_size, 1);
    assert_eq!(discovered.table_size, 2);
    assert_eq!(discovered.suggested_count, 1);
    assert_eq!(looked_up.queryable_count, 1);
    assert_eq!(looked_up.pending_count, 1);

    let evidence = serde_json::json!({
        "schema": "i2pr-netdb-routing-table-mesh-v1",
        "plan": 489,
        "mesh": {
            "peer_count": 6,
            "table_members": driver.table().len(),
            "occupied_buckets": driver.table().occupied_buckets(),
        },
        "exploration": {
            "wire_lookup_type": message.lookup_type,
            "suggested": discovery.suggested,
            "indexed": discovery.indexed,
            "refused": discovery.refused,
        },
        "lookup": {
            "wire_lookup_type": lookup_message.lookup_type,
            "queryable": looked_up.queryable_count,
            "pending": looked_up.pending_count,
            "answered": 1,
        },
        "receipt_digests": {
            "explored": hex(&explored.digest),
            "discovered": hex(&discovered.digest),
            "looked_up": hex(&looked_up.digest),
        },
    });
    let rendered = serde_json::to_string_pretty(&evidence).expect("render evidence");
    for peer in [
        alpha_record.key(),
        zeta_record.key(),
        key_of(&delta),
        epsilon_record.key(),
        theta_record.key(),
    ] {
        assert!(
            !rendered.contains(&hex(peer.as_bytes())),
            "the evidence file must not carry a peer hash"
        );
    }

    let path = evidence_path();
    std::fs::create_dir_all(path.parent().expect("evidence directory"))
        .expect("create evidence dir");
    std::fs::write(&path, format!("{rendered}\n")).expect("write evidence");
}
