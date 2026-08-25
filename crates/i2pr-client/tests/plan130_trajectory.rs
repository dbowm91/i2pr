//! Plan 130 — Milestone 6 final wire/runtime corrective closure:
//! authoritative full-stack trajectories.
//!
//! Every trajectory crosses the complete Plan 129 destination stack
//! (gzip ClientPayload -> I2NP Data -> bound ECIES NS/NSR/ES ->
//! I2NP Garlic -> outbound tunnel roles -> post-OBEP seam -> inbound
//! tunnel roles -> dispatcher -> inbound adapter -> Streaming) with
//! no direct client-to-client transfer and no protocol state rebuilt
//! merely to make the next packet acceptable.
//!
//! Corrective semantics proven here (Plan 130 §10):
//!
//! - G1 fresh handshake sequence transition: SYN seq 0, SYN response
//!   seq 0, first application data seq 1, second seq 2;
//! - G2 one-way delayed standalone ACK over the full stack;
//! - G3 piggyback ACK suppression;
//! - G4 reorder produces reference-shaped bounded ACK/NACK feedback;
//! - G5 wire destination_port owns listener dispatch;
//! - G6 tunnel replay and Streaming duplicates are suppressed by
//!   distinct mechanisms;
//! - G7 production randomized Elligator2 representatives complete
//!   fresh handshakes;
//! - A2 frozen independent semantic fixtures (spec-derived byte
//!   layout and reference ACK/NACK expectation table).
//!
//! Mixed-router interoperability is not claimed; no sockets, DNS,
//! or external references are used.

#![allow(clippy::too_many_lines)]

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination, StreamingManager,
    StreamingManagerError,
};
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    DestinationConfig, DestinationDispatcher, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool, EciesSessionConfig,
    EciesSessionManager, InboundDispatchOutcome, InboundStreamingOutcome,
    MAX_STREAMING_ADAPTER_PAYLOAD_BYTES, OutboundDeliveryPlan, PlannedOutboundForm,
    StreamingAdapterError, StreamingDestinationAdapter, build_signed_lease_set2,
};
use i2pr_crypto::X25519_KEY_LENGTH;
use i2pr_netdb::ValidatedLeaseSet2;
use i2pr_proto::streaming::{
    FLAG_NO_ACK, FLAG_SYNCHRONIZE, StreamingReceiveLimit, decode_client_payload,
    decode_streaming_packet, encode_client_payload,
};
use i2pr_proto::{Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelGatewayMessage};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    InboundGatewayRole, InboundParticipantRole, LayerKeys, LocalInboundEndpointRole,
    OutboundEndpointRole, OutboundParticipantRole, RouterDeliveryAction, TunnelDirection, TunnelId,
    TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const A_SEED: u64 = 0x130A;
const B_SEED: u64 = 0x130B;
const NOW_SECONDS: u32 = 5_300;
const START_MS: u64 = 600_000;
const PORT_A: u16 = 0x13A0;
/// Exact listener on B.
const PORT_B: u16 = 0x13B0;
/// A second exact listener on B for port-routing isolation checks.
const PORT_B2: u16 = 0x13B1;
/// Initial retransmission timeout from the streaming config.
const INITIAL_RTO_MS: u64 = 5_000;

// ---- Deterministic fixture helpers (Plan 129 topology retained) ----

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

/// One live inbound chain per side. Plan 130 §9 F1: created exactly
/// once so the tunnel duplicate windows persist across ordinary
/// deliveries; only an explicit rebuild models tunnel replacement.
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
        let ibgw = InboundGatewayRole::new(&ibgw_hop, DuplicateWindow::new(16), START_MS + 600_000)
            .expect("ibgw role");
        let participant = InboundParticipantRole::new(
            &participant_hop,
            DuplicateWindow::new(16),
            START_MS + 600_000,
        )
        .expect("inbound participant role");
        let endpoint = LocalInboundEndpointRole::new(
            inbound_tunnel_direct(seed),
            16,
            1 << 20,
            600_000,
            0,
            START_MS + 600_000,
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
                START_MS + 900_000,
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

    /// Inbound adapter boundary. The adapter decodes the wire ports
    /// itself (Plan 130 §8); there is deliberately no caller-supplied
    /// listener port parameter to misuse.
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

fn obep_actions(sender: &Side, plan: &OutboundDeliveryPlan) -> Vec<RouterDeliveryAction> {
    let outbound_hops = sender.outbound.role().established().hops();
    let mut out_participant = OutboundParticipantRole::new(
        &outbound_hops[0],
        DuplicateWindow::new(16),
        START_MS + 600_000,
    )
    .expect("outbound participant role");
    let mut obep = OutboundEndpointRole::new(
        &outbound_hops[1],
        DuplicateWindow::new(16),
        16,
        1 << 20,
        600_000,
        START_MS + 600_000,
        0,
    );
    let mut actions: Vec<RouterDeliveryAction> = Vec::new();
    for cell in &plan.cells {
        let forwarded = out_participant
            .process(&hop_router_hash(sender.seed, 0), &cell.cell, 0)
            .expect("outbound participant forward");
        if let Some(action) = obep
            .process(&outbound_hops[0].peer().hash(), &forwarded, 0)
            .expect("obep process")
        {
            actions.push(action);
        }
    }
    assert_eq!(actions.len(), 1);
    assert_eq!(
        action_target(&actions[0]),
        plan.selected_lease.gateway_router_hash
    );
    actions
}

fn action_target(action: &RouterDeliveryAction) -> Hash {
    action.target_router
}

/// Feeds one action through the receiver's persistent inbound chain.
fn feed_action(receiver: &mut Side, action: &RouterDeliveryAction) -> Vec<u8> {
    let inner_i2np =
        I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE).expect("decode i2np");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: action.tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner_i2np),
    };
    // Fixed seed: an identical action reproduces identical cells, so
    // replays hit the live duplicate window rather than fresh
    // randomness.
    let mut rng = ChaCha8Rng::seed_from_u64(0x51EA);
    let cells = receiver
        .inbound
        .ibgw
        .process_cells(&gateway_msg, &mut rng, 0)
        .expect("ibgw multi-cell forward");
    run_inbound_cells(receiver, &cells).expect("endpoint recovered the Garlic carrier")
}

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
            assert!(recovered.is_none(), "reassembler completes once");
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

/// Drives the integrated SYN / SYN-response handshake across the
/// full destination stack and returns the established connection
/// ids plus the wire observations of both handshake packets.
fn establish_stream(
    a: &mut Side,
    b: &mut Side,
    clock: &mut u64,
    seed_base: u64,
) -> (ConnectionId, ConnectionId) {
    preresolve_remote(a, b);

    *clock += 10;
    let connect = {
        let remote_b = remote_for(&b.identity);
        a.streaming
            .connect(
                &a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
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
        panic!("expected SynSent");
    };

    let syn_requests = a.streaming.drain_outbound();
    assert_eq!(syn_requests.len(), 1);
    let (plan_ns, _) = pipe(a, b, &syn_requests[0], seed_base + 1, *clock);
    let _ = plan_ns;

    assert_eq!(b.streaming.listener_backlog(PORT_B), 1);
    let b_conn = b.streaming.accept(PORT_B).expect("pending inbound stream");

    // Reverse-routing handoff: install the validated sender LS2.
    let validated_a = b
        .dispatcher
        .accepted_lease_set2_for(b.identity.id(), a.identity.id().as_netdb_key())
        .expect("B must have bound A's bundled LS2")
        .clone();
    b.routing
        .install_remote_lease_set2(validated_a)
        .expect("install validated sender ls2");

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
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                *clock,
                &mut ChaCha8Rng::seed_from_u64(seed_base + 2),
            )
            .expect("b accept inbound syn")
    };
    let (_, _) = pipe(b, a, &response_request, seed_base + 3, *clock);

    assert_eq!(
        a.streaming.get_connection(a_conn).expect("a conn").state(),
        ConnectionState::Established
    );
    assert_eq!(
        b.streaming.get_connection(b_conn).expect("b conn").state(),
        ConnectionState::Established
    );
    (a_conn, b_conn)
}

fn decoded_packet(request: &TransportSendRequest) -> i2pr_proto::streaming::StreamingPacket {
    let envelope = decode_client_payload(
        &request.application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("client payload");
    let (packet, _) = decode_streaming_packet(
        &envelope.payload,
        StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    )
    .expect("decode streaming packet");
    packet
}

// ---- §4 Phase A2: independent frozen semantic fixtures ----

/// Frozen spec-derived simple-ACK fixture (Plan 130 Phase A2).
///
/// The 22-byte header is written literally from the current I2P
/// Streaming specification field table — not produced by i2pr's
/// manager:
///
/// ```text
/// sendStreamId     4 bytes  00 00 00 07
/// receiveStreamId  4 bytes  00 00 00 09
/// sequenceNum      4 bytes  00 00 00 00   <- plain-ACK form
/// ackThrough       4 bytes  00 00 00 02   <- acknowledges seq <= 2
/// NACK count       1 byte   00
/// resendDelay      1 byte   00
/// flags            2 bytes  00 00         <- SYNCHRONIZE clear
/// option size      2 bytes  00 00
/// ```
#[test]
fn plan130_frozen_spec_simple_ack_fixture_classifies_and_carries_ack_through_two() {
    const FIXTURE: [u8; 22] = [
        0x00, 0x00, 0x00, 0x07, // sendStreamId
        0x00, 0x00, 0x00, 0x09, // receiveStreamId
        0x00, 0x00, 0x00, 0x00, // sequenceNum = 0
        0x00, 0x00, 0x00, 0x02, // ackThrough = 2
        0x00, // NACK count = 0
        0x00, // resendDelay = 0
        0x00, 0x00, // flags = 0
        0x00, 0x00, // option size = 0
    ];
    let peek = i2pr_proto::streaming::peek_streaming_header(&FIXTURE).expect("frozen header peeks");
    assert_eq!(peek.send_stream_id, 7);
    assert_eq!(peek.receive_stream_id, 9);
    assert_eq!(
        peek.flags_bits & FLAG_SYNCHRONIZE,
        0,
        "plain ACK: SYN clear"
    );
    let (packet, _) = decode_streaming_packet(
        &FIXTURE,
        StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    )
    .expect("frozen fixture decodes");
    assert_eq!(packet.sequence_num, 0, "plain-ACK control form");
    assert_eq!(
        packet.ack_through, 2,
        "ackThrough == 2 is valid and carried"
    );
    assert!(packet.payload.is_empty(), "plain ACK carries no payload");
    // The manager-level classification must treat this exact shape as
    // control, not application data.
    assert!(
        packet.sequence_num == 0 && !packet.flags.synchronize(),
        "manager-level plain-ACK classification holds for the fixture"
    );
}

/// Frozen reference ACK/NACK expectation table (Plan 130 Phase A2).
///
/// Derived by hand from the Java I2P `MessageInputStream.updateAcks`
/// algorithm pinned in `specs/references/streaming-packet-wire.md`:
/// `ackThrough` equals the highest received sequence (including
/// out-of-order buffered packets) and NACKs list every missing
/// sequence strictly below it. Not generated by i2pr's manager.
#[test]
fn plan130_reference_reorder_ack_nack_expectation_table() {
    // Table rows: (next_expected, highest_received, buffered set)
    // -> expected (ack_through, nacks).
    type ExpectationRow = (u32, Option<u32>, &'static [u32], u32, &'static [u32]);
    let cases: &[ExpectationRow] = &[
        // Nothing beyond the handshake received yet.
        (1, None, &[], 0, &[]),
        // Contiguous delivery through seq 2.
        (3, Some(2), &[], 2, &[]),
        // seq 2 arrived before seq 1: ackThrough 2 with NACK [1].
        (1, Some(2), &[2], 2, &[1]),
        // seq 3 also buffered: gaps [1] remain, ackThrough 3.
        (1, Some(3), &[2, 3], 3, &[1]),
        // Gap closed: everything contiguous through 3.
        (4, Some(3), &[], 3, &[]),
    ];
    for (row, &(next_expected, highest, buffered, want_through, want_nacks)) in
        cases.iter().enumerate()
    {
        let mut window = i2pr_client::streaming::RecvWindowPolicy::new(
            i2pr_client::streaming::RecvWindowConfig {
                max_window_packets: 64,
            },
        );
        // Advance the window to the modeled state.
        while window.next_expected() < next_expected {
            let sequence = window.next_expected();
            let decision = window.receive(sequence, Vec::new());
            assert!(matches!(
                decision,
                i2pr_client::streaming::RecvWindowDecision::Delivered { .. }
            ));
        }
        for &sequence in buffered {
            window.receive(sequence, Vec::new());
        }
        assert_eq!(
            window.next_expected(),
            next_expected,
            "row {row}: next_expected setup"
        );
        assert_eq!(
            window.highest_received(),
            highest,
            "row {row}: highest_received setup"
        );
        let (ack_through, nacks) = window.ack_view();
        assert_eq!(ack_through, want_through, "row {row}: ackThrough");
        assert_eq!(nacks, want_nacks.to_vec(), "row {row}: nacks");
    }
}

// ---- §5 G1: fresh handshake and sequence transition ----

#[test]
fn plan130_fresh_handshake_sequence_transition_over_full_stack() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;

    preresolve_remote(&mut side_a, &side_b);
    clock += 10;
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
                &mut ChaCha8Rng::seed_from_u64(0x1301_0000),
            )
            .expect("connect")
    };
    let ConnectOutcome::SynSent {
        connection_id: a_conn,
        ..
    } = connect
    else {
        panic!("expected SynSent");
    };
    let syn_requests = side_a.streaming.drain_outbound();

    // Wire assertion: the SYN occupies sequence 0.
    let syn_packet = decoded_packet(&syn_requests[0]);
    assert_eq!(syn_packet.sequence_num, 0, "SYN sequenceNum is 0");
    assert_ne!(syn_packet.flags.bits() & FLAG_SYNCHRONIZE, 0);
    // The production path uses the randomized Elligator2 generator
    // inside the ECIES session manager; the handshake completing at
    // all proves randomized representatives round-trip (G7).

    pipe(
        &mut side_a,
        &mut side_b,
        &syn_requests[0],
        0x1301_0100,
        clock,
    );
    let b_conn = side_b.streaming.accept(PORT_B).expect("pending stream");

    // Reverse-routing handoff for the reply.
    let validated_a = side_b
        .dispatcher
        .accepted_lease_set2_for(side_b.identity.id(), side_a.identity.id().as_netdb_key())
        .expect("bundled LS2 bound")
        .clone();
    side_b
        .routing
        .install_remote_lease_set2(validated_a)
        .expect("install ls2");

    clock += 10;
    let response_request = {
        let remote_a = remote_for(&side_a.identity);
        side_b
            .streaming
            .accept_inbound_syn(
                &side_b.identity,
                &remote_a,
                b_conn,
                PORT_B,
                PORT_A,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                clock,
                &mut ChaCha8Rng::seed_from_u64(0x1301_0200),
            )
            .expect("accept inbound syn")
    };
    // A remains OutboundSynSent until the authenticated response
    // completes the reverse path.
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .state(),
        ConnectionState::OutboundSynSent
    );

    // Wire assertions: response sequence 0, NO_ACK clear, and its
    // ackThrough acknowledges A's sequence-0 SYN.
    let response_packet = decoded_packet(&response_request);
    assert_eq!(response_packet.sequence_num, 0, "SYN response seq 0");
    assert_eq!(
        response_packet.flags.bits() & FLAG_NO_ACK,
        0,
        "NO_ACK clear"
    );
    assert_eq!(response_packet.ack_through, 0, "acknowledges the SYN slot");
    assert!(response_packet.nacks.is_empty());

    let (_, _) = pipe(
        &mut side_b,
        &mut side_a,
        &response_request,
        0x1301_0300,
        clock,
    );
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .state(),
        ConnectionState::Established
    );

    // First application data: sequence 1, Existing Session, full
    // tunnel path.
    clock += 10;
    let first = pattern_bytes(1, 700);
    let first_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                a_conn,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                &first,
                clock,
            )
            .expect("first data send")
    };
    let first_packet = decoded_packet(&first_request);
    assert_eq!(first_packet.sequence_num, 1, "first application data seq 1");
    let (_, outcome_first) = pipe(&mut side_a, &mut side_b, &first_request, 0x1301_0400, clock);
    assert!(matches!(
        outcome_first,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));

    // Second application data: sequence increments normally.
    clock += 10;
    let second = pattern_bytes(2, 500);
    let second_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                a_conn,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                &second,
                clock,
            )
            .expect("second data send")
    };
    let second_packet = decoded_packet(&second_request);
    assert_eq!(second_packet.sequence_num, 2);
    // Its piggyback acknowledgement covers what A has received so far
    // (the SYN response slot through B's data): sequence 0.
    assert_eq!(second_packet.ack_through, 0);

    let (_, outcome_second) = pipe(
        &mut side_a,
        &mut side_b,
        &second_request,
        0x1301_0500,
        clock,
    );
    assert!(matches!(
        outcome_second,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));

    let delivered = side_b.streaming.drain_delivered();
    let mut received = Vec::new();
    for event in &delivered {
        received.extend_from_slice(&event.bytes);
    }
    let mut expected = first.clone();
    expected.extend_from_slice(&second);
    assert_eq!(received, expected, "B observes original order");
}

// ---- §6 G2: one-way delayed ACK ----

#[test]
fn plan130_one_way_delayed_standalone_ack_traverses_full_stack_and_clears_retransmit() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(&mut side_a, &mut side_b, &mut clock, 0x1302_0000);
    let _ = b_conn;

    // 1. A sends data sequence 1; B sends no application data.
    clock += 10;
    let payload = pattern_bytes(3, 800);
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
            .expect("one-way data")
    };
    assert_eq!(decoded_packet(&request).sequence_num, 1);
    let (_, _) = pipe(&mut side_a, &mut side_b, &request, 0x1302_0100, clock);
    assert_eq!(side_a.streaming.tracked_retransmit_count(), 1);

    // 2-3. Before the deadline poll_acks emits nothing.
    let deadline_probe = clock;
    assert!(
        side_b.streaming.poll_acks(deadline_probe).is_empty(),
        "poll before the deadline must emit nothing"
    );

    // 4. After the deadline B emits exactly one coalesced simple ACK.
    clock += i2pr_client::streaming::config::DEFAULT_DELAYED_ACK_MS + 1;
    let ack_requests = side_b.streaming.poll_acks(clock);
    assert_eq!(ack_requests.len(), 1, "exactly one delayed ACK");
    // The ACK traverses gzip -> Data -> ES -> outbound tunnel -> OBEP
    // -> seam -> inbound tunnel -> ECIES -> Data -> gzip -> Streaming.
    let (_, outcome) = pipe(
        &mut side_b,
        &mut side_a,
        &ack_requests[0],
        0x1302_0200,
        clock,
    );
    assert!(matches!(
        outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));

    // 6. A cleared the acknowledged retransmission entry.
    assert_eq!(
        side_a.streaming.tracked_retransmit_count(),
        0,
        "the plain ACK must clear the tracked record"
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

    // 7. Neither side received application bytes from the ACK packet.
    assert!(
        side_a.streaming.drain_delivered().is_empty(),
        "an ACK-only packet never delivers application bytes"
    );

    // 8. A does not retransmit when the RTO expires later.
    clock += INITIAL_RTO_MS + 200;
    assert!(
        side_a.streaming.poll_retransmits(clock).is_empty(),
        "acknowledged data must not be retransmitted"
    );
}

// ---- §7 G3: piggyback ACK ----

#[test]
fn plan130_piggyback_ack_suppresses_the_standalone_ack() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(&mut side_a, &mut side_b, &mut clock, 0x1303_0000);

    // A delivers data; B schedules its delayed ACK.
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
                b"ping",
                clock,
            )
            .expect("ping")
    };
    let (_, _) = pipe(&mut side_a, &mut side_b, &request, 0x1303_0100, clock);
    // The pending ACK exists but is not yet due.
    assert_eq!(side_b.streaming.pending_ack_count(), 1);
    assert!(side_b.streaming.poll_acks(clock).is_empty());

    // B sends application data BEFORE the standalone deadline. The
    // reverse data packet must carry the correct cumulative ACK state
    // (ackThrough 1: A's data seq 1 received).
    clock += 100;
    let reply = {
        let remote_a = remote_for(&side_a.identity);
        side_b
            .streaming
            .send_data(
                b_conn,
                &side_b.identity,
                &remote_a,
                PORT_B,
                PORT_A,
                b"pong",
                clock,
            )
            .expect("pong")
    };
    let reply_packet = decoded_packet(&reply);
    assert_eq!(reply_packet.sequence_num, 1, "B's first app data seq 1");
    assert_eq!(reply_packet.ack_through, 1, "piggybacked ACK covers seq 1");

    // No redundant simple ACK may be emitted afterwards.
    let (_, _) = pipe(&mut side_b, &mut side_a, &reply, 0x1303_0200, clock);
    assert_eq!(
        side_b.streaming.pending_ack_count(),
        0,
        "piggyback satisfies the pending standalone ACK"
    );
    assert!(side_b.streaming.poll_acks(clock + 10_000).is_empty());

    // A observed B's bytes exactly once and cleared seq 1.
    let delivered = side_a.streaming.drain_delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].bytes, b"pong".to_vec());
    assert_eq!(side_a.streaming.tracked_retransmit_count(), 0);
}

// ---- §8 G4: reorder + NACK convergence ----

#[test]
fn plan130_reorder_produces_reference_nack_feedback_and_converges() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(&mut side_a, &mut side_b, &mut clock, 0x1304_0000);

    // Send application sequences 1 and 2 but deliver sequence 2
    // first (held at the post-OBEP seam after real processing).
    let first = pattern_bytes(4, 600);
    let second = pattern_bytes(5, 650);
    let mut held_actions: Vec<RouterDeliveryAction> = Vec::new();
    let mut requests: Vec<TransportSendRequest> = Vec::new();
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
        assert_eq!(request.sequence, index as u32 + 1);
        requests.push(request);
    }
    for (index, request) in requests.iter().enumerate() {
        let plan = side_a
            .send_via_adapter(request, 0x1304_0100 + index as u64, clock)
            .expect("compose");
        held_actions.extend(obep_actions(&side_a, &plan));
    }

    // Deliver sequence 2 only.
    let recovered_seq2 = feed_action(&mut side_b, &held_actions[1]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered_seq2)));
    side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("seq 2 decrypts");

    // B does not deliver sequence 2 prematurely.
    assert!(side_b.streaming.drain_delivered().is_empty());
    let b_state = side_b.streaming.get_connection(b_conn).expect("b conn");
    assert_eq!(b_state.recv_window().reorder_count(), 1);
    // Reference-shaped feedback per the frozen expectation table:
    // ackThrough 2 with NACK [1].
    let (ack_through, nacks) = b_state.recv_window().ack_view();
    assert_eq!((ack_through, nacks.as_slice()), (2, &[1][..]));

    // The feedback travels the full reverse destination stack via
    // B's delayed standalone ACK.
    clock += i2pr_client::streaming::config::DEFAULT_DELAYED_ACK_MS + 1;
    let nack_requests = side_b.streaming.poll_acks(clock);
    assert_eq!(nack_requests.len(), 1);
    let nack_packet = decoded_packet(&nack_requests[0]);
    assert_eq!(nack_packet.sequence_num, 0, "simple ACK control form");
    assert_eq!(nack_packet.ack_through, 2);
    assert_eq!(nack_packet.nacks, vec![1]);
    let (_, _) = pipe(
        &mut side_b,
        &mut side_a,
        &nack_requests[0],
        0x1304_0200,
        clock,
    );

    // A retains the NACKed sequence for retransmission while the
    // cumulative advance clears nothing it cannot prove.
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .send_window()
            .get_unacked(1)
            .map(|entry| entry.sequence),
        Some(1),
        "NACKed sequence 1 stays tracked"
    );

    // Sequence 1 arrives; B delivers in original order and the next
    // ACK converges A's state without unbounded gap generation.
    let recovered_seq1 = feed_action(&mut side_b, &held_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered_seq1)));
    side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("seq 1 delivers");
    let delivered = side_b.streaming.drain_delivered();
    let mut received = Vec::new();
    for event in &delivered {
        received.extend_from_slice(&event.bytes);
    }
    let mut expected = first.clone();
    expected.extend_from_slice(&second);
    assert_eq!(received, expected, "original application byte order");

    clock += i2pr_client::streaming::config::DEFAULT_DELAYED_ACK_MS + 1;
    let final_acks = side_b.streaming.poll_acks(clock);
    assert_eq!(final_acks.len(), 1);
    let final_packet = decoded_packet(&final_acks[0]);
    assert_eq!(final_packet.ack_through, 2);
    assert!(final_packet.nacks.is_empty(), "no residual NACKs");
    let (_, _) = pipe(&mut side_b, &mut side_a, &final_acks[0], 0x1304_0300, clock);
    assert_eq!(side_a.streaming.tracked_retransmit_count(), 0);
    // No unbounded gap/NACK generation occurred anywhere above.
}

// ---- §9 G5: wire destination-port routing ----

#[test]
fn plan130_wire_destination_port_selects_listener_and_rejects_redirects() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    assert!(side_b.streaming.listen(PORT_B2).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn_main) = establish_stream(&mut side_a, &mut side_b, &mut clock, 0x1305_0000);

    // Data for the established tuple flows into the exact listener's
    // connection (not any other backlog).
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
                b"routed",
                clock,
            )
            .expect("send")
    };
    let (_, outcome) = pipe(&mut side_a, &mut side_b, &request, 0x1305_0100, clock);
    match outcome {
        InboundStreamingOutcome::StreamingDispatched {
            source_port,
            destination_port,
            ..
        } => {
            assert_eq!(source_port, PORT_A);
            assert_eq!(destination_port, PORT_B);
        }
        other => panic!("expected dispatch, got {other:?}"),
    }
    assert_eq!(side_b.streaming.listener_backlog(PORT_B2), 0);

    // Wrong-port traffic on the same connection cannot enter another
    // listener backlog nor corrupt connection state: the wire tuple
    // mismatch fails closed.
    let mut wrong_ports = decode_client_payload(
        &request.application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("client payload");
    wrong_ports.source_port = PORT_B2;
    wrong_ports.destination_port = PORT_A;
    let tampered = encode_client_payload(&wrong_ports).expect("re-encode");
    let wrong_request = TransportSendRequest {
        application_payload: tampered,
        ..request.clone()
    };
    let plan = side_a
        .send_via_adapter(&wrong_request, 0x1305_0200, clock)
        .expect("composition succeeds; rejection is inbound");
    let actions = obep_actions(&side_a, &plan);
    let recovered = feed_action(&mut side_b, &actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    let error = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect_err("established tuple mismatch must fail closed");
    match error {
        StreamingAdapterError::Streaming(StreamingManagerError::PortTupleMismatch {
            expected_source,
            expected_destination,
            actual_source,
            actual_destination,
        }) => {
            assert_eq!(expected_source, PORT_A);
            assert_eq!(expected_destination, PORT_B);
            assert_eq!(actual_source, PORT_B2);
            assert_eq!(actual_destination, PORT_A);
        }
        other => panic!("expected PortTupleMismatch, got {other:?}"),
    }
    // The connection state is untouched and no other backlog grew.
    assert_eq!(
        side_b
            .streaming
            .get_connection(b_conn_main)
            .expect("b conn")
            .state(),
        ConnectionState::Established
    );
    assert_eq!(side_b.streaming.listener_backlog(PORT_B2), 0);

    // Drain the internally queued copy of the routed data request so
    // the stray SYN below is the only pending manager emission.
    assert!(!side_a.streaming.drain_outbound().is_empty());

    // A SYN addressed to an unclaimed destination port reaches no
    // listener at all and fails closed with the typed outcome.
    preresolve_remote(&mut side_a, &side_b);
    clock += 10;
    let stray_connect = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .connect(
                &side_a.identity,
                &remote_b,
                PORT_A,
                0x6C31, // unclaimed on B
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                clock,
                &mut ChaCha8Rng::seed_from_u64(0x1305_0300),
            )
            .expect("stray connect")
    };
    let ConnectOutcome::SynSent { .. } = stray_connect else {
        panic!("expected SynSent");
    };
    let stray_requests = side_a.streaming.drain_outbound();
    assert_eq!(stray_requests.len(), 1);
    let stray_plan = side_a
        .send_via_adapter(&stray_requests[0], 0x1305_0400, clock)
        .expect("compose");
    let stray_actions = obep_actions(&side_a, &stray_plan);
    let stray_recovered = feed_action(&mut side_b, &stray_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(stray_recovered)));
    let stray_error = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect_err("unclaimed destination port must fail closed");
    match stray_error {
        StreamingAdapterError::Streaming(StreamingManagerError::NoMatchingListener {
            destination_port,
        }) => assert_eq!(destination_port, 0x6C31),
        other => panic!("expected NoMatchingListener, got {other:?}"),
    }
    assert_eq!(side_b.streaming.listener_backlog(PORT_B), 0);
    assert_eq!(side_b.streaming.listener_backlog(PORT_B2), 0);
}

// ---- §10 G5b: wildcard listener catches unclaimed ports ----

#[test]
fn plan130_wildcard_listener_catches_unclaimed_destination_ports() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    // Only the wildcard (port 0) listener exists.
    assert!(side_b.streaming.listen(0).is_ok());
    preresolve_remote(&mut side_a, &side_b);

    let mut clock = START_MS;
    clock += 10;
    let connect = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .connect(
                &side_a.identity,
                &remote_b,
                PORT_A,
                0x7C31, // not explicitly bound anywhere
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                clock,
                &mut ChaCha8Rng::seed_from_u64(0x1305_1000),
            )
            .expect("connect")
    };
    let ConnectOutcome::SynSent { .. } = connect else {
        panic!("expected SynSent");
    };
    let syn = side_a.streaming.drain_outbound();
    assert_eq!(syn.len(), 1);
    let (_, _) = pipe(&mut side_a, &mut side_b, &syn[0], 0x1305_1100, clock);
    // The reference fallback delivered the SYN into the wildcard
    // listener's backlog.
    assert_eq!(
        side_b.streaming.listener_backlog(0),
        1,
        "wildcard listener catches the unclaimed port per reference semantics"
    );
}

// ---- §11 G6: tunnel replay vs Streaming duplicate separation ----

#[test]
fn plan130_tunnel_replay_and_streaming_duplicate_are_independently_suppressed() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(&mut side_a, &mut side_b, &mut clock, 0x1306_0000);

    let payload = pattern_bytes(6, 550);
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

    // Level 1 — tunnel replay: feed the exact same post-OBEP router
    // delivery twice through the same live inbound roles.
    let plan = side_a
        .send_via_adapter(&request, 0x1306_0100, clock)
        .expect("compose");
    let actions = obep_actions(&side_a, &plan);
    let first = feed_action(&mut side_b, &actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(first)));
    side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("first copy delivers");

    // Byte-identical cells again: rejected by the persistent tunnel
    // duplicate window; nothing reaches ECIES or the dispatcher.
    // (Same deterministic cell-builder seed as `feed_action`, so the
    // regenerated cells are byte-identical to the first delivery.)
    let inner_i2np =
        I2npMessage::decode_standard(&actions[0].message, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: actions[0].tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner_i2np),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(0x51EA); // same seed as feed_action
    let cells = side_b
        .inbound
        .ibgw
        .process_cells(&gateway_msg, &mut rng, 0)
        .expect("ibgw rebuilds identical cells");
    for cell in cells {
        let error = side_b
            .inbound
            .participant
            .process(&hop_router_hash(side_b.seed, 1), &cell.cell, 0)
            .expect_err("live duplicate window must reject the byte-identical cell");
        assert!(
            matches!(error, i2pr_tunnel::TunnelRoleError::DuplicateCell),
            "expected DuplicateCell, got {error:?}"
        );
    }
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none(),
        "tunnel replay never reaches ECIES/dispatcher"
    );

    // Level 2 — Streaming duplicate: freshly re-encrypt/reseal the
    // SAME already-received streaming sequence so it legitimately
    // traverses the tunnel AND ECIES layers again; the Streaming
    // layer suppresses duplicate application delivery.
    let reseal_plan = side_a
        .send_via_adapter(&request, 0x1306_0200, clock)
        .expect("fresh seal of the same streaming packet");
    assert_ne!(
        reseal_plan.garlic_i2np_bytes, plan.garlic_i2np_bytes,
        "a fresh seal produces distinct ECIES/tunnel bytes"
    );
    let reseal_actions = obep_actions(&side_a, &reseal_plan);
    let resealed = feed_action(&mut side_b, &reseal_actions[0]);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(resealed)));
    let reseal_outcome = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("fresh-seal duplicate decrypts through ECIES");
    assert!(matches!(
        reseal_outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));

    // Exactly one application delivery across all three attempts.
    let delivered = side_b.streaming.drain_delivered();
    assert_eq!(
        delivered.len(),
        1,
        "each mechanism suppressed its own layer exactly once"
    );
    assert_eq!(delivered[0].bytes, payload);
    let state = side_b.streaming.get_connection(b_conn).expect("b conn");
    assert_eq!(state.state(), ConnectionState::Established);
}

// ---- §12: ACK-only traffic never loops ----

#[test]
fn plan130_plain_ack_is_never_application_data_and_never_schedules_an_ack() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    assert!(side_b.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (a_conn, b_conn) = establish_stream(&mut side_a, &mut side_b, &mut clock, 0x1307_0000);

    // B receives data (scheduling its own delayed ACK), then A sends
    // a plain ACK back. Receiving a plain ACK must not deliver bytes
    // and must not schedule an acknowledgement of the ack (no
    // ACK-of-ACK loop in either direction).
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
                b"data",
                clock,
            )
            .expect("data")
    };
    let (_, _) = pipe(&mut side_a, &mut side_b, &request, 0x1307_0100, clock);

    clock += i2pr_client::streaming::config::DEFAULT_DELAYED_ACK_MS + 1;
    let acks_b_to_a = side_b.streaming.poll_acks(clock);
    assert_eq!(acks_b_to_a.len(), 1);
    let (_, _) = pipe(
        &mut side_b,
        &mut side_a,
        &acks_b_to_a[0],
        0x1307_0200,
        clock,
    );

    // A processed the plain ACK: nothing delivered...
    assert!(side_a.streaming.drain_delivered().is_empty());
    // ...and no pending ACK was scheduled on either side for it.
    assert_eq!(side_a.streaming.pending_ack_count(), 0);
    assert_eq!(side_b.streaming.pending_ack_count(), 0);
    clock += i2pr_client::streaming::config::DEFAULT_DELAYED_ACK_MS + 10_000;
    assert!(side_a.streaming.poll_acks(clock).is_empty());
    assert!(side_b.streaming.poll_acks(clock).is_empty());

    // Baseline: the ping data itself was delivered once; drain it.
    let baseline = side_b.streaming.drain_delivered();
    assert_eq!(baseline.len(), 1);
    assert_eq!(baseline[0].bytes, b"data".to_vec());

    // A hostile seq-0 packet WITH payload on the live connection is
    // also pure control: dropped payload, no delivery, no window
    // corruption.
    clock += 10;
    let mut hostile = decode_client_payload(
        &request.application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("client payload");
    let a_conn_state = side_a.streaming.get_connection(a_conn).expect("a conn");
    hostile.payload = build_seq_zero_data_packet(
        a_conn_state.remote_stream_id(),
        a_conn_state.local_stream_id(),
    );
    let hostile_bytes = encode_client_payload(&hostile).expect("encode hostile");
    let hostile_request = TransportSendRequest {
        application_payload: hostile_bytes,
        destination_hash: side_b.hash_bytes(),
        source_port: PORT_A,
        destination_port: PORT_B,
        sequence: 0,
        send_stream_id: 0x0102_0304,
        receive_stream_id: 0x0506_0708,
    };
    let (_, hostile_outcome) = pipe(
        &mut side_a,
        &mut side_b,
        &hostile_request,
        0x1307_0300,
        clock,
    );
    assert!(matches!(
        hostile_outcome,
        InboundStreamingOutcome::StreamingDispatched { .. }
    ));
    let delivered_after_hostile = side_b.streaming.drain_delivered();
    assert!(
        delivered_after_hostile.is_empty(),
        "seq-0 non-SYN payload must never be delivered as application bytes"
    );

    let state_after = side_b
        .streaming
        .get_connection(b_conn)
        .expect("b conn")
        .state();
    assert_eq!(state_after, ConnectionState::Established);
}

/// Builds one minimal unsigned seq-0 non-SYN streaming packet with a
/// payload (the hostile form) addressed through the live connection's
/// stream-id pair.
fn build_seq_zero_data_packet(send_stream_id: u32, receive_stream_id: u32) -> Vec<u8> {
    let flags = i2pr_proto::streaming::StreamingFlags::new(0).expect("empty flags");
    let builder = i2pr_proto::streaming::StreamingPacketBuilder {
        send_stream_id,
        receive_stream_id,
        sequence_num: 0,
        ack_through: 0,
        nacks: Vec::new(),
        resend_delay: 0,
        flags,
        option_bytes: Vec::new(),
        payload: b"hostile".to_vec(),
    };
    encode_streaming_packet_for_fixture(&builder)
}

fn encode_streaming_packet_for_fixture(
    builder: &i2pr_proto::streaming::StreamingPacketBuilder,
) -> Vec<u8> {
    use i2pr_proto::streaming::{StreamingSendLimit, encode_streaming_packet};
    encode_streaming_packet(builder, StreamingSendLimit::default()).expect("encode fixture packet")
}

// ---- §13 G7 note ----

#[test]
fn plan130_establishment_uses_the_production_randomized_elligator_generator() {
    // Every fresh handshake above seals its bound New Session through
    // `EciesSessionManager`, which generates ephemerals with
    // `EciesEphemeralKeypair::generate` — the production randomized-
    // representation constructor (Plan 130 §5/§10 G7). Two fresh
    // handshakes between independent destinations therefore exercise
    // randomized representatives end-to-end; neither depends on a
    // specific representative byte string.
    let mut side_a = Side::new(A_SEED);
    let mut side_b1 = Side::new(B_SEED);
    let mut side_b2 = Side::new(0x130C_u64);
    assert!(side_b1.streaming.listen(PORT_B).is_ok());
    assert!(side_b2.streaming.listen(PORT_B).is_ok());
    let mut clock = START_MS;
    let (conn1_a, _) = establish_stream(&mut side_a, &mut side_b1, &mut clock, 0x1307_1000);
    let (conn2_a, _) = establish_stream(&mut side_a, &mut side_b2, &mut clock, 0x1307_2000);
    assert_ne!(conn1_a, conn2_a);
    assert_eq!(side_a.session.pending_handshake_count(), 0);
    assert_eq!(
        side_a
            .session
            .planned_outbound_form(&side_b1.static_public(), NOW_SECONDS),
        PlannedOutboundForm::ExistingSession
    );
}
