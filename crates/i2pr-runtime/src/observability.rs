//! Privacy-aware runtime events and bounded aggregate snapshots.
//!
//! This module is deliberately an observation boundary, not an event store.
//! Snapshots contain aggregate counters and typed categories only.  In
//! particular, service health detail is omitted from the redacted projection
//! so a parser-controlled string cannot become a default diagnostic field.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_core::{
    FailureCategory, HealthSnapshot, HealthState, LifecycleState, ResourceUsage,
    ServiceClassification, ServiceName,
};

use crate::channel::ChannelSnapshot;
use crate::context::RuntimeClock;

/// Stable names for structured runtime and simulation events.
pub mod event {
    /// A service registration passed graph validation.
    pub const SERVICE_REGISTERED: &str = "runtime.service.registered";
    /// A service instance began startup.
    pub const SERVICE_STARTING: &str = "runtime.service.starting";
    /// A service instance signalled readiness.
    pub const SERVICE_READY: &str = "runtime.service.ready";
    /// A service reported degraded health.
    pub const SERVICE_DEGRADED: &str = "runtime.service.degraded";
    /// A service failed with a static category.
    pub const SERVICE_FAILED: &str = "runtime.service.failed";
    /// A service replacement attempt was scheduled.
    pub const SERVICE_RESTARTING: &str = "runtime.service.restarting";
    /// A service began shutdown.
    pub const SERVICE_STOPPING: &str = "runtime.service.stopping";
    /// A service completed shutdown.
    pub const SERVICE_STOPPED: &str = "runtime.service.stopped";
    /// The supervisor received a shutdown request.
    pub const SHUTDOWN_REQUESTED: &str = "runtime.shutdown.requested";
    /// The supervisor had to abort one or more services.
    pub const SHUTDOWN_FORCED: &str = "runtime.shutdown.forced";
    /// A bounded channel rejected or dropped an item.
    pub const CHANNEL_REJECTED: &str = "runtime.channel.rejected";
    /// A bounded resource admission was denied.
    pub const RESOURCE_DENIED: &str = "runtime.resource.denied";
    /// A controlled NTCP2 listener accepted an admitted stream.
    pub const NTCP2_ACCEPTED: &str = "runtime.ntcp2.accepted";
    /// A controlled NTCP2 inbound attempt was denied by a bounded limit.
    pub const NTCP2_ADMISSION_DENIED: &str = "runtime.ntcp2.admission_denied";
    /// A controlled NTCP2 dial completed with a typed outcome.
    pub const NTCP2_DIAL_COMPLETED: &str = "runtime.ntcp2.dial_completed";
    /// A controlled NTCP2 link was replaced or entered drain.
    pub const NTCP2_LINK_REPLACED: &str = "runtime.ntcp2.link_replaced";
    /// A controlled NTCP2 link closed with a typed category.
    pub const NTCP2_LINK_CLOSED: &str = "runtime.ntcp2.link_closed";
    /// A synthetic fault rule was applied.
    pub const SIMULATION_FAULT_APPLIED: &str = "simulation.fault.applied";
    /// A deterministic simulation completed.
    pub const SIMULATION_COMPLETED: &str = "simulation.completed";
}

/// Maximum number of channel observations retained by one aggregate snapshot.
pub const MAX_SNAPSHOT_CHANNELS: usize = 256;
/// Maximum number of resource observations retained by one aggregate snapshot.
pub const MAX_SNAPSHOT_RESOURCES: usize = 32;

/// Router-level lifecycle used by aggregate snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterLifecycle {
    /// The validated graph has not started.
    Registered,
    /// Startup sequencing is in progress.
    Starting,
    /// All required services have signalled readiness.
    Ready,
    /// Shutdown has begun.
    Stopping,
    /// All owned work has completed.
    Stopped,
    /// Startup or essential operation failed.
    Failed,
}

/// Redacted service state retained in a runtime snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceSnapshot {
    /// Validated static service identifier.
    pub service: ServiceName,
    /// Service failure classification.
    pub classification: ServiceClassification,
    /// Current lifecycle phase.
    pub lifecycle: LifecycleState,
    /// Current typed health state.
    pub health: HealthState,
    /// Number of replacement attempts started.
    pub restart_count: u32,
    /// Last static failure category, if any.
    pub last_failure: Option<FailureCategory>,
    /// Monotonic transition sequence.
    pub transition_sequence: u64,
    /// Monotonic elapsed time at the transition.
    pub transition_time: Duration,
}

impl ServiceSnapshot {
    fn from_health(snapshot: &HealthSnapshot) -> Option<Self> {
        Some(Self {
            service: snapshot.service_name()?.clone(),
            classification: snapshot.classification()?,
            lifecycle: snapshot.lifecycle(),
            health: snapshot.health(),
            restart_count: snapshot.restart_count(),
            last_failure: snapshot.last_failure(),
            transition_sequence: snapshot.transition_sequence(),
            transition_time: snapshot.transition_time(),
        })
    }
}

/// Bounded supervisor state without arbitrary health detail text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupervisorSnapshot {
    /// Router lifecycle.
    pub lifecycle: RouterLifecycle,
    /// Whether all required services are currently ready.
    pub ready: bool,
    /// Per-service redacted observations in deterministic order.
    pub services: Vec<ServiceSnapshot>,
    /// Currently owned service manager tasks.
    pub owned_service_tasks: usize,
    /// Currently owned child tasks.
    pub owned_child_tasks: usize,
    /// Whether cancellation has been requested.
    pub shutdown_requested: bool,
    /// Number of services forced through abort cleanup.
    pub forced_abort_count: usize,
    /// Monotonic elapsed runtime since supervisor construction.
    pub elapsed: Duration,
}

/// Aggregate counts supplied by a deterministic simulation, if present.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SimulationSnapshot {
    /// Pending scheduled delivery units.
    pub pending_deliveries: usize,
    /// Bytes retained by scheduled delivery units.
    pub buffered_bytes: usize,
    /// Registered manual timers.
    pub pending_timers: usize,
    /// Active synthetic stream links.
    pub stream_links: u64,
    /// Active synthetic datagram links.
    pub datagram_links: u64,
}

/// Error returned when an aggregate snapshot input exceeds its bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// Too many channel observations were supplied.
    TooManyChannels { maximum: usize },
    /// Too many resource observations were supplied.
    TooManyResources { maximum: usize },
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyChannels { maximum } => {
                write!(formatter, "runtime snapshot exceeds {maximum} channels")
            }
            Self::TooManyResources { maximum } => {
                write!(formatter, "runtime snapshot exceeds {maximum} resources")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Coherent-at-call-time aggregate of runtime and optional simulation state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSnapshot {
    /// Supervisor lifecycle and service observations.
    pub supervisor: SupervisorSnapshot,
    /// Bounded channel metadata and counters.
    pub channels: Vec<ChannelSnapshot>,
    /// Bounded resource metadata and counters.
    pub resources: Vec<ResourceUsage>,
    /// Aggregate simulation counters.
    pub simulation: SimulationSnapshot,
}

impl RuntimeSnapshot {
    /// Assembles a bounded snapshot without awaiting or retaining mutable locks.
    pub fn try_new(
        supervisor: SupervisorSnapshot,
        mut channels: Vec<ChannelSnapshot>,
        mut resources: Vec<ResourceUsage>,
        simulation: SimulationSnapshot,
    ) -> Result<Self, SnapshotError> {
        if channels.len() > MAX_SNAPSHOT_CHANNELS {
            return Err(SnapshotError::TooManyChannels {
                maximum: MAX_SNAPSHOT_CHANNELS,
            });
        }
        if resources.len() > MAX_SNAPSHOT_RESOURCES {
            return Err(SnapshotError::TooManyResources {
                maximum: MAX_SNAPSHOT_RESOURCES,
            });
        }
        channels.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        resources.sort_unstable_by_key(|resource| resource.class);
        Ok(Self {
            supervisor,
            channels,
            resources,
            simulation,
        })
    }
}

/// Shared counters used to prove task ownership and final cleanup.
#[derive(Debug)]
pub(crate) struct TaskCounters {
    lifecycle: Mutex<RouterLifecycle>,
    service_tasks: AtomicUsize,
    child_tasks: AtomicUsize,
    shutdown_requested: AtomicBool,
    forced_aborts: AtomicUsize,
}

impl TaskCounters {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            lifecycle: Mutex::new(RouterLifecycle::Registered),
            service_tasks: AtomicUsize::new(0),
            child_tasks: AtomicUsize::new(0),
            shutdown_requested: AtomicBool::new(false),
            forced_aborts: AtomicUsize::new(0),
        })
    }

    pub(crate) fn set_lifecycle(&self, lifecycle: RouterLifecycle) {
        if let Ok(mut current) = self.lifecycle.lock() {
            *current = lifecycle;
        }
    }

    pub(crate) fn service_started(&self) {
        self.service_tasks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn service_finished(&self) {
        self.service_tasks
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            })
            .ok();
    }

    pub(crate) fn child_started(&self) {
        self.child_tasks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn child_finished(&self) {
        self.child_tasks
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            })
            .ok();
    }

    pub(crate) fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    pub(crate) fn forced_abort(&self) {
        self.forced_aborts.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(
        &self,
        health: &BTreeMap<ServiceName, Arc<crate::context::SharedHealth>>,
        clock: &RuntimeClock,
    ) -> SupervisorSnapshot {
        let services: Vec<ServiceSnapshot> = health
            .values()
            .filter_map(|health| ServiceSnapshot::from_health(&health.snapshot()))
            .collect();
        SupervisorSnapshot {
            lifecycle: self
                .lifecycle
                .lock()
                .map(|lifecycle| *lifecycle)
                .unwrap_or(RouterLifecycle::Failed),
            ready: services
                .iter()
                .filter(|service| {
                    matches!(
                        service.classification,
                        ServiceClassification::Essential | ServiceClassification::Restartable
                    )
                })
                .all(|service| service.health.is_ready()),
            services,
            owned_service_tasks: self.service_tasks.load(Ordering::Acquire),
            owned_child_tasks: self.child_tasks.load(Ordering::Acquire),
            shutdown_requested: self.shutdown_requested.load(Ordering::Acquire),
            forced_abort_count: self.forced_aborts.load(Ordering::Acquire),
            elapsed: clock.now(),
        }
    }
}

/// Emits a lifecycle event using only validated static or typed fields.
pub(crate) fn service_event(
    name: &ServiceName,
    classification: ServiceClassification,
    lifecycle: LifecycleState,
    restart_count: u32,
    failure: Option<FailureCategory>,
    event_name: &'static str,
) {
    tracing::info!(
        target: "i2pr.runtime",
        event = event_name,
        service = %name,
        classification = ?classification,
        lifecycle = ?lifecycle,
        restart_attempts = restart_count,
        failure_category = ?failure,
    );
}

/// Emits a bounded shutdown event.
pub(crate) fn shutdown_event(event_name: &'static str, forced_services: usize) {
    tracing::info!(
        target: "i2pr.runtime",
        event = event_name,
        forced_services,
    );
}

/// Emits a bounded channel outcome without retaining or formatting its item.
pub(crate) fn channel_event(
    snapshot: &ChannelSnapshot,
    event_name: &'static str,
    outcome: &'static str,
) {
    tracing::debug!(
        target: "i2pr.runtime",
        event = event_name,
        channel = %snapshot.name,
        owner = %snapshot.owner,
        outcome,
        capacity_slots = snapshot.capacity,
        queue_depth_items = snapshot.queued,
    );
}
