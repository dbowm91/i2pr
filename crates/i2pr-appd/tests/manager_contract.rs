//! Plan 369 WP2 — `i2pr-appd` contract evidence.
//!
//! These drive the manager over an in-memory duplex transport, exactly as the
//! daemon will drive it over the inherited pipes. Nothing here reaches a
//! socket, a listener, a process, or a filesystem.

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, EffectiveGrant, Frame, FrameKind, HANDSHAKE_BYTES, Handshake,
    MANAGER_PROTOCOL_MAJOR, MANAGER_PROTOCOL_MINOR, ManagerErrorCode, ManagerGatewayLimits,
    ManagerInstanceId, ManagerPrincipal, ManagerProtocolError, ManagerRole, ManagerToDaemonMessage,
    decode_daemon_to_manager_control, encode_manager_to_daemon_control,
};
use i2pr_app_proto::{AppId, Capability, PublisherId, RequestId};
use i2pr_appd::{Appd, AppdError, AppdState, DuplexTransport, transition};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::time::{Duration, timeout};

/// Every await is bounded so a defect fails the test instead of hanging.
const DEADLINE: Duration = Duration::from_secs(10);

fn rid(value: u32) -> RequestId {
    RequestId::new(value).expect("nonzero request id")
}

fn principal(instance: u128) -> ManagerPrincipal {
    ManagerPrincipal {
        app_id: AppId::parse("example.client").expect("app id"),
        instance_id: ManagerInstanceId::new(instance),
        publisher_id: Some(PublisherId::parse("example.publisher").expect("publisher")),
    }
}

struct ManagerPeer {
    stream: DuplexStream,
}

impl ManagerPeer {
    fn new() -> (Self, DuplexStream) {
        let (mine, theirs) = tokio::io::duplex(64 * 1024);
        (Self { stream: mine }, theirs)
    }

    async fn read_handshake(&mut self) -> Result<(), String> {
        let mut bytes = [0_u8; HANDSHAKE_BYTES];
        self.stream
            .read_exact(&mut bytes)
            .await
            .map_err(|e| e.to_string())?;
        Handshake::decode(&bytes).map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn send(&mut self, message: &ManagerToDaemonMessage) -> Result<(), String> {
        let payload = encode_manager_to_daemon_control(message).map_err(|e| e.to_string())?;
        let frame = Frame::control(payload)
            .encode()
            .map_err(|e| e.to_string())?;
        self.stream
            .write_all(&frame)
            .await
            .map_err(|e| e.to_string())
    }

    async fn recv(&mut self) -> Result<DaemonToManagerMessage, String> {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = self
                .stream
                .read(&mut chunk)
                .await
                .map_err(|e| e.to_string())?;
            if read == 0 {
                return Err("manager transport closed".to_owned());
            }
            buffer.extend_from_slice(&chunk[..read]);
            match Frame::decode(&buffer) {
                Ok((frame, consumed)) => {
                    buffer.drain(..consumed);
                    return decode_daemon_to_manager_control(&frame.payload)
                        .map_err(|e| e.to_string());
                }
                Err(ManagerProtocolError::TruncatedFrame) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

#[tokio::test]
async fn handshake_is_the_frozen_manager_greeting() {
    let (mut peer, appd_side) = ManagerPeer::new();
    let (_reader, mut writer) = tokio::io::split(appd_side);
    let mut appd = Appd::new();
    assert_eq!(appd.state(), AppdState::Starting);

    appd.write_handshake(&mut writer).await.expect("handshake");
    // The handshake is what moves the manager out of `Starting`: the daemon may
    // only treat this service as ready once these nine bytes are out.
    assert_eq!(appd.state(), AppdState::Idle);
    peer.read_handshake().await.expect("peer handshake");
}

#[tokio::test]
async fn every_request_is_refused_with_a_typed_reason_and_no_authority_is_allocated() {
    let (mut peer, appd_side) = ManagerPeer::new();
    let (appd_reader, appd_writer) = tokio::io::split(appd_side);
    let transport = DuplexTransport::new(appd_reader, appd_writer);
    let driver = tokio::spawn(async move { Appd::new().run(transport).await });

    peer.read_handshake().await.expect("peer handshake");

    // A `create_session` that names a plausible principal and grant set. The
    // manager must still refuse it: Plan 369 has no launch authority owner, and
    // a wire message must never be able to manufacture one.
    peer.send(&ManagerToDaemonMessage::CreateSession {
        request_id: rid(1),
        principal: principal(7),
        effective_capabilities: vec![EffectiveGrant {
            capability: Capability::Sam,
        }],
        limits: ManagerGatewayLimits::new(4).expect("limits"),
    })
    .await
    .expect("send");

    let reply = timeout(DEADLINE, peer.recv())
        .await
        .expect("reply deadline")
        .expect("reply");
    match reply {
        DaemonToManagerMessage::Rejected { request_id, error } => {
            assert_eq!(request_id, rid(1));
            assert_eq!(error.code, ManagerErrorCode::UnsupportedOperation);
            assert!(
                error.diagnostic.unwrap_or_default().contains("authority"),
                "refusal must name the missing authority owner"
            );
        }
        other => panic!("create_session must be refused, got {other:?}"),
    }

    peer.send(&ManagerToDaemonMessage::Health { request_id: rid(2) })
        .await
        .expect("send");
    let reply = timeout(DEADLINE, peer.recv())
        .await
        .expect("reply deadline")
        .expect("reply");
    assert!(matches!(reply, DaemonToManagerMessage::Rejected { .. }));

    // Closing the transport is the manager's only shutdown path.
    drop(peer);
    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn a_data_frame_in_the_manager_direction_is_a_protocol_violation() {
    let (mut peer, appd_side) = ManagerPeer::new();
    let (appd_reader, appd_writer) = tokio::io::split(appd_side);
    let transport = DuplexTransport::new(appd_reader, appd_writer);
    let driver = tokio::spawn(async move { Appd::new().run(transport).await });

    peer.read_handshake().await.expect("peer handshake");

    // Manager->daemon has no data frame. Sending one must end the session
    // rather than be skipped as an unknown-but-harmless frame.
    let frame = Frame {
        kind: FrameKind::Data,
        stream_id: 1,
        payload: b"x".to_vec(),
    }
    .encode()
    .expect("encode");
    peer.stream.write_all(&frame).await.expect("write");

    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert_eq!(
        outcome,
        Err(AppdError::Protocol(ManagerProtocolError::MalformedFrame))
    );
}

#[tokio::test]
async fn an_oversize_declared_frame_length_is_refused_before_allocation() {
    let (mut peer, appd_side) = ManagerPeer::new();
    let (appd_reader, appd_writer) = tokio::io::split(appd_side);
    let transport = DuplexTransport::new(appd_reader, appd_writer);
    let driver = tokio::spawn(async move { Appd::new().run(transport).await });

    peer.read_handshake().await.expect("peer handshake");

    // version 1, kind 1 (control), two reserved zeros, stream id 0, and a
    // declared length far above the ceiling.
    let mut hostile = vec![1_u8, 1, 0, 0];
    hostile.extend_from_slice(&0_u32.to_be_bytes());
    hostile.extend_from_slice(&u32::MAX.to_be_bytes());
    peer.stream.write_all(&hostile).await.expect("write");

    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert!(
        matches!(outcome, Err(AppdError::Protocol(_))),
        "oversize declared length must fail closed, got {outcome:?}"
    );
}

#[tokio::test]
async fn duplicate_request_ids_are_refused_by_the_ledger() {
    // Scope note: this covers *duplicates*. A zero request id is not merely
    // refused here, it is unrepresentable — `RequestId::new(0)` fails in
    // `i2pr-app-proto`, so no wire message can carry one. `RequestLedger::admit`
    // re-checks zero as defense in depth below that invariant; this test does
    // not claim to reach it.
    let (mut peer, appd_side) = ManagerPeer::new();
    let (appd_reader, appd_writer) = tokio::io::split(appd_side);
    let transport = DuplexTransport::new(appd_reader, appd_writer);
    let driver = tokio::spawn(async move { Appd::new().run(transport).await });
    peer.read_handshake().await.expect("peer handshake");

    // Two identical in-flight ids are a protocol error, not two requests.
    peer.send(&ManagerToDaemonMessage::Health { request_id: rid(9) })
        .await
        .expect("send");
    let first = timeout(DEADLINE, peer.recv())
        .await
        .expect("deadline")
        .expect("reply");
    assert!(matches!(first, DaemonToManagerMessage::Rejected { .. }));

    peer.send(&ManagerToDaemonMessage::Health { request_id: rid(9) })
        .await
        .expect("send");
    let second = timeout(DEADLINE, peer.recv())
        .await
        .expect("deadline")
        .expect("reply");
    assert!(matches!(second, DaemonToManagerMessage::Rejected { .. }));

    drop(peer);
    let _ = timeout(DEADLINE, driver)
        .await
        .expect("deadline")
        .expect("join");
}

#[test]
fn the_state_machine_rejects_every_illegal_transition() {
    // The legal chain Plan 369 §C defines.
    assert_eq!(
        transition(AppdState::Starting, AppdState::ConnectedToRouter),
        Ok(AppdState::ConnectedToRouter)
    );
    assert_eq!(
        transition(AppdState::ConnectedToRouter, AppdState::Idle),
        Ok(AppdState::Idle)
    );
    assert_eq!(
        transition(AppdState::Idle, AppdState::Launching),
        Ok(AppdState::Launching)
    );
    assert_eq!(
        transition(AppdState::Launching, AppdState::AwaitingHello),
        Ok(AppdState::AwaitingHello)
    );
    assert_eq!(
        transition(AppdState::AwaitingHello, AppdState::Running),
        Ok(AppdState::Running)
    );
    assert_eq!(
        transition(AppdState::Running, AppdState::Stopping),
        Ok(AppdState::Stopping)
    );
    assert_eq!(
        transition(AppdState::Stopping, AppdState::Closed),
        Ok(AppdState::Closed)
    );

    // Illegal: cannot skip the handshake, cannot run without launching,
    // cannot reopen a closed manager.
    for (from, to) in [
        (AppdState::Starting, AppdState::Idle),
        (AppdState::Starting, AppdState::Running),
        (AppdState::Idle, AppdState::Running),
        (AppdState::Closed, AppdState::Idle),
        (AppdState::Running, AppdState::Launching),
        (AppdState::Idle, AppdState::ConnectedToRouter),
    ] {
        assert_eq!(
            transition(from, to),
            Err(AppdError::InvalidTransition(from.as_str(), to.as_str())),
            "{from:?} -> {to:?} must fail closed"
        );
    }
}

#[tokio::test]
async fn the_manager_greeting_is_the_exact_frozen_byte_sequence() {
    let (mut peer, appd_side) = ManagerPeer::new();
    let (_reader, mut writer) = tokio::io::split(appd_side);
    let mut appd = Appd::new();
    appd.write_handshake(&mut writer).await.expect("handshake");

    let mut bytes = [0_u8; HANDSHAKE_BYTES];
    peer.stream.read_exact(&mut bytes).await.expect("read");
    assert_eq!(&bytes, b"I2PM\x01\x00\x01\x00\x00");
    assert_eq!(
        Handshake::decode(&bytes).expect("decode"),
        Handshake {
            role: ManagerRole::Manager,
            major: MANAGER_PROTOCOL_MAJOR,
            minor: MANAGER_PROTOCOL_MINOR,
        }
    );
}
