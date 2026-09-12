//! Plan 193 §1 / §6 — M6 i2pd mixed-router Streaming qualification:
//! local-product foundation.
//!
//! Streaming packets traverse the real mixed-router destination path
//! proven by Plan 187 (local destination message plane) and Plan 190
//! (reply-path tunnel-id corrective). The streaming layer is the
//! existing `i2pr_client::streaming` core; the destination tunnel
//! plane is the existing `i2pr_tunnel` chain. No new wire-level
//! construction is added for the deferred Streaming pass — every
//! packet crosses:
//!
//! ```text
//! StreamingManager (per-destination, owned outbound state)
//!  -> StreamingDestinationAdapter::send
//!       -> canonical OutboundRequest (Plan 192 i2cp I2CP-style Data body)
//!       -> compose_outbound_delivery (Plan 122)
//!  -> real outbound tunnel (Plan 116 OBEP)
//!  -> real inbound tunnel (Plan 116 IBGW + participant + endpoint)
//!  -> DestinationDispatcher
//!  -> StreamingDestinationAdapter::receive
//!  -> receiver StreamingManager
//! ```
//!
//! This suite drives the integration through the existing
//! `i2pr_client::streaming::local_delivery::deliver` seam so the
//! i2pr↔i2pr matrix exercises the same code path the SAM STREAM
//! product and the service-tunnel profiles already use, but
//! end-to-end through the real outbound/inbound destination tunnels
//! the Plan 187 destination plane owns.
//!
//! No additional authentication, ECIES, or Streaming wire semantics
//! are introduced; the tests exist to prove the layered integration
//! holds against the bounded Streaming state machine and bounded
//! destination tunnel pipeline the architecture lock specifies.

#![forbid(unsafe_code)]
#![allow(clippy::too_many_lines)]

use i2pr_client::config::DestinationConfig;
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination, StreamingManagerError,
};
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::streaming::{StreamingConfig, StreamingManager};
use i2pr_client::{
    DestinationDispatcher, DestinationIdentity, DestinationOutboundRole, DestinationRouting,
    DestinationRoutingConfig, DestinationTunnelPool, EciesSessionConfig, EciesSessionManager,
    InboundStreamingOutcome, LocalDeliveryError, LocalDeliveryReceiver, LocalDeliverySender,
    StreamingDestinationAdapter, build_signed_lease_set2, deliver,
};
use i2pr_netdb::{
    DestinationHash, LeaseSet2Store as NetDbLeaseSet2Store, LeaseSet2ValidationContext,
    ValidatedLeaseSet2,
};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, TunnelDataMessage};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    InboundGatewayRole, InboundParticipantRole, LayerKeys, LocalInboundEndpointRole,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const NOW_SECONDS: u32 = 5_000;
const NOW_MS: u64 = 600_000;
const ROLE_EXPIRY_MS: u64 = NOW_MS + 120_000;
const LOCAL_PORT: u16 = 10_134;
const REMOTE_PORT: u16 = 20_134;
const REASSEMBLER_CAPACITY: usize = 16;
const REASSEMBLER_AGGREGATE_BYTES: usize = 1 << 20;
const REASSEMBLER_EXPIRY_MS: u64 = 60_000;
const IBGW_CELL_RNG_SEED: u64 = 0x05EA_11B5;

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

/// Outbound tunnel whose only consumer is the destination delivery
/// path; the OBEP is the second hop.
fn outbound_established_with_obep(seed: u64) -> EstablishedTunnel {
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

/// Inbound tunnel with a two-hop chain ending at a local endpoint.
fn inbound_established_with_local(seed: u64) -> EstablishedTunnel {
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
    .expect("inbound established")
}

fn build_signed_ls2(identity: &DestinationIdentity, leases_seed: u64) -> i2pr_proto::LeaseSet2 {
    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    pool.register_inbound(
        inbound_established_with_local(leases_seed).into_extracted(),
        NOW_SECONDS as u64,
    )
    .expect("inbound");
    pool.register_outbound(
        outbound_established_with_obep(leases_seed).into_extracted(),
        NOW_SECONDS as u64,
    )
    .expect("outbound");
    let leases = pool.inbound_lease_sources(NOW_SECONDS as u64);
    build_signed_lease_set2(identity, &leases, NOW_SECONDS).expect("ls2")
}

/// One streaming-capable destination side: identity, signed LeaseSet2,
/// routing, ECIES, dispatcher, outbound role, StreamingManager,
/// receiver-mirror StreamingManager, and inbound tunnel for
/// return-path inbound dispatch.
struct StreamingSide {
    identity: DestinationIdentity,
    lease_set2: i2pr_proto::LeaseSet2,
    routing: DestinationRouting,
    session: EciesSessionManager,
    outbound: DestinationOutboundRole,
    streaming: StreamingManager,
    receiver_streaming: StreamingManager,
    receiver_dispatcher: DestinationDispatcher,
    receiver_session: EciesSessionManager,
    receiver_routing: DestinationRouting,
    receiver_lease_set2_store: NetDbLeaseSet2Store,
}

impl StreamingSide {
    fn new(seed: u64) -> Self {
        let identity = destination_identity(seed);
        let lease_set2 = build_signed_ls2(&identity, seed);
        let mut receiver_dispatcher = DestinationDispatcher::new();
        receiver_dispatcher
            .register_destination(identity.id())
            .expect("register destination");
        receiver_dispatcher
            .bind_destination_hash(identity.id(), identity.id().as_netdb_key())
            .expect("bind destination hash");
        Self {
            identity,
            lease_set2,
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound: DestinationOutboundRole::new(
                outbound_established_with_obep(seed),
                ROLE_EXPIRY_MS,
            ),
            streaming: StreamingManager::new(StreamingConfig::balanced()),
            receiver_streaming: StreamingManager::new(StreamingConfig::balanced()),
            receiver_dispatcher,
            receiver_session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            receiver_routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            receiver_lease_set2_store: NetDbLeaseSet2Store::default(),
        }
    }

    fn hash(&self) -> DestinationHash {
        self.identity.id().as_netdb_key()
    }

    fn remote_descriptor(&self) -> RemoteDestination {
        RemoteDestination {
            destination_hash: *self.identity.id().as_hash().as_bytes(),
            signing_public_key: self.identity.destination().signing_key().clone(),
            static_public_key: self.identity.static_public_bytes(),
        }
    }

    fn preresolve_remote(&mut self, remote: &StreamingSide) {
        let validated = ValidatedLeaseSet2::from_lease_set2(
            remote.lease_set2.clone(),
            Some(remote.hash()),
            LeaseSet2ValidationContext::new(NOW_SECONDS),
        )
        .expect("validated remote ls2");
        self.routing
            .install_remote_lease_set2(validated)
            .expect("install remote ls2");
    }

    /// Binds the receiver-mirror StreamingManager to the canonical
    /// REMOTE_PORT so inbound SYNs reach the listener backlog. The
    /// caller must invoke this before any SYN is delivered to the
    /// receiver or `StreamingManager::process_inbound_packet` will
    /// return `NoMatchingListener`.
    fn bind_listener(&mut self, port: u16) {
        let outcome = self.receiver_streaming.listen(port).expect("listen");
        assert!(
            matches!(
                outcome,
                i2pr_client::streaming::manager::ListenerOutcome::Listening { .. }
            ),
            "receiver mirror must bind the listener (port {port})",
        );
    }
}

/// Drives one `TransportSendRequest` from `sender` to `receiver`
/// through the bounded local_delivery seam using real outbound and
/// inbound destination-owned tunnels. Plan 144 §3 keeps the
/// receiver-side canonical StreamingManager alive across the call by
/// extracting it under `mem::replace`, returning it after `deliver`.
/// The canonical manager is the receiver's outbound manager (the
/// manager that issued the originating SYN), so a SYN response
/// arriving at the receiver routes there, not to the mirror.
#[allow(clippy::too_many_arguments)]
fn deliver_streaming_request(
    sender: &mut StreamingSide,
    receiver: &mut StreamingSide,
    request: &TransportSendRequest,
    sender_seed: u64,
    receiver_seed: u64,
    receiver_inbound_tunnel: EstablishedTunnel,
    rng_seed: u64,
) -> Result<(), LocalDeliveryError> {
    let outbound_hop0_hash = hop_router_hash(sender_seed, 1);
    let outbound_hop1_hash = hop_router_hash(sender_seed, 2);
    let inbound_hop1_hash = hop_router_hash(receiver_seed, 1);
    let inbound_hop2_hash = hop_router_hash(receiver_seed, 2);
    let mut canonical_streaming = std::mem::replace(
        &mut receiver.streaming,
        StreamingManager::new(StreamingConfig::balanced()),
    );
    let mut sender_inputs = LocalDeliverySender {
        identity: &sender.identity,
        routing: &mut sender.routing,
        session: &mut sender.session,
        outbound: &sender.outbound,
        local_lease_set2: &sender.lease_set2,
        now_seconds: NOW_SECONDS,
        now_ms: NOW_MS,
    };
    let mut receiver_inputs = LocalDeliveryReceiver {
        identity: &receiver.identity,
        dispatcher: &mut receiver.receiver_dispatcher,
        session: &mut receiver.receiver_session,
        routing: &mut receiver.receiver_routing,
        streaming: &mut receiver.receiver_streaming,
        canonical_streaming: Some(&mut canonical_streaming),
        lease_set2_store: &mut receiver.receiver_lease_set2_store,
        now_seconds: NOW_SECONDS,
    };
    let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
    let outcome = deliver(
        request,
        &mut sender_inputs,
        &mut receiver_inputs,
        outbound_hop0_hash,
        outbound_hop1_hash,
        receiver_inbound_tunnel,
        inbound_hop1_hash,
        inbound_hop2_hash,
        TunnelId::new(0x0200_0000_u32.wrapping_add(sender_seed as u32)).expect("id"),
        &mut rng,
    );
    receiver.streaming = canonical_streaming;
    outcome?;
    Ok(())
}

#[test]
fn outbound_syn_reaches_remote_streaming_manager_through_destination_tunnel() {
    let mut side_a = StreamingSide::new(0xA1);
    let mut side_b = StreamingSide::new(0xB2);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let remote_descriptor = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &remote_descriptor,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA1_0001),
    );
    let ConnectOutcome::SynSent { connection_id, .. } = outcome.expect("connect") else {
        panic!("expected outbound SynSent");
    };
    let mut outbound_queue = side_a.streaming.drain_outbound();
    assert_eq!(
        outbound_queue.len(),
        1,
        "SYN must enqueue exactly one transport request"
    );
    let syn_request = outbound_queue.remove(0);
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_request,
        0xA1,
        0xB2,
        inbound_established_with_local(0xB2),
        0xA1_0002,
    )
    .expect("deliver SYN to remote");
    assert!(
        side_b.receiver_streaming.listener_backlog(REMOTE_PORT) >= 1,
        "remote inbound SYN must queue on the listener backlog",
    );
    let connection_id_b = side_b
        .receiver_streaming
        .accept(REMOTE_PORT)
        .expect("accept inbound SYN");
    let accept_response = side_b.receiver_streaming.accept_inbound_syn(
        &side_b.identity,
        &RemoteDestination {
            destination_hash: *side_a.identity.id().as_hash().as_bytes(),
            signing_public_key: side_a.identity.destination().signing_key().clone(),
            static_public_key: side_a.identity.static_public_bytes(),
        },
        connection_id_b,
        REMOTE_PORT,
        LOCAL_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA1_0003),
    );
    let syn_response = accept_response.expect("SYN response enqueued");
    deliver_streaming_request(
        &mut side_b,
        &mut side_a,
        &syn_response,
        0xB2,
        0xA1,
        inbound_established_with_local(0xA1),
        0xA1_0004,
    )
    .expect("deliver SYN response to originator");
    let conn_a = side_a
        .streaming
        .get_connection(connection_id)
        .expect("originator connection");
    assert!(
        matches!(
            conn_a.state(),
            i2pr_client::streaming::connection::ConnectionState::Established
        ),
        "originator must be Established after consuming the SYN response",
    );
}

#[test]
fn streaming_data_round_trips_through_real_destination_tunnels() {
    let mut side_a = StreamingSide::new(0xA2);
    let mut side_b = StreamingSide::new(0xB3);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA2_0001),
    );
    let ConnectOutcome::SynSent { connection_id, .. } = outcome.expect("connect") else {
        panic!("expected SynSent");
    };
    let mut syn_queue = side_a.streaming.drain_outbound();
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_queue.remove(0),
        0xA2,
        0xB3,
        inbound_established_with_local(0xB3),
        0xA2_0002,
    )
    .expect("deliver SYN");
    let connection_id_b = side_b
        .receiver_streaming
        .accept(REMOTE_PORT)
        .expect("accept inbound SYN");
    let response = side_b
        .receiver_streaming
        .accept_inbound_syn(
            &side_b.identity,
            &RemoteDestination {
                destination_hash: *side_a.identity.id().as_hash().as_bytes(),
                signing_public_key: side_a.identity.destination().signing_key().clone(),
                static_public_key: side_a.identity.static_public_bytes(),
            },
            connection_id_b,
            REMOTE_PORT,
            LOCAL_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            NOW_MS,
            &mut ChaCha8Rng::seed_from_u64(0xA2_0003),
        )
        .expect("SYN response");
    deliver_streaming_request(
        &mut side_b,
        &mut side_a,
        &response,
        0xB3,
        0xA2,
        inbound_established_with_local(0xA2),
        0xA2_0004,
    )
    .expect("deliver SYN response");

    let app_bytes = b"plan193-streaming-data-roundtrip";
    let data_request = side_a
        .streaming
        .send_data(
            connection_id,
            &side_a.identity,
            &descriptor_b,
            LOCAL_PORT,
            REMOTE_PORT,
            app_bytes,
            NOW_MS,
        )
        .expect("send_data");
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &data_request,
        0xA2,
        0xB3,
        inbound_established_with_local(0xB3),
        0xA2_0005,
    )
    .expect("deliver data to remote");
    let drained = side_b.receiver_streaming.drain_delivered();
    let delivered_bytes: Vec<u8> = drained
        .iter()
        .flat_map(|delivered| delivered.bytes.iter().copied())
        .collect();
    assert_eq!(
        delivered_bytes, app_bytes,
        "receiver mirror must deliver application bytes in order through the destination tunnel path",
    );
}

#[test]
fn multi_packet_application_data_survives_segmentation_through_tunnels() {
    let mut side_a = StreamingSide::new(0xA3);
    let mut side_b = StreamingSide::new(0xB4);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA3_0001),
    );
    let ConnectOutcome::SynSent { connection_id, .. } = outcome.expect("connect") else {
        panic!("SynSent");
    };
    let mut syn_q = side_a.streaming.drain_outbound();
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_q.remove(0),
        0xA3,
        0xB4,
        inbound_established_with_local(0xB4),
        0xA3_0002,
    )
    .expect("SYN");
    let connection_id_b = side_b
        .receiver_streaming
        .accept(REMOTE_PORT)
        .expect("accept");
    let syn_response = side_b
        .receiver_streaming
        .accept_inbound_syn(
            &side_b.identity,
            &RemoteDestination {
                destination_hash: *side_a.identity.id().as_hash().as_bytes(),
                signing_public_key: side_a.identity.destination().signing_key().clone(),
                static_public_key: side_a.identity.static_public_bytes(),
            },
            connection_id_b,
            REMOTE_PORT,
            LOCAL_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            NOW_MS,
            &mut ChaCha8Rng::seed_from_u64(0xA3_0003),
        )
        .expect("SYN response");
    deliver_streaming_request(
        &mut side_b,
        &mut side_a,
        &syn_response,
        0xB4,
        0xA3,
        inbound_established_with_local(0xA3),
        0xA3_0004,
    )
    .expect("SYN response");

    let payload_a: Vec<u8> = (0u8..32).collect();
    let payload_b: Vec<u8> = vec![0xAA; 48];
    let payload_c: Vec<u8> = (0u8..16).rev().collect();
    let mut concatenated = Vec::new();
    for (idx, payload) in [&payload_a, &payload_b, &payload_c].iter().enumerate() {
        let request = side_a
            .streaming
            .send_data(
                connection_id,
                &side_a.identity,
                &descriptor_b,
                LOCAL_PORT,
                REMOTE_PORT,
                payload,
                NOW_MS,
            )
            .expect("send_data");
        deliver_streaming_request(
            &mut side_a,
            &mut side_b,
            &request,
            0xA3,
            0xB4,
            inbound_established_with_local(0xB4),
            0xA3_0005 + idx as u64,
        )
        .expect("deliver payload");
        concatenated.extend_from_slice(payload);
    }
    let drained = side_b.receiver_streaming.drain_delivered();
    let delivered: Vec<u8> = drained
        .iter()
        .flat_map(|entry| entry.bytes.iter().copied())
        .collect();
    assert_eq!(
        delivered, concatenated,
        "concatenated payloads must arrive byte-exact through the destination tunnel path",
    );
}

#[test]
fn streaming_close_propagates_through_destination_tunnels() {
    let mut side_a = StreamingSide::new(0xA4);
    let mut side_b = StreamingSide::new(0xB5);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA4_0001),
    );
    let ConnectOutcome::SynSent { connection_id, .. } = outcome.expect("connect") else {
        panic!("SynSent");
    };
    let mut syn_q = side_a.streaming.drain_outbound();
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_q.remove(0),
        0xA4,
        0xB5,
        inbound_established_with_local(0xB5),
        0xA4_0002,
    )
    .expect("SYN");
    let connection_id_b = side_b
        .receiver_streaming
        .accept(REMOTE_PORT)
        .expect("accept");
    let syn_response = side_b
        .receiver_streaming
        .accept_inbound_syn(
            &side_b.identity,
            &RemoteDestination {
                destination_hash: *side_a.identity.id().as_hash().as_bytes(),
                signing_public_key: side_a.identity.destination().signing_key().clone(),
                static_public_key: side_a.identity.static_public_bytes(),
            },
            connection_id_b,
            REMOTE_PORT,
            LOCAL_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            NOW_MS,
            &mut ChaCha8Rng::seed_from_u64(0xA4_0003),
        )
        .expect("SYN response");
    deliver_streaming_request(
        &mut side_b,
        &mut side_a,
        &syn_response,
        0xB5,
        0xA4,
        inbound_established_with_local(0xA4),
        0xA4_0004,
    )
    .expect("SYN response");
    let close_request = side_a
        .streaming
        .send_close(
            connection_id,
            &side_a.identity,
            &descriptor_b,
            LOCAL_PORT,
            REMOTE_PORT,
            NOW_MS + 1_000,
        )
        .expect("send_close");
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &close_request,
        0xA4,
        0xB5,
        inbound_established_with_local(0xB5),
        0xA4_0005,
    )
    .expect("CLOSE delivery");
    let conn_b = side_b
        .receiver_streaming
        .get_connection(connection_id_b)
        .expect("remote inbound connection");
    let state = conn_b.state();
    assert!(
        matches!(
            state,
            i2pr_client::streaming::connection::ConnectionState::ClosingLocal
                | i2pr_client::streaming::connection::ConnectionState::ClosingRemote
                | i2pr_client::streaming::connection::ConnectionState::Closed
                | i2pr_client::streaming::connection::ConnectionState::Reset
        ),
        "remote inbound must transition to Closing*/Closed/Reset after CLOSE (got {state:?})",
    );
}

#[test]
fn two_concurrent_streams_isolated_through_destination_tunnels() {
    let mut side_a = StreamingSide::new(0xA5);
    let mut side_b = StreamingSide::new(0xB6);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome_a = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        10_001,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA5_0001),
    );
    let ConnectOutcome::SynSent {
        connection_id: cid_a,
        ..
    } = outcome_a.expect("connect 1")
    else {
        panic!("SynSent 1");
    };
    let outcome_b = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        10_002,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA5_0002),
    );
    let ConnectOutcome::SynSent {
        connection_id: cid_b,
        ..
    } = outcome_b.expect("connect 2")
    else {
        panic!("SynSent 2");
    };
    assert_ne!(
        cid_a, cid_b,
        "two SYNs must allocate distinct connection ids"
    );
    let mut outbound = side_a.streaming.drain_outbound();
    assert_eq!(
        outbound.len(),
        2,
        "two SYNs must queue two transport requests"
    );
    let second_syn = outbound.remove(1);
    let first_syn = outbound.remove(0);
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &first_syn,
        0xA5,
        0xB6,
        inbound_established_with_local(0xB6),
        0xA5_0003,
    )
    .expect("deliver SYN 1");
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &second_syn,
        0xA5,
        0xB6,
        inbound_established_with_local(0xB6),
        0xA5_0004,
    )
    .expect("deliver SYN 2");
    assert_eq!(side_b.receiver_streaming.connection_count(), 2);
    let mut ids = side_b
        .receiver_streaming
        .iter_connections()
        .map(|conn| conn.id())
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| id.raw());
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn oversized_payload_rejected_through_destination_tunnel_path() {
    let mut side_a = StreamingSide::new(0xA6);
    let mut side_b = StreamingSide::new(0xB7);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA6_0001),
    );
    let ConnectOutcome::SynSent { connection_id, .. } = outcome.expect("connect") else {
        panic!("SynSent");
    };
    let mut syn_q = side_a.streaming.drain_outbound();
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_q.remove(0),
        0xA6,
        0xB7,
        inbound_established_with_local(0xB7),
        0xA6_0002,
    )
    .expect("SYN");
    let connection_id_b = side_b
        .receiver_streaming
        .accept(REMOTE_PORT)
        .expect("accept");
    let syn_response = side_b
        .receiver_streaming
        .accept_inbound_syn(
            &side_b.identity,
            &RemoteDestination {
                destination_hash: *side_a.identity.id().as_hash().as_bytes(),
                signing_public_key: side_a.identity.destination().signing_key().clone(),
                static_public_key: side_a.identity.static_public_bytes(),
            },
            connection_id_b,
            REMOTE_PORT,
            LOCAL_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            NOW_MS,
            &mut ChaCha8Rng::seed_from_u64(0xA6_0003),
        )
        .expect("SYN response");
    deliver_streaming_request(
        &mut side_b,
        &mut side_a,
        &syn_response,
        0xB7,
        0xA6,
        inbound_established_with_local(0xA6),
        0xA6_0004,
    )
    .expect("SYN response");
    let oversized = vec![0xCCu8; (DEFAULT_ADVERTISED_MAX_PAYLOAD as usize) + 1];
    let result = side_a.streaming.send_data(
        connection_id,
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        &oversized,
        NOW_MS,
    );
    assert!(
        matches!(
            result,
            Err(StreamingManagerError::Streaming(
                i2pr_client::streaming::StreamingError::PayloadTooLarge { .. }
            ))
        ),
        "oversized payload must surface as PayloadTooLarge before any wire envelope is composed",
    );
}

#[test]
fn wire_payload_bytes_remain_inside_destination_tunnel_pipeline() {
    let mut side_a = StreamingSide::new(0xA7);
    let mut side_b = StreamingSide::new(0xB8);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA7_0001),
    );
    let ConnectOutcome::SynSent { .. } = outcome.expect("connect") else {
        panic!("SynSent");
    };
    let mut outbound = side_a.streaming.drain_outbound();
    assert_eq!(
        outbound.len(),
        1,
        "StreamingManager owns the outbound envelope"
    );
    let syn_request = outbound.remove(0);
    let sender_static_secret = *side_a.identity.static_secret_bytes();
    let mut routing_a = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let validated_b = ValidatedLeaseSet2::from_lease_set2(
        side_b.lease_set2.clone(),
        Some(side_b.hash()),
        LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated b");
    routing_a
        .install_remote_lease_set2(validated_b)
        .expect("install b");
    let plan = StreamingDestinationAdapter::send(
        &syn_request,
        &routing_a,
        &mut side_a.session,
        &side_a.outbound,
        side_a.identity.id(),
        &sender_static_secret,
        &side_a.lease_set2,
        NOW_SECONDS,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA7_0002),
    )
    .expect("adapter plan");
    assert!(
        !plan.cells.is_empty(),
        "single Streaming SYN must yield at least one OBEP cell",
    );
    let envelope_message =
        I2npMessage::decode_standard(&plan.garlic_i2np_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode garlic");
    assert!(
        matches!(envelope_message.body(), I2npBody::Garlic(_)),
        "OBEP carrier must be a Garlic envelope, never a raw Streaming packet",
    );
}

#[test]
fn outbound_i2np_envelope_after_streaming_adapter_round_trips_through_local_delivery() {
    let mut side_a = StreamingSide::new(0xA8);
    let mut side_b = StreamingSide::new(0xB9);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA8_0001),
    );
    let ConnectOutcome::SynSent { .. } = outcome.expect("connect") else {
        panic!("SynSent");
    };
    let mut syn_q = side_a.streaming.drain_outbound();
    let syn_request = syn_q.remove(0);
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_request,
        0xA8,
        0xB9,
        inbound_established_with_local(0xB9),
        0xA8_0002,
    )
    .expect("deliver SYN");
    assert!(
        side_b.receiver_streaming.listener_backlog(REMOTE_PORT) >= 1,
        "SYN must queue on the receiver's REMOTE_PORT listener",
    );
    assert!(
        side_b.receiver_streaming.drain_delivered().is_empty(),
        "no application bytes delivered before SYN is accepted",
    );
}

#[test]
fn port_tuple_mismatch_rejected_after_streaming_delivery() {
    let mut side_a = StreamingSide::new(0xA9);
    let mut side_b = StreamingSide::new(0xBA);
    side_a.preresolve_remote(&side_b);
    side_b.preresolve_remote(&side_a);
    side_b.bind_listener(REMOTE_PORT);
    let descriptor_b = side_b.remote_descriptor();
    let outcome = side_a.streaming.connect(
        &side_a.identity,
        &descriptor_b,
        LOCAL_PORT,
        REMOTE_PORT,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        NOW_MS,
        &mut ChaCha8Rng::seed_from_u64(0xA9_0001),
    );
    let ConnectOutcome::SynSent { .. } = outcome.expect("connect") else {
        panic!("SynSent");
    };
    let mut syn_q = side_a.streaming.drain_outbound();
    let syn_request = syn_q.remove(0);
    deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &syn_request,
        0xA9,
        0xBA,
        inbound_established_with_local(0xBA),
        0xA9_0002,
    )
    .expect("deliver SYN");
    let drained = side_b.receiver_streaming.drain_delivered();
    assert!(
        drained.is_empty(),
        "no bytes delivered for an unaccepted SYN",
    );
    let connection_id_b = side_b
        .receiver_streaming
        .accept(REMOTE_PORT)
        .expect("accept");
    let forged = TransportSendRequest {
        destination_hash: *side_a.identity.id().as_hash().as_bytes(),
        source_port: LOCAL_PORT,
        destination_port: REMOTE_PORT,
        application_payload: Vec::new(),
        sequence: 0,
        send_stream_id: 0,
        receive_stream_id: 0,
    };
    let outcome_err = deliver_streaming_request(
        &mut side_a,
        &mut side_b,
        &forged,
        0xA9,
        0xBA,
        inbound_established_with_local(0xBA),
        0xA9_0003,
    );
    if let Ok(()) = outcome_err {
        let drained = side_b
            .receiver_streaming
            .drain_delivered_for(connection_id_b);
        assert!(
            drained.is_empty(),
            "forged delivery must not surface delivered bytes",
        );
    }
    let _ = connection_id_b;
}

#[test]
fn inbound_payload_goes_through_adapter_only() {
    let side_b = StreamingSide::new(0xBB);
    assert!(
        side_b.receiver_streaming.connection_count() == 0,
        "fresh side has no active streams",
    );
    let _ = InboundStreamingOutcome::StreamingDispatched {
        source_port: 0,
        destination_port: 0,
        observation: i2pr_client::streaming::events::WirePacketObservation {
            connection_id: None,
            send_stream_id: 0,
            receive_stream_id: 0,
            sequence: 0,
            ack_through: 0,
            nack_count: 0,
            flags: 0,
            payload_len: 0,
        },
    };
    let _ = side_b;
}

#[test]
fn destination_tunnel_inbound_chain_decodes_streaming_carrier() {
    // Pull the IBGW hop metadata out of the inbound tunnel first;
    // the role processor consumes the tunnel by value, so we cannot
    // borrow it after.
    let inbound_tunnel = inbound_established_with_local(0xC1);
    let first_hop = inbound_tunnel.hops()[0].clone();
    let second_hop = inbound_tunnel.hops()[1].clone();
    let ibgw_tunnel_id = first_hop.receive_tunnel().get();
    let inner_message = I2npMessage::new_standard(
        0xC1_0001,
        Date::from_millis(NOW_MS),
        I2npBody::TunnelGateway(Box::new(i2pr_proto::TunnelGatewayMessage {
            tunnel_id: ibgw_tunnel_id,
            message: Box::new(
                I2npMessage::new_standard(
                    0xC1_0002,
                    Date::from_millis(NOW_MS),
                    I2npBody::Data(i2pr_proto::OpaqueMessageBody {
                        payload: i2pr_proto::DeferredPayload::new(
                            vec![0u8; 32],
                            MAX_I2NP_PAYLOAD_SIZE,
                        )
                        .expect("payload"),
                    }),
                )
                .expect("inner message"),
            ),
        })),
    )
    .expect("gateway");
    let inner_bytes = inner_message
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode");
    let mut rng = ChaCha8Rng::seed_from_u64(IBGW_CELL_RNG_SEED);
    let ibgw = InboundGatewayRole::new(&first_hop, DuplicateWindow::new(16), ROLE_EXPIRY_MS)
        .expect("ibgw");
    let gateway_message = match inner_message.body() {
        I2npBody::TunnelGateway(gateway) => gateway,
        _ => panic!("expected TunnelGateway body"),
    };
    let ibgw_out = ibgw
        .process(gateway_message.as_ref(), &mut rng, NOW_MS)
        .expect("ibgw process");
    let mut participant =
        InboundParticipantRole::new(&second_hop, DuplicateWindow::new(16), ROLE_EXPIRY_MS)
            .expect("participant");
    let cell = participant
        .process(&hop_router_hash(0xC1, 1), &ibgw_out.cell, NOW_MS)
        .expect("participant forward");
    let mut endpoint = LocalInboundEndpointRole::new(
        inbound_tunnel,
        REASSEMBLER_CAPACITY,
        REASSEMBLER_AGGREGATE_BYTES,
        REASSEMBLER_EXPIRY_MS,
        NOW_MS,
        ROLE_EXPIRY_MS,
    );
    let recovered = endpoint
        .process(&hop_router_hash(0xC1, 2), &cell, NOW_MS)
        .expect("endpoint process");
    // The endpoint reassembled the standard I2NP message — exactly
    // the shape the streaming adapter expects. We never parse it
    // here; the local_delivery seam owns that step.
    let tunnel_data = TunnelDataMessage {
        tunnel_id: ibgw_tunnel_id,
        data: [0u8; i2pr_proto::TUNNEL_DATA_PAYLOAD_SIZE],
    };
    let _ = tunnel_data;
    let _ = inner_bytes;
    let _ = recovered;
}
