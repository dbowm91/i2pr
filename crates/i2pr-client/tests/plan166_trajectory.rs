//! Plan 166 client-owned destination + LeaseSet2 validation trajectory.
//!
//! These unit tests exercise the M9 I2CP client-owned destination path:
//!
//! - The destination runtime never receives the client's signing
//!   private key.
//! - `install_client_lease_set2` validates the supplied Standard
//!   LeaseSet2 atomically with the supplied inbound decryption
//!   capability; the encryption public key inside the LS2 must match
//!   the capability's public key.
//! - Foreign, expired, duplicate, or unknown leases are rejected.
//! - Lease rotation requests a fresh Standard LeaseSet2 from the
//!   client rather than signing one locally.
//! - Cancellation releases every owned resource.
//!
//! The fixtures reuse the deterministic inbound/outbound tunnel
//! builders under [`i2pr_client::testing`] so the lease (gateway,
//! tunnel_id) pairs the runtime owns are exactly the pairs the LS2
//! must advertise.

use i2pr_client::{
    DestinationConfig, DestinationIdentity, DestinationIdentityError, DestinationOwnership,
    DestinationPublic, DestinationRuntime, InboundDecryptionCapability, LeaseSetError,
    build_signed_lease_set2,
};
use i2pr_crypto::verify_lease_set2;
use i2pr_proto::{Lease2, LeaseSet2};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_client::testing::{established_inbound, established_outbound};

const NOW_SECONDS: u64 = 1_000;

/// Builds a `DestinationIdentity` for the "client" and the matching
/// [`DestinationPublic`] the runtime is allowed to see.
fn client_identity_and_public(seed: u64) -> (DestinationIdentity, DestinationPublic) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let identity = DestinationIdentity::generate(&mut rng).expect("identity");
    let public = DestinationPublic::from_destination(identity.destination().clone())
        .expect("destination public");
    (identity, public)
}

/// Builds an [`InboundDecryptionCapability`] whose secret matches the
/// supplied destination's static X25519 public key. Used to model the
/// "client supplied the matching decryption key" path of Plan 166 §6.
fn capability_for(identity: &DestinationIdentity) -> InboundDecryptionCapability {
    InboundDecryptionCapability::from_secret_bytes(
        identity.static_public_bytes(),
        *identity.static_secret_bytes(),
    )
}

/// One usable client-owned destination runtime with one inbound
/// tunnel registered and one outbound tunnel ready.
fn usable_client_runtime(
    seed: u64,
) -> (DestinationIdentity, DestinationPublic, DestinationRuntime) {
    let (identity, public) = client_identity_and_public(seed);
    let config = DestinationConfig::balanced();
    let mut runtime =
        DestinationRuntime::new_client_owned(public.clone(), config).expect("client runtime");
    runtime
        .admit_inbound(established_inbound(seed.wrapping_mul(3) + 1), NOW_SECONDS)
        .expect("inbound");
    runtime
        .admit_outbound(established_outbound(seed.wrapping_mul(3) + 2), NOW_SECONDS)
        .expect("outbound");
    (identity, public, runtime)
}

#[test]
fn client_owned_runtime_carries_no_signing_secret() {
    let (_identity, _public, runtime) = usable_client_runtime(1);
    assert_eq!(runtime.ownership(), DestinationOwnership::ClientOwned);
    assert!(runtime.identity().is_none(), "no private identity");
    assert!(runtime.identity_arc().is_none(), "no shared private arc");
    // The static X25519 public bytes are still observable.
    let pool_public = runtime.static_public_bytes();
    assert_eq!(pool_public.len(), i2pr_crypto::X25519_KEY_LENGTH);
    // No decryption capability yet, so no static secret is held.
    assert!(runtime.static_secret_bytes().is_none());
    assert!(runtime.decryption_capability().is_none());
}

#[test]
fn legacy_elgamal_destination_is_accepted_but_cannot_install() {
    // Plan 170 policy relocation: the Destination encryption-key slot
    // is an I2P legacy field that every unmodified client (Java I2P
    // 2.13.0, go-i2cp, i2pd) still populates with ElGamal even for
    // X25519 LeaseSet2 sessions. The public wrapper accepts it (with
    // a zeroed static slot, since the legacy 256-byte field carries
    // no X25519 key) so independent-client sessions can register.
    // Plan 172 §11: for ElGamal-slot destinations the early
    // capability-vs-destination pre-check is skipped (the slot has
    // been unused since 2005); X25519 enforcement lives in
    // `install_external` as the LS2-key-vs-capability match plus
    // lease ownership. A foreign/empty-pool LS2 therefore still
    // fails closed, but with pool/lease validation rather than the
    // pre-check mismatch.
    let signing = i2pr_crypto::SigningPrivateKey::from_bytes([0x99_u8; 32]);
    let signing_public = signing.public_key().expect("signing public");
    let public_key =
        i2pr_proto::PublicKey::new(i2pr_proto::CryptoKeyType::ElGamal, vec![0xCC; 256])
            .expect("public");
    let cert = i2pr_proto::Certificate::Key(
        i2pr_proto::KeyCertificate::for_types(
            i2pr_proto::SigningKeyType::EdDsaSha512Ed25519,
            i2pr_proto::CryptoKeyType::ElGamal,
        )
        .expect("cert"),
    );
    let dest = i2pr_proto::Destination::new(
        i2pr_proto::KeyAndCert::new(public_key, signing_public, vec![0x33; 96], cert).expect("kc"),
    )
    .expect("dest");
    let public = DestinationPublic::from_destination(dest).expect("legacy slot accepted");
    assert_eq!(
        public.encryption_public_key_type(),
        i2pr_proto::CryptoKeyType::ElGamal
    );
    // A foreign LS2 against an empty pool stays fail-closed via pool
    // validation (no usable inbound tunnels), not via the skipped
    // ElGamal pre-check. Build a well-formed LS2 from an unrelated
    // X25519 identity to prove the rejection is fail-closed.
    let (identity, _helper_public, helper_runtime) = usable_client_runtime(3);
    let helper_leases = helper_runtime.inbound_lease_sources(NOW_SECONDS);
    let record =
        build_signed_lease_set2(&identity, &helper_leases, NOW_SECONDS.try_into().unwrap())
            .expect("helper LS2");
    let capability = InboundDecryptionCapability::from_secret_bytes([0x42_u8; 32], [0x43_u8; 32]);
    let config = DestinationConfig::balanced();
    let mut runtime = DestinationRuntime::new_client_owned(public, config).expect("client runtime");
    let error = runtime
        .install_client_lease_set2(record, capability, NOW_SECONDS)
        .expect_err("legacy slot install stays fail-closed");
    assert!(matches!(
        error,
        LeaseSetError::NoUsableInboundTunnels
            | LeaseSetError::ForeignLease { .. }
            | LeaseSetError::SelfValidation(_)
    ));
}

#[test]
fn install_client_lease_set2_with_matching_decryption_key_commits_atomically() {
    let (identity, public, mut runtime) = usable_client_runtime(3);
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    assert_eq!(leases.len(), 1);
    let record = build_signed_lease_set2(&identity, &leases, NOW_SECONDS.try_into().unwrap())
        .expect("client-signed LS2");
    // Verify the signature against the embedded destination's signing
    // public key before installing.
    verify_lease_set2(&record).expect("client signature verifies");
    let capability = capability_for(&identity);
    runtime
        .install_client_lease_set2(record, capability, NOW_SECONDS)
        .expect("install");
    let client_ls2 = runtime
        .client_lease_set()
        .expect("client lease set installed");
    assert_eq!(client_ls2.key(), public.id().as_netdb_key());
    assert_eq!(
        client_ls2.lease_set2().leases().len(),
        runtime.inbound_lease_sources(NOW_SECONDS).len()
    );
    assert!(runtime.decryption_capability().is_some());
    assert!(runtime.static_secret_bytes().is_some());
    let summary = runtime.lease_set_summary();
    assert!(summary.present);
    assert_eq!(summary.lease_count, 1);
}

#[test]
fn install_with_mismatched_decryption_key_is_rejected_atomically() {
    let (identity, _public, mut runtime) = usable_client_runtime(4);
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let record = build_signed_lease_set2(&identity, &leases, NOW_SECONDS.try_into().unwrap())
        .expect("client-signed LS2");
    // Build a destination with a different static key and reuse its
    // X25519 secret for the capability; the install must reject the
    // mismatched public key without mutating any state.
    let (other_identity, _other_public) = client_identity_and_public(404);
    let bad_capability = InboundDecryptionCapability::from_secret_bytes(
        other_identity.static_public_bytes(),
        *other_identity.static_secret_bytes(),
    );
    let error = runtime
        .install_client_lease_set2(record, bad_capability, NOW_SECONDS)
        .expect_err("decryption mismatch rejected");
    assert!(matches!(
        error,
        LeaseSetError::Identity(DestinationIdentityError::DecryptionCapabilityKeyMismatch)
    ));
    assert!(runtime.client_lease_set().is_none());
    assert!(runtime.decryption_capability().is_none());
}

#[test]
fn install_with_foreign_lease_is_rejected() {
    let (identity, _public, mut runtime) = usable_client_runtime(5);
    // Build an LS2 that advertises a lease the runtime does NOT own.
    let foreign_source = {
        let mut pool = i2pr_client::DestinationTunnelPool::new(DestinationConfig::balanced())
            .expect("foreign pool");
        pool.register_inbound(established_inbound(99), NOW_SECONDS)
            .expect("foreign inbound");
        pool.inbound_lease_sources(NOW_SECONDS).remove(0)
    };
    let record = build_signed_lease_set2(
        &identity,
        std::slice::from_ref(&foreign_source),
        NOW_SECONDS.try_into().unwrap(),
    )
    .expect("foreign lease set");
    let capability = capability_for(&identity);
    let error = runtime
        .install_client_lease_set2(record, capability, NOW_SECONDS)
        .expect_err("foreign lease rejected");
    assert!(matches!(error, LeaseSetError::ForeignLease { .. }));
    // No state mutated.
    assert!(runtime.client_lease_set().is_none());
    assert!(runtime.decryption_capability().is_none());
}

#[test]
fn install_with_unknown_pool_lease_is_rejected_after_eviction() {
    let (identity, _public, mut runtime) = usable_client_runtime(6);
    // Capture the lease source for tunnel A, then admit a fresh
    // tunnel B and evict A. The pool still satisfies the minimum
    // usable-inbound ceiling so install proceeds to the lease-ownership
    // check.
    let leases_a = runtime.inbound_lease_sources(NOW_SECONDS);
    let evicted = leases_a[0];
    runtime
        .admit_inbound(
            established_inbound(602),
            NOW_SECONDS + u64::from(DestinationConfig::balanced().tunnel_lifetime_seconds()) / 2,
        )
        .expect("inbound two");
    runtime.pool_mut().remove(evicted.slot());
    let leases_after = runtime.inbound_lease_sources(NOW_SECONDS);
    assert_eq!(leases_after.len(), 1);
    assert_ne!(leases_after[0].slot(), evicted.slot());
    // Build an LS2 referencing the evicted lease. The runtime must
    // reject it as foreign.
    let record = build_signed_lease_set2(
        &identity,
        std::slice::from_ref(&evicted),
        NOW_SECONDS.try_into().unwrap(),
    )
    .expect("client-signed LS2");
    let capability = capability_for(&identity);
    let error = runtime
        .install_client_lease_set2(record, capability, NOW_SECONDS)
        .expect_err("foreign lease rejected");
    assert!(matches!(error, LeaseSetError::ForeignLease { .. }));
}

#[test]
fn install_with_duplicate_leases_is_rejected() {
    let (identity, _public, mut runtime) = usable_client_runtime(7);
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let first = leases[0];
    // Build a two-lease LS2 where both leases point at the same
    // gateway + tunnel id. `build_signed_lease_set2` does not permit
    // duplicates; craft the LS2 directly.
    let lease2 = Lease2::new(
        first.gateway(),
        first.gateway_receive_tunnel_id(),
        i2pr_proto::Date32::from_seconds(
            u32::try_from(first.advertised_expires_seconds()).unwrap(),
        ),
    );
    let (header, encryption_keys, signed_region) =
        build_unsigned(&identity, &[first], NOW_SECONDS.try_into().unwrap());
    let signed = identity.sign(&signed_region).expect("sign");
    let duplicate = LeaseSet2::new(
        header,
        i2pr_proto::Mapping::empty(),
        encryption_keys,
        vec![lease2, lease2],
        signed,
    )
    .expect("ls2");
    let capability = capability_for(&identity);
    let error = runtime
        .install_client_lease_set2(duplicate, capability, NOW_SECONDS)
        .expect_err("duplicate lease rejected");
    assert!(matches!(error, LeaseSetError::DuplicateLease { .. }));
    assert!(runtime.client_lease_set().is_none());
}

#[test]
fn lease_set_rotation_requests_refresh_instead_of_signing_locally() {
    let (identity, _public, mut runtime) = usable_client_runtime(8);
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let record = build_signed_lease_set2(&identity, &leases, NOW_SECONDS.try_into().unwrap())
        .expect("client-signed LS2");
    runtime
        .install_client_lease_set2(record, capability_for(&identity), NOW_SECONDS)
        .expect("install");
    // Advance the deterministic clock past the configured rotation
    // margin; the lifecycle must emit a RequestClientRefresh decision
    // rather than synthesizing a replacement.
    let lifetime = u64::from(DestinationConfig::balanced().tunnel_lifetime_seconds());
    let rotation_margin = u64::from(DestinationConfig::balanced().lease_rotation_margin_seconds());
    let late = NOW_SECONDS + lifetime - rotation_margin;
    let decision = runtime.refresh_lease_set(late).expect("refresh");
    assert!(matches!(
        decision,
        i2pr_client::LeaseSetDecision::RequestClientRefresh(
            i2pr_client::ClientRefreshCause::ApproachingExpiry
        )
    ));
    // The runtime must still report no router-signed lease set.
    assert!(runtime.lease_set().is_none());
    // The Plan 167 daemon pulls the request material from the runtime.
    let request = runtime
        .take_client_refresh_request(late)
        .expect("take")
        .expect("request pending");
    assert!(!request.leases.is_empty());
    assert_eq!(
        request.cause,
        i2pr_client::ClientRefreshCause::ApproachingExpiry
    );
}

#[test]
fn refreshed_lease_set_replaces_old_one_through_install_only() {
    let (identity, _public, mut runtime) = usable_client_runtime(9);
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let initial = build_signed_lease_set2(&identity, &leases, NOW_SECONDS.try_into().unwrap())
        .expect("first");
    runtime
        .install_client_lease_set2(initial.clone(), capability_for(&identity), NOW_SECONDS)
        .expect("install first");
    let first_gen = runtime.lease_set_summary().generations;
    // Admitting another inbound tunnel changes the pool; install a
    // new LS2 reflecting the new set.
    let new_now =
        NOW_SECONDS + u64::from(DestinationConfig::balanced().tunnel_lifetime_seconds()) / 2;
    runtime
        .admit_inbound(established_inbound(901), new_now)
        .expect("inbound two");
    let leases2 = runtime.inbound_lease_sources(new_now);
    let second = build_signed_lease_set2(&identity, &leases2, NOW_SECONDS.try_into().unwrap())
        .expect("second");
    runtime
        .install_client_lease_set2(second, capability_for(&identity), NOW_SECONDS)
        .expect("install second");
    assert!(runtime.lease_set_summary().generations > first_gen);
    assert_eq!(
        runtime
            .client_lease_set()
            .expect("installed")
            .lease_set2()
            .leases()
            .len(),
        leases2.len()
    );
    let _ = initial;
}

#[test]
fn shutdown_releases_client_lease_set_and_decryption_capability() {
    let (identity, _public, mut runtime) = usable_client_runtime(10);
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let record = build_signed_lease_set2(&identity, &leases, NOW_SECONDS.try_into().unwrap())
        .expect("client-signed LS2");
    runtime
        .install_client_lease_set2(record, capability_for(&identity), NOW_SECONDS)
        .expect("install");
    assert!(runtime.decryption_capability().is_some());
    let shutdown = runtime.shutdown();
    assert_eq!(shutdown.released_pool_slots, 2);
    assert!(runtime.client_lease_set().is_none());
    assert!(runtime.decryption_capability().is_none());
    assert!(runtime.static_secret_bytes().is_none());
    // Idempotent.
    let again = runtime.shutdown();
    assert_eq!(again.released_pool_slots, 0);
}

#[test]
fn install_while_stopping_is_rejected() {
    let (identity, _public, mut runtime) = usable_client_runtime(11);
    runtime.request_shutdown();
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let record = build_signed_lease_set2(&identity, &leases, NOW_SECONDS.try_into().unwrap())
        .expect("client-signed LS2");
    let error = runtime
        .install_client_lease_set2(record, capability_for(&identity), NOW_SECONDS)
        .expect_err("stopping install rejected");
    assert!(matches!(error, LeaseSetError::InstallWhileStopping));
}

#[test]
fn router_owned_runtime_rejects_client_install() {
    let mut rng = ChaCha8Rng::seed_from_u64(12);
    let identity = DestinationIdentity::generate(&mut rng).expect("router-owned identity");
    let mut runtime =
        DestinationRuntime::new(identity, DestinationConfig::balanced()).expect("runtime");
    runtime
        .admit_inbound(established_inbound(121), NOW_SECONDS)
        .expect("inbound");
    runtime
        .admit_outbound(established_outbound(122), NOW_SECONDS)
        .expect("outbound");
    let leases = runtime.inbound_lease_sources(NOW_SECONDS);
    let record = build_signed_lease_set2(
        runtime.identity().expect("router-owned"),
        &leases,
        NOW_SECONDS.try_into().unwrap(),
    )
    .expect("ls2");
    let (_, public) = client_identity_and_public(12_001);
    let capability = InboundDecryptionCapability::from_secret_bytes(
        *public.static_public_bytes(),
        *runtime.identity().expect("id").static_secret_bytes(),
    );
    let error = runtime
        .install_client_lease_set2(record, capability, NOW_SECONDS)
        .expect_err("router-owned install rejected");
    assert!(matches!(error, LeaseSetError::InstallWhileStopping));
}

// ---------------------------------------------------------------------------
// Internal helper: signs a one-lease unsigned LS2 so the duplicate-lease
// test can re-attach the same lease twice. `build_signed_lease_set2`
// rejects duplicates at the construction layer; we go through the
// proto layer directly to exercise the runtime-side dedup check.
// ---------------------------------------------------------------------------
fn build_unsigned(
    identity: &DestinationIdentity,
    leases: &[i2pr_client::pool::InboundLeaseSource],
    published_seconds: u32,
) -> (
    i2pr_proto::LeaseSet2Header,
    Vec<i2pr_proto::LeaseSet2EncryptionKey>,
    Vec<u8>,
) {
    let mut lease2 = Vec::with_capacity(leases.len());
    let mut latest_expiry = published_seconds;
    for source in leases {
        let end = u32::try_from(source.advertised_expires_seconds()).expect("end fits");
        lease2.push(Lease2::new(
            source.gateway(),
            source.gateway_receive_tunnel_id(),
            i2pr_proto::Date32::from_seconds(end),
        ));
        latest_expiry = latest_expiry.max(end);
    }
    let offset =
        u16::try_from(latest_expiry.saturating_sub(published_seconds)).expect("offset fits");
    let encryption_keys = vec![
        i2pr_proto::LeaseSet2EncryptionKey::new(
            i2pr_crypto::ROUTER_CRYPTO_KEY_TYPE,
            identity.static_public_bytes().to_vec(),
        )
        .expect("key"),
    ];
    let placeholder = i2pr_proto::SignatureValue::new(
        i2pr_crypto::ROUTER_SIGNING_KEY_TYPE,
        vec![0_u8; i2pr_crypto::SIGNATURE_LENGTH],
    )
    .expect("placeholder");
    let header = i2pr_proto::LeaseSet2Header::new(
        identity.destination().clone(),
        published_seconds,
        offset,
        i2pr_proto::LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let unsigned = LeaseSet2::new(
        header.clone(),
        i2pr_proto::Mapping::empty(),
        encryption_keys.clone(),
        lease2,
        placeholder,
    )
    .expect("unsigned ls2");
    let signed_region = unsigned.signature_preimage();
    (header, encryption_keys, signed_region)
}
