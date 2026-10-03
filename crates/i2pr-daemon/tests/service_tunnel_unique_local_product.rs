//! Plan 292 `unique_local_address` product tests.
//!
//! Black-box over loopback TCP only: a generic server with the flag
//! set dials its TCP target from a deterministic 127/8 source
//! derived from the caller hash; without the flag the wildcard
//! source (`127.0.0.1`) is used. A recording echo fixture observes
//! the source address — the tunnel path itself is unchanged, only
//! the dial source differs.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpServerPolicy, LocalListenerSpec, ServerAccessPolicy,
    ServerTarget, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir")
}

fn build_manager(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
) -> Arc<ServiceTunnelManager> {
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
        aliases: Arc::new(aliases),
    };
    Arc::new(ServiceTunnelManager::new(manager_config).expect("manager builds"))
}

fn client_spec(id: &str, server_b64: &str) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericClient,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse(server_b64).expect("destination")),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        http_policy: HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

fn server_spec(id: &str, target: SocketAddr, unique_local_address: bool) -> ServiceTunnelSpec {
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
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address,
        multihoming: false,
        reply_bundling: false,
        http_policy: HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

async fn probe_server_b64(data_dir: &Path, target: SocketAddr, server_id: &str) -> String {
    let probe = build_manager(
        data_dir,
        vec![server_spec(server_id, target, false)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("server probe");
    probe
        .service_destination_b64(server_id)
        .expect("server b64")
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

/// Echo fixture that records the source address of each accepted
/// connection (up to `connections`) before echoing.
async fn start_recording_echo(
    connections: usize,
) -> (
    SocketAddr,
    tokio::sync::mpsc::UnboundedReceiver<SocketAddr>,
    tokio::task::JoinHandle<usize>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (seen_tx, seen_rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut total = 0_usize;
        for _ in 0..connections {
            let Ok(accepted) =
                tokio::time::timeout(Duration::from_secs(20), listener.accept()).await
            else {
                break;
            };
            let (mut stream, _) = accepted.expect("accept");
            let peer = stream.peer_addr().expect("peer addr");
            let _ = seen_tx.send(peer);
            let mut chunk = [0_u8; 4096];
            loop {
                match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await {
                    Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                    Ok(Ok(n)) => {
                        total += n;
                        if stream.write_all(&chunk[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
        total
    });
    (addr, seen_rx, task)
}

async fn roundtrip(listener: SocketAddr, payload: &[u8]) {
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(payload).await.expect("write");
    let mut out = vec![0_u8; payload.len()];
    tokio::time::timeout(Duration::from_secs(20), stream.read_exact(&mut out))
        .await
        .expect("echo deadline")
        .expect("echo bytes");
    assert_eq!(out, payload);
    let _ = stream.shutdown().await;
}

/// Whether the platform has aliases beyond 127.0.0.1 assigned
/// (Linux binds the whole 127/8; macOS configures only
/// 127.0.0.1, so unique-local dials fall back to wildcard there).
async fn platform_has_loopback_aliases() -> bool {
    let probe = tokio::net::TcpSocket::new_v4().expect("socket");
    probe
        .bind(SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 6, 6, 6)),
            0,
        ))
        .is_ok()
}

/// With the flag set, the server dials from a deterministic
/// loopback source that is stable per caller across connections
/// and distinct from the wildcard default. Where the platform
/// has no alias assigned, the dial still succeeds from the
/// wildcard source and the fallback is counted.
#[tokio::test(flavor = "current_thread")]
async fn unique_local_dial_uses_deterministic_source() {
    let directory = temp_data_dir("unique-local-on");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (target_addr, mut seen, _echo) = start_recording_echo(2).await;
    let server_b64 = probe_server_b64(&data_dir, target_addr, "alpha-server").await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("alpha-client", &server_b64),
            server_spec("alpha-server", target_addr, true),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    roundtrip(listener, b"plan292-unique-local-001").await;
    roundtrip(listener, b"plan292-unique-local-002").await;
    let first = tokio::time::timeout(Duration::from_secs(10), seen.recv())
        .await
        .expect("first source")
        .expect("source");
    let second = tokio::time::timeout(Duration::from_secs(10), seen.recv())
        .await
        .expect("second source")
        .expect("source");
    assert!(first.ip().is_loopback(), "source stays loopback: {first}");
    if platform_has_loopback_aliases().await {
        assert_ne!(
            first.ip().to_string(),
            "127.0.0.1",
            "source is not the wildcard default"
        );
        assert_eq!(first.ip(), second.ip(), "source is deterministic per peer");
        assert_eq!(manager.unique_local_fallbacks_for("alpha-server"), 0);
    } else {
        assert_eq!(first.ip().to_string(), "127.0.0.1");
        assert_eq!(
            manager.unique_local_fallbacks_for("alpha-server"),
            2,
            "both dials counted their fallback"
        );
    }
}

/// Without the flag, the server dials from the wildcard source
/// (negative control: the flag changes the dial, nothing else).
#[tokio::test(flavor = "current_thread")]
async fn default_dial_uses_wildcard_source() {
    let directory = temp_data_dir("unique-local-off");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (target_addr, mut seen, _echo) = start_recording_echo(1).await;
    let server_b64 = probe_server_b64(&data_dir, target_addr, "alpha-server").await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("alpha-client", &server_b64),
            server_spec("alpha-server", target_addr, false),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    roundtrip(listener, b"plan292-wildcard-source-001").await;
    let source = tokio::time::timeout(Duration::from_secs(10), seen.recv())
        .await
        .expect("source observed")
        .expect("source");
    assert_eq!(source.ip().to_string(), "127.0.0.1");
}
