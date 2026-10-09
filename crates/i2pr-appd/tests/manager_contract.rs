//! Plan 369 WP4 — `i2pr-appd` manager-protocol contract evidence.
//!
//! # The direction rule these tests exist to pin
//!
//! The manager protocol has two disjoint control vocabularies. The manager
//! *sends* `ManagerToDaemonMessage` and *receives* `DaemonToManagerMessage`.
//!
//! WP2 got this backwards in both the implementation and its test: the read loop
//! decoded inbound frames with `decode_manager_to_daemon_control`, and the test
//! peer sent manager-direction frames, so the test asserted the inverted
//! contract and the two mistakes cancelled. Against the real bridge the first
//! reply would have ended the transport.
//!
//! So this peer is a **daemon**: it reads the manager greeting, and every frame
//! it sends is daemon-to-manager. `inbound_decodes_daemon_to_manager_vocabulary`
//! and `manager_direction_bytes_are_not_daemon_bytes` are the regression guards;
//! a future change that swaps the two vocabularies fails them rather than
//! failing only against a real router.
//!
//! Nothing here reaches a socket, a listener, or a process. Application sessions
//! are covered separately in `session_contract.rs`.

use std::time::Duration;

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, EffectiveGrant, Frame, HANDSHAKE_BYTES, Handshake,
    MANAGER_PROTOCOL_MAJOR, MANAGER_PROTOCOL_MINOR, ManagerError, ManagerErrorCode,
    ManagerGatewayLimits, ManagerInstanceId, ManagerPrincipal, ManagerProtocolError, ManagerRole,
    ManagerServiceStreamId, ManagerSessionId, ManagerToDaemonMessage, ServiceEndReason,
    decode_manager_to_daemon_control, encode_daemon_to_manager_control,
    encode_manager_to_daemon_control,
};
use i2pr_app_proto::{AppId, Capability, PublisherId, RequestId};
use i2pr_appd::{Appd, AppdError, AppdState, DuplexTransport, transition};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::time::timeout;

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

/// A daemon-side peer: reads manager bytes, writes daemon bytes.
struct DaemonPeer {
    stream: DuplexStream,
}

impl DaemonPeer {
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

    /// Sends a daemon→manager control frame.
    async fn send(&mut self, message: &DaemonToManagerMessage) -> Result<(), String> {
        let payload = encode_daemon_to_manager_control(message).map_err(|e| e.to_string())?;
        let frame = Frame::control(payload)
            .encode()
            .map_err(|e| e.to_string())?;
        self.stream
            .write_all(&frame)
            .await
            .map_err(|e| e.to_string())
    }

    async fn send_data(&mut self, stream: ManagerServiceStreamId, bytes: &[u8]) {
        let frame = Frame::data(stream, bytes.to_vec())
            .encode()
            .expect("encode");
        self.stream.write_all(&frame).await.expect("write");
    }
}

/// Starts a manager over an in-memory duplex and returns it plus a daemon peer.
async fn start_manager() -> (DaemonPeer, tokio::task::JoinHandle<Result<(), AppdError>>) {
    let (mut daemon, appd_side) = DaemonPeer::new();
    let (appd_reader, appd_writer) = tokio::io::split(appd_side);
    let transport = DuplexTransport::new(appd_reader, appd_writer);
    let driver = tokio::spawn(async move { Appd::new().run(transport).await });
    daemon.read_handshake().await.expect("manager greeting");
    (daemon, driver)
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

#[test]
fn inbound_decodes_daemon_to_manager_vocabulary() {
    let bytes = encode_daemon_to_manager_control(&DaemonToManagerMessage::Rejected {
        request_id: rid(1),
        error: ManagerError {
            code: ManagerErrorCode::Internal,
            diagnostic: None,
        },
    })
    .expect("encode");
    assert_eq!(
        i2pr_app_manager_proto::decode_daemon_to_manager_control(&bytes),
        Ok(DaemonToManagerMessage::Rejected {
            request_id: rid(1),
            error: ManagerError {
                code: ManagerErrorCode::Internal,
                diagnostic: None,
            },
        })
    );
}

#[test]
fn manager_direction_bytes_are_not_daemon_bytes() {
    // The two vocabularies share no tags, so a direction mistake is a hard
    // failure rather than a mis-parse. That is precisely why WP2's inversion
    // would have surfaced only against the real bridge.
    let manager_bytes =
        encode_manager_to_daemon_control(&ManagerToDaemonMessage::Health { request_id: rid(3) })
            .expect("encode");
    assert_eq!(
        i2pr_app_manager_proto::decode_daemon_to_manager_control(&manager_bytes),
        Err(ManagerProtocolError::InvalidControl),
        "a manager-direction frame must not decode as a daemon frame"
    );

    let daemon_bytes = encode_daemon_to_manager_control(&DaemonToManagerMessage::SessionEnded {
        session: ManagerSessionId::new(1).expect("handle"),
        reason: ServiceEndReason::SessionEnded,
    })
    .expect("encode");
    assert_eq!(
        decode_manager_to_daemon_control(&daemon_bytes),
        Err(ManagerProtocolError::InvalidControl),
        "a daemon-direction frame must not decode as a manager frame"
    );
}

// ---------------------------------------------------------------------------
// Greeting and lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handshake_is_the_frozen_manager_greeting() {
    let (mut peer, appd_side) = DaemonPeer::new();
    let (_reader, mut writer) = tokio::io::split(appd_side);
    let mut appd = Appd::new();
    assert_eq!(appd.state(), AppdState::Starting);

    appd.write_handshake(&mut writer).await.expect("handshake");
    // The greeting is what moves the manager out of `Starting`: the daemon may
    // only treat this service as ready once these nine bytes are out.
    assert_eq!(appd.state(), AppdState::Idle);
    peer.read_handshake().await.expect("peer handshake");
}

#[tokio::test]
async fn the_manager_greeting_is_the_exact_frozen_byte_sequence() {
    let (mut peer, appd_side) = DaemonPeer::new();
    let (_reader, mut writer) = tokio::io::split(appd_side);
    let mut appd = Appd::new();
    appd.write_handshake(&mut writer).await.expect("handshake");

    let mut bytes = [0_u8; HANDSHAKE_BYTES];
    peer.stream.read_exact(&mut bytes).await.expect("read");
    assert_eq!(&bytes, b"I2PM\x01\x01\x01\x00\x00");
    assert_eq!(
        Handshake::decode(&bytes).expect("decode"),
        Handshake {
            role: ManagerRole::Manager,
            major: MANAGER_PROTOCOL_MAJOR,
            minor: MANAGER_PROTOCOL_MINOR,
        }
    );
}

#[tokio::test]
async fn a_manager_with_the_production_catalog_launches_nothing() {
    // The shipped binary's catalog yields no authority, so the manager greets,
    // sits on its transport, and launches nothing — no process, no session, and
    // no reply for the daemon to correlate.
    let (daemon, driver) = start_manager().await;

    // A daemon that has nothing to say: close the transport. The manager must
    // reach EOF and return cleanly.
    drop(daemon);
    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn a_data_frame_in_the_daemon_direction_is_a_protocol_violation() {
    let (mut daemon, driver) = start_manager().await;

    // The daemon does send service data, but only on a handle the manager has
    // opened. Before any session exists there is no such handle, so a data frame
    // here is a direction error rather than noise.
    daemon
        .send_data(ManagerServiceStreamId::new(1).expect("handle"), b"x")
        .await;

    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert_eq!(
        outcome,
        Err(AppdError::Protocol(ManagerProtocolError::InvalidHandle)),
        "data on a handle the manager never opened must fail closed"
    );
}

#[tokio::test]
async fn an_oversize_declared_frame_length_is_refused_before_allocation() {
    let (mut daemon, driver) = start_manager().await;

    // version 1, kind 1 (control), two reserved zeros, stream id 0, and a
    // declared length far above the ceiling.
    let mut hostile = vec![1_u8, 1, 0, 0];
    hostile.extend_from_slice(&0_u32.to_be_bytes());
    hostile.extend_from_slice(&u32::MAX.to_be_bytes());
    daemon.stream.write_all(&hostile).await.expect("write");

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
async fn a_correlated_reply_for_a_request_the_manager_never_sent_fails_closed() {
    let (mut daemon, driver) = start_manager().await;

    // The manager initiates every request in this protocol, so a correlated
    // reply has no legitimate origin. It is the exact shape a confused or
    // hostile daemon would send to make a manager act on something it never
    // asked about.
    daemon
        .send(&DaemonToManagerMessage::SessionOpened {
            request_id: rid(9),
            session: ManagerSessionId::new(1).expect("handle"),
        })
        .await
        .expect("send");

    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert_eq!(
        outcome,
        Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest))
    );
}

#[tokio::test]
async fn no_launch_authority_can_be_constructed_from_anything_the_daemon_sends() {
    // Plan 369 §9: launch authority is manager-created. There is no manager
    // protocol message that asks the manager to launch, so the strongest form of
    // this property is structural — but it is worth driving the closest thing
    // that exists (a plausible `create_session`) and asserting the manager never
    // produces authority for it.
    let (mut daemon, driver) = start_manager().await;

    // The shape that *would* request a session if it were manager→daemon. It is
    // framed correctly, so the failure is at the control-vocabulary layer rather
    // than at the frame layer — which is the layer the property lives on.
    let payload = encode_manager_to_daemon_control(&ManagerToDaemonMessage::CreateSession {
        request_id: rid(1),
        principal: principal(7),
        effective_capabilities: vec![EffectiveGrant {
            capability: Capability::Sam,
        }],
        limits: ManagerGatewayLimits::new(4).expect("limits"),
    })
    .expect("encode");
    let bytes = Frame::control(payload).encode().expect("frame");
    daemon.stream.write_all(&bytes).await.expect("send");

    let outcome = timeout(DEADLINE, driver)
        .await
        .expect("driver deadline")
        .expect("join");
    assert_eq!(
        outcome,
        Err(AppdError::Protocol(ManagerProtocolError::InvalidControl)),
        "a launch-shaped message in the wrong direction must fail closed, never be honoured"
    );
}

#[tokio::test]
async fn duplicate_request_ids_are_refused_by_the_ledger() {
    // Scope note: this covers *duplicates*. A zero request id is not merely
    // refused here, it is unrepresentable — `RequestId::new(0)` fails in
    // `i2pr-app-proto`, so no wire message can carry one.
    let mut ledger = i2pr_appd::RequestLedger::default();
    ledger.admit(rid(9)).expect("admit");
    assert_eq!(ledger.len(), 1);
    assert_eq!(
        ledger.admit(rid(9)),
        Err(AppdError::Protocol(ManagerProtocolError::InvalidHandle))
    );
    ledger.retire(rid(9)).expect("retire");
    assert!(ledger.is_empty());
    assert_eq!(
        ledger.retire(rid(9)),
        Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest))
    );
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

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

/// A session walks the *same* frozen table, so the manager level and the session
/// level can never disagree about what a legal lifecycle is.
#[test]
fn a_session_walks_the_same_frozen_table_as_the_manager() {
    let chain = [
        AppdState::Idle,
        AppdState::Launching,
        AppdState::AwaitingHello,
        AppdState::Running,
        AppdState::Stopping,
        AppdState::Closed,
    ];
    for pair in chain.windows(2) {
        assert_eq!(
            transition(pair[0], pair[1]),
            Ok(pair[1]),
            "{} -> {} must be legal",
            pair[0].as_str(),
            pair[1].as_str()
        );
    }
}
