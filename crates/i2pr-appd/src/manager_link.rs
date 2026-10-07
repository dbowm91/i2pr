//! Plan 369 — the manager side of the Plan 368 private protocol.
//!
//! # Direction is the load-bearing detail here
//!
//! The manager protocol has two disjoint control vocabularies, one per
//! direction: [`ManagerToDaemonMessage`] is what the manager *sends*, and
//! [`DaemonToManagerMessage`] is what the daemon *sends*. Every tag is distinct,
//! so decoding an inbound frame with the wrong vocabulary cannot produce a
//! mis-parsed message — it produces `InvalidControl`.
//!
//! That is exactly the failure the manager would hit against a real daemon, and
//! it is the failure WP2 shipped: WP2's read loop decoded inbound frames with
//! `decode_manager_to_daemon_control`, and WP2's test peer *also* sent
//! manager-direction frames, so the test asserted the inverted contract and the
//! two mistakes cancelled. Against the real bridge, the first reply the manager
//! read would have ended the transport.
//!
//! So this module decodes inbound frames as [`DaemonToManagerMessage`], and
//! `tests/manager_contract.rs` drives the peer in the daemon's direction.
//!
//! # Why two tasks and not one
//!
//! A manager session is a long-lived duplex: application data frames must keep
//! flowing while an `open_service` request is outstanding, and several sessions
//! are in flight at once. A single read-handle-reply loop cannot do that, so the
//! link is split into one reader and one writer task with a single-writer
//! queue between them. Writes therefore cannot interleave or reorder, and
//! nothing is buffered on a peer's behalf without a bound.
//!
//! # Bounding
//!
//! - one outbound frame queue, `MAX_QUEUED_OUTBOUND_FRAMES`;
//! - one per-session event queue, `MAX_SESSION_EVENT_QUEUE`;
//! - one in-flight request per `RequestId`, refused on duplicate;
//! - one reply deadline, [`MANAGER_REPLY_GRACE`].
//!
//! A session whose event queue is full is dropped rather than buffered behind:
//! the manager never holds unbounded application data for a peer that stopped
//! reading.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, FRAME_HEADER_BYTES, Frame, FrameKind, MAX_CONTROL_BYTES,
    MAX_DATA_FRAME_BYTES, MAX_INFLIGHT_REQUESTS, ManagerProtocolError, ManagerServiceStreamId,
    ManagerSessionId, ManagerToDaemonMessage, ServiceEndReason, decode_daemon_to_manager_control,
    encode_manager_to_daemon_control,
};
use i2pr_app_proto::RequestId;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::AppdError;

/// Bound on frames queued for the single writer task. A full queue means the
/// daemon is not draining; the caller waits rather than growing without limit.
pub const MAX_QUEUED_OUTBOUND_FRAMES: usize = 64;

/// Bound on notifications and data queued for one application session.
pub const MAX_SESSION_EVENT_QUEUE: usize = 64;

/// Bounded wait for one correlated daemon reply.
///
/// The daemon answers an admitted request in the same task that admitted it, so
/// this exists to convert a wedged daemon into a terminated session rather than
/// to absorb normal latency.
pub const MANAGER_REPLY_GRACE: std::time::Duration = std::time::Duration::from_secs(15);

/// Read granularity. Fixed, so inbound buffering is governed solely by the frame
/// ceiling below.
const READ_CHUNK_BYTES: usize = 8 * 1024;

/// Largest frame header plus payload the manager will hold for one frame.
const MAX_BUFFERED_FRAME_BYTES: usize = FRAME_HEADER_BYTES
    + if MAX_DATA_FRAME_BYTES > MAX_CONTROL_BYTES {
        MAX_DATA_FRAME_BYTES
    } else {
        MAX_CONTROL_BYTES
    };

/// Something the daemon said about one session that is not a reply to a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManagerEvent {
    /// One backend ended. The stream binding stays until the session removes it,
    /// because the daemon's watcher and its data pump are separate tasks and data
    /// already in flight can legitimately land after this.
    StreamEnded {
        stream: ManagerServiceStreamId,
        reason: ServiceEndReason,
    },
    /// The whole session and every descendant stream died.
    SessionEnded { reason: ServiceEndReason },
    /// Exact backend octets for one stream.
    Data {
        stream: ManagerServiceStreamId,
        payload: Vec<u8>,
    },
}

/// Routing the reader performs on behalf of a waiting caller.
///
/// Without this, a notification emitted between "reply delivered" and "caller
/// registered its route" would be dropped. Doing the registration inside the
/// reader, before the reply is handed over, removes that window entirely.
///
/// Only the *session* route needs an explicit bind. A service stream's handle is
/// daemon-assigned and unknown to the caller at request time, but the
/// `service_opened` reply names both the session and the stream, so the reader
/// can bind it itself — and it must, because the daemon's backend pump can
/// enqueue data as soon as it has opened the service.
enum Bind {
    /// Register this request's session event channel under the id the daemon
    /// assigns in `session_opened`.
    Session(mpsc::Sender<ManagerEvent>),
}

struct Pending {
    reply: oneshot::Sender<DaemonToManagerMessage>,
    bind: Option<Bind>,
}

#[derive(Default)]
struct LinkState {
    pending: BTreeMap<u32, Pending>,
    sessions: BTreeMap<ManagerSessionId, mpsc::Sender<ManagerEvent>>,
    streams: BTreeMap<ManagerServiceStreamId, ManagerSessionId>,
    /// Terminal state. Once set, every later request fails immediately instead of
    /// queueing onto a transport that will never answer.
    closed: bool,
    /// Data frames for a handle this manager never opened. Counted so a repeated
    /// direction error is visible rather than merely refused.
    unroutable_data: u64,
}

/// The manager's handle on the daemon transport.
///
/// Cheap to clone; every clone shares one writer task, one reader task, and one
/// routing table.
#[derive(Clone)]
pub struct ManagerLink {
    state: Arc<Mutex<LinkState>>,
    outbound: mpsc::Sender<Vec<u8>>,
    next_request: Arc<AtomicU32>,
}

impl ManagerLink {
    /// Splits `transport`, starts the reader and writer tasks, and returns the
    /// link plus a handle that completes when the transport ends.
    ///
    /// Nothing is written before the caller sends: the manager greeting is
    /// written directly onto the transport before `drive` is called, so the
    /// daemon always sees the greeting first.
    pub fn drive<R, W>(read: R, write: W) -> (Self, JoinHandle<Result<(), AppdError>>)
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (outbound, queue) = mpsc::channel(MAX_QUEUED_OUTBOUND_FRAMES);
        let state = Arc::new(Mutex::new(LinkState::default()));
        // A watch, not a queue close: `mpsc::Sender` has no `close_channel`, and
        // dropping senders is not an option because every `ManagerLink` clone is
        // one. The reader is the only party that can observe the daemon going
        // away, so it owns this signal.
        let (stop_tx, stop_rx) = watch::channel(false);
        let writer = tokio::spawn(writer_task(write, queue, stop_rx));

        let driver = tokio::spawn({
            let state = Arc::clone(&state);
            async move {
                let read_outcome = reader_task(read, Arc::clone(&state)).await;
                // Best effort: if the writer already returned this is a no-op.
                let _ = stop_tx.send(true);
                let write_outcome = writer
                    .await
                    .map_err(|_| AppdError::TransportClosed)?
                    .and(Ok(()));
                read_outcome.and(write_outcome)
            }
        });

        (
            Self {
                state,
                outbound: outbound.clone(),
                next_request: Arc::new(AtomicU32::new(0)),
            },
            driver,
        )
    }

    /// Sends one correlated request and waits for its reply.
    pub async fn request(
        &self,
        message: ManagerToDaemonMessage,
    ) -> Result<DaemonToManagerMessage, AppdError> {
        self.request_inner(message, None, None).await
    }

    /// A request on a caller-chosen deadline.
    ///
    /// Used for teardown paths, where the manager is already exiting and a
    /// wedged daemon must not extend the shutdown by the full reply grace.
    pub async fn request_within(
        &self,
        message: ManagerToDaemonMessage,
        grace: std::time::Duration,
    ) -> Result<DaemonToManagerMessage, AppdError> {
        self.request_inner(message, None, Some(grace)).await
    }

    /// `create_session`, with the session event channel registered by the reader
    /// before the reply is delivered.
    ///
    /// Returning the receiver here rather than handing it out later is what makes
    /// "no notification can be missed between open and use" true rather than
    /// approximately true.
    pub async fn open_session(
        &self,
        message: ManagerToDaemonMessage,
        events: mpsc::Sender<ManagerEvent>,
    ) -> Result<DaemonToManagerMessage, AppdError> {
        self.request_inner(message, Some(Bind::Session(events)), None)
            .await
    }

    /// Allocates a transport-unique request id.
    ///
    /// Unique across the whole link, not per session: the pending ledger is keyed
    /// by request id alone, so two sessions sharing an id would make one
    /// application's reply correlate to another's request.
    pub fn allocate_request_id(&self) -> RequestId {
        loop {
            let raw = self
                .next_request
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
            // Zero is reserved by the contract, so a counter that wraps past it
            // is nudged on rather than sent.
            let raw = if raw == 0 { 1 } else { raw };
            if let Ok(request_id) = RequestId::new(raw) {
                return request_id;
            }
        }
    }

    async fn request_inner(
        &self,
        message: ManagerToDaemonMessage,
        bind: Option<Bind>,
        grace: Option<std::time::Duration>,
    ) -> Result<DaemonToManagerMessage, AppdError> {
        let request_id = correlation_of(&message);
        let key = u32::from(request_id);
        let (reply_tx, reply_rx) = oneshot::channel();

        {
            let mut state = self.state.lock().await;
            if state.closed {
                return Err(AppdError::TransportClosed);
            }
            if state.pending.len() >= MAX_INFLIGHT_REQUESTS {
                // Bounded independently of the outbound queue: a caller that
                // never collects replies would otherwise grow this map for ever.
                return Err(AppdError::Protocol(ManagerProtocolError::LimitExceeded(
                    "in-flight manager requests",
                )));
            }
            // Duplicate and zero ids are refused before anything is queued.
            if state.pending.contains_key(&key) {
                return Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest));
            }
            state.pending.insert(
                key,
                Pending {
                    reply: reply_tx,
                    bind,
                },
            );
        }

        let encoded = encode_manager_to_daemon_control(&message).map_err(AppdError::Protocol)?;
        let bytes = Frame::control(encoded)
            .encode()
            .map_err(AppdError::Protocol)?;
        if self.outbound.send(bytes).await.is_err() {
            self.abandon(key).await;
            return Err(AppdError::TransportClosed);
        }

        let reply = match tokio::time::timeout(grace.unwrap_or(MANAGER_REPLY_GRACE), reply_rx).await
        {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => {
                self.abandon(key).await;
                return Err(AppdError::TransportClosed);
            }
            Err(_) => {
                self.abandon(key).await;
                return Err(AppdError::ManagerTimeout);
            }
        };
        Ok(reply)
    }

    /// Drops a request that will never be answered, so its id can be reused by a
    /// later caller's genuinely new request rather than wedging the ledger.
    async fn abandon(&self, key: u32) {
        let mut state = self.state.lock().await;
        state.pending.remove(&key);
    }

    /// Forwards exact backend octets for one stream.
    pub async fn send_data(
        &self,
        stream: ManagerServiceStreamId,
        payload: Vec<u8>,
    ) -> Result<(), AppdError> {
        let bytes = Frame::data(stream, payload)
            .encode()
            .map_err(AppdError::Protocol)?;
        self.outbound
            .send(bytes)
            .await
            .map_err(|_| AppdError::TransportClosed)
    }

    /// Removes one stream binding once the session no longer owns it.
    pub async fn unbind_stream(&self, stream: ManagerServiceStreamId) {
        self.state.lock().await.streams.remove(&stream);
    }

    /// Removes a session and every binding that descended from it.
    ///
    /// Called on every teardown path, including app EOF and session end, so a
    /// dead session cannot keep receiving daemon traffic.
    pub async fn drop_session(&self, session: ManagerSessionId) {
        let mut state = self.state.lock().await;
        state.sessions.remove(&session);
        let owned: Vec<ManagerServiceStreamId> = state
            .streams
            .iter()
            .filter(|(_, owner)| **owner == session)
            .map(|(stream, _)| *stream)
            .collect();
        for stream in owned {
            state.streams.remove(&stream);
        }
    }

    /// Live stream bindings this link holds. Used by tests to prove a teardown
    /// released them.
    pub async fn bound_streams(&self) -> usize {
        self.state.lock().await.streams.len()
    }

    /// Live session routes this link holds.
    pub async fn routed_sessions(&self) -> usize {
        self.state.lock().await.sessions.len()
    }

    /// Data frames refused because they named a handle this manager never
    /// opened.
    pub async fn unroutable_data(&self) -> u64 {
        self.state.lock().await.unroutable_data
    }

    /// Marks the link terminal and fails every outstanding request.
    pub async fn close(&self) {
        let mut state = self.state.lock().await;
        state.closed = true;
        state.pending.clear();
        state.sessions.clear();
        state.streams.clear();
    }
}

/// The request id a manager-originated message correlates on.
fn correlation_of(message: &ManagerToDaemonMessage) -> RequestId {
    match message {
        ManagerToDaemonMessage::CreateSession { request_id, .. }
        | ManagerToDaemonMessage::CloseSession { request_id, .. }
        | ManagerToDaemonMessage::OpenService { request_id, .. }
        | ManagerToDaemonMessage::CloseService { request_id, .. }
        | ManagerToDaemonMessage::ResetService { request_id, .. }
        | ManagerToDaemonMessage::Health { request_id }
        | ManagerToDaemonMessage::Shutdown { request_id, .. } => *request_id,
    }
}

/// Reads frames and routes them until the transport ends or a violation occurs.
async fn reader_task<R>(reader: R, state: Arc<Mutex<LinkState>>) -> Result<(), AppdError>
where
    R: AsyncRead + Unpin,
{
    let mut reader = reader;
    let mut buffered: Vec<u8> = Vec::new();
    let mut chunk = vec![0_u8; READ_CHUNK_BYTES];

    loop {
        // Consume every whole frame already buffered before reading more. A
        // partial frame stays buffered; the ceiling below bounds that buffer.
        loop {
            if buffered.is_empty() {
                break;
            }
            match Frame::decode(&buffered) {
                Ok((frame, consumed)) => {
                    buffered.drain(..consumed);
                    route(&state, frame).await?;
                }
                Err(ManagerProtocolError::TruncatedFrame) => break,
                Err(error) => return Err(AppdError::Protocol(error)),
            }
        }

        if buffered.len() > MAX_BUFFERED_FRAME_BYTES {
            return Err(AppdError::Protocol(ManagerProtocolError::LimitExceeded(
                "manager frame",
            )));
        }

        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        if read == 0 {
            // EOF is the daemon's teardown signal and the manager's only
            // shutdown path. Every outstanding request is failed so no session
            // waits out its full reply grace on a transport that is gone.
            let mut state = state.lock().await;
            state.closed = true;
            state.pending.clear();
            state.sessions.clear();
            state.streams.clear();
            return Ok(());
        }
        buffered.extend_from_slice(&chunk[..read]);
    }
}

async fn route(state: &Arc<Mutex<LinkState>>, frame: Frame) -> Result<(), AppdError> {
    if frame.kind != FrameKind::Control {
        let handle = ManagerServiceStreamId::try_from(u64::from(frame.stream_id))
            .map_err(|_| AppdError::Protocol(ManagerProtocolError::InvalidHandle))?;
        let mut state = state.lock().await;
        let Some(&session) = state.streams.get(&handle) else {
            // Data naming a handle this manager never opened is a direction
            // error, not noise: forwarding it would let a stale handle look
            // live. Data for a stream that merely *ended* still has a binding,
            // because the session removes it deliberately.
            state.unroutable_data += 1;
            return Err(AppdError::Protocol(ManagerProtocolError::InvalidHandle));
        };
        let Some(events) = state.sessions.get(&session) else {
            state.unroutable_data += 1;
            return Err(AppdError::Protocol(ManagerProtocolError::InvalidHandle));
        };
        if events
            .try_send(ManagerEvent::Data {
                stream: handle,
                payload: frame.payload,
            })
            .is_err()
        {
            // The session is gone or is not draining. Dropping the route makes
            // the daemon see the session end rather than leaving it open with
            // nobody reading.
            state.sessions.remove(&session);
            let owned: Vec<ManagerServiceStreamId> = state
                .streams
                .iter()
                .filter(|(_, owner)| **owner == session)
                .map(|(stream, _)| *stream)
                .collect();
            for stream in owned {
                state.streams.remove(&stream);
            }
        }
        return Ok(());
    }

    let message = decode_daemon_to_manager_control(&frame.payload).map_err(AppdError::Protocol)?;
    let mut state = state.lock().await;

    let Some(request_id) = message.correlation() else {
        return notify(&mut state, message);
    };
    let key = u32::from(request_id);
    let Some(entry) = state.pending.remove(&key) else {
        // A correlated reply for a request this manager never sent. The manager
        // initiates every request in this protocol, so there is no legitimate
        // reading of this message.
        return Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest));
    };
    apply_bind(&mut state, entry.bind, &message)?;
    // A `service_opened` also mints the handle, so its binding is registered
    // here rather than by the caller — the daemon's backend pump can enqueue
    // data for it before the caller has even read the reply.
    if let DaemonToManagerMessage::ServiceOpened {
        session, stream, ..
    } = &message
    {
        if !state.sessions.contains_key(session) {
            // A service opened in a session this manager has no route for: the
            // bytes would have nowhere to go. Fail closed rather than register a
            // dead binding.
            return Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest));
        }
        state.streams.insert(*stream, *session);
    }
    // A dropped receiver means the caller already gave up; the reply is simply
    // discarded, which is not a protocol fault.
    let _ = entry.reply.send(message);
    Ok(())
}

fn apply_bind(
    state: &mut LinkState,
    bind: Option<Bind>,
    message: &DaemonToManagerMessage,
) -> Result<(), AppdError> {
    let Some(Bind::Session(events)) = bind else {
        return Ok(());
    };
    match message {
        // A refusal means nothing was allocated, so there is nothing to bind.
        DaemonToManagerMessage::Rejected { .. } => Ok(()),
        DaemonToManagerMessage::SessionOpened { session, .. } => {
            state.sessions.insert(*session, events);
            Ok(())
        }
        _ => Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest)),
    }
}

/// Routes an unsolicited daemon message. Notifications carry no correlation, so
/// anything correlated that reached here would already have been dispatched.
fn notify(state: &mut LinkState, message: DaemonToManagerMessage) -> Result<(), AppdError> {
    match message {
        DaemonToManagerMessage::ServiceEnded {
            session,
            stream,
            reason,
        } => {
            // The binding is deliberately kept: the daemon's end-watcher and its
            // data pump are separate tasks, so data already queued can still
            // arrive. The session removes the binding when it acts on the event.
            if let Some(events) = state.sessions.get(&session) {
                let _ = events.try_send(ManagerEvent::StreamEnded { stream, reason });
            }
            Ok(())
        }
        DaemonToManagerMessage::SessionEnded { session, reason } => {
            // A session with no route is a late notification for a session this
            // manager already tore down. Dropping it is correct, not an error.
            if let Some(events) = state.sessions.remove(&session) {
                let _ = events.try_send(ManagerEvent::SessionEnded { reason });
            }
            Ok(())
        }
        _ => Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest)),
    }
}

/// The only writer. Owns the write half so frames cannot interleave.
async fn writer_task<W>(
    mut writer: W,
    mut queue: mpsc::Receiver<Vec<u8>>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), AppdError>
where
    W: AsyncWrite + Unpin,
{
    loop {
        let bytes = tokio::select! {
            biased;
            // The reader saw EOF or a violation. Queued frames are dropped
            // deliberately: writing them onto a transport the daemon may already
            // have torn down would just delay the manager's exit.
            _ = wait_for_stop(&mut stop) => break,
            bytes = queue.recv() => match bytes {
                Some(bytes) => bytes,
                // Every sender is gone, so no frame will ever be queued again.
                None => break,
            },
        };
        writer
            .write_all(&bytes)
            .await
            .map_err(|_| AppdError::TransportClosed)?;
    }
    // Shutting the write half down lets the daemon observe the manager finishing
    // rather than waiting for its own teardown.
    writer
        .shutdown()
        .await
        .map_err(|_| AppdError::TransportClosed)
}

async fn wait_for_stop(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() {
            return;
        }
        if stop.changed().await.is_err() {
            // The sender is gone, which means the reader task ended. Treat that
            // as the stop signal rather than waiting for a value that can no
            // longer arrive.
            return;
        }
    }
}
