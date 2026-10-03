//! Plan 290 §httpbidirserver black-box product tests.
//!
//! The deprecated bidirectional HTTP server composes the filtered
//! HTTP server half and the no-outproxy HTTP client half under one
//! lifecycle generation and one persistent public server identity.
//! Tests drive behavior only through TCP after listener startup:
//!
//! - server half round-trip through the bidir loop with the server
//!   privacy filter (origin-form, `Host` replacement, identifying
//!   strip, forced close, framing preservation);
//! - client half proxies absolute-form requests to a
//!   locally-resolved alias destination (no outproxy needed);
//! - client half rejects clearnet authorities with `403`;
//! - restart preserves the single persistent server identity;
//! - sibling bidir services are isolated;
//! - server-half target failure increments the failure counter and
//!   releases the active slot (no leak, no hang).

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpClientOptions, LocalListenerSpec, ServerTarget,
    ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec,
    StaticAliasTable,
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

fn base_spec(id: &str, kind: ServiceTunnelKind) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind,
        enabled: true,
        listener: None,
        target: None,
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

fn bidir_spec(target: SocketAddr) -> ServiceTunnelSpec {
    let mut spec = base_spec("alpha-bidir", ServiceTunnelKind::HttpBidirServer);
    spec.listener = Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener"));
    spec.target = Some(ServerTarget::LoopbackTcp(target));
    spec.http_options = Some(HttpClientOptions::default());
    spec
}

fn client_spec(destination: DestinationRef) -> ServiceTunnelSpec {
    let mut spec = base_spec("alpha-client", ServiceTunnelKind::GenericClient);
    spec.listener = Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener"));
    spec.destination = Some(destination);
    spec
}

fn generic_server_spec(id: &str, target: SocketAddr) -> ServiceTunnelSpec {
    let mut spec = base_spec(id, ServiceTunnelKind::GenericServer);
    spec.target = Some(ServerTarget::LoopbackTcp(target));
    spec
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

/// Builds a paired manager: generic client -> bidir server half ->
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
    // The bidir server half publishes its own persistent identity;
    // the paired client addresses it by configured destination.
    let bidir_probe = build_manager(
        data_dir,
        vec![bidir_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    bidir_probe.prepare().await.expect("bidir probe prepare");
    let bidir_b64 = bidir_probe
        .service_destination_b64("alpha-bidir")
        .expect("bidir b64");
    drop(bidir_probe);
    let destination = DestinationRef::ConfiguredDestination(bidir_b64);
    let manager = build_manager(
        data_dir,
        vec![client_spec(destination), bidir_spec(fixture_addr)],
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
            _ => panic!("read failed while reading a head"),
        }
    }
    buffer
}

async fn read_body_bounded(stream: &mut TcpStream, head: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(head).into_owned();
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
    let end = head
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("head terminator");
    // Same-read bytes after the head belong to the body.
    let mut body = head[end..].to_vec();
    let mut chunk = [0_u8; 1024];
    let started = std::time::Instant::now();
    while body.len() < content_length {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "timed out reading a body"
        );
        match tokio::time::timeout(Duration::from_secs(2), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => body.extend_from_slice(&chunk[..n]),
            _ => panic!("read failed while reading a body"),
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
async fn http_bidir_server_half_roundtrip_filters() {
    let directory = temp_data_dir("http-bidi-server-half");
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
    // The bidir server half applies the server privacy contract:
    // origin-form preserved, Host replaced with the loopback
    // target, identifying headers stripped, close forced.
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
    assert!(
        request.body.is_empty(),
        "GET carries no body bytes to the target"
    );
    // Client observes the filtered response with framing intact.
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "status: {text}");
    assert!(text.contains("content-length: 5\r\n"), "framing: {text}");
    assert!(
        !text.to_ascii_lowercase().contains("server:"),
        "Server stripped: {text}"
    );
    let body = read_body_bounded(&mut stream, &head).await;
    assert_eq!(body, b"hello");
}

#[tokio::test(flavor = "current_thread")]
async fn http_bidir_client_half_proxies_to_local_alias() {
    let directory = temp_data_dir("http-bidi-client-half");
    let (fixture_addr, mut observed, _fixture) = start_http_fixture(CANNED_RESPONSE.to_vec());
    // Probe the generic server destination, then build one manager
    // holding the bidir service plus the alias-resolved server.
    let probe = build_manager(
        directory.path(),
        vec![generic_server_spec("alpha-origin", fixture_addr)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64("alpha-origin")
        .expect("origin b64");
    drop(probe);
    let mut aliases = StaticAliasTable::new();
    aliases
        .insert("web.i2p", DestinationRef::ConfiguredDestination(server_b64))
        .expect("alias inserts");
    let manager = build_manager(
        directory.path(),
        vec![
            bidir_spec(fixture_addr),
            generic_server_spec("alpha-origin", fixture_addr),
        ],
        aliases,
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-bidir")
        .expect("bidir listener");
    // The client half requires absolute-form and resolves the alias
    // locally; no outproxy is involved.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://web.i2p/ HTTP/1.1\r\nHost: web.i2p\r\n\r\n")
        .await
        .expect("write proxy GET");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "status: {text}");
    let body = read_body_bounded(&mut stream, &head).await;
    assert_eq!(body, b"hello", "proxied body arrives intact");
    // The origin observed an origin-form request: the proxy
    // rewrote the absolute-form target before forwarding.
    let request = next_observed(&mut observed).await;
    assert!(
        request.head.starts_with("GET / HTTP/1.1\r\n"),
        "absolute-form rewritten to origin-form: {}",
        request.head
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_bidir_client_half_rejects_clearnet() {
    let directory = temp_data_dir("http-bidi-clearnet");
    let (fixture_addr, _observed, _fixture) = start_http_fixture(CANNED_RESPONSE.to_vec());
    let manager = build_manager(
        directory.path(),
        vec![bidir_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-bidir")
        .expect("bidir listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
        .await
        .expect("write clearnet GET");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "clearnet has no outproxy escape: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_bidir_restart_preserves_single_identity() {
    let directory = temp_data_dir("http-bidi-identity");
    let (fixture_addr, _observed, _fixture) = start_http_fixture(CANNED_RESPONSE.to_vec());
    let first = build_manager(
        directory.path(),
        vec![bidir_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    first.prepare().await.expect("prepare");
    let before = first
        .service_destination_b64("alpha-bidir")
        .expect("bidir b64");
    assert!(!before.is_empty());
    drop(first);
    let second = build_manager(
        directory.path(),
        vec![bidir_spec(fixture_addr)],
        StaticAliasTable::new(),
    );
    second.prepare().await.expect("prepare");
    let after = second
        .service_destination_b64("alpha-bidir")
        .expect("bidir b64");
    assert_eq!(before, after, "one persistent identity across restart");
}

#[tokio::test(flavor = "current_thread")]
async fn http_bidir_siblings_are_isolated() {
    let directory = temp_data_dir("http-bidi-siblings");
    let (first_addr, _first_observed, _first) = start_http_fixture(CANNED_RESPONSE.to_vec());
    let (second_addr, _second_observed, _second) = start_http_fixture(CANNED_RESPONSE.to_vec());
    let mut first_spec = bidir_spec(first_addr);
    first_spec.id = ServiceTunnelId::parse("bidir-one").expect("id");
    let mut second_spec = bidir_spec(second_addr);
    second_spec.id = ServiceTunnelId::parse("bidir-two").expect("id");
    // Each sibling needs its own paired client through its own
    // server half; reuse the paired-manager trick per sibling.
    let probe = build_manager(
        directory.path(),
        vec![first_spec.clone(), second_spec.clone()],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let one_b64 = probe.service_destination_b64("bidir-one").expect("one b64");
    let two_b64 = probe.service_destination_b64("bidir-two").expect("two b64");
    assert_ne!(one_b64, two_b64, "siblings own distinct identities");
    drop(probe);
    let mut first_client = client_spec(DestinationRef::ConfiguredDestination(one_b64));
    first_client.id = ServiceTunnelId::parse("client-one").expect("id");
    let mut second_client = client_spec(DestinationRef::ConfiguredDestination(two_b64));
    second_client.id = ServiceTunnelId::parse("client-two").expect("id");
    let manager = build_manager(
        directory.path(),
        vec![first_spec, second_spec, first_client, second_client],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    for (service, path) in [("client-one", "/one"), ("client-two", "/two")] {
        let listener = manager.client_listener_address(service).expect("listener");
        let mut stream = TcpStream::connect(listener).await.expect("connect");
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: example.i2p\r\n\r\n").as_bytes())
            .await
            .expect("write");
        let head = read_head_bounded(&mut stream).await;
        let text = String::from_utf8_lossy(&head);
        assert!(
            text.starts_with("HTTP/1.1 200 OK\r\n"),
            "{service} serves: {text}"
        );
        let body = read_body_bounded(&mut stream, &head).await;
        assert_eq!(body, b"hello", "{service} body intact");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn http_bidir_server_half_target_failure_releases_slot() {
    let directory = temp_data_dir("http-bidi-target-failure");
    // A bound-then-dropped loopback port is refused deterministically.
    let dead = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr");
    // Rebuild with the bidir server half pointed at the dead port.
    let probe = build_manager(
        directory.path(),
        vec![bidir_spec(dead)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let bidir_b64 = probe
        .service_destination_b64("alpha-bidir")
        .expect("bidir b64");
    drop(probe);
    let manager = build_manager(
        directory.path(),
        vec![
            client_spec(DestinationRef::ConfiguredDestination(bidir_b64)),
            bidir_spec(dead),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET /down HTTP/1.1\r\nHost: example.i2p\r\n\r\n")
        .await
        .expect("write GET");
    // The failed target connect surfaces through the failure
    // counter and the active slot is released: no leak, no hang.
    let started = std::time::Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "target failure surfaces promptly"
        );
        if manager.failed_connects("alpha-bidir") > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        manager.active_connections("alpha-bidir"),
        0,
        "no slot leak after target failure"
    );
    drop(stream);
    drop(manager);
}
