//! Service registration and deterministic dependency-graph validation.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use i2pr_core::{MAX_HEALTH_DETAIL_BYTES, ServiceClassification, ServiceFailure, ServiceName};

use crate::context::{ChildFailurePolicy, ServiceContext};

/// Maximum number of services accepted by one graph.
pub const MAX_SERVICE_COUNT: usize = 128;
/// Maximum registration timeout for any one service phase.
pub const MAX_SERVICE_TIMEOUT: Duration = Duration::from_secs(3_600);
/// Bounded default startup deadline.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounded default readiness deadline.
pub const DEFAULT_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounded default graceful service shutdown period.
pub const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// Maximum restart attempts permitted by a policy.
pub const MAX_RESTART_ATTEMPTS: u32 = 32;

/// The concrete future returned by a service factory.
pub type ServiceFuture = Pin<Box<dyn Future<Output = ServiceResult> + Send + 'static>>;

/// Result returned by a service implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceResult {
    /// The service observed its owned cancellation and exited cleanly.
    RequestedShutdown,
    /// The service exited without an owned shutdown request.
    Completed,
    /// The service returned a typed, privacy-safe failure.
    Failed(ServiceFailure),
}

/// Policy applied after a restartable service exhausts its budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartExhaustion {
    /// Keep the router running and publish a degraded snapshot.
    Degrade,
    /// Cancel the graph and fail the router.
    Shutdown,
}

/// A bounded deterministic restart policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestartPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    maximum_delay: Duration,
    reset_after_ready: Option<Duration>,
    exhaustion: RestartExhaustion,
}

impl RestartPolicy {
    /// Creates a policy. `max_attempts` counts replacement attempts.
    pub fn new(
        max_attempts: u32,
        initial_delay: Duration,
        maximum_delay: Duration,
    ) -> Result<Self, RestartPolicyError> {
        if max_attempts == 0 || max_attempts > MAX_RESTART_ATTEMPTS {
            return Err(RestartPolicyError::InvalidAttemptCount {
                maximum: MAX_RESTART_ATTEMPTS,
            });
        }
        if initial_delay.is_zero() {
            return Err(RestartPolicyError::ZeroDelay);
        }
        if maximum_delay < initial_delay {
            return Err(RestartPolicyError::MaximumBeforeInitial);
        }
        if maximum_delay > MAX_SERVICE_TIMEOUT {
            return Err(RestartPolicyError::DelayTooLong {
                maximum: MAX_SERVICE_TIMEOUT,
            });
        }
        Ok(Self {
            max_attempts,
            initial_delay,
            maximum_delay,
            reset_after_ready: None,
            exhaustion: RestartExhaustion::Shutdown,
        })
    }

    /// Sets the sustained-ready interval after which attempts may reset.
    pub fn reset_after_ready(mut self, duration: Duration) -> Result<Self, RestartPolicyError> {
        if duration.is_zero() || duration > MAX_SERVICE_TIMEOUT {
            return Err(RestartPolicyError::DelayTooLong {
                maximum: MAX_SERVICE_TIMEOUT,
            });
        }
        self.reset_after_ready = Some(duration);
        Ok(self)
    }

    /// Selects the bounded behavior after the final replacement attempt.
    pub const fn on_exhaustion(mut self, exhaustion: RestartExhaustion) -> Self {
        self.exhaustion = exhaustion;
        self
    }

    /// Maximum replacement attempts.
    pub const fn max_attempts(self) -> u32 {
        self.max_attempts
    }

    /// Optional sustained-ready reset interval.
    pub const fn reset_interval(self) -> Option<Duration> {
        self.reset_after_ready
    }

    /// Exhaustion behavior.
    pub const fn exhaustion(self) -> RestartExhaustion {
        self.exhaustion
    }

    /// Deterministic exponential backoff for a one-based replacement number.
    pub fn delay_for(self, replacement: u32) -> Duration {
        let shift = replacement.saturating_sub(1).min(31);
        let multiplier = 1_u32 << shift;
        self.initial_delay
            .checked_mul(multiplier)
            .unwrap_or(self.maximum_delay)
            .min(self.maximum_delay)
    }
}

/// Errors produced while constructing a restart policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartPolicyError {
    /// The number of attempts was outside the bounded range.
    InvalidAttemptCount { maximum: u32 },
    /// Zero delay would permit an unbounded hot loop.
    ZeroDelay,
    /// Maximum delay was less than the initial delay.
    MaximumBeforeInitial,
    /// A delay exceeded the shared service timeout maximum.
    DelayTooLong { maximum: Duration },
}

impl std::fmt::Display for RestartPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAttemptCount { maximum } => {
                write!(
                    formatter,
                    "restart attempts must be between 1 and {maximum}"
                )
            }
            Self::ZeroDelay => formatter.write_str("restart delay must be nonzero"),
            Self::MaximumBeforeInitial => {
                formatter.write_str("restart maximum delay must not precede initial delay")
            }
            Self::DelayTooLong { maximum } => {
                write!(formatter, "restart delay exceeds {maximum:?}")
            }
        }
    }
}

impl std::error::Error for RestartPolicyError {}

/// A registered service and its owned task factory.
pub struct ServiceSpec {
    name: ServiceName,
    classification: ServiceClassification,
    dependencies: BTreeSet<ServiceName>,
    startup_timeout: Duration,
    readiness_timeout: Duration,
    shutdown_grace: Duration,
    restart_policy: Option<RestartPolicy>,
    child_failure_policy: ChildFailurePolicy,
    description: Option<&'static str>,
    factory: Arc<dyn Fn(ServiceContext) -> ServiceFuture + Send + Sync>,
}

impl Clone for ServiceSpec {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            classification: self.classification,
            dependencies: self.dependencies.clone(),
            startup_timeout: self.startup_timeout,
            readiness_timeout: self.readiness_timeout,
            shutdown_grace: self.shutdown_grace,
            restart_policy: self.restart_policy,
            child_failure_policy: self.child_failure_policy,
            description: self.description,
            factory: Arc::clone(&self.factory),
        }
    }
}

impl std::fmt::Debug for ServiceSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceSpec")
            .field("name", &self.name)
            .field("classification", &self.classification)
            .field("dependencies", &self.dependencies)
            .field("startup_timeout", &self.startup_timeout)
            .field("readiness_timeout", &self.readiness_timeout)
            .field("shutdown_grace", &self.shutdown_grace)
            .field("restart_policy", &self.restart_policy)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl ServiceSpec {
    /// Registers a concrete async service factory.
    pub fn new<F, Fut>(name: ServiceName, classification: ServiceClassification, factory: F) -> Self
    where
        F: Fn(ServiceContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ServiceResult> + Send + 'static,
    {
        Self {
            name,
            classification,
            dependencies: BTreeSet::new(),
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            readiness_timeout: DEFAULT_READINESS_TIMEOUT,
            shutdown_grace: DEFAULT_SHUTDOWN_GRACE,
            restart_policy: None,
            child_failure_policy: ChildFailurePolicy::FailParent,
            description: None,
            factory: Arc::new(move |context| Box::pin(factory(context))),
        }
    }

    /// Adds a dependency by stable service identifier.
    pub fn depends_on(mut self, dependency: ServiceName) -> Self {
        self.dependencies.insert(dependency);
        self
    }

    /// Sets bounded startup, readiness, and graceful-shutdown deadlines.
    pub fn timeouts(
        mut self,
        startup_timeout: Duration,
        readiness_timeout: Duration,
        shutdown_grace: Duration,
    ) -> Self {
        self.startup_timeout = startup_timeout;
        self.readiness_timeout = readiness_timeout;
        self.shutdown_grace = shutdown_grace;
        self
    }

    /// Sets the explicit restart policy for a restartable service.
    pub fn restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = Some(policy);
        self
    }

    /// Sets the explicit policy for a failed child task.
    pub const fn child_failure_policy(mut self, policy: ChildFailurePolicy) -> Self {
        self.child_failure_policy = policy;
        self
    }

    /// Adds a static description for bounded diagnostics.
    pub fn description(mut self, description: &'static str) -> Self {
        self.description = Some(description);
        self
    }

    pub(crate) fn name(&self) -> &ServiceName {
        &self.name
    }

    pub(crate) const fn classification(&self) -> ServiceClassification {
        self.classification
    }

    pub(crate) fn dependencies(&self) -> &BTreeSet<ServiceName> {
        &self.dependencies
    }

    pub(crate) const fn startup_timeout(&self) -> Duration {
        self.startup_timeout
    }

    pub(crate) const fn readiness_timeout(&self) -> Duration {
        self.readiness_timeout
    }

    pub(crate) const fn shutdown_grace(&self) -> Duration {
        self.shutdown_grace
    }

    pub(crate) const fn restart_config(&self) -> Option<RestartPolicy> {
        self.restart_policy
    }

    pub(crate) const fn description_text(&self) -> Option<&'static str> {
        self.description
    }

    pub(crate) const fn child_policy(&self) -> ChildFailurePolicy {
        self.child_failure_policy
    }

    pub(crate) fn factory(&self) -> Arc<dyn Fn(ServiceContext) -> ServiceFuture + Send + Sync> {
        Arc::clone(&self.factory)
    }
}

/// Errors that prevent any service task from starting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphError {
    /// The configured router-wide maximum was invalid.
    InvalidMaximum { maximum: usize },
    /// A service identifier was registered more than once.
    DuplicateService { name: ServiceName },
    /// The graph exceeded its explicit service-count maximum.
    TooManyServices { maximum: usize },
    /// The caller required an essential service but registered none.
    MissingEssentialService,
    /// A declared dependency was not registered.
    MissingDependency {
        service: ServiceName,
        dependency: ServiceName,
    },
    /// A service depended on itself.
    SelfDependency { service: ServiceName },
    /// The graph contained a dependency cycle.
    DependencyCycle { services: Vec<ServiceName> },
    /// A timeout was zero or above the shared maximum.
    InvalidTimeout {
        service: ServiceName,
        field: &'static str,
    },
    /// A restart policy was missing or invalid for the classification.
    InvalidRestartPolicy {
        service: ServiceName,
        classification: ServiceClassification,
    },
    /// Static diagnostic text exceeded the health-detail bound.
    DescriptionTooLong { service: ServiceName },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMaximum { maximum } => {
                write!(formatter, "invalid service maximum {maximum}")
            }
            Self::DuplicateService { name } => write!(formatter, "duplicate service {name}"),
            Self::TooManyServices { maximum } => {
                write!(formatter, "service graph exceeds {maximum} services")
            }
            Self::MissingEssentialService => {
                formatter.write_str("service graph has no essential service")
            }
            Self::MissingDependency {
                service,
                dependency,
            } => {
                write!(
                    formatter,
                    "service {service} depends on missing {dependency}"
                )
            }
            Self::SelfDependency { service } => {
                write!(formatter, "service {service} depends on itself")
            }
            Self::DependencyCycle { services } => {
                write!(formatter, "service dependency cycle involving {services:?}")
            }
            Self::InvalidTimeout { service, field } => {
                write!(formatter, "service {service} has invalid {field} timeout")
            }
            Self::InvalidRestartPolicy {
                service,
                classification,
            } => {
                write!(
                    formatter,
                    "service {service} has invalid restart policy for {classification:?}"
                )
            }
            Self::DescriptionTooLong { service } => {
                write!(formatter, "service {service} description is too long")
            }
        }
    }
}

impl std::error::Error for GraphError {}

/// Builder for a fully validated service graph.
#[derive(Debug)]
pub struct ServiceGraphBuilder {
    maximum: usize,
    require_essential: bool,
    services: BTreeMap<ServiceName, ServiceSpec>,
}

impl ServiceGraphBuilder {
    /// Creates a builder with an explicit router-wide service maximum.
    pub fn new(maximum: usize) -> Result<Self, GraphError> {
        if maximum == 0 || maximum > MAX_SERVICE_COUNT {
            return Err(GraphError::InvalidMaximum { maximum });
        }
        Ok(Self {
            maximum,
            require_essential: true,
            services: BTreeMap::new(),
        })
    }

    /// Controls whether at least one essential service is required.
    pub const fn require_essential(mut self, required: bool) -> Self {
        self.require_essential = required;
        self
    }

    /// Registers one service before graph validation.
    pub fn register(&mut self, service: ServiceSpec) -> Result<(), GraphError> {
        let name = service.name.clone();
        if self.services.contains_key(&name) {
            return Err(GraphError::DuplicateService { name });
        }
        if self.services.len() >= self.maximum {
            return Err(GraphError::TooManyServices {
                maximum: self.maximum,
            });
        }
        self.services.insert(name, service);
        Ok(())
    }

    /// Validates and freezes the graph without starting a task.
    pub fn build(self) -> Result<ServiceGraph, GraphError> {
        if self.require_essential
            && !self
                .services
                .values()
                .any(|service| service.classification == ServiceClassification::Essential)
        {
            return Err(GraphError::MissingEssentialService);
        }

        for service in self.services.values() {
            for dependency in &service.dependencies {
                if dependency == &service.name {
                    return Err(GraphError::SelfDependency {
                        service: service.name.clone(),
                    });
                }
                if !self.services.contains_key(dependency) {
                    return Err(GraphError::MissingDependency {
                        service: service.name.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
            for (field, timeout) in [
                ("startup", service.startup_timeout),
                ("readiness", service.readiness_timeout),
                ("shutdown", service.shutdown_grace),
            ] {
                if timeout.is_zero() || timeout > MAX_SERVICE_TIMEOUT {
                    return Err(GraphError::InvalidTimeout {
                        service: service.name.clone(),
                        field,
                    });
                }
            }
            if service
                .description
                .is_some_and(|description| description.len() > MAX_HEALTH_DETAIL_BYTES)
            {
                return Err(GraphError::DescriptionTooLong {
                    service: service.name.clone(),
                });
            }
            let policy_required = service.classification == ServiceClassification::Restartable;
            if policy_required != service.restart_policy.is_some()
                || (service.classification != ServiceClassification::Restartable
                    && service.restart_policy.is_some())
            {
                return Err(GraphError::InvalidRestartPolicy {
                    service: service.name.clone(),
                    classification: service.classification,
                });
            }
        }

        let mut indegree: BTreeMap<ServiceName, usize> = self
            .services
            .iter()
            .map(|(name, service)| (name.clone(), service.dependencies.len()))
            .collect();
        let mut ready = indegree
            .iter()
            .filter_map(|(name, count)| (*count == 0).then_some(name.clone()))
            .collect::<BTreeSet<_>>();
        let mut startup_order = Vec::with_capacity(self.services.len());
        while let Some(name) = ready.pop_first() {
            startup_order.push(name.clone());
            for service in self.services.values() {
                if service.dependencies.contains(&name) {
                    let count = indegree
                        .get_mut(service.name())
                        .expect("all registered services have indegree entries");
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(service.name.clone());
                    }
                }
            }
        }
        if startup_order.len() != self.services.len() {
            let services = indegree
                .into_iter()
                .filter_map(|(name, count)| (count != 0).then_some(name))
                .collect();
            return Err(GraphError::DependencyCycle { services });
        }

        Ok(ServiceGraph {
            services: self.services,
            startup_order,
            require_essential: self.require_essential,
        })
    }
}

/// An immutable, deterministically ordered service graph.
pub struct ServiceGraph {
    services: BTreeMap<ServiceName, ServiceSpec>,
    startup_order: Vec<ServiceName>,
    require_essential: bool,
}

impl std::fmt::Debug for ServiceGraph {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceGraph")
            .field("services", &self.services.keys().collect::<Vec<_>>())
            .field("startup_order", &self.startup_order)
            .field("require_essential", &self.require_essential)
            .finish()
    }
}

impl ServiceGraph {
    /// Starts a graph builder with an explicit service-count bound.
    pub fn builder(maximum: usize) -> Result<ServiceGraphBuilder, GraphError> {
        ServiceGraphBuilder::new(maximum)
    }

    /// Returns the deterministic dependency-first startup order.
    pub fn startup_order(&self) -> &[ServiceName] {
        &self.startup_order
    }

    pub(crate) fn service(&self, name: &ServiceName) -> &ServiceSpec {
        self.services
            .get(name)
            .expect("validated startup order references registered services")
    }

    pub(crate) fn services(&self) -> &BTreeMap<ServiceName, ServiceSpec> {
        &self.services
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> ServiceName {
        ServiceName::new(value).expect("valid name")
    }

    fn service(value: &str, classification: ServiceClassification) -> ServiceSpec {
        ServiceSpec::new(name(value), classification, |_context| async {
            ServiceResult::Completed
        })
    }

    #[test]
    fn topological_order_is_lexically_deterministic() {
        let mut builder = ServiceGraph::builder(8).expect("bound");
        builder
            .register(service("zeta", ServiceClassification::Optional))
            .expect("register");
        builder
            .register(service("essential", ServiceClassification::Essential))
            .expect("register");
        builder
            .register(
                service("middle", ServiceClassification::Optional).depends_on(name("essential")),
            )
            .expect("register");
        builder
            .register(service("leaf", ServiceClassification::Optional).depends_on(name("middle")))
            .expect("register");
        let graph = builder.build().expect("valid graph");
        let order = graph
            .startup_order()
            .iter()
            .map(ServiceName::as_str)
            .collect::<Vec<_>>();
        assert_eq!(order, ["essential", "middle", "leaf", "zeta"]);
    }

    #[test]
    fn invalid_graphs_are_rejected_before_startup() {
        let mut builder = ServiceGraph::builder(4).expect("bound");
        builder
            .register(service("essential", ServiceClassification::Essential))
            .expect("register");
        builder
            .register(
                service("dependent", ServiceClassification::Optional).depends_on(name("missing")),
            )
            .expect("register");
        assert!(matches!(
            builder.build(),
            Err(GraphError::MissingDependency { .. })
        ));
    }

    #[test]
    fn restartable_services_require_a_policy() {
        let mut builder = ServiceGraph::builder(2).expect("bound");
        builder
            .register(service("essential", ServiceClassification::Essential))
            .expect("register");
        builder
            .register(service("worker", ServiceClassification::Restartable))
            .expect("register");
        assert!(matches!(
            builder.build(),
            Err(GraphError::InvalidRestartPolicy { .. })
        ));
    }
}
