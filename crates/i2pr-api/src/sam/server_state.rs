//! SAM v3.1 connection state machine (Plan 137 §5, Plan 138 §3/§7/§8).
//!
//! Each accepted TCP socket progresses through a deterministic
//! state machine:
//!
//! ```text
//! AwaitHello
//!   -- HELLO VERSION compatible --> UtilityReady
//! UtilityReady
//!   -- DEST GENERATE --> UtilityReady (reply only)
//!   -- SESSION CREATE --> SessionControl { session_id, destination_id }
//!   -- PING --> UtilityReady (echo reply)
//!   -- QUIT | STOP | EXIT --> Closed
//!   -- STREAM CONNECT --> RequireStreamConnect (Plan 138)
//!   -- STREAM ACCEPT --> RequireStreamAccept (Plan 138)
//!   -- STREAM FORWARD --> UtilityReady (NOT_IMPLEMENTED, Plan 139)
//!   -- NAMING LOOKUP --> UtilityReady (NOT_IMPLEMENTED)
//! SessionControl
//!   -- QUIT | STOP | EXIT --> Closed (caller must tear session down)
//!   -- PING --> SessionControl (echo reply)
//!   -- DEST GENERATE --> SessionControl (reply only)
//!   -- second SESSION CREATE --> SessionControl (DUPLICATED_ID /
//!     DUPLICATED_DESTINATION / I2P_ERROR)
//!   -- STREAM CONNECT --> RequireStreamConnect (Plan 138)
//!   -- STREAM ACCEPT --> RequireStreamAccept (Plan 138)
//!   -- STREAM FORWARD --> SessionControl (NOT_IMPLEMENTED, Plan 139)
//!   -- NAMING LOOKUP --> SessionControl (NOT_IMPLEMENTED)
//! ```
//!
//! The state machine itself is runtime-neutral. The TCP reader, the
//! per-connection cancellation, the destination/runtime insertion,
//! and the actual session-create / stream-connect / stream-accept
//! transactions live in `i2pr-daemon`.

use i2pr_client::DestinationId;

use crate::sam::command::{
    Command, CommandKind, CommandOutcome, MalformedReason, StreamAcceptError, StreamAcceptRequest,
    StreamConnectError, StreamConnectRequest, UnsupportedReason, extract_versions,
    parse_stream_accept, parse_stream_connect,
};
use crate::sam::reply::{Reply, SessionStatus, StreamStatus};
use crate::sam::session::SamSessionId;
use crate::sam::session_create::SessionCreateRequest;
use crate::sam::version::{MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION};

/// Typed outcome of dispatching one [`CommandOutcome`] against the
/// current [`ServerConnectionState`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchOutcome {
    /// Stay in the same state, send `reply` (if any) to the client.
    Stay {
        /// Optional reply to send before waiting for the next line.
        reply: Option<Reply>,
    },
    /// Advance to `state`, send `reply` (if any).
    Advance {
        /// New state.
        state: ServerConnectionState,
        /// Optional reply.
        reply: Option<Reply>,
    },
    /// Close the connection after sending `reply` (if any). The
    /// daemon is responsible for tearing the session down.
    Close {
        /// Reason code (never observed by the client directly; used
        /// for diagnostic logging).
        close_reason: CloseReason,
        /// Optional reply.
        reply: Option<Reply>,
    },
    /// The command required opening a new session. The runtime must
    /// drive the SAM/DestinationRegistry transaction and then call
    /// [`apply_session_outcome`] to finish the dispatch.
    RequireSessionCreate {
        /// Validated `SESSION CREATE` request.
        request: Box<SessionCreateRequest>,
    },
    /// The command required driving a `STREAM CONNECT` transaction.
    /// The runtime executes the underlying `StreamingManager::connect`
    /// call, drives the per-stream SAM raw-mode transition, and then
    /// calls [`apply_stream_connect_outcome`] to finalise the reply.
    RequireStreamConnect {
        /// Validated `STREAM CONNECT` request.
        request: Box<StreamConnectRequest>,
    },
    /// The command required driving a `STREAM ACCEPT` transaction.
    /// The runtime registers one pending-accept waiter, drives the
    /// inbound SYN observation, captures the peer destination, and
    /// then calls [`apply_stream_accept_outcome`] to finalise the
    /// reply.
    RequireStreamAccept {
        /// Validated `STREAM ACCEPT` request.
        request: Box<StreamAcceptRequest>,
    },
    /// The command was rejected as a malformed line. Send `reply`
    /// and close.
    Malformed {
        /// The reason.
        reason: MalformedReason,
        /// Reply to send (typically `I2P_ERROR`).
        reply: Reply,
    },
    /// The command was recognised as unsupported at the M7 baseline
    /// (e.g. DATAGRAM). Send `reply` and stay in the same state.
    Unsupported {
        /// Reason.
        reason: UnsupportedReason,
        /// Reply to send.
        reply: Reply,
    },
}

impl DispatchOutcome {
    /// Returns the optional reply carried by this outcome.
    pub fn reply(&self) -> Option<&Reply> {
        match self {
            Self::Stay { reply } => reply.as_ref(),
            Self::Advance { reply, .. } => reply.as_ref(),
            Self::Close { reply, .. } => reply.as_ref(),
            Self::Malformed { reply, .. } => Some(reply),
            Self::Unsupported { reply, .. } => Some(reply),
            Self::RequireSessionCreate { .. } => None,
            Self::RequireStreamConnect { .. } => None,
            Self::RequireStreamAccept { .. } => None,
        }
    }
}

/// Reason a connection was closed by the state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReason {
    /// Client sent `QUIT` / `STOP` / `EXIT`.
    ClientQuit,
    /// Client sent `HELLO VERSION` with an incompatible range.
    VersionMismatch,
    /// Parser reported a malformed line that must terminate the
    /// connection.
    MalformedLine,
    /// Parser reported an unknown command that must terminate the
    /// connection.
    UnknownCommand,
    /// Parser reported an unsupported command that must terminate
    /// the connection.
    UnsupportedCommand,
    /// The runtime reported that the per-socket line buffer
    /// overflowed.
    LineOverflow,
    /// The runtime reported a control byte inside a command line.
    ControlByte,
}

/// Deterministic per-connection state. Plan 138 will extend the
/// `AttachReady` variant to model separate attachment sockets; Plan
/// 137 leaves the variant unreachable from `AwaitHello`/`UtilityReady`
/// to make the state-machine contract explicit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerConnectionState {
    /// Waiting for the canonical HELLO VERSION 3.1 handshake.
    AwaitHello,
    /// HELLO succeeded; DEST GENERATE / SESSION CREATE / utility
    /// commands are accepted.
    UtilityReady,
    /// SESSION CREATE succeeded; the per-socket task owns one
    /// session until the connection closes.
    SessionControl {
        /// Session identifier (`ID=`).
        session_id: SamSessionId,
        /// Owning destination identifier.
        destination_id: DestinationId,
    },
    /// Connection is closed. State transitions to this are terminal.
    Closed,
}

impl ServerConnectionState {
    /// Returns `true` when the connection is in the terminal
    /// `Closed` state.
    pub const fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// Dispatch a single [`CommandOutcome`] against the supplied state.
///
/// The function is pure: it consumes no I/O, no runtime resources,
/// and never blocks. Callers (the daemon's per-socket task) drive the
/// state machine once per command.
pub fn dispatch(state: ServerConnectionState, command: &CommandOutcome) -> DispatchOutcome {
    if state.is_closed() {
        return DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: None,
        };
    }

    match command {
        CommandOutcome::UnknownCommand(observed) => {
            tracing_warn_unknown(observed);
            DispatchOutcome::Close {
                close_reason: CloseReason::UnknownCommand,
                reply: Some(Reply::Session(SessionStatus::error(
                    crate::sam::reply::ReplyResult::I2pError,
                    Some(format!("unknown command {}", observed.observed)),
                ))),
            }
        }
        CommandOutcome::UnknownAction(observed) => {
            tracing_warn_unknown_action(observed);
            DispatchOutcome::Close {
                close_reason: CloseReason::UnknownCommand,
                reply: Some(Reply::Session(SessionStatus::error(
                    crate::sam::reply::ReplyResult::I2pError,
                    Some(format!("unknown action {}", observed.observed)),
                ))),
            }
        }
        CommandOutcome::Malformed(malformed) => DispatchOutcome::Malformed {
            reason: malformed.reason,
            reply: Reply::Session(SessionStatus::error(
                crate::sam::reply::ReplyResult::I2pError,
                Some(format!("malformed command: {:?}", malformed.reason)),
            )),
        },
        CommandOutcome::Unsupported(unsupported) => handle_unsupported(&state, unsupported),
        CommandOutcome::Recognised(command) => handle_recognised(state, command),
    }
}

fn tracing_warn_unknown(_observed: &crate::sam::command::UnknownCommand) {}
fn tracing_warn_unknown_action(_observed: &crate::sam::command::UnknownCommand) {}

fn handle_recognised(state: ServerConnectionState, command: &Command) -> DispatchOutcome {
    match command.kind() {
        CommandKind::HelloVersion => handle_hello(state, command),
        CommandKind::DestGenerate => handle_dest_generate(state),
        CommandKind::SessionCreate => handle_session_create(state, command),
        CommandKind::StreamConnect => handle_stream_connect(command),
        CommandKind::StreamAccept => handle_stream_accept(command),
        CommandKind::StreamForward => handle_stream_unsupported(state),
        CommandKind::NamingLookup => handle_naming_unsupported(state),
        CommandKind::Ping => handle_ping(state, command),
        CommandKind::Pong => handle_pong(state),
        CommandKind::Quit => DispatchOutcome::Close {
            close_reason: CloseReason::ClientQuit,
            reply: None,
        },
    }
}

fn handle_hello(state: ServerConnectionState, command: &Command) -> DispatchOutcome {
    if !matches!(state, ServerConnectionState::AwaitHello) {
        return DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: Some(Reply::Hello(crate::sam::reply::HelloReply::no_version(
                Some("HELLO VERSION already negotiated".to_owned()),
            ))),
        };
    }
    let versions = match extract_versions(command) {
        Ok(versions) => versions,
        Err(reason) => {
            return DispatchOutcome::Malformed {
                reason,
                reply: Reply::Hello(crate::sam::reply::HelloReply::no_version(Some(
                    "bad version literal".to_owned(),
                ))),
            };
        }
    };
    let (min, max) = versions;
    let requested_min = min.unwrap_or(MIN_SUPPORTED_VERSION);
    let requested_max = max.unwrap_or(MAX_SUPPORTED_VERSION);
    let overlap = crate::sam::version::negotiate(requested_min, requested_max);
    match overlap {
        crate::sam::version::NegotiatedVersion::Agreed(version) => DispatchOutcome::Advance {
            state: ServerConnectionState::UtilityReady,
            reply: Some(Reply::Hello(crate::sam::reply::HelloReply::ok(version))),
        },
        crate::sam::version::NegotiatedVersion::NoOverlap { .. } => {
            tracing_warn_no_version(requested_min, requested_max);
            DispatchOutcome::Close {
                close_reason: CloseReason::VersionMismatch,
                reply: Some(Reply::Hello(crate::sam::reply::HelloReply::no_version(
                    Some("no overlap with server version set".to_owned()),
                ))),
            }
        }
    }
}

fn tracing_warn_no_version(
    _min: crate::sam::version::SamVersion,
    _max: crate::sam::version::SamVersion,
) {
}

fn handle_dest_generate(state: ServerConnectionState) -> DispatchOutcome {
    match state {
        ServerConnectionState::AwaitHello => DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: Some(Reply::Dest(crate::sam::reply::DestReply::error(
                crate::sam::reply::ReplyResult::I2pError,
                Some("DEST GENERATE before HELLO".to_owned()),
            ))),
        },
        ServerConnectionState::UtilityReady | ServerConnectionState::SessionControl { .. } => {
            // DEST GENERATE is a utility command; the daemon is
            // responsible for executing `dest_generate` and replying
            // with the real `PUB`/`PRIV`. We mark Stay so the caller
            // can drop the command into a separate codepath that
            // bypasses session ownership.
            DispatchOutcome::Stay { reply: None }
        }
        ServerConnectionState::Closed => DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: None,
        },
    }
}

fn handle_session_create(state: ServerConnectionState, command: &Command) -> DispatchOutcome {
    match state {
        ServerConnectionState::AwaitHello => DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: Some(Reply::Session(SessionStatus::error(
                crate::sam::reply::ReplyResult::I2pError,
                Some("SESSION CREATE before HELLO".to_owned()),
            ))),
        },
        ServerConnectionState::UtilityReady => {
            let request = match build_session_create_request(command) {
                Ok(request) => request,
                Err(error) => {
                    return DispatchOutcome::Malformed {
                        reason: MalformedReason::InvalidQuoting,
                        reply: Reply::Session(SessionStatus::error(
                            map_session_create_error_to_result(&error),
                            Some(format!("{error}")),
                        )),
                    };
                }
            };
            DispatchOutcome::RequireSessionCreate {
                request: Box::new(request),
            }
        }
        ServerConnectionState::SessionControl { .. } => DispatchOutcome::Stay {
            reply: Some(Reply::Session(SessionStatus::error(
                crate::sam::reply::ReplyResult::I2pError,
                Some("SESSION CREATE already executed on this control socket".to_owned()),
            ))),
        },
        ServerConnectionState::Closed => DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: None,
        },
    }
}

fn build_session_create_request(
    command: &Command,
) -> Result<SessionCreateRequest, crate::sam::session_create::SessionCreateError> {
    let id = command.value("ID").unwrap_or("");
    let style = command.value("STYLE").unwrap_or("");
    let destination = command.value("DESTINATION").unwrap_or("");
    crate::sam::session_create::parse_session_create(id, style, destination)
}

fn map_session_create_error_to_result(
    error: &crate::sam::session_create::SessionCreateError,
) -> crate::sam::reply::ReplyResult {
    use crate::sam::session_create::SessionCreateError;
    match error {
        SessionCreateError::MissingId
        | SessionCreateError::MissingStyle
        | SessionCreateError::MissingDestination
        | SessionCreateError::UnsupportedStyle(_)
        | SessionCreateError::InvalidDestination(_) => crate::sam::reply::ReplyResult::I2pError,
        SessionCreateError::PrivateDestination(_) | SessionCreateError::Base64(_) => {
            crate::sam::reply::ReplyResult::InvalidKey
        }
    }
}

fn handle_stream_unsupported(_state: ServerConnectionState) -> DispatchOutcome {
    DispatchOutcome::Unsupported {
        reason: UnsupportedReason::StreamConnectPortOptionUnsupported,
        reply: Reply::Stream(StreamStatus::error(
            crate::sam::reply::ReplyResult::NotImplemented,
            Some("STREAM FORWARD lands in Plan 139".to_owned()),
        )),
    }
}

fn handle_stream_connect(command: &Command) -> DispatchOutcome {
    match parse_stream_connect(command) {
        Ok(request) => DispatchOutcome::RequireStreamConnect {
            request: Box::new(request),
        },
        Err(error) => DispatchOutcome::Malformed {
            reason: stream_connect_error_to_malformed(&error),
            reply: Reply::Stream(StreamStatus::error(
                stream_connect_error_to_result(&error),
                Some(format!("{error}")),
            )),
        },
    }
}

fn handle_stream_accept(command: &Command) -> DispatchOutcome {
    match parse_stream_accept(command) {
        Ok(request) => DispatchOutcome::RequireStreamAccept {
            request: Box::new(request),
        },
        Err(error) => DispatchOutcome::Malformed {
            reason: stream_accept_error_to_malformed(&error),
            reply: Reply::Stream(StreamStatus::error(
                stream_accept_error_to_result(&error),
                Some(format!("{error}")),
            )),
        },
    }
}

fn stream_connect_error_to_malformed(error: &StreamConnectError) -> MalformedReason {
    match error {
        StreamConnectError::MissingId => {
            MalformedReason::MissingRequiredOption(crate::sam::command::MissingOption::SessionId)
        }
        StreamConnectError::MissingDestination => MalformedReason::MissingRequiredOption(
            crate::sam::command::MissingOption::SessionDestination,
        ),
        StreamConnectError::InvalidSilent(_) | StreamConnectError::InvalidId => {
            MalformedReason::InvalidQuoting
        }
    }
}

fn stream_connect_error_to_result(error: &StreamConnectError) -> crate::sam::reply::ReplyResult {
    match error {
        StreamConnectError::MissingId
        | StreamConnectError::MissingDestination
        | StreamConnectError::InvalidSilent(_) => crate::sam::reply::ReplyResult::I2pError,
        StreamConnectError::InvalidId => crate::sam::reply::ReplyResult::InvalidId,
    }
}

fn stream_accept_error_to_malformed(error: &StreamAcceptError) -> MalformedReason {
    match error {
        StreamAcceptError::MissingId => {
            MalformedReason::MissingRequiredOption(crate::sam::command::MissingOption::SessionId)
        }
        StreamAcceptError::InvalidSilent(_) | StreamAcceptError::InvalidId => {
            MalformedReason::InvalidQuoting
        }
    }
}

fn stream_accept_error_to_result(error: &StreamAcceptError) -> crate::sam::reply::ReplyResult {
    match error {
        StreamAcceptError::MissingId | StreamAcceptError::InvalidSilent(_) => {
            crate::sam::reply::ReplyResult::I2pError
        }
        StreamAcceptError::InvalidId => crate::sam::reply::ReplyResult::InvalidId,
    }
}

/// Successful outcome of a `STREAM CONNECT` follow-up executed by the
/// daemon. The runtime already advanced the per-stream state machine
/// to `Established`; this payload carries the data the per-stream
/// socket task uses to drive the raw-mode transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamConnectApplied {
    /// Stream id assigned by the registry.
    pub stream_id: u32,
}

/// Failed outcome of a `STREAM CONNECT` follow-up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamConnectFailed {
    /// Protocol-level result vocabulary.
    pub result: crate::sam::reply::ReplyResult,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Successful outcome of a `STREAM ACCEPT` follow-up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamAcceptApplied {
    /// Stream id assigned by the registry.
    pub stream_id: u32,
}

/// Failed outcome of a `STREAM ACCEPT` follow-up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamAcceptFailed {
    /// Protocol-level result vocabulary.
    pub result: crate::sam::reply::ReplyResult,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Convert the daemon's `STREAM CONNECT` follow-up outcome into the
/// per-connection dispatch outcome. A successful connect emits the
/// pre-raw `STREAM STATUS RESULT=OK` line; a failure emits the
/// matching typed failure and closes the per-stream socket.
pub fn apply_stream_connect_outcome(
    outcome: Result<StreamConnectApplied, StreamConnectFailed>,
) -> DispatchOutcome {
    match outcome {
        Ok(_applied) => DispatchOutcome::Stay {
            reply: Some(Reply::Stream(StreamStatus::ok())),
        },
        Err(failed) => DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: Some(Reply::Stream(StreamStatus::error(
                failed.result,
                Some(failed.message),
            ))),
        },
    }
}

/// Convert the daemon's `STREAM ACCEPT` follow-up outcome into the
/// per-connection dispatch outcome.
pub fn apply_stream_accept_outcome(
    outcome: Result<StreamAcceptApplied, StreamAcceptFailed>,
) -> DispatchOutcome {
    match outcome {
        Ok(_applied) => DispatchOutcome::Stay {
            reply: Some(Reply::Stream(StreamStatus::ok())),
        },
        Err(failed) => DispatchOutcome::Close {
            close_reason: CloseReason::MalformedLine,
            reply: Some(Reply::Stream(StreamStatus::error(
                failed.result,
                Some(failed.message),
            ))),
        },
    }
}

fn handle_naming_unsupported(_state: ServerConnectionState) -> DispatchOutcome {
    DispatchOutcome::Unsupported {
        reason: UnsupportedReason::NamingLookupOptions,
        reply: Reply::Naming(crate::sam::reply::NamingReply::error(
            crate::sam::reply::ReplyResult::NotImplemented,
            Some("NAMING LOOKUP lands in Plan 139".to_owned()),
        )),
    }
}

fn handle_ping(state: ServerConnectionState, command: &Command) -> DispatchOutcome {
    let _ = state;
    let payload = command.action_target();
    let reply = match payload {
        Some(payload) => Reply::Pong(crate::sam::reply::PongReply::echo(payload.to_owned())),
        None => Reply::Pong(crate::sam::reply::PongReply::empty()),
    };
    DispatchOutcome::Stay { reply: Some(reply) }
}

fn handle_pong(_state: ServerConnectionState) -> DispatchOutcome {
    DispatchOutcome::Stay { reply: None }
}

fn handle_unsupported(
    _state: &ServerConnectionState,
    unsupported: &crate::sam::command::Unsupported,
) -> DispatchOutcome {
    let reply = match unsupported.kind {
        CommandKind::SessionCreate => Reply::Session(SessionStatus::error(
            crate::sam::reply::ReplyResult::NotImplemented,
            Some(format!("{:?}", unsupported.reason)),
        )),
        CommandKind::StreamConnect | CommandKind::StreamAccept | CommandKind::StreamForward => {
            Reply::Stream(StreamStatus::error(
                crate::sam::reply::ReplyResult::NotImplemented,
                Some(format!("{:?}", unsupported.reason)),
            ))
        }
        CommandKind::NamingLookup => Reply::Naming(crate::sam::reply::NamingReply::error(
            crate::sam::reply::ReplyResult::NotImplemented,
            Some(format!("{:?}", unsupported.reason)),
        )),
        _ => Reply::Session(SessionStatus::error(
            crate::sam::reply::ReplyResult::NotImplemented,
            Some(format!("{:?}", unsupported.reason)),
        )),
    };
    DispatchOutcome::Stay { reply: Some(reply) }
}

/// Apply a successful or failed session-create transaction result to
/// the current state. Used by the daemon to convert the result of a
/// [`DispatchOutcome::RequireSessionCreate`] follow-up into the
/// final reply.
pub fn apply_session_outcome(
    _state: ServerConnectionState,
    outcome: Result<SessionCreateApplied, SessionCreateFailed>,
) -> DispatchOutcome {
    match outcome {
        Ok(applied) => DispatchOutcome::Advance {
            state: ServerConnectionState::SessionControl {
                session_id: applied.session_id,
                destination_id: applied.destination_id,
            },
            reply: Some(Reply::Session(SessionStatus::ok(
                &applied.public_destination_b64,
            ))),
        },
        Err(failed) => DispatchOutcome::Stay {
            reply: Some(Reply::Session(SessionStatus::error(
                failed.result,
                Some(failed.message),
            ))),
        },
    }
}

/// Successful session-create payload returned by the daemon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionCreateApplied {
    /// Newly-created session identifier.
    pub session_id: SamSessionId,
    /// Newly-created destination identifier.
    pub destination_id: DestinationId,
    /// SAM public-destination Base64 text for the reply.
    pub public_destination_b64: String,
}

/// Failed session-create payload returned by the daemon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionCreateFailed {
    /// Protocol-level result vocabulary.
    pub result: crate::sam::reply::ReplyResult,
    /// Human-readable diagnostic message.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sam::parser::parse_line;
    use crate::sam::reply::ReplyResult;
    use crate::sam::version::SamVersion;

    fn hello() -> CommandOutcome {
        parse_line("HELLO VERSION MIN=3.1 MAX=3.1").expect("parse")
    }

    fn dest_generate() -> CommandOutcome {
        parse_line("DEST GENERATE SIGNATURE_TYPE=7").expect("parse")
    }

    fn session_create_transient(id: &str) -> CommandOutcome {
        parse_line(&format!(
            "SESSION CREATE STYLE=STREAM ID={id} DESTINATION=TRANSIENT"
        ))
        .expect("parse")
    }

    #[test]
    fn hello_from_await_hello_advances() {
        let outcome = dispatch(ServerConnectionState::AwaitHello, &hello());
        match outcome {
            DispatchOutcome::Advance {
                state: ServerConnectionState::UtilityReady,
                reply: Some(Reply::Hello(reply)),
            } => {
                assert_eq!(reply.result(), ReplyResult::Ok);
                assert_eq!(reply.version(), Some(SamVersion::const_new(3, 1)));
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    #[test]
    fn hello_after_utility_ready_closes() {
        let state = ServerConnectionState::UtilityReady;
        let outcome = dispatch(state, &hello());
        assert!(matches!(
            outcome,
            DispatchOutcome::Close {
                close_reason: CloseReason::MalformedLine,
                ..
            }
        ));
    }

    #[test]
    fn session_create_before_hello_closes() {
        let outcome = dispatch(
            ServerConnectionState::AwaitHello,
            &session_create_transient("alpha"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Close {
                close_reason: CloseReason::MalformedLine,
                ..
            }
        ));
    }

    #[test]
    fn session_create_in_utility_ready_requires_runtime() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &session_create_transient("alpha"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::RequireSessionCreate { .. }
        ));
    }

    #[test]
    fn session_create_in_session_control_is_rejected() {
        let session_id = SamSessionId::new("alpha").unwrap();
        let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([1_u8; 32]));
        let state = ServerConnectionState::SessionControl {
            session_id,
            destination_id,
        };
        let outcome = dispatch(state, &session_create_transient("beta"));
        match outcome {
            DispatchOutcome::Stay {
                reply: Some(Reply::Session(reply)),
            } => {
                assert_eq!(reply.result(), ReplyResult::I2pError);
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    #[test]
    fn dest_generate_before_hello_closes() {
        let outcome = dispatch(ServerConnectionState::AwaitHello, &dest_generate());
        assert!(matches!(
            outcome,
            DispatchOutcome::Close {
                close_reason: CloseReason::MalformedLine,
                ..
            }
        ));
    }

    #[test]
    fn dest_generate_in_utility_ready_stays() {
        let outcome = dispatch(ServerConnectionState::UtilityReady, &dest_generate());
        assert!(matches!(outcome, DispatchOutcome::Stay { reply: None }));
    }

    #[test]
    fn quit_closes() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("QUIT").expect("parse"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Close {
                close_reason: CloseReason::ClientQuit,
                ..
            }
        ));
    }

    #[test]
    fn ping_echoes_payload() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("PING hello").expect("parse"),
        );
        match outcome {
            DispatchOutcome::Stay {
                reply: Some(Reply::Pong(reply)),
            } => {
                assert_eq!(reply.payload(), Some("hello"));
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    #[test]
    fn stream_connect_returns_require_runtime_handshake() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("STREAM CONNECT ID=alpha DESTINATION=foo").expect("parse"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::RequireStreamConnect { .. }
        ));
    }

    #[test]
    fn stream_accept_returns_require_runtime_handshake() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("STREAM ACCEPT ID=alpha").expect("parse"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::RequireStreamAccept { .. }
        ));
    }

    #[test]
    fn stream_forward_returns_not_implemented() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("STREAM FORWARD ID=alpha DESTINATION=foo").expect("parse"),
        );
        assert!(matches!(outcome, DispatchOutcome::Unsupported { .. }));
    }

    #[test]
    fn stream_connect_missing_destination_is_malformed() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("STREAM CONNECT ID=alpha").expect("parse"),
        );
        match outcome {
            DispatchOutcome::Malformed {
                reply: Reply::Stream(_),
                ..
            } => {}
            other => panic!("expected malformed stream reply, got {other:?}"),
        }
    }

    #[test]
    fn unknown_command_closes() {
        let outcome = dispatch(
            ServerConnectionState::UtilityReady,
            &parse_line("WHAT AM I").expect("parse"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Close {
                close_reason: CloseReason::UnknownCommand,
                ..
            }
        ));
    }

    #[test]
    fn apply_session_outcome_advances() {
        let session_id = SamSessionId::new("alpha").unwrap();
        let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([7_u8; 32]));
        let outcome = apply_session_outcome(
            ServerConnectionState::UtilityReady,
            Ok(SessionCreateApplied {
                session_id: session_id.clone(),
                destination_id,
                public_destination_b64: "PUB".to_owned(),
            }),
        );
        match outcome {
            DispatchOutcome::Advance {
                state:
                    ServerConnectionState::SessionControl {
                        session_id: ref advanced_id,
                        destination_id: advanced_dest,
                    },
                reply: Some(Reply::Session(reply)),
            } => {
                assert_eq!(advanced_id, &session_id);
                assert_eq!(advanced_dest, destination_id);
                assert_eq!(reply.result(), ReplyResult::Ok);
                assert_eq!(reply.destination(), Some("PUB"));
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    #[test]
    fn apply_session_outcome_stays_on_failure() {
        let outcome = apply_session_outcome(
            ServerConnectionState::UtilityReady,
            Err(SessionCreateFailed {
                result: ReplyResult::DuplicatedId,
                message: "duplicate id".to_owned(),
            }),
        );
        match outcome {
            DispatchOutcome::Stay {
                reply: Some(Reply::Session(reply)),
            } => {
                assert_eq!(reply.result(), ReplyResult::DuplicatedId);
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    #[test]
    fn no_overlap_hello_closes() {
        let outcome = dispatch(
            ServerConnectionState::AwaitHello,
            &parse_line("HELLO VERSION MIN=2.0 MAX=2.0").expect("parse"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Close {
                close_reason: CloseReason::VersionMismatch,
                ..
            }
        ));
    }
}
