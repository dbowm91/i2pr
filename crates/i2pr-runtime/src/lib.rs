//! Concrete Tokio-backed supervision for the non-networked router runtime.
//!
//! `i2pr-runtime` is the only production crate in this milestone that owns
//! Tokio tasks, timers, channels, or wakeable cancellation. Protocol, crypto,
//! storage, and runtime-neutral core crates remain free of runtime coupling.

#![forbid(unsafe_code)]

mod cancel;
mod channel;
mod context;
mod graph;
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
pub use supervisor::{
    MAX_SHUTDOWN_DEADLINE, ShutdownOutcome, ShutdownReport, Supervisor, SupervisorConfigError,
    SupervisorError, SupervisorHandle,
};

pub use i2pr_core::{
    CancellationReason, DegradationCode, FailureCategory, HealthDetail, HealthSnapshot,
    HealthState, InvalidLifecycleTransition, LifecycleState, ServiceClassification,
    ServiceCompletion, ServiceFailure, ServiceFailureCategory, ServiceName, ServiceNameError,
    ShutdownReason,
};
