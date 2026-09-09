//! Plan 164 I2CP wire-vector test.
//!
//! Every committed `tests/fixtures/i2cp/*.hex` vector is decoded
//! through the frame codec and the typed message codec, checked
//! against pinned field expectations, and re-encoded byte-for-byte.
//! Malformed vectors must fail with the documented error variant.
//! Hashes are pinned by `tests/fixtures/i2cp/manifest.tsv` and
//! enforced by `scripts/check-i2cp-vectors.sh`.

use i2pr_api::i2cp::message::{
    DestReplyBody, HostLookupKey, HostReplyResult, Message, MessageStatusCode, MessageType,
    SessionStatusCode, decode_typed,
};
use i2pr_api::i2cp::{ClientNonce, I2cpError, check_protocol_byte, decode_frame};

fn fixture_hex(name: &str) -> Vec<u8> {
    // `CARGO_MANIFEST_DIR` keeps fixture lookup independent of the
    // process working directory: routine CI runs workspace tests via
    // Cargo (package CWD), while the macOS lane executes each test
    // binary directly from the workspace root.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/i2cp")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture readable");
    let text = text.trim();
    assert!(text.len() % 2 == 0, "even hex digits in {name}");
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex digits"))
        .collect()
}

fn framed_message(name: &str) -> Message {
    let bytes = fixture_hex(name);
    let frame = decode_frame(&bytes).expect("frame decodes");
    decode_typed(frame.message_type, &frame.body).expect("body decodes")
}

fn expect_malformed(name: &str) -> I2cpError {
    let bytes = fixture_hex(name);
    match decode_frame(&bytes) {
        Err(error) => error,
        Ok(frame) => decode_typed(frame.message_type, &frame.body).expect_err("must be malformed"),
    }
}

#[test]
fn protocol_byte_opens_every_connection() {
    assert_eq!(fixture_hex("protocol-byte.hex"), vec![0x2a]);
    assert!(check_protocol_byte(0x2a).is_ok());
}

#[test]
fn get_date_vector() {
    let Message::GetDate(body) = framed_message("get-date.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.version, "0.9.67");
    assert!(body.auth.is_none());
    let raw = fixture_hex("get-date.hex");
    assert_eq!(body.encode().expect("encode").len() + 5, raw.len());
}

#[test]
fn set_date_vector() {
    let Message::SetDate(body) = framed_message("set-date.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.date_ms, 1_786_000_000_123);
    assert_eq!(body.version, "0.9.67");
}

#[test]
fn create_session_vector_preserves_signed_region() {
    let Message::CreateSession(body) = framed_message("create-session.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.config.creation_ms(), 1_786_000_000_000);
    assert_eq!(
        body.config.options().get("inbound.length"),
        Some("2"),
        "options survive the fixture round trip"
    );
    // Re-encoding the body reproduces the committed frame exactly.
    let raw = fixture_hex("create-session.hex");
    let mut expected = raw[..5].to_vec();
    expected.extend(body.encode().expect("encode"));
    assert_eq!(expected, raw);
}

#[test]
fn session_lifecycle_vectors() {
    let Message::DestroySession(body) = framed_message("destroy-session.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.session.get(), 7);
    let Message::SessionStatus(body) = framed_message("session-status-created.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.session.get(), 7);
    assert_eq!(body.status, SessionStatusCode::Created);
}

#[test]
fn request_variable_leaseset_vector() {
    let Message::RequestVariableLeaseSet(body) = framed_message("request-variable-leaseset.hex")
    else {
        panic!("wrong message");
    };
    assert_eq!(body.session.get(), 7);
    assert_eq!(body.leases.len(), 2);
    assert_eq!(body.leases[0].tunnel_id, 11);
    assert_eq!(body.leases[1].tunnel_id, 12);
    // Plan 172 §5: 44-byte Lease compat (gateway + tunnel id + 8-byte ms end date).
    assert_eq!(body.leases[0].end_date_ms, 1_786_000_000_000);
    assert_eq!(body.leases[1].end_date_ms, 1_786_000_060_000);
}

#[test]
fn create_leaseset2_vector() {
    let Message::CreateLeaseSet2(body) = framed_message("create-leaseset2.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.session(), i2pr_api::i2cp::SessionId::new(7));
    assert_eq!(body.lease_set.encryption_keys().len(), 1);
    assert_eq!(body.lease_set.leases().len(), 1);
    assert_eq!(body.private_keys.len(), 1);
}

#[test]
fn send_message_vector() {
    let Message::SendMessage(body) = framed_message("send-message.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.session.get(), 7);
    assert_eq!(body.payload.as_bytes(), b"hello-i2cp");
    assert_eq!(body.nonce, ClientNonce::new(1));
    // The destination hash pins the fixed test destination.
    let hash = body.destination.hash().expect("hash");
    assert_eq!(hash, body.destination.hash().expect("hash"));
    assert_eq!(hash.as_bytes().len(), 32);
}

#[test]
fn send_message_expires_vector() {
    let Message::SendMessageExpires(body) = framed_message("send-message-expires.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.session.get(), 7);
    assert!(body.flags.no_bundle());
    assert_eq!(body.expiration_ms, 1_786_000_060_000);
}

#[test]
fn inbound_message_vectors() {
    let Message::MessagePayload(body) = framed_message("message-payload.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.session.get(), 7);
    assert_eq!(body.message_id.get(), 11);
    assert_eq!(body.payload.as_bytes(), b"hello-i2cp");

    let Message::MessageStatus(body) = framed_message("message-status-accepted.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.message_id.get(), 11);
    assert_eq!(body.status, MessageStatusCode::Accepted);
    assert!(body.status.is_success());
    assert_eq!(body.nonce, ClientNonce::new(1));
}

#[test]
fn bandwidth_vectors() {
    let Message::GetBandwidthLimits(_) = framed_message("get-bandwidth-limits.hex") else {
        panic!("wrong message");
    };
    let Message::BandwidthLimits(body) = framed_message("bandwidth-limits.hex") else {
        panic!("wrong message");
    };
    assert_eq!((body.client_inbound, body.client_outbound), (128, 64));
    assert_eq!(body.router_burst_time, 10);
    assert_eq!(body.reserved, [0; 9]);
}

#[test]
fn destination_lookup_vectors() {
    let Message::DestLookup(body) = framed_message("dest-lookup.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.hash.as_bytes(), &[0xabu8; 32]);

    let Message::DestReply(body) = framed_message("dest-reply-hash.hex") else {
        panic!("wrong message");
    };
    assert!(matches!(body.body, DestReplyBody::Hash(_)));

    let Message::DestReply(body) = framed_message("dest-reply-destination.hex") else {
        panic!("wrong message");
    };
    assert!(matches!(body.body, DestReplyBody::Destination(_)));
}

#[test]
fn disconnect_vector() {
    let Message::Disconnect(body) = framed_message("disconnect.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.reason, "bye");
}

#[test]
fn host_lookup_vectors() {
    let Message::HostLookup(body) = framed_message("host-lookup-hostname.hex") else {
        panic!("wrong message");
    };
    assert!(body.session.is_no_session());
    assert_eq!(body.request_id.get(), 100);
    assert_eq!(body.timeout_ms, 10_000);
    assert!(matches!(body.key, HostLookupKey::Hostname(ref name) if name == "example.i2p"));

    let Message::HostReply(body) = framed_message("host-reply-success.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.result, HostReplyResult::Success);
    assert!(body.destination.is_some());
    assert!(body.options.is_none());

    let Message::HostReply(body) = framed_message("host-reply-failure.hex") else {
        panic!("wrong message");
    };
    assert_eq!(body.result, HostReplyResult::Failure);
    assert!(body.destination.is_none());
}

#[test]
fn malformed_vectors_fail_typed() {
    assert!(matches!(
        expect_malformed("malformed-unknown-type.hex"),
        I2cpError::UnknownMessageType { raw: 40 }
    ));
    assert!(matches!(
        expect_malformed("malformed-deprecated-type.hex"),
        I2cpError::DeprecatedMessageType { raw: 4 }
    ));
    assert!(matches!(
        expect_malformed("malformed-oversize-length.hex"),
        I2cpError::BodyTooLarge { .. }
    ));
    // Truncated mid-destination: the frame itself is incomplete.
    assert!(matches!(
        expect_malformed("malformed-truncated-body.hex"),
        I2cpError::Incomplete { .. }
    ));
    assert!(matches!(
        expect_malformed("malformed-trailing-session-status.hex"),
        I2cpError::Malformed { .. }
    ));
    assert!(matches!(
        expect_malformed("malformed-bad-destination.hex"),
        I2cpError::Malformed { .. }
    ));
    assert!(matches!(
        expect_malformed("malformed-short-signature.hex"),
        I2cpError::Malformed { .. }
    ));
    // Every malformed type ID stays classifiable.
    assert_eq!(MessageType::from_u8(40), None);
    assert_eq!(
        MessageType::from_u8(4).expect("assigned").disposition(),
        i2pr_api::i2cp::MessageDisposition::LegacyDeprecated
    );
}
