//! Plan 296 `multihoming` product tests.
//!
//! Black-box over loopback TCP only: a multihomed generic server
//! selects its TCP target per connection in round-robin order with
//! sequential failover; without the flag every connection dials the
//! first target. Two recording echo fixtures observe which target
//! each connection lands on — the tunnel path itself is unchanged,
//! only the dial target differs.

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

fn client_spec_live(id: &str, server_b64: &str) -> ServiceTunnelSpec {
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

fn multihomed_server_spec(
    id: &str,
    first: SocketAddr,
    rest: Vec<SocketAddr>,
    multihoming: bool,
) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(first)),
        targets: rest.into_iter().map(ServerTarget::LoopbackTcp).collect(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming,
        reply_bundling: false,
        http_policy: HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

async fn probe_server_b64(data_dir: &Path, spec: ServiceTunnelSpec, server_id: &str) -> String {
    let probe = build_manager(data_dir, vec![spec], StaticAliasTable::new());
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

/// Echo fixture that reports one arrival per accepted connection on
/// `seen` before echoing the payload back.
async fn start_arrival_echo(
    connections: usize,
) -> (
    SocketAddr,
    tokio::sync::mpsc::UnboundedReceiver<()>,
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
            let _ = seen_tx.send(());
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

async fn expect_arrival(seen: &mut tokio::sync::mpsc::UnboundedReceiver<()>, which: &str) {
    tokio::time::timeout(Duration::from_secs(10), seen.recv())
        .await
        .unwrap_or_else(|_| panic!("{which} arrival missing"))
        .expect("arrival");
}

async fn expect_no_arrival(seen: &mut tokio::sync::mpsc::UnboundedReceiver<()>, which: &str) {
    assert!(
        tokio::time::timeout(Duration::from_millis(500), seen.recv())
            .await
            .is_err(),
        "{which} must see no arrival"
    );
}

/// With the flag set, consecutive connections rotate across the
/// configured targets in order (first target first), proving the
/// selection owner consumes the flag.
#[tokio::test(flavor = "current_thread")]
async fn multihoming_rotates_targets_in_order() {
    let directory = temp_data_dir("multihoming-rotate");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (addr_a, mut seen_a, _echo_a) = start_arrival_echo(2).await;
    let (addr_b, mut seen_b, _echo_b) = start_arrival_echo(2).await;
    let server_b64 = probe_server_b64(
        &data_dir,
        multihomed_server_spec("mh-server", addr_a, vec![addr_b], true),
        "mh-server",
    )
    .await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec_live("mh-client", &server_b64),
            multihomed_server_spec("mh-server", addr_a, vec![addr_b], true),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("mh-client")
        .expect("listener");
    roundtrip(listener, b"plan296-multihoming-001").await;
    expect_arrival(&mut seen_a, "first").await;
    roundtrip(listener, b"plan296-multihoming-002").await;
    expect_arrival(&mut seen_b, "second").await;
    roundtrip(listener, b"plan296-multihoming-003").await;
    expect_arrival(&mut seen_a, "third").await;
    roundtrip(listener, b"plan296-multihoming-004").await;
    expect_arrival(&mut seen_b, "fourth").await;
}

/// Without the flag, every connection dials the first target even
/// when more targets are configured (legacy behavior preserved).
#[tokio::test(flavor = "current_thread")]
async fn first_target_serves_every_connection_without_flag() {
    let directory = temp_data_dir("multihoming-off");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (addr_a, mut seen_a, _echo_a) = start_arrival_echo(2).await;
    // Capacity 1 keeps the second listener (and its sender) alive so
    // the quiet wait below observes a real timeout, not a dropped
    // sender.
    let (addr_b, mut seen_b, _echo_b) = start_arrival_echo(1).await;
    let server_b64 = probe_server_b64(
        &data_dir,
        multihomed_server_spec("mh-server", addr_a, vec![addr_b], false),
        "mh-server",
    )
    .await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec_live("mh-client", &server_b64),
            multihomed_server_spec("mh-server", addr_a, vec![addr_b], false),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("mh-client")
        .expect("listener");
    roundtrip(listener, b"plan296-first-target-001").await;
    expect_arrival(&mut seen_a, "first").await;
    roundtrip(listener, b"plan296-first-target-002").await;
    expect_arrival(&mut seen_a, "second").await;
    expect_no_arrival(&mut seen_b, "second target").await;
}

/// A refused first target fails over to the next target within the
/// same connection instead of failing the connection.
#[tokio::test(flavor = "current_thread")]
async fn refused_target_fails_over_to_next_target() {
    let directory = temp_data_dir("multihoming-failover");
    let data_dir: PathBuf = directory.path().to_path_buf();
    // Reserve a loopback port, then release it so the first dial is
    // refused fast (the temporary listener drops at the end of the
    // statement, freeing the port).
    let dead = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind")
        .local_addr()
        .expect("addr");
    let (addr_b, mut seen_b, _echo_b) = start_arrival_echo(1).await;
    let server_b64 = probe_server_b64(
        &data_dir,
        multihomed_server_spec("mh-server", dead, vec![addr_b], true),
        "mh-server",
    )
    .await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec_live("mh-client", &server_b64),
            multihomed_server_spec("mh-server", dead, vec![addr_b], true),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("mh-client")
        .expect("listener");
    roundtrip(listener, b"plan296-failover-001").await;
    expect_arrival(&mut seen_b, "failover").await;
}

/// A single target leaves nothing to select across: spec
/// validation rejects it, and even an unvalidated spec fails safe
/// to first-target dials (the selection requires two committed TCP
/// targets, and the rotation counter never advances).
#[tokio::test(flavor = "current_thread")]
async fn single_target_multihoming_is_rejected_and_fails_safe() {
    let directory = temp_data_dir("multihoming-single");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (addr_a, _seen_a, _echo_a) = start_arrival_echo(1).await;
    let spec = multihomed_server_spec("mh-server", addr_a, Vec::new(), true);
    assert!(
        spec.validate().is_err(),
        "single-target multihoming must not validate"
    );
    let manager = build_manager(&data_dir, vec![spec], StaticAliasTable::new());
    let _runtimes = manager.prepare().await.expect("prepare stages");
    let runtime = manager
        .service_runtime_for_spec("mh-server")
        .expect("runtime");
    assert_eq!(manager.server_dial_targets_for(&runtime), vec![addr_a]);
    assert_eq!(manager.server_dial_targets_for(&runtime), vec![addr_a]);
}
