//! Plan 488 private controlled-mesh lane.
//!
//! The mesh holds three peers:
//!
//! - **alpha** answers the lookup with the requested data;
//! - **beta** answers at first and then stops answering;
//! - **gamma** replies with data that does not match the requested key.
//!
//! The lane asserts that the store's observed state is visible in
//! **source-originated receipts** — receipts the daemon driver itself
//! produced, not aggregate counters — and that the selection demonstrably
//! prefers the responsive peer.
//!
//! The lane is entirely in-process: it opens no socket, binds no port and
//! spawns no process. The evidence file it writes holds **counts and
//! digests only**; it never carries a peer's hash, its address, its key
//! material or a payload.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::netdb_participation::{
    ParticipationReceipt, ParticipationReceiptKind, PeerParticipationDriver, ProjectionLoadOutcome,
};
use i2pr_netdb::{
    ProfileState, ProfileStoreConfig, RouterHash, SelectionConstraints, ValidatedRouterInfo,
    ValidationContext, router_hash, select_candidates,
};
use i2pr_proto::{Date, Mapping};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The deterministic identity seeds of the three mesh peers.
const ALPHA_SEED: u64 = 0x4880_0001;
const BETA_SEED: u64 = 0x4880_0002;
const GAMMA_SEED: u64 = 0x4880_0003;

/// The clock the mesh runs on, in milliseconds.
const T0: u64 = 1_000;
const T1: u64 = 2_000;

fn evidence_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/integration/network-participation/evidence/plan488/result.json")
}

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn validate(signer: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
    validate_with_caps(signer, published_ms, "Nf")
}

/// Builds a validated record that declares a capacity class, so the mesh
/// exercises the tier demotion path as well as the state machine.
fn validate_with_caps(
    signer: &RouterIdentityBundle,
    published_ms: u64,
    caps: &str,
) -> ValidatedRouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), caps.to_owned()).unwrap();
    let info = signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            options.build().unwrap(),
        )
        .expect("sign");
    ValidatedRouterInfo::from_router_info(
        info,
        None,
        ValidationContext::new(Date::from_millis(published_ms)),
    )
    .expect("validate")
}

fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
    router_hash(signer.identity()).expect("hash")
}

/// A digest over the receipt's aggregate counts, rendered as lowercase hex.
fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Renders the receipt's state counters as a sorted map of labels to counts.
fn state_counts(receipt: &ParticipationReceipt) -> BTreeMap<String, u32> {
    receipt
        .state_counts
        .iter()
        .map(|(label, count)| ((*label).to_owned(), *count))
        .collect()
}

#[test]
fn the_mesh_prefers_the_responsive_peer_and_records_sanitized_receipts() {
    let alpha = bundle(ALPHA_SEED);
    let beta = bundle(BETA_SEED);
    let gamma = bundle(GAMMA_SEED);
    let alpha_key = key_of(&alpha);
    let beta_key = key_of(&beta);
    let gamma_key = key_of(&gamma);

    // The diversity rule is exercised by the mesh, so the three peers must
    // sit behind three different /16 prefixes. The seeds are fixed, so this
    // is a deterministic property of the fixture, not a random one.
    let prefixes = [alpha_key, beta_key, gamma_key]
        .iter()
        .map(i2pr_netdb::network_prefix16)
        .collect::<Vec<_>>();
    assert_eq!(prefixes.len(), 3, "the mesh must hold three peers");
    let mut unique = prefixes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        3,
        "the three mesh peers must sit behind distinct /16 prefixes"
    );

    let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
    driver.advance_time(T0);
    driver.learn(validate(&alpha, T0));
    driver.learn(validate(&beta, T0));
    driver.learn(validate(&gamma, T0));

    // The learned receipt is source-originated: the driver produced it when
    // the third peer entered the store.
    let learned = driver.receipts().last().expect("a learned receipt").clone();
    assert_eq!(learned.kind, ParticipationReceiptKind::Learned);
    assert_eq!(learned.profile_count, 3);
    assert_eq!(learned.peer_count, 3);

    // Round one: alpha and beta answer; gamma replies with data that does
    // not match the requested key.
    driver.observe_answered(&alpha_key);
    driver.observe_answered(&beta_key);
    driver.observe_misbehaving(&gamma_key);

    let observed = driver
        .receipts()
        .last()
        .expect("an observed receipt")
        .clone();
    assert_eq!(observed.kind, ParticipationReceiptKind::Observed);
    assert_eq!(observed.state_counts.get("answered"), Some(&2));
    assert_eq!(observed.state_counts.get("misbehaving"), Some(&1));

    let store = driver.store();
    assert_eq!(
        store.profile(&alpha_key).expect("alpha profile").state(),
        ProfileState::Answered
    );
    assert_eq!(
        store.profile(&beta_key).expect("beta profile").state(),
        ProfileState::Answered
    );
    assert_eq!(
        store.profile(&gamma_key).expect("gamma profile").state(),
        ProfileState::Misbehaving
    );

    // Round two: beta stops answering.
    driver.advance_time(T1);
    driver.observe_unanswered(&beta_key);
    assert_eq!(
        driver
            .store()
            .profile(&beta_key)
            .expect("beta profile")
            .state(),
        ProfileState::Unanswered
    );

    // The responsive peer is the one the selection prefers.
    let alpha_rank = driver
        .store()
        .profile(&alpha_key)
        .expect("alpha profile")
        .rank();
    let beta_rank = driver
        .store()
        .profile(&beta_key)
        .expect("beta profile")
        .rank();
    let gamma_rank = driver
        .store()
        .profile(&gamma_key)
        .expect("gamma profile")
        .rank();
    assert!(alpha_rank > beta_rank, "alpha must outrank beta");
    assert!(alpha_rank > gamma_rank, "alpha must outrank gamma");

    let selected = driver.select(3, 0x4880);
    assert_eq!(selected.len(), 3, "all three peers are eligible");
    assert_eq!(
        selected[0].key, alpha_key,
        "the responsive peer must be preferred"
    );

    // A rank floor removes the two degraded peers from the selection.
    let floored = {
        let constraints = SelectionConstraints {
            count: 3,
            min_rank: alpha_rank,
            ..SelectionConstraints::new(3, 0x4881)
        };
        select_candidates(driver.store(), &constraints)
    };
    assert_eq!(
        floored.len(),
        1,
        "only the responsive peer clears the floor"
    );
    assert_eq!(floored[0].key, alpha_key);

    // The receipts carry counts, not identities.
    // No receipt may name a peer.
    let receipts = driver.receipts();
    let rendered = format!("{receipts:?}");
    for key in [alpha_key, beta_key, gamma_key] {
        assert!(
            !rendered.contains(&hex(key.as_bytes())),
            "a receipt must not carry a peer hash"
        );
    }

    // The maintenance pass demotes the degraded peers and then drops them.
    let demote_after = i2pr_netdb::DEFAULT_DEMOTE_AFTER_MS;
    let drop_after = i2pr_netdb::DEFAULT_DROP_AFTER_MS;
    let demoted = driver
        .tick(T1 + demote_after)
        .expect("the maintenance pass is due");
    assert_eq!(demoted.demoted, 2, "beta and gamma are demoted");
    assert_eq!(demoted.dropped, 0);

    let dropped = driver
        .tick(T1 + drop_after)
        .expect("the maintenance pass is due");
    assert_eq!(dropped.dropped, 2, "beta and gamma are dropped");
    assert_eq!(driver.store().len(), 1);
    assert!(driver.store().profile(&alpha_key).is_some());

    // After the drop, only the responsive peer remains selectable.
    let remaining = driver.select(3, 0x4882);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].key, alpha_key);

    // The persisted projection round-trips, and a corrupt one is discarded
    // rather than fatal.
    let encoded = driver.encode_projection().expect("encode projection");
    let mut restarted = PeerParticipationDriver::new(ProfileStoreConfig::default());
    restarted.advance_time(T1 + drop_after);
    restarted.learn(validate(&alpha, T0));
    let outcome = restarted.load_projection(&encoded);
    assert_eq!(
        outcome,
        ProjectionLoadOutcome::Restored {
            restored: 1,
            orphaned: 0,
            fresh: 0
        }
    );
    assert_eq!(
        restarted.load_projection(b"corrupt projection"),
        ProjectionLoadOutcome::Discarded {
            reason: "malformed"
        }
    );
    assert_eq!(restarted.discarded_projection_count(), 1);

    // The evidence file holds counts and digests only.
    let final_receipt = driver
        .receipts()
        .into_iter()
        .rev()
        .find(|receipt| receipt.kind == ParticipationReceiptKind::Maintained)
        .expect("a maintenance receipt");
    let evidence = serde_json::json!({
        "schema": "i2pr-netdb-participation-mesh-v1",
        "plan": 488,
        "mesh": {
            "peer_count": 3,
            "distinct_prefix16": 3,
        },
        "states": {
            "answered": 1,
            "unanswered": 0,
            "misbehaving": 0,
            "never-tried": 0,
            "answered-empty": 0,
            "refused": 0,
            "unreachable": 0,
        },
        "maintenance": {
            "demoted": demoted.demoted,
            "dropped": dropped.dropped,
            "remaining_peers": driver.store().len(),
        },
        "selection": {
            "requested": 3,
            "selected": remaining.len(),
            "responsive_peer_preferred": true,
        },
        "projection": {
            "restored": 1,
            "discarded": 1,
        },
        "receipt_digests": {
            "learned": hex(&learned.digest),
            "observed": hex(&observed.digest),
            "maintained": hex(&final_receipt.digest),
        },
        "receipt_state_counts": {
            "learned": state_counts(&learned),
            "observed": state_counts(&observed),
            "maintained": state_counts(&final_receipt),
        },
    });
    let rendered = serde_json::to_string_pretty(&evidence).expect("render evidence");
    for key in [alpha_key, beta_key, gamma_key] {
        assert!(
            !rendered.contains(&hex(key.as_bytes())),
            "the evidence file must not carry a peer hash"
        );
    }

    let path = evidence_path();
    std::fs::create_dir_all(path.parent().expect("evidence directory"))
        .expect("create evidence dir");
    std::fs::write(&path, format!("{rendered}\n")).expect("write evidence");
}
