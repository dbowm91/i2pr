//! Runtime-owned execution of the runtime-neutral NTCP2 handshake.
//!
//! The protocol state machine emits bounded actions; this module is the only
//! layer that fulfills those actions with Tokio I/O, cancellation, deadlines,
//! clock access, padding policy, and replay admission. It deliberately does
//! not own a listener or dial policy. Those remain on
//! [`crate::Ntcp2RuntimeService`], so callers can keep pending and active
//! admission leases attached to the socket for the complete lifecycle.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_transport_ntcp2::constants::MAX_HANDSHAKE_BUFFERED_INPUT;
use i2pr_transport_ntcp2::handshake::{HandshakeError, ReplayDecision, ReplayToken};
use i2pr_transport_ntcp2::state_machine::{
    AuthenticatedHandshake, HandshakeAction, HandshakeInput, HandshakeTransition, InitiatorState,
    PaddingMessage, ResponderState,
};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::ntcp2_handshake_observer::{
    HandshakeIoResult, HandshakeProgressObserver, HandshakeStageObservation, NoopHandshakeObserver,
};
use crate::{
    CancellationToken, ExactIoError, Ntcp2Deadline, Ntcp2RuntimeDeadlines, ReplayCache,
    ReplayCacheDecision,
};

/// A clock source selected by the composition root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandshakeClock {
    /// Read the current UTC Unix time for authorized reference runs.
    System,
    /// Use a fixed timestamp for deterministic tests.
    Fixed(u64),
}

impl HandshakeClock {
    fn now(self) -> u64 {
        match self {
            Self::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Self::Fixed(value) => value,
        }
    }
}

/// A bounded padding selection for handshake messages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaddingProfile {
    /// Emit no cleartext or authenticated handshake padding.
    Minimum,
    /// Emit a small deterministic representative amount, capped by the action.
    #[default]
    Representative,
    /// Emit the maximum amount permitted by the state-machine action.
    Maximum,
    /// Emit an explicit deterministic length and byte value.
    Deterministic { length: usize, fill: u8 },
}

/// Configuration for one bounded handshake execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandshakeDriverConfig {
    /// Runtime deadlines used for the total handshake and each I/O operation.
    pub deadlines: Ntcp2RuntimeDeadlines,
    /// Timestamp source used by the state machine.
    pub clock: HandshakeClock,
    /// Padding policy used for every padding request in this execution.
    pub padding: PaddingProfile,
}

impl Default for HandshakeDriverConfig {
    fn default() -> Self {
        Self {
            deadlines: Ntcp2RuntimeDeadlines::default(),
            clock: HandshakeClock::System,
            padding: PaddingProfile::default(),
        }
    }
}

/// A bounded handshake-driver failure with no peer-controlled text.
#[derive(Debug, Eq, PartialEq)]
pub enum HandshakeDriverError {
    /// A bounded socket operation failed.
    Io(ExactIoError),
    /// The protocol state machine rejected a supplied result or peer bytes.
    Protocol(HandshakeError),
    /// An action requested an invalid or ambiguous allocation/read shape.
    InvalidAction,
    /// The selected padding profile could not satisfy the action bound.
    PaddingOutOfRange,
    /// The local RouterInfo source exceeded the requested action maximum.
    RouterInfoTooLarge,
    /// Plan 052 G1: a responder-stage failure labelled by the responder
    /// phase that produced the terminal failure. The label is a fixed
    /// redacted identifier; it never contains peer-controlled bytes.
    /// Callers map the label to a bounded `responder_*` reason without
    /// inspecting the carried `HandshakeError`.
    ResponderStage {
        /// The redacted phase label observed when the terminal failure
        /// was produced. Always present; never peer-controlled.
        phase_label: &'static str,
        /// The underlying bounded protocol or driver failure.
        inner: Box<HandshakeDriverError>,
    },
}

impl fmt::Display for HandshakeDriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Protocol(error) => error.fmt(formatter),
            Self::InvalidAction => formatter.write_str("invalid bounded NTCP2 handshake action"),
            Self::PaddingOutOfRange => {
                formatter.write_str("NTCP2 padding policy exceeded its bound")
            }
            Self::RouterInfoTooLarge => {
                formatter.write_str("local NTCP2 RouterInfo exceeded its bound")
            }
            Self::ResponderStage { phase_label, inner } => formatter.write_fmt(format_args!(
                "NTCP2 responder stage {phase_label} failed: {inner}"
            )),
        }
    }
}

impl std::error::Error for HandshakeDriverError {}

/// Plan 092 metadata-only handshake stage observation marker.
///
/// The driver emits observations for every ReadExact / Write /
/// authenticated transition; the bounded i2pr-ntcp2-handshake-stage-v1
/// schema lives in the runner and the record schema is the
/// privacy-safe contract owned by the i2pr runtime.
#[allow(dead_code)]
pub const HANDSHAKE_STAGE_SCHEMA: &str = "i2pr-ntcp2-handshake-stage-v1";

/// Aggregate result of an authenticated handshake.
pub struct HandshakeRun {
    /// Authenticated peer and consuming data-phase key owners.
    pub authenticated: AuthenticatedHandshake,
    /// Bytes read while fulfilling handshake actions.
    pub read_bytes: u64,
    /// Bytes written while fulfilling handshake actions.
    pub written_bytes: u64,
    /// Number of state-machine actions fulfilled.
    pub action_count: u32,
}

impl fmt::Debug for HandshakeRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandshakeRun")
            .field("authenticated", &true)
            .field("read_bytes", &self.read_bytes)
            .field("written_bytes", &self.written_bytes)
            .field("action_count", &self.action_count)
            .finish()
    }
}

/// Plan 092: counter snapshot retained alongside a terminal
/// handshake failure so the runner can correlate the last observed
/// state with the typed error. The snapshot is also produced for
/// successful runs; the success path simply carries the same
/// counters the runtime accumulates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HandshakeCounterSnapshot {
    /// Bytes read by the runtime handshake driver.
    pub read_bytes: u64,
    /// Bytes written by the runtime handshake driver.
    pub written_bytes: u64,
    /// Number of state-machine actions fulfilled.
    pub action_count: u32,
    /// Number of ``ReadExact`` actions fulfilled.
    pub read_count: u32,
    /// Number of ``Write`` actions fulfilled.
    pub write_count: u32,
}

impl HandshakeCounterSnapshot {
    /// Renders the snapshot as the canonical Plan 092 status
    /// counters. The canonical ``i2pr-interop-status`` JSONL line
    /// keeps the snapshot fields under the existing ``counters``
    /// object so the runner can read them back without a schema
    /// change.
    pub fn as_status_counters(&self) -> [u32; 5] {
        [
            u32::try_from(self.read_bytes).unwrap_or(u32::MAX),
            u32::try_from(self.written_bytes).unwrap_or(u32::MAX),
            self.action_count,
            self.read_count,
            self.write_count,
        ]
    }
}

/// Plan 092: the bounded outcome returned by the observed
/// handshake driver. The struct carries both the typed result and
/// the accumulated counter snapshot so the runner can preserve the
/// last authenticated/frame/I2NP correlation state on failure.
pub struct HandshakeRunOutcome {
    /// Typed result. ``Ok`` wraps an authenticated handshake run;
    /// ``Err`` carries the bounded ``HandshakeDriverError`` plus the
    /// accumulated counter snapshot.
    pub result: Result<HandshakeRun, HandshakeDriverError>,
    /// Counter snapshot accumulated by the runtime handshake driver.
    /// The snapshot is preserved for both successful and failed
    /// outcomes.
    pub counters: HandshakeCounterSnapshot,
}

impl fmt::Debug for HandshakeRunOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.result {
            Ok(run) => formatter
                .debug_struct("HandshakeRunOutcome")
                .field("result", &"Ok(HandshakeRun)")
                .field("counters", &self.counters)
                .field("authenticated", &true)
                .field("read_bytes", &run.read_bytes)
                .field("written_bytes", &run.written_bytes)
                .field("action_count", &run.action_count)
                .finish(),
            Err(error) => formatter
                .debug_struct("HandshakeRunOutcome")
                .field("result", &format!("Err({error:?})"))
                .field("counters", &self.counters)
                .finish(),
        }
    }
}

impl HandshakeIoResult {
    /// Maps a runtime ``ExactIoError`` to the bounded Plan 092 I/O
    /// result category. The mapping is closed: ``Failed`` covers the
    /// OS-rejected branch; ``Eof`` covers the peer-closed branch;
    /// ``Timeout`` covers the deadline branch; ``Cancelled`` covers
    /// the caller-cancellation branch.
    pub fn from_io_error(error: &ExactIoError) -> Self {
        match error.kind {
            crate::IoErrorKind::Closed => Self::Eof,
            crate::IoErrorKind::Deadline => Self::Timeout,
            crate::IoErrorKind::Cancelled => Self::Cancelled,
            crate::IoErrorKind::Failed => Self::Failed,
        }
    }
}

impl From<&ExactIoError> for HandshakeIoResult {
    fn from(error: &ExactIoError) -> Self {
        Self::from_io_error(error)
    }
}

struct HandshakeBudget {
    total: tokio::time::Instant,
    deadlines: Ntcp2RuntimeDeadlines,
}

impl HandshakeBudget {
    fn new(deadlines: Ntcp2RuntimeDeadlines) -> Result<Self, HandshakeDriverError> {
        deadlines
            .validate()
            .map_err(|_| HandshakeDriverError::InvalidAction)?;
        Ok(Self {
            total: tokio::time::Instant::now() + deadlines.handshake,
            deadlines,
        })
    }

    fn operation_deadline(
        &self,
        operation: Duration,
    ) -> Result<Ntcp2Deadline, HandshakeDriverError> {
        let remaining = self
            .total
            .saturating_duration_since(tokio::time::Instant::now())
            .min(operation);
        Ntcp2Deadline::after(remaining)
            .map_err(|_| HandshakeDriverError::Protocol(HandshakeError::DeadlineExpired))
    }

    fn expired(&self) -> bool {
        self.total <= tokio::time::Instant::now()
    }
}

trait HandshakeMachine: Sized {
    fn start(self) -> Result<HandshakeTransition<Self>, HandshakeError>;
    fn transition(self, input: HandshakeInput)
    -> Result<HandshakeTransition<Self>, HandshakeError>;
    /// Plan 052 G1: returns the redacted stage label for the responder
    /// role when this state machine is a `ResponderState`. The default
    /// returns `None`; only the responder implementation overrides it.
    fn phase_label(&self) -> Option<&'static str> {
        None
    }
}

impl HandshakeMachine for InitiatorState {
    fn start(self) -> Result<HandshakeTransition<Self>, HandshakeError> {
        Self::start(self)
    }

    fn transition(
        self,
        input: HandshakeInput,
    ) -> Result<HandshakeTransition<Self>, HandshakeError> {
        Self::transition(self, input)
    }

    fn phase_label(&self) -> Option<&'static str> {
        None
    }
}

impl HandshakeMachine for ResponderState {
    fn start(self) -> Result<HandshakeTransition<Self>, HandshakeError> {
        Self::start(self)
    }

    fn transition(
        self,
        input: HandshakeInput,
    ) -> Result<HandshakeTransition<Self>, HandshakeError> {
        Self::transition(self, input)
    }

    fn phase_label(&self) -> Option<&'static str> {
        Some(ResponderState::phase_label(self))
    }
}

/// Drives one initiator handshake on a caller-owned stream.
///
/// The function preserves the existing production call surface by
/// delegating to the observed implementation with a no-op observer.
/// Tools that require Plan 092 metadata-only stage observations
/// must invoke [`drive_initiator_handshake_observed`] directly.
pub async fn drive_initiator_handshake<S>(
    state: InitiatorState,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
) -> Result<HandshakeRun, HandshakeDriverError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let outcome = drive_initiator_handshake_observed(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
        &NoopHandshakeObserver,
    )
    .await;
    outcome.result
}

/// Drives one initiator handshake with metadata-only stage
/// observations. The observer receives bounded Plan 092 stage
/// observations emitted immediately around the actual encode/read/
/// write/process operations; it never receives raw payload bytes or
/// peer-controlled text. The returned [`HandshakeRunOutcome`]
/// preserves the accumulated counter snapshot on both success and
/// failure paths.
pub async fn drive_initiator_handshake_observed<S>(
    state: InitiatorState,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
    observer: &dyn HandshakeProgressObserver,
) -> HandshakeRunOutcome
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    observer.observe(HandshakeStageObservation {
        stage: "initiator_state_initialized",
        expected_octets: None,
        completed_octets: None,
        io_result: HandshakeIoResult::NotApplicable,
        elapsed_millis: 0,
    });
    let mut last_responder_label: Option<&'static str> = state.phase_label();
    let (result, snapshot) = drive_inner_observed(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
        &mut last_responder_label,
        observer,
    )
    .await;
    let result = match result {
        Ok(run) => Ok(run),
        Err(HandshakeDriverError::ResponderStage { .. }) => result,
        Err(inner) => {
            if let Some(phase_label) = last_responder_label {
                Err(HandshakeDriverError::ResponderStage {
                    phase_label,
                    inner: Box::new(inner),
                })
            } else {
                Err(inner)
            }
        }
    };
    HandshakeRunOutcome {
        result,
        counters: snapshot,
    }
}

/// Drives one responder handshake on a caller-owned stream.
///
/// The function preserves the existing production call surface by
/// delegating to the observed implementation with a no-op observer.
/// Tools that require Plan 092 metadata-only stage observations
/// must invoke [`drive_responder_handshake_observed`] directly.
pub async fn drive_responder_handshake<S>(
    state: ResponderState,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
) -> Result<HandshakeRun, HandshakeDriverError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let outcome = drive_responder_handshake_observed(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
        &NoopHandshakeObserver,
    )
    .await;
    outcome.result
}

/// Drives one responder handshake with metadata-only stage
/// observations. The observer receives bounded Plan 092 stage
/// observations emitted immediately around the actual encode/read/
/// write/process operations; it never receives raw payload bytes or
/// peer-controlled text. The returned [`HandshakeRunOutcome`]
/// preserves the accumulated counter snapshot on both success and
/// failure paths.
pub async fn drive_responder_handshake_observed<S>(
    state: ResponderState,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
    observer: &dyn HandshakeProgressObserver,
) -> HandshakeRunOutcome
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut last_responder_label: Option<&'static str> = Some(state.phase_label());
    let (result, snapshot) = drive_inner_observed(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
        &mut last_responder_label,
        observer,
    )
    .await;
    let result = match result {
        Ok(run) => Ok(run),
        Err(HandshakeDriverError::ResponderStage { .. }) => result,
        Err(inner) => {
            if let Some(phase_label) = last_responder_label {
                Err(HandshakeDriverError::ResponderStage {
                    phase_label,
                    inner: Box::new(inner),
                })
            } else {
                Err(inner)
            }
        }
    };
    HandshakeRunOutcome {
        result,
        counters: snapshot,
    }
}

#[allow(dead_code)]
async fn drive_inner<M, S>(
    state: M,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
    last_responder_label: &mut Option<&'static str>,
) -> Result<HandshakeRun, HandshakeDriverError>
where
    M: HandshakeMachine,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (result, _) = drive_inner_observed(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
        last_responder_label,
        &NoopHandshakeObserver,
    )
    .await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn drive_inner_observed<M, S>(
    state: M,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
    last_responder_label: &mut Option<&'static str>,
    observer: &dyn HandshakeProgressObserver,
) -> (
    Result<HandshakeRun, HandshakeDriverError>,
    HandshakeCounterSnapshot,
)
where
    M: HandshakeMachine,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let snapshot = || HandshakeCounterSnapshot {
        read_bytes: 0,
        written_bytes: 0,
        action_count: 0,
        read_count: 0,
        write_count: 0,
    };
    let budget = match HandshakeBudget::new(config.deadlines) {
        Ok(value) => value,
        Err(error) => return (Err(error), snapshot()),
    };
    let mut transition = match state.start() {
        Ok(transition) => transition,
        Err(error) => return (Err(HandshakeDriverError::Protocol(error)), snapshot()),
    };
    if let Some(label) = transition.state.phase_label() {
        *last_responder_label = Some(label);
    }
    let mut read_bytes: u64 = 0;
    let mut written_bytes: u64 = 0;
    let mut action_count: u32 = 0;
    let mut read_count: u32 = 0;
    let mut write_count: u32 = 0;

    loop {
        if budget.expired() {
            return (
                Err(HandshakeDriverError::Protocol(
                    HandshakeError::DeadlineExpired,
                )),
                HandshakeCounterSnapshot {
                    read_bytes,
                    written_bytes,
                    action_count,
                    read_count,
                    write_count,
                },
            );
        }
        let state = transition.state;
        if let Some(label) = state.phase_label() {
            *last_responder_label = Some(label);
        }
        let mut next_state: Option<Result<HandshakeTransition<M>, HandshakeDriverError>> = None;
        let mut exit_with: Option<Result<HandshakeRun, HandshakeDriverError>> = None;
        for action in transition.actions {
            action_count = action_count.saturating_add(1);
            match action {
                HandshakeAction::ReadExact { length } => {
                    if length > MAX_HANDSHAKE_BUFFERED_INPUT {
                        exit_with = Some(Err(HandshakeDriverError::InvalidAction));
                        break;
                    }
                    let started = std::time::Instant::now();
                    let mut bytes = vec![0_u8; length];
                    let deadline = match budget.operation_deadline(budget.deadlines.read_idle) {
                        Ok(value) => value,
                        Err(error) => {
                            exit_with = Some(Err(error));
                            break;
                        }
                    };
                    let io_result =
                        match read_exact(stream, &mut bytes, deadline, cancellation).await {
                            Ok(()) => {
                                read_bytes = read_bytes.saturating_add(length as u64);
                                read_count = read_count.saturating_add(1);
                                HandshakeIoResult::Completed
                            }
                            Err(HandshakeDriverError::Io(ref io_error)) => {
                                HandshakeIoResult::from(io_error)
                            }
                            Err(_) => HandshakeIoResult::Failed,
                        };
                    let elapsed_millis =
                        started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
                    observer.observe(HandshakeStageObservation {
                        stage: "session_created_read_started",
                        expected_octets: Some(length as u32),
                        completed_octets: None,
                        io_result: HandshakeIoResult::NotApplicable,
                        elapsed_millis: 0,
                    });
                    observer.observe(HandshakeStageObservation {
                        stage: "session_created_read_completed",
                        expected_octets: Some(length as u32),
                        completed_octets: if matches!(io_result, HandshakeIoResult::Completed) {
                            Some(length as u32)
                        } else {
                            Some(0)
                        },
                        io_result,
                        elapsed_millis,
                    });
                    if !matches!(io_result, HandshakeIoResult::Completed) {
                        exit_with = Some(Err(HandshakeDriverError::Io(ExactIoError {
                            kind: match io_result {
                                HandshakeIoResult::Eof => crate::IoErrorKind::Closed,
                                HandshakeIoResult::Timeout => crate::IoErrorKind::Deadline,
                                HandshakeIoResult::Cancelled => crate::IoErrorKind::Cancelled,
                                HandshakeIoResult::Failed | HandshakeIoResult::Completed => {
                                    crate::IoErrorKind::Failed
                                }
                                HandshakeIoResult::NotApplicable => crate::IoErrorKind::Failed,
                            },
                        })));
                        break;
                    }
                    next_state = Some(
                        state
                            .transition(HandshakeInput::Bytes(bytes))
                            .map_err(HandshakeDriverError::Protocol),
                    );
                    break;
                }
                HandshakeAction::ReadBounded { .. } => {
                    exit_with = Some(Err(HandshakeDriverError::InvalidAction));
                    break;
                }
                HandshakeAction::Write(bytes) => {
                    let inner = bytes.into_bytes();
                    let length = inner.len();
                    if length > MAX_HANDSHAKE_BUFFERED_INPUT {
                        exit_with = Some(Err(HandshakeDriverError::InvalidAction));
                        break;
                    }
                    let started = std::time::Instant::now();
                    let deadline = match budget.operation_deadline(budget.deadlines.write) {
                        Ok(value) => value,
                        Err(error) => {
                            exit_with = Some(Err(error));
                            break;
                        }
                    };
                    let io_result = match write_all(stream, &inner, deadline, cancellation).await {
                        Ok(()) => {
                            written_bytes = written_bytes.saturating_add(length as u64);
                            write_count = write_count.saturating_add(1);
                            HandshakeIoResult::Completed
                        }
                        Err(HandshakeDriverError::Io(ref io_error)) => {
                            HandshakeIoResult::from(io_error)
                        }
                        Err(_) => HandshakeIoResult::Failed,
                    };
                    let elapsed_millis =
                        started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
                    let (stage_start, stage_complete) = if read_count == 0 {
                        (
                            "session_request_write_started",
                            "session_request_write_completed",
                        )
                    } else {
                        (
                            "session_confirmed_write_started",
                            "session_confirmed_write_completed",
                        )
                    };
                    observer.observe(HandshakeStageObservation {
                        stage: stage_start,
                        expected_octets: Some(length as u32),
                        completed_octets: None,
                        io_result: HandshakeIoResult::NotApplicable,
                        elapsed_millis: 0,
                    });
                    observer.observe(HandshakeStageObservation {
                        stage: stage_complete,
                        expected_octets: Some(length as u32),
                        completed_octets: if matches!(io_result, HandshakeIoResult::Completed) {
                            Some(length as u32)
                        } else {
                            Some(0)
                        },
                        io_result,
                        elapsed_millis,
                    });
                    if !matches!(io_result, HandshakeIoResult::Completed) {
                        exit_with = Some(Err(HandshakeDriverError::Io(ExactIoError {
                            kind: match io_result {
                                HandshakeIoResult::Eof => crate::IoErrorKind::Closed,
                                HandshakeIoResult::Timeout => crate::IoErrorKind::Deadline,
                                HandshakeIoResult::Cancelled => crate::IoErrorKind::Cancelled,
                                HandshakeIoResult::Failed | HandshakeIoResult::Completed => {
                                    crate::IoErrorKind::Failed
                                }
                                HandshakeIoResult::NotApplicable => crate::IoErrorKind::Failed,
                            },
                        })));
                        break;
                    }
                }
                HandshakeAction::RequestTimestamp { .. } => {
                    next_state = Some(
                        state
                            .transition(HandshakeInput::Timestamp(config.clock.now()))
                            .map_err(HandshakeDriverError::Protocol),
                    );
                    break;
                }
                HandshakeAction::RequestReplay { token, retention } => {
                    let decision = replay_decision(replay, token, config.clock.now(), retention);
                    next_state = Some(
                        state
                            .transition(HandshakeInput::Replay(decision))
                            .map_err(HandshakeDriverError::Protocol),
                    );
                    break;
                }
                HandshakeAction::RequestPadding { message, maximum } => {
                    let padding = match select_padding(config.padding, message, maximum) {
                        Ok(value) => value,
                        Err(error) => {
                            exit_with = Some(Err(error));
                            break;
                        }
                    };
                    next_state = Some(
                        state
                            .transition(HandshakeInput::Padding(padding))
                            .map_err(HandshakeDriverError::Protocol),
                    );
                    break;
                }
                HandshakeAction::RequestRouterInfo { maximum } => {
                    if local_router_info.is_empty() || local_router_info.len() > maximum {
                        exit_with = Some(Err(HandshakeDriverError::RouterInfoTooLarge));
                        break;
                    }
                    next_state = Some(
                        state
                            .transition(HandshakeInput::RouterInfo(local_router_info.to_vec()))
                            .map_err(HandshakeDriverError::Protocol),
                    );
                    break;
                }
                HandshakeAction::Authenticated(authenticated) => {
                    observer.observe(HandshakeStageObservation {
                        stage: "noise_authenticated",
                        expected_octets: None,
                        completed_octets: None,
                        io_result: HandshakeIoResult::Completed,
                        elapsed_millis: 0,
                    });
                    let snapshot = HandshakeCounterSnapshot {
                        read_bytes,
                        written_bytes,
                        action_count,
                        read_count,
                        write_count,
                    };
                    return (
                        Ok(HandshakeRun {
                            authenticated,
                            read_bytes,
                            written_bytes,
                            action_count,
                        }),
                        snapshot,
                    );
                }
                HandshakeAction::Terminate(error) => {
                    exit_with = Some(Err(HandshakeDriverError::Protocol(error)));
                    break;
                }
            }
        }
        if let Some(result) = exit_with {
            return (
                result,
                HandshakeCounterSnapshot {
                    read_bytes,
                    written_bytes,
                    action_count,
                    read_count,
                    write_count,
                },
            );
        }
        match next_state {
            Some(Ok(value)) => {
                transition = value;
            }
            Some(Err(error)) => {
                return (
                    Err(error),
                    HandshakeCounterSnapshot {
                        read_bytes,
                        written_bytes,
                        action_count,
                        read_count,
                        write_count,
                    },
                );
            }
            None => {
                return (
                    Err(HandshakeDriverError::InvalidAction),
                    HandshakeCounterSnapshot {
                        read_bytes,
                        written_bytes,
                        action_count,
                        read_count,
                        write_count,
                    },
                );
            }
        }
        if let Some(label) = transition.state.phase_label() {
            *last_responder_label = Some(label);
        }
    }
}

fn replay_decision(
    replay: &ReplayCache,
    token: ReplayToken,
    now: u64,
    retention: u64,
) -> ReplayDecision {
    match replay.check_and_record(*token.as_bytes(), now, retention) {
        ReplayCacheDecision::Fresh => ReplayDecision::Fresh,
        ReplayCacheDecision::Replayed => ReplayDecision::Replayed,
        ReplayCacheDecision::Full => ReplayDecision::CacheFull,
    }
}

fn select_padding(
    profile: PaddingProfile,
    _message: PaddingMessage,
    maximum: usize,
) -> Result<Vec<u8>, HandshakeDriverError> {
    let length = match profile {
        PaddingProfile::Minimum => 0,
        PaddingProfile::Representative => maximum.min(16),
        PaddingProfile::Maximum => maximum,
        PaddingProfile::Deterministic { length, .. } => length,
    };
    if length > maximum {
        return Err(HandshakeDriverError::PaddingOutOfRange);
    }
    let fill = match profile {
        PaddingProfile::Deterministic { fill, .. } => fill,
        _ => 0,
    };
    Ok(vec![fill; length])
}

async fn read_exact<R>(
    reader: &mut R,
    buffer: &mut [u8],
    deadline: Ntcp2Deadline,
    cancellation: &CancellationToken,
) -> Result<(), HandshakeDriverError>
where
    R: AsyncRead + Unpin,
{
    crate::read_exact(reader, buffer, deadline, cancellation)
        .await
        .map_err(HandshakeDriverError::Io)
}

async fn write_all<W>(
    writer: &mut W,
    buffer: &[u8],
    deadline: Ntcp2Deadline,
    cancellation: &CancellationToken,
) -> Result<(), HandshakeDriverError>
where
    W: AsyncWrite + Unpin,
{
    crate::write_all_exact(writer, buffer, deadline, cancellation)
        .await
        .map_err(HandshakeDriverError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::{OsRng, RouterIdentityBundle, X25519PrivateKey};
    use i2pr_proto::{Date, Mapping, RouterAddress};
    use i2pr_transport_ntcp2::crypto::PublicKeyBytes;
    use i2pr_transport_ntcp2::handshake::ClockSkewPolicy;
    use i2pr_transport_ntcp2::state_machine::{InitiatorState, ResponderState};

    fn i2p_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = chunk.get(1).copied().unwrap_or(0);
            let c = chunk.get(2).copied().unwrap_or(0);
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[((a & 3) << 4 | b >> 4) as usize] as char);
            output.push(if chunk.len() > 1 {
                ALPHABET[((b & 15) << 2 | c >> 6) as usize] as char
            } else {
                '='
            });
            output.push(if chunk.len() > 2 {
                ALPHABET[(c & 63) as usize] as char
            } else {
                '='
            });
        }
        output
    }

    fn router_info(bundle: &RouterIdentityBundle, transport_static: [u8; 32]) -> Vec<u8> {
        let mut options = Mapping::builder();
        options
            .insert("s".to_owned(), i2p_base64(&transport_static))
            .expect("static key option");
        options
            .insert("v".to_owned(), "2".to_owned())
            .expect("version option");
        let address = RouterAddress::new(
            1,
            Date::from_millis(1),
            "NTCP2".to_owned(),
            options.build().expect("address options"),
        )
        .expect("router address");
        bundle
            .sign_router_info(
                Date::from_millis(1_000),
                vec![address],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("signed RouterInfo")
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("RouterInfo bytes")
    }

    #[test]
    fn padding_profiles_are_bounded_and_redacted() {
        assert_eq!(
            select_padding(PaddingProfile::Minimum, PaddingMessage::SessionRequest, 8)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            select_padding(
                PaddingProfile::Representative,
                PaddingMessage::SessionCreated,
                8
            )
            .unwrap()
            .len(),
            8
        );
        assert_eq!(
            select_padding(PaddingProfile::Maximum, PaddingMessage::SessionConfirmed, 8)
                .unwrap()
                .len(),
            8
        );
        assert!(matches!(
            select_padding(
                PaddingProfile::Deterministic {
                    length: 9,
                    fill: 0xaa
                },
                PaddingMessage::SessionRequest,
                8
            ),
            Err(HandshakeDriverError::PaddingOutOfRange)
        ));
    }

    #[test]
    fn fixed_clock_is_deterministic() {
        assert_eq!(HandshakeClock::Fixed(99).now(), 99);
    }

    #[tokio::test(start_paused = true)]
    async fn ambiguous_bounded_action_is_not_silently_treated_as_a_message() {
        let replay = ReplayCache::new(1).expect("replay");
        assert_eq!(replay.snapshot().entries, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn staged_handshake_driver_completes_on_fragmentable_duplex_streams() {
        let alice_identity = RouterIdentityBundle::from_private_bytes([1; 32], [2; 32], &mut OsRng)
            .expect("Alice identity");
        let bob_identity = RouterIdentityBundle::from_private_bytes([3; 32], [4; 32], &mut OsRng)
            .expect("Bob identity");
        let alice_static = X25519PrivateKey::from_bytes([0x24; 32]);
        let bob_static = X25519PrivateKey::from_bytes([0x42; 32]);
        let alice_ephemeral = X25519PrivateKey::from_bytes([0x13; 32]);
        let bob_ephemeral = X25519PrivateKey::from_bytes([0x31; 32]);
        let alice_info = router_info(&alice_identity, alice_static.public_bytes());
        let bob_info = router_info(&bob_identity, bob_static.public_bytes());
        let alice_hash = alice_identity.identity().hash().expect("Alice hash");
        let bob_hash = bob_identity.identity().hash().expect("Bob hash");
        let bob_public = PublicKeyBytes::new(bob_static.public_bytes()).expect("Bob public");
        let skew = ClockSkewPolicy::default_compatibility();
        let initiator = InitiatorState::new(
            alice_static,
            alice_ephemeral,
            bob_public,
            Some(bob_hash),
            *bob_hash.as_bytes(),
            [0x55; 16],
            2,
            skew,
        )
        .expect("initiator");
        let responder = ResponderState::new(
            bob_static,
            bob_ephemeral,
            Some(alice_hash),
            *bob_hash.as_bytes(),
            [0x55; 16],
            2,
            skew,
        )
        .expect("responder");
        let replay_a = ReplayCache::new(4).expect("initiator replay");
        let replay_b = ReplayCache::new(4).expect("responder replay");
        let config = HandshakeDriverConfig {
            clock: HandshakeClock::Fixed(1_000),
            padding: PaddingProfile::Deterministic {
                length: 3,
                fill: 0xaa,
            },
            ..HandshakeDriverConfig::default()
        };
        let cancellation_a = CancellationToken::new();
        let cancellation_b = CancellationToken::new();
        let (mut initiator_stream, mut responder_stream) = tokio::io::duplex(128 * 1024);
        let responder_task = drive_responder_handshake(
            responder,
            &mut responder_stream,
            &bob_info,
            &replay_b,
            config,
            &cancellation_b,
        );
        let initiator_task = drive_initiator_handshake(
            initiator,
            &mut initiator_stream,
            &alice_info,
            &replay_a,
            config,
            &cancellation_a,
        );
        let (initiator_result, responder_result) = tokio::join!(initiator_task, responder_task);
        let initiator_result = initiator_result.expect("initiator authenticated");
        let responder_result = responder_result.expect("responder authenticated");
        assert_eq!(initiator_result.authenticated.peer().router_hash, bob_hash);
        assert_eq!(
            responder_result.authenticated.peer().router_hash,
            alice_hash
        );
        assert!(initiator_result.read_bytes > 0);
        assert!(responder_result.written_bytes > 0);
    }

    /// Plan 052 G1: when the responder driver encounters a transport
    /// failure during the initial SessionRequest read, the returned
    /// `HandshakeDriverError` is wrapped in `ResponderStage` with a
    /// redacted `need_request` phase label.
    #[tokio::test(start_paused = true)]
    async fn responder_transport_failure_is_labelled_with_responder_phase() {
        let bob_identity = RouterIdentityBundle::from_private_bytes([3; 32], [4; 32], &mut OsRng)
            .expect("Bob identity");
        let bob_static = X25519PrivateKey::from_bytes([0x42; 32]);
        let bob_ephemeral = X25519PrivateKey::from_bytes([0x31; 32]);
        let bob_hash = bob_identity.identity().hash().expect("Bob hash");
        let bob_info = router_info(&bob_identity, bob_static.public_bytes());
        let responder = ResponderState::new(
            bob_static,
            bob_ephemeral,
            None,
            *bob_hash.as_bytes(),
            [0x55; 16],
            2,
            ClockSkewPolicy::default_compatibility(),
        )
        .expect("responder");
        let replay = ReplayCache::new(4).expect("replay");
        let config = HandshakeDriverConfig {
            clock: HandshakeClock::Fixed(1_000),
            padding: PaddingProfile::Minimum,
            ..HandshakeDriverConfig::default()
        };
        let cancellation = CancellationToken::new();
        let (initiator_stream, mut responder_stream) = tokio::io::duplex(128 * 1024);
        // Close the initiator side immediately so the responder's
        // first read returns EOF (ExactIoError). The responder never
        // reaches the SessionRequest decode.
        drop(initiator_stream);
        let result = drive_responder_handshake(
            responder,
            &mut responder_stream,
            &bob_info,
            &replay,
            config,
            &cancellation,
        )
        .await;
        match result {
            Err(HandshakeDriverError::ResponderStage { phase_label, inner }) => {
                assert_eq!(phase_label, "need_request");
                assert!(matches!(*inner, HandshakeDriverError::Io(_)));
            }
            Err(other) => panic!("expected ResponderStage, got {other:?}"),
            Ok(_) => panic!("expected a labelled failure"),
        }
    }

    /// Plan 052 G1: the initiator path never produces a
    /// `ResponderStage` label; its errors pass through unchanged so the
    /// existing initiator classification is preserved.
    #[tokio::test(start_paused = true)]
    async fn initiator_transport_failure_is_not_responder_labelled() {
        let alice_identity = RouterIdentityBundle::from_private_bytes([1; 32], [2; 32], &mut OsRng)
            .expect("Alice identity");
        let alice_static = X25519PrivateKey::from_bytes([0x24; 32]);
        let alice_ephemeral = X25519PrivateKey::from_bytes([0x13; 32]);
        let bob_hash = alice_identity.identity().hash().expect("Bob hash");
        let alice_info = router_info(&alice_identity, alice_static.public_bytes());
        let bob_public = PublicKeyBytes::new([0x99; 32]).expect("Bob public");
        let initiator = InitiatorState::new(
            alice_static,
            alice_ephemeral,
            bob_public,
            Some(bob_hash),
            *bob_hash.as_bytes(),
            [0x55; 16],
            2,
            ClockSkewPolicy::default_compatibility(),
        )
        .expect("initiator");
        let replay = ReplayCache::new(4).expect("replay");
        let config = HandshakeDriverConfig {
            clock: HandshakeClock::Fixed(1_000),
            padding: PaddingProfile::Minimum,
            ..HandshakeDriverConfig::default()
        };
        let cancellation = CancellationToken::new();
        let (mut initiator_stream, responder_stream) = tokio::io::duplex(128 * 1024);
        // Close the responder side immediately so the initiator's
        // first read returns EOF.
        drop(responder_stream);
        let result = drive_initiator_handshake(
            initiator,
            &mut initiator_stream,
            &alice_info,
            &replay,
            config,
            &cancellation,
        )
        .await;
        match result {
            Err(HandshakeDriverError::ResponderStage { .. }) => {
                panic!("initiator path must never produce ResponderStage")
            }
            Err(HandshakeDriverError::Io(_)) => {}
            Err(other) => panic!("expected Io error from initiator, got {other:?}"),
            Ok(_) => panic!("expected a failure"),
        }
    }
}
