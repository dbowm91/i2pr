//! Plan 292 proxy-authentication product tests.
//!
//! Black-box over loopback TCP only: SOCKS5 RFC 1929 (`05 02` +
//! subnegotiation), HTTP/CONNECT Basic (`407` challenge), and the
//! socksirc composition share one enforcement core. Verified
//! requests proceed to the normal path (unknown-host
//! differentiation proves the request stage is reached: SOCKS answers
//! `04`, HTTP answers non-407); no I2P round-trip is required.
//!
//! Pre-292 open-listener behavior stays covered by the Plan 177/176
//! suites; these tests pin the guarded behavior and its exact
//! negative matrix.

#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    ConnectClientOptions, DestinationPolicy, DestinationRef, HttpClientOptions, IrcClientOptions,
    LocalListenerSpec, PROXY_AUTH_REALM_CONNECT, PROXY_AUTH_REALM_HTTP, PROXY_AUTH_REALM_SOCKS,
    ProxyCredentials, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, Socks5ClientOptions, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

const USERNAME: &str = "operator";
const PASSWORD: &str = "s3cret!";
const WRONG_PASSWORD: &str = "wrong!";

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir")
}

fn build_manager(data_dir: &Path, spec: ServiceTunnelSpec) -> Arc<ServiceTunnelManager> {
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(ServiceTunnelSet {
            tunnels: vec![spec],
        }),
        aliases: Arc::new(StaticAliasTable::new()),
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

fn canonical_b32() -> String {
    format!("{}.b32.i2p", "a".repeat(52))
}

fn base_spec(id: &str, kind: ServiceTunnelKind) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse(&canonical_b32()).expect("destination")),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: i2pr_service_tunnels::ServerAccessPolicy::default(),
        unique_local_address: false,
        http_policy: i2pr_service_tunnels::HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

fn socks_credentials() -> ProxyCredentials {
    ProxyCredentials::new(USERNAME, PASSWORD, PROXY_AUTH_REALM_SOCKS).expect("credentials")
}

fn http_credentials() -> ProxyCredentials {
    ProxyCredentials::new(USERNAME, PASSWORD, PROXY_AUTH_REALM_HTTP).expect("credentials")
}

fn connect_credentials() -> ProxyCredentials {
    ProxyCredentials::new(USERNAME, PASSWORD, PROXY_AUTH_REALM_CONNECT).expect("credentials")
}

/// Reads up to `expect_len` bytes with a bounded deadline; returns
/// what arrived (empty on close/timeout).
async fn read_bounded(stream: &mut TcpStream, expect_len: usize) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 512];
    let started = std::time::Instant::now();
    while buffer.len() < expect_len && started.elapsed() < Duration::from_secs(5) {
        match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
            Ok(Ok(n)) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
    buffer
}

async fn read_exact_2(stream: &mut TcpStream) -> Vec<u8> {
    let mut reply = [0_u8; 2];
    match tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut reply)).await {
        Ok(Ok(_)) => reply.to_vec(),
        _ => Vec::new(),
    }
}

fn subnegotiation(user: &str, pass: &str) -> Vec<u8> {
    let mut bytes = vec![0x01, user.len() as u8];
    bytes.extend_from_slice(user.as_bytes());
    bytes.push(pass.len() as u8);
    bytes.extend_from_slice(pass.as_bytes());
    bytes
}

fn socks_connect_unknown() -> Vec<u8> {
    let host = b"unknown.i2p";
    let mut bytes = vec![0x05, 0x01, 0x00, 0x03, host.len() as u8];
    bytes.extend_from_slice(host);
    bytes.extend_from_slice(&443_u16.to_be_bytes());
    bytes
}

/// SOCKS5 guarded listener: no-auth and wrong-credential greetings
/// fail closed; verified credentials reach the request stage.
#[tokio::test(flavor = "current_thread")]
async fn socks_guarded_listener_enforces_credentials() {
    let directory = temp_data_dir("auth-socks");
    let mut spec = base_spec("auth-socks", ServiceTunnelKind::Socks5Client);
    let mut options = Socks5ClientOptions::defaults();
    options.proxy_auth = Some(socks_credentials());
    spec.socks5_options = Some(options);
    let manager = build_manager(directory.path(), spec);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("auth-socks")
        .expect("listener");

    // No-auth greeting on a guarded listener: `05 ff` + close.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("greeting");
    assert_eq!(read_exact_2(&mut stream).await, vec![0x05, 0xff]);
    assert!(
        read_bounded(&mut stream, 1).await.is_empty(),
        "socket closed after 05 ff"
    );

    // Username/password offer selects subnegotiation ...
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&[0x05, 0x01, 0x02])
        .await
        .expect("greeting");
    assert_eq!(read_exact_2(&mut stream).await, vec![0x05, 0x02]);
    // ... wrong credentials answer `01 01` and close.
    stream
        .write_all(&subnegotiation(USERNAME, WRONG_PASSWORD))
        .await
        .expect("wrong creds");
    assert_eq!(read_exact_2(&mut stream).await, vec![0x01, 0x01]);
    assert!(
        read_bounded(&mut stream, 1).await.is_empty(),
        "socket closed after 01 01"
    );

    // Verified credentials answer `01 00` and reach the request
    // stage (unknown host proves it: `04`, not a greeting error).
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&[0x05, 0x02, 0x00, 0x02])
        .await
        .expect("greeting");
    assert_eq!(read_exact_2(&mut stream).await, vec![0x05, 0x02]);
    stream
        .write_all(&subnegotiation(USERNAME, PASSWORD))
        .await
        .expect("correct creds");
    assert_eq!(read_exact_2(&mut stream).await, vec![0x01, 0x00]);
    stream
        .write_all(&socks_connect_unknown())
        .await
        .expect("request");
    let reply = read_bounded(&mut stream, 10).await;
    assert!(
        reply.starts_with(&[0x05, 0x04]),
        "unknown host after auth: {reply:?}"
    );
}

/// SOCKS4a carries no authentication: a guarded listener rejects it
/// without a reply instead of downgrading.
#[tokio::test(flavor = "current_thread")]
async fn socks_guarded_listener_rejects_socks4a() {
    let directory = temp_data_dir("auth-socks4a");
    let mut spec = base_spec("auth-socks4a", ServiceTunnelKind::Socks5Client);
    let mut options = Socks5ClientOptions::defaults();
    options.proxy_auth = Some(socks_credentials());
    spec.socks5_options = Some(options);
    let manager = build_manager(directory.path(), spec);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("auth-socks4a")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    // Minimal SOCKS4a CONNECT request.
    let mut request = vec![0x04, 0x01, 0x01, 0xbb, 0, 0, 0, 0x01];
    request.extend_from_slice(b"user\0unknown.i2p\0");
    stream.write_all(&request).await.expect("4a request");
    assert!(
        read_bounded(&mut stream, 8).await.is_empty(),
        "4a closed with no reply on a guarded listener"
    );
}

/// HTTP guarded proxy: unauthenticated requests answer 407 with a
/// challenge; verified requests proceed past the gate.
#[tokio::test(flavor = "current_thread")]
async fn http_guarded_proxy_challenges_and_passes() {
    let directory = temp_data_dir("auth-http");
    let mut spec = base_spec("auth-http", ServiceTunnelKind::HttpClient);
    let mut options = HttpClientOptions::defaults();
    options.proxy_auth = Some(http_credentials());
    spec.http_options = Some(options);
    let manager = build_manager(directory.path(), spec);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("auth-http")
        .expect("listener");

    // No credentials: 407 with a Basic challenge, then close.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://unknown.i2p/ HTTP/1.1\r\nHost: unknown.i2p\r\n\r\n")
        .await
        .expect("request");
    let reply = String::from_utf8(read_bounded(&mut stream, 256).await).expect("utf-8");
    assert!(
        reply.starts_with("HTTP/1.1 407 Proxy Authentication Required\r\n"),
        "challenge: {reply:?}"
    );
    assert!(
        reply.contains("Proxy-Authenticate: Basic realm=\"i2pr-http-proxy\""),
        "realm challenge: {reply:?}"
    );

    // Wrong credentials: same 407 (no oracle between missing and
    // wrong at the status line).
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(
            b"GET http://unknown.i2p/ HTTP/1.1\r\nHost: unknown.i2p\r\nProxy-Authorization: Basic d3Jvbmc6d3Jvbmc=\r\n\r\n",
        )
        .await
        .expect("request");
    let reply = String::from_utf8(read_bounded(&mut stream, 256).await).expect("utf-8");
    assert!(
        reply.starts_with("HTTP/1.1 407 "),
        "wrong creds challenged: {reply:?}"
    );

    // Correct credentials proceed past the gate (unknown host fails
    // downstream with a non-407 status, proving the gate passed).
    // "operator:s3cret!" in Base64.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(
            b"GET http://unknown.i2p/ HTTP/1.1\r\nHost: unknown.i2p\r\nProxy-Authorization: Basic b3BlcmF0b3I6czNjcmV0IQ==\r\n\r\n",
        )
        .await
        .expect("request");
    let reply = String::from_utf8(read_bounded(&mut stream, 256).await).expect("utf-8");
    assert!(
        reply.starts_with("HTTP/1.1 ") && !reply.starts_with("HTTP/1.1 407"),
        "verified request passes the gate: {reply:?}"
    );
}

/// Strict-CONNECT guarded listener: unauthenticated CONNECT answers
/// 407; verified CONNECT proceeds past the gate.
#[tokio::test(flavor = "current_thread")]
async fn connect_guarded_listener_challenges_and_passes() {
    let directory = temp_data_dir("auth-connect");
    let mut spec = base_spec("auth-connect", ServiceTunnelKind::ConnectClient);
    let mut options = ConnectClientOptions::defaults();
    options.proxy_auth = Some(connect_credentials());
    spec.connect_options = Some(options);
    let manager = build_manager(directory.path(), spec);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("auth-connect")
        .expect("listener");

    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"CONNECT unknown.i2p:443 HTTP/1.1\r\nHost: unknown.i2p:443\r\n\r\n")
        .await
        .expect("connect");
    let reply = String::from_utf8(read_bounded(&mut stream, 256).await).expect("utf-8");
    assert!(
        reply.starts_with("HTTP/1.1 407 Proxy Authentication Required\r\n"),
        "challenge: {reply:?}"
    );
    assert!(
        reply.contains("Proxy-Authenticate: Basic realm=\"i2pr-connect\""),
        "realm challenge: {reply:?}"
    );

    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(
            b"CONNECT unknown.i2p:443 HTTP/1.1\r\nHost: unknown.i2p:443\r\nProxy-Authorization: Basic b3BlcmF0b3I6czNjcmV0IQ==\r\n\r\n",
        )
        .await
        .expect("connect");
    let reply = String::from_utf8(read_bounded(&mut stream, 256).await).expect("utf-8");
    assert!(
        reply.starts_with("HTTP/1.1 ") && !reply.starts_with("HTTP/1.1 407"),
        "verified CONNECT passes the gate: {reply:?}"
    );
}

/// SOCKS+IRC composition shares the SOCKS negotiator: a guarded
/// listener refuses no-auth greetings the same way.
#[tokio::test(flavor = "current_thread")]
async fn socksirc_guarded_listener_refuses_no_auth() {
    let directory = temp_data_dir("auth-socksirc");
    let mut spec = base_spec("auth-socksirc", ServiceTunnelKind::SocksIrc);
    let mut socks = Socks5ClientOptions::defaults();
    socks.proxy_auth = Some(socks_credentials());
    spec.socks5_options = Some(socks);
    spec.irc_options = Some(IrcClientOptions::defaults());
    let manager = build_manager(directory.path(), spec);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("auth-socksirc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("greeting");
    assert_eq!(read_exact_2(&mut stream).await, vec![0x05, 0xff]);
    assert!(
        read_bounded(&mut stream, 1).await.is_empty(),
        "socket closed after 05 ff"
    );
}
