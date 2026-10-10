//! Plan 368 daemon-side trusted AppManager bridge.
//!
//! This module consumes the private manager protocol (`i2pr-app-manager-proto`)
//! over an **injected** reliable duplex byte stream and projects an
//! already-authenticated manager-supplied principal into the existing Plan 355
//! `AppGatewaySession`.
//!
//! # Authority
//!
//! The bridge never constructs authorization from an application's `hello`, a
//! `RequestedCapability`, manifest bytes, or service data bytes. It has no
//! decoder from any of those into `AppGatewayAuthorization`. Only a
//! `create_session` request on the already-trusted manager transport supplies a
//! principal and effective grants, and those grants are still re-derived through
//! the existing administrator path before any backend is allocated.
//!
//! # Deliberate non-features
//!
//! - The private manager transport remains injected and anonymous. The daemon
//!   alone owns explicitly granted loopback local-service listeners.
//! - No package, grant-persistence, launch-profile, process, or router-config
//!   authority. The protocol vocabulary cannot express them.
//! - `control_scoped` is unrepresentable in the service vocabulary and is refused
//!   before any side effect.
//!
//! # Lifecycle
//!
//! One manager transport maps to one `AppManagerBridge`. Every accepted session
//! maps to exactly one `AppGatewaySession` bound to one `AppInstanceId`, with
//! service handles local to that session. Manager transport EOF cancels every
//! descendant gateway session and backend connection.
//!
//! # Backpressure
//!
//! Every hop is bounded: a single reader task owns the inbound control loop, a
//! single writer task owns the transport write half and is fed by a bounded
//! channel, and each service stream's in-memory duplex plus its inbound queue are
//! capped. A slow manager fills a bounded queue and stalls the reader; it can
//! never cause unbounded buffering.

// Plan 368 closes the router-facing contract and bridge; Plan 369 is the
// production caller that supervises `i2pr-appd` over the concrete inherited
// transport. Until then the bridge is exercised by its own module and
// integration tests, exactly as `app_gateway.rs` was between Plans 355 and 368.
// The allowance is scoped to this module and names its own reason.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, EffectiveGrant, Frame, FrameKind, Handshake, ManagerError,
    ManagerErrorCode, ManagerGatewayLimits, ManagerLocalServiceId, ManagerProtocolError,
    ManagerScopeLimits, ManagerService, ManagerServiceStreamId, ManagerSessionId,
    ManagerToDaemonMessage, ServiceEndReason, ServiceStreamLedger,
};
use i2pr_app_proto::{
    AdministratorPrincipal, AppService, EffectiveCapabilities, GrantedCapability, RequestId,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::app_gateway::{
    AppGatewayAuthorization, AppGatewayComposition, AppGatewayError, AppGatewayLimits,
    AppGatewaySession,
};
use crate::config::{I2cpConfig, SamConfig};

/// Bound on queued outbound frames waiting for the single writer task.
const MAX_QUEUED_OUTBOUND_FRAMES: usize = 64;
/// Bound on queued inbound octets waiting for one backend driver.
const MAX_QUEUED_INBOUND_CHUNKS: usize = 64;

/// Injected reliable duplex byte stream suitable for in-memory qualification.
///
/// This is deliberately a narrow `AsyncRead + AsyncWrite` object rather than a
/// concrete socket type: Plan 368 chooses no transport, and Plan 369 supplies the
/// anonymous inherited one.
pub(crate) trait ManagerTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T> ManagerTransport for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

/// Daemon-owned composition inputs shared by every session on one transport.
pub(crate) struct AppManagerComposition {
    pub(crate) sam: SamConfig,
    pub(crate) i2cp: I2cpConfig,
    pub(crate) addressbook: crate::addressbook::SharedAddressBook,
    /// Child scope parented to the bridge owner; every gateway session and every
    /// backend pump is spawned under it.
    pub(crate) children: ChildScope,
    /// Parent cancellation retained so session drop tears down all descendant
    /// gateway state.
    pub(crate) cancellation: CancellationToken,
}

/// The single owner of the transport's write half.
///
/// Every outbound frame — control reply or backend notification — passes through
/// this task, so writes cannot interleave or reorder.
struct OutboundWriter(mpsc::Sender<Vec<u8>>);

impl OutboundWriter {
    /// Queues one already-encoded frame. A full queue means the manager is not
    /// draining, so the reader stalls rather than buffering without bound.
    async fn send(&self, frame: Frame) -> Result<(), AppManagerBridgeError> {
        let bytes = frame.encode().map_err(AppManagerBridgeError::Protocol)?;
        self.0
            .send(bytes)
            .await
            .map_err(|_| AppManagerBridgeError::Protocol(ManagerProtocolError::TransportClosed))
    }
}

/// One live manager session: exactly one gateway session plus its local
/// service-stream registry, with the cancellation root that reaches it.
struct Session {
    state: Mutex<SessionState>,
    /// Cancellation root for this session's gateway state and backend pumps.
    cancellation: CancellationToken,
}

struct SessionState {
    gateway: AppGatewaySession,
    streams: ServiceStreamLedger,
    /// Per-session backend connections. Keys are session-local handles, so a
    /// handle belonging to another session is simply absent.
    connections: BTreeMap<ManagerServiceStreamId, StreamState>,
    /// Bounded inbound byte queues, keyed by the same session-local handle.
    inbound: BTreeMap<ManagerServiceStreamId, mpsc::Sender<Vec<u8>>>,
    local_services: BTreeMap<ManagerLocalServiceId, LocalServiceState>,
    local_service_names: BTreeSet<String>,
    local_service_authorized: bool,
}

struct LocalServiceState {
    name: String,
    cancellation: CancellationToken,
}

/// One live service stream.
///
/// The `AppGatewayConnection` itself is owned by the backend-watcher task, which
/// is the only place that may consume it. The registry keeps a per-stream
/// cancellation root so an explicit close or reset reaches that task.
struct StreamState {
    cancellation: CancellationToken,
    local_service: Option<ManagerLocalServiceId>,
}

/// The trusted bridge over one manager transport.
pub(crate) struct AppManagerBridge {
    composition: Arc<AppManagerComposition>,
    sessions: Mutex<BTreeMap<ManagerSessionId, Arc<Session>>>,
    scope: Mutex<ManagerScopeLimits>,
    outbound: Mutex<Option<OutboundWriter>>,
    next_session: AtomicU64,
    next_stream: AtomicU64,
    next_local_service: AtomicU64,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum AppManagerBridgeError {
    /// The manager transport ended. Every descendant gateway session is torn
    /// down before this is reported.
    #[error("manager transport closed")]
    TransportClosed,
    #[error("manager protocol violation: {0}")]
    Protocol(#[from] ManagerProtocolError),
    #[error("bridge child scope rejected a task")]
    ResourceLimit,
}

impl AppManagerBridge {
    pub(crate) fn new(composition: AppManagerComposition) -> Self {
        Self {
            composition: Arc::new(composition),
            sessions: Mutex::new(BTreeMap::new()),
            scope: Mutex::new(ManagerScopeLimits::default()),
            outbound: Mutex::new(None),
            next_session: AtomicU64::new(1),
            next_stream: AtomicU64::new(1),
            next_local_service: AtomicU64::new(1),
        }
    }

    /// Handles are never reused within one transport's lifetime, so a stale
    /// handle can never resolve to a different session.
    fn allocate_session(&self) -> Result<ManagerSessionId, AppManagerBridgeError> {
        ManagerSessionId::new(self.next_session.fetch_add(1, Ordering::Relaxed))
            .map_err(AppManagerBridgeError::Protocol)
    }

    fn allocate_stream(&self) -> Result<ManagerServiceStreamId, AppManagerBridgeError> {
        ManagerServiceStreamId::new(self.next_stream.fetch_add(1, Ordering::Relaxed))
            .map_err(AppManagerBridgeError::Protocol)
    }

    /// Drives one manager transport to completion.
    ///
    /// On return — for any reason, including cancellation or protocol violation
    /// — every session and backend connection this bridge created is torn down.
    pub(crate) async fn run<T>(self: &Arc<Self>, transport: T) -> Result<(), AppManagerBridgeError>
    where
        T: ManagerTransport + 'static,
    {
        self.run_announcing_ready(transport, None).await
    }

    /// Drives one manager transport, publishing the handshake outcome on
    /// `ready` before any request is served.
    ///
    /// Plan 369 §B requires that a supervised manager service report readiness
    /// **only after** the manager-protocol handshake succeeds: a child process
    /// that has been spawned is not yet a running manager. The bridge is the
    /// only component that sees the handshake, so the owning service learns
    /// the outcome here rather than inferring it from a timer.
    ///
    /// `ready` is fired exactly once, on either outcome. A dropped receiver
    /// (the service already timed out and tore down) is not an error here.
    pub(crate) async fn run_announcing_ready<T>(
        self: &Arc<Self>,
        transport: T,
        ready: Option<oneshot::Sender<Result<(), AppManagerBridgeError>>>,
    ) -> Result<(), AppManagerBridgeError>
    where
        T: ManagerTransport + 'static,
    {
        let result = self.drive(transport, ready).await;
        self.teardown_all().await;
        result
    }

    async fn drive<T>(
        self: &Arc<Self>,
        transport: T,
        ready: Option<oneshot::Sender<Result<(), AppManagerBridgeError>>>,
    ) -> Result<(), AppManagerBridgeError>
    where
        T: ManagerTransport + 'static,
    {
        // Owned halves, so both tasks below are `'static` children of the bridge
        // scope rather than borrows of the caller.
        let (mut reader, mut writer) = tokio::io::split(transport);

        // The writer task owns the transport write half for the whole lifetime.
        let (sender, mut receiver) = mpsc::channel::<Vec<u8>>(MAX_QUEUED_OUTBOUND_FRAMES);
        let writer_spawned = self.composition.children.spawn(move |_| async move {
            while let Some(frame) = receiver.recv().await {
                if writer.write_all(&frame).await.is_err() {
                    break;
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
            Ok(())
        });
        if writer_spawned.is_err() {
            return Err(AppManagerBridgeError::ResourceLimit);
        }
        *self.outbound.lock().await = Some(OutboundWriter(sender));

        let result = match self.perform_handshake(&mut reader).await {
            Ok(handshake) => {
                if let Some(ready) = ready {
                    let _ = ready.send(Ok(()));
                }
                self.serve(&handshake, &mut reader).await
            }
            Err(error) => {
                if let Some(ready) = ready {
                    let _ = ready.send(Err(error));
                }
                Err(error)
            }
        };
        // Dropping the sender lets the writer drain and exit, so the transport is
        // never left half-written when the bridge returns.
        drop(self.outbound.lock().await.take());
        result
    }

    async fn perform_handshake<R>(&self, reader: &mut R) -> Result<Handshake, AppManagerBridgeError>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = [0_u8; i2pr_app_manager_proto::HANDSHAKE_BYTES];
        reader
            .read_exact(&mut bytes)
            .await
            .map_err(|_| AppManagerBridgeError::Protocol(ManagerProtocolError::TransportClosed))?;
        Ok(Handshake::decode(&bytes)?)
    }

    async fn serve<R>(
        self: &Arc<Self>,
        handshake: &Handshake,
        reader: &mut R,
    ) -> Result<(), AppManagerBridgeError>
    where
        R: AsyncRead + Unpin,
    {
        loop {
            let frame = match self.read_frame(reader).await? {
                Some(frame) => frame,
                None => return Err(AppManagerBridgeError::TransportClosed),
            };
            match frame.kind {
                // A control frame is a request; the daemon answers it exactly
                // once, correlated by the manager's own request id.
                FrameKind::Control => {
                    let message = handshake
                        .decode_manager_to_daemon(&frame.payload)
                        .map_err(AppManagerBridgeError::Protocol)?;
                    for reply in self.dispatch(message).await {
                        self.send_control(reply).await?;
                    }
                }
                // Data is forwarded as exact protocol octets for a named stream,
                // inside the session that owns it. Never parsed or rewritten.
                FrameKind::Data => {
                    let stream = ManagerServiceStreamId::try_from(u64::from(frame.stream_id))
                        .map_err(|_| {
                            AppManagerBridgeError::Protocol(ManagerProtocolError::InvalidHandle)
                        })?;
                    self.forward_data(stream, frame.payload).await?;
                }
            }
        }
    }

    async fn send_control(
        &self,
        message: DaemonToManagerMessage,
    ) -> Result<(), AppManagerBridgeError> {
        let payload = i2pr_app_manager_proto::encode_daemon_to_manager_control(&message)
            .map_err(AppManagerBridgeError::Protocol)?;
        let guard = self.outbound.lock().await;
        match guard.as_ref() {
            Some(writer) => writer.send(Frame::control(payload)).await,
            None => Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::TransportClosed,
            )),
        }
    }

    async fn read_frame<R>(&self, reader: &mut R) -> Result<Option<Frame>, AppManagerBridgeError>
    where
        R: AsyncRead + Unpin,
    {
        let mut header = [0_u8; i2pr_app_manager_proto::FRAME_HEADER_BYTES];
        let filled = read_until_eof(reader, &mut header).await?;
        if filled == 0 {
            return Ok(None);
        }
        if filled < header.len() {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::TruncatedFrame,
            ));
        }
        if header[0] != i2pr_app_manager_proto::FRAME_VERSION || header[2] != 0 || header[3] != 0 {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::UnsupportedFrame,
            ));
        }
        let kind = match header[1] {
            1 => FrameKind::Control,
            2 => FrameKind::Data,
            _ => {
                return Err(AppManagerBridgeError::Protocol(
                    ManagerProtocolError::UnsupportedFrame,
                ));
            }
        };
        let stream_id =
            u32::from_be_bytes(header[4..8].try_into().map_err(|_| {
                AppManagerBridgeError::Protocol(ManagerProtocolError::MalformedFrame)
            })?);
        let declared =
            u32::from_be_bytes(header[8..12].try_into().map_err(|_| {
                AppManagerBridgeError::Protocol(ManagerProtocolError::MalformedFrame)
            })?) as usize;
        // Validate the declared length against the ceiling *before* allocating,
        // so hostile input cannot drive an allocation.
        let limit = match kind {
            FrameKind::Control => i2pr_app_manager_proto::MAX_CONTROL_BYTES,
            FrameKind::Data => i2pr_app_manager_proto::MAX_DATA_FRAME_BYTES,
        };
        if declared > limit {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::LimitExceeded("frame payload"),
            ));
        }
        // Kind/stream discipline is validated before any payload work.
        if (kind == FrameKind::Control) != (stream_id == i2pr_app_manager_proto::CONTROL_STREAM_ID)
        {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::MalformedFrame,
            ));
        }
        let mut payload = vec![0_u8; declared];
        if read_until_eof(reader, &mut payload).await? != declared {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::TruncatedFrame,
            ));
        }
        Ok(Some(Frame {
            kind,
            stream_id,
            payload,
        }))
    }

    /// Handles one validated manager request and returns its terminal reply plus
    /// any notifications that must accompany it.
    ///
    /// Every rejection path is side-effect free: nothing is allocated for a
    /// request the daemon refuses.
    async fn dispatch(
        self: &Arc<Self>,
        message: ManagerToDaemonMessage,
    ) -> Vec<DaemonToManagerMessage> {
        match message {
            ManagerToDaemonMessage::CreateSession {
                request_id,
                principal,
                effective_capabilities,
                limits,
            } => vec![
                self.create_session(request_id, principal, effective_capabilities, limits)
                    .await,
            ],
            ManagerToDaemonMessage::CloseSession {
                request_id,
                session,
            } => {
                if self.close_session(session).await {
                    vec![DaemonToManagerMessage::SessionClosed {
                        request_id,
                        session,
                    }]
                } else {
                    vec![reject(
                        request_id,
                        ManagerErrorCode::NotFound,
                        "unknown session",
                    )]
                }
            }
            ManagerToDaemonMessage::OpenService {
                request_id,
                session,
                service,
            } => self.open_service(request_id, session, service).await,
            ManagerToDaemonMessage::PublishLocalService {
                request_id,
                session,
                service_name,
                preferred_port,
            } => {
                self.publish_local_service(request_id, session, service_name, preferred_port)
                    .await
            }
            ManagerToDaemonMessage::UnpublishLocalService {
                request_id,
                session,
                service,
            } => {
                self.unpublish_local_service(request_id, session, service)
                    .await
            }
            ManagerToDaemonMessage::CloseService {
                request_id,
                session,
                stream,
            } => {
                if self.close_service(session, stream).await {
                    vec![DaemonToManagerMessage::ServiceClosed {
                        request_id,
                        session,
                        stream,
                    }]
                } else {
                    vec![reject(
                        request_id,
                        ManagerErrorCode::NotFound,
                        "unknown service stream",
                    )]
                }
            }
            ManagerToDaemonMessage::ResetService {
                request_id,
                session,
                stream,
                reason: _,
            } => {
                if self.close_service(session, stream).await {
                    vec![DaemonToManagerMessage::ServiceReset {
                        request_id,
                        session,
                        stream,
                    }]
                } else {
                    vec![reject(
                        request_id,
                        ManagerErrorCode::NotFound,
                        "unknown service stream",
                    )]
                }
            }
            ManagerToDaemonMessage::Health { request_id } => {
                vec![DaemonToManagerMessage::HealthStatus {
                    request_id,
                    state: "running".to_owned(),
                    detail: None,
                }]
            }
            ManagerToDaemonMessage::Shutdown {
                request_id,
                reason: _,
            } => {
                vec![DaemonToManagerMessage::ShutdownAck { request_id }]
            }
        }
    }

    async fn create_session(
        self: &Arc<Self>,
        request_id: RequestId,
        principal: i2pr_app_manager_proto::ManagerPrincipal,
        effective_capabilities: Vec<EffectiveGrant>,
        limits: ManagerGatewayLimits,
    ) -> DaemonToManagerMessage {
        // The wire principal must convert to the application contract's principal
        // before it can bind a gateway session; an unconvertible one is refused
        // rather than partially applied.
        let Ok(principal) = principal.to_app_principal() else {
            return reject(
                request_id,
                ManagerErrorCode::InvalidRequest,
                "principal rejected",
            );
        };
        // Authority is re-derived here, not trusted: each manager-asserted grant
        // must survive the administrator-grant path before it can exist.
        let Ok(administrator) = AdministratorPrincipal::from_authenticated_session(1) else {
            return reject(
                request_id,
                ManagerErrorCode::Internal,
                "administrator unavailable",
            );
        };
        let mut grants = Vec::with_capacity(effective_capabilities.len());
        for grant in effective_capabilities {
            match GrantedCapability::from_administrator_policy(&administrator, grant.capability) {
                Ok(granted) => grants.push(granted),
                // A reserved capability is refused outright, never dropped.
                Err(_) => {
                    return reject(
                        request_id,
                        ManagerErrorCode::UnsupportedOperation,
                        "capability cannot be granted",
                    );
                }
            }
        }
        let Ok(effective) = EffectiveCapabilities::from_grants(&grants) else {
            return reject(
                request_id,
                ManagerErrorCode::InvalidRequest,
                "capability set rejected",
            );
        };
        let local_service_authorized = effective
            .as_slice()
            .contains(&i2pr_app_proto::Capability::LocalService);
        let Ok(limits) = AppGatewayLimits::new(limits.max_connections as usize) else {
            return reject(
                request_id,
                ManagerErrorCode::InvalidRequest,
                "connection limit rejected",
            );
        };
        let Ok(session_id) = self.allocate_session() else {
            return reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "handle allocation exhausted",
            );
        };

        // One manager session maps to exactly one gateway session bound to one
        // app instance. No backend state is allocated here.
        let state = SessionState {
            gateway: AppGatewaySession::new(
                AppGatewayAuthorization::from_trusted_composition(principal, effective),
                limits,
                AppGatewayComposition {
                    sam: self.composition.sam.clone(),
                    sam_max_version: i2pr_api::sam::version::MAX_SUPPORTED_VERSION,
                    i2cp: self.composition.i2cp.clone(),
                    addressbook: self.composition.addressbook.clone(),
                    children: ChildScope::child_of(
                        &self.composition.cancellation,
                        ChildFailurePolicy::CollectResult,
                    ),
                    cancellation: self.composition.cancellation.child_token(),
                },
            ),
            streams: ServiceStreamLedger::default(),
            connections: BTreeMap::new(),
            inbound: BTreeMap::new(),
            local_services: BTreeMap::new(),
            local_service_names: BTreeSet::new(),
            local_service_authorized,
        };

        let admitted = self.scope.lock().await.open_session(session_id).is_ok();
        if !admitted {
            return reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "manager session limit reached",
            );
        }
        // A session admitted here is torn down as one unit: the wrapper's
        // cancellation root reaches this gateway session and its pumps.
        self.sessions.lock().await.insert(
            session_id,
            Arc::new(Session {
                state: Mutex::new(state),
                cancellation: self.composition.cancellation.child_token(),
            }),
        );
        DaemonToManagerMessage::SessionOpened {
            request_id,
            session: session_id,
        }
    }

    async fn close_session(self: &Arc<Self>, session: ManagerSessionId) -> bool {
        let removed = self.sessions.lock().await.remove(&session);
        let Some(session_state) = removed else {
            return false;
        };
        // Cancelling the session root tears down every descendant backend
        // connection before the map entry disappears.
        session_state
            .cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
        {
            let mut guard = session_state.state.lock().await;
            // Dropping the inbound senders ends each manager->backend pump,
            // which is what propagates EOF into the private protocol drivers.
            for (_, sender) in std::mem::take(&mut guard.inbound) {
                drop(sender);
            }
            for (_, entry) in std::mem::take(&mut guard.connections) {
                entry
                    .cancellation
                    .cancel(i2pr_core::CancellationReason::ParentScope);
            }
            guard.streams = ServiceStreamLedger::default();
        }
        session_state.state.lock().await.gateway.shutdown().await;
        self.scope.lock().await.close_session(session).is_ok()
    }

    async fn publish_local_service(
        self: &Arc<Self>,
        request_id: RequestId,
        session: ManagerSessionId,
        service_name: String,
        preferred_port: Option<u16>,
    ) -> Vec<DaemonToManagerMessage> {
        let Some(state) = self.sessions.lock().await.get(&session).cloned() else {
            return vec![reject(
                request_id,
                ManagerErrorCode::NotFound,
                "unknown session",
            )];
        };
        {
            let guard = state.state.lock().await;
            if !guard.local_service_authorized {
                return vec![reject(
                    request_id,
                    ManagerErrorCode::PermissionDenied,
                    "local_service capability not granted",
                )];
            }
            if guard.local_services.len() >= i2pr_app_proto::MAX_LOCAL_SERVICES {
                return vec![reject(
                    request_id,
                    ManagerErrorCode::ResourceLimit,
                    "local service ceiling reached",
                )];
            }
            if guard.local_service_names.contains(&service_name) {
                return vec![reject(
                    request_id,
                    ManagerErrorCode::Conflict,
                    "duplicate local service name",
                )];
            }
        }
        let port = preferred_port.unwrap_or(0);
        let listener = match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
            Ok(value) => value,
            Err(_) => {
                return vec![reject(
                    request_id,
                    ManagerErrorCode::Conflict,
                    "loopback port unavailable",
                )];
            }
        };
        let Ok(address) = listener.local_addr() else {
            return vec![reject(
                request_id,
                ManagerErrorCode::Internal,
                "local listener address unavailable",
            )];
        };
        let Ok(service) =
            ManagerLocalServiceId::new(self.next_local_service.fetch_add(1, Ordering::Relaxed))
        else {
            return vec![reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "local service ids exhausted",
            )];
        };
        let cancellation = state.cancellation.child_token();
        {
            let mut guard = state.state.lock().await;
            if guard.local_services.len() >= i2pr_app_proto::MAX_LOCAL_SERVICES
                || !guard.local_service_names.insert(service_name.clone())
            {
                return vec![reject(
                    request_id,
                    ManagerErrorCode::Conflict,
                    "local service changed during bind",
                )];
            }
            guard.local_services.insert(
                service,
                LocalServiceState {
                    name: service_name.clone(),
                    cancellation: cancellation.clone(),
                },
            );
        }
        let bridge = Arc::clone(self);
        let accept_cancel = cancellation.clone();
        if self.composition.children.spawn(move |_| async move {
            loop {
                let accepted = tokio::select! { biased; () = accept_cancel.cancelled() => break, result = listener.accept() => result };
                match accepted { Ok((socket, peer)) if peer.ip().is_loopback() => bridge.accept_local_connection(session, service, socket).await, Ok((_socket, _)) => {}, Err(_) => break }
            }
            Ok(())
        }).is_err() {
            let mut guard = state.state.lock().await;
            guard.local_services.remove(&service);
            guard.local_service_names.remove(&service_name);
            return vec![reject(request_id, ManagerErrorCode::ResourceLimit, "accept task unavailable")];
        }
        vec![DaemonToManagerMessage::LocalServicePublished {
            request_id,
            session,
            service,
            port: address.port(),
        }]
    }

    async fn accept_local_connection(
        self: &Arc<Self>,
        session: ManagerSessionId,
        service: ManagerLocalServiceId,
        socket: TcpStream,
    ) {
        let Some(state) = self.sessions.lock().await.get(&session).cloned() else {
            return;
        };
        let stream = match self.allocate_stream() {
            Ok(value) => value,
            Err(_) => return,
        };
        let (inbound, mut inbound_rx) =
            mpsc::channel::<Vec<u8>>(i2pr_app_manager_proto::MAX_LOCAL_SERVICE_QUEUE_FRAMES);
        let cancellation = state.cancellation.child_token();
        {
            let mut guard = state.state.lock().await;
            if !guard.local_services.contains_key(&service)
                || guard
                    .connections
                    .values()
                    .filter(|entry| entry.local_service.is_some())
                    .count()
                    >= i2pr_app_manager_proto::MAX_LOCAL_SERVICE_CONNECTIONS_PER_SESSION
                || guard.streams.open(stream).is_err()
            {
                return;
            }
            guard.inbound.insert(stream, inbound);
            guard.connections.insert(
                stream,
                StreamState {
                    cancellation: cancellation.clone(),
                    local_service: Some(service),
                },
            );
        }
        let Some(outbound) = self.outbound_sender().await else {
            let _ = self.close_service(session, stream).await;
            return;
        };
        let event = DaemonToManagerMessage::LocalServiceIncoming {
            session,
            service,
            stream,
        };
        if let Ok(payload) = i2pr_app_manager_proto::encode_daemon_to_manager_control(&event)
            && let Ok(bytes) = Frame::control(payload).encode()
            && outbound.send(bytes).await.is_err()
        {
            let _ = self.close_service(session, stream).await;
            return;
        }
        let (mut reader, mut writer) = tokio::io::split(socket);
        let pump_outbound = outbound.clone();
        let pump_cancel = cancellation.clone();
        let write_cancel = cancellation.clone();
        let bridge = Arc::clone(self);
        if self.composition.children.spawn(move |_| async move {
            let mut read_buffer = vec![0_u8; 16 * 1024];
            let write_pump = async { loop { tokio::select! { biased; () = write_cancel.cancelled() => break, chunk = inbound_rx.recv() => match chunk { Some(bytes) if writer.write_all(&bytes).await.is_ok() => {}, _ => break } } } let _ = writer.shutdown().await; };
            let read_pump = async { loop { tokio::select! { biased; () = pump_cancel.cancelled() => break, read = reader.read(&mut read_buffer) => match read { Ok(n) if n > 0 => { let frame = Frame::data(stream, read_buffer[..n].to_vec()); let Ok(frame) = frame.encode() else { break }; if pump_outbound.send(frame).await.is_err() { break; } }, _ => break } } } };
            let ended = tokio::select! { biased; () = pump_cancel.cancelled() => false, _ = write_pump => true, _ = read_pump => true };
            if ended {
                pump_cancel.cancel(i2pr_core::CancellationReason::ParentScope);
                bridge.notify_local_stream_ended(session, stream, ServiceEndReason::BackendClosed).await;
                let _ = bridge.close_service(session, stream).await;
            }
            Ok(())
        }).is_err() { let _ = self.close_service(session, stream).await; }
    }

    async fn unpublish_local_service(
        &self,
        request_id: RequestId,
        session: ManagerSessionId,
        service: ManagerLocalServiceId,
    ) -> Vec<DaemonToManagerMessage> {
        let Some(state) = self.sessions.lock().await.get(&session).cloned() else {
            return vec![reject(
                request_id,
                ManagerErrorCode::NotFound,
                "unknown session",
            )];
        };
        let mut guard = state.state.lock().await;
        let Some(entry) = guard.local_services.remove(&service) else {
            return vec![reject(
                request_id,
                ManagerErrorCode::NotFound,
                "unknown local service",
            )];
        };
        entry
            .cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
        guard.local_service_names.remove(&entry.name);
        let streams = guard
            .connections
            .iter()
            .filter_map(|(stream, value)| (value.local_service == Some(service)).then_some(*stream))
            .collect::<Vec<_>>();
        drop(guard);
        for stream in streams {
            self.notify_local_stream_ended(session, stream, ServiceEndReason::ManagerClosed)
                .await;
            let _ = self.close_service(session, stream).await;
        }
        vec![DaemonToManagerMessage::LocalServiceUnpublished {
            request_id,
            session,
            service,
        }]
    }

    async fn notify_local_stream_ended(
        &self,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
        reason: ServiceEndReason,
    ) {
        let Some(outbound) = self.outbound_sender().await else {
            return;
        };
        let event = DaemonToManagerMessage::ServiceEnded {
            session,
            stream,
            reason,
        };
        if let Ok(payload) = i2pr_app_manager_proto::encode_daemon_to_manager_control(&event)
            && let Ok(bytes) = Frame::control(payload).encode()
        {
            let _ = outbound.send(bytes).await;
        }
    }

    async fn open_service(
        self: &Arc<Self>,
        request_id: RequestId,
        session: ManagerSessionId,
        service: ManagerService,
    ) -> Vec<DaemonToManagerMessage> {
        let Some(session_state) = self.sessions.lock().await.get(&session).cloned() else {
            return vec![reject(
                request_id,
                ManagerErrorCode::NotFound,
                "unknown session",
            )];
        };
        let mut guard = session_state.state.lock().await;

        // The capability check happens in the daemon *before* backend allocation
        // and is side-effect free. `control_scoped` is unrepresentable in this
        // vocabulary, so this is the only service path that can reach here.
        if let Err(error) = guard.gateway.authorize(app_service(service)) {
            return vec![reject(
                request_id,
                gateway_code(error),
                "service capability denied",
            )];
        }
        let Ok(stream_id) = self.allocate_stream() else {
            return vec![reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "handle allocation exhausted",
            )];
        };
        if guard.streams.open(stream_id).is_err() {
            return vec![reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "service stream limit reached",
            )];
        }

        // A bounded in-memory duplex carries exact SAM/I2CP octets between the
        // manager and the private backend driver.
        let (manager_side, backend_side) =
            tokio::io::duplex(i2pr_app_manager_proto::MAX_DATA_FRAME_BYTES);
        let opened = match service {
            ManagerService::Sam => guard.gateway.open_sam(Box::new(backend_side)).await,
            ManagerService::SamDatagram => {
                guard.gateway.open_datagram(Box::new(backend_side)).await
            }
            ManagerService::I2cp => guard.gateway.open_i2cp(Box::new(backend_side)).await,
        };
        let connection = match opened {
            Ok(connection) => connection,
            Err(error) => {
                let _ = guard.streams.close(stream_id);
                return vec![reject(
                    request_id,
                    gateway_code(error),
                    "service open refused",
                )];
            }
        };

        // Bounded inbound queue, private to this stream.
        let (inbound, mut inbound_rx) = mpsc::channel::<Vec<u8>>(MAX_QUEUED_INBOUND_CHUNKS);
        let stream_cancellation = session_state.cancellation.child_token();
        guard.inbound.insert(stream_id, inbound);
        guard.connections.insert(
            stream_id,
            StreamState {
                cancellation: stream_cancellation.clone(),
                local_service: None,
            },
        );

        // Pump both directions. The manager->backend leg carries exact octets
        // from the bounded queue; the backend->manager leg drains the protocol
        // driver's replies, which is what makes the reply visible at all.
        //
        // Dropping the inbound sender ends the manager->backend leg, which closes
        // the backend half and so delivers EOF to the private protocol driver.
        let outbound = self.outbound_sender().await;
        let (read_half, mut write_half) = tokio::io::split(manager_side);
        let pump_stream = stream_id;
        let pump = self.composition.children.spawn(move |_| async move {
            let mut buffer = vec![0_u8; i2pr_app_manager_proto::MAX_DATA_FRAME_BYTES];
            let outbound_frames = outbound.clone();
            tokio::join!(
                async {
                    while let Some(chunk) = inbound_rx.recv().await {
                        if write_half.write_all(&chunk).await.is_err() {
                            break;
                        }
                    }
                    let _ = write_half.shutdown().await;
                },
                async {
                    let mut reader = read_half;
                    while let Ok(count) = reader.read(&mut buffer).await {
                        if count == 0 {
                            break;
                        }
                        let Some(sender) = &outbound_frames else {
                            break;
                        };
                        let frame = Frame::data(pump_stream, buffer[..count].to_vec());
                        let Ok(bytes) = frame.encode() else { break };
                        // A full outbound queue is bounded backpressure: this
                        // leg stops rather than buffering without limit.
                        if sender.send(bytes).await.is_err() {
                            break;
                        }
                    }
                },
            );
            Ok(())
        });
        if pump.is_err() {
            // Allocation refused: undo every side effect, so a rejected open
            // leaves no partial state and no live backend connection behind.
            guard.inbound.remove(&stream_id);
            if let Some(entry) = guard.connections.remove(&stream_id) {
                entry
                    .cancellation
                    .cancel(i2pr_core::CancellationReason::ParentScope);
            }
            let _ = guard.streams.close(stream_id);
            return vec![reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "child scope rejected the service pump",
            )];
        }

        // The backend watcher owns the connection, so a backend EOF is observed
        // here and reported for this stream only. Sibling streams and the parent
        // session are untouched.
        let outbound = self.outbound_sender().await;
        let notify_session = session;
        let notify_stream = stream_id;
        let watcher = self.composition.children.spawn(move |_| async move {
            // `wait_closed` consumes the connection; racing it against the
            // stream's own cancellation root is what lets an explicit close reach
            // the backend as well as a backend-driven end.
            let reason = tokio::select! {
                biased;
                () = stream_cancellation.cancelled() => ServiceEndReason::ManagerClosed,
                ended = connection.wait_closed() => match ended {
                    crate::app_gateway::AppGatewayConnectionEnd::BackendRejected => {
                        ServiceEndReason::BackendRejected
                    }
                    _ => ServiceEndReason::BackendClosed,
                },
            };
            if let Some(outbound) = outbound {
                let payload = i2pr_app_manager_proto::encode_daemon_to_manager_control(
                    &DaemonToManagerMessage::ServiceEnded {
                        session: notify_session,
                        stream: notify_stream,
                        reason,
                    },
                );
                if let Ok(payload) = payload
                    && let Ok(bytes) = Frame::control(payload).encode()
                {
                    let _ = outbound.send(bytes).await;
                }
            }
            Ok(())
        });
        if watcher.is_err() {
            guard.inbound.remove(&stream_id);
            if let Some(entry) = guard.connections.remove(&stream_id) {
                entry
                    .cancellation
                    .cancel(i2pr_core::CancellationReason::ParentScope);
            }
            let _ = guard.streams.close(stream_id);
            return vec![reject(
                request_id,
                ManagerErrorCode::ResourceLimit,
                "child scope rejected the backend watcher",
            )];
        }

        vec![DaemonToManagerMessage::ServiceOpened {
            request_id,
            session,
            stream: stream_id,
        }]
    }

    /// Clones the outbound writer's sender, if the bridge is already driving a
    /// transport. `None` means the bridge is not serving, so no notification can
    /// be delivered and none is required.
    async fn outbound_sender(&self) -> Option<mpsc::Sender<Vec<u8>>> {
        self.outbound
            .lock()
            .await
            .as_ref()
            .map(|writer| writer.0.clone())
    }

    /// Forwards exact manager octets into the backend byte stream.
    ///
    /// The handle is resolved only through the session that owns it, so a handle
    /// belonging to another session can never be delivered to this one.
    async fn forward_data(
        self: &Arc<Self>,
        stream: ManagerServiceStreamId,
        payload: Vec<u8>,
    ) -> Result<(), AppManagerBridgeError> {
        let owner = self.session_owning(stream).await;
        let Some((session, state)) = owner else {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::InvalidHandle,
            ));
        };
        let guard = state.state.lock().await;
        let Some(sender) = guard.inbound.get(&stream) else {
            return Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::InvalidHandle,
            ));
        };
        // A full queue means the backend is not draining. Refusing here is the
        // bounded-backpressure behaviour: the stream resets rather than growing.
        match sender.try_send(payload) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(AppManagerBridgeError::Protocol(
                ManagerProtocolError::InvalidHandle,
            )),
            Err(mpsc::error::TrySendError::Full(_)) => {
                drop(guard);
                self.notify_local_stream_ended(session, stream, ServiceEndReason::BackendRejected)
                    .await;
                let _ = self.close_service(session, stream).await;
                Ok(())
            }
        }
    }

    /// Finds the one session whose local map contains `stream`.
    ///
    /// Because handles are allocated monotonically and never reused, the match is
    /// unique. A stale handle matches nothing.
    async fn session_owning(
        &self,
        stream: ManagerServiceStreamId,
    ) -> Option<(ManagerSessionId, Arc<Session>)> {
        let sessions = self.sessions.lock().await;
        for (session, session_state) in sessions.iter() {
            if session_state
                .state
                .lock()
                .await
                .inbound
                .contains_key(&stream)
            {
                return Some((*session, Arc::clone(session_state)));
            }
        }
        None
    }

    async fn close_service(
        &self,
        session: ManagerSessionId,
        stream: ManagerServiceStreamId,
    ) -> bool {
        let Some(session_state) = self.sessions.lock().await.get(&session).cloned() else {
            return false;
        };
        let mut guard = session_state.state.lock().await;
        // A stale or cross-session handle fails here, before any teardown.
        if guard.streams.close(stream).is_err() {
            return false;
        }
        drop(guard.inbound.remove(&stream));
        match guard.connections.remove(&stream) {
            Some(entry) => {
                // Cancelling only this stream's root leaves siblings and the
                // parent session untouched.
                entry
                    .cancellation
                    .cancel(i2pr_core::CancellationReason::ParentScope);
                true
            }
            None => false,
        }
    }

    /// Deterministic teardown of every session and backend connection.
    async fn teardown_all(&self) {
        let sessions: Vec<ManagerSessionId> = self.sessions.lock().await.keys().copied().collect();
        for session in sessions {
            let removed = self.sessions.lock().await.remove(&session);
            if let Some(session_state) = removed {
                session_state
                    .cancellation
                    .cancel(i2pr_core::CancellationReason::ParentScope);
                let mut guard = session_state.state.lock().await;
                for (_, sender) in std::mem::take(&mut guard.inbound) {
                    drop(sender);
                }
                for (_, entry) in std::mem::take(&mut guard.connections) {
                    entry
                        .cancellation
                        .cancel(i2pr_core::CancellationReason::ParentScope);
                }
                guard.gateway.shutdown().await;
            }
            self.scope.lock().await.close_session(session).ok();
        }
        self.sessions.lock().await.clear();
    }

    /// Cancels the bridge root; used when the owning service shuts down.
    pub(crate) fn cancel(&self) {
        self.composition
            .cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
    }

    /// Session and stream counts, for bounded-lifecycle evidence.
    pub(crate) async fn counts(&self) -> (usize, usize) {
        let sessions = self.sessions.lock().await;
        let mut streams = 0_usize;
        for session_state in sessions.values() {
            streams += session_state.state.lock().await.streams.len();
        }
        (sessions.len(), streams)
    }
}

fn reject(
    request_id: RequestId,
    code: ManagerErrorCode,
    diagnostic: &str,
) -> DaemonToManagerMessage {
    DaemonToManagerMessage::Rejected {
        request_id,
        error: ManagerError {
            code,
            diagnostic: Some(diagnostic.to_owned()),
        },
    }
}

fn gateway_code(error: AppGatewayError) -> ManagerErrorCode {
    match error {
        AppGatewayError::PermissionDenied => ManagerErrorCode::PermissionDenied,
        AppGatewayError::UnsupportedService => ManagerErrorCode::UnsupportedOperation,
        AppGatewayError::ResourceLimit => ManagerErrorCode::ResourceLimit,
        AppGatewayError::InvalidLimits | AppGatewayError::BackendUnavailable => {
            ManagerErrorCode::Internal
        }
    }
}

fn app_service(service: ManagerService) -> AppService {
    match service {
        ManagerService::Sam | ManagerService::SamDatagram => AppService::Sam,
        ManagerService::I2cp => AppService::I2cp,
    }
}

/// Reads until `buf` is full or the peer reaches EOF. Returns the byte count; a
/// return of 0 means a clean EOF at a frame boundary, and a short return means a
/// truncated frame, which the caller rejects.
async fn read_until_eof<R>(reader: &mut R, buf: &mut [u8]) -> Result<usize, AppManagerBridgeError>
where
    R: AsyncRead + Unpin,
{
    let mut filled = 0_usize;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]).await {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err(AppManagerBridgeError::TransportClosed),
        }
    }
    Ok(filled)
}
#[cfg(test)]
mod tests {
    //! Plan 368 WP4 isolation, authority, and lifecycle evidence.
    //!
    //! These drive the bridge exactly as a manager would: an injected duplex
    //! byte stream, the frozen handshake, and the directional control codec.
    //! Nothing here reaches a socket, a listener, or a router listener.

    use super::*;
    use i2pr_app_manager_proto::{
        MAX_MANAGER_SESSIONS, MAX_SERVICE_STREAMS_PER_SESSION, ManagerRole,
        decode_daemon_to_manager_control,
    };
    use i2pr_app_proto::{AppId, AppInstanceId, Capability, PublisherId};
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
    use tokio::time::{Duration, timeout};

    const IO_BUFFER: usize = 64 * 1024;
    /// Every await on the manager side is bounded so a defect fails the test
    /// instead of hanging the suite.
    const DEADLINE: Duration = Duration::from_secs(10);

    // -- manager-side harness ------------------------------------------------

    /// What the bridge wrote back on the transport.
    enum ManagerReply {
        Control(DaemonToManagerMessage),
        Data(u32, Vec<u8>),
    }

    /// Minimal manager-protocol client used only by these tests.
    struct ManagerClient {
        stream: DuplexStream,
        pending: Vec<DaemonToManagerMessage>,
    }

    impl ManagerClient {
        async fn new() -> (Self, DuplexStream) {
            let (mine, bridge_side) = tokio::io::duplex(IO_BUFFER);
            (
                Self {
                    stream: mine,
                    pending: Vec::new(),
                },
                bridge_side,
            )
        }

        async fn handshake(&mut self) -> Result<(), ManagerProtocolError> {
            let bytes = Handshake {
                role: ManagerRole::Manager,
                major: i2pr_app_manager_proto::MANAGER_PROTOCOL_MAJOR,
                minor: i2pr_app_manager_proto::MANAGER_PROTOCOL_MINOR,
            }
            .encode();
            self.stream
                .write_all(&bytes)
                .await
                .map_err(|_| ManagerProtocolError::TransportClosed)
        }

        async fn write_frame(&mut self, frame: Frame) -> Result<(), ManagerProtocolError> {
            let bytes = frame.encode()?;
            self.stream
                .write_all(&bytes)
                .await
                .map_err(|_| ManagerProtocolError::TransportClosed)
        }

        async fn send(&mut self, message: &ManagerToDaemonMessage) {
            let payload = i2pr_app_manager_proto::encode_manager_to_daemon_control(message)
                .expect("encode manager control");
            self.write_frame(Frame::control(payload))
                .await
                .expect("write manager control");
        }

        async fn send_data(&mut self, stream: ManagerServiceStreamId, bytes: &[u8]) {
            self.write_frame(Frame::data(stream, bytes.to_vec()))
                .await
                .expect("write manager data");
        }

        /// Reads the next frame, queueing notifications for later matching.
        /// Reads the next **control** message, buffering any data frames seen
        /// so a service reply is never mistaken for a control reply.
        async fn next_message(&mut self) -> DaemonToManagerMessage {
            loop {
                match self.next_message_with_payload().await {
                    ManagerReply::Control(message) => return message,
                    ManagerReply::Data(_, _) => continue,
                }
            }
        }

        async fn next_message_with_payload(&mut self) -> ManagerReply {
            timeout(DEADLINE, async {
                let mut header = [0_u8; i2pr_app_manager_proto::FRAME_HEADER_BYTES];
                self.stream
                    .read_exact(&mut header)
                    .await
                    .expect("read header");
                // Re-assemble header+payload so the shared decoder is the single
                // definition of a valid frame, rather than trusting the reader.
                let mut bytes = header.to_vec();
                let declared = u32::from_be_bytes(header[8..12].try_into().expect("len")) as usize;
                bytes.resize(i2pr_app_manager_proto::FRAME_HEADER_BYTES + declared, 0);
                self.stream
                    .read_exact(&mut bytes[i2pr_app_manager_proto::FRAME_HEADER_BYTES..])
                    .await
                    .expect("read payload");
                let (frame, consumed) = Frame::decode(&bytes).expect("decode frame");
                assert_eq!(
                    consumed,
                    bytes.len(),
                    "the reader must consume exactly one frame"
                );
                match frame.kind {
                    // Exact backend octets, on the handle the daemon assigned.
                    FrameKind::Data => ManagerReply::Data(frame.stream_id, frame.payload),
                    FrameKind::Control => ManagerReply::Control(
                        decode_daemon_to_manager_control(&frame.payload).expect("decode control"),
                    ),
                }
            })
            .await
            .expect("manager reply within deadline")
        }

        /// Sends a request and returns the reply correlated to its id.
        async fn request(&mut self, message: ManagerToDaemonMessage) -> DaemonToManagerMessage {
            let expected = match &message {
                ManagerToDaemonMessage::CreateSession { request_id, .. }
                | ManagerToDaemonMessage::CloseSession { request_id, .. }
                | ManagerToDaemonMessage::OpenService { request_id, .. }
                | ManagerToDaemonMessage::CloseService { request_id, .. }
                | ManagerToDaemonMessage::ResetService { request_id, .. }
                | ManagerToDaemonMessage::PublishLocalService { request_id, .. }
                | ManagerToDaemonMessage::UnpublishLocalService { request_id, .. }
                | ManagerToDaemonMessage::Health { request_id }
                | ManagerToDaemonMessage::Shutdown { request_id, .. } => *request_id,
            };
            self.send(&message).await;
            loop {
                if let Some(index) = self
                    .pending
                    .iter()
                    .position(|message| message.correlation() == Some(expected))
                {
                    return self.pending.remove(index);
                }
                let next = self.next_message().await;
                if next.correlation() == Some(expected) {
                    return next;
                }
                // A notification observed while waiting is retained, not dropped.
                self.pending.push(next);
            }
        }

        /// Waits for the next notification, ignoring correlated replies.
        async fn next_notification(&mut self) -> DaemonToManagerMessage {
            if let Some(message) = self
                .pending
                .iter()
                .position(|message| message.correlation().is_none())
                .map(|index| self.pending.remove(index))
            {
                return message;
            }
            loop {
                let next = self.next_message().await;
                if next.correlation().is_none() {
                    return next;
                }
                self.pending.push(next);
            }
        }

        async fn open_session(
            &mut self,
            request_id: RequestId,
            instance: u128,
            capabilities: &[Capability],
        ) -> DaemonToManagerMessage {
            self.request(ManagerToDaemonMessage::CreateSession {
                request_id,
                principal: principal(instance),
                effective_capabilities: capabilities
                    .iter()
                    .map(|capability| EffectiveGrant {
                        capability: *capability,
                    })
                    .collect(),
                limits: ManagerGatewayLimits::new(4).expect("limits"),
            })
            .await
        }
    }

    fn principal(instance: u128) -> i2pr_app_manager_proto::ManagerPrincipal {
        i2pr_app_manager_proto::ManagerPrincipal {
            app_id: AppId::parse("example.client").expect("app id"),
            instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(instance),
            publisher_id: Some(PublisherId::parse("example.publisher").expect("publisher")),
        }
    }

    fn rid(value: u32) -> RequestId {
        RequestId::new(value).expect("non-zero request id")
    }

    fn composition() -> AppManagerComposition {
        let cancellation = CancellationToken::new();
        let children = ChildScope::for_test(&cancellation, ChildFailurePolicy::CollectResult);
        AppManagerComposition {
            sam: SamConfig {
                enabled: true,
                bind_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 7656,
                udp_port: 0,
                limits: i2pr_api::sam::limits::SamLimits::loopback_test_profile(),
            },
            i2cp: I2cpConfig::loopback_test_profile(8, 16, 64 * 1024, 64),
            addressbook: crate::addressbook::SharedAddressBook::new(),
            children,
            cancellation,
        }
    }

    /// Starts a bridge over a fresh injected transport and returns both ends.
    async fn start_bridge() -> (Arc<AppManagerBridge>, ManagerClient) {
        let (mut manager, bridge_side) = ManagerClient::new().await;
        let bridge = Arc::new(AppManagerBridge::new(composition()));
        let driver = {
            let bridge = Arc::clone(&bridge);
            async move { bridge.run(bridge_side).await }
        };
        tokio::spawn(driver);
        manager.handshake().await.expect("manager handshake");
        (bridge, manager)
    }

    fn opened_session(message: DaemonToManagerMessage) -> ManagerSessionId {
        match message {
            DaemonToManagerMessage::SessionOpened { session, .. } => session,
            other => panic!("expected session_opened, got {other:?}"),
        }
    }

    fn opened_stream(message: DaemonToManagerMessage) -> ManagerServiceStreamId {
        match message {
            DaemonToManagerMessage::ServiceOpened { stream, .. } => stream,
            other => panic!("expected service_opened, got {other:?}"),
        }
    }

    fn rejection(message: DaemonToManagerMessage) -> ManagerErrorCode {
        match message {
            DaemonToManagerMessage::Rejected { error, .. } => error.code,
            other => panic!("expected rejected, got {other:?}"),
        }
    }

    /// Asserts the next frame is backend data on `expected`, returning its exact
    /// octets. A control frame here is a failure: service bytes must never be
    /// wrapped as control, and data must never arrive on another handle.
    async fn expect_data(manager: &mut ManagerClient, expected: ManagerServiceStreamId) -> Vec<u8> {
        match manager.next_message_with_payload().await {
            ManagerReply::Data(stream, payload) => {
                assert_eq!(
                    u64::from(stream),
                    expected.get(),
                    "backend data arrived on the wrong stream handle"
                );
                payload
            }
            ManagerReply::Control(other) => {
                panic!("service data must not arrive as control: {other:?}")
            }
        }
    }

    // -- authority -----------------------------------------------------------

    #[tokio::test]
    async fn trusted_session_create_is_accepted_and_reserved_grants_are_refused() {
        let (bridge, mut manager) = start_bridge().await;

        let reply = manager.open_session(rid(1), 1, &[Capability::Sam]).await;
        let session = opened_session(reply);

        // A reserved capability cannot be granted, and the refusal allocates no
        // session at all rather than creating an empty one.
        let reply = manager
            .open_session(rid(2), 2, &[Capability::BrokeredTcp])
            .await;
        assert_eq!(
            rejection(reply),
            ManagerErrorCode::UnsupportedOperation,
            "brokered_tcp must be refused by the administrator-grant path"
        );
        let (sessions, _streams) = bridge.counts().await;
        assert_eq!(sessions, 1, "only the accepted session may exist");

        let _ = session;
    }

    #[tokio::test]
    async fn capability_denial_and_control_scoped_allocate_nothing() {
        let (bridge, mut manager) = start_bridge().await;
        let session = opened_session(manager.open_session(rid(1), 1, &[]).await);

        // No capabilities at all: both service opens are refused, side-effect free.
        let reply = manager
            .request(ManagerToDaemonMessage::OpenService {
                request_id: rid(2),
                session,
                service: ManagerService::Sam,
            })
            .await;
        assert_eq!(rejection(reply), ManagerErrorCode::PermissionDenied);

        let reply = manager
            .request(ManagerToDaemonMessage::OpenService {
                request_id: rid(3),
                session,
                service: ManagerService::I2cp,
            })
            .await;
        assert_eq!(rejection(reply), ManagerErrorCode::PermissionDenied);

        let (sessions, streams) = bridge.counts().await;
        assert_eq!(
            (sessions, streams),
            (1, 0),
            "a denied open allocates nothing"
        );

        // `control_scoped` is unrepresentable: the daemon cannot even be asked.
        let payload =
            br#"{"type":"open_service","request_id":4,"session":1,"service":"control_scoped"}"#;
        manager
            .write_frame(Frame::control(payload.to_vec()))
            .await
            .expect("write control_scoped attempt");
        // The bridge treats the undecodable payload as a protocol violation and
        // closes the transport rather than opening anything.
        let (sessions, streams) = bridge.counts().await;
        assert_eq!((sessions, streams), (1, 0));
    }

    #[tokio::test]
    async fn application_declarations_cannot_construct_manager_authorization() {
        // An application `hello` and a requested-capability list are ordinary
        // app-protocol values. Neither has a decoder into the manager authority
        // type, so neither can produce authority on this bridge.
        let hello = i2pr_app_proto::AppToHostMessage::Hello {
            request_id: rid(1),
            app_id: AppId::parse("example.client").expect("app id"),
            instance_id: AppInstanceId::new(7).expect("instance id"),
            protocol_major: 1,
            protocol_minor: 0,
        };
        let bytes = i2pr_app_proto::encode_app_to_host_control(&hello).expect("encode hello");
        assert!(
            i2pr_app_manager_proto::decode_manager_to_daemon_control(&bytes).is_err(),
            "an application hello must not decode as a manager request"
        );

        let requested = i2pr_app_proto::AppToHostMessage::PermissionRequest {
            request_id: rid(2),
            capabilities: vec![i2pr_app_proto::RequestedCapability {
                capability: Capability::Sam,
            }],
        };
        let bytes =
            i2pr_app_proto::encode_app_to_host_control(&requested).expect("encode permission");
        assert!(
            i2pr_app_manager_proto::decode_manager_to_daemon_control(&bytes).is_err(),
            "a requested capability must not decode as a manager request"
        );

        // And a manager request cannot be smuggled in through the app protocol.
        let create = ManagerToDaemonMessage::CreateSession {
            request_id: rid(3),
            principal: principal(1),
            effective_capabilities: Vec::new(),
            limits: ManagerGatewayLimits::new(2).expect("limits"),
        };
        let bytes =
            i2pr_app_manager_proto::encode_manager_to_daemon_control(&create).expect("encode");
        assert!(
            i2pr_app_proto::decode_app_to_host_control(&bytes).is_err(),
            "a manager request must not decode as an application message"
        );
    }

    // -- exact byte forwarding ----------------------------------------------

    #[tokio::test]
    async fn sam_service_stream_forwards_exact_octets_through_the_gateway() {
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(manager.open_session(rid(1), 1, &[Capability::Sam]).await);
        let stream = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(2),
                    session,
                    service: ManagerService::Sam,
                })
                .await,
        );

        // Exact SAM octets in, exact SAM reply out: no base64, no wrapping.
        manager
            .send_data(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\n")
            .await;
        let payload = expect_data(&mut manager, stream).await;
        let text = String::from_utf8_lossy(&payload);
        assert!(
            text.starts_with("HELLO REPLY RESULT=OK"),
            "unexpected SAM reply: {text}"
        );

        manager
            .request(ManagerToDaemonMessage::CloseService {
                request_id: rid(3),
                session,
                stream,
            })
            .await;
    }

    #[tokio::test]
    async fn sam_datagram_service_is_typed_and_scoped_to_private_gateway() {
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(manager.open_session(rid(1), 1, &[Capability::Sam]).await);
        let stream = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(2),
                    session,
                    service: ManagerService::SamDatagram,
                })
                .await,
        );
        let request = i2pr_app_manager_proto::datagram::DatagramRequest::Receive {
            primary_id: "missing-primary".to_owned(),
            child_id: "missing-child".to_owned(),
        }
        .encode()
        .expect("encode private datagram operation");
        let mut framed = (request.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(&request);
        manager.send_data(stream, &framed).await;
        let response = expect_data(&mut manager, stream).await;
        let declared = u32::from_be_bytes(response[..4].try_into().expect("reply length")) as usize;
        assert_eq!(declared, response.len() - 4);
        assert_eq!(
            i2pr_app_manager_proto::datagram::DatagramReply::decode(&response[4..]).unwrap(),
            i2pr_app_manager_proto::datagram::DatagramReply::Rejected
        );
        manager
            .request(ManagerToDaemonMessage::CloseService {
                request_id: rid(3),
                session,
                stream,
            })
            .await;
    }

    #[tokio::test]
    async fn i2cp_service_stream_carries_exact_protocol_bytes() {
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(manager.open_session(rid(1), 1, &[Capability::I2cp]).await);
        let stream = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(2),
                    session,
                    service: ManagerService::I2cp,
                })
                .await,
        );

        let mut frame = vec![i2pr_api::i2cp::PROTOCOL_BYTE];
        let request = i2pr_api::i2cp::Message::GetDate(i2pr_api::i2cp::GetDate {
            version: "0.9.67".to_owned(),
            auth: None,
        });
        frame.extend_from_slice(
            &i2pr_api::i2cp::encode_frame(
                request.message_type() as u8,
                &request.encode_body().expect("body"),
            )
            .expect("frame"),
        );
        manager.send_data(stream, &frame).await;

        let payload = expect_data(&mut manager, stream).await;
        let decoded = i2pr_api::i2cp::decode_frame(&payload).expect("decode I2CP reply");
        assert_eq!(
            decoded.message_type,
            i2pr_api::i2cp::MessageType::SetDate as u8,
            "the private I2CP driver must answer on the same stream"
        );
    }

    // -- isolation -----------------------------------------------------------

    #[tokio::test]
    async fn two_sessions_with_identical_app_identifiers_stay_isolated() {
        let (_bridge, mut manager) = start_bridge().await;
        // Same app identity, different launch instances: the app-level SAM
        // session id is deliberately identical below to prove the daemon's
        // per-session isolation, not app-side naming, provides the boundary.
        let first = opened_session(manager.open_session(rid(1), 1, &[Capability::Sam]).await);
        let second = opened_session(manager.open_session(rid(2), 2, &[Capability::Sam]).await);
        assert_ne!(first, second);

        let first_stream = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(3),
                    session: first,
                    service: ManagerService::Sam,
                })
                .await,
        );
        let second_stream = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(4),
                    session: second,
                    service: ManagerService::Sam,
                })
                .await,
        );
        assert_ne!(first_stream, second_stream);

        for stream in [first_stream, second_stream] {
            manager
                .send_data(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\n")
                .await;
        }
        let mut replies = 0;
        for stream in [first_stream, second_stream] {
            let payload = expect_data(&mut manager, stream).await;
            let text = String::from_utf8_lossy(&payload);
            assert!(text.starts_with("HELLO REPLY RESULT=OK"), "{text}");
            replies += 1;
        }
        assert_eq!(replies, 2, "both sessions answered independently");

        // Both sessions may use the identical SAM session id without attaching
        // to each other: session B cannot reach session A's session.
        manager
            .send_data(
                first_stream,
                b"SESSION CREATE STYLE=STREAM ID=shared DESTINATION=TRANSIENT\n",
            )
            .await;
        let payload = expect_data(&mut manager, first_stream).await;
        assert!(
            String::from_utf8_lossy(&payload).starts_with("SESSION STATUS RESULT=OK"),
            "first session must own its own SAM session id"
        );
        manager
            .send_data(second_stream, b"STREAM ACCEPT ID=shared\n")
            .await;
        let payload = expect_data(&mut manager, second_stream).await;
        let text = String::from_utf8_lossy(&payload);
        assert!(
            text.contains("RESULT=I2P_ERROR") || text.contains("RESULT=INVALID_ID"),
            "cross-principal attach must be denied: {text}"
        );
    }

    #[tokio::test]
    async fn cross_session_and_stale_handles_fail_deterministically() {
        let (bridge, mut manager) = start_bridge().await;
        let first = opened_session(manager.open_session(rid(1), 1, &[Capability::Sam]).await);
        let second = opened_session(manager.open_session(rid(2), 2, &[Capability::Sam]).await);
        let first_stream = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(3),
                    session: first,
                    service: ManagerService::Sam,
                })
                .await,
        );

        // A handle from session A, named against session B, is not found there.
        let reply = manager
            .request(ManagerToDaemonMessage::CloseService {
                request_id: rid(4),
                session: second,
                stream: first_stream,
            })
            .await;
        assert_eq!(rejection(reply), ManagerErrorCode::NotFound);
        // ...and it did not close session A's stream either.
        let (sessions, streams) = bridge.counts().await;
        assert_eq!((sessions, streams), (2, 1), "cross-session close is inert");

        // Closing it for real, then closing again, is deterministic.
        let reply = manager
            .request(ManagerToDaemonMessage::CloseService {
                request_id: rid(5),
                session: first,
                stream: first_stream,
            })
            .await;
        assert!(matches!(
            reply,
            DaemonToManagerMessage::ServiceClosed { .. }
        ));
        let reply = manager
            .request(ManagerToDaemonMessage::CloseService {
                request_id: rid(6),
                session: first,
                stream: first_stream,
            })
            .await;
        assert_eq!(rejection(reply), ManagerErrorCode::NotFound);

        // A never-issued session handle is refused the same way.
        let unknown = ManagerSessionId::new(9_999).expect("handle");
        let reply = manager
            .request(ManagerToDaemonMessage::CloseSession {
                request_id: rid(7),
                session: unknown,
            })
            .await;
        assert_eq!(rejection(reply), ManagerErrorCode::NotFound);
    }

    #[tokio::test]
    async fn one_backend_eof_does_not_close_a_sibling_stream() {
        let (bridge, mut manager) = start_bridge().await;
        let session = opened_session(manager.open_session(rid(1), 1, &[Capability::Sam]).await);
        let first = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(2),
                    session,
                    service: ManagerService::Sam,
                })
                .await,
        );
        let second = opened_stream(
            manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(3),
                    session,
                    service: ManagerService::Sam,
                })
                .await,
        );

        manager
            .send_data(first, b"HELLO VERSION MIN=3.1 MAX=3.1\n")
            .await;
        let payload = expect_data(&mut manager, first).await;
        assert!(String::from_utf8_lossy(&payload).starts_with("HELLO REPLY RESULT=OK"));

        // Close only the first stream. The manager must observe a terminal
        // notification naming *that* stream and that session, and nothing else.
        manager
            .request(ManagerToDaemonMessage::CloseService {
                request_id: rid(4),
                session,
                stream: first,
            })
            .await;
        match manager.next_notification().await {
            DaemonToManagerMessage::ServiceEnded {
                session: ended_session,
                stream: ended_stream,
                reason,
            } => {
                assert_eq!(
                    ended_session, session,
                    "notification names the owning session"
                );
                assert_eq!(
                    u64::from(ended_stream),
                    first.get(),
                    "notification must name the closed stream, not the sibling"
                );
                assert!(
                    matches!(
                        reason,
                        ServiceEndReason::ManagerClosed | ServiceEndReason::BackendClosed
                    ),
                    "unexpected end reason: {reason:?}"
                );
            }
            other => panic!("expected service_ended, got {other:?}"),
        }

        // The sibling is still usable on the same session.
        manager
            .send_data(second, b"HELLO VERSION MIN=3.1 MAX=3.1\n")
            .await;
        let payload = expect_data(&mut manager, second).await;
        assert!(
            String::from_utf8_lossy(&payload).starts_with("HELLO REPLY RESULT=OK"),
            "a sibling backend EOF must not close another stream"
        );
        let (_sessions, streams) = bridge.counts().await;
        assert_eq!(streams, 1, "only the closed stream is gone");
    }

    // -- lifecycle and bounded resources ------------------------------------

    #[tokio::test]
    async fn manager_transport_eof_tears_down_every_gateway_session() {
        let (bridge, mut manager) = start_bridge().await;
        for index in 1..=3_u32 {
            let _ = manager
                .open_session(rid(index), u128::from(index), &[Capability::Sam])
                .await;
        }
        let (sessions, _streams) = bridge.counts().await;
        assert_eq!(sessions, 3);

        // Manager EOF: the bridge must cancel every session it created.
        drop(manager.stream);
        let drained = timeout(DEADLINE, async {
            loop {
                if bridge.counts().await.0 == 0 {
                    return true;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(
            drained.expect("teardown within deadline"),
            "manager EOF must tear down all sessions"
        );
    }

    /// Admission is bounded on both axes, and the *implementation* ceiling is
    /// lower than the *protocol* ceiling.
    ///
    /// The protocol allows `MAX_SERVICE_STREAMS_PER_SESSION` (128) live streams
    /// per session, but each stream costs two child tasks (one bidirectional pump
    /// plus the gateway's own backend driver) and `i2pr-runtime::MAX_CHILD_TASKS`
    /// is 64. The honest contract is therefore: admission **fails closed** with a
    /// typed `ResourceLimit` and allocates nothing once either ceiling is reached,
    /// rather than the protocol number being silently asserted as reachable.
    #[tokio::test]
    async fn max_plus_one_session_admission_is_rejected_and_allocates_nothing() {
        let (bridge, mut manager) = start_bridge().await;
        for index in 1..=MAX_MANAGER_SESSIONS as u32 {
            opened_session(
                manager
                    .open_session(rid(index), u128::from(index) + 1, &[Capability::Sam])
                    .await,
            );
        }
        assert_eq!(bridge.counts().await.0, MAX_MANAGER_SESSIONS);

        // max+1 session: refused synchronously, and nothing is allocated.
        let reply = manager
            .open_session(rid(900), 9_000, &[Capability::Sam])
            .await;
        assert_eq!(rejection(reply), ManagerErrorCode::ResourceLimit);
        assert_eq!(
            bridge.counts().await.0,
            MAX_MANAGER_SESSIONS,
            "a refused session must not allocate"
        );

        // Freeing one slot makes admission succeed again, proving the ceiling is
        // a live resource limit and not a poisoned state.
        manager
            .request(ManagerToDaemonMessage::CloseSession {
                request_id: rid(901),
                session: ManagerSessionId::new(1).expect("handle"),
            })
            .await;
        opened_session(
            manager
                .open_session(rid(902), 9_002, &[Capability::Sam])
                .await,
        );
        assert_eq!(bridge.counts().await.0, MAX_MANAGER_SESSIONS);
    }

    #[tokio::test]
    async fn stream_admission_fails_closed_once_the_child_scope_is_exhausted() {
        let (bridge, mut manager) = start_bridge().await;
        let session = opened_session(manager.open_session(rid(1), 1, &[Capability::Sam]).await);

        let mut admitted = 0_usize;
        let mut refused_at = None;
        // Open until the bridge refuses. Every refusal must be a typed
        // `ResourceLimit`, and the accepted count must stay within both the
        // protocol ceiling and the runtime child-task ceiling.
        for index in 1..=MAX_SERVICE_STREAMS_PER_SESSION as u32 + 1_u32 {
            let reply = manager
                .request(ManagerToDaemonMessage::OpenService {
                    request_id: rid(1_000 + index),
                    session,
                    service: ManagerService::Sam,
                })
                .await;
            match reply {
                DaemonToManagerMessage::ServiceOpened { .. } => admitted += 1,
                other => {
                    assert_eq!(
                        rejection(other),
                        ManagerErrorCode::ResourceLimit,
                        "exhaustion must be a typed ResourceLimit"
                    );
                    refused_at = Some(index);
                    break;
                }
            }
        }
        let refused_at = refused_at.expect("admission must eventually refuse");
        assert!(
            admitted > 0,
            "at least some streams must be admissible, otherwise the test is vacuous"
        );
        assert!(
            admitted <= MAX_SERVICE_STREAMS_PER_SESSION,
            "admitted {admitted} exceeds the protocol ceiling"
        );
        assert!(
            admitted < refused_at as usize,
            "refusal must happen at max+1, not early"
        );

        // A refused open allocates nothing: the ledger still matches the admitted
        // set exactly.
        let (_sessions, streams) = bridge.counts().await;
        assert_eq!(
            streams, admitted,
            "a refused open must leave no partial stream state"
        );
    }

    #[tokio::test]
    async fn repeated_create_open_close_returns_counts_to_baseline() {
        let (bridge, mut manager) = start_bridge().await;
        for round in 1..=4_u32 {
            let session = opened_session(
                manager
                    .open_session(rid(round * 10), u128::from(round) + 100, &[Capability::Sam])
                    .await,
            );
            let stream = opened_stream(
                manager
                    .request(ManagerToDaemonMessage::OpenService {
                        request_id: rid(round * 10 + 1),
                        session,
                        service: ManagerService::Sam,
                    })
                    .await,
            );
            // A close releases the stream slot; a session close releases the
            // session slot. Every drop path must return capacity.
            manager
                .request(ManagerToDaemonMessage::CloseService {
                    request_id: rid(round * 10 + 2),
                    session,
                    stream,
                })
                .await;
            manager
                .request(ManagerToDaemonMessage::CloseSession {
                    request_id: rid(round * 10 + 3),
                    session,
                })
                .await;
            let drained = timeout(DEADLINE, async {
                loop {
                    if bridge.counts().await == (0, 0) {
                        return true;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await;
            assert!(
                drained.expect("counts return to baseline"),
                "round {round} leaked state"
            );
        }
    }

    #[tokio::test]
    async fn health_and_shutdown_are_bounded_and_correlated() {
        let (_bridge, mut manager) = start_bridge().await;
        let reply = manager
            .request(ManagerToDaemonMessage::Health { request_id: rid(1) })
            .await;
        match reply {
            DaemonToManagerMessage::HealthStatus {
                request_id, state, ..
            } => {
                assert_eq!(request_id, rid(1));
                assert_eq!(state, "running");
            }
            other => panic!("expected health_status, got {other:?}"),
        }

        let reply = manager
            .request(ManagerToDaemonMessage::Shutdown {
                request_id: rid(2),
                reason: "operator request".to_owned(),
            })
            .await;
        assert!(matches!(reply, DaemonToManagerMessage::ShutdownAck { .. }));
    }

    #[tokio::test]
    async fn wrong_manager_magic_and_version_are_rejected_before_any_session() {
        let (mut manager, bridge_side) = ManagerClient::new().await;
        let bridge = Arc::new(AppManagerBridge::new(composition()));
        let driver = {
            let bridge = Arc::clone(&bridge);
            async move { bridge.run(bridge_side).await }
        };
        let task = tokio::spawn(driver);

        // Wrong magic.
        let mut bytes = Handshake {
            role: ManagerRole::Manager,
            major: 1,
            minor: 0,
        }
        .encode();
        bytes[0] = b'X';
        let _ = manager.stream.write_all(&bytes).await;
        assert_eq!(bridge.counts().await.0, 0);

        // Wrong major version.
        // Wrong major version on a fresh transport.
        let (mut other, other_side) = ManagerClient::new().await;
        let other_task = {
            let bridge = Arc::clone(&bridge);
            tokio::spawn(async move { bridge.run(other_side).await })
        };
        other
            .stream
            .write_all(
                &Handshake {
                    role: ManagerRole::Manager,
                    major: 9,
                    minor: 0,
                }
                .encode(),
            )
            .await
            .expect("write bad version handshake");
        let joined = timeout(DEADLINE, other_task)
            .await
            .expect("bridge returns within deadline")
            .expect("bridge task must not panic");
        assert!(
            joined.is_err(),
            "an unsupported major version must end the transport"
        );
        assert_eq!(bridge.counts().await.0, 0, "no session may exist");
        let _ = task.await;
        let _ = &mut manager;
    }

    #[tokio::test]
    async fn oversize_declared_frame_length_is_refused_before_allocation() {
        let (mut manager, bridge_side) = ManagerClient::new().await;
        let bridge = Arc::new(AppManagerBridge::new(composition()));
        manager
            .stream
            .write_all(
                &Handshake {
                    role: ManagerRole::Manager,
                    major: 1,
                    minor: 0,
                }
                .encode(),
            )
            .await
            .expect("handshake");
        let driver = {
            let bridge = Arc::clone(&bridge);
            async move { bridge.run(bridge_side).await }
        };
        let task = tokio::spawn(driver);

        // A control frame claiming a near-overflow payload must be refused on the
        // declared length, before any buffer is sized from it.
        let mut header = [0_u8; i2pr_app_manager_proto::FRAME_HEADER_BYTES];
        header[0] = 1;
        header[1] = 1;
        header[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        manager
            .stream
            .write_all(&header)
            .await
            .expect("write oversize header");

        let joined = timeout(DEADLINE, task)
            .await
            .expect("bridge returns within deadline")
            .expect("bridge task must not panic");
        assert!(
            joined.is_err(),
            "an oversize declared length must end the transport"
        );
        assert_eq!(bridge.counts().await.0, 0);
    }

    #[tokio::test]
    async fn manager_cancel_root_tears_down_descendant_state() {
        let (bridge, mut manager) = start_bridge().await;
        let _ = manager.open_session(rid(1), 1, &[Capability::Sam]).await;
        assert_eq!(bridge.counts().await.0, 1);
        bridge.cancel();
        bridge.teardown_all().await;
        assert_eq!(bridge.counts().await, (0, 0));
    }

    #[tokio::test]
    async fn granted_local_service_forwards_exact_bytes_over_loopback() {
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(
            manager
                .open_session(rid(1), 51, &[Capability::LocalService])
                .await,
        );
        let published = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(2),
                session,
                service_name: "rpc".into(),
                preferred_port: None,
            })
            .await;
        let (service, port) = match published {
            DaemonToManagerMessage::LocalServicePublished { service, port, .. } => (service, port),
            other => panic!("expected published service, got {other:?}"),
        };
        assert!(port >= 1024);
        let mut client = timeout(DEADLINE, TcpStream::connect((Ipv4Addr::LOCALHOST, port)))
            .await
            .expect("connect deadline")
            .expect("loopback listener");
        let incoming = manager.next_notification().await;
        let stream = match incoming {
            DaemonToManagerMessage::LocalServiceIncoming {
                session: actual,
                service: actual_service,
                stream,
            } => {
                assert_eq!(actual, session);
                assert_eq!(actual_service, service);
                stream
            }
            other => panic!("expected incoming stream, got {other:?}"),
        };
        client
            .write_all(b"client-to-app")
            .await
            .expect("client write");
        match manager.next_message_with_payload().await {
            ManagerReply::Data(actual, bytes) => {
                assert_eq!(actual, stream.get() as u32);
                assert_eq!(bytes, b"client-to-app");
            }
            _ => panic!("expected forwarded client bytes"),
        }
        manager.send_data(stream, b"app-to-client").await;
        let mut received = [0_u8; 13];
        timeout(DEADLINE, client.read_exact(&mut received))
            .await
            .expect("read deadline")
            .expect("read client data");
        assert_eq!(&received, b"app-to-client");
        let result = manager
            .request(ManagerToDaemonMessage::UnpublishLocalService {
                request_id: rid(3),
                session,
                service,
            })
            .await;
        assert!(matches!(
            result,
            DaemonToManagerMessage::LocalServiceUnpublished { .. }
        ));
        assert_eq!(
            timeout(DEADLINE, client.read(&mut [0_u8; 1]))
                .await
                .expect("teardown deadline")
                .expect("teardown read"),
            0
        );
    }

    #[tokio::test]
    async fn local_service_grant_and_session_ownership_are_enforced() {
        let (_bridge, mut manager) = start_bridge().await;
        let denied_session =
            opened_session(manager.open_session(rid(10), 61, &[Capability::Sam]).await);
        let denied = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(11),
                session: denied_session,
                service_name: "denied".into(),
                preferred_port: None,
            })
            .await;
        assert!(matches!(
            denied,
            DaemonToManagerMessage::Rejected {
                error: ManagerError {
                    code: ManagerErrorCode::PermissionDenied,
                    ..
                },
                ..
            }
        ));

        let owner = opened_session(
            manager
                .open_session(rid(12), 62, &[Capability::LocalService])
                .await,
        );
        let foreign = opened_session(
            manager
                .open_session(rid(13), 63, &[Capability::LocalService])
                .await,
        );
        let published = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(14),
                session: owner,
                service_name: "rpc".into(),
                preferred_port: None,
            })
            .await;
        let (service, port) = match published {
            DaemonToManagerMessage::LocalServicePublished { service, port, .. } => (service, port),
            other => panic!("expected publish, got {other:?}"),
        };
        let duplicate = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(15),
                session: owner,
                service_name: "rpc".into(),
                preferred_port: None,
            })
            .await;
        assert!(matches!(
            duplicate,
            DaemonToManagerMessage::Rejected {
                error: ManagerError {
                    code: ManagerErrorCode::Conflict,
                    ..
                },
                ..
            }
        ));
        let foreign_unpublish = manager
            .request(ManagerToDaemonMessage::UnpublishLocalService {
                request_id: rid(16),
                session: foreign,
                service,
            })
            .await;
        assert!(matches!(
            foreign_unpublish,
            DaemonToManagerMessage::Rejected {
                error: ManagerError {
                    code: ManagerErrorCode::NotFound,
                    ..
                },
                ..
            }
        ));
        let own_unpublish = manager
            .request(ManagerToDaemonMessage::UnpublishLocalService {
                request_id: rid(17),
                session: owner,
                service,
            })
            .await;
        assert!(matches!(
            own_unpublish,
            DaemonToManagerMessage::LocalServiceUnpublished { .. }
        ));
        assert!(
            TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_err(),
            "unpublished listener must be gone"
        );
    }

    #[tokio::test]
    async fn local_service_connection_ceiling_rejects_the_seventeenth_stream() {
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(
            manager
                .open_session(rid(20), 71, &[Capability::LocalService])
                .await,
        );
        let published = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(21),
                session,
                service_name: "bounded".into(),
                preferred_port: None,
            })
            .await;
        let (service, port) = match published {
            DaemonToManagerMessage::LocalServicePublished { service, port, .. } => (service, port),
            other => panic!("expected published service, got {other:?}"),
        };
        let mut clients = Vec::new();
        for _ in 0..i2pr_app_manager_proto::MAX_LOCAL_SERVICE_CONNECTIONS_PER_SESSION {
            clients.push(
                TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                    .await
                    .expect("connect under ceiling"),
            );
            assert!(matches!(
                manager.next_notification().await,
                DaemonToManagerMessage::LocalServiceIncoming { .. }
            ));
        }
        let mut excess = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .expect("kernel may complete the bounded backlog connect");
        let mut byte = [0_u8; 1];
        assert_eq!(
            timeout(DEADLINE, excess.read(&mut byte))
                .await
                .expect("excess close deadline")
                .expect("excess close read"),
            0
        );
        assert_eq!(
            manager
                .request(ManagerToDaemonMessage::UnpublishLocalService {
                    request_id: rid(22),
                    session,
                    service
                })
                .await,
            DaemonToManagerMessage::LocalServiceUnpublished {
                request_id: rid(22),
                session,
                service
            }
        );
        drop(clients);
    }

    #[tokio::test]
    async fn local_service_bind_conflict_does_not_choose_another_port() {
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(
            manager
                .open_session(rid(30), 81, &[Capability::LocalService])
                .await,
        );
        let reply = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(31),
                session,
                service_name: "conflict".into(),
                preferred_port: Some(port),
            })
            .await;
        assert!(matches!(
            reply,
            DaemonToManagerMessage::Rejected {
                error: ManagerError {
                    code: ManagerErrorCode::Conflict,
                    ..
                },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn closing_the_owner_session_closes_listener_and_accepted_streams() {
        let (_bridge, mut manager) = start_bridge().await;
        let session = opened_session(
            manager
                .open_session(rid(40), 91, &[Capability::LocalService])
                .await,
        );
        let published = manager
            .request(ManagerToDaemonMessage::PublishLocalService {
                request_id: rid(41),
                session,
                service_name: "lifecycle".into(),
                preferred_port: None,
            })
            .await;
        let (service, port) = match published {
            DaemonToManagerMessage::LocalServicePublished { service, port, .. } => (service, port),
            other => panic!("expected publication, got {other:?}"),
        };
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        assert!(
            matches!(manager.next_notification().await, DaemonToManagerMessage::LocalServiceIncoming { service: actual, .. } if actual == service)
        );
        let closed = manager
            .request(ManagerToDaemonMessage::CloseSession {
                request_id: rid(42),
                session,
            })
            .await;
        assert!(matches!(
            closed,
            DaemonToManagerMessage::SessionClosed { .. }
        ));
        let mut byte = [0_u8; 1];
        assert_eq!(
            timeout(DEADLINE, client.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(
            TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_err()
        );
    }
}
