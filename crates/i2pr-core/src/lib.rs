//! Runtime-neutral contracts shared by the future router services.
//!
//! This crate owns small lifecycle, health, cancellation, and resource-domain
//! types.  It does not own a runtime, configuration parsing, filesystem state,
//! network transports, protocol codecs, or router composition.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    /// The service exists but has not started.
    Created,
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
                (Self::Created, Self::Starting | Self::Stopping)
                    | (
                        Self::Starting,
                        Self::Ready | Self::Degraded | Self::Stopping | Self::Failed
                    )
                    | (Self::Ready, Self::Degraded | Self::Stopping | Self::Failed)
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthDetail(String);

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
    state: HealthState,
    transition_sequence: u64,
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
            state,
            transition_sequence,
            detail,
        }
    }

    /// Current typed state.
    pub const fn state(&self) -> HealthState {
        self.state
    }

    /// Monotonic transition sequence supplied by the owning service.
    pub const fn transition_sequence(&self) -> u64 {
        self.transition_sequence
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

/// Resource categories reserved for router-wide accounting.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceClass {
    /// Supervised task count.
    Tasks,
    /// Bytes retained in bounded buffers.
    BufferedBytes,
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

/// Current usage for one resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUsage {
    /// Accounted resource class.
    pub class: ResourceClass,
    /// Currently held units.
    pub used: u64,
    /// Configured maximum.
    pub limit: u64,
}

#[derive(Debug, Default)]
struct BudgetState {
    limits: BTreeMap<ResourceClass, u64>,
    used: BTreeMap<ResourceClass, u64>,
}

#[derive(Debug)]
struct BudgetInner {
    state: Mutex<BudgetState>,
}

/// Small in-memory budget that provides tested release-on-drop semantics.
#[derive(Clone, Debug)]
pub struct ResourceBudget {
    inner: Arc<BudgetInner>,
}

impl ResourceBudget {
    /// Creates a budget from positive, non-duplicated limits.
    pub fn new(limits: impl IntoIterator<Item = ResourceLimit>) -> Result<Self, ResourceError> {
        let mut state = BudgetState::default();
        for limit in limits {
            if limit.maximum == 0 {
                return Err(ResourceError::ZeroLimit { class: limit.class });
            }
            if state.limits.insert(limit.class, limit.maximum).is_some() {
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
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| ResourceError::Poisoned)?;
        let limit =
            state
                .limits
                .get(&request.class)
                .copied()
                .ok_or(ResourceError::MissingLimit {
                    class: request.class,
                })?;
        let used = state.used.get(&request.class).copied().unwrap_or(0);
        let next = used
            .checked_add(request.amount)
            .ok_or(ResourceError::Exhausted {
                class: request.class,
                requested: request.amount,
                available: limit.saturating_sub(used),
            })?;
        if next > limit {
            return Err(ResourceError::Exhausted {
                class: request.class,
                requested: request.amount,
                available: limit.saturating_sub(used),
            });
        }
        state.used.insert(request.class, next);
        drop(state);
        Ok(ResourceLease {
            budget: self.clone(),
            class: request.class,
            amount: request.amount,
        })
    }

    /// Returns a snapshot for a configured class.
    pub fn usage(&self, class: ResourceClass) -> Result<ResourceUsage, ResourceError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| ResourceError::Poisoned)?;
        let limit = state
            .limits
            .get(&class)
            .copied()
            .ok_or(ResourceError::MissingLimit { class })?;
        Ok(ResourceUsage {
            class,
            used: state.used.get(&class).copied().unwrap_or(0),
            limit,
        })
    }

    fn release(&self, class: ResourceClass, amount: u64) {
        if let Ok(mut state) = self.inner.state.lock() {
            if let Some(used) = state.used.get_mut(&class) {
                if *used >= amount {
                    *used -= amount;
                }
                if *used == 0 {
                    state.used.remove(&class);
                }
            }
        }
    }
}

/// An owned resource lease that releases its units when dropped.
#[derive(Debug)]
pub struct ResourceLease {
    budget: ResourceBudget,
    class: ResourceClass,
    amount: u64,
}

impl Drop for ResourceLease {
    fn drop(&mut self) {
        self.budget.release(self.class, self.amount);
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
    /// No limit was configured for the requested class.
    MissingLimit { class: ResourceClass },
    /// The request would exceed the remaining capacity.
    Exhausted {
        class: ResourceClass,
        requested: u64,
        available: u64,
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
            Self::MissingLimit { class } => write!(formatter, "missing limit for {class:?}"),
            Self::Exhausted {
                class,
                requested,
                available,
            } => write!(
                formatter,
                "resource {class:?} exhausted: requested {requested}, available {available}"
            ),
            Self::Poisoned => formatter.write_str("resource budget lock poisoned"),
        }
    }
}

impl std::error::Error for ResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_rejects_recovery_from_stopped() {
        assert!(
            LifecycleState::Stopped
                .transition(LifecycleState::Ready)
                .is_err()
        );
        assert_eq!(
            LifecycleState::Created
                .transition(LifecycleState::Starting)
                .expect("created can start"),
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
    fn bounded_types_reject_oversized_values() {
        assert!(ServiceName::new("x".repeat(MAX_SERVICE_NAME_BYTES + 1)).is_err());
        assert!(HealthDetail::new("x".repeat(MAX_HEALTH_DETAIL_BYTES + 1)).is_err());
    }

    #[test]
    fn cancellation_is_shared_by_clones() {
        let token = CancellationToken::default();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
    }
}
