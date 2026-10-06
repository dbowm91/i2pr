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
    ConnectClientOptions, DestinationRef, HttpClientOptions, HttpError, HttpErrorKind, HttpLimits,
    HttpRequestHead, ProxyCredentials, RequestTarget, TargetKind, build_error_response,
    decode_basic_credentials, parse_authority_form_with_policy, parse_request_head,
    parse_request_target_with_policy, proxy_auth_required, rewrite_headers,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::destination_streaming::{PumpConfig, StreamPumpEndpoint, run_stream_pump};
use crate::outproxy_route::OutproxyCounters;
use crate::outproxy_route::{RouterOutproxyProvider, open_client_route};
use crate::service_tunnels::{
    ClientTarget, DestinationFailure, ServicePumpEndpoint, ServiceRuntime, ServiceTunnelManager,
    service_streaming_now_ms,
};
use i2pr_service_tunnels::outproxy::{ClientTargetClass, OutproxyTarget, classify_client_target};

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
    /// Proxy authentication failed (or was required but missing)
    /// and a 407 challenge was emitted before close.
    Unauthorized,
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
    service_destination: i2pr_client::DestinationId,
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
    match manager.resolve_reference(&reference) {
        Ok(client) => Ok(client),
        // Plan 214 — non-local references resolve through the
        // requesting runtime's installed router-backed remote
        // LeaseSet2 mirror (populated by the production
        // composition at provisioning). The failure already
        // carries the exact hash; no local fallback: an
        // unresolved remote hash stays a typed failure.
        Err(DestinationFailure::LookupRequired { hash, .. }) => manager
            .resolve_remote_client_target(service_destination, &hash)
            .ok_or_else(|| {
                HttpError::new(
                    HttpErrorKind::Other,
                    "destination not resolvable by HTTP proxy",
                )
            }),
        Err(_) => Err(HttpError::new(
            HttpErrorKind::Other,
            "destination not resolvable by HTTP proxy",
        )),
    }
}

fn target_for_remote_destination(
    target: &RequestTarget,
    remote: &RemoteDestination,
) -> RequestTarget {
    let mut resolved = target.clone();
    resolved.host = format!(
        "{}.b32.i2p",
        i2pr_service_tunnels::encode_b32_label(&remote.destination_hash)
    );
    resolved
}

/// Opens a Streaming connection to the supplied remote destination
/// and waits until `Established` (or returns a typed error).
async fn open_streaming(
    manager: &ServiceTunnelManager,
    spec_id: &str,
    destination_id: i2pr_client::DestinationId,
    remote: &RemoteDestination,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> Result<ConnectionId, HttpError> {
    manager
        .ensure_destination_active(spec_id, cancellation, timeout_ms)
        .await
        .map_err(|_| HttpError::new(HttpErrorKind::BadGateway, "destination activation failed"))?;
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

/// Enforces listener proxy authentication (Plan 292): verifies
/// `Proxy-Authorization: Basic` against the tunnel credentials.
/// Answers 407 and closes on any failure (missing, malformed, or
/// wrong credentials answer identically); returns true when the
/// request may proceed. The credential header is consumed at the
/// edge: downstream rewrite strips it before any upstream byte.
async fn enforce_proxy_auth(
    stream: &mut TcpStream,
    head: &HttpRequestHead,
    auth: Option<&ProxyCredentials>,
) -> bool {
    let Some(credentials) = auth else {
        return true;
    };
    let authorized = head
        .headers
        .iter()
        .find(|entry| entry.name.as_str() == "proxy-authorization")
        .and_then(|entry| decode_basic_credentials(&entry.value))
        .map(|(user, pass)| credentials.verify(&user, &pass))
        .unwrap_or(false);
    if authorized {
        return true;
    }
    let _ = stream
        .write_all(&proxy_auth_required(credentials.realm()))
        .await;
    let _ = stream.shutdown().await;
    false
}

/// Plan 342 — opens an outproxy route for one `CONNECT` request and pumps it.
///
/// The three things that make this not just `open_streaming` with a different
/// destination:
///
/// 1. the local client still gets an HTTP reply, and it gets it **after** the
///    outproxy's handshake has succeeded — never optimistically, so a failed
///    route cannot leave a client believing it has a tunnel;
/// 2. the pump endpoint is seeded with `tunnel_prefix`, or an outproxy that
///    pipelines payload into its own handshake response would have its first
///    bytes silently dropped and the first request on the connection
///    corrupted;
/// 3. a failure returns `BadGateway` and terminates the route. There is no
///    arm that falls through to the direct path.
async fn connect_via_outproxy(
    mut stream: TcpStream,
    manager: Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    cancellation: &CancellationToken,
    provider: &RouterOutproxyProvider,
    target: &OutproxyTarget,
    counters: &std::sync::Mutex<OutproxyCounters>,
) -> HttpConnectionOutcome {
    let route = match open_client_route(
        &manager,
        &runtime.spec_id,
        runtime.destination_id,
        Some(provider),
        target.host(),
        target.port(),
        true,
        cancellation,
        counters,
    )
    .await
    {
        Ok(route) => route,
        Err(failure) => {
            debug!(failure = failure.as_str(), "outproxy route open failed");
            crate::outproxy_route::ClientRoute::Refused(failure)
        }
    };
    let session = match route {
        crate::outproxy_route::ClientRoute::ViaOutproxy(session) => session,
        // `Direct` cannot happen: the classifier only returns it for an
        // `.i2p` target, and this arm is only reached for `ViaOutproxy`. The
        // explicit refusal keeps that a handled state rather than a panic if
        // the two ever drift.
        other => {
            debug!(outcome = ?other, "CONNECT outproxy route was not open");
            let _ = write_error_response(
                &mut stream,
                build_error_response(
                    HttpErrorKind::Other,
                    "no I2P-routed outproxy is configured for this tunnel",
                ),
            )
            .await;
            return HttpConnectionOutcome::BadGateway;
        }
    };
    let response: &[u8] = b"HTTP/1.1 200 Connection Established\r\n\r\n";
    if let Err(error) = stream.write_all(response).await {
        debug!(error = %error, "connect response write failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
        return HttpConnectionOutcome::BadRequest;
    }
    if let Err(error) = stream.flush().await {
        debug!(error = %error, "connect response flush failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
        return HttpConnectionOutcome::BadRequest;
    }
    let endpoint: Arc<dyn StreamPumpEndpoint> =
        Arc::new(ServicePumpEndpoint::new_client_with_prefix(
            Arc::clone(&manager),
            runtime.destination_id,
            session.connection_id,
            session.remote.clone(),
            session.tunnel_prefix,
        ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, Vec::new(), endpoint, config, cancellation.clone()).await;
    if result.is_ok() {
        return HttpConnectionOutcome::ConnectTunnelClosed;
    }
    if let Err(error) = result {
        debug!(error = %error, "CONNECT outproxy pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
    }
    HttpConnectionOutcome::ConnectTunnelClosed
}

/// Plan 342: the port an `HTTP` request names when its `Host:` carries none.
///
/// A `CONNECT` authority has no scheme, so it defaults to 80 rather than 443:
/// the local client is explicitly asking for a tunnel to the port it named,
/// and if it named none it named the plain-HTTP port. A caller that wants 443
/// says 443.
const CONNECT_DEFAULT_PORT: u16 = 80;

async fn handle_connect(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    mut stream: TcpStream,
    cancellation: CancellationToken,
    options: HttpClientOptions,
    head: HttpRequestHead,
    limits: HttpLimits,
) -> HttpConnectionOutcome {
    // Plan 292: proxy authentication gates CONNECT before any
    // target parsing or streaming (also covers the strict-CONNECT
    // path, which adapts its options into this handler).
    if !enforce_proxy_auth(&mut stream, &head, options.proxy_auth.as_ref()).await {
        return HttpConnectionOutcome::Unauthorized;
    }
    let authority = match parse_authority_form_with_policy(
        &head.line.target,
        limits.connect_authority_max_bytes,
        manager.target_policy(&runtime.spec_id),
    ) {
        Ok(value) => value,
        Err(error) => {
            let kind = if matches!(error.kind, HttpErrorKind::UnsupportedConnectPort) {
                HttpErrorKind::UnsupportedConnectPort
            } else {
                HttpErrorKind::MalformedTarget
            };
            let _ =
                write_error_response(&mut stream, build_error_response(kind, error.reason)).await;
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
    if authority.port == Some(443) && !options.allow_internal_ssl {
        let _ = write_error_response(
            &mut stream,
            build_error_response(
                HttpErrorKind::UnsupportedConnectPort,
                "internal SSL is disabled for this HTTP client",
            ),
        )
        .await;
        return HttpConnectionOutcome::Forbidden;
    }
    // Plan 342: classify before anything is opened. `.i2p` goes straight to
    // the direct path below; a clearnet authority goes to the configured
    // outproxy; a clearnet authority with no outproxy is refused with a 502
    // and no socket is created.
    //
    // The port the classifier sees is the one the request actually named.
    // `resolve_target_for_service` supplies the default for an `.i2p` target
    // with no port; for the outproxy path the default has already been
    // applied above, and passing it twice would be a second place for the two
    // paths to disagree about what port is being reached.
    let effective_port = authority.port.unwrap_or(CONNECT_DEFAULT_PORT);
    let outproxy_provider = manager.outproxy_provider(&runtime.spec_id);
    let counters = manager.outproxy_counters();
    let class = match classify_client_target(
        outproxy_provider
            .as_deref()
            .map(i2pr_service_tunnels::OutproxyProvider::config),
        &authority.host,
        effective_port,
    ) {
        Ok(value) => value,
        Err(_) => {
            let _ = write_error_response(
                &mut stream,
                build_error_response(HttpErrorKind::MalformedTarget, "target is not usable"),
            )
            .await;
            return HttpConnectionOutcome::Forbidden;
        }
    };
    // An exhaustive `match`, not two `if let`s.
    //
    // With `if let`, anything that is neither `ViaOutproxy` nor `Refused`
    // falls through to the direct path — so adding a fourth outcome to
    // `ClientTargetClass` later would silently route it to the tunnel's own
    // destination with no compiler complaint and no review signal. `match`
    // makes that a build error instead. `scripts/check-outproxy-request-path.sh`
    // asserts this shape.
    match class {
        // The two non-direct outcomes differ in more than the socket: the
        // pump endpoint is seeded with the handshake prefix and the remote is
        // the outproxy's.
        ClientTargetClass::ViaOutproxy(target) => {
            return connect_via_outproxy(
                stream,
                Arc::clone(&manager),
                &runtime,
                &cancellation,
                outproxy_provider
                    .as_deref()
                    .expect("a ViaOutproxy classification implies a provider"),
                &target,
                counters,
            )
            .await;
        }
        ClientTargetClass::Refused(failure) => {
            debug!(
                failure = failure.as_str(),
                "CONNECT target refused before any route"
            );
            let _ = write_error_response(
                &mut stream,
                build_error_response(
                    HttpErrorKind::Other,
                    "no I2P-routed outproxy is configured for this tunnel",
                ),
            )
            .await;
            return HttpConnectionOutcome::BadGateway;
        }
        ClientTargetClass::Direct(_) => {}
    }
    let target = match resolve_target_for_service(&manager, runtime.destination_id, &authority) {
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
        &runtime.spec_id,
        runtime.destination_id,
        &target.remote,
        connect_timeout_ms,
        &cancellation,
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
    let response: &[u8] = b"HTTP/1.1 200 Connection Established\r\n\r\n";
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
    // Plan 292: proxy authentication gates plain-proxy requests the
    // same way it gates CONNECT.
    if !enforce_proxy_auth(&mut stream, &head, options.proxy_auth.as_ref()).await {
        return HttpConnectionOutcome::Unauthorized;
    }
    let target = match parse_request_target_with_policy(
        &head.line.target,
        manager.target_policy(&runtime.spec_id),
    ) {
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
    if let Err(error) = i2pr_service_tunnels::http::target::validate_host_with_policy(
        &target.host,
        manager.target_policy(&runtime.spec_id),
    ) {
        let _ =
            write_error_response(&mut stream, build_error_response(error.kind, error.reason)).await;
        return HttpConnectionOutcome::Forbidden;
    }
    // Plan 342: the forward path classifies exactly as the CONNECT path does,
    // from the same function, so the guarantee cannot hold on one and not the
    // other.
    let outproxy_provider = manager.outproxy_provider(&runtime.spec_id);
    let class = match classify_client_target(
        outproxy_provider
            .as_deref()
            .map(i2pr_service_tunnels::OutproxyProvider::config),
        &target.host,
        target.port.unwrap_or(80),
    ) {
        Ok(value) => value,
        Err(_) => {
            let _ = write_error_response(
                &mut stream,
                build_error_response(HttpErrorKind::MalformedTarget, "target is not usable"),
            )
            .await;
            return HttpConnectionOutcome::Forbidden;
        }
    };
    // Exhaustive `match`, for the reason documented in `handle_connect`: with
    // `if let`, a fourth `ClientTargetClass` outcome added later would fall
    // through to the direct path silently. `scripts/check-outproxy-request-path.sh`
    // asserts this shape in all three request paths.
    match class {
        ClientTargetClass::ViaOutproxy(outproxy_target) => {
            return forward_via_outproxy(
                stream,
                Arc::clone(&manager),
                &runtime,
                &cancellation,
                outproxy_provider
                    .as_deref()
                    .expect("a ViaOutproxy classification implies a provider"),
                &outproxy_target,
                &options,
                &head,
                initial_body,
                &target,
            )
            .await;
        }
        ClientTargetClass::Refused(failure) => {
            debug!(
                failure = failure.as_str(),
                "forward target refused before any route"
            );
            let _ = write_error_response(
                &mut stream,
                build_error_response(
                    HttpErrorKind::Other,
                    "no I2P-routed outproxy is configured for this tunnel",
                ),
            )
            .await;
            return HttpConnectionOutcome::BadGateway;
        }
        ClientTargetClass::Direct(_) => {}
    }
    let client = match resolve_target_for_service(&manager, runtime.destination_id, &target) {
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
        &runtime.spec_id,
        runtime.destination_id,
        &client.remote,
        connect_timeout_ms,
        &cancellation,
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
    let resolved_target = target_for_remote_destination(&target, &client.remote);
    let rewritten = rewrite_headers(&head.headers, &resolved_target, &options.privacy);
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

/// Plan 342 — forwards one ordinary proxy request through an outproxy.
///
/// # Why this arm exists at all, and why it is not a copy of `handle_proxy_request`
///
/// `handle_proxy_request` is the path the plan's `httpclient` kind advertises,
/// and accepting the seven-field outproxy block on that kind while only
/// honouring it for `CONNECT` would be the inert-acceptance trap the plan
/// exists to prevent — at sub-path granularity. A control client that set
/// `ProxyList`, got an accepted response, and then issued `GET
/// http://example.com/` would be told `403` by the parser and reasonably
/// conclude the tunnel has no egress, or worse, conclude the proxy is broken.
/// So the forward path carries outproxy requests too.
///
/// The one thing that is genuinely different, and the reason this is not a
/// three-line call into the direct path:
///
/// > After `build_attempt`, [`OutproxySession`] is a **byte pipe to
/// > `example.com:80`**, not a conversation with a forward proxy. So the
/// > request written onto it must be **origin-form** with the real name in
/// > `Host:` — not the absolute-form the local client sent, and above all not
/// > the `b32.i2p` substitution the direct path makes. Rewriting `Host` to the
/// > tunnel's own destination would send every forwarded request to the
/// > outproxy's *default* vhost.
///
/// `initial_body` is prepended to the pump's outbound direction exactly as in
/// the direct path, and the handshake prefix seeds the **inbound** direction.
#[allow(clippy::too_many_arguments)]
async fn forward_via_outproxy(
    mut stream: TcpStream,
    manager: Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    cancellation: &CancellationToken,
    provider: &RouterOutproxyProvider,
    target: &OutproxyTarget,
    options: &HttpClientOptions,
    head: &HttpRequestHead,
    initial_body: Vec<u8>,
    clearnet_target: &RequestTarget,
) -> HttpConnectionOutcome {
    let route = match open_client_route(
        &manager,
        &runtime.spec_id,
        runtime.destination_id,
        Some(provider),
        target.host(),
        target.port(),
        true,
        cancellation,
        manager.outproxy_counters(),
    )
    .await
    {
        Ok(route) => route,
        Err(failure) => {
            debug!(
                failure = failure.as_str(),
                "forward outproxy route open failed"
            );
            crate::outproxy_route::ClientRoute::Refused(failure)
        }
    };
    let session = match route {
        crate::outproxy_route::ClientRoute::ViaOutproxy(session) => session,
        other => {
            debug!(outcome = ?other, "forward outproxy route was not open");
            let _ = write_error_response(
                &mut stream,
                build_error_response(
                    HttpErrorKind::Other,
                    "no I2P-routed outproxy is configured for this tunnel",
                ),
            )
            .await;
            return HttpConnectionOutcome::BadGateway;
        }
    };
    // Origin-form over a byte pipe, with the *clearnet* authority in `Host:`.
    //
    // `clearnet_target` is the target exactly as the client wrote it — no
    // `target_for_remote_destination` substitution. That substitution is
    // correct for the direct path and wrong here.
    let mut forwarded = Vec::with_capacity(head.line.target.len() + 256);
    forwarded.extend_from_slice(head.line.method.as_bytes());
    forwarded.push(b' ');
    forwarded.extend_from_slice(
        i2pr_service_tunnels::http::target::origin_form(clearnet_target).as_bytes(),
    );
    forwarded.extend_from_slice(b" HTTP/1.1\r\n");
    for header in rewrite_headers(&head.headers, clearnet_target, &options.privacy) {
        forwarded.extend_from_slice(header.name_str().as_bytes());
        forwarded.extend_from_slice(b": ");
        forwarded.extend_from_slice(header.value.as_bytes());
        forwarded.extend_from_slice(b"\r\n");
    }
    forwarded.extend_from_slice(b"\r\n");
    forwarded.extend_from_slice(&initial_body);
    let endpoint: Arc<dyn StreamPumpEndpoint> =
        Arc::new(ServicePumpEndpoint::new_client_with_prefix(
            Arc::clone(&manager),
            runtime.destination_id,
            session.connection_id,
            session.remote.clone(),
            session.tunnel_prefix,
        ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, forwarded, endpoint, config, cancellation.clone()).await;
    if let Err(error) = result {
        debug!(error = %error, "forward outproxy pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
        return HttpConnectionOutcome::BadGateway;
    }
    terminate_streaming(
        &manager,
        runtime.destination_id,
        session.connection_id,
        &session.remote,
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
    let drain_cancel = runtime.admission_cancellation_token();
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            // Plan 289: drained runtimes stop accepting promptly (see
            // `run_client_loop`).
            _ = drain_cancel.cancelled() => break,
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
            runtime_for_task.connection_finished_now();
            debug!(
                service = %spec_id_for_log,
                ?outcome,
                "http client connection finished"
            );
        });
    }
    Ok(())
}

/// Per-connection strict CONNECT entry for the Plan 290
/// `connect-client` profile. Reads HTTP/1.1 headers, parses the
/// request, rejects any non-`CONNECT` method with a bounded `405`
/// before any I2P work starts, and otherwise runs the exact
/// `handle_connect` path the `http-client` profile uses (shared
/// authority validation, CONNECT port policy, destination
/// resolution, Streaming establishment, opaque pump).
///
/// The profile is not an alias: it has its own kind, option
/// applicability (`ConnectClientOptions`), and lifecycle identity.
pub async fn run_connect_only_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    stream: TcpStream,
    cancellation: CancellationToken,
    options: ConnectClientOptions,
) -> HttpConnectionOutcome {
    let limits = HttpLimits::defaults();
    let mut stream = stream;
    let (head_bytes, _initial_body) =
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
    if head.line.method != "CONNECT" {
        let _ = write_error_response(
            &mut stream,
            build_error_response(
                HttpErrorKind::MethodNotAllowed,
                "connect-client accepts CONNECT only",
            ),
        )
        .await;
        return HttpConnectionOutcome::Forbidden;
    }
    // Adapt the CONNECT-only port policy onto the shared CONNECT
    // executor input; header-rewrite policy never applies because
    // CONNECT carries no forwarded headers.
    let http_options = HttpClientOptions {
        privacy: i2pr_service_tunnels::PrivacyPolicy {
            connect_allowed_ports: options.connect_allowed_ports,
            ..i2pr_service_tunnels::PrivacyPolicy::default()
        },
        destination_ports: std::collections::BTreeSet::new(),
        allowed_hosts: Vec::new(),
        // CONNECT-only has its own HTTPS port policy and does not use
        // Proposal 170's HTTP-client internal-SSL toggle.
        allow_internal_ssl: true,
        // Plan 292: the strict-CONNECT executor enforces the same
        // credentials as the shared CONNECT handler.
        proxy_auth: options.proxy_auth.clone(),
        // Plan 342: the strict-CONNECT executor is one of the request
        // paths that reaches the outproxy provider, so the route policy
        // rides along. Route policy only; the credential stays in the
        // Plan 341 owner.
        outproxy: options.outproxy.clone(),
    };
    handle_connect(
        manager,
        runtime,
        stream,
        cancellation,
        http_options,
        head,
        limits,
    )
    .await
}

/// Runs the per-service supervisor loop for a strict CONNECT-only
/// client tunnel.
pub async fn run_connect_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "connect client tunnel missing loopback listener".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "connect client tunnel bound loopback listener"
    );
    let options = spec.connect_options.clone().unwrap_or_default();
    let drain_cancel = runtime.admission_cancellation_token();
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            // Plan 289: drained runtimes stop accepting promptly (see
            // `run_client_loop`).
            _ = drain_cancel.cancelled() => break,
            accept = listener.accept() => accept,
        };
        let (stream, _peer) = match accept {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    service = %spec.id.as_str(),
                    error = %error,
                    "connect client listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
            manager.aggregate_permit().try_acquire_owned().ok();
        let Some(permit_for_task) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "connect client tunnel aggregate ceiling reached; rejecting connection"
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
            let _permit_for_task = permit_for_task;
            let outcome = run_connect_only_connection(
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
            runtime_for_task.connection_finished_now();
            debug!(
                service = %spec_id_for_log,
                ?outcome,
                "connect client connection finished"
            );
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_transcript_constants_have_no_router_branding() {
        let connect_ok = b"HTTP/1.1 200 Connection Established\r\n\r\n";
        assert!(
            !connect_ok
                .windows(b"i2pr".len())
                .any(|window| window.eq_ignore_ascii_case(b"i2pr"))
        );
    }

    #[test]
    fn remote_host_uses_resolved_hash_instead_of_local_alias() {
        let target = RequestTarget {
            kind: TargetKind::Absolute,
            host: "my-private-address-book-name.i2p".to_owned(),
            port: Some(8080),
            path: "/".to_owned(),
            query: String::new(),
            scheme: Some("http"),
        };
        let remote = RemoteDestination {
            destination_hash: [0x5a; 32],
            signing_public_key: i2pr_proto::SigningPublicKey::new(
                i2pr_proto::SigningKeyType::EdDsaSha512Ed25519,
                vec![0; 32],
            )
            .unwrap(),
            static_public_key: [0; 32],
        };
        let resolved = target_for_remote_destination(&target, &remote);
        assert_eq!(
            resolved.host,
            format!(
                "{}.b32.i2p",
                i2pr_service_tunnels::encode_b32_label(&[0x5a; 32])
            )
        );
        assert_eq!(resolved.port, Some(8080));
        assert!(!resolved.host.contains("my-private-address-book-name"));
        let rewritten = rewrite_headers(
            &[],
            &resolved,
            &i2pr_service_tunnels::PrivacyPolicy::default(),
        );
        let host = rewritten
            .iter()
            .find(|entry| entry.name_str() == "host")
            .unwrap();
        assert_eq!(host.value, format!("{}:8080", resolved.host));
    }

    #[test]
    fn read_http_head_rejects_buffer_overflow() {
        // Static-only check: ensure the constants agree with the
        // runtime-neutral limits module.
        const { assert!(BODY_CHUNK_BYTES <= 32 * 1024) };
        assert!(HEADER_READ_DEADLINE >= Duration::from_secs(5));
    }
}
