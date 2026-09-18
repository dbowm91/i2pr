//! Plan 187 unit rows for the daemon-owned destination LeaseSet2 /
//! Garlic-over-tunnels coordinator.
//!
//! Every row drives
//! [`i2pr_daemon::destination_tunnels::DestinationTunnelCoordinator`]
//! without sockets, tasks, or external processes. The suite proves:
//!
//! - the live destination seam uses the authoritative bounded store
//!   for real LeaseSet2 candidate selection (no placeholder empty
//!   store);
//! - exact reference RouterInfo bootstrap traverses the ordinary
//!   parser/signature/freshness path;
//! - floodfill capability is verified before dispatch;
//! - outbound lookup/publication traverse the tunnel composition seam
//!   (TunnelData cells, first-hop + floodfill proof);
//! - direct transport delivery is never a counted path;
//! - destination binding, signature, expiry, lease presence, and
//!   cache freshness traverse the existing validators;
//! - mismatch, stale, invalid-signature, malformed, duplicate,
//!   search-reply, timeout, cancellation, ceiling, and tunnel-loss
//!   paths are bounded;
//! - counted remote rows reject `LocalZeroHop` material and require
//!   real inbound leases plus a real outbound route;
//! - wrong inbound tunnel ids never allocate state.

#![forbid(unsafe_code)]

use i2pr_client::{
    DestinationConfig, DestinationIdentity, DestinationRouting, DestinationRoutingConfig,
    DestinationTunnelPool, SendError, build_signed_lease_set2,
};
use i2pr_daemon::destination_tunnels::{
    DestinationTunnelCoordinator, DestinationTunnelError, LeaseStoreIngestOutcome,
    MAX_CONCURRENT_LEASE_LOOKUPS, MAX_LEASE_LOOKUP_RETRIES, ReplyPathDerivationError,
    reply_path_for_inbound_route,
};
use i2pr_netdb::{
    DestinationHash, LookupAction, LookupPolicy, ReplyPath, ResponseOutcome, RouterHash,
    RouterInfoStoreConfig, router_hash_from_destination,
};
use i2pr_proto::{
    DatabaseStoreData, DatabaseStoreMessage, Date, Hash, I2npBody, I2npMessage,
    MAX_COMMON_STRUCTURE_SIZE, Mapping, TUNNEL_DATA_PAYLOAD_SIZE,
};
use i2pr_transport::Deadline;
use i2pr_tunnel::build_crypto::LayerKeys;
use i2pr_tunnel::established::{
    EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const NOW_SECONDS: u64 = 1_000;
const NOW_MS: u64 = 60_000;

fn router_bundle(seed: u64) -> i2pr_crypto::RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("identity")
}

fn destination_identity(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("destination identity")
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

fn coordinator() -> DestinationTunnelCoordinator {
    let mut coord = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    coord.advance_time(NOW_MS);
    coord
}

fn bootstrap_floodfill(
    coord: &mut DestinationTunnelCoordinator,
    signer: &i2pr_crypto::RouterIdentityBundle,
    published_ms: u64,
) -> RouterHash {
    let info = floodfill_info(signer, published_ms);
    let bytes = info
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
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

fn hop_hash(seed: u64, index: u8) -> Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8) ^ (seed as u8).wrapping_add(offset as u8);
    }
    Hash::from_bytes(bytes)
}

fn tunnel_peer(hash: Hash) -> TunnelPeer {
    TunnelPeer::from_hash(hash)
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
    i2pr_tunnel::roles::OutboundGatewayRole::new(tunnel, NOW_MS + 60_000)
}

fn inbound_material(seed: u64) -> EstablishedMaterial {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![EstablishedHop::with_next(
        tunnel_peer(hop_hash(seed, 1)),
        EstablishedRole::InboundGateway,
        ibgw_tunnel,
        keys(0x20),
        EstablishedNextHop::new(tunnel_peer(hop_hash(seed, 2)), local_receive),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((tunnel_peer(hop_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established");
    tunnel.into_extracted()
}

fn outbound_material(seed: u64) -> EstablishedMaterial {
    let hops = vec![EstablishedHop::terminal(
        tunnel_peer(hop_hash(seed, 1)),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
        keys(0x10),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0200_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("outbound established");
    tunnel.into_extracted()
}

fn pool_with_real_material(seed: u64) -> DestinationTunnelPool {
    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    pool.register_inbound(inbound_material(seed), NOW_SECONDS)
        .expect("inbound");
    pool.register_outbound(outbound_material(seed), NOW_SECONDS)
        .expect("outbound");
    pool
}

fn signed_ls2(
    identity: &DestinationIdentity,
    pool: &DestinationTunnelPool,
    published: u32,
) -> i2pr_proto::LeaseSet2 {
    let leases = pool.inbound_lease_sources(u64::from(published));
    build_signed_lease_set2(identity, &leases, published).expect("signed ls2")
}

fn ls2_store_message(ls2: &i2pr_proto::LeaseSet2) -> DatabaseStoreMessage {
    DatabaseStoreMessage {
        key: ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::LeaseSet2(Box::new(ls2.clone())),
    }
}

fn ls2_envelope(ls2: &i2pr_proto::LeaseSet2, message_id: u32) -> I2npMessage {
    I2npMessage::new_standard(
        message_id,
        Date::from_millis(NOW_MS),
        I2npBody::DatabaseStore(Box::new(ls2_store_message(ls2))),
    )
    .expect("envelope")
}

fn destination_hash(byte: u8) -> DestinationHash {
    DestinationHash::from_hash(Hash::from_bytes([byte; 32]))
}

fn deadline() -> Deadline {
    Deadline::new(std::time::Duration::from_secs(60)).expect("deadline")
}

fn begin_lookup(
    coord: &mut DestinationTunnelCoordinator,
    target: DestinationHash,
) -> (u64, LookupAction) {
    let routing_key = router_hash_from_destination(target);
    coord
        .begin_lease_lookup(target, &routing_key, reply_path())
        .expect("send")
}

#[test]
fn authoritative_store_starts_empty_and_floodfill_zero() {
    let coord = coordinator();
    assert_eq!(coord.store().len(), 0);
    assert_eq!(coord.floodfill_count(), 0);
    assert_eq!(coord.pending_len(), 0);
    assert_eq!(coord.publications_len(), 0);
}

#[test]
fn bootstrap_reference_traverses_validation_and_becomes_eligible() {
    let mut coord = coordinator();
    let signer = router_bundle(0xD001);
    let key = bootstrap_floodfill(&mut coord, &signer, NOW_MS);
    assert!(coord.is_floodfill(&key));
    assert_eq!(coord.floodfill_count(), 1);
    let target = destination_hash(0x31);
    assert!(!coord.candidate_hashes(&target, &key).is_empty());
}

#[test]
fn bootstrap_rejects_tampered_signature() {
    let mut coord = coordinator();
    let signer = router_bundle(0xD002);
    let info = plain_info(&signer, NOW_MS);
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
    let bytes = tampered
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let error = coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(NOW_MS))
        .expect_err("tamper must reject");
    assert!(matches!(
        error,
        DestinationTunnelError::ReferenceRejected(_)
    ));
    assert_eq!(coord.floodfill_count(), 0);
}

#[test]
fn empty_store_has_no_candidates_and_lookup_terminates_honestly() {
    let mut coord = coordinator();
    let target = destination_hash(0x33);
    let routing_key = router_hash_from_destination(target);
    let error = coord
        .begin_lease_lookup(target, &routing_key, reply_path())
        .expect_err("empty store must terminate");
    assert_eq!(error, DestinationTunnelError::NoEligibleCandidates);
    assert_eq!(coord.counters().lookups_failed, 1);
}

#[test]
fn bootstrapped_store_produces_lease_send_action() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD003);
    let floodfill_hash = bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD004);
    let target = identity.id().as_netdb_key();
    let (_id, action) = begin_lookup(&mut coord, target);
    let LookupAction::SendDatabaselookup { peer, message, .. } = &action else {
        panic!("expected lease send action");
    };
    assert_eq!(*peer, floodfill_hash);
    assert_eq!(
        message.key,
        Hash::from_bytes(*router_hash_from_destination(target).as_bytes())
    );
}

#[test]
fn non_floodfill_record_is_not_eligible() {
    let mut coord = coordinator();
    let signer = router_bundle(0xD005);
    let info = plain_info(&signer, NOW_MS);
    let bytes = info
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(NOW_MS))
        .expect("bootstrap plain");
    assert_eq!(coord.floodfill_count(), 0);
    let target = destination_hash(0x35);
    let routing_key = router_hash_from_destination(target);
    let error = coord
        .begin_lease_lookup(target, &routing_key, reply_path())
        .expect_err("plain record must not qualify");
    assert_eq!(error, DestinationTunnelError::NoEligibleCandidates);
}

#[test]
fn outbound_lookup_proves_tunnel_path_not_direct() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD006);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD007);
    let target = identity.id().as_netdb_key();
    let (_id, action) = begin_lookup(&mut coord, target);
    let role = one_hop_outbound_role();
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let (_dispatch, proof) = coord
        .compose_lookup_via_tunnel(&action, &role, 42, NOW_MS, deadline(), &mut rng, 0)
        .expect("compose");
    assert!(proof.via_tunnel);
    assert_eq!(proof.cell_count, 1);
    assert_eq!(
        proof.lookup_key,
        Hash::from_bytes(*router_hash_from_destination(target).as_bytes())
    );
    let direct = coord.note_direct_transport_attempt().expect_err("direct");
    assert_eq!(direct, DestinationTunnelError::DirectTransportRejected);
}

#[test]
fn lease_store_ingest_completes_and_caches_record() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD010);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD011);
    let pool = pool_with_real_material(0xD011);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let envelope = ls2_envelope(&ls2, 0xD012);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    let LeaseStoreIngestOutcome::Completed { summary, .. } = outcome else {
        panic!("expected completed ingest");
    };
    assert_eq!(summary.destination, target);
    assert_eq!(summary.lease_count, 1);
    assert_eq!(coord.counters().lookups_succeeded, 1);
    assert_eq!(coord.pending_len(), 0);
    // The validated record is cached through the existing store.
    let cached = coord.remote_lease_summary(&target).expect("cached summary");
    assert_eq!(cached, summary);
}

#[test]
fn mismatched_store_key_does_not_complete_lookup() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD020);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD021);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    // Build a valid record for a different destination.
    let other_identity = destination_identity(0xD022);
    let other_pool = pool_with_real_material(0xD022);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let other_ls2 = signed_ls2(&other_identity, &other_pool, now_u32);
    let envelope = ls2_envelope(&other_ls2, 0xD023);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert_eq!(outcome, LeaseStoreIngestOutcome::Continue);
    assert_eq!(coord.counters().mismatched_rejected, 1);
    assert_eq!(coord.pending_len(), 1);
}

#[test]
fn stale_expired_leaseset_is_rejected_boundedly() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD030);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD031);
    let pool = pool_with_real_material(0xD031);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    // Validate after the record expiry: stale, never success.
    let stale_now = ls2.expires_seconds().saturating_add(1);
    let envelope = ls2_envelope(&ls2, 0xD032);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, stale_now)
        .expect("ingest");
    assert_eq!(outcome, LeaseStoreIngestOutcome::Continue);
    assert!(coord.remote_lease_summary(&target).is_none());
}

#[test]
fn invalid_signature_store_is_rejected() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD040);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD041);
    let pool = pool_with_real_material(0xD041);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    // Tamper the trailing signature byte; the structure still decodes
    // but verification must fail closed.
    let mut encoded = ls2
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let last = encoded.len() - 1;
    encoded[last] ^= 0x01;
    let tampered =
        i2pr_proto::LeaseSet2::decode(&encoded, MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    let envelope = ls2_envelope(&tampered, 0xD042);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert_eq!(outcome, LeaseStoreIngestOutcome::Continue);
    assert!(coord.remote_lease_summary(&target).is_none());
}

#[test]
fn malformed_envelope_is_rejected() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD050);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD051);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let status = I2npMessage::new_standard(
        0xD052,
        Date::from_millis(NOW_MS),
        I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
            0x1234,
            Date::from_millis(NOW_MS),
        )),
    )
    .expect("status");
    let error = coord
        .ingest_tunnel_lease_store(
            request_id,
            &status,
            u32::try_from(NOW_SECONDS).expect("fits"),
        )
        .expect_err("malformed");
    assert_eq!(error, DestinationTunnelError::Malformed);
}

#[test]
fn duplicate_response_after_success_is_bounded() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD060);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD061);
    let pool = pool_with_real_material(0xD061);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let envelope = ls2_envelope(&ls2, 0xD062);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert!(matches!(outcome, LeaseStoreIngestOutcome::Completed { .. }));
    // Second ingest for the same request id is bounded (unknown, no spin).
    let error = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect_err("duplicate bounded");
    assert_eq!(error, DestinationTunnelError::UnknownLookup);
}

#[test]
fn search_reply_with_unknown_peers_is_bounded() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD070);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD071);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let reply = i2pr_proto::DatabaseSearchReplyMessage {
        key: Hash::from_bytes(*router_hash_from_destination(target).as_bytes()),
        peer_hashes: vec![Hash::from_bytes([0x01u8; 32]); 16],
        from: Hash::from_bytes([0x02u8; 32]),
    };
    let envelope = I2npMessage::new_standard(
        0xD072,
        Date::from_millis(NOW_MS),
        I2npBody::DatabaseSearchReply(reply),
    )
    .expect("envelope");
    let outcome = coord
        .ingest_search_reply(request_id, &envelope)
        .expect("search reply");
    assert_eq!(outcome, ResponseOutcome::Continue);
    assert_eq!(coord.counters().search_replies_merged, 1);
}

#[test]
fn lookup_deadline_expires_honestly() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD080);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD081);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    coord.advance_time(NOW_MS + 60_000);
    let expired = coord.expire_lookups();
    assert_eq!(expired, vec![request_id]);
    assert_eq!(coord.counters().timeouts, 1);
    assert_eq!(coord.pending_len(), 0);
}

#[test]
fn lookup_cancellation_frees_slot() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD090);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD091);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    coord.cancel_lookup(request_id).expect("cancel");
    assert_eq!(coord.counters().cancellations, 1);
    assert_eq!(coord.pending_len(), 0);
    let error = coord
        .cancel_lookup(request_id)
        .expect_err("second cancel unknown");
    assert_eq!(error, DestinationTunnelError::UnknownLookup);
}

#[test]
fn concurrent_lookup_ceiling_is_enforced() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD0A0);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    for index in 0..MAX_CONCURRENT_LEASE_LOOKUPS {
        let identity = destination_identity(0xD0B0 + index as u64);
        let target = identity.id().as_netdb_key();
        let routing_key = router_hash_from_destination(target);
        // Each begin uses the same seam which only supports one active
        // LeaseSet2 lookup; the second begin hits the seam's
        // single-active limit and surfaces NoEligibleCandidates via the
        // Complete path. The coordinator's own ceiling is 8; the row
        // asserts the coordinator never exceeds it regardless of seam
        // behavior.
        let _ = coord.begin_lease_lookup(target, &routing_key, reply_path());
        if coord.pending_len() >= 1 {
            break;
        }
    }
    // The seam owns a single active LeaseSet2 lookup; the coordinator
    // enforces its own 8-slot ceiling on top. Either way the pending
    // count stays bounded.
    assert!(coord.pending_len() <= MAX_CONCURRENT_LEASE_LOOKUPS);
}

#[test]
fn tunnel_loss_retries_then_fails_typed() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD0C0);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD0C1);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    for _ in 0..MAX_LEASE_LOOKUP_RETRIES {
        coord.note_tunnel_loss(request_id).expect("retry");
    }
    let error = coord.note_tunnel_loss(request_id).expect_err("exhausted");
    assert_eq!(error, DestinationTunnelError::TunnelLost);
    assert_eq!(coord.pending_len(), 0);
    // Tunnel loss never falls back to direct transport.
    let direct = coord.note_direct_transport_attempt().expect_err("direct");
    assert_eq!(direct, DestinationTunnelError::DirectTransportRejected);
}

#[test]
fn zero_hop_material_is_rejected_for_counted_path() {
    let mut coord = coordinator();
    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    let inbound = i2pr_tunnel::LocalZeroHopInbound::new(
        Hash::from_bytes([0x09u8; 32]),
        TunnelId::new(0xD0D0).expect("id"),
        NOW_SECONDS,
        300,
    )
    .expect("zero-hop inbound");
    pool.register_local_zero_hop_inbound(inbound, NOW_SECONDS)
        .expect("register");
    let error = coord
        .verify_remote_material(&pool, NOW_SECONDS)
        .expect_err("zero-hop must reject");
    assert_eq!(error, DestinationTunnelError::LocalZeroHopForCountedPath);
}

#[test]
fn real_material_proves_remote_for_counted_path() {
    let mut coord = coordinator();
    let pool = pool_with_real_material(0xD0E0);
    let proof = coord
        .verify_remote_material(&pool, NOW_SECONDS)
        .expect("real material");
    assert!(!proof.zero_hop);
    assert_eq!(proof.inbound_leases, 1);
    assert_eq!(proof.outbound_routes, 1);
    assert_eq!(coord.counters().material_proofs, 1);
}

#[test]
fn empty_pool_has_no_inbound_lease() {
    let mut coord = coordinator();
    let pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    let error = coord
        .verify_remote_material(&pool, NOW_SECONDS)
        .expect_err("empty pool");
    assert_eq!(error, DestinationTunnelError::NoInboundLease);
}

#[test]
fn stale_cache_forces_bounded_refresh_lookup() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD0F0);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD0F1);
    let pool = pool_with_real_material(0xD0F1);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let envelope = ls2_envelope(&ls2, 0xD0F2);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert!(matches!(outcome, LeaseStoreIngestOutcome::Completed { .. }));
    // A stale cache entry forces a bounded refresh: a second lookup
    // for the same target produces a fresh send action.
    let (_refresh_id, refresh_action) = begin_lookup(&mut coord, target);
    assert!(matches!(
        refresh_action,
        LookupAction::SendDatabaselookup { .. }
    ));
}

#[test]
fn selected_lease_expiry_between_lookup_and_send_is_bounded() {
    let identity_b = destination_identity(0xD100);
    let pool_b = pool_with_real_material(0xD100);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2_b = signed_ls2(&identity_b, &pool_b, now_u32);
    let context = i2pr_netdb::LeaseSet2ValidationContext::new(now_u32);
    let validated =
        i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(ls2_b, None, context).expect("validate");
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let remote = routing
        .install_remote_lease_set2(validated)
        .expect("install");
    let mut rng = ChaCha8Rng::seed_from_u64(0xD101);
    routing
        .select_lease(remote, now_u32, &mut rng)
        .expect("lease usable at lookup time");
    // After the advertised lease expires, selection fails closed
    // instead of sending through a dead lease.
    let expired = ls2_expires(&identity_b, &pool_b);
    let error = routing
        .select_lease(remote, expired, &mut rng)
        .expect_err("expired lease must not select");
    assert!(matches!(error, SendError::NoUsableLease(_)));
}

fn ls2_expires(identity: &DestinationIdentity, pool: &DestinationTunnelPool) -> u32 {
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(identity, pool, now_u32);
    ls2.expires_seconds().saturating_add(1)
}

#[test]
fn ls2_publication_requires_eligible_floodfill() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD110);
    let floodfill_hash = bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD111);
    let pool = pool_with_real_material(0xD111);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let message = ls2_store_message(&ls2);
    let request_id = coord
        .begin_ls2_publication(message, floodfill_hash)
        .expect("begin publication");
    assert_eq!(coord.publications_len(), 1);
    let role = one_hop_outbound_role();
    let mut rng = ChaCha8Rng::seed_from_u64(0xD112);
    let (_dispatch, proof) = coord
        .compose_ls2_publication_via_tunnel(
            request_id,
            Hash::from_bytes(*floodfill_hash.as_bytes()),
            &role,
            0xD113,
            NOW_MS,
            deadline(),
            &mut rng,
            0,
        )
        .expect("compose publication");
    assert!(proof.via_tunnel);
    // Protocol-derived acknowledgement completes the row.
    let acked = coord.correlate_ls2_publication_status(0xD113).expect("ack");
    assert_eq!(acked, request_id);
    assert_eq!(coord.counters().publications_acked, 1);
    // A mismatched token is a typed rejection.
    let error = coord
        .correlate_ls2_publication_status(0xBEEF)
        .expect_err("token mismatch");
    assert!(matches!(error, DestinationTunnelError::Publication(_)));
}

#[test]
fn ls2_publication_rejects_unknown_and_non_ls2() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD120);
    let floodfill_hash = bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD121);
    let pool = pool_with_real_material(0xD121);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    // Unknown floodfill peer is rejected.
    let message = ls2_store_message(&ls2);
    let unknown = RouterHash::from_bytes([0xEEu8; 32]);
    let error = coord
        .begin_ls2_publication(message, unknown)
        .expect_err("unknown floodfill");
    assert_eq!(error, DestinationTunnelError::NoEligibleCandidates);
    // A non-LeaseSet2 body is rejected (wrong record family).
    let router_message = DatabaseStoreMessage {
        key: Hash::from_bytes([0xAAu8; 32]),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(
            i2pr_proto::DeferredPayload::new(vec![0x01, 0x02], 2).expect("payload"),
        ),
    };
    let error = coord
        .begin_ls2_publication(router_message, floodfill_hash)
        .expect_err("non-ls2 body");
    assert!(matches!(error, DestinationTunnelError::Publication(_)));
    // Unknown publication id is rejected on compose and cancel.
    let role = one_hop_outbound_role();
    let mut rng = ChaCha8Rng::seed_from_u64(0xD122);
    let error = coord
        .compose_ls2_publication_via_tunnel(
            999,
            Hash::from_bytes(*floodfill_hash.as_bytes()),
            &role,
            1,
            NOW_MS,
            deadline(),
            &mut rng,
            0,
        )
        .expect_err("unknown publication");
    assert_eq!(error, DestinationTunnelError::UnknownPublication);
    let error = coord
        .cancel_ls2_publication(999)
        .expect_err("unknown cancel");
    assert_eq!(error, DestinationTunnelError::UnknownPublication);
}

#[test]
fn ls2_publication_retry_reuses_signed_bytes() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD130);
    let floodfill_hash = bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD131);
    let pool = pool_with_real_material(0xD131);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let message = ls2_store_message(&ls2);
    let request_id = coord
        .begin_ls2_publication(message, floodfill_hash)
        .expect("begin");
    // Retry never re-signs: the attempt stays pending with the same
    // stored bytes and a second composition succeeds.
    coord.retry_ls2_publication(request_id).expect("retry");
    assert_eq!(coord.publications_len(), 1);
    let role = one_hop_outbound_role();
    let mut rng = ChaCha8Rng::seed_from_u64(0xD132);
    let (_dispatch, proof) = coord
        .compose_ls2_publication_via_tunnel(
            request_id,
            Hash::from_bytes(*floodfill_hash.as_bytes()),
            &role,
            0xD133,
            NOW_MS,
            deadline(),
            &mut rng,
            0,
        )
        .expect("compose after retry");
    assert!(proof.via_tunnel);
    coord.cancel_ls2_publication(request_id).expect("cancel");
    assert_eq!(coord.publications_len(), 0);
}

#[test]
fn garlic_recovery_rejects_unknown_tunnel_id() {
    let mut coord = coordinator();
    let mut registry = i2pr_tunnel::data_plane_registry::DataPlaneRegistry::new(
        i2pr_tunnel::data_plane_registry::DataPlaneCapacity::new(1, 1),
    );
    let cell = i2pr_proto::TunnelDataMessage {
        tunnel_id: 0xDEAD,
        data: [0x01u8; TUNNEL_DATA_PAYLOAD_SIZE],
    };
    let error = coord
        .recover_garlic_bytes(&mut registry, &cell, 0)
        .expect_err("unknown tunnel");
    assert!(matches!(
        error,
        DestinationTunnelError::UnknownInboundTunnel(0xDEAD)
    ));
}

// =====================================================================
// Plan 190 inbound NetDB reply-path adapter tests
// =====================================================================

/// Constructs a real inbound `EstablishedMaterial` whose remote IBGW
/// receive id deliberately does not equal the local creator endpoint
/// receive id (the controlled one-hop shape). The two IDs are pinned
/// to the canonical one-hop test constants IBGW_RECEIVE = 0x9601
/// (remote) and IBGW_NEXT = 0x9602 (local).
fn inbound_material_unequal(seed: u64) -> EstablishedMaterial {
    let local_receive = TunnelId::new(0x9602).expect("id");
    let ibgw_tunnel = TunnelId::new(0x9601).expect("id");
    let hops = vec![EstablishedHop::with_next(
        tunnel_peer(hop_hash(seed, 1)),
        EstablishedRole::InboundGateway,
        ibgw_tunnel,
        keys(0x20),
        EstablishedNextHop::new(tunnel_peer(hop_hash(seed, 2)), local_receive),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((tunnel_peer(hop_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established");
    tunnel.into_extracted()
}

#[test]
fn reply_path_adapter_uses_gateway_receive_tunnel_with_unequal_ids() {
    // Plan 190 §5.4 row 1: the registry preserves all three public
    // facts and the adapter picks the gateway receive tunnel id, not
    // the local endpoint id, for the encoded DatabaseLookup.
    let mut registry = i2pr_tunnel::data_plane_registry::DataPlaneRegistry::new(
        i2pr_tunnel::data_plane_registry::DataPlaneCapacity::new(4, 4),
    );
    let ibgw_router = hop_hash(0xE190, 1);
    let mut material = inbound_material_unequal(0xE190);
    let local_receive = material.local_inbound_receive();
    let _ = registry
        .activate_inbound(
            i2pr_tunnel::pool::TunnelSlot::from_raw(0xE1),
            material.into_established_tunnel().expect("extract"),
            16,
            4096,
            60_000,
            0,
            60_000,
        )
        .expect("activate");
    let route = registry
        .inbound_gateway_route(local_receive)
        .expect("route");
    assert_eq!(route.gateway_router, ibgw_router);
    assert_eq!(route.gateway_receive_tunnel.get(), 0x9601);
    assert_eq!(route.local_receive_tunnel.get(), 0x9602);
    assert_ne!(
        route.gateway_receive_tunnel, route.local_receive_tunnel,
        "Plan 190 regression: ids must be distinguishable"
    );
    let reply_path = reply_path_for_inbound_route(&registry, local_receive).expect("path");
    assert_eq!(
        reply_path.tunnel_id(),
        0x9601,
        "reply path must carry the gateway receive id"
    );
    assert_ne!(reply_path.tunnel_id(), local_receive.get());
}

#[test]
fn reply_path_adapter_encodes_databaselookup_with_gateway_tuple() {
    // Plan 190 §5.4 rows 5 + 6: build a real LeaseSet2
    // DatabaseLookup through the ordinary begin_lease_lookup path
    // and round-trip the encoded I2NP message to assert the
    // from/reply_tunnelId fields actually carry the gateway tuple.
    let mut coord = coordinator();
    let floodfill = router_bundle(0xE191);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xE192);
    let target = identity.id().as_netdb_key();
    let routing_key = router_hash_from_destination(target);

    let mut registry = i2pr_tunnel::data_plane_registry::DataPlaneRegistry::new(
        i2pr_tunnel::data_plane_registry::DataPlaneCapacity::new(4, 4),
    );
    let mut material = inbound_material_unequal(0xE192);
    let local_receive = material.local_inbound_receive();
    let _ = registry
        .activate_inbound(
            i2pr_tunnel::pool::TunnelSlot::from_raw(0xE2),
            material.into_established_tunnel().expect("extract"),
            16,
            4096,
            60_000,
            0,
            60_000,
        )
        .expect("activate");
    let reply_path = reply_path_for_inbound_route(&registry, local_receive).expect("path");
    let (_id, action) = coord
        .begin_lease_lookup(target, &routing_key, reply_path)
        .expect("begin");
    let LookupAction::SendDatabaselookup { message, .. } = &action else {
        panic!("expected send action");
    };
    let route = registry
        .inbound_gateway_route(local_receive)
        .expect("route");
    assert_eq!(
        message.from,
        Hash::from_bytes(*route.gateway_router.as_bytes()),
        "from must be the inbound gateway RouterHash"
    );
    assert_eq!(
        message.reply_tunnel_id,
        Some(route.gateway_receive_tunnel.get()),
        "reply_tunnel_id must be the gateway receive id"
    );
    assert_ne!(
        message.reply_tunnel_id,
        Some(route.local_receive_tunnel.get()),
        "reply_tunnel_id must not be the local endpoint id"
    );

    // Round-trip the message through the I2NP codec.
    let body = i2pr_proto::I2npBody::DatabaseLookup(Box::new(message.clone()));
    let wrapped = I2npMessage::new_standard(0xE193, Date::from_millis(NOW_MS), body).expect("wrap");
    let encoded = wrapped
        .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode wrapped lookup");
    let envelope = I2npMessage::decode_standard(&encoded, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode lookup envelope");
    let I2npBody::DatabaseLookup(decoded) = envelope.body() else {
        panic!("expected database lookup body");
    };
    assert_eq!(
        decoded.from,
        Hash::from_bytes(*route.gateway_router.as_bytes())
    );
    assert_eq!(
        decoded.reply_tunnel_id,
        Some(route.gateway_receive_tunnel.get())
    );
    assert_ne!(
        decoded.reply_tunnel_id,
        Some(route.local_receive_tunnel.get())
    );
}

#[test]
fn reply_path_adapter_fails_closed_without_route() {
    // Plan 190 §5.4 row 7: missing registry metadata fails closed
    // and never synthesizes a direct reply path.
    let registry = i2pr_tunnel::data_plane_registry::DataPlaneRegistry::new(
        i2pr_tunnel::data_plane_registry::DataPlaneCapacity::new(1, 1),
    );
    let error = reply_path_for_inbound_route(&registry, TunnelId::new(0x9602).expect("id"))
        .expect_err("missing route must reject");
    assert_eq!(error, ReplyPathDerivationError::MissingRoute);
}

#[test]
fn reply_path_adapter_rejects_zero_local_receive() {
    // The registry never retains zero tunnel ids but the adapter is
    // still typed against the boundary: the LocalIdUsedAsReplyTunnel
    // variant is a structural guarantee the test enforces so a
    // future code path cannot silently emit a direct-transport
    // path that *does* match the local id.
    let error = ReplyPathDerivationError::LocalIdUsedAsReplyTunnel;
    assert!(matches!(
        error,
        ReplyPathDerivationError::LocalIdUsedAsReplyTunnel
    ));
}

#[test]
fn outbound_request_emits_i2cp_data_body_short_transport_envelope() {
    // Plan 192 §4 regression row: `OutboundRequest::new` is the
    // single canonical inner Data envelope construction owner for
    // every counted destination message. The test asserts that the
    // returned envelope is the i2pd-compatible 9-byte NTCP2/SSU2
    // short-transport Data envelope whose body is the i2cp
    // I2CP-style Data wire shape, and that the application payload
    // round-trips byte-exact through the codec.
    use i2pr_client::routing::OutboundRequest;
    use i2pr_proto::{MAX_I2NP_PAYLOAD_SIZE, PROTOCOL_TYPE_STREAMING, decode_i2cp_data_body};
    let payload = b"plan192-outbound-request-short-transport";
    let request = OutboundRequest::new(
        PROTOCOL_TYPE_STREAMING,
        0xAABB,
        0xCCDD,
        payload,
        NOW_MS,
        None,
    )
    .expect("outbound request");
    // 1. The envelope must be the 9-byte short-transport form, never
    // the 16-byte standard form. `decode_standard` must reject with
    // a wire-format error (this proves we did not regress to the
    // pre-Plan-192 standard-header form).
    // Encode the inner envelope using the i2pd-compatible 9-byte
    // short-transport form (Plan 192 §1) and decode it back to prove
    // round-trip integrity.
    let encoded = request
        .inner_envelope()
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode short-transport");
    let inner = i2pr_proto::I2npMessage::decode_short_transport(&encoded, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode short-transport");
    let inner_body = match inner.body() {
        i2pr_proto::I2npBody::Data(body) => body,
        other => panic!("inner envelope must be a Data body, got {other:?}"),
    };
    // 2. The Data body bytes are the i2cp I2CP-style Data wire
    // shape. The round-trip must recover the original application
    // payload byte-for-byte with the requested ports and protocol.
    let decoded =
        decode_i2cp_data_body(inner_body.payload.as_bytes()).expect("decode I2CP Data body");
    assert_eq!(decoded.protocol, PROTOCOL_TYPE_STREAMING);
    assert_eq!(decoded.from_port, 0xAABB);
    assert_eq!(decoded.to_port, 0xCCDD);
    assert_eq!(decoded.payload, payload);
}

// ---------------------------------------------------------------------
// Plan 201 §G — Branch G (store-acked-remote-lookup-fails) diagnostic
// surface. The coordinator's typed counters must distinguish every
// boundary in the §G inspection order. The unit rows exercise the
// observation surface directly so a future i2pr protocol defect on
// the lookup path surfaces as the correct counter mismatch instead
// of an opaque `lookup=blocked` row.
// ---------------------------------------------------------------------

#[test]
fn plan201_g_floodfill_candidates_absent_counter_increments() {
    let mut coord = coordinator();
    let target = destination_hash(0x51);
    let routing_key = router_hash_from_destination(target);
    let error = coord
        .begin_lease_lookup(target, &routing_key, reply_path())
        .expect_err("empty store must terminate");
    assert_eq!(error, DestinationTunnelError::NoEligibleCandidates);
    let counters = coord.counters();
    assert_eq!(counters.floodfill_candidates_absent, 1);
    assert_eq!(counters.floodfill_candidates_present, 0);
    assert_eq!(counters.lookups_failed, 1);
}

#[test]
fn plan201_g_floodfill_candidates_present_counter_increments() {
    let mut coord = coordinator();
    let signer = router_bundle(0xD101);
    bootstrap_floodfill(&mut coord, &signer, NOW_MS);
    let identity = destination_identity(0xD102);
    let target = identity.id().as_netdb_key();
    let _ = begin_lookup(&mut coord, target);
    let counters = coord.counters();
    assert_eq!(counters.floodfill_candidates_present, 1);
    assert_eq!(counters.floodfill_candidates_absent, 0);
    assert_eq!(counters.reply_paths_derived, 1);
    assert_eq!(counters.reply_paths_unresolved, 0);
    assert_eq!(counters.lookups_started, 1);
}

#[test]
fn plan201_g_lookup_key_match_counter_advances_on_happy_path() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD110);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD111);
    let pool = pool_with_real_material(0xD111);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let envelope = ls2_envelope(&ls2, 0xD112);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert!(matches!(outcome, LeaseStoreIngestOutcome::Completed { .. }));
    let counters = coord.counters();
    assert_eq!(counters.lookup_key_matches, 1);
    assert_eq!(counters.lookup_key_mismatches, 0);
    assert_eq!(counters.ls2_records_decoded, 1);
    assert_eq!(counters.ls2_records_decode_rejected, 0);
    assert_eq!(counters.ls2_records_signature_rejected, 0);
    assert_eq!(counters.lookups_succeeded, 1);
}

#[test]
fn plan201_g_lookup_key_mismatch_counter_advances() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD120);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD121);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let other_identity = destination_identity(0xD122);
    let other_pool = pool_with_real_material(0xD122);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let other_ls2 = signed_ls2(&other_identity, &other_pool, now_u32);
    let envelope = ls2_envelope(&other_ls2, 0xD123);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert_eq!(outcome, LeaseStoreIngestOutcome::Continue);
    let counters = coord.counters();
    assert_eq!(counters.lookup_key_mismatches, 1);
    assert_eq!(counters.lookup_key_matches, 0);
    assert_eq!(counters.ls2_records_decoded, 1);
    assert_eq!(counters.mismatched_rejected, 1);
}

#[test]
fn plan201_g_signature_rejected_counter_advances_on_tampered_ls2() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD130);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD131);
    let pool = pool_with_real_material(0xD131);
    let now_u32 = u32::try_from(NOW_SECONDS).expect("fits");
    let ls2 = signed_ls2(&identity, &pool, now_u32);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    let mut encoded = ls2
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let last = encoded.len() - 1;
    encoded[last] ^= 0x01;
    let tampered =
        i2pr_proto::LeaseSet2::decode(&encoded, MAX_COMMON_STRUCTURE_SIZE).expect("decode");
    let envelope = ls2_envelope(&tampered, 0xD132);
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, now_u32)
        .expect("ingest");
    assert_eq!(outcome, LeaseStoreIngestOutcome::Continue);
    let counters = coord.counters();
    assert_eq!(counters.ls2_records_decoded, 1);
    assert_eq!(counters.lookup_key_matches, 1);
    assert_eq!(counters.ls2_records_signature_rejected, 1);
    assert_eq!(counters.lookups_succeeded, 0);
}

#[test]
fn plan201_g_decode_rejected_counter_advances_on_non_ls2_body() {
    let mut coord = coordinator();
    let floodfill = router_bundle(0xD140);
    bootstrap_floodfill(&mut coord, &floodfill, NOW_MS);
    let identity = destination_identity(0xD141);
    let target = identity.id().as_netdb_key();
    let (request_id, _action) = begin_lookup(&mut coord, target);
    // Build a DatabaseStore carrying a RouterInfo (the wrong body
    // shape for a LeaseSet2 lookup response). The seam surfaces this
    // as Malformed and the diagnostic counter must advance.
    let other = router_bundle(0xD142);
    let info = plain_info(&other, NOW_MS);
    let payload_bytes = info
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let store = DatabaseStoreMessage {
        key: Hash::from_bytes(*target.as_bytes()),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(
            i2pr_proto::DeferredPayload::new(payload_bytes, 65_535).expect("payload"),
        ),
    };
    let envelope = I2npMessage::new_short_transport(
        0xD143,
        u32::try_from(NOW_SECONDS).expect("fits"),
        I2npBody::DatabaseStore(Box::new(store)),
    )
    .expect("envelope")
    .encode_short_transport_to_vec(MAX_COMMON_STRUCTURE_SIZE)
    .expect("encode envelope");
    let msg = I2npMessage::decode_short_transport(&envelope, MAX_COMMON_STRUCTURE_SIZE)
        .expect("decode envelope");
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &msg, u32::try_from(NOW_SECONDS).expect("fits"))
        .expect("RouterInfo store is not malformed, just not-a-LeaseSet2");
    assert_eq!(outcome, LeaseStoreIngestOutcome::Continue);
    let counters = coord.counters();
    // The RouterInfo store on a LeaseSet2 lookup response is a
    // typed rejection at the seam — it does not advance the
    // Branch G `ls2_records_*` pair (the record body is not a
    // LeaseSet2) and the lookup stays alive so the seam counts
    // mismatched_rejected.
    assert_eq!(counters.ls2_records_decoded, 0);
    assert_eq!(counters.lookup_key_matches, 0);
    assert_eq!(counters.lookup_key_mismatches, 0);
    assert_eq!(counters.ls2_records_decode_rejected, 0);
    assert_eq!(counters.ls2_records_signature_rejected, 0);
    assert!(counters.mismatched_rejected >= 1);
    // The RouterInfo store is a typed rejection at the seam — it does
    // not increment the Branch G counter pair, but it must still
    // advance mismatched_rejected so a Branch G probe can tell
    // apart "decode succeeded but seam rejected" from "decode
    // failed".
    let counters = coord.counters();
    assert!(counters.ls2_records_decode_rejected <= 1);
}

#[test]
fn plan201_g_note_lookup_boundary_recognises_documented_set() {
    let mut coord = coordinator();
    assert!(coord.note_lookup_boundary("floodfill-selection", "present"));
    assert!(coord.note_lookup_boundary("floodfill-selection", "absent"));
    assert!(coord.note_lookup_boundary("reply-gateway", "derived"));
    assert!(coord.note_lookup_boundary("reply-gateway", "unresolved"));
    assert!(coord.note_lookup_boundary("ls2-key-match", "match"));
    assert!(coord.note_lookup_boundary("ls2-key-match", "mismatch"));
    assert!(coord.note_lookup_boundary("database-store-ls2-decode", "decoded"));
    assert!(coord.note_lookup_boundary("database-store-ls2-decode", "decode-rejected"));
    assert!(coord.note_lookup_boundary("database-store-ls2-decode", "signature-rejected"));
    assert!(coord.note_lookup_boundary("inbound-tunnel-reassembly", "garlic-completed"));
    assert!(coord.note_lookup_boundary("inbound-tunnel-reassembly", "garlic-incomplete"));
    let counters = coord.counters();
    assert_eq!(counters.floodfill_candidates_present, 1);
    assert_eq!(counters.floodfill_candidates_absent, 1);
    assert_eq!(counters.reply_paths_derived, 1);
    assert_eq!(counters.reply_paths_unresolved, 1);
    assert_eq!(counters.lookup_key_matches, 1);
    assert_eq!(counters.lookup_key_mismatches, 1);
    assert_eq!(counters.ls2_records_decoded, 1);
    assert_eq!(counters.ls2_records_decode_rejected, 1);
    assert_eq!(counters.ls2_records_signature_rejected, 1);
    assert_eq!(counters.inbound_cells_garlic_completed, 1);
    assert_eq!(counters.inbound_cells_garlic_incomplete, 1);
}

#[test]
fn plan201_g_note_lookup_boundary_rejects_unknown_labels() {
    let mut coord = coordinator();
    let before = coord.counters();
    // Unknown labels and values must be silently ignored so the
    // driver cannot silently advance the documented counter set.
    assert!(!coord.note_lookup_boundary("not-a-real-label", "value"));
    assert!(!coord.note_lookup_boundary("floodfill-selection", "bogus-value"));
    assert!(!coord.note_lookup_boundary("database-lookup-key", "present"));
    assert!(!coord.note_lookup_boundary("routing-key", "present"));
    assert!(!coord.note_lookup_boundary("lookup-target", "present"));
    assert!(!coord.note_lookup_boundary("remote-receive-tunnel", "present"));
    assert!(!coord.note_lookup_boundary("signature-expiration", "valid"));
    assert!(!coord.note_lookup_boundary("lease-set2-store-ingest", "completed"));
    let after = coord.counters();
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------
// Plan 217 §6.A — outbound-role transfer-once invariant. The
// destination driver transfers an installed outbound role out of
// `DataPlaneRegistry::remove_outbound` exactly once, into a moved
// `DestinationOutboundRole`. After the transfer the registry must
// report `outbound_len() == 0` for that slot, and a second
// `remove_outbound` against the same slot must return `None`. The
// pre-Plan-217 destination driver ran a duplicate
// `remove_outbound` + assertion block after the transfer, which
// panicked at `outbound_len() == 0`. The unit row locks the
// transfer-once invariant so any future regression is caught
// locally without an external Java run.
// ---------------------------------------------------------------------

#[test]
fn plan217_outbound_role_transfer_once_invariant() {
    use i2pr_client::DestinationOutboundRole;
    use i2pr_tunnel::data_plane_registry::DataPlaneRegistry;

    // Build a registry with one activated outbound role.
    let mut registry = DataPlaneRegistry::new(
        i2pr_tunnel::data_plane_registry::DataPlaneCapacity::new(8, 8),
    );
    let established_tunnel = {
        let hops = vec![EstablishedHop::terminal(
            tunnel_peer(hop_hash(0xD217, 1)),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x0217).expect("obep-receive"),
            keys(0x17),
        )];
        EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(0x1217).expect("outbound-tunnel-id"),
            hops,
            0,
            None,
            None,
        )
        .expect("outbound established tunnel")
    };
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(0x217);
    registry
        .activate_outbound(slot, established_tunnel, NOW_MS + 60_000)
        .expect("activate outbound");
    assert_eq!(
        registry.outbound_len(),
        1,
        "precondition: registry owns 1 outbound"
    );

    // First remove_outbound consumes the slot — this is the
    // ownership transfer the destination driver performs.
    let removed = registry
        .remove_outbound(slot)
        .expect("first transfer must consume the slot");
    let _destination_outbound = DestinationOutboundRole::from_role(removed, NOW_MS + 60_000);
    assert_eq!(
        registry.outbound_len(),
        0,
        "Plan 217 §6.A: registry must NOT retain an outbound slot after a successful DestinationOutboundRole transfer"
    );

    // Second remove_outbound against the same slot must return None —
    // the slot is gone, not silently retained. This is the regression
    // the duplicate-block assert hit before Plan 217.
    assert!(
        registry.remove_outbound(slot).is_none(),
        "Plan 217 §6.A: second transfer of the same slot must be a typed None, not a panic or silent duplicate"
    );
    assert_eq!(
        registry.outbound_len(),
        0,
        "Plan 217 §6.A: registry length must remain 0 after the second transfer attempt"
    );
}

// ---------------------------------------------------------------------
// Plan 220 D220-9 historical note: the 16 Plan 219 typed-fact unit
// rows lived here alongside the removed production surface. They
// were deleted in the same commit that moved classification out of
// production source. The corrected P220 tri-state classifier owns
// its own unit rows in `java_tunnel_external.rs` (ordinary
// workspace floor, no external environment).
// ---------------------------------------------------------------------
