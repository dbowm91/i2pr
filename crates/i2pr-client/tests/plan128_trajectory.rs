//! Plan 128 §12 / §9 / §6 deterministic Streaming manager trajectories.
//!
//! Fast Streaming-only tests over the corrected Plan 128 wire format:
//!
//! - the canonical SYN -> SYN-response handshake with exact stream-id
//!   ownership on subsequent data packets,
//! - CLOSE (`0x000A`) and RESET (`0x000C`) shapes with raw final
//!   signatures verified against the retained peer identity,
//! - signature corruption failing closed,
//! - payload-max negotiation as `min(local, remote)` with intentionally
//!   different advertised values.
//!
//! No sockets, no DNS, no external I2P reference.

use i2pr_client::identity::DestinationIdentity;
use i2pr_client::streaming::config::StreamingConfig;
use i2pr_client::streaming::connection::ConnectionState;
use i2pr_client::streaming::manager::{ConnectOutcome, RemoteDestination, StreamingManager};
use i2pr_proto::streaming::{
    CLOSE_FLAGS, MAX_STREAMING_PACKET_BYTES, RESET_FLAGS, SYN_REPLAY_NACK_COUNT, SignatureLocation,
    StreamingOptionDecodeContext, StreamingReceiveLimit, build_signature_preimage,
    decode_client_payload, decode_streaming_packet, encode_syn_replay_binding,
    peek_streaming_header,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const LOCAL_PORT: u16 = 0x1234;
const REMOTE_PORT: u16 = 0xabcd;

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

fn decode_envelope(bytes: &[u8]) -> Vec<u8> {
    decode_client_payload(bytes, MAX_STREAMING_PACKET_BYTES + 256)
        .expect("client payload decoding")
        .payload
}

/// Established pair helper driving the canonical Plan 128 handshake.
#[allow(clippy::too_many_arguments)]
fn establish_pair(
    alice_seed: u64,
    bob_seed: u64,
    alice_advertised_max: u16,
    bob_advertised_max: u16,
) -> (
    DestinationIdentity,
    DestinationIdentity,
    RemoteDestination,
    RemoteDestination,
    StreamingManager,
    StreamingManager,
    i2pr_client::streaming::connection::ConnectionId,
    i2pr_client::streaming::connection::ConnectionId,
) {
    let alice_dest = build_destination(alice_seed);
    let bob_dest = build_destination(bob_seed);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(StreamingConfig::balanced());
    let mut bob_mgr = StreamingManager::new(StreamingConfig::balanced());
    bob_mgr.listen(REMOTE_PORT).expect("bob listen");

    let mut rng = ChaCha8Rng::seed_from_u64(77);
    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            alice_advertised_max,
            0,
            &mut rng,
        )
        .expect("alice connect");
    let syn = alice_mgr.drain_outbound();
    assert_eq!(syn.len(), 1);

    // B validates the canonical SYN.
    let syn_streaming = decode_envelope(&syn[0].application_payload);
    bob_mgr
        .process_inbound_packet(
            &syn_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            0,
        )
        .expect("bob process SYN");
    let bob_inbound = bob_mgr.accept(REMOTE_PORT).expect("bob pending accept");

    // B accept emits the canonical SYN response.
    let response = bob_mgr
        .accept_inbound_syn(
            &bob_dest,
            &alice_remote,
            bob_inbound,
            REMOTE_PORT,
            LOCAL_PORT,
            bob_advertised_max,
            0,
            &mut rng,
        )
        .expect("bob syn response");
    // A validates; both sides reach Established.
    let response_streaming = decode_envelope(&response.application_payload);
    alice_mgr
        .process_inbound_packet(
            &response_streaming,
            &bob_remote.destination_hash,
            &alice_dest,
            REMOTE_PORT,
            LOCAL_PORT,
            0,
        )
        .expect("alice process syn response");

    let alice_conn_id = alice_mgr
        .lookup_outbound(syn[0].receive_stream_id)
        .expect("alice conn id");

    (
        alice_dest,
        bob_dest,
        alice_remote,
        bob_remote,
        alice_mgr,
        bob_mgr,
        alice_conn_id,
        bob_inbound,
    )
}

/// Plan 128 §12: full handshake plus one ordinary data packet each
/// direction with exact stream-id field assertions.
#[test]
fn plan128_manager_handshake_and_bidirectional_data_stream_ids() {
    let (
        alice_dest,
        bob_dest,
        alice_remote,
        bob_remote,
        mut alice_mgr,
        mut bob_mgr,
        alice_conn_id,
        bob_conn_id,
    ) = establish_pair(11, 22, 1730, 1730);

    assert_eq!(
        alice_mgr.get_connection(alice_conn_id).unwrap().state(),
        ConnectionState::Established
    );
    assert_eq!(
        bob_mgr.get_connection(bob_conn_id).unwrap().state(),
        ConnectionState::Established
    );

    // Alice sends one ordinary data packet.
    alice_mgr
        .send_data(
            alice_conn_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            b"ping",
            20,
        )
        .expect("alice data");
    let from_alice = alice_mgr.drain_outbound();
    // Plan 130: the first ordinary application packet carries
    // sequence 1; sequence 0 is owned by the SYN / SYN-response /
    // plain-ACK forms.
    let ping = from_alice
        .iter()
        .find(|request| request.sequence == 1)
        .expect("alice ping packet");
    let ping_streaming = decode_envelope(&ping.application_payload);

    let bob_observation = bob_mgr
        .process_inbound_packet(
            &ping_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            20,
        )
        .expect("bob receive ping");

    // A future sendStreamId == B's selected receive id; A's
    // receiveStreamId == A's own id.
    let alice_conn = alice_mgr.get_connection(alice_conn_id).unwrap();
    let b_receive_id = bob_conn_id_raw(&bob_mgr, bob_conn_id);
    assert_eq!(ping.send_stream_id, b_receive_id);
    assert_eq!(ping.receive_stream_id, alice_conn.local_stream_id());
    assert_eq!(bob_observation.send_stream_id, b_receive_id);
    assert_eq!(
        bob_observation.receive_stream_id,
        alice_conn.local_stream_id()
    );

    // Bob replies with one ordinary data packet.
    let _ = alice_conn;
    bob_mgr
        .send_data(
            bob_conn_id,
            &bob_dest,
            &alice_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            b"pong",
            30,
        )
        .expect("bob data");
    let from_bob = bob_mgr.drain_outbound();
    let pong = from_bob
        .iter()
        .find(|request| request.sequence == 1)
        .expect("bob pong packet");
    let pong_streaming = decode_envelope(&pong.application_payload);
    let pong_wire_send_id = {
        let peek = peek_streaming_header(&pong_streaming).unwrap();
        peek.send_stream_id
    };

    let alice_observation = alice_mgr
        .process_inbound_packet(
            &pong_streaming,
            &bob_remote.destination_hash,
            &alice_dest,
            REMOTE_PORT,
            LOCAL_PORT,
            30,
        )
        .expect("alice receive pong");

    // B future sendStreamId == A's receive id; B receiveStreamId ==
    // B's own id.
    let a_receive_id = alice_mgr
        .get_connection(alice_conn_id)
        .unwrap()
        .local_stream_id();
    assert_eq!(pong_wire_send_id, a_receive_id);
    let bob_conn = bob_mgr.get_connection(bob_conn_id).unwrap();
    assert_eq!(pong.send_stream_id, a_receive_id);
    assert_eq!(pong.receive_stream_id, bob_conn.local_stream_id());
    assert_eq!(alice_observation.send_stream_id, a_receive_id);
    assert_eq!(
        alice_observation.receive_stream_id,
        bob_conn.local_stream_id()
    );
}

/// Returns the inbound connection's local receive stream id (the id
/// Bob selected and Alice addresses).
fn bob_conn_id_raw(
    bob_mgr: &StreamingManager,
    connection_id: i2pr_client::streaming::connection::ConnectionId,
) -> u32 {
    bob_mgr
        .get_connection(connection_id)
        .expect("bob conn")
        .local_stream_id()
}

/// Plan 128 §9: emitted CLOSE carries flags 0x000A with the raw final
/// signature, and the receiver verifies it against the retained peer
/// identity without any FROM option.
#[test]
fn plan128_close_shape_and_retained_peer_verification() {
    let (
        alice_dest,
        bob_dest,
        alice_remote,
        bob_remote,
        mut alice_mgr,
        mut bob_mgr,
        alice_conn_id,
        _bob_conn_id,
    ) = establish_pair(31, 32, 1730, 1400);

    alice_mgr
        .send_close(
            alice_conn_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            40,
        )
        .expect("alice close");
    let close_req = alice_mgr
        .drain_outbound()
        .into_iter()
        .next()
        .expect("close packet");
    let close_streaming = decode_envelope(&close_req.application_payload);

    // Shape: flags 0x000A, no FROM, raw final signature of exactly 64
    // bytes at the tail.
    let (packet, location) = decode_streaming_packet(
        &close_streaming,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::with_peer_key(alice_dest.signing_public_key()),
    )
    .expect("decode close");
    assert_eq!(packet.flags.bits(), CLOSE_FLAGS);
    assert!(!packet.flags.from_included());
    assert!(packet.options.from_destination.is_none());
    assert_eq!(packet.options.signature.as_ref().map(Vec::len), Some(64));
    let SignatureLocation { offset, length } = location.expect("signature location");
    assert_eq!(offset + length, close_streaming.len());

    // Receiver-side processing verifies with the retained peer key and
    // drives the inbound connection toward Closed.
    let observation = bob_mgr
        .process_inbound_packet(
            &close_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            40,
        )
        .expect("bob verify close");
    assert!(observation.flags & CLOSE_FLAGS == CLOSE_FLAGS);

    // Corruption: flip one signature byte; verification must fail.
    let mut corrupted = close_streaming.clone();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 0x01;
    let result = bob_mgr.process_inbound_packet(
        &corrupted,
        &alice_remote.destination_hash,
        &bob_dest,
        LOCAL_PORT,
        REMOTE_PORT,
        41,
    );
    assert!(
        result.is_err(),
        "corrupted CLOSE signature must fail closed"
    );
}

/// Plan 128 §9: emitted RESET carries flags 0x000C with the raw final
/// signature, and an unsigned RESET fails closed on receipt.
#[test]
fn plan128_reset_shape_and_unsigned_control_rejection() {
    let (
        alice_dest,
        bob_dest,
        alice_remote,
        bob_remote,
        mut alice_mgr,
        mut bob_mgr,
        alice_conn_id,
        _bob_conn_id,
    ) = establish_pair(41, 42, 1730, 1730);

    alice_mgr
        .send_reset(
            alice_conn_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            50,
        )
        .expect("alice reset");
    let reset_req = alice_mgr
        .drain_outbound()
        .into_iter()
        .next()
        .expect("reset packet");
    let reset_streaming = decode_envelope(&reset_req.application_payload);

    let (packet, location) = decode_streaming_packet(
        &reset_streaming,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::with_peer_key(alice_dest.signing_public_key()),
    )
    .expect("decode reset");
    assert_eq!(packet.flags.bits(), RESET_FLAGS);
    assert!(!packet.flags.from_included());
    assert_eq!(packet.options.signature.as_ref().map(Vec::len), Some(64));
    let SignatureLocation { offset, length } = location.expect("signature location");
    assert_eq!(offset + length, reset_streaming.len());

    bob_mgr
        .process_inbound_packet(
            &reset_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            50,
        )
        .expect("bob verify reset");

    // Strip the signature flag so the same bytes present an unsigned
    // standalone control packet; the receiver must fail closed.
    let mut unsigned = reset_streaming.clone();
    let flags_offset = 18usize; // nackCount == 0
    let flags = u16::from_be_bytes([unsigned[flags_offset], unsigned[flags_offset + 1]]);
    let unsigned_flags = flags & !0x0008; // clear SIGNATURE_INCLUDED
    unsigned[flags_offset..flags_offset + 2].copy_from_slice(&unsigned_flags.to_be_bytes());
    // Truncate the now-unexplained option region entirely.
    let option_size = u16::from_be_bytes([unsigned[20], unsigned[21]]) as usize;
    let end = unsigned.len();
    unsigned.drain(end - option_size..end);
    unsigned[20..22].copy_from_slice(&0_u16.to_be_bytes());
    let result = bob_mgr.process_inbound_packet(
        &unsigned,
        &alice_remote.destination_hash,
        &bob_dest,
        LOCAL_PORT,
        REMOTE_PORT,
        51,
    );
    assert!(
        matches!(
            result,
            Err(
                i2pr_client::streaming::manager::StreamingManagerError::Codec(
                    i2pr_proto::streaming::StreamingPacketError::ResetMissingSignature
                )
            )
        ),
        "unsigned RESET must fail closed: {result:?}"
    );
}

/// Plan 128 §9: a signed standalone control packet delivered to an
/// unknown stream has no identity context and fails closed.
#[test]
fn plan128_unknown_standalone_signed_control_fails_closed() {
    let (
        alice_dest,
        _bob_dest,
        alice_remote,
        bob_remote,
        mut alice_mgr,
        mut bob_mgr,
        alice_conn_id,
        _bob_conn_id,
    ) = establish_pair(51, 52, 1730, 1730);

    alice_mgr
        .send_reset(
            alice_conn_id,
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            60,
        )
        .expect("reset");
    let reset_req = alice_mgr.drain_outbound().into_iter().next().unwrap();
    let reset_streaming = decode_envelope(&reset_req.application_payload);
    // Retarget the packet at a stream id no connection owns.
    let mut stray = reset_streaming.clone();
    stray[0..4].copy_from_slice(&0x7E57_C0DE_u32.to_be_bytes());
    let result = bob_mgr.process_inbound_packet(
        &stray,
        &alice_remote.destination_hash,
        &build_destination(53),
        LOCAL_PORT,
        REMOTE_PORT,
        60,
    );
    assert!(
        result.is_err(),
        "signed control on unknown stream must fail closed"
    );
}

/// Plan 128 §6: negotiated payload max is `min(local advertised,
/// remote advertised)` using intentionally different values.
#[test]
fn plan128_negotiated_max_is_min_of_both_advertisements() {
    // Alice advertises 1200, Bob advertises 2000 -> negotiated 1200.
    let (_, _, _, _, alice_mgr, bob_mgr, alice_conn_id, bob_conn_id) =
        establish_pair(61, 62, 1200, 2000);
    assert_eq!(
        alice_mgr
            .get_connection(alice_conn_id)
            .unwrap()
            .max_payload_size(),
        1200
    );
    assert_eq!(
        bob_mgr
            .get_connection(bob_conn_id)
            .unwrap()
            .max_payload_size(),
        1200
    );

    // Reverse: Alice advertises 3000 (clamped by the wire ceiling),
    // Bob advertises 1500 -> negotiated 1500.
    let (_, _, _, _, alice_mgr, bob_mgr, alice_conn_id, bob_conn_id) =
        establish_pair(63, 64, 3000, 1500);
    assert_eq!(
        alice_mgr
            .get_connection(alice_conn_id)
            .unwrap()
            .max_payload_size(),
        1500
    );
    assert_eq!(
        bob_mgr
            .get_connection(bob_conn_id)
            .unwrap()
            .max_payload_size(),
        1500
    );
}

/// Plan 128 §12 / Plan 125 §6: the originator remains
/// `OutboundSynSent` after connect and only transitions to
/// `Established` once the valid signed SYN response is processed.
#[test]
fn plan128_originator_stays_outbound_syn_sent_until_valid_response() {
    let alice_dest = build_destination(81);
    let bob_dest = build_destination(82);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(StreamingConfig::balanced());
    let mut bob_mgr = StreamingManager::new(StreamingConfig::balanced());
    bob_mgr.listen(REMOTE_PORT).expect("bob listen");
    let mut rng = ChaCha8Rng::seed_from_u64(83);

    let connect = alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let ConnectOutcome::SynSent { connection_id, .. } = connect else {
        panic!("expected SynSent, got {connect:?}")
    };
    assert_eq!(
        alice_mgr.get_connection(connection_id).unwrap().state(),
        ConnectionState::OutboundSynSent
    );
    // An unrelated data packet must not move the connection out of
    // SynSent; it cannot even be routed (unknown stream).
    let syn = alice_mgr.drain_outbound();
    let syn_streaming = decode_envelope(&syn[0].application_payload);
    bob_mgr
        .process_inbound_packet(
            &syn_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            LOCAL_PORT,
            REMOTE_PORT,
            0,
        )
        .expect("bob process syn");
    let bob_inbound = bob_mgr.accept(REMOTE_PORT).expect("pending accept");
    let response = bob_mgr
        .accept_inbound_syn(
            &bob_dest,
            &alice_remote,
            bob_inbound,
            REMOTE_PORT,
            LOCAL_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("response");
    // Alice still has not processed the response.
    assert_eq!(
        alice_mgr.get_connection(connection_id).unwrap().state(),
        ConnectionState::OutboundSynSent
    );

    let response_streaming = decode_envelope(&response.application_payload);
    alice_mgr
        .process_inbound_packet(
            &response_streaming,
            &bob_remote.destination_hash,
            &alice_dest,
            REMOTE_PORT,
            LOCAL_PORT,
            0,
        )
        .expect("process response");
    assert_eq!(
        alice_mgr.get_connection(connection_id).unwrap().state(),
        ConnectionState::Established
    );
}

/// Plan 128 §7: the initial SYN carries the Proposal 164 replay
/// binding (eight NACK words holding the receiver destination hash)
/// and the signature covers that replay hash.
#[test]
fn plan128_initial_syn_replay_binding_and_signature_cover_it() {
    use i2pr_crypto::verify_signature;
    use i2pr_proto::SignatureValue;

    let alice_dest = build_destination(71);
    let bob_dest = build_destination(72);
    let _alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(StreamingConfig::balanced());
    let mut rng = ChaCha8Rng::seed_from_u64(73);
    alice_mgr
        .connect(
            &alice_dest,
            &bob_remote,
            LOCAL_PORT,
            REMOTE_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("connect");
    let syn = alice_mgr.drain_outbound().into_iter().next().unwrap();
    let syn_streaming = decode_envelope(&syn.application_payload);

    let peek = peek_streaming_header(&syn_streaming).unwrap();
    assert_eq!(
        peek.flags_bits & !i2pr_proto::streaming::FLAG_RESERVED_MASK,
        i2pr_proto::streaming::INITIAL_SYN_FLAGS
    );
    let (packet, location) = decode_streaming_packet(
        &syn_streaming,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::anonymous(),
    )
    .expect("decode syn");
    assert_eq!(packet.nacks.len(), SYN_REPLAY_NACK_COUNT);
    assert_eq!(
        packet.nacks[..],
        encode_syn_replay_binding(&bob_remote.destination_hash)[..]
    );

    // The preimage zeroes exactly the raw signature bytes and verifies
    // against the FROM signing key; the replay hash participates in
    // the signed bytes because it sits before the signature.
    let destination = packet.options.from_destination.clone().unwrap();
    let signature = packet.options.signature.clone().unwrap();
    let preimage = build_signature_preimage(&syn_streaming, location);
    let value = SignatureValue::new(destination.signing_key().key_type(), signature).unwrap();
    verify_signature(destination.signing_key(), &preimage, &value).expect("SYN verifies");
}
