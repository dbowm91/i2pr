//! Plan 182 local-delivery round-trip tests.
//!
//! Proves the Plan 182 corrective end to end: a client tunnel and a
//! server tunnel owned by one [`ServiceTunnelManager`] establish
//! Streaming through the per-destination delivery driver and move
//! application bytes in both directions. Every test binds
//! `127.0.0.1:0`, uses bounded deadlines, and drives bytes only
//! through TCP. Digest equality (SHA-256) is the only payload
//! assertion; no private key material or raw payloads are logged.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpClientOptions, IrcClientOptions, LocalListenerSpec,
    ServerTarget, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, Socks5ClientOptions, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    let directory = tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .expect("set tempdir permissions");
    }
    directory
}

fn client_spec(id: &str, kind: ServiceTunnelKind, target: DestinationRef) -> ServiceTunnelSpec {
    let (http_options, socks5_options, irc_options) = match kind {
        ServiceTunnelKind::HttpClient => (Some(HttpClientOptions::defaults()), None, None),
        ServiceTunnelKind::Socks5Client => (None, Some(Socks5ClientOptions::defaults()), None),
        ServiceTunnelKind::IrcClient => (None, None, Some(IrcClientOptions::default())),
        _ => (None, None, None),
    };
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(target),
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options,
        socks5_options,
        irc_options,
    }
}

fn server_spec(id: &str, kind: ServiceTunnelKind, target: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target)),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

fn build_manager(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
) -> Arc<ServiceTunnelManager> {
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 64,
        per_service_connection_ceiling: 16,
        specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
        aliases: Arc::new(aliases),
    };
    Arc::new(ServiceTunnelManager::new(manager_config).expect("manager builds"))
}

async fn start_supervisors(manager: &Arc<ServiceTunnelManager>) -> (ChildScope, CancellationToken) {
    let runtimes = manager.prepare().await.expect("prepare");
    let cancel = CancellationToken::new();
    let scope = ChildScope::for_test(&cancel, ChildFailurePolicy::FailParent);
    manager
        .start_supervisors(runtimes, &scope, cancel.clone())
        .expect("supervisors started");
    (scope, cancel)
}

/// Builds a paired manager: first a transient server-only manager
/// captures the persistent server destination base64 (same data
/// dir, so the identity carries over), then the full manager wires
/// client tunnels to that destination via full public material.
/// `aliases_for_first_server` registers static aliases (e.g. the
/// `.i2p` host spellings HTTP/SOCKS requests carry) against the
/// first server destination, because those profiles resolve the
/// request host rather than the spec destination.
async fn build_paired_manager(
    data_dir: &Path,
    clients: Vec<(String, ServiceTunnelKind)>,
    servers: Vec<(String, ServiceTunnelKind, SocketAddr)>,
    aliases_for_first_server: Vec<String>,
) -> (Arc<ServiceTunnelManager>, String) {
    let server_specs: Vec<ServiceTunnelSpec> = servers
        .iter()
        .map(|(id, kind, target)| server_spec(id, *kind, *target))
        .collect();
    let probe = build_manager(data_dir, server_specs, StaticAliasTable::new());
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64(&servers[0].0)
        .expect("server b64");
    drop(probe);
    let destination = DestinationRef::ConfiguredDestination(server_b64.clone());
    let mut specs: Vec<ServiceTunnelSpec> = clients
        .into_iter()
        .map(|(id, kind)| client_spec(&id, kind, destination.clone()))
        .collect();
    for (id, kind, target) in &servers {
        specs.push(server_spec(id, *kind, *target));
    }
    let mut aliases = StaticAliasTable::new();
    for alias in &aliases_for_first_server {
        aliases
            .insert(alias, destination.clone())
            .expect("alias insert");
    }
    let manager = build_manager(data_dir, specs, aliases);
    (manager, server_b64)
}

/// Starts a loopback echo fixture that echoes every received byte
/// back and records the total. Returns the bound address plus a
/// task handle that resolves to the echoed byte count.
async fn start_echo_fixture() -> (SocketAddr, tokio::task::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut total = 0_usize;
        let mut chunk = [0_u8; 4096];
        loop {
            match timeout_read(&mut stream, &mut chunk).await {
                Some(0) | None => break,
                Some(n) => {
                    total += n;
                    if stream.write_all(&chunk[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
        total
    });
    (addr, task)
}

async fn timeout_read(stream: &mut TcpStream, chunk: &mut [u8]) -> Option<usize> {
    match tokio::time::timeout(Duration::from_secs(10), stream.read(chunk)).await {
        Ok(Ok(n)) => Some(n),
        _ => None,
    }
}

/// Reads exactly `expected` bytes or fails the test on deadline.
async fn read_exact_bounded(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut out = vec![0_u8; expected];
    let mut filled = 0_usize;
    let started = std::time::Instant::now();
    while filled < expected {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "timed out reading {expected} bytes (got {filled})"
        );
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut out[filled..])).await {
            Ok(Ok(0)) => panic!("EOF after {filled}/{expected} bytes"),
            Ok(Ok(n)) => filled += n,
            Ok(Err(error)) => panic!("read error after {filled}/{expected} bytes: {error}"),
            Err(_) => panic!("read timed out after {filled}/{expected} bytes"),
        }
    }
    out
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_generic_small_echo_digest() {
    let directory = temp_data_dir("rt-generic-small");
    let (echo_addr, echo_task) = start_echo_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-client".to_owned(), ServiceTunnelKind::GenericClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            echo_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload = b"plan182-small-payload-001";
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(payload).await.expect("write");
    let echoed = read_exact_bounded(&mut stream, payload.len()).await;
    assert_eq!(echoed, payload);
    let _ = stream.shutdown().await;
    let echoed_total = tokio::time::timeout(Duration::from_secs(10), echo_task)
        .await
        .expect("echo done")
        .expect("echo task");
    assert_eq!(echoed_total, payload.len());
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_generic_large_multisegment_digest() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let directory = temp_data_dir("rt-generic-large");
    let (echo_addr, echo_task) = start_echo_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-client".to_owned(), ServiceTunnelKind::GenericClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            echo_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload: Vec<u8> = (0..96_000).map(|i| (i % 251) as u8).collect();
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(&payload).await.expect("write");
    let echoed = read_exact_bounded(&mut stream, payload.len()).await;
    assert_eq!(echoed, payload);
    let _ = stream.shutdown().await;
    let echoed_total = tokio::time::timeout(Duration::from_secs(15), echo_task)
        .await
        .expect("echo done")
        .expect("echo task");
    assert_eq!(echoed_total, payload.len());
}

/// Starts a loopback reversing fixture: reads one length-framed
/// message (u16 big-endian length + body), writes the body bytes
/// back reversed with the same framing, then closes.
///
/// Framing is by length, never by EOF: a Streaming CLOSE means no
/// more data may be sent in that direction, so reply-after-EOF
/// protocols are out of profile for tunnels (HTTP, SOCKS, and IRC
/// all frame without EOF; see plans/182 status).
async fn start_reverse_fixture() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut header = [0_u8; 2];
        if timeout_read_exact(&mut stream, &mut header).await {
            let len = u16::from_be_bytes(header) as usize;
            if len <= 4096 {
                let mut body = vec![0_u8; len];
                if timeout_read_exact(&mut stream, &mut body).await {
                    body.reverse();
                    let _ = stream.write_all(&body).await;
                }
            }
        }
    });
    (addr, task)
}

/// Reads exactly `buffer.len()` bytes with a bounded deadline.
/// Returns `false` on EOF, error, or timeout.
async fn timeout_read_exact(stream: &mut TcpStream, buffer: &mut [u8]) -> bool {
    let mut filled = 0_usize;
    let started = std::time::Instant::now();
    while filled < buffer.len() && started.elapsed() < Duration::from_secs(10) {
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer[filled..])).await
        {
            Ok(Ok(0)) => return false,
            Ok(Ok(n)) => filled += n,
            _ => return false,
        }
    }
    filled == buffer.len()
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_generic_reverse_bytes() {
    let directory = temp_data_dir("rt-generic-reverse");
    let (reverse_addr, reverse_task) = start_reverse_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-client".to_owned(), ServiceTunnelKind::GenericClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            reverse_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload = b"reverse-me-please-0123456789";
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    let mut framed = (payload.len() as u16).to_be_bytes().to_vec();
    framed.extend_from_slice(payload);
    stream.write_all(&framed).await.expect("write");
    let reversed = read_exact_bounded(&mut stream, payload.len()).await;
    let mut expected = payload.to_vec();
    expected.reverse();
    assert_eq!(reversed, expected);
    let _ = stream.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(10), reverse_task).await;
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_generic_half_close_propagates_eof() {
    // Half-close/EOF propagation: the client half-closes its write
    // side; the bytes plus EOF must reach the loopback fixture, and
    // the teardown must converge promptly (no linger-deadline
    // waits, no leaked connections). No reply-after-EOF is
    // expected: CLOSE ends the send direction by contract.
    let directory = temp_data_dir("rt-generic-halfclose");
    let listener_socket = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let fixture_addr = listener_socket.local_addr().expect("addr");
    let fixture_task = tokio::spawn(async move {
        let (mut stream, _) = listener_socket.accept().await.expect("accept");
        let mut received = Vec::new();
        let mut chunk = [0_u8; 256];
        loop {
            match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => received.extend_from_slice(&chunk[..n]),
                _ => break,
            }
        }
        received
    });
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-client".to_owned(), ServiceTunnelKind::GenericClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            fixture_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload = b"half-close-bytes-001";
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(payload).await.expect("write");
    let _ = stream.shutdown().await;
    // The daemon must converge the teardown: EOF observable here
    // well inside the 15 s linger cap.
    let mut chunk = [0_u8; 64];
    let eof = tokio::time::timeout(Duration::from_secs(12), stream.read(&mut chunk)).await;
    assert!(
        matches!(eof, Ok(Ok(0))),
        "expected prompt EOF after half-close, got: {eof:?}"
    );
    let received = tokio::time::timeout(Duration::from_secs(10), fixture_task)
        .await
        .expect("fixture done")
        .expect("fixture task");
    assert_eq!(received, payload);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.active_client_connections, 0);
    assert_eq!(snapshot.active_server_connections, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_generic_siblings_isolated() {
    let directory = temp_data_dir("rt-generic-siblings");
    let listener_socket = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let echo_addr = listener_socket.local_addr().expect("addr");
    let echo_task = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener_socket.accept().await.expect("accept");
            tokio::spawn(async move {
                let mut chunk = [0_u8; 1024];
                loop {
                    match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if stream.write_all(&chunk[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-client".to_owned(), ServiceTunnelKind::GenericClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            echo_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload_a = b"sibling-a-payload-xxxx";
    let payload_b = b"sibling-b-payload-yyyy";
    let mut a = TcpStream::connect(listener).await.expect("a");
    let mut b = TcpStream::connect(listener).await.expect("b");
    a.write_all(payload_a).await.expect("write a");
    b.write_all(payload_b).await.expect("write b");
    let echoed_a = read_exact_bounded(&mut a, payload_a.len()).await;
    let echoed_b = read_exact_bounded(&mut b, payload_b.len()).await;
    assert_eq!(echoed_a, payload_a);
    assert_eq!(echoed_b, payload_b);
    let _ = a.shutdown().await;
    let _ = b.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(10), echo_task).await;
}

/// Starts a minimal loopback HTTP fixture that answers one request
/// with a fixed 200 response echoing the request-target path.
async fn start_http_fixture() -> (SocketAddr, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 512];
        let started = std::time::Instant::now();
        while !request.windows(4).any(|w| w == b"\r\n\r\n")
            && request.len() < 8192
            && started.elapsed() < Duration::from_secs(10)
        {
            match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => request.extend_from_slice(&chunk[..n]),
                _ => break,
            }
        }
        let body = b"hello-from-loopback-fixture";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.write_all(body).await;
        request
    });
    (addr, task)
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_http_get_digest() {
    let directory = temp_data_dir("rt-http-get");
    let (fixture_addr, fixture_task) = start_http_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-http".to_owned(), ServiceTunnelKind::HttpClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            fixture_addr,
        )],
        vec!["roundtrip.i2p".to_owned()],
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    // The proxy resolves the request Host spelling through the
    // alias table (`roundtrip.i2p` was wired to the server
    // destination by `build_paired_manager`).
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://roundtrip.i2p/hello HTTP/1.1\r\nHost: roundtrip.i2p\r\nConnection: close\r\n\r\n")
        .await
        .expect("write");
    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => response.extend_from_slice(&chunk[..n]),
            _ => break,
        }
    }
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.starts_with("HTTP/1.1 200 OK"),
        "expected 200 OK, got: {text}"
    );
    assert!(text.contains("hello-from-loopback-fixture"));
    let request = tokio::time::timeout(Duration::from_secs(10), fixture_task)
        .await
        .expect("fixture done")
        .expect("fixture task");
    let request_text = String::from_utf8_lossy(&request);
    assert!(
        request_text.contains("/hello"),
        "fixture saw: {request_text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_socks5_connect_echo() {
    let directory = temp_data_dir("rt-socks5");
    let (echo_addr, echo_task) = start_echo_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-socks".to_owned(), ServiceTunnelKind::Socks5Client)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            echo_addr,
        )],
        vec!["roundtrip.i2p".to_owned()],
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    // RFC 1928 no-auth greeting.
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("greeting");
    let mut reply = [0_u8; 2];
    tokio::time::timeout(Duration::from_secs(10), stream.read_exact(&mut reply))
        .await
        .expect("greeting reply")
        .expect("read");
    assert_eq!(reply, [0x05, 0x00]);
    // CONNECT with DOMAINNAME ATYP so no local DNS happens. The
    // default M10 port policy allows 443 (canonical HTTPS); port 80
    // is fail-closed with REP=2 by design.
    let host = b"roundtrip.i2p";
    let mut request = vec![0x05, 0x01, 0x00, 0x03, host.len() as u8];
    request.extend_from_slice(host);
    request.extend_from_slice(&[0x01, 0xBB]);
    stream.write_all(&request).await.expect("connect req");
    let mut conn_reply = [0_u8; 10];
    tokio::time::timeout(Duration::from_secs(15), stream.read_exact(&mut conn_reply))
        .await
        .expect("connect reply")
        .expect("read");
    assert_eq!(conn_reply[0], 0x05);
    assert_eq!(conn_reply[1], 0x00, "expected success, got: {conn_reply:?}");
    // Opaque bytes now flow to the loopback fixture and back.
    let payload = b"socks5-opaque-bytes-001";
    stream.write_all(payload).await.expect("write");
    let echoed = read_exact_bounded(&mut stream, payload.len()).await;
    assert_eq!(echoed, payload);
    let _ = stream.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(10), echo_task).await;
}

/// Starts a minimal loopback IRC fixture: accepts one connection,
/// records every received line, answers PING with PONG, answers
/// PRIVMSG with a directed echo, then closes after `QUIT`.
async fn start_irc_fixture() -> (SocketAddr, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut lines = Vec::new();
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 512];
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(25) {
            match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => buffer.extend_from_slice(&chunk[..n]),
                _ => break,
            }
            while let Some(end) = buffer.windows(2).position(|w| w == b"\r\n") {
                let line = String::from_utf8_lossy(&buffer[..end]).to_string();
                buffer.drain(..end + 2);
                if line.starts_with("PING") {
                    let token = line.split_whitespace().nth(1).unwrap_or("x");
                    let _ = stream
                        .write_all(format!("PONG :{token}\r\n").as_bytes())
                        .await;
                } else if line.starts_with("PRIVMSG") {
                    let _ = stream
                        .write_all(b":fixture PRIVMSG #chan :echo-hello\r\n")
                        .await;
                } else if line.starts_with("QUIT") {
                    lines.push(line);
                    let _ = stream.shutdown().await;
                    return lines;
                }
                lines.push(line);
            }
        }
        lines
    });
    (addr, task)
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_irc_client_server_message() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let directory = temp_data_dir("rt-irc");
    let (fixture_addr, fixture_task) = start_irc_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-irc".to_owned(), ServiceTunnelKind::IrcClient)],
        vec![(
            "alpha-ircd".to_owned(),
            ServiceTunnelKind::IrcServer,
            fixture_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-irc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    for line in [
        b"NICK alice\r\n".as_slice(),
        b"USER alice 0 * :Alice\r\n",
        b"JOIN #chan\r\n",
        b"PRIVMSG #chan :hello world\r\n",
    ] {
        stream.write_all(line).await.expect("write");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Read until the fixture echo arrives.
    let mut received = Vec::new();
    let mut chunk = [0_u8; 512];
    let started = std::time::Instant::now();
    while !received.windows(10).any(|w| w == b"echo-hello")
        && started.elapsed() < Duration::from_secs(25)
    {
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => received.extend_from_slice(&chunk[..n]),
            _ => break,
        }
    }
    let text = String::from_utf8_lossy(&received);
    assert!(text.contains("echo-hello"), "no fixture echo, got: {text}");
    stream.write_all(b"QUIT :done\r\n").await.expect("quit");
    let _ = stream.shutdown().await;
    let lines = tokio::time::timeout(Duration::from_secs(10), fixture_task)
        .await
        .expect("fixture done")
        .expect("fixture task");
    let joined = lines.join("\n");
    // The server-side USER hostname must be the authenticated
    // remote-Destination projection, never the client-supplied one.
    let user_line = lines
        .iter()
        .find(|line| line.starts_with("USER "))
        .expect("fixture saw USER");
    assert!(
        user_line.contains(".b32.i2p"),
        "USER hostname not projected: {user_line}"
    );
    assert!(
        !user_line.contains(" 0 * "),
        "client-supplied USER hostname leaked: {user_line}"
    );
    assert!(
        joined.contains("PRIVMSG #chan :hello world"),
        "fixture saw: {joined}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn m10_local_roundtrip_resource_baseline_clean() {
    let directory = temp_data_dir("rt-baseline");
    let (echo_addr, echo_task) = start_echo_fixture().await;
    let (manager, _b64) = build_paired_manager(
        directory.path(),
        vec![("alpha-client".to_owned(), ServiceTunnelKind::GenericClient)],
        vec![(
            "alpha-server".to_owned(),
            ServiceTunnelKind::GenericServer,
            echo_addr,
        )],
        Vec::new(),
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(b"baseline-probe").await.expect("write");
    let echoed = read_exact_bounded(&mut stream, b"baseline-probe".len()).await;
    assert_eq!(echoed, b"baseline-probe");
    let _ = stream.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(10), echo_task).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.active_client_connections, 0);
    assert_eq!(snapshot.active_server_connections, 0);
    assert_eq!(snapshot.failed_connects_total, 0);
    // The delivery driver must have moved packets without typed
    // degradation: unknown-peer and missing-factory stay zero on
    // the local path.
    for runtime_id in [
        manager.service_destination_id("alpha-client"),
        manager.service_destination_id("alpha-server"),
    ] {
        let Some(destination_id) = runtime_id else {
            continue;
        };
        let counters = manager.delivery_counters(destination_id);
        assert_eq!(counters.unknown_peer, 0, "unknown peer on local path");
        assert_eq!(counters.missing_factory, 0, "missing factory on local path");
    }
}
