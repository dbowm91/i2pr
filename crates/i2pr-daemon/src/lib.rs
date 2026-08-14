//! Non-networked CLI shell and future daemon composition root.
//!
//! This crate validates configuration and exposes the explicit local identity
//! lifecycle boundary. It does not open listeners, download reseed data, or
//! claim support for any I2P transport or application protocol.

#![forbid(unsafe_code)]

pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod error;
pub mod netdb_seam;

pub use error::DaemonError;

use cli::{CheckConfigArgs, Cli, Command, IdentityCommand, RunArgs};
use config::Config;
use i2pr_crypto::{OsRng, RouterIdentityBundle};
use i2pr_netdb::{LocalRouterInfoBuilder, RouterInfoStoreConfig};
use i2pr_runtime::{ServiceClassification, ServiceName, ServiceSpec};
use i2pr_storage::IdentityStore;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Result of a successful side-effect-free validation command.
#[derive(Debug, Eq, PartialEq)]
pub enum CommandOutcome {
    /// A configuration was validated for the requested command.
    Validated {
        /// Whether the validation came from `run --dry-run`.
        dry_run: bool,
        /// The normalized snapshot used for validation.
        config: Config,
    },
    /// A new private identity was created at the configured path.
    IdentityGenerated {
        /// The private identity file path.
        path: PathBuf,
    },
    /// An existing identity was loaded and structurally summarized.
    IdentityInspected {
        /// The private identity file path.
        path: PathBuf,
        /// Public algorithm identifiers only.
        summary: IdentitySummary,
    },
    /// The daemon is ready to run with the given configuration.
    RunReady {
        /// The normalized snapshot used for execution.
        config: Config,
    },
}

/// Non-secret summary returned by identity inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentitySummary {
    /// I2P signing-key type code.
    pub signing_algorithm: u16,
    /// I2P router encryption-key type code.
    pub encryption_algorithm: u16,
}

/// Executes a parsed CLI command without initializing runtime or network state.
pub fn execute(cli: Cli) -> Result<CommandOutcome, DaemonError> {
    match cli.command {
        Command::CheckConfig(CheckConfigArgs { config }) => Ok(CommandOutcome::Validated {
            dry_run: false,
            config: Config::load(&config)?,
        }),
        Command::Identity {
            command: IdentityCommand::Generate(args),
        } => {
            let config = Config::load(&args.config)?;
            IdentityStore::prepare_directory(&config.router.data_dir)?;
            let store = IdentityStore::in_data_dir(&config.router.data_dir);
            let mut rng = OsRng;
            let bundle = RouterIdentityBundle::generate(&mut rng)?;
            store.save_new(&bundle)?;
            Ok(CommandOutcome::IdentityGenerated {
                path: store.path().to_path_buf(),
            })
        }
        Command::Identity {
            command: IdentityCommand::Inspect(args),
        } => {
            let config = Config::load(&args.config)?;
            let store = IdentityStore::in_data_dir(&config.router.data_dir);
            let bundle = store.load()?;
            Ok(CommandOutcome::IdentityInspected {
                path: store.path().to_path_buf(),
                summary: IdentitySummary {
                    signing_algorithm: bundle.identity().signing_key().key_type().code(),
                    encryption_algorithm: bundle.identity().public_key().key_type().code(),
                },
            })
        }
        Command::Run(RunArgs { config, dry_run }) => {
            let config = Config::load(&config)?;
            if dry_run {
                return Ok(CommandOutcome::Validated { dry_run, config });
            }
            Ok(CommandOutcome::RunReady { config })
        }
    }
}

/// Builds the daemon service graph for the given configuration.
///
/// Returns the graph so callers and tests can inspect the registered services
/// without starting the supervisor loop. The graph is transport-neutral:
/// no `ntcp2-transport` service is registered under the current Plan 101
/// activation guard.
pub fn build_daemon_graph(config: &Config) -> Result<i2pr_runtime::ServiceGraph, DaemonError> {
    if config.transport.ntcp2.enabled {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "NTCP2 activation is not available while support is experimental".to_string(),
        ));
    }

    let mut builder = i2pr_runtime::ServiceGraph::builder(i2pr_runtime::MAX_SERVICE_COUNT)
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to create service graph: {e}"))
        })?;

    let lifecycle_name = ServiceName::new("lifecycle").expect("valid service name");
    builder
        .register(ServiceSpec::new(
            lifecycle_name,
            ServiceClassification::Essential,
            |_ctx| {
                Box::pin(async {
                    tokio::signal::ctrl_c().await.ok();
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register service: {e}"))
        })?;

    let netdb_name = ServiceName::new("netdb-bootstrap").expect("valid service name");
    builder
        .register(ServiceSpec::new(
            netdb_name,
            ServiceClassification::Essential,
            |ctx| {
                Box::pin(async move {
                    // The Plan 106 netdb-bootstrap service is a
                    // long-lived observability owner that keeps the
                    // supervisor alive while the daemon is healthy.
                    // The actual bootstrap pipeline runs synchronously
                    // in `run_daemon` before the supervisor starts so
                    // the bounded startup pipeline is observable in
                    // the CLI exit path. The service here is the
                    // cancellation-aware wait that ties the supervisor
                    // lifetime to the lifecycle signal.
                    let cancellation = ctx.cancellation();
                    cancellation.cancelled().await;
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register service: {e}"))
        })?;

    builder
        .build()
        .map_err(|e| DaemonError::RuntimeSupervisorFailed(format!("invalid service graph: {e}")))
}

/// Runs the Plan 106 bounded bootstrap pipeline synchronously and
/// returns the sanitized report.
///
/// The pipeline:
/// 1. loads the persistent router identity;
/// 2. revalidates the persistent RouterInfo cache;
/// 3. constructs and self-validates the local RouterInfo;
/// 4. recomputes bootstrap readiness;
/// 5. performs at most one bounded reseed attempt if enabled.
///
/// The function never opens sockets, never performs DNS, and never
/// contacts I2P peers.
pub fn bootstrap_daemon(
    config: &Config,
    now_seconds: u64,
    offline_reseed_path: Option<std::path::PathBuf>,
) -> Result<(bootstrap::BootstrapReport, Arc<Mutex<bootstrap::Bootstrap>>), DaemonError> {
    let store = IdentityStore::in_data_dir(&config.router.data_dir);
    let bundle = store.load().map_err(|e| {
        DaemonError::RuntimeIdentity(format!("failed to load router identity: {e}"))
    })?;
    let builder = LocalRouterInfoBuilder::new(&bundle);
    let store_config =
        RouterInfoStoreConfig::new(config.netdb.max_records, config.netdb.max_encoded_bytes);
    let mut bootstrap = bootstrap::Bootstrap::new(store_config, config.reseed.clone());
    if let Some(path) = offline_reseed_path {
        bootstrap = bootstrap.with_offline_reseed_path(path);
    }
    let policy = bootstrap::BootstrapPolicy::from_config(config);
    let report = bootstrap
        .run(&config.router.data_dir, &builder, policy, now_seconds)
        .map_err(|e| DaemonError::RuntimeBootstrap(e.to_string()))?;
    let shared = Arc::new(Mutex::new(bootstrap));
    Ok((report, shared))
}

/// Executes the live daemon run with Tokio runtime and supervisor.
///
/// The function runs the Plan 106 bounded bootstrap pipeline
/// synchronously before starting the supervisor, then drives the
/// supervisor loop until shutdown or failure.
pub async fn run_daemon(config: Config) -> Result<(), DaemonError> {
    let now_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let (report, _bootstrap_handle) = bootstrap_daemon(&config, now_seconds, None)?;
    tracing::info!(
        state = %report.final_state,
        record_count = report.snapshot.record_count,
        floodfill_advertisers = report.snapshot.floodfill_advertisers,
        reseed_attempts = report.snapshot.reseed_attempts,
        "bootstrap pipeline completed"
    );

    let graph = build_daemon_graph(&config)?;

    let supervisor =
        i2pr_runtime::Supervisor::new(graph, Duration::from_secs(30)).map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to create supervisor: {e}"))
        })?;

    let handle = supervisor.handle();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        handle.shutdown(i2pr_runtime::ShutdownReason::Requested);
    });

    let report = supervisor
        .run()
        .await
        .map_err(|e| DaemonError::RuntimeSupervisorFailed(format!("supervisor failed: {e}")))?;

    if !report.was_graceful() {
        return Err(DaemonError::RuntimeShutdownTimeout);
    }

    Ok(())
}

/// Initializes the future daemon logging subscriber using validated settings.
///
/// Repeated initialization is intentionally harmless for embedding tests.  A
/// later composition plan will own subscriber layering and redaction policy.
pub fn initialize_logging(config: &config::LoggingConfig) {
    let filter = tracing_subscriber::EnvFilter::new(config.filter.clone());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::*;
    use crate::cli::{CheckConfigArgs, Command, IdentityArgs, IdentityCommand, RunArgs};
    use crate::error::ExitCode;

    #[test]
    fn missing_file_has_unavailable_exit_code() {
        let cli = Cli {
            command: Command::CheckConfig(CheckConfigArgs {
                config: PathBuf::from("missing-config.toml"),
            }),
        };
        let error = execute(cli).expect_err("missing config must fail");
        assert_eq!(error.exit_code(), ExitCode::ConfigUnavailable);
    }

    #[test]
    fn live_run_returns_run_ready() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "schema_version = 1\n[router]\ndata_dir = \"state\"\n",
        )
        .expect("write config");
        let cli = Cli {
            command: Command::Run(RunArgs {
                config: path,
                dry_run: false,
            }),
        };
        let outcome = execute(cli).expect("live run should return RunReady");
        assert!(matches!(outcome, CommandOutcome::RunReady { .. }));
    }

    #[test]
    fn parser_exposes_required_commands_and_flags() {
        let cli = Cli::try_parse_from(["i2pr", "run", "--config", "config.toml", "--dry-run"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Run(RunArgs { dry_run: true, .. })
        ));
    }

    #[test]
    fn explicit_identity_lifecycle_generates_and_inspects_without_secret_output() {
        let directory = tempfile::tempdir().expect("temp directory");
        let data_dir = directory.path().join("state");
        let config_path = directory.path().join("config.toml");
        std::fs::write(
            &config_path,
            format!(
                "schema_version = 1\n[router]\ndata_dir = {:?}\n",
                data_dir.to_string_lossy()
            ),
        )
        .expect("write config");

        let generated = execute(Cli {
            command: Command::Identity {
                command: IdentityCommand::Generate(IdentityArgs {
                    config: config_path.clone(),
                }),
            },
        })
        .expect("generate identity");
        assert!(matches!(
            generated,
            CommandOutcome::IdentityGenerated { .. }
        ));

        let inspected = execute(Cli {
            command: Command::Identity {
                command: IdentityCommand::Inspect(IdentityArgs {
                    config: config_path,
                }),
            },
        })
        .expect("inspect identity");
        assert_eq!(
            inspected,
            CommandOutcome::IdentityInspected {
                path: data_dir.join("router.identity"),
                summary: IdentitySummary {
                    signing_algorithm: 7,
                    encryption_algorithm: 4,
                },
            }
        );
    }

    #[test]
    fn daemon_graph_contains_no_ntcp2_transport_service() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("valid config");
        let graph = build_daemon_graph(&config).expect("graph builds");
        let names: Vec<_> = graph
            .startup_order()
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        assert!(
            !names.iter().any(|n| n == "ntcp2-transport"),
            "service graph must not contain ntcp2-transport, got: {names:?}"
        );
        assert!(
            names.iter().any(|n| n == "lifecycle"),
            "service graph must contain lifecycle, got: {names:?}"
        );
    }

    #[test]
    fn daemon_graph_rejects_ntcp2_enabled_config() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[transport.ntcp2]\nenabled = true\n",
            path.to_string_lossy()
        );
        let err = Config::parse(&text).expect_err("config should reject enabled = true");
        assert!(matches!(
            err,
            crate::config::ConfigError::Semantic {
                field: "transport.ntcp2.enabled",
                ..
            }
        ));
    }
}
