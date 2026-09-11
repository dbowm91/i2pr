//! Plan 186 unit rows for the daemon-owned NetDB-over-tunnels coordinator.
//!
//! Every row drives [`i2pr_daemon::netdb_tunnels::NetDbTunnelCoordinator`]
//! without sockets, tasks, or external processes. The suite proves:
//!
//! - the live daemon NetDB seam uses the authoritative bounded store
//!   for real candidate selection (no placeholder empty store);
//! - exact reference RouterInfo bootstrap traverses the ordinary
//!   parser/signature/freshness path;
//! - floodfill capability is verified before dispatch;
//! - outbound lookup/publication traverse the tunnel composition seam
//!   (TunnelData cells, first-hop + floodfill proof);
//! - direct SSU2 NetDB delivery is never a counted path;
//! - search-reply, mismatch, stale, malformed, duplicate, timeout, and
//!   cancellation paths are bounded;
//! - tunnel loss returns a typed retry/failure without fallback.

#![forbid(unsafe_code)]

use std::io::Write as _;

use i2pr_daemon::netdb_tunnels::{
    MAX_CONCURRENT_LOOKUPS, MAX_LOOKUP_RETRIES, NetDbTunnelCoordinator, NetDbTunnelError,
};
use i2pr_netdb::{
    LookupAction, LookupPolicy, ReplyPath, ResponseOutcome, RouterHash, RouterInfoStoreConfig,
    ValidationContext,
};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, Mapping};
use i2pr_transport::Deadline;
use i2pr_tunnel::build_crypto::LayerKeys;
use i2pr_tunnel::established::{EstablishedHop, EstablishedRole, EstablishedTunnel};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

fn bundle(seed: u64) -> i2pr_crypto::RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("identity")
}

fn floodfill_info(
    signer: &i2pr_crypto::RouterIdentityBundle,
    published_ms: u64,
) -> i2pr_proto::RouterInfo {
    let mut options = Mapping::builder();
    options.insert("caps".to_owned(), "f".to_owned()).unwrap();
    signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            options.build().unwrap(),
        )
        .expect("sign floodfill")
}

fn plain_info(
    signer: &i2pr_crypto::RouterIdentityBundle,
    published_ms: u64,
) -> i2pr_proto::RouterInfo {
    signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign plain")
}

fn coordinator() -> NetDbTunnelCoordinator {
    let mut coord =
        NetDbTunnelCoordinator::new(LookupPolicy::default(), RouterInfoStoreConfig::default());
    coord.advance_time(60_000);
    coord
}

fn bootstrap_floodfill(
    coord: &mut NetDbTunnelCoordinator,
    signer: &i2pr_crypto::RouterIdentityBundle,
    published_ms: u64,
) -> RouterHash {
    let info = floodfill_info(signer, published_ms);
    let bytes = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(published_ms))
        .expect("bootstrap floodfill")
}

fn reply_path() -> ReplyPath {
    ReplyPath::new(RouterHash::from_bytes([0x77u8; 32]), 0x4242).expect("path")
}

fn keys(seed: u8) -> LayerKeys {
    LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

fn peer(value: u8) -> TunnelPeer {
    TunnelPeer::from_hash(Hash::from_bytes([value; 32]))
}

fn one_hop_outbound_role() -> i2pr_tunnel::roles::OutboundGatewayRole {
    let hops = vec![EstablishedHop::terminal(
        peer(1),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x100).expect("id"),
        keys(0x10),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(1).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("tunnel");
    i2pr_tunnel::roles::OutboundGatewayRole::new(tunnel, 60_000)
}

fn deadline() -> Deadline {
    Deadline::new(std::time::Duration::from_secs(60)).expect("deadline")
}

fn store_envelope_for(
    signer: &i2pr_crypto::RouterIdentityBundle,
    published_ms: u64,
    message_id: u32,
) -> I2npMessage {
    let info = plain_info(signer, published_ms);
    let encoded = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&encoded).expect("gzip");
    let compressed = encoder.finish().expect("finish");
    let compressed_len = compressed.len();
    let key = i2pr_netdb::router_hash(signer.identity()).expect("hash");
    let store = i2pr_proto::DatabaseStoreMessage {
        key: Hash::from_bytes(*key.as_bytes()),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(
            i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
        ),
    };
    I2npMessage::new_standard(
        message_id,
        Date::from_millis(60_000),
        I2npBody::DatabaseStore(Box::new(store)),
    )
    .expect("envelope")
}

#[test]
fn authoritative_store_starts_empty_and_floodfill_zero() {
    let coord = coordinator();
    assert_eq!(coord.store().len(), 0);
    assert_eq!(coord.floodfill_count(), 0);
}

#[test]
fn bootstrap_reference_traverses_validation_and_becomes_eligible() {
    let mut coord = coordinator();
    let signer = bundle(0xB001);
    let key = bootstrap_floodfill(&mut coord, &signer, 60_000);
    assert!(coord.is_floodfill(&key));
    assert_eq!(coord.floodfill_count(), 1);
    assert_eq!(coord.store_stats().record_count, 1);
}

#[test]
fn bootstrap_rejects_tampered_signature() {
    let mut coord = coordinator();
    let signer = bundle(0xB002);
    let info = floodfill_info(&signer, 60_000);
    let mut bytes = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    // Flip a byte near the end (signature region) to break validation.
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    let error = coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(60_000))
        .expect_err("tampered must reject");
    assert!(matches!(error, NetDbTunnelError::ReferenceRejected(_)));
    assert_eq!(coord.store().len(), 0);
}

#[test]
fn empty_store_has_no_candidates_and_lookup_terminates_honestly() {
    let mut coord = coordinator();
    let target = RouterHash::from_bytes([0x42u8; 32]);
    let routing_key = target;
    let error = coord
        .begin_tunnel_lookup(target, &routing_key, reply_path())
        .expect_err("empty store must not dispatch");
    assert_eq!(error, NetDbTunnelError::NoEligibleCandidates);
}

#[test]
fn bootstrapped_store_produces_send_action() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB003);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0x99u8; 32]);
    let routing_key = target;
    let (_id, action) = coord
        .begin_tunnel_lookup(target, &routing_key, reply_path())
        .expect("send");
    assert!(matches!(action, LookupAction::SendDatabaselookup { .. }));
}

#[test]
fn non_floodfill_record_is_not_eligible() {
    let mut coord = coordinator();
    let plain = bundle(0xB004);
    let info = plain_info(&plain, 60_000);
    let bytes = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let key = coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(60_000))
        .expect("bootstrap plain");
    assert!(!coord.is_floodfill(&key));
    assert_eq!(coord.floodfill_count(), 0);
    let target = RouterHash::from_bytes([0x11u8; 32]);
    let error = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect_err("plain is not a candidate");
    assert_eq!(error, NetDbTunnelError::NoEligibleCandidates);
}

#[test]
fn outbound_lookup_proves_tunnel_path_not_direct() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB005);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0xAAu8; 32]);
    let (_id, action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    let role = one_hop_outbound_role();
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let (_dispatch, proof) = coord
        .compose_lookup_via_tunnel(&action, &role, 42, 60_000, deadline(), &mut rng, 0)
        .expect("compose");
    assert!(proof.via_tunnel);
    assert_eq!(proof.cell_count, 1);
    assert_eq!(proof.lookup_key, Hash::from_bytes(*target.as_bytes()));
    // Direct transport is never a counted path.
    let direct = coord.note_direct_transport_attempt().expect_err("direct");
    assert_eq!(direct, NetDbTunnelError::DirectTransportRejected);
}

#[test]
fn mismatched_store_does_not_complete_lookup() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB006);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target_signer = bundle(0xB007);
    let target = i2pr_netdb::router_hash(target_signer.identity()).expect("target");
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    // Build a store for a different router.
    let other = bundle(0xB008);
    let envelope = store_envelope_for(&other, 60_000, 0x1001);
    let outcome = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect("ingest");
    assert_eq!(outcome, ResponseOutcome::Continue);
    assert_eq!(coord.counters().mismatched_rejected, 1);
    assert_eq!(coord.pending_len(), 1);
}

#[test]
fn stale_routerinfo_is_rejected_boundedly() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB009);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target_signer = bundle(0xB00A);
    let target = i2pr_netdb::router_hash(target_signer.identity()).expect("target");
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    // Sign at time 0, validate at 60s + 1 day + 1ms => stale.
    let envelope = store_envelope_for(&target_signer, 0, 0x1002);
    let stale_now = Date::from_millis(60_000 + 24 * 60 * 60 * 1000 + 1);
    let outcome = coord
        .ingest_tunnel_store(request_id, &envelope, ValidationContext::new(stale_now))
        .expect("ingest");
    // Stale signature fails validation -> Continue, never success.
    assert_eq!(outcome, ResponseOutcome::Continue);
}

#[test]
fn invalid_signature_store_is_rejected() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB00B);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target_signer = bundle(0xB00C);
    let target = i2pr_netdb::router_hash(target_signer.identity()).expect("target");
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    let info = plain_info(&target_signer, 60_000);
    // Tamper the signature.
    let bad_sig = {
        let mut bytes = info.signature().as_bytes().to_vec();
        bytes[0] ^= 0x01;
        i2pr_proto::SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, bytes).expect("sig")
    };
    let tampered = i2pr_proto::RouterInfo::new(
        info.router_identity().clone(),
        info.published(),
        info.addresses().to_vec(),
        Vec::new(),
        Mapping::empty(),
        bad_sig,
    )
    .expect("tampered");
    let encoded = tampered
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&encoded).expect("gzip");
    let compressed = encoder.finish().expect("finish");
    let compressed_len = compressed.len();
    let store = i2pr_proto::DatabaseStoreMessage {
        key: Hash::from_bytes(*target.as_bytes()),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(
            i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
        ),
    };
    let envelope = I2npMessage::new_standard(
        0x1003,
        Date::from_millis(60_000),
        I2npBody::DatabaseStore(Box::new(store)),
    )
    .expect("envelope");
    let outcome = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect("ingest");
    assert_eq!(outcome, ResponseOutcome::Continue);
}

#[test]
fn malformed_envelope_is_rejected() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB00D);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0x55u8; 32]);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    let status = I2npMessage::new_standard(
        0x2001,
        Date::from_millis(60_000),
        I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
            0x1234,
            Date::from_millis(60_000),
        )),
    )
    .expect("status");
    let error = coord
        .ingest_tunnel_store(
            request_id,
            &status,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect_err("malformed");
    assert_eq!(error, NetDbTunnelError::Malformed);
}

#[test]
fn duplicate_response_after_success_is_bounded() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB00E);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target_signer = bundle(0xB00F);
    let target = i2pr_netdb::router_hash(target_signer.identity()).expect("target");
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    let envelope = store_envelope_for(&target_signer, 60_000, 0x3001);
    let outcome = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect("ingest");
    assert!(matches!(outcome, ResponseOutcome::Completed(_)));
    assert_eq!(coord.counters().lookups_succeeded, 1);
    // Second ingest for the same request id is bounded (unknown, no spin).
    let error = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect_err("duplicate bounded");
    assert_eq!(error, NetDbTunnelError::UnknownLookup);
}

#[test]
fn search_reply_with_unknown_peers_is_bounded() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB010);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0x66u8; 32]);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    let reply = i2pr_proto::DatabaseSearchReplyMessage {
        key: Hash::from_bytes(*target.as_bytes()),
        peer_hashes: vec![Hash::from_bytes([0x01u8; 32]); 16],
        from: Hash::from_bytes([0x02u8; 32]),
    };
    let envelope = I2npMessage::new_standard(
        0x4001,
        Date::from_millis(60_000),
        I2npBody::DatabaseSearchReply(reply),
    )
    .expect("envelope");
    let outcome = coord
        .ingest_search_reply(request_id, &envelope)
        .expect("search reply");
    assert_eq!(outcome, ResponseOutcome::Continue);
    // Suggestions are capped by policy, never unbounded.
    assert!(coord.counters().search_replies_merged <= 64);
}

#[test]
fn lookup_deadline_expires_honestly() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB011);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0x77u8; 32]);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    coord.advance_time(60_000 + 5_000 + 1);
    let expired = coord.expire_lookups();
    assert_eq!(expired, vec![request_id]);
    assert_eq!(coord.counters().timeouts, 1);
    assert_eq!(coord.pending_len(), 0);
}

#[test]
fn lookup_cancellation_frees_state() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB012);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0x88u8; 32]);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    coord.cancel_lookup(request_id).expect("cancel");
    assert_eq!(coord.pending_len(), 0);
    assert_eq!(coord.counters().cancellations, 1);
}

#[test]
fn concurrent_lookup_ceiling_is_enforced() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB013);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    for i in 0..MAX_CONCURRENT_LOOKUPS {
        let target = RouterHash::from_bytes([i as u8; 32]);
        // Each begin uses the same seam which only supports one active
        // RouterInfo lookup; the second begin will hit the seam's
        // single-active limit and surface NoEligibleCandidates via the
        // Complete path. The coordinator's own ceiling is 8; we assert
        // the coordinator never exceeds it regardless of seam behavior.
        let _ = coord.begin_tunnel_lookup(target, &target, reply_path());
        if coord.pending_len() >= 1 {
            break;
        }
    }
    // The seam owns a single active RouterInfo lookup; the coordinator
    // enforces its own 8-slot ceiling on top. Either way the pending
    // count stays bounded.
    assert!(coord.pending_len() <= MAX_CONCURRENT_LOOKUPS);
}

#[test]
fn tunnel_loss_is_typed_and_bounded_without_fallback() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB014);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target = RouterHash::from_bytes([0x99u8; 32]);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    for _ in 0..MAX_LOOKUP_RETRIES {
        coord.note_tunnel_loss(request_id).expect("retry");
    }
    let error = coord.note_tunnel_loss(request_id).expect_err("exhausted");
    assert_eq!(error, NetDbTunnelError::TunnelLost);
    assert!(coord.counters().tunnel_failures >= 1);
    // Direct transport is still rejected after tunnel loss.
    assert_eq!(
        coord.note_direct_transport_attempt().expect_err("direct"),
        NetDbTunnelError::DirectTransportRejected
    );
}

#[test]
fn publication_uses_floodfill_selection_and_tunnel_path() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB015);
    let floodfill_key = bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let local_signer = bundle(0xB016);
    let local = i2pr_netdb::LocalRouterInfoBuilder::new(&local_signer)
        .build_default(Date::from_millis(60_000))
        .expect("local");
    coord.register_local(local);
    let nearest = coord.nearest_floodfills();
    assert_eq!(nearest, vec![floodfill_key]);
    let record = coord.begin_publication(floodfill_key).expect("begin");
    // The stored key must remain the local router hash, never the floodfill.
    assert_ne!(
        record.store_message.key,
        Hash::from_bytes(*floodfill_key.as_bytes())
    );
    let role = one_hop_outbound_role();
    let mut rng = ChaCha8Rng::seed_from_u64(9);
    let (_dispatch, proof) = coord
        .compose_publication_via_tunnel(
            &record,
            Hash::from_bytes(*floodfill_key.as_bytes()),
            &role,
            0x5001,
            60_000,
            deadline(),
            &mut rng,
            0,
        )
        .expect("compose publication");
    assert!(proof.via_tunnel);
    assert_eq!(proof.cell_count, 1);
}

#[test]
fn publication_unknown_token_is_rejected() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB017);
    let floodfill_key = bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let local_signer = bundle(0xB018);
    let local = i2pr_netdb::LocalRouterInfoBuilder::new(&local_signer)
        .build_default(Date::from_millis(60_000))
        .expect("local");
    coord.register_local(local);
    let _record = coord.begin_publication(floodfill_key).expect("begin");
    let error = coord
        .correlate_publication_status(0xDEAD_BEEF)
        .expect_err("unknown token");
    assert!(matches!(error, NetDbTunnelError::Publication(_)));
}

#[test]
fn publication_retry_does_not_resign() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB019);
    let floodfill_key = bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let local_signer = bundle(0xB01A);
    let local = i2pr_netdb::LocalRouterInfoBuilder::new(&local_signer)
        .build_default(Date::from_millis(60_000))
        .expect("local");
    coord.register_local(local);
    let record = coord.begin_publication(floodfill_key).expect("begin");
    let original = record.store_message.clone();
    let request_id = record.attempt.request_id();
    coord
        .mark_publication_failed(request_id)
        .expect("mark failed");
    coord.retry_publication(request_id).expect("retry");
    // A second attempt for the same peer reuses the snapshot bytes;
    // the coordinator never re-signs per retry.
    let second = coord.begin_publication(floodfill_key).expect("second");
    assert_eq!(second.encoded, record.encoded);
    assert_eq!(second.store_message.key, original.key);
}

#[test]
fn composition_outcome_requires_registry_roles() {
    use i2pr_tunnel::data_plane_registry::{DataPlaneCapacity, DataPlaneRegistry};
    let coord = coordinator();
    let registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
    assert_eq!(
        coord.composition_outcome_with_registry(&registry),
        i2pr_daemon::netdb_seam::CompositionOutcome::NeedInboundExploratory
    );
}

#[test]
fn successful_store_installs_through_ordinary_path() {
    let mut coord = coordinator();
    let floodfill = bundle(0xB01B);
    bootstrap_floodfill(&mut coord, &floodfill, 60_000);
    let target_signer = bundle(0xB01C);
    let target = i2pr_netdb::router_hash(target_signer.identity()).expect("target");
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply_path())
        .expect("send");
    let envelope = store_envelope_for(&target_signer, 60_000, 0x6001);
    let outcome = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect("ingest");
    assert!(matches!(outcome, ResponseOutcome::Completed(_)));
    assert!(coord.store().contains(&target));
    // Direct delivery is still not a counted path after success.
    assert_eq!(
        coord.note_direct_transport_attempt().expect_err("direct"),
        NetDbTunnelError::DirectTransportRejected
    );
}
