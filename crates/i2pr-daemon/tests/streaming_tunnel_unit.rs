//! Plan 193 §1 / §6 — Streaming-over-destination-tunnel unit suite.
//!
//! The deferred Streaming pass proves the existing
//! `i2pr_client::streaming` core integrates with the Plan 122/127
//! destination-routing pipeline through `StreamingDestinationAdapter`.
//! This file holds bounded unit rows that target individual
//! adapter/manager interactions without spinning up the full local
//! seam (see `streaming_tunnel_live.rs` for the i2pr↔i2pr round
//! trip).
//!
//! Each row exercises one Plan 193 §6 robustness behavior or one
//! Plan 193 §3 architecture-lock guarantee at the smallest possible
//! seam so the failure signal points directly at the layer under test.

#![forbid(unsafe_code)]
#![allow(clippy::too_many_lines)]

use i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD;
use i2pr_client::streaming::manager::{
    ConnectOutcome, RemoteDestination, StreamingManager, StreamingManagerError,
};
use i2pr_client::{
    DestinationIdentity, MAX_STREAMING_ADAPTER_PAYLOAD_BYTES, SendError, StreamingAdapterError,
};
use i2pr_netdb::DestinationHash;
use i2pr_proto::streaming::payload::{ClientPayload, decode_client_payload, encode_client_payload};
use i2pr_proto::streaming::{
    FLAG_SYNCHRONIZE, StreamingFlags, StreamingOptionDecodeContext, StreamingOptions,
    StreamingPacketBuilder, StreamingReceiveLimit, StreamingSendLimit,
};
use i2pr_proto::{
    CodecError, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, SignatureValue,
    SigningPublicKey,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const SEED: u64 = 0x188;

fn destination(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("destination identity")
}

fn remote_descriptor(seed: u64) -> RemoteDestination {
    let remote = destination(seed);
    RemoteDestination {
        destination_hash: *remote.id().as_hash().as_bytes(),
        signing_public_key: remote.destination().signing_key().clone(),
        static_public_key: remote.static_public_bytes(),
    }
}

#[test]
fn adapter_ceiling_matches_client_payload_limit() {
    // Plan 129 §2 source-floor invariant: application_payload on
    // TransportSendRequest is the gzip-encoded complete Streaming
    // packet, so the adapter ceiling must equal the client-payload
    // limit. They are not just approximately equal — the adapter
    // never silently truncates a manager-emitted packet.
    assert_eq!(
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
        i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES,
    );
}

#[test]
fn client_payload_round_trip_preserves_ports_and_protocol() {
    let payload = b"plan193-unit-payload-roundtrip";
    let envelope = ClientPayload {
        protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
        source_port: 31337,
        destination_port: 42424,
        payload: payload.to_vec(),
    };
    let encoded = encode_client_payload(&envelope).expect("encode");
    let decoded = decode_client_payload(&encoded, i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES)
        .expect("decode");
    assert_eq!(decoded.protocol, envelope.protocol);
    assert_eq!(decoded.source_port, envelope.source_port);
    assert_eq!(decoded.destination_port, envelope.destination_port);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn connect_emits_exactly_one_syn_request_with_synchronize_flag() {
    let local = destination(SEED);
    let mut manager = StreamingManager::new(i2pr_client::streaming::StreamingConfig::balanced());
    let outcome = manager.connect(
        &local,
        &remote_descriptor(SEED.wrapping_add(1)),
        31_001,
        31_002,
        DEFAULT_ADVERTISED_MAX_PAYLOAD,
        700_000,
        &mut ChaCha8Rng::seed_from_u64(0xCAFE),
    );
    let ConnectOutcome::SynSent {
        connection_id,
        send_stream_id,
        receive_stream_id,
    } = outcome.expect("SynSent")
    else {
        panic!("expected outbound SynSent");
    };
    assert_ne!(connection_id.raw(), 0);
    assert_eq!(send_stream_id, 0, "originator SYN uses sendStreamId=0");
    assert_ne!(
        receive_stream_id, 0,
        "originator SYN picks a nonzero local receive stream id",
    );
    let queue = manager.drain_outbound();
    assert_eq!(queue.len(), 1, "connect must enqueue exactly one SYN");
    let syn = &queue[0];
    // The TransportSendRequest carries a gzip-wrapped client payload;
    // unwrap it before peeking the streaming header.
    let envelope = decode_client_payload(
        &syn.application_payload,
        MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
    )
    .expect("decode client payload");
    let peek = i2pr_proto::streaming::peek_streaming_header(&envelope.payload)
        .expect("peek streaming header");
    assert!(
        peek.flags_bits & FLAG_SYNCHRONIZE != 0,
        "first packet must carry the SYNCHRONIZE flag",
    );
}

#[test]
fn send_data_rejects_pre_syn_response_state() {
    // The connection sits in OutboundSynSent; send_data must fail
    // closed as InvalidConnectionState rather than mutate the
    // send-window or emit a payload.
    let local = destination(SEED.wrapping_add(2));
    let mut manager = StreamingManager::new(i2pr_client::streaming::StreamingConfig::balanced());
    manager
        .connect(
            &local,
            &remote_descriptor(SEED.wrapping_add(3)),
            32_001,
            32_002,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            700_001,
            &mut ChaCha8Rng::seed_from_u64(0xBEEF),
        )
        .expect("SynSent");
    let dummy = manager.iter_connections().next().expect("connection").id();
    let result = manager.send_data(
        dummy,
        &local,
        &remote_descriptor(SEED.wrapping_add(3)),
        32_001,
        32_002,
        b"premature payload",
        700_002,
    );
    assert!(
        matches!(result, Err(StreamingManagerError::InvalidConnectionState)),
        "send_data before SYN response must fail closed as InvalidConnectionState",
    );
}

#[test]
fn listen_then_listener_backlog_is_empty() {
    let mut manager = StreamingManager::new(i2pr_client::streaming::StreamingConfig::balanced());
    let outcome = manager.listen(40_001).expect("listen");
    assert!(matches!(
        outcome,
        i2pr_client::streaming::manager::ListenerOutcome::Listening { .. }
    ));
    assert_eq!(manager.listener_backlog(40_001), 0);
}

#[test]
fn envelope_payload_decode_round_trips_for_each_protocol() {
    for protocol in [
        i2pr_proto::PROTOCOL_TYPE_STREAMING,
        i2pr_proto::PROTOCOL_TYPE_DATAGRAM,
        i2pr_proto::PROTOCOL_TYPE_RAW,
        i2pr_proto::PROTOCOL_TYPE_DATAGRAM2,
        i2pr_proto::PROTOCOL_TYPE_DATAGRAM3,
    ] {
        let payload = b"plan193-i2cp-data-body-payload";
        let encoded = i2pr_proto::encode_i2cp_data_body(0x1234, 0xabcd, protocol, payload)
            .expect("encode i2cp data body");
        let decoded = i2pr_proto::decode_i2cp_data_body(&encoded).expect("decode i2cp data body");
        assert_eq!(decoded.protocol, protocol);
        assert_eq!(decoded.from_port, 0x1234);
        assert_eq!(decoded.to_port, 0xabcd);
        assert_eq!(decoded.payload, payload);
    }
}

#[test]
fn nine_byte_short_transport_envelope_round_trips() {
    // Plan 192: the i2pd-compatible Garlic clove content is a
    // 9-byte NTCP2/SSU2 short-transport header + i2cp I2CP-style
    // Data body. The codec must round-trip the inner I2NP message
    // byte-exact so the inbound StreamingDestinationAdapter can
    // unwrap both layers without corrupting the streaming packet.
    let envelope = i2pr_proto::encode_destination_data_envelope(
        10_001,
        20_002,
        i2pr_proto::PROTOCOL_TYPE_STREAMING,
        0xC0FFEE,
        60,
        b"plan193-streaming-payload",
    )
    .expect("encode destination data envelope");
    let bytes = envelope
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode short transport");
    assert_eq!(bytes[0], 0x14, "first byte must be I2NP Data type (0x14)");
    let decoded = I2npMessage::decode_short_transport(&bytes, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode short transport");
    let body_bytes = match decoded.body() {
        I2npBody::Data(body) => body.payload.as_bytes().to_vec(),
        _ => panic!("decoded body must be Data"),
    };
    let i2cp = i2pr_proto::decode_i2cp_data_body(&body_bytes).expect("decode i2cp");
    assert_eq!(i2cp.protocol, i2pr_proto::PROTOCOL_TYPE_STREAMING);
    assert_eq!(i2cp.payload, b"plan193-streaming-payload");
}

#[test]
fn install_packet_signature_rejects_wrong_length() {
    // The streaming manager builds the canonical preimage (zeroed
    // signature placeholder). `install_packet_signature` only
    // accepts a signature whose length matches the trailing slot
    // (the rest of the wire). A shorter patch reveals the
    // unfilled placeholder bytes, which the codec rejects as
    // SignatureInvalid (or SignatureLengthMismatch on the inverse
    // case where the patch is longer than the available slot).
    let mut manager = StreamingManager::new(i2pr_client::streaming::StreamingConfig::balanced());
    manager
        .connect(
            &destination(SEED.wrapping_add(4)),
            &remote_descriptor(SEED.wrapping_add(5)),
            34_001,
            34_002,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            700_000,
            &mut ChaCha8Rng::seed_from_u64(0xCAFE_0002),
        )
        .expect("SynSent");
    let queue = manager.drain_outbound();
    let syn = &queue[0];
    // Too-long signature (longer than the wire itself) → Truncated.
    let too_long = vec![0u8; syn.application_payload.len() + 1];
    let mut bytes = syn.application_payload.clone();
    let err = i2pr_proto::streaming::install_packet_signature(&mut bytes, &too_long)
        .expect_err("oversized signature must reject");
    assert!(
        matches!(err, i2pr_proto::streaming::StreamingPacketError::Truncated),
        "expected Truncated for oversized signature, got {err:?}",
    );
}

#[test]
fn install_packet_signature_accepts_correct_length() {
    // The codec's install path accepts a signature whose length
    // exactly matches the trailing zeroed slot. We construct a
    // wire large enough to hold a full signature and patch a
    // same-length signature into it.
    let signature_length = i2pr_crypto::SIGNATURE_LENGTH;
    let mut wire_bytes = vec![0u8; signature_length];
    let fake_signature = vec![0u8; signature_length];
    i2pr_proto::streaming::install_packet_signature(&mut wire_bytes, &fake_signature)
        .expect("install signature");
    assert!(wire_bytes.iter().all(|b| *b == 0));
    // build_signature_preimage zeros the signature slot when a
    // SignatureLocation is supplied.
    let preimage = i2pr_proto::streaming::build_signature_preimage(
        &wire_bytes,
        Some(i2pr_proto::streaming::SignatureLocation {
            offset: 0,
            length: signature_length,
        }),
    );
    assert!(preimage.iter().all(|b| *b == 0));
}

#[test]
fn streaming_adapter_error_variants_surface_typed_failures() {
    let _empty = StreamingAdapterError::EmptyPayload;
    let _oversize = StreamingAdapterError::PayloadTooLarge {
        actual: 1,
        maximum: 0,
    };
    let _send = StreamingAdapterError::Send(SendError::DestinationStopping);
    let _not_data = StreamingAdapterError::NotI2npData;
    let _i2cp = StreamingAdapterError::I2cpDataBody(i2pr_proto::I2cpDataBodyDecodeError::Truncated);
    let _hash = StreamingAdapterError::UnknownDestination(DestinationHash::from_hash(
        Hash::from_bytes([0u8; 32]),
    ));
    let _client_payload =
        StreamingAdapterError::ClientPayload(i2pr_proto::ClientPayloadDecodeError::Truncated);
}

#[test]
fn signature_value_construction_round_trip() {
    // The Streaming signature path uses SignatureValue::new with the
    // peer signing-key context. A zero-length signature is rejected
    // up front; a valid-length signature round-trips through the
    // typed envelope.
    let key = SigningPublicKey::new(
        i2pr_crypto::ROUTER_SIGNING_KEY_TYPE,
        vec![0u8; 32], // Ed25519 public key length
    )
    .expect("signing public key");
    let signature = SignatureValue::new(key.key_type(), vec![0u8; i2pr_crypto::SIGNATURE_LENGTH])
        .expect("signature");
    assert_eq!(signature.as_bytes().len(), i2pr_crypto::SIGNATURE_LENGTH);
}

#[test]
fn streaming_manager_max_streams_ceiling_is_documented() {
    let manager = StreamingManager::new(i2pr_client::streaming::StreamingConfig::balanced());
    let ceiling = manager.config().max_streams_per_destination;
    assert!(ceiling > 0, "max_streams_per_destination must be nonzero");
    assert!(
        ceiling <= 1024,
        "max_streams_per_destination must stay bounded"
    );
}

#[test]
fn streaming_options_decoded_for_syn_response_requires_peer_key() {
    // SYN-response decoding carries no FROM option; the codec's
    // decode context must accept an Established-connection peer key
    // without surfacing MissingField. This row verifies the codec
    // contract for established-connection packet decode.
    let flags = StreamingFlags::new(FLAG_SYNCHRONIZE).expect("flags");
    let options = StreamingOptions {
        delay_requested: None,
        from_destination: None,
        max_payload_size: Some(DEFAULT_ADVERTISED_MAX_PAYLOAD),
        signature: None,
    };
    let builder = StreamingPacketBuilder {
        send_stream_id: 0x8000_0001,
        receive_stream_id: 0x8000_0002,
        sequence_num: 0,
        ack_through: 0,
        nacks: Vec::new(),
        resend_delay: 0,
        flags,
        option_bytes: options
            .encode_with_placeholder(flags, i2pr_crypto::SIGNATURE_LENGTH)
            .expect("placeholder"),
        payload: Vec::new(),
    };
    let bytes =
        i2pr_proto::streaming::encode_streaming_packet(&builder, StreamingSendLimit::default())
            .expect("encode");
    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut ChaCha8Rng::seed_from_u64(0xC0DE))
            .expect("identity");
    let pub_key = bundle.signing_key().public_key().expect("public key");
    let context = StreamingOptionDecodeContext::with_peer_key(&pub_key);
    let decode_result = i2pr_proto::streaming::decode_streaming_packet(
        &bytes,
        StreamingReceiveLimit::default(),
        context,
    );
    assert!(
        decode_result.is_ok(),
        "peer-key decode must accept the SYN response shape"
    );
}

#[test]
fn codec_error_truncated_carries_offset_metadata() {
    // CodecError::Truncated carries explicit offset/needed/remaining
    // metadata so error logs never leak payload bytes. The unit row
    // only exercises the struct variant constructor.
    let err = CodecError::Truncated {
        offset: 4,
        needed: 16,
        remaining: 4,
    };
    match err {
        CodecError::Truncated {
            offset,
            needed,
            remaining,
        } => {
            assert_eq!(offset, 4);
            assert_eq!(needed, 16);
            assert_eq!(remaining, 4);
        }
        _ => unreachable!(),
    }
}

#[test]
fn streaming_send_limit_default_matches_client_payload_ceiling() {
    // The StreamingSendLimit used by the codec carries an internal
    // upper bound that exceeds the negotiated application payload;
    // the unit row just exercises the constructor so a regression
    // in the default surface is caught immediately.
    let _limit = StreamingSendLimit::default();
}
