//! Plan 179 Milestone 10 IRC `.i2p` server tunnel profile executor.
//!
//! The Plan 179 IRC server tunnel is the fourth M10 application
//! profile. The runtime-neutral registration interceptor, USER
//! hostname rewrite, and bounded pre-registration policy live in
//! `i2pr-service-tunnels::irc::server`; this module owns the
//! socket/Streaming lifetime and the per-connection handoff to a
//! local loopback IRC target.
//!
//! ```text
//! remote I2P IRC client
//!   -> i2pr persistent IRC server Destination / Streaming accept
//!   -> bounded registration interceptor
//!   -> authenticated peer Destination -> safe IRC hostname projection
//!   -> loopback IRC server target
//!   -> raw bounded byte pump after registration
//! ```
//!
//! The tunnel is loopback-only on the target side, disabled by
//! default, and uses the existing Plan 175 persistent server
//! destination plus the Plan 174 shared socket <-> Streaming pump.
//! No new Garlic/I2NP/Streaming implementation is introduced.
//!
//! The full I2P Streaming byte round-trip over local TCP for the
//! IRC server profile is owned by the Plan 180 reconcile pass,
//! which generalizes the per-destination runtime driver to service
//! tunnels. Plan 179 ships the bounded executor skeleton; the
//! black-box matrix in `service_tunnel_irc_server_product.rs`
//! exercises every behavior that does not require the full
//! per-destination driver loop.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use i2pr_client::streaming::connection::ConnectionId;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::IrcLimits;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::destination_streaming::{PumpConfig, StreamPumpEndpoint, run_stream_pump};
use crate::service_tunnels::{
    ServicePumpEndpoint, ServiceRuntime, ServiceTunnelManager, service_streaming_now_ms,
};

// Re-exports for the integration test seam. The runtime-neutral
// types already live in `i2pr_service_tunnels`; this re-export
// keeps integration tests sourcing every Plan 179 type from
// `i2pr_daemon::service_tunnels_irc_server`.
pub use i2pr_service_tunnels::irc::server::{
    IrcServerOptions, IrcServerRegistration, RegistrationOutcome, RegistrationRejection,
    RegistrationState, project_peer_hostname,
};

/// Total registration deadline. Plan 179 §5 requires a typed
/// ceiling; 30 s is the documented M10 default.
pub const IRC_SERVER_REGISTRATION_DEADLINE: Duration = Duration::from_secs(30);
/// Per-connection read poll cadence while waiting for more I2P
/// bytes during registration.
pub const IRC_SERVER_READ_POLL_INTERVAL: Duration = Duration::from_millis(20);
/// Connect deadline for the loopback IRC target.
pub const IRC_SERVER_TARGET_CONNECT_DEADLINE: Duration = Duration::from_secs(10);

/// Outcome of one IRC server connection attempt (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrcServerConnectionOutcome {
    /// Registration prefix was written, the loopback target
    /// received the rewritten bytes, and the raw pump completed
    /// (peer closed or local cancel).
    TunnelClosed,
    /// Registration was rejected by the runtime-neutral
    /// interceptor (typed reason available through snapshot).
    RegistrationRejected,
    /// The remote peer closed the Streaming connection before
    /// USER was observed.
    PeerClosed,
    /// The configured loopback target refused/timeout out the
    /// connect; the prefix was never written to the target.
    TargetUnavailable,
    /// A read or connect deadline expired.
    TimedOut,
}

/// Bounded outcome of the registration interception phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterceptionResult {
    /// Interception succeeded with the rewritten prefix and
    /// any same-read leftover bytes that follow USER.
    Ready {
        /// Registration prefix to write to the target exactly once.
        prefix: Vec<u8>,
        /// Bytes that follow the terminating USER line in the
        /// same read; preserved verbatim as the first raw-pump
        /// bytes.
        leftover: Vec<u8>,
    },
    /// Registration was rejected.
    Rejected(RegistrationRejection),
    /// The remote peer closed before USER was observed.
    PeerClosed,
}

/// Bounded delivery-queue abstraction for the registration
/// interceptor. Production uses the streaming manager's
/// `drain_delivered_for`; tests can use a buffered channel.
pub trait InterceptionSource: Send {
    /// Drains every currently available application byte chunk for
    /// the active streaming connection, in arrival order. Each
    /// `Vec<u8>` is a single delivery unit.
    fn drain(&mut self) -> Vec<Vec<u8>>;
    /// Returns `true` when the active streaming connection is in
    /// a terminal state (closing-remote, closed, reset, or
    /// released).
    fn is_terminal(&self) -> bool;
}

/// Production interception source backed by the
/// [`ServiceTunnelManager`] receiver-mirror streaming queue.
pub struct StreamingInterceptionSource<'a> {
    manager: &'a ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
}

impl<'a> StreamingInterceptionSource<'a> {
    /// Creates a new production source.
    pub fn new(
        manager: &'a ServiceTunnelManager,
        destination_id: i2pr_client::DestinationId,
        connection_id: ConnectionId,
    ) -> Self {
        Self {
            manager,
            destination_id,
            connection_id,
        }
    }
}

impl<'a> InterceptionSource for StreamingInterceptionSource<'a> {
    fn drain(&mut self) -> Vec<Vec<u8>> {
        self.manager
            .with_destination_bridge(self.destination_id, |bridge| {
                bridge
                    .receiver_streaming_mut()
                    .drain_delivered_for(self.connection_id)
                    .into_iter()
                    .map(|entry| entry.bytes)
                    .filter(|bytes| !bytes.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_terminal(&self) -> bool {
        self.manager
            .with_destination_bridge(self.destination_id, |bridge| {
                bridge
                    .receiver_streaming()
                    .get_connection(self.connection_id)
                    .is_none_or(|conn| {
                        use i2pr_client::streaming::connection::ConnectionState;
                        matches!(
                            conn.state(),
                            ConnectionState::ClosingRemote
                                | ConnectionState::Closed
                                | ConnectionState::Reset
                        )
                    })
            })
            .unwrap_or(true)
    }
}

/// Test interception source backed by a bounded buffered channel.
///
/// This is a development-test seam used by the
/// `service_tunnel_irc_server_product` integration tests; it
/// mirrors the production `StreamingInterceptionSource` API but
/// is fed by an in-process channel. Production code must use
/// [`StreamingInterceptionSource`] instead.
pub struct ChannelInterceptionSource {
    receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
    closed: bool,
}

/// Bounded capacity for the test interception source channel. The
/// production path uses the Plan 149 streaming manager queue, so
/// this value only has to be large enough to drive the
/// `service_tunnel_irc_server_product` tests without backpressuring
/// the producer.
const CHANNEL_INTERCEPTION_SOURCE_CAPACITY: usize = 64;

impl ChannelInterceptionSource {
    /// Creates a bounded test source plus its producer handle.
    pub fn bounded() -> (Self, tokio::sync::mpsc::Sender<Vec<u8>>) {
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_INTERCEPTION_SOURCE_CAPACITY);
        (
            Self {
                receiver: rx,
                closed: false,
            },
            tx,
        )
    }

    /// Marks the source as closed. After this call, the
    /// interceptor treats the source as terminal and runs the
    /// EOF path. Mirrors the production path where the
    /// streaming connection enters `Closed`/`Reset` state.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

impl InterceptionSource for ChannelInterceptionSource {
    fn drain(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Ok(chunk) = self.receiver.try_recv() {
            out.push(chunk);
        }
        out
    }

    fn is_terminal(&self) -> bool {
        // When the producer drops its handle, `try_recv` returns
        // `Disconnected` and the channel reports `is_empty`; treat
        // either as a terminal signal for the registration
        // interceptor. We also honour the explicit `close` flag
        // so a test can mark a still-open source as terminal.
        (self.closed || self.receiver.is_closed()) && self.receiver.is_empty()
    }
}

/// Runs the bounded registration interception phase against the
/// supplied source. Returns the typed [`InterceptionResult`].
///
/// The function never blocks on socket I/O; the
/// [`IRC_SERVER_REGISTRATION_DEADLINE`] bounds the total scan,
/// and the per-iteration poll cadence is
/// [`IRC_SERVER_READ_POLL_INTERVAL`].
pub async fn intercept_registration<S>(
    source: &mut S,
    peer_destination_hash: [u8; 32],
    options: IrcServerOptions,
    cancellation: &CancellationToken,
) -> InterceptionResult
where
    S: InterceptionSource,
{
    let limits = IrcLimits::defaults();
    let mut registration = IrcServerRegistration::new(options, limits, peer_destination_hash);
    let started = Instant::now();
    let mut carry: Vec<u8> = Vec::new();
    loop {
        if started.elapsed() >= IRC_SERVER_REGISTRATION_DEADLINE {
            return InterceptionResult::Rejected(RegistrationRejection::BufferOverflow);
        }
        let delivered = source.drain();
        if !delivered.is_empty() {
            for chunk in delivered {
                carry.extend_from_slice(&chunk);
            }
            let outcome = registration.advance(&carry);
            carry.clear();
            match outcome {
                RegistrationOutcome::Incomplete { retained } => {
                    carry = retained;
                }
                RegistrationOutcome::Ready { prefix, leftover } => {
                    return InterceptionResult::Ready { prefix, leftover };
                }
                RegistrationOutcome::Rejected(reason) => {
                    return InterceptionResult::Rejected(reason);
                }
                RegistrationOutcome::Eof => {
                    return InterceptionResult::PeerClosed;
                }
            }
            continue;
        }
        if source.is_terminal() {
            let outcome = registration.finish_eof();
            return match outcome {
                RegistrationOutcome::Eof => InterceptionResult::PeerClosed,
                RegistrationOutcome::Rejected(reason) => InterceptionResult::Rejected(reason),
                _ => InterceptionResult::PeerClosed,
            };
        }
        // Wait briefly for more delivered bytes.
        let poll_interval = IRC_SERVER_READ_POLL_INTERVAL
            .min(IRC_SERVER_REGISTRATION_DEADLINE.saturating_sub(started.elapsed()));
        if poll_interval.is_zero() {
            continue;
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return InterceptionResult::PeerClosed,
            _ = tokio::time::sleep(poll_interval) => {}
        }
    }
}

/// Writes the registration prefix (plus any same-read leftover)
/// to the local loopback target exactly once. Returns the local
/// target stream on success.
async fn write_prefix_to_target(
    target: SocketAddr,
    prefix: &[u8],
    leftover: &[u8],
) -> Result<TcpStream, IrcServerConnectionOutcome> {
    let mut target_stream = match timeout(
        IRC_SERVER_TARGET_CONNECT_DEADLINE,
        TcpStream::connect(target),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        _ => return Err(IrcServerConnectionOutcome::TargetUnavailable),
    };
    if !prefix.is_empty()
        && let Err(_error) = target_stream.write_all(prefix).await
    {
        return Err(IrcServerConnectionOutcome::TargetUnavailable);
    }
    if !leftover.is_empty()
        && let Err(_error) = target_stream.write_all(leftover).await
    {
        return Err(IrcServerConnectionOutcome::TargetUnavailable);
    }
    if let Err(_error) = target_stream.flush().await {
        return Err(IrcServerConnectionOutcome::TargetUnavailable);
    }
    Ok(target_stream)
}

/// Drives the raw byte pump from the local target through the
/// authenticated Streaming connection. Returns the typed
/// [`IrcServerConnectionOutcome`].
async fn run_irc_raw_pump(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    target_stream: TcpStream,
    connection_id: ConnectionId,
    cancellation: CancellationToken,
) -> IrcServerConnectionOutcome {
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_server(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
    ));
    let config = PumpConfig::defaults(32 * 1024);
    let result = run_stream_pump(target_stream, Vec::new(), endpoint, config, cancellation).await;
    let graceful = result.is_ok();
    if let Err(ref error) = result {
        debug!(error = %error, "irc server raw pump failed");
    }
    // Always release the receiver-side connection entry so the
    // per-connection accounting is decremented and the
    // streaming manager does not retain a phantom entry.
    terminate_irc_streaming(&manager, runtime.destination_id, connection_id, graceful);
    IrcServerConnectionOutcome::TunnelClosed
}

fn terminate_irc_streaming(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    _graceful: bool,
) {
    let _ = now_unused();
    manager.with_destination_bridge(destination_id, |bridge| {
        let _ = bridge
            .receiver_streaming_mut()
            .remove_connection(connection_id);
    });
}

#[inline]
fn now_unused() -> u64 {
    service_streaming_now_ms()
}

/// Per-connection IRC server entry. The full streaming handoff is
/// owned by the supervisor loop (`run_irc_server_loop`); this
/// function owns the post-accept registration interception,
/// target connect, and raw pump phases.
pub async fn run_irc_server_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    target: SocketAddr,
    connection_id: ConnectionId,
    peer_destination_hash: [u8; 32],
    options: IrcServerOptions,
    cancellation: CancellationToken,
) -> IrcServerConnectionOutcome {
    let mut source =
        StreamingInterceptionSource::new(&manager, runtime.destination_id, connection_id);
    let interception =
        intercept_registration(&mut source, peer_destination_hash, options, &cancellation).await;
    match interception {
        InterceptionResult::PeerClosed => IrcServerConnectionOutcome::PeerClosed,
        InterceptionResult::Rejected(_reason) => IrcServerConnectionOutcome::RegistrationRejected,
        InterceptionResult::Ready { prefix, leftover } => {
            let target_stream = match write_prefix_to_target(target, &prefix, &leftover).await {
                Ok(stream) => stream,
                Err(outcome) => return outcome,
            };
            run_irc_raw_pump(manager, runtime, target_stream, connection_id, cancellation).await
        }
    }
}

/// Per-service supervisor loop for an IRC server tunnel. Drives
/// the Plan 175 server accept loop, dispatches each accepted
/// connection to a per-connection task, and applies the per-listener
/// permit / accounting budget.
pub async fn run_irc_server_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    _spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let target_socket = manager.server_target_for(runtime).ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "irc server tunnel missing loopback target".to_owned(),
        )
    })?;
    info_irc_server_loop_started(runtime, target_socket);
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let port = manager.server_streaming_port_for(runtime).unwrap_or(1_u16);
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            _ = ticker.tick() => {}
        }
        let mut accepted_ids = Vec::new();
        manager.with_destination_bridge(runtime.destination_id, |bridge| {
            while let Some(connection_id) = bridge.receiver_streaming_mut().accept(port) {
                accepted_ids.push(connection_id);
            }
        });
        for connection_id in accepted_ids {
            spawn_irc_server_connection(
                Arc::clone(manager),
                Arc::clone(runtime),
                target_socket,
                connection_id,
                cancellation.clone(),
            )
            .await;
        }
    }
    Ok(())
}

fn info_irc_server_loop_started(runtime: &ServiceRuntime, target: std::net::SocketAddr) {
    tracing::info!(
        service = %runtime.spec_id,
        target = %target,
        "irc server tunnel bound streaming listener"
    );
}

async fn spawn_irc_server_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    target: std::net::SocketAddr,
    connection_id: ConnectionId,
    cancellation: CancellationToken,
) {
    let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
        manager.aggregate_permit().try_acquire_owned().ok();
    let Some(aggregate_permit) = aggregate_permit else {
        warn!(
            service = %runtime.spec_id,
            "irc server tunnel aggregate ceiling reached; rejecting inbound SYN"
        );
        runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
        return;
    };
    runtime.active_connections.fetch_add(1, Ordering::Relaxed);
    let options = IrcServerOptions::default();
    let manager_for_task = Arc::clone(&manager);
    let runtime_for_task = Arc::clone(&runtime);
    let cancellation_for_task = cancellation.clone();
    let spec_id_for_log = runtime.spec_id.clone();
    // The semaphore permit is moved into the spawned task so the
    // connection slot is released on task exit. Holding the
    // permit here would be a deadlock against the supervisor's
    // permit-acquire loop.
    let _aggregate_permit = aggregate_permit;
    tokio::spawn(async move {
        let outcome = run_irc_server_connection_established(
            manager_for_task.clone(),
            runtime_for_task.clone(),
            target,
            connection_id,
            options,
            cancellation_for_task,
        )
        .await;
        if matches!(
            outcome,
            IrcServerConnectionOutcome::RegistrationRejected
                | IrcServerConnectionOutcome::TargetUnavailable
                | IrcServerConnectionOutcome::TimedOut
        ) {
            runtime_for_task
                .failed_connects
                .fetch_add(1, Ordering::Relaxed);
        }
        runtime_for_task
            .active_connections
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            })
            .ok();
        debug!(
            service = %spec_id_for_log,
            ?outcome,
            "irc server connection finished"
        );
        // The permit is dropped here, releasing the slot.
        drop(_aggregate_permit);
    });
}

/// Waits for the inbound streaming connection to reach the
/// `Established` state under a bounded deadline, then invokes the
/// per-connection registration + target handoff + raw pump.
async fn run_irc_server_connection_established(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    target: std::net::SocketAddr,
    connection_id: ConnectionId,
    options: IrcServerOptions,
    cancellation: CancellationToken,
) -> IrcServerConnectionOutcome {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut peer_hash: Option<[u8; 32]> = None;
    while Instant::now() < deadline {
        if cancellation.is_cancelled() {
            return IrcServerConnectionOutcome::PeerClosed;
        }
        let snapshot: Option<
            Option<(
                [u8; 32],
                i2pr_client::streaming::connection::ConnectionState,
            )>,
        > = manager.with_destination_bridge(runtime.destination_id, move |bridge| {
            let conn = bridge.receiver_streaming().get_connection(connection_id);
            conn.map(|c| (*c.peer_destination_hash(), c.state()))
        });
        let Some(Some((hash, state))) = snapshot else {
            return IrcServerConnectionOutcome::PeerClosed;
        };
        {
            use i2pr_client::streaming::connection::ConnectionState;
            if matches!(state, ConnectionState::Closed | ConnectionState::Reset) {
                return IrcServerConnectionOutcome::PeerClosed;
            }
            // The peer destination hash is available from the
            // moment the SYN is accepted (it is part of the
            // authenticated I2P metadata). Plan 179 §3 requires
            // the hash be the only acceptable source for the
            // projected hostname, so we capture it here even
            // before the connection reaches Established.
            peer_hash = Some(hash);
            if matches!(state, ConnectionState::Established) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let Some(peer_hash) = peer_hash else {
        return IrcServerConnectionOutcome::PeerClosed;
    };
    run_irc_server_connection(
        manager,
        runtime,
        target,
        connection_id,
        peer_hash,
        options,
        cancellation,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_are_bounded() {
        assert!(IRC_SERVER_REGISTRATION_DEADLINE >= Duration::from_secs(5));
        assert!(IRC_SERVER_TARGET_CONNECT_DEADLINE >= Duration::from_secs(1));
        assert!(IRC_SERVER_READ_POLL_INTERVAL >= Duration::from_millis(1));
    }
}

// Re-exports for the integration test seam. The runtime-neutral
// types already live in `i2pr_service_tunnels`; this re-export
// keeps integration tests sourcing every Plan 179 type from
// `i2pr_daemon::service_tunnels_irc_server`.
