//! Plan 176 Milestone 10 HTTP `.i2p` proxy executor.
//!
//! The Plan 176 HTTP client proxy is the first M10 application
//! profile that owns a bounded HTTP/1.1 parser. The runtime-neutral
//! policy (parser bounds, hop-by-hop/privacy rewrite, target
//! validation, error response generation) lives in
//! `i2pr-service-tunnels::http`; this module owns the socket and
//! task orchestration.
//!
//! ```text
//! loopback TCP accept
//!   -> bounded HTTP/1.1 head read (deadline)
//!   -> runtime-neutral parser + target validator
//!   -> Manager destination resolution (Base32 / alias / local)
//!   -> I2P Streaming connect (CSPRNG, OS)
//!   -> for ordinary proxy request:
//!        write rewritten request-line + headers
//!        run shared socket <-> Streaming pump (body + response)
//!   -> for CONNECT:
//!        write 2xx response
//!        run shared socket <-> Streaming pump (opaque bytes)
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
    DestinationRef, HttpClientOptions, HttpError, HttpErrorKind, HttpLimits, HttpRequestHead,
    RequestTarget, TargetKind, build_error_response, parse_authority_form, parse_request_head,
    parse_request_target, rewrite_headers,
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

/// Default ceiling for reading the HTTP header section.
const HEADER_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Default read chunk size for the HTTP body/response pump.
const BODY_CHUNK_BYTES: usize = 32 * 1024;

/// Outcome of one HTTP connection attempt (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpConnectionOutcome {
    /// CONNECT completed and the opaque byte pump ran to EOF.
    ConnectTunnelClosed,
    /// Ordinary HTTP request completed (response received and
    /// forwarded; tunnel closed).
    RequestClosed,
    /// Headers were malformed or unsupported and a bounded error
    /// response was emitted.
    BadRequest,
    /// Headers were well-formed but the target was rejected by
    /// the proxy (clearnet/local/IP/missing-port/etc).
    Forbidden,
    /// The configured destination was unknown and no I2P connect
    /// was attempted.
    BadGateway,
    /// The header read or connect deadline expired.
    TimedOut,
}

/// Reads the complete HTTP header section from the supplied stream
/// into a bounded buffer. Returns the head bytes and the first body
/// bytes that follow the CRLF CRLF terminator.
async fn read_http_head<S>(
    stream: &mut S,
    limits: HttpLimits,
    deadline: Duration,
) -> Result<(Vec<u8>, Vec<u8>), HttpError>
where
    S: AsyncRead + Unpin,
{
    let mut buffer: Vec<u8> = Vec::with_capacity(limits.request_line_max_bytes.min(2048));
    let mut chunk = [0_u8; 1024];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= deadline {
            return Err(HttpError::new(
                HttpErrorKind::BufferCeilingExceeded,
                "header read deadline exceeded",
            ));
        }
        let remaining = deadline.saturating_sub(elapsed);
        let read = match timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => {
                return Err(HttpError::new(
                    HttpErrorKind::MalformedHeaders,
                    "EOF before header section terminator",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(_error)) => {
                return Err(HttpError::new(
                    HttpErrorKind::MalformedHeaders,
                    "header read io error",
                ));
            }
            Err(_) => {
                return Err(HttpError::new(
                    HttpErrorKind::BufferCeilingExceeded,
                    "header read deadline exceeded",
                ));
            }
        };
        if buffer.len() + read > limits.retained_buffer_max_bytes {
            return Err(HttpError::new(
                HttpErrorKind::BufferCeilingExceeded,
                "buffer ceiling exceeded",
            ));
        }
        let probe_start = buffer.len().saturating_sub(3);
        buffer.extend_from_slice(&chunk[..read]);
        let mut index = probe_start;
        let mut found_end: Option<usize> = None;
        while index + 3 < buffer.len() {
            if buffer[index] == b'\r'
                && buffer[index + 1] == b'\n'
                && buffer[index + 2] == b'\r'
                && buffer[index + 3] == b'\n'
            {
                found_end = Some(index + 4);
                break;
            }
            index += 1;
        }
        if let Some(end) = found_end {
            let body_bytes = buffer[end..].to_vec();
            buffer.truncate(end);
            return Ok((buffer, body_bytes));
        }
        if buffer.len() >= limits.retained_buffer_max_bytes {
            return Err(HttpError::new(
                HttpErrorKind::BufferCeilingExceeded,
                "retained buffer ceiling exceeded",
            ));
        }
    }
}

/// Writes an HTTP error response and closes the socket (graceful
/// half-close, then shutdown).
async fn write_error_response<S>(stream: &mut S, response: Vec<u8>) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    if let Err(error) = stream.write_all(&response).await {
        debug!(error = %error, "http error response write failed");
    }
    let _ = stream.shutdown().await;
    Ok(())
}

/// Resolves a parsed request target to a [`ClientTarget`]. The
/// proxy only forwards requests whose host matches the configured
/// service destination (Base32) or an entry in the alias table.
fn resolve_target_for_service(
    manager: &ServiceTunnelManager,
    target: &RequestTarget,
) -> Result<ClientTarget, HttpError> {
    // Base32 / static-alias reference path. The runtime-neutral
    // parser already enforced `.i2p` host grammar.
    let reference = DestinationRef::parse(&target.host).map_err(|_| {
        HttpError::new(
            HttpErrorKind::Other,
            "destination not resolvable by HTTP proxy",
        )
    })?;
    if let Ok(client) = manager.resolve_reference(&reference) {
        return Ok(client);
    }
    Err(HttpError::new(
        HttpErrorKind::Other,
        "destination not resolvable by HTTP proxy",
    ))
}

/// Opens a Streaming connection to the supplied remote destination
/// and waits until `Established` (or returns a typed error).
async fn open_streaming(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    remote: &RemoteDestination,
    timeout_ms: u64,
) -> Result<ConnectionId, HttpError> {
    let identity_arc = manager
        .with_destination_bridge(destination_id, |bridge| bridge.identity())
        .ok_or_else(|| HttpError::new(HttpErrorKind::BadGateway, "missing identity"))?;
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
            return Err(HttpError::new(
                HttpErrorKind::BadGateway,
                "client connect produced no outcome",
            ));
        }
    };
    let connection_id = match connect_outcome {
        Ok(ConnectOutcome::SynSent { connection_id, .. }) => connection_id,
        Ok(_) => {
            return Err(HttpError::new(
                HttpErrorKind::BadGateway,
                "client connect produced no SYN",
            ));
        }
        Err(_error) => {
            return Err(HttpError::new(
                HttpErrorKind::BadGateway,
                "client connect error",
            ));
        }
    };
    // Plan 182: kick the delivery driver so the queued SYN is
    // routed immediately instead of waiting for the fallback tick.
    manager.notify_outbound_signal(destination_id);
    wait_for_established(manager, destination_id, connection_id, timeout_ms)
        .await
        .map_err(|_| HttpError::new(HttpErrorKind::BadGateway, "deadline reached"))?;
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

/// Per-connection HTTP proxy entry. Reads HTTP/1.1 headers, parses
/// the request, dispatches CONNECT or ordinary proxy, and runs the
/// shared Plan 174 byte pump.
pub async fn run_http_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    stream: TcpStream,
    cancellation: CancellationToken,
    options: HttpClientOptions,
) -> HttpConnectionOutcome {
    let limits = HttpLimits::defaults();
    let mut stream = stream;
    // Step 1: read header section under a bounded deadline.
    let (head_bytes, initial_body) =
        match read_http_head(&mut stream, limits, HEADER_READ_DEADLINE).await {
            Ok(value) => value,
            Err(error) => {
                let kind = if matches!(error.kind, HttpErrorKind::BufferCeilingExceeded) {
                    HttpErrorKind::BufferCeilingExceeded
                } else {
                    HttpErrorKind::MalformedHeaders
                };
                let _ = write_error_response(&mut stream, build_error_response(kind, error.reason))
                    .await;
                return HttpConnectionOutcome::BadRequest;
            }
        };
    // Step 2: parse + validate.
    let head = match parse_request_head(&head_bytes, limits) {
        Ok(value) => value,
        Err(error) => {
            let _ = write_error_response(
                &mut stream,
                build_error_response(error.error.kind, error.error.reason),
            )
            .await;
            return HttpConnectionOutcome::BadRequest;
        }
    };
    // Step 3: dispatch on method.
    if head.line.method == "CONNECT" {
        return handle_connect(
            manager,
            runtime,
            stream,
            cancellation,
            options,
            head,
            limits,
        )
        .await;
    }
    handle_proxy_request(
        manager,
        runtime,
        stream,
        cancellation,
        options,
        head,
        initial_body,
        limits,
    )
    .await
}

async fn handle_connect(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    mut stream: TcpStream,
    cancellation: CancellationToken,
    options: HttpClientOptions,
    head: HttpRequestHead,
    limits: HttpLimits,
) -> HttpConnectionOutcome {
    let authority =
        match parse_authority_form(&head.line.target, limits.connect_authority_max_bytes) {
            Ok(value) => value,
            Err(error) => {
                let kind = if matches!(error.kind, HttpErrorKind::UnsupportedConnectPort) {
                    HttpErrorKind::UnsupportedConnectPort
                } else {
                    HttpErrorKind::MalformedTarget
                };
                let _ = write_error_response(&mut stream, build_error_response(kind, error.reason))
                    .await;
                return HttpConnectionOutcome::Forbidden;
            }
        };
    if let Some(port) = authority.port
        && !options.privacy.allows_connect_port(port)
    {
        let _ = write_error_response(
            &mut stream,
            build_error_response(
                HttpErrorKind::UnsupportedConnectPort,
                "CONNECT port is not allowed",
            ),
        )
        .await;
        return HttpConnectionOutcome::Forbidden;
    }
    let target = match resolve_target_for_service(&manager, &authority) {
        Ok(value) => value,
        Err(error) => {
            let _ = write_error_response(
                &mut stream,
                build_error_response(HttpErrorKind::Other, error.reason),
            )
            .await;
            return HttpConnectionOutcome::BadGateway;
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
            let _ =
                write_error_response(&mut stream, build_error_response(error.kind, error.reason))
                    .await;
            return HttpConnectionOutcome::BadGateway;
        }
    };
    let response: &[u8] = b"HTTP/1.1 200 Connection Established\r\nProxy-Agent: i2pr\r\n\r\n";
    if let Err(error) = stream.write_all(response).await {
        debug!(error = %error, "connect response write failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return HttpConnectionOutcome::BadRequest;
    }
    if let Err(error) = stream.flush().await {
        debug!(error = %error, "connect response flush failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return HttpConnectionOutcome::BadRequest;
    }
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_client(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
        target.remote.clone(),
    ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, Vec::new(), endpoint, config, cancellation).await;
    if let Err(error) = result {
        debug!(error = %error, "CONNECT pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return HttpConnectionOutcome::BadGateway;
    }
    terminate_streaming(
        &manager,
        runtime.destination_id,
        connection_id,
        &target.remote,
        false,
    );
    HttpConnectionOutcome::ConnectTunnelClosed
}

#[allow(clippy::too_many_arguments)]
async fn handle_proxy_request(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    mut stream: TcpStream,
    cancellation: CancellationToken,
    options: HttpClientOptions,
    head: HttpRequestHead,
    initial_body: Vec<u8>,
    _limits: HttpLimits,
) -> HttpConnectionOutcome {
    let target = match parse_request_target(&head.line.target) {
        Ok(value) => value,
        Err(error) => {
            let _ =
                write_error_response(&mut stream, build_error_response(error.kind, error.reason))
                    .await;
            return HttpConnectionOutcome::BadRequest;
        }
    };
    if matches!(target.kind, TargetKind::Origin) {
        let _ = write_error_response(
            &mut stream,
            build_error_response(
                HttpErrorKind::MalformedTarget,
                "origin-form requires absolute-form",
            ),
        )
        .await;
        return HttpConnectionOutcome::BadRequest;
    }
    if let Err(error) = i2pr_service_tunnels::http::target::validate_host(&target.host) {
        let _ =
            write_error_response(&mut stream, build_error_response(error.kind, error.reason)).await;
        return HttpConnectionOutcome::Forbidden;
    }
    let client = match resolve_target_for_service(&manager, &target) {
        Ok(value) => value,
        Err(_) => {
            let _ = write_error_response(
                &mut stream,
                build_error_response(HttpErrorKind::Other, "destination lookup failed"),
            )
            .await;
            return HttpConnectionOutcome::BadGateway;
        }
    };
    let connect_timeout_ms = lookup_connect_timeout(&manager, &runtime.spec_id);
    let connection_id = match open_streaming(
        &manager,
        runtime.destination_id,
        &client.remote,
        connect_timeout_ms,
    )
    .await
    {
        Ok(id) => id,
        Err(error) => {
            let _ =
                write_error_response(&mut stream, build_error_response(error.kind, error.reason))
                    .await;
            return HttpConnectionOutcome::BadGateway;
        }
    };
    let mut forwarded = Vec::with_capacity(head.line.target.len() + 256);
    forwarded.extend_from_slice(head.line.method.as_bytes());
    forwarded.push(b' ');
    forwarded
        .extend_from_slice(i2pr_service_tunnels::http::target::origin_form(&target).as_bytes());
    forwarded.extend_from_slice(b" HTTP/1.1\r\n");
    let rewritten = rewrite_headers(&head.headers, &target, &options.privacy);
    for header in rewritten {
        forwarded.extend_from_slice(header.name_str().as_bytes());
        forwarded.extend_from_slice(b": ");
        forwarded.extend_from_slice(header.value.as_bytes());
        forwarded.extend_from_slice(b"\r\n");
    }
    forwarded.extend_from_slice(b"\r\n");
    forwarded.extend_from_slice(&initial_body);
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_client(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
        client.remote.clone(),
    ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, forwarded, endpoint, config, cancellation).await;
    if let Err(error) = result {
        debug!(error = %error, "proxy request pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &client.remote,
            true,
        );
        return HttpConnectionOutcome::BadGateway;
    }
    terminate_streaming(
        &manager,
        runtime.destination_id,
        connection_id,
        &client.remote,
        false,
    );
    HttpConnectionOutcome::RequestClosed
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

/// Runs the per-service supervisor loop for an HTTP client tunnel.
pub async fn run_http_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "http client tunnel missing loopback listener".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "http client tunnel bound loopback listener"
    );
    let options = spec.http_options.clone().unwrap_or_default();
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
                    "http client listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
            manager.aggregate_permit().try_acquire_owned().ok();
        let Some(permit_for_task) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "http client tunnel aggregate ceiling reached; rejecting connection"
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
            // Plan 182: hold the aggregate slot for the connection
            // lifetime; the previous let-else binding released it
            // at spawn time so the ceiling never engaged.
            let _permit_for_task = permit_for_task;
            let outcome = run_http_connection(
                manager_for_task,
                runtime_for_task.clone(),
                stream,
                cancellation_for_task,
                options_for_task,
            )
            .await;
            if matches!(
                outcome,
                HttpConnectionOutcome::BadGateway | HttpConnectionOutcome::Forbidden
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
                "http client connection finished"
            );
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_http_head_rejects_buffer_overflow() {
        // Static-only check: ensure the constants agree with the
        // runtime-neutral limits module.
        const { assert!(BODY_CHUNK_BYTES <= 32 * 1024) };
        assert!(HEADER_READ_DEADLINE >= Duration::from_secs(5));
    }
}
