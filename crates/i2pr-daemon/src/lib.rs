//! Non-networked CLI shell and future daemon composition root.
//!
//! This crate validates configuration and exposes the explicit local identity
//! lifecycle boundary. It does not open listeners, download reseed data, or
//! claim support for any I2P transport or application protocol.

#![forbid(unsafe_code)]

pub mod cli;
pub mod config;
pub mod error;

use cli::{CheckConfigArgs, Cli, Command, IdentityCommand, RunArgs};
use config::Config;
use error::DaemonError;
use i2pr_crypto::{OsRng, RouterIdentityBundle};
use i2pr_runtime::{
    CancellationToken, Ntcp2RuntimeConfig, Ntcp2RuntimeService, ServiceClassification, ServiceName,
    ServiceSpec,
};
use i2pr_storage::IdentityStore;
use std::path::PathBuf;
use std::sync::Arc;
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

/// Executes the live daemon run with Tokio runtime and supervisor.
pub async fn run_daemon(config: Config) -> Result<(), DaemonError> {
    let store = IdentityStore::in_data_dir(&config.router.data_dir);
    let _bundle = store.load().map_err(|e| {
        DaemonError::RuntimeIdentity(format!("failed to load router identity: {e}"))
    })?;

    let ntcp2_config = Ntcp2RuntimeConfig {
        deadlines: i2pr_runtime::Ntcp2RuntimeDeadlines {
            connect: config.transport.ntcp2.connect_timeout,
            handshake: config.transport.ntcp2.handshake_timeout,
            read_idle: config.transport.ntcp2.read_idle_timeout,
            write: config.transport.ntcp2.write_timeout,
            queue_wait: config.transport.ntcp2.queue_wait_timeout,
            drain: config.transport.ntcp2.drain_timeout,
        },
        limits: i2pr_runtime::Ntcp2RuntimeLimits {
            max_active_links: config.transport.ntcp2.max_active_links,
            max_replay_entries: config.transport.ntcp2.max_replay_entries,
            ..i2pr_runtime::Ntcp2RuntimeLimits::default()
        },
        prefixes: i2pr_runtime::IpPrefixPolicy {
            ipv4_prefix: config.transport.ntcp2.ipv4_prefix,
            ipv6_prefix: config.transport.ntcp2.ipv6_prefix,
        },
    };

    let service = Arc::new(Ntcp2RuntimeService::new(ntcp2_config).map_err(|e| {
        DaemonError::RuntimeSupervisorFailed(format!("failed to create NTCP2 runtime: {e}"))
    })?);

    let listen_address = config.network.listen_socket();

    let mut builder = i2pr_runtime::ServiceGraph::builder(i2pr_runtime::MAX_SERVICE_COUNT)
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to create service graph: {e}"))
        })?;

    let ntcp2_service_name = ServiceName::new("ntcp2-transport").expect("valid service name");
    builder
        .register(ServiceSpec::new(
            ntcp2_service_name.clone(),
            ServiceClassification::Essential,
            move |_ctx| {
                let service = Arc::clone(&service);
                let address = listen_address;
                Box::pin(async move {
                    let root = CancellationToken::new();
                    let scope = service.child_scope(&root);

                    match service.listen(address, &scope).await {
                        Ok(_listener) => {
                            tokio::signal::ctrl_c().await.ok();
                            let cleanup = scope.shutdown().await;
                            if cleanup.failed() {
                                let detail =
                                    i2pr_core::HealthDetail::new("listener cleanup failed").ok();
                                i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::Internal,
                                    detail,
                                ))
                            } else {
                                i2pr_runtime::ServiceResult::RequestedShutdown
                            }
                        }
                        Err(e) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "failed to bind listener: {e:?}"
                            ))
                            .ok();
                            i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::ResourceExhausted,
                                detail,
                            ))
                        }
                    }
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register service: {e}"))
        })?;

    let graph = builder
        .build()
        .map_err(|e| DaemonError::RuntimeSupervisorFailed(format!("invalid service graph: {e}")))?;

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
}
