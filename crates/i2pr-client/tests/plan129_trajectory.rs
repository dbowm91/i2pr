//! Plan 129 — Milestone 6 integrated destination + Streaming final
//! gate.
//!
//! The authoritative path exercised by every trajectory:
//!
//! ```text
//! Streaming packet (Plan 128 wire form)
//!  -> canonical I2P protocol-6 gzip ClientPayload
//!  -> I2NP Data
//!  -> corrected bound ECIES NS / NSR / ES (Plans 126/127)
//!  -> I2NP Garlic
//!  -> local destination outbound tunnel
//!  -> outbound participant(s) -> OBEP TUNNEL(remote gateway, tunnel id)
//!  -> authenticated-router-link-bypassed-local-seam
//!  -> remote inbound gateway -> inbound participant(s)
//!  -> local inbound endpoint
//!  -> remote Destination owner (DestinationDispatcher)
//!  -> corrected ECIES tag/session authentication
//!  -> I2NP Data
//!  -> canonical gzip ClientPayload decode
//!  -> protocol == 6, destination ports
//!  -> destination-port / local Streaming listener
//!  -> Streaming packet
//! ```
//!
//! `TransportSendRequest` is never transferred directly between
//! Streaming managers: every delivery crosses the real destination
//! stack through [`StreamingDestinationAdapter::send`] and the Plan
//! 129 inbound adapter. Faults are injected only at the post-OBEP
//! router-delivery seam, preserving the protocol work under test. No
//! sockets, no DNS, no external reference; mixed-router
//! interoperability is not claimed.

#![allow(clippy::too_many_lines)]

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination, StreamingManager,
};
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    DestinationConfig, DestinationDispatcher, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool, EciesSessionConfig,
    EciesSessionManager, InboundDispatchError, InboundDispatchOutcome, InboundStreamingOutcome,
    MAX_STREAMING_ADAPTER_PAYLOAD_BYTES, OutboundDeliveryPlan, PlannedOutboundForm,
    StreamingAdapterError, StreamingDestinationAdapter, build_signed_lease_set2,
};
use i2pr_crypto::X25519_KEY_LENGTH;
use i2pr_netdb::ValidatedLeaseSet2;
use i2pr_proto::streaming::{FLAG_NO_ACK, decode_client_payload, encode_client_payload};
use i2pr_proto::{Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelGatewayMessage};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    InboundGatewayRole, InboundParticipantRole, LayerKeys, LocalInboundEndpointRole,
    OutboundEndpointRole, OutboundParticipantRole, RouterDeliveryAction, RouterDeliveryKind,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const A_SEED: u64 = 0x129A;
const B_SEED: u64 = 0x129B;
const NOW_SECONDS: u32 = 5_200;
const START_MS: u64 = 400_000;
const PORT_A: u16 = 0x12A0;
const PORT_B: u16 = 0x12B0;
/// Initial retransmission timeout from the streaming config.
const INITIAL_RTO_MS: u64 = 5_000;

// ---- Deterministic fixture helpers ----

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

fn remote_for(identity: &DestinationIdentity) -> RemoteDestination {
    RemoteDestination {
        destination_hash: *identity.id().as_hash().as_bytes(),
        signing_public_key: identity.destination().signing_key().clone(),
        static_public_key: identity.static_public_bytes(),
    }
}

/// Deterministic pseudo-random application bytes with position
/// encoding so ordering faults are observable.
fn pattern_bytes(label: u8, len: usize) -> Vec<u8> {
    let mut state = u32::from(label).wrapping_mul(0x9E37_79B9) | 1;
    std::iter::repeat_with(|| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state & 0xff) as u8
    })
    .take(len)
    .collect()
}

/// One local destination side owning every integrated surface:
/// identity, signed current LeaseSet2, explicit tunnel roles,
/// routing, ECIES session manager, dispatcher, StreamingManager, and
/// the outbound/inbound adapters applied at every boundary.
struct Side {
    seed: u64,
    identity: DestinationIdentity,
    lease_set2: i2pr_proto::LeaseSet2,
    routing: DestinationRouting,
    dispatcher: DestinationDispatcher,
    session: EciesSessionManager,
    outbound: DestinationOutboundRole,
    inbound: InboundChain,
    streaming: StreamingManager,
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
            streaming: StreamingManager::new(
                i2pr_client::streaming::config::StreamingConfig::balanced(),
            ),
        }
    }

    fn hash_bytes(&self) -> [u8; 32] {
        *self.identity.id().as_hash().as_bytes()
    }

    fn static_public(&self) -> [u8; X25519_KEY_LENGTH] {
        self.identity.static_public_bytes()
    }

    /// Composes one outbound delivery plan through the Plan 129
    /// outbound adapter boundary.
    fn send_via_adapter(
        &mut self,
        request: &TransportSendRequest,
        rng_seed: u64,
        now_ms: u64,
    ) -> Result<OutboundDeliveryPlan, StreamingAdapterError> {
        let Side {
            routing,
            session,
            outbound,
            identity,
            lease_set2,
            ..
        } = self;
        let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
        StreamingDestinationAdapter::send(
            request,
            routing,
            session,
            outbound,
            identity.id(),
            identity.static_secret_bytes(),
            lease_set2,
            NOW_SECONDS,
            now_ms,
            &mut rng,
        )
    }

    /// Dispatches one recovered Garlic envelope through this side's
    /// owner/session/routing surfaces.
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

    /// Runs the Plan 130 §8 inbound adapter against the next queued
    /// dispatcher payload. The adapter derives both wire ports from
    /// the decoded ClientPayload itself; no caller-side listener port
    /// exists anymore.
    fn receive_next_payload(
        &mut self,
        from_destination_hash: &[u8; 32],
        now_ms: u64,
    ) -> Result<InboundStreamingOutcome, StreamingAdapterError> {
        let payload = self
            .dispatcher
            .pop_payload(self.identity.id())
            .expect("dispatcher payload for the owning destination");
        let Side {
            streaming,
            identity,
            ..
        } = self;
        StreamingDestinationAdapter::receive(
            payload.bytes(),
            identity,
            streaming,
            from_destination_hash,
            now_ms,
        )
    }
}

// ---- Post-OBEP seam primitives ----

/// Runs the sender's real outbound roles (participant + OBEP) over
/// every plan cell and returns the post-OBEP router-delivery actions.
/// Asserts the OBEP target equals the selected Lease2 and that the
/// carried bytes are exactly the standard-encoded Garlic carrier.
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
    // OBEP TUNNEL(remote gateway, remote tunnel id): exact selected
    // Lease2 targeting.
    assert_eq!(
        action.target_router,
        plan.selected_lease.gateway_router_hash
    );
    assert_eq!(
        action.tunnel_id.expect("tunnel id").get(),
        plan.selected_lease.tunnel_id
    );
    assert!(matches!(action.kind, RouterDeliveryKind::TunnelGateway));
    // authenticated-router-link-bypassed-local-seam: exact bytes.
    assert_eq!(action.message, plan.garlic_i2np_bytes);
    actions
}

/// Crosses the router-link seam into the receiver's real inbound
/// chain (IBGW -> participant -> local endpoint) and returns the
/// recovered I2NP carrier. The seam never decrypts, re-encrypts, or
/// rewrites the target gateway or tunnel id. Large Garlic carriers
/// fragment across multiple inbound TunnelData cells exactly like the
/// production IBGW path.
///
/// Plan 130 §9 F1: the inbound roles are created **once** per side,
/// so their duplicate windows stay live across every ordinary
/// delivery. Ordinary deliveries never rebuild tunnel state.
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
    run_inbound_cells(receiver, &cells).expect("endpoint recovered the Garlic carrier")
}

/// Feeds every inbound cell through the participant hop and the local
/// endpoint reassembler, returning the reconstructed carrier.
fn run_inbound_cells(receiver: &mut Side, cells: &[i2pr_tunnel::OutboundCell]) -> Option<Vec<u8>> {
    let mut recovered = None;
    for cell in cells {
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
    recovered
}

fn recovered_envelope(recovered: Vec<u8>) -> I2npMessage {
    let message =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    assert!(matches!(message.body(), I2npBody::Garlic(_)));
    message
}

fn expect_processed_ok(outcome: InboundDispatchOutcome) -> InboundDispatchOutcome {
    if let InboundDispatchOutcome::Rejected(error) = &outcome {
        panic!("expected processed dispatch outcome, got rejection: {error:?}");
    }
    outcome
}

/// Delivers one composed plan across the full integrated stack:
/// adapter -> tunnels -> OBEP -> seam -> inbound chain -> dispatcher
/// -> inbound adapter -> streaming manager. Returns the emitted plan
/// and the final inbound adapter outcome.
fn pipe(
    sender: &mut Side,
    receiver: &mut Side,
    request: &TransportSendRequest,
    rng_seed: u64,
    now_ms: u64,
) -> (OutboundDeliveryPlan, InboundStreamingOutcome) {
    let plan = sender
        .send_via_adapter(request, rng_seed, now_ms)
        .expect("outbound adapter composition");
    let actions = obep_actions(sender, &plan);
    let recovered = feed_action(receiver, &actions[0]);
    let envelope = recovered_envelope(recovered);
    expect_processed_ok(receiver.dispatch(&envelope));
    let outcome = receiver
        .receive_next_payload(&sender.hash_bytes(), now_ms)
        .expect("inbound adapter protocol-6 dispatch");
    (plan, outcome)
}

/// Registers `remote`'s validated LeaseSet2 in `side`'s routing state
/// ahead of first contact.
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

/// Drives the complete integrated SYN / SYN-response handshake and
/// returns the established connection ids. Every hop crosses the full
/// destination stack; intermediate forms are asserted on the way.
fn establish_stream(
    a: &mut Side,
    b: &mut Side,
    advertised_a: u16,
    advertised_b: u16,
    clock: &mut u64,
    seed_base: u64,
) -> (ConnectionId, ConnectionId) {
    preresolve_remote(a, b);

    // A initiates.
    *clock += 10;
    let connect = {
        let remote_b = remote_for(&b.identity);
        a.streaming
            .connect(
                &a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                advertised_a,
                *clock,
                &mut ChaCha8Rng::seed_from_u64(seed_base),
            )
            .expect("a connect")
    };
    let ConnectOutcome::SynSent {
        connection_id: a_conn,
        ..
    } = connect
    else {
        panic!("expected SynSent, got {connect:?}");
    };
    assert_eq!(
        a.streaming.get_connection(a_conn).expect("a conn").state(),
        ConnectionState::OutboundSynSent
    );

    let syn_requests = a.streaming.drain_outbound();
    assert_eq!(syn_requests.len(), 1);
    // §5: the first contact's destination ECIES form is New Session;
    // a subsequent stream between already-paired destinations rides
    // the Existing Session form. Either way no plaintext Streaming
    // bytes cross the router-link seam.
    let paired_already = a
        .session
        .planned_outbound_form(&b.static_public(), NOW_SECONDS)
        == PlannedOutboundForm::ExistingSession;
    let expected_first_form = if paired_already {
        "existing-session"
    } else {
        "new-session"
    };
    let syn_wire = decode_client_payload(
        &syn_requests[0].application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("syn client payload")
    .payload;
    let (plan_ns, outcome) = pipe(a, b, &syn_requests[0], seed_base + 1, *clock);
    assert_eq!(plan_ns.encrypted_message.form_name(), expected_first_form);
    match outcome {
        InboundStreamingOutcome::StreamingDispatched {
            source_port,
            destination_port,
            ..
        } => {
            assert_eq!(source_port, PORT_A);
            assert_eq!(destination_port, PORT_B);
        }
        other => panic!("expected StreamingDispatched, got {other:?}"),
    }
    assert!(
        !plan_ns
            .garlic_i2np_bytes
            .windows(syn_wire.len())
            .any(|window| window == syn_wire.as_slice()),
        "the plaintext Streaming packet must never cross the router-link seam"
    );
    assert!(
        !plan_ns
            .garlic_i2np_bytes
            .windows(syn_requests[0].application_payload.len())
            .any(|window| window == syn_requests[0].application_payload.as_slice()),
        "the gzip-encoded client payload must never cross the seam unencrypted"
    );

    // B has exactly one pending inbound stream on its listening port.
    assert_eq!(b.streaming.listener_backlog(PORT_B), 1);
    let b_conn = b.streaming.accept(PORT_B).expect("pending inbound stream");
    assert_eq!(
        b.streaming.get_connection(b_conn).expect("b conn").state(),
        ConnectionState::InboundSynReceived
    );
    assert_eq!(
        a.streaming.get_connection(a_conn).expect("a conn").state(),
        ConnectionState::OutboundSynSent,
        "A remains OutboundSynSent until the signed response arrives"
    );

    // Reverse-routing handoff: install the validated sender LS2 so B
    // can route the reply before emitting it (§5).
    let validated_a = match b
        .dispatcher
        .accepted_lease_set2_for(b.identity.id(), a.identity.id().as_netdb_key())
    {
        Some(validated) => validated.clone(),
        None => panic!("B must have bound A's bundled LS2"),
    };
    let installed = b
        .routing
        .install_remote_lease_set2(validated_a)
        .expect("install validated sender ls2");
    assert_eq!(installed, a.identity.id().as_netdb_key());
    assert_eq!(
        b.routing
            .remote_static_public_key(a.identity.id().as_netdb_key())
            .expect("a static key resolved for reverse routing"),
        a.static_public()
    );

    // B emits the canonical SYN response through the retained reply
    // context.
    *clock += 10;
    let response_request = {
        let remote_a = remote_for(&a.identity);
        b.streaming
            .accept_inbound_syn(
                &b.identity,
                &remote_a,
                b_conn,
                PORT_B,
                PORT_A,
                advertised_b,
                *clock,
                &mut ChaCha8Rng::seed_from_u64(seed_base + 2),
            )
            .expect("b accept inbound syn")
    };
    if !paired_already {
        assert_eq!(
            b.session
                .planned_outbound_form(&a.static_public(), NOW_SECONDS),
            PlannedOutboundForm::NewSessionReply,
            "B's first reply must use the retained New Session Reply context"
        );
    }
    // §6 wire assertions on the response: zero replay NACKs, NO_ACK
    // clear.
    let response_envelope = decode_client_payload(
        &response_request.application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("response client payload");
    let (response_packet, _) = i2pr_proto::streaming::decode_streaming_packet(
        &response_envelope.payload,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    )
    .expect("decode response packet");
    assert!(
        response_packet.nacks.is_empty(),
        "SYN response must not carry replay NACKs"
    );
    assert!(
        response_packet.flags.bits() & FLAG_NO_ACK == 0,
        "SYN response must not set NO_ACK"
    );

    let (plan_reply, _) = pipe(b, a, &response_request, seed_base + 3, *clock);
    if !paired_already {
        assert_eq!(
            plan_reply.encrypted_message.form_name(),
            "new-session-reply",
            "B's first reply ECIES form must be New Session Reply"
        );
    }
    // A becomes Established only after this exact reverse path.
    assert_eq!(
        a.streaming.get_connection(a_conn).expect("a conn").state(),
        ConnectionState::Established
    );
    assert_eq!(
        b.streaming.get_connection(b_conn).expect("b conn").state(),
        ConnectionState::Established,
        "B must be accepted/Established according to the local state model"
    );
    // Negotiated payload max = min(A advertised, B advertised).
    let expected_negotiated = advertised_a.min(advertised_b);
    assert_eq!(
        a.streaming
            .get_connection(a_conn)
            .expect("a conn")
            .max_payload_size(),
        u32::from(expected_negotiated)
    );
    assert_eq!(
        b.streaming
            .get_connection(b_conn)
            .expect("b conn")
            .max_payload_size(),
        u32::from(expected_negotiated)
    );
    (a_conn, b_conn)
}

// ---- §4/§5/§6/§7 master trajectories ----

#[test]
fn plan_129_master_handshake_both_directions_through_full_stack() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;

    let (a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        1400,
        &mut clock,
        0x1290_0000,
    );
    let _ = (a_conn, b_conn);

    // §7 steady-state Existing Session data both directions with
    // nontrivial ordered payloads.
    let chunks_a = [
        pattern_bytes(1, 900),
        pattern_bytes(2, 1100),
        pattern_bytes(3, 700),
    ];
    let chunks_b = [
        pattern_bytes(4, 800),
        pattern_bytes(5, 1300),
        pattern_bytes(6, 500),
    ];

    for (index, chunk) in chunks_a.iter().enumerate() {
        clock += 20;
        let request = {
            let remote_b = remote_for(&side_b.identity);
            side_a
                .streaming
                .send_data(
                    a_conn,
                    &side_a.identity,
                    &remote_b,
                    PORT_A,
                    PORT_B,
                    chunk,
                    clock,
                )
                .expect("a send data")
        };
        let (plan, outcome) = pipe(
            &mut side_a,
            &mut side_b,
            &request,
            0x1290_0100 + index as u64,
            clock,
        );
        assert_eq!(
            plan.encrypted_message.form_name(),
            "existing-session",
            "every steady-state packet must be Existing Session"
        );
        assert!(matches!(
            outcome,
            InboundStreamingOutcome::StreamingDispatched { .. }
        ));
    }
    let delivered_to_b = side_b.streaming.drain_delivered();
    let mut received_by_b = Vec::new();
    for event in &delivered_to_b {
        received_by_b.extend_from_slice(&event.bytes);
    }
    let mut expected_for_b = Vec::new();
    for chunk in &chunks_a {
        expected_for_b.extend_from_slice(chunk);
    }
    assert_eq!(
        received_by_b, expected_for_b,
        "B observes exact ordered bytes"
    );

    for (index, chunk) in chunks_b.iter().enumerate() {
        clock += 20;
        let request = {
            let remote_a = remote_for(&side_a.identity);
            side_b
                .streaming
                .send_data(
                    b_conn,
                    &side_b.identity,
                    &remote_a,
                    PORT_B,
                    PORT_A,
                    chunk,
                    clock,
                )
                .expect("b send data")
        };
        let (plan, _) = pipe(
            &mut side_b,
            &mut side_a,
            &request,
            0x1290_0200 + index as u64,
            clock,
        );
        assert_eq!(plan.encrypted_message.form_name(), "existing-session");
    }
    let delivered_to_a = side_a.streaming.drain_delivered();
    let mut received_by_a = Vec::new();
    for event in &delivered_to_a {
        received_by_a.extend_from_slice(&event.bytes);
    }
    let mut expected_for_a = Vec::new();
    for chunk in &chunks_b {
        expected_for_a.extend_from_slice(chunk);
    }
    assert_eq!(
        received_by_a, expected_for_a,
        "A observes exact ordered bytes"
    );

    // No additional bound New Session while the established session
    // is healthy.
    assert_eq!(side_a.session.pending_handshake_count(), 0);
    assert_eq!(side_b.session.pending_handshake_count(), 0);
    assert_eq!(
        side_a
            .session
            .planned_outbound_form(&side_b.static_public(), NOW_SECONDS),
        PlannedOutboundForm::ExistingSession
    );
    assert_eq!(
        side_b
            .session
            .planned_outbound_form(&side_a.static_public(), NOW_SECONDS),
        PlannedOutboundForm::ExistingSession
    );
}

// ---- §8 fault injection at the post-OBEP seam ----

#[test]
fn plan_129_integrated_drop_causes_real_retransmission_and_exact_once_delivery() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_1000,
    );

    let alpha = pattern_bytes(11, 600);
    let beta = pattern_bytes(12, 650);

    // Two typed router deliveries; drop the second after real OBEP
    // processing.
    let mut requests: Vec<TransportSendRequest> = Vec::new();
    for payload in [&alpha, &beta] {
        clock += 5;
        let remote_b = remote_for(&side_b.identity);
        requests.push(
            side_a
                .streaming
                .send_data(
                    a_conn,
                    &side_a.identity,
                    &remote_b,
                    PORT_A,
                    PORT_B,
                    payload,
                    clock,
                )
                .expect("send data"),
        );
    }

    let plans: Vec<OutboundDeliveryPlan> = requests
        .iter()
        .enumerate()
        .map(|(index, request)| {
            side_a
                .send_via_adapter(request, 0x1290_1100 + index as u64, clock)
                .expect("adapter composition")
        })
        .collect();
    let actions_first = obep_actions(&side_a, &plans[0]);
    // The second action is dropped at the seam after real OBEP
    // processing; it is deliberately never fed to B.
    let _actions_dropped = obep_actions(&side_a, &plans[1]);

    // First delivery lands; the second action is dropped at the seam.
    let recovered = feed_action(&mut side_b, &actions_first[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("first delivery reaches B");

    // Advance the ManualClock past the retransmission deadline.
    clock += INITIAL_RTO_MS + 200;
    let retransmits = side_a.streaming.poll_retransmits(clock);
    assert_eq!(retransmits.len(), 2, "both unacked packets expire");
    // Retransmissions carry the exact original client-payload bytes.
    assert_eq!(
        retransmits[0].application_payload,
        requests[0].application_payload
    );
    assert_eq!(
        retransmits[1].application_payload,
        requests[1].application_payload
    );

    // Each retransmission again traverses gzip -> ECIES -> tunnel.
    let retransmit_plans: Vec<OutboundDeliveryPlan> = retransmits
        .iter()
        .enumerate()
        .map(|(index, request)| {
            side_a
                .send_via_adapter(request, 0x1290_1200 + index as u64, clock)
                .expect("retransmit adapter composition")
        })
        .collect();
    assert_eq!(
        retransmit_plans[0].encrypted_message.form_name(),
        "existing-session"
    );

    // Retransmitted seq 0 is a duplicate at the Streaming layer: no
    // second delivery of the application bytes.
    let dup_actions = obep_actions(&side_a, &retransmit_plans[0]);
    let recovered_dup = feed_action(&mut side_b, &dup_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered_dup)));
    let dup_outcome = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("retransmitted seq0 decrypts");
    assert!(matches!(
        dup_outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));

    // Retransmitted seq 1 delivers the dropped bytes once.
    let retry_actions = obep_actions(&side_a, &retransmit_plans[1]);
    let recovered_retry = feed_action(&mut side_b, &retry_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered_retry)));
    side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("retransmitted seq1 delivers");

    let delivered = side_b.streaming.drain_delivered();
    let mut received = Vec::new();
    for event in &delivered {
        received.extend_from_slice(&event.bytes);
    }
    let mut expected = alpha.clone();
    expected.extend_from_slice(&beta);
    assert_eq!(
        received, expected,
        "receiver delivers each byte exactly once"
    );
    let b_conn_state = side_b.streaming.get_connection(b_conn).expect("b conn");
    assert_eq!(b_conn_state.recv_window().next_expected(), 3);

    // ACK eventually clears the tracked packets: B's reverse traffic
    // carries the cumulative acknowledgement.
    clock += 20;
    let ack_carrier = {
        let remote_a = remote_for(&side_a.identity);
        side_b
            .streaming
            .send_data(
                b_conn,
                &side_b.identity,
                &remote_a,
                PORT_B,
                PORT_A,
                b"ack-carrier",
                clock,
            )
            .expect("b reply")
    };
    let (ack_plan, _) = pipe(&mut side_b, &mut side_a, &ack_carrier, 0x1290_1300, clock);
    assert_eq!(ack_plan.encrypted_message.form_name(), "existing-session");
    let _ = side_a.streaming.drain_delivered();
    assert_eq!(
        side_a.streaming.tracked_retransmit_count(),
        0,
        "the cumulative ACK must clear the tracked retransmission records"
    );
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .send_window()
            .unacked_count(),
        0
    );
}

#[test]
fn plan_129_integrated_duplicate_is_idempotent_and_state_stays_healthy() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_2000,
    );

    let payload = pattern_bytes(21, 500);
    clock += 10;
    let request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                a_conn,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                &payload,
                clock,
            )
            .expect("send")
    };
    let plan = side_a
        .send_via_adapter(&request, 0x1290_2100, clock)
        .expect("compose");
    let actions = obep_actions(&side_a, &plan);

    // Duplicate the exact router-delivery action at the same seam.
    let first = feed_action(&mut side_b, &actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(first)));
    side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("first copy delivers");

    // The replayed tunnel cell is rejected by the persistent inbound
    // duplicate window before any ECIES work: the tolerant feed
    // recovers no carrier and nothing reaches the dispatcher
    // (Plan 130 §9 F2.1 — tunnel-layer evidence).
    let second = feed_action_replay_tolerant(&mut side_b, &actions[0]);
    assert!(
        second.is_none(),
        "the exact tunnel replay must be suppressed by the live duplicate window"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );

    // An ECIES-level duplicate (fresh seal of the same Streaming
    // packet) reaches the Streaming layer and is deduplicated by
    // sequence: still exactly one delivery.
    let reseal_plan = side_a
        .send_via_adapter(&request, 0x1290_2200, clock)
        .expect("fresh seal of the same streaming packet");
    let reseal_actions = obep_actions(&side_a, &reseal_plan);
    assert_ne!(
        reseal_plan.garlic_i2np_bytes, plan.garlic_i2np_bytes,
        "a fresh seal must produce a distinct ECIES envelope"
    );
    let recovered_reseal = feed_action(&mut side_b, &reseal_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered_reseal)));
    let reseal_outcome = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("fresh-seal duplicate decrypts");
    assert!(matches!(
        reseal_outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));

    // Exactly one delivery of the application bytes across the whole
    // trajectory: neither duplicate copy delivered a second time.
    let delivered = side_b.streaming.drain_delivered();
    assert_eq!(
        delivered.len(),
        1,
        "the application bytes must surface exactly once"
    );
    assert_eq!(delivered[0].bytes, payload);
    let b_state = side_b.streaming.get_connection(b_conn).expect("b conn");
    assert_eq!(b_state.recv_window().next_expected(), 2);
    assert_eq!(b_state.recv_window().delivered_count(), 1);
    assert_eq!(
        b_state.state(),
        ConnectionState::Established,
        "state remains healthy"
    );
}

/// Feeds an action like [`feed_action`] but tolerates the persistent
/// inbound chain rejecting the delivery (tunnel duplicate window or
/// reassembler); returns the recovered carrier only when the whole
/// chain accepts it.
fn feed_action_replay_tolerant(
    receiver: &mut Side,
    action: &RouterDeliveryAction,
) -> Option<Vec<u8>> {
    let inner_i2np = I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE).ok()?;
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: action.tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner_i2np),
    };
    // Same deterministic cell-builder seed as `feed_action`: an
    // identical action regenerates byte-identical TunnelData cells,
    // so an exact router-delivery replay hits the live duplicate
    // window rather than being masked by fresh randomness.
    let mut rng = ChaCha8Rng::seed_from_u64(0x51EA);
    let cells = receiver
        .inbound
        .ibgw
        .process_cells(&gateway_msg, &mut rng, 0)
        .ok()?;
    let mut recovered = None;
    for cell in cells {
        let forwarded = receiver
            .inbound
            .participant
            .process(&hop_router_hash(receiver.seed, 1), &cell.cell, 0)
            .ok()?;
        if let Some(message) = receiver
            .inbound
            .endpoint
            .process(&hop_router_hash(receiver.seed, 2), &forwarded, 0)
            .expect("local endpoint process")
        {
            recovered = Some(message);
        }
    }
    recovered
}

#[test]
fn plan_129_integrated_reorder_yields_original_application_byte_order() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_3000,
    );

    let first = pattern_bytes(31, 520);
    let second = pattern_bytes(32, 530);
    let mut held_actions: Vec<RouterDeliveryAction> = Vec::new();
    for (index, payload) in [&first, &second].into_iter().enumerate() {
        clock += 10;
        let request = {
            let remote_b = remote_for(&side_b.identity);
            side_a
                .streaming
                .send_data(
                    a_conn,
                    &side_a.identity,
                    &remote_b,
                    PORT_A,
                    PORT_B,
                    payload,
                    clock,
                )
                .expect("send")
        };
        let plan = side_a
            .send_via_adapter(&request, 0x1290_3100 + index as u64, clock)
            .expect("compose");
        held_actions.extend(obep_actions(&side_a, &plan));
    }
    assert!(
        held_actions.len() >= 2,
        "two queued post-OBEP actions required"
    );

    // Deliver in reverse order.
    for action in held_actions.iter().rev() {
        let recovered = feed_action(&mut side_b, action);
        expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
        side_b
            .receive_next_payload(&side_a.hash_bytes(), clock)
            .expect("each reordered delivery decrypts");
    }

    // The receiver reorders by Streaming sequence and the application
    // observes the original byte order; NACK/ACK state converges.
    let delivered = side_b.streaming.drain_delivered();
    let mut received = Vec::new();
    for event in &delivered {
        received.extend_from_slice(&event.bytes);
    }
    let mut expected = first;
    expected.extend_from_slice(&second);
    assert_eq!(received, expected);
    let b_state = side_b.streaming.get_connection(b_conn).expect("b conn");
    assert_eq!(b_state.recv_window().reorder_count(), 0);
    assert_eq!(b_state.recv_window().next_expected(), 3);
    // After full in-order convergence the reference ack view carries
    // the highest received sequence and zero NACKs.
    let (ack_through, nacks) = b_state.recv_window().ack_view();
    assert_eq!(ack_through, 2);
    assert!(nacks.is_empty());
}

// ---- §9 corruption tests at protocol-appropriate layers ----

#[test]
fn plan_129_invalid_streaming_signature_rejected_after_valid_destination_delivery() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());

    preresolve_remote(&mut side_a, &side_b);
    let clock = START_MS;
    let connect = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .connect(
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                clock,
                &mut ChaCha8Rng::seed_from_u64(0x1290_4000),
            )
            .expect("connect")
    };
    let ConnectOutcome::SynSent { .. } = connect else {
        panic!("expected SynSent");
    };
    let syn = side_a.streaming.drain_outbound();
    assert_eq!(syn.len(), 1);

    // Corrupt the signed control packet BEFORE destination
    // encryption: flip one byte inside the raw final signature (the
    // last option field, hence the tail of the Streaming packet).
    let mut envelope = decode_client_payload(
        &syn[0].application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("client payload");
    let last = envelope.payload.len() - 1;
    envelope.payload[last] ^= 0x01;
    let corrupted_payload = encode_client_payload(&envelope).expect("re-encode");
    let corrupted_request = TransportSendRequest {
        application_payload: corrupted_payload,
        ..syn[0].clone()
    };

    // Full destination path: ECIES succeeds, gzip succeeds, then the
    // Streaming signature verification fails.
    let plan = side_a
        .send_via_adapter(&corrupted_request, 0x1290_4100, clock)
        .expect("ECIES/tunnel composition still succeeds");
    let actions = obep_actions(&side_a, &plan);
    let recovered = feed_action(&mut side_b, &actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    let error = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect_err("signature verification must fail");
    assert!(
        matches!(error, StreamingAdapterError::Streaming(_)),
        "expected a streaming-layer rejection, got {error:?}"
    );

    // No connection state transition, no app data, no pending
    // inbound stream.
    assert_eq!(side_b.streaming.connection_count(), 0);
    assert_eq!(side_b.streaming.listener_backlog(PORT_B), 0);
    assert!(side_b.streaming.drain_delivered().is_empty());
}

#[test]
fn plan_129_bad_gzip_crc_rejected_before_streaming_processing() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());

    preresolve_remote(&mut side_a, &side_b);
    let clock = START_MS;
    let connect = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .connect(
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                clock,
                &mut ChaCha8Rng::seed_from_u64(0x1290_5000),
            )
            .expect("connect")
    };
    let ConnectOutcome::SynSent { .. } = connect else {
        panic!("expected SynSent");
    };
    let syn = side_a.streaming.drain_outbound();

    // Corrupt the gzip CRC trailer (last 8 bytes: CRC32 LE || ISIZE
    // LE), then encrypt/tunnel the malformed payload normally.
    let mut malformed = syn[0].application_payload.clone();
    let crc_byte = malformed.len() - 6;
    malformed[crc_byte] ^= 0x80;
    let corrupted_request = TransportSendRequest {
        application_payload: malformed,
        ..syn[0].clone()
    };

    let plan = side_a
        .send_via_adapter(&corrupted_request, 0x1290_5100, clock)
        .expect("ECIES succeeds over arbitrary bounded bytes");
    let actions = obep_actions(&side_a, &plan);
    let recovered = feed_action(&mut side_b, &actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    let error = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect_err("gzip CRC mismatch must fail typed");
    match error {
        StreamingAdapterError::ClientPayload(
            i2pr_proto::streaming::ClientPayloadDecodeError::InvalidCrc { .. },
        ) => {}
        other => panic!("expected InvalidCrc, got {other:?}"),
    }
    // The StreamingManager never saw a packet.
    assert_eq!(side_b.streaming.connection_count(), 0);
    assert!(side_b.streaming.drain_delivered().is_empty());
}

#[test]
fn plan_129_ecies_ciphertext_tamper_seam_after_tunnel_recovery_yields_no_plaintext() {
    // The tamper seam below is explicitly NOT the router-link seam:
    // the recovered Garlic body is mutated between tunnel recovery
    // and destination dispatch to prove the AEAD gate.
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_6000,
    );

    clock += 10;
    let request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                a_conn,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                b"tamper-target",
                clock,
            )
            .expect("send")
    };
    let plan = side_a
        .send_via_adapter(&request, 0x1290_6100, clock)
        .expect("compose");
    let mut actions = obep_actions(&side_a, &plan);
    // Tamper inside the encrypted body (well past the I2NP header),
    // then repair the I2NP standard-header checksum so the tamper
    // seam exercises exactly the ECIES AEAD gate — not the I2NP
    // integrity check.
    let tampered = &mut actions[0];
    let mid = 16 + (tampered.message.len() - 16) / 2;
    tampered.message[mid] ^= 0x40;
    let checksum = i2pr_crypto::sha256(&tampered.message[16..]);
    tampered.message[15] = checksum.as_bytes()[0];
    let recovered = feed_action(&mut side_b, tampered);
    let outcome = side_b.dispatch(&recovered_envelope(recovered));
    match outcome {
        InboundDispatchOutcome::Rejected(InboundDispatchError::Session(_)) => {}
        other => panic!("expected AEAD rejection at the ECIES layer, got {other:?}"),
    }
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none(),
        "no inner Data / gzip / Streaming processing may occur"
    );
    assert!(side_b.streaming.drain_delivered().is_empty());
    let _ = b_conn;
}

// ---- §3 non-streaming protocol dispatch ----

#[test]
fn plan_129_non_protocol_six_client_payload_never_reaches_streaming() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_7000,
    );

    // Craft a valid client payload carrying a future datagram/I2CP
    // protocol number and send it through the full destination path.
    let datagram_envelope = encode_client_payload(&i2pr_proto::streaming::ClientPayload {
        protocol: 17,
        source_port: PORT_A,
        destination_port: PORT_B,
        payload: b"future-datagram".to_vec(),
    })
    .expect("encode non-streaming payload");
    let request = TransportSendRequest {
        destination_hash: side_b.hash_bytes(),
        source_port: PORT_A,
        destination_port: PORT_B,
        application_payload: datagram_envelope,
        sequence: 0,
        send_stream_id: 0,
        receive_stream_id: 0,
    };
    let connections_before = side_b.streaming.connection_count();
    let (_, outcome) = pipe(&mut side_a, &mut side_b, &request, 0x1290_7100, clock);
    match outcome {
        InboundStreamingOutcome::UnsupportedProtocol { protocol } => assert_eq!(protocol, 17),
        other => panic!("expected UnsupportedProtocol, got {other:?}"),
    }
    assert_eq!(
        side_b.streaming.connection_count(),
        connections_before,
        "non-streaming payloads must not touch the streaming manager"
    );
    assert!(side_b.streaming.drain_delivered().is_empty());
    let _ = a_conn;
}

// ---- §2 adapter sizing ceiling ----

#[test]
fn plan_129_outbound_adapter_bounds_the_encoded_client_payload_not_the_mtu() {
    let mut side_a = Side::new(A_SEED);
    let side_b = Side::new(B_SEED);
    preresolve_remote(&mut side_a, &side_b);

    let base = TransportSendRequest {
        destination_hash: side_b.hash_bytes(),
        source_port: PORT_A,
        destination_port: PORT_B,
        application_payload: Vec::new(),
        sequence: 0,
        send_stream_id: 0,
        receive_stream_id: 0,
    };

    // Empty payload rejected.
    let error = side_a
        .send_via_adapter(&base, 0x1290_8000, START_MS)
        .expect_err("empty payload must fail");
    assert!(matches!(error, StreamingAdapterError::EmptyPayload));

    // One byte above the encoded client-payload ceiling is rejected.
    let oversized = TransportSendRequest {
        application_payload: vec![0_u8; MAX_STREAMING_ADAPTER_PAYLOAD_BYTES + 1],
        ..base.clone()
    };
    let error = side_a
        .send_via_adapter(&oversized, 0x1290_8100, START_MS)
        .expect_err("over-ceiling payload must fail");
    assert!(
        matches!(
            error,
            StreamingAdapterError::PayloadTooLarge { actual, maximum }
                if actual == MAX_STREAMING_ADAPTER_PAYLOAD_BYTES + 1
                    && maximum == MAX_STREAMING_ADAPTER_PAYLOAD_BYTES
        ),
        "expected PayloadTooLarge, got {error:?}"
    );
}

// ---- §10 graceful CLOSE over the integrated path ----

#[test]
fn plan_129_graceful_close_completes_only_through_peer_response_over_full_path() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_9000,
    );

    // A queues a signed CLOSE; neither side is closed yet.
    clock += 10;
    let close_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_close(a_conn, &side_a.identity, &remote_b, PORT_A, PORT_B, clock)
            .expect("a close")
    };
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .state(),
        ConnectionState::ClosingLocal,
        "A must stay in ClosingLocal until the peer response arrives"
    );
    assert_eq!(
        side_b
            .streaming
            .get_connection(b_conn)
            .expect("b conn")
            .state(),
        ConnectionState::Established,
        "B must not close merely because A locally queued CLOSE"
    );

    // The CLOSE crosses the full ES destination path; B verifies the
    // signature using the established peer identity and enters its
    // close-drain state.
    let (_, _) = pipe(&mut side_a, &mut side_b, &close_request, 0x1290_9100, clock);
    assert_eq!(
        side_b
            .streaming
            .get_connection(b_conn)
            .expect("b conn")
            .state(),
        ConnectionState::ClosingRemote
    );

    // B sends its required CLOSE response through the reverse full
    // path.
    clock += 10;
    let close_response = {
        let remote_a = remote_for(&side_a.identity);
        side_b
            .streaming
            .send_close(b_conn, &side_b.identity, &remote_a, PORT_B, PORT_A, clock)
            .expect("b close response")
    };
    assert_eq!(
        side_b
            .streaming
            .get_connection(b_conn)
            .expect("b conn")
            .state(),
        ConnectionState::Closed,
        "B's half completes once its own CLOSE is emitted"
    );
    let (_, _) = pipe(
        &mut side_b,
        &mut side_a,
        &close_response,
        0x1290_9200,
        clock,
    );
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .state(),
        ConnectionState::Closed,
        "A completes graceful close only after the peer response arrives"
    );

    // Resources release boundedly: removing the terminal connections
    // drops every tracked record.
    assert!(side_a.streaming.remove_connection(a_conn).is_some());
    assert!(side_b.streaming.remove_connection(b_conn).is_some());
    assert_eq!(side_a.streaming.tracked_retransmit_count(), 0);
    assert_eq!(side_b.streaming.tracked_retransmit_count(), 0);
}

// ---- §11 RESET over the integrated path ----

#[test]
fn plan_129_reset_terminates_immediately_and_unrelated_streams_survive() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;

    // Stream 1 and an unrelated stream 2 between the same
    // destinations.
    let (conn1_a, conn1_b) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_a000,
    );
    let (conn2_a, conn2_b) = establish_stream(
        &mut side_a,
        &mut side_b,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        &mut clock,
        0x1290_a100,
    );

    // Queue application data toward stream 1 but HOLD it at the seam.
    let held_payload = pattern_bytes(41, 400);
    clock += 10;
    let held_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                conn1_a,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                &held_payload,
                clock,
            )
            .expect("held send")
    };
    let held_plan = side_a
        .send_via_adapter(&held_request, 0x1290_a200, clock)
        .expect("held compose");
    let held_actions = obep_actions(&side_a, &held_plan);

    // Signed RESET through the full destination path.
    clock += 10;
    let reset_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_reset(conn1_a, &side_a.identity, &remote_b, PORT_A, PORT_B, clock)
            .expect("reset")
    };
    let (reset_plan, reset_outcome) =
        pipe(&mut side_a, &mut side_b, &reset_request, 0x1290_a300, clock);
    assert_eq!(
        reset_plan.encrypted_message.form_name(),
        "existing-session",
        "RESET rides the established session: ECIES and gzip succeed"
    );
    assert!(matches!(
        reset_outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));
    assert_eq!(
        side_b
            .streaming
            .get_connection(conn1_b)
            .expect("conn1 b")
            .state(),
        ConnectionState::Reset,
        "receiver terminates the stream immediately after verifying the signature"
    );

    // Queued application data is not delivered afterward.
    let recovered_held = feed_action(&mut side_b, &held_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered_held)));
    let held_outcome = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("late data decrypts but must not surface");
    assert!(matches!(
        held_outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));
    assert!(
        side_b.streaming.drain_delivered().is_empty(),
        "queued application data must not be delivered after RESET"
    );

    // Unrelated streams remain unaffected.
    clock += 10;
    let survivor = pattern_bytes(42, 300);
    let survivor_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                conn2_a,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                &survivor,
                clock,
            )
            .expect("survivor send")
    };
    let (_, _) = pipe(
        &mut side_a,
        &mut side_b,
        &survivor_request,
        0x1290_a400,
        clock,
    );
    let delivered = side_b.streaming.drain_delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].connection_id, conn2_b);
    assert_eq!(delivered[0].bytes, survivor);
}

// ---- §12 0-RTT scope ----

#[test]
fn plan_129_outbound_syn_sent_is_not_established_and_admits_no_data() {
    let mut side_a = Side::new(A_SEED);
    let side_b = Side::new(B_SEED);
    assert!(side_a.streaming.listen(PORT_A).is_ok());
    preresolve_remote(&mut side_a, &side_b);

    let connect = {
        let remote_b = remote_for(&side_b.identity);
        let mut rng = ChaCha8Rng::seed_from_u64(0x1290_b000);
        side_a
            .streaming
            .connect(
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                START_MS,
                &mut rng,
            )
            .expect("connect")
    };
    let ConnectOutcome::SynSent { connection_id, .. } = connect else {
        panic!("expected SynSent");
    };
    // OutboundSynSent may support bounded pre-response data in the
    // future, but it is not equivalent to Established and admits no
    // data sends today.
    assert_eq!(
        side_a
            .streaming
            .get_connection(connection_id)
            .expect("conn")
            .state(),
        ConnectionState::OutboundSynSent
    );
    let remote_b = remote_for(&side_b.identity);
    let error = side_a
        .streaming
        .send_data(
            connection_id,
            &side_a.identity,
            &remote_b,
            PORT_A,
            PORT_B,
            b"premature",
            START_MS,
        )
        .expect_err("data send must fail while OutboundSynSent");
    assert!(
        matches!(
            error,
            i2pr_client::streaming::manager::StreamingManagerError::InvalidConnectionState
        ),
        "expected InvalidConnectionState, got {error:?}"
    );
}
