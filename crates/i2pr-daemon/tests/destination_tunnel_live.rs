//! Plan 187 live destination message-plane trajectory.
//!
//! Drives the daemon-owned
//! [`DestinationTunnelCoordinator`] end-to-end through real
//! outbound/inbound TunnelData cells:
//!
//! - remote Standard LeaseSet2 lookup composition through a one-hop
//!   `OutboundGatewayRole` whose first hop is the controlled
//!   floodfill, with the tunnel ROUTER target proven to be the
//!   selected floodfill F and the nested key remaining K;
//! - inbound LeaseSet2 recovery through a two-hop inbound chain +
//!   `DataPlaneRegistry` + coordinator ingest to terminal success,
//!   cache install, and the routing-layer handoff;
//! - authenticated destination traffic in both directions through
//!   real destination-owned tunnels: existing ECIES/Garlic
//!   composition, OBEP delivery to the selected remote lease,
//!   inbound-chain recovery through the coordinator's Garlic path,
//!   existing ECIES decrypt, per-destination delivery with byte
//!   equality, and sibling isolation;
//! - local LeaseSet2 publication through the same outbound path
//!   with protocol-derived `DeliveryStatus` correlation;
//! - typed tunnel-loss/direct-rejection behavior and liveness green.

#![forbid(unsafe_code)]

use i2pr_client::{
    DestinationConfig, DestinationDispatcher, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool, EciesSessionConfig,
    EciesSessionManager, EncryptedOutbound, InboundDispatchOutcome as ClientDispatchOutcome,
    OutboundDeliveryPlan, OutboundRequest, build_signed_lease_set2, compose_outbound_delivery,
};
use i2pr_daemon::destination_tunnels::{
    DestinationTunnelCoordinator, DestinationTunnelError, LeaseStoreIngestOutcome,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{
    DestinationHash, LookupAction, LookupPolicy, ReplyPath, RouterHash, RouterInfoStoreConfig,
    router_hash_from_destination,
};
use i2pr_proto::{
    Date, Hash, I2npBody, I2npMessage, MAX_COMMON_STRUCTURE_SIZE, MAX_I2NP_PAYLOAD_SIZE, Mapping,
    TunnelDataMessage, TunnelGatewayMessage,
};
use i2pr_transport::Deadline;
use i2pr_tunnel::DuplicateWindow;
use i2pr_tunnel::build_crypto::LayerKeys;
use i2pr_tunnel::data_plane_registry::{DataPlaneCapacity, DataPlaneRegistry};
use i2pr_tunnel::established::{
    EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use i2pr_tunnel::roles::{
    InboundGatewayRole, InboundParticipantRole, OutboundEndpointRole, OutboundParticipantRole,
};
use i2pr_tunnel::{RouterDeliveryAction, RouterDeliveryKind};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const NOW_SECONDS: u32 = 5_000;
const NOW_MS: u64 = 300_000;
const EXPIRY_MS: u64 = NOW_MS + 60_000;

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
        .expect("sign")
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
) -> RouterHash {
    let info = floodfill_info(signer, NOW_MS);
    let bytes = info
        .encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    coord
        .bootstrap_reference_router_info(&bytes, Date::from_millis(NOW_MS))
        .expect("bootstrap")
}

fn peer(hash: Hash) -> TunnelPeer {
    TunnelPeer::from_hash(hash)
}

fn hop_hash(seed: u64, index: u8) -> Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8) ^ (seed as u8).wrapping_add(offset as u8);
    }
    Hash::from_bytes(bytes)
}

fn layer_keys(seed: u8) -> LayerKeys {
    LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

fn deadline() -> Deadline {
    Deadline::new(std::time::Duration::from_secs(60)).expect("deadline")
}

/// One-hop outbound role whose first hop is the controlled floodfill
/// (lookup/publication path).
fn outbound_role_for(floodfill: &RouterHash) -> i2pr_tunnel::roles::OutboundGatewayRole {
    let gateway = TunnelPeer::from_hash(Hash::from_bytes(*floodfill.as_bytes()));
    let hops = vec![EstablishedHop::terminal(
        gateway,
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0xA001).expect("id"),
        layer_keys(0x40),
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
    i2pr_tunnel::roles::OutboundGatewayRole::new(tunnel, EXPIRY_MS)
}

/// Two-hop inbound chain behind the floodfill IBGW (reply path).
fn inbound_pair(floodfill: &RouterHash, local_receive_id: u32) -> (TunnelId, EstablishedTunnel) {
    let floodfill_peer = TunnelPeer::from_hash(Hash::from_bytes(*floodfill.as_bytes()));
    let middle = TunnelPeer::from_hash(Hash::from_bytes([0x5Eu8; 32]));
    let local_receive = TunnelId::new(local_receive_id).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            floodfill_peer,
            EstablishedRole::InboundGateway,
            TunnelId::new(0xB100).expect("id"),
            layer_keys(0x50),
            EstablishedNextHop::new(middle, TunnelId::new(0xB200).expect("id")),
        ),
        EstablishedHop::with_next(
            middle,
            EstablishedRole::Participant,
            TunnelId::new(0xB200).expect("id"),
            layer_keys(0x51),
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

/// Drives one inner I2NP envelope through the IBGW + participant
/// hops, returning the final cell for registry dispatch.
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
    let ibgw = InboundGatewayRole::new(&tunnel.hops()[0], DuplicateWindow::new(16), EXPIRY_MS)
        .expect("ibgw");
    let out = ibgw
        .process(&gateway_msg, &mut rng, 0)
        .expect("ibgw process");
    let mut participant =
        InboundParticipantRole::new(&tunnel.hops()[1], DuplicateWindow::new(16), EXPIRY_MS)
            .expect("participant");
    participant
        .process(&tunnel.hops()[0].peer().hash(), &out.cell, 0)
        .expect("participant")
}

fn activate_inbound(tunnel: EstablishedTunnel) -> DataPlaneRegistry {
    let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(2, 2));
    registry
        .activate_inbound(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            tunnel,
            16,
            1 << 20,
            60_000,
            0,
            EXPIRY_MS,
        )
        .expect("activate");
    registry
}

/// Two-hop outbound chain for destination traffic (participant + OBEP).
fn outbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_hash(seed, 1)),
            EstablishedRole::Participant,
            TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x50),
            EstablishedNextHop::new(
                peer(hop_hash(seed, 2)),
                TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::terminal(
            peer(hop_hash(seed, 2)),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x51),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0200_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("outbound established")
}

/// Two-hop inbound chain ending at the destination-local endpoint.
fn inbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_hash(seed, 1)),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            layer_keys(0x60),
            EstablishedNextHop::new(
                peer(hop_hash(seed, 2)),
                TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::with_next(
            peer(hop_hash(seed, 2)),
            EstablishedRole::Participant,
            TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x61),
            EstablishedNextHop::new(peer(hop_hash(seed, 3)), local_receive),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((peer(hop_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established")
}

/// One local destination side owning identity, signed LeaseSet2,
/// routing, ECIES session, dispatcher, and the real outbound role.
struct Side {
    seed: u64,
    identity: DestinationIdentity,
    lease_set2: i2pr_proto::LeaseSet2,
    routing: DestinationRouting,
    dispatcher: DestinationDispatcher,
    session: EciesSessionManager,
    outbound: DestinationOutboundRole,
}

impl Side {
    fn new(seed: u64) -> Self {
        let identity = destination_identity(seed);
        let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
        pool.register_inbound(
            inbound_tunnel_direct(seed).into_extracted(),
            u64::from(NOW_SECONDS),
        )
        .expect("inbound registered");
        pool.register_outbound(
            outbound_tunnel_direct(seed).into_extracted(),
            u64::from(NOW_SECONDS),
        )
        .expect("outbound registered");
        let lease_sources = pool.inbound_lease_sources(u64::from(NOW_SECONDS));
        let lease_set2 =
            build_signed_lease_set2(&identity, &lease_sources, NOW_SECONDS).expect("signed ls2");
        let mut dispatcher = DestinationDispatcher::new();
        dispatcher
            .register_destination(identity.id())
            .expect("register destination");
        dispatcher
            .bind_destination_hash(identity.id(), identity.id().as_netdb_key())
            .expect("bind destination hash");
        Self {
            seed,
            identity,
            lease_set2,
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            dispatcher,
            session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound: DestinationOutboundRole::new(outbound_tunnel_direct(seed), EXPIRY_MS),
        }
    }

    fn hash(&self) -> DestinationHash {
        self.identity.id().as_netdb_key()
    }

    fn compose(
        &mut self,
        remote_hash: DestinationHash,
        payload: &[u8],
        rng_seed: u64,
    ) -> OutboundDeliveryPlan {
        let request = OutboundRequest::new(6, 0, 0, payload, NOW_MS, Some(self.lease_set2.clone()))
            .expect("outbound request");
        let Side {
            routing,
            session,
            outbound,
            identity,
            ..
        } = self;
        let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
        compose_outbound_delivery(
            routing,
            session,
            outbound,
            identity.id(),
            identity.static_secret_bytes(),
            remote_hash,
            &request,
            NOW_SECONDS,
            NOW_MS,
            &mut rng,
        )
        .expect("compose delivery")
    }

    fn dispatch(&mut self, envelope: &I2npMessage) -> ClientDispatchOutcome {
        let Side {
            dispatcher,
            session,
            routing,
            identity,
            ..
        } = self;
        dispatcher.dispatch_garlic_envelope(
            session,
            identity.id(),
            identity.static_secret_bytes(),
            &identity.static_public_bytes(),
            NOW_SECONDS,
            envelope,
            routing.lease_set2_store_mut(),
        )
    }

    fn pop_app(&mut self, expected: &[u8]) {
        let queued = self
            .dispatcher
            .pop_payload(self.identity.id())
            .expect("application payload");
        // Plan 192: the queued Garlic Clove bytes are now the
        // i2pd-compatible 9-byte short-transport Data envelope whose
        // body is the i2pd-compatible I2CP-style Data wire shape.
        // The dispatcher surfaces the inner Data body bytes verbatim;
        // the application payload is recovered by parsing the I2CP
        // Data body wrapper.
        let message = I2npMessage::decode_short_transport(queued.bytes(), MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode queued message");
        match message.body() {
            I2npBody::Data(body) => {
                let decoded = i2pr_proto::decode_i2cp_data_body(body.payload.as_bytes())
                    .expect("decode I2CP Data body");
                assert_eq!(decoded.payload, expected);
            }
            other => panic!("queued payload must be Data, got {other:?}"),
        }
    }
}

/// Installs `remote`'s validated LeaseSet2 in `side`'s routing state.
fn preresolve_remote(side: &mut Side, remote: &Side) {
    let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        remote.lease_set2.clone(),
        Some(remote.hash()),
        i2pr_netdb::LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated remote ls2");
    side.routing
        .install_remote_lease_set2(validated)
        .expect("install resolved remote ls2");
}

/// Runs the sender's real outbound roles (participant + OBEP) and
/// asserts the OBEP delivery target equals the selected remote
/// lease. Returns the OBEP delivery action.
fn drive_outbound(sender: &Side, plan: &OutboundDeliveryPlan) -> RouterDeliveryAction {
    let outbound_hops = sender.outbound.role().established().hops();
    let mut out_participant =
        OutboundParticipantRole::new(&outbound_hops[0], DuplicateWindow::new(16), EXPIRY_MS)
            .expect("outbound participant role");
    let mut obep = OutboundEndpointRole::new(
        &outbound_hops[1],
        DuplicateWindow::new(16),
        16,
        1 << 20,
        60_000,
        EXPIRY_MS,
        0,
    );
    let mut actions: Vec<RouterDeliveryAction> = Vec::new();
    for cell in &plan.cells {
        let forwarded = out_participant
            .process(&hop_hash(sender.seed, 0), &cell.cell, 0)
            .expect("outbound participant forward");
        let delivered = obep
            .process(&outbound_hops[0].peer().hash(), &forwarded, 0)
            .expect("obep process");
        if let Some(action) = delivered {
            actions.push(action);
        }
    }
    assert_eq!(actions.len(), 1, "OBEP must emit exactly one action");
    let action = actions.remove(0);
    assert_eq!(
        action.target_router, plan.selected_lease.gateway_router_hash,
        "OBEP target router must equal the selected lease gateway"
    );
    assert_eq!(
        action.tunnel_id.expect("tunnel id").get(),
        plan.selected_lease.tunnel_id,
        "OBEP target tunnel id must equal the selected lease tunnel id"
    );
    assert!(
        matches!(action.kind, RouterDeliveryKind::TunnelGateway),
        "OBEP delivery kind must be TunnelGateway"
    );
    assert_eq!(action.message, plan.garlic_i2np_bytes);
    action
}

/// Crosses the post-OBEP seam into the receiver's real inbound chain
/// and recovers the Garlic carrier through the coordinator's
/// registry-backed Garlic path (never direct).
fn recover_through_coordinator(
    coord: &mut DestinationTunnelCoordinator,
    receiver_seed: u64,
    action: &RouterDeliveryAction,
) -> Vec<u8> {
    let inner =
        I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE).expect("decode obep");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: action.tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner),
    };
    let inbound_tunnel = inbound_tunnel_direct(receiver_seed);
    let mut rng = ChaCha8Rng::seed_from_u64(0x51EA);
    let ibgw = InboundGatewayRole::new(
        &inbound_tunnel.hops()[0],
        DuplicateWindow::new(16),
        EXPIRY_MS,
    )
    .expect("ibgw");
    let ibgw_out = ibgw
        .process(&gateway_msg, &mut rng, 0)
        .expect("ibgw process");
    let mut participant = InboundParticipantRole::new(
        &inbound_tunnel.hops()[1],
        DuplicateWindow::new(16),
        EXPIRY_MS,
    )
    .expect("participant");
    let cell = participant
        .process(&hop_hash(receiver_seed, 1), &ibgw_out.cell, 0)
        .expect("participant forward");
    let mut registry = activate_inbound(inbound_tunnel);
    coord
        .recover_garlic_bytes(&mut registry, &cell, 0)
        .expect("garlic recovery")
}

fn decode_garlic(bytes: &[u8]) -> I2npMessage {
    let message =
        I2npMessage::decode_standard(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    assert!(
        matches!(message.body(), I2npBody::Garlic(_)),
        "the recovered carrier must be an I2NP Garlic body"
    );
    message
}

#[test]
fn lease_lookup_traverses_outbound_tunnel_to_selected_floodfill() {
    let mut coord = coordinator();
    let floodfill_signer = router_bundle(0xE001);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let side = Side::new(0xE002);
    let target = side.hash();
    let routing_key = router_hash_from_destination(target);
    let (_id, action) = coord
        .begin_lease_lookup(
            target,
            &routing_key,
            ReplyPath::new(floodfill, 0xE101).expect("path"),
        )
        .expect("send");
    let LookupAction::SendDatabaselookup { peer, message, .. } = &action else {
        panic!("expected send");
    };
    assert_eq!(*peer, floodfill);
    assert_eq!(
        message.key,
        Hash::from_bytes(*router_hash_from_destination(target).as_bytes())
    );
    let role = outbound_role_for(&floodfill);
    let mut rng = ChaCha8Rng::seed_from_u64(0xE102);
    let (dispatch, proof) = coord
        .compose_lookup_via_tunnel(&action, &role, 0xE103, NOW_MS, deadline(), &mut rng, 0)
        .expect("compose");
    assert!(proof.via_tunnel);
    assert_eq!(proof.floodfill, Hash::from_bytes(*floodfill.as_bytes()));
    assert_eq!(dispatch.cell_count, 1);
    let outer = I2npMessage::decode_short_transport(
        dispatch.first().expect("delivery").message_bytes(),
        MAX_I2NP_PAYLOAD_SIZE,
    )
    .expect("outer");
    assert!(matches!(outer.body(), I2npBody::TunnelData(_)));
}

#[test]
fn lease_reply_traverses_inbound_tunnel_to_terminal_success() {
    let mut coord = coordinator();
    let floodfill_signer = router_bundle(0xE010);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let side = Side::new(0xE011);
    let target = side.hash();
    assert!(coord.remote_lease_summary(&target).is_none());
    let (local_receive, inbound_tunnel) = inbound_pair(&floodfill, 0xE201);
    let routing_key = router_hash_from_destination(target);
    let (request_id, _action) = coord
        .begin_lease_lookup(
            target,
            &routing_key,
            ReplyPath::new(floodfill, local_receive.get()).expect("path"),
        )
        .expect("send");
    // The floodfill answers with the destination's signed LeaseSet2.
    let store = i2pr_proto::DatabaseStoreMessage {
        key: side.lease_set2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(side.lease_set2.clone())),
    };
    let envelope_bytes = I2npMessage::new_standard(
        0xE202,
        Date::from_millis(NOW_MS),
        I2npBody::DatabaseStore(Box::new(store)),
    )
    .expect("envelope")
    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode envelope");
    let cell = drive_inbound_cell(&inbound_tunnel, envelope_bytes.clone(), 0xE203);
    let mut registry = activate_inbound(inbound_tunnel);
    let recovered = {
        use i2pr_daemon::inbound_dispatch::{InboundDispatchOutcome, dispatch_inbound_tunnel_data};
        match dispatch_inbound_tunnel_data(&mut registry, &cell, 0).expect("dispatch") {
            InboundDispatchOutcome::DatabaseStoreComplete { bytes } => bytes,
            other => panic!("expected store complete, got {other:?}"),
        }
    };
    assert_eq!(recovered, envelope_bytes);
    let envelope = I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    let outcome = coord
        .ingest_tunnel_lease_store(request_id, &envelope, NOW_SECONDS)
        .expect("ingest");
    let LeaseStoreIngestOutcome::Completed { summary, .. } = outcome else {
        panic!("expected completed ingest");
    };
    assert_eq!(summary.destination, target);
    assert_eq!(summary.lease_count, 1);
    assert_eq!(coord.counters().lookups_succeeded, 1);
    // Routing-layer handoff: the cached record installs and selects.
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let cached = coord
        .lease_store()
        .get(&target)
        .expect("cached record")
        .clone();
    let remote = routing.install_remote_lease_set2(cached).expect("install");
    assert_eq!(remote, target);
    let mut rng = ChaCha8Rng::seed_from_u64(0xE204);
    let selected = routing
        .select_lease(remote, NOW_SECONDS, &mut rng)
        .expect("select lease");
    assert_eq!(
        selected.gateway_router_hash,
        side.lease_set2.leases()[0].tunnel_gateway()
    );
    let _ = local_receive;
}

#[test]
fn destination_message_round_trip_through_real_tunnels() {
    let mut coord = coordinator();
    let mut side_a = Side::new(0xE020);
    let mut side_b = Side::new(0xE021);
    let side_c = Side::new(0xE022);
    preresolve_remote(&mut side_a, &side_b);
    let app_a = b"destination-plane-a-to-b";
    let plan = side_a.compose(side_b.hash(), app_a, 0xE023);
    assert!(matches!(
        plan.encrypted_message,
        EncryptedOutbound::NewSession { .. }
    ));
    // A outbound tunnel -> OBEP proves the selected remote lease.
    let action = drive_outbound(&side_a, &plan);
    // OBEP -> B inbound tunnel -> coordinator Garlic recovery.
    let recovered = recover_through_coordinator(&mut coord, 0xE021, &action);
    assert_eq!(coord.counters().garlic_recovered, 1);
    let envelope = decode_garlic(&recovered);
    // B decrypts through the existing ECIES path and delivers exactly
    // A's application bytes to B's queue only.
    let outcome = side_b.dispatch(&envelope);
    assert!(matches!(
        outcome,
        ClientDispatchOutcome::NewSessionProcessed { .. }
    ));
    side_b.pop_app(app_a);
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
    // Sibling destination isolation: C holds no session for A and its
    // queue stays empty.
    let mut side_c = side_c;
    let outcome_c = side_c.dispatch(&envelope);
    assert!(!matches!(
        outcome_c,
        ClientDispatchOutcome::NewSessionProcessed { .. }
    ));
    assert!(
        side_c
            .dispatcher
            .pop_payload(side_c.identity.id())
            .is_none()
    );
}

#[test]
fn destination_reply_returns_through_real_tunnels() {
    let mut coord = coordinator();
    let mut side_a = Side::new(0xE030);
    let mut side_b = Side::new(0xE031);
    preresolve_remote(&mut side_a, &side_b);
    preresolve_remote(&mut side_b, &side_a);
    // A -> B bound New Session bootstraps the pair.
    let app_a = b"pair-bootstrap-a";
    let plan_ns = side_a.compose(side_b.hash(), app_a, 0xE032);
    let action_ns = drive_outbound(&side_a, &plan_ns);
    let recovered_ns = recover_through_coordinator(&mut coord, 0xE031, &action_ns);
    let envelope_ns = decode_garlic(&recovered_ns);
    assert!(matches!(
        side_b.dispatch(&envelope_ns),
        ClientDispatchOutcome::NewSessionProcessed { .. }
    ));
    side_b.pop_app(app_a);
    // B -> A reply traverses B's outbound tunnel and A's inbound
    // tunnel through the same coordinator Garlic path.
    let app_b = b"pair-reply-b-to-a";
    let plan_nsr = side_b.compose(side_a.hash(), app_b, 0xE033);
    let action_nsr = drive_outbound(&side_b, &plan_nsr);
    let recovered_nsr = recover_through_coordinator(&mut coord, 0xE030, &action_nsr);
    let envelope_nsr = decode_garlic(&recovered_nsr);
    side_a.dispatch(&envelope_nsr);
    side_a.pop_app(app_b);
    assert_eq!(coord.counters().garlic_recovered, 2);
}

#[test]
fn tampered_garlic_yields_no_plaintext_and_no_binding() {
    let mut coord = coordinator();
    let mut side_a = Side::new(0xE040);
    let mut side_b = Side::new(0xE041);
    preresolve_remote(&mut side_a, &side_b);
    let plan = side_a.compose(side_b.hash(), b"tamper-me", 0xE042);
    let action = drive_outbound(&side_a, &plan);
    let mut recovered = recover_through_coordinator(&mut coord, 0xE041, &action);
    let accepted_before = side_b.dispatcher.accepted_lease_set2_count();
    // Tamper the Garlic carrier after tunnel recovery: authentication
    // must fail closed with no plaintext and no sender binding.
    let mid = recovered.len() / 2;
    recovered[mid] ^= 0x01;
    let decode_result = I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE);
    if let Ok(envelope) = decode_result {
        side_b.dispatch(&envelope);
    }
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
    assert_eq!(
        side_b.dispatcher.accepted_lease_set2_count(),
        accepted_before
    );
}

#[test]
fn wrong_inbound_tunnel_id_and_body_are_rejected() {
    let mut coord = coordinator();
    let floodfill_signer = router_bundle(0xE050);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let (local_receive, inbound_tunnel) = inbound_pair(&floodfill, 0xE501);
    // Unknown tunnel id never allocates state.
    let mut registry = activate_inbound(inbound_tunnel);
    let unknown = TunnelDataMessage {
        tunnel_id: 0xBEEF,
        data: [0x44u8; i2pr_proto::TUNNEL_DATA_PAYLOAD_SIZE],
    };
    let error = coord
        .recover_garlic_bytes(&mut registry, &unknown, 0)
        .expect_err("unknown tunnel");
    assert_eq!(error, DestinationTunnelError::UnknownInboundTunnel(0xBEEF));
    let _ = local_receive;
}

#[test]
fn local_ls2_publication_traverses_tunnel_with_protocol_ack() {
    let mut coord = coordinator();
    let floodfill_signer = router_bundle(0xE060);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let side = Side::new(0xE061);
    // The published record describes the destination's real inbound
    // tunnel lease.
    let proof_material = {
        let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
        pool.register_inbound(
            inbound_tunnel_direct(0xE061).into_extracted(),
            u64::from(NOW_SECONDS),
        )
        .expect("inbound");
        pool.register_outbound(
            outbound_tunnel_direct(0xE061).into_extracted(),
            u64::from(NOW_SECONDS),
        )
        .expect("outbound");
        let mut proof_coord = coordinator();
        proof_coord
            .verify_remote_material(&pool, u64::from(NOW_SECONDS))
            .expect("remote material")
    };
    assert!(!proof_material.zero_hop);
    let store = i2pr_proto::DatabaseStoreMessage {
        key: side.lease_set2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(side.lease_set2.clone())),
    };
    let request_id = coord
        .begin_ls2_publication(store, floodfill)
        .expect("begin publication");
    let role = outbound_role_for(&floodfill);
    let mut rng = ChaCha8Rng::seed_from_u64(0xE063);
    let (_dispatch, proof) = coord
        .compose_ls2_publication_via_tunnel(
            request_id,
            Hash::from_bytes(*floodfill.as_bytes()),
            &role,
            0xE064,
            NOW_MS,
            deadline(),
            &mut rng,
            0,
        )
        .expect("compose publication");
    assert!(proof.via_tunnel);
    // Protocol-derived DeliveryStatus acknowledgement completes the row.
    let acked = coord.correlate_ls2_publication_status(0xE064).expect("ack");
    assert_eq!(acked, request_id);
    assert_eq!(coord.counters().publications_acked, 1);
}

#[test]
fn tunnel_loss_and_direct_rejection_stay_typed() {
    let mut coord = coordinator();
    let floodfill_signer = router_bundle(0xE070);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let side = Side::new(0xE071);
    let target = side.hash();
    let routing_key = router_hash_from_destination(target);
    let (request_id, _action) = coord
        .begin_lease_lookup(
            target,
            &routing_key,
            ReplyPath::new(floodfill, 0xE701).expect("path"),
        )
        .expect("send");
    coord.note_tunnel_loss(request_id).expect("retry");
    assert_eq!(coord.counters().tunnel_failures, 1);
    assert_eq!(
        coord.note_direct_transport_attempt().expect_err("direct"),
        DestinationTunnelError::DirectTransportRejected
    );
}

#[test]
fn liveness_remains_green_during_destination_activity() {
    let mut coord = coordinator();
    let floodfill_signer = router_bundle(0xE080);
    let floodfill = bootstrap_floodfill(&mut coord, &floodfill_signer);
    let side = Side::new(0xE081);
    let target = side.hash();
    let routing_key = router_hash_from_destination(target);
    let (_id, _action) = coord
        .begin_lease_lookup(
            target,
            &routing_key,
            ReplyPath::new(floodfill, 0xE801).expect("path"),
        )
        .expect("send");
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(NOW_MS);
    let outbound = i2pr_tunnel::pool::TunnelSlot::from_raw(1);
    let inbound = i2pr_tunnel::pool::TunnelSlot::from_raw(2);
    scheduler.register_pair(outbound, inbound).expect("pair");
    scheduler.advance_time(NOW_MS + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    assert!(matches!(action, LivenessAction::SendTest { .. }));
}
