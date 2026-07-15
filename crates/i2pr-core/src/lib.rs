//! Runtime-neutral contracts shared by the future router services.
//!
//! This crate owns small lifecycle, health, cancellation, and resource-domain
//! types.  It does not own a runtime, configuration parsing, filesystem state,
//! network transports, protocol codecs, or router composition.

#![forbid(unsafe_code)]

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Maximum UTF-8 byte length of a service identifier.
pub const MAX_SERVICE_NAME_BYTES: usize = 64;
/// Maximum UTF-8 byte length of bounded health context.
pub const MAX_HEALTH_DETAIL_BYTES: usize = 160;

/// A bounded, human-readable service identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ServiceName(String);

impl ServiceName {
    /// Creates a service name after applying the shared size and emptiness rules.
    pub fn new(value: impl Into<String>) -> Result<Self, ServiceNameError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ServiceNameError::Empty);
        }
        if value.len() > MAX_SERVICE_NAME_BYTES {
            return Err(ServiceNameError::TooLong {
                maximum: MAX_SERVICE_NAME_BYTES,
            });
        }
        Ok(Self(value))
    }

    /// Returns the validated name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ServiceName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Error returned when a service name violates its bounded contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceNameError {
    /// The supplied name was empty.
    Empty,
    /// The supplied name exceeded the maximum byte length.
    TooLong { maximum: usize },
}

impl fmt::Display for ServiceNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("service name must not be empty"),
            Self::TooLong { maximum } => {
                write!(formatter, "service name exceeds the {maximum}-byte limit")
            }
        }
    }
}

impl std::error::Error for ServiceNameError {}

/// Lifecycle state of a supervised service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    /// The service is registered but has not entered startup sequencing.
    Registered,
    /// The service is waiting for its declared dependencies.
    WaitingForDependencies,
    /// Startup is in progress.
    Starting,
    /// The service can serve its required work.
    Ready,
    /// The service remains live but has reduced capability.
    Degraded,
    /// Shutdown has begun.
    Stopping,
    /// Shutdown completed.
    Stopped,
    /// Startup or operation failed and the service cannot recover in place.
    Failed,
}

impl LifecycleState {
    /// Attempts a state transition while enforcing the initial lifecycle graph.
    pub fn transition(self, next: Self) -> Result<Self, InvalidLifecycleTransition> {
        let valid = self == next
            || matches!(
                (self, next),
                (
                    Self::Registered,
                    Self::WaitingForDependencies | Self::Starting | Self::Stopping
                ) | (
                    Self::WaitingForDependencies,
                    Self::Starting | Self::Stopping | Self::Failed
                ) | (
                    Self::Starting,
                    Self::Ready | Self::Degraded | Self::Stopping | Self::Failed
                ) | (Self::Ready, Self::Degraded | Self::Stopping | Self::Failed)
                    | (Self::Degraded, Self::Ready | Self::Stopping | Self::Failed)
                    | (Self::Stopping, Self::Stopped)
                    | (Self::Failed, Self::Stopping)
            );
        if valid {
            Ok(next)
        } else {
            Err(InvalidLifecycleTransition {
                from: self,
                to: next,
            })
        }
    }

    /// Returns whether the state represents a completed terminal condition.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }
}

/// An attempted lifecycle transition that is not allowed by the initial model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidLifecycleTransition {
    /// State before the attempted transition.
    pub from: LifecycleState,
    /// Requested destination state.
    pub to: LifecycleState,
}

impl fmt::Display for InvalidLifecycleTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid lifecycle transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for InvalidLifecycleTransition {}

/// Failure policy for a long-lived service.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ServiceClassification {
    /// Failure cancels the router and produces a router failure result.
    Essential,
    /// Failure may be recovered by an explicit bounded restart policy.
    Restartable,
    /// Failure is visible as degradation while other required services continue.
    Degradable,
    /// Failure is recorded without changing router readiness.
    Optional,
}

/// Static categories that may be safely retained in health and completion data.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FailureCategory {
    /// A service returned a typed failure.
    ServiceFailure,
    /// A service exited normally without a shutdown request.
    UnexpectedCleanExit,
    /// The task was terminated by a panic.
    Panic,
    /// The runtime could not join the task.
    TaskJoinFailure,
    /// A service did not start before its startup deadline.
    StartupTimeout,
    /// A service did not signal readiness before its deadline.
    ReadinessTimeout,
    /// Graceful shutdown exceeded its deadline.
    GracefulShutdownTimeout,
    /// A remaining task was forcibly aborted.
    ForcedAbort,
    /// The restart policy no longer permits another attempt.
    RestartBudgetExhausted,
    /// A hard dependency is permanently unavailable.
    DependencyUnavailable,
}

/// Typed categories a service may report without exposing arbitrary errors.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ServiceFailureCategory {
    /// A bounded internal service failure.
    Internal,
    /// A required dependency was unavailable.
    DependencyUnavailable,
    /// A bounded resource could not be acquired.
    ResourceExhausted,
    /// The service observed an invalid local state.
    InvalidState,
}

/// A service failure with privacy-safe, bounded optional detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceFailure {
    category: ServiceFailureCategory,
    detail: Option<HealthDetail>,
}

impl ServiceFailure {
    /// Creates a typed failure. Detail is retained only after bounded validation.
    pub const fn new(category: ServiceFailureCategory, detail: Option<HealthDetail>) -> Self {
        Self { category, detail }
    }

    /// Returns the static failure category.
    pub const fn category(&self) -> ServiceFailureCategory {
        self.category
    }

    /// Returns bounded diagnostic context, if supplied.
    pub fn detail(&self) -> Option<&HealthDetail> {
        self.detail.as_ref()
    }
}

/// The completion reported by a service future or synthesized by its owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceCompletion {
    /// The service observed an owned cancellation and exited cleanly.
    RequestedShutdown,
    /// The service returned normally without an owned shutdown request.
    UnexpectedCleanExit,
    /// The service returned a typed failure.
    Failed(ServiceFailure),
    /// The task panicked; the panic payload is deliberately not retained.
    Panic,
    /// The runtime could not join the task.
    TaskJoinFailure,
    /// Startup or readiness did not complete before its deadline.
    StartupTimeout,
    /// The service did not signal readiness before its deadline.
    ReadinessTimeout,
    /// Graceful shutdown exceeded its deadline.
    GracefulShutdownTimeout,
    /// The owner forcibly aborted the task.
    ForcedAbort,
    /// The restart budget was exhausted.
    RestartBudgetExhausted,
}

impl ServiceCompletion {
    /// Returns the static category represented by this completion.
    pub const fn category(&self) -> Option<FailureCategory> {
        match self {
            Self::RequestedShutdown => None,
            Self::UnexpectedCleanExit => Some(FailureCategory::UnexpectedCleanExit),
            Self::Failed(_) => Some(FailureCategory::ServiceFailure),
            Self::Panic => Some(FailureCategory::Panic),
            Self::TaskJoinFailure => Some(FailureCategory::TaskJoinFailure),
            Self::StartupTimeout => Some(FailureCategory::StartupTimeout),
            Self::ReadinessTimeout => Some(FailureCategory::ReadinessTimeout),
            Self::GracefulShutdownTimeout => Some(FailureCategory::GracefulShutdownTimeout),
            Self::ForcedAbort => Some(FailureCategory::ForcedAbort),
            Self::RestartBudgetExhausted => Some(FailureCategory::RestartBudgetExhausted),
        }
    }

    /// Whether this completion represents a failure rather than a clean request.
    pub const fn is_failure(&self) -> bool {
        self.category().is_some()
    }
}

/// Bounded reasons for cancellation of runtime-owned work.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CancellationReason {
    /// An operator requested shutdown.
    OperatorRequest,
    /// An essential service failed.
    EssentialServiceFailure,
    /// Startup could not complete.
    StartupFailure,
    /// A shutdown deadline was reached.
    ShutdownDeadline,
    /// A parent scope was cancelled.
    ParentScope,
    /// The deterministic test harness is tearing down.
    TestHarnessTeardown,
}

/// Typed reason for a service to report degraded health.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DegradationCode {
    /// A required-but-recoverable dependency is unavailable.
    DependencyUnavailable,
    /// A shared resource budget is under pressure.
    ResourcePressure,
    /// A local configuration or policy prevents full operation.
    LocalPolicy,
}

/// Health state suitable for a bounded snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthState {
    /// Startup has not reached readiness.
    Starting,
    /// Required service work is available.
    Ready,
    /// The service is live with a typed limitation.
    Degraded(DegradationCode),
    /// Shutdown is in progress.
    Stopping,
    /// The service is no longer live.
    Failed,
}

impl HealthState {
    /// Whether the service should still be considered live.
    pub const fn is_live(self) -> bool {
        !matches!(self, Self::Stopping | Self::Failed)
    }

    /// Whether the service is ready for its required work.
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Bounded diagnostic context for a health snapshot.
#[derive(Clone, Eq, PartialEq)]
pub struct HealthDetail(String);

impl fmt::Debug for HealthDetail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HealthDetail")
            .field("redacted", &true)
            .finish()
    }
}

impl HealthDetail {
    /// Creates context after enforcing the bounded diagnostic limit.
    pub fn new(value: impl Into<String>) -> Result<Self, HealthDetailError> {
        let value = value.into();
        if value.len() > MAX_HEALTH_DETAIL_BYTES {
            return Err(HealthDetailError::TooLong {
                maximum: MAX_HEALTH_DETAIL_BYTES,
            });
        }
        Ok(Self(value))
    }

    /// Returns the bounded context.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned when health context exceeds the bounded snapshot size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthDetailError {
    /// The context exceeded the configured maximum.
    TooLong { maximum: usize },
}

impl fmt::Display for HealthDetailError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::TooLong { maximum } = self;
        write!(formatter, "health detail exceeds the {maximum}-byte limit")
    }
}

impl std::error::Error for HealthDetailError {}

/// Immutable health observation with explicit liveness and readiness flags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthSnapshot {
    service: Option<ServiceName>,
    classification: Option<ServiceClassification>,
    lifecycle: LifecycleState,
    state: HealthState,
    restart_count: u32,
    last_failure: Option<FailureCategory>,
    transition_sequence: u64,
    transition_time: Duration,
    detail: Option<HealthDetail>,
}

impl HealthSnapshot {
    /// Creates a snapshot from typed state and bounded optional context.
    pub const fn new(
        state: HealthState,
        transition_sequence: u64,
        detail: Option<HealthDetail>,
    ) -> Self {
        Self {
            service: None,
            classification: None,
            lifecycle: match state {
                HealthState::Starting => LifecycleState::Starting,
                HealthState::Ready => LifecycleState::Ready,
                HealthState::Degraded(_) => LifecycleState::Degraded,
                HealthState::Stopping => LifecycleState::Stopping,
                HealthState::Failed => LifecycleState::Failed,
            },
            state,
            restart_count: 0,
            last_failure: None,
            transition_sequence,
            transition_time: Duration::ZERO,
            detail,
        }
    }

    /// Creates a full runtime-facing snapshot with bounded service metadata.
    #[allow(clippy::too_many_arguments)]
    pub const fn for_service(
        service: ServiceName,
        classification: ServiceClassification,
        lifecycle: LifecycleState,
        state: HealthState,
        restart_count: u32,
        last_failure: Option<FailureCategory>,
        transition_sequence: u64,
        transition_time: Duration,
        detail: Option<HealthDetail>,
    ) -> Self {
        Self {
            service: Some(service),
            classification: Some(classification),
            lifecycle,
            state,
            restart_count,
            last_failure,
            transition_sequence,
            transition_time,
            detail,
        }
    }

    /// Stable service identity, present for supervisor-created snapshots.
    pub fn service_name(&self) -> Option<&ServiceName> {
        self.service.as_ref()
    }

    /// Registered service failure classification, when known.
    pub const fn classification(&self) -> Option<ServiceClassification> {
        self.classification
    }

    /// Current lifecycle phase.
    pub const fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }

    /// Current health state.
    pub const fn health(&self) -> HealthState {
        self.state
    }

    /// Number of replacement attempts that have started.
    pub const fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Last static failure category, if any.
    pub const fn last_failure(&self) -> Option<FailureCategory> {
        self.last_failure
    }

    /// Current typed state.
    pub const fn state(&self) -> HealthState {
        self.state
    }

    /// Monotonic transition sequence supplied by the owning service.
    pub const fn transition_sequence(&self) -> u64 {
        self.transition_sequence
    }

    /// Monotonic runtime time at which this snapshot was published.
    pub const fn transition_time(&self) -> Duration {
        self.transition_time
    }

    /// Whether this snapshot reports a live service.
    pub const fn is_live(&self) -> bool {
        self.state.is_live()
    }

    /// Whether this snapshot reports a ready service.
    pub const fn is_ready(&self) -> bool {
        self.state.is_ready()
    }

    /// Optional bounded, privacy-reviewed diagnostic context.
    pub fn detail(&self) -> Option<&HealthDetail> {
        self.detail.as_ref()
    }
}

/// Why a service or daemon shutdown was requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownReason {
    /// An operator or owning service requested shutdown.
    Requested,
    /// An operating-system termination signal was received.
    Signal,
    /// An essential service failed.
    FatalFailure,
    /// Configuration prevented startup.
    Configuration,
    /// A deterministic test requested shutdown.
    Test,
}

/// Runtime-neutral cancellation signal for owned work.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Marks this token as cancelled. Repeated cancellation is harmless.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Maximum number of resource classes a budget or bundle may contain.
pub const MAX_RESOURCE_CLASSES: usize = 32;

/// Resource categories reserved for router-wide accounting.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceClass {
    /// Supervised service task count.
    ServiceTasks,
    /// Tasks owned by a supervised service child scope.
    ChildTasks,
    /// Commands retained by a bounded service queue.
    CommandQueueItems,
    /// Events retained by a bounded service queue.
    EventQueueItems,
    /// Bytes retained in bounded buffers.
    BufferedBytes,
    /// Stream links used by a deterministic simulated peer.
    SimulatedStreamLinks,
    /// Datagram links used by a deterministic simulated peer.
    SimulatedDatagramLinks,
    /// Timers registered for later service work.
    PendingTimers,
    /// Peers represented by a deterministic test harness.
    TestPeers,
    /// Legacy aggregate task count retained for existing callers.
    Tasks,
    /// Pending transport handshakes.
    PendingHandshakes,
    /// Active peer links.
    ActiveLinks,
    /// Outstanding NetDB queries.
    NetDbQueries,
    /// In-progress tunnel builds.
    TunnelBuilds,
    /// Local destinations.
    Destinations,
    /// Application streaming sessions.
    Streams,
    /// SAM or I2CP client sessions.
    ApiSessions,
}

impl ResourceClass {
    /// All currently defined classes in deterministic order.
    pub const ALL: [Self; 17] = [
        Self::ServiceTasks,
        Self::ChildTasks,
        Self::CommandQueueItems,
        Self::EventQueueItems,
        Self::BufferedBytes,
        Self::SimulatedStreamLinks,
        Self::SimulatedDatagramLinks,
        Self::PendingTimers,
        Self::TestPeers,
        Self::Tasks,
        Self::PendingHandshakes,
        Self::ActiveLinks,
        Self::NetDbQueries,
        Self::TunnelBuilds,
        Self::Destinations,
        Self::Streams,
        Self::ApiSessions,
    ];

    /// Number of currently defined classes.
    pub const COUNT: usize = Self::ALL.len();
}

/// A positive limit for one resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimit {
    /// Accounted resource class.
    pub class: ResourceClass,
    /// Maximum simultaneously held units.
    pub maximum: u64,
}

impl ResourceLimit {
    /// Creates a resource limit, rejecting zero because it cannot service a request.
    pub const fn new(class: ResourceClass, maximum: u64) -> Result<Self, ResourceError> {
        if maximum == 0 {
            Err(ResourceError::ZeroLimit { class })
        } else {
            Ok(Self { class, maximum })
        }
    }
}

/// A positive resource acquisition request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceRequest {
    /// Accounted resource class.
    pub class: ResourceClass,
    /// Number of units requested.
    pub amount: u64,
}

impl ResourceRequest {
    /// Creates a request, rejecting zero-sized leases.
    pub const fn new(class: ResourceClass, amount: u64) -> Result<Self, ResourceError> {
        if amount == 0 {
            Err(ResourceError::ZeroRequest { class })
        } else {
            Ok(Self { class, amount })
        }
    }
}

/// Current usage and bounded accounting diagnostics for one resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUsage {
    /// Accounted resource class.
    pub class: ResourceClass,
    /// Currently held units.
    pub used: u64,
    /// Configured maximum.
    pub limit: u64,
    /// Highest usage observed since budget creation.
    pub high_water: u64,
    /// Number of denied acquisitions, saturating at `u64::MAX`.
    pub denied: u64,
    /// Number of invalid releases observed by this class, saturating at
    /// `u64::MAX`. A nonzero value records an internal accounting fault.
    pub release_underflow: u64,
}

impl ResourceUsage {
    /// Returns the highest usage observed since budget creation.
    pub const fn high_water_mark(self) -> u64 {
        self.high_water
    }

    /// Returns the number of denied acquisitions.
    pub const fn denied_count(self) -> u64 {
        self.denied
    }

    /// Returns the number of invalid release attempts observed.
    pub const fn release_underflow_count(self) -> u64 {
        self.release_underflow
    }
}

#[derive(Debug, Default)]
struct ClassState {
    limit: u64,
    used: u64,
    high_water: u64,
    denied: u64,
    release_underflow: u64,
}

#[derive(Debug, Default)]
struct BudgetState {
    classes: BTreeMap<ResourceClass, ClassState>,
}

#[derive(Debug)]
struct BudgetInner {
    state: Mutex<BudgetState>,
}

/// Small in-memory budget with immutable limits and owned lease accounting.
#[derive(Clone, Debug)]
pub struct ResourceBudget {
    inner: Arc<BudgetInner>,
}

impl ResourceBudget {
    /// Creates a budget from positive, non-duplicated limits.
    pub fn new(limits: impl IntoIterator<Item = ResourceLimit>) -> Result<Self, ResourceError> {
        let mut state = BudgetState::default();
        for (index, limit) in limits.into_iter().enumerate() {
            if index >= MAX_RESOURCE_CLASSES {
                return Err(ResourceError::TooManyClasses {
                    maximum: MAX_RESOURCE_CLASSES,
                });
            }
            if limit.maximum == 0 {
                return Err(ResourceError::ZeroLimit { class: limit.class });
            }
            if state
                .classes
                .insert(
                    limit.class,
                    ClassState {
                        limit: limit.maximum,
                        ..ClassState::default()
                    },
                )
                .is_some()
            {
                return Err(ResourceError::DuplicateLimit { class: limit.class });
            }
        }
        Ok(Self {
            inner: Arc::new(BudgetInner {
                state: Mutex::new(state),
            }),
        })
    }

    /// Attempts to acquire a bounded lease without exceeding its class limit.
    pub fn try_acquire(&self, request: ResourceRequest) -> Result<ResourceLease, ResourceError> {
        if request.amount == 0 {
            return Err(ResourceError::ZeroRequest {
                class: request.class,
            });
        }
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| ResourceError::Poisoned)?;
        let class_state =
            state
                .classes
                .get_mut(&request.class)
                .ok_or(ResourceError::MissingLimit {
                    class: request.class,
                })?;
        let available = class_state.limit.saturating_sub(class_state.used);
        let next = match class_state.used.checked_add(request.amount) {
            Some(next) if next <= class_state.limit => next,
            Some(_) => {
                class_state.denied = class_state.denied.saturating_add(1);
                return Err(ResourceError::Exhausted {
                    class: request.class,
                    requested: request.amount,
                    available,
                });
            }
            None => {
                class_state.denied = class_state.denied.saturating_add(1);
                return Err(ResourceError::ArithmeticOverflow {
                    class: request.class,
                    used: class_state.used,
                    requested: request.amount,
                });
            }
        };
        class_state.used = next;
        class_state.high_water = class_state.high_water.max(next);
        drop(state);
        Ok(ResourceLease {
            budget: Some(self.clone()),
            class: request.class,
            amount: request.amount,
        })
    }

    /// Attempts to acquire several classes atomically.
    ///
    /// Requests are copied, validated, and ordered by [`ResourceClass`] before
    /// accounting. A duplicate class, missing limit, or exhausted class leaves
    /// every class unchanged. Both owned requests and borrowed slices are
    /// accepted through the [`Borrow`] bound.
    pub fn try_acquire_bundle<I, R>(&self, request_iter: I) -> Result<ResourceBundle, ResourceError>
    where
        I: IntoIterator<Item = R>,
        R: Borrow<ResourceRequest>,
    {
        let mut requests: Vec<ResourceRequest> = Vec::new();
        for request in request_iter {
            if requests.len() >= MAX_RESOURCE_CLASSES {
                return Err(ResourceError::TooManyClasses {
                    maximum: MAX_RESOURCE_CLASSES,
                });
            }
            let request = *request.borrow();
            if request.amount == 0 {
                return Err(ResourceError::ZeroRequest {
                    class: request.class,
                });
            }
            requests.push(request);
        }
        if requests.is_empty() {
            return Err(ResourceError::EmptyBundle);
        }

        requests.sort_unstable_by_key(|request| request.class);
        for pair in requests.windows(2) {
            if pair[0].class == pair[1].class {
                return Err(ResourceError::DuplicateRequest {
                    class: pair[0].class,
                });
            }
        }

        // Allocate the bounded result before mutating accounting. The exact
        // capacity means later pushes cannot fail due to vector growth.
        let mut leases = Vec::with_capacity(requests.len());
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| ResourceError::Poisoned)?;

        for request in &requests {
            if !state.classes.contains_key(&request.class) {
                return Err(ResourceError::MissingLimit {
                    class: request.class,
                });
            }
        }

        let mut next_values = Vec::with_capacity(requests.len());
        for request in &requests {
            let class_state = state
                .classes
                .get(&request.class)
                .expect("bundle limits were validated above");
            let available = class_state.limit.saturating_sub(class_state.used);
            match class_state.used.checked_add(request.amount) {
                Some(next) if next <= class_state.limit => next_values.push(next),
                Some(_) => {
                    let class_state = state
                        .classes
                        .get_mut(&request.class)
                        .expect("bundle limits were validated above");
                    class_state.denied = class_state.denied.saturating_add(1);
                    return Err(ResourceError::Exhausted {
                        class: request.class,
                        requested: request.amount,
                        available,
                    });
                }
                None => {
                    let class_state = state
                        .classes
                        .get_mut(&request.class)
                        .expect("bundle limits were validated above");
                    class_state.denied = class_state.denied.saturating_add(1);
                    return Err(ResourceError::ArithmeticOverflow {
                        class: request.class,
                        used: class_state.used,
                        requested: request.amount,
                    });
                }
            }
        }

        for (request, next) in requests.iter().zip(next_values) {
            let class_state = state
                .classes
                .get_mut(&request.class)
                .expect("bundle limits were validated above");
            class_state.used = next;
            class_state.high_water = class_state.high_water.max(next);
        }
        drop(state);

        for request in requests {
            leases.push(ResourceLease {
                budget: Some(self.clone()),
                class: request.class,
                amount: request.amount,
            });
        }
        Ok(ResourceBundle { leases })
    }

    /// Returns a snapshot for a configured class.
    pub fn usage(&self, class: ResourceClass) -> Result<ResourceUsage, ResourceError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| ResourceError::Poisoned)?;
        let class_state = state
            .classes
            .get(&class)
            .ok_or(ResourceError::MissingLimit { class })?;
        Ok(ResourceUsage {
            class,
            used: class_state.used,
            limit: class_state.limit,
            high_water: class_state.high_water,
            denied: class_state.denied,
            release_underflow: class_state.release_underflow,
        })
    }

    /// Returns a bounded, deterministic snapshot of every configured class.
    pub fn snapshot(&self) -> Result<Vec<ResourceUsage>, ResourceError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| ResourceError::Poisoned)?;
        Ok(state
            .classes
            .iter()
            .map(|(&class, class_state)| ResourceUsage {
                class,
                used: class_state.used,
                limit: class_state.limit,
                high_water: class_state.high_water,
                denied: class_state.denied,
                release_underflow: class_state.release_underflow,
            })
            .collect())
    }

    fn release(&self, class: ResourceClass, amount: u64) {
        // A lease drop is cleanup and must remain best-effort even if another
        // thread panicked while holding the accounting mutex. The governor
        // never exposes its mutable state, so recovering the poisoned guard is
        // preferable to leaking an owned grant during unwinding.
        let mut state = match self.inner.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(class_state) = state.classes.get_mut(&class) {
            if amount <= class_state.used {
                class_state.used -= amount;
            } else {
                class_state.release_underflow = class_state.release_underflow.saturating_add(1);
                class_state.used = 0;
            }
        }
    }

    #[cfg(test)]
    fn release_for_test(&self, class: ResourceClass, amount: u64) {
        self.release(class, amount);
    }
}

/// A group of resource leases admitted atomically.
#[derive(Debug)]
pub struct ResourceBundle {
    leases: Vec<ResourceLease>,
}

impl ResourceBundle {
    /// Number of class grants held by this bundle.
    pub fn len(&self) -> usize {
        self.leases.len()
    }

    /// Whether this bundle contains no class grants.
    pub fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }

    /// Iterates over grants in deterministic [`ResourceClass`] order.
    pub fn iter(&self) -> std::slice::Iter<'_, ResourceLease> {
        self.leases.iter()
    }

    /// Releases every grant by consuming the bundle.
    pub fn release(self) {
        drop(self);
    }
}

/// An owned resource lease that releases its units when dropped.
#[derive(Debug)]
pub struct ResourceLease {
    budget: Option<ResourceBudget>,
    class: ResourceClass,
    amount: u64,
}

impl Drop for ResourceLease {
    fn drop(&mut self) {
        if let Some(budget) = self.budget.take() {
            budget.release(self.class, self.amount);
        }
    }
}

impl ResourceLease {
    /// Resource class held by this lease.
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Number of units held by this lease.
    pub const fn amount(&self) -> u64 {
        self.amount
    }

    /// Releases this lease by consuming it.
    pub fn release(self) {
        drop(self);
    }
}

/// Errors produced by resource limits and lease acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceError {
    /// A configured limit was zero.
    ZeroLimit { class: ResourceClass },
    /// A request asked for zero units.
    ZeroRequest { class: ResourceClass },
    /// The same class was configured more than once.
    DuplicateLimit { class: ResourceClass },
    /// The same class appeared more than once in one atomic bundle.
    DuplicateRequest { class: ResourceClass },
    /// A bundle contained no resource requests.
    EmptyBundle,
    /// A budget or bundle exceeded the bounded class count.
    TooManyClasses { maximum: usize },
    /// No limit was configured for the requested class.
    MissingLimit { class: ResourceClass },
    /// The request would exceed the remaining capacity.
    Exhausted {
        class: ResourceClass,
        requested: u64,
        available: u64,
    },
    /// Adding the request to current usage would overflow `u64`.
    ArithmeticOverflow {
        class: ResourceClass,
        used: u64,
        requested: u64,
    },
    /// The internal lock was poisoned by a prior panic.
    Poisoned,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit { class } => write!(formatter, "zero limit for {class:?}"),
            Self::ZeroRequest { class } => write!(formatter, "zero request for {class:?}"),
            Self::DuplicateLimit { class } => write!(formatter, "duplicate limit for {class:?}"),
            Self::DuplicateRequest { class } => {
                write!(formatter, "duplicate bundle request for {class:?}")
            }
            Self::EmptyBundle => formatter.write_str("resource bundle must not be empty"),
            Self::TooManyClasses { maximum } => {
                write!(
                    formatter,
                    "resource class count exceeds the {maximum}-class limit"
                )
            }
            Self::MissingLimit { class } => write!(formatter, "missing limit for {class:?}"),
            Self::Exhausted {
                class,
                requested,
                available,
            } => write!(
                formatter,
                "resource {class:?} exhausted: requested {requested}, available {available}"
            ),
            Self::ArithmeticOverflow {
                class,
                used,
                requested,
            } => write!(
                formatter,
                "resource {class:?} accounting overflow: used {used}, requested {requested}"
            ),
            Self::Poisoned => formatter.write_str("resource budget lock poisoned"),
        }
    }
}

impl std::error::Error for ResourceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn lifecycle_rejects_recovery_from_stopped() {
        assert!(
            LifecycleState::Stopped
                .transition(LifecycleState::Ready)
                .is_err()
        );
        assert_eq!(
            LifecycleState::Registered
                .transition(LifecycleState::Starting)
                .expect("registered can start"),
            LifecycleState::Starting
        );
    }

    #[test]
    fn health_snapshot_exposes_typed_readiness() {
        let detail = HealthDetail::new("waiting for dependency").expect("bounded detail");
        let snapshot = HealthSnapshot::new(
            HealthState::Degraded(DegradationCode::DependencyUnavailable),
            4,
            Some(detail),
        );
        assert!(snapshot.is_live());
        assert!(!snapshot.is_ready());
        assert_eq!(snapshot.transition_sequence(), 4);
    }

    #[test]
    fn resource_lease_releases_on_drop_and_rejects_overcommit() {
        let limit = ResourceLimit::new(ResourceClass::Tasks, 2).expect("positive limit");
        let budget = ResourceBudget::new([limit]).expect("unique limit");
        let request = ResourceRequest::new(ResourceClass::Tasks, 2).expect("positive request");
        let lease = budget.try_acquire(request).expect("capacity available");
        assert_eq!(budget.usage(ResourceClass::Tasks).expect("usage").used, 2);
        assert!(budget.try_acquire(request).is_err());
        drop(lease);
        assert_eq!(budget.usage(ResourceClass::Tasks).expect("usage").used, 0);
    }

    #[test]
    fn resource_classes_and_snapshots_are_bounded_and_deterministic() {
        assert_eq!(ResourceClass::COUNT, ResourceClass::ALL.len());
        const {
            assert!(ResourceClass::COUNT <= MAX_RESOURCE_CLASSES);
        }

        let limits = ResourceClass::ALL
            .into_iter()
            .map(|class| ResourceLimit::new(class, 1).expect("positive limit"));
        let budget = ResourceBudget::new(limits).expect("all classes fit");
        let snapshot = budget.snapshot().expect("snapshot");

        assert_eq!(snapshot.len(), ResourceClass::COUNT);
        assert!(
            snapshot
                .windows(2)
                .all(|pair| pair[0].class < pair[1].class)
        );
        assert!(snapshot.iter().all(|usage| {
            usage.used == 0
                && usage.limit == 1
                && usage.high_water == 0
                && usage.denied == 0
                && usage.release_underflow == 0
        }));
    }

    #[test]
    fn resource_usage_records_exact_limit_denial_and_high_water() {
        let class = ResourceClass::ServiceTasks;
        let budget =
            ResourceBudget::new([ResourceLimit::new(class, 2).expect("limit")]).expect("budget");
        let exact = ResourceRequest::new(class, 2).expect("request");
        let lease = budget.try_acquire(exact).expect("exact limit is accepted");

        let usage = budget.usage(class).expect("usage");
        assert_eq!(usage.used, 2);
        assert_eq!(usage.high_water_mark(), 2);
        assert_eq!(usage.denied_count(), 0);
        assert_eq!(usage.release_underflow_count(), 0);

        assert!(matches!(
            budget.try_acquire(ResourceRequest::new(class, 1).expect("request")),
            Err(ResourceError::Exhausted {
                class: denied_class,
                requested: 1,
                available: 0,
            }) if denied_class == class
        ));
        drop(lease);

        let usage = budget.usage(class).expect("usage");
        assert_eq!(usage.used, 0);
        assert_eq!(usage.high_water, 2);
        assert_eq!(usage.denied, 1);
        assert_eq!(usage.release_underflow, 0);
    }

    #[test]
    fn resource_validation_rejects_zero_and_handles_u64_overflow() {
        let class = ResourceClass::BufferedBytes;
        assert_eq!(
            ResourceLimit::new(class, 0),
            Err(ResourceError::ZeroLimit { class })
        );
        assert_eq!(
            ResourceRequest::new(class, 0),
            Err(ResourceError::ZeroRequest { class })
        );

        let budget = ResourceBudget::new([ResourceLimit::new(class, u64::MAX).expect("limit")])
            .expect("budget");
        let lease = budget
            .try_acquire(ResourceRequest::new(class, u64::MAX).expect("request"))
            .expect("maximum u64 grant");
        assert_eq!(budget.usage(class).expect("usage").used, u64::MAX);

        assert!(matches!(
            budget.try_acquire(ResourceRequest::new(class, 1).expect("request")),
            Err(ResourceError::ArithmeticOverflow {
                class: overflow_class,
                used: u64::MAX,
                requested: 1,
            }) if overflow_class == class
        ));
        drop(lease);

        let malformed = ResourceRequest { class, amount: 0 };
        assert!(matches!(
            budget.try_acquire(malformed),
            Err(ResourceError::ZeroRequest { class: denied_class }) if denied_class == class
        ));
    }

    #[test]
    fn resource_release_is_consuming_drop_safe_and_unwind_safe() {
        let class = ResourceClass::ChildTasks;
        let budget =
            ResourceBudget::new([ResourceLimit::new(class, 1).expect("limit")]).expect("budget");

        let lease = budget
            .try_acquire(ResourceRequest::new(class, 1).expect("request"))
            .expect("grant");
        lease.release();
        assert_eq!(budget.usage(class).expect("usage").used, 0);

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _lease = budget
                .try_acquire(ResourceRequest::new(class, 1).expect("request"))
                .expect("grant");
            panic!("exercise lease cleanup during unwind");
        }));
        assert!(result.is_err());
        assert_eq!(budget.usage(class).expect("usage").used, 0);
        assert_eq!(
            budget
                .usage(class)
                .expect("usage")
                .release_underflow_count(),
            0
        );
    }

    #[test]
    fn invalid_release_is_visible_without_wrapping_or_panicking() {
        let class = ResourceClass::Tasks;
        let budget =
            ResourceBudget::new([ResourceLimit::new(class, 2).expect("limit")]).expect("budget");
        let lease = budget
            .try_acquire(ResourceRequest::new(class, 1).expect("request"))
            .expect("grant");
        drop(lease);

        budget.release_for_test(class, 2);
        let usage = budget.usage(class).expect("usage");
        assert_eq!(usage.used, 0);
        assert_eq!(usage.release_underflow_count(), 1);
    }

    #[test]
    fn resource_bundle_is_atomic_sorted_and_releases_together() {
        let service = ResourceClass::ServiceTasks;
        let child = ResourceClass::ChildTasks;
        let budget = ResourceBudget::new([
            ResourceLimit::new(service, 2).expect("limit"),
            ResourceLimit::new(child, 1).expect("limit"),
        ])
        .expect("budget");
        let child_request = ResourceRequest::new(child, 1).expect("request");
        let service_request = ResourceRequest::new(service, 2).expect("request");
        let blocker = budget.try_acquire(child_request).expect("child grant");

        assert!(matches!(
            budget.try_acquire_bundle([service_request, child_request]),
            Err(ResourceError::Exhausted {
                class: denied_class,
                requested: 1,
                available: 0,
            }) if denied_class == child
        ));
        assert_eq!(budget.usage(service).expect("usage").used, 0);
        assert_eq!(budget.usage(child).expect("usage").used, 1);
        assert_eq!(budget.usage(child).expect("usage").denied, 1);

        drop(blocker);
        let requests = [child_request, service_request];
        let bundle = budget
            .try_acquire_bundle(requests.as_slice())
            .expect("bundle grant");
        assert_eq!(bundle.len(), 2);
        assert_eq!(
            bundle.iter().map(ResourceLease::class).collect::<Vec<_>>(),
            vec![service, child]
        );
        assert_eq!(budget.usage(service).expect("usage").used, 2);
        assert_eq!(budget.usage(child).expect("usage").used, 1);

        bundle.release();
        assert_eq!(budget.usage(service).expect("usage").used, 0);
        assert_eq!(budget.usage(child).expect("usage").used, 0);
        assert_eq!(budget.usage(service).expect("usage").release_underflow, 0);
        assert_eq!(budget.usage(child).expect("usage").release_underflow, 0);
    }

    #[test]
    fn resource_bundle_rejects_duplicates_without_mutation() {
        let class = ResourceClass::EventQueueItems;
        let budget =
            ResourceBudget::new([ResourceLimit::new(class, 2).expect("limit")]).expect("budget");
        let request = ResourceRequest::new(class, 1).expect("request");

        assert!(matches!(
            budget.try_acquire_bundle([request, request]),
            Err(ResourceError::DuplicateRequest { class: duplicate_class })
                if duplicate_class == class
        ));
        assert_eq!(budget.usage(class).expect("usage").used, 0);
        assert_eq!(budget.usage(class).expect("usage").denied, 0);
        assert!(matches!(
            budget.try_acquire_bundle(std::iter::empty::<ResourceRequest>()),
            Err(ResourceError::EmptyBundle)
        ));
    }

    #[test]
    fn concurrent_acquisition_never_exceeds_the_limit() {
        const THREADS: usize = 16;
        const LIMIT: usize = 4;
        let class = ResourceClass::SimulatedDatagramLinks;
        let budget = Arc::new(
            ResourceBudget::new([ResourceLimit::new(class, LIMIT as u64).expect("limit")])
                .expect("budget"),
        );
        let ready = Arc::new(Barrier::new(THREADS + 1));
        let release = Arc::new(Barrier::new(THREADS + 1));
        let granted = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(THREADS);

        for _ in 0..THREADS {
            let budget = Arc::clone(&budget);
            let ready = Arc::clone(&ready);
            let release = Arc::clone(&release);
            let granted = Arc::clone(&granted);
            handles.push(thread::spawn(move || {
                let lease = budget
                    .try_acquire(ResourceRequest::new(class, 1).expect("request"))
                    .ok();
                if lease.is_some() {
                    granted.fetch_add(1, Ordering::Relaxed);
                }
                ready.wait();
                release.wait();
                drop(lease);
            }));
        }

        ready.wait();
        assert_eq!(granted.load(Ordering::Relaxed), LIMIT);
        assert_eq!(budget.usage(class).expect("usage").used, LIMIT as u64);
        release.wait();
        for handle in handles {
            handle.join().expect("worker joined");
        }
        assert_eq!(budget.usage(class).expect("usage").used, 0);
        assert_eq!(
            budget.usage(class).expect("usage").denied,
            (THREADS - LIMIT) as u64
        );
    }

    #[test]
    fn bounded_types_reject_oversized_values() {
        assert!(ServiceName::new("x".repeat(MAX_SERVICE_NAME_BYTES + 1)).is_err());
        assert!(HealthDetail::new("x".repeat(MAX_HEALTH_DETAIL_BYTES + 1)).is_err());
    }

    #[test]
    fn health_detail_debug_is_redacted() {
        let detail = HealthDetail::new("attacker-controlled diagnostic text").expect("detail");
        let debug = format!("{detail:?}");
        assert!(!debug.contains("attacker-controlled"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn cancellation_is_shared_by_clones() {
        let token = CancellationToken::default();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
    }
}
