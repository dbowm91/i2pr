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
        }
    }
}

impl std::error::Error for HandshakeDriverError {}

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
}

/// Drives one initiator handshake on a caller-owned stream.
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
    drive(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
    )
    .await
}

/// Drives one responder handshake on a caller-owned stream.
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
    drive(
        state,
        stream,
        local_router_info,
        replay,
        config,
        cancellation,
    )
    .await
}

async fn drive<M, S>(
    state: M,
    stream: &mut S,
    local_router_info: &[u8],
    replay: &ReplayCache,
    config: HandshakeDriverConfig,
    cancellation: &CancellationToken,
) -> Result<HandshakeRun, HandshakeDriverError>
where
    M: HandshakeMachine,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let budget = HandshakeBudget::new(config.deadlines)?;
    let mut transition = state.start().map_err(HandshakeDriverError::Protocol)?;
    let mut read_bytes = 0_u64;
    let mut written_bytes = 0_u64;
    let mut action_count = 0_u32;

    loop {
        if budget.expired() {
            return Err(HandshakeDriverError::Protocol(
                HandshakeError::DeadlineExpired,
            ));
        }
        let state = transition.state;
        let mut next_transition = None;
        for action in transition.actions {
            action_count = action_count.saturating_add(1);
            match action {
                HandshakeAction::ReadExact { length } => {
                    if length > MAX_HANDSHAKE_BUFFERED_INPUT {
                        return Err(HandshakeDriverError::InvalidAction);
                    }
                    let mut bytes = vec![0_u8; length];
                    let deadline = budget.operation_deadline(budget.deadlines.read_idle)?;
                    read_exact(stream, &mut bytes, deadline, cancellation).await?;
                    read_bytes = read_bytes.saturating_add(length as u64);
                    next_transition = Some(
                        state
                            .transition(HandshakeInput::Bytes(bytes))
                            .map_err(HandshakeDriverError::Protocol)?,
                    );
                    break;
                }
                HandshakeAction::ReadBounded { .. } => {
                    // An unframed variable handshake message cannot be safely
                    // delimited by an arbitrary TCP read. Plan 042's staged
                    // prefix/padding actions are the only accepted path.
                    return Err(HandshakeDriverError::InvalidAction);
                }
                HandshakeAction::Write(bytes) => {
                    let length = bytes.len();
                    if length > MAX_HANDSHAKE_BUFFERED_INPUT {
                        return Err(HandshakeDriverError::InvalidAction);
                    }
                    let deadline = budget.operation_deadline(budget.deadlines.write)?;
                    write_all(stream, &bytes.into_bytes(), deadline, cancellation).await?;
                    written_bytes = written_bytes.saturating_add(length as u64);
                }
                HandshakeAction::RequestTimestamp { .. } => {
                    next_transition = Some(
                        state
                            .transition(HandshakeInput::Timestamp(config.clock.now()))
                            .map_err(HandshakeDriverError::Protocol)?,
                    );
                    break;
                }
                HandshakeAction::RequestReplay { token, retention } => {
                    let decision = replay_decision(replay, token, config.clock.now(), retention);
                    next_transition = Some(
                        state
                            .transition(HandshakeInput::Replay(decision))
                            .map_err(HandshakeDriverError::Protocol)?,
                    );
                    break;
                }
                HandshakeAction::RequestPadding { message, maximum } => {
                    let padding = select_padding(config.padding, message, maximum)?;
                    next_transition = Some(
                        state
                            .transition(HandshakeInput::Padding(padding))
                            .map_err(HandshakeDriverError::Protocol)?,
                    );
                    break;
                }
                HandshakeAction::RequestRouterInfo { maximum } => {
                    if local_router_info.is_empty() || local_router_info.len() > maximum {
                        return Err(HandshakeDriverError::RouterInfoTooLarge);
                    }
                    next_transition = Some(
                        state
                            .transition(HandshakeInput::RouterInfo(local_router_info.to_vec()))
                            .map_err(HandshakeDriverError::Protocol)?,
                    );
                    break;
                }
                HandshakeAction::Authenticated(authenticated) => {
                    return Ok(HandshakeRun {
                        authenticated,
                        read_bytes,
                        written_bytes,
                        action_count,
                    });
                }
                HandshakeAction::Terminate(error) => {
                    return Err(HandshakeDriverError::Protocol(error));
                }
            }
        }
        transition = next_transition.ok_or(HandshakeDriverError::InvalidAction)?;
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
}
