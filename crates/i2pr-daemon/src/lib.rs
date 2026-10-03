//! Non-networked CLI shell and future daemon composition root.
//!
//! This crate validates configuration and exposes the explicit local identity
//! lifecycle boundary. It does not open listeners, download reseed data, or
//! claim support for any I2P transport or application protocol.

#![forbid(unsafe_code)]

pub mod addressbook;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod control_sources;
pub mod destination_peers;
pub mod destination_streaming;
pub mod destination_tunnels;
pub mod error;
pub mod exploratory_build;
pub mod floodfill;
pub mod i2cp;
pub mod i2pcontrol;
pub mod i2pcontrol_inspection;
pub mod i2pcontrol_tunnels;
pub mod inbound_dispatch;
pub mod netdb_seam;
pub mod netdb_tunnels;
pub mod outbound_lookup;
pub mod peer_test;
pub mod router_i2np;
pub mod sam;
pub mod service_delivery;
pub mod service_generation;
pub mod service_product;
pub mod service_tunnels;
pub mod service_tunnels_http;
pub mod service_tunnels_http_bidir;
pub mod service_tunnels_http_server;
pub mod service_tunnels_irc_client;
pub mod service_tunnels_irc_server;
pub mod service_tunnels_socks5;
pub mod service_tunnels_socks_irc;
pub mod service_tunnels_streamr;
pub mod service_tunnels_tls;
pub mod transit_compose;
pub mod transit_owner;
pub mod tunnel_liveness;

pub use error::DaemonError;
pub use i2cp::{I2cpServiceError, I2cpServiceSnapshot, I2cpServiceState};
pub use i2pcontrol::{I2pControlServiceError, I2pControlServiceSnapshot, I2pControlServiceState};
pub use i2pcontrol_inspection::InspectionHandles;
pub use netdb_seam::{
    CompositionOutcome, ExploratoryPathStatus, LeaseSet2ResponseOutcome, NetDbSeam, NetDbSeamError,
};
pub use sam::{SamServiceError, SamServiceState, StreamingPools};

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
    build_daemon_graph_with_inspection(config).map(|(graph, _)| graph)
}

/// Builds the daemon service graph plus the shared Plan 288 inspection
/// handles.
///
/// The handles carry static configuration truth (network id, service
/// enablement/binds, startup service inventory). Each optional service
/// factory publishes its live state into the shared handles after
/// construction, so the I2PControl inspection plane reads from the real
/// owners without a global router context. Callers that only need the
/// graph use [`build_daemon_graph`].
pub fn build_daemon_graph_with_inspection(
    config: &Config,
) -> Result<(i2pr_runtime::ServiceGraph, Arc<InspectionHandles>), DaemonError> {
    let inspection = Arc::new(InspectionHandles::from_config(config));
    let graph = build_daemon_graph_inner(config, &inspection, None)?;
    Ok((graph, inspection))
}

/// Shared graph construction over explicit inspection handles.
fn build_daemon_graph_inner(
    config: &Config,
    inspection: &Arc<InspectionHandles>,
    bootstrap: Option<Arc<Mutex<bootstrap::Bootstrap>>>,
) -> Result<i2pr_runtime::ServiceGraph, DaemonError> {
    let has_enabled_service_tunnels = config
        .service_tunnels
        .tunnels
        .tunnels
        .iter()
        .any(|tunnel| tunnel.enabled);
    if has_enabled_service_tunnels && bootstrap.is_none() {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "enabled service tunnels require the normal-daemon Destination-group provider, which is not active"
                .to_owned(),
        ));
    }
    if has_enabled_service_tunnels && !config.ssu2.enabled {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "enabled service tunnels require the normal SSU2 owner".to_owned(),
        ));
    }
    if has_enabled_service_tunnels && !config.service_tunnels.enabled {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "enabled service tunnels require service_tunnels.enabled = true".to_owned(),
        ));
    }
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

    // Plan 294: activate the canonical address-book subsystem before
    // any service factory runs so every consumer installs the same
    // shared resolver cell. Disabled (default) activation never
    // touches the filesystem and publishes nothing; consumers keep
    // their legacy behavior.
    let addressbook = Arc::new(crate::addressbook::AddressBookManager::activate(
        config.addressbook.clone(),
    ));
    inspection.publish_addressbook(addressbook.shared());

    // Plan 295: install the control-plane source owners and attest the
    // static rows. Live rows (log ring, metrics, SSU2) are read per
    // request; rows with no owner in this graph attest empty at
    // composition (the owners were consulted: none exist in the
    // default graph), never fabricated.
    inspection.publish_log_ring(crate::control_sources::LogRing::global());
    inspection.publish_metrics(Arc::new(crate::control_sources::ControlMetrics::new()));
    let bans = crate::control_sources::BanLedger::new();
    if inspection.publish_bans(bans.attested()).is_err() {
        tracing::warn!("ban attestation rejected");
    }
    if inspection
        .publish_netdb(
            Vec::new(),
            Vec::new(),
            crate::i2pcontrol_inspection::FloodfillMode::Disabled,
        )
        .is_err()
    {
        tracing::warn!("netdb attestation rejected");
    }
    if inspection
        .publish_transport(Vec::new(), Vec::new(), "loopback-only", Vec::new())
        .is_err()
    {
        tracing::warn!("transport attestation rejected");
    }
    if inspection.publish_tunnels(0, 0, 0, 0).is_err() {
        tracing::warn!("tunnel attestation rejected");
    }

    if config.sam.enabled {
        register_sam_service(&mut builder, config, inspection, &addressbook)?;
    }

    if config.i2cp.enabled {
        register_i2cp_service(&mut builder, config, inspection)?;
    }

    if config.i2pcontrol.enabled {
        register_i2pcontrol_service(&mut builder, config, inspection, &addressbook)?;
    }

    if addressbook.is_active() {
        register_addressbook_refresh_service(&mut builder, &addressbook)?;
    }

    if config.ssu2.enabled {
        register_ssu2_service(&mut builder, config, inspection, &addressbook, bootstrap)?;
    }

    builder
        .build()
        .map_err(|e| DaemonError::RuntimeSupervisorFailed(format!("invalid service graph: {e}")))
}

/// Registers the supervised loopback SAM service in the supplied
/// builder. The factory captures the [`SamServiceState`] so the
/// per-connection tokio tasks own Arc clones that share the same
/// session/destination/streaming registries.
fn register_sam_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
) -> Result<(), DaemonError> {
    let sam_config = config.sam.clone();
    let address = sam_config.bind_socket();
    let sam_name = ServiceName::new("sam-bridge").expect("valid service name");
    let inspection = Arc::clone(inspection);
    let addressbook = Arc::clone(addressbook);
    builder
        .register(ServiceSpec::new(
            sam_name,
            ServiceClassification::Optional,
            move |ctx| {
                let sam_config = sam_config.clone();
                let inspection = Arc::clone(&inspection);
                let addressbook = Arc::clone(&addressbook);
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                Box::pin(async move {
                    let state = match SamServiceState::new(sam_config) {
                        Ok(state) => Arc::new(state),
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "SAM service construction failed: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    // Publish the live owner for the Plan 288
                    // inspection plane (read-only snapshot access).
                    inspection.publish_sam(Arc::clone(&state));
                    // Install the Plan 294 canonical resolver cell (a
                    // no-op empty cell unless the subsystem is active).
                    state.set_addressbook_handle(addressbook.shared());
                    let token = cancellation.clone();
                    let join_result =
                        i2pr_runtime::bounded_timeout(Duration::from_secs(1), async {
                            state.run(address, children, token).await
                        })
                        .await;
                    if join_result.is_err() {
                        let detail = i2pr_core::HealthDetail::new(
                            "SAM listener failed to start within the bounded timeout",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::Internal,
                                detail,
                            ),
                        );
                    }
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register SAM service: {e}"))
        })?;
    Ok(())
}

/// Registers the supervised loopback I2CP service in the supplied
/// builder. The factory captures the [`I2cpServiceState`] so the
/// per-connection tokio tasks own Arc clones that share the same
/// session/destination registries. Plan 167 keeps I2CP experimental,
/// loopback-only, and disabled by default.
fn register_i2cp_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
) -> Result<(), DaemonError> {
    let i2cp_config = config.i2cp.clone();
    let address = i2cp_config.bind_socket();
    let i2cp_name = ServiceName::new("i2cp-bridge").expect("valid service name");
    let inspection = Arc::clone(inspection);
    builder
        .register(ServiceSpec::new(
            i2cp_name,
            ServiceClassification::Optional,
            move |ctx| {
                let i2cp_config = i2cp_config.clone();
                let inspection = Arc::clone(&inspection);
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                Box::pin(async move {
                    let state = match I2cpServiceState::new(i2cp_config) {
                        Ok(state) => Arc::new(state),
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "I2CP service construction failed: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    // Publish the live owner for the Plan 288
                    // inspection plane (read-only snapshot access).
                    inspection.publish_i2cp(Arc::clone(&state));
                    let token = cancellation.clone();
                    let join_result =
                        i2pr_runtime::bounded_timeout(Duration::from_secs(1), async {
                            state.run(address, children, token).await
                        })
                        .await;
                    if join_result.is_err() {
                        let detail = i2pr_core::HealthDetail::new(
                            "I2CP listener failed to start within the bounded timeout",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::Internal,
                                detail,
                            ),
                        );
                    }
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register I2CP service: {e}"))
        })?;
    Ok(())
}

/// Registers the supervised I2PControl HTTPS service in the supplied
/// builder. The factory captures the [`I2pControlServiceState`] so the
/// per-connection tokio tasks own Arc clones that share the same token
/// and throttle tables. Plan 287 keeps I2PControl experimental,
/// loopback-by-default, and disabled by default with no plaintext
/// fallback.
fn register_i2pcontrol_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
) -> Result<(), DaemonError> {
    let i2pcontrol_config = config.i2pcontrol.clone();
    let address = i2pcontrol_config.bind_socket();
    let i2pcontrol_name = ServiceName::new("i2pcontrol").expect("valid service name");
    // Plan 289: the control-owned service manager is built here (store
    // beneath the router data dir, fresh control-only manager) and
    // installed on the service state before serving. Construction
    // touches only the filesystem; definitions load and reconcile at
    // service startup. A construction failure fails the service
    // fail-closed without touching the network.
    let control = match i2pcontrol_tunnels::TunnelControlState::for_config(config) {
        Ok(control) => Arc::new(control),
        Err(error) => {
            return Err(DaemonError::RuntimeSupervisorFailed(format!(
                "failed to register I2PControl service: {error}"
            )));
        }
    };
    let inspection = Arc::clone(inspection);
    let addressbook = Arc::clone(addressbook);
    builder
        .register(ServiceSpec::new(
            i2pcontrol_name,
            ServiceClassification::Optional,
            move |ctx| {
                let i2pcontrol_config = i2pcontrol_config.clone();
                let inspection = Arc::clone(&inspection);
                let addressbook = Arc::clone(&addressbook);
                let control = Arc::clone(&control);
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                Box::pin(async move {
                    let state = match I2pControlServiceState::new_with_inspection(
                        i2pcontrol_config,
                        inspection,
                    ) {
                        Ok(state) => Arc::new(state),
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "I2PControl service construction failed: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    state.set_control_manager(Arc::clone(&control));
                    state.set_addressbook_manager(Arc::clone(&addressbook));
                    let token = cancellation.clone();
                    let join_result =
                        i2pr_runtime::bounded_timeout(Duration::from_secs(1), async {
                            state.run(address, children, token).await
                        })
                        .await;
                    if join_result.is_err() {
                        let detail = i2pr_core::HealthDetail::new(
                            "I2PControl listener failed to start within the bounded timeout",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::Internal,
                                detail,
                            ),
                        );
                    }
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!(
                "failed to register I2PControl service: {e}"
            ))
        })?;
    Ok(())
}
/// Registers the Plan 294 subscription-refresh worker. The worker
/// wakes once per committed refresh interval and drains the bounded
/// queue through the manager (attempt, diagnostic artifact, promote).
/// With no downloader owner composed, attempts report unavailable;
/// the cadence, queue discipline, and artifact remain live and
/// tested. Cancellation stops the worker between wakes.
fn register_addressbook_refresh_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
) -> Result<(), DaemonError> {
    let worker_name = ServiceName::new("addressbook-refresh").expect("valid service name");
    let manager = Arc::clone(addressbook);
    builder
        .register(ServiceSpec::new(
            worker_name,
            ServiceClassification::Optional,
            move |ctx| {
                let manager = Arc::clone(&manager);
                let cancellation = ctx.cancellation().clone();
                Box::pin(async move {
                    if !manager.is_active() {
                        return i2pr_runtime::ServiceResult::RequestedShutdown;
                    }
                    loop {
                        let hours = manager.refresh_interval_hours().max(1);
                        tokio::select! {
                            _ = cancellation.cancelled() => break,
                            _ = tokio::time::sleep(Duration::from_secs(hours * 3600)) => {
                                manager.run_queued_refreshes(
                                    i2pr_addressbook::RefreshReason::IntervalElapsed,
                                );
                            }
                        }
                    }
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!(
                "failed to register address-book refresh service: {e}"
            ))
        })?;
    Ok(())
}
/// Bounded Plan 279 normal-path evaluation period.
const NORMAL_FLOODFILL_EVALUATION_PERIOD: Duration = Duration::from_secs(30);
/// Serving-NetDB record age ceiling for the normal path (mirrors the
/// controlled coordinator default).
const NORMAL_FLOODFILL_MAX_RECORD_AGE_MS: u64 = 60 * 60 * 1000;
/// Bounded drain for one normal withdrawal.
const NORMAL_FLOODFILL_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// One inbound queue owner for the normal daemon. The group product wraps
/// the same SSU2 handle; it never starts another transport or receiver.
enum NormalSsu2Owner {
    Router(crate::router_i2np::Ssu2DaemonHandle),
    Groups(Box<crate::service_product::ServiceProduct>),
}

impl NormalSsu2Owner {
    fn handle(&self) -> &crate::router_i2np::Ssu2DaemonHandle {
        match self {
            Self::Router(handle) => handle,
            Self::Groups(product) => product.ssu2_handle(),
        }
    }

    async fn next_inbound(&mut self) -> Option<i2pr_runtime::Ssu2InboundI2np> {
        match self {
            Self::Router(handle) => handle.next_inbound().await,
            Self::Groups(product) => product.next_inbound().await,
        }
    }

    async fn process_group_inbound(
        &mut self,
        inbound: &i2pr_runtime::Ssu2InboundI2np,
    ) -> Result<(), crate::service_product::ServiceProductError> {
        if let Self::Groups(product) = self {
            product.process_inbound(inbound).await;
            product.advance_destination_pools().await?;
        }
        Ok(())
    }

    async fn advance_groups(&mut self) -> Result<(), crate::service_product::ServiceProductError> {
        if let Self::Groups(product) = self {
            product.advance_destination_pools().await?;
        }
        Ok(())
    }

    async fn shutdown(self) {
        match self {
            Self::Router(handle) => handle.shutdown(),
            Self::Groups(product) => {
                let _ = (*product).shutdown().await;
            }
        }
    }
}

/// Runs one bounded Plan 279 evaluation tick for the router-owned
/// floodfill state: re-reads operator intent, gathers live signals,
/// evaluates, and applies the resulting step. Status output is
/// categorical (requested intent, effective role, reason labels); no
/// peer identifiers, addresses, or key material are logged.
async fn run_floodfill_evaluation(
    state: &mut crate::floodfill::FloodfillServiceState,
    handle: &crate::router_i2np::Ssu2DaemonHandle,
    bundle: &RouterIdentityBundle,
    data_dir: &std::path::Path,
    config_path: Option<&std::path::Path>,
    cancellation: &i2pr_runtime::CancellationToken,
) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    if let Some(path) = config_path {
        // The file is a few kilobytes and this runs every 30 s; a
        // blocking read keeps the daemon's Tokio feature surface
        // unchanged.
        match std::fs::read_to_string(path) {
            Ok(text) => match state.refresh_intent(&text) {
                Ok(true) => tracing::info!(
                    requested = state.intent(),
                    "floodfill operator intent changed; next tick applies it"
                ),
                Ok(false) => {}
                Err(_) => tracing::warn!(
                    "floodfill config re-read failed to parse; keeping previous intent"
                ),
            },
            Err(_) => {
                tracing::warn!("floodfill config re-read failed to read; keeping previous intent")
            }
        }
    }
    let material = handle.service().publication_material(now_ms).ok();
    let _ = state.maintenance_tick(
        now_ms,
        NORMAL_FLOODFILL_MAX_RECORD_AGE_MS,
        crate::floodfill::MAX_FLOODFILL_MAINTENANCE_BATCH,
    );
    let readiness = crate::floodfill::NormalReadiness {
        material: material.as_ref(),
        wall_now_ms: now_ms,
        // The serving NetDB is constructed with the state and the
        // persistent cache passed revalidation in the bootstrap
        // pipeline before the supervisor started.
        netdb_ready: true,
        storage_ready: std::fs::metadata(data_dir).is_ok(),
        // This tick just ran the maintenance batch above.
        maintenance_ready: true,
        resource_headroom: state.check_headroom(),
        clock_sane: crate::floodfill::is_sane_wall_ms(now_ms),
        supervision_healthy: !cancellation.is_cancelled(),
    };
    match state.evaluate(&readiness) {
        crate::floodfill::FloodfillServiceStep::Idle => {}
        crate::floodfill::FloodfillServiceStep::Activate => {
            match state
                .apply_activation(handle, bundle, &readiness, cancellation)
                .await
            {
                Ok(_) => tracing::info!(
                    requested = true,
                    role = "Active",
                    "normal floodfill activated"
                ),
                Err(error) => tracing::warn!(
                    requested = true,
                    role = "Failed",
                    reason = %error,
                    "normal floodfill activation failed closed"
                ),
            }
        }
        crate::floodfill::FloodfillServiceStep::Withdraw => {
            match state
                .apply_withdrawal(
                    handle,
                    bundle,
                    now_ms,
                    NORMAL_FLOODFILL_DRAIN_TIMEOUT,
                    NORMAL_FLOODFILL_MAX_RECORD_AGE_MS,
                    cancellation,
                )
                .await
            {
                Ok(_) => tracing::info!(
                    requested = state.intent(),
                    role = "Disabled",
                    "normal floodfill withdrew"
                ),
                Err(error) => tracing::warn!(
                    requested = state.intent(),
                    reason = %error,
                    "normal floodfill withdrawal failed; role stays Draining for retry"
                ),
            }
        }
    }
}

/// Registers the supervised loopback SSU2 router service in the
/// supplied builder. Plan 184 owns the first daemon activation of
/// the existing SSU2 runtime under the strict controlled profile
/// (loopback bind, `advertise = false`, no introducer service). The
/// factory loads the persistent router bundle, generates ephemeral
/// controlled SSU2 identity material with the OS CSPRNG, starts the
/// daemon-owned [`crate::router_i2np::Ssu2DaemonService`] under the
/// service child scope, and pumps the bounded central dispatcher
/// until cancellation. No hidden standalone runtime coexists with
/// this instance in counted tests.
fn register_ssu2_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
    bootstrap: Option<Arc<Mutex<bootstrap::Bootstrap>>>,
) -> Result<(), DaemonError> {
    use crate::router_i2np::{
        Ssu2DaemonService, dispatch_router_i2np, generate_controlled_identity,
    };
    use crate::transit_owner::controlled_transit_disabled_probe;
    let ssu2_config = config.ssu2.clone();
    let data_dir = config.router.data_dir.clone();
    let router_info_store_config =
        RouterInfoStoreConfig::new(config.netdb.max_records, config.netdb.max_encoded_bytes);
    let service_tunnels = config.service_tunnels.clone();
    let addressbook = Arc::clone(addressbook);
    let floodfill_enabled = config.floodfill.enabled;
    let floodfill_config_path = config.source_path.clone();
    let ssu2_name = ServiceName::new("ssu2-router").expect("valid service name");
    let inspection = Arc::clone(inspection);
    builder
        .register(ServiceSpec::new(
            ssu2_name,
            ServiceClassification::Optional,
            move |ctx| {
                let ssu2_config = ssu2_config.clone();
                let data_dir = data_dir.clone();
                let router_info_store_config = router_info_store_config;
                let inspection = Arc::clone(&inspection);
                let floodfill_config_path = floodfill_config_path.clone();
                let service_tunnels = service_tunnels.clone();
                let bootstrap = bootstrap.clone();
                let addressbook = Arc::clone(&addressbook);
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                let readiness = ctx.readiness();
                Box::pin(async move {
                    // Strict profile is enforced twice: once at config
                    // parse time and again here so a future bypass
                    // cannot reach socket ownership.
                    if !ssu2_config.enabled
                        || ssu2_config.advertise
                        || ssu2_config.introducer_service
                    {
                        let detail = i2pr_core::HealthDetail::new(
                            "SSU2 service requires the strict controlled profile",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
                    let store = IdentityStore::in_data_dir(&data_dir);
                    let bundle = match store.load() {
                        Ok(bundle) => bundle,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "SSU2 router identity unavailable: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    let bundle = Arc::new(bundle);
                    // Controlled RouterInfo host/port: prefer IPv4
                    // loopback when bound, otherwise IPv6 loopback.
                    // An ephemeral `port = 0` uses a loopback
                    // placeholder for the in-band RouterInfo because
                    // `advertise = false` means the record is never
                    // published; authentication binds through the
                    // static key with out-of-band dial addresses.
                    let (host, ri_port) = if ssu2_config.bind_ipv4.is_some() {
                        (
                            "127.0.0.1",
                            if ssu2_config.port == 0 {
                                44001
                            } else {
                                ssu2_config.port
                            },
                        )
                    } else if ssu2_config.bind_ipv6.is_some() {
                        (
                            "::1",
                            if ssu2_config.port == 0 {
                                44001
                            } else {
                                ssu2_config.port
                            },
                        )
                    } else {
                        let detail =
                            i2pr_core::HealthDetail::new("SSU2 service has no loopback bind").ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    };
                    let identity = match generate_controlled_identity(&bundle, host, ri_port) {
                        Ok(identity) => identity,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "SSU2 identity failed: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    let daemon_service = match Ssu2DaemonService::new(&ssu2_config, identity) {
                        Ok(service) => service,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "SSU2 service failed: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    let handle = match daemon_service.start(&children, &ssu2_config).await {
                        Ok(handle) => handle,
                        Err(error) => {
                            let detail =
                                i2pr_core::HealthDetail::new(format!("SSU2 bind failed: {error}"))
                                    .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::Internal,
                                    detail,
                                ),
                            );
                        }
                    };
                    // Plan 295: publish the cloned runtime service so
                    // the inspection plane reads session/error
                    // snapshots and primes the metrics windows without
                    // touching the pump path.
                    inspection.publish_ssu2(handle.service().clone());
                    // Central dispatcher pump: no task per message, no
                    // unbounded retention. Outcomes are classified and
                    // dropped; Plan 185/186 own the live tunnel/NetDB
                    // hooks. Cancellation drains orderly.
                    //
                    // Plan 279: when the operator opts into normal
                    // floodfill, the router service additionally owns a
                    // FloodfillServiceState next to the socket. The
                    // role starts Disabled with an empty serving NetDB
                    // and evaluates upward from there on a bounded
                    // tick; inbound control traffic routes through the
                    // coordinator only while the role is Active,
                    // otherwise the dispatcher below owns the message
                    // exactly as before. On a loopback-only tree the
                    // address gate keeps the role Disabled by
                    // construction, so the branch is inert there.
                    let local_hash = match bundle.identity().hash() {
                        Ok(hash) => hash,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "floodfill local hash unavailable: {error}"
                            ))
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        }
                    };
                    let mut floodfill = if floodfill_enabled {
                        match crate::floodfill::FloodfillServiceState::new(local_hash, true) {
                            Ok(state) => {
                                tracing::info!(
                                    requested = true,
                                    role = "Disabled",
                                    "normal floodfill opted in; evaluating eligibility"
                                );
                                Some(state)
                            }
                            Err(error) => {
                                let detail = i2pr_core::HealthDetail::new(format!(
                                    "floodfill service state failed: {error:?}"
                                ))
                                .ok();
                                return i2pr_runtime::ServiceResult::Failed(
                                    i2pr_core::ServiceFailure::new(
                                        i2pr_core::ServiceFailureCategory::InvalidState,
                                        detail,
                                    ),
                                );
                            }
                        }
                    } else {
                        None
                    };
                    let mut evaluation = tokio::time::interval(
                        NORMAL_FLOODFILL_EVALUATION_PERIOD,
                    );
                    evaluation
                        .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    let mut group_tick = tokio::time::interval(Duration::from_millis(250));
                    group_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    let pump_started = tokio::time::Instant::now();
                    let token = cancellation.clone();
                    let has_enabled_groups = service_tunnels
                        .tunnels
                        .tunnels
                        .iter()
                        .any(|tunnel| tunnel.enabled);
                    let mut owner = if has_enabled_groups {
                        let Some(bootstrap) = bootstrap.as_ref() else {
                            handle.shutdown();
                            let detail = i2pr_core::HealthDetail::new(
                                "validated bootstrap store unavailable for Destination groups",
                            )
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        };
                        let router_infos = match bootstrap.lock() {
                            Ok(bootstrap) => bootstrap.validated_router_info_snapshot(),
                            Err(_) => {
                                handle.shutdown();
                                let detail = i2pr_core::HealthDetail::new(
                                    "validated bootstrap store unavailable for Destination groups",
                                )
                                .ok();
                                return i2pr_runtime::ServiceResult::Failed(
                                    i2pr_core::ServiceFailure::new(
                                        i2pr_core::ServiceFailureCategory::InvalidState,
                                        detail,
                                    ),
                                );
                            }
                        };
                        let Some(ssu2_bind) = handle.local_v4().or_else(|| handle.local_v6()) else {
                            handle.shutdown();
                            let detail = i2pr_core::HealthDetail::new(
                                "Destination groups require a bound loopback SSU2 endpoint",
                            )
                            .ok();
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    detail,
                                ),
                            );
                        };
                        let spec = crate::service_product::ServiceProductSpec {
                            data_dir: data_dir.clone(),
                            ssu2_bind,
                            router_bundle: Arc::clone(&bundle),
                            service_tunnels: Arc::new(service_tunnels.tunnels.clone()),
                            aliases: Arc::new(service_tunnels.aliases.clone()),
                            aggregate_connection_ceiling: service_tunnels
                                .limits
                                .max_active_connections_aggregate,
                            per_service_connection_ceiling: service_tunnels
                                .limits
                                .max_active_connections_per_service,
                            reference: None,
                            options: crate::service_product::ServiceProductOptions::default(),
                            addressbook: addressbook.shared(),
                        };
                        match crate::service_product::ServiceProduct::start_over_existing_daemon(
                            spec,
                            handle,
                            children.clone(),
                            token.clone(),
                            router_infos,
                            router_info_store_config,
                        )
                        .await
                        {
                            Ok(product) => NormalSsu2Owner::Groups(Box::new(product)),
                            Err(error) => {
                                let detail = i2pr_core::HealthDetail::new(format!(
                                    "Destination-group product failed before readiness: {error}"
                                ))
                                .ok();
                                return i2pr_runtime::ServiceResult::Failed(
                                    i2pr_core::ServiceFailure::new(
                                        i2pr_core::ServiceFailureCategory::InvalidState,
                                        detail,
                                    ),
                                );
                            }
                        }
                    } else {
                        NormalSsu2Owner::Router(handle)
                    };
                    if matches!(
                        &owner,
                        NormalSsu2Owner::Groups(product)
                            if !product.readiness().router_ready()
                    ) {
                        owner.shutdown().await;
                        let detail = i2pr_core::HealthDetail::new(
                            "Destination-group readiness was not established",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
                    if readiness.signal_ready().is_err() {
                        owner.shutdown().await;
                        let detail = i2pr_core::HealthDetail::new(
                            "SSU2 service readiness signal was rejected",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
                    loop {
                        tokio::select! {
                            biased;
                            _ = token.cancelled() => {
                                owner.shutdown().await;
                                return i2pr_runtime::ServiceResult::RequestedShutdown;
                            }
                            inbound = owner.next_inbound() => {
                                let Some(inbound) = inbound else {
                                    owner.shutdown().await;
                                    return i2pr_runtime::ServiceResult::RequestedShutdown;
                                };
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
                                    .unwrap_or(0);
                                let mut served = false;
                                if let Some(state) = floodfill.as_mut() {
                                    let time = i2pr_netdb::FloodfillTime {
                                        wall_ms: now_ms,
                                        monotonic_ms: pump_started
                                            .elapsed()
                                            .as_millis()
                                            .min(u128::from(u64::MAX))
                                            as u64,
                                    };
                                    match state.handle_inbound(&inbound, time) {
                                        Ok(crate::floodfill::FloodfillDispatchOutcome::Ignored(_)) => {}
                                        Ok(_) => {
                                            state
                                                .drain_effects(
                                                    owner.handle(),
                                                    now_ms,
                                                    NORMAL_FLOODFILL_MAX_RECORD_AGE_MS,
                                                    &token,
                                                )
                                                .await;
                                            served = true;
                                        }
                                        Err(_) => {}
                                    }
                                }
                                if owner.process_group_inbound(&inbound).await.is_err() {
                                    owner.shutdown().await;
                                    let detail = i2pr_core::HealthDetail::new(
                                        "Destination-group pool advancement failed",
                                    )
                                    .ok();
                                    return i2pr_runtime::ServiceResult::Failed(
                                        i2pr_core::ServiceFailure::new(
                                            i2pr_core::ServiceFailureCategory::Internal,
                                            detail,
                                        ),
                                    );
                                }
                                if !served {
                                    // Bounded dispatch: success and failure
                                    // both release the inbound bytes with the
                                    // call frame. Unsupported bodies are an
                                    // explicit disposition, not an error.
                                    // Controlled transit stays disabled in
                                    // ordinary product profiles: the live
                                    // transit owner is consulted only as a
                                    // disabled probe so the production
                                    // caller references the live-owner
                                    // module without dispatching.
                                    if let Ok(outcome) =
                                        dispatch_router_i2np(&inbound, now_ms)
                                    {
                                        let _ = controlled_transit_disabled_probe(&outcome);
                                    }
                                }
                            }
                            _ = evaluation.tick() => {
                                if let Some(state) = floodfill.as_mut() {
                                    run_floodfill_evaluation(
                                        state,
                                        owner.handle(),
                                        &bundle,
                                        &data_dir,
                                        floodfill_config_path.as_deref(),
                                        &token,
                                    )
                                    .await;
                                }
                            }
                            _ = group_tick.tick() => {
                                if owner.advance_groups().await.is_err() {
                                    owner.shutdown().await;
                                    let detail = i2pr_core::HealthDetail::new(
                                        "Destination-group pool advancement failed",
                                    )
                                    .ok();
                                    return i2pr_runtime::ServiceResult::Failed(
                                        i2pr_core::ServiceFailure::new(
                                            i2pr_core::ServiceFailureCategory::Internal,
                                            detail,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register SSU2 service: {e}"))
        })?;
    Ok(())
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
    let (report, bootstrap_handle) = bootstrap_daemon(&config, now_seconds, None)?;
    tracing::info!(
        state = %report.final_state,
        record_count = report.snapshot.record_count,
        floodfill_advertisers = report.snapshot.floodfill_advertisers,
        reseed_attempts = report.snapshot.reseed_attempts,
        "bootstrap pipeline completed"
    );

    let inspection = Arc::new(InspectionHandles::from_config(&config));
    let graph =
        build_daemon_graph_inner(&config, &inspection, Some(Arc::clone(&bootstrap_handle)))?;
    // Publish the local router hash for the Plan 288 inspection plane.
    // The hash is public RouterInfo material; no secret crosses into the
    // control plane. When bootstrap built no local RouterInfo the row
    // stays publish-gated and `router.hash` fails explicitly.
    if let Ok(bootstrap) = bootstrap_handle.lock()
        && let Some(hash) = bootstrap.local_hash()
    {
        match i2pr_netdb::encode(hash.as_bytes()) {
            Ok(hash_b64) => {
                if inspection.publish_router_hash(&hash_b64).is_err() {
                    tracing::warn!("router hash publication rejected");
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "router hash encoding failed");
            }
        }
    }

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
/// The subscriber layers the normal formatted stdout path (gated by
/// the configured `EnvFilter`) with the Plan 295 [`control_sources`]
/// log-ring feed (INFO and above, secret-marker redaction). Repeated
/// initialization is intentionally harmless for embedding tests: the
/// first installed subscriber wins and the global ring keeps feeding
/// it.
pub fn initialize_logging(config: &config::LoggingConfig) {
    use tracing_subscriber::layer::Layer as _;
    use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    let filter = tracing_subscriber::EnvFilter::new(config.filter.clone());
    let ring_layer =
        crate::control_sources::LogRingLayer::new(crate::control_sources::LogRing::global());
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter))
        .with(ring_layer)
        .try_init();
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

    #[test]
    fn daemon_graph_rejects_enabled_service_tunnels_without_provider() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[service_tunnels]\nenabled = true\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\nenabled = true\ntarget = \"127.0.0.1:9090\"\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("service tunnel config is valid");
        let err = build_daemon_graph(&config).expect_err("enabled tunnel must have an owner");
        assert!(matches!(
            err,
            DaemonError::RuntimeSupervisorFailed(message)
                if message.contains("normal-daemon Destination-group provider, which is not active")
        ));
    }

    #[test]
    fn daemon_graph_requires_the_service_tunnel_subsystem_switch() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\n[service_tunnels]\nenabled = false\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\nenabled = true\ntarget = \"127.0.0.1:9090\"\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("service tunnel config is valid");
        let bootstrap = Arc::new(Mutex::new(bootstrap::Bootstrap::new(
            RouterInfoStoreConfig::default(),
            config.reseed.clone(),
        )));
        let inspection = Arc::new(InspectionHandles::from_config(&config));
        let error = build_daemon_graph_inner(&config, &inspection, Some(bootstrap))
            .expect_err("individual enabled tunnel must honor the subsystem switch");
        assert!(matches!(
            error,
            DaemonError::RuntimeSupervisorFailed(message)
                if message.contains("service_tunnels.enabled = true")
        ));
    }

    #[test]
    fn daemon_graph_registers_enabled_groups_with_bootstrap_provider() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("not-created");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\n[service_tunnels]\nenabled = true\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\nenabled = true\ntarget = \"127.0.0.1:9090\"\n",
            path.to_string_lossy()
        );
        let config = Config::parse(&text).expect("service tunnel config is valid");
        let bootstrap = Arc::new(Mutex::new(bootstrap::Bootstrap::new(
            RouterInfoStoreConfig::default(),
            config.reseed.clone(),
        )));
        let inspection = Arc::new(InspectionHandles::from_config(&config));
        let graph = build_daemon_graph_inner(&config, &inspection, Some(bootstrap))
            .expect("normal graph accepts enabled groups with their production provider");
        assert!(
            graph
                .startup_order()
                .iter()
                .any(|service| service.as_str() == "ssu2-router")
        );
    }
}
