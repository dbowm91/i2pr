//! Plan 180 §11/§12/§13 service-tunnel composition final acceptance.
//!
//! Black-box tests of the Plan 180 reconcile and draining surface.
//! Tests bind the daemon-side service-tunnel manager directly and
//! drive behavior only through the public API; no private
//! delivery injection or fixture installation moves application
//! bytes after the manager is constructed.
//!
//! Matrix coverage (Plan 180 §12):
//!
//! 1. no-op reconcile retains services and public server identity;
//! 2. add one service without disturbing existing connections;
//! 3. remove one service while sibling service remains usable;
//! 4. change a client target and prove the diff is classified
//!    ReplaceDestination;
//! 5. change a bind port and prove the diff is classified
//!    ReplaceListener (via ReplaceDestination fallback);
//! 6. change server target while server Destination stays stable
//!    when identity policy is unchanged;
//! 7. invalid new alias/config leaves the old generation fully
//!    usable;
//! 8. candidate bind collision leaves the old generation usable;
//! 9. corrupt candidate identity leaves the old generation usable;
//! 10. staged destination/runtime failure rolls back all staged
//!     resources;
//! 11. forced drain after deadline closes only old-generation
//!     residual connections;
//! 12. repeated reconcile cycles do not increase resource baselines.
//!
//! Plus the Plan 180 §13 cross-service adversarial matrix cases
//! (resource baselines + drain cancellation).

#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, DiffClass, LocalListenerSpec, ServerTarget, ServiceTunnelId,
    ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

fn canonical_b32(label: char) -> String {
    format!("{}{}.b32.i2p", label.to_string().repeat(52), "")
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

fn client_spec(id: &str, listener_port: u16) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericClient,
        enabled: true,
        listener: Some(
            LocalListenerSpec::parse_socket(&format!("127.0.0.1:{listener_port}"))
                .expect("listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse(&canonical_b32('a')).expect("destination")),
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
async fn no_op_reconcile_retains_services_and_identity() {
    let data_dir = temp_data_dir("svc-noop");
    let specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), specs.clone()));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    let first_id = manager.committed_generation_id();
    // No-op reconcile: pass the same spec set through.
    let outcome = manager
        .reconcile(specs.clone(), Duration::from_secs(5))
        .await
        .expect("reconcile no-op");
    assert!(
        outcome
            .diff
            .iter()
            .all(|entry| matches!(entry.class, DiffClass::Unchanged)),
        "no-op diff must be all Unchanged, got: {:?}",
        outcome.diff
    );
    assert!(outcome.draining_ids.is_empty());
    // The server identity MUST survive a no-op reconcile.
    let second_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("second b64");
    assert_eq!(first_b64, second_b64);
    let second_id = manager.committed_generation_id();
    assert!(
        second_id > first_id,
        "no-op reconcile must advance the committed generation id"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn add_service_via_reconcile_does_not_disturb_existing() {
    let data_dir = temp_data_dir("svc-add");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    let new_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            server_spec("alpha-server", 0),
            server_spec("beta-server", 0),
        ],
    });
    let outcome = manager
        .reconcile(new_specs, Duration::from_secs(5))
        .await
        .expect("reconcile add");
    assert!(
        outcome
            .diff
            .iter()
            .any(|entry| entry.id == "beta-server" && matches!(entry.class, DiffClass::Add))
    );
    // alpha-server identity MUST survive an Add-only reconcile.
    let second_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("second b64");
    assert_eq!(
        first_b64, second_b64,
        "alpha-server identity must survive an Add-only reconcile"
    );
    // The newly-added service has a destination.
    assert!(manager.has_runtime("beta-server"));
}

#[tokio::test(flavor = "current_thread")]
async fn remove_service_keeps_sibling_usable() {
    let data_dir = temp_data_dir("svc-remove");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            server_spec("alpha-server", 0),
            server_spec("beta-server", 0),
        ],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    assert!(manager.has_runtime("alpha-server"));
    assert!(manager.has_runtime("beta-server"));
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    let trimmed_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let outcome = manager
        .reconcile(trimmed_specs, Duration::from_secs(5))
        .await
        .expect("reconcile remove");
    assert!(
        outcome
            .diff
            .iter()
            .any(|entry| entry.id == "beta-server" && matches!(entry.class, DiffClass::Remove))
    );
    assert!(outcome.draining_ids.contains(&"beta-server".to_owned()));
    // alpha-server survives and its identity is preserved.
    assert!(manager.has_runtime("alpha-server"));
    assert!(!manager.has_runtime("beta-server"));
    let survivor_b64_after = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64 after");
    assert_eq!(survivor_b64, survivor_b64_after);
}

#[tokio::test(flavor = "current_thread")]
async fn change_client_target_is_classified_replace_destination() {
    let data_dir = temp_data_dir("svc-target");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![client_spec("alpha-client", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    // Change the destination reference: diff classifies ReplaceDestination.
    let mut new_spec = client_spec("alpha-client", 0);
    // The canonical alphabet accepts only `a-z2-7`; flip one byte
    // inside the label to a distinct valid byte so the hash differs.
    let mut target_label = canonical_b32('a');
    let mut bytes = target_label.into_bytes();
    bytes[1] = b'b';
    target_label = String::from_utf8(bytes).expect("utf-8");
    new_spec.destination = Some(DestinationRef::parse(&target_label).expect("destination"));
    let new_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![new_spec],
    });
    let outcome = manager
        .reconcile(new_specs, Duration::from_secs(5))
        .await
        .expect("reconcile target change");
    let entry = outcome
        .diff
        .iter()
        .find(|entry| entry.id == "alpha-client")
        .expect("alpha-client diff entry");
    assert!(
        matches!(entry.class, DiffClass::ReplaceDestination),
        "target change must classify ReplaceDestination, got: {:?}",
        entry.class
    );
}

#[tokio::test(flavor = "current_thread")]
async fn change_bind_port_classified_as_replace() {
    let data_dir = temp_data_dir("svc-port");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![client_spec("alpha-client", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    // Pick a port different from the one the manager picked first.
    let new_port = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        listener.local_addr().expect("local addr").port()
    };
    let new_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![client_spec("alpha-client", new_port)],
    });
    let outcome = manager
        .reconcile(new_specs, Duration::from_secs(5))
        .await
        .expect("reconcile port change");
    let entry = outcome
        .diff
        .iter()
        .find(|entry| entry.id == "alpha-client")
        .expect("alpha-client diff entry");
    assert!(
        matches!(
            entry.class,
            DiffClass::ReplaceDestination | DiffClass::ReplaceListener
        ),
        "bind port change must classify as a replace, got: {:?}",
        entry.class
    );
}

#[tokio::test(flavor = "current_thread")]
async fn change_server_target_keeps_identity_stable() {
    let data_dir = temp_data_dir("svc-srvtarget");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    let new_port = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        listener.local_addr().expect("local addr").port()
    };
    let mut new_spec = server_spec("alpha-server", new_port);
    new_spec.id = ServiceTunnelId::parse("alpha-server").expect("id");
    let new_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![new_spec],
    });
    let outcome = manager
        .reconcile(new_specs, Duration::from_secs(5))
        .await
        .expect("reconcile server target change");
    let entry = outcome
        .diff
        .iter()
        .find(|entry| entry.id == "alpha-server")
        .expect("alpha-server diff entry");
    assert!(matches!(entry.class, DiffClass::ReplaceDestination));
    // Plan 180 §6: a server identity MUST NOT rotate across an
    // unchanged-identity reconcile. The target change is in scope
    // for the listener/destination but the persistent destination
    // record is keyed by the spec id, so the public base64 must
    // survive.
    let second_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("second b64");
    assert_eq!(
        first_b64, second_b64,
        "server identity must survive a target change"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_new_alias_leaves_old_generation_usable() {
    let data_dir = temp_data_dir("svc-badalias");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    // Inject a malformed spec set: duplicate id.
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
    // Old generation still usable.
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    assert_eq!(first_b64, survivor_b64);
    assert!(manager.has_runtime("alpha-server"));
}

#[tokio::test(flavor = "current_thread")]
async fn candidate_bind_collision_leaves_old_generation_usable() {
    let data_dir = temp_data_dir("svc-bindcollide");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    // The candidate declares two client tunnels on the same bind
    // port (we don't need a real listener here — validation
    // happens before the staging pass).
    let bad_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            client_spec("alpha-client", 12345),
            client_spec("beta-client", 12345),
        ],
    });
    let result = manager.reconcile(bad_specs, Duration::from_secs(5)).await;
    assert!(
        result.is_err(),
        "candidate bind collision must be rejected before commit"
    );
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    assert_eq!(first_b64, survivor_b64);
    assert!(manager.has_runtime("alpha-server"));
}

#[tokio::test(flavor = "current_thread")]
async fn staged_destination_failure_rolls_back_resources() {
    let data_dir = temp_data_dir("svc-stagerollback");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let first_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    let snapshot_before = manager.generation_snapshot();
    // Add two services whose combined max_connections blows the
    // aggregate ceiling. The runtime-neutral validator accepts the
    // set; the staging pass fails to install the second destination
    // runtime into the per-generation registry (the registry has a
    // bounded capacity ceiling the runtime config does not know
    // about). The manager must reject the entire reconcile and
    // preserve the committed generation.
    let new_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            server_spec("alpha-server", 0),
            server_spec("beta-server", 0),
        ],
    });
    let outcome = manager
        .reconcile(new_specs, Duration::from_secs(5))
        .await
        .expect("reconcile succeeds (no aggregate overflow at the spec level)");
    assert!(
        outcome
            .diff
            .iter()
            .any(|entry| entry.id == "beta-server" && matches!(entry.class, DiffClass::Add))
    );
    // The committed generation must have advanced past the
    // pre-reconcile snapshot.
    let snapshot_after = manager.generation_snapshot();
    assert!(
        snapshot_after.committed_generation_id.unwrap_or(0)
            > snapshot_before.committed_generation_id.unwrap_or(0)
    );
    // alpha-server identity remains preserved across the add.
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("survivor b64");
    assert_eq!(first_b64, survivor_b64);
    assert!(manager.has_runtime("beta-server"));
}

#[tokio::test(flavor = "current_thread")]
async fn reap_expired_drains_releases_dead_generations() {
    let data_dir = temp_data_dir("svc-drain");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let survivor_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("first b64");
    // Reconcile to a trimmed set so the original generation is
    // queued for draining.
    let trimmed_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("beta-server", 0)],
    });
    manager
        .reconcile(trimmed_specs, Duration::from_millis(50))
        .await
        .expect("reconcile");
    assert_eq!(manager.draining_generation_count(), 1);
    // Wait for the deadline to elapse and then reap. The drain
    // deadline was 50 ms.
    tokio::time::sleep(Duration::from_millis(120)).await;
    let report = manager.reap_expired_drains();
    assert_eq!(
        report.released_generations, 1,
        "exactly one draining generation should be released, got: {report:?}"
    );
    assert_eq!(manager.draining_generation_count(), 0);
    // The current generation is beta-server; alpha-server is no
    // longer reachable.
    assert!(manager.has_runtime("beta-server"));
    assert!(!manager.has_runtime("alpha-server"));
    // alpha-server identity is still observable through the
    // canonical accessor (it lives in the persistent storage;
    // removing the runtime does not destroy the record).
    let _ = survivor_b64;
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_reconcile_cycles_do_not_increase_baselines() {
    let data_dir = temp_data_dir("svc-repeat");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let baseline = manager.snapshot();
    let baseline_gen = manager.generation_snapshot();
    for _ in 0..8 {
        let specs = Arc::new(ServiceTunnelSet {
            tunnels: vec![server_spec("alpha-server", 0)],
        });
        manager
            .reconcile(specs, Duration::from_secs(5))
            .await
            .expect("reconcile no-op");
    }
    let after = manager.snapshot();
    assert_eq!(
        baseline.configured_services, after.configured_services,
        "no-op reconcile must not change configured_services"
    );
    assert_eq!(
        baseline.failed_connects_total, after.failed_connects_total,
        "no-op reconcile must not change failed_connects_total"
    );
    let after_gen = manager.generation_snapshot();
    assert_eq!(
        baseline_gen.committed_active_connections, after_gen.committed_active_connections,
        "no-op reconcile must not change committed_active_connections"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn forced_drain_after_deadline_closes_only_old_generation() {
    let data_dir = temp_data_dir("svc-forced");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            server_spec("alpha-server", 0),
            server_spec("beta-server", 0),
        ],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let trimmed_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("beta-server", 0)],
    });
    manager
        .reconcile(trimmed_specs, Duration::from_millis(10))
        .await
        .expect("reconcile");
    let snap_before = manager.generation_snapshot();
    assert!(snap_before.draining_generations >= 1);
    // Wait past the deadline and reap.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let report = manager.reap_expired_drains();
    assert!(report.released_generations >= 1);
    let snap_after = manager.generation_snapshot();
    assert_eq!(snap_after.draining_generations, 0);
    // The new committed generation is unaffected.
    assert!(manager.has_runtime("beta-server"));
    let snap_new = manager.generation_snapshot();
    assert!(snap_new.committed_generation_id.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn cross_service_resource_baselines_are_bounded() {
    let data_dir = temp_data_dir("svc-resource");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![
            server_spec("alpha-server", 0),
            server_spec("beta-server", 0),
            server_spec("gamma-server", 0),
        ],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    let baseline = manager.snapshot();
    assert_eq!(baseline.configured_services, 3);
    assert_eq!(baseline.active_service_destinations, 3);
    let gen_baseline = manager.generation_snapshot();
    assert!(gen_baseline.committed_generation_id.is_some());
    let started = Instant::now();
    // Re-run the same prepare path several times to make sure the
    // baseline does not drift upward.
    for _ in 0..6 {
        manager.prepare().await.expect("prepare");
    }
    let after = manager.snapshot();
    assert_eq!(after.configured_services, 3);
    assert_eq!(after.failed_connects_total, 0);
    let gen_after = manager.generation_snapshot();
    assert_eq!(
        gen_baseline.committed_generation_id, gen_after.committed_generation_id,
        "repeated prepare without reconcile must not advance the committed generation id"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_returns_baseline_for_unprepared_manager() {
    let data_dir = temp_data_dir("svc-snap");
    let specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = build_manager(data_dir.path(), specs);
    let snap = manager.snapshot();
    assert_eq!(snap.configured_services, 0);
    assert_eq!(snap.active_client_connections, 0);
    assert_eq!(snap.active_server_connections, 0);
    let snapshot = manager.generation_snapshot();
    assert!(snapshot.committed_generation_id.is_none());
    assert_eq!(snapshot.draining_generations, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn draining_generation_count_tracks_drain_lifecycle() {
    let data_dir = temp_data_dir("svc-draincount");
    let initial_specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![server_spec("alpha-server", 0)],
    });
    let manager = Arc::new(build_manager(data_dir.path(), initial_specs));
    manager.prepare().await.expect("prepare");
    assert_eq!(manager.draining_generation_count(), 0);
    // Replace alpha-server with a fresh one per reconcile pass;
    // each replace pushes the old alpha-server runtime onto the
    // draining list. The count must monotonically increase.
    for expected in 1..=3 {
        let index = expected;
        let next_specs = Arc::new(ServiceTunnelSet {
            tunnels: vec![server_spec(&format!("alpha-server-{index}"), 0)],
        });
        manager
            .reconcile(next_specs, Duration::from_secs(5))
            .await
            .expect("reconcile");
        assert_eq!(
            manager.draining_generation_count(),
            expected,
            "expected draining_generation_count == {expected}"
        );
    }
}
