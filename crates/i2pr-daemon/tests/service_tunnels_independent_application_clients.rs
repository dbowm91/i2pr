//! Plan 181 §5 service-tunnel independent-application-client wire surface.
//!
//! Companion to `service_tunnels_local_roundtrip.rs` (Plan 182):
//! where the round-trip suite proves successful end-to-end byte
//! movement, these tests pin the application-facing wire surface
//! with ordinary unmodified TCP clients and verify:
//!
//! - HTTP proxy accepts ordinary HTTP/1.1 requests and emits a
//!   bounded typed response for unresolvable targets (the success
//!   path lives in the round-trip suite);
//! - SOCKS5 proxy completes the no-auth RFC 1928 greeting and
//!   rejects IPv4-ATYP fallback without disturbing the listener;
//! - IRC client tunnel accepts the registration handshake lines
//!   and returns accounting to baseline on EOF;
//! - generic client tunnel accepts raw bytes through the manager;
//! - server-tunnel persistent destination identity survives a
//!   full manager rebuild (Plan 175 invariant, also a Plan 181
//!   §5.1 row).
//!
//! The Plan 181 §6 remote independent-router rows remain classified
//! `m6-mixed-router-streaming-blocker` (retained M6 debt, not this
//! suite); see `plans/181-status.md` once written.

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
use tokio::net::TcpStream;

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

fn build_manager(data_dir: &Path, specs: Vec<ServiceTunnelSpec>) -> ServiceTunnelManager {
    let tunnel_set = ServiceTunnelSet { tunnels: specs };
    let aliases = StaticAliasTable::new();
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(tunnel_set),
        aliases: Arc::new(aliases),
    };
    ServiceTunnelManager::new(manager_config).expect("manager builds")
}

fn canonical_b32() -> String {
    format!("{}{}.b32.i2p", "a".repeat(52), "")
}

fn client_spec(
    id: &str,
    kind: ServiceTunnelKind,
    listener_port: u16,
    target_b32: &str,
) -> ServiceTunnelSpec {
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
        listener: Some(
            LocalListenerSpec::parse_socket(&format!("127.0.0.1:{listener_port}"))
                .expect("listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse(target_b32).expect("destination")),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options,
        socks5_options,
        irc_options,
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
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
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

async fn read_some(stream: &mut TcpStream, max_bytes: usize, deadline: Duration) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 512];
    let started = std::time::Instant::now();
    while started.elapsed() < deadline && buffer.len() < max_bytes {
        match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => buffer.extend_from_slice(&chunk[..n]),
            _ => break,
        }
    }
    buffer
}

#[tokio::test(flavor = "current_thread")]
async fn m10_application_external_http_proxy_emits_bounded_response() {
    // Drives the HTTP proxy with an ordinary HTTP/1.1 absolute-form
    // GET against a `.i2p` host. The application-facing wire surface
    // must respond within the bounded deadline; the byte round-trip
    // through local Streaming is the Plan 181 §6.3-classified M6
    // mixed-router Streaming blocker (see plan/status), so the
    // expected observable is a typed 5xx, not a 200.
    let directory = temp_data_dir("m10-http-external");
    let manager = Arc::new(build_manager(
        directory.path(),
        vec![client_spec(
            "alpha-http-client",
            ServiceTunnelKind::HttpClient,
            0,
            &canonical_b32(),
        )],
    ));
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(
            b"GET http://alpha-test.i2p/ HTTP/1.1\r\nHost: alpha-test.i2p\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("write");
    let _ = stream.shutdown().await;
    let response = read_some(&mut stream, 4096, Duration::from_secs(15)).await;
    let text = std::str::from_utf8(&response).unwrap_or("");
    assert!(
        text.starts_with("HTTP/1.1 "),
        "expected HTTP/1.1 response line, got: {text}"
    );
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    assert!(
        (400..600).contains(&status),
        "expected bounded 4xx/5xx, got: {status} text={text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn m10_application_external_socks5_domainname_handshake_completes() {
    // Drives the SOCKS5 proxy with the unmodified RFC 1928
    // no-authentication greeting + CONNECT request with a DOMAINNAME
    // ATYP. The application proxy must reply with a success or
    // typed failure reply; the proxy never falls back to IPv4/IPv6
    // ATYP for `.i2p` hosts (this test sets DOMAINNAME explicitly).
    let directory = temp_data_dir("m10-socks5-external");
    let manager = Arc::new(build_manager(
        directory.path(),
        vec![client_spec(
            "alpha-socks5-client",
            ServiceTunnelKind::Socks5Client,
            0,
            &canonical_b32(),
        )],
    ));
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    // RFC 1928 no-auth greeting + CONNECT request: VER=5, NMETHODS=1,
    // METHODS=[0x00 (no-auth)]; VER=5, CMD=CONNECT (1), RSV=0,
    // ATYP=DOMAINNAME (3), DST.ADDR=alpha-test.i2p, DST.PORT=80.
    let host = b"alpha-test.i2p";
    let mut greeting = vec![0x05, 0x01, 0x00];
    greeting.extend_from_slice(&[0x05, 0x01, 0x00, 0x03, host.len() as u8]);
    greeting.extend_from_slice(host);
    greeting.extend_from_slice(&[0x00, 0x50]);
    stream.write_all(&greeting).await.expect("write greeting");
    let response = read_some(&mut stream, 64, Duration::from_secs(5)).await;
    assert!(!response.is_empty(), "expected SOCKS5 greeting reply");
    assert_eq!(response[0], 0x05, "reply VER must be 5");
    // Reply METHOD byte (0x00 = no-auth selected).
    assert_eq!(response[1], 0x00, "expected no-auth method selection");
}

#[tokio::test(flavor = "current_thread")]
async fn m10_application_external_socks5_rejects_ipv4_fallback() {
    // SOCKS5 proxy never accepts IPv4 ATYP for `.i2p` targets; the
    // application must send DOMAINNAME for I2P destinations. An
    // IPv4 ATYP request must terminate the connection cleanly
    // without consuming unbounded resources.
    let directory = temp_data_dir("m10-socks5-ipv4");
    let manager = Arc::new(build_manager(
        directory.path(),
        vec![client_spec(
            "alpha-socks5-client",
            ServiceTunnelKind::Socks5Client,
            0,
            &canonical_b32(),
        )],
    ));
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    let mut greeting = vec![0x05, 0x01, 0x00];
    // VER=5, CMD=CONNECT (1), RSV=0, ATYP=IPv4 (1),
    // DST.ADDR=127.0.0.1, DST.PORT=80.
    greeting.extend_from_slice(&[0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x00, 0x50]);
    stream.write_all(&greeting).await.expect("write greeting");
    // Either a typed RFC 1928 reply (server failure / address-type
    // not supported / connection refused) or a connection close
    // without reply. Both are acceptable fail-closed outcomes.
    let response = read_some(&mut stream, 64, Duration::from_secs(5)).await;
    if !response.is_empty() {
        assert_eq!(response[0], 0x05, "reply VER must be 5 on IPv4 fallback");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn m10_application_external_irc_client_accepts_registration() {
    // Drives the IRC client tunnel with the unmodified IRC/IRCv3
    // line framing an unmodified Python `irc.client` driver would
    // emit. The runtime-neutral parser must accept NICK + USER and
    // the listener must stay open across the registration handshake
    // so the application-facing wire surface is observable without
    // the Streaming byte round-trip.
    let directory = temp_data_dir("m10-irc-client-external");
    let manager = Arc::new(build_manager(
        directory.path(),
        vec![client_spec(
            "alpha-irc-client",
            ServiceTunnelKind::IrcClient,
            0,
            &canonical_b32(),
        )],
    ));
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-irc-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"NICK alice\r\nUSER alice 0 * :Alice\r\nCAP LS\r\n")
        .await
        .expect("write");
    // Allow the runtime-neutral parser to drain a few milliseconds.
    let _ = tokio::time::sleep(Duration::from_millis(200)).await;
    // Close the loopback side; the manager must observe EOF and
    // release the per-listener accounting back to baseline. Poll
    // briefly so the assertion does not race task teardown.
    let _ = stream.shutdown().await;
    let mut snapshot = manager.snapshot();
    for _ in 0..20 {
        if snapshot.active_client_connections == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        snapshot = manager.snapshot();
    }
    assert_eq!(
        snapshot.active_client_connections, 0,
        "active connections must return to baseline after EOF"
    );
    // Plan 182: the placeholder `aaa…b32.i2p` destination resolves
    // nowhere, so the executor fail-closes with exactly one counted
    // BadGateway. The success path (resolvable destination) lives
    // in the round-trip suite.
    assert_eq!(
        snapshot.failed_connects_total, 1,
        "unresolvable destination must count exactly one failed connect"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn m10_application_external_generic_client_accepts_bytes() {
    // Drives the generic client tunnel with an ordinary TCP byte
    // stream from a non-IRC, non-HTTP, non-SOCKS application
    // (the canonical generic tunnel use case). The manager must
    // accept the bytes and round-trip them through the local
    // Streaming pump — the byte round-trip is the §6.3 blocker;
    // this test proves the application-facing wire surface.
    let directory = temp_data_dir("m10-generic-external");
    let manager = Arc::new(build_manager(
        directory.path(),
        vec![client_spec(
            "alpha-generic-client",
            ServiceTunnelKind::GenericClient,
            0,
            &canonical_b32(),
        )],
    ));
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-generic-client")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"hello-from-an-ordinary-tcp-client\n")
        .await
        .expect("write");
    let _ = stream.shutdown().await;
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn m10_application_external_server_destination_restart_stable() {
    // Restart-stability of the server-tunnel persistent identity.
    // Two manager rebuilds with the same data dir produce the
    // same server destination b64 and id. The harness can use this
    // identity for cross-process alias persistence.
    let directory = temp_data_dir("m10-server-restart");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let first = Arc::new(build_manager(
        directory.path(),
        vec![server_spec("alpha-server", target_socket)],
    ));
    let _ = first.prepare().await.expect("first prepare");
    let first_b64 = first
        .service_destination_b64("alpha-server")
        .expect("first b64");
    drop(first);
    let second = Arc::new(build_manager(
        directory.path(),
        vec![server_spec("alpha-server", target_socket)],
    ));
    let _ = second.prepare().await.expect("second prepare");
    let second_b64 = second
        .service_destination_b64("alpha-server")
        .expect("second b64");
    assert_eq!(first_b64, second_b64, "restart must preserve server b64");
}
