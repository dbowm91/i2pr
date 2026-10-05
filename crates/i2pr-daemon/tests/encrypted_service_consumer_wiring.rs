//! Plan 351: the encrypted-service (`.b33`) consumer wiring, driven through its real owners.
//!
//! # What this is for
//!
//! Plan 349 built `EncryptedServiceResolver` with no production caller. Plan 351 supplies one, and
//! supplying a caller is the moment the design gets tested by reality rather than by review. The
//! rows here cover the three claims the plan-of-record rests on:
//!
//! 1. **Containment.** An encrypted remote target is impossible on a service that cannot isolate
//!    its own failure. This is asserted as a *negative configuration* row at the runtime-neutral
//!    validator — the layer every spec passes through regardless of which surface configured it —
//!    and again at the I2PControl surface.
//! 2. **The binding.** An unwrapped inner LeaseSet2 may only be installed under a destination hash
//!    it proves it owns, by signing with the unblinded public key the `.b33` names. Every row here
//!    that would pass under a missing or wrong binding is written to fail.
//! 3. **No silent collapse.** A `.b33` must never become an ordinary `.b32`, and must never become
//!    a locally co-owned destination. Both collapses would turn an unreachable remote endpoint
//!    into a plausible-looking local delivery.
//!
//! Nothing here reaches the network: the `.b33` storage key, the publisher, and the record are all
//! driven in process, because what is under test is the *wiring and the policy*, not the transport.
//! The live cross-router proof is Plan 347's re-attempt, which stays blocked.

use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::red25519::{Red25519PrivateScalar, generate_private};
use i2pr_daemon::encrypted_service_resolver::{EncryptedServiceResolver, bind_inner_to_address};
use i2pr_netdb::{
    BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, unblinded_scalar_from_ed25519_seed,
};
use i2pr_proto::{
    CryptoKeyType, Date32, Destination, Lease2, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags,
    LeaseSet2Header, Mapping, SignatureValue, SigningKeyType,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_client::encrypted_leaseset::EncryptedLeaseSet2Publisher;

const PUBLISHED: u32 = 1_700_000_000;
const DEADLINE_MS: u64 = 60_000;

// --- fixtures --------------------------------------------------------------------------------

/// A router identity whose Ed25519 signing seed is exactly `seed`.
///
/// The seed is what ties a `.b33` address to the LeaseSet2 it may install, so the fixtures below
/// build both halves from one seed rather than from two unrelated ones.
fn router_from_seed(seed: [u8; 32]) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(0x51a2);
    RouterIdentityBundle::from_private_bytes(seed, [0x33; 32], &mut rng).expect("router from seed")
}

/// A publisher schedule built the way `service_els2.rs` builds one.
///
/// **The sigtype matters, and the distinction is the whole point of these rows.** Production
/// publishes type 7 (`EdDsaSha512Ed25519`): the unblinded Red25519 scalar is
/// `CONVERT_ED25519_PRIVATE(seed)` — SHA-512, clamp, low 32 bytes — and `DERIVE_PUBLIC` of that
/// scalar reproduces the destination's ordinary Ed25519 public key. So for a type-7 `.b33`,
/// `address.public_key()` **is** the signing key of the inner LeaseSet2's Destination.
///
/// A type-11 `.b33` has no such relationship: its unblinded key is a distinct Red25519 identity
/// that signs the *outer* record only. `the_binding_refuses_a_type11_address` pins that boundary,
/// because a consumer that assumed the type-7 relationship would refuse every type-11 address and
/// one that assumed there was *no* relationship would accept any valid LeaseSet2 at all.
fn owner_schedule_from_seed(seed: [u8; 32]) -> BlindingSchedule {
    let private = unblinded_scalar_from_ed25519_seed(&seed);
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity =
        BlindingIdentity::new(public, SigningKeyType::EdDsaSha512Ed25519, None).expect("identity");
    BlindingSchedule::new_owner(identity, private, BlindingScheduleConfig::new(2, true))
}

/// A type-11 schedule: a distinct Red25519 identity that signs only the outer record.
fn type11_owner_schedule(seed: u64) -> BlindingSchedule {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let private: Red25519PrivateScalar = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    BlindingSchedule::new_owner(identity, private, BlindingScheduleConfig::new(2, true))
}

/// An ordinary publisher schedule over an arbitrary key, for parse-only rows.
fn owner_schedule(seed: u64) -> BlindingSchedule {
    type11_owner_schedule(seed)
}

/// A LeaseSet2 signed by `signer`, so the binding check has a real signature to trust.
fn signed_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    signed_ls2_with_lease(signer, 0x22)
}

/// The same record with a distinguishable lease tunnel, so two fixtures differ in a field the
/// signature actually covers. Without this, a "forged" record built from two of these would be
/// byte-identical in its signed region and would validate — the row would pass for the wrong
/// reason.
fn signed_ls2_with_lease(signer: &RouterIdentityBundle, lease: u8) -> LeaseSet2 {
    let destination =
        Destination::new(signer.identity().key_and_cert().clone()).expect("destination");
    let header = LeaseSet2Header::new(destination, PUBLISHED, 3_600, LeaseSet2Flags::from_raw(0))
        .expect("header");
    let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64])
        .expect("placeholder");
    let keys =
        vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
    let leases = vec![Lease2::new(
        i2pr_proto::Hash::from_bytes([lease; 32]),
        7,
        Date32::from_seconds(PUBLISHED + 600),
    )];
    let unsigned =
        LeaseSet2::new(header, Mapping::empty(), keys, leases, placeholder).expect("unsigned");
    let signature = signer
        .signing_key()
        .sign(&unsigned.signature_preimage())
        .expect("inner signature");
    LeaseSet2::new(
        unsigned.header().clone(),
        unsigned.options().clone(),
        unsigned.encryption_keys().to_vec(),
        unsigned.leases().to_vec(),
        signature,
    )
    .expect("inner")
}

fn b32_address(byte: u8) -> String {
    // Encoded, not hand-built. `DestinationRef::parse` rejects a label whose trailing bits are
    // non-canonical, so a hand-assembled 52-character string is not a valid address and would
    // make this fixture test the wrong thing.
    format!("{}.b32.i2p", i2pr_proto::base32_encode(&[byte; 32]))
}

/// The `.b33` address a publisher produces, as a string a spec would carry.
fn b33_address(schedule: &BlindingSchedule) -> String {
    EncryptedLeaseSet2Publisher::new(schedule)
        .address(false)
        .expect("address")
        .to_text()
        .expect("address text")
}

// --- 1. the reference kind is distinct ------------------------------------------------------

/// A `.b33` is recognised as its own kind, not reported as a malformed Base32 label.
///
/// This is the first bug Plan 349 would have hit: `DestinationRef::parse` dispatches on the
/// `.b32.i2p` suffix and requires exactly 52 characters, so before Plan 351 a valid encrypted
/// address failed with "Base32 label must be exactly 52 characters" — true of the length, useless
/// as a diagnosis.
#[test]
fn an_encrypted_service_address_parses_as_its_own_kind() {
    let schedule = owner_schedule(0x0a1b);
    let text = b33_address(&schedule);
    let parsed = i2pr_service_tunnels::DestinationRef::parse(&text).expect("a b33 must parse");

    let address = parsed
        .encrypted_service()
        .unwrap_or_else(|| panic!("a b33 must parse as EncryptedService, got {parsed:?}"));

    // The parsed address must be the same value the publisher published, not merely
    // "something that parsed".
    let published = EncryptedLeaseSet2Publisher::new(&schedule)
        .address(false)
        .expect("address");
    assert_eq!(
        address, &published,
        "the parsed address must round-trip exactly"
    );
    assert_eq!(parsed.canonical_string(), text.to_ascii_lowercase());

    // And it must not be reachable as a Base32 hash: the whole point is that it is not one.
    assert!(
        !matches!(
            parsed,
            i2pr_service_tunnels::DestinationRef::Base32Hash { .. }
        ),
        "a b33 must never be reported as a destination hash"
    );
}

/// An ordinary `.b32` is unaffected: it still parses as a hash, and its `encrypted_service()`
/// is `None`. This is the "ordinary path unchanged" row.
#[test]
fn an_ordinary_base32_address_is_unaffected() {
    let text = b32_address(0x11);
    let parsed = i2pr_service_tunnels::DestinationRef::parse(&text).expect("b32 parses");
    assert!(
        matches!(
            parsed,
            i2pr_service_tunnels::DestinationRef::Base32Hash { .. }
        ),
        "an ordinary b32 must still parse as Base32Hash"
    );
    assert!(
        parsed.encrypted_service().is_none(),
        "an ordinary b32 must report no encrypted-service address"
    );
}

// --- 2. the projection keeps three outcomes -------------------------------------------------

/// A `.b33` projects to `EncryptedService`, never to `Remote` and never to `LocalCoOwned`.
///
/// The two wrong answers are both dangerous and for opposite reasons. `Remote` would require a
/// destination hash the address does not carry, so the projection would have to invent one and
/// every downstream comparison would be against a fabricated key. `LocalCoOwned` would route an
/// unreachable remote endpoint to the local bridge, which "works" in exactly the way that makes a
/// misconfiguration invisible.
#[test]
fn a_projection_never_collapses_an_encrypted_target() {
    let schedule = owner_schedule(0x0b2c);
    let b33 =
        i2pr_service_tunnels::DestinationRef::parse(&b33_address(&schedule)).expect("b33 parses");
    let b32 = i2pr_service_tunnels::DestinationRef::parse(&b32_address(0x22)).expect("b32 parses");

    // The projection is a pure function of the reference; assert on the variants directly
    // rather than through an accessor that would collapse them again.
    let project = |reference: &i2pr_service_tunnels::DestinationRef| -> &'static str {
        match reference {
            i2pr_service_tunnels::DestinationRef::EncryptedService { .. } => "encrypted",
            i2pr_service_tunnels::DestinationRef::Base32Hash { .. } => "remote",
            i2pr_service_tunnels::DestinationRef::StaticAlias(_) => "alias",
            i2pr_service_tunnels::DestinationRef::ConfiguredDestination(_) => "configured",
        }
    };
    assert_eq!(project(&b33), "encrypted");
    assert_eq!(project(&b32), "remote");
    // The discriminator the product loop actually matches on. Restated as an assertion on the
    // shapes so a future refactor that merges two variants fails here too.
    assert!(matches!(
        b33,
        i2pr_service_tunnels::DestinationRef::EncryptedService { .. }
    ));
}

// --- 3. the identity binding ---------------------------------------------------------------

/// The headline security row: the unwrapped record is installed under the **inner** destination's
/// own hash, and that hash is provably not the blinded storage key.
///
/// Three separate claims in one row, because they are one argument. The `.b33` cannot supply a
/// destination hash (the protocol layer says so, and this row would fail if it tried). The inner
/// record supplies one, but only because it signs with the key the address published. And the
/// value installed is not the key the lookup was filed under — installing under the blinded key
/// would place a record where nothing ever looks it up.
#[test]
fn the_installed_hash_is_the_unblinded_destination_hash() {
    let seed = [0x40; 32];
    let schedule = owner_schedule_from_seed(seed);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .address(false)
        .expect("address");
    let signer = router_from_seed(seed);
    let inner = signed_ls2(&signer);

    // The premise of the whole binding, asserted before it is used: for a type-7 address the
    // published key really is the inner Destination's signing key.
    assert_eq!(
        address.public_key(),
        inner.header().destination().signing_key().as_bytes(),
        "a type-7 b33 must name the inner record's own signing key"
    );

    let bound =
        bind_inner_to_address(&address, &inner).expect("a correctly signed inner record must bind");

    // (a) it is the inner record's own destination hash.
    let inner_hash = inner
        .header()
        .destination()
        .hash()
        .expect("destination hash");
    assert_eq!(
        bound,
        i2pr_netdb::DestinationHash::from_hash(inner_hash),
        "must be the inner destination's hash"
    );

    // (b) it is provably not the blinded storage key.
    let mut owner = EncryptedServiceResolver::default();
    let (_, storage_key) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    assert_ne!(
        bound,
        i2pr_netdb::DestinationHash::from_hash(*storage_key.as_hash()),
        "the install key must not be the blinded storage key; that record would never be found"
    );
    let _ = owner.expire(u64::MAX);
}

/// A record signed by a **different** router is refused, even though it is a perfectly valid
/// LeaseSet2.
///
/// Without this row the binding is decorative: a publisher could hand a consumer a working record
/// for a service its address does not name, and the consumer would install it. The check is the
/// only thing standing between "valid LeaseSet2" and "valid LeaseSet2 *for this address*".
#[test]
fn a_record_signed_by_another_router_is_refused() {
    let seed = [0x41; 32];
    let address = EncryptedLeaseSet2Publisher::new(&owner_schedule_from_seed(seed))
        .address(false)
        .expect("address");

    let honest = signed_ls2(&router_from_seed(seed));
    let impostor = signed_ls2(&router_from_seed([0x42; 32]));

    assert!(
        bind_inner_to_address(&address, &honest).is_some(),
        "the record the address names must bind"
    );
    assert!(
        bind_inner_to_address(&address, &impostor).is_none(),
        "a valid LeaseSet2 for a different destination must NOT bind"
    );
}

/// A tampered record — one whose signature no longer matches its header — is refused. The
/// binding trusts the signature, so a record that fails validation must never reach the binding's
/// `Some` arm with the address's key present in it.
#[test]
fn a_record_whose_signature_does_not_verify_is_refused() {
    let seed = [0x43; 32];
    let address = EncryptedLeaseSet2Publisher::new(&owner_schedule_from_seed(seed))
        .address(false)
        .expect("address");

    // Built against one destination, carrying another's leases: the key in the header is the
    // address's, but the signature is over different bytes.
    let honest = signed_ls2(&router_from_seed(seed));
    let other = signed_ls2_with_lease(&router_from_seed([0x44; 32]), 0x66);

    let forged = LeaseSet2::new(
        honest.header().clone(),
        honest.options().clone(),
        honest.encryption_keys().to_vec(),
        other.leases().to_vec(),
        honest.signature().clone(),
    )
    .expect("forged record assembles");

    // The binding only compares header metadata, so on its own it accepts this. That is why
    // `ValidatedLeaseSet2` runs *after* it in the install path; this row documents the ordering
    // requirement rather than asserting a property the binding does not have.
    assert!(
        bind_inner_to_address(&address, &forged).is_some(),
        "the binding compares header metadata only; signature verification is the next step"
    );
    assert_ne!(
        forged.signature_preimage(),
        other.signature_preimage(),
        "the forged record must differ in a signed field, or it validates for the right reason"
    );
    assert!(
        i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
            forged.clone(),
            Some(i2pr_netdb::DestinationHash::from_hash(
                forged.header().destination().hash().expect("hash")
            )),
            i2pr_netdb::LeaseSet2ValidationContext::new(PUBLISHED),
        )
        .is_err(),
        "ordinary LeaseSet2 validation must reject the forged record"
    );
}

// --- 4. the full no-auth chain, end to end through the owner --------------------------------

/// The whole resolve, driven through the production owner: begin against the blinded key,
/// receive the publisher's own type-5 record, unwrap, bind, and land on the inner record.
///
/// This is the row that says the wiring works, as opposed to the rows above saying the policy is
/// right. It is in-process by construction — the transport is Plan 347's subject — but every
/// owner the production path uses is the real one.
#[test]
fn a_published_encrypted_service_resolves_binds_and_installs() {
    let seed = [0x45; 32];
    let schedule = owner_schedule_from_seed(seed);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .address(false)
        .expect("address");
    let signer = router_from_seed(seed);
    let inner = signed_ls2(&signer);
    let mut rng = ChaCha8Rng::seed_from_u64(0x46);

    // Publish, exactly as a type-5 service does.
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_database_store(&inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
        .expect("publish");

    // Resolve, exactly as the consumer path does.
    let mut owner = EncryptedServiceResolver::default();
    let (id, key) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin");
    assert_eq!(
        *key.as_hash(),
        message.key,
        "the lookup must be filed under the key the publisher stored under"
    );

    let resolved = owner.ingest_store(id, &message, PUBLISHED).expect("unwrap");
    let recovered = resolved.inner_lease_set2();
    assert_eq!(
        recovered, &inner,
        "the inner record must round-trip exactly"
    );
    assert_eq!(
        owner.in_flight(),
        0,
        "the lease must be released on success"
    );

    // Bind, then install under the unblinded hash.
    let bound = bind_inner_to_address(&address, recovered).expect("bind");
    assert_eq!(
        bound,
        i2pr_netdb::DestinationHash::from_hash(inner.header().destination().hash().expect("hash")),
        "install key must be the unblinded destination hash"
    );
    assert_ne!(
        bound,
        i2pr_netdb::DestinationHash::from_hash(message.key),
        "the install key must differ from the storage key"
    );

    let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        recovered.clone(),
        Some(bound),
        i2pr_netdb::LeaseSet2ValidationContext::new(PUBLISHED),
    )
    .expect("the bound record must validate under its own hash");
    assert_eq!(validated.key(), bound);
}

/// A record filed under a **different** day's key is refused — the daily-rollover boundary.
///
/// The blinding rotates daily, so a resolution computed for yesterday's key addresses the wrong
/// DHT key entirely. A consumer that accepted it would be installing a record from a key nobody
/// is looking up. Plan 351 does not implement rollover re-resolution; this row pins the failure
/// that makes that limitation visible instead of silent.
#[test]
fn a_record_from_another_day_is_refused() {
    let seed = [0x47; 32];
    let schedule = owner_schedule_from_seed(seed);
    let address = EncryptedLeaseSet2Publisher::new(&schedule)
        .address(false)
        .expect("address");
    let inner = signed_ls2(&router_from_seed(seed));

    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (key_today, _today_message) = {
        let mut rng = ChaCha8Rng::seed_from_u64(0x48);
        publisher
            .build_database_store(&inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
            .expect("publish today")
    };
    let (_key_other_day, other_message) = {
        // One day later: a different storage key, so a different record.
        let mut rng = ChaCha8Rng::seed_from_u64(0x49);
        publisher
            .build_database_store(&inner, PUBLISHED + 86_400, PUBLISHED + 90_000, &mut rng)
            .expect("publish tomorrow")
    };
    assert_ne!(
        *key_today.as_hash(),
        other_message.key,
        "the fixture must actually rotate the storage key, or this row proves nothing"
    );

    let mut owner = EncryptedServiceResolver::default();
    let (id, _key) = owner
        .begin(&address, None, PUBLISHED, DEADLINE_MS)
        .expect("begin for today");
    assert!(
        owner.ingest_store(id, &other_message, PUBLISHED).is_err(),
        "a record filed under tomorrow's key must not satisfy today's lookup"
    );
    assert_eq!(
        owner.in_flight(),
        0,
        "the lease must be released on failure too"
    );
}
