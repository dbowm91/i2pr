//! Owned service startup, health, restart, and shutdown orchestration.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use i2pr_core::{
    CancellationReason, DegradationCode, FailureCategory, HealthDetail, HealthState,
    LifecycleState, ServiceClassification, ServiceCompletion, ServiceFailure,
    ServiceFailureCategory, ServiceName, ShutdownReason,
};
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::cancel::CancellationToken;
use crate::context::{
    ChildFailurePolicy, ChildScope, HealthReceiver, Readiness, RuntimeClock, ServiceContext,
    SharedHealth,
};
use crate::graph::{
    MAX_SERVICE_TIMEOUT, RestartExhaustion, ServiceGraph, ServiceResult, ServiceSpec,
};
use crate::observability::{
    RouterLifecycle, RuntimeSnapshot, SimulationSnapshot, SupervisorSnapshot, TaskCounters, event,
    service_event, shutdown_event,
};

/// The maximum router-wide graceful shutdown deadline.
pub const MAX_SHUTDOWN_DEADLINE: Duration = MAX_SERVICE_TIMEOUT;

/// Final disposition of owned service tasks during shutdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    /// Every owned task completed within the graceful deadline.
    Graceful,
    /// At least one task had to be aborted, but all joins completed.
    PartiallyForced,
    /// A runtime join invariant failed while cleaning up.
    FailedCleanup,
}

/// Bounded evidence returned after a supervisor stops.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    outcome: ShutdownOutcome,
    forced_services: Vec<ServiceName>,
    completions: BTreeMap<ServiceName, ServiceCompletion>,
    joined_tasks: usize,
    remaining_tasks: usize,
}

impl ShutdownReport {
    /// Returns the final shutdown disposition.
    pub const fn outcome(&self) -> ShutdownOutcome {
        self.outcome
    }

    /// Returns service identifiers whose manager tasks were forcibly aborted.
    pub fn forced_services(&self) -> &[ServiceName] {
        &self.forced_services
    }

    /// Returns the final completion for a service.
    pub fn completion(&self, service: &ServiceName) -> Option<&ServiceCompletion> {
        self.completions.get(service)
    }

    /// Returns all final completions in deterministic service order.
    pub fn completions(&self) -> &BTreeMap<ServiceName, ServiceCompletion> {
        &self.completions
    }

    /// Number of manager tasks joined by the supervisor.
    pub const fn joined_tasks(&self) -> usize {
        self.joined_tasks
    }

    /// Number of owned tasks remaining after shutdown. This must be zero.
    pub const fn remaining_tasks(&self) -> usize {
        self.remaining_tasks
    }

    /// Whether all service managers exited without forced abort.
    pub const fn was_graceful(&self) -> bool {
        matches!(self.outcome, ShutdownOutcome::Graceful)
    }
}

/// Configuration errors for a supervisor's global shutdown policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorConfigError {
    /// The shutdown deadline was zero or exceeded the global maximum.
    InvalidShutdownDeadline { maximum: Duration },
}

impl std::fmt::Display for SupervisorConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::InvalidShutdownDeadline { maximum } = self;
        write!(
            formatter,
            "shutdown deadline must be between 1ns and {maximum:?}"
        )
    }
}

impl std::error::Error for SupervisorConfigError {}

/// Failures returned by supervisor startup or coordinated service execution.
#[derive(Debug)]
pub enum SupervisorError {
    /// A service failed before the complete graph became ready.
    StartupFailed {
        service: ServiceName,
        completion: ServiceCompletion,
        report: ShutdownReport,
    },
    /// An essential service failed after startup.
    EssentialServiceFailed {
        service: ServiceName,
        completion: ServiceCompletion,
        report: ShutdownReport,
    },
    /// A restartable service exhausted its explicit budget with shutdown policy.
    RestartBudgetExhausted {
        service: ServiceName,
        report: ShutdownReport,
    },
}

impl std::fmt::Display for SupervisorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StartupFailed {
                service,
                completion,
                ..
            } => {
                write!(
                    formatter,
                    "service {service} failed during startup: {completion:?}"
                )
            }
            Self::EssentialServiceFailed {
                service,
                completion,
                ..
            } => {
                write!(
                    formatter,
                    "essential service {service} failed: {completion:?}"
                )
            }
            Self::RestartBudgetExhausted { service, .. } => {
                write!(formatter, "restart budget exhausted for service {service}")
            }
        }
    }
}

impl std::error::Error for SupervisorError {}

/// A handle that can request one idempotent supervisor shutdown.
#[derive(Clone, Debug)]
pub struct SupervisorHandle {
    cancellation: CancellationToken,
    state: Arc<TaskCounters>,
    health: Arc<BTreeMap<ServiceName, Arc<SharedHealth>>>,
    clock: Arc<RuntimeClock>,
}

impl SupervisorHandle {
    /// Requests shutdown with a bounded semantic reason.
    pub fn shutdown(&self, reason: ShutdownReason) -> bool {
        self.cancellation.cancel(match reason {
            ShutdownReason::Requested | ShutdownReason::Signal => {
                CancellationReason::OperatorRequest
            }
            ShutdownReason::FatalFailure => CancellationReason::EssentialServiceFailure,
            ShutdownReason::Configuration => CancellationReason::StartupFailure,
            ShutdownReason::Test => CancellationReason::TestHarnessTeardown,
        })
    }

    /// Returns whether a shutdown request has been recorded.
    pub fn is_shutdown_requested(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Returns a bounded, redacted supervisor snapshot without awaiting.
    pub fn snapshot(&self) -> SupervisorSnapshot {
        self.state.snapshot(&self.health, &self.clock)
    }
}

#[derive(Debug)]
struct ManagerOutput {
    name: ServiceName,
    completion: ServiceCompletion,
}

#[derive(Debug)]
struct ActiveManager {
    cancellation: CancellationToken,
    graceful_period: Duration,
}

#[derive(Debug)]
struct AttemptOutput {
    completion: ServiceCompletion,
    sustained_ready: bool,
}

enum ReadinessPhase {
    Ready,
    Closed,
    Completed(Result<ServiceResult, Box<dyn std::any::Any + Send>>),
}

/// Concrete Tokio-backed owner of every registered service manager task.
pub struct Supervisor {
    graph: ServiceGraph,
    root: CancellationToken,
    shutdown_deadline: Duration,
    clock: Arc<RuntimeClock>,
    health: BTreeMap<ServiceName, Arc<SharedHealth>>,
    state: Arc<TaskCounters>,
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Supervisor")
            .field("graph", &self.graph)
            .field("shutdown_deadline", &self.shutdown_deadline)
            .field("health_services", &self.health.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Supervisor {
    /// Validates global shutdown policy and prepares bounded health snapshots.
    pub fn new(
        graph: ServiceGraph,
        shutdown_deadline: Duration,
    ) -> Result<Self, SupervisorConfigError> {
        if shutdown_deadline.is_zero() || shutdown_deadline > MAX_SHUTDOWN_DEADLINE {
            return Err(SupervisorConfigError::InvalidShutdownDeadline {
                maximum: MAX_SHUTDOWN_DEADLINE,
            });
        }
        let root = CancellationToken::new();
        let clock = Arc::new(RuntimeClock::new());
        let state = TaskCounters::new();
        let health: BTreeMap<ServiceName, Arc<SharedHealth>> = graph
            .services()
            .iter()
            .map(|(name, service)| {
                (
                    name.clone(),
                    SharedHealth::new(
                        name.clone(),
                        service.classification(),
                        service.description_text(),
                        Arc::clone(&clock),
                    ),
                )
            })
            .collect();
        for (name, service) in &health {
            service_event(
                name,
                service
                    .snapshot()
                    .classification()
                    .expect("service classification"),
                LifecycleState::Registered,
                0,
                None,
                event::SERVICE_REGISTERED,
            );
        }
        Ok(Self {
            graph,
            root,
            shutdown_deadline,
            clock,
            health,
            state,
        })
    }

    /// Returns a cloneable shutdown handle.
    pub fn handle(&self) -> SupervisorHandle {
        SupervisorHandle {
            cancellation: self.root.clone(),
            state: Arc::clone(&self.state),
            health: Arc::new(self.health.clone()),
            clock: Arc::clone(&self.clock),
        }
    }

    /// Returns a bounded, redacted supervisor snapshot without awaiting.
    pub fn snapshot(&self) -> SupervisorSnapshot {
        self.state.snapshot(&self.health, &self.clock)
    }

    /// Assembles a bounded aggregate snapshot with caller-owned channels,
    /// resources, and optional deterministic simulation counters.
    pub fn runtime_snapshot(
        &self,
        channels: Vec<crate::ChannelSnapshot>,
        resources: Vec<i2pr_core::ResourceUsage>,
        simulation: SimulationSnapshot,
    ) -> Result<RuntimeSnapshot, crate::SnapshotError> {
        RuntimeSnapshot::try_new(self.snapshot(), channels, resources, simulation)
    }

    /// Subscribes to one service's bounded latest-state health snapshot.
    pub fn health(&self, service: &ServiceName) -> Option<HealthReceiver> {
        self.health.get(service).map(SharedHealth::receiver)
    }

    /// Starts the graph in deterministic dependency order and owns all managers
    /// until graceful completion or forced abort.
    pub async fn run(self) -> Result<ShutdownReport, SupervisorError> {
        self.state.set_lifecycle(RouterLifecycle::Starting);
        let mut tasks = JoinSet::new();
        let mut active = BTreeMap::<ServiceName, ActiveManager>::new();
        let mut completions = BTreeMap::new();
        let mut startup_receivers = BTreeMap::<ServiceName, watch::Receiver<bool>>::new();

        for name in self.graph.startup_order() {
            let spec = self.graph.service(name);
            self.set_health(
                name,
                LifecycleState::WaitingForDependencies,
                HealthState::Starting,
                0,
                None,
                None,
            );
            let (manager, receiver) = self.spawn_manager(spec, &mut tasks);
            active.insert(name.clone(), manager);
            startup_receivers.insert(name.clone(), receiver);

            let receiver = startup_receivers
                .get_mut(name)
                .expect("startup receiver was inserted")
                .clone();
            match self
                .wait_for_initial_ready(
                    name,
                    spec.startup_timeout(),
                    receiver,
                    &mut tasks,
                    &mut active,
                    &mut completions,
                )
                .await
            {
                Ok(()) => {}
                Err(completion) => {
                    let report = self
                        .shutdown(
                            &mut tasks,
                            &mut active,
                            &mut completions,
                            CancellationReason::StartupFailure,
                        )
                        .await;
                    self.state.set_lifecycle(RouterLifecycle::Failed);
                    return Err(SupervisorError::StartupFailed {
                        service: name.clone(),
                        completion,
                        report,
                    });
                }
            }
        }

        self.state.set_lifecycle(RouterLifecycle::Ready);

        loop {
            if active.is_empty() {
                let report = self
                    .shutdown(
                        &mut tasks,
                        &mut active,
                        &mut completions,
                        CancellationReason::OperatorRequest,
                    )
                    .await;
                return Ok(report);
            }
            tokio::select! {
                _ = self.root.cancelled() => {
                    return Ok(self.shutdown(
                        &mut tasks,
                        &mut active,
                        &mut completions,
                        self.root.reason().unwrap_or(CancellationReason::OperatorRequest),
                    ).await);
                }
                joined = tasks.join_next() => {
                    self.state.service_finished();
                    let Some(joined) = joined else {
                        let report = self.shutdown(
                            &mut tasks,
                            &mut active,
                            &mut completions,
                            CancellationReason::OperatorRequest,
                        ).await;
                        return Ok(report);
                    };
                    let output = match joined {
                        Ok(output) => output,
                        Err(_) => {
                            let name = active.keys().next().cloned().unwrap_or_else(|| {
                                ServiceName::new("unknown").expect("static fallback name")
                            });
                            ManagerOutput {
                                name,
                                completion: ServiceCompletion::TaskJoinFailure,
                            }
                        }
                    };
                    let Some(manager) = active.remove(&output.name) else {
                        continue;
                    };
                    let classification = self.graph.service(&output.name).classification();
                    let failure = output.completion.is_failure();
                    if !failure || self.root.is_cancelled() {
                        self.record_completion(&output.name, &output.completion, &mut completions);
                        continue;
                    }
                    match classification {
                        ServiceClassification::Essential => {
                            self.set_health(
                                &output.name,
                                LifecycleState::Failed,
                                HealthState::Failed,
                                0,
                                output.completion.category(),
                                None,
                            );
                            let _ = self.root.cancel(CancellationReason::EssentialServiceFailure);
                            let report = self.shutdown(
                                &mut tasks,
                                &mut active,
                                &mut completions,
                                CancellationReason::EssentialServiceFailure,
                            ).await;
                            self.state.set_lifecycle(RouterLifecycle::Failed);
                            completions.insert(output.name.clone(), output.completion.clone());
                            return Err(SupervisorError::EssentialServiceFailed {
                                service: output.name,
                                completion: output.completion,
                                report,
                            });
                        }
                        ServiceClassification::Restartable => {
                            let policy = self.graph.service(&output.name)
                                .restart_config()
                                .expect("graph validates restartable policy");
                            if matches!(policy.exhaustion(), RestartExhaustion::Degrade)
                                && matches!(output.completion, ServiceCompletion::RestartBudgetExhausted)
                            {
                                self.set_health(
                                    &output.name,
                                    LifecycleState::Degraded,
                                    HealthState::Degraded(DegradationCode::LocalPolicy),
                                    0,
                                    output.completion.category(),
                                    None,
                                );
                                completions.insert(output.name, output.completion);
                            } else if matches!(output.completion, ServiceCompletion::RestartBudgetExhausted) {
                                let _ = self.root.cancel(CancellationReason::EssentialServiceFailure);
                                let report = self.shutdown(
                                    &mut tasks,
                                    &mut active,
                                    &mut completions,
                                    CancellationReason::EssentialServiceFailure,
                                ).await;
                                self.state.set_lifecycle(RouterLifecycle::Failed);
                                completions.insert(output.name.clone(), output.completion);
                                return Err(SupervisorError::RestartBudgetExhausted {
                                    service: output.name,
                                    report,
                                });
                            } else {
                                self.record_completion(&output.name, &output.completion, &mut completions);
                            }
                        }
                        ServiceClassification::Degradable => {
                            self.set_health(
                                &output.name,
                                LifecycleState::Degraded,
                                HealthState::Degraded(DegradationCode::LocalPolicy),
                                0,
                                output.completion.category(),
                                None,
                            );
                            completions.insert(output.name.clone(), output.completion.clone());
                            self.mark_dependents_degraded(&output.name, &active);
                        }
                        ServiceClassification::Optional => {
                            self.set_health(
                                &output.name,
                                LifecycleState::Failed,
                                HealthState::Failed,
                                0,
                                output.completion.category(),
                                None,
                            );
                            completions.insert(output.name.clone(), output.completion.clone());
                            self.mark_dependents_degraded(&output.name, &active);
                        }
                    }
                    let _ = manager;
                }
            }
        }
    }

    fn spawn_manager(
        &self,
        spec: &ServiceSpec,
        tasks: &mut JoinSet<ManagerOutput>,
    ) -> (ActiveManager, watch::Receiver<bool>) {
        let manager_token = self.root.child_token();
        let (ready_sender, ready_receiver) = watch::channel(false);
        let health = self
            .health
            .get(spec.name())
            .expect("health exists for every graph service")
            .clone();
        let spec = spec.clone();
        let graceful_period = spec.shutdown_grace();
        let manager_name = spec.name().clone();
        let clock = Arc::clone(&self.clock);
        let state = Arc::clone(&self.state);
        let manager_token_for_task = manager_token.clone();
        tasks.spawn(async move {
            match AssertUnwindSafe(run_manager(
                spec,
                manager_token_for_task,
                health,
                clock,
                state,
                ready_sender,
            ))
            .catch_unwind()
            .await
            {
                Ok(output) => output,
                Err(_) => ManagerOutput {
                    name: manager_name,
                    completion: ServiceCompletion::Panic,
                },
            }
        });
        self.state.service_started();
        (
            ActiveManager {
                cancellation: manager_token,
                graceful_period,
            },
            ready_receiver,
        )
    }

    async fn wait_for_initial_ready(
        &self,
        current: &ServiceName,
        timeout: Duration,
        mut receiver: watch::Receiver<bool>,
        tasks: &mut JoinSet<ManagerOutput>,
        active: &mut BTreeMap<ServiceName, ActiveManager>,
        completions: &mut BTreeMap<ServiceName, ServiceCompletion>,
    ) -> Result<(), ServiceCompletion> {
        let wait = async {
            loop {
                tokio::select! {
                    changed = receiver.changed() => {
                        if changed.is_err() {
                            loop {
                                self.state.service_finished();
                                match tasks.join_next().await {
                                    Some(Ok(output)) if output.name == *current => {
                                        let reached_ready = self
                                            .health
                                            .get(current)
                                            .is_some_and(|health| health.snapshot().is_ready());
                                        active.remove(current);
                                        if reached_ready
                                            && matches!(
                                                self.graph.service(current).classification(),
                                                ServiceClassification::Degradable
                                                    | ServiceClassification::Optional
                                            )
                                        {
                                            self.record_startup_completion(
                                                &output.name,
                                                &output.completion,
                                                completions,
                                            );
                                            return Ok(());
                                        }
                                        self.record_completion(
                                            &output.name,
                                            &output.completion,
                                            completions,
                                        );
                                        return Err(output.completion);
                                    }
                                    Some(Ok(output)) => {
                                        active.remove(&output.name);
                                        self.record_startup_completion(
                                            &output.name,
                                            &output.completion,
                                            completions,
                                        );
                                        if self.graph.service(&output.name).classification()
                                            == ServiceClassification::Essential
                                            && output.completion.is_failure()
                                        {
                                            return Err(output.completion);
                                        }
                                        if self.graph.service(current).dependencies().contains(&output.name) {
                                            return Err(ServiceCompletion::Failed(ServiceFailure::new(
                                                ServiceFailureCategory::DependencyUnavailable,
                                                None,
                                            )));
                                        }
                                    }
                                    Some(Err(_)) => return Err(ServiceCompletion::TaskJoinFailure),
                                    None => return Err(ServiceCompletion::TaskJoinFailure),
                                }
                            }
                        }
                        if *receiver.borrow() {
                            return Ok(());
                        }
                    }
                    joined = tasks.join_next() => {
                        self.state.service_finished();
                        match joined {
                            Some(Ok(output)) if output.name == *current => {
                                let reached_ready = self
                                    .health
                                    .get(current)
                                    .is_some_and(|health| health.snapshot().is_ready());
                                active.remove(current);
                                if reached_ready
                                    && matches!(
                                        self.graph.service(current).classification(),
                                        ServiceClassification::Degradable
                                            | ServiceClassification::Optional
                                    )
                                {
                                    self.record_startup_completion(
                                        &output.name,
                                        &output.completion,
                                        completions,
                                    );
                                    return Ok(());
                                }
                                self.record_completion(
                                    &output.name,
                                    &output.completion,
                                    completions,
                                );
                                return Err(output.completion);
                            }
                            Some(Ok(output)) => {
                                active.remove(&output.name);
                                self.record_startup_completion(
                                    &output.name,
                                    &output.completion,
                                    completions,
                                );
                                if self.graph.service(&output.name).classification()
                                    == ServiceClassification::Essential
                                    && output.completion.is_failure()
                                {
                                    return Err(output.completion);
                                }
                                if self.graph.service(current).dependencies().contains(&output.name) {
                                    return Err(ServiceCompletion::Failed(ServiceFailure::new(
                                        ServiceFailureCategory::DependencyUnavailable,
                                        None,
                                    )));
                                }
                            }
                            Some(Err(_)) => return Err(ServiceCompletion::TaskJoinFailure),
                            None => return Err(ServiceCompletion::TaskJoinFailure),
                        }
                    }
                }
            }
        };
        match tokio::time::timeout(timeout, wait).await {
            Ok(result) => result,
            Err(_) => Err(ServiceCompletion::ReadinessTimeout),
        }
    }

    fn set_health(
        &self,
        name: &ServiceName,
        lifecycle: LifecycleState,
        health: HealthState,
        restart_count: u32,
        failure: Option<FailureCategory>,
        detail: Option<HealthDetail>,
    ) {
        if let Some(health_state) = self.health.get(name) {
            health_state.set(lifecycle, health, restart_count, failure, detail);
            let event_name = match health {
                HealthState::Degraded(_) => Some(event::SERVICE_DEGRADED),
                HealthState::Stopping => Some(event::SERVICE_STOPPING),
                HealthState::Failed => Some(event::SERVICE_FAILED),
                HealthState::Starting | HealthState::Ready => None,
            };
            if let Some(event_name) = event_name {
                service_event(
                    name,
                    self.graph.service(name).classification(),
                    lifecycle,
                    restart_count,
                    failure,
                    event_name,
                );
            }
        }
    }

    fn record_completion(
        &self,
        name: &ServiceName,
        completion: &ServiceCompletion,
        completions: &mut BTreeMap<ServiceName, ServiceCompletion>,
    ) {
        let (lifecycle, health) = if matches!(completion, ServiceCompletion::RequestedShutdown) {
            (LifecycleState::Stopped, HealthState::Stopping)
        } else {
            (LifecycleState::Failed, HealthState::Failed)
        };
        self.set_health(name, lifecycle, health, 0, completion.category(), None);
        completions.insert(name.clone(), completion.clone());
    }

    fn record_startup_completion(
        &self,
        name: &ServiceName,
        completion: &ServiceCompletion,
        completions: &mut BTreeMap<ServiceName, ServiceCompletion>,
    ) {
        if matches!(completion, ServiceCompletion::RequestedShutdown) {
            self.record_completion(name, completion, completions);
            return;
        }
        let service = self.graph.service(name);
        match service.classification() {
            ServiceClassification::Degradable => self.set_health(
                name,
                LifecycleState::Degraded,
                HealthState::Degraded(DegradationCode::LocalPolicy),
                0,
                completion.category(),
                None,
            ),
            ServiceClassification::Optional => self.set_health(
                name,
                LifecycleState::Failed,
                HealthState::Failed,
                0,
                completion.category(),
                None,
            ),
            ServiceClassification::Restartable => self.set_health(
                name,
                LifecycleState::Failed,
                HealthState::Failed,
                0,
                completion.category(),
                None,
            ),
            ServiceClassification::Essential => self.set_health(
                name,
                LifecycleState::Failed,
                HealthState::Failed,
                0,
                completion.category(),
                None,
            ),
        }
        completions.insert(name.clone(), completion.clone());
    }

    fn mark_dependents_degraded(
        &self,
        failed: &ServiceName,
        active: &BTreeMap<ServiceName, ActiveManager>,
    ) {
        for (name, manager) in active {
            if self.graph.service(name).dependencies().contains(failed) {
                let _ = manager.cancellation.cancel(CancellationReason::ParentScope);
                self.set_health(
                    name,
                    LifecycleState::Degraded,
                    HealthState::Degraded(DegradationCode::DependencyUnavailable),
                    0,
                    Some(FailureCategory::DependencyUnavailable),
                    None,
                );
            }
        }
    }

    async fn shutdown(
        &self,
        tasks: &mut JoinSet<ManagerOutput>,
        active: &mut BTreeMap<ServiceName, ActiveManager>,
        completions: &mut BTreeMap<ServiceName, ServiceCompletion>,
        reason: CancellationReason,
    ) -> ShutdownReport {
        self.state.set_lifecycle(RouterLifecycle::Stopping);
        self.state.request_shutdown();
        shutdown_event(event::SHUTDOWN_REQUESTED, 0);
        let _ = self.root.cancel(reason);
        for manager in active.values() {
            let _ = manager.cancellation.cancel(reason);
        }

        let mut forced_services = BTreeSet::new();
        let mut joined_tasks = 0;
        let mut cleanup_failed = false;
        let mut aborted = false;
        let graceful_period = active
            .values()
            .map(|manager| manager.graceful_period)
            .max()
            .unwrap_or(self.shutdown_deadline)
            .min(self.shutdown_deadline);
        let deadline = tokio::time::sleep(graceful_period);
        tokio::pin!(deadline);

        while !tasks.is_empty() {
            tokio::select! {
                joined = tasks.join_next() => {
                    self.state.service_finished();
                    let Some(joined) = joined else { break };
                    joined_tasks += 1;
                    match joined {
                        Ok(output) => {
                            active.remove(&output.name);
                            self.record_completion(&output.name, &output.completion, completions);
                        }
                        Err(_) => {
                            cleanup_failed = true;
                            if let Some(name) = active.keys().next().cloned() {
                                active.remove(&name);
                                let completion = ServiceCompletion::TaskJoinFailure;
                                self.record_completion(&name, &completion, completions);
                            }
                        }
                    }
                }
                _ = &mut deadline => {
                    forced_services.extend(active.keys().cloned());
                    for _ in 0..active.len() {
                        self.state.forced_abort();
                    }
                    for name in &forced_services {
                        let completion = ServiceCompletion::ForcedAbort;
                        self.set_health(
                            name,
                            LifecycleState::Stopped,
                            HealthState::Stopping,
                            0,
                            completion.category(),
                            None,
                        );
                        completions.insert(name.clone(), completion);
                    }
                    active.clear();
                    tasks.abort_all();
                    shutdown_event(event::SHUTDOWN_FORCED, forced_services.len());
                    aborted = true;
                    break;
                }
            }
        }
        while let Some(joined) = tasks.join_next().await {
            self.state.service_finished();
            joined_tasks += 1;
            if joined.is_err() && !aborted {
                cleanup_failed = true;
            }
        }

        let outcome = if cleanup_failed {
            ShutdownOutcome::FailedCleanup
        } else if forced_services.is_empty() {
            ShutdownOutcome::Graceful
        } else {
            ShutdownOutcome::PartiallyForced
        };
        self.state.set_lifecycle(RouterLifecycle::Stopped);
        shutdown_event(event::SERVICE_STOPPED, forced_services.len());
        ShutdownReport {
            outcome,
            forced_services: forced_services.into_iter().collect(),
            completions: completions.clone(),
            joined_tasks,
            remaining_tasks: tasks.len(),
        }
    }
}

async fn run_manager(
    spec: ServiceSpec,
    token: CancellationToken,
    health: Arc<SharedHealth>,
    clock: Arc<RuntimeClock>,
    state: Arc<TaskCounters>,
    startup_sender: watch::Sender<bool>,
) -> ManagerOutput {
    let name = spec.name().clone();
    let mut restart_count = 0;
    loop {
        health.set(
            LifecycleState::Starting,
            HealthState::Starting,
            restart_count,
            None,
            None,
        );
        service_event(
            &name,
            spec.classification(),
            LifecycleState::Starting,
            restart_count,
            None,
            event::SERVICE_STARTING,
        );
        let attempt = run_attempt(
            &spec,
            &token,
            restart_count,
            &health,
            &clock,
            &state,
            &startup_sender,
        )
        .await;
        if matches!(attempt.completion, ServiceCompletion::RequestedShutdown)
            || token.is_cancelled()
        {
            health.set(
                LifecycleState::Stopped,
                HealthState::Stopping,
                restart_count,
                None,
                None,
            );
            service_event(
                &name,
                spec.classification(),
                LifecycleState::Stopping,
                restart_count,
                None,
                event::SERVICE_STOPPING,
            );
            return ManagerOutput {
                name,
                completion: ServiceCompletion::RequestedShutdown,
            };
        }

        let Some(policy) = spec.restart_config() else {
            return ManagerOutput {
                name,
                completion: attempt.completion,
            };
        };
        if spec.classification() != ServiceClassification::Restartable
            || !attempt.completion.is_failure()
        {
            service_event(
                &name,
                spec.classification(),
                LifecycleState::Failed,
                restart_count,
                attempt.completion.category(),
                event::SERVICE_FAILED,
            );
            return ManagerOutput {
                name,
                completion: attempt.completion,
            };
        }
        if restart_count >= policy.max_attempts() {
            let completion = ServiceCompletion::RestartBudgetExhausted;
            health.set(
                match policy.exhaustion() {
                    RestartExhaustion::Degrade => LifecycleState::Degraded,
                    RestartExhaustion::Shutdown => LifecycleState::Failed,
                },
                match policy.exhaustion() {
                    RestartExhaustion::Degrade => {
                        HealthState::Degraded(DegradationCode::LocalPolicy)
                    }
                    RestartExhaustion::Shutdown => HealthState::Failed,
                },
                restart_count,
                completion.category(),
                None,
            );
            service_event(
                &name,
                spec.classification(),
                match policy.exhaustion() {
                    RestartExhaustion::Degrade => LifecycleState::Degraded,
                    RestartExhaustion::Shutdown => LifecycleState::Failed,
                },
                restart_count,
                completion.category(),
                event::SERVICE_FAILED,
            );
            return ManagerOutput { name, completion };
        }
        restart_count += 1;
        let delay = policy.delay_for(restart_count);
        service_event(
            &name,
            spec.classification(),
            LifecycleState::Starting,
            restart_count,
            attempt.completion.category(),
            event::SERVICE_RESTARTING,
        );
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = token.cancelled() => {
                health.set(
                    LifecycleState::Stopped,
                    HealthState::Stopping,
                    restart_count,
                    None,
                    None,
                );
                return ManagerOutput { name, completion: ServiceCompletion::RequestedShutdown };
            }
        }
        if attempt.sustained_ready && policy.reset_interval().is_some() {
            restart_count = 0;
        }
    }
}

async fn run_attempt(
    spec: &ServiceSpec,
    token: &CancellationToken,
    restart_count: u32,
    health: &Arc<SharedHealth>,
    clock: &Arc<RuntimeClock>,
    state: &Arc<TaskCounters>,
    startup_sender: &watch::Sender<bool>,
) -> AttemptOutput {
    let service_token = token.child_token();
    let (readiness, mut readiness_receiver) = Readiness::new();
    let readiness_observer = readiness.clone();
    let children = ChildScope::new(&service_token, spec.child_policy(), Arc::clone(state));
    let context = ServiceContext::new(
        spec.name().clone(),
        service_token.clone(),
        readiness,
        health.reporter(),
        children.clone(),
    );
    let factory = spec.factory();
    let service =
        std::panic::AssertUnwindSafe(async move { (factory)(context).await }).catch_unwind();
    tokio::pin!(service);

    let mut ready_at = None;
    let phase = tokio::time::timeout(spec.startup_timeout(), async {
        tokio::time::timeout(spec.readiness_timeout(), async {
            tokio::select! {
                ready = Readiness::wait(&mut readiness_receiver) => {
                    if ready.is_ok() {
                        ready_at = Some(clock.now());
                        let _ = startup_sender.send(true);
                        health.set(
                            LifecycleState::Ready,
                            HealthState::Ready,
                            restart_count,
                            None,
                            None,
                        );
                        service_event(
                            spec.name(),
                            spec.classification(),
                            LifecycleState::Ready,
                            restart_count,
                            None,
                            event::SERVICE_READY,
                        );
                        ReadinessPhase::Ready
                    } else {
                        ReadinessPhase::Closed
                    }
                }
                result = &mut service => ReadinessPhase::Completed(result),
            }
        })
        .await
    })
    .await;

    let completion = match phase {
        Err(_) => ServiceCompletion::StartupTimeout,
        Ok(Err(_)) => ServiceCompletion::ReadinessTimeout,
        Ok(Ok(ReadinessPhase::Ready | ReadinessPhase::Closed)) => match service.await {
            Ok(ServiceResult::RequestedShutdown) => ServiceCompletion::RequestedShutdown,
            Ok(ServiceResult::Completed) => {
                if service_token.is_cancelled() {
                    ServiceCompletion::RequestedShutdown
                } else {
                    ServiceCompletion::UnexpectedCleanExit
                }
            }
            Ok(ServiceResult::Failed(failure)) => ServiceCompletion::Failed(failure),
            Err(_) => ServiceCompletion::Panic,
        },
        Ok(Ok(ReadinessPhase::Completed(result))) => match result {
            Ok(ServiceResult::RequestedShutdown) => ServiceCompletion::RequestedShutdown,
            Ok(ServiceResult::Completed) => {
                if service_token.is_cancelled() {
                    ServiceCompletion::RequestedShutdown
                } else {
                    ServiceCompletion::UnexpectedCleanExit
                }
            }
            Ok(ServiceResult::Failed(failure)) => ServiceCompletion::Failed(failure),
            Err(_) => ServiceCompletion::Panic,
        },
    };

    if readiness_observer.is_signalled() && ready_at.is_none() {
        ready_at = Some(clock.now());
        let _ = startup_sender.send(true);
        health.set(
            LifecycleState::Ready,
            HealthState::Ready,
            restart_count,
            None,
            None,
        );
    }

    let child_report = children.shutdown().await;
    let completion = if child_report.failed() && children.policy() == ChildFailurePolicy::FailParent
    {
        ServiceCompletion::Failed(ServiceFailure::new(
            ServiceFailureCategory::Internal,
            HealthDetail::new("owned child task failed").ok(),
        ))
    } else {
        if child_report.failed()
            && children.policy() == ChildFailurePolicy::DegradeParent
            && !matches!(completion, ServiceCompletion::RequestedShutdown)
        {
            health.set(
                LifecycleState::Degraded,
                HealthState::Degraded(DegradationCode::LocalPolicy),
                restart_count,
                Some(FailureCategory::ServiceFailure),
                HealthDetail::new("owned child task degraded parent").ok(),
            );
        }
        completion
    };

    let sustained_ready = ready_at
        .zip(
            spec.restart_config()
                .and_then(|policy| policy.reset_interval()),
        )
        .is_some_and(|(ready_at, duration)| clock.now().saturating_sub(ready_at) >= duration);
    AttemptOutput {
        completion,
        sustained_ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{RestartPolicy, ServiceGraph};
    use i2pr_core::{CancellationReason, ServiceName, ShutdownReason};

    fn name(value: &str) -> ServiceName {
        ServiceName::new(value).expect("valid name")
    }

    fn forever_service(value: &str, classification: ServiceClassification) -> ServiceSpec {
        ServiceSpec::new(name(value), classification, |context| async move {
            context.signal_ready().expect("first readiness");
            context.cancellation().cancelled().await;
            ServiceResult::RequestedShutdown
        })
    }

    fn graph(service: ServiceSpec) -> ServiceGraph {
        let mut builder = ServiceGraph::builder(8).expect("bound");
        builder.register(service).expect("register");
        builder.build().expect("graph")
    }

    #[tokio::test(start_paused = true)]
    async fn services_start_ready_and_shutdown_gracefully() {
        let supervisor = Supervisor::new(
            graph(forever_service("core", ServiceClassification::Essential)),
            Duration::from_secs(5),
        )
        .expect("supervisor");
        let handle = supervisor.handle();
        let task = tokio::spawn(supervisor.run());
        tokio::task::yield_now().await;
        assert!(handle.shutdown(ShutdownReason::Test));
        let report = task.await.expect("supervisor joined").expect("graceful");
        assert!(report.was_graceful());
        assert_eq!(report.remaining_tasks(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn supervisor_snapshot_tracks_readiness_and_owned_tasks() {
        let supervisor = Supervisor::new(
            graph(forever_service("core", ServiceClassification::Essential)),
            Duration::from_secs(5),
        )
        .expect("supervisor");
        let handle = supervisor.handle();
        assert_eq!(handle.snapshot().lifecycle, RouterLifecycle::Registered);
        assert!(!handle.snapshot().ready);
        let task = tokio::spawn(supervisor.run());
        for _ in 0..32 {
            tokio::task::yield_now().await;
            if handle.snapshot().ready {
                break;
            }
        }
        let running = handle.snapshot();
        assert_eq!(running.lifecycle, RouterLifecycle::Ready);
        assert!(running.ready, "running snapshot: {running:?}");
        assert_eq!(running.owned_service_tasks, 1);
        assert_eq!(running.owned_child_tasks, 0);
        assert!(!format!("{running:?}").contains("private"));
        handle.shutdown(ShutdownReason::Test);
        let report = task.await.expect("supervisor joined").expect("graceful");
        assert!(report.was_graceful());
        let stopped = handle.snapshot();
        assert_eq!(stopped.lifecycle, RouterLifecycle::Stopped);
        assert_eq!(stopped.owned_service_tasks, 0);
        assert_eq!(stopped.owned_child_tasks, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn panic_is_classified_without_payload() {
        let service = ServiceSpec::new(
            name("panic"),
            ServiceClassification::Essential,
            |_context| async {
                panic!("secret panic payload");
            },
        );
        let supervisor =
            Supervisor::new(graph(service), Duration::from_secs(5)).expect("supervisor");
        let result = supervisor.run().await;
        let Err(SupervisorError::StartupFailed {
            service,
            completion,
            ..
        }) = result
        else {
            panic!("panic should fail startup");
        };
        assert_eq!(service, name("panic"));
        assert_eq!(completion, ServiceCompletion::Panic);
        assert!(!format!("{completion:?}").contains("secret panic payload"));
    }

    #[tokio::test(start_paused = true)]
    async fn essential_failure_during_later_startup_stops_the_graph() {
        let failed = ServiceSpec::new(
            name("a-essential"),
            ServiceClassification::Essential,
            |context| async move {
                context.signal_ready().expect("ready");
                ServiceResult::Failed(ServiceFailure::new(ServiceFailureCategory::Internal, None))
            },
        );
        let waiting = ServiceSpec::new(
            name("b-essential"),
            ServiceClassification::Essential,
            |_context| async move { std::future::pending::<ServiceResult>().await },
        );
        let mut builder = ServiceGraph::builder(8).expect("bound");
        builder.register(failed).expect("register");
        builder.register(waiting).expect("register");
        let supervisor = Supervisor::new(builder.build().expect("graph"), Duration::from_secs(5))
            .expect("supervisor");
        let result = supervisor.run().await;
        let Err(SupervisorError::StartupFailed { completion, .. }) = result else {
            panic!("essential startup failure should stop the graph");
        };
        assert!(matches!(
            completion,
            ServiceCompletion::Failed(failure)
                if failure.category() == ServiceFailureCategory::Internal
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn restartable_services_use_bounded_backoff() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attempts_for_factory = Arc::clone(&attempts);
        let policy = RestartPolicy::new(2, Duration::from_secs(1), Duration::from_secs(2))
            .expect("policy")
            .on_exhaustion(RestartExhaustion::Degrade);
        let service = ServiceSpec::new(
            name("worker"),
            ServiceClassification::Restartable,
            move |context| {
                let attempts = Arc::clone(&attempts_for_factory);
                async move {
                    let number = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if number < 2 {
                        return ServiceResult::Failed(ServiceFailure::new(
                            ServiceFailureCategory::Internal,
                            None,
                        ));
                    }
                    context.signal_ready().expect("ready");
                    context.cancellation().cancelled().await;
                    ServiceResult::RequestedShutdown
                }
            },
        )
        .restart_policy(policy);
        let graph = graph_with_optional_essential(service);
        let supervisor = Supervisor::new(graph, Duration::from_secs(5)).expect("supervisor");
        let handle = supervisor.handle();
        let task = tokio::spawn(supervisor.run());
        while attempts.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(1)).await;
        while attempts.load(std::sync::atomic::Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(2)).await;
        while attempts.load(std::sync::atomic::Ordering::SeqCst) < 3 {
            tokio::task::yield_now().await;
        }
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
        handle.shutdown(ShutdownReason::Test);
        let _ = task.await.expect("joined").expect("shutdown");
    }

    fn graph_with_optional_essential(restartable: ServiceSpec) -> ServiceGraph {
        let mut builder = ServiceGraph::builder(8).expect("bound");
        builder
            .register(forever_service(
                "essential",
                ServiceClassification::Essential,
            ))
            .expect("register");
        builder.register(restartable).expect("register");
        builder.build().expect("graph")
    }

    #[tokio::test(start_paused = true)]
    async fn degradable_and_optional_failures_do_not_stop_essential_work() {
        let degraded = ServiceSpec::new(
            name("degradable"),
            ServiceClassification::Degradable,
            |context| async move {
                context.signal_ready().expect("ready");
                ServiceResult::Failed(ServiceFailure::new(ServiceFailureCategory::Internal, None))
            },
        )
        .depends_on(name("essential"));
        let optional = ServiceSpec::new(
            name("optional"),
            ServiceClassification::Optional,
            |context| async move {
                context.signal_ready().expect("ready");
                ServiceResult::Failed(ServiceFailure::new(ServiceFailureCategory::Internal, None))
            },
        )
        .depends_on(name("essential"));
        let mut builder = ServiceGraph::builder(8).expect("bound");
        builder
            .register(forever_service(
                "essential",
                ServiceClassification::Essential,
            ))
            .expect("register");
        builder.register(degraded).expect("register");
        builder.register(optional).expect("register");
        let graph = builder.build().expect("graph");
        let supervisor = Supervisor::new(graph, Duration::from_secs(5)).expect("supervisor");
        let mut degraded_health = supervisor.health(&name("degradable")).expect("health");
        let mut optional_health = supervisor.health(&name("optional")).expect("health");
        let handle = supervisor.handle();
        let task = tokio::spawn(supervisor.run());
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(
            matches!(
                degraded_health.snapshot().health(),
                HealthState::Degraded(DegradationCode::LocalPolicy)
            ),
            "degraded snapshot: {:?}",
            degraded_health.snapshot()
        );
        assert_eq!(
            degraded_health
                .snapshot()
                .service_name()
                .expect("service name")
                .as_str(),
            "degradable"
        );
        assert_eq!(
            degraded_health.snapshot().classification(),
            Some(ServiceClassification::Degradable)
        );
        assert_eq!(optional_health.snapshot().health(), HealthState::Failed);
        assert!(!handle.is_shutdown_requested());
        handle.shutdown(ShutdownReason::Test);
        let report = task.await.expect("joined").expect("shutdown");
        assert!(report.was_graceful());
        let _ = degraded_health.changed().await;
        let _ = optional_health.changed().await;
    }

    #[tokio::test(start_paused = true)]
    async fn forced_shutdown_aborts_noncooperative_service() {
        let service = ServiceSpec::new(
            name("stuck"),
            ServiceClassification::Essential,
            |context| async move {
                context.signal_ready().expect("ready");
                std::future::pending::<ServiceResult>().await
            },
        );
        let supervisor =
            Supervisor::new(graph(service), Duration::from_secs(2)).expect("supervisor");
        let handle = supervisor.handle();
        let task = tokio::spawn(supervisor.run());
        tokio::task::yield_now().await;
        handle.shutdown(ShutdownReason::Test);
        tokio::time::advance(Duration::from_secs(2)).await;
        let result = task.await.expect("supervisor joined").expect("report");
        assert_eq!(result.outcome(), ShutdownOutcome::PartiallyForced);
        assert_eq!(result.remaining_tasks(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn service_child_scope_is_joined_before_manager_completion() {
        let child_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let child_finished_for_factory = Arc::clone(&child_finished);
        let service = ServiceSpec::new(
            name("parent"),
            ServiceClassification::Essential,
            move |context| {
                let child_finished = Arc::clone(&child_finished_for_factory);
                async move {
                    context
                        .children()
                        .spawn(move |cancellation| async move {
                            cancellation.cancelled().await;
                            child_finished.store(true, std::sync::atomic::Ordering::SeqCst);
                            Ok(())
                        })
                        .expect("child registered");
                    context.signal_ready().expect("ready");
                    context.cancellation().cancelled().await;
                    ServiceResult::RequestedShutdown
                }
            },
        );
        let supervisor =
            Supervisor::new(graph(service), Duration::from_secs(5)).expect("supervisor");
        let handle = supervisor.handle();
        let task = tokio::spawn(supervisor.run());
        tokio::task::yield_now().await;
        handle.shutdown(ShutdownReason::Test);
        let report = task.await.expect("joined").expect("shutdown");
        assert!(report.was_graceful());
        assert!(child_finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn cancellation_reason_is_static() {
        assert_eq!(
            CancellationReason::OperatorRequest,
            CancellationReason::OperatorRequest
        );
    }
}
