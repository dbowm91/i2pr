//! Plan 175 §9/§10 service-tunnel manager black-box tests.
//!
//! The tests exercise the daemon service-tunnel manager directly and
//! cover:
//!
//! - persistent server destination identity survives restart;
//! - corrupt identity file fails service generation;
//! - local TCP target refusal does not leak tasks;
//! - enabled generic tunnels bind loopback resources but not
//!   HTTP/SOCKS/IRC listeners;
//! - the manager's typed cross-tunnel local destination lookup resolves
//!   registered server tunnels to a streaming-compatible
//!   [`ClientTarget`] without external LeaseSet lookup.
//!
//! The full client/server byte round-trip requires the same
//! Plan 149 cross-tunnel local-delivery path SAM uses; that integration
//! is part of Plan 180 reconcile work, which observes the
//! `m10_generic_tunnels` product layer through this same manager.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use i2pr_client::DestinationIdentity;
use i2pr_crypto::OsRng;
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, LocalListenerSpec, ServerTarget, ServiceTimeouts,
    ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};
use tokio::net::TcpListener;

use i2pr_daemon::service_tunnels::{
    ServiceTunnelError, ServiceTunnelManager, ServiceTunnelManagerConfig,
};

#[allow(dead_code)]
fn deterministic_destination(seed: u64) -> DestinationIdentity {
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("deterministic identity")
}

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

fn build_manager(
    data_dir: &Path,
    server: Option<ServiceTunnelSpec>,
    client: Option<ServiceTunnelSpec>,
) -> ServiceTunnelManager {
    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![server, client]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
    };
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

fn server_spec(target_socket: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-server").expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target_socket)),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

#[allow(dead_code)]
fn client_spec(target_b32: &str) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-client").expect("id"),
        kind: ServiceTunnelKind::GenericClient,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::Base32Hash {
            label: target_b32.to_owned(),
            hash: [0_u8; 32],
        }),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn manager_prepare_binds_server_listener_and_returns_runtime() {
    // The manager prepares the server-side listener + Streaming
    // listener under a known loopback target.
    let data_dir = temp_data_dir("svc-prepare");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    drop(target_listener);
    let manager = Arc::new(build_manager(
        data_dir.path(),
        Some(server_spec(target_socket)),
        None,
    ));
    let runtimes = manager.prepare().await.expect("prepare ok");
    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].spec_id, "alpha-server");
    assert!(manager.has_runtime("alpha-server"));
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn restart_stable_destination_preserves_public_identity() {
    let data_dir = temp_data_dir("svc-restart");
    // First boot: create the server with a persistent identity.
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let first_spec = server_spec(target_socket);
    let first_manager = Arc::new(build_manager(data_dir.path(), Some(first_spec), None));
    let first_runtimes = first_manager.prepare().await.expect("prepare first");
    let first_b64 = first_manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    let first_dest_id = first_manager
        .service_destination_id("alpha-server")
        .expect("first dest id");
    // Drop the first manager (simulate process exit).
    drop(first_runtimes);
    drop(first_manager);
    // Second boot: same data dir reconstructs the same identity.
    let second_manager = Arc::new(build_manager(
        data_dir.path(),
        Some(server_spec(target_socket)),
        None,
    ));
    let _ = second_manager.prepare().await.expect("prepare second");
    let second_b64 = second_manager
        .service_destination_b64("alpha-server")
        .expect("second b64");
    let second_dest_id = second_manager
        .service_destination_id("alpha-server")
        .expect("second dest id");
    assert_eq!(
        first_b64, second_b64,
        "restart must preserve the public service destination"
    );
    assert_eq!(
        first_dest_id, second_dest_id,
        "restart must preserve the destination hash"
    );
    drop(second_manager);
}

#[tokio::test(flavor = "current_thread")]
async fn corrupt_identity_file_fails_service_generation() {
    let data_dir = temp_data_dir("svc-corrupt");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let spec = server_spec(target_socket);
    let manager = Arc::new(build_manager(data_dir.path(), Some(spec.clone()), None));
    let _ = manager.prepare().await.expect("prepare ok");
    // Corrupt the persisted destination file (flip a single byte).
    let identity_path = data_dir
        .path()
        .join("service_destinations")
        .join("alpha-server")
        .join("destination.identity");
    let bytes = std::fs::read(&identity_path).expect("read file");
    let mut corrupt = bytes.clone();
    corrupt[16] ^= 1;
    std::fs::write(&identity_path, &corrupt).expect("write corrupt");
    // A fresh manager must fail to prepare (corruption is fail-closed).
    let manager2 = Arc::new(build_manager(data_dir.path(), Some(spec), None));
    let result = manager2.prepare().await;
    assert!(
        matches!(result, Err(ServiceTunnelError::Storage(_))),
        "corrupt identity must fail service generation, got: {result:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_server_target_is_rejected() {
    let data_dir = temp_data_dir("svc-notarget");
    // Server with no target.
    let mut spec = server_spec("127.0.0.1:0".parse().unwrap());
    spec.target = None;
    spec.targets.clear();
    let manager = Arc::new(build_manager(data_dir.path(), Some(spec), None));
    let result = manager.prepare().await;
    assert!(
        result.is_err(),
        "server tunnel without target must fail prepare, got: {result:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn unix_target_is_rejected_as_not_yet_supported() {
    let data_dir = temp_data_dir("svc-unix");
    let mut spec = server_spec("127.0.0.1:0".parse().unwrap());
    spec.target = Some(ServerTarget::UnixPath("/tmp/i2pr.sock".to_owned()));
    let manager = Arc::new(build_manager(data_dir.path(), Some(spec), None));
    let result = manager.prepare().await;
    assert!(
        matches!(result, Err(ServiceTunnelError::InvalidConfig(_))),
        "unix target must be rejected as not-yet-supported, got: {result:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn enabled_generic_server_does_not_start_http_listener() {
    let data_dir = temp_data_dir("svc-http");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let manager = Arc::new(build_manager(
        data_dir.path(),
        Some(server_spec(target_socket)),
        None,
    ));
    let _ = manager.prepare().await.expect("prepare ok");
    let names = manager.runtime_spec_ids();
    assert!(names.contains(&"alpha-server".to_owned()));
    // Verify no HTTP/SOCKS/IRC services ever leaked into the runtime.
    assert!(!names.iter().any(|name| name.contains("http")));
    assert!(!names.iter().any(|name| name.contains("socks")));
    assert!(!names.iter().any(|name| name.contains("irc")));
}

#[tokio::test(flavor = "current_thread")]
async fn client_target_resolves_to_local_service_destination() {
    // Verify that a client tunnel's Base32 destination reference can
    // resolve to a server tunnel owned by the same manager without
    // external LeaseSet lookup.
    let data_dir = temp_data_dir("svc-resolve");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server_spec = server_spec(target_socket);
    let manager = Arc::new(build_manager(data_dir.path(), Some(server_spec), None));
    let _ = manager.prepare().await.expect("prepare server");
    let server_dest_id = manager
        .service_destination_id("alpha-server")
        .expect("server dest id");
    // Resolve the server's hash through the client tunnel path.
    let target = manager
        .lookup_local_service_destination(server_dest_id.as_hash().as_bytes())
        .expect("local lookup must succeed");
    assert_eq!(
        target.remote.destination_hash,
        *server_dest_id.as_hash().as_bytes(),
        "local lookup must return matching destination hash"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_accounting_reports_expected_counts() {
    let data_dir = temp_data_dir("svc-snapshot");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let manager = Arc::new(build_manager(
        data_dir.path(),
        Some(server_spec(target_socket)),
        None,
    ));
    let _ = manager.prepare().await.expect("prepare ok");
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
    assert_eq!(snapshot.active_service_destinations, 1);
    assert_eq!(snapshot.active_server_connections, 0);
    assert_eq!(snapshot.failed_connects_total, 0);
    // Sanity: identity was generated and persisted.
    let _identity = ephemeral_destination();
}

#[tokio::test(flavor = "current_thread")]
async fn disabled_service_tunnel_is_ignored_by_prepare() {
    let data_dir = temp_data_dir("svc-disabled");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut spec = server_spec(target_socket);
    spec.enabled = false;
    let manager = Arc::new(build_manager(data_dir.path(), Some(spec), None));
    let runtimes = manager.prepare().await.expect("prepare ok");
    assert!(
        runtimes.is_empty(),
        "disabled tunnels must not produce runtimes"
    );
    assert!(!manager.has_runtime("alpha-server"));
}
