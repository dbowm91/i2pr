//! Concrete Tokio-backed supervision for the non-networked router runtime.
//!
//! `i2pr-runtime` is the only production crate in this milestone that owns
//! Tokio tasks, timers, channels, or wakeable cancellation. Protocol, crypto,
//! storage, and runtime-neutral core crates remain free of runtime coupling.
//! It also exposes privacy-aware aggregate snapshots and fixed-name tracing
//! conventions; it never installs a global subscriber.

#![forbid(unsafe_code)]

use std::future::Future;
use std::time::Duration;

mod cancel;
mod channel;
mod context;
mod graph;
mod ntcp2_driver;
mod ntcp2_link;
mod ntcp2_runtime;
mod observability;
mod supervisor;

pub use cancel::CancellationToken;
pub use channel::{
    ChannelConfigError, ChannelName, ChannelNameError, ChannelSnapshot, ChannelSpec,
    CommunicationClass, EventReceiver, EventSendError, EventSender, LatestState,
    LatestStateReceiver, LatestStateSender, MAX_CHANNEL_CAPACITY, MAX_CHANNEL_NAME_BYTES,
    MAX_QUEUE_ITEM_BYTES, OverflowPolicy, QueueCharge, ReceiveError, Received, ReceivedRequest,
    RequestChannelParts, RequestError, RequestReceiver, RequestSender, SendError, StateUpdateError,
    TryReceiveError, command_channel, event_channel, latest_state_channel, request_channel,
};
pub use context::{
    ChildFailurePolicy, ChildScope, ChildScopeError, ChildShutdownReport, ChildTaskFailure,
    HealthReceiver, HealthReporter, MAX_CHILD_TASKS, Readiness, ReadinessError, ServiceContext,
};
pub use graph::{
    GraphError, MAX_RESTART_ATTEMPTS, MAX_SERVICE_COUNT, MAX_SERVICE_TIMEOUT, RestartExhaustion,
    RestartPolicy, RestartPolicyError, ServiceFuture, ServiceGraph, ServiceGraphBuilder,
    ServiceResult, ServiceSpec,
};
pub use ntcp2_driver::{
    HandshakeClock, HandshakeDriverConfig, HandshakeDriverError, HandshakeRun, PaddingProfile,
    drive_initiator_handshake, drive_responder_handshake,
};
pub use ntcp2_link::{
    AuthenticatedLink, AuthenticatedLinkError, AuthenticatedLinkSnapshot,
    AuthenticatedLinkStartError, ReceivedFrameLease,
};
pub use ntcp2_runtime::{
    ActiveLinkAdmission, ActiveLinkAdmissionError, ActiveLinkPermit, ActiveLinkSnapshot,
    AddressFamily, AdmissionDenied, AdmissionRejection, AdmissionSnapshot, AdmittedInboundStream,
    AuthenticatedInboundStream, BoundNtcp2Listener, DialAdmission, DialAttempt, DialBackoffConfig,
    DialBackoffDecision, DialBackoffSnapshot, DialKey, DialKeyError, DialOutcome, ExactIoError,
    InboundAdmission, InboundChunk, InboundPermit, IoErrorKind, IpPrefixPolicy, LinkHandle, LinkId,
    LinkSendError, LinkSnapshot, LinkStartError, LinkTermination, ListenerHandle, ListenerSnapshot,
    Ntcp2Deadline, Ntcp2DeadlineError, Ntcp2Event, Ntcp2EventKind, Ntcp2RuntimeConfig,
    Ntcp2RuntimeConfigError, Ntcp2RuntimeDeadlines, Ntcp2RuntimeLimits, Ntcp2RuntimeService,
    ReplayCache, ReplayCacheDecision, ReplayCacheSnapshot, RuntimeLimitKind, WriteOutcome,
    read_exact, write_all_exact,
};
pub use observability::{
    MAX_SNAPSHOT_CHANNELS, MAX_SNAPSHOT_RESOURCES, RouterLifecycle, RuntimeSnapshot,
    ServiceSnapshot, SimulationSnapshot, SnapshotError, SupervisorSnapshot, event,
};
pub use supervisor::{
    MAX_SHUTDOWN_DEADLINE, ShutdownOutcome, ShutdownReport, Supervisor, SupervisorConfigError,
    SupervisorError, SupervisorHandle,
};

/// Runs one runtime-owned future on a disposable current-thread Tokio runtime.
///
/// This helper is for non-production composition roots such as the isolated
/// interoperability launcher. Production daemon startup remains responsible
/// for selecting and owning its process runtime.
pub fn run_blocking<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime construction is a local invariant")
        .block_on(future)
}

/// Runs one future with a bounded runtime-owned timeout.
pub async fn bounded_timeout<F>(duration: Duration, future: F) -> Result<F::Output, ()>
where
    F: Future,
{
    tokio::time::timeout(duration, future).await.map_err(|_| ())
}

pub use i2pr_core::{
    CancellationReason, DegradationCode, FailureCategory, HealthDetail, HealthSnapshot,
    HealthState, InvalidLifecycleTransition, LifecycleState, ServiceClassification,
    ServiceCompletion, ServiceFailure, ServiceFailureCategory, ServiceName, ServiceNameError,
    ShutdownReason,
};
