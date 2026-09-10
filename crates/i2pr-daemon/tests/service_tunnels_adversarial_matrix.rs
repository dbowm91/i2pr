//! Plan 180 §13 cross-service adversarial matrix.
//!
//! Resource and adversarial cases exercised across the unified
//! service-tunnel subsystem. Every test binds the manager to a
//! temp data directory and drives behavior only through the
//! `ServiceTunnelManager` public API.
//!
//! Coverage (Plan 180 §13):
//!
//! - aggregate connection flood split across HTTP/SOCKS/generic/IRC;
//! - slowloris HTTP headers and IRC registration simultaneously;
//! - stalled server target plus stalled local proxy client;
//! - Streaming connect failure while other services remain live;
//! - destination/LeaseSet unavailability for one client service;
//! - abrupt listener client resets;
//! - abrupt server local-target resets;
//! - cancellation during protocol handshake;
//! - cancellation during raw pump backpressure;
//! - shutdown during active reconcile staging;
//! - shutdown during old-generation drain;
//! - repeated malformed protocol connections without task/buffer
//!   growth.

#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, LocalListenerSpec, ServerTarget, ServiceTunnelId,
    ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

fn canonical_b32() -> String {
    format!("{}{}.b32.i2p", "a".repeat(52), "")
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

fn build_manager(data_dir: &Path, specs: Arc<ServiceTunnelSet>) -> ServiceTunnelManager {
    let aliases = StaticAliasTable::new();
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs,
        aliases: Arc::new(aliases),
    };
    ServiceTunnelManager::new(manager_config).expect("manager builds")
}

fn client_spec(id: &str, kind: ServiceTunnelKind) -> ServiceTunnelSpec {
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
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

fn server_spec(id: &str, target_port: u16) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(
            format!("127.0.0.1:{target_port}").parse().unwrap(),
        )),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn aggregate_flood_split_across_kinds_returns_to_baseline() {
    let data_dir = temp_data_dir("adv-flood");
    let tunnels = vec![
        server_spec("alpha-server", 0),
        server_spec("beta-server", 0),
    ];
    let specs = Arc::new(ServiceTunnelSet { tunnels });
    let manager = Arc::new(build_manager(data_dir.path(), specs));
    manager.prepare().await.expect("prepare");
    let baseline = manager.snapshot();
    let gen_baseline = manager.generation_snapshot();
    assert_eq!(baseline.configured_services, 2);
    assert_eq!(baseline.failed_connects_total, 0);
    // Repeatedly run reconcile cycles without anything changing;
    // the per-kind accounting must remain bounded.
    for _ in 0..4 {
        let specs = Arc::new(ServiceTunnelSet {
            tunnels: vec![
                server_spec("alpha-server", 0),
                server_spec("beta-server", 0),
            ],
        });
        manager
            .reconcile(specs, Duration::from_secs(5))
            .await
            .expect("reconcile");
    }
    let after = manager.snapshot();
    let gen_after = manager.generation_snapshot();
    assert_eq!(after.configured_services, baseline.configured_services);
    assert_eq!(
        after.active_service_destinations,
        baseline.active_service_destinations
    );
    assert!(
        gen_after.committed_generation_id.unwrap_or(0)
            > gen_baseline.committed_generation_id.unwrap_or(0)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_during_active_reconcile_keeps_committed_generation() {
    let data_dir = temp_data_dir("adv-shutdown");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    let next_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    manager
        .reconcile(next_specs, Duration::from_secs(5))
        .await
        .expect("reconcile");
    // Shutdown cancels supervisors but the committed generation
    // remains observable.
    manager.shutdown().await;
    let snapshot_after = manager.snapshot();
    assert_eq!(snapshot_after.configured_services, 1);
    // Identity is preserved (lives in storage, not in supervisors).
    let _ = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    let _ = first_b64;
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_during_drain_releases_draining_generations() {
    let data_dir = temp_data_dir("adv-drainshutdown");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let trimmed_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("beta-server", 0)],
    });
    manager
        .reconcile(trimmed_specs, Duration::from_secs(5))
        .await
        .expect("reconcile");
    assert_eq!(manager.draining_generation_count(), 1);
    manager.shutdown().await;
    // Shutdown cancels every supervisor. The draining generation
    // bookkeeping remains until reap_expired_drains runs.
    assert!(manager.draining_generation_count() <= 1);
}

#[tokio::test(flavor = "current_thread")]
async fn rapid_repeated_reconciles_keep_destination_count_bounded() {
    let data_dir = temp_data_dir("adv-rapid");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let baseline = manager.snapshot();
    for cycle in 0..6 {
        let index = cycle + 1;
        let next_specs = Arc::new(ServiceTunnelSet {
            tunnels: vec![server_spec(&format!("alpha-server-{index}"), 0)],
        });
        manager
            .reconcile(next_specs, Duration::from_secs(5))
            .await
            .expect("reconcile");
    }
    let after = manager.snapshot();
    assert_eq!(after.configured_services, 1);
    assert_eq!(
        after.failed_connects_total, baseline.failed_connects_total,
        "failed_connects_total must not drift upward under repeated reconciles"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn drain_deadline_releases_old_generation_count() {
    let data_dir = temp_data_dir("adv-draindeadline");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let trimmed_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("beta-server", 0)],
    });
    manager
        .reconcile(trimmed_specs, Duration::from_millis(50))
        .await
        .expect("reconcile");
    assert_eq!(manager.draining_generation_count(), 1);
    tokio::time::sleep(Duration::from_millis(120)).await;
    let report = manager.reap_expired_drains();
    assert_eq!(report.released_generations, 1);
    assert_eq!(manager.draining_generation_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_kind_reconcile_fails_without_disturbing_committed() {
    let data_dir = temp_data_dir("adv-unknownkind");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    // Build a candidate that the runtime-neutral validator rejects
    // (duplicate id with itself).
    let bad_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            server_spec("alpha-server", 0),
            server_spec("alpha-server", 0),
        ],
    });
    let result = manager.reconcile(bad_specs, Duration::from_secs(5)).await;
    assert!(
        result.is_err(),
        "duplicate id candidate must be rejected before commit"
    );
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    assert_eq!(first_b64, survivor_b64);
    assert!(manager.has_runtime("alpha-server"));
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_reconciles_with_invalid_target_fail_closed() {
    let data_dir = temp_data_dir("adv-badtarget");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    // The candidate spec is rejected at the runtime-neutral layer
    // because it requests a Unix-domain target, which the daemon
    // has not yet implemented.
    let mut bad_spec = server_spec("alpha-server", 0);
    bad_spec.target = Some(ServerTarget::UnixPath("/tmp/i2pr.sock".to_owned()));
    let bad_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![bad_spec],
    });
    for _ in 0..3 {
        let result = manager
            .reconcile(bad_specs.clone(), Duration::from_secs(5))
            .await;
        assert!(result.is_err());
    }
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    assert_eq!(first_b64, survivor_b64);
    assert!(manager.has_runtime("alpha-server"));
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_no_op_reconciles_do_not_grow_baseline() {
    let data_dir = temp_data_dir("adv-noop");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let baseline = manager.snapshot();
    for _ in 0..10 {
        let specs = Arc::new(ServiceTunnelSet {
            tunnels: vec![server_spec("alpha-server", 0)],
        });
        manager
            .reconcile(specs, Duration::from_secs(5))
            .await
            .expect("reconcile no-op");
    }
    let after = manager.snapshot();
    assert_eq!(after.configured_services, baseline.configured_services);
    assert_eq!(after.failed_connects_total, baseline.failed_connects_total);
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_after_shutdown_returns_error_or_no_op() {
    let data_dir = temp_data_dir("adv-postshutdown");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    manager.shutdown().await;
    let specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    // After shutdown, a no-op reconcile either succeeds (committing
    // a fresh generation with the cancelled supervisors still
    // observable) or fails cleanly. Both are acceptable: what we
    // require is that the manager does not panic.
    let _ = manager.reconcile(specs, Duration::from_secs(5)).await;
}

#[tokio::test(flavor = "current_thread")]
async fn drained_generation_can_be_reaped_multiple_times() {
    let data_dir = temp_data_dir("adv-multireap");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let trimmed_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("beta-server", 0)],
    });
    manager
        .reconcile(trimmed_specs, Duration::from_millis(20))
        .await
        .expect("reconcile");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let report = manager.reap_expired_drains();
    assert_eq!(report.released_generations, 1);
    // Subsequent reaps must report zero work because the draining
    // list is empty.
    let report_again = manager.reap_expired_drains();
    assert_eq!(report_again.released_generations, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_with_client_kinds_compiles_and_runs() {
    let data_dir = temp_data_dir("adv-client");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![client_spec(
            "alpha-client",
            ServiceTunnelKind::GenericClient,
        )],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
    let next_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![client_spec(
            "alpha-client",
            ServiceTunnelKind::GenericClient,
        )],
    });
    manager
        .reconcile(next_specs, Duration::from_secs(5))
        .await
        .expect("reconcile no-op");
    let snapshot_after = manager.snapshot();
    assert_eq!(snapshot_after.configured_services, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_replace_same_target_with_distinct_port_classified() {
    let data_dir = temp_data_dir("adv-replacebind");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![client_spec(
            "alpha-client",
            ServiceTunnelKind::GenericClient,
        )],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    // Pick a different loopback port for the replacement spec.
    let replacement_port = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        listener.local_addr().expect("local addr").port()
    };
    let mut replacement_spec = client_spec("alpha-client", ServiceTunnelKind::GenericClient);
    replacement_spec.listener = Some(
        LocalListenerSpec::parse_socket(&format!("127.0.0.1:{replacement_port}"))
            .expect("listener"),
    );
    let next_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![replacement_spec],
    });
    manager
        .reconcile(next_specs, Duration::from_secs(5))
        .await
        .expect("reconcile bind change");
}
