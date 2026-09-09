//! Plan 174 shared daemon Streaming runtime.
//!
//! The proven SAM-specific socket pump from `sam::raw_stream` is
//! separated from SAM protocol state here. This module owns the
//! protocol-agnostic bounded socket <-> Streaming byte pump; SAM
//! adapts it through a narrow capability rather than retaining a
//! second duplicate pump.
//!
//! ```text
//! local TCP/Unix socket
//!        <-> bounded supervised byte pump (this module)
//!        <-> StreamingManager (via StreamPumpEndpoint)
//!        <-> StreamingDestinationAdapter
//!        <-> existing DestinationRuntime / routing
//! ```
//!
//! Ownership:
//! - the pump owns one local socket after handoff;
//! - the endpoint capability owns exactly one Streaming connection
//!   (sibling isolation is enforced by draining only that
//!   connection's bytes);
//! - cancellation, EOF, and remote-terminal convergence terminate
//!   the pump; terminal CLOSE/RESET packet emission and SAM
//!   attachment release remain with the protocol adapter
//!   (`sam::raw_stream::finish_raw_stream`).
//!
//! Properties retained from Plans 147/151:
//! - bounded per-read chunk;
//! - negotiated Streaming segmentation;
//! - send-window backpressure without busy spin;
//! - sibling-stream isolation;
//! - fair opportunity for ACK/delivery driver progress;
//! - cancellation/EOF/remote terminal close convergence;
//! - no command parser retains the raw socket after handoff.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Direction of the underlying Streaming connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PumpDirection {
    /// Local endpoint originated the connection.
    Outbound,
    /// Local endpoint accepted the connection.
    Inbound,
}

/// Disposition of one `try_send` segment admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PumpSendDisposition {
    /// The Streaming send window accepted the segment.
    Accepted,
    /// The Streaming send window is full; the caller must park the
    /// remainder and retry after a bounded sleep.
    Backpressured,
}

/// Typed endpoint failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PumpEndpointError {
    /// The Streaming connection is unknown (already released).
    #[error("unknown streaming connection")]
    UnknownConnection,
    /// The Streaming connection is in a state that rejects data.
    #[error("invalid streaming connection state")]
    InvalidState,
    /// The Streaming manager rejected the segment.
    #[error("streaming manager: {0}")]
    Streaming(String),
}

/// Narrow capability for exactly one Streaming connection.
///
/// Implementations must:
/// - report the negotiated maximum payload size;
/// - admit one bounded segment at a time, returning
///   [`PumpSendDisposition::Backpressured`] instead of blocking or
///   growing memory when the send window is full;
/// - drain only the owning connection's delivered bytes (sibling
///   isolation);
/// - report terminal state for convergence;
/// - wake the destination delivery driver after admission.
///
/// The trait carries no `SamServiceState`, no command parser, and no
/// global context object. Socket ownership stays with the pump
/// caller (daemon-side).
pub trait StreamPumpEndpoint: Send + Sync {
    /// Returns the negotiated maximum Streaming payload size in
    /// bytes (at least 1).
    fn max_payload_bytes(&self) -> usize;
    /// Attempts to admit one bounded segment into the Streaming
    /// send window.
    fn try_send(&self, segment: &[u8]) -> Result<PumpSendDisposition, PumpEndpointError>;
    /// Drains only the owning connection's delivered application
    /// bytes, retaining sibling bytes in arrival order.
    fn drain_delivered(&self) -> Vec<Vec<u8>>;
    /// Returns `true` when the owning connection is terminal
    /// (closing-remote, closed, reset, or released).
    fn is_terminal(&self) -> bool;
    /// Wakes the per-destination delivery driver so admitted
    /// Streaming packets are routed without waiting for a poll.
    fn notify_outbound(&self);
}

/// Bounded pump configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PumpConfig {
    /// Maximum bytes per TCP read chunk (clamped to 1..=32768 by
    /// callers).
    pub max_chunk_bytes: usize,
    /// Read-side poll timeout that guarantees periodic drain
    /// progress even when no TCP byte arrives.
    pub read_timeout: Duration,
    /// Bounded park while backpressured (avoids a ready-loop while
    /// the peer's delayed ACK timer runs).
    pub backpressure_sleep: Duration,
}

impl PumpConfig {
    /// Returns the Plan 147-compatible defaults: 32 KiB chunk cap,
    /// 20 ms read timeout, 5 ms backpressure park.
    pub fn defaults(max_chunk_bytes: usize) -> Self {
        Self {
            max_chunk_bytes: max_chunk_bytes.clamp(1, 32 * 1024),
            read_timeout: Duration::from_millis(20),
            backpressure_sleep: Duration::from_millis(5),
        }
    }
}

/// Typed pump failure.
#[derive(Debug, thiserror::Error)]
pub enum PumpError {
    /// Local socket read or write failed.
    #[error("pump io: {0}")]
    Io(#[from] std::io::Error),
    /// The Streaming endpoint rejected application data.
    #[error("pump endpoint: {0}")]
    Endpoint(#[from] PumpEndpointError),
}

/// Runs one generic bounded socket <-> Streaming byte pump.
///
/// `socket` is owned for the duration of the pump; `initial_bytes`
/// carries same-read command+raw bytes preserved across the
/// command-to-raw handoff and is emitted first. `endpoint` is the
/// narrow capability for exactly one Streaming connection.
/// `cancellation` converges the pump on parent close, service
/// shutdown, or session teardown.
///
/// The pump terminates when the local socket reaches EOF or I/O
/// error, the endpoint reports terminal state, or cancellation
/// fires. It never retains a command parser reference to the socket.
pub async fn run_stream_pump<S>(
    mut socket: S,
    initial_bytes: Vec<u8>,
    endpoint: Arc<dyn StreamPumpEndpoint>,
    config: PumpConfig,
    cancellation: i2pr_runtime::CancellationToken,
) -> Result<(), PumpError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let max_chunk = config.max_chunk_bytes.clamp(1, 32 * 1024);
    let mut chunk = vec![0_u8; max_chunk];
    let mut carry: Vec<u8> = initial_bytes;
    let mut eof = false;
    let mut read_timeout = tokio::time::interval(config.read_timeout);
    read_timeout.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    read_timeout.tick().await;

    while !eof {
        // ----- Local -> Streaming: read bounded chunk and admit -----
        let mut timed_out = false;
        let mut backpressured = false;
        let read_size = if carry.is_empty() {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                _ = read_timeout.tick() => {
                    timed_out = true;
                    0
                }
                result = socket.read(&mut chunk) => match result {
                    Ok(0) => 0,
                    Ok(n) => n,
                    Err(error) => return Err(PumpError::Io(error)),
                },
            }
        } else {
            carry.len()
        };
        if timed_out {
            // No local byte, but Streaming->local data may be ready.
        } else if read_size == 0 && carry.is_empty() {
            eof = true;
        }
        if !eof && !timed_out {
            let payload: Vec<u8> = if carry.is_empty() {
                chunk[..read_size].to_vec()
            } else {
                std::mem::take(&mut carry)
            };
            let max_payload = endpoint.max_payload_bytes().max(1);
            let mut offset = 0_usize;
            while offset < payload.len() {
                let end = (offset + max_payload).min(payload.len());
                let segment = &payload[offset..end];
                match endpoint.try_send(segment) {
                    Ok(PumpSendDisposition::Accepted) => {
                        offset = end;
                        endpoint.notify_outbound();
                    }
                    Ok(PumpSendDisposition::Backpressured) => {
                        carry = payload[offset..].to_vec();
                        backpressured = true;
                        break;
                    }
                    Err(error) => return Err(PumpError::Endpoint(error)),
                }
            }
            // Give the runtime driver a chance to run delivery before
            // looping back to the socket read; otherwise a fast loop
            // can starve the driver task on a single-threaded runtime.
            tokio::task::yield_now().await;
        }

        // ----- Streaming -> Local: drain owned bytes, write socket -----
        // Sibling isolation lives in the endpoint: only the owning
        // connection's bytes are returned.
        for delivered in endpoint.drain_delivered() {
            if delivered.is_empty() {
                continue;
            }
            let write = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = socket.write_all(&delivered) => result,
            };
            if let Err(error) = write {
                return Err(PumpError::Io(error));
            }
            if let Err(error) = socket.flush().await {
                return Err(PumpError::Io(error));
            }
        }
        if endpoint.is_terminal() {
            eof = true;
        }
        if backpressured && !carry.is_empty() {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                _ = tokio::time::sleep(config.backpressure_sleep) => {}
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::Mutex;
    use tokio::io::duplex;

    /// Deterministic in-memory Streaming capability for pump tests.
    ///
    /// Models the two queues a real `StreamingManager` exposes per
    /// connection: `try_send` appends to `received` (local->I2P
    /// direction) subject to a bounded send window, and
    /// `drain_delivered` pops from `deliverable` (I2P->local
    /// direction) for exactly one connection id.
    struct MockEndpoint {
        state: Arc<Mutex<MockState>>,
        connection: u64,
        max_payload: usize,
    }

    struct MockState {
        received: HashMap<u64, Vec<u8>>,
        deliverable: HashMap<u64, VecDeque<Vec<u8>>>,
        terminal: HashSet<u64>,
        send_window_cap: usize,
        send_window_used: HashMap<u64, usize>,
        backpressure_once: HashSet<u64>,
        notify_count: usize,
    }

    impl MockState {
        fn new() -> Self {
            Self {
                received: HashMap::new(),
                deliverable: HashMap::new(),
                terminal: HashSet::new(),
                send_window_cap: usize::MAX,
                send_window_used: HashMap::new(),
                backpressure_once: HashSet::new(),
                notify_count: 0,
            }
        }
    }

    impl StreamPumpEndpoint for MockEndpoint {
        fn max_payload_bytes(&self) -> usize {
            self.max_payload
        }

        fn try_send(&self, segment: &[u8]) -> Result<PumpSendDisposition, PumpEndpointError> {
            let mut state = self.state.lock().expect("mock poisoned");
            if state.terminal.contains(&self.connection) {
                return Err(PumpEndpointError::InvalidState);
            }
            if state.backpressure_once.remove(&self.connection) {
                return Ok(PumpSendDisposition::Backpressured);
            }
            let used = state
                .send_window_used
                .get(&self.connection)
                .copied()
                .unwrap_or(0);
            if used.saturating_add(segment.len()) > state.send_window_cap {
                return Ok(PumpSendDisposition::Backpressured);
            }
            state
                .send_window_used
                .insert(self.connection, used.saturating_add(segment.len()));
            state
                .received
                .entry(self.connection)
                .or_default()
                .extend_from_slice(segment);
            Ok(PumpSendDisposition::Accepted)
        }

        fn drain_delivered(&self) -> Vec<Vec<u8>> {
            let mut state = self.state.lock().expect("mock poisoned");
            let mut out = Vec::new();
            if let Some(queue) = state.deliverable.get_mut(&self.connection) {
                while let Some(bytes) = queue.pop_front() {
                    out.push(bytes);
                }
            }
            out
        }

        fn is_terminal(&self) -> bool {
            self.state
                .lock()
                .expect("mock poisoned")
                .terminal
                .contains(&self.connection)
        }

        fn notify_outbound(&self) {
            self.state.lock().expect("mock poisoned").notify_count += 1;
        }
    }

    fn mock_pair(
        connection: u64,
        max_payload: usize,
    ) -> (Arc<Mutex<MockState>>, Arc<MockEndpoint>) {
        let state = Arc::new(Mutex::new(MockState::new()));
        let endpoint = Arc::new(MockEndpoint {
            state: Arc::clone(&state),
            connection,
            max_payload,
        });
        (state, endpoint)
    }

    #[tokio::test]
    async fn pump_moves_small_bidirectional_bytes_exact_once() {
        let (state, endpoint) = mock_pair(7, 1730);
        state
            .lock()
            .expect("mock")
            .deliverable
            .entry(7)
            .or_default()
            .push_back(b"pong".to_vec());

        let (mut local, pump_side) = duplex(64 * 1024);
        let cancel = i2pr_runtime::CancellationToken::new();
        let pump_cancel = cancel.clone();
        let pump = tokio::spawn(async move {
            run_stream_pump(
                pump_side,
                b"ping".to_vec(),
                endpoint,
                PumpConfig::defaults(32 * 1024),
                pump_cancel,
            )
            .await
            .expect("pump")
        });

        // I2P->local direction arrives first.
        let mut buf = [0_u8; 4];
        tokio::time::timeout(Duration::from_secs(5), local.read_exact(&mut buf))
            .await
            .expect("read timeout")
            .expect("read");
        assert_eq!(&buf, b"pong");
        // Local->I2P direction: the initial bytes plus one write.
        local.write_all(b"!").await.expect("write");
        // Give the pump a chance to admit, then cancel to converge.
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        tokio::time::timeout(Duration::from_secs(5), pump)
            .await
            .expect("join timeout")
            .expect("join");

        let received = state
            .lock()
            .expect("mock")
            .received
            .get(&7)
            .cloned()
            .unwrap_or_default();
        assert_eq!(received, b"ping!");
    }

    #[tokio::test]
    async fn pump_segments_multi_segment_payload() {
        let (state, endpoint) = mock_pair(9, 10);
        let (mut local, pump_side) = duplex(64 * 1024);
        let cancel = i2pr_runtime::CancellationToken::new();
        let pump_cancel = cancel.clone();
        let pump = tokio::spawn(async move {
            run_stream_pump(
                pump_side,
                Vec::new(),
                endpoint,
                PumpConfig::defaults(32 * 1024),
                pump_cancel,
            )
            .await
            .expect("pump")
        });

        let payload = vec![0xabu8; 100];
        local.write_all(&payload).await.expect("write");
        // Wait until all 100 bytes are admitted as ten 10-byte segments.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let len = state
                    .lock()
                    .expect("mock")
                    .received
                    .get(&9)
                    .map(Vec::len)
                    .unwrap_or(0);
                if len >= 100 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("admission timeout");
        cancel.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        tokio::time::timeout(Duration::from_secs(5), pump)
            .await
            .expect("join")
            .expect("join");
        let received = state
            .lock()
            .expect("mock")
            .received
            .get(&9)
            .cloned()
            .unwrap_or_default();
        assert_eq!(received, payload);
    }

    #[tokio::test]
    async fn pump_backpressure_with_stalled_reader_parks_and_recovers() {
        let (state, endpoint) = mock_pair(11, 1730);
        // First admission attempt backpressures once; the pump must
        // park the remainder and retry rather than drop or spin.
        state.lock().expect("mock").backpressure_once.insert(11);
        let (_local, pump_side) = duplex(64 * 1024);
        let cancel = i2pr_runtime::CancellationToken::new();
        let pump_cancel = cancel.clone();
        let pump = tokio::spawn(async move {
            run_stream_pump(
                pump_side,
                b"backpressure-payload".to_vec(),
                endpoint,
                PumpConfig::defaults(32 * 1024),
                pump_cancel,
            )
            .await
            .expect("pump")
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let len = state
                    .lock()
                    .expect("mock")
                    .received
                    .get(&11)
                    .map(Vec::len)
                    .unwrap_or(0);
                if len == b"backpressure-payload".len() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("backpressure recovery timeout");
        cancel.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        tokio::time::timeout(Duration::from_secs(5), pump)
            .await
            .expect("join")
            .expect("join");
    }

    #[tokio::test]
    async fn sibling_stream_a_cannot_drain_b() {
        let shared = Arc::new(Mutex::new(MockState::new()));
        shared
            .lock()
            .expect("mock")
            .deliverable
            .entry(1)
            .or_default()
            .push_back(b"for-a".to_vec());
        shared
            .lock()
            .expect("mock")
            .deliverable
            .entry(2)
            .or_default()
            .push_back(b"for-b".to_vec());
        let endpoint_a = Arc::new(MockEndpoint {
            state: Arc::clone(&shared),
            connection: 1,
            max_payload: 1730,
        });
        let endpoint_b = Arc::new(MockEndpoint {
            state: Arc::clone(&shared),
            connection: 2,
            max_payload: 1730,
        });

        let (mut local_a, pump_a_side) = duplex(64 * 1024);
        let (mut local_b, pump_b_side) = duplex(64 * 1024);
        let cancel = i2pr_runtime::CancellationToken::new();
        let (cancel_a, cancel_b) = (cancel.clone(), cancel.clone());
        let pump_a = tokio::spawn(async move {
            run_stream_pump(
                pump_a_side,
                Vec::new(),
                endpoint_a,
                PumpConfig::defaults(32 * 1024),
                cancel_a,
            )
            .await
            .expect("pump a")
        });
        let pump_b = tokio::spawn(async move {
            run_stream_pump(
                pump_b_side,
                Vec::new(),
                endpoint_b,
                PumpConfig::defaults(32 * 1024),
                cancel_b,
            )
            .await
            .expect("pump b")
        });

        let mut buf_a = [0_u8; 5];
        let mut buf_b = [0_u8; 5];
        tokio::time::timeout(Duration::from_secs(5), local_a.read_exact(&mut buf_a))
            .await
            .expect("a timeout")
            .expect("a read");
        tokio::time::timeout(Duration::from_secs(5), local_b.read_exact(&mut buf_b))
            .await
            .expect("b timeout")
            .expect("b read");
        assert_eq!(&buf_a, b"for-a");
        assert_eq!(&buf_b, b"for-b");
        cancel.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        tokio::time::timeout(Duration::from_secs(5), pump_a)
            .await
            .expect("join a")
            .expect("join a");
        tokio::time::timeout(Duration::from_secs(5), pump_b)
            .await
            .expect("join b")
            .expect("join b");
    }

    #[tokio::test]
    async fn cancellation_and_half_close_release_resources() {
        let (_state, endpoint) = mock_pair(13, 1730);
        let (local, pump_side) = duplex(64 * 1024);
        let cancel = i2pr_runtime::CancellationToken::new();
        let pump_cancel = cancel.clone();
        let pump = tokio::spawn(async move {
            run_stream_pump(
                pump_side,
                Vec::new(),
                endpoint,
                PumpConfig::defaults(1024),
                pump_cancel,
            )
            .await
            .expect("pump")
        });
        // Half-close: drop the local write side; the pump must
        // observe EOF and converge without hanging.
        drop(local);
        tokio::time::timeout(Duration::from_secs(5), pump)
            .await
            .expect("half-close must converge")
            .expect("join");

        // Cancellation converges a parked pump.
        let (_state2, endpoint2) = mock_pair(14, 1730);
        let (_local2, pump_side2) = duplex(64 * 1024);
        let cancel2 = i2pr_runtime::CancellationToken::new();
        let pump_cancel2 = cancel2.clone();
        let pump2 = tokio::spawn(async move {
            run_stream_pump(
                pump_side2,
                Vec::new(),
                endpoint2,
                PumpConfig::defaults(1024),
                pump_cancel2,
            )
            .await
            .expect("pump2")
        });
        cancel2.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        tokio::time::timeout(Duration::from_secs(5), pump2)
            .await
            .expect("cancel must converge")
            .expect("join");
    }
}
