//! Plan 296 garlic reply-bundling trajectory.
//!
//! End-to-end proof through the real destination stack: two
//! destinations handshake (bound New Session), the responder
//! composes one bundled New Session Reply carrying two application
//! payloads through the production composer, the envelope crosses
//! the OBEP/inbound seams, and the initiator's dispatcher queues
//! both payloads in wire order. Form and policy gating (Existing
//! Session, disabled policy, empty/overcount bundles) is proven
//! against live session state, not mocks.

#![forbid(unsafe_code)]

use i2pr_client::{
    BundleError, DestinationConfig, DestinationDispatcher, DestinationIdentity,
    DestinationOutboundRole, DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool,
    EciesSessionConfig, EciesSessionManager, InboundDispatchOutcome, OutboundDeliveryPlan,
    OutboundRequest, ReplyBundling, build_signed_lease_set2, compose_bundled_reply_delivery,
    compose_outbound_delivery,
};
use i2pr_netdb::ValidatedLeaseSet2;
use i2pr_proto::{Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelGatewayMessage};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    InboundGatewayRole, InboundParticipantRole, LayerKeys, LocalInboundEndpointRole,
    OutboundEndpointRole, OutboundParticipantRole, RouterDeliveryAction, RouterDeliveryKind,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const A_SEED: u64 = 0x296A;
const B_SEED: u64 = 0x296B;
const NOW_SECONDS: u32 = 5_200;
const START_MS: u64 = 400_000;

fn peer(value: Hash) -> TunnelPeer {
    TunnelPeer::from_hash(value)
}

fn hop_router_hash(seed: u64, index: u8) -> Hash {
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

fn destination_identity(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("destination identity")
}

fn outbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::Participant,
            TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x50),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::terminal(
            peer(hop_router_hash(seed, 2)),
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

fn inbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            layer_keys(0x60),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 2)),
            EstablishedRole::Participant,
            TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x61),
            EstablishedNextHop::new(peer(hop_router_hash(seed, 3)), local_receive),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((peer(hop_router_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established")
}

struct InboundChain {
    ibgw: InboundGatewayRole,
    participant: InboundParticipantRole,
    endpoint: LocalInboundEndpointRole,
}

impl InboundChain {
    fn new(seed: u64) -> Self {
        let inbound_tunnel = inbound_tunnel_direct(seed);
        let ibgw_hop = inbound_tunnel.hops()[0].clone();
        let participant_hop = inbound_tunnel.hops()[1].clone();
        let ibgw = InboundGatewayRole::new(&ibgw_hop, DuplicateWindow::new(16), START_MS + 120_000)
            .expect("ibgw role");
        let participant = InboundParticipantRole::new(
            &participant_hop,
            DuplicateWindow::new(16),
            START_MS + 120_000,
        )
        .expect("inbound participant role");
        let endpoint = LocalInboundEndpointRole::new(
            inbound_tunnel_direct(seed),
            16,
            1 << 20,
            60_000,
            0,
            START_MS + 120_000,
        );
        Self {
            ibgw,
            participant,
            endpoint,
        }
    }
}

/// One destination side: identity, signed LeaseSet2, routing,
/// session, dispatcher, and explicit tunnel roles (no streaming:
/// payloads compare at the dispatcher queue).
struct Side {
    seed: u64,
    identity: DestinationIdentity,
    lease_set2: i2pr_proto::LeaseSet2,
    routing: DestinationRouting,
    dispatcher: DestinationDispatcher,
    session: EciesSessionManager,
    outbound: DestinationOutboundRole,
    inbound: InboundChain,
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
            outbound: DestinationOutboundRole::new(
                outbound_tunnel_direct(seed),
                START_MS + 300_000,
            ),
            inbound: InboundChain::new(seed),
        }
    }

    fn hash_bytes(&self) -> [u8; 32] {
        *self.identity.id().as_hash().as_bytes()
    }

    fn remote_hash(&self) -> i2pr_netdb::DestinationHash {
        i2pr_netdb::DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(self.hash_bytes()))
    }

    fn dispatch(&mut self, envelope: &I2npMessage) -> InboundDispatchOutcome {
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
}

fn preresolve_remote(side: &mut Side, remote: &Side) {
    let validated = ValidatedLeaseSet2::from_lease_set2(
        remote.lease_set2.clone(),
        Some(remote.identity.id().as_netdb_key()),
        i2pr_netdb::LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated remote ls2");
    side.routing
        .install_remote_lease_set2(validated)
        .expect("install resolved remote ls2");
}

fn obep_actions(sender: &Side, plan: &OutboundDeliveryPlan) -> Vec<RouterDeliveryAction> {
    let outbound_hops = sender.outbound.role().established().hops();
    let mut out_participant = OutboundParticipantRole::new(
        &outbound_hops[0],
        DuplicateWindow::new(16),
        START_MS + 120_000,
    )
    .expect("outbound participant role");
    let mut obep = OutboundEndpointRole::new(
        &outbound_hops[1],
        DuplicateWindow::new(16),
        16,
        1 << 20,
        60_000,
        START_MS + 120_000,
        0,
    );
    let mut actions: Vec<RouterDeliveryAction> = Vec::new();
    for cell in &plan.cells {
        let forwarded = out_participant
            .process(&hop_router_hash(sender.seed, 0), &cell.cell, 0)
            .expect("outbound participant forward");
        let delivered = obep
            .process(&outbound_hops[0].peer().hash(), &forwarded, 0)
            .expect("obep process");
        if let Some(action) = delivered {
            actions.push(action);
        }
    }
    assert_eq!(
        actions.len(),
        1,
        "one integrated send must produce exactly one post-OBEP action"
    );
    let action = &actions[0];
    assert_eq!(
        action.target_router,
        plan.selected_lease.gateway_router_hash
    );
    assert_eq!(
        action.tunnel_id.expect("tunnel id").get(),
        plan.selected_lease.tunnel_id
    );
    assert!(matches!(action.kind, RouterDeliveryKind::TunnelGateway));
    assert_eq!(action.message, plan.garlic_i2np_bytes);
    actions
}

fn feed_action(receiver: &mut Side, action: &RouterDeliveryAction) -> Vec<u8> {
    let inner_i2np =
        I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE).expect("decode i2np");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: action.tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner_i2np),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(0x51EA);
    let cells = receiver
        .inbound
        .ibgw
        .process_cells(&gateway_msg, &mut rng, 0)
        .expect("ibgw multi-cell forward");
    let mut recovered = None;
    for cell in &cells {
        let forwarded = receiver
            .inbound
            .participant
            .process(&hop_router_hash(receiver.seed, 1), &cell.cell, 0)
            .expect("inbound participant forward");
        if let Some(message) = receiver
            .inbound
            .endpoint
            .process(&hop_router_hash(receiver.seed, 2), &forwarded, 0)
            .expect("local endpoint process")
        {
            assert!(
                recovered.is_none(),
                "the reassembler must complete exactly once per delivery"
            );
            recovered = Some(message);
        }
    }
    recovered.expect("endpoint recovered the Garlic carrier")
}

fn request_for(payload: &[u8], lease_set2: Option<i2pr_proto::LeaseSet2>) -> OutboundRequest {
    OutboundRequest::new(
        6,
        0x12A0,
        0x12B0,
        payload,
        u64::from(NOW_SECONDS) * 1_000,
        lease_set2,
    )
    .expect("outbound request")
}

/// Drives side A's bound New Session into side B and returns B's
/// envelope for A after asserting the bound session processed.
fn handshake_a_to_b(a: &mut Side, b: &mut Side, rng_seed: u64) {
    let request = request_for(b"plan296-handshake", Some(a.lease_set2.clone()));
    let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
    let plan = compose_outbound_delivery(
        &a.routing,
        &mut a.session,
        &a.outbound,
        a.identity.id(),
        a.identity.static_secret_bytes(),
        b.remote_hash(),
        &request,
        NOW_SECONDS,
        START_MS,
        &mut rng,
    )
    .expect("bound new session composes");
    let actions = obep_actions(a, &plan);
    let recovered = feed_action(b, &actions[0]);
    let envelope =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    assert!(matches!(envelope.body(), I2npBody::Garlic(_)));
    if let InboundDispatchOutcome::Rejected(error) = b.dispatch(&envelope) {
        panic!("bound new session must dispatch, got {error:?}");
    }
}

/// One bundled New Session Reply carrying two application payloads
/// travels the full stack and queues both payloads in wire order.
#[test]
fn plan296_bundled_reply_delivers_two_payloads_end_to_end() {
    let mut a = Side::new(A_SEED);
    let mut b = Side::new(B_SEED);
    preresolve_remote(&mut a, &b);
    preresolve_remote(&mut b, &a);
    handshake_a_to_b(&mut a, &mut b, 0x2960);

    // Bob's session now holds a provisional responder for Alice, so
    // the reply form is planned: the bundle composes.
    let first = request_for(b"plan296-bundled-first", None);
    let second = request_for(b"plan296-bundled-second", None);
    let mut rng = ChaCha8Rng::seed_from_u64(0x2961);
    let plan = compose_bundled_reply_delivery(
        &b.routing,
        &mut b.session,
        &b.outbound,
        b.identity.id(),
        b.identity.static_secret_bytes(),
        a.remote_hash(),
        &[first, second],
        ReplyBundling::enabled(),
        NOW_SECONDS,
        START_MS,
        &mut rng,
    )
    .expect("bundled reply composes");

    let actions = obep_actions(&b, &plan);
    let recovered = feed_action(&mut a, &actions[0]);
    let envelope =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    match a.dispatch(&envelope) {
        InboundDispatchOutcome::NewSessionReplyProcessed { .. } => {}
        other => panic!("bundled reply must process, got {other:?}"),
    }
    // Both application payloads queue in wire order behind the same
    // short-transport envelope the composer emitted.
    let first_popped = a
        .dispatcher
        .pop_payload(a.identity.id())
        .expect("first bundled payload");
    let second_popped = a
        .dispatcher
        .pop_payload(a.identity.id())
        .expect("second bundled payload");
    assert!(a.dispatcher.pop_payload(a.identity.id()).is_none());
    let first_bytes = request_envelope_bytes(b"plan296-bundled-first");
    let second_bytes = request_envelope_bytes(b"plan296-bundled-second");
    assert_eq!(first_popped.bytes(), first_bytes.as_slice());
    assert_eq!(second_popped.bytes(), second_bytes.as_slice());
}

fn request_envelope_bytes(payload: &[u8]) -> Vec<u8> {
    let request = request_for(payload, None);
    request
        .inner_envelope()
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("envelope encodes")
}

/// The reply form gates bundling against live session state: an
/// Existing Session plan, a disabled policy, and empty/overcount
/// bundles all fail typed without touching the session.
#[test]
fn plan296_bundled_reply_gating_is_typed() {
    let mut a = Side::new(A_SEED);
    let mut b = Side::new(B_SEED);
    preresolve_remote(&mut a, &b);
    preresolve_remote(&mut b, &a);
    handshake_a_to_b(&mut a, &mut b, 0x2962);

    let first = request_for(b"plan296-gate-first", None);
    let second = request_for(b"plan296-gate-second", None);
    let pair = vec![first, second];
    let mut rng = ChaCha8Rng::seed_from_u64(0x2963);
    // Disabled policy never bundles, even with a reply pending.
    assert!(matches!(
        compose_bundled_reply_delivery(
            &b.routing,
            &mut b.session,
            &b.outbound,
            b.identity.id(),
            b.identity.static_secret_bytes(),
            a.remote_hash(),
            &pair,
            ReplyBundling::disabled(),
            NOW_SECONDS,
            START_MS,
            &mut rng,
        ),
        Err(BundleError::PolicyDisabled)
    ));
    // Empty and overcount bundles fail before any session use.
    assert!(matches!(
        compose_bundled_reply_delivery(
            &b.routing,
            &mut b.session,
            &b.outbound,
            b.identity.id(),
            b.identity.static_secret_bytes(),
            a.remote_hash(),
            &[],
            ReplyBundling::enabled(),
            NOW_SECONDS,
            START_MS,
            &mut rng,
        ),
        Err(BundleError::EmptyBundle)
    ));
    let five = vec![
        request_for(b"1", None),
        request_for(b"2", None),
        request_for(b"3", None),
        request_for(b"4", None),
        request_for(b"5", None),
    ];
    assert!(matches!(
        compose_bundled_reply_delivery(
            &b.routing,
            &mut b.session,
            &b.outbound,
            b.identity.id(),
            b.identity.static_secret_bytes(),
            a.remote_hash(),
            &five,
            ReplyBundling::enabled(),
            NOW_SECONDS,
            START_MS,
            &mut rng,
        ),
        Err(BundleError::TooManyCloves {
            actual: 5,
            maximum: 4
        })
    ));
    // Complete the pairing with a single reply: the session is now
    // an Existing Session, which never bundles.
    let single = request_for(b"plan296-gate-single", None);
    let plan = compose_bundled_reply_delivery(
        &b.routing,
        &mut b.session,
        &b.outbound,
        b.identity.id(),
        b.identity.static_secret_bytes(),
        a.remote_hash(),
        &[single],
        ReplyBundling::enabled(),
        NOW_SECONDS,
        START_MS,
        &mut rng,
    )
    .expect("first reply bundles");
    let actions = obep_actions(&b, &plan);
    let recovered = feed_action(&mut a, &actions[0]);
    let envelope =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    match a.dispatch(&envelope) {
        InboundDispatchOutcome::NewSessionReplyProcessed { .. } => {}
        other => panic!("single reply must process, got {other:?}"),
    }
    // Alice answers through the now-paired session: her form is an
    // Existing Session, and Bob's is too after sealing the reply
    // above, so bundling refuses with the planned form named.
    assert!(matches!(
        compose_bundled_reply_delivery(
            &b.routing,
            &mut b.session,
            &b.outbound,
            b.identity.id(),
            b.identity.static_secret_bytes(),
            a.remote_hash(),
            &pair,
            ReplyBundling::enabled(),
            NOW_SECONDS,
            START_MS,
            &mut rng,
        ),
        Err(BundleError::FormNotBundlable { .. })
    ));
}
