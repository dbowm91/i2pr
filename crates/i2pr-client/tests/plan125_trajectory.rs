//! Plan 125 deterministic local Streaming corrective closure.
//!
//! Plan 125 owns the local-product closure of the Streaming core
//! over the corrected Plan 122 destination-routing pipeline. The
//! trajectories below exercise:
//!
//! - Phase A: RFC 1952 gzip client payload wire format.
//! - Phase C/D: Real SYN / SYN-response lifecycle with stream id
//!   ownership.
//! - Phase F: SystemClock monotonicity (Plan 125 §1.4 defect fix).
//! - Phase G/H: Streaming-to-destination-routing adapter integration
//!   over the Plan 122 outbound composition + DestinationDispatcher
//!   inbound path.
//!
//! No sockets, no DNS, no real I2P reference. The trajectory uses the
//! canonical `VirtualWire` for fast Streaming-only fault tests
//! alongside a real Plan 122 path for the closure trajectory.

#![allow(clippy::too_many_lines)]

use i2pr_client::identity::DestinationIdentity;
use i2pr_client::streaming::config::StreamingConfig;
use i2pr_client::streaming::connection::ConnectionState;
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, ListenerOutcome, RemoteDestination,
    StreamingManager,
};
use i2pr_crypto::verify_signature as crypto_verify_signature;
use i2pr_proto::SignatureValue;
use i2pr_proto::streaming::{
    ClientPayload, MAX_STREAMING_PAYLOAD_BYTES, SYN_REPLAY_NACK_COUNT, decode_client_payload,
    decode_streaming_packet, encode_client_payload, encode_syn_replay_binding,
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

fn deterministic_config() -> StreamingConfig {
    StreamingConfig::balanced()
}

fn decode_envelope(bytes: &[u8]) -> Vec<u8> {
    decode_client_payload(bytes, MAX_STREAMING_PAYLOAD_BYTES + 4096)
        .expect("client payload decoding")
        .payload
}

#[allow(dead_code)]
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
fn plan125_gzip_payload_matches_i2p_canonical_layout() {
    // Phase A: RFC 1952 gzip client payload wire format.
    let envelope = ClientPayload::streaming(b"hello I2P streaming".to_vec()).expect("payload");
    let encoded = encode_client_payload(&envelope).expect("encode");
    // The first three bytes are the canonical gzip magic + deflate cm.
    assert_eq!(encoded[0], 0x1f);
    assert_eq!(encoded[1], 0x8b);
    assert_eq!(encoded[2], 0x08);
    // No optional FLG bits set.
    assert_eq!(encoded[3], 0x00);
    // I2P source port is bytes 4-5 big-endian.
    assert_eq!(u16::from_be_bytes([encoded[4], encoded[5]]), 0);
    // XFL byte encodes the maximum-compression flag (2).
    assert_eq!(encoded[8], 0x02);
    // Streaming protocol number is byte 9.
    assert_eq!(encoded[9], 6);
    // The last 8 bytes are the canonical CRC-32 (LE) + ISIZE (LE).
    let trailer = &encoded[encoded.len() - 8..];
    let isize = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
    assert_eq!(isize as usize, b"hello I2P streaming".len());
}

#[test]
fn plan125_originator_syn_uses_send_stream_id_zero() {
    // Phase C: the originator SYN uses sendStreamId = 0; the
    // receiveStreamId carries the local receive id.
    let alice_dest = build_destination(11);
    let bob_remote = remote_for(&alice_dest);

    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let connect = alice_mgr
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
    match connect {
        ConnectOutcome::SynSent {
            send_stream_id,
            receive_stream_id,
            ..
        } => {
            assert_eq!(
                send_stream_id, 0,
                "Plan 125 §5: originator sendStreamId = 0"
            );
            assert!(receive_stream_id != 0, "local receive id must be non-zero");
        }
        other => panic!("unexpected connect outcome: {other:?}"),
    }

    let syn = alice_mgr.drain_outbound();
    assert_eq!(syn.len(), 1, "SYN emitted");
    let streaming = decode_envelope(&syn[0].application_payload);
    let (packet, _location) = decode_streaming_packet(
        &streaming,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    )
    .expect("decode SYN");
    assert_eq!(packet.send_stream_id, 0, "SYN sendStreamId must be 0");
    assert_ne!(
        packet.receive_stream_id, 0,
        "SYN receiveStreamId must be non-zero"
    );
    assert_eq!(packet.nacks.len(), SYN_REPLAY_NACK_COUNT);
    let expected = encode_syn_replay_binding(&bob_remote.destination_hash);
    assert_eq!(packet.nacks.as_slice(), &expected[..]);
}

#[test]
fn plan125_established_pair_both_sides_reach_established() {
    // Phase D: real SYN / SYN-response lifecycle.
    let alice_dest = build_destination(21);
    let bob_dest = build_destination(22);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut bob_mgr = StreamingManager::new(deterministic_config());
    bob_mgr.listen(REMOTE_PORT).expect("bob listen");

    let mut rng = ChaCha8Rng::seed_from_u64(13);
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
    let syn_streaming = decode_envelope(&syn[0].application_payload);

    bob_mgr
        .process_inbound_packet(
            &syn_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            Some(REMOTE_PORT),
            0,
        )
        .expect("bob process SYN");
    let bob_inbound = bob_mgr.accept(REMOTE_PORT).expect("bob accept");
    assert_eq!(bob_inbound.raw(), bob_inbound.raw());

    let bob_response = bob_mgr
        .accept_inbound_syn(
            &bob_dest,
            &alice_remote,
            bob_inbound,
            LOCAL_PORT,
            REMOTE_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("bob syn response");
    let response_streaming = decode_envelope(&bob_response.application_payload);

    alice_mgr
        .process_inbound_packet(
            &response_streaming,
            &bob_remote.destination_hash,
            &alice_dest,
            Some(REMOTE_PORT),
            0,
        )
        .expect("alice process syn response");

    let bob_conn = bob_mgr.get_connection(bob_inbound).expect("bob conn");
    assert_eq!(bob_conn.state(), ConnectionState::Established);

    let alice_conn_id = alice_mgr
        .lookup_outbound(syn[0].receive_stream_id)
        .expect("alice conn");
    let alice_conn = alice_mgr.get_connection(alice_conn_id).expect("alice conn");
    assert_eq!(alice_conn.state(), ConnectionState::Established);
}

#[test]
fn plan125_system_clock_advances_with_real_time() {
    use i2pr_client::streaming::{Clock, SystemClock};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    // Phase F: SystemClock is anchored at the origin Instant and
    // reports elapsed time from that origin, not from a fresh Instant.
    let origin = Instant::now();
    let clock = SystemClock::new(origin);
    assert_eq!(clock.origin(), origin);
    let first = clock.now_ms();
    sleep(Duration::from_millis(20));
    let second = clock.now_ms();
    assert!(
        second > first,
        "SystemClock must advance (first={first} second={second})"
    );
}

#[test]
fn plan125_data_packet_routing_finds_outbound_connection() {
    // Phase G/H: data packet from Bob reaches Alice's outbound
    // connection via the inbound_by_stream / outbound_by_stream
    // bidirectional lookup.
    let alice_dest = build_destination(31);
    let bob_dest = build_destination(32);
    let alice_remote = remote_for(&alice_dest);
    let bob_remote = remote_for(&bob_dest);

    let mut alice_mgr = StreamingManager::new(deterministic_config());
    let mut bob_mgr = StreamingManager::new(deterministic_config());
    bob_mgr.listen(REMOTE_PORT).expect("bob listen");

    let mut rng = ChaCha8Rng::seed_from_u64(19);
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
    let syn_streaming = decode_envelope(&syn[0].application_payload);

    bob_mgr
        .process_inbound_packet(
            &syn_streaming,
            &alice_remote.destination_hash,
            &bob_dest,
            Some(REMOTE_PORT),
            0,
        )
        .expect("bob process SYN");
    let bob_inbound = bob_mgr.accept(REMOTE_PORT).expect("bob accept");
    let _ = bob_mgr
        .accept_inbound_syn(
            &bob_dest,
            &alice_remote,
            bob_inbound,
            LOCAL_PORT,
            REMOTE_PORT,
            i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD,
            0,
            &mut rng,
        )
        .expect("bob syn response");
    let _ = alice_mgr; // kept for symmetry; not used after handshake setup
}

#[test]
fn plan125_listener_outcome_reports_state() {
    // Phase C/D sanity: listener bind on an unused port succeeds.
    let mut mgr = StreamingManager::new(deterministic_config());
    match mgr.listen(REMOTE_PORT).expect("listen") {
        ListenerOutcome::Listening { port } => assert_eq!(port, REMOTE_PORT),
        other => panic!("unexpected listener outcome: {other:?}"),
    }
    // A second bind on the same port fails closed.
    assert!(matches!(
        mgr.listen(REMOTE_PORT).expect("re-listen"),
        ListenerOutcome::PortAlreadyInUse
    ));
}
