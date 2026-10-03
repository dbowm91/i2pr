//! Plan 291 Streamr black-box product tests.
//!
//! The Streamr subscriber/publisher pair composes the
//! runtime-neutral repliable-datagram substrate with daemon-owned
//! loopback UDP sockets: the client subscribes to the configured
//! producer over Datagram1 and forwards producer-bound raw media
//! to its loopback UDP target; the server receives media on its
//! loopback UDP source and fans raw media out to authenticated
//! subscribers. Tests drive behavior only through UDP sockets
//! after supervisor startup:
//!
//! - subscribe + fanout with digest equality;
//! - multi-subscriber fanout;
//! - oversize media dropped (never fragmented);
//! - restart preserves the single persistent server identity and
//!   clears the ephemeral subscriber table;
//! - sibling pairs are isolated.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind,
    ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable, StreamrOptions,
};
use tokio::net::UdpSocket;

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

/// Distinct loopback ports process-wide (parallel tests never share).
fn free_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    static BASE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    let base = *BASE.get_or_init(|| 36_000 + (std::process::id() % 5_000) as u16);
    let port = base + NEXT.fetch_add(1, Ordering::Relaxed) * 41;
    // Bind-then-drop reserves a currently-free port; the race is
    // negligible for loopback tests.
    let socket = std::net::UdpSocket::bind(("127.0.0.1", port)).expect("reserve");
    let bound = socket.local_addr().expect("addr").port();
    drop(socket);
    bound
}

fn streamr_server_spec(id: &str, media_source: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::StreamrServer,
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
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: Some(StreamrOptions {
            local_udp: Some(media_source),
            ..StreamrOptions::default()
        }),
    }
}

fn streamr_client_spec(
    id: &str,
    producer_b64: String,
    media_target: SocketAddr,
) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::StreamrClient,
        enabled: true,
        listener: None,
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::ConfiguredDestination(producer_b64)),
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
        streamr_options: Some(StreamrOptions {
            local_udp: Some(media_target),
            ..StreamrOptions::default()
        }),
    }
}

fn build_manager(data_dir: &Path, specs: Vec<ServiceTunnelSpec>) -> Arc<ServiceTunnelManager> {
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 64,
        per_service_connection_ceiling: 16,
        specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
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

/// Builds a subscribed pair: server probe for the persistent
/// identity, then one manager holding server + client. Returns
/// the manager, the server media-source address, and a bound UDP
/// socket already listening on the client media target.
async fn build_subscribed_pair(
    data_dir: &Path,
    server_id: &str,
    client_id: &str,
    server_port: u16,
    target_port: u16,
) -> (Arc<ServiceTunnelManager>, SocketAddr, UdpSocket) {
    let server_udp: SocketAddr = format!("127.0.0.1:{server_port}").parse().expect("addr");
    let media_target: SocketAddr = format!("127.0.0.1:{target_port}").parse().expect("addr");
    let probe = build_manager(data_dir, vec![streamr_server_spec(server_id, server_udp)]);
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64(server_id)
        .expect("server b64");
    drop(probe);
    let manager = build_manager(
        data_dir,
        vec![
            streamr_server_spec(server_id, server_udp),
            streamr_client_spec(client_id, server_b64, media_target),
        ],
    );
    let player = UdpSocket::bind(media_target).await.expect("player binds");
    (manager, server_udp, player)
}

fn media_bytes(len: usize) -> Vec<u8> {
    // Deterministic pseudo-media with a digest-stable pattern.
    (0..len)
        .map(|index| (index.wrapping_mul(31).wrapping_add(17)) as u8)
        .collect()
}

async fn recv_media(player: &UdpSocket, timeout: Duration) -> Option<Vec<u8>> {
    let mut chunk = [0_u8; 2048];
    match tokio::time::timeout(timeout, player.recv_from(&mut chunk)).await {
        Ok(Ok((length, _))) => Some(chunk[..length].to_vec()),
        _ => None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn streamr_subscribe_and_fanout_digest() {
    let directory = temp_data_dir("streamr-fanout");
    let server_port = free_port();
    let target_port = free_port();
    let (manager, server_udp, player) = build_subscribed_pair(
        directory.path(),
        "alpha-pub",
        "alpha-sub",
        server_port,
        target_port,
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    // Fast-start subscribes (2 s cadence) land well inside this
    // bound; the server table is populated before media flows.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let source = UdpSocket::bind("127.0.0.1:0").await.expect("source binds");
    let media = media_bytes(1024);
    source
        .send_to(&media, server_udp)
        .await
        .expect("media sends");
    let received = recv_media(&player, Duration::from_secs(10))
        .await
        .expect("media arrives");
    assert_eq!(received, media, "fanout is byte-exact");
    // A second packet proves the subscription persists past one
    // fanout (refresh keeps it alive).
    let media_two = media_bytes(512);
    source
        .send_to(&media_two, server_udp)
        .await
        .expect("media sends");
    let received_two = recv_media(&player, Duration::from_secs(10))
        .await
        .expect("second media arrives");
    assert_eq!(received_two, media_two, "second fanout is byte-exact");
}

#[tokio::test(flavor = "current_thread")]
async fn streamr_multi_subscriber_fanout() {
    let directory = temp_data_dir("streamr-multi");
    let server_port = free_port();
    let first_target = free_port();
    let second_target = free_port();
    let server_udp: SocketAddr = format!("127.0.0.1:{server_port}").parse().expect("addr");
    let probe = build_manager(
        directory.path(),
        vec![streamr_server_spec("alpha-pub", server_udp)],
    );
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64("alpha-pub")
        .expect("server b64");
    drop(probe);
    let manager = build_manager(
        directory.path(),
        vec![
            streamr_server_spec("alpha-pub", server_udp),
            streamr_client_spec("sub-one", server_b64.clone(), {
                format!("127.0.0.1:{first_target}").parse().expect("addr")
            }),
            streamr_client_spec("sub-two", server_b64, {
                format!("127.0.0.1:{second_target}").parse().expect("addr")
            }),
        ],
    );
    let first_player = UdpSocket::bind(format!("127.0.0.1:{first_target}"))
        .await
        .expect("first player binds");
    let second_player = UdpSocket::bind(format!("127.0.0.1:{second_target}"))
        .await
        .expect("second player binds");
    let (_scope, _cancel) = start_supervisors(&manager).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let source = UdpSocket::bind("127.0.0.1:0").await.expect("source binds");
    let media = media_bytes(768);
    source
        .send_to(&media, server_udp)
        .await
        .expect("media sends");
    let first = recv_media(&first_player, Duration::from_secs(10))
        .await
        .expect("first subscriber receives");
    let second = recv_media(&second_player, Duration::from_secs(10))
        .await
        .expect("second subscriber receives");
    assert_eq!(first, media);
    assert_eq!(second, media);
}

#[tokio::test(flavor = "current_thread")]
async fn streamr_oversize_media_is_dropped() {
    let directory = temp_data_dir("streamr-oversize");
    let server_port = free_port();
    let target_port = free_port();
    let (manager, server_udp, player) = build_subscribed_pair(
        directory.path(),
        "alpha-pub",
        "alpha-sub",
        server_port,
        target_port,
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let source = UdpSocket::bind("127.0.0.1:0").await.expect("source binds");
    // 1500 bytes exceeds the 1200-byte application ceiling: the
    // server rejects it instead of fragmenting.
    source
        .send_to(&media_bytes(1500), server_udp)
        .await
        .expect("oversize sends");
    assert!(
        recv_media(&player, Duration::from_secs(3)).await.is_none(),
        "oversize media never reaches the player"
    );
    // The subscription still works afterwards (the drop is
    // per-packet, not a connection break).
    let media = media_bytes(256);
    source
        .send_to(&media, server_udp)
        .await
        .expect("media sends");
    let received = recv_media(&player, Duration::from_secs(10))
        .await
        .expect("good media arrives after the drop");
    assert_eq!(received, media);
}

#[tokio::test(flavor = "current_thread")]
async fn streamr_restart_preserves_identity_and_clears_subscriptions() {
    let directory = temp_data_dir("streamr-restart");
    let server_port = free_port();
    let target_port = free_port();
    let server_udp: SocketAddr = format!("127.0.0.1:{server_port}").parse().expect("addr");
    // Probe the persistent server identity first (mirrors the
    // restart-identity pattern: prepare creates, supervisors run).
    let probe = build_manager(
        directory.path(),
        vec![streamr_server_spec("alpha-pub", server_udp)],
    );
    probe.prepare().await.expect("probe prepare");
    let before = probe
        .service_destination_b64("alpha-pub")
        .expect("server b64 before");
    drop(probe);
    let (manager, _, player) = build_subscribed_pair(
        directory.path(),
        "alpha-pub",
        "alpha-sub",
        server_port,
        target_port,
    )
    .await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let source = UdpSocket::bind("127.0.0.1:0").await.expect("source binds");
    let media = media_bytes(128);
    source
        .send_to(&media, server_udp)
        .await
        .expect("media sends");
    assert_eq!(
        recv_media(&player, Duration::from_secs(10))
            .await
            .expect("media"),
        media,
        "subscription live before restart"
    );
    drop(_scope);
    drop(_cancel);
    drop(manager);
    drop(player);
    // Phase two: server only, same directory. The persistent
    // identity survives; the in-memory subscriber table does not,
    // so media has nowhere to go.
    let second = build_manager(
        directory.path(),
        vec![streamr_server_spec("alpha-pub", server_udp)],
    );
    second.prepare().await.expect("prepare");
    let after = second
        .service_destination_b64("alpha-pub")
        .expect("server b64 after");
    assert_eq!(before, after, "one persistent identity across restart");
    let (_scope, _cancel) = start_supervisors(&second).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    source
        .send_to(&media, server_udp)
        .await
        .expect("media sends");
    // No subscriber exists anymore; bind a fresh listener on the
    // old target to prove nothing arrives.
    let silent = UdpSocket::bind(format!("127.0.0.1:{target_port}"))
        .await
        .expect("silent binds");
    assert!(
        recv_media(&silent, Duration::from_secs(3)).await.is_none(),
        "no stale fanout after restart"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn streamr_siblings_are_isolated() {
    let directory = temp_data_dir("streamr-siblings");
    let a_server = free_port();
    let a_target = free_port();
    let b_server = free_port();
    let b_target = free_port();
    let a_udp: SocketAddr = format!("127.0.0.1:{a_server}").parse().expect("addr");
    let b_udp: SocketAddr = format!("127.0.0.1:{b_server}").parse().expect("addr");
    let probe = build_manager(
        directory.path(),
        vec![
            streamr_server_spec("pub-a", a_udp),
            streamr_server_spec("pub-b", b_udp),
        ],
    );
    probe.prepare().await.expect("probe prepare");
    let a_b64 = probe.service_destination_b64("pub-a").expect("a b64");
    let b_b64 = probe.service_destination_b64("pub-b").expect("b b64");
    assert_ne!(a_b64, b_b64);
    drop(probe);
    let manager = build_manager(
        directory.path(),
        vec![
            streamr_server_spec("pub-a", a_udp),
            streamr_server_spec("pub-b", b_udp),
            streamr_client_spec("sub-a", a_b64, {
                format!("127.0.0.1:{a_target}").parse().expect("addr")
            }),
            streamr_client_spec("sub-b", b_b64, {
                format!("127.0.0.1:{b_target}").parse().expect("addr")
            }),
        ],
    );
    let player_a = UdpSocket::bind(format!("127.0.0.1:{a_target}"))
        .await
        .expect("a binds");
    let player_b = UdpSocket::bind(format!("127.0.0.1:{b_target}"))
        .await
        .expect("b binds");
    let (_scope, _cancel) = start_supervisors(&manager).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let source = UdpSocket::bind("127.0.0.1:0").await.expect("source binds");
    let media_a = media_bytes(100);
    source.send_to(&media_a, a_udp).await.expect("send a");
    assert_eq!(
        recv_media(&player_a, Duration::from_secs(10))
            .await
            .expect("a arrives"),
        media_a
    );
    assert!(
        recv_media(&player_b, Duration::from_secs(3))
            .await
            .is_none(),
        "pair B never sees pair A media"
    );
}
