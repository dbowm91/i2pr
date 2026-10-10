//! Plan 368 contract evidence for `i2pr-app-manager-proto`.
//!
//! These are black-box tests against the public contract surface only. They
//! cover handshake golden bytes, strict directional separation, unknown/duplicate
//! field rejection, truncation, oversize, max+1, handle validation, request/reply
//! correlation, bounded accounting, and a deterministic no-panic fuzz smoke.

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, EffectiveGrant, Frame, FrameKind, HANDSHAKE_MAGIC, Handshake,
    MAX_CONTROL_BYTES, MAX_DATA_FRAME_BYTES, MAX_INFLIGHT_REQUESTS, MAX_MANAGER_SESSIONS,
    MAX_SERVICE_STREAMS_PER_SESSION, ManagerError, ManagerErrorCode, ManagerGatewayLimits,
    ManagerProtocolError, ManagerRole, ManagerScopeLimits, ManagerService, ManagerServiceStreamId,
    ManagerSessionId, ManagerToDaemonMessage, ServiceEndReason, decode_daemon_to_manager_control,
    decode_manager_to_daemon_control, encode_daemon_to_manager_control,
    encode_manager_to_daemon_control,
};
use i2pr_app_proto::{AppId, Capability, PublisherId, RequestId};

fn principal(instance: u128) -> i2pr_app_manager_proto::ManagerPrincipal {
    i2pr_app_manager_proto::ManagerPrincipal {
        app_id: AppId::parse("example.client").expect("app id"),
        instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(instance),
        publisher_id: Some(PublisherId::parse("example.publisher").expect("publisher")),
    }
}

fn handshake() -> Handshake {
    Handshake {
        role: ManagerRole::Manager,
        major: i2pr_app_manager_proto::MANAGER_PROTOCOL_MAJOR,
        minor: i2pr_app_manager_proto::MANAGER_PROTOCOL_MINOR,
    }
}

fn request_id(value: u32) -> RequestId {
    RequestId::new(value).expect("non-zero request id")
}

fn session(value: u64) -> ManagerSessionId {
    ManagerSessionId::new(value).expect("session handle")
}

fn stream(value: u64) -> ManagerServiceStreamId {
    ManagerServiceStreamId::new(value).expect("stream handle")
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

#[test]
fn handshake_golden_bytes_are_frozen_and_version_independent() {
    let encoded = handshake().encode();
    assert_eq!(
        encoded,
        [b'I', b'2', b'P', b'M', 1, 0, 1, 0, 0,],
        "handshake golden bytes must stay frozen for a language-neutral protocol"
    );
    assert_eq!(&encoded[..4], &HANDSHAKE_MAGIC);
    assert_ne!(
        &HANDSHAKE_MAGIC,
        &i2pr_app_proto::HANDSHAKE_MAGIC,
        "manager and application protocol magic must never collide"
    );
    assert_eq!(Handshake::decode(&encoded).expect("decode"), handshake());
}

#[test]
fn handshake_rejects_wrong_magic_version_role_length_and_reserved_bytes() {
    let base = handshake().encode();

    let mut wrong_magic = base;
    wrong_magic[0] = b'X';
    assert_eq!(
        Handshake::decode(&wrong_magic),
        Err(ManagerProtocolError::BadMagic)
    );

    // The application protocol magic must not be accepted as a manager.
    let mut app_magic = base;
    app_magic[..4].copy_from_slice(&i2pr_app_proto::HANDSHAKE_MAGIC);
    assert_eq!(
        Handshake::decode(&app_magic),
        Err(ManagerProtocolError::BadMagic)
    );

    let mut wrong_major = base;
    wrong_major[4] = 2;
    assert_eq!(
        Handshake::decode(&wrong_major),
        Err(ManagerProtocolError::UnsupportedVersion)
    );

    let mut wrong_role = base;
    wrong_role[6] = 2;
    assert_eq!(
        Handshake::decode(&wrong_role),
        Err(ManagerProtocolError::RoleMismatch)
    );

    let mut reserved = base;
    reserved[8] = 1;
    assert_eq!(
        Handshake::decode(&reserved),
        Err(ManagerProtocolError::MalformedFrame)
    );

    assert_eq!(
        Handshake::decode(&base[..8]),
        Err(ManagerProtocolError::TruncatedFrame)
    );
    assert_eq!(
        Handshake::decode(&[0_u8; 9]),
        Err(ManagerProtocolError::BadMagic)
    );

    // A compatible later minor version is still admitted; the major is the gate.
    let mut later_minor = base;
    later_minor[5] = 7;
    assert_eq!(
        Handshake::decode(&later_minor)
            .expect("compatible minor")
            .minor,
        7
    );
}

// ---------------------------------------------------------------------------
// Directional control separation
// ---------------------------------------------------------------------------

#[test]
fn control_vocabulary_is_strictly_directional() {
    let request = ManagerToDaemonMessage::Health {
        request_id: request_id(1),
    };
    let encoded = encode_manager_to_daemon_control(&request).expect("encode request");
    assert_eq!(
        decode_manager_to_daemon_control(&encoded).expect("decode request"),
        request
    );
    // The daemon-side decoder must refuse a manager request outright.
    assert_eq!(
        decode_daemon_to_manager_control(&encoded),
        Err(ManagerProtocolError::InvalidControl)
    );

    let reply = DaemonToManagerMessage::HealthStatus {
        request_id: request_id(1),
        state: "running".to_owned(),
        detail: None,
    };
    let encoded = encode_daemon_to_manager_control(&reply).expect("encode reply");
    assert_eq!(
        decode_daemon_to_manager_control(&encoded).expect("decode reply"),
        reply
    );
    assert_eq!(
        decode_manager_to_daemon_control(&encoded),
        Err(ManagerProtocolError::InvalidControl)
    );
}

#[test]
fn handshake_direction_guards_reject_the_wrong_role() {
    // The handshake carries the manager role, and both direction guards require
    // it; a future administrator role must not be able to reuse this codec.
    assert!(handshake().decode_manager_to_daemon(b"{}").is_err());
    assert!(handshake().decode_daemon_to_manager(b"{}").is_err());
}

#[test]
fn unknown_and_duplicate_fields_are_rejected() {
    let unknown = br#"{"type":"health","request_id":1,"surprise":true}"#;
    assert_eq!(
        decode_manager_to_daemon_control(unknown),
        Err(ManagerProtocolError::InvalidControl)
    );

    let duplicate = br#"{"type":"health","request_id":1,"request_id":2}"#;
    assert_eq!(
        decode_manager_to_daemon_control(duplicate),
        Err(ManagerProtocolError::InvalidControl)
    );

    // An unrecognised message type is refused rather than defaulted.
    assert_eq!(
        decode_manager_to_daemon_control(br#"{"type":"grant","request_id":1}"#),
        Err(ManagerProtocolError::InvalidControl)
    );
    assert_eq!(
        decode_daemon_to_manager_control(br#"{"type":"grant_revoked","request_id":1}"#),
        Err(ManagerProtocolError::InvalidControl)
    );

    // Trailing bytes are not consumed: a control frame decodes exactly.
    let mut trailing = br#"{"type":"health","request_id":1}"#.to_vec();
    trailing.push(b' ');
    assert_eq!(
        decode_manager_to_daemon_control(&trailing),
        Err(ManagerProtocolError::InvalidControl)
    );

    // Duplicates nested inside an object are refused too, not just at the top
    // level. This is what makes the strict scan load-bearing: without it these
    // two payloads decode identically and are indistinguishable on the wire.
    let nested =
        br#"{"type":"rejected","request_id":1,"error":{"code":"internal","code":"conflict"}}"#;
    assert_eq!(
        decode_daemon_to_manager_control(nested),
        Err(ManagerProtocolError::InvalidControl)
    );
    // The unambiguous equivalent still decodes, proving the scan is not simply
    // rejecting every `rejected` payload.
    let unambiguous = br#"{"type":"rejected","request_id":1,"error":{"code":"internal"}}"#;
    assert!(decode_daemon_to_manager_control(unambiguous).is_ok());

    // A duplicate array element is legal JSON and must stay legal.
    let repeated_value = br#"{"type":"create_session","request_id":1,"principal":{"app_id":"a.b","instance_id":1,"publisher_id":null},"effective_capabilities":[{"capability":"sam"},{"capability":"sam"}],"limits":{"max_connections":4}}"#;
    assert!(decode_manager_to_daemon_control(repeated_value).is_err());
}

#[test]
fn oversized_control_payloads_are_rejected_before_parsing() {
    let oversized = vec![b'{'; MAX_CONTROL_BYTES + 1];
    assert_eq!(
        decode_manager_to_daemon_control(&oversized),
        Err(ManagerProtocolError::LimitExceeded("control"))
    );
    assert_eq!(
        decode_daemon_to_manager_control(&oversized),
        Err(ManagerProtocolError::LimitExceeded("control"))
    );
}

#[test]
fn session_create_validates_limits_and_distinct_grants() {
    let ok = ManagerToDaemonMessage::CreateSession {
        request_id: request_id(1),
        principal: principal(1),
        effective_capabilities: vec![
            EffectiveGrant {
                capability: Capability::Sam,
            },
            EffectiveGrant {
                capability: Capability::I2cp,
            },
        ],
        limits: ManagerGatewayLimits::new(8).expect("limits"),
    };
    assert!(encode_manager_to_daemon_control(&ok).is_ok());

    // A duplicate grant is ambiguous authority and must not round-trip.
    let duplicate = ManagerToDaemonMessage::CreateSession {
        request_id: request_id(1),
        principal: principal(1),
        effective_capabilities: vec![
            EffectiveGrant {
                capability: Capability::Sam,
            },
            EffectiveGrant {
                capability: Capability::Sam,
            },
        ],
        limits: ManagerGatewayLimits::new(8).expect("limits"),
    };
    assert_eq!(
        encode_manager_to_daemon_control(&duplicate),
        Err(ManagerProtocolError::InvalidControl)
    );

    // Zero connections and above-ceiling connections are both refused rather
    // than silently clamped.
    assert_eq!(
        ManagerGatewayLimits::new(0),
        Err(ManagerProtocolError::LimitExceeded("gateway connections"))
    );
    assert_eq!(
        ManagerGatewayLimits::new(i2pr_app_manager_proto::MAX_GATEWAY_CONNECTIONS + 1),
        Err(ManagerProtocolError::LimitExceeded("gateway connections"))
    );
    let oversize = ManagerToDaemonMessage::CreateSession {
        request_id: request_id(1),
        principal: principal(1),
        effective_capabilities: Vec::new(),
        limits: ManagerGatewayLimits {
            max_connections: i2pr_app_manager_proto::MAX_GATEWAY_CONNECTIONS + 1,
        },
    };
    assert!(encode_manager_to_daemon_control(&oversize).is_err());
}

#[test]
fn control_scoped_is_unrepresentable_and_typed_rejected() {
    // The enum cannot name it: only "sam" and "i2cp" are accepted spellings.
    for spelling in [
        br#""control_scoped""#.as_slice(),
        br#""sam""#.as_slice(),
        br#""i2cp""#.as_slice(),
        br#""brokered_tcp""#.as_slice(),
    ] {
        let message = format!(
            r#"{{"type":"open_service","request_id":1,"session":1,"service":{}}}"#,
            std::str::from_utf8(spelling).expect("ascii spelling")
        );
        let decoded = decode_manager_to_daemon_control(message.as_bytes());
        assert_eq!(
            decoded.is_ok(),
            matches!(spelling, b"\"sam\"" | b"\"i2cp\""),
            "only sam/i2cp may decode as an openable service: {message}"
        );
    }
    // ...and the spelling is still recognised so the refusal is typed.
    assert_eq!(
        ManagerService::parse("control_scoped"),
        Err(ManagerProtocolError::UnsupportedService)
    );
    assert_eq!(ManagerService::parse("sam"), Ok(ManagerService::Sam));
    assert_eq!(
        ManagerService::parse("sam_datagram"),
        Ok(ManagerService::SamDatagram)
    );
    assert_eq!(ManagerService::parse("i2cp"), Ok(ManagerService::I2cp));
    assert_eq!(
        ManagerService::parse("brokered_tcp"),
        Err(ManagerProtocolError::InvalidIdentifier)
    );
    // Nor is there any administrator vocabulary on this protocol at all.
    assert_eq!(ManagerService::Sam.required_capability(), Capability::Sam);
    assert_eq!(
        ManagerService::SamDatagram.required_capability(),
        Capability::Sam
    );
    assert_eq!(ManagerService::I2cp.required_capability(), Capability::I2cp);
}

#[test]
fn non_ascii_or_oversize_diagnostics_are_rejected() {
    let long = ManagerToDaemonMessage::ResetService {
        request_id: request_id(1),
        session: session(1),
        stream: stream(1),
        reason: "x".repeat(i2pr_app_manager_proto::MAX_DIAGNOSTIC_BYTES + 1),
    };
    assert_eq!(
        encode_manager_to_daemon_control(&long),
        Err(ManagerProtocolError::LimitExceeded("diagnostic"))
    );

    let non_ascii = ManagerToDaemonMessage::Shutdown {
        request_id: request_id(1),
        reason: "caf\u{e9}".to_owned(),
    };
    assert_eq!(
        encode_manager_to_daemon_control(&non_ascii),
        Err(ManagerProtocolError::LimitExceeded("diagnostic"))
    );
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

#[test]
fn frame_golden_layout_round_trips_and_enforces_kind_stream_discipline() {
    let control = Frame::control(br#"{"type":"health","request_id":1}"#.to_vec());
    let encoded = control.encode().expect("encode control");
    assert_eq!(&encoded[..4], &[1, 1, 0, 0]);
    assert_eq!(encoded[4..8], [0, 0, 0, 0], "control frames are unstreamed");
    let (decoded, consumed) = Frame::decode(&encoded).expect("decode control");
    assert_eq!(decoded, control);
    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded.kind, FrameKind::Control);

    let data = Frame::data(stream(7), vec![0xAA; 5]);
    let encoded = data.encode().expect("encode data");
    assert_eq!(encoded[1], 2);
    assert_eq!(encoded[4..8], 7_u32.to_be_bytes());
    let (decoded, _) = Frame::decode(&encoded).expect("decode data");
    assert_eq!(decoded, data);

    // A data frame on stream 0, or a control frame on a stream, is malformed.
    let mut bad_data = data.clone();
    bad_data.stream_id = 0;
    assert_eq!(bad_data.encode(), Err(ManagerProtocolError::MalformedFrame));
    let mut bad_control = control.clone();
    bad_control.stream_id = 3;
    assert_eq!(
        bad_control.encode(),
        Err(ManagerProtocolError::MalformedFrame)
    );
}

#[test]
fn frame_truncation_oversize_and_reserved_bytes_are_rejected() {
    let control = Frame::control(vec![0_u8; 8]).encode().expect("encode");

    assert_eq!(
        Frame::decode(&control[..11]),
        Err(ManagerProtocolError::TruncatedFrame)
    );
    assert_eq!(
        Frame::decode(&control[..control.len() - 1]),
        Err(ManagerProtocolError::TruncatedFrame)
    );

    let mut wrong_version = control.clone();
    wrong_version[0] = 9;
    assert_eq!(
        Frame::decode(&wrong_version),
        Err(ManagerProtocolError::UnsupportedFrame)
    );

    let mut wrong_kind = control.clone();
    wrong_kind[1] = 7;
    assert_eq!(
        Frame::decode(&wrong_kind),
        Err(ManagerProtocolError::UnsupportedFrame)
    );

    let mut reserved = control.clone();
    reserved[3] = 1;
    assert_eq!(
        Frame::decode(&reserved),
        Err(ManagerProtocolError::MalformedFrame)
    );

    // An oversize declared length is refused before any allocation or copy.
    let mut oversize = control.clone();
    oversize[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        Frame::decode(&oversize),
        Err(ManagerProtocolError::LimitExceeded("frame payload"))
    );

    assert_eq!(
        Frame::control(vec![0_u8; MAX_CONTROL_BYTES + 1]).encode(),
        Err(ManagerProtocolError::LimitExceeded("frame payload"))
    );
    assert_eq!(
        Frame::data(stream(1), vec![0_u8; MAX_DATA_FRAME_BYTES + 1]).encode(),
        Err(ManagerProtocolError::LimitExceeded("frame payload"))
    );
    // A declared length must never overflow the checked header addition.
    let mut near_overflow = control.clone();
    near_overflow[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    near_overflow[1] = 1;
    assert!(Frame::decode(&near_overflow).is_err());
}

/// Regression: the manager protocol must carry the 128-bit launch-instance id so
/// a `create_session` request actually round-trips.
///
/// `AppInstanceId` serialises as a JSON **number** and `serde_json` cannot
/// deserialize `u128`, so routing `create_session` through `AppPrincipal` made
/// every real session create undecodable. The contract carries it as bounded
/// decimal digits instead.
#[test]
fn manager_principal_round_trips_including_the_widest_instance_id() {
    for instance in [1_u128, 7, u64::MAX as u128, u128::MAX] {
        let original = principal(instance);
        let message = ManagerToDaemonMessage::CreateSession {
            request_id: request_id(1),
            principal: original.clone(),
            effective_capabilities: Vec::new(),
            limits: ManagerGatewayLimits::new(4).expect("limits"),
        };
        let encoded = encode_manager_to_daemon_control(&message).expect("encode");
        let decoded = decode_manager_to_daemon_control(&encoded).expect("decode");
        assert_eq!(
            decoded, message,
            "a real create_session must round-trip at instance id {instance}"
        );
        let ManagerToDaemonMessage::CreateSession { principal, .. } = decoded else {
            unreachable!("just constructed")
        };
        let converted = principal
            .to_app_principal()
            .expect("convert to AppPrincipal");
        assert_eq!(converted.instance_id.get(), instance);
    }

    // The id is canonical: one id, exactly one spelling.
    let widest = principal(u128::MAX);
    let request = ManagerToDaemonMessage::CreateSession {
        request_id: request_id(1),
        principal: widest,
        effective_capabilities: Vec::new(),
        limits: ManagerGatewayLimits::new(4).expect("limits"),
    };
    let encoded = encode_manager_to_daemon_control(&request).expect("encode");
    assert!(
        String::from_utf8_lossy(&encoded).contains(&format!("\"{}\"", u128::MAX)),
        "the widest id must appear verbatim as its 39 decimal digits"
    );
    // A value just past u128::MAX is refused rather than silently truncated.
    assert_eq!(
        i2pr_app_manager_proto::ManagerInstanceId::try_from(String::from(
            "340282366920938463463374607431768211456",
        )),
        Err(ManagerProtocolError::InvalidIdentifier),
        "an out-of-range id must be refused, not truncated"
    );
}

#[test]
fn manager_principal_rejects_non_canonical_and_oversize_instance_ids() {
    for bad in ["", "0", "007", "-1", "1.0", "abc", "+1", " 1", "1 "] {
        let payload = format!(
            r#"{{"type":"create_session","request_id":1,"principal":{{"app_id":"a.b","instance_id":"{bad}","publisher_id":null}},"effective_capabilities":[],"limits":{{"max_connections":2}}}}"#
        );
        assert!(
            decode_manager_to_daemon_control(payload.as_bytes()).is_err(),
            "non-canonical instance id must be refused: {bad:?}"
        );
    }
    // Over-long decimal digits are refused before any numeric parse.
    let long = "9".repeat(i2pr_app_manager_proto::MAX_DECIMAL_DIGITS + 1);
    let payload = format!(
        r#"{{"type":"create_session","request_id":1,"principal":{{"app_id":"a.b","instance_id":"{long}","publisher_id":null}},"effective_capabilities":[],"limits":{{"max_connections":2}}}}"#
    );
    assert!(decode_manager_to_daemon_control(payload.as_bytes()).is_err());
}

// ---------------------------------------------------------------------------
// Correlation and bounded accounting
// ---------------------------------------------------------------------------

#[test]
fn reply_correlation_is_exact_and_notifications_are_uncorrelated() {
    for reply in [
        DaemonToManagerMessage::SessionOpened {
            request_id: request_id(4),
            session: session(1),
        },
        DaemonToManagerMessage::SessionClosed {
            request_id: request_id(4),
            session: session(1),
        },
        DaemonToManagerMessage::ServiceOpened {
            request_id: request_id(4),
            session: session(1),
            stream: stream(2),
        },
        DaemonToManagerMessage::ServiceClosed {
            request_id: request_id(4),
            session: session(1),
            stream: stream(2),
        },
        DaemonToManagerMessage::ServiceReset {
            request_id: request_id(4),
            session: session(1),
            stream: stream(2),
        },
        DaemonToManagerMessage::HealthStatus {
            request_id: request_id(4),
            state: "running".to_owned(),
            detail: None,
        },
        DaemonToManagerMessage::ShutdownAck {
            request_id: request_id(4),
        },
        DaemonToManagerMessage::Rejected {
            request_id: request_id(4),
            error: ManagerError {
                code: ManagerErrorCode::PermissionDenied,
                diagnostic: None,
            },
        },
    ] {
        assert_eq!(reply.correlation(), Some(request_id(4)));
        let encoded = encode_daemon_to_manager_control(&reply).expect("encode reply");
        assert_eq!(
            decode_daemon_to_manager_control(&encoded)
                .expect("decode reply")
                .correlation(),
            Some(request_id(4))
        );
    }

    for notification in [
        DaemonToManagerMessage::ServiceEnded {
            session: session(1),
            stream: stream(2),
            reason: ServiceEndReason::BackendClosed,
        },
        DaemonToManagerMessage::SessionEnded {
            session: session(1),
            reason: ServiceEndReason::SessionEnded,
        },
    ] {
        assert_eq!(notification.correlation(), None);
    }
}

#[test]
fn handles_reject_zero_and_malformed_values() {
    assert_eq!(
        ManagerSessionId::new(0),
        Err(ManagerProtocolError::InvalidHandle)
    );
    assert_eq!(
        ManagerServiceStreamId::new(0),
        Err(ManagerProtocolError::InvalidHandle)
    );
    // Wire spellings that cannot round-trip into a handle are refused, so a
    // fabricated `0`, negative, or non-numeric value never reaches the daemon.
    for payload in ["0", "-1", r#""1""#] {
        assert!(
            decode_manager_to_daemon_control(
                format!(r#"{{"type":"close_session","request_id":1,"session":{payload}}}"#)
                    .as_bytes()
            )
            .is_err(),
            "malformed handle must not decode: {payload}"
        );
    }

    // Handle zero is reserved for control frames, so a data frame can never be
    // ambiguous with an unstreamed control frame.
    assert_eq!(
        Frame::data(stream(1), vec![0_u8; 1])
            .encode()
            .map(|bytes| bytes[4..8].to_vec()),
        Ok(1_u32.to_be_bytes().to_vec())
    );
}

#[test]
fn scope_accounting_is_bounded_at_capacity_one_exact_and_max_plus_one() {
    let mut scope = ManagerScopeLimits::default();

    // Exact capacity, then max+1.
    for index in 1..=MAX_MANAGER_SESSIONS as u64 {
        scope
            .open_session(session(index))
            .expect("session within ceiling");
    }
    assert_eq!(scope.session_count(), MAX_MANAGER_SESSIONS);
    assert_eq!(
        scope.open_session(session(MAX_MANAGER_SESSIONS as u64 + 1)),
        Err(ManagerProtocolError::LimitExceeded("manager sessions"))
    );
    // A duplicate handle inside one scope is rejected, not re-counted.
    assert_eq!(
        scope.open_session(session(1)),
        Err(ManagerProtocolError::InvalidHandle)
    );

    // A single-session scope accepts exactly one, and the next is refused.
    let mut single = ManagerScopeLimits::default();
    single.open_session(session(1)).expect("first");
    assert_eq!(single.session_count(), 1);
    assert_eq!(
        single.close_session(session(1)),
        Ok(()),
        "close must release the slot"
    );
    assert_eq!(
        single.close_session(session(1)),
        Err(ManagerProtocolError::InvalidHandle),
        "a stale handle must fail deterministically"
    );
    // The freed slot is reusable, so repeated create/close returns to baseline.
    single.open_session(session(2)).expect("reuse freed slot");
    assert_eq!(single.session_count(), 1);
    assert!(!single.has_session(session(1)));

    // Repeated open/close returns the count to baseline on every path.
    for _ in 0..4 {
        let mut cycle = ManagerScopeLimits::default();
        cycle.open_session(session(9)).expect("open");
        cycle.close_session(session(9)).expect("close");
        assert_eq!(cycle.session_count(), 0);
        assert!(!cycle.has_session(session(9)));
    }

    // In-flight request ledger: exact capacity then max+1.
    let mut requests = ManagerScopeLimits::default();
    for index in 1..=MAX_INFLIGHT_REQUESTS as u32 {
        requests
            .register_request(request_id(index))
            .expect("register");
    }
    assert_eq!(
        requests.register_request(request_id(MAX_INFLIGHT_REQUESTS as u32 + 1)),
        Err(ManagerProtocolError::LimitExceeded("in-flight requests"))
    );
    requests.complete_request(request_id(1)).expect("complete");
    assert_eq!(
        requests.complete_request(request_id(1)),
        Err(ManagerProtocolError::UnmatchedRequest)
    );
    assert_eq!(requests.inflight_count(), MAX_INFLIGHT_REQUESTS - 1);
}

#[test]
fn declared_ceilings_match_the_plan_368_contract() {
    // These are frozen protocol facts asserted by the reference spec. Changing
    // one is a protocol change requiring a new plan.
    assert_eq!(MAX_SERVICE_STREAMS_PER_SESSION, 128);
    assert_eq!(MAX_MANAGER_SESSIONS, 32);
    assert_eq!(MAX_INFLIGHT_REQUESTS, 64);
    assert_eq!(MAX_CONTROL_BYTES, 16_384);
    assert_eq!(MAX_DATA_FRAME_BYTES, 65_536);
    assert_eq!(i2pr_app_manager_proto::FRAME_HEADER_BYTES, 12);
    assert_eq!(i2pr_app_manager_proto::MANAGER_PROTOCOL_MAJOR, 1);
}

// ---------------------------------------------------------------------------
// Fuzz smoke
// ---------------------------------------------------------------------------

/// Deterministic mutation sweep. The contract must never panic, never
/// allocate on an unbounded declared length, and always return a typed error.
#[test]
fn fuzz_smoke_never_panics_and_always_fails_closed() {
    let seeds: Vec<Vec<u8>> = vec![
        handshake().encode().to_vec(),
        Frame::control(br#"{"type":"health","request_id":1}"#.to_vec())
            .encode()
            .expect("encode seed"),
        Frame::data(stream(3), vec![1, 2, 3, 4])
            .encode()
            .expect("encode seed"),
        encode_manager_to_daemon_control(&ManagerToDaemonMessage::OpenService {
            request_id: request_id(1),
            session: session(1),
            service: ManagerService::Sam,
        })
        .expect("encode seed"),
        encode_daemon_to_manager_control(&DaemonToManagerMessage::ServiceEnded {
            session: session(1),
            stream: stream(2),
            reason: ServiceEndReason::BackendRejected,
        })
        .expect("encode seed"),
        b"{\"type\":\"create_session\"".to_vec(),
    ];

    // xorshift64* gives a reproducible sweep without an external RNG dependency.
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut accepted = 0_usize;
    let mut rejected = 0_usize;
    for round in 0..20_000 {
        let seed = &seeds[round % seeds.len()];
        let mut bytes = seed.clone();
        for _ in 0..(round % 5) {
            let index = (next() as usize) % bytes.len().max(1);
            if index < bytes.len() {
                bytes[index] = (next() & 0xFF) as u8;
            }
        }
        let truncation = (next() as usize) % (bytes.len() + 1);
        bytes.truncate(truncation);
        let suffix = (next() as usize) % 8;
        bytes.extend(std::iter::repeat_n((next() & 0xFF) as u8, suffix));

        if Handshake::decode(&bytes).is_ok() {
            accepted += 1;
        } else {
            rejected += 1;
        }
        if Frame::decode(&bytes).is_ok() {
            accepted += 1;
        } else {
            rejected += 1;
        }
        if decode_manager_to_daemon_control(&bytes).is_ok() {
            accepted += 1;
        } else {
            rejected += 1;
        }
        if decode_daemon_to_manager_control(&bytes).is_ok() {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }

    assert!(
        rejected > accepted,
        "a mutation sweep must reject far more than it accepts \
         (accepted={accepted} rejected={rejected})"
    );
}
