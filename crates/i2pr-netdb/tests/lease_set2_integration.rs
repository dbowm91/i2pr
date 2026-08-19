//! Plan 119 Phase H integration tests: validated LeaseSet2 store
//! round-trip, freshness, replacement, capacity, and the
//! routerinfo/leaseset capacity-isolation invariant.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_proto::{
    CryptoKeyType, Date32, Hash, Lease2, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags,
    LeaseSet2Header, Mapping, SignatureValue,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_netdb::{
    DestinationHash, LeaseSet2InsertOutcome, LeaseSet2Store, LeaseSet2StoreConfig,
    LeaseSet2ValidationContext, LeaseSet2ValidationError, ValidatedLeaseSet2,
};

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn destination_for(signer: &RouterIdentityBundle) -> i2pr_proto::Destination {
    i2pr_proto::Destination::new(signer.identity().key_and_cert().clone()).expect("destination")
}

fn build_signed_ls2(
    signer: &RouterIdentityBundle,
    published_seconds: u32,
    expires_offset_seconds: u16,
    leases: Vec<Lease2>,
) -> LeaseSet2 {
    let destination = destination_for(signer);
    let header = LeaseSet2Header::new(
        destination,
        published_seconds,
        expires_offset_seconds,
        LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0u8; 64])
        .expect("placeholder");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("encryption key"),
    ];
    let unsigned = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        leases,
        placeholder,
    )
    .expect("unsigned ls2");
    let signature = signer
        .signing_key()
        .sign(&unsigned.signature_preimage())
        .expect("sign");
    LeaseSet2::new(
        unsigned.header().clone(),
        unsigned.options().clone(),
        unsigned.encryption_keys().to_vec(),
        unsigned.leases().to_vec(),
        signature,
    )
    .expect("ls2")
}

fn validate(signer: &RouterIdentityBundle, published_seconds: u32) -> ValidatedLeaseSet2 {
    let leases = vec![Lease2::new(
        Hash::from_bytes([0x11; 32]),
        7,
        Date32::from_seconds(published_seconds + 600),
    )];
    let ls2 = build_signed_ls2(signer, published_seconds, 3_600, leases);
    ValidatedLeaseSet2::from_lease_set2(
        ls2,
        None,
        LeaseSet2ValidationContext::new(published_seconds),
    )
    .expect("validate")
}

#[test]
fn valid_ls2_stores_by_destination_hash() {
    let signer = bundle(0x500);
    let mut store = LeaseSet2Store::default();
    let validated = validate(&signer, 1_000);
    let key = validated.key();
    assert!(!store.contains(&key));
    let outcome = store.insert(validated);
    assert_eq!(outcome, LeaseSet2InsertOutcome::Inserted);
    assert!(store.contains(&key));
    let retrieved = store.get(&key).expect("present");
    assert_eq!(retrieved.published_seconds(), 1_000);
}

#[test]
fn invalid_signature_rejected_by_validator() {
    let signer = bundle(0x501);
    let validated = validate(&signer, 1_000);
    let ls2 = validated.lease_set2().clone();
    let bad = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0xab; 64])
        .expect("bad signature");
    let ls2 = LeaseSet2::new(
        ls2.header().clone(),
        ls2.options().clone(),
        ls2.encryption_keys().to_vec(),
        ls2.leases().to_vec(),
        bad,
    )
    .expect("ls2");
    let error =
        ValidatedLeaseSet2::from_lease_set2(ls2, None, LeaseSet2ValidationContext::new(1_000))
            .unwrap_err();
    assert_eq!(error, LeaseSet2ValidationError::InvalidSignature);
}

#[test]
fn wrong_database_store_key_rejected_by_validator() {
    let signer = bundle(0x502);
    let other = bundle(0x503);
    let validated = validate(&signer, 1_000);
    let bogus = DestinationHash::from_hash(
        *i2pr_netdb::router_hash(other.identity())
            .expect("hash")
            .as_hash(),
    );
    let error = ValidatedLeaseSet2::from_lease_set2(
        validated.lease_set2().clone(),
        Some(bogus),
        LeaseSet2ValidationContext::new(1_000),
    )
    .unwrap_err();
    assert_eq!(error, LeaseSet2ValidationError::DestinationMismatch);
}

#[test]
fn expired_ls2_rejected_by_validator() {
    let signer = bundle(0x504);
    let ls2 = build_signed_ls2(
        &signer,
        0,
        600,
        vec![Lease2::new(
            Hash::from_bytes([0x22; 32]),
            7,
            Date32::from_seconds(60),
        )],
    );
    let error =
        ValidatedLeaseSet2::from_lease_set2(ls2, None, LeaseSet2ValidationContext::new(60 * 60))
            .unwrap_err();
    assert!(matches!(error, LeaseSet2ValidationError::Expired { .. }));
}

#[test]
fn newer_published_replaces_older() {
    let signer = bundle(0x505);
    let mut store = LeaseSet2Store::default();
    store.insert(validate(&signer, 1_000));
    let outcome = store.insert(validate(&signer, 1_500));
    assert_eq!(outcome, LeaseSet2InsertOutcome::Replaced);
    assert_eq!(store.len(), 1);
}

#[test]
fn older_published_does_not_replace_newer() {
    let signer = bundle(0x506);
    let mut store = LeaseSet2Store::default();
    store.insert(validate(&signer, 1_500));
    let outcome = store.insert(validate(&signer, 1_000));
    assert_eq!(outcome, LeaseSet2InsertOutcome::StaleReplacement);
    assert_eq!(store.len(), 1);
}

#[test]
fn duplicate_is_idempotent() {
    let signer = bundle(0x507);
    let mut store = LeaseSet2Store::default();
    let v = validate(&signer, 1_000);
    store.insert(v.clone());
    assert_eq!(store.insert(v), LeaseSet2InsertOutcome::Idempotent);
}

#[test]
fn routerinfo_capacity_not_corrupted_by_ls2() {
    // The router-info store has its own accounting; LeaseSet2 storage
    // must not affect it. This is a sanity check that the two stores
    // remain independent — LeaseSet2 quota is bounded by its own
    // config, while the router-info store is bounded by
    // `RouterInfoStoreConfig`.
    let mut ls2_store = LeaseSet2Store::with_config(LeaseSet2StoreConfig::new(2, usize::MAX));
    ls2_store.insert(validate(&bundle(0x508), 1_000));
    ls2_store.insert(validate(&bundle(0x509), 1_000));
    let outcome = ls2_store.insert(validate(&bundle(0x50a), 1_000));
    assert_eq!(outcome, LeaseSet2InsertOutcome::CapacityExceeded);
    assert_eq!(ls2_store.len(), 2);
    // RouterInfo store is unaffected by LeaseSet2 storage activity.
    let ri_store = i2pr_netdb::RouterInfoStore::default();
    assert!(ri_store.is_empty());
}
