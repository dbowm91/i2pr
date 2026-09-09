//! Runtime-neutral I2CP connection state machine (Plan 165 §2).
//!
//! The machine exposes a finite, deterministic set of legal transitions:
//!
//! ```text
//! AwaitProtocolByte
//!   -> AwaitGetDate
//!   -> ReadyForSession
//!   -> SessionPending
//!   -> Active
//!   -> Closing
//!   -> Closed
//! ```
//!
//! Message families that the protocol does not permit in the current
//! state are rejected with [`I2cpError::IllegalInState`]. The state
//! machine does not silently resynchronize: a duplicate GetDate, a
//! SetDate before GetDate, or a CreateSession before SetDate are all
//! rejected with typed errors.
//!
//! The state machine owns no sockets, timers, Tokio tasks, or session
//! secret material. The Plan 167 daemon projects typed actions out of
//! the state machine into the [`crate::i2cp`] message codecs.

use crate::i2cp::error::I2cpError;
use crate::i2cp::ids::SessionId;
use crate::i2cp::message::{GetDate, MessageType, SetDate};

/// Maximum accepted I2CP API version string byte length.
///
/// The I2CP specification treats the version as a free-form string;
/// this hard cap stops a hostile peer from supplying megabytes of
/// data while preserving every reference client implementation.
pub const MAX_VERSION_STRING_BYTES: usize = 32;

/// The I2CP API versions the M9 profile advertises and accepts.
pub const M9_ADVERTISED_VERSION: &str = "0.9.67";

/// The connection lifecycle states.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConnectionState {
    /// The first byte of the TCP stream has not been observed yet.
    AwaitProtocolByte,
    /// The protocol byte was accepted; waiting for the first
    /// `GetDate` from the client.
    AwaitGetDate,
    /// The client's `GetDate` was accepted; the router replied with
    /// `SetDate`. The connection is ready to accept a `CreateSession`.
    ReadyForSession,
    /// A `CreateSession` is being processed; the SessionConfig is
    /// under verification and a destination reservation is open.
    SessionPending,
    /// A session has been committed; session-bearing messages are
    /// accepted from this connection.
    Active,
    /// A graceful `Disconnect` was received; outstanding reservations
    /// are rolled back. No further messages are accepted.
    Closing,
    /// The connection is closed; no further input is processed.
    Closed,
}

impl ConnectionState {
    /// Returns the static name used for error diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::AwaitProtocolByte => "AwaitProtocolByte",
            Self::AwaitGetDate => "AwaitGetDate",
            Self::ReadyForSession => "ReadyForSession",
            Self::SessionPending => "SessionPending",
            Self::Active => "Active",
            Self::Closing => "Closing",
            Self::Closed => "Closed",
        }
    }

    /// Reports whether a message family is legal in this state.
    pub fn accepts(self, message_type: MessageType) -> bool {
        match self {
            Self::AwaitProtocolByte => false,
            Self::AwaitGetDate => false,
            Self::ReadyForSession => matches!(
                message_type,
                MessageType::CreateSession | MessageType::Disconnect
            ),
            Self::SessionPending => matches!(
                message_type,
                MessageType::CreateSession | MessageType::DestroySession | MessageType::Disconnect
            ),
            Self::Active => matches!(
                message_type,
                MessageType::CreateSession
                    | MessageType::ReconfigureSession
                    | MessageType::DestroySession
                    | MessageType::CreateLeaseSet2
                    | MessageType::SendMessage
                    | MessageType::SendMessageExpires
                    | MessageType::MessageStatus
                    | MessageType::GetBandwidthLimits
                    | MessageType::DestLookup
                    | MessageType::HostLookup
                    | MessageType::Disconnect
            ),
            Self::Closing | Self::Closed => false,
        }
    }
}

/// One connection state machine, runtime-neutral. The Plan 167 daemon
/// owns one machine per accepted TCP socket and feeds bytes into it.
#[derive(Debug)]
pub struct ConnectionStateMachine {
    state: ConnectionState,
    version: Option<String>,
    has_get_date: bool,
    has_set_date: bool,
    active_session: Option<SessionId>,
}

impl Default for ConnectionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionStateMachine {
    /// Creates a fresh state machine in [`ConnectionState::AwaitProtocolByte`].
    pub const fn new() -> Self {
        Self {
            state: ConnectionState::AwaitProtocolByte,
            version: None,
            has_get_date: false,
            has_set_date: false,
            active_session: None,
        }
    }

    /// Returns the current state.
    pub const fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns the negotiated I2CP API version, if any.
    pub fn negotiated_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Returns whether a `GetDate` has already been observed.
    pub const fn has_get_date(&self) -> bool {
        self.has_get_date
    }

    /// Returns whether a `SetDate` reply has already been emitted.
    pub const fn has_set_date(&self) -> bool {
        self.has_set_date
    }

    /// Returns the currently active session, if any.
    pub const fn active_session(&self) -> Option<SessionId> {
        self.active_session
    }

    /// Validates that a `GetDate` body may be accepted in the current
    /// state and returns the negotiated version on success. The
    /// machine transitions to [`ConnectionState::AwaitGetDate`] if it
    /// was waiting for the protocol byte.
    pub fn observe_protocol_byte(&mut self) -> Result<(), I2cpError> {
        if self.state != ConnectionState::AwaitProtocolByte {
            return Err(I2cpError::HandshakeOrdering {
                context: "protocol byte observed more than once",
            });
        }
        self.state = ConnectionState::AwaitGetDate;
        Ok(())
    }

    /// Accepts a `GetDate` body and returns the `SetDate` the router
    /// must reply with.
    ///
    /// Rejects malformed ordering (duplicate GetDate, premature
    /// SetDate), unsupported authentication, or unparsable version
    /// strings.
    pub fn handle_get_date(
        &mut self,
        get_date: &GetDate,
        now_ms: u64,
    ) -> Result<SetDate, I2cpError> {
        if self.state == ConnectionState::AwaitProtocolByte {
            return Err(I2cpError::HandshakeOrdering {
                context: "GetDate received before protocol byte",
            });
        }
        if self.has_get_date {
            return Err(I2cpError::HandshakeOrdering {
                context: "duplicate GetDate",
            });
        }
        // The M9 profile only rejects authentication mappings that
        // carry a non-empty entry set. Java I2P 2.13.0 and go-i2cp
        // both append an empty `Mapping` body after the version
        // string for I2CP 0.9.11+ compliance; treating that
        // presence as a credentialed authentication request would
        // have rejected every unmodified client.
        if let Some(auth) = get_date.auth.as_ref()
            && !auth.entries().is_empty()
        {
            return Err(I2cpError::AuthNotSupported {
                context: "GetDate username/password authentication is not accepted in M9",
            });
        }
        let version = validate_version(&get_date.version)?;
        self.version = Some(version.clone());
        self.has_get_date = true;
        self.has_set_date = true;
        self.state = ConnectionState::ReadyForSession;
        Ok(SetDate {
            date_ms: now_ms,
            version,
        })
    }

    /// Marks the router as having emitted a `SetDate` reply. Called by
    /// the daemon immediately after it transmits the reply.
    pub fn mark_set_date_sent(&mut self) {
        self.has_set_date = true;
        if matches!(self.state, ConnectionState::AwaitGetDate) {
            self.state = ConnectionState::ReadyForSession;
        }
    }

    /// Begins a session lifecycle event for `CreateSession`. The machine
    /// transitions to [`ConnectionState::SessionPending`] when
    /// currently in [`ConnectionState::ReadyForSession`], and rejects
    /// any further `CreateSession` once a session has been activated
    /// (M9 default policy: one primary session per connection).
    pub fn begin_create_session(&mut self) -> Result<(), I2cpError> {
        match self.state {
            ConnectionState::ReadyForSession => {
                self.state = ConnectionState::SessionPending;
                Ok(())
            }
            ConnectionState::SessionPending => Ok(()),
            ConnectionState::Active => Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: MessageType::CreateSession.code(),
            }),
            _ => Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: MessageType::CreateSession.code(),
            }),
        }
    }

    /// Begins a `DestroySession` lifecycle event. The session must
    /// have been activated; DestroySession is rejected in
    /// `ReadyForSession`, `SessionPending`, or terminal states.
    pub fn begin_destroy_session(&mut self) -> Result<(), I2cpError> {
        match self.state {
            ConnectionState::Active => {
                // The active session will be torn down by the daemon;
                // the machine returns to `ReadyForSession` when the
                // session registry confirms destruction.
                Ok(())
            }
            _ => Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: MessageType::DestroySession.code(),
            }),
        }
    }

    /// Commits the supplied session id as the connection's primary
    /// session and transitions to [`ConnectionState::Active`].
    pub fn activate_session(&mut self, session: SessionId) -> Result<(), I2cpError> {
        if !matches!(
            self.state,
            ConnectionState::SessionPending | ConnectionState::Active
        ) {
            return Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: MessageType::CreateSession.code(),
            });
        }
        if let Some(previous) = self.active_session
            && previous != session
        {
            return Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: MessageType::CreateSession.code(),
            });
        }
        self.active_session = Some(session);
        self.state = ConnectionState::Active;
        Ok(())
    }

    /// Clears the active session (after DestroySession or a fatal
    /// destination failure) and returns to [`ConnectionState::ReadyForSession`].
    pub fn deactivate_session(&mut self) {
        self.active_session = None;
        if matches!(self.state, ConnectionState::Active) {
            self.state = ConnectionState::ReadyForSession;
        }
    }

    /// Begins a graceful disconnect. Returns the closed reason if
    /// `Disconnect` is the active transition.
    pub fn begin_close(&mut self) -> Result<(), I2cpError> {
        if !self.state.accepts(MessageType::Disconnect) {
            return Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: MessageType::Disconnect.code(),
            });
        }
        self.state = ConnectionState::Closing;
        Ok(())
    }

    /// Transitions from [`ConnectionState::Closing`] to
    /// [`ConnectionState::Closed`].
    pub fn finish_close(&mut self) {
        if matches!(self.state, ConnectionState::Closing) {
            self.state = ConnectionState::Closed;
        }
    }

    /// Validates a `GetDate` body without mutating state.
    pub fn validate_message_type(&self, message_type: MessageType) -> Result<(), I2cpError> {
        if self.state.accepts(message_type) {
            Ok(())
        } else {
            Err(I2cpError::IllegalInState {
                state: self.state.name(),
                message_type: message_type.code(),
            })
        }
    }
}

/// Validates a bounded I2CP version string.
///
/// The M9 profile advertises [`M9_ADVERTISED_VERSION`] (`0.9.67`) in
/// its `SetDate` reply but accepts any syntactically valid
/// `major.minor.patch` string up to [`MAX_VERSION_STRING_BYTES`].
/// The official Java I2P 2.13.0 reference sends `0.9.70`, the
/// go-i2cp reference sends `0.9.66`, and i2pd 2.61.0 sends `0.9.66`;
///
/// pinning the negotiation to one string would have rejected every
/// unmodified client. The router records the negotiated version for
/// diagnostics but never gates session logic on it.
fn validate_version(raw: &str) -> Result<String, I2cpError> {
    if raw.is_empty() || raw.len() > MAX_VERSION_STRING_BYTES {
        return Err(I2cpError::VersionNegotiation {
            reason: "version string outside [1, MAX_VERSION_STRING_BYTES) bytes",
        });
    }
    // I2CP version negotiation (Plan 170 §5): accept any well-formed
    // `0.x.y` client version and answer with the M9 advertised
    // version in SetDate. Unmodified Java I2P 2.13.0 sends `0.9.70`
    // and go-i2cp sends `0.9.67`; both must complete the handshake
    // against the same daemon. A non-zero major version speaks a
    // different protocol family and is rejected as unnegotiable.
    let mut parts = raw.split('.');
    let compatible = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(major), Some(_minor), Some(_patch), None) => major == "0",
        _ => false,
    };
    if !compatible {
        return Err(I2cpError::VersionNegotiation {
            reason: "version outside the negotiable 0.x family",
        });
    }
    Ok(raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::Mapping;

    fn version_string() -> String {
        M9_ADVERTISED_VERSION.to_owned()
    }

    #[test]
    fn fresh_machine_is_in_await_protocol_byte_state() {
        let machine = ConnectionStateMachine::new();
        assert_eq!(machine.state(), ConnectionState::AwaitProtocolByte);
        assert!(machine.negotiated_version().is_none());
        assert!(!machine.has_get_date());
        assert!(!machine.has_set_date());
        assert!(machine.active_session().is_none());
    }

    #[test]
    fn observe_protocol_byte_then_get_date_advances_to_ready() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        let get_date = GetDate {
            version: version_string(),
            auth: None,
        };
        let set_date = machine.handle_get_date(&get_date, 0).expect("get date");
        assert_eq!(set_date.version, M9_ADVERTISED_VERSION);
        assert_eq!(machine.state(), ConnectionState::ReadyForSession);
        assert!(machine.has_get_date());
        assert!(machine.has_set_date());
    }

    #[test]
    fn duplicate_get_date_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        let get_date = GetDate {
            version: version_string(),
            auth: None,
        };
        machine.handle_get_date(&get_date, 0).expect("first");
        let error = machine
            .handle_get_date(&get_date, 0)
            .expect_err("duplicate rejected");
        assert!(matches!(error, I2cpError::HandshakeOrdering { .. }));
    }

    #[test]
    fn get_date_before_protocol_byte_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        let get_date = GetDate {
            version: version_string(),
            auth: None,
        };
        let error = machine
            .handle_get_date(&get_date, 0)
            .expect_err("ordering rejected");
        assert!(matches!(error, I2cpError::HandshakeOrdering { .. }));
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        let get_date = GetDate {
            version: "1.2.3".to_owned(),
            auth: None,
        };
        let error = machine
            .handle_get_date(&get_date, 0)
            .expect_err("unknown version rejected");
        assert!(matches!(error, I2cpError::VersionNegotiation { .. }));
    }

    #[test]
    fn auth_in_get_date_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        let auth = Mapping::from_entries(vec![
            ("i2cp.username".to_owned(), "user".to_owned()),
            ("i2cp.password".to_owned(), "secret".to_owned()),
        ])
        .expect("auth");
        let get_date = GetDate {
            version: version_string(),
            auth: Some(auth),
        };
        let error = machine
            .handle_get_date(&get_date, 0)
            .expect_err("auth rejected");
        assert!(matches!(error, I2cpError::AuthNotSupported { .. }));
    }

    #[test]
    fn create_session_before_set_date_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        let error = machine
            .begin_create_session()
            .expect_err("ordering rejected");
        assert!(matches!(error, I2cpError::IllegalInState { .. }));
    }

    #[test]
    fn multi_session_creation_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        machine
            .handle_get_date(
                &GetDate {
                    version: version_string(),
                    auth: None,
                },
                0,
            )
            .expect("get date");
        machine.begin_create_session().expect("begin session");
        machine
            .activate_session(SessionId::new(7))
            .expect("activate");
        let error = machine
            .begin_create_session()
            .expect_err("multi-session rejected");
        assert!(matches!(error, I2cpError::IllegalInState { .. }));
    }

    #[test]
    fn session_status_in_ready_state_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        machine
            .handle_get_date(
                &GetDate {
                    version: version_string(),
                    auth: None,
                },
                0,
            )
            .expect("get date");
        let error = machine
            .validate_message_type(MessageType::SessionStatus)
            .expect_err("session status rejected");
        assert!(matches!(error, I2cpError::IllegalInState { .. }));
    }

    #[test]
    fn disconnect_transitions_through_closing_to_closed() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        machine
            .handle_get_date(
                &GetDate {
                    version: version_string(),
                    auth: None,
                },
                0,
            )
            .expect("get date");
        machine.begin_close().expect("begin close");
        assert_eq!(machine.state(), ConnectionState::Closing);
        machine.finish_close();
        assert_eq!(machine.state(), ConnectionState::Closed);
        let error = machine
            .validate_message_type(MessageType::GetDate)
            .expect_err("closed accepts nothing");
        assert!(matches!(error, I2cpError::IllegalInState { .. }));
    }

    #[test]
    fn deactivate_session_returns_to_ready_state() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        machine
            .handle_get_date(
                &GetDate {
                    version: version_string(),
                    auth: None,
                },
                0,
            )
            .expect("get date");
        machine.begin_create_session().expect("begin session");
        machine
            .activate_session(SessionId::new(11))
            .expect("activate");
        machine.deactivate_session();
        assert_eq!(machine.state(), ConnectionState::ReadyForSession);
        assert!(machine.active_session().is_none());
    }

    #[test]
    fn destroying_session_before_session_is_rejected() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        machine
            .handle_get_date(
                &GetDate {
                    version: version_string(),
                    auth: None,
                },
                0,
            )
            .expect("get date");
        let error = machine
            .begin_destroy_session()
            .expect_err("destroy without session rejected");
        assert!(matches!(error, I2cpError::IllegalInState { .. }));
    }

    #[test]
    fn destroy_session_in_active_state_is_accepted() {
        let mut machine = ConnectionStateMachine::new();
        machine.observe_protocol_byte().expect("protocol byte");
        machine
            .handle_get_date(
                &GetDate {
                    version: version_string(),
                    auth: None,
                },
                0,
            )
            .expect("get date");
        machine.begin_create_session().expect("begin session");
        machine
            .activate_session(SessionId::new(11))
            .expect("activate");
        machine
            .begin_destroy_session()
            .expect("destroy accepted in active");
        machine.deactivate_session();
        assert_eq!(machine.state(), ConnectionState::ReadyForSession);
    }
}
