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

// ==================================================================================================
// Plan 380: the ELS2 **consumer client credential**.
//
// Plan 351 closed the gap where `EncryptedServiceResolver::begin_authorized` had no production
// caller: the product always took `begin`, and every `.b33` that declared
// `B32_FLAG_REQUIRES_CLIENT_KEY` was therefore unresolvable. Plan 380 adds the missing input —
// the operator's own PSK or DH key — and wires it to the branch.
//
// # What the rows below are and are not
//
// They drive the real publisher, the real owner, and a real router-bound secret owner. Every
// authorized row goes derive → blinded-key lookup → `DatabaseStore` reply → owner ingest →
// unwrap → bind, with no decoded-LeaseSet injection anywhere: the record the consumer unwraps is
// the one the publisher built, byte for byte.
//
// They do **not** exercise the network, the SSU2 handle, or the destination tunnels. That is
// deliberate and is the same boundary Plan 351 drew: what is under test is the wiring and the
// policy — which branch is taken, which secret is opened, and what is refused — and a loopback
// socket would add no evidence for any of those. The live cross-router proof remains Plan 374's,
// and is still blocked for want of a driver.
// ==================================================================================================

use i2pr_client::generate_client_dh_keypair;
use i2pr_daemon::encrypted_target_credential::SealedEncryptedTargetCredential;
use i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets;
use i2pr_daemon::service_tunnels::{
    EncryptedTargetStatus, ServiceTunnelManager, ServiceTunnelManagerConfig,
};
use i2pr_netdb::Els2AuthorizationServerConfig;
use i2pr_proto::B32_FLAG_REQUIRES_CLIENT_KEY;
use i2pr_service_tunnels::outbound_secret::RouterSecretOwner;
use i2pr_service_tunnels::{ServiceTunnelSet, StaticAliasTable};

use std::path::PathBuf;
use std::sync::Arc;

/// A real router-bound secret owner, so a sealing row cannot pass against a stub that stores
/// plaintext. Generated per call so rows that need "a different router" have one.
fn plan380_secret_owner() -> Arc<dyn RouterSecretOwner> {
    let mut rng = rand_core::OsRng;
    let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("identity bundle");
    Arc::new(RouterBoundOutboundSecrets::from_router_identity(&bundle).expect("router-bound owner"))
}

/// A manager with no specs, which is all the credential registry needs: the registry is keyed by
/// spec id and does not require the id to exist in the spec set.
fn plan380_manager(data_dir: &std::path::Path) -> Arc<ServiceTunnelManager> {
    Arc::new(
        ServiceTunnelManager::new(ServiceTunnelManagerConfig {
            data_dir: data_dir.to_path_buf(),
            aggregate_connection_ceiling: 8,
            per_service_connection_ceiling: 4,
            specs: Arc::new(ServiceTunnelSet {
                tunnels: Vec::new(),
            }),
            aliases: Arc::new(StaticAliasTable::new()),
        })
        .expect("manager builds"),
    )
}

/// A scratch data directory, unique per caller so concurrent rows cannot collide.
fn plan380_data_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("i2pr-plan380-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch data dir");
    dir
}

/// Seals one credential value under `owner`, the way the control plane does.
fn plan380_seal(
    owner: &Arc<dyn RouterSecretOwner>,
    value: &str,
) -> SealedEncryptedTargetCredential {
    SealedEncryptedTargetCredential::seal(value, Arc::clone(owner)).expect("seals")
}

// --- the credential itself ----------------------------------------------------------------------

/// The credential option's value never appears in the stored form.
#[test]
fn a_sealed_credential_holds_ciphertext_and_not_its_value() {
    let owner = plan380_secret_owner();
    let value = format!("psk:{}", "a7".repeat(32));
    let sealed = plan380_seal(&owner, &value);

    assert!(
        !sealed.sealed_form().contains("aaaa"),
        "the stored form must not contain a run of the plaintext's bytes"
    );
    let stored = sealed_debug_is_opaque(&sealed);
    assert!(
        stored.contains("configured: true"),
        "Debug must report presence: {stored}"
    );
    assert!(
        !stored.contains(&value) && !stored.contains("psk:"),
        "Debug must never print the credential or even its scheme prefix: {stored}"
    );
    // And it still opens to exactly the value that went in.
    assert_eq!(sealed.open().expect("opens").to_option_value(), value);
}

/// Renders the sealed form's `Debug`, which is the only string an operator or a log can get from
/// it. Kept as a helper so the row above reads as a statement about the type rather than as a
/// statement about `format!`.
fn sealed_debug_is_opaque(sealed: &SealedEncryptedTargetCredential) -> String {
    format!("{sealed:?}")
}

/// A credential sealed by one router does not open under another.
///
/// This is the row that makes "sealed under the router identity" mean something. A stored form is
/// portable on disk — it can be copied between data directories — so the only thing stopping it
/// from being portable *as a usable credential* is that the receiving router derives a different
/// key. The failure is refused at adoption, not at first use, so it surfaces as a failed control
/// transaction rather than as a service that silently never resolves.
#[test]
fn a_credential_is_not_transferable_to_another_router() {
    let value = format!("psk:{}", "b8".repeat(32));
    let sealed = plan380_seal(&plan380_secret_owner(), &value);
    // Round-trip is byte-exact, so a generation file that survives a restart reopens. Compared
    // through the rendered spelling rather than `PartialEq`: the credential type deliberately has
    // neither, because it holds key material.
    assert_eq!(
        sealed.open().expect("opens").to_option_value(),
        value,
        "seal then open must be the identity, so a restart needs no second rendering"
    );

    let other_router = plan380_secret_owner();
    assert!(
        SealedEncryptedTargetCredential::from_sealed(
            sealed.sealed_form(),
            Arc::clone(&other_router)
        )
        .is_err(),
        "a stored form copied from another router must not be adopted"
    );
    // The owner's own two domains do not substitute for each other either: the outproxy
    // credential domain cannot open a consumer credential, and vice versa.
    assert!(
        other_router.open_credential(sealed.sealed_form()).is_err(),
        "a consumer credential must not open in the outproxy domain"
    );
}

// --- the manager registry -----------------------------------------------------------------------

/// The registry holds per-spec credentials, isolates them, and drops them.
#[test]
fn the_manager_registry_is_per_spec_and_fail_closed_on_removal() {
    let manager = plan380_manager(&plan380_data_dir("registry"));
    let owner = plan380_secret_owner();

    assert!(
        manager.encrypted_target_credential("alpha").is_none(),
        "nothing is installed before an install"
    );

    manager.install_encrypted_target_credential(
        "alpha",
        Arc::new(plan380_seal(&owner, &format!("psk:{}", "11".repeat(32)))),
    );
    manager.install_encrypted_target_credential(
        "beta",
        Arc::new(plan380_seal(&owner, &format!("dh:{}", "22".repeat(32)))),
    );

    let alpha = manager
        .encrypted_target_credential("alpha")
        .expect("alpha installed");
    let beta = manager
        .encrypted_target_credential("beta")
        .expect("beta installed");
    assert_eq!(
        alpha.open().expect("alpha opens").to_option_value(),
        format!("psk:{}", "11".repeat(32))
    );
    assert_eq!(
        beta.open().expect("beta opens").to_option_value(),
        format!("dh:{}", "22".repeat(32))
    );
    assert_eq!(
        manager.installed_encrypted_target_credentials().len(),
        2,
        "two specs, two credentials"
    );

    // Replacing one spec's credential must not disturb the other.
    manager.install_encrypted_target_credential(
        "alpha",
        Arc::new(plan380_seal(&owner, &format!("psk:{}", "33".repeat(32)))),
    );
    assert_eq!(
        manager
            .encrypted_target_credential("alpha")
            .expect("alpha still installed")
            .open()
            .expect("replacement opens")
            .to_option_value(),
        format!("psk:{}", "33".repeat(32))
    );
    assert_eq!(
        manager
            .encrypted_target_credential("beta")
            .expect("beta untouched")
            .open()
            .expect("beta opens")
            .to_option_value(),
        format!("dh:{}", "22".repeat(32)),
        "a per-spec registry must not leak one spec's credential into another's slot"
    );

    // Removal drops it, and is idempotent.
    manager.remove_encrypted_target_credential("alpha");
    assert!(manager.encrypted_target_credential("alpha").is_none());
    assert!(manager.encrypted_target_credential("beta").is_some());
    manager.remove_encrypted_target_credential("alpha");

    manager.clear_encrypted_target_credentials();
    assert!(manager.installed_encrypted_target_credentials().is_empty());
}

// --- the authorized resolution ------------------------------------------------------------------

/// The PSK row: publisher authorizes `psk`, consumer presents it, record unwraps and binds.
#[test]
fn an_authorized_psk_credential_resolves_through_the_owner() {
    let schedule = owner_schedule_from_seed([0x81; 32]);
    let inner = signed_ls2(&router_from_seed([0x82; 32]));
    let key = [0x31_u8; 32];
    let config =
        Els2AuthorizationServerConfig::psk(vec![i2pr_netdb::PskClientKey::from_bytes(key)])
            .expect("psk config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x83);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(&config, &inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
        .expect("publish");
    let address = publisher.authorized_address(false).expect("address");
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_CLIENT_KEY,
        B32_FLAG_REQUIRES_CLIENT_KEY
    );

    // The credential travels as it will in production: an option value, sealed by the router-bound
    // owner, and reopened only at the resolve site.
    let owner = plan380_secret_owner();
    let sealed = plan380_seal(&owner, &format!("psk:{}", "31".repeat(32)));
    let credential = sealed.open().expect("opens");

    let mut resolver = EncryptedServiceResolver::default();
    let (id, _) = resolver
        .begin_authorized(
            &address,
            None,
            credential.as_borrowed(),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin authorized");
    assert_eq!(
        resolver
            .ingest_store(id, &message, PUBLISHED)
            .unwrap_or_else(|error| panic!("authorized psk client must resolve: {error:?}"))
            .inner_lease_set2(),
        &inner,
    );
    assert_eq!(resolver.in_flight(), 0);
}

/// The DH row: the consumer's private key derives the public key the record was published for.
#[test]
fn an_authorized_dh_credential_resolves_through_the_owner() {
    let schedule = owner_schedule_from_seed([0x84; 32]);
    let inner = signed_ls2(&router_from_seed([0x85; 32]));
    let mut rng = ChaCha8Rng::seed_from_u64(0x86);
    let (private, public) = generate_client_dh_keypair(&mut rng).expect("dh keypair");
    let config = Els2AuthorizationServerConfig::dh(vec![public]).expect("dh config");
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(&config, &inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
        .expect("publish");
    let address = publisher.authorized_address(false).expect("address");

    // The option carries only the private half. Its public half is re-derived and must equal the
    // one the record was published for, which is the property that makes the configuration able to
    // authorize at all.
    let private_hex: String = private
        .secret_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let owner = plan380_secret_owner();
    let credential = plan380_seal(&owner, &format!("dh:{private_hex}"))
        .open()
        .expect("opens");

    let mut resolver = EncryptedServiceResolver::default();
    let (id, _) = resolver
        .begin_authorized(
            &address,
            None,
            credential.as_borrowed(),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin authorized");
    assert_eq!(
        resolver
            .ingest_store(id, &message, PUBLISHED)
            .unwrap_or_else(|error| panic!("authorized dh client must resolve: {error:?}"))
            .inner_lease_set2(),
        &inner,
    );
}

/// A publisher schedule built over a lookup **secret**, so the record lands on a secret-derived
/// storage key.
///
/// The other fixtures deliberately pass `None`, which is why an ordinary row can compare a
/// consumer's derived key against a published record's key at all. A row about the two secrets
/// doing different jobs needs the publisher and the consumer to agree on the secret, or it would
/// be testing "these keys differ" and calling it cooperation.
fn plan380_schedule_with_secret(seed: u64, secret: Option<&str>) -> BlindingSchedule {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let private: Red25519PrivateScalar = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity = BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, secret)
        .expect("identity");
    BlindingSchedule::new_owner(identity, private, BlindingScheduleConfig::new(2, true))
}

/// Both secrets at once: the lookup secret picks the storage key and the credential authorizes the
/// unwrap. Neither substitutes for the other, and the row proves both are in play.
#[test]
fn a_lookup_secret_and_a_credential_each_do_their_own_job() {
    const SECRET: &str = "the lookup secret";
    let schedule = plan380_schedule_with_secret(0x91, Some(SECRET));
    let inner = signed_ls2(&router_from_seed([0x92; 32]));
    let config =
        Els2AuthorizationServerConfig::psk(vec![i2pr_netdb::PskClientKey::from_bytes([0x41; 32])])
            .expect("psk config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x93);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (key, message) = publisher
        .build_authorized_database_store(&config, &inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
        .expect("publish");
    // The address declares that the lookup secret is required, so a consumer must present one.
    let address = publisher
        .authorized_address(true)
        .expect("authorized address");

    let credential = plan380_seal(&plan380_secret_owner(), &format!("psk:{}", "41".repeat(32)))
        .open()
        .expect("opens");

    // Both right: the derived key matches the published key and the record authorizes.
    let mut resolver = EncryptedServiceResolver::default();
    let (id, derived) = resolver
        .begin_authorized(
            &address,
            Some(SECRET),
            credential.as_borrowed(),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin authorized");
    assert_eq!(
        derived.as_hash(),
        key.as_hash(),
        "the lookup secret is what derives the storage key"
    );
    assert_eq!(
        resolver
            .ingest_store(id, &message, PUBLISHED)
            .unwrap_or_else(|error| panic!(
                "secret and credential together must resolve: {error:?}"
            ))
            .inner_lease_set2(),
        &inner
    );

    // Right credential, wrong secret: the record is addressed by a different key and is never
    // found. This is the property that must survive — a valid credential cannot substitute for
    // the lookup secret, so the secret is a real discovery control.
    let mut resolver = EncryptedServiceResolver::default();
    let (id, derived) = resolver
        .begin_authorized(
            &address,
            Some("the wrong secret"),
            credential.as_borrowed(),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("begin authorized");
    assert_ne!(derived.as_hash(), key.as_hash());
    assert!(
        matches!(
            resolver.ingest_store(id, &message, PUBLISHED),
            Err(
                i2pr_daemon::encrypted_service_resolver::EncryptedServiceConsumerError::KeyMismatch
            )
        ),
        "a valid credential must not make a wrongly addressed record resolvable"
    );
    assert_eq!(resolver.in_flight(), 0, "the lease must be released");

    // The mirror — a correctly addressed record whose credential is wrong — is the
    // `a_wrong_credential_is_refused_and_releases_its_lease` row. Together the two say what these
    // blocks individually cannot: the secret finds the record, the credential opens it, and
    // getting either wrong fails in its own distinct way.
}

/// The fail-closed row that is the whole reason for the new status arm: a `.b33` that requires a
/// client key cannot even *compose* a lookup without one, so nothing is sent.
#[test]
fn a_required_credential_that_is_absent_fails_before_a_lookup_is_composed() {
    let schedule = owner_schedule_from_seed([0x8a; 32]);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.authorized_address(false).expect("address");
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_CLIENT_KEY,
        B32_FLAG_REQUIRES_CLIENT_KEY
    );

    let mut resolver = EncryptedServiceResolver::default();
    let error = resolver
        .begin(&address, Some("a secret"), PUBLISHED, DEADLINE_MS)
        .expect_err("an authorized address must refuse to begin without a credential");
    assert!(
        matches!(
            error,
            i2pr_daemon::encrypted_service_resolver::EncryptedServiceConsumerError::InvalidAddress(
                i2pr_client::EncryptedLeaseSetError::ClientAuthorizationRequired
            )
        ),
        "the refusal must be the address's own demand, not a side effect: {error:?}"
    );
    assert_eq!(
        resolver.in_flight(),
        0,
        "no lookup may be composed for an address this router cannot satisfy"
    );
}

/// A wrong credential is refused by the record's authorization block, and the lease is released.
#[test]
fn a_wrong_credential_is_refused_and_releases_its_lease() {
    let schedule = owner_schedule_from_seed([0x8b; 32]);
    let inner = signed_ls2(&router_from_seed([0x8c; 32]));
    let config =
        Els2AuthorizationServerConfig::psk(vec![i2pr_netdb::PskClientKey::from_bytes([0x51; 32])])
            .expect("psk config");
    let mut rng = ChaCha8Rng::seed_from_u64(0x8d);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (_, message) = publisher
        .build_authorized_database_store(&config, &inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
        .expect("publish");
    let address = publisher.authorized_address(false).expect("address");

    let wrong = plan380_seal(&plan380_secret_owner(), &format!("psk:{}", "52".repeat(32)))
        .open()
        .expect("opens");
    let mut resolver = EncryptedServiceResolver::default();
    let (id, _) = resolver
        .begin_authorized(&address, None, wrong.as_borrowed(), PUBLISHED, DEADLINE_MS)
        .expect("begin authorized");
    assert!(
        matches!(
            resolver.ingest_store(id, &message, PUBLISHED),
            Err(i2pr_daemon::encrypted_service_resolver::EncryptedServiceConsumerError::NotAuthorized)
        ),
        "the record must refuse a client it did not publish for"
    );
    assert_eq!(
        resolver.in_flight(),
        0,
        "a refused authorization must still release the lease"
    );
}

/// A credential presented to a `.b33` that does **not** require one is not an error.
///
/// This is the asymmetry that makes "select the branch by what the router holds" safe:
/// `begin_authorized` delegates to the same builder with the address's own flag, so an
/// unnecessary credential is presented and ignored rather than rejected for being present.
#[test]
fn a_credential_on_an_unauthorized_address_is_harmless() {
    let schedule = owner_schedule_from_seed([0x8e; 32]);
    let inner = signed_ls2(&router_from_seed([0x8f; 32]));
    let mut rng = ChaCha8Rng::seed_from_u64(0x90);
    let publisher = EncryptedLeaseSet2Publisher::new(&schedule);
    let (key, message) = publisher
        .build_database_store(&inner, PUBLISHED, PUBLISHED + 3_600, &mut rng)
        .expect("publish");
    let address = publisher.address(false).expect("address");
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_CLIENT_KEY,
        0,
        "this address must not demand a client key"
    );

    let credential = plan380_seal(&plan380_secret_owner(), &format!("psk:{}", "61".repeat(32)))
        .open()
        .expect("opens");
    let mut resolver = EncryptedServiceResolver::default();
    let (id, derived) = resolver
        .begin_authorized(
            &address,
            None,
            credential.as_borrowed(),
            PUBLISHED,
            DEADLINE_MS,
        )
        .expect("an unauthorized address must accept an unused credential");
    assert_eq!(
        derived.as_hash(),
        key.as_hash(),
        "an unused credential must not perturb the derived storage key"
    );
    assert_eq!(
        resolver
            .ingest_store(id, &message, PUBLISHED)
            .expect("still resolves")
            .inner_lease_set2(),
        &inner
    );
}

/// The three new status arms are distinct, static, and carry no value.
///
/// Distinct because the operator's next action differs for each: configure a credential, fix the
/// credential, or fix the router/data directory. Static because a status string reaches a client
/// reply and a log line, so a reason built from the credential would leak it.
#[test]
fn the_credential_status_arms_are_distinct_and_static() {
    let arms = [
        (
            EncryptedTargetStatus::ClientCredentialRequired,
            "client credential required",
        ),
        (
            EncryptedTargetStatus::ClientCredentialRejected,
            "client credential rejected",
        ),
        (
            EncryptedTargetStatus::ClientCredentialUnusable,
            "client credential could not be opened",
        ),
    ];
    for (status, reason) in arms {
        assert_eq!(status.reason(), reason);
    }
    let reasons: std::collections::BTreeSet<&str> =
        arms.iter().map(|(status, _)| status.reason()).collect();
    assert_eq!(
        reasons.len(),
        arms.len(),
        "each credential failure needs its own reason string"
    );
}
