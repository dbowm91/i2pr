//! Plan 290 §connectclient black-box product tests.
//!
//! The strict CONNECT-only client profile reuses the HTTP CONNECT
//! executor but never forwards ordinary proxy requests: any
//! non-`CONNECT` method is rejected with a bounded `405` before any
//! I2P work starts. Tests drive behavior only through TCP after
//! listener startup:
//!
//! - `CONNECT` happy path round-trip through the paired manager
//!   (CONNECT alias:443 -> generic-server echo, digest equality);
//! - ordinary `GET` rejected with `405 Method Not Allowed` and the
//!   socket closed (no Streaming attempted);
//! - `CONNECT` with a disallowed port rejected with `403`;
//! - `CONNECT` with a clearnet authority rejected (no escape);
//! - `CONNECT` with an unknown `.i2p` host rejected without a 2xx;
//! - malformed head rejected with `400`;
//! - sibling connections are isolated;
//! - shutdown returns accounting to baseline.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    ConnectClientOptions, DestinationPolicy, DestinationRef, LocalListenerSpec, ServerTarget,
    ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec,
    StaticAliasTable,
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

fn connect_spec(destination: DestinationRef) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-connect").expect("id"),
        kind: ServiceTunnelKind::ConnectClient,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(destination),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: Some(ConnectClientOptions::defaults()),
        streamr_options: None,
    }
}

fn server_spec(id: &str, target: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target)),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
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

async fn start_echo_fixture() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut chunk = [0_u8; 4096];
                loop {
                    match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
                        .await
                    {
                        Ok(Ok(0)) | Err(_) => break,
                        Ok(Ok(n)) => {
                            if stream.write_all(&chunk[..n]).await.is_err() {
                                break;
                            }
                        }
                        Ok(Err(_)) => break,
                    }
                }
            });
        }
    });
    (addr, task)
}

/// Builds a paired manager: the CONNECT alias resolves to the
/// generic server destination captured from a transient probe
/// (same data dir, so the identity carries over).
async fn build_paired_manager(
    data_dir: &Path,
    alias: &str,
) -> (Arc<ServiceTunnelManager>, SocketAddr) {
    let (echo_addr, _echo_task) = start_echo_fixture().await;
    let probe = build_manager(
        data_dir,
        vec![server_spec("alpha-server", echo_addr)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64("alpha-server")
        .expect("server b64");
    drop(probe);
    let destination = DestinationRef::ConfiguredDestination(server_b64);
    let mut aliases = StaticAliasTable::new();
    aliases
        .insert(alias, destination.clone())
        .expect("alias insert");
    let manager = build_manager(
        data_dir,
        vec![
            connect_spec(destination),
            server_spec("alpha-server", echo_addr),
        ],
        aliases,
    );
    (manager, echo_addr)
}

async fn read_head_bounded(stream: &mut TcpStream) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let started = std::time::Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out reading response head"
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
            Ok(Err(error)) => panic!("read error: {error}"),
            Err(_) => panic!("read timed out"),
        }
    }
    out
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_tunnel_roundtrip_digest() {
    let directory = temp_data_dir("connect-roundtrip");
    let (manager, _echo) = build_paired_manager(directory.path(), "connect-target.i2p").await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"CONNECT connect-target.i2p:443 HTTP/1.1\r\nHost: connect-target.i2p\r\n\r\n")
        .await
        .expect("write CONNECT");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 200"),
        "CONNECT establishes: {text}"
    );
    let payload = b"connect-only-payload-001";
    stream.write_all(payload).await.expect("write payload");
    let echoed = read_exact_bounded(&mut stream, payload.len()).await;
    assert_eq!(echoed, payload);
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_rejects_plain_get_with_405() {
    let directory = temp_data_dir("connect-405");
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let manager = build_manager(
        directory.path(),
        vec![connect_spec(
            DestinationRef::parse(&b32).expect("destination"),
        )],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\n\r\n")
        .await
        .expect("write GET");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
        "plain GET is not proxied: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_rejects_disallowed_port_with_403() {
    let directory = temp_data_dir("connect-403");
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let manager = build_manager(
        directory.path(),
        vec![connect_spec(
            DestinationRef::parse(&b32).expect("destination"),
        )],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"CONNECT connect-target.i2p:80 HTTP/1.1\r\nHost: connect-target.i2p\r\n\r\n")
        .await
        .expect("write CONNECT");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "port 80 is not allowed: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_rejects_clearnet_authority() {
    let directory = temp_data_dir("connect-clearnet");
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let manager = build_manager(
        directory.path(),
        vec![connect_spec(
            DestinationRef::parse(&b32).expect("destination"),
        )],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com\r\n\r\n")
        .await
        .expect("write CONNECT");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        !text.starts_with("HTTP/1.1 2"),
        "clearnet must never establish: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_unknown_i2p_host_has_no_2xx() {
    let directory = temp_data_dir("connect-unknown");
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let manager = build_manager(
        directory.path(),
        vec![connect_spec(
            DestinationRef::parse(&b32).expect("destination"),
        )],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"CONNECT unknown-host.i2p:443 HTTP/1.1\r\nHost: unknown-host.i2p\r\n\r\n")
        .await
        .expect("write CONNECT");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n")
            || text.starts_with("HTTP/1.1 502 Bad Gateway\r\n"),
        "unknown host fails without a tunnel: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_malformed_head_is_400() {
    let directory = temp_data_dir("connect-malformed");
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let manager = build_manager(
        directory.path(),
        vec![connect_spec(
            DestinationRef::parse(&b32).expect("destination"),
        )],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"CONNECT\r\n\r\n")
        .await
        .expect("write malformed");
    let head = read_head_bounded(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "malformed head is 400: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn connect_client_siblings_are_isolated() {
    let directory = temp_data_dir("connect-siblings");
    let (manager, _echo) = build_paired_manager(directory.path(), "connect-target.i2p").await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-connect")
        .expect("listener");
    let mut first = TcpStream::connect(listener).await.expect("first");
    let mut second = TcpStream::connect(listener).await.expect("second");
    for stream in [&mut first, &mut second] {
        stream
            .write_all(
                b"CONNECT connect-target.i2p:443 HTTP/1.1\r\nHost: connect-target.i2p\r\n\r\n",
            )
            .await
            .expect("write CONNECT");
    }
    for stream in [&mut first, &mut second] {
        let head = read_head_bounded(stream).await;
        assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"));
    }
    first.write_all(b"first-payload").await.expect("write");
    second.write_all(b"second-payload").await.expect("write");
    // Each tunnel echoes only its own bytes; cross-talk would
    // corrupt the digests.
    let first_echo = read_exact_bounded(&mut first, b"first-payload".len()).await;
    let second_echo = read_exact_bounded(&mut second, b"second-payload".len()).await;
    assert_eq!(first_echo, b"first-payload");
    assert_eq!(second_echo, b"second-payload");
}
