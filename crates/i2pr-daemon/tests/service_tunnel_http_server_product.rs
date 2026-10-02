//! Plan 290 §httpserver black-box product tests.
//!
//! The filtered HTTP server terminates inbound I2P Streaming at a
//! bounded loopback target: origin-form enforcement, required
//! `Host` with replacement (spoof protection), hop-by-hop +
//! identifying strip, forced close, `Transfer-Encoding` rejection,
//! response privacy stripping with framing preservation. Tests
//! drive behavior only through TCP after listener startup, using a
//! paired generic client (opaque bytes in) and a scripted HTTP
//! fixture as the local target:
//!
//! - `GET` round-trip with digest equality, `Host` replacement,
//!   `Server`/`Via` stripping, forced `Connection: close`;
//! - absolute-form rejected with `400`;
//! - missing `Host` rejected with `400`;
//! - `POST` body paced with digest equality;
//! - chunked request rejected (never dechunked);
//! - overlong head rejected by the retained ceiling (slowloris
//!   bound) without a 200;
//! - restart preserves the persistent server identity;
//! - sibling connections are isolated.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, LocalListenerSpec, ServerTarget, ServiceTimeouts,
    ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

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

fn client_spec(destination: DestinationRef) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-client").expect("id"),
        kind: ServiceTunnelKind::GenericClient,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(destination),
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
    }
}

fn http_server_spec(target: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-web").expect("id"),
        kind: ServiceTunnelKind::HttpServer,
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
        connect_options: None,
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

/// One observed fixture request: raw head plus body bytes.
#[derive(Debug)]
struct ObservedRequest {
    head: String,
    body: Vec<u8>,
}

/// Scripted HTTP fixture: records each request (head + body) and
/// answers with the canned response. Accepts in a loop.
fn start_http_fixture(
    response: Vec<u8>,
) -> (
    SocketAddr,
    mpsc::UnboundedReceiver<ObservedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let addr = listener.local_addr().expect("addr");
    let listener = TcpListener::from_std(listener).expect("tokio listener");
    let (observed_tx, observed_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let observed = observed_tx.clone();
            let response = response.clone();
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                // Head.
                loop {
                    match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                        .await
                    {
                        Ok(Ok(0)) | Err(_) => return,
                        Ok(Ok(n)) => {
                            buffer.extend_from_slice(&chunk[..n]);
                            if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                            if buffer.len() > 128 * 1024 {
                                return;
                            }
                        }
                        Ok(Err(_)) => return,
                    }
                }
                let end = buffer
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .expect("terminator")
                    + 4;
                let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
                // Body by declared Content-Length (0 when absent).
                let content_length = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.trim().eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                let mut body = buffer[end..].to_vec();
                while body.len() < content_length {
                    match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                        .await
                    {
                        Ok(Ok(0)) | Err(_) => break,
                        Ok(Ok(n)) => body.extend_from_slice(&chunk[..n]),
                        Ok(Err(_)) => break,
                    }
                }
                body.truncate(content_length);
                let _ = observed.send(ObservedRequest { head, body });
                let _ = stream.write_all(&response).await;
            });
        }
    });
    (addr, observed_rx, task)
}

/// Builds a paired manager: generic client -> http-server ->
/// fixture. Returns the manager, the fixture observer, and the
/// fixture address (for `Host` expectations).
async fn build_paired_manager(
    data_dir: &Path,
    response: Vec<u8>,
) -> (
    Arc<ServiceTunnelManager>,
    mpsc::UnboundedReceiver<ObservedRequest>,
    SocketAddr,
) {
    let (fixture_addr, observed_rx, _fixture) = start_http_fixture(response);
    let probe = build_manager(
        data_dir,
        vec![http_server_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64("alpha-web")
        .expect("server b64");
    drop(probe);
    let destination = DestinationRef::ConfiguredDestination(server_b64);
    let manager = build_manager(
        data_dir,
        vec![client_spec(destination), http_server_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    (manager, observed_rx, fixture_addr)
}

async fn read_head_bounded(stream: &mut TcpStream) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let started = std::time::Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "timed out reading a head"
        );
        match tokio::time::timeout(Duration::from_secs(2), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                buffer.extend_from_slice(&chunk[..n]);
                if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            _ => break,
        }
    }
    buffer
}

async fn read_body_bounded(stream: &mut TcpStream, head: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(head);
    let content_length = text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    // Bytes already buffered past the head terminator belong to
    // the body.
    let end = head
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(head.len());
    let mut body = head[end..].to_vec();
    let mut chunk = [0_u8; 4096];
    let started = std::time::Instant::now();
    while body.len() < content_length {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "timed out reading a body"
        );
        match tokio::time::timeout(Duration::from_secs(2), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => body.extend_from_slice(&chunk[..n]),
            _ => break,
        }
    }
    body.truncate(content_length);
    body
}

async fn next_observed(rx: &mut mpsc::UnboundedReceiver<ObservedRequest>) -> ObservedRequest {
    tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("request arrives")
        .expect("channel open")
}

const CANNED_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nServer: secret/1.0\r\nVia: 1.1 evil\r\nContent-Length: 5\r\nConnection: keep-alive\r\n\r\nhello";

#[tokio::test(flavor = "current_thread")]
async fn http_server_get_roundtrip_filters() {
    let directory = temp_data_dir("http-server-get");
    let (manager, mut observed, fixture_addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET /path?q=1 HTTP/1.1\r\nHost: example.i2p\r\nVia: 1.1 browser\r\n\r\n")
        .await
        .expect("write GET");
    // Fixture observes the filtered request: Host replaced with
    // the loopback target, identifying headers stripped.
    let request = next_observed(&mut observed).await;
    assert!(
        request.head.starts_with("GET /path?q=1 HTTP/1.1\r\n"),
        "origin-form preserved: {}",
        request.head
    );
    assert!(
        request.head.contains(&format!("host: {fixture_addr}\r\n")),
        "Host replaced with the local target: {}",
        request.head
    );
    assert!(
        !request.head.contains("example.i2p"),
        "client Host never reaches the target: {}",
        request.head
    );
    assert!(
        !request.head.to_ascii_lowercase().contains("via:"),
        "Via stripped: {}",
        request.head
    );
    assert!(
        request.head.contains("connection: close\r\n"),
        "close forced: {}",
        request.head
    );
    // Client observes the filtered response: framing preserved,
    // Server/Via stripped, close forced.
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "status: {text}");
    assert!(text.contains("content-length: 5\r\n"), "framing: {text}");
    assert!(
        !text.to_ascii_lowercase().contains("server:"),
        "Server stripped: {text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("via:"),
        "Via stripped: {text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("keep-alive"),
        "keep-alive gone: {text}"
    );
    assert!(text.contains("connection: close\r\n"), "close: {text}");
    let body = read_body_bounded(&mut stream, &head).await;
    assert_eq!(body, b"hello");
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_absolute_form_is_400() {
    let directory = temp_data_dir("http-server-absolute");
    let (manager, _observed, _addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://example.i2p/path HTTP/1.1\r\nHost: example.i2p\r\n\r\n")
        .await
        .expect("write");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "absolute-form is a framing mismatch: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_missing_host_is_400() {
    let directory = temp_data_dir("http-server-nohost");
    let (manager, _observed, _addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET /path HTTP/1.1\r\nX-Custom: yes\r\n\r\n")
        .await
        .expect("write");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "missing Host is 400: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_post_body_is_paced_with_digest() {
    let directory = temp_data_dir("http-server-post");
    let (manager, mut observed, _addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    let body = vec![0x55_u8; 4096];
    let head = format!(
        "POST /submit HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.expect("head");
    stream.write_all(&body).await.expect("body");
    let request = next_observed(&mut observed).await;
    assert_eq!(request.body, body, "POST body arrives byte-exact");
    let response_head = read_head_bounded(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&response_head).starts_with("HTTP/1.1 200 OK\r\n"),
        "response follows the body"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_chunked_request_is_rejected() {
    let directory = temp_data_dir("http-server-chunked");
    let (manager, _observed, _addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"POST /submit HTTP/1.1\r\nHost: example.i2p\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n")
        .await
        .expect("write");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "chunked is never dechunked: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_overlong_head_is_rejected_by_ceiling() {
    let directory = temp_data_dir("http-server-overlong");
    let (manager, _observed, _addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    // 70 KiB of header bytes with no terminator: exceeds the
    // 64 KiB retained ceiling (slowloris bound) without waiting
    // for the 30 s deadline.
    let filler = vec![b'x'; 70 * 1024];
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: example.i2p\r\nX-Filler: ")
        .await
        .expect("head");
    stream.write_all(&filler).await.expect("filler");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        !text.starts_with("HTTP/1.1 200"),
        "overlong head never reaches the target: {text:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_restart_preserves_identity() {
    let directory = temp_data_dir("http-server-identity");
    let (fixture_addr, _observed, _fixture) = start_http_fixture(CANNED_RESPONSE.to_vec());
    let first = build_manager(
        directory.path(),
        vec![http_server_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    first.prepare().await.expect("prepare");
    let before = first
        .service_destination_b64("alpha-web")
        .expect("server b64");
    assert!(!before.is_empty());
    drop(first);
    let second = build_manager(
        directory.path(),
        vec![http_server_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    second.prepare().await.expect("prepare");
    let after = second
        .service_destination_b64("alpha-web")
        .expect("server b64");
    assert_eq!(before, after, "no identity churn across restart");
}

#[tokio::test(flavor = "current_thread")]
async fn http_server_siblings_are_isolated() {
    let directory = temp_data_dir("http-server-siblings");
    let (manager, _observed, _addr) =
        build_paired_manager(directory.path(), CANNED_RESPONSE.to_vec()).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut first = TcpStream::connect(listener).await.expect("first");
    let mut second = TcpStream::connect(listener).await.expect("second");
    first
        .write_all(b"GET /one HTTP/1.1\r\nHost: example.i2p\r\n\r\n")
        .await
        .expect("write");
    second
        .write_all(b"GET /two HTTP/1.1\r\nHost: example.i2p\r\n\r\n")
        .await
        .expect("write");
    for stream in [&mut first, &mut second] {
        let head = read_head_bounded(stream).await;
        let text = String::from_utf8_lossy(&head);
        assert!(
            text.starts_with("HTTP/1.1 200 OK\r\n"),
            "both serve: {text}"
        );
        let body = read_body_bounded(stream, &head).await;
        assert_eq!(body, b"hello");
    }
}
