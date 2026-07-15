//! Runtime-owned authenticated NTCP2 data-phase link composition.
//!
//! The link keeps directional frame state in supervised reader/writer
//! children. The runtime queue carries block owners, never plaintext bytes or
//! an unframed raw-link payload. Every queued command owns its item/byte
//! release through `Drop`, including cancellation, receiver closure, and
//! supervisor teardown.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use i2pr_transport_ntcp2::block::Block;
use i2pr_transport_ntcp2::constants::MAX_WIRE_FRAME_LENGTH;
use i2pr_transport_ntcp2::frame::{
    FrameAssemblyPolicy, FrameError, ReceiveState, ReceivedFrame, TransmitState,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::{
    ActiveLinkAdmission, ActiveLinkAdmissionError, ActiveLinkPermit, AddressFamily,
    CancellationToken, ChildScope, ExactIoError, IoErrorKind, Ntcp2Deadline, Ntcp2RuntimeDeadlines,
    Ntcp2RuntimeLimits, WriteOutcome, read_exact, write_all_exact,
};

const MAX_FRAME_QUEUE_BYTES: usize = MAX_WIRE_FRAME_LENGTH;

#[derive(Default)]
struct QueueCounts {
    outbound_items: usize,
    outbound_bytes: usize,
    inbound_items: usize,
    inbound_bytes: usize,
}

#[derive(Default)]
struct LinkState {
    queue: Mutex<QueueCounts>,
    read_frames: AtomicU64,
    written_frames: AtomicU64,
    read_bytes: AtomicU64,
    written_bytes: AtomicU64,
    queue_release_underflows: AtomicU64,
    closed: AtomicBool,
}

impl LinkState {
    fn reserve_outbound(&self, bytes: usize, limits: Ntcp2RuntimeLimits) -> bool {
        let Ok(mut queue) = self.queue.lock() else {
            return false;
        };
        if queue.outbound_items >= limits.max_link_queue_items
            || queue.outbound_bytes.saturating_add(bytes) > limits.max_link_queue_bytes
        {
            return false;
        }
        queue.outbound_items += 1;
        queue.outbound_bytes += bytes;
        true
    }

    fn reserve_inbound(&self, bytes: usize, limits: Ntcp2RuntimeLimits) -> bool {
        let Ok(mut queue) = self.queue.lock() else {
            return false;
        };
        if queue.inbound_items >= limits.max_link_queue_items
            || queue.inbound_bytes.saturating_add(bytes) > limits.max_link_queue_bytes
        {
            return false;
        }
        queue.inbound_items += 1;
        queue.inbound_bytes += bytes;
        true
    }

    fn release_outbound(&self, bytes: usize) {
        let Ok(mut queue) = self.queue.lock() else {
            self.queue_release_underflows
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        if queue.outbound_items == 0 || queue.outbound_bytes < bytes {
            self.queue_release_underflows
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        queue.outbound_items -= 1;
        queue.outbound_bytes -= bytes;
    }

    fn release_inbound(&self, bytes: usize) {
        let Ok(mut queue) = self.queue.lock() else {
            self.queue_release_underflows
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        if queue.inbound_items == 0 || queue.inbound_bytes < bytes {
            self.queue_release_underflows
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        queue.inbound_items -= 1;
        queue.inbound_bytes -= bytes;
    }

    fn queue_snapshot(&self) -> (usize, usize) {
        self.queue
            .lock()
            .map(|queue| {
                (
                    queue.outbound_items.saturating_add(queue.inbound_items),
                    queue.outbound_bytes.saturating_add(queue.inbound_bytes),
                )
            })
            .unwrap_or_default()
    }
}

/// A runtime-owned authenticated frame that releases its inbound queue lease
/// when dropped.
pub struct ReceivedFrameLease {
    frame: Option<ReceivedFrame>,
    state: Arc<LinkState>,
    bytes: usize,
    released: bool,
}

impl ReceivedFrameLease {
    /// Borrows the authenticated frame and its parsed, bounded plaintext.
    pub fn frame(&self) -> &ReceivedFrame {
        self.frame
            .as_ref()
            .expect("received frame lease was consumed")
    }

    /// Consumes the queue lease and returns the authenticated frame owner.
    pub fn into_frame(mut self) -> ReceivedFrame {
        self.release();
        self.frame
            .take()
            .expect("received frame lease was not consumed")
    }

    fn release(&mut self) {
        if !self.released {
            self.state.release_inbound(self.bytes);
            self.released = true;
        }
    }
}

impl fmt::Debug for ReceivedFrameLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReceivedFrameLease")
            .field("bytes", &self.bytes)
            .field("frame", &self.frame.as_ref().map(|_| "<authenticated>"))
            .finish()
    }
}

impl Drop for ReceivedFrameLease {
    fn drop(&mut self) {
        self.release();
    }
}

struct FrameCommand {
    blocks: Option<Vec<Block>>,
    policy: FrameAssemblyPolicy,
    state: Arc<LinkState>,
    bytes: usize,
}

impl Drop for FrameCommand {
    fn drop(&mut self) {
        self.state.release_outbound(self.bytes);
    }
}

/// Typed failures from an authenticated link owner.
#[derive(Debug, Eq, PartialEq)]
pub enum AuthenticatedLinkError {
    /// The bounded queue could not admit the frame before its deadline.
    QueueFull,
    /// The caller cancelled the queue operation.
    Cancelled,
    /// The link is closed.
    Closed,
    /// A frame or block violated the runtime-neutral protocol bounds.
    Frame(FrameError),
    /// A bounded socket operation failed.
    Io(IoErrorKind),
    /// The child scope could not retain the link owner.
    ChildScope,
}

impl fmt::Display for AuthenticatedLinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueueFull => formatter.write_str("authenticated NTCP2 link queue is full"),
            Self::Cancelled => formatter.write_str("authenticated NTCP2 link operation cancelled"),
            Self::Closed => formatter.write_str("authenticated NTCP2 link is closed"),
            Self::Frame(error) => error.fmt(formatter),
            Self::Io(kind) => write!(formatter, "authenticated NTCP2 link I/O failed: {kind:?}"),
            Self::ChildScope => {
                formatter.write_str("authenticated NTCP2 link child scope rejected")
            }
        }
    }
}

impl std::error::Error for AuthenticatedLinkError {}

/// Aggregate authenticated-link counters without payloads or endpoints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedLinkSnapshot {
    /// Runtime link identifier.
    pub id: u64,
    /// Coarse address family.
    pub family: AddressFamily,
    /// Queued item count across both directions.
    pub queued_items: usize,
    /// Queued byte count across both directions.
    pub queued_bytes: usize,
    /// Authenticated frames received.
    pub received_frames: u64,
    /// Authenticated frames written.
    pub written_frames: u64,
    /// Wire bytes read.
    pub read_bytes: u64,
    /// Wire bytes written.
    pub written_bytes: u64,
    /// Whether cancellation/terminal processing has started.
    pub closed: bool,
    /// Queue-release underflow diagnostics.
    pub queue_release_underflows: u64,
}

/// A supervised runtime owner for one authenticated NTCP2 data session.
pub struct AuthenticatedLink {
    id: u64,
    family: AddressFamily,
    sender: mpsc::Sender<FrameCommand>,
    receiver: mpsc::Receiver<ReceivedFrameLease>,
    cancellation: CancellationToken,
    state: Arc<LinkState>,
    limits: Ntcp2RuntimeLimits,
    _active_permit: ActiveLinkPermit,
}

impl fmt::Debug for AuthenticatedLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedLink")
            .field("id", &self.id)
            .field("family", &self.family)
            .field("state", &self.snapshot())
            .finish()
    }
}

impl AuthenticatedLink {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_with_admission(
        scope: &ChildScope,
        stream: TcpStream,
        family: AddressFamily,
        id: u64,
        transmit: TransmitState,
        receive: ReceiveState,
        limits: Ntcp2RuntimeLimits,
        deadlines: Ntcp2RuntimeDeadlines,
        admission: &ActiveLinkAdmission,
    ) -> Result<Self, AuthenticatedLinkStartError> {
        let active_permit = admission
            .admit()
            .map_err(AuthenticatedLinkStartError::Admission)?;
        let (sender, command_receiver) = mpsc::channel(limits.max_link_queue_items);
        let (event_sender, receiver) = mpsc::channel(limits.max_link_queue_items);
        let cancellation = CancellationToken::new();
        let state = Arc::new(LinkState::default());
        let (mut reader, mut writer) = stream.into_split();
        let writer_cancel = cancellation.clone();
        let writer_state = Arc::clone(&state);
        if scope
            .spawn(move |child| async move {
                writer_task(
                    &mut writer,
                    command_receiver,
                    transmit,
                    deadlines,
                    child,
                    writer_cancel,
                    writer_state,
                )
                .await;
                Ok(())
            })
            .is_err()
        {
            return Err(AuthenticatedLinkStartError::ChildScope);
        }
        let reader_cancel = cancellation.clone();
        let reader_state = Arc::clone(&state);
        let reader_limits = limits;
        if scope
            .spawn(move |child| async move {
                reader_task(
                    &mut reader,
                    event_sender,
                    receive,
                    deadlines,
                    reader_limits,
                    child,
                    reader_cancel,
                    reader_state,
                )
                .await;
                Ok(())
            })
            .is_err()
        {
            let _ = cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
            return Err(AuthenticatedLinkStartError::ChildScope);
        }
        Ok(Self {
            id,
            family,
            sender,
            receiver,
            cancellation,
            state,
            limits,
            _active_permit: active_permit,
        })
    }

    /// Queues blocks for bounded frame assembly and authenticated writing.
    pub async fn send_blocks(
        &self,
        blocks: Vec<Block>,
        policy: FrameAssemblyPolicy,
        deadline: Ntcp2Deadline,
        cancellation: &CancellationToken,
    ) -> Result<WriteOutcome, AuthenticatedLinkError> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(AuthenticatedLinkError::Closed);
        }
        let block_bytes = blocks.iter().try_fold(0_usize, |total, block| {
            total.checked_add(block.encoded_len())
        });
        let estimated = block_bytes
            .and_then(|bytes| bytes.checked_add(3 + policy.selected_padding))
            .and_then(|bytes| bytes.checked_add(2 + 16))
            .ok_or(AuthenticatedLinkError::Frame(FrameError::PayloadTooLarge))?;
        if estimated > MAX_FRAME_QUEUE_BYTES {
            return Err(AuthenticatedLinkError::Frame(FrameError::PayloadTooLarge));
        }
        if !self.state.reserve_outbound(estimated, self.limits) {
            return Err(AuthenticatedLinkError::QueueFull);
        }
        let command = FrameCommand {
            blocks: Some(blocks),
            policy,
            state: Arc::clone(&self.state),
            bytes: estimated,
        };
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(AuthenticatedLinkError::Cancelled),
            _ = self.cancellation.cancelled() => Err(AuthenticatedLinkError::Closed),
            result = tokio::time::timeout(deadline.remaining(), self.sender.send(command)) => {
                match result {
                    Ok(Ok(())) => Ok(WriteOutcome::Accepted),
                    Ok(Err(_)) => Err(AuthenticatedLinkError::Closed),
                    Err(_) => Err(AuthenticatedLinkError::QueueFull),
                }
            }
        };
        result
    }

    /// Receives the next authenticated frame, retaining its bounded queue lease.
    pub async fn recv(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<Option<ReceivedFrameLease>, AuthenticatedLinkError> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(AuthenticatedLinkError::Cancelled),
            _ = self.cancellation.cancelled() => Ok(None),
            frame = self.receiver.recv() => Ok(frame),
        }
    }

    /// Requests cancellation of both supervised link children.
    pub fn close(&self) {
        let _ = self
            .cancellation
            .cancel(i2pr_core::CancellationReason::OperatorRequest);
        self.state.closed.store(true, Ordering::Release);
    }

    /// Returns privacy-safe authenticated frame and queue counters.
    pub fn snapshot(&self) -> AuthenticatedLinkSnapshot {
        let (queued_items, queued_bytes) = self.state.queue_snapshot();
        AuthenticatedLinkSnapshot {
            id: self.id,
            family: self.family,
            queued_items,
            queued_bytes,
            received_frames: self.state.read_frames.load(Ordering::Acquire),
            written_frames: self.state.written_frames.load(Ordering::Acquire),
            read_bytes: self.state.read_bytes.load(Ordering::Acquire),
            written_bytes: self.state.written_bytes.load(Ordering::Acquire),
            closed: self.state.closed.load(Ordering::Acquire),
            queue_release_underflows: self.state.queue_release_underflows.load(Ordering::Acquire),
        }
    }
}

impl Drop for AuthenticatedLink {
    fn drop(&mut self) {
        self.close();
    }
}

/// Failure while starting a supervised authenticated link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticatedLinkStartError {
    /// The active-link admission limit was reached.
    Admission(ActiveLinkAdmissionError),
    /// The configured queue byte ceiling cannot hold a protocol frame.
    QueueLimitTooLarge,
    /// The child scope could not retain both link children.
    ChildScope,
}

impl fmt::Display for AuthenticatedLinkStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission(error) => error.fmt(formatter),
            Self::QueueLimitTooLarge => {
                formatter.write_str("authenticated NTCP2 queue limit is too small")
            }
            Self::ChildScope => {
                formatter.write_str("authenticated NTCP2 link child scope rejected")
            }
        }
    }
}

impl std::error::Error for AuthenticatedLinkStartError {}

async fn writer_task(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    mut commands: mpsc::Receiver<FrameCommand>,
    mut transmit: TransmitState,
    deadlines: Ntcp2RuntimeDeadlines,
    child: CancellationToken,
    cancellation: CancellationToken,
    state: Arc<LinkState>,
) {
    loop {
        let mut command = tokio::select! {
            biased;
            _ = child.cancelled() => break,
            _ = cancellation.cancelled() => break,
            command = commands.recv() => match command {
                Some(command) => command,
                None => break,
            },
        };
        let blocks = command.blocks.take().unwrap_or_default();
        let bytes = match transmit.seal_blocks(blocks, command.policy) {
            Ok(frame) => frame.into_bytes(),
            Err(_) => {
                state.closed.store(true, Ordering::Release);
                let _ = cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
                break;
            }
        };
        let length = bytes.len();
        let result = match Ntcp2Deadline::after(deadlines.write) {
            Ok(deadline) => write_all_exact(writer, &bytes, deadline, &cancellation).await,
            Err(_) => Err(ExactIoError {
                kind: IoErrorKind::Deadline,
            }),
        };
        if result.is_err() {
            state.closed.store(true, Ordering::Release);
            let _ = cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
            break;
        }
        state.written_frames.fetch_add(1, Ordering::Relaxed);
        state
            .written_bytes
            .fetch_add(length as u64, Ordering::Relaxed);
        drop(command);
    }
}

#[allow(clippy::too_many_arguments)]
async fn reader_task(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    sender: mpsc::Sender<ReceivedFrameLease>,
    mut receive: ReceiveState,
    deadlines: Ntcp2RuntimeDeadlines,
    limits: Ntcp2RuntimeLimits,
    child: CancellationToken,
    cancellation: CancellationToken,
    state: Arc<LinkState>,
) {
    loop {
        let mut prefix = [0_u8; 2];
        let deadline = match Ntcp2Deadline::after(deadlines.read_idle) {
            Ok(value) => value,
            Err(_) => break,
        };
        if read_exact(reader, &mut prefix, deadline, &cancellation)
            .await
            .is_err()
        {
            break;
        }
        let length = match receive.decode_length(prefix) {
            Ok(value) => value,
            Err(_) => break,
        };
        let mut ciphertext = vec![0_u8; length.ciphertext_length];
        let deadline = match Ntcp2Deadline::after(deadlines.read_idle) {
            Ok(value) => value,
            Err(_) => break,
        };
        if read_exact(reader, &mut ciphertext, deadline, &cancellation)
            .await
            .is_err()
        {
            break;
        }
        let frame = match receive.open_ciphertext(&ciphertext) {
            Ok(value) => value,
            Err(_) => break,
        };
        let wire_bytes = 2 + length.ciphertext_length;
        if !state.reserve_inbound(wire_bytes, limits) {
            break;
        }
        let lease = ReceivedFrameLease {
            frame: Some(frame),
            state: Arc::clone(&state),
            bytes: wire_bytes,
            released: false,
        };
        state.read_frames.fetch_add(1, Ordering::Relaxed);
        state
            .read_bytes
            .fetch_add(wire_bytes as u64, Ordering::Relaxed);
        let send_result = tokio::select! {
            biased;
            _ = child.cancelled() => Err(()),
            _ = cancellation.cancelled() => Err(()),
            result = sender.send(lease) => result.map_err(|_| ()),
        };
        if send_result.is_err() {
            break;
        }
    }
    state.closed.store(true, Ordering::Release);
    let _ = cancellation.cancel(i2pr_core::CancellationReason::ParentScope);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_release_returns_to_baseline() {
        let state = Arc::new(LinkState::default());
        let limits = Ntcp2RuntimeLimits::default();
        assert!(state.reserve_outbound(32, limits));
        state.release_outbound(32);
        assert_eq!(state.queue_snapshot(), (0, 0));
        state.release_outbound(1);
        assert_eq!(state.queue_release_underflows.load(Ordering::Acquire), 1);
    }
}
