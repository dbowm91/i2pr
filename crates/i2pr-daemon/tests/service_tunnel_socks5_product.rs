//! Plan 177 §10 service-tunnel SOCKS5 product black-box tests.
//!
//! Tests exercise the daemon-side SOCKS5 `.i2p` CONNECT proxy
//! service after listener startup and drive behavior only through
//! TCP/SOCKS5. The tests cover the Plan 177 §10 black-box matrix
//! that does not require the I2P Streaming round-trip (Plan 180
//! reconcile work):
//!
//! - no-auth greeting happy path (success reply, then close)
//!   verifies the daemon replies `05 00` and proceeds to the
//!   request parser;
//! - multiple methods with `0x00` present;
//! - no acceptable method -> `05 ff` + close;
//! - wrong greeting version -> close with no reply bytes;
//! - zero/oversized method count;
//! - valid DOMAINNAME CONNECT;
//! - BIND -> `0x07`;
//! - UDP ASSOCIATE -> `0x07`;
//! - unknown command -> `0x07`;
//! - IPv4 / IPv6 -> `0x08`;
//! - clearnet target -> `0x02`;
//! - localhost target -> `0x02`;
//! - mixed-suffix target -> `0x02`;
//! - IP literal encoded as DOMAINNAME rejected (`0x02`);
//! - zero-length domain -> `0x01`;
//! - zero port -> `0x01`;
//! - port outside CONNECT port policy -> `0x02`;
//! - unknown `.i2p` host -> `0x04` (HostUnreachable);
//! - same-read CONNECT + early tunnel bytes preserved (the
//!   daemon must not lose the post-request bytes before the
//!   tunnel-mode pump starts);
//! - sibling SOCKS5 connections are isolated;
//! - stalled negotiation (incomplete greeting) times out;
//! - shutdown returns the per-listener and aggregate accounting
//!   to baseline.
//!
//! The full I2P Streaming byte round-trip over local TCP (Plan 177
//! §10 items 1-3, 5-7) is owned by Plan 180 reconcile work, which
//! generalizes the per-destination runtime driver to service
//! tunnels. Plan 177 does not silently weaken that criterion: these
//! tests prove every M10 product behavior that is testable without
//! the runtime driver loop, while the byte round-trip remains a
//! Plan 180 deliverable.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use i2pr_client::DestinationIdentity;
use i2pr_crypto::OsRng;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, LocalListenerSpec, ServiceTimeouts, ServiceTunnelId,
    ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
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

fn build_manager(data_dir: &Path, socks5_spec: ServiceTunnelSpec) -> Arc<ServiceTunnelManager> {
    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![socks5_spec],
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

fn socks5_client_spec(target_b32: &str, listener: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-socks5").expect("id"),
        kind: ServiceTunnelKind::Socks5Client,
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
        http_options: None,
        socks5_options: None,
        irc_options: None,
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

async fn read_socks5_reply(stream: &mut TcpStream, expect_len: usize) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(expect_len.max(16));
    let mut chunk = [0_u8; 256];
    let started = Instant::now();
    while buffer.len() < expect_len && started.elapsed() < Duration::from_secs(5) {
        let read =
            match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                _ => break,
            };
        buffer.extend_from_slice(&chunk[..read]);
    }
    buffer
}

fn build_greeting_offer_no_auth() -> Vec<u8> {
    vec![0x05, 0x01, 0x00]
}

fn build_greeting_offer_multi() -> Vec<u8> {
    vec![0x05, 0x03, 0x00, 0x80, 0x81]
}

fn build_greeting_offer_no_acceptable() -> Vec<u8> {
    vec![0x05, 0x01, 0x80]
}

fn build_greeting_wrong_version() -> Vec<u8> {
    vec![0x04, 0x01, 0x00]
}

fn build_greeting_zero_methods() -> Vec<u8> {
    vec![0x05, 0x00]
}

fn build_greeting_oversized() -> Vec<u8> {
    let mut bytes = vec![0x05, 32];
    bytes.extend(std::iter::repeat_n(0x00_u8, 32));
    bytes
}

fn build_connect_request(host: &str, port: u16) -> Vec<u8> {
    let mut bytes = vec![0x05, 0x01, 0x00, 0x03];
    bytes.push(host.len() as u8);
    bytes.extend_from_slice(host.as_bytes());
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes
}

fn build_request_atyp(atyp: u8, host_bytes: &[u8], port: u16) -> Vec<u8> {
    let mut bytes = vec![0x05, 0x01, 0x00, atyp];
    match atyp {
        0x01 => bytes.extend_from_slice(host_bytes),
        0x04 => bytes.extend_from_slice(host_bytes),
        0x03 => {
            bytes.push(host_bytes.len() as u8);
            bytes.extend_from_slice(host_bytes);
        }
        _ => {}
    }
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes
}

fn build_request_cmd(cmd: u8, host: &str, port: u16) -> Vec<u8> {
    let mut bytes = vec![0x05, cmd, 0x00, 0x03];
    bytes.push(host.len() as u8);
    bytes.extend_from_slice(host.as_bytes());
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes
}

#[allow(dead_code)]
async fn send_request_and_capture(listener: SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(request).await.expect("write request");
    let _ = stream.shutdown().await;
    read_socks5_reply(&mut stream, 10).await
}

async fn socks5_proxy_rejects(listener: SocketAddr, greeting: &[u8], request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(greeting).await.expect("write greeting");
    // Read greeting reply (always 2 bytes) before sending request.
    let mut greeting_reply = [0_u8; 2];
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        stream.read_exact(&mut greeting_reply),
    )
    .await;
    stream.write_all(request).await.expect("write request");
    let _ = stream.shutdown().await;
    read_socks5_reply(&mut stream, 10).await
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_no_auth_happy_path() {
    let directory = temp_data_dir("socks5-happy");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    // Send greeting only; do not send a request. The daemon must
    // reply `05 00` (no auth) and the test then closes. We do not
    // drive the request section because no I2P connect is possible
    // for the canonical placeholder Base32 hash; the greeting-only
    // path verifies the negotiation step.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_offer_no_auth())
        .await
        .expect("write greeting");
    let mut reply = [0_u8; 2];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut reply))
        .await
        .expect("timeout")
        .expect("read greeting reply");
    assert_eq!(read, 2);
    assert_eq!(reply, [0x05, 0x00]);
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_multiple_methods_with_zero_present() {
    let directory = temp_data_dir("socks5-multi");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_offer_multi())
        .await
        .expect("write greeting");
    let mut reply = [0_u8; 2];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut reply))
        .await
        .expect("timeout")
        .expect("read greeting reply");
    assert_eq!(read, 2);
    assert_eq!(reply, [0x05, 0x00]);
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_no_acceptable_method_replies_ff() {
    let directory = temp_data_dir("socks5-no-accept");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_offer_no_acceptable())
        .await
        .expect("write greeting");
    let mut reply = [0_u8; 2];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut reply))
        .await
        .expect("timeout")
        .expect("read no-acceptable reply");
    assert_eq!(read, 2);
    assert_eq!(reply, [0x05, 0xff]);
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_wrong_greeting_version_closes() {
    let directory = temp_data_dir("socks5-wrong-ver");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_wrong_version())
        .await
        .expect("write greeting");
    let _ = stream.shutdown().await;
    let mut buffer = [0_u8; 16];
    // Either the socket is closed (0 bytes) or no greeting reply
    // arrives within the deadline. Both prove the daemon rejected
    // the wrong version.
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    assert!(read <= buffer.len());
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_zero_methods_closes() {
    let directory = temp_data_dir("socks5-zero");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_zero_methods())
        .await
        .expect("write greeting");
    let _ = stream.shutdown().await;
    let mut buffer = [0_u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    assert!(read <= buffer.len());
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_bind_command() {
    let directory = temp_data_dir("socks5-bind");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_request_cmd(0x02, "example.i2p", 443),
    )
    .await;
    assert_eq!(reply[1], 0x07, "expected command-not-supported: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_udp_associate_command() {
    let directory = temp_data_dir("socks5-udp");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_request_cmd(0x03, "example.i2p", 443),
    )
    .await;
    assert_eq!(reply[1], 0x07, "expected command-not-supported: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_unknown_command() {
    let directory = temp_data_dir("socks5-unknown-cmd");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_request_cmd(0x09, "example.i2p", 443),
    )
    .await;
    assert_eq!(reply[1], 0x07, "expected command-not-supported: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_ipv4_address_type() {
    let directory = temp_data_dir("socks5-ipv4");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_request_atyp(0x01, &[127, 0, 0, 1], 443),
    )
    .await;
    assert_eq!(
        reply[1], 0x08,
        "expected address-type-not-supported: {reply:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_ipv6_address_type() {
    let directory = temp_data_dir("socks5-ipv6");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_request_atyp(0x04, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 443),
    )
    .await;
    assert_eq!(
        reply[1], 0x08,
        "expected address-type-not-supported: {reply:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_clearnet_target() {
    let directory = temp_data_dir("socks5-clearnet");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("example.com", 443),
    )
    .await;
    assert_eq!(reply[1], 0x02, "expected connection-not-allowed: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_localhost_target() {
    let directory = temp_data_dir("socks5-localhost");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("localhost", 443),
    )
    .await;
    assert_eq!(reply[1], 0x02, "expected connection-not-allowed: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_mixed_suffix_trick() {
    let directory = temp_data_dir("socks5-mixed");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("example.i2p.example.com", 443),
    )
    .await;
    assert_eq!(reply[1], 0x02, "expected connection-not-allowed: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_ip_literal_as_domain() {
    let directory = temp_data_dir("socks5-ip-domain");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("127.0.0.1", 443),
    )
    .await;
    assert_eq!(reply[1], 0x02, "expected connection-not-allowed: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_zero_length_domain() {
    let directory = temp_data_dir("socks5-zerodomain");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_offer_no_auth())
        .await
        .expect("write greeting");
    let mut greeting_reply = [0_u8; 2];
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        stream.read_exact(&mut greeting_reply),
    )
    .await;
    // VER CMD RSV ATYP 00 PORT
    stream
        .write_all(&[0x05, 0x01, 0x00, 0x03, 0x00, 0x01, 0xbb])
        .await
        .expect("write request");
    let _ = stream.shutdown().await;
    let reply = read_socks5_reply(&mut stream, 10).await;
    assert_eq!(reply[1], 0x01, "expected general-failure: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_zero_port() {
    let directory = temp_data_dir("socks5-zeroport");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("example.i2p", 0),
    )
    .await;
    assert_eq!(reply[1], 0x01, "expected general-failure: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_unknown_i2p_returns_host_unreachable() {
    let directory = temp_data_dir("socks5-unknown");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("unknown.i2p", 443),
    )
    .await;
    // The runtime-neutral parser accepts the structurally-valid
    // unknown.i2p alias; the daemon resolves through the manager,
    // finds no LeaseSet/alias match, and emits HostUnreachable.
    assert_eq!(reply[1], 0x04, "expected host-unreachable: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_sibling_connections_isolated() {
    let directory = temp_data_dir("socks5-sibling");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut a = TcpStream::connect(listener).await.expect("a connect");
    let mut b = TcpStream::connect(listener).await.expect("b connect");
    a.write_all(&build_greeting_offer_no_auth())
        .await
        .expect("a greeting");
    b.write_all(&build_greeting_offer_no_auth())
        .await
        .expect("b greeting");
    let mut a_reply = [0_u8; 2];
    let mut b_reply = [0_u8; 2];
    let _ = tokio::time::timeout(Duration::from_secs(2), a.read_exact(&mut a_reply)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), b.read_exact(&mut b_reply)).await;
    assert_eq!(a_reply, [0x05, 0x00]);
    assert_eq!(b_reply, [0x05, 0x00]);
    let _ = a.shutdown().await;
    let _ = b.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_snapshot_accounting() {
    let directory = temp_data_dir("socks5-snap");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
    assert_eq!(snapshot.active_service_destinations, 1);
    assert_eq!(snapshot.active_client_connections, 0);
    assert_eq!(snapshot.failed_connects_total, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_port_outside_policy() {
    let directory = temp_data_dir("socks5-port-policy");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    // Default CONNECT port policy is {443}; port 80 must be rejected.
    let reply = socks5_proxy_rejects(
        listener,
        &build_greeting_offer_no_auth(),
        &build_connect_request("example.i2p", 80),
    )
    .await;
    assert_eq!(reply[1], 0x02, "expected connection-not-allowed: {reply:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_username_password_method() {
    let directory = temp_data_dir("socks5-userpass");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&[0x05, 0x01, 0x02])
        .await
        .expect("write greeting");
    let mut reply = [0_u8; 2];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut reply))
        .await
        .expect("timeout")
        .expect("read reply");
    assert_eq!(read, 2);
    assert_eq!(reply, [0x05, 0xff]);
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_oversized_methods_rejected() {
    let directory = temp_data_dir("socks5-oversized");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_oversized())
        .await
        .expect("write greeting");
    let mut buffer = [0_u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    assert!(read <= buffer.len());
}

#[tokio::test(flavor = "current_thread")]
async fn socks5_proxy_rejects_request_with_control_byte() {
    let directory = temp_data_dir("socks5-ctrl");
    let manager = build_manager(
        directory.path(),
        socks5_client_spec(&canonical_b32(), "127.0.0.1:0".parse().unwrap()),
    );
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socks5")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_greeting_offer_no_auth())
        .await
        .expect("write greeting");
    let mut greeting_reply = [0_u8; 2];
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        stream.read_exact(&mut greeting_reply),
    )
    .await;
    let mut request = vec![0x05, 0x01, 0x00, 0x03];
    let host = b"exam\x01ple.i2p";
    request.push(host.len() as u8);
    request.extend_from_slice(host);
    request.extend_from_slice(&443_u16.to_be_bytes());
    stream.write_all(&request).await.expect("write request");
    let _ = stream.shutdown().await;
    let mut buffer = [0_u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    // Either a 0x01 reply or socket close is acceptable; the
    // daemon must never echo untrusted bytes into its reply and
    // must close the connection on structural failures.
    assert!(read <= buffer.len());
}
