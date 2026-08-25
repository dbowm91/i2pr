//! Plan 128 §11 Streaming packet wire fixture/reference tests.
//!
//! These tests are independent of `StreamingManager` behavior: they
//! pin the normative flag constants, option-region layout, MAX_
//! PACKET_SIZE payload semantics, replay-binding NACK words, raw
//! final-signature placement, and the canonical zeroed-signature
//! preimage against fixed byte expectations.
//!
//! Normative provenance: `specs/references/streaming-packet-wire.md`.

#![allow(clippy::too_many_lines)]

use i2pr_proto::streaming::{
    CLOSE_FLAGS, DEFAULT_ADVERTISED_MAX_PAYLOAD, FLAG_CLOSE, FLAG_DELAY_REQUESTED, FLAG_ECHO,
    FLAG_FROM_INCLUDED, FLAG_MAX_PACKET_SIZE_INCLUDED, FLAG_NO_ACK, FLAG_OFFLINE_SIGNATURE,
    FLAG_PROFILE_INTERACTIVE, FLAG_RESET, FLAG_SIGNATURE_INCLUDED, FLAG_SIGNATURE_REQUESTED,
    FLAG_SYNCHRONIZE, INITIAL_SYN_FLAGS, MAX_STREAMING_NACK_COUNT, MAX_STREAMING_OPTION_BYTES,
    MAX_STREAMING_PACKET_BYTES, MAX_STREAMING_PAYLOAD_BYTES, MIN_STREAMING_HEADER_BYTES,
    RESET_FLAGS, SYN_REPLAY_NACK_COUNT, SYN_RESPONSE_FLAGS, SignatureLocation,
    StreamingOptionDecodeContext, StreamingOptions, StreamingPacketBuilder, StreamingReceiveLimit,
    StreamingSendLimit, build_signature_preimage, decode_streaming_packet, encode_streaming_packet,
    encode_syn_replay_binding, install_packet_signature, peek_streaming_header,
};
use i2pr_proto::{
    Certificate, CryptoKeyType, Destination, KeyAndCert, KeyCertificate, PublicKey, SigningKeyType,
    SigningPublicKey,
};

/// Builds a structurally valid Ed25519+X25519 Destination with
/// deliberately asymmetric filler bytes so byte-order mistakes cannot
/// pass accidentally.
fn asymmetric_destination() -> Destination {
    let public_len = CryptoKeyType::X25519.public_key_len().unwrap();
    let signing_len = SigningKeyType::EdDsaSha512Ed25519.public_key_len().unwrap();
    let padding_len = 384 - public_len - signing_len;
    let keys = KeyAndCert::new(
        PublicKey::new(CryptoKeyType::X25519, vec![0xA5; public_len]).unwrap(),
        SigningPublicKey::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x5C; signing_len]).unwrap(),
        vec![0x77; padding_len],
        Certificate::Key(
            KeyCertificate::for_types(SigningKeyType::EdDsaSha512Ed25519, CryptoKeyType::X25519)
                .unwrap(),
        ),
    )
    .unwrap();
    Destination::new(keys).unwrap()
}

const SIG_LEN: usize = 64;

#[test]
fn plan128_every_flag_constant_matches_normative_i2p_bit_assignment() {
    assert_eq!(FLAG_SYNCHRONIZE, 0x0001);
    assert_eq!(FLAG_CLOSE, 0x0002);
    assert_eq!(FLAG_RESET, 0x0004);
    assert_eq!(FLAG_SIGNATURE_INCLUDED, 0x0008);
    assert_eq!(FLAG_SIGNATURE_REQUESTED, 0x0010);
    assert_eq!(FLAG_FROM_INCLUDED, 0x0020);
    assert_eq!(FLAG_DELAY_REQUESTED, 0x0040);
    assert_eq!(FLAG_MAX_PACKET_SIZE_INCLUDED, 0x0080);
    assert_eq!(FLAG_PROFILE_INTERACTIVE, 0x0100);
    assert_eq!(FLAG_ECHO, 0x0200);
    assert_eq!(FLAG_NO_ACK, 0x0400);
    assert_eq!(FLAG_OFFLINE_SIGNATURE, 0x0800);
    assert_eq!(FLAG_SYNCHRONIZE << 1, FLAG_CLOSE);
}

#[test]
fn plan128_policy_flag_sets_match_current_m6_packet_shapes() {
    // Initial originator SYN = SYN | SIG | FROM | MAX | NO_ACK.
    assert_eq!(INITIAL_SYN_FLAGS, 0x04A9);
    assert_eq!(
        INITIAL_SYN_FLAGS,
        FLAG_SYNCHRONIZE
            | FLAG_SIGNATURE_INCLUDED
            | FLAG_FROM_INCLUDED
            | FLAG_MAX_PACKET_SIZE_INCLUDED
            | FLAG_NO_ACK
    );
    // SYN response = SYN | SIG | FROM | MAX (no NO_ACK).
    assert_eq!(SYN_RESPONSE_FLAGS, 0x00A9);
    assert_eq!(
        SYN_RESPONSE_FLAGS,
        FLAG_SYNCHRONIZE
            | FLAG_SIGNATURE_INCLUDED
            | FLAG_FROM_INCLUDED
            | FLAG_MAX_PACKET_SIZE_INCLUDED
    );
    // CLOSE / RESET carry only their control bit plus SIGNATURE.
    assert_eq!(CLOSE_FLAGS, 0x000A);
    assert_eq!(CLOSE_FLAGS, FLAG_CLOSE | FLAG_SIGNATURE_INCLUDED);
    assert_eq!(RESET_FLAGS, 0x000C);
    assert_eq!(RESET_FLAGS, FLAG_RESET | FLAG_SIGNATURE_INCLUDED);
}

#[test]
fn plan128_size_constants_separate_payload_from_packet_bounds() {
    // Minimum fixed header.
    assert_eq!(MIN_STREAMING_HEADER_BYTES, 22);
    // Default advertised maximum is the current I2P payload default.
    assert_eq!(DEFAULT_ADVERTISED_MAX_PAYLOAD, 1730);
    // The negotiated-payload ceiling bounds the payload only; it must
    // NOT be defined as the packet ceiling minus the header.
    assert_eq!(MAX_STREAMING_PAYLOAD_BYTES, 1730);
    assert_ne!(
        MAX_STREAMING_PAYLOAD_BYTES,
        MAX_STREAMING_PACKET_BYTES - MIN_STREAMING_HEADER_BYTES
    );
    // The full encoded packet ceiling covers header + NACKs + options
    // + payload as a checked sum of independent bounds.
    assert_eq!(
        MAX_STREAMING_PACKET_BYTES,
        MIN_STREAMING_HEADER_BYTES
            + MAX_STREAMING_NACK_COUNT * 4
            + MAX_STREAMING_OPTION_BYTES
            + MAX_STREAMING_PAYLOAD_BYTES
    );
    // A max-size payload plus minimum header fits inside the packet
    // ceiling but the reverse equality never holds.
    const {
        assert!(
            MIN_STREAMING_HEADER_BYTES + MAX_STREAMING_PAYLOAD_BYTES <= MAX_STREAMING_PACKET_BYTES
        )
    }
}

#[test]
fn plan128_initial_syn_wire_layout_is_exact() {
    let destination = asymmetric_destination();
    let destination_bytes = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .unwrap();
    let receiver_hash: [u8; 32] = core::array::from_fn(|index| index as u8);

    let options = StreamingOptions {
        delay_requested: None,
        from_destination: Some(destination.clone()),
        max_payload_size: Some(DEFAULT_ADVERTISED_MAX_PAYLOAD),
        signature: None,
    };
    let option_bytes = options
        .encode_with_placeholder(
            i2pr_proto::streaming::StreamingFlags::new(INITIAL_SYN_FLAGS).unwrap(),
            SIG_LEN,
        )
        .unwrap();

    // Exact option-region layout: [destination][06 c2][64 zero bytes].
    let mut expected_options = Vec::new();
    expected_options.extend_from_slice(&destination_bytes);
    expected_options.extend_from_slice(&1730_u16.to_be_bytes());
    expected_options.resize(expected_options.len() + SIG_LEN, 0_u8);
    assert_eq!(option_bytes, expected_options);
    // MAX_PACKET_SIZE 1730 encodes exactly to 06 c2 big-endian and
    // sits immediately before the signature placeholder.
    assert_eq!(
        &option_bytes[destination_bytes.len()..destination_bytes.len() + 2],
        &[0x06, 0xC2]
    );

    let nacks = encode_syn_replay_binding(&receiver_hash).to_vec();
    assert_eq!(nacks.len(), SYN_REPLAY_NACK_COUNT);
    let builder =
        StreamingPacketBuilder::new_initial_syn(0, 0x1122_3344, 0, option_bytes.clone(), nacks)
            .unwrap();
    let wire = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();

    // Fixed header fields at their canonical offsets.
    assert_eq!(&wire[0..4], &0_u32.to_be_bytes(), "sendStreamId = 0");
    assert_eq!(&wire[4..8], &0x1122_3344_u32.to_be_bytes());
    assert_eq!(&wire[8..12], &0_u32.to_be_bytes(), "sequenceNum = 0");
    assert_eq!(wire[16], SYN_REPLAY_NACK_COUNT as u8, "replay NACK count");
    // The NACK words carry the exact receiver Destination hash bytes.
    assert_eq!(&wire[17..49], &receiver_hash);
    assert_eq!(wire[49], 0, "resendDelay");
    assert_eq!(&wire[50..52], &INITIAL_SYN_FLAGS.to_be_bytes());

    // Round trip through the strict decoder.
    let (packet, location) = decode_streaming_packet(
        &wire,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::anonymous(),
    )
    .unwrap();
    assert_eq!(packet.flags.bits(), INITIAL_SYN_FLAGS);
    assert!(packet.flags.no_ack());
    assert_eq!(packet.send_stream_id, 0);
    assert_eq!(packet.receive_stream_id, 0x1122_3344);
    assert_eq!(packet.nacks.len(), SYN_REPLAY_NACK_COUNT);
    assert_eq!(
        &packet.nacks[..],
        &encode_syn_replay_binding(&receiver_hash)[..]
    );
    assert_eq!(
        packet.options.max_payload_size,
        Some(DEFAULT_ADVERTISED_MAX_PAYLOAD)
    );
    let from = packet.options.from_destination.as_ref().unwrap();
    assert_eq!(
        from.signing_key().as_bytes(),
        destination.signing_key().as_bytes()
    );

    // The signature location covers exactly the trailing 64 bytes of
    // the option region and nothing else.
    let location = location.unwrap();
    let option_start = MIN_STREAMING_HEADER_BYTES + SYN_REPLAY_NACK_COUNT * 4;
    assert_eq!(location.offset, option_start + option_bytes.len() - SIG_LEN);
    assert_eq!(location.length, SIG_LEN);
    assert_eq!(
        &wire[location.offset..location.offset + location.length],
        &[0u8; SIG_LEN]
    );
}

#[test]
fn plan128_syn_response_has_zero_nacks_and_no_no_ack_bit() {
    let destination = asymmetric_destination();
    let options = StreamingOptions {
        delay_requested: None,
        from_destination: Some(destination.clone()),
        max_payload_size: Some(1400), // intentionally different MTU value
        signature: None,
    };
    let option_bytes = options
        .encode_with_placeholder(
            i2pr_proto::streaming::StreamingFlags::new(SYN_RESPONSE_FLAGS).unwrap(),
            SIG_LEN,
        )
        .unwrap();
    let builder =
        StreamingPacketBuilder::new_syn_response(0xAAAA_1111, 0xBBBB_2222, 0, 0, option_bytes)
            .unwrap();
    let wire = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();

    // With nackCount == 0 the flag field sits directly after
    // resendDelay in the fixed 22-byte header.
    assert_eq!(&wire[18..20], &SYN_RESPONSE_FLAGS.to_be_bytes());
    assert_eq!(wire[16], 0, "response NACK count must be 0");

    let (packet, _) = decode_streaming_packet(
        &wire,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::anonymous(),
    )
    .unwrap();
    assert_eq!(packet.nacks.len(), 0);
    assert!(!packet.flags.no_ack());
    assert_eq!(packet.options.max_payload_size, Some(1400));
    assert_eq!(packet.send_stream_id, 0xAAAA_1111);
    assert_eq!(packet.receive_stream_id, 0xBBBB_2222);
}

#[test]
fn plan128_option_region_contains_no_synthetic_tlv_tags() {
    let destination = asymmetric_destination();
    let destination_bytes = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .unwrap();
    let options = StreamingOptions {
        delay_requested: None,
        from_destination: Some(destination.clone()),
        max_payload_size: Some(DEFAULT_ADVERTISED_MAX_PAYLOAD),
        signature: None,
    };
    let option_bytes = options
        .encode_with_placeholder(
            i2pr_proto::streaming::StreamingFlags::new(INITIAL_SYN_FLAGS).unwrap(),
            SIG_LEN,
        )
        .unwrap();
    // The historical invented TLV encoding emitted type=1 / length=4 in
    // front of MAX_PACKET_SIZE and type=3 / length=64 in front of the
    // signature. Neither marker may appear anywhere in the region.
    assert!(
        !option_bytes.windows(2).any(|w| w == [0x01, 0x04]),
        "MAX_PACKET_SIZE must not carry a type=1 length=4 TLV prefix"
    );
    assert!(
        !option_bytes.windows(2).any(|w| w == [0x03, SIG_LEN as u8]),
        "signature must not carry a type=3 length TLV prefix"
    );
    // The full region is exactly destination + u16 max + signature.
    assert_eq!(option_bytes.len(), destination_bytes.len() + 2 + SIG_LEN);
    assert_eq!(
        &option_bytes[..destination_bytes.len()],
        &destination_bytes[..]
    );
}

#[test]
fn plan128_signature_is_last_raw_option_field_and_preimage_differs_only_by_signature_zeroing() {
    let destination = asymmetric_destination();
    let flags = i2pr_proto::streaming::StreamingFlags::new(CLOSE_FLAGS).unwrap();
    // A signed CLOSE carries ONLY the signature field in its option
    // region; there is no FROM since 0.9.20.
    let option_bytes = options_close_with_signature_tail();
    let builder = StreamingPacketBuilder {
        send_stream_id: 0x0F0F_0F0F,
        receive_stream_id: 0x1010_1010,
        sequence_num: 3,
        ack_through: 7,
        nacks: Vec::new(),
        resend_delay: 0,
        flags,
        option_bytes: option_bytes.clone(),
        payload: Vec::new(),
    };
    let wire = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();

    let context = StreamingOptionDecodeContext::with_peer_key(destination.signing_key());
    let (packet, location) =
        decode_streaming_packet(&wire, StreamingReceiveLimit::default(), context).unwrap();
    assert_eq!(packet.flags.bits(), CLOSE_FLAGS);
    let SignatureLocation { offset, length } = location.expect("signed packet has signature");
    assert_eq!(length, SIG_LEN);
    // The signature occupies exactly the tail of the packet.
    assert_eq!(offset + length, wire.len());
    assert_eq!(
        &wire[offset..],
        &option_bytes[option_bytes.len() - SIG_LEN..]
    );

    // Zeroing exactly those bytes yields the canonical preimage; every
    // other byte is untouched.
    let preimage = build_signature_preimage(&wire, location);
    assert_eq!(preimage.len(), wire.len());
    assert_eq!(&preimage[offset..offset + length], &[0u8; SIG_LEN]);
    let mut expected_preimage = wire.clone();
    expected_preimage[offset..offset + length].fill(0);
    assert_eq!(preimage, expected_preimage);
    // At least one byte outside the signature differs from zero so the
    // zeroing comparison above is meaningful.
    assert!(expected_preimage.iter().any(|byte| *byte != 0));
}

/// Encodes a CLOSE-shaped option region with a recognisable nonzero
/// "signature" occupying the final field.
fn options_close_with_signature_tail() -> Vec<u8> {
    let mut out = vec![0xEE; SIG_LEN];
    out.reverse();
    out
}

#[test]
fn plan128_delay_requested_parses_before_from_in_normative_order() {
    let destination = asymmetric_destination();
    let flags =
        i2pr_proto::streaming::StreamingFlags::new(FLAG_DELAY_REQUESTED | FLAG_FROM_INCLUDED)
            .unwrap();
    let options = StreamingOptions {
        delay_requested: Some(0x0102),
        from_destination: Some(destination.clone()),
        max_payload_size: None,
        signature: None,
    };
    let option_bytes = options.encode_with_placeholder(flags, 0).unwrap();
    // DELAY (2 bytes) precedes FROM (self-encoded destination).
    assert_eq!(&option_bytes[0..2], &0x0102_u16.to_be_bytes());

    let builder = StreamingPacketBuilder {
        send_stream_id: 1,
        receive_stream_id: 2,
        sequence_num: 0,
        ack_through: 0,
        nacks: Vec::new(),
        resend_delay: 0,
        flags,
        option_bytes,
        payload: b"payload-after-options".to_vec(),
    };
    let wire = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
    let (packet, _) = decode_streaming_packet(
        &wire,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::anonymous(),
    )
    .unwrap();
    assert_eq!(packet.options.delay_requested, Some(0x0102));
    assert!(packet.options.from_destination.is_some());
    assert_eq!(packet.payload, b"payload-after-options");
}

#[test]
fn plan128_trailing_option_garbage_is_rejected_fail_closed() {
    let flags = i2pr_proto::streaming::StreamingFlags::new(FLAG_MAX_PACKET_SIZE_INCLUDED).unwrap();
    let options = StreamingOptions {
        delay_requested: None,
        from_destination: None,
        max_payload_size: Some(1200),
        signature: None,
    };
    let mut option_bytes = options.encode_with_placeholder(flags, 0).unwrap();
    option_bytes.push(0xFF); // unparsed trailing byte
    let builder = StreamingPacketBuilder {
        send_stream_id: 1,
        receive_stream_id: 2,
        sequence_num: 0,
        ack_through: 0,
        nacks: Vec::new(),
        resend_delay: 0,
        flags,
        option_bytes,
        payload: Vec::new(),
    };
    let wire = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
    let error = decode_streaming_packet(
        &wire,
        StreamingReceiveLimit::default(),
        StreamingOptionDecodeContext::anonymous(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        i2pr_proto::streaming::StreamingPacketError::TrailingBytes
    ));
}

#[test]
fn plan128_header_peek_routes_without_parsing_options() {
    let destination = asymmetric_destination();
    let options = StreamingOptions {
        delay_requested: None,
        from_destination: Some(destination),
        max_payload_size: Some(DEFAULT_ADVERTISED_MAX_PAYLOAD),
        signature: None,
    };
    let option_bytes = options
        .encode_with_placeholder(
            i2pr_proto::streaming::StreamingFlags::new(INITIAL_SYN_FLAGS).unwrap(),
            SIG_LEN,
        )
        .unwrap();
    let nacks = encode_syn_replay_binding(&[9u8; 32]).to_vec();
    let builder = StreamingPacketBuilder::new_initial_syn(0, 42, 0, option_bytes, nacks).unwrap();
    let wire = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
    let peek = peek_streaming_header(&wire).unwrap();
    assert_eq!(peek.send_stream_id, 0);
    assert_eq!(peek.receive_stream_id, 42);
    assert_eq!(peek.flags_bits, INITIAL_SYN_FLAGS);
}

#[test]
fn plan128_install_signature_rejects_nonzero_placeholder_tail() {
    let mut wire = vec![0xAB_u8; 40];
    wire[..36].copy_from_slice(&[0u8; 36]);
    // Tail four bytes are nonzero: installation must fail closed.
    let error = install_packet_signature(&mut wire, &[1, 2, 3, 4]).unwrap_err();
    assert!(matches!(
        error,
        i2pr_proto::streaming::StreamingPacketError::SignatureInvalid
    ));
}
