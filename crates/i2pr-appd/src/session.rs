//! Plan 369 §E — binding the managed-app v1 application protocol session.
//!
//! # What a session is
//!
//! One launched application, its apphost transport, and its manager-protocol
//! session. This is the only place in Plan 369 that speaks managed-app v1 to an
//! application, and the only place that maps application logical stream ids to
//! daemon manager-protocol handles.
//!
//! # The three rules that carry the security weight
//!
//! 1. **Hello is declaration, not authentication.** The application was spawned
//!    by apphost, which was spawned by this manager, and the private stdio
//!    channel was handed to it. `hello` only confirms that the process is
//!    speaking for the identity this manager already selected. App id and
//!    instance id match exactly; the major must match, and older compatible
//!    minors are accepted with newer capabilities filtered out. Any identity
//!    mismatch, duplicate, future minor, or data-before-hello ends the launch.
//! 2. **A permission request cannot raise authority.** [`LaunchAuthority`] owns
//!    its capabilities behind no `&mut` path at all, so there is nothing to
//!    mutate. The observable consequence is that a capability the authority does
//!    not hold is still refused after it has been requested.
//! 3. **Streams are session-local.** An application's stream id is meaningful
//!    only inside its own session, and the daemon-issued handle behind it is
//!    per-session too, so one application cannot name another's.
//!
//! # Concurrency
//!
//! The application read half is owned by one task that emits bounded events, so
//! the session loop never cancels a partially-read frame. The session task owns
//! the write half and is the only writer, so application bytes cannot interleave.
//! At most one `open_service` is outstanding per session, because the reply is
//! awaited inline; that is the tightest bound available on pending opens.
//!
//! # What a refusal is and is not
//!
//! A refusal of an *open* is a normal answer: the application gets a typed
//! failure and the session continues. A *protocol* violation by the application
//! is not: the launch is terminated, because the launch authority's entire basis
//! is the identity the application failed to present.

use std::collections::BTreeMap;
use std::sync::Arc;

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, ManagerErrorCode, ManagerService, ManagerServiceStreamId,
    ManagerSessionId, ManagerToDaemonMessage, ServiceEndReason,
};
use i2pr_app_proto::{
    AppRequestOutcome, AppService, AppToHostMessage, ContractError, Frame, FrameKind,
    HANDSHAKE_BYTES, HostToAppMessage, PROTOCOL_MAJOR, PROTOCOL_MINOR, PermissionStatus,
    RequestError, RequestErrorCode, RequestId, SessionLimits,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::apphost_launch::LaunchedApphost;
use crate::authority::LaunchAuthority;
use crate::manager_link::{MAX_SESSION_EVENT_QUEUE, ManagerEvent, ManagerLink};
use crate::{AppdError, AppdState};

/// Read granularity for the application transport.
const APP_READ_CHUNK_BYTES: usize = 8 * 1024;

/// Largest application frame the session will hold while assembling one.
const MAX_APP_FRAME_BYTES: usize =
    i2pr_app_proto::FRAME_HEADER_BYTES + i2pr_app_proto::MAX_FRAME_PAYLOAD_BYTES;

/// Bound on how long one application may take to present its greeting and `hello`.
///
/// Without this, a launched process that says nothing holds a session slot and a
/// supervisor task for ever. It is deliberately *not* applied afterwards: a
/// running application may legitimately be quiet, and its lifetime is bounded by
/// the daemon session, not by a manager-side timer.
pub const APP_HELLO_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// Deadline for the teardown `close_session`.
///
/// Short, because the manager is already exiting and a wedged daemon must not
/// extend shutdown by the full reply grace.
pub const SESSION_CLOSE_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// How a session ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEnd {
    /// The application closed its channel.
    ApplicationClosed,
    /// The daemon ended the session and every descendant stream.
    ManagerEnded(ServiceEndReason),
    /// The launch or session was refused. Carries the typed reason.
    Refused(AppdError),
}

/// Why one application `open` was refused.
///
/// This is an *answer*, not a failure: the session stays up and the application
/// receives a typed `Reply`. Protocol and transport failures are [`AppdError`]
/// and terminate the launch instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenRefusal {
    /// The requested service has no implementation in Plan 369.
    UnsupportedService,
    /// The launch authority does not hold the capability this service needs.
    CapabilityDenied,
    /// The daemon refused, with its own typed reason.
    DaemonRefused(ManagerErrorCode),
    /// The stream id is already open in this session.
    DuplicateStream,
    /// The per-session stream ceiling is reached.
    StreamLimit,
}

impl OpenRefusal {
    const fn code(self) -> RequestErrorCode {
        match self {
            Self::UnsupportedService => RequestErrorCode::UnsupportedOperation,
            Self::CapabilityDenied => RequestErrorCode::PermissionDenied,
            Self::DaemonRefused(ManagerErrorCode::PermissionDenied) => {
                RequestErrorCode::PermissionDenied
            }
            Self::DaemonRefused(ManagerErrorCode::ResourceLimit) => RequestErrorCode::ResourceLimit,
            Self::DaemonRefused(ManagerErrorCode::NotFound) => RequestErrorCode::NotFound,
            Self::DaemonRefused(ManagerErrorCode::Conflict) => RequestErrorCode::Conflict,
            Self::DaemonRefused(ManagerErrorCode::UnsupportedOperation) => {
                RequestErrorCode::UnsupportedOperation
            }
            Self::DaemonRefused(_) => RequestErrorCode::Internal,
            Self::DuplicateStream => RequestErrorCode::Conflict,
            Self::StreamLimit => RequestErrorCode::ResourceLimit,
        }
    }

    const fn diagnostic(self) -> &'static str {
        match self {
            Self::UnsupportedService => {
                "control_scoped has no implementation in Plan 369 and is refused by type"
            }
            Self::CapabilityDenied => "this launch holds no capability for the requested service",
            Self::DaemonRefused(ManagerErrorCode::PermissionDenied) => {
                "daemon refused: capability not granted"
            }
            Self::DaemonRefused(ManagerErrorCode::ResourceLimit) => {
                "daemon refused: resource limit reached"
            }
            Self::DaemonRefused(ManagerErrorCode::NotFound) => "daemon refused: service not found",
            Self::DaemonRefused(ManagerErrorCode::Conflict) => {
                "daemon refused: stream already open"
            }
            Self::DaemonRefused(ManagerErrorCode::UnsupportedOperation) => {
                "daemon refused: service unavailable"
            }
            Self::DaemonRefused(ManagerErrorCode::InvalidRequest) => {
                "daemon refused: invalid request"
            }
            Self::DaemonRefused(ManagerErrorCode::Internal) => "daemon refused: internal error",
            Self::DuplicateStream => "this stream id is already open in this session",
            Self::StreamLimit => "this session holds the maximum number of streams",
        }
    }
}

/// Something the application side produced.
enum AppEvent {
    /// The 9-byte greeting, already decoded.
    Greeting(i2pr_app_proto::Handshake),
    Frame(Frame),
    /// The application closed its channel.
    Eof,
    Failed(AppdError),
}

/// Reads the application transport and emits bounded events.
///
/// Stopping is explicit: the session drops the stop sender and awaits this task,
/// so the read half is released before the apphost is asked to exit.
async fn read_application<R>(
    mut reader: R,
    events: mpsc::Sender<AppEvent>,
    mut stop: mpsc::Receiver<()>,
) where
    R: AsyncRead + Unpin,
{
    let mut buffered: Vec<u8> = Vec::new();
    let mut chunk = vec![0_u8; APP_READ_CHUNK_BYTES];

    // The greeting is a fixed 9 bytes and is *not* a frame: it precedes framing
    // entirely, which is what makes "hello first" a checkable property rather
    // than a convention.
    while buffered.len() < HANDSHAKE_BYTES {
        let read = tokio::select! {
            biased;
            _ = stop.recv() => return,
            read = reader.read(&mut chunk) => read,
        };
        match read {
            Ok(0) => {
                let _ = events.send(AppEvent::Eof).await;
                return;
            }
            Ok(read) => buffered.extend_from_slice(&chunk[..read]),
            Err(_) => {
                let _ = events
                    .send(AppEvent::Failed(AppdError::TransportClosed))
                    .await;
                return;
            }
        }
    }
    let greeting_bytes: [u8; HANDSHAKE_BYTES] =
        buffered[..HANDSHAKE_BYTES].try_into().ok().unwrap();
    buffered.drain(..HANDSHAKE_BYTES);

    match i2pr_app_proto::Handshake::decode(&greeting_bytes) {
        Ok(greeting) => {
            if events.send(AppEvent::Greeting(greeting)).await.is_err() {
                return;
            }
        }
        Err(error) => {
            let _ = events
                .send(AppEvent::Failed(AppdError::Contract(error)))
                .await;
            return;
        }
    }

    loop {
        loop {
            if buffered.is_empty() {
                break;
            }
            match Frame::decode(&buffered) {
                Ok((frame, consumed)) => {
                    buffered.drain(..consumed);
                    if events.send(AppEvent::Frame(frame)).await.is_err() {
                        return;
                    }
                }
                Err(ContractError::TruncatedFrame) => break,
                Err(error) => {
                    let _ = events
                        .send(AppEvent::Failed(AppdError::Contract(error)))
                        .await;
                    return;
                }
            }
        }
        if buffered.len() > MAX_APP_FRAME_BYTES {
            let _ = events
                .send(AppEvent::Failed(AppdError::Contract(
                    ContractError::LimitExceeded("frame payload"),
                )))
                .await;
            return;
        }

        let read = tokio::select! {
            biased;
            _ = stop.recv() => return,
            read = reader.read(&mut chunk) => read,
        };
        match read {
            Ok(0) => {
                let _ = events.send(AppEvent::Eof).await;
                return;
            }
            Ok(read) => buffered.extend_from_slice(&chunk[..read]),
            Err(_) => {
                let _ = events
                    .send(AppEvent::Failed(AppdError::TransportClosed))
                    .await;
                return;
            }
        }
    }
}

/// One live application session.
pub struct AppSession {
    authority: LaunchAuthority,
    session: ManagerSessionId,
    link: Arc<ManagerLink>,
    apphost: Option<LaunchedApphost>,
    writer: Option<Box<dyn AsyncWrite + Unpin + Send>>,
    events: mpsc::Receiver<AppEvent>,
    manager_events: mpsc::Receiver<ManagerEvent>,
    stop: Option<mpsc::Sender<()>>,
    reader_task: Option<JoinHandle<()>>,
    state: AppdState,
    limits: SessionLimits,
    /// Application stream id → daemon-issued handle.
    streams: BTreeMap<u32, ManagerServiceStreamId>,
    local_services: BTreeMap<u32, i2pr_app_manager_proto::ManagerLocalServiceId>,
    protocol_minor: u8,
    next_incoming_stream: u32,
    next_local_service: u32,
    /// Set once the daemon has ended the session, so teardown does not ask it to
    /// close something that is already gone.
    manager_ended: bool,
}

impl AppSession {
    /// Starts a session over an already-launched apphost.
    ///
    /// The apphost has reported `ready`, so the application process exists; this
    /// owns it from here and is the only thing that can end it. This is the only
    /// constructor production uses, and the only one that is given a process.
    pub fn start(
        authority: LaunchAuthority,
        session: ManagerSessionId,
        link: Arc<ManagerLink>,
        mut apphost: LaunchedApphost,
        manager_events: mpsc::Receiver<ManagerEvent>,
    ) -> Result<Self, AppdError> {
        let Some((from_host, to_host)) = apphost.take_transport() else {
            return Err(AppdError::App("apphost transport was already taken"));
        };
        Ok(Self::attach(
            authority,
            session,
            link,
            Some(apphost),
            manager_events,
            from_host,
            Box::new(to_host),
        ))
    }

    /// Attaches a session to an application transport that was not produced by
    /// [`AppSession::start`].
    ///
    /// This exists so the session *protocol* — hello matching, capability
    /// presentation, stream mapping, teardown — can be qualified against an
    /// in-memory application without spawning one. It deliberately does **not**
    /// take authority or a launch request: those still come only from
    /// [`crate::authority::LaunchAuthority`], which has no decoder. WP5 exercises
    /// the real path, with a real apphost process and a real application.
    #[allow(clippy::too_many_arguments)]
    pub fn attach(
        authority: LaunchAuthority,
        session: ManagerSessionId,
        link: Arc<ManagerLink>,
        apphost: Option<LaunchedApphost>,
        manager_events: mpsc::Receiver<ManagerEvent>,
        from_host: impl AsyncRead + Unpin + Send + 'static,
        to_host: Box<dyn AsyncWrite + Unpin + Send>,
    ) -> Self {
        let (events_tx, events) = mpsc::channel(MAX_SESSION_EVENT_QUEUE);
        let (stop, stop_rx) = mpsc::channel(1);
        let reader_task = tokio::spawn(read_application(from_host, events_tx, stop_rx));

        Self {
            state: AppdState::Launching,
            authority,
            session,
            link,
            apphost,
            writer: Some(to_host),
            events,
            manager_events,
            stop: Some(stop),
            reader_task: Some(reader_task),
            limits: SessionLimits::default(),
            streams: BTreeMap::new(),
            local_services: BTreeMap::new(),
            protocol_minor: 0,
            next_incoming_stream: u32::MAX,
            next_local_service: 1,
            manager_ended: false,
        }
    }

    pub const fn state(&self) -> AppdState {
        self.state
    }

    /// The daemon session this application is bound to.
    pub const fn session(&self) -> ManagerSessionId {
        self.session
    }

    /// Application stream ids this session currently holds open.
    pub fn open_streams(&self) -> usize {
        self.streams.len()
    }

    /// Effective capabilities this launch was admitted with.
    pub fn capabilities(&self) -> &[i2pr_app_proto::Capability] {
        self.authority.capabilities()
    }

    /// Drives the session until the application ends, the daemon ends it, or the
    /// application violates the contract.
    ///
    /// Every exit path releases the same resources: the reader task, the
    /// application write half, the daemon session route, and the apphost child.
    pub async fn run(mut self) -> SessionEnd {
        let end = self.drive().await;
        self.teardown().await;
        end
    }

    async fn drive(&mut self) -> SessionEnd {
        match self.greet_and_hello().await {
            Ok(()) => {}
            Err(end) => return end,
        }
        self.loop_running().await
    }

    /// Phase one: greeting, then exactly one hello.
    async fn greet_and_hello(&mut self) -> Result<(), SessionEnd> {
        let greeting = match self.next_app_event().await {
            Some(AppEvent::Greeting(greeting)) => greeting,
            Some(AppEvent::Eof) => return Err(SessionEnd::ApplicationClosed),
            Some(AppEvent::Failed(error)) => return Err(SessionEnd::Refused(error)),
            // A frame cannot precede the greeting: `read_application` has not
            // emitted one yet, so this arm is unreachable by construction.
            Some(AppEvent::Frame(_)) | None => {
                return Err(SessionEnd::Refused(AppdError::App(
                    "application presented a frame before its greeting",
                )));
            }
        };
        if greeting.role != i2pr_app_proto::Role::Application {
            // An administrator-role process reached an application session. It
            // is not a lesser case; it is the wrong principal entirely.
            return Err(SessionEnd::Refused(AppdError::Contract(
                ContractError::RoleMismatch,
            )));
        }
        self.state = AppdState::AwaitingHello;

        let hello = match self.next_app_event().await {
            Some(AppEvent::Frame(frame)) => {
                if frame.kind != FrameKind::Control {
                    return Err(SessionEnd::Refused(AppdError::App(
                        "application sent data before hello",
                    )));
                }
                match i2pr_app_proto::decode_app_to_host_control(&frame.payload) {
                    Ok(message) => message,
                    Err(error) => return Err(SessionEnd::Refused(AppdError::Contract(error))),
                }
            }
            Some(AppEvent::Eof) => return Err(SessionEnd::ApplicationClosed),
            Some(AppEvent::Failed(error)) => return Err(SessionEnd::Refused(error)),
            Some(AppEvent::Greeting(_)) => {
                return Err(SessionEnd::Refused(AppdError::App(
                    "application presented a second greeting",
                )));
            }
            None => return Err(SessionEnd::ApplicationClosed),
        };
        let AppToHostMessage::Hello {
            request_id,
            app_id,
            instance_id,
            protocol_major,
            protocol_minor,
        } = hello
        else {
            // The first application request is not `hello`. There is no "let it
            // try again" here: the launch authority names this exact message as
            // the only thing that may come first.
            return Err(SessionEnd::Refused(AppdError::App(
                "the first application request was not hello",
            )));
        };

        if let Err(error) =
            self.match_identity(&app_id, &instance_id, protocol_major, protocol_minor)
        {
            return Err(SessionEnd::Refused(error));
        }

        self.limits
            .register_request(u32::from(request_id))
            .map_err(|error| SessionEnd::Refused(AppdError::Contract(error)))?;
        if let Err(error) = self
            .write_to_app(&HostToAppMessage::Reply {
                request_id,
                outcome: AppRequestOutcome::Succeeded,
            })
            .await
        {
            // The lease is released here too: a session that ends with an
            // outstanding request must not keep its slot against the ceiling.
            let _ = self.limits.complete_request(request_id);
            return Err(SessionEnd::Refused(error));
        }
        self.limits
            .complete_request(request_id)
            .map_err(|error| SessionEnd::Refused(AppdError::Contract(error)))?;

        // The effective capability set the application is now held to. It is the
        // authority's, sent once, and there is no message that can change it.
        self.protocol_minor = protocol_minor;
        let capabilities = self
            .authority
            .capabilities()
            .iter()
            .copied()
            .filter(|cap| *cap != i2pr_app_proto::Capability::LocalService || protocol_minor >= 1)
            .collect();
        if let Err(error) = self
            .write_to_app(&HostToAppMessage::Capabilities { capabilities })
            .await
        {
            return Err(SessionEnd::Refused(error));
        }
        self.state = AppdState::Running;
        Ok(())
    }

    async fn loop_running(&mut self) -> SessionEnd {
        loop {
            tokio::select! {
                biased;
                event = self.manager_events.recv() => match event {
                    Some(ManagerEvent::SessionEnded { reason }) => {
                        self.manager_ended = true;
                        return SessionEnd::ManagerEnded(reason);
                    }
                    Some(ManagerEvent::StreamEnded { stream, reason }) => {
                        if let Err(error) = self.on_stream_ended(stream, reason).await {
                            return SessionEnd::Refused(error);
                        }
                    }
                    Some(ManagerEvent::Data { stream, payload }) => {
                        if let Err(error) = self.on_backend_data(stream, payload).await {
                            return SessionEnd::Refused(error);
                        }
                    }
                    Some(ManagerEvent::LocalServiceIncoming { service, stream }) => {
                        if let Err(error) = self.on_local_service_incoming(service, stream).await {
                            return SessionEnd::Refused(error);
                        }
                    }
                    // The link dropped every route: the daemon is gone.
                    None => {
                        self.manager_ended = true;
                        return SessionEnd::ManagerEnded(ServiceEndReason::SessionEnded);
                    }
                },
                event = self.events.recv() => match event {
                    Some(AppEvent::Greeting(_)) => return SessionEnd::Refused(AppdError::App(
                        "application presented a second greeting",
                    )),
                    Some(AppEvent::Eof) => return SessionEnd::ApplicationClosed,
                    Some(AppEvent::Failed(error)) => return SessionEnd::Refused(error),
                    Some(AppEvent::Frame(frame)) => {
                        match self.on_frame(frame).await {
                            Ok(Some(end)) => return end,
                            Ok(None) => {}
                            Err(error) => return SessionEnd::Refused(error),
                        }
                    }
                    None => return SessionEnd::ApplicationClosed,
                },
            }
        }
    }

    /// The one identity check. Everything about the launch is decided here.
    fn match_identity(
        &self,
        app_id: &i2pr_app_proto::AppId,
        instance_id: &i2pr_app_proto::AppInstanceId,
        protocol_major: u8,
        protocol_minor: u8,
    ) -> Result<(), AppdError> {
        if app_id != &self.authority.principal().app_id {
            return Err(AppdError::App("hello declared a different app id"));
        }
        if instance_id != self.authority.instance_id() {
            return Err(AppdError::App("hello declared a different instance id"));
        }
        if protocol_major != PROTOCOL_MAJOR || protocol_minor > PROTOCOL_MINOR {
            return Err(AppdError::Contract(ContractError::UnsupportedVersion));
        }
        Ok(())
    }

    async fn on_frame(&mut self, frame: Frame) -> Result<Option<SessionEnd>, AppdError> {
        if frame.kind == FrameKind::Control {
            // `Frame::decode` enforces control ⇔ stream id 0, so this is the only
            // shape a control frame can have here.
            let message = i2pr_app_proto::decode_app_to_host_control(&frame.payload)
                .map_err(AppdError::Contract)?;
            self.on_control(message).await
        } else {
            self.on_app_data(frame).await
        }
    }

    async fn on_control(
        &mut self,
        message: AppToHostMessage,
    ) -> Result<Option<SessionEnd>, AppdError> {
        match message {
            // Exactly-once: the second hello terminates the launch rather than
            // being ignored, because an application that re-declares identity is
            // an application whose identity cannot be trusted.
            AppToHostMessage::Hello { .. } => {
                Err(AppdError::App("application sent a second hello"))
            }
            AppToHostMessage::PermissionRequest {
                request_id,
                capabilities: _,
            } => {
                // Deterministic denial, no mutation, no persistence. The
                // authority's capabilities are behind no `&mut` path, so there is
                // nothing for this branch to change even if it wanted to.
                self.limits
                    .register_request(u32::from(request_id))
                    .map_err(AppdError::Contract)?;
                let written = self
                    .write_to_app(&HostToAppMessage::PermissionReply {
                        request_id,
                        status: PermissionStatus::Denied,
                    })
                    .await;
                let _ = self.limits.complete_request(request_id);
                written?;
                Ok(None)
            }
            AppToHostMessage::Open {
                request_id,
                stream_id,
                service,
            } => self.on_open(request_id, stream_id, service).await,
            AppToHostMessage::Close { stream_id } => {
                self.on_close(stream_id).await?;
                Ok(None)
            }
            AppToHostMessage::Reset { stream_id, reason } => {
                self.on_reset(stream_id, reason).await?;
                Ok(None)
            }
            // No UI host exists in Plan 369, so there is nothing to bridge to.
            AppToHostMessage::UiMessage { .. } => {
                Err(AppdError::App("ui_message has no host in Plan 369"))
            }
            AppToHostMessage::PublishLocalService {
                request_id,
                service_name,
                preferred_port,
            } => {
                self.publish_local_service(request_id, service_name, preferred_port)
                    .await?;
                Ok(None)
            }
            AppToHostMessage::UnpublishLocalService {
                request_id,
                service_id,
            } => {
                self.unpublish_local_service(request_id, service_id).await?;
                Ok(None)
            }
        }
    }

    async fn on_open(
        &mut self,
        request_id: RequestId,
        stream_id: u32,
        service: AppService,
    ) -> Result<Option<SessionEnd>, AppdError> {
        self.limits
            .register_request(u32::from(request_id))
            .map_err(AppdError::Contract)?;
        let outcome = self.open_stream(stream_id, service).await;
        // The lease is released on every path, including the fatal ones, so a
        // failed open cannot leak a slot against the in-flight ceiling.
        let released = self
            .limits
            .complete_request(request_id)
            .map_err(AppdError::Contract);
        let outcome = match (outcome, released) {
            (Err(failure), _) => Err(failure),
            // A lease that could not be retired is a contract failure in the
            // session's own accounting, not a refusal of the open.
            (Ok(_), Err(error)) => Err(OpenFailure::Fatal(error)),
            (Ok(result), Ok(())) => Ok(result),
        };

        let reply = match outcome {
            Ok(()) => HostToAppMessage::Reply {
                request_id,
                outcome: AppRequestOutcome::Succeeded,
            },
            // A refusal is an answer: the application is told and the session
            // continues.
            Err(OpenFailure::Refused(refusal)) => HostToAppMessage::Reply {
                request_id,
                outcome: AppRequestOutcome::Failed(RequestError {
                    code: refusal.code(),
                    diagnostic: Some(refusal.diagnostic().to_owned()),
                }),
            },
            // A protocol or transport failure terminates the launch.
            Err(OpenFailure::Fatal(error)) => return Err(error),
        };
        self.write_to_app(&reply).await?;
        Ok(None)
    }

    async fn publish_local_service(
        &mut self,
        request_id: RequestId,
        service_name: String,
        preferred_port: Option<u16>,
    ) -> Result<(), AppdError> {
        self.limits
            .register_request(u32::from(request_id))
            .map_err(AppdError::Contract)?;
        let response = if self.protocol_minor < 1
            || !self
                .authority
                .permits(i2pr_app_proto::Capability::LocalService)
        {
            HostToAppMessage::Reply {
                request_id,
                outcome: AppRequestOutcome::Failed(RequestError {
                    code: RequestErrorCode::PermissionDenied,
                    diagnostic: Some("local_service capability is not granted".into()),
                }),
            }
        } else if self.local_services.len() >= i2pr_app_proto::MAX_LOCAL_SERVICES {
            HostToAppMessage::Reply {
                request_id,
                outcome: AppRequestOutcome::Failed(RequestError {
                    code: RequestErrorCode::ResourceLimit,
                    diagnostic: Some("local service limit reached".into()),
                }),
            }
        } else {
            let manager_request = self.link.allocate_request_id();
            match self
                .link
                .request(ManagerToDaemonMessage::PublishLocalService {
                    request_id: manager_request,
                    session: self.session,
                    service_name,
                    preferred_port,
                })
                .await
            {
                Ok(DaemonToManagerMessage::LocalServicePublished { service, port, .. }) => {
                    let app_id = self.next_local_service;
                    self.next_local_service = self
                        .next_local_service
                        .checked_add(1)
                        .ok_or(AppdError::App("local service ids exhausted"))?;
                    self.local_services.insert(app_id, service);
                    HostToAppMessage::LocalServicePublished {
                        request_id,
                        service_id: app_id,
                        port,
                    }
                }
                Ok(DaemonToManagerMessage::Rejected { error, .. }) => HostToAppMessage::Reply {
                    request_id,
                    outcome: AppRequestOutcome::Failed(RequestError {
                        code: match error.code {
                            ManagerErrorCode::PermissionDenied => {
                                RequestErrorCode::PermissionDenied
                            }
                            ManagerErrorCode::ResourceLimit => RequestErrorCode::ResourceLimit,
                            ManagerErrorCode::Conflict => RequestErrorCode::Conflict,
                            _ => RequestErrorCode::InvalidRequest,
                        },
                        diagnostic: Some("daemon refused local service".into()),
                    }),
                },
                Ok(_) => {
                    return Err(AppdError::App(
                        "daemon answered local-service publish with unrelated message",
                    ));
                }
                Err(error) => return Err(error),
            }
        };
        self.limits
            .complete_request(request_id)
            .map_err(AppdError::Contract)?;
        self.write_to_app(&response).await
    }

    async fn unpublish_local_service(
        &mut self,
        request_id: RequestId,
        app_service_id: u32,
    ) -> Result<(), AppdError> {
        self.limits
            .register_request(u32::from(request_id))
            .map_err(AppdError::Contract)?;
        let service = self.local_services.remove(&app_service_id);
        let result = if let Some(service) = service {
            let manager_request = self.link.allocate_request_id();
            self.link
                .request(ManagerToDaemonMessage::UnpublishLocalService {
                    request_id: manager_request,
                    session: self.session,
                    service,
                })
                .await
        } else {
            Err(AppdError::App("unknown local service"))
        };
        let _ = self.limits.complete_request(request_id);
        match result {
            Ok(DaemonToManagerMessage::LocalServiceUnpublished { .. }) => {
                self.write_to_app(&HostToAppMessage::LocalServiceUnpublished {
                    request_id,
                    service_id: app_service_id,
                })
                .await
            }
            Ok(DaemonToManagerMessage::Rejected { error, .. }) => {
                self.write_to_app(&HostToAppMessage::Reply {
                    request_id,
                    outcome: AppRequestOutcome::Failed(RequestError {
                        code: RequestErrorCode::NotFound,
                        diagnostic: Some(format!("local service refused: {:?}", error.code)),
                    }),
                })
                .await
            }
            Err(AppdError::App(_)) => {
                self.write_to_app(&HostToAppMessage::Reply {
                    request_id,
                    outcome: AppRequestOutcome::Failed(RequestError {
                        code: RequestErrorCode::NotFound,
                        diagnostic: Some("local service not found".into()),
                    }),
                })
                .await
            }
            Err(error) => Err(error),
            _ => Err(AppdError::App(
                "daemon answered local-service unpublish with unrelated message",
            )),
        }
    }

    async fn on_local_service_incoming(
        &mut self,
        service: i2pr_app_manager_proto::ManagerLocalServiceId,
        stream: ManagerServiceStreamId,
    ) -> Result<(), AppdError> {
        if !self.local_services.values().any(|value| *value == service) {
            return Err(AppdError::App("daemon named an unowned local service"));
        }
        let mut app_stream = self.next_incoming_stream;
        while app_stream == 0 || self.streams.contains_key(&app_stream) {
            app_stream = app_stream
                .checked_sub(1)
                .ok_or(AppdError::App("incoming stream ids exhausted"))?;
        }
        self.next_incoming_stream = app_stream.saturating_sub(1);
        self.limits
            .open_stream(app_stream)
            .map_err(AppdError::Contract)?;
        self.streams.insert(app_stream, stream);
        let service_id = self
            .local_services
            .iter()
            .find_map(|(id, value)| (*value == service).then_some(*id))
            .ok_or(AppdError::App("daemon named an unowned local service"))?;
        self.write_to_app(&HostToAppMessage::LocalServiceIncoming {
            service_id,
            stream_id: app_stream,
        })
        .await
    }

    /// Maps one application open onto a manager-protocol service stream.
    async fn open_stream(
        &mut self,
        stream_id: u32,
        service: AppService,
    ) -> Result<(), OpenFailure> {
        let manager_service = map_service(service)?;
        let required = manager_service.required_capability();
        if !self.authority.permits(required) {
            // §11: a capability the launch authority does not hold is refused
            // whether or not the application asked for it first.
            return Err(OpenFailure::Refused(OpenRefusal::CapabilityDenied));
        }
        // The session limit is the session's own statement of which streams
        // exist; it is checked before anything reaches the daemon, so an
        // over-limit or duplicate request allocates nothing at all.
        self.limits
            .open_stream(stream_id)
            .map_err(|_| OpenFailure::Refused(refusal_for_open_error(&self.limits)))?;

        let request_id = self.link.allocate_request_id();
        let reply = self
            .link
            .request(ManagerToDaemonMessage::OpenService {
                request_id,
                session: self.session,
                service: manager_service,
            })
            .await;

        match reply {
            Ok(DaemonToManagerMessage::ServiceOpened { stream, .. }) => {
                self.streams.insert(stream_id, stream);
                Ok(())
            }
            Ok(DaemonToManagerMessage::Rejected { error, .. }) => {
                self.limits.close_stream(stream_id);
                Err(OpenFailure::Refused(OpenRefusal::DaemonRefused(error.code)))
            }
            Ok(_) => {
                self.limits.close_stream(stream_id);
                Err(OpenFailure::Fatal(AppdError::App(
                    "daemon answered an open with an unrelated message",
                )))
            }
            Err(error) => {
                self.limits.close_stream(stream_id);
                Err(OpenFailure::Fatal(error))
            }
        }
    }

    async fn on_close(&mut self, stream_id: u32) -> Result<(), AppdError> {
        // Closing a stream this session does not hold is a no-op, not a
        // violation: a close routinely arrives after a backend already ended the
        // stream, and refusing it would punish the application for the daemon's
        // timing.
        let Some(handle) = self.streams.remove(&stream_id) else {
            return Ok(());
        };
        self.limits.close_stream(stream_id);
        let request_id = self.link.allocate_request_id();
        let _ = self
            .link
            .request(ManagerToDaemonMessage::CloseService {
                request_id,
                session: self.session,
                stream: handle,
            })
            .await?;
        self.link.unbind_stream(handle).await;
        Ok(())
    }

    async fn on_reset(&mut self, stream_id: u32, reason: String) -> Result<(), AppdError> {
        let Some(handle) = self.streams.remove(&stream_id) else {
            return Ok(());
        };
        self.limits.close_stream(stream_id);
        let request_id = self.link.allocate_request_id();
        let _ = self
            .link
            .request(ManagerToDaemonMessage::ResetService {
                request_id,
                session: self.session,
                stream: handle,
                reason,
            })
            .await?;
        self.link.unbind_stream(handle).await;
        Ok(())
    }

    async fn on_app_data(&mut self, frame: Frame) -> Result<Option<SessionEnd>, AppdError> {
        let Some(handle) = self.streams.get(&frame.stream_id).copied() else {
            // A data frame on a stream this session never opened. Forwarding it
            // would let an application speak for a handle it does not own, so the
            // launch is refused rather than the frame ignored.
            return Err(AppdError::App("data on a stream that is not open"));
        };
        self.link.send_data(handle, frame.payload).await?;
        Ok(None)
    }

    async fn on_backend_data(
        &mut self,
        stream: ManagerServiceStreamId,
        payload: Vec<u8>,
    ) -> Result<(), AppdError> {
        let Some(app_stream) = self.app_stream_for(stream) else {
            // Data for a handle this session no longer holds. This is the
            // daemon's end-watcher racing its own data pump; the bytes are
            // dropped rather than delivered to an application that has already
            // been told the stream ended.
            return Ok(());
        };
        let bytes = Frame {
            kind: FrameKind::Data,
            stream_id: app_stream,
            payload,
        }
        .encode()
        .map_err(AppdError::Contract)?;
        let Some(writer) = self.writer.as_mut() else {
            return Err(AppdError::TransportClosed);
        };
        writer
            .write_all(&bytes)
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        writer.flush().await.map_err(|_| AppdError::TransportClosed)
    }

    async fn on_stream_ended(
        &mut self,
        stream: ManagerServiceStreamId,
        reason: ServiceEndReason,
    ) -> Result<(), AppdError> {
        // The binding goes first: any data frame already in flight for this
        // stream then fails routing in the link and is counted, instead of
        // reaching an application that has already been told the stream ended.
        self.link.unbind_stream(stream).await;
        let Some(app_stream) = self.app_stream_for(stream) else {
            // A terminal notification for a stream this session already
            // released. Dropping it is correct.
            return Ok(());
        };
        self.streams.remove(&app_stream);
        self.limits.close_stream(app_stream);
        let message = match reason {
            ServiceEndReason::ManagerClosed | ServiceEndReason::SessionEnded => {
                HostToAppMessage::StreamClosed {
                    stream_id: app_stream,
                }
            }
            ServiceEndReason::BackendClosed | ServiceEndReason::BackendRejected => {
                HostToAppMessage::StreamReset {
                    stream_id: app_stream,
                    reason: diagnostic(reason),
                }
            }
        };
        self.write_to_app(&message).await
    }

    fn app_stream_for(&self, handle: ManagerServiceStreamId) -> Option<u32> {
        self.streams
            .iter()
            .find(|(_, mapped)| **mapped == handle)
            .map(|(app_stream, _)| *app_stream)
    }

    /// Waits for the next application event inside the greeting/hello window,
    /// where the bound applies.
    async fn next_app_event(&mut self) -> Option<AppEvent> {
        match tokio::time::timeout(APP_HELLO_GRACE, self.events.recv()).await {
            Ok(event) => event,
            // A launch that never greets is a launch that failed, not a session
            // that is merely slow.
            Err(_) => Some(AppEvent::Failed(AppdError::App(
                "application did not greet within the bound",
            ))),
        }
    }

    async fn write_to_app(&mut self, message: &HostToAppMessage) -> Result<(), AppdError> {
        let payload =
            i2pr_app_proto::encode_host_to_app_control(message).map_err(AppdError::Contract)?;
        let bytes = Frame {
            kind: FrameKind::Control,
            stream_id: 0,
            payload,
        }
        .encode()
        .map_err(AppdError::Contract)?;
        let Some(writer) = self.writer.as_mut() else {
            return Err(AppdError::TransportClosed);
        };
        writer
            .write_all(&bytes)
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        writer.flush().await.map_err(|_| AppdError::TransportClosed)
    }

    /// Releases everything, in the only order that lets the child exit.
    async fn teardown(&mut self) {
        // The application's stdin closes first: EOF is how an application learns
        // to stop, and holding the write half until after the wait means every
        // teardown burns the apphost's full exit grace before being killed.
        self.writer = None;
        drop(self.stop.take());
        if let Some(task) = self.reader_task.take() {
            let _ = task.await;
        }

        // Tell the daemon the session is over before releasing the route, so the
        // gateway state it holds is torn down by the daemon rather than leaking
        // until its own transport dies.
        if !self.manager_ended {
            let request_id = self.link.allocate_request_id();
            let _ = self
                .link
                .request_within(
                    ManagerToDaemonMessage::CloseSession {
                        request_id,
                        session: self.session,
                    },
                    SESSION_CLOSE_GRACE,
                )
                .await;
        }
        self.manager_events.close();
        let handles: Vec<ManagerServiceStreamId> = self.streams.values().copied().collect();
        for handle in handles {
            self.link.unbind_stream(handle).await;
        }
        self.streams.clear();
        self.link.drop_session(self.session).await;
        if let Some(apphost) = self.apphost.take() {
            let _ = apphost.finish().await;
        }

        self.state = match self.state {
            AppdState::Running | AppdState::Launching | AppdState::AwaitingHello => {
                AppdState::Stopping
            }
            other => other,
        };
        self.state = crate::transition(self.state, AppdState::Closed).unwrap_or(AppdState::Closed);
    }
}

/// Why an open did not produce a stream.
enum OpenFailure {
    /// The application gets a typed refusal and the session continues.
    Refused(OpenRefusal),
    /// The launch is terminated.
    Fatal(AppdError),
}

impl From<AppdError> for OpenFailure {
    fn from(value: AppdError) -> Self {
        Self::Fatal(value)
    }
}

impl From<OpenRefusal> for OpenFailure {
    fn from(value: OpenRefusal) -> Self {
        Self::Refused(value)
    }
}

/// Classifies a `SessionLimits` refusal without inspecting its message.
///
/// `SessionLimits` refuses with `InvalidControl` for a zero or duplicate id and
/// `LimitExceeded("streams")` at the ceiling. The ceiling is recoverable from
/// the count itself; the two `InvalidControl` cases are not distinguishable from
/// the outside, and both mean "this stream id is not available", so they share
/// one refusal code. That is deliberate rather than a shortcut: an application
/// gets the same answer either way, so nothing is disclosed.
fn refusal_for_open_error(limits: &SessionLimits) -> OpenRefusal {
    if limits.stream_count() >= i2pr_app_proto::MAX_STREAMS {
        OpenRefusal::StreamLimit
    } else {
        OpenRefusal::DuplicateStream
    }
}

/// Maps the application service vocabulary onto the manager protocol.
///
/// `control_scoped` has no manager-protocol spelling, and inventing one would be
/// exactly the "available unless separately implemented" creep §11 forbids. It is
/// refused by type, so the gap cannot be reached by a request that happens to
/// carry the right capability.
fn map_service(service: AppService) -> Result<ManagerService, OpenRefusal> {
    match service {
        AppService::Sam => Ok(ManagerService::Sam),
        AppService::I2cp => Ok(ManagerService::I2cp),
        AppService::ControlScoped => Err(OpenRefusal::UnsupportedService),
    }
}

/// Kept next to the message it builds so the two cannot drift.
fn diagnostic(reason: ServiceEndReason) -> String {
    match reason {
        ServiceEndReason::BackendClosed => "backend closed".to_owned(),
        ServiceEndReason::BackendRejected => "backend rejected the connection".to_owned(),
        ServiceEndReason::ManagerClosed => "manager closed the stream".to_owned(),
        ServiceEndReason::SessionEnded => "session ended".to_owned(),
    }
}
