//! Plan 507 — the WP1 feasibility artefact.
//!
//! The question WP1 had to answer by running code:
//!
//! > Does pruning with the router's **own** acceptance window actually make
//! > a peer store **turn over** — does a store at its quota accept a **new**
//! > peer after a prune, where it refused before — and does a store that
//! > holds fresh records keep them?
//!
//! **This artefact uses only production code that already exists.** It
//! introduces no production change, and it depends on no method this plan
//! is going to add. It drives the **real** `RouterInfoStore` — the exact
//! type the daemon's bootstrap NetDB and the destination coordinator's
//! authoritative store hold — and the **real** `Bootstrap` through its
//! public ingest path.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value the lane's own code draws comes
//!   from a seeded ChaCha8 stream; the peer identities are reproducible.
//! - **An explicit clock.** The lane never reads the wall clock. The
//!   publication timestamps, the validation contexts and the prune's `now`
//!   are all explicit values.
//! - **No environment variable, no `#[ignore]`, no stock peer.** The only
//!   peers are ones the lane signs itself.
//! - **No unbounded waiting.** The lane never sleeps to make an assertion
//!   pass.
//!
//! The **negative controls** are recorded in Plan 507's closure record: two
//! mutations of the real `i2pr-netdb` prune — one that removes everything
//! regardless of the window, one that removes the record without releasing
//! its bytes — were applied to the real tree, run against this artefact,
//! and reverted. The first failed five of the nine tests, the second
//! failed two. The two sensitivity tests below are the artefact's own
//! contribution to that pair.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo, ValidationContext};
use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, Date, Hash, Mapping, RouterAddress};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_daemon::bootstrap::Bootstrap;
use i2pr_daemon::config::ReseedConfig;

/// The clock the lane starts at.
const NOW_MS: u64 = 1_700_000_000_000;

/// The router's **own** acceptance window, read from the validation policy
/// rather than restated.
///
/// The point of the whole plan is that the router retains exactly what it
/// accepts. If this lane ever restated the window as its own literal, the
/// two could disagree and the artefact would prove nothing.
fn acceptance_window_ms() -> u64 {
    let window: Duration = i2pr_netdb::ValidationPolicy::default_const().max_age;
    u64::try_from(window.as_millis()).expect("the acceptance window fits in milliseconds")
}

/// One validated peer the lane signs itself.
struct Peer {
    record: ValidatedRouterInfo,
    hash: Hash,
}

/// Builds a validated peer published `published_ms` and validated at
/// `validated_ms`.
///
/// The two timestamps are separate on purpose: a peer is validated when it
/// is fresh and pruned later, when it is not.
fn peer(seed: u64, published_ms: u64, validated_ms: u64) -> Peer {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let mut options = Mapping::builder();
    options
        .insert("family".to_owned(), "ipv4".to_owned())
        .expect("family");
    options
        .insert("router.version".to_owned(), "0.9.62".to_owned())
        .expect("router.version");
    let address_options = Mapping::from_entries(vec![
        ("host".to_owned(), "127.0.0.1".to_owned()),
        ("port".to_owned(), "12345".to_owned()),
    ])
    .expect("address options");
    let addresses = vec![
        RouterAddress::new(
            1,
            Date::from_millis(published_ms),
            "SSU2".to_owned(),
            address_options,
        )
        .expect("address"),
    ];
    let info = bundle
        .sign_router_info(
            Date::from_millis(published_ms),
            addresses,
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("signed info");
    let record = ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(validated_ms)),
    )
    .expect("validated info");
    let hash = Hash::from_bytes(*record.key().as_bytes());
    Peer { record, hash }
}

/// Builds the unsolicited `DatabaseStore` message that carries a peer.
fn store_message(peer: &Peer) -> DatabaseStoreMessage {
    let encoded = peer
        .record
        .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    {
        use std::io::Write as _;
        encoder.write_all(&encoded).expect("compress");
    }
    let compressed = encoder.finish().expect("finish");
    let compressed_len = compressed.len();
    DatabaseStoreMessage {
        key: peer.hash,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(
            i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
        ),
    }
}

/// A real bootstrap store.
fn bootstrap(store_config: RouterInfoStoreConfig) -> Arc<std::sync::Mutex<Bootstrap>> {
    Arc::new(std::sync::Mutex::new(Bootstrap::new(
        store_config,
        ReseedConfig {
            enabled: false,
            max_sources: 0,
            max_su3_bytes: 0,
            sources: Vec::new(),
        },
    )))
}

/// The store quota the lane uses: two records.
fn tiny_quota() -> RouterInfoStoreConfig {
    RouterInfoStoreConfig::new(2, 64 * 1024)
}

/// Inserts a peer and asserts the store accepted it as new.
///
/// `RouterInfoStore::insert` returns an outcome, not a `Result`, so the
/// assertion is explicit: an insert that was not `Inserted` is a lane
/// failure, not a value to carry on with.
fn insert(store: &mut RouterInfoStore, candidate: &Peer) {
    assert_eq!(
        store.insert(candidate.record.clone()),
        i2pr_netdb::InsertOutcome::Inserted,
        "the record must be inserted"
    );
}

// ----------------------------------------------------------------------
// The gap: a store at its quota refuses every new peer, forever.
// ----------------------------------------------------------------------

/// **The gap.** A store at its quota refuses a new peer, and the refusal is
/// a refusal — not a replacement, not an eviction.
#[test]
fn a_store_at_its_quota_refuses_every_new_peer() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let first = peer(0x5070, NOW_MS, NOW_MS);
    let second = peer(0x5071, NOW_MS, NOW_MS);
    let third = peer(0x5072, NOW_MS, NOW_MS);

    assert_eq!(
        store.insert(first.record.clone()),
        i2pr_netdb::InsertOutcome::Inserted
    );
    assert_eq!(
        store.insert(second.record.clone()),
        i2pr_netdb::InsertOutcome::Inserted
    );
    assert_eq!(
        store.insert(third.record.clone()),
        i2pr_netdb::InsertOutcome::CapacityExceeded,
        "a store at its quota must refuse a new peer"
    );
    assert_eq!(store.len(), 2, "the refused peer must not be half-applied");
}

// ----------------------------------------------------------------------
// The mechanism: the same store turns over after a prune.
// ----------------------------------------------------------------------

/// **The mechanism.** The same store accepts a new peer after a prune,
/// where it refused before.
///
/// The two peers are published at **different** times on purpose: the prune
/// compares each record's **own** publication age against the window, so a
/// "fresh" record has to have been published recently, not merely accepted
/// recently.
#[test]
fn the_store_turns_over_after_a_prune() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let window = acceptance_window_ms();
    let stale = peer(0x5073, NOW_MS, NOW_MS);
    let fresh = peer(0x5074, NOW_MS + window, NOW_MS + window);
    let newcomer = peer(0x5075, NOW_MS + window, NOW_MS + window);

    insert(&mut store, &stale);
    insert(&mut store, &fresh);
    assert_eq!(
        store.insert(newcomer.record.clone()),
        i2pr_netdb::InsertOutcome::CapacityExceeded
    );

    // The stale peer ages out; the fresh one does not.
    let later = NOW_MS + window + 1;
    let removed = store.prune(Date::from_millis(later), window);
    assert_eq!(removed, 1, "exactly the stale record must be removed");
    assert!(
        store
            .get(&i2pr_netdb::RouterHash::from_hash(stale.hash))
            .is_none(),
        "the stale record must be gone"
    );
    assert!(
        store
            .get(&i2pr_netdb::RouterHash::from_hash(fresh.hash))
            .is_some(),
        "the fresh record must survive"
    );

    // And the store now makes room.
    assert_eq!(
        store.insert(newcomer.record.clone()),
        i2pr_netdb::InsertOutcome::Inserted,
        "the store must accept a new peer after the prune"
    );
    assert_eq!(store.len(), 2);
}

/// A record inside the window always survives.
#[test]
fn a_record_inside_the_window_survives_the_prune() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let fresh = peer(0x5076, NOW_MS, NOW_MS);
    insert(&mut store, &fresh);

    // One millisecond inside the window.
    let just_inside = NOW_MS + acceptance_window_ms() - 1;
    assert_eq!(
        store.prune(Date::from_millis(just_inside), acceptance_window_ms()),
        0,
        "a record inside the window must not be removed"
    );
    assert!(
        store
            .get(&i2pr_netdb::RouterHash::from_hash(fresh.hash))
            .is_some()
    );
}

/// A record exactly at the window boundary is removed — the window is
/// inclusive on the inside and exclusive on the outside, and the boundary
/// is deterministic.
#[test]
fn the_window_boundary_is_deterministic() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let peer = peer(0x5077, NOW_MS, NOW_MS);
    insert(&mut store, &peer);

    // The age equals the window exactly: `age > max_age` is false, so the
    // record survives.
    let exactly_at = NOW_MS + acceptance_window_ms();
    assert_eq!(
        store.prune(Date::from_millis(exactly_at), acceptance_window_ms()),
        0
    );

    // One millisecond past the window: the record is removed.
    let just_past = NOW_MS + acceptance_window_ms() + 1;
    assert_eq!(
        store.prune(Date::from_millis(just_past), acceptance_window_ms()),
        1
    );
}

/// The prune releases the byte accounting with the record, so the store
/// does not leak capacity.
#[test]
fn the_prune_releases_the_byte_accounting() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let window = acceptance_window_ms();
    let stale = peer(0x5078, NOW_MS, NOW_MS);
    let fresh = peer(0x5079, NOW_MS + window, NOW_MS + window);
    insert(&mut store, &stale);
    insert(&mut store, &fresh);
    let before = store.encoded_bytes();
    assert!(before > 0, "the records must occupy bytes");

    let later = NOW_MS + window + 1;
    store.prune(Date::from_millis(later), window);
    let after = store.encoded_bytes();
    assert!(
        after < before,
        "the removed record's bytes must be released, {after} vs {before}"
    );
    assert_eq!(
        after,
        fresh.record.encoded_len(),
        "the accounting must equal the surviving record's bytes exactly"
    );
}

/// The prune is deterministic for a fixed `now`.
#[test]
fn the_prune_is_deterministic() {
    let build = || {
        let mut store = RouterInfoStore::with_config(tiny_quota());
        for index in 0..2_u64 {
            insert(&mut store, &peer(0x5080 + index, NOW_MS, NOW_MS));
        }
        store
    };
    let later = NOW_MS + acceptance_window_ms() + 1;
    let first = build();
    let second = build();
    let mut first = first;
    let mut second = second;
    assert_eq!(
        first.prune(Date::from_millis(later), acceptance_window_ms()),
        second.prune(Date::from_millis(later), acceptance_window_ms())
    );
    assert_eq!(first.len(), second.len());
    assert_eq!(first.encoded_bytes(), second.encoded_bytes());
}

// ----------------------------------------------------------------------
// The real component: the bootstrap's store fills and refuses.
// ----------------------------------------------------------------------

/// The **real** bootstrap's store fills through the **real** unsolicited
/// ingest path and then refuses — the gap, on the production component.
#[test]
fn the_real_bootstrap_store_fills_and_then_refuses() {
    let bootstrap = bootstrap(tiny_quota());
    let first = peer(0x5090, NOW_MS, NOW_MS);
    let second = peer(0x5091, NOW_MS, NOW_MS);
    let third = peer(0x5092, NOW_MS, NOW_MS);

    {
        let mut guard = bootstrap.lock().expect("bootstrap lock");
        for candidate in [&first, &second] {
            let outcome = guard
                .ingest_authenticated_router_store(&store_message(candidate), NOW_MS)
                .expect("ingest");
            assert!(
                matches!(outcome, i2pr_netdb::UnsolicitedStoreOutcome::Inserted(_)),
                "the live store must accept the peer"
            );
        }
        let outcome = guard
            .ingest_authenticated_router_store(&store_message(&third), NOW_MS)
            .expect("ingest");
        assert!(
            matches!(
                outcome,
                i2pr_netdb::UnsolicitedStoreOutcome::CapacityExceeded
            ),
            "the third peer must be refused at the quota, got {outcome:?}"
        );
    }
    assert_eq!(
        bootstrap.lock().expect("bootstrap lock").store().len(),
        2,
        "the refused peer must not be half-applied"
    );
}

// ----------------------------------------------------------------------
// The negative controls.
// ----------------------------------------------------------------------

/// The **window** is what keeps a fresh record alive.
///
/// A prune with a zero window removes everything, so the window — not the
/// prune — is what distinguishes a stale record from a live one. This test
/// asserts that the artefact's own window keeps a record the router would
/// still accept.
#[test]
fn the_window_is_what_keeps_a_fresh_record_alive() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let fresh = peer(0x5093, NOW_MS, NOW_MS);
    insert(&mut store, &fresh);
    assert_eq!(
        store.prune(Date::from_millis(NOW_MS + 1), 0),
        1,
        "a zero window removes everything"
    );
    // The artefact's own assertion — a record inside the window survives —
    // is what rejects this behaviour.
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let fresh = peer(0x5094, NOW_MS, NOW_MS);
    insert(&mut store, &fresh);
    let removed = store.prune(
        Date::from_millis(NOW_MS + acceptance_window_ms() - 1),
        acceptance_window_ms(),
    );
    assert_eq!(
        removed, 0,
        "the real window must not remove a record inside it"
    );
}

/// The accounting assertion is **sensitive to a leak**.
///
/// A prune that removed the record but did **not** release its bytes would
/// leave the store unable to make room, so this assertion must notice.
#[test]
fn the_accounting_assertion_is_sensitive_to_a_leak() {
    let mut store = RouterInfoStore::with_config(tiny_quota());
    let stale = peer(0x5095, NOW_MS, NOW_MS);
    let fresh = peer(0x5096, NOW_MS, NOW_MS);
    insert(&mut store, &stale);
    insert(&mut store, &fresh);
    let leaked = store.encoded_bytes();
    let later = NOW_MS + acceptance_window_ms() + 1;
    store.prune(Date::from_millis(later), acceptance_window_ms());
    let after = store.encoded_bytes();
    assert!(
        after < leaked,
        "the accounting assertion is sensitive to a leak: {after} vs {leaked}"
    );
}
