//! Plan 369 — `i2pr-appd`, the trusted managed-application runtime manager.
//!
//! This crate is a **separate runtime trust zone** (Plan 369 §6). It may depend
//! on the managed-app contracts and on reviewed process/async primitives. It
//! must not depend on `i2pr-daemon`, `i2pr-runtime`, `i2pr-client`, `i2pr-api`,
//! `i2pr-i2pcontrol`, NetDB, tunnel, or transport crates, and it owns no router
//! protocol internals, no NetDB/tunnel/transport state, no Proposal-170
//! administrator credential, and no sandbox enforcement.
//!
//! # What it does own
//!
//! - the outer managed-app v1 handshake/frame/session state (Plan 369 WP4);
//! - application launch-instance identity;
//! - mapping application logical stream ids to daemon manager-protocol handles
//!   (Plan 369 WP4);
//! - process lifecycle for `i2pr-apphost` children (Plan 369 WP3);
//! - effective-capability presentation to applications (Plan 369 WP4);
//! - bounded diagnostics.
//!
//! # WP2 scope
//!
//! This file implements WP2 only: the transport, the state machine's first
//! three states, and the manager-protocol handshake. It **refuses every
//! request**, with a typed reason, because Plan 369 has no package or grant
//! owner. That refusal is the load-bearing security property of this work
//! package, not a stub: Plan 369 §9 requires that launch authority be
//! manager-created and that no decoder from wire bytes ever produce one. With no
//! authority owner, the only correct answer is no.
//!
//! # Security posture
//!
//! - No listener, no socket, no discoverable endpoint. The transport is the two
//!   inherited anonymous pipes in [`transport`], and its possession is the
//!   authentication fact (ADR 0035).
//! - No sandbox attestation type exists anywhere in this crate, and none may be
//!   fabricated. `LaunchProfile::Secured` is refused by the bootstrap contract
//!   before any exec (Plan 369 §2).
//! - No decoder from application protocol messages, manifest bytes, or any
//!   other peer-supplied bytes into authority.

pub mod apphost_launch;
pub mod transport;

use std::collections::BTreeSet;

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, Frame, FrameKind, Handshake, MANAGER_PROTOCOL_MAJOR,
    MANAGER_PROTOCOL_MINOR, MAX_CONTROL_BYTES, ManagerError, ManagerErrorCode,
    ManagerProtocolError, ManagerRole, ManagerToDaemonMessage, decode_manager_to_daemon_control,
    encode_daemon_to_manager_control,
};
use i2pr_app_proto::RequestId;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub use transport::{DuplexTransport, inherited};

/// Bound on one accepted frame's declared payload. The protocol ceiling is
/// re-checked by [`Frame::decode`]; this is the manager's own second bound so a
/// hostile length cannot reach an allocation before the codec sees it.
const MAX_ACCEPTED_FRAME_BYTES: usize = MAX_CONTROL_BYTES;

/// Read granularity for the transport. Fixed, so the inbound buffer's growth is
/// governed solely by the frame ceiling above.
const READ_CHUNK_BYTES: usize = 8 * 1024;

/// Typed manager failures. No variant carries payload bytes or secrets.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AppdError {
    #[error("manager transport closed")]
    TransportClosed,
    #[error("manager protocol violation: {0}")]
    Protocol(#[from] ManagerProtocolError),
    #[error("invalid lifecycle transition: {0} -> {1}")]
    InvalidTransition(&'static str, &'static str),
}

/// The manager lifecycle states that Plan 369 §C defines.
///
/// WP2 implements `Starting`, `ConnectedToRouter`, and `Idle`. `Launching`,
/// `AwaitingHello`, `Running`, `Stopping`, and `Closed` arrive with WP4, which
/// owns the apphost child and the application session. The enum is declared in
/// full now so a later work package extends the machine rather than replacing
/// it, and so `transition` can be written once against the real shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppdState {
    Starting,
    ConnectedToRouter,
    Idle,
    Launching,
    AwaitingHello,
    Running,
    Stopping,
    Closed,
}

impl AppdState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::ConnectedToRouter => "connected_to_router",
            Self::Idle => "idle",
            Self::Launching => "launching",
            Self::AwaitingHello => "awaiting_hello",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Closed => "closed",
        }
    }
}

/// Plan 369 §C state machine. Invalid transitions fail closed.
pub fn transition(from: AppdState, to: AppdState) -> Result<AppdState, AppdError> {
    let legal = matches!(
        (from, to),
        (AppdState::Starting, AppdState::ConnectedToRouter)
            | (AppdState::ConnectedToRouter, AppdState::Idle)
            | (AppdState::Idle, AppdState::Launching)
            | (AppdState::Launching, AppdState::AwaitingHello)
            | (AppdState::AwaitingHello, AppdState::Running)
            | (AppdState::Running, AppdState::Stopping)
            | (AppdState::Launching, AppdState::Stopping)
            | (AppdState::AwaitingHello, AppdState::Stopping)
            | (AppdState::Running, AppdState::Idle)
            | (AppdState::Stopping, AppdState::Closed)
    );
    if legal {
        Ok(to)
    } else {
        Err(AppdError::InvalidTransition(from.as_str(), to.as_str()))
    }
}

/// Bounded accounting of one in-flight request ledger.
///
/// A manager never emits an uncorrelated reply, so every request id is recorded
/// on arrival and retired exactly once. The bound keeps a hostile peer from
/// growing this set without limit.
#[derive(Debug, Default)]
pub struct RequestLedger {
    in_flight: BTreeSet<u32>,
}

impl RequestLedger {
    /// Admit one request id, refusing zero and duplicates.
    pub fn admit(&mut self, request_id: RequestId) -> Result<(), AppdError> {
        let raw = u32::from(request_id);
        if raw == 0 || !self.in_flight.insert(raw) {
            return Err(AppdError::Protocol(ManagerProtocolError::InvalidHandle));
        }
        Ok(())
    }

    /// Retire one request id, refusing an unknown or already-retired id.
    pub fn retire(&mut self, request_id: RequestId) -> Result<(), AppdError> {
        if !self.in_flight.remove(&u32::from(request_id)) {
            return Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest));
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.in_flight.len()
    }

    pub fn is_empty(&self) -> bool {
        self.in_flight.is_empty()
    }
}

/// The trusted manager process body.
///
/// It owns one transport, one state machine, and one bounded request ledger.
/// It holds no authority in WP2: every request is refused with a typed reason,
/// so a manager that somehow received a `create_session` still allocates
/// nothing.
pub struct Appd {
    state: AppdState,
    ledger: RequestLedger,
    rejected: u64,
}

impl Default for Appd {
    fn default() -> Self {
        Self::new()
    }
}

impl Appd {
    pub fn new() -> Self {
        Self {
            state: AppdState::Starting,
            ledger: RequestLedger::default(),
            rejected: 0,
        }
    }

    pub const fn state(&self) -> AppdState {
        self.state
    }

    /// Number of requests this manager refused. WP2 refuses all of them; the
    /// counter is what makes "refused everything" an observable claim rather
    /// than an assertion.
    pub const fn rejected(&self) -> u64 {
        self.rejected
    }

    pub fn in_flight(&self) -> usize {
        self.ledger.len()
    }

    /// Writes the frozen 9-byte manager handshake.
    ///
    /// The daemon reads exactly these bytes before any frame. There is no
    /// credential here on purpose: possession of the inherited transport is the
    /// authentication fact, and a credential would only add a value that could
    /// leak while proving nothing extra.
    pub async fn write_handshake<W>(&mut self, writer: &mut W) -> Result<(), AppdError>
    where
        W: AsyncWrite + Unpin,
    {
        let handshake = Handshake {
            role: ManagerRole::Manager,
            major: MANAGER_PROTOCOL_MAJOR,
            minor: MANAGER_PROTOCOL_MINOR,
        };
        writer
            .write_all(&handshake.encode())
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        writer
            .flush()
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        self.state = transition(self.state, AppdState::ConnectedToRouter)?;
        self.state = transition(self.state, AppdState::Idle)?;
        Ok(())
    }

    /// Drives one transport to EOF or to a protocol violation.
    ///
    /// On return the manager has emitted at most one typed refusal per request
    /// and holds no authority, so a clean return and a failed return leave the
    /// router in the same place.
    pub async fn run<T>(&mut self, mut transport: T) -> Result<(), AppdError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        self.write_handshake(&mut transport).await?;

        let mut buffered: Vec<u8> = Vec::new();
        let mut chunk = vec![0_u8; READ_CHUNK_BYTES];
        loop {
            // Try to consume every whole frame already buffered before reading
            // more. A partial frame stays buffered; the ceiling below is what
            // stops that buffer from growing.
            loop {
                if buffered.is_empty() {
                    break;
                }
                match Frame::decode(&buffered) {
                    Ok((frame, consumed)) => {
                        buffered.drain(..consumed);
                        let reply = self.handle_frame(frame).await?;
                        self.write_frame(&mut transport, reply).await?;
                    }
                    Err(ManagerProtocolError::TruncatedFrame) => break,
                    Err(error) => {
                        self.state = AppdState::Closed;
                        return Err(AppdError::Protocol(error));
                    }
                }
            }
            if buffered.len()
                > MAX_ACCEPTED_FRAME_BYTES + i2pr_app_manager_proto::FRAME_HEADER_BYTES
            {
                self.state = AppdState::Closed;
                return Err(AppdError::Protocol(ManagerProtocolError::LimitExceeded(
                    "control",
                )));
            }

            let read = transport
                .read(&mut chunk)
                .await
                .map_err(|_| AppdError::TransportClosed)?;
            if read == 0 {
                // EOF is the daemon's teardown signal and the manager's only
                // shutdown path. There is no second, discoverable channel.
                self.state = AppdState::Closed;
                return Ok(());
            }
            buffered.extend_from_slice(&chunk[..read]);
        }
    }

    async fn handle_frame(&mut self, frame: Frame) -> Result<DaemonToManagerMessage, AppdError> {
        if frame.kind != FrameKind::Control {
            // The manager-protocol control vocabulary has no manager->daemon
            // data frame. A data frame here is a direction error, not noise.
            return Err(AppdError::Protocol(ManagerProtocolError::MalformedFrame));
        }
        let message =
            decode_manager_to_daemon_control(&frame.payload).map_err(AppdError::Protocol)?;
        self.dispatch(message).await
    }

    async fn write_frame<W>(
        &self,
        writer: &mut W,
        reply: DaemonToManagerMessage,
    ) -> Result<(), AppdError>
    where
        W: AsyncWrite + Unpin,
    {
        let encoded = encode_daemon_to_manager_control(&reply).map_err(AppdError::Protocol)?;
        let out = Frame::control(encoded)
            .encode()
            .map_err(AppdError::Protocol)?;
        writer
            .write_all(&out)
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        writer.flush().await.map_err(|_| AppdError::TransportClosed)
    }

    /// Handles exactly one request, and refuses exactly one.
    ///
    /// WP2 has no package or grant owner, so there is no way to hold a launch
    /// authority. Every variant therefore maps to the same typed refusal. The
    /// `match` is exhaustive so WP4 must make each case deliberate rather than
    /// inheriting this blanket answer by accident.
    async fn dispatch(
        &mut self,
        message: ManagerToDaemonMessage,
    ) -> Result<DaemonToManagerMessage, AppdError> {
        let request_id = match &message {
            ManagerToDaemonMessage::CreateSession { request_id, .. }
            | ManagerToDaemonMessage::CloseSession { request_id, .. }
            | ManagerToDaemonMessage::OpenService { request_id, .. }
            | ManagerToDaemonMessage::CloseService { request_id, .. }
            | ManagerToDaemonMessage::ResetService { request_id, .. }
            | ManagerToDaemonMessage::Health { request_id }
            | ManagerToDaemonMessage::Shutdown { request_id, .. } => *request_id,
        };
        self.ledger.admit(request_id)?;
        let reply = self.refuse(request_id, &message);
        self.ledger.retire(request_id)?;
        Ok(reply)
    }

    /// The typed refusal. WP2's whole authority surface.
    fn refuse(
        &mut self,
        request_id: RequestId,
        message: &ManagerToDaemonMessage,
    ) -> DaemonToManagerMessage {
        self.rejected += 1;
        let reason = match message {
            ManagerToDaemonMessage::CreateSession { .. } => {
                "no launch authority owner exists in Plan 369"
            }
            ManagerToDaemonMessage::Health { .. } => "manager health is reported by the daemon",
            _ => "operation is unavailable until a launch authority owner exists",
        };
        DaemonToManagerMessage::Rejected {
            request_id,
            error: ManagerError {
                code: ManagerErrorCode::UnsupportedOperation,
                diagnostic: Some(reason.to_owned()),
            },
        }
    }
}

/// Convenience for a process body that owns an inherited transport.
pub async fn serve<T>(transport: T) -> Result<(), AppdError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    Appd::new().run(transport).await
}
