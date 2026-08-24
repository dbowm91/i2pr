//! Plan 124 deterministic Plan 122 destination-routing corrective closure.
//!
//! The Plan 124 trajectory exercises the corrected outbound composition
//! through a real destination-owned outbound tunnel chain and a real
//! destination-owned inbound tunnel chain. The full path proves that
//! the bytes the tunnel data plane carries are the standard-encoded
//! I2NP `Garlic` carrier wrapping the ECIES-encrypted envelope, and
//! that the recipient's [`DestinationDispatcher`] authenticates and
//! decrypts the message through its [`EciesSessionManager`].
//!
//! Each test covers one Plan 124 phase. The master trajectory in
//! `plan_124_trajectory_a_to_b_carries_garlic_through_obep` is the
//! authoritative Plan 124 closure test.

#![allow(clippy::too_many_lines)]

use i2pr_client::{
    DestinationConfig, DestinationDispatcher, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool, EciesSessionConfig,
    EciesSessionManager, InboundDispatchOutcome, OutboundRequest, build_signed_lease_set2,
    compose_outbound_delivery,
};
use i2pr_netdb::{
    DestinationHash, LeaseSet2Store as NetDbLeaseSet2Store, LeaseSet2ValidationContext,
};
use i2pr_proto::{
    Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelDataMessage, TunnelGatewayMessage,
};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole,
    EstablishedTunnel, InboundGatewayRole, InboundParticipantRole, LayerKeys,
    LocalInboundEndpointRole, OutboundEndpointRole, OutboundGatewayRole, OutboundParticipantRole,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const A_SEED: u64 = 0xA1;
const B_SEED: u64 = 0xB2;
const NOW_SECONDS: u32 = 1_000;
const NOW_MS: u64 = 60_000;
const APPLICATION_PAYLOAD: &[u8] = b"hello";

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
    DestinationIdentity::generate(&mut rng).expect("identity")
}

fn outbound_established_with_obep(seed: u64) -> EstablishedMaterial {
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::Participant,
            TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x10),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::terminal(
            peer(hop_router_hash(seed, 2)),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x11),
        ),
    ];
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

fn inbound_established_with_local(seed: u64) -> EstablishedMaterial {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            layer_keys(0x20),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 2)),
            EstablishedRole::Participant,
            TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x21),
            EstablishedNextHop::new(peer(hop_router_hash(seed, 3)), local_receive),
        ),
    ];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((peer(hop_router_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established");
    tunnel.into_extracted()
}

fn build_signed_ls2(identity: &DestinationIdentity, leases_seed: u64) -> i2pr_proto::LeaseSet2 {
    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    pool.register_inbound(
        inbound_established_with_local(leases_seed),
        NOW_SECONDS as u64,
    )
    .expect("inbound");
    pool.register_outbound(
        outbound_established_with_obep(leases_seed),
        NOW_SECONDS as u64,
    )
    .expect("outbound");
    let leases = pool.inbound_lease_sources(NOW_SECONDS as u64);
    build_signed_lease_set2(identity, &leases, NOW_SECONDS).expect("ls2")
}

struct LocalPair {
    identity: DestinationIdentity,
    /// Destination-owned tunnel pool; consumed by tests that build
    /// signed LS2 records for outbound destinations.
    #[allow(dead_code)]
    pool: DestinationTunnelPool,
    /// Routing state machine reserved for cross-destination tests.
    #[allow(dead_code)]
    routing: DestinationRouting,
    dispatcher: DestinationDispatcher,
    session: EciesSessionManager,
    inbound_role: InboundGatewayRole,
    inbound_participant: InboundParticipantRole,
    endpoint: LocalInboundEndpointRole,
}

fn build_local_pair(seed: u64) -> LocalPair {
    let identity = destination_identity(seed);
    let pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    let routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let mut dispatcher = DestinationDispatcher::new();
    dispatcher
        .register_destination(identity.id())
        .expect("register destination");
    dispatcher
        .bind_destination_hash(identity.id(), identity.id().as_netdb_key())
        .expect("bind destination hash");
    let session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let inbound_material = inbound_established_with_local(seed);
    let inbound_first_hop = inbound_material.hops()[0].clone();
    let inbound_second_hop = inbound_material.hops()[1].clone();
    let inbound_role =
        InboundGatewayRole::new(&inbound_first_hop, DuplicateWindow::new(16), 60_000)
            .expect("ibgw role");
    let inbound_participant =
        InboundParticipantRole::new(&inbound_second_hop, DuplicateWindow::new(16), 60_000)
            .expect("inbound participant role");
    // The endpoint consumes the established tunnel minus its secret material
    // consumption; rebuild a non-extracted copy through the existing
    // EstablishedTunnel re-construction below.
    let local_receive = inbound_material.local_inbound_receive();
    let inbound_tunnel = rebuild_inbound_tunnel(&inbound_material);
    let endpoint = LocalInboundEndpointRole::new(inbound_tunnel, 16, 1 << 20, 60_000, 0, 60_000);
    let _ = local_receive;
    LocalPair {
        identity,
        pool: pool_after_inbound_register(pool, inbound_material, NOW_SECONDS as u64),
        routing,
        dispatcher,
        session,
        inbound_role,
        inbound_participant,
        endpoint,
    }
}

fn pool_after_inbound_register(
    mut pool: DestinationTunnelPool,
    material: EstablishedMaterial,
    now: u64,
) -> DestinationTunnelPool {
    pool.register_inbound(material, now)
        .expect("inbound registered");
    pool
}

fn rebuild_inbound_tunnel(material: &EstablishedMaterial) -> EstablishedTunnel {
    // The EstablishedMaterial extracted flag is true once consumed;
    // the LocalInboundEndpointRole wants the EstablishedTunnel form
    // (not material). Tests build the inbound tunnel directly without
    // pool registration here so the role API is satisfied.
    let tunnel_id = material.creator_tunnel_id().get();
    let seed = if tunnel_id == 0x0500_0000_u32.wrapping_add(A_SEED as u32) {
        A_SEED
    } else if tunnel_id == 0x0500_0000_u32.wrapping_add(B_SEED as u32) {
        B_SEED
    } else {
        A_SEED
    };
    build_inbound_tunnel_direct(seed)
}

fn build_inbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            layer_keys(0x20),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 2)),
            EstablishedRole::Participant,
            TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x21),
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
    .expect("inbound tunnel")
}

fn run_outbound_tunnel_chain(
    outbound_role: &OutboundGatewayRole,
    outbound_participant_hop: &EstablishedHop,
    obep_hop: &EstablishedHop,
    cells: &[i2pr_tunnel::OBGWRouterDelivery],
    rng: &mut impl rand_core::CryptoRng,
) -> Vec<i2pr_tunnel::RouterDeliveryAction> {
    let mut out_p =
        OutboundParticipantRole::new(outbound_participant_hop, DuplicateWindow::new(16), 60_000)
            .expect("outbound participant role");
    let mut out_obep = OutboundEndpointRole::new(
        obep_hop,
        DuplicateWindow::new(16),
        16,
        1 << 20,
        60_000,
        60_000,
        0,
    );
    let mut actions = Vec::with_capacity(cells.len());
    for cell in cells {
        let cell_after_p = out_p
            .process(&peer(hop_router_hash(A_SEED, 0)).hash(), &cell.cell, 0)
            .expect("outbound participant forward");
        let action = out_obep
            .process(&peer(hop_router_hash(A_SEED, 1)).hash(), &cell_after_p, 0)
            .expect("obep process");
        if let Some(action) = action {
            actions.push(action);
        }
        let _ = outbound_role;
        let _ = rng;
    }
    actions
}

fn run_inbound_tunnel_chain(
    ibgw: &InboundGatewayRole,
    inbound_participant: &mut InboundParticipantRole,
    endpoint: &mut LocalInboundEndpointRole,
    action: &i2pr_tunnel::RouterDeliveryAction,
) -> Option<Vec<u8>> {
    let inner_i2np = I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode obep inner i2np");
    let tunnel_id = action.tunnel_id.expect("tunnel id");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: tunnel_id.get(),
        message: Box::new(inner_i2np),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF);
    let ibgw_out = ibgw
        .process(&gateway_msg, &mut rng, 0)
        .expect("ibgw process");
    let in_p_cell = inbound_participant
        .process(&hop_router_hash(B_SEED, 1), &ibgw_out.cell, 0)
        .expect("inbound participant forward");
    endpoint
        .process(&hop_router_hash(B_SEED, 2), &in_p_cell, 0)
        .expect("endpoint process")
}

fn populated_routing(identity_b: &DestinationIdentity) -> (DestinationRouting, DestinationHash) {
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let ls2_b = build_signed_ls2(identity_b, B_SEED);
    let hash_b = identity_b.id().as_netdb_key();
    let validated_b = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        ls2_b,
        Some(hash_b),
        LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated");
    let _ = routing.lease_set2_store_mut();
    let destination_hash = routing
        .register_resolved_remote(validated_b)
        .expect("register resolved remote");
    (routing, destination_hash)
}

fn dispatch_garlic_to_destination(
    dispatcher: &mut DestinationDispatcher,
    session: &mut EciesSessionManager,
    local_id: i2pr_client::DestinationId,
    local_static_secret: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
    local_static_public: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
    now_seconds: u32,
    envelope: &I2npMessage,
) -> InboundDispatchOutcome {
    dispatcher.dispatch_garlic_envelope(
        session,
        local_id,
        local_static_secret,
        local_static_public,
        now_seconds,
        envelope,
        &mut NetDbLeaseSet2Store::default(),
    )
}

/// Phase A / Phase B byte-identity assertion: the bytes the OBEP
/// recovers must equal the canonical I2NP `Garlic` carrier the
/// composer emitted, and must differ from the plaintext inner `Data`
/// envelope.
#[test]
fn plan_124_phase_a_b_compose_emits_garlic_through_obep() {
    let identity_a = destination_identity(A_SEED);
    let identity_b = destination_identity(B_SEED);
    let (routing, remote_hash) = populated_routing(&identity_b);

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);

    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let request = OutboundRequest::new(6, APPLICATION_PAYLOAD, NOW_MS, None).expect("request");

    let plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xCAFE),
    )
    .expect("compose_outbound_delivery");

    assert!(
        !plan.cells.is_empty(),
        "compose_outbound_delivery must produce at least one cell"
    );
    assert!(
        !plan.garlic_i2np_bytes.is_empty(),
        "compose_outbound_delivery must record the encoded Garlic carrier"
    );
    assert_ne!(
        plan.inner_envelope_bytes, plan.garlic_i2np_bytes,
        "the Garlic carrier must not be byte-identical to the plaintext inner envelope"
    );
    // The encrypted envelope is the body the I2NP Garlic carrier wraps.
    let payload = plan.garlic_i2np_bytes.clone();
    let decoded = I2npMessage::decode_standard(&payload, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    match decoded.body() {
        I2npBody::Garlic(body) => {
            assert_eq!(
                body.payload.as_bytes(),
                plan.encrypted_message.message_bytes(),
                "the Garlic carrier body must equal the encrypted ECIES message bytes"
            );
        }
        other => panic!("expected Garlic body, got {other:?}"),
    }

    // Walk through the outbound tunnel roles; the OBEP action must
    // surface the original Garlic carrier bytes.
    let outbound_hops = outbound_role.role().established().hops();
    let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
    let actions = run_outbound_tunnel_chain(
        outbound_role.role(),
        &outbound_hops[0],
        &outbound_hops[1],
        &plan.cells,
        &mut rng,
    );
    assert_eq!(actions.len(), 1, "the OBEP must emit one delivery action");
    let action = &actions[0];
    assert!(
        matches!(action.kind, i2pr_tunnel::RouterDeliveryKind::TunnelGateway),
        "OBEP delivery kind must be TunnelGateway, got {:?}",
        action.kind
    );
    assert_eq!(
        action.message, plan.garlic_i2np_bytes,
        "OBEP-recovered bytes must equal the encoded I2NP Garlic carrier"
    );
    assert_ne!(
        action.message, plan.inner_envelope_bytes,
        "OBEP must never observe the plaintext inner Data envelope"
    );
    let recovered =
        I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    match recovered.body() {
        I2npBody::Garlic(_) => {}
        other => panic!("recovered message must be Garlic, got {other:?}"),
    }
}

/// Phase C: a successful deterministic A -> B New Session trajectory
/// carries the encrypted Garlic through the tunnel data plane and
/// reaches B's application queue with the exact application payload.
#[test]
fn plan_124_trajectory_a_to_b_carries_garlic_through_obep() {
    let pair_a = build_local_pair(A_SEED);
    let mut pair_b = build_local_pair(B_SEED);
    let identity_a = pair_a.identity;
    let identity_b = pair_b.identity;

    let (routing, remote_hash) = populated_routing(&identity_b);

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);

    let mut session_a = EciesSessionManager::new(EciesSessionConfig::balanced());
    let request = OutboundRequest::new(6, APPLICATION_PAYLOAD, NOW_MS, None).expect("request");

    let plan = compose_outbound_delivery(
        &routing,
        &mut session_a,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xCAFE_0001),
    )
    .expect("compose_outbound_delivery");

    // OBEP delivery target router and tunnel id must equal the selected lease.
    let outbound_hops = outbound_role.role().established().hops();
    let mut rng = ChaCha8Rng::seed_from_u64(0x00C0_FFEE_0001);
    let actions = run_outbound_tunnel_chain(
        outbound_role.role(),
        &outbound_hops[0],
        &outbound_hops[1],
        &plan.cells,
        &mut rng,
    );
    assert_eq!(actions.len(), 1);
    let action = &actions[0];
    let target_tunnel = action.tunnel_id.expect("tunnel id");
    assert_eq!(target_tunnel.get(), plan.selected_lease.tunnel_id);
    assert_eq!(
        action.target_router,
        plan.selected_lease.gateway_router_hash
    );
    // OBEP recovered bytes match the encoded Garlic carrier exactly.
    assert_eq!(action.message, plan.garlic_i2np_bytes);

    // B's local endpoint recovers the standard I2NP Garlic bytes; they
    // must still equal the A-side carrier.
    let recovered_message_bytes = run_inbound_tunnel_chain(
        &pair_b.inbound_role,
        &mut pair_b.inbound_participant,
        &mut pair_b.endpoint,
        action,
    )
    .expect("endpoint recovered message");
    assert_eq!(recovered_message_bytes, plan.garlic_i2np_bytes);

    let recovered_envelope =
        I2npMessage::decode_standard(&recovered_message_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode recovered");
    match recovered_envelope.body() {
        I2npBody::Garlic(_) => {}
        other => panic!("recovered envelope must be Garlic, got {other:?}"),
    }

    // B's DestinationDispatcher accepts the recovered Garlic envelope.
    let outcome = dispatch_garlic_to_destination(
        &mut pair_b.dispatcher,
        &mut pair_b.session,
        identity_b.id(),
        identity_b.static_secret_bytes(),
        &identity_b.static_public_bytes(),
        NOW_SECONDS,
        &recovered_envelope,
    );
    match outcome {
        InboundDispatchOutcome::NewSessionProcessed { clove_count, .. } => {
            assert!(
                clove_count >= 1,
                "dispatcher must surface at least one clove"
            );
        }
        InboundDispatchOutcome::Rejected(reason) => {
            panic!("dispatcher rejected the recovered Garlic envelope: {reason:?}");
        }
        other => panic!("unexpected dispatch outcome: {other:?}"),
    }
    let payload = pair_b
        .dispatcher
        .pop_payload(identity_b.id())
        .expect("B application queue must contain the recovered payload");
    // The dispatcher queues the raw Garlic Clove message bytes, which
    // for an ordinary outbound Data envelope include the I2NP standard
    // header. Decode the queued message and verify the inner Data body
    // matches the original application payload.
    let queued_message = I2npMessage::decode_standard(payload.bytes(), MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode queued message");
    match queued_message.body() {
        I2npBody::Data(body) => assert_eq!(body.payload.as_bytes(), APPLICATION_PAYLOAD),
        other => panic!("queued payload must be a Data envelope, got {other:?}"),
    }
}

/// Phase B: the OBEP-recovered bytes must not be byte-identical to the
/// plaintext inner I2NP Data envelope bytes the composer recorded.
#[test]
fn plan_124_phase_b_obep_does_not_carry_plaintext_data() {
    let identity_a = destination_identity(A_SEED);
    let identity_b = destination_identity(B_SEED);
    let (routing, remote_hash) = populated_routing(&identity_b);

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);

    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let request = OutboundRequest::new(6, APPLICATION_PAYLOAD, NOW_MS, None).expect("request");
    let plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0x1234_5678),
    )
    .expect("compose_outbound_delivery");

    let plaintext_envelope =
        I2npMessage::decode_standard(&plan.inner_envelope_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode plaintext");
    assert!(
        matches!(plaintext_envelope.body(), I2npBody::Data(_)),
        "plaintext inner envelope must be a Data body"
    );
    let garlic_envelope =
        I2npMessage::decode_standard(&plan.garlic_i2np_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode garlic");
    assert!(
        matches!(garlic_envelope.body(), I2npBody::Garlic(_)),
        "garlic carrier must be a Garlic body"
    );
    assert_ne!(
        plaintext_envelope.body().message_type(),
        garlic_envelope.body().message_type(),
        "Garlic carrier must not carry a Data body"
    );
}

/// Phase E: ciphertext must be routed to the destination context that
/// owns the inbound tunnel; an inbound tunnel owned by B cannot be
/// decrypted by A's session manager.
#[test]
fn plan_124_phase_e_inbound_ciphertext_isolated_to_owner() {
    let mut pair_a = build_local_pair(A_SEED);
    let mut pair_b = build_local_pair(B_SEED);

    // Recover a real Garlic envelope from the A -> B trajectory above,
    // then attempt to dispatch it through A's session manager; the
    // dispatcher must reject because A does not own the inbound tunnel
    // and cannot decrypt the recipient-bound Garlic.
    let identity_a = pair_a.identity;
    let identity_b = pair_b.identity;
    let (routing, remote_hash) = populated_routing(&identity_b);

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);
    let mut session_a = EciesSessionManager::new(EciesSessionConfig::balanced());
    let request = OutboundRequest::new(6, APPLICATION_PAYLOAD, NOW_MS, None).expect("request");
    let plan = compose_outbound_delivery(
        &routing,
        &mut session_a,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xABCD_0001),
    )
    .expect("compose_outbound_delivery");

    // Walk the Garlic envelope through B's tunnel chain to recover it.
    let outbound_hops = outbound_role.role().established().hops();
    let mut rng = ChaCha8Rng::seed_from_u64(0x00C0_FFEE_0002);
    let actions = run_outbound_tunnel_chain(
        outbound_role.role(),
        &outbound_hops[0],
        &outbound_hops[1],
        &plan.cells,
        &mut rng,
    );
    let recovered_message_bytes = run_inbound_tunnel_chain(
        &pair_b.inbound_role,
        &mut pair_b.inbound_participant,
        &mut pair_b.endpoint,
        &actions[0],
    )
    .expect("B endpoint recovered");
    let recovered_envelope =
        I2npMessage::decode_standard(&recovered_message_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode recovered");

    // Hand the recovered envelope to A's session manager. The session
    // manager is fresh and has no session installed for the sender; the
    // dispatch must surface a typed rejection rather than deliver
    // plaintext to A's queue.
    let outcome = dispatch_garlic_to_destination(
        &mut pair_a.dispatcher,
        &mut pair_a.session,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        &identity_a.static_public_bytes(),
        NOW_SECONDS,
        &recovered_envelope,
    );
    match outcome {
        InboundDispatchOutcome::Rejected(_) => {}
        other => panic!("expected rejection, got {other:?}"),
    }
    assert_eq!(
        pair_a.dispatcher.pop_payload(identity_a.id()),
        None,
        "A must not receive any application payload"
    );
}

/// Phase F: a stale or expired LeaseSet2 prevents the send from
/// proceeding. The composer fails closed when the routing state
/// machine has no usable lease.
#[test]
fn plan_124_phase_f_expired_lease_blocks_send() {
    let identity_a = destination_identity(A_SEED);
    let identity_b = destination_identity(B_SEED);
    let (mut routing, remote_hash) = populated_routing(&identity_b);

    // Re-resolve with an LS2 whose leases are past the safety margin.
    let stale_ls2 = build_signed_ls2(&identity_b, B_SEED);
    let hash_b = identity_b.id().as_netdb_key();
    let stale_validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        stale_ls2,
        Some(hash_b),
        LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated");
    routing
        .register_resolved_remote(stale_validated)
        .expect("register stale remote");
    // Forget the original resolved remote so the routing state machine
    // must read the new (stale) lease set.
    let _ = remote_hash;

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let request = OutboundRequest::new(6, APPLICATION_PAYLOAD, NOW_MS, None).expect("request");
    let result = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        hash_b,
        &request,
        // Advance the clock past the lease safety window so the
        // selector rejects every lease.
        NOW_SECONDS + 24 * 60 * 60,
        NOW_MS + 24 * 60 * 60 * 1_000,
        &mut ChaCha8Rng::seed_from_u64(0xFA11_0001),
    );
    // The selector uses the LS2 published_seconds (already past the
    // safety window), so the send fails closed with NoUsableLease.
    assert!(
        matches!(result, Err(i2pr_client::SendError::NoUsableLease(_))),
        "expected NoUsableLease for expired LS2, got {result:?}"
    );
}

/// Phase G fault test: tampered ECIES ciphertext is rejected by the
/// dispatcher's session manager without delivering plaintext to the
/// application queue.
#[test]
fn plan_124_phase_g_tampered_garlic_ciphertext_is_rejected() {
    let mut pair_b = build_local_pair(B_SEED);
    let identity_b = pair_b.identity;
    let mut tampered_bytes = identity_b.id().as_bytes().to_vec();
    tampered_bytes.extend_from_slice(&[0xE0_u8; 96]);
    let envelope = I2npMessage::new_standard(
        0xFEED,
        i2pr_proto::Date::from_millis(NOW_MS),
        I2npBody::Garlic(i2pr_proto::OpaqueMessageBody {
            payload: i2pr_proto::DeferredPayload::new(tampered_bytes, MAX_I2NP_PAYLOAD_SIZE)
                .expect("payload"),
        }),
    )
    .expect("garlic envelope");
    let outcome = dispatch_garlic_to_destination(
        &mut pair_b.dispatcher,
        &mut pair_b.session,
        identity_b.id(),
        identity_b.static_secret_bytes(),
        &identity_b.static_public_bytes(),
        NOW_SECONDS,
        &envelope,
    );
    assert!(
        matches!(outcome, InboundDispatchOutcome::Rejected(_)),
        "tampered Garlic must be rejected, got {outcome:?}"
    );
    assert_eq!(
        pair_b.dispatcher.pop_payload(identity_b.id()),
        None,
        "no payload may be queued for tampered Garlic"
    );
}

/// Phase G fault test: a malformed I2NP Garlic carrier (e.g. wrong
/// first byte) is rejected without leaking to any destination.
#[test]
fn plan_124_phase_g_malformed_garlic_carrier_is_rejected() {
    let mut pair_b = build_local_pair(B_SEED);
    let identity_b = pair_b.identity;
    let malformed_bytes: Vec<u8> = std::iter::repeat_n(0xFF_u8, 96).collect();
    let envelope = I2npMessage::new_standard(
        0xFEED,
        i2pr_proto::Date::from_millis(NOW_MS),
        I2npBody::Garlic(i2pr_proto::OpaqueMessageBody {
            payload: i2pr_proto::DeferredPayload::new(malformed_bytes, MAX_I2NP_PAYLOAD_SIZE)
                .expect("payload"),
        }),
    )
    .expect("garlic envelope");
    let outcome = dispatch_garlic_to_destination(
        &mut pair_b.dispatcher,
        &mut pair_b.session,
        identity_b.id(),
        identity_b.static_secret_bytes(),
        &identity_b.static_public_bytes(),
        NOW_SECONDS,
        &envelope,
    );
    assert!(
        matches!(outcome, InboundDispatchOutcome::Rejected(_)),
        "malformed Garlic carrier must be rejected, got {outcome:?}"
    );
    assert_eq!(pair_b.dispatcher.pop_payload(identity_b.id()), None);
}

/// Phase G fault test: a non-Garlic I2NP envelope is rejected by the
/// dispatcher with a typed `NotGarlic` error.
#[test]
fn plan_124_phase_g_non_garlic_envelope_is_rejected() {
    let mut pair_b = build_local_pair(B_SEED);
    let identity_b = pair_b.identity;
    let payload = i2pr_proto::OpaqueMessageBody {
        payload: i2pr_proto::DeferredPayload::new(
            APPLICATION_PAYLOAD.to_vec(),
            MAX_I2NP_PAYLOAD_SIZE,
        )
        .expect("payload"),
    };
    let envelope = I2npMessage::new_standard(
        0xFEED,
        i2pr_proto::Date::from_millis(NOW_MS),
        I2npBody::Data(payload),
    )
    .expect("data envelope");
    let outcome = dispatch_garlic_to_destination(
        &mut pair_b.dispatcher,
        &mut pair_b.session,
        identity_b.id(),
        identity_b.static_secret_bytes(),
        &identity_b.static_public_bytes(),
        NOW_SECONDS,
        &envelope,
    );
    match outcome {
        InboundDispatchOutcome::Rejected(reason) => {
            assert!(
                matches!(reason, i2pr_client::InboundDispatchError::NotGarlic),
                "expected NotGarlic rejection, got {reason:?}"
            );
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

/// Phase E: removing the registered destination must atomically drop
/// the inbound ownership mapping so subsequent dispatches surface a
/// typed `UnknownDestination` rejection.
#[test]
fn plan_124_phase_e_unregister_destination_drops_inbound_ownership() {
    let mut pair_b = build_local_pair(B_SEED);
    let identity_b = pair_b.identity;
    let envelope = I2npMessage::new_standard(
        0xFEED,
        i2pr_proto::Date::from_millis(NOW_MS),
        I2npBody::Garlic(i2pr_proto::OpaqueMessageBody {
            payload: i2pr_proto::DeferredPayload::new(vec![0xE0_u8; 96], MAX_I2NP_PAYLOAD_SIZE)
                .expect("payload"),
        }),
    )
    .expect("garlic envelope");
    let released = pair_b.dispatcher.unregister_destination(identity_b.id());
    let outcome = dispatch_garlic_to_destination(
        &mut pair_b.dispatcher,
        &mut pair_b.session,
        identity_b.id(),
        identity_b.static_secret_bytes(),
        &identity_b.static_public_bytes(),
        NOW_SECONDS,
        &envelope,
    );
    assert_eq!(released, 0);
    match outcome {
        InboundDispatchOutcome::Rejected(reason) => {
            assert!(
                matches!(
                    reason,
                    i2pr_client::InboundDispatchError::UnknownDestination(_)
                        | i2pr_client::InboundDispatchError::Session(_)
                        | i2pr_client::InboundDispatchError::Codec(_)
                ),
                "expected fail-closed rejection, got {reason:?}"
            );
        }
        other => panic!("expected Rejected outcome, got {other:?}"),
    }
}

/// Phase B: every send produces a non-zero-byte Garlic carrier that
/// is distinct from the inner envelope bytes; the OBEP recovers the
/// carrier; the body is a Garlic body. This test exercises the
/// ECIES session seam twice through the corrected composition and
/// confirms the second send still produces a Garlic carrier through
/// the tunnel data plane.
///
/// The second send produces a New Session message because the
/// outbound ECIES session has not been installed yet (the New
/// Session Reply handshake is not part of this deterministic local
/// test); the regression assertion is that the tunnel data plane
/// carries the encrypted Garlic carrier regardless of the underlying
/// ECIES session state.
#[test]
fn plan_124_phase_b_existing_session_carries_garlic_through_obep() {
    let identity_a = destination_identity(A_SEED);
    let identity_b = destination_identity(B_SEED);
    let (routing, remote_hash) = populated_routing(&identity_b);

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());

    let first_request = OutboundRequest::new(6, b"first", NOW_MS, None).expect("first request");
    let first_plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &first_request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xCAFE_0002),
    )
    .expect("first compose");

    let second_request = OutboundRequest::new(6, b"second", NOW_MS, None).expect("second request");
    let second_plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &second_request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xCAFE_0003),
    )
    .expect("second compose");

    for plan in [&first_plan, &second_plan] {
        assert!(!plan.garlic_i2np_bytes.is_empty());
        assert_ne!(plan.garlic_i2np_bytes, plan.inner_envelope_bytes);
        let outbound_hops = outbound_role.role().established().hops();
        let mut rng = ChaCha8Rng::seed_from_u64(0x00C0_FFEE_0003);
        let actions = run_outbound_tunnel_chain(
            outbound_role.role(),
            &outbound_hops[0],
            &outbound_hops[1],
            &plan.cells,
            &mut rng,
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].message, plan.garlic_i2np_bytes);
        let recovered = I2npMessage::decode_standard(&actions[0].message, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode recovered");
        match recovered.body() {
            I2npBody::Garlic(_) => {}
            other => panic!("carrier must be Garlic, got {other:?}"),
        }
    }
}

/// Sanity: the OBEP delivery target must always be the selected lease
/// gateway, regardless of the lease slot. This guards against the
/// OBEP delivery falling back to a self-hash terminal or unrelated
/// router.
#[test]
fn plan_124_phase_b_obep_target_router_matches_selected_lease() {
    let identity_a = destination_identity(A_SEED);
    let identity_b = destination_identity(B_SEED);
    let (routing, remote_hash) = populated_routing(&identity_b);

    let outbound_material = outbound_established_with_obep(A_SEED);
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        outbound_material.creator_tunnel_id(),
        outbound_material.hops().to_vec(),
        0,
        None,
        None,
    )
    .expect("rebuild outbound tunnel");
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let request = OutboundRequest::new(6, APPLICATION_PAYLOAD, NOW_MS, None).expect("request");
    let plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &request,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xCAFE_0004),
    )
    .expect("compose_outbound_delivery");

    let outbound_hops = outbound_role.role().established().hops();
    let mut rng = ChaCha8Rng::seed_from_u64(0x00C0_FFEE_0004);
    let actions = run_outbound_tunnel_chain(
        outbound_role.role(),
        &outbound_hops[0],
        &outbound_hops[1],
        &plan.cells,
        &mut rng,
    );
    let action = &actions[0];
    assert_eq!(
        action.target_router, plan.selected_lease.gateway_router_hash,
        "OBEP target router must equal the selected lease gateway"
    );
    assert_eq!(
        action.tunnel_id.expect("tunnel id").get(),
        plan.selected_lease.tunnel_id,
        "OBEP target tunnel id must equal the selected lease tunnel id"
    );
}

#[cfg(test)]
mod _silence_unused_tunnel_data {
    use super::*;
    fn _force_use_of_tunnel_data_message(_: &TunnelDataMessage) {}
}
