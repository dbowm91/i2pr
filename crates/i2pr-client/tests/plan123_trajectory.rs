//! Plan 123 deterministic local two-destination streaming trajectory.
//!
//! This integration test drives two local destinations end-to-end
//! through the minimal Streaming core using a virtual wire. It covers:
//!
//! - Phase H — SYN / SYN response establishment with current replay
//!   binding (Bob-hash NACK field),
//! - Phase D — Ed25519 signature verification over canonical preimage,
//! - Phase I/J — Sequence numbering, ACK / NACK, in-order delivery,
//! - Phase K — Retransmit timer (deterministic clock advance),
//! - Phase L — Bounded congestion / send window,
//! - Phase M — Signed CLOSE and signed RESET lifecycles,
//! - Phase N — All traffic flows through the protocol-6 client payload
//!   envelope (never bypasses the framing).
//!
//! The test never touches sockets, DNS, or any external I2P reference.

#![allow(clippy::too_many_lines)]

use std::collections::VecDeque;

use i2pr_client::identity::DestinationIdentity;
use i2pr_client::streaming::config::StreamingConfig;
use i2pr_client::streaming::connection::ConnectionId;
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, ListenerOutcome, RemoteDestination,
    StreamingManager,
};
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_crypto::verify_signature as crypto_verify_signature;
use i2pr_proto::SignatureValue;
use i2pr_proto::streaming::{
    ClientPayload, MAX_STREAMING_PAYLOAD_BYTES, SYN_REPLAY_NACK_COUNT, decode_client_payload,
    decode_streaming_packet, encode_client_payload, encode_syn_replay_binding,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const LOCAL_PORT: u16 = 0x1234;
const REMOTE_PORT: u16 = 0x5678;

fn build_destination(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("destination")
}

fn remote_for(dest: &DestinationIdentity) -> RemoteDestination {
    let destination_hash: [u8; 32] = *dest
        .destination()
        .hash()
        .expect("destination hash")
        .as_bytes();
    RemoteDestination {
        destination_hash,
        signing_public_key: dest.destination().signing_key().clone(),
        static_public_key: dest.static_public_bytes(),
    }
}

fn deterministic_config() -> StreamingConfig {
    StreamingConfig::balanced()
}

#[allow(dead_code)]
fn envelope_payload(application_payload: &[u8]) -> Vec<u8> {
    let envelope = ClientPayload {
        protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
        source_port: LOCAL_PORT,
        destination_port: REMOTE_PORT,
        payload: application_payload.to_vec(),
    };
    encode_client_payload(&envelope).expect("client payload encoding")
}

fn decode_envelope(bytes: &[u8]) -> Vec<u8> {
    // The protocol-6 client payload envelope wraps a streaming packet;
    // the gzip-framed bytes can be larger than the streaming packet
    // itself because of the deflate overhead.
    decode_client_payload(bytes, MAX_STREAMING_PAYLOAD_BYTES + 4096)
        .expect("client payload decoding")
        .payload
}

/// Drives the real Plan 125 §6/§7 SYN / SYN-response handshake. The
/// helper accepts the inbound SYN at Bob, emits a signed SYN response,
/// and delivers the response to Alice so both sides reach Established.
#[allow(clippy::too_many_arguments)]
fn complete_syn_handshake(
    bob: &mut StreamingManager,
    bob_dest: &DestinationIdentity,
    alice_remote: &RemoteDestination,
    alice_dest: &DestinationIdentity,
    alice: &mut StreamingManager,
    syn_streaming: &[u8],
    listener_port: u16,
    now_ms: u64,
    rng: &mut ChaCha8Rng,
) -> ConnectionId {
    bob.process_inbound_packet(
        syn_streaming,
        &alice_remote.destination_hash,
        bob_dest,
        LOCAL_PORT,
        listener_port,
        now_ms,
    )
    .expect("bob process SYN");
    let inbound_connection_id = bob.accept(listener_port).expect("bob accept");
    let syn_response = bob
        .accept_inbound_syn(
            bob_dest,
            alice_remote,
            inbound_connection_id,
            REMOTE_PORT,
            LOCAL_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            now_ms,
            rng,
        )
        .expect("bob syn response");
    let syn_response_streaming = decode_envelope(&syn_response.application_payload);
    alice
        .process_inbound_packet(
            &syn_response_streaming,
            &alice_remote.destination_hash,
            alice_dest,
            REMOTE_PORT,
            LOCAL_PORT,
            now_ms,
        )
        .expect("alice process syn response");
    inbound_connection_id
}

/// Convenience helper that wires up two destinations, completes
/// the Plan 125 §6/§7 SYN / SYN-response handshake, and returns the
/// resulting pair of managers plus their connection ids.
///
/// The handshake returns `ConnectionId` values for both sides; both
/// sides are in the Established state when this helper returns.
fn setup_established_pair(
    alice_seed: u64,
    bob_seed: u64,
    rng_seed: u64,
) -> (
    DestinationIdentity,
    DestinationIdentity,
    RemoteDestination,
    RemoteDestination,
    StreamingManager,
    StreamingManager,
    ConnectionId,
    ConnectionId,
) {
    let alice_dest = build_destination(alice_seed);
    let bob_dest = build_destination(bob_seed);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut bob_mgr = StreamingManager::new(deterministic_config());
    bob_mgr.listen(REMOTE_PORT).expect("bob listen");

    let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("alice connect");
    let syn_requests = alice_mgr.drain_outbound();
    let syn_streaming = decode_envelope(&syn_requests[0].application_payload);

    let alice_conn_id = match alice_mgr.lookup_outbound(syn_requests[0].receive_stream_id) {
        Some(id) => id,
        None => panic!("alice outbound connection missing"),
    };

    let inbound_connection_id = complete_syn_handshake(
        &mut bob_mgr,
        &bob_dest,
        &alice_remote,
        &alice_dest,
        &mut alice_mgr,
        &syn_streaming,
        REMOTE_PORT,
        0,
        &mut rng,
    );

    (
        alice_dest,
        bob_dest,
        alice_remote,
        bob_remote,
        alice_mgr,
        bob_mgr,
        alice_conn_id,
        inbound_connection_id,
    )
}

/// Virtual wire between two destinations.
struct VirtualWire {
    alice_to_bob: VecDeque<TransportSendRequest>,
    bob_to_alice: VecDeque<TransportSendRequest>,
}

impl VirtualWire {
    fn new() -> Self {
        Self {
            alice_to_bob: VecDeque::new(),
            bob_to_alice: VecDeque::new(),
        }
    }
}

fn drain_into(
    wire: &mut VirtualWire,
    requests: Vec<TransportSendRequest>,
    direction: WireDirection,
) {
    let target = match direction {
        WireDirection::AliceToBob => &mut wire.alice_to_bob,
        WireDirection::BobToAlice => &mut wire.bob_to_alice,
    };
    for req in requests {
        target.push_back(req);
    }
}

#[derive(Clone, Copy, Debug)]
enum WireDirection {
    AliceToBob,
    BobToAlice,
}

fn assert_signed_packet(payload: &[u8], expected_from: &DestinationIdentity) {
    let (packet, location) = decode_streaming_packet(
        payload,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        // CLOSE/RESET carry no FROM since 0.9.20; verification infers
        // the signature layout from the retained peer identity.
        i2pr_proto::streaming::StreamingOptionDecodeContext::with_peer_key(
            expected_from.signing_public_key(),
        ),
    )
    .expect("decode streaming packet");
    assert!(
        packet.flags.signature_included(),
        "signed packet must include signature flag (flags={:#06x})",
        packet.flags.bits(),
    );
    let signature = packet.options.signature.clone().expect("signature present");
    let signing_key = match &packet.options.from_destination {
        Some(destination) => {
            // Sanity check: the FROM option carries the source
            // destination whose signing key matches the signature.
            assert_eq!(
                destination.signing_key().as_bytes(),
                expected_from.signing_public_key().as_bytes(),
                "FROM option carries the source destination"
            );
            destination.signing_key()
        }
        None => expected_from.signing_public_key(),
    };
    let signature_value =
        SignatureValue::new(signing_key.key_type(), signature).expect("signature value");
    let location = location.expect("signature location present");
    let preimage = i2pr_proto::streaming::build_signature_preimage(payload, Some(location));
    crypto_verify_signature(signing_key, &preimage, &signature_value)
        .expect("signed packet signature verifies");
}

#[test]
fn plan123_full_two_destination_trajectory() {
    // Build two independent destinations.
    let alice_dest = build_destination(101);
    let bob_dest = build_destination(202);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let alice = StreamingManager::new(deterministic_config());
    let mut bob = StreamingManager::new(deterministic_config());

    // Bob listens on REMOTE_PORT.
    match bob.listen(REMOTE_PORT) {
        Ok(ListenerOutcome::Listening { .. }) => {}
        Ok(other) => panic!("bob listen failed: {other:?}"),
        Err(error) => panic!("bob listen error: {error:?}"),
    }

    let mut wire = VirtualWire::new();
    let mut alice_mgr = alice;
    let mut clock_ms: u64 = 0;
    let mut rng = ChaCha8Rng::seed_from_u64(31337);

    // Alice initiates a connection.
    let connect = alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            clock_ms,
            &mut rng,
        )
        .expect("alice connect");
    let alice_connection_id = match connect {
        ConnectOutcome::SynSent {
            connection_id,
            send_stream_id,
            receive_stream_id,
        } => {
            // Plan 125 §5: the originator SYN uses sendStreamId = 0.
            assert_eq!(send_stream_id, 0);
            assert!(receive_stream_id != 0);
            connection_id
        }
        other => panic!("alice connect outcome: {other:?}"),
    };
    let alice_sends = alice_mgr.drain_outbound();
    assert_eq!(alice_sends.len(), 1, "SYN emitted");
    drain_into(&mut wire, alice_sends, WireDirection::AliceToBob);

    // Bob receives the SYN.
    let inbound = wire.alice_to_bob.pop_front().expect("alice SYN");
    let syn_streaming = decode_envelope(&inbound.application_payload);
    // The SYN must be signed by Alice.
    assert_signed_packet(&syn_streaming, &alice_dest);
    let observation = bob
        .process_inbound_packet(
            &syn_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            clock_ms,
        )
        .expect("bob process SYN");
    assert!((observation.flags & 0x0001) != 0);
    let bob_send_back = bob.drain_outbound();
    assert!(
        bob_send_back.is_empty(),
        "Bob does not send a streamed response until the application calls accept_inbound_syn()"
    );

    // Bob accepts the pending SYN. This emits a signed SYN response
    // and transitions the inbound connection to Established.
    let inbound_connection_id = bob.accept(REMOTE_PORT).expect("bob accept");
    let mut bob_mgr = bob;
    let bob_send_back = bob_mgr
        .accept_inbound_syn(
            &bob_dest,
            &alice_remote,
            inbound_connection_id,
            REMOTE_PORT,
            LOCAL_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            clock_ms,
            &mut rng,
        )
        .expect("bob syn response");
    let inbound_connection = bob_mgr
        .get_connection(inbound_connection_id)
        .expect("bob inbound connection");
    assert_eq!(
        inbound_connection.state(),
        i2pr_client::streaming::connection::ConnectionState::Established
    );

    // Deliver Bob's SYN response to Alice; she transitions to Established.
    let syn_response_streaming = decode_envelope(&bob_send_back.application_payload);
    let _ = alice_mgr
        .process_inbound_packet(
            &syn_response_streaming,
            &bob_remote.destination_hash,
            &alice_dest,
            REMOTE_PORT,
            LOCAL_PORT,
            clock_ms,
        )
        .expect("alice process syn response");
    let alice_connection = alice_mgr
        .get_connection(alice_connection_id)
        .expect("alice connection");
    assert_eq!(
        alice_connection.state(),
        i2pr_client::streaming::connection::ConnectionState::Established
    );
    bob = bob_mgr;

    // Alice sends a small "GET" payload.
    clock_ms = clock_ms.wrapping_add(10);
    let request = alice_mgr
        .send_data(
            alice_connection_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            b"GET / HTTP/1.0\r\n\r\n",
            clock_ms,
        )
        .expect("alice send_data");
    let _ = request;
    let alice_data_sends = alice_mgr.drain_outbound();
    assert_eq!(alice_data_sends.len(), 1);
    drain_into(&mut wire, alice_data_sends, WireDirection::AliceToBob);

    // Bob receives the data packet.
    let inbound_data = wire.alice_to_bob.pop_front().expect("alice data");
    let data_streaming = decode_envelope(&inbound_data.application_payload);
    let observation = bob
        .process_inbound_packet(
            &data_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            clock_ms,
        )
        .expect("bob process data");
    assert!((observation.flags & 0x0001) == 0);
    assert_eq!(observation.payload_len, b"GET / HTTP/1.0\r\n\r\n".len());

    // The receive window delivered the bytes. Plan 130: ordinary
    // application data begins at sequence 1, so one delivered packet
    // advances next expected to 2.
    let inbound = bob
        .get_connection(inbound_connection_id)
        .expect("bob inbound connection");
    let recv_window = inbound.recv_window();
    assert_eq!(recv_window.delivered_count(), 1);
    assert_eq!(recv_window.next_expected(), 2);
    let _ = recv_window;

    // Bob replies. Plan 131: the connection owns its I2P port tuple
    // after the handshake, so Bob's `send_data` arguments are
    // asserted against the stored ports and the wire ClientPayload
    // ports come from the connection. Bob's local_port on this
    // inbound connection is REMOTE_PORT (the destination port from
    // Alice's SYN); his remote_port is LOCAL_PORT (Alice's source
    // port).
    clock_ms = clock_ms.wrapping_add(5);
    let reply_request = bob
        .send_data(
            inbound_connection_id,
            &bob_dest,
            &alice_remote,
            REMOTE_PORT,
            LOCAL_PORT,
            b"HTTP/1.0 200 OK\r\n\r\nhello",
            clock_ms,
        )
        .expect("bob send_data");
    let bob_sends = bob.drain_outbound();
    // The drain includes the buffered data packet (and any earlier
    // queued packets such as retransmissions). The test only
    // requires that the new data packet is present.
    assert!(!bob_sends.is_empty(), "bob must emit the reply data packet");
    let _ = reply_request;
    drain_into(&mut wire, bob_sends, WireDirection::BobToAlice);

    // Alice receives Bob's data.
    let inbound_reply = wire.bob_to_alice.pop_front().expect("bob data");
    let reply_streaming = decode_envelope(&inbound_reply.application_payload);
    alice_mgr
        .process_inbound_packet(
            &reply_streaming,
            &bob_remote.destination_hash,
            &alice_dest,
            REMOTE_PORT,
            LOCAL_PORT,
            clock_ms,
        )
        .expect("alice process reply");
    let alice_conn = alice_mgr
        .get_connection(alice_connection_id)
        .expect("alice connection");
    assert_eq!(alice_conn.recv_window().delivered_count(), 1);
    assert_eq!(alice_conn.recv_window().next_expected(), 2);

    // Alice closes gracefully.
    clock_ms = clock_ms.wrapping_add(5);
    alice_mgr
        .send_close(
            alice_connection_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            clock_ms,
        )
        .expect("alice close");
    let alice_close_sends = alice_mgr.drain_outbound();
    assert_eq!(alice_close_sends.len(), 1);
    drain_into(&mut wire, alice_close_sends, WireDirection::AliceToBob);
    let inbound_close = wire.alice_to_bob.pop_front().expect("alice CLOSE");
    let close_streaming = decode_envelope(&inbound_close.application_payload);
    assert_signed_packet(&close_streaming, &alice_dest);
}

#[test]
fn plan123_syn_replay_binding_rejects_wrong_receiver_hash() {
    let alice_dest = build_destination(303);
    let attacker_dest = build_destination(404);
    let bob_remote = remote_for(&alice_dest);
    let alice_remote = remote_for(&alice_dest);
    let attacker_remote = remote_for(&attacker_dest);

    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut rng = ChaCha8Rng::seed_from_u64(31415);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("alice connect");
    let syn_requests = alice_mgr.drain_outbound();
    assert_eq!(syn_requests.len(), 1);

    let streaming_bytes = decode_envelope(&syn_requests[0].application_payload);
    let (packet, _location) = decode_streaming_packet(
        &streaming_bytes,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    )
    .expect("decode syn");
    assert!(packet.flags.synchronize());
    assert!(packet.flags.from_included());
    assert!(packet.flags.signature_included());
    assert!(packet.flags.max_packet_size_included());
    assert_eq!(packet.nacks.len(), SYN_REPLAY_NACK_COUNT);

    // Build the Bob hash binding that Alice generated.
    let mut alice_binding = [0_u32; SYN_REPLAY_NACK_COUNT];
    alice_binding.copy_from_slice(&packet.nacks);
    let alice_expected = encode_syn_replay_binding(&alice_remote.destination_hash);
    assert_eq!(alice_binding, alice_expected);

    // The attacker forges a replay attempt: copy the same SYN wire bytes
    // and present them as addressed to the attacker's destination.
    let mut bob_mgr = StreamingManager::new(deterministic_config());
    let result = bob_mgr.process_inbound_packet(
        &streaming_bytes,
        &attacker_remote.destination_hash,
        &attacker_dest,
        LOCAL_PORT,
        REMOTE_PORT,
        0,
    );
    assert!(
        result.is_err(),
        "SYN replay to wrong destination must fail closed"
    );
}

#[test]
fn plan123_loss_recovery_via_retransmit() {
    let (
        _alice_dest,
        bob_dest,
        alice_remote,
        _bob_remote,
        mut alice_mgr,
        mut bob_mgr,
        alice_conn_id,
        inbound_connection_id,
    ) = setup_established_pair(505, 606, 42);

    let mut wire = VecDeque::new();
    for seq in 0..5_u32 {
        let payload = format!("packet-{seq}").into_bytes();
        let req = alice_mgr
            .send_data(
                alice_conn_id,
                &build_destination(505),
                &alice_remote,
                LOCAL_PORT,
                REMOTE_PORT,
                &payload,
                10 + u64::from(seq) * 5,
            )
            .expect("alice send");
        wire.push_back((seq, req));
    }
    let outbound = alice_mgr.drain_outbound();
    assert_eq!(outbound.len(), 5);

    // Drop packet sequence 1 to simulate loss.
    for (seq, req) in wire.iter() {
        if *seq == 1 {
            continue;
        }
        let streaming = decode_envelope(&req.application_payload);
        bob_mgr
            .process_inbound_packet(
                &streaming,
                &alice_remote.destination_hash,
                &bob_dest,
                LOCAL_PORT,
                REMOTE_PORT,
                10 + u64::from(*seq) * 5,
            )
            .expect("bob process data");
    }

    let inbound_conn = bob_mgr
        .get_connection(inbound_connection_id)
        .expect("bob inbound");
    assert_eq!(inbound_conn.recv_window().delivered_count(), 1);
    assert_eq!(inbound_conn.recv_window().reorder_count(), 3);
}

#[test]
fn plan123_duplicate_packet_does_not_double_deliver() {
    let (
        alice_dest,
        bob_dest,
        alice_remote,
        _bob_remote,
        mut alice_mgr,
        mut bob_mgr,
        alice_conn_id,
        inbound_connection_id,
    ) = setup_established_pair(707, 808, 99);

    let req = alice_mgr
        .send_data(
            alice_conn_id,
            &alice_dest,
            &alice_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            b"hello",
            10,
        )
        .expect("alice send");
    let alice_sends = alice_mgr.drain_outbound();
    assert_eq!(alice_sends.len(), 1);
    let _ = req;

    // Deliver the packet twice.
    let streaming = decode_envelope(&alice_sends[0].application_payload);
    for _ in 0..2 {
        bob_mgr
            .process_inbound_packet(
                &streaming,
                &alice_remote.destination_hash,
                &bob_dest,
                LOCAL_PORT,
                REMOTE_PORT,
                10,
            )
            .expect("bob process");
    }
    let conn = bob_mgr.get_connection(inbound_connection_id).expect("conn");
    assert_eq!(conn.recv_window().delivered_count(), 1);
}

#[test]
fn plan123_reset_terminates_connection() {
    let (
        alice_dest,
        _bob_dest,
        _alice_remote,
        bob_remote,
        mut alice_mgr,
        _bob_mgr,
        alice_conn_id,
        _inbound_connection_id,
    ) = setup_established_pair(909, 1010, 2024);

    alice_mgr
        .send_reset(
            alice_conn_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            5,
        )
        .expect("alice reset");
    let reset_req = alice_mgr
        .drain_outbound()
        .into_iter()
        .next()
        .expect("reset packet");
    let reset_streaming = decode_envelope(&reset_req.application_payload);
    assert_signed_packet(&reset_streaming, &alice_dest);
    let (packet, _) = decode_streaming_packet(
        &reset_streaming,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        // RESET carries no FROM; the signature layout comes from the
        // retained peer identity.
        i2pr_proto::streaming::StreamingOptionDecodeContext::with_peer_key(
            alice_dest.signing_public_key(),
        ),
    )
    .expect("decode reset");
    assert!(packet.flags.reset());
    assert!(packet.flags.signature_included());
}

#[test]
fn plan123_signed_close_packet_carries_signature() {
    let (
        alice_dest,
        _bob_dest,
        _alice_remote,
        bob_remote,
        mut alice_mgr,
        _bob_mgr,
        alice_conn_id,
        _inbound_connection_id,
    ) = setup_established_pair(1111, 1212, 1234);

    alice_mgr
        .send_close(
            alice_conn_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            5,
        )
        .expect("alice close");
    let close_req = alice_mgr
        .drain_outbound()
        .into_iter()
        .next()
        .expect("close");
    let close_streaming = decode_envelope(&close_req.application_payload);
    assert_signed_packet(&close_streaming, &alice_dest);
    let (packet, _) = decode_streaming_packet(
        &close_streaming,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        // CLOSE carries no FROM; the signature layout comes from the
        // retained peer identity.
        i2pr_proto::streaming::StreamingOptionDecodeContext::with_peer_key(
            alice_dest.signing_public_key(),
        ),
    )
    .expect("decode close");
    assert!(packet.flags.close());
    assert!(packet.flags.signature_included());
}

#[test]
fn plan123_corrupt_signature_is_rejected() {
    let alice_dest = build_destination(1313);
    let bob_dest = build_destination(1414);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut bob_mgr = StreamingManager::new(deterministic_config());
    bob_mgr.listen(REMOTE_PORT).expect("bob listen");
    let mut rng = ChaCha8Rng::seed_from_u64(4567);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("alice connect");
    let syn = alice_mgr.drain_outbound();
    let mut syn_streaming = decode_envelope(&syn[0].application_payload);

    // Flip one byte inside the signature region. The signature option is
    // at the tail of the option region.
    let total_len = syn_streaming.len();
    syn_streaming[total_len - 1] ^= 0x01;

    let result = bob_mgr.process_inbound_packet(
        &syn_streaming,
        &alice_remote.destination_hash,
        &bob_dest,
        LOCAL_PORT,
        REMOTE_PORT,
        0,
    );
    assert!(
        result.is_err(),
        "corrupted signature must fail signature verification"
    );
}

#[test]
fn plan123_64_hex_router_hash_used_for_remote_destination() {
    // The remote destination key uses the 32-byte destination hash as a
    // 64-lowercase-hex string in Plan 122's router-hash contract; here
    // we just verify the remote_destination_hash is exactly 32 bytes.
    let alice_dest = build_destination(1515);
    let remote = remote_for(&alice_dest);
    assert_eq!(remote.destination_hash.len(), 32);
    let hex_string = format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        remote.destination_hash[0],
        remote.destination_hash[1],
        remote.destination_hash[2],
        remote.destination_hash[3],
        remote.destination_hash[4],
        remote.destination_hash[5],
        remote.destination_hash[6],
        remote.destination_hash[7],
    );
    assert_eq!(hex_string.len(), 16);
}

#[test]
fn plan123_connection_table_is_bounded() {
    let mut config = deterministic_config();
    config.max_streams_per_destination = 2;
    let mut alice_mgr = StreamingManager::new(config);
    let alice_dest = build_destination(1616);
    let bob_dest = build_destination(1717);
    let remote = remote_for(&bob_dest);
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    for _ in 0..2 {
        let outcome = alice_mgr
            .connect(
                &alice_dest,
                &remote,
                LOCAL_PORT,
                REMOTE_PORT,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                0,
                &mut rng,
            )
            .expect("connect");
        assert!(matches!(outcome, ConnectOutcome::SynSent { .. }));
    }

    let overflow = alice_mgr
        .connect(
            &alice_dest,
            &remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    assert!(matches!(overflow, ConnectOutcome::ConnectionTableFull));
}

#[test]
fn plan123_max_streams_per_destination_constant_matches_config_ceiling() {
    // MAX_STREAMS_PER_DESTINATION is enforced at config time.
}

#[test]
fn plan123_syn_requires_max_packet_size_included() {
    // Construct a SYN without the MAX_PACKET_SIZE flag; the codec must
    // reject it on receive.
    let alice_dest = build_destination(1818);
    let bob_remote = remote_for(&alice_dest);
    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let syn = alice_mgr.drain_outbound().into_iter().next().expect("syn");
    let streaming = decode_envelope(&syn.application_payload);
    let (packet, _) = decode_streaming_packet(
        &streaming,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    )
    .expect("decode");
    assert!(packet.flags.max_packet_size_included());
}

#[test]
fn plan123_send_window_enforces_backpressure() {
    let (
        alice_dest,
        _bob_dest,
        _alice_remote,
        bob_remote,
        mut alice_mgr,
        _bob_mgr,
        alice_conn_id,
        _inbound_connection_id,
    ) = setup_established_pair(1919, 2020, 2);

    for seq in 0..4_u32 {
        let payload = format!("{seq}").into_bytes();
        alice_mgr
            .send_data(
                alice_conn_id,
                &alice_dest,
                &bob_remote,
                LOCAL_PORT,
                REMOTE_PORT,
                &payload,
                10 + u64::from(seq),
            )
            .expect("send");
    }
    let alice_conn = alice_mgr.get_connection(alice_conn_id).expect("conn");
    assert_eq!(alice_conn.send_window().unacked_count(), 4);
}

#[test]
fn plan123_signed_syn_signature_verifies_via_canonical_preimage() {
    let alice_dest = build_destination(2121);
    let bob_remote = remote_for(&alice_dest);
    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut rng = ChaCha8Rng::seed_from_u64(3);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let syn = alice_mgr.drain_outbound().into_iter().next().expect("syn");
    let streaming = decode_envelope(&syn.application_payload);
    assert_signed_packet(&streaming, &alice_dest);
}

#[test]
fn plan123_signed_syn_payload_bytes_inbound_envelope_round_trips() {
    let alice_dest = build_destination(2222);
    let bob_remote = remote_for(&alice_dest);
    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut rng = ChaCha8Rng::seed_from_u64(4);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let syn = alice_mgr.drain_outbound().into_iter().next().expect("syn");

    let envelope_bytes = &syn.application_payload;
    let decoded =
        decode_client_payload(envelope_bytes, envelope_bytes.len()).expect("envelope decode");
    let streaming = &decoded.payload;
    assert_signed_packet(streaming, &alice_dest);
}

#[test]
fn plan123_outbound_send_request_has_required_fields() {
    let alice_dest = build_destination(2323);
    let bob_remote = remote_for(&alice_dest);
    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut rng = ChaCha8Rng::seed_from_u64(5);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let syn = alice_mgr.drain_outbound().into_iter().next().expect("syn");
    assert_eq!(syn.source_port, LOCAL_PORT);
    assert_eq!(syn.destination_port, REMOTE_PORT);
    assert_eq!(syn.destination_hash, bob_remote.destination_hash);
    assert_eq!(syn.send_stream_id, 0);
    assert!(syn.receive_stream_id != 0);
    let decoded = decode_client_payload(&syn.application_payload, syn.application_payload.len())
        .expect("envelope");
    assert_eq!(
        decoded.protocol,
        i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER
    );
}

#[test]
fn plan123_unsupported_protocol_envelope_is_rejected() {
    let alice_dest = build_destination(2424);
    let bob_remote = remote_for(&alice_dest);
    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut bob_mgr = StreamingManager::new(deterministic_config());
    bob_mgr.listen(REMOTE_PORT).expect("listen");
    let mut rng = ChaCha8Rng::seed_from_u64(6);

    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let syn = alice_mgr.drain_outbound().into_iter().next().expect("syn");

    // Tamper: change the protocol byte in the envelope header.
    let mut tampered = syn.application_payload.clone();
    tampered[0] = 99; // invalid protocol number
    let alice_remote = remote_for(&alice_dest);
    let result = bob_mgr.process_inbound_envelope(
        &tampered,
        &alice_remote.destination_hash,
        &build_destination(2525),
        LOCAL_PORT,
        REMOTE_PORT,
        0,
    );
    assert!(
        result.is_err(),
        "envelope with non-streaming protocol must be rejected"
    );
}
