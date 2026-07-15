//! Pure, consuming NTCP2 handshake state machines.
//!
//! The state machines in this module own protocol sequencing only. A runtime
//! fulfills the returned actions and supplies their results through
//! [`HandshakeInput`]. No action waits, performs I/O, consults a clock, or
//! touches a replay store itself.

#![allow(clippy::module_name_repetitions)]

use std::fmt;

use i2pr_crypto::X25519PrivateKey;
use i2pr_proto::Hash;

use crate::constants;
use crate::crypto::{
    AesObfuscationState, Ntcp2CryptoError, PublicKeyBytes, Role, SplitKeys, Transcript,
};
use crate::frame::{ReceiveState, TransmitState, into_directional_states};
use crate::handshake::{
    AuthenticatedPeer, ClockSkewPolicy, ConfirmedPayload, HandshakeError, ReplayDecision,
    ReplayToken, SessionConfirmed, SessionCreated, SessionCreatedOptions, SessionRequest,
    SessionRequestOptions, validate_router_info,
};

/// The kind of padding requested from the runtime policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaddingMessage {
    /// Cleartext padding following SessionRequest.
    SessionRequest,
    /// Cleartext padding following SessionCreated.
    SessionCreated,
    /// Authenticated padding in SessionConfirmed part two.
    SessionConfirmed,
}

/// The handshake operation for which a timestamp is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimestampPurpose {
    /// Timestamp to place in SessionRequest.
    SessionRequest,
    /// Timestamp to place in SessionCreated.
    SessionCreated,
    /// Local timestamp used to classify a peer timestamp.
    PeerValidation,
}

/// Bounded owned handshake bytes with redacted diagnostics.
pub struct HandshakeBytes(Vec<u8>);

impl HandshakeBytes {
    fn new(bytes: Vec<u8>) -> Result<Self, HandshakeError> {
        if bytes.len() > constants::MAX_HANDSHAKE_BUFFERED_INPUT {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(Self(bytes))
    }

    /// Borrows the complete owned byte sequence.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns the bounded byte length.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the byte sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Transfers ownership of the encoded bytes to the runtime adapter.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for HandshakeBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandshakeBytes")
            .field("length", &self.len())
            .finish()
    }
}

/// An immediate runtime operation emitted by a handshake state.
pub enum HandshakeAction {
    /// Read one complete message up to the supplied inclusive bounds.
    ReadBounded {
        /// Minimum accepted read length.
        minimum: usize,
        /// Maximum accepted read length.
        maximum: usize,
    },
    /// Read exactly the negotiated number of bytes.
    ReadExact {
        /// Exact read length.
        length: usize,
    },
    /// Write one owned, bounded handshake message.
    Write(HandshakeBytes),
    /// Ask the runtime for a wall-clock timestamp in Unix seconds.
    RequestTimestamp {
        /// The protocol use of the requested timestamp.
        purpose: TimestampPurpose,
    },
    /// Ask a replay service to admit a bounded token.
    RequestReplay {
        /// Token derived from the SessionRequest ephemeral field.
        token: ReplayToken,
        /// Minimum retention requested by the clock-skew policy.
        retention: u64,
    },
    /// Ask a bounded padding policy for bytes for one message.
    RequestPadding {
        /// Message receiving the padding.
        message: PaddingMessage,
        /// Inclusive maximum number of padding bytes.
        maximum: usize,
    },
    /// Ask an authenticated local source for RouterInfo bytes.
    RequestRouterInfo {
        /// Maximum RouterInfo bytes that may be returned.
        maximum: usize,
    },
    /// Report the completed authenticated handshake.
    Authenticated(AuthenticatedHandshake),
    /// Report a typed terminal failure to the runtime adapter.
    Terminate(HandshakeError),
}

impl fmt::Debug for HandshakeAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadBounded { minimum, maximum } => formatter
                .debug_struct("ReadBounded")
                .field("minimum", minimum)
                .field("maximum", maximum)
                .finish(),
            Self::ReadExact { length } => formatter
                .debug_struct("ReadExact")
                .field("length", length)
                .finish(),
            Self::Write(bytes) => formatter.debug_tuple("Write").field(bytes).finish(),
            Self::RequestTimestamp { purpose } => formatter
                .debug_struct("RequestTimestamp")
                .field("purpose", purpose)
                .finish(),
            Self::RequestReplay { token, retention } => formatter
                .debug_struct("RequestReplay")
                .field("token", token)
                .field("retention", retention)
                .finish(),
            Self::RequestPadding { message, maximum } => formatter
                .debug_struct("RequestPadding")
                .field("message", message)
                .field("maximum", maximum)
                .finish(),
            Self::RequestRouterInfo { maximum } => formatter
                .debug_struct("RequestRouterInfo")
                .field("maximum", maximum)
                .finish(),
            Self::Authenticated(result) => formatter
                .debug_struct("Authenticated")
                .field("role", &result.role)
                .field("router_hash", &result.peer.router_hash)
                .finish(),
            Self::Terminate(error) => formatter.debug_tuple("Terminate").field(error).finish(),
        }
    }
}

/// One result supplied by the runtime in response to a handshake action.
pub enum HandshakeInput {
    /// Bytes returned by a read action.
    Bytes(Vec<u8>),
    /// A wall-clock timestamp in Unix seconds.
    Timestamp(u64),
    /// A replay-cache admission result.
    Replay(ReplayDecision),
    /// Bytes returned by a padding policy.
    Padding(Vec<u8>),
    /// Bytes returned by the authenticated local RouterInfo source.
    RouterInfo(Vec<u8>),
    /// Cancellation requested by the owning runtime scope.
    Cancelled,
    /// The owning runtime scope's deadline expired.
    DeadlineExpired,
    /// The underlying transport disconnected.
    Disconnected,
}

impl fmt::Debug for HandshakeInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(bytes) => formatter
                .debug_struct("Bytes")
                .field("length", &bytes.len())
                .finish(),
            Self::Timestamp(value) => formatter.debug_tuple("Timestamp").field(value).finish(),
            Self::Replay(decision) => formatter.debug_tuple("Replay").field(decision).finish(),
            Self::Padding(bytes) => formatter
                .debug_struct("Padding")
                .field("length", &bytes.len())
                .finish(),
            Self::RouterInfo(bytes) => formatter
                .debug_struct("RouterInfo")
                .field("length", &bytes.len())
                .finish(),
            Self::Cancelled => formatter.write_str("Cancelled"),
            Self::DeadlineExpired => formatter.write_str("DeadlineExpired"),
            Self::Disconnected => formatter.write_str("Disconnected"),
        }
    }
}

/// The negotiated values retained with an authenticated handshake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NegotiatedParameters {
    /// NTCP2 network identifier from SessionRequest.
    pub network_id: u8,
    /// Exact SessionConfirmed part-two ciphertext length.
    pub session_confirmed_part2_length: usize,
    /// SessionRequest cleartext padding length.
    pub session_request_padding_length: usize,
    /// SessionCreated cleartext padding length.
    pub session_created_padding_length: usize,
    /// SessionConfirmed plaintext payload length.
    pub session_confirmed_plaintext_length: usize,
}

/// The authenticated result handed to the later data-phase owner.
pub struct AuthenticatedHandshake {
    role: Role,
    peer: AuthenticatedPeer,
    negotiated: NegotiatedParameters,
    split_keys: SplitKeys,
}

impl AuthenticatedHandshake {
    /// Returns the handshake role.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Returns the authenticated peer identity and transport key binding.
    pub const fn peer(&self) -> AuthenticatedPeer {
        self.peer
    }

    /// Returns the bounded negotiated parameters.
    pub const fn negotiated(&self) -> NegotiatedParameters {
        self.negotiated
    }

    /// Borrows the consuming data-phase key owners.
    pub const fn split_keys(&mut self) -> &mut SplitKeys {
        &mut self.split_keys
    }

    /// Consumes the handshake result into independent runtime-neutral data
    /// phase transmit and receive owners.
    pub fn into_data_phase(self) -> (TransmitState, ReceiveState) {
        into_directional_states(self.split_keys)
    }
}

/// A successful consuming transition and its immediate actions.
pub struct HandshakeTransition<S> {
    /// The only state from which the next transition may be made.
    pub state: S,
    /// Bounded actions for the runtime adapter.
    pub actions: Vec<HandshakeAction>,
}

fn transition<S>(state: S, actions: Vec<HandshakeAction>) -> HandshakeTransition<S> {
    debug_assert!(actions.len() <= constants::MAX_HANDSHAKE_ACTIONS);
    HandshakeTransition { state, actions }
}

fn public_key(key: &X25519PrivateKey) -> Result<PublicKeyBytes, HandshakeError> {
    PublicKeyBytes::new(key.public_bytes()).map_err(|_| HandshakeError::DeobfuscationFailure)
}

fn map_crypto(error: Ntcp2CryptoError) -> HandshakeError {
    match error {
        Ntcp2CryptoError::InvalidPublicKey => HandshakeError::DeobfuscationFailure,
        Ntcp2CryptoError::AuthenticationFailed | Ntcp2CryptoError::EncryptionFailed => {
            HandshakeError::AuthenticationFailure
        }
        Ntcp2CryptoError::PeerStaticMismatch => HandshakeError::TransportStaticKeyMismatch,
        Ntcp2CryptoError::InvalidState | Ntcp2CryptoError::WrongRole => {
            HandshakeError::TranscriptMismatch
        }
        other => HandshakeError::Crypto(other),
    }
}

fn check_padding(actual: usize, expected: usize) -> Result<(), HandshakeError> {
    if actual != expected {
        return Err(if actual > expected {
            HandshakeError::ExcessivePadding
        } else {
            HandshakeError::InvalidFixedLength
        });
    }
    Ok(())
}

fn replay_result(decision: ReplayDecision) -> Result<(), HandshakeError> {
    match decision {
        ReplayDecision::Fresh => Ok(()),
        ReplayDecision::Replayed => Err(HandshakeError::ReplayDetected),
        ReplayDecision::CacheFull | ReplayDecision::Unavailable => {
            Err(HandshakeError::ReplayCacheUnavailable)
        }
    }
}

struct InitiatorCommon {
    local_static: X25519PrivateKey,
    ephemeral: X25519PrivateKey,
    responder_static: PublicKeyBytes,
    expected_peer_hash: Option<Hash>,
    network_id: u8,
    skew: ClockSkewPolicy,
}

enum InitiatorPhase {
    NeedRouterInfo {
        common: InitiatorCommon,
        obfuscation: AesObfuscationState,
        transcript: Transcript,
    },
    NeedRequestTimestamp {
        common: InitiatorCommon,
        obfuscation: AesObfuscationState,
        transcript: Transcript,
        router_info: Vec<u8>,
    },
    NeedRequestPadding {
        common: InitiatorCommon,
        obfuscation: AesObfuscationState,
        transcript: Transcript,
        router_info: Vec<u8>,
        timestamp: u64,
    },
    NeedConfirmedPadding {
        common: InitiatorCommon,
        obfuscation: AesObfuscationState,
        transcript: Transcript,
        router_info: Vec<u8>,
        timestamp: u64,
        request_padding: Vec<u8>,
    },
    AwaitCreated {
        common: InitiatorCommon,
        obfuscation: AesObfuscationState,
        transcript: Transcript,
        confirmed_plaintext: Vec<u8>,
        request_padding_length: usize,
        expected_part2_length: usize,
    },
    NeedPeerTimestamp {
        common: InitiatorCommon,
        transcript: Transcript,
        confirmed_plaintext: Vec<u8>,
        request_padding_length: usize,
        expected_part2_length: usize,
        created_padding_length: usize,
        created_timestamp: u32,
        replay_token: ReplayToken,
        responder_ephemeral: PublicKeyBytes,
    },
    NeedConfirmedReplay {
        common: InitiatorCommon,
        transcript: Transcript,
        confirmed_plaintext: Vec<u8>,
        request_padding_length: usize,
        expected_part2_length: usize,
        created_padding_length: usize,
        responder_ephemeral: PublicKeyBytes,
    },
    Done,
}

/// A consuming initiator handshake state.
pub struct InitiatorState {
    phase: InitiatorPhase,
}

impl InitiatorState {
    /// Creates an initiator with injected static and ephemeral X25519 keys.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        local_static: X25519PrivateKey,
        ephemeral: X25519PrivateKey,
        responder_static: PublicKeyBytes,
        expected_peer_hash: Option<Hash>,
        responder_router_hash: [u8; 32],
        obfuscation_iv: [u8; 16],
        network_id: u8,
        skew: ClockSkewPolicy,
    ) -> Result<Self, HandshakeError> {
        let transcript = Transcript::new(Role::Initiator, responder_static);
        Ok(Self {
            phase: InitiatorPhase::NeedRouterInfo {
                common: InitiatorCommon {
                    local_static,
                    ephemeral,
                    responder_static,
                    expected_peer_hash,
                    network_id,
                    skew,
                },
                obfuscation: AesObfuscationState::new(responder_router_hash, obfuscation_iv),
                transcript,
            },
        })
    }

    /// Emits the first action. The state is consumed even when called at the wrong phase.
    pub fn start(self) -> Result<HandshakeTransition<Self>, HandshakeError> {
        match self.phase {
            InitiatorPhase::NeedRouterInfo { .. } => Ok(transition(
                self,
                vec![HandshakeAction::RequestRouterInfo {
                    maximum: constants::MAX_ROUTER_INFO_PAYLOAD,
                }],
            )),
            _ => Err(HandshakeError::StateViolation),
        }
    }

    /// Consumes this state and applies exactly one runtime result.
    pub fn transition(
        self,
        input: HandshakeInput,
    ) -> Result<HandshakeTransition<Self>, HandshakeError> {
        let input = match input {
            HandshakeInput::Cancelled => {
                return Ok(transition(
                    Self {
                        phase: InitiatorPhase::Done,
                    },
                    vec![HandshakeAction::Terminate(HandshakeError::Cancelled)],
                ));
            }
            HandshakeInput::DeadlineExpired => {
                return Ok(transition(
                    Self {
                        phase: InitiatorPhase::Done,
                    },
                    vec![HandshakeAction::Terminate(HandshakeError::DeadlineExpired)],
                ));
            }
            HandshakeInput::Disconnected => {
                return Ok(transition(
                    Self {
                        phase: InitiatorPhase::Done,
                    },
                    vec![HandshakeAction::Terminate(HandshakeError::Disconnected)],
                ));
            }
            input => input,
        };
        let phase = self.phase;
        match (phase, input) {
            (
                InitiatorPhase::NeedRouterInfo {
                    common,
                    obfuscation,
                    transcript,
                },
                HandshakeInput::RouterInfo(router_info),
            ) => {
                let local_static = public_key(&common.local_static)?;
                validate_router_info(
                    &router_info,
                    constants::MAX_ROUTER_INFO_PAYLOAD,
                    None,
                    local_static,
                )?;
                Ok(transition(
                    Self {
                        phase: InitiatorPhase::NeedRequestTimestamp {
                            common,
                            obfuscation,
                            transcript,
                            router_info,
                        },
                    },
                    vec![HandshakeAction::RequestTimestamp {
                        purpose: TimestampPurpose::SessionRequest,
                    }],
                ))
            }
            (
                InitiatorPhase::NeedRequestTimestamp {
                    common,
                    obfuscation,
                    transcript,
                    router_info,
                },
                HandshakeInput::Timestamp(timestamp),
            ) => Ok(transition(
                Self {
                    phase: InitiatorPhase::NeedRequestPadding {
                        common,
                        obfuscation,
                        transcript,
                        router_info,
                        timestamp,
                    },
                },
                vec![HandshakeAction::RequestPadding {
                    message: PaddingMessage::SessionRequest,
                    maximum: constants::MAX_SESSION_REQUEST_PADDING,
                }],
            )),
            (
                InitiatorPhase::NeedRequestPadding {
                    common,
                    obfuscation,
                    transcript,
                    router_info,
                    timestamp,
                },
                HandshakeInput::Padding(request_padding),
            ) => {
                if request_padding.len() > constants::MAX_SESSION_REQUEST_PADDING {
                    return Err(HandshakeError::ExcessivePadding);
                }
                Ok(transition(
                    Self {
                        phase: InitiatorPhase::NeedConfirmedPadding {
                            common,
                            obfuscation,
                            transcript,
                            router_info,
                            timestamp,
                            request_padding,
                        },
                    },
                    vec![HandshakeAction::RequestPadding {
                        message: PaddingMessage::SessionConfirmed,
                        maximum: constants::MAX_SESSION_CONFIRMED_PART2_PLAINTEXT,
                    }],
                ))
            }
            (
                InitiatorPhase::NeedConfirmedPadding {
                    common,
                    mut obfuscation,
                    transcript,
                    router_info,
                    timestamp,
                    request_padding,
                },
                HandshakeInput::Padding(confirmed_padding),
            ) => {
                let payload = ConfirmedPayload::new(router_info, None, Some(confirmed_padding))?;
                let confirmed_plaintext = payload.encode();
                let expected_part2_length = confirmed_plaintext
                    .len()
                    .checked_add(constants::AUTH_TAG_LENGTH)
                    .ok_or(HandshakeError::InvalidFixedLength)?;
                if expected_part2_length > constants::MAX_SESSION_CONFIRMED_PART2 {
                    return Err(HandshakeError::InvalidFixedLength);
                }
                let options = SessionRequestOptions::new(
                    common.network_id,
                    request_padding.len(),
                    expected_part2_length,
                    timestamp,
                )?;
                let ephemeral = public_key(&common.ephemeral)?;
                let shared = common
                    .ephemeral
                    .diffie_hellman(common.responder_static.as_bytes())
                    .map_err(HandshakeError::from)?;
                let (transcript, encrypted_options) = transcript
                    .session_request(ephemeral, shared, &options.encode())
                    .map_err(map_crypto)?;
                let encrypted_ephemeral = obfuscation.encrypt(&ephemeral);
                let request = SessionRequest::new(
                    *encrypted_ephemeral.as_bytes(),
                    encrypted_options,
                    request_padding,
                )?;
                let request_bytes = HandshakeBytes::new(request.encode())?;
                let next = Self {
                    phase: InitiatorPhase::AwaitCreated {
                        common,
                        obfuscation,
                        transcript: transcript
                            .mix_padding(request.padding())
                            .map_err(map_crypto)?,
                        confirmed_plaintext,
                        request_padding_length: request.padding().len(),
                        expected_part2_length,
                    },
                };
                Ok(transition(
                    next,
                    vec![
                        HandshakeAction::Write(request_bytes),
                        HandshakeAction::ReadBounded {
                            minimum: constants::MIN_HANDSHAKE_MESSAGE_LENGTH,
                            maximum: constants::MAX_HANDSHAKE_MESSAGE_LENGTH,
                        },
                    ],
                ))
            }
            (
                InitiatorPhase::AwaitCreated {
                    common,
                    mut obfuscation,
                    transcript,
                    confirmed_plaintext,
                    request_padding_length,
                    expected_part2_length,
                },
                HandshakeInput::Bytes(bytes),
            ) => {
                let created =
                    SessionCreated::decode(&bytes, constants::MAX_HANDSHAKE_MESSAGE_LENGTH)?;
                let encrypted_ephemeral =
                    PublicKeyBytes::from_bytes_for_test(*created.encrypted_ephemeral());
                let responder_ephemeral = obfuscation
                    .decrypt(&encrypted_ephemeral)
                    .map_err(map_crypto)?;
                let shared = common
                    .ephemeral
                    .diffie_hellman(responder_ephemeral.as_bytes())
                    .map_err(HandshakeError::from)?;
                let (transcript, plaintext) = transcript
                    .accept_session_created(
                        responder_ephemeral,
                        shared,
                        created.encrypted_options(),
                    )
                    .map_err(map_crypto)?;
                let options = SessionCreatedOptions::decode(&plaintext)?;
                check_padding(created.padding().len(), usize::from(options.padding_length))?;
                let transcript = transcript
                    .mix_padding(created.padding())
                    .map_err(map_crypto)?;
                Ok(transition(
                    Self {
                        phase: InitiatorPhase::NeedPeerTimestamp {
                            common,
                            transcript,
                            confirmed_plaintext,
                            request_padding_length,
                            expected_part2_length,
                            created_padding_length: created.padding().len(),
                            created_timestamp: options.timestamp,
                            replay_token: ReplayToken::from_ephemeral_bytes(
                                created.encrypted_ephemeral(),
                            ),
                            responder_ephemeral,
                        },
                    },
                    vec![HandshakeAction::RequestTimestamp {
                        purpose: TimestampPurpose::PeerValidation,
                    }],
                ))
            }
            (
                InitiatorPhase::NeedPeerTimestamp {
                    common,
                    transcript,
                    confirmed_plaintext,
                    request_padding_length,
                    expected_part2_length,
                    created_padding_length,
                    created_timestamp,
                    replay_token,
                    responder_ephemeral,
                },
                HandshakeInput::Timestamp(local_timestamp),
            ) => {
                common.skew.classify(local_timestamp, created_timestamp)?;
                let retention = common.skew.replay_retention();
                Ok(transition(
                    Self {
                        phase: InitiatorPhase::NeedConfirmedReplay {
                            common,
                            transcript,
                            confirmed_plaintext,
                            request_padding_length,
                            expected_part2_length,
                            created_padding_length,
                            responder_ephemeral,
                        },
                    },
                    vec![HandshakeAction::RequestReplay {
                        token: replay_token,
                        retention,
                    }],
                ))
            }
            (
                InitiatorPhase::NeedConfirmedReplay {
                    common,
                    transcript,
                    confirmed_plaintext,
                    request_padding_length,
                    expected_part2_length,
                    created_padding_length,
                    responder_ephemeral,
                },
                HandshakeInput::Replay(decision),
            ) => {
                replay_result(decision)?;
                let local_static = public_key(&common.local_static)?;
                let se = common
                    .local_static
                    .diffie_hellman(responder_ephemeral.as_bytes())
                    .map_err(HandshakeError::from)?;
                let (transcript, static_frame) = transcript
                    .encrypt_static(local_static, se)
                    .map_err(map_crypto)?;
                let (transcript, payload_frame) = transcript
                    .encrypt_confirmed_payload(&confirmed_plaintext)
                    .map_err(map_crypto)?;
                if payload_frame.len() != expected_part2_length {
                    return Err(HandshakeError::InvalidFixedLength);
                }
                let confirmed = SessionConfirmed::new(static_frame, payload_frame)?;
                let peer_hash = common
                    .expected_peer_hash
                    .ok_or(HandshakeError::PeerIdentityMismatch)?;
                let split_keys = transcript.split().map_err(map_crypto)?;
                let result = AuthenticatedHandshake {
                    role: Role::Initiator,
                    peer: AuthenticatedPeer {
                        router_hash: peer_hash,
                        transport_static_key: common.responder_static,
                    },
                    negotiated: NegotiatedParameters {
                        network_id: common.network_id,
                        session_confirmed_part2_length: expected_part2_length,
                        session_request_padding_length: request_padding_length,
                        session_created_padding_length: created_padding_length,
                        session_confirmed_plaintext_length: confirmed_plaintext.len(),
                    },
                    split_keys,
                };
                Ok(transition(
                    Self {
                        phase: InitiatorPhase::Done,
                    },
                    vec![
                        HandshakeAction::Write(HandshakeBytes::new(confirmed.encode())?),
                        HandshakeAction::Authenticated(result),
                    ],
                ))
            }
            _ => Err(HandshakeError::StateViolation),
        }
    }
}

struct ResponderCommon {
    local_static: X25519PrivateKey,
    ephemeral: X25519PrivateKey,
    local_static_public: PublicKeyBytes,
    expected_peer_hash: Option<Hash>,
    network_id: u8,
    skew: ClockSkewPolicy,
}

enum ResponderPhase {
    NeedRequest {
        common: ResponderCommon,
        obfuscation: AesObfuscationState,
    },
    AwaitReplay {
        common: ResponderCommon,
        obfuscation: AesObfuscationState,
        request: SessionRequest,
    },
    NeedPeerTimestamp {
        common: ResponderCommon,
        obfuscation: AesObfuscationState,
        request_padding: Vec<u8>,
        request_options: SessionRequestOptions,
        initiator_ephemeral: PublicKeyBytes,
        transcript: Transcript,
    },
    NeedCreatedPadding {
        common: ResponderCommon,
        obfuscation: AesObfuscationState,
        request_padding: Vec<u8>,
        request_options: SessionRequestOptions,
        initiator_ephemeral: PublicKeyBytes,
        transcript: Transcript,
        timestamp: u64,
    },
    AwaitConfirmed {
        common: ResponderCommon,
        transcript: Transcript,
        expected_part2_length: usize,
        request_padding_length: usize,
        created_padding_length: usize,
        network_id: u8,
    },
    Done,
}

/// A consuming responder handshake state.
pub struct ResponderState {
    phase: ResponderPhase,
}

impl ResponderState {
    /// Creates a responder with injected static and ephemeral X25519 keys.
    pub fn new(
        local_static: X25519PrivateKey,
        ephemeral: X25519PrivateKey,
        expected_peer_hash: Option<Hash>,
        local_router_hash: [u8; 32],
        obfuscation_iv: [u8; 16],
        network_id: u8,
        skew: ClockSkewPolicy,
    ) -> Result<Self, HandshakeError> {
        let local_static_public = public_key(&local_static)?;
        Ok(Self {
            phase: ResponderPhase::NeedRequest {
                common: ResponderCommon {
                    local_static,
                    ephemeral,
                    local_static_public,
                    expected_peer_hash,
                    network_id,
                    skew,
                },
                obfuscation: AesObfuscationState::new(local_router_hash, obfuscation_iv),
            },
        })
    }

    /// Emits the first bounded read action.
    pub fn start(self) -> Result<HandshakeTransition<Self>, HandshakeError> {
        match self.phase {
            ResponderPhase::NeedRequest { .. } => Ok(transition(
                self,
                vec![HandshakeAction::ReadBounded {
                    minimum: constants::MIN_HANDSHAKE_MESSAGE_LENGTH,
                    maximum: constants::MAX_HANDSHAKE_MESSAGE_LENGTH,
                }],
            )),
            _ => Err(HandshakeError::StateViolation),
        }
    }

    /// Consumes this state and applies exactly one runtime result.
    pub fn transition(
        self,
        input: HandshakeInput,
    ) -> Result<HandshakeTransition<Self>, HandshakeError> {
        let input = match input {
            HandshakeInput::Cancelled => {
                return Ok(transition(
                    Self {
                        phase: ResponderPhase::Done,
                    },
                    vec![HandshakeAction::Terminate(HandshakeError::Cancelled)],
                ));
            }
            HandshakeInput::DeadlineExpired => {
                return Ok(transition(
                    Self {
                        phase: ResponderPhase::Done,
                    },
                    vec![HandshakeAction::Terminate(HandshakeError::DeadlineExpired)],
                ));
            }
            HandshakeInput::Disconnected => {
                return Ok(transition(
                    Self {
                        phase: ResponderPhase::Done,
                    },
                    vec![HandshakeAction::Terminate(HandshakeError::Disconnected)],
                ));
            }
            input => input,
        };
        let phase = self.phase;
        match (phase, input) {
            (
                ResponderPhase::NeedRequest {
                    common,
                    obfuscation,
                },
                HandshakeInput::Bytes(bytes),
            ) => {
                let request =
                    SessionRequest::decode(&bytes, constants::MAX_HANDSHAKE_MESSAGE_LENGTH)?;
                let token = ReplayToken::from_ephemeral_bytes(request.encrypted_ephemeral());
                let retention = common.skew.replay_retention();
                Ok(transition(
                    Self {
                        phase: ResponderPhase::AwaitReplay {
                            common,
                            obfuscation,
                            request,
                        },
                    },
                    vec![HandshakeAction::RequestReplay { token, retention }],
                ))
            }
            (
                ResponderPhase::AwaitReplay {
                    common,
                    mut obfuscation,
                    request,
                },
                HandshakeInput::Replay(decision),
            ) => {
                replay_result(decision)?;
                let encrypted_ephemeral =
                    PublicKeyBytes::from_bytes_for_test(*request.encrypted_ephemeral());
                let initiator_ephemeral = obfuscation
                    .decrypt(&encrypted_ephemeral)
                    .map_err(map_crypto)?;
                let shared = common
                    .local_static
                    .diffie_hellman(initiator_ephemeral.as_bytes())
                    .map_err(HandshakeError::from)?;
                let transcript = Transcript::new(Role::Responder, common.local_static_public);
                let (transcript, plaintext) = transcript
                    .accept_session_request(
                        initiator_ephemeral,
                        shared,
                        request.encrypted_options(),
                    )
                    .map_err(map_crypto)?;
                let options = SessionRequestOptions::decode(&plaintext)?;
                if options.network_id != common.network_id {
                    return Err(HandshakeError::WrongNetwork);
                }
                check_padding(request.padding().len(), usize::from(options.padding_length))?;
                let transcript = transcript
                    .mix_padding(request.padding())
                    .map_err(map_crypto)?;
                Ok(transition(
                    Self {
                        phase: ResponderPhase::NeedPeerTimestamp {
                            common,
                            obfuscation,
                            request_padding: request.padding().to_vec(),
                            request_options: options,
                            initiator_ephemeral,
                            transcript,
                        },
                    },
                    vec![HandshakeAction::RequestTimestamp {
                        purpose: TimestampPurpose::PeerValidation,
                    }],
                ))
            }
            (
                ResponderPhase::NeedPeerTimestamp {
                    common,
                    obfuscation,
                    request_padding,
                    request_options,
                    initiator_ephemeral,
                    transcript,
                },
                HandshakeInput::Timestamp(local_timestamp),
            ) => {
                common
                    .skew
                    .classify(local_timestamp, request_options.timestamp)?;
                Ok(transition(
                    Self {
                        phase: ResponderPhase::NeedCreatedPadding {
                            common,
                            obfuscation,
                            request_padding,
                            request_options,
                            initiator_ephemeral,
                            transcript,
                            timestamp: local_timestamp,
                        },
                    },
                    vec![HandshakeAction::RequestPadding {
                        message: PaddingMessage::SessionCreated,
                        maximum: constants::MAX_SESSION_CREATED_PADDING,
                    }],
                ))
            }
            (
                ResponderPhase::NeedCreatedPadding {
                    common,
                    mut obfuscation,
                    request_padding,
                    request_options,
                    initiator_ephemeral,
                    transcript,
                    timestamp,
                },
                HandshakeInput::Padding(created_padding),
            ) => {
                if created_padding.len() > constants::MAX_SESSION_CREATED_PADDING {
                    return Err(HandshakeError::ExcessivePadding);
                }
                let responder_ephemeral = public_key(&common.ephemeral)?;
                let shared = common
                    .ephemeral
                    .diffie_hellman(initiator_ephemeral.as_bytes())
                    .map_err(HandshakeError::from)?;
                let options = SessionCreatedOptions::new(created_padding.len(), timestamp)?;
                let (transcript, encrypted_options) = transcript
                    .session_created(responder_ephemeral, shared, &options.encode())
                    .map_err(map_crypto)?;
                let transcript = transcript
                    .mix_padding(&created_padding)
                    .map_err(map_crypto)?;
                let encrypted_ephemeral = obfuscation.encrypt(&responder_ephemeral);
                let created = SessionCreated::new(
                    *encrypted_ephemeral.as_bytes(),
                    encrypted_options,
                    created_padding,
                )?;
                let expected_part2_length =
                    usize::from(request_options.session_confirmed_part2_length);
                let created_bytes = HandshakeBytes::new(created.encode())?;
                Ok(transition(
                    Self {
                        phase: ResponderPhase::AwaitConfirmed {
                            common,
                            transcript,
                            expected_part2_length,
                            request_padding_length: request_padding.len(),
                            created_padding_length: created.padding().len(),
                            network_id: request_options.network_id,
                        },
                    },
                    vec![
                        HandshakeAction::Write(created_bytes),
                        HandshakeAction::ReadExact {
                            length: constants::SESSION_CONFIRMED_PART1_LENGTH
                                .checked_add(expected_part2_length)
                                .ok_or(HandshakeError::InvalidFixedLength)?,
                        },
                    ],
                ))
            }
            (
                ResponderPhase::AwaitConfirmed {
                    common,
                    transcript,
                    expected_part2_length,
                    request_padding_length,
                    created_padding_length,
                    network_id,
                },
                HandshakeInput::Bytes(bytes),
            ) => {
                let confirmed = SessionConfirmed::decode(
                    &bytes,
                    expected_part2_length,
                    constants::MAX_SESSION_CONFIRMED_LENGTH,
                )?;
                let (transcript, initiator_static) = transcript
                    .decrypt_static_unchecked(confirmed.static_frame())
                    .map_err(map_crypto)?;
                let se = common
                    .ephemeral
                    .diffie_hellman(initiator_static.as_bytes())
                    .map_err(HandshakeError::from)?;
                let transcript = transcript.mix_static_secret(se).map_err(map_crypto)?;
                let (transcript, plaintext) = transcript
                    .decrypt_confirmed_payload(confirmed.payload_frame())
                    .map_err(map_crypto)?;
                let payload =
                    ConfirmedPayload::decode(&plaintext, constants::MAX_ROUTER_INFO_PAYLOAD)?;
                let peer = validate_router_info(
                    payload.router_info(),
                    constants::MAX_ROUTER_INFO_PAYLOAD,
                    common.expected_peer_hash,
                    initiator_static,
                )?;
                let split_keys = transcript.split().map_err(map_crypto)?;
                let result = AuthenticatedHandshake {
                    role: Role::Responder,
                    peer,
                    negotiated: NegotiatedParameters {
                        network_id,
                        session_confirmed_part2_length: expected_part2_length,
                        session_request_padding_length: request_padding_length,
                        session_created_padding_length: created_padding_length,
                        session_confirmed_plaintext_length: plaintext.len(),
                    },
                    split_keys,
                };
                Ok(transition(
                    Self {
                        phase: ResponderPhase::Done,
                    },
                    vec![HandshakeAction::Authenticated(result)],
                ))
            }
            _ => Err(HandshakeError::StateViolation),
        }
    }
}
