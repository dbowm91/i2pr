//! Plan 290 Milestone 10 filtered HTTP server executor.
//!
//! Java I2PTunnel's HTTP server mode ("Creates a destination to a
//! local HTTP server ip:port") terminates inbound I2P Streaming
//! connections, filters the HTTP request through the server privacy
//! contract, and forwards to a bounded loopback target:
//!
//! ```text
//! Streaming SYN poll (shared accept path)
//!   -> shared SYN accept (real peer metadata, permit, driver wake)
//!   -> TCP connect to the loopback target (bounded deadline)
//!   -> read request head from Streaming (deadline + ceiling:
//!      slowloris protection)
//!   -> runtime-neutral server filter (origin-form, Host replace,
//!      hop-by-hop + identifying strip, forced close)
//!   -> forward filtered head + paced body to the target
//!   -> read response head from the target, filter, admit to
//!      Streaming (framing + x-i2p-gzip preserved)
//!   -> relay the remainder both directions to EOF/cancel/terminal
//! ```
//!
//! The profile publishes one persistent destination, never dials
//! out, never resolves remote destinations, and never injects
//! peer-identity headers. No new Garlic/I2NP/Streaming
//! implementation is introduced.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use i2pr_client::streaming::connection::ConnectionId;
use i2pr_client::streaming::manager::RemoteDestination;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::{
    HttpErrorKind, HttpLimits, build_error_response, classify_presentation, filter_server_request,
    filter_server_response, parse_origin_form, parse_request_head,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::debug;

use crate::destination_streaming::{PumpEndpointError, PumpSendDisposition, StreamPumpEndpoint};
use crate::service_tunnels::{
    ServiceRuntime, ServiceTunnelManager, accept_server_syn, poll_streaming_accept,
};

/// Default ceiling for reading one head section (request from
/// Streaming, response from the target).
const HEAD_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Read chunk size for head/body relay.
const RELAY_CHUNK_BYTES: usize = 32 * 1024;
/// Bounded admission retries before a backpressured forward times out.
const ADMIT_RETRIES: usize = 400;
/// Park between backpressured admission retries.
const ADMIT_PARK: Duration = Duration::from_millis(5);
/// Poll cadence for delivered Streaming bytes.
const DELIVERED_POLL: Duration = Duration::from_millis(50);

/// Outcome of one HTTP server connection (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpServerConnectionOutcome {
    /// Request/response relay completed and Streaming was torn down.
    RelayClosed,
    /// Request head was malformed/filtered-out; a bounded error
    /// response was admitted best-effort.
    BadRequest,
    /// Request presentation class is gated closed by the HTTP
    /// server policy; a bounded 403 was admitted best-effort.
    Forbidden,
    /// The local target could not be reached.
    BadGateway,
    /// A head/body read or admission deadline expired.
    TimedOut,
}

/// Per-connection filtered HTTP server entry: terminates one
/// accepted inbound Streaming connection at the loopback target.
pub(crate) async fn run_http_server_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    dial_targets: Vec<SocketAddr>,
    connection_id: ConnectionId,
    peer: RemoteDestination,
    cancellation: CancellationToken,
) -> HttpServerConnectionOutcome {
    let connect_deadline = lookup_connect_timeout(&manager, &runtime.spec_id);
    let unique_local = manager.unique_local_for(&runtime.spec_id);
    let dial = timeout(
        Duration::from_millis(connect_deadline),
        crate::service_tunnels::dial_server_targets(
            &dial_targets,
            &peer.destination_hash,
            unique_local,
        ),
    )
    .await;
    let (target_stream, used_target) = match dial {
        Ok(Ok((stream, used, fell_back))) => {
            if fell_back {
                runtime
                    .unique_local_fallbacks
                    .fetch_add(1, Ordering::Relaxed);
            }
            // Plan 297: same TLS upgrade as the generic server
            // path (verified TLS or typed failure, never
            // plaintext fallback).
            if manager.use_ssl_for(&runtime.spec_id) {
                match crate::service_tunnels::upgrade_server_target_tls(
                    &manager, &runtime, stream, used,
                )
                .await
                {
                    Ok(tls) => {
                        runtime.tls_handshakes_ok.fetch_add(1, Ordering::Relaxed);
                        (
                            crate::service_tunnels::ServerTargetStream::Tls(Box::new(tls)),
                            used,
                        )
                    }
                    Err(_) => {
                        runtime
                            .tls_handshakes_failed
                            .fetch_add(1, Ordering::Relaxed);
                        runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
                        remove_streaming(manager.as_ref(), runtime.destination_id, connection_id);
                        return HttpServerConnectionOutcome::BadGateway;
                    }
                }
            } else {
                (
                    crate::service_tunnels::ServerTargetStream::Plain(stream),
                    used,
                )
            }
        }
        Ok(Err(_)) | Err(_) => {
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            remove_streaming(manager.as_ref(), runtime.destination_id, connection_id);
            return HttpServerConnectionOutcome::BadGateway;
        }
    };
    let endpoint: Arc<dyn StreamPumpEndpoint> =
        Arc::new(crate::service_tunnels::ServicePumpEndpoint::new_server(
            Arc::clone(&manager),
            runtime.destination_id,
            connection_id,
            peer,
        ));
    let outcome = drive_relay(
        used_target,
        target_stream,
        endpoint,
        manager.http_policy_for(&runtime.spec_id),
        &cancellation,
    )
    .await;
    remove_streaming(manager.as_ref(), runtime.destination_id, connection_id);
    outcome
}

/// Drives the filter relay for one established server connection.
async fn drive_relay<S>(
    target: SocketAddr,
    target_stream: S,
    endpoint: Arc<dyn StreamPumpEndpoint>,
    policy: i2pr_service_tunnels::HttpServerPolicy,
    cancellation: &CancellationToken,
) -> HttpServerConnectionOutcome
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let limits = HttpLimits::defaults();
    let (mut target_reader, mut target_writer) = tokio::io::split(target_stream);
    // Step 1: read the request head from Streaming (slowloris:
    // deadline + retained ceiling).
    let (head_bytes, head_prefix) =
        match read_streaming_head(endpoint.as_ref(), limits, cancellation).await {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };
    let head = match parse_request_head(&head_bytes, limits) {
        Ok(value) => value,
        Err(error) => {
            admit_error(
                endpoint.as_ref(),
                error.error.kind,
                error.error.reason,
                cancellation,
            )
            .await;
            return HttpServerConnectionOutcome::BadRequest;
        }
    };
    // Same-read pipelined bytes after the head belong to the
    // request body; carry them into the body forward.
    let mut body_prefix = head.initial_body_bytes.clone();
    body_prefix.extend_from_slice(&head_prefix);
    // Plan 292: presentation gates run before the filter so a
    // closed helper/jump class never reaches the local target.
    // Only well-formed origin-form targets can match a class;
    // anything else stays Ordinary and the filter below still
    // rejects non-origin-form targets as before.
    if head.line.target.starts_with('/')
        && let Ok((path, query)) = parse_origin_form(&head.line.target)
    {
        let class = classify_presentation(&path, &query);
        if !policy.admits(class) {
            admit_error(
                endpoint.as_ref(),
                HttpErrorKind::PresentationRefused,
                "presentation class gated closed",
                cancellation,
            )
            .await;
            return HttpServerConnectionOutcome::Forbidden;
        }
    }
    let filtered = match filter_server_request(&head, &target.to_string()) {
        Ok(value) => value,
        Err(error) => {
            admit_error(endpoint.as_ref(), error.kind, error.reason, cancellation).await;
            return HttpServerConnectionOutcome::BadRequest;
        }
    };
    // Step 2: forward the filtered head to the target.
    if let Err(error) = timeout(
        HEAD_READ_DEADLINE,
        target_writer.write_all(&filtered.head_bytes),
    )
    .await
    {
        debug!(error = %error, "http server target head write failed");
        return HttpServerConnectionOutcome::BadGateway;
    }
    // Step 3: pace the request body (POST throttling: bounded
    // chunks under deadline, never buffered whole).
    if let Some(content_length) = filtered.content_length
        && !forward_body_to_target(
            endpoint.as_ref(),
            &mut target_writer,
            &mut body_prefix,
            content_length,
            cancellation,
        )
        .await
    {
        return HttpServerConnectionOutcome::TimedOut;
    }
    if let Err(error) = target_writer.flush().await {
        debug!(error = %error, "http server target flush failed");
        return HttpServerConnectionOutcome::BadGateway;
    }
    // Step 4: read the response head from the target and filter it.
    let (response_head, response_prefix) =
        match read_target_head(&mut target_reader, limits, cancellation).await {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };
    let filtered_response = match filter_server_response(&response_head) {
        Ok(bytes) => bytes,
        Err(error) => {
            admit_error(endpoint.as_ref(), error.kind, error.reason, cancellation).await;
            return HttpServerConnectionOutcome::BadRequest;
        }
    };
    if admit_bytes(endpoint.as_ref(), &filtered_response, cancellation)
        .await
        .is_err()
    {
        return HttpServerConnectionOutcome::TimedOut;
    }
    // Same-read bytes after the response head belong to the body;
    // admit them before the relay loop.
    if !response_prefix.is_empty()
        && admit_bytes(endpoint.as_ref(), &response_prefix, cancellation)
            .await
            .is_err()
    {
        return HttpServerConnectionOutcome::TimedOut;
    }
    // Step 5: relay the remainder (response body framing passes
    // through untouched) until target EOF, Streaming terminal, or
    // cancel.
    relay_remainder(endpoint.as_ref(), &mut target_reader, cancellation).await;
    HttpServerConnectionOutcome::RelayClosed
}

/// Reads one CRLF CRLF-terminated head section from delivered
/// Streaming bytes under a bounded deadline. Returns the head
/// plus any same-read bytes already buffered past the terminator
/// (request body prefix; never discarded).
async fn read_streaming_head(
    endpoint: &dyn StreamPumpEndpoint,
    limits: HttpLimits,
    cancellation: &CancellationToken,
) -> Result<(Vec<u8>, Vec<u8>), HttpServerConnectionOutcome> {
    let started = Instant::now();
    let mut buffered = Vec::new();
    loop {
        if cancellation.is_cancelled() || endpoint.is_terminal() {
            return Err(HttpServerConnectionOutcome::RelayClosed);
        }
        if started.elapsed() >= HEAD_READ_DEADLINE {
            return Err(HttpServerConnectionOutcome::TimedOut);
        }
        // Accumulate the whole drain batch before scanning for
        // the terminator: `drain_delivered` destructively removes
        // every pending unit from the manager, so returning from
        // inside the per-unit loop would silently drop same-batch
        // body bytes that arrived with the head (multi-segment
        // POST stalls: the peer already counts them delivered).
        for delivered in endpoint.drain_delivered() {
            buffered.extend_from_slice(&delivered);
            if buffered.len() > limits.retained_buffer_max_bytes {
                return Err(HttpServerConnectionOutcome::BadRequest);
            }
        }
        if let Some(end) = find_head_end(&buffered) {
            let prefix = buffered[end..].to_vec();
            return Ok((buffered[..end].to_vec(), prefix));
        }
        // Poll Streaming timers + wake the driver so ACKs and
        // retransmits flow during pure-receive phases (mirrors the
        // shared pump's `notify_outbound` discipline; without this
        // multi-segment bodies stall after the initial window).
        endpoint.notify_outbound();
        tokio::task::yield_now().await;
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(HttpServerConnectionOutcome::RelayClosed),
            _ = tokio::time::sleep(DELIVERED_POLL) => {}
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(HttpServerConnectionOutcome::RelayClosed),
            _ = tokio::time::sleep(DELIVERED_POLL) => {}
        }
    }
}

/// Reads one CRLF CRLF-terminated head section from the local
/// target under a bounded deadline. Returns the head plus any
/// same-read bytes already buffered past the terminator (response
/// body prefix; never discarded).
async fn read_target_head(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    limits: HttpLimits,
    cancellation: &CancellationToken,
) -> Result<(Vec<u8>, Vec<u8>), HttpServerConnectionOutcome> {
    let mut buffered = Vec::new();
    let mut chunk = [0_u8; 1024];
    let started = Instant::now();
    loop {
        if cancellation.is_cancelled() {
            return Err(HttpServerConnectionOutcome::RelayClosed);
        }
        if started.elapsed() >= HEAD_READ_DEADLINE {
            return Err(HttpServerConnectionOutcome::TimedOut);
        }
        let remaining = HEAD_READ_DEADLINE.saturating_sub(started.elapsed());
        match timeout(remaining, reader.read(&mut chunk)).await {
            Ok(Ok(0)) => return Err(HttpServerConnectionOutcome::BadGateway),
            Ok(Ok(n)) => {
                buffered.extend_from_slice(&chunk[..n]);
                if buffered.len() > limits.retained_buffer_max_bytes {
                    return Err(HttpServerConnectionOutcome::BadRequest);
                }
                if let Some(end) = find_head_end(&buffered) {
                    let prefix = buffered[end..].to_vec();
                    return Ok((buffered[..end].to_vec(), prefix));
                }
            }
            Ok(Err(_)) => return Err(HttpServerConnectionOutcome::BadGateway),
            Err(_) => return Err(HttpServerConnectionOutcome::TimedOut),
        }
    }
}

/// Returns the end offset (exclusive) of the first CRLF CRLF
/// terminator, if present.
fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

/// Admits one bounded error response into Streaming best-effort
/// (the handshake may never complete; the connection is torn down
/// either way).
async fn admit_error(
    endpoint: &dyn StreamPumpEndpoint,
    kind: HttpErrorKind,
    reason: &'static str,
    cancellation: &CancellationToken,
) {
    let bytes = build_error_response(kind, reason);
    let _ = admit_bytes(endpoint, &bytes, cancellation).await;
}

/// Admits bytes into Streaming in max-payload segments with
/// bounded backpressure retries.
async fn admit_bytes(
    endpoint: &dyn StreamPumpEndpoint,
    bytes: &[u8],
    cancellation: &CancellationToken,
) -> Result<(), HttpServerConnectionOutcome> {
    let max_payload = endpoint.max_payload_bytes().max(1);
    let mut offset = 0_usize;
    let mut retries = 0_usize;
    while offset < bytes.len() {
        if cancellation.is_cancelled() || endpoint.is_terminal() {
            return Err(HttpServerConnectionOutcome::RelayClosed);
        }
        let end = (offset + max_payload).min(bytes.len());
        match endpoint.try_send(&bytes[offset..end]) {
            Ok(PumpSendDisposition::Accepted) => {
                offset = end;
                retries = 0;
                endpoint.notify_outbound();
            }
            Ok(PumpSendDisposition::Backpressured) => {
                retries = retries.saturating_add(1);
                if retries > ADMIT_RETRIES {
                    return Err(HttpServerConnectionOutcome::TimedOut);
                }
                tokio::time::sleep(ADMIT_PARK).await;
            }
            Err(PumpEndpointError::UnknownConnection | PumpEndpointError::InvalidState) => {
                return Err(HttpServerConnectionOutcome::BadGateway);
            }
            Err(_) => return Err(HttpServerConnectionOutcome::BadRequest),
        }
    }
    Ok(())
}

/// Forwards exactly `content_length` body bytes (plus already
/// buffered same-read prefix) from Streaming to the target in
/// bounded chunks.
async fn forward_body_to_target(
    endpoint: &dyn StreamPumpEndpoint,
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    prefix: &mut Vec<u8>,
    content_length: u64,
    cancellation: &CancellationToken,
) -> bool {
    let started = Instant::now();
    let mut remaining = content_length;
    // Same-read pipelined prefix counts toward the body first.
    let take = (prefix.len() as u64).min(remaining) as usize;
    if writer.write_all(&prefix[..take]).await.is_err() {
        return false;
    }
    prefix.drain(..take);
    remaining -= take as u64;
    while remaining > 0 {
        if cancellation.is_cancelled() || endpoint.is_terminal() {
            return false;
        }
        if started.elapsed() >= HEAD_READ_DEADLINE {
            return false;
        }
        let mut progressed = false;
        for delivered in endpoint.drain_delivered() {
            if delivered.is_empty() {
                continue;
            }
            let take = (delivered.len() as u64).min(remaining) as usize;
            if writer.write_all(&delivered[..take]).await.is_err() {
                return false;
            }
            remaining -= take as u64;
            progressed = true;
            if remaining == 0 {
                break;
            }
        }
        // Same timer/driver discipline as above: body pacing is a
        // pure-receive phase for Streaming.
        endpoint.notify_outbound();
        if !progressed {
            tokio::time::sleep(DELIVERED_POLL).await;
        } else {
            // Give the delivery driver a chance to run delivery
            // before looping back to the drain; otherwise a fast
            // loop starves the driver task on a single-threaded
            // runtime (mirrors the shared pump).
            tokio::task::yield_now().await;
        }
    }
    true
}

/// Relays response bytes from the target into Streaming until
/// target EOF, Streaming terminal, or cancel. The request side is
/// complete at this point (`Connection: close`, no pipelining):
/// any further delivered Streaming bytes terminate the relay
/// instead of being forwarded.
async fn relay_remainder(
    endpoint: &dyn StreamPumpEndpoint,
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    cancellation: &CancellationToken,
) {
    let mut chunk = [0_u8; RELAY_CHUNK_BYTES];
    loop {
        if cancellation.is_cancelled() || endpoint.is_terminal() {
            return;
        }
        if !endpoint.drain_delivered().is_empty() {
            // Pipelined data after a complete close-delimited
            // request: not forwarded, relay ends.
            return;
        }
        // Keep timers/driver moving while the target streams.
        endpoint.notify_outbound();
        match timeout(DELIVERED_POLL, reader.read(&mut chunk)).await {
            Ok(Ok(0)) => return,
            Ok(Ok(n)) => {
                if admit_bytes(endpoint, &chunk[..n], cancellation)
                    .await
                    .is_err()
                {
                    return;
                }
                // Same yield discipline: the relay must not starve
                // the driver on a single-threaded runtime.
                tokio::task::yield_now().await;
            }
            Ok(Err(_)) => return,
            Err(_) => continue,
        }
    }
}

/// Removes one server Streaming connection from the receiver
/// manager (mirrors the generic server pump teardown).
fn remove_streaming(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
) {
    manager.with_destination_bridge(destination_id, |bridge| {
        let _ = bridge
            .receiver_streaming_mut()
            .remove_connection(connection_id);
    });
}

/// Looks up the configured connect deadline for one service.
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

/// Handles one accepted inbound SYN for the HTTP server profile:
/// shared SYN accept, then the filtered relay task under the
/// aggregate slot.
async fn handle_http_server_syn(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    connection_id: ConnectionId,
    cancellation: &CancellationToken,
) {
    let Some((peer, permit_for_task)) = accept_server_syn(manager, runtime, connection_id).await
    else {
        return;
    };
    let manager_for_task = Arc::clone(manager);
    let runtime_for_task = Arc::clone(runtime);
    let cancellation_for_task = cancellation.clone();
    let spec_id_for_log = runtime.spec_id.clone();
    // Plan 296: resolve the ordered dial targets per connection so
    // multihoming rotation advances once per accepted SYN.
    let dial_targets = manager.server_dial_targets_for(runtime);
    tokio::spawn(async move {
        let outcome = run_http_server_connection(
            manager_for_task,
            runtime_for_task.clone(),
            dial_targets,
            connection_id,
            peer,
            cancellation_for_task,
        )
        .await;
        if matches!(
            outcome,
            HttpServerConnectionOutcome::BadGateway | HttpServerConnectionOutcome::BadRequest
        ) {
            runtime_for_task
                .failed_connects
                .fetch_add(1, Ordering::Relaxed);
        }
        runtime_for_task.connection_finished_now();
        drop(permit_for_task);
        debug!(
            service = %spec_id_for_log,
            ?outcome,
            "http server connection finished"
        );
    });
}

/// Runs the per-service supervisor loop for a filtered HTTP
/// server tunnel.
pub async fn run_http_server_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let target_socket = runtime.server_target.ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "http server tunnel missing loopback target".to_owned(),
        )
    })?;
    tracing::info!(
        service = %spec.id.as_str(),
        target = %target_socket,
        "http server tunnel bound streaming listener"
    );
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let port = runtime.server_streaming_port.unwrap_or(0_u16);
    let drain_cancel = runtime.admission_cancellation_token();
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            // Plan 289: drained server runtimes stop polling
            // promptly (see `run_server_loop`).
            _ = drain_cancel.cancelled() => break,
            _ = ticker.tick() => {}
        }
        for connection_id in poll_streaming_accept(manager, runtime, port) {
            handle_http_server_syn(manager, runtime, connection_id, cancellation).await;
        }
    }
    Ok(())
}
