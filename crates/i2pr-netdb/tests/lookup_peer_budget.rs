//! Plan 500: the lookup engine's per-lookup peer budget is load-bearing.
//!
//! `LookupPolicy::max_peers_per_lookup` was declared, validated at
//! construction and exposed by an accessor — and read by nothing. The
//! engine asked every candidate the selector produced, so the policy's
//! tighter peer ceiling was never applied.
//!
//! These rows are the black-box proof that the ceiling now binds:
//!
//! - a lookup whose candidate set is larger than the budget stops at
//!   the budget and reports the typed exhaustion, which is distinct
//!   from the candidate-list exhaustion;
//! - a lookup within the budget behaves exactly as it did before — the
//!   budget never fires spuriously and a successful lookup still
//!   succeeds;
//! - the policy still refuses a zero budget and a budget above the
//!   candidate ceiling;
//! - the budget binds on **every** entry point — `start`,
//!   `start_with_candidates` and `start_with_preferred` — and not on
//!   one of them only;
//! - Plan 489's frozen candidate list and Plan 497's preference prefix
//!   are unchanged under the budget: the candidate set is the same,
//!   only the number of queried peers differs.
//!
//! Every row drives the engine through its public surface only. No
//! private field is touched, no socket is opened and no peer identity
//! leaks into a diagnostic.

use std::io::Write as _;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    LookupAction, LookupFinalState, LookupId, LookupKind, LookupPolicy, LookupPolicyError,
    LookupResult, ReplyPath, ResponseOutcome, RouterHash, RouterInfoLookup, RouterInfoStore,
    StartOutcome, ValidatedRouterInfo, ValidationContext, handle_database_store, router_hash,
    select_floodfill_candidates,
};
use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The publication moment every record in this file carries.
const PUBLISHED_MS: u64 = 1;

/// The largest encoded `RouterInfo` the rows build.
const MAX_ROUTER_INFO_BYTES: usize = 16 * 1024;

/// The deterministic seed the rows draw their peers from.
const SEED_BASE: u64 = 0x500_0000;

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Signs a plain record — a peer that does not advertise `f`.
fn record(signer: &RouterIdentityBundle) -> ValidatedRouterInfo {
    let published = Date::from_millis(PUBLISHED_MS);
    let info = signer
        .sign_router_info(published, Vec::new(), Vec::new(), Mapping::empty())
        .expect("sign");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
        .expect("validate")
}

/// Signs a floodfill record — a peer that advertises `f`.
fn floodfill(signer: &RouterIdentityBundle) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    let published = Date::from_millis(PUBLISHED_MS);
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

/// The reply path every row hands to the lookup.
fn reply_path() -> ReplyPath {
    ReplyPath::new(RouterHash::from_bytes([0x77; 32]), 0xCAFE).expect("reply path")
}

/// Fills the store with `count` floodfills and returns their keys.
fn floodfill_store(count: u64) -> (RouterInfoStore, Vec<RouterHash>) {
    let mut store = RouterInfoStore::default();
    let mut keys = Vec::new();
    for index in 0..count {
        let signer = bundle(SEED_BASE + index);
        store.insert(floodfill(&signer));
        keys.push(key_of(&signer));
    }
    (store, keys)
}

/// Drives one attempt out of the lookup and returns the peer it asked.
fn asked_peer(outcome: StartOutcome) -> RouterHash {
    let StartOutcome::PendingAttempt(action) = outcome else {
        panic!("expected an attempt, got {outcome:?}");
    };
    let LookupAction::SendDatabaselookup { peer, .. } = action else {
        panic!("expected SendDatabaselookup");
    };
    peer
}

/// Returns the final state of a terminal outcome.
fn terminal_state(outcome: &StartOutcome) -> LookupFinalState {
    let StartOutcome::Terminal(result) = outcome else {
        panic!("expected a terminal outcome, got {outcome:?}");
    };
    let LookupResult::Failure { final_state, .. } = result.as_ref() else {
        panic!("expected a failure, got {result:?}");
    };
    *final_state
}

/// Returns the attempt count a terminal outcome carries.
fn terminal_attempts(outcome: &StartOutcome) -> usize {
    let StartOutcome::Terminal(result) = outcome else {
        panic!("expected a terminal outcome, got {outcome:?}");
    };
    let LookupResult::Failure { diagnostics, .. } = result.as_ref() else {
        panic!("expected a failure, got {result:?}");
    };
    diagnostics.attempts
}

#[test]
fn the_peer_budget_stops_a_lookup_whose_candidate_set_is_larger_than_the_budget() {
    // The discriminator: twelve candidates, four peers to ask.
    let budget = 4usize;
    let policy = LookupPolicy::new(12, budget, 128, 64, 5_000, 1_500).expect("policy");
    let (store, _keys) = floodfill_store(12);
    let target = RouterHash::from_bytes([0xC0; 32]);
    let routing_key = RouterHash::from_bytes([0x11; 32]);
    let lookup_id = LookupId::new(0x5001, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);

    // The candidate set is the selector's own and the budget does not
    // narrow it: the selector still produces every candidate.
    let selection = select_floodfill_candidates(&store, &target, &routing_key, &[], &policy);
    assert_eq!(
        selection.len(),
        12,
        "the budget must narrow the attempts, never the candidate set"
    );

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(
        matches!(outcome, StartOutcome::NeedsReplyPath(_)),
        "the lookup must ask for a reply path first"
    );
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));

    let mut asked = vec![asked_peer(
        lookup.handle_pending_after_path(&store, &routing_key),
    )];
    for _ in 1..budget {
        asked.push(asked_peer(lookup.advance_after_reply(&store, &routing_key)));
    }
    assert_eq!(
        asked,
        selection.hashes()[..budget].to_vec(),
        "the lookup asks the budget's worth of candidates, in the selector's order"
    );
    assert_eq!(
        lookup.diagnostics().expect("an active lookup").attempts,
        budget,
        "the attempt counter tracks the emitted attempts"
    );

    // The next advance finds the budget spent: the lookup terminates
    // with the typed exhaustion and emits no further attempt.
    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerBudgetExhausted,
        "the budget exhaustion is typed and distinct from the candidate exhaustion"
    );
    assert_eq!(
        terminal_attempts(&outcome),
        budget,
        "the lookup stops at the budget and never exceeds it"
    );
    assert!(
        lookup.active_lookup().is_none(),
        "a terminated lookup holds no active state"
    );
}

#[test]
fn a_lookup_within_the_budget_reports_the_candidate_exhaustion() {
    // The control: three candidates, a budget of eight. The budget
    // must never fire, and the candidate list running out keeps its
    // own final state.
    let policy = LookupPolicy::new(12, 8, 128, 64, 5_000, 1_500).expect("policy");
    let (store, _keys) = floodfill_store(3);
    let target = RouterHash::from_bytes([0xC1; 32]);
    let routing_key = RouterHash::from_bytes([0x12; 32]);
    let lookup_id = LookupId::new(0x5002, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    asked_peer(lookup.handle_pending_after_path(&store, &routing_key));
    asked_peer(lookup.advance_after_reply(&store, &routing_key));
    asked_peer(lookup.advance_after_reply(&store, &routing_key));

    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerExhausted,
        "the candidate list ran out — the budget must not have fired"
    );
    assert_eq!(
        terminal_attempts(&outcome),
        3,
        "every candidate was asked exactly once"
    );
}

#[test]
fn a_successful_lookup_within_the_budget_still_succeeds() {
    // The success control: the budget does not touch the success path.
    let policy = LookupPolicy::default();
    let mut store = RouterInfoStore::default();
    let target_signer = bundle(SEED_BASE + 0x40);
    let target_record = record(&target_signer);
    let target = target_record.key();
    let peer = bundle(SEED_BASE + 0x41);
    store.insert(floodfill(&peer));

    let routing_key = RouterHash::from_bytes([0x13; 32]);
    let lookup_id = LookupId::new(0x5003, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);

    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    asked_peer(lookup.handle_pending_after_path(&store, &routing_key));

    // The peer answers with the record the lookup asked for.
    let encoded = target_record
        .encoded(MAX_ROUTER_INFO_BYTES)
        .expect("encode");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&encoded).expect("compress");
    let compressed = encoder.finish().expect("finish");
    let compressed_len = compressed.len();
    let store_message = DatabaseStoreMessage {
        key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(
            DeferredPayload::new(compressed, compressed_len).expect("payload"),
        ),
    };
    let outcome = handle_database_store(
        &mut lookup,
        &mut store,
        lookup_id,
        &store_message,
        ValidationContext::new(Date::from_millis(PUBLISHED_MS)),
    )
    .expect("handle");
    let ResponseOutcome::Completed(result) = outcome else {
        panic!("the answer must reach the caller, got {outcome:?}");
    };
    let LookupResult::Success {
        lookup_id: completed,
        ..
    } = *result
    else {
        panic!("the lookup must succeed, got {result:?}");
    };
    assert_eq!(completed, lookup_id);
    assert!(
        lookup.active_lookup().is_none(),
        "a completed lookup holds no active state"
    );
}

#[test]
fn the_policy_still_refuses_a_zero_budget_and_a_budget_above_the_candidate_ceiling() {
    // The refusal row: the policy's own validations stay load-bearing.
    let error = LookupPolicy::new(2, 0, 1, 1, 1000, 1000).expect_err("a zero budget is refused");
    assert_eq!(error, LookupPolicyError::ZeroPeers);
    let error = LookupPolicy::new(2, 3, 1, 1, 1000, 1000)
        .expect_err("a budget above the candidate ceiling is refused");
    assert_eq!(error, LookupPolicyError::PeerBudgetExceedsCandidates);
}

#[test]
fn the_budget_binds_on_every_entry_point() {
    let budget = 2usize;
    let policy = LookupPolicy::new(8, budget, 128, 64, 5_000, 1_500).expect("policy");
    let (store, keys) = floodfill_store(6);
    let target = RouterHash::from_bytes([0xC2; 32]);
    let routing_key = RouterHash::from_bytes([0x14; 32]);

    // ---- `start` ------------------------------------------------------
    let lookup_id = LookupId::new(0x5004, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);
    let outcome = lookup.start(&store, lookup_id, &routing_key);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    asked_peer(lookup.handle_pending_after_path(&store, &routing_key));
    asked_peer(lookup.advance_after_reply(&store, &routing_key));
    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerBudgetExhausted
    );
    assert_eq!(terminal_attempts(&outcome), budget);

    // ---- `start_with_candidates` --------------------------------------
    let lookup_id = LookupId::new(0x5005, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);
    let outcome = lookup.start_with_candidates(lookup_id, &keys);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    asked_peer(lookup.handle_pending_after_path(&store, &routing_key));
    asked_peer(lookup.advance_after_reply(&store, &routing_key));
    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerBudgetExhausted
    );
    assert_eq!(terminal_attempts(&outcome), budget);

    // ---- `start_with_preferred` ---------------------------------------
    let lookup_id = LookupId::new(0x5006, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);
    let outcome = lookup.start_with_preferred(&store, lookup_id, &routing_key, &keys[..1]);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    asked_peer(lookup.handle_pending_after_path(&store, &routing_key));
    asked_peer(lookup.advance_after_reply(&store, &routing_key));
    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerBudgetExhausted
    );
    assert_eq!(terminal_attempts(&outcome), budget);
}

#[test]
fn the_pinned_candidate_list_is_unchanged_under_the_budget() {
    // Plan 489's invariant: the caller's list is the candidate list,
    // and the budget applies to how many of them are *queried*, not to
    // which ones exist.
    let budget = 3usize;
    let policy = LookupPolicy::new(12, budget, 128, 64, 5_000, 1_500).expect("policy");
    let (store, keys) = floodfill_store(12);
    let target = RouterHash::from_bytes([0xC3; 32]);
    let routing_key = RouterHash::from_bytes([0x15; 32]);
    let lookup_id = LookupId::new(0x5007, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);

    let outcome = lookup.start_with_candidates(lookup_id, &keys);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    let mut asked = vec![asked_peer(
        lookup.handle_pending_after_path(&store, &routing_key),
    )];
    for _ in 1..budget {
        asked.push(asked_peer(lookup.advance_after_reply(&store, &routing_key)));
    }

    // The list is untouched: the lookup still knows every peer the
    // caller handed it, and the peers it did not ask are still there.
    let active = lookup.active_for_test().expect("an active lookup");
    assert_eq!(
        active.pinned_candidates(),
        Some(keys.as_slice()),
        "the budget must not narrow the pinned candidate list"
    );
    assert_eq!(
        lookup.diagnostics().expect("an active lookup").attempts,
        budget,
        "only the budget's worth of peers were queried"
    );
    assert_eq!(
        asked,
        keys[..budget].to_vec(),
        "the caller's order is honoured, prefix first"
    );

    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerBudgetExhausted
    );
    assert_eq!(terminal_attempts(&outcome), budget);
}

#[test]
fn the_preferred_path_still_reaches_a_peer_ingested_mid_lookup_under_the_budget() {
    // Plan 497's invariant: the preference is a priority prefix, not a
    // closed set, so the store is re-selected on every advance and a
    // peer ingested while the lookup runs is still eligible.
    let budget = 2usize;
    let policy = LookupPolicy::new(8, budget, 128, 64, 5_000, 1_500).expect("policy");
    let mut store = RouterInfoStore::default();
    let first = bundle(SEED_BASE + 0x50);
    store.insert(floodfill(&first));
    let first_key = key_of(&first);
    let target = RouterHash::from_bytes([0xC4; 32]);
    let routing_key = RouterHash::from_bytes([0x16; 32]);
    let lookup_id = LookupId::new(0x5008, LookupKind::RouterInfo, target);
    let mut lookup = RouterInfoLookup::new(policy);

    let outcome = lookup.start_with_preferred(&store, lookup_id, &routing_key, &[first_key]);
    assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
    assert!(lookup.accept_reply_path(lookup_id, reply_path()));
    let mut asked = vec![asked_peer(
        lookup.handle_pending_after_path(&store, &routing_key),
    )];
    assert_eq!(asked[0], first_key, "the preferred peer leads");

    // A second floodfill arrives in the store while the lookup runs.
    let second = bundle(SEED_BASE + 0x51);
    store.insert(floodfill(&second));
    let second_key = key_of(&second);

    asked.push(asked_peer(lookup.advance_after_reply(&store, &routing_key)));
    assert_eq!(
        asked[1], second_key,
        "the store is re-selected on every advance, so the peer ingested mid-lookup is still eligible"
    );

    // A third floodfill arrives too. The candidate set is now larger
    // than the budget, so the budget — and not the candidate list — is
    // what stops the lookup.
    let third = bundle(SEED_BASE + 0x52);
    store.insert(floodfill(&third));

    let outcome = lookup.advance_after_reply(&store, &routing_key);
    assert_eq!(
        terminal_state(&outcome),
        LookupFinalState::PeerBudgetExhausted
    );
    assert_eq!(terminal_attempts(&outcome), budget);
    assert_eq!(
        asked.len(),
        budget,
        "the lookup asked exactly the budget's worth of peers"
    );
}
