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
mod ntcp2_data_oracle;
mod ntcp2_driver;
mod ntcp2_handshake_observer;
mod ntcp2_link;
mod ntcp2_runtime;
mod observability;
mod ssu2_runtime;
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
pub use ntcp2_data_oracle::{
    BOUNDED_STEP_BUDGET, DataOracleError, DataOracleError as DataOracle, MatchedTarget,
    ORACLE_MAX_BLOCKS, ORACLE_MAX_FRAMES, ORACLE_MAX_NON_TARGET_I2NP, ORACLE_MAX_PLAINTEXT_BYTES,
    ORACLE_SCHEMA, OracleAccept, OracleConfig, OracleCounters, PeerRouterHashBinding,
    bounded_deadline_step, drop_lease as drop_received_frame_lease,
    receive_correlated_delivery_status,
};
pub use ntcp2_driver::{
    HandshakeClock, HandshakeCounterSnapshot, HandshakeDriverConfig, HandshakeDriverError,
    HandshakeRun, HandshakeRunOutcome, PaddingProfile, drive_initiator_handshake,
    drive_initiator_handshake_observed, drive_responder_handshake,
    drive_responder_handshake_observed,
};
pub use ntcp2_handshake_observer::{
    HandshakeIoResult, HandshakeProgressObserver, HandshakeStageObservation, NoopHandshakeObserver,
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
pub use ssu2_runtime::{
    MAX_SSU2_ACTIVE_CEILING, MAX_SSU2_INBOUND_QUEUE_CEILING, MAX_SSU2_PENDING_CEILING,
    MAX_SSU2_RUNTIME_DURATION, MAX_SSU2_STAGED_BYTES_CEILING, MAX_SSU2_STAGED_DATAGRAMS_CEILING,
    SSU2_BACKOFF_ENTRIES, SSU2_CACHED_TOKEN_GRACE, SSU2_CONFIRMED_MTU_PAYLOAD, SSU2_DEFAULT_MTU,
    SSU2_LINKS_PER_PEER, SSU2_MANAGER_BYTES_PER_LINK, SSU2_MANAGER_MESSAGES_PER_LINK,
    SSU2_MAX_DRAIN_PER_SESSION, SSU2_MAX_PATH_CANDIDATES_GLOBAL, SSU2_SEND_QUEUE_BYTES,
    SSU2_SEND_QUEUE_MESSAGES, SSU2_TOKEN_CACHE_PEERS, SSU2_TOKEN_CACHE_PER_PEER,
    SSU2_TOKEN_REQUEST_SOURCES, SSU2_TOKEN_REQUESTS_PER_SECOND, Ssu2BindError, Ssu2DialOutcome,
    Ssu2DialTarget, Ssu2DialTargetError, Ssu2EstablishedLink, Ssu2IdentityMaterial,
    Ssu2InboundI2np, Ssu2LimitKind, Ssu2LinkHandle, Ssu2RuntimeConfig, Ssu2RuntimeConfigError,
    Ssu2RuntimeDeadlines, Ssu2RuntimeLimits, Ssu2RuntimeService, Ssu2SendOutcome,
    Ssu2ServiceHandle, Ssu2Snapshot, Ssu2SocketConfig, Ssu2TestFaults,
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
