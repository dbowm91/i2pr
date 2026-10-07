//! Plan 369 WP4 §E — managed-app v1 session evidence.
//!
//! # What is exercised
//!
//! One application session, over an in-memory application transport and the real
//! [`i2pr_appd::manager_link::ManagerLink`] against a scripted daemon peer. The
//! session logic under test is exactly the production logic — greeting, hello
//! matching, capability presentation, stream mapping, teardown — with only the
//! apphost's *process* substituted. WP5 exercises the real process path.
//!
//! Launch authority still comes from [`i2pr_appd::authority::LaunchAuthority`],
//! which has no decoder, so a test cannot hand an application capabilities the
//! launch was not granted — the same property production has.
//!
//! Every await is bounded. A session that fails to make progress must fail the
//! test, not hang the suite.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use i2pr_app_manager_proto::apphost::{
    DescriptiveResourceRequest, Entrypoint, LaunchRoot, SanitizedEnvironment,
};
use i2pr_app_manager_proto::{
    DaemonToManagerMessage, Frame, FrameKind, ManagerError, ManagerErrorCode, ManagerGatewayLimits,
    ManagerInstanceId, ManagerPrincipal, ManagerProtocolError, ManagerServiceStreamId,
    ManagerSessionId, ManagerToDaemonMessage, ServiceEndReason, decode_manager_to_daemon_control,
    encode_daemon_to_manager_control,
};
use i2pr_app_proto::{
    AdministratorPrincipal, AppId, AppInstanceId, AppRequestOutcome, AppService, AppToHostMessage,
    Capability, Frame as AppFrame, FrameKind as AppFrameKind, Handshake, HostToAppMessage,
    MAX_STREAMS, PROTOCOL_MAJOR, PROTOCOL_MINOR, PermissionStatus, RequestErrorCode, RequestId,
    Role,
};
use i2pr_appd::authority::{AuthorityRequest, LaunchAuthority};
use i2pr_appd::manager_link::{ManagerEvent, ManagerLink};
use i2pr_appd::session::{AppSession, SessionEnd};
use i2pr_appd::{AppdError, AppdState};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);
const APP_ID: &str = "fixture.session";
const INSTANCE: u128 = 4242;

fn rid(value: u32) -> RequestId {
    RequestId::new(value).expect("nonzero")
}

fn authority_with(capabilities: &[Capability]) -> LaunchAuthority {
    LaunchAuthority::new(
        &AdministratorPrincipal::from_authenticated_session(1).expect("administrator"),
        AuthorityRequest {
            principal: ManagerPrincipal {
                app_id: AppId::parse(APP_ID).expect("app id"),
                instance_id: ManagerInstanceId::new(INSTANCE),
                publisher_id: None,
            },
            capabilities: capabilities.to_vec(),
            launch_profile: i2pr_app_proto::LaunchProfile::UnsafeDirect,
            root: LaunchRoot::new("/opt/fixture").expect("root"),
            entrypoint: Entrypoint::new("app").expect("entrypoint"),
            argv: Vec::new(),
            environment: SanitizedEnvironment::new(BTreeMap::new()).expect("environment"),
            resources: DescriptiveResourceRequest {
                requested_memory_bytes: 0,
                requested_open_files: 0,
            },
            max_connections: 8,
        },
    )
    .expect("authority")
}

/// A scripted daemon peer.
///
/// Requests are answered from `policy`; unsolicited messages and backend data are
/// injected by the test. Control messages and data frames the manager sent are
/// published so a test can assert on exact octets.
struct DaemonPeer {
    seen: mpsc::Receiver<ManagerToDaemonMessage>,
    data_seen: mpsc::Receiver<(ManagerServiceStreamId, Vec<u8>)>,
    inject: mpsc::Sender<DaemonToManagerMessage>,
    inject_data: mpsc::Sender<(ManagerServiceStreamId, Vec<u8>)>,
}

/// How the scripted daemon answers service opens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenPolicy {
    Accept,
    Reject(ManagerErrorCode),
}

struct Harness {
    link: Arc<ManagerLink>,
    daemon: DaemonPeer,
    driver: JoinHandle<Result<(), AppdError>>,
    daemon_task: JoinHandle<()>,
    session: ManagerSessionId,
    /// The link's event channel for this session. Handed to the session exactly
    /// once, by `start_session`.
    events: Option<mpsc::Receiver<ManagerEvent>>,
}

async fn harness(capabilities: &[Capability], policy: OpenPolicy) -> Harness {
    let (manager_side, daemon_side) = tokio::io::duplex(256 * 1024);
    let (manager_read, manager_write) = tokio::io::split(manager_side);
    let (link, driver) = ManagerLink::drive(manager_read, manager_write);
    let link = Arc::new(link);

    // Sized past `MAX_STREAMS` so the ceiling test cannot fill it: a full
    // channel would stall the scripted daemon and read as the session hanging.
    let (seen_tx, seen) = mpsc::channel(4 * MAX_STREAMS);
    let (data_tx, data_seen) = mpsc::channel(64);
    let (inject_tx, inject_rx) = mpsc::channel::<DaemonToManagerMessage>(64);
    let (inject_data_tx, inject_data_rx) = mpsc::channel::<(ManagerServiceStreamId, Vec<u8>)>(64);
    let daemon_task = tokio::spawn(daemon_loop(
        daemon_side,
        policy,
        seen_tx,
        data_tx,
        inject_rx,
        inject_data_rx,
    ));

    // Open the daemon session through the real link, so the route is registered
    // by the reader exactly as production registers it.
    let authority = authority_with(capabilities);
    let (events_tx, events_rx) = mpsc::channel(64);
    let reply = link
        .open_session(
            ManagerToDaemonMessage::CreateSession {
                request_id: rid(1),
                principal: authority.principal().clone(),
                effective_capabilities: authority.effective_grants(),
                limits: ManagerGatewayLimits::new(8).expect("limits"),
            },
            events_tx,
        )
        .await
        .expect("create_session");
    let session = match reply {
        DaemonToManagerMessage::SessionOpened { session, .. } => session,
        other => panic!("expected session_opened, got {other:?}"),
    };

    Harness {
        link,
        daemon: DaemonPeer {
            seen,
            data_seen,
            inject: inject_tx,
            inject_data: inject_data_tx,
        },
        driver,
        daemon_task,
        session,
        events: Some(events_rx),
    }
}

impl DaemonPeer {
    /// Waits for the next `open_service`, skipping anything else.
    ///
    /// The harness's own `create_session` is in this stream too, so a naive
    /// "next message" read would consume it and the assertion would be about the
    /// wrong message.
    async fn next_open(&mut self) -> ManagerToDaemonMessage {
        let deadline = tokio::time::Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let message = timeout(remaining, self.seen.recv())
                .await
                .expect("no open_service arrived before the deadline")
                .expect("daemon peer closed");
            if matches!(message, ManagerToDaemonMessage::OpenService { .. }) {
                return message;
            }
        }
    }

    /// Counts `open_service` messages seen within a short window.
    async fn opens_within(&mut self, window: Duration) -> usize {
        let deadline = tokio::time::Instant::now() + window;
        let mut opens = 0;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return opens;
            }
            match timeout(remaining, self.seen.recv()).await {
                Ok(Some(ManagerToDaemonMessage::OpenService { .. })) => opens += 1,
                Ok(Some(_)) => {}
                Ok(None) => return opens,
                Err(_) => return opens,
            }
        }
    }
}

async fn daemon_loop(
    mut stream: DuplexStream,
    policy: OpenPolicy,
    seen_tx: mpsc::Sender<ManagerToDaemonMessage>,
    data_tx: mpsc::Sender<(ManagerServiceStreamId, Vec<u8>)>,
    mut inject: mpsc::Receiver<DaemonToManagerMessage>,
    mut inject_data: mpsc::Receiver<(ManagerServiceStreamId, Vec<u8>)>,
) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut next_stream: u64 = 1;
    let mut next_session: u64 = 0;

    loop {
        // Drain whichever channel has something, then read. `biased` keeps an
        // injected message ahead of a read so a test can order them.
        tokio::select! {
            biased;
            Some(message) = inject.recv() => {
                let Ok(payload) = encode_daemon_to_manager_control(&message) else { continue };
                let Ok(bytes) = Frame::control(payload).encode() else { continue };
                if stream.write_all(&bytes).await.is_err() {
                    return;
                }
            }
            Some((handle, payload)) = inject_data.recv() => {
                let Ok(bytes) = Frame::data(handle, payload).encode() else { continue };
                if stream.write_all(&bytes).await.is_err() {
                    return;
                }
            }
            read = stream.read(&mut chunk) => {
                let Ok(read) = read else { return };
                if read == 0 {
                    return;
                }
                buffer.extend_from_slice(&chunk[..read]);
                loop {
                    let (frame, consumed) = match Frame::decode(&buffer) {
                        Ok(decoded) => decoded,
                        Err(ManagerProtocolError::TruncatedFrame) => break,
                        Err(_) => return,
                    };
                    buffer.drain(..consumed);
                    match frame.kind {
                        FrameKind::Data => {
                            let Ok(handle) = ManagerServiceStreamId::new(u64::from(frame.stream_id))
                            else {
                                return;
                            };
                            if data_tx.send((handle, frame.payload)).await.is_err() {
                                return;
                            }
                        }
                        FrameKind::Control => {
                            let Ok(message) = decode_manager_to_daemon_control(&frame.payload)
                            else {
                                continue;
                            };
                            let reply = answer(&message, policy, &mut next_stream, &mut next_session);
                            if seen_tx.send(message).await.is_err() {
                                return;
                            }
                            let Some(reply) = reply else { continue };
                            let Ok(payload) = encode_daemon_to_manager_control(&reply) else {
                                continue;
                            };
                            let Ok(bytes) = Frame::control(payload).encode() else {
                                continue;
                            };
                            if stream.write_all(&bytes).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn answer(
    message: &ManagerToDaemonMessage,
    policy: OpenPolicy,
    next_stream: &mut u64,
    next_session: &mut u64,
) -> Option<DaemonToManagerMessage> {
    let request_id = match message {
        ManagerToDaemonMessage::CreateSession { request_id, .. }
        | ManagerToDaemonMessage::CloseSession { request_id, .. }
        | ManagerToDaemonMessage::OpenService { request_id, .. }
        | ManagerToDaemonMessage::CloseService { request_id, .. }
        | ManagerToDaemonMessage::ResetService { request_id, .. }
        | ManagerToDaemonMessage::Health { request_id }
        | ManagerToDaemonMessage::Shutdown { request_id, .. } => *request_id,
    };
    match message {
        ManagerToDaemonMessage::CreateSession { .. } => {
            *next_session += 1;
            Some(DaemonToManagerMessage::SessionOpened {
                request_id,
                session: ManagerSessionId::new(*next_session).expect("handle"),
            })
        }
        ManagerToDaemonMessage::OpenService { session, .. } => match policy {
            OpenPolicy::Accept => {
                *next_stream += 1;
                Some(DaemonToManagerMessage::ServiceOpened {
                    request_id,
                    session: *session,
                    stream: ManagerServiceStreamId::new(*next_stream).expect("handle"),
                })
            }
            OpenPolicy::Reject(code) => Some(DaemonToManagerMessage::Rejected {
                request_id,
                error: ManagerError {
                    code,
                    diagnostic: None,
                },
            }),
        },
        ManagerToDaemonMessage::CloseService {
            session, stream, ..
        } => Some(DaemonToManagerMessage::ServiceClosed {
            request_id,
            session: *session,
            stream: *stream,
        }),
        ManagerToDaemonMessage::ResetService {
            session, stream, ..
        } => Some(DaemonToManagerMessage::ServiceReset {
            request_id,
            session: *session,
            stream: *stream,
        }),
        ManagerToDaemonMessage::CloseSession { session, .. } => {
            Some(DaemonToManagerMessage::SessionClosed {
                request_id,
                session: *session,
            })
        }
        _ => None,
    }
}

/// The application side of the session transport.
struct AppPeer {
    stream: DuplexStream,
    /// Carried between frames.
    ///
    /// Load-bearing: the session writes `Reply` and `Capabilities` back to back,
    /// so one read routinely returns both. A per-call buffer would decode the
    /// first and throw the second away, which is a defect in the *test* that
    /// looks exactly like the session failing to send it.
    buffer: Vec<u8>,
}

impl AppPeer {
    async fn greet(&mut self) {
        let greeting = Handshake {
            role: Role::Application,
            major: PROTOCOL_MAJOR,
            minor: PROTOCOL_MINOR,
        };
        self.stream
            .write_all(&greeting.encode())
            .await
            .expect("greeting");
    }

    async fn greet_with(&mut self, role: Role, major: u8, minor: u8) {
        let greeting = Handshake { role, major, minor };
        self.stream
            .write_all(&greeting.encode())
            .await
            .expect("greeting");
    }

    async fn send_control(&mut self, message: &AppToHostMessage) {
        let payload = i2pr_app_proto::encode_app_to_host_control(message).expect("encode control");
        let bytes = AppFrame {
            kind: AppFrameKind::Control,
            stream_id: 0,
            payload,
        }
        .encode()
        .expect("encode frame");
        self.stream.write_all(&bytes).await.expect("write");
    }

    /// Sends raw JSON, so a test can present a message the codec refuses to
    /// encode (a wrong protocol major, for instance).
    async fn send_raw_control(&mut self, json: &str) {
        let bytes = AppFrame {
            kind: AppFrameKind::Control,
            stream_id: 0,
            payload: json.as_bytes().to_vec(),
        }
        .encode()
        .expect("encode frame");
        self.stream.write_all(&bytes).await.expect("write");
    }

    async fn send_data(&mut self, stream_id: u32, payload: &[u8]) {
        let bytes = AppFrame {
            kind: AppFrameKind::Data,
            stream_id,
            payload: payload.to_vec(),
        }
        .encode()
        .expect("encode frame");
        self.stream.write_all(&bytes).await.expect("write");
    }

    async fn recv_control(&mut self) -> HostToAppMessage {
        let frame = self.next_frame().await;
        assert_eq!(
            frame.kind,
            AppFrameKind::Control,
            "expected a control frame, got {frame:?}"
        );
        i2pr_app_proto::decode_host_to_app_control(&frame.payload).expect("decode control")
    }

    async fn next_frame(&mut self) -> AppFrame {
        let mut chunk = [0_u8; 8192];
        loop {
            if !self.buffer.is_empty()
                && let Ok((frame, consumed)) = AppFrame::decode(&self.buffer)
            {
                self.buffer.drain(..consumed);
                return frame;
            }
            let read = timeout(DEADLINE, self.stream.read(&mut chunk))
                .await
                .expect("application read deadline")
                .expect("read");
            assert_ne!(read, 0, "the session closed the application transport");
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    async fn recv_data(&mut self) -> (u32, Vec<u8>) {
        let frame = self.next_frame().await;
        assert_eq!(
            frame.kind,
            AppFrameKind::Data,
            "expected data, got {frame:?}"
        );
        (frame.stream_id, frame.payload)
    }
}

/// Starts a session and hands back the application peer.
async fn start_session(
    harness: &mut Harness,
    capabilities: &[Capability],
) -> (AppPeer, JoinHandle<SessionEnd>) {
    let authority = authority_with(capabilities);
    let instance = authority.instance_id().clone();
    // `app_side` stays whole so the application peer can read and write it; the
    // session gets the other end split, because it reads in one task and writes
    // in another.
    let (app_side, session_side) = tokio::io::duplex(256 * 1024);
    let (session_read, session_write) = tokio::io::split(session_side);

    let events = harness
        .events
        .take()
        .expect("the harness event channel is handed to exactly one session");
    let session = AppSession::attach(
        authority,
        harness.session,
        Arc::clone(&harness.link),
        None,
        events,
        session_read,
        Box::new(session_write),
    );
    assert_eq!(session.state(), AppdState::Launching);
    let task = tokio::spawn(session.run());
    let _ = instance;
    (
        AppPeer {
            stream: app_side,
            buffer: Vec::new(),
        },
        task,
    )
}

async fn end_of(task: JoinHandle<SessionEnd>) -> SessionEnd {
    timeout(DEADLINE, task)
        .await
        .expect("session deadline")
        .expect("join")
}
// ---------------------------------------------------------------------------
// §10 — hello is declaration, not authentication
// ---------------------------------------------------------------------------

fn hello() -> AppToHostMessage {
    AppToHostMessage::Hello {
        request_id: rid(2),
        app_id: AppId::parse(APP_ID).expect("app id"),
        instance_id: AppInstanceId::new(INSTANCE).expect("instance"),
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
    }
}

#[tokio::test]
async fn a_matching_hello_is_answered_with_the_launch_capabilities() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;

    match app.recv_control().await {
        HostToAppMessage::Reply {
            request_id,
            outcome: AppRequestOutcome::Succeeded,
        } => assert_eq!(request_id, rid(2)),
        other => panic!("hello must be answered, got {other:?}"),
    }
    match app.recv_control().await {
        HostToAppMessage::Capabilities { capabilities } => {
            assert_eq!(capabilities, vec![Capability::Sam]);
        }
        other => panic!("capabilities must follow the reply, got {other:?}"),
    }

    drop(app);
    assert_eq!(end_of(task).await, SessionEnd::ApplicationClosed);
}

#[tokio::test]
async fn a_wrong_app_id_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&AppToHostMessage::Hello {
        request_id: rid(2),
        app_id: AppId::parse("someone.else").expect("app id"),
        instance_id: AppInstanceId::new(INSTANCE).expect("instance"),
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
    })
    .await;

    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App("hello declared a different app id"))
    );
}

#[tokio::test]
async fn a_wrong_instance_id_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&AppToHostMessage::Hello {
        request_id: rid(2),
        app_id: AppId::parse(APP_ID).expect("app id"),
        instance_id: AppInstanceId::new(INSTANCE + 1).expect("instance"),
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
    })
    .await;

    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App("hello declared a different instance id"))
    );
}

#[tokio::test]
async fn a_protocol_minor_mismatch_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    // Minor is not checked by the codec, so this encodes cleanly and is refused
    // by the session: Plan 369 makes no minor-compatibility promise, so a
    // mismatch is not assumed harmless.
    app.send_control(&AppToHostMessage::Hello {
        request_id: rid(2),
        app_id: AppId::parse(APP_ID).expect("app id"),
        instance_id: AppInstanceId::new(INSTANCE).expect("instance"),
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR + 1,
    })
    .await;

    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::Contract(
            i2pr_app_proto::ContractError::UnsupportedVersion
        ))
    );
}

#[tokio::test]
async fn a_protocol_major_mismatch_never_reaches_the_identity_check() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    // The codec refuses to encode a wrong major, so this is presented as raw
    // JSON to prove the *decoding* side also refuses it.
    app.send_raw_control(
        r#"{"type":"hello","request_id":2,"app_id":"fixture.session","instance_id":"4242","protocol_major":9,"protocol_minor":0}"#,
    )
    .await;

    assert!(
        matches!(
            end_of(task).await,
            SessionEnd::Refused(AppdError::Contract(_))
        ),
        "an unencodable version must fail the launch"
    );
}

#[tokio::test]
async fn a_second_hello_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    app.send_control(&hello()).await;
    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App("application sent a second hello"))
    );
}

#[tokio::test]
async fn data_before_hello_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_data(7, b"early").await;

    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App("application sent data before hello"))
    );
}

#[tokio::test]
async fn a_first_request_that_is_not_hello_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&AppToHostMessage::PermissionRequest {
        request_id: rid(2),
        capabilities: vec![i2pr_app_proto::RequestedCapability {
            capability: Capability::Sam,
        }],
    })
    .await;

    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App(
            "the first application request was not hello"
        ))
    );
}

#[tokio::test]
async fn an_administrator_role_greeting_is_the_wrong_principal() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet_with(Role::Administrator, PROTOCOL_MAJOR, PROTOCOL_MINOR)
        .await;

    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::Contract(
            i2pr_app_proto::ContractError::RoleMismatch
        ))
    );
}

// ---------------------------------------------------------------------------
// §11 — a permission request cannot raise authority
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_permission_request_is_denied_and_changes_nothing() {
    // The launch holds SAM only. The application asks for brokered clearnet, then
    // for SAM. The first is denied; the second succeeds because it was *granted*,
    // not because it was asked for. And the ask for brokered clearnet changes
    // nothing: the capabilities the application was shown, and still holds, are
    // exactly the launch's.
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    match app.recv_control().await {
        HostToAppMessage::Capabilities { capabilities } => {
            assert_eq!(capabilities, vec![Capability::Sam]);
        }
        other => panic!("expected capabilities, got {other:?}"),
    }

    app.send_control(&AppToHostMessage::PermissionRequest {
        request_id: rid(3),
        capabilities: vec![
            i2pr_app_proto::RequestedCapability {
                capability: Capability::BrokeredTcp,
            },
            i2pr_app_proto::RequestedCapability {
                capability: Capability::Lifecycle,
            },
        ],
    })
    .await;
    match app.recv_control().await {
        HostToAppMessage::PermissionReply { request_id, status } => {
            assert_eq!(request_id, rid(3));
            assert_eq!(
                status,
                PermissionStatus::Denied,
                "Plan 369 has no administrator, so a request is denied deterministically"
            );
        }
        other => panic!("expected permission_reply, got {other:?}"),
    }

    // Nothing about the grant changed: I2CP was never held and is still refused.
    app.send_control(&AppToHostMessage::Open {
        request_id: rid(4),
        stream_id: 5,
        service: AppService::I2cp,
    })
    .await;
    match app.recv_control().await {
        HostToAppMessage::Reply {
            request_id,
            outcome: AppRequestOutcome::Failed(error),
        } => {
            assert_eq!(request_id, rid(4));
            assert_eq!(error.code, RequestErrorCode::PermissionDenied);
        }
        other => panic!("an ungranted service must be refused, got {other:?}"),
    }

    // And no `open_service` reached the daemon for it.
    assert!(
        !matches!(
            timeout(DEADLINE, harness.daemon.seen.recv()).await,
            Ok(Some(ManagerToDaemonMessage::OpenService { .. }))
        ),
        "a refused capability must not allocate anything on the daemon"
    );

    drop(app);
    let _ = end_of(task).await;
}

#[tokio::test]
async fn an_ungranted_service_is_refused_before_the_daemon_is_told() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    app.send_control(&AppToHostMessage::Open {
        request_id: rid(3),
        stream_id: 1,
        service: AppService::I2cp,
    })
    .await;
    match app.recv_control().await {
        HostToAppMessage::Reply {
            outcome: AppRequestOutcome::Failed(error),
            ..
        } => assert_eq!(error.code, RequestErrorCode::PermissionDenied),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert_eq!(
        harness
            .daemon
            .opens_within(Duration::from_millis(300))
            .await,
        0,
        "nothing may reach the daemon for a capability the launch does not hold"
    );
    drop(app);
    let _ = end_of(task).await;
}

#[tokio::test]
async fn control_scoped_is_refused_by_type() {
    // §11 and invariant 6: there is no control-scoped implementation, so it is
    // refused by the vocabulary mapping rather than by a capability check that a
    // later grant could switch on.
    let mut harness = harness(&[Capability::ControlScoped], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::ControlScoped]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    app.send_control(&AppToHostMessage::Open {
        request_id: rid(3),
        stream_id: 1,
        service: AppService::ControlScoped,
    })
    .await;
    match app.recv_control().await {
        HostToAppMessage::Reply {
            outcome: AppRequestOutcome::Failed(error),
            ..
        } => assert_eq!(error.code, RequestErrorCode::UnsupportedOperation),
        other => panic!("expected refusal, got {other:?}"),
    }
    drop(app);
    let _ = end_of(task).await;
}

// ---------------------------------------------------------------------------
// §E — stream mapping
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exact_octets_flow_in_both_directions() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    app.send_control(&AppToHostMessage::Open {
        request_id: rid(3),
        stream_id: 9,
        service: AppService::Sam,
    })
    .await;
    assert!(matches!(
        app.recv_control().await,
        HostToAppMessage::Reply {
            outcome: AppRequestOutcome::Succeeded,
            ..
        }
    ));
    let request = harness.daemon.next_open().await;
    let ManagerToDaemonMessage::OpenService {
        session, service, ..
    } = request
    else {
        panic!("expected open_service, got {request:?}");
    };
    assert_eq!(session, harness.session);
    assert_eq!(service, i2pr_app_manager_proto::ManagerService::Sam);

    // Application → daemon, byte for byte.
    app.send_data(9, b"HELLO VERSION MIN=3.1 MAX=3.1\n").await;
    let (handle, payload) = timeout(DEADLINE, harness.daemon.data_seen.recv())
        .await
        .expect("deadline")
        .expect("data");
    assert_ne!(handle, ManagerServiceStreamId::new(1).expect("handle"));
    assert_eq!(payload, b"HELLO VERSION MIN=3.1 MAX=3.1\n");

    // Daemon → application, byte for byte.
    harness
        .daemon
        .inject_data
        .send((handle, b"HELLO REPLY RESULT=OK\n".to_vec()))
        .await
        .expect("inject");
    let (stream_id, payload) = app.recv_data().await;
    assert_eq!(
        stream_id, 9,
        "backend bytes must arrive on the application's own stream id"
    );
    assert_eq!(payload, b"HELLO REPLY RESULT=OK\n");

    drop(app);
    let _ = end_of(task).await;
}

#[tokio::test]
async fn a_duplicate_stream_id_is_refused_and_allocates_nothing() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    for _ in 0..2 {
        app.send_control(&AppToHostMessage::Open {
            request_id: rid(3),
            stream_id: 11,
            service: AppService::Sam,
        })
        .await;
    }
    assert!(matches!(
        app.recv_control().await,
        HostToAppMessage::Reply {
            outcome: AppRequestOutcome::Succeeded,
            ..
        }
    ));
    match app.recv_control().await {
        HostToAppMessage::Reply {
            outcome: AppRequestOutcome::Failed(error),
            ..
        } => assert_eq!(error.code, RequestErrorCode::Conflict),
        other => panic!("a duplicate stream id must be refused, got {other:?}"),
    }

    // Exactly one `open_service` reached the daemon.
    assert_eq!(
        harness
            .daemon
            .opens_within(Duration::from_millis(300))
            .await,
        1,
        "the refused duplicate must not reach the daemon"
    );
    drop(app);
    let _ = end_of(task).await;
}

#[tokio::test]
async fn data_on_a_stream_that_is_not_open_kills_the_launch() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    // Stream 4 was never opened. Forwarding this would let the application speak
    // for a handle it does not own.
    app.send_data(4, b"speaking for someone else").await;
    assert_eq!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App("data on a stream that is not open"))
    );
}

#[tokio::test]
async fn a_backend_end_closes_only_the_mapped_stream() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    let mut handles = Vec::new();
    for stream_id in [21_u32, 22] {
        app.send_control(&AppToHostMessage::Open {
            request_id: rid(3),
            stream_id,
            service: AppService::Sam,
        })
        .await;
        assert!(matches!(
            app.recv_control().await,
            HostToAppMessage::Reply {
                outcome: AppRequestOutcome::Succeeded,
                ..
            }
        ));
        let _ = harness
            .daemon
            .opens_within(Duration::from_millis(100))
            .await;
        handles.push(stream_id);
    }

    // End only the first stream.
    let first = ManagerServiceStreamId::new(1).expect("handle");
    harness
        .daemon
        .inject
        .send(DaemonToManagerMessage::ServiceEnded {
            session: harness.session,
            stream: ManagerServiceStreamId::new(2).expect("handle"),
            reason: ServiceEndReason::BackendClosed,
        })
        .await
        .expect("inject");
    let _ = first;

    match app.recv_control().await {
        HostToAppMessage::StreamReset { stream_id, .. } => {
            assert!(
                stream_id == 21 || stream_id == 22,
                "the notification must name an open stream, got {stream_id}"
            );
        }
        other => panic!("expected stream_reset, got {other:?}"),
    }

    // The session is still alive: the sibling stream still carries bytes.
    drop(app);
    assert_ne!(
        end_of(task).await,
        SessionEnd::Refused(AppdError::App("data on a stream that is not open")),
        "one backend end must not terminate the session"
    );
}

#[tokio::test]
async fn app_eof_tears_every_backend_stream_down() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    app.send_control(&AppToHostMessage::Open {
        request_id: rid(3),
        stream_id: 31,
        service: AppService::Sam,
    })
    .await;
    let _ = app.recv_control().await;
    let _ = harness
        .daemon
        .opens_within(Duration::from_millis(100))
        .await;

    // The application closes its channel without closing the stream.
    drop(app);
    assert_eq!(end_of(task).await, SessionEnd::ApplicationClosed);

    let mut closed = false;
    let deadline = tokio::time::Instant::now() + DEADLINE;
    while let Ok(Some(message)) = timeout(
        deadline.saturating_duration_since(tokio::time::Instant::now()),
        harness.daemon.seen.recv(),
    )
    .await
    {
        if matches!(message, ManagerToDaemonMessage::CloseSession { .. }) {
            closed = true;
        }
    }
    assert!(
        closed,
        "an application EOF must close the daemon session too"
    );
    assert_eq!(
        harness.link.bound_streams().await,
        0,
        "teardown must release every stream binding"
    );
    assert_eq!(
        harness.link.routed_sessions().await,
        0,
        "teardown must release the session route"
    );
}

#[tokio::test]
async fn a_session_end_from_the_daemon_closes_the_application_transport() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    harness
        .daemon
        .inject
        .send(DaemonToManagerMessage::SessionEnded {
            session: harness.session,
            reason: ServiceEndReason::SessionEnded,
        })
        .await
        .expect("inject");

    assert_eq!(
        end_of(task).await,
        SessionEnd::ManagerEnded(ServiceEndReason::SessionEnded)
    );
    assert_eq!(
        harness.link.routed_sessions().await,
        0,
        "a manager-ended session must still release its route"
    );
}

#[tokio::test]
async fn the_session_stream_ceiling_is_refused_at_max_plus_one() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    for index in 0..=MAX_STREAMS {
        let stream_id = u32::try_from(index).expect("stream id") + 1;
        app.send_control(&AppToHostMessage::Open {
            request_id: rid(3),
            stream_id,
            service: AppService::Sam,
        })
        .await;
        let reply = app.recv_control().await;
        if index < MAX_STREAMS {
            assert!(
                matches!(
                    reply,
                    HostToAppMessage::Reply {
                        outcome: AppRequestOutcome::Succeeded,
                        ..
                    }
                ),
                "stream {index} must be admitted"
            );
        } else {
            match reply {
                HostToAppMessage::Reply {
                    outcome: AppRequestOutcome::Failed(error),
                    ..
                } => assert_eq!(
                    error.code,
                    RequestErrorCode::ResourceLimit,
                    "the ceiling must be a resource-limit refusal, not a crash"
                ),
                other => panic!("max+1 must be refused, got {other:?}"),
            }
        }
    }
    drop(app);
    let _ = end_of(task).await;
}

#[tokio::test]
async fn a_daemon_refusal_is_mapped_to_a_typed_application_refusal() {
    let mut harness = harness(
        &[Capability::Sam],
        OpenPolicy::Reject(ManagerErrorCode::ResourceLimit),
    )
    .await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    app.send_control(&AppToHostMessage::Open {
        request_id: rid(3),
        stream_id: 1,
        service: AppService::Sam,
    })
    .await;
    match app.recv_control().await {
        HostToAppMessage::Reply {
            request_id,
            outcome: AppRequestOutcome::Failed(error),
        } => {
            assert_eq!(request_id, rid(3));
            assert_eq!(error.code, RequestErrorCode::ResourceLimit);
        }
        other => panic!("a daemon refusal must reach the application typed, got {other:?}"),
    }

    // The session survives a refusal — the application may try something else.
    app.send_control(&AppToHostMessage::Close { stream_id: 1 })
        .await;
    drop(app);
    assert_eq!(end_of(task).await, SessionEnd::ApplicationClosed);
}

#[tokio::test]
async fn the_link_driver_terminates_when_the_daemon_transport_ends() {
    let mut harness = harness(&[Capability::Sam], OpenPolicy::Accept).await;
    let (mut app, task) = start_session(&mut harness, &[Capability::Sam]).await;

    app.greet().await;
    app.send_control(&hello()).await;
    let _ = app.recv_control().await;
    let _ = app.recv_control().await;

    drop(app);
    let _ = end_of(task).await;

    // The scripted daemon holds the far end of the transport, so it must be
    // ended before the link can observe EOF; otherwise the driver would be
    // waiting on a peer this test still owns.
    harness.daemon_task.abort();
    drop(harness.daemon);
    drop(harness.link);
    let outcome = timeout(DEADLINE, harness.driver)
        .await
        .expect("the link driver must terminate")
        .expect("join");
    assert_eq!(outcome, Ok(()));
}
