//! Non-networked CLI shell and future daemon composition root.
//!
//! This crate validates configuration and exposes the explicit local identity
//! lifecycle boundary. It does not open listeners, download reseed data, or
//! claim support for any I2P transport or application protocol.

#![forbid(unsafe_code)]

pub mod addressbook;
mod addressbook_fetch;
pub mod app_gateway;
pub mod app_manager_bridge;
pub mod app_runtime;
#[cfg(test)]
mod app_runtime_qualification;
mod bandwidth_eligibility;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod console;
pub mod control_sources;
pub mod destination_peers;
pub mod destination_streaming;
pub mod destination_tunnels;
pub mod encrypted_service_resolver;
pub mod encrypted_target_credential;
pub mod error;
pub mod exploratory_build;
pub mod floodfill;
pub mod i2cp;
pub mod i2pcontrol;
pub mod i2pcontrol_dispatch;
pub mod i2pcontrol_inspection;
pub mod i2pcontrol_tunnels;
pub mod inbound_dispatch;
pub mod netdb_seam;
pub mod netdb_tunnels;
mod news;
pub mod outbound_lookup;
pub mod outbound_secret;
pub mod outproxy_options;
pub mod outproxy_route;
pub mod peer_test;
pub mod router_i2np;
pub mod sam;
pub mod service_delivery;
pub mod service_els2;
pub mod service_generation;
mod service_lifecycle;
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
pub mod transit_volume;
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
use i2pr_netdb::{
    LocalRouterInfoBuilder, RouterInfoStore, RouterInfoStoreConfig, ValidationContext,
};
use i2pr_netdb_persist::{ReseedIngestLimits, ReseedIngestor};
use i2pr_proto::Date;
use i2pr_runtime::{ServiceClassification, ServiceName, ServiceSpec};
use i2pr_storage::{IdentityStore, Ssu2TransportIdentityStore};
use rand_core::TryRngCore;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Bounded startup deadline installed on every supervised daemon service.
///
/// The supervisor treats a startup or readiness timeout as fatal for the
/// whole graph, so every service this composition root registers must
/// report readiness inside this deadline. The value is reported verbatim in
/// the startup diagnostic (Plan 360 scope item 2).
const SERVICE_STARTUP_DEADLINE: Duration = Duration::from_secs(30);

/// Bounded deadline for the initial readiness signal of one service attempt.
const SERVICE_READINESS_DEADLINE: Duration = Duration::from_secs(30);

/// Bounded graceful stop period for one supervised service.
const SERVICE_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

fn revalidate_ntcp2_peer_snapshot(
    candidates: Vec<i2pr_netdb::ValidatedRouterInfo>,
    now_ms: u64,
) -> Vec<i2pr_netdb::ValidatedRouterInfo> {
    candidates
        .into_iter()
        .filter_map(|candidate| {
            i2pr_netdb::ValidatedRouterInfo::from_router_info(
                candidate.router_info().clone(),
                Some(candidate.key()),
                ValidationContext::new(Date::from_millis(now_ms)),
            )
            .ok()
        })
        .collect()
}

fn spawn_controlled_request_driven_lookup(
    children: &i2pr_runtime::ChildScope,
    bootstrap: Option<Arc<Mutex<bootstrap::Bootstrap>>>,
    mut local_router_info: tokio::sync::watch::Receiver<Option<Arc<Vec<u8>>>>,
    delivery: crate::router_i2np::RouterDeliveryService,
    registry: crate::router_i2np::Ntcp2RouterDeliveryRegistry,
    maximum_pending: usize,
) {
    if children
        .spawn(move |cancellation| async move {
        tracing::info!("controlled request-driven NTCP2 lookup worker started");
        let Some(bootstrap) = bootstrap else {
            tracing::warn!("controlled request-driven NTCP2 lookup has no bootstrap owner");
            return Ok(());
        };
        let local_info_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let local_router_info = loop {
            if let Some(info) = local_router_info.borrow_and_update().as_ref().cloned() {
                break info;
            }
            if cancellation.is_cancelled() || tokio::time::Instant::now() >= local_info_deadline {
                tracing::warn!("controlled request-driven NTCP2 lookup has no published local RouterInfo");
                return Ok(());
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                changed = local_router_info.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                }
            }
        };
        // Let the controlled peer fixture finish binding its listener before
        // the first request-driven attempt. This delay is confined to the
        // opt-in qualification probe and remains below the dial deadline.
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(()),
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        let local_info = i2pr_proto::RouterInfo::decode(
            local_router_info.as_ref(),
            i2pr_proto::MAX_COMMON_STRUCTURE_SIZE,
        );
        let local_hash = local_info
            .ok()
            .and_then(|info| i2pr_crypto::router_identity_hash(info.router_identity()).ok());
        let peer_deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let candidate = loop {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok());
            let Some(now_ms) = now_ms else {
                tracing::warn!("controlled request-driven NTCP2 lookup has no wall clock");
                return Ok(());
            };
            let snapshot = bootstrap
                .lock()
                .ok()
                .map(|owner| owner.validated_router_info_snapshot())
                .unwrap_or_default();
            let snapshot_count = snapshot.len();
            let peers = revalidate_ntcp2_peer_snapshot(snapshot, now_ms);
            let validated_count = peers.len();
            if let Some(candidate) = peers.into_iter().find(|candidate| {
                local_hash.is_none_or(|hash| candidate.key().as_bytes() != hash.as_bytes())
            }) {
                break candidate;
            }
            if cancellation.is_cancelled() || tokio::time::Instant::now() >= peer_deadline {
                tracing::warn!(
                    snapshot_records = snapshot_count,
                    revalidated_records = validated_count,
                    local_hash_available = local_hash.is_some(),
                    "controlled request-driven NTCP2 lookup found no eligible peer"
                );
                return Ok(());
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        };
        let peer = i2pr_transport::PeerId::from_hash(i2pr_proto::Hash::from_bytes(
            *candidate.key().as_bytes(),
        ));
        let mut lookup_key = [0_u8; 32];
        if i2pr_crypto::OsRng.try_fill_bytes(&mut lookup_key).is_err() {
            tracing::warn!("controlled request-driven NTCP2 lookup randomness unavailable");
            return Ok(());
        }
        let lookup_key = i2pr_proto::Hash::from_bytes(lookup_key);
        let Some(now_ms) = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        else {
            tracing::warn!("controlled request-driven NTCP2 lookup has no wall clock");
            return Ok(());
        };
        let Ok(message) = crate::router_i2np::build_initiator_database_lookup_probe(
            local_router_info.as_ref(),
            lookup_key,
            now_ms,
        ) else {
            tracing::warn!("controlled request-driven NTCP2 DatabaseLookup encoding failed");
            return Ok(());
        };
        let timeout = Duration::from_secs(10);
        if let Some(link_id) = registry.current_peer_link(peer) {
            let Some(now_ms) = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            else {
                tracing::warn!("controlled request-driven NTCP2 lookup has no wall clock");
                return Ok(());
            };
            if !registry.register_lookup(
                peer,
                link_id,
                lookup_key,
                now_ms,
                now_ms.saturating_add(120_000),
                maximum_pending,
            ) {
                tracing::warn!("controlled request-driven NTCP2 response correlation was not admitted");
                return Ok(());
            }
            let Ok(request) =
                crate::router_i2np::RouterDeliveryRequest::new(peer, message, timeout)
            else {
                registry.cancel_lookup(peer, link_id);
                return Ok(());
            };
            let (outcome, selected) = delivery.deliver_with_transport(request, &cancellation);
            if outcome == crate::router_i2np::RouterDeliveryOutcome::Accepted
                && selected == Some(crate::router_i2np::RouterDeliveryTransport::Ntcp2(link_id))
            {
                tracing::info!(
                    link_generation = link_id.value(),
                    "controlled request-driven DatabaseLookup submitted over authenticated NTCP2"
                );
            } else {
                registry.cancel_lookup(peer, link_id);
                tracing::warn!(
                    outcome = ?outcome,
                    "controlled request-driven DatabaseLookup did not use its current authenticated NTCP2 generation"
                );
            }
            return Ok(());
        }
        let Ok(first_request) =
            crate::router_i2np::RouterDeliveryRequest::new(peer, message.clone(), timeout)
        else {
            tracing::warn!("controlled request-driven NTCP2 DatabaseLookup request rejected");
            return Ok(());
        };
        let (first_outcome, _) = delivery.deliver_with_transport(first_request, &cancellation);
        if first_outcome != crate::router_i2np::RouterDeliveryOutcome::NoActiveSession {
            tracing::warn!(
                outcome = ?first_outcome,
                "controlled request-driven NTCP2 lookup did not begin with a route miss"
            );
            return Ok(());
        }
        tracing::info!(
            "controlled product DatabaseLookup requested a peer dial after route miss"
        );

        let dial_deadline = tokio::time::Instant::now() + Duration::from_secs(90);
                        let mut poll = tokio::time::interval(Duration::from_secs(1));
        poll.tick().await;
        let mut retries = 0_u8;
        let link_id = loop {
            if cancellation.is_cancelled() || tokio::time::Instant::now() >= dial_deadline {
                tracing::warn!("controlled request-driven NTCP2 peer dial did not complete");
                return Ok(());
            }
            if let Some(link_id) = registry.current_outbound_link(peer) {
                break link_id;
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                _ = poll.tick() => {
                    if retries < 20 {
                        retries += 1;
                        if registry.request_outbound_dial(peer) {
                            tracing::info!(
                                retry = retries,
                                "controlled request-driven NTCP2 peer dial request retried"
                            );
                        }
                    }
                }
            }
        };
        let expires_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .unwrap_or(0)
            .saturating_add(120_000);
        let now_ms = expires_ms.saturating_sub(120_000);
        if !registry.register_lookup(
            peer,
            link_id,
            lookup_key,
            now_ms,
            expires_ms,
            maximum_pending,
        ) {
            tracing::warn!("controlled request-driven NTCP2 response correlation was not admitted");
            return Ok(());
        }
        let Ok(retry) =
            crate::router_i2np::RouterDeliveryRequest::new(peer, message, timeout)
        else {
            registry.cancel_lookup(peer, link_id);
            return Ok(());
        };
        let (outcome, selected) = delivery.deliver_with_transport(retry, &cancellation);
        if outcome == crate::router_i2np::RouterDeliveryOutcome::Accepted
            && selected == Some(crate::router_i2np::RouterDeliveryTransport::Ntcp2(link_id))
        {
            tracing::info!(
                link_generation = link_id.value(),
                "controlled request-driven DatabaseLookup submitted over authenticated NTCP2"
            );
        } else {
            registry.cancel_lookup(peer, link_id);
            tracing::warn!(
                outcome = ?outcome,
                "controlled request-driven DatabaseLookup retry did not use its authenticated NTCP2 generation"
            );
        }
        Ok(())
        })
        .is_err()
    {
        tracing::warn!("controlled request-driven NTCP2 lookup task could not be supervised");
    }
}

/// Bounded deadline for one loopback listener bind.
///
/// Applies to `bind` only. The serving phase runs until cancellation: a
/// timeout wrapped around the listener *lifetime* would abort a healthy
/// service instead of starting one.
const LISTENER_BIND_DEADLINE: Duration = Duration::from_secs(1);

/// What one supervised service is waiting for when it reports readiness.
///
/// This is the single source of truth for the readiness contract in the
/// composition root: it is installed as the service's bounded
/// [`ServiceSpec::description`] and quoted in the startup diagnostic when a
/// service fails to become ready. Readiness means **"this service is
/// running"** — never "some unrelated task finished" and never "we hope":
///
/// * a cancellation-scoped lifetime anchor is up the instant it is
///   scheduled, so it signals immediately;
/// * a listener-owning service signals only once its loopback socket is
///   actually bound, because an unbound listener is not a running service;
/// * a periodic worker signals once its cadence loop is established, not
///   once its first remote fetch happens to succeed.
fn readiness_expectation(service: &str) -> &'static str {
    match service {
        LIFECYCLE_SERVICE_NAME => "cancellation-scoped lifetime anchor running",
        NETDB_BOOTSTRAP_SERVICE_NAME => "long-lived NetDB observability owner running",
        SAM_SERVICE_NAME => "loopback SAM listener bound and accepting",
        I2CP_SERVICE_NAME => "loopback I2CP listener bound and accepting",
        I2PCONTROL_SERVICE_NAME => "loopback I2PControl listener bound and accepting",
        CONSOLE_SERVICE_NAME => "loopback router console listener bound and accepting",
        ADDRESSBOOK_REFRESH_SERVICE_NAME => "address-book refresh cadence loop running",
        SIGNED_NEWS_REFRESH_SERVICE_NAME => "signed-news refresh cadence loop running",
        NTCP2_SERVICE_NAME => "controlled NTCP2 TCP listener bound with local RouterInfo ready",
        SSU2_SERVICE_NAME => "authenticated router I2NP owner running",
        APP_RUNTIME_SERVICE_NAME => {
            "i2pr-appd manager child spawned and manager handshake completed"
        }
        _ => "supervised service running",
    }
}

/// Builds a service spec carrying this composition root's bounded deadlines
/// and its readiness expectation.
fn service_spec<F, Fut>(
    service: &'static str,
    classification: ServiceClassification,
    factory: F,
) -> ServiceSpec
where
    F: Fn(i2pr_runtime::ServiceContext) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = i2pr_runtime::ServiceResult> + Send + 'static,
{
    ServiceSpec::new(
        ServiceName::new(service).expect("static service name is valid"),
        classification,
        factory,
    )
    .timeouts(
        SERVICE_STARTUP_DEADLINE,
        SERVICE_READINESS_DEADLINE,
        SERVICE_SHUTDOWN_GRACE,
    )
    .description(readiness_expectation(service))
}

/// Reports that one service is running, or fails the service loudly.
///
/// A service that cannot publish its own readiness must name itself in the
/// diagnostic instead of letting the supervisor time it out anonymously.
/// Returns the failure as a [`i2pr_runtime::ServiceResult`] so callers can
/// either `return` it directly or propagate it with `?` from a service body
/// or from [`bind_then_serve`].
fn signal_running(
    service: &'static str,
    readiness: &i2pr_runtime::Readiness,
) -> Result<(), i2pr_runtime::ServiceResult> {
    readiness.signal_ready().map_err(|error| {
        let detail =
            i2pr_core::HealthDetail::new(format!("{service} could not report readiness: {error}"))
                .ok();
        i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
            i2pr_core::ServiceFailureCategory::Internal,
            detail,
        ))
    })
}

/// Binds one supervised loopback listener, reports readiness, then serves.
///
/// Readiness is signalled **after** the bind succeeds, so a supervisor that
/// observes readiness can prove a listener exists. `serve` is awaited
/// unbounded: it owns the listener lifetime and returns when the token
/// fires, so a timeout must never wrap the serving phase.
async fn bind_then_serve<Bind, Serve, BindOk, BindErr, ServeFut>(
    service: &'static str,
    readiness: &i2pr_runtime::Readiness,
    bind: Bind,
    serve: Serve,
) -> i2pr_runtime::ServiceResult
where
    Bind: std::future::Future<Output = Result<BindOk, BindErr>>,
    BindErr: std::fmt::Display,
    Serve: FnOnce(BindOk) -> ServeFut,
    ServeFut: std::future::Future<Output = i2pr_runtime::ServiceResult>,
{
    let bound = i2pr_runtime::bounded_timeout(LISTENER_BIND_DEADLINE, bind).await;
    let listener = match bound {
        Ok(Ok(listener)) => listener,
        Ok(Err(error)) => {
            let detail =
                i2pr_core::HealthDetail::new(format!("{service} bind failed: {error}")).ok();
            return i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                i2pr_core::ServiceFailureCategory::InvalidState,
                detail,
            ));
        }
        Err(_) => {
            let detail = i2pr_core::HealthDetail::new(format!(
                "{service} listener did not bind within {LISTENER_BIND_DEADLINE:?}"
            ))
            .ok();
            return i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                i2pr_core::ServiceFailureCategory::Internal,
                detail,
            ));
        }
    };
    if let Err(failure) = signal_running(service, readiness) {
        return failure;
    }
    serve(listener).await
}

/// Renders one supervised startup failure as an operator-facing diagnostic.
///
/// The supervisor reports the service and the completion variant; on its own
/// that is a bare `ReadinessTimeout` with no deadline and no subject. Plan
/// 360 scope item 2 requires the operator to learn **which service**, **which
/// deadline**, and **what it awaited**.
fn describe_startup_failure(
    service: &i2pr_core::ServiceName,
    completion: &i2pr_core::ServiceCompletion,
) -> String {
    let awaited = readiness_expectation(service.as_str());
    let reason = match completion {
        i2pr_core::ServiceCompletion::Failed(failure) => failure
            .detail()
            .map(|detail| detail.as_str().to_owned())
            .unwrap_or_else(|| format!("{:?}", failure.category())),
        i2pr_core::ServiceCompletion::ReadinessTimeout => {
            "it never reported initial readiness".to_owned()
        }
        i2pr_core::ServiceCompletion::StartupTimeout => "it never completed startup".to_owned(),
        other => format!("{other:?}"),
    };
    format!(
        "service `{service}` failed during startup: {reason} \
         (startup deadline {SERVICE_STARTUP_DEADLINE:?}, \
         readiness deadline {SERVICE_READINESS_DEADLINE:?}; \
         readiness means {awaited})"
    )
}

/// Renders any supervisor error, enriching the startup case.
fn describe_supervisor_error(error: &i2pr_runtime::SupervisorError) -> String {
    match error {
        i2pr_runtime::SupervisorError::StartupFailed {
            service,
            completion,
            ..
        } => format!(
            "supervisor failed: {}",
            describe_startup_failure(service, completion)
        ),
        other => format!("supervisor failed: {other}"),
    }
}

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
    let lifecycle = crate::service_lifecycle::ServiceLifecycleController::new();
    let graph = build_daemon_graph_inner(config, &inspection, None, lifecycle)?;
    Ok((graph, inspection))
}

/// Shared graph construction over explicit inspection handles.
fn build_daemon_graph_inner(
    config: &Config,
    inspection: &Arc<InspectionHandles>,
    bootstrap: Option<Arc<Mutex<bootstrap::Bootstrap>>>,
    service_lifecycle: crate::service_lifecycle::ServiceLifecycleController,
) -> Result<i2pr_runtime::ServiceGraph, DaemonError> {
    let has_enabled_service_tunnels = config
        .service_tunnels
        .tunnels
        .tunnels
        .iter()
        .any(|tunnel| tunnel.enabled);
    if !has_enabled_service_tunnels {
        service_lifecycle.set_status(crate::service_lifecycle::LifecycleStatus::NoGroups);
    }
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
    if config.transport.ntcp2.enabled
        && config.transport.ntcp2.qualification_profile
            != crate::config::Ntcp2QualificationProfile::ControlledLoopback
    {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "enabled NTCP2 requires the controlled-loopback qualification profile".to_string(),
        ));
    }
    if config.transport.ntcp2.enabled && config.network.network_id > u16::from(u8::MAX) {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "controlled NTCP2 network ID exceeds the handshake width".to_owned(),
        ));
    }

    let bandwidth = if config.bandwidth.enabled {
        Some(
            i2pr_runtime::BandwidthGovernor::new(i2pr_runtime::BandwidthGovernorConfig {
                inbound_bytes_per_second: config.bandwidth.inbound_bytes_per_second,
                outbound_bytes_per_second: config.bandwidth.outbound_bytes_per_second,
                burst_bytes: config.bandwidth.burst_bytes,
                control_reserve_percent: config.bandwidth.control_reserve_percent,
                max_pending_requests: config.bandwidth.max_pending_requests,
                max_pending_per_peer: config.bandwidth.max_pending_per_peer,
            })
            .map_err(|_| {
                DaemonError::RuntimeSupervisorFailed(
                    "normalized bandwidth limits failed runtime validation".to_owned(),
                )
            })?,
        )
    } else {
        None
    };

    let mut builder = i2pr_runtime::ServiceGraph::builder(i2pr_runtime::MAX_SERVICE_COUNT)
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to create service graph: {e}"))
        })?;

    let group_lifecycle = service_lifecycle.clone();
    builder
        .register(service_spec(
            LIFECYCLE_SERVICE_NAME,
            ServiceClassification::Essential,
            move |ctx| {
                let cancellation = ctx.cancellation().clone();
                let readiness = ctx.readiness();
                Box::pin(async move {
                    // Plan 360 readiness contract: this service owns no
                    // listener and no work item — it is the cancellation-
                    // scoped anchor that keeps the supervisor's lifetime
                    // tied to the daemon. It is up the instant it is
                    // scheduled, so reporting readiness immediately is
                    // truthful. Reporting it only after cancellation would
                    // time the service out and tear the whole graph down.
                    if let Err(failure) = signal_running(LIFECYCLE_SERVICE_NAME, &readiness) {
                        return failure;
                    }
                    cancellation.cancelled().await;
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register service: {e}"))
        })?;

    builder
        .register(service_spec(
            NETDB_BOOTSTRAP_SERVICE_NAME,
            ServiceClassification::Essential,
            move |ctx| {
                let cancellation = ctx.cancellation().clone();
                let readiness = ctx.readiness();
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
                    //
                    // Plan 360: readiness deliberately does **not** wait
                    // on that pipeline, on NetDB population, or on a
                    // reseed source. Those have already run (or failed
                    // non-fatally) before the supervisor is constructed,
                    // and a router with an empty NetDB is still a running
                    // router. Readiness means "this service is running".
                    if let Err(failure) = signal_running(NETDB_BOOTSTRAP_SERVICE_NAME, &readiness) {
                        return failure;
                    }
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
    // Plan 340: the product's transit participation posture. Production
    // composition never constructs a transit data-plane owner — the
    // inbound path consults `controlled_transit_disabled_probe` instead —
    // so this router relays nothing and says so. The `Disabled` variant
    // owns no counters, so the zeros the three transit selectors report
    // are a property of the enforced posture rather than a substituted
    // measurement. Enabling participation is a separate decision and is
    // deliberately not reachable from here.
    inspection.publish_transit_participation(crate::transit_volume::TransitParticipation::Disabled);
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
    // No build coordinator is installed in this composition, so the
    // separate Tunnel Build Message queue is authoritatively empty.
    inspection.publish_tbm_queue(0);

    // Plan 337: build the ONE service-tunnel manager here and inject the
    // same `Arc` into both owners — the I2PControl control state and the
    // destination-group product. Plan 289 required "one existing M10
    // `ServiceTunnelManager`"; until now each owner built a private one,
    // so a control-created server was never on the publication path.
    //
    // Construction is filesystem-only and touches no network. The
    // executable router delivery backend is installed by the product when
    // the SSU2 service starts, which the graph now orders ahead of the
    // I2PControl service, so a control reconcile can never land on a
    // manager that cannot deliver.
    let shared_service_manager = build_shared_service_manager(config)?;

    if config.sam.enabled {
        register_sam_service(
            &mut builder,
            config,
            inspection,
            &addressbook,
            shared_service_manager.clone(),
        )?;
    }

    if config.app_runtime.enabled {
        register_app_runtime_service(&mut builder, config, &addressbook)?;
    }

    if config.i2cp.enabled {
        register_i2cp_service(&mut builder, config, inspection)?;
    }

    if config.console.enabled {
        register_console_service(&mut builder, config, inspection)?;
    }

    if config.i2pcontrol.enabled {
        let manager = shared_service_manager.clone().ok_or_else(|| {
            DaemonError::RuntimeSupervisorFailed(
                "I2PControl requires the shared service-tunnel manager".to_owned(),
            )
        })?;
        register_i2pcontrol_service(&mut builder, config, inspection, &addressbook, manager)?;
    }

    if addressbook.is_active() {
        register_addressbook_refresh_service(&mut builder, &addressbook)?;
    }

    if config.news.enabled {
        register_news_refresh_service(&mut builder, config, inspection)?;
    }

    // NTCP2 and SSU2 feed one bounded router-I2NP ingress owner. Keeping
    // the bridge here makes the composition explicit: the NTCP2 link task
    // authenticates and frames bytes, while the existing router product
    // owns decode, NetDB, tunnel, and control effects.
    let router_ingress_budget = i2pr_core::ResourceBudget::new([i2pr_core::ResourceLimit {
        class: i2pr_core::ResourceClass::BufferedBytes,
        maximum: NORMAL_ROUTER_INGRESS_CAPACITY as u64
            * crate::router_i2np::MAX_ROUTER_I2NP_BYTES as u64,
    }])
    .expect("static router ingress budget is valid");
    let ntcp2_delivery = crate::router_i2np::Ntcp2RouterDeliveryRegistry::default();
    let (router_ingress_tx, router_ingress_rx) =
        tokio::sync::mpsc::channel(NORMAL_ROUTER_INGRESS_CAPACITY);
    let (local_router_info_tx, local_router_info_rx) =
        tokio::sync::watch::channel::<Option<Arc<Vec<u8>>>>(None);
    let ntcp2_ingress = Ntcp2IngressOwner {
        sender: router_ingress_tx.clone(),
        byte_budget: router_ingress_budget,
        delivery: ntcp2_delivery,
    };

    if config.transport.ntcp2.enabled {
        register_ntcp2_service(
            &mut builder,
            Ntcp2ServiceRegistration {
                config: &config.transport.ntcp2,
                ssu2_config: &config.ssu2,
                controlled_dual_transport: config.floodfill.qualification_profile
                    == crate::config::FloodfillQualificationProfile::ControlledLoopback,
                send_initiator_lookup_probe: config.floodfill.send_initiator_lookup_probe,
                network_id: config.network.network_id,
                data_dir: &config.router.data_dir,
                bandwidth: bandwidth.clone(),
                bootstrap: bootstrap.clone(),
                ingress: ntcp2_ingress.clone(),
                local_router_info: local_router_info_tx.clone(),
            },
        )?;
    }

    if config.ssu2.enabled || config.transport.ntcp2.enabled {
        register_ssu2_service(
            &mut builder,
            config,
            inspection,
            &addressbook,
            bootstrap,
            group_lifecycle,
            shared_service_manager,
            bandwidth,
            router_ingress_rx,
            router_ingress_tx.clone(),
            ntcp2_ingress.delivery.clone(),
            local_router_info_tx,
            local_router_info_rx,
        )?;
    }

    builder
        .build()
        .map_err(|e| DaemonError::RuntimeSupervisorFailed(format!("invalid service graph: {e}")))
}

/// Registers the supervised managed-application manager child (Plan 369).
///
/// The service is `Restartable`, not `Essential`: invariant 1 requires the
/// router to stay fully usable when the app runtime is disabled, absent, or
/// broken, so a manager that cannot be resolved, cannot be spawned, or dies
/// repeatedly must degrade this feature only. `RestartPolicy::Degrade` on
/// exhaustion is what guarantees that — the alternative, shutting the router
/// down because an application manager crashed, would turn an optional feature
/// into an availability dependency.
///
/// It is additionally registered with [`i2pr_core::StartupRequirement::Optional`]
/// (Plan 371), which is a different guarantee from the restart policy. The
/// restart policy covers *repeated failure*; the startup requirement covers a
/// manager that never becomes usable at all — a handshake that never completes
/// before the readiness deadline, for instance. Without it, such a manager
/// would abort router startup even though the router has no other use for it.
/// `Optional` also excludes this service from `SupervisorSnapshot::ready`, so a
/// broken manager does not leave a usable router permanently reporting that it
/// is not ready.
///
/// It depends on the SAM and I2CP services only for their *configuration*: the
/// bridge uses them to open backend connections for an application session, so
/// a manager session that opened a SAM stream needs a SAM configuration to
/// exist. There is no listener dependency and no loopback port is involved.
fn register_app_runtime_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
) -> Result<(), DaemonError> {
    let state_root = crate::app_runtime::prepare_state_root(&config.router.data_dir)
        .map_err(DaemonError::RuntimeSupervisorFailed)?;
    let inputs = crate::app_runtime::AppRuntimeInputs {
        sam: config.sam.clone(),
        i2cp: config.i2cp.clone(),
        addressbook: addressbook.shared(),
        state_root,
        manager_path_override: crate::app_runtime::manager_path_override(),
    };
    // Preflight the manager before registering the service.
    //
    // The service is startup-optional (Plan 371), so an unresolvable sibling
    // would no longer abort the router — it would degrade silently. That is the
    // right behaviour for a manager that is *present but broken*, and the wrong
    // one for a feature the operator explicitly enabled without installing its
    // manager: the operator would get a usable router with no indication that
    // the app runtime they asked for never started. Resolving the path here
    // turns that case into an actionable configuration error at composition
    // time, and it costs no authority: the same resolution the service will use
    // is checked up front.
    if let Err(error) = inputs.manager_path() {
        return Err(DaemonError::RuntimeSupervisorFailed(format!(
            "the managed-application runtime is enabled but unusable: {error}"
        )));
    }
    let mut spec = service_spec(
        APP_RUNTIME_SERVICE_NAME,
        ServiceClassification::Restartable,
        move |ctx| {
            let inputs = inputs.clone();
            Box::pin(async move { crate::app_runtime::run_manager_service(ctx, inputs).await })
        },
    );
    spec = spec
        .restart_policy(
            i2pr_runtime::RestartPolicy::new(
                APP_RUNTIME_RESTART_ATTEMPTS,
                APP_RUNTIME_RESTART_INITIAL_DELAY,
                APP_RUNTIME_RESTART_MAX_DELAY,
            )
            .map_err(|error| {
                DaemonError::RuntimeSupervisorFailed(format!(
                    "invalid app runtime restart policy: {error}"
                ))
            })?
            .reset_after_ready(APP_RUNTIME_RESTART_RESET)
            .map_err(|error| {
                DaemonError::RuntimeSupervisorFailed(format!(
                    "invalid app runtime restart reset: {error}"
                ))
            })?
            .on_exhaustion(i2pr_runtime::RestartExhaustion::Degrade),
        )
        .startup_requirement(i2pr_core::StartupRequirement::Optional)
        .depends_on(i2pr_runtime::ServiceName::new(LIFECYCLE_SERVICE_NAME).expect("static name"));
    builder.register(spec).map_err(|error| {
        DaemonError::RuntimeSupervisorFailed(format!("failed to register service: {error}"))
    })?;
    Ok(())
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
    shared_service_manager: Option<Arc<crate::service_tunnels::ServiceTunnelManager>>,
) -> Result<(), DaemonError> {
    let sam_config = config.sam.clone();
    let sam_product_mode = config.sam_product_mode;
    let address = sam_config.bind_socket();
    let inspection = Arc::clone(inspection);
    let addressbook = Arc::clone(addressbook);
    let shared_service_manager = shared_service_manager.clone();
    builder
        .register(service_spec(
            SAM_SERVICE_NAME,
            ServiceClassification::Optional,
            move |ctx| {
                let sam_config = sam_config.clone();
                let sam_product_mode = sam_product_mode;
                let inspection = Arc::clone(&inspection);
                let addressbook = Arc::clone(&addressbook);
                let shared_service_manager = shared_service_manager.clone();
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                let readiness = ctx.readiness();
                Box::pin(async move {
                    let state_result = match sam_product_mode {
                        crate::config::SamProductMode::RouterNetwork => {
                            SamServiceState::new_router_network(sam_config)
                        }
                        crate::config::SamProductMode::LocalOnly => {
                            SamServiceState::new_local_only(sam_config)
                        }
                    };
                    let state = match state_result {
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
                    if sam_product_mode == crate::config::SamProductMode::RouterNetwork
                        && let Some(manager) = shared_service_manager
                    {
                        state.attach_router_network_manager(manager);
                    }
                    // Publish the live owner for the Plan 288
                    // inspection plane (read-only snapshot access).
                    inspection.publish_sam(Arc::clone(&state));
                    // Install the Plan 294 canonical resolver cell (a
                    // no-op empty cell unless the subsystem is active).
                    state.set_addressbook_handle(addressbook.shared());
                    let token = cancellation.clone();
                    let binder = Arc::clone(&state);
                    bind_then_serve(
                        SAM_SERVICE_NAME,
                        &readiness,
                        binder.bind(address),
                        |(listener, _bound_address)| async move {
                            match state.serve(listener, children, token).await {
                                Ok(()) => i2pr_runtime::ServiceResult::RequestedShutdown,
                                Err(error) => {
                                    let detail = i2pr_core::HealthDetail::new(format!(
                                        "SAM serve failed: {error}"
                                    ))
                                    .ok();
                                    i2pr_runtime::ServiceResult::Failed(
                                        i2pr_core::ServiceFailure::new(
                                            i2pr_core::ServiceFailureCategory::Internal,
                                            detail,
                                        ),
                                    )
                                }
                            }
                        },
                    )
                    .await
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register SAM service: {e}"))
        })?;
    Ok(())
}

/// Registers the supervised loopback router console.
///
/// The console is experimental, loopback-only, and disabled by default,
/// exactly like SAM, I2CP, and I2PControl. This module owns the listener
/// and the EggServe lifecycle; the console crate owns only the browser
/// application. A construction or bind failure fails the service rather
/// than degrading to a half-open console.
fn register_console_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
) -> Result<(), DaemonError> {
    let console_config = config.console.clone();
    let address = console_config.bind_socket();
    let inspection = Arc::clone(inspection);
    let spec = service_spec(
        CONSOLE_SERVICE_NAME,
        i2pr_runtime::ServiceClassification::Optional,
        move |ctx| {
            let console_config = console_config.clone();
            let inspection = Arc::clone(&inspection);
            let cancellation = ctx.cancellation().clone();
            let readiness = ctx.readiness();
            Box::pin(async move {
                let state =
                    match crate::console::ConsoleServiceState::new(console_config, inspection) {
                        Ok(state) => state,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "console service construction failed: {error}"
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
                // Bind under a bounded deadline so an unusable address
                // fails the service promptly. The serving phase then runs
                // until cancellation: wrapping the listener lifetime in a
                // timeout would abort a healthy console after one second.
                // Plan 360: readiness is signalled after the bind succeeds
                // and before serving, so a ready console provably owns a
                // bound loopback socket.
                let token = cancellation.clone();
                let binder = Arc::clone(&state);
                bind_then_serve(
                    CONSOLE_SERVICE_NAME,
                    &readiness,
                    binder.bind(address),
                    |(listener, resolved)| async move {
                        match state.serve(listener, resolved, token).await {
                            Ok(()) => i2pr_runtime::ServiceResult::RequestedShutdown,
                            Err(error) => {
                                let detail = i2pr_core::HealthDetail::new(format!(
                                    "console serve failed: {error}"
                                ))
                                .ok();
                                i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::Internal,
                                    detail,
                                ))
                            }
                        }
                    },
                )
                .await
            })
        },
    );
    builder.register(spec).map_err(|e| {
        DaemonError::RuntimeSupervisorFailed(format!("failed to register console service: {e}"))
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
    let inspection = Arc::clone(inspection);
    builder
        .register(service_spec(
            I2CP_SERVICE_NAME,
            ServiceClassification::Optional,
            move |ctx| {
                let i2cp_config = i2cp_config.clone();
                let inspection = Arc::clone(&inspection);
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                let readiness = ctx.readiness();
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
                    let binder = Arc::clone(&state);
                    bind_then_serve(
                        I2CP_SERVICE_NAME,
                        &readiness,
                        binder.bind(address),
                        |(listener, _bound_address)| async move {
                            match state.serve(listener, children, token).await {
                                Ok(()) => i2pr_runtime::ServiceResult::RequestedShutdown,
                                Err(error) => {
                                    let detail = i2pr_core::HealthDetail::new(format!(
                                        "I2CP serve failed: {error}"
                                    ))
                                    .ok();
                                    i2pr_runtime::ServiceResult::Failed(
                                        i2pr_core::ServiceFailure::new(
                                            i2pr_core::ServiceFailureCategory::Internal,
                                            detail,
                                        ),
                                    )
                                }
                            }
                        },
                    )
                    .await
                })
            },
        ))
        .map_err(|e| {
            DaemonError::RuntimeSupervisorFailed(format!("failed to register I2CP service: {e}"))
        })?;
    Ok(())
}

/// Builds the one shared service-tunnel manager (Plan 337), or `None`
/// when this configuration can never own a service runtime.
///
/// Both owners receive the same instance: the I2PControl control state
/// and the destination-group product. Exactly one manager therefore owns
/// every service runtime, and a control-created server is delivered and
/// published through the same path as a startup-configured one.
///
/// Plan 297: the explicit TLS identity/trust policy is installed here,
/// before any service prepares, so `use_ssl` tunnels dial under it from
/// their first connection. While each owner built a private manager only
/// the control-owned one ever saw the policy, so startup-owned `use_ssl`
/// services silently ran without it.
///
/// Construction is filesystem-only and touches no network. The executable
/// router delivery backend is installed later, by the product, when the
/// SSU2 service starts; the graph orders that service ahead of
/// I2PControl, so a control reconcile can never land on a manager that
/// cannot deliver.
pub fn build_shared_service_manager(
    config: &Config,
) -> Result<Option<Arc<crate::service_tunnels::ServiceTunnelManager>>, DaemonError> {
    if !(config.i2pcontrol.enabled
        || (config.ssu2.enabled && service_tunnels_active(config))
        || (config.sam.enabled
            && config.sam_product_mode == crate::config::SamProductMode::RouterNetwork))
    {
        return Ok(None);
    }
    let manager = crate::service_tunnels::ServiceTunnelManager::new(
        crate::service_tunnels::ServiceTunnelManagerConfig {
            data_dir: config.router.data_dir.clone(),
            aggregate_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_aggregate,
            per_service_connection_ceiling: config
                .service_tunnels
                .limits
                .max_active_connections_per_service,
            specs: Arc::new(config.service_tunnels.tunnels.clone()),
            aliases: Arc::new(config.service_tunnels.aliases.clone()),
        },
    )
    .map_err(|error| {
        DaemonError::RuntimeSupervisorFailed(format!(
            "failed to build the shared service-tunnel manager: {error}"
        ))
    })?;
    if let Some(policy) = &config.service_tunnels.tls_policy {
        manager.set_service_tls_policy(policy.policy());
    }
    Ok(Some(Arc::new(manager)))
}

/// Builds the one outbound-credential owner for this router (Plan 342).
///
/// Derived **once**, here, and shared as the same `Arc` by the control plane
/// (which seals a credential into a generation file) and every tunnel runtime
/// (which opens it while building a header). Two derived keys would mean a
/// credential sealed by one and opened by the other fails closed for a reason
/// that looks like tampering, so "derive once" is a correctness requirement,
/// not a tidiness one.
///
/// The signing seed is reached through
/// [`RouterIdentityBundle::with_signing_seed`], a closure: the seed is the
/// root of every router identity credential and is never named in a wider
/// scope than the single HKDF derivation that consumes it.
///
/// Fails over to `NoOutboundSecrets` rather than erroring when the router
/// identity cannot be loaded. The consequence is that every outproxy
/// credential field is refused at create/edit time — before any listener or
/// destination is allocated — rather than accepted and silently unusable.
///
/// Plan 380 widens the return type to `RouterSecretOwner`, the supertrait that
/// adds the ELS2 consumer-credential domain. One derivation, one `Arc`, two
/// sealed domains: the composition root still derives the owner exactly once,
/// and a caller that only needs the outproxy half keeps an ordinary
/// `Arc<dyn OutboundSecretStore>` by upcasting.
pub fn build_outbound_secret_store(
    config: &Config,
) -> Arc<dyn i2pr_service_tunnels::outbound_secret::RouterSecretOwner> {
    let identity = IdentityStore::in_data_dir(&config.router.data_dir).load();
    let derived = identity.as_ref().ok().and_then(|bundle| {
        crate::outbound_secret::RouterBoundOutboundSecrets::from_router_identity(bundle).ok()
    });
    match derived {
        Some(store) => Arc::new(store),
        None => Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets),
    }
}

/// Whether the shared network product owner must come up for this configuration.
///
/// Plan 337: the destination-group product owns the one shared
/// `ServiceTunnelManager`, so it must exist whenever a service tunnel can
/// be created — including through the I2PControl control plane, which is
/// how an operator adds the *first* service tunnel to a router that has
/// none configured. Before Plan 337 the gate was "at least one startup
/// tunnel is enabled", which meant a control-created tunnel had no product
/// to be published through. I2PControl is disabled by default, so an
/// ordinary profile is unaffected. Plan 458 also starts the shared owner
/// for enabled router-network SAM, without eagerly creating service groups.
fn service_tunnels_active(config: &Config) -> bool {
    config.i2pcontrol.enabled
        || (config.sam.enabled
            && config.sam_product_mode == crate::config::SamProductMode::RouterNetwork)
        || config
            .service_tunnels
            .tunnels
            .tunnels
            .iter()
            .any(|tunnel| tunnel.enabled)
}

/// Stable identifier of the supervised SSU2 router service. Plan 337
/// makes the I2PControl service depend on it, so the two must not drift
/// apart; [`register_ssu2_service`] registers this exact name.
const SSU2_SERVICE_NAME: &str = "ssu2-router";
/// Stable supervisor identifier for the controlled loopback NTCP2 owner.
const NTCP2_SERVICE_NAME: &str = "ntcp2-router";
// A signed placeholder is required by the runtime constructor before the
// socket exists. It is overwritten from the OS-reported endpoint before the
// service reports readiness or hands the owner to a product consumer.
const SSU2_PREBIND_ROUTER_INFO_PORT: u16 = 44_001;

/// Supervisor service name for the loopback router console (Plan 356).
const CONSOLE_SERVICE_NAME: &str = "router-console";

/// Supervisor service name for the managed-application manager child
/// (Plan 369). This service owns no listener: its "running" state is a live
/// `i2pr-appd` process that has completed the manager-protocol handshake over
/// the inherited anonymous transport.
const APP_RUNTIME_SERVICE_NAME: &str = "app-runtime";

/// Bounded restart budget for the managed-application manager child.
///
/// These are deliberately small and short. A manager that cannot start is
/// almost always a missing or mismatched sibling binary, which restarting
/// cannot fix, so the budget exists to absorb a transient spawn failure and
/// then degrade the feature rather than to keep hammering `exec`.
const APP_RUNTIME_RESTART_ATTEMPTS: u32 = 3;
const APP_RUNTIME_RESTART_INITIAL_DELAY: Duration = Duration::from_millis(250);
const APP_RUNTIME_RESTART_MAX_DELAY: Duration = Duration::from_secs(5);
/// Sustained-ready window after which the attempt counter may reset.
const APP_RUNTIME_RESTART_RESET: Duration = Duration::from_secs(300);

/// Stable identifiers of the remaining supervised services. Plan 360 keeps
/// them in one place so the readiness contract and the registered graph
/// cannot drift apart.
const LIFECYCLE_SERVICE_NAME: &str = "lifecycle";
const NETDB_BOOTSTRAP_SERVICE_NAME: &str = "netdb-bootstrap";
const SAM_SERVICE_NAME: &str = "sam-bridge";
const I2CP_SERVICE_NAME: &str = "i2cp-bridge";
const I2PCONTROL_SERVICE_NAME: &str = "i2pcontrol";
const ADDRESSBOOK_REFRESH_SERVICE_NAME: &str = "addressbook-refresh";
const SIGNED_NEWS_REFRESH_SERVICE_NAME: &str = "signed-news-refresh";

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
    manager: Arc<crate::service_tunnels::ServiceTunnelManager>,
) -> Result<(), DaemonError> {
    let i2pcontrol_config = config.i2pcontrol.clone();
    let address = i2pcontrol_config.bind_socket();
    // Plan 289/337: the control state is built here over the **one**
    // shared manager the product layer also owns, with the store beneath
    // the router data dir. Construction touches only the filesystem;
    // definitions load and reconcile at service startup. A construction
    // failure fails the service fail-closed without touching the network.
    // Plan 342: one derived owner, handed to the control plane. The same
    // `Arc` reaches the per-tunnel runtimes through `control.outbound_secrets()`.
    let outbound_secrets = build_outbound_secret_store(config);
    let control =
        match i2pcontrol_tunnels::TunnelControlState::for_config(config, manager, outbound_secrets)
        {
            Ok(control) => Arc::new(control),
            Err(error) => {
                return Err(DaemonError::RuntimeSupervisorFailed(format!(
                    "failed to register I2PControl service: {error}"
                )));
            }
        };
    let inspection = Arc::clone(inspection);
    let addressbook = Arc::clone(addressbook);
    let mut spec = service_spec(
        I2PCONTROL_SERVICE_NAME,
        ServiceClassification::Optional,
        move |ctx| {
            let i2pcontrol_config = i2pcontrol_config.clone();
            let inspection = Arc::clone(&inspection);
            let addressbook = Arc::clone(&addressbook);
            let control = Arc::clone(&control);
            let cancellation = ctx.cancellation().clone();
            let children = ctx.children();
            let readiness = ctx.readiness();
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
                // Plan 289 control startup runs before the listener binds so
                // restored tunnels are serving when the control plane
                // answers. Per-definition failures isolate; they never fail
                // the service. (This is the same phase
                // `I2pControlServiceState::run` performs; the composition
                // root inlines it so readiness can be signalled between the
                // bind and the serving phase.)
                let failures = control.startup(&children, &cancellation).await;
                for (name, reason) in failures {
                    tracing::warn!(tunnel = %name, reason = %reason, "control tunnel failed at startup");
                }
                control.spawn_idle_sweeper(&children, &cancellation);
                // Plan 360: readiness after a successful bind, so a ready
                // I2PControl service provably owns a bound loopback socket.
                let token = cancellation.clone();
                let binder = Arc::clone(&state);
                bind_then_serve(
                    I2PCONTROL_SERVICE_NAME,
                    &readiness,
                    binder.bind(address),
                    |(listener, _bound_address)| async move {
                        match state.serve(listener, children, token).await {
                            Ok(()) => i2pr_runtime::ServiceResult::RequestedShutdown,
                            Err(error) => {
                                let detail = i2pr_core::HealthDetail::new(format!(
                                    "I2PControl serve failed: {error}"
                                ))
                                .ok();
                                i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::Internal,
                                    detail,
                                ))
                            }
                        }
                    },
                )
                .await
            })
        },
    );
    // Plan 337: the graph's default order is lexical, which starts
    // `i2pcontrol` before `ssu2`. The control state reconciles onto the
    // shared manager, so it must run after the product has prepared that
    // manager and installed the executable router delivery backend.
    // Declaring the dependency is only valid when SSU2 is actually
    // registered, so it is conditional on the same configuration.
    if config.ssu2.enabled {
        spec = spec.depends_on(ServiceName::new(SSU2_SERVICE_NAME).expect("valid service name"));
    }
    builder.register(spec).map_err(|e| {
        DaemonError::RuntimeSupervisorFailed(format!("failed to register I2PControl service: {e}"))
    })?;
    Ok(())
}
/// Registers the Plan 321 subscription-refresh worker. The manager owns
/// queue state and the narrow fetch capability; the daemon service owns
/// cadence and cancellation. Dropping an in-flight fetch cancels its
/// socket operation and releases the queue's active slot.
fn register_addressbook_refresh_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
) -> Result<(), DaemonError> {
    let worker_name =
        ServiceName::new(ADDRESSBOOK_REFRESH_SERVICE_NAME).expect("valid service name");
    let manager = Arc::clone(addressbook);
    builder
        .register(ServiceSpec::new(
            worker_name,
            ServiceClassification::Optional,
            move |ctx| {
                let manager = Arc::clone(&manager);
                let cancellation = ctx.cancellation().clone();
                let readiness = ctx.readiness();
                Box::pin(async move {
                    if !manager.is_active() {
                        // Plan 360: an inactive subsystem is a real,
                        // named reason this worker cannot run. Failing
                        // loudly beats a service that quietly reports
                        // ready while owning no cadence.
                        let detail = i2pr_core::HealthDetail::new(
                            "address-book refresh service is not active",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
                    // Plan 360: readiness means the cadence loop is running.
                    // It deliberately does not wait for a subscription fetch,
                    // which is remote, bounded, and failure-tolerant.
                    if let Err(failure) =
                        signal_running(ADDRESSBOOK_REFRESH_SERVICE_NAME, &readiness)
                    {
                        return failure;
                    }
                    loop {
                        let hours = manager.refresh_interval_hours().max(1);
                        tokio::select! {
                            _ = cancellation.cancelled() => break,
                            _ = tokio::time::sleep(Duration::from_secs(hours * 3600)) => {
                                let reason = i2pr_addressbook::RefreshReason::IntervalElapsed;
                                let _ = manager.enqueue_current_for_refresh(reason);
                                tokio::select! {
                                    _ = cancellation.cancelled() => {
                                        manager.release_refresh_after_cancel();
                                        break;
                                    },
                                    _ = manager.run_queued_fetches(reason) => {}
                                }
                            }
                            _ = manager.refresh_requested() => {
                                let reason = i2pr_addressbook::RefreshReason::SubscriptionsReplaced;
                                tokio::select! {
                                    _ = cancellation.cancelled() => {
                                        manager.release_refresh_after_cancel();
                                        break;
                                    },
                                    _ = manager.run_queued_fetches(reason) => {}
                                }
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

fn register_news_refresh_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
) -> Result<(), DaemonError> {
    use crate::addressbook_fetch::{BoundedContentFetcher, LoopbackProxyFetcher};

    let fetcher: Arc<dyn BoundedContentFetcher> = Arc::new(LoopbackProxyFetcher {
        host: config.news.proxy_host.to_string(),
        port: config.news.proxy_port,
    });
    let manager = Arc::new(crate::news::NewsManager::new(
        config.news.clone(),
        config.router.data_dir.clone(),
        fetcher,
    ));
    inspection.publish_news_manager(Arc::clone(&manager));
    let interval = config.news.refresh_interval;
    builder
        .register(service_spec(
            SIGNED_NEWS_REFRESH_SERVICE_NAME,
            ServiceClassification::Optional,
            move |ctx| {
                let manager = Arc::clone(&manager);
                let cancellation = ctx.cancellation().clone();
                let readiness = ctx.readiness();
                Box::pin(async move {
                    // Plan 360: readiness means the refresh cadence loop is
                    // running. It deliberately does not wait for the first
                    // remote fetch, whose outcome is non-fatal by design.
                    if let Err(failure) =
                        signal_running(SIGNED_NEWS_REFRESH_SERVICE_NAME, &readiness)
                    {
                        return failure;
                    }
                    let _ = manager.refresh_once().await;
                    loop {
                        tokio::select! {
                            _ = cancellation.cancelled() => break,
                            _ = tokio::time::sleep(interval) => {
                                let _ = manager.refresh_once().await;
                            }
                        }
                    }
                    i2pr_runtime::ServiceResult::RequestedShutdown
                })
            },
        ))
        .map_err(|error| {
            DaemonError::RuntimeSupervisorFailed(format!(
                "failed to register signed-news refresh service: {error}"
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
/// Maximum authenticated router messages waiting for the shared product owner.
const NORMAL_ROUTER_INGRESS_CAPACITY: usize = 128;

struct ClearDualRouterInfoOnDrop(tokio::sync::watch::Sender<Option<Arc<Vec<u8>>>>);

impl Drop for ClearDualRouterInfoOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(None);
    }
}
static NEXT_NTCP2_LINK_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn allocate_ntcp2_link_generation() -> Option<u64> {
    NEXT_NTCP2_LINK_GENERATION
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |current| current.checked_add(1),
        )
        .ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthenticatedRouterTransport {
    Ntcp2,
    Ssu2,
}

/// A bounded, authenticated message handed from a transport owner to the
/// normal router product. The NTCP2 byte lease follows its payload until the
/// product finishes processing it.
#[derive(Debug)]
struct AuthenticatedRouterIngress {
    transport: AuthenticatedRouterTransport,
    inbound: i2pr_runtime::Ssu2InboundI2np,
    _byte_lease: Option<i2pr_core::ResourceLease>,
}

#[derive(Clone)]
struct Ntcp2IngressOwner {
    sender: tokio::sync::mpsc::Sender<AuthenticatedRouterIngress>,
    byte_budget: i2pr_core::ResourceBudget,
    delivery: crate::router_i2np::Ntcp2RouterDeliveryRegistry,
}

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
        outcome: crate::router_i2np::RouterI2npOutcome,
        message: Option<&i2pr_proto::I2npMessage>,
    ) -> Result<(), crate::service_product::ServiceProductError> {
        if let Self::Groups(product) = self {
            product.process_inbound(inbound, outcome, message).await;
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

struct FloodfillEvaluationContext<'a> {
    data_dir: &'a std::path::Path,
    config_path: Option<&'a std::path::Path>,
    controlled_profile: bool,
    controlled_ntcp2_endpoint: Option<std::net::SocketAddr>,
    local_router_info: &'a tokio::sync::watch::Sender<Option<Arc<Vec<u8>>>>,
    inspection: &'a Arc<InspectionHandles>,
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
    context: FloodfillEvaluationContext<'_>,
    cancellation: &i2pr_runtime::CancellationToken,
) {
    let FloodfillEvaluationContext {
        data_dir,
        config_path,
        controlled_profile,
        controlled_ntcp2_endpoint,
        local_router_info,
        inspection,
    } = context;
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
    let dual_router_info = local_router_info.borrow().clone();
    let step = if controlled_profile {
        state.evaluate_controlled_loopback(&readiness, dual_router_info.is_some())
    } else {
        state.evaluate(&readiness)
    };
    match step {
        crate::floodfill::FloodfillServiceStep::Idle => {}
        crate::floodfill::FloodfillServiceStep::Activate => {
            let activation = if controlled_profile {
                match dual_router_info {
                    Some(info) => match controlled_ntcp2_endpoint {
                        Some(endpoint) => {
                            state
                                .apply_controlled_dual_activation(
                                    handle,
                                    bundle,
                                    info.as_ref(),
                                    endpoint,
                                    &readiness,
                                    cancellation,
                                )
                                .await
                        }
                        None => Err(crate::floodfill::NormalActivationError::InvalidConfig),
                    },
                    None => Err(crate::floodfill::NormalActivationError::InvalidConfig),
                }
            } else {
                state
                    .apply_activation(handle, bundle, &readiness, cancellation)
                    .await
            };
            match activation {
                Ok(encoded) => {
                    if controlled_profile {
                        match i2pr_netdb::encode_router_info(&encoded) {
                            Ok(info_b64) => match inspection
                                .publish_local_router_info_b64(info_b64.trim_end_matches('='))
                            {
                                Ok(()) => tracing::info!(
                                    router_info_bytes = encoded.len(),
                                    "controlled floodfill RouterInfo inspection snapshot published"
                                ),
                                Err(error) => tracing::warn!(
                                    reason = %error,
                                    "controlled floodfill RouterInfo inspection publication rejected"
                                ),
                            },
                            Err(error) => tracing::warn!(
                                reason = %error,
                                "controlled floodfill RouterInfo base64 encoding failed"
                            ),
                        }
                        let _ = local_router_info.send(Some(Arc::new(encoded)));
                    }
                    tracing::info!(
                        requested = true,
                        role = "Active",
                        "floodfill qualification activated"
                    );
                }
                Err(error) => tracing::warn!(
                    requested = true,
                    role = "Failed",
                    reason = %error,
                    "normal floodfill activation failed closed"
                ),
            }
        }
        crate::floodfill::FloodfillServiceStep::Withdraw => {
            let dual_transport_available = dual_router_info.is_some();
            match state
                .apply_withdrawal(
                    handle,
                    bundle,
                    crate::floodfill::FloodfillWithdrawalContext {
                        wall_now_ms: now_ms,
                        drain_timeout: NORMAL_FLOODFILL_DRAIN_TIMEOUT,
                        max_record_age_ms: NORMAL_FLOODFILL_MAX_RECORD_AGE_MS,
                        preserve_dual_transport: !controlled_profile || dual_transport_available,
                        controlled_ntcp2_endpoint,
                    },
                    cancellation,
                )
                .await
            {
                Ok(withdrawn) => {
                    if controlled_profile && dual_transport_available && !withdrawn.is_empty() {
                        if let Ok(info_b64) = i2pr_netdb::encode_router_info(&withdrawn)
                            && inspection
                                .publish_local_router_info_b64(info_b64.trim_end_matches('='))
                                .is_err()
                        {
                            tracing::warn!(
                                "withdrawn floodfill RouterInfo inspection publication rejected"
                            );
                        }
                        let _ = local_router_info.send(Some(Arc::new(withdrawn)));
                    }
                    tracing::info!(
                        requested = state.intent(),
                        role = "Disabled",
                        "floodfill advertisement withdrew"
                    );
                }
                Err(error) => tracing::warn!(
                    requested = state.intent(),
                    reason = %error,
                    "normal floodfill withdrawal failed; role stays Draining for retry"
                ),
            }
        }
    }
}

fn ntcp2_service_failure(
    category: i2pr_core::ServiceFailureCategory,
    reason: &'static str,
) -> i2pr_runtime::ServiceResult {
    let detail = i2pr_core::HealthDetail::new(reason).ok();
    i2pr_runtime::ServiceResult::Failed(i2pr_core::ServiceFailure::new(category, detail))
}

async fn forward_authenticated_ntcp2_message(
    sender: &tokio::sync::mpsc::Sender<AuthenticatedRouterIngress>,
    peer: i2pr_transport::PeerId,
    link_id: i2pr_transport::LinkId,
    bytes: Vec<u8>,
    cancellation: &i2pr_runtime::CancellationToken,
    byte_budget: &i2pr_core::ResourceBudget,
) -> bool {
    if bytes.is_empty() || bytes.len() > crate::router_i2np::MAX_ROUTER_I2NP_BYTES {
        return false;
    }
    let Ok(byte_count) = u64::try_from(bytes.len()) else {
        return false;
    };
    let Ok(byte_lease) = byte_budget.try_acquire(i2pr_core::ResourceRequest {
        class: i2pr_core::ResourceClass::BufferedBytes,
        amount: byte_count,
    }) else {
        return false;
    };
    let ingress = AuthenticatedRouterIngress {
        transport: AuthenticatedRouterTransport::Ntcp2,
        inbound: i2pr_runtime::Ssu2InboundI2np {
            link_id,
            peer,
            bytes,
        },
        _byte_lease: Some(byte_lease),
    };
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => false,
        result = sender.send(ingress) => result.is_ok(),
    }
}

fn correlate_ntcp2_lookup_reply(
    peer: i2pr_transport::PeerId,
    link_id: i2pr_transport::LinkId,
    message: Option<&i2pr_proto::I2npMessage>,
    delivery: &crate::router_i2np::Ntcp2RouterDeliveryRegistry,
    now_ms: u64,
) -> bool {
    let Some(message) = message else {
        return false;
    };
    let key = match message.body() {
        i2pr_proto::I2npBody::DatabaseSearchReply(reply)
            if reply.from.as_bytes() == peer.hash().as_bytes() =>
        {
            Some(reply.key)
        }
        _ => None,
    };
    key.is_some_and(|key| delivery.correlate_lookup_response(peer, link_id, key, now_ms))
}

struct Ntcp2ServiceRegistration<'a> {
    config: &'a crate::config::Ntcp2Config,
    ssu2_config: &'a crate::config::Ssu2Config,
    controlled_dual_transport: bool,
    send_initiator_lookup_probe: bool,
    network_id: u16,
    data_dir: &'a std::path::Path,
    bandwidth: Option<i2pr_runtime::BandwidthGovernor>,
    bootstrap: Option<Arc<Mutex<bootstrap::Bootstrap>>>,
    ingress: Ntcp2IngressOwner,
    local_router_info: tokio::sync::watch::Sender<Option<Arc<Vec<u8>>>>,
}

/// Registers the guarded loopback NTCP2 runtime owner.
fn register_ntcp2_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    registration: Ntcp2ServiceRegistration<'_>,
) -> Result<(), DaemonError> {
    let Ntcp2ServiceRegistration {
        config,
        ssu2_config,
        controlled_dual_transport,
        send_initiator_lookup_probe,
        network_id,
        data_dir,
        bandwidth,
        bootstrap,
        ingress,
        local_router_info,
    } = registration;
    use i2pr_runtime::{
        IpPrefixPolicy, Ntcp2RuntimeConfig, Ntcp2RuntimeDeadlines, Ntcp2RuntimeLimits,
        Ntcp2RuntimeService,
    };
    use i2pr_storage::{IdentityStore, Ntcp2TransportIdentityStore, Ssu2TransportIdentityStore};
    use std::net::SocketAddr;
    use zeroize::Zeroizing;

    let config = config.clone();
    let ssu2_config = ssu2_config.clone();
    let router_ingress = ingress.sender.clone();
    let router_ingress_budget = ingress.byte_budget.clone();
    let ntcp2_delivery = ingress.delivery.clone();
    let (outbound_dial_tx, outbound_dial_rx) =
        tokio::sync::mpsc::channel(config.max_active_links.max(1));
    let outbound_dial_rx = Arc::new(tokio::sync::Mutex::new(outbound_dial_rx));
    if bootstrap.is_some() && !ntcp2_delivery.attach_dial_request_sender(outbound_dial_tx) {
        return Err(DaemonError::RuntimeSupervisorFailed(
            "NTCP2 outbound dial request queue could not be attached".to_owned(),
        ));
    }
    let data_dir = data_dir.to_path_buf();
    let limits = Ntcp2RuntimeLimits {
        max_active_links: config.max_active_links,
        max_replay_entries: config.max_replay_entries,
        ..Ntcp2RuntimeLimits::default()
    };
    let runtime_config = Ntcp2RuntimeConfig {
        limits,
        deadlines: Ntcp2RuntimeDeadlines {
            connect: config.connect_timeout,
            handshake: config.handshake_timeout,
            read_idle: config.read_idle_timeout,
            write: config.write_timeout,
            queue_wait: config.queue_wait_timeout,
            drain: config.drain_timeout,
        },
        prefixes: IpPrefixPolicy::new(config.ipv4_prefix, config.ipv6_prefix).map_err(|_| {
            DaemonError::RuntimeSupervisorFailed(
                "normalized NTCP2 admission prefixes are invalid".to_owned(),
            )
        })?,
    };
    let spec = service_spec(
        NTCP2_SERVICE_NAME,
        ServiceClassification::Optional,
        move |ctx| {
            let config = config.clone();
            let ssu2_config = ssu2_config.clone();
            let data_dir = data_dir.clone();
            let bandwidth = bandwidth.clone();
            let router_ingress = router_ingress.clone();
            let router_ingress_budget = router_ingress_budget.clone();
            let ntcp2_delivery = ntcp2_delivery.clone();
            let bootstrap = bootstrap.clone();
            let local_router_info = local_router_info.clone();
            let controlled_dual_transport = controlled_dual_transport;
            let send_initiator_lookup_probe = send_initiator_lookup_probe;
            let ntcp2_send_timeout = config.write_timeout;
            let cancellation = ctx.cancellation().clone();
            let children = ctx.children();
            let readiness = ctx.readiness();
            let outbound_dial_rx = Arc::clone(&outbound_dial_rx);
            Box::pin(async move {
                if !config.enabled
                    || config.qualification_profile
                        != crate::config::Ntcp2QualificationProfile::ControlledLoopback
                    || !config.bind_address.is_loopback()
                    || config.listen_port < 1024
                {
                    return ntcp2_service_failure(
                        i2pr_core::ServiceFailureCategory::InvalidState,
                        "NTCP2 requires the explicit controlled-loopback profile",
                    );
                }
                let runtime = match Ntcp2RuntimeService::new_with_bandwidth(
                    runtime_config,
                    bandwidth.clone(),
                ) {
                    Ok(runtime) => runtime,
                    Err(_) => {
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "NTCP2 runtime or shared bandwidth limits are invalid",
                        );
                    }
                };
                let requested = SocketAddr::new(config.bind_address, config.listen_port);
                let mut listener = match runtime.listen(requested, &children).await {
                    Ok(listener) if listener.local_addr() == requested => listener,
                    Ok(listener) => {
                        listener.shutdown();
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "NTCP2 bound endpoint differs from normalized configuration",
                        );
                    }
                    Err(_) => {
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::Internal,
                            "NTCP2 loopback listener could not bind",
                        );
                    }
                };
                let bundle = match IdentityStore::in_data_dir(&data_dir).load() {
                    Ok(bundle) => bundle,
                    Err(_) => {
                        listener.shutdown();
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "NTCP2 requires the existing persistent router identity",
                        );
                    }
                };
                let router_hash = match bundle.identity().hash() {
                    Ok(hash) => *hash.as_bytes(),
                    Err(_) => {
                        listener.shutdown();
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "NTCP2 router identity hash is unavailable",
                        );
                    }
                };
                let transport = match Ntcp2TransportIdentityStore::in_data_dir(&data_dir)
                    .load_or_generate(router_hash, &mut i2pr_crypto::OsRng)
                {
                    Ok(material) => material,
                    Err(_) => {
                        listener.shutdown();
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "NTCP2 identity-bound transport keys are unavailable",
                        );
                    }
                };
                let router_info_bytes = if controlled_dual_transport {
                    let configured_ssu2 = ssu2_config
                        .bind_ipv4
                        .or(ssu2_config.bind_ipv6)
                        .map(|address| SocketAddr::new(address, ssu2_config.port));
                    let Some(ssu2_endpoint) = configured_ssu2.filter(|endpoint| {
                        endpoint.port() >= 1024
                            && endpoint.ip().is_loopback()
                            && endpoint.is_ipv4() == listener.local_addr().is_ipv4()
                    }) else {
                        listener.shutdown();
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "controlled dual-transport profile has no matching fixed SSU2 endpoint",
                        );
                    };
                    let ssu2_transport = match Ssu2TransportIdentityStore::in_data_dir(&data_dir)
                        .load_or_generate(router_hash, &mut i2pr_crypto::OsRng)
                    {
                        Ok(material) => material,
                        Err(_) => {
                            listener.shutdown();
                            return ntcp2_service_failure(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                "controlled dual-transport SSU2 identity-bound keys are unavailable",
                            );
                        }
                    };
                    crate::router_i2np::generate_controlled_dual_transport_router_info(
                        &bundle,
                        ssu2_endpoint,
                        &ssu2_transport,
                        listener.local_addr(),
                        &transport,
                        network_id,
                    )
                } else {
                    crate::router_i2np::generate_controlled_ntcp2_router_info(
                        &bundle,
                        listener.local_addr().ip(),
                        listener.local_addr().port(),
                        network_id,
                        &transport,
                    )
                };
                let router_info = match router_info_bytes {
                    Ok(info) => Arc::new(info),
                    Err(_) => {
                        listener.shutdown();
                        return ntcp2_service_failure(
                            i2pr_core::ServiceFailureCategory::InvalidState,
                            "NTCP2 signed RouterInfo does not match the bound endpoint",
                        );
                    }
                };
                if controlled_dual_transport
                    && local_router_info.send(Some(Arc::clone(&router_info))).is_err()
                {
                    listener.shutdown();
                    return ntcp2_service_failure(
                        i2pr_core::ServiceFailureCategory::Internal,
                        "controlled dual-transport RouterInfo has no live SSU2 publisher",
                    );
                }
                let _clear_dual_router_info = controlled_dual_transport.then(|| {
                    ClearDualRouterInfoOnDrop(local_router_info.clone())
                });
                let local_router_info_rx = local_router_info.subscribe();
                let static_secret =
                    Arc::new(Zeroizing::new(*transport.transport().key().secret_bytes()));
                let obfuscation_iv = *transport.transport().iv();
                if let Err(result) = signal_running(NTCP2_SERVICE_NAME, &readiness) {
                    listener.shutdown();
                    return result;
                }
                if bootstrap.is_some() || send_initiator_lookup_probe {
                    let outbound_runtime = runtime.clone();
                    let outbound_bootstrap = bootstrap.clone();
                    let outbound_local_info = Arc::clone(&router_info);
                    let mut outbound_local_info_rx = local_router_info_rx.clone();
                    let outbound_static_secret = Arc::clone(&static_secret);
                    let outbound_scope = children.clone();
                    let outbound_cancel = cancellation.clone();
                    let outbound_ingress = router_ingress.clone();
                    let outbound_ingress_budget = router_ingress_budget.clone();
                    let outbound_delivery = ntcp2_delivery.clone();
                    let outbound_send_timeout = ntcp2_send_timeout;
                    let _ = children.spawn(move |task_cancel| async move {
                        let mut cursor = 0usize;
                        let mut health_tick = tokio::time::interval(Duration::from_secs(15));
                        health_tick
                            .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        health_tick.tick().await;
                        loop {
                            let requested_peer = tokio::select! {
                                biased;
                                _ = task_cancel.cancelled() => return Ok(()),
                                peer = async {
                                    outbound_dial_rx.lock().await.recv().await
                                } => peer,
                                _ = health_tick.tick() => None,
                            };
                            let outbound_peers = outbound_bootstrap
                                .as_ref()
                                .and_then(|bootstrap| {
                                    bootstrap
                                        .lock()
                                        .ok()
                                        .map(|owner| owner.validated_router_info_snapshot())
                                })
                                .unwrap_or_default();
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .ok()
                                .and_then(|duration| u64::try_from(duration.as_millis()).ok());
                            let Some(now_ms) = now_ms else {
                                tracing::warn!("controlled NTCP2 dial skipped because wall clock is unavailable");
                                continue;
                            };
                            // Store membership means the record was valid when
                            // admitted. It does not make an old snapshot
                            // permanently eligible: re-run the normal NetDB
                            // signature, identity, and age checks immediately
                            // before using it for a dial. The snapshot is
                            // fetched on every wake, so an updated record for
                            // the same RouterHash supersedes the old one.
                            let outbound_peers =
                                revalidate_ntcp2_peer_snapshot(outbound_peers, now_ms);
                            tracing::info!(
                                validated_peer_records = outbound_peers.len(),
                                "controlled NTCP2 outbound peer snapshot refreshed"
                            );
                            if outbound_peers.is_empty() {
                                continue;
                            }
                            let candidate = if let Some(peer) = requested_peer {
                                outbound_peers.iter().find(|candidate| {
                                    candidate.key().as_bytes() == peer.hash().as_bytes()
                                })
                            } else {
                                let candidate = outbound_peers.get(cursor % outbound_peers.len());
                                cursor = cursor.wrapping_add(1);
                                candidate
                            };
                            let Some(candidate) = candidate else {
                                continue;
                            };
                            if candidate.key().as_bytes() == &router_hash {
                                continue;
                            }
                            let peer = i2pr_transport::PeerId::from_hash(
                                i2pr_proto::Hash::from_bytes(*candidate.key().as_bytes()),
                            );
                            if outbound_delivery.has_outbound_peer_route(peer) {
                                continue;
                            }
                            let static_key =
                                i2pr_crypto::X25519PrivateKey::from_bytes(**outbound_static_secret);
                            let local_info = outbound_local_info_rx
                                .borrow_and_update()
                                .as_ref()
                                .map(|info| info.as_ref().clone())
                                .unwrap_or_else(|| outbound_local_info.as_ref().clone());
                            let Some(raw_id) = allocate_ntcp2_link_generation() else {
                                return Ok(());
                            };
                            let mut link = match outbound_runtime
                                .dial_authenticated_loopback_peer(
                                    &outbound_scope,
                                    candidate.router_info(),
                                    *candidate.key().as_bytes(),
                                    static_key,
                                    &local_info,
                                    network_id as u8,
                                    raw_id,
                                    &outbound_cancel,
                                )
                                .await
                            {
                                Ok(link) => link,
                                Err(error) => {
                                    tracing::info!(
                                        error = ?error,
                                        "controlled NTCP2 outbound peer dial attempt did not authenticate"
                                    );
                                    continue;
                                }
                            };
                            let Some(link_id) = i2pr_transport::LinkId::new(raw_id).ok() else {
                                link.close();
                                continue;
                            };
                            let (send_tx, mut send_rx) = tokio::sync::mpsc::channel(16);
                            if !outbound_delivery.register_outbound(peer, link_id, send_tx) {
                                link.close();
                                continue;
                            }
                            tracing::info!(
                                link_generation = raw_id,
                                "NTCP2 authenticated outbound link promoted"
                            );
                            let link_delivery = outbound_delivery.clone();
                            let link_ingress = outbound_ingress.clone();
                            let link_ingress_budget = outbound_ingress_budget.clone();
                            let link_send_timeout = outbound_send_timeout;
                            let link_scope = outbound_scope.clone();
                            if link_scope
                                .spawn(move |link_cancel| async move {
                                    loop {
                                        tokio::select! {
                                            biased;
                                            _ = link_cancel.cancelled() => break,
                                            message = send_rx.recv() => {
                                                let Some(message) = message else { break; };
                                                if link.send_i2np_message(
                                                    message,
                                                    link_send_timeout,
                                                    &link_cancel,
                                                ).await.is_err() {
                                                    break;
                                                }
                                            }
                                            received = link.recv_i2np_messages(&link_cancel) => {
                                                let Ok(Some(messages)) = received else { break; };
                                                let mut accepted = true;
                                                for bytes in messages {
                                                    if !forward_authenticated_ntcp2_message(
                                                        &link_ingress,
                                                        peer,
                                                        link_id,
                                                        bytes,
                                                        &link_cancel,
                                                        &link_ingress_budget,
                                                    ).await {
                                                        accepted = false;
                                                        break;
                                                    }
                                                }
                                                if !accepted {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    link.close();
                                    link_delivery.remove(peer, link_id);
                                    Ok(())
                                })
                                .is_err()
                            {
                                outbound_delivery.remove(peer, link_id);
                                continue;
                            }
                        }
                    });
                }
                loop {
                    tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => {
                            listener.shutdown();
                            return i2pr_runtime::ServiceResult::RequestedShutdown;
                        }
                        inbound = listener.next() => {
                            let Some(inbound) = inbound else {
                                return i2pr_runtime::ServiceResult::RequestedShutdown;
                            };
                            let runtime = runtime.clone();
                            let router_info = Arc::clone(&router_info);
                            let mut local_router_info_rx = local_router_info_rx.clone();
                            let static_secret = Arc::clone(&static_secret);
                            let task_scope = children.clone();
                            let task_ingress = router_ingress.clone();
                        let task_ingress_budget = router_ingress_budget.clone();
                        let task_delivery = ntcp2_delivery.clone();
                        let task_send_timeout = ntcp2_send_timeout;
                            let _ = children.spawn(move |task_cancel| async move {
                                let current_router_info = local_router_info_rx
                                    .borrow_and_update()
                                    .as_ref()
                                    .cloned()
                                    .unwrap_or(router_info);
                                let responder_identity = i2pr_runtime::Ntcp2ResponderIdentity::new(
                                    &static_secret,
                                    router_hash,
                                    obfuscation_iv,
                                    network_id as u8,
                                    current_router_info.as_ref(),
                                );
                                let result = runtime.authenticate_inbound(
                                    inbound,
                                    responder_identity,
                                    &task_cancel,
                                ).await;
                                let Ok((inbound, handshake)) = result else { return Ok(()); };
                                let peer = i2pr_transport::PeerId::from_hash(
                                    handshake.authenticated.peer().router_hash,
                                );
                                let Some(raw_id) = allocate_ntcp2_link_generation() else {
                                    return Ok(());
                                };
                                let Some(link_id) = i2pr_transport::LinkId::new(raw_id).ok() else {
                                    return Ok(());
                                };
                                let Ok(mut link) = runtime.promote_authenticated_inbound(
                                    &task_scope, inbound, handshake, raw_id,
                                ) else { return Ok(()); };
                                tracing::info!(
                                    link_generation = raw_id,
                                    "NTCP2 authenticated inbound link promoted"
                                );
                            let (send_tx, mut send_rx) = tokio::sync::mpsc::channel(16);
                            if !task_delivery.register(peer, link_id, send_tx) {
                                link.close();
                                return Ok(());
                            }
                                loop {
                                    tokio::select! {
                                        biased;
                                        _ = task_cancel.cancelled() => break,
                                    message = send_rx.recv() => {
                                        let Some(message) = message else { break; };
                                        let _ = link.send_i2np_message(
                                            message,
                                            task_send_timeout,
                                            &task_cancel,
                                        ).await;
                                    }
                                        received = link.recv_i2np_messages(&task_cancel) => {
                                            let Ok(Some(messages)) = received else { break; };
                                            for bytes in messages {
                                                if !forward_authenticated_ntcp2_message(
                                                    &task_ingress,
                                                    peer,
                                                    link_id,
                                                    bytes,
                                                    &task_cancel,
                                                    &task_ingress_budget,
                                                ).await {
                                                    link.close();
                                                    task_delivery.remove(peer, link_id);
                                                    return Ok(());
                                                }
                                                tracing::info!(
                                                    "authenticated NTCP2 I2NP message forwarded to router ingress"
                                                );
                                            }
                                        }
                                    }
                                }
                                link.close();
                            task_delivery.remove(peer, link_id);
                                Ok(())
                            });
                        }
                    }
                }
            })
        },
    )
    .startup_requirement(i2pr_core::StartupRequirement::Optional)
    .depends_on(ServiceName::new(SSU2_SERVICE_NAME).expect("static service name"))
    .depends_on(ServiceName::new(LIFECYCLE_SERVICE_NAME).expect("static service name"));
    builder.register(spec).map_err(|error| {
        DaemonError::RuntimeSupervisorFailed(format!("failed to register NTCP2 service: {error}"))
    })?;
    Ok(())
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
#[allow(clippy::too_many_arguments)]
fn register_ssu2_service(
    builder: &mut i2pr_runtime::ServiceGraphBuilder,
    config: &Config,
    inspection: &Arc<InspectionHandles>,
    addressbook: &Arc<crate::addressbook::AddressBookManager>,
    bootstrap: Option<Arc<Mutex<bootstrap::Bootstrap>>>,
    group_lifecycle: crate::service_lifecycle::ServiceLifecycleController,
    shared_service_manager: Option<Arc<crate::service_tunnels::ServiceTunnelManager>>,
    bandwidth: Option<i2pr_runtime::BandwidthGovernor>,
    router_ingress: tokio::sync::mpsc::Receiver<AuthenticatedRouterIngress>,
    router_ingress_keepalive: tokio::sync::mpsc::Sender<AuthenticatedRouterIngress>,
    ntcp2_delivery: crate::router_i2np::Ntcp2RouterDeliveryRegistry,
    local_router_info: tokio::sync::watch::Sender<Option<Arc<Vec<u8>>>>,
    local_router_info_rx: tokio::sync::watch::Receiver<Option<Arc<Vec<u8>>>>,
) -> Result<(), DaemonError> {
    use crate::router_i2np::{
        Ssu2DaemonService, decode_authenticated_router_i2np, floodfill_body_from_message,
        generate_controlled_identity_with_keys, generate_private_lan_identity_with_keys,
    };
    use crate::transit_owner::controlled_transit_disabled_probe;
    let ssu2_config = config.ssu2.clone();
    let data_dir = config.router.data_dir.clone();
    let router_info_store_config =
        RouterInfoStoreConfig::new(config.netdb.max_records, config.netdb.max_encoded_bytes);
    let service_tunnels = config.service_tunnels.clone();
    // Plan 337: resolved once at composition, since the gate depends only
    // on validated configuration and not on runtime state.
    let service_runtime_active = service_tunnels_active(config);
    let addressbook = Arc::clone(addressbook);
    let floodfill_enabled = config.floodfill.enabled;
    let controlled_floodfill = config.floodfill.qualification_profile
        == crate::config::FloodfillQualificationProfile::ControlledLoopback;
    let reply_token_probe_enabled = config.floodfill.send_reply_token_probe;
    let request_driven_lookup_probe_enabled = config.floodfill.send_initiator_lookup_probe;
    let ntcp2_max_active_links = config.transport.ntcp2.max_active_links;
    let controlled_ntcp2_endpoint = controlled_floodfill.then(|| {
        std::net::SocketAddr::new(
            config.transport.ntcp2.bind_address,
            config.transport.ntcp2.listen_port,
        )
    });
    let floodfill_config_path = config.source_path.clone();
    let inspection = Arc::clone(inspection);
    let router_ingress = Arc::new(tokio::sync::Mutex::new(router_ingress));
    builder
        .register(service_spec(
            SSU2_SERVICE_NAME,
            ServiceClassification::Optional,
            move |ctx| {
                let ssu2_config = ssu2_config.clone();
                let data_dir = data_dir.clone();
                let router_info_store_config = router_info_store_config;
                let inspection = Arc::clone(&inspection);
                let router_ingress = Arc::clone(&router_ingress);
                let _router_ingress_keepalive = router_ingress_keepalive.clone();
                let ntcp2_delivery = ntcp2_delivery.clone();
                let local_router_info = local_router_info.clone();
                let mut local_router_info_rx = local_router_info_rx.clone();
                let floodfill_config_path = floodfill_config_path.clone();
                let controlled_floodfill = controlled_floodfill;
                let reply_token_probe_enabled = reply_token_probe_enabled;
                let request_driven_lookup_probe_enabled = request_driven_lookup_probe_enabled;
                let ntcp2_max_active_links = ntcp2_max_active_links;
                let controlled_ntcp2_endpoint = controlled_ntcp2_endpoint;
                let bandwidth = bandwidth.clone();
                let service_tunnels = service_tunnels.clone();
                let service_runtime_active = service_runtime_active;
                let bootstrap = bootstrap.clone();
                let addressbook = Arc::clone(&addressbook);
                let group_lifecycle = group_lifecycle.clone();
                let shared_service_manager = shared_service_manager.clone();
                let cancellation = ctx.cancellation().clone();
                let children = ctx.children();
                let readiness = ctx.readiness();
                Box::pin(async move {
                    if !ssu2_config.enabled {
                        if readiness.signal_ready().is_err() {
                            return i2pr_runtime::ServiceResult::Failed(
                                i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    i2pr_core::HealthDetail::new(
                                        "router ingress owner readiness signal was rejected",
                                    )
                                    .ok(),
                                ),
                            );
                        }
                        loop {
                            let inbound = tokio::select! {
                                _ = cancellation.cancelled() => {
                                    return i2pr_runtime::ServiceResult::RequestedShutdown;
                                }
                                inbound = async {
                                    router_ingress.lock().await.recv().await
                                } => inbound,
                            };
                            let Some(inbound) = inbound else {
                                return i2pr_runtime::ServiceResult::RequestedShutdown;
                            };
                            if inbound.transport == AuthenticatedRouterTransport::Ntcp2
                                && !ntcp2_delivery.is_current(
                                    inbound.inbound.peer,
                                    inbound.inbound.link_id,
                                )
                            {
                                continue;
                            }
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|duration| {
                                    duration.as_millis().min(u128::from(u64::MAX)) as u64
                                })
                                .unwrap_or(0);
                            let Ok(decoded) = decode_authenticated_router_i2np(
                                &inbound.inbound,
                                now_ms,
                            ) else {
                                tracing::debug!(
                                    "authenticated router ingress rejected by bounded I2NP decoder"
                                );
                                continue;
                            };
                            if inbound.transport == AuthenticatedRouterTransport::Ntcp2
                                && correlate_ntcp2_lookup_reply(
                                    inbound.inbound.peer,
                                    inbound.inbound.link_id,
                                    decoded.message(),
                                    &ntcp2_delivery,
                                    now_ms,
                                )
                            {
                                tracing::info!(
                                    link_generation = inbound.inbound.link_id.value(),
                                    "controlled NTCP2 initiator DatabaseLookup response correlated"
                                );
                            }
                            if let (
                                Some(bootstrap),
                                Some(i2pr_proto::I2npBody::DatabaseStore(store)),
                            ) = (
                                bootstrap.as_ref(),
                                decoded.message().map(i2pr_proto::I2npMessage::body),
                            ) {
                                match bootstrap.lock() {
                                    Ok(mut bootstrap) => match bootstrap
                                        .ingest_authenticated_router_store(store, now_ms)
                                    {
                                        Ok(outcome) => tracing::debug!(
                                            ?outcome,
                                            "authenticated RouterInfo store applied"
                                        ),
                                        Err(error) => tracing::debug!(
                                            reason = %error,
                                            "authenticated unsolicited DatabaseStore refused"
                                        ),
                                    },
                                    Err(_) => tracing::warn!(
                                        "bootstrap NetDB lock poisoned; authenticated RouterInfo store refused"
                                    ),
                                }
                            }
                        }
                    }
                    // Strict profile is enforced twice: once at config
                    // parse time and again here so a future bypass
                    // cannot reach socket ownership.
                    if ssu2_config.advertise
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
                    let router_hash = match bundle.identity().hash() {
                        Ok(hash) => hash,
                        Err(_) => {
                            let detail = i2pr_core::HealthDetail::new(
                                "SSU2 router identity hash unavailable",
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
                    let transport_store = Ssu2TransportIdentityStore::in_data_dir(&data_dir);
                    let transport_identity = match transport_store
                        .load_or_generate(*router_hash.as_bytes(), &mut i2pr_crypto::OsRng)
                    {
                        Ok(identity) => identity,
                        Err(error) => {
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "SSU2 transport identity unavailable: {error}"
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
                    // Seed the runtime with a valid local identity, then replace
                    // its RouterInfo from the OS-reported socket endpoint before
                    // readiness or any owned product receives the handle.
                    let bind_ip = ssu2_config.bind_ipv4.or(ssu2_config.bind_ipv6);
                    let Some(bind_ip) = bind_ip else {
                        let detail =
                            i2pr_core::HealthDetail::new("SSU2 service has no loopback bind").ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    };
                    let initial_host = bind_ip.to_string();
                    let initial_port = if ssu2_config.port == 0 {
                        SSU2_PREBIND_ROUTER_INFO_PORT
                    } else {
                        ssu2_config.port
                    };
                    let identity = match if ssu2_config.qualification_profile
                        == crate::config::Ssu2QualificationProfile::PrivateLan
                    {
                        generate_private_lan_identity_with_keys(
                            &bundle,
                            &initial_host,
                            initial_port,
                            &transport_identity,
                        )
                    } else {
                        generate_controlled_identity_with_keys(
                            &bundle,
                            &initial_host,
                            initial_port,
                            &transport_identity,
                        )
                    } {
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
                    let daemon_service = match Ssu2DaemonService::new_with_bandwidth(
                        &ssu2_config,
                        identity,
                        bandwidth,
                    ) {
                        Ok(service) => service,
                        Err(error) => {
                            tracing::warn!(reason = %error, "SSU2 service construction failed");
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
                            tracing::warn!(reason = %error, "SSU2 service startup failed");
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
                    let bound = handle.local_v4().or_else(|| handle.local_v6());
                    let Some(bound) = bound else {
                        handle.shutdown();
                        let detail = i2pr_core::HealthDetail::new(
                            "SSU2 runtime reported no bound endpoint",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    };
                    if controlled_floodfill
                        && ssu2_config
                            .bind_ipv4
                            .or(ssu2_config.bind_ipv6)
                            .is_none_or(|ip| {
                                std::net::SocketAddr::new(ip, ssu2_config.port) != bound
                            })
                    {
                        tracing::warn!("controlled dual-transport SSU2 endpoint did not match the live bind");
                        handle.shutdown();
                        let detail = i2pr_core::HealthDetail::new(
                            "controlled dual-transport SSU2 bound endpoint differs from configuration",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
                    let signed = match if ssu2_config.qualification_profile
                        == crate::config::Ssu2QualificationProfile::PrivateLan
                    {
                        generate_private_lan_identity_with_keys(
                            &bundle,
                            &bound.ip().to_string(),
                            bound.port(),
                            &transport_identity,
                        )
                    } else {
                        generate_controlled_identity_with_keys(
                            &bundle,
                            &bound.ip().to_string(),
                            bound.port(),
                            &transport_identity,
                        )
                    } {
                        Ok(identity) => identity,
                        Err(error) => {
                            handle.shutdown();
                            let detail = i2pr_core::HealthDetail::new(format!(
                                "SSU2 bound RouterInfo failed: {error}"
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
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
                        .unwrap_or(0);
                    let base_router_info = signed.router_info.clone();
                    if handle
                        .service()
                        .install_local_router_info(signed.router_info, now_ms)
                        .is_err()
                    {
                        handle.shutdown();
                        let detail = i2pr_core::HealthDetail::new(
                            "SSU2 bound RouterInfo did not match the live socket and keys",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
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
                        match crate::floodfill::FloodfillServiceState::new_with_profile(
                            local_hash,
                            true,
                            controlled_floodfill,
                        ) {
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
                    let mut reply_token_probe_sent = HashSet::new();
                    let mut pending_reply_token_probes = HashMap::new();
                    let token = cancellation.clone();
                    // Plan 337: the destination-group product must come
                    // up whenever a service tunnel can exist at all,
                    // including when the only way to create one is
                    // I2PControl. See `service_tunnels_active`.
                    let has_enabled_groups = service_runtime_active;
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
 extra_bootstrap_peers: Vec::new(),
                            options: crate::service_product::ServiceProductOptions::default(),
                            addressbook: addressbook.shared(),
                            // Plan 337: the one shared manager, also held
                            // by the I2PControl control state.
                            shared_manager: shared_service_manager.clone(),
                        };
                        match crate::service_product::ServiceProduct::start_over_existing_daemon(
                            spec,
                            handle,
                            children.clone(),
                            token.clone(),
                            router_infos,
                            router_info_store_config,
                            group_lifecycle.clone(),
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
                    if !owner
                        .handle()
                        .delivery()
                        .attach_ntcp2_registry(ntcp2_delivery.clone())
                    {
                        owner.shutdown().await;
                        let detail = i2pr_core::HealthDetail::new(
                            "NTCP2 router-delivery registry could not be attached",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::Internal,
                                detail,
                            ),
                        );
                    }
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
                    if request_driven_lookup_probe_enabled {
                        tracing::info!("controlled request-driven NTCP2 lookup worker scheduled");
                        spawn_controlled_request_driven_lookup(
                            &children,
                            bootstrap.clone(),
                            local_router_info_rx.clone(),
                            owner.handle().delivery().clone(),
                            ntcp2_delivery.clone(),
                            ntcp2_max_active_links,
                        );
                    }
                    loop {
                        tokio::select! {
                            biased;
                            _ = token.cancelled() => {
                                owner.shutdown().await;
                                return i2pr_runtime::ServiceResult::RequestedShutdown;
                            }
                            changed = local_router_info_rx.changed() => {
                                if changed.is_err() {
                                    owner.shutdown().await;
                                    return i2pr_runtime::ServiceResult::RequestedShutdown;
                                }
                                let info = local_router_info_rx.borrow().clone()
                                    .unwrap_or_else(|| Arc::new(base_router_info.clone()));
                                {
                                    let installed = owner.handle().service().installed_local_router_info();
                                    if installed.as_deref() != Some(info.as_ref()) {
                                        let now_ms = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
                                            .unwrap_or(0);
                                        let installed_result = match controlled_ntcp2_endpoint {
                                            Some(endpoint) => owner.handle()
                                                .install_controlled_dual_router_info(
                                                    info.as_ref().clone(),
                                                    now_ms,
                                                    endpoint,
                                                ),
                                            None => owner.handle().service()
                                                .install_local_router_info(
                                                    info.as_ref().clone(),
                                                    now_ms,
                                                ),
                                        };
                                        if installed_result.is_err() {
                                            owner.shutdown().await;
                                            let detail = i2pr_core::HealthDetail::new(
                                                "shared NTCP2 RouterInfo failed SSU2 installation",
                                            ).ok();
                                            return i2pr_runtime::ServiceResult::Failed(
                                                i2pr_core::ServiceFailure::new(
                                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                                    detail,
                                                ),
                                            );
                                        }
                                    }
                                }
                                match i2pr_netdb::encode_router_info(info.as_ref()) {
                                    Ok(encoded) => match inspection
                                        .publish_local_router_info_b64(encoded.trim_end_matches('='))
                                    {
                                        Ok(()) => tracing::info!(
                                            router_info_bytes = info.len(),
                                            "updated local RouterInfo inspection snapshot published"
                                        ),
                                        Err(error) => tracing::warn!(
                                            reason = %error,
                                            "updated local RouterInfo inspection publication rejected"
                                        ),
                                    },
                                    Err(error) => tracing::warn!(
                                        reason = %error,
                                        "updated local RouterInfo base64 encoding failed"
                                    ),
                                }
                            }
                            inbound = async {
                                tokio::select! {
                                    biased;
                                    inbound = owner.next_inbound() => inbound.map(|inbound| {
                                        AuthenticatedRouterIngress {
                                            transport: AuthenticatedRouterTransport::Ssu2,
                                            inbound,
                                            _byte_lease: None,
                                        }
                                    }),
                                    inbound = async {
                                        router_ingress.lock().await.recv().await
                                    } => inbound,
                                }
                            } => {
                                let Some(inbound) = inbound else {
                                    owner.shutdown().await;
                                    return i2pr_runtime::ServiceResult::RequestedShutdown;
                                };
                                let transport = inbound.transport;
                                let inbound = inbound.inbound;
                                if transport == AuthenticatedRouterTransport::Ntcp2
                                    && !ntcp2_delivery.is_current(inbound.peer, inbound.link_id)
                                {
                                    // The link may have closed after queue admission but
                                    // before this receiver took the event. Never let a
                                    // stale generation mutate router state.
                                    continue;
                                }
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
                                    .unwrap_or(0);
                                let decoded = match decode_authenticated_router_i2np(&inbound, now_ms) {
                                    Ok(decoded) => decoded,
                                    Err(_) => continue,
                                };
                                let outcome = decoded.outcome();
                                let message = decoded.message();
                                if let (
                                    Some(bootstrap),
                                    Some(i2pr_proto::I2npBody::DatabaseStore(store)),
                                ) = (
                                    bootstrap.as_ref(),
                                    message.map(i2pr_proto::I2npMessage::body),
                                ) {
                                    match bootstrap.lock() {
                                        Ok(mut bootstrap) => match bootstrap
                                            .ingest_authenticated_router_store(store, now_ms)
                                        {
                                            Ok(outcome) => tracing::info!(
                                            transport_family = match transport {
                                                AuthenticatedRouterTransport::Ntcp2 => "ntcp2",
                                                AuthenticatedRouterTransport::Ssu2 => "ssu2",
                                            },
                                                inserted = matches!(
                                                    outcome,
                                                    i2pr_netdb::UnsolicitedStoreOutcome::Inserted(_)
                                                ),
                                                "authenticated RouterInfo store applied to bootstrap peer store"
                                            ),
                                            Err(error) => tracing::debug!(
                                                reason = %error,
                                                "authenticated unsolicited DatabaseStore refused"
                                            ),
                                        },
                                        Err(_) => tracing::warn!(
                                            "bootstrap NetDB lock poisoned; authenticated RouterInfo store refused"
                                        ),
                                    }
                                }
                                if transport == AuthenticatedRouterTransport::Ntcp2
                                    && correlate_ntcp2_lookup_reply(
                                        inbound.peer,
                                        inbound.link_id,
                                        message,
                                        &ntcp2_delivery,
                                        now_ms,
                                    )
                                {
                                    tracing::info!(
                                        link_generation = inbound.link_id.value(),
                                        "controlled NTCP2 initiator DatabaseLookup response correlated"
                                    );
                                }
                                reply_token_probe_sent.retain(|(peer, link_id)| {
                                    ntcp2_delivery.is_current(*peer, *link_id)
                                });
                                pending_reply_token_probes.retain(|(peer, link_id), (_, expires_ms)| {
                                    now_ms < *expires_ms
                                        && ntcp2_delivery.is_current(*peer, *link_id)
                                });
                                if transport == AuthenticatedRouterTransport::Ntcp2
                                    && let Some(i2pr_proto::I2npBody::DeliveryStatus(status)) =
                                        message.map(i2pr_proto::I2npMessage::body)
                                    && pending_reply_token_probes
                                        .get(&(inbound.peer, inbound.link_id))
                                        .is_some_and(|(token, expires_ms)| {
                                            crate::router_i2np::reply_token_status_matches(
                                                *token,
                                                status.message_id,
                                                status.timestamp.as_millis(),
                                                now_ms,
                                            )
                                                && now_ms < *expires_ms
                                        })
                                {
                                    pending_reply_token_probes
                                        .remove(&(inbound.peer, inbound.link_id));
                                    tracing::info!(
                                        link_generation = inbound.link_id.value(),
                                        "controlled NTCP2 RouterInfo DeliveryStatus correlation completed"
                                    );
                                }
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
                                    match state.handle_decoded_inbound(
                                        inbound.peer,
                                        inbound.link_id,
                                        outcome,
                                        floodfill_body_from_message(message),
                                        time,
                                    ) {
                                        Ok(crate::floodfill::FloodfillDispatchOutcome::Ignored(_)) => {}
                                        Ok(crate::floodfill::FloodfillDispatchOutcome::Lookup(Ok(()))) => {
                                            tracing::info!(
                                                "authenticated floodfill DatabaseLookup accepted and reply queued"
                                            );
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
                                        Ok(crate::floodfill::FloodfillDispatchOutcome::Store(
                                            i2pr_netdb::FloodfillStoreEffect::Stored {
                                                acknowledgement: Some(_),
                                                ..
                                            },
                                        )) => {
                                            tracing::info!(
                                                transport_family = match transport {
                                                    AuthenticatedRouterTransport::Ntcp2 => "ntcp2",
                                                    AuthenticatedRouterTransport::Ssu2 => "ssu2",
                                                },
                                                "authenticated floodfill DatabaseStore accepted with response owed"
                                            );
                                            state
                                                .drain_effects(
                                                    owner.handle(),
                                                    now_ms,
                                                    NORMAL_FLOODFILL_MAX_RECORD_AGE_MS,
                                                    &token,
                                                )
                                                .await;
                                            served = true;
                                            if reply_token_probe_enabled
                                                && transport == AuthenticatedRouterTransport::Ntcp2
                                                && !reply_token_probe_sent
                                                    .contains(&(inbound.peer, inbound.link_id))
                                                && reply_token_probe_sent.len()
                                                    < ntcp2_max_active_links
                                                && let Some(local_info) =
                                                    local_router_info_rx.borrow().as_ref().cloned()
                                            {
                                                let mut rng = OsRng;
                                                let mut token_bytes = [0_u8; 4];
                                                let mut reply_token = 0;
                                                for _ in 0..4 {
                                                    if rng.try_fill_bytes(&mut token_bytes).is_err() {
                                                        reply_token = 0;
                                                        break;
                                                    }
                                                    reply_token = u32::from_be_bytes(token_bytes);
                                                    if reply_token != 0
                                                        && reply_token != u32::MAX
                                                        && !pending_reply_token_probes.values().any(
                                                            |(pending, _)| *pending == reply_token,
                                                        )
                                                    {
                                                        break;
                                                    }
                                                    reply_token = 0;
                                                }
                                                if reply_token != 0
                                                    && let Ok((_local_peer, bytes)) =
                                                        crate::router_i2np::build_reply_token_router_info_probe(
                                                            local_info.as_ref(),
                                                            reply_token,
                                                            now_ms,
                                                        )
                                                    && let Ok(request) =
                                                        crate::router_i2np::RouterDeliveryRequest::new(
                                                            inbound.peer,
                                                            bytes,
                                                            Duration::from_secs(10),
                                                        )
                                                {
                                                    let delivery = owner
                                                        .handle()
                                                        .delivery()
                                                        .deliver(request, &token);
                                                    if delivery
                                                        == crate::router_i2np::RouterDeliveryOutcome::Accepted
                                                    {
                                                        reply_token_probe_sent.insert((
                                                            inbound.peer,
                                                            inbound.link_id,
                                                        ));
                                                        pending_reply_token_probes.insert(
                                                            (inbound.peer, inbound.link_id),
                                                            (reply_token, now_ms.saturating_add(120_000)),
                                                        );
                                                        tracing::info!(
                                                            link_generation = inbound.link_id.value(),
                                                            "controlled NTCP2 RouterInfo reply-token probe submitted"
                                                        );
                                                    }
                                                }
                                            }
                                        }
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
                                if !served
                                    && let Some(i2pr_proto::I2npBody::DatabaseStore(store)) =
                                        message.map(i2pr_proto::I2npMessage::body)
                                    && let Some(bootstrap) = bootstrap.as_ref()
                                {
                                    match bootstrap.lock() {
                                        Ok(mut bootstrap) => match bootstrap
                                            .ingest_authenticated_router_store(store, now_ms)
                                        {
                                            Ok(outcome) => tracing::debug!(
                                                ?outcome,
                                                "authenticated NTCP2/SSU2 RouterInfo store applied"
                                            ),
                                            Err(error) => tracing::debug!(
                                                reason = %error,
                                                "authenticated unsolicited DatabaseStore refused"
                                            ),
                                        },
                                        Err(_) => tracing::warn!(
                                            "bootstrap NetDB lock poisoned; authenticated RouterInfo store refused"
                                        ),
                                    }
                                }
                                if owner.process_group_inbound(&inbound, outcome, message).await.is_err() {
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
                                    let _ = controlled_transit_disabled_probe(&outcome);
                                }
                            }
                            _ = evaluation.tick() => {
                                if let Some(state) = floodfill.as_mut() {
                                    run_floodfill_evaluation(
                                        state,
                                        owner.handle(),
                                        &bundle,
                                        FloodfillEvaluationContext {
                                            data_dir: &data_dir,
                                            config_path: floodfill_config_path.as_deref(),
                                            controlled_profile: controlled_floodfill,
                                            controlled_ntcp2_endpoint,
                                            local_router_info: &local_router_info,
                                            inspection: &inspection,
                                        },
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
    bootstrap_daemon_inner(config, now_seconds, offline_reseed_path, None)
}

fn bootstrap_daemon_inner(
    config: &Config,
    now_seconds: u64,
    offline_reseed_path: Option<std::path::PathBuf>,
    online_reseed_bundle: Option<Vec<u8>>,
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
    if let Some(bundle) = online_reseed_bundle {
        bootstrap = bootstrap.with_online_reseed_bundle(bundle);
    }
    let policy = bootstrap::BootstrapPolicy::from_config(config);
    let report = bootstrap
        .run(&config.router.data_dir, &builder, policy, now_seconds)
        .map_err(|e| DaemonError::RuntimeBootstrap(e.to_string()))?;
    let shared = Arc::new(Mutex::new(bootstrap));
    Ok((report, shared))
}

async fn fetch_online_reseed(
    config: &Config,
    now_seconds: u64,
    minimum_new_records: usize,
) -> Option<Vec<u8>> {
    if !config.reseed.enabled
        || !config
            .reseed
            .sources
            .iter()
            .any(|source| source.url.is_some())
    {
        return None;
    }
    let trust = match bootstrap::build_trust_set(&config.reseed.sources) {
        Ok(trust) => trust,
        Err(_) => {
            tracing::warn!("online reseed trust store is unavailable");
            return None;
        }
    };
    let limits = ReseedIngestLimits {
        max_su3_bytes: config
            .reseed
            .max_su3_bytes
            .min(bootstrap::MAX_RESEED_BYTES_HARD),
        ..ReseedIngestLimits::default()
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    for source in &config.reseed.sources {
        let Some(url) = source.url.as_deref() else {
            continue;
        };
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let response = crate::addressbook_fetch::fetch_reseed_https(
            url,
            source.tls_root_path.as_deref(),
            config.reseed.max_su3_bytes,
            remaining.min(Duration::from_secs(20)),
        )
        .await;
        let Ok(response) = response else {
            continue;
        };
        let framing = i2pr_su3::parse(
            &response.body,
            i2pr_su3::Su3Limits {
                max_file_bytes: config.reseed.max_su3_bytes,
                max_content_bytes: config.reseed.max_su3_bytes,
                ..i2pr_su3::Su3Limits::default()
            },
        );
        if !matches!(framing, Ok(ref header) if header.signer_id == source.signer_id) {
            continue;
        }
        let mut probe = RouterInfoStore::with_config(RouterInfoStoreConfig::new(
            config.netdb.max_records,
            config.netdb.max_encoded_bytes,
        ));
        let ingestor = ReseedIngestor::with_limits(&trust, limits);
        let validation_time = now_seconds.saturating_mul(1000);
        if let Ok(report) = ingestor.ingest_su3_into(
            &response.body,
            now_seconds,
            ValidationContext::new(Date::from_millis(validation_time)),
            &mut probe,
            None,
        ) && report.verifier.accepted.len() >= minimum_new_records
        {
            return Some(response.body);
        }
    }
    tracing::warn!("all configured HTTPS reseed sources failed bounded verification");
    None
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
    let (mut report, mut bootstrap_handle) = bootstrap_daemon(&config, now_seconds, None)?;
    if matches!(
        report.final_state,
        bootstrap::BootstrapState::Empty | bootstrap::BootstrapState::ReseedRequired
    ) && let Some(bundle) = fetch_online_reseed(
        &config,
        now_seconds,
        config
            .netdb
            .min_router_infos
            .saturating_sub(report.snapshot.record_count),
    )
    .await
    {
        (report, bootstrap_handle) =
            bootstrap_daemon_inner(&config, now_seconds, None, Some(bundle))?;
    }
    tracing::info!(
        state = %report.final_state,
        record_count = report.snapshot.record_count,
        floodfill_advertisers = report.snapshot.floodfill_advertisers,
        reseed_attempts = report.snapshot.reseed_attempts,
        "bootstrap pipeline completed"
    );

    let inspection = Arc::new(InspectionHandles::from_config(&config));
    let service_lifecycle = crate::service_lifecycle::ServiceLifecycleController::new();
    let graph = build_daemon_graph_inner(
        &config,
        &inspection,
        Some(Arc::clone(&bootstrap_handle)),
        service_lifecycle.clone(),
    )?;
    if let Ok(bootstrap) = bootstrap_handle.lock()
        && let Some(local) = bootstrap.local()
    {
        match local.encoded(crate::i2pcontrol_inspection::MAX_LOCAL_ROUTER_INFO_BYTES) {
            Ok(encoded) => match i2pr_netdb::encode_router_info(&encoded) {
                Ok(info_b64) => {
                    if inspection
                        .publish_local_router_info_b64(info_b64.trim_end_matches('='))
                        .is_err()
                    {
                        tracing::warn!("local RouterInfo publication rejected");
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, "local RouterInfo base64 encoding failed")
                }
            },
            Err(error) => tracing::warn!(error = %error, "local RouterInfo encoding failed"),
        }
    }
    // Publish the local router hash for the Plan 288 inspection plane.
    // The hash is public RouterInfo material; no secret crosses into the
    // control plane. When bootstrap built no local RouterInfo the row
    // stays publish-gated and `router.hash` fails explicitly.
    if let Ok(bootstrap) = bootstrap_handle.lock()
        && let Some(hash) = bootstrap.local_hash()
    {
        match i2pr_netdb::encode(hash.as_bytes()) {
            Ok(hash_b64) => {
                if inspection
                    .publish_router_hash(hash_b64.trim_end_matches('='))
                    .is_err()
                {
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
    let mut supervisor_run = Box::pin(supervisor.run());
    let report = 'running: loop {
        tokio::select! {
            result = &mut supervisor_run => {
                break 'running result.map_err(|e| {
                    DaemonError::RuntimeSupervisorFailed(describe_supervisor_error(&e))
                })?;
            }
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok()
                    && service_lifecycle.status()
                        == crate::service_lifecycle::LifecycleStatus::Active
                {
                    service_lifecycle.request_graceful();
                    tokio::select! {
                        result = &mut supervisor_run => {
                            break 'running result.map_err(|e| {
                                DaemonError::RuntimeSupervisorFailed(describe_supervisor_error(&e))
                            })?;
                        }
                        _ = tokio::signal::ctrl_c() => {
                            service_lifecycle.request_hard();
                            handle.shutdown(i2pr_runtime::ShutdownReason::Signal);
                        }
                        _ = service_lifecycle.wait_for_terminal_status() => {
                            handle.shutdown(i2pr_runtime::ShutdownReason::Requested);
                        }
                    }
                } else {
                    service_lifecycle.request_hard();
                    handle.shutdown(i2pr_runtime::ShutdownReason::Requested);
                }
            }
        }
    };

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

    #[path = "support/reseed_fixture.rs"]
    mod reseed_fixture;

    use super::*;
    use crate::cli::{CheckConfigArgs, Command, IdentityArgs, IdentityCommand, RunArgs};
    use crate::error::ExitCode;

    #[test]
    fn ntcp2_peer_snapshot_revalidation_drops_stale_records() {
        let now_ms = 3 * 24 * 60 * 60 * 1000;
        let make_candidate = |seed: u8, published_ms: u64| {
            let bundle = i2pr_crypto::RouterIdentityBundle::from_private_bytes(
                [seed; 32],
                [seed.wrapping_add(1); 32],
                &mut OsRng,
            )
            .expect("router identity");
            let info = bundle
                .sign_router_info(
                    Date::from_millis(published_ms),
                    Vec::new(),
                    Vec::new(),
                    i2pr_proto::Mapping::empty(),
                )
                .expect("signed RouterInfo");
            i2pr_netdb::ValidatedRouterInfo::from_router_info(
                info,
                None,
                ValidationContext::new(Date::from_millis(published_ms)),
            )
            .expect("freshly admitted RouterInfo")
        };
        let stale = make_candidate(0x31, now_ms - 25 * 60 * 60 * 1000);
        let current = make_candidate(0x41, now_ms - 60 * 1000);

        let eligible = revalidate_ntcp2_peer_snapshot(vec![stale, current.clone()], now_ms);

        assert_eq!(eligible, [current]);
    }

    #[test]
    fn ntcp2_lookup_response_requires_authenticated_source_and_pending_key() {
        let registry = crate::router_i2np::Ntcp2RouterDeliveryRegistry::default();
        let peer = i2pr_transport::PeerId::from_hash(i2pr_proto::Hash::from_bytes([0x72; 32]));
        let other = i2pr_transport::PeerId::from_hash(i2pr_proto::Hash::from_bytes([0x73; 32]));
        let link_id = i2pr_transport::LinkId::new(72).expect("link id");
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        assert!(registry.register(peer, link_id, sender));
        let key = i2pr_proto::Hash::from_bytes([0x74; 32]);
        let now_ms = 1_800_000_000_000;
        assert!(registry.register_lookup(peer, link_id, key, now_ms, now_ms + 10_000, 1,));
        let make_reply = |from| {
            i2pr_proto::I2npMessage::new_short_transport(
                19,
                (now_ms / 1000 + 60) as u32,
                i2pr_proto::I2npBody::DatabaseSearchReply(i2pr_proto::DatabaseSearchReplyMessage {
                    key,
                    peer_hashes: Vec::new(),
                    from,
                }),
            )
            .expect("DatabaseSearchReply")
        };
        assert!(!correlate_ntcp2_lookup_reply(
            peer,
            link_id,
            Some(&make_reply(other.hash())),
            &registry,
            now_ms + 1,
        ));
        assert!(!correlate_ntcp2_lookup_reply(
            peer,
            link_id,
            Some(&make_reply(i2pr_proto::Hash::from_bytes([0x75; 32]))),
            &registry,
            now_ms + 2,
        ));
        assert!(correlate_ntcp2_lookup_reply(
            peer,
            link_id,
            Some(&make_reply(peer.hash())),
            &registry,
            now_ms + 3,
        ));
    }

    #[tokio::test]
    async fn ntcp2_ingress_preserves_authenticated_peer_link_and_fails_closed() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let peer = i2pr_transport::PeerId::from_hash(i2pr_proto::Hash::from_bytes([7; 32]));
        let link_id = i2pr_transport::LinkId::new(19).expect("valid link id");
        let cancellation = i2pr_runtime::CancellationToken::new();
        let byte_budget = i2pr_core::ResourceBudget::new([i2pr_core::ResourceLimit {
            class: i2pr_core::ResourceClass::BufferedBytes,
            maximum: 64,
        }])
        .expect("valid test byte budget");

        assert!(
            forward_authenticated_ntcp2_message(
                &sender,
                peer,
                link_id,
                vec![1, 2, 3],
                &cancellation,
                &byte_budget,
            )
            .await
        );
        let ingress = receiver.try_recv().expect("queued authenticated message");
        assert_eq!(ingress.transport, AuthenticatedRouterTransport::Ntcp2);
        assert_eq!(ingress.inbound.peer, peer);
        assert_eq!(ingress.inbound.link_id, link_id);
        assert_eq!(ingress.inbound.bytes, [1, 2, 3]);
        assert_eq!(
            byte_budget
                .usage(i2pr_core::ResourceClass::BufferedBytes)
                .expect("byte budget usage")
                .used,
            3,
            "the lease must follow the message into the product queue"
        );
        drop(ingress);
        assert_eq!(
            byte_budget
                .usage(i2pr_core::ResourceClass::BufferedBytes)
                .expect("released byte budget usage")
                .used,
            0,
            "dropping a processed message releases its byte lease"
        );

        drop(receiver);
        assert!(
            !forward_authenticated_ntcp2_message(
                &sender,
                peer,
                link_id,
                vec![4],
                &cancellation,
                &byte_budget,
            )
            .await
        );

        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        let cancelled = i2pr_runtime::CancellationToken::new();
        let _ = cancelled.cancel(i2pr_core::CancellationReason::OperatorRequest);
        assert!(
            !forward_authenticated_ntcp2_message(
                &sender,
                peer,
                link_id,
                vec![5],
                &cancelled,
                &byte_budget,
            )
            .await
        );
    }

    #[tokio::test]
    async fn online_reseed_does_not_fetch_without_operator_consent() {
        let directory = tempfile::tempdir().expect("temp directory");
        let config_text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[reseed]\nenabled = false\n[[reseed.sources]]\nurl = \"https://one.example/i2pseeds.su3?netid=2\"\nsigner_id = \"one\"\ncertificate_path = \"missing-one.der\"\n[[reseed.sources]]\nurl = \"https://two.example/i2pseeds.su3?netid=2\"\nsigner_id = \"two\"\ncertificate_path = \"missing-two.der\"\n",
            directory.path().join("state").to_string_lossy()
        );
        let config = Config::parse(&config_text).expect("valid disabled reseed config");
        assert!(
            fetch_online_reseed(&config, 1_800_000_000, 1)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn authorized_https_reseed_bootstraps_and_reuses_cold_start_cache() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let directory = tempfile::tempdir().expect("temp directory");
        let data_dir = directory.path().join("state");
        IdentityStore::prepare_directory(&data_dir).expect("prepare identity directory");
        let identity =
            i2pr_crypto::RouterIdentityBundle::generate(&mut OsRng).expect("fresh local identity");
        IdentityStore::in_data_dir(&data_dir)
            .save_new(&identity)
            .expect("persist local identity");

        let now_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs();
        let (signed_bundle, signer_certificate) =
            reseed_fixture::signed_reseed_fixture(now_seconds, "trusted", 0x432_104);
        let (_, second_signer_certificate) =
            reseed_fixture::signed_reseed_fixture(now_seconds, "spare", 0x432_204);

        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("local HTTPS certificate");
        let server_certificate = certified.cert.der().clone();
        let server_key = rustls_pki_types::PrivateKeyDer::Pkcs8(
            rustls_pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()),
        );
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![server_certificate.clone()], server_key)
            .expect("local HTTPS server config");
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind local HTTPS fixture");
        let port = listener.local_addr().expect("fixture address").port();
        let tls_root = directory.path().join("https-root.der");
        std::fs::write(&tls_root, server_certificate.as_ref()).expect("persist HTTPS root");
        let signer_path = directory.path().join("signer.der");
        let second_signer_path = directory.path().join("second-signer.der");
        std::fs::write(&signer_path, signer_certificate).expect("persist signer");
        std::fs::write(&second_signer_path, second_signer_certificate)
            .expect("persist second signer");

        let response_body = signed_bundle.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept HTTPS client");
            let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config));
            let mut stream = acceptor.accept(stream).await.expect("accept TLS");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 512];
            while !request.ends_with(b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.expect("read request");
                assert_ne!(count, 0, "request ended before headers");
                request.extend_from_slice(&buffer[..count]);
                assert!(request.len() <= 4096, "bounded fixture request");
            }
            assert!(request.starts_with(b"GET /i2pseeds.su3?netid=2 HTTP/1.1\r\nHost: localhost:"));
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            );
            stream
                .write_all(headers.as_bytes())
                .await
                .expect("write response headers");
            stream
                .write_all(&response_body)
                .await
                .expect("write signed response");
            stream.shutdown().await.expect("close HTTPS fixture");
        });

        let config_text = format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n[netdb]\nmin_router_infos = 1\nmin_floodfill_advertisers = 0\n[reseed]\nenabled = true\nmax_su3_bytes = 1048576\n[[reseed.sources]]\nurl = \"https://localhost:{port}/i2pseeds.su3?netid=2\"\nsigner_id = \"trusted\"\ncertificate_path = {:?}\ntls_root_path = {:?}\n[[reseed.sources]]\nurl = \"https://two.example/i2pseeds.su3?netid=2\"\nsigner_id = \"spare\"\ncertificate_path = {:?}\n",
            data_dir.to_string_lossy(),
            signer_path.to_string_lossy(),
            tls_root.to_string_lossy(),
            second_signer_path.to_string_lossy(),
        );
        let config = Config::parse(&config_text).expect("authorized two-source config");
        let fetched = fetch_online_reseed(&config, now_seconds, 1)
            .await
            .expect("valid signed bundle fetched over trusted HTTPS");
        assert_eq!(fetched, signed_bundle);

        let (report, bootstrap) = bootstrap_daemon_inner(&config, now_seconds, None, Some(fetched))
            .expect("cold-start bootstrap from online bundle");
        assert_eq!(
            report.final_state,
            bootstrap::BootstrapState::ReadyForNetworkIntegration
        );
        assert_eq!(report.snapshot.record_count, 1);
        let cache_directory = data_dir.join("netdb").join("routers");
        assert!(cache_directory.is_dir(), "validated cache persisted");
        drop(bootstrap);

        let (restarted, _) =
            bootstrap_daemon(&config, now_seconds, None).expect("restart from persisted cache");
        assert_eq!(
            restarted.final_state,
            bootstrap::BootstrapState::ReadyForNetworkIntegration
        );
        assert_eq!(restarted.snapshot.record_count, 1);
    }

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
                field: "transport.ntcp2.qualification_profile",
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
        let error = build_daemon_graph_inner(
            &config,
            &inspection,
            Some(bootstrap),
            crate::service_lifecycle::ServiceLifecycleController::new(),
        )
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
        let graph = build_daemon_graph_inner(
            &config,
            &inspection,
            Some(bootstrap),
            crate::service_lifecycle::ServiceLifecycleController::new(),
        )
        .expect("normal graph accepts enabled groups with their production provider");
        assert!(
            graph
                .startup_order()
                .iter()
                .any(|service| service.as_str() == "ssu2-router")
        );
    }

    /// Plan 360 scope item 2: a startup failure must name the service, the
    /// deadline, and what the service was awaiting — for every completion
    /// variant, including the readiness timeout that produced the original
    /// defect.
    #[test]
    fn startup_diagnostic_names_service_deadline_and_expectation() {
        let service = ServiceName::new("sam-bridge").expect("valid service name");
        for completion in [
            i2pr_core::ServiceCompletion::ReadinessTimeout,
            i2pr_core::ServiceCompletion::StartupTimeout,
        ] {
            let rendered = describe_startup_failure(&service, &completion);
            assert!(
                rendered.contains("sam-bridge"),
                "must name the service: {rendered}"
            );
            assert!(
                rendered.contains("readiness deadline"),
                "must name the deadline: {rendered}"
            );
            assert!(
                rendered.contains("startup deadline"),
                "must name the startup deadline: {rendered}"
            );
            assert!(
                rendered.contains(readiness_expectation("sam-bridge")),
                "must name what the service awaited: {rendered}"
            );
        }
    }

    /// A readiness timeout must not degrade back to the bare variant name it
    /// replaced; the diagnostic has to explain itself.
    #[test]
    fn readiness_timeout_diagnostic_explains_the_missing_signal() {
        let service = ServiceName::new(LIFECYCLE_SERVICE_NAME).expect("valid service name");
        let rendered =
            describe_startup_failure(&service, &i2pr_core::ServiceCompletion::ReadinessTimeout);
        assert!(
            rendered.contains("never reported initial readiness"),
            "must explain that no readiness signal arrived: {rendered}"
        );
        assert!(
            !rendered.ends_with("ReadinessTimeout"),
            "must not reduce to the bare variant: {rendered}"
        );
    }

    /// A typed service failure must carry its bounded detail through.
    #[test]
    fn failed_service_detail_reaches_the_startup_diagnostic() {
        let service = ServiceName::new(SAM_SERVICE_NAME).expect("valid service name");
        let detail = i2pr_core::HealthDetail::new("sam-bridge bind failed: address in use")
            .expect("bounded detail");
        let completion = i2pr_core::ServiceCompletion::Failed(i2pr_core::ServiceFailure::new(
            i2pr_core::ServiceFailureCategory::InvalidState,
            Some(detail),
        ));
        let rendered = describe_startup_failure(&service, &completion);
        assert!(
            rendered.contains("address in use"),
            "must carry the failure reason: {rendered}"
        );
    }

    /// Every registered service must be able to describe itself, so the
    /// contract table and the graph cannot drift apart.
    #[test]
    fn readiness_expectation_covers_every_supervised_service() {
        for service in [
            LIFECYCLE_SERVICE_NAME,
            NETDB_BOOTSTRAP_SERVICE_NAME,
            SAM_SERVICE_NAME,
            I2CP_SERVICE_NAME,
            I2PCONTROL_SERVICE_NAME,
            CONSOLE_SERVICE_NAME,
            ADDRESSBOOK_REFRESH_SERVICE_NAME,
            SIGNED_NEWS_REFRESH_SERVICE_NAME,
            SSU2_SERVICE_NAME,
        ] {
            let expectation = readiness_expectation(service);
            assert_ne!(
                expectation, "supervised service running",
                "{service} has no specific readiness expectation"
            );
            assert!(
                expectation.len() <= i2pr_core::MAX_HEALTH_DETAIL_BYTES,
                "{service} expectation exceeds the bounded health-detail limit"
            );
        }
    }
}
