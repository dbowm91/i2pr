//! Plan 177 Milestone 10 SOCKS5 `.i2p` CONNECT proxy executor.
//!
//! The Plan 177 SOCKS5 client proxy is the second M10 application
//! profile that owns a bounded protocol parser. The runtime-neutral
//! policy (parser bounds, negotiation, request validation, reply
//! generation) lives in `i2pr-service-tunnels::socks5`; this module
//! owns the socket and task orchestration.
//!
//! ```text
//! loopback TCP accept
//!   -> bounded greeting read (deadline)
//!   -> runtime-neutral greeting negotiation
//!   -> bounded CONNECT request read (deadline)
//!   -> runtime-neutral request parser + target validator
//!   -> Manager destination resolution (Base32 / alias / local)
//!   -> I2P Streaming connect (CSPRNG, OS)
//!   -> on Streaming Established: send SOCKS5 success reply
//!   -> run shared socket <-> Streaming pump in opaque tunnel mode
//! ```
//!
//! The proxy is loopback-only, disabled by default, and uses the
//! existing Plan 149 local destination product path plus the Plan
//! 174 shared socket <-> Streaming pump. No new Garlic/I2NP/Streaming
//! implementation is introduced.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination,
};
use i2pr_crypto::OsRng;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::{
    ConnectDestination, GreetingOutcome, GreetingParser, RequestOutcome, RequestParser,
    Socks5ClientOptions, Socks5Error, Socks5ErrorKind, Socks5Limits, build_socks5_reply,
    build_socks5_reply_from_code,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::destination_streaming::{PumpConfig, StreamPumpEndpoint, run_stream_pump};
use crate::service_tunnels::{
    ClientTarget, ServicePumpEndpoint, ServiceRuntime, ServiceTunnelManager,
    service_streaming_now_ms,
};

/// Default ceiling for reading the SOCKS5 greeting section.
const GREETING_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Default ceiling for reading the SOCKS5 CONNECT request section.
const REQUEST_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Default read chunk size for the SOCKS5 byte pump.
const BODY_CHUNK_BYTES: usize = 32 * 1024;

/// Outcome of one SOCKS5 connection attempt (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Socks5ConnectionOutcome {
    /// CONNECT completed and the opaque byte pump ran to EOF.
    TunnelClosed,
    /// Greeting was malformed or unsupported and the socket was
    /// closed after the bounded reply was flushed.
    BadGreeting,
    /// CONNECT request was malformed or unsupported and the
    /// bounded SOCKS5 reply was emitted before close.
    BadRequest,
    /// CONNECT target was rejected by the proxy policy (clearnet,
    /// local, IP, mixed-suffix, etc).
    Forbidden,
    /// The configured destination was unknown and no I2P connect
    /// was attempted.
    BadGateway,
    /// The header read or connect deadline expired.
    TimedOut,
}

/// Reads the complete SOCKS5 greeting section from the supplied
/// stream into a bounded buffer, dispatching into the runtime-
/// neutral greeting parser. Returns the bytes after the greeting
/// (which belong to the request section) on success.
async fn read_greeting<S>(
    stream: &mut S,
    limits: Socks5Limits,
    deadline: Duration,
) -> Result<(GreetingOutcome, Vec<u8>), Socks5Error>
where
    S: AsyncRead + Unpin,
{
    let mut parser = GreetingParser::new();
    let mut chunk = [0_u8; 256];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= deadline {
            return Err(Socks5Error::new(
                Socks5ErrorKind::GreetingCeiling,
                "greeting read deadline exceeded",
            ));
        }
        let remaining = deadline.saturating_sub(elapsed);
        let read = match timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::GreetingCeiling,
                    "EOF before greeting section terminator",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(_error)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::GreetingCeiling,
                    "greeting read io error",
                ));
            }
            Err(_) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::GreetingCeiling,
                    "greeting read deadline exceeded",
                ));
            }
        };
        match parser.advance(&chunk[..read], limits)? {
            Some((outcome, consumed)) => {
                // Bytes past the greeting terminator belong to the
                // CONNECT request; forward them as the initial
                // request buffer. After truncation the parser
                // retained only the greeting bytes, so `consumed`
                // is the offset into the chunk where the request
                // section begins.
                let remainder = if consumed < read {
                    chunk[consumed..read].to_vec()
                } else {
                    Vec::new()
                };
                return Ok((outcome, remainder));
            }
            None => continue,
        }
    }
}

/// Reads the complete SOCKS5 CONNECT request section from the
/// supplied stream into a bounded buffer, dispatching into the
/// runtime-neutral request parser. Returns the parsed
/// [`RequestOutcome`] on success.
async fn read_request<S>(
    stream: &mut S,
    initial: Vec<u8>,
    limits: Socks5Limits,
    deadline: Duration,
) -> Result<RequestOutcome, Socks5Error>
where
    S: AsyncRead + Unpin,
{
    let mut parser = RequestParser::new();
    if !initial.is_empty()
        && let Some(outcome) = parser.advance(&initial, limits)?
    {
        return Ok(outcome);
    }
    let mut chunk = [0_u8; 1024];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= deadline {
            return Err(Socks5Error::new(
                Socks5ErrorKind::BufferCeilingExceeded,
                "request read deadline exceeded",
            ));
        }
        let remaining = deadline.saturating_sub(elapsed);
        let read = match timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "EOF before request section terminator",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(_error)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "request read io error",
                ));
            }
            Err(_) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "request read deadline exceeded",
                ));
            }
        };
        match parser.advance(&chunk[..read], limits)? {
            Some(outcome) => return Ok(outcome),
            None => continue,
        }
    }
}

/// Writes a bounded SOCKS5 reply and (optionally) shuts down the
/// write half of the socket.
async fn write_reply<S>(stream: &mut S, reply: [u8; 10]) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    if let Err(error) = stream.write_all(&reply).await {
        debug!(error = %error, "socks5 reply write failed");
    }
    Ok(())
}

/// Resolves a parsed CONNECT destination to a [`ClientTarget`]. The
/// proxy only forwards requests whose host matches the configured
/// service destination (Base32) or an entry in the alias table.
fn resolve_target_for_service(
    manager: &ServiceTunnelManager,
    destination: &ConnectDestination,
) -> Result<ClientTarget, Socks5Error> {
    let reference =
        i2pr_service_tunnels::DestinationRef::parse(&destination.host).map_err(|_| {
            Socks5Error::new(
                Socks5ErrorKind::NonI2pTarget,
                "destination not resolvable by SOCKS5 proxy",
            )
        })?;
    manager.resolve_reference(&reference).map_err(|error| {
        let _ = error;
        Socks5Error::new(
            Socks5ErrorKind::NonI2pTarget,
            "destination not resolvable by SOCKS5 proxy",
        )
    })
}

/// Opens a Streaming connection to the supplied remote destination
/// and waits until `Established` (or returns a typed error).
async fn open_streaming(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    remote: &RemoteDestination,
    timeout_ms: u64,
) -> Result<ConnectionId, Socks5Error> {
    let identity_arc = manager
        .with_destination_bridge(destination_id, |bridge| bridge.identity())
        .ok_or_else(|| Socks5Error::new(Socks5ErrorKind::ConnectFailure, "missing identity"))?;
    let connect_outcome = {
        let mut os_rng = OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        manager.with_destination_bridge(destination_id, |bridge| {
            bridge.streaming_mut().connect(
                identity_arc.as_ref(),
                remote,
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                service_streaming_now_ms(),
                &mut rng,
            )
        })
    };
    let connect_outcome = match connect_outcome {
        Some(value) => value,
        None => {
            return Err(Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "client connect produced no outcome",
            ));
        }
    };
    let connection_id = match connect_outcome {
        Ok(ConnectOutcome::SynSent { connection_id, .. }) => connection_id,
        Ok(_) => {
            return Err(Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "client connect produced no SYN",
            ));
        }
        Err(_error) => {
            return Err(Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "client connect error",
            ));
        }
    };
    wait_for_established(manager, destination_id, connection_id, timeout_ms)
        .await
        .map_err(|_| Socks5Error::new(Socks5ErrorKind::ConnectFailure, "deadline reached"))?;
    Ok(connection_id)
}

async fn wait_for_established(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    timeout_ms: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let established = manager.with_destination_bridge(destination_id, |bridge| {
            bridge
                .streaming()
                .get_connection(connection_id)
                .is_some_and(|conn| matches!(conn.state(), ConnectionState::Established))
        });
        if established.unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Box::new(std::io::Error::other("deadline reached")));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Closes the Streaming connection after the per-connection pump
/// exits. Emits a CLOSE packet on success and a RESET on error.
fn terminate_streaming(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    remote: &RemoteDestination,
    reset: bool,
) {
    let now_ms = service_streaming_now_ms();
    let identity_arc = manager.with_destination_bridge(destination_id, |bridge| bridge.identity());
    let Some(identity_arc) = identity_arc else {
        return;
    };
    manager.with_destination_bridge(destination_id, |bridge| {
        let conn = bridge.streaming().get_connection(connection_id);
        let Some(conn) = conn else {
            return;
        };
        let local_port = conn.local_port();
        let remote_port = conn.remote_port();
        let request = if reset {
            bridge.streaming_mut().send_reset(
                connection_id,
                identity_arc.as_ref(),
                remote,
                local_port,
                remote_port,
                now_ms,
            )
        } else {
            bridge.streaming_mut().send_close(
                connection_id,
                identity_arc.as_ref(),
                remote,
                local_port,
                remote_port,
                now_ms,
            )
        };
        let _ = request;
    });
    manager.with_destination_bridge(destination_id, |bridge| {
        let _ = bridge.streaming_mut().remove_connection(connection_id);
    });
}

/// Per-connection SOCKS5 proxy entry. Reads greeting, negotiates
/// no-auth, parses CONNECT, opens Streaming, sends success, runs
/// the shared Plan 174 byte pump.
pub async fn run_socks5_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    stream: TcpStream,
    cancellation: CancellationToken,
    options: Socks5ClientOptions,
) -> Socks5ConnectionOutcome {
    let limits = Socks5Limits::defaults();
    let mut stream = stream;
    // Step 1: greeting read + negotiation.
    let (greeting_outcome, request_initial) =
        match read_greeting(&mut stream, limits, GREETING_READ_DEADLINE).await {
            Ok(value) => value,
            Err(_error) => {
                let _ = stream.shutdown().await;
                return Socks5ConnectionOutcome::BadGreeting;
            }
        };
    let _ = greeting_outcome; // Currently only NoAuthentication is reachable.
    let reply = match greeting_outcome {
        GreetingOutcome::NoAuthentication => GreetingParser::no_auth_reply(),
        GreetingOutcome::NoAcceptableMethod => {
            let no_accept = GreetingParser::no_acceptable_method_reply();
            if let Err(error) = stream.write_all(&no_accept).await {
                debug!(error = %error, "no-acceptable-method write failed");
            }
            let _ = stream.shutdown().await;
            return Socks5ConnectionOutcome::BadGreeting;
        }
    };
    if let Err(error) = stream.write_all(&reply).await {
        debug!(error = %error, "greeting reply write failed");
        let _ = stream.shutdown().await;
        return Socks5ConnectionOutcome::BadGreeting;
    }
    // Step 2: CONNECT request read + parse.
    let request_outcome =
        match read_request(&mut stream, request_initial, limits, REQUEST_READ_DEADLINE).await {
            Ok(value) => value,
            Err(_error) => {
                let _ = stream.shutdown().await;
                return Socks5ConnectionOutcome::BadRequest;
            }
        };
    let (destination, leftover) = match request_outcome {
        RequestOutcome::ReadyToConnect {
            destination,
            leftover,
        } => (destination, leftover),
        RequestOutcome::Rejected { reply_code } => {
            let reply = build_socks5_reply_from_code(reply_code);
            let _ = write_reply(&mut stream, reply).await;
            let _ = stream.shutdown().await;
            return Socks5ConnectionOutcome::Forbidden;
        }
    };
    if !options.port_policy.allows_connect_port(destination.port) {
        let reply = build_socks5_reply(i2pr_service_tunnels::Socks5ReplyCode::ConnectionNotAllowed);
        let _ = write_reply(&mut stream, reply).await;
        let _ = stream.shutdown().await;
        return Socks5ConnectionOutcome::Forbidden;
    }
    let target = match resolve_target_for_service(&manager, &destination) {
        Ok(value) => value,
        Err(_error) => {
            let reply = build_socks5_reply(i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable);
            let _ = write_reply(&mut stream, reply).await;
            let _ = stream.shutdown().await;
            return Socks5ConnectionOutcome::BadGateway;
        }
    };
    let connect_timeout_ms = lookup_connect_timeout(&manager, &runtime.spec_id);
    let connection_id = match open_streaming(
        &manager,
        runtime.destination_id,
        &target.remote,
        connect_timeout_ms,
    )
    .await
    {
        Ok(id) => id,
        Err(error) => {
            let code = error.reply_code();
            let reply = build_socks5_reply(code);
            let _ = write_reply(&mut stream, reply).await;
            let _ = stream.shutdown().await;
            return Socks5ConnectionOutcome::BadGateway;
        }
    };
    let success = build_socks5_reply(i2pr_service_tunnels::Socks5ReplyCode::Success);
    if let Err(error) = stream.write_all(&success).await {
        debug!(error = %error, "socks5 success reply write failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadRequest;
    }
    if let Err(error) = stream.flush().await {
        debug!(error = %error, "socks5 success reply flush failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadRequest;
    }
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_client(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
        target.remote.clone(),
    ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, leftover, endpoint, config, cancellation).await;
    if let Err(error) = result {
        debug!(error = %error, "socks5 pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadGateway;
    }
    terminate_streaming(
        &manager,
        runtime.destination_id,
        connection_id,
        &target.remote,
        false,
    );
    Socks5ConnectionOutcome::TunnelClosed
}

fn lookup_connect_timeout(manager: &ServiceTunnelManager, spec_id: &str) -> u64 {
    manager
        .config()
        .specs
        .tunnels
        .iter()
        .find(|spec| spec.id.as_str() == spec_id)
        .map(|spec| spec.timeouts.connect_timeout_ms)
        .unwrap_or(10_000)
}

/// Runs the per-service supervisor loop for a SOCKS5 client tunnel.
pub async fn run_socks5_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "socks5 client tunnel missing loopback listener".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "socks5 client tunnel bound loopback listener"
    );
    let options = spec.socks5_options.clone().unwrap_or_default();
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            accept = listener.accept() => accept,
        };
        let (stream, _peer) = match accept {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    service = %spec.id.as_str(),
                    error = %error,
                    "socks5 client listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
            manager.aggregate_permit().try_acquire_owned().ok();
        let Some(_aggregate_permit) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "socks5 client tunnel aggregate ceiling reached; rejecting connection"
            );
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            drop(stream);
            continue;
        };
        runtime.active_connections.fetch_add(1, Ordering::Relaxed);
        let manager_for_task = Arc::clone(manager);
        let runtime_for_task = Arc::clone(runtime);
        let options_for_task = options.clone();
        let cancellation_for_task = cancellation.clone();
        let spec_id_for_log = runtime.spec_id.clone();
        tokio::spawn(async move {
            let outcome = run_socks5_connection(
                manager_for_task,
                runtime_for_task.clone(),
                stream,
                cancellation_for_task,
                options_for_task,
            )
            .await;
            if matches!(
                outcome,
                Socks5ConnectionOutcome::BadGateway | Socks5ConnectionOutcome::Forbidden
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
                "socks5 client connection finished"
            );
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_chunk_size_is_bounded() {
        // Static-only check: ensure the read chunk constants stay
        // within the runtime-neutral limits.
        const { assert!(BODY_CHUNK_BYTES <= 32 * 1024) };
        assert!(GREETING_READ_DEADLINE.as_secs() >= 5);
        assert!(REQUEST_READ_DEADLINE.as_secs() >= 5);
    }
}
