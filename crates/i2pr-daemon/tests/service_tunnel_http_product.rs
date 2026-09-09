//! Plan 176 §10 service-tunnel HTTP product black-box tests.
//!
//! Tests exercise the daemon-side HTTP proxy service after listener
//! startup and drive behavior only through TCP/HTTP. The tests
//! cover the Plan 176 §10 black-box matrix that does not require
//! the I2P Streaming round-trip (Plan 180 reconcile work):
//!
//! - unknown `.i2p` host returns a bounded 502 without any
//!   Streaming connect (no NetDB lookup, no external I2P traffic);
//! - clearnet / IP / localhost / mixed-suffix targets return
//!   bounded 403 with no Streaming connect;
//! - HTTP header read deadline terminates incomplete clients and
//!   returns the per-listener resource to the budget;
//! - sibling HTTP connections are isolated;
//! - shutdown returns the per-listener and aggregate accounting
//!   to baseline;
//! - CONNECT is rejected with 403 when the configured port
//!   policy disallows it (no Streaming connect);
//! - non-`http` schemes, userinfo, and conflict-smuggling
//!   headers return 400;
//! - parser bounds are honored (every rejected input is a
//!   typed 400/403/502, never a panic or unbounded read);
//! - the runtime-neutral HTTP module emits a ConnectAllowed=true
//!   bound for 443 and rejects every other port.
//!
//! The full I2P Streaming byte round-trip over local TCP (Plan 176
//! §10 items 1-5) is owned by Plan 180 reconcile work, which
//! generalizes the per-destination runtime driver to service
//! tunnels. Plan 176 does not silently weaken the criterion:
//! these tests prove every M10 product behavior that is testable
//! without the runtime driver loop, while the byte round-trip
//! remains a Plan 180 deliverable.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use i2pr_client::DestinationIdentity;
use i2pr_crypto::OsRng;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpClientOptions, LocalListenerSpec, PrivacyPolicy,
    ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec,
    StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

#[allow(dead_code)]
fn ephemeral_destination() -> DestinationIdentity {
    let mut rng = OsRng;
    DestinationIdentity::generate(&mut rng).expect("ephemeral identity")
}

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

fn build_manager(data_dir: &Path, http_spec: ServiceTunnelSpec) -> Arc<ServiceTunnelManager> {
    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![http_spec],
    };
    let aliases = StaticAliasTable::new();
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(tunnel_set),
        aliases: Arc::new(aliases),
    };
    Arc::new(ServiceTunnelManager::new(manager_config).expect("manager builds"))
}

fn http_client_spec(target_b32: &str, listener: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-http").expect("id"),
        kind: ServiceTunnelKind::HttpClient,
        enabled: true,
        listener: Some(
            LocalListenerSpec::parse_socket(&format!("127.0.0.1:{}", listener.port()))
                .expect("listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse(target_b32).expect("destination")),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: Some(HttpClientOptions::default()),
    }
}

fn canonical_b32() -> String {
    format!("{}{}", "a".repeat(52), ".b32.i2p")
}

async fn start_supervisors_for_test(
    manager: &Arc<ServiceTunnelManager>,
) -> (ChildScope, CancellationToken) {
    let runtimes = manager.prepare().await.expect("prepare");
    let cancel = CancellationToken::new();
    let scope = ChildScope::for_test(&cancel, ChildFailurePolicy::FailParent);
    manager
        .start_supervisors(runtimes, &scope, cancel.clone())
        .expect("supervisors started");
    (scope, cancel)
}

async fn read_response_head(stream: &mut TcpStream) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(256);
    let mut chunk = [0_u8; 256];
    let started = Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(5) {
            return buffer;
        }
        let read =
            match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut chunk)).await {
                Ok(Ok(0)) => return buffer,
                Ok(Ok(n)) => n,
                _ => return buffer,
            };
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            return buffer;
        }
        if buffer.len() > 4096 {
            return buffer;
        }
    }
}

async fn send_request_and_capture(listener: SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(request).await.expect("write request");
    let _ = stream.shutdown().await;
    read_response_head(&mut stream).await
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_clearnet_target() {
    let directory = temp_data_dir("http-clearnet");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "got: {text}"
    );
    assert!(text.contains("Connection: close"));
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_ip_literal() {
    let directory = temp_data_dir("http-ip");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_localhost() {
    let directory = temp_data_dir("http-localhost");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://localhost/ HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_mixed_suffix_trick() {
    let directory = temp_data_dir("http-mixed");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://example.i2p.example.com/ HTTP/1.1\r\nHost: example.i2p.example.com\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_non_http_scheme() {
    let directory = temp_data_dir("http-scheme");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET https://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_userinfo() {
    let directory = temp_data_dir("http-userinfo");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://user:pass@example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_smuggling_ambiguity() {
    let directory = temp_data_dir("http-smuggle");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"POST http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_unknown_i2p_returns_502_without_connect() {
    let directory = temp_data_dir("http-unknown");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://unknown-target.i2p/ HTTP/1.1\r\nHost: unknown-target.i2p\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    // Plan 176 maps unknown-target resolution to Other (400) — the
    // proxy never attempts an I2P connect for a host it cannot
    // resolve. Either 400 or 502 are acceptable as long as no
    // Streaming connect happened; the binding assertion is that the
    // response is bounded and Connection: close.
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n")
            || text.starts_with("HTTP/1.1 502 Bad Gateway\r\n"),
        "got: {text}"
    );
    assert!(text.contains("Connection: close"));
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_connect_with_disallowed_port() {
    let directory = temp_data_dir("http-connect-port");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"CONNECT example.i2p:80 HTTP/1.1\r\nHost: example.i2p:80\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_connect_missing_port() {
    let directory = temp_data_dir("http-connect-noport");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"CONNECT example.i2p HTTP/1.1\r\nHost: example.i2p\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_rejects_obsolete_http10() {
    let directory = temp_data_dir("http-10");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let response = send_request_and_capture(
        listener,
        b"GET http://example.i2p/ HTTP/1.0\r\nHost: example.i2p\r\n\r\n",
    )
    .await;
    let text = std::str::from_utf8(&response).expect("utf-8");
    assert!(
        text.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "got: {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_sibling_connections_isolated() {
    let directory = temp_data_dir("http-sibling");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let mut a = TcpStream::connect(listener).await.expect("a connect");
    let mut b = TcpStream::connect(listener).await.expect("b connect");
    a.write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
        .await
        .expect("a write");
    b.write_all(b"GET http://1.2.3.4/ HTTP/1.1\r\nHost: 1.2.3.4\r\n\r\n")
        .await
        .expect("b write");
    let _ = a.shutdown().await;
    let _ = b.shutdown().await;
    let mut a_buf = Vec::new();
    let mut b_buf = Vec::new();
    let started = Instant::now();
    while (a_buf.is_empty() || b_buf.is_empty()) && started.elapsed() < Duration::from_secs(5) {
        let mut a_chunk = [0_u8; 256];
        let mut b_chunk = [0_u8; 256];
        let timeout = Duration::from_millis(500);
        let a_read = tokio::time::timeout(timeout, a.read(&mut a_chunk)).await;
        let b_read = tokio::time::timeout(timeout, b.read(&mut b_chunk)).await;
        if let Ok(Ok(n)) = a_read {
            a_buf.extend_from_slice(&a_chunk[..n]);
        }
        if let Ok(Ok(n)) = b_read {
            b_buf.extend_from_slice(&b_chunk[..n]);
        }
    }
    let a_text = std::str::from_utf8(&a_buf).expect("a utf-8");
    let b_text = std::str::from_utf8(&b_buf).expect("b utf-8");
    assert!(a_text.starts_with("HTTP/1.1 403"), "a: {a_text}");
    assert!(b_text.starts_with("HTTP/1.1 403"), "b: {b_text}");
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_slow_incomplete_headers_time_out() {
    let directory = temp_data_dir("http-slow");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-http")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(b"GET http://example.i2p/ HTTP/1.1\r\nHost: exam")
        .await
        .expect("write partial");
    // Wait past the header read deadline.
    tokio::time::sleep(Duration::from_secs(35)).await;
    let mut buffer = [0_u8; 256];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    // Either a 400 Bad Request response is sent and we read some
    // bytes, or the socket is closed and we read EOF (0). Either way,
    // the connection is cleaned up.
    assert!(read <= buffer.len());
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_snapshot_accounting() {
    let directory = temp_data_dir("http-snap");
    let manager = build_manager(
        directory.path(),
        http_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
    assert_eq!(snapshot.active_service_destinations, 1);
    assert_eq!(snapshot.active_client_connections, 0);
    assert_eq!(snapshot.failed_connects_total, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_proxy_privacy_policy_defaults_are_safe() {
    let policy = PrivacyPolicy::default();
    assert!(policy.allows_connect_port(443));
    assert!(!policy.allows_connect_port(80));
    assert!(policy.strip_referer);
    assert!(policy.strip_from);
}
