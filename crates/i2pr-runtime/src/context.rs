//! Runtime-owned service context, readiness, health, and child-task scopes.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::FutureExt;
use i2pr_core::{
    CancellationReason, DegradationCode, FailureCategory, HealthDetail, HealthSnapshot,
    HealthState, LifecycleState, ServiceClassification, ServiceName,
};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::task::JoinSet;

use crate::cancel::CancellationToken;
use crate::observability::TaskCounters;

/// Maximum child tasks owned by one service scope.
pub const MAX_CHILD_TASKS: usize = 64;

/// A Tokio monotonic clock anchored when a supervisor is created.
#[derive(Debug)]
pub(crate) struct RuntimeClock {
    origin: tokio::time::Instant,
}

impl RuntimeClock {
    pub(crate) fn new() -> Self {
        Self {
            origin: tokio::time::Instant::now(),
        }
    }

    pub(crate) fn now(&self) -> Duration {
        tokio::time::Instant::now().saturating_duration_since(self.origin)
    }
}

/// Error returned when readiness cannot be signalled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessError {
    /// The service instance already signalled readiness.
    Duplicate,
    /// The supervisor no longer owns a readiness receiver.
    Closed,
}

impl std::fmt::Display for ReadinessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate => formatter.write_str("readiness was already signalled"),
            Self::Closed => formatter.write_str("readiness receiver is closed"),
        }
    }
}

impl std::error::Error for ReadinessError {}

/// One-shot readiness signal for one service instance.
#[derive(Clone, Debug)]
pub struct Readiness {
    sent: Arc<AtomicBool>,
    sender: watch::Sender<bool>,
}

impl Readiness {
    pub(crate) fn new() -> (Self, watch::Receiver<bool>) {
        let (sender, receiver) = watch::channel(false);
        (
            Self {
                sent: Arc::new(AtomicBool::new(false)),
                sender,
            },
            receiver,
        )
    }

    /// Signals readiness exactly once for this service instance.
    pub fn signal_ready(&self) -> Result<(), ReadinessError> {
        if self
            .sent
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ReadinessError::Duplicate);
        }
        self.sender.send(true).map_err(|_| ReadinessError::Closed)
    }

    /// Alias used by service implementations.
    pub fn ready(&self) -> Result<(), ReadinessError> {
        self.signal_ready()
    }

    pub(crate) fn is_signalled(&self) -> bool {
        self.sent.load(Ordering::Acquire)
    }

    pub(crate) async fn wait(receiver: &mut watch::Receiver<bool>) -> Result<(), ReadinessError> {
        if *receiver.borrow() {
            return Ok(());
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return Ok(());
            }
        }
        Err(ReadinessError::Closed)
    }
}

/// A latest-state health subscription. No unbounded event history is retained.
#[derive(Debug)]
pub struct HealthReceiver {
    receiver: watch::Receiver<HealthSnapshot>,
}

impl HealthReceiver {
    /// Returns the latest snapshot without waiting.
    pub fn snapshot(&self) -> HealthSnapshot {
        self.receiver.borrow().clone()
    }

    /// Waits for the snapshot to change.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.receiver.changed().await
    }
}

#[derive(Debug)]
struct SharedHealthState {
    service: ServiceName,
    classification: ServiceClassification,
    lifecycle: LifecycleState,
    health: HealthState,
    restart_count: u32,
    last_failure: Option<FailureCategory>,
    transition_sequence: u64,
    detail: Option<HealthDetail>,
    sender: watch::Sender<HealthSnapshot>,
    clock: Arc<RuntimeClock>,
}

/// Shared latest-state health storage used by a supervisor and its service.
#[derive(Debug)]
pub(crate) struct SharedHealth {
    state: Mutex<SharedHealthState>,
}

impl SharedHealth {
    pub(crate) fn new(
        service: ServiceName,
        classification: ServiceClassification,
        description: Option<&'static str>,
        clock: Arc<RuntimeClock>,
    ) -> Arc<Self> {
        let detail = description.and_then(|value| HealthDetail::new(value).ok());
        let initial = HealthSnapshot::for_service(
            service.clone(),
            classification,
            LifecycleState::Registered,
            HealthState::Starting,
            0,
            None,
            0,
            clock.now(),
            detail.clone(),
        );
        let (sender, _) = watch::channel(initial);
        Arc::new(Self {
            state: Mutex::new(SharedHealthState {
                service,
                classification,
                lifecycle: LifecycleState::Registered,
                health: HealthState::Starting,
                restart_count: 0,
                last_failure: None,
                transition_sequence: 0,
                detail,
                sender,
                clock,
            }),
        })
    }

    pub(crate) fn receiver(self: &Arc<Self>) -> HealthReceiver {
        let receiver = self
            .state
            .lock()
            .expect("health mutex is not poisoned")
            .sender
            .subscribe();
        HealthReceiver { receiver }
    }

    pub(crate) fn reporter(self: &Arc<Self>) -> HealthReporter {
        HealthReporter {
            shared: Arc::clone(self),
        }
    }

    pub(crate) fn set(
        &self,
        lifecycle: LifecycleState,
        health: HealthState,
        restart_count: u32,
        last_failure: Option<FailureCategory>,
        detail: Option<HealthDetail>,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.lifecycle = lifecycle;
        state.health = health;
        state.restart_count = restart_count;
        state.last_failure = last_failure;
        state.detail = detail;
        state.transition_sequence = state.transition_sequence.saturating_add(1);
        let snapshot = HealthSnapshot::for_service(
            state.service.clone(),
            state.classification,
            state.lifecycle,
            state.health,
            state.restart_count,
            state.last_failure,
            state.transition_sequence,
            state.clock.now(),
            state.detail.clone(),
        );
        // `send` leaves a watch channel unchanged when there are no
        // subscribers. `send_replace` keeps the latest-state snapshot valid
        // for direct supervisor inspection as well as subscribers.
        let _ = state.sender.send_replace(snapshot);
    }

    pub(crate) fn report(&self, health: HealthState, detail: Option<HealthDetail>) {
        let lifecycle = match health {
            HealthState::Starting => LifecycleState::Starting,
            HealthState::Ready => LifecycleState::Ready,
            HealthState::Degraded(_) => LifecycleState::Degraded,
            HealthState::Stopping => LifecycleState::Stopping,
            HealthState::Failed => LifecycleState::Failed,
        };
        self.set(
            lifecycle,
            health,
            self.snapshot().restart_count(),
            self.snapshot().last_failure(),
            detail,
        );
    }

    pub(crate) fn snapshot(&self) -> HealthSnapshot {
        self.state
            .lock()
            .expect("health mutex is not poisoned")
            .sender
            .borrow()
            .clone()
    }
}

/// A service-owned health publisher.
#[derive(Clone, Debug)]
pub struct HealthReporter {
    shared: Arc<SharedHealth>,
}

impl HealthReporter {
    /// Publishes the latest bounded health state.
    pub fn report(&self, state: HealthState, detail: Option<HealthDetail>) {
        self.shared.report(state, detail);
    }

    /// Publishes readiness with optional bounded context.
    pub fn ready(&self, detail: Option<HealthDetail>) {
        self.report(HealthState::Ready, detail);
    }

    /// Publishes a typed degraded state.
    pub fn degraded(&self, code: DegradationCode, detail: Option<HealthDetail>) {
        self.report(HealthState::Degraded(code), detail);
    }
}

/// Policy for a child-task failure within a service scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildFailurePolicy {
    /// A failed child makes the owning service fail.
    FailParent,
    /// A failed child marks the owning service degraded.
    DegradeParent,
    /// The owning service collects the child result explicitly.
    CollectResult,
}

/// Static child-task failure categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildTaskFailure {
    /// The child reported a bounded failure.
    Explicit,
    /// The child panicked; its payload is not retained.
    Panic,
}

/// Errors returned when a child cannot be registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildScopeError {
    /// The scope has begun shutdown and accepts no new work.
    Closed,
    /// The scope reached its bounded child-task limit.
    TooManyTasks { maximum: usize },
}

impl std::fmt::Display for ChildScopeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("child scope is closed"),
            Self::TooManyTasks { maximum } => {
                write!(formatter, "child scope exceeds {maximum} tasks")
            }
        }
    }
}

impl std::error::Error for ChildScopeError {}

#[derive(Debug)]
struct ChildTaskOutput(Result<(), ChildTaskFailure>);

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ChildShutdownReport {
    failed: bool,
    joined: usize,
    remaining: usize,
    forced: bool,
}

impl ChildShutdownReport {
    /// Returns whether any child failed, panicked, or failed to join.
    pub const fn failed(&self) -> bool {
        self.failed
    }

    /// Returns the number of child tasks joined by the scope.
    pub const fn joined(&self) -> usize {
        self.joined
    }

    /// Returns the number of child tasks that could not be joined.
    pub const fn remaining(&self) -> usize {
        self.remaining
    }

    /// Returns whether the scope used forced abort before joining.
    pub const fn was_forced(&self) -> bool {
        self.forced
    }
}

struct ChildScopeInner {
    closed: AtomicBool,
    cancellation: CancellationToken,
    tasks: AsyncMutex<Option<JoinSet<ChildTaskOutput>>>,
    counters: Arc<TaskCounters>,
}

impl std::fmt::Debug for ChildScopeInner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChildScopeInner")
            .field("closed", &self.closed.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl Drop for ChildScopeInner {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut tasks) = self.tasks.try_lock() {
            if let Some(tasks) = tasks.as_mut() {
                tasks.abort_all();
            }
        }
    }
}

/// Owned scope for concurrent work created by one service.
#[derive(Clone, Debug)]
pub struct ChildScope {
    inner: Arc<ChildScopeInner>,
    policy: ChildFailurePolicy,
}

impl ChildScope {
    pub(crate) fn new(
        parent: &CancellationToken,
        policy: ChildFailurePolicy,
        counters: Arc<TaskCounters>,
    ) -> Self {
        Self {
            inner: Arc::new(ChildScopeInner {
                closed: AtomicBool::new(false),
                cancellation: parent.child_token(),
                tasks: AsyncMutex::new(Some(JoinSet::new())),
                counters,
            }),
            policy,
        }
    }

    /// Spawns a checked child future under this service's cancellation scope.
    pub fn spawn<F, Fut>(&self, factory: F) -> Result<(), ChildScopeError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ChildTaskFailure>> + Send + 'static,
    {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(ChildScopeError::Closed);
        }
        let mut tasks = self
            .inner
            .tasks
            .try_lock()
            .map_err(|_| ChildScopeError::Closed)?;
        let set = tasks.as_mut().ok_or(ChildScopeError::Closed)?;
        if set.len() >= MAX_CHILD_TASKS {
            return Err(ChildScopeError::TooManyTasks {
                maximum: MAX_CHILD_TASKS,
            });
        }
        let cancellation = self.inner.cancellation.clone();
        self.inner.counters.child_started();
        set.spawn(async move {
            let child = cancellation.child_token();
            let result = std::panic::AssertUnwindSafe(async move { factory(child).await })
                .catch_unwind()
                .await;
            ChildTaskOutput(match result {
                Ok(result) => result,
                Err(_) => Err(ChildTaskFailure::Panic),
            })
        });
        Ok(())
    }

    /// Requests cancellation and joins every child. The report is bounded.
    pub async fn shutdown(&self) -> ChildShutdownReport {
        self.inner.closed.store(true, Ordering::Release);
        let _ = self
            .inner
            .cancellation
            .cancel(CancellationReason::ParentScope);
        let mut tasks = self.inner.tasks.lock().await;
        let Some(set) = tasks.as_mut() else {
            return ChildShutdownReport::default();
        };
        let mut report = ChildShutdownReport::default();
        while let Some(result) = set.join_next().await {
            self.record_join(&mut report, result);
        }
        *tasks = None;
        report
    }

    /// Aborts and drains this exact child collection after its manager was
    /// forcibly stopped. The bounded poll budget prevents a non-cooperative
    /// child from extending supervisor shutdown indefinitely; any remaining
    /// handles stay accounted and are reported as cleanup failure.
    pub(crate) async fn force_shutdown(&self) -> ChildShutdownReport {
        self.inner.closed.store(true, Ordering::Release);
        let _ = self
            .inner
            .cancellation
            .cancel(CancellationReason::ShutdownDeadline);
        let mut tasks = self.inner.tasks.lock().await;
        let Some(set) = tasks.as_mut() else {
            return ChildShutdownReport {
                forced: true,
                ..ChildShutdownReport::default()
            };
        };
        set.abort_all();
        let mut report = ChildShutdownReport {
            forced: true,
            ..ChildShutdownReport::default()
        };
        for _ in 0..=MAX_CHILD_TASKS {
            if set.is_empty() {
                break;
            }
            tokio::select! {
                biased;
                result = set.join_next() => {
                    if let Some(result) = result {
                        self.record_join(&mut report, result);
                    }
                }
                _ = tokio::task::yield_now() => {}
            }
        }
        report.remaining = set.len();
        if report.remaining == 0 {
            *tasks = None;
        }
        report
    }

    fn record_join(
        &self,
        report: &mut ChildShutdownReport,
        result: Result<ChildTaskOutput, tokio::task::JoinError>,
    ) {
        report.joined += 1;
        self.inner.counters.child_finished();
        match result {
            Ok(ChildTaskOutput(Err(_))) | Err(_) => report.failed = true,
            Ok(ChildTaskOutput(Ok(()))) => {}
        }
    }

    /// Returns whether a child failure should fail the parent.
    pub const fn policy(&self) -> ChildFailurePolicy {
        self.policy
    }
}

/// The narrow capability bundle passed to one service instance.
#[derive(Clone, Debug)]
pub struct ServiceContext {
    name: ServiceName,
    cancellation: CancellationToken,
    readiness: Readiness,
    health: HealthReporter,
    children: ChildScope,
}

impl ServiceContext {
    pub(crate) fn new(
        name: ServiceName,
        cancellation: CancellationToken,
        readiness: Readiness,
        health: HealthReporter,
        children: ChildScope,
    ) -> Self {
        Self {
            name,
            cancellation,
            readiness,
            health,
            children,
        }
    }

    /// Stable identifier of this service instance.
    pub fn name(&self) -> &ServiceName {
        &self.name
    }

    /// Returns this service's wakeable cancellation scope.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Returns this instance's one-shot readiness signal.
    pub fn readiness(&self) -> Readiness {
        self.readiness.clone()
    }

    /// Signals readiness for this service instance.
    pub fn signal_ready(&self) -> Result<(), ReadinessError> {
        self.readiness.signal_ready()
    }

    /// Returns the latest-state health publisher.
    pub fn health(&self) -> HealthReporter {
        self.health.clone()
    }

    /// Returns the owned child-task scope.
    pub fn children(&self) -> ChildScope {
        self.children.clone()
    }
}
