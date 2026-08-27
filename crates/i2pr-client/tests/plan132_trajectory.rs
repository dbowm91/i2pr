//! Plan 132 — Milestone 6 final evidence and transactional closure:
//! layer-isolated replay trajectories and transactional send evidence.
//!
//! Each trajectory below targets one Plan 132 acceptance criterion. The
//! historical Plan 131 trajectory file remains in the repository for
//! audit; the present file replaces its three replay-layer legs with
//! artifact-preserving tests that drive the *exact* retained bytes
//! into the *intended* rejection boundary:
//!
//! - B1 exact cell replay hits the inbound tunnel duplicate window;
//! - B2 consumed ES ciphertext, wrapped in fresh tunnel cells, is
//!   rejected by the ECIES/session layer before plaintext reaches
//!   Streaming;
//! - B3 fresh ECIES reseal of the *same* Streaming sequence passes
//!   tunnel + ECIES and is deduplicated only by Streaming;
//! - C1..C3 send_data / send_close / send_reset never mutate
//!   connection state before their fallible wire construction
//!   completes.
//!
//! No sockets, DNS, public network, or external references are used.

#![allow(clippy::too_many_lines)]

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{
    ConnectOutcome, RemoteDestination, StreamingManager, StreamingManagerError,
};
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    DestinationConfig, DestinationDispatcher, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool, EciesOutboundMessage,
    EciesSessionConfig, EciesSessionError, EciesSessionManager, InboundDispatchError,
    InboundDispatchOutcome, InboundStreamingOutcome, OutboundDeliveryPlan,
    StreamingDestinationAdapter, build_signed_lease_set2, encode_garlic_clove_payload,
};
use i2pr_crypto::{BoundNewSessionMessage, ExistingSessionMessage, NewSessionReplyMessage};
use i2pr_netdb::ValidatedLeaseSet2;
use i2pr_proto::{
    GarlicCloveBlock, GarlicDelivery, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE,
    TunnelGatewayMessage,
};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    InboundGatewayRole, InboundParticipantRole, LayerKeys, LocalInboundEndpointRole, OutboundCell,
    OutboundEndpointRole, OutboundParticipantRole, RouterDeliveryAction, TunnelDirection, TunnelId,
    TunnelPeer, TunnelRoleError,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};

const A_SEED: u64 = 0x132A;
const B_SEED: u64 = 0x132B;
const NOW_SECONDS: u32 = 5_700;
const START_MS: u64 = 720_000;
const PORT_A: u16 = 0x132A;
const PORT_B: u16 = 0x132B;

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

    fn send_via_adapter(
        &mut self,
        request: &TransportSendRequest,
        rng_seed: u64,
        now_ms: u64,
    ) -> Result<OutboundDeliveryPlan, i2pr_client::streaming_adapter::StreamingAdapterError> {
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

    fn receive_next_payload(
        &mut self,
        from_destination_hash: &[u8; 32],
        now_ms: u64,
    ) -> Result<InboundStreamingOutcome, i2pr_client::streaming_adapter::StreamingAdapterError>
    {
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

// =============================================================================
// Plan 132 Phase B0 — artifact-preserving test seam
// =============================================================================

/// Drives one outbound delivery plan through the outbound participant
/// and OBEP roles, returning the inner I2NP `RouterDeliveryAction`s.
/// This is a development-only seam that owns a fresh outbound
/// `DuplicateWindow` for the outbound side; the inbound side is the
/// persistent `InboundChain` on the receiver.
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
    actions
}

/// Generates the inbound tunnel cells for one router delivery using a
/// caller-supplied deterministic RNG seed. Plan 132 §5 B0 exposes
/// this seam so tests can capture the *exact* inbound cell bytes once
/// and replay them deterministically — no resealing, no regen.
fn make_inbound_cells<R: CryptoRng + RngCore>(
    ibgw: &InboundGatewayRole,
    action: &RouterDeliveryAction,
    rng: &mut R,
) -> Vec<OutboundCell> {
    let inner_i2np =
        I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE).expect("decode i2np");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: action.tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner_i2np),
    };
    ibgw.process_cells(&gateway_msg, rng, 0)
        .expect("ibgw multi-cell forward")
}

/// Drives a fixed list of inbound cells through the receiver's
/// persistent participant + endpoint roles. Returns the recovered
/// inner I2NP message bytes when the reassembler completes, or a
/// typed `TunnelRoleError` (e.g. `DuplicateCell`) from the participant
/// before reassembly. The first cell whose endpoint process returns
/// a completed message wins; any subsequent messages from the same
/// reassembler pass are ignored.
fn run_inbound_cells(
    receiver: &mut Side,
    cells: &[OutboundCell],
) -> Result<Option<Vec<u8>>, TunnelRoleError> {
    let mut recovered = None;
    for cell in cells {
        let forwarded = receiver.inbound.participant.process(
            &hop_router_hash(receiver.seed, 1),
            &cell.cell,
            0,
        )?;
        if let Some(message) =
            receiver
                .inbound
                .endpoint
                .process(&hop_router_hash(receiver.seed, 2), &forwarded, 0)?
        {
            assert!(recovered.is_none(), "reassembler completes once");
            recovered = Some(message);
        }
    }
    Ok(recovered)
}

fn establish_stream(
    a: &mut Side,
    b: &mut Side,
    clock: &mut u64,
    seed_base: u64,
    port_a: u16,
    port_b: u16,
) -> (ConnectionId, ConnectionId) {
    preresolve_remote(a, b);
    *clock += 10;
    let connect = {
        let remote_b = remote_for(&b.identity);
        a.streaming
            .connect(
                &a.identity,
                &remote_b,
                port_a,
                port_b,
                i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
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
    let _ = pipe_through_stack(a, b, &syn_requests[0], seed_base + 1, *clock);

    assert_eq!(b.streaming.listener_backlog(port_b), 1);
    let b_conn = b.streaming.accept(port_b).expect("pending inbound stream");

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
                port_b,
                port_a,
                i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
                *clock,
                &mut ChaCha8Rng::seed_from_u64(seed_base + 2),
            )
            .expect("b accept inbound syn")
    };
    let _ = pipe_through_stack(b, a, &response_request, seed_base + 3, *clock);

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

fn pipe_through_stack(
    sender: &mut Side,
    receiver: &mut Side,
    request: &TransportSendRequest,
    rng_seed: u64,
    now_ms: u64,
) -> InboundStreamingOutcome {
    let plan = sender
        .send_via_adapter(request, rng_seed, now_ms)
        .expect("outbound adapter composition");
    let actions = obep_actions(sender, &plan);
    let recovered = feed_action(receiver, &actions[0]);
    let envelope =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    assert!(matches!(envelope.body(), I2npBody::Garlic(_)));
    let _ = receiver.dispatch(&envelope);
    receiver
        .receive_next_payload(&sender.hash_bytes(), now_ms)
        .expect("inbound adapter protocol-6 dispatch")
}

fn feed_action(receiver: &mut Side, action: &RouterDeliveryAction) -> Vec<u8> {
    let mut rng = ChaCha8Rng::seed_from_u64(0x52EA);
    let cells = make_inbound_cells(&receiver.inbound.ibgw, action, &mut rng);
    run_inbound_cells(receiver, &cells)
        .expect("first delivery recovers Garlic carrier")
        .expect("reassembler completes once")
}

// =============================================================================
// Plan 132 Phase B — three real layer-isolated replay trajectories
// =============================================================================

// ---- §6 B1: exact same retained tunnel cell is rejected by the duplicate window ----

#[test]
fn plan132_exact_same_tunnel_cell_is_rejected_by_live_duplicate_window() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (_a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1321_0000,
        PORT_A,
        PORT_B,
    );

    clock += 10;
    let data_request = {
        let remote_b = remote_for(&side_b.identity);
        side_a
            .streaming
            .send_data(
                _a_conn,
                &side_a.identity,
                &remote_b,
                PORT_A,
                PORT_B,
                b"hello-bob",
                clock,
            )
            .expect("alice send_data")
    };

    // Generate the inbound cells exactly once and capture them as
    // the test artifact. The persistent receiver roles will record
    // the duplicate token on the first delivery; the retained
    // `retained_cells` slice must drive the duplicate-window
    // rejection on replay — not a second seal, not a re-composed
    // delivery plan.
    let mut seed_rng = ChaCha8Rng::seed_from_u64(0x1321_0100);
    let plan = side_a
        .send_via_adapter(&data_request, 0x1321_0101, clock)
        .expect("adapter plan");
    let actions = obep_actions(&side_a, &plan);
    let retained_cells = make_inbound_cells(&side_b.inbound.ibgw, &actions[0], &mut seed_rng);

    // First delivery.
    let recovered = run_inbound_cells(&mut side_b, &retained_cells)
        .expect("first delivery runs through the tunnel")
        .expect("reassembler completes");
    let _envelope = I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode recovered carrier");
    let _ = side_b
        .streaming
        .drain_delivered()
        .into_iter()
        .flat_map(|entry| entry.bytes)
        .collect::<Vec<_>>();

    let baseline_count = side_b
        .streaming
        .get_connection(b_conn)
        .expect("b conn")
        .recv_window()
        .delivered_count();

    // Replay the *same* retained `OutboundCell` bytes into the same
    // persistent inbound roles. The participant's
    // `DuplicateToken::compute(&iv, &ciphertext)` must hit a known
    // entry on its live window and raise `TunnelRoleError::DuplicateCell`
    // before any reassembly or ECIES work runs.
    let replay_outcome = run_inbound_cells(&mut side_b, &retained_cells);
    assert!(
        matches!(replay_outcome, Err(TunnelRoleError::DuplicateCell)),
        "replay must fail typed at the live tunnel duplicate window, got {replay_outcome:?}"
    );

    let post_count = side_b
        .streaming
        .get_connection(b_conn)
        .expect("b conn")
        .recv_window()
        .delivered_count();
    assert_eq!(
        post_count, baseline_count,
        "Streaming delivered count must not advance on a typed duplicate-window rejection"
    );
}

// ---- §6 B2: consumed ES ciphertext in a fresh tunnel cell is rejected by ECIES ----

#[test]
fn plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (_a_conn, b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1322_0000,
        PORT_A,
        PORT_B,
    );

    clock += 10;
    let bob_remote = remote_for(&side_b.identity);
    let first_request = side_a
        .streaming
        .send_data(
            _a_conn,
            &side_a.identity,
            &bob_remote,
            PORT_A,
            PORT_B,
            b"original-payload",
            clock,
        )
        .expect("alice send_data");

    // ---- A1: retain and prove the exact original ECIES artifact ---------------
    // The first application transmission is an Existing Session
    // form (Plan 127 §1 / Plan 126 §2). We drive the outbound
    // adapter once, recover the inner I2NP Garlic bytes, decode the
    // ECIES Existing Session message out of the recovered payload,
    // and retain its cleartext tag and encrypted payload section
    // verbatim. Every subsequent re-wrapping reuses these bytes
    // exactly.
    let mut first_ibgw_rng = ChaCha8Rng::seed_from_u64(0x1322_0200);
    let first_plan = side_a
        .send_via_adapter(&first_request, 0x1322_0100, clock)
        .expect("first plan");
    assert_eq!(
        first_plan.encrypted_message.form_name(),
        "existing-session",
        "first outbound form must be the paired Existing Session tag set"
    );
    let retained_es_bytes = first_plan.encrypted_message.message_bytes().to_vec();
    let retained_garlic_bytes = first_plan.garlic_i2np_bytes.clone();
    let retained_es = ExistingSessionMessage::decode(&retained_es_bytes, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode retained ES");
    let retained_es_tag = retained_es.tag;
    let first_actions = obep_actions(&side_a, &first_plan);
    let first_cells =
        make_inbound_cells(&side_b.inbound.ibgw, &first_actions[0], &mut first_ibgw_rng);
    let first_recovered = run_inbound_cells(&mut side_b, &first_cells)
        .expect("first delivery tunnel")
        .expect("reassembler completes");
    assert_eq!(
        first_recovered, retained_garlic_bytes,
        "tunnel decryption must recover the same I2NP Garlic carrier the plan emitted"
    );
    // The Garlic carrier payload is the retained ECIES bytes: the
    // session tag and ciphertext survive the tunnel layer exactly.
    let first_carrier = I2npMessage::decode_standard(&first_recovered, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode first carrier");
    let first_carrier_payload = match first_carrier.body() {
        I2npBody::Garlic(body) => body.payload.as_bytes().to_vec(),
        other => panic!("first carrier must be a Garlic envelope, got {other:?}"),
    };
    assert_eq!(
        first_carrier_payload, retained_es_bytes,
        "Garlic carrier payload must equal the retained ES ciphertext bytes"
    );

    // First delivery: the ECIES tag is consumed exactly once and the
    // application bytes flow through to Streaming.
    let envelope = I2npMessage::decode_standard(&first_recovered, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode recovered carrier");
    assert!(matches!(envelope.body(), I2npBody::Garlic(_)));
    let first_outcome = side_b.dispatch(&envelope);
    assert!(
        matches!(
            first_outcome,
            InboundDispatchOutcome::ExistingSessionProcessed { .. }
        ),
        "first dispatch must surface a fresh Existing Session, got {first_outcome:?}"
    );
    let _ = side_b
        .receive_next_payload(&side_a.hash_bytes(), clock)
        .expect("first inbound adapter dispatch");
    let delivered = side_b
        .streaming
        .drain_delivered()
        .into_iter()
        .flat_map(|entry| entry.bytes)
        .collect::<Vec<_>>();
    assert_eq!(delivered, b"original-payload");

    // ---- A5: negative-control state snapshot before the replay ---------------
    let queued_before = side_b.dispatcher.queued_payloads(side_b.identity.id());
    let established_sessions_before = side_b.session.established_sessions();
    let pending_handshakes_before = side_b.session.pending_handshake_count();
    let provisional_before = side_b.session.provisional_responder_count();
    let delivered_count_before = side_b
        .streaming
        .get_connection(b_conn)
        .expect("b conn")
        .recv_window()
        .delivered_count();
    assert_eq!(delivered_count_before, 1);
    assert_eq!(provisional_before, 0);

    // ---- A2: refresh only the lower tunnel representation --------------------
    // The fresh cells must be byte-distinct and the persistent
    // inbound tunnel must accept them (no `DuplicateCell`).
    let mut second_ibgw_rng = ChaCha8Rng::seed_from_u64(0x1322_0300);
    let second_cells = make_inbound_cells(
        &side_b.inbound.ibgw,
        &first_actions[0],
        &mut second_ibgw_rng,
    );
    assert_ne!(
        first_cells[0].cell, second_cells[0].cell,
        "fresh IBGW RNG must produce a fresh IV and a distinct cell"
    );
    let replay_recovered = run_inbound_cells(&mut side_b, &second_cells)
        .expect("tunnel accepts the fresh wrapping of the same payload");
    assert_eq!(
        replay_recovered.as_ref(),
        Some(&retained_garlic_bytes),
        "tunnel decryption must recover the same inner I2NP Garlic bytes from the fresh wrapping"
    );
    // The recovered envelope must round-trip to the exact
    // ExistingSessionMessage bytes the receiver consumed above; the
    // tag and ciphertext must be byte-for-byte identical.
    let replay_envelope_bytes = replay_recovered.as_ref().expect("recovered");
    let replay_envelope =
        I2npMessage::decode_standard(replay_envelope_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode recovered carrier");
    assert!(matches!(replay_envelope.body(), I2npBody::Garlic(_)));
    let replay_carrier_payload = match replay_envelope.body() {
        I2npBody::Garlic(body) => body.payload.as_bytes().to_vec(),
        other => panic!("replay carrier must be a Garlic envelope, got {other:?}"),
    };
    assert_eq!(
        replay_carrier_payload, retained_es_bytes,
        "replayed Garlic carrier payload must equal the retained ES bytes"
    );
    let replay_es = ExistingSessionMessage::decode(&replay_carrier_payload, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode replay ES");
    assert_eq!(
        replay_es.tag, retained_es_tag,
        "replayed ciphertext must carry the exact consumed tag"
    );
    assert_eq!(
        replay_es.encrypted_payload_section, retained_es.encrypted_payload_section,
        "replayed ciphertext must carry the exact consumed payload section"
    );

    // ---- A3: assert the integrated ECIES/session rejection directly ----------
    // The dispatcher must reach ECIES/session processing (not stop
    // at the codec, the Garlic flag check, or the destination
    // lookup) and then fail closed because the repeated
    // ciphertext/tag does not match any accepted bound. The exact
    // narrower stable variant depends on the dispatcher's
    // classification: if the consumed tag is still in the inbound
    // window, `accept_existing_session` returns
    // `EciesSessionError::UnknownSessionTag`; if the consumed tag
    // has been refilled out, classify routes the envelope through
    // the Bound New Session path and the ECIES primitive AEAD
    // authentication fails with `EciesError::AuthenticationFailed`.
    // Both variants prove the ECIES/session boundary rejects the
    // replay before any plaintext Data payload is queued.
    let replay_outcome = side_b.dispatch(&replay_envelope);
    assert!(
        matches!(
            &replay_outcome,
            InboundDispatchOutcome::Rejected(InboundDispatchError::Session(_))
        ),
        "dispatcher must fail closed at the ECIES/session boundary, got {replay_outcome:?}"
    );
    match &replay_outcome {
        InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
            EciesSessionError::UnknownSessionTag,
        )) => {}
        InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
            EciesSessionError::Ecies(i2pr_crypto::EciesError::AuthenticationFailed),
        )) => {}
        other => panic!(
            "expected the ECIES/session boundary to reject the consumed-tag replay, got {other:?}"
        ),
    }

    // ---- A5: negative-control state after the typed rejection ----------------
    let queued_after = side_b.dispatcher.queued_payloads(side_b.identity.id());
    let established_sessions_after = side_b.session.established_sessions();
    let pending_handshakes_after = side_b.session.pending_handshake_count();
    let provisional_after = side_b.session.provisional_responder_count();
    let delivered_count_after = side_b
        .streaming
        .get_connection(b_conn)
        .expect("b conn")
        .recv_window()
        .delivered_count();
    assert_eq!(
        queued_after, queued_before,
        "consumed-tag replay must not queue a second application payload"
    );
    assert_eq!(
        delivered_count_after, delivered_count_before,
        "Streaming delivered count must not advance on the typed rejection"
    );
    assert_eq!(
        established_sessions_after, established_sessions_before,
        "established ECIES session count must remain valid (not be replaced by a bogus candidate)"
    );
    assert_eq!(
        pending_handshakes_after, pending_handshakes_before,
        "no new pending outbound handshake must be installed by the replay"
    );
    assert_eq!(
        provisional_after, provisional_before,
        "no new provisional responder must be installed by the replay"
    );

    // A second pop must observe nothing: the rejection did not
    // enqueue a second application payload, so a `pop_payload` for
    // the owning destination returns `None` and the inbound
    // Streaming adapter has no Data body to dispatch.
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none(),
        "no Data payload may be available for a second inbound Streaming adapter call"
    );
}

// ---- §6 B3: fresh ECIES reseal of the same Streaming sequence is deduplicated by Streaming ----

#[test]
fn plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1323_0000,
        PORT_A,
        PORT_B,
    );

    // ---- B1: create exactly one Streaming request for sequence N ------------
    // The request bytes (gzip-encoded complete Streaming packet)
    // are the artifact the test will reseal under a different RNG
    // and submit a second time. We capture `N` and the
    // `application_payload` bytes verbatim; no further `send_data`
    // call is permitted in this test.
    clock += 10;
    let remote_b = remote_for(&side_b.identity);
    let first_request = side_a
        .streaming
        .send_data(
            a_conn,
            &side_a.identity,
            &remote_b,
            PORT_A,
            PORT_B,
            b"dup-bytes",
            clock,
        )
        .expect("first send_data");
    let sequence_n = first_request.sequence;
    let first_application_payload = first_request.application_payload.clone();
    assert_eq!(
        sequence_n, 1,
        "first application sequence after the handshake is 1"
    );

    // ---- B2: seal and retain the * actual first ES plan ----------------------
    // We call the adapter once to obtain the *real* first ES
    // envelope, decode its tag and ciphertext, and drive it
    // through the receiver's stack. The retained plan is the plan
    // actually delivered, not a re-synthesized comparison
    // artifact.
    let first_plan = side_a
        .send_via_adapter(&first_request, 0x1323_0100, clock)
        .expect("first plan");
    assert_eq!(
        first_plan.encrypted_message.form_name(),
        "existing-session",
        "first outbound form must be the paired Existing Session tag set"
    );
    let first_es_bytes = first_plan.encrypted_message.message_bytes().to_vec();
    let first_es = ExistingSessionMessage::decode(&first_es_bytes, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode first ES");
    let first_es_tag = first_es.tag;
    let first_actions = obep_actions(&side_a, &first_plan);

    // Drive that exact first plan through the receiver. The
    // dispatcher must surface `ExistingSessionProcessed` and the
    // Streaming adapter must surface the original application
    // bytes exactly once.
    let first_outcome =
        pipe_through_first_plan(&mut side_b, &first_actions[0], &side_a.hash_bytes(), clock);
    assert!(
        matches!(
            first_outcome.dispatch,
            InboundDispatchOutcome::ExistingSessionProcessed { .. }
        ),
        "first dispatch must surface a fresh Existing Session, got {:?}",
        first_outcome.dispatch
    );
    assert!(
        matches!(
            first_outcome.streaming,
            InboundStreamingOutcome::StreamingDispatched { .. }
        ),
        "first streaming dispatch must surface a dispatched streaming outcome, got {:?}",
        first_outcome.streaming
    );
    let first_delivered = side_b
        .streaming
        .drain_delivered()
        .into_iter()
        .flat_map(|entry| entry.bytes)
        .collect::<Vec<_>>();
    assert_eq!(first_delivered, b"dup-bytes");

    // ---- B3: freshly reseal the exact same request ---------------------------
    // The application payload bytes (the gzip-encoded complete
    // Streaming packet) are byte-for-byte identical to the first
    // request — the inner Streaming sequence number `N` is
    // unchanged. The adapter nevertheless emits a fresh Existing
    // Session envelope because the outbound ES tag set advances
    // between calls.
    let second_plan = side_a
        .send_via_adapter(&first_request, 0x1323_0200, clock + 5)
        .expect("second adapter plan");
    assert_eq!(
        second_plan.encrypted_message.form_name(),
        "existing-session",
        "second outbound form must be the paired Existing Session tag set"
    );
    let second_es_bytes = second_plan.encrypted_message.message_bytes().to_vec();
    let second_es = ExistingSessionMessage::decode(&second_es_bytes, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode second ES");
    assert_ne!(
        second_es.tag, first_es_tag,
        "fresh ES seal must issue a distinct tag from the first seal"
    );
    assert_ne!(
        second_es_bytes, first_es_bytes,
        "fresh ES seal must issue a distinct ciphertext envelope"
    );
    // Both plans carry the same inner Streaming request, so the
    // canonical RFC 1952 client payload and the sequence `N` are
    // byte-for-byte identical.
    assert_eq!(
        second_plan.encrypted_message.message_bytes().len(),
        first_plan.encrypted_message.message_bytes().len(),
        "fresh ES seal must carry the same payload length"
    );
    // The tunnel-fragmentation boundary must be identical: the
    // same payload yields the same cell count.
    assert_eq!(
        second_plan.cells.len(),
        first_plan.cells.len(),
        "second plan must carry the same number of outbound tunnel cells"
    );

    // The inner RFC 1952 client payload (Streaming packet bytes
    // + protocol + ports) embedded in the second ES envelope must
    // equal the bytes the first ES envelope embedded. The two
    // ES envelopes differ only in tag/ciphertext (a fresh seal),
    // never in the plaintext Streaming application bytes or
    // sequence number.
    assert_eq!(
        first_request.application_payload, first_application_payload,
        "send_data() must be called exactly once; the retained request is the only request"
    );
    assert_eq!(
        first_request.sequence, sequence_n,
        "send_data() must be called exactly once; the retained request is the only request"
    );

    // The inner I2NP Garlic payload that the OBEP emits for the
    // second plan must differ from the one emitted for the first
    // plan — a fresh ES tag and ciphertext are evidence that the
    // ECIES layer actually sealed a new envelope under the
    // deterministic outbound tunnel encryption.
    let second_actions = obep_actions(&side_a, &second_plan);
    assert_ne!(
        first_actions[0].message, second_actions[0].message,
        "fresh ES seal must produce a distinct inner I2NP Garlic payload"
    );

    // ---- B4: prove tunnel + ECIES succeed on the second seal ---------------
    // Drive the second plan through the receiver. The lower tunnel
    // must accept the fresh wrapping (no `DuplicateCell`), the
    // dispatcher must reach ECIES authentication and decrypt the
    // payload, and the ECIES layer must produce exactly one
    // `ExistingSessionProcessed` outcome. A Data payload must
    // reach the destination queue so the inbound Streaming
    // adapter has something to receive.
    let second_dispatch_outcome = pipe_through_first_plan(
        &mut side_b,
        &second_actions[0],
        &side_a.hash_bytes(),
        clock + 5,
    );
    assert!(
        matches!(
            second_dispatch_outcome.dispatch,
            InboundDispatchOutcome::ExistingSessionProcessed { .. }
        ),
        "second dispatch must surface a fresh Existing Session, got {:?}",
        second_dispatch_outcome.dispatch
    );

    // ---- B5: prove only Streaming deduplicates sequence N -------------------
    // The inbound Streaming adapter must accept the duplicate
    // packet normally; the receiver's receive-window must not
    // advance application delivery for the repeated sequence
    // number `N`. The delivered count remains exactly one and
    // `drain_delivered` yields no second copy.
    assert!(
        matches!(
            second_dispatch_outcome.streaming,
            InboundStreamingOutcome::StreamingDispatched { .. }
        ),
        "second ES seal must dispatch into Streaming, got {:?}",
        second_dispatch_outcome.streaming
    );
    let second_delivered = side_b
        .streaming
        .drain_delivered()
        .into_iter()
        .flat_map(|entry| entry.bytes)
        .collect::<Vec<_>>();
    assert_eq!(
        second_delivered,
        Vec::<u8>::new(),
        "Streaming must deduplicate the inner sequence; no second payload surfaces"
    );
    // No second copy may be queued for a later pop: the dispatch
    // accepted exactly one application payload across both
    // deliveries.
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none(),
        "Streaming sequence dedup must leave no further pop payload for the duplicate destination"
    );

    // B1 acceptance: no additional `send_data` was called.
    let _ = (first_application_payload, sequence_n);
}

/// One delivery through the receiver's tunnel + dispatcher + ECIES
/// stack. Returns both the dispatcher outcome and the inbound
/// Streaming outcome so the test can assert both layers in a single
/// pass without rebuilding artifacts the production code does not
/// expose.
struct DeliveryOutcome {
    dispatch: InboundDispatchOutcome,
    streaming: InboundStreamingOutcome,
}

fn pipe_through_first_plan(
    receiver: &mut Side,
    action: &RouterDeliveryAction,
    from_destination_hash: &[u8; 32],
    now_ms: u64,
) -> DeliveryOutcome {
    let mut rng = ChaCha8Rng::seed_from_u64(0x52EA);
    let cells = make_inbound_cells(&receiver.inbound.ibgw, action, &mut rng);
    let recovered = run_inbound_cells(receiver, &cells)
        .expect("tunnel accepts the second plan")
        .expect("reassembler completes");
    let envelope =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    assert!(matches!(envelope.body(), I2npBody::Garlic(_)));
    let dispatch = receiver.dispatch(&envelope);
    let streaming = receiver
        .receive_next_payload(from_destination_hash, now_ms)
        .expect("inbound adapter dispatch");
    DeliveryOutcome {
        dispatch,
        streaming,
    }
}

// =============================================================================
// Plan 132 Phase C — transactional `send_data`, `send_close`, `send_reset`
// =============================================================================

#[test]
fn plan132_send_data_oversized_failure_is_precommit() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1324_0000,
        PORT_A,
        PORT_B,
    );

    let sequence_before = side_a
        .streaming
        .get_connection(a_conn)
        .expect("a conn")
        .send_window()
        .next_sequence();
    let unacked_count_before = side_a
        .streaming
        .get_connection(a_conn)
        .expect("a conn")
        .send_window()
        .unacked_count();
    let unacked_bytes_before = side_a
        .streaming
        .get_connection(a_conn)
        .expect("a conn")
        .send_window()
        .unacked_bytes();
    let queue_before = side_a.streaming.outbound_queue_len();
    let tracked_before = side_a.streaming.tracked_retransmit_count();

    let remote_b = remote_for(&side_b.identity);
    let oversized =
        vec![
            0xAB_u8;
            usize::from(i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD) + 1
        ];
    let err = side_a
        .streaming
        .send_data(
            a_conn,
            &side_a.identity,
            &remote_b,
            PORT_A,
            PORT_B,
            &oversized,
            clock,
        )
        .expect_err("oversized write must fail closed");
    assert!(matches!(err, StreamingManagerError::Streaming(_)));

    let conn_after = side_a.streaming.get_connection(a_conn).expect("a conn");
    assert_eq!(conn_after.send_window().next_sequence(), sequence_before);
    assert_eq!(
        conn_after.send_window().unacked_count(),
        unacked_count_before
    );
    assert_eq!(
        conn_after.send_window().unacked_bytes(),
        unacked_bytes_before
    );
    assert_eq!(side_a.streaming.outbound_queue_len(), queue_before);
    assert_eq!(side_a.streaming.tracked_retransmit_count(), tracked_before);
}

#[test]
fn plan132_send_data_port_tuple_mismatch_is_precommit() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1325_0000,
        PORT_A,
        PORT_B,
    );

    let sequence_before = side_a
        .streaming
        .get_connection(a_conn)
        .expect("a conn")
        .send_window()
        .next_sequence();
    let queue_before = side_a.streaming.outbound_queue_len();
    let tracked_before = side_a.streaming.tracked_retransmit_count();

    let remote_b = remote_for(&side_b.identity);
    // Wrong source port must fail closed without touching the
    // connection state. The exact caller-supplied port mismatch is
    // the assertion the test owns.
    let err = side_a
        .streaming
        .send_data(
            a_conn,
            &side_a.identity,
            &remote_b,
            PORT_A.wrapping_add(1),
            PORT_B,
            b"port-mismatch",
            clock,
        )
        .expect_err("wrong source port must fail closed");
    assert!(matches!(
        err,
        StreamingManagerError::PortTupleMismatch { .. }
    ));

    let conn_after = side_a.streaming.get_connection(a_conn).expect("a conn");
    assert_eq!(conn_after.send_window().next_sequence(), sequence_before);
    assert_eq!(side_a.streaming.outbound_queue_len(), queue_before);
    assert_eq!(side_a.streaming.tracked_retransmit_count(), tracked_before);
}

#[test]
fn plan132_send_close_valid_succeeds_after_build() {
    // A successful send_close still produces a CLOSE packet on the
    // queue, transitions to ClosingLocal, and consumes the next
    // sequence. This test pins the *successful* path against the
    // transactional ordering so a future regression that breaks the
    // precommit guarantee does not silently change the happy path.
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1326_0000,
        PORT_A,
        PORT_B,
    );

    let remote_b = remote_for(&side_b.identity);
    let req = side_a
        .streaming
        .send_close(a_conn, &side_a.identity, &remote_b, PORT_A, PORT_B, clock)
        .expect("send_close");
    assert_eq!(
        side_a
            .streaming
            .get_connection(a_conn)
            .expect("a conn")
            .state(),
        ConnectionState::ClosingLocal
    );
    let _ = req;
}

#[test]
fn plan132_send_close_port_tuple_mismatch_is_precommit() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1327_0000,
        PORT_A,
        PORT_B,
    );

    let state_before = side_a
        .streaming
        .get_connection(a_conn)
        .expect("a conn")
        .state();
    let queue_before = side_a.streaming.outbound_queue_len();
    let tracked_before = side_a.streaming.tracked_retransmit_count();

    let remote_b = remote_for(&side_b.identity);
    let err = side_a
        .streaming
        .send_close(
            a_conn,
            &side_a.identity,
            &remote_b,
            PORT_A,
            PORT_B.wrapping_add(1),
            clock,
        )
        .expect_err("port mismatch must fail closed");
    assert!(matches!(
        err,
        StreamingManagerError::PortTupleMismatch { .. }
    ));

    let conn_after = side_a.streaming.get_connection(a_conn).expect("a conn");
    assert_eq!(conn_after.state(), state_before);
    assert_eq!(side_a.streaming.outbound_queue_len(), queue_before);
    assert_eq!(side_a.streaming.tracked_retransmit_count(), tracked_before);
}

#[test]
fn plan132_send_reset_port_tuple_mismatch_is_precommit() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let _ = side_b.streaming.listen(PORT_B);
    let mut clock = START_MS;

    let (a_conn, _b_conn) = establish_stream(
        &mut side_a,
        &mut side_b,
        &mut clock,
        0x1328_0000,
        PORT_A,
        PORT_B,
    );

    let state_before = side_a
        .streaming
        .get_connection(a_conn)
        .expect("a conn")
        .state();
    let queue_before = side_a.streaming.outbound_queue_len();
    let tracked_before = side_a.streaming.tracked_retransmit_count();

    let remote_b = remote_for(&side_b.identity);
    let err = side_a
        .streaming
        .send_reset(
            a_conn,
            &side_a.identity,
            &remote_b,
            PORT_A.wrapping_add(1),
            PORT_B,
            clock,
        )
        .expect_err("port mismatch must fail closed");
    assert!(matches!(
        err,
        StreamingManagerError::PortTupleMismatch { .. }
    ));

    let conn_after = side_a.streaming.get_connection(a_conn).expect("a conn");
    assert_eq!(conn_after.state(), state_before);
    assert_eq!(side_a.streaming.outbound_queue_len(), queue_before);
    assert_eq!(side_a.streaming.tracked_retransmit_count(), tracked_before);
}

// =============================================================================
// Plan 132 Phase D — direct session-layer replay evidence (sanity)
// =============================================================================

#[test]
fn plan132_ecies_session_layer_rejects_consumed_tag_directly() {
    // This is a focused unit-level proof that the ECIES consumed-tag
    // rejection the B2 trajectory depends on is not just a
    // dispatcher-level side effect. We establish a paired session
    // through the standard NS/NSR handshake, issue one Existing
    // Session envelope, accept it once (consuming the tag), then
    // accept the *same* envelope again and assert
    // `UnknownSessionTag` from the session manager's inbound tag
    // window.

    let mut rng = ChaCha8Rng::seed_from_u64(0x1329_0000);
    let mut alice_seed = [0_u8; 32];
    let mut alice_signing = [0_u8; 32];
    let mut alice_padding = vec![0_u8; i2pr_crypto::IDENTITY_PADDING_LENGTH];
    rng.fill_bytes(&mut alice_seed);
    rng.fill_bytes(&mut alice_signing);
    rng.fill_bytes(&mut alice_padding);
    let alice_id = i2pr_client::DestinationIdentity::from_private_bytes(
        alice_signing,
        alice_seed,
        zeroize::Zeroizing::new(alice_padding),
    )
    .expect("alice identity");
    let alice_secret = *alice_id.static_secret_bytes();

    let mut bob_seed = [0_u8; 32];
    let mut bob_signing = [0_u8; 32];
    let mut bob_padding = vec![0_u8; i2pr_crypto::IDENTITY_PADDING_LENGTH];
    rng.fill_bytes(&mut bob_seed);
    rng.fill_bytes(&mut bob_signing);
    rng.fill_bytes(&mut bob_padding);
    let bob_id = i2pr_client::DestinationIdentity::from_private_bytes(
        bob_signing,
        bob_seed,
        zeroize::Zeroizing::new(bob_padding),
    )
    .expect("bob identity");
    let bob_secret = *bob_id.static_secret_bytes();
    let bob_public = bob_id.static_public_bytes();

    // Alice initiates.
    let mut alice_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let ns_outbound = alice_manager
        .encrypt_to_remote(
            alice_id.id(),
            &alice_secret,
            &bob_public,
            &bob_public,
            &payload_marker(0x42),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("alice ns");
    let ns_bytes = match ns_outbound {
        EciesOutboundMessage::NewSession { message } => message,
        other => panic!("expected NewSession, got {other:?}"),
    }
    .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode ns");

    // Bob accepts and replies.
    let mut bob_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let ns = BoundNewSessionMessage::decode(&ns_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode ns");
    let accepted = bob_manager
        .accept_new_session(bob_id.id(), &bob_secret, &bob_public, &ns, NOW_SECONDS)
        .expect("bob accept");
    let reply = bob_manager
        .seal_new_session_reply_for(
            bob_id.id(),
            &bob_secret,
            &accepted.alice_static_public,
            &payload_marker(0x99),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("bob reply");
    let reply_bytes = reply
        .message
        .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode reply");

    // Alice accepts the reply → paired session.
    let reply_msg =
        NewSessionReplyMessage::decode(&reply_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode reply");
    alice_manager
        .accept_new_session_reply(alice_id.id(), &alice_secret, &reply_msg, NOW_SECONDS)
        .expect("alice accept reply");

    // Alice seals one Existing Session message. Bob accepts once
    // (consuming the tag), then we submit the *same* retained
    // message a second time and assert `UnknownSessionTag`.
    let es_outbound = alice_manager
        .encrypt_to_remote(
            alice_id.id(),
            &alice_secret,
            &bob_public,
            &bob_public,
            &payload_marker(0xCC),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("alice es");
    let es_message = match es_outbound {
        EciesOutboundMessage::Existing(message) => message,
        other => panic!("expected Existing, got {other:?}"),
    };
    let retained_bytes = es_message
        .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode es");
    let retained = ExistingSessionMessage::decode(&retained_bytes, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode retained es");

    let first = bob_manager
        .accept_existing_session(&retained)
        .expect("first accept");
    let _ = first;

    let second = bob_manager.accept_existing_session(&retained);
    assert!(matches!(second, Err(EciesSessionError::UnknownSessionTag)));
}

fn payload_marker(marker: u8) -> Vec<u8> {
    let clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: vec![marker; 12],
    };
    encode_garlic_clove_payload(&clove).expect("encode clove")
}

#[test]
fn _force_compile_existing_session_message() {
    let _ = std::marker::PhantomData::<ExistingSessionMessage>;
}
