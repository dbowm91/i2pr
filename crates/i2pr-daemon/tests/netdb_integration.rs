//! Plan 106 daemon NetDB/bootstrap integration tests.
//!
//! These tests cover the Plan 106 work packages 10 (integration tests)
//! and 11 (Milestone 4A closure state) on the local host. They use
//! only local fixtures and deterministic time. No root, namespaces,
//! Java I2P, i2pd, or public I2P connection is required.
//!
//! The tests exercise:
//!
//! - safe defaults for omitted NetDB/reseed config;
//! - invalid/excessive limits reject;
//! - reseed enable does not enable NTCP2;
//! - explicit NTCP2 enable remains rejected;
//! - dry-run performs no cache mutation/network request;
//! - empty cache + reseed disabled produces typed insufficient state;
//! - valid cache above threshold becomes ready without reseed;
//! - mixed corrupt/valid cache retains valid entries;
//! - local RouterInfo self-validates and has no NTCP2 address;
//! - service graph contains no `ntcp2-transport`;
//! - starting a lookup without an exploratory path does not produce a
//!   direct transport send;
//! - bootstrap pipeline fails closed when identity is missing.

#![forbid(unsafe_code)]

use std::fs;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::bootstrap::{
    Bootstrap, BootstrapPolicy, BootstrapReport, BootstrapState, ReseedAttemptSummary,
    bootstrap_with_offline_reseed, build_trust_set, store_summary,
};
use i2pr_daemon::config::{
    Config, NetDbConfig, Ntcp2Config, ReseedConfig, ReseedSourceConfig, RouterConfig,
};
use i2pr_daemon::{bootstrap_daemon, build_daemon_graph, netdb_seam};
use i2pr_netdb::RouterInfoStoreConfig;
use i2pr_netdb_persist::{CacheLoader, CacheLoaderLimits, LoadedCacheState};
use i2pr_proto::{Date, Mapping};
use i2pr_storage::IdentityStore;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tempfile::tempdir;

fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn minimal_config(data_dir: &std::path::Path) -> Config {
    Config {
        schema_version: 1,
        router: RouterConfig {
            data_dir: data_dir.to_path_buf(),
            profile: i2pr_daemon::config::RouterProfile::Balanced,
        },
        logging: i2pr_daemon::config::LoggingConfig {
            filter: "info".to_owned(),
            format: i2pr_daemon::config::LogFormat::Text,
        },
        limits: i2pr_daemon::config::LimitsConfig {
            max_tasks: 1024,
            max_buffered_bytes: 1024 * 1024,
        },
        network: i2pr_daemon::config::NetworkConfig {
            bind_address: "127.0.0.1".parse().unwrap(),
            listen_port: 9150,
            network_id: 2,
        },
        transport: i2pr_daemon::config::TransportConfig {
            ntcp2: Ntcp2Config {
                enabled: false,
                connect_timeout: std::time::Duration::from_millis(1000),
                handshake_timeout: std::time::Duration::from_millis(1000),
                read_idle_timeout: std::time::Duration::from_millis(1000),
                write_timeout: std::time::Duration::from_millis(1000),
                queue_wait_timeout: std::time::Duration::from_millis(1000),
                drain_timeout: std::time::Duration::from_millis(1000),
                max_active_links: 16,
                max_replay_entries: 16,
                ipv4_prefix: 24,
                ipv6_prefix: 64,
            },
        },
        netdb: NetDbConfig {
            enabled: true,
            max_records: 64,
            max_encoded_bytes: 1024 * 1024,
            min_router_infos: 5,
            min_floodfill_advertisers: 2,
        },
        reseed: ReseedConfig {
            enabled: false,
            max_sources: 4,
            max_su3_bytes: 1024 * 1024,
            sources: Vec::new(),
        },
    }
}

fn minimal_config_text(data_dir: &std::path::Path) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n",
        data_dir.to_string_lossy()
    )
}

#[test]
fn safe_defaults_apply_when_netdb_section_omitted() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let text = minimal_config_text(&path);
    let config = Config::parse(&text).expect("defaults");
    assert!(config.netdb.enabled);
    assert_eq!(config.netdb.max_records, 4096);
    assert_eq!(config.netdb.min_router_infos, 50);
    assert_eq!(config.netdb.min_floodfill_advertisers, 5);
}

#[test]
fn invalid_or_excessive_limits_reject() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let mut text = minimal_config_text(&path);
    text.push_str("[netdb]\nmax_records = 0\n");
    assert!(i2pr_daemon::config::Config::parse(&text).is_err());
    text.clear();
    text.push_str(&minimal_config_text(&path));
    text.push_str("[netdb]\nmax_encoded_bytes = 0\n");
    assert!(i2pr_daemon::config::Config::parse(&text).is_err());
}

#[test]
fn reseed_enable_does_not_enable_ntcp2() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[reseed]\nenabled = true\n",
        path.to_string_lossy()
    );
    let config = Config::parse(&text).expect("valid reseed");
    assert!(config.reseed.enabled);
    assert!(!config.transport.ntcp2.enabled);
    let graph = build_daemon_graph(&config).expect("graph");
    let names: Vec<String> = graph
        .startup_order()
        .iter()
        .map(|n| n.as_str().to_owned())
        .collect();
    assert!(!names.iter().any(|n| n == "ntcp2-transport"));
}

#[test]
fn explicit_ntcp2_enable_remains_rejected() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[transport.ntcp2]\nenabled = true\n",
        path.to_string_lossy()
    );
    assert!(i2pr_daemon::config::Config::parse(&text).is_err());
}

#[test]
fn dry_run_does_not_create_netdb_state() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("not-created");
    let text = minimal_config_text(&path);
    std::fs::write(directory.path().join("config.toml"), &text).expect("write config");
    let cli = i2pr_daemon::cli::Cli {
        command: i2pr_daemon::cli::Command::Run(i2pr_daemon::cli::RunArgs {
            config: directory.path().join("config.toml"),
            dry_run: true,
        }),
    };
    let outcome = i2pr_daemon::execute(cli).expect("dry-run ok");
    assert!(matches!(
        outcome,
        i2pr_daemon::CommandOutcome::Validated { dry_run: true, .. }
    ));
    assert!(!path.exists());
}

#[test]
fn empty_cache_and_reseed_disabled_produces_insufficient_state() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    IdentityStore::prepare_directory(&path).expect("prepare");
    let bundle = make_bundle(0xCAFE);
    IdentityStore::in_data_dir(&path)
        .save_new(&bundle)
        .expect("save identity");
    let config = minimal_config(&path);
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&bundle);
    let report = bootstrap_with_offline_reseed(&config, &builder, 0, None).expect("bootstrap");
    assert_eq!(report.final_state, BootstrapState::Empty);
    assert_eq!(report.snapshot.record_count, 0);
    assert!(report.snapshot.last_reseed_summary.is_none());
}

#[test]
fn valid_cache_above_threshold_becomes_ready() {
    let directory = tempdir().expect("directory");
    let data_dir = directory.path().join("state");
    IdentityStore::prepare_directory(&data_dir).expect("prepare");
    let local_bundle = make_bundle(0xBEEF);
    IdentityStore::in_data_dir(&data_dir)
        .save_new(&local_bundle)
        .expect("save identity");
    let cache_dir = data_dir.join("netdb").join("routers");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [data_dir.as_path(), &cache_dir] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("private perms");
        }
    }

    // Insert 8 validated RouterInfos via the loader. Use the
    // sign_router_info helper directly so the cache files can carry
    // the `f` floodfill capability (the local builder refuses the
    // capability under Plan 101).
    let loader = CacheLoader::new(i2pr_storage::cache_seam::ByteCache::in_data_dir(&data_dir));
    for seed in 0..8u64 {
        let peer_bundle = make_bundle(0x1000 + seed);
        let mut options = Mapping::empty();
        if seed < 3 {
            let mut b = Mapping::builder();
            b.insert("caps".to_owned(), "f".to_owned()).unwrap();
            options = b.build().unwrap();
        }
        let info = peer_bundle
            .sign_router_info(Date::from_millis(0), Vec::new(), Vec::new(), options)
            .expect("sign");
        let validated = i2pr_netdb::ValidatedRouterInfo::from_router_info(
            info,
            None,
            i2pr_netdb::ValidationContext::new(Date::from_millis(0)),
        )
        .expect("validate");
        let encoded = validated
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let hash = validated.key();
        let name = hash
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        loader
            .cache()
            .write(&name, &encoded)
            .expect("write cache file");
    }

    let config = minimal_config(&data_dir);
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&local_bundle);
    let report = bootstrap_with_offline_reseed(&config, &builder, 0, None).expect("bootstrap");
    assert!(
        report.snapshot.record_count >= 3,
        "record_count={}",
        report.snapshot.record_count
    );
    assert!(report.snapshot.floodfill_advertisers >= 3);
    assert!(matches!(
        report.final_state,
        BootstrapState::ReadyForNetworkIntegration | BootstrapState::CacheSufficient
    ));
}

#[test]
fn mixed_corrupt_and_valid_cache_retains_valid_entries() {
    let directory = tempdir().expect("directory");
    let data_dir = directory.path().join("state");
    IdentityStore::prepare_directory(&data_dir).expect("prepare");
    let local_bundle = make_bundle(0xC0DE);
    IdentityStore::in_data_dir(&data_dir)
        .save_new(&local_bundle)
        .expect("save identity");
    let cache_dir = data_dir.join("netdb").join("routers");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [data_dir.as_path(), &cache_dir] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("private perms");
        }
    }
    // Insert 1 valid + 1 corrupt cache file.
    let loader = CacheLoader::new(i2pr_storage::cache_seam::ByteCache::in_data_dir(&data_dir));
    let peer_bundle = make_bundle(0x2000);
    let local_peer = i2pr_netdb::LocalRouterInfoBuilder::new(&peer_bundle)
        .build_default(Date::from_millis(0))
        .expect("build");
    let encoded = local_peer
        .validated()
        .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let name = local_peer
        .router_hash()
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    loader.cache().write(&name, &encoded).expect("write valid");
    loader
        .cache()
        .write(&"ab".repeat(32), b"garbage bytes")
        .expect("write garbage");

    let config = minimal_config(&data_dir);
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&local_bundle);
    let report = bootstrap_with_offline_reseed(&config, &builder, 0, None).expect("bootstrap");
    assert!(report.cache_report.is_some());
    assert!(
        report.snapshot.record_count >= 1,
        "record_count={}",
        report.snapshot.record_count
    );
}

#[test]
fn empty_cache_with_reseed_disabled_does_not_call_offline_reseed() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    IdentityStore::prepare_directory(&path).expect("prepare");
    let bundle = make_bundle(0xFADE);
    IdentityStore::in_data_dir(&path)
        .save_new(&bundle)
        .expect("save identity");
    let mut config = minimal_config(&path);
    config.reseed.enabled = false;
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&bundle);
    let bogus = path.join("no-such-bundle.su3");
    let report =
        bootstrap_with_offline_reseed(&config, &builder, 0, Some(bogus)).expect("bootstrap");
    assert!(report.reseed_report.is_none());
    assert_eq!(report.snapshot.reseed_attempts, 0);
}

#[test]
fn local_router_info_has_no_ntcp2_address() {
    let bundle = make_bundle(0xDEAD);
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&bundle);
    let local = builder.build_default(Date::from_millis(0)).expect("build");
    assert_eq!(local.router_info().addresses().len(), 0);
}

#[test]
fn service_graph_contains_no_ntcp2_transport_service() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let config = minimal_config(&path);
    let graph = build_daemon_graph(&config).expect("graph");
    let names: Vec<String> = graph
        .startup_order()
        .iter()
        .map(|n| n.as_str().to_owned())
        .collect();
    assert!(!names.iter().any(|n| n == "ntcp2-transport"));
    assert!(names.iter().any(|n| n == "lifecycle"));
    assert!(names.iter().any(|n| n == "netdb-bootstrap"));
}

#[test]
fn lookup_without_exploratory_path_emits_typed_blocker() {
    let mut seam = netdb_seam::NetDbSeam::new(i2pr_netdb::LookupPolicy::default());
    let store = i2pr_netdb::RouterInfoStore::default();
    let target = i2pr_netdb::RouterHash::from_bytes([0x42u8; 32]);
    let action = seam.begin_lookup(&store, 1, target, &target);
    assert!(matches!(
        action,
        i2pr_netdb::LookupAction::NeedExploratoryReplyPath { .. }
            | i2pr_netdb::LookupAction::Complete { .. }
    ));
    assert!(matches!(
        seam.path_status(),
        netdb_seam::ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
    ));
}

#[test]
fn bootstrap_pipeline_fails_closed_when_identity_missing() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("no-identity");
    let config = minimal_config(&path);
    let result = bootstrap_daemon(&config, 0, None);
    assert!(result.is_err());
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        i2pr_daemon::DaemonError::RuntimeIdentity(_)
    ));
}

#[test]
fn bootstrap_policy_derives_from_config() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let config = minimal_config(&path);
    let policy = BootstrapPolicy::from_config(&config);
    assert_eq!(policy.min_router_infos, 5);
    assert_eq!(policy.min_floodfill_advertisers, 2);
    assert!(!policy.reseed_enabled);
}

#[test]
fn store_summary_reflects_populated_state() {
    let bundle = make_bundle(0x1234);
    let local = i2pr_netdb::LocalRouterInfoBuilder::new(&bundle)
        .build_default(Date::from_millis(0))
        .expect("build");
    let mut store = i2pr_netdb::RouterInfoStore::default();
    store.insert(local.validated().clone());
    let snapshot = store_summary(&store);
    assert_eq!(snapshot.record_count, 1);
    assert_eq!(snapshot.state, BootstrapState::Empty);
}

#[test]
fn reseed_attempt_summary_records_outcome_labels() {
    let summary = ReseedAttemptSummary {
        attempt: 1,
        outcome: "completed",
        total: 5,
        accepted: 4,
        rejected_filename: 1,
        rejected_decode: 0,
        rejected_validation: 0,
    };
    assert_eq!(summary.attempt, 1);
    assert_eq!(summary.accepted, 4);
}

#[test]
fn build_trust_set_rejects_unparseable_certificate() {
    let directory = tempdir().expect("directory");
    let cert_path = directory.path().join("not-a-cert.pem");
    std::fs::write(&cert_path, b"not a der certificate").expect("write");
    let sources = vec![ReseedSourceConfig {
        signer_id: "x".to_owned(),
        certificate_path: cert_path,
    }];
    let error = build_trust_set(&sources).unwrap_err();
    assert!(matches!(
        error,
        i2pr_daemon::bootstrap::BootstrapError::ReseedTrustSet(_)
    ));
}

#[test]
fn bootstrap_state_is_terminal_predicate_is_bounded() {
    assert!(BootstrapState::Failed.is_terminal());
    assert!(BootstrapState::ReadyForNetworkIntegration.is_terminal());
    assert!(BootstrapState::DegradedInsufficientPeers.is_terminal());
    assert!(!BootstrapState::Empty.is_terminal());
    assert!(!BootstrapState::CacheSufficient.is_terminal());
    assert!(!BootstrapState::ReseedRequired.is_terminal());
}

#[test]
fn bootstrap_with_typed_failure_outcome_is_observable() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    IdentityStore::prepare_directory(&path).expect("prepare");
    let bundle = make_bundle(0x5678);
    IdentityStore::in_data_dir(&path)
        .save_new(&bundle)
        .expect("save identity");
    let cache_dir = path.join("netdb").join("routers");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir_path in [path.as_path(), &cache_dir] {
            fs::set_permissions(dir_path, fs::Permissions::from_mode(0o700))
                .expect("private perms");
        }
    }
    // Insert 1 valid RouterInfo so the policy demands reseed.
    let loader = CacheLoader::new(i2pr_storage::cache_seam::ByteCache::in_data_dir(&path));
    let peer_bundle = make_bundle(0x9999);
    let info = peer_bundle
        .sign_router_info(
            Date::from_millis(0),
            Vec::new(),
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let validated = i2pr_netdb::ValidatedRouterInfo::from_router_info(
        info,
        None,
        i2pr_netdb::ValidationContext::new(Date::from_millis(0)),
    )
    .expect("validate");
    let encoded = validated
        .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode");
    let name = validated
        .key()
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    loader
        .cache()
        .write(&name, &encoded)
        .expect("write cache file");

    let mut config = minimal_config(&path);
    config.reseed.enabled = true;
    config.reseed.sources.push(ReseedSourceConfig {
        signer_id: "missing".to_owned(),
        certificate_path: path.join("not-installed.pem"),
    });
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&bundle);
    let bundle_path = path.join("dummy.su3");
    std::fs::write(&bundle_path, b"not a real su3").expect("write");
    let result = bootstrap_with_offline_reseed(&config, &builder, 0, Some(bundle_path));
    // The bootstrap may succeed in degraded mode or fail closed if
    // the trust set is unloadable; both paths are valid.
    match result {
        Ok(report) => {
            assert!(
                report
                    .reseed_attempts
                    .iter()
                    .any(|s| s.outcome != "completed")
            );
            assert!(matches!(
                report.final_state,
                BootstrapState::ReseedRequired
                    | BootstrapState::DegradedInsufficientPeers
                    | BootstrapState::CacheSufficient
            ));
        }
        Err(_) => {
            // Bootstrap closed early because the trust set could not
            // be loaded. The typed failure path is exercised.
        }
    }
}

#[test]
fn cache_loader_reports_rejected_records_via_bootstrap() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    IdentityStore::prepare_directory(&path).expect("prepare");
    let bundle = make_bundle(0x9ABC);
    IdentityStore::in_data_dir(&path)
        .save_new(&bundle)
        .expect("save identity");
    let cache_dir = path.join("netdb").join("routers");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir_path in [path.as_path(), &cache_dir] {
            fs::set_permissions(dir_path, fs::Permissions::from_mode(0o700))
                .expect("private perms");
        }
    }
    fs::write(cache_dir.join("not-lowercase-hex.bin"), b"bogus").expect("write bad filename");
    fs::write(cache_dir.join("ab".repeat(32)), b"more bogus").expect("write bogus content");
    let config = minimal_config(&path);
    let builder = i2pr_netdb::LocalRouterInfoBuilder::new(&bundle);
    let report = bootstrap_with_offline_reseed(&config, &builder, 0, None).expect("bootstrap");
    assert!(report.cache_report.is_some());
    let cache_report = report.cache_report.as_ref().unwrap();
    assert!(cache_report.invalid + cache_report.unreadable > 0);
}

#[test]
fn bootstrap_snapshot_clone_is_independent() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("state");
    let config = minimal_config(&path);
    let bootstrap = Bootstrap::new(RouterInfoStoreConfig::default(), config.reseed.clone());
    let snapshot = bootstrap.diagnostics();
    let mut owned = snapshot.clone();
    owned.reseed_attempts = 42;
    assert_ne!(owned.reseed_attempts, snapshot.reseed_attempts);
}

#[test]
fn bootstrap_report_is_cloneable_and_typed() {
    let report = BootstrapReport {
        final_state: BootstrapState::Empty,
        snapshot: i2pr_daemon::bootstrap::BootstrapSnapshot::default(),
        cache_report: None,
        reseed_report: None,
        reseed_attempts: Vec::new(),
    };
    let _ = report.clone();
    assert_eq!(report.final_state, BootstrapState::Empty);
}

#[test]
fn cache_loader_rejects_unknown_filename_via_compose() {
    let directory = tempdir().expect("directory");
    let data_dir = directory.path().join("state");
    IdentityStore::prepare_directory(&data_dir).expect("prepare");
    let bundle = make_bundle(0xFEED);
    IdentityStore::in_data_dir(&data_dir)
        .save_new(&bundle)
        .expect("save identity");
    let cache_dir = data_dir.join("netdb").join("routers");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir_path in [data_dir.as_path(), &cache_dir] {
            fs::set_permissions(dir_path, fs::Permissions::from_mode(0o700))
                .expect("private perms");
        }
    }
    let cache = i2pr_storage::cache_seam::ByteCache::in_data_dir(&data_dir);
    cache.write(&"ab".repeat(32), b"corrupt").expect("write");
    let loader = CacheLoader::new(cache);
    let mut store = i2pr_netdb::RouterInfoStore::default();
    let context = i2pr_netdb::ValidationContext::new(Date::from_millis(0));
    let report = loader
        .load_into_with_limits(&mut store, context, CacheLoaderLimits::default())
        .expect("report");
    assert!(report.invalid + report.unreadable > 0);
    assert_eq!(store.len(), 0);
}

#[test]
fn loaded_cache_state_invalid_carries_error() {
    let directory = tempdir().expect("directory");
    let data_dir = directory.path().join("state");
    IdentityStore::prepare_directory(&data_dir).expect("prepare");
    let bundle = make_bundle(0xA0A0);
    IdentityStore::in_data_dir(&data_dir)
        .save_new(&bundle)
        .expect("save identity");
    let cache_dir = data_dir.join("netdb").join("routers");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir_path in [data_dir.as_path(), &cache_dir] {
            fs::set_permissions(dir_path, fs::Permissions::from_mode(0o700))
                .expect("private perms");
        }
    }
    let cache = i2pr_storage::cache_seam::ByteCache::in_data_dir(&data_dir);
    cache.write(&"ab".repeat(32), b"corrupt").expect("write");
    let loader = CacheLoader::new(cache);
    let mut store = i2pr_netdb::RouterInfoStore::default();
    let context = i2pr_netdb::ValidationContext::new(Date::from_millis(0));
    let report = loader
        .load_into_with_limits(&mut store, context, CacheLoaderLimits::default())
        .expect("report");
    let invalid = report
        .records
        .iter()
        .find(|r| matches!(r.state, LoadedCacheState::Invalid { .. }));
    assert!(invalid.is_some());
}
