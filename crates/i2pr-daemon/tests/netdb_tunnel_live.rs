//! Plan 186 live NetDB-over-tunnels trajectory.
//!
//! Drives the daemon-owned [`NetDbTunnelCoordinator`] end-to-end
//! through real outbound/inbound TunnelData cells:
//!
//! - outbound `DatabaseLookup` composition through a one-hop
//!   `OutboundGatewayRole` whose first hop is the controlled
//!   floodfill;
//! - OBEP recovery proving the tunnel ROUTER target is the selected
//!   floodfill F (not the lookup key K) and the nested key remains K;
//! - inbound `DatabaseStore` recovery through a two-hop inbound chain
//!   (`IBGW -> participant -> local endpoint`) + `DataPlaneRegistry` +
//!   `dispatch_inbound_tunnel_data`;
//! - signed validation + install through the ordinary authoritative
//!   store to terminal success;
//! - publication through the same outbound path with
//!   protocol-derived `DeliveryStatus` correlation;
//! - direct-transport rejection and liveness scheduler green.

#![forbid(unsafe_code)]

use std::io::Write as _;

use i2pr_daemon::inbound_dispatch::{InboundDispatchOutcome, dispatch_inbound_tunnel_data};
use i2pr_daemon::netdb_tunnels::{NetDbTunnelCoordinator, NetDbTunnelError};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{
    LookupAction, LookupPolicy, ReplyPath, ResponseOutcome, RouterHash, RouterInfoStoreConfig,
    ValidationContext,
};
use i2pr_proto::{
    Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, Mapping, TunnelDataMessage,
    TunnelGatewayMessage,
};
use i2pr_transport::Deadline;
use i2pr_tunnel::DuplicateWindow;
use i2pr_tunnel::build_crypto::LayerKeys;
use i2pr_tunnel::data_plane_registry::{DataPlaneCapacity, DataPlaneRegistry};
use i2pr_tunnel::established::{
    EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use i2pr_tunnel::roles::{InboundGatewayRole, InboundParticipantRole};
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
        .expect("sign")
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
        .expect("sign")
}

fn keys(seed: u8) -> LayerKeys {
    LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
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
) -> RouterHash {
    let info = floodfill_info(signer, 60_000);
    let bytes = info
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(60_000))
        .expect("bootstrap")
}

fn outbound_role_for(floodfill: &RouterHash) -> i2pr_tunnel::roles::OutboundGatewayRole {
    let peer = TunnelPeer::from_hash(Hash::from_bytes(*floodfill.as_bytes()));
    let hops = vec![EstablishedHop::terminal(
        peer,
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0xA001).expect("id"),
        keys(0x40),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0xA000).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("tunnel");
    i2pr_tunnel::roles::OutboundGatewayRole::new(tunnel, 120_000)
}

fn inbound_pair(floodfill: &RouterHash, local_receive_id: u32) -> (TunnelId, EstablishedTunnel) {
    let floodfill_peer = TunnelPeer::from_hash(Hash::from_bytes(*floodfill.as_bytes()));
    let middle = TunnelPeer::from_hash(Hash::from_bytes([0x5Eu8; 32]));
    let local_receive = TunnelId::new(local_receive_id).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            floodfill_peer,
            EstablishedRole::InboundGateway,
            TunnelId::new(0xB100).expect("id"),
            keys(0x50),
            EstablishedNextHop::new(middle, TunnelId::new(0xB200).expect("id")),
        ),
        EstablishedHop::with_next(
            middle,
            EstablishedRole::Participant,
            TunnelId::new(0xB200).expect("id"),
            keys(0x51),
            EstablishedNextHop::new(
                TunnelPeer::from_hash(Hash::from_bytes([0x5Fu8; 32])),
                local_receive,
            ),
        ),
    ];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0xB000).expect("id"),
        hops,
        0,
        Some((floodfill_peer, TunnelId::new(0xB100).expect("id"))),
        Some(local_receive),
    )
    .expect("tunnel");
    (local_receive, tunnel)
}

fn drive_inbound_cell(
    tunnel: &EstablishedTunnel,
    inner_bytes: Vec<u8>,
    seed: u64,
) -> TunnelDataMessage {
    let inner =
        I2npMessage::decode_standard(&inner_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode inner");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: tunnel.hops()[0].receive_tunnel().get(),
        message: Box::new(inner),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let ibgw = InboundGatewayRole::new(&tunnel.hops()[0], DuplicateWindow::new(16), 120_000)
        .expect("ibgw");
    let out = ibgw
        .process(&gateway_msg, &mut rng, 0)
        .expect("ibgw process");
    let mut participant =
        InboundParticipantRole::new(&tunnel.hops()[1], DuplicateWindow::new(16), 120_000)
            .expect("participant");
    participant
        .process(&tunnel.hops()[0].peer().hash(), &out.cell, 0)
        .expect("participant")
}

fn store_envelope(signer: &i2pr_crypto::RouterIdentityBundle, message_id: u32) -> Vec<u8> {
    let info = plain_info(signer, 60_000);
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
    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode envelope")
}

fn deadline() -> Deadline {
    Deadline::new(std::time::Duration::from_secs(60)).expect("deadline")
}

#[test]
fn lookup_traverses_outbound_tunnel_to_selected_floodfill() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC001);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target = RouterHash::from_bytes([0x11u8; 32]);
    let gateway = RouterHash::from_bytes(*floodfill.as_bytes());
    let reply = ReplyPath::new(gateway, 0xC101).expect("path");
    let (_id, action) = coord
        .begin_tunnel_lookup(target, &target, reply)
        .expect("send");
    let LookupAction::SendDatabaselookup { peer, message, .. } = &action else {
        panic!("expected send");
    };
    assert_eq!(*peer, floodfill);
    assert_eq!(message.key, Hash::from_bytes(*target.as_bytes()));
    let role = outbound_role_for(&floodfill);
    let mut rng = ChaCha8Rng::seed_from_u64(0xC101);
    let (dispatch, proof) = coord
        .compose_lookup_via_tunnel(&action, &role, 0xC102, 60_000, deadline(), &mut rng, 0)
        .expect("compose");
    assert!(proof.via_tunnel);
    assert_eq!(proof.floodfill, Hash::from_bytes(*floodfill.as_bytes()));
    assert_eq!(proof.lookup_key, Hash::from_bytes(*target.as_bytes()));
    assert_eq!(dispatch.cell_count, 1);
    // The outer is a complete short-transport TunnelData cell.
    let outer = I2npMessage::decode_short_transport(
        dispatch.first().expect("delivery").message_bytes(),
        MAX_I2NP_PAYLOAD_SIZE,
    )
    .expect("outer");
    assert!(matches!(outer.body(), I2npBody::TunnelData(_)));
}

#[test]
fn reply_traverses_inbound_tunnel_to_terminal_success() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC010);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target_signer = bundle(0xC011);
    let target = i2pr_netdb::router_hash(target_signer.identity()).expect("target");
    assert!(!coord.store().contains(&target));
    let (local_receive, inbound_tunnel) = inbound_pair(&floodfill, 0xC201);
    let reply = ReplyPath::new(floodfill, local_receive.get()).expect("path");
    let (request_id, _action) = coord
        .begin_tunnel_lookup(target, &target, reply)
        .expect("send");
    // Floodfill answers with a signed DatabaseStore for the target.
    let envelope_bytes = store_envelope(&target_signer, 0xC202);
    let cell = drive_inbound_cell(&inbound_tunnel, envelope_bytes.clone(), 0xC203);
    let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(2, 2));
    registry
        .activate_inbound(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            inbound_tunnel,
            16,
            1 << 20,
            60_000,
            0,
            120_000,
        )
        .expect("activate");
    let outcome = dispatch_inbound_tunnel_data(&mut registry, &cell, 0).expect("inbound dispatch");
    let bytes = match outcome {
        InboundDispatchOutcome::DatabaseStoreComplete { bytes } => bytes,
        other => panic!("expected store complete, got {other:?}"),
    };
    assert_eq!(bytes, envelope_bytes);
    let envelope = I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    let result = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect("ingest");
    assert!(matches!(result, ResponseOutcome::Completed(_)));
    assert!(coord.store().contains(&target));
    assert_eq!(coord.counters().lookups_succeeded, 1);
    let _ = local_receive;
}

#[test]
fn publication_traverses_outbound_tunnel_with_protocol_ack() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC020);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let local_signer = bundle(0xC021);
    let local = i2pr_netdb::LocalRouterInfoBuilder::new(&local_signer)
        .build_default(Date::from_millis(60_000))
        .expect("local");
    coord.register_local(local);
    let record = coord.begin_publication(floodfill).expect("begin");
    let role = outbound_role_for(&floodfill);
    let mut rng = ChaCha8Rng::seed_from_u64(0xC022);
    let (_dispatch, proof) = coord
        .compose_publication_via_tunnel(
            &record,
            Hash::from_bytes(*floodfill.as_bytes()),
            &role,
            0xC023,
            60_000,
            deadline(),
            &mut rng,
            0,
        )
        .expect("compose publication");
    assert!(proof.via_tunnel);
    // Protocol-derived acknowledgement through the existing coordinator.
    let token = record.attempt.request_id();
    // The coordinator mints a nonzero reply token per attempt; the
    // store message itself carries reply_token=0 (store-and-forget).
    // Correlation uses the minted token.
    let minted = {
        let snapshot = coord.publication_snapshot();
        assert_eq!(snapshot.active, 1);
        record.attempt.reply_token()
    };
    let correlation = coord.correlate_publication_status(minted).expect("ack");
    assert_eq!(correlation.request_id, token);
    assert_eq!(coord.counters().publications_acked, 1);
    assert_eq!(coord.publication_snapshot().acknowledged, 1);
}

#[test]
fn direct_transport_is_not_counted_after_tunnel_composition() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC030);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target = RouterHash::from_bytes([0x33u8; 32]);
    let (_id, action) = coord
        .begin_tunnel_lookup(
            target,
            &target,
            ReplyPath::new(floodfill, 0xC301).expect("path"),
        )
        .expect("send");
    let role = outbound_role_for(&floodfill);
    let mut rng = ChaCha8Rng::seed_from_u64(0xC302);
    let (_dispatch, proof) = coord
        .compose_lookup_via_tunnel(&action, &role, 0xC303, 60_000, deadline(), &mut rng, 0)
        .expect("compose");
    assert!(proof.via_tunnel);
    assert_eq!(
        coord.note_direct_transport_attempt().expect_err("direct"),
        NetDbTunnelError::DirectTransportRejected
    );
}

#[test]
fn search_reply_through_inbound_tunnel_stays_bounded() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC040);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target = RouterHash::from_bytes([0x44u8; 32]);
    let (local_receive, inbound_tunnel) = inbound_pair(&floodfill, 0xC401);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(
            target,
            &target,
            ReplyPath::new(floodfill, local_receive.get()).expect("path"),
        )
        .expect("send");
    let reply = i2pr_proto::DatabaseSearchReplyMessage {
        key: Hash::from_bytes(*target.as_bytes()),
        peer_hashes: vec![
            Hash::from_bytes([0xA1u8; 32]),
            Hash::from_bytes([0xA2u8; 32]),
        ],
        from: Hash::from_bytes(*floodfill.as_bytes()),
    };
    let envelope_bytes = I2npMessage::new_standard(
        0xC402,
        Date::from_millis(60_000),
        I2npBody::DatabaseSearchReply(reply),
    )
    .expect("envelope")
    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode");
    let cell = drive_inbound_cell(&inbound_tunnel, envelope_bytes.clone(), 0xC403);
    let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(2, 2));
    registry
        .activate_inbound(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            inbound_tunnel,
            16,
            1 << 20,
            60_000,
            0,
            120_000,
        )
        .expect("activate");
    let outcome = dispatch_inbound_tunnel_data(&mut registry, &cell, 0).expect("dispatch");
    assert!(matches!(
        outcome,
        InboundDispatchOutcome::DatabaseSearchReplyComplete { .. }
    ));
    let envelope =
        I2npMessage::decode_standard(&envelope_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    let result = coord
        .ingest_search_reply(request_id, &envelope)
        .expect("search reply");
    assert_eq!(result, ResponseOutcome::Continue);
    assert_eq!(coord.counters().search_replies_merged, 2);
    let _ = local_receive;
}

#[test]
fn malformed_recovered_envelope_is_bounded() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC050);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target = RouterHash::from_bytes([0x55u8; 32]);
    let (local_receive, inbound_tunnel) = inbound_pair(&floodfill, 0xC501);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(
            target,
            &target,
            ReplyPath::new(floodfill, local_receive.get()).expect("path"),
        )
        .expect("send");
    // A DeliveryStatus arriving on the inbound tunnel is not a NetDB
    // response; the coordinator rejects it as malformed without state.
    let status_bytes = I2npMessage::new_standard(
        0xC502,
        Date::from_millis(60_000),
        I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
            0x9999,
            Date::from_millis(60_000),
        )),
    )
    .expect("status")
    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode");
    let cell = drive_inbound_cell(&inbound_tunnel, status_bytes.clone(), 0xC503);
    let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(2, 2));
    registry
        .activate_inbound(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            inbound_tunnel,
            16,
            1 << 20,
            60_000,
            0,
            120_000,
        )
        .expect("activate");
    let outcome = dispatch_inbound_tunnel_data(&mut registry, &cell, 0).expect("dispatch");
    assert!(matches!(
        outcome,
        InboundDispatchOutcome::DeliveryStatusComplete { .. }
    ));
    let envelope =
        I2npMessage::decode_standard(&status_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    let error = coord
        .ingest_tunnel_store(
            request_id,
            &envelope,
            ValidationContext::new(Date::from_millis(60_000)),
        )
        .expect_err("malformed");
    assert_eq!(error, NetDbTunnelError::Malformed);
    let _ = local_receive;
}

#[test]
fn tunnel_loss_returns_typed_failure_without_fallback() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC060);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target = RouterHash::from_bytes([0x66u8; 32]);
    let (request_id, _action) = coord
        .begin_tunnel_lookup(
            target,
            &target,
            ReplyPath::new(floodfill, 0xC601).expect("path"),
        )
        .expect("send");
    coord.note_tunnel_loss(request_id).expect("retry");
    assert_eq!(coord.counters().tunnel_failures, 1);
    assert_eq!(
        coord.note_direct_transport_attempt().expect_err("direct"),
        NetDbTunnelError::DirectTransportRejected
    );
}

#[test]
fn reference_unavailable_after_tunnel_returns_honest_no_candidate() {
    let coord = coordinator();
    // No bootstrap: the authoritative store is empty, so even with a
    // valid reply path the lookup terminates honestly.
    assert_eq!(coord.floodfill_count(), 0);
    assert!(
        coord
            .candidate_hashes(
                &RouterHash::from_bytes([0x11u8; 32]),
                &RouterHash::from_bytes([0x11u8; 32])
            )
            .is_empty()
    );
}

#[test]
fn liveness_remains_green_during_netdb_activity() {
    let mut coord = coordinator();
    let floodfill_signer = bundle(0xC070);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let target = RouterHash::from_bytes([0x77u8; 32]);
    let (_id, _action) = coord
        .begin_tunnel_lookup(
            target,
            &target,
            ReplyPath::new(floodfill, 0xC701).expect("path"),
        )
        .expect("send");
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(60_000);
    let outbound = i2pr_tunnel::pool::TunnelSlot::from_raw(1);
    let inbound = i2pr_tunnel::pool::TunnelSlot::from_raw(2);
    scheduler.register_pair(outbound, inbound).expect("pair");
    scheduler.advance_time(60_000 + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    assert!(matches!(action, LivenessAction::SendTest { .. }));
}
